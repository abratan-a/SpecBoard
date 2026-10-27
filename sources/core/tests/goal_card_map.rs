//! A goal's card map: what a goal holds is cards inside it, made by their path on from the
//! goal's, at the end of what it holds; the boxes of the cards inside a goal are read
//! (`card_read`'s `include: ["places"]`) and put (`card_update`'s `places`) as a section's
//! diagram's are; a card inside with no place there is put in the order they are inside it;
//! the window reads them with the goal (`project_detail`'s `itemPlaces`) and puts them by hand
//! (`places`), a locked main too.

use crate::common;

use common::{mcp, op, project_id, script_store};
use serde_json::{Value, json};
use specboard_core::Core;

const BOARD: &str = "~board";

/// The same, in the board.
fn board(core: &Core, tool: &str, mut arguments: Value) -> Result<Value, String> {
    arguments["branch"] = "_project_board_".into();
    mcp(core, tool, arguments)
}

/// A goal's card map as `card_read` returns it: by the card's path.
fn map(core: &Core, goal: &str) -> Value {
    let read = board(
        core,
        "card_read",
        json!({"target": goal, "value": {"include": ["places"]}}),
    )
    .unwrap();
    assert!(read["places"].is_object(), "{read}");
    read["places"].clone()
}

/// The same as a list of boxes in the order of the map, each with its path under `card`.
fn rows(places: &Value) -> Vec<Value> {
    places
        .as_object()
        .unwrap()
        .iter()
        .map(|(path, b)| {
            let mut row = b.clone();
            row["card"] = path.clone().into();
            row
        })
        .collect()
}

fn places(core: &Core, goal: &str) -> Vec<Value> {
    rows(&map(core, goal))
}

/// Each box's card and its top left, as `(path, x, y)`.
fn spots(boxes: &[Value]) -> Vec<(String, i64, i64)> {
    boxes
        .iter()
        .map(|b| {
            (
                b["card"].as_str().unwrap().to_string(),
                b["topLeft"]["x"].as_i64().unwrap(),
                b["topLeft"]["y"].as_i64().unwrap(),
            )
        })
        .collect()
}

/// The board's commits, newest first: each commit's lines.
fn commits(core: &Core) -> Vec<Vec<String>> {
    let read = op(
        core,
        json!({"op": "commits", "project": project_id(core), "branch": BOARD}),
    );
    read["result"]["commits"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| {
            c["lines"]
                .as_array()
                .unwrap()
                .iter()
                .map(|l| l["summary"].as_str().unwrap().to_string())
                .collect()
        })
        .collect()
}

fn items(core: &Core, goal: &str) -> Value {
    board(core, "card_read", json!({"target": goal})).unwrap()["cards"].clone()
}

#[test]
fn a_card_made_in_a_goal_goes_at_the_end_of_what_it_holds_in_one_commit() {
    let core = Core::open(script_store()).unwrap();
    let before = commits(&core).len();
    let made = board(
        &core,
        "card_create",
        json!({"target": "Goals:Ship.Decide", "value": {"kind": "task", "label": "決める"}}),
    )
    .unwrap();
    assert_eq!(made["created"][0]["path"], "Goals:Ship.Decide", "{made}");
    assert_eq!(
        items(&core, "Goals:Ship"),
        json!(["Goals:Ship.Design", "Goals:Ship.t2", "Goals:Ship.Decide"])
    );
    let after = commits(&core);
    assert_eq!(after.len(), before + 1, "1 つの書き込みは 1 コミット");
    assert_eq!(after[0], ["カード Goals:Ship.Decide を作成"]);
    // A goal inside it, and cards inside that one made in one call with it.
    board(
        &core,
        "card_create",
        json!({"value": {
            "Goals:Ship.Later": {"kind": "goal"},
            "Goals:Ship.Later.A": {"kind": "task"},
            "Goals:Ship.Later.B": {"kind": "task"}
        }}),
    )
    .unwrap();
    assert_eq!(items(&core, "Goals:Ship")[3], "Goals:Ship.Later");
    assert_eq!(
        items(&core, "Goals:Ship.Later"),
        json!(["Goals:Ship.Later.A", "Goals:Ship.Later.B"])
    );
}

