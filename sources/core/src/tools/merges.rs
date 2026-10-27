//! A merge as MCP asks for it (`branch_read` with `against`, `branch_merge`;
//! `MCPSurface.branches` in the Swift app): its preview — what goes in, what conflicts,
//! with the three versions of each unit in MCP's shape, and where paths overlap — and the
//! merge itself, refused while a conflict is left or a path would hold two things.

use super::Seen;
use super::cards::Resolver;
use crate::write::{MergeChange, MergePreview, MergeSide, MergeTake, Tx};
use serde_json::{Map, Value};
use std::collections::HashMap;

/// Conflicts chosen, as `branch_merge` takes them (`take`), into a locked main for a merge request too.
pub(crate) fn take(given: Option<&Value>) -> Result<MergeTake, String> {
    let mut out = MergeTake::new();
    let Some(Value::Object(by_path)) = given else {
        return Ok(out);
    };
    for (path, units) in by_path {
        let Some(chosen) = units.as_object() else {
            return Err(text!("merges.take.shape", path));
        };
        for (unit, side) in chosen {
            let Some(picked) = side.as_str().and_then(MergeSide::named) else {
                return Err(text!("merges.take.side", path, unit));
            };
            out.entry(path.clone())
                .or_default()
                .insert(unit.clone(), picked);
        }
    }
    Ok(out)
}

/// What `take` chooses that no conflict has, against the preview made
/// without it: told, rather than the conflicts said again.
pub(crate) fn unmatched_takes(bare: &MergePreview, take: &MergeTake) -> Result<(), String> {
    let mut wrong: Vec<String> = take
        .iter()
        .flat_map(|(path, units)| units.keys().map(move |unit| (path, unit)))
        .filter(|(path, unit)| {
            !bare.conflicts.iter().any(|c| {
                &&c.path == path
                    && (&&c.label == unit
                        || &&c.unit == unit
                        || (c.unit == crate::write::MergeWhole
                            && crate::write::merge_is_whole(unit)))
            })
        })
        .map(|(path, unit)| text!("merges.take.unitOf", path, unit))
        .collect();
    if wrong.is_empty() {
        return Ok(());
    }
    wrong.sort();
    let there: Vec<String> = bare
        .conflicts
        .iter()
        .map(|c| text!("merges.take.unitOf", path = c.path, unit = c.label))
        .collect();
    Err(text!(
        "merges.take.noConflict",
        wrong = crate::text::list(&wrong),
        there = if there.is_empty() {
            text!("merges.take.noConflictNone")
        } else {
            crate::text::list(&there)
        }
    ))
}

/// One version of a unit of a card (of `kind`) or a section in MCP's shape: what card_read would say
/// of it, the cards and sections it names by their paths in `project` — a merge's conflict, a
/// commit's change.
pub(crate) fn said(
    section: bool,
    kind: Option<&str>,
    unit: &str,
    value: Option<&Value>,
    project: &crate::export::Project,
) -> Value {
    let Some(value) = value else {
        return Value::Null;
    };
    if section {
        return value.clone();
    }
    let kinds = project.defs.cards();
    let Some(def) = kind.and_then(|k| kinds.get(k)) else {
        return value.clone();
    };
    let resolver = Resolver {
        project,
        base: project.directory.home.clone(),
        made: &HashMap::new(),
        inside: None,
        unmade: None,
    };
    let wire = |doc: Value| crate::kinds::to_wire(def, &doc, &resolver);
    if unit == crate::write::MergeWhole {
        let mut out = Map::new();
        if let Some(name) = value.get("name") {
            out.insert("name".into(), name.clone());
        }
        if let Value::Object(w) = wire(value.clone()) {
            out.extend(w);
        }
        return Value::Object(out);
    }
    if let Some(key) = unit.strip_prefix("f:") {
        let mut fields = Map::new();
        fields.insert(key.to_string(), value.clone());
        let mut doc = Map::new();
        doc.insert("fields".into(), Value::Object(fields));
        return wire(Value::Object(doc))
            .get(key)
            .cloned()
            .unwrap_or_else(|| value.clone());
    }
    let rows = |name: &str, list: Value| {
        let mut rows = Map::new();
        rows.insert(name.to_string(), list);
        let mut doc = Map::new();
        doc.insert("rows".into(), Value::Object(rows));
        wire(Value::Object(doc)).get(name).cloned()
    };
    if let Some(rest) = unit.strip_prefix("r:") {
        let name = rest.split(':').next().unwrap_or(rest);
        let mut row = value.clone();
        if let Some(o) = row.as_object_mut() {
            o.remove("~parent");
        }
        return rows(name, Value::Array(vec![row]))
            .and_then(|l| l.get(0).cloned())
            .unwrap_or_else(|| value.clone());
    }
    if let Some(name) = unit.strip_prefix("rows.") {
        return rows(name, value.clone()).unwrap_or_else(|| value.clone());
    }
    // A line of the card's (`connections`): as card_read says it, the card it reaches by
    // its path.
    if unit.starts_with("c:") {
        let doc = serde_json::json!({ "connections": [value.clone()] });
        return crate::kinds::connections_to_wire(&doc, &resolver)
            .and_then(|l| l.get(0).cloned())
            .unwrap_or_else(|| value.clone());
    }
    // A key of the card's own: its section by its path.
    if unit == "section"
        && let Some(id) = value.as_str()
    {
        return project.directory.full_section(id).into();
    }
    value.clone()
}

