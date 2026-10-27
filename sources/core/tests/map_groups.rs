//! Groups on a diagram (`specboard://SpecBoard/Docs/Design:Overview` §12): named frames on a section's
//! map and a container's card map, written with `groups` of `section_update` and `card_update` and read
//! with `include: ["groups"]`. What is in a group is worked out from the boxes each time — a box whose
//! top-left cell is inside, a group whose top-left is inside and whose frame is smaller — and moves with
//! it. A group is kept as a box's place is: no commit, a branch's over main's, merged with `places`.

use crate::common;

use common::{ok, op, project_id, refused, script_store};
use serde_json::{Value, json};
use specboard_core::Core;
use std::collections::HashMap;

/// The sample with three notes on the free section's map: A at (2, 3), B at (20, 12) and C at (25, 2),
/// and the section's own note out of the way.
fn core() -> Core {
    let core = Core::open(script_store()).unwrap();
    ok(
        &core,
        "card_create",
        json!({"value": {
            "Free:A": {"kind": "note", "body": "a"},
            "Free:B": {"kind": "note", "body": "b"},
            "Free:C": {"kind": "note", "body": "c"},
        }}),
    );
    let put = ok(
        &core,
        "section_update",
        json!({"target": "Free", "value": {"places": {
            "Free:Memo": {"topLeft": {"x": 300, "y": 300}},
            "Free:A": {"topLeft": {"x": 2, "y": 3}},
            "Free:B": {"topLeft": {"x": 20, "y": 12}},
            "Free:C": {"topLeft": {"x": 25, "y": 2}},
        }}}),
    );
    assert!(put.get("jumped").is_none(), "{put}");
    core
}

/// `groups` written on the free section's map in `branch`: the answer.
fn write(core: &Core, branch: &str, groups: Value) -> Value {
    ok(
        core,
        "section_update",
        json!({"branch": branch, "target": "Free", "value": {"groups": groups}}),
    )
}

/// The free section's groups as `section_read` returns them in `branch`.
fn groups(core: &Core, branch: &str) -> Value {
    let read = ok(
        core,
        "section_read",
        json!({"branch": branch, "target": "Free", "value": {"include": ["groups"]}}),
    );
    read["groups"].clone()
}

/// Where a box of the free section's map sits in `branch`.
fn at(core: &Core, branch: &str, path: &str) -> Value {
    let read = ok(
        core,
        "section_read",
        json!({"branch": branch, "target": "Free", "value": {"include": ["places"]}}),
    );
    read["places"][path]["topLeft"].clone()
}

fn xy(x: i64, y: i64) -> Value {
    json!({"x": x, "y": y})
}

/// A section's id in main, by its name.
fn section_id(core: &Core, name: &str) -> String {
    rusqlite::Connection::open(core.place())
        .unwrap()
        .query_row(
            "SELECT id FROM section_docs WHERE branch_id = '' AND name = ?",
            [name],
            |r| r.get(0),
        )
        .unwrap()
}

/// How many group rows a tree keeps of its own on a diagram.
fn group_rows(core: &Core, view: &str) -> i64 {
    rusqlite::Connection::open(core.place())
        .unwrap()
        .query_row(
            "SELECT count(*) FROM places WHERE item_kind = 'group' AND view = ?",
            [view],
            |r| r.get(0),
        )
        .unwrap()
}

/// The group Auth at (0, 0), 24 by 14: A and B in it by their top-left, C not.
fn with_auth(core: &Core) -> Value {
    write(
        core,
        "",
        json!({"Auth": {"topLeft": xy(0, 0), "size": {"w": 24, "h": 14}, "color": "blue"}}),
    )
}

#[test]
fn a_group_is_made_and_holds_the_boxes_whose_top_left_is_in_it() {
    let core = core();
    let made = with_auth(&core);
    assert_eq!(
        made["groups"]["Auth"],
        json!({"topLeft": xy(0, 0), "bottomRight": xy(23, 13), "size": {"w": 24, "h": 14}}),
        "{made}"
    );
    assert!(made.get("moved").is_none(), "{made}");
    let read = groups(&core, "");
    // B reaches out of the frame, but its top-left is in it; C's is not.
    assert_eq!(
        read,
        json!({"Auth": {"topLeft": xy(0, 0), "bottomRight": xy(23, 13), "size": {"w": 24, "h": 14},
            "color": "blue", "members": ["Free:A", "Free:B"]}})
    );
    // Grown out of the frame, a box stays in it.
    ok(
        &core,
        "card_update",
        json!({"target": "Free:A", "value": {"size": {"w": 10, "h": 30}}}),
    );
    assert_eq!(
        groups(&core, "")["Auth"]["members"],
        json!(["Free:A", "Free:B"])
    );
    // A map with none reads empty.
    let read = ok(
        &core,
        "section_read",
        json!({"target": "Schemas", "value": {"include": ["groups", "places"]}}),
    );
    assert_eq!(read["groups"], json!({}));
    assert!(read["places"].is_object(), "{read}");
}

