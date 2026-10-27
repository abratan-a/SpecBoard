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
    update(&core, "Free:Wide", json!({"size": {"w": 20, "h": 6}}));
    let read = at(&core, "Free:Wide");
    assert_eq!(read["size"], json!({"w": 20, "h": 6}), "{read}");
    assert_eq!(read["ownSize"], json!({"w": 20, "h": 6}), "{read}");
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
fn a_height_too_short_for_the_strips_is_placed_as_tall_as_it_is_drawn() {
    let core = Core::open(script_store()).unwrap();
    // Cards of 1, 2 and 6 rows and a note of 6 lines, apart from each other.
    let made: [(&str, usize, i64); 3] = [("One", 1, 200), ("Two", 2, 212), ("Six", 6, 224)];
    for (name, _, x) in made {
        ok(
            &core,
            "card_create",
            json!({"target": format!("Free:{name}"), "value": {"kind": "entity", "topLeft": {"x": x, "y": 200}}}),
        );
    }
    for (name, count, _) in made {
        let fields: Vec<String> = (0..count).map(|i| format!("Free:{name}.f{i}")).collect();
        ok(
            &core,
            "card_create",
            json!({"target": fields, "value": {"type": "string"}}),
        );
    }
    ok(
        &core,
        "card_create",
        json!({"target": "Free:Jot", "value": {"kind": "note", "body": "1\n2\n3\n4\n5\n6", "topLeft": {"x": 236, "y": 200}}}),
    );
    // Written h:3, placed as tall as the screen draws it: one row and the half rows above and below
    // it, however many rows it has — the size written stays as written.
    for name in ["One", "Two", "Six"] {
        let path = format!("Free:{name}");
        update(&core, &path, json!({"size": {"w": 10, "h": 3}}));
        let read = at(&core, &path);
        assert_eq!(read["size"], json!({"w": 10, "h": 4}), "{read}");
        assert_eq!(read["bottomRight"]["y"], 200 + 4 - 1, "{read}");
        assert_eq!(read["ownSize"], json!({"w": 10, "h": 3}), "{read}");
        assert_eq!(
            placed(&core, "Free", &path)["size"],
            json!({"w": 10, "h": 3})
        );
    }
    // As tall as that or taller: as written.
    for h in [4, 8] {
        update(&core, "Free:Six", json!({"size": {"w": 10, "h": h}}));
        assert_eq!(at(&core, "Free:Six")["size"], json!({"w": 10, "h": h}));
    }
    // A note is a page of Markdown, as a task's body is: as tall as written, the page scrolled in it.
    update(&core, "Free:Jot", json!({"size": {"w": 10, "h": 3}}));
    let read = at(&core, "Free:Jot");
    assert_eq!(read["size"], json!({"w": 10, "h": 3}), "{read}");
    assert_eq!(read["ownSize"], json!({"w": 10, "h": 3}), "{read}");
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

/// A note's body is Markdown, drawn as a task's body is: its kind's face draws the field `body`,
/// and its box is as tall as that page is, up to the project's whole card.
#[test]
fn a_note_is_drawn_as_its_markdown() {
    let core = core();
    let kind = ok(&core, "kind_read", json!({"target": "note"}));
    assert_eq!(kind["face"], json!({"markdown": "body"}), "{kind}");
    let body = kind["fields"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["name"] == "body")
        .cloned()
        .unwrap();
    assert_eq!(body["fieldtype"], "markdown", "{body}");
    ok(
        &core,
        "card_create",
        json!({"value": {
            "Free:Short": {"kind": "note", "body": "一行だけ", "topLeft": {"x": 0, "y": 300}},
            "Free:Long": {"kind": "note", "body": "- 項目\n".repeat(80), "topLeft": {"x": 20, "y": 300}},
            "Free:Doc": {"kind": "freedoc", "body": "文書", "topLeft": {"x": 40, "y": 300}},
        }}),
    );
    let detail = op(
        &core,
        json!({"op": "project_detail", "project": project_id(&core)}),
    );
    let face_rows = |name: &str| {
        detail["result"]["schemas"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["name"] == name)
            .and_then(|c| c["faceRows"].as_i64())
    };
    let short = face_rows("Short").expect("一行の note に faceRows が無い");
    let long = face_rows("Long").expect("長い note に faceRows が無い");
    assert!(short <= 4, "{short}");
    assert!(long > short, "{long} {short}");
    // The longest is the project's whole card, as a document is.
    let whole = placed(&core, "Free", "Free:Doc");
    let tall = |b: &Value| {
        b["bottomRight"]["y"].as_i64().unwrap() - b["topLeft"]["y"].as_i64().unwrap() + 1
    };
    assert_eq!(long, tall(&whole));
    assert_eq!(tall(&placed(&core, "Free", "Free:Long")), long);
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
fn a_card_made_over_another_says_where_it_went() {
    let core = core();
    let answer = ok(
        &core,
        "card_create",
        json!({"value": {
            "Free:Over": {"kind": "note", "topLeft": {"x": 200, "y": 200}},
            "Free:Alias": {"kind": "ghost", "target": "Schemas:Customer", "topLeft": {"x": 212, "y": 200}},
        }}),
    );
    let warnings: Vec<&str> = answer["warnings"]
        .as_array()
        .unwrap_or_else(|| panic!("{answer}"))
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert_eq!(warnings.len(), 2, "{answer}");
    for (path, said) in ["Free:Over", "Free:Alias"].iter().zip(&warnings) {
        assert!(said.starts_with(&format!("{path}: ")), "{answer}");
        assert!(said.contains("近くの空いたマス"), "{answer}");
    }
    assert_ne!(
        at(&core, "Free:Over")["topLeft"],
        json!({"x": 200, "y": 200})
    );
    assert_ne!(
        at(&core, "Free:Alias")["topLeft"],
        json!({"x": 212, "y": 200})
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

/// `core()` with `Free:Wide` sized `{w: 20}` on main and `Free:Next` at (240, 200), clear of it, and
/// then the branch `work`, where neither has a row of its own yet.
fn sized_then_branched() -> Core {
    let core = core();
    update(&core, "Free:Next", json!({"topLeft": {"x": 240, "y": 200}}));
    update(&core, "Free:Wide", json!({"size": {"w": 20}}));
    ok(&core, "branch_create", json!({"target": "work"}));
    core
}

/// The box `card_read` says a card is drawn at in `branch` (`include: at`).
fn at_in(core: &Core, branch: &str, path: &str) -> Value {
    ok(
        core,
        "card_read",
        json!({"branch": branch, "target": path, "value": {"include": ["at"]}}),
    )
}

/// The id a tree keeps a card under, by its name.
fn card_id(core: &Core, name: &str) -> String {
    rusqlite::Connection::open(core.place())
        .unwrap()
        .query_row(
            "SELECT id FROM card_docs WHERE branch_id = '' AND name = ?",
            [name],
            |r| r.get(0),
        )
        .unwrap()
}

/// The id the store keeps a branch under, by its name.
fn branch_id(core: &Core, name: &str) -> String {
    rusqlite::Connection::open(core.place())
        .unwrap()
        .query_row("SELECT id FROM branches WHERE name = ?", [name], |r| {
            r.get(0)
        })
        .unwrap()
}

/// One way of moving a card: what it is called, the move, and where the card goes.
type Way = (&'static str, fn(&Core), (i64, i64));

#[test]
fn a_card_moved_on_a_branch_keeps_the_size_main_gave_it() {
    let moves: [Way; 5] = [
        (
            "topLeft",
            |core| {
                ok(
                    core,
                    "card_update",
                    json!({"branch": "work", "target": "Free:Wide", "value": {"topLeft": {"x": 300, "y": 300}}}),
                );
            },
            (300, 300),
        ),
        (
            "places",
            |core| {
                ok(
                    core,
                    "section_update",
                    json!({"branch": "work", "target": "Free", "value": {"places": {"Free:Wide": {"topLeft": {"x": 300, "y": 300}}}}}),
                );
            },
            (300, 300),
        ),
        (
            "move",
            |core| {
                ok(
                    core,
                    "section_update",
                    json!({"branch": "work", "target": "Free", "value": {"move": {"cards": ["Free:Wide"], "by": {"x": 0, "y": 20}}}}),
                );
            },
            (200, 220),
        ),
        (
            "arrange",
            |core| {
                ok(
                    core,
                    "section_update",
                    json!({"branch": "work", "target": "Free", "value": {"arrange": {"by": "order", "cards": ["Free:Wide"], "columns": 1, "at": {"x": 400, "y": 400}}}}),
                );
            },
            (400, 400),
        ),
        (
            "the window's places",
            |core| {
                let said = op(
                    core,
                    json!({"op": "places", "project": project_id(core), "branch": branch_id(core, "work"),
                        "boxes": [{"item": card_id(core, "Wide"), "x": 300, "y": 300}]}),
                );
                assert!(said.get("error").is_none(), "{said}");
            },
            (300, 300),
        ),
    ];
    for (how, put, (x, y)) in moves {
        let core = sized_then_branched();
        put(&core);
        let moved = at_in(&core, "work", "Free:Wide");
        assert_eq!(moved["topLeft"], json!({"x": x, "y": y}), "{how}: {moved}");
        assert_eq!(moved["ownSize"], json!({"w": 20}), "{how}: {moved}");
        let main = at_in(&core, "main", "Free:Wide");
        assert_eq!(
            main["topLeft"],
            json!({"x": 200, "y": 200}),
            "{how}: {main}"
        );
        assert_eq!(main["ownSize"], json!({"w": 20}), "{how}: {main}");
    }
}

#[test]
fn a_card_tidied_on_a_branch_keeps_the_size_main_gave_it() {
    let core = core();
    update(
        &core,
        "Free:Next",
        json!({"topLeft": {"x": 200, "y": 214}, "size": {"w": 14}}),
    );
    assert_eq!(
        at(&core, "Free:Next")["topLeft"],
        json!({"x": 200, "y": 214})
    );
    update(&core, "Free:Wide", json!({"size": {"w": 20, "h": 20}}));
    ok(&core, "branch_create", json!({"target": "work"}));
    let tidied = ok(
        &core,
        "section_update",
        json!({"branch": "work", "target": "Free", "value": {"tidy": true}}),
    );
    let names: Vec<&str> = tidied["tidied"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|t| t["card"].as_str())
        .collect();
    assert_eq!(names, ["Free:Next"], "{tidied}");
    let next = at_in(&core, "work", "Free:Next");
    assert_eq!(next["ownSize"], json!({"w": 14}), "{next}");
    let wide = at_in(&core, "work", "Free:Wide");
    let corner =
        |v: &Value, key: &str| (v[key]["x"].as_i64().unwrap(), v[key]["y"].as_i64().unwrap());
    let ((nx, ny), (nr, nb)) = (corner(&next, "topLeft"), corner(&next, "bottomRight"));
    let ((wx, wy), (wr, wb)) = (corner(&wide, "topLeft"), corner(&wide, "bottomRight"));
    assert!(
        nr < wx || wr < nx || nb < wy || wb < ny,
        "{next} は {wide} に重なる"
    );
    let main = at_in(&core, "main", "Free:Next");
    assert_eq!(main["topLeft"], json!({"x": 200, "y": 214}), "{main}");
    assert_eq!(main["ownSize"], json!({"w": 14}), "{main}");
}

#[test]
fn a_size_written_on_a_branch_keeps_the_place_main_gave_the_card() {
    let core = sized_then_branched();
    ok(
        &core,
        "card_update",
        json!({"branch": "work", "target": "Free:Wide", "value": {"size": {"w": 24}}}),
    );
    let read = at_in(&core, "work", "Free:Wide");
    assert_eq!(read["topLeft"], json!({"x": 200, "y": 200}), "{read}");
    assert_eq!(read["ownSize"], json!({"w": 24}), "{read}");
    assert_eq!(
        at_in(&core, "main", "Free:Wide")["ownSize"],
        json!({"w": 20})
    );

    let core = sized_then_branched();
    let put = ok(
        &core,
        "section_update",
        json!({"branch": "work", "target": "Free", "value": {"places": {"Free:Wide": {"size": {"w": 24}}}}}),
    );
    assert_eq!(put["placed"], 1, "{put}");
    let read = at_in(&core, "work", "Free:Wide");
    assert_eq!(read["topLeft"], json!({"x": 200, "y": 200}), "{read}");
    assert_eq!(read["ownSize"], json!({"w": 24}), "{read}");
    assert_eq!(
        at_in(&core, "main", "Free:Wide")["ownSize"],
        json!({"w": 20})
    );
}

#[test]
fn other_boxes_moved_on_a_branch_keep_the_size_main_gave_them() {
    let map_box = |core: &Core, branch: &str| -> Value {
        ok(
            core,
            "card_read",
            json!({"branch": branch, "target": "Free:Aim", "value": {"include": ["places"]}}),
        )["places"]["Free:Aim.Step"]
            .clone()
    };
    let made = || {
        let core = core();
        ok(&core, "section_create", json!({"target": "Free/Other"}));
        ok(
            &core,
            "section_update",
            json!({"target": "Free", "value": {"places": {"Free/Other": {"topLeft": {"x": 600, "y": 600}, "size": {"w": 16}}}}}),
        );
        ok(
            &core,
            "card_create",
            json!({"value": {"Free:Aim": {"kind": "goal", "topLeft": {"x": 500, "y": 500}}, "Free:Aim.Step": {"kind": "task"}}}),
        );
        update(&core, "Free:Aim.Step", json!({"size": {"w": 18}}));
        let step = map_box(&core, "main");
        assert_eq!(step["topLeft"], json!({"x": 0, "y": 0}), "{step}");
        assert_eq!(step["size"], json!({"w": 18}), "{step}");
        ok(&core, "branch_create", json!({"target": "work"}));
        core
    };

    let core = made();
    ok(
        &core,
        "section_update",
        json!({"branch": "work", "target": "Free", "value": {"places": {"Free/Other": {"topLeft": {"x": 640, "y": 640}}}}}),
    );
    let ghost = ok(
        &core,
        "section_read",
        json!({"branch": "work", "target": "Free", "value": {"include": ["places"]}}),
    )["places"]["Free/Other"]
        .clone();
    assert_eq!(ghost["topLeft"], json!({"x": 640, "y": 640}), "{ghost}");
    assert_eq!(ghost["size"], json!({"w": 16}), "{ghost}");
    ok(
        &core,
        "card_update",
        json!({"branch": "work", "target": "Free:Aim", "value": {"places": {"Free:Aim.Step": {"topLeft": {"x": 30, "y": 30}}}}}),
    );
    let step = map_box(&core, "work");
    assert_eq!(step["topLeft"], json!({"x": 30, "y": 30}), "{step}");
    assert_eq!(step["size"], json!({"w": 18}), "{step}");

    let core = made();
    ok(
        &core,
        "card_update",
        json!({"branch": "work", "target": "Free:Aim.Step", "value": {"size": {"w": 20}}}),
    );
    let step = map_box(&core, "work");
    assert_eq!(step["topLeft"], json!({"x": 0, "y": 0}), "{step}");
    assert_eq!(step["size"], json!({"w": 20}), "{step}");
}

#[test]
fn a_card_moved_to_another_section_on_a_branch_is_given_room_there() {
    let made = || {
        let core = core();
        ok(&core, "section_create", json!({"target": "Free/Other"}));
        update(&core, "Free:Wide", json!({"size": {"w": 20}}));
        ok(&core, "branch_create", json!({"target": "work"}));
        core
    };
    let core = made();
    ok(
        &core,
        "card_update",
        json!({"branch": "work", "target": "Free:Wide", "value": {"section": "Free/Other"}}),
    );
    let moved = at_in(&core, "work", "Free/Other:Wide");
    let main = at_in(&core, "main", "Free:Wide");
    assert_eq!(main["topLeft"], json!({"x": 200, "y": 200}), "{main}");
    assert_eq!(main["ownSize"], json!({"w": 20}), "{main}");

    let core = made();
    update(&core, "Free:Wide", json!({"section": "Free/Other"}));
    let on_main = at(&core, "Free/Other:Wide");
    assert_eq!(on_main["topLeft"], json!({"x": 0, "y": 0}), "{on_main}");
    assert_eq!(moved["topLeft"], on_main["topLeft"], "{moved}");
    assert_eq!(moved["ownSize"], json!({"w": 20}), "{moved}");
}
