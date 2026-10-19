//! Code a goal's branch names before it is written (`specboard://SpecBoard/Docs/Design:CoreApi`
//! `inspection.planned`): main is what is implemented and a goal's branch is its plan, so on the branch
//! of a goal not done a line to a file or a function not there yet is said as information — the
//! goal's plan — and on main or any other branch as a path to fix. The item is the same one either
//! way (its id), so a mark follows it. When the goal is finished, the lines its branch's cards still
//! have in main to code not there are said in the finish's warnings.

use crate::common;

use common::{checked, ok};
use serde_json::{Value, json};
use specboard_core::Core;
use std::fs;
use std::path::{Path, PathBuf};

const BOARD: &str = "_project_board_";
const ORDER: &str = "Schemas/Ordering:Order";

const A_RS: &str = "pub struct Order {\n    lines: Vec<u32>,\n}\n\nimpl Order {\n    pub fn total(&self) -> u32 {\n        self.lines.iter().sum()\n    }\n}\n";

/// The sample with the working directory `repo` bound to a folder holding `src/a.rs`.
fn with_repository() -> (Core, PathBuf) {
    let core = Core::open(common::script_store()).unwrap();
    let root = common::place("planned");
    let file = root.join("src/a.rs");
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    fs::write(file, A_RS).unwrap();
    bind(&core, &root);
    (core, root)
}

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
            "project": common::PROJECT, "target": "repo", "value": {"folder": folder.to_string_lossy()}
        }}),
    );
    assert!(said.get("error").is_none(), "{said}");
}

/// The goal `Refund` on the board, started: its branch `refund`.
fn started(core: &Core) {
    ok(
        core,
        "card_create",
        json!({"branch": BOARD, "target": "Goals:Refund", "value": {"kind": "goal"}}),
    );
    ok(
        core,
        "card_action",
        json!({"branch": BOARD, "target": "Goals:Refund", "op": "start"}),
    );
}

/// The order's lines to a function and a file not written yet, in `tree`.
fn plan_code(core: &Core, tree: &str) {
    ok(
        core,
        "card_update",
        json!({"branch": tree, "target": ORDER, "value": {"connections": {"add": [
            {"line": "implementedBy", "to": "repo/src:'a.rs'.Order.refund"},
            {"line": "implementedBy", "from": "id", "to": "repo/src:'refund.rs'"}
        ]}}}),
    );
}

/// The check's items of paths not resolving, by the path each names.
fn broken(core: &Core, tree: &str) -> Vec<Value> {
    let mut items: Vec<Value> = checked(core, tree)["brokenPaths"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|i| i["at"].as_str().is_some_and(|a| a.starts_with(ORDER)))
        .collect();
    items.sort_by_key(|i| i["path"].as_str().unwrap_or_default().to_string());
    items
}

#[test]
fn code_not_there_yet_on_a_goals_branch_is_its_plan_and_on_main_a_path_to_fix() {
    let (core, _root) = with_repository();
    started(&core);
    plan_code(&core, "refund");
    let planned = broken(&core, "refund");
    assert_eq!(planned.len(), 2, "{planned:?}");
    for item in &planned {
        assert_eq!(item["kind"], "info", "{item}");
        assert_eq!(item["planned"], "Goals:Refund", "{item}");
        assert!(
            item["problem"].as_str().unwrap().contains("まだありません"),
            "{item}"
        );
    }
    // Listed with what is to fix by default: not there.
    let listed = ok(&core, "inspection_list", json!({"branch": "refund"}));
    assert!(!listed.to_string().contains("refund.rs"), "{listed}");
    // The same lines on main: to fix, and the same items.
    plan_code(&core, "main");
    let fix = broken(&core, "main");
    assert_eq!(fix.len(), 2, "{fix:?}");
    for (fix, planned) in fix.iter().zip(&planned) {
        assert_eq!(fix["kind"], "fix", "{fix}");
        assert!(fix.get("planned").is_none(), "{fix}");
        assert_eq!(fix["id"], planned["id"]);
    }
}

#[test]
fn a_branch_no_goal_works_in_or_a_goal_done_works_in_says_it_to_fix() {
    let (core, _root) = with_repository();
    ok(&core, "branch_create", json!({"target": "loose"}));
    plan_code(&core, "loose");
    let items = broken(&core, "loose");
    assert_eq!(items.len(), 2, "{items:?}");
    assert!(items.iter().all(|i| i["kind"] == "fix"), "{items:?}");
    // A goal naming the branch that is done: not its plan any more.
    ok(
        &core,
        "card_create",
        json!({"branch": BOARD, "target": "Goals:Old", "value": {"kind": "goal", "status": "完了", "workBranch": "loose"}}),
    );
    let items = broken(&core, "loose");
    assert!(items.iter().all(|i| i["kind"] == "fix"), "{items:?}");
}

#[test]
fn a_goal_finished_says_the_lines_its_branch_left_to_code_not_there() {
    let (core, root) = with_repository();
    started(&core);
    plan_code(&core, "refund");
    // A card the branch did not write: not the goal's.
    ok(
        &core,
        "card_update",
        json!({"target": "Schemas:Customer", "value": {"connections": {"add": [
            {"line": "implementedBy", "to": "repo/src:'customer.rs'"}
        ]}}}),
    );
    ok(&core, "branch_merge", json!({"target": "refund"}));
    let done = ok(
        &core,
        "card_action",
        json!({"branch": BOARD, "target": "Goals:Refund", "op": "finish"}),
    );
    let warnings = done["warnings"].to_string();
    assert!(warnings.contains("2 本"), "{done}");
    assert!(warnings.contains("Order.refund"), "{done}");
    assert!(warnings.contains("refund.rs"), "{done}");
    assert!(!warnings.contains("customer.rs"), "{done}");
    assert_eq!(done["path"], "Goals/Complete:Refund", "{done}");

    // Written and read again: nothing left to say.
    ok(
        &core,
        "card_create",
        json!({"branch": BOARD, "target": "Goals:Again", "value": {"kind": "goal"}}),
    );
    ok(
        &core,
        "card_action",
        json!({"branch": BOARD, "target": "Goals:Again", "op": "start"}),
    );
    ok(
        &core,
        "card_update",
        json!({"branch": "again", "target": ORDER, "value": {"label": "注文（返金つき）"}}),
    );
    ok(&core, "branch_merge", json!({"target": "again"}));
    fs::write(
        root.join("src/a.rs"),
        A_RS.replace(
            "    pub fn total",
            "    pub fn refund(&self) -> u32 {\n        0\n    }\n\n    pub fn total",
        ),
    )
    .unwrap();
    fs::write(root.join("src/refund.rs"), "pub fn refund() {}\n").unwrap();
    ok(
        &core,
        "section_action",
        json!({"target": "repo", "op": "read"}),
    );
    let done = ok(
        &core,
        "card_action",
        json!({"branch": BOARD, "target": "Goals:Again", "op": "finish"}),
    );
    assert!(done.get("warnings").is_none(), "{done}");
}
