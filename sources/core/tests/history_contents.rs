//! One commit read and undone through MCP (`specboard://SpecBoard/Docs/Design:DesignBranches` §7):
//! `branch_read` with `as: history` and `seq` says what the commit changed, unit by unit, before and
//! after, in the shape card_read gives — so what a person lost by a slip can be written again; and
//! `branch_revert` with `undo` takes back that one commit alone, three ways as a merge does, refusing
//! the whole of it where a later commit changed the same unit. Another writer's commit is undone only
//! when a person allows it (`branch_revert.others`, asked each time by default), one named commit at a
//! time; main is never written by an undo, which goes in through a branch and a merge.

use crate::common;

use common::{PROJECT, call, ok, op, project_id, refused, script_store};
use serde_json::{Value, json};
use specboard_core::{App, Core};
use std::sync::{Arc, Mutex};

/// A window that answers what it is asked as it was told to, and keeps what it was asked.
#[derive(Default)]
struct Person {
    answers: Mutex<Vec<Value>>,
    asked: Mutex<Vec<Value>>,
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

/// A tool called as the window calls it: its answer.
fn window(core: &Core, tool: &str, mut arguments: Value) -> Value {
    arguments["project"] = PROJECT.into();
    let said = op(
        core,
        json!({"op": "tool", "name": tool, "arguments": arguments}),
    );
    assert!(said.get("error").is_none(), "{tool}: {said}");
    said["result"]["json"].clone()
}

/// The newest commit of a tree, as MCP lists it.
fn newest(core: &Core, tree: &str) -> Value {
    ok(
        core,
        "branch_read",
        json!({"target": tree, "value": {"as": "history", "limit": 1}}),
    )["commits"][0]
        .clone()
}

fn seq_of(core: &Core, tree: &str) -> i64 {
    newest(core, tree)["seq"].as_i64().unwrap()
}

/// How many commits a tree has.
fn count(core: &Core, tree: &str) -> usize {
    ok(
        core,
        "branch_read",
        json!({"target": tree, "value": {"as": "history", "limit": 1000}}),
    )["commits"]
        .as_array()
        .unwrap()
        .len()
}

/// One commit read through MCP.
fn contents(core: &Core, tree: &str, value: Value) -> Value {
    let mut value = value;
    value["as"] = "history".into();
    ok(core, "branch_read", json!({"target": tree, "value": value}))
}

fn card(core: &Core, tree: &str, path: &str) -> Value {
    ok(core, "card_read", json!({"branch": tree, "target": path}))
}

/// A flow whose decision has a line taken by a condition, as a person writes it.
fn make_pay(core: &Core) {
    ok(
        core,
        "card_create",
        json!({"value": {
            "Charts/Shop:Pay": {"kind": "flow", "label": "支払う"},
            "Charts/Shop:Pay.start": {"kind": "begin", "trigger": "manual", "connections": [{"line": "next", "to": "decide"}]},
            "Charts/Shop:Pay.decide": {"kind": "eval", "value": "Schemas/Ordering:Order.status",
                "connections": [
                    {"line": "branch", "to": "done", "fields": {"when": "書類がそろっていない"}},
                    {"line": "else", "to": "failed"}
                ]},
            "Charts/Shop:Pay.done": {"kind": "end", "outcome": "success"},
            "Charts/Shop:Pay.failed": {"kind": "end", "outcome": "failure"}
        }}),
    );
}

#[test]
fn a_line_a_person_deleted_by_a_slip_can_be_read_back_with_its_condition() {
    let core = Core::open(script_store()).unwrap();
    make_pay(&core);
    window(
        &core,
        "card_update",
        json!({"target": "Charts/Shop:Pay.decide", "value": {"connections": {"remove": [{"line": "branch", "to": "Charts/Shop:Pay.done"}]}}}),
    );
    let seq = seq_of(&core, "main");
    let read = contents(&core, "main", json!({"seq": seq}));
    assert_eq!(read["branch"], "main");
    assert_eq!(read["commit"]["seq"], seq);
    assert_eq!(read["commit"]["author"], "画面");
    assert_eq!(
        read["changes"],
        json!([{
            "path": "Charts/Shop:Pay.decide",
            "change": "change",
            "units": [{
                "unit": "connections.branch",
                "before": {"line": "branch", "to": "Charts/Shop:Pay.done", "fields": {"when": "書類がそろっていない"}},
                "after": null
            }]
        }]),
        "{read}"
    );
}

#[test]
fn what_a_commit_made_removed_or_renamed_is_said_whole_or_by_its_path_before() {
    let core = Core::open(script_store()).unwrap();
    ok(
        &core,
        "card_create",
        json!({"target": "Schemas:Voucher", "value": {"kind": "entity", "label": "券", "description": "割引の券"}}),
    );
    let made = seq_of(&core, "main");
    let read = contents(&core, "main", json!({"seq": made}));
    let change = &read["changes"][0];
    assert_eq!(change["path"], "Schemas:Voucher");
    assert_eq!(change["change"], "add");
    assert_eq!(change["after"]["kind"], "entity");
    assert_eq!(change["after"]["label"], "券");
    assert_eq!(change["after"]["description"], "割引の券");
    assert_eq!(change["after"]["section"], "Schemas");
    assert!(change.get("before").is_none());
    // What a later commit wrote of it too: an undo of the first meets it.
    ok(
        &core,
        "card_update",
        json!({"target": "Schemas:Voucher", "value": {"name": "Coupon"}}),
    );
    let renamed = seq_of(&core, "main");
    let read = contents(&core, "main", json!({"seq": made}));
    assert_eq!(read["changes"][0]["later"], json!([renamed]));
    let read = contents(&core, "main", json!({"seq": renamed}));
    assert_eq!(read["changes"][0]["path"], "Schemas:Coupon");
    assert_eq!(read["changes"][0]["from"], "Schemas:Voucher");
    assert_eq!(read["changes"][0]["units"][0]["unit"], "name");
    ok(&core, "card_delete", json!({"target": "Schemas:Coupon"}));
    let read = contents(&core, "main", json!({"seq": seq_of(&core, "main")}));
    assert_eq!(read["changes"][0]["change"], "remove");
    assert_eq!(read["changes"][0]["path"], "Schemas:Coupon");
    assert_eq!(read["changes"][0]["before"]["label"], "券");
    // A section's keys, sections before cards; asked by a path, that change alone.
    ok(
        &core,
        "batch",
        json!({"value": {"steps": [
            {"tool": "section_update", "arguments": {"target": "Charts", "value": {"description": "図の置き場"}}},
            {"tool": "card_update", "arguments": {"target": "Schemas:Customer", "value": {"label": "お客"}}}
        ]}, "execute": true}),
    );
    let both = seq_of(&core, "main");
    let read = contents(&core, "main", json!({"seq": both}));
    let paths: Vec<&str> = read["changes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["path"].as_str().unwrap())
        .collect();
    assert_eq!(paths, ["Charts", "Schemas:Customer"], "{read}");
    assert_eq!(read["changes"][0]["section"], true);
    assert_eq!(read["changes"][0]["units"][0]["unit"], "description");
    assert_eq!(read["changes"][0]["units"][0]["after"], "図の置き場");
    let one = contents(
        &core,
        "main",
        json!({"seq": both, "path": "Schemas:Customer"}),
    );
    assert_eq!(one["changes"].as_array().unwrap().len(), 1);
    assert_eq!(one["changes"][0]["units"][0]["after"], "お客");
    let not = refused(
        &core,
        "branch_read",
        json!({"target": "main", "value": {"as": "history", "seq": both, "path": "Schemas:Order"}}),
    );
    assert!(not.contains("Schemas:Order"), "{not}");
}

#[test]
fn a_long_text_changed_in_one_line_is_said_by_that_line_unless_asked_whole() {
    let core = Core::open(script_store()).unwrap();
    let lines: Vec<String> = (1..=200)
        .map(|n| format!("{n} 行目の文。説明を足して長くする"))
        .collect();
    let body = lines.join("\n");
    ok(
        &core,
        "card_create",
        json!({"target": "Docs:Long", "value": {"kind": "freedoc", "body": body}}),
    );
    let changed = body.replace("100 行目の文。", "100 行目を直した。");
    ok(
        &core,
        "card_update",
        json!({"target": "Docs:Long", "value": {"body": changed}}),
    );
    let seq = seq_of(&core, "main");
    let read = contents(&core, "main", json!({"seq": seq}));
    assert_eq!(
        read["changes"][0]["units"],
        json!([{"unit": "body", "key": "body", "lines": [{"line": 100, "before": "100 行目の文。説明を足して長くする", "after": "100 行目を直した。説明を足して長くする"}]}]),
        "{read}"
    );
    let whole = contents(&core, "main", json!({"seq": seq, "full": true}));
    assert_eq!(whole["changes"][0]["units"][0]["before"], body);
    assert_eq!(whole["changes"][0]["units"][0]["after"], changed);
}

#[test]
fn a_commit_over_an_answers_size_is_read_on_from_the_path_it_stopped_at() {
    let core = Core::open(script_store()).unwrap();
    let text = "長い本文。".repeat(600);
    let made: serde_json::Map<String, Value> = (0..30)
        .map(|n| {
            (
                format!("Free:Memo{n:02}"),
                json!({"kind": "note", "body": format!("{n} {text}")}),
            )
        })
        .collect();
    ok(&core, "card_create", json!({"value": made}));
    let seq = seq_of(&core, "main");
    let first = contents(&core, "main", json!({"seq": seq}));
    assert_eq!(first["truncated"], true, "{}", first["changes"]);
    assert_eq!(first["matched"], 30);
    let shown = first["changes"].as_array().unwrap().len();
    assert!(shown < 30);
    assert_eq!(first["next"], first["changes"][shown - 1]["path"]);
    let rest = contents(&core, "main", json!({"seq": seq, "after": first["next"]}));
    assert!(rest.get("truncated").is_none() || rest["truncated"] == true);
    assert_eq!(
        rest["changes"][0]["path"],
        format!("Free:Memo{shown:02}"),
        "{}",
        rest["changes"][0]["path"]
    );
    // One commit's contents, not a count of commits; a number no commit has.
    let both = refused(
        &core,
        "branch_read",
        json!({"target": "main", "value": {"as": "history", "seq": seq, "limit": 5}}),
    );
    assert!(both.contains("limit"), "{both}");
    let none = refused(
        &core,
        "branch_read",
        json!({"target": "main", "value": {"as": "history", "seq": 99999}}),
    );
    assert!(none.contains("99999"), "{none}");
}

#[test]
fn the_boards_commits_are_read_and_undone_as_the_boards_writes() {
    let core = Core::open(script_store()).unwrap();
    let board = "_project_board_";
    ok(
        &core,
        "card_create",
        json!({"branch": board, "target": "Goals:Check", "value": {"kind": "task", "label": "確かめる", "body": "前"}}),
    );
    ok(
        &core,
        "card_update",
        json!({"branch": board, "target": "Goals:Check", "value": {"body": "後"}}),
    );
    let seq = seq_of(&core, board);
    let read = contents(&core, board, json!({"seq": seq}));
    assert_eq!(read["branch"], board);
    assert_eq!(read["changes"][0]["path"], "Goals:Check");
    // The board's item says whether MCP may: not the branches'.
    let project = project_id(&core);
    let set = |item: &str, state: &str| {
        let said = op(
            &core,
            json!({"op": "mcp_writes_set", "project": project, "item": item, "state": state}),
        );
        assert!(said.get("error").is_none(), "{said}");
    };
    set("board", "deny");
    let denied = refused(
        &core,
        "branch_revert",
        json!({"target": board, "value": {"undo": seq}}),
    );
    assert!(denied.contains("board"), "{denied}");
    set("board", "allow");
    set("branch_revert", "deny");
    let undone = ok(
        &core,
        "branch_revert",
        json!({"target": board, "value": {"undo": seq}}),
    );
    assert_eq!(undone["undone"], seq);
    assert_eq!(card(&core, board, "Goals:Check")["body"], "前");
}

#[test]
fn undoing_ones_own_commit_keeps_what_later_commits_changed_elsewhere() {
    let core = Core::open(script_store()).unwrap();
    ok(&core, "branch_create", json!({"target": "design"}));
    let was = card(&core, "design", "Schemas:Customer");
    ok(
        &core,
        "card_update",
        json!({"branch": "design", "target": "Schemas:Customer", "value": {"description": "設計が変えた"}}),
    );
    let first = seq_of(&core, "design");
    ok(
        &core,
        "card_update",
        json!({"branch": "design", "target": "Schemas:Customer", "value": {"label": "あとの名"}}),
    );
    let before = count(&core, "design");
    let undone = ok(
        &core,
        "branch_revert",
        json!({"target": "design", "value": {"undo": first}}),
    );
    assert_eq!(undone["undone"], first);
    assert_eq!(undone["changed"], json!(["Schemas:Customer"]));
    let now = card(&core, "design", "Schemas:Customer");
    assert_eq!(now["description"], was["description"]);
    assert_eq!(now["label"], "あとの名");
    // One commit of its own, saying what it undid; the one undone marked.
    assert_eq!(count(&core, "design"), before + 1);
    let commit = newest(&core, "design");
    assert_eq!(commit["seq"], undone["commit"]);
    assert_eq!(commit["kind"], "revert");
    assert_eq!(
        commit["lines"][0],
        format!("コミット {first}（MCP・{}）の変更を取り消す", common::KEY)
    );
    let listed = ok(
        &core,
        "branch_read",
        json!({"target": "design", "value": {"as": "history", "limit": 1000}}),
    );
    let marked = listed["commits"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["seq"] == first)
        .unwrap();
    assert_eq!(marked["reverted"], true);
}

#[test]
fn a_unit_a_later_commit_changed_too_refuses_the_whole_undo_and_writes_nothing() {
    let core = Core::open(script_store()).unwrap();
    ok(&core, "branch_create", json!({"target": "design"}));
    ok(
        &core,
        "card_update",
        json!({"branch": "design", "target": "Schemas:Customer", "value": {"description": "一度目", "label": "名"}}),
    );
    let first = seq_of(&core, "design");
    ok(
        &core,
        "card_update",
        json!({"branch": "design", "target": "Schemas:Customer", "value": {"description": "二度目"}}),
    );
    let second = seq_of(&core, "design");
    let before = count(&core, "design");
    let said = refused(
        &core,
        "branch_revert",
        json!({"target": "design", "value": {"undo": first}}),
    );
    let shown: Value = serde_json::from_str(&said[said.find('{').unwrap()..]).unwrap();
    assert_eq!(shown["later"], json!([second]), "{said}");
    let conflict = &shown["conflicts"][0];
    assert_eq!(conflict["path"], "Schemas:Customer");
    assert_eq!(conflict["unit"], "description");
    assert_eq!(conflict["after"], "一度目");
    assert_eq!(conflict["now"], "二度目");
    assert_eq!(shown["conflicts"].as_array().unwrap().len(), 1);
    assert_eq!(count(&core, "design"), before);
    assert_eq!(card(&core, "design", "Schemas:Customer")["label"], "名");
}

#[test]
fn a_persons_commit_is_undone_only_when_a_person_allows_it_one_at_a_time() {
    let core = Core::open(script_store()).unwrap();
    let project = project_id(&core);
    window(
        &core,
        "card_update",
        json!({"target": "Schemas:Customer", "value": {"label": "人が変えた"}}),
    );
    let theirs = seq_of(&core, "main");
    // Asked each time by default: refused without `request`, pointing at it.
    let said = refused(
        &core,
        "branch_revert",
        json!({"target": "main", "value": {"undo": theirs}}),
    );
    assert!(said.contains("branch_revert.others"), "{said}");
    assert!(said.contains("request: true"), "{said}");
    // A range across it stays refused, and says how to undo it.
    let range = refused(
        &core,
        "branch_revert",
        json!({"target": "main", "value": {"to": theirs - 1}}),
    );
    assert!(range.contains("undo"), "{range}");
    // Asked: the window is shown which commit, and the undo says it was allowed.
    let person = Arc::new(Person::default());
    core.set_app(person.clone());
    person
        .answers
        .lock()
        .unwrap()
        .push(json!({"answer": "once", "client": "MacBook"}));
    let (refused_now, text) = call(
        &core,
        "branch_revert",
        json!({"project": PROJECT, "target": "main", "value": {"undo": theirs}, "request": true}),
    );
    assert!(!refused_now, "{text}");
    let done: Value = serde_json::from_str(&text).unwrap();
    let made = done["made"].as_str().unwrap().to_string();
    assert_eq!(made, format!("undo-{theirs}"));
    let asked = person.asked.lock().unwrap().clone();
    let what = asked[0]["what"].as_str().unwrap();
    assert!(
        what.contains(&format!("コミット {theirs}（画面、")),
        "{what}"
    );
    assert!(what.contains("Schemas:Customer"), "{what}");
    assert!(
        newest(&core, &made)["lines"]
            .as_array()
            .unwrap()
            .contains(&json!(format!("main のコミット {theirs}（画面）の変更を取り消した形（画面で許可を得て。merge で main に入る）"))),
        "{}",
        newest(&core, &made)
    );
    // main untouched until the branch is merged.
    assert_eq!(
        card(&core, "main", "Schemas:Customer")["label"],
        "人が変えた"
    );
    assert_ne!(
        card(&core, &made, "Schemas:Customer")["label"],
        "人が変えた"
    );
    // Denied: refused unasked. Allowed: through unasked.
    let set = |state: &str| {
        let said = op(
            &core,
            json!({"op": "mcp_writes_set", "project": project, "item": "branch_revert.others", "state": state}),
        );
        assert!(said.get("error").is_none(), "{said}");
    };
    set("deny");
    let denied = refused(
        &core,
        "branch_revert",
        json!({"target": "main", "value": {"undo": theirs}, "request": true}),
    );
    assert!(denied.contains("branch_revert.others"), "{denied}");
    set("allow");
    let through = ok(
        &core,
        "branch_revert",
        json!({"target": "main", "value": {"undo": theirs, "name": "もう一度"}}),
    );
    assert_eq!(through["made"], "もう一度");
    assert_eq!(person.asked.lock().unwrap().len(), 1);
    assert!(
        newest(&core, "もう一度")["lines"]
            .as_array()
            .unwrap()
            .contains(&json!(format!(
                "main のコミット {theirs}（画面）の変更を取り消した形（merge で main に入る）"
            )))
    );
}

#[test]
fn mains_commit_is_undone_through_a_branch_even_while_main_is_locked() {
    let core = Core::open(script_store()).unwrap();
    let was = card(&core, "main", "Schemas:Customer")["description"].clone();
    ok(
        &core,
        "card_update",
        json!({"target": "Schemas:Customer", "value": {"description": "まちがい"}}),
    );
    let slip = seq_of(&core, "main");
    ok(
        &core,
        "card_update",
        json!({"target": "Schemas:Customer", "value": {"label": "残す"}}),
    );
    let project = project_id(&core);
    let locked = op(
        &core,
        json!({"op": "main_lock", "project": project, "on": true}),
    );
    assert!(locked.get("error").is_none(), "{locked}");
    let commits = count(&core, "main");
    let done = ok(
        &core,
        "branch_revert",
        json!({"target": "main", "value": {"undo": slip}}),
    );
    assert_eq!(done["made"], format!("undo-{slip}"));
    assert_eq!(done["changed"], json!(["Schemas:Customer"]));
    assert_eq!(count(&core, "main"), commits);
    assert_eq!(
        card(&core, "main", "Schemas:Customer")["description"],
        "まちがい"
    );
    let branch = done["made"].as_str().unwrap();
    let there = card(&core, branch, "Schemas:Customer");
    assert_eq!(there["description"], was);
    assert_eq!(there["label"], "残す");
    // Merged in (main unlocked), main is as it was but for the slip.
    let unlocked = op(
        &core,
        json!({"op": "main_lock", "project": project, "on": false}),
    );
    assert!(unlocked.get("error").is_none(), "{unlocked}");
    ok(&core, "branch_merge", json!({"target": branch}));
    let now = card(&core, "main", "Schemas:Customer");
    assert_eq!(now["description"], was);
    assert_eq!(now["label"], "残す");
}

#[test]
fn an_undo_that_cannot_be_written_whole_is_refused_and_says_why() {
    let core = Core::open(script_store()).unwrap();
    ok(&core, "branch_create", json!({"target": "work"}));
    // One or the other.
    for value in [json!({"to": 1, "undo": 1}), json!({"name": "x"})] {
        let said = refused(
            &core,
            "branch_revert",
            json!({"target": "work", "value": value}),
        );
        assert!(said.contains("to") && said.contains("undo"), "{said}");
    }
    // A commit that wrote no card nor section.
    ok(
        &core,
        "project_update",
        json!({"value": {"description": "説明だけ"}}),
    );
    let settings = seq_of(&core, "main");
    let said = refused(
        &core,
        "branch_revert",
        json!({"target": "main", "value": {"undo": settings}}),
    );
    assert!(said.contains("文書を変えていない"), "{said}");
    // What is as it was already.
    ok(
        &core,
        "card_update",
        json!({"branch": "work", "target": "Schemas:Customer", "value": {"label": "元"}}),
    );
    ok(
        &core,
        "card_update",
        json!({"branch": "work", "target": "Schemas:Customer", "value": {"label": "一時"}}),
    );
    let slip = seq_of(&core, "work");
    ok(
        &core,
        "card_update",
        json!({"branch": "work", "target": "Schemas:Customer", "value": {"label": "元"}}),
    );
    let said = refused(
        &core,
        "branch_revert",
        json!({"target": "work", "value": {"undo": slip}}),
    );
    assert!(said.contains("今はもう前の形"), "{said}");
    // A name taken since.
    ok(
        &core,
        "card_update",
        json!({"branch": "work", "target": "Schemas:Customer", "value": {"name": "Client"}}),
    );
    let renamed = seq_of(&core, "work");
    ok(
        &core,
        "card_create",
        json!({"branch": "work", "target": "Schemas:Customer", "value": {"kind": "entity"}}),
    );
    let said = refused(
        &core,
        "branch_revert",
        json!({"target": "work", "value": {"undo": renamed}}),
    );
    assert!(
        said.contains("Schemas:Customer") && said.contains("ほかのものがある"),
        "{said}"
    );
    // A section gone since.
    ok(
        &core,
        "section_create",
        json!({"branch": "work", "target": "Schemas/Tmp"}),
    );
    ok(
        &core,
        "card_create",
        json!({"branch": "work", "target": "Schemas/Tmp:Thing", "value": {"kind": "entity"}}),
    );
    ok(
        &core,
        "card_delete",
        json!({"branch": "work", "target": "Schemas/Tmp:Thing"}),
    );
    let deleted = seq_of(&core, "work");
    ok(
        &core,
        "section_delete",
        json!({"branch": "work", "target": "Schemas/Tmp"}),
    );
    let commits = count(&core, "work");
    let said = refused(
        &core,
        "branch_revert",
        json!({"target": "work", "value": {"undo": deleted}}),
    );
    assert!(
        said.contains("Schemas/Tmp:Thing") && said.contains("もう無い"),
        "{said}"
    );
    assert_eq!(count(&core, "work"), commits);
}

#[test]
fn a_branchs_undo_of_its_first_copy_comes_back_to_main_as_the_branch_sees_it() {
    let core = Core::open(script_store()).unwrap();
    ok(&core, "branch_create", json!({"target": "work"}));
    let was = card(&core, "work", "Schemas:Customer");
    ok(
        &core,
        "card_update",
        json!({"branch": "work", "target": "Schemas:Customer", "value": {"description": "branch の説明"}}),
    );
    let copy = seq_of(&core, "work");
    // main moves on: the branch keeps seeing main as it was made.
    ok(
        &core,
        "card_update",
        json!({"target": "Schemas:Customer", "value": {"label": "main のあとの名"}}),
    );
    ok(
        &core,
        "branch_revert",
        json!({"target": "work", "value": {"undo": copy}}),
    );
    let now = card(&core, "work", "Schemas:Customer");
    assert_eq!(now["description"], was["description"]);
    assert_eq!(now["label"], was["label"]);
}
