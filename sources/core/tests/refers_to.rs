//! What a card points at, card by card: `card_read` and `card_list` with `ref_to` in
//! `include` (or no `include`) answer `refersTo` — where each of its lines leaves (a field
//! by its path in the card, the head by `(card)`) and the cards they reach — the other side
//! of `referencedBy`; every kind of line since the lines are connections. What was written
//! through a card ghost is said as it was written. A ghost read itself has none. Over the
//! script's project: Schemas/Ordering:Order refers to Schemas:Status (`status`, embed),
//! Schemas/Ordering:Line (`lines`, embed), Schemas:Customer (`customer_id`, points) and
//! Schemas:Customer from its head (the key it signs with, `signedBy`).

use crate::common;

use common::{call, line_edits, script_store};
use serde_json::{Value, json};
use specboard_core::Core;

fn ok(core: &Core, tool: &str, mut arguments: Value) -> Value {
    arguments["project"] = "見本".into();
    let (refused, text) = call(core, tool, arguments.clone());
    assert!(!refused, "{tool} {arguments}: {text}");
    serde_json::from_str(&text).unwrap()
}

/// `refersTo`'s fields in the order given, each with what it names.
fn refers(read: &Value) -> Vec<(String, Vec<String>)> {
    read["refersTo"]
        .as_object()
        .unwrap_or_else(|| panic!("refersTo が無い: {read}"))
        .iter()
        .map(|(k, v)| {
            (
                k.clone(),
                v.as_array()
                    .unwrap()
                    .iter()
                    .map(|p| p.as_str().unwrap().to_string())
                    .collect(),
            )
        })
        .collect()
}

fn pair(field: &str, to: &[&str]) -> (String, Vec<String>) {
    (
        field.to_string(),
        to.iter().map(|s| s.to_string()).collect(),
    )
}

#[test]
fn a_card_says_what_its_fields_point_at_with_or_without_the_fields() {
    let core = Core::open(script_store()).unwrap();
    // Read whole, its refs and its pointer, in the order of its fields.
    let order = ok(
        &core,
        "card_read",
        json!({"target": "Schemas/Ordering:Order"}),
    );
    let said = vec![
        pair("status", &["Schemas:Status"]),
        pair("lines", &["Schemas/Ordering:Line"]),
        pair("customer_id", &["Schemas:Customer"]),
        pair("(card)", &["Schemas:Customer"]),
    ];
    assert_eq!(refers(&order), said);
    // Asked for alone: without the fields.
    let alone = ok(
        &core,
        "card_read",
        json!({"target": "Schemas/Ordering:Order", "value": {"include": ["ref_to"]}}),
    );
    assert_eq!(refers(&alone), said);
    assert!(alone.get("fields").is_none(), "{alone}");
    assert!(alone.get("referencedBy").is_none(), "{alone}");
    // Not asked for: none.
    let without = ok(
        &core,
        "card_read",
        json!({"target": "Schemas/Ordering:Order", "value": {"include": ["fields"]}}),
    );
    assert!(without.get("refersTo").is_none(), "{without}");
    // The other side: what the card it names says of it.
    let customer = ok(&core, "card_read", json!({"target": "Schemas:Customer"}));
    assert!(
        customer["referencedBy"]
            .as_array()
            .unwrap()
            .contains(&json!("Schemas/Ordering:Order.customer_id"))
    );
    // A card that points at nothing says nothing.
    assert!(customer.get("refersTo").is_none(), "{customer}");
    // card_list says the same of each card.
    let listed = ok(
        &core,
        "card_list",
        json!({"value": {"section": "Schemas/Ordering", "include": ["ref_to"]}}),
    );
    let row = listed["cards"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["path"] == "Schemas/Ordering:Order")
        .unwrap();
    assert_eq!(refers(row), said);
}

