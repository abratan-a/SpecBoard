//! MCP's words, short and plain: the list says each tool's `value` keys, the instructions say paths
//! and what the arguments mean, and `guide` answers one topic at a time within its room — a tool's
//! purpose, value, answer, example and refusals; a kind's own keys with an example; how one key or
//! mechanism is written — each thing said once and pointed at after. A document read without `as`
//! says where its text is; a card the same call could not make is said to be why a card pointing at
//! it is refused; the entity's guide says how a field that embeds is written.

use crate::common;

use common::{call, ok, refused, script_store};
use serde_json::{Value, json};
use specboard_core::{Core, mcp};
use std::collections::HashMap;

const PROJECT: &str = "見本";

fn core() -> Core {
    Core::open(script_store()).unwrap()
}

fn guide(about: &str) -> Value {
    match mcp::guide(Some(about)) {
        Ok(mcp::Output::Json(said)) => said,
        _ => panic!("{about} の guide がありません"),
    }
}

/// How many characters an answer is as MCP sends it: packed JSON, or the text.
fn sent(output: &mcp::Output) -> usize {
    match output {
        mcp::Output::Json(value) => serde_json::to_string(value).unwrap().chars().count(),
        other => other.text().chars().count(),
    }
}

/// The topics the guide answers, by family, as its list without `about` gives them.
fn topics() -> Vec<(String, String)> {
    let Ok(mcp::Output::Json(index)) = mcp::guide(None) else {
        panic!("guide の一覧がありません");
    };
    let mut out = Vec::new();
    for family in ["tools", "kinds", "keys", "guides"] {
        for name in index[family].as_object().unwrap().keys() {
            out.push((family.to_string(), name.clone()));
        }
    }
    out
}

/// The room of each topic, in characters as sent, the same in every language: an agent reads one
/// topic for one question, so a topic holds what that question needs and points at the rest.
fn room(family: &str, name: &str) -> usize {
    match (family, name) {
        // The reads of a card and of a section say a form for each way they are read, a map as text
        // and as a picture among them, a ui section's mock with its parts' faces, and the functions
        // and types in a file, the implementation lines whose code changed, and what a goal or task
        // needs gathered in one read, and a box drawn apart from the size written for it.
        ("tools", "card_read") => 6_100,
        // A working directory's section says how and from where it is read, and is set too (how its
        // files map to languages among it).
        ("tools", "section_read") => 4_000,
        ("tools", "section_update") => 3_500,
        // The card tools write every kind's keys, a body read from a file (`bodyFrom`) among them,
        // and a size of a width alone or of both.
        ("tools", "card_create" | "card_update") => 4_300,
        // The list narrows by every kind of card, and by a list of them, lists the functions and
        // types in files by what they are and their names, can leave the bodies out, and finds
        // what is not linked to code and whose code changed.
        ("tools", "card_list") => 4_250,
        // The batch says which steps may point at which, and takes its steps from a file.
        ("tools", "batch") => 3_650,
        // A tree is read four ways, and one commit of its history and a branch's changes by what
        // each unit was before and after.
        ("tools", "branch_read") => 4_800,
        // The check names every group, the implementation whose code changed among them.
        ("tools", "inspection_list") => 3_100,
        ("tools", _) => 3_000,
        ("kinds", "ghost") => 3_500,
        // The flow's nodes and how they are joined, dispatched to and waited at, and made in a batch.
        ("kinds", "flow") => 4_100,
        // The formatdoc's rows, a text row's body read from a file among them, and its size.
        ("kinds", "formatdoc") => 2_700,
        ("kinds", _) => 2_500,
        ("keys", _) => 2_000,
        // The branches' guide says how one commit is read and undone alone too.
        ("guides", "branch") => 3_700,
        ("guides", "board" | "ownKinds") => 3_500,
        ("guides", "kinds") => 6_000,
        ("guides", "glossary") => usize::MAX,
        // The way of working names the topics to read at each step of it.
        ("guides", "sdd") => 2_500,
        _ => 2_000,
    }
}

/// The room of each tool's value in the list: the card tools take the most kinds of target.
fn value_room(tool: &str) -> usize {
    match tool {
        "card_create" | "card_update" | "card_read" => 700,
        "card_list" | "batch" | "branch_merge" | "section_update" => 300,
        _ => 150,
    }
}

