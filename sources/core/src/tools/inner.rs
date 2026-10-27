//! What is inside a card written one at a time: a formatdoc's rows (`createRowAt`・
//! `updateRowAt`・`deleteRowAt` in the Swift app's surface). Each is the card's list as MCP
//! writes it, changed, and written back whole through the card's edit — checked as whole as
//! `rows` is — with what moved said, so the paths into it follow. A flow's nodes are cards
//! inside it now (`containers`), written as cards are.

use super::Seen;
use super::cards::{Resolver, Scope};
use super::update::edit_card;
use super::views::Projection;
use super::walk::locate;
use crate::write::Tx;
use serde_json::{Map, Value};
use std::collections::HashMap;

/// The card a path starts in, when there is one (`cardHolding`).
pub(crate) fn card_holding<'a>(
    project: &crate::export::Project<'a>,
    path: &str,
) -> Option<&'a crate::model::Card> {
    let written = crate::export::paths::WrittenPath::new(path)?;
    project.find(&written.card, &project.directory.home)
}

/// A card's list (`rows` or `nodes`) as MCP writes it.
pub(crate) fn wire_list(tx: &Tx, scope: &Scope, id: &str, key: &str) -> Result<Vec<Value>, String> {
    let seen = Seen::in_tx(tx, scope)?;
    let project = seen.project();
    let card = project
        .card(id)
        .ok_or_else(|| text!("containers.card.missing"))?;
    let doc = scope
        .docs(tx)?
        .remove(id)
        .ok_or_else(|| text!("containers.card.missing"))?;
    let def = project
        .defs
        .cards()
        .get(&card.kind)
        .ok_or_else(|| text!("inner.error.noDefinition", kind = card.kind))?;
    let resolver = Resolver {
        project: &project,
        base: project.section_of(card),
        made: &HashMap::new(),
        inside: None,
        unmade: None,
    };
    let wire = crate::kinds::to_wire(def, &Value::Object(doc), &resolver);
    Ok(wire
        .get(key)
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default())
}

/// The list written back, and what it is answered with: the row or node at `reading`,
/// or the card.
fn write_back(
    tx: &mut Tx,
    scope: &Scope,
    id: &str,
    key: &str,
    list: Vec<Value>,
    moves: &[(Vec<String>, Vec<String>)],
    reading: Option<&[String]>,
) -> Result<Value, String> {
    let mut value = Map::new();
    value.insert(key.to_string(), Value::Array(list));
    let said = edit_card(tx, scope, id, &value, moves)?;
    let seen = Seen::in_tx(tx, scope)?;
    let project = seen.project();
    let card = project
        .card(id)
        .ok_or_else(|| text!("inner.error.unreadable"))?;
    let view = match reading {
        Some(names) => seen.row_view(card, names),
        None => seen.card_view(card, &Projection::none(), None),
    };
    Ok(super::warned(view, &said))
}

/// Rows' names one after another, as a path writes them.
fn joined(names: &[String]) -> String {
    crate::pathtext::join_names(names.iter().map(String::as_str))
}

fn row_name(row: &Value) -> Option<&str> {
    row.get("name").and_then(Value::as_str)
}

/// The rows directly under `parent` (the doc's own for none).
fn children_mut<'l>(rows: &'l mut Vec<Value>, parent: &[String]) -> Option<&'l mut Vec<Value>> {
    let mut list = rows;
    for name in parent {
        let row = list
            .iter_mut()
            .find(|r| row_name(r) == Some(name.as_str()))?
            .as_object_mut()?;
        list = row
            .entry("rows")
            .or_insert_with(|| Value::Array(Vec::new()))
            .as_array_mut()?;
    }
    Some(list)
}

