//! What a write costs.
//!
//! The whole project is worked out of the documents again only when a row has changed since
//! (`Seen::in_tx`), not at each of the places a card's update asks for it, so a step costs the size
//! of what it touches and not of the project. The speed itself is not held here (a runner's time is
//! not steady); the count is, and that what a step wrote is seen by the next.

use crate::common;

use common::{call, script_store};
use serde_json::{Value, json};
use specboard_core::{
    Core, project_bring_ups_on_this_thread, project_rebuilds_on_this_thread,
    projects_held_on_this_thread,
};

const PROJECT: &str = "見本";

fn core() -> Core {
    let core = Core::open(script_store()).unwrap();
    // The key the tests call MCP with is made on the first call: not in what is counted.
    let (failed, text) = call(&core, "project_list", json!({}));
    assert!(!failed, "{text}");
    core
}

fn ok(core: &Core, tool: &str, mut arguments: Value) -> Value {
    if arguments.get("target").is_none() || tool != "batch" {
        arguments["project"] = PROJECT.into();
    }
    let (failed, text) = call(core, tool, arguments.clone());
    assert!(!failed, "{tool} {arguments}: {text}");
    serde_json::from_str(&text).unwrap_or(Value::Null)
}

/// How many times the project was worked out whole while `act` ran: what a write's own reading
/// (`Seen::in_tx`) worked out anew, not what it reads whole without it (`Scope::docs`,
/// `Scope::detail`) nor a read's (`Seen::read`). Never none: each call's transaction starts with
/// nothing kept, so a call that writes works it out at least once — none would say the counting
/// has stopped (the calls run on this thread no more, say), not that the write got cheaper.
fn rebuilds_in(act: impl FnOnce()) -> u64 {
    let before = project_rebuilds_on_this_thread();
    act();
    let n = project_rebuilds_on_this_thread() - before;
    assert!(n >= 1, "組み直しを 1 回も数えていない（数え方が止まった）");
    n
}

/// Steps that each change a card already there, in the sample project.
fn updates(count: usize) -> Value {
    (0..count)
        .map(|i| {
            json!({"tool": "card_update", "arguments": {
                "target": "Free:Memo",
                "value": {"body": {"append": format!("\n足した {i}")}}}})
        })
        .collect()
}

#[test]
fn a_card_update_works_the_project_out_a_few_times_not_a_dozen() {
    let core = core();
    let n = rebuilds_in(|| {
        ok(
            &core,
            "card_update",
            json!({"target": "Free:Memo", "value": {"body": "ひとこと"}}),
        );
    });
    println!("card_update: {n}");
    // Once, before it writes: what it writes after (`Free:Memo`'s body, the history's line, the
    // project marked as written) is brought up — the working directories, read by no tree here, write
    // nothing in between.
    assert_eq!(n, 1, "card_update が全体を {n} 回組み直した");
}

#[test]
fn a_batch_works_the_project_out_a_few_times_not_once_a_step() {
    let core = core();
    let steps = 12;
    let n = rebuilds_in(|| {
        ok(
            &core,
            "batch",
            json!({"value": {"steps": updates(steps)}, "execute": true}),
        );
    });
    println!("batch of {steps}: {n}");
    // A step writes a body, said in its card's own entry alone: what the next step looks at is
    // brought up to it, not worked out again.
    assert!(n <= 3, "{steps} 手の batch が全体を {n} 回組み直した");
    let memo = ok(&core, "card_read", json!({"target": "Free:Memo"}));
    let body = memo["body"].as_str().unwrap();
    assert!(
        (0..steps).all(|i| body.contains(&format!("足した {i}"))),
        "{body}"
    );
}

