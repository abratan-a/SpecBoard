//! A merge from one tree into another, main among them (`specboard://SpecBoard/Docs/Design:DesignBranches` §6;
//! `Repository.mergePreview`・`merge` and `Merge3` in the Swift app): what the source
//! changed, compared unit by unit with what the target holds, from what both last had in
//! common, and written into the target the way the target keeps it.
//!
//! A card is taken apart into its units: each key of its document, each of its fields'
//! values (`fields`; a data card's `data` by its top-level fields, an array whose elements' shape
//! has a key an element at a time, where the array stands among them worked out apart), each row
//! of a record's fields, of an enum's values, of a stackdoc's
//! versions and of a formatdoc's rows by id (its parent is part of it; where it stands among
//! its siblings is worked out apart), and every other kind of row whole — a flow's steps,
//! whose parts lead to one another, and a table a project's own kind defines: mixing them
//! would make shapes neither side wrote. A section is taken apart into its keys. A unit only
//! the source changed takes the source's; only the target changed, the target's; both alike,
//! as it is; both apart, it is a conflict, settled by what `take` says or left for it to say.
//! A row one side removed while the other changed it or what it holds is one conflict, of
//! that row: taken either way whole, so nothing inside is dropped unsaid. Two things the merge
//! would leave at one path refuse it: a card or a section, and a row a card keeps by id — a
//! field under one parent, an enum's value, a stackdoc's version, a formatdoc's row under one
//! parent — made with one name on both sides.

use super::{Table, Tx, db};
use rusqlite::{Connection, params};
use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};

/// Which side a conflict takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Source,
    Target,
}

impl Side {
    pub fn named(word: &str) -> Option<Side> {
        match word {
            "source" => Some(Side::Source),
            "target" => Some(Side::Target),
            _ => None,
        }
    }
}

/// Choices for conflicts: by the card's or section's path, by unit (as a person names it,
/// or as it is kept apart).
pub type Take = HashMap<String, HashMap<String, Side>>;

/// What a merge does to one card or section of the target.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Change {
    Add,
    Change,
    Remove,
}

#[derive(Clone, Debug)]
pub struct Item {
    /// The card's or the section's path, as the target names it.
    pub path: String,
    pub section: bool,
    pub change: Change,
}

/// A unit both sides changed apart, left for `take`.
#[derive(Clone, Debug)]
pub struct Conflict {
    pub path: String,
    pub section: bool,
    /// The card's kind, for saying the unit in MCP's shape; none for a section.
    pub kind: Option<String>,
    /// The unit as it is kept apart: a key, `f:<key>` for a field's value,
    /// `r:<row>:<id>` for a row, `rows.<row>` for rows taken whole, `(whole)` for one
    /// side removing what the other changed.
    pub unit: String,
    /// The unit as a person names it, one of the card's alone: `label`, `fields.total`,
    /// `values.paid`, `versions.v0_2`, `rows.auth.login`, `steps`, `(whole)`.
    pub label: String,
    pub base: Option<Value>,
    pub source: Option<Value>,
    pub target: Option<Value>,
}

/// What merging one tree into another would do.
#[derive(Clone, Debug, Default)]
pub struct Preview {
    /// The source and the target by name; `main` for main.
    pub source: String,
    pub target: String,
    pub items: Vec<Item>,
    pub conflicts: Vec<Conflict>,
    /// Paths the target would hold two different things at: refused until one is renamed.
    pub overlaps: Vec<String>,
    /// How many more boxes the source moved would change the target, were they taken along
    /// (`places`: what the source placed, over where the target places it), than go in
    /// without — a merged card's the target places nowhere goes in either way, and is not
    /// counted. None from main.
    pub places: usize,
}

impl Preview {
    /// Nothing of what the cards and sections hold goes in; the boxes are apart (`places`).
    pub fn is_empty(&self) -> bool {
        self.items.is_empty() && self.conflicts.is_empty() && self.overlaps.is_empty()
    }

    fn count(&self, change: Change) -> usize {
        self.items.iter().filter(|i| i.change == change).count()
    }

    /// `足す n・直す n・消す n`, kept to be said in the language of whoever reads it.
    pub fn counts_keyed(&self) -> crate::text::Keyed {
        keyed!(
            "write.merge.counts",
            add = self.count(Change::Add),
            change = self.count(Change::Change),
            remove = self.count(Change::Remove)
        )
    }

    /// `足す n・直す n・消す n`, in the language of the call.
    pub fn counts(&self) -> String {
        self.counts_keyed().render()
    }
}

/// Why the boxes are not taken along from main: a merge from main takes main's changes into
/// a branch, and the branch keeps where it put its own.
pub fn places_from_main() -> String {
    text!("write.merge.placesFromMain")
}

/// The unit and the name of a conflict of a whole card or section (one side removed what the other
/// changed): the same in every language, as a `take` names it.
pub const WHOLE: &str = "(whole)";

/// Whether a unit a `take` names is the whole card or section: `WHOLE`, or what a request kept
/// before named it — the whole's name in a language, a card's or a section's.
pub fn is_whole(unit: &str) -> bool {
    unit == WHOLE
        || crate::text::LANGUAGES.iter().any(|lang| {
            [
                keyed!("write.merge.wholeCard"),
                keyed!("write.merge.wholeSection"),
            ]
            .iter()
            .any(|said| said.render_in(lang) == unit)
        })
}

// MARK: - Three versions

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Card,
    Section,
}

/// The tables of a card that may be kept apart row by row.
const BY_ROW: [&str; 4] = ["fields", "values", "versions", "rows"];
const PARENT: &str = "~parent";

/// A data card's top-level arrays whose elements' shape has a key: by the array's field, its key
/// fields' names, in the shape's order.
pub(crate) type DataKeys = HashMap<String, Vec<String>>;

/// The units a data card's `data` is kept apart in start so: `d:` that it is there, `d:<field>` a
/// top-level field's value, `d:<field>[]` that a keyed array is there, `d:<field>[<key>]` an element
/// of it (`d:tests[name=helper]`).
const DATA: &str = "d:";

/// An element's key as a path picks it (`[name=helper]`): none when a key field has no text,
/// number or truth.
fn element_key(keys: &[String], element: &Value) -> Option<String> {
    use crate::tools::picks::{Lit, Pick};
    keys.iter()
        .map(|k| Some((k.clone(), Lit::of(element.get(k)?)?)))
        .collect::<Option<Vec<_>>>()
        .map(|picked| Pick::Where(picked).written())
}

/// A version's `data`, when it has one as an object.
fn data_of(doc: Option<&Map<String, Value>>) -> Option<&Map<String, Value>> {
    doc?.get("fields")?.get("data")?.as_object()
}

/// The keyed arrays a merge takes an element at a time: those whose every element in every version
/// has its whole key, and no two the same. One that does not is taken whole, as any other field.
fn data_split(keys: &DataKeys, versions: &[Option<&Map<String, Value>>]) -> DataKeys {
    keys.iter()
        .filter(|(field, names)| {
            versions
                .iter()
                .filter_map(|v| data_of(*v))
                .all(|data| match data.get(*field) {
                    None => true,
                    Some(Value::Array(items)) => {
                        let mut seen = HashSet::new();
                        items
                            .iter()
                            .all(|i| element_key(names, i).is_some_and(|k| seen.insert(k)))
                    }
                    Some(_) => false,
                })
        })
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}

/// A data card's keyed arrays (`DataKeys`), read from its shape's document and those of the shapes
/// its arrays' elements are (`doc_of`, a card's document by its id): each top-level array of an
/// embedded shape, or of an object, with primary key fields. None for a card that is not data or
/// whose shape is not there; empty for one whose arrays have no key.
fn data_keys(
    card: &Map<String, Value>,
    doc_of: &dyn Fn(&str) -> Option<Map<String, Value>>,
) -> Option<DataKeys> {
    if card.get("kind").and_then(Value::as_str) != Some(crate::kinds::INSTANCE) {
        return None;
    }
    let shape_id = card
        .get("connections")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|l| l.get("line").and_then(Value::as_str) == Some(crate::kinds::OF_LINE))
        .and_then(|l| l.get("to").and_then(Value::as_str))?;
    let shape = doc_of(shape_id)?;
    let primary = |rows: &[Value]| -> Vec<String> {
        rows.iter()
            .filter(|r| r["fields"]["primaryKey"].as_bool() == Some(true))
            .filter_map(|r| r["fields"]["name"].as_str().map(str::to_string))
            .collect()
    };
    let mut out = DataKeys::new();
    for row in rows_of(&shape, "fields") {
        let fields = &row["fields"];
        let Some(name) = fields["name"].as_str() else {
            continue;
        };
        if fields["array"].as_bool() != Some(true) {
            continue;
        }
        let embedded = shape
            .get("connections")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .find(|l| {
                l.get("line").and_then(Value::as_str) == Some("embed")
                    && l.get("from").and_then(Value::as_str) == Some(name)
            })
            .and_then(|l| l.get("to").and_then(Value::as_str))
            .and_then(doc_of);
        let keys = match embedded {
            Some(element) => primary(rows_of(&element, "fields")),
            None => primary(
                row.get("rows")
                    .and_then(|r| r.get("fields"))
                    .and_then(Value::as_array)
                    .map(Vec::as_slice)
                    .unwrap_or(&[]),
            ),
        };
        if !keys.is_empty() {
            out.insert(name.to_string(), keys);
        }
    }
    Some(out)
}

/// The keys of a version's keyed array, in its order.
fn element_sequence(doc: Option<&Map<String, Value>>, field: &str, keys: &[String]) -> Vec<String> {
    data_of(doc)
        .and_then(|d| d.get(field))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|i| element_key(keys, i))
        .collect()
}

/// The field of the top of `data` a data unit is of (`d:tests[name=x]` → `tests`).
fn data_field(unit: &str) -> Option<&str> {
    let rest = unit.strip_prefix(DATA)?;
    let name = rest.split('[').next().unwrap_or(rest);
    (!name.is_empty()).then_some(name)
}

/// A data card's `data` put back together from its units: its top-level fields in the order the
/// versions had them, a keyed array's elements in the order of the side that moved them.
fn data_assembled(
    units: &HashMap<String, Value>,
    split: &DataKeys,
    versions: [Option<&Map<String, Value>>; 3],
) -> Option<Value> {
    if !units.contains_key(DATA) {
        return None;
    }
    let [ancestor, source, target] = versions;
    let mut names: Vec<String> = Vec::new();
    for version in [target, source] {
        for k in data_of(version).into_iter().flat_map(|d| d.keys()) {
            if !names.contains(k) {
                names.push(k.clone());
            }
        }
    }
    let mut extra: Vec<String> = units
        .keys()
        .filter_map(|u| data_field(u))
        .filter(|n| !names.iter().any(|m| m == n))
        .map(str::to_string)
        .collect();
    extra.sort();
    extra.dedup();
    names.extend(extra);
    let mut out = Map::new();
    for name in names {
        if let Some(v) = units.get(&format!("{DATA}{name}")) {
            out.insert(name, v.clone());
            continue;
        }
        let Some(keys) = split.get(&name) else {
            continue;
        };
        let prefix = format!("{DATA}{name}[");
        let marker = format!("{DATA}{name}[]");
        let held: Vec<String> = units
            .keys()
            .filter(|u| u.starts_with(&prefix) && **u != marker)
            .map(|u| u[prefix.len() - 1..].to_string())
            .collect();
        if held.is_empty() && !units.contains_key(&marker) {
            continue;
        }
        let order = ordered(
            &held,
            &element_sequence(ancestor, &name, keys),
            &element_sequence(source, &name, keys),
            &element_sequence(target, &name, keys),
        );
        let items: Vec<Value> = order
            .iter()
            .filter_map(|k| units.get(&format!("{DATA}{name}{k}")).cloned())
            .collect();
        out.insert(name, Value::Array(items));
    }
    Some(Value::Object(out))
}

/// Whether a card's rows of `name` are kept apart row by row — a record's fields, an enum's
/// values, a stackdoc's versions, a formatdoc's rows (each its own: one added on one side and
/// another changed on the other both go in). A formatdoc's rows are by row only in a
/// formatdoc: a table a project's own kind names `rows` is its own, and one unit.
fn by_row(card: Option<&str>, name: &str) -> bool {
    match name {
        "rows" => card == Some("formatdoc"),
        _ => BY_ROW.contains(&name),
    }
}

/// The key a row is named by among its siblings.
fn name_key(_: &str) -> &'static str {
    "name"
}