#[test]
fn a_smaller_group_whose_top_left_is_inside_is_held_and_one_of_the_same_size_is_not() {
    let core = core();
    with_auth(&core);
    write(
        &core,
        "",
        json!({
            "Session": {"topLeft": xy(1, 10), "size": {"w": 8, "h": 3}},
            "Twin": {"topLeft": xy(10, 1), "size": {"w": 24, "h": 14}},
        }),
    );
    let read = groups(&core, "");
    assert_eq!(read["Auth"]["groups"], json!(["Session"]), "{read}");
    assert!(read["Twin"].get("groups").is_none(), "{read}");
    // Twin's top-left is in Auth but it is as big: neither holds the other.
    assert_eq!(read["Twin"]["members"], json!(["Free:C", "Free:B"]));
    assert!(read["Session"]["members"].as_array().unwrap().is_empty());
}

#[test]
fn moving_a_group_moves_what_is_in_it_by_as_much() {
    let core = core();
    with_auth(&core);
    write(
        &core,
        "",
        json!({"Session": {"topLeft": xy(1, 10), "size": {"w": 8, "h": 3}}}),
    );
    let moved = write(&core, "", json!({"Auth": {"topLeft": xy(0, 40)}}));
    assert_eq!(moved["groups"]["Auth"]["topLeft"], xy(0, 40), "{moved}");
    assert_eq!(
        moved["moved"],
        json!([
            {"card": "Free:A", "topLeft": xy(2, 43)},
            {"card": "Free:B", "topLeft": xy(20, 52)},
            {"group": "Session", "topLeft": xy(1, 50)},
        ]),
        "{moved}"
    );
    assert!(moved.get("shifted").is_none(), "{moved}");
    assert_eq!(at(&core, "", "Free:A"), xy(2, 43));
    assert_eq!(at(&core, "", "Free:C"), xy(25, 2));
    let read = groups(&core, "");
    assert_eq!(read["Session"]["topLeft"], xy(1, 50));
    assert_eq!(read["Auth"]["members"], json!(["Free:A", "Free:B"]));
    // The inner group moved alone moves only what is in it.
    write(&core, "", json!({"Session": {"topLeft": xy(1, 51)}}));
    assert_eq!(at(&core, "", "Free:A"), xy(2, 43));
    // carry: false moves the frame alone; what is in it is counted by the frame where it is now.
    let alone = write(
        &core,
        "",
        json!({"Auth": {"topLeft": xy(100, 100), "carry": false}}),
    );
    assert!(alone.get("moved").is_none(), "{alone}");
    assert_eq!(at(&core, "", "Free:A"), xy(2, 43));
    assert_eq!(
        groups(&core, "")["Auth"]["members"],
        json!([]),
        "nothing is in it where it went"
    );
    // A size alone moves nothing.
    write(&core, "", json!({"Auth": {"size": {"w": 30, "h": 20}}}));
    assert_eq!(at(&core, "", "Free:A"), xy(2, 43));
}

#[test]
fn what_a_group_carries_shifts_together_clear_of_a_box_outside_and_a_frame_blocks_nothing() {
    let core = core();
    with_auth(&core);
    // D sits where A would land; another group's frame lies where B lands.
    ok(
        &core,
        "card_create",
        json!({"target": "Free:D", "value": {"kind": "note", "body": "d", "topLeft": xy(2, 23)}}),
    );
    write(
        &core,
        "",
        json!({"Other": {"topLeft": xy(15, 25), "size": {"w": 20, "h": 10}}}),
    );
    let moved = write(&core, "", json!({"Auth": {"topLeft": xy(0, 20)}}));
    let shifted = moved["shifted"].clone();
    assert!(shifted.is_object(), "{moved}");
    let (dx, dy) = (
        shifted["x"].as_i64().unwrap(),
        shifted["y"].as_i64().unwrap(),
    );
    // Moved as one: the frame and both boxes by the same offset.
    assert_eq!(moved["groups"]["Auth"]["topLeft"], xy(dx, 20 + dy));
    assert_eq!(at(&core, "", "Free:A"), xy(2 + dx, 23 + dy));
    assert_eq!(at(&core, "", "Free:B"), xy(20 + dx, 32 + dy));
    assert_eq!(at(&core, "", "Free:D"), xy(2, 23));
}

#[test]
fn move_takes_groups_by_name_with_the_cards_it_picks() {
    let core = core();
    with_auth(&core);
    let moved = ok(
        &core,
        "section_update",
        json!({"target": "Free", "value": {"move": {"groups": ["auth"], "cards": ["Free:C"], "by": xy(0, 60)}}}),
    );
    assert_eq!(at(&core, "", "Free:A"), xy(2, 63), "{moved}");
    assert_eq!(at(&core, "", "Free:C"), xy(25, 62));
    assert_eq!(groups(&core, "")["Auth"]["topLeft"], xy(0, 60));
    let rows = moved["moved"].as_array().unwrap();
    assert!(
        rows.contains(&json!({"group": "Auth", "topLeft": xy(0, 60)})),
        "{moved}"
    );
    assert!(
        rows.contains(&json!({"card": "Free:C", "topLeft": xy(25, 62)})),
        "{moved}"
    );
    // to: the top left of all of them, the frame among them.
    ok(
        &core,
        "section_update",
        json!({"target": "Free", "value": {"move": {"groups": ["Auth"], "to": xy(10, 100)}}}),
    );
    assert_eq!(groups(&core, "")["Auth"]["topLeft"], xy(10, 100));
    assert_eq!(at(&core, "", "Free:A"), xy(12, 103));
    // A name no group has is refused, nothing moved.
    let said = refused(
        &core,
        "section_update",
        json!({"target": "Free", "value": {"move": {"groups": ["Nope"], "by": xy(1, 0)}}}),
    );
    assert!(said.contains("Nope"), "{said}");
    assert!(said.contains("Auth"), "{said}");
    assert_eq!(at(&core, "", "Free:A"), xy(12, 103));
}

