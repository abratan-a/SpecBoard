//! TypeScript types — one file of the export settings (`TypeScriptExport`).
//!
//! Nesting is taken out into its own `interface`; a reference keeps the target's type;
//! what a value merely designates keeps its own type and says what it points at.

use super::ident;
use super::{
    CodeFile, Project, Union, card_lines, note_comment, points_at_notes, rule_comment, sorted,
};
use crate::model::{Card, Field};
use crate::notetext::plain_note;
use crate::ojson;
use std::collections::{HashMap, HashSet};

fn scalar(kind: &str) -> &'static str {
    match kind {
        "string" | "text" | "uuid" | "url" | "email" | "date" | "datetime" | "decimal" => "string",
        "int" | "float" => "number",
        "bool" => "boolean",
        "json" => "unknown",
        _ => "string",
    }
}

struct Writer<'p, 'a> {
    project: &'p Project<'a>,
    file: &'p mut CodeFile,
    card: &'a Card,
    taken: &'p mut HashSet<String>,
    extracted: Vec<String>,
    nested: HashMap<String, String>,
    /// The card's lines its kinds of line write out in comments.
    lines: super::CardLines,
}

impl Writer<'_, '_> {
    fn type_of(&mut self, field: &Field, path: &[String]) -> String {
        // A constant is the type: it narrows to one value.
        if let Some(fixed) = field.fixed() {
            let literal = ojson::pretty(&sorted(fixed));
            return if field.is_array {
                format!("{literal}[]")
            } else {
                literal
            };
        }
        let mut text = if field.is_ref() {
            match field
                .ref_schema_id
                .as_deref()
                .and_then(|id| self.project.card(id))
            {
                Some(target) => {
                    let name = self.file.type_name(target);
                    self.taken.insert(name.clone());
                    name
                }
                None => "unknown".to_string(),
            }
        } else if field.is_object() {
            if let Some(known) = self.nested.get(&field.id) {
                known.clone()
            } else {
                let mut inner_path = path.to_vec();
                inner_path.push(field.name.clone());
                let name =
                    ident::nested_type(&self.card.name, &inner_path, field.is_array, self.taken);
                self.nested.insert(field.id.clone(), name.clone());
                let doc = ident::doc_comment(
                    &[
                        Some(field.label.clone()),
                        Some(text!(
                            "exportCode.typescript.nested",
                            card = self.card.name,
                            path =
                                crate::pathtext::join_names(inner_path.iter().map(String::as_str))
                        )),
                    ],
                    "",
                );
                let body = self.body(&field.children, &inner_path);
                self.extracted
                    .push(format!("{doc}export interface {name} {{\n{body}\n}}"));
                name
            }
        } else {
            scalar(&field.kind).to_string()
        };
        let element = text.clone();
        if field.is_array {
            text = format!("{text}[]");
        }
        // The other types it also takes (`also`), after its own, in the order JSON Schema says them.
        let mut types = vec![text];
        for t in super::data::type_list("", &field.also, false) {
            let other = match t.as_str() {
                "string" => "string".to_string(),
                "integer" | "number" => "number".to_string(),
                "boolean" => "boolean".to_string(),
                "object" if !field.is_object() && !field.is_ref() => {
                    "Record<string, unknown>".to_string()
                }
                "array" if !field.is_array => format!("{element}[]"),
                _ => continue,
            };
            if !types.contains(&other) {
                types.push(other);
            }
        }
        if field.is_nullable {
            types.push("null".to_string());
        }
        types.join(" | ")
    }

    fn notes(&self, field: &Field) -> Vec<Option<String>> {
        let mut out = vec![
            Some(field.label.clone()),
            Some(plain_note(
                &field.description,
                &self.project.work_dir_names(),
            )),
        ];
        if let Some(fixed) = field.fixed() {
            out.push(Some(text!(
                "exportCode.typescript.fixed",
                value = ojson::pretty(&sorted(fixed))
            )));
        }
        out.extend(points_at_notes(field).into_iter().map(Some));
        out.extend(self.lines.notes_of(&field.id).into_iter().map(Some));
        out.extend(field.rules.iter().map(|r| Some(rule_comment(r))));
        if field.is_pk {
            out.push(Some(text!("exportCode.typescript.primaryKey")));
        }
        if field.is_unique {
            out.push(Some(text!("exportCode.typescript.unique")));
        }
        out
    }

