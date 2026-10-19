//! Names of any characters but control ones (`specboard://SpecBoard/Docs/Design:DesignSections`
//! 「名前に使える字と、パスの中での囲み」): a section, a card, a field, a row and a node named with
//! spaces, dots, slashes, quotes or Japanese are made, read back by paths that quote only the names
//! that need it, named in texts and lines, followed through a rename with their quotes, and told
//! apart from one another in any ASCII case alone.

use crate::common;

use common::{checked, ok, refused, script_store};
use serde_json::{Value, json};
use specboard_core::Core;

fn core() -> Core {
    Core::open(script_store()).unwrap()
}

/// The places with names a path quotes: a document, a section and one inside it with a note, two
/// fields (one bare, one quoted), and a document that names them all.
fn named(core: &Core) {
    ok(
        core,
        "card_create",
        json!({"target": "Docs:'release notes.md'", "value": {"kind": "freedoc", "body": "見た"}}),
    );
    ok(core, "section_create", json!({"target": "Free/'.claude'"}));
    ok(
        core,
        "section_create",
        json!({"target": "Free/'.claude'/rules"}),
    );
    ok(
        core,
        "card_create",
        json!({"target": "Free/'.claude'/rules:'ci.md'", "value": {"kind": "note", "body": "CI"}}),
    );
    ok(
        core,
        "card_create",
        json!({"target": "Schemas:Customer.e-mail", "value": {"type": "string"}}),
    );
    ok(
        core,
        "card_create",
        json!({"target": "Schemas:Customer.'home address'", "value": {"type": "string", "const": "Tokyo"}}),
    );
    ok(
        core,
        "card_create",
        json!({"target": "Docs:Index", "value": {"kind": "freedoc", "body":
            "[[Docs:'release notes.md']] [[Free/'.claude'/rules:'ci.md']] [['Schemas':Customer.'e-mail']] \
             住所は {{Schemas:Customer.'home address'}}"}}),
    );
}

fn body(core: &Core, path: &str) -> String {
    ok(core, "card_read", json!({"target": path}))["body"]
        .as_str()
        .unwrap()
        .to_string()
}

#[test]
fn a_name_that_holds_a_separator_is_quoted_in_every_path_and_read_back_by_it() {
    let core = core();
    named(&core);
    let sections = ok(&core, "section_list", json!({}));
    let listed: Vec<&str> = sections["sections"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["section"].as_str().unwrap())
        .collect();
    assert!(listed.contains(&"Free/'.claude'"), "{listed:?}");
    for (target, said) in [
        ("Free/'設計/詳細'", "Free/'設計/詳細'"),
        ("Free/'Q&A 2026'", "Free/'Q&A 2026'"),
        ("Free/注文-明細", "Free/注文-明細"),
    ] {
        let made = ok(&core, "section_create", json!({"target": target}));
        assert_eq!(made["created"], said, "{made}");
    }
    assert!(listed.contains(&"Free/'.claude'/rules"), "{listed:?}");
    let note = ok(
        &core,
        "card_read",
        json!({"target": "Free/'.claude'/rules:'ci.md'"}),
    );
    assert_eq!(note["path"], "Free/'.claude'/rules:'ci.md'");
    // Quoted where it need not be, the same card, answered as it is kept.
    let again = ok(
        &core,
        "card_read",
        json!({"target": "'Free'/'.claude'/'rules':'ci.md'"}),
    );
    assert_eq!(again["path"], "Free/'.claude'/rules:'ci.md'");
    let customer = ok(&core, "card_read", json!({"target": "Schemas:Customer"}));
    let names: Vec<&str> = customer["fields"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["id", "name", "e-mail", "home address"]);
    let field = ok(
        &core,
        "card_read",
        json!({"target": "Schemas:Customer.'home address'"}),
    );
    assert_eq!(field["path"], "Schemas:Customer.'home address'");
    // What the document names, each quoted where it needs it as it was kept (a needless quote let
    // go of).
    let index = ok(&core, "card_read", json!({"target": "Docs:Index"}));
    assert!(
        index["body"]
            .as_str()
            .unwrap()
            .contains("[[Schemas:Customer.e-mail]]"),
        "{index}"
    );
    assert_eq!(
        index["mentions"],
        json!([
            "Docs:'release notes.md'",
            "Free/'.claude'/rules:'ci.md'",
            "Schemas:Customer.e-mail",
            "Schemas:Customer.'home address'"
        ])
    );
    let inspection = checked(&core, "main");
    let broken: Vec<&Value> = inspection["brokenPaths"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|i| i["at"] == "Docs:Index")
        .collect();
    assert!(broken.is_empty(), "{broken:?}");
    // A quote that never closes is said so.
    let why = refused(
        &core,
        "card_read",
        json!({"target": "Docs:'release notes.md"}),
    );
    assert!(why.contains("一重引用符が閉じていません"), "{why}");
}