#[test]
fn around_frames_the_boxes_named_with_room_for_the_name() {
    let core = core();
    let made = write(
        &core,
        "",
        json!({"Pair": {"around": ["Free:A", "Free:C"], "headerPlace": "bottomRight"}}),
    );
    let pair = &made["groups"]["Pair"];
    assert_eq!(pair["topLeft"], xy(1, 0), "{made}");
    let read = groups(&core, "");
    let members = read["Pair"]["members"].as_array().unwrap();
    assert_eq!(members[..2], [json!("Free:C"), json!("Free:A")], "{read}");
    assert_eq!(read["Pair"]["headerPlace"], "bottomRight");
    // Right of C's box by one: C's right edge is 25 + 10 - 1.
    assert_eq!(pair["bottomRight"]["x"], 35, "{made}");
    // A box not on this map is refused.
    let said = refused(
        &core,
        "section_update",
        json!({"target": "Free", "value": {"groups": {"Far": {"around": ["Schemas:Customer"]}}}}),
    );
    assert!(said.contains("Schemas:Customer"), "{said}");
}

#[test]
fn a_group_is_renamed_looked_after_and_let_go_of_its_cards_kept() {
    let core = core();
    with_auth(&core);
    let renamed = write(
        &core,
        "",
        json!({"Auth": {"name": "Login", "label": "認証", "fill": "zebraDiagonal", "headerPlace": "topLeft"}}),
    );
    assert!(renamed["groups"]["Login"].is_object(), "{renamed}");
    let read = groups(&core, "");
    assert!(read.get("Auth").is_none(), "{read}");
    // The default corner is not said; the rest is.
    assert_eq!(read["Login"]["label"], "認証");
    assert_eq!(read["Login"]["fill"], "zebraDiagonal");
    assert_eq!(read["Login"]["color"], "blue");
    assert!(read["Login"].get("headerPlace").is_none(), "{read}");
    // null takes a look away, none puts the default back.
    write(
        &core,
        "",
        json!({"Login": {"color": null, "label": null, "fill": "none"}}),
    );
    let read = groups(&core, "");
    for key in ["color", "label", "fill"] {
        assert!(read["Login"].get(key).is_none(), "{key}: {read}");
    }
    // A read group written back as it is changes nothing.
    let back = read["Login"].clone();
    write(&core, "", json!({"Login": back}));
    assert_eq!(groups(&core, ""), read);
    // Let go of: its cards stay.
    let gone = write(&core, "", json!({"login": null}));
    assert_eq!(gone["groups"], json!({"Login": null}), "{gone}");
    assert_eq!(groups(&core, ""), json!({}));
    assert_eq!(at(&core, "", "Free:A"), xy(2, 3));
}

#[test]
fn what_a_group_cannot_be_is_refused_and_nothing_is_written() {
    let core = core();
    with_auth(&core);
    let before = groups(&core, "");
    let cases: Vec<(Value, &str)> = vec![
        (
            json!({"Other": {"topLeft": xy(50, 50)}, "Auth2": {"topLeft": xy(70, 70), "name": "auth"}}),
            "もうあります",
        ),
        (
            json!({"Small": {"topLeft": xy(50, 50), "size": {"w": 5, "h": 3}}}),
            "5×3",
        ),
        (
            json!({"Flat": {"topLeft": xy(50, 50), "size": {"w": 6, "h": 2}}}),
            "6×2",
        ),
        (
            json!({"Huge": {"topLeft": xy(50, 50), "size": {"w": 1001, "h": 3}}}),
            "1001×3",
        ),
        (
            json!({"Pink": {"topLeft": xy(50, 50), "color": "pink"}}),
            "pink",
        ),
        (
            json!({"Hatch": {"topLeft": xy(50, 50), "fill": "hatch"}}),
            "hatch",
        ),
        (
            json!({"Mid": {"topLeft": xy(50, 50), "headerPlace": "middle"}}),
            "middle",
        ),
        (
            json!({"Odd": {"topLeft": xy(50, 50), "shape": "round"}}),
            "shape",
        ),
        (json!({"Nowhere": {"size": {"w": 8, "h": 4}}}), "Nowhere"),
        (
            json!({"Both": {"around": ["Free:A"], "topLeft": xy(0, 0)}}),
            "around",
        ),
        (json!({"Missing": null}), "Missing"),
        (json!({" ": {"topLeft": xy(50, 50)}}), "80"),
        (json!({"x".repeat(81): {"topLeft": xy(50, 50)}}), "80"),
    ];
    for (value, says) in cases {
        let said = refused(
            &core,
            "section_update",
            json!({"target": "Free", "value": {"groups": value}}),
        );
        assert!(said.contains(says), "{value}: {said}");
        assert_eq!(groups(&core, ""), before, "{value}");
    }
    // The same name but for case is the group there, moved (not a second one).
    let moved = write(&core, "", json!({"AUTH": {"topLeft": xy(0, 1)}}));
    assert!(moved["groups"]["Auth"].is_object(), "{moved}");
}

