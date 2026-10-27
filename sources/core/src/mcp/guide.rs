//! `guide`: what an agent reads besides the list, a topic at a time and each short — a tool's
//! purpose, its `value` by form as a table of keys, its answer, an example and the refusals easy to
//! hit; a kind's own keys with an example and what it is for; how one key or mechanism is written;
//! the catalog's longer guides. A topic says a thing once and points at another topic rather than
//! copying it, and inside one answer what was said already is pointed at (`same`).

use super::{Output, brief, catalog, text, told, tool, tools};
use crate::ojson;
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::OnceLock;

/// Every key with a shape inside it, in its fullest spelling, with where it is taken
/// (`argumentShapes`).
pub fn argument_shapes() -> BTreeMap<String, (Value, Vec<String>)> {
    argument_forms()
        .into_iter()
        .map(|(key, mut forms)| (key, forms.remove(0)))
        .collect()
}

/// Each key's shapes, the fullest first and then any other a place writes it in (`card_update`'s
/// `connections` adds and takes away, `card_create`'s is the list): each with where it is.
pub fn argument_forms() -> BTreeMap<String, Vec<(Value, Vec<String>)>> {
    fn more(object: &Map<String, Value>) -> bool {
        if object.contains_key("properties") {
            return true;
        }
        let Some(Value::Object(items)) = object.get("items") else {
            return false;
        };
        items.contains_key("properties")
            || (items.get("type").and_then(Value::as_str) == Some("object")
                && object.contains_key("description"))
    }
    fn walk(spec: &Value, place: &str, found: &mut BTreeMap<String, Vec<(Value, String)>>) {
        let Value::Object(mut body) = spec.clone() else {
            return;
        };
        let mut place = place.to_string();
        if let Some(Value::Object(items)) = body.get("items").cloned() {
            body = items;
            place += "[]";
        }
        let Some(Value::Object(properties)) = body.get("properties") else {
            return;
        };
        for (key, value) in properties {
            let here = format!("{place}.{key}");
            if let Value::Object(object) = value
                && more(object)
            {
                found
                    .entry(key.clone())
                    .or_default()
                    .push((value.clone(), here.clone()));
            }
            walk(value, &here, found);
        }
    }
    fn keys(spec: &Value) -> HashSet<String> {
        let Value::Object(mut body) = spec.clone() else {
            return HashSet::new();
        };
        if let Some(Value::Object(items)) = body.get("items").cloned() {
            body = items;
        }
        match body.get("properties") {
            Some(Value::Object(p)) => p.keys().cloned().collect(),
            _ => HashSet::new(),
        }
    }
    let mut found: BTreeMap<String, Vec<(Value, String)>> = BTreeMap::new();
    for t in told() {
        let name = t["name"].as_str().unwrap_or_default();
        walk(&t["inputSchema"], name, &mut found);
        for form in t["forms"].as_array().map(Vec::as_slice).unwrap_or(&[]) {
            walk(
                &form["spec"],
                &format!(
                    "{name}.value({})",
                    form["label"].as_str().unwrap_or_default()
                ),
                &mut found,
            );
        }
    }
    let mut shapes = BTreeMap::new();
    for (key, places) in found {
        let mut forms: Vec<(Value, Vec<String>)> = Vec::new();
        type Place<'p> = &'p (Value, String);
        let mut left: Vec<Place> = places.iter().collect();
        while !left.is_empty() {
            // The fullest: the longest written, the first of those.
            let mut fullest = left[0];
            for p in &left {
                if ojson::compact(&p.0).chars().count() > ojson::compact(&fullest.0).chars().count()
                {
                    fullest = p;
                }
            }
            let shape = keys(&fullest.0);
            let kind = |v: &Value| v.get("type").cloned();
            let (here, rest): (Vec<Place>, Vec<Place>) = left
                .into_iter()
                .partition(|p| keys(&p.0).is_subset(&shape) && kind(&p.0) == kind(&fullest.0));
            forms.push((
                fullest.0.clone(),
                here.iter().map(|p| p.1.clone()).collect(),
            ));
            left = rest;
        }
        shapes.insert(key, forms);
    }
    shapes
}

// MARK: - What can be asked

/// A flow's kinds of node, each a topic of its own.
pub(crate) const NODE_KINDS: [&str; 7] = ["begin", "process", "run", "eval", "wait", "join", "end"];

/// The built-in kinds of card `guide` tells of by their names, in the order the topic list gives
/// them (the nodes after them).
pub(crate) const CARD_KINDS: [&str; 17] = [
    "entity",
    "value",
    "enum",
    "store",
    "instance",
    "table",
    "flow",
    "note",
    "freedoc",
    "formatdoc",
    "stackdoc",
    "svg",
    "html",
    "frame",
    "goal",
    "task",
    "ghost",
];

