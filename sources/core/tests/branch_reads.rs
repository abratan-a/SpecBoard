//! What a tree is read for: `project_read` says the project itself, `branch_read` a tree — what a
//! branch changed (`as: diff`), its diagram and its history — and `branch_list` lists the branches;
//! what is wrong in a tree is `inspection_list`'s (read whole here through `common::checked`). The
//! board and the working directories' tree are named `_project_board_` and `_directory_`.

use crate::common;

use common::{checked, ok, refused, script_store, text_in};
use serde_json::{Value, json};
use specboard_core::Core;

fn core() -> Core {
    Core::open(script_store()).unwrap()
}

#[test]
fn the_project_is_read_apart_from_what_is_wrong_in_a_tree() {
    let core = core();
    let read = ok(&core, "project_read", json!({}));
    let keys: Vec<&str> = read
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        keys,
        [
            "project",
            "description",
            "access",
            "mcpAsk",
            "exportLanguage",
            "counts",
            "tags",
            "projectTree",
            "layout",
            "size"
        ],
        "{read}"
    );
    // The tree is branch_read's: project_read takes none.
    let said = refused(&core, "project_read", json!({"branch": "refunds"}));
    assert!(said.contains("受け付けない引数です: branch"), "{said}");
    let said = refused(&core, "project_read", json!({"value": {"as": "diagram"}}));
    assert!(said.contains("rules"), "{said}");
    // The rules: only the places with one of their own.
    let rules = ok(&core, "project_read", json!({"value": {"as": "rules"}}));
    let places: Vec<&str> = rules["places"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["place"].as_str().unwrap())
        .collect();
    assert!(places.contains(&"card.description"), "{rules}");
    assert!(!places.contains(&"card.entity.description"), "{rules}");
    assert!(
        rules["places"]
            .as_array()
            .unwrap()
            .iter()
            .all(|p| p.get("rule").is_some())
    );
    let one = ok(
        &core,
        "project_read",
        json!({"value": {"as": "rules", "place": "card.entity.description"}}),
    );
    assert_eq!(one["setAt"], "card.description", "{one}");
}

#[test]
fn each_tree_is_checked_by_its_name() {
    let core = core();
    let main = checked(&core, "main");
    assert_eq!(main["branch"], "main");
    // The sample's main says three paths that reach nothing (`Docs:Overview`'s body).
    let broken = main["brokenPaths"].to_string();
    assert_eq!(main["brokenPaths"].as_array().unwrap().len(), 3, "{main}");
    for path in ["Nope:Missing", "Nope:Gone", "Schemas:Nothing"] {
        assert!(broken.contains(path), "{path}: {main}");
    }
    let branch = checked(&core, "REFUNDS");
    assert_eq!(branch["branch"], "refunds");
    let board = checked(&core, "_project_board_");
    assert_eq!(board["branch"], "_project_board_");
    assert!(board.get("brokenPaths").is_none(), "{board}");
    let said = refused(&core, "inspection_list", json!({"branch": "nope"}));
    assert!(said.contains("main / refunds / _project_board_"), "{said}");
    // A branch of the app's own is not MCP's.
    let said = refused(&core, "inspection_list", json!({"branch": "~group"}));
    assert!(said.contains("branch「~group」はありません"), "{said}");
    // The list takes nothing else in its value.
    let said = refused(&core, "inspection_list", json!({"value": {"full": true}}));
    assert!(said.contains("full"), "{said}");
}

#[test]
fn the_check_is_not_a_way_to_read_a_branch() {
    let core = core();
    // `as: verify` is gone: the check of a tree is inspection_list's.
    let said = refused(
        &core,
        "branch_read",
        json!({"target": "main", "value": {"as": "verify"}}),
    );
    assert_eq!(
        said,
        "as: verify はありません。点検は inspection_list で読みます（1 件の中身は inspection_read、設計どおりの印は inspection_update）"
    );
    // Of no other as either.
    let said = refused(
        &core,
        "branch_read",
        json!({"target": "main", "value": {"as": "check"}}),
    );
    assert!(said.contains("diff・history・diagram"), "{said}");
    assert!(!said.contains("verify"), "{said}");
}

#[test]
fn a_tree_is_drawn_and_its_history_read() {
    let core = core();
    // Drawn as JSON: the tree it is of, what the diagram was drawn from, and the mermaid.
    let drawn = ok(
        &core,
        "branch_read",
        json!({"target": "refunds", "value": {"as": "diagram"}}),
    );
    assert_eq!(drawn["branch"], "refunds", "{drawn}");
    assert_eq!(drawn["roots"], "*", "{drawn}");
    assert!(drawn["rev"].is_string(), "{drawn}");
    assert!(
        text_in(&drawn, "mermaid").starts_with("classDiagram\n"),
        "{drawn}"
    );
    ok(
        &core,
        "card_update",
        json!({"target": "Schemas:Customer", "value": {"description": "一つ目"}}),
    );
    ok(
        &core,
        "card_update",
        json!({"target": "Schemas:Customer", "value": {"description": "二つ目"}}),
    );
    let history = ok(
        &core,
        "branch_read",
        json!({"target": "main", "value": {"as": "history", "limit": 1}}),
    );
    assert_eq!(history["commits"].as_array().unwrap().len(), 1, "{history}");
    // Cut, through MCP it says how many there are and the seq to go on from, as every list's next is:
    // a string.
    assert_eq!(history["truncated"], true, "{history}");
    assert_eq!(
        history["next"],
        history["commits"][0]["seq"].to_string().as_str()
    );
    let total = history["matched"].as_u64().unwrap();
    let mut seqs = vec![history["commits"][0]["seq"].as_i64().unwrap()];
    let mut after = history["next"].clone();
    while !after.is_null() {
        let more = ok(
            &core,
            "branch_read",
            json!({"target": "main", "value": {"as": "history", "limit": 1, "after": after}}),
        );
        seqs.extend(
            more["commits"]
                .as_array()
                .unwrap()
                .iter()
                .map(|c| c["seq"].as_i64().unwrap()),
        );
        after = more.get("next").cloned().unwrap_or(Value::Null);
    }
    assert_eq!(seqs.len() as u64, total, "{seqs:?}");
    assert!(seqs.windows(2).all(|w| w[0] > w[1]), "{seqs:?}");
    let said = refused(
        &core,
        "branch_read",
        json!({"target": "main", "value": {"as": "history", "limit": 0}}),
    );
    assert!(said.contains("1〜1000"), "{said}");
    // A tree that is not a branch has no main to differ from.
    let said = refused(&core, "branch_read", json!({"target": "main"}));
    assert!(said.contains("main は branch ではない"), "{said}");
}

