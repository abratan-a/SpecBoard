//! A goal's and a task's actions (`card_action`, `specboard://SpecBoard/Docs/Design:DesignBoard` §9):
//! each written in one transaction — the branch made or let go of, the card's fields, the card moved
//! to where finished or cancelled cards are kept — or nothing written at all; what a read says can be
//! used now is what the action itself decides.

use crate::common;

use common::{PROJECT, mcp, ok, op, refused};
use serde_json::{Value, json};
use specboard_core::Core;

const BOARD: &str = "_project_board_";

fn sample() -> Core {
    Core::open(common::script_store()).unwrap()
}

/// A goal made on the board directly in Goals.
fn goal(core: &Core, name: &str, value: Value) {
    let mut value = value;
    value["kind"] = "goal".into();
    ok(
        core,
        "card_create",
        json!({"branch": BOARD, "target": format!("Goals:{name}"), "value": value}),
    );
}

fn action(core: &Core, target: &str, op: &str, value: Value) -> Result<Value, String> {
    mcp(
        core,
        "card_action",
        json!({"branch": BOARD, "target": target, "op": op, "value": value}),
    )
}

/// A refusal as MCP answers it: `{message, guide?, use?}`.
fn refusal(core: &Core, target: &str, op: &str, value: Value) -> Value {
    let (refused, said) = common::call(
        core,
        "card_action",
        json!({"project": PROJECT, "branch": BOARD, "target": target, "op": op, "value": value}),
    );
    assert!(refused, "{said}");
    serde_json::from_str::<Value>(&said).unwrap()["error"].clone()
}

/// A card of the board's fields as `card_read` reads them.
fn read(core: &Core, path: &str) -> Value {
    ok(core, "card_read", json!({"branch": BOARD, "target": path}))
}

fn branches(core: &Core) -> Vec<String> {
    ok(core, "branch_list", json!({}))["branches"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["branch"].as_str().unwrap().to_string())
        .collect()
}

/// The newest commit's lines of a tree (`""` main, a branch's name, or the board).
fn last_lines(core: &Core, branch: &str) -> Vec<String> {
    let project = common::project_id(core);
    let tree = match branch {
        "" => String::new(),
        BOARD => "~board".to_string(),
        name => rusqlite::Connection::open(core.place())
            .unwrap()
            .query_row(
                "SELECT id FROM branches WHERE project_id = ? AND name = ?",
                [&project, name],
                |r| r.get(0),
            )
            .unwrap(),
    };
    let request = json!({"op": "commits", "project": project, "branch": tree, "limit": 1});
    let said = op(core, request);
    said["result"]["commits"][0]["lines"]
        .as_array()
        .unwrap_or_else(|| panic!("{said}"))
        .iter()
        .map(|l| l["summary"].as_str().unwrap().to_string())
        .collect()
}

/// The card's actions as a read says them: `(op, available)`.
fn available(core: &Core, path: &str) -> Vec<(String, bool)> {
    ok(
        core,
        "card_read",
        json!({"branch": BOARD, "target": path, "value": {"include": ["actions"]}}),
    )["actions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| {
            (
                a["op"].as_str().unwrap().to_string(),
                a["available"].as_bool().unwrap(),
            )
        })
        .collect()
}

fn of(list: &[(&str, bool)]) -> Vec<(String, bool)> {
    list.iter().map(|(o, a)| (o.to_string(), *a)).collect()
}