/// The keys of a card's value that only some kinds take, each with those kinds. Every other key of
/// the card's form is every kind's (where it sits and its size are every boxed card's): the topics
/// of `card_create` and `card_update` say those, and a kind's topic its own.
const KIND_KEYS: [(&str, &[&str]); 26] = [
    ("fields", &["entity", "value"]),
    ("rules", &["entity", "value"]),
    ("absentFields", &["entity", "value"]),
    ("discriminator", &["entity", "value"]),
    ("face", &["entity", "value"]),
    ("standalone", &["value", "enum"]),
    ("values", &["enum"]),
    ("open", &["enum"]),
    ("of", &["instance", "table"]),
    ("data", &["instance"]),
    ("within", &["table"]),
    ("where", &["table"]),
    ("columns", &["table"]),
    ("sort", &["table"]),
    ("unnest", &["table"]),
    ("diagram", &["flow"]),
    ("body", &["note", "freedoc", "svg", "html", "goal", "task"]),
    ("rows", &["formatdoc"]),
    ("view", &["formatdoc"]),
    ("versions", &["stackdoc"]),
    ("version", &["stackdoc"]),
    ("places", &["goal"]),
    ("target", &["ghost", "frame"]),
    ("headerPlace", &["svg", "html", "frame"]),
    ("note", &["svg", "html", "frame"]),
    ("notePlace", &["svg", "html", "frame"]),
];

/// The forms beside the card's that only some kinds take: a field's (a target written down to a
/// field), a formatdoc's row's, a stackdoc's version's, a data card's value's, an enum's value's and
/// a process node's step's.
const KIND_FORMS: [(&str, &[&str]); 6] = [
    ("field", &["entity", "value"]),
    ("row", &["formatdoc"]),
    ("version", &["stackdoc"]),
    ("value", &["instance"]),
    ("enumValue", &["enum"]),
    ("step", &["process"]),
];

/// The keys of a node's value that only some kinds of node take; the rest of the node's form is
/// every node's.
const NODE_KEYS: [(&str, &[&str]); 16] = [
    ("trigger", &["begin"]),
    ("every", &["begin"]),
    ("program", &["begin"]),
    ("initiator", &["begin"]),
    ("anyCaller", &["begin"]),
    ("carries", &["begin", "run"]),
    ("via", &["begin", "run"]),
    ("returns", &["run"]),
    ("steps", &["process"]),
    ("value", &["eval"]),
    ("for", &["wait"]),
    ("timeout", &["wait"]),
    ("join", &["join"]),
    ("partial", &["join"]),
    ("outcome", &["end"]),
    ("return", &["end"]),
];

/// The topics that are a text of the catalog's, said as it is: by name, the catalog's key.
const TEXTS: [(&str, &str); 9] = [
    ("glossary", "glossary"),
    ("branch", "branchGuide"),
    ("board", "boardGuide"),
    ("kinds", "kindsGuide"),
    ("ownKinds", "ownKindGuide"),
    ("rev", "revGuide"),
    ("paging", "pagingGuide"),
    ("paths", "pathsGuide"),
    ("sdd", "sddGuide"),
];

/// The keys whose topic says in words how they are written, before their shape: by name, the
/// catalog's key, and whether the words say each key inside it too (the shape then says only
/// their types).
const KEY_TEXTS: [(&str, &str, bool); 3] = [
    ("connections", "linesGuide", true),
    ("body", "bodyGuide", true),
    ("layout", "layoutGuide", false),
];

/// The keys a table says in a line and leaves to a topic of their own (a line's shape would
/// otherwise be said in every form that takes lines): by key, the topic.
const POINTED: [(&str, &str); 8] = [
    ("connections", "connections"),
    ("rev", "rev"),
    ("body", "body"),
    ("rules", "rules"),
    ("cells", "cells"),
    ("topLeft", "layout"),
    ("size", "layout"),
    ("places", "layout"),
];

fn kinds_of(
    table: &[(&str, &'static [&'static str])],
    key: &str,
) -> Option<&'static [&'static str]> {
    table
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, kinds)| *kinds)
}

