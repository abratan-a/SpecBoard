//! JSON Schema (draft 2020-12): one schema, which the file puts under `$defs` with the
//! others of its file (`JSONSchemaExport`).

use super::{Project, Union, points_at_notes, rule_kind, rule_params, rule_path_params, sorted};
use crate::model::{Card, Field, Rule};
use crate::notetext::{plain_note, plain_parts, plain_path};
use serde_json::{Map, Value, json};

/// Where a schema sits among the files written: its file, and the reference from it to
/// a card written as JSON Schema in the same export directory.
pub struct Placement<'r> {
    pub id: String,
    pub refer: &'r dyn Fn(&str) -> Option<String>,
}

fn scalar(kind: &str) -> Vec<(&'static str, &'static str)> {
    match kind {
        "string" | "text" => vec![("type", "string")],
        "int" => vec![("type", "integer")],
        "float" => vec![("type", "number")],
        "decimal" => vec![("type", "string"), ("format", "decimal")],
        "bool" => vec![("type", "boolean")],
        "uuid" => vec![("type", "string"), ("format", "uuid")],
        "url" => vec![("type", "string"), ("format", "uri")],
        "email" => vec![("type", "string"), ("format", "email")],
        "date" => vec![("type", "string"), ("format", "date")],
        "datetime" => vec![("type", "string"), ("format", "date-time")],
        "json" => vec![],
        _ => vec![("type", "string")],
    }
}

fn strings(list: &[String]) -> Value {
    Value::Array(list.iter().map(|s| Value::String(s.clone())).collect())
}

struct Writer<'p, 'a> {
    project: &'p Project<'a>,
    base: String,
    placement: Option<&'p Placement<'p>>,
    /// The card's lines its kinds of line write out (comments, annotations).
    lines: super::CardLines,
}

impl Writer<'_, '_> {
    /// Lines written out as annotations (`x-…`), each named by its key: the card it reaches
    /// and where it lands (as a path is written out), what it says and its fields.
    fn annotations(&self, lines: &[super::OutLine], into: &mut Map<String, Value>) {
        for line in lines {
            let Some(key) = &line.key else {
                continue;
            };
            let mut entry = Map::new();
            let reached = match (&line.landing, &line.to) {
                (Some(landing), _) => Some(self.pointer(landing)),
                (None, Some(id)) => self
                    .project
                    .card(id)
                    .map(|c| self.pointer(&self.project.full(c))),
                (None, None) => None,
            };
            match reached {
                Some(Value::Object(at)) => entry.extend(at),
                Some(other) => {
                    entry.insert("schema".into(), other);
                }
                None => {}
            }
            if let Some(d) = &line.description {
                entry.insert(
                    "description".into(),
                    plain_note(d, &self.project.work_dir_names()).into(),
                );
            }
            if !line.fields.is_empty() {
                entry.insert("fields".into(), sorted(&Value::Object(line.fields.clone())));
            }
            let list = into
                .entry(key.clone())
                .or_insert_with(|| Value::Array(Vec::new()));
            if let Value::Array(list) = list {
                list.push(Value::Object(entry));
            }
        }
    }

    /// A path of the project as it is written out: `{schema, field}`, never the app's path.
    fn pointer(&self, stored: &str) -> Value {
        let (card, _, field) = plain_parts(stored);
        let Some(target) = self.project.find(&card, &self.base) else {
            return Value::String(plain_path(stored));
        };
        let schema = self
            .placement
            .and_then(|p| (p.refer)(&target.id))
            .unwrap_or_else(|| target.name.clone());
        let mut out = Map::new();
        out.insert("schema".into(), schema.into());
        if let Some(f) = field {
            out.insert("field".into(), f.into());
        }
        Value::Object(out)
    }

    fn rules(&self, list: &[Rule]) -> Value {
        Value::Array(
            list.iter()
                .map(|rule| {
                    let kind = rule_kind(rule);
                    let paths = rule_path_params(kind);
                    let mut out = Map::new();
                    out.insert("kind".into(), kind.into());
                    for (key, value) in rule_params(rule) {
                        let written = match value {
                            Value::String(path) if paths.contains(&key.as_str()) => {
                                self.pointer(path)
                            }
                            other => sorted(other),
                        };
                        out.insert(key.clone(), written);
                    }
                    Value::Object(out)
                })
                .collect(),
        )
    }