#[test]
fn a_goal_started_has_its_branch_made_and_is_in_progress() {
    let core = sample();
    goal(&core, "McpPictures", json!({}));
    assert_eq!(
        available(&core, "Goals:McpPictures"),
        of(&[("start", true), ("finish", true), ("cancel", true)])
    );
    let read_actions = ok(
        &core,
        "card_read",
        json!({"branch": BOARD, "target": "Goals:McpPictures", "value": {"include": ["actions"]}}),
    );
    assert_eq!(
        read_actions["actions"][0]["suggestedBranch"],
        "mcp-pictures"
    );
    assert_eq!(read_actions["actions"][0]["label"], "開始");
    let done = action(&core, "Goals:McpPictures", "start", json!({})).unwrap();
    assert_eq!(done["branch"], BOARD);
    assert_eq!(done["path"], "Goals:McpPictures");
    assert_eq!(done["op"], "start");
    assert_eq!(done["branchCreated"], "mcp-pictures");
    assert!(done["rev"].is_string(), "{done}");
    assert!(done.get("movedTo").is_none());
    assert_eq!(
        done["changes"],
        json!([
            "branch mcp-pictures を作った",
            "status: inProgress",
            "workBranch: mcp-pictures"
        ])
    );
    let card = read(&core, "Goals:McpPictures");
    assert_eq!(card["status"], "inProgress");
    assert_eq!(card["workBranch"], "mcp-pictures");
    assert!(branches(&core).contains(&"mcp-pictures".to_string()));
    // One line on the board for the action, and the branch's own.
    assert_eq!(
        last_lines(&core, BOARD),
        [
            "カード Goals:McpPictures を開始（branch mcp-pictures を作った / status: inProgress / workBranch: mcp-pictures）"
        ]
    );
    assert_eq!(
        last_lines(&core, "mcp-pictures"),
        ["branch mcp-pictures を作成"]
    );
    // In progress with its branch there: not started again.
    assert_eq!(
        available(&core, "Goals:McpPictures"),
        of(&[("start", false), ("finish", true), ("cancel", true)])
    );
    let said = refusal(&core, "Goals:McpPictures", "start", json!({}));
    assert!(
        said["message"]
            .as_str()
            .unwrap()
            .contains("mcp-pictures で作業中"),
        "{said}"
    );
}

#[test]
fn a_start_whose_branch_cannot_be_made_writes_nothing() {
    let core = sample();
    goal(&core, "Refunds", json!({}));
    // `refunds` is the sample's branch already.
    let said = refused(
        &core,
        "card_action",
        json!({"branch": BOARD, "target": "Goals:Refunds", "op": "start"}),
    );
    assert!(said.contains("refunds"), "{said}");
    let said = refused(
        &core,
        "card_action",
        json!({"branch": BOARD, "target": "Goals:Refunds", "op": "start", "value": {"branch": "a/b"}}),
    );
    assert!(said.contains("/"), "{said}");
    let card = read(&core, "Goals:Refunds");
    assert!(card.get("status").is_none(), "{card}");
    assert!(card.get("workBranch").is_none(), "{card}");
    assert_eq!(branches(&core), ["refunds"]);
    // Named another way, it goes.
    let done = action(
        &core,
        "Goals:Refunds",
        "start",
        json!({"branch": "refunds-2"}),
    )
    .unwrap();
    assert_eq!(done["branchCreated"], "refunds-2");
    // In progress, its branch gone: started again.
    goal(
        &core,
        "Again",
        json!({"status": "inProgress", "workBranch": "gone-away"}),
    );
    assert_eq!(
        available(&core, "Goals:Again")[0],
        ("start".to_string(), true)
    );
    let done = action(&core, "Goals:Again", "start", json!({})).unwrap();
    assert_eq!(done["branchCreated"], "again");
}

#[test]
fn a_goal_finished_lets_go_of_its_branch_and_moves_with_what_it_holds() {
    let core = sample();
    goal(&core, "Launch", json!({}));
    for (name, done) in [("Plan", true), ("Build", false)] {
        ok(
            &core,
            "card_create",
            json!({"branch": BOARD, "target": format!("Goals:Launch.{name}"), "value": {"kind": "task", "done": done}}),
        );
    }
    action(&core, "Goals:Launch", "start", json!({})).unwrap();
    let done = action(&core, "Goals:Launch", "finish", json!({})).unwrap();
    assert_eq!(done["path"], "Goals/Complete:Launch");
    assert_eq!(done["movedTo"], "Goals/Complete");
    assert_eq!(done["branchDeleted"], "launch");
    assert_eq!(
        done["warnings"],
        json!(["済んでいない task が 1 件あります: Goals:Launch.Build"])
    );
    let card = read(&core, "Goals/Complete:Launch");
    assert_eq!(card["status"], "done");
    assert!(card.get("workBranch").is_none(), "{card}");
    assert_eq!(
        card["cards"],
        json!(["Goals/Complete:Launch.Plan", "Goals/Complete:Launch.Build"])
    );
    assert!(!branches(&core).contains(&"launch".to_string()));
    assert_eq!(
        last_lines(&core, BOARD),
        [
            "カード Goals:Launch を完了（branch launch を消した / status: done / workBranch を空に / Goals/Complete へ移した）"
        ]
    );
    assert_eq!(last_lines(&core, ""), ["branch launch を削除"]);
    // Finished: nothing more to do with it.
    assert_eq!(
        available(&core, "Goals/Complete:Launch"),
        of(&[("start", false), ("finish", false), ("cancel", false)])
    );
    let said = refusal(&core, "Goals/Complete:Launch", "cancel", json!({}));
    let message = said["message"].as_str().unwrap();
    assert!(message.contains("状態が完了なので"), "{message}");
    assert!(message.contains("今使えるもの: なし"), "{message}");
}