/// A card's kind, as the first of its versions that has one says it.
fn card_kind<'d>(versions: &[Option<&'d Map<String, Value>>]) -> Option<&'d str> {
    versions
        .iter()
        .flatten()
        .find_map(|d| d.get("kind").and_then(Value::as_str))
}

/// A value as one string: its keys in a fixed order, so the same content compares equal
/// however it was written.
fn canonical(value: &Value) -> String {
    fn sorted(value: &Value) -> Value {
        match value {
            Value::Object(o) => {
                let mut keys: Vec<&String> = o.keys().collect();
                keys.sort();
                Value::Object(
                    keys.into_iter()
                        .map(|k| (k.clone(), sorted(&o[k])))
                        .collect(),
                )
            }
            Value::Array(a) => Value::Array(a.iter().map(sorted).collect()),
            other => other.clone(),
        }
    }
    sorted(value).to_string()
}

fn text(doc: &Map<String, Value>) -> String {
    canonical(&Value::Object(doc.clone()))
}

fn rows_of<'d>(doc: &'d Map<String, Value>, name: &str) -> &'d [Value] {
    doc.get("rows")
        .and_then(|r| r.get(name))
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

/// The rows a card keeps by id that share a name with a sibling — a record's fields under one
/// parent (by name), an enum's values (by value), a stackdoc's versions (by name), a
/// formatdoc's rows under one parent (by name) — each said by its path in the card (`memo`,
/// `address.street`, `high`, `v0_2_34`, `auth.login`) with the ids at it.
fn rows_at_one_name(doc: &Map<String, Value>) -> Vec<(String, Vec<String>)> {
    fn gather(list: &[Value], name: &str, prefix: &str, out: &mut Vec<(String, Vec<String>)>) {
        let key = name_key(name);
        let mut at: Vec<(String, Vec<String>)> = Vec::new();
        for row in list {
            let (Some(id), Some(said)) = (
                row.get("id").and_then(Value::as_str),
                row["fields"][key].as_str(),
            ) else {
                continue;
            };
            match at.iter_mut().find(|(n, _)| n == said) {
                Some((_, ids)) => ids.push(id.to_string()),
                None => at.push((said.to_string(), vec![id.to_string()])),
            }
            let inner = row
                .get("rows")
                .and_then(|r| r.get(name))
                .and_then(Value::as_array)
                .map(Vec::as_slice)
                .unwrap_or(&[]);
            gather(
                inner,
                name,
                &format!("{prefix}{}.", crate::pathtext::quote(said)),
                out,
            );
        }
        out.extend(
            at.into_iter()
                .filter(|(_, ids)| ids.len() > 1)
                .map(|(said, ids)| (format!("{prefix}{}", crate::pathtext::quote(&said)), ids)),
        );
    }
    let card = doc.get("kind").and_then(Value::as_str);
    let mut out = Vec::new();
    for name in BY_ROW {
        if by_row(card, name) {
            gather(rows_of(doc, name), name, "", &mut out);
        }
    }
    out
}

/// The ids of a version's lines (`connections`), in its order.
fn line_sequence(doc: Option<&Map<String, Value>>) -> Vec<String> {
    doc.and_then(|d| d.get("connections"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|c| c.get("id").and_then(Value::as_str).map(str::to_string))
        .collect()
}

/// A document as its units: each line of a card (`connections`) one by its id
/// (`c:<id>`), as a field is; a data card's `data` by its top-level fields and the elements of the
/// keyed arrays `split` names, when it is given.
fn units(
    doc: &Map<String, Value>,
    kind: Kind,
    card: Option<&str>,
    split: Option<&DataKeys>,
) -> HashMap<String, Value> {
    let mut out = HashMap::new();
    for (key, value) in doc {
        if kind == Kind::Card && (key == "fields" || key == "rows" || key == "connections") {
            continue;
        }
        out.insert(key.clone(), value.clone());
    }
    if kind == Kind::Section {
        return out;
    }
    for line in doc
        .get("connections")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if let Some(id) = line.get("id").and_then(Value::as_str) {
            out.insert(format!("c:{id}"), line.clone());
        }
    }
    if let Some(fields) = doc.get("fields").and_then(Value::as_object) {
        for (key, value) in fields {
            if key == "data"
                && let (Some(split), Some(data)) = (split, value.as_object())
            {
                out.insert(DATA.into(), Value::Bool(true));
                for (field, held) in data {
                    match (split.get(field), held.as_array()) {
                        (Some(keys), Some(items)) => {
                            out.insert(format!("{DATA}{field}[]"), Value::Bool(true));
                            for item in items {
                                if let Some(key) = element_key(keys, item) {
                                    out.insert(format!("{DATA}{field}{key}"), item.clone());
                                }
                            }
                        }
                        _ => {
                            out.insert(format!("{DATA}{field}"), held.clone());
                        }
                    }
                }
                continue;
            }
            out.insert(format!("f:{key}"), value.clone());
        }
    }
    if let Some(rows) = doc.get("rows").and_then(Value::as_object) {
        for (name, list) in rows {
            if !by_row(card, name) {
                out.insert(format!("rows.{name}"), list.clone());
                continue;
            }
            fn walk(
                name: &str,
                list: &[Value],
                parent: Option<&str>,
                out: &mut HashMap<String, Value>,
            ) {
                for row in list {
                    let Some(object) = row.as_object() else {
                        continue;
                    };
                    let Some(id) = object.get("id").and_then(Value::as_str) else {
                        continue;
                    };
                    let mut unit = object.clone();
                    let inner = unit.remove("rows");
                    unit.insert(PARENT.into(), parent.map_or(Value::Null, Value::from));
                    out.insert(format!("r:{name}:{id}"), Value::Object(unit));
                    if let Some(children) = inner
                        .as_ref()
                        .and_then(|r| r.get(name))
                        .and_then(Value::as_array)
                    {
                        walk(name, children, Some(id), out);
                    }
                }
            }
            walk(
                name,
                list.as_array().map(Vec::as_slice).unwrap_or(&[]),
                None,
                &mut out,
            );
        }
    }
    out
}

/// The ids under `parent` in a version's rows of `name`, in its order.
fn sequence(doc: Option<&Map<String, Value>>, name: &str, parent: Option<&str>) -> Vec<String> {
    fn find<'v>(list: &'v [Value], name: &str, parent: &str) -> Option<&'v [Value]> {
        for row in list {
            let inner = row
                .get("rows")
                .and_then(|r| r.get(name))
                .and_then(Value::as_array)
                .map(Vec::as_slice)
                .unwrap_or(&[]);
            if row.get("id").and_then(Value::as_str) == Some(parent) {
                return Some(inner);
            }
            if let Some(found) = find(inner, name, parent) {
                return Some(found);
            }
        }
        None
    }
    let Some(doc) = doc else {
        return Vec::new();
    };
    let top = rows_of(doc, name);
    let list = match parent {
        None => Some(top),
        Some(p) => find(top, name, p),
    };
    list.unwrap_or(&[])
        .iter()
        .filter_map(|r| r.get("id").and_then(Value::as_str).map(str::to_string))
        .collect()
}

/// Siblings in the order the merge leaves them: the order of the side that moved them
/// (the target's when both did, or neither), and each one only the other side holds
/// after the last of those it follows there.
fn ordered(held: &[String], base: &[String], source: &[String], target: &[String]) -> Vec<String> {
    let all: HashSet<&String> = held.iter().collect();
    let in_source: HashSet<&String> = source.iter().collect();
    let in_target: HashSet<&String> = target.iter().collect();
    let common: HashSet<&String> = base
        .iter()
        .filter(|id| all.contains(id) && in_source.contains(id) && in_target.contains(id))
        .collect();
    let among = |seq: &[String]| -> Vec<String> {
        seq.iter()
            .filter(|id| common.contains(id))
            .cloned()
            .collect()
    };
    let was = among(base);
    let (primary, other) = if among(source) != was && among(target) == was {
        (source, target)
    } else {
        (target, source)
    };
    let mut out: Vec<String> = primary
        .iter()
        .filter(|id| all.contains(id))
        .cloned()
        .collect();
    let at_in = |seq: &[String]| -> HashMap<String, usize> {
        seq.iter()
            .enumerate()
            .map(|(i, id)| (id.clone(), i))
            .collect()
    };
    let (in_primary, in_other) = (at_in(primary), at_in(other));
    let mut rest: Vec<&String> = other.iter().filter(|id| all.contains(id)).collect();
    let mut leftover: Vec<&String> = held
        .iter()
        .filter(|id| !in_other.contains_key(*id))
        .collect();
    leftover.sort();
    rest.extend(leftover);
    // Where each the result holds stands in it, kept as rows are put in.
    let mut place: HashMap<String, usize> = at_in(&out);
    for id in rest {
        if place.contains_key(id) {
            continue;
        }
        let (seq, at) = match in_other.get(id) {
            Some(&at) => (other, at),
            None => (primary, in_primary.get(id).copied().unwrap_or(0)),
        };
        // After the last of those it followed there that the result holds.
        let to = seq[..at]
            .iter()
            .filter_map(|before| place.get(before))
            .max()
            .map_or(0, |i| i + 1);
        for p in place.values_mut() {
            if *p >= to {
                *p += 1;
            }
        }
        out.insert(to, id.clone());
        place.insert(id.clone(), to);
    }
    out
}

