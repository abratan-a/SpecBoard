//! A card's document and MCP's shape of it, carried both ways by the kind's definition.
//!
//! On the wire a card is flat: `kind`, then each field by name and each kind of row by
//! its name, a row being its fields and the kinds of row under it, by name — so the
//! built-in kinds read and write with the keys MCP has always had (`fields`, `values`,
//! `nodes`, `rows`, `body`, …). In the document the fields and rows sit apart and every
//! row has an id that does not move, so a merge and the history can find it again.
//!
//! References held by a card's id (`store: id`, and cards put inside one) are paths on
//! the wire: `Names` turns one into the other.

use super::check::Problem;
use super::{Address, Child, Field, FieldType, Kind, Row};
use serde_json::{Map, Value};
use std::collections::HashSet;

/// Paths for ids and ids for paths, as the project names its cards now.
pub trait Names {
    fn path(&self, id: &str) -> Option<String>;
    fn id(&self, path: &str) -> Option<String>;
    /// A path that names no card because the call writing it was to make that card and could not:
    /// that card, by its full path. None for any other.
    fn unmade(&self, _path: &str) -> Option<String> {
        None
    }
    /// Why a path names no card, when there is more to say than that (the names that look like
    /// it); none for none.
    fn why_not(&self, _path: &str) -> Option<String> {
        None
    }
    /// A section's full path, by its id; none when there is no such section.
    fn section_path(&self, _id: &str) -> Option<String> {
        None
    }
    /// The id of the section a path names, read from the card written; none for none.
    fn section_id(&self, _path: &str) -> Option<String> {
        None
    }
    /// A path that goes on past a working directory's file into what is in it
    /// (`repo/src:'a.rs'.Order.total`): the file's id and the path inside it; none for any other.
    fn inside_file(&self, _path: &str) -> Option<(String, String)> {
        None
    }
}

/// How a path that names no card is refused: as a card made in the same call and refused there,
/// when it is one — the refusal is that card's, and goes once it is put right (otherwise
/// cards that pointed at one card that was refused would be refused for it).
pub fn no_card(names: &dyn Names, path: &str) -> crate::text::Keyed {
    match (names.unmade(path), names.why_not(path)) {
        (Some(card), _) => keyed!("kindConvert.problem.noCardUnmade", path, card),
        (None, Some(why)) => keyed!("kindConvert.problem.noCardWhy", path, why),
        (None, None) => keyed!("kindConvert.problem.noCard", path),
    }
}

/// A card's parts read from the wire: what goes into the document, and how it looks
/// (fields marked `look`), which is kept outside it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Parts {
    pub fields: Map<String, Value>,
    pub rows: Map<String, Value>,
    pub looks: Map<String, Value>,
}

// MARK: - Document to wire

/// MCP's shape of a card's document: `kind`, its fields in the definition's order, then
/// its rows. How it looks is not in the document, so not here either.
pub fn to_wire(kind: &Kind, doc: &Value, names: &dyn Names) -> Value {
    let mut out = Map::new();
    out.insert("kind".to_string(), Value::String(kind.name.clone()));
    let empty = Map::new();
    let fields = doc
        .get("fields")
        .and_then(Value::as_object)
        .unwrap_or(&empty);
    fields_out(&kind.fields, fields, names, &mut out);
    let rows = doc.get("rows").and_then(Value::as_object).unwrap_or(&empty);
    for row in &kind.rows {
        if let Some(items) = rows.get(&row.name).and_then(Value::as_array) {
            let lineage = [row];
            out.insert(
                row.name.clone(),
                Value::Array(
                    items
                        .iter()
                        .map(|i| row_out(row, i, names, &lineage))
                        .collect(),
                ),
            );
        }
    }
    Value::Object(out)
}

fn fields_out(
    defs: &[Field],
    values: &Map<String, Value>,
    names: &dyn Names,
    out: &mut Map<String, Value>,
) {
    for def in defs {
        if let Some(value) = values.get(&def.name) {
            out.insert(def.name.clone(), value_out(def, value, names));
        }
    }
}

fn row_out(row: &Row, item: &Value, names: &dyn Names, lineage: &[&Row]) -> Value {
    let mut out = Map::new();
    let empty = Map::new();
    let fields = item
        .get("fields")
        .and_then(Value::as_object)
        .unwrap_or(&empty);
    fields_out(&row.fields, fields, names, &mut out);
    let children = item
        .get("rows")
        .and_then(Value::as_object)
        .unwrap_or(&empty);
    for child in &row.children {
        let Some(items) = children.get(child.name()).and_then(Value::as_array) else {
            continue;
        };
        let Some(inner) = resolve(child, lineage) else {
            continue;
        };
        let mut deeper = lineage.to_vec();
        if matches!(child, Child::Row(_)) {
            deeper.push(inner);
        }
        out.insert(
            child.name().to_string(),
            Value::Array(
                items
                    .iter()
                    .map(|i| row_out(inner, i, names, &deeper))
                    .collect(),
            ),
        );
    }
    Value::Object(out)
}

