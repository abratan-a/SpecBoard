//! What a goal or a task needs, read in one call (`card_read` with `as: context`,
//! `specboard://SpecBoard/Docs/Design:CoreApi`「材料を集める」): the board points at main by id and at
//! a branch's cards only by the paths its bodies write, so the read resolves them in the tree the
//! goal works in — its branch while it is there, else main — and gathers its work, the spec rows
//! those point at, their shapes and the code carrying them out, one hop each, within the room of
//! one answer, saying what it left out with a call that reads it.

use crate::common;

use common::{PROJECT, ok, refused};
use serde_json::{Value, json};
use specboard_core::Core;
use std::fs;
use std::path::Path;

const BOARD: &str = "_project_board_";

const A_RS: &str = "pub struct Order {\n    lines: Vec<u32>,\n}\n\nimpl Order {\n    pub fn total(&self) -> u32 {\n        self.lines.iter().sum()\n    }\n}\n";

fn bind(core: &Core, folder: &Path) {
    let mut tree = ok(core, "project_read", json!({}))["projectTree"].clone();
    tree["main_sections"]
        .as_array_mut()
        .unwrap()
        .push(json!({"type": "workdir", "name": "repo"}));
    ok(
        core,
        "project_update",
        json!({"value": {"project_tree": tree}}),
    );
    let said = common::op(
        core,
        json!({"op": "tool", "name": "section_update", "arguments": {
            "project": PROJECT, "target": "repo", "value": {"folder": folder.to_string_lossy()}
        }}),
    );
    assert!(said.get("error").is_none(), "{said}");
}

/// The goal `Sharing` started (branch `sharing`) with the task `Rules` pointing at a spec item
/// written on the branch alone, whose text points at a shape and which is carried out by a function.
fn sharing() -> Core {
    let core = Core::open(common::script_store()).unwrap();
    let root = common::place("context");
    let file = root.join("src/a.rs");
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    fs::write(file, A_RS).unwrap();
    bind(&core, &root);
    ok(
        &core,
        "card_create",
        json!({"branch": BOARD, "target": "Goals:Sharing", "value": {"kind": "goal", "body": "共有の仕組み"}}),
    );
    ok(
        &core,
        "card_create",
        json!({"branch": BOARD, "target": "Goals:Sharing.Rules", "value": {"kind": "task", "body": "決める: [[Docs:Rules.can_edit]]"}}),
    );
    ok(
        &core,
        "card_action",
        json!({"branch": BOARD, "target": "Goals:Sharing", "op": "start"}),
    );
    ok(
        &core,
        "card_create",
        json!({"branch": "sharing", "target": "Docs:Rules", "value": {"kind": "formatdoc", "rows": [
            {"name": "can_edit", "heading": "編集できる人", "rows": [
                {"name": "text", "kind": "text", "body": "[[Schemas:Customer]] が編集できる"}
            ]}
        ]}}),
    );
    ok(
        &core,
        "card_update",
        json!({"branch": "sharing", "target": "Docs:Rules", "value": {"connections": {"add": [
            {"line": "implementedBy", "from": "can_edit", "to": "repo/src:'a.rs'.Order.total"}
        ]}}}),
    );
    core
}

fn context(core: &Core, target: &str) -> Value {
    ok(
        core,
        "card_read",
        json!({"branch": BOARD, "target": target, "value": {"as": "context"}}),
    )
}

