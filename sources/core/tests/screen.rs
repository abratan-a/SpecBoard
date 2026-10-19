//! The window's own requests (`tools::screen`), over the script's project: what it reads by
//! id — the projects and branches, what a branch marks, the layout, the export directories
//! and writing rules, what a commit changed and what the history takes, where boxes sit
//! — and what it writes as the window, the MCP tools among them.

use crate::common;

use common::{call, script_store};
use serde_json::{Value, json};
use specboard_core::Core;
use std::collections::HashMap;
use std::path::PathBuf;

const PROJECT: &str = "見本";

struct Screen {
    core: Core,
    store: PathBuf,
    project: String,
}

impl Screen {
    fn new() -> Screen {
        let store = script_store();
        let project: String = rusqlite::Connection::open(&store)
            .unwrap()
            .query_row("SELECT id FROM projects WHERE name = ?", [PROJECT], |r| {
                r.get(0)
            })
            .unwrap();
        let core = Core::open(&store).unwrap();
        Screen {
            core,
            store,
            project,
        }
    }

    fn answer(&self, mut request: Value) -> Value {
        if request.get("project").is_none() {
            request["project"] = self.project.clone().into();
        }
        serde_json::from_str(&self.core.call(&request.to_string())).unwrap()
    }

    fn ok(&self, request: Value) -> Value {
        let answer = self.answer(request.clone());
        assert!(answer.get("error").is_none(), "{request}: {answer}");
        answer["result"].clone()
    }

    fn refused(&self, request: Value) -> String {
        let answer = self.answer(request.clone());
        answer["error"]
            .as_str()
            .unwrap_or_else(|| panic!("{request} が通った: {answer}"))
            .to_string()
    }

    fn mcp(&self, tool: &str, mut arguments: Value) -> Value {
        arguments["project"] = PROJECT.into();
        let (failed, text) = call(&self.core, tool, arguments.clone());
        assert!(!failed, "{tool} {arguments}: {text}");
        serde_json::from_str(&text).unwrap_or(Value::String(text))
    }

    fn mcp_refused(&self, tool: &str, mut arguments: Value) -> String {
        arguments["project"] = PROJECT.into();
        let (failed, text) = call(&self.core, tool, arguments.clone());
        assert!(failed, "{tool} {arguments} が通った: {text}");
        // What it said, out of the answer's `error`.
        serde_json::from_str::<Value>(&text)
            .ok()
            .and_then(|v| v["error"]["message"].as_str().map(str::to_string))
            .unwrap_or(text)
    }

    /// The window's tool call, as the tool answers it.
    fn tool(&self, name: &str, mut arguments: Value) -> Value {
        arguments["project"] = PROJECT.into();
        self.ok(json!({"op": "tool", "name": name, "arguments": arguments}))["json"].clone()
    }

    fn tool_refused(&self, name: &str, mut arguments: Value) -> String {
        arguments["project"] = PROJECT.into();
        self.refused(json!({"op": "tool", "name": name, "arguments": arguments}))
    }

    /// The project as a tree sees it, whole.
    fn detail(&self, branch: &str) -> Value {
        self.ok(json!({"op": "project_detail", "branch": branch}))
    }

    /// Card and section ids by name, as main has them.
    fn ids(&self) -> HashMap<String, String> {
        let detail = self.detail("");
        let mut out = HashMap::new();
        for key in ["schemas", "sections"] {
            for item in detail[key].as_array().unwrap() {
                out.insert(
                    item["name"].as_str().unwrap().to_string(),
                    item["id"].as_str().unwrap().to_string(),
                );
            }
        }
        out
    }

    fn card(&self, branch: &str, id: &str) -> Value {
        self.detail(branch)["schemas"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["id"] == id)
            .cloned()
            .unwrap_or(Value::Null)
    }

    fn branch_id(&self, name: &str) -> String {
        self.ok(json!({"op": "branches"}))["branches"]
            .as_array()
            .unwrap()
            .iter()
            .find(|b| b["name"] == name)
            .unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string()
    }

    /// The newest commit's number in a tree.
    fn last_seq(&self, branch: &str) -> i64 {
        self.ok(json!({"op": "commits", "branch": branch, "limit": 1}))["commits"][0]["seq"]
            .as_i64()
            .unwrap()
    }

    fn changes(&self, branch: &str, seq: i64) -> Value {
        self.ok(json!({"op": "commit_changes", "branch": branch, "seq": seq}))["changes"].clone()
    }
}

