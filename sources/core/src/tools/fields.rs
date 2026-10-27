//! A record's fields written one at a time (`create_field`, `update_field`,
//! `delete_field` and `Repository`'s field writes in the Swift app), in the document's
//! rows: a field is a row of `fields`, an object's fields the rows under it.

use super::args::Reason;
use super::cards::{Resolver, Scope, identifier, nest_fields, text};
use super::walk::Target;
use super::{Seen, views::Projection};
use crate::export::Project;
use crate::write::Tx;
use serde_json::{Map, Value};
use std::collections::HashMap;

fn reason(at: Option<&str>, text: impl Into<String>) -> Vec<Reason> {
    vec![Reason {
        at: at.map(str::to_string),
        text: text.into(),
    }]
}

/// The rows of `fields` a field's names lead to: the card's own for none.
fn rows_at<'d>(doc: &'d mut Map<String, Value>, names: &[String]) -> Option<&'d mut Vec<Value>> {
    let mut list = doc
        .entry("rows")
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()?
        .entry("fields")
        .or_insert_with(|| Value::Array(Vec::new()))
        .as_array_mut()?;
    for name in names {
        let row = list
            .iter_mut()
            .find(|r| r["fields"]["name"].as_str() == Some(name.as_str()))?;
        let row = row.as_object_mut()?;
        list = row
            .entry("rows")
            .or_insert_with(|| Value::Object(Map::new()))
            .as_object_mut()?
            .entry("fields")
            .or_insert_with(|| Value::Array(Vec::new()))
            .as_array_mut()?;
    }
    Some(list)
}

/// Every field under a row, itself included.
fn count(row: &Value) -> usize {
    1 + row["rows"]["fields"]
        .as_array()
        .map_or(0, |a| a.iter().map(count).sum())
}

/// The kinds that hold fields, and what another is told instead.
fn holds_fields(card: &crate::model::Card) -> Result<(), String> {
    if matches!(card.kind.as_str(), "entity" | "value") {
        return Ok(());
    }
    let instead = match card.kind.as_str() {
        "note" | "freedoc" | "svg" | "html" => text!("fields.instead.body"),
        "formatdoc" => text!("fields.instead.rows"),
        "stackdoc" => text!("fields.instead.versions"),
        _ => text!("fields.instead.values"),
    };
    Err(text!(
        "fields.error.noFields",
        card = card.name,
        kind = card.kind,
        instead
    ))
}

/// Where a field not made yet would go: its card, the names to its parent, its name
/// (`MCPViews.parent`).
fn parent_of<'a>(
    project: &Project<'a>,
    path: &str,
) -> Result<(&'a crate::model::Card, Vec<String>, String), String> {
    let Some((parent, name)) = crate::pathtext::last_dot(path.trim())
        .filter(|(parent, name)| !parent.trim().is_empty() && !name.trim().is_empty())
    else {
        return Err(text!("fields.error.pathNotField", path));
    };
    let (parent, name) = (
        parent.to_string(),
        crate::pathtext::unquote(name).into_owned(),
    );
    match super::walk::locate(project, &parent, None)? {
        Target::FormatRow(..) => Err(text!("fields.error.parentIsFormatRow", parent)),
        Target::CustomRow(..) | Target::EnumValue(..) | Target::Step(..) => {
            Err(text!("fields.error.parentIsCustomRow", parent))
        }
        Target::Symbol(..) => Err(text!("code.symbol.readOnly", path = parent)),
        Target::Card(card) => Ok((card, Vec::new(), name)),
        // A field made through a card ghost goes in the card it leads to.
        Target::Ghost(ghost) => match project.ghost_card(ghost) {
            Some(card) => Ok((card, Vec::new(), name)),
            None => Err(project
                .directory
                .entry_of(&ghost.id)
                .map(|e| project.directory.dead_end(e, &project.full_ghost(ghost)))
                .unwrap_or_default()),
        },
        Target::Field(card, field, walked) => {
            if !field.is_object() {
                return Err(text!(
                    "fields.error.parentNotObject",
                    parent,
                    kind = field.kind
                ));
            }
            Ok((card, walked, name))
        }
    }
}

