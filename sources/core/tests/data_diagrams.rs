//! Data drawn (`instance`; `specboard://SpecBoard/Docs/Design:CardKinds`「データ（instance）と定義のセクション」):
//! a section holding data, or one data card, put in a document with `![[…]]` is a flowchart of the
//! data and the lines between — the same picture written out, in the window's documents and through
//! MCP's `as: diagram`.

use crate::common;

use common::{checked, ok, op, refused, script_store};
use serde_json::json;
use specboard_core::Core;

const PROJECT: &str = "見本";

/// The sample project with hosts as data: `Data/CORE` holds git (two ports) and proxy, which
/// reaches git's port 3000; `Data/SANDBOX` holds wb_server, which is blocked from git's port 22;
/// git links to a document.
fn hosts() -> Core {
    let core = Core::open(script_store()).unwrap();
    let read = ok(&core, "project_read", json!({}));
    let mut tree = read["projectTree"].clone();
    tree["main_sections"]
        .as_array_mut()
        .unwrap()
        .push(json!({"type": "definition", "name": "Data"}));
    ok(
        &core,
        "project_update",
        json!({"value": {"project_tree": tree}}),
    );
    for section in ["Data/CORE", "Data/SANDBOX"] {
        ok(&core, "section_create", json!({"target": section}));
    }
    for (name, color, dash) in [("reaches", "green", "solid"), ("blocked", "red", "dashed")] {
        ok(
            &core,
            "kind_create",
            json!({"target": name, "value": {"of": "line", "label": name,
                "description": "ホストからホストのポートへ", "base": "link",
                "look": {"color": color, "dash": dash, "begin": "none", "end": "arrow"},
                "from": [{"kinds": ["instance"], "at": "card"}],
                "to": {"kinds": ["instance"], "at": ["card", "row"]}}}),
        );
    }
    ok(
        &core,
        "card_create",
        json!({"value": {
            "Schemas:Port": {"kind": "value", "fields": [
                {"name": "number", "type": "int"}, {"name": "protocol", "type": "string"}]},
            "Schemas:Host": {"kind": "entity", "fields": [
                {"name": "ip", "type": "string"}, {"name": "ports", "type": "string", "array": true}],
                "connections": [{"line": "embed", "from": "ports", "to": "Schemas:Port"}]}
        }}),
    );
    ok(
        &core,
        "card_create",
        json!({"value": {
            "Data/CORE:git": {"kind": "instance", "of": "Schemas:Host", "data": {"ip": "192.0.2.5",
                "ports": [{"number": 3000, "protocol": "tcp"}, {"number": 22, "protocol": "tcp"}]}},
            "Data/CORE:proxy": {"kind": "instance", "of": "Schemas:Host", "data": {"ip": "192.0.2.2"}},
            "Data/SANDBOX:wb_server": {"kind": "instance", "of": "Schemas:Host",
                "data": {"ip": "192.0.2.99"}}
        }}),
    );
    for (card, line) in [
        (
            "Data/CORE:proxy",
            json!({"line": "reaches", "to": "Data/CORE:git", "at": "ports[number=3000,protocol=tcp]"}),
        ),
        (
            "Data/SANDBOX:wb_server",
            json!({"line": "blocked", "to": "Data/CORE:git", "at": "ports[number=22,protocol=tcp]"}),
        ),
        (
            "Data/CORE:git",
            json!({"line": "link", "to": "Docs:Overview"}),
        ),
    ] {
        ok(
            &core,
            "card_update",
            json!({"target": card, "value": {"connections": {"add": [line]}}}),
        );
    }
    core
}

/// A document of the docs export holding `body`, as it is written out.
fn written(core: &Core, body: &str) -> String {
    ok(
        core,
        "card_create",
        json!({"target": "Docs:Map", "value": {"kind": "freedoc", "body": body}}),
    );
    let read = ok(
        core,
        "export_read",
        json!({"targetdir": "[default]", "target": "docs", "value": {"files": ["notes/Map.md"]}}),
    );
    read["files"][0]["text"].as_str().unwrap().to_string()
}

