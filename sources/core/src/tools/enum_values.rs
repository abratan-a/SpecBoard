//! An enum's values one at a time, by their paths (`Schemas:Status.shipped`): added, renamed,
//! relabelled or described, moved among the others, and let go of. Each is the enum's `values` as MCP writes
//! them, changed, and written back whole through the card's edit — checked as `values` is — so a
//! value renamed takes along the paths that name it. A rename also writes the data cards' values
//! that hold it; a value let go of leaves them as they are, and says where they are.

use super::Seen;
use super::cards::{Scope, text};
use super::inner::{card_holding, wire_list};
use super::update::{edit_card, put_checked};
use crate::write::Tx;
use serde_json::{Map, Value, json};

/// The most data paths a warning names, then a count.
const MOST_NAMED: usize = 12;

/// A value's name as written: trimmed, not empty, 80 characters at most.
fn value_name(raw: &str) -> Result<String, String> {
    let name = raw.trim().to_string();
    if name.is_empty() {
        return Err(text!("update.error.valueEmpty"));
    }
    if name.chars().count() > 80 {
        return Err(text!("update.error.valueTooLong"));
    }
    Ok(name)
}

/// The value's name in a wire entry (`{name, label?}`).
fn named(entry: &Value) -> Option<&str> {
    entry.get("name").and_then(Value::as_str)
}

/// A card's `values` as MCP writes them (`[{name, label?}]`, a bare text a name alone) made the
/// entries the rest of the core reads (`[{name, label?}]`).
pub(crate) fn wired(values: &Value) -> Value {
    let Value::Array(items) = values else {
        return values.clone();
    };
    Value::Array(
        items
            .iter()
            .map(|item| match item {
                Value::String(name) => {
                    let mut entry = Map::new();
                    entry.insert("name".into(), name.clone().into());
                    Value::Object(entry)
                }
                other => other.clone(),
            })
            .collect(),
    )
}

/// A card written with `values` named by `value` (before they were named by `name`): refused with
/// the entries written the way they are now.
pub(crate) fn refuse_old(card: &Map<String, Value>) -> Result<(), String> {
    let Some(Value::Array(items)) = card.get("values") else {
        return Ok(());
    };
    let old = |item: &Value| item.as_object().is_some_and(|o| o.contains_key("value"));
    if !items.iter().any(old) {
        return Ok(());
    }
    let now: Vec<Value> = items
        .iter()
        .map(|item| match item {
            Value::Object(o) if old(item) && !o.contains_key("name") => {
                let mut entry = Map::new();
                for (key, v) in o {
                    entry.insert(if key == "value" { "name" } else { key }.into(), v.clone());
                }
                Value::Object(entry)
            }
            other => other.clone(),
        })
        .collect();
    Err(crate::mcp::refusal(
        text!("enumValues.error.valueKey"),
        json!({ "fix": [{ "at": "/values", "to": now }] }),
    ))
}

/// A label as written (a string, or null for none): trimmed, 120 characters at most.
fn label(given: &Value) -> Result<Option<String>, String> {
    match given {
        Value::Null => Ok(None),
        Value::String(raw) => {
            let said = text(raw, &text!("common.label.displayName"), 120)?;
            Ok((!said.is_empty()).then_some(said))
        }
        _ => Err(text!("enumValues.error.labelShape")),
    }
}

/// A description as written (a string, or null for none): trimmed, one line, 500 characters at
/// most. An empty one is none.
pub(crate) fn description(given: &Value) -> Result<Option<String>, String> {
    match given {
        Value::Null => Ok(None),
        Value::String(raw) => {
            let said = text(raw, &text!("common.label.description"), 500)?;
            if said.contains(['\n', '\r']) {
                return Err(text!("kindCheck.value.lineBreak"));
            }
            Ok((!said.is_empty()).then_some(said))
        }
        _ => Err(text!("enumValues.error.descriptionShape")),
    }
}

/// Where `position` puts a value among `count` others: from 0, past the end at the end.
fn place(given: &Value, count: usize) -> Result<usize, String> {
    given
        .as_i64()
        .filter(|p| *p >= 0)
        .map(|p| (p as usize).min(count))
        .ok_or_else(|| text!("enumValues.error.position"))
}

/// The values written back whole, the paths that name a renamed one following it (`moves`), with
/// a line of the history that says what changed.
fn write_back(
    tx: &mut Tx,
    scope: &Scope,
    id: &str,
    list: Vec<Value>,
    moves: &[(Vec<String>, Vec<String>)],
    said: crate::text::Keyed,
) -> Result<Vec<String>, String> {
    let mut value = Map::new();
    value.insert("values".into(), Value::Array(list));
    let warnings = edit_card(tx, scope, id, &value, moves)?;
    tx.line(
        &scope.project,
        &scope.tree,
        "enumValue.set",
        "enumValue",
        Some(id),
        said,
    )?;
    Ok(warnings)
}