/// The kind of the row at `names` in the formatdoc `id`, as it is written now; none when there is none.
pub(crate) fn row_kind(tx: &Tx, scope: &Scope, id: &str, names: &[String]) -> Option<String> {
    let rows = wire_list(tx, scope, id, "rows").ok()?;
    row_at(&rows, names)?
        .get("kind")
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn row_at<'l>(rows: &'l [Value], names: &[String]) -> Option<&'l Value> {
    let (last, parent) = names.split_last()?;
    let mut list = rows;
    for name in parent {
        list = list
            .iter()
            .find(|r| row_name(r) == Some(name.as_str()))?
            .get("rows")?
            .as_array()?;
    }
    list.iter().find(|r| row_name(r) == Some(last.as_str()))
}

fn kind_of(row: &Value) -> &str {
    row.get("kind").and_then(Value::as_str).unwrap_or("item")
}

/// Where a row is put among the rows beside it: before or after the one named (a sibling's
/// name, so that rows another adds meanwhile do not move it elsewhere).
struct Beside {
    after: bool,
    name: String,
}

/// Where `value` puts its row (`before` or `after`), taken out of what the row is written
/// with; none when it says neither.
fn beside(value: &mut Map<String, Value>) -> Result<Option<Beside>, String> {
    let (before, after) = (value.shift_remove("before"), value.shift_remove("after"));
    let (key, name) = match (before, after) {
        (None, None) => return Ok(None),
        (Some(_), Some(_)) => {
            return Err(text!("inner.error.beforeAfter"));
        }
        (Some(name), None) => ("before", name),
        (None, Some(name)) => ("after", name),
    };
    let name = name
        .as_str()
        .filter(|n| !n.is_empty())
        .ok_or_else(|| text!("inner.error.besideName", key))?;
    Ok(Some(Beside {
        after: key == "after",
        name: name.to_string(),
    }))
}

/// `row` put among `siblings` before or after the one `beside` names: one of them, not itself.
fn put_beside(siblings: &mut Vec<Value>, row: Value, beside: &Beside) -> Result<(), String> {
    let key = if beside.after { "after" } else { "before" };
    if row_name(&row) == Some(beside.name.as_str()) {
        return Err(text!("inner.error.besideSelf", key, name = beside.name));
    }
    let Some(at) = siblings
        .iter()
        .position(|r| row_name(r) == Some(beside.name.as_str()))
    else {
        let names: Vec<&str> = siblings
            .iter()
            .filter_map(row_name)
            .filter(|n| Some(*n) != row_name(&row))
            .collect();
        return Err(text!(
            "inner.error.besideMissing",
            key,
            name = beside.name,
            names = if names.is_empty() {
                text!("inner.part.none")
            } else {
                crate::text::list(&names)
            }
        ));
    };
    siblings.insert(if beside.after { at + 1 } else { at }, row);
    Ok(())
}

/// The strings of a row's key: a table's headings.
fn strings(row: &Map<String, Value>, key: &str) -> Vec<String> {
    row.get(key)
        .and_then(Value::as_array)
        .map(|l| {
            l.iter()
                .map(|v| v.as_str().unwrap_or_default().to_string())
                .collect()
        })
        .unwrap_or_default()
}

/// A table's cells: a row of strings each.
fn table_cells(row: &Map<String, Value>) -> Vec<Vec<String>> {
    row.get("cells")
        .and_then(Value::as_array)
        .map(|rows| {
            rows.iter()
                .map(|r| {
                    r.as_array()
                        .map(|l| {
                            l.iter()
                                .map(|v| v.as_str().unwrap_or_default().to_string())
                                .collect()
                        })
                        .unwrap_or_default()
                })
                .collect()
        })
        .unwrap_or_default()
}

// MARK: - Rows of a formatdoc