fn value_out(def: &Field, value: &Value, names: &dyn Names) -> Value {
    match (def.many, value) {
        (0, _) => one_out(def, value, names),
        (_, Value::Array(items)) => Value::Array(
            items
                .iter()
                .map(|item| {
                    if def.many == 2 {
                        value_out_inner(def, item, names)
                    } else {
                        one_out(def, item, names)
                    }
                })
                .collect(),
        ),
        _ => value.clone(),
    }
}

fn value_out_inner(def: &Field, value: &Value, names: &dyn Names) -> Value {
    match value {
        Value::Array(items) => Value::Array(items.iter().map(|i| one_out(def, i, names)).collect()),
        other => other.clone(),
    }
}

fn one_out(def: &Field, value: &Value, names: &dyn Names) -> Value {
    match def.fieldtype {
        FieldType::Card => path_of(value, names),
        FieldType::Path if def.by_id => path_of(value, names),
        FieldType::Section => match value.as_str().and_then(|id| names.section_path(id)) {
            Some(path) => Value::String(path),
            None => value.clone(),
        },
        FieldType::Group => match value.as_object() {
            Some(inner) => {
                let mut out = Map::new();
                fields_out(&def.fields, inner, names, &mut out);
                Value::Object(out)
            }
            None => value.clone(),
        },
        _ => value.clone(),
    }
}

/// An id as its path now. One that leads nowhere is given back as it is.
fn path_of(value: &Value, names: &dyn Names) -> Value {
    match value.as_str().and_then(|id| names.path(id)) {
        Some(path) => Value::String(path),
        None => value.clone(),
    }
}

/// The kind of row a child stands for: its own, or an ancestor's again.
fn resolve<'a>(child: &'a Child, lineage: &[&'a Row]) -> Option<&'a Row> {
    match child {
        Child::Row(row) => Some(row),
        Child::Again { name, .. } => lineage.iter().rev().find(|r| &r.name == name).copied(),
    }
}

// MARK: - Wire to document

/// The parts of a card's document read from MCP's shape of it. `prior` is the document
/// as it is (if the card is there already): a row the wire names the same way keeps its
/// id, a new row is given one by `new_id`. The parts are not checked against the kind
/// here; that is `check_card`'s, on the whole document they make.
pub fn from_wire(
    kind: &Kind,
    wire: &Map<String, Value>,
    prior: Option<&Value>,
    names: &dyn Names,
    new_id: &mut dyn FnMut() -> String,
) -> Result<Parts, Vec<Problem>> {
    let mut reading = Reading {
        names,
        new_id,
        problems: Vec::new(),
    };
    let mut parts = Parts::default();
    for (key, value) in wire {
        if key == "kind" {
            if value.as_str() != Some(kind.name.as_str()) {
                reading.problems.push(Problem::new(
                    "kind",
                    keyed!("kindConvert.problem.kindIs", value, kind = kind.name),
                ));
            }
            continue;
        }
        if let Some(def) = kind.field(key) {
            let read = reading.value(def, value, key);
            if def.look {
                parts.looks.insert(key.clone(), read);
            } else {
                parts.fields.insert(key.clone(), read);
            }
        } else if let Some(row) = kind.row(key) {
            let before = prior
                .and_then(|p| p.get("rows"))
                .and_then(|r| r.get(key))
                .and_then(Value::as_array);
            let lineage = [row];
            let items = reading.rows(row, value, before, key, &lineage);
            parts.rows.insert(key.clone(), items);
        } else {
            reading.problems.push(Problem::new(
                key,
                keyed!("kindConvert.problem.strayKey", kind = kind.name),
            ));
        }
    }
    if reading.problems.is_empty() {
        Ok(parts)
    } else {
        Err(reading.problems)
    }
}

struct Reading<'n> {
    names: &'n dyn Names,
    new_id: &'n mut dyn FnMut() -> String,
    problems: Vec<Problem>,
}

