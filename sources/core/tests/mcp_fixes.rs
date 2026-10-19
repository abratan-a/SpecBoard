//! What MCP says: the protocol's envelope, the guide and the tools' words, what a write refuses or
//! tells, and what it counts.

use crate::common;

use common::{call, headers, ok, op, refused, script_store};
use serde_json::{Value, json};
use specboard_core::Core;

const PROJECT: &str = "見本";

fn core() -> Core {
    Core::open(script_store()).unwrap()
}

/// A message as it came, answered by the core: the JSON-RPC body.
fn sent(core: &Core, message: Value) -> Value {
    let reply: Value = serde_json::from_str(
        &core.call(&json!({"op": "mcp", "message": message, "headers": headers(core)}).to_string()),
    )
    .unwrap();
    reply["result"]["body"].clone()
}

// MARK: - The envelope

#[test]
fn a_batch_and_what_is_not_json_rpc_are_turned_away() {
    let core = core();
    let batch = sent(
        &core,
        json!([{"jsonrpc": "2.0", "id": 1, "method": "tools/list"}]),
    );
    assert_eq!(batch["error"]["code"], -32600, "{batch}");
    assert!(
        batch["error"]["message"]
            .as_str()
            .unwrap()
            .contains("まとめ送り"),
        "{batch}"
    );
    let bare = sent(&core, json!({"id": 2, "method": "tools/list"}));
    assert_eq!(bare["error"]["code"], -32600, "{bare}");
    assert_eq!(bare["id"], 2, "{bare}");
    let text = sent(
        &core,
        json!({"jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": {"name": "project_list", "arguments": "x"}}),
    );
    assert_eq!(text["error"]["code"], -32602, "{text}");
    assert!(
        text["error"]["message"]
            .as_str()
            .unwrap()
            .contains("オブジェクト"),
        "{text}"
    );
}

/// A tool of before is said gone by the name it was called by (`get_plan`), not by the project's
/// (`get_project`).
#[test]
fn a_tool_of_before_is_said_gone_by_its_own_name() {
    let core = core();
    for (gone, instead) in [
        ("get_plan", "project_read"),
        ("list_plans", "project_list"),
        ("plan_save", "export_execute"),
    ] {
        let reply = sent(
            &core,
            json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": gone, "arguments": {}}}),
        );
        let message = reply["error"]["message"].as_str().unwrap_or_default();
        assert!(
            message.starts_with(&format!("{gone} はありません")) && message.contains(instead),
            "{reply}"
        );
    }
}

#[test]
fn initialize_agrees_to_a_version_it_knows_and_says_the_apps() {
    let core = core();
    let hello = |version: &str| {
        sent(
            &core,
            json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": version, "capabilities": {}, "clientInfo": {"name": "t", "version": "0"}}}),
        )["result"]
            .clone()
    };
    assert_eq!(hello("2025-06-18")["protocolVersion"], "2025-06-18");
    assert_eq!(hello("2024-11-05")["protocolVersion"], "2024-11-05");
    assert_eq!(hello("2099-01-01")["protocolVersion"], "2025-11-25");
    let version =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../VERSION")).unwrap();
    assert_eq!(hello("2025-06-18")["serverInfo"]["version"], version.trim());
}

// MARK: - The tools' words

#[test]
fn the_guide_tells_of_itself_an_mr_by_its_number_and_a_branch_by_its_name() {
    let core = core();
    let (failed, text) = call(&core, "guide", json!({"about": "guide"}));
    assert!(!failed, "{text}");
    assert!(text.contains("\"tool\":\"guide\""), "{text}");
    // A number is an MR's number, not a type refused.
    let missing = refused(&core, "mr_read", json!({"target": 99}));
    assert!(!missing.contains("string で指定"), "{missing}");
    let named = refused(&core, "branch_create", json!({"target": "feat/x"}));
    assert!(named.contains("branch の名前に"), "{named}");
}

