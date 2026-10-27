//! The kinds' tools: the kinds the project has, each in a line of what it is for (`kind_list`), one's
//! definition (`kind_read`), and the project's own made, changed and let go by name (`kind_create`,
//! `kind_update`, `kind_delete`) — a built-in kind's context written over, a name one kind's among the
//! three lists, `dryRun` writing nothing; and what is directly under each root the project's
//! (`project_read`'s `projectTree`, `project_update`'s `project_tree`).
//!
//! Over the sample project (`見本`).

use crate::common;

use common::{checked, ok, refused, script_store};
use serde_json::{Value, json};
use specboard_core::Core;

/// The kind `risk` (a choice `severity`, rows `steps`), as `kind_create`'s value.
fn risk() -> Value {
    json!({
        "of": "card", "label": "リスク", "color": "#C2410C", "header": ["severity"],
        "context": { "purpose": "起こりうる困りごと", "use_when": "設計の危うい所を残すとき" },
        "fields": [
            { "name": "severity", "label": "深刻さ", "fieldtype": "radio", "options": ["高", "中", "低"], "default": "中" }
        ],
        "rows": [
            { "name": "steps", "label": "対策", "fields": [
                { "name": "name", "fieldtype": "text", "pattern": "identifier", "required": true }
            ] }
        ]
    })
}

/// `risk`, a section kind `risks` that holds it, and a section `Risks` of it under main's root.
fn defined(core: &Core) {
    ok(
        core,
        "kind_create",
        json!({"target": "risk", "value": risk()}),
    );
    ok(
        core,
        "kind_create",
        json!({"target": "risks", "value": {"of": "section", "label": "リスク", "holds": ["risk", "note"]}}),
    );
    let mut tree = ok(core, "project_read", json!({}))["projectTree"].clone();
    tree["main_sections"]
        .as_array_mut()
        .unwrap()
        .push(json!({"type": "risks", "name": "Risks"}));
    ok(
        core,
        "project_update",
        json!({"value": {"project_tree": tree}}),
    );
}

fn listed<'a>(list: &'a Value, key: &str, name: &str) -> &'a Value {
    list.as_array()
        .unwrap()
        .iter()
        .find(|k| k[key] == name)
        .unwrap_or_else(|| panic!("{name} が一覧に無い: {list}"))
}

#[test]
fn the_list_says_each_kind_in_a_line_and_where_it_goes() {
    let core = Core::open(script_store()).unwrap();
    let list = ok(&core, "kind_list", json!({}));
    let entity = listed(&list["cards"], "kind", "entity");
    assert_eq!(entity["purpose"], "単位で保存・送信される記録（ID を持つ）");
    assert_eq!(entity["in"], json!(["schema", "definition"]));
    // When to use it and the rest are kind_read's and the guide's; a built-in kind is not marked.
    for key in ["fields", "use_when", "not_for", "builtin", "own"] {
        assert!(entity.get(key).is_none(), "{key}: {entity}");
    }
    let begin = listed(&list["cards"], "kind", "begin");
    assert_eq!(begin["in"], json!(["flow"]));
    // A kind of section: what it is for from its description, where it is made.
    let schema = listed(&list["sections"], "section", "schema");
    assert!(schema["purpose"].as_str().unwrap().contains("データの形"));
    assert_eq!(schema["trees"], json!(["main"]));
    // A kind of line: what it is for, the kinds it leaves and reaches.
    let embed = listed(&list["lines"], "line", "embed");
    assert_eq!(embed["to"], json!(["entity", "value", "enum"]));
    assert_eq!(embed["label"], "内包", "{embed}");
    assert!(embed["purpose"].is_string(), "{embed}");
    // Short enough to read whole, in every language: the sample's working directories put their
    // kinds of section in it too, beside the actor.
    for lang in specboard_core::text::LANGUAGES {
        let _lang = specboard_core::text::bind(Some(lang));
        let list = ok(&core, "kind_list", json!({}));
        let chars = serde_json::to_string(&list).unwrap().chars().count();
        assert!(chars <= 9_000, "{lang}: {chars}");
    }
}

