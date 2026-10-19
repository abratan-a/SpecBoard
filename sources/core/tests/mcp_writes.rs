//! What MCP may do in a project (`specboard://SpecBoard/Docs/Design:DesignAccess` §6): an item for each tool
//! that writes, each allowed, asked of a person each time, or denied — the project's own setting, which the
//! window sets (`mcp_writes_set`) by item or by group and MCP only reads (`project_read`'s `mcpAsk` and
//! `mcpDenied`). Every item is allowed but ten (the writing rules, the slug, main's root's name, the working
//! directories' paths, the exports' made, changed and let go, their folders, and undoing another writer's
//! commit), asked each time, until a
//! person says otherwise; a call that touches a denied item is refused, naming each, and writes nothing; one
//! that touches an asked item asks a person through the window when it says `request: true` (`App::ask`), and
//! is refused when it does not; a batch's steps are judged one by one, and asked about once, for the run; the
//! board's tree is the board's item alone; the window writes whatever it writes. And `project_update`'s `name`
//! and `rules`.

use crate::common;

use common::{call, op};
use serde_json::{Value, json};
use specboard_core::{App, Core};
use std::sync::{Arc, Mutex};

const PROJECT: &str = "見本";

/// What a call is told for the project's items (as `<id>（<name>）`) that are denied.
fn denied(items: &[&str]) -> String {
    format!(
        "このプロジェクトでは、{} は MCP から使えません（拒否）。変えられるのは人だけで、アプリのプロジェクト設定の「MCP」のタブで許可か都度承認にします",
        items.join("・")
    )
}

/// What a call is told for the items it touches that are asked each time, when it did not say
/// `request: true`.
fn unrequested(items: &[&str], tool: &str) -> String {
    format!(
        "{} は都度承認です。{tool} に request: true を付けて呼び直すと、画面に許可を求めて答えを待ちます（2 分まで）",
        items.join("・")
    )
}

const RULES: &str = "project_update.rules（書き方の決まり）";
const CARD_UPDATE: &str = "card_update（カードを変える）";
const CARD_CREATE: &str = "card_create（カードを作る）";
const CARD_DELETE: &str = "card_delete（カードを削除）";

/// A window that answers what it is asked as it was told to, and keeps what it was asked.
#[derive(Default)]
struct Person {
    answers: Mutex<Vec<Value>>,
    asked: Mutex<Vec<Value>>,
}

impl Person {
    fn will(&self, answer: Value) {
        self.answers.lock().unwrap().push(answer);
    }

    fn asked(&self) -> Vec<Value> {
        self.asked.lock().unwrap().clone()
    }
}

impl App for Person {
    fn app_control(&self, _: &str, _: &str) -> String {
        json!({"error": "画面は動かしません"}).to_string()
    }

    fn ask(&self, request: &str) -> String {
        self.asked
            .lock()
            .unwrap()
            .push(serde_json::from_str(request).unwrap());
        let mut answers = self.answers.lock().unwrap();
        assert!(!answers.is_empty(), "答えを決めていない求め: {request}");
        answers.remove(0).to_string()
    }
}

/// What a person may say, as a window answers with it.
fn answer(said: &str) -> Value {
    json!({"answer": said, "client": "MacBook"})
}

struct Store {
    core: Core,
    project: String,
}

fn store() -> Store {
    let core = Core::open(common::script_store()).unwrap();
    let project = rusqlite::Connection::open(core.place())
        .unwrap()
        .query_row("SELECT id FROM projects WHERE name = ?", [PROJECT], |r| {
            r.get(0)
        })
        .unwrap();
    Store { core, project }
}

impl Store {
    /// The project's slug, shown beside its name where a person is asked.
    fn slug(&self) -> String {
        rusqlite::Connection::open(self.core.place())
            .unwrap()
            .query_row(
                "SELECT slug FROM projects WHERE id = ?",
                [&self.project],
                |r| r.get(0),
            )
            .unwrap()
    }

    /// A tool called through MCP with the tests' key: its answer, or what refused it.
    fn mcp(&self, tool: &str, arguments: Value) -> Result<Value, String> {
        let (refused, text) = call(&self.core, tool, arguments);
        let said: Value = serde_json::from_str(&text).unwrap_or_else(|_| text.clone().into());
        if refused {
            return Err(said["error"]["message"]
                .as_str()
                .map_or(text, str::to_string));
        }
        Ok(said)
    }

    /// A tool called as the window calls it (`tool`).
    fn window_tool(&self, tool: &str, arguments: Value) -> Result<Value, String> {
        let said = op(
            &self.core,
            json!({"op": "tool", "name": tool, "arguments": arguments}),
        );
        match said.get("error") {
            Some(e) => Err(e.as_str().unwrap().to_string()),
            None => Ok(said["result"]["json"].clone()),
        }
    }

    /// An item or a group set to a state, through the window: the view it answers.
    fn set(&self, what: Value) -> Value {
        let mut request = json!({"op": "mcp_writes_set", "project": self.project});
        for (k, v) in what.as_object().unwrap() {
            request[k] = v.clone();
        }
        let said = op(&self.core, request);
        assert!(said.get("error").is_none(), "{said}");
        said["result"].clone()
    }

    fn view(&self) -> Value {
        let said = op(
            &self.core,
            json!({"op": "mcp_writes", "project": self.project}),
        );
        assert!(said.get("error").is_none(), "{said}");
        said["result"].clone()
    }

    /// The items not allowed, as the window reads them: id and state.
    fn not_allowed(&self) -> Vec<(String, String)> {
        not_allowed_in(&self.view())
    }

    /// What `project_read` says of the items asked each time and those denied.
    fn read_says(&self) -> (Value, Value) {
        let read = self
            .mcp("project_read", json!({"project": PROJECT}))
            .unwrap();
        (read["mcpAsk"].clone(), read["mcpDenied"].clone())
    }