#[test]
fn section_list_refuses_a_key_it_does_not_take_and_the_guide_shows_the_keys_flat() {
    let core = core();
    // The paging keys sit directly in value; nesting them under a label would be read as nothing.
    let nested = refused(
        &core,
        "section_list",
        json!({"value": {"list": {"after": "Docs", "limit": 1}}}),
    );
    assert!(nested.contains("list"), "{nested}");
    let page = ok(&core, "section_list", json!({"value": {"limit": 1}}));
    assert_eq!(page["sections"].as_array().unwrap().len(), 1, "{page}");
    let (failed, guide) = call(&core, "guide", json!({"about": "section_list"}));
    assert!(!failed, "{guide}");
    let guide: Value = serde_json::from_str(&guide).unwrap();
    assert!(guide["value"].get("list").is_none(), "{guide}");
    assert!(guide["value"].get("limit").is_some(), "{guide}");
}

// MARK: - A merge's take

#[test]
fn a_take_of_what_does_not_conflict_is_said() {
    let core = core();
    ok(&core, "branch_create", json!({"target": "work"}));
    ok(
        &core,
        "card_update",
        json!({"branch": "work", "target": "Schemas:Customer", "value": {"description": "branch"}}),
    );
    ok(
        &core,
        "card_update",
        json!({"target": "Schemas:Customer", "value": {"description": "main"}}),
    );
    let said = refused(
        &core,
        "branch_merge",
        json!({"target": "work", "value": {"take": {"Schemas:Nope": {"description": "source"}}}}),
    );
    assert!(
        said.contains("Schemas:Nope の description は衝突にありません"),
        "{said}"
    );
    assert!(said.contains("Schemas:Customer の description"), "{said}");
    // What does conflict, chosen, goes in.
    ok(
        &core,
        "branch_merge",
        json!({"target": "work", "value": {"take": {"Schemas:Customer": {"description": "source"}}}}),
    );
    let merged = ok(&core, "card_read", json!({"target": "Schemas:Customer"}));
    assert_eq!(merged["description"], "branch", "{merged}");
}

// MARK: - Where a card is written

#[test]
fn a_file_a_card_is_left_out_of_is_not_where_it_is_written() {
    let core = core();
    let file = |path: &str| {
        let read = ok(
            &core,
            "card_read",
            json!({"target": path, "value": {"include": ["exports"]}}),
        );
        read["writtenTo"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|f| Some(format!("{}/{}", f["export"].as_str()?, f["file"].as_str()?)))
            .collect::<Vec<_>>()
    };
    let before = file("Schemas:Customer");
    let Some(shared) = before.first().cloned() else {
        panic!("Customer は書き出されていません");
    };
    let (dir, path) = shared.split_once('/').unwrap();
    // The same file for Order, in another format: the export writes one of them there.
    let project: String = rusqlite::Connection::open(core.place())
        .unwrap()
        .query_row("SELECT id FROM projects WHERE name = ?", [PROJECT], |r| {
            r.get(0)
        })
        .unwrap();
    common::op(
        &core,
        json!({"op": "mcp_writes_set", "project": project, "item": "export_update", "state": "allow"}),
    );
    ok(
        &core,
        "export_update",
        json!({"target": dir, "value": {"entries": {"add": [{"from": "Schemas/Ordering:Order", "to": path, "format": "zod"}]}}}),
    );
    let (_, problems) = call(
        &core,
        "export_read",
        json!({"targetdir": "[default]", "project": PROJECT}),
    );
    assert!(problems.contains("両方を書こうとしています"), "{problems}");
    let both = [file("Schemas:Customer"), file("Schemas/Ordering:Order")];
    assert_eq!(
        both.iter().filter(|f| f.contains(&shared)).count(),
        1,
        "{both:?}"
    );
}

// MARK: - A flow's node, the evacuation

#[test]
fn a_node_whose_next_is_itself_is_said() {
    let core = core();
    ok(
        &core,
        "card_create",
        json!({"target": "Charts:Loop", "value": {"kind": "flow"}}),
    );
    let made = ok(
        &core,
        "card_create",
        json!({"target": "Charts:Loop.start", "value": {"kind": "begin", "trigger": "manual", "connections": [{"line": "next", "to": "start"}]}}),
    );
    assert!(
        made["warnings"]
            .to_string()
            .contains("start の next が自分に戻っています"),
        "{made}"
    );
}