#[test]
fn a_kind_is_made_read_and_named_once_among_the_lists() {
    let core = Core::open(script_store()).unwrap();
    defined(&core);
    let read = ok(&core, "kind_read", json!({"target": "risk"}));
    assert_eq!(read["of"], "card");
    assert_eq!(read["kind"], "risk");
    assert_eq!(read["fields"][0]["name"], "severity");
    let both = ok(&core, "kind_read", json!({"target": ["risk", "risks"]}));
    assert_eq!(both["kinds"][1]["of"], "section");
    let list = ok(&core, "kind_list", json!({}));
    assert_eq!(
        listed(&list["cards"], "kind", "risk")["in"],
        json!(["risks"])
    );
    assert_eq!(listed(&list["cards"], "kind", "risk")["own"], true);
    // One name, one kind: the built-in ones' too.
    for (name, of) in [("risk", "section"), ("entity", "card"), ("schema", "line")] {
        let said = refused(
            &core,
            "kind_create",
            json!({"target": name, "value": {"of": of, "label": "x"}}),
        );
        assert!(said.contains("もうあります"), "{name}: {said}");
    }
    assert!(
        refused(
            &core,
            "kind_create",
            json!({"target": "x", "value": {"label": "x"}})
        )
        .contains("value に of（作る種類の別")
    );
    let card = ok(
        &core,
        "card_create",
        json!({"target": "Risks:Outage", "value": {"kind": "risk", "severity": "高"}}),
    );
    assert_eq!(card["created"][0]["path"], "Risks:Outage");
}

#[test]
fn a_kind_changes_by_what_is_written_and_renamed_its_cards_follow() {
    let core = Core::open(script_store()).unwrap();
    defined(&core);
    ok(
        &core,
        "card_create",
        json!({"target": "Risks:Outage", "value": {"kind": "risk", "severity": "高"}}),
    );
    // A key written over, another taken away.
    let changed = ok(
        &core,
        "kind_update",
        json!({"target": "risk", "value": {"label": "危うさ", "header": null}}),
    );
    assert_eq!(changed["label"], "危うさ");
    assert!(changed.get("header").is_none(), "{changed}");
    assert_eq!(changed["fields"][0]["name"], "severity");
    // A field renamed with its values, and the kind: the definition and the cards follow.
    let tried = ok(
        &core,
        "kind_update",
        json!({"target": "risk", "value": {"dryRun": true, "renames": [{"field": "severity", "to": "level"}]}}),
    );
    assert_eq!(tried["ok"], true, "{tried}");
    assert_eq!(tried["renamed"][0]["cards"], 1);
    let renamed = ok(
        &core,
        "kind_update",
        json!({"target": "risk", "value": {"name": "hazard", "renames": [{"field": "severity", "to": "level"}]}}),
    );
    assert_eq!(renamed["kind"], "hazard");
    assert_eq!(renamed["fields"][0]["name"], "level");
    let card = ok(&core, "card_read", json!({"target": "Risks:Outage"}));
    assert_eq!(card["kind"], "hazard");
    assert_eq!(card["level"], "高");
    // The old name is gone; the section kind that held it names it by the new one.
    assert!(refused(&core, "kind_read", json!({"target": "risk"})).contains("ありません"));
    assert_eq!(renamed["alsoChanged"], json!(["risks"]), "{renamed}");
    assert_eq!(
        ok(&core, "kind_read", json!({"target": "risks"}))["holds"],
        json!(["hazard", "note"])
    );
    for (value, said) in [
        (json!({"of": "section"}), "of（種類の別）は変えられません"),
        (json!({"kind": "x"}), "名前を変えるのは value の name です"),
        (json!({}), "変える項目"),
    ] {
        let refusal = refused(
            &core,
            "kind_update",
            json!({"target": "hazard", "value": value}),
        );
        assert!(refusal.contains(said), "{said}: {refusal}");
    }
    assert!(
        refused(
            &core,
            "kind_update",
            json!({"target": "risks", "value": {"name": "perils"}})
        )
        .contains("まだ変えられません")
    );
}