// MARK: - The list and the instructions

/// The list says what value is written with, each tool's whole and short, and the arguments every
/// tool means the same by only by their names and types.
#[test]
fn the_list_says_each_tools_value_within_its_room() {
    for lang in specboard_core::text::LANGUAGES {
        let _lang = specboard_core::text::bind(Some(lang));
        for tool in mcp::listed() {
            let name = tool["name"].as_str().unwrap();
            let properties = &tool["inputSchema"]["properties"];
            for key in ["project", "branch", "request"] {
                if let Some(argument) = properties.get(key) {
                    assert!(argument.get("description").is_none(), "{lang} {name} {key}");
                }
            }
            let Some(value) = properties.get("value") else {
                continue;
            };
            let said = value["description"]
                .as_str()
                .unwrap_or_else(|| panic!("{lang} {name}"));
            let chars = said.chars().count();
            assert!(chars <= value_room(name), "{lang} {name}: {chars} {said}");
        }
    }
}

/// The instructions say paths and what the arguments mean, and send no one to read the kinds and
/// the guide first: the list says the keys, and the guide is read when a refusal points there.
#[test]
fn the_instructions_say_paths_and_the_arguments() {
    for lang in specboard_core::text::LANGUAGES {
        let _lang = specboard_core::text::bind(Some(lang));
        let instructions = mcp::catalog()["instructions"].as_str().unwrap();
        let chars = instructions.chars().count();
        assert!(chars <= 3_500, "{lang}: {chars}");
        for argument in [
            "- project:",
            "- target:",
            "- value:",
            "- branch:",
            "- request:",
        ] {
            assert!(instructions.contains(argument), "{lang} {argument}");
        }
        for first in ["最初に読むもの", "What to read first"] {
            assert!(!instructions.contains(first), "{lang}: {first}");
        }
        assert!(instructions.contains("about: <"), "{lang}: {instructions}");
    }
}

// MARK: - Each topic in its room

#[test]
fn every_topic_keeps_to_its_room_in_every_language() {
    let mut over = Vec::new();
    for lang in specboard_core::text::LANGUAGES {
        let _lang = specboard_core::text::bind(Some(lang));
        for (family, name) in topics() {
            let said = mcp::guide(Some(&name)).unwrap_or_else(|e| panic!("{lang} {name}: {e}"));
            let chars = sent(&said);
            if chars > room(&family, &name) {
                over.push(format!("{lang} {family} {name}: {chars}"));
            }
        }
    }
    assert!(over.is_empty(), "{over:#?}");
}

/// The list of topics says how long each is, as it is sent.
#[test]
fn the_topic_list_gives_each_topics_size() {
    let Ok(mcp::Output::Json(index)) = mcp::guide(None) else {
        panic!("guide の一覧がありません");
    };
    assert!(index["about"].is_string());
    for (family, name) in topics() {
        let size = index[&family][&name].as_u64().unwrap() as usize;
        assert_eq!(size, sent(&mcp::guide(Some(&name)).unwrap()), "{name}");
    }
    for kind in [
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
        "goal",
        "task",
        "ghost",
        "begin",
        "process",
        "end",
    ] {
        assert!(index["kinds"].get(kind).is_some(), "{kind}");
    }
    for name in ["connections", "body", "layout"] {
        assert!(index["keys"].get(name).is_some(), "{name}");
    }
    for name in [
        "branch", "board", "kinds", "ownKinds", "rev", "paging", "paths", "sdd", "glossary",
    ] {
        assert!(index["guides"].get(name).is_some(), "{name}");
    }
    let Err(unknown) = mcp::guide(Some("no_such_thing")) else {
        panic!("no_such_thing に答えた");
    };
    assert!(unknown.contains("about を省いた guide"), "{unknown}");
}

