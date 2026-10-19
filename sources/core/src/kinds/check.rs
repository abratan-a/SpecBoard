//! Checking a card's document against its kind: what each field holds, which fields
//! apply where (`when`), which are required, what may not be written, how rows sit under
//! one another and how deep. Whether a path leads anywhere, and the checks only a kind's
//! own code makes (a flow's nodes joining up, a rule's parameters), come with writing.
//!
//! A document is `{id, kind, name, section, v, fields, rows}`: `fields` the card's own
//! fields by name, `rows` its rows by the name of their kind, each row `{id, fields,
//! rows}`. Every problem found is returned, not just the first.

use super::{Address, Child, Cond, Field, FieldType, Kind, Kinds, PageKind, Row};
use crate::text::Keyed;
use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};
use std::fmt;

/// One thing wrong, and where.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    /// Where in the card: a field, or a row by its address (`fields.id.type`). Empty for
    /// the card itself.
    pub at: String,
    /// What is wrong, kept by key: the check tells its items apart by what they are, not by how
    /// they are said (`tools::inspection`).
    pub message: Keyed,
    /// A field or a kind of row the document holds that its kind does not (any more): what
    /// a project's own kind leaves behind when its definition changes (「定義に無い欄」),
    /// kept and not refused for one of those (`strays`).
    pub stray: bool,
}

impl Problem {
    pub(crate) fn new(at: &str, message: Keyed) -> Problem {
        Problem {
            at: at.to_string(),
            message,
            stray: false,
        }
    }

    fn stray(at: &str, message: Keyed) -> Problem {
        Problem {
            stray: true,
            ..Problem::new(at, message)
        }
    }
}

impl Problem {
    /// Said as `Display` says it, kept by key.
    pub fn keyed(&self) -> Keyed {
        if self.at.is_empty() {
            self.message.clone()
        } else {
            keyed!("kindCheck.problem.at", at = self.at, message = self.message)
        }
    }
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.at.is_empty() {
            write!(f, "{}", self.message)
        } else {
            write!(f, "{}: {}", self.at, self.message)
        }
    }
}

/// A name a path is made of — a card's, a field's, a row's, a node's: 1 to 255 characters, no
/// control character, no whitespace around it (`pathtext::name_problem`). Quoted in a path where
/// it needs it.
pub fn path_name(name: &str) -> bool {
    crate::pathtext::name_problem(name).is_none()
}