#[test]
fn new_makes_a_group_only_and_a_name_already_there_but_for_case_is_refused_with_nothing_moved() {
    let core = core();
    with_auth(&core);
    let before = groups(&core, "");
    let (a, b) = (at(&core, "", "Free:A"), at(&core, "", "Free:B"));
    for value in [
        json!({"auth": {"topLeft": xy(0, 30), "new": true}}),
        json!({"AUTH": {"around": ["Free:C"], "new": true}}),
    ] {
        let said = refused(
            &core,
            "section_update",
            json!({"target": "Free", "value": {"groups": value}}),
        );
        assert!(
            said.contains("「Auth」") && said.contains("もうあります"),
            "{value}: {said}"
        );
        assert_eq!(groups(&core, ""), before, "{value}");
        assert_eq!(at(&core, "", "Free:A"), a, "{value}");
        assert_eq!(at(&core, "", "Free:B"), b, "{value}");
    }
    // Neither a place nor a frame, or not a yes or no: refused as the other keys are.
    let said = refused(
        &core,
        "section_update",
        json!({"target": "Free", "value": {"groups": {"Billing": {"new": true}}}}),
    );
    assert!(
        said.contains("Billing") && said.contains("topLeft"),
        "{said}"
    );
    let said = refused(
        &core,
        "section_update",
        json!({"target": "Free", "value": {"groups": {"Billing": {"topLeft": xy(40, 0), "new": "yes"}}}}),
    );
    assert!(said.contains("new"), "{said}");
    assert_eq!(groups(&core, ""), before);
    // A name not there is made.
    let made = write(
        &core,
        "",
        json!({"Billing": {"topLeft": xy(40, 0), "new": true}}),
    );
    assert_eq!(
        made["groups"]["Billing"],
        json!({"topLeft": xy(40, 0), "bottomRight": xy(45, 3), "size": {"w": 6, "h": 4}}),
        "{made}"
    );
    assert_eq!(groups(&core, "")["Auth"], before["Auth"]);
}

#[test]
fn a_map_holds_at_most_two_hundred_groups() {
    let core = core();
    let free = section_id(&core, "Free");
    let project = project_id(&core);
    let conn = rusqlite::Connection::open(core.place()).unwrap();
    for n in 0..200 {
        conn.execute(
            "INSERT INTO places (project_id, branch_id, view, item_kind, item, x, y, w, h, pinned) VALUES (?, '', ?, 'group', ?, 0, 0, 6, 3, 1)",
            rusqlite::params![project, free, format!("g{n}")],
        )
        .unwrap();
    }
    let said = refused(
        &core,
        "section_update",
        json!({"target": "Free", "value": {"groups": {"One more": {"topLeft": xy(0, 0)}}}}),
    );
    assert!(said.contains("200"), "{said}");
}

#[test]
fn a_ui_section_and_a_working_directorys_section_hold_no_groups() {
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
    ok(&core, "section_create", json!({"target": "UI/Home"}));
    for section in ["UI/Home", "ts"] {
        let said = refused(
            &core,
            "section_update",
            json!({"target": section, "value": {"groups": {"G": {"topLeft": xy(0, 0)}}}}),
        );
        assert!(said.contains("グループを置けません"), "{section}: {said}");
    }
}