    fn description(&self, path: &str, branch: &str) -> Value {
        let read = self
            .mcp(
                "card_read",
                json!({"project": PROJECT, "branch": branch, "target": path}),
            )
            .unwrap();
        read["description"].clone()
    }

    fn kept(&self) -> Option<String> {
        rusqlite::Connection::open(self.core.place())
            .unwrap()
            .query_row(
                "SELECT mcp_writes FROM projects WHERE id = ?",
                [&self.project],
                |r| r.get(0),
            )
            .unwrap()
    }

    /// The summaries of main's newest lines.
    fn lines(&self, count: usize) -> Vec<String> {
        let commits = op(
            &self.core,
            json!({"op": "commits", "project": self.project, "limit": count}),
        );
        commits["result"]["commits"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["lines"][0]["summary"].as_str().unwrap().to_string())
            .collect()
    }

    /// A window that answers what it is asked, handed to the core.
    fn person(&self) -> Arc<Person> {
        let person = Arc::new(Person::default());
        self.core.set_app(person.clone());
        person
    }
}

fn not_allowed_in(view: &Value) -> Vec<(String, String)> {
    view["groups"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|g| g["items"].as_array().unwrap().iter())
        .filter(|i| i["state"] != "allow")
        .map(|i| {
            (
                i["id"].as_str().unwrap().to_string(),
                i["state"].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

#[test]
fn by_default_ten_items_are_asked_each_time_and_the_rest_allowed() {
    let s = store();
    let view = s.view();
    let groups: Vec<(&str, &str, usize)> = view["groups"]
        .as_array()
        .unwrap()
        .iter()
        .map(|g| {
            (
                g["id"].as_str().unwrap(),
                g["name"].as_str().unwrap(),
                g["items"].as_array().unwrap().len(),
            )
        })
        .collect();
    assert_eq!(
        groups,
        [
            ("project", "プロジェクト", 5),
            ("kind", "種類", 3),
            ("section", "セクション", 4),
            ("card", "カード", 3),
            ("tag", "タグ", 1),
            ("export", "エクスポート", 6),
            ("branch", "branch", 6),
            ("mr", "MR", 1),
            ("board", "ボード", 1),
        ]
    );
    let rules = &view["groups"][0]["items"][1];
    assert_eq!(
        (
            &rules["id"],
            &rules["name"],
            &rules["state"],
            &rules["byDefault"]
        ),
        (
            &json!("project_update.rules"),
            &json!("書き方の決まり"),
            &json!("ask"),
            &json!("ask")
        )
    );
    assert_eq!(
        s.not_allowed(),
        [
            ("project_update.rules".to_string(), "ask".to_string()),
            ("project_update.slug".to_string(), "ask".to_string()),
            ("project_update.main_tree".to_string(), "ask".to_string()),
            ("section_update.folder".to_string(), "ask".to_string()),
            ("export_create".to_string(), "ask".to_string()),
            ("export_update".to_string(), "ask".to_string()),
            ("export_delete".to_string(), "ask".to_string()),
            ("export.dir".to_string(), "ask".to_string()),
            ("export.default".to_string(), "ask".to_string()),
            ("branch_revert.others".to_string(), "ask".to_string())
        ]
    );
    // MCP reads it, by id; none denied, none said.
    assert_eq!(
        s.read_says(),
        (
            json!([
                "project_update.rules",
                "project_update.slug",
                "project_update.main_tree",
                "section_update.folder",
                "export_create",
                "export_update",
                "export_delete",
                "export.dir",
                "export.default",
                "branch_revert.others"
            ]),
            Value::Null
        )
    );
    // A call that writes the rules and does not ask is refused whole: the description with it is
    // not kept.
    let rules = json!({"card.entity.description": "型の役目を一文で"});
    assert_eq!(
        s.mcp(
            "project_update",
            json!({"project": PROJECT, "value": {"description": "書かない", "rules": rules}})
        )
        .unwrap_err(),
        unrequested(&[RULES], "project_update")
    );
    let read = s.mcp("project_read", json!({"project": PROJECT})).unwrap();
    assert_ne!(read["description"], "書かない");
    assert!(
        s.mcp(
            "project_update",
            json!({"project": PROJECT, "value": {"description": "書く"}})
        )
        .is_ok()
    );
    // The window sets the rules whatever the project lets MCP do.
    let written = s
        .window_tool(
            "project_update",
            json!({"project": PROJECT, "value": {"rules": rules}}),
        )
        .unwrap();
    assert_eq!(written["rules"], rules);
    // Nothing kept while every item is as it was.
    assert_eq!(s.kept(), None);
}

#[test]
fn an_item_or_a_whole_group_is_switched() {
    let s = store();
    for item in [
        "project_update.rules",
        "project_update.slug",
        "project_update.main_tree",
        "section_update.folder",
        "export_create",
        "export_update",
        "export_delete",
        "export.dir",
        "export.default",
        "branch_revert.others",
    ] {
        s.set(json!({"item": item, "state": "allow"}));
    }
    assert!(s.not_allowed().is_empty());
    assert_eq!(
        s.read_says(),
        (Value::Null, Value::Null),
        "全部許可なら出さない"
    );
    assert_eq!(
        s.kept().as_deref(),
        Some(
            r#"{"project_update.rules":"allow","project_update.slug":"allow","project_update.main_tree":"allow","section_update.folder":"allow","export_create":"allow","export_update":"allow","export_delete":"allow","export.dir":"allow","export.default":"allow","branch_revert.others":"allow"}"#
        )
    );
    assert!(
        s.mcp(
            "project_update",
            json!({"project": PROJECT, "value": {"rules": {"project.description": "一文で"}}})
        )
        .is_ok()
    );

    // A group at once: every item of it.
    let before = s.description("Schemas:Customer", "main");
    s.set(json!({"group": "card", "state": "deny"}));
    assert_eq!(
        s.not_allowed(),
        [
            ("card_create".to_string(), "deny".to_string()),
            ("card_update".to_string(), "deny".to_string()),
            ("card_delete".to_string(), "deny".to_string()),
        ]
    );
    assert_eq!(
        s.mcp(
            "card_update",
            json!({"project": PROJECT, "target": "Schemas:Customer", "value": {
                "tags": ["core"], "description": "書かない", "topLeft": {"x": 0, "y": 40}
            }})
        )
        .unwrap_err(),
        denied(&[CARD_UPDATE])
    );
    assert_eq!(s.description("Schemas:Customer", "main"), before);
    // Another tool's item is its own.
    assert!(
        s.mcp(
            "tag_update",
            json!({"project": PROJECT, "target": "core", "value": {"add": ["Schemas:Customer"]}})
        )
        .is_ok()
    );

    // One item of it back: the rest stays as it was.
    let view = s.set(json!({"item": "card_update", "state": "allow"}));
    assert_eq!(
        not_allowed_in(&view),
        [
            ("card_create".to_string(), "deny".to_string()),
            ("card_delete".to_string(), "deny".to_string()),
        ]
    );
    assert!(
        s.mcp(
            "card_update",
            json!({"project": PROJECT, "target": "Schemas:Customer", "value": {"description": "書く"}})
        )
        .is_ok()
    );
    assert_eq!(s.description("Schemas:Customer", "main"), "書く");
    assert_eq!(
        s.read_says(),
        (Value::Null, json!(["card_create", "card_delete"]))
    );

    // Back to how it was: nothing kept.
    s.set(json!({"item": "project_update.slug", "state": "ask"}));
    s.set(json!({"item": "project_update.main_tree", "state": "ask"}));
    s.set(json!({"item": "section_update.folder", "state": "ask"}));
    s.set(json!({"group": "export", "state": "ask"}));
    s.set(json!({"item": "export_execute", "state": "allow"}));
    s.set(json!({"item": "branch_revert.others", "state": "ask"}));
    s.set(json!({"group": "card", "state": "allow"}));
    s.set(json!({"item": "project_update.rules", "state": "ask"}));
    assert_eq!(s.kept(), None);
    assert_eq!(
        s.read_says(),
        (
            json!([
                "project_update.rules",
                "project_update.slug",
                "project_update.main_tree",
                "section_update.folder",
                "export_create",
                "export_update",
                "export_delete",
                "export.dir",
                "export.default",
                "branch_revert.others"
            ]),
            Value::Null
        )
    );

    // Each change is a line in main's history.
    assert_eq!(
        &s.lines(2)[..],
        [
            "MCP の project_update.rules を都度承認にする",
            "MCP の card_create・card_update・card_delete を許可にする",
        ]
    );

    // What the window asks is checked.
    let ask = |request: Value| op(&s.core, request)["error"].as_str().unwrap().to_string();
    assert!(
        ask(
            json!({"op": "mcp_writes_set", "project": s.project, "item": "card.update", "state": "allow"})
        )
        .starts_with("MCP の項目「card.update」はありません（ある: project_update・project_update.rules・")
    );
    assert_eq!(
        ask(
            json!({"op": "mcp_writes_set", "project": s.project, "group": "spec", "state": "allow"})
        ),
        "MCP の項目の組「spec」はありません（ある: project・kind・section・card・tag・export・branch・mr・board）"
    );
    assert_eq!(
        ask(
            json!({"op": "mcp_writes_set", "project": s.project, "item": "card_update", "group": "card", "state": "allow"})
        ),
        "mcp_writes_set には item（項目の id）か group（組の id）のどちらか 1 つが要ります"
    );
    for state in [json!(null), json!(true), json!("yes")] {
        assert_eq!(
            ask(
                json!({"op": "mcp_writes_set", "project": s.project, "item": "card_update", "state": state})
            ),
            "mcp_writes_set には state（allow・ask・deny のどれか）が要ります"
        );
    }
    assert_eq!(
        ask(json!({"op": "mcp_writes", "project": "nope"})),
        "プロジェクトが見つかりません"
    );
}

#[test]
fn the_spec_and_the_board_are_apart() {
    let s = store();
    s.set(json!({"group": "card", "state": "deny"}));
    s.set(json!({"group": "section", "state": "deny"}));
    // main and a branch are the spec.
    assert_eq!(
        s.mcp(
            "card_create",
            json!({"project": PROJECT, "target": "Free:Scrap", "value": {"kind": "note", "body": "x"}})
        )
        .unwrap_err(),
        denied(&[CARD_CREATE])
    );
    assert_eq!(
        s.mcp(
            "card_update",
            json!({"project": PROJECT, "branch": "refunds", "target": "Schemas:Refund", "value": {"description": "x"}})
        )
        .unwrap_err(),
        denied(&[CARD_UPDATE])
    );
    // The board is not: its own item says, whatever the spec's do.
    assert!(
        s.mcp(
            "card_update",
            json!({"project": PROJECT, "branch": "_project_board_", "target": "Goals:Ship.Design", "value": {
                "description": "ボードは書ける", "tags": ["design"]
            }})
        )
        .is_ok()
    );
    assert!(
        s.mcp(
            "card_create",
            json!({"project": PROJECT, "branch": "_project_board_", "target": "Goals:Memo", "value": {"kind": "note", "body": "x"}})
        )
        .is_ok()
    );

    s.set(json!({"group": "card", "state": "allow"}));
    s.set(json!({"group": "section", "state": "allow"}));
    s.set(json!({"item": "board", "state": "deny"}));
    let board = denied(&["board（ボードに書く）"]);
    assert_eq!(
        s.mcp(
            "card_update",
            json!({"project": PROJECT, "branch": "_project_board_", "target": "Goals:Ship.Design", "value": {
                "description": "書けない", "tags": [], "topLeft": {"x": 0, "y": 0}
            }})
        )
        .unwrap_err(),
        board
    );
    assert_eq!(
        s.mcp(
            "section_create",
            json!({"project": PROJECT, "branch": "_PROJECT_BOARD_", "target": "Goals/Later"})
        )
        .unwrap_err(),
        board
    );
    assert!(
        s.mcp(
            "card_update",
            json!({"project": PROJECT, "target": "Free:Memo", "value": {"description": "main は書ける"}})
        )
        .is_ok()
    );
    // Reading is not a write.
    assert!(
        s.mcp(
            "card_list",
            json!({"project": PROJECT, "branch": "_project_board_"})
        )
        .is_ok()
    );
}

#[test]
fn branches_requests_and_writing_out_are_items_of_their_own() {
    let s = store();
    for group in ["branch", "mr", "export"] {
        s.set(json!({"group": group, "state": "deny"}));
    }
    for (tool, arguments, name) in [
        (
            "branch_create",
            json!({"project": PROJECT, "target": "later"}),
            "branch を作る",
        ),
        (
            "branch_update",
            json!({"project": PROJECT, "target": "refunds", "value": {"name": "returns"}}),
            "branch の名前を変える",
        ),
        (
            "branch_delete",
            json!({"project": PROJECT, "target": "refunds", "value": {"discard": true}}),
            "branch を削除",
        ),
        (
            "branch_revert",
            json!({"project": PROJECT, "target": "main", "value": {"to": 0}}),
            "戻す",
        ),
        (
            "branch_merge",
            json!({"project": PROJECT, "target": "refunds"}),
            "merge",
        ),
        (
            "mr_delete",
            json!({"project": PROJECT, "target": "1"}),
            "MR を取り下げる",
        ),
        (
            "export_execute",
            json!({"targetdir": "[default]", "project": PROJECT}),
            "エクスポートする",
        ),
    ] {
        // Into the folder each export has by default, its item of its own too.
        let mut items = vec![format!("{tool}（{name}）")];
        if tool == "export_execute" {
            items.push("export.default（既定の先へエクスポートする）".to_string());
        }
        let items: Vec<&str> = items.iter().map(String::as_str).collect();
        assert_eq!(
            s.mcp(tool, arguments).unwrap_err(),
            denied(&items),
            "{tool}"
        );
    }
    // Into a locked main, a merge is a merge request: still branch_merge's.
    let locked = op(
        &s.core,
        json!({"op": "main_lock", "project": s.project, "on": true}),
    );
    assert!(locked.get("error").is_none(), "{locked}");
    assert_eq!(
        s.mcp(
            "branch_merge",
            json!({"project": PROJECT, "target": "refunds", "value": {"title": "返金"}})
        )
        .unwrap_err(),
        denied(&["branch_merge（merge）"])
    );
    // Let through, it is tried as a merge request is, and made.
    s.set(json!({"item": "branch_merge", "state": "allow"}));
    let made = s
        .mcp(
            "branch_merge",
            json!({"project": PROJECT, "target": "refunds", "value": {"title": "返金"}}),
        )
        .unwrap();
    assert_eq!(made["mr"], 1, "{made}");
    // Reading them is not writing them.
    assert!(s.mcp("branch_list", json!({"project": PROJECT})).is_ok());
    assert!(s.mcp("mr_list", json!({"project": PROJECT})).is_ok());
    assert!(
        s.mcp(
            "export_read",
            json!({"targetdir": "[default]", "project": PROJECT})
        )
        .is_ok()
    );
    // The window's own are its own.
    assert!(
        s.window_tool(
            "branch_create",
            json!({"project": PROJECT, "target": "later"})
        )
        .is_ok()
    );
}

#[test]
fn a_batch_is_held_to_it_step_by_step() {
    let s = store();
    s.set(json!({"item": "card_delete", "state": "deny"}));
    let opened = s
        .mcp(
            "batch",
            json!({"project": PROJECT, "value": {"open": true, "steps": [{"tool": "card_update", "arguments": {
                "target": "Schemas:Customer", "value": {"description": "バッチで"}
            }}]}}),
        )
        .unwrap();
    let token = opened["batch"].as_str().unwrap().to_string();
    assert!(
        opened["said"]
            .as_str()
            .unwrap()
            .starts_with("書いていません"),
        "{opened}"
    );
    assert_eq!(
        s.mcp(
            "batch",
            json!({"target": token, "value": {"steps": [{"tool": "card_delete", "arguments": {"target": "Free:Memo"}}]}})
        )
        .unwrap_err(),
        format!(
            "steps[0]（card_delete）は通りません: {}。積んでいません",
            denied(&[CARD_DELETE])
        )
    );
    assert_eq!(
        s.mcp(
            "batch",
            json!({"target": token, "value": {"steps": [
                {"tool": "card_update", "arguments": {"target": "Free:Memo", "value": {"body": "x"}}},
                {"tool": "card_delete", "arguments": {"target": "Free:Memo"}},
            ]}})
        )
        .unwrap_err(),
        format!(
            "steps[1]（card_delete）は通りません: {}。積んでいません",
            denied(&[CARD_DELETE])
        )
    );
    // Denied after it was staged: tried and run as the project says now.
    s.set(json!({"item": "card_update", "state": "deny"}));
    assert_eq!(
        s.mcp("batch", json!({"target": token})).unwrap_err(),
        format!(
            "step 0 (card_update) が通りません: {}。何も変えていません。value の remove で外すか、プロジェクトの今の状態に合わせて積み直してください",
            denied(&[CARD_UPDATE])
        )
    );
    assert_eq!(
        s.mcp("batch", json!({"target": token, "execute": true}))
            .unwrap_err(),
        format!(
            "step 0 (card_update) で失敗しました: {}。何も書いていません（すべて取り消しました）。バッチはそのまま残っています",
            denied(&[CARD_UPDATE])
        )
    );
    // Asked each time: tried, and said; run only when the run asks.
    s.set(json!({"item": "card_update", "state": "ask"}));
    let tried = s.mcp("batch", json!({"target": token})).unwrap();
    assert_eq!(
        tried["steps"][0]["warnings"],
        json!([format!(
            "{CARD_UPDATE} は都度承認です（execute: true に request: true を付けると、流すときに画面で許可を求めます）"
        )])
    );
    assert_eq!(
        s.mcp("batch", json!({"target": token, "execute": true}))
            .unwrap_err(),
        unrequested(&[CARD_UPDATE], "batch")
    );
    s.set(json!({"item": "card_update", "state": "allow"}));
    let ran = s
        .mcp("batch", json!({"target": token, "execute": true}))
        .unwrap();
    assert_eq!(ran["executed"], 1, "{ran}");
    assert_eq!(s.description("Schemas:Customer", "main"), "バッチで");

    // A batch in the board's tree: its steps are the board's.
    s.set(json!({"group": "card", "state": "deny"}));
    let board = s
        .mcp(
            "batch",
            json!({"project": PROJECT, "branch": "_project_board_", "value": {"open": true, "steps": [{"tool": "card_update", "arguments": {
                "target": "Goals:Ship.Design", "value": {"description": "ボードのバッチ"}
            }}]}}),
        )
        .unwrap();
    let token = board["batch"].as_str().unwrap();
    // The project's own settings in it are the project's: staged, and said.
    let staged = s
        .mcp(
            "batch",
            json!({"target": token, "value": {"steps": [{"tool": "project_update", "arguments": {"value": {"rules": {"project.description": "一文で"}}}}]}}),
        )
        .unwrap();
    assert_eq!(
        staged["steps"][0]["warnings"],
        json!([format!(
            "{RULES} は都度承認です（execute: true に request: true を付けると、流すときに画面で許可を求めます）"
        )])
    );
    s.mcp("batch", json!({"target": token, "value": {"remove": 1}}))
        .unwrap();
    assert!(
        s.mcp("batch", json!({"target": token, "execute": true}))
            .is_ok()
    );
}

#[test]
fn the_window_writes_whatever_it_writes() {
    let s = store();
    for group in [
        "project", "section", "card", "tag", "export", "branch", "mr", "board",
    ] {
        s.set(json!({"group": group, "state": "deny"}));
    }
    assert_eq!(s.not_allowed().len(), 27);
    for (tool, arguments) in [
        (
            "project_update",
            json!({"project": PROJECT, "value": {"name": "画面で", "rules": {"project.description": "x"}}}),
        ),
        (
            "card_update",
            json!({"project": "画面で", "target": "Schemas:Customer", "value": {"description": "画面", "tags": ["core"], "topLeft": {"x": 0, "y": 60}}}),
        ),
        (
            "card_update",
            // A goal's task by its path in the goal.
            json!({"project": "画面で", "branch": "_project_board_", "target": "Goals:Ship.Design", "value": {"description": "画面"}}),
        ),
        (
            "section_create",
            json!({"project": "画面で", "target": "Schemas/Later"}),
        ),
    ] {
        assert!(
            s.window_tool(tool, arguments.clone()).is_ok(),
            "{tool} {arguments}"
        );
    }
    // And MCP reads every item as denied.
    let read = s.mcp("project_read", json!({"project": "画面で"})).unwrap();
    assert_eq!(read["mcpDenied"].as_array().unwrap().len(), 27);
    assert_eq!(read["mcpDenied"][0], "project_update");
    assert!(read.get("mcpAsk").is_none());
}

#[test]
fn a_person_is_asked_when_a_call_requests_it() {
    let s = store();
    s.set(json!({"item": "card_update", "state": "ask"}));
    let write = |description: &str, request: bool| {
        let mut arguments = json!({"project": PROJECT, "target": "Schemas:Customer", "value": {"description": description}});
        if request {
            arguments["request"] = true.into();
        }
        s.mcp("card_update", arguments)
    };
    let before = s.description("Schemas:Customer", "main");
    // Not asked for: refused, saying it may be; no one asked.
    let person = s.person();
    assert_eq!(
        write("書かない", false).unwrap_err(),
        unrequested(&[CARD_UPDATE], "card_update")
    );
    assert!(person.asked().is_empty());

    // Once: through, and asked with who, what tool, what it is to do, and whose item.
    person.will(answer("once"));
    write("今回だけ", true).unwrap();
    assert_eq!(s.description("Schemas:Customer", "main"), "今回だけ");
    let asked = &person.asked()[0];
    assert_eq!(asked["tool"], "card_update");
    assert_eq!(asked["key"], common::KEY);
    assert_eq!(asked["seconds"], 120);
    assert_eq!(
        asked["items"],
        json!([{"id": "card_update", "name": "カードを変える", "whose": "project", "whoseId": s.project, "whoseName": PROJECT, "whoseSlug": s.slug()}])
    );
    assert_eq!(
        asked["what"],
        r#"{"target":"Schemas:Customer","value":{"description":"今回だけ"}}"#
    );
    assert_eq!(
        s.read_says().0,
        json!([
            "project_update.rules",
            "project_update.slug",
            "project_update.main_tree",
            "section_update.folder",
            "card_update",
            "export_create",
            "export_update",
            "export_delete",
            "export.dir",
            "export.default",
            "branch_revert.others"
        ])
    );

    // No: refused, nothing written, nothing changed.
    person.will(answer("deny"));
    assert_eq!(
        write("拒否", true).unwrap_err(),
        format!("人が拒否しました（{CARD_UPDATE}）。何もしていません")
    );
    assert_eq!(s.description("Schemas:Customer", "main"), "今回だけ");

    // Always: through, and allowed from now on, as the window of the Mac that answered sets it.
    person.will(answer("always"));
    write("常に", true).unwrap();
    assert_eq!(s.description("Schemas:Customer", "main"), "常に");
    assert_eq!(
        s.not_allowed(),
        [
            ("project_update.rules".to_string(), "ask".to_string()),
            ("project_update.slug".to_string(), "ask".to_string()),
            ("project_update.main_tree".to_string(), "ask".to_string()),
            ("section_update.folder".to_string(), "ask".to_string()),
            ("export_create".to_string(), "ask".to_string()),
            ("export_update".to_string(), "ask".to_string()),
            ("export_delete".to_string(), "ask".to_string()),
            ("export.dir".to_string(), "ask".to_string()),
            ("export.default".to_string(), "ask".to_string()),
            ("branch_revert.others".to_string(), "ask".to_string())
        ]
    );
    assert_eq!(s.lines(2)[1], "MCP の card_update を許可にする");
    write("もう聞かない", false).unwrap();
    assert_eq!(person.asked().len(), 3);

    // Never: refused, and denied from now on, saying where a person changes it; then refused
    // without asking.
    s.set(json!({"item": "card_update", "state": "ask"}));
    person.will(answer("never"));
    assert_eq!(
        write("二度と", true).unwrap_err(),
        format!(
            "人が拒否し、{CARD_UPDATE} をこれから拒否にしました（変えられるのは人だけで、アプリのプロジェクト設定の「MCP」のタブ）。何もしていません"
        )
    );
    assert_eq!(s.read_says().1, json!(["card_update"]));
    assert_eq!(write("二度と", true).unwrap_err(), denied(&[CARD_UPDATE]));
    assert_eq!(person.asked().len(), 4);

    // No answer in time; the window's own refusal; an answer not read.
    s.set(json!({"item": "card_update", "state": "ask"}));
    person.will(json!({"timeout": true}));
    assert_eq!(
        write("遅い", true).unwrap_err(),
        "2 分のうちに答えがありませんでした（求めは取り下げました）。何もしていません"
    );
    person.will(json!({"error": "このアプリは求めを受けません"}));
    assert_eq!(
        write("受けない", true).unwrap_err(),
        "このアプリは求めを受けません"
    );
    person.will(json!({"answer": "maybe"}));
    assert_eq!(
        write("読めない", true).unwrap_err(),
        r#"画面の答えが読めません: {"answer":"maybe"}"#
    );
    assert_ne!(s.description("Schemas:Customer", "main"), before);
    assert_eq!(s.description("Schemas:Customer", "main"), "もう聞かない");

    // No window to ask on.
    let alone = store();
    alone.set(json!({"item": "card_update", "state": "ask"}));
    assert_eq!(
        alone
            .mcp(
                "card_update",
                json!({"project": PROJECT, "target": "Schemas:Customer", "value": {"description": "x"}, "request": true})
            )
            .unwrap_err(),
        "許可を求める画面がありません（アプリが開いていません）。人に、アプリの設定で許可にしてもらうか、アプリを開いてから呼び直してください"
    );
}

#[test]
fn a_batch_run_asks_once_for_all_its_steps() {
    let s = store();
    let person = s.person();
    s.set(json!({"item": "card_update", "state": "ask"}));
    s.set(json!({"item": "card_create", "state": "ask"}));
    let opened = s
        .mcp(
            "batch",
            json!({"project": PROJECT, "value": {"open": true, "steps": [
                {"tool": "card_update", "arguments": {"target": "Schemas:Customer", "value": {"description": "一つ目"}}},
                {"tool": "card_create", "arguments": {"target": "Free:Asked", "value": {"kind": "note", "body": "x"}}},
                {"tool": "card_update", "arguments": {"target": "Free:Memo", "value": {"description": "三つ目"}}},
            ]}}),
        )
        .unwrap();
    let token = opened["batch"].as_str().unwrap().to_string();
    assert!(person.asked().is_empty(), "積むときは聞かない");
    assert_eq!(
        s.mcp("batch", json!({"target": token, "execute": true}))
            .unwrap_err(),
        unrequested(&[CARD_UPDATE, CARD_CREATE], "batch")
    );
    person.will(answer("once"));
    let ran = s
        .mcp(
            "batch",
            json!({"target": token, "execute": true, "request": true}),
        )
        .unwrap();
    assert_eq!(ran["executed"], 3, "{ran}");
    let asked = person.asked();
    assert_eq!(asked.len(), 1);
    assert_eq!(asked[0]["tool"], "batch");
    let ids: Vec<&str> = asked[0]["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["card_update", "card_create"]);
    assert_eq!(
        asked[0]["what"],
        [
            r#"0. card_update {"target":"Schemas:Customer","value":{"description":"一つ目"}}"#,
            r#"1. card_create {"target":"Free:Asked","value":{"kind":"note","body":"x"}}"#,
            r#"2. card_update {"target":"Free:Memo","value":{"description":"三つ目"}}"#,
        ]
        .join("\n")
    );
    assert_eq!(s.description("Schemas:Customer", "main"), "一つ目");
    // Said no: nothing of it written, and the batch kept.
    let again = s
        .mcp(
            "batch",
            json!({"project": PROJECT, "value": {"open": true, "steps": [{"tool": "card_update", "arguments": {"target": "Schemas:Customer", "value": {"description": "書かない"}}}]}}),
        )
        .unwrap();
    let token = again["batch"].as_str().unwrap().to_string();
    person.will(answer("deny"));
    assert_eq!(
        s.mcp(
            "batch",
            json!({"target": token, "execute": true, "request": true})
        )
        .unwrap_err(),
        format!("人が拒否しました（{CARD_UPDATE}）。何もしていません")
    );
    assert_eq!(s.description("Schemas:Customer", "main"), "一つ目");
    assert!(
        s.mcp("batch", json!({"target": token})).is_ok(),
        "バッチは残る"
    );
}

#[test]
fn the_setting_goes_out_with_the_projects_file_and_is_not_read_back_in() {
    let s = store();
    let defaults = s.not_allowed();
    s.set(json!({"item": "card_delete", "state": "deny"}));
    s.set(json!({"item": "project_update.rules", "state": "allow"}));
    let text = op(&s.core, json!({"op": "project_file", "project": s.project}))["result"]["text"]
        .as_str()
        .unwrap()
        .to_string();
    let file: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(
        file["project"]["mcp_writes"],
        json!({"project_update.rules": "allow", "card_delete": "deny"})
    );
    // What MCP may write without asking is the store's: a file read in could hand itself any.
    let copy = op(
        &s.core,
        json!({"op": "project_file_read", "text": text, "landing": "copy", "name": "写し"}),
    );
    let copy = copy["result"]["project"].as_str().unwrap().to_string();
    let view = op(&s.core, json!({"op": "mcp_writes", "project": copy}))["result"].clone();
    assert_eq!(not_allowed_in(&view), defaults);
}

#[test]
fn project_update_renames_the_project_and_sets_its_rules() {
    let s = store();
    let slug = |s: &Store| -> String {
        rusqlite::Connection::open(s.core.place())
            .unwrap()
            .query_row(
                "SELECT slug FROM projects WHERE id = ?",
                [&s.project],
                |r| r.get(0),
            )
            .unwrap()
    };
    let was = slug(&s);
    let renamed = s
        .mcp(
            "project_update",
            json!({"project": PROJECT, "value": {"name": "  改めた名前 "}}),
        )
        .unwrap();
    assert_eq!(renamed["project"], "改めた名前");
    assert_eq!(slug(&s), was, "slug は変えない");
    let commits = op(
        &s.core,
        json!({"op": "commits", "project": s.project, "limit": 1}),
    );
    assert_eq!(
        commits["result"]["commits"][0]["lines"][0]["summary"],
        "プロジェクト名を「改めた名前」に変更"
    );
    assert_eq!(
        s.mcp(
            "project_update",
            json!({"project": "改めた名前", "value": {"name": " "}})
        )
        .unwrap_err(),
        "プロジェクト名 を入力してください"
    );
    assert_eq!(
        s.mcp(
            "project_update",
            json!({"project": "改めた名前", "value": {"name": "あ".repeat(121)}})
        )
        .unwrap_err(),
        "プロジェクト名 は 120 文字以内です"
    );

    // The rules, once a person lets MCP write them.
    s.set(json!({"item": "project_update.rules", "state": "allow"}));
    let update = |rules: Value| {
        s.mcp(
            "project_update",
            json!({"project": "改めた名前", "value": {"rules": rules}}),
        )
    };
    let set = update(json!({
        "card.entity.description": "型の役目を一文で",
        "project.description": "プロジェクトの目的",
    }))
    .unwrap();
    assert_eq!(
        set["rules"],
        json!({"card.entity.description": "型の役目を一文で", "project.description": "プロジェクトの目的"})
    );
    // An empty text (or null) lets a place take the app's rule again; the rest stays.
    let set = update(json!({"project.description": ""})).unwrap();
    assert_eq!(
        set["rules"],
        json!({"card.entity.description": "型の役目を一文で"})
    );
    let set = update(json!({"card.entity.description": null})).unwrap();
    assert_eq!(set["rules"], json!({}));
    let read = s
        .mcp(
            "project_read",
            json!({"project": "改めた名前", "value": {"as": "rules", "place": "project.description"}}),
        )
        .unwrap();
    assert!(!read.to_string().contains("プロジェクトの目的"), "{read}");
    // Checked as the window's are.
    assert!(
        update(json!({"nope": "x"}))
            .unwrap_err()
            .starts_with("知らない書く場所です: nope。書ける場所は ")
    );
    assert_eq!(
        update(json!({"project.description": "あ".repeat(4001)})).unwrap_err(),
        "project.description の決まりが長すぎます（4000 文字まで）"
    );
    assert_eq!(
        update(json!({})).unwrap_err(),
        "rules に書く場所がありません。rules は {書く場所: 文} のオブジェクトです（空の文でアプリの既定に戻す。場所と今の決まりは project_read の as: rules）"
    );
    assert_eq!(
        update(json!({"project.description": 3})).unwrap_err(),
        "rules の project.description は文です（空の文でアプリの既定に戻す）"
    );
    // Said as the window's are: one line for the call.
    let commits = op(
        &s.core,
        json!({"op": "commits", "project": s.project, "limit": 1}),
    );
    assert_eq!(
        commits["result"]["commits"][0]["lines"][0]["summary"],
        "書き方の決まりを更新（0 か所）"
    );
    assert_eq!(
        s.mcp(
            "project_update",
            json!({"project": "改めた名前", "value": {}})
        )
        .unwrap_err(),
        "変更する項目がありません（name・description・layout・rules・project_tree・renames・slug）"
    );
}

#[test]
fn under_mains_lock_the_projects_settings_and_working_directories_still_go_through() {
    let s = store();
    let locked = op(
        &s.core,
        json!({"op": "main_lock", "project": s.project, "on": true}),
    );
    assert!(locked.get("error").is_none(), "{locked}");
    s.set(json!({"item": "project_update.rules", "state": "allow"}));
    // The project's own settings are not main's contents: the lock does not hold them.
    s.mcp(
        "project_update",
        json!({"project": PROJECT, "value": {"description": "ロック中の説明", "rules": {"card.description": "一文で言う"}}}),
    )
    .unwrap();
    s.mcp(
        "project_update",
        json!({"project": PROJECT, "value": {"name": "見本（ロック中）"}}),
    )
    .unwrap();
    // How a working directory is read is the project's setting too.
    let set = s
        .mcp(
            "section_update",
            json!({"project": "見本（ロック中）", "target": "ts", "value": {"readType": "basic", "description": "型"}}),
        )
        .unwrap();
    assert_eq!(
        (&set["readType"], &set["section"]),
        (&json!("basic"), &json!("ts")),
        "{set}"
    );
    // Its folder on this machine, as the item lets MCP bind one: asked each time by default.
    let said = s
        .mcp(
            "section_update",
            json!({"project": "見本（ロック中）", "target": "ts", "value": {"folder": "/tmp/nowhere"}}),
        )
        .unwrap_err();
    assert!(said.contains("section_update.folder"), "{said}");
    s.set(json!({"item": "section_update.folder", "state": "allow"}));
    let bound = s
        .mcp(
            "section_update",
            json!({"project": "見本（ロック中）", "target": "ts", "value": {"folder": "/tmp/nowhere"}}),
        )
        .unwrap();
    assert_eq!(bound["folder"], "/tmp/nowhere", "{bound}");
}

const MAIN_TREE: &str = "project_update.main_tree（ロック中の main の根の直下を変える）";

#[test]
fn under_mains_lock_the_roots_are_an_item_of_their_own() {
    // `project_update.main_tree`: main's sections directly under its root, no merge carries them —
    // under the lock, MCP changes them only as the item lets it; asked each time by default. The
    // window is not held to it.
    let s = store();
    let tree = |extra: &[(&str, &str)]| -> Value {
        let mut tree =
            s.mcp("project_read", json!({"project": PROJECT})).unwrap()["projectTree"].clone();
        for (kind, name) in extra {
            tree["main_sections"]
                .as_array_mut()
                .unwrap()
                .push(json!({"type": kind, "name": name}));
        }
        tree
    };
    let main_names = |s: &Store| -> Vec<String> {
        s.mcp("project_read", json!({"project": PROJECT})).unwrap()["projectTree"]["main_sections"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap().to_string())
            .collect()
    };
    // Not locked: nothing is asked, as it was.
    s.mcp(
        "project_update",
        json!({"project": PROJECT, "value": {"project_tree": tree(&[("free", "Early")])}}),
    )
    .unwrap();
    assert!(main_names(&s).contains(&"Early".to_string()));

    let locked = op(
        &s.core,
        json!({"op": "main_lock", "project": s.project, "on": true}),
    );
    assert!(locked.get("error").is_none(), "{locked}");
    let person = s.person();

    // Locked, asked each time: not said to be asked for, it is refused, saying it may be.
    let more = tree(&[("free", "Drafts")]);
    assert_eq!(
        s.mcp(
            "project_update",
            json!({"project": PROJECT, "value": {"project_tree": more.clone()}})
        )
        .unwrap_err(),
        unrequested(&[MAIN_TREE], "project_update")
    );
    assert!(person.asked().is_empty());
    // A check writes nothing and asks nothing; neither does a change of the board's alone.
    let checked = s
        .mcp(
            "project_update",
            json!({"project": PROJECT, "value": {"project_tree": more.clone(), "dryRun": true}}),
        )
        .unwrap();
    assert_eq!(checked["ok"], true, "{checked}");
    let mut board = tree(&[]);
    board["board_sections"]
        .as_array_mut()
        .unwrap()
        .push(json!({"type": "free", "name": "Notes"}));
    s.mcp(
        "project_update",
        json!({"project": PROJECT, "value": {"project_tree": board}}),
    )
    .unwrap();
    assert!(person.asked().is_empty());
    assert!(!main_names(&s).contains(&"Drafts".to_string()));

    // Asked, once: through, and the item is not allowed for it.
    person.will(answer("once"));
    s.mcp(
        "project_update",
        json!({"project": PROJECT, "value": {"project_tree": more}, "request": true}),
    )
    .unwrap();
    assert!(main_names(&s).contains(&"Drafts".to_string()));
    let asked = &person.asked()[0];
    assert_eq!(asked["tool"], "project_update");
    assert_eq!(asked["items"][0]["id"], "project_update.main_tree");
    assert_eq!(
        s.not_allowed()
            .iter()
            .filter(|(id, _)| id == "project_update.main_tree")
            .count(),
        1
    );

    // A rename is as much a change of them.
    let mut renamed = tree(&[]);
    for root in renamed["main_sections"].as_array_mut().unwrap() {
        if root["name"] == "Drafts" {
            root["name"] = "Sketches".into();
        }
    }
    let rename = json!({"project": PROJECT, "value": {"project_tree": renamed,
        "renames": [{"top_section": "Drafts", "to": "Sketches"}]}});
    assert_eq!(
        s.mcp("project_update", rename.clone()).unwrap_err(),
        unrequested(&[MAIN_TREE], "project_update")
    );

    // Denied: refused without asking, saying where a person changes it.
    s.set(json!({"item": "project_update.main_tree", "state": "deny"}));
    let mut request = rename.clone();
    request["request"] = true.into();
    assert_eq!(
        s.mcp("project_update", request.clone()).unwrap_err(),
        denied(&[MAIN_TREE])
    );
    assert_eq!(person.asked().len(), 1);

    // Allowed: through without asking.
    s.set(json!({"item": "project_update.main_tree", "state": "allow"}));
    s.mcp("project_update", rename).unwrap();
    assert!(main_names(&s).contains(&"Sketches".to_string()));
    assert_eq!(person.asked().len(), 1);

    // The window is not held to the item.
    s.set(json!({"item": "project_update.main_tree", "state": "deny"}));
    let mut more = tree(&[]);
    more["main_sections"]
        .as_array_mut()
        .unwrap()
        .push(json!({"type": "free", "name": "ByHand"}));
    s.window_tool(
        "project_update",
        json!({"project": PROJECT, "value": {"project_tree": more}}),
    )
    .unwrap();
    assert!(main_names(&s).contains(&"ByHand".to_string()));
}
