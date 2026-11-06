//! A merge between branches compares from the common ancestor of the two: the older version of main
//! either side's copy came from, not the copy written first; and what it leaves in a branch that sees
//! an older main says the same version it was mixed with (`specboard://SpecBoard/Docs/Design:DesignBranches` §6).

use crate::common;

use common::{ok, op, script_store};
use serde_json::{Value, json};
use specboard_core::Core;

const ORDER: &str = "Schemas/Ordering:Order";

fn core() -> Core {
    Core::open(script_store()).unwrap()
}

fn read(core: &Core, branch: &str, path: &str) -> Value {
    ok(core, "card_read", json!({"branch": branch, "target": path}))
}

#[test]
fn an_older_base_is_the_older_main_version_whatever_was_written_first() {
    let core = core();
    ok(&core, "branch_create", json!({"target": "early"}));
    ok(
        &core,
        "card_update",
        json!({"target": ORDER, "value": {"description": "main で後から"}}),
    );
    ok(&core, "branch_create", json!({"target": "late"}));
    ok(
        &core,
        "card_update",
        json!({"branch": "late", "target": ORDER, "value": {"tags": ["late"]}}),
    );
    ok(
        &core,
        "card_update",
        json!({"branch": "early", "target": ORDER, "value": {"label": "early の名"}}),
    );
    ok(
        &core,
        "branch_merge",
        json!({"target": "early", "value": {"into": "late"}}),
    );
    let order = read(&core, "late", ORDER);
    assert_eq!(order["label"], "early の名", "{order}");
    assert_eq!(order["tags"], json!(["late"]), "{order}");
    assert_eq!(order["description"], "main で後から", "{order}");
}

#[test]
fn a_merge_into_a_branch_that_sees_an_older_main_keeps_mains_change() {
    let core = core();
    ok(&core, "branch_create", json!({"target": "s"}));
    ok(&core, "branch_create", json!({"target": "t"}));
    ok(
        &core,
        "card_update",
        json!({"target": ORDER, "value": {"description": "main で後から"}}),
    );
    ok(
        &core,
        "card_update",
        json!({"branch": "s", "target": ORDER, "value": {"label": "s の名"}}),
    );
    ok(
        &core,
        "branch_merge",
        json!({"target": "s", "value": {"into": "t"}}),
    );
    ok(&core, "branch_merge", json!({"target": "t"}));
    let main = read(&core, "main", ORDER);
    assert_eq!(main["label"], "s の名", "{main}");
    assert_eq!(main["description"], "main で後から", "{main}");
}

#[test]
fn a_merge_that_comes_out_as_main_shows_as_main_in_the_target() {
    let core = core();
    ok(&core, "branch_create", json!({"target": "s"}));
    ok(&core, "branch_create", json!({"target": "t"}));
    ok(
        &core,
        "card_update",
        json!({"target": ORDER, "value": {"label": "X"}}),
    );
    ok(
        &core,
        "card_update",
        json!({"branch": "s", "target": ORDER, "value": {"label": "X"}}),
    );
    ok(
        &core,
        "branch_merge",
        json!({"target": "s", "value": {"into": "t"}}),
    );
    assert_eq!(read(&core, "t", ORDER)["label"], "X");
}

#[test]
fn a_card_main_made_after_the_target_is_no_conflict() {
    let core = core();
    ok(&core, "branch_create", json!({"target": "t"}));
    ok(
        &core,
        "card_create",
        json!({"target": "Schemas:Voucher", "value": {"kind": "entity"}}),
    );
    ok(&core, "branch_create", json!({"target": "s"}));
    ok(
        &core,
        "card_update",
        json!({"branch": "s", "target": "Schemas:Voucher", "value": {"label": "s の名"}}),
    );
    let preview = ok(
        &core,
        "branch_read",
        json!({"target": "s", "value": {"against": "t"}}),
    );
    assert!(preview.get("conflicts").is_none(), "{preview}");
}