#[test]
fn a_goals_card_map_holds_groups_and_a_card_that_holds_nothing_does_not() {
    let core = core();
    let board = |tool: &str, mut arguments: Value| {
        arguments["branch"] = "_project_board_".into();
        common::mcp(&core, tool, arguments)
    };
    board(
        "card_update",
        json!({"target": "Goals:Ship", "value": {"places": {
            "Goals:Ship.Design": {"topLeft": xy(0, 0)},
            "Goals:Ship.t2": {"topLeft": xy(20, 0)},
        }}}),
    )
    .unwrap();
    let made = board(
        "card_update",
        json!({"target": "Goals:Ship", "value": {"groups": {"Plan": {"around": ["Goals:Ship.Design"], "color": "violet"}}}}),
    )
    .unwrap();
    assert!(made["groups"]["Plan"].is_object(), "{made}");
    let read = board(
        "card_read",
        json!({"target": "Goals:Ship", "value": {"include": ["groups", "places"]}}),
    )
    .unwrap();
    assert_eq!(
        read["groups"]["Plan"]["members"],
        json!(["Goals:Ship.Design"]),
        "{read}"
    );
    let design = read["places"]["Goals:Ship.Design"]["topLeft"].clone();
    let top = read["groups"]["Plan"]["topLeft"].clone();
    // Moved, what is in it moves with it on the card map.
    let moved = board(
        "card_update",
        json!({"target": "Goals:Ship", "value": {"groups": {"Plan": {"topLeft": {"x": top["x"], "y": top["y"].as_i64().unwrap() + 30}}}}}),
    )
    .unwrap();
    assert_eq!(
        moved["moved"][0],
        json!({"card": "Goals:Ship.Design", "topLeft": {"x": design["x"], "y": design["y"].as_i64().unwrap() + 30}}),
        "{moved}"
    );
    // A task holds no card map.
    let said = board(
        "card_update",
        json!({"target": "Goals:Ship.t2", "value": {"groups": {"G": {"topLeft": xy(0, 0)}}}}),
    )
    .unwrap_err();
    assert!(said.contains("グループを置けません"), "{said}");
    let said = board(
        "card_read",
        json!({"target": "Goals:Ship.t2", "value": {"include": ["groups"]}}),
    )
    .unwrap_err();
    assert!(said.contains("groups"), "{said}");
    // The window reads them by the goal's id.
    let ship: String = rusqlite::Connection::open(core.place())
        .unwrap()
        .query_row(
            "SELECT id FROM card_docs WHERE branch_id = '~board' AND name = 'Ship'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let positions = op(
        &core,
        json!({"op": "section_positions", "project": project_id(&core), "branch": "~board"}),
    );
    assert_eq!(
        positions["result"]["groups"][&ship]["Plan"]["color"], "violet",
        "{positions}"
    );
}

#[test]
fn the_window_reads_the_groups_with_the_positions_and_cannot_put_one_as_a_box() {
    let core = core();
    with_auth(&core);
    let project = project_id(&core);
    let free = section_id(&core, "Free");
    let read = op(
        &core,
        json!({"op": "section_positions", "project": project}),
    );
    assert_eq!(
        read["result"]["groups"],
        json!({free.clone(): {"Auth": {"x": 0, "y": 0, "w": 24, "h": 14, "color": "blue"}}}),
        "{read}"
    );
    let put = op(
        &core,
        json!({"op": "places", "project": project, "boxes": [{"view": free, "kind": "group", "item": "Auth", "x": 5, "y": 5}]}),
    );
    assert!(put.get("error").is_some(), "{put}");
    assert!(put.to_string().contains("groups"), "{put}");
    assert_eq!(groups(&core, "")["Auth"]["topLeft"], xy(0, 0));
}

#[test]
fn a_branch_sees_mains_groups_and_keeps_its_own_changes_to_itself() {
    let core = core();
    with_auth(&core);
    write(
        &core,
        "",
        json!({"Old": {"topLeft": xy(60, 0), "size": {"w": 6, "h": 4}}}),
    );
    ok(&core, "branch_create", json!({"target": "work"}));
    assert_eq!(groups(&core, "work"), groups(&core, ""));
    // Moved, renamed and let go of on the branch: main as it was.
    write(&core, "work", json!({"Auth": {"topLeft": xy(0, 40)}}));
    write(&core, "work", json!({"Old": {"name": "Older"}}));
    let on_branch = groups(&core, "work");
    assert_eq!(on_branch["Auth"]["topLeft"], xy(0, 40));
    assert!(on_branch.get("Old").is_none(), "{on_branch}");
    assert!(on_branch["Older"].is_object(), "{on_branch}");
    let main = groups(&core, "");
    assert_eq!(main["Auth"]["topLeft"], xy(0, 0));
    assert!(main["Old"].is_object(), "{main}");
    assert_eq!(at(&core, "", "Free:A"), xy(2, 3));
    // The window does not see what the branch let go of.
    let positions = op(
        &core,
        json!({"op": "section_positions", "project": project_id(&core), "branch": branch_id(&core, "work")}),
    );
    let free = section_id(&core, "Free");
    assert!(
        positions["result"]["groups"][&free].get("Old").is_none(),
        "{positions}"
    );
    // A group main makes later shows through.
    write(&core, "", json!({"Later": {"topLeft": xy(80, 0)}}));
    assert!(groups(&core, "work")["Later"].is_object());
}

fn branch_id(core: &Core, name: &str) -> String {
    rusqlite::Connection::open(core.place())
        .unwrap()
        .query_row("SELECT id FROM branches WHERE name = ?", [name], |r| {
            r.get(0)
        })
        .unwrap()
}

