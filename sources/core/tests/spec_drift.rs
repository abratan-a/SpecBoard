//! An implementation's line remembers the code it reaches: drawn, it records the code's
//! fingerprint; when the code differs later, its card says so (`card_read`'s `codeReview`, the
//! check's `codeChanged`, the screen's reading) until it is reviewed (`card_action`'s `reviewed`),
//! which records the code again without a commit. What is not linked to code is listed by
//! `card_list`'s `implementation`.

use crate::common;

use common::{call, op, project_id};
use serde_json::{Value, json};
use specboard_core::Core;
use std::fs;
use std::path::{Path, PathBuf};

const PROJECT: &str = "見本";
const ORDER: &str = "Schemas/Ordering:Order";

fn ok(core: &Core, tool: &str, mut arguments: Value) -> Value {
    arguments["project"] = PROJECT.into();
    let (refused, text) = call(core, tool, arguments.clone());
    assert!(!refused, "{tool} {arguments}: {text}");
    serde_json::from_str(&text).unwrap()
}

fn refused(core: &Core, tool: &str, mut arguments: Value) -> String {
    arguments["project"] = PROJECT.into();
    let (refused, text) = call(core, tool, arguments.clone());
    assert!(refused, "{tool} {arguments} went through: {text}");
    text
}

/// A tool called as the window calls it.
fn window(core: &Core, tool: &str, mut arguments: Value) -> Value {
    arguments["project"] = PROJECT.into();
    let said = op(
        core,
        json!({"op": "tool", "name": tool, "arguments": arguments}),
    );
    assert!(said.get("error").is_none(), "{tool}: {said}");
    said["result"]["json"].clone()
}

const A_RS: &str = r#"/// An order.
pub struct Order {
    lines: Vec<u32>,
}

impl Order {
    pub fn new() -> Order {
        Order { lines: Vec::new() }
    }

    pub fn total(&self) -> u32 {
        self.lines.iter().sum::<u32>()
    }
}

fn helper(n: u32) -> u32 {
    n + 1
}

fn other(n: u32) -> u32 {
    n * 2
}
"#;

fn repository() -> PathBuf {
    let root = common::place("drift");
    for (path, text) in [("src/a.rs", A_RS), ("notes.txt", "a note\n")] {
        let file = root.join(path);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(file, text).unwrap();
    }
    root
}

/// The working directory `repo` listed in the project tree.
fn listed(core: &Core) {
    let mut tree = ok(core, "project_read", json!({}))["projectTree"].clone();
    tree["main_sections"]
        .as_array_mut()
        .unwrap()
        .push(json!({"type": "workdir", "name": "repo"}));
    ok(
        core,
        "project_update",
        json!({"value": {"project_tree": tree}}),
    );
}

/// The working directory bound to `folder` on this machine, or to none.
fn bind(core: &Core, folder: Option<&Path>) {
    window(
        core,
        "section_update",
        json!({"target": "repo", "value": {"folder": folder.map(|f| f.to_string_lossy().to_string())}}),
    );
}

fn core_with_repository() -> (Core, PathBuf) {
    let core = Core::open(common::script_store()).unwrap();
    let repo = repository();
    listed(&core);
    bind(&core, Some(&repo));
    (core, repo)
}

/// The file changed: `from` replaced with `to` in it.
fn edit(repo: &Path, from: &str, to: &str) {
    let file = repo.join("src/a.rs");
    let text = fs::read_to_string(&file).unwrap();
    assert!(text.contains(from), "{from}");
    fs::write(&file, text.replacen(from, to, 1)).unwrap();
}

fn draw(core: &Core, card: &str, lines: Value) -> Value {
    ok(
        core,
        "card_update",
        json!({"target": card, "value": {"connections": {"add": lines}}}),
    )
}

fn review_of(core: &Core, card: &str) -> Value {
    ok(core, "card_read", json!({"target": card}))["codeReview"].clone()
}

