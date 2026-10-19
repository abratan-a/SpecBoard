//! goal and task read as a list of issues (`card_list`'s `where` and `in`, a goal's
//! progress, goals in the order their labels number them). A goal's tasks are cards inside
//! it: their paths go on from the goal's.

use crate::common;

use common::{call, checked, script_store};
use serde_json::{Value, json};
use specboard_core::Core;

fn list(core: &Core, value: Value) -> Result<Value, String> {
    let (failed, text) = call(
        core,
        "card_list",
        json!({"project": "見本", "branch": "_project_board_", "value": value}),
    );
    if failed {
        return Err(text);
    }
    Ok(serde_json::from_str(&text).unwrap())
}

fn paths(answer: &Value) -> Vec<String> {
    answer["cards"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["path"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn a_goal_says_how_far_it_has_come() {
    let core = Core::open(script_store()).unwrap();
    let goals = list(&core, json!({"kind": "goal"})).unwrap();
    assert_eq!(paths(&goals), ["Goals:Ship"]);
    assert_eq!(goals["cards"][0]["progress"], json!({"done": 1, "of": 2}));
}

#[test]
fn cards_are_picked_by_the_values_of_their_fields() {
    let core = Core::open(script_store()).unwrap();
    // A box not ticked reads as false.
    let open = list(&core, json!({"kind": "task", "fields": {"done": false}})).unwrap();
    assert_eq!(paths(&open), ["Goals:Ship.t2"]);
    // A goal's fields by their names in English.
    let going = list(
        &core,
        json!({"fields": {"status": ["未着手", "進行中"], "due": "2026-10-10"}}),
    )
    .unwrap();
    assert_eq!(paths(&going), ["Goals:Ship"]);
    let none = list(&core, json!({"fields": {"status": "完了"}})).unwrap();
    assert_eq!(none["matched"], 0);
    let refused = list(&core, json!({"fields": ["done"]})).unwrap_err();
    assert!(
        refused.contains("value の fields は object で指定してください"),
        "{refused}"
    );
    let refused = list(&core, json!({"fields": {"status": {"is": "完了"}}})).unwrap_err();
    assert!(
        refused.contains("fields は {欄の名前: 値} です"),
        "{refused}"
    );
    // A name of before is said as it is now, and one taken out as gone.
    let before = list(&core, json!({"fields": {"状態": "進行中"}})).unwrap_err();
    assert!(
        before.contains("goal の欄「状態」は今は status です"),
        "{before}"
    );
    let gone = list(&core, json!({"fields": {"担当": "Claude"}})).unwrap_err();
    assert!(gone.contains("goal の欄「担当」は外しました"), "{gone}");
}

#[test]
fn a_goal_reads_its_fields_in_english_and_keeps_what_was_taken_out() {
    let core = Core::open(script_store()).unwrap();
    let read = |target: &str| {
        let (failed, text) = call(
            &core,
            "card_read",
            json!({"project": "見本", "branch": "_project_board_", "target": target}),
        );
        assert!(!failed, "{text}");
        serde_json::from_str::<Value>(&text).unwrap()
    };
    let ship = read("Goals:Ship");
    assert_eq!(ship["status"], "進行中");
    assert_eq!(ship["due"], "2026-10-10");
    // Who had it is no field of a goal's: its value stays with the card, as 「定義に無い欄」, and
    // goes when it is let go of.
    assert!(ship.get("担当").is_none() && ship.get("状態").is_none());
    assert_eq!(ship["strayFields"], json!({"担当": "Claude"}));
    // The window reads it so too (`project_detail`), the goal's own fields apart from it.
    let project: String = rusqlite::Connection::open(core.place())
        .unwrap()
        .query_row("SELECT id FROM projects WHERE name = '見本'", [], |r| {
            r.get(0)
        })
        .unwrap();
    let detail: Value = serde_json::from_str(&core.call(
        &json!({"op": "project_detail", "project": project, "branch": "~board"}).to_string(),
    ))
    .unwrap();
    let card = detail["result"]["schemas"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "Ship")
        .unwrap()
        .clone();
    assert_eq!(card["strayFields"], json!({"担当": "Claude"}));
    assert_eq!(card["form"]["fields"]["status"], "進行中");
    let summary = checked(&core, "_project_board_");
    let stray = summary["strayFields"].as_array().unwrap();
    assert_eq!(stray.len(), 1, "{summary}");
    assert_eq!(stray[0]["at"], "Goals:Ship", "{summary}");
    assert_eq!(stray[0]["fields"], json!(["担当"]), "{summary}");
    // Written again, it is kept, not refused.
    let (failed, text) = call(
        &core,
        "card_update",
        json!({"project": "見本", "branch": "_project_board_", "target": "Goals:Ship", "value": {"status": "完了", "priority": "高"}}),
    );
    assert!(!failed, "{text}");
    let ship = read("Goals:Ship");
    assert_eq!(
        (&ship["status"], &ship["priority"]),
        (&json!("完了"), &json!("高"))
    );
    assert_eq!(ship["strayFields"], json!({"担当": "Claude"}));
    // A name of before is said as it is now; one taken out, as gone.
    for (value, said) in [
        (
            json!({"状態": "完了"}),
            "goal の欄「状態」は今は status です",
        ),
        (
            json!({"期限": "2026-11-01"}),
            "goal の欄「期限」は今は due です",
        ),
        (json!({"担当": "Claude"}), "goal の欄「担当」は外しました"),
    ] {
        let (failed, text) = call(
            &core,
            "card_update",
            json!({"project": "見本", "branch": "_project_board_", "target": "Goals:Ship", "value": value}),
        );
        assert!(failed && text.contains(said), "{text}");
    }
    let (failed, text) = call(
        &core,
        "card_create",
        json!({"project": "見本", "branch": "_project_board_", "target": "Goals:Ship.t3", "value": {"kind": "task", "label": "急ぐ", "優先度": "高"}}),
    );
    assert!(
        failed && text.contains("task の欄「優先度」は外しました"),
        "{text}"
    );
    let (failed, text) = call(
        &core,
        "card_update",
        json!({"project": "見本", "branch": "_project_board_", "target": "Goals:Ship", "value": {"strayFields": null}}),
    );
    assert!(!failed, "{text}");
    assert!(read("Goals:Ship").get("strayFields").is_none());
}

#[test]
fn what_a_goal_holds_is_read_all_the_way_down() {
    let core = Core::open(script_store()).unwrap();
    let held = list(&core, json!({"in": "Goals:Ship"})).unwrap();
    assert_eq!(paths(&held), ["Goals:Ship.Design", "Goals:Ship.t2"]);
    let left = list(
        &core,
        json!({"in": "Goals:Ship", "fields": {"done": false}}),
    )
    .unwrap();
    assert_eq!(paths(&left), ["Goals:Ship.t2"]);
    let refused = list(&core, json!({"in": "Goals:Ship.t2"})).unwrap_err();
    assert!(
        refused.contains("in は入れ物（goal・flow）のパスです（Goals:Ship.t2 は task）"),
        "{refused}"
    );
    // A goal in it: what that holds too, below it.
    let (failed, text) = call(
        &core,
        "card_create",
        json!({"project": "見本", "branch": "_project_board_", "value": {
            "Goals:Ship.Sub": {"kind": "goal"},
            "Goals:Ship.Sub.deep": {"kind": "task"}}}),
    );
    assert!(!failed, "{text}");
    let held = list(&core, json!({"in": "Goals:Ship"})).unwrap();
    let mut held = paths(&held);
    held.sort();
    assert_eq!(
        held,
        [
            "Goals:Ship.Design",
            "Goals:Ship.Sub",
            "Goals:Ship.Sub.deep",
            "Goals:Ship.t2"
        ]
    );
}

#[test]
fn goals_go_in_the_order_their_labels_number_them() {
    let core = Core::open(script_store()).unwrap();
    for (name, label) in [("Later", "10. あと"), ("First", "2. 先"), ("Next", "9. 次")] {
        let (failed, text) = call(
            &core,
            "card_create",
            json!({"project": "見本", "branch": "_project_board_", "target": format!("Goals:{name}"), "value": {"kind": "goal", "label": label}}),
        );
        assert!(!failed, "{text}");
    }
    let goals = list(&core, json!({"kind": "goal"})).unwrap();
    assert_eq!(
        paths(&goals),
        ["Goals:First", "Goals:Next", "Goals:Later", "Goals:Ship"]
    );
}

#[test]
fn a_goal_says_how_urgent_it_is_when_it_is_said() {
    let core = Core::open(script_store()).unwrap();
    // Not said is not decided: none of the three.
    let unsaid = list(
        &core,
        json!({"kind": "goal", "fields": {"priority": ["高", "中", "低"]}}),
    )
    .unwrap();
    assert_eq!(unsaid["matched"], 0);
    let (failed, text) = call(
        &core,
        "card_update",
        json!({"project": "見本", "branch": "_project_board_", "target": "Goals:Ship", "value": {"priority": "高"}}),
    );
    assert!(!failed, "{text}");
    let urgent = list(&core, json!({"fields": {"priority": "高"}})).unwrap();
    assert_eq!(paths(&urgent), ["Goals:Ship"]);
    assert_eq!(urgent["cards"][0]["priority"], "高");
    let (failed, text) = call(
        &core,
        "card_update",
        json!({"project": "見本", "branch": "_project_board_", "target": "Goals:Ship", "value": {"priority": "最優先"}}),
    );
    assert!(failed);
    assert!(text.contains("priority"), "{text}");
    // A task has no priority: the order it is done in is its place in its goal.
    let (failed, text) = call(
        &core,
        "card_update",
        json!({"project": "見本", "branch": "_project_board_", "target": "Goals:Ship.t2", "value": {"priority": "高"}}),
    );
    assert!(failed, "{text}");
    assert!(text.contains("priority は goal にだけ書けます"), "{text}");
}

/// A project file before 14 kept a goal's fields by their Japanese names: read in, they go by
/// their names in English, said in one commit of the board's after the read; what a goal has no
/// more stays with it, and written again the file is of now (15).
#[test]
fn a_project_file_before_14_has_its_goals_fields_named_in_english() {
    let core = Core::open(script_store()).unwrap();
    let project: String = rusqlite::Connection::open(core.place())
        .unwrap()
        .query_row("SELECT id FROM projects WHERE name = '見本'", [], |r| {
            r.get(0)
        })
        .unwrap();
    let op = |request: Value| -> Value {
        serde_json::from_str(&core.call(&request.to_string())).unwrap()
    };
    let text = op(json!({"op": "project_file", "project": project}))["result"]["text"]
        .as_str()
        .unwrap()
        .to_string();
    let mut file: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(file["version"], 28);
    // As 13 wrote it.
    file["version"] = 13.into();
    let ship = file["board"]["cards"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|c| c["name"] == "Ship")
        .unwrap();
    let fields = ship["fields"].as_object_mut().unwrap();
    let status = fields.shift_remove("status").unwrap();
    let due = fields.shift_remove("due").unwrap();
    fields.insert("状態".into(), status);
    fields.insert("期限".into(), due);
    fields.insert("優先度".into(), "中".into());
    let read =
        op(json!({"op": "project_file_read", "text": file.to_string(), "landing": "replace"}));
    assert!(read.get("error").is_none(), "{read}");
    let (failed, text) = call(
        &core,
        "card_read",
        json!({"project": "見本", "branch": "_project_board_", "target": "Goals:Ship"}),
    );
    assert!(!failed, "{text}");
    let ship: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(
        (&ship["status"], &ship["priority"], &ship["due"]),
        (&json!("進行中"), &json!("中"), &json!("2026-10-10"))
    );
    assert_eq!(ship["strayFields"], json!({"担当": "Claude"}));
    let commits =
        op(json!({"op": "commits", "project": project, "branch": "~board"}))["result"]["commits"]
            .clone();
    let said: Vec<&str> = commits
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["lines"][0]["summary"].as_str().unwrap())
        .collect();
    assert_eq!(
        said,
        [
            "goal 1 枚の欄 状態・優先度・期限 を status・priority・due にした（値はそのまま。Ship）",
            "プロジェクトのファイル（版 13）から読み込んだ"
        ]
    );
    let again: Value = serde_json::from_str(
        op(json!({"op": "project_file", "project": project}))["result"]["text"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(again["version"], 28);
    let ship = again["board"]["cards"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "Ship")
        .unwrap();
    assert!(ship["fields"].get("状態").is_none());
    assert_eq!(ship["fields"]["status"], "進行中");
}

#[test]
fn a_tasks_order_is_its_place_in_its_goal() {
    let core = Core::open(script_store()).unwrap();
    let held = list(&core, json!({"in": "Goals:Ship"})).unwrap();
    assert_eq!(paths(&held), ["Goals:Ship.Design", "Goals:Ship.t2"]);
    let (failed, text) = call(
        &core,
        "card_update",
        json!({"project": "見本", "branch": "_project_board_", "target": "Goals:Ship.t2", "value": {"position": 0}}),
    );
    assert!(!failed, "{text}");
    let held = list(&core, json!({"in": "Goals:Ship"})).unwrap();
    assert_eq!(paths(&held), ["Goals:Ship.t2", "Goals:Ship.Design"]);
}

/// A project file before 15 kept a goal's merge request as its `PR`: read in, it goes by `MR`,
/// said in one commit of the board's after the read, its value as it was.
#[test]
fn a_project_file_before_15_has_its_goals_merge_request_by_mr() {
    let core = Core::open(script_store()).unwrap();
    let project: String = rusqlite::Connection::open(core.place())
        .unwrap()
        .query_row("SELECT id FROM projects WHERE name = '見本'", [], |r| {
            r.get(0)
        })
        .unwrap();
    let op = |request: Value| -> Value {
        serde_json::from_str(&core.call(&request.to_string())).unwrap()
    };
    let text = op(json!({"op": "project_file", "project": project}))["result"]["text"]
        .as_str()
        .unwrap()
        .to_string();
    let mut file: Value = serde_json::from_str(&text).unwrap();
    // As 14 wrote it.
    file["version"] = 14.into();
    let ship = file["board"]["cards"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|c| c["name"] == "Ship")
        .unwrap();
    let fields = ship["fields"].as_object_mut().unwrap();
    fields.shift_remove("MR");
    fields.insert("PR".into(), 3.into());
    let read =
        op(json!({"op": "project_file_read", "text": file.to_string(), "landing": "replace"}));
    assert!(read.get("error").is_none(), "{read}");
    let (failed, text) = call(
        &core,
        "card_read",
        json!({"project": "見本", "branch": "_project_board_", "target": "Goals:Ship"}),
    );
    assert!(!failed, "{text}");
    let ship: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(ship["MR"], 3, "{ship}");
    assert!(ship.get("PR").is_none(), "{ship}");
    let commits =
        op(json!({"op": "commits", "project": project, "branch": "~board"}))["result"]["commits"]
            .clone();
    let said: Vec<&str> = commits
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["lines"][0]["summary"].as_str().unwrap())
        .collect();
    assert_eq!(
        said,
        [
            "goal 1 枚の欄 PR を MR にした（値はそのまま。Ship）",
            "プロジェクトのファイル（版 14）から読み込んだ"
        ]
    );
    // Written by the name of before, refused with how it is written now.
    let (failed, text) = call(
        &core,
        "card_update",
        json!({"project": "見本", "branch": "_project_board_", "target": "Goals:Ship", "value": {"PR": 4}}),
    );
    assert!(failed);
    assert!(text.contains("今は MR"), "{text}");
}

/// How tall a goal's box is, as the screen draws it: a row for each choice — state and priority,
/// written or not — one for each other field written (期限・branch・MR), one for each card inside it.
#[test]
fn a_goal_is_as_tall_as_the_screen_draws_it() {
    let core = Core::open(script_store()).unwrap();
    let call_ok = |tool: &str, arguments: Value| {
        let (failed, text) = call(&core, tool, arguments.clone());
        assert!(!failed, "{tool} {arguments}: {text}");
        serde_json::from_str::<Value>(&text).unwrap_or(Value::Null)
    };
    let board = |arguments: Value| {
        let mut arguments = arguments;
        arguments["project"] = "見本".into();
        arguments["branch"] = "_project_board_".into();
        arguments
    };
    call_ok(
        "card_create",
        board(json!({"target": "Goals:Sized", "value": {"kind": "goal", "due": "2026-10-31"}})),
    );
    for task in ["a", "b"] {
        call_ok(
            "card_create",
            board(json!({"target": format!("Goals:Sized.{task}"), "value": {"kind": "task"}})),
        );
    }
    let rows = || {
        let read = call_ok(
            "section_read",
            board(json!({"target": "Goals", "value": {"include": ["places"]}})),
        );
        let place = read["places"]["Goals:Sized"].clone();
        place["bottomRight"]["y"].as_i64().unwrap() - place["topLeft"]["y"].as_i64().unwrap() + 1
    };
    // Five rows (状態・優先度・期限・a・b): 2 + 40 + 12 + 5 × 27 points, 7 grid rows of 27.
    assert_eq!(rows(), 7);
    // A choice written is still the one row; a branch written is one more.
    call_ok(
        "card_update",
        board(json!({"target": "Goals:Sized", "value": {"priority": "高", "workBranch": "work"}})),
    );
    assert_eq!(rows(), 8);
}

/// The cards done in a goal are one row 「済み」, folded as the box first shows: a task checked, a
/// goal inside 完了 — the box no taller for each one done, one row for them all, none while none
/// is.
#[test]
fn the_cards_done_in_a_goal_are_one_row() {
    let core = Core::open(script_store()).unwrap();
    let call_ok = |tool: &str, arguments: Value| {
        let (failed, text) = call(&core, tool, arguments.clone());
        assert!(!failed, "{tool} {arguments}: {text}");
        serde_json::from_str::<Value>(&text).unwrap_or(Value::Null)
    };
    let board = |arguments: Value| {
        let mut arguments = arguments;
        arguments["project"] = "見本".into();
        arguments["branch"] = "_project_board_".into();
        arguments
    };
    call_ok(
        "card_create",
        board(json!({"value": {
            "Goals:Folded": {"kind": "goal"},
            "Goals:Folded.a": {"kind": "task"},
            "Goals:Folded.b": {"kind": "task"},
            "Goals:Folded.c": {"kind": "task"},
            "Goals:Folded.Sub": {"kind": "goal"}
        }})),
    );
    let rows = || {
        let read = call_ok(
            "section_read",
            board(json!({"target": "Goals", "value": {"include": ["places"]}})),
        );
        let place = read["places"]["Goals:Folded"].clone();
        place["bottomRight"]["y"].as_i64().unwrap() - place["topLeft"]["y"].as_i64().unwrap() + 1
    };
    let tall = |count: f64| ((2.0 + 40.0 + 12.0 + count * 27.0) / 27.0_f64).ceil() as i64;
    // 状態・優先度 and four cards inside: six rows.
    assert_eq!(rows(), tall(6.0));
    // a done: 状態・優先度・b・c・Sub・済み — as tall as before.
    call_ok(
        "card_update",
        board(json!({"target": "Goals:Folded.a", "value": {"done": true}})),
    );
    assert_eq!(rows(), tall(6.0));
    // b and Sub done too: 状態・優先度・c・済み.
    call_ok(
        "card_update",
        board(
            json!({"value": {"Goals:Folded.b": {"done": true}, "Goals:Folded.Sub": {"status": "完了"}}}),
        ),
    );
    assert_eq!(rows(), tall(4.0));
    // All of them: 状態・優先度・済み.
    call_ok(
        "card_update",
        board(json!({"target": "Goals:Folded.c", "value": {"done": true}})),
    );
    assert_eq!(rows(), tall(3.0));
}

/// A goal's working branch is its `workBranch`, as `branch` heads every answer as the tree it is of.
/// Read and listed by it; the key of before refused with the
/// one of now; a project file before 17 has it moved, its value as it was.
#[test]
fn a_goals_working_branch_is_its_work_branch() {
    let core = Core::open(script_store()).unwrap();
    let board = |mut arguments: Value| {
        arguments["project"] = "見本".into();
        arguments["branch"] = "_project_board_".into();
        arguments
    };
    let (failed, text) = call(
        &core,
        "card_create",
        board(
            json!({"target": "Goals:Work", "value": {"kind": "goal", "workBranch": "refunds", "MR": 3}}),
        ),
    );
    assert!(!failed, "{text}");
    let (_, text) = call(&core, "card_read", board(json!({"target": "Goals:Work"})));
    let work: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(work["branch"], "_project_board_", "{work}");
    assert_eq!(work["workBranch"], "refunds", "{work}");
    let (_, text) = call(
        &core,
        "card_list",
        board(json!({"value": {"fields": {"workBranch": "refunds"}}})),
    );
    let listed: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(listed["matched"], 1, "{listed}");
    // The key of before, refused with the one of now: written, and asked of the list.
    for (tool, arguments) in [
        (
            "card_update",
            json!({"target": "Goals:Work", "value": {"branch": "refunds"}}),
        ),
        (
            "card_list",
            json!({"value": {"fields": {"branch": "refunds"}}}),
        ),
    ] {
        let (failed, text) = call(&core, tool, board(arguments));
        assert!(failed, "{tool}: {text}");
        assert!(text.contains("workBranch"), "{tool}: {text}");
    }

    // As 16 wrote it.
    let project: String = rusqlite::Connection::open(core.place())
        .unwrap()
        .query_row("SELECT id FROM projects WHERE name = '見本'", [], |r| {
            r.get(0)
        })
        .unwrap();
    let op = |request: Value| -> Value {
        serde_json::from_str(&core.call(&request.to_string())).unwrap()
    };
    let text = op(json!({"op": "project_file", "project": project}))["result"]["text"]
        .as_str()
        .unwrap()
        .to_string();
    let mut file: Value = serde_json::from_str(&text).unwrap();
    file["version"] = 16.into();
    let work = file["board"]["cards"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|c| c["name"] == "Work")
        .unwrap();
    let fields = work["fields"].as_object_mut().unwrap();
    let working = fields.shift_remove("workBranch").unwrap();
    fields.insert("branch".into(), working);
    let read =
        op(json!({"op": "project_file_read", "text": file.to_string(), "landing": "replace"}));
    assert!(read.get("error").is_none(), "{read}");
    let (_, text) = call(&core, "card_read", board(json!({"target": "Goals:Work"})));
    let work: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(
        (&work["workBranch"], &work["MR"]),
        (&json!("refunds"), &json!(3)),
        "{work}"
    );
    assert!(work.get("strayFields").is_none(), "{work}");
}