#[test]
fn a_goal_whose_branch_has_changes_is_not_finished_until_they_are_merged() {
    let core = sample();
    goal(&core, "Orders", json!({}));
    action(&core, "Goals:Orders", "start", json!({})).unwrap();
    ok(
        &core,
        "card_create",
        json!({"branch": "orders", "target": "Free:WorkMemo", "value": {"kind": "note", "body": "メモ"}}),
    );
    assert_eq!(
        available(&core, "Goals:Orders"),
        of(&[("start", false), ("finish", false), ("cancel", true)])
    );
    let said = refusal(&core, "Goals:Orders", "finish", json!({}));
    assert!(
        said["message"]
            .as_str()
            .unwrap()
            .contains("main へ入っていない変更が 1 件"),
        "{said}"
    );
    assert_eq!(
        said["use"],
        json!({"tool": "branch_merge", "arguments": {"target": "orders", "project": PROJECT}})
    );
    // Refused, nothing was written: the goal and its branch as they were.
    assert_eq!(read(&core, "Goals:Orders")["status"], "inProgress");
    assert!(branches(&core).contains(&"orders".to_string()));
    // Merged, the branch is set aside: finished, the field emptied, nothing to delete.
    ok(&core, "branch_merge", json!({"target": "orders"}));
    let done = action(&core, "Goals:Orders", "finish", json!({})).unwrap();
    assert!(done.get("branchDeleted").is_none(), "{done}");
    assert_eq!(done["movedTo"], "Goals/Complete");
    assert!(
        read(&core, "Goals/Complete:Orders")
            .get("workBranch")
            .is_none()
    );
}

#[test]
fn a_goal_cancelled_throws_its_changes_away_only_when_told() {
    let core = sample();
    goal(&core, "Coupons", json!({}));
    action(&core, "Goals:Coupons", "start", json!({})).unwrap();
    ok(
        &core,
        "card_create",
        json!({"branch": "coupons", "target": "Free:WorkMemo", "value": {"kind": "note", "body": "メモ"}}),
    );
    let listed = ok(
        &core,
        "card_read",
        json!({"branch": BOARD, "target": "Goals:Coupons", "value": {"include": ["actions"]}}),
    );
    assert_eq!(listed["actions"][2]["discardChanges"], 1);
    let said = refusal(&core, "Goals:Coupons", "cancel", json!({}));
    assert_eq!(said["use"]["arguments"]["value"], json!({"discard": true}));
    assert_eq!(read(&core, "Goals:Coupons")["status"], "inProgress");
    let done = action(&core, "Goals:Coupons", "cancel", json!({"discard": true})).unwrap();
    assert_eq!(done["path"], "Goals/Cancelled:Coupons");
    assert_eq!(done["branchDeleted"], "coupons");
    assert_eq!(
        read(&core, "Goals/Cancelled:Coupons")["status"],
        "cancelled"
    );
    assert!(!branches(&core).contains(&"coupons".to_string()));
    // Not started, no branch: cancelled and moved.
    goal(&core, "Idea", json!({}));
    let done = action(&core, "Goals:Idea", "cancel", json!({})).unwrap();
    assert_eq!(done["movedTo"], "Goals/Cancelled");
    assert_eq!(
        done["changes"],
        json!(["status: cancelled", "Goals/Cancelled へ移した"])
    );
}