/// What one `about` names.
enum Topic {
    Text(&'static str),
    Kind(&'static str),
    Tool(&'static Value),
    Key(String),
}

fn topic_of(about: &str) -> Option<Topic> {
    if let Some((_, key)) = TEXTS.iter().find(|(name, _)| *name == about) {
        return Some(Topic::Text(key));
    }
    // custom_goal is another name of goal.
    let about = if about == "custom_goal" {
        "goal"
    } else {
        about
    };
    if let Some(kind) = CARD_KINDS
        .iter()
        .chain(NODE_KINDS.iter())
        .find(|k| **k == about)
        .filter(|k| crate::kinds::builtin_definition(k).is_some())
    {
        return Some(Topic::Kind(kind));
    }
    // Any tool, `guide` itself too (its refusals point at it).
    if let Some(t) = tools().iter().find(|t| t["name"] == about) {
        return Some(Topic::Tool(t));
    }
    if KEY_TEXTS.iter().any(|(k, _, _)| *k == about) || argument_forms().contains_key(about) {
        return Some(Topic::Key(about.to_string()));
    }
    None
}

// MARK: - guide

/// `guide` of one topic, or of none: the topics there are, each with its size.
pub fn guide(about: Option<&str>) -> Result<Output, String> {
    let about = about.map(str::trim).filter(|a| !a.is_empty());
    let Some(about) = about else {
        return Ok(Output::Json(index().clone()));
    };
    let topic = topic_of(about).ok_or_else(|| unknown(about))?;
    let mut said = Said::default();
    Ok(match said.topic(&topic) {
        Value::String(text) => Output::Blocks(vec![text]),
        mut answer => {
            said_once(&mut answer);
            Output::Json(answer)
        }
    })
}

/// `guide` of several topics at once: each by its name, what was said in one pointed at from the
/// others. Names it does not know are left to `elsewhere` (a project's own kind), and refused
/// when it does not know them either.
pub fn guide_of(
    abouts: &[String],
    elsewhere: &dyn Fn(&str) -> Option<Value>,
) -> Result<Output, String> {
    let mut said = Said::default();
    let mut out = Map::new();
    let mut missing = Vec::new();
    for about in abouts.iter().map(|a| a.trim()).filter(|a| !a.is_empty()) {
        match topic_of(about) {
            Some(topic) => {
                said.topic = format!("{about}.");
                let answer = said.topic(&topic);
                out.insert(about.to_string(), answer);
            }
            None => match elsewhere(about) {
                Some(answer) => {
                    out.insert(about.to_string(), answer);
                }
                None => missing.push(about.to_string()),
            },
        }
    }
    if !missing.is_empty() {
        return Err(unknown_names(&missing));
    }
    let mut answer = Value::Object(out);
    said_once(&mut answer);
    Ok(Output::Json(answer))
}

/// A name `guide` has no topic of, refused with what it may have meant: each name with the topics
/// close to it and, when it is a value a tool's key takes (`impact`, `as` of `card_read`), that key.
fn unknown(about: &str) -> String {
    unknown_names(&[about.to_string()])
}

/// `unknown`, of several names at once.
fn unknown_names(names: &[String]) -> String {
    let mut said = text!("mcp.guide.unknownAbout", about = crate::text::list(names));
    for name in names {
        let close = close_topics(name);
        if !close.is_empty() {
            said += &text!(
                "mcp.guide.closeTopics",
                name,
                topics = crate::text::list(&close)
            );
        }
        let takers = taken_by(name);
        if !takers.is_empty() {
            said += &text!("mcp.guide.valueOf", name, keys = crate::text::list(&takers));
        }
    }
    said
}

/// The topics whose names are close to `name`: one holding the other, ignoring case, or a few
/// letters apart — at most five.
fn close_topics(name: &str) -> Vec<String> {
    let wanted = name.to_lowercase();
    let mut names: Vec<String> = told()
        .map(|t| t["name"].as_str().unwrap_or_default().to_string())
        .chain(
            CARD_KINDS
                .iter()
                .chain(NODE_KINDS.iter())
                .map(|k| k.to_string()),
        )
        .chain(TEXTS.iter().map(|(n, _)| n.to_string()))
        .chain(KEY_TEXTS.iter().map(|(k, _, _)| k.to_string()))
        .chain(argument_forms().into_keys())
        .collect();
    names.sort();
    names.dedup();
    let near = |other: &str| {
        let other = other.to_lowercase();
        (wanted.len() >= 3 && (other.contains(&wanted) || wanted.contains(&other)))
            || apart(&wanted, &other) <= (wanted.chars().count() / 4).clamp(1, 2)
    };
    names.retain(|n| near(n));
    names.truncate(5);
    names
}

/// How many letters two names are apart (Levenshtein).
fn apart(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut diagonal = row[0];
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let above = row[j + 1];
            row[j + 1] = (diagonal + usize::from(ca != *cb))
                .min(row[j] + 1)
                .min(above + 1);
            diagonal = above;
        }
    }
    row[b.len()]
}

/// The tools' keys that take `name` as one of their values (`value.as` of `card_read`).
fn taken_by(name: &str) -> Vec<String> {
    fn walk(spec: &Value, place: &str, name: &str, found: &mut Vec<String>) {
        let Value::Object(body) = spec else {
            return;
        };
        if let Some(Value::Array(values)) = body.get("enum")
            && values.iter().any(|v| v == name)
            && !found.iter().any(|f| f == place)
        {
            found.push(place.to_string());
        }
        if let Some(items) = body.get("items") {
            walk(items, place, name, found);
        }
        if let Some(Value::Object(properties)) = body.get("properties") {
            for (key, inner) in properties {
                walk(inner, &format!("{place}.{key}"), name, found);
            }
        }
    }
    let mut found = Vec::new();
    for t in tools() {
        let tool = t["name"].as_str().unwrap_or_default();
        let mut here = Vec::new();
        if let Some(Value::Object(properties)) = t["inputSchema"].get("properties") {
            for (key, inner) in properties {
                walk(inner, key, name, &mut here);
            }
        }
        for form in t["forms"].as_array().map(Vec::as_slice).unwrap_or(&[]) {
            walk(&form["spec"], "value", name, &mut here);
        }
        found.extend(
            here.into_iter()
                .map(|key| text!("mcp.guide.keyOf", tool, key)),
        );
    }
    found
}

/// The topics there are, by family, each with how long its answer is in the language of the call.
fn index() -> &'static Value {
    static MADE: [OnceLock<Value>; crate::text::LANGUAGES.len()] =
        [const { OnceLock::new() }; crate::text::LANGUAGES.len()];
    MADE[crate::text::current_index()].get_or_init(|| {
        let size = |about: &str| -> Value {
            match guide(Some(about)) {
                Ok(output) => output_size(&output).into(),
                Err(_) => Value::Null,
            }
        };
        let family = |names: Vec<String>| -> Value {
            Value::Object(names.iter().map(|n| (n.clone(), size(n))).collect())
        };
        let mut keys: Vec<String> = argument_forms().into_keys().collect();
        for (key, _, _) in KEY_TEXTS {
            if !keys.iter().any(|k| k == key) {
                keys.push(key.to_string());
            }
        }
        keys.sort();
        json!({
            "about": text!("mcp.guide.index"),
            "tools": family(told().map(|t| t["name"].as_str().unwrap_or_default().to_string()).collect()),
            "kinds": family(CARD_KINDS.iter().chain(NODE_KINDS.iter()).map(|k| k.to_string()).collect()),
            "keys": family(keys),
            "guides": family(TEXTS.iter().map(|(n, _)| n.to_string()).collect()),
        })
    })
}