#[test]
fn a_merge_takes_a_branchs_groups_only_with_places_and_what_it_let_go_of_goes() {
    let core = core();
    with_auth(&core);
    write(
        &core,
        "",
        json!({"Old": {"topLeft": xy(60, 0), "size": {"w": 6, "h": 4}}}),
    );
    ok(&core, "branch_create", json!({"target": "work"}));
    write(&core, "work", json!({"Auth": {"topLeft": xy(0, 40)}}));
    write(&core, "work", json!({"Old": null}));
    // A section the branch makes, with a group on its map: it comes with the section.
    ok(
        &core,
        "section_create",
        json!({"branch": "work", "target": "Free/Sub"}),
    );
    ok(
        &core,
        "section_update",
        json!({"branch": "work", "target": "Free/Sub", "value": {"groups": {"Here": {"topLeft": xy(0, 0)}}}}),
    );
    let preview = ok(
        &core,
        "branch_read",
        json!({"target": "work", "value": {"against": "main"}}),
    );
    let merged = ok(
        &core,
        "branch_merge",
        json!({"target": "work", "value": {"keep": true}}),
    );
    // Left out: the group moved, the one let go of and the two cards it carried, one each; the
    // group on the new section's map went in with it and is not counted.
    assert_eq!(merged["placesLeft"], 4, "{merged}");
    let main = groups(&core, "");
    assert_eq!(main["Auth"]["topLeft"], xy(0, 0), "{preview}");
    assert!(main["Old"].is_object(), "{main}");
    let sub = ok(
        &core,
        "section_read",
        json!({"target": "Free/Sub", "value": {"include": ["groups"]}}),
    );
    assert!(sub["groups"]["Here"].is_object(), "{sub}");
    // With its places: the moved group and what it carried go in, the one let go of goes.
    ok(
        &core,
        "branch_merge",
        json!({"target": "work", "value": {"places": true}}),
    );
    let main = groups(&core, "");
    assert_eq!(main["Auth"]["topLeft"], xy(0, 40), "{main}");
    assert!(main.get("Old").is_none(), "{main}");
    assert_eq!(at(&core, "", "Free:A"), xy(2, 43));
}

#[test]
fn a_locked_main_takes_groups_with_no_commit() {
    let core = core();
    let project = project_id(&core);
    let commits = || {
        op(&core, json!({"op": "commits", "project": project}))["result"]["commits"]
            .as_array()
            .unwrap()
            .len()
    };
    let locked = op(
        &core,
        json!({"op": "main_lock", "project": project, "on": true}),
    );
    assert!(locked.get("error").is_none(), "{locked}");
    let before = commits();
    with_auth(&core);
    write(&core, "", json!({"Auth": {"topLeft": xy(0, 30)}}));
    assert_eq!(commits(), before);
    assert_eq!(at(&core, "", "Free:A"), xy(2, 33));
}