#[test]
fn a_task_done_outside_a_goal_moves_and_inside_one_stays() {
    let core = sample();
    ok(
        &core,
        "card_create",
        json!({"branch": BOARD, "target": "Goals:Fix", "value": {"kind": "task"}}),
    );
    assert_eq!(available(&core, "Goals:Fix"), of(&[("done", true)]));
    let done = action(&core, "Goals:Fix", "done", json!({})).unwrap();
    assert_eq!(done["path"], "Goals/Complete:Fix");
    assert_eq!(read(&core, "Goals/Complete:Fix")["done"], true);
    assert_eq!(
        last_lines(&core, BOARD),
        ["カード Goals:Fix を済みに（done: true / Goals/Complete へ移した）"]
    );
    // Inside a goal it stays where it is.
    let done = action(&core, "Goals:Ship.t2", "done", json!({})).unwrap();
    assert_eq!(done["path"], "Goals:Ship.t2");
    assert!(done.get("movedTo").is_none());
    assert_eq!(available(&core, "Goals:Ship.t2"), of(&[("done", false)]));
    let said = refusal(&core, "Goals:Ship.t2", "done", json!({}));
    assert!(
        said["message"]
            .as_str()
            .unwrap()
            .contains("もう済んでいます"),
        "{said}"
    );
}

#[test]
fn what_the_card_cannot_do_is_refused_with_what_it_can() {
    let core = sample();
    // An op its kind has not.
    let said = refusal(&core, "Goals:Ship.t2", "start", json!({}));
    assert_eq!(
        said["message"],
        "task のアクションは done です（start はありません。今使えるもの: done）"
    );
    // One card at a time.
    let said = refused(
        &core,
        "card_action",
        json!({"branch": BOARD, "target": ["Goals:Ship", "Goals:Ship.t2"], "op": "finish"}),
    );
    assert!(said.contains("target"), "{said}");
    let said = refused(
        &core,
        "card_action",
        json!({"target": "Schemas/Ordering:Order.id", "op": "finish"}),
    );
    assert!(said.contains("カード 1 枚"), "{said}");
    // A card with no actions.
    let said = refused(
        &core,
        "card_action",
        json!({"target": "Schemas/Ordering:Order", "op": "done"}),
    );
    assert!(said.contains("アクションはありません"), "{said}");
    // A key the op does not take.
    let said = refused(
        &core,
        "card_action",
        json!({"branch": BOARD, "target": "Goals:Ship", "op": "finish", "value": {"branch": "x"}}),
    );
    assert!(said.contains("branch"), "{said}");
    // The board's goal looked for in main: the same call on the board.
    let (refused, said) = common::call(
        &core,
        "card_action",
        json!({"project": PROJECT, "target": "Goals:Ship", "op": "finish"}),
    );
    assert!(refused);
    let said: Value = serde_json::from_str(&said).unwrap();
    assert_eq!(
        said["error"]["use"],
        json!({"tool": "card_action", "arguments": {"project": PROJECT, "target": "Goals:Ship", "op": "finish", "branch": BOARD}})
    );
}

/// What a read says can be used is what the action does: an op said not available is refused with
/// the same reason, and one said available goes through.
#[test]
fn what_a_read_says_is_available_is_what_the_action_decides() {
    let core = sample();
    goal(&core, "Mixed", json!({}));
    action(&core, "Goals:Mixed", "start", json!({})).unwrap();
    ok(
        &core,
        "card_create",
        json!({"branch": "mixed", "target": "Free:WorkMemo", "value": {"kind": "note", "body": "メモ"}}),
    );
    for path in [
        "Goals:Mixed",
        "Goals:Ship",
        "Goals:Ship.t2",
        "Goals:Ship.Design",
    ] {
        let listed = ok(
            &core,
            "card_read",
            json!({"branch": BOARD, "target": path, "value": {"include": ["actions"]}}),
        )["actions"]
            .clone();
        for entry in listed.as_array().unwrap() {
            if entry["available"] == true {
                continue;
            }
            let op = entry["op"].as_str().unwrap();
            let said = refusal(&core, path, op, json!({}));
            let why = entry["why"].as_str().unwrap();
            assert!(
                said["message"].as_str().unwrap().contains(why),
                "{path} {op}: {said} / {why}"
            );
        }
    }
    // Ship is in progress with no branch of its own: finish is said available, and goes.
    let listed = available(&core, "Goals:Ship");
    assert!(listed.contains(&("finish".to_string(), true)), "{listed:?}");
    action(&core, "Goals:Ship", "finish", json!({})).unwrap();
}

#[test]
fn a_renamed_section_for_what_is_finished_is_still_where_goals_go() {
    let core = sample();
    ok(
        &core,
        "section_update",
        json!({"branch": BOARD, "target": "Goals/Complete", "value": {"name": "Done"}}),
    );
    goal(&core, "Renamed", json!({}));
    let done = action(&core, "Goals:Renamed", "finish", json!({})).unwrap();
    assert_eq!(done["movedTo"], "Goals/Done");
    assert_eq!(done["path"], "Goals/Done:Renamed");
}