/// The way of writing the spec first and implementing it is a topic of its own, which the instructions
/// and the guide's own words name: an agent new to a project reads it before the rest, and it sends
/// each step to the topic that says it.
#[test]
fn the_way_of_working_is_a_topic_the_instructions_name() {
    for lang in specboard_core::text::LANGUAGES {
        let _lang = specboard_core::text::bind(Some(lang));
        let said = mcp::guide(Some("sdd")).unwrap().text();
        for named in [
            "about: board",
            "about: flow",
            "about: connections",
            "as: context",
            "implementedBy",
            "branch_merge",
            "op: finish",
            "targetdir",
            "about: ownKinds",
        ] {
            assert!(said.contains(named), "{lang}: {named}");
        }
        if *lang == "en" {
            assert!(!specboard_core::text::has_japanese(&said), "{said}");
        }
        let instructions = mcp::catalog()["instructions"].as_str().unwrap();
        assert!(instructions.contains("about: sdd"), "{lang}");
        let board = mcp::guide(Some("board")).unwrap().text();
        assert!(
            board.contains("\"context\"") && board.contains("about: sdd"),
            "{lang}"
        );
        // A branch is written out too, so the branches' guide does not send it to a preview alone.
        let branch = mcp::guide(Some("branch")).unwrap().text();
        assert!(branch.contains("[default]"), "{lang}: {branch}");
        for stale in ["main だけで、", "works on main only"] {
            assert!(!branch.contains(stale), "{lang}: {stale}");
        }
    }
}

/// What a topic says twice: a text long enough to be worth pointing at, said again (an example is
/// the arguments of a call, left as they are).
fn said_twice(said: &Value) -> Vec<String> {
    fn walk(value: &Value, at: &str, seen: &mut HashMap<String, String>, twice: &mut Vec<String>) {
        match value {
            Value::String(text) if text.chars().count() >= 80 => {
                if let Some(was) = seen.get(text) {
                    twice.push(format!("{at}: {was} と同じ"));
                }
                seen.insert(text.clone(), at.to_string());
            }
            Value::Object(object) => {
                for (k, v) in object {
                    if k != "example" {
                        walk(v, &format!("{at}/{k}"), seen, twice);
                    }
                }
            }
            Value::Array(items) => {
                for (i, v) in items.iter().enumerate() {
                    walk(v, &format!("{at}/{i}"), seen, twice);
                }
            }
            _ => {}
        }
    }
    let mut twice = Vec::new();
    walk(said, "", &mut HashMap::new(), &mut twice);
    twice
}

#[test]
fn no_topic_says_a_text_twice() {
    for lang in specboard_core::text::LANGUAGES {
        let _lang = specboard_core::text::bind(Some(lang));
        for (_, name) in topics() {
            if let Ok(mcp::Output::Json(said)) = mcp::guide(Some(&name)) {
                let twice = said_twice(&said);
                assert!(twice.is_empty(), "{lang} {name}: {twice:?}");
            }
        }
    }
}

// MARK: - What each topic says

/// A tool's topic: what it is for, its value by form as a table of keys, what it answers, one
/// example and the refusals easy to hit — no schema and no list of the kinds, which are elsewhere.
#[test]
fn a_tool_says_its_purpose_value_answer_example_and_refusals() {
    let Ok(mcp::Output::Json(index)) = mcp::guide(None) else {
        panic!("guide の一覧がありません");
    };
    for name in index["tools"].as_object().unwrap().keys() {
        let said = guide(name);
        assert_eq!(said["tool"], name.as_str());
        assert!(said["purpose"].is_string(), "{name}");
        assert!(said["answer"].is_string(), "{name}");
        assert!(said["example"].is_object(), "{name}");
        assert!(said["refusals"].is_array(), "{name}");
        assert!(said.get("inputSchema").is_none(), "{name}");
        let text = said.to_string();
        assert!(!text.contains("\"properties\""), "{name}: {text}");
    }
    // The keys every kind takes, and where a kind's own are.
    let update = guide("card_update");
    let card = &update["value"]["card"];
    for key in [
        "name",
        "label",
        "tags",
        "connections",
        "section",
        "position",
        "rev",
    ] {
        assert!(card[key].is_string(), "{key}: {card}");
    }
    for key in ["fields", "values", "body", "rows", "steps"] {
        assert!(card.get(key).is_none(), "{key}: {card}");
    }
    assert!(
        update["value"]["field"]
            .as_str()
            .unwrap()
            .contains("entity")
    );
    // A key with a topic of its own is said in a line, and the topic named.
    assert!(
        card["connections"]
            .as_str()
            .unwrap()
            .contains("about: connections")
    );
    assert!(card["rev"].as_str().unwrap().contains("about: rev"));
    // A form's array of entries said in words keeps them (export_create's formats).
    let entries = guide("export_create")["value"]["entries"]
        .as_str()
        .unwrap()
        .to_string();
    for word in ["typescript", "zod", "json-schema", "json-data", "Markdown"] {
        assert!(entries.contains(word), "{word}: {entries}");
    }
    // How a kind is defined is a topic of its own, not inside both tools'.
    for name in ["kind_create", "kind_update"] {
        let said = guide(name);
        assert!(
            said["more"].as_str().unwrap().contains("about: kinds"),
            "{said}"
        );
        assert!(!said.to_string().contains("statemachine"), "{name}");
    }
}

