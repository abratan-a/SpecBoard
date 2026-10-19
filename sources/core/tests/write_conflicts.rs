//! A card's rev: `card_read` says each card's rev (of the card a field, a row or a node is in),
//! `card_create` and `card_update` the rev of what they wrote and a batch run every card's it wrote
//! (`revs`); `card_update`'s and `card_delete`'s value give it back to be refused, nothing written,
//! when another wrote the card in between — said with who did, and when. A call (a batch's steps all)
//! is held to the tree before it, the rev is made of the card's document alone (where its box sits and
//! how it looks are not in it, nor the history), and the window's writes do not look at it.

use crate::common;

use common::{call, key_for_every_project, ok, refused, script_store};
use serde_json::{Value, json};
use specboard_core::{Core, mcp};

const PROJECT: &str = "見本";

fn core() -> Core {
    Core::open(script_store()).unwrap()
}

/// A tool called through MCP with the access key `key` (its value): whether it was refused, and
/// its text.
fn call_as(core: &Core, key: &str, tool: &str, mut arguments: Value) -> (bool, String) {
    arguments["project"] = PROJECT.into();
    let message = json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": tool, "arguments": arguments}});
    let headers = json!({ "authorization": format!("Bearer {key}") });
    let reply: Value = serde_json::from_str(
        &core.call(&json!({"op": "mcp", "message": message, "headers": headers}).to_string()),
    )
    .unwrap();
    let result = &reply["result"]["body"]["result"];
    let text: Vec<&str> = result["content"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|c| c["text"].as_str())
        .collect();
    (
        result["isError"].as_bool().unwrap_or(false),
        text.join("\n"),
    )
}

/// A second access key that writes every project, named `name`: its value.
fn other_key(core: &Core, name: &str) -> String {
    // The tests' own key first, so that it is made before (`common::headers`).
    ok(core, "project_read", json!({}));
    key_for_every_project(core, name).1
}

/// The rev `card_read` says of `target` (in `branch`).
fn rev(core: &Core, target: &str, branch: Option<&str>) -> String {
    let mut arguments = json!({"target": target});
    if let Some(branch) = branch {
        arguments["branch"] = branch.into();
    }
    let read = ok(core, "card_read", arguments);
    let rev = read["rev"]
        .as_str()
        .unwrap_or_else(|| panic!("rev がない: {read}"));
    assert_eq!(rev.len(), 12, "{rev}");
    assert!(rev.chars().all(|c| c.is_ascii_hexdigit()), "{rev}");
    rev.to_string()
}

fn read_text(core: &Core, target: &str, key: &str) -> String {
    ok(core, "card_read", json!({"target": target}))[key]
        .as_str()
        .unwrap_or_default()
        .to_string()
}

#[test]
fn a_card_two_keys_read_is_written_by_the_first_and_refused_to_the_second() {
    let core = core();
    let second = other_key(&core, "二つめのキー");
    let read_by_first = rev(&core, "Schemas:Customer", None);
    let read_by_second = read_by_first.clone();
    // The first writes with what it read: through, the answer saying the rev now.
    let wrote = ok(
        &core,
        "card_update",
        json!({"target": "Schemas:Customer", "value": {"label": "顧客（直した）", "rev": read_by_first}}),
    );
    let now = wrote["rev"].as_str().unwrap().to_string();
    assert_ne!(now, read_by_first);
    assert_eq!(now, rev(&core, "Schemas:Customer", None));
    // The second, with what it read before: refused, nothing written, saying who wrote it last.
    let (failed, said) = call_as(
        &core,
        &second,
        "card_update",
        json!({"target": "Schemas:Customer", "value": {"description": "二つめが書く", "rev": read_by_second}}),
    );
    assert!(failed, "{said}");
    assert!(
        said.contains(&format!(
            "Schemas:Customer は読んだあと（rev {read_by_second}）に変わっています（今は rev {now}。最後に変えたのは MCP・{}、",
            common::KEY
        )),
        "{said}"
    );
    assert!(said.contains("読み直してから書きます"), "{said}");
    assert_ne!(
        read_text(&core, "Schemas:Customer", "description"),
        "二つめが書く"
    );
    // Read again, it goes through.
    let (failed, said) = call_as(
        &core,
        &second,
        "card_update",
        json!({"target": "Schemas:Customer", "value": {"description": "二つめが書く", "rev": now}}),
    );
    assert!(!failed, "{said}");
    assert_eq!(
        read_text(&core, "Schemas:Customer", "description"),
        "二つめが書く"
    );
    // Written by the second: the first is told the second's key wrote it.
    let said = refused(
        &core,
        "card_update",
        json!({"target": "Schemas:Customer", "value": {"label": "顧客", "rev": now}}),
    );
    assert!(
        said.contains("最後に変えたのは MCP・二つめのキー、"),
        "{said}"
    );
}

