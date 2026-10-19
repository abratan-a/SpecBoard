//! A card's lines (`connections`): one each, of a kind of line the project defines (`defs`),
//! kept in the card's document as what they are — a line kind, the row it leaves
//! (`from`, a path inside the card; none for its head), the card or card ghost it reaches
//! (`to`, by id, so it follows a rename and a move), where it lands there (`at`, a path
//! inside that card; one that does not designate may land elsewhere, written in full), a
//! word about it (`description`) and its own fields (`fields`, as the line's kind defines
//! them).
//!
//! ```json
//! "connections": [ { "id": "…", "line": "points", "from": "buyer_id", "to": "<id>", "at": "id",
//!                    "description": "注文した人", "fields": { "cardinality": "N:1" } } ]
//! ```
//!
//! On MCP a line is the same with a path in place of the id (`"to": "Data/Tenant:User"`)
//! and no id: a line written the same way as one there is (its kind, row, card and landing,
//! and its fields and word first, so that of two lines between the same rows each keeps its
//! own) keeps its id, a new one is given one.

use super::check::{Problem, check_fields};
use super::{Defs, Names};
use serde_json::{Map, Value};
use std::collections::HashSet;

/// The keys a line has in a card's document.
const KEYS: [&str; 7] = ["id", "line", "from", "to", "at", "description", "fields"];

/// The keys a line is written with on MCP.
const WIRE_KEYS: [&str; 6] = ["line", "from", "to", "at", "description", "fields"];

/// The longest word about one line.
const DESCRIPTION_MAX: usize = 2000;

/// A card's lines as its document keeps them.
pub fn connections_of(doc: &Value) -> &[Value] {
    doc.get("connections")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

fn text<'a>(line: &'a Map<String, Value>, key: &str) -> Option<&'a str> {
    line.get(key).and_then(Value::as_str)
}

/// Whether a line names no card and needs none: a pointer told apart by its discriminator.
pub fn discriminated(line: &Map<String, Value>) -> bool {
    line.get("fields")
        .and_then(|f| f.get("discriminatedBy"))
        .and_then(Value::as_str)
        .is_some_and(|s| !s.is_empty())
}

/// Every problem with a card's lines as its document keeps them: the shape of each, its
/// kind, and its fields against the kind. Whether `from`, `to` and `at` lead where the kind
/// allows is the project's to say (`tools::lines`).
pub fn check_connections(defs: &Defs, doc: &Value) -> Vec<Problem> {
    let mut problems = Vec::new();
    let Some(value) = doc.get("connections") else {
        return problems;
    };
    let Some(list) = value.as_array() else {
        return vec![Problem::new(
            "connections",
            keyed!("kindConnections.problem.list"),
        )];
    };
    let mut ids = HashSet::new();
    for (i, line) in list.iter().enumerate() {
        let at = format!("connections[{i}]");
        let Some(line) = line.as_object() else {
            problems.push(Problem::new(&at, keyed!("kindConnections.problem.object")));
            continue;
        };
        for key in line.keys() {
            if !KEYS.contains(&key.as_str()) {
                problems.push(Problem::new(
                    &at,
                    keyed!("kindConnections.problem.key", key),
                ));
            }
        }
        match text(line, "id") {
            Some(id) if !id.is_empty() => {
                if !ids.insert(id.to_string()) {
                    problems.push(Problem::new(
                        &at,
                        keyed!("kindConnections.problem.idTwice", id),
                    ));
                }
            }
            _ => problems.push(Problem::new(
                &at,
                keyed!("kindConnections.problem.idMissing"),
            )),
        }
        for key in ["from", "to", "at", "description"] {
            if line.get(key).is_some_and(|v| !v.is_string()) {
                problems.push(Problem::new(
                    &at,
                    keyed!("kindConnections.problem.string", key),
                ));
            }
        }
        if text(line, "description").is_some_and(|d| d.chars().count() > DESCRIPTION_MAX) {
            problems.push(Problem::new(
                &at,
                keyed!(
                    "kindConnections.problem.descriptionMax",
                    max = DESCRIPTION_MAX
                ),
            ));
        }
        let Some(name) = text(line, "line") else {
            problems.push(Problem::new(
                &at,
                keyed!("kindConnections.problem.lineMissing"),
            ));
            continue;
        };
        let Some(def) = defs.line(name) else {
            problems.push(Problem::new(
                &at,
                keyed!(
                    "kindConnections.problem.lineUnknown",
                    name,
                    names = crate::text::List::of(writable_names(defs))
                ),
            ));
            continue;
        };
        if def.from_text {
            problems.push(Problem::new(
                &at,
                keyed!("kindConnections.problem.fromText", name, label = def.label),
            ));
        }
        if text(line, "to").is_none_or(str::is_empty) && !discriminated(line) {
            problems.push(Problem::new(
                &at,
                keyed!("kindConnections.problem.toMissing"),
            ));
        }
        let empty = Map::new();
        match line.get("fields") {
            None => check_fields(&def.fields, &empty, &format!("{at}.fields"), &mut problems),
            Some(Value::Object(fields)) => {
                check_fields(&def.fields, fields, &format!("{at}.fields"), &mut problems)
            }
            Some(_) => problems.push(Problem::new(
                &at,
                keyed!("kindConnections.problem.fieldsObject"),
            )),
        }
    }
    problems
}