impl Reading<'_> {
    fn rows(
        &mut self,
        row: &Row,
        value: &Value,
        before: Option<&Vec<Value>>,
        at: &str,
        lineage: &[&Row],
    ) -> Value {
        let Some(items) = value.as_array() else {
            self.problems
                .push(Problem::new(at, keyed!("kindConvert.problem.rowsList")));
            return Value::Array(Vec::new());
        };
        let mut taken = HashSet::new();
        let mut out = Vec::new();
        for (index, item) in items.iter().enumerate() {
            let here = format!("{at}[{index}]");
            let Some(item) = item.as_object() else {
                self.problems
                    .push(Problem::new(&here, keyed!("kindConvert.problem.rowObject")));
                continue;
            };
            let earlier = before.and_then(|b| same_row(row, item, index, b, &mut taken));
            let id = earlier
                .and_then(|e| e.get("id"))
                .and_then(Value::as_str)
                .map(str::to_string)
                .unwrap_or_else(|| (self.new_id)());
            let mut fields = Map::new();
            let mut children = Map::new();
            for (key, value) in item {
                if let Some(def) = row.fields.iter().find(|f| &f.name == key) {
                    fields.insert(
                        key.clone(),
                        self.value(def, value, &format!("{here}.{key}")),
                    );
                } else if let Some(child) = row.children.iter().find(|c| c.name() == key) {
                    let Some(inner) = resolve(child, lineage) else {
                        self.problems.push(Problem::new(
                            &here,
                            keyed!("kindConvert.problem.rowNotPlaceable", key),
                        ));
                        continue;
                    };
                    let mut deeper = lineage.to_vec();
                    if matches!(child, Child::Row(_)) {
                        deeper.push(inner);
                    }
                    let before = earlier
                        .and_then(|e| e.get("rows"))
                        .and_then(|r| r.get(key))
                        .and_then(Value::as_array);
                    children.insert(
                        key.clone(),
                        self.rows(inner, value, before, &format!("{here}.{key}"), &deeper),
                    );
                } else {
                    self.problems.push(Problem::new(
                        &here,
                        keyed!("kindConvert.problem.rowKey", key, label = row.label),
                    ));
                }
            }
            let mut written = Map::new();
            written.insert("id".to_string(), Value::String(id));
            if !fields.is_empty() {
                written.insert("fields".to_string(), Value::Object(fields));
            }
            if !children.is_empty() {
                written.insert("rows".to_string(), Value::Object(children));
            }
            out.push(Value::Object(written));
        }
        Value::Array(out)
    }

    /// A field's value from the wire. A list takes one value written alone.
    fn value(&mut self, def: &Field, value: &Value, at: &str) -> Value {
        match def.many {
            0 => self.one(def, value, at),
            1 => {
                let items = match value {
                    Value::Array(items) => items.clone(),
                    one => vec![one.clone()],
                };
                Value::Array(
                    items
                        .iter()
                        .enumerate()
                        .map(|(i, v)| self.one(def, v, &format!("{at}[{i}]")))
                        .collect(),
                )
            }
            _ => match value.as_array() {
                Some(items) => Value::Array(
                    items
                        .iter()
                        .enumerate()
                        .map(|(i, inner)| match inner.as_array() {
                            Some(cells) => Value::Array(
                                cells
                                    .iter()
                                    .enumerate()
                                    .map(|(j, v)| self.one(def, v, &format!("{at}[{i}][{j}]")))
                                    .collect(),
                            ),
                            None => inner.clone(),
                        })
                        .collect(),
                ),
                None => value.clone(),
            },
        }
    }

    fn one(&mut self, def: &Field, value: &Value, at: &str) -> Value {
        match def.fieldtype {
            FieldType::Card => self.id_of(value, at),
            FieldType::Path if def.by_id => self.id_of(value, at),
            FieldType::Section => self.section_of(value, at),
            FieldType::Group => match value.as_object() {
                Some(inner) => {
                    let mut out = Map::new();
                    for (key, v) in inner {
                        match def.fields.iter().find(|f| &f.name == key) {
                            Some(field) => {
                                out.insert(
                                    key.clone(),
                                    self.value(field, v, &format!("{at}.{key}")),
                                );
                            }
                            None => self.problems.push(Problem::new(
                                at,
                                keyed!("kindConvert.problem.groupKey", key, label = def.label),
                            )),
                        }
                    }
                    Value::Object(out)
                }
                None => value.clone(),
            },
            _ => value.clone(),
        }
    }

    /// A section's path as its id. One that names no section is refused, as a card's is.
    fn section_of(&mut self, value: &Value, at: &str) -> Value {
        let Some(path) = value.as_str() else {
            return value.clone();
        };
        match self.names.section_id(path) {
            Some(id) => Value::String(id),
            None => {
                self.problems.push(Problem::new(
                    at,
                    keyed!("kindConvert.problem.noSection", path),
                ));
                value.clone()
            }
        }
    }

    /// A path as the id of the card it names. One that names no card is refused: a
    /// reference held by id must have something to hold.
    fn id_of(&mut self, value: &Value, at: &str) -> Value {
        let Some(path) = value.as_str() else {
            return value.clone();
        };
        match self.names.id(path) {
            Some(id) => Value::String(id),
            None => {
                self.problems
                    .push(Problem::new(at, no_card(self.names, path)));
                value.clone()
            }
        }
    }
}

/// The row as it was that the wire's row names: the same name or value, or the same
/// place for rows named by place. Each earlier row is matched once.
fn same_row<'b>(
    row: &Row,
    item: &Map<String, Value>,
    index: usize,
    before: &'b [Value],
    taken: &mut HashSet<usize>,
) -> Option<&'b Value> {
    let key = match row.address {
        Address::Name => "name",
        Address::Value => "value",
        Address::Index => {
            return before.get(index).filter(|_| taken.insert(index));
        }
    };
    let wanted = item.get(key)?;
    let found = before.iter().enumerate().find(|(i, earlier)| {
        !taken.contains(i) && earlier.get("fields").and_then(|f| f.get(key)) == Some(wanted)
    })?;
    taken.insert(found.0);
    Some(found.1)
}