#[test]
fn a_goal_holds_goals_and_tasks_and_moves_them_by_in() {
    let core = Core::open(script_store()).unwrap();
    let before = commits(&core).len();
    let refused = |target: &str, value: Value| {
        board(
            &core,
            "card_create",
            json!({"target": target, "value": value}),
        )
        .unwrap_err()
    };
    // A kind a goal does not hold.
    let said = refused("Goals:Ship.Memo", json!({"kind": "note"}));
    assert_eq!(
        said,
        "Goals:Ship.Memo: Goals:Ship（goal）の中に置けるのは goal・task です（note ではなく）"
    );
    // Inside what holds none.
    let said = refused("Goals:Ship.t2.T9", json!({"kind": "task"}));
    assert!(
        said.starts_with("Goals:Ship.t2 は task で、中にカードを置けません"),
        "{said}"
    );
    // Nothing of them was made.
    assert_eq!(commits(&core).len(), before);
    assert_eq!(
        items(&core, "Goals:Ship"),
        json!(["Goals:Ship.Design", "Goals:Ship.t2"])
    );
    // Moved to another goal by `in`, the paths that named it on from the new one.
    board(
        &core,
        "card_create",
        json!({"target": "Goals:Other", "value": {"kind": "goal", "body": "[[Goals:Ship.t2]] を見る"}}),
    )
    .unwrap();
    let moved = board(
        &core,
        "card_update",
        json!({"target": "Goals:Ship.t2", "value": {"in": "Goals:Other"}}),
    )
    .unwrap();
    assert_eq!(moved["path"], "Goals:Other.t2", "{moved}");
    assert_eq!(items(&core, "Goals:Other"), json!(["Goals:Other.t2"]));
    let other = board(&core, "card_read", json!({"target": "Goals:Other"})).unwrap();
    assert_eq!(other["body"], "[[Goals:Other.t2]] を見る");
    // A goal does not go inside itself or what it holds.
    let said = board(
        &core,
        "card_update",
        json!({"target": "Goals:Other", "value": {"in": "Goals:Other"}}),
    )
    .unwrap_err();
    assert!(said.contains("Goals:Other かその中なので"), "{said}");
    // Out of the goal, into its section.
    board(
        &core,
        "card_update",
        json!({"target": "Goals:Other.t2", "value": {"in": null}}),
    )
    .unwrap();
    assert!(board(&core, "card_read", json!({"target": "Goals:t2"})).is_ok());
    // Its section is the goal's: moving it by `section` is refused, with `in` said.
    let said = board(
        &core,
        "card_update",
        json!({"target": "Goals:Ship.Design", "value": {"section": "Goals"}}),
    )
    .unwrap_err();
    assert!(said.contains("value の in"), "{said}");
    // In main: an entity goes in no goal; the free section holds goals and tasks.
    let said = mcp(
        &core,
        "card_create",
        json!({"value": {"Free:Aim": {"kind": "goal"}, "Free:Aim.Order": {"kind": "entity"}}}),
    )
    .unwrap_err();
    assert!(
        said.contains("Free:Aim（goal）の中に置けるのは goal・task です（entity ではなく）"),
        "{said}"
    );
    mcp(
        &core,
        "card_create",
        json!({"value": {"Free:Aim": {"kind": "goal"}, "Free:Aim.Step": {"kind": "task"}}}),
    )
    .unwrap();
    let aim = mcp(&core, "card_read", json!({"target": "Free:Aim"})).unwrap();
    assert_eq!(aim["cards"], json!(["Free:Aim.Step"]));
    assert_eq!(aim["progress"], json!({"done": 0, "of": 1}));
}