#[test]
fn a_line_label_is_numbered_among_its_own_name_only() {
    let core = core();
    ok(
        &core,
        "card_create",
        json!({"target": format!("{ORDER}.kindx"), "value": {"type": "string"}}),
    );
    ok(
        &core,
        "card_update",
        json!({"target": ORDER, "value": {
        "connections": {"add": [
            {"line": "link", "from": "kindx", "to": "Schemas:Customer"},
            {"line": "link", "from": "kind", "to": "Schemas:Customer"}
        ]}}}),
    );
    ok(&core, "branch_create", json!({"target": "s"}));
    ok(&core, "branch_create", json!({"target": "t"}));
    for (branch, said) in [("s", "s の言い方"), ("t", "t の言い方")] {
        ok(
            &core,
            "card_update",
            json!({"branch": branch, "target": ORDER, "value": {"connections": {
                "remove": [{"line": "link", "from": "kind", "to": "Schemas:Customer"}],
                "add": [{"line": "link", "from": "kind", "to": "Schemas:Customer", "description": said}]
            }}}),
        );
    }
    let preview = ok(
        &core,
        "branch_read",
        json!({"target": "s", "value": {"against": "t"}}),
    );
    let said = preview["conflicts"].to_string();
    assert!(said.contains("connections.link.kind\""), "{preview}");
    assert!(!said.contains("connections.link.kind#2"), "{preview}");
}

#[test]
fn a_name_the_target_still_sees_is_an_overlap() {
    let core = core();
    ok(&core, "branch_create", json!({"target": "t"}));
    ok(
        &core,
        "card_update",
        json!({"target": ORDER, "value": {"name": "Purchase"}}),
    );
    ok(&core, "branch_create", json!({"target": "s"}));
    ok(
        &core,
        "card_create",
        json!({"branch": "s", "target": ORDER, "value": {"kind": "entity"}}),
    );
    let preview = ok(
        &core,
        "branch_read",
        json!({"target": "s", "value": {"against": "t"}}),
    );
    assert!(
        preview["overlaps"]
            .as_array()
            .is_some_and(|o| !o.is_empty()),
        "{preview}"
    );
}

#[test]
fn a_frozen_copy_after_taking_main_in_is_as_new_as_that() {
    let core = core();
    ok(&core, "branch_create", json!({"target": "t"}));
    let describe = |said: &str| {
        ok(
            &core,
            "card_update",
            json!({"target": ORDER, "value": {"description": said}}),
        );
    };
    describe("a");
    ok(&core, "branch_create", json!({"target": "s"}));
    ok(
        &core,
        "card_update",
        json!({"branch": "s", "target": ORDER, "value": {"tags": ["s"]}}),
    );
    describe("b");
    ok(
        &core,
        "branch_merge",
        json!({"target": "main", "value": {"into": "t"}}),
    );
    describe("c");
    ok(
        &core,
        "card_update",
        json!({"branch": "t", "target": ORDER, "value": {"label": "t の名"}}),
    );
    ok(
        &core,
        "branch_merge",
        json!({"target": "s", "value": {"into": "t"}}),
    );
    let order = read(&core, "t", ORDER);
    assert_eq!(order["description"], "b", "{order}");
    assert_eq!(order["tags"], json!(["s"]), "{order}");
}

#[test]
fn a_field_the_target_never_touched_is_no_conflict() {
    let core = core();
    ok(&core, "branch_create", json!({"target": "t"}));
    ok(
        &core,
        "card_update",
        json!({"target": ORDER, "value": {"description": "m"}}),
    );
    ok(&core, "branch_create", json!({"target": "s"}));
    ok(
        &core,
        "card_update",
        json!({"branch": "s", "target": ORDER, "value": {"description": "s"}}),
    );
    let preview = ok(
        &core,
        "branch_read",
        json!({"target": "s", "value": {"against": "t"}}),
    );
    assert!(preview.get("conflicts").is_none(), "{preview}");
}

#[test]
fn going_back_on_main_is_not_undone_by_a_merge_between_branches() {
    let core = core();
    let project = common::project_id(&core);
    let window = |request: Value| {
        let said = op(&core, request);
        assert!(said.get("error").is_none(), "{said}");
        said
    };
    ok(
        &core,
        "card_update",
        json!({"target": ORDER, "value": {"description": "1"}}),
    );
    // The window puts main right and gives it up: main goes back to "1". bb is made while main
    // holds "2", and copies it; cc after, and copies "1".
    window(json!({"op": "group_begin", "project": project}));
    window(json!({"op": "tool", "name": "card_update", "arguments": {
        "project": common::PROJECT, "target": ORDER, "value": {"description": "2"}}}));
    ok(&core, "branch_create", json!({"target": "bb"}));
    ok(
        &core,
        "card_update",
        json!({"branch": "bb", "target": ORDER, "value": {"tags": ["bb"]}}),
    );
    window(json!({"op": "group_abandon", "project": project}));
    assert_eq!(read(&core, "main", ORDER)["description"], "1");
    ok(&core, "branch_create", json!({"target": "cc"}));
    ok(
        &core,
        "card_update",
        json!({"branch": "cc", "target": ORDER, "value": {"label": "cc の名"}}),
    );
    ok(
        &core,
        "branch_merge",
        json!({"target": "bb", "value": {"into": "cc"}}),
    );
    let order = read(&core, "cc", ORDER);
    assert_eq!(order["description"], "1", "{order}");
}

