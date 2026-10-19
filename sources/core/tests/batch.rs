//! A change planned, then made, with one tool: steps tried in a batch opened for them, or run at
//! once; what is broken said once, as it stands when every step is done.

use crate::common;

use common::{call, script_store};
use serde_json::{Value, json};
use specboard_core::Core;

const PROJECT: &str = "見本";

fn core() -> Core {
    Core::open(script_store()).unwrap()
}

fn ok(core: &Core, mut arguments: Value) -> Value {
    if arguments.get("target").is_none() {
        arguments["project"] = PROJECT.into();
    }
    let (failed, text) = call(core, "batch", arguments.clone());
    assert!(!failed, "batch {arguments}: {text}");
    serde_json::from_str(&text).unwrap()
}

fn refused(core: &Core, mut arguments: Value) -> String {
    if arguments.get("target").is_none() {
        arguments["project"] = PROJECT.into();
    }
    let (failed, text) = call(core, "batch", arguments.clone());
    assert!(failed, "batch {arguments} が通った: {text}");
    text
}

fn body(core: &Core, target: &str) -> String {
    let (_, text) = call(
        core,
        "card_read",
        json!({"project": PROJECT, "target": target}),
    );
    let read: Value = serde_json::from_str(&text).unwrap();
    read["body"].as_str().unwrap_or_default().to_string()
}

/// A path written by one step and made to resolve by a later one, and one left as it is.
fn steps() -> Value {
    json!([
        {"tool": "card_update", "arguments": {"target": "Free:Memo", "value": {"body": "[[Schemas:Later]]"}}},
        {"tool": "card_create", "arguments": {"target": "Schemas:Later", "value": {"kind": "entity"}}},
        {"tool": "card_update", "arguments": {"target": "Free:Memo", "value": {"body": {"append": "[[Schemas:Never]]"}}}},
    ])
}

#[test]
fn what_is_broken_is_said_once_as_it_stands_when_the_steps_are_done() {
    let core = core();
    let tried = ok(&core, json!({"value": {"steps": steps(), "open": true}}));
    let token = tried["batch"].as_str().unwrap().to_string();
    assert_eq!(
        tried["said"],
        format!(
            "書いていません。流すのは batch の target に {token}、execute: true（捨てるのは value の discard: true）"
        )
    );
    let warnings = tried["warnings"].to_string();
    assert!(warnings.contains("Schemas:Never"), "{tried}");
    assert!(!warnings.contains("Schemas:Later"), "{tried}");
    for row in tried["steps"].as_array().unwrap() {
        assert!(row.get("warnings").is_none(), "{tried}");
    }
    assert_eq!(tried["staged"], 3);
    // Tried again as it is: the same.
    let again = ok(&core, json!({"target": token}));
    assert_eq!(again["warnings"], tried["warnings"]);
    assert_eq!(again["steps"].as_array().unwrap().len(), 3);
    // Nothing written until it is run.
    assert!(!body(&core, "Free:Memo").contains("Schemas:Never"));
    let ran = ok(&core, json!({"target": token, "execute": true}));
    assert_eq!(ran["executed"], 3, "{ran}");
    assert!(ran.get("steps").is_none(), "{ran}");
    assert!(
        ran["warnings"].to_string().contains("Schemas:Never"),
        "{ran}"
    );
    assert!(body(&core, "Free:Memo").contains("Schemas:Never"));
    // Run, the batch is gone.
    let said = refused(&core, json!({"target": token}));
    assert!(said.contains("はありません"), "{said}");
}

#[test]
fn steps_given_alone_are_only_tried_and_keep_no_batch() {
    let core = core();
    let tried = ok(&core, json!({"value": {"steps": steps()}}));
    assert!(tried.get("batch").is_none(), "{tried}");
    assert!(tried.get("staged").is_none(), "{tried}");
    assert!(
        tried["said"]
            .as_str()
            .unwrap()
            .starts_with("書いていません"),
        "{tried}"
    );
    let rows = tried["steps"].as_array().unwrap();
    assert_eq!(rows.len(), 3, "{tried}");
    assert_eq!(rows[1]["tool"], "card_create");
    assert!(tried["warnings"].to_string().contains("Schemas:Never"));
    assert!(!body(&core, "Free:Memo").contains("Schemas:Never"));
    // A step that does not go is told by its place among the steps.
    let said = refused(
        &core,
        json!({"value": {"steps": [{"tool": "card_delete", "arguments": {"target": "Schemas:Nope"}}]}}),
    );
    assert!(
        said.contains("steps[0]（card_delete）は通りません"),
        "{said}"
    );
    // No steps and nothing to open: nothing to try.
    let said = refused(&core, json!({"value": {}}));
    assert!(said.contains("stepsFrom）に 1 手以上"), "{said}");
}

#[test]
fn a_batch_opened_with_no_steps_takes_them_by_its_id() {
    let core = core();
    let opened = ok(&core, json!({"value": {"open": true}}));
    let token = opened["batch"].as_str().unwrap().to_string();
    assert_eq!(opened["staged"], 0, "{opened}");
    assert_eq!(opened["steps"], json!([]), "{opened}");
    let added = ok(&core, json!({"target": token, "value": {"steps": steps()}}));
    assert_eq!(added["staged"], 3, "{added}");
    let ran = ok(&core, json!({"target": token, "execute": true}));
    assert_eq!(ran["executed"], 3, "{ran}");
}

