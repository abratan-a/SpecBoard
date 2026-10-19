//! A size of a card's own (`specboard://SpecBoard/Docs/Design:Overview` §12): every boxed card may
//! carry one, `{w}` setting the width alone and leaving the height to the contents, `{w, h}` both.
//! What is sent is the size whole; a new overlap is said, not moved.

use crate::common;

use common::{checked, mcp, ok, op, project_id, refused, script_store};
use serde_json::{Value, json};
use specboard_core::Core;

/// The sample with two entities side by side in `Free`, a cell clear of each other's margin:
/// `Free:Wide` at x 200, `Free:Next` at x 212.
fn core() -> Core {
    let core = Core::open(script_store()).unwrap();
    ok(
        &core,
        "card_create",
        json!({"value": {
            "Free:Wide": {"kind": "entity", "topLeft": {"x": 200, "y": 200}},
            "Free:Next": {"kind": "entity", "topLeft": {"x": 212, "y": 200}},
        }}),
    );
    ok(
        &core,
        "card_create",
        json!({"target": ["Free:Wide.a", "Free:Wide.b"], "value": {"type": "string"}}),
    );
    core
}

/// The box `card_read` says a card is drawn at (`include: at`).
fn at(core: &Core, path: &str) -> Value {
    ok(
        core,
        "card_read",
        json!({"target": path, "value": {"include": ["at"]}}),
    )
}

/// A box of `section`'s diagram as `section_read` returns it.
fn placed(core: &Core, section: &str, path: &str) -> Value {
    ok(
        core,
        "section_read",
        json!({"target": section, "value": {"include": ["places"]}}),
    )["places"][path]
        .clone()
}

fn update(core: &Core, path: &str, value: Value) -> Value {
    ok(core, "card_update", json!({"target": path, "value": value}))
}

#[test]
fn a_width_alone_leaves_the_height_to_the_contents() {
    let core = core();
    let first = at(&core, "Free:Wide");
    assert_eq!(first["size"]["w"], 10, "{first}");
    assert!(first.get("ownSize").is_none(), "{first}");
    let tall = first["size"]["h"].as_i64().unwrap();
    update(&core, "Free:Wide", json!({"size": {"w": 20}}));
    let read = at(&core, "Free:Wide");
    assert_eq!(read["size"], json!({"w": 20, "h": tall}), "{read}");
    assert_eq!(read["ownSize"], json!({"w": 20}), "{read}");
    assert_eq!(read["topLeft"], json!({"x": 200, "y": 200}), "{read}");
    assert_eq!(placed(&core, "Free", "Free:Wide")["size"], json!({"w": 20}));
    // A field more: as tall as its rows still.
    ok(
        &core,
        "card_create",
        json!({"target": "Free:Wide.c", "value": {"type": "string"}}),
    );
    assert_eq!(at(&core, "Free:Wide")["size"]["h"], tall + 1);
    // Both: fixed, whatever its rows.
    update(&core, "Free:Wide", json!({"size": {"w": 20, "h": 3}}));
    let read = at(&core, "Free:Wide");
    assert_eq!(read["size"], json!({"w": 20, "h": 3}), "{read}");
    assert_eq!(read["ownSize"], json!({"w": 20, "h": 3}), "{read}");
    // The width alone again: what is sent is the size, the height back to the contents.
    update(&core, "Free:Wide", json!({"size": {"w": 20}}));
    let read = at(&core, "Free:Wide");
    assert_eq!(read["size"], json!({"w": 20, "h": tall + 1}), "{read}");
    assert_eq!(read["ownSize"], json!({"w": 20}), "{read}");
    // Let go of: the project's.
    update(&core, "Free:Wide", json!({"size": null}));
    let read = at(&core, "Free:Wide");
    assert_eq!(read["size"]["w"], 10, "{read}");
    assert!(read.get("ownSize").is_none(), "{read}");
    assert!(placed(&core, "Free", "Free:Wide").get("size").is_none());
    // The window reads the width alone with the card.
    update(&core, "Free:Wide", json!({"size": {"w": 14}}));
    let detail = op(
        &core,
        json!({"op": "project_detail", "project": project_id(&core)}),
    );
    let card = detail["result"]["schemas"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "Wide")
        .cloned()
        .unwrap();
    assert_eq!(card["size"], json!({"w": 14}), "{card}");
}

