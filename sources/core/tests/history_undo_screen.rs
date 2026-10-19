//! One commit undone from the window (`undo_commit`, `specboard://SpecBoard/Docs/Design:CoreApi`):
//! the same undo MCP has, for whoever wrote the commit, answered for the window to show. A branch or
//! the board gets a commit of its own; main gets a branch to merge; where a later commit changed what
//! the commit changed too nothing is written and the answer carries the units, which is what MCP's
//! refusal carries.

use crate::common;

use common::{PROJECT, ok, op, project_id, refused, script_store};
use serde_json::{Value, json};
use specboard_core::Core;

/// A tool called as the window calls it.
fn window(core: &Core, tool: &str, mut arguments: Value) {
    arguments["project"] = PROJECT.into();
    let said = op(
        core,
        json!({"op": "tool", "name": tool, "arguments": arguments}),
    );
    assert!(said.get("error").is_none(), "{tool}: {said}");
}

fn branch_id(core: &Core, name: &str) -> String {
    rusqlite::Connection::open(core.place())
        .unwrap()
        .query_row("SELECT id FROM branches WHERE name = ?", [name], |r| {
            r.get(0)
        })
        .unwrap()
}

/// The window's `undo_commit` on a tree (`""` for main), the whole answer.
fn undo(core: &Core, tree: &str, seq: i64) -> Value {
    op(
        core,
        json!({"op": "undo_commit", "project": project_id(core), "branch": tree, "seq": seq}),
    )
}

fn commits(core: &Core, tree: &str) -> Vec<Value> {
    let said = op(
        core,
        json!({"op": "commits", "project": project_id(core), "branch": tree, "limit": 1000}),
    );
    said["result"]["commits"].as_array().unwrap().clone()
}

fn newest(core: &Core, tree: &str) -> Value {
    commits(core, tree)[0].clone()
}

fn card(core: &Core, tree: &str, path: &str) -> Value {
    ok(core, "card_read", json!({"branch": tree, "target": path}))
}

#[test]
fn a_branchs_commit_is_undone_by_a_commit_of_its_own_and_the_one_undone_is_marked() {
    let core = Core::open(script_store()).unwrap();
    ok(&core, "branch_create", json!({"target": "design"}));
    let id = branch_id(&core, "design");
    let was = card(&core, "design", "Schemas:Customer");
    window(
        &core,
        "card_update",
        json!({"branch": "design", "target": "Schemas:Customer", "value": {"description": "まちがい"}}),
    );
    let slip = newest(&core, &id)["seq"].as_i64().unwrap();
    // A later commit elsewhere in the same card is kept.
    ok(
        &core,
        "card_update",
        json!({"branch": "design", "target": "Schemas:Customer", "value": {"label": "残す"}}),
    );
    let before = commits(&core, &id).len();
    let said = undo(&core, &id, slip);
    assert!(said.get("error").is_none(), "{said}");
    let result = &said["result"];
    assert_eq!(result["undone"], slip);
    assert_eq!(result["changed"], json!(["Schemas:Customer"]));
    let now = card(&core, "design", "Schemas:Customer");
    assert_eq!(now["description"], was["description"]);
    assert_eq!(now["label"], "残す");
    assert_eq!(commits(&core, &id).len(), before + 1);
    let top = newest(&core, &id);
    assert_eq!(top["seq"], result["commit"]);
    assert_eq!(top["kind"], "revert");
    assert_eq!(top["authorKind"], "screen");
    let marked = commits(&core, &id)
        .into_iter()
        .find(|c| c["seq"] == slip)
        .unwrap();
    assert_eq!(marked["reverted"], true);
}