#[test]
fn nothing_is_made_in_or_moved_into_the_evacuation() {
    let core = core();
    let mut tree = ok(&core, "project_read", json!({}))["projectTree"].clone();
    let kept: Vec<Value> = tree["main_sections"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|s| s["name"] != "Free")
        .cloned()
        .collect();
    tree["main_sections"] = Value::Array(kept);
    ok(
        &core,
        "project_update",
        json!({"value": {"project_tree": tree}}),
    );
    let made = refused(
        &core,
        "card_create",
        json!({"target": "_evacuation_:New", "value": {"kind": "note", "body": "x"}}),
    );
    assert!(made.contains("退避のセクション"), "{made}");
    let moved = refused(
        &core,
        "card_update",
        json!({"target": "Schemas:Customer", "value": {"section": "_evacuation_/Free"}}),
    );
    assert!(moved.contains("退避のセクション"), "{moved}");
}

// MARK: - The rules

#[test]
fn a_rule_of_a_known_kind_says_what_it_must_and_nothing_it_may_not() {
    let core = core();
    let field = |rules: Value| json!({"target": "Schemas:Customer", "value": {"rules": rules}});
    let guessed = refused(
        &core,
        "card_update",
        field(json!([{"kind": "uniqueBy", "fields": ["id"]}])),
    );
    assert!(
        guessed.contains("rule「uniqueBy」に fields は書けません"),
        "{guessed}"
    );
    let short = refused(&core, "card_update", field(json!([{"kind": "pattern"}])));
    assert!(
        short.contains("rule「pattern」には regex が要ります"),
        "{short}"
    );
    let order = refused(
        &core,
        "card_update",
        field(json!([{"kind": "sortedBy", "order": "up"}])),
    );
    assert!(order.contains("asc / desc"), "{order}");
    // As it should be, and a kind it does not know: kept as written.
    let rules = json!([
        {"kind": "uniqueBy", "field": "id", "keys": ["id"], "note": "一意"},
        {"kind": "notYetKnown", "anything": 1}
    ]);
    ok(&core, "card_update", field(rules.clone()));
    let read = ok(&core, "card_read", json!({"target": "Schemas:Customer"}));
    assert_eq!(read["rules"], rules, "{read}");
    // A field's own, written with the field.
    let in_field = refused(
        &core,
        "card_update",
        json!({"target": "Schemas:Customer.name", "value": {"rules": [{"kind": "gteRef"}]}}),
    );
    assert!(in_field.contains("target が要ります"), "{in_field}");
}

// MARK: - What a write tells

#[test]
fn a_write_tells_what_it_did_not_what_was_so_already() {
    let core = core();
    let made = ok(
        &core,
        "card_create",
        json!({"target": "Docs:Memo", "value": {"kind": "freedoc", "body": "[[Schemas:Nowhere]]"}}),
    );
    assert!(
        made["warnings"].to_string().contains("Schemas:Nowhere"),
        "{made}"
    );
    // Its label changed: the path it could not resolve before is not said again.
    let relabelled = ok(
        &core,
        "card_update",
        json!({"target": "Docs:Memo", "value": {"label": "メモ"}}),
    );
    assert_eq!(relabelled.get("warnings"), None, "{relabelled}");
    // A path written anew that does not resolve is.
    let rewritten = ok(
        &core,
        "card_update",
        json!({"target": "Docs:Memo", "value": {"body": "[[Schemas:Nowhere]] [[Schemas:Elsewhere]]"}}),
    );
    let warned = rewritten["warnings"].to_string();
    assert!(warned.contains("Schemas:Elsewhere"), "{rewritten}");
    assert!(!warned.contains("Schemas:Nowhere"), "{rewritten}");
    // Both, several at once: the same.
    let several = ok(
        &core,
        "card_update",
        json!({"value": {"Docs:Memo": {"label": "メモ2"}, "Schemas:Customer": {"label": "客"}}}),
    );
    assert!(!several.to_string().contains("Nowhere"), "{several}");
}