/// A row added where its path says (`createRowAt`).
pub(crate) fn create_row(
    tx: &mut Tx,
    scope: &Scope,
    path: &str,
    value: &Map<String, Value>,
) -> Result<Value, String> {
    let seen = Seen::in_tx(tx, scope)?;
    let project = seen.project();
    let doc = card_holding(&project, path)
        .filter(|c| c.kind == "formatdoc")
        .ok_or_else(|| text!("inner.error.noFormatdoc", path))?;
    let (id, full) = (doc.id.clone(), project.full(doc));
    let mut value = value.clone();
    let beside = beside(&mut value)?;
    let rest = crate::pathtext::split_section(path).1;
    let names: Vec<String> = crate::pathtext::names(rest).into_iter().skip(1).collect();
    let (name, parent) = names
        .split_last()
        .ok_or_else(|| text!("inner.error.rowNameMissing"))?;
    let mut rows = wire_list(tx, scope, &id, "rows")?;
    if !parent.is_empty() {
        let Some(above) = row_at(&rows, parent) else {
            let at = parent
                .iter()
                .fold(full, |at, name| crate::pathtext::member(&at, name));
            locate(&project, &at, None)?;
            return Err(text!("inner.error.rowNotFound", path = joined(parent)));
        };
        if kind_of(above) != "item" {
            return Err(text!(
                "inner.error.notAnItem",
                path = joined(parent),
                kind = kind_of(above)
            ));
        }
    }
    let siblings =
        children_mut(&mut rows, parent).ok_or_else(|| text!("inner.error.rowMissing"))?;
    if siblings.iter().any(|r| row_name(r) == Some(name.as_str())) {
        return Err(text!("inner.error.exists", path));
    }
    if let Some(given) = value.get("name").and_then(Value::as_str)
        && given != name
    {
        return Err(text!("inner.error.rowNameFromPath", name));
    }
    let mut row = value;
    row.insert("name".into(), name.clone().into());
    match &beside {
        Some(beside) => put_beside(siblings, Value::Object(row), beside)?,
        None => siblings.push(Value::Object(row)),
    }
    write_back(tx, scope, &id, "rows", rows, &[], Some(&names))
}

/// What the value says, over what the row was — its kind too, which then holds only what
/// that kind holds; a new name renames it, and the paths that name it (`updateRowAt`).
pub(crate) fn update_row(
    tx: &mut Tx,
    scope: &Scope,
    id: &str,
    names: &[String],
    value: &Map<String, Value>,
) -> Result<Value, String> {
    let mut rows = wire_list(tx, scope, id, "rows")?;
    let (old, parent) = names
        .split_last()
        .ok_or_else(|| text!("inner.error.rowNameMissing"))?;
    let siblings = children_mut(&mut rows, parent)
        .filter(|s| s.iter().any(|r| row_name(r) == Some(old.as_str())))
        .ok_or_else(|| text!("inner.error.rowNotFound", path = joined(names)))?;
    if value.is_empty() {
        return Err(text!("inner.error.nothingToChange"));
    }
    let mut value = value.clone();
    let beside = beside(&mut value)?;
    let mut moves = Vec::new();
    let mut at = names.to_vec();
    let renamed = value
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|n| *n != old.as_str())
        .map(str::to_string);
    if let Some(name) = &renamed {
        if siblings.iter().any(|r| row_name(r) == Some(name.as_str())) {
            return Err(text!("inner.error.sameLevel", name));
        }
        at = parent.to_vec();
        at.push(name.clone());
        moves.push((names.to_vec(), at.clone()));
    }
    let row = siblings
        .iter_mut()
        .find(|r| row_name(r) == Some(old.as_str()))
        .and_then(Value::as_object_mut)
        .ok_or_else(|| text!("inner.error.rowNotFound", path = joined(names)))?;
    let kind_after = match value.get("kind") {
        Some(kind) => kind.as_str().unwrap_or("item"),
        None => row.get("kind").and_then(Value::as_str).unwrap_or("item"),
    }
    .to_string();
    // A new kind holds only what it holds: what the old one held goes unless written.
    if let Some(kind) = value.get("kind")
        && Some(kind) != row.get("kind")
    {
        let keep: Vec<&str> = if kind.as_str() == Some("item") || kind.is_null() {
            vec!["name", "kind", "heading", "rows"]
        } else {
            vec!["name", "kind"]
        };
        row.retain(|k, _| keep.contains(&k.as_str()));
    }
    // A table's headings and cells as read, which its cells changed in part are named over.
    let (columns, cells) = (strings(row, "columns"), table_cells(row));
    // What its body was, changed in part: answered short through MCP (`in_part::quiet`).
    let mut in_part = None;
    // What its cells were and are, changed in part: answered short the same way.
    let mut cells_part = None;
    for (key, given) in &value {
        if key == "name" {
            continue;
        }
        // A text row's body changed in part, over what it is (`in_part`).
        let given = match (key.as_str(), given) {
            ("body", Value::Object(_)) => {
                let before = row.get("body").and_then(Value::as_str).unwrap_or_default();
                let after =
                    super::in_part::changed(before, given, &format!("{}.body", joined(names)))?
                        .unwrap_or_default();
                in_part = Some((before.to_string(), after.clone()));
                Value::String(after)
            }
            // A table's cells changed in part, over the table as read (`cells`).
            ("cells", Value::Object(edits)) => {
                if kind_after != "table" {
                    return Err(text!(
                        "inner.error.notATable",
                        path = joined(names),
                        kind = kind_after
                    ));
                }
                let (whole, gone) = super::cells::changed(
                    &columns,
                    &cells,
                    edits,
                    &format!("{}.cells", joined(names)),
                )?;
                let written = serde_json::json!(whole);
                cells_part = Some((cells.clone(), whole, gone));
                written
            }
            _ => given.clone(),
        };
        row.insert(key.clone(), given);
    }
    if let Some(name) = renamed {
        row.insert("name".into(), name.into());
    }
    // Put elsewhere among the rows beside it, by the name of one of them.
    if let Some(beside) = &beside {
        let own = at.last().cloned().unwrap_or_default();
        let from = siblings
            .iter()
            .position(|r| row_name(r) == Some(own.as_str()))
            .ok_or_else(|| text!("inner.error.rowNotFound", path = joined(names)))?;
        let moved = siblings.remove(from);
        put_beside(siblings, moved, beside)?;
    }
    let mut answer = write_back(tx, scope, id, "rows", rows, &moves, Some(&at))?;
    if let Some((before, after)) = in_part.filter(|_| tx.who().side == crate::write::Side::Mcp) {
        super::in_part::quieted(&mut answer, "body", &before, &after);
    }
    if let Some((before, after, gone)) =
        cells_part.filter(|_| tx.who().side == crate::write::Side::Mcp)
    {
        super::cells::quieted(&mut answer, &before, &after, &gone);
    }
    Ok(answer)
}