#[test]
fn the_projects_are_listed_and_renamed() {
    let s = Screen::new();
    let projects = s.ok(json!({"op": "projects"}))["projects"].clone();
    let one = &projects[0];
    assert_eq!(one["id"], s.project.as_str());
    assert_eq!(one["name"], PROJECT);
    assert_eq!(one["slug"], PROJECT);
    assert_eq!(one["tagGroups"], json!([]));
    // The sample's 25 cards but the 14 nodes of its flows; its sections with the root.
    assert_eq!(one["schemaCount"], 11);
    assert_eq!(one["sectionCount"], 12);
    assert_eq!(one["archivedAt"], Value::Null);
    assert_eq!(one["lockedAt"], Value::Null);
    assert_eq!(one["public"], false);
    for key in ["description", "createdAt", "updatedAt"] {
        assert!(one[key].is_string(), "{key}");
    }

    // Made public (by the access keys' requests, not these): the list says so.
    rusqlite::Connection::open(&s.store)
        .unwrap()
        .execute(
            "INSERT INTO project_public (project_id) VALUES (?)",
            [&s.project],
        )
        .unwrap();
    assert_eq!(
        s.ok(json!({"op": "projects"}))["projects"][0]["public"],
        true
    );

    // Renamed: the slug stays, and main's history says it.
    s.ok(json!({"op": "project_rename", "name": "  新しい名前 "}));
    let renamed = &s.ok(json!({"op": "projects"}))["projects"][0];
    assert_eq!(renamed["name"], "新しい名前");
    assert_eq!(renamed["slug"], PROJECT);
    let last = &s.ok(json!({"op": "commits", "limit": 1}))["commits"][0];
    assert_eq!(last["authorKind"], "screen");
    assert_eq!(
        last["lines"][0]["summary"],
        "プロジェクト名を「新しい名前」に変更"
    );
    assert_eq!(
        s.refused(json!({"op": "project_rename", "name": " "})),
        "プロジェクト名 を入力してください"
    );
    assert_eq!(
        s.refused(json!({"op": "project_rename", "name": "x".repeat(121)})),
        "プロジェクト名 は 120 文字以内です"
    );
    assert_eq!(
        s.refused(json!({"op": "project_rename", "project": "nope", "name": "x"})),
        "プロジェクトが見つかりません"
    );
    assert_eq!(
        serde_json::from_str::<Value>(&s.core.call(r#"{"op":"project_rename","name":"x"}"#))
            .unwrap()["error"],
        "project_rename には project（プロジェクトの id）が要ります"
    );

    // The slug, from the window: not held to MCP's item for it, asked each time by default.
    let said = s.tool("project_update", json!({"value": {"slug": "Shop Orders"}}));
    assert_eq!(said["slug"], "shop-orders");
    let listed = &s.ok(json!({"op": "projects"}))["projects"][0];
    assert_eq!(listed["slug"], "shop-orders");
    let last = &s.ok(json!({"op": "commits", "limit": 1}))["commits"][0];
    assert_eq!(last["authorKind"], "screen");
    assert_eq!(
        last["lines"][0]["summary"],
        "slug を「見本」から「shop-orders」に変更"
    );
    // The old slug names it no more (the helpers name it by 見本, its slug until now).
    assert_eq!(
        s.tool_refused("project_update", json!({"value": {"slug": "again"}})),
        "プロジェクト「見本」がありません。あるのは: 新しい名前"
    );
    // Archived, it is not changed.
    assert!(
        s.ok(json!({"op": "project_archive"}))
            .get("error")
            .is_none()
    );
    let said = s.refused(json!({"op": "tool", "name": "project_update",
        "arguments": {"project": s.project, "value": {"slug": "again"}}}));
    assert!(said.contains("アーカイブ"), "{said}");
}

#[test]
fn a_branch_marks_what_it_and_main_did_since() {
    let s = Screen::new();
    let branches = s.ok(json!({"op": "branches"}))["branches"].clone();
    assert_eq!(branches.as_array().unwrap().len(), 1);
    let refunds = &branches[0];
    assert_eq!(refunds["name"], "refunds");
    assert_eq!(refunds["projectId"], s.project.as_str());
    assert_eq!(refunds["sortOrder"], 0);
    assert_eq!(refunds["archivedAt"], Value::Null);
    assert!(refunds["createdAt"].is_string());

    s.mcp("branch_create", json!({"target": "work"}));
    let work = s.branch_id("work");
    // The branch changes one of main's cards and makes one and a section; main changes the
    // same card, and one the branch never wrote.
    s.mcp(
        "card_update",
        json!({"branch": "work", "target": "Schemas:Customer", "value": {"description": "branch で書いた"}}),
    );
    s.mcp(
        "card_create",
        json!({"branch": "work", "target": "Schemas:Fresh", "value": {"kind": "entity"}}),
    );
    s.mcp(
        "section_create",
        json!({"branch": "work", "target": "Schemas/Billing"}),
    );
    s.mcp(
        "card_update",
        json!({"target": "Schemas:Customer", "value": {"description": "main で書いた"}}),
    );
    s.mcp(
        "card_update",
        json!({"target": "Schemas:Status", "value": {"description": "main だけ"}}),
    );
    let ids = s.ids();
    let fresh = s.detail(&work)["schemas"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "Fresh")
        .unwrap()["id"]
        .clone();
    let billing = s.detail(&work)["sections"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "Billing")
        .unwrap()["id"]
        .clone();
    let marks = s.ok(json!({"op": "branch_marks", "branch": work}));
    assert_eq!(marks["changed"], json!([ids["Customer"]]));
    assert_eq!(marks["made"], json!([fresh]));
    let mut main_changed = vec![ids["Customer"].clone(), ids["Status"].clone()];
    main_changed.sort();
    assert_eq!(marks["mainChanged"], json!(main_changed));
    assert_eq!(marks["sectionsMade"], json!([billing]));
    assert_eq!(marks["sectionsChanged"], json!([]));
    // main changed Customer and Status since, and the branch was made after the script's own.
    assert_eq!(marks["mainMoved"], 2);

    let none = json!({"changed": [], "made": [], "removed": [], "mainChanged": [],
        "sectionsChanged": [], "sectionsMade": [], "sectionsRemoved": [],
        "gone": {"schemas": [], "ghosts": [], "inner": [], "sections": []}, "mainMoved": 0});
    assert_eq!(s.ok(json!({"op": "branch_marks"})), none);
    assert_eq!(
        s.ok(json!({"op": "branch_marks", "branch": "~board"})),
        none
    );
    assert_eq!(
        s.refused(json!({"op": "branch_marks", "branch": "nope"})),
        "branch「nope」が見つかりません"
    );
}

/// What `branch_read` lists of a branch, by its paths, as ids by the main of now.
fn listed(s: &Screen, branch: &str, key: &str) -> Vec<String> {
    let read = s.mcp("branch_read", json!({"target": branch}));
    read[key]
        .as_array()
        .into_iter()
        .flatten()
        .map(|row| row.get("card").unwrap_or(row).as_str().unwrap().to_string())
        .collect()
}

#[test]
fn a_branch_marks_what_branch_read_lists_and_what_it_removed_where_it_was() {
    let s = Screen::new();
    s.mcp("section_create", json!({"target": "Schemas/Old"}));
    s.mcp(
        "card_create",
        json!({"target": "Schemas/Old:Kept", "value": {"kind": "note"}}),
    );
    let ids = s.ids();
    let rule_in_main = s.card("", &ids["Rule"]);
    s.mcp("branch_create", json!({"target": "work"}));
    let work = s.branch_id("work");
    // Customer is written and written back: a copy no different from main's is not changed.
    let said = |text: &str| json!({"branch": "work", "target": "Schemas:Customer", "value": {"description": text}});
    let was = s.mcp("card_read", json!({"target": "Schemas:Customer"}))["description"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    s.mcp("card_update", said("branch で書いた"));
    s.mcp("card_update", said(&was));
    // Order moves to a section the branch makes, Voucher is made there; Rule and the section
    // Old (Kept first) are removed, and a node of Checkout.
    s.mcp(
        "section_create",
        json!({"branch": "work", "target": "Schemas/Billing"}),
    );
    s.mcp(
        "card_update",
        json!({"branch": "work", "target": "Schemas/Ordering:Order", "value": {"section": "Schemas/Billing"}}),
    );
    s.mcp(
        "card_create",
        json!({"branch": "work", "target": "Schemas/Billing:Voucher", "value": {"kind": "entity"}}),
    );
    s.mcp(
        "card_delete",
        json!({"branch": "work", "target": "Schemas:Rule"}),
    );
    s.mcp(
        "card_delete",
        json!({"branch": "work", "target": "Schemas/Old:Kept"}),
    );
    s.mcp(
        "section_delete",
        json!({"branch": "work", "target": "Schemas/Old"}),
    );
    s.mcp(
        "card_delete",
        json!({"branch": "work", "target": "Charts/Shop:Checkout.failed"}),
    );
    // main changes Order and Status, and makes a card.
    s.mcp(
        "card_update",
        json!({"target": "Schemas/Ordering:Order", "value": {"description": "main で書いた"}}),
    );
    s.mcp(
        "card_update",
        json!({"target": "Schemas:Status", "value": {"description": "main だけ"}}),
    );
    s.mcp(
        "card_create",
        json!({"target": "Schemas:Later", "value": {"kind": "note"}}),
    );

    let marks = s.ok(json!({"op": "branch_marks", "branch": work}));
    let mine = s.detail(&work);
    let id_in = |detail: &Value, key: &str, name: &str| -> String {
        detail[key]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["name"] == name)
            .unwrap_or_else(|| panic!("{name}"))["id"]
            .as_str()
            .unwrap()
            .to_string()
    };
    let checkout_failed = s.detail("")["inner"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "failed" && c["in"] == ids["Checkout"].as_str())
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let sorted = |mut list: Vec<String>| {
        list.sort();
        json!(list)
    };
    // What it changed is what branch_read lists as changed: the move, and what names Order by
    // its path or lost a line to the node removed; not Customer, written back as it was.
    let read = s.mcp("branch_read", json!({"target": "work"}));
    let order = read["changed"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["card"] == "Schemas/Billing:Order")
        .unwrap_or_else(|| panic!("{read}"));
    assert!(
        order["changes"]
            .as_array()
            .unwrap()
            .contains(&json!("セクション Schemas/Ordering → Schemas/Billing")),
        "{order}"
    );
    let changed: Vec<String> = marks["changed"]
        .as_array()
        .unwrap()
        .iter()
        .map(|id| id.as_str().unwrap().to_string())
        .collect();
    assert_eq!(changed.len(), listed(&s, "work", "changed").len(), "{read}");
    let check = s.detail("")["inner"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "check" && c["in"] == ids["Checkout"].as_str())
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    for id in [&ids["Order"], &ids["Spec"], &check] {
        assert!(changed.contains(id), "{id}: {marks}");
    }
    for id in [&ids["Customer"], &ids["Checkout"]] {
        assert!(!changed.contains(id), "{id}: {marks}");
    }
    assert_eq!(marks["made"], json!([id_in(&mine, "schemas", "Voucher")]));
    assert_eq!(
        marks["removed"],
        sorted(vec![
            ids["Rule"].clone(),
            ids["Kept"].clone(),
            checkout_failed.clone()
        ])
    );
    assert_eq!(listed(&s, "work", "removed").len(), 3, "{read}");
    assert_eq!(
        marks["sectionsMade"],
        json!([id_in(&mine, "sections", "Billing")])
    );
    assert_eq!(marks["sectionsRemoved"], json!([ids["Old"]]));
    // main changed Order (which the branch moved) and Status; Later is not in the branch.
    assert_eq!(
        marks["mainChanged"],
        sorted(vec![ids["Order"].clone(), ids["Status"].clone()])
    );
    assert_eq!(marks["mainMoved"], 3);
    let row = s.mcp("branch_list", json!({}))["branches"]
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["branch"] == "work")
        .unwrap()
        .clone();
    assert_eq!(row["mainMoved"], 3);

    // What it removed, as the screen reads cards, where it was: Rule as main reads it.
    let gone = &marks["gone"];
    let named = |key: &str| -> Vec<String> {
        let mut out: Vec<String> = gone[key]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["name"].as_str().unwrap().to_string())
            .collect();
        out.sort();
        out
    };
    assert_eq!(named("schemas"), ["Kept", "Rule"]);
    assert_eq!(named("inner"), ["failed"]);
    assert_eq!(named("sections"), ["Old"]);
    assert_eq!(named("ghosts"), Vec::<String>::new());
    let rule = gone["schemas"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "Rule")
        .unwrap();
    assert_eq!(*rule, rule_in_main);
    let kept = gone["schemas"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "Kept")
        .unwrap();
    assert_eq!(kept["sectionId"], ids["Old"].as_str());
    let failed = &gone["inner"][0];
    assert_eq!(failed["in"], ids["Checkout"].as_str());
    assert_eq!(failed["kind"], "end");
    // Nothing of it is in the branch's own reading.
    let names = |key: &str| -> Vec<String> {
        mine[key]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["id"].as_str().unwrap().to_string())
            .collect()
    };
    assert!(!names("schemas").contains(&ids["Rule"]));
    assert!(!names("sections").contains(&ids["Old"]));
    assert!(!names("inner").contains(&checkout_failed));
}