/// A field made at `path` (`create_field`): answered with its row as its parent lists it.
pub(crate) fn create_field(
    tx: &mut Tx,
    scope: &Scope,
    path: &str,
    value: &Map<String, Value>,
) -> Result<Value, Vec<Reason>> {
    let fail = |e: String| reason(None, e);
    let seen = Seen::in_tx(tx, scope).map_err(fail)?;
    let project = seen.project();
    let (card, parents, name) = parent_of(&project, path).map_err(fail)?;
    holds_fields(card).map_err(fail)?;
    let name = identifier(&name, &text!("fields.label.name")).map_err(fail)?;
    text(
        value.get("label").and_then(Value::as_str).unwrap_or(""),
        &text!("common.label.displayName"),
        120,
    )
    .map_err(fail)?;
    let mut docs = scope.docs(tx).map_err(fail)?;
    let mut doc = docs
        .remove(&card.id)
        .ok_or_else(|| fail(text!("update.error.noSchema")))?;
    let siblings =
        rows_at(&mut doc, &parents).ok_or_else(|| fail(text!("fields.error.noField")))?;
    if siblings
        .iter()
        .any(|r| r["fields"]["name"].as_str() == Some(name.as_str()))
    {
        return Err(fail(text!("fields.error.exists", name)));
    }
    let mut entry = value.clone();
    // As a field's dotted name is read: quoted where it needs it.
    entry.insert("name".into(), crate::pathtext::quote(&name).into());
    let label = project.full(card);
    let nested = nest_fields(&label, &[Value::Object(entry)])?;
    let made = HashMap::new();
    let resolver = Resolver {
        project: &project,
        base: project.section_of(card),
        made: &made,
        inside: None,
        unmade: None,
    };
    let kinds = crate::kinds::builtin();
    let def = kinds
        .get(&card.kind)
        .ok_or_else(|| fail(text!("fields.error.noDefinition", kind = card.kind)))?;
    let mut wire = Map::new();
    wire.insert("fields".into(), Value::Array(nested));
    let parts = crate::kinds::from_wire(def, &wire, None, &resolver, &mut crate::write::new_id)
        .map_err(|p| {
            p.into_iter()
                .map(|p| Reason {
                    at: None,
                    text: p.message.to_string(),
                })
                .collect::<Vec<_>>()
        })?;
    let row = parts.rows["fields"][0].clone();
    rows_at(&mut doc, &parents).unwrap().push(row);
    let problems = crate::kinds::check_card(kinds, &Value::Object(doc.clone()));
    if !problems.is_empty() {
        return Err(problems
            .into_iter()
            .map(|p| Reason {
                at: None,
                text: p.to_string(),
            })
            .collect());
    }
    super::update::require_new_rules(tx, scope, &doc).map_err(fail)?;
    tx.put_card(&scope.project, &scope.tree, &doc)
        .map_err(fail)?;
    tx.line(
        &scope.project,
        &scope.tree,
        "field.create",
        "field",
        None,
        keyed!("fields.line.added", card = card.name, name),
    )
    .map_err(fail)?;
    let changed = field_changes(value);
    if !changed.is_empty() {
        tx.line(
            &scope.project,
            &scope.tree,
            "field.update",
            "field",
            None,
            keyed!(
                "fields.line.updated",
                card = card.name,
                name,
                changed = super::slashed(changed)
            ),
        )
        .map_err(fail)?;
    }
    super::follow::store_in_full(tx, scope, std::slice::from_ref(&card.id)).map_err(fail)?;
    field_row(
        tx,
        scope,
        &format!("{label}.{}", {
            let mut names = parents.clone();
            names.push(name);
            crate::pathtext::join_names(names.iter().map(String::as_str))
        }),
    )
    .map_err(fail)
}

/// What an edit of a field is called in its history line (`updateField`'s `changed`).
fn field_changes(value: &Map<String, Value>) -> Vec<crate::text::Keyed> {
    let mut out = Vec::new();
    if value.contains_key("label") {
        out.push(keyed!("common.label.displayName"));
    }
    if let Some(t) = value.get("type").and_then(Value::as_str)
        && t != "string"
    {
        out.push(keyed!("fields.changed.type", fieldType = t));
    }
    for (key, label) in [
        ("array", keyed!("fields.changed.array")),
        ("required", keyed!("fields.changed.required")),
        ("nullable", keyed!("fields.changed.nullable")),
        ("unique", keyed!("fields.changed.unique")),
        ("primaryKey", keyed!("fields.changed.primaryKey")),
    ] {
        if value.contains_key(key) {
            out.push(label);
        }
    }
    for (key, label) in [
        ("description", keyed!("common.label.description")),
        ("const", keyed!("fields.changed.const")),
        ("rules", keyed!("fields.changed.rules")),
        ("variants", keyed!("fields.changed.variants")),
        ("also", keyed!("fields.changed.also")),
    ] {
        if value.contains_key(key) {
            out.push(label);
        }
    }
    out
}