#[test]
fn a_step_sees_what_the_step_before_it_wrote() {
    let core = core();
    let steps = json!([
        {"tool": "card_create", "arguments": {
            "target": "Schemas:Fresh",
            "value": {"kind": "entity", "fields": [{"name": "a", "type": "string"}]}}},
        {"tool": "card_update", "arguments": {
            "target": "Schemas:Fresh", "value": {"description": "作ったばかり"}}},
        {"tool": "card_create", "arguments": {
            "target": "Schemas:Next",
            "value": {"kind": "entity",
                "fields": [{"name": "fresh_id", "type": "string"}],
                "connections": [{"line": "points", "from": "fresh_id", "to": "Schemas:Fresh", "at": "a"}]}}},
        {"tool": "card_update", "arguments": {
            "target": "Free:Memo", "value": {"body": "[[Schemas:Next]] と [[Schemas:Fresh]]"}}},
        {"tool": "card_update", "arguments": {
            "target": "Schemas:Fresh", "value": {"label": "名前を付けた"}}},
    ]);
    let ran = ok(
        &core,
        "batch",
        json!({"value": {"steps": steps}, "execute": true}),
    );
    assert!(
        ran["warnings"].as_array().is_none_or(|w| w.is_empty()),
        "{ran}"
    );
    let read = ok(&core, "card_read", json!({"target": "Schemas:Fresh"}));
    assert_eq!(read["description"], "作ったばかり", "{read}");
    assert_eq!(read["label"], "名前を付けた", "{read}");
    let next = ok(&core, "card_read", json!({"target": "Schemas:Next"}));
    assert!(next.to_string().contains("Schemas:Fresh"), "{next}");
    // And a card taken away is gone for the next step: the memo is not what is asked.
    let gone = ok(
        &core,
        "batch",
        json!({"value": {"steps": [
            {"tool": "card_delete", "arguments": {"target": "Schemas:Next"}},
            {"tool": "card_update", "arguments": {
                "target": "Free:Memo", "value": {"body": "[[Schemas:Next]] はもう無い"}}},
        ]}, "execute": true}),
    );
    assert!(gone.to_string().contains("Schemas:Next"), "{gone}");
}

/// The sample project with `count` entities more in `Schemas` (`Many0`…) and `count` notes more in
/// `Free` (`Note0`…), made in two calls.
fn with_many(core: &Core, count: usize) {
    let mut entities = serde_json::Map::new();
    let mut notes = serde_json::Map::new();
    for n in 0..count {
        entities.insert(
            format!("Schemas:Many{n}"),
            json!({"kind": "entity", "fields": [{"name": "id", "type": "string"}]}),
        );
        notes.insert(
            format!("Free:Note{n}"),
            json!({"kind": "note", "body": format!("覚え {n}")}),
        );
    }
    ok(core, "card_create", json!({"value": entities}));
    ok(core, "card_create", json!({"value": notes}));
}

/// A call that writes what is said in each card's own entry alone (a label, a description, a body)
/// of 200 cards works the project out a few times, not once a card. Built for the tests,
/// each time it is brought up instead of worked out again it is held to what working it out again
/// makes (`Seen::in_tx`).
#[test]
fn a_call_that_writes_200_cards_works_the_project_out_a_few_times() {
    let core = core();
    with_many(&core, 100);
    let mut value = serde_json::Map::new();
    for n in 0..100 {
        value.insert(
            format!("Schemas:Many{n}"),
            json!({"label": format!("札 {n}"), "description": format!("説明 {n}")}),
        );
        value.insert(
            format!("Free:Note{n}"),
            json!({"body": format!("書き直した {n} [[Schemas:Many{n}]]")}),
        );
    }
    let n = rebuilds_in(|| {
        let out = ok(&core, "card_update", json!({"value": value}));
        assert_eq!(out["results"].as_array().map(Vec::len), Some(200), "{out}");
    });
    println!("card_update of 200: {n}");
    assert!(n <= 3, "200 枚の card_update が全体を {n} 回組み直した");
    let read = ok(&core, "card_read", json!({"target": "Schemas:Many99"}));
    assert_eq!(read["label"], "札 99", "{read}");
    assert_eq!(read["description"], "説明 99", "{read}");
    let note = ok(&core, "card_read", json!({"target": "Free:Note99"}));
    assert!(
        note["body"]
            .as_str()
            .is_some_and(|b| b.starts_with("書き直した 99")),
        "{note}"
    );
}

