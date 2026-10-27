//! A card's lines as its connections: each of a kind of line the project defines, written with
//! `connections` on the card, read with `card_read`, held to its kind by the core's writes,
//! followed through a rename or a move, merged line by line, written out as its kind says; the
//! kinds' definitions read and written as the project's settings; a store, a branch and a
//! project file of before moved into them once.
//!
//! Over the sample project (`見本`), moved: Schemas/Ordering:Order embeds Schemas:Status
//! (`status`) and Schemas/Ordering:Line (`lines`), points at Schemas:Customer by id
//! (`customer_id`), and signs with Schemas:Customer.id (`signedBy`, a line of the project's own
//! the move made of its payload); Schemas:Orders holds Order.

use crate::common;

use common::{call, checked, line_edits, ok, op, project_id, refused, script_store};
use serde_json::{Value, json};
use specboard_core::Core;

const PROJECT: &str = "見本";

fn read(core: &Core, target: &str) -> Value {
    ok(core, "card_read", json!({"target": target}))
}

/// A card's lines as it writes them.
fn lines(core: &Core, card: &str) -> Vec<Value> {
    read(core, card)["connections"]
        .as_array()
        .cloned()
        .unwrap_or_default()
}

/// A card's lines with `more` after them: added (`card_update`'s `{add, remove}`).
fn with(card: &str, more: &[Value]) -> Value {
    json!({"target": card, "value": {"connections": {"add": more}}})
}

/// A card's lines written as `after`, as `card_update` writes them: what goes from the lines
/// it has to those.
fn whole(core: &Core, card: &str, after: &[Value]) -> Value {
    json!({"target": card, "value": {"connections": line_edits(&lines(core, card), after)}})
}

/// The whole project as the window reads it.
fn detail(core: &Core, branch: &str) -> Value {
    op(
        core,
        json!({"op": "project_detail", "project": project_id(core), "branch": branch}),
    )["result"]
        .clone()
}

fn schema<'d>(detail: &'d Value, name: &str) -> &'d Value {
    detail["schemas"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == name)
        .unwrap()
}

fn field<'c>(card: &'c Value, name: &str) -> &'c Value {
    card["fields"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["name"] == name)
        .unwrap()
}

/// The Order's lines as the move made them.
fn order_lines() -> Value {
    json!([
        { "line": "embed", "from": "status", "to": "Schemas:Status" },
        { "line": "embed", "from": "lines", "to": "Schemas/Ordering:Line" },
        { "line": "points", "from": "customer_id", "to": "Schemas:Customer", "fields": { "cardinality": "N:1" } },
        { "line": "signedBy", "to": "Schemas:Customer", "at": "id", "fields": { "excludes": ["Schemas/Ordering:Order.kind"], "payload": "approval" } }
    ])
}

// MARK: - Read