/// The field written, as its parent lists it (`MCPViews.fieldRow`).
pub(crate) fn field_row(tx: &Tx, scope: &Scope, path: &str) -> Result<Value, String> {
    let seen = Seen::in_tx(tx, scope)?;
    let project = seen.project();
    let Target::Field(card, field, walked) = super::walk::locate(&project, path, None)? else {
        return Err(text!("fields.error.notField", path));
    };
    let full = walked
        .iter()
        .fold(project.full(card), |at, n| crate::pathtext::member(&at, n));
    let parent = crate::pathtext::last_dot(&full)
        .map(|(p, _)| p.to_string())
        .unwrap_or_default();
    let owner_at = (walked.len() == 1).then_some((card, field.name.as_str()));
    let mut out = Map::new();
    out.insert("project".into(), seen.summary.name.clone().into());
    out.insert("path".into(), full.into());
    out.insert("parent".into(), parent.clone().into());
    out.insert(
        "fields".into(),
        Value::Array(vec![seen.field_view(
            field,
            &parent,
            card,
            owner_at,
            &Projection::everything(),
        )]),
    );
    Ok(Value::Object(out))
}

/// A field changed (`update_field`): answered with its row, and what a rename rewrote.
pub(crate) fn update_field(
    tx: &mut Tx,
    scope: &Scope,
    path: &str,
    value: &Map<String, Value>,
) -> Result<Value, Vec<Reason>> {
    let fail = |e: String| reason(None, e);
    let before = Seen::in_tx(tx, scope).map_err(fail)?;
    let project = before.project();
    let Target::Field(card, field, walked) =
        super::walk::locate(&project, path, None).map_err(fail)?
    else {
        return Err(fail(text!("fields.error.notField", path)));
    };
    let card_id = card.id.clone();
    let card_name = card.name.clone();
    let old_name = field.name.clone();
    // An embedded field's type is the line's (`connections`, line embed).
    let embedded = field.is_ref();
    let mut docs = scope.docs(tx).map_err(fail)?;
    let mut doc = docs
        .remove(&card_id)
        .ok_or_else(|| fail(text!("fields.error.noField")))?;
    let parents = walked[..walked.len() - 1].to_vec();
    let mut changed: Vec<crate::text::Keyed> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();
    let mut renamed = None;
    {
        let siblings =
            rows_at(&mut doc, &parents).ok_or_else(|| fail(text!("fields.error.noField")))?;
        if let Some(raw) = value.get("name").and_then(Value::as_str) {
            let name = identifier(raw, &text!("fields.label.name")).map_err(fail)?;
            if name != old_name {
                if siblings
                    .iter()
                    .any(|r| r["fields"]["name"].as_str() == Some(name.as_str()))
                {
                    return Err(fail(text!("fields.error.exists", name)));
                }
                renamed = Some(name);
            }
        }
        let row = siblings
            .iter_mut()
            .find(|r| r["fields"]["name"].as_str() == Some(old_name.as_str()))
            .ok_or_else(|| fail(text!("fields.error.noField")))?;
        let children = row["rows"]["fields"].as_array().map_or(0, Vec::len);
        let row = row.as_object_mut().unwrap();
        let fields = row
            .entry("fields")
            .or_insert_with(|| Value::Object(Map::new()))
            .as_object_mut()
            .unwrap();
        if let Some(name) = &renamed {
            fields.insert("name".into(), name.clone().into());
            changed.push(keyed!("update.changed.name", name));
        }
        if let Some(label) = value.get("label").and_then(Value::as_str) {
            let label = text(label, &text!("common.label.displayName"), 120).map_err(fail)?;
            if label.is_empty() {
                fields.remove("label");
            } else {
                fields.insert("label".into(), label.into());
            }
            changed.push(keyed!("common.label.displayName"));
        }
        let was_type = fields
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("string")
            .to_string();
        if let Some(next_type) = value.get("type").and_then(Value::as_str) {
            if embedded {
                return Err(fail(text!("fields.error.embedded", name = old_name)));
            }
            if next_type != was_type {
                if was_type == "object" && children > 0 {
                    return Err(fail(text!("fields.error.nestedLeft", count = children)));
                }
                fields.insert("type".into(), next_type.into());
                changed.push(keyed!("fields.changed.type", fieldType = next_type));
            }
        }
        for (key, label) in [
            ("array", keyed!("fields.changed.array")),
            ("required", keyed!("fields.changed.required")),
            ("nullable", keyed!("fields.changed.nullable")),
            ("unique", keyed!("fields.changed.unique")),
            ("primaryKey", keyed!("fields.changed.primaryKey")),
        ] {
            if let Some(on) = value.get(key).and_then(Value::as_bool) {
                if on {
                    fields.insert(key.into(), true.into());
                } else {
                    fields.remove(key);
                }
                changed.push(label);
            }
        }
        if let Some(raw) = value.get("description").and_then(Value::as_str) {
            let said = text(raw, &text!("common.label.description"), 2000).map_err(fail)?;
            if said.is_empty() {
                fields.remove("description");
            } else {
                fields.insert("description".into(), said.into());
            }
            changed.push(keyed!("common.label.description"));
        }
        if let Some(fixed) = value.get("const") {
            if fixed.is_null() {
                fields.remove("const");
            } else {
                fields.insert("const".into(), fixed.clone());
            }
            changed.push(keyed!("fields.changed.const"));
        }
        for (key, label) in [
            ("rules", keyed!("fields.changed.rules")),
            ("variants", keyed!("fields.changed.variants")),
            ("also", keyed!("fields.changed.also")),
        ] {
            if let Some(v) = value.get(key) {
                if v.as_array().is_some_and(Vec::is_empty) {
                    fields.remove(key);
                } else {
                    fields.insert(key.into(), v.clone());
                }
                changed.push(label);
            }
        }
    }
    let problems = crate::kinds::check_card(crate::kinds::builtin(), &Value::Object(doc.clone()));
    if !problems.is_empty() {
        return Err(problems
            .into_iter()
            .map(|p| Reason {
                at: None,
                text: p.to_string(),
            })
            .collect());
    }
    super::update::require_new_rules(tx, scope, &doc).map_err(fail)?;
    if changed.is_empty() {
        return field_row(tx, scope, path).map_err(fail);
    }
    tx.put_card(&scope.project, &scope.tree, &doc)
        .map_err(fail)?;
    if renamed.is_some() {
        let rewritten =
            super::follow::follow(tx, scope, &before.was(), &Default::default()).map_err(fail)?;
        if rewritten > 0 {
            changed.push(keyed!("update.changed.paths", rewritten));
            warnings.push(text!("fields.warn.pathsRewritten", rewritten));
        }
    }
    super::follow::store_in_full(tx, scope, std::slice::from_ref(&card_id)).map_err(fail)?;
    tx.line(
        &scope.project,
        &scope.tree,
        "field.update",
        "field",
        None,
        keyed!(
            "fields.line.updated",
            card = card_name,
            name = old_name,
            changed = super::slashed(changed)
        ),
    )
    .map_err(fail)?;
    let mut names = parents.clone();
    names.push(renamed.unwrap_or(old_name));
    let now_path = format!(
        "{}.{}",
        project.full(card),
        crate::pathtext::join_names(names.iter().map(String::as_str))
    );
    let row = field_row(tx, scope, &now_path).map_err(fail)?;
    Ok(super::warned(row, &warnings))
}