#[test]
fn a_field_inside_one_and_one_through_a_ghost_are_said_as_written() {
    let core = Core::open(script_store()).unwrap();
    // A pointer inside an object: by its path in the card.
    ok(
        &core,
        "card_create",
        json!({"target": "Schemas/Ordering:Order.shipping.owner", "value": {"type": "string"}}),
    );
    // An embedding through a card ghost: the ghost's path; the card it leads to says it too.
    ok(
        &core,
        "card_create",
        json!({"target": "Schemas/Ordering:Client", "value": {"kind": "ghost", "target": "Schemas:Customer"}}),
    );
    ok(
        &core,
        "card_create",
        json!({"target": "Schemas/Ordering:Order.buyer", "value": {"type": "string"}}),
    );
    let mut lines = ok(
        &core,
        "card_read",
        json!({"target": "Schemas/Ordering:Order", "value": {"include": ["connections"]}}),
    )["connections"]
        .as_array()
        .unwrap()
        .clone();
    let before = lines.clone();
    lines.push(json!({"line": "points", "from": "shipping.owner", "to": "Schemas:Customer"}));
    lines.push(json!({"line": "embed", "from": "buyer", "to": "Client"}));
    ok(
        &core,
        "card_update",
        json!({"target": "Schemas/Ordering:Order", "value": {"connections": line_edits(&before, &lines)}}),
    );
    let order = ok(
        &core,
        "card_read",
        json!({"target": "Schemas/Ordering:Order", "value": {"include": ["ref_to"]}}),
    );
    let said = refers(&order);
    assert!(
        said.contains(&pair("shipping.owner", &["Schemas:Customer"])),
        "{said:?}"
    );
    assert!(
        said.contains(&pair("buyer", &["Schemas/Ordering:Client"])),
        "{said:?}"
    );
    let customer = ok(&core, "card_read", json!({"target": "Schemas:Customer"}));
    let by = customer["referencedBy"].as_array().unwrap();
    assert!(
        by.contains(&json!("Schemas/Ordering:Order.buyer")),
        "{by:?}"
    );
    assert!(
        by.contains(&json!("Schemas/Ordering:Order.shipping.owner")),
        "{by:?}"
    );
    // A ghost read itself says none.
    let ghost = ok(
        &core,
        "card_read",
        json!({"target": "Schemas/Ordering:Client", "value": {"include": ["ref_to"]}}),
    );
    assert!(ghost.get("refersTo").is_none(), "{ghost}");
    // Listed, a ghost says none either.
    let through = ok(
        &core,
        "card_list",
        json!({"value": {"section": "Schemas/Ordering", "kind": "ghost"}}),
    );
    let listed = through["cards"].as_array().unwrap();
    assert!(
        listed
            .iter()
            .any(|c| c["path"] == "Schemas/Ordering:Client"),
        "{through}"
    );
    assert!(
        listed.iter().all(|c| c.get("refersTo").is_none()),
        "{through}"
    );
}

#[test]
fn a_nodes_lines_and_a_goals_cards_lines_are_said_too() {
    let core = Core::open(script_store()).unwrap();
    // A flow's node and the card inside a goal read what their lines reach, and what reaches
    // them, as any card does.
    ok(
        &core,
        "card_create",
        json!({"value": {
            "Charts:Ship": {"kind": "flow"},
            "Charts:Ship.start": {"kind": "begin", "trigger": "manual", "connections": [{"line": "next", "to": "save"}]},
            "Charts:Ship.save": {"kind": "end", "outcome": "success"}
        }}),
    );
    let start = ok(
        &core,
        "card_read",
        json!({"target": "Charts:Ship.start", "value": {"include": ["ref_to"]}}),
    );
    assert_eq!(refers(&start), vec![pair("(card)", &["Charts:Ship.save"])]);
    let save = ok(
        &core,
        "card_read",
        json!({"target": "Charts:Ship.save", "value": {"include": ["ref_by"]}}),
    );
    assert_eq!(save["referencedBy"], json!(["Charts:Ship.start"]));

    let board = |mut arguments: Value| {
        arguments["branch"] = "_project_board_".into();
        arguments
    };
    ok(
        &core,
        "card_create",
        board(json!({"value": {
            "Goals:Refer": {"kind": "goal"},
            "Goals:Refer.design": {"kind": "task"},
            "Goals:Refer.build": {"kind": "task", "connections": [{"line": "link", "to": "Goals:Refer.design"}]}
        }})),
    );
    let build = ok(
        &core,
        "card_read",
        board(json!({"target": "Goals:Refer.build", "value": {"include": ["ref_to"]}})),
    );
    assert_eq!(
        refers(&build),
        vec![pair("(card)", &["Goals:Refer.design"])]
    );
}