#[test]
fn a_goals_boxes_are_read_and_put_as_a_sections_are() {
    let core = Core::open(script_store()).unwrap();
    // Nothing sits on the map yet; a write places what has no place.
    assert!(places(&core, "Goals:Ship").is_empty());
    board(
        &core,
        "card_update",
        json!({"target": "Goals:Ship.t2", "value": {"label": "保存と画面"}}),
    )
    .unwrap();
    let boxes = places(&core, "Goals:Ship");
    assert_eq!(
        spots(&boxes),
        [
            ("Goals:Ship.Design".to_string(), 0, 0),
            ("Goals:Ship.t2".to_string(), 11, 0)
        ]
    );
    // A card the project's width; a task drawn by its kind's face: its check and, its body
    // written, as tall as the page of it is drawn (a line here: five rows), the project's
    // whole card at the most.
    assert_eq!(boxes[0]["bottomRight"], json!({"x": 9, "y": 4}));
    assert_eq!(boxes[1]["bottomRight"], json!({"x": 20, "y": 4}));
    assert!(boxes[0].get("size").is_none());
    // A long body: the project's whole card, the rest scrolled in it.
    let long: Vec<String> = (1..=40).map(|n| format!("- {n} 行目")).collect();
    board(
        &core,
        "card_update",
        json!({"target": "Goals:Ship.t2", "value": {"body": long.join("\n")}}),
    )
    .unwrap();
    let boxes = places(&core, "Goals:Ship");
    assert_eq!(boxes[1]["bottomRight"], json!({"x": 20, "y": 18}));
    // With places alone, the card's head and its boxes.
    let read = board(
        &core,
        "card_read",
        json!({"target": "Goals:Ship", "value": {"include": ["places"]}}),
    )
    .unwrap();
    assert!(
        read.get("cards").is_none() && read["kind"] == "goal",
        "{read}"
    );

    // Put over another, it goes to the nearest clear cell.
    let put = board(
        &core,
        "card_update",
        json!({"target": "Goals:Ship", "value": {"places": {"Goals:Ship.t2": {"topLeft": {"x": 0, "y": 0}}}}}),
    )
    .unwrap();
    assert_eq!(put["placed"], 1, "{put}");
    let jumped = put["jumped"].as_array().unwrap();
    assert_eq!(jumped.len(), 1);
    assert_eq!(jumped[0]["card"], "Goals:Ship.t2");
    assert_ne!(jumped[0]["topLeft"], json!({"x": 0, "y": 0}));
    assert_eq!(
        spots(&places(&core, "Goals:Ship"))[1],
        (
            "Goals:Ship.t2".to_string(),
            jumped[0]["topLeft"]["x"].as_i64().unwrap(),
            jumped[0]["topLeft"]["y"].as_i64().unwrap()
        )
    );
    // In the clear, where it says; the cards are named by their names in the goal too.
    let before = commits(&core).len();
    let put = board(
        &core,
        "card_update",
        json!({"target": "Goals:Ship", "value": {"places": {"t2": {"topLeft": {"x": 40, "y": 0}}, "Goals:Ship.Design": {"topLeft": {"x": 0, "y": 30}}}}}),
    )
    .unwrap();
    assert_eq!(put["placed"], 2);
    assert!(put.get("jumped").is_none(), "{put}");
    assert_eq!(
        spots(&places(&core, "Goals:Ship")),
        [
            ("Goals:Ship.Design".to_string(), 0, 30),
            ("Goals:Ship.t2".to_string(), 40, 0)
        ]
    );
    assert_eq!(
        commits(&core).len(),
        before,
        "置くのは見かけでコミットにならない"
    );
}