fn describe(core: &Core, said: &str) {
    ok(
        core,
        "card_update",
        json!({"target": ORDER, "value": {"description": said}}),
    );
}

/// The target sees an older main frozen, the source copied a newer one: what the target keeps is
/// on the newer, so main going back after it is not undone when the target goes into main.
#[test]
fn a_source_on_a_newer_main_than_the_target_sees_leaves_that_base() {
    let core = core();
    describe(&core, "1");
    ok(&core, "branch_create", json!({"target": "t"}));
    describe(&core, "2");
    ok(&core, "branch_create", json!({"target": "s"}));
    ok(
        &core,
        "card_update",
        json!({"branch": "s", "target": ORDER, "value": {"label": "S"}}),
    );
    describe(&core, "1");
    ok(
        &core,
        "branch_merge",
        json!({"target": "s", "value": {"into": "t"}}),
    );
    ok(&core, "branch_merge", json!({"target": "t"}));
    let order = read(&core, "main", ORDER);
    assert_eq!(order["label"], "S", "{order}");
    assert_eq!(order["description"], "1", "{order}");
}

/// The same, main going on instead: the target into main is no conflict.
#[test]
fn a_source_on_a_newer_main_than_the_target_sees_is_no_conflict_later() {
    let core = core();
    describe(&core, "1");
    ok(&core, "branch_create", json!({"target": "t"}));
    describe(&core, "2");
    ok(&core, "branch_create", json!({"target": "s"}));
    ok(
        &core,
        "card_update",
        json!({"branch": "s", "target": ORDER, "value": {"label": "S"}}),
    );
    describe(&core, "3");
    ok(
        &core,
        "branch_merge",
        json!({"target": "s", "value": {"into": "t"}}),
    );
    let preview = ok(
        &core,
        "branch_read",
        json!({"target": "t", "value": {"against": "main"}}),
    );
    assert!(preview.get("conflicts").is_none(), "{preview}");
}

/// The newest commit of a tree's history, by its number.
fn newest(core: &Core, tree: &str) -> i64 {
    ok(
        core,
        "branch_read",
        json!({"target": tree, "value": {"as": "history", "limit": 1}}),
    )["commits"][0]["seq"]
        .as_i64()
        .unwrap()
}

#[test]
fn going_back_past_a_merge_that_came_out_as_main_brings_back_what_the_branch_saw() {
    let core = core();
    ok(&core, "branch_create", json!({"target": "s"}));
    ok(&core, "branch_create", json!({"target": "t"}));
    let was = read(&core, "t", ORDER)["label"].clone();
    ok(
        &core,
        "card_update",
        json!({"target": ORDER, "value": {"label": "X"}}),
    );
    ok(
        &core,
        "card_update",
        json!({"branch": "s", "target": ORDER, "value": {"label": "X"}}),
    );
    let before = newest(&core, "t");
    ok(
        &core,
        "branch_merge",
        json!({"target": "s", "value": {"into": "t"}}),
    );
    assert_eq!(read(&core, "t", ORDER)["label"], "X");
    ok(
        &core,
        "branch_revert",
        json!({"target": "t", "value": {"to": before}}),
    );
    assert_eq!(read(&core, "t", ORDER)["label"], was);
}

#[test]
fn going_back_past_taking_main_in_brings_back_what_the_branch_saw() {
    let core = core();
    ok(&core, "branch_create", json!({"target": "t"}));
    let was = read(&core, "t", ORDER)["label"].clone();
    ok(
        &core,
        "card_update",
        json!({"target": ORDER, "value": {"label": "X"}}),
    );
    let before = newest(&core, "t");
    ok(
        &core,
        "branch_merge",
        json!({"target": "main", "value": {"into": "t"}}),
    );
    assert_eq!(read(&core, "t", ORDER)["label"], "X");
    ok(
        &core,
        "branch_revert",
        json!({"target": "t", "value": {"to": before}}),
    );
    assert_eq!(read(&core, "t", ORDER)["label"], was);
    // Going back on going back: main as it is now again.
    let reverted = newest(&core, "t");
    ok(
        &core,
        "branch_revert",
        json!({"target": "t", "value": {"to": reverted - 1}}),
    );
    assert_eq!(read(&core, "t", ORDER)["label"], "X");
}