/// A field let go (`delete_field`), and what is inside it.
pub(crate) fn delete_field(tx: &mut Tx, scope: &Scope, path: &str) -> Result<Value, Vec<Reason>> {
    let fail = |e: String| reason(None, e);
    let seen = Seen::in_tx(tx, scope).map_err(fail)?;
    let project = seen.project();
    let Target::Field(card, field, walked) =
        super::walk::locate(&project, path, None).map_err(fail)?
    else {
        return Err(fail(text!("fields.error.notField", path)));
    };
    let full = walked
        .iter()
        .fold(project.full(card), |at, n| crate::pathtext::member(&at, n));
    let card_id = card.id.clone();
    let line = keyed!("fields.line.deleted", card = card.name, name = field.name);
    let name = field.name.clone();
    let mut docs = scope.docs(tx).map_err(fail)?;
    let mut doc = docs
        .remove(&card_id)
        .ok_or_else(|| fail(text!("fields.error.noField")))?;
    let siblings = rows_at(&mut doc, &walked[..walked.len() - 1])
        .ok_or_else(|| fail(text!("fields.error.noField")))?;
    let at = siblings
        .iter()
        .position(|r| r["fields"]["name"].as_str() == Some(name.as_str()))
        .ok_or_else(|| fail(text!("fields.error.noField")))?;
    let gone = siblings.remove(at);
    // The lines that left it go with it, and a face's name for it.
    super::lines::drop_from(
        &mut doc,
        &crate::pathtext::join_names(walked.iter().map(String::as_str)),
    );
    if walked.len() == 1 {
        super::faces::pruned(&mut doc);
    }
    tx.put_card(&scope.project, &scope.tree, &doc)
        .map_err(fail)?;
    tx.line(
        &scope.project,
        &scope.tree,
        "field.delete",
        "field",
        None,
        line,
    )
    .map_err(fail)?;
    let mut out = Map::new();
    out.insert("deleted".into(), full.into());
    out.insert("fieldCount".into(), count(&gone).into());
    Ok(Value::Object(out))
}