#[test]
fn a_batch_of_50_label_steps_works_the_project_out_a_few_times() {
    let core = core();
    with_many(&core, 50);
    let steps: Vec<Value> = (0..50)
        .map(|n| {
            json!({"tool": "card_update", "arguments": {
                "target": format!("Schemas:Many{n}"), "value": {"label": format!("手 {n}")}}})
        })
        .collect();
    let n = rebuilds_in(|| {
        ok(
            &core,
            "batch",
            json!({"value": {"steps": steps}, "execute": true}),
        );
    });
    println!("batch of 50: {n}");
    assert!(n <= 3, "50 手の batch が全体を {n} 回組み直した");
    let read = ok(&core, "card_read", json!({"target": "Schemas:Many49"}));
    assert_eq!(read["label"], "手 49", "{read}");
}

/// What a write keeps worked out (`Seen`) is let go once its call is done — a write's, a batch's,
/// a read's — so a large project is not held between calls.
#[test]
fn nothing_worked_out_is_held_once_a_call_is_done() {
    let core = core();
    with_many(&core, 20);
    let held = projects_held_on_this_thread();
    assert_eq!(held, 0, "呼ぶ前");
    let mut value = serde_json::Map::new();
    for n in 0..20 {
        value.insert(
            format!("Schemas:Many{n}"),
            json!({"label": format!("札 {n}")}),
        );
    }
    ok(&core, "card_update", json!({"value": value}));
    assert_eq!(projects_held_on_this_thread(), held, "card_update のあと");
    ok(
        &core,
        "batch",
        json!({"value": {"steps": updates(5)}, "execute": true}),
    );
    assert_eq!(projects_held_on_this_thread(), held, "batch のあと");
    let tried = ok(&core, "batch", json!({"value": {"steps": updates(2)}}));
    assert!(tried.get("batch").is_none(), "{tried}");
    assert_eq!(projects_held_on_this_thread(), held, "試した batch のあと");
    let opened = ok(
        &core,
        "batch",
        json!({"value": {"steps": updates(2), "open": true}}),
    );
    assert_eq!(projects_held_on_this_thread(), held, "開いた batch のあと");
    ok(
        &core,
        "batch",
        json!({"target": opened["batch"], "value": {"discard": true}}),
    );
    ok(&core, "card_read", json!({"target": "Schemas:Many0"}));
    assert_eq!(projects_held_on_this_thread(), held, "card_read のあと");
    // A call refused part way lets go too.
    let (refused, _) = common::call(
        &core,
        "card_update",
        json!({"project": PROJECT, "value": {"Schemas:Many0": {"label": "x"}, "Schemas:Nowhere": {"label": "y"}}}),
    );
    assert!(refused);
    assert_eq!(projects_held_on_this_thread(), held, "断られたあと");
}

/// What is brought up instead of worked out again is so in a branch — a card of main's written
/// there the first time, and again — and on the board too (built for the tests, each time it is
/// held to what working it out again makes: `Seen::in_tx`).
#[test]
fn a_branch_and_the_board_are_brought_up_as_main_is() {
    let core = core();
    with_many(&core, 10);
    ok(&core, "branch_create", json!({"target": "labels"}));
    for round in 0..2 {
        let mut value = serde_json::Map::new();
        for n in 0..10 {
            value.insert(
                format!("Schemas:Many{n}"),
                json!({"label": format!("枝 {round} {n}"), "description": format!("説明 {round}")}),
            );
            value.insert(
                format!("Free:Note{n}"),
                json!({"body": format!("枝で書いた {round} {n}")}),
            );
        }
        let n = rebuilds_in(|| {
            ok(
                &core,
                "card_update",
                json!({"branch": "labels", "value": value}),
            );
        });
        assert!(n <= 3, "branch の 20 枚が全体を {n} 回組み直した");
    }
    let read = ok(
        &core,
        "card_read",
        json!({"branch": "labels", "target": "Schemas:Many9"}),
    );
    assert_eq!(read["label"], "枝 1 9", "{read}");
    // main is as it was.
    let main = ok(&core, "card_read", json!({"target": "Schemas:Many9"}));
    assert_ne!(main["label"], "枝 1 9", "{main}");
    // The board: notes made, then written again in one call.
    let mut notes = serde_json::Map::new();
    for n in 0..5 {
        notes.insert(
            format!("Goals:Board{n}"),
            json!({"kind": "note", "body": format!("ボード {n}")}),
        );
    }
    ok(
        &core,
        "card_create",
        json!({"branch": "_project_board_", "value": notes}),
    );
    let mut value = serde_json::Map::new();
    for n in 0..5 {
        value.insert(
            format!("Goals:Board{n}"),
            json!({"body": format!("書き直した {n}"), "label": format!("札 {n}")}),
        );
    }
    let n = rebuilds_in(|| {
        ok(
            &core,
            "card_update",
            json!({"branch": "_project_board_", "value": value}),
        );
    });
    assert!(n <= 3, "ボードの 5 枚が全体を {n} 回組み直した");
}