#[test]
fn writing_without_a_rev_goes_through_as_before() {
    let core = core();
    let second = other_key(&core, "二つめのキー");
    ok(
        &core,
        "card_update",
        json!({"target": "Schemas:Customer", "value": {"label": "一つめ"}}),
    );
    let (failed, said) = call_as(
        &core,
        &second,
        "card_update",
        json!({"target": "Schemas:Customer", "value": {"label": "二つめ"}}),
    );
    assert!(!failed, "{said}");
    assert_eq!(read_text(&core, "Schemas:Customer", "label"), "二つめ");
}

#[test]
fn a_field_a_row_and_a_card_inside_a_container_are_read_with_their_card_s_rev() {
    let core = core();
    let order = rev(&core, "Schemas/Ordering:Order", None);
    assert_eq!(rev(&core, "Schemas/Ordering:Order.total", None), order);
    let spec = rev(&core, "Docs:Spec", None);
    assert_eq!(rev(&core, "Docs:Spec.intro", None), spec);
    assert_eq!(rev(&core, "Docs:Spec.intro.body", None), spec);
    // A flow's node is a card of its own: its own rev, not the flow's.
    let flow = rev(&core, "Charts/Shop:Checkout", None);
    let node = rev(&core, "Charts/Shop:Checkout.check", None);
    assert_ne!(flow, node);
    // Several read at once: each its own.
    let read = ok(
        &core,
        "card_read",
        json!({"target": ["Schemas/Ordering:Order", "Docs:Spec.intro"]}),
    );
    assert_eq!(read["cards"][0]["rev"], order.as_str());
    assert_eq!(read["cards"][1]["rev"], spec.as_str());
    // A field written with its card's rev: through, and the card's rev changes.
    let wrote = ok(
        &core,
        "card_update",
        json!({"target": "Schemas/Ordering:Order.total", "value": {"description": "税抜き", "rev": order}}),
    );
    let after = wrote["rev"].as_str().unwrap();
    assert_ne!(after, order);
    assert_eq!(after, rev(&core, "Schemas/Ordering:Order", None));
    // Another field of it, with the card's rev of before: refused, the rev being the card's.
    refused(
        &core,
        "card_update",
        json!({"target": "Schemas/Ordering:Order.status", "value": {"description": "状態", "rev": order}}),
    );
    // The node written: its own rev changes, the flow's does not.
    ok(
        &core,
        "card_update",
        json!({"target": "Charts/Shop:Checkout.check", "value": {"label": "確かめる", "rev": node}}),
    );
    assert_ne!(rev(&core, "Charts/Shop:Checkout.check", None), node);
    assert_eq!(rev(&core, "Charts/Shop:Checkout", None), flow);
}

#[test]
fn where_a_card_sits_its_size_and_how_it_looks_leave_its_rev_as_it_was() {
    let core = core();
    let customer = rev(&core, "Schemas:Customer", None);
    let moved = ok(
        &core,
        "card_update",
        json!({"target": "Schemas:Customer", "value": {"topLeft": {"x": 40, "y": 30}, "rev": customer}}),
    );
    assert_eq!(moved["rev"], customer.as_str());
    let memo = rev(&core, "Free:Memo", None);
    let sized = ok(
        &core,
        "card_update",
        json!({"target": "Free:Memo", "value": {"size": {"w": 12, "h": 8}, "rev": memo}}),
    );
    assert_eq!(sized["rev"], memo.as_str());
    let spec = rev(&core, "Docs:Spec", None);
    ok(
        &core,
        "card_update",
        json!({"target": "Docs:Spec", "value": {"view": "document"}}),
    );
    assert_eq!(rev(&core, "Docs:Spec", None), spec);
    assert_eq!(rev(&core, "Schemas:Customer", None), customer);
    assert_eq!(rev(&core, "Free:Memo", None), memo);
    // What it says changes it.
    ok(
        &core,
        "card_update",
        json!({"target": "Free:Memo", "value": {"body": "書き直した"}}),
    );
    assert_ne!(rev(&core, "Free:Memo", None), memo);
}