#[test]
fn a_branch_let_go_says_who_still_works_in_it() {
    let core = core();
    ok(&core, "branch_create", json!({"target": "feat"}));
    ok(
        &core,
        "card_create",
        json!({"branch": "_project_board_", "target": "Goals:Work", "value": {"kind": "goal", "workBranch": "feat"}}),
    );
    let gone = ok(&core, "branch_delete", json!({"target": "feat"}));
    let warned = gone["warnings"].to_string();
    assert!(warned.contains("Goals:Work"), "{gone}");
    assert!(warned.contains("feat"), "{gone}");
}

#[test]
fn a_card_deleted_counts_every_reference_it_leaves() {
    let core = core();
    ok(
        &core,
        "card_create",
        json!({"target": "Docs:Memo", "value": {"kind": "freedoc", "body": "[[Schemas:Customer]]"}}),
    );
    let gone = ok(&core, "card_delete", json!({"target": "Schemas:Customer"}));
    let warnings = gone["warnings"].as_array().unwrap();
    assert!(
        warnings
            .iter()
            .any(|w| w.as_str().unwrap().contains("Docs:Memo")),
        "{gone}"
    );
    assert_eq!(gone["danglingRefs"], warnings.len(), "{gone}");
}

// MARK: - Words

#[test]
fn the_project_says_the_kinds_and_lines_by_their_names_now() {
    let core = core();
    let read = ok(&core, "project_read", json!({}));
    let counts = read["counts"].as_object().unwrap();
    for gone in ["action", "custom_goal"] {
        assert!(!counts.contains_key(gone), "{read}");
    }
    let drawn = ok(
        &core,
        "section_read",
        json!({"target": "Schemas", "value": {"as": "diagram"}}),
    );
    assert!(!drawn["kinds"].to_string().contains("lookup"), "{drawn}");
    assert!(drawn.get("direction").is_none(), "{drawn}");
    let rooted = ok(
        &core,
        "card_read",
        json!({"target": "Schemas:Customer", "value": {"as": "diagram", "kinds": ["points"]}}),
    );
    assert_eq!(rooted["kinds"], json!(["points"]), "{rooted}");
    assert_eq!(rooted["direction"], "out", "{rooted}");
}

// MARK: - What a branch says it changed

#[test]
fn a_branch_says_each_thing_it_changed_and_that_it_went_into_main() {
    let core = core();
    ok(&core, "branch_create", json!({"target": "work"}));
    // Its label alone is a change: what a merge would take in.
    ok(
        &core,
        "card_update",
        json!({"branch": "work", "target": "Schemas:Customer", "value": {"label": "得意先"}}),
    );
    let listed = ok(&core, "branch_list", json!({}));
    let row = listed["branches"]
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["branch"] == "work")
        .unwrap()
        .clone();
    assert_eq!(row["changes"], 1, "{listed}");
    // A document's rows, a row at a time by where it is.
    let spec = ok(&core, "card_read", json!({"target": "Docs:Spec"}));
    let first = spec["rows"][0]["path"].as_str().unwrap().to_string();
    let name = first.rsplit('.').next().unwrap().to_string();
    ok(
        &core,
        "card_update",
        json!({"branch": "work", "target": first, "value": {"heading": "書き換えた見出し"}}),
    );
    ok(
        &core,
        "card_create",
        json!({"branch": "work", "target": "Docs:Spec.added", "value": {"kind": "text", "body": "足した"}}),
    );
    let read = ok(&core, "branch_read", json!({"target": "work"}));
    let said = read["changed"].to_string();
    assert!(said.contains("表示名"), "{read}");
    assert!(said.contains("得意先"), "{read}");
    assert!(said.contains(&format!("行を変える {name}")), "{read}");
    assert!(said.contains("行を足す added"), "{read}");
    assert!(!said.contains("行・鍵・規則などを変える"), "{read}");
    assert_eq!(read.get("merged"), None, "{read}");
    // Merged into main and kept, nothing written since: said so.
    ok(
        &core,
        "branch_merge",
        json!({"target": "work", "value": {"keep": true}}),
    );
    let read = ok(&core, "branch_read", json!({"target": "work"}));
    assert!(read["merged"].is_string(), "{read}");
    let listed = ok(&core, "branch_list", json!({}));
    let row = listed["branches"]
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["branch"] == "work")
        .unwrap()
        .clone();
    assert_eq!(row["changes"], 0, "{listed}");
    assert!(row["merged"].is_string(), "{listed}");
    // Written again: no longer.
    ok(
        &core,
        "card_update",
        json!({"branch": "work", "target": "Schemas:Customer", "value": {"label": "顧客"}}),
    );
    let read = ok(&core, "branch_read", json!({"target": "work"}));
    assert_eq!(read.get("merged"), None, "{read}");
}