#[test]
fn a_rename_is_followed_with_the_quotes_its_new_name_needs() {
    let core = core();
    named(&core);
    ok(
        &core,
        "card_update",
        json!({"target": "Schemas/Ordering:Order", "value": {"connections": {"add": [
            {"line": "link", "to": "Docs:'release notes.md'"}
        ]}}}),
    );
    ok(
        &core,
        "card_update",
        json!({"target": "Docs:'release notes.md'", "value": {"name": "release-notes"}}),
    );
    ok(
        &core,
        "card_update",
        json!({"target": "Schemas:Customer.e-mail", "value": {"name": "mail address"}}),
    );
    ok(
        &core,
        "section_update",
        json!({"target": "Free/'.claude'", "value": {"name": "claude"}}),
    );
    ok(
        &core,
        "card_update",
        json!({"target": "Free/claude/rules:'ci.md'", "value": {"name": "CI: rules"}}),
    );
    assert_eq!(
        body(&core, "Docs:Index"),
        "[[Docs:release-notes]] [[Free/claude/rules:'CI: rules']] [[Schemas:Customer.'mail address']] \
         住所は {{Schemas:Customer.'home address'}}"
    );
    // A line to a field by a quoted name, followed too.
    ok(
        &core,
        "card_update",
        json!({"target": "Schemas/Ordering:Order", "value": {"connections": {"add": [
            {"line": "points", "from": "customer_id", "to": "Schemas:Customer", "at": "'mail address'"}
        ]}}}),
    );
    ok(
        &core,
        "card_update",
        json!({"target": "Schemas:Customer.'mail address'", "value": {"name": "mail.address"}}),
    );
    let order = ok(
        &core,
        "card_read",
        json!({"target": "Schemas/Ordering:Order", "value": {"include": ["connections"]}}),
    );
    let lines: Vec<&Value> = order["connections"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|l| l["line"] == "points" && l.get("at").is_some())
        .collect();
    assert_eq!(lines.len(), 1, "{}", order["connections"]);
    assert!(
        order["connections"]
            .as_array()
            .unwrap()
            .contains(&json!({"line": "link", "to": "Docs:release-notes"})),
        "{}",
        order["connections"]
    );
    assert_eq!(lines[0]["at"], "'mail.address'");
    assert!(body(&core, "Docs:Index").contains("[[Schemas:Customer.'mail.address']]"));
}

#[test]
fn rows_nodes_and_versions_take_names_a_path_quotes() {
    let core = core();
    ok(
        &core,
        "card_create",
        json!({"target": "Docs:Guide", "value": {"kind": "formatdoc", "rows": [
            {"name": "Owner's guide", "kind": "item", "heading": "持ち主", "rows": [
                {"name": "step 1", "kind": "text", "body": "まず"}
            ]}
        ]}}),
    );
    let row = ok(
        &core,
        "card_read",
        json!({"target": "Docs:Guide.'Owner''s guide'.'step 1'"}),
    );
    assert_eq!(row["path"], "Docs:Guide.'Owner''s guide'.'step 1'");
    assert_eq!(row["body"], "まず");
    ok(
        &core,
        "card_create",
        json!({"target": "Docs:Changes", "value": {"kind": "stackdoc", "versions": [
            {"name": "0.2.1", "key": "0.2.1", "body": "[[Docs:Guide.'Owner''s guide']]"}
        ]}}),
    );
    let version = ok(
        &core,
        "card_read",
        json!({"target": "Docs:Changes.'0.2.1'"}),
    );
    assert_eq!(version["path"], "Docs:Changes.'0.2.1'");
    ok(
        &core,
        "card_update",
        json!({"target": "Docs:Guide.'Owner''s guide'", "value": {"name": "持ち主の手引き"}}),
    );
    let version = ok(
        &core,
        "card_read",
        json!({"target": "Docs:Changes.'0.2.1'"}),
    );
    assert_eq!(version["body"], "[[Docs:Guide.持ち主の手引き]]");
    // A flow's node of a name with a space.
    let flow = ok(
        &core,
        "card_read",
        json!({"target": "Charts/Shop:Checkout"}),
    );
    assert!(flow.get("path").is_some());
    ok(
        &core,
        "card_create",
        json!({"target": "Charts/Shop:Checkout.'pay now'", "value": {"kind": "process"}}),
    );
    let node = ok(
        &core,
        "card_read",
        json!({"target": "Charts/Shop:Checkout.'pay now'"}),
    );
    assert_eq!(node["path"], "Charts/Shop:Checkout.'pay now'");
}