    fn leaf(&self, field: &Field) -> Value {
        let mut leaf = Map::new();
        if field.is_ref() {
            if let Some(id) = &field.ref_schema_id {
                let reference = match self.placement {
                    Some(p) => (p.refer)(id),
                    None => self.project.card(id).map(|c| {
                        format!(
                            "./{}.schema.json",
                            super::percent_encode(&super::folder_name(&c.name))
                        )
                    }),
                };
                if let Some(r) = reference {
                    leaf.insert("$ref".into(), r.into());
                }
            }
        } else if field.is_object() {
            leaf.insert("type".into(), "object".into());
            for (k, v) in self.properties(&field.children) {
                leaf.insert(k, v);
            }
            leaf.insert("additionalProperties".into(), false.into());
            if !field.label.is_empty() {
                leaf.insert("title".into(), field.label.clone().into());
            }
        } else {
            for (k, v) in scalar(&field.kind) {
                leaf.insert(k.into(), v.into());
            }
        }

        // A fixed value pins the field; the type stays for readers that ignore const.
        if let Some(fixed) = field.fixed() {
            leaf.insert("const".into(), sorted(fixed));
        }

        if !field.is_array
            && !field.also.is_empty()
            && let Some(Value::String(t)) = leaf.get("type").cloned()
        {
            // Other types it takes: the types a list, an array of its type as `items`.
            let list = super::data::type_list(&t, &field.also, field.is_nullable);
            if list.iter().any(|x| x == "array") {
                let mut item = Map::new();
                item.insert("type".into(), t.clone().into());
                leaf.insert("items".into(), Value::Object(item));
            }
            leaf.insert("type".into(), json!(list));
        } else if field.is_nullable && !field.is_array {
            if let Some(Value::String(t)) = leaf.get("type").cloned() {
                leaf.insert("type".into(), json!([t, "null"]));
            } else if let Some(Value::String(r)) = leaf.get("$ref").cloned() {
                let mut replaced = Map::new();
                replaced.insert("anyOf".into(), json!([{"$ref": r}, {"type": "null"}]));
                leaf = replaced;
            } else if !leaf.contains_key("type") {
                leaf.insert(
                    "type".into(),
                    json!(["null", "string", "number", "boolean", "object", "array"]),
                );
            }
        }

        if !field.points_at.is_empty() {
            let entries: Vec<Value> = field
                .points_at
                .iter()
                .map(|p| {
                    let mut entry = Map::new();
                    if let Some(schema) = &p.schema {
                        let to = self.pointer(schema);
                        let schema_value = match &to {
                            Value::Object(o) => o.get("schema").cloned().unwrap_or(Value::Null),
                            other => other.clone(),
                        };
                        entry.insert("schema".into(), schema_value);
                    }
                    entry.insert("as".into(), p.designation.clone().into());
                    if let Some(i) = &p.exists_in {
                        entry.insert("in".into(), self.pointer(i));
                    }
                    if let Some(w) = &p.when {
                        entry.insert("when".into(), json!({"field": w.field, "equals": w.equals}));
                    }
                    if let Some(by) = &p.discriminated_by {
                        entry.insert("discriminatedBy".into(), by.clone().into());
                    }
                    if let Some(there) = &p.discriminator_field {
                        entry.insert("discriminatorField".into(), there.clone().into());
                    }
                    Value::Object(entry)
                })
                .collect();
            leaf.insert("x-pointsAt".into(), Value::Array(entries));
            if field.description.is_empty() {
                leaf.insert(
                    "description".into(),
                    points_at_notes(field).join(" / ").into(),
                );
            }
        }
        if !field.description.is_empty() {
            leaf.insert(
                "description".into(),
                plain_note(&field.description, &self.project.work_dir_names()).into(),
            );
        }
        // What its lines say, after what the field says itself.
        let notes = self.lines.notes_of(&field.id);
        if !notes.is_empty() {
            let said = leaf
                .get("description")
                .and_then(Value::as_str)
                .map(str::to_string);
            let all: Vec<String> = said.into_iter().chain(notes).collect();
            leaf.insert("description".into(), all.join(" / ").into());
        }
        if let Some(lines) = self.lines.fields.get(&field.id) {
            self.annotations(lines, &mut leaf);
        }

        let mut wrapped = if field.is_array {
            let mut w = Map::new();
            // Null is the array's, not its elements'.
            if field.is_nullable {
                w.insert("type".into(), json!(["array", "null"]));
            } else {
                w.insert("type".into(), "array".into());
            }
            w.insert("items".into(), Value::Object(leaf));
            w
        } else {
            leaf
        };

        if !field.rules.is_empty() {
            // uniqueBy over whole elements is native; the rest rides along as x-rules.
            if field.is_array
                && field
                    .rules
                    .iter()
                    .any(|r| rule_kind(r) == "uniqueBy" && !r.contains_key("field"))
            {
                wrapped.insert("uniqueItems".into(), true.into());
            }
            wrapped.insert("x-rules".into(), self.rules(&field.rules));
        }
        if !field.variants.is_empty() {
            wrapped.insert("x-variants".into(), strings(&field.variants));
        }
        if field.is_pk {
            wrapped.insert("x-primaryKey".into(), true.into());
        }
        if field.is_unique {
            wrapped.insert("x-unique".into(), true.into());
        }
        Value::Object(wrapped)
    }

    fn properties(&self, list: &[Field]) -> Map<String, Value> {
        let mut props = Map::new();
        let mut required: Vec<String> = Vec::new();
        for field in list {
            props.insert(field.name.clone(), self.leaf(field));
            if field.is_required {
                required.push(field.name.clone());
            }
        }
        let mut out = Map::new();
        out.insert("properties".into(), Value::Object(props));
        if !required.is_empty() {
            out.insert("required".into(), strings(&required));
        }
        out
    }
}