/// How many characters an answer is as it is sent (packed JSON, or its text).
fn output_size(output: &Output) -> usize {
    match output {
        Output::Json(v) => ojson::compact(v).chars().count(),
        other => other.text().chars().count(),
    }
}

// MARK: - The topics

/// The op a form of a tool is for, when a top-level `op` picks it (its label `op: <op>`).
fn op_of(form: &Value) -> Option<&str> {
    form["label"].as_str()?.strip_prefix("op: ")
}

/// What one answer has said so far: each line of a table, by its key's last name and what it says,
/// with where it was said.
#[derive(Default)]
struct Said {
    lines: HashMap<(String, String), String>,
    /// Where the topic being said stands in the answer: its name, when it is one of several.
    topic: String,
    /// The words the kind being said has for a key in place of the form's own (`kindKeyTexts`).
    kind_texts: Vec<(String, String)>,
}

impl Said {
    fn topic(&mut self, topic: &Topic) -> Value {
        match topic {
            Topic::Text(key) => text(key).into(),
            Topic::Kind(kind) => self.kind(kind),
            Topic::Tool(t) => self.tool(t),
            Topic::Key(key) => self.key(key),
        }
    }

    /// A tool: what it is for, its value by form, what it answers, an example and the refusals
    /// easy to hit. `card_create`'s and `card_update`'s say the keys every kind takes, and where
    /// the rest are.
    fn tool(&mut self, t: &Value) -> Value {
        let name = t["name"].as_str().unwrap_or_default();
        let mut out = Map::new();
        out.insert("tool".into(), name.into());
        out.insert("purpose".into(), t["description"].clone());
        let forms = t["forms"].as_array().map(Vec::as_slice).unwrap_or(&[]);
        // A tool whose forms a top-level `op` picks: `op` said beside target, and value said once
        // for each op that takes one, under a key that names the op — a form's label inside value
        // would read as a key of value, which the tool does not take.
        if let Some(ops) = t["inputSchema"]["properties"]["op"]["enum"].as_array()
            && !forms.is_empty()
            && forms.iter().all(|f| op_of(f).is_some())
        {
            let ops: Vec<&str> = ops.iter().filter_map(Value::as_str).collect();
            out.insert(
                "op".into(),
                text!("mcp.guide.opArgument", ops = ops.join("|")).into(),
            );
            for form in forms {
                let empty = form["spec"]["properties"]
                    .as_object()
                    .is_none_or(Map::is_empty);
                if empty {
                    continue;
                }
                let at = format!("value (op: {})", op_of(form).unwrap_or_default());
                let table = self.table(&form["spec"], &at, "");
                out.insert(at, table.into());
            }
            self.told(name, &mut out);
            return Value::Object(out);
        }
        let mut value = Map::new();
        let card_tool = ["card_create", "card_update"].contains(&name);
        for form in forms {
            let label = form["label"].as_str().unwrap_or_default();
            let at = format!("value.{label}");
            if card_tool && label == "card" {
                let spec = keeping(&form["spec"], &|k| kinds_of(&KIND_KEYS, k).is_none());
                let mut table = self.table(&spec, &at, "");
                table.insert("…".into(), text!("mcp.guide.kindKeys").into());
                value.insert(label.into(), table.into());
            } else if card_tool {
                value.insert(label.into(), form_pointer(label).into());
            } else {
                value.insert(label.into(), self.table(&form["spec"], &at, "").into());
            }
        }
        // A tool with one form takes its keys directly in value: a label naming that one form would
        // read as a key to nest them under (`value: {list: {...}}`), which the tool does not take.
        // Several forms are shown by label because value takes the shape of one of them.
        if forms.len() == 1
            && let Some(Value::Object(only)) =
                value.shift_remove(forms[0]["label"].as_str().unwrap_or_default())
        {
            value = only;
        }
        if !value.is_empty() {
            out.insert("value".into(), value.into());
        }
        self.told(name, &mut out);
        Value::Object(out)
    }