#[test]
fn a_rev_is_the_same_once_the_store_is_opened_again_and_its_history_cut() {
    let store = script_store();
    let (before, after_write) = {
        let core = Core::open(&store).unwrap();
        let before = rev(&core, "Schemas:Customer", None);
        let wrote = ok(
            &core,
            "card_update",
            json!({"target": "Schemas:Customer", "value": {"label": "顧客（直した）"}}),
        );
        core.close();
        (before, wrote["rev"].as_str().unwrap().to_string())
    };
    // The history cut away whole.
    {
        let conn = rusqlite::Connection::open(&store).unwrap();
        conn.execute("DELETE FROM commit_rows", []).unwrap();
        conn.execute("DELETE FROM commit_lines", []).unwrap();
        conn.execute("DELETE FROM commits", []).unwrap();
    }
    let core = Core::open(&store).unwrap();
    assert_eq!(rev(&core, "Schemas:Customer", None), after_write);
    // Refused all the same, the history saying nothing of who wrote it.
    let said = refused(
        &core,
        "card_update",
        json!({"target": "Schemas:Customer", "value": {"label": "顧客", "rev": before}}),
    );
    assert!(
        said.contains(&format!(
            "Schemas:Customer は読んだあと（rev {before}）に変わっています（今は rev {after_write}）。読み直してから書きます"
        )),
        "{said}"
    );
    assert!(!said.contains("最後に変えたのは"), "{said}");
}

#[test]
fn card_delete_with_a_rev_read_before_another_wrote_is_refused() {
    let core = core();
    let memo = rev(&core, "Free:Memo", None);
    ok(
        &core,
        "card_update",
        json!({"target": "Free:Memo", "value": {"body": "書き直した"}}),
    );
    let said = refused(
        &core,
        "card_delete",
        json!({"target": "Free:Memo", "value": {"rev": memo}}),
    );
    assert!(said.contains("Free:Memo は読んだあと"), "{said}");
    // By path, with what it is now: gone.
    let now = rev(&core, "Free:Memo", None);
    ok(
        &core,
        "card_delete",
        json!({"value": {"Free:Memo": {"rev": now}}}),
    );
    let (failed, _) = call(
        &core,
        "card_read",
        json!({"project": PROJECT, "target": "Free:Memo"}),
    );
    assert!(failed);
    // A rev is all its value takes.
    let said = refused(
        &core,
        "card_delete",
        json!({"target": "Schemas:Customer", "value": {"label": "x"}}),
    );
    assert!(said.contains("書けるのは rev"), "{said}");
}

#[test]
fn a_call_s_targets_are_held_to_the_tree_before_it() {
    let core = core();
    let order = rev(&core, "Schemas/Ordering:Order", None);
    // Two targets of one card, by path, each with the rev read: the second is not refused for
    // what the first wrote.
    let wrote = ok(
        &core,
        "card_update",
        json!({"value": {
            "Schemas/Ordering:Order": {"label": "注文（直した）", "rev": order},
            "Schemas/Ordering:Order.total": {"description": "税抜き", "rev": order},
        }}),
    );
    let after = rev(&core, "Schemas/Ordering:Order", None);
    assert_ne!(after, order);
    let results = wrote["results"].as_array().unwrap();
    assert_eq!(results.len(), 2, "{wrote}");
    for row in results {
        assert_eq!(row["result"]["rev"], after.as_str(), "{wrote}");
    }
    // Targets in a list share the value, its rev too.
    ok(
        &core,
        "card_update",
        json!({"target": ["Schemas/Ordering:Order.total", "Schemas/Ordering:Order.status"], "value": {"description": "書いた", "rev": after}}),
    );
    // One of several not as it was read: nothing of the call is written.
    let customer = rev(&core, "Schemas:Customer", None);
    let said = refused(
        &core,
        "card_update",
        json!({"value": {
            "Schemas:Customer": {"label": "顧客（直した）", "rev": customer},
            "Schemas/Ordering:Order": {"label": "古い", "rev": order},
        }}),
    );
    assert!(
        said.contains("Schemas/Ordering:Order は読んだあと"),
        "{said}"
    );
    assert_eq!(rev(&core, "Schemas:Customer", None), customer);
    // A path the call renames to was not read.
    let before = rev(&core, "Schemas:Customer", None);
    let said = refused(
        &core,
        "card_update",
        json!({"value": {
            "Schemas:Customer": {"name": "Client"},
            "Schemas:Client": {"label": "客", "rev": customer},
        }}),
    );
    assert!(
        said.contains("Schemas:Client はこの呼び出しの前にはありません"),
        "{said}"
    );
    // Nothing of the call written: not renamed.
    assert_eq!(rev(&core, "Schemas:Customer", None), before);
}

