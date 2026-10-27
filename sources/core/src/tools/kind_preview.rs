//! The window's preview of a definition being written (`kind_preview`, `specboard://SpecBoard/Docs/Design:CoreApi`
//! 「ops」): the draft put among the project's own kinds the way `kind_create` and `kind_update` put it, read
//! as the project's kinds are read (`Defs::of_project`), and — a kind of card — a card of it read with
//! the draft as `project_detail` reads a card: one of the tree's, or one of values made up for it. The
//! answer is in `project_detail`'s shapes, so the window draws it as the section map draws a card.
//! Nothing is written and nothing is held: a draft is tried as often as it is typed. What is wrong
//! with a draft is `dryRun`'s to say, not this.

use super::kind_tools::{OF, builtin_in, free};
use super::own_kinds::{kept, name_key};
use crate::kinds::{FieldType, Kind};
use crate::store::{Composition, Item, Reader};
use serde_json::{Map, Value, json};

/// The id of the card of values made up for a preview: no card of a store's is called so.
const MADE: &str = "kind_preview:made";

/// The base fields every kind takes: what a card says in its own places (its label, its description,
/// its tags and metadata), not values to show a kind by.
const BASE_FIELDS: [&str; 4] = ["label", "description", "tags", "metadata"];

/// `kind_preview`: the draft `definition` of the list `of` names (`card`, `section`, `line`), in place
/// of the project's own kind `replacing` (a new one when none), read with the project's kinds as
/// though it were written: `kind`, as `project_detail`'s `kinds` says it (none when the draft
/// cannot be read as a definition), and for a kind of card `card`, as `project_detail`'s `schemas`
/// says a card — the tree's card at `sample` read with the draft, or one of made-up values
/// (`made: true`) when no sample is asked for or the one asked for is not a card of the kind.
pub(crate) fn preview(
    reader: &Reader,
    project: &str,
    tree: &str,
    request: &Value,
) -> Result<Value, String> {
    let of = request
        .get("of")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let Some(list) = OF.iter().find(|(o, _)| *o == of).map(|(_, l)| *l) else {
        return Err(text!("kindTools.create.ofShape", of));
    };
    let replacing = request
        .get("replacing")
        .and_then(Value::as_str)
        .filter(|n| !n.is_empty());
    let definition = request.get("definition").unwrap_or(&Value::Null);
    let own = kept(reader.conn, project)?;
    let none = || Ok(json!({ "kind": null }));
    let Some((own, name, builtin)) = placed(own, list, replacing, definition) else {
        return none();
    };
    let own_text = Value::Object(own).to_string();
    let Some(kind) = read_back(&own_text, list, &name, builtin) else {
        return none();
    };
    let mut out = Map::new();
    out.insert("kind".into(), kind);
    if list == "cards" {
        let sample = request.get("sample").and_then(Value::as_str);
        let renamed = replacing.filter(|r| !builtin && *r != name);
        let branch = (!tree.is_empty()).then_some(tree);
        let card = card(reader, project, branch, &own_text, &name, renamed, sample)?;
        out.insert("card".into(), card);
    }
    Ok(Value::Object(out))
}

/// The project's own kinds with the draft put in: in place of `replacing` (where it was in its list),
/// or last in its list — as `kind_create` and `kind_update` put it. A built-in kind is as it is built
/// (the draft of one is read only). The kinds, the draft's name and whether it is built in; none when
/// the draft names no kind, or one another kind has.
fn placed(
    mut own: Map<String, Value>,
    list: &str,
    replacing: Option<&str>,
    definition: &Value,
) -> Option<(Map<String, Value>, String, bool)> {
    let key = name_key(list);
    if let Some(name) = replacing.filter(|n| builtin_in(list, n)) {
        return Some((own, name.to_string(), true));
    }
    let mut definition = definition.as_object()?.clone();
    let name = match definition.get(key) {
        Some(Value::String(n)) => n.trim().to_string(),
        None => replacing?.to_string(),
        Some(_) => return None,
    };
    if name.is_empty() {
        return None;
    }
    definition.insert(key.into(), name.clone().into());
    let mut items = own
        .get(list)
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let at = replacing.and_then(|r| items.iter().position(|d| d[key] == r));
    if let Some(at) = at {
        items.remove(at);
    }
    own.insert(list.into(), Value::Array(items.clone()));
    free(&own, &name).ok()?;
    match at {
        Some(at) => items.insert(at, Value::Object(definition)),
        None => items.push(Value::Object(definition)),
    }
    own.insert(list.into(), Value::Array(items));
    Some((own, name, false))
}