/// The value at `name` as `card_read` reads it, with what was said.
fn answer(tx: &Tx, scope: &Scope, id: &str, name: &str, said: &[String]) -> Result<Value, String> {
    let seen = Seen::in_tx(tx, scope)?;
    let project = seen.project();
    let card = project
        .card(id)
        .ok_or_else(|| text!("inner.error.unreadable"))?;
    Ok(super::warned(seen.enum_value_view(card, name), said))
}

/// A value added where its path says (`Schemas:Status.cancelled`): at the end, or at `position`.
pub(crate) fn create(
    tx: &mut Tx,
    scope: &Scope,
    path: &str,
    value: &Map<String, Value>,
) -> Result<Value, String> {
    let (id, full, name) = {
        let seen = Seen::in_tx(tx, scope)?;
        let project = seen.project();
        let card = card_holding(&project, path)
            .filter(|c| c.kind == "enum")
            .ok_or_else(|| text!("enumValues.error.noEnum", path))?;
        let names = crate::export::paths::WrittenPath::new(path)
            .map(|w| w.names)
            .unwrap_or_default();
        let [name] = names.as_slice() else {
            return Err(text!("enumValues.error.path", path));
        };
        (
            card.id.clone(),
            project.full(card),
            value_name(&crate::pathtext::unquote(name))?,
        )
    };
    if let Some(given) = value.get("name")
        && given.as_str().map(str::trim) != Some(name.as_str())
    {
        return Err(text!("enumValues.error.nameFromPath", name));
    }
    let mut list = wire_list(tx, scope, &id, "values")?;
    if list.iter().any(|v| named(v) == Some(name.as_str())) {
        return Err(text!("update.error.valueDuplicate", value = name));
    }
    let mut entry = Map::new();
    entry.insert("name".into(), name.clone().into());
    if let Some(said) = value.get("label").map(label).transpose()?.flatten() {
        entry.insert("label".into(), said.into());
    }
    if let Some(said) = value
        .get("description")
        .map(description)
        .transpose()?
        .flatten()
    {
        entry.insert("description".into(), said.into());
    }
    let at = match value.get("position") {
        Some(given) => place(given, list.len())?,
        None => list.len(),
    };
    list.insert(at, Value::Object(entry));
    let said = write_back(
        tx,
        scope,
        &id,
        list,
        &[],
        keyed!("enumValues.line.added", full, name = name.clone()),
    )?;
    answer(tx, scope, &id, &name, &said)
}

/// What the value says, over what the value was: a new name renames it — the paths that name it
/// and the data cards' values that hold it follow —, a label, a description, a place among the
/// others.
pub(crate) fn update(
    tx: &mut Tx,
    scope: &Scope,
    id: &str,
    old: &str,
    value: &Map<String, Value>,
) -> Result<Value, String> {
    if value.is_empty() {
        return Err(text!("inner.error.nothingToChange"));
    }
    let full = {
        let seen = Seen::in_tx(tx, scope)?;
        let project = seen.project();
        project
            .card(id)
            .map(|c| project.full(c))
            .unwrap_or_default()
    };
    let mut list = wire_list(tx, scope, id, "values")?;
    let from = list
        .iter()
        .position(|v| named(v) == Some(old))
        .ok_or_else(|| text!("enumValues.error.missing", name = old, full))?;
    let renamed = match value.get("name") {
        None => None,
        Some(Value::String(raw)) => Some(value_name(raw)?).filter(|n| n != old),
        Some(_) => return Err(text!("enumValues.error.nameShape")),
    };
    if let Some(name) = &renamed
        && list.iter().any(|v| named(v) == Some(name.as_str()))
    {
        return Err(text!("update.error.valueDuplicate", value = name));
    }
    let entry = list[from]
        .as_object_mut()
        .ok_or_else(|| text!("inner.error.unreadable"))?;
    if let Some(name) = &renamed {
        entry.insert("name".into(), name.clone().into());
    }
    if let Some(given) = value.get("label") {
        match label(given)? {
            Some(said) => {
                entry.insert("label".into(), said.into());
            }
            None => {
                entry.shift_remove("label");
            }
        }
    }
    if let Some(given) = value.get("description") {
        match description(given)? {
            Some(said) => {
                entry.insert("description".into(), said.into());
            }
            None => {
                entry.shift_remove("description");
            }
        }
    }
    if let Some(given) = value.get("position") {
        let moved = list.remove(from);
        let to = place(given, list.len())?;
        list.insert(to, moved);
    }
    let now = renamed.clone().unwrap_or_else(|| old.to_string());
    let moves: Vec<(Vec<String>, Vec<String>)> = renamed
        .iter()
        .map(|n| (vec![old.to_string()], vec![n.clone()]))
        .collect();
    let line = match &renamed {
        Some(name) => keyed!(
            "enumValues.line.renamed",
            full = full.clone(),
            name = old,
            now = name.clone()
        ),
        None => keyed!("enumValues.line.changed", full = full.clone(), name = old),
    };
    let said = write_back(tx, scope, id, list, &moves, line)?;
    let rewritten = match &renamed {
        Some(name) => {
            faces_renamed(tx, scope, id, old, name)?;
            data_renamed(tx, scope, id, old, name)?
        }
        None => Vec::new(),
    };
    let mut out = answer(tx, scope, id, &now, &said)?;
    if !rewritten.is_empty()
        && let Value::Object(body) = &mut out
    {
        body.insert(
            "dataRewritten".into(),
            Value::Array(rewritten.into_iter().map(Value::from).collect()),
        );
    }
    Ok(out)
}