#[test]
fn what_is_not_on_a_goals_map_is_refused() {
    let core = Core::open(script_store()).unwrap();
    let update = |target: &str, value: Value| {
        board(
            &core,
            "card_update",
            json!({"target": target, "value": value}),
        )
        .unwrap_err()
    };
    // A card the goal does not hold.
    board(
        &core,
        "card_create",
        json!({"target": "Goals:Stray", "value": {"kind": "task"}}),
    )
    .unwrap();
    let said = update(
        "Goals:Ship",
        json!({"places": {"Goals:Stray": {"topLeft": {"x": 0, "y": 0}}}}),
    );
    assert_eq!(
        said,
        "places の「Goals:Stray」: Goals:Ship の中にありません（コンテンツマップに置けるのは goal の中のカードです）"
    );
    // A card inside a goal is sized by its own value (`size`), not here.
    let said = update(
        "Goals:Ship",
        json!({"places": {"Goals:Ship.t2": {"size": {"w": 12, "h": 4}}}}),
    );
    assert_eq!(
        said,
        "places の「Goals:Ship.t2」: コンテンツマップの places に書くのは topLeft だけです（task の大きさは、そのカードの card_update の value の size で変えます）"
    );
    board(
        &core,
        "card_create",
        json!({"target": "Goals:Ship.Inner", "value": {"kind": "goal"}}),
    )
    .unwrap();
    // A goal inside a goal carries a size too, written on the card, not in the map's places.
    let said = update(
        "Goals:Ship",
        json!({"places": {"Goals:Ship.Inner": {"size": {"w": 12, "h": 4}}}}),
    );
    assert_eq!(
        said,
        "places の「Goals:Ship.Inner」: コンテンツマップの places に書くのは topLeft だけです（goal の大きさは、そのカードの card_update の value の size で変えます）"
    );
    let said = update(
        "Goals:Ship",
        json!({"places": {"Goals:Ship.t2": {"at": {"x": 0, "y": 0}}}}),
    );
    assert_eq!(
        said,
        "places の「Goals:Ship.t2」に使えないキーです: at。使えるのは topLeft"
    );
    // Only a goal has a card map.
    let said = update("Goals:Ship.t2", json!({"places": {}}));
    assert!(
        said.starts_with("places は goal のコンテンツマップの箱です（Goals:Ship.t2 は task。"),
        "{said}"
    );
    let said = board(
        &core,
        "card_read",
        json!({"target": "Goals:Ship.t2", "value": {"include": ["places"]}}),
    )
    .unwrap_err();
    assert!(
        said.starts_with(
            "include の places は goal のコンテンツマップの箱です（Goals:Ship.t2 は task）。"
        ),
        "{said}"
    );
    // card_list has no card maps to give.
    let said = board(
        &core,
        "card_list",
        json!({"value": {"include": ["places"]}}),
    )
    .unwrap_err();
    assert!(said.contains("places"), "{said}");
}

/// A task is sized by hand where it sits on its goal's card
/// map, how it looks — no commit, a locked main too — read with the goal's boxes and with
/// the card; let go of, it is drawn by its face again.
#[test]
fn a_task_is_sized_by_hand_on_its_goals_map() {
    let core = Core::open(script_store()).unwrap();
    let project = project_id(&core);
    // A task with nothing in its body: its check alone.
    board(
        &core,
        "card_create",
        json!({"target": "Goals:Ship.Plain", "value": {"kind": "task", "label": "まだ書かない"}}),
    )
    .unwrap();
    let boxes = places(&core, "Goals:Ship");
    let plain = boxes
        .iter()
        .find(|b| b["card"] == "Goals:Ship.Plain")
        .unwrap();
    let at = plain["topLeft"].clone();
    assert_eq!(
        plain["bottomRight"]["y"].as_i64().unwrap() - at["y"].as_i64().unwrap(),
        2
    );
    let before = commits(&core).len();
    board(
        &core,
        "card_update",
        json!({"target": "Goals:Ship.Plain", "value": {"size": {"w": 14, "h": 8}}}),
    )
    .unwrap();
    assert_eq!(
        commits(&core).len(),
        before,
        "大きさは見かけでコミットにならない"
    );
    let boxes = places(&core, "Goals:Ship");
    let plain = boxes
        .iter()
        .find(|b| b["card"] == "Goals:Ship.Plain")
        .unwrap();
    // Where it sat, as big as it was made.
    assert_eq!(plain["topLeft"], at);
    assert_eq!(plain["size"], json!({"w": 14, "h": 8}));
    assert_eq!(
        plain["bottomRight"],
        json!({"x": at["x"].as_i64().unwrap() + 13, "y": at["y"].as_i64().unwrap() + 7})
    );
    // The window reads it with the card, where it sits on the map.
    let read = op(
        &core,
        json!({"op": "project_detail", "project": project, "branch": BOARD}),
    );
    let card = read["result"]["schemas"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "Plain")
        .unwrap()
        .clone();
    assert_eq!(card["size"], json!({"w": 14, "h": 8}));
    // Sized by hand, it is told no height of its face; one with its body written and no size,
    // as tall as the core places it.
    assert!(card.get("faceRows").is_none(), "{card}");
    let t2 = read["result"]["schemas"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "t2")
        .unwrap()
        .clone();
    assert_eq!(t2["faceRows"], json!(5), "{t2}");
    // Let go of: drawn by its face again.
    board(
        &core,
        "card_update",
        json!({"target": "Goals:Ship.Plain", "value": {"size": null}}),
    )
    .unwrap();
    let boxes = places(&core, "Goals:Ship");
    let plain = boxes
        .iter()
        .find(|b| b["card"] == "Goals:Ship.Plain")
        .unwrap();
    assert!(plain.get("size").is_none());
    assert_eq!(plain["topLeft"], at);
    // In main, locked, a task in the free section is sized too, on its own section's diagram.
    mcp(
        &core,
        "card_create",
        json!({"target": "Free:Loose", "value": {"kind": "task"}}),
    )
    .unwrap();
    let locked = op(
        &core,
        json!({"op": "main_lock", "project": project, "on": true}),
    );
    assert!(locked.get("error").is_none(), "{locked}");
    mcp(
        &core,
        "card_update",
        json!({"target": "Free:Loose", "value": {"size": {"w": 12, "h": 6}}}),
    )
    .unwrap();
    let read = mcp(
        &core,
        "card_read",
        json!({"target": "Free:Loose", "value": {"include": ["at"]}}),
    )
    .unwrap();
    let (x, y) = (
        read["topLeft"]["x"].as_i64().unwrap(),
        read["topLeft"]["y"].as_i64().unwrap(),
    );
    assert_eq!(read["bottomRight"], json!({"x": x + 11, "y": y + 5}));
}