/// The kind `name` of `list` as `project_detail`'s `kinds` says it with `own` the project's: none
/// when it was not read (a draft that is not a definition).
fn read_back(own: &str, list: &str, name: &str, builtin: bool) -> Option<Value> {
    let defs = crate::kinds::Defs::of_project(Some(own));
    let read = match list {
        "cards" => defs.cards().get(name).is_some_and(|k| k.builtin == builtin),
        "sections" => defs.section(name).is_some_and(|s| s.builtin == builtin),
        _ => defs.line(name).is_some_and(|l| l.builtin == builtin),
    };
    if !read {
        return None;
    }
    let kinds = crate::detail::kinds_json(Some(own));
    kinds[list]
        .as_array()
        .into_iter()
        .flatten()
        .find(|d| d[name_key(list)] == name && (d["builtin"] == true) == builtin)
        .cloned()
}

/// A card of the kind `name` read with the project's kinds `own`, as `project_detail`'s `schemas` says
/// a card: the card at `sample` in the tree (its kind called `renamed` before, read as `name`), or
/// one of values made up for the kind.
fn card(
    reader: &Reader,
    project: &str,
    branch: Option<&str>,
    own: &str,
    name: &str,
    renamed: Option<&str>,
    sample: Option<&str>,
) -> Result<Value, String> {
    let mut row = reader
        .project(project)?
        .ok_or_else(|| text!("detail.error.projectMissing", project))?;
    row.kinds = Some(own.to_string());
    let layout = super::grid::Layout::read(reader.conn, project)?;
    if let Some(sample) = sample {
        let mut made = reader.compose(project, branch)?;
        if let Some(before) = renamed {
            for item in &mut made.cards {
                if item.doc.get("kind").and_then(Value::as_str) == Some(before) {
                    item.doc.insert("kind".into(), name.into());
                }
            }
        }
        let (raw, named) = crate::detail::of_made_named(&row, &made);
        let id = named
            .iter()
            .find(|(_, path)| path.as_str() == sample)
            .map(|(id, _)| id.clone())
            .or_else(|| named.contains_key(sample).then(|| sample.to_string()));
        if let Some(id) = id
            && let Some(card) = dressed(&raw, &id, name, &layout)?
        {
            return Ok(card);
        }
    }
    let kind = crate::kinds::Defs::of_project(Some(own))
        .cards()
        .get(name)
        .cloned()
        .ok_or_else(|| text!("kindPreview.unread", name))?;
    let made = Composition {
        cards: vec![Item {
            id: MADE.to_string(),
            doc: made_doc(&kind),
        }],
        ..Composition::default()
    };
    let raw = crate::detail::of_made(&row, &made);
    let mut card =
        dressed(&raw, MADE, name, &layout)?.ok_or_else(|| text!("kindPreview.unread", name))?;
    card["made"] = true.into();
    Ok(card)
}

/// The card `id` of a project read whole (`raw`), as `project_detail` dresses a card of `schemas`: the
/// keys of the typed reading left out, the rows its face draws it in (`faceRows`) and what it holds
/// that its kind no longer has (`strayFields`). None when it is not a card of the kind `name`.
fn dressed(
    raw: &Value,
    id: &str,
    name: &str,
    layout: &super::grid::Layout,
) -> Result<Option<Value>, String> {
    let Some(entry) = raw["schemas"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|c| c["id"] == id && c["kind"] == name)
    else {
        return Ok(None);
    };
    let typed = crate::model::typed(raw)?;
    let project = crate::export::Project::new(&typed);
    let Some(card) = typed.schemas.iter().find(|c| c.id == id) else {
        return Ok(None);
    };
    let mut out = entry.as_object().cloned().unwrap_or_default();
    crate::screen_detail::strip_card(&mut out);
    if let Some(rows) = crate::screen_detail::face_rows_of(card, project.defs.cards(), layout) {
        out.insert("faceRows".into(), rows.into());
    }
    if let Some(found) = crate::screen_detail::strays_of(&project, card) {
        out.insert("strayFields".into(), found);
    }
    Ok(Some(Value::Object(out)))
}