/// The shapes whose face marks by a field of the enum `id`, their mark's value `old` named `now`.
fn faces_renamed(tx: &mut Tx, scope: &Scope, id: &str, old: &str, now: &str) -> Result<(), String> {
    let docs: Vec<Map<String, Value>> = {
        let seen = Seen::in_tx(tx, scope)?;
        let shapes = super::faces::marking_enum(&seen.project(), id);
        let mut docs = scope.docs(tx)?;
        shapes
            .iter()
            .filter_map(|s| docs.remove(s))
            .filter_map(|mut doc| super::faces::value_renamed(&mut doc, old, now).then_some(doc))
            .collect()
    };
    for doc in docs {
        put_checked(tx, scope, &doc)?;
    }
    Ok(())
}

/// The data cards' values that hold the value `old` of the enum `id`, written `now`: the full paths
/// of those written.
fn data_renamed(
    tx: &mut Tx,
    scope: &Scope,
    id: &str,
    old: &str,
    now: &str,
) -> Result<Vec<String>, String> {
    let (written, full) = {
        let seen = Seen::in_tx(tx, scope)?;
        let project = seen.project();
        let full = project
            .card(id)
            .map(|c| crate::pathtext::member(&project.full(c), now))
            .unwrap_or_default();
        let mut docs = scope.docs(tx)?;
        let mut written: Vec<(Map<String, Value>, String, Vec<String>)> = Vec::new();
        for (card_id, paths) in super::instances::holding_enum_value(&project, id, old) {
            let (Some(card), Some(mut doc)) = (project.card(&card_id), docs.remove(&card_id))
            else {
                continue;
            };
            let Some(shape) = super::instances::shape(&project, card) else {
                continue;
            };
            let Some(data) = doc.get_mut("fields").and_then(|f| f.get_mut("data")) else {
                continue;
            };
            super::instances::enum_values_mut(&project, shape, data, id, &mut |_, value| {
                if value.as_str() == Some(old) {
                    *value = Value::from(now);
                }
            });
            written.push((doc, project.full(card), paths));
        }
        (written, full)
    };
    let mut out = Vec::new();
    for (doc, card, paths) in written {
        put_checked(tx, scope, &doc)?;
        let card_id = doc["id"].as_str().unwrap_or_default().to_string();
        tx.line(
            &scope.project,
            &scope.tree,
            "schema.update",
            "schema",
            Some(&card_id),
            keyed!(
                "enumValues.line.dataFollowed",
                card,
                value = full.clone(),
                count = paths.len()
            ),
        )?;
        out.extend(paths);
    }
    Ok(out)
}

/// A value let go of. The data cards' values that hold it stay as they are, and the answer says
/// where they are.
pub(crate) fn delete(tx: &mut Tx, scope: &Scope, id: &str, name: &str) -> Result<Value, String> {
    let (full, open) = {
        let seen = Seen::in_tx(tx, scope)?;
        let project = seen.project();
        let card = project
            .card(id)
            .ok_or_else(|| text!("inner.error.unreadable"))?;
        (project.full(card), card.enum_open)
    };
    let mut list = wire_list(tx, scope, id, "values")?;
    let before = list.len();
    list.retain(|v| named(v) != Some(name));
    if list.len() == before {
        return Err(text!("enumValues.error.missing", name, full = full.clone()));
    }
    let count = list.len();
    let mut said = write_back(
        tx,
        scope,
        id,
        list,
        &[],
        keyed!("enumValues.line.removed", full = full.clone(), name),
    )?;
    let held: Vec<String> = {
        let seen = Seen::in_tx(tx, scope)?;
        super::instances::holding_enum_value(&seen.project(), id, name)
            .into_iter()
            .flat_map(|(_, paths)| paths)
            .collect()
    };
    if !held.is_empty() {
        let paths = super::capped(&held, MOST_NAMED);
        said.push(if open {
            text!("enumValues.warn.heldOpen", paths, name)
        } else {
            text!("enumValues.warn.held", paths, name)
        });
    }
    let mut out = Map::new();
    out.insert("project".into(), scope.project_name.clone().into());
    out.insert(
        "deleted".into(),
        crate::pathtext::member(&full, name).into(),
    );
    out.insert("valueCount".into(), count.into());
    Ok(super::warned(Value::Object(out), &said))
}
