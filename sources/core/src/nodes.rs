//! A flow's nodes of before — rows of the flow (`rows.nodes`), their ways named by node
//! name and their places by path — made cards of their own inside the flow, the ways and
//! paths the flow's lines.
//!
//! The one way a node of before becomes a card: read by the move of a store and a project file
//! (`container_move`).
//!
//! - Its name and kind are the card's; its fields its kind still has are the card's fields
//!   (`label`, `description`, `trigger` …; an end's `return` without what became lines), and
//!   a process node's rows (`steps`) its rows, each without what became lines.
//! - Ways to other nodes are lines to the node cards: `next`, `branches` (`branch`, the
//!   condition its field `when`), `else`, `onError`, `onTimeout`.
//! - Paths are lines to what they name: a run's `target` (`runs`), an event's flow
//!   (`event`, its outcome the field `outcome`), a peer (`peer`), a key (`key`, `use` sign or
//!   verify), a read's or a write's paths (`reads`, `writes`, from the row: `steps[0]`). What
//!   it carries, returns, the value an eval reads and what a signature covers stay paths.
//! - A path that names nothing cannot be a line: it is written at the end of the node's
//!   description, and said. A way to a node the flow does not have is said and let go.

use serde_json::{Map, Value};
use std::collections::HashMap;

type Obj = Map<String, Value>;

/// The fields each kind of node keeps as fields (the rest of before became lines).
fn kept_fields(kind: &str) -> &'static [&'static str] {
    match kind {
        "begin" => &["trigger", "every", "program", "carries", "via", "initiator"],
        "run" => &["carries", "returns", "via"],
        "eval" => &["value"],
        "wait" => &["for", "timeout"],
        "join" => &["join", "partial"],
        "end" => &["outcome"],
        _ => &[],
    }
}

/// The fields a process node's row keeps as fields.
const STEP_FIELDS: [&str; 8] = [
    "do",
    "description",
    "paths",
    "program",
    "carries",
    "returns",
    "via",
    "initiator",
];

/// What a line reaches: another node of the flow, by its name, or a place by its path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Reach {
    Node(String),
    Path(String),
}

/// A line of a node, before its card and place are read into ids.
#[derive(Clone, Debug)]
pub(crate) struct Way {
    pub line: &'static str,
    /// The row it leaves (`steps[0]`); none for the node's head.
    pub from: Option<String>,
    pub reach: Reach,
    pub fields: Obj,
    /// Where it was written, for what is said when it cannot be a line: `target`,
    /// `steps[0].paths`.
    pub said: String,
}

/// A node of before, read: its fields (what the card keeps), its rows' fields, its lines.
#[derive(Clone, Debug, Default)]
pub(crate) struct Node {
    pub name: String,
    pub kind: String,
    pub fields: Obj,
    pub steps: Vec<Obj>,
    pub ways: Vec<Way>,
}

fn strings(value: Option<&Value>) -> Vec<String> {
    match value {
        Some(Value::String(s)) if !s.is_empty() => vec![s.clone()],
        Some(Value::Array(a)) => a
            .iter()
            .filter_map(|v| v.as_str().filter(|s| !s.is_empty()).map(str::to_string))
            .collect(),
        _ => Vec::new(),
    }
}