// MARK: - Values made up

/// The document of a card of `kind` with values made up for it: named as the screen's language
/// calls a sample, each field of its own with a value of its type (`made_value`), and one row of
/// each kind of row.
fn made_doc(kind: &Kind) -> Map<String, Value> {
    let mut doc = Map::new();
    doc.insert("kind".into(), kind.name.clone().into());
    doc.insert("name".into(), text!("kindPreview.made.name").into());
    let fields = made_fields(
        kind.fields
            .iter()
            .filter(|f| !BASE_FIELDS.contains(&f.name.as_str())),
    );
    if !fields.is_empty() {
        doc.insert("fields".into(), Value::Object(fields));
    }
    let mut rows = Map::new();
    for (i, row) in kind.rows.iter().enumerate() {
        let mut one = Map::new();
        one.insert("id".into(), format!("{MADE}:{i}").into());
        let fields = made_fields(row.fields.iter());
        if !fields.is_empty() {
            one.insert("fields".into(), Value::Object(fields));
        }
        rows.insert(row.name.clone(), Value::Array(vec![Value::Object(one)]));
    }
    if !rows.is_empty() {
        doc.insert("rows".into(), Value::Object(rows));
    }
    doc
}

/// Each field with a value made up for it (`made_value`), in their order: a field that points
/// somewhere, or holds what its kind's own checks shape, left empty.
fn made_fields<'a>(fields: impl Iterator<Item = &'a crate::kinds::Field>) -> Map<String, Value> {
    fields
        .filter(|f| !f.look)
        .filter_map(|f| made_value(f).map(|v| (f.name.clone(), v)))
        .collect()
}

/// A value made up for a field: its default when it has one, else one of its type — a list of
/// two when it holds a list, two lists of two when it holds a list of lists.
fn made_value(field: &crate::kinds::Field) -> Option<Value> {
    if field.many > 0
        && let Some(given @ Value::Array(_)) = &field.default
    {
        return Some(given.clone());
    }
    let two = || -> Option<Value> {
        let items: Vec<Value> = (0..2).filter_map(|i| made_one(field, i)).collect();
        (!items.is_empty()).then_some(Value::Array(items))
    };
    match field.many {
        0 => made_one(field, 0),
        1 => two(),
        _ => two().map(|row| Value::Array(vec![row.clone(), row])),
    }
}

/// The `i`th value made up for one value of a field: its default; a choice's option (the first, then
/// the next); a check off; a text and a Markdown in the screen's language; a number, a date, a URL,
/// a person, a branch and a merge request fixed; a group's fields each; none for what points
/// somewhere or what only the kind's own checks shape.
fn made_one(field: &crate::kinds::Field, i: usize) -> Option<Value> {
    if let Some(given) = field.default.as_ref().filter(|d| !d.is_array()) {
        return Some(given.clone());
    }
    Some(match field.fieldtype {
        FieldType::Text | FieldType::LongText => text!("kindPreview.made.text").into(),
        FieldType::Markdown => text!("kindPreview.made.markdown").into(),
        FieldType::Number => {
            let n = 3.0_f64
                .max(field.min.unwrap_or(f64::MIN))
                .min(field.max.unwrap_or(f64::MAX));
            json!(n as i64)
        }
        FieldType::Checkbox => false.into(),
        FieldType::Date => "2026-04-01".into(),
        FieldType::Url => "https://example.com".into(),
        FieldType::Radio | FieldType::DropdownList => {
            if field.options.is_empty() {
                return None;
            }
            field.options[i % field.options.len()].clone().into()
        }
        FieldType::Person => "Claude".into(),
        FieldType::Branch => "main".into(),
        FieldType::Mr => 1.into(),
        FieldType::Group => {
            let inner = made_fields(field.fields.iter());
            if inner.is_empty() {
                return None;
            }
            Value::Object(inner)
        }
        FieldType::Card
        | FieldType::Path
        | FieldType::Section
        | FieldType::Json
        | FieldType::Map => {
            return None;
        }
    })
}