#[test]
fn a_commit_says_what_it_changed() {
    let s = Screen::new();
    s.mcp(
        "card_update",
        json!({"target": "Schemas:Customer", "value": {"description": "客の説明"}}),
    );
    assert_eq!(
        s.changes("", s.last_seq("")),
        json!([{"path": "Schemas:Customer", "change": "change", "section": false, "details": ["説明を変える"]}])
    );

    // Against the next commit's before, not the card now.
    s.mcp(
        "card_create",
        json!({"target": "Schemas/Ordering:Order.memo", "value": {"type": "string"}}),
    );
    let added = s.last_seq("");
    s.mcp(
        "card_create",
        json!({"target": "Schemas/Ordering:Order.note", "value": {"type": "string"}}),
    );
    assert_eq!(s.changes("", added)[0]["details"], json!(["足す memo"]));

    // Renamed, and the name taken again by a card made after: each read as it was.
    s.mcp(
        "card_update",
        json!({"target": "Schemas:Customer", "value": {"name": "Client"}}),
    );
    let renamed = s.last_seq("");
    s.mcp(
        "card_create",
        json!({"target": "Schemas:Customer", "value": {"kind": "entity"}}),
    );
    let made = s.last_seq("");
    let said = s.changes("", renamed);
    let client = said
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["path"] == "Schemas:Client")
        .unwrap();
    assert_eq!(client["change"], "change");
    assert_eq!(
        client["details"].as_array().unwrap()[0..2].to_vec(),
        vec![
            json!("Schemas:Customer → Schemas:Client"),
            json!("名前 Customer → Client")
        ]
    );
    assert_eq!(
        s.changes("", made),
        json!([{"path": "Schemas:Customer", "change": "add", "section": false, "details": []}])
    );

    // Sections first; a card let go.
    s.mcp("section_create", json!({"target": "Schemas/Billing"}));
    assert_eq!(
        s.changes("", s.last_seq(""))[0],
        json!({"path": "Schemas/Billing", "change": "add", "section": true, "details": []})
    );
    s.mcp("card_delete", json!({"target": "Free:Memo"}));
    assert!(s.changes("", s.last_seq("")).as_array().unwrap().contains(
        &json!({"path": "Free:Memo", "change": "remove", "section": false, "details": []})
    ));

    // In a branch: the first write of one of main's cards, against main's.
    s.mcp("branch_create", json!({"target": "work"}));
    let work = s.branch_id("work");
    s.mcp(
        "card_update",
        json!({"branch": "work", "target": "Schemas:Status", "value": {"description": "branch の説明"}}),
    );
    assert_eq!(
        s.changes(&work, s.last_seq(&work)),
        json!([{"path": "Schemas:Status", "change": "change", "section": false, "details": ["説明を変える"]}])
    );

    assert_eq!(
        s.refused(json!({"op": "commit_changes", "seq": 999})),
        "コミット 999 はありません"
    );
    assert_eq!(
        s.refused(json!({"op": "commit_changes"})),
        "commit_changes には seq（コミットの番号）が要ります"
    );
}

#[test]
fn what_the_history_takes_and_a_cut_would_let_go() {
    let s = Screen::new();
    for text in ["一", "二", "三"] {
        s.mcp(
            "card_update",
            json!({"target": "Schemas:Customer", "value": {"description": text}}),
        );
    }
    let size = s.ok(json!({"op": "project_size"}));
    assert!(size["content"].as_i64().unwrap() > 0);
    let trees = size["branches"].as_array().unwrap();
    assert_eq!(trees[0]["id"], Value::Null);
    assert_eq!(trees[0]["name"], "main");
    assert_eq!(trees[1]["name"], "refunds");
    assert_eq!(trees[1]["id"], s.branch_id("refunds").as_str());
    let main_history = trees[0]["history"].as_i64().unwrap();

    let last = s.last_seq("");
    let cost = s.ok(json!({"op": "cut_cost", "before": last}));
    assert_eq!(cost["commits"], last - 1);
    let bytes = cost["bytes"].as_i64().unwrap();
    assert!(bytes > 0);
    assert_eq!(s.ok(json!({"op": "cut", "before": last}))["cut"], last - 1);
    let after = s.ok(json!({"op": "project_size"}))["branches"][0]["history"]
        .as_i64()
        .unwrap();
    assert_eq!(after, main_history - bytes);
    assert_eq!(
        s.ok(json!({"op": "cut_cost", "before": last})),
        json!({"commits": 0, "bytes": 0})
    );
}

#[test]
fn boxes_are_put_where_the_window_says() {
    let s = Screen::new();
    let ids = s.ids();
    let (schemas, order, customer, status, memo) = (
        &ids["Schemas"],
        &ids["Order"],
        &ids["Customer"],
        &ids["Status"],
        &ids["Memo"],
    );
    assert_eq!(
        s.ok(json!({"op": "section_positions"})),
        json!({"positions": {}, "sectionGhosts": {}})
    );
    // Another section's card on this section's diagram: no history.
    let before = s.last_seq("");
    s.ok(json!({"op": "places", "boxes": [
        {"view": schemas, "item": order, "x": 3, "y": 4.4},
        {"view": schemas, "item": status, "x": 9, "y": 1},
    ]}));
    assert_eq!(s.last_seq(""), before);
    let positions = s.ok(json!({"op": "section_positions"}))["positions"].clone();
    assert_eq!(positions[schemas][order], json!({"x": 3, "y": 4}));
    // A section ghost on it, by the section it sums up: apart from the cards.
    let charts = &ids["Charts"];
    s.ok(json!({"op": "places", "boxes": [
        {"view": schemas, "kind": "section", "item": charts, "x": -14, "y": 2},
    ]}));
    let read = s.ok(json!({"op": "section_positions"}));
    assert_eq!(
        read["sectionGhosts"][schemas][charts],
        json!({"x": -14, "y": 2})
    );
    assert_eq!(read["positions"][schemas].get(charts.as_str()), None);
    let boxes = s.mcp(
        "section_read",
        json!({"target": "Schemas", "value": {"include": ["places"]}}),
    );
    assert_eq!(
        boxes["places"]["Charts"]["topLeft"],
        json!({"x": -14, "y": 2}),
        "{boxes}"
    );

    // A card on its own section's diagram, arranged: not pinned. A writing card keeps
    // its size.
    s.ok(json!({"op": "places", "boxes": [{"item": customer, "x": 40, "y": 2}]}));
    let card = s.card("", customer);
    assert_eq!(
        (card["posX"].clone(), card["posY"].clone()),
        (json!(40.0), json!(2.0))
    );
    assert_eq!(card["pinned"], false);
    s.mcp(
        "card_update",
        json!({"target": "Free:Memo", "value": {"size": {"w": 12, "h": 6}}}),
    );
    s.ok(json!({"op": "places", "boxes": [{"item": memo, "x": 5, "y": 6, "pinned": true}]}));
    let card = s.card("", memo);
    assert_eq!(card["pinned"], true);
    assert_eq!(card["posX"], json!(5.0));
    assert_eq!(card["size"], json!({"w": 12, "h": 6}));

    // A branch's over main's.
    s.mcp("branch_create", json!({"target": "work"}));
    let work = s.branch_id("work");
    s.ok(json!({"op": "places", "branch": work, "boxes": [{"view": schemas, "item": order, "x": 7, "y": 8}]}));
    let there = s.ok(json!({"op": "section_positions", "branch": work}))["positions"].clone();
    assert_eq!(there[schemas][order], json!({"x": 7, "y": 8}));
    assert_eq!(there[schemas][status], json!({"x": 9, "y": 1}));
    assert_eq!(
        s.ok(json!({"op": "section_positions"}))["positions"][schemas][order],
        json!({"x": 3, "y": 4})
    );

    assert_eq!(
        s.refused(json!({"op": "places"})),
        "places には boxes（置く箱の並び）が要ります"
    );
    assert_eq!(
        s.refused(
            json!({"op": "places", "boxes": [{"kind": "box", "item": order, "x": 0, "y": 0}]})
        ),
        "箱の kind は card か section です（box ではなく）"
    );
    assert_eq!(
        s.refused(json!({"op": "places", "boxes": [{"x": 0, "y": 0}]})),
        "箱には item（カード・セクションの id）が要ります"
    );
    assert_eq!(
        s.refused(json!({"op": "places", "boxes": [{"item": order, "x": 0}]})),
        format!("箱には x・y（マス）が要ります（{order}）")
    );
}