    /// What the catalog writes of a tool, after its keys: what it answers, an example, the refusals
    /// easy to hit.
    fn told(&self, name: &str, out: &mut Map<String, Value>) {
        let written = &catalog()["toolGuides"][name];
        for key in ["answer", "example", "refusals"] {
            if let Some(said) = written.get(key) {
                out.insert(key.into(), said.clone());
            }
        }
        if ["kind_create", "kind_update"].contains(&name) {
            out.insert("more".into(), text!("mcp.guide.kindsMore").into());
        }
    }

    /// A built-in kind: what it is for, the keys only it takes (a node: and the keys every node
    /// takes), the forms written to a target inside it, and an example.
    fn kind(&mut self, kind: &str) -> Value {
        let Some(definition) = crate::kinds::builtin_definition(kind) else {
            return Value::Null;
        };
        // One key is said differently by kinds that share it: `target` of a ghost points at a
        // card, a frame's at a section.
        self.kind_texts = catalog()["kindKeyTexts"][kind]
            .as_object()
            .into_iter()
            .flatten()
            .filter_map(|(key, said)| Some((key.clone(), said.as_str()?.to_string())))
            .collect();
        let mut out = Map::new();
        out.insert("kind".into(), kind.into());
        out.insert("label".into(), definition["label"].clone());
        out.insert("context".into(), definition["context"].clone());
        if NODE_KINDS.contains(&kind) {
            let own = |k: &str| kinds_of(&NODE_KEYS, k).is_some_and(|ks| ks.contains(&kind));
            if let Some(keys) = self.both("node", &own, "keys", false) {
                out.insert("keys".into(), keys);
            }
            let any = |k: &str| kinds_of(&NODE_KEYS, k).is_none();
            if let Some(keys) = self.both("node", &any, "nodeKeys", true) {
                out.insert("nodeKeys".into(), keys);
            }
            self.kind_forms(kind, &mut out);
        } else {
            let own = |k: &str| kinds_of(&KIND_KEYS, k).is_some_and(|ks| ks.contains(&kind));
            if let Some(keys) = self.both("card", &own, "keys", false) {
                out.insert("keys".into(), keys);
            }
            if let Some(fields) = definition_fields(definition) {
                out.insert("fields".into(), fields);
            }
            self.kind_forms(kind, &mut out);
        }
        // What a card of it is made to do in one go (`card_action`).
        let ops: Vec<String> = definition["actions"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|o| o.as_str().map(str::to_string))
            .collect();
        if let Some(actions) = crate::tools::described_actions(&ops) {
            out.insert("actions".into(), actions);
        }
        if kind == "flow" {
            let nodes: Vec<Value> = NODE_KINDS
                .iter()
                .filter_map(|k| {
                    let d = crate::kinds::builtin_definition(k)?;
                    Some(Value::String(text!(
                        "mcp.guide.kindLine",
                        kind = k,
                        label = d["label"].as_str().unwrap_or_default(),
                        purpose = brief(d["context"]["purpose"].as_str().unwrap_or_default())
                    )))
                })
                .collect();
            out.insert("nodes".into(), nodes.into());
            out.insert("value".into(), text("nodesGuide").into());
        }
        if ["entity", "value"].contains(&kind) {
            out.insert("embed".into(), text("embedGuide").into());
        }
        if kind == "ghost" {
            out.insert("value".into(), text("ghostGuide").into());
        }
        if let Some(example) = catalog()["kindExamples"].get(kind) {
            out.insert("example".into(), example.clone());
        }
        // Where the rest is: a node's flow, the board of a goal and a task. The keys every kind
        // takes are the card tools', which the list says.
        let more = if NODE_KINDS.contains(&kind) {
            Some(text!("mcp.guide.flowSaidThere"))
        } else if ["goal", "task"].contains(&kind) {
            Some(text!("mcp.guide.boardMore"))
        } else {
            None
        };
        if let Some(more) = more {
            out.insert("more".into(), more.into());
        }
        Value::Object(out)
    }

    /// The forms written to a target inside a card of `kind` (a field, a row, an enum's value, a
    /// step), each by its label.
    fn kind_forms(&mut self, kind: &str, out: &mut Map<String, Value>) {
        for (label, kinds) in KIND_FORMS {
            if kinds.contains(&kind)
                && let Some(form) = self.both(label, &|_| true, label, false)
            {
                out.insert(label.into(), form);
            }
        }
    }

