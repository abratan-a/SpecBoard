//! A process node's steps one at a time, by their places (`Charts/Shop:Checkout.save.steps[1]`, the
//! text a line's `from` names a step by): read as one element of the node's `steps`, changed key by
//! key, moved, added at a place and let go of. A step has only its place, so a write that moves
//! steps renumbers what names them by it — the node's lines that leave a step follow it, and a path
//! written elsewhere does too — or a line would silently leave another step.

use crate::common;

use common::{checked, ok, refused, script_store};
use serde_json::{Value, json};
use specboard_core::Core;

const NODE: &str = "Charts/Shop:Checkout.save";

fn node(core: &Core) -> Value {
    ok(core, "card_read", json!({"target": NODE}))
}

/// The node's lines that leave a step: `(line, from)`, in their order.
fn step_lines(core: &Core) -> Vec<(String, String)> {
    node(core)["connections"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|l| l.get("from").is_some())
        .map(|l| {
            (
                l["line"].as_str().unwrap().to_string(),
                l["from"].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

fn pair(line: &str, from: &str) -> (String, String) {
    (line.to_string(), from.to_string())
}

#[test]
fn a_step_is_read_alone_as_the_node_lists_it() {
    let core = Core::open(script_store()).unwrap();
    let step = ok(
        &core,
        "card_read",
        json!({"target": format!("{NODE}.steps[1]")}),
    );
    assert_eq!(step["path"], format!("{NODE}.steps[1]"));
    assert_eq!(step["do"], "write");
    assert_eq!(step["description"], "記録に置く");
    assert_eq!(step["rev"], node(&core)["rev"]);
    // Past the last step: refused with how many there are.
    let beyond = refused(
        &core,
        "card_read",
        json!({"target": format!("{NODE}.steps[2]")}),
    );
    assert!(
        beyond.contains("2") && beyond.contains("steps[1]"),
        "{beyond}"
    );
}

#[test]
fn a_step_changed_changes_only_the_keys_written() {
    let core = Core::open(script_store()).unwrap();
    let before = node(&core);
    let changed = ok(
        &core,
        "card_update",
        json!({"target": format!("{NODE}.steps[1]"),
            "value": {"description": "Store the order in the Orders store"}}),
    );
    assert_eq!(changed["path"], format!("{NODE}.steps[1]"));
    assert_eq!(
        changed["description"],
        "Store the order in the Orders store"
    );
    let after = node(&core);
    assert_eq!(
        after["steps"],
        json!([{"do": "read"}, {"do": "write", "description": "Store the order in the Orders store"}])
    );
    assert_eq!(after["connections"], before["connections"]);
    // null takes a key away; a key a step does not take is refused by its form.
    ok(
        &core,
        "card_update",
        json!({"target": format!("{NODE}.steps[1]"), "value": {"description": null}}),
    );
    assert_eq!(node(&core)["steps"][1], json!({"do": "write"}));
    let stray = refused(
        &core,
        "card_update",
        json!({"target": format!("{NODE}.steps[1]"), "value": {"line": "writes"}}),
    );
    assert!(stray.contains("position"), "{stray}");
}

#[test]
fn a_step_moved_takes_its_lines_and_the_paths_that_name_it() {
    let core = Core::open(script_store()).unwrap();
    ok(
        &core,
        "card_create",
        json!({"target": "Free:Saving", "value": {"kind": "note",
            "body": format!("書くのは [[{NODE}.steps[1]]]")}}),
    );
    let moved = ok(
        &core,
        "card_update",
        json!({"target": format!("{NODE}.steps[1]"), "value": {"position": 0}}),
    );
    assert_eq!(moved["path"], format!("{NODE}.steps[0]"));
    assert_eq!(moved["do"], "write");
    assert_eq!(
        node(&core)["steps"],
        json!([{"do": "write", "description": "記録に置く"}, {"do": "read"}])
    );
    // The step passed moves the other way, its line with it.
    assert_eq!(
        step_lines(&core),
        [pair("reads", "steps[1]"), pair("writes", "steps[0]")]
    );
    let note = ok(&core, "card_read", json!({"target": "Free:Saving"}));
    assert_eq!(note["body"], format!("書くのは [[{NODE}.steps[0]]]"));
    // A path to a step resolves: the inspection does not say it broken.
    let broken = checked(&core, "main")
        .get("brokenPaths")
        .map(Value::to_string)
        .unwrap_or_default();
    assert!(!broken.contains("Free:Saving"), "{broken}");
}

#[test]
fn a_step_added_at_a_place_moves_the_later_ones_on() {
    let core = Core::open(script_store()).unwrap();
    let made = ok(
        &core,
        "card_create",
        json!({"target": format!("{NODE}.steps[1]"),
            "value": {"do": "compute", "description": "合計を確かめる"}}),
    );
    assert_eq!(made["path"], format!("{NODE}.steps[1]"));
    assert_eq!(made["do"], "compute");
    assert!(made["rev"].is_string(), "{made}");
    assert_eq!(
        node(&core)["steps"],
        json!([{"do": "read"}, {"do": "compute", "description": "合計を確かめる"},
            {"do": "write", "description": "記録に置く"}])
    );
    assert_eq!(
        step_lines(&core),
        [pair("reads", "steps[0]"), pair("writes", "steps[2]")]
    );
    // The place after the last adds at the end; a place past it is refused with the count.
    ok(
        &core,
        "card_create",
        json!({"target": format!("{NODE}.steps[3]"), "value": {"do": "compute"}}),
    );
    assert_eq!(node(&core)["steps"].as_array().unwrap().len(), 4);
    let beyond = refused(
        &core,
        "card_create",
        json!({"target": format!("{NODE}.steps[9]"), "value": {"do": "compute"}}),
    );
    assert!(beyond.contains("4"), "{beyond}");
}

#[test]
fn a_step_let_go_of_takes_its_lines_and_moves_the_later_ones_back() {
    let core = Core::open(script_store()).unwrap();
    let gone = ok(
        &core,
        "card_delete",
        json!({"target": format!("{NODE}.steps[0]")}),
    );
    assert_eq!(gone["deleted"], format!("{NODE}.steps[0]"));
    assert_eq!(gone["stepCount"], 1);
    assert_eq!(
        gone["linesRemoved"],
        json!([{"line": "reads", "from": "steps[0]", "to": "Schemas/Ordering:Order", "at": "total"}])
    );
    assert_eq!(
        node(&core)["steps"],
        json!([{"do": "write", "description": "記録に置く"}])
    );
    assert_eq!(step_lines(&core), [pair("writes", "steps[0]")]);
}

#[test]
fn a_path_to_a_step_let_go_of_is_said_broken_rather_than_moved_onto_the_next() {
    let core = Core::open(script_store()).unwrap();
    ok(
        &core,
        "card_create",
        json!({"target": "Free:Reading", "value": {"kind": "note",
            "body": format!("読むのは [[{NODE}.steps[0]]]")}}),
    );
    let gone = ok(
        &core,
        "card_delete",
        json!({"target": format!("{NODE}.steps[0]")}),
    );
    assert!(
        gone["warnings"].to_string().contains("Free:Reading"),
        "{gone}"
    );
    let note = ok(&core, "card_read", json!({"target": "Free:Reading"}));
    assert_ne!(note["body"], format!("読むのは [[{NODE}.steps[0]]]"));
}

#[test]
fn a_step_that_needs_its_line_moves_with_it_in_one_write() {
    // A sign step is held to have its key line: the node is checked once the line has followed.
    let core = Core::open(script_store()).unwrap();
    let signing = "Charts/Shop:Checkout.signing";
    ok(
        &core,
        "card_create",
        json!({"target": signing, "value": {"kind": "process",
        "steps": [{"do": "read"}, {"do": "sign"}],
        "connections": [
            {"line": "reads", "from": "steps[0]", "to": "Schemas/Ordering:Order", "at": "total"},
            {"line": "key", "from": "steps[1]", "to": "Schemas:Customer", "at": "id", "fields": {"use": "sign"}}
        ]}}),
    );
    ok(
        &core,
        "card_update",
        json!({"target": format!("{signing}.steps[1]"), "value": {"position": 0}}),
    );
    let read = ok(&core, "card_read", json!({"target": signing}));
    assert_eq!(read["steps"], json!([{"do": "sign"}, {"do": "read"}]));
    let froms: Vec<(Value, Value)> = read["connections"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| (l["line"].clone(), l["from"].clone()))
        .collect();
    assert_eq!(
        froms,
        [
            (json!("reads"), json!("steps[1]")),
            (json!("key"), json!("steps[0]"))
        ]
    );
}

#[test]
fn a_blank_step_added_is_refused_when_its_kind_needs_a_key_and_kept_when_it_only_lacks_a_line() {
    // The window keeps a refused row as a draft and a row only waiting for its line as saved.
    let core = Core::open(script_store()).unwrap();
    let said = refused(
        &core,
        "card_create",
        json!({"target": format!("{NODE}.steps[2]"), "value": {"do": "sign"}}),
    );
    assert!(said.contains("key"), "{said}");
    assert_eq!(node(&core)["steps"].as_array().unwrap().len(), 2);
    ok(
        &core,
        "card_create",
        json!({"target": format!("{NODE}.steps[2]"), "value": {"do": "write"}}),
    );
    assert_eq!(node(&core)["steps"].as_array().unwrap().len(), 3);
}