#[test]
fn a_section_under_the_root_is_not_renamed_in_a_branch() {
    let core = core();
    ok(&core, "branch_create", json!({"target": "work"}));
    let said = refused(
        &core,
        "section_update",
        json!({"branch": "work", "target": "Docs", "value": {"name": "Docs2"}}),
    );
    assert!(said.contains("branch では変えません"), "{said}");
    let main = ok(&core, "section_list", json!({}));
    assert!(main.to_string().contains("\"Docs\""), "{main}");
    // Its description is the branch's to write; its name, main's.
    ok(
        &core,
        "section_update",
        json!({"branch": "work", "target": "Docs", "value": {"description": "branch で書いた"}}),
    );
    ok(
        &core,
        "section_update",
        json!({"target": "Docs", "value": {"name": "Docs2"}}),
    );
}

// MARK: - A document read whole

#[test]
fn a_document_is_read_as_one_markdown_with_its_paths_as_written() {
    let core = core();
    ok(
        &core,
        "card_create",
        json!({"target": "Docs:Guide", "value": {"kind": "formatdoc", "label": "手引き", "rows": [
            {"name": "intro", "kind": "item", "heading": "はじめに", "rows": [
                {"name": "body", "kind": "text", "body": "[[Schemas:Customer]] を見る"}
            ]},
            {"name": "pic", "kind": "diagram", "target": "Schemas:Customer"}
        ]}}),
    );
    let said = ok(
        &core,
        "card_read",
        json!({"target": "Docs:Guide", "value": {"as": "markdown"}}),
    );
    // It says the card's rev too.
    let read = ok(&core, "card_read", json!({"target": "Docs:Guide"}));
    assert_eq!(said["branch"], "main", "{said}");
    assert_eq!(said["path"], "Docs:Guide", "{said}");
    assert_eq!(said["rev"], read["rev"], "{said}");
    let text = said["markdown"].as_str().unwrap();
    assert!(text.contains("# 手引き"), "{text}");
    assert!(text.contains("はじめに"), "{text}");
    assert!(text.contains("[[Schemas:Customer]] を見る"), "{text}");
    assert!(text.contains("![[Schemas:Customer]]"), "{text}");
    // A note: its body. Two at once: one apiece.
    ok(
        &core,
        "card_create",
        json!({"target": "Docs:Memo", "value": {"kind": "note", "body": "メモの本文"}}),
    );
    let both = ok(
        &core,
        "card_read",
        json!({"target": ["Docs:Guide", "Docs:Memo"], "value": {"as": "markdown"}}),
    );
    let memo = &both["cards"][1];
    assert_eq!(memo["path"], "Docs:Memo", "{both}");
    assert!(memo["rev"].is_string(), "{both}");
    assert_eq!(memo["markdown"], "メモの本文", "{both}");
    // Not a document: refused.
    let said = refused(
        &core,
        "card_read",
        json!({"target": "Schemas:Customer", "value": {"as": "markdown"}}),
    );
    assert!(said.contains("as: markdown で読めるのは"), "{said}");
}