    /// A key: how it is written (in words, for a few), its fullest shape as a table with where it
    /// is taken, and its other shapes with where.
    fn key(&mut self, key: &str) -> Value {
        let mut out = Map::new();
        out.insert("key".into(), key.into());
        let mut bare = false;
        if let Some((_, said, whole)) = KEY_TEXTS.iter().find(|(k, _, _)| *k == key) {
            out.insert("value".into(), text(said).into());
            bare = *whole;
        }
        let forms = argument_forms().remove(key).unwrap_or_default();
        for (i, (spec, at)) in forms.iter().enumerate() {
            let here = match i {
                0 => "shape".to_string(),
                _ => format!("otherShapes.{}.shape", i - 1),
            };
            let shape = self.shape(spec, &here, key, bare);
            let mut one = Map::new();
            one.insert("in".into(), json!(at));
            one.insert("shape".into(), shape);
            if i == 0 {
                out.extend(one);
            } else {
                let others = out
                    .entry("otherShapes")
                    .or_insert_with(|| Value::Array(Vec::new()));
                if let Value::Array(list) = others {
                    list.push(Value::Object(one));
                }
            }
        }
        Value::Object(out)
    }

    /// One shape of a key: a line for itself, and a table of its keys when it has some (their
    /// types only when `bare`).
    fn shape(&mut self, spec: &Value, at: &str, key: &str, bare: bool) -> Value {
        let inner = inner_of(spec);
        let head = line(spec, false, None, inner.is_some()).said(bare);
        match inner {
            Some(inner) => {
                let mut table = Map::new();
                table.insert(String::new(), head.into());
                table.extend(self.lines_of(inner, at, key, bare));
                Value::Object(table)
            }
            None => head.into(),
        }
    }

    /// A form of `card_create` and `card_update` with the keys `keep` keeps, each tool's as a
    /// table, `card_update`'s lines that say what `card_create`'s did left to it. `bare` says each
    /// key's type only (keys every card takes, said in full by the card tools' topics).
    fn both(
        &mut self,
        label: &str,
        keep: &dyn Fn(&str) -> bool,
        at: &str,
        bare: bool,
    ) -> Option<Value> {
        let mut out = Map::new();
        for name in ["card_create", "card_update"] {
            let Some(spec) = form_of(name, label) else {
                continue;
            };
            let mut spec = keeping(spec, keep);
            if spec["properties"].as_object().is_none_or(Map::is_empty) {
                continue;
            }
            for (key, said) in &self.kind_texts {
                if let Some(Value::Object(property)) = spec["properties"].get_mut(key) {
                    property.insert("description".into(), said.clone().into());
                }
            }
            let table = self.lines_of(&spec, &format!("{at}.{name}"), "", bare);
            out.insert(name.into(), table.into());
        }
        (!out.is_empty()).then_some(Value::Object(out))
    }

    /// A shape's keys as a table: each key (one inside another, or inside an array's elements, in
    /// an object of its own) with its type, whether it is required and what it is; a key with a
    /// topic of its own (`POINTED`; not the one this table is for) in its first sentence and the
    /// topic. A key said before in this answer in the same words is not said again: the keys are
    /// named in `same` with where they were said.
    fn table(&mut self, spec: &Value, at: &str, about: &str) -> Map<String, Value> {
        self.lines_of(spec, at, about, false)
    }

    fn lines_of(&mut self, spec: &Value, at: &str, about: &str, bare: bool) -> Map<String, Value> {
        let mut lines = Vec::new();
        rows(spec, "", about, &mut lines);
        let required = text!("mcp.guide.required");
        let mut out = Vec::new();
        // Where each left line was said, in the order they come, with their keys.
        let mut same: Vec<(String, Vec<String>)> = Vec::new();
        let mut left: Vec<String> = Vec::new();
        for (key, line) in lines {
            if left.iter().any(|l| key.starts_with(&format!("{l}."))) {
                continue;
            }
            let (parent, last) = match key.rfind('.') {
                Some(i) => (&key[..i], &key[i + 1..]),
                None => ("", key.as_str()),
            };
            // A key is the same as one said before when it is said in the same words: its type
            // (a form may take null where another does not) and whether it is required are the
            // form's own. Said by its type only, its type is its words.
            let one = (
                last.to_string(),
                match (&line.description, bare) {
                    (Some(description), false) => description.clone(),
                    _ => line.said(true).replacen(&required, "", 1),
                },
            );
            if let Some(was) = self.lines.get(&one) {
                match same.iter_mut().find(|(w, _)| w == was) {
                    Some((_, keys)) => keys.push(key.clone()),
                    None => same.push((was.clone(), vec![key.clone()])),
                }
                left.push(key);
                continue;
            }
            let here = if parent.is_empty() {
                format!("{}{at}", self.topic)
            } else {
                format!("{}{at}.{parent}", self.topic)
            };
            self.lines.insert(one, here);
            out.push((key, line.said(bare)));
        }
        if !same.is_empty() {
            let said: Vec<String> = same
                .iter()
                .map(|(was, keys)| self.same(was, keys))
                .collect();
            out.push(("same".into(), said.join(&text!("mcp.guide.sameJoin"))));
        }
        nested(out)
    }