#[test]
fn a_commit_mcp_wrote_and_the_newest_commit_are_undone_from_the_window_too() {
    let core = Core::open(script_store()).unwrap();
    ok(&core, "branch_create", json!({"target": "design"}));
    let id = branch_id(&core, "design");
    let was = card(&core, "design", "Schemas:Customer");
    ok(
        &core,
        "card_update",
        json!({"branch": "design", "target": "Schemas:Customer", "value": {"description": "AI が変えた"}}),
    );
    let newest_seq = newest(&core, &id)["seq"].as_i64().unwrap();
    let said = undo(&core, &id, newest_seq);
    assert!(said.get("error").is_none(), "{said}");
    assert_eq!(
        card(&core, "design", "Schemas:Customer")["description"],
        was["description"]
    );
    // The line names the writer of the commit undone, not the window.
    let line = newest(&core, &id)["lines"][0]["summary"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(line.contains(&format!("コミット {newest_seq}")), "{line}");
    assert!(line.contains(common::KEY), "{line}");
}

#[test]
fn a_unit_a_later_commit_changed_too_is_answered_as_a_result_with_nothing_written() {
    let core = Core::open(script_store()).unwrap();
    ok(&core, "branch_create", json!({"target": "design"}));
    let id = branch_id(&core, "design");
    ok(
        &core,
        "card_update",
        json!({"branch": "design", "target": "Schemas:Customer", "value": {"description": "一度目", "label": "名"}}),
    );
    let first = newest(&core, &id)["seq"].as_i64().unwrap();
    ok(
        &core,
        "card_update",
        json!({"branch": "design", "target": "Schemas:Customer", "value": {"description": "二度目"}}),
    );
    let second = newest(&core, &id)["seq"].as_i64().unwrap();
    let before = commits(&core, &id).len();
    let said = undo(&core, &id, first);
    assert!(said.get("error").is_none(), "{said}");
    let result = &said["result"];
    assert_eq!(result["later"], json!([second]));
    let rows = result["conflicts"].as_array().unwrap();
    assert_eq!(rows.len(), 1, "{result}");
    assert_eq!(rows[0]["path"], "Schemas:Customer");
    assert_eq!(rows[0]["unit"], "description");
    assert_eq!(rows[0]["after"], "一度目");
    assert_eq!(rows[0]["now"], "二度目");
    assert!(rows[0].get("before").is_some(), "{result}");
    assert!(result.get("undone").is_none());
    assert_eq!(commits(&core, &id).len(), before);
    assert_eq!(card(&core, "design", "Schemas:Customer")["label"], "名");
    // MCP, refused the same undo, is told the same units.
    let refusal = refused(
        &core,
        "branch_revert",
        json!({"target": "design", "value": {"undo": first}}),
    );
    let shown: Value = serde_json::from_str(&refusal[refusal.find('{').unwrap()..]).unwrap();
    assert_eq!(shown["conflicts"], result["conflicts"]);
    assert_eq!(shown["later"], result["later"]);
}

#[test]
fn mains_commit_is_undone_in_a_branch_made_for_it_locked_or_not() {
    let core = Core::open(script_store()).unwrap();
    let was = card(&core, "main", "Schemas:Customer")["description"].clone();
    window(
        &core,
        "card_update",
        json!({"target": "Schemas:Customer", "value": {"description": "まちがい"}}),
    );
    let slip = newest(&core, "")["seq"].as_i64().unwrap();
    window(
        &core,
        "card_update",
        json!({"target": "Schemas:Customer", "value": {"label": "残す"}}),
    );
    let project = project_id(&core);
    for locked in [false, true] {
        if locked {
            let said = op(
                &core,
                json!({"op": "main_lock", "project": project, "on": true}),
            );
            assert!(said.get("error").is_none(), "{said}");
        }
        let count = commits(&core, "").len();
        let said = undo(&core, "", slip);
        assert!(said.get("error").is_none(), "{said}");
        let result = &said["result"];
        assert_eq!(result["undone"], slip);
        let name = result["name"].as_str().unwrap();
        assert!(name.starts_with(&format!("undo-{slip}")), "{name}");
        if locked {
            // The name taken by the first, the second is told apart.
            assert!(name.ends_with("(2)"), "{name}");
        }
        assert_eq!(result["changed"], json!(["Schemas:Customer"]));
        assert_eq!(commits(&core, "").len(), count);
        assert_eq!(
            card(&core, "main", "Schemas:Customer")["description"],
            "まちがい"
        );
        let made = result["branch"].as_str().unwrap();
        assert_eq!(branch_id(&core, name), made);
        let there = op(
            &core,
            json!({"op": "commits", "project": project, "branch": made}),
        );
        assert!(there.get("error").is_none(), "{there}");
        let in_branch = card(&core, name, "Schemas:Customer");
        assert_eq!(in_branch["description"], was);
        assert_eq!(in_branch["label"], "残す");
        // main's commit is not marked: it is undone only once merged.
        let marked = commits(&core, "")
            .into_iter()
            .find(|c| c["seq"] == slip)
            .unwrap();
        assert_eq!(marked["reverted"], false);
    }
}

#[test]
fn the_boards_commit_is_undone_as_a_branchs_is() {
    let core = Core::open(script_store()).unwrap();
    let board = "_project_board_";
    ok(
        &core,
        "card_create",
        json!({"branch": board, "target": "Goals:Check", "value": {"kind": "task", "label": "確かめる", "body": "前"}}),
    );
    ok(
        &core,
        "card_update",
        json!({"branch": board, "target": "Goals:Check", "value": {"body": "後"}}),
    );
    let seq = newest(&core, "~board")["seq"].as_i64().unwrap();
    let said = undo(&core, "~board", seq);
    assert!(said.get("error").is_none(), "{said}");
    assert_eq!(said["result"]["undone"], seq);
    assert_eq!(card(&core, board, "Goals:Check")["body"], "前");
}

#[test]
fn what_cannot_be_undone_is_refused_with_its_reason_and_nothing_written() {
    let core = Core::open(script_store()).unwrap();
    ok(&core, "branch_create", json!({"target": "work"}));
    let id = branch_id(&core, "work");
    let project = project_id(&core);
    // No number.
    let said = op(
        &core,
        json!({"op": "undo_commit", "project": project, "branch": id}),
    );
    let error = said["error"].as_str().unwrap();
    assert!(
        error.contains("undo_commit") && error.contains("seq"),
        "{said}"
    );
    // A number there is not.
    let said = undo(&core, &id, 9999);
    assert!(said["error"].as_str().unwrap().contains("9999"), "{said}");
    // A commit that wrote no card nor section.
    ok(
        &core,
        "project_update",
        json!({"value": {"description": "説明だけ"}}),
    );
    let settings = newest(&core, "")["seq"].as_i64().unwrap();
    let said = undo(&core, "", settings);
    assert!(
        said["error"]
            .as_str()
            .unwrap()
            .contains("文書を変えていない"),
        "{said}"
    );
    // Undone once, a second time is the form it already is.
    window(
        &core,
        "card_update",
        json!({"branch": "work", "target": "Schemas:Customer", "value": {"label": "一時"}}),
    );
    let slip = newest(&core, &id)["seq"].as_i64().unwrap();
    assert!(undo(&core, &id, slip).get("error").is_none());
    let count = commits(&core, &id).len();
    let said = undo(&core, &id, slip);
    assert!(
        said["error"].as_str().unwrap().contains("今はもう前の形"),
        "{said}"
    );
    assert_eq!(commits(&core, &id).len(), count);
    // A branch there is not.
    let said = undo(&core, "no-such-branch", 1);
    assert!(said.get("error").is_some(), "{said}");
}
