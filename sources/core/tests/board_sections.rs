//! The board's sections for what is finished (`complete`) and what was cancelled (`cancelled`):
//! one of each inside every goals section directly under the board's root, found by their kind, so
//! they may be renamed but are made, moved and deleted by the core alone
//! (`specboard://SpecBoard/Docs/Design:DesignBoard` §9).

use crate::common;

use common::{op, project_file};
use serde_json::{Value, json};
use specboard_core::Core;

const BOARD: &str = "_project_board_";

/// The board's sections of the sample as `section_list` reads them: each path and its kind.
fn sections(core: &Core) -> Vec<(String, String)> {
    common::ok(core, "section_list", json!({"branch": BOARD}))["sections"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| {
            (
                s["section"].as_str().unwrap().to_string(),
                s["type"].as_str().unwrap_or_default().to_string(),
            )
        })
        .collect()
}

fn pairs(list: &[(&str, &str)]) -> Vec<(String, String)> {
    list.iter()
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .collect()
}

#[test]
fn the_sections_for_what_is_finished_and_cancelled_are_the_cores_to_make_move_and_delete() {
    let core = Core::open(common::script_store()).unwrap();
    assert_eq!(
        sections(&core),
        pairs(&[
            ("Board", ""),
            ("Goals", "goals"),
            ("Goals/Complete", "complete"),
            ("Goals/Cancelled", "cancelled")
        ])
    );
    // A second one is not made: the kind comes from where a section is made, and it is not one a
    // call names.
    let said = common::refused(
        &core,
        "section_create",
        json!({"branch": BOARD, "target": "Goals/Done", "value": {"type": "complete"}}),
    );
    assert!(said.contains("type"), "{said}");
    // What is made inside Goals is a goals section, and inside Complete too.
    common::ok(
        &core,
        "section_create",
        json!({"branch": BOARD, "target": "Goals/Later"}),
    );
    common::ok(
        &core,
        "section_create",
        json!({"branch": BOARD, "target": "Goals/Complete/2026"}),
    );
    // Not moved out of Goals, nor deleted.
    let said = common::refused(
        &core,
        "section_update",
        json!({"branch": BOARD, "target": "Goals/Complete", "value": {"parent": "Goals/Later"}}),
    );
    assert!(said.contains("card_action"), "{said}");
    let said = common::refused(
        &core,
        "section_delete",
        json!({"branch": BOARD, "target": "Goals/Cancelled"}),
    );
    assert!(said.contains("card_action"), "{said}");
    // Renamed, it is still the one of its kind.
    common::ok(
        &core,
        "section_update",
        json!({"branch": BOARD, "target": "Goals/Complete", "value": {"name": "Done"}}),
    );
    assert_eq!(
        sections(&core),
        pairs(&[
            ("Board", ""),
            ("Goals", "goals"),
            ("Goals/Done", "complete"),
            ("Goals/Done/2026", "goals"),
            ("Goals/Cancelled", "cancelled"),
            ("Goals/Later", "goals")
        ])
    );
}

/// A file of before (version 23): its Goals given the two sections, a goals section of its own named
/// for one turned into it with what it holds; written again, the file is of the version of now.
#[test]
fn a_project_file_of_before_has_its_goals_given_the_sections() {
    let mut file: Value = serde_json::from_str(&project_file("sample")).unwrap();
    file["version"] = 23.into();
    let board = file["board"]["sections"].as_array_mut().unwrap();
    board.retain(|s| s["kind"] != "complete" && s["kind"] != "cancelled");
    let goals = board.iter().find(|s| s["name"] == "Goals").unwrap()["id"].clone();
    board.push(
        json!({"id": "s-completed", "parent_id": goals, "name": "completed", "kind": "goals"}),
    );
    let core = Core::create(common::place("board-sections-file.sqlite")).unwrap();
    let read = op(
        &core,
        json!({"op": "project_file_read", "text": file.to_string()}),
    );
    assert!(read.get("error").is_none(), "{read}");
    assert_eq!(
        sections(&core),
        pairs(&[
            ("Board", ""),
            ("Goals", "goals"),
            ("Goals/completed", "complete"),
            ("Goals/Cancelled", "cancelled")
        ])
    );
    let id = common::project_id(&core);
    let again: Value = serde_json::from_str(
        op(&core, json!({"op": "project_file", "project": id}))["result"]["text"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(again["version"], 28);
    let kinds: Vec<&str> = again["board"]["sections"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|s| s["kind"].as_str())
        .collect();
    assert_eq!(kinds, ["goals", "complete", "cancelled"]);
}

/// A goals section put directly under the board's root by the project's tree holds the two as well.
#[test]
fn a_goals_section_put_under_the_boards_root_is_given_the_sections() {
    let core = Core::open(common::script_store()).unwrap();
    let tree = common::ok(&core, "project_read", json!({}))["projectTree"].clone();
    let mut tree = tree;
    tree["board_sections"]
        .as_array_mut()
        .unwrap()
        .push(json!({"type": "goals", "name": "Ideas"}));
    common::ok(
        &core,
        "project_update",
        json!({"value": {"project_tree": tree}}),
    );
    let listed = sections(&core);
    for wanted in [
        ("Ideas/Complete", "complete"),
        ("Ideas/Cancelled", "cancelled"),
    ] {
        assert!(
            listed.contains(&(wanted.0.to_string(), wanted.1.to_string())),
            "{listed:?}"
        );
    }
}