#[test]
fn a_cards_lines_read_as_written_and_in_the_keys_the_screen_read_before() {
    let core = Core::open(script_store()).unwrap();
    assert_eq!(
        Value::Array(lines(&core, "Schemas/Ordering:Order")),
        order_lines()
    );
    // An embedding's field reads as `ref`; what it embeds is in its line.
    let order = read(&core, "Schemas/Ordering:Order");
    assert_eq!(field(&order, "status")["type"], "ref");
    assert!(field(&order, "status").get("ref").is_none(), "{order}");
    assert!(field(&order, "customer_id").get("pointsAt").is_none());
    assert_eq!(
        lines(&core, "Schemas:Orders"),
        [json!({ "line": "holds", "to": "Schemas/Ordering:Order" })]
    );

    // The window's whole project: the lines as kept, and every line a relation. The keys of
    // before (a field's `refSchemaId` and `pointsAt`, a card's `payloads` and `holds`) are
    // not there.
    let whole = detail(&core, "");
    let order = schema(&whole, "Order");
    let status_id = schema(&whole, "Status")["id"].clone();
    let customer_id = schema(&whole, "Customer")["id"].clone();
    assert_eq!(order["connections"].as_array().unwrap().len(), 4);
    assert!(order["connections"][0]["id"].is_string());
    assert_eq!(order["connections"][0]["to"], status_id);
    assert_eq!(field(order, "status")["type"], "ref");
    for key in ["refSchemaId", "pointsAt"] {
        assert!(field(order, "status").get(key).is_none(), "{order}");
        assert!(field(order, "customer_id").get(key).is_none(), "{order}");
    }
    assert!(order.get("payloads").is_none(), "{order}");
    assert!(schema(&whole, "Orders").get("holds").is_none());
    // Every line a relation: its kind, its row, where it lands, the line it is.
    let from_order: Vec<(String, String)> = whole["relations"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["fromSchemaId"] == order["id"])
        .map(|r| {
            assert!(
                r.get("legacy").is_none() && r.get("refKind").is_none(),
                "{r}"
            );
            assert!(
                r.get("refTarget").is_none() && r.get("path").is_none(),
                "{r}"
            );
            (
                r["line"].as_str().unwrap().to_string(),
                r["from"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    assert_eq!(
        from_order,
        [
            ("embed".to_string(), "status".to_string()),
            ("embed".to_string(), "lines".to_string()),
            ("points".to_string(), "customer_id".to_string()),
            ("signedBy".to_string(), String::new()),
        ]
    );
    let lines_of_order: Vec<&Value> = order["connections"].as_array().unwrap().iter().collect();
    let pointer = whole["relations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["line"] == "points")
        .unwrap();
    assert_eq!(pointer["toSchemaId"], customer_id);
    assert_eq!(pointer["cardinality"], "N:1");
    assert_eq!(pointer["isArray"], false);
    assert!(pointer["fieldId"].is_string());
    assert_eq!(pointer["connection"], lines_of_order[2]["id"]);
    let signed = whole["relations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["line"] == "signedBy")
        .unwrap();
    assert_eq!(signed["toSchemaId"], customer_id);
    assert_eq!(signed["at"], "id");
    assert!(signed.get("fieldId").is_none());
    // The kinds of line and of section it has: the built-in ones, then its own.
    let kinds: Vec<&str> = whole["kinds"]["lines"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["line"].as_str().unwrap())
        .collect();
    assert_eq!(
        kinds,
        [
            "embed",
            "points",
            "link",
            "holds",
            "shapeOf",
            "rules",
            "implementedBy",
            "mentions",
            "next",
            "condition",
            "else",
            "onError",
            "onTimeout",
            "runs",
            "awaits",
            "reads",
            "writes",
            "partner",
            "key",
            "signedBy"
        ]
    );
    assert_eq!(whole["kinds"]["lines"][0]["builtin"], true);
    let own = whole["kinds"]["lines"]
        .as_array()
        .unwrap()
        .iter()
        .find(|l| l["line"] == "signedBy")
        .unwrap();
    assert!(own.get("builtin").is_none());
    assert_eq!(whole["kinds"]["sections"].as_array().unwrap().len(), 12);
    assert_eq!(whole["project"]["kinds"]["lines"][0]["line"], "signedBy");
}

// MARK: - Written

#[test]
fn lines_are_written_whole_and_held_to_their_kinds() {
    let core = Core::open(script_store()).unwrap();
    let card = "Schemas/Ordering:Order";
    let written = ok(
        &core,
        "card_update",
        with(
            card,
            &[
                json!({"line": "link", "to": "Docs:Spec", "description": "仕様"}),
                json!({"line": "points", "from": "total", "to": "Schemas:Customer", "at": "Schemas:Customer.id"}),
            ],
        ),
    );
    assert!(written.get("warnings").is_none(), "{written}");
    let now = lines(&core, card);
    assert_eq!(now.len(), 6);
    assert_eq!(
        now[4],
        json!({"line": "link", "to": "Docs:Spec", "description": "仕様"})
    );
    // A landing written in full in the card it reaches is its names there.
    assert_eq!(
        now[5],
        json!({"line": "points", "from": "total", "to": "Schemas:Customer", "at": "id"})
    );
    // A line from the head is said by the card alone.
    let referred = read(&core, "Docs:Spec")["referencedBy"].clone();
    assert!(
        referred
            .as_array()
            .unwrap()
            .contains(&json!("Schemas/Ordering:Order")),
        "{referred}"
    );

    // What does not fit its kind of line is refused, and nothing is written.
    let each = |line: Value| refused(&core, "card_update", with(card, &[line]));
    let said = each(json!({"line": "points", "to": "Schemas:Customer"}));
    assert!(
        said.contains("この線は entity・valueの欄 から出せます（entity の頭からは出せません）"),
        "{said}"
    );
    let said = each(json!({"line": "embed", "from": "kind", "to": "Charts/Shop:Checkout"}));
    assert!(
        said.contains("相手は entity・value・enumの頭 です（Charts/Shop:Checkout は flow）"),
        "{said}"
    );
    let said = each(json!({"line": "embed", "from": "status", "to": "Schemas:Customer"}));
    assert!(
        said.contains("内包（embed）は 1 つの所から 1 本までです（status から 2 本）"),
        "{said}"
    );
    let said = each(json!({"line": "holds", "to": "Schemas:Customer"}));
    assert!(said.contains("この線は storeの頭 から出せます"), "{said}");
    let said = each(json!({"line": "sideways", "to": "Schemas:Customer"}));
    assert!(
        said.contains("「sideways」という線の種類はありません"),
        "{said}"
    );
    let said = each(json!({"line": "link", "to": "Schemas:Nobody"}));
    assert!(
        said.contains("「Schemas:Nobody」というカードはありません"),
        "{said}"
    );
    let said = each(json!({"line": "link", "from": "nope", "to": "Schemas:Customer"}));
    assert!(
        said.contains("出元の「nope」は Schemas/Ordering:Order にありません"),
        "{said}"
    );
    let said = each(
        json!({"line": "points", "from": "total", "to": "Schemas:Customer", "fields": {"cardinality": "many"}}),
    );
    assert!(said.contains("\"many\" は選べません"), "{said}");
    let said = each(json!({"line": "mentions", "to": "Schemas:Customer"}));
    assert!(said.contains("connections には書きません"), "{said}");
    // Written whole or not at all: a line that fits, beside one that does not, is not written either.
    let said = refused(
        &core,
        "card_update",
        with(
            card,
            &[
                json!({"line": "link", "to": "Docs:Overview"}),
                json!({"line": "sideways", "to": "Schemas:Customer"}),
            ],
        ),
    );
    assert!(
        said.contains("「sideways」という線の種類はありません"),
        "{said}"
    );
    assert_eq!(lines(&core, card).len(), 6);

    // Made with its lines, the cards of the call reaching each other.
    ok(
        &core,
        "card_create",
        json!({"value": {
            "Schemas:Pair": {"kind": "entity",
                "fields": [{"name": "half", "type": "string"}],
                "connections": [{"line": "embed", "from": "half", "to": "Schemas:Half"}]},
            "Schemas:Half": {"kind": "value"}
        }}),
    );
    // Held to its kind against the project with every card of the call in it.
    let said = refused(
        &core,
        "card_create",
        json!({"value": {
            "Schemas:Odd": {"kind": "entity",
                "fields": [{"name": "it", "type": "string"}],
                "connections": [{"line": "embed", "from": "it", "to": "Charts/Shop:Wrong"}]},
            "Charts/Shop:Wrong": {"kind": "flow", "nodes": [{"name": "s", "kind": "begin", "trigger": "manual", "next": ["e"]}, {"name": "e", "kind": "end", "outcome": "success"}]}
        }}),
    );
    assert!(said.contains("（Charts/Shop:Wrong は flow）"), "{said}");
    let pair = read(&core, "Schemas:Pair");
    assert_eq!(field(&pair, "half")["type"], "ref");
    assert_eq!(
        pair["connections"],
        json!([{ "line": "embed", "from": "half", "to": "Schemas:Half" }])
    );
}

#[test]
fn the_keys_of_before_are_refused_with_how_to_write_lines_now() {
    let core = Core::open(script_store()).unwrap();
    for (target, value) in [
        ("Schemas/Ordering:Order.kind", json!({"type": "ref"})),
        (
            "Schemas/Ordering:Order.kind",
            json!({"ref": "Schemas:Status"}),
        ),
        (
            "Schemas/Ordering:Order.kind",
            json!({"pointsAt": [{"schema": "Schemas:Status", "as": "id"}]}),
        ),
        ("Schemas:Orders", json!({"holds": ["Schemas:Customer"]})),
        ("Charts/Shop:Checkout", json!({"notes": ["Docs:Spec"]})),
        ("Schemas/Ordering:Order", json!({"payloads": []})),
    ] {
        let said = refused(
            &core,
            "card_update",
            json!({"target": target, "value": value}),
        );
        assert!(
            said.contains("は使いません") || said.contains("型 ref は書けません"),
            "{target} {value}: {said}"
        );
        assert!(said.contains("connections"), "{said}");
    }
    let said = refused(
        &core,
        "card_create",
        json!({"target": "Schemas:Old", "value": {"kind": "entity", "fields": [{"name": "a", "type": "ref", "ref": "Schemas:Status"}]}}),
    );
    assert!(said.contains("line\": \"embed\""), "{said}");
    let said = refused(
        &core,
        "card_create",
        json!({"target": "Schemas/Ordering:Order.extra", "value": {"type": "string", "pointsAt": []}}),
    );
    assert!(said.starts_with("pointsAt は使いません"), "{said}");
}

#[test]
fn an_embedded_fields_type_is_its_line() {
    let core = Core::open(script_store()).unwrap();
    let said = refused(
        &core,
        "card_update",
        json!({"target": "Schemas/Ordering:Order.status", "value": {"type": "int"}}),
    );
    assert!(
        said.contains("内包の線があるので、型は相手のカードです"),
        "{said}"
    );
    // The line taken off: a string again.
    let kept: Vec<Value> = lines(&core, "Schemas/Ordering:Order")
        .into_iter()
        .filter(|l| l["from"] != "status")
        .collect();
    ok(
        &core,
        "card_update",
        whole(&core, "Schemas/Ordering:Order", &kept),
    );
    let order = read(&core, "Schemas/Ordering:Order");
    assert_eq!(field(&order, "status")["type"], "string");
    ok(
        &core,
        "card_update",
        json!({"target": "Schemas/Ordering:Order.status", "value": {"type": "int"}}),
    );
}

// MARK: - Followed

#[test]
fn a_rename_or_a_move_is_followed_by_the_lines() {
    let core = Core::open(script_store()).unwrap();
    let order = "Schemas/Ordering:Order";
    ok(
        &core,
        "card_update",
        with(
            order,
            &[json!({"line": "points", "from": "total", "to": "Schemas:Customer", "at": "id"})],
        ),
    );
    // The row a line leaves, renamed.
    ok(
        &core,
        "card_update",
        json!({"target": "Schemas/Ordering:Order.customer_id", "value": {"name": "buyer_id"}}),
    );
    // Where lines land, renamed in the card they reach.
    ok(
        &core,
        "card_update",
        json!({"target": "Schemas:Customer.id", "value": {"name": "cid"}}),
    );
    // A path a line's field holds, renamed.
    ok(
        &core,
        "card_update",
        json!({"target": "Schemas/Ordering:Order.kind", "value": {"name": "sort"}}),
    );
    // The card they reach renamed and moved: held by id, they follow by themselves.
    ok(
        &core,
        "card_update",
        json!({"target": "Schemas:Customer", "value": {"name": "Client", "section": "Schemas/Ordering"}}),
    );
    assert_eq!(
        Value::Array(lines(&core, order)),
        json!([
            { "line": "embed", "from": "status", "to": "Schemas:Status" },
            { "line": "embed", "from": "lines", "to": "Schemas/Ordering:Line" },
            { "line": "points", "from": "buyer_id", "to": "Schemas/Ordering:Client", "fields": { "cardinality": "N:1" } },
            { "line": "signedBy", "to": "Schemas/Ordering:Client", "at": "cid", "fields": { "excludes": ["Schemas/Ordering:Order.sort"], "payload": "approval" } },
            { "line": "points", "from": "total", "to": "Schemas/Ordering:Client", "at": "cid" }
        ])
    );
    let broken = checked(&core, "main")["brokenPaths"].clone();
    let via: Vec<&str> = broken
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["via"].as_str().unwrap())
        .collect();
    assert!(!via.iter().any(|v| v.contains("connections")), "{broken}");
}

#[test]
fn a_field_gone_takes_its_lines_and_a_card_gone_leaves_them_broken() {
    let core = Core::open(script_store()).unwrap();
    ok(
        &core,
        "card_delete",
        json!({"target": "Schemas/Ordering:Order.customer_id"}),
    );
    let left: Vec<String> = lines(&core, "Schemas/Ordering:Order")
        .iter()
        .map(|l| l["line"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(left, ["embed", "embed", "signedBy"]);
    // The card a line reaches gone: the line stays, said as broken, written back as read.
    ok(&core, "card_delete", json!({"target": "Schemas:Customer"}));
    let now = lines(&core, "Schemas/Ordering:Order");
    assert_eq!(now[2]["to"], Value::Null);
    let broken = checked(&core, "main")["brokenPaths"].clone();
    let line = broken
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["via"] == "connections.signedBy.to")
        .unwrap_or_else(|| panic!("{broken}"));
    assert_eq!(line["at"], "Schemas/Ordering:Order");
    // Said by the path the card had.
    assert_eq!(line["path"], "Schemas:Customer", "{broken}");
    assert!(
        line["hint"]
            .as_str()
            .unwrap()
            .contains("相手のカードがありません")
    );
    // Written back as read: nothing to add or take away.
    ok(
        &core,
        "card_update",
        whole(&core, "Schemas/Ordering:Order", &now),
    );
    assert_eq!(lines(&core, "Schemas/Ordering:Order").len(), 3);
}

// MARK: - The kinds' definitions

#[test]
fn mentions_are_left_out_of_a_diagram_unless_asked() {
    // The lines a text's [[ ]] names are the most of any kind: drawn only when asked for.
    let core = Core::open(script_store()).unwrap();
    let read = ok(&core, "kind_read", json!({}));
    let mentions = read["lines"]
        .as_array()
        .unwrap()
        .iter()
        .find(|l| l["line"] == "mentions")
        .unwrap();
    assert_eq!(mentions["visible"], false, "{mentions}");
    ok(
        &core,
        "card_update",
        json!({"target": "Free:Memo", "value": {"body": "決めること: [[Schemas/Ordering:Order]] の返金"}}),
    );
    let diagram = |value: Value| {
        ok(
            &core,
            "section_read",
            json!({"target": "Free", "value": value}),
        )["mermaid"]
            .as_str()
            .unwrap()
            .to_string()
    };
    let plain = diagram(json!({"as": "diagram"}));
    assert!(!plain.contains("mentions"), "{plain}");
    let asked = diagram(json!({"as": "diagram", "kinds": ["mentions"]}));
    assert!(asked.contains(": mentions"), "{asked}");
}

#[test]
fn a_contents_map_leaves_out_hidden_kinds_unless_asked() {
    let core = Core::open(script_store()).unwrap();
    ok(
        &core,
        "kind_create",
        json!({"target": "aside", "value": {"of": "line", "label": "余談", "base": "link", "visible": false,
            "from": [{"kinds": ["task"], "at": "card"}], "to": {"kinds": ["task"], "at": ["card"]}}}),
    );
    let board = |tool: &str, args: Value| {
        let mut args = args;
        args["branch"] = json!("_project_board_");
        ok(&core, tool, args)
    };
    board(
        "card_create",
        json!({"target": "Goals:Plan", "value": {"kind": "goal"}}),
    );
    board(
        "card_create",
        json!({"target": "Goals:Plan.A", "value": {"kind": "task"}}),
    );
    board(
        "card_create",
        json!({"target": "Goals:Plan.B", "value": {"kind": "task",
            "connections": [{"line": "aside", "to": "Goals:Plan.A"}]}}),
    );
    let diagram = |value: Value| {
        board("card_read", json!({"target": "Goals:Plan", "value": value}))["mermaid"]
            .as_str()
            .unwrap()
            .to_string()
    };
    let plain = diagram(json!({"as": "diagram"}));
    assert!(!plain.contains("余談"), "{plain}");
    let asked = diagram(json!({"as": "diagram", "kinds": ["aside"]}));
    assert!(asked.contains("余談"), "{asked}");
}

#[test]
fn the_projects_own_kinds_are_read_and_written_and_one_in_use_stays() {
    let core = Core::open(script_store()).unwrap();
    let read = ok(&core, "kind_read", json!({}));
    let names: Vec<(&str, bool)> = read["lines"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| (l["line"].as_str().unwrap(), l["builtin"] == true))
        .collect();
    assert_eq!(
        names,
        [
            ("embed", true),
            ("points", true),
            ("link", true),
            ("holds", true),
            ("shapeOf", true),
            ("rules", true),
            ("implementedBy", true),
            ("mentions", true),
            ("next", true),
            ("condition", true),
            ("else", true),
            ("onError", true),
            ("onTimeout", true),
            ("runs", true),
            ("awaits", true),
            ("reads", true),
            ("writes", true),
            ("partner", true),
            ("key", true),
            ("signedBy", false)
        ]
    );
    let all = ok(&core, "kind_read", json!({}));
    // Every built-in kind of card but the abstract ones (the sample has none of its own) and the
    // frame, which goes only in a ui section and the sample has none.
    let builtin: Value = serde_json::from_str(include_str!("../src/kinds/builtin.json")).unwrap();
    let mut want: Vec<&str> = builtin["kinds"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|k| k["abstract"] != true && k["kind"] != "frame")
        .map(|k| k["kind"].as_str().unwrap())
        .collect();
    let mut got: Vec<&str> = all["cards"]
        .as_array()
        .unwrap()
        .iter()
        .map(|k| k["kind"].as_str().unwrap())
        .collect();
    want.sort();
    got.sort();
    assert_eq!(got, want);

    // The line the project uses stays.
    let said = common::set_kinds(&core, PROJECT, json!({"lines": []})).unwrap_err();
    // main's Order, and the branch's copy of it.
    assert!(
        said.contains("線の種類「signedBy」は 2 か所で使われているので消せません（Order）"),
        "{said}"
    );
    let signed = read["lines"]
        .as_array()
        .unwrap()
        .iter()
        .find(|l| l["line"] == "signedBy")
        .unwrap()
        .clone();
    // A line of its own, written in the comments; a section of its own.
    let reason = json!({ "line": "根拠", "label": "根拠", "base": "link",
        "from": [{ "kinds": ["entity", "value"], "at": "card" }],
        "to": { "kinds": ["*"], "at": ["card", "row"] },
        "export": { "as": "comment" } });
    let keys = json!({ "section": "keys", "label": "鍵", "holds": ["entity", "note"] });
    common::set_kinds(
        &core,
        PROJECT,
        json!({"lines": [signed.clone(), reason.clone()], "sections": [keys]}),
    )
    .unwrap();
    assert_eq!(
        ok(&core, "kind_read", json!({"target": "根拠"}))["of"],
        "line"
    );
    let bad = common::set_kinds(
        &core,
        PROJECT,
        json!({"lines": [signed.clone(), {"line": "points", "base": "link", "from": [], "to": {"kinds": ["*"]}}]}),
    )
    .unwrap_err();
    assert!(bad.contains("もうあります（組み込みの線の種類）"), "{bad}");

    ok(
        &core,
        "card_update",
        with(
            "Schemas:Customer",
            &[json!({"line": "根拠", "to": "Docs:Spec", "description": "仕様の根拠"})],
        ),
    );
    let ts = ok(
        &core,
        "export_read",
        json!({"targetdir": "[default]", "target": "ts", "value": {"files": ["customer.ts"]}}),
    );
    assert!(
        ts["files"][0]["text"]
            .as_str()
            .unwrap()
            .contains("仕様の根拠 → Spec"),
        "{ts}"
    );
    // One written as an annotation: in JSON Schema's `x-` too, on the field it leaves.
    let evidence = json!({ "line": "証拠", "base": "designate",
        "from": [{ "kinds": ["entity"], "at": "row" }],
        "to": { "kinds": ["*"], "at": ["card", "row"] },
        "fields": [{ "name": "strength", "fieldtype": "dropdownlist", "options": ["弱", "強"] }],
        "export": { "as": "annotation", "key": "x-evidence" } });
    common::set_kinds(
        &core,
        PROJECT,
        json!({"lines": [signed.clone(), reason.clone(), evidence.clone()]}),
    )
    .unwrap();
    ok(
        &core,
        "card_update",
        with(
            "Schemas:Customer",
            &[
                json!({"line": "証拠", "from": "name", "to": "Schemas/Ordering:Order", "at": "id", "fields": {"strength": "強"}}),
            ],
        ),
    );
    // Written into a JSON Schema of its own instead (the project's exports).
    let set = op(
        &core,
        json!({"op": "mcp_writes_set", "project": project_id(&core), "item": "export_update", "state": "allow"}),
    );
    assert!(set.get("error").is_none(), "{set}");
    ok(
        &core,
        "export_update",
        json!({"target": "ts", "value": {"entries": {
            "remove": [{"from": "Schemas:Customer"}],
            "add": [{"from": "Schemas:Customer", "to": "customer.schema.json"}],
        }}}),
    );
    let schema: Value = serde_json::from_str(
        ok(
            &core,
            "export_read",
            json!({"targetdir": "[default]", "target": "ts", "value": {"files": ["customer.schema.json"]}}),
        )["files"][0]["text"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    let name = &schema["$defs"]["Customer"]["properties"]["name"];
    assert_eq!(
        name["x-evidence"],
        json!([{ "schema": "Order", "field": "id", "fields": { "strength": "強" } }]),
        "{schema}"
    );
    assert_eq!(name["description"], "証拠 → Order.id");
    let customer = &schema["$defs"]["Customer"];
    assert!(
        customer["description"]
            .as_str()
            .unwrap()
            .contains("仕様の根拠 → Spec"),
        "{customer}"
    );

    // Changed so the line no longer fits: said by the check, not refused.
    let mut narrower = reason.clone();
    narrower["from"] = json!([{ "kinds": ["value"], "at": "card" }]);
    common::set_kinds(
        &core,
        PROJECT,
        json!({"lines": [signed, narrower, evidence]}),
    )
    .unwrap();
    let problems = checked(&core, "main")["lineProblems"].clone();
    assert_eq!(problems[0]["at"], "Schemas:Customer");
    assert!(
        problems[0]["problem"]
            .as_str()
            .unwrap()
            .contains("（entity の頭からは出せません）"),
        "{problems}"
    );

    // A section of the project's own kind directly under the root, put there by the project
    // (`project_tree`): holds what it says; taken out again once empty.
    let read = ok(&core, "project_read", json!({}));
    let before = read["projectTree"].clone();
    let mut tree = before.clone();
    tree["main_sections"]
        .as_array_mut()
        .unwrap()
        .push(json!({"type": "keys", "name": "Keys"}));
    let made = ok(
        &core,
        "project_update",
        json!({"value": {"project_tree": tree}}),
    );
    assert_eq!(made["projectTree"][0]["create"], json!(["Keys"]), "{made}");
    ok(
        &core,
        "card_create",
        json!({"target": "Keys:Signing", "value": {"kind": "entity"}}),
    );
    let said = refused(
        &core,
        "card_create",
        json!({"target": "Keys:Flow", "value": {"kind": "flow"}}),
    );
    assert!(
        said.contains("鍵セクションなので flow は置けません"),
        "{said}"
    );
    let said = refused(&core, "section_create", json!({"target": "Elsewhere"}));
    assert!(
        said.contains("プロジェクトの project_tree で決まり"),
        "{said}"
    );
    // Changed so it no longer holds what is in it: said by the check, not refused.
    common::set_kinds(
        &core,
        PROJECT,
        json!({"sections": [{ "section": "keys", "label": "鍵", "holds": ["note"] }]}),
    )
    .unwrap();
    let problems = checked(&core, "main")["sectionProblems"].clone();
    assert_eq!(problems.as_array().unwrap().len(), 1, "{problems}");
    assert_eq!(problems[0]["at"], "Keys:Signing", "{problems}");
    assert_eq!(
        problems[0]["problem"], "鍵セクションは entity を入れません（入れるのは note）",
        "{problems}"
    );
    ok(
        &core,
        "card_update",
        json!({"target": "Keys:Signing", "value": {"section": "Schemas"}}),
    );
    let gone = ok(
        &core,
        "project_update",
        json!({"value": {"project_tree": before}}),
    );
    assert_eq!(gone["projectTree"][0]["delete"], json!(["Keys"]), "{gone}");
}

// MARK: - A branch

#[test]
fn a_branch_and_main_each_add_a_line_and_the_merge_takes_both() {
    let core = Core::open(script_store()).unwrap();
    let order = "Schemas/Ordering:Order";
    ok(&core, "branch_create", json!({"target": "work"}));
    let mut in_branch = with(order, &[json!({"line": "link", "to": "Docs:Spec"})]);
    in_branch["branch"] = "work".into();
    ok(&core, "card_update", in_branch);
    ok(
        &core,
        "card_update",
        with(order, &[json!({"line": "link", "to": "Docs:Overview"})]),
    );
    let diff = ok(&core, "branch_read", json!({"target": "work"})).to_string();
    assert!(diff.contains("線を足す link → Docs:Spec"), "{diff}");
    ok(
        &core,
        "branch_merge",
        json!({"target": "work", "value": {"into": "main"}}),
    );
    let tos: Vec<String> = lines(&core, order)
        .iter()
        .filter(|l| l["line"] == "link")
        .map(|l| l["to"].as_str().unwrap().to_string())
        .collect();
    // Each after the line it followed on its side: the branch's, then main's.
    assert_eq!(tos, ["Docs:Spec", "Docs:Overview"]);
}

// MARK: - A designating line lands inside the card it reaches

/// A line that designates lands inside the card it reaches: its landing written in another
/// card is refused, with how to write where the values are (a line of its own); written in
/// full inside the card, it is its names there. The project's own lines that designate and
/// one told apart by its discriminator alike; the other bases as before.
#[test]
fn a_designating_lines_landing_is_inside_the_card_it_reaches() {
    let core = Core::open(script_store()).unwrap();
    let order = "Schemas/Ordering:Order";
    let mut written = lines(&core, order);
    let i = written.iter().position(|l| l["line"] == "points").unwrap();
    let write = |connections: &[Value]| whole(&core, order, connections);
    // A line by what it is told apart by, wherever it is among the card's.
    let line_of = |line: &str, from: &str| {
        lines(&core, order)
            .into_iter()
            .find(|l| l["line"] == line && l["from"] == from)
            .unwrap_or_else(|| panic!("{line} {from}"))
    };

    // In another card: refused, with the line to write there instead.
    written[i]["at"] = "Schemas/Ordering:Order.kind".into();
    let said = refused(&core, "card_update", write(&written));
    assert!(
        said.contains(
            "customer_id の ID で指す（points）: 着く所（at）は相手 Schemas:Customer の中の所だけです（欄の名前。例: id）。Schemas/Ordering:Order.kind は相手の外です。値のある別のカードへは、別の線（つながる link など）を引いてください: {\"line\":\"link\",\"from\":\"customer_id\",\"to\":\"Schemas/Ordering:Order\",\"at\":\"kind\"}"
        ),
        "{said}"
    );
    // Naming no card: refused alike, with no line to write.
    written[i]["at"] = "Schemas:Nope.id".into();
    let said = refused(&core, "card_update", write(&written));
    assert!(said.contains("Schemas:Nope.id は相手の外です"), "{said}");
    assert!(!said.contains("{\"line\""), "{said}");
    // In full inside the card it reaches: its names there.
    written[i]["at"] = "Schemas:Customer.id".into();
    ok(&core, "card_update", write(&written));
    assert_eq!(
        line_of("points", "customer_id"),
        json!({"line": "points", "from": "customer_id", "to": "Schemas:Customer", "at": "id", "fields": {"cardinality": "N:1"}})
    );
    // Where the values are: a link of its own, from the field to the field.
    written[i].as_object_mut().unwrap().remove("at");
    written.insert(
        i + 1,
        json!({"line": "link", "from": "customer_id", "to": order, "at": "kind", "description": "使ってよい値の一覧"}),
    );
    ok(&core, "card_update", write(&written));
    assert_eq!(line_of("link", "customer_id"), written[i + 1]);

    // A line of the project's own that designates: the same.
    let kinds = ok(&core, "kind_read", json!({}));
    let signed = kinds["lines"]
        .as_array()
        .unwrap()
        .iter()
        .find(|l| l["line"] == "signedBy")
        .unwrap()
        .clone();
    let etag = json!({ "line": "etag", "label": "etag", "base": "designate",
        "from": [{ "kinds": ["entity", "value"], "at": "row" }],
        "to": { "kinds": ["*"], "at": ["card", "row"] } });
    common::set_kinds(&core, PROJECT, json!({"lines": [signed, etag]})).unwrap();
    let mut own = written.clone();
    own.push(json!({"line": "etag", "from": "customer_id", "to": "Schemas:Customer", "at": "Schemas/Ordering:Order.kind"}));
    let said = refused(&core, "card_update", write(&own));
    assert!(
        said.contains(
            "customer_id の etag（etag）: 着く所（at）は相手 Schemas:Customer の中の所だけです"
        ),
        "{said}"
    );
    // One told apart by its discriminator reaches no card: no landing in full.
    let mut told = written.clone();
    told.push(json!({"line": "points", "from": "kind", "at": "Schemas:Customer.id", "fields": {"discriminatedBy": "kind"}}));
    let said = refused(&core, "card_update", write(&told));
    assert!(
        said.contains("kind の ID で指す（points）: 着く所（at）は相手の中の所だけです"),
        "{said}"
    );
    // The other bases as they were: a key may land in full elsewhere.
    let mut keyed = written.clone();
    let key = keyed.iter().position(|l| l["line"] == "signedBy").unwrap();
    keyed[key]["at"] = "Schemas/Ordering:Order.kind".into();
    ok(&core, "card_update", write(&keyed));
    assert_eq!(
        lines(&core, order)
            .iter()
            .find(|l| l["line"] == "signedBy")
            .unwrap()["at"],
        "Schemas/Ordering:Order.kind"
    );
}

/// What `as: lines` answers keeps to it: drawn to a field of another card, the line reaches
/// that card and lands inside it — written so it is taken, while a pointer to its card
/// landing there is not.
#[test]
fn the_lines_a_place_may_draw_land_inside_the_card_they_reach() {
    let core = Core::open(script_store()).unwrap();
    let order = "Schemas/Ordering:Order";
    let (read, now, full) = drawable(
        &core,
        &format!("{order}.customer_id"),
        json!({"to": "Schemas/Ordering:Order.kind"}),
    );
    assert_eq!(
        (now, full),
        (vec!["points".to_string(), "link".to_string()], vec![])
    );
    assert_eq!(
        (&read["to"], &read["at"]),
        (&json!("Schemas/Ordering:Order"), &json!("kind"))
    );
    // Drawn as it answers: taken.
    ok(
        &core,
        "card_update",
        with(
            order,
            &[json!({"line": "link", "from": "customer_id", "to": read["to"], "at": read["at"]})],
        ),
    );
}

// MARK: - Moved once

#[test]
fn a_project_file_before_12_has_its_lines_moved() {
    let core = Core::open(script_store()).unwrap();
    let project = project_id(&core);
    let before = Value::Array(lines(&core, "Schemas/Ordering:Order"));
    let text = op(&core, json!({"op": "project_file", "project": project}))["result"]["text"]
        .as_str()
        .unwrap()
        .to_string();
    let mut file: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(file["version"], 33);
    // As 11 wrote it: the lines in the cards' fields, no kinds of the project's own.
    file["version"] = 11.into();
    file["project"].as_object_mut().unwrap().remove("kinds");
    let id_of = |file: &Value, name: &str| {
        file["cards"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["name"] == name)
            .unwrap()["id"]
            .clone()
    };
    let (status, line, customer) = (
        id_of(&file, "Status"),
        id_of(&file, "Line"),
        id_of(&file, "Customer"),
    );
    for card in file["cards"].as_array_mut().unwrap() {
        match card["name"].as_str().unwrap() {
            "Order" => {
                card.as_object_mut().unwrap().remove("connections");
                card["fields"]["payloads"] = json!([{ "name": "approval", "excludes": ["Schemas/Ordering:Order.kind"], "signedBy": ["Schemas:Customer.id"] }]);
                for row in card["rows"]["fields"].as_array_mut().unwrap() {
                    let f = &mut row["fields"];
                    match f["name"].as_str().unwrap() {
                        "status" => {
                            f["type"] = "ref".into();
                            f["ref"] = status.clone();
                        }
                        "lines" => {
                            f["type"] = "ref".into();
                            f["ref"] = line.clone();
                        }
                        "customer_id" => {
                            f["pointsAt"] =
                                json!([{ "schema": customer, "as": "id", "cardinality": "N:1" }]);
                        }
                        _ => {}
                    }
                }
            }
            "Orders" => {
                card.as_object_mut().unwrap().remove("connections");
                card["fields"] = json!({ "holds": ["Schemas/Ordering:Order"] });
            }
            _ => {}
        }
    }
    let read_in = op(
        &core,
        json!({"op": "project_file_read", "text": file.to_string(), "landing": "replace"}),
    );
    assert!(read_in.get("error").is_none(), "{read_in}");
    // The same lines as the store's move made (their ids made from what they are).
    assert_eq!(Value::Array(lines(&core, "Schemas/Ordering:Order")), before);
    let whole = detail(&core, "");
    let moved = &schema(&whole, "Order")["connections"];
    let kept: Value = serde_json::from_str::<Value>(&text).unwrap()["cards"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "Order")
        .unwrap()["connections"]
        .clone();
    assert_eq!(moved, &kept, "the same ids");
    assert_eq!(
        lines(&core, "Schemas:Orders"),
        [json!({ "line": "holds", "to": "Schemas/Ordering:Order" })]
    );
    let commits =
        op(&core, json!({"op": "commits", "project": project}))["result"]["commits"].clone();
    let said: Vec<&str> = commits[0]["lines"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["summary"].as_str().unwrap())
        .collect();
    assert_eq!(
        said,
        [
            "線の種類 署名する をプロジェクトに作った（pointsAt の as と payloads の鍵から）",
            "カードの線 5 本を connections に移した（内包 2・ID で指す 1・置く 1・署名する 1）"
        ]
    );
}

/// The id of a card the project has, by its name, in the file's documents.
fn file_card<'f>(file: &'f mut Value, name: &str) -> &'f mut Value {
    file["cards"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|c| c["name"] == name)
        .unwrap()
}

/// What a tree's newest commit says: its name and its records.
fn newest(core: &Core, tree: &str) -> (String, Vec<String>) {
    let conn = rusqlite::Connection::open(core.place()).unwrap();
    let (id, name): (String, String) = conn
        .query_row(
            "SELECT id, name FROM commits WHERE project_id = ? AND branch_id = ? ORDER BY seq DESC LIMIT 1",
            [project_id(core), tree.to_string()],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    let mut stmt = conn
        .prepare("SELECT summary FROM commit_lines WHERE commit_id = ? ORDER BY idx")
        .unwrap();
    let said = stmt
        .query_map([id], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    (name, said)
}

/// How many commits of a tree are named so.
fn commits_named(core: &Core, tree: &str, name: &str) -> i64 {
    rusqlite::Connection::open(core.place())
        .unwrap()
        .query_row(
            "SELECT count(*) FROM commits WHERE project_id = ? AND branch_id = ? AND name = ?",
            [project_id(core), tree.to_string(), name.to_string()],
            |r| r.get(0),
        )
        .unwrap()
}

/// Order's pointer split in two: to the card it names with no landing, and a link from the
/// same field to where its values are.
fn split_in_two() -> [Value; 2] {
    [
        json!({"line": "points", "from": "customer_id", "to": "Schemas:Customer", "fields": {"cardinality": "N:1"}}),
        json!({"line": "link", "from": "customer_id", "to": "Schemas/Ordering:Order", "at": "kind", "description": "使ってよい値の一覧（移し替え）"}),
    ]
}

/// A project file of before 12 whose pointer's values were in another card than the one it
/// names (`in`): two lines, and the record says so.
#[test]
fn a_project_file_before_12_has_a_pointer_landing_elsewhere_split_in_two() {
    let core = Core::open(script_store()).unwrap();
    let project = project_id(&core);
    let text = op(&core, json!({"op": "project_file", "project": project}))["result"]["text"]
        .as_str()
        .unwrap()
        .to_string();
    let mut file: Value = serde_json::from_str(&text).unwrap();
    file["version"] = 11.into();
    let customer = file_card(&mut file, "Customer")["id"].clone();
    let card = file_card(&mut file, "Order");
    card.as_object_mut().unwrap().remove("connections");
    for row in card["rows"]["fields"].as_array_mut().unwrap() {
        let f = &mut row["fields"];
        if f["name"] == "customer_id" {
            // Written in the card's own section.
            f["pointsAt"] = json!([{ "schema": customer, "as": "id", "cardinality": "N:1", "in": "Order.kind" }]);
        }
    }
    let read_in = op(
        &core,
        json!({"op": "project_file_read", "text": file.to_string(), "landing": "replace"}),
    );
    assert!(read_in.get("error").is_none(), "{read_in}");
    assert_eq!(lines(&core, "Schemas/Ordering:Order"), split_in_two());
    let (name, said) = newest(&core, "");
    assert_eq!(name, "線の書き方を移した");
    assert_eq!(
        said,
        [
            "カードの線 2 本を connections に移した（ID で指す 1・つながる 1）",
            "pointsAt の in が相手とは別のカードにあった線 1 本を、相手への線と、値のある所への「つながる」線の 2 本ずつに分けた（Order）"
        ]
    );
    // Nothing more to split once it is in.
    assert_eq!(commits_named(&core, "", "線の着く所を移した"), 0);
}

/// A project file with a pointer landing in another card (the form before a line lands inside the
/// card it reaches): split in two when it is read in, in a commit of one record.
#[test]
fn a_project_file_with_a_pointer_landing_elsewhere_has_it_split_in_two() {
    let core = Core::open(script_store()).unwrap();
    let project = project_id(&core);
    let text = op(&core, json!({"op": "project_file", "project": project}))["result"]["text"]
        .as_str()
        .unwrap()
        .to_string();
    let mut file: Value = serde_json::from_str(&text).unwrap();
    let customer = file_card(&mut file, "Customer")["id"].clone();
    let card = file_card(&mut file, "Order");
    let lines_kept = card["connections"].as_array_mut().unwrap();
    for line in lines_kept.iter_mut() {
        if line["line"] == "points" {
            line["at"] = "Schemas/Ordering:Order.kind".into();
        }
    }
    // One landing in full where there is no card: no line can be made of it.
    lines_kept.push(json!({"id": "l-nowhere", "line": "points", "from": "kind", "to": customer, "at": "Schemas:Nope.id"}));
    let read_in = op(
        &core,
        json!({"op": "project_file_read", "text": file.to_string(), "landing": "replace"}),
    );
    assert!(read_in.get("error").is_none(), "{read_in}");
    let now = lines(&core, "Schemas/Ordering:Order");
    let at = now.iter().position(|l| l["line"] == "points").unwrap();
    assert_eq!(now[at..at + 2], split_in_two());
    assert_eq!(
        now.last().unwrap(),
        &json!({"line": "points", "from": "kind", "to": "Schemas:Customer"})
    );
    let description = read(&core, "Schemas/Ordering:Order")["description"].clone();
    assert!(
        description.as_str().unwrap().ends_with(
            "（線の着く所の移し替え 2026-10-06 で線にできなかったもの）\n- kind から出る線 points の着く所 Schemas:Nope.id"
        ),
        "{description}"
    );
    assert_eq!(
        newest(&core, ""),
        (
            "線の着く所を移した".to_string(),
            vec![
                "着く所が相手の外にあった線 1 本を、相手への線と、値のある所への「つながる」線の 2 本ずつに分けた（Order）"
                    .to_string(),
                "Order の kind から出る線 points の着く所「Schemas:Nope.id」は相手の外で、そこにカードが無いので線にできず、説明の末尾に書き写した"
                    .to_string()
            ]
        )
    );
    // The board had none: nothing added there.
    assert_eq!(commits_named(&core, "~board", "線の着く所を移した"), 0);
}

/// A store with the mark `core_0005_connections`, with
/// a pointer landing in another card in main, in a branch's copy and its base, and in the
/// history: split in two once when it is opened (`core_0007_landings`), alike in each, each
/// tree that had one saying so; opened again, nothing more.
#[test]
fn a_store_moved_before_has_its_landings_elsewhere_split_once() {
    let place = script_store();
    let core = Core::open(&place).unwrap();
    let order = "Schemas/Ordering:Order";
    ok(&core, "branch_create", json!({"target": "work"}));
    let mut in_branch = with(order, &[]);
    in_branch["value"]["description"] = "枝で直した".into();
    in_branch["branch"] = "work".into();
    ok(&core, "card_update", in_branch);
    // A commit in main that kept the card as it was.
    let mut in_main = with(order, &[]);
    in_main["value"]["description"] = "main で直した".into();
    ok(&core, "card_update", in_main);
    let project = project_id(&core);
    let branch: String = rusqlite::Connection::open(&place)
        .unwrap()
        .query_row("SELECT id FROM branches WHERE name = 'work'", [], |r| {
            r.get(0)
        })
        .unwrap();
    core.close();
    drop(core);
    {
        // As a store of before kept it: the pointer landing in Order's own field.
        let conn = rusqlite::Connection::open(&place).unwrap();
        let elsewhere = |text: &str| -> String {
            let mut doc: Value = serde_json::from_str(text).unwrap();
            for line in doc["connections"].as_array_mut().unwrap() {
                if line["line"] == "points" {
                    line["at"] = "Schemas/Ordering:Order.kind".into();
                }
            }
            doc.to_string()
        };
        let rows: Vec<(String, String, Option<String>)> = conn
            .prepare(
                "SELECT branch_id, doc, base FROM card_docs WHERE project_id = ? AND name = 'Order'",
            )
            .unwrap()
            .query_map([&project], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert!(rows.len() >= 2);
        for (tree, doc, base) in rows {
            conn.execute(
                "UPDATE card_docs SET doc = ?, base = ? WHERE project_id = ? AND branch_id = ? AND name = 'Order'",
                rusqlite::params![elsewhere(&doc), base.as_deref().map(elsewhere), project, tree],
            )
            .unwrap();
        }
        let kept: Vec<(String, String, String)> = conn
            .prepare("SELECT r.commit_id, r.row_id, r.before FROM commit_rows r JOIN commits c ON c.id = r.commit_id WHERE c.project_id = ? AND r.tbl = 'card_docs' AND r.before LIKE '%customer_id%'")
            .unwrap()
            .query_map([&project], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert!(!kept.is_empty());
        for (commit, row, before) in kept {
            let mut columns: Value = serde_json::from_str(&before).unwrap();
            let doc = columns["doc"].as_str().unwrap().to_string();
            columns["doc"] = elsewhere(&doc).into();
            conn.execute(
                "UPDATE commit_rows SET before = ? WHERE commit_id = ? AND row_id = ?",
                rusqlite::params![columns.to_string(), commit, row],
            )
            .unwrap();
        }
        conn.execute_batch("DELETE FROM _migrations WHERE name = 'core_0007_landings'")
            .unwrap();
    }

    let core = Core::open(&place).unwrap();
    let said = "着く所が相手の外にあった線 1 本を、相手への線と、値のある所への「つながる」線の 2 本ずつに分けた（Order）";
    for tree in ["", branch.as_str()] {
        let mut asked = json!({"target": order});
        if !tree.is_empty() {
            asked["branch"] = "work".into();
        }
        let now = ok(&core, "card_read", asked)["connections"]
            .as_array()
            .cloned()
            .unwrap();
        let at = now.iter().position(|l| l["line"] == "points").unwrap();
        assert_eq!(now[at..at + 2], split_in_two(), "{tree}");
        assert_eq!(
            newest(&core, tree),
            ("線の着く所を移した".to_string(), vec![said.to_string()])
        );
    }
    assert_eq!(commits_named(&core, "~board", "線の着く所を移した"), 0);
    // Alike in main, the branch and the branch's base: the link the same line.
    let conn = rusqlite::Connection::open(&place).unwrap();
    let docs: Vec<(Option<String>, Option<String>)> = conn
        .prepare("SELECT doc, base FROM card_docs WHERE project_id = ? AND name = 'Order'")
        .unwrap()
        .query_map([&project], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    let link_ids: std::collections::BTreeSet<String> = docs
        .iter()
        .flat_map(|(doc, base)| [doc, base])
        .flatten()
        .map(|t| serde_json::from_str::<Value>(t).unwrap())
        .flat_map(|d| d["connections"].as_array().cloned().unwrap_or_default())
        .filter(|l| l["line"] == "link" && l["from"] == "customer_id")
        .map(|l| l["id"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(link_ids.len(), 1, "{link_ids:?}");
    // The history keeps no landing elsewhere: what a commit kept is split alike.
    let kept: Vec<String> = conn
        .prepare("SELECT before FROM commit_rows WHERE tbl = 'card_docs' AND before LIKE '%customer_id%'")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert!(!kept.is_empty());
    for before in kept {
        let columns: Value = serde_json::from_str(&before).unwrap();
        let doc: Value = serde_json::from_str(columns["doc"].as_str().unwrap()).unwrap();
        let lines = doc["connections"].as_array().unwrap();
        let at = lines.iter().position(|l| l["line"] == "points").unwrap();
        assert!(lines[at].get("at").is_none(), "{doc}");
        assert_eq!(
            lines[at + 1]["id"].as_str(),
            link_ids.iter().next().map(String::as_str)
        );
    }
    let after = checked(&core, "main");
    assert!(after.get("lineProblems").is_none(), "{after}");
    // Opened again: nothing more.
    core.close();
    drop(core);
    let core = Core::open(&place).unwrap();
    assert_eq!(commits_named(&core, "", "線の着く所を移した"), 1);
}

// MARK: - The window

#[test]
fn the_window_writes_a_stores_holds_and_a_tasks_links_as_connections() {
    let core = Core::open(script_store()).unwrap();
    let window = |name: &str, arguments: Value| {
        op(
            &core,
            json!({"op": "tool", "name": name, "arguments": arguments}),
        )
    };
    // The keys of before are refused from the window too.
    let said = window(
        "card_update",
        json!({"project": PROJECT, "target": "Schemas:Orders", "value": {"holds": ["Schemas:Customer"]}}),
    );
    assert!(
        said["error"]
            .as_str()
            .unwrap()
            .contains("holds は使いません"),
        "{said}"
    );
    let wrote = window(
        "card_update",
        json!({"project": PROJECT, "target": "Schemas:Orders", "value": {"connections": [
            {"line": "holds", "to": "Schemas:Customer"},
            {"line": "holds", "to": "Schemas/Ordering:Order"}
        ]}}),
    );
    assert!(wrote.get("error").is_none(), "{wrote}");
    assert_eq!(
        lines(&core, "Schemas:Orders"),
        [
            json!({ "line": "holds", "to": "Schemas:Customer" }),
            json!({ "line": "holds", "to": "Schemas/Ordering:Order" })
        ]
    );
    let refused_links = window(
        "card_create",
        json!({"project": PROJECT, "branch": "_project_board_", "target": "Goals:Check", "value": {"kind": "task", "label": "確かめる", "links": ["Goals:Check"]}}),
    );
    assert!(
        refused_links["error"]
            .as_str()
            .unwrap()
            .contains("links は使いません"),
        "{refused_links}"
    );
    let made = window(
        "card_create",
        json!({"project": PROJECT, "branch": "_project_board_", "target": "Goals:Check", "value": {"kind": "task", "label": "確かめる", "connections": [{"line": "link", "to": "Goals:Check"}]}}),
    );
    assert!(made.get("error").is_none(), "{made}");
    let task = ok(
        &core,
        "card_read",
        json!({"branch": "_project_board_", "target": "Goals:Check"}),
    );
    assert_eq!(
        task["connections"],
        json!([{ "line": "link", "to": "Goals:Check" }])
    );
}

// MARK: - What a place may draw

/// What `card_read`'s `as: lines` says `target` may draw (to `to`, when `value` names one):
/// the answer, the kinds it may draw now, and those at their kind's `max`.
fn drawable(core: &Core, target: &str, mut value: Value) -> (Value, Vec<String>, Vec<String>) {
    value["as"] = "lineKinds".into();
    let read = ok(core, "card_read", json!({"target": target, "value": value}));
    let names = |key: &str| -> Vec<String> {
        read[key]
            .as_array()
            .into_iter()
            .flatten()
            .map(|l| l["line"].as_str().unwrap().to_string())
            .collect()
    };
    let (now, full) = (names("lines"), names("full"));
    (read, now, full)
}

#[test]
fn the_lines_a_place_may_draw_follow_the_kinds_from_to_and_max() {
    let core = Core::open(script_store()).unwrap();
    let order = "Schemas/Ordering:Order";

    // A field to a card: an embedding (one at most), a pointer (it has one already) and a
    // link (from a field too).
    let (read, now, full) = drawable(
        &core,
        &format!("{order}.customer_id"),
        json!({"to": "Schemas:Customer"}),
    );
    assert_eq!(
        (now, full),
        (
            vec![
                "embed".to_string(),
                "points".to_string(),
                "link".to_string()
            ],
            vec![]
        )
    );
    assert_eq!(read["path"], "Schemas/Ordering:Order.customer_id");
    assert_eq!(read["from"], "customer_id");
    assert_eq!(read["to"], "Schemas:Customer");
    assert!(read.get("at").is_none(), "{read}");
    let points = &read["lines"][1];
    assert_eq!(points["label"], "ID で指す");
    assert_eq!(points["base"], "designate");
    assert_eq!(points["count"], 1);
    assert_eq!(read["lines"][0]["max"], 1);
    // To a field: an embedding lands on a card only; where it lands, as connections writes it.
    let (read, now, full) = drawable(
        &core,
        &format!("{order}.customer_id"),
        json!({"to": "Schemas:Customer.id"}),
    );
    assert_eq!(
        (now, full),
        (vec!["points".to_string(), "link".to_string()], vec![])
    );
    assert_eq!(
        (&read["to"], &read["at"]),
        (&json!("Schemas:Customer"), &json!("id"))
    );
    // A field that embeds already has as many embeddings as it may.
    let (_, now, full) = drawable(
        &core,
        &format!("{order}.status"),
        json!({"to": "Schemas:Customer"}),
    );
    assert_eq!(
        (now, full),
        (
            vec!["points".to_string(), "link".to_string()],
            vec!["embed".to_string()]
        )
    );

    // A card's head: a link, and the project's own line (its field the line needs).
    let (read, now, _) = drawable(&core, order, json!({"to": "Schemas:Customer"}));
    assert_eq!(now, ["link", "signedBy"]);
    assert_eq!(read["lines"][1]["required"], json!(["payload"]));
    let (_, now, _) = drawable(&core, order, json!({"to": "Schemas:Customer.id"}));
    assert_eq!(now, ["link", "signedBy"]);
    // A store: what it holds is an entity or a value.
    let (_, now, _) = drawable(&core, "Schemas:Orders", json!({"to": order}));
    assert_eq!(now, ["link", "holds"]);
    let (_, now, _) = drawable(&core, "Schemas:Orders", json!({"to": "Schemas:Status"}));
    assert_eq!(now, ["link"]);

    // A flow's rules: a note, a document or an item of a formatdoc — a row a link lands on
    // too.
    let checkout = "Charts/Shop:Checkout";
    let (_, now, _) = drawable(&core, checkout, json!({"to": "Free:Memo"}));
    assert_eq!(now, ["link", "rules"]);
    let (read, now, _) = drawable(&core, checkout, json!({"to": "Docs:Spec.intro"}));
    assert_eq!(now, ["link", "rules"]);
    assert_eq!(
        (&read["to"], &read["at"]),
        (&json!("Docs:Spec"), &json!("intro"))
    );
    let (_, now, _) = drawable(&core, checkout, json!({"to": "Schemas:Customer"}));
    assert_eq!(now, ["link"]);

    // A node: the ways to a node beside it in its flow, its else had already; none to another
    // flow's node.
    let (read, now, full) = drawable(
        &core,
        "Charts/Shop:Refund.check",
        json!({"to": "Charts/Shop:Refund.done"}),
    );
    assert_eq!(
        (now, full),
        (
            vec!["link".to_string(), "condition".to_string()],
            vec!["else".to_string()]
        )
    );
    assert_eq!(read["to"], "Charts/Shop:Refund.done");
    assert_eq!(read["lines"][1]["required"], json!(["when"]));
    let (_, now, _) = drawable(
        &core,
        "Charts/Shop:Refund.check",
        json!({"to": "Charts/Shop:Checkout.done"}),
    );
    assert_eq!(now, ["link"]);
    // A process node's row (`from`): what its kind of row writes (a read, reads) — and a link, a
    // row as any.
    let (read, now, _) = drawable(
        &core,
        "Charts/Shop:Refund.a",
        json!({"from": "steps[0]", "to": "Schemas:Customer.id"}),
    );
    assert_eq!(now, ["link", "reads"]);
    assert_eq!(read["from"], "steps[0]");
    assert_eq!(read["path"], "Charts/Shop:Refund.a");
    assert_eq!(read["lines"][1]["count"], 1);

    // Without `to`: what it may draw, and what each reaches.
    let (read, now, _) = drawable(&core, &format!("{order}.customer_id"), json!({}));
    assert_eq!(now, ["embed", "points", "link", "implementedBy"]);
    assert_eq!(read["lines"][0]["reaches"], "entity・value・enumの頭");
    assert_eq!(read["lines"][1]["reaches"], "entity・valueの頭・欄");
    assert_eq!(read["lines"][2]["reaches"], "どのカードの頭・欄・行");

    // The project's own line, one at most: drawn once, it is full.
    let kinds = ok(&core, "kind_read", json!({}));
    let signed = kinds["lines"]
        .as_array()
        .unwrap()
        .iter()
        .find(|l| l["line"] == "signedBy")
        .unwrap()
        .clone();
    let reason = json!({ "line": "根拠", "label": "根拠", "base": "link", "max": 1,
        "from": [{ "kinds": ["entity"], "at": "card" }],
        "to": { "kinds": ["*"], "at": ["card", "row"] } });
    common::set_kinds(&core, PROJECT, json!({"lines": [signed, reason]})).unwrap();
    let (_, now, full) = drawable(&core, order, json!({"to": "Docs:Spec"}));
    assert_eq!(
        (now, full),
        (
            vec![
                "link".to_string(),
                "signedBy".to_string(),
                "根拠".to_string()
            ],
            vec![]
        )
    );
    ok(
        &core,
        "card_update",
        with(order, &[json!({"line": "根拠", "to": "Docs:Spec"})]),
    );
    // To an item: a row, where a link and a key land too.
    let (_, now, full) = drawable(&core, order, json!({"to": "Docs:Spec.intro"}));
    assert_eq!(
        (now, full),
        (
            vec!["link".to_string(), "signedBy".to_string()],
            vec!["根拠".to_string()]
        )
    );

    // Through a card ghost: written to the ghost, landing in the card it leads to — and what it
    // says is written as it is.
    ok(
        &core,
        "card_create",
        json!({"target": "Free:Buyer", "value": {"kind": "ghost", "target": "Schemas:Customer"}}),
    );
    let (read, now, _) = drawable(
        &core,
        &format!("{order}.customer_id"),
        json!({"to": "Free:Buyer.id"}),
    );
    assert_eq!(now, ["points", "link"]);
    assert_eq!(
        (&read["to"], &read["at"]),
        (&json!("Free:Buyer"), &json!("id"))
    );
    ok(
        &core,
        "card_update",
        with(
            order,
            &[json!({"line": "points", "from": "customer_id", "to": read["to"], "at": read["at"]})],
        ),
    );
    assert!(lines(&core, order).contains(
        &json!({"line": "points", "from": "customer_id", "to": "Free:Buyer", "at": "id"})
    ));

    // What it cannot be asked.
    let said = refused(
        &core,
        "card_read",
        json!({"target": "Free:Buyer", "value": {"as": "lineKinds"}}),
    );
    assert!(said.contains("カードゴーストで、線を持ちません"), "{said}");
    let said = refused(
        &core,
        "card_read",
        json!({"target": format!("{order}.customer_id"), "value": {"as": "lineKinds", "from": "status"}}),
    );
    assert!(
        said.contains("from（status）は target がカードのときに書きます"),
        "{said}"
    );
    let said = refused(
        &core,
        "card_read",
        json!({"target": "Charts/Shop:Refund.a", "value": {"as": "lineKinds", "from": "steps[5]"}}),
    );
    assert!(
        said.contains("出元の「steps[5]」は Charts/Shop:Refund.a にありません"),
        "{said}"
    );
    let said = refused(
        &core,
        "card_read",
        json!({"target": order, "value": {"as": "lineKinds", "to": "Schemas:Nope"}}),
    );
    assert!(said.contains("カード「Nope」がありません"), "{said}");
    let said = refused(
        &core,
        "card_read",
        json!({"target": [order, "Schemas:Customer"], "value": {"as": "lineKinds"}}),
    );
    assert!(
        said.contains("as: lines の target は、線を引く所 1 つです"),
        "{said}"
    );
    let said = refused(
        &core,
        "card_read",
        json!({"target": order, "value": {"as": "lineKinds", "depth": 2}}),
    );
    assert!(said.contains("使えないキーです: depth"), "{said}");
}

/// MCP adds a card's lines and takes them away: `{add, remove}` on `card_update` — taken away
/// first, by the keys a line is told apart by that the call gives, one line each; added after, none
/// twice. A whole list is the window's (and `card_create`'s); MCP is told the shape.
#[test]
fn mcp_adds_and_takes_away_a_cards_lines() {
    let core = Core::open(script_store()).unwrap();
    let order = "Schemas/Ordering:Order";
    let before = lines(&core, order);
    let edit = |edits: Value| json!({"target": order, "value": {"connections": edits}});
    // A whole list from MCP: refused, with the shape.
    let said = refused(
        &core,
        "card_update",
        json!({"target": order, "value": {"connections": before}}),
    );
    assert!(said.contains("add"), "{said}");
    assert!(said.contains("remove"), "{said}");
    // Added: after the ones it has.
    ok(
        &core,
        "card_update",
        edit(
            json!({"add": [{"line": "link", "to": "Schemas:Customer", "description": "注文した客"}]}),
        ),
    );
    let now = lines(&core, order);
    assert_eq!(now.len(), before.len() + 1);
    assert_eq!(&now[..before.len()], &before[..]);
    assert_eq!(
        now.last().unwrap(),
        &json!({"line": "link", "to": "Schemas:Customer", "description": "注文した客"})
    );
    // Twice: refused.
    let said = refused(
        &core,
        "card_update",
        edit(json!({"add": [{"line": "link", "to": "Schemas:Customer"}]})),
    );
    assert!(said.contains("既にあります"), "{said}");
    // Taken away by what it gives, and put back otherwise in one call.
    ok(
        &core,
        "card_update",
        edit(json!({
            "remove": [{"line": "link", "to": "Schemas:Customer"}],
            "add": [{"line": "link", "to": "Schemas:Customer", "description": "支払う客"}]
        })),
    );
    let now = lines(&core, order);
    assert_eq!(now.len(), before.len() + 1);
    assert_eq!(now.last().unwrap()["description"], "支払う客");
    // What names no line, and what names two: refused, with the lines there are.
    let said = refused(
        &core,
        "card_update",
        edit(json!({"remove": [{"line": "link", "to": "Schemas:Orders"}]})),
    );
    assert!(said.contains("合う線がありません"), "{said}");
    let said = refused(
        &core,
        "card_update",
        edit(json!({"remove": [{"line": "embed"}]})),
    );
    assert!(said.contains("合う線が 2 本あります"), "{said}");
    // Taken away, nothing else touched.
    ok(
        &core,
        "card_update",
        edit(json!({"remove": [{"line": "link"}]})),
    );
    assert_eq!(lines(&core, order), before);
    // A line whose card is gone: taken away by to: null.
    ok(&core, "card_delete", json!({"target": "Schemas:Customer"}));
    let gone = lines(&core, order)
        .into_iter()
        .filter(|l| l["to"].is_null())
        .count();
    assert!(gone > 0);
    let first_gone = lines(&core, order)
        .into_iter()
        .find(|l| l["to"].is_null())
        .unwrap();
    ok(
        &core,
        "card_update",
        edit(
            json!({"remove": [{"line": first_gone["line"], "from": first_gone["from"], "to": null}]}),
        ),
    );
    assert_eq!(
        lines(&core, order)
            .into_iter()
            .filter(|l| l["to"].is_null())
            .count(),
        gone - 1
    );
    // The guide of the key says both: the list a card is made with, and what card_update takes.
    let (_, guide) = call(&core, "guide", json!({"about": "connections"}));
    let guide: Value = serde_json::from_str(&guide).unwrap();
    // Each shape's own line says its type first: `[…]` a list, `{…}` an object.
    let mut said = vec![(guide["shape"][""].to_string(), guide["in"].to_string())];
    for other in guide["otherShapes"].as_array().unwrap() {
        said.push((other["shape"][""].to_string(), other["in"].to_string()));
    }
    assert!(
        said.iter()
            .any(|(t, at)| t.starts_with("\"[") && at.contains("card_create")),
        "{guide}"
    );
    assert!(
        said.iter()
            .any(|(t, at)| t.starts_with("\"{") && at.contains("card_update")),
        "{guide}"
    );
    // Neither add nor remove: refused.
    let said = refused(&core, "card_update", edit(json!({"set": []})));
    assert!(
        said.contains("value.connections に使えないキーです: set。使えるのは: add / remove"),
        "{said}"
    );
}

/// Two lines with the same ends and landing that differ only in their fields: added one by one
/// (the same fields again refused), taken away one by one by the fields the call writes (left
/// out, both match and are named), the one left keeping its own id.
#[test]
fn lines_differing_only_in_their_fields_are_added_and_taken_away_one_by_one() {
    let core = Core::open(script_store()).unwrap();
    let order = "Schemas/Ordering:Order";
    let edit = |edits: Value| json!({"target": order, "value": {"connections": edits}});
    let signed = |payload: &str| json!({"line": "signedBy", "to": "Schemas:Customer", "at": "id", "fields": {"payload": payload}});
    let before = lines(&core, order);
    // Fields alone differ: added.
    ok(
        &core,
        "card_update",
        edit(json!({"add": [signed("receipt")]})),
    );
    let now = lines(&core, order);
    assert_eq!(now.len(), before.len() + 1);
    assert_eq!(now.last().unwrap(), &signed("receipt"));
    // The same fields again: refused.
    let said = refused(
        &core,
        "card_update",
        edit(json!({"add": [signed("receipt")]})),
    );
    assert!(said.contains("connections.add[0] の線（"), "{said}");
    assert!(said.contains("）は既にあります"), "{said}");
    let ids = |core: &Core| -> Vec<(String, String)> {
        schema(&detail(core, ""), "Order")["connections"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|l| l["line"] == "signedBy")
            .map(|l| {
                (
                    l["fields"]["payload"].as_str().unwrap().to_string(),
                    l["id"].as_str().unwrap().to_string(),
                )
            })
            .collect()
    };
    let both = ids(&core);
    assert_eq!(both.len(), 2, "{both:?}");
    assert_eq!(both[0].0, "approval");
    // Without the fields both match: refused, both named.
    let said = refused(
        &core,
        "card_update",
        edit(json!({"remove": [{"line": "signedBy", "to": "Schemas:Customer", "at": "id"}]})),
    );
    assert!(
        said.contains("connections.remove[0] に合う線が 2 本あります（line・from・to・at・description・fields を足して 1 本に絞ります）"),
        "{said}"
    );
    assert!(
        said.contains("approval") && said.contains("receipt"),
        "{said}"
    );
    // Fields that no line has: refused, with the lines there are.
    let said = refused(
        &core,
        "card_update",
        edit(json!({"remove": [{"line": "signedBy", "fields": {"payload": "refund"}}]})),
    );
    assert!(
        said.contains("connections.remove[0] に合う線がありません。今ある線: "),
        "{said}"
    );
    // The first taken away by its fields: the other stays, with its own id.
    ok(
        &core,
        "card_update",
        edit(
            json!({"remove": [{"line": "signedBy", "to": "Schemas:Customer", "at": "id", "fields": {"payload": "approval"}}]}),
        ),
    );
    let left = ids(&core);
    assert_eq!(left, vec![both[1].clone()]);
    assert_eq!(lines(&core, order).len(), before.len());
    // And put back: a line of its own again, the other's id untouched.
    ok(
        &core,
        "card_update",
        edit(json!({"add": [signed("approval")]})),
    );
    let again = ids(&core);
    assert_eq!(again.len(), 2);
    assert_eq!(again[0], both[1]);
    assert_ne!(again[1].1, both[1].1);
}

#[test]
fn a_flows_rules_are_drawn_from_its_head_in_a_section_diagram() {
    let core = Core::open(script_store()).unwrap();
    let checkout = "Charts/Shop:Checkout";
    ok(
        &core,
        "card_update",
        with(
            checkout,
            &[
                json!({"line": "rules", "to": "Docs:Spec", "at": "intro"}),
                json!({"line": "rules", "to": "Free:Memo"}),
            ],
        ),
    );
    let drawn = ok(
        &core,
        "section_read",
        json!({"target": "Charts/Shop", "value": {"as": "diagram"}}),
    );
    let mermaid = drawn["mermaid"].as_str().unwrap();
    // One line each from the flow, saying `notes` and where it lands — no row of the flow.
    let said = |to: &str| -> Vec<String> {
        mermaid
            .lines()
            .map(|line| line.trim().to_string())
            .filter(|line| line.starts_with(&format!("Charts_Shop__Checkout ..> {to} ")))
            .collect()
    };
    let to_spec = said("Docs__Spec");
    assert_eq!(to_spec.len(), 1, "{mermaid}");
    assert!(to_spec[0].ends_with(": rules ← intro"), "{mermaid}");
    let to_memo = said("Free__Memo");
    assert_eq!(to_memo.len(), 1, "{mermaid}");
    assert!(to_memo[0].ends_with(": rules"), "{mermaid}");
    assert!(
        !mermaid.contains("＃d") && !mermaid.contains("#d"),
        "{mermaid}"
    );
}