/// A kind's topic: what it is for, the keys only it takes, an example; a node also the keys every
/// node takes, and where its flow is said.
#[test]
fn a_kind_says_its_own_keys_an_example_and_what_it_is_for() {
    for kind in [
        "entity",
        "value",
        "enum",
        "instance",
        "table",
        "flow",
        "note",
        "freedoc",
        "formatdoc",
        "stackdoc",
        "goal",
        "task",
        "ghost",
    ] {
        let said = guide(kind);
        assert!(said["context"]["purpose"].is_string(), "{kind}");
        assert!(
            said["keys"]["card_create"].is_object() || said["keys"]["card_update"].is_object(),
            "{kind}: {said}"
        );
        assert!(said["example"]["value"].is_object(), "{kind}");
    }
    let entity = guide("entity");
    assert_eq!(
        entity["context"]["purpose"],
        "単位で保存・送信される記録（ID を持つ）"
    );
    let fields = &entity["keys"]["card_create"]["fields"];
    for key in ["name", "type", "required", "rules"] {
        assert!(fields[key].is_string(), "{key}: {fields}");
    }
    assert!(fields["rules"].as_str().unwrap().contains("about: rules"));
    assert!(
        entity["field"]["card_update"]["name"].is_string(),
        "{entity}"
    );
    assert!(entity.get("card_create").is_none());
    for node in ["begin", "process", "run", "eval", "wait", "join", "end"] {
        let said = guide(node);
        assert!(
            said["nodeKeys"]["card_create"]["kind"].is_string(),
            "{node}"
        );
        assert!(
            said["more"].as_str().unwrap().contains("about: flow"),
            "{node}"
        );
        assert!(said["example"]["value"].is_object(), "{node}");
    }
    let begin = guide("begin");
    assert!(
        begin["keys"]["card_create"]["trigger"].is_string(),
        "{begin}"
    );
    assert!(begin["keys"]["card_create"].get("steps").is_none());
    // The flow says how it is made with its nodes, with an example that does.
    let flow = guide("flow");
    assert_eq!(flow["value"], mcp::catalog()["nodesGuide"]);
    let example = flow["example"]["value"].as_object().unwrap();
    assert!(example.values().any(|card| card["kind"] == "flow"));
    assert!(example.values().any(|card| card["kind"] == "begin"));
}

/// Several topics read in one call are said once: what one said, another points at.
#[test]
fn several_topics_in_one_call_say_a_thing_once() {
    let none = |_: &str| None;
    let Ok(mcp::Output::Json(both)) =
        mcp::guide_of(&["entity".into(), "value".into(), "rev".into()], &none)
    else {
        panic!("entity と value を 1 回で読めない");
    };
    assert!(both["entity"]["embed"].as_str().unwrap().contains("embed"));
    assert_eq!(both["value"]["embed"], "「entity.embed」と同じ");
    assert!(both["rev"].is_string());
    let alone =
        sent(&mcp::guide(Some("entity")).unwrap()) + sent(&mcp::guide(Some("value")).unwrap());
    assert!(
        serde_json::to_string(&both["entity"]).unwrap().len()
            + serde_json::to_string(&both["value"]).unwrap().len()
            < alone * 3
    );
    let Err(unknown) = mcp::guide_of(&["entity".into(), "no_such_thing".into()], &none) else {
        panic!("no_such_thing に答えた");
    };
    assert!(unknown.contains("no_such_thing"), "{unknown}");
}