/// A goal inside a goal is drawn on the map as on a section's diagram: a row for each field it
/// holds something in, the branch it is worked in (`workBranch`) too.
#[test]
fn a_goal_inside_a_goal_has_a_row_for_the_branch_it_is_worked_in() {
    let core = Core::open(script_store()).unwrap();
    mcp(&core, "branch_create", json!({"target": "work"})).unwrap();
    board(
        &core,
        "card_create",
        json!({"value": {
            "Goals:Ship.Bare": {"kind": "goal"},
            "Goals:Ship.Due": {"kind": "goal", "due": "2026-11-01"},
            "Goals:Ship.Worked": {"kind": "goal", "workBranch": "work"}
        }}),
    )
    .unwrap();
    let boxes = places(&core, "Goals:Ship");
    let tall = |path: &str| {
        let b = boxes.iter().find(|b| b["card"] == path).unwrap();
        b["bottomRight"]["y"].as_i64().unwrap() - b["topLeft"]["y"].as_i64().unwrap()
    };
    assert!(
        tall("Goals:Ship.Due") > tall("Goals:Ship.Bare"),
        "{boxes:?}"
    );
    assert_eq!(
        tall("Goals:Ship.Worked"),
        tall("Goals:Ship.Due"),
        "{boxes:?}"
    );
}

/// A goal's status is said in its head, never a row; its other fields are a row each only when
/// written, a choice too.
#[test]
fn a_goals_status_is_in_its_head_and_its_fields_are_rows_when_written() {
    let core = Core::open(script_store()).unwrap();
    board(
        &core,
        "card_create",
        json!({"value": {
            "Goals:Ship.Bare": {"kind": "goal"},
            "Goals:Ship.Started": {"kind": "goal", "status": "inProgress"},
            "Goals:Ship.Urgent": {"kind": "goal", "priority": "high"}
        }}),
    )
    .unwrap();
    let boxes = places(&core, "Goals:Ship");
    let tall = |path: &str| {
        let b = boxes.iter().find(|b| b["card"] == path).unwrap();
        b["bottomRight"]["y"].as_i64().unwrap() - b["topLeft"]["y"].as_i64().unwrap()
    };
    assert_eq!(
        tall("Goals:Ship.Started"),
        tall("Goals:Ship.Bare"),
        "{boxes:?}"
    );
    assert!(
        tall("Goals:Ship.Urgent") > tall("Goals:Ship.Bare"),
        "{boxes:?}"
    );
}

