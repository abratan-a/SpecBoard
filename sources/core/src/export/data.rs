//! The data cards written as JSON (the export format `json-data`;
//! `specboard://SpecBoard/Docs/Design:DesignSections` §7): each data card
//! (`instance`) one element of a list, its data as it is — and a form a line of `export: {as: schema}`
//! leaves a field or an element for, written there as JSON Schema: the field's value is replaced by it,
//! an element of an array holds it under the line's `key`. What the project holds as MCP tools
//! (`tools.json`, what `tools/list` and `guide` are made of; `mcp::catalog`) is written so.
//!
//! The forms are written as the part of JSON Schema the tools use — `type`, `enum`, `items`,
//! `description`, `properties`, `required`, in that order: a field's type (a string, a number…), whether it
//! is an array, the other types it takes (`also`), whether it may be null, the enum it embeds, the fields
//! of the object it is or embeds.

use super::Project;
use crate::kinds::ExportAs;
use crate::model::{Card, Field};
use crate::ojson;
use crate::text::Keyed;
use serde_json::{Map, Value};

/// The order the types of a field are written in.
const ORDER: [&str; 7] = [
    "string", "integer", "number", "boolean", "object", "array", "null",
];

/// A field's types, its own and what else it takes (`also`), and null when it may be: in the order
/// tools write them.
pub fn type_list(primary: &str, also: &[String], nullable: bool) -> Vec<String> {
    let mut have: Vec<String> = vec![primary.to_string()];
    for t in also {
        if !have.contains(t) {
            have.push(t.clone());
        }
    }
    if nullable && !have.iter().any(|t| t == "null") {
        have.push("null".into());
    }
    have.sort_by_key(|t| ORDER.iter().position(|o| o == t).unwrap_or(ORDER.len()));
    have
}

/// The JSON type of a field's own kind; none for what may be anything (`json`).
fn json_type(kind: &str) -> Option<&'static str> {
    match kind {
        "int" => Some("integer"),
        "float" | "decimal" => Some("number"),
        "bool" => Some("boolean"),
        "object" => Some("object"),
        "json" => None,
        _ => Some("string"),
    }
}

/// The data cards of these cards (the instances; others are left), in their order, as the text of the
/// file — a list of their data, each form a line of `export: {as: schema}` reaches put in as JSON
/// Schema (`inline_schema`); or every problem found.
pub fn build(project: &Project, cards: &[&Card]) -> Result<String, Vec<Keyed>> {
    let mut list = Vec::new();
    let mut problems = Vec::new();
    for card in cards.iter().filter(|c| c.kind == "instance") {
        let here = project.full(card);
        let Some(data) = card.instance_data().cloned() else {
            problems.push(keyed!("export.data.empty", here));
            continue;
        };
        let mut data = data;
        for line in &card.connections {
            let Some(export) = project
                .defs
                .line(&line.line)
                .and_then(|d| d.export.as_ref())
                .filter(|e| e.as_ == ExportAs::Schema)
            else {
                continue;
            };
            let form = line
                .to
                .as_deref()
                .and_then(|id| project.card(id))
                .filter(|c| matches!(c.kind.as_str(), "entity" | "value"));
            let Some(form) = form else {
                problems.push(keyed!("export.data.noForm", here, line = line.line));
                continue;
            };
            let key = export.key.as_deref().unwrap_or(SCHEMA_KEY);
            if let Err(why) = put(&mut data, &line.from, key, inline_schema(project, form)) {
                problems.push(keyed!(
                    "export.data.lineFrom",
                    here,
                    line = line.line,
                    from = line.from,
                    why
                ));
            }
        }
        list.push(data);
    }
    if !problems.is_empty() {
        return Err(problems);
    }
    Ok(ojson::pretty(&Value::Array(list)) + "\n")
}

/// The key an element holds the schema under when its line names none.
const SCHEMA_KEY: &str = "schema";