#[test]
fn a_diagram_let_go_of_takes_its_groups_with_it() {
    let core = core();
    ok(&core, "section_create", json!({"target": "Free/Sub"}));
    ok(
        &core,
        "section_update",
        json!({"target": "Free/Sub", "value": {"groups": {"G": {"topLeft": xy(0, 0)}}}}),
    );
    let sub = section_id(&core, "Sub");
    assert_eq!(group_rows(&core, &sub), 1);
    ok(&core, "section_delete", json!({"target": "Free/Sub"}));
    assert_eq!(group_rows(&core, &sub), 0);
    // A goal's card map: the goal gone, its groups go.
    ok(
        &core,
        "card_create",
        json!({"value": {"Free:Aim": {"kind": "goal"}, "Free:Aim.Step": {"kind": "task"}}}),
    );
    ok(
        &core,
        "card_update",
        json!({"target": "Free:Aim", "value": {"groups": {"G": {"around": ["Free:Aim.Step"]}}}}),
    );
    let aim: String = rusqlite::Connection::open(core.place())
        .unwrap()
        .query_row(
            "SELECT id FROM card_docs WHERE branch_id = '' AND name = 'Aim'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(group_rows(&core, &aim), 1);
    ok(&core, "card_delete", json!({"target": "Free:Aim"}));
    assert_eq!(group_rows(&core, &aim), 0);
}

#[test]
fn a_card_placed_with_no_place_said_keeps_out_of_a_group() {
    let core = core();
    write(
        &core,
        "",
        json!({"Big": {"topLeft": xy(0, 0), "size": {"w": 60, "h": 40}}}),
    );
    ok(
        &core,
        "card_create",
        json!({"target": "Free:Fresh", "value": {"kind": "note", "body": "new"}}),
    );
    let fresh = at(&core, "", "Free:Fresh");
    let (x, y) = (fresh["x"].as_i64().unwrap(), fresh["y"].as_i64().unwrap());
    assert!(x >= 60 || y >= 40, "{fresh}");
    // Put in the frame by hand, it stays where it was put, in the group.
    let put = ok(
        &core,
        "card_update",
        json!({"target": "Free:Fresh", "value": {"topLeft": xy(40, 30)}}),
    );
    assert!(put.get("warnings").is_none(), "{put}");
    assert_eq!(at(&core, "", "Free:Fresh"), xy(40, 30));
}

#[test]
fn groups_go_with_the_project_file_and_come_back_as_they_were() {
    let core = core();
    write(
        &core,
        "",
        json!({"Auth": {"topLeft": xy(0, 0), "size": {"w": 24, "h": 14}, "color": "amber", "label": "認証"}}),
    );
    ok(&core, "branch_create", json!({"target": "work"}));
    write(&core, "work", json!({"Auth": null}));
    let project = project_id(&core);
    let text = op(&core, json!({"op": "project_file", "project": project}))["result"]["text"]
        .as_str()
        .unwrap()
        .to_string();
    let file: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(file["version"], 33);
    let rows: Vec<&Value> = file["places"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|p| p["item_kind"] == "group")
        .collect();
    assert_eq!(rows.len(), 2, "{rows:?}");
    let main_row = rows.iter().find(|p| p.get("branch_id").is_none()).unwrap();
    assert_eq!(main_row["item"], "Auth");
    assert_eq!(main_row["look"], json!({"label": "認証", "color": "amber"}));
    // The branch's mark of one let go of: a row with no cells.
    let mark = rows.iter().find(|p| p.get("branch_id").is_some()).unwrap();
    assert!(mark.get("x").is_none(), "{mark}");
    let before = groups(&core, "");
    let read = op(
        &core,
        json!({"op": "project_file_read", "text": text, "landing": "replace"}),
    );
    assert!(read.get("error").is_none(), "{read}");
    assert_eq!(groups(&core, ""), before);
    assert_eq!(groups(&core, "work"), json!({}));
    // A file a newer app wrote is refused.
    let mut newer = file.clone();
    newer["version"] = 34.into();
    let refused = op(
        &core,
        json!({"op": "project_file_read", "text": newer.to_string(), "landing": "replace"}),
    );
    assert!(refused.get("error").is_some(), "{refused}");
}

#[test]
fn a_group_past_its_reach_in_a_project_file_is_read_within_it() {
    let core = core();
    write(
        &core,
        "",
        json!({
            "Auth": {"topLeft": xy(0, 0), "size": {"w": 24, "h": 14}},
            "Big": {"topLeft": xy(2, 2), "size": {"w": 8, "h": 6}},
        }),
    );
    let project = project_id(&core);
    let text = op(&core, json!({"op": "project_file", "project": project}))["result"]["text"]
        .as_str()
        .unwrap()
        .to_string();
    let mut file: Value = serde_json::from_str(&text).unwrap();
    for place in file["places"].as_array_mut().unwrap() {
        if place["item_kind"] != "group" {
            continue;
        }
        let (x, y, w, h) = if place["item"] == "Auth" {
            (i64::MAX, i64::MAX, i64::MAX, i64::MAX)
        } else {
            (i64::MIN, i64::MIN, -5, 0)
        };
        for (key, n) in [("x", x), ("y", y), ("w", w), ("h", h)] {
            place[key] = n.into();
        }
    }
    let read = op(
        &core,
        json!({"op": "project_file_read", "text": file.to_string(), "landing": "replace"}),
    );
    assert!(read.get("error").is_none(), "{read}");
    let project = project_id(&core);
    let read = groups(&core, "");
    assert_eq!(read["Auth"]["topLeft"], xy(1_000_000, 1_000_000), "{read}");
    assert_eq!(
        read["Auth"]["size"],
        json!({"w": 1000, "h": 1000}),
        "{read}"
    );
    assert_eq!(read["Big"]["topLeft"], xy(-1_000_000, -1_000_000), "{read}");
    assert_eq!(read["Big"]["size"], json!({"w": 6, "h": 3}), "{read}");
    let free = section_id(&core, "Free");
    let window = op(
        &core,
        json!({"op": "section_positions", "project": project}),
    );
    assert_eq!(
        window["result"]["groups"][&free],
        json!({
            "Auth": {"x": 1_000_000, "y": 1_000_000, "w": 1000, "h": 1000},
            "Big": {"x": -1_000_000, "y": -1_000_000, "w": 6, "h": 3},
        }),
        "{window}"
    );
}

#[test]
fn groups_are_written_before_move_in_one_call_and_what_both_moved_is_said_together() {
    let core = core();
    let said = ok(
        &core,
        "section_update",
        json!({"target": "Free", "value": {
            "groups": {"New": {"topLeft": xy(0, 0), "size": {"w": 24, "h": 14}}},
            "move": {"groups": ["New"], "cards": ["Free:C"], "by": xy(0, 50)},
        }}),
    );
    assert_eq!(said["groups"]["New"]["topLeft"], xy(0, 0), "{said}");
    assert_eq!(groups(&core, "")["New"]["topLeft"], xy(0, 50));
    let moved = said["moved"].as_array().unwrap();
    assert_eq!(moved.len(), 4, "{said}");
    assert_eq!(at(&core, "", "Free:A"), xy(2, 53));
    assert_eq!(at(&core, "", "Free:C"), xy(25, 52));
}

// MARK: - Tidying and framing beside a group

/// The ids of the free section's notes, by name.
fn note_ids(core: &Core) -> HashMap<String, String> {
    let conn = rusqlite::Connection::open(core.place()).unwrap();
    let mut stmt = conn
        .prepare("SELECT name, id FROM card_docs WHERE branch_id = '' AND kind = 'note'")
        .unwrap();
    stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

/// The free section's A made three lines long (10 by 5) and B one (10 by 3), put where they are
/// said as the window puts them, over one another or not; C out of the way.
fn tidy_core(a: (i64, i64), b: (i64, i64)) -> Core {
    let core = core();
    ok(
        &core,
        "card_update",
        json!({"target": "Free:A", "value": {"body": "a\nb\nc"}}),
    );
    let ids = note_ids(&core);
    let said = op(
        &core,
        json!({"op": "places", "project": project_id(&core), "boxes": [
            {"item": ids["A"], "x": a.0, "y": a.1},
            {"item": ids["B"], "x": b.0, "y": b.1},
            {"item": ids["C"], "x": 200, "y": 200},
        ]}),
    );
    assert!(said.get("error").is_none(), "{said}");
    core
}

fn tidy(core: &Core) -> Value {
    ok(
        core,
        "section_update",
        json!({"target": "Free", "value": {"tidy": true}}),
    )
}

#[test]
fn tidy_takes_a_group_as_one_box_and_moves_what_it_holds_with_it() {
    let core = tidy_core((0, 0), (1, 5));
    write(
        &core,
        "",
        json!({"G": {"topLeft": xy(0, 3), "size": {"w": 14, "h": 9}},
               "H": {"topLeft": xy(14, 3), "size": {"w": 6, "h": 4}}}),
    );
    let tidied = tidy(&core);
    let read = groups(&core, "");
    assert_eq!(read["G"]["topLeft"], xy(0, 6));
    assert_eq!(at(&core, "", "Free:B"), xy(1, 8));
    assert_eq!(read["H"]["topLeft"], xy(14, 3));
    assert_eq!(at(&core, "", "Free:A"), xy(0, 0));
    assert_eq!(
        tidied["tidied"],
        json!([{"card": "Free:B", "topLeft": xy(1, 8)}, {"group": "G", "topLeft": xy(0, 6)}])
    );
}

#[test]
fn tidy_grows_a_frame_to_keep_what_moved_inside_it() {
    let core = tidy_core((1, 2), (1, 4));
    write(
        &core,
        "",
        json!({"G": {"topLeft": xy(0, 0), "size": {"w": 12, "h": 8}}}),
    );
    let tidied = tidy(&core);
    assert_eq!(at(&core, "", "Free:B"), xy(1, 8));
    let read = groups(&core, "");
    assert_eq!(read["G"]["topLeft"], xy(0, 0));
    assert_eq!(read["G"]["size"], json!({"w": 12, "h": 13}));
    assert_eq!(read["G"]["members"], json!(["Free:A", "Free:B"]));
    assert_eq!(
        tidied["tidied"],
        json!([{"card": "Free:B", "topLeft": xy(1, 8)},
               {"group": "G", "topLeft": xy(0, 0), "size": {"w": 12, "h": 13}}])
    );
}

#[test]
fn around_beside_another_group_keeps_out_of_its_frame() {
    let core = core();
    ok(&core, "section_create", json!({"target": "Free/Other"}));
    for name in ["Free/Other:A", "Free/Other:B"] {
        ok(
            &core,
            "card_create",
            json!({"target": name, "value": {"kind": "note", "body": "x"}}),
        );
    }
    // In each map, A at (0, 0) and B a cell right of it, both three lines long (10 by 5).
    for section in ["Free", "Free/Other"] {
        let (a, b) = (format!("{section}:A"), format!("{section}:B"));
        for card in [&a, &b] {
            ok(
                &core,
                "card_update",
                json!({"target": card, "value": {"body": "a\nb\nc"}}),
            );
        }
        ok(
            &core,
            "section_update",
            json!({"target": section, "value": {"places": {
                a: {"topLeft": xy(0, 0)},
                b: {"topLeft": xy(11, 0)},
            }}}),
        );
    }
    let left = write(&core, "", json!({"Left": {"around": ["Free:A"]}}));
    assert_eq!(left["groups"]["Left"]["topLeft"], xy(-1, -2));
    assert_eq!(left["groups"]["Left"]["size"], json!({"w": 12, "h": 8}));
    let right = write(&core, "", json!({"Right": {"around": ["Free:B"]}}));
    assert_eq!(right["groups"]["Right"]["topLeft"], xy(11, -2), "{right}");
    assert_eq!(right["groups"]["Right"]["size"], json!({"w": 11, "h": 8}));
    let read = groups(&core, "");
    assert!(read["Left"].get("groups").is_none(), "{read}");
    // Inside a group holding both, the frame keeps out of the one beside and goes into the big one.
    let other = |groups: Value| {
        ok(
            &core,
            "section_update",
            json!({"target": "Free/Other", "value": {"groups": groups}}),
        )
    };
    other(json!({"Big": {"topLeft": xy(-5, -5), "size": {"w": 40, "h": 20}}}));
    other(json!({"Left": {"around": ["Free/Other:A"]}}));
    other(json!({"Right": {"around": ["Free/Other:B"]}}));
    let read = ok(
        &core,
        "section_read",
        json!({"target": "Free/Other", "value": {"include": ["groups"]}}),
    )["groups"]
        .clone();
    assert_eq!(read["Big"]["groups"], json!(["Left", "Right"]), "{read}");
    assert!(read["Left"].get("groups").is_none(), "{read}");
}
