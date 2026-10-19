//! Tables (`table`; `specboard://SpecBoard/Docs/Design:CardKinds`「表（table）」): the data cards of a shape — the
//! entity or value the table's line `of` goes to — gathered by its rules into a table, to put in a
//! document (`![[…]]`, a formatdoc's row `card`), on the diagram, written out and read by MCP.
//!
//! The rules are the table's fields: `within` (a section: its data and that of the sections in
//! it), `where` (`[{path, op, value}]`, every one met), `unnest` (an array: a row an element),
//! `sort` (`[{path, desc}]`; by the data cards' names when none) and `columns` (`[{path, label,
//! show}]`; `@name` and the shape's top fields when none). A path is a value's path by the shape's field
//! names, as a data card's is, an array's elements picked too; `[]` is every element (in a cell,
//! their values joined by「, 」; of the array unnested, the row's); `@name`・`@label` are the data
//! card's name and display name. A column's `show` says its cells otherwise than as the values are:
//! `label` an enum's value by its display name, `link` a value naming a card as `[[that path]]` —
//! what is compared and sorted is the value still.

use super::picks::{self, Found, Pick};
use crate::export::Project;
use crate::kinds::TABLE;
use crate::model::{Card, EnumValue, Field};
use serde_json::Value;
use std::collections::HashMap;

/// How many rows MCP reads of a table (`card_read`'s `table`).
pub(crate) const READ_ROWS: usize = 100;

/// The ways a rule compares.
const OPS: [&str; 8] = ["eq", "ne", "lt", "le", "gt", "ge", "contains", "exists"];

/// A table's shape: the entity or value its line `of` goes to.
pub(crate) fn shape<'a>(project: &Project<'a>, card: &Card) -> Option<&'a Card> {
    let id = card.shape_of()?;
    project
        .card(id)
        .filter(|c| c.kind == "entity" || c.kind == "value")
}

struct Filter {
    path: String,
    op: String,
    value: Value,
}

struct Column {
    path: String,
    label: Option<String>,
    show: Option<Show>,
}

/// How a column says its values, when not as they are.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Show {
    /// An enum's value by its display name (the value when it has none).
    Label,
    /// A value naming a card as `[[its path in full]]` (the value when it names none).
    Link,
}

/// The ways a column may say its values.
const SHOWS: [&str; 2] = ["label", "link"];

impl Show {
    fn read(written: &Value) -> Option<Show> {
        match written.as_str()? {
            "label" => Some(Show::Label),
            "link" => Some(Show::Link),
            _ => None,
        }
    }
}

struct Order {
    path: String,
    desc: bool,
}

/// A table's rules, as written.
struct Rules {
    within: Option<String>,
    filters: Vec<Filter>,
    columns: Vec<Column>,
    sort: Vec<Order>,
    unnest: Option<String>,
}

fn list<'v>(card: &'v Card, name: &str) -> &'v [Value] {
    card.table_field(name)
        .and_then(Value::as_array)
        .map_or(&[], Vec::as_slice)
}

fn text_of(card: &Card, name: &str) -> Option<String> {
    card.table_field(name)
        .and_then(Value::as_str)
        .map(|s| s.trim_matches([' ', '\t']).to_string())
        .filter(|s| !s.is_empty())
}

fn rules(card: &Card) -> Rules {
    let path = |v: &Value| v["path"].as_str().unwrap_or_default().to_string();
    Rules {
        within: text_of(card, "within"),
        filters: list(card, "where")
            .iter()
            .map(|f| Filter {
                path: path(f),
                op: f["op"].as_str().unwrap_or("eq").to_string(),
                value: f.get("value").cloned().unwrap_or(Value::Null),
            })
            .collect(),
        columns: list(card, "columns")
            .iter()
            .map(|c| Column {
                path: path(c),
                label: c["label"]
                    .as_str()
                    .filter(|l| !l.is_empty())
                    .map(str::to_string),
                show: Show::read(&c["show"]),
            })
            .collect(),
        sort: list(card, "sort")
            .iter()
            .map(|s| Order {
                path: path(s),
                desc: s["desc"].as_bool().unwrap_or(false),
            })
            .collect(),
        unnest: text_of(card, "unnest"),
    }
}

/// Whether a name is every element of an array (`ports[]`).
fn every(name: &str) -> bool {
    name.trim_matches([' ', '\t']).ends_with("[]")
}