/// A name of a kind: a letter or `_` first, then letters, digits and `_`, at most 80
/// (`Validation.swift`'s identifier).
pub fn identifier(name: &str) -> bool {
    let mut chars = name.chars();
    name.len() <= 80
        && chars
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// The keys of a card's document. Its lines (`connections`) are checked against the project's
/// kinds of line apart (`check_connections`).
const CARD_KEYS: [&str; 10] = [
    "id",
    "kind",
    "name",
    "section",
    "in",
    "v",
    "order",
    "fields",
    "rows",
    "connections",
];

/// Every problem with a card's document.
pub fn check_card(kinds: &Kinds, doc: &Value) -> Vec<Problem> {
    let mut problems = Vec::new();
    let Some(doc) = doc.as_object() else {
        return vec![Problem::new("", keyed!("kindCheck.card.notObject"))];
    };
    for key in doc.keys() {
        if !CARD_KEYS.contains(&key.as_str()) {
            problems.push(Problem::new(
                "",
                keyed!("kindCheck.card.keyNotAllowed", key = key),
            ));
        }
    }
    for key in ["id", "section"] {
        if doc
            .get(key)
            .and_then(Value::as_str)
            .is_none_or(str::is_empty)
        {
            problems.push(Problem::new("", keyed!("kindCheck.card.missing", key)));
        }
    }
    match doc.get("name").and_then(Value::as_str) {
        Some(name) if path_name(name) => {}
        Some(name) => problems.push(Problem::new("", keyed!("kindCheck.card.nameForm", name))),
        None => problems.push(Problem::new("", keyed!("kindCheck.card.noName"))),
    }
    if doc
        .get("in")
        .is_some_and(|i| i.as_str().is_none_or(str::is_empty))
    {
        problems.push(Problem::new("", keyed!("kindCheck.card.inNotText")));
    }
    if doc.get("v").and_then(Value::as_u64).is_none_or(|v| v < 1) {
        problems.push(Problem::new("", keyed!("kindCheck.card.noVersion")));
    }
    let kind = match doc.get("kind").and_then(Value::as_str) {
        None => {
            problems.push(Problem::new("", keyed!("kindCheck.card.noKind")));
            return problems;
        }
        Some(name) => match kinds.get(name) {
            None => {
                problems.push(Problem::new("", keyed!("kindCheck.card.unknownKind", name)));
                return problems;
            }
            Some(kind) if kind.is_abstract => {
                problems.push(Problem::new(
                    "",
                    keyed!("kindCheck.card.abstractKind", name),
                ));
                return problems;
            }
            Some(kind) => kind,
        },
    };
    if doc.get("order").is_some_and(|o| !o.is_i64()) {
        problems.push(Problem::new("", keyed!("kindCheck.card.orderForm")));
    }
    // A card ghost is a name and what it points at: the lines are its card's.
    if kind.name == crate::detail::GHOST && doc.contains_key("connections") {
        problems.push(Problem::new("", keyed!("kindCheck.card.ghostNoLines")));
    }
    let empty = Map::new();
    match doc.get("fields") {
        None => check_fields(&kind.fields, &empty, "", &mut problems),
        Some(Value::Object(fields)) => check_fields(&kind.fields, fields, "", &mut problems),
        Some(_) => problems.push(Problem::new("", keyed!("kindCheck.doc.fieldsNotObject"))),
    }
    match doc.get("rows") {
        None => {}
        Some(Value::Object(rows)) => check_card_rows(kind, rows, &mut problems),
        Some(_) => problems.push(Problem::new("", keyed!("kindCheck.doc.rowsNotObject"))),
    }
    // A field its face draws as a picture holds one: a text that is not would draw nothing.
    if let Some(page) = kind.face.as_ref().and_then(|f| f.page.as_ref())
        && page.drawn == PageKind::Svg
        && let Some(text) = doc
            .get("fields")
            .and_then(|f| f.get(&page.field))
            .and_then(Value::as_str)
        && let Some(why) = super::svg::problem(text)
    {
        problems.push(Problem::new(&page.field, why));
    }
    problems
}

// MARK: - What the definition no longer has

/// The fields and kinds of row a card's document holds that its kind does not (「定義に無い欄」):
/// each by where it is (`old`, `items`, `items.a.memo` — as
/// a problem says it) and its value as MCP reads it (a row's fields and the rows under it by
/// name, no ids). What a project's own kind leaves behind when a field is taken out of its
/// definition or renamed without its values.
pub fn strays(kind: &Kind, doc: &Value) -> Vec<(String, Value)> {
    let mut found = Vec::new();
    if let Some(doc) = doc.as_object() {
        let mut copy = doc.clone();
        stray_walk(kind, &mut copy, &mut |at, value| {
            found.push((at.to_string(), value.clone()));
            false
        });
    }
    found
}

/// The strays of a card's document taken out of it: those `which` names (by where they
/// are), or every one. Where each was, in the order found.
pub fn drop_strays(
    kind: &Kind,
    doc: &mut Map<String, Value>,
    which: Option<&HashSet<String>>,
) -> Vec<String> {
    let mut dropped = Vec::new();
    stray_walk(kind, doc, &mut |at, _| {
        let drop = which.is_none_or(|w| w.contains(at));
        if drop {
            dropped.push(at.to_string());
        }
        drop
    });
    dropped
}

/// Rows of the document's form as MCP reads them: a row's fields and the rows under it by
/// name, without their ids.
fn rows_flat(value: &Value) -> Value {
    match value {
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|item| {
                    let mut out = Map::new();
                    if let Some(fields) = item.get("fields").and_then(Value::as_object) {
                        out.extend(fields.clone());
                    }
                    if let Some(rows) = item.get("rows").and_then(Value::as_object) {
                        for (name, list) in rows {
                            out.insert(name.clone(), rows_flat(list));
                        }
                    }
                    Value::Object(out)
                })
                .collect(),
        ),
        other => other.clone(),
    }
}