#[test]
fn a_name_with_a_control_character_is_refused_and_spaces_around_one_are_dropped() {
    let core = core();
    for (tool, target, value) in [
        ("card_create", "Docs:'a\nb'", json!({"kind": "note"})),
        ("section_create", "Free/'a\nb'", json!({})),
        (
            "card_create",
            "Schemas:Customer.'a\nb'",
            json!({"type": "string"}),
        ),
        ("card_update", "Schemas:Customer", json!({"name": "a\nb"})),
    ] {
        let why = refused(&core, tool, json!({"target": target, "value": value}));
        assert!(why.contains("制御文字"), "{tool} {target}: {why}");
    }
    let why = refused(
        &core,
        "section_update",
        json!({"target": "Free", "value": {"name": "a\tb"}}),
    );
    assert!(why.contains("制御文字"), "{why}");
    let why = refused(
        &core,
        "card_update",
        json!({"target": "Schemas:Customer.name", "value": {"name": "x\u{7}"}}),
    );
    assert!(why.contains("制御文字"), "{why}");
    let made = ok(
        &core,
        "card_update",
        json!({"target": "Schemas:Customer", "value": {"name": "  Client  "}}),
    );
    assert_eq!(made["path"], "Schemas:Client", "{made}");
    let long = "x".repeat(256);
    let why = refused(
        &core,
        "card_update",
        json!({"target": "Schemas:Client", "value": {"name": long}}),
    );
    assert!(why.contains("255"), "{why}");
}

#[test]
fn names_are_one_in_any_ascii_case_and_other_letters_are_their_own() {
    let core = core();
    let why = refused(
        &core,
        "card_create",
        json!({"target": "Schemas:customer", "value": {"kind": "entity"}}),
    );
    assert!(why.contains("customer"), "{why}");
    ok(
        &core,
        "card_create",
        json!({"target": "Schemas:Éclair", "value": {"kind": "entity"}}),
    );
    ok(
        &core,
        "card_create",
        json!({"target": "Schemas:éclair", "value": {"kind": "entity"}}),
    );
    ok(
        &core,
        "card_create",
        json!({"target": "Schemas:注文", "value": {"kind": "entity"}}),
    );
    refused(
        &core,
        "card_create",
        json!({"target": "Schemas:注文", "value": {"kind": "entity"}}),
    );
    ok(&core, "section_create", json!({"target": "Free/Ämter"}));
    ok(&core, "section_create", json!({"target": "Free/ämter"}));
    refused(&core, "section_create", json!({"target": "Free/ÄMTER"}));
    // A section in any ASCII case is the same section to a path.
    let read = ok(&core, "section_read", json!({"target": "free/Ämter"}));
    assert_eq!(read["section"], "Free/Ämter", "{read}");
}

#[test]
fn a_path_written_before_names_could_hold_a_separator_reads_as_it_did() {
    let core = core();
    for (target, path) in [
        ("Schemas:Customer.name", "Schemas:Customer.name"),
        (" Schemas : Customer . name ", "Schemas:Customer.name"),
        ("Docs:Spec", "Docs:Spec"),
        ("Charts/Shop:Checkout.start", "Charts/Shop:Checkout.start"),
    ] {
        let read = ok(&core, "card_read", json!({"target": target}));
        assert_eq!(read["path"], path, "{target}: {read}");
    }
}

/// What the exports would write now (`export_files`): by `<export>/<path>`, and the problems.
fn exported(core: &Core) -> (serde_json::Map<String, Value>, Vec<String>) {
    let project = common::project_id(core);
    let said = common::op(core, json!({"op": "export_files", "project": project}));
    let files = said["result"]["files"]
        .as_object()
        .cloned()
        .unwrap_or_default();
    let problems = said["result"]["problems"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|p| p.as_str().unwrap().to_string())
        .collect();
    (files, problems)
}