/// The kinds of line a card may write in its connections, by name.
pub fn writable_names(defs: &Defs) -> Vec<String> {
    defs.lines
        .iter()
        // A data card's shape is its key `of`, not a line written among them.
        .filter(|l| !l.from_text && l.name != super::OF_LINE)
        .map(|l| l.name.clone())
        .collect()
}

// MARK: - Document to wire

/// A card's lines as MCP reads them: the card each reaches by its path now (as written: a
/// card ghost by its own), no ids. One whose card is gone says `to: null`.
pub fn connections_to_wire(doc: &Value, names: &dyn Names) -> Option<Value> {
    let list = connections_of(doc);
    if list.is_empty() {
        return None;
    }
    Some(Value::Array(
        list.iter()
            .filter_map(Value::as_object)
            .map(|line| {
                let mut out = Map::new();
                // What is inside a working directory's file is said past the file's path
                // (`repo/src:'a.rs'.Order.total`), as it is written.
                let inside = line
                    .get("to")
                    .and_then(Value::as_str)
                    .filter(|id| id.starts_with("file:"))
                    .and(line.get("at").and_then(Value::as_str))
                    .filter(|at| !at.is_empty());
                for key in WIRE_KEYS {
                    let Some(value) = line.get(key) else {
                        continue;
                    };
                    let value = match (key, value) {
                        ("at", _) if inside.is_some() => continue,
                        ("to", Value::String(id)) => match names.path(id) {
                            Some(path) => Value::String(match inside {
                                Some(at) => format!("{path}.{at}"),
                                None => path,
                            }),
                            None => Value::Null,
                        },
                        _ => value.clone(),
                    };
                    out.insert(key.to_string(), value);
                }
                Value::Object(out)
            })
            .collect(),
    ))
}

// MARK: - Wire to document

/// What a line is told apart by, to keep its id: its kind, its row, its card and where it
/// lands.
fn same(a: &Map<String, Value>, b: &Map<String, Value>) -> bool {
    ["line", "from", "to", "at"]
        .iter()
        .all(|k| text(a, k).unwrap_or_default() == text(b, k).unwrap_or_default())
}

/// Whether a line written is taken for one the card had.
type Alike = fn(&Map<String, Value>, &Map<String, Value>) -> bool;

/// How closely a line written is taken for one the card had, closest first: the same in
/// everything it says, the same fields, then the same ends alone. Lines between the same rows
/// that differ in their fields each find their own before any finds another's, so taking one
/// away does not give the one left the id of the one taken.
const LIKENESS: [Alike; 3] = [
    |a, b| {
        same(a, b)
            && a.get("fields") == b.get("fields")
            && a.get("description") == b.get("description")
    },
    |a, b| same(a, b) && a.get("fields") == b.get("fields"),
    same,
];