#[test]
fn a_built_in_kind_has_its_context_alone_written_over() {
    let core = Core::open(script_store()).unwrap();
    let written = ok(
        &core,
        "kind_update",
        json!({"target": "formatdoc", "value": {"context": {"use_when": "このプロジェクトの設計書と仕様書"}}}),
    );
    assert_eq!(written["builtin"], true);
    assert_eq!(written["contextChanged"], true);
    assert_eq!(
        written["context"]["use_when"],
        "このプロジェクトの設計書と仕様書"
    );
    assert!(
        written["context"]["purpose"]
            .as_str()
            .unwrap()
            .contains("決めた書式")
    );
    let list = ok(&core, "kind_list", json!({}));
    let formatdoc = listed(&list["cards"], "kind", "formatdoc");
    assert_eq!(formatdoc["contextChanged"], true);
    assert_eq!(
        ok(&core, "kind_read", json!({"target": "formatdoc"}))["context"]["use_when"],
        "このプロジェクトの設計書と仕様書"
    );
    // A section's and a line's too; nothing else of a built-in kind.
    ok(
        &core,
        "kind_update",
        json!({"target": "embed", "value": {"context": {"use_when": "住所"}}}),
    );
    assert!(
        refused(
            &core,
            "kind_update",
            json!({"target": "entity", "value": {"label": "記録の形"}})
        )
        .contains("context")
    );
    assert!(
        refused(
            &core,
            "kind_update",
            json!({"target": "entity", "value": {"context": {"why": "x"}}})
        )
        .contains("why")
    );
    // Back to its own.
    let back = ok(
        &core,
        "kind_update",
        json!({"target": "formatdoc", "value": {"context": null}}),
    );
    assert!(back.get("contextChanged").is_none(), "{back}");
    assert!(
        back["context"]["use_when"]
            .as_str()
            .unwrap()
            .contains("設計書・仕様書のように")
    );
}

#[test]
fn a_kind_in_use_stays_and_a_built_in_one_cannot_go() {
    let core = Core::open(script_store()).unwrap();
    defined(&core);
    ok(
        &core,
        "card_create",
        json!({"target": "Risks:Outage", "value": {"kind": "risk"}}),
    );
    let said = refused(&core, "kind_delete", json!({"target": "risk"}));
    assert!(said.contains("使われている"), "{said}");
    assert!(refused(&core, "kind_delete", json!({"target": "entity"})).contains("組み込み"));
    ok(&core, "card_delete", json!({"target": "Risks:Outage"}));
    let tried = ok(
        &core,
        "kind_delete",
        json!({"target": "risk", "value": {"dryRun": true}}),
    );
    assert_eq!(tried["ok"], true, "{tried}");
    assert!(ok(&core, "kind_read", json!({"target": "risk"}))["kind"] == "risk");
    let gone = ok(&core, "kind_delete", json!({"target": "risk"}));
    assert_eq!(gone["deleted"], json!(["risk"]));
}

#[test]
fn what_is_under_each_root_is_the_projects_to_read_and_write() {
    let core = Core::open(script_store()).unwrap();
    let read = ok(&core, "project_read", json!({}));
    let tree = read["projectTree"].clone();
    assert!(tree["main_sections"].is_array(), "{read}");
    assert_eq!(tree["board_sections"][0]["type"], "goals");
    let mut written = tree.clone();
    written["main_sections"]
        .as_array_mut()
        .unwrap()
        .push(json!({"type": "document", "name": "More"}));
    let tried = ok(
        &core,
        "project_update",
        json!({"value": {"dryRun": true, "project_tree": written}}),
    );
    assert_eq!(tried["ok"], true, "{tried}");
    assert!(tried["projectTree"].to_string().contains("More"), "{tried}");
    assert_eq!(ok(&core, "project_read", json!({}))["projectTree"], tree);
    // The kinds are not project_update's any more, nor project_read's.
    assert!(
        refused(
            &core,
            "project_update",
            json!({"value": {"kinds": {"cards": []}}})
        )
        .contains("に使えないキーです: kinds")
    );
    assert!(
        refused(
            &core,
            "project_update",
            json!({"value": {"renames": [{"kind": "risk", "to": "hazard"}]}})
        )
        .contains("kind_update")
    );
}

/// The kinds guide as `guide about: kinds` says it.
fn kinds_guide() -> String {
    let Ok(specboard_core::mcp::Output::Json(guide)) =
        specboard_core::mcp::guide(Some("kind_create"))
    else {
        panic!("guide の about: kind_create が JSON でない")
    };
    assert!(guide["more"].as_str().unwrap().contains("about: kinds"));
    let said = specboard_core::mcp::catalog()["kindsGuide"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        specboard_core::mcp::guide(Some("kinds")).unwrap().text(),
        said
    );
    said
}

/// The definition the guide's bullet starting with `head` gives: its first JSON object.
fn guide_definition(said: &str, head: &str) -> Value {
    let line = said
        .lines()
        .find(|l| l.starts_with(head))
        .unwrap_or_else(|| panic!("{head} の段落が無い: {said}"));
    let at = line.find('{').unwrap();
    serde_json::Deserializer::from_str(&line[at..])
        .into_iter::<Value>()
        .next()
        .unwrap()
        .unwrap()
}