#[test]
fn a_size_said_wrongly_or_of_a_card_that_cannot_carry_one_is_refused() {
    let core = core();
    let said = refused(
        &core,
        "card_update",
        json!({"target": "Free:Wide", "value": {"size": {"h": 8}}}),
    );
    assert!(said.contains("w は必須"), "{said}");
    let said = refused(
        &core,
        "card_update",
        json!({"target": "Free:Wide", "value": {"size": {"w": 3}}}),
    );
    assert!(said.contains(r#"（{"w": 3} ではなく）"#), "{said}");
    let said = refused(
        &core,
        "card_update",
        json!({"target": "Free:Wide", "value": {"size": {"w": 20, "h": 200}}}),
    );
    assert!(said.contains("（20×200 ではなく）"), "{said}");
    // A flow's node is drawn by its node's shape.
    let said = refused(
        &core,
        "card_update",
        json!({"target": "Charts/Shop:Checkout.check", "value": {"size": {"w": 12}}}),
    );
    assert!(said.contains("eval のカードは大きさを持てません"), "{said}");
    // A card ghost is drawn at the size of the card it leads to.
    ok(
        &core,
        "card_create",
        json!({"target": "Free:Shadow", "value": {"kind": "ghost", "target": "Schemas:Customer"}}),
    );
    let said = refused(
        &core,
        "section_update",
        json!({"target": "Free", "value": {"places": {"Free:Shadow": {"size": {"w": 12}}}}}),
    );
    assert!(
        said.contains("カードゴーストは指す先のカードの大きさ"),
        "{said}"
    );
    assert!(at(&core, "Free:Wide").get("ownSize").is_none());
}

#[test]
fn a_card_made_with_a_width_is_placed_where_the_width_fits() {
    let core = core();
    ok(
        &core,
        "card_create",
        json!({"target": "Free:Narrow", "value": {"kind": "enum", "values": ["a", "b"], "size": {"w": 6}}}),
    );
    let read = at(&core, "Free:Narrow");
    assert_eq!(read["ownSize"], json!({"w": 6}), "{read}");
    assert_eq!(read["size"]["w"], 6, "{read}");
    ok(
        &core,
        "card_create",
        json!({"target": "Free:Broad", "value": {"kind": "entity", "size": {"w": 30}}}),
    );
    assert_eq!(at(&core, "Free:Broad")["size"]["w"], 30);
    let overlaps: Vec<Value> = checked(&core, "main")["overlaps"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|o| o["section"] == "Free")
        .collect();
    assert!(overlaps.is_empty(), "{overlaps:?}");
}

#[test]
fn a_task_of_a_narrower_width_is_as_tall_as_its_body_is_there() {
    let core = core();
    let body = "大きさを持つカードの本文は、そのカードの幅で折り返して見積もる。".repeat(6);
    let board = |tool: &str, mut arguments: Value| {
        arguments["branch"] = "_project_board_".into();
        mcp(&core, tool, arguments).unwrap()
    };
    board(
        "card_create",
        json!({"target": "Goals:Long", "value": {"kind": "task", "body": body}}),
    );
    let face_rows = || {
        let detail = op(
            &core,
            json!({"op": "project_detail", "project": project_id(&core), "branch": "~board"}),
        );
        let card = detail["result"]["schemas"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["name"] == "Long")
            .cloned()
            .unwrap();
        card["faceRows"].as_i64()
    };
    let wide = face_rows().unwrap();
    board(
        "card_update",
        json!({"target": "Goals:Long", "value": {"size": {"w": 6}}}),
    );
    let narrow = face_rows().unwrap();
    assert!(narrow > wide, "{narrow} {wide}");
    let read = board(
        "card_read",
        json!({"target": "Goals:Long", "value": {"include": ["at"]}}),
    );
    assert_eq!(read["size"], json!({"w": 6, "h": narrow}), "{read}");
    // A height of its own: no height of its face is told.
    board(
        "card_update",
        json!({"target": "Goals:Long", "value": {"size": {"w": 6, "h": 4}}}),
    );
    assert_eq!(face_rows(), None);
}

#[test]
fn a_card_grown_over_another_stays_and_says_so() {
    let core = core();
    let answer = update(&core, "Free:Wide", json!({"size": {"w": 20}}));
    let said = answer["warnings"][0].as_str().unwrap_or_default();
    assert!(
        said.contains("大きさを変えたので Free:Next と重なる"),
        "{answer}"
    );
    assert_eq!(
        at(&core, "Free:Wide")["topLeft"],
        json!({"x": 200, "y": 200})
    );
    // The check lists it, and a tidy clears it.
    let over = |core: &Core| -> Vec<Value> {
        checked(core, "main")["overlaps"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter(|o| o["section"] == "Free")
            .collect()
    };
    assert_eq!(over(&core).len(), 1, "{:?}", over(&core));
    // An overlap it had already is not said again.
    let answer = update(&core, "Free:Wide", json!({"size": {"w": 22}}));
    assert!(answer.get("warnings").is_none(), "{answer}");
    // Smaller, nothing newly over.
    let answer = update(&core, "Free:Wide", json!({"size": {"w": 8}}));
    assert!(answer.get("warnings").is_none(), "{answer}");
    let answer = update(&core, "Free:Wide", json!({"size": {"w": 20}}));
    assert!(answer["warnings"][0].is_string(), "{answer}");
    ok(
        &core,
        "section_update",
        json!({"target": "Free", "value": {"tidy": true}}),
    );
    assert!(over(&core).is_empty(), "{:?}", over(&core));
}

#[test]
fn a_card_grown_and_placed_in_one_call_goes_clear_of_the_others() {
    let core = core();
    let answer = update(
        &core,
        "Free:Wide",
        json!({"size": {"w": 20}, "topLeft": {"x": 200, "y": 200}}),
    );
    let warnings = answer["warnings"].as_array().unwrap();
    assert_eq!(warnings.len(), 1, "{answer}");
    assert!(
        warnings[0].as_str().unwrap().contains("近くの空いたマス"),
        "{answer}"
    );
    assert_ne!(
        at(&core, "Free:Wide")["topLeft"],
        json!({"x": 200, "y": 200})
    );
}

#[test]
fn a_part_grown_over_another_in_a_ui_section_is_not_said() {
    let core = core();
    let mut tree = ok(&core, "project_read", json!({}))["projectTree"].clone();
    tree["main_sections"]
        .as_array_mut()
        .unwrap()
        .push(json!({"type": "ui", "name": "UI"}));
    ok(
        &core,
        "project_update",
        json!({"value": {"project_tree": tree}}),
    );
    ok(
        &core,
        "card_create",
        json!({"value": {
            "UI:A": {"kind": "html", "body": "a", "topLeft": {"x": 0, "y": 0}},
            "UI:B": {"kind": "html", "body": "b", "topLeft": {"x": 10, "y": 0}},
        }}),
    );
    let answer = update(&core, "UI:A", json!({"size": {"w": 20}}));
    assert!(answer.get("warnings").is_none(), "{answer}");
    assert_eq!(at(&core, "UI:A")["size"]["w"], 20);
}

#[test]
fn a_card_grown_on_a_goals_map_is_checked_there() {
    let core = core();
    let board = |tool: &str, mut arguments: Value| {
        arguments["branch"] = "_project_board_".into();
        mcp(&core, tool, arguments).unwrap()
    };
    board(
        "card_create",
        json!({"value": {
            "Goals:Ship.Left": {"kind": "task", "topLeft": {"x": 300, "y": 300}},
            "Goals:Ship.Right": {"kind": "task", "topLeft": {"x": 312, "y": 300}},
        }}),
    );
    let answer = board(
        "card_update",
        json!({"target": "Goals:Ship.Left", "value": {"size": {"w": 20}}}),
    );
    let said = answer["warnings"][0].as_str().unwrap_or_default();
    assert!(said.contains("Goals:Ship.Right"), "{answer}");
}

#[test]
fn a_width_alone_goes_through_the_project_file_and_a_merge() {
    let core = core();
    update(&core, "Free:Wide", json!({"size": {"w": 20}}));
    // An old size of both keeps its box.
    update(&core, "Free:Memo", json!({"size": {"w": 12, "h": 6}}));
    let project = project_id(&core);
    let text = op(&core, json!({"op": "project_file", "project": project}))["result"]["text"]
        .as_str()
        .unwrap()
        .to_string();
    let read = op(
        &core,
        json!({"op": "project_file_read", "text": text, "landing": "replace"}),
    );
    assert!(read.get("error").is_none(), "{read}");
    assert_eq!(at(&core, "Free:Wide")["ownSize"], json!({"w": 20}));
    assert_eq!(at(&core, "Free:Memo")["ownSize"], json!({"w": 12, "h": 6}));
    // Sized on a branch, and merged with its places.
    ok(&core, "branch_create", json!({"target": "work"}));
    ok(
        &core,
        "card_update",
        json!({"branch": "work", "target": "Free:Next", "value": {"size": {"w": 16}}}),
    );
    ok(
        &core,
        "branch_merge",
        json!({"target": "work", "value": {"places": true}}),
    );
    assert_eq!(at(&core, "Free:Next")["ownSize"], json!({"w": 16}));
}

#[test]
fn a_kinds_face_said_resizable_is_read_and_let_go() {
    let core = core();
    ok(
        &core,
        "kind_create",
        json!({"target": "risk", "value": {"of": "card",
            "fields": [{"name": "memo", "fieldtype": "markdown"}],
            "face": {"markdown": "memo", "resizable": false}}}),
    );
    let task = ok(&core, "kind_read", json!({"target": "task"}));
    assert_eq!(
        task["face"],
        json!({"top": ["done"], "markdown": "body"}),
        "{task}"
    );
}