/// Cards let go of, lines added, and a goal's and a task's fields written, each many in one call,
/// work the project out a few times, not once a card. What the writes touch is brought up
/// instead — the cards written and those whose entries say them — and, built for the tests, held
/// to what working it out again makes (`Seen::in_tx`).
#[test]
fn cards_let_go_lines_added_and_the_board_written_work_the_project_out_a_few_times() {
    let core = core();
    with_many(&core, 30);
    // Each entity a line to the one after it, then the lines' cards let go of.
    let mut value = serde_json::Map::new();
    for n in 0..29 {
        value.insert(
            format!("Schemas:Many{n}"),
            json!({"connections": {"add": [
                {"line": "points", "from": "id", "to": format!("Schemas:Many{}", n + 1), "at": "id"}
            ]}}),
        );
    }
    let n = rebuilds_in(|| {
        ok(&core, "card_update", json!({"value": value}));
    });
    println!("lines added to 29: {n}");
    assert!(n <= 3, "29 本の線が全体を {n} 回組み直した");
    let read = ok(&core, "card_read", json!({"target": "Schemas:Many0"}));
    assert!(read.to_string().contains("Schemas:Many1"), "{read}");
    let targets: Vec<String> = (10..30).map(|n| format!("Schemas:Many{n}")).collect();
    let mut deleted = Value::Null;
    let n = rebuilds_in(|| {
        deleted = ok(&core, "card_delete", json!({"target": targets}));
    });
    println!("card_delete of 20: {n}");
    assert!(n <= 3, "20 枚の card_delete が全体を {n} 回組み直した");
    // The one before the first let go of says its line now points nowhere.
    assert!(deleted.to_string().contains("Many9"), "{deleted}");
    let (refused, _) = common::call(
        &core,
        "card_read",
        json!({"project": PROJECT, "target": "Schemas:Many10"}),
    );
    assert!(refused);
    // A goal and its tasks on the board, written a step at a time and many in one call.
    let mut value = serde_json::Map::new();
    value.insert(
        "Goals:Speed".into(),
        json!({"kind": "goal", "body": "測る"}),
    );
    for t in 0..10 {
        value.insert(
            format!("Goals:Speed.T{t}"),
            json!({"kind": "task", "body": format!("[[Schemas:Many{t}]]")}),
        );
    }
    ok(
        &core,
        "card_create",
        json!({"branch": "_project_board_", "value": value}),
    );
    let n = rebuilds_in(|| {
        ok(
            &core,
            "card_update",
            json!({"branch": "_project_board_", "target": "Goals:Speed.T1", "value": {"done": true}}),
        );
    });
    assert!(n <= 2, "task の 1 手が全体を {n} 回組み直した");
    let mut value = serde_json::Map::new();
    for t in 0..10 {
        value.insert(
            format!("Goals:Speed.T{t}"),
            json!({"done": t % 2 == 0, "body": format!("書き直した [[Schemas:Many{t}]]")}),
        );
    }
    value.insert("Goals:Speed".into(), json!({"status": "inProgress"}));
    let n = rebuilds_in(|| {
        ok(
            &core,
            "card_update",
            json!({"branch": "_project_board_", "value": value}),
        );
    });
    println!("board of 11: {n}");
    assert!(n <= 3, "ボードの 11 枚が全体を {n} 回組み直した");
    let goal = ok(
        &core,
        "card_read",
        json!({"branch": "_project_board_", "target": "Goals:Speed"}),
    );
    assert_eq!(goal["progress"], json!({"done": 5, "of": 10}), "{goal}");
}

