//! A shape's list face (`specboard://SpecBoard/Docs/Design:CardKinds`「データの一覧の顔」): how data of an
//! entity's or a value's shape is drawn as a list of records — on a data card's shape the array that
//! gives the rows (`list`) and the fields shown in the head (`header`); on the array's element shape
//! a row's text (`text`), a note after it (`note`), a mark (`mark`) and what unfolds under it
//! (`fold`). Every name is one of the shape's own top-level fields, so a field's rename or removal is
//! followed on the shape alone. The core checks it, follows it, counts the rows a data card draws by
//! it and gives the screen each row worked out; the screen draws them.

use super::instances;
use super::picks::Pick;
use crate::export::Project;
use crate::model::{Card, Field};
use serde_json::{Map, Value, json};
use std::collections::HashMap;

/// A mark's roles, in the order a head counts them: each a symbol and a colour the screen gives it.
pub(crate) const ROLES: [&str; 4] = ["ok", "bad", "muted", "hollow"];

/// The keys a face is written with.
const KEYS: [&str; 6] = ["list", "header", "text", "note", "mark", "fold"];

/// The keys of a face's `mark`.
const MARK_KEYS: [&str; 3] = ["field", "values", "none"];

/// How deep the rows unfolded under an element go into the shapes it embeds: a shape that embeds
/// itself stops here.
const FOLD_DEPTH: usize = 6;

// MARK: - Checked

