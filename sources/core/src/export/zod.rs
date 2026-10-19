//! Zod 4 schemas — one file of the export settings (`ZodExport`). Objects are
//! `z.strictObject`, as JSON Schema says `additionalProperties: false`.

use super::ident;
use super::{
    COMPILABLE_RULES, CodeFile, Project, Union, card_lines, note_comment, points_at_notes,
    rule_comment, rule_kind, rule_string, sorted,
};
use crate::model::{Card, Field};
use crate::notetext::plain_note;
use crate::ojson;
use std::collections::{HashMap, HashSet};

/// A text of several lines as `// ` comment lines: the text decides where its lines break, as a
/// language's sentences are not as long as another's.
fn line_comments(text: &str) -> String {
    text.lines()
        .map(|line| format!("// {line}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn scalar(kind: &str) -> &'static str {
    match kind {
        "string" | "text" | "decimal" => "z.string()",
        "uuid" => "z.uuid()",
        "url" => "z.url()",
        "email" => "z.email()",
        "date" => "z.iso.date()",
        "datetime" => "z.iso.datetime()",
        "int" => "z.int()",
        "float" => "z.number()",
        "bool" => "z.boolean()",
        "json" => "z.unknown()",
        _ => "z.string()",
    }
}

struct Writer<'p, 'a> {
    project: &'p Project<'a>,
    file: &'p mut CodeFile,
    position: &'p HashMap<String, usize>,
    index: usize,
    used_getters: bool,
    recorded_only: bool,
    /// The card's lines its kinds of line write out in comments.
    lines: super::CardLines,
}

impl Writer<'_, '_> {
    fn expr(&mut self, field: &Field, indent: &str) -> (String, bool) {
        if let Some(fixed) = field.fixed() {
            let mut literal = format!("z.literal({})", ojson::compact(&sorted(fixed)));
            if field.is_array {
                literal = format!("z.array({literal})");
            }
            if field.is_nullable {
                literal += ".nullable()";
            }
            if !field.is_required {
                literal += ".optional()";
            }
            return (literal, false);
        }
        let mut deferred = false;
        let mut text = if field.is_ref() {
            let id = field.ref_schema_id.as_deref();
            match id.and_then(|id| self.project.card(id)) {
                Some(target) if !self.position.contains_key(&target.id) => {
                    // Another file's, imported from it.
                    deferred = true;
                    self.file.type_name(target)
                }
                Some(target) => {
                    let at = self.position[&target.id];
                    deferred = at >= self.index;
                    ident::pascal(&target.name)
                }
                None => "z.unknown()".to_string(),
            }
        } else if field.is_object() {
            let inner = format!("{indent}  ");
            format!(
                "z.strictObject({{\n{}\n{indent}}})",
                self.body(&field.children, &inner)
            )
        } else {
            scalar(&field.kind).to_string()
        };
        let element = text.clone();
        if field.is_array {
            text = format!("z.array({text})");
        }
        text += &refinements(field);
        // The other types it also takes (`also`): a union with its own, in the order JSON Schema says
        // them.
        let mut types = vec![text];
        for t in super::data::type_list("", &field.also, false) {
            let other = match t.as_str() {
                "string" => "z.string()".to_string(),
                "integer" => "z.int()".to_string(),
                "number" => "z.number()".to_string(),
                "boolean" => "z.boolean()".to_string(),
                "object" if !field.is_object() && !field.is_ref() => {
                    "z.record(z.string(), z.unknown())".to_string()
                }
                "array" if !field.is_array => format!("z.array({element})"),
                _ => continue,
            };
            if !types.contains(&other) {
                types.push(other);
            }
        }
        let mut text = if types.len() == 1 {
            types.remove(0)
        } else {
            format!("z.union([{}])", types.join(", "))
        };
        // nullable before optional: null is a value, absence is not
        if field.is_nullable {
            text += ".nullable()";
        }
        if !field.is_required {
            text += ".optional()";
        }
        (text, deferred)
    }

    fn comments(&mut self, field: &Field, indent: &str) -> String {
        let mut notes: Vec<String> = Vec::new();
        if !field.label.is_empty() {
            notes.push(field.label.clone());
        }
        if !field.description.is_empty() {
            notes.push(plain_note(
                &field.description,
                &self.project.work_dir_names(),
            ));
        }
        if let Some(fixed) = field.fixed() {
            notes.push(text!(
                "exportCode.zod.fixed",
                value = ojson::compact(&sorted(fixed))
            ));
        }
        notes.extend(points_at_notes(field));
        notes.extend(self.lines.notes_of(&field.id));
        for rule in &field.rules {
            if !COMPILABLE_RULES.contains(&rule_kind(rule)) {
                self.recorded_only = true;
                notes.push(text!(
                    "exportCode.zod.notGenerated",
                    rule = rule_comment(rule)
                ));
            }
        }
        if field.is_pk {
            notes.push(text!("exportCode.zod.primaryKey"));
        }
        if field.is_unique {
            notes.push(text!("exportCode.zod.unique"));
        }
        notes
            .iter()
            .map(|n| format!("{indent}// {}\n", ident::line_comment(n)))
            .collect()
    }

    fn line(&mut self, field: &Field, indent: &str) -> String {
        let (text, deferred) = self.expr(field, indent);
        let doc = self.comments(field, indent);
        if deferred {
            // Zod 4 getter form: inference keeps working through cycles.
            self.used_getters = true;
            return format!(
                "{doc}{indent}get {}() {{ return {text}; }},",
                ident::key(&field.name)
            );
        }
        format!("{doc}{indent}{}: {text},", ident::key(&field.name))
    }

    fn body(&mut self, list: &[Field], indent: &str) -> String {
        list.iter()
            .map(|f| self.line(f, indent))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// The rules self-contained enough to run.
fn refinements(field: &Field) -> String {
    let mut out = String::new();
    for rule in &field.rules {
        let kind = rule_kind(rule);
        if !COMPILABLE_RULES.contains(&kind) {
            continue;
        }
        if kind == "pattern"
            && let Some(regex) = rule_string(rule, "regex")
        {
            out += &format!(".regex({})", js_regex_literal(regex));
            continue;
        }
        // Ordering and uniqueness only mean something on arrays.
        if !field.is_array {
            continue;
        }
        let pick = rule_string(rule, "field")
            .map(|f| ident::access("v", f))
            .unwrap_or_else(|| "v".into());
        if kind == "uniqueBy" {
            out += &format!(
                ".refine(a => new Set(a.map(v => {pick})).size === a.length, {{ message: {} }})",
                ojson::quote(&text!("exportCode.zod.duplicate", name = field.name))
            );
        }
        if kind == "sortedBy" {
            let descending = rule_string(rule, "order") == Some("desc");
            let comparison = if descending { ">= 0" } else { "<= 0" };
            let previous = rule_string(rule, "field")
                .map(|f| ident::access("arr[i - 1]", f))
                .unwrap_or_else(|| "arr[i - 1]".into());
            out += &format!(
                ".refine(a => a.every((v, i, arr) => i === 0 || String({previous}).localeCompare(String({pick})) {comparison}), {{ message: {} }})",
                ojson::quote(&if descending {
                    text!("exportCode.zod.sortedDescending", name = field.name)
                } else {
                    text!("exportCode.zod.sortedAscending", name = field.name)
                })
            );
        }
    }
    out
}

/// `new RegExp(source).toString()`: a literal, bare `/` escaped, line breaks as their
/// escapes, a last lone `\` doubled.
pub fn js_regex_literal(source: &str) -> String {
    if source.is_empty() {
        return "/(?:)/".into();
    }
    let mut out = String::new();
    let mut escaped = false;
    for c in source.chars() {
        let letter = match c {
            '\n' => Some("n"),
            '\r' => Some("r"),
            '\u{2028}' => Some("u2028"),
            '\u{2029}' => Some("u2029"),
            _ => None,
        };
        if let Some(letter) = letter {
            if !escaped {
                out.push('\\');
            }
            out.push_str(letter);
            escaped = false;
            continue;
        }
        if escaped {
            out.push(c);
            escaped = false;
            continue;
        }
        if c == '\\' {
            out.push('\\');
            escaped = true;
            continue;
        }
        if c == '/' {
            out.push_str("\\/");
            continue;
        }
        out.push(c);
    }
    if escaped {
        out.push('\\');
    }
    format!("/{out}/")
}

/// Dependency order, cycles broken so every card still appears.
fn topo_order<'a>(project: &Project<'a>, members: &HashSet<String>) -> Vec<&'a Card> {
    let cards: Vec<&Card> = project
        .detail
        .schemas
        .iter()
        .map(|c| &**c)
        .filter(|c| members.contains(&c.id))
        .collect();
    let mut deps: HashMap<&str, Vec<&str>> =
        cards.iter().map(|c| (c.id.as_str(), Vec::new())).collect();
    for relation in &project.detail.relations {
        // Only embedded references need their target defined first.
        if relation.from_schema_id == relation.to_schema_id || relation.ref_kind != "embed" {
            continue;
        }
        if !members.contains(&relation.from_schema_id) || !members.contains(&relation.to_schema_id)
        {
            continue;
        }
        let list = deps.get_mut(relation.from_schema_id.as_str()).unwrap();
        if !list.contains(&relation.to_schema_id.as_str()) {
            list.push(relation.to_schema_id.as_str());
        }
    }
    let mut out: Vec<&Card> = Vec::new();
    let mut seen: HashSet<&str> = HashSet::new();
    fn visit<'a>(
        id: &'a str,
        guard: &mut HashSet<&'a str>,
        deps: &HashMap<&'a str, Vec<&'a str>>,
        seen: &mut HashSet<&'a str>,
        out: &mut Vec<&'a str>,
    ) {
        if seen.contains(id) || guard.contains(id) {
            return;
        }
        guard.insert(id);
        for d in deps.get(id).cloned().unwrap_or_default() {
            visit(d, guard, deps, seen, out);
        }
        guard.remove(id);
        if seen.insert(id) {
            out.push(id);
        }
    }
    let mut ids = Vec::new();
    for card in &cards {
        let mut guard = HashSet::new();
        visit(card.id.as_str(), &mut guard, &deps, &mut seen, &mut ids);
    }
    for id in ids {
        if let Some(card) = project.card(id) {
            out.push(card);
        }
    }
    out
}

/// The cards `file` holds, importing what they use from the other files.
pub fn build(project: &Project, file: &mut CodeFile) -> String {
    let ordered = topo_order(project, &file.members.clone());
    let position: HashMap<String, usize> = ordered
        .iter()
        .enumerate()
        .map(|(i, c)| (c.id.clone(), i))
        .collect();
    let mut blocks: Vec<String> = vec!["import { z } from \"zod\";".into()];
    let mut used_getters = false;
    let mut recorded_only = false;

    for (index, card) in ordered.iter().enumerate() {
        if card.kind == "note" {
            blocks.push(note_comment(card, &project.work_dir_names()));
            continue;
        }
        let cname = ident::pascal(&card.name);
        if card.kind == "enum" {
            let list = card
                .values
                .iter()
                .map(|v| ojson::quote(&v.value))
                .collect::<Vec<_>>()
                .join(", ");
            let head_notes = card_lines(project, card).head_notes();
            let described = super::described_values(card);
            if card.enum_open {
                let mut parts = vec![
                    Some(card.label.clone()),
                    Some(plain_note(&card.description, &project.work_dir_names())),
                    Some(text!("exportCode.zod.enumOpen", cname)),
                ];
                parts.extend(head_notes.into_iter().map(Some));
                let doc = ident::doc_comment_lines(&parts, &described, "");
                blocks.push(format!(
                    "{doc}export const {cname}Known = [{list}] as const;\nexport const {cname} = z.string();\nexport type {cname} = z.infer<typeof {cname}>;"
                ));
            } else {
                let mut parts = vec![
                    Some(card.label.clone()),
                    Some(plain_note(&card.description, &project.work_dir_names())),
                ];
                parts.extend(head_notes.into_iter().map(Some));
                let doc = ident::doc_comment_lines(&parts, &described, "");
                blocks.push(format!(
                    "{doc}export const {cname} = z.enum([{list}]);\nexport type {cname} = z.infer<typeof {cname}>;"
                ));
            }
            continue;
        }

        let mut parts = vec![
            Some(card.label.clone()),
            Some(plain_note(&card.description, &project.work_dir_names())),
        ];
        parts.extend(card.rules.iter().map(|r| {
            Some(if COMPILABLE_RULES.contains(&rule_kind(r)) {
                rule_comment(r)
            } else {
                text!("exportCode.zod.notGenerated", rule = rule_comment(r))
            })
        }));
        parts.extend(card.absent_fields.iter().map(|a| {
            Some(text!(
                "exportCode.zod.absent",
                name = a.name,
                reason = plain_note(&a.reason, &project.work_dir_names())
            ))
        }));
        let lines = card_lines(project, card);
        parts.extend(lines.head_notes().into_iter().map(Some));
        let head = ident::doc_comment(&parts, "");
        if card
            .rules
            .iter()
            .any(|r| !COMPILABLE_RULES.contains(&rule_kind(r)))
        {
            recorded_only = true;
        }

        let mut writer = Writer {
            project,
            file,
            position: &position,
            index,
            used_getters: false,
            recorded_only: false,
            lines,
        };
        if let Some(union) = Union::of(card, project) {
            let mut branches = Vec::new();
            for value in &union.values {
                let mut members: Vec<&Field> = union.shared.clone();
                members.extend(union.by_value.get(value).cloned().unwrap_or_default());
                let inner = members
                    .iter()
                    .map(|f| writer.line(f, "    "))
                    .collect::<Vec<_>>()
                    .join("\n");
                branches.push(format!(
                    "  z.strictObject({{\n    {}: z.literal({}),\n{}  }}),",
                    ident::key(&union.discriminator),
                    ojson::quote(value),
                    if inner.is_empty() {
                        String::new()
                    } else {
                        inner + "\n"
                    }
                ));
            }
            used_getters |= writer.used_getters;
            recorded_only |= writer.recorded_only;
            blocks.push(format!(
                "{head}export const {cname} = z.discriminatedUnion({}, [\n{}\n]);\nexport type {cname} = z.infer<typeof {cname}>;",
                ojson::quote(&union.discriminator),
                branches.join("\n")
            ));
            continue;
        }
        let shape = format!(
            "z.strictObject({{\n{}\n}})",
            writer.body(&card.fields, "  ")
        );
        used_getters |= writer.used_getters;
        recorded_only |= writer.recorded_only;
        blocks.push(format!(
            "{head}export const {cname} = {shape};\nexport type {cname} = z.infer<typeof {cname}>;"
        ));
    }

    let mut preamble = vec![line_comments(&text!("exportCode.zod.headerStrict"))];
    if used_getters {
        preamble.push(line_comments(&text!("exportCode.zod.headerGetters")));
    }
    if recorded_only {
        preamble.push(line_comments(&text!("exportCode.zod.headerRecorded")));
    }
    let mut first = vec![blocks[0].clone()];
    first.extend(file.import_lines(false));
    blocks[0] = first.join("\n");
    for (i, p) in preamble.into_iter().enumerate() {
        blocks.insert(1 + i, p);
    }
    blocks.join("\n\n") + "\n"
}