#[test]
fn the_branches_are_listed_and_what_they_changed_is_read_by_paths() {
    let core = core();
    let listed = ok(&core, "branch_list", json!({}));
    let row = listed["branches"][0].as_object().unwrap();
    for key in row.keys() {
        assert!(
            ["branch", "changes", "mainMoved", "merged"].contains(&key.as_str()),
            "{listed}"
        );
    }
    assert_eq!(row["branch"], "refunds", "{listed}");
    assert!(row["changes"].as_u64().is_some(), "{listed}");
    // A line taken out to a card the branch then removed: said by the path it has in main.
    let refunds = json!({"branch": "refunds"});
    let mut take = refunds.clone();
    take["target"] = "Schemas/Ordering:Order".into();
    take["value"] =
        json!({"connections": {"remove": [{"line": "signedBy", "to": "Schemas:Customer"}]}});
    ok(&core, "card_update", take);
    let mut gone = refunds.clone();
    gone["target"] = "Schemas:Customer".into();
    ok(&core, "card_delete", gone);
    let diff = ok(&core, "branch_read", json!({"target": "refunds"}));
    let order = diff["changed"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["card"] == "Schemas/Ordering:Order")
        .unwrap_or_else(|| panic!("{diff}"))
        .to_string();
    assert!(order.contains("Schemas:Customer"), "{order}");
    assert!(!order.contains("1a420d63"), "{order}");
    // And the line left to it is said as broken, by the path the card had.
    let after = checked(&core, "refunds");
    let line = after["brokenPaths"]
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["via"] == "connections.points.to")
        .unwrap_or_else(|| panic!("{after}"))
        .clone();
    assert_eq!(line["path"], "Schemas:Customer", "{after}");
}

#[test]
fn the_board_and_the_working_directories_are_named_as_the_apps_own() {
    let core = core();
    let read = ok(&core, "section_list", json!({"branch": "_project_board_"}));
    assert_eq!(read["branch"], "_project_board_");
    // board is a branch's name like any other now; a name wrapped in _ is the app's.
    ok(&core, "branch_create", json!({"target": "board"}));
    for name in ["_project_board_", "_directory_"] {
        let said = refused(&core, "branch_create", json!({"target": name}));
        assert!(said.contains("アプリが使う名前"), "{name}: {said}");
    }
    let said = refused(
        &core,
        "branch_create",
        json!({"target": "later", "value": {"copy": false, "x": 1}}),
    );
    assert!(said.contains("value の copy は外しました"), "{said}");
    // A value of no keys says so, not an empty list.
    let said = refused(
        &core,
        "branch_create",
        json!({"target": "later", "value": {"x": 1}}),
    );
    assert!(
        said.contains("value にはキーがありません（書かれたのは x）"),
        "{said}"
    );
}

#[test]
fn a_display_name_that_was_empty_is_said_as_empty() {
    // An empty name is said as empty, not as nothing between two spaces.
    let core = core();
    ok(
        &core,
        "card_create",
        json!({"target": "Schemas:Port", "value": {"kind": "entity"}}),
    );
    ok(&core, "branch_create", json!({"target": "feat"}));
    ok(
        &core,
        "card_update",
        json!({"branch": "feat", "target": "Schemas:Port", "value": {"label": "ポート-feat"}}),
    );
    let read = ok(&core, "branch_read", json!({"target": "feat"}));
    let said = read.to_string();
    assert!(said.contains("表示名 （空） → ポート-feat"), "{said}");
    assert!(!said.contains("表示名  →"), "{said}");
}