/// Every problem with a shape's face as written (`face`), against its fields as they are now: each
/// said with the card's path, refused together before anything is written.
pub(crate) fn said(project: &Project, card: &Card, face: &Value) -> Result<(), String> {
    let full = project.full(card);
    let found = problems(project, &card.fields, face);
    if found.is_empty() {
        return Ok(());
    }
    Err(found
        .into_iter()
        .map(|(at, why)| {
            if at.is_empty() {
                format!("{full}.face: {why}")
            } else {
                format!("{full}.face.{at}: {why}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n"))
}

/// What is wrong with a face over `fields` (its shape's top level): where in the face and why.
fn problems(project: &Project, fields: &[Field], face: &Value) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    let Some(face) = face.as_object() else {
        out.push((String::new(), text!("faces.said.notObject")));
        return out;
    };
    for key in face.keys() {
        if !KEYS.contains(&key.as_str()) {
            out.push((
                String::new(),
                text!(
                    "faces.said.keyNotAllowed",
                    key,
                    keys = crate::text::List::of(KEYS)
                ),
            ));
        }
    }
    let names = || crate::text::List::of(fields.iter().map(|f| f.name.as_str()));
    // A name of the shape's own fields, or why not.
    let field = |at: &str, raw: &Value, out: &mut Vec<(String, String)>| -> Option<&Field> {
        let Some(name) = raw.as_str() else {
            out.push((at.to_string(), text!("faces.said.notName")));
            return None;
        };
        let found = fields.iter().find(|f| f.name == name);
        if found.is_none() {
            out.push((
                at.to_string(),
                text!("faces.said.noField", name, names = names()),
            ));
        }
        found
    };
    let names_of = |key: &str, out: &mut Vec<(String, String)>| -> Vec<Value> {
        match face.get(key) {
            None => Vec::new(),
            Some(Value::Array(items)) => items.clone(),
            Some(_) => {
                out.push((key.to_string(), text!("faces.said.notNames")));
                Vec::new()
            }
        }
    };
    if let Some(raw) = face.get("list")
        && let Some(f) = field("list", raw, &mut out)
        && !(f.is_array && shaped(project, f).is_some())
    {
        out.push((
            "list".into(),
            text!("faces.said.listNotShapes", name = f.name),
        ));
    }
    for (i, raw) in names_of("header", &mut out).iter().enumerate() {
        let at = format!("header[{i}]");
        if let Some(f) = field(&at, raw, &mut out)
            && (f.is_array || f.is_object() || shaped(project, f).is_some())
        {
            out.push((at, text!("faces.said.headerNotOne", name = f.name)));
        }
    }
    for key in ["text", "note"] {
        if let Some(raw) = face.get(key)
            && let Some(f) = field(key, raw, &mut out)
            && !texty(project, f)
        {
            out.push((key.to_string(), text!("faces.said.notText", name = f.name)));
        }
    }
    let mut mark_field: Option<String> = None;
    if let Some(mark) = face.get("mark") {
        match mark.as_object() {
            None => out.push(("mark".into(), text!("faces.said.markShape"))),
            Some(mark) => {
                for key in mark.keys() {
                    if !MARK_KEYS.contains(&key.as_str()) {
                        out.push((
                            "mark".into(),
                            text!(
                                "faces.said.keyNotAllowed",
                                key,
                                keys = crate::text::List::of(MARK_KEYS)
                            ),
                        ));
                    }
                }
                let marked = match mark.get("field") {
                    None => {
                        out.push(("mark.field".into(), text!("faces.said.markFieldNeeded")));
                        None
                    }
                    Some(raw) => field("mark.field", raw, &mut out),
                };
                let values = marked.and_then(|f| {
                    mark_field = Some(f.name.clone());
                    let values = mark_values(project, f);
                    if values.is_none() {
                        out.push((
                            "mark.field".into(),
                            text!("faces.said.markNotEnum", name = f.name),
                        ));
                    }
                    values
                });
                match mark.get("values") {
                    None => {}
                    Some(Value::Object(given)) => {
                        for (value, role) in given {
                            let at = format!("mark.values.{value}");
                            if let Some(values) = &values
                                && !values.contains(value)
                            {
                                out.push((
                                    at.clone(),
                                    text!(
                                        "faces.said.notValue",
                                        value,
                                        values = crate::text::List::of(values)
                                    ),
                                ));
                            }
                            role_said(&at, role, &mut out);
                        }
                    }
                    Some(_) => out.push(("mark.values".into(), text!("faces.said.valuesShape"))),
                }
                if let Some(role) = mark.get("none") {
                    role_said("mark.none", role, &mut out);
                }
            }
        }
    }
    let mut folded: Vec<String> = Vec::new();
    for (i, raw) in names_of("fold", &mut out).iter().enumerate() {
        let at = format!("fold[{i}]");
        let Some(f) = field(&at, raw, &mut out) else {
            continue;
        };
        if folded.contains(&f.name) {
            out.push((at, text!("faces.said.foldTwice", name = f.name)));
            continue;
        }
        let shown = ["text", "note"]
            .iter()
            .filter_map(|k| face.get(*k).and_then(Value::as_str))
            .chain(mark_field.as_deref())
            .any(|n| n == f.name);
        if shown {
            out.push((at, text!("faces.said.foldShown", name = f.name)));
        }
        folded.push(f.name.clone());
    }
    out
}

/// A role a face's mark gives, or why it is none.
fn role_said(at: &str, role: &Value, out: &mut Vec<(String, String)>) {
    if !role.as_str().is_some_and(|r| ROLES.contains(&r)) {
        out.push((
            at.to_string(),
            text!(
                "faces.said.notRole",
                role = role.to_string(),
                roles = crate::text::List::of(ROLES)
            ),
        ));
    }
}

/// The entity or value a field embeds, when it does.
fn shaped<'a>(project: &Project<'a>, field: &Field) -> Option<&'a Card> {
    field
        .ref_schema_id
        .as_deref()
        .filter(|_| field.is_ref())
        .and_then(|id| project.card(id))
        .filter(|c| c.kind == "entity" || c.kind == "value")
}

/// The enum a field embeds, when it does.
fn enum_of<'a>(project: &Project<'a>, field: &Field) -> Option<&'a Card> {
    field
        .ref_schema_id
        .as_deref()
        .filter(|_| field.is_ref())
        .and_then(|id| project.card(id))
        .filter(|c| c.kind == "enum")
}

/// Whether a field holds one text a row can show: a string, a text or an enum's value.
fn texty(project: &Project, field: &Field) -> bool {
    !field.is_array
        && (field.kind == "string" || field.kind == "text" || enum_of(project, field).is_some())
}

/// The values a mark may map from a field: an enum's values, or a truth's `true` and `false`. None
/// for a field a mark cannot be read from.
fn mark_values(project: &Project, field: &Field) -> Option<Vec<String>> {
    if field.is_array {
        return None;
    }
    if field.kind == "bool" {
        return Some(vec!["true".into(), "false".into()]);
    }
    enum_of(project, field).map(|e| e.values.iter().map(|v| v.value.clone()).collect())
}