#[test]
fn opening_is_not_running() {
    let core = core();
    let said = refused(
        &core,
        json!({"value": {"steps": steps(), "open": true}, "execute": true}),
    );
    assert!(said.contains("open は execute: true と一緒には"), "{said}");
}

#[test]
fn steps_given_with_execute_are_run_at_once() {
    let core = core();
    let ran = ok(&core, json!({"value": {"steps": steps()}, "execute": true}));
    assert_eq!(ran["executed"], 3, "{ran}");
    assert!(ran.get("batch").is_none(), "{ran}");
    assert!(body(&core, "Free:Memo").contains("Schemas:Never"));
    // A step that does not go: nothing written.
    let said = refused(
        &core,
        json!({"value": {"steps": [
            {"tool": "card_update", "arguments": {"target": "Free:Memo", "value": {"body": "書かない"}}},
            {"tool": "card_delete", "arguments": {"target": "Schemas:Nope"}},
        ]}, "execute": true}),
    );
    assert!(
        said.contains("steps[1]（card_delete）で失敗しました"),
        "{said}"
    );
    assert!(body(&core, "Free:Memo").contains("Schemas:Never"));
}

#[test]
fn what_a_call_mixes_is_refused() {
    let core = core();
    let tried = ok(&core, json!({"value": {"steps": steps(), "open": true}}));
    let token = tried["batch"].as_str().unwrap();
    let said = refused(
        &core,
        json!({"target": token, "value": {"steps": steps()}, "execute": true}),
    );
    assert!(
        said.contains("execute: true で流すのは、積んだ手だけです"),
        "{said}"
    );
    let said = refused(&core, json!({"value": {"remove": 0}}));
    assert!(said.contains("target にバッチの id"), "{said}");
    let said = refused(
        &core,
        json!({"target": token, "value": {"remove": 0, "discard": true}}),
    );
    assert!(said.contains("1 回に 1 つだけ"), "{said}");
    let said = refused(&core, json!({"target": token, "branch": "refunds"}));
    assert!(said.contains("1 度だけ名指します"), "{said}");
    let said = refused(
        &core,
        json!({"value": {"steps": [{"tool": "card_update", "arguments": {"target": "Free:Memo", "branch": "refunds"}}]}}),
    );
    assert!(said.contains("手の引数に branch は書きません"), "{said}");
    // Taken out and let go of.
    let removed = ok(&core, json!({"target": token, "value": {"remove": 2}}));
    assert_eq!(removed["staged"], 2, "{removed}");
    assert!(removed.get("warnings").is_none(), "{removed}");
    let gone = ok(&core, json!({"target": token, "value": {"discard": true}}));
    assert_eq!(gone["discarded"], token);
}

#[test]
fn what_a_flow_lacks_is_said_as_it_stands_when_the_steps_are_done() {
    // A begin one step left without a way on and a later step joined is not said; what a
    // flow lacks still is, once, of the whole batch.
    let core = core();
    let steps = json!([
        {"tool": "card_create", "arguments": {"value": {
            "Charts/Shop:Job": {"kind": "flow"},
            "Charts/Shop:Job.open": {"kind": "begin", "trigger": "manual"},
            "Charts/Shop:Job.done": {"kind": "end", "outcome": "success"}
        }}},
        {"tool": "card_update", "arguments": {"target": "Charts/Shop:Job.open",
            "value": {"connections": {"add": [{"line": "next", "to": "done"}]}}}},
        {"tool": "card_create", "arguments": {"value": {
            "Charts/Shop:Left": {"kind": "flow"},
            "Charts/Shop:Left.open": {"kind": "begin", "trigger": "manual"}
        }}},
        {"tool": "card_update", "arguments": {"target": "Charts/Shop:Left", "value": {"label": "残る"}}},
    ]);
    let tried = ok(&core, json!({"value": {"steps": steps}}));
    let ran = ok(&core, json!({"value": {"steps": steps}, "execute": true}));
    for said in [&tried, &ran] {
        let warnings: Vec<&str> = said["warnings"]
            .as_array()
            .unwrap_or_else(|| panic!("{said}"))
            .iter()
            .filter_map(Value::as_str)
            .collect();
        assert!(
            warnings.iter().all(|w| !w.contains("Charts/Shop:Job")),
            "{said}"
        );
        let left: Vec<&&str> = warnings
            .iter()
            .filter(|w| w.contains("Charts/Shop:Left"))
            .collect();
        assert_eq!(left.len(), 1, "{said}");
        assert!(!left[0].starts_with("step "), "{said}");
    }
    for row in tried["steps"].as_array().unwrap() {
        assert!(row.get("warnings").is_none(), "{tried}");
    }
    // What a flow lacked before the batch began, and no step said: not the batch's to say.
    let again = ok(
        &core,
        json!({"value": {"steps": [
            {"tool": "card_update", "arguments": {"target": "Free:Memo", "value": {"body": "メモ"}}}
        ]}, "execute": true}),
    );
    assert!(again.get("warnings").is_none(), "{again}");
}