fn paths(said: &Value, layer: &str) -> Vec<String> {
    said[layer]
        .as_array()
        .unwrap_or_else(|| panic!("{layer}: {said}"))
        .iter()
        .map(|e| e["path"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn a_task_reads_the_spec_row_its_shape_and_its_code_on_the_goals_branch() {
    let core = sharing();
    let said = context(&core, "Goals:Sharing.Rules");
    assert_eq!(said["branch"], BOARD, "{said}");
    assert_eq!(said["tree"], "sharing", "{said}");
    assert!(said.get("treeNote").is_none(), "{said}");
    assert_eq!(paths(&said, "work"), ["Goals:Sharing.Rules"]);
    assert_eq!(said["work"][0]["body"], "決める: [[Docs:Rules.can_edit]]");
    assert_eq!(said["goal"]["path"], "Goals:Sharing");
    assert_eq!(said["goal"]["workBranch"], "sharing");
    assert_eq!(paths(&said, "refs"), ["Docs:Rules.can_edit"]);
    let row = &said["refs"][0];
    assert!(
        row["markdown"].as_str().unwrap().contains("編集できる人"),
        "{row}"
    );
    assert_eq!(row["via"], json!(["Goals:Sharing.Rules"]));
    assert_eq!(
        row["lines"],
        json!([{"line": "implementedBy", "to": "repo/src:'a.rs'", "at": "Order.total"}]),
        "{row}"
    );
    assert_eq!(paths(&said, "shapes"), ["Schemas:Customer"]);
    assert_eq!(said["shapes"][0]["via"], json!(["Docs:Rules.can_edit"]));
    assert_eq!(paths(&said, "code"), ["repo/src:'a.rs'.Order.total"]);
    let code = &said["code"][0];
    assert!(
        code["body"].as_str().unwrap().contains("fn total"),
        "{code}"
    );
    assert_eq!(code["language"], "rust", "{code}");
    assert!(said.get("truncated").is_none(), "{said}");
    // The order of the layers, as they are put in.
    let keys: Vec<&str> = said
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .filter(|k| ["work", "refs", "shapes", "code"].contains(k))
        .collect();
    assert_eq!(keys, ["work", "refs", "shapes", "code"]);
}

#[test]
fn a_goal_reads_its_open_tasks_and_names_the_done_ones() {
    let core = sharing();
    ok(
        &core,
        "card_create",
        json!({"branch": BOARD, "target": "Goals:Sharing.Old", "value": {"kind": "task", "done": true, "body": "[[Schemas:Status]]"}}),
    );
    let said = context(&core, "Goals:Sharing");
    assert_eq!(
        paths(&said, "work"),
        ["Goals:Sharing", "Goals:Sharing.Rules"],
        "{said}"
    );
    assert_eq!(said["doneTasks"], json!(["Goals:Sharing.Old"]));
    assert!(said.get("goal").is_none(), "{said}");
    // What the done task points at is not gathered.
    assert!(!said.to_string().contains("Schemas:Status"), "{said}");
    assert_eq!(paths(&said, "refs"), ["Docs:Rules.can_edit"]);
}

#[test]
fn a_goal_whose_branch_was_merged_is_read_on_main_and_says_why() {
    let core = sharing();
    ok(&core, "branch_merge", json!({"target": "sharing"}));
    let said = context(&core, "Goals:Sharing.Rules");
    assert_eq!(said["tree"], "main", "{said}");
    assert!(
        said["treeNote"].as_str().unwrap().contains("sharing"),
        "{said}"
    );
    assert_eq!(paths(&said, "refs"), ["Docs:Rules.can_edit"]);
}

#[test]
fn what_is_nowhere_is_unresolved_and_a_boards_card_is_left_unfollowed() {
    let core = sharing();
    ok(
        &core,
        "card_update",
        json!({"branch": BOARD, "target": "Goals:Sharing.Rules", "value": {"body": "[[Docs:Nothing]] と [[Goals:Sharing]] と [[Docs:Rules.can_edit]]"}}),
    );
    let said = context(&core, "Goals:Sharing.Rules");
    assert_eq!(
        said["unresolved"],
        json!([{"path": "Docs:Nothing", "via": "Goals:Sharing.Rules"}]),
        "{said}"
    );
    let left = said["left"].as_array().unwrap();
    assert_eq!(left.len(), 1, "{said}");
    assert_eq!(left[0]["path"], "Goals:Sharing");
    assert_eq!(left[0]["read"]["arguments"]["branch"], BOARD);
    assert_eq!(paths(&said, "refs"), ["Docs:Rules.can_edit"]);
}

#[test]
fn what_does_not_fit_is_left_with_a_call_that_reads_it() {
    let core = sharing();
    // A long note cut at a line, and many notes more than an answer holds.
    let long: String = (1..=600)
        .map(|n| format!("{n} 行目の長い文。説明を足す"))
        .collect::<Vec<_>>()
        .join("\n");
    let mut made = serde_json::Map::new();
    made.insert(
        "Free:Long".into(),
        json!({"kind": "note", "body": long.clone()}),
    );
    let mut body = String::from("[[Free:Long]]");
    for n in 0..12 {
        made.insert(
            format!("Free:Big{n:02}"),
            json!({"kind": "note", "body": format!("{n} {}", "長い本文。".repeat(1_400))}),
        );
        body.push_str(&format!(" [[Free:Big{n:02}]]"));
    }
    ok(
        &core,
        "card_create",
        json!({"branch": "sharing", "value": made}),
    );
    ok(
        &core,
        "card_update",
        json!({"branch": BOARD, "target": "Goals:Sharing.Rules", "value": {"body": body}}),
    );
    let (_, text) = common::call(
        &core,
        "card_read",
        json!({"project": PROJECT, "branch": BOARD, "target": "Goals:Sharing.Rules", "value": {"as": "context"}}),
    );
    assert!(text.chars().count() <= 40_000, "{}", text.chars().count());
    let said: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(said["truncated"], true, "{said}");
    let long_said = said["refs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["path"] == "Free:Long")
        .unwrap();
    let cut = &long_said["cut"];
    assert_eq!(cut["lineCount"], 600, "{long_said}");
    assert!(long_said["markdown"].as_str().unwrap().chars().count() <= 8_000);
    assert_eq!(cut["read"]["tool"], "card_read");
    assert_eq!(cut["read"]["arguments"]["branch"], "sharing");
    assert!(
        cut["read"]["arguments"]["value"]["lines"]["from"]
            .as_u64()
            .unwrap()
            > 1
    );
    let left = said["left"].as_array().unwrap();
    assert!(!left.is_empty(), "{said}");
    for one in left {
        assert!(one["chars"].as_u64().unwrap() > 0, "{one}");
        assert_eq!(one["read"]["arguments"]["target"], one["path"], "{one}");
    }
}

#[test]
fn it_is_read_on_the_board_of_one_goal_or_task_with_as_alone() {
    let core = sharing();
    let said = refused(
        &core,
        "card_read",
        json!({"target": "Goals:Sharing.Rules", "value": {"as": "context"}}),
    );
    assert!(said.contains("_project_board_"), "{said}");
    let (_, text) = common::call(
        &core,
        "card_read",
        json!({"project": PROJECT, "target": "Goals:Sharing.Rules", "value": {"as": "context"}}),
    );
    let error: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(
        error["error"]["use"]["arguments"]["branch"], BOARD,
        "{error}"
    );
    let said = refused(
        &core,
        "card_read",
        json!({"branch": BOARD, "target": ["Goals:Sharing", "Goals:Sharing.Rules"], "value": {"as": "context"}}),
    );
    assert!(said.contains("2"), "{said}");
    let said = refused(
        &core,
        "card_read",
        json!({"branch": BOARD, "target": "Goals:Sharing", "value": {"as": "context", "context": 3}}),
    );
    assert!(said.contains("context"), "{said}");
    // A board's card that is not a goal or a task: read as a document is.
    ok(
        &core,
        "card_create",
        json!({"branch": BOARD, "target": "Goals:Memo", "value": {"kind": "note", "body": "メモ"}}),
    );
    let said = refused(
        &core,
        "card_read",
        json!({"branch": BOARD, "target": "Goals:Memo", "value": {"as": "context"}}),
    );
    assert!(said.contains("as: markdown"), "{said}");
}

#[test]
fn code_in_a_working_directory_not_read_here_is_left_with_why() {
    let core = Core::open(common::script_store()).unwrap();
    let mut tree = ok(&core, "project_read", json!({}))["projectTree"].clone();
    tree["main_sections"]
        .as_array_mut()
        .unwrap()
        .push(json!({"type": "workdir", "name": "repo"}));
    ok(
        &core,
        "project_update",
        json!({"value": {"project_tree": tree}}),
    );
    ok(
        &core,
        "card_update",
        json!({"target": "Schemas:Customer", "value": {"connections": {"add": [
            {"line": "implementedBy", "to": "repo/src:'customer.rs'"}
        ]}}}),
    );
    ok(
        &core,
        "card_create",
        json!({"branch": BOARD, "target": "Goals:Plain", "value": {"kind": "goal", "body": "[[Schemas:Customer]]"}}),
    );
    let said = context(&core, "Goals:Plain");
    assert_eq!(said["tree"], "main", "{said}");
    // A shape named in the work's body is said with the shapes.
    assert_eq!(paths(&said, "shapes"), ["Schemas:Customer"], "{said}");
    let left = said["left"].as_array().unwrap_or_else(|| panic!("{said}"));
    assert_eq!(left.len(), 1, "{said}");
    assert!(left[0]["why"].is_string(), "{said}");
}