fn one(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

fn with(fields: &[(&str, Value)]) -> Obj {
    fields
        .iter()
        .map(|(k, v)| (k.to_string(), v.clone()))
        .collect()
}

/// A node of before, as its fields (`next` one or many) and its rows' fields, read into
/// what its card keeps and its lines.
pub(crate) fn read(node: &Obj, steps: &[Obj]) -> Node {
    let name = one(node.get("name")).unwrap_or_default();
    let kind = one(node.get("kind")).unwrap_or_default();
    let mut fields = Obj::new();
    if let Some(v) = node.get("label") {
        fields.insert("label".into(), v.clone());
    }
    match node.get("description").or_else(|| node.get("note")) {
        Some(v) if !v.is_null() => {
            fields.insert("description".into(), v.clone());
        }
        _ => {}
    }
    for key in kept_fields(&kind) {
        if let Some(v) = node.get(*key).filter(|v| !v.is_null()) {
            fields.insert(key.to_string(), v.clone());
        }
    }
    let mut ways = Vec::new();
    let head = |line: &'static str, reach: Reach, fields: Obj, said: &str| Way {
        line,
        from: None,
        reach,
        fields,
        said: said.to_string(),
    };
    for to in strings(node.get("next")) {
        ways.push(head("next", Reach::Node(to), Obj::new(), "next"));
    }
    for (i, branch) in node
        .get("branches")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .enumerate()
    {
        if let Some(to) = one(branch.get("next")) {
            let when = branch
                .get("when")
                .cloned()
                .unwrap_or(Value::String(String::new()));
            ways.push(head(
                "branch",
                Reach::Node(to),
                with(&[("when", when)]),
                &format!("branches[{i}]"),
            ));
        }
    }
    for (key, line) in [
        ("else", "else"),
        ("onError", "onError"),
        ("onTimeout", "onTimeout"),
    ] {
        if let Some(to) = one(node.get(key)) {
            ways.push(head(line, Reach::Node(to), Obj::new(), key));
        }
    }
    if let Some(target) = one(node.get("target")) {
        ways.push(head("runs", Reach::Path(target), Obj::new(), "target"));
    }
    if let Some(event) = node.get("event").and_then(Value::as_object)
        && let Some(flow) = one(event.get("flow"))
    {
        let outcome = event.get("outcome").cloned().unwrap_or(Value::Null);
        let fields = if outcome.is_null() {
            Obj::new()
        } else {
            with(&[("outcome", outcome)])
        };
        ways.push(head("event", Reach::Path(flow), fields, "event.flow"));
    }
    let key_use = |sign: bool| with(&[("use", Value::from(if sign { "sign" } else { "verify" }))]);
    match kind.as_str() {
        "begin" => {
            if let Some(peer) = one(node.get("peer")) {
                ways.push(head("peer", Reach::Path(peer), Obj::new(), "peer"));
            }
            if let Some(key) = one(node.get("verifyWith")) {
                ways.push(head("key", Reach::Path(key), key_use(false), "verifyWith"));
            }
        }
        "run" => {
            if let Some(key) = one(node.get("signWith")) {
                ways.push(head("key", Reach::Path(key), key_use(true), "signWith"));
            }
            if let Some(key) = one(node.get("verifyWith")) {
                ways.push(head("key", Reach::Path(key), key_use(false), "verifyWith"));
            }
        }
        "end" => {
            if let Some(reply) = node.get("return").and_then(Value::as_object) {
                let mut kept = Obj::new();
                for key in ["carries", "program"] {
                    if let Some(v) = reply.get(key).filter(|v| !v.is_null()) {
                        kept.insert(key.into(), v.clone());
                    }
                }
                if let Some(peer) = one(reply.get("peer")) {
                    ways.push(head("peer", Reach::Path(peer), Obj::new(), "return.peer"));
                }
                if let Some(key) = one(reply.get("signWith")) {
                    ways.push(head(
                        "key",
                        Reach::Path(key),
                        key_use(true),
                        "return.signWith",
                    ));
                }
                // An answer with nothing but lines left is still an answer: its fields
                // stay, empty, when it had none of them.
                if !kept.is_empty() || reply.is_empty() {
                    fields.insert("return".into(), Value::Object(kept));
                }
            }
        }
        _ => {}
    }
    let mut rows = Vec::new();
    for (i, step) in steps.iter().enumerate() {
        let from = format!("steps[{i}]");
        let op = one(step.get("do")).unwrap_or_default();
        let mut kept = Obj::new();
        for key in STEP_FIELDS {
            let Some(v) = step.get(key).filter(|v| !v.is_null()) else {
                continue;
            };
            // What a read or a write names are its lines.
            if key == "paths" && op != "sign" && op != "verify" {
                continue;
            }
            kept.insert(key.into(), v.clone());
        }
        if step.get("description").is_none()
            && let Some(v) = step.get("comment").filter(|v| !v.is_null())
        {
            kept.insert("description".into(), v.clone());
        }
        let row = |line: &'static str, reach: Reach, fields: Obj, said: String| Way {
            line,
            from: Some(from.clone()),
            reach,
            fields,
            said,
        };
        if op == "read" || op == "write" {
            let line = if op == "read" { "reads" } else { "writes" };
            for (j, path) in strings(step.get("paths")).into_iter().enumerate() {
                ways.push(row(
                    line,
                    Reach::Path(path),
                    Obj::new(),
                    format!("steps[{i}].paths[{j}]"),
                ));
            }
        }
        if let Some(key) = one(step.get("key")) {
            ways.push(row(
                "key",
                Reach::Path(key),
                key_use(op == "sign"),
                format!("steps[{i}].key"),
            ));
        }
        if let Some(peer) = one(step.get("peer")) {
            ways.push(row(
                "peer",
                Reach::Path(peer),
                Obj::new(),
                format!("steps[{i}].peer"),
            ));
        }
        if let Some(key) = one(step.get("signWith")) {
            ways.push(row(
                "key",
                Reach::Path(key),
                key_use(true),
                format!("steps[{i}].signWith"),
            ));
        }
        if let Some(key) = one(step.get("verifyWith")) {
            ways.push(row(
                "key",
                Reach::Path(key),
                key_use(false),
                format!("steps[{i}].verifyWith"),
            ));
        }
        rows.push(kept);
    }
    Node {
        name,
        kind,
        fields,
        steps: rows,
        ways,
    }
}

