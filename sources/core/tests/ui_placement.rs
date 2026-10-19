//! Where parts sit in a ui section (`specboard://SpecBoard/Docs/Design:CardKinds`「UI のセクションとはめ込み」
//! 「置き方」): a mock needs parts edge to edge and one over another, so a ui section keeps a
//! position as written, with no margin and no jump, while every other section keeps its margin.

use crate::common;

use common::{checked, ok, script_store};
use serde_json::{Value, json};
use specboard_core::Core;

/// The sample with a ui section `UI/Home` under a ui section `UI`, and the free section `Free/Lab`.
fn core() -> Core {
    let core = Core::open(script_store()).unwrap();
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
    ok(&core, "section_create", json!({"target": "Free/Lab"}));
    core
}

fn parts(core: &Core, section: &str, kind: &str) {
    ok(
        core,
        "card_create",
        json!({"value": {
            format!("{section}:A"): {"kind": kind, "body": "a"},
            format!("{section}:B"): {"kind": kind, "body": "b"},
            format!("{section}:C"): {"kind": kind, "body": "c"},
        }}),
    );
}

fn places(core: &Core, section: &str) -> Value {
    ok(
        core,
        "section_read",
        json!({"target": section, "value": {"include": ["places"]}}),
    )["places"]
        .clone()
}

fn top_left(core: &Core, path: &str) -> Value {
    ok(core, "card_read", json!({"target": path}))["topLeft"].clone()
}

/// Three parts: B against A's right edge, C over both.
fn put_side_by_side_and_over(core: &Core, section: &str) -> Value {
    ok(
        core,
        "section_update",
        json!({"target": section, "value": {"places": {
            format!("{section}:A"): {"topLeft": {"x": 0, "y": 0}, "size": {"w": 20, "h": 6}},
            format!("{section}:B"): {"topLeft": {"x": 20, "y": 0}, "size": {"w": 20, "h": 6}},
            format!("{section}:C"): {"topLeft": {"x": 15, "y": 2}, "size": {"w": 10, "h": 3}},
        }}}),
    )
}

#[test]
fn parts_of_a_ui_section_stay_edge_to_edge_and_over_one_another() {
    let core = core();
    parts(&core, "UI/Home", "html");
    let answer = put_side_by_side_and_over(&core, "UI/Home");
    assert_eq!(answer["placed"], 3, "{answer}");
    assert!(answer.get("jumped").is_none(), "{answer}");
    let boxes = places(&core, "UI/Home");
    for (name, at) in [
        ("A", json!({"x": 0, "y": 0})),
        ("B", json!({"x": 20, "y": 0})),
        ("C", json!({"x": 15, "y": 2})),
    ] {
        assert_eq!(boxes[format!("UI/Home:{name}")]["topLeft"], at, "{boxes}");
    }
}

#[test]
fn the_same_places_in_a_free_section_still_jump() {
    let core = core();
    parts(&core, "Free/Lab", "note");
    let answer = put_side_by_side_and_over(&core, "Free/Lab");
    assert!(
        answer["jumped"].as_array().is_some_and(|j| !j.is_empty()),
        "{answer}"
    );
}

#[test]
fn a_part_made_or_updated_over_another_in_a_ui_section_is_put_there_without_a_warning() {
    let core = core();
    parts(&core, "UI/Home", "html");
    ok(
        &core,
        "card_update",
        json!({"target": "UI/Home:A", "value": {"topLeft": {"x": 0, "y": 0}}}),
    );
    // Against A's edge, then right over it.
    for (at, name) in [
        (json!({"x": 10, "y": 0}), "B"),
        (json!({"x": 2, "y": 1}), "C"),
    ] {
        let answer = ok(
            &core,
            "card_update",
            json!({"target": format!("UI/Home:{name}"), "value": {"topLeft": at}}),
        );
        assert!(answer.get("warnings").is_none(), "{answer}");
        assert_eq!(top_left(&core, &format!("UI/Home:{name}")), at);
    }
    let made = ok(
        &core,
        "card_create",
        json!({"target": "UI/Home:Badge", "value": {"kind": "svg", "body": "<svg/>", "topLeft": {"x": 1, "y": 1}}}),
    );
    assert!(made.get("warnings").is_none(), "{made}");
    assert_eq!(top_left(&core, "UI/Home:Badge"), json!({"x": 1, "y": 1}));
    // The same in a free section moves the second one off the first.
    parts(&core, "Free/Lab", "note");
    ok(
        &core,
        "card_update",
        json!({"target": "Free/Lab:A", "value": {"topLeft": {"x": 0, "y": 0}}}),
    );
    let answer = ok(
        &core,
        "card_update",
        json!({"target": "Free/Lab:B", "value": {"topLeft": {"x": 2, "y": 1}}}),
    );
    assert!(answer.get("warnings").is_some(), "{answer}");
}

#[test]
fn boxes_moved_in_a_ui_section_go_exactly_where_asked() {
    let core = core();
    parts(&core, "UI/Home", "html");
    put_side_by_side_and_over(&core, "UI/Home");
    let moved = ok(
        &core,
        "section_update",
        json!({"target": "UI/Home", "value": {"move": {"cards": ["UI/Home:C"], "to": {"x": 0, "y": 0}}}}),
    );
    assert!(moved.get("shifted").is_none(), "{moved}");
    assert_eq!(top_left(&core, "UI/Home:C"), json!({"x": 0, "y": 0}));
}

#[test]
fn the_inspection_leaves_the_overlaps_of_a_ui_section_out_and_still_says_a_free_ones() {
    let core = core();
    for (section, kind) in [("UI/Home", "html"), ("Free/Lab", "note")] {
        parts(&core, section, kind);
        for (name, y) in [("A", 0), ("B", 10)] {
            ok(
                &core,
                "card_update",
                json!({"target": format!("{section}:{name}"), "value": {"topLeft": {"x": 0, "y": y}}}),
            );
        }
        // A grown over B.
        ok(
            &core,
            "card_update",
            json!({"target": format!("{section}:A"), "value": {"size": {"w": 20, "h": 16}}}),
        );
    }
    let overlaps = checked(&core, "main")["overlaps"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let in_section = |section: &str| overlaps.iter().filter(|o| o["section"] == section).count();
    assert_eq!(in_section("UI/Home"), 0, "{overlaps:?}");
    assert!(in_section("Free/Lab") > 0, "{overlaps:?}");
}

#[test]
fn the_guide_of_a_frame_says_its_target_as_a_section_and_a_ghosts_as_a_card() {
    let core = core();
    let frame = ok(&core, "guide", json!({"about": "frame"})).to_string();
    assert!(frame.contains("UI のセクションのパス"), "{frame}");
    assert!(!frame.contains("kind: ghost"), "{frame}");
    let ghost = ok(&core, "guide", json!({"about": "ghost"})).to_string();
    assert!(ghost.contains("kind: ghost"), "{ghost}");
    assert!(!ghost.contains("UI のセクションのパス"), "{ghost}");
}