#[test]
fn cards_with_no_place_on_the_map_go_in_the_order_the_goal_holds_them() {
    let core = Core::open(script_store()).unwrap();
    board(
        &core,
        "card_create",
        json!({"target": "Goals:Many", "value": {"kind": "goal"}}),
    )
    .unwrap();
    board(
        &core,
        "card_create",
        json!({"target": ["Goals:Many.A", "Goals:Many.B", "Goals:Many.C", "Goals:Many.D", "Goals:Many.E"], "value": {"kind": "task"}}),
    )
    .unwrap();
    // Four a row, a card wide and the margin apart; the fifth under the first.
    assert_eq!(
        spots(&places(&core, "Goals:Many")),
        [
            ("Goals:Many.A".to_string(), 0, 0),
            ("Goals:Many.B".to_string(), 11, 0),
            ("Goals:Many.C".to_string(), 22, 0),
            ("Goals:Many.D".to_string(), 33, 0),
            ("Goals:Many.E".to_string(), 0, 4),
        ]
    );
}

#[test]
fn the_window_reads_the_boxes_with_the_goal_and_puts_them_by_hand() {
    let core = Core::open(script_store()).unwrap();
    let project = project_id(&core);
    board(
        &core,
        "card_update",
        json!({"target": "Goals:Ship", "value": {"label": "出す"}}),
    )
    .unwrap();
    let detail = || {
        let read = op(
            &core,
            json!({"op": "project_detail", "project": project, "branch": BOARD}),
        );
        read["result"]["schemas"].as_array().unwrap().clone()
    };
    let id = |cards: &[Value], name: &str| {
        cards.iter().find(|c| c["name"] == name).unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string()
    };
    let cards = detail();
    let (ship, design, t2) = (id(&cards, "Ship"), id(&cards, "Design"), id(&cards, "t2"));
    let goal = cards.iter().find(|c| c["id"] == ship).unwrap();
    assert_eq!(
        goal["itemPlaces"],
        json!({ &design: {"x": 0, "y": 0}, &t2: {"x": 11, "y": 0} })
    );
    // A task holds no map of its own.
    assert!(
        cards
            .iter()
            .find(|c| c["id"] == t2)
            .unwrap()
            .get("itemPlaces")
            .is_none()
    );

    // Dropped by hand: where it says, no history.
    let before = commits(&core).len();
    let put = op(
        &core,
        json!({"op": "places", "project": project, "branch": BOARD, "boxes": [{"view": ship, "item": t2, "x": 5, "y": 9, "pinned": true}]}),
    );
    assert!(put.get("error").is_none(), "{put}");
    let cards = detail();
    let goal = cards.iter().find(|c| c["id"] == ship).unwrap();
    assert_eq!(goal["itemPlaces"][&t2], json!({"x": 5, "y": 9}));
    assert_eq!(commits(&core).len(), before);
    // Not a section's diagram.
    let positions = op(
        &core,
        json!({"op": "section_positions", "project": project, "branch": BOARD}),
    );
    assert!(
        positions["result"]["positions"].get(&ship).is_none(),
        "{positions}"
    );

    // What has no place is given one when the window asks.
    rusqlite::Connection::open(core.place())
        .unwrap()
        .execute("DELETE FROM places WHERE view = ?", [&ship])
        .unwrap();
    assert!(
        detail()
            .iter()
            .find(|c| c["id"] == ship)
            .unwrap()
            .get("itemPlaces")
            .is_none()
    );
    let placed = op(
        &core,
        json!({"op": "place_unplaced", "project": project, "branch": BOARD}),
    );
    assert!(placed.get("error").is_none(), "{placed}");
    let cards = detail();
    let goal = cards.iter().find(|c| c["id"] == ship).unwrap();
    assert_eq!(goal["itemPlaces"].as_object().unwrap().len(), 2);
}