/// Cards renamed, many in one call, work the project out a few times, and the paths written
/// elsewhere follow them. Built for the tests, what following a rename rewrites is held to what
/// following it through every card rewrites (`follow::follow`).
#[test]
fn cards_renamed_in_one_call_work_the_project_out_a_few_times_and_are_followed() {
    let core = core();
    with_many(&core, 20);
    // A note that names each, and a line from each to the one after it.
    let body: Vec<String> = (0..20).map(|n| format!("[[Schemas:Many{n}]]")).collect();
    ok(
        &core,
        "card_create",
        json!({"target": "Free:Index", "value": {"kind": "note", "body": body.join(" ")}}),
    );
    let mut value = serde_json::Map::new();
    for n in 0..19 {
        value.insert(
            format!("Schemas:Many{n}"),
            json!({"connections": {"add": [
                {"line": "points", "from": "id", "to": format!("Schemas:Many{}", n + 1), "at": "id"}
            ]}}),
        );
    }
    ok(&core, "card_update", json!({"value": value}));
    let mut value = serde_json::Map::new();
    for n in 0..20 {
        value.insert(
            format!("Schemas:Many{n}"),
            json!({"name": format!("Renamed{n}")}),
        );
    }
    let mut said = Value::Null;
    let n = rebuilds_in(|| {
        said = ok(&core, "card_update", json!({"value": value}));
    });
    println!("renamed 20: {n}");
    assert!(n <= 3, "20 枚の改名が全体を {n} 回組み直した");
    assert!(said.to_string().contains("書き換えました"), "{said}");
    let index = ok(&core, "card_read", json!({"target": "Free:Index"}));
    let text = index["body"].as_str().unwrap_or_default();
    assert!(
        text.contains("[[Schemas:Renamed0]]") && text.contains("[[Schemas:Renamed19]]"),
        "{index}"
    );
    assert!(!text.contains("Many"), "{index}");
    let read = ok(&core, "card_read", json!({"target": "Schemas:Renamed0"}));
    assert!(read.to_string().contains("Schemas:Renamed1"), "{read}");
}

/// Containers renamed — a flow, a goal — are brought up as any card is, and what they hold with
/// them: the paths of their nodes and tasks are made again with theirs, every other path is as it
/// was. Built for the tests, each time it is brought up it is held to what working it out again
/// makes (`Seen::in_tx`).
#[test]
fn containers_renamed_are_brought_up_with_what_they_hold() {
    let core = core();
    ok(
        &core,
        "card_create",
        json!({"target": "Free:Paths", "value": {"kind": "note",
            "body": "[[Charts/Shop:Checkout.save]] [[Charts/Shop:Refund.ask]]"}}),
    );
    let n = rebuilds_in(|| {
        ok(
            &core,
            "card_update",
            json!({"value": {
                "Charts/Shop:Checkout": {"name": "Purchase"},
                "Charts/Shop:Refund": {"name": "Return"},
                "Free:Paths": {"label": "パス"},
            }}),
        );
    });
    println!("flows renamed: {n}");
    assert!(n <= 3, "flow 2 つの改名が全体を {n} 回組み直した");
    let note = ok(&core, "card_read", json!({"target": "Free:Paths"}));
    let body = note["body"].as_str().unwrap_or_default();
    assert!(
        body.contains("[[Charts/Shop:Purchase.save]]")
            && body.contains("[[Charts/Shop:Return.ask]]"),
        "{note}"
    );
    ok(
        &core,
        "card_read",
        json!({"target": "Charts/Shop:Purchase.save"}),
    );
    // On the board: the goal renamed, a task that names the other follows it.
    ok(
        &core,
        "card_update",
        json!({"branch": "_project_board_", "target": "Goals:Ship.Design",
            "value": {"body": "[[Goals:Ship.t2]]"}}),
    );
    let n = rebuilds_in(|| {
        ok(
            &core,
            "card_update",
            json!({"branch": "_project_board_", "value": {
                "Goals:Ship.t2": {"done": true},
                "Goals:Ship": {"name": "Launch"},
            }}),
        );
    });
    println!("goal renamed: {n}");
    assert!(n <= 3, "goal の改名が全体を {n} 回組み直した");
    let task = ok(
        &core,
        "card_read",
        json!({"branch": "_project_board_", "target": "Goals:Launch.Design"}),
    );
    assert_eq!(task["body"], "[[Goals:Launch.t2]]", "{task}");
    let goal = ok(
        &core,
        "card_read",
        json!({"branch": "_project_board_", "target": "Goals:Launch"}),
    );
    // Design was done already; t2 is now.
    assert_eq!(goal["progress"], json!({"done": 2, "of": 2}), "{goal}");
}