#[test]
fn what_is_written_out_is_named_from_the_names_as_each_place_may_hold_them() {
    let core = core();
    ok(
        &core,
        "card_create",
        json!({"value": {
            "Schemas:'Order.v2'": {"kind": "entity", "fields": [
                {"name": "'user profile'", "type": "string"}, {"name": "e-mail", "type": "string"}
            ]},
            "Schemas:'Ship to'": {"kind": "value", "fields": [{"name": "city", "type": "string"}]},
            "Schemas:Shipment": {"kind": "entity", "fields": [{"name": "address", "type": "string"}],
                "connections": [{"line": "embed", "from": "address", "to": "Schemas:'Ship to'"}]}
        }}),
    );
    let (files, problems) = exported(&core);
    assert!(problems.is_empty(), "{problems:?}");
    let text = |key: &str| files[key].as_str().unwrap().to_string();
    // A type's name of the name's letters and digits; a key that is no identifier, quoted.
    let ts = text("ts/types.ts");
    assert!(ts.contains("export interface OrderV2 {"), "{ts}");
    assert!(ts.contains("\"user profile\"?: string;"), "{ts}");
    assert!(ts.contains("\"e-mail\"?: string;"), "{ts}");
    assert!(ts.contains("address?: ShipTo;"), "{ts}");
    let zod = text("zod/types.zod.ts");
    assert!(
        zod.contains("export const OrderV2 = z.strictObject({"),
        "{zod}"
    );
    // A $ref to a name of a space: as a URI's fragment writes it.
    let schema: Value = serde_json::from_str(&text("schema/types.schema.json")).unwrap();
    assert_eq!(
        schema["$defs"]["Shipment"]["properties"]["address"]["$ref"],
        "#/$defs/Ship%20to"
    );
    assert!(schema["$defs"]["Order.v2"]["properties"]["user profile"].is_object());

    // Two cards of one type's name in one file: said, and the file not written.
    ok(
        &core,
        "card_create",
        json!({"target": "Schemas:'Ship.to'", "value": {"kind": "value"}}),
    );
    let (files, problems) = exported(&core);
    assert!(!files.contains_key("ts/types.ts"));
    assert!(
        problems.iter().any(|p| p.contains("ShipTo")
            && p.contains("Schemas:'Ship to'")
            && p.contains("Schemas:'Ship.to'")),
        "{problems:?}"
    );
    ok(&core, "card_delete", json!({"target": "Schemas:'Ship.to'"}));

    // A document's file: what a file's name cannot hold put as `_`, two that come to one said.
    ok(
        &core,
        "card_create",
        json!({"value": {
            "Docs:'release notes.md'": {"kind": "freedoc", "body": "x"},
            "Docs:'a/b'": {"kind": "freedoc", "body": "x"},
            "Docs:'a:b'": {"kind": "freedoc", "body": "x"}
        }}),
    );
    let (files, problems) = exported(&core);
    assert!(
        files.contains_key("docs/notes/release notes.md.md"),
        "{:?}",
        files.keys()
    );
    assert!(
        problems
            .iter()
            .any(|p| p.contains("notes/a_b.md") && p.contains("Docs:'a/b'")),
        "{problems:?}"
    );
    ok(&core, "card_delete", json!({"target": "Docs:'a:b'"}));
    // Two of names apart in ASCII case alone, one file on a Mac: said.
    ok(
        &core,
        "card_create",
        json!({"target": "Free:'A/B'", "value": {"kind": "freedoc", "body": "y"}}),
    );
    common::op(
        &core,
        json!({"op": "mcp_writes_set", "project": common::project_id(&core), "group": "export", "state": "allow"}),
    );
    ok(
        &core,
        "export_create",
        json!({"target": "both", "value": {"entries": [
            {"from": "Docs", "to": "all/", "kinds": ["freedoc"]},
            {"from": "Free", "to": "all/", "kinds": ["freedoc"]}
        ]}}),
    );
    let (_, problems) = exported(&core);
    assert!(
        problems
            .iter()
            .any(|p| p.contains("all/a_b.md") && p.contains("all/A_B.md")),
        "{problems:?}"
    );
}

#[test]
fn a_diagram_s_ids_are_told_apart_and_its_labels_say_the_names() {
    let core = core();
    for name in ["注文", "顧客"] {
        ok(
            &core,
            "card_create",
            json!({"target": format!("Schemas:{name}"), "value": {"kind": "entity", "fields": [
                {"name": "f(x)", "type": "string"}
            ]}}),
        );
    }
    let read = ok(
        &core,
        "section_read",
        json!({"target": "Schemas", "value": {"as": "diagram", "fields": true}}),
    );
    let drawn = read["mermaid"].as_str().unwrap();
    assert!(
        drawn.contains("class Schemas____[\"Schemas:注文\"]"),
        "{drawn}"
    );
    assert!(
        drawn.contains("class Schemas_____2[\"Schemas:顧客\"]"),
        "{drawn}"
    );
    assert!(drawn.contains("Schemas____ : string f（x）"), "{drawn}");
    for node in ["'pay now'", "pay_now"] {
        ok(
            &core,
            "card_create",
            json!({"target": format!("Charts/Shop:Checkout.{node}"), "value": {"kind": "process"}}),
        );
    }
    let read = ok(
        &core,
        "card_read",
        json!({"target": "Charts/Shop:Checkout", "value": {"as": "diagram", "shape": "flowchart"}}),
    );
    let drawn = read["mermaid"].as_str().unwrap();
    assert!(
        drawn.contains("n_pay_now[") && drawn.contains("n_pay_now_2["),
        "{drawn}"
    );
}