/// The mermaid blocks of a text, each without its fences.
fn blocks(text: &str) -> Vec<String> {
    text.split("```mermaid\n")
        .skip(1)
        .map(|b| b[..b.find("```").unwrap()].to_string())
        .collect()
}

/// A diagram's picture: past its head.
fn picture(mermaid: &str) -> &str {
    &mermaid[mermaid.find('\n').unwrap() + 1..]
}

/// The rev its head says.
fn rev(mermaid: &str) -> &str {
    let head = mermaid.lines().next().unwrap();
    &head[head.find("rev=").unwrap() + 4..]
}

const CORE_WITH_IP: &str = r#"flowchart LR
    n1["git<br/>192.0.2.5"]
    n2["proxy<br/>192.0.2.2"]
    n3["Docs:Overview"]
    n4["Data/SANDBOX:wb_server<br/>192.0.2.99"]
    n1 --> n3
    n2 -->|"3000/tcp"| n1
    n4 -->|"22/tcp"| n1
    linkStyle 0 stroke:#6B7787,stroke-dasharray:2 4
    linkStyle 1 stroke:#1E7A44
    linkStyle 2 stroke:#A83232,stroke-dasharray:7 5
"#;

#[test]
fn a_section_of_data_is_put_in_a_document_as_a_flowchart_of_its_data_and_their_lines() {
    let core = hosts();
    let text = written(&core, "![[Data/CORE:|ip]]\n\n![[Data:]]");
    let drawn = blocks(&text);
    assert_eq!(drawn.len(), 2, "{text}");
    // A box a data card's name and the field asked; a line in its kind's colour and dash, said by
    // the values its landing picks (none at the card's head); the cards outside by their paths.
    assert!(drawn[0].starts_with("%% specboard rev="), "{}", drawn[0]);
    assert_eq!(picture(&drawn[0]), CORE_WITH_IP);
    // The sections in it each a subgraph, named from it; no field asked, the names alone.
    assert_eq!(
        picture(&drawn[1]),
        r#"flowchart LR
    subgraph s1["CORE"]
        n1["git"]
        n2["proxy"]
    end
    subgraph s2["SANDBOX"]
        n3["wb_server"]
    end
    n4["Docs:Overview"]
    n1 --> n4
    n2 -->|"3000/tcp"| n1
    n3 -->|"22/tcp"| n1
    linkStyle 0 stroke:#6B7787,stroke-dasharray:2 4
    linkStyle 1 stroke:#1E7A44
    linkStyle 2 stroke:#A83232,stroke-dasharray:7 5
"#
    );
}

#[test]
fn a_data_card_is_put_in_a_document_as_it_and_the_cards_its_lines_meet() {
    let core = hosts();
    let text = written(&core, "![[Data/CORE:git|ip]]\n\n![[Data/CORE:git|nope]]");
    let drawn = blocks(&text);
    assert_eq!(
        picture(&drawn[0]),
        r#"flowchart LR
    n1["git<br/>192.0.2.5"]
    n2["Docs:Overview"]
    n3["proxy<br/>192.0.2.2"]
    n4["Data/SANDBOX:wb_server<br/>192.0.2.99"]
    n1 --> n2
    n3 -->|"3000/tcp"| n1
    n4 -->|"22/tcp"| n1
    linkStyle 0 stroke:#6B7787,stroke-dasharray:2 4
    linkStyle 1 stroke:#1E7A44
    linkStyle 2 stroke:#A83232,stroke-dasharray:7 5
"#
    );
    // A field the data's shape does not have: said in its place, and by the check.
    assert!(
        text.contains("— 欄「nope」は、描くデータの形（Host）にありません"),
        "{text}"
    );
    let broken = checked(&core, "main")["brokenPaths"].to_string();
    assert!(broken.contains("Docs:Map"), "{broken}");
    assert!(broken.contains("nope"), "{broken}");
    assert!(!broken.contains("|ip"), "{broken}");
}