/// `count` flows in the sample's `Charts/Shop`, each with 6 nodes (a begin, 4 processes each reading
/// an order's total, an end), as one `card_create`'s value.
fn flows(count: usize) -> Value {
    let mut value = serde_json::Map::new();
    for f in 0..count {
        let flow = format!("Charts/Shop:Many{f}");
        value.insert(flow.clone(), json!({"kind": "flow"}));
        value.insert(
            format!("{flow}.start"),
            json!({"kind": "begin", "trigger": "manual", "connections": [{"line": "next", "to": "p1"}]}),
        );
        for k in 1..=4 {
            let next = if k == 4 {
                "done".to_string()
            } else {
                format!("p{}", k + 1)
            };
            value.insert(
                format!("{flow}.p{k}"),
                json!({"kind": "process", "steps": [{"do": "read"}], "connections": [
                    {"line": "reads", "from": "steps[0]", "to": "Schemas/Ordering:Order", "at": "total"},
                    {"line": "next", "to": next}
                ]}),
            );
        }
        value.insert(
            format!("{flow}.done"),
            json!({"kind": "end", "outcome": "success"}),
        );
    }
    json!({"value": value})
}

/// Flows made with their nodes in one call work the project out once, and bring it up once — not
/// once a flow nor once a node: the call's cards are all written, the project is brought up once and
/// each flow's nodes are checked in it, and the places of the nodes of the flows that have none come
/// from one working out.
#[test]
fn flows_made_with_their_nodes_in_one_call_work_the_project_out_once() {
    let core = core();
    let before = project_bring_ups_on_this_thread();
    let n = rebuilds_in(|| {
        let out = ok(&core, "card_create", flows(8));
        assert!(out.get("refused").is_none(), "{out}");
    });
    let brought = project_bring_ups_on_this_thread() - before;
    println!("8 flows of 6 nodes: worked out {n}, brought up {brought}");
    assert!(n <= 2, "flow 8 つの card_create が全体を {n} 回組み直した");
    assert!(
        brought <= 2,
        "flow 8 つの card_create が全体を {brought} 回持ち上げた（ノードごとに持ち上げている）"
    );
    // Every flow's nodes have their places on its card map, each flow's alike (all are alike).
    let spots = |f: usize| -> Vec<Value> {
        ["start", "p1", "p2", "p3", "p4", "done"]
            .iter()
            .map(|node| {
                ok(
                    &core,
                    "card_read",
                    json!({"target": format!("Charts/Shop:Many{f}.{node}")}),
                )["topLeft"]
                    .clone()
            })
            .collect()
    };
    let first = spots(0);
    assert!(first.iter().all(Value::is_object), "{first:?}");
    for f in 1..8 {
        assert_eq!(spots(f), first, "Many{f} のノードの置き場");
    }
}