/// Units back into a document: the keys in the order the versions had them, the fields'
/// values, the rows — those kept row by row as their tree.
fn assemble(
    units: &HashMap<String, Value>,
    kind: Kind,
    card: Option<&str>,
    versions: [Option<&Map<String, Value>>; 3],
    split: Option<&DataKeys>,
) -> Map<String, Value> {
    let [ancestor, source, target] = versions;
    let order = [target, source];
    let mut keys: Vec<String> = Vec::new();
    for version in order.into_iter().flatten() {
        for key in version.keys() {
            if !keys.contains(key) {
                keys.push(key.clone());
            }
        }
    }
    let mut extra: Vec<&String> = units
        .keys()
        .filter(|k| {
            !k.starts_with("f:")
                && !k.starts_with("r:")
                && !k.starts_with("c:")
                && !k.starts_with("rows.")
                && !k.starts_with(DATA)
                && !keys.contains(k)
        })
        .collect();
    extra.sort();
    keys.extend(extra.into_iter().cloned());
    if kind == Kind::Card {
        for key in ["fields", "rows", "connections"] {
            if !keys.iter().any(|k| k == key) {
                keys.push(key.to_string());
            }
        }
    }
    let mut out = Map::new();
    for key in keys {
        match (kind, key.as_str()) {
            (Kind::Card, "fields") => {
                let mut names: Vec<String> = Vec::new();
                for version in order.into_iter().flatten() {
                    for k in version
                        .get("fields")
                        .and_then(Value::as_object)
                        .into_iter()
                        .flat_map(|f| f.keys())
                    {
                        if !names.contains(k) {
                            names.push(k.clone());
                        }
                    }
                }
                let mut extra: Vec<String> = units
                    .keys()
                    .filter_map(|k| k.strip_prefix("f:"))
                    .filter(|k| !names.iter().any(|n| n == k))
                    .map(str::to_string)
                    .collect();
                if units.contains_key(DATA) && !names.iter().any(|n| n == "data") {
                    extra.push("data".into());
                }
                extra.sort();
                names.extend(extra);
                let data = split.and_then(|split| data_assembled(units, split, versions));
                let fields: Map<String, Value> = names
                    .into_iter()
                    .filter_map(|n| match (&data, n.as_str()) {
                        (Some(d), "data") => Some((n, d.clone())),
                        _ => units.get(&format!("f:{n}")).map(|v| (n, v.clone())),
                    })
                    .collect();
                if !fields.is_empty() {
                    out.insert(key, Value::Object(fields));
                }
            }
            (Kind::Card, "rows") => {
                let mut names: Vec<String> = Vec::new();
                for version in order.into_iter().flatten() {
                    for k in version
                        .get("rows")
                        .and_then(Value::as_object)
                        .into_iter()
                        .flat_map(|r| r.keys())
                    {
                        if !names.contains(k) {
                            names.push(k.clone());
                        }
                    }
                }
                for k in units.keys() {
                    let name = k
                        .strip_prefix("rows.")
                        .or_else(|| k.strip_prefix("r:").and_then(|r| r.split(':').next()));
                    if let Some(name) = name
                        && !names.iter().any(|n| n == name)
                    {
                        names.push(name.to_string());
                    }
                }
                let mut rows = Map::new();
                for name in names {
                    if !by_row(card, &name) {
                        if let Some(v) = units.get(&format!("rows.{name}")) {
                            rows.insert(name, v.clone());
                        }
                        continue;
                    }
                    let prefix = format!("r:{name}:");
                    let mut children: HashMap<Option<String>, Vec<Map<String, Value>>> =
                        HashMap::new();
                    for (k, v) in units {
                        if !k.starts_with(&prefix) {
                            continue;
                        }
                        let Some(row) = v.as_object() else {
                            continue;
                        };
                        let parent = row.get(PARENT).and_then(Value::as_str).map(str::to_string);
                        children.entry(parent).or_default().push(row.clone());
                    }
                    fn tree(
                        name: &str,
                        parent: Option<&str>,
                        children: &HashMap<Option<String>, Vec<Map<String, Value>>>,
                        versions: [Option<&Map<String, Value>>; 3],
                    ) -> Vec<Value> {
                        let Some(here) = children.get(&parent.map(str::to_string)) else {
                            return Vec::new();
                        };
                        let held: Vec<String> = here
                            .iter()
                            .filter_map(|r| r.get("id").and_then(Value::as_str).map(str::to_string))
                            .collect();
                        let [a, s, t] = versions;
                        let order = ordered(
                            &held,
                            &sequence(a, name, parent),
                            &sequence(s, name, parent),
                            &sequence(t, name, parent),
                        );
                        let by_id: HashMap<&str, &Map<String, Value>> = here
                            .iter()
                            .filter_map(|r| r.get("id").and_then(Value::as_str).map(|id| (id, r)))
                            .collect();
                        order
                            .iter()
                            .filter_map(|id| by_id.get(id.as_str()).copied())
                            .map(|row| {
                                let mut row = row.clone();
                                row.remove(PARENT);
                                let id = row
                                    .get("id")
                                    .and_then(Value::as_str)
                                    .unwrap_or_default()
                                    .to_string();
                                let inner = tree(name, Some(&id), children, versions);
                                if !inner.is_empty() {
                                    let mut rows = Map::new();
                                    rows.insert(name.to_string(), Value::Array(inner));
                                    row.insert("rows".into(), Value::Object(rows));
                                }
                                Value::Object(row)
                            })
                            .collect()
                    }
                    let list = tree(&name, None, &children, [ancestor, source, target]);
                    if !list.is_empty() {
                        rows.insert(name, Value::Array(list));
                    }
                }
                if !rows.is_empty() {
                    out.insert(key, Value::Object(rows));
                }
            }
            // Its lines, each as merged, in the order of the side that moved them.
            (Kind::Card, "connections") => {
                let held: Vec<String> = units
                    .keys()
                    .filter_map(|k| k.strip_prefix("c:").map(str::to_string))
                    .collect();
                let order = ordered(
                    &held,
                    &line_sequence(ancestor),
                    &line_sequence(source),
                    &line_sequence(target),
                );
                let list: Vec<Value> = order
                    .iter()
                    .filter_map(|id| units.get(&format!("c:{id}")).cloned())
                    .collect();
                if !list.is_empty() {
                    out.insert(key, Value::Array(list));
                }
            }
            _ => {
                if let Some(v) = units.get(&key) {
                    out.insert(key, v.clone());
                }
            }
        }
    }
    out
}

/// Units named as a person reads them, from the versions' own names.
struct Labels {
    /// A row kept apart by id, by its table and id: its name among its siblings and its parent.
    rows: HashMap<(String, String), (String, Option<String>)>,
    /// A line: its kind and the row it leaves (`connections.<line>.<from>`), numbered
    /// when several are the same.
    lines: HashMap<String, String>,
}

impl Labels {
    fn of(versions: [Option<&Map<String, Value>>; 2]) -> Labels {
        let mut out = Labels {
            rows: HashMap::new(),
            lines: HashMap::new(),
        };
        fn walk(name: &str, list: &[Value], parent: Option<&str>, out: &mut Labels) {
            for row in list {
                let Some(id) = row.get("id").and_then(Value::as_str) else {
                    continue;
                };
                let said = row["fields"][name_key(name)]
                    .as_str()
                    .unwrap_or(id)
                    .to_string();
                out.rows
                    .entry((name.to_string(), id.to_string()))
                    .or_insert((said, parent.map(str::to_string)));
                let inner = row
                    .get("rows")
                    .and_then(|r| r.get(name))
                    .and_then(Value::as_array)
                    .map(Vec::as_slice)
                    .unwrap_or(&[]);
                walk(name, inner, Some(id), out);
            }
        }
        for version in versions.into_iter().flatten() {
            for name in BY_ROW {
                walk(name, rows_of(version, name), None, &mut out);
            }
            for line in version
                .get("connections")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                let Some(id) = line.get("id").and_then(Value::as_str) else {
                    continue;
                };
                if out.lines.contains_key(id) {
                    continue;
                }
                let mut said = format!(
                    "connections.{}",
                    line.get("line").and_then(Value::as_str).unwrap_or_default()
                );
                if let Some(from) = line.get("from").and_then(Value::as_str) {
                    said = format!("{said}.{from}");
                }
                let taken = out.lines.values().filter(|l| l.starts_with(&said)).count();
                if taken > 0 {
                    said = format!("{said}#{}", taken + 1);
                }
                out.lines.insert(id.to_string(), said);
            }
        }
        out
    }

    /// A row by its path among its parents (`fields.address.zip`, `rows.auth.login`,
    /// `values.paid`, `versions.v0_2_34`), a line by what it is, any other by its key.
    fn label(&self, unit: &str) -> String {
        if let Some((name, id)) = unit.strip_prefix("r:").and_then(|r| r.split_once(':')) {
            let mut names = Vec::new();
            let mut at = Some(id.to_string());
            let mut seen = HashSet::new();
            while let Some(here) = at {
                if !seen.insert(here.clone()) {
                    break;
                }
                let Some((said, parent)) = self.rows.get(&(name.to_string(), here)) else {
                    break;
                };
                names.insert(0, said.clone());
                at = parent.clone();
            }
            if names.is_empty() {
                names.push(id.to_string());
            }
            return format!(
                "{name}.{}",
                crate::pathtext::join_names(names.iter().map(String::as_str))
            );
        }
        if let Some(id) = unit.strip_prefix("c:") {
            return self
                .lines
                .get(id)
                .cloned()
                .unwrap_or_else(|| format!("connections.{id}"));
        }
        if let Some(key) = unit.strip_prefix("f:") {
            return key.to_string();
        }
        if let Some(rest) = unit.strip_prefix(DATA) {
            let rest = rest.strip_suffix("[]").unwrap_or(rest);
            return if rest.is_empty() {
                "data".to_string()
            } else {
                format!("data.{rest}")
            };
        }
        if let Some(name) = unit.strip_prefix("rows.") {
            return name.to_string();
        }
        unit.to_string()
    }
}

/// Each unit with its label, the same label told apart by a number (`#2`…) in the order of
/// the units, so `take` names one conflict of a card by it alone.
fn unique(mut labelled: Vec<(String, String)>) -> HashMap<String, String> {
    labelled.sort();
    let mut seen: HashMap<String, usize> = HashMap::new();
    labelled
        .into_iter()
        .map(|(unit, label)| {
            let n = seen.entry(label.clone()).or_default();
            *n += 1;
            let said = if *n == 1 {
                label
            } else {
                format!("{label}#{n}")
            };
            (unit, said)
        })
        .collect()
}

/// A unit left for `take`: as it is kept apart, as a person names it, and its three
/// versions (base, source, target).
type Unsettled = (String, String, Option<Value>, Option<Value>, Option<Value>);

/// One card or section merged: the result (none when removed), the conflicts `take` did
/// not settle (the result holds the target's side of each), and whether the result is not
/// what the target holds already.
struct Outcome {
    result: Option<Map<String, Value>>,
    conflicts: Vec<Unsettled>,
    changes: bool,
}

/// A row one side removed while the other changed it or what it holds under it: one
/// conflict, of that row, so nothing the keeper changed inside goes unsaid.
struct Removal {
    /// The row removed, as it is kept apart (`r:<table>:<id>`).
    top: String,
    remover: Side,
    /// The keeper's rows from `top` down, `top` among them.
    inside: Vec<String>,
    /// Those of `inside` the remover holds nowhere: gone with `top`.
    dropped: Vec<String>,
}

/// Each row's children, by its unit: the units of the rows under it, as a version holds them.
fn children(units: &HashMap<String, Value>) -> HashMap<String, Vec<String>> {
    let mut out: HashMap<String, Vec<String>> = HashMap::new();
    for (unit, value) in units {
        let (Some((name, _)), Some(parent)) = (
            unit.strip_prefix("r:").and_then(|r| r.split_once(':')),
            value.get(PARENT).and_then(Value::as_str),
        ) else {
            continue;
        };
        out.entry(format!("r:{name}:{parent}"))
            .or_default()
            .push(unit.clone());
    }
    out
}

/// The rows one side removed while the other changed them or anything under them, the
/// outermost of each: the side that kept a row changed inside it is told, not dropped.
fn removals(
    base: &HashMap<String, Value>,
    mine: &HashMap<String, Value>,
    theirs: &HashMap<String, Value>,
) -> Vec<Removal> {
    let mut tops: Vec<&String> = base
        .keys()
        .filter(|k| k.starts_with("r:") && mine.contains_key(*k) != theirs.contains_key(*k))
        .collect();
    if tops.is_empty() {
        return Vec::new();
    }
    tops.sort();
    let (kids_mine, kids_theirs) = (children(mine), children(theirs));
    let mut found: Vec<Removal> = Vec::new();
    for top in tops {
        let (remover, keeper, gone, kids) = if theirs.contains_key(top) {
            (Side::Source, theirs, mine, &kids_theirs)
        } else {
            (Side::Target, mine, theirs, &kids_mine)
        };
        let mut inside = vec![top.clone()];
        let mut seen: HashSet<&String> = HashSet::from([top]);
        let mut at = 0;
        while at < inside.len() {
            if let Some(under) = kids.get(&inside[at]) {
                for unit in under {
                    if seen.insert(unit) {
                        inside.push(unit.clone());
                    }
                }
            }
            at += 1;
        }
        let changed = inside
            .iter()
            .any(|u| keeper.get(u).map(canonical) != base.get(u).map(canonical));
        if !changed {
            continue;
        }
        let dropped = inside
            .iter()
            .filter(|u| !gone.contains_key(*u))
            .cloned()
            .collect();
        found.push(Removal {
            top: top.clone(),
            remover,
            inside,
            dropped,
        });
    }
    // One inside another removed by the same side is settled with the outer.
    let outer: Vec<(String, Side, HashSet<String>)> = found
        .iter()
        .map(|r| (r.top.clone(), r.remover, r.inside.iter().cloned().collect()))
        .collect();
    found.retain(|r| {
        !outer
            .iter()
            .any(|(top, by, inside)| *top != r.top && *by == r.remover && inside.contains(&r.top))
    });
    found
}

/// A removed row's version for its conflict: the row with the rows it holds under it, as
/// that version has them — what goes, or stays, with it.
fn inside_of(
    units: &HashMap<String, Value>,
    doc: Option<&Map<String, Value>>,
    unit: &str,
) -> Option<Value> {
    fn find<'v>(list: &'v [Value], name: &str, id: &str) -> Option<&'v Value> {
        list.iter().find_map(|row| {
            if row.get("id").and_then(Value::as_str) == Some(id) {
                return Some(row);
            }
            find(
                row.get("rows")
                    .and_then(|r| r.get(name))
                    .and_then(Value::as_array)
                    .map(Vec::as_slice)
                    .unwrap_or(&[]),
                name,
                id,
            )
        })
    }
    let mut row = units.get(unit)?.clone();
    if let (Some((name, id)), Some(doc), Some(o)) = (
        unit.strip_prefix("r:").and_then(|r| r.split_once(':')),
        doc,
        row.as_object_mut(),
    ) && let Some(inner) = find(rows_of(doc, name), name, id).and_then(|r| r.get("rows"))
    {
        o.insert("rows".into(), inner.clone());
    }
    Some(row)
}