/// A form's JSON Schema put into a data card's data at the place a line leaves: a field's value is
/// replaced by it, an element of an array gets it under `key`.
fn put(data: &mut Value, from: &str, key: &str, schema: Value) -> Result<(), Keyed> {
    use crate::tools::picks::{Found, split, step_kept};
    if from.is_empty() {
        return Err(keyed!("export.data.pathNeeded"));
    }
    let names = split(from);
    let mut at: &mut Value = data;
    let last = names.len() - 1;
    for (i, name) in names.iter().enumerate() {
        let (field, pick) = step_kept(name)?;
        if !at.is_object() {
            return Err(keyed!("export.data.notContainer", field));
        }
        let object = at.as_object_mut().expect("an object");
        if !object.contains_key(&*field) {
            if pick.is_some() {
                return Err(keyed!("export.data.noField", field));
            }
            object.insert(field.to_string(), Value::Object(Map::new()));
        }
        at = object.get_mut(&*field).expect("just put");
        if let Some(pick) = pick {
            let Value::Array(items) = at else {
                return Err(keyed!("export.data.notList", field));
            };
            match pick.find(items) {
                Found::One(n) => at = &mut items[n],
                _ => return Err(keyed!("export.data.notOne", name)),
            }
            if i == last {
                let Some(element) = at.as_object_mut() else {
                    return Err(keyed!("export.data.notObject", name));
                };
                element.insert(key.to_string(), schema);
                return Ok(());
            }
        }
    }
    *at = schema;
    Ok(())
}

/// A form (entity or value) as JSON Schema in the part of it the MCP tools use: `type`, `description`,
/// `properties`, `required`, and for the fields `enum` and `items`; every form inline, no `$ref`.
pub fn inline_schema(project: &Project, card: &Card) -> Value {
    object_schema(project, card, 0)
}

/// The deepest an embedded form is followed.
const DEEP: usize = 12;

/// A form (entity or value) as an object: `type`, its description, `properties` and `required`.
fn object_schema(project: &Project, card: &Card, depth: usize) -> Value {
    let mut out = Map::new();
    out.insert("type".into(), "object".into());
    if !card.description.is_empty() {
        out.insert("description".into(), card.description.clone().into());
    }
    fields_into(project, &card.fields, depth, &mut out);
    // The form of the arguments (or of a `value`) always says its properties, none or not.
    if !out.contains_key("properties") {
        out.insert("properties".into(), Value::Object(Map::new()));
        if let Some(required) = out.shift_remove("required") {
            out.insert("required".into(), required);
        }
    }
    Value::Object(out)
}

/// Fields as `properties` and `required` of an object.
fn fields_into(project: &Project, fields: &[Field], depth: usize, out: &mut Map<String, Value>) {
    let mut properties = Map::new();
    let mut required = Vec::new();
    for field in fields {
        properties.insert(field.name.clone(), field_schema(project, field, depth + 1));
        if field.is_required {
            required.push(Value::from(field.name.clone()));
        }
    }
    // Names alone (fields that may be anything, said nothing of): what an object must have, with no
    // list of what it may — an argument checked on its `properties` would turn the rest away.
    let names_only = !fields.is_empty()
        && fields.iter().all(|f| {
            f.kind == "json"
                && f.description.is_empty()
                && !f.is_array
                && !f.is_nullable
                && f.also.is_empty()
                && f.ref_schema_id.is_none()
        });
    if !properties.is_empty() && !names_only {
        out.insert("properties".into(), Value::Object(properties));
    }
    if !required.is_empty() {
        out.insert("required".into(), Value::Array(required));
    }
}