#[test]
fn a_documents_markdown_says_which_card_each_table_is_and_names_its_items() {
    let core = core();
    ok(
        &core,
        "card_create",
        json!({"target": "Docs:Decisions", "value": {"kind": "formatdoc", "label": "決めたこと", "rows": [
            {"name": "adr_single_binary", "kind": "item", "heading": "1 つのバイナリで配る", "rows": [
                {"name": "why", "kind": "item", "heading": "理由", "rows": [
                    {"name": "body", "kind": "text", "body": "[[Docs:Decisions.adr_single_binary]] のとおり"}
                ]}
            ]},
            {"name": "order", "kind": "card", "target": "Schemas/Ordering:Order"},
            {"name": "refund", "kind": "card", "target": "Charts/Shop:Refund"},
            {"name": "overview", "kind": "card", "target": "Docs:Overview"}
        ]}}),
    );
    let text = ok(
        &core,
        "card_read",
        json!({"target": "Docs:Decisions", "value": {"as": "markdown"}}),
    );
    let text = text["markdown"].as_str().unwrap();
    // Each item's path under its heading, the nested one's too.
    assert!(
        text.contains("## 1 つのバイナリで配る\n<!-- row adr_single_binary -->\n"),
        "{text}"
    );
    assert!(
        text.contains("### 理由\n<!-- row adr_single_binary.why -->\n"),
        "{text}"
    );
    // The card above its table; a card of no table, the path to it, not struck through.
    assert!(
        text.contains("[[Schemas/Ordering:Order]]\n\n| 名前 | 型 | 必須 | 説明 |"),
        "{text}"
    );
    assert!(text.contains("\n\n[[Charts/Shop:Refund]]\n\n"), "{text}");
    assert!(text.ends_with("\n\n[[Docs:Overview]]"), "{text}");
    assert!(!text.contains("~~"), "{text}");
    // Written out: the card's name, a link when it is written out in the same export, and the items
    // named alike.
    let written = ok(
        &core,
        "export_read",
        json!({"targetdir": "[default]", "target": "docs",
            "value": {"files": ["notes/Decisions.md"]}}),
    );
    let written = written["files"][0]["text"].as_str().unwrap();
    assert!(
        written.contains("<!-- row adr_single_binary -->"),
        "{written}"
    );
    assert!(
        written.contains("<!-- row adr_single_binary.why -->"),
        "{written}"
    );
    assert!(written.contains("\n\n注文\n\n| 名前 |"), "{written}");
    assert!(written.contains("\n\n返金\n\n"), "{written}");
    assert!(
        written.contains("\n\n[Overview](Overview.md)\n"),
        "{written}"
    );
    assert!(!written.contains("~~"), "{written}");
}

// MARK: - The sizes

#[test]
fn the_answers_are_packed_and_say_the_project_once() {
    let core = core();
    let reply = sent(
        &core,
        json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": "card_update", "arguments": {
            "project": PROJECT, "target": ["Schemas:Customer", "Schemas:Rule"], "value": {"description": "二つに"}
        }}}),
    );
    let text = reply["result"]["content"][0]["text"].as_str().unwrap();
    assert!(!text.contains("\n"), "{text}");
    let said: Value = serde_json::from_str(text).unwrap();
    assert_eq!(said["project"], PROJECT, "{said}");
    for row in said["results"].as_array().unwrap() {
        assert!(row["result"].get("project").is_none(), "{said}");
        assert!(row["result"].get("branch").is_none(), "{said}");
    }
    let read = ok(
        &core,
        "card_read",
        json!({"target": ["Schemas:Customer", "Schemas:Rule"]}),
    );
    assert_eq!(read["project"], PROJECT, "{read}");
    assert!(
        read["cards"]
            .as_array()
            .unwrap()
            .iter()
            .all(|c| c.get("project").is_none()),
        "{read}"
    );
    // The arguments every tool means the same by are said once, in the instructions; what value
    // is written with is each tool's own, said in the list.
    let listed = sent(
        &core,
        json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
    );
    for tool in listed["result"]["tools"].as_array().unwrap() {
        let properties = &tool["inputSchema"]["properties"];
        for key in ["project", "branch", "request"] {
            assert!(
                properties[key].get("description").is_none(),
                "{}: {key}",
                tool["name"]
            );
        }
        if let Some(value) = properties.get("value") {
            assert!(value["description"].is_string(), "{}", tool["name"]);
        }
    }
    let instructions = specboard_core::mcp::catalog()["instructions"]
        .as_str()
        .unwrap();
    for key in [
        "- project:",
        "- target:",
        "- value:",
        "- branch:",
        "- request:",
    ] {
        assert!(instructions.contains(key), "{key}");
    }
}