fn recorded(core: &Core) -> Vec<(String, String, String)> {
    let conn = rusqlite::Connection::open(core.place()).unwrap();
    let mut stmt = conn
        .prepare("SELECT file_id, symbol, digest FROM code_marks ORDER BY symbol")
        .unwrap();
    stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

fn commits(core: &Core) -> i64 {
    rusqlite::Connection::open(core.place())
        .unwrap()
        .query_row("SELECT COUNT(*) FROM commits", [], |r| r.get(0))
        .unwrap()
}

fn group(core: &Core, name: &str) -> Vec<Value> {
    let said = ok(
        core,
        "inspection_list",
        json!({"value": {"group": name, "show": "all"}}),
    );
    said["items"].as_array().cloned().unwrap_or_default()
}

#[test]
fn a_line_drawn_records_its_code_and_says_nothing_while_it_is_the_same() {
    let (core, _repo) = core_with_repository();
    draw(
        &core,
        ORDER,
        json!([
            {"line": "implementedBy", "to": "repo/src:'a.rs'.helper"},
            {"line": "implementedBy", "from": "total", "to": "repo/src:'a.rs'.Order.total"},
            {"line": "implementedBy", "from": "status", "to": "repo:'notes.txt'"}
        ]),
    );
    let kept = recorded(&core);
    assert_eq!(kept.len(), 3, "{kept:?}");
    assert!(
        kept.iter().all(|(_, _, d)| d.starts_with("w1:")),
        "{kept:?}"
    );
    assert_eq!(kept[0].1, "", "the whole file: {kept:?}");
    assert_eq!(review_of(&core, ORDER), Value::Null);
    assert!(group(&core, "codeChanged").is_empty());
}

#[test]
fn the_code_changed_marks_the_card_until_it_is_reviewed_without_a_commit() {
    let (core, repo) = core_with_repository();
    draw(
        &core,
        ORDER,
        json!([
            {"line": "implementedBy", "to": "repo/src:'a.rs'.helper"},
            {"line": "implementedBy", "from": "total", "to": "repo/src:'a.rs'.Order.total"}
        ]),
    );
    edit(&repo, "n + 1", "n + 2");
    let review = review_of(&core, ORDER);
    let items = review.as_array().unwrap();
    assert_eq!(items.len(), 1, "{review}");
    assert_eq!(items[0]["line"], "implementedBy");
    assert_eq!(items[0]["to"], "repo/src:'a.rs'.helper");
    assert_eq!(items[0]["state"], "changed");
    assert!(items[0]["recordedAt"].is_string(), "{review}");
    assert!(items[0].get("from").is_none(), "{review}");
    // The check and the screen say it too.
    let found = group(&core, "codeChanged");
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0]["at"], ORDER);
    let detail = op(
        &core,
        json!({"op": "project_detail", "project": project_id(&core)}),
    )["result"]
        .clone();
    let order = detail["schemas"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "Order")
        .unwrap();
    assert_eq!(order["codeReview"][0]["state"], "changed", "{order}");
    assert!(order["codeReview"][0]["id"].is_string(), "{order}");
    // The action is offered on an entity, as its line asks.
    let actions = ok(
        &core,
        "card_read",
        json!({"target": ORDER, "value": {"include": ["actions"]}}),
    )["actions"]
        .clone();
    assert_eq!(actions[0]["op"], "reviewed", "{actions}");
    assert_eq!(actions[0]["available"], true, "{actions}");
    // Reviewed: recorded again, no commit.
    let before = commits(&core);
    let done = ok(
        &core,
        "card_action",
        json!({"target": ORDER, "op": "reviewed"}),
    );
    assert_eq!(
        done["recorded"],
        json!([{"to": "repo/src:'a.rs'.helper"}]),
        "{done}"
    );
    assert_eq!(commits(&core), before);
    assert_eq!(review_of(&core, ORDER), Value::Null);
    assert!(group(&core, "codeChanged").is_empty());
    // Nothing more to review: refused, and said why.
    let said = refused(
        &core,
        "card_action",
        json!({"target": ORDER, "op": "reviewed"}),
    );
    assert!(said.contains("reviewed") || said.contains("見直"), "{said}");
}

#[test]
fn spaces_another_function_and_a_move_change_nothing_and_a_word_does() {
    let (core, repo) = core_with_repository();
    draw(
        &core,
        ORDER,
        json!([
            {"line": "implementedBy", "to": "repo/src:'a.rs'.helper"},
            {"line": "implementedBy", "from": "id", "to": "repo/src:'a.rs'.Order"}
        ]),
    );
    // Re-indented, another function changed, and the function moved.
    edit(&repo, "    n + 1\n", "        n   +   1\n");
    edit(&repo, "n * 2", "n * 3");
    let text = fs::read_to_string(repo.join("src/a.rs")).unwrap();
    let helper = "fn helper(n: u32) -> u32 {\n        n   +   1\n}\n";
    let moved = format!("{helper}\n{}", text.replacen(helper, "", 1));
    fs::write(repo.join("src/a.rs"), moved).unwrap();
    assert_eq!(review_of(&core, ORDER), Value::Null);
    // A type is the blocks of its own too: a method changed changes it.
    edit(&repo, "sum::<u32>()", "sum::<u32>() + 1");
    let review = review_of(&core, ORDER);
    assert_eq!(review.as_array().unwrap().len(), 1, "{review}");
    assert_eq!(review[0]["to"], "repo/src:'a.rs'.Order");
    assert_eq!(review[0]["from"], "id");
}