/// The flows' exchanges the check says, by where each is written.
fn peers(read: &Value) -> Vec<(String, String)> {
    read["peerMismatches"]
        .as_array()
        .map(|rows| {
            rows.iter()
                .map(|r| {
                    (
                        r["at"].as_str().unwrap().to_string(),
                        r["problem"].as_str().unwrap().to_string(),
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn a_send_or_a_receive_the_other_flow_does_not_answer_is_checked_from_its_side() {
    let core = core();
    // The sample's refund sends to the checkout and waits to be sent from it, and the checkout
    // neither takes nor sends: both of the refund's sides are said.
    let read = checked(&core, "main");
    let said = peers(&read);
    let at: Vec<&str> = said.iter().map(|(a, _)| a.as_str()).collect();
    assert_eq!(
        at,
        [
            "Charts/Shop:Refund.start",
            "Charts/Shop:Refund.ask.steps[1]"
        ],
        "{read}"
    );
    assert!(said[0].1.contains("Charts/Shop:Refund へ送る行"), "{read}");
    assert!(
        said[1].1.contains("Charts/Shop:Refund から受ける口"),
        "{read}"
    );
    assert_eq!(
        read["peerMismatches"][1]["carries"],
        json!(["Schemas/Ordering:Order.id"])
    );
    assert_eq!(read["peerMismatches"][1]["peer"], "Charts/Shop:Checkout");
    // A sink that takes from whoever sends, but something else than the log sends.
    ok(
        &core,
        "card_create",
        json!({"value": {
            "Charts/Shop:Sink": {"kind": "flow"},
            "Charts/Shop:Sink.start": {"kind": "begin", "trigger": "receive", "carries": ["Schemas/Ordering:Order.total"],
                "connections": [{"line": "next", "to": "done"}]},
            "Charts/Shop:Sink.done": {"kind": "end", "outcome": "success"},
            "Charts/Shop:Log": {"kind": "flow"},
            "Charts/Shop:Log.start": {"kind": "begin", "trigger": "manual", "connections": [{"line": "next", "to": "push"}]},
            "Charts/Shop:Log.push": {"kind": "process", "steps": [{"do": "send", "carries": ["Schemas/Ordering:Order.id"]}],
                "connections": [{"line": "peer", "from": "steps[0]", "to": "Charts/Shop:Sink"}, {"line": "next", "to": "done"}]},
            "Charts/Shop:Log.done": {"kind": "end", "outcome": "success"}
        }}),
    );
    let said = peers(&checked(&core, "main"));
    let log = said
        .iter()
        .find(|(a, _)| a == "Charts/Shop:Log.push.steps[0]")
        .unwrap_or_else(|| panic!("{said:?}"));
    assert!(
        log.1
            .contains("Schemas/Ordering:Order.id を Charts/Shop:Sink の受ける口が受けません"),
        "{}",
        log.1
    );
    assert!(log.1.contains("Charts/Shop:Sink.start"), "{}", log.1);
    // A begin naming no one is said against every flow naming its flow: it takes not what the log
    // sends, and what it takes no one sends.
    let sink: Vec<&str> = said
        .iter()
        .filter(|(a, _)| a == "Charts/Shop:Sink.start")
        .map(|(_, p)| p.as_str())
        .collect();
    assert_eq!(sink.len(), 2, "{said:?}");
    assert!(
        sink[0].contains("Charts/Shop:Log から送られる Schemas/Ordering:Order.id を"),
        "{}",
        sink[0]
    );
    assert!(
        sink[1].contains("Schemas/Ordering:Order.total はどの送り手も送りません"),
        "{}",
        sink[1]
    );
    // Taking the whole order takes its id too.
    ok(
        &core,
        "card_update",
        json!({"target": "Charts/Shop:Sink.start", "value": {"carries": ["Schemas/Ordering:Order"]}}),
    );
    let said = peers(&checked(&core, "main"));
    assert!(
        !said
            .iter()
            .any(|(a, _)| a.starts_with("Charts/Shop:Log") || a.starts_with("Charts/Shop:Sink")),
        "{said:?}"
    );
    // A sink that names the refund as who sends: the refund sends nothing there, and the log's
    // send is no longer taken.
    ok(
        &core,
        "card_update",
        json!({"target": "Charts/Shop:Sink.start", "value": {"connections": {"add": [{"line": "peer", "to": "Charts/Shop:Refund"}]}}}),
    );
    let said = peers(&checked(&core, "main"));
    assert!(
        said.iter().any(|(a, p)| a == "Charts/Shop:Sink.start"
            && p.contains("Charts/Shop:Refund に、Charts/Shop:Sink へ送る行")),
        "{said:?}"
    );
    assert!(
        said.iter()
            .any(|(a, p)| a == "Charts/Shop:Log.push.steps[0]"
                && p.contains("Charts/Shop:Log から受ける口")),
        "{said:?}"
    );
}

#[test]
fn each_flow_a_place_names_is_checked_on_its_own_and_a_program_beside_them_is_not() {
    let core = core();
    // A door naming two senders and a program: the client sends there, the agent never does.
    ok(
        &core,
        "card_create",
        json!({"value": {
            "Charts/Shop:Usher": {"kind": "flow"},
            "Charts/Shop:Usher.start": {"kind": "begin", "trigger": "receive", "program": "外の監視",
                "carries": ["Schemas/Ordering:Order.id"],
                "connections": [{"line": "peer", "to": "Charts/Shop:Client"},
                    {"line": "peer", "to": "Charts/Shop:Agent"}, {"line": "next", "to": "done"}]},
            "Charts/Shop:Usher.done": {"kind": "end", "outcome": "success"},
            "Charts/Shop:Client": {"kind": "flow"},
            "Charts/Shop:Client.start": {"kind": "begin", "trigger": "manual", "connections": [{"line": "next", "to": "ask"}]},
            "Charts/Shop:Client.ask": {"kind": "process", "steps": [{"do": "send", "carries": ["Schemas/Ordering:Order.id"]}],
                "connections": [{"line": "peer", "from": "steps[0]", "to": "Charts/Shop:Usher"},
                    {"line": "peer", "from": "steps[0]", "to": "Charts/Shop:Agent"}, {"line": "next", "to": "done"}]},
            "Charts/Shop:Client.done": {"kind": "end", "outcome": "success"},
            "Charts/Shop:Agent": {"kind": "flow"},
            "Charts/Shop:Agent.start": {"kind": "begin", "trigger": "manual", "connections": [{"line": "next", "to": "done"}]},
            "Charts/Shop:Agent.done": {"kind": "end", "outcome": "success"}
        }}),
    );
    let read = checked(&core, "main");
    let ours: Vec<&Value> = read["peerMismatches"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|m| {
            [
                "Charts/Shop:Usher",
                "Charts/Shop:Client",
                "Charts/Shop:Agent",
            ]
            .iter()
            .any(|f| m["at"].as_str().unwrap().starts_with(f))
        })
        .collect();
    // The begin is said once, for the agent alone; the client's send once, for the agent alone.
    let said: Vec<(&str, &str)> = ours
        .iter()
        .map(|m| (m["at"].as_str().unwrap(), m["peer"].as_str().unwrap()))
        .collect();
    assert_eq!(
        said,
        [
            ("Charts/Shop:Usher.start", "Charts/Shop:Agent"),
            ("Charts/Shop:Client.ask.steps[0]", "Charts/Shop:Agent")
        ],
        "{read}"
    );
    let problem = ours[0]["problem"].as_str().unwrap();
    assert!(
        problem.contains("ここから Charts/Shop:Agent への peer の線を外します"),
        "{problem}"
    );
}

#[test]
fn a_return_naming_no_one_answers_the_flow_that_called_and_waits() {
    let core = core();
    // A door many flows call: it takes from whoever sends and returns to whoever called, naming
    // no one. A caller sends there and waits for the answer.
    ok(
        &core,
        "card_create",
        json!({"value": {
            "Charts/Shop:Door": {"kind": "flow"},
            "Charts/Shop:Door.start": {"kind": "begin", "trigger": "receive", "carries": ["Schemas/Ordering:Order.id"],
                "connections": [{"line": "next", "to": "done"}]},
            "Charts/Shop:Door.done": {"kind": "end", "outcome": "success", "return": {"carries": ["Schemas/Ordering:Order.total"]}},
            "Charts/Shop:Caller": {"kind": "flow"},
            "Charts/Shop:Caller.start": {"kind": "begin", "trigger": "manual", "connections": [{"line": "next", "to": "ask"}]},
            "Charts/Shop:Caller.ask": {"kind": "process",
                "steps": [{"do": "send", "carries": ["Schemas/Ordering:Order.id"]}, {"do": "receive", "carries": ["Schemas/Ordering:Order.total"]}],
                "connections": [{"line": "peer", "from": "steps[0]", "to": "Charts/Shop:Door"},
                    {"line": "peer", "from": "steps[1]", "to": "Charts/Shop:Door"}, {"line": "next", "to": "done"}]},
            "Charts/Shop:Caller.done": {"kind": "end", "outcome": "success"},
            // Waits for the door but never calls it: no return comes back here.
            "Charts/Shop:Waiter": {"kind": "flow"},
            "Charts/Shop:Waiter.start": {"kind": "begin", "trigger": "manual", "connections": [{"line": "next", "to": "wait"}]},
            "Charts/Shop:Waiter.wait": {"kind": "process", "steps": [{"do": "receive"}],
                "connections": [{"line": "peer", "from": "steps[0]", "to": "Charts/Shop:Door"}, {"line": "next", "to": "done"}]},
            "Charts/Shop:Waiter.done": {"kind": "end", "outcome": "success"},
            // Started by the door: a return to whoever called starts no new run.
            "Charts/Shop:Started": {"kind": "flow"},
            "Charts/Shop:Started.start": {"kind": "begin", "trigger": "receive",
                "connections": [{"line": "peer", "to": "Charts/Shop:Door"}, {"line": "next", "to": "done"}]},
            "Charts/Shop:Started.done": {"kind": "end", "outcome": "success"}
        }}),
    );
    let said = peers(&checked(&core, "main"));
    assert!(
        !said
            .iter()
            .any(|(a, _)| a.starts_with("Charts/Shop:Caller") || a.starts_with("Charts/Shop:Door")),
        "{said:?}"
    );
    let waiter = said
        .iter()
        .find(|(a, _)| a == "Charts/Shop:Waiter.wait.steps[0]")
        .unwrap_or_else(|| panic!("{said:?}"));
    assert!(
        waiter
            .1
            .contains("send・call か run で Charts/Shop:Door を呼んで"),
        "{}",
        waiter.1
    );
    assert!(
        said.iter().any(|(a, _)| a == "Charts/Shop:Started.start"),
        "{said:?}"
    );
}

#[test]
fn a_flow_another_runs_is_answered_by_the_run() {
    let core = core();
    // A chore the boss runs: its begin names the boss, and its end returns to the boss by name.
    // A run starts the flow it runs and waits for its end, so both are answered.
    ok(
        &core,
        "card_create",
        json!({"value": {
            "Charts/Shop:Chore": {"kind": "flow"},
            "Charts/Shop:Chore.start": {"kind": "begin", "trigger": "receive", "carries": ["Schemas/Ordering:Order.id"],
                "connections": [{"line": "peer", "to": "Charts/Shop:Boss"}, {"line": "next", "to": "done"}]},
            "Charts/Shop:Chore.done": {"kind": "end", "outcome": "success", "return": {"carries": ["Schemas/Ordering:Order.total"]},
                "connections": [{"line": "peer", "to": "Charts/Shop:Boss"}]},
            "Charts/Shop:Boss": {"kind": "flow"},
            "Charts/Shop:Boss.start": {"kind": "begin", "trigger": "manual", "connections": [{"line": "next", "to": "chore"}]},
            "Charts/Shop:Boss.chore": {"kind": "run", "carries": ["Schemas/Ordering:Order.id"], "returns": ["Schemas/Ordering:Order.total"],
                "connections": [{"line": "runs", "to": "Charts/Shop:Chore"}, {"line": "next", "to": "done"}]},
            "Charts/Shop:Boss.done": {"kind": "end", "outcome": "success"}
        }}),
    );
    let said = peers(&checked(&core, "main"));
    assert!(
        !said
            .iter()
            .any(|(a, _)| a.starts_with("Charts/Shop:Chore") || a.starts_with("Charts/Shop:Boss")),
        "{said:?}"
    );
    // A run carrying something else than the begin takes is said at the begin.
    ok(
        &core,
        "card_update",
        json!({"target": "Charts/Shop:Boss.chore", "value": {"carries": ["Schemas:Customer"]}}),
    );
    let said = peers(&checked(&core, "main"));
    let start = said
        .iter()
        .find(|(a, _)| a == "Charts/Shop:Chore.start")
        .unwrap_or_else(|| panic!("{said:?}"));
    assert!(
        start
            .1
            .contains("Charts/Shop:Boss から送られる Schemas:Customer を"),
        "{}",
        start.1
    );
    assert!(start.1.contains("Charts/Shop:Boss.chore"), "{}", start.1);
}

#[test]
fn a_begin_naming_a_flow_that_neither_runs_nor_sends_to_it_is_said() {
    let core = core();
    // The boss runs one chore and sends to none; the other chore names it all the same.
    ok(
        &core,
        "card_create",
        json!({"value": {
            "Charts/Shop:Chore": {"kind": "flow"},
            "Charts/Shop:Chore.start": {"kind": "begin", "trigger": "receive",
                "connections": [{"line": "next", "to": "done"}]},
            "Charts/Shop:Chore.done": {"kind": "end", "outcome": "success"},
            "Charts/Shop:Stray": {"kind": "flow"},
            "Charts/Shop:Stray.start": {"kind": "begin", "trigger": "receive",
                "connections": [{"line": "peer", "to": "Charts/Shop:Boss"}, {"line": "next", "to": "wait"}]},
            // A run starts a new run of the flow it runs: a receive mid-flow is not what it answers.
            "Charts/Shop:Stray.wait": {"kind": "process", "steps": [{"do": "receive"}],
                "connections": [{"line": "peer", "from": "steps[0]", "to": "Charts/Shop:Boss"}, {"line": "next", "to": "done"}]},
            "Charts/Shop:Stray.done": {"kind": "end", "outcome": "success"},
            "Charts/Shop:Boss": {"kind": "flow"},
            "Charts/Shop:Boss.start": {"kind": "begin", "trigger": "manual", "connections": [{"line": "next", "to": "chore"}]},
            "Charts/Shop:Boss.chore": {"kind": "run",
                "connections": [{"line": "runs", "to": "Charts/Shop:Chore"}, {"line": "next", "to": "done"}]},
            "Charts/Shop:Boss.done": {"kind": "end", "outcome": "success"}
        }}),
    );
    let said = peers(&checked(&core, "main"));
    let stray = said
        .iter()
        .find(|(a, _)| a == "Charts/Shop:Stray.start")
        .unwrap_or_else(|| panic!("{said:?}"));
    assert!(
        stray
            .1
            .contains("Charts/Shop:Stray を実行する run。Charts/Shop:Stray へ peer の線を引くか、run の runs の線を Charts/Shop:Stray へ引く"),
        "{}",
        stray.1
    );
    // Running the stray answers its begin, not the receive it waits on mid-flow.
    ok(
        &core,
        "card_create",
        json!({"value": {
            "Charts/Shop:Boss.stray": {"kind": "run",
                "connections": [{"line": "runs", "to": "Charts/Shop:Stray"}]}
        }}),
    );
    ok(
        &core,
        "card_update",
        json!({"target": "Charts/Shop:Boss.chore", "value": {"connections": {
            "remove": [{"line": "next", "to": "done"}], "add": [{"line": "next", "to": "stray"}]}}}),
    );
    ok(
        &core,
        "card_update",
        json!({"target": "Charts/Shop:Boss.stray", "value": {"connections": {"add": [{"line": "next", "to": "done"}]}}}),
    );
    let said = peers(&checked(&core, "main"));
    let at: Vec<&str> = said
        .iter()
        .map(|(a, _)| a.as_str())
        .filter(|a| a.starts_with("Charts/Shop:Stray") || a.starts_with("Charts/Shop:Boss"))
        .collect();
    assert_eq!(at, ["Charts/Shop:Stray.wait.steps[0]"], "{said:?}");
}

/// The exchanges the check says in the flows whose paths start with one of `flows`.
fn peers_in(core: &Core, flows: &[&str]) -> Vec<(String, String)> {
    peers(&checked(core, "main"))
        .into_iter()
        .filter(|(a, _)| flows.iter().any(|f| a.starts_with(&format!("{f}."))))
        .collect()
}

/// A flow that sends `carries` from its one row to `to`.
fn sender(name: &str, to: &str, carries: Value) -> Value {
    json!({
        format!("Charts/Shop:{name}"): {"kind": "flow"},
        format!("Charts/Shop:{name}.start"): {"kind": "begin", "trigger": "manual",
            "connections": [{"line": "next", "to": "push"}]},
        format!("Charts/Shop:{name}.push"): {"kind": "process", "steps": [{"do": "send", "carries": carries}],
            "connections": [{"line": "peer", "from": "steps[0]", "to": to}, {"line": "next", "to": "done"}]},
        format!("Charts/Shop:{name}.done"): {"kind": "end", "outcome": "success"}
    })
}

/// Cards for `card_create`, all of them in one value.
fn cards(parts: &[Value]) -> Value {
    let mut all = serde_json::Map::new();
    for part in parts {
        all.extend(part.as_object().unwrap().clone());
    }
    json!({ "value": all })
}

#[test]
fn a_begin_is_compared_with_what_all_its_senders_send() {
    let core = core();
    // Each sender sends one of the things the ledger takes.
    ok(
        &core,
        "card_create",
        cards(&[
            json!({
                "Charts/Shop:Ledger": {"kind": "flow"},
                "Charts/Shop:Ledger.start": {"kind": "begin", "trigger": "receive",
                    "carries": ["Schemas:Customer", "Schemas:Status"],
                    "connections": [{"line": "peer", "to": "Charts/Shop:One"},
                        {"line": "peer", "to": "Charts/Shop:Two"}, {"line": "next", "to": "done"}]},
                "Charts/Shop:Ledger.done": {"kind": "end", "outcome": "success"}
            }),
            sender("One", "Charts/Shop:Ledger", json!(["Schemas:Customer"])),
            sender("Two", "Charts/Shop:Ledger", json!(["Schemas:Status"])),
        ]),
    );
    let flows = ["Charts/Shop:Ledger", "Charts/Shop:One", "Charts/Shop:Two"];
    assert_eq!(peers_in(&core, &flows), []);
    // The second sends what the ledger does not take, and so no one sends the status.
    ok(
        &core,
        "card_update",
        json!({"target": "Charts/Shop:Two.push.steps[0]", "value": {"carries": ["Schemas/Ordering:Order"]}}),
    );
    let said = peers_in(&core, &flows);
    let at: Vec<&str> = said.iter().map(|(a, _)| a.as_str()).collect();
    assert_eq!(
        at,
        [
            "Charts/Shop:Ledger.start",
            "Charts/Shop:Ledger.start",
            "Charts/Shop:Two.push.steps[0]"
        ],
        "{said:?}"
    );
    assert!(
        said[0].1.contains(
            "Charts/Shop:Two から送られる Schemas/Ordering:Order を、受けるもの（Schemas:Customer・Schemas:Status）が受けません: Charts/Shop:Two.push.steps[0]"
        ),
        "{}",
        said[0].1
    );
    assert!(
        said[1]
            .1
            .contains("Schemas:Status はどの送り手も送りません"),
        "{}",
        said[1].1
    );
    assert!(
        said[2].1.contains(
            "送るもの（Schemas/Ordering:Order）のうち、Schemas/Ordering:Order を Charts/Shop:Ledger の受ける口が受けません"
        ),
        "{}",
        said[2].1
    );
    // A third sender the ledger names but that sends it nothing is said for itself as before.
    ok(
        &core,
        "card_create",
        json!({"value": {
            "Charts/Shop:Three": {"kind": "flow"},
            "Charts/Shop:Three.start": {"kind": "begin", "trigger": "manual", "connections": [{"line": "next", "to": "done"}]},
            "Charts/Shop:Three.done": {"kind": "end", "outcome": "success"}
        }}),
    );
    ok(
        &core,
        "card_update",
        json!({"target": "Charts/Shop:Ledger.start", "value": {"connections": {"add": [{"line": "peer", "to": "Charts/Shop:Three"}]}}}),
    );
    let said = peers_in(&core, &flows);
    assert!(
        said.iter().any(|(a, p)| a == "Charts/Shop:Ledger.start"
            && p.contains("Charts/Shop:Three に、Charts/Shop:Ledger へ送る行")),
        "{said:?}"
    );
}

#[test]
fn what_a_begin_takes_that_no_sender_sends_is_said_unless_a_sender_says_nothing() {
    let core = core();
    ok(
        &core,
        "card_create",
        cards(&[
            json!({
                "Charts/Shop:Ledger": {"kind": "flow"},
                "Charts/Shop:Ledger.start": {"kind": "begin", "trigger": "receive",
                    "carries": ["Schemas:Customer", "Schemas:Status", "Schemas/Ordering:Order"],
                    "connections": [{"line": "peer", "to": "Charts/Shop:One"},
                        {"line": "peer", "to": "Charts/Shop:Two"}, {"line": "next", "to": "done"}]},
                "Charts/Shop:Ledger.done": {"kind": "end", "outcome": "success"}
            }),
            sender("One", "Charts/Shop:Ledger", json!(["Schemas:Customer"])),
            sender("Two", "Charts/Shop:Ledger", json!(["Schemas:Status"])),
        ]),
    );
    let flows = ["Charts/Shop:Ledger", "Charts/Shop:One", "Charts/Shop:Two"];
    let said = peers_in(&core, &flows);
    assert_eq!(said.len(), 1, "{said:?}");
    assert_eq!(said[0].0, "Charts/Shop:Ledger.start");
    assert!(
        said[0].1.contains("Schemas/Ordering:Order はどの送り手も送りません: Charts/Shop:One.push.steps[0]（Schemas:Customer） / Charts/Shop:Two.push.steps[0]（Schemas:Status）"),
        "{}",
        said[0].1
    );
    let read = checked(&core, "main");
    let item = read["peerMismatches"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["at"] == "Charts/Shop:Ledger.start")
        .unwrap();
    assert_eq!(item["peer"], "Charts/Shop:One, Charts/Shop:Two", "{item}");
    // A sender that says nothing of what it carries may send the order.
    ok(
        &core,
        "card_update",
        json!({"target": "Charts/Shop:Two.push.steps[0]", "value": {"carries": []}}),
    );
    assert_eq!(peers_in(&core, &flows), []);
    // A begin that says nothing of what it takes is not compared.
    ok(
        &core,
        "card_update",
        json!({"target": "Charts/Shop:Ledger.start", "value": {"carries": []}}),
    );
    ok(
        &core,
        "card_update",
        json!({"target": "Charts/Shop:Two.push.steps[0]", "value": {"carries": ["Schemas:Anything"]}}),
    );
    assert_eq!(peers_in(&core, &flows), []);
}

#[test]
fn a_begin_naming_no_one_is_compared_with_every_send_naming_its_flow() {
    let core = core();
    ok(
        &core,
        "card_create",
        cards(&[
            json!({
                "Charts/Shop:Ledger": {"kind": "flow"},
                "Charts/Shop:Ledger.start": {"kind": "begin", "trigger": "receive",
                    "carries": ["Schemas:Customer", "Schemas:Status"],
                    "connections": [{"line": "next", "to": "done"}]},
                "Charts/Shop:Ledger.done": {"kind": "end", "outcome": "success"}
            }),
            sender("One", "Charts/Shop:Ledger", json!(["Schemas:Customer"])),
            sender("Two", "Charts/Shop:Ledger", json!(["Schemas:Status"])),
        ]),
    );
    let flows = [
        "Charts/Shop:Ledger",
        "Charts/Shop:One",
        "Charts/Shop:Two",
        "Charts/Shop:Three",
    ];
    assert_eq!(peers_in(&core, &flows), []);
    ok(
        &core,
        "card_create",
        cards(&[sender(
            "Three",
            "Charts/Shop:Ledger",
            json!(["Schemas/Ordering:Order"]),
        )]),
    );
    let said = peers_in(&core, &flows);
    let at: Vec<(&str, bool)> = said
        .iter()
        .map(|(a, p)| (a.as_str(), p.contains("Schemas/Ordering:Order")))
        .collect();
    assert_eq!(
        at,
        [
            ("Charts/Shop:Ledger.start", true),
            ("Charts/Shop:Three.push.steps[0]", true)
        ],
        "{said:?}"
    );
    assert!(
        said[0].1.contains("Charts/Shop:Three から送られる"),
        "{}",
        said[0].1
    );
}

/// A login with three ends returning to whoever called, and a caller that sends to it and waits,
/// taking `takes`.
fn login(takes: Value) -> Value {
    json!({"value": {
        "Charts/Shop:Login": {"kind": "flow"},
        "Charts/Shop:Login.start": {"kind": "begin", "trigger": "receive", "carries": ["Schemas:Customer.id"],
            "connections": [{"line": "next", "to": "logged_in"}, {"line": "next", "to": "refused_login"},
                {"line": "next", "to": "refused_shape"}]},
        "Charts/Shop:Login.logged_in": {"kind": "end", "outcome": "success", "return": {"carries": ["Schemas:Customer"]}},
        "Charts/Shop:Login.refused_login": {"kind": "end", "outcome": "failure", "return": {"carries": ["Schemas:Status"]}},
        "Charts/Shop:Login.refused_shape": {"kind": "end", "outcome": "failure", "return": {"carries": ["Schemas/Ordering:Order"]}},
        "Charts/Shop:Agent": {"kind": "flow"},
        "Charts/Shop:Agent.start": {"kind": "begin", "trigger": "manual", "connections": [{"line": "next", "to": "ask"}]},
        "Charts/Shop:Agent.ask": {"kind": "process",
            "steps": [{"do": "send", "carries": ["Schemas:Customer.id"]}, {"do": "receive", "carries": takes}],
            "connections": [{"line": "peer", "from": "steps[0]", "to": "Charts/Shop:Login"},
                {"line": "peer", "from": "steps[1]", "to": "Charts/Shop:Login"}, {"line": "next", "to": "done"}]},
        "Charts/Shop:Agent.done": {"kind": "end", "outcome": "success"}
    }})
}

#[test]
fn a_receive_waiting_for_a_call_takes_what_every_end_may_return() {
    let flows = ["Charts/Shop:Login", "Charts/Shop:Agent"];
    let core = core();
    ok(
        &core,
        "card_create",
        login(json!([
            "Schemas:Customer",
            "Schemas:Status",
            "Schemas/Ordering:Order"
        ])),
    );
    assert_eq!(peers_in(&core, &flows), []);
    // Taking only the login leaves the two refusals unreceived.
    let core = self::core();
    ok(&core, "card_create", login(json!(["Schemas:Customer"])));
    let said = peers_in(&core, &flows);
    assert_eq!(said.len(), 1, "{said:?}");
    assert_eq!(said[0].0, "Charts/Shop:Agent.ask.steps[1]");
    assert!(
        said[0].1.contains(
            "Charts/Shop:Login の end が返す Schemas:Status・Schemas/Ordering:Order を、受けるもの（Schemas:Customer）が受けません"
        ),
        "{}",
        said[0].1
    );
    assert!(
        said[0].1.contains(
            "Charts/Shop:Login.refused_login（Schemas:Status） / Charts/Shop:Login.refused_shape（Schemas/Ordering:Order）"
        ),
        "{}",
        said[0].1
    );
    // Taking what no end returns is said too.
    let core = self::core();
    ok(
        &core,
        "card_create",
        login(json!([
            "Schemas:Customer",
            "Schemas:Status",
            "Schemas/Ordering:Order",
            "Schemas:Receipt"
        ])),
    );
    let said = peers_in(&core, &flows);
    assert_eq!(said.len(), 1, "{said:?}");
    assert!(
        said[0]
            .1
            .contains("Schemas:Receipt は Charts/Shop:Login のどの end も返しません"),
        "{}",
        said[0].1
    );
    // A receive that says nothing of what it takes is not compared.
    let core = self::core();
    ok(&core, "card_create", login(json!([])));
    assert_eq!(peers_in(&core, &flows), []);
}

#[test]
fn a_receive_in_an_exchange_of_several_turns_takes_one_send() {
    let core = core();
    // The client sends, takes an answer, sends again and takes another; the desk answers each turn.
    ok(
        &core,
        "card_create",
        json!({"value": {
            "Charts/Shop:Client": {"kind": "flow"},
            "Charts/Shop:Client.start": {"kind": "begin", "trigger": "manual", "connections": [{"line": "next", "to": "talk"}]},
            "Charts/Shop:Client.talk": {"kind": "process",
                "steps": [{"do": "send", "carries": ["Schemas:Customer"]}, {"do": "receive", "carries": ["Schemas:Status"]},
                    {"do": "send", "carries": ["Schemas/Ordering:Order"]}, {"do": "receive", "carries": ["Schemas/Ordering:Order.total"]}],
                "connections": [{"line": "peer", "from": "steps[0]", "to": "Charts/Shop:Desk"},
                    {"line": "peer", "from": "steps[1]", "to": "Charts/Shop:Desk"},
                    {"line": "peer", "from": "steps[2]", "to": "Charts/Shop:Desk"},
                    {"line": "peer", "from": "steps[3]", "to": "Charts/Shop:Desk"}, {"line": "next", "to": "done"}]},
            "Charts/Shop:Client.done": {"kind": "end", "outcome": "success"},
            "Charts/Shop:Desk": {"kind": "flow"},
            "Charts/Shop:Desk.start": {"kind": "begin", "trigger": "receive", "carries": ["Schemas:Customer"],
                "connections": [{"line": "peer", "to": "Charts/Shop:Client"}, {"line": "next", "to": "talk"}]},
            "Charts/Shop:Desk.talk": {"kind": "process",
                "steps": [{"do": "send", "carries": ["Schemas:Status"]}, {"do": "receive", "carries": ["Schemas/Ordering:Order"]},
                    {"do": "send", "carries": ["Schemas/Ordering:Order.total"]}],
                "connections": [{"line": "peer", "from": "steps[0]", "to": "Charts/Shop:Client"},
                    {"line": "peer", "from": "steps[1]", "to": "Charts/Shop:Client"},
                    {"line": "peer", "from": "steps[2]", "to": "Charts/Shop:Client"}, {"line": "next", "to": "done"}]},
            "Charts/Shop:Desk.done": {"kind": "end", "outcome": "success"}
        }}),
    );
    let flows = ["Charts/Shop:Client", "Charts/Shop:Desk"];
    assert_eq!(peers_in(&core, &flows), []);
    // A turn that matches no single send is said as before, with every send it could be.
    ok(
        &core,
        "card_update",
        json!({"target": "Charts/Shop:Client.talk.steps[1]", "value": {"carries": ["Schemas:Status", "Schemas/Ordering:Order.total"]}}),
    );
    // The desk's turn that this receive was is then taken by no receive of the client either.
    let said = peers_in(&core, &flows);
    let mut at: Vec<&str> = said.iter().map(|(a, _)| a.as_str()).collect();
    at.sort_unstable();
    assert_eq!(
        at,
        [
            "Charts/Shop:Client.talk.steps[1]",
            "Charts/Shop:Desk.talk.steps[0]"
        ],
        "{said:?}"
    );
    assert!(
        said.iter().all(|(_, p)| p.contains("合いません")),
        "{said:?}"
    );
}

#[test]
fn a_path_takes_what_is_inside_it_and_is_sent_by_what_is_inside_it() {
    let core = core();
    ok(
        &core,
        "card_create",
        cards(&[
            json!({
                "Charts/Shop:Ledger": {"kind": "flow"},
                "Charts/Shop:Ledger.start": {"kind": "begin", "trigger": "receive",
                    "carries": ["Schemas/Ordering:Order"],
                    "connections": [{"line": "peer", "to": "Charts/Shop:One"},
                        {"line": "peer", "to": "Charts/Shop:Two"}, {"line": "next", "to": "done"}]},
                "Charts/Shop:Ledger.done": {"kind": "end", "outcome": "success"}
            }),
            sender(
                "One",
                "Charts/Shop:Ledger",
                json!(["Schemas/Ordering:Order.id"]),
            ),
            sender(
                "Two",
                "Charts/Shop:Ledger",
                json!(["Schemas/Ordering:Order.total"]),
            ),
        ]),
    );
    let flows = ["Charts/Shop:Ledger", "Charts/Shop:One", "Charts/Shop:Two"];
    assert_eq!(peers_in(&core, &flows), []);
    ok(
        &core,
        "card_update",
        json!({"target": "Charts/Shop:Ledger.start", "value": {"carries": ["Schemas/Ordering:Order.id", "Schemas/Ordering:Order.total"]}}),
    );
    ok(
        &core,
        "card_update",
        json!({"target": "Charts/Shop:One.push.steps[0]", "value": {"carries": ["Schemas/Ordering:Order"]}}),
    );
    assert_eq!(peers_in(&core, &flows), []);
}

/// The value and enum cards the check says nothing points at.
fn unreferenced(read: &Value) -> Vec<String> {
    read["unreferenced"]
        .as_array()
        .map(|rows| {
            rows.iter()
                .map(|r| r["at"].as_str().unwrap().to_string())
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn a_value_only_a_flows_node_uses_is_referenced() {
    let core = core();
    // One value a begin carries, one a process row reads by its line, one an end returns, and one
    // nothing uses.
    ok(
        &core,
        "card_create",
        json!({"value": {
            "Schemas:Coupon": {"kind": "value"},
            "Schemas:Coupon.code": {"type": "string"},
            "Schemas:Stamp": {"kind": "value"},
            "Schemas:Stamp.at": {"type": "string"},
            "Schemas:Receipt": {"kind": "value"},
            "Schemas:Receipt.total": {"type": "string"},
            "Schemas:Spare": {"kind": "value"},
            "Schemas:Spare.note": {"type": "string"},
            "Charts/Shop:Redeem": {"kind": "flow"},
            "Charts/Shop:Redeem.start": {"kind": "begin", "trigger": "receive", "carries": ["Schemas:Coupon.code"],
                "connections": [{"line": "next", "to": "look"}]},
            "Charts/Shop:Redeem.look": {"kind": "process", "steps": [{"do": "read"}],
                "connections": [{"line": "reads", "from": "steps[0]", "to": "Schemas:Stamp"}, {"line": "next", "to": "done"}]},
            "Charts/Shop:Redeem.done": {"kind": "end", "outcome": "success", "return": {"carries": ["Schemas:Receipt"]}}
        }}),
    );
    let read = checked(&core, "main");
    let said = unreferenced(&read);
    for used in ["Schemas:Coupon", "Schemas:Stamp", "Schemas:Receipt"] {
        assert!(!said.iter().any(|s| s == used), "{used}: {read}");
    }
    assert!(said.iter().any(|s| s == "Schemas:Spare"), "{read}");
}

#[test]
fn a_flows_problems_are_each_said_at_the_flow() {
    let core = core();
    // Unfinished, a node nothing leads to, and a node that comes back to itself: each said at the
    // flow, so a reader opens it without reading the sentence.
    ok(
        &core,
        "card_create",
        json!({"value": {
            "Charts/Shop:Loop": {"kind": "flow"},
            "Charts/Shop:Loop.start": {"kind": "begin", "trigger": "manual", "connections": [{"line": "next", "to": "spin"}]},
            "Charts/Shop:Loop.spin": {"kind": "process", "steps": [{"do": "read"}],
                "connections": [{"line": "next", "to": "spin"}]},
            "Charts/Shop:Loop.lost": {"kind": "end", "outcome": "failure"}
        }}),
    );
    let read = checked(&core, "main");
    let rows: Vec<&Value> = read["flowProblems"]
        .as_array()
        .unwrap_or_else(|| panic!("{read}"))
        .iter()
        .filter(|r| r["at"] == "Charts/Shop:Loop")
        .collect();
    let problems: Vec<&str> = rows
        .iter()
        .map(|r| r["problem"].as_str().unwrap())
        .collect();
    for said in [
        "Charts/Shop:Loop は未完成です: ",
        "Charts/Shop:Loop: ノード lost にはどこからも来ません",
        "Charts/Shop:Loop: ノード spin の next が自分に戻っています",
    ] {
        assert!(
            problems.iter().any(|p| p.starts_with(said)),
            "{said}: {read}"
        );
    }
}

#[test]
fn a_pointer_told_apart_by_a_field_no_card_pins_is_said() {
    let core = core();
    ok(
        &core,
        "card_create",
        json!({"value": {
            "Schemas:Event": {"kind": "entity"},
            "Schemas:Event.kind": {"type": "string"},
            "Schemas:Event.subject": {"type": "string"}
        }}),
    );
    ok(
        &core,
        "card_update",
        json!({"target": "Schemas:Event", "value": {"connections": {"add": [
            {"line": "points", "from": "subject", "fields": {"discriminatedBy": "kind"}}]}}}),
    );
    // No card of the section pins its kind: whatever the pointer holds leads nowhere.
    let read = checked(&core, "main");
    let rows = read["unresolvedPointers"].as_array().unwrap();
    assert_eq!(rows.len(), 1, "{read}");
    assert_eq!(rows[0]["at"], "Schemas:Event.subject", "{read}");
    assert_eq!(rows[0]["by"], "kind", "{read}");
    assert_eq!(rows[0]["kind"], "fix", "{read}");
    // A card pinning it is a candidate, and the pointer resolves as its value says.
    ok(
        &core,
        "card_create",
        json!({"target": "Schemas:Customer.kind", "value": {"type": "string", "const": "customer"}}),
    );
    let read = checked(&core, "main");
    assert!(read.get("unresolvedPointers").is_none(), "{read}");
}

#[test]
fn the_check_names_the_groups_it_says_that_are_there_by_design() {
    let core = core();
    let list = ok(&core, "inspection_list", json!({"value": {"show": "all"}}));
    // The sample's broken paths and peers are to fix; a card left in the free section can be as
    // meant.
    let by_design: Vec<&str> = list["groups"]
        .as_object()
        .unwrap()
        .iter()
        .filter(|(_, g)| g["kind"] == "info" && g["count"].as_u64().unwrap() > 0)
        .map(|(name, _)| name.as_str())
        .collect();
    assert_eq!(by_design, ["freeCards"], "{list}");
    assert_eq!(list["groups"]["brokenPaths"]["kind"], "fix", "{list}");
    // A page of the check carries the groups whole.
    let page = ok(
        &core,
        "inspection_list",
        json!({"value": {"show": "all", "limit": 1}}),
    );
    assert!(page.get("next").is_some(), "{page}");
    assert_eq!(page["groups"], list["groups"], "{page}");
    assert_eq!(page["counts"], list["counts"], "{page}");
}
