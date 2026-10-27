//! A built-in kind's own fields and rows (a process's `steps`, a goal's `status`, a task's
//! `done`) are named in the history line, so a card written only through them still leaves one,
//! and a batch tried first lists the change.

use crate::common;

use common::{mcp, op, project_id, script_store};
use serde_json::{Value, json};
use specboard_core::Core;

const BOARD: &str = "_project_board_";

fn put(core: &Core, branch: &str, target: &str, value: Value) {
    mcp(
        core,
        "card_update",
        json!({"branch": branch, "target": target, "value": value}),
    )
    .unwrap();
}

fn make(core: &Core, branch: &str, target: &str, value: Value) {
    mcp(
        core,
        "card_create",
        json!({"branch": branch, "target": target, "value": value}),
    )
    .unwrap();
}

fn latest(core: &Core, branch: &str) -> String {
    let read = op(
        core,
        json!({"op": "commits", "project": project_id(core), "branch": match branch { BOARD => "~board", "main" => "", other => other }, "limit": 1}),
    );
    read["result"]["commits"][0]["lines"][0]["summary"]
        .as_str()
        .unwrap()
        .to_string()
}

#[test]
fn a_process_steps_are_named_in_the_history_line() {
    let core = Core::open(script_store()).unwrap();
    make(&core, "main", "Free:Flow", json!({"kind": "flow"}));
    make(&core, "main", "Free:Flow.read", json!({"kind": "process"}));
    put(
        &core,
        "main",
        "Free:Flow.read",
        json!({"steps": [{"do": "read", "description": "読む"}]}),
    );
    assert_eq!(
        latest(&core, "main"),
        "カード Free:Flow.read を更新（欄 steps）"
    );
}

#[test]
fn a_goals_status_and_a_tasks_done_are_named_in_the_history_line() {
    let core = Core::open(script_store()).unwrap();
    make(
        &core,
        BOARD,
        "Goals:Plan",
        json!({"kind": "goal", "label": "目標"}),
    );
    make(&core, BOARD, "Goals:Plan.Step", json!({"kind": "task"}));
    put(&core, BOARD, "Goals:Plan", json!({"status": "inProgress"}));
    assert_eq!(
        latest(&core, BOARD),
        "カード Goals:Plan を更新（欄 status）"
    );
    put(&core, BOARD, "Goals:Plan.Step", json!({"done": true}));
    assert_eq!(
        latest(&core, BOARD),
        "カード Goals:Plan.Step を更新（欄 done）"
    );
}

#[test]
fn a_batch_tried_first_lists_the_change_of_a_built_in_field() {
    let core = Core::open(script_store()).unwrap();
    make(&core, "main", "Free:Flow", json!({"kind": "flow"}));
    make(&core, "main", "Free:Flow.read", json!({"kind": "process"}));
    let tried = mcp(
        &core,
        "batch",
        json!({"project": "見本", "value": {"open": true, "steps": [
            {"tool": "card_update", "arguments": {"target": "Free:Flow.read", "value": {"steps": [{"do": "read"}]}}},
        ]}}),
    )
    .unwrap();
    assert_eq!(
        tried["steps"][0]["changes"],
        json!(["カード Free:Flow.read を更新（欄 steps）"]),
        "{tried}"
    );
}