pub fn build(card: &Card, project: &Project, placement: Option<&Placement>) -> Value {
    let writer = Writer {
        project,
        base: project.section_of(card),
        placement,
        lines: super::card_lines(project, card),
    };
    let mut head = Map::new();
    head.insert(
        "$schema".into(),
        "https://json-schema.org/draft/2020-12/schema".into(),
    );
    head.insert(
        "$id".into(),
        placement
            .map(|p| p.id.clone())
            .unwrap_or_else(|| format!("{}.schema.json", super::folder_name(&card.name)))
            .into(),
    );
    head.insert("title".into(), card.name.clone().into());
    // What the card's lines from its head say, after what the card says itself.
    let about = [
        card.label.clone(),
        plain_note(&card.description, &project.work_dir_names()),
    ]
    .into_iter()
    .chain(writer.lines.head_notes())
    .filter(|s| !s.is_empty())
    .collect::<Vec<_>>()
    .join(" — ");
    if !about.is_empty() {
        head.insert("description".into(), about.clone().into());
    }
    if !card.metadata.is_empty() {
        head.insert(
            "x-metadata".into(),
            sorted(&Value::Object(card.metadata.clone())),
        );
    }
    writer.annotations(&writer.lines.head, &mut head);
    if !card.rules.is_empty() {
        head.insert("x-rules".into(), writer.rules(&card.rules));
    }
    if !card.absent_fields.is_empty() {
        head.insert(
            "x-absent-fields".into(),
            Value::Array(
                card.absent_fields
                    .iter()
                    .map(|a| json!({"name": a.name, "reason": plain_note(&a.reason, &project.work_dir_names())}))
                    .collect(),
            ),
        );
    }
    if !card.payloads.is_empty() {
        head.insert(
            "x-payloads".into(),
            Value::Array(
                card.payloads
                    .iter()
                    .map(|p| {
                        let mut out = Map::new();
                        out.insert("name".into(), p.name.clone().into());
                        out.insert("excludes".into(), strings(&p.excludes));
                        if let Some(v) = &p.value_at {
                            out.insert("valueAt".into(), v.clone().into());
                        }
                        if !p.signed_by.is_empty() {
                            out.insert(
                                "signedBy".into(),
                                Value::Array(
                                    p.signed_by.iter().map(|k| writer.pointer(k)).collect(),
                                ),
                            );
                        }
                        if !p.verified_by.is_empty() {
                            out.insert(
                                "verifiedBy".into(),
                                Value::Array(
                                    p.verified_by.iter().map(|k| writer.pointer(k)).collect(),
                                ),
                            );
                        }
                        Value::Object(out)
                    })
                    .collect(),
            ),
        );
    }

    if card.kind == "enum" {
        let values: Vec<String> = card.values.iter().map(|v| v.value.clone()).collect();
        let mut out = head;
        out.insert("type".into(), "string".into());
        if card.enum_open {
            out.insert("x-enum-open".into(), true.into());
            out.insert("x-known-values".into(), strings(&values));
            // This replaces the description above, keeping its place.
            out.insert(
                "description".into(),
                format!(
                    "{}{}",
                    if about.is_empty() {
                        String::new()
                    } else {
                        format!("{about} — ")
                    },
                    text!("exportCode.jsonSchema.enumOpen")
                )
                .into(),
            );
        } else {
            out.insert("enum".into(), strings(&values));
        }
        // What each value stands for, beside the list (`enum` keeps its shape): only those said.
        let described: Map<String, Value> = card
            .values
            .iter()
            .filter(|v| !v.description.is_empty())
            .map(|v| (v.value.clone(), v.description.clone().into()))
            .collect();
        if !described.is_empty() {
            out.insert("x-enum-descriptions".into(), Value::Object(described));
        }
        return Value::Object(out);
    }

    if let Some(union) = Union::of(card, project) {
        let branches: Vec<Value> = union
            .values
            .iter()
            .map(|value| {
                let mut members: Vec<&Field> = union.shared.clone();
                members.extend(union.by_value.get(value).cloned().unwrap_or_default());
                let mut props = Map::new();
                props.insert(
                    union.discriminator.clone(),
                    json!({"type": "string", "const": value}),
                );
                let mut required = vec![union.discriminator.clone()];
                for field in members {
                    props.insert(field.name.clone(), writer.leaf(field));
                    if field.is_required {
                        required.push(field.name.clone());
                    }
                }
                let mut branch = Map::new();
                branch.insert("type".into(), "object".into());
                branch.insert("properties".into(), Value::Object(props));
                branch.insert("required".into(), strings(&required));
                branch.insert("additionalProperties".into(), false.into());
                Value::Object(branch)
            })
            .collect();
        let mut out = head;
        out.insert("x-discriminator".into(), union.discriminator.clone().into());
        out.insert("oneOf".into(), Value::Array(branches));
        return Value::Object(out);
    }

    let mut out = head;
    out.insert("type".into(), "object".into());
    for (k, v) in writer.properties(&card.fields) {
        out.insert(k, v);
    }
    out.insert("additionalProperties".into(), false.into());
    Value::Object(out)
}