#[test]
fn a_batch_holds_its_steps_revs_to_the_tree_before_they_run() {
    let core = core();
    let customer = rev(&core, "Schemas:Customer", None);
    let order = rev(&core, "Schemas/Ordering:Order", None);
    // Run at once: the second step's rev is what was read, not what the first wrote.
    let ran = ok(
        &core,
        "batch",
        json!({"execute": true, "value": {"steps": [
            {"tool": "card_update", "arguments": {"target": "Schemas:Customer", "value": {"label": "顧客 1", "rev": customer}}},
            {"tool": "card_update", "arguments": {"target": "Schemas:Customer", "value": {"description": "二手め", "rev": customer}}},
            {"tool": "card_create", "arguments": {"target": "Free:Plan", "value": {"kind": "note", "body": "作った"}}},
        ]}}),
    );
    assert_eq!(ran["executed"], 3);
    let revs = ran["revs"].as_object().unwrap();
    assert_eq!(
        revs["Schemas:Customer"],
        rev(&core, "Schemas:Customer", None).as_str()
    );
    assert_eq!(revs["Free:Plan"], rev(&core, "Free:Plan", None).as_str());
    assert!(!revs.contains_key("Schemas/Ordering:Order"), "{ran}");
    // Made by an earlier step, or renamed to by one: refused, nothing written.
    let said = refused(
        &core,
        "batch",
        json!({"execute": true, "value": {"steps": [
            {"tool": "card_create", "arguments": {"target": "Free:Later", "value": {"kind": "note"}}},
            {"tool": "card_update", "arguments": {"target": "Free:Later", "value": {"body": "x", "rev": order}}},
        ]}}),
    );
    assert!(
        said.contains("Free:Later はこの呼び出しの前にはありません"),
        "{said}"
    );
    assert!(said.contains("何も書いていません"), "{said}");
    let (failed, _) = call(
        &core,
        "card_read",
        json!({"project": PROJECT, "target": "Free:Later"}),
    );
    assert!(failed);
    // One step's rev not as it was read: the batch writes nothing.
    let said = refused(
        &core,
        "batch",
        json!({"execute": true, "value": {"steps": [
            {"tool": "card_update", "arguments": {"target": "Schemas/Ordering:Order", "value": {"label": "注文 1", "rev": order}}},
            {"tool": "card_update", "arguments": {"target": "Schemas:Customer", "value": {"label": "古い", "rev": customer}}},
        ]}}),
    );
    assert!(
        said.contains("steps[1]（card_update）で失敗しました"),
        "{said}"
    );
    assert!(said.contains("Schemas:Customer は読んだあと"), "{said}");
    assert_eq!(rev(&core, "Schemas/Ordering:Order", None), order);
}