/// The fields a path walks in a shape, or why it does not: a name the shape has not there, an
/// array passed with no `[]` or pick, a pick written wrong. `@name` and `@label` walk none.
fn walk_fields<'a>(
    project: &Project<'a>,
    shape: &'a Card,
    path: &str,
) -> Result<Vec<&'a Field>, String> {
    if path == "@name" || path == "@label" {
        return Ok(Vec::new());
    }
    if path.is_empty() {
        return Err(text!("tables.path.none"));
    }
    let names = picks::split(path);
    let mut list: &'a [Field] = &shape.fields;
    let mut out: Vec<&'a Field> = Vec::new();
    let mut open = false;
    for name in &names {
        if open {
            let at = out.last().map_or("", |f| f.name.as_str());
            return Err(text!("tables.path.inArray", at));
        }
        let (base, pick) = picks::step(name)?;
        let Some(field) = list.iter().find(|f| f.name == base) else {
            return Err(text!(
                "tables.path.noField",
                shape = shape.name,
                base,
                fields = crate::text::List::of(list.iter().map(|f| f.name.as_str()))
            ));
        };
        let inner = super::instances::inner_fields(project, field);
        let all = every(name);
        if (pick.is_some() || all) && !field.is_array {
            return Err(text!("tables.path.notArray", base));
        }
        if let Some(pick) = &pick {
            super::instances::held(project, pick, &base, inner)?;
        }
        out.push(field);
        open = field.is_array && pick.is_none() && !all;
        list = inner;
    }
    Ok(out)
}

/// What is wrong with a table's rules as written, each said with where (`where[0].path`): a shape
/// missing, a path its shape does not have, a way to compare that is none, an unnest that is not
/// an array. None when it is right.
fn wrongs(project: &Project, card: &Card) -> Vec<(String, String, String)> {
    let mut out = Vec::new();
    let Some(shape) = shape(project, card) else {
        return out;
    };
    let rules = rules(card);
    let mut check = |via: String, path: &str| {
        if let Err(why) = walk_fields(project, shape, path) {
            out.push((via, path.to_string(), why));
        }
    };
    for (i, f) in rules.filters.iter().enumerate() {
        check(format!("where[{i}].path"), &f.path);
    }
    for (i, c) in rules.columns.iter().enumerate() {
        check(format!("columns[{i}].path"), &c.path);
    }
    for (i, s) in rules.sort.iter().enumerate() {
        check(format!("sort[{i}].path"), &s.path);
    }
    if let Some(unnest) = &rules.unnest {
        match walk_fields(project, shape, unnest) {
            Err(why) => out.push(("unnest".into(), unnest.clone(), why)),
            Ok(fields) => {
                let names = picks::split(unnest);
                if !fields.last().is_some_and(|f| f.is_array)
                    || names.iter().any(|n| !crate::pathtext::step(n).1.is_empty())
                {
                    out.push((
                        "unnest".into(),
                        unnest.clone(),
                        text!("tables.unnest.shape"),
                    ));
                }
            }
        }
    }
    if let Some(within) = &rules.within
        && super::section_id(project, within).is_err()
    {
        out.push((
            "within".into(),
            within.clone(),
            text!("tables.within.noSection", within),
        ));
    }
    out
}

/// The paths of a table that do not resolve against its shape now (the check's brokenPaths): where
/// (`where[0].path`), the path, and why.
pub(crate) fn broken(project: &Project, card: &Card) -> Vec<(String, String, String)> {
    if card.kind != TABLE {
        return Vec::new();
    }
    wrongs(project, card)
}