/// Every stray of a document, each told to `visit` (where, and its value as MCP reads it),
/// taken out when `visit` says so.
fn stray_walk(
    kind: &Kind,
    doc: &mut Map<String, Value>,
    visit: &mut dyn FnMut(&str, &Value) -> bool,
) {
    if let Some(Value::Object(fields)) = doc.get_mut("fields") {
        stray_fields(&kind.fields, fields, "", visit);
        if fields.is_empty() {
            doc.remove("fields");
        }
    }
    if let Some(Value::Object(rows)) = doc.get_mut("rows") {
        let names: Vec<String> = rows.keys().cloned().collect();
        for name in names {
            match kind.row(&name) {
                None => {
                    let flat = rows_flat(&rows[&name]);
                    if visit(&name, &flat) {
                        rows.remove(&name);
                    }
                }
                Some(row) => {
                    if let Some(items) = rows.get_mut(&name) {
                        stray_rows(row, items, &name, &[row], visit);
                    }
                }
            }
        }
        if rows.is_empty() {
            doc.remove("rows");
        }
    }
}

fn stray_fields(
    defs: &[Field],
    values: &mut Map<String, Value>,
    at: &str,
    visit: &mut dyn FnMut(&str, &Value) -> bool,
) {
    let names: Vec<String> = values.keys().cloned().collect();
    for name in names {
        if defs.iter().all(|d| d.name != name) && visit(&join(at, &name), &values[&name]) {
            values.remove(&name);
        }
    }
}

fn stray_rows(
    row: &Row,
    items: &mut Value,
    at: &str,
    lineage: &[&Row],
    visit: &mut dyn FnMut(&str, &Value) -> bool,
) {
    let Some(items) = items.as_array_mut() else {
        return;
    };
    for (i, item) in items.iter_mut().enumerate() {
        let here = row_at(row, item, at, i);
        let Some(item) = item.as_object_mut() else {
            continue;
        };
        if let Some(Value::Object(fields)) = item.get_mut("fields") {
            stray_fields(&row.fields, fields, &here, visit);
            if fields.is_empty() {
                item.remove("fields");
            }
        }
        if let Some(Value::Object(children)) = item.get_mut("rows") {
            let names: Vec<String> = children.keys().cloned().collect();
            for name in names {
                let inner = row
                    .children
                    .iter()
                    .find(|c| c.name() == name)
                    .and_then(|c| match c {
                        Child::Row(r) => Some(r),
                        Child::Again { name, .. } => {
                            lineage.iter().rev().find(|r| &r.name == name).copied()
                        }
                    });
                let place = join(&here, &name);
                match inner {
                    None => {
                        let flat = rows_flat(&children[&name]);
                        if visit(&place, &flat) {
                            children.remove(&name);
                        }
                    }
                    Some(inner) => {
                        let mut deeper = lineage.to_vec();
                        if !deeper.iter().any(|r| r.name == inner.name) {
                            deeper.push(inner);
                        }
                        if let Some(list) = children.get_mut(&name) {
                            stray_rows(inner, list, &place, &deeper, visit);
                        }
                    }
                }
            }
            if children.is_empty() {
                item.remove("rows");
            }
        }
    }
}

fn check_card_rows(kind: &Kind, rows: &Map<String, Value>, problems: &mut Vec<Problem>) {
    let mut walk = Walk {
        ids: HashSet::new(),
        totals: HashMap::new(),
        problems,
    };
    for (name, items) in rows {
        match kind.row(name) {
            Some(row) => walk.rows(row, items, name, 1, &[row]),
            None => walk.problems.push(Problem::stray(
                "",
                keyed!("kindCheck.card.notRowKind", name, kind = kind.name),
            )),
        }
    }
    for row in &kind.rows {
        walk.too_many(row);
    }
}