#[test]
fn an_action_asks_for_the_items_of_what_it_writes() {
    let core = sample();
    goal(&core, "Guarded", json!({}));
    let project = common::project_id(&core);
    let set = |item: &str, state: &str| {
        let said = op(
            &core,
            json!({"op": "mcp_writes_set", "project": project, "item": item, "state": state}),
        );
        assert!(said.get("error").is_none(), "{said}");
    };
    set("branch_create", "deny");
    let said = refused(
        &core,
        "card_action",
        json!({"branch": BOARD, "target": "Goals:Guarded", "op": "start"}),
    );
    assert!(said.contains("branch_create"), "{said}");
    assert!(branches(&core) == ["refunds"]);
    // Cancelling one with no branch touches branch_delete, not branch_create.
    set("branch_delete", "ask");
    let said = refused(
        &core,
        "card_action",
        json!({"branch": BOARD, "target": "Goals:Guarded", "op": "cancel"}),
    );
    assert!(
        said.contains("branch_delete") && said.contains("request"),
        "{said}"
    );
    set("branch_delete", "allow");
    set("board", "deny");
    let said = refused(
        &core,
        "card_action",
        json!({"branch": BOARD, "target": "Goals:Guarded", "op": "cancel"}),
    );
    assert!(said.contains("board"), "{said}");
    set("board", "allow");
    action(&core, "Goals:Guarded", "cancel", json!({})).unwrap();
}

#[test]
fn the_window_runs_an_action_as_mcp_does() {
    let core = sample();
    goal(&core, "Screen", json!({}));
    let done = op(
        &core,
        json!({"op": "tool", "name": "card_action", "arguments": {"project": PROJECT, "branch": BOARD, "target": "Goals:Screen", "op": "start", "value": {"branch": "screen-work"}}}),
    );
    assert_eq!(
        done["result"]["json"]["branchCreated"], "screen-work",
        "{done}"
    );
    let refused = op(
        &core,
        json!({"op": "tool", "name": "card_action", "arguments": {"project": PROJECT, "branch": BOARD, "target": "Goals:Screen", "op": "start"}}),
    );
    assert!(
        refused["error"]
            .as_str()
            .unwrap()
            .contains("screen-work で作業中"),
        "{refused}"
    );
}