/// Rows the merge leaves under no row it keeps — their parent gone, or moved under one
/// another on the two sides — go under the nearest of their parents that is kept (as the
/// target, the source, then the base had them), or at the top: never dropped unsaid.
fn rehome(result: &mut HashMap<String, Value>, sides: [&HashMap<String, Value>; 3]) {
    let parent_of = |units: &HashMap<String, Value>, unit: &str| -> Option<String> {
        let (name, _) = unit.strip_prefix("r:")?.split_once(':')?;
        let parent = units.get(unit)?.get(PARENT)?.as_str()?;
        Some(format!("r:{name}:{parent}"))
    };
    loop {
        let kids = children(result);
        let mut reached: HashSet<String> = HashSet::new();
        let mut next: Vec<String> = result
            .iter()
            .filter(|(k, v)| k.starts_with("r:") && v.get(PARENT).is_none_or(Value::is_null))
            .map(|(k, _)| k.clone())
            .collect();
        while let Some(unit) = next.pop() {
            if reached.insert(unit.clone()) {
                next.extend(kids.get(&unit).into_iter().flatten().cloned());
            }
        }
        let mut lost: Vec<&String> = result
            .keys()
            .filter(|k| k.starts_with("r:") && !reached.contains(*k))
            .collect();
        if lost.is_empty() {
            return;
        }
        lost.sort();
        // One whose parent is gone first; where every lost one's parent is kept, they are
        // under one another, and the first is taken out.
        let unit = lost
            .iter()
            .find(|u| parent_of(result, u).is_some_and(|p| !result.contains_key(&p)))
            .unwrap_or(&lost[0])
            .to_string();
        let mut at = parent_of(result, &unit);
        let mut seen = HashSet::from([unit.clone()]);
        let home = loop {
            let Some(here) = at else {
                break None;
            };
            if reached.contains(&here) {
                break Some(here);
            }
            if !seen.insert(here.clone()) {
                break None;
            }
            at = sides.iter().find_map(|s| parent_of(s, &here));
        };
        let id = home
            .as_deref()
            .and_then(|h| h.strip_prefix("r:"))
            .and_then(|r| r.split_once(':'))
            .map(|(_, id)| Value::from(id));
        if let Some(Value::Object(row)) = result.get_mut(&unit) {
            row.insert(PARENT.into(), id.unwrap_or(Value::Null));
        }
    }
}

fn merge3(
    kind: Kind,
    ancestor: Option<&Map<String, Value>>,
    source: Option<&Map<String, Value>>,
    target: Option<&Map<String, Value>>,
    take: Option<&HashMap<String, Side>>,
    keys: Option<&DataKeys>,
) -> Outcome {
    let (a, s, t) = (ancestor.map(text), source.map(text), target.map(text));
    let kept = |changes| Outcome {
        result: target.cloned(),
        conflicts: Vec::new(),
        changes,
    };
    if s == t || s == a {
        return kept(false);
    }
    if t == a {
        return Outcome {
            result: source.cloned(),
            conflicts: Vec::new(),
            changes: true,
        };
    }
    let chosen = |label: &str, unit: &str| {
        take.and_then(|t| t.get(label).or_else(|| t.get(unit)))
            .copied()
    };
    // One side removed what the other changed.
    let (Some(source_doc), Some(target_doc)) = (source, target) else {
        let chose = chosen(WHOLE, WHOLE).or_else(|| {
            take.and_then(|t| t.iter().find(|(unit, _)| is_whole(unit)))
                .map(|(_, side)| *side)
        });
        return match chose {
            Some(Side::Source) => Outcome {
                result: source.cloned(),
                conflicts: Vec::new(),
                changes: true,
            },
            Some(Side::Target) => kept(false),
            None => Outcome {
                result: target.cloned(),
                conflicts: vec![(
                    WHOLE.to_string(),
                    WHOLE.to_string(),
                    ancestor.map(|d| Value::Object(d.clone())),
                    source.map(|d| Value::Object(d.clone())),
                    target.map(|d| Value::Object(d.clone())),
                )],
                changes: false,
            },
        };
    };
    let card = card_kind(&[Some(target_doc), Some(source_doc), ancestor]);
    // A data card's data by its top-level fields, and its clean keyed arrays an element at a time.
    let split = (card == Some(crate::kinds::INSTANCE)).then(|| {
        data_split(
            keys.unwrap_or(&DataKeys::new()),
            &[ancestor, Some(source_doc), Some(target_doc)],
        )
    });
    let split = split.as_ref().filter(|_| keys.is_some());
    let base = ancestor
        .map(|d| units(d, kind, card, split))
        .unwrap_or_default();
    let mine = units(source_doc, kind, card, split);
    let theirs = units(target_doc, kind, card, split);
    let names = Labels::of([Some(target_doc), Some(source_doc)]);
    let side = |s: Side| if s == Side::Source { &mine } else { &theirs };
    let mut all: Vec<&String> = base
        .keys()
        .chain(mine.keys())
        .chain(theirs.keys())
        .collect();
    all.sort();
    all.dedup();
    let removals = removals(&base, &mine, &theirs);
    // What a removal dropped is settled with it, never apart.
    let dropped: HashSet<&String> = removals.iter().flat_map(|r| &r.dropped).collect();
    let apart = |unit: &String| {
        let was = base.get(unit).map(canonical);
        let now = mine.get(unit).map(canonical);
        let held = theirs.get(unit).map(canonical);
        now != held && now != was && held != was
    };
    // Named before any is chosen, from every unit that could be left for `take`, so a
    // conflict has one name whatever else is chosen.
    let labels = unique(
        all.iter()
            .filter(|u| !dropped.contains(*u) && apart(u))
            .map(|u| (u.to_string(), names.label(u)))
            .chain(
                removals
                    .iter()
                    .map(|r| (r.top.clone(), names.label(&r.top))),
            )
            .collect(),
    );
    let label_of = |unit: &str| {
        labels
            .get(unit)
            .cloned()
            .unwrap_or_else(|| names.label(unit))
    };
    let mut result = theirs.clone();
    let mut conflicts = Vec::new();
    let mut settled: HashSet<String> = HashSet::new();
    for removal in &removals {
        let label = label_of(&removal.top);
        let picked = chosen(&label, &removal.top);
        let wins = picked.unwrap_or(Side::Target);
        // The remover chosen takes all inside as it has it; the keeper, or nothing chosen
        // yet, only what the remover dropped, the rest merged one by one so that its own
        // conflicts are said.
        let decided = if picked.is_some() && wins == removal.remover {
            &removal.inside
        } else {
            &removal.dropped
        };
        for unit in decided {
            if !settled.insert(unit.clone()) {
                continue;
            }
            match side(wins).get(unit) {
                Some(v) => result.insert(unit.clone(), v.clone()),
                None => result.remove(unit),
            };
        }
        if picked.is_none() {
            conflicts.push((
                removal.top.clone(),
                label,
                inside_of(&base, ancestor, &removal.top),
                inside_of(&mine, Some(source_doc), &removal.top),
                inside_of(&theirs, Some(target_doc), &removal.top),
            ));
        }
    }
    for unit in all {
        if settled.contains(unit) {
            continue;
        }
        let was = base.get(unit).map(canonical);
        let now = mine.get(unit).map(canonical);
        let held = theirs.get(unit).map(canonical);
        if now == held || now == was {
            continue;
        }
        if held == was {
            match mine.get(unit) {
                Some(v) => result.insert(unit.clone(), v.clone()),
                None => result.remove(unit),
            };
            continue;
        }
        let label = label_of(unit);
        match chosen(&label, unit) {
            Some(Side::Source) => {
                match mine.get(unit) {
                    Some(v) => result.insert(unit.clone(), v.clone()),
                    None => result.remove(unit),
                };
            }
            Some(Side::Target) => {}
            None => conflicts.push((
                unit.clone(),
                label,
                base.get(unit).cloned(),
                mine.get(unit).cloned(),
                theirs.get(unit).cloned(),
            )),
        }
    }
    rehome(&mut result, [&theirs, &mine, &base]);
    let merged = assemble(
        &result,
        kind,
        card,
        [ancestor, Some(source_doc), Some(target_doc)],
        split,
    );
    let changes = text(&merged) != text(target_doc);
    Outcome {
        result: Some(merged),
        conflicts,
        changes,
    }
}

/// One card or section merged three ways on its own, with no choices (`merge3`): what undoing one
/// commit writes, a row at a time.
pub(crate) struct ThreeWay {
    pub result: Option<Map<String, Value>>,
    /// Each unit both sides changed apart: as it is kept apart, as a person names it, and its base,
    /// source and target.
    pub conflicts: Vec<Unsettled>,
    /// Whether the result is not what the target holds already.
    pub changes: bool,
}

/// `source` over `target` from `ancestor`, unit by unit, as a merge takes them: a section's by its
/// keys, a card's by its keys, fields, rows and lines.
pub(crate) fn three_way(
    section: bool,
    ancestor: Option<&Map<String, Value>>,
    source: Option<&Map<String, Value>>,
    target: Option<&Map<String, Value>>,
) -> ThreeWay {
    let kind = if section { Kind::Section } else { Kind::Card };
    let Outcome {
        result,
        conflicts,
        changes,
    } = merge3(kind, ancestor, source, target, None, None);
    ThreeWay {
        result,
        conflicts,
        changes,
    }
}

/// A unit of a card's or a section's document that two versions of it hold apart.
#[derive(Clone, Debug)]
pub struct UnitChange {
    /// As it is kept apart (`f:<key>`, `r:<rows>:<id>`, `c:<id>`, a key).
    pub unit: String,
    /// As a person names it, one of the document's alone (`fields.total`, `connections.branch`).
    pub label: String,
    pub before: Option<Value>,
    pub after: Option<Value>,
}

/// The units two versions of one card or section hold apart, named as a merge's conflicts are
/// (`Labels`, the same name told apart by a number), in the order of their names.
pub fn unit_changes(
    section: bool,
    before: &Map<String, Value>,
    after: &Map<String, Value>,
) -> Vec<UnitChange> {
    let kind = if section { Kind::Section } else { Kind::Card };
    let card = card_kind(&[Some(after), Some(before)]);
    let (was, now) = (
        units(before, kind, card, None),
        units(after, kind, card, None),
    );
    let names = Labels::of([Some(after), Some(before)]);
    let mut changed: Vec<&String> = was
        .keys()
        .chain(now.keys())
        .filter(|u| was.get(*u).map(canonical) != now.get(*u).map(canonical))
        .collect();
    changed.sort();
    changed.dedup();
    let labels = unique(
        changed
            .iter()
            .map(|u| (u.to_string(), names.label(u)))
            .collect(),
    );
    let mut out: Vec<UnitChange> = changed
        .into_iter()
        .map(|u| UnitChange {
            unit: u.clone(),
            label: labels.get(u).cloned().unwrap_or_else(|| names.label(u)),
            before: was.get(u).cloned(),
            after: now.get(u).cloned(),
        })
        .collect();
    out.sort_by(|a, b| a.label.cmp(&b.label));
    out
}

// MARK: - Working it out

struct DocRow {
    removed: bool,
    doc: Option<String>,
    base: Option<String>,
    based_at: Option<String>,
}

/// What a branch keeps of main's rows of `table` as main held them when it was made
/// (`branch_frozen`): by id, none for one main made since.
fn frozen_rows(
    conn: &Connection,
    table: Table,
    project: &str,
    tree: &str,
) -> Result<HashMap<String, Option<String>>, String> {
    if tree.is_empty() {
        return Ok(HashMap::new());
    }
    let mut stmt = conn
        .prepare(
            "SELECT id, doc FROM branch_frozen WHERE project_id = ? AND branch_id = ? AND tbl = ?",
        )
        .map_err(db)?;
    stmt.query_map(params![project, tree, table.name()], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?))
    })
    .map_err(db)?
    .collect::<Result<_, _>>()
    .map_err(db)
}

fn doc_rows(
    conn: &Connection,
    table: Table,
    project: &str,
    tree: &str,
) -> Result<HashMap<String, DocRow>, String> {
    let mut stmt = conn
        .prepare(&format!(
            "SELECT id, removed, doc, base, based_at FROM {} WHERE project_id = ? AND branch_id = ?",
            table.name()
        ))
        .map_err(db)?;
    stmt.query_map(params![project, tree], |r| {
        Ok((
            r.get::<_, String>(0)?,
            DocRow {
                removed: r.get::<_, i64>(1)? != 0,
                doc: r.get(2)?,
                base: r.get(3)?,
                based_at: r.get(4)?,
            },
        ))
    })
    .map_err(db)?
    .collect::<Result<_, _>>()
    .map_err(db)
}