/// One field.
fn field_schema(project: &Project, field: &Field, depth: usize) -> Value {
    // What it is, before arrays and the other types it takes: its type, an enum's values, the fields
    // of an object.
    let mut own = Map::new();
    let embedded = field
        .ref_schema_id
        .as_deref()
        .and_then(|id| project.card(id))
        .filter(|_| field.is_ref());
    match embedded {
        Some(card) if card.kind == "enum" => {
            own.insert("type".into(), "string".into());
            own.insert(
                "enum".into(),
                Value::Array(
                    card.values
                        .iter()
                        .map(|v| Value::from(v.value.clone()))
                        .collect(),
                ),
            );
        }
        Some(card) if depth < DEEP => {
            own.insert("type".into(), "object".into());
            fields_into(project, &card.fields, depth, &mut own);
        }
        Some(_) => {
            own.insert("type".into(), "object".into());
        }
        None if field.is_object() => {
            own.insert("type".into(), "object".into());
            fields_into(project, &field.children, depth, &mut own);
        }
        None => {
            if let Some(t) = json_type(&field.kind) {
                own.insert("type".into(), t.into());
            }
        }
    }
    // What may be anything says no type (and no other).
    let Some(primary) = own.get("type").and_then(Value::as_str).map(str::to_string) else {
        let mut out = Map::new();
        if field.is_array {
            out.insert(
                "type".into(),
                one_or_list(type_list("array", &[], field.is_nullable)),
            );
            out.insert("items".into(), Value::Object(Map::new()));
        }
        if !field.description.is_empty() {
            out.insert("description".into(), field.description.clone().into());
        }
        return Value::Object(out);
    };
    // What the form an array's elements embed says of them.
    let item_says = embedded
        .filter(|c| c.kind != "enum" && !c.description.is_empty())
        .map(|c| c.description.clone());
    let element = |own: &Map<String, Value>| -> Map<String, Value> {
        // An element: its type, what an enum or an object adds, what its form says of it.
        let mut e = Map::new();
        for key in ["type", "enum"] {
            if let Some(v) = own.get(key) {
                e.insert(key.into(), v.clone());
            }
        }
        if let Some(says) = &item_says {
            e.insert("description".into(), says.clone().into());
        }
        for key in ["properties", "required"] {
            if let Some(v) = own.get(key) {
                e.insert(key.into(), v.clone());
            }
        }
        e
    };

    let mut out = Map::new();
    if field.is_array {
        // What else it takes beside the array (a value of its own shape, an object of edits).
        let types = type_list("array", &field.also, field.is_nullable);
        out.insert("type".into(), one_or_list(types));
        out.insert("items".into(), Value::Object(element(&own)));
    } else if field.also.is_empty() {
        let types = type_list(&primary, &[], field.is_nullable);
        let single = types.len() == 1;
        out.insert(
            "type".into(),
            if single {
                primary.clone().into()
            } else {
                one_or_list(types)
            },
        );
        if let Some(v) = own.get("enum") {
            out.insert("enum".into(), with_null(v, field.is_nullable));
        }
    } else {
        let types = type_list(&primary, &field.also, field.is_nullable);
        out.insert("type".into(), one_or_list(types.clone()));
        if let Some(v) = own.get("enum") {
            out.insert("enum".into(), with_null(v, field.is_nullable));
        }
        if types.iter().any(|t| t == "array") {
            out.insert("items".into(), Value::Object(element(&own)));
        }
    }
    if !field.description.is_empty() {
        out.insert("description".into(), field.description.clone().into());
    }
    if !field.is_array {
        for key in ["properties", "required"] {
            if let Some(v) = own.get(key) {
                out.insert(key.into(), v.clone());
            }
        }
    }
    Value::Object(out)
}

/// An enum's values, and null among them when the field may be null.
fn with_null(values: &Value, nullable: bool) -> Value {
    let mut list = values.as_array().cloned().unwrap_or_default();
    if nullable && !list.iter().any(Value::is_null) {
        list.push(Value::Null);
    }
    Value::Array(list)
}

/// A type as `"string"`, or a list when there are several.
fn one_or_list(types: Vec<String>) -> Value {
    if types.len() == 1 {
        types[0].clone().into()
    } else {
        Value::Array(types.into_iter().map(Value::from).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_types_of_a_field_are_in_the_order_tools_write_them() {
        let list = |p: &str, also: &[&str], null: bool| {
            type_list(
                p,
                &also.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
                null,
            )
        };
        assert_eq!(list("string", &["array"], false), ["string", "array"]);
        assert_eq!(list("string", &[], true), ["string", "null"]);
        assert_eq!(list("object", &["string"], false), ["string", "object"]);
        assert_eq!(list("string", &["integer"], false), ["string", "integer"]);
        assert_eq!(
            list("boolean", &["string", "array"], true),
            ["string", "boolean", "array", "null"]
        );
        assert_eq!(list("string", &["string"], false), ["string"]);
    }
}