/// A state machine drawn as a kind and a line from a row to a row of the same card draws nothing
/// on a diagram: the guide says to write one as a flow, and its definitions are the ones it
/// gives for a card's kind and a line's, each made as written.
#[test]
fn the_guide_says_state_transitions_are_a_flow_and_its_definitions_are_made_as_written() {
    let said = kinds_guide();
    assert!(said.contains("状態遷移"), "{said}");
    assert!(said.contains("flow で書く"), "{said}");
    assert!(!said.contains("statemachine"), "{said}");
    let core = Core::open(script_store()).unwrap();
    let mut card = guide_definition(&said, "- カードの種類（of: card）");
    card["of"] = "card".into();
    ok(
        &core,
        "kind_create",
        json!({"target": "risk", "value": card}),
    );
    let mut line = guide_definition(&said, "- 線の種類（of: line）");
    line["of"] = "line".into();
    ok(
        &core,
        "kind_create",
        json!({"target": "verifies", "value": line}),
    );
    ok(
        &core,
        "card_create",
        json!({"target": "Free:Leak", "value": {"kind": "risk", "severity": "高",
            "steps": [{"name": "rotate"}]}}),
    );
    let read = checked(&core, "main");
    for key in ["lineProblems", "cardProblems"] {
        assert!(read.get(key).is_none(), "{key}: {read}");
    }
}

#[test]
fn a_projects_values_of_via_are_added_on_the_flow_and_held_to_as_the_built_in_ones() {
    let core = Core::open(script_store()).unwrap();
    let builtin = [
        "stdio",
        "file",
        "exec",
        "http",
        "grpc",
        "websocket",
        "queue",
    ];
    let flow = ok(&core, "kind_read", json!({"target": "flow"}));
    assert_eq!(flow["vias"], json!(builtin), "{flow}");
    assert!(flow.get("viasChanged").is_none(), "{flow}");
    // Named in lowercase letters, digits and -, not a built-in one, once each.
    for (vias, said) in [
        (json!(["Kafka"]), "英小文字・数字・-"),
        (json!(["grpc"]), "組み込みの値です"),
        (json!(["nats", "nats"]), "2 度あります"),
    ] {
        let refused = refused(
            &core,
            "kind_update",
            json!({"target": "flow", "value": {"vias": vias}}),
        );
        assert!(refused.contains(said), "{said}: {refused}");
    }
    // On the flow alone: the list is one for a begin, a row and a run.
    let said = refused(
        &core,
        "kind_update",
        json!({"target": "process", "value": {"vias": ["kafka"]}}),
    );
    assert!(said.contains("vias は変えられません"), "{said}");
    ok(
        &core,
        "kind_update",
        json!({"target": "flow", "value": {"vias": ["kafka"]}}),
    );
    let flow = ok(&core, "kind_read", json!({"target": "flow"}));
    let mut all: Vec<&str> = builtin.to_vec();
    all.push("kafka");
    assert_eq!(flow["vias"], json!(all), "{flow}");
    assert_eq!(flow["viasChanged"], true);
    let listed = ok(&core, "kind_list", json!({}));
    assert_eq!(listed_flow(&listed)["viasChanged"], true, "{listed}");
    let begin = ok(&core, "kind_read", json!({"target": "begin"}));
    assert!(begin.to_string().contains("\"kafka\""), "{begin}");
    // Written in a node as a built-in one is; one the project has not added is refused.
    ok(
        &core,
        "card_create",
        json!({"value": {
            "Charts/Shop:Logs": {"kind": "flow"},
            "Charts/Shop:Logs.start": {"kind": "begin", "trigger": "receive", "program": "agent", "via": "kafka",
                "connections": [{"line": "next", "to": "push"}]},
            "Charts/Shop:Logs.push": {"kind": "process", "steps": [{"do": "send", "program": "store", "via": "kafka"}],
                "connections": [{"line": "next", "to": "done"}]},
            "Charts/Shop:Logs.done": {"kind": "end", "outcome": "success"}
        }}),
    );
    let said = refused(
        &core,
        "card_update",
        json!({"target": "Charts/Shop:Logs.push", "value": {"steps": [{"do": "send", "program": "store", "via": "nats"}]}}),
    );
    assert!(said.contains("nats") && said.contains("kafka"), "{said}");
    // Added to and taken from by name; one a node says stays.
    ok(
        &core,
        "kind_update",
        json!({"target": "flow", "value": {"vias": {"add": ["nats"]}}}),
    );
    let flow = ok(&core, "kind_read", json!({"target": "flow"}));
    assert_eq!(
        flow["vias"].as_array().unwrap()[builtin.len()..],
        [json!("kafka"), json!("nats")],
        "{flow}"
    );
    let said = refused(
        &core,
        "kind_update",
        json!({"target": "flow", "value": {"vias": {"remove": ["kafka"]}}}),
    );
    assert!(
        said.contains("via の値「kafka」は 2 か所で使われているので消せません"),
        "{said}"
    );
    ok(
        &core,
        "kind_update",
        json!({"target": "flow", "value": {"vias": {"remove": ["nats"]}}}),
    );
    // The screen reads them from the core.
    let project: String = rusqlite::Connection::open(core.place())
        .unwrap()
        .query_row("SELECT id FROM projects WHERE name = ?", ["見本"], |r| {
            r.get(0)
        })
        .unwrap();
    let detail = common::op(&core, json!({"op": "project_detail", "project": project}));
    assert_eq!(detail["result"]["kinds"]["vias"], json!(all), "{detail}");
}

