//! The MCP tools held by a project: the tools of `src/mcp/tools.json` made into cards — an instance of the
//! entity `Tool` for each, the form of its arguments and of each of its `value`s beside it, an enum where
//! one is named, the project's own lines `schemaOf` and `form` (`export: {as: schema}`) — and written out
//! again by an export of the format `json-data` as what they were.
//!
//! The same calls that make the cards here make them in the project (`SPECBOARD_TOOLS_PLAN=<file>`
//! writes them out as JSON for a tool that plays them there; `SPECBOARD_TOOLS_ROOT` is the section they
//! go under, `Free/Definition` unless it is said).
//!
//! Run as it is (CI), it goes round with a few of the tools that between them have every shape it
//! makes cards of (`SOME`); with all of them by `SPECBOARD_TOOLS_ALL=1`, and always when it writes
//! the calls or what was written out (`SPECBOARD_TOOLS_PLAN`, `SPECBOARD_TOOLS_WRITE`): those are of
//! every tool. `SPECBOARD_TOOLS_ONLY=card_create,card_update` names the ones to go round with.

use crate::common;

use common::{op, script_store};
use serde_json::{Map, Value, json};
use specboard_core::Core;
use std::collections::HashSet;
use std::path::PathBuf;

const PROJECT: &str = "見本";

/// The tools a run goes round with unless it is told otherwise: between them, every shape `plan`
/// makes cards of — a schema with `also` (a string or an object: `card_update`'s `body`; an integer
/// or a string: `mr_read`'s `target`), nested two deep and more, an array of arrays and of objects, an
/// enum at the top, in an array and nested (`card_update`, `app_control`'s `op`, `kind_create`'s
/// `of`), an item said only by its description (`export_create`'s `entries`), a nullable one and a
/// schema with a description of its own (`kind_create`), every scalar (`app_control`'s `area` for a
/// number), several forms, none, and an empty one, and an array of strings that also takes an object
/// (`kind_update`'s `vias`).
const SOME: [&str; 6] = [
    "kind_create",
    "kind_update",
    "card_update",
    "export_create",
    "app_control",
    "mr_read",
];

fn tools_json() -> Vec<Value> {
    let text = std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/mcp/tools.json"),
    )
    .unwrap();
    let all = serde_json::from_str::<Value>(&text)
        .unwrap()
        .as_array()
        .unwrap()
        .clone();
    assert!(all.len() > 40, "tools.json が欠けている: {} 本", all.len());
    let set = |name: &str| std::env::var_os(name).is_some();
    let only = std::env::var("SPECBOARD_TOOLS_ONLY").ok();
    let names: Vec<&str> = match &only {
        // `SPECBOARD_TOOLS_ONLY=card_create,card_update`: a few of them, for working on one.
        Some(only) => only.split(',').map(str::trim).collect(),
        None if std::env::var("SPECBOARD_TOOLS_ALL").as_deref() == Ok("1")
            || set("SPECBOARD_TOOLS_PLAN")
            || set("SPECBOARD_TOOLS_WRITE") =>
        {
            return all;
        }
        None => SOME.to_vec(),
    };
    for name in &names {
        assert!(
            all.iter().any(|t| t["name"] == *name),
            "tools.json に {name} がありません"
        );
    }
    all.into_iter()
        .filter(|t| names.contains(&t["name"].as_str().unwrap_or_default()))
        .collect()
}

/// Where two values differ, by path, each said short.
fn differences(got: &Value, want: &Value, at: &str, out: &mut Vec<String>) {
    let short = |v: &Value| {
        let t = v.to_string();
        if t.chars().count() > 90 {
            format!("{}…", t.chars().take(90).collect::<String>())
        } else {
            t
        }
    };
    match (got, want) {
        (Value::Object(g), Value::Object(w)) => {
            for (k, wv) in w {
                match g.get(k) {
                    Some(gv) => differences(gv, wv, &format!("{at}.{k}"), out),
                    None => out.push(format!("{at}.{k}: 無い（元は {}）", short(wv))),
                }
            }
            for (k, gv) in g {
                if !w.contains_key(k) {
                    out.push(format!("{at}.{k}: 余分（{}）", short(gv)));
                }
            }
        }
        (Value::Array(g), Value::Array(w)) if g.len() == w.len() => {
            for (i, (gv, wv)) in g.iter().zip(w).enumerate() {
                differences(gv, wv, &format!("{at}[{i}]"), out);
            }
        }
        _ if got != want => out.push(format!("{at}: {} ← 元 {}", short(got), short(want))),
        _ => {}
    }
}