// MARK: - Followed

/// A face with each name written again: `rename` gives a name's new one, or none for a field let go
/// of (its name taken out; a `list` or a mark's field gone takes the list or the mark with it). How
/// many names changed.
pub(crate) fn rename(face: &mut Value, rename: &mut dyn FnMut(&str) -> Option<String>) -> usize {
    let Some(face) = face.as_object_mut() else {
        return 0;
    };
    let mut count = 0;
    let mut one = |slot: &mut Value, count: &mut usize| -> bool {
        let Some(name) = slot.as_str() else {
            return true;
        };
        match rename(name) {
            Some(now) if now == name => true,
            Some(now) => {
                *slot = Value::from(now);
                *count += 1;
                true
            }
            None => {
                *count += 1;
                false
            }
        }
    };
    for key in ["list", "text", "note"] {
        if let Some(slot) = face.get_mut(key)
            && !one(slot, &mut count)
        {
            face.shift_remove(key);
        }
    }
    for key in ["header", "fold"] {
        if let Some(Value::Array(items)) = face.get_mut(key) {
            items.retain_mut(|slot| one(slot, &mut count));
        }
    }
    if let Some(Value::Object(mark)) = face.get_mut("mark")
        && let Some(slot) = mark.get_mut("field")
        && !one(slot, &mut count)
    {
        face.shift_remove("mark");
    }
    count
}

/// A shape's document with its face's names as its fields are named now: the card as it was
/// (`before`) gives each name's field, `now` each field's name by its id (a field not there, let go
/// of). How many names changed; none when the document has no face.
pub(crate) fn followed(before: &Card, doc: &mut Value, now: &HashMap<String, String>) -> usize {
    let Some(face) = doc.pointer_mut("/fields/face") else {
        return 0;
    };
    rename(face, &mut |name| {
        match before.fields.iter().find(|f| f.name == name) {
            Some(field) => now.get(&field.id).cloned(),
            // Not one of its fields then: written with the fields of now, as it is.
            None => Some(name.to_string()),
        }
    })
}

/// A shape's document with its face let go of the names that are not its top-level fields now.
pub(crate) fn pruned(doc: &mut Map<String, Value>) -> usize {
    let names: Vec<String> = doc
        .get("rows")
        .and_then(|r| r.get("fields"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|row| row["fields"]["name"].as_str().map(str::to_string))
        .collect();
    let Some(face) = doc.get_mut("fields").and_then(|f| f.get_mut("face")) else {
        return 0;
    };
    rename(face, &mut |name| {
        names.iter().any(|n| n == name).then(|| name.to_string())
    })
}

/// The shapes whose face marks by a field that takes the enum `enum_id`: each card's id.
pub(crate) fn marking_enum(project: &Project, enum_id: &str) -> Vec<String> {
    project
        .detail
        .schemas
        .iter()
        .filter(|c| c.kind == "entity" || c.kind == "value")
        .filter(|c| {
            let Some(name) = c
                .face
                .as_ref()
                .and_then(|f| f.pointer("/mark/field"))
                .and_then(Value::as_str)
            else {
                return false;
            };
            c.fields
                .iter()
                .find(|f| f.name == name)
                .is_some_and(|f| f.ref_schema_id.as_deref() == Some(enum_id))
        })
        .map(|c| c.id.clone())
        .collect()
}

/// A shape's document with its mark's value `old` named `now` (an enum's value renamed). Whether it
/// changed.
pub(crate) fn value_renamed(doc: &mut Map<String, Value>, old: &str, now: &str) -> bool {
    let Some(Value::Object(values)) = doc
        .get_mut("fields")
        .and_then(|f| f.get_mut("face"))
        .and_then(|f| f.get_mut("mark"))
        .and_then(|m| m.get_mut("values"))
    else {
        return false;
    };
    if !values.contains_key(old) {
        return false;
    }
    *values = std::mem::take(values)
        .into_iter()
        .map(|(k, v)| {
            if k == old {
                (now.to_string(), v)
            } else {
                (k, v)
            }
        })
        .collect();
    true
}

// MARK: - Drawn

/// The list a data card's shape draws it as: its face's `list` field and that array's element shape.
fn listed<'a>(project: &Project<'a>, card: &Card) -> Option<(&'a Card, &'a Field, &'a Card)> {
    let shape = instances::shape(project, card)?;
    let name = shape.face.as_ref()?.get("list")?.as_str()?;
    let field = shape.fields.iter().find(|f| f.name == name)?;
    if !field.is_array {
        return None;
    }
    let element = shaped(project, field)?;
    Some((shape, field, element))
}