fn listed_flow(list: &Value) -> &Value {
    list["cards"]
        .as_array()
        .unwrap()
        .iter()
        .find(|k| k["kind"] == "flow")
        .unwrap_or_else(|| panic!("flow が一覧に無い: {list}"))
}

#[test]
fn kind_list_says_what_each_kind_is_for() {
    let core = Core::open(script_store()).unwrap();
    let list = ok(&core, "kind_list", json!({}));
    for line in list["lines"].as_array().unwrap() {
        assert!(
            line["label"].is_string() && line["purpose"].is_string(),
            "線の種類に表示名と何のためかが無い: {line}"
        );
    }
    assert_eq!(listed(&list["lines"], "line", "mentions")["label"], "言及");
    // The first sentence says what the kind holds, not its label again.
    for (of, key) in [
        ("cards", "kind"),
        ("sections", "section"),
        ("lines", "line"),
    ] {
        for kind in list[of].as_array().unwrap() {
            let label = kind["label"].as_str().unwrap_or_default();
            let purpose = kind["purpose"].as_str().unwrap_or_default();
            let bare = purpose
                .trim_end_matches(['。', '.'])
                .trim_end_matches("です");
            let short = bare.chars().count() <= label.chars().count() + 12;
            assert!(
                !label.is_empty() && bare != label && !(short && bare.ends_with(label)),
                "{} の何のためかが表示名の言い直し: {kind}",
                kind[key]
            );
        }
    }
    // Only the kinds card_create and section_create refuse, made by the core alone.
    let made_by_core = [
        "file",
        "symbol",
        "section",
        "complete",
        "cancelled",
        "workdir",
        "directory",
    ];
    let mut marked = Vec::new();
    for (of, key) in [("cards", "kind"), ("sections", "section")] {
        for kind in list[of].as_array().unwrap() {
            let name = kind[key].as_str().unwrap();
            assert_eq!(
                kind.get("systemOnly") == Some(&json!(true)),
                made_by_core.contains(&name),
                "{name} の systemOnly: {kind}"
            );
            if kind.get("systemOnly").is_some() {
                marked.push(name.to_string());
            }
        }
    }
    assert!(marked.contains(&"complete".to_string()), "{marked:?}");
}

#[test]
fn an_enum_has_no_shape_only_keys() {
    let core = Core::open(script_store()).unwrap();
    let read = ok(&core, "kind_read", json!({"target": ["enum"]}));
    let kinds = read["kinds"].as_array().unwrap_or_else(|| panic!("{read}"));
    let of = |name: &str| {
        kinds
            .iter()
            .find(|k| k["kind"] == name)
            .unwrap_or_else(|| panic!("{read}"))
    };
    // The keys only a shape has (rules, absentFields, discriminator) are the shape's own, not
    // inherited by an enum.
    assert_eq!(of("enum")["extends"], "base", "{read}");
    let names: Vec<&str> = of("enum")["fields"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|f| f["name"].as_str())
        .collect();
    assert_eq!(names, ["open", "standalone"], "{read}");
}