#[test]
fn a_batch_tried_holds_its_revs_too() {
    let core = core();
    let customer = rev(&core, "Schemas:Customer", None);
    let opened = ok(
        &core,
        "batch",
        json!({"value": {"open": true, "steps": [
            {"tool": "card_update", "arguments": {"target": "Schemas:Customer", "value": {"label": "顧客 1", "rev": customer}}},
        ]}}),
    );
    let token = opened["batch"].as_str().unwrap().to_string();
    // Another writes the card while the batch waits (the batch holds the tree from the window, not
    // from MCP): the run is refused, the batch kept.
    let second = other_key(&core, "二つめのキー");
    let (failed, said) = call_as(
        &core,
        &second,
        "card_update",
        json!({"target": "Schemas:Customer", "value": {"label": "二つめ"}}),
    );
    assert!(!failed, "{said}");
    let said = refused(&core, "batch", json!({"target": token, "execute": true}));
    assert!(
        said.contains("最後に変えたのは MCP・二つめのキー、"),
        "{said}"
    );
    assert!(said.contains("バッチはそのまま残っています"), "{said}");
    // A step added to it is tried with the others: refused, not added.
    let said = refused(
        &core,
        "batch",
        json!({"target": token, "value": {"steps": [
            {"tool": "card_update", "arguments": {"target": "Free:Memo", "value": {"body": "x"}}},
        ]}}),
    );
    assert!(said.contains("Schemas:Customer は読んだあと"), "{said}");
    let gone = ok(
        &core,
        "batch",
        json!({"target": token, "value": {"discard": true}}),
    );
    assert_eq!(gone["steps"], 1, "{gone}");
}

#[test]
fn an_append_sent_again_with_its_rev_is_refused_saying_this_key_wrote_it() {
    let core = core();
    let memo = rev(&core, "Free:Memo", None);
    let body = read_text(&core, "Free:Memo", "body");
    let send =
        json!({"target": "Free:Memo", "value": {"body": {"append": "- 足した"}, "rev": memo}});
    let first = ok(&core, "card_update", send.clone());
    // Answered short, with the rev now.
    assert!(first["body"].get("lineCount").is_some(), "{first}");
    let now = first["rev"].as_str().unwrap().to_string();
    assert_ne!(now, memo);
    // Sent again, its answer lost on the way: refused, the first said to have been this key's.
    let said = refused(&core, "card_update", send);
    assert!(
        said.contains(&format!(
            "（今は rev {now}。最後に変えたのは このアクセスキー（MCP・{}）、",
            common::KEY
        )),
        "{said}"
    );
    assert_eq!(
        read_text(&core, "Free:Memo", "body"),
        format!("{body}\n- 足した")
    );
}

#[test]
fn a_branch_reads_a_card_of_main_at_main_s_rev_and_writes_its_own() {
    let core = core();
    let customer = rev(&core, "Schemas:Customer", None);
    ok(
        &core,
        "branch_create",
        json!({"target": "naming", "value": {}}),
    );
    // Not written in the branch: as main has it — moved there, too.
    assert_eq!(rev(&core, "Schemas:Customer", Some("naming")), customer);
    ok(
        &core,
        "card_update",
        json!({"branch": "naming", "target": "Schemas:Customer", "value": {"topLeft": {"x": 50, "y": 40}}}),
    );
    assert_eq!(rev(&core, "Schemas:Customer", Some("naming")), customer);
    // Its tags written in the branch and back as they were: the same document, the same rev.
    let tags = ok(&core, "card_read", json!({"target": "Schemas:Customer"}))["tags"].clone();
    ok(
        &core,
        "card_update",
        json!({"branch": "naming", "target": "Schemas:Customer", "value": {"tags": ["仮"]}}),
    );
    assert_ne!(rev(&core, "Schemas:Customer", Some("naming")), customer);
    let back = if tags.is_null() { json!([]) } else { tags };
    ok(
        &core,
        "card_update",
        json!({"branch": "naming", "target": "Schemas:Customer", "value": {"tags": back}}),
    );
    assert_eq!(rev(&core, "Schemas:Customer", Some("naming")), customer);
    // main changes it since: the branch sees it as it was made, and its rev so.
    ok(
        &core,
        "card_update",
        json!({"target": "Schemas:Customer", "value": {"label": "main で直した"}}),
    );
    let on_main = rev(&core, "Schemas:Customer", None);
    assert_ne!(on_main, customer);
    assert_eq!(rev(&core, "Schemas:Customer", Some("naming")), customer);
    // Written in the branch with what it read there: through, main's as it was.
    let wrote = ok(
        &core,
        "card_update",
        json!({"branch": "naming", "target": "Schemas:Customer", "value": {"description": "branch で書く", "rev": customer}}),
    );
    let in_branch = wrote["rev"].as_str().unwrap().to_string();
    assert_eq!(rev(&core, "Schemas:Customer", Some("naming")), in_branch);
    assert_eq!(rev(&core, "Schemas:Customer", None), on_main);
    // main's rev given in the branch: refused, the branch's own writer said.
    let said = refused(
        &core,
        "card_update",
        json!({"branch": "naming", "target": "Schemas:Customer", "value": {"label": "x", "rev": on_main}}),
    );
    assert!(
        said.contains(&format!(
            "（今は rev {in_branch}。最後に変えたのは このアクセスキー"
        )),
        "{said}"
    );
    // The board's cards have theirs too, a goal's card inside it its own.
    let goal = rev(&core, "Goals:Ship", Some("_project_board_"));
    let task = rev(&core, "Goals:Ship.Design", Some("_project_board_"));
    assert_ne!(goal, task);
}