#[test]
fn a_kinds_actions_are_read_with_it_and_in_its_guide() {
    let core = sample();
    let goal = ok(&core, "kind_read", json!({"target": "goal"}));
    let ops: Vec<&str> = goal["actions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["op"].as_str().unwrap())
        .collect();
    assert_eq!(ops, ["start", "finish", "cancel"]);
    assert_eq!(goal["actions"][0]["label"], "開始");
    assert_eq!(goal["actions"][0]["value"], json!(["branch"]));
    assert_eq!(goal["actions"][2]["value"], json!(["discard"]));
    let task = ok(&core, "kind_read", json!({"target": "task"}));
    assert_eq!(task["actions"][0]["label"], "済みにする");
    let (refused, guide) = common::call(&core, "guide", json!({"about": "task"}));
    assert!(!refused);
    assert!(guide.contains("済みにする"), "{guide}");
}

/// One transaction: a finish refused at its last write (a goal of the same name where it would go)
/// leaves the branch it would have let go of and the fields it would have written as they were.
#[test]
fn a_finish_refused_at_its_move_writes_nothing() {
    let core = sample();
    ok(
        &core,
        "card_create",
        json!({"branch": BOARD, "target": "Goals/Complete:Twice", "value": {"kind": "goal", "status": "done"}}),
    );
    goal(&core, "Twice", json!({}));
    action(&core, "Goals:Twice", "start", json!({})).unwrap();
    let said = refused(
        &core,
        "card_action",
        json!({"branch": BOARD, "target": "Goals:Twice", "op": "finish"}),
    );
    assert!(said.contains("Twice"), "{said}");
    let card = read(&core, "Goals:Twice");
    assert_eq!(card["status"], "inProgress");
    assert_eq!(card["workBranch"], "twice");
    assert!(branches(&core).contains(&"twice".to_string()));
    assert_eq!(
        last_lines(&core, BOARD),
        [
            "カード Goals:Twice を開始（branch twice を作った / status: inProgress / workBranch: twice）"
        ]
    );
}

/// A list says what a read says of each goal and task, whatever state they are in, and says nothing
/// of the cards that have no actions.
#[test]
fn a_list_says_the_actions_a_read_says() {
    let core = sample();
    goal(&core, "Idle", json!({}));
    goal(&core, "Working", json!({}));
    action(&core, "Goals:Working", "start", json!({})).unwrap();
    goal(&core, "Changed", json!({}));
    action(&core, "Goals:Changed", "start", json!({})).unwrap();
    ok(
        &core,
        "card_create",
        json!({"branch": "changed", "target": "Free:WorkMemo", "value": {"kind": "note", "body": "メモ"}}),
    );
    goal(&core, "Same", json!({}));
    action(&core, "Goals:Same", "start", json!({})).unwrap();
    ok(
        &core,
        "card_create",
        json!({"branch": BOARD, "target": "Goals:Idle.Step", "value": {"kind": "task"}}),
    );
    ok(
        &core,
        "card_create",
        json!({"branch": BOARD, "target": "Goals:Loose", "value": {"kind": "task", "done": true}}),
    );
    ok(
        &core,
        "card_create",
        json!({"branch": BOARD, "target": "Goals:Memo", "value": {"kind": "note", "body": "メモ"}}),
    );
    let listed = ok(
        &core,
        "card_list",
        json!({"branch": BOARD, "value": {"include": ["actions"], "limit": 200}}),
    );
    let cards = listed["cards"].as_array().unwrap();
    let mut compared = 0;
    for card in cards {
        let path = card["path"].as_str().unwrap();
        let read = ok(
            &core,
            "card_read",
            json!({"branch": BOARD, "target": path, "value": {"include": ["actions"]}}),
        );
        assert_eq!(card.get("actions"), read.get("actions"), "{path}");
        if card.get("actions").is_some() {
            compared += 1;
        }
    }
    assert!(compared >= 6, "{compared} of {} compared", cards.len());
    let changed = cards.iter().find(|c| c["path"] == "Goals:Changed").unwrap();
    assert_eq!(changed["actions"][2]["discardChanges"], 1, "{changed}");
    let memo = cards.iter().find(|c| c["path"] == "Goals:Memo").unwrap();
    assert!(memo.get("actions").is_none(), "{memo}");
    // Without the include, no card carries them.
    let plain = ok(&core, "card_list", json!({"branch": BOARD}));
    assert!(
        plain["cards"]
            .as_array()
            .unwrap()
            .iter()
            .all(|c| c.get("actions").is_none())
    );
}

/// The window reads a board longer than one list in pages: each says where the next goes on from, and
/// the pages together are the cards, each with its actions once.
#[test]
fn the_window_reads_the_boards_actions_in_pages() {
    let core = sample();
    for name in ["One", "Two", "Three", "Four", "Five"] {
        goal(&core, name, json!({}));
    }
    ok(
        &core,
        "card_create",
        json!({"branch": BOARD, "target": "Goals:Five.Step", "value": {"kind": "task"}}),
    );
    let page = |after: Option<&str>| {
        let mut value = json!({"include": ["actions"], "limit": 2});
        if let Some(after) = after {
            value["after"] = after.into();
        }
        op(
            &core,
            json!({"op": "tool", "name": "card_list", "arguments": {"project": PROJECT, "branch": BOARD, "value": value}}),
        )["result"]["json"]
            .clone()
    };
    let mut paths: Vec<String> = Vec::new();
    let mut after: Option<String> = None;
    loop {
        let said = page(after.as_deref());
        for card in said["cards"].as_array().unwrap() {
            assert!(card["actions"].is_array(), "{card}");
            paths.push(card["path"].as_str().unwrap().to_string());
        }
        match said["next"].as_str() {
            Some(next) => after = Some(next.to_string()),
            None => break,
        }
    }
    let whole = page_whole(&core);
    assert!(whole.len() > 2 && paths == whole, "{paths:?} / {whole:?}");
}

/// The paths of the board's cards in one list.
fn page_whole(core: &Core) -> Vec<String> {
    op(
        core,
        json!({"op": "tool", "name": "card_list", "arguments": {"project": PROJECT, "branch": BOARD, "value": {"include": ["actions"], "limit": 200}}}),
    )["result"]["json"]["cards"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["path"].as_str().unwrap().to_string())
        .collect()
}