/// A table as it is written, held to its shape and its rules' form: refused with every reason, or
/// fine. Its shape missing is said by its line `of`.
pub(crate) fn said(project: &Project, card: &Card) -> Result<(), String> {
    let full = project.full(card);
    let Some(_) = shape(project, card) else {
        return Err(text!("tables.said.ofNeeded", full));
    };
    let mut refused: Vec<String> = Vec::new();
    for (name, keys) in [
        ("where", &["path", "op", "value"][..]),
        ("columns", &["path", "label", "show"][..]),
        ("sort", &["path", "desc"][..]),
    ] {
        match card.table_field(name) {
            None => {}
            Some(Value::Array(items)) => {
                for (i, item) in items.iter().enumerate() {
                    let Some(o) = item.as_object() else {
                        refused.push(text!("tables.said.itemObject", full, name, i));
                        continue;
                    };
                    if let Some(k) = o.keys().find(|k| !keys.contains(&k.as_str())) {
                        refused.push(text!(
                            "tables.said.keyNotAllowed",
                            full,
                            name,
                            i,
                            k,
                            allowed = crate::text::List::of(keys.iter())
                        ));
                    }
                    if !o.get("path").is_some_and(Value::is_string) {
                        refused.push(text!("tables.said.pathNeeded", full, name, i));
                    }
                    if name == "where"
                        && let Some(op) = o.get("op")
                        && !op.as_str().is_some_and(|op| OPS.contains(&op))
                    {
                        refused.push(text!(
                            "tables.said.opShape",
                            full,
                            name,
                            i,
                            ops = crate::text::List::of(OPS.iter())
                        ));
                    }
                    if name == "columns" && o.get("label").is_some_and(|l| !l.is_string()) {
                        refused.push(text!("tables.said.labelShape", full, name, i));
                    }
                    if name == "columns"
                        && let Some(show) = o.get("show")
                        && Show::read(show).is_none()
                    {
                        refused.push(text!(
                            "tables.said.showShape",
                            full,
                            name,
                            i,
                            shows = crate::text::List::of(SHOWS.iter())
                        ));
                    }
                    if name == "sort" && o.get("desc").is_some_and(|d| !d.is_boolean()) {
                        refused.push(text!("tables.said.descShape", full, name, i));
                    }
                }
            }
            Some(_) => refused.push(text!("tables.said.notArray", full, name)),
        }
    }
    if refused.is_empty() {
        refused.extend(
            wrongs(project, card)
                .into_iter()
                .map(|(via, _, why)| format!("{full}.{via}: {why}")),
        );
    }
    if refused.is_empty() {
        Ok(())
    } else {
        Err(refused.join("\n"))
    }
}

/// A table built: its columns' headings and its rows' cells.
#[derive(Debug, Default)]
pub(crate) struct Built {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

/// One row: a data card, and the element of the array unnested, when one is.
struct Row<'a> {
    card: &'a Card,
    element: Option<&'a Value>,
}

/// The values a path reaches in a row: none, one, or (through `[]`) one an element.
fn values<'a>(row: &Row<'a>, path: &str, unnest: Option<&[String]>) -> Vec<&'a Value> {
    let names = picks::split(path);
    let mut here: Vec<&'a Value> = match row.card.instance_data() {
        Some(data) => vec![data],
        None => Vec::new(),
    };
    let mut trail: Vec<String> = Vec::new();
    for name in &names {
        let Ok((base, pick)) = picks::step(name) else {
            return Vec::new();
        };
        trail.push(base.to_string());
        let mut next: Vec<&'a Value> = here.iter().filter_map(|v| v.get(&*base)).collect();
        if every(name) {
            // The array unnested: the row's element.
            if unnest == Some(trail.as_slice())
                && let Some(element) = row.element
            {
                next = vec![element];
            } else {
                next = next
                    .into_iter()
                    .flat_map(|v| {
                        v.as_array()
                            .map(|a| a.iter().collect())
                            .unwrap_or_else(Vec::new)
                    })
                    .collect();
            }
        } else if let Some(pick) = pick {
            next = next
                .into_iter()
                .filter_map(|v| {
                    let items = v.as_array()?;
                    match pick.find(items) {
                        Found::One(k) => items.get(k),
                        _ => None,
                    }
                })
                .collect();
        }
        here = next;
    }
    here
}

/// A value as a cell says it: a text as it is (its line breaks spaces), a number or a truth as it
/// is, an object or an array as JSON, nothing for null.
pub(crate) fn cell(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(s) => s.replace(['\r', '\n'], " "),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        other => serde_json::to_string(&crate::export::sorted(other)).unwrap_or_default(),
    }
}

/// What a path says in a row: the card's name or display name, or its values' cells joined.
fn said_in(row: &Row, path: &str, unnest: Option<&[String]>) -> Vec<String> {
    match path {
        "@name" => vec![row.card.name.clone()],
        "@label" => vec![if row.card.label.is_empty() {
            row.card.name.clone()
        } else {
            row.card.label.clone()
        }],
        _ => values(row, path, unnest)
            .into_iter()
            .filter(|v| !v.is_null())
            .map(cell)
            .collect(),
    }
}