/// A branch of the project: its name and when it was made. `""` is main.
fn branch_info(conn: &Connection, project: &str, tree: &str) -> Result<(String, String), String> {
    if tree.is_empty() {
        return Ok(("main".to_string(), String::new()));
    }
    if tree == crate::store::BOARD {
        return Err(text!("write.merge.boardNotMerged"));
    }
    conn.query_row(
        "SELECT name, created_at FROM branches WHERE id = ? AND project_id = ?",
        params![tree, project],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )
    .map_err(|_| text!("write.merge.branchNotFound"))
}

/// One card or section, merged, and how it goes into the target.
struct Merged {
    id: String,
    /// Its path, to say what could not be written.
    path: String,
    table: Table,
    result: Option<Map<String, Value>>,
    changes: bool,
    /// The base the target's row takes, and when: none for one of its own.
    base: Option<(String, String)>,
    /// One side removed it, the other changed it, and neither is chosen yet: whether it stays
    /// is not known, so no path it holds is said to meet another.
    undecided: bool,
}

/// How a merged row goes into the target.
enum Write<'d> {
    /// The target's own row let go of: the row removed, or main's beneath a branch as it is.
    Clear,
    /// A branch's mark that it removed main's row.
    Mark,
    /// The merged document, with the base a branch's row is on.
    Put(&'d Map<String, Value>, Option<(String, String)>),
}

/// What a merge works out before it writes anything.
pub(crate) struct Work {
    pub preview: Preview,
    merged: Vec<Merged>,
    /// The boxes the source moved that would change the target (`places`), all written when
    /// they are taken along; those that go in only then are counted (`Preview::places`).
    moves: Vec<Moved>,
}

/// A box of the source's that changes the target when the boxes are taken along: a card's
/// (a card ghost's, a writing card's size with it) on its own section's diagram, another
/// section's card or a section ghost on a section's diagram, a flow's node, a card on a
/// goal's card map.
struct Moved {
    view: String,
    item_kind: String,
    item: String,
    /// The source's box; none for a branch's own to go, main's beneath it the same.
    spot: Option<super::Spot>,
    /// Whether it changes the target only when the boxes are taken along: not one of a
    /// merged card's (or its nodes') that the target places nowhere, which goes in anyway
    /// (`place_merged`).
    beyond: bool,
}

/// Every box a tree keeps of its own, by (view, kind, item).
fn boxes_of(
    conn: &Connection,
    project: &str,
    tree: &str,
) -> Result<HashMap<(String, String, String), super::Spot>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT view, item_kind, item, x, y, w, h, pinned, look FROM places WHERE project_id = ? AND branch_id = ?",
        )
        .map_err(db)?;
    stmt.query_map(params![project, tree], |r| {
        Ok((
            (r.get(0)?, r.get(1)?, r.get(2)?),
            super::Spot {
                x: r.get(3)?,
                y: r.get(4)?,
                w: r.get(5)?,
                h: r.get(6)?,
                pinned: r.get::<_, i64>(7)? != 0,
                look: r.get(8)?,
            },
        ))
    })
    .map_err(db)?
    .collect::<Result<_, _>>()
    .map_err(db)
}

/// The paths of a tree's cards and sections, by id.
fn names(conn: &Connection, project: &str, tree: &str) -> Result<HashMap<String, String>, String> {
    let reader = crate::store::Reader { conn };
    let branch = (!tree.is_empty()).then_some(tree);
    let detail = crate::model::typed(&crate::detail::read(&reader, project, branch)?)?;
    let project = crate::export::Project::new(&detail);
    let mut out: HashMap<String, String> = detail
        .sections
        .iter()
        .map(|s| (s.id.clone(), project.directory.full_section(&s.id)))
        .collect();
    for card in detail.schemas.iter().chain(detail.inner.iter()) {
        out.insert(card.id.clone(), project.full(card));
    }
    for ghost in &detail.ghosts {
        out.insert(ghost.id.clone(), project.full_ghost(ghost));
    }
    Ok(out)
}

fn parse(text: Option<&String>) -> Option<Map<String, Value>> {
    crate::store::parse(text.map(String::as_str))
}