/// A row let go, and what is inside it (`deleteRowAt`).
pub(crate) fn delete_row(
    tx: &mut Tx,
    scope: &Scope,
    id: &str,
    names: &[String],
) -> Result<Value, String> {
    let mut rows = wire_list(tx, scope, id, "rows")?;
    let (last, parent) = names
        .split_last()
        .ok_or_else(|| text!("inner.error.rowNameMissing"))?;
    if let Some(siblings) = children_mut(&mut rows, parent) {
        siblings.retain(|r| row_name(r) != Some(last.as_str()));
    }
    write_back(tx, scope, id, "rows", rows, &[], None)
}

// MARK: - Versions of a stackdoc

/// A stackdoc's versions written back whole, answered with the version at `reading` (its
/// name), or the card.
fn write_versions(
    tx: &mut Tx,
    scope: &Scope,
    id: &str,
    list: Vec<Value>,
    moves: &[(Vec<String>, Vec<String>)],
    reading: Option<&str>,
) -> Result<Value, String> {
    let mut value = Map::new();
    value.insert("versions".to_string(), Value::Array(list));
    let said = edit_card(tx, scope, id, &value, moves)?;
    let seen = Seen::in_tx(tx, scope)?;
    let project = seen.project();
    let card = project
        .card(id)
        .ok_or_else(|| text!("inner.error.unreadable"))?;
    let view = match reading {
        Some(name) => seen.version_view(card, name),
        None => seen.card_view(card, &Projection::none(), None),
    };
    Ok(super::warned(view, &said))
}