#[test]
fn a_projects_name_is_its_own_in_the_store() {
    // Two projects of one name, and a call by it would reach either.
    let core = Core::open(script_store()).unwrap();
    let make = |name: &str| call(&core, "project_create", json!({"project": name}));
    let (refused, said) = make("dup-test");
    assert!(!refused, "{said}");
    for again in ["dup-test", " DUP-test "] {
        let (refused, said) = make(again);
        assert!(refused, "{again}: {said}");
        assert!(
            said.contains("プロジェクト「dup-test」はもうあります（slug dup-test）"),
            "{said}"
        );
    }
    // Renamed to one another has: by MCP and by the window alike; to its own name, in another case.
    let (refused, said) = call(
        &core,
        "project_update",
        json!({"project": PROJECT, "value": {"name": "Dup-Test"}}),
    );
    assert!(refused, "{said}");
    assert!(said.contains("はもうあります"), "{said}");
    let id: String = rusqlite::Connection::open(core.place())
        .unwrap()
        .query_row("SELECT id FROM projects WHERE name = ?", [PROJECT], |r| {
            r.get(0)
        })
        .unwrap();
    let renamed = op(
        &core,
        json!({"op": "project_rename", "project": id, "name": "dup-test"}),
    );
    assert!(
        renamed["error"].to_string().contains("はもうあります"),
        "{renamed}"
    );
    let (refused, said) = make("Other");
    assert!(!refused, "{said}");
    let (refused, said) = call(
        &core,
        "project_update",
        json!({"project": "Other", "value": {"name": "other"}}),
    );
    assert!(!refused, "{said}");

    // Two of one name made before: a call by it is refused, saying their slugs; by a slug, it goes.
    rusqlite::Connection::open(core.place())
        .unwrap()
        .execute(
            "UPDATE projects SET name = '重なり' WHERE slug IN ('dup-test', 'other')",
            [],
        )
        .unwrap();
    let (refused, said) = call(&core, "project_read", json!({"project": "重なり"}));
    assert!(refused, "{said}");
    assert!(
        said.contains("同じ名前「重なり」のプロジェクトが 2 つあります（slug ")
            && said.contains("slug dup-test・id ")
            && said.contains("slug other・id ")
            && said.contains("）。名前と違う slug か id で呼びます"),
        "{said}"
    );
    for slug in ["dup-test", "other"] {
        let (refused, said) = call(&core, "project_read", json!({"project": slug}));
        assert!(!refused, "{slug}: {said}");
    }
    // A name two have that is one's slug too: refused all the same, for a call by the name means
    // either; the one whose slug it is goes by its id, the other by its slug.
    rusqlite::Connection::open(core.place())
        .unwrap()
        .execute(
            "UPDATE projects SET name = 'other' WHERE slug IN ('dup-test', 'other')",
            [],
        )
        .unwrap();
    let (refused, said) = call(&core, "project_read", json!({"project": "other"}));
    assert!(refused && said.contains("2 つあります"), "{said}");
    let (refused, said) = call(&core, "project_read", json!({"project": "OTHER"}));
    assert!(refused && said.contains("2 つあります"), "{said}");
    let other: String = rusqlite::Connection::open(core.place())
        .unwrap()
        .query_row("SELECT id FROM projects WHERE slug = 'other'", [], |r| {
            r.get(0)
        })
        .unwrap();
    for by in [other.as_str(), "dup-test"] {
        let (refused, said) = call(&core, "project_read", json!({"project": by}));
        assert!(!refused, "{by}: {said}");
    }
    // A write by the name is refused before it asks or writes anything.
    let (refused, said) = call(
        &core,
        "card_create",
        json!({"project": "other", "target": "Free:Stray", "value": {"kind": "note"}}),
    );
    assert!(refused && said.contains("2 つあります"), "{said}");
}