#[test]
fn card_create_answers_with_the_rev_of_each_card_it_made() {
    let core = core();
    let made = ok(
        &core,
        "card_create",
        json!({"value": {
            "Free:Plan": {"kind": "note", "body": "作った"},
            "Schemas:Shipment": {"kind": "entity", "fields": [{"name": "id", "type": "uuid"}]},
        }}),
    );
    let created = made["created"].as_array().unwrap();
    assert_eq!(created.len(), 2, "{made}");
    for card in created {
        let path = card["path"].as_str().unwrap();
        assert_eq!(card["rev"], rev(&core, path, None).as_str(), "{made}");
    }
    // A field made in a card: the card's rev once it is there.
    let field = ok(
        &core,
        "card_create",
        json!({"target": "Schemas:Shipment.carrier", "value": {"type": "string"}}),
    );
    assert_eq!(
        field["rev"],
        rev(&core, "Schemas:Shipment", None).as_str(),
        "{field}"
    );
}

#[test]
fn the_window_writes_without_looking_at_a_rev() {
    let core = core();
    let customer = rev(&core, "Schemas:Customer", None);
    ok(
        &core,
        "card_update",
        json!({"target": "Schemas:Customer", "value": {"label": "MCP で直した"}}),
    );
    let reply = common::op(
        &core,
        json!({"op": "tool", "name": "card_update", "arguments": {
            "project": PROJECT, "target": "Schemas:Customer",
            "value": {"label": "画面で直した", "rev": customer}}}),
    );
    assert!(reply.get("error").is_none(), "{reply}");
    // Nor says one: what it shows is as it was.
    assert!(reply["result"]["json"].get("rev").is_none(), "{reply}");
    assert_eq!(
        read_text(&core, "Schemas:Customer", "label"),
        "画面で直した"
    );
    // Written by the window: MCP is told so.
    let said = refused(
        &core,
        "card_update",
        json!({"target": "Schemas:Customer", "value": {"label": "x", "rev": customer}}),
    );
    assert!(said.contains("最後に変えたのは 画面、"), "{said}");
}

#[test]
fn a_rev_given_wrong_is_refused() {
    let core = core();
    let said = refused(
        &core,
        "card_update",
        json!({"target": "Schemas:Customer", "value": {"label": "x", "rev": 12}}),
    );
    assert!(said.contains("rev は読んだときのカードの rev"), "{said}");
    let said = refused(
        &core,
        "card_update",
        json!({"target": "Schemas:Customer", "value": {"label": "x", "rev": "000000000000"}}),
    );
    assert!(said.contains("Schemas:Customer は読んだあと"), "{said}");
    // card_create takes none: what it makes was not read.
    refused(
        &core,
        "card_create",
        json!({"target": "Free:Plan", "value": {"kind": "note", "rev": "000000000000"}}),
    );
}

/// How a rev is written is said once, in its own topic; the tools that take one point there.
#[test]
fn the_guide_says_once_when_a_rev_is_given() {
    let Ok(mcp::Output::Json(index)) = mcp::guide(None) else {
        panic!("guide の一覧がありません");
    };
    let mut said: Vec<String> = Vec::new();
    for family in ["tools", "kinds", "keys", "guides"] {
        for about in index[family].as_object().unwrap().keys() {
            let text = mcp::guide(Some(about)).unwrap().text();
            if text.contains("送った中身が入っていれば") {
                said.push(about.clone());
            }
        }
    }
    assert_eq!(said, ["rev"]);
    let Ok(mcp::Output::Json(update)) = mcp::guide(Some("card_update")) else {
        panic!();
    };
    assert!(
        update["value"]["card"]["rev"]
            .as_str()
            .unwrap()
            .contains("about: rev"),
        "{update}"
    );
}