/// A field's value read beside the others: written, or else its default.
fn reader<'a>(
    defs: &'a [Field],
    values: &'a Map<String, Value>,
) -> impl Fn(&str) -> Option<Value> + 'a {
    move |name: &str| {
        values.get(name).cloned().or_else(|| {
            defs.iter()
                .find(|d| d.name == name)
                .and_then(|d| d.default.clone())
        })
    }
}

/// How a condition reads, for a person: `kind が begin・trigger が receive`. Kept by key, so it
/// is said in the language of whoever reads the problem it is part of.
fn told(cond: &Cond) -> Keyed {
    match cond {
        Cond::Always => keyed!("kindCheck.cond.always"),
        Cond::Never => keyed!("kindCheck.cond.never"),
        Cond::AnyOf(alternatives) => either(
            alternatives
                .iter()
                .map(|all| {
                    both(
                        all.iter()
                            .map(|(field, values)| {
                                let values: Vec<String> = values
                                    .iter()
                                    .map(|v| {
                                        v.as_str()
                                            .map(str::to_string)
                                            .unwrap_or_else(|| v.to_string())
                                    })
                                    .collect();
                                keyed!(
                                    "kindCheck.cond.is",
                                    field = field,
                                    values = crate::text::List::of(values)
                                )
                            })
                            .collect(),
                    )
                })
                .collect(),
        ),
    }
}

/// Conditions that all hold, each said after the one before (nested, as a list of any length
/// is joined by a sentence).
fn both(items: Vec<Keyed>) -> Keyed {
    let mut items = items.into_iter().rev();
    let Some(last) = items.next() else {
        return keyed!("kindCheck.cond.none");
    };
    items.fold(last, |rest, first| {
        keyed!("kindCheck.cond.both", first, rest)
    })
}

/// Alternatives of which one is enough, each said after the one before.
fn either(items: Vec<Keyed>) -> Keyed {
    let mut items = items.into_iter().rev();
    let Some(last) = items.next() else {
        return keyed!("kindCheck.cond.none");
    };
    items.fold(last, |rest, first| {
        keyed!("kindCheck.cond.either", first, rest)
    })
}

fn join(at: &str, name: &str) -> String {
    if at.is_empty() {
        name.to_string()
    } else {
        format!("{at}.{name}")
    }
}

/// Checks a set of fields: each written one is known, applies here and holds the right
/// kind of value; each required one that applies is written.
pub(crate) fn check_fields(
    defs: &[Field],
    values: &Map<String, Value>,
    at: &str,
    problems: &mut Vec<Problem>,
) {
    let read = reader(defs, values);
    for (name, value) in values {
        let here = join(at, name);
        let Some(def) = defs.iter().find(|d| &d.name == name) else {
            problems.push(Problem::stray(&here, keyed!("kindCheck.field.notHeld")));
            continue;
        };
        if def.look {
            problems.push(Problem::new(&here, keyed!("kindCheck.field.lookOnly")));
            continue;
        }
        if !def.when.holds(&read) {
            problems.push(Problem::new(
                &here,
                keyed!("kindCheck.field.onlyWhen", when = told(&def.when)),
            ));
            continue;
        }
        check_value(def, value, &here, problems);
    }
    for def in defs {
        if !def.look
            && !values.contains_key(&def.name)
            && def.when.holds(&read)
            && def.required.holds(&read)
        {
            problems.push(Problem::new(
                &join(at, &def.name),
                keyed!("kindCheck.field.required"),
            ));
        }
    }
}