    fn line(&mut self, field: &Field, path: &[String], indent: &str) -> String {
        let doc = ident::doc_comment(&self.notes(field), indent);
        let kind = self.type_of(field, path);
        format!(
            "{doc}{indent}{}{}: {kind};",
            ident::key(&field.name),
            if field.is_required { "" } else { "?" }
        )
    }

    fn body(&mut self, list: &[Field], path: &[String]) -> String {
        list.iter()
            .map(|f| self.line(f, path, "  "))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// The cards `file` holds, importing what they use from the other files.
pub fn build(project: &Project, file: &mut CodeFile) -> String {
    let mut blocks: Vec<String> = Vec::new();
    let emitted: Vec<&Card> = project
        .detail
        .schemas
        .iter()
        .map(|c| &**c)
        .filter(|c| file.contains(c))
        .collect();
    // Every top-level name is reserved before nested ones are minted.
    let mut taken: HashSet<String> = emitted
        .iter()
        .filter(|c| c.is_shape())
        .map(|c| ident::pascal(&c.name))
        .collect();

    for card in emitted {
        // A note in the file: a comment where it sits among the types.
        if card.kind == "note" {
            blocks.push(note_comment(card, &project.work_dir_names()));
            continue;
        }
        if card.kind == "enum" {
            let listed: Vec<String> = card.values.iter().map(|v| ojson::quote(&v.value)).collect();
            let union = if card.enum_open {
                let mut all = listed.clone();
                all.push("(string & {})".into());
                all.join(" | ")
            } else if listed.is_empty() {
                "never".to_string()
            } else {
                listed.join(" | ")
            };
            let mut parts = vec![
                Some(card.label.clone()),
                Some(plain_note(&card.description, &project.work_dir_names())),
                card.enum_open
                    .then(|| text!("exportCode.typescript.enumOpen")),
            ];
            parts.extend(card_lines(project, card).head_notes().into_iter().map(Some));
            let doc = ident::doc_comment_lines(&parts, &super::described_values(card), "");
            blocks.push(format!(
                "{doc}export type {} = {union};",
                ident::pascal(&card.name)
            ));
            continue;
        }

        let lines = card_lines(project, card);
        let head_notes = lines.head_notes();
        let mut writer = Writer {
            project,
            file,
            card,
            taken: &mut taken,
            extracted: Vec::new(),
            nested: HashMap::new(),
            lines,
        };
        if let Some(union) = Union::of(card, project) {
            let mut branches = Vec::new();
            for value in &union.values {
                let mut members: Vec<&Field> = union.shared.clone();
                members.extend(union.by_value.get(value).cloned().unwrap_or_default());
                let inner = members
                    .iter()
                    .map(|f| writer.line(f, &[], "      "))
                    .collect::<Vec<_>>()
                    .join("\n");
                branches.push(format!(
                    "  | {{\n      {}: {};\n{}    }}",
                    ident::key(&union.discriminator),
                    ojson::quote(value),
                    if inner.is_empty() {
                        String::new()
                    } else {
                        inner + "\n"
                    }
                ));
            }
            let extracted = std::mem::take(&mut writer.extracted);
            blocks.extend(extracted);
            let mut parts = vec![
                Some(card.label.clone()),
                Some(plain_note(&card.description, &project.work_dir_names())),
                Some(text!(
                    "exportCode.typescript.discriminator",
                    name = union.discriminator
                )),
            ];
            parts.extend(head_notes.iter().cloned().map(Some));
            let doc = ident::doc_comment(&parts, "");
            blocks.push(format!(
                "{doc}export type {} =\n{};",
                ident::pascal(&card.name),
                branches.join("\n")
            ));
            continue;
        }

        let rendered = writer.body(&card.fields, &[]);
        let extracted = std::mem::take(&mut writer.extracted);
        blocks.extend(extracted);
        let mut parts = vec![
            Some(card.label.clone()),
            Some(plain_note(&card.description, &project.work_dir_names())),
        ];
        parts.extend(card.rules.iter().map(|r| Some(rule_comment(r))));
        parts.extend(card.absent_fields.iter().map(|a| {
            Some(text!(
                "exportCode.typescript.absent",
                name = a.name,
                reason = plain_note(&a.reason, &project.work_dir_names())
            ))
        }));
        parts.extend(head_notes.into_iter().map(Some));
        let head = ident::doc_comment(&parts, "");
        blocks.push(format!(
            "{head}export interface {} {{\n{rendered}\n}}",
            ident::pascal(&card.name)
        ));
    }

    let head = file.import_lines(true);
    if !head.is_empty() {
        blocks.insert(0, head.join("\n"));
    }
    blocks.join("\n\n") + "\n"
}