#[test]
fn a_line_not_recorded_is_said_without_a_mark_and_reviewed_records_it() {
    let core = Core::open(common::script_store()).unwrap();
    let repo = repository();
    listed(&core);
    // Drawn while no folder is bound here: nothing is recorded.
    draw(
        &core,
        ORDER,
        json!([
            {"line": "implementedBy", "to": "repo/src:'a.rs'.helper"},
            {"line": "implementedBy", "from": "total", "to": "repo/src:'a.rs'.missing"}
        ]),
    );
    assert!(recorded(&core).is_empty());
    assert_eq!(review_of(&core, ORDER), Value::Null);
    bind(&core, Some(&repo));
    let review = review_of(&core, ORDER);
    assert_eq!(review.as_array().unwrap().len(), 1, "{review}");
    assert_eq!(review[0]["state"], "unrecorded");
    assert!(group(&core, "codeChanged").is_empty());
    let listed = ok(
        &core,
        "card_list",
        json!({"value": {"implementation": "unrecorded"}}),
    );
    assert_eq!(listed["cards"][0]["path"], ORDER, "{listed}");
    assert_eq!(listed["cards"][0]["codeReview"][0]["state"], "unrecorded");
    let done = ok(
        &core,
        "card_action",
        json!({"target": ORDER, "op": "reviewed"}),
    );
    assert_eq!(recorded(&core).len(), 1);
    // The one not in the file is not read: said, not recorded.
    assert_eq!(done["notRead"][0]["from"], "total", "{done}");
    assert!(done["notRead"][0]["why"].is_string(), "{done}");
    assert_eq!(review_of(&core, ORDER), Value::Null);
}

#[test]
fn a_line_drawn_on_a_branch_keeps_its_record_through_the_merge() {
    let (core, repo) = core_with_repository();
    ok(&core, "branch_create", json!({"target": "work"}));
    ok(
        &core,
        "card_update",
        json!({"branch": "work", "target": ORDER, "value": {"connections": {"add": [
            {"line": "implementedBy", "to": "repo/src:'a.rs'.helper"}
        ]}}}),
    );
    let first = recorded(&core);
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].1, "helper");
    ok(&core, "branch_merge", json!({"target": "work"}));
    assert_eq!(recorded(&core), first);
    assert_eq!(review_of(&core, ORDER), Value::Null);
    // Drawn to another function instead: recorded for it, and the one let go of says nothing.
    ok(
        &core,
        "card_update",
        json!({"target": ORDER, "value": {"connections": {
            "remove": [{"line": "implementedBy", "to": "repo/src:'a.rs'.helper"}],
            "add": [{"line": "implementedBy", "to": "repo/src:'a.rs'.other"}]
        }}}),
    );
    let now = recorded(&core);
    assert!(now.iter().any(|(_, s, _)| s == "other"), "{now:?}");
    edit(&repo, "n + 1", "n + 9");
    assert_eq!(review_of(&core, ORDER), Value::Null);
}

#[test]
fn reviewed_passes_a_locked_main_and_a_machine_without_the_folder_says_nothing() {
    let (core, repo) = core_with_repository();
    draw(
        &core,
        ORDER,
        json!([{"line": "implementedBy", "to": "repo/src:'a.rs'.helper"}]),
    );
    edit(&repo, "n + 1", "n - 1");
    let locked = op(
        &core,
        json!({"op": "main_lock", "project": project_id(&core), "on": true}),
    );
    assert!(locked.get("error").is_none(), "{locked}");
    ok(
        &core,
        "card_action",
        json!({"target": ORDER, "op": "reviewed"}),
    );
    assert_eq!(review_of(&core, ORDER), Value::Null);
    // The folder let go of on this machine: the record stays, nothing is said.
    edit(&repo, "n - 1", "n - 2");
    bind(&core, None);
    assert_eq!(review_of(&core, ORDER), Value::Null);
    assert_eq!(recorded(&core).len(), 1);
    bind(&core, Some(&repo));
    assert_eq!(review_of(&core, ORDER)[0]["state"], "changed");
}