fn check_value(def: &Field, value: &Value, at: &str, problems: &mut Vec<Problem>) {
    if def.many == 0 {
        return check_one(def, value, at, problems);
    }
    let Some(items) = value.as_array() else {
        return problems.push(Problem::new(at, keyed!("kindCheck.value.list")));
    };
    if def.min_count.is_some_and(|n| items.len() < n) {
        problems.push(Problem::new(
            at,
            keyed!(
                "kindCheck.value.countAtLeast",
                count = def.min_count.unwrap_or_default()
            ),
        ));
    }
    if def.max_count.is_some_and(|n| items.len() > n) {
        problems.push(Problem::new(
            at,
            keyed!(
                "kindCheck.value.countAtMost",
                count = def.max_count.unwrap_or_default()
            ),
        ));
    }
    if def.unique {
        let mut seen = HashSet::new();
        for item in items {
            if !seen.insert(item.to_string()) {
                problems.push(Problem::new(at, keyed!("kindCheck.value.duplicate", item)));
            }
        }
    }
    for (i, item) in items.iter().enumerate() {
        let here = format!("{at}[{i}]");
        if def.many == 2 {
            match item.as_array() {
                Some(inner) => {
                    for (j, one) in inner.iter().enumerate() {
                        check_one(def, one, &format!("{here}[{j}]"), problems);
                    }
                }
                None => problems.push(Problem::new(&here, keyed!("kindCheck.value.listOfLists"))),
            }
        } else {
            check_one(def, item, &here, problems);
        }
    }
}

fn check_one(def: &Field, value: &Value, at: &str, problems: &mut Vec<Problem>) {
    if def.fieldtype == FieldType::Group {
        match value.as_object() {
            Some(inner) => check_fields(&def.fields, inner, at, problems),
            None => problems.push(Problem::new(at, keyed!("kindCheck.value.object"))),
        }
        return;
    }
    for message in wrong_one(def, value) {
        problems.push(Problem::new(at, message));
    }
}

/// The known kinds of rule: what each must say, and what else it may (`knownRules` of the
/// Swift app, with what its projects wrote; `field` and `note` on any). Any other kind is kept
/// as written.
pub const KNOWN_RULES: [(&str, &[&str], &[&str]); 11] = [
    ("sortedBy", &[], &["by", "keys", "order"]),
    ("uniqueBy", &[], &["by", "keys"]),
    ("requiredIf", &["when", "then"], &[]),
    ("absentIf", &["when", "then"], &[]),
    ("derivedFrom", &["from"], &[]),
    ("monotonicPerKey", &["keys"], &[]),
    ("chainedBy", &["field"], &[]),
    ("memberOf", &["target"], &[]),
    ("lteRef", &["target"], &[]),
    ("gteRef", &["target"], &[]),
    ("pattern", &["regex"], &[]),
];

/// What is wrong with the rules a card's document holds — its own, and its fields' at any
/// depth — that `fresh` says are new: a card is held to them as it is written, what it kept
/// from before is not asked again (a project written before they were checked keeps working).
pub fn rule_problems(doc: &Value, fresh: &dyn Fn(&Value) -> bool) -> Vec<Problem> {
    fn rules(
        list: Option<&Value>,
        at: &str,
        fresh: &dyn Fn(&Value) -> bool,
        out: &mut Vec<Problem>,
    ) {
        for (i, rule) in list
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .enumerate()
        {
            if fresh(rule) {
                for message in wrong_rule(rule) {
                    out.push(Problem::new(&format!("{at}rules[{i}]"), message));
                }
            }
        }
    }
    fn fields(
        rows: Option<&Value>,
        at: &str,
        fresh: &dyn Fn(&Value) -> bool,
        out: &mut Vec<Problem>,
    ) {
        for row in rows.and_then(Value::as_array).into_iter().flatten() {
            let name = row["fields"]["name"].as_str().unwrap_or("?");
            let here = format!("{at}{name}.");
            rules(row["fields"].get("rules"), &here, fresh, out);
            fields(
                row.get("rows").and_then(|r| r.get("fields")),
                &here,
                fresh,
                out,
            );
        }
    }
    let mut out = Vec::new();
    rules(
        doc.get("fields").and_then(|f| f.get("rules")),
        "",
        fresh,
        &mut out,
    );
    fields(
        doc.get("rows").and_then(|r| r.get("fields")),
        "",
        fresh,
        &mut out,
    );
    out
}