#[test]
fn a_card_with_no_place_is_given_one() {
    let s = Screen::new();
    let ids = s.ids();
    let customer = &ids["Customer"];
    rusqlite::Connection::open(&s.store)
        .unwrap()
        .execute(
            "DELETE FROM places WHERE branch_id = '' AND view = '' AND item = ?",
            [customer],
        )
        .unwrap();
    assert!(s.card("", customer).get("posX").is_none());
    s.ok(json!({"op": "place_unplaced"}));
    let card = s.card("", customer);
    assert!(
        card["posX"].is_number() && card["posY"].is_number(),
        "{card}"
    );
    assert_eq!(card["pinned"], false);
}

#[test]
fn the_layout_and_the_writing_rules() {
    let s = Screen::new();
    assert_eq!(
        s.ok(json!({"op": "layout"})),
        json!({"size": {"rows": 15, "width": 352}, "margin": 1, "docView": "document"})
    );
    s.tool(
        "project_update",
        json!({"value": {"layout": {"cardWidth": "auto", "cardRows": 12}}}),
    );
    assert_eq!(
        s.ok(json!({"op": "layout"}))["size"],
        json!({"rows": 12, "width": null})
    );

    assert_eq!(s.ok(json!({"op": "writing_rules"})), json!({"rules": {}}));
    // A person's to change under main's lock too.
    s.ok(json!({"op": "main_lock", "on": true}));
    assert_eq!(
        s.ok(
            json!({"op": "writing_rules_set", "place": "card.description", "text": "一文目で言う"})
        ),
        json!({"rules": {"card.description": "一文目で言う"}})
    );
    assert_eq!(
        s.ok(json!({"op": "writing_rules"})),
        json!({"rules": {"card.description": "一文目で言う"}})
    );
    let last = &s.ok(json!({"op": "commits", "limit": 1}))["commits"][0];
    assert_eq!(
        last["lines"][0]["summary"],
        "書き方の決まりを更新（1 か所）"
    );
    let read = s.mcp(
        "project_read",
        json!({"value": {"as": "rules", "place": "card.entity.description"}}),
    );
    assert_eq!(read["rule"], "一文目で言う");
    assert_eq!(read["from"], "project");

    let unknown = s.refused(json!({"op": "writing_rules_set", "place": "nope", "text": "x"}));
    assert!(
        unknown.starts_with("知らない書く場所です: nope。書ける場所は project.description / "),
        "{unknown}"
    );
    assert_eq!(
        s.refused(
            json!({"op": "writing_rules_set", "place": "note.body", "text": "あ".repeat(4001)})
        ),
        "note.body の決まりが長すぎます（4000 文字まで）"
    );
    // Empty: the app's again.
    assert_eq!(
        s.ok(json!({"op": "writing_rules_set", "place": "card.description", "text": " \n"})),
        json!({"rules": {}})
    );
    assert_eq!(s.ok(json!({"op": "writing_rules"})), json!({"rules": {}}));
}