#[test]
fn what_is_not_linked_to_code_is_listed_with_the_rows_left() {
    let (core, repo) = core_with_repository();
    draw(
        &core,
        ORDER,
        json!([
            {"line": "implementedBy", "from": "id", "to": "repo/src:'a.rs'.helper"},
            {"line": "implementedBy", "from": "total", "to": "repo/src:'a.rs'.Order"}
        ]),
    );
    let none = ok(
        &core,
        "card_list",
        json!({"value": {"section": "Schemas/Ordering", "implementation": "none", "limit": 200}}),
    );
    let order = none["cards"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["path"] == ORDER)
        .unwrap_or_else(|| panic!("{none}"))
        .clone();
    let unlinked: Vec<&str> = order["unlinked"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r.as_str().unwrap())
        .collect();
    assert!(unlinked.contains(&"status"), "{order}");
    assert!(unlinked.contains(&"shipping"), "{order}");
    assert!(
        !unlinked.contains(&"id") && !unlinked.contains(&"total"),
        "{order}"
    );
    // A card none leaves is listed bare.
    assert!(
        none["cards"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["path"] != ORDER && c.get("unlinked").is_none()),
        "{none}"
    );
    // The functions and types no line reaches: those inside a type a line reaches are reached.
    let symbols = ok(
        &core,
        "card_list",
        json!({"value": {"kind": "symbol", "section": "repo", "implementation": "none"}}),
    );
    let paths: Vec<&str> = symbols["cards"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["path"].as_str().unwrap())
        .collect();
    assert_eq!(paths, ["repo/src:'a.rs'.other"], "{symbols}");
    // The files no line reaches.
    let files = ok(
        &core,
        "card_list",
        json!({"value": {"kind": "file", "implementation": "none"}}),
    );
    let paths: Vec<&str> = files["cards"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["path"].as_str().unwrap())
        .collect();
    assert_eq!(paths, ["repo:'notes.txt'"], "{files}");
    // Changed: the card, the file and the function, each with its lines.
    edit(&repo, "n + 1", "n + 3");
    let changed = ok(
        &core,
        "card_list",
        json!({"value": {"implementation": ["changed", "unrecorded"]}}),
    );
    assert_eq!(changed["matched"], 1, "{changed}");
    assert_eq!(changed["cards"][0]["codeReview"][0]["from"], "id");
    let file = ok(
        &core,
        "card_list",
        json!({"value": {"kind": "file", "implementation": "changed"}}),
    );
    assert_eq!(file["cards"][0]["path"], "repo/src:'a.rs'", "{file}");
    assert_eq!(
        file["cards"][0]["codeReview"][0]["from"],
        format!("{ORDER}.id")
    );
    let symbol = ok(
        &core,
        "card_list",
        json!({"value": {"kind": "symbol", "section": "repo", "implementation": "changed"}}),
    );
    assert_eq!(symbol["matched"], 1, "{symbol}");
    assert_eq!(symbol["cards"][0]["path"], "repo/src:'a.rs'.helper");
    let wrong = refused(
        &core,
        "card_list",
        json!({"value": {"implementation": "linked"}}),
    );
    assert!(wrong.contains("implementation"), "{wrong}");
}

#[test]
fn a_project_file_carries_the_records_of_the_lines_it_holds() {
    let (core, _repo) = core_with_repository();
    draw(
        &core,
        ORDER,
        json!([{"line": "implementedBy", "to": "repo/src:'a.rs'.helper"}]),
    );
    let text = op(
        &core,
        json!({"op": "project_file", "project": project_id(&core)}),
    )["result"]["text"]
        .as_str()
        .unwrap()
        .to_string();
    let file: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(file["version"], 28);
    let marks = file["codeMarks"].as_array().unwrap();
    assert_eq!(marks.len(), 1, "{marks:?}");
    assert_eq!(marks[0]["symbol"], "helper");
    let kept = recorded(&core);
    // Read into another store: the same records.
    let place = common::place("drift-file.sqlite");
    let other = Core::create(&place).unwrap();
    let read = op(&other, json!({"op": "project_file_read", "text": text}));
    assert!(read.get("error").is_none(), "{read}");
    assert_eq!(recorded(&other), kept);
}