/// Every rule a card's document holds, its own and its fields', as written.
pub fn rules_held(doc: &Value) -> Vec<Value> {
    let mut out: Vec<Value> = doc
        .get("fields")
        .and_then(|f| f.get("rules"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut rows: Vec<&Value> = doc
        .get("rows")
        .and_then(|r| r.get("fields"))
        .and_then(Value::as_array)
        .map(|l| l.iter().collect())
        .unwrap_or_default();
    while let Some(row) = rows.pop() {
        if let Some(list) = row["fields"].get("rules").and_then(Value::as_array) {
            out.extend(list.iter().cloned());
        }
        if let Some(inner) = row
            .get("rows")
            .and_then(|r| r.get("fields"))
            .and_then(Value::as_array)
        {
            rows.extend(inner.iter());
        }
    }
    out
}

/// What is wrong with one rule (`Validate.rules`): a kind it has; a known kind with what it
/// must say and nothing it may not.
fn wrong_rule(value: &Value) -> Vec<Keyed> {
    let Some(rule) = value.as_object() else {
        return vec![keyed!("kindCheck.rule.notObject")];
    };
    let Some(kind) = rule
        .get("kind")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|k| !k.is_empty())
    else {
        return vec![keyed!("kindCheck.rule.noKind")];
    };
    let Some((_, needed, optional)) = KNOWN_RULES.iter().find(|(k, _, _)| *k == kind) else {
        return Vec::new();
    };
    let mut wrong = Vec::new();
    let missing: Vec<&str> = needed
        .iter()
        .copied()
        .filter(|name| {
            rule.get(*name)
                .is_none_or(|v| v.is_null() || v.as_str().is_some_and(str::is_empty))
        })
        .collect();
    if !missing.is_empty() {
        wrong.push(keyed!(
            "kindCheck.rule.needs",
            kind,
            missing = names_and(&missing)
        ));
    }
    let unknown: Vec<&str> = rule
        .keys()
        .map(String::as_str)
        .filter(|k| {
            !["kind", "field", "note"].contains(k) && !needed.contains(k) && !optional.contains(k)
        })
        .collect();
    if !unknown.is_empty() {
        let mut may: Vec<&str> = needed.iter().chain(optional.iter()).copied().collect();
        for always in ["field", "note"] {
            if !may.contains(&always) {
                may.push(always);
            }
        }
        wrong.push(keyed!(
            "kindCheck.rule.notAllowed",
            kind,
            unknown = crate::text::List::of(unknown),
            may = crate::text::List::of(may)
        ));
    }
    for name in ["keys", "by"] {
        if rule.get(name).is_some_and(|v| !v.is_array()) {
            wrong.push(keyed!("kindCheck.rule.namesNotList", kind, name));
        }
    }
    if let Some(order) = rule.get("order")
        && !matches!(order.as_str(), Some("asc" | "desc"))
    {
        wrong.push(keyed!("kindCheck.rule.orderForm", kind));
    }
    wrong
}

/// Names joined by "and" (nested, each said after the one before), kept so they are joined the
/// way the reader's language does.
fn names_and(names: &[&str]) -> Keyed {
    names
        .iter()
        .rev()
        .fold(None, |rest: Option<Keyed>, first| {
            Some(match rest {
                None => keyed!("kindCheck.rule.name", name = first),
                Some(rest) => keyed!("kindCheck.rule.and", first, rest),
            })
        })
        .unwrap_or_else(|| keyed!("kindCheck.cond.none"))
}

/// What is wrong with one value of a field that is not a group.
fn wrong_one(def: &Field, value: &Value) -> Vec<Keyed> {
    let mut wrong = Vec::new();
    match def.fieldtype {
        FieldType::Text | FieldType::LongText | FieldType::Markdown | FieldType::Url => {
            let Some(text) = value.as_str() else {
                return vec![keyed!("kindCheck.value.text")];
            };
            let length = text.chars().count() as f64;
            if let Some(min) = def.min.filter(|n| length < *n) {
                wrong.push(keyed!("kindCheck.value.charsAtLeast", min));
            }
            if let Some(max) = def.max.filter(|n| length > *n) {
                wrong.push(keyed!("kindCheck.value.charsAtMost", max));
            }
            if def.identifier && !path_name(text) {
                wrong.push(keyed!("kindCheck.value.nameForm", text));
            }
            match text.chars().find(|c| def.forbid.contains(*c)) {
                Some('\n' | '\r') => wrong.push(keyed!("kindCheck.value.lineBreak")),
                Some(c) => wrong.push(keyed!("kindCheck.value.charForbidden", c)),
                None => {}
            }
        }
        FieldType::Person
        | FieldType::Branch
        | FieldType::Card
        | FieldType::Path
        | FieldType::Section => {
            if value.as_str().is_none_or(str::is_empty) {
                wrong.push(keyed!("kindCheck.value.nonEmptyText"));
            }
        }
        FieldType::Number => match value.as_f64() {
            None => wrong.push(keyed!("kindCheck.value.number")),
            Some(n) => {
                if let Some(min) = def.min.filter(|m| n < *m) {
                    wrong.push(keyed!("kindCheck.value.numberAtLeast", min));
                }
                if let Some(max) = def.max.filter(|m| n > *m) {
                    wrong.push(keyed!("kindCheck.value.numberAtMost", max));
                }
            }
        },
        FieldType::Checkbox => {
            if !value.is_boolean() {
                wrong.push(keyed!("kindCheck.value.boolean"));
            }
        }
        FieldType::Date => {
            if !value.as_str().is_some_and(date) {
                wrong.push(keyed!("kindCheck.value.date"));
            }
        }
        FieldType::Radio | FieldType::DropdownList => {
            if !value
                .as_str()
                .is_some_and(|chosen| def.options.iter().any(|o| o == chosen))
            {
                wrong.push(keyed!(
                    "kindCheck.value.notOption",
                    value,
                    options = crate::text::List::of(&def.options)
                ));
            }
        }
        FieldType::Mr => {
            if value.as_u64().is_none_or(|n| n < 1) {
                wrong.push(keyed!("kindCheck.value.mr"));
            }
        }
        FieldType::Json | FieldType::Group => {}
        FieldType::Map => match value.as_object() {
            None => wrong.push(keyed!("kindCheck.value.stringMap")),
            Some(pairs) => {
                if let Some(max) = def.max_count.filter(|n| pairs.len() > *n) {
                    wrong.push(keyed!("kindCheck.value.pairsAtMost", max));
                }
                if let Some((key, _)) = pairs.iter().find(|(_, v)| !v.is_string()) {
                    wrong.push(keyed!("kindCheck.value.pairNotText", key));
                }
            }
        },
    }
    wrong
}

/// `YYYY-MM-DD`, a day that is in its month.
fn date(text: &str) -> bool {
    let parts: Vec<&str> = text.split('-').collect();
    let [y, m, d] = parts.as_slice() else {
        return false;
    };
    if y.len() != 4 || m.len() != 2 || d.len() != 2 {
        return false;
    }
    let (Ok(y), Ok(m), Ok(d)) = (y.parse::<u32>(), m.parse::<u32>(), d.parse::<u32>()) else {
        return false;
    };
    let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let days = match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return false,
    };
    (1..=days).contains(&d)
}