/// A line is said once, in the topic of connections: its keys by their types, the words beside
/// them saying the rest.
#[test]
fn the_topic_of_connections_says_a_line_once() {
    let said = guide("connections");
    let lines = said["value"].as_str().unwrap();
    assert!(lines.contains("at（相手の中の着く所"), "{lines}");
    assert_eq!(said["shape"]["add"]["line"], "string、必須");
    assert_eq!(said["shape"]["same"], "「shape.add」のキー");
    assert!(said["shape"]["remove"].is_string(), "{said}");
    assert!(
        said["otherShapes"][0]["in"][0]
            .as_str()
            .unwrap()
            .starts_with("card_create")
    );
}

// MARK: - A document read as it is

#[test]
fn a_document_read_without_as_says_where_its_text_is() {
    let core = core();
    // A formatdoc whose items hold rows: their headings and counts, and where the text is.
    let spec = ok(&core, "card_read", json!({"target": "Docs:Spec"}));
    assert!(spec["rows"].to_string().contains("rowCount"), "{spec}");
    assert!(
        spec["hint"].as_str().unwrap().contains("as: markdown"),
        "{spec}"
    );
    // An item whose rows are its text: nothing left out, nothing said.
    let intro = ok(&core, "card_read", json!({"target": "Docs:Spec.intro"}));
    assert!(intro.get("hint").is_none(), "{intro}");
    // An item with items inside: its own path to read it by, which reads its text.
    ok(
        &core,
        "card_create",
        json!({"target": "Docs:Deep", "value": {"kind": "formatdoc", "rows": [
            {"name": "a", "kind": "item", "heading": "A", "rows": [
                {"name": "b", "kind": "item", "heading": "B", "rows": [
                    {"name": "t", "kind": "text", "body": "中の文"}
                ]}
            ]}
        ]}}),
    );
    let a = ok(&core, "card_read", json!({"target": "Docs:Deep.a"}));
    assert!(
        a["hint"]
            .as_str()
            .unwrap()
            .contains("この項目 Docs:Deep.a を value の as: markdown"),
        "{a}"
    );
    let text = ok(
        &core,
        "card_read",
        json!({"target": "Docs:Deep.a", "value": {"as": "markdown"}}),
    );
    assert!(
        text["markdown"].as_str().unwrap().contains("中の文"),
        "{text}"
    );
    // Its text whole, read with its body: nothing said.
    ok(
        &core,
        "card_create",
        json!({"target": "Docs:Flat", "value": {"kind": "formatdoc", "rows": [
            {"name": "t", "kind": "text", "body": "全部ここ"}
        ]}}),
    );
    let flat = ok(&core, "card_read", json!({"target": "Docs:Flat"}));
    assert!(flat.get("hint").is_none(), "{flat}");
    // A stackdoc: its versions without their bodies, and where they are.
    ok(
        &core,
        "card_create",
        json!({"target": "Docs:Log", "value": {"kind": "stackdoc", "versions": [
            {"name": "v1", "key": "1", "body": "版の本文"}
        ]}}),
    );
    let log = ok(&core, "card_read", json!({"target": "Docs:Log"}));
    assert!(
        log["hint"].as_str().unwrap().contains("as: markdown"),
        "{log}"
    );
    // Read in part, a list: not there.
    let found = ok(
        &core,
        "card_read",
        json!({"target": "Docs:Log.v1", "value": {"find": "本文"}}),
    );
    assert!(found.get("hint").is_none(), "{found}");
    let listed = ok(&core, "card_list", json!({"value": {"section": "Docs"}}));
    assert!(!listed.to_string().contains("\"hint\""), "{listed}");
}

// MARK: - A card the call could not make