/// A card's lines read from MCP's shape: each card a path names, by its id (a path that
/// names no card is refused); a line written as one the card has (`prior`) keeps its id,
/// a new one is given one by `new_id`. `[]` in a row's path is notation and is left out.
pub fn connections_from_wire(
    value: &Value,
    prior: Option<&Value>,
    names: &dyn Names,
    new_id: &mut dyn FnMut() -> String,
) -> Result<Vec<Value>, Vec<Problem>> {
    let mut problems = Vec::new();
    let Some(list) = value.as_array() else {
        return Err(vec![Problem::new(
            "connections",
            keyed!("kindConnections.problem.listShape"),
        )]);
    };
    let before: Vec<&Map<String, Value>> = prior
        .map(connections_of)
        .unwrap_or(&[])
        .iter()
        .filter_map(Value::as_object)
        .collect();
    let mut taken = HashSet::new();
    let mut out = Vec::new();
    let mut read_all: Vec<Map<String, Value>> = Vec::new();
    // Each line whose card is gone takes back the card of one it was read as, not one another took.
    let mut gone_taken = HashSet::new();
    for (i, line) in list.iter().enumerate() {
        let at = format!("connections[{i}]");
        let Some(line) = line.as_object() else {
            problems.push(Problem::new(&at, keyed!("kindConnections.problem.object")));
            continue;
        };
        let mut read = Map::new();
        // `to: null` is a line whose card is gone, written back as it was read: it keeps
        // what it had, found again by its kind, row and landing.
        let gone = line.get("to").is_some_and(Value::is_null);
        for (key, value) in line {
            if !WIRE_KEYS.contains(&key.as_str()) {
                problems.push(Problem::new(
                    &at,
                    keyed!(
                        "kindConnections.problem.keyAllowed",
                        key,
                        allowed = crate::text::List::of(WIRE_KEYS)
                    ),
                ));
                continue;
            }
            match (key.as_str(), value) {
                (_, Value::Null) => {}
                ("to", Value::String(path)) => match names.id(path) {
                    Some(id) => {
                        read.insert("to".into(), id.into());
                    }
                    // Into a working directory's file: the file, and where in it.
                    None => match names.inside_file(path) {
                        Some((id, inside)) if !line.contains_key("at") => {
                            read.insert("to".into(), id.into());
                            read.insert("at".into(), inside.into());
                        }
                        _ => problems.push(Problem::new(
                            &format!("{at}.to"),
                            super::no_card(names, path),
                        )),
                    },
                },
                // Kept as a path writes it: each name quoted only where it needs it.
                ("from", Value::String(path)) => {
                    let path = crate::pathtext::without_alls(path.trim());
                    if !path.is_empty() {
                        read.insert(
                            "from".into(),
                            crate::pathtext::canonical(&path).into_owned().into(),
                        );
                    }
                }
                ("at", Value::String(path)) => {
                    if !path.trim().is_empty() {
                        read.insert(
                            "at".into(),
                            crate::pathtext::canonical(path.trim()).into_owned().into(),
                        );
                    }
                }
                ("description", Value::String(said)) => {
                    let said = said.trim();
                    if !said.is_empty() {
                        read.insert("description".into(), said.into());
                    }
                }
                ("fields", Value::Object(fields)) => {
                    if !fields.is_empty() {
                        read.insert("fields".into(), Value::Object(fields.clone()));
                    }
                }
                ("line", Value::String(_)) => {
                    read.insert(key.clone(), value.clone());
                }
                ("fields", _) => problems.push(Problem::new(
                    &at,
                    keyed!("kindConnections.problem.fieldsObject"),
                )),
                (other, _) => problems.push(Problem::new(
                    &at,
                    keyed!("kindConnections.problem.string", key = other),
                )),
            }
        }
        // A landing written in full in the card it reaches is its names there.
        if let (Some(Value::String(landing)), Some(Value::String(to))) =
            (read.get("at"), read.get("to"))
            && let Some(colon) = crate::pathtext::colon(landing)
        {
            let after = &landing[colon + 1..];
            let first = crate::pathtext::dots(after)[0];
            let (head, rest) = if first.len() < after.len() {
                (
                    format!(
                        "{}{}",
                        &landing[..=colon],
                        crate::pathtext::without_all(first)
                    ),
                    after[first.len() + 1..].to_string(),
                )
            } else {
                (landing.clone(), String::new())
            };
            if names.id(&head).as_deref() == Some(to.as_str()) {
                if rest.is_empty() {
                    read.remove("at");
                } else {
                    read.insert("at".into(), rest.into());
                }
            }
        }
        if gone {
            let kept = before.iter().enumerate().find(|(j, b)| {
                !gone_taken.contains(j)
                    && text(b, "to").is_some_and(|to| names.path(to).is_none())
                    && ["line", "from", "at"].iter().all(|k| {
                        text(b, k).unwrap_or_default() == text(&read, k).unwrap_or_default()
                    })
            });
            match kept.and_then(|(j, b)| text(b, "to").map(|to| (j, to))) {
                Some((j, to)) => {
                    gone_taken.insert(j);
                    read.insert("to".into(), to.into());
                }
                None => problems.push(Problem::new(
                    &format!("{at}.to"),
                    keyed!("kindConnections.problem.goneNull"),
                )),
            }
        }
        read_all.push(read);
    }
    if !problems.is_empty() {
        return Err(problems);
    }
    let mut kept_id: Vec<Option<usize>> = vec![None; read_all.len()];
    for like in LIKENESS {
        for (i, read) in read_all.iter().enumerate() {
            if kept_id[i].is_some() {
                continue;
            }
            if let Some(j) =
                (0..before.len()).find(|j| !taken.contains(j) && like(before[*j], read))
            {
                taken.insert(j);
                kept_id[i] = Some(j);
            }
        }
    }
    for (read, earlier) in read_all.iter().zip(kept_id) {
        let id = earlier
            .and_then(|j| text(before[j], "id"))
            .map(str::to_string)
            .unwrap_or_else(&mut *new_id);
        let mut written = Map::new();
        written.insert("id".into(), id.into());
        for key in WIRE_KEYS {
            if let Some(v) = read.get(key) {
                written.insert(key.to_string(), v.clone());
            }
        }
        out.push(Value::Object(written));
    }
    Ok(out)
}