/// The rows of a card, walked with what must hold across them all.
struct Walk<'p> {
    ids: HashSet<String>,
    totals: HashMap<String, usize>,
    problems: &'p mut Vec<Problem>,
}

impl Walk<'_> {
    /// `lineage`: the row kinds from the card down to this one, for `ref`.
    fn rows(&mut self, row: &Row, items: &Value, at: &str, depth: usize, lineage: &[&Row]) {
        let Some(items) = items.as_array() else {
            return self
                .problems
                .push(Problem::new(at, keyed!("kindCheck.row.list")));
        };
        if row.depth.is_some_and(|d| depth > d) {
            self.problems.push(Problem::new(
                at,
                keyed!(
                    "kindCheck.row.depthAtMost",
                    depth = row.depth.unwrap_or_default()
                ),
            ));
        }
        *self.totals.entry(row.name.clone()).or_default() += items.len();
        self.unique(row, items, at);
        for (i, item) in items.iter().enumerate() {
            let here = row_at(row, item, at, i);
            let Some(item) = item.as_object() else {
                self.problems
                    .push(Problem::new(&here, keyed!("kindCheck.row.notObject")));
                continue;
            };
            for key in item.keys() {
                if !["id", "fields", "rows"].contains(&key.as_str()) {
                    self.problems.push(Problem::new(
                        &here,
                        keyed!("kindCheck.row.keyNotAllowed", key = key),
                    ));
                }
            }
            match item.get("id").and_then(Value::as_str) {
                Some(id) if !id.is_empty() => {
                    if !self.ids.insert(id.to_string()) {
                        self.problems
                            .push(Problem::new(&here, keyed!("kindCheck.row.idDuplicate", id)));
                    }
                }
                _ => self
                    .problems
                    .push(Problem::new(&here, keyed!("kindCheck.row.noId"))),
            }
            let empty = Map::new();
            let fields = match item.get("fields") {
                None => &empty,
                Some(Value::Object(fields)) => fields,
                Some(_) => {
                    self.problems
                        .push(Problem::new(&here, keyed!("kindCheck.doc.fieldsNotObject")));
                    &empty
                }
            };
            check_fields(&row.fields, fields, &here, self.problems);
            match item.get("rows") {
                None => {}
                Some(Value::Object(children)) => {
                    self.children(row, fields, children, &here, depth, lineage)
                }
                Some(_) => self
                    .problems
                    .push(Problem::new(&here, keyed!("kindCheck.doc.rowsNotObject"))),
            }
        }
    }

    fn children(
        &mut self,
        row: &Row,
        fields: &Map<String, Value>,
        children: &Map<String, Value>,
        at: &str,
        depth: usize,
        lineage: &[&Row],
    ) {
        let read = reader(&row.fields, fields);
        for (name, items) in children {
            let here = join(at, name);
            let Some(child) = row.children.iter().find(|c| c.name() == name) else {
                self.problems.push(Problem::stray(
                    &here,
                    keyed!("kindCheck.row.notUnder", row = row.name),
                ));
                continue;
            };
            if !child.when().holds(&read) {
                self.problems.push(Problem::new(
                    &here,
                    keyed!("kindCheck.row.onlyWhen", when = told(child.when())),
                ));
                continue;
            }
            match child {
                Child::Row(inner) => {
                    let mut deeper = lineage.to_vec();
                    deeper.push(inner);
                    self.rows(inner, items, &here, 1, &deeper);
                }
                Child::Again { name, .. } => match lineage.iter().rev().find(|r| &r.name == name) {
                    // The same kind under itself goes a level deeper.
                    Some(again) => {
                        let level = if again.name == row.name { depth + 1 } else { 1 };
                        self.rows(again, items, &here, level, lineage);
                    }
                    None => self
                        .problems
                        .push(Problem::new(&here, keyed!("kindCheck.row.cannotPlace"))),
                },
            }
        }
    }

    /// Fields marked unique differ among the rows beside one another.
    fn unique(&mut self, row: &Row, items: &[Value], at: &str) {
        for def in row.fields.iter().filter(|f| f.unique) {
            let mut seen = HashSet::new();
            for item in items {
                if let Some(value) = item.get("fields").and_then(|f| f.get(&def.name)) {
                    let key = value
                        .as_str()
                        .map(str::to_string)
                        .unwrap_or_else(|| value.to_string());
                    if !seen.insert(key.clone()) {
                        self.problems.push(Problem::new(
                            at,
                            keyed!("kindCheck.row.valueDuplicate", label = def.label, key),
                        ));
                    }
                }
            }
        }
    }

    fn too_many(&mut self, row: &Row) {
        if let Some(max) = row.max_total
            && self.totals.get(&row.name).is_some_and(|n| *n > max)
        {
            self.problems.push(Problem::new(
                &row.name,
                keyed!("kindCheck.row.totalAtMost", max),
            ));
        }
    }
}

/// Where a row is: by its name or value when it has one, else by its place.
fn row_at(row: &Row, item: &Value, at: &str, index: usize) -> String {
    let named = |field: &str| {
        item.get("fields")
            .and_then(|f| f.get(field))
            .and_then(Value::as_str)
            .map(|s| join(at, s))
    };
    match row.address {
        Address::Name => named("name"),
        Address::Value => named("value"),
        Address::Index => None,
    }
    .unwrap_or_else(|| format!("{at}[{index}]"))
}