/// One version of a unit of a commit's change in MCP's shape (`said`), with what a card or a section
/// is in — its container and its parent section — by its path too.
pub(crate) fn said_change(
    section: bool,
    kind: Option<&str>,
    unit: &str,
    value: Option<&Value>,
    project: &crate::export::Project,
) -> Value {
    match (section, unit, value.and_then(Value::as_str)) {
        (true, "parent_id", Some(id)) => project.directory.full_section(id).into(),
        (false, "in", Some(id)) => project.named(id).unwrap_or_else(|| id.to_string()).into(),
        _ => said(section, kind, unit, value, project),
    }
}

/// A card's or a section's document whole in MCP's shape, as card_create and section_create are
/// given it: its keys, the cards and sections it names by their paths, a card's fields and rows as its
/// kind says them, and its lines.
pub(crate) fn said_whole(
    section: bool,
    doc: &Map<String, Value>,
    project: &crate::export::Project,
) -> Value {
    let kind = doc.get("kind").and_then(Value::as_str);
    let mut out = Map::new();
    for (key, value) in doc {
        if key == "id" || (!section && ["fields", "rows", "connections"].contains(&key.as_str())) {
            continue;
        }
        out.insert(
            key.clone(),
            said_change(section, kind, key, Some(value), project),
        );
    }
    if section {
        return Value::Object(out);
    }
    let whole = Value::Object(doc.clone());
    match said(false, kind, crate::write::MergeWhole, Some(&whole), project) {
        Value::Object(wire) if kind.is_some_and(|k| project.defs.cards().get(k).is_some()) => {
            for (key, value) in wire {
                out.entry(key).or_insert(value);
            }
        }
        _ => {
            for key in ["fields", "rows"] {
                if let Some(value) = doc.get(key) {
                    out.insert(key.to_string(), value.clone());
                }
            }
        }
    }
    let resolver = Resolver {
        project,
        base: project.directory.home.clone(),
        made: &HashMap::new(),
        inside: None,
        unmade: None,
    };
    if let Some(lines) = crate::kinds::connections_to_wire(&whole, &resolver) {
        out.insert("connections".into(), lines);
    }
    Value::Object(out)
}

/// A merge's preview as MCP answers it (`previewView`).
pub(crate) fn preview_view(preview: &MergePreview, seen: &Seen) -> Map<String, Value> {
    let mut out = Map::new();
    out.insert("project".into(), seen.summary.name.clone().into());
    out.insert("source".into(), preview.source.clone().into());
    out.insert("into".into(), preview.target.clone().into());
    for (key, change) in [
        ("add", MergeChange::Add),
        ("change", MergeChange::Change),
        ("remove", MergeChange::Remove),
    ] {
        let paths: Vec<Value> = preview
            .items
            .iter()
            .filter(|i| i.change == change)
            .map(|i| i.path.clone().into())
            .collect();
        if !paths.is_empty() {
            out.insert(key.into(), Value::Array(paths));
        }
    }
    if !preview.conflicts.is_empty() {
        let project = seen.project();
        out.insert(
            "conflicts".into(),
            Value::Array(
                preview
                    .conflicts
                    .iter()
                    .map(|c| {
                        let said = |value: Option<&Value>| {
                            said(c.section, c.kind.as_deref(), &c.unit, value, &project)
                        };
                        let mut row = Map::new();
                        row.insert("path".into(), c.path.clone().into());
                        row.insert("unit".into(), c.label.clone().into());
                        row.insert("base".into(), said(c.base.as_ref()));
                        row.insert("source".into(), said(c.source.as_ref()));
                        row.insert("target".into(), said(c.target.as_ref()));
                        Value::Object(row)
                    })
                    .collect(),
            ),
        );
    }
    if !preview.overlaps.is_empty() {
        out.insert(
            "overlaps".into(),
            Value::Array(preview.overlaps.iter().cloned().map(Value::from).collect()),
        );
    }
    if preview.places > 0 {
        out.insert("places".into(), preview.places.into());
    }
    if preview.is_empty() {
        out.insert("nothing".into(), true.into());
    }
    out
}

/// Whether the boxes the source moved go in too, as `branch_merge` takes it
/// (`places`).
pub(crate) fn places(given: &Map<String, Value>) -> Result<bool, String> {
    match given.get("places") {
        None | Some(Value::Null) => Ok(false),
        Some(Value::Bool(on)) => Ok(*on),
        Some(_) => Err(text!("merges.places.shape")),
    }
}

/// The target's broken paths, as words, to say which the merge broke (`mergeBroken`).
pub(crate) fn broken(seen: &Seen) -> Vec<String> {
    let project = seen.project();
    seen.detail
        .schemas
        .iter()
        .flat_map(|card| {
            super::check::problems(&project, card)
                .into_iter()
                .map(|(_, n)| text!("merges.broken.item", at = n.at, via = n.via, path = n.path))
        })
        .collect()
}

/// A tree as a merge's end names it, read in the transaction.
pub(crate) fn seen_in(
    tx: &Tx,
    project: &str,
    project_name: &str,
    tree: &str,
    name: &str,
) -> Result<std::rc::Rc<Seen>, String> {
    Seen::in_tx(
        tx,
        &super::cards::Scope {
            project: project.to_string(),
            project_name: project_name.to_string(),
            tree: tree.to_string(),
            tree_name: name.to_string(),
        },
    )
}