#[test]
fn undoing_one_merge_brings_back_the_frozen_copy() {
    let core = core();
    ok(&core, "branch_create", json!({"target": "s"}));
    ok(&core, "branch_create", json!({"target": "t"}));
    let was = read(&core, "t", ORDER)["label"].clone();
    ok(
        &core,
        "card_update",
        json!({"target": ORDER, "value": {"label": "X"}}),
    );
    ok(
        &core,
        "card_update",
        json!({"branch": "s", "target": ORDER, "value": {"label": "X"}}),
    );
    ok(
        &core,
        "branch_merge",
        json!({"target": "s", "value": {"into": "t"}}),
    );
    let merged = newest(&core, "t");
    ok(
        &core,
        "branch_revert",
        json!({"target": "t", "value": {"undo": merged}}),
    );
    assert_eq!(read(&core, "t", ORDER)["label"], was);
}

/// Two branches s and t, main and s both naming Order X, s merged into t (which lets t's frozen
/// copy of Order go, as t comes out as main): the label t had before, and the merge's number.
fn merged_as_main(core: &Core) -> (Value, i64) {
    ok(core, "branch_create", json!({"target": "s"}));
    ok(core, "branch_create", json!({"target": "t"}));
    let was = read(core, "t", ORDER)["label"].clone();
    ok(
        core,
        "card_update",
        json!({"target": ORDER, "value": {"label": "X"}}),
    );
    ok(
        core,
        "card_update",
        json!({"branch": "s", "target": ORDER, "value": {"label": "X"}}),
    );
    ok(
        core,
        "branch_merge",
        json!({"target": "s", "value": {"into": "t"}}),
    );
    (was, newest(core, "t"))
}

#[test]
fn undoing_one_merge_after_taking_main_in_again_leaves_main_as_it_is_now() {
    let core = core();
    let (_, merged) = merged_as_main(&core);
    ok(
        &core,
        "card_update",
        json!({"target": ORDER, "value": {"label": "Y"}}),
    );
    ok(
        &core,
        "branch_merge",
        json!({"target": "main", "value": {"into": "t"}}),
    );
    assert_eq!(read(&core, "t", ORDER)["label"], "Y");
    let _ = common::mcp(
        &core,
        "branch_revert",
        json!({"target": "t", "value": {"undo": merged}}),
    );
    assert_eq!(read(&core, "t", ORDER)["label"], "Y");
}

#[test]
fn going_back_past_taking_main_in_says_main_moved_again() {
    let core = core();
    ok(&core, "branch_create", json!({"target": "t"}));
    ok(
        &core,
        "card_update",
        json!({"target": ORDER, "value": {"label": "X"}}),
    );
    let moved = |core: &Core| -> Value {
        ok(core, "branch_list", json!({}))["branches"]
            .as_array()
            .unwrap()
            .iter()
            .find(|b| b["branch"] == "t")
            .map(|b| b["mainMoved"].clone())
            .unwrap_or(Value::Null)
    };
    assert_eq!(moved(&core), 1);
    let before = newest(&core, "t");
    ok(
        &core,
        "branch_merge",
        json!({"target": "main", "value": {"into": "t"}}),
    );
    ok(
        &core,
        "branch_revert",
        json!({"target": "t", "value": {"to": before}}),
    );
    assert_eq!(moved(&core), 1);
}

#[test]
fn undoing_a_going_back_alone_lets_the_frozen_copy_go_again() {
    let core = core();
    let (was, merged) = merged_as_main(&core);
    ok(
        &core,
        "branch_revert",
        json!({"target": "t", "value": {"to": merged - 1}}),
    );
    assert_eq!(read(&core, "t", ORDER)["label"], was);
    let reverted = newest(&core, "t");
    ok(
        &core,
        "branch_revert",
        json!({"target": "t", "value": {"undo": reverted}}),
    );
    assert_eq!(read(&core, "t", ORDER)["label"], "X");
}

#[test]
fn undoing_taking_main_in_alone_brings_back_what_the_branch_saw() {
    let core = core();
    ok(&core, "branch_create", json!({"target": "t"}));
    let was = read(&core, "t", ORDER)["label"].clone();
    ok(
        &core,
        "card_update",
        json!({"target": ORDER, "value": {"label": "X"}}),
    );
    ok(
        &core,
        "branch_merge",
        json!({"target": "main", "value": {"into": "t"}}),
    );
    let taken = newest(&core, "t");
    ok(
        &core,
        "branch_revert",
        json!({"target": "t", "value": {"undo": taken}}),
    );
    assert_eq!(read(&core, "t", ORDER)["label"], was);
}