#[test]
fn the_working_directories_are_read_for_the_window_and_set_on_screen_under_the_lock() {
    let s = Screen::new();
    // What the window puts over every tree: each working directory as set and read here.
    let read = s.ok(json!({"op": "workdir_detail"}));
    let names: Vec<&str> = read["dirs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["ts", "zod", "schema", "charts_2", "docs_2"]);
    let ts = read["dirs"][0].clone();
    assert_eq!(ts["description"], "");
    assert_eq!(ts["folder"], Value::Null);
    assert_eq!(ts["readType"], "git");
    assert_eq!(
        ts["section"],
        format!("dir:{}", ts["id"].as_str().unwrap()).as_str()
    );
    assert_eq!(read["sections"], json!([]));
    assert_eq!(read["schemas"], json!([]));
    // The same reading again: unchanged, nothing sent.
    let digest = read["digest"].as_str().unwrap().to_string();
    assert_eq!(
        s.ok(json!({"op": "workdir_detail", "since": digest})),
        json!({"digest": digest, "unchanged": true})
    );

    // Its folder, bound by the window as the section's (`section_update`'s `folder`).
    let folder = |s: &Screen| s.ok(json!({"op": "workdir_detail"}))["dirs"][0]["folder"].clone();
    s.tool(
        "section_update",
        json!({"target": "ts", "value": {"folder": "  ~/out/ts\u{3000}"}}),
    );
    assert_eq!(folder(&s), "~/out/ts");
    assert_eq!(
        s.mcp("section_read", json!({"target": "ts"}))["folder"],
        "~/out/ts"
    );
    assert_ne!(
        s.ok(json!({"op": "workdir_detail"}))["digest"],
        digest.as_str()
    );
    for (bad, said) in [
        ("out/ts", "フォルダは / か ~/ から書いてください"),
        ("/out/\nts", "フォルダに改行や制御文字は使えません"),
        ("/out/\u{200B}ts", "フォルダに改行や制御文字は使えません"),
    ] {
        let refused = s.refused(
            json!({"op": "tool", "name": "section_update", "arguments": {
                "project": PROJECT, "target": "ts", "value": {"folder": bad}
            }}),
        );
        assert!(refused.contains(said), "{bad}: {refused}");
    }
    s.tool(
        "section_update",
        json!({"target": "ts", "value": {"folder": null}}),
    );
    assert_eq!(folder(&s), Value::Null);
    s.tool(
        "section_update",
        json!({"target": "ts", "value": {"folder": "/tmp/ts"}}),
    );
    s.tool(
        "section_update",
        json!({"target": "ts", "value": {"folder": " "}}),
    );
    assert_eq!(folder(&s), Value::Null);

    // A folder read: its directories and files, as the tree's entries are said; where one was
    // moved on a map, in the tree's reading.
    let root = common::place("screen-dir");
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/a.rs"), "?").unwrap();
    s.tool(
        "section_update",
        json!({"target": "ts", "value": {"folder": root.to_string_lossy()}}),
    );
    let read = s.ok(json!({"op": "workdir_detail"}));
    assert_eq!(read["dirs"][0]["files"], 1, "{read}");
    assert_eq!(read["dirs"][0]["directories"], 1, "{read}");
    let dir = read["sections"][0].clone();
    assert_eq!(
        (&dir["name"], &dir["kind"], &dir["parentId"]),
        (&json!("src"), &json!("directory"), &ts["section"]),
        "{dir}"
    );
    let file = read["schemas"][0].clone();
    assert_eq!(
        (&file["name"], &file["kind"]),
        (&json!("a.rs"), &json!("file")),
        "{file}"
    );
    assert_eq!(file["sectionId"], dir["id"], "{file}");
    s.tool(
        "card_update",
        json!({"target": "ts/src:'a.rs'", "value": {"topLeft": {"x": 2, "y": 3}}}),
    );
    let detail = s.ok(json!({"op": "project_detail"}));
    assert_eq!(
        detail["filePlaces"][file["id"].as_str().unwrap()],
        json!({"x": 2, "y": 3, "pinned": true}),
        "{}",
        detail["filePlaces"]
    );
    assert!(
        !detail["schemas"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["kind"] == "file")
    );

    // Under main's lock: the project's setting, not main's — the window's and MCP's go through.
    s.ok(json!({"op": "main_lock", "on": true}));
    let set = s.tool(
        "section_update",
        json!({"target": "ts", "value": {"description": "TypeScript の型"}}),
    );
    assert_eq!(set["description"], "TypeScript の型", "{set}");
    let set = s.mcp(
        "section_update",
        json!({"target": "zod", "value": {"readType": "basic"}}),
    );
    assert_eq!(set["readType"], "basic", "{set}");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn the_window_runs_the_tools_as_itself() {
    let s = Screen::new();
    assert_eq!(s.tool("project_read", json!({}))["project"], PROJECT);
    s.tool(
        "card_update",
        json!({"target": "Schemas:Customer", "value": {"description": "画面で書いた"}}),
    );
    let last = &s.ok(json!({"op": "commits", "limit": 1}))["commits"][0];
    assert_eq!(last["authorKind"], "screen");
    assert_eq!(
        s.refused(json!({"op": "tool", "name": "project_read", "arguments": []})),
        "tool の arguments はオブジェクトです"
    );
    assert_eq!(
        s.refused(json!({"op": "tool"})),
        "tool には name（ツールの名前）が要ります"
    );
}

#[test]
fn a_documents_diagram_is_drawn_by_the_core_as_the_window_shows_it() {
    let s = Screen::new();
    let ids = s.ids();
    // A section, as the window draws it in a document: the head says what it was drawn
    // from; written out, only the mark and the rev — and the same rev.
    let drawn = s.ok(json!({"op": "embed_diagram", "path": "Schemas:", "section": ids["Free"]}));
    let shown = drawn["mermaid"].as_str().unwrap().to_string();
    assert!(shown.starts_with("%% specboard project="), "{shown}");
    assert!(shown.contains("classDiagram"), "{shown}");
    let written = s.ok(json!({"op": "embed_diagram", "path": "Schemas:", "section": ids["Free"], "exported": true}));
    let out = written["mermaid"].as_str().unwrap().to_string();
    assert!(out.starts_with("%% specboard rev="), "{out}");
    let rev = |text: &str| {
        text.split("rev=")
            .nth(1)
            .unwrap()
            .split_whitespace()
            .next()
            .unwrap()
            .to_string()
    };
    assert_eq!(rev(&shown), rev(&out));
    // A flow, in the shape asked for.
    let flow =
        s.ok(json!({"op": "embed_diagram", "path": "Charts/Shop:Checkout", "option": "sequence"}));
    assert!(
        flow["mermaid"]
            .as_str()
            .unwrap()
            .contains("sequenceDiagram"),
        "{flow}"
    );
    // What cannot be drawn says why, as a document's broken diagram does.
    let wrong = s.ok(json!({"op": "embed_diagram", "path": "Free:Memo", "section": ids["Free"]}));
    assert!(wrong.get("mermaid").is_none(), "{wrong}");
    assert!(
        wrong["problem"]
            .as_str()
            .unwrap()
            .contains("図になりません"),
        "{wrong}"
    );
    let missing = s.ok(json!({"op": "embed_diagram", "path": "Nowhere:", "section": ids["Free"]}));
    assert!(
        missing["problem"]
            .as_str()
            .unwrap()
            .contains("見つかりません"),
        "{missing}"
    );
}

#[test]
fn a_sections_documents_are_read_in_one_request_as_each_part_is_read_alone() {
    let s = Screen::new();
    let ids = s.ids();
    let parts = s.ok(json!({"op": "document_parts", "documents": [
        {"card": ids["Overview"],
         "places": ["Schemas/Ordering:Order", "Nope:Gone", "Schemas/Ordering:Order"],
         "values": ["Schemas:Status", "Schemas:Nothing"],
         "embeds": [{"path": "Charts/Shop:Refund", "option": "sequence"}, {"path": "Schemas:"},
                    {"path": "Nope:Gone"}]},
        {"card": ids["Rule"], "embeds": [{"path": "Ordering:Order"}]},
    ]}));
    let documents = parts["documents"].as_array().unwrap();
    assert_eq!(documents.len(), 2, "{parts}");
    let overview = &documents[0];
    assert_eq!(overview["card"], ids["Overview"].as_str());
    // A place that resolves is not listed; one that does not, with why.
    let missing = overview["missing"].as_object().unwrap();
    assert_eq!(
        missing.keys().collect::<Vec<_>>(),
        vec!["Nope:Gone"],
        "{overview}"
    );
    assert!(
        !missing["Nope:Gone"].as_str().unwrap().is_empty(),
        "{overview}"
    );
    // The values as `inserted_values` puts them in, from the card's own section.
    let alone = s.ok(
        json!({"op": "inserted_values", "paths": ["Schemas:Status", "Schemas:Nothing"],
                            "section": ids["Docs"]}),
    );
    assert_eq!(overview["values"], alone["values"]);
    assert_eq!(overview["problems"], alone["problems"]);
    assert!(
        overview["problems"]
            .as_object()
            .unwrap()
            .contains_key("Schemas:Nothing")
    );
    // Each diagram as `embed_diagram` draws it for the window, in the order asked.
    let asked = [
        ("Charts/Shop:Refund", Some("sequence")),
        ("Schemas:", None),
        ("Nope:Gone", None),
    ];
    for (at, (path, option)) in asked.iter().enumerate() {
        let mut one = json!({"op": "embed_diagram", "path": path, "section": ids["Docs"]});
        if let Some(option) = option {
            one["option"] = (*option).into();
        }
        assert_eq!(overview["embeds"][at], s.ok(one), "{path}");
    }
    assert!(overview["embeds"][2].get("problem").is_some(), "{overview}");
    // Another document's paths read from its own section.
    let rule = &documents[1];
    assert_eq!(
        rule["embeds"][0],
        s.ok(json!({"op": "embed_diagram", "path": "Ordering:Order", "section": ids["Schemas"]}))
    );
    assert_eq!(rule["missing"], json!({}));
    assert_eq!(
        s.refused(json!({"op": "document_parts"})),
        "document_parts には documents（文書のカードの並び）が要ります"
    );
}

/// The flow's nodes as the window reads them: the cards inside the flow (`project_detail`'s
/// `inner`), in their order.
fn nodes(s: &Screen) -> Vec<Value> {
    let flow = s.ids()["Checkout"].clone();
    s.detail("")["inner"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|n| n["in"] == flow)
        .cloned()
        .collect()
}

/// Where a node sits on its flow's card map, as the window reads it.
fn at(node: &Value) -> Value {
    json!({"x": node["posX"].as_f64().unwrap() as i64, "y": node["posY"].as_f64().unwrap() as i64})
}

#[test]
fn the_window_keeps_a_node_not_finished_yet_but_not_one_that_is_wrong() {
    let s = Screen::new();
    // A node lacking a field of its own — an end whose outcome is taken away — kept from
    // the window, which picks its fields after putting it (MCP refuses it).
    s.tool(
        "card_create",
        json!({"target": "Charts/Shop:Checkout.later", "value": {"kind": "end", "outcome": "success"}}),
    );
    let value = json!({"target": "Charts/Shop:Checkout.later", "value": {"outcome": null}});
    s.mcp_refused("card_update", value.clone());
    s.tool("card_update", value);
    let read = nodes(&s);
    assert_eq!(read.last().unwrap()["name"], "later");
    assert!(
        read.last().unwrap()["form"]["fields"]
            .get("outcome")
            .is_none()
    );
    // One not joined up yet is kept and said.
    let kept = s.tool(
        "card_create",
        json!({"target": "Charts/Shop:Checkout.run2", "value": {"kind": "run"}}),
    );
    assert!(kept["warnings"].to_string().contains("run2"), "{kept}");
    // What is wrong is refused from the window too, and not made.
    let said = s.tool_refused(
        "card_create",
        json!({"target": "Charts/Shop:Checkout.bad", "value": {"kind": "nope"}}),
    );
    assert!(said.contains("のいずれかです"), "{said}");
    let said = s.tool_refused(
        "card_create",
        json!({"target": "Charts/Shop:Checkout.later", "value": {"kind": "end", "outcome": "success"}}),
    );
    assert!(said.contains("カード「later」は既にあります"), "{said}");
    let nodes = s.mcp(
        "card_list",
        json!({"value": {"in": "Charts/Shop:Checkout"}}),
    );
    let paths: Vec<&str> = nodes["cards"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["path"].as_str().unwrap())
        .collect();
    assert!(!paths.contains(&"Charts/Shop:Checkout.bad"), "{nodes}");
    assert_eq!(
        paths
            .iter()
            .filter(|p| **p == "Charts/Shop:Checkout.later")
            .count(),
        1,
        "{nodes}"
    );
    // The flow's nodes written whole in the keys of before are refused from the window too.
    let said = s.tool_refused(
        "card_update",
        json!({"target": "Charts/Shop:Checkout", "value": {"nodes": []}}),
    );
    assert!(said.contains("nodes は使いません"), "{said}");
}

#[test]
fn a_renamed_node_and_rows_written_whole_take_the_paths_into_them_along() {
    let s = Screen::new();
    s.mcp(
        "card_update",
        json!({"target": "Free:Memo", "value": {"body": "見る [[Docs:Spec.intro]] と [[Charts/Shop:Checkout.save]]"}}),
    );

    // A node renamed as a card, the ways to it following by id.
    let out = s.tool(
        "card_update",
        json!({"target": "Charts/Shop:Checkout.save", "value": {"name": "persist"}}),
    );
    assert_eq!(out["branch"], "main");
    let body = s.mcp("card_read", json!({"target": "Free:Memo"}))["body"].clone();
    assert_eq!(
        body,
        "見る [[Docs:Spec.intro]] と [[Charts/Shop:Checkout.persist]]"
    );
    let persist = nodes(&s)
        .into_iter()
        .find(|n| n["name"] == "persist")
        .unwrap();
    assert!(
        nodes(&s).iter().any(|n| n["connections"]
            .as_array()
            .unwrap()
            .iter()
            .any(|l| l["to"] == persist["id"])),
        "{:?}",
        nodes(&s)
    );

    // A formatdoc's row renamed.
    let rows = json!([{"name": "opening", "kind": "item", "heading": "はじめに",
                       "rows": [{"name": "body", "kind": "text", "body": "始める"}]}]);
    s.tool(
        "card_update",
        json!({"target": "Docs:Spec", "value": {"rows": rows},
               "moves": [{"from": ["intro"], "to": ["opening"]}]}),
    );
    let body = s.mcp("card_read", json!({"target": "Free:Memo"}))["body"].clone();
    assert_eq!(
        body,
        "見る [[Docs:Spec.opening]] と [[Charts/Shop:Checkout.persist]]"
    );

    assert_eq!(
        s.tool_refused(
            "card_update",
            json!({"target": "Docs:Spec", "value": {"rows": rows, "description": "x"}, "moves": []}),
        ),
        "moves は rows を丸ごと書くときに添えます（value の description とは一緒に書けません）"
    );
    assert!(
        s.tool_refused(
            "card_update",
            json!({"target": "Docs:Spec", "value": {"rows": rows}, "moves": [{"from": "intro"}]}),
        )
        .starts_with("moves は [")
    );
    assert_eq!(
        s.tool_refused(
            "card_update",
            json!({"target": "Docs:Spec.opening", "value": {"rows": rows}, "moves": []}),
        ),
        "Docs:Spec.opening はカードではありません（moves はカードの rows を丸ごと書くときに添えます）"
    );
}

#[test]
fn the_history_is_trimmed_and_the_windows_holds_let_go() {
    let s = Screen::new();
    // Nothing past the stop yet: nothing goes.
    assert_eq!(s.ok(json!({"op": "prune"})), json!({"pruned": 0}));
    assert_eq!(
        s.refused(json!({"op": "prune", "project": "nope"})),
        "プロジェクトが見つかりません"
    );
    // Main's history grown past its stop (1,000 commits) with 1,100 older than the days kept
    // (90), under the commits it has; a branch's with as many of now. Only what is past both
    // goes: the oldest of main's, never the branch's, never main's newest.
    let mut conn = rusqlite::Connection::open(&s.store).unwrap();
    let branch: String = conn
        .query_row(
            "SELECT id FROM branches WHERE project_id = ? LIMIT 1",
            [&s.project],
            |r| r.get(0),
        )
        .unwrap();
    let seqs = |conn: &rusqlite::Connection, tree: &str| -> Vec<i64> {
        let mut stmt = conn
            .prepare("SELECT seq FROM commits WHERE project_id = ? AND branch_id = ? ORDER BY seq")
            .unwrap();
        stmt.query_map([s.project.as_str(), tree], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    };
    let had = seqs(&conn, "");
    assert!(!had.is_empty() && had.len() < 1_000, "{had:?}");
    let branch_had = seqs(&conn, &branch).len() as i64;
    let tx = conn.transaction().unwrap();
    tx.execute(
        "UPDATE commits SET seq = seq + 10000 WHERE project_id = ? AND branch_id = ''",
        [&s.project],
    )
    .unwrap();
    for seq in 1..=1_100_i64 {
        for (tree, ago, shift) in [("", "-100 days", 0), (branch.as_str(), "-0 days", 20000)] {
            tx.execute(
                "INSERT INTO commits (id, project_id, branch_id, seq, at, author_kind)
                 VALUES (?1, ?2, ?3, ?4, strftime('%Y-%m-%dT%H:%M:%fZ', 'now', ?5), 'mcp')",
                rusqlite::params![
                    format!("old-{tree}-{seq}"),
                    s.project,
                    tree,
                    seq + shift,
                    ago
                ],
            )
            .unwrap();
        }
    }
    tx.commit().unwrap();
    let past = 1_100 + had.len() as i64 - 1_000;
    assert_eq!(s.ok(json!({"op": "prune"})), json!({"pruned": past}));
    let main = seqs(&conn, "");
    assert_eq!(main.len(), 1_000);
    assert_eq!(main[0], past + 1, "the oldest went first");
    for seq in &had {
        assert!(main.contains(&(seq + 10000)), "main's newest {seq} is kept");
    }
    assert_eq!(seqs(&conn, &branch).len() as i64, branch_had + 1_100);
    assert_eq!(s.ok(json!({"op": "prune"})), json!({"pruned": 0}));
    s.ok(json!({"op": "group_begin"}));
    assert_eq!(s.ok(json!({"op": "holds"}))["holds"][0]["holder"], "screen");
    s.ok(json!({"op": "release_screen_holds"}));
    assert_eq!(s.ok(json!({"op": "holds"}))["holds"], json!([]));
}

#[test]
fn steps_only_tried_leave_the_tree_to_the_window_and_an_opened_batch_holds_it() {
    let s = Screen::new();
    let step =
        json!({"tool": "project_update", "arguments": {"value": {"description": "バッチで"}}});
    let tried = s.mcp("batch", json!({"value": {"steps": [step.clone()]}}));
    assert!(tried.get("batch").is_none(), "{tried}");
    assert_eq!(s.ok(json!({"op": "holds"}))["holds"], json!([]));
    let opened = s.mcp("batch", json!({"value": {"open": true, "steps": [step]}}));
    assert!(opened["batch"].is_string(), "{opened}");
    let said = s.refused(json!({"op": "group_begin"}));
    assert!(said.contains("MCP バッチ処理中です"), "{said}");
    s.mcp("batch", json!({"target": opened["batch"], "execute": true}));
    assert_eq!(s.ok(json!({"op": "holds"}))["holds"], json!([]));
}

#[test]
fn the_window_lets_go_of_the_batches_holding_a_tree() {
    let s = Screen::new();
    let opened = s.mcp(
        "batch",
        json!({"value": {"open": true, "steps": [{"tool": "project_update", "arguments": {"value": {"description": "バッチで"}}}]}}),
    );
    assert!(opened["batch"].is_string(), "{opened}");
    assert_eq!(s.ok(json!({"op": "holds"}))["holds"][0]["holder"], "mcp");
    assert_eq!(
        s.ok(json!({"op": "batches_cancel"})),
        json!({"cancelled": 1})
    );
    assert_eq!(s.ok(json!({"op": "holds"}))["holds"], json!([]));
    // Nothing held, nothing to let go.
    assert_eq!(
        s.ok(json!({"op": "batches_cancel"})),
        json!({"cancelled": 0})
    );
}

#[test]
fn a_project_file_read_in_its_place_keeps_where_this_mac_writes_it_out() {
    let s = Screen::new();
    let dirs = s.ok(json!({"op": "workdir_detail"}))["dirs"].clone();
    let dir = dirs[0]["id"]
        .as_str()
        .expect("the script's project has a working directory")
        .to_string();
    s.tool(
        "section_update",
        json!({"target": "ts", "value": {"folder": "~/書き出し先"}}),
    );
    let text = s.ok(json!({"op": "project_file"}))["text"].clone();
    s.ok(json!({"op": "project_file_read", "text": text, "landing": "replace"}));
    let after = s.ok(json!({"op": "workdir_detail"}))["dirs"].clone();
    let kept = after
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["id"] == dir.as_str())
        .unwrap();
    assert_eq!(kept["folder"], "~/書き出し先", "{after}");
}

/// What main's lock lets through (`specboard://SpecBoard/Docs/Design:DesignBranches` §8), the same for the window
/// and for MCP: how main is arranged and tagged, and the project's own settings. The rest of
/// a card, a section whole, and what sits on a section's diagram for another section are
/// changed in a branch and merged.
struct Locked {
    s: Screen,
    schemas: String,
    ordering: String,
    order: String,
    customer: String,
    root: String,
}

impl Locked {
    /// The script's project, with Order drawn on Schemas' diagram and Ordering summed up
    /// there, a second section beside Ordering, and then main locked.
    fn new() -> Locked {
        let s = Screen::new();
        s.mcp("section_create", json!({"target": "Schemas/Billing"}));
        let ids = s.ids();
        let id = |name: &str| ids[name].clone();
        let root = s.detail("")["sections"]
            .as_array()
            .unwrap()
            .iter()
            .find(|section| section["parentId"].is_null())
            .unwrap()["name"]
            .as_str()
            .unwrap()
            .to_string();
        let locked = Locked {
            schemas: id("Schemas"),
            ordering: id("Ordering"),
            order: id("Order"),
            customer: id("Customer"),
            root,
            s,
        };
        locked.s.ok(json!({"op": "places", "boxes": [
            {"view": locked.schemas, "item": locked.order, "x": 300, "y": 4},
            {"view": locked.schemas, "kind": "section", "item": locked.ordering, "x": 300, "y": 40},
        ]}));
        locked.s.ok(json!({"op": "main_lock", "on": true}));
        locked
    }

    fn tags(&self, path: &str) -> Value {
        self.s.mcp("card_read", json!({"target": path}))["tags"].clone()
    }

    /// The first line of main's newest commit.
    fn said(&self) -> Value {
        self.s.ok(json!({"op": "commits", "limit": 1}))["commits"][0]["lines"][0]["summary"].clone()
    }
}

fn locked_main(said: &str) -> bool {
    said.contains(&specboard_core::write::locked_main())
}

/// What a locked main refuses, the same from both sides: the rest of a card, with its
/// tags or alone, a card ghost made (a card of its own), and every section's own —
/// the root's name, the order.
fn refused_under_the_lock(l: &Locked) -> Vec<(&'static str, Value)> {
    vec![
        (
            "card_update",
            json!({"target": "Schemas:Customer", "value": {"description": "x"}}),
        ),
        (
            "card_update",
            json!({"target": "Schemas:Customer", "value": {"tags": ["顧客"], "label": "客"}}),
        ),
        (
            "section_update",
            json!({"target": l.root, "value": {"name": "Root"}}),
        ),
        (
            "card_create",
            json!({"target": "Schemas:OrderGhost", "value": {"kind": "ghost", "target": "Schemas/Ordering:Order"}}),
        ),
        (
            "section_update",
            json!({"target": "Schemas/Billing", "value": {"position": 0}}),
        ),
    ]
}

#[test]
fn a_locked_main_is_arranged_and_tagged_from_the_window_and_nothing_else() {
    let l = Locked::new();
    let s = &l.s;
    let before = s.last_seq("");

    // Its own cards' boxes, a writing card's size, a formatdoc's look, a flow's nodes:
    // no commit. A node is put as a card on its flow's card map, by its id.
    let save = nodes(s).into_iter().find(|n| n["name"] == "save").unwrap();
    s.ok(json!({"op": "places", "boxes": [
        {"item": l.customer, "x": 40, "y": 2, "pinned": true},
        {"view": s.ids()["Checkout"], "kind": "card", "item": save["id"], "x": 5, "y": 5},
    ]}));
    let save = nodes(s).into_iter().find(|n| n["name"] == "save").unwrap();
    assert_eq!(at(&save), json!({"x": 5, "y": 5}));
    assert!(
        s.refused(json!({"op": "places", "boxes": [
            {"view": s.ids()["Checkout"], "kind": "node", "item": "save", "x": 5, "y": 5}
        ]}))
        .starts_with("箱の kind は card か section です")
    );
    s.tool(
        "card_update",
        json!({"target": "Free:Memo", "value": {"size": {"w": 12, "h": 6}}}),
    );
    s.tool(
        "card_update",
        json!({"target": "Docs:Spec", "value": {"view": "document"}}),
    );
    assert_eq!(s.last_seq(""), before);
    assert_eq!(s.card("", &l.customer)["posX"], json!(40.0));

    // Its tags, on the card and by the tag: in main's history.
    s.tool(
        "card_update",
        json!({"target": "Schemas:Customer", "value": {"tags": ["顧客"]}}),
    );
    s.tool(
        "tag_update",
        json!({"target": "重要", "value": {"add": ["Schemas:Customer"]}}),
    );
    assert_eq!(l.tags("Schemas:Customer"), json!(["顧客", "重要"]));
    assert_eq!(s.last_seq(""), before + 2);
    assert_eq!(l.said(), "1 件のタグを変更（+重要）");
    // The tag groups and the project's own settings.
    s.tool(
        "tag_update",
        json!({"target": "group:種類", "value": {"add": ["顧客"]}}),
    );
    s.tool(
        "project_update",
        json!({"value": {"description": "ロック中も直す", "layout": {"cardRows": 12}}}),
    );
    s.ok(json!({"op": "writing_rules_set", "place": "card.description", "text": "一文目で言う"}));

    // What sits on Schemas' diagram for another section moves too — moved, or put
    // there: no commit either.
    s.ok(json!({"op": "places", "boxes": [
        {"view": l.schemas, "item": l.order, "x": 310, "y": 4},
        {"view": l.schemas, "kind": "section", "item": l.ordering, "x": 310, "y": 40},
        {"view": l.ordering, "item": l.customer, "x": 0, "y": 90},
    ]}));

    for (tool, arguments) in refused_under_the_lock(&l) {
        let said = s.tool_refused(tool, arguments.clone());
        assert!(locked_main(&said), "{tool} {arguments}: {said}");
    }
    assert_eq!(l.tags("Schemas:Customer"), json!(["顧客", "重要"]));
}

#[test]
fn a_locked_main_is_arranged_and_tagged_from_mcp_and_nothing_else() {
    let l = Locked::new();
    let s = &l.s;
    let before = s.last_seq("");

    // Its own cards' boxes, a writing card's size, a formatdoc's look, a flow's nodes.
    s.mcp(
        "card_update",
        json!({"target": "Schemas:Customer", "value": {"topLeft": {"x": 40, "y": 2}}}),
    );
    s.mcp(
        "section_update",
        json!({"target": "Schemas", "value": {"places": {
            "Schemas:Status": {"topLeft": {"x": 80, "y": 2}},
        }}}),
    );
    s.mcp(
        "card_update",
        json!({"target": "Free:Memo", "value": {"size": {"w": 12, "h": 6}}}),
    );
    s.mcp(
        "card_update",
        json!({"target": "Docs:Spec", "value": {"view": "document"}}),
    );
    // A flow's node is a card inside it: its box on the flow's card map.
    s.mcp(
        "card_update",
        json!({"target": "Charts/Shop:Checkout.save", "value": {"topLeft": {"x": 30, "y": 30}}}),
    );
    assert_eq!(s.last_seq(""), before);
    let save = nodes(s).into_iter().find(|n| n["name"] == "save").unwrap();
    assert_eq!(at(&save), json!({"x": 30, "y": 30}));

    // Its tags, on the card and by the tag: in main's history.
    s.mcp(
        "card_update",
        json!({"target": "Schemas:Customer", "value": {"tags": ["顧客"]}}),
    );
    s.mcp(
        "tag_update",
        json!({"target": "重要", "value": {"add": ["Schemas:Customer"]}}),
    );
    assert_eq!(l.tags("Schemas:Customer"), json!(["顧客", "重要"]));
    assert_eq!(s.last_seq(""), before + 2);
    assert_eq!(l.said(), "1 件のタグを変更（+重要）");
    // The tag groups and the project's own settings.
    s.mcp(
        "tag_update",
        json!({"target": "group:種類", "value": {"add": ["顧客"]}}),
    );
    s.mcp(
        "project_update",
        json!({"value": {"description": "ロック中も直す", "layout": {"cardRows": 12}}}),
    );

    // What sits on Schemas' diagram for another section moves too.
    s.mcp(
        "section_update",
        json!({"target": "Schemas", "value": {"places": {
            "Schemas/Ordering:Order": {"topLeft": {"x": 310, "y": 4}},
            "Schemas/Ordering": {"topLeft": {"x": 310, "y": 40}},
        }}}),
    );
    let boxes = s.mcp(
        "section_read",
        json!({"target": "Schemas", "value": {"include": ["places"]}}),
    )["places"]
        .clone();
    let order = &boxes["Schemas/Ordering:Order"];
    assert_eq!(order["topLeft"], json!({"x": 310, "y": 4}));

    for (tool, arguments) in refused_under_the_lock(&l) {
        let said = s.mcp_refused(tool, arguments.clone());
        assert!(locked_main(&said), "{tool} {arguments}: {said}");
    }
    assert_eq!(l.tags("Schemas:Customer"), json!(["顧客", "重要"]));
}

/// A locked main put right in one go: in a branch of the app's own the window names and MCP
/// does not, held with main; closed, merged into main as one commit, the boxes moved along;
/// given up, gone; left when the app quit, merged when it opens.
#[test]
fn a_locked_main_is_put_right_in_one_go_in_a_branch_of_its_own() {
    let s = Screen::new();
    let customer = s.ids()["Customer"].clone();
    s.ok(json!({"op": "main_lock", "on": true}));
    let begun = s.ok(json!({"op": "group_begin"}));
    let group = begun["branch"]
        .as_str()
        .expect("a branch of its own")
        .to_string();
    // Begun again: the same one.
    assert_eq!(s.ok(json!({"op": "group_begin"}))["branch"], group.as_str());
    let listed = s.ok(json!({"op": "branches"}));
    assert!(!listed.to_string().contains("~group"), "{listed}");
    for tree in ["", group.as_str()] {
        assert_eq!(
            s.ok(json!({"op": "holds", "branch": tree}))["holds"][0]["holder"],
            "screen"
        );
    }
    // The window writes there; main is as it was until it closes.
    s.tool(
        "card_update",
        json!({"branch": "~group", "target": "Schemas:Customer", "value": {"label": "お客さま"}}),
    );
    s.tool(
        "card_update",
        json!({"branch": "~group", "target": "Schemas:Customer", "value": {"description": "まとめて"}}),
    );
    s.ok(json!({"op": "places", "branch": group, "boxes": [{"item": customer, "x": 40, "y": 2}]}));
    assert_eq!(s.card(&group, &customer)["label"], "お客さま");
    assert_ne!(s.card("", &customer)["label"], "お客さま");
    // MCP sees no such branch, and main is the window's while it lasts.
    assert!(
        s.mcp_refused(
            "card_read",
            json!({"branch": "~group", "target": "Schemas:Customer"})
        )
        .starts_with("branch「~group」はありません"),
    );
    assert_eq!(
        s.refused(json!({"op": "main_lock", "on": false})),
        "まとめて直している間は、main のロックを外せません（閉じるかキャンセルのあとで外します）"
    );
    let before = s.last_seq("");
    s.ok(json!({"op": "group_end", "branch": group}));
    let card = s.card("", &customer);
    assert_eq!(card["label"], "お客さま");
    assert_eq!(card["description"], "まとめて");
    assert_eq!(
        (card["posX"].clone(), card["posY"].clone()),
        (json!(40.0), json!(2.0))
    );
    assert_eq!(s.last_seq(""), before + 1, "one commit in main");
    let commits = s.ok(json!({"op": "commits", "limit": 1}))["commits"].clone();
    assert_eq!(
        commits[0]["lines"][0]["summary"],
        "まとめて直した（足す 0・直す 1・消す 0・位置 1）"
    );
    assert_eq!(s.ok(json!({"op": "holds"}))["holds"], json!([]));
    let left: i64 = rusqlite::Connection::open(&s.store)
        .unwrap()
        .query_row(
            "SELECT count(*) FROM branches WHERE name LIKE '~%'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(left, 0);

    // Given up: main as it was, the branch gone.
    let group = s.ok(json!({"op": "group_begin"}))["branch"]
        .as_str()
        .unwrap()
        .to_string();
    s.tool(
        "card_update",
        json!({"branch": "~group", "target": "Schemas:Customer", "value": {"label": "やめる分"}}),
    );
    assert_eq!(
        s.ok(json!({"op": "group_abandon", "branch": group})),
        json!({"undone": 1})
    );
    assert_eq!(s.card("", &customer)["label"], "お客さま");
    assert_eq!(s.ok(json!({"op": "holds"}))["holds"], json!([]));

    // Left when the app quit: merged as it opens again.
    s.ok(json!({"op": "group_begin"}));
    s.tool(
        "card_update",
        json!({"branch": "~group", "target": "Schemas:Customer", "value": {"label": "閉じ忘れ"}}),
    );
    s.ok(json!({"op": "release_screen_holds"}));
    assert_eq!(s.card("", &customer)["label"], "閉じ忘れ");
    assert_eq!(s.ok(json!({"op": "holds"}))["holds"], json!([]));
    s.ok(json!({"op": "main_lock", "on": false}));
}

/// The windows of two Macs on an agent's core: each request with the Mac it came from (`client`,
/// as the agent hands it on). What one puts right in one go, the other waits for; each lets go
/// of its own when it opens again.
#[test]
fn the_windows_of_two_macs_are_two_hands() {
    let s = Screen::new();
    let from = |mac: &str, mut request: Value| {
        request["client"] = mac.into();
        request
    };
    let card = |mac: &str, label: &str| {
        from(
            mac,
            json!({"op": "tool", "name": "card_update", "arguments": {
                "project": PROJECT, "target": "Schemas:Customer", "value": {"label": label}}}),
        )
    };
    s.ok(from("MacBook", json!({"op": "group_begin"})));
    let holds = s.ok(json!({"op": "holds"}));
    assert_eq!(holds["holds"][0]["client"], "MacBook", "{holds}");
    // Its own window writes; the other Mac's, and the app holding the store itself, wait.
    s.ok(card("MacBook", "まとめて"));
    let refused = s.refused(card("iMac", "割り込み"));
    assert!(
        refused.contains("MacBook の画面でまとめて直しているので"),
        "{refused}"
    );
    assert!(s.refused(card("", "割り込み")).contains("MacBook"));
    assert!(
        s.refused(from("iMac", json!({"op": "group_begin"})))
            .contains("MacBook")
    );
    assert!(
        s.refused(from("iMac", json!({"op": "main_lock", "on": true})))
            .contains("MacBook")
    );
    // Another Mac opening lets go of its own only.
    s.ok(from("iMac", json!({"op": "release_screen_holds"})));
    assert_eq!(
        s.ok(json!({"op": "holds"}))["holds"][0]["client"],
        "MacBook"
    );
    // Ended by the other Mac: nothing of its own to end, the hold stays.
    s.ok(from("iMac", json!({"op": "group_end"})));
    assert_eq!(
        s.ok(json!({"op": "holds"}))["holds"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    s.ok(from("MacBook", json!({"op": "release_screen_holds"})));
    assert_eq!(s.ok(json!({"op": "holds"}))["holds"], json!([]));
    s.ok(card("iMac", "あとから"));

    // A locked main: one Mac's group branch is not the other's to close.
    s.ok(from("MacBook", json!({"op": "main_lock", "on": true})));
    let group = s.ok(from("MacBook", json!({"op": "group_begin"})))["branch"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(
        s.refused(from("iMac", json!({"op": "group_begin"})))
            .contains("MacBook")
    );
    s.ok(from("iMac", json!({"op": "group_end", "branch": group})));
    assert!(
        s.ok(json!({"op": "holds", "branch": group}))["holds"]
            .as_array()
            .is_some_and(|h| !h.is_empty())
    );
    s.ok(from("MacBook", json!({"op": "group_end"})));
    assert_eq!(s.ok(json!({"op": "holds"}))["holds"], json!([]));
}