/// How many rows a data card draws by its shape's list face, every element folded: one an element.
/// None when its shape draws no list.
pub(crate) fn rows_count(project: &Project, card: &Card) -> Option<usize> {
    let (_, field, _) = listed(project, card)?;
    Some(
        card.instance_data()
            .and_then(|d| d.get(&field.name))
            .and_then(Value::as_array)
            .map_or(0, Vec::len),
    )
}

/// A value as a row says it: a text as it is, anything else as its JSON.
fn said_value(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// An enum's value as a head says it: its label, or the value when it has none.
fn labelled(project: &Project, field: &Field, value: &Value) -> String {
    if let (Some(e), Some(text)) = (enum_of(project, field), value.as_str())
        && let Some(v) = e.values.iter().find(|v| v.value == text)
        && !v.label.is_empty()
    {
        return v.label.clone();
    }
    said_value(value)
}

/// A field's name as a row heads it: its label, or its name.
fn heading(field: &Field) -> String {
    if field.label.is_empty() {
        field.name.clone()
    } else {
        field.label.clone()
    }
}

/// An element's line: its face's text, else its key, else its place (`[0]`) — or, `whole` and of a
/// shape with no face, its JSON — and whether it fell back.
fn line_text(
    face: Option<&Map<String, Value>>,
    keys: &[&Field],
    item: &Value,
    i: usize,
    whole: bool,
) -> (String, bool) {
    if let Some(text) = face
        .and_then(|f| f.get("text"))
        .and_then(Value::as_str)
        .and_then(|name| item.get(name))
        .filter(|v| !v.is_null())
    {
        return (said_value(text), false);
    }
    if whole && face.is_none() {
        return (item.to_string(), true);
    }
    if let Some(pick) = instances::key_pick(keys, item) {
        return (key_values(&pick), true);
    }
    (format!("[{i}]"), true)
}

/// A key's values as a line shows them, one after another.
fn key_values(pick: &Pick) -> String {
    match pick {
        Pick::Where(keys) => keys
            .iter()
            .map(|(_, lit)| lit.text.clone())
            .collect::<Vec<_>>()
            .join(", "),
        other => other.written(),
    }
}

/// The lines an element's `fold` opens to, under it (`depth` the level they are drawn at): each fold
/// field's values, its heading on the first; a value one line, an array of values a line each, an
/// array of shaped elements a line each by its shape's face with that face's `fold` a level further.
fn fold_lines(project: &Project, shape: &Card, item: &Value, depth: usize, out: &mut Vec<Value>) {
    let Some(face) = shape.face.as_ref().and_then(Value::as_object) else {
        return;
    };
    let names: Vec<&str> = face
        .get("fold")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    for name in names {
        let Some(field) = shape.fields.iter().find(|f| f.name == name) else {
            continue;
        };
        let Some(value) = item.get(name).filter(|v| !v.is_null()) else {
            continue;
        };
        let mut first = Some(heading(field));
        let mut line = |text: String, at: usize, out: &mut Vec<Value>| {
            let mut row = Map::new();
            row.insert("depth".into(), at.into());
            if let Some(label) = first.take() {
                row.insert("label".into(), label.into());
            }
            row.insert("text".into(), text.into());
            out.push(Value::Object(row));
        };
        let inner = shaped(project, field);
        let items: Vec<&Value> = match value {
            Value::Array(items) if field.is_array => items.iter().collect(),
            Value::Array(items) if items.is_empty() => continue,
            other => vec![other],
        };
        if items.is_empty() {
            continue;
        }
        for (i, element) in items.into_iter().enumerate() {
            match inner {
                Some(inner) if depth + 1 < FOLD_DEPTH && element.is_object() => {
                    let inner_face = inner.face.as_ref().and_then(Value::as_object);
                    let keys = instances::key_fields(&inner.fields);
                    let (text, _) = line_text(inner_face, &keys, element, i, true);
                    line(text, depth, out);
                    fold_lines(project, inner, element, depth + 1, out);
                }
                _ => line(said_value(element), depth, out),
            }
        }
    }
}

/// The list a data card is drawn as when its shape has a list face: its head's fields and its marks
/// counted by role, and a row for each element of the list (its place as the screen's rows name it,
/// its path by its key, its text, note, mark, and the lines its fold opens to). None when its shape
/// draws no list.
pub(crate) fn drawn(project: &Project, card: &Card) -> Option<Value> {
    let (shape, field, element) = listed(project, card)?;
    let data = card.instance_data();
    let face = shape.face.as_ref().and_then(Value::as_object)?;
    let mut header: Vec<Value> = Vec::new();
    for name in face
        .get("header")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
    {
        let (Some(f), Some(value)) = (
            shape.fields.iter().find(|f| f.name == name),
            data.and_then(|d| d.get(name)).filter(|v| !v.is_null()),
        ) else {
            continue;
        };
        header
            .push(json!({"field": name, "label": heading(f), "text": labelled(project, f, value)}));
    }
    let row_face = element.face.as_ref().and_then(Value::as_object);
    let keys = instances::key_fields(&element.fields);
    let mark = row_face
        .and_then(|f| f.get("mark"))
        .and_then(Value::as_object);
    let mark_field = mark.and_then(|m| m.get("field")).and_then(Value::as_str);
    let items: &[Value] = data
        .and_then(|d| d.get(&field.name))
        .and_then(Value::as_array)
        .map_or(&[], Vec::as_slice);
    let base = crate::pathtext::quote(&field.name).into_owned();
    let mut counts: HashMap<&str, usize> = HashMap::new();
    let mut rows: Vec<Value> = Vec::new();
    for (i, item) in items.iter().enumerate() {
        let mut row = Map::new();
        let place = format!("{base}[{i}]");
        let path = match instances::key_pick(&keys, item) {
            Some(pick) if pick.find(items) == super::picks::Found::One(i) => {
                format!("{base}{}", pick.written())
            }
            _ => place.clone(),
        };
        row.insert("at".into(), place.into());
        row.insert("path".into(), path.into());
        let (text, fallback) = line_text(row_face, &keys, item, i, false);
        row.insert("text".into(), text.into());
        if fallback {
            row.insert("fallback".into(), true.into());
        }
        if let Some(note) = row_face
            .and_then(|f| f.get("note"))
            .and_then(Value::as_str)
            .and_then(|n| item.get(n))
            .filter(|v| !v.is_null())
        {
            row.insert("note".into(), said_value(note).into());
        }
        if let Some(mark) = mark {
            let role = match mark_field
                .and_then(|n| item.get(n))
                .filter(|v| !v.is_null())
            {
                Some(value) => mark
                    .get("values")
                    .and_then(|v| v.get(said_value(value)))
                    .and_then(Value::as_str),
                None => mark.get("none").and_then(Value::as_str),
            };
            if let Some(role) = role.filter(|r| ROLES.contains(r)) {
                *counts.entry(role).or_default() += 1;
                row.insert("mark".into(), role.into());
            }
        }
        match row_face {
            Some(_) => {
                let mut lines = Vec::new();
                fold_lines(project, element, item, 0, &mut lines);
                if !lines.is_empty() {
                    row.insert("fold".into(), Value::Array(lines));
                }
            }
            // No face of its own: the element's fields unfold as the tree draws them.
            None => {
                row.insert("tree".into(), true.into());
            }
        }
        rows.push(Value::Object(row));
    }
    let counted: Vec<Value> = ROLES
        .iter()
        .filter_map(|role| counts.get(role).map(|n| json!({"role": role, "count": n})))
        .collect();
    Some(json!({
        "list": field.name,
        "header": header,
        "counts": counted,
        "rows": rows,
    }))
}