    /// How a table says the keys it left to where they were said: all of that place's keys, all
    /// but a few, or by name.
    fn same(&self, was: &str, keys: &[String]) -> String {
        let last = |key: &str| key.rsplit('.').next().unwrap_or(key).to_string();
        let ours: HashSet<String> = keys.iter().map(|k| last(k)).collect();
        let mut theirs: Vec<String> = self
            .lines
            .iter()
            .filter(|(_, at)| at.as_str() == was)
            .map(|((name, _), _)| name.clone())
            .collect();
        theirs.sort();
        theirs.dedup();
        let lacking: Vec<String> = theirs
            .iter()
            .filter(|t| !ours.contains(*t))
            .cloned()
            .collect();
        if ours.len() > 1 && ours.iter().all(|o| theirs.contains(o)) {
            if lacking.is_empty() {
                return text!("mcp.guide.sameAll", was);
            }
            if lacking.len() < ours.len().min(4) {
                return text!(
                    "mcp.guide.sameBut",
                    was,
                    lacking = crate::text::list(&lacking)
                );
            }
        }
        text!("mcp.guide.sameAs", keys = crate::text::list(keys), was)
    }
}

/// One key's line of a table: its type and whether it is required (`head`), and what it is.
struct Line {
    head: String,
    description: Option<String>,
    /// The topic that says the rest of it.
    pointed: Option<&'static str>,
}

impl Line {
    fn said(&self, bare: bool) -> String {
        match (&self.description, bare) {
            (Some(description), false) => {
                text!("mcp.guide.line", kind = self.head, description)
            }
            _ => match self.pointed {
                Some(topic) => format!("{}{}", self.head, text!("mcp.guide.see", topic)),
                None => self.head.clone(),
            },
        }
    }
}

/// A table's lines by their keys' paths (`a.b`) as objects inside objects: a key with keys inside
/// it says its own line under `""`.
fn nested(lines: Vec<(String, String)>) -> Map<String, Value> {
    let mut out = Map::new();
    for (key, said) in lines {
        let mut names: Vec<&str> = key.split('.').collect();
        let last = names.pop().unwrap_or_default();
        let mut at = &mut out;
        for name in names {
            let entry = at.entry(name).or_insert_with(|| Value::Object(Map::new()));
            if let Value::String(head) = entry {
                let mut object = Map::new();
                object.insert(String::new(), Value::String(std::mem::take(head)));
                *entry = Value::Object(object);
            }
            let Value::Object(inner) = entry else {
                unreachable!("a key with keys inside it is an object")
            };
            at = inner;
        }
        at.insert(last.to_string(), said.into());
    }
    out
}

/// The fields a built-in kind's definition has that the card tools' forms do not say (a goal's
/// status, a task's done): each by its name, with its type, its choices, and what people call it.
fn definition_fields(definition: &Value) -> Option<Value> {
    let said: HashSet<&str> = ["card_create", "card_update"]
        .iter()
        .filter_map(|name| form_of(name, "card"))
        .filter_map(|spec| spec["properties"].as_object())
        .flat_map(|keys| keys.keys().map(String::as_str))
        .collect();
    let fields: Map<String, Value> = definition["fields"]
        .as_array()?
        .iter()
        .filter_map(|field| {
            let name = field["name"].as_str()?;
            if said.contains(name) {
                return None;
            }
            let mut head = field["fieldtype"].as_str().unwrap_or("text").to_string();
            if let Some(Value::Array(options)) = field.get("options") {
                let options: Vec<&str> = options.iter().filter_map(Value::as_str).collect();
                head = format!("{head} {}", options.join("|"));
            }
            let label = field["label"].as_str().unwrap_or(name);
            let description = match field["description"].as_str() {
                Some(d) => text!("mcp.guide.labelled", label, description = d),
                None => label.to_string(),
            };
            Some((
                name.to_string(),
                text!("mcp.guide.line", kind = head, description).into(),
            ))
        })
        .collect();
    (!fields.is_empty()).then_some(Value::Object(fields))
}

/// A shape with only the keys `keep` keeps (and only those of them it requires).
fn keeping(spec: &Value, keep: &dyn Fn(&str) -> bool) -> Value {
    let mut spec = spec.clone();
    if let Some(Value::Object(properties)) = spec.get_mut("properties") {
        properties.retain(|k, _| keep(k));
    }
    if let Some(Value::Array(required)) = spec.get_mut("required") {
        required.retain(|r| r.as_str().is_some_and(keep));
    }
    spec
}

/// A tool's form by its label.
fn form_of(name: &str, label: &str) -> Option<&'static Value> {
    tool(name)?["forms"]
        .as_array()?
        .iter()
        .find(|f| f["label"] == label)
        .map(|f| &f["spec"])
}

/// What `card_create`'s and `card_update`'s topics say of a form a kind's topic says whole.
fn form_pointer(label: &str) -> String {
    if label == "node" {
        return text!("mcp.guide.formNode", kinds = crate::text::list(&NODE_KINDS));
    }
    let kinds = kinds_of(&KIND_FORMS, label).unwrap_or(&[]);
    text!("mcp.guide.formOf", label, kinds = crate::text::list(kinds))
}