/// What a column says in a row: as `said_in`, but an array's elements each a value of its own, as
/// `[]` gives them — a field that is also a list (`also: [array]`) joined as `ports[].number` is,
/// not its JSON.
fn shown_in(row: &Row, path: &str, unnest: Option<&[String]>) -> Vec<String> {
    if path.starts_with('@') {
        return said_in(row, path, unnest);
    }
    values(row, path, unnest)
        .into_iter()
        .flat_map(|v| match v.as_array() {
            Some(items) => items.iter().collect(),
            None => vec![v],
        })
        .filter(|v| !v.is_null())
        .map(cell)
        .collect()
}

/// A value compared with what a rule gives: as numbers when both are, else as their texts.
fn compare(have: &str, given: &Value) -> std::cmp::Ordering {
    let want = match given {
        Value::String(s) => s.clone(),
        other => cell(other),
    };
    match (have.parse::<f64>(), want.parse::<f64>()) {
        (Ok(a), Ok(b)) => a.partial_cmp(&b).unwrap_or(std::cmp::Ordering::Equal),
        _ => have.cmp(want.as_str()),
    }
}

/// Whether a row meets a rule: any of the values its path reaches does.
fn meets(row: &Row, filter: &Filter, unnest: Option<&[String]>, broken: bool) -> bool {
    use std::cmp::Ordering::*;
    if broken {
        return false;
    }
    if filter.op == "exists" {
        let there = !said_in(row, &filter.path, unnest).is_empty();
        return there == filter.value.as_bool().unwrap_or(true);
    }
    if filter.op == "contains" {
        let raw = match filter.path.as_str() {
            "@name" | "@label" => Vec::new(),
            path => values(row, path, unnest),
        };
        // An array: one of its elements is the value.
        if raw.iter().any(|v| v.is_array()) {
            return raw.iter().filter_map(|v| v.as_array()).flatten().any(|e| {
                e.as_str()
                    .map_or_else(|| cell(e), str::to_string)
                    .eq(&match &filter.value {
                        Value::String(s) => s.clone(),
                        other => cell(other),
                    })
            });
        }
        let want = match &filter.value {
            Value::String(s) => s.clone(),
            other => cell(other),
        };
        return said_in(row, &filter.path, unnest)
            .iter()
            .any(|have| have.contains(&want));
    }
    let said = said_in(row, &filter.path, unnest);
    if filter.op == "ne" {
        return !said
            .iter()
            .any(|have| compare(have, &filter.value) == Equal);
    }
    said.iter().any(|have| {
        let o = compare(have, &filter.value);
        match filter.op.as_str() {
            "eq" => o == Equal,
            "lt" => o == Less,
            "le" => o != Greater,
            "gt" => o == Greater,
            "ge" => o != Less,
            _ => false,
        }
    })
}