/// A version added on top, as its path names it (`Docs:Changelog.v0_2_24`: the newest
/// version is the first).
pub(crate) fn create_version(
    tx: &mut Tx,
    scope: &Scope,
    path: &str,
    value: &Map<String, Value>,
) -> Result<Value, String> {
    let seen = Seen::in_tx(tx, scope)?;
    let project = seen.project();
    let card = card_holding(&project, path)
        .filter(|c| c.kind == "stackdoc")
        .ok_or_else(|| text!("inner.error.noStackdoc", path))?;
    let id = card.id.clone();
    let names = crate::export::paths::WrittenPath::new(path)
        .map(|w| w.names)
        .unwrap_or_default();
    let [name] = names.as_slice() else {
        return Err(text!("inner.error.versionPath", path));
    };
    let name = &crate::pathtext::unquote(name).into_owned();
    let mut versions = wire_list(tx, scope, &id, "versions")?;
    if versions.iter().any(|v| row_name(v) == Some(name.as_str())) {
        return Err(text!("inner.error.exists", path));
    }
    if let Some(given) = value.get("name").and_then(Value::as_str)
        && given != name
    {
        return Err(text!("inner.error.versionNameFromPath", name));
    }
    let mut version = value.clone();
    version.insert("name".into(), name.clone().into());
    versions.insert(0, Value::Object(version));
    write_versions(tx, scope, &id, versions, &[], Some(name))
}

/// What the value says, over what the version was; a new name renames it, the paths that
/// name it and the version the card has open.
pub(crate) fn update_version(
    tx: &mut Tx,
    scope: &Scope,
    id: &str,
    old: &str,
    value: &Map<String, Value>,
) -> Result<Value, String> {
    if value.is_empty() {
        return Err(text!("inner.error.nothingToChange"));
    }
    let open = Seen::in_tx(tx, scope)?
        .project()
        .card(id)
        .and_then(|c| c.version.clone());
    let mut versions = wire_list(tx, scope, id, "versions")?;
    let renamed = value
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|n| *n != old)
        .map(str::to_string);
    if let Some(name) = &renamed
        && versions.iter().any(|v| row_name(v) == Some(name.as_str()))
    {
        return Err(text!("inner.error.versionExists", name));
    }
    let version = versions
        .iter_mut()
        .find(|v| row_name(v) == Some(old))
        .and_then(Value::as_object_mut)
        .ok_or_else(|| text!("inner.error.versionNotFound", name = old))?;
    // What its body was, changed in part: answered short through MCP (`in_part::quiet`).
    let mut in_part = None;
    for (key, given) in value {
        if key == "name" {
            continue;
        }
        // Its body changed in part, over what it is (`in_part`).
        let given = match (key.as_str(), given) {
            ("body", Value::Object(_)) => {
                let before = version
                    .get("body")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let after = super::in_part::changed(
                    before,
                    given,
                    &format!("{}.body", crate::pathtext::quote(old)),
                )?
                .unwrap_or_default();
                in_part = Some((before.to_string(), after.clone()));
                Value::String(after)
            }
            _ => given.clone(),
        };
        version.insert(key.clone(), given);
    }
    let mut moves = Vec::new();
    let at = match &renamed {
        Some(name) => {
            version.insert("name".into(), name.clone().into());
            moves.push((vec![old.to_string()], vec![name.clone()]));
            name.clone()
        }
        None => old.to_string(),
    };
    let mut answer = write_versions(tx, scope, id, versions, &moves, Some(&at))?;
    if let Some((before, after)) = in_part.filter(|_| tx.who().side == crate::write::Side::Mcp) {
        super::in_part::quieted(&mut answer, "body", &before, &after);
    }
    if renamed.is_some() && open.as_deref() == Some(old) {
        tx.look(
            &scope.project,
            &scope.tree,
            id,
            "",
            "version",
            Some(&Value::String(at)),
        )?;
    }
    Ok(answer)
}

/// A version let go.
pub(crate) fn delete_version(
    tx: &mut Tx,
    scope: &Scope,
    id: &str,
    name: &str,
) -> Result<Value, String> {
    let mut versions = wire_list(tx, scope, id, "versions")?;
    versions.retain(|v| row_name(v) != Some(name));
    write_versions(tx, scope, id, versions, &[], None)
}