#[test]
fn a_locked_main_takes_a_goals_map_and_a_goal_gone_takes_its_boxes() {
    let core = Core::open(script_store()).unwrap();
    let project = project_id(&core);
    mcp(
        &core,
        "card_create",
        json!({"value": {"Free:Aim": {"kind": "goal"}, "Free:Aim.Step": {"kind": "task"}}}),
    )
    .unwrap();
    let locked = op(
        &core,
        json!({"op": "main_lock", "project": project, "on": true}),
    );
    assert!(locked.get("error").is_none(), "{locked}");
    // Where a goal's cards sit is the goal's own diagram, as a flow's nodes are.
    mcp(
        &core,
        "card_update",
        json!({"target": "Free:Aim", "value": {"places": {"Free:Aim.Step": {"topLeft": {"x": 3, "y": 4}}}}}),
    )
    .unwrap();
    let read = mcp(
        &core,
        "card_read",
        json!({"target": "Free:Aim", "value": {"include": ["places"]}}),
    )
    .unwrap();
    assert_eq!(
        spots(&rows(&read["places"])),
        [("Free:Aim.Step".to_string(), 3, 4)]
    );
    let unlocked = op(
        &core,
        json!({"op": "main_lock", "project": project, "on": false}),
    );
    assert!(unlocked.get("error").is_none(), "{unlocked}");

    // The goal gone, its map goes with it.
    let aim: String = rusqlite::Connection::open(core.place())
        .unwrap()
        .query_row(
            "SELECT id FROM card_docs WHERE project_id = ? AND json_extract(doc, '$.name') = 'Aim'",
            [&project],
            |r| r.get(0),
        )
        .unwrap();
    let count = || -> i64 {
        rusqlite::Connection::open(core.place())
            .unwrap()
            .query_row("SELECT count(*) FROM places WHERE view = ?", [&aim], |r| {
                r.get(0)
            })
            .unwrap()
    };
    assert_eq!(count(), 1);
    // The goal gone, what was inside it is out in its section.
    let gone = mcp(&core, "card_delete", json!({"target": "Free:Aim"})).unwrap();
    assert_eq!(count(), 0);
    assert!(
        gone["warnings"][0]
            .as_str()
            .is_some_and(|w| w.contains("Free:Aim.Step")),
        "{gone}"
    );
    assert!(mcp(&core, "card_read", json!({"target": "Free:Step"})).is_ok());
}

/// A goal's card map as mermaid: `card_read`'s `as: diagram` of a goal draws what it holds —
/// numbered in the order they are done in, a task's check as ☑ or ☐ — and the lines between them,
/// as the window's map does; and a section's diagram draws its own cards whether or not a line
/// reaches them.
#[test]
fn a_goals_map_and_a_section_with_no_lines_are_drawn_as_mermaid() {
    let core = Core::open(script_store()).unwrap();
    let text = |answer: Value| answer["mermaid"].as_str().unwrap_or_default().to_string();
    let map = text(
        board(
            &core,
            "card_read",
            json!({"target": "Goals:Ship", "value": {"as": "diagram"}}),
        )
        .unwrap(),
    );
    assert!(map.contains("flowchart LR"), "{map}");
    assert!(map.contains("n1[\"1. ☑ "), "{map}");
    assert!(map.contains("n2[\"2. ☐ "), "{map}");
    assert!(!map.contains("-->"), "{map}");
    // A line between two cards it holds: drawn, with its kind's name.
    board(
        &core,
        "card_update",
        json!({"target": "Goals:Ship.t2", "value": {"connections": {"add": [{"line": "link", "to": "Goals:Ship.Design"}]}}}),
    )
    .unwrap();
    let joined = text(
        board(
            &core,
            "card_read",
            json!({"target": "Goals:Ship", "value": {"as": "diagram"}}),
        )
        .unwrap(),
    );
    assert!(joined.contains("n2 -->|"), "{joined}");
    assert!(joined.contains("| n1"), "{joined}");
    let refused = board(
        &core,
        "card_read",
        json!({"target": "Goals:Ship", "value": {"as": "diagram", "depth": 2}}),
    )
    .unwrap_err();
    assert!(
        refused.contains("コンテンツマップの図では depth は使いません"),
        "{refused}"
    );
    // The board's Goals, no line in it: its goal drawn all the same.
    let section = text(
        board(
            &core,
            "section_read",
            json!({"target": "Goals", "value": {"as": "diagram"}}),
        )
        .unwrap(),
    );
    assert!(section.contains("Goals__Ship"), "{section}");
    assert!(
        !section.contains("条件に一致するスキーマがありません"),
        "{section}"
    );
}