#[test]
fn the_window_and_mcp_draw_data_as_it_is_written_out() {
    let core = hosts();
    let exported = blocks(&written(&core, "![[Data/CORE:|ip]]"))[0].clone();
    // The window's document: the head saying what it was drawn from, the picture and its rev the
    // same.
    let project: String = rusqlite::Connection::open(core.place())
        .unwrap()
        .query_row("SELECT id FROM projects WHERE name = ?", [PROJECT], |r| {
            r.get(0)
        })
        .unwrap();
    let window = op(
        &core,
        json!({"op": "embed_diagram", "project": project, "path": "Data/CORE:", "option": "ip",
            "exported": false}),
    );
    let window = window["result"]["mermaid"].as_str().unwrap().to_string();
    assert!(window.starts_with("%% specboard project="), "{window}");
    assert!(
        window.contains(" data=Data/CORE: field=ip rev="),
        "{window}"
    );
    assert_eq!(picture(&window), CORE_WITH_IP);
    assert_eq!(rev(&window), rev(&exported));
    // MCP: the section, and one data card, the field asked by the value.
    let section = ok(
        &core,
        "section_read",
        json!({"target": "Data/CORE", "value": {"as": "diagram", "field": "ip"}}),
    );
    assert_eq!(section["data"], "Data/CORE:", "{section}");
    assert_eq!(section["field"], "ip", "{section}");
    assert!(section["rev"].is_string(), "{section}");
    assert_eq!(section["mermaid"], CORE_WITH_IP);
    assert_eq!(section["rev"], rev(&exported));
    let card = ok(
        &core,
        "card_read",
        json!({"target": "Data/CORE:git", "value": {"as": "diagram", "field": "ip"}}),
    );
    assert_eq!(card["data"], "Data/CORE:git", "{card}");
    assert_eq!(card["field"], "ip", "{card}");
    assert!(
        card["mermaid"]
            .as_str()
            .unwrap()
            .starts_with("flowchart LR\n    n1[\"git<br/>192.0.2.5\"]"),
        "{card}"
    );
    // Its rev alone.
    let only = ok(
        &core,
        "section_read",
        json!({"target": "Data/CORE", "value": {"as": "diagram", "field": "ip", "revisionOnly": true}}),
    );
    assert_eq!(only["rev"], rev(&exported), "{only}");
    assert_eq!(only["spec"]["data"], "Data/CORE:", "{only}");
    // What a class diagram is cut by is no data diagram's; a field, no class diagram's.
    let why = refused(
        &core,
        "section_read",
        json!({"target": "Data/CORE", "value": {"as": "diagram", "depth": 2}}),
    );
    assert!(why.contains("データの図では depth は使いません"), "{why}");
    let why = refused(
        &core,
        "section_read",
        json!({"target": "Schemas", "value": {"as": "diagram", "field": "ip"}}),
    );
    assert!(
        why.contains("field はデータ（instance）の図のときだけです"),
        "{why}"
    );
}

#[test]
fn a_section_holding_shapes_beside_its_data_is_drawn_as_its_shapes() {
    let core = hosts();
    ok(
        &core,
        "card_create",
        json!({"value": {
            "Free:spare": {"kind": "instance", "of": "Schemas:Host", "data": {"ip": "192.0.2.9"}},
            "Free:Thing": {"kind": "entity", "fields": [{"name": "id", "type": "uuid"}]}
        }}),
    );
    let text = written(&core, "![[Free:]]\n\n![[Free:|ip]]");
    let drawn = blocks(&text);
    assert!(picture(&drawn[0]).starts_with("classDiagram"), "{text}");
    assert!(
        text.contains("— セクションの図に |ip は付きません"),
        "{text}"
    );
}