impl Tx<'_> {
    /// What merging `source` into `target` (`""` for main) would do.
    pub fn merge_preview(
        &self,
        project: &str,
        source: &str,
        target: &str,
        take: &Take,
    ) -> Result<Preview, String> {
        Ok(self.merge_work(project, source, target, take)?.preview)
    }

    pub(crate) fn merge_work(
        &self,
        project: &str,
        source: &str,
        target: &str,
        take: &Take,
    ) -> Result<Work, String> {
        if source == target {
            return Err(text!("write.merge.sameTree"));
        }
        let conn = self.conn();
        let (from, from_made) = branch_info(conn, project, source)?;
        let (into, into_made) = branch_info(conn, project, target)?;
        let mut preview = Preview {
            source: from,
            target: into,
            ..Preview::default()
        };
        let now = self.now()?;
        let views = [names(conn, project, target)?, names(conn, project, source)?];
        let name = |id: &str| -> String {
            views
                .iter()
                .find_map(|v| v.get(id).cloned())
                .unwrap_or_else(|| id.to_string())
        };
        let mut merged = Vec::new();
        for (table, section) in [(Table::Sections, true), (Table::Cards, false)] {
            let kind = if section { Kind::Section } else { Kind::Card };
            let main = doc_rows(conn, table, project, "")?;
            let mine = if source.is_empty() {
                HashMap::new()
            } else {
                doc_rows(conn, table, project, source)?
            };
            let theirs = if target.is_empty() {
                HashMap::new()
            } else {
                doc_rows(conn, table, project, target)?
            };
            // What each side sees of main where it wrote nothing: main as it was made, where main
            // has changed it since (`branch_frozen`).
            let (kept_mine, kept_theirs) = (
                frozen_rows(conn, table, project, source)?,
                frozen_rows(conn, table, project, target)?,
            );
            let version = |rows: &HashMap<String, DocRow>,
                           kept: &HashMap<String, Option<String>>,
                           id: &str|
             -> Option<Map<String, Value>> {
                match rows.get(id) {
                    Some(row) if row.removed => None,
                    Some(row) => parse(row.doc.as_ref()),
                    None => match kept.get(id) {
                        Some(then) => parse(then.as_ref()),
                        None => parse(main.get(id).and_then(|r| r.doc.as_ref())),
                    },
                }
            };
            // A base and when it was taken; one from before times were kept, as old as
            // its branch.
            let based = |row: Option<&DocRow>, made: &str| -> Option<(String, String)> {
                let row = row?;
                let base = row.base.clone()?;
                Some((
                    base,
                    row.based_at.clone().unwrap_or_else(|| made.to_string()),
                ))
            };
            // What the source changed: its own rows; main's changes, for what the target
            // holds rows for.
            let mut ids: Vec<&String> = if source.is_empty() {
                theirs.keys().collect()
            } else {
                mine.keys().collect()
            };
            ids.sort();
            for id in ids {
                let mine_base = if source.is_empty() {
                    None
                } else {
                    based(mine.get(id), &from_made)
                };
                let their_base = if target.is_empty() {
                    None
                } else {
                    based(theirs.get(id), &into_made)
                };
                let mut ancestor = match (&mine_base, &their_base) {
                    (Some(m), Some(t)) => Some(if m.1 <= t.1 { m.clone() } else { t.clone() }),
                    (Some(m), None) => Some(m.clone()),
                    (None, Some(t)) => Some(t.clone()),
                    (None, None) => None,
                };
                // One the target made itself, main's never held.
                if source.is_empty() && theirs.get(id).is_some_and(|r| r.base.is_none()) {
                    ancestor = None;
                }
                let path = name(id);
                let main_now = parse(main.get(id).and_then(|r| r.doc.as_ref()));
                let source_doc = if source.is_empty() {
                    main_now.clone()
                } else {
                    version(&mine, &kept_mine, id)
                };
                let target_doc = if target.is_empty() {
                    main_now.clone()
                } else {
                    version(&theirs, &kept_theirs, id)
                };
                let ancestor_doc = ancestor.as_ref().and_then(|a| parse(Some(&a.0)));
                // A data card's keyed arrays, by its shape as the target has it (else the source).
                let keys = (!section)
                    .then(|| {
                        let doc_of = |id: &str| {
                            let target_version = if target.is_empty() {
                                parse(main.get(id).and_then(|r| r.doc.as_ref()))
                            } else {
                                version(&theirs, &kept_theirs, id)
                            };
                            target_version.or_else(|| {
                                if source.is_empty() {
                                    parse(main.get(id).and_then(|r| r.doc.as_ref()))
                                } else {
                                    version(&mine, &kept_mine, id)
                                }
                            })
                        };
                        [&target_doc, &source_doc, &ancestor_doc]
                            .into_iter()
                            .flatten()
                            .find_map(|d| data_keys(d, &doc_of))
                    })
                    .flatten();
                let outcome = merge3(
                    kind,
                    ancestor_doc.as_ref(),
                    source_doc.as_ref(),
                    target_doc.as_ref(),
                    take.get(&path),
                    keys.as_ref(),
                );
                // The base the target's row is on: main now where main is mixed in, the
                // newer of two where both sides kept one.
                let mut base = None;
                if !target.is_empty()
                    && let Some(main_text) = main.get(id).and_then(|r| r.doc.clone())
                {
                    base = Some(if source.is_empty() || !theirs.contains_key(id) {
                        (main_text, now.clone())
                    } else {
                        match (&mine_base, &their_base) {
                            (Some(m), Some(t)) => {
                                if m.1 >= t.1 {
                                    m.clone()
                                } else {
                                    t.clone()
                                }
                            }
                            _ => their_base
                                .clone()
                                .or(mine_base.clone())
                                .unwrap_or((main_text, now.clone())),
                        }
                    });
                }
                let card_kind = [&source_doc, &target_doc, &ancestor_doc]
                    .into_iter()
                    .flatten()
                    .find_map(|d| d.get("kind").and_then(Value::as_str).map(str::to_string));
                for (unit, label, b, s, t) in &outcome.conflicts {
                    preview.conflicts.push(Conflict {
                        path: path.clone(),
                        section,
                        kind: if section { None } else { card_kind.clone() },
                        unit: unit.clone(),
                        label: label.clone(),
                        base: b.clone(),
                        source: s.clone(),
                        target: t.clone(),
                    });
                }
                if outcome.changes {
                    let had = if target.is_empty() {
                        main.contains_key(id)
                    } else {
                        version(&theirs, &kept_theirs, id).is_some()
                    };
                    let change = match (&outcome.result, had) {
                        (None, _) => Change::Remove,
                        (Some(_), true) => Change::Change,
                        (Some(_), false) => Change::Add,
                    };
                    preview.items.push(Item {
                        path: path.clone(),
                        section,
                        change,
                    });
                }
                let undecided = outcome.conflicts.iter().any(|c| c.0 == WHOLE);
                merged.push(Merged {
                    id: id.clone(),
                    path,
                    table,
                    result: outcome.result,
                    changes: outcome.changes,
                    base,
                    undecided,
                });
            }
            // main taken in: what main changed since the target was made and the target kept as
            // it was, coming in as main has it now (nothing to choose: the target wrote none of it).
            if source.is_empty() {
                let mut kept: Vec<(&String, &Option<String>)> = kept_theirs
                    .iter()
                    .filter(|(id, _)| !theirs.contains_key(*id))
                    .collect();
                kept.sort_by(|a, b| a.0.cmp(b.0));
                for (id, then) in kept {
                    let now = main.get(id).and_then(|r| r.doc.as_ref());
                    let change = match (then.as_ref(), now) {
                        (None, Some(_)) => Change::Add,
                        (Some(_), None) => Change::Remove,
                        (Some(a), Some(b)) if parse(Some(a)) != parse(Some(b)) => Change::Change,
                        _ => continue,
                    };
                    preview.items.push(Item {
                        path: name(id),
                        section,
                        change,
                    });
                }
            }
        }
        preview.overlaps = self.overlaps(project, target, &merged, &name)?;
        let moves = self.place_moves(project, source, target, &merged)?;
        preview.places = moves.iter().filter(|m| m.beyond).count();
        Ok(Work {
            preview,
            merged,
            moves,
        })
    }

    /// The boxes the source moved that change the target when they are taken along: every
    /// box the source keeps of its own whose diagram and whose card, section or node the
    /// target holds once merged, and that the target does not place there the same way
    /// already (its own box, or main's beneath a branch). None from main.
    fn place_moves(
        &self,
        project: &str,
        source: &str,
        target: &str,
        merged: &[Merged],
    ) -> Result<Vec<Moved>, String> {
        if source.is_empty() {
            return Ok(Vec::new());
        }
        let conn = self.conn();
        let own = boxes_of(conn, project, source)?;
        if own.is_empty() {
            return Ok(Vec::new());
        }
        let main = boxes_of(conn, project, "")?;
        let theirs = if target.is_empty() {
            HashMap::new()
        } else {
            boxes_of(conn, project, target)?
        };
        let changed: HashMap<(Table, &str), Option<&Map<String, Value>>> = merged
            .iter()
            .filter(|m| m.changes)
            .map(|m| ((m.table, m.id.as_str()), m.result.as_ref()))
            .collect();
        // A card's or a section's document in the target once merged.
        let after = |table: Table, id: &str| -> Result<Option<Map<String, Value>>, String> {
            if let Some(result) = changed.get(&(table, id)) {
                return Ok(result.cloned());
            }
            Ok(self
                .seen(table, project, target, id)?
                .filter(|s| !s.removed)
                .and_then(|s| s.doc()))
        };
        let held = |table: Table, id: &str| -> Result<bool, String> {
            Ok(!id.is_empty() && after(table, id)?.is_some())
        };
        // A diagram: a card's own section's (`""`), a section's, or a card's own (a
        // container's card map: a goal's, a flow's).
        let drawn = |view: &str| -> Result<bool, String> {
            Ok(view.is_empty() || held(Table::Sections, view)? || held(Table::Cards, view)?)
        };
        // A diagram the merge itself brings: a section or a card the target holds only once merged.
        let made_here = |view: &str| -> Result<bool, String> {
            for table in [Table::Sections, Table::Cards] {
                if changed.get(&(table, view)).is_some_and(Option::is_some)
                    && self
                        .seen(table, project, target, view)?
                        .filter(|s| !s.removed)
                        .is_none()
                {
                    return Ok(true);
                }
            }
            Ok(false)
        };
        let mut keys: Vec<&(String, String, String)> = own.keys().collect();
        keys.sort();
        let mut out = Vec::new();
        for key in keys {
            let (view, item_kind, item) = key;
            let there = match item_kind.as_str() {
                "card" => held(Table::Cards, item)? && drawn(view)?,
                "section" => held(Table::Sections, item)? && drawn(view)?,
                // A group on a diagram the target holds once merged, by its name.
                "group" => drawn(view)?,
                _ => false,
            };
            if !there {
                continue;
            }
            let spot = &own[key];
            let seen = theirs.get(key).or_else(|| main.get(key));
            if seen == Some(spot) {
                continue;
            }
            // A group the source let go of that the target has none of: nothing to let go of.
            if item_kind == "group" && spot.x.is_none() && seen.is_none_or(|s| s.x.is_none()) {
                continue;
            }
            // A branch taking main's box as it is: its own goes, and main's shows.
            let as_main = !target.is_empty() && main.get(key) == Some(spot);
            // Where the target places a merged card (or its nodes) nowhere, the source's
            // box goes in without `places` too (`place_merged`).
            let card = match item_kind.as_str() {
                "card" => Some(item),
                "node" => Some(view),
                _ => None,
            };
            // A group on a diagram the merge brings goes in with it, as the boxes of a merged card do.
            let anyway = if item_kind == "group" {
                made_here(view)?
            } else {
                seen.is_none()
                    && card.is_some_and(|c| {
                        changed
                            .get(&(Table::Cards, c.as_str()))
                            .is_some_and(Option::is_some)
                    })
            };
            out.push(Moved {
                view: view.clone(),
                item_kind: item_kind.clone(),
                item: item.clone(),
                spot: (!as_main).then(|| spot.clone()),
                beyond: !anyway,
            });
        }
        Ok(out)
    }

    /// One box the source moved, put in the target over its own. A group the source let go of goes
    /// from the target too: on a branch over main's group of that name, as a row with no cells.
    fn put_moved(&mut self, project: &str, target: &str, moved: &Moved) -> Result<(), String> {
        let gone = matches!(&moved.spot, Some(s) if moved.item_kind == "group" && s.x.is_none());
        let spot = if gone {
            let in_main = !target.is_empty()
                && !crate::store::is_own_tree(target)
                && self
                    .conn()
                    .query_row(
                        "SELECT 1 FROM places WHERE project_id = ? AND branch_id = '' AND view = ? AND item_kind = ? AND item = ? AND x IS NOT NULL",
                        params![project, moved.view, moved.item_kind, moved.item],
                        |_| Ok(()),
                    )
                    .is_ok();
            moved.spot.as_ref().filter(|_| in_main)
        } else {
            moved.spot.as_ref()
        };
        match spot {
            Some(s) => self.conn().execute(
                "INSERT INTO places (project_id, branch_id, view, item_kind, item, x, y, w, h, pinned, look) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
                 ON CONFLICT(project_id, branch_id, view, item_kind, item) DO UPDATE SET x = excluded.x, y = excluded.y, w = excluded.w, h = excluded.h, pinned = excluded.pinned, look = excluded.look",
                params![project, target, moved.view, moved.item_kind, moved.item, s.x, s.y, s.w, s.h, s.pinned as i64, s.look],
            ),
            None => self.conn().execute(
                "DELETE FROM places WHERE project_id = ? AND branch_id = ? AND view = ? AND item_kind = ? AND item = ?",
                params![project, target, moved.view, moved.item_kind, moved.item],
            ),
        }
        .map_err(db)?;
        Ok(())
    }

    /// The boxes the source moved, put in the target over its own (`places`); into main,
    /// the source lets go of its own that main now holds the same, as its documents went.
    fn place_moved(
        &mut self,
        work: &Work,
        project: &str,
        source: &str,
        target: &str,
    ) -> Result<(), String> {
        for moved in &work.moves {
            self.put_moved(project, target, moved)?;
        }
        if !work.moves.is_empty() {
            self.touch(project, target);
        }
        if target.is_empty() {
            let main = boxes_of(self.conn(), project, "")?;
            let mut let_go = false;
            for (key, spot) in boxes_of(self.conn(), project, source)? {
                if main.get(&key) != Some(&spot) {
                    continue;
                }
                let (view, item_kind, item) = key;
                self.conn()
                    .execute(
                        "DELETE FROM places WHERE project_id = ? AND branch_id = ? AND view = ? AND item_kind = ? AND item = ?",
                        params![project, source, view, item_kind, item],
                    )
                    .map_err(db)?;
                let_go = true;
            }
            if let_go {
                self.touch(project, source);
            }
        }
        Ok(())
    }

    /// Paths the target would hold two different things at: what it sees, with what the
    /// merge brings in — cards and sections, and the rows a card keeps by id that the merge brings
    /// to share one name (`rows_at_one_name`: both sides' versions would go in, at one path).
    fn overlaps(
        &self,
        project: &str,
        target: &str,
        merged: &[Merged],
        name: &dyn Fn(&str) -> String,
    ) -> Result<Vec<String>, String> {
        let mut found = Vec::new();
        // The cards as the target sees them before the merge: what was at one name there already
        // is not the merge's doing.
        let mut cards_before: HashMap<String, Map<String, Value>> = HashMap::new();
        for (table, section) in [(Table::Sections, true), (Table::Cards, false)] {
            let mut held: HashMap<String, Map<String, Value>> = HashMap::new();
            for (id, row) in doc_rows(self.conn(), table, project, "")? {
                if let Some(doc) = parse(row.doc.as_ref()) {
                    held.insert(id, doc);
                }
            }
            if !target.is_empty() {
                for (id, row) in doc_rows(self.conn(), table, project, target)? {
                    match (row.removed, parse(row.doc.as_ref())) {
                        (false, Some(doc)) => {
                            held.insert(id, doc);
                        }
                        _ => {
                            held.remove(&id);
                        }
                    }
                }
            }
            if !section {
                cards_before = held.clone();
            }
            for item in merged.iter().filter(|m| m.table == table) {
                if item.undecided {
                    held.remove(&item.id);
                    continue;
                }
                if !item.changes {
                    continue;
                }
                match &item.result {
                    Some(doc) => held.insert(item.id.clone(), doc.clone()),
                    None => held.remove(&item.id),
                };
            }
            let mut at: HashMap<String, Vec<String>> = HashMap::new();
            for (id, doc) in &held {
                let text = |key: &str| {
                    doc.get(key)
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string()
                };
                let key = if section {
                    format!(
                        "{}\u{1}{}",
                        text("parent_id"),
                        crate::pathtext::fold(&text("name"))
                    )
                } else {
                    // A card's name is its own where it is: its section, or its container — told
                    // apart as the database tells names apart (`card_docs_name`).
                    format!(
                        "{}\u{1}{}",
                        super::holder(doc),
                        crate::pathtext::fold(&text("name"))
                    )
                };
                at.entry(key).or_default().push(id.clone());
            }
            for mut ids in at.into_values().filter(|ids| ids.len() > 1) {
                let touched = ids
                    .iter()
                    .any(|id| merged.iter().any(|m| &m.id == id && m.changes));
                if !touched {
                    continue;
                }
                ids.sort();
                found.push(name(&ids[0]));
            }
        }
        for item in merged
            .iter()
            .filter(|m| m.table == Table::Cards && m.changes)
        {
            let Some(doc) = &item.result else {
                continue;
            };
            let before = cards_before.get(&item.id).map(rows_at_one_name);
            for (at, ids) in rows_at_one_name(doc) {
                let already = before.as_ref().is_some_and(|was| {
                    was.iter()
                        .any(|(p, then)| *p == at && ids.iter().all(|id| then.contains(id)))
                });
                if !already {
                    found.push(format!("{}.{at}", name(&item.id)));
                }
            }
        }
        found.sort();
        Ok(found)
    }

    /// The merge, written: refused while a conflict is left or a path would hold two
    /// things, and then nothing is written. One commit in the target; into main, one in
    /// the source too — it is main again. With `places`, every box the source moved goes
    /// in over the target's (`specboard://SpecBoard/Docs/Design:DesignBranches` §6); without, only where the target
    /// places a merged card nowhere. Boxes are how it looks: they go into a locked main.
    /// `keep`: a branch merged into main stays as it is rather than set aside (`set_aside`).
    pub fn merge(
        &mut self,
        project: &str,
        source: &str,
        target: &str,
        take: &Take,
        places: bool,
        keep: bool,
    ) -> Result<Preview, String> {
        if places && source.is_empty() {
            return Err(places_from_main());
        }
        let work = self.merge_work(project, source, target, take)?;
        if !work.preview.overlaps.is_empty() {
            return Err(text!(
                "write.merge.overlaps",
                paths = crate::text::list_comma(&work.preview.overlaps)
            ));
        }
        if !work.preview.conflicts.is_empty() {
            return Err(text!(
                "write.merge.conflicts",
                count = work.preview.conflicts.len(),
                units = crate::text::list_comma(
                    &work
                        .preview
                        .conflicts
                        .iter()
                        .map(|c| text!("write.merge.conflictAt", path = c.path, label = c.label))
                        .collect::<Vec<_>>()
                )
            ));
        }
        // Into main: main as it was, for what the board names of it (`follow_board`).
        let main_before = if target.is_empty() && !source.is_empty() {
            Some(crate::model::typed(&crate::detail::read(
                &self.reader(),
                project,
                None,
            )?)?)
        } else {
            None
        };
        // main's documents are written by a merge, under its lock too.
        self.merging();
        let mut writes: Vec<(&Merged, Write)> = Vec::new();
        for item in &work.merged {
            if !(item.changes || (source.is_empty() && !target.is_empty())) {
                continue;
            }
            let doc = item.result.as_ref();
            let main_now: Option<String> = self
                .conn()
                .query_row(
                    &format!(
                        "SELECT doc FROM {} WHERE project_id = ? AND branch_id = '' AND id = ?",
                        item.table.name()
                    ),
                    params![project, item.id],
                    |r| r.get(0),
                )
                .ok()
                .flatten();
            let write = if !target.is_empty() {
                let same_as_main = match (doc, &main_now) {
                    (Some(d), Some(m)) => parse(Some(m)).is_some_and(|m| text(&m) == text(d)),
                    _ => false,
                };
                if same_as_main {
                    Write::Clear
                } else if let Some(doc) = doc {
                    let base = if main_now.is_none() {
                        None
                    } else {
                        item.base.clone()
                    };
                    Write::Put(doc, base)
                } else if main_now.is_some() && item.base.is_some() {
                    Write::Mark
                } else {
                    Write::Clear
                }
            } else if item.changes {
                match doc {
                    Some(doc) => Write::Put(doc, None),
                    None => Write::Clear,
                }
            } else {
                continue;
            };
            writes.push((item, write));
        }
        // Out of the way first, so names that change hands — a card deleted and made again
        // with its name, two cards trading names — never meet on the way.
        for (item, _) in &writes {
            self.merge_delete(item.table, project, target, &item.id)?;
        }
        for (item, write) in writes {
            match write {
                Write::Clear => {}
                Write::Mark => {
                    self.merge_row(item, project, target, None, true, item.base.clone())?
                }
                Write::Put(doc, base) => {
                    self.merge_row(item, project, target, Some(doc), false, base)?
                }
            }
        }
        if places {
            self.place_moved(&work, project, source, target)?;
        } else {
            self.place_merged(&work, project, source, target)?;
        }
        // Into main, the source is set aside unless kept (an app's own, `~…`, is the app's to let go
        // of): kept as it was, for a revert of the merge to bring back as it was merged.
        let aside = !source.is_empty()
            && target.is_empty()
            && !keep
            && !work.preview.source.starts_with('~');
        if !source.is_empty() && target.is_empty() {
            // Into main and kept: the source is main again, and remembers main as it is now.
            if !aside {
                for table in [Table::Cards, Table::Sections] {
                    let ids: Vec<String> = doc_rows(self.conn(), table, project, source)?
                        .into_keys()
                        .collect();
                    for id in ids {
                        self.merge_delete(table, project, source, &id)?;
                    }
                }
                self.take_prints(source, project)?;
            }
            if let Some(before) = &main_before {
                crate::tools::follow::follow_board(
                    self,
                    project,
                    before,
                    &crate::tools::follow::InnerMoves::new(),
                )?;
            }
        } else if source.is_empty() && !target.is_empty() {
            // main taken in: the target remembers main as it is now.
            self.take_prints(target, project)?;
        }
        self.mark(project, target, "merge");
        // A locked main put right in one go (`begin_group`): said as that, not as its branch.
        let group = work.preview.source == super::branches::GROUP_BRANCH;
        let about = if source.is_empty() || group {
            project
        } else {
            source
        };
        let counts = work.preview.counts_keyed();
        let source_name = &work.preview.source;
        let placed = places && !(group && work.preview.places == 0);
        let said = match (group, placed) {
            (true, false) => keyed!("write.merge.line.group", counts),
            (true, true) => keyed!(
                "write.merge.line.groupPlaced",
                counts,
                places = work.preview.places
            ),
            (false, false) => keyed!("write.merge.line.merged", source = source_name, counts),
            (false, true) => keyed!(
                "write.merge.line.mergedPlaced",
                source = source_name,
                counts,
                places = work.preview.places
            ),
        };
        self.line(
            project,
            target,
            "branch.merge",
            "project",
            Some(about),
            said,
        )?;
        if !source.is_empty() && target.is_empty() {
            self.mark(project, source, "merge");
            self.line(
                project,
                source,
                "branch.merge",
                "project",
                Some(source),
                keyed!("write.merge.line.intoMain"),
            )?;
            // It sees main as it is now; what a revert of it brings back comes back; and it is
            // set aside unless kept (an app's own, `~…`, is the app's to let go of).
            self.thaw(project, source)?;
            self.bring_back(project, source)?;
            if aside {
                self.set_aside(project, source)?;
            }
        } else if source.is_empty() && !target.is_empty() {
            // main taken in: what the target kept of main as it was made, let go of.
            self.thaw(project, target)?;
        }
        Ok(work.preview)
    }

    /// Where the merged cards sit in the target: where it already places them, or where
    /// the source did when the target places them nowhere (`specboard://SpecBoard/Docs/Design:DesignBranches` §6).
    fn place_merged(
        &mut self,
        work: &Work,
        project: &str,
        source: &str,
        target: &str,
    ) -> Result<(), String> {
        if source.is_empty() {
            return Ok(());
        }
        // The groups on a diagram the merge brings go in with it (`place_moves`' `beyond`).
        let mut brought = false;
        for moved in work
            .moves
            .iter()
            .filter(|m| m.item_kind == "group" && !m.beyond)
        {
            self.put_moved(project, target, moved)?;
            brought = true;
        }
        if brought {
            self.touch(project, target);
        }
        let cards: HashSet<&String> = work
            .merged
            .iter()
            .filter(|m| m.table == Table::Cards && m.result.is_some() && m.changes)
            .map(|m| &m.id)
            .collect();
        if cards.is_empty() {
            return Ok(());
        }
        type Boxed = (
            String,
            String,
            String,
            Option<i64>,
            Option<i64>,
            Option<i64>,
            Option<i64>,
            i64,
        );
        let boxes: Vec<Boxed> = {
            let mut stmt = self
                .conn()
                .prepare("SELECT view, item_kind, item, x, y, w, h, pinned FROM places WHERE project_id = ? AND branch_id = ?")
                .map_err(db)?;
            stmt.query_map(params![project, source], |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                    r.get(6)?,
                    r.get(7)?,
                ))
            })
            .map_err(db)?
            .collect::<Result<_, _>>()
            .map_err(db)?
        };
        for (view, item_kind, item, x, y, w, h, pinned) in boxes {
            let card = match item_kind.as_str() {
                "card" => &item,
                // Where their nodes sit, the same way, node by node.
                "node" => &view,
                _ => continue,
            };
            if !cards.contains(card) {
                continue;
            }
            let held = self
                .conn()
                .query_row(
                    "SELECT 1 FROM places WHERE project_id = ? AND branch_id IN (?, '') AND view = ? AND item_kind = ? AND item = ?",
                    params![project, target, view, item_kind, item],
                    |_| Ok(()),
                )
                .is_ok();
            if held {
                continue;
            }
            self.conn()
                .execute(
                    "INSERT INTO places (project_id, branch_id, view, item_kind, item, x, y, w, h, pinned) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                    params![project, target, view, item_kind, item, x, y, w, h, pinned],
                )
                .map_err(db)?;
        }
        Ok(())
    }

    /// A row as a merge leaves it, its base and when it was taken as given: its key
    /// columns from the document, or from the base for a removed mark.
    fn merge_row(
        &mut self,
        item: &Merged,
        project: &str,
        tree: &str,
        doc: Option<&Map<String, Value>>,
        removed: bool,
        base: Option<(String, String)>,
    ) -> Result<(), String> {
        let (table, id) = (item.table, item.id.as_str());
        let shape = doc
            .cloned()
            .or_else(|| base.as_ref().and_then(|b| parse(Some(&b.0))))
            .unwrap_or_default();
        let keys = table.keys(&shape);
        if tree.is_empty() {
            self.freeze(table, project, id)?;
        }
        self.history.before(&self.tx, table, project, tree, id)?;
        let mut names: Vec<&str> = vec!["project_id", "branch_id", "id"];
        names.extend(keys.iter().map(|k| k.0));
        names.extend(["removed", "doc", "base", "based_at"]);
        let mut binds: Vec<rusqlite::types::Value> = vec![
            project.to_string().into(),
            tree.to_string().into(),
            id.to_string().into(),
        ];
        for (_, value) in &keys {
            binds.push(match value {
                Value::String(s) => s.clone().into(),
                _ => rusqlite::types::Value::Null,
            });
        }
        binds.push((removed as i64).into());
        binds.push(doc.map(|d| Value::Object(d.clone()).to_string()).into());
        binds.push(base.as_ref().map(|b| b.0.clone()).into());
        binds.push(base.as_ref().map(|b| b.1.clone()).into());
        let updates: Vec<String> = names[3..]
            .iter()
            .map(|c| format!("{c} = excluded.{c}"))
            .collect();
        self.tx
            .execute(
                &format!(
                    "INSERT INTO {} ({}) VALUES ({}) ON CONFLICT(project_id, branch_id, id) DO UPDATE SET {}",
                    table.name(),
                    names.join(", "),
                    vec!["?"; names.len()].join(", "),
                    updates.join(", ")
                ),
                rusqlite::params_from_iter(binds),
            )
            .map_err(|e| {
                if super::name_taken(&e) {
                    text!("write.merge.notWritten", path = item.path)
                } else {
                    db(e)
                }
            })?;
        self.touch(project, tree);
        Ok(())
    }

    fn merge_delete(
        &mut self,
        table: Table,
        project: &str,
        tree: &str,
        id: &str,
    ) -> Result<(), String> {
        if tree.is_empty() {
            self.freeze(table, project, id)?;
        }
        self.history.before(&self.tx, table, project, tree, id)?;
        self.tx
            .execute(
                &format!(
                    "DELETE FROM {} WHERE project_id = ? AND branch_id = ? AND id = ?",
                    table.name()
                ),
                params![project, tree, id],
            )
            .map_err(db)?;
        self.touch(project, tree);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn doc(v: Value) -> Map<String, Value> {
        v.as_object().unwrap().clone()
    }

    /// A merge of documents that are not data cards' (no shapes to read keys from).
    fn merge3(
        kind: Kind,
        ancestor: Option<&Map<String, Value>>,
        source: Option<&Map<String, Value>>,
        target: Option<&Map<String, Value>>,
        take: Option<&HashMap<String, Side>>,
    ) -> Outcome {
        super::merge3(kind, ancestor, source, target, take, None)
    }

    #[test]
    fn units_changed_apart_merge_and_alike_conflict() {
        let base = doc(
            json!({"id": "c", "kind": "entity", "name": "Order", "fields": {"label": "o"}, "rows": {"fields": [
            {"id": "f1", "fields": {"name": "total", "type": "int"}}]}}),
        );
        let mut source = base.clone();
        source["fields"]["description"] = json!("branch");
        source["fields"]["label"] = json!("branch の名");
        source["rows"]["fields"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id": "f2", "fields": {"name": "refunded", "type": "bool"}}));
        let mut target = base.clone();
        target["fields"]["label"] = json!("main の名");
        target["rows"]["fields"][0]["fields"]["type"] = json!("decimal");
        let out = merge3(Kind::Card, Some(&base), Some(&source), Some(&target), None);
        assert_eq!(out.conflicts.len(), 1);
        assert_eq!(out.conflicts[0].1, "label");
        let merged = out.result.unwrap();
        assert_eq!(merged["fields"]["description"], "branch");
        assert_eq!(merged["fields"]["label"], "main の名");
        let fields = merged["rows"]["fields"].as_array().unwrap();
        assert_eq!(fields.len(), 2);
        assert_eq!(fields[0]["fields"]["type"], "decimal");
        assert_eq!(fields[1]["fields"]["name"], "refunded");
        let take: HashMap<String, Side> = [("label".to_string(), Side::Source)].into();
        let out = merge3(
            Kind::Card,
            Some(&base),
            Some(&source),
            Some(&target),
            Some(&take),
        );
        assert!(out.conflicts.is_empty());
        assert_eq!(out.result.unwrap()["fields"]["label"], "branch の名");
    }

    #[test]
    fn a_removal_against_a_change_is_a_conflict_of_the_whole() {
        let base = doc(json!({"id": "c", "kind": "note", "name": "Memo", "fields": {"body": "a"}}));
        let mut target = base.clone();
        target["fields"]["body"] = json!("b");
        let out = merge3(Kind::Card, Some(&base), None, Some(&target), None);
        assert_eq!(out.conflicts[0].1, WHOLE);
        assert!(out.result.is_some() && !out.changes);
        let take: HashMap<String, Side> = [(WHOLE.to_string(), Side::Source)].into();
        let out = merge3(Kind::Card, Some(&base), None, Some(&target), Some(&take));
        assert!(out.result.is_none() && out.changes);
        // A request kept before named the whole by its Japanese name: still taken.
        let take: HashMap<String, Side> = [(
            keyed!("write.merge.wholeCard").render_in("ja"),
            Side::Source,
        )]
        .into();
        let out = merge3(Kind::Card, Some(&base), None, Some(&target), Some(&take));
        assert!(out.result.is_none() && out.changes);
    }

    #[test]
    fn siblings_keep_the_order_the_side_that_moved_them_gave() {
        let ids = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        // The source moved b before a; the target added c at the end.
        let got = ordered(
            &ids(&["a", "b", "c"]),
            &ids(&["a", "b"]),
            &ids(&["b", "a"]),
            &ids(&["a", "b", "c"]),
        );
        assert_eq!(got, ids(&["b", "a", "c"]));
        // The source added d after a; the target left them be.
        let got = ordered(
            &ids(&["a", "b", "d"]),
            &ids(&["a", "b"]),
            &ids(&["a", "d", "b"]),
            &ids(&["a", "b"]),
        );
        assert_eq!(got, ids(&["a", "d", "b"]));
    }

    #[test]
    fn a_nested_field_goes_back_under_its_parent() {
        let base = doc(
            json!({"id": "c", "kind": "value", "name": "Ship", "rows": {"fields": [
            {"id": "p", "fields": {"name": "address", "type": "object"}, "rows": {"fields": [
                {"id": "z", "fields": {"name": "zip"}}]}}]}}),
        );
        let mut source = base.clone();
        source["rows"]["fields"][0]["rows"]["fields"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id": "s", "fields": {"name": "street"}}));
        let mut target = base.clone();
        target.insert("fields".into(), json!({"label": "配送"}));
        let out = merge3(Kind::Card, Some(&base), Some(&source), Some(&target), None);
        let merged = out.result.unwrap();
        let inner = merged["rows"]["fields"][0]["rows"]["fields"]
            .as_array()
            .unwrap();
        assert_eq!(inner.len(), 2);
        assert_eq!(inner[1]["fields"]["name"], "street");
        assert_eq!(merged["fields"]["label"], "配送");
        assert!(inner.iter().all(|r| r.get(PARENT).is_none()));
    }

    /// A formatdoc as the store keeps it: `intro` holding `body`, and `terms`.
    fn spec() -> Map<String, Value> {
        doc(
            json!({"id": "c", "kind": "formatdoc", "name": "Spec", "rows": {"rows": [
            {"id": "i", "fields": {"name": "intro", "kind": "item", "heading": "はじめに"}, "rows": {"rows": [
                {"id": "b", "fields": {"name": "body", "kind": "text", "body": "前"}}]}},
            {"id": "t", "fields": {"name": "terms", "kind": "text", "body": "規約"}}]}}),
        )
    }

    fn take(said: &[(&str, Side)]) -> HashMap<String, Side> {
        said.iter().map(|(k, v)| (k.to_string(), *v)).collect()
    }

    #[test]
    fn formatdoc_rows_changed_apart_both_go_in_and_one_changed_alike_is_named_by_its_path() {
        let base = spec();
        let mut source = base.clone();
        source["rows"]["rows"][0]["rows"]["rows"][0]["fields"]["body"] = json!("branch");
        let mut target = base.clone();
        target["rows"]["rows"][1]["fields"]["body"] = json!("main");
        let out = merge3(Kind::Card, Some(&base), Some(&source), Some(&target), None);
        assert!(out.conflicts.is_empty());
        let merged = out.result.unwrap();
        assert_eq!(
            merged["rows"]["rows"][0]["rows"]["rows"][0]["fields"]["body"],
            "branch"
        );
        assert_eq!(merged["rows"]["rows"][1]["fields"]["body"], "main");
        target["rows"]["rows"][0]["rows"]["rows"][0]["fields"]["body"] = json!("main も");
        let out = merge3(Kind::Card, Some(&base), Some(&source), Some(&target), None);
        let said: Vec<(&str, &str)> = out
            .conflicts
            .iter()
            .map(|c| (c.0.as_str(), c.1.as_str()))
            .collect();
        assert_eq!(said, [("r:rows:b", "rows.intro.body")]);
        assert!(out.conflicts[0].2.as_ref().unwrap().get("rows").is_none());
        for chosen in ["rows.intro.body", "r:rows:b"] {
            let out = merge3(
                Kind::Card,
                Some(&base),
                Some(&source),
                Some(&target),
                Some(&take(&[(chosen, Side::Source)])),
            );
            assert!(out.conflicts.is_empty());
            let merged = out.result.unwrap();
            assert_eq!(
                merged["rows"]["rows"][0]["rows"]["rows"][0]["fields"]["body"],
                "branch"
            );
            assert_eq!(merged["rows"]["rows"][1]["fields"]["body"], "main");
        }
    }

    #[test]
    fn a_table_a_project_kind_names_rows_is_one_unit() {
        let mut base = spec();
        base["kind"] = json!("memo");
        let mut source = base.clone();
        source["rows"]["rows"][1]["fields"]["body"] = json!("branch");
        let mut target = base.clone();
        target["rows"]["rows"][0]["fields"]["heading"] = json!("main");
        let out = merge3(Kind::Card, Some(&base), Some(&source), Some(&target), None);
        let said: Vec<&str> = out.conflicts.iter().map(|c| c.0.as_str()).collect();
        assert_eq!(said, ["rows.rows"]);
    }

    #[test]
    fn a_row_added_inside_an_item_the_other_side_removed_is_a_conflict_of_the_item() {
        let base = spec();
        let mut source = base.clone();
        source["rows"]["rows"][0]["rows"]["rows"]
            .as_array_mut()
            .unwrap()
            .push(
                json!({"id": "n", "fields": {"name": "added", "kind": "text", "body": "足した"}}),
            );
        let mut target = base.clone();
        target["rows"]["rows"].as_array_mut().unwrap().remove(0);
        let out = merge3(Kind::Card, Some(&base), Some(&source), Some(&target), None);
        assert_eq!(out.conflicts.len(), 1, "{:?}", out.conflicts);
        let (unit, label, was, mine, theirs) = &out.conflicts[0];
        assert_eq!((unit.as_str(), label.as_str()), ("r:rows:i", "rows.intro"));
        assert!(theirs.is_none());
        assert_eq!(
            was.as_ref().unwrap()["rows"]["rows"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            mine.as_ref().unwrap()["rows"]["rows"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        // The keeper: the item and all it holds stay.
        let out = merge3(
            Kind::Card,
            Some(&base),
            Some(&source),
            Some(&target),
            Some(&take(&[("rows.intro", Side::Source)])),
        );
        assert!(out.conflicts.is_empty());
        let merged = out.result.unwrap();
        let names: Vec<&str> = merged["rows"]["rows"][0]["rows"]["rows"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["fields"]["name"].as_str().unwrap())
            .collect();
        assert_eq!(names, ["body", "added"]);
        assert_eq!(merged["rows"]["rows"][0]["fields"]["name"], "intro");
        // The remover: the item goes, and what was added inside it with it.
        let out = merge3(
            Kind::Card,
            Some(&base),
            Some(&source),
            Some(&target),
            Some(&take(&[("rows.intro", Side::Target)])),
        );
        assert!(out.conflicts.is_empty());
        let merged = out.result.unwrap();
        assert_eq!(merged["rows"]["rows"].as_array().unwrap().len(), 1);
        assert_eq!(merged["rows"]["rows"][0]["fields"]["name"], "terms");
    }

    #[test]
    fn an_item_removed_where_the_other_side_changed_a_row_inside_keeps_the_rest_when_kept() {
        let mut base = spec();
        base["rows"]["rows"][0]["rows"]["rows"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id": "s", "fields": {"name": "second", "kind": "text", "body": "二"}}));
        let mut source = base.clone();
        source["rows"]["rows"].as_array_mut().unwrap().remove(0);
        let mut target = base.clone();
        target["rows"]["rows"][0]["rows"]["rows"][0]["fields"]["body"] = json!("main");
        let out = merge3(Kind::Card, Some(&base), Some(&source), Some(&target), None);
        let said: Vec<&str> = out.conflicts.iter().map(|c| c.1.as_str()).collect();
        assert_eq!(said, ["rows.intro"]);
        let out = merge3(
            Kind::Card,
            Some(&base),
            Some(&source),
            Some(&target),
            Some(&take(&[("rows.intro", Side::Target)])),
        );
        let merged = out.result.unwrap();
        let inner = merged["rows"]["rows"][0]["rows"]["rows"]
            .as_array()
            .unwrap();
        assert_eq!(inner.len(), 2, "{merged:?}");
        assert_eq!(inner[0]["fields"]["body"], "main");
        assert_eq!(inner[1]["fields"]["name"], "second");
        // Removed where nothing inside changed, it goes without asking.
        let out = merge3(Kind::Card, Some(&base), Some(&source), Some(&base), None);
        assert!(out.conflicts.is_empty());
        assert_eq!(
            out.result.unwrap()["rows"]["rows"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn a_nested_field_added_under_an_object_the_other_side_removed_is_not_dropped() {
        let base = doc(
            json!({"id": "c", "kind": "value", "name": "Ship", "rows": {"fields": [
            {"id": "p", "fields": {"name": "address", "type": "object"}, "rows": {"fields": [
                {"id": "z", "fields": {"name": "zip"}}]}}]}}),
        );
        let mut source = base.clone();
        source["rows"]["fields"][0]["rows"]["fields"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id": "s", "fields": {"name": "street"}}));
        let mut target = base.clone();
        target["rows"]["fields"].as_array_mut().unwrap().clear();
        let out = merge3(Kind::Card, Some(&base), Some(&source), Some(&target), None);
        let said: Vec<&str> = out.conflicts.iter().map(|c| c.1.as_str()).collect();
        assert_eq!(said, ["fields.address"]);
    }

    #[test]
    fn rows_moved_under_one_another_on_two_sides_are_not_dropped() {
        let base = doc(
            json!({"id": "c", "kind": "formatdoc", "name": "Spec", "rows": {"rows": [
            {"id": "a", "fields": {"name": "a", "kind": "item"}},
            {"id": "b", "fields": {"name": "b", "kind": "item"}}]}}),
        );
        let row = |id: &str| json!({"id": id, "fields": {"name": id, "kind": "item"}});
        let under = |outer: &str, inner: &str| {
            let mut o = row(outer);
            o["rows"] = json!({"rows": [row(inner)]});
            doc(json!({"id": "c", "kind": "formatdoc", "name": "Spec", "rows": {"rows": [o]}}))
        };
        let out = merge3(
            Kind::Card,
            Some(&base),
            Some(&under("b", "a")),
            Some(&under("a", "b")),
            None,
        );
        assert!(out.conflicts.is_empty());
        let merged = out.result.unwrap();
        let mut names = Vec::new();
        fn walk(list: &[Value], out: &mut Vec<String>) {
            for r in list {
                out.push(r["fields"]["name"].as_str().unwrap().to_string());
                if let Some(inner) = r["rows"]["rows"].as_array() {
                    walk(inner, out);
                }
            }
        }
        walk(merged["rows"]["rows"].as_array().unwrap(), &mut names);
        names.sort();
        assert_eq!(names, ["a", "b"]);
    }

    #[test]
    fn versions_changed_apart_on_both_sides_are_named_one_by_one() {
        let base = doc(
            json!({"id": "c", "kind": "stackdoc", "name": "Log", "rows": {"versions": [
            {"id": "v2", "fields": {"name": "v2", "key": "2", "body": "二"}},
            {"id": "v1", "fields": {"name": "v1", "key": "1", "body": "一"}}]}}),
        );
        let mut source = base.clone();
        let mut target = base.clone();
        for i in 0..2 {
            source["rows"]["versions"][i]["fields"]["body"] = json!("branch");
            target["rows"]["versions"][i]["fields"]["body"] = json!("main");
        }
        let out = merge3(Kind::Card, Some(&base), Some(&source), Some(&target), None);
        let mut said: Vec<&str> = out.conflicts.iter().map(|c| c.1.as_str()).collect();
        said.sort();
        assert_eq!(said, ["versions.v1", "versions.v2"]);
        let out = merge3(
            Kind::Card,
            Some(&base),
            Some(&source),
            Some(&target),
            Some(&take(&[
                ("versions.v1", Side::Source),
                ("versions.v2", Side::Target),
            ])),
        );
        assert!(out.conflicts.is_empty());
        let merged = out.result.unwrap();
        assert_eq!(merged["rows"]["versions"][0]["fields"]["body"], "main");
        assert_eq!(merged["rows"]["versions"][1]["fields"]["body"], "branch");
    }

    #[test]
    fn labels_alike_are_told_apart_by_a_number() {
        let got = unique(vec![
            ("r:rows:y".into(), "rows.x".into()),
            ("r:rows:x".into(), "rows.x".into()),
            ("f:label".into(), "label".into()),
        ]);
        assert_eq!(got["r:rows:x"], "rows.x");
        assert_eq!(got["r:rows:y"], "rows.x#2");
        assert_eq!(got["f:label"], "label");
    }
}