/// The object inside a shape whose keys a table says in lines of their own: its own, or its
/// elements' — when one of them says more than its type (`{x, y}` is said whole in its line).
fn inner_of(spec: &Value) -> Option<&Value> {
    let says_more = |object: &Value| {
        object
            .get("properties")
            .and_then(Value::as_object)
            .is_some_and(|keys| {
                keys.values().any(|key| {
                    ["description", "enum", "properties", "items"]
                        .iter()
                        .any(|k| key.get(*k).is_some())
                })
            })
    };
    if says_more(spec) {
        return Some(spec);
    }
    spec.get("items").filter(|items| says_more(items))
}

/// The lines of a table: each key of `spec` under `prefix`, the keys inside it after it.
fn rows(spec: &Value, prefix: &str, about: &str, out: &mut Vec<(String, Line)>) {
    let Some(Value::Object(properties)) = spec.get("properties") else {
        return;
    };
    let required: Vec<&str> = spec["required"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    for (key, sub) in properties {
        let path = if prefix.is_empty() {
            key.clone()
        } else {
            format!("{prefix}.{key}")
        };
        let pointed = POINTED
            .iter()
            .find(|(k, topic)| k == key && *topic != about)
            .map(|(_, topic)| *topic);
        let inner = inner_of(sub).filter(|_| pointed.is_none());
        out.push((
            path.clone(),
            line(
                sub,
                required.contains(&key.as_str()),
                pointed,
                inner.is_some(),
            ),
        ));
        if let Some(inner) = inner {
            rows(inner, &path, about, out);
        }
    }
}

/// One key's line: its type (`string`, `a|b` for a choice, `{x, y}` for an object, `[…]` for a
/// list), whether it is required, and what it is — in its first sentence and the topic that says
/// the rest, when one does.
fn line(spec: &Value, required: bool, pointed: Option<&'static str>, keys_follow: bool) -> Line {
    let mut head = type_of(spec, keys_follow);
    if required {
        head += &text!("mcp.guide.required");
    }
    // An array's elements said only in words (no keys of their own): their words after its own.
    let elements = spec
        .get("items")
        .filter(|items| items.get("properties").is_none())
        .and_then(|items| items.get("description"))
        .and_then(Value::as_str);
    let description = spec
        .get("description")
        .and_then(Value::as_str)
        .map(|description| match elements {
            Some(elements) => text!("mcp.guide.elements", description, elements),
            None => description.to_string(),
        })
        .map(|description| match pointed {
            Some(topic) => {
                let first = brief(&description);
                let first = first.trim_end_matches(['.', '。']);
                format!("{first}{}", text!("mcp.guide.see", topic))
            }
            None => description,
        });
    Line {
        head,
        description,
        pointed,
    }
}

/// A shape's type as a table says it; an object's keys as `…` when the lines after it say them.
fn type_of(spec: &Value, keys_follow: bool) -> String {
    if let Some(Value::Array(choices)) = spec.get("enum") {
        return choices
            .iter()
            .map(|c| match c {
                Value::String(s) => s.clone(),
                other => ojson::compact(other),
            })
            .collect::<Vec<_>>()
            .join("|");
    }
    let one = |kind: &str| -> String {
        match kind {
            "object" => match spec.get("properties").and_then(Value::as_object) {
                Some(_) if keys_follow => "{…}".into(),
                Some(p) if !p.is_empty() => {
                    format!("{{{}}}", p.keys().cloned().collect::<Vec<_>>().join(", "))
                }
                _ => "object".into(),
            },
            "array" => match spec.get("items") {
                Some(items) if !items.as_object().is_none_or(Map::is_empty) => {
                    format!("[{}]", type_of(items, keys_follow))
                }
                _ => "array".into(),
            },
            other => other.into(),
        }
    };
    match spec.get("type") {
        Some(Value::String(kind)) => one(kind),
        Some(Value::Array(kinds)) => kinds
            .iter()
            .filter_map(Value::as_str)
            .map(one)
            .collect::<Vec<_>>()
            .join("|"),
        _ if spec.get("properties").is_some() => one("object"),
        _ => "any".into(),
    }
}

// MARK: - Said once

/// How long a text is before it is said again by where it was.
const SAID_AGAIN: usize = 80;

/// An answer's texts, each said once: a text given before (as long as `SAID_AGAIN` or more) is said
/// by where it was (`Same as "<where>"`), `<where>` its keys joined by `.`. An example is left as
/// it is.
fn said_once(answer: &mut Value) {
    fn walk(value: &mut Value, at: &str, said: &mut HashMap<String, String>) {
        match value {
            Value::String(text) if text.chars().count() >= SAID_AGAIN => {
                match said.get(text.as_str()) {
                    Some(was) => *text = text!("mcp.same.shape", was),
                    None => {
                        said.insert(text.clone(), at.to_string());
                    }
                }
            }
            Value::Array(items) => {
                for (i, item) in items.iter_mut().enumerate() {
                    walk(item, &format!("{at}[{i}]"), said);
                }
            }
            Value::Object(object) => {
                for (key, inner) in object.iter_mut() {
                    if key == "example" {
                        continue;
                    }
                    let here = if at.is_empty() {
                        key.clone()
                    } else {
                        format!("{at}.{key}")
                    };
                    walk(inner, &here, said);
                }
            }
            _ => {}
        }
    }
    walk(answer, "", &mut HashMap::new());
}