#[test]
fn a_card_pointing_at_one_the_call_could_not_make_is_said_so() {
    let core = core();
    let along = "同じ呼び出しで作れなかった";
    // Refused before it is read (a kind there is not), and pointed at by a line.
    let said = refused(
        &core,
        "card_create",
        json!({"value": {
            "Schemas:Job": {"kind": "no_such_kind"},
            "Schemas:Run": {"kind": "entity", "fields": [{"name": "job_id", "type": "uuid"}],
                "connections": [{"line": "points", "from": "job_id", "to": "Schemas:Job"}]}
        }}),
    );
    assert!(
        said.contains(&format!(
            "「Schemas:Job」というカードはありません（{along} Schemas:Job を指すため。Schemas:Job を直せば通る）"
        )),
        "{said}"
    );
    // Refused once it is read (a line to no card): the cards pointing at it, by a name in their
    // section too, are refused for it; the card that is not there is said as it was.
    let said = refused(
        &core,
        "card_create",
        json!({"value": {
            "Schemas:Job": {"kind": "entity", "fields": [{"name": "owner", "type": "uuid"}],
                "connections": [{"line": "points", "from": "owner", "to": "Schemas:Nobody"}]},
            "Schemas:Run": {"kind": "entity", "fields": [{"name": "job_id", "type": "uuid"}],
                "connections": [{"line": "points", "from": "job_id", "to": "Job"}]}
        }}),
    );
    assert!(
        said.contains("「Schemas:Nobody」というカードはありません"),
        "{said}"
    );
    assert!(!said.contains("Nobody を指すため"), "{said}");
    assert!(
        said.contains(&format!(
            "「Job」というカードはありません（{along} Schemas:Job を指すため"
        )),
        "{said}"
    );
    // A flow's node, by its name beside it.
    let said = refused(
        &core,
        "card_create",
        json!({"value": {
            "Charts:Pay": {"kind": "flow"},
            "Charts:Pay.start": {"kind": "begin", "trigger": "manual",
                "connections": [{"line": "next", "to": "check"}]},
            "Charts:Pay.check": {"kind": "no_such_kind"}
        }}),
    );
    assert!(
        said.contains(&format!(
            "「check」というカードはありません（{along} Charts:Pay.check を指すため"
        )),
        "{said}"
    );
    // Nothing was written.
    let (failed, _) = call(
        &core,
        "card_read",
        json!({"project": PROJECT, "target": "Schemas:Run"}),
    );
    assert!(failed);
}

// MARK: - A field that embeds

fn field<'v>(card: &'v Value, name: &str) -> &'v Value {
    card["fields"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["name"] == name)
        .unwrap_or_else(|| panic!("{name} がありません: {card}"))
}

/// What the entity's guide says of a field that embeds is what a write does.
#[test]
fn the_guide_says_how_a_field_that_embeds_is_written() {
    for kind in ["entity", "value"] {
        let embed = guide(kind)["embed"].as_str().unwrap().to_string();
        for word in [
            "type は string など object 以外",
            "type: ref",
            "出元",
            "string に戻ります",
        ] {
            assert!(embed.contains(word), "{kind}: {word}: {embed}");
        }
    }
    assert!(guide("store").get("embed").is_none());
    let core = core();
    // An object's field: refused, how to put it right said.
    let said = refused(
        &core,
        "card_create",
        json!({"target": "Schemas:Holder", "value": {"kind": "entity",
            "fields": [{"name": "address", "type": "object"}],
            "connections": [{"line": "embed", "from": "address", "to": "Schemas:Customer"}]}}),
    );
    assert!(
        said.contains("中のフィールドを消して型を外してから"),
        "{said}"
    );
    // A field it does not have: refused.
    let said = refused(
        &core,
        "card_create",
        json!({"target": "Schemas:Holder", "value": {"kind": "entity",
            "connections": [{"line": "embed", "from": "address", "to": "Schemas:Customer"}]}}),
    );
    assert!(said.contains("出元の「address」"), "{said}");
    // json or string: written, read as ref.
    for (name, written) in [("JsonHolder", "json"), ("TextHolder", "string")] {
        ok(
            &core,
            "card_create",
            json!({"target": format!("Schemas:{name}"), "value": {"kind": "entity",
                "fields": [{"name": "address", "type": written}],
                "connections": [{"line": "embed", "from": "address", "to": "Schemas:Customer"}]}}),
        );
        let read = ok(
            &core,
            "card_read",
            json!({"target": format!("Schemas:{name}")}),
        );
        assert_eq!(field(&read, "address")["type"], "ref", "{read}");
    }
    // The line taken off: string, not the type written before it.
    ok(
        &core,
        "card_update",
        json!({"target": "Schemas:JsonHolder", "value": {"connections": {"remove": [
            {"line": "embed", "from": "address"}
        ]}}}),
    );
    let read = ok(&core, "card_read", json!({"target": "Schemas:JsonHolder"}));
    assert_eq!(field(&read, "address")["type"], "string", "{read}");
}