/// A node of before as its document keeps it in the flow (`rows.nodes`): its fields, its
/// rows' fields under `steps`.
pub(crate) fn read_row(row: &Value) -> (Node, String, Vec<String>) {
    let fields = row
        .get("fields")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let step_rows: Vec<&Value> = row
        .get("rows")
        .and_then(|r| r.get("steps"))
        .and_then(Value::as_array)
        .map(|a| a.iter().collect())
        .unwrap_or_default();
    let steps: Vec<Obj> = step_rows
        .iter()
        .map(|s| {
            s.get("fields")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default()
        })
        .collect();
    let step_ids: Vec<String> = step_rows
        .iter()
        .map(|s| {
            s.get("id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        })
        .collect();
    let id = row
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    (read(&fields, &steps), id, step_ids)
}

/// A node card made, and what of its node could not be a line.
pub(crate) struct Made {
    pub doc: Obj,
    pub lost: Vec<crate::text::Keyed>,
}

/// What a node card is made in: its flow and section, its place among the flow's nodes,
/// and how a path and a node's name are read into what a line holds.
pub(crate) struct Making<'a> {
    pub flow: &'a str,
    pub section: &'a str,
    pub order: i64,
    pub id: &'a str,
    /// The rows' ids, by place (made anew past the end).
    pub step_ids: &'a [String],
    /// A node of the flow by its name: its card's id.
    pub nodes: &'a HashMap<String, String>,
    /// A path: the id of the card (or card ghost) it starts in and the names after it there,
    /// as written; none when it names nothing.
    pub resolve: &'a dyn Fn(&str) -> Option<(String, String)>,
    /// A line's id: from what it is (`connection`).
    pub line_id: &'a mut dyn FnMut(&str, &str, &str, &str) -> String,
    pub new_id: &'a mut dyn FnMut() -> String,
}

/// A connection's id made from what it is (`line_move` makes them alike).
pub(crate) fn line_id(card: &str, line: &str, from: &str, to: &str, at: &str, n: usize) -> String {
    crate::ids::made_id(&["connection", card, line, from, to, at, &n.to_string()])
}

/// The card a node of before becomes, in its flow.
pub(crate) fn card(node: &Node, making: &mut Making) -> Made {
    let mut doc = Obj::new();
    doc.insert("id".into(), making.id.into());
    doc.insert("kind".into(), node.kind.clone().into());
    doc.insert("name".into(), node.name.clone().into());
    doc.insert("section".into(), making.section.into());
    doc.insert("in".into(), making.flow.into());
    doc.insert("v".into(), crate::kinds::builtin().version.into());
    if making.order != 0 {
        doc.insert("order".into(), making.order.into());
    }
    let mut fields = node.fields.clone();
    // What is said of each way that could not be a line, and (for a path) what is written at the
    // end of the card's description.
    let mut lost: Vec<crate::text::Keyed> = Vec::new();
    let mut unread: Vec<String> = Vec::new();
    let mut lines: Vec<Value> = Vec::new();
    for way in &node.ways {
        let (to, at) = match &way.reach {
            Reach::Node(name) => match making.nodes.get(name) {
                Some(id) => (id.clone(), String::new()),
                None => {
                    lost.push(keyed!(
                        "nodes.line.nodeMissing",
                        node = node.name.as_str(),
                        said = way.said.as_str(),
                        name
                    ));
                    continue;
                }
            },
            Reach::Path(path) => match (making.resolve)(path) {
                Some(found) => found,
                None => {
                    unread.push(format!("{}: {path}", way.said));
                    lost.push(keyed!(
                        "nodes.line.pathWithoutPlace",
                        node = node.name.as_str(),
                        said = way.said.as_str(),
                        path
                    ));
                    continue;
                }
            },
        };
        let from = way.from.clone().unwrap_or_default();
        let id = (making.line_id)(way.line, &from, &to, &at);
        let mut line = Obj::new();
        line.insert("id".into(), id.into());
        line.insert("line".into(), way.line.into());
        if !from.is_empty() {
            line.insert("from".into(), from.into());
        }
        line.insert("to".into(), to.into());
        if !at.is_empty() {
            line.insert("at".into(), at.into());
        }
        if !way.fields.is_empty() {
            line.insert("fields".into(), Value::Object(way.fields.clone()));
        }
        lines.push(Value::Object(line));
    }
    // What names nothing is kept where a person reads it.
    if !unread.is_empty() {
        let mut description = fields
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        if !description.is_empty() {
            description.push_str("\n\n");
        }
        description.push_str("（ノードの移し替え 2026-10-05 で線にできなかったもの）");
        for one in &unread {
            description.push_str("\n- ");
            description.push_str(one);
        }
        fields.insert("description".into(), description.into());
    }
    if !fields.is_empty() {
        doc.insert("fields".into(), Value::Object(fields));
    }
    if !node.steps.is_empty() {
        let steps: Vec<Value> = node
            .steps
            .iter()
            .enumerate()
            .map(|(i, step)| {
                let id = making
                    .step_ids
                    .get(i)
                    .filter(|s| !s.is_empty())
                    .cloned()
                    .unwrap_or_else(|| (making.new_id)());
                let mut row = Obj::new();
                row.insert("id".into(), id.into());
                if !step.is_empty() {
                    row.insert("fields".into(), Value::Object(step.clone()));
                }
                Value::Object(row)
            })
            .collect();
        let mut rows = Obj::new();
        rows.insert("steps".into(), Value::Array(steps));
        doc.insert("rows".into(), Value::Object(rows));
    }
    if !lines.is_empty() {
        doc.insert("connections".into(), Value::Array(lines));
    }
    Made { doc, lost }
}