fn pascal(name: &str) -> String {
    name.split(['_', '-', '.', ' '])
        .filter(|p| !p.is_empty())
        .map(|p| {
            let mut c = p.chars();
            c.next()
                .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
                .unwrap_or_default()
        })
        .collect()
}

/// A call to make: a tool and its arguments (the project is said by whoever plays it).
type Call = (String, Value);

struct Plan {
    root: String,
    calls: Vec<Call>,
    taken: HashSet<String>,
}

impl Plan {
    fn call(&mut self, tool: &str, arguments: Value) {
        self.calls.push((tool.to_string(), arguments));
    }

    /// A name of a card in a section no other has there.
    fn fresh(&mut self, section: &str, wanted: &str) -> String {
        let mut name = wanted.to_string();
        let mut n = 2;
        while !self.taken.insert(format!("{section}:{name}")) {
            name = format!("{wanted}{n}");
            n += 1;
        }
        name
    }

    /// The fields of a schema's `properties`, as a card writes them: the top ones in the card, the
    /// ones inside an object made one by one after it (`nested`, by path); the enums they name made
    /// apart (their cards) and the lines that embed them (`lines`, by the field's path).
    fn fields(&mut self, section: &str, stem: &str, schema: &Value, at: &str, got: &mut Collected) {
        let required: Vec<&str> = schema["required"]
            .as_array()
            .map(|r| r.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();
        let properties = schema["properties"]
            .as_object()
            .cloned()
            .unwrap_or_default();
        for (name, prop) in &properties {
            self.field(
                section,
                stem,
                name,
                prop,
                required.contains(&name.as_str()),
                at,
                got,
            );
        }
        // An object with required names and no properties (`items: {required: [kind]}`): the names
        // are fields that may be anything, said nothing of (the form says the names alone).
        if properties.is_empty() {
            for name in required {
                let def = json!({"name": name, "type": "json", "required": true});
                got.put(at, name, def);
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn field(
        &mut self,
        section: &str,
        stem: &str,
        name: &str,
        prop: &Value,
        required: bool,
        at: &str,
        got: &mut Collected,
    ) {
        let path = if at.is_empty() {
            name.to_string()
        } else {
            format!("{at}.{name}")
        };
        let mut field = Map::new();
        field.insert("name".into(), name.into());
        if let Some(d) = prop["description"].as_str() {
            field.insert("description".into(), d.into());
        }
        if required {
            field.insert("required".into(), true.into());
        }
        let types: Vec<&str> = match &prop["type"] {
            Value::String(t) => vec![t.as_str()],
            Value::Array(l) => l.iter().filter_map(Value::as_str).collect(),
            _ => vec!["string"],
        };
        if types.contains(&"null") {
            field.insert("nullable".into(), true.into());
        }
        let rest: Vec<&str> = types.iter().copied().filter(|t| *t != "null").collect();
        // An array of its items (`array` alone), or its own type with other types besides.
        // An array whose items are none of the other types is the field's own (`vias`: names, or an
        // object of edits): the others are what it also takes.
        let items_apart = prop["items"]["type"]
            .as_str()
            .is_some_and(|t| !rest.contains(&t));
        let (primary, mut also): (&str, Vec<&str>) = if rest == ["array"] {
            ("array", vec![])
        } else if rest.contains(&"array") && items_apart {
            (
                "array",
                rest.iter().copied().filter(|t| *t != "array").collect(),
            )
        } else if prop["properties"].is_object() {
            (
                "object",
                rest.iter().copied().filter(|t| *t != "object").collect(),
            )
        } else {
            (rest[0], rest[1..].to_vec())
        };
        let element = if primary == "array" {
            field.insert("array".into(), true.into());
            prop["items"].clone()
        } else {
            prop.clone()
        };
        // Of what may be anything — no type; and an array's array, which a field has not — it is `json`.
        let own = if primary == "array" {
            match element["type"].as_str() {
                Some("array") | None => "json",
                Some(t) => t,
            }
        } else if prop["type"].is_null() {
            "json"
        } else {
            primary
        };
        // The array this field also takes: its items are its own type (`also: [array]`).
        if primary != "array" && also.contains(&"array") {
            assert_eq!(
                prop["items"]["type"].as_str().unwrap_or("string"),
                own,
                "{path}: the items of what takes one or an array are its own type"
            );
        }
        also.dedup();
        if !also.is_empty() {
            field.insert("also".into(), json!(also));
        }
        // What it is.
        let mut children = None;
        if let Some(values) = element["enum"].as_array() {
            let wanted = format!("{stem}{}Enum", pascal(&path.replace('.', "_")));
            let enum_name = self.fresh(section, &wanted);
            self.call(
                "card_create",
                json!({
                    "target": format!("{section}:{enum_name}"),
                    "value": {"kind": "enum", "values": values.iter().filter_map(Value::as_str).map(|v| json!({"name": v})).collect::<Vec<_>>()},
                }),
            );
            field.insert("type".into(), "string".into());
            got.lines
                .push((path.clone(), format!("{section}:{enum_name}")));
        } else if own == "object" {
            if primary == "array"
                && element["description"].is_string()
                && !element["properties"].is_object()
                && !element["required"].is_array()
            {
                // What the elements of an array are, said apart: a form of no fields with the words.
                let wanted = format!("{stem}{}Item", pascal(&path.replace('.', "_")));
                let item = self.fresh(section, &wanted);
                self.call(
                    "card_create",
                    json!({"target": format!("{section}:{item}"), "value": {
                        "kind": "value", "description": element["description"].clone()}}),
                );
                field.insert("type".into(), "string".into());
                got.lines.push((path.clone(), format!("{section}:{item}")));
            } else {
                field.insert("type".into(), "object".into());
                if element["properties"].is_object() || element["required"].is_array() {
                    children = Some(element.clone());
                }
            }
        } else {
            let kind = match own {
                "integer" => "int",
                "number" => "float",
                "boolean" => "bool",
                "json" => "json",
                _ => "string",
            };
            field.insert("type".into(), kind.into());
        }
        got.put(at, name, Value::Object(field));
        if let Some(element) = children {
            self.fields(section, stem, &element, &path, got);
        }
    }

    /// An entity for a schema: its fields, its description; the enums beside it.
    fn form(&mut self, section: &str, name: &str, label: &str, schema: &Value) -> String {
        let mut got = Collected::default();
        self.fields(section, name, schema, "", &mut got);
        let card = format!("{section}:{name}");
        let mut value = Map::new();
        value.insert("kind".into(), "entity".into());
        value.insert("label".into(), label.into());
        if let Some(d) = schema["description"].as_str() {
            value.insert("description".into(), d.into());
        }
        value.insert("fields".into(), Value::Array(got.top.clone()));
        let line = |(from, to): &(String, String)| json!({"line": "embed", "from": from, "to": to});
        let top_lines: Vec<Value> = got
            .lines
            .iter()
            .filter(|(f, _)| !f.contains('.'))
            .map(line)
            .collect();
        if !top_lines.is_empty() {
            value.insert("connections".into(), Value::Array(top_lines));
        }
        self.call(
            "card_create",
            json!({"target": card.clone(), "value": Value::Object(value)}),
        );
        for (path, def) in got.nested.clone() {
            let mut def = def.as_object().cloned().unwrap();
            def.remove("name");
            self.call(
                "card_create",
                json!({"target": format!("{card}.{path}"), "value": Value::Object(def)}),
            );
        }
        let nested_lines: Vec<Value> = got
            .lines
            .iter()
            .filter(|(f, _)| f.contains('.'))
            .map(line)
            .collect();
        if !nested_lines.is_empty() {
            self.call(
                "card_update",
                json!({"target": card.clone(), "value": {"connections": {"add": nested_lines}}}),
            );
        }
        card
    }
}

/// What a form's fields came to: the top ones, the ones inside objects (by path), the embed lines.
#[derive(Default)]
struct Collected {
    top: Vec<Value>,
    nested: Vec<(String, Value)>,
    lines: Vec<(String, String)>,
}

impl Collected {
    fn put(&mut self, at: &str, name: &str, def: Value) {
        if at.is_empty() {
            self.top.push(def);
        } else {
            self.nested.push((format!("{at}.{name}"), def));
        }
    }
}

/// The calls that make a project hold these tools.
fn plan(tools: &[Value], root: &str) -> Vec<Call> {
    let mut p = Plan {
        root: root.to_string(),
        calls: Vec::new(),
        taken: HashSet::new(),
    };
    let mcp = format!("{}/MCP", p.root);
    // The project's own lines: from an instance's field or element to a form, written as its JSON Schema.
    p.call(
        "kind_create",
        json!({"target": "schemaOf", "value": {"of": "line", "label": "形の JSON Schema",
            "description": "データ（instance）の欄の値が、どの形（entity・value）を JSON Schema にしたものか。MCP のツール定義の引数の形（Tool の欄 inputSchema）。書き出し（形式 json-data）が、その欄を形の JSON Schema に置き換える",
            "base": "link", "look": {"color": "teal", "dash": "dashed", "begin": "none", "end": "arrow"},
            "from": [{"kinds": ["instance"], "at": "row"}], "to": {"kinds": ["entity", "value"], "at": ["card"]},
            "max": 1, "visible": false, "export": {"as": "schema"}}}),
    );
    p.call(
        "kind_create",
        json!({"target": "form", "value": {"of": "line", "label": "value の形",
            "description": "データ（instance）の並びの要素が、どの形（entity・value）か。MCP のツール定義の forms の要素（label と、value の形）。書き出し（形式 json-data）が、その要素に形の JSON Schema を key（spec）で足す",
            "base": "link", "look": {"color": "teal", "dash": "dashed", "begin": "none", "end": "arrow"},
            "from": [{"kinds": ["instance"], "at": "row"}], "to": {"kinds": ["entity", "value"], "at": ["card"]},
            "max": null, "visible": false, "export": {"as": "schema", "key": "spec"}}}),
    );
    p.call("section_create", json!({"target": p.root.clone()}));
    p.call("section_create", json!({"target": mcp.clone()}));
    let hints: Vec<Value> = [
        "readOnlyHint",
        "destructiveHint",
        "idempotentHint",
        "openWorldHint",
    ]
    .iter()
    .map(|n| json!({"name": n, "type": "bool", "required": true}))
    .collect();
    p.call(
        "card_create",
        json!({"target": format!("{mcp}:ToolAnnotations"), "value": {"kind": "value", "label": "ツールの注記",
            "fields": hints}}),
    );
    p.call(
        "card_create",
        json!({"target": format!("{mcp}:ToolForm"), "value": {"kind": "value", "label": "value の形の名前",
            "fields": [{"name": "label", "type": "string", "required": true, "description": "guide が value の形を選ぶときの名前（card・field など）"}]}}),
    );
    p.call(
        "card_create",
        json!({"target": format!("{mcp}:Tool"), "value": {"kind": "entity", "label": "MCP のツール",
            "description": "MCP のツール 1 本。引数の形は欄 inputSchema から線 schemaOf で、value の形は forms の要素から線 form で引く（書き出しの形式 json-data が辿る）",
            "fields": [
                {"name": "name", "type": "string", "required": true, "primaryKey": true},
                {"name": "title", "type": "string"},
                {"name": "description", "type": "text"},
                {"name": "inputSchema", "type": "object", "description": "引数の形（線 schemaOf で entity へ）"},
                {"name": "annotations", "type": "string", "required": true},
                {"name": "forms", "type": "string", "array": true, "description": "value の形（要素ごとに線 form で entity・value へ）"}
            ],
            "connections": [
                {"line": "embed", "from": "annotations", "to": format!("{mcp}:ToolAnnotations")},
                {"line": "embed", "from": "forms", "to": format!("{mcp}:ToolForm")}
            ]}}),
    );
    let mut groups: Vec<String> = Vec::new();
    for tool in tools {
        let name = tool["name"].as_str().unwrap();
        let group = pascal(name.split('_').next().unwrap());
        let section = format!("{mcp}/{group}");
        if !groups.contains(&group) {
            groups.push(group.clone());
            p.call("section_create", json!({"target": section.clone()}));
        }
        let stem = pascal(name);
        let mut lines = vec![];
        let args = p.form(
            &section,
            &format!("{stem}Args"),
            &format!("{} の引数", tool["title"].as_str().unwrap_or(name)),
            &tool["inputSchema"],
        );
        lines.push(json!({"line": "schemaOf", "from": "inputSchema", "to": args}));
        let mut forms = Vec::new();
        for (i, form) in tool["forms"].as_array().into_iter().flatten().enumerate() {
            let label = form["label"].as_str().unwrap();
            let card = p.form(
                &section,
                &format!("{stem}Form{}", i + 1),
                label,
                &form["spec"],
            );
            forms.push(json!({"label": label}));
            let quoted = format!("\"{}\"", label.replace('\\', "\\\\").replace('"', "\\\""));
            lines.push(
                json!({"line": "form", "from": format!("forms[label={quoted}]"), "to": card}),
            );
        }
        let mut data = Map::new();
        data.insert("name".into(), name.into());
        data.insert("title".into(), tool["title"].clone());
        data.insert("description".into(), tool["description"].clone());
        data.insert("annotations".into(), tool["annotations"].clone());
        if !forms.is_empty() {
            data.insert("forms".into(), Value::Array(forms));
        }
        p.call(
            "card_create",
            json!({"target": format!("{section}:{name}"), "value": {
                "kind": "instance", "of": format!("{mcp}:Tool"), "label": tool["title"].clone(),
                "data": Value::Object(data), "connections": lines}}),
        );
    }
    p.call(
        "export_create",
        json!({"target": "tools", "value": {"entries": [
            {"from": mcp, "to": "tools.json", "format": "json-data"}]}}),
    );
    p.calls
}

/// The tools a batch takes (a transaction for many of them at once).
fn batchable(tool: &str) -> bool {
    tool.starts_with("section_") || tool.starts_with("card_")
}

fn play(core: &Core, project: &str, calls: &[Call]) {
    let mut at = 0;
    while at < calls.len() {
        let (tool, arguments) = &calls[at];
        if batchable(tool) {
            let end = (at..calls.len().min(at + 100))
                .take_while(|i| batchable(&calls[*i].0))
                .last()
                .unwrap()
                + 1;
            let steps: Vec<Value> = calls[at..end]
                .iter()
                .map(|(t, a)| json!({"tool": t, "arguments": a}))
                .collect();
            let reply = op(
                core,
                json!({"op": "tool", "name": "batch", "arguments": {
                    "project": project, "execute": true, "value": {"steps": steps}}}),
            );
            assert!(
                reply.get("error").is_none(),
                "batch from {at}: {}",
                reply["error"]
            );
            at = end;
        } else {
            let mut arguments = arguments.clone();
            arguments["project"] = project.into();
            let reply = op(
                core,
                json!({"op": "tool", "name": tool, "arguments": arguments}),
            );
            assert!(
                reply.get("error").is_none(),
                "{tool} {arguments}: {}",
                reply["error"]
            );
            at += 1;
        }
    }
}

#[test]
fn the_tools_a_project_holds_are_written_out_as_they_were() {
    let tools = tools_json();
    let root = std::env::var("SPECBOARD_TOOLS_ROOT").unwrap_or_else(|_| "Free/Definition".into());
    let calls = plan(&tools, &root);
    if let Ok(path) = std::env::var("SPECBOARD_TOOLS_PLAN") {
        let list: Vec<Value> = calls
            .iter()
            .map(|(t, a)| json!({"tool": t, "arguments": a}))
            .collect();
        std::fs::write(path, serde_json::to_string_pretty(&list).unwrap()).unwrap();
    }
    let store = script_store();
    let core = Core::open(&store).unwrap();
    let id = {
        let conn = rusqlite::Connection::open(&store).unwrap();
        conn.query_row("SELECT id FROM projects WHERE name = ?", [PROJECT], |r| {
            r.get::<_, String>(0)
        })
        .unwrap()
    };
    play(&core, &id, &calls);
    // Written into a folder of its own.
    let folder = common::place("tools");
    std::fs::create_dir_all(&folder).unwrap();
    let written = op(
        &core,
        json!({"op": "tool", "name": "export_execute", "arguments": {
            "project": id, "targetdir": folder.to_str().unwrap(), "target": "tools"}}),
    );
    assert!(written.get("error").is_none(), "{written}");
    let text = std::fs::read_to_string(folder.join("tools.json")).unwrap();
    // `SPECBOARD_TOOLS_WRITE=<file>`: what is written, kept (it is `src/mcp/tools.json` once the project
    // holds the tools).
    if let Ok(path) = std::env::var("SPECBOARD_TOOLS_WRITE") {
        std::fs::write(path, &text).unwrap();
    }
    let got: Value = serde_json::from_str(&text).unwrap();
    let (got_tools, want_tools) = (got.as_array().unwrap(), &tools);
    assert_eq!(got_tools.len(), want_tools.len());
    let mut found = Vec::new();
    for (g, w) in got_tools.iter().zip(want_tools) {
        assert_eq!(
            g["name"], w["name"],
            "the tools are in the order they were made in"
        );
        differences(g, w, w["name"].as_str().unwrap_or_default(), &mut found);
    }
    assert!(
        found.is_empty(),
        "{} 個所が元と違います:\n{}",
        found.len(),
        found
            .iter()
            .take(40)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
    // Written again to the same folder: the same text.
    let again = op(
        &core,
        json!({"op": "tool", "name": "export_execute", "arguments": {
            "project": id, "targetdir": folder.to_str().unwrap(), "target": "tools"}}),
    );
    assert!(again.get("error").is_none(), "{again}");
    assert_eq!(
        std::fs::read_to_string(folder.join("tools.json")).unwrap(),
        text
    );
}