/// The data cards a table reads: of its shape, in its section and the sections in it (the whole
/// tree when it names none), by their names.
fn gathered<'a>(project: &Project<'a>, card: &Card, rules: &Rules) -> Vec<&'a Card> {
    let Some(shape) = shape(project, card) else {
        return Vec::new();
    };
    let within: Option<Vec<String>> = match &rules.within {
        None => None,
        Some(w) => match super::section_id(project, w) {
            Ok(id) => {
                let mut all = vec![id.clone()];
                let mut i = 0;
                while i < all.len() {
                    let inner = project.directory.sections_in(&all[i]);
                    all.extend(inner);
                    i += 1;
                }
                Some(all)
            }
            Err(_) => Some(Vec::new()),
        },
    };
    let mut out: Vec<&'a Card> = project
        .detail
        .schemas
        .iter()
        .map(|c| &**c)
        .filter(|c| c.kind == crate::kinds::INSTANCE && c.instance_of() == Some(shape.id.as_str()))
        .filter(|c| {
            within
                .as_ref()
                .is_none_or(|w| w.contains(&project.section_of(c)))
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// A table built by its rules: its headings and its rows (none when its shape is gone). A path that
/// does not resolve against its shape leaves its cells empty and meets no rule.
pub(crate) fn build(project: &Project, card: &Card) -> Built {
    let Some(shape) = shape(project, card) else {
        return Built::default();
    };
    let rules = rules(card);
    let resolves = |path: &str| walk_fields(project, shape, path).is_ok();
    let unnest: Option<Vec<String>> = rules
        .unnest
        .as_ref()
        .filter(|u| resolves(u))
        .map(|u| picks::split(u));
    // A row a data card, or an element of the array unnested.
    let mut rows: Vec<Row> = Vec::new();
    for data in gathered(project, card, &rules) {
        match &unnest {
            None => rows.push(Row {
                card: data,
                element: None,
            }),
            Some(names) => {
                let mut here = data.instance_data();
                for name in names {
                    here = here.and_then(|h| h.get(name));
                }
                for element in here.and_then(Value::as_array).into_iter().flatten() {
                    rows.push(Row {
                        card: data,
                        element: Some(element),
                    });
                }
            }
        }
    }
    let unnested = unnest.as_deref();
    rows.retain(|row| {
        rules.filters.iter().all(|f| {
            meets(
                row,
                f,
                unnested,
                !resolves(&f.path) && !f.path.starts_with('@'),
            )
        })
    });
    // Ordered by each key in turn; a row with no value for a key after those with one.
    let keys: Vec<&Order> = rules.sort.iter().filter(|s| resolves(&s.path)).collect();
    rows.sort_by(|a, b| {
        for key in &keys {
            let (x, y) = (
                said_in(a, &key.path, unnested),
                said_in(b, &key.path, unnested),
            );
            let o = match (x.first(), y.first()) {
                (None, None) => std::cmp::Ordering::Equal,
                (None, Some(_)) => return std::cmp::Ordering::Greater,
                (Some(_), None) => return std::cmp::Ordering::Less,
                (Some(p), Some(q)) => compare(p, &Value::String(q.clone())),
            };
            let o = if key.desc { o.reverse() } else { o };
            if o != std::cmp::Ordering::Equal {
                return o;
            }
        }
        std::cmp::Ordering::Equal
    });
    // Its columns: those written, else the card's name and the shape's top fields.
    let columns: Vec<Column> = if rules.columns.is_empty() {
        std::iter::once(Column {
            path: "@name".into(),
            label: None,
            show: None,
        })
        .chain(shape.fields.iter().map(|f| Column {
            path: f.name.clone(),
            label: None,
            show: None,
        }))
        .collect()
    } else {
        rules.columns
    };
    let heading = |c: &Column| -> String {
        if let Some(label) = &c.label {
            return label.clone();
        }
        match c.path.as_str() {
            "@name" => text!("common.label.name"),
            "@label" => text!("common.label.displayName"),
            path => match walk_fields(project, shape, path) {
                Ok(fields) if picks::split(path).len() == 1 => fields
                    .last()
                    .filter(|f| !f.label.is_empty())
                    .map_or_else(|| path.to_string(), |f| f.label.clone()),
                _ => path.to_string(),
            },
        }
    };
    Built {
        columns: columns.iter().map(heading).collect(),
        rows: rows
            .iter()
            .map(|row| {
                columns
                    .iter()
                    .map(|c| {
                        if !c.path.starts_with('@') && !resolves(&c.path) {
                            return String::new();
                        }
                        let cells = shown_in(row, &c.path, unnested);
                        match c.show {
                            None => cells,
                            Some(Show::Label) => {
                                let values = enum_of(project, shape, &c.path);
                                cells.into_iter().map(|v| labelled(values, v)).collect()
                            }
                            Some(Show::Link) => cells
                                .into_iter()
                                .map(|v| linked(project, row.card, v))
                                .collect(),
                        }
                        .join(", ")
                    })
                    .collect()
            })
            .collect(),
    }
}

/// The values of the enum a column's field holds, when it holds one.
fn enum_of<'a>(project: &Project<'a>, shape: &'a Card, path: &str) -> &'a [EnumValue] {
    let Ok(fields) = walk_fields(project, shape, path) else {
        return &[];
    };
    fields
        .last()
        .filter(|f| f.is_ref())
        .and_then(|f| f.ref_schema_id.as_deref())
        .and_then(|id| project.card(id))
        .filter(|c| c.kind == "enum")
        .map_or(&[], |c| c.values.as_slice())
}

/// An enum's value by its display name: the value when the enum has none for it, or has it not.
fn labelled(values: &[EnumValue], value: String) -> String {
    values
        .iter()
        .find(|v| v.value == value)
        .filter(|v| !v.label.is_empty())
        .map_or(value, |v| v.label.clone())
}

/// A value naming a card as `[[its path in full]]`, read from the data card's section as a path
/// written in it is — so the link lands the same wherever the table is put. The value as it is
/// when it names none.
fn linked(project: &Project, data: &Card, value: String) -> String {
    if !crate::export::paths::resolves(project, &value, data) {
        return value;
    }
    let Some(written) = crate::export::paths::WrittenPath::new(&value) else {
        return value;
    };
    let Some(card) = project.find_seeing(&written.card, &project.section_of(data)) else {
        return value;
    };
    let trimmed = value.trim_matches([' ', '\t']);
    let rest = crate::pathtext::split_section(trimmed).1;
    let inside = picks::split(rest);
    let mut path = project.full(card);
    for name in inside.iter().skip(1) {
        path.push('.');
        path.push_str(name.trim_matches([' ', '\t']));
    }
    format!("[[{path}]]")
}

/// A table as Markdown: its headings, the line under them, a row a line (`|` let out).
pub(crate) fn markdown(built: &Built) -> String {
    let esc = |s: &str| s.replace('|', "\\|");
    let line = |cells: &[String]| {
        format!(
            "| {} |",
            cells.iter().map(|c| esc(c)).collect::<Vec<_>>().join(" | ")
        )
    };
    let mut out = vec![
        line(&built.columns),
        format!("|{}|", vec!["---"; built.columns.len()].join("|")),
    ];
    out.extend(built.rows.iter().map(|r| line(r)));
    out.join("\n") + "\n"
}

/// A table's rules with each path named as the shape's fields are named now: read against its
/// shape as it was (`before`), the fields known by their ids. How many changed; none when none did.
pub(crate) fn renamed(
    before: &Project,
    card: &Card,
    fields_now: &HashMap<String, String>,
    doc: &mut Value,
) -> usize {
    let Some(shape) = shape(before, card) else {
        return 0;
    };
    let again = |path: &str| -> Option<String> {
        if path.starts_with('@') {
            return None;
        }
        let names = picks::split(path);
        let walked = walk_fields(before, shape, path).ok()?;
        let mut out = Vec::new();
        for (name, field) in names.iter().zip(&walked) {
            let (_, pick) = picks::step(name).ok()?;
            let now = fields_now
                .get(&field.id)
                .cloned()
                .unwrap_or_else(|| field.name.clone());
            let suffix = match &pick {
                Some(Pick::Where(keys)) => {
                    let inner = super::instances::inner_fields(before, field);
                    Pick::Where(
                        keys.iter()
                            .map(|(k, lit)| {
                                let named = inner
                                    .iter()
                                    .find(|f| &f.name == k)
                                    .and_then(|f| fields_now.get(&f.id))
                                    .cloned()
                                    .unwrap_or_else(|| k.clone());
                                (named, lit.clone())
                            })
                            .collect(),
                    )
                    .written()
                }
                Some(other) => other.written(),
                None if every(name) => "[]".to_string(),
                None => String::new(),
            };
            out.push(format!("{}{suffix}", crate::pathtext::quote(&now)));
        }
        let now = out.join(".");
        (now != path).then_some(now)
    };
    let mut count = 0;
    for key in ["where", "columns", "sort"] {
        if let Some(items) = doc
            .pointer_mut(&format!("/fields/{key}"))
            .and_then(Value::as_array_mut)
        {
            for item in items.iter_mut() {
                if let Some(path) = item.get("path").and_then(Value::as_str)
                    && let Some(now) = again(path)
                {
                    item["path"] = now.into();
                    count += 1;
                }
            }
        }
    }
    if let Some(path) = doc.pointer("/fields/unnest").and_then(Value::as_str)
        && let Some(now) = again(path)
    {
        doc["fields"]["unnest"] = now.into();
        count += 1;
    }
    count
}

/// A table as MCP reads it apart from its rules: its headings and up to `READ_ROWS` rows, how many
/// there are when it holds more.
pub(crate) fn read(project: &Project, card: &Card) -> Value {
    let built = build(project, card);
    let mut out = serde_json::Map::new();
    out.insert("columns".into(), built.columns.clone().into());
    let total = built.rows.len();
    out.insert(
        "rows".into(),
        built
            .rows
            .into_iter()
            .take(READ_ROWS)
            .map(Value::from)
            .collect::<Vec<_>>()
            .into(),
    );
    if total > READ_ROWS {
        out.insert("truncated".into(), true.into());
        out.insert("rowCount".into(), total.into());
    }
    Value::Object(out)
}
