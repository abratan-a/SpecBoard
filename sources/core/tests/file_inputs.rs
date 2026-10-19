//! Text read from files of a working directory: a body in a formatdoc's text rows at any depth and in a
//! stackdoc's versions (`bodyFrom`), and a batch's steps from a JSON file (`stepsFrom`), each read once, when
//! the call is received, and held to what one call may read in all.

use crate::common;

use common::{call, ok, op, refused, script_store};
use serde_json::{Value, json};
use specboard_core::{App, Core};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

const PROJECT: &str = "見本";

fn core() -> Core {
    Core::open(script_store()).unwrap()
}

/// A folder bound as the working directory `repo`.
fn working_directory(core: &Core) -> PathBuf {
    let root = common::place("fileinputs").join("repo");
    std::fs::create_dir_all(&root).unwrap();
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
    let bound = op(
        core,
        json!({"op": "tool", "name": "section_update", "arguments": {
            "project": PROJECT, "target": "repo", "value": {"folder": root.to_string_lossy()}
        }}),
    );
    assert!(bound.get("error").is_none(), "{bound}");
    root
}

fn write(root: &Path, path: &str, text: &str) {
    let file = root.join(path);
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(file, text).unwrap();
}

fn from(path: &str) -> Value {
    json!({"directory": "repo", "path": path})
}

fn body(core: &Core, target: &str) -> String {
    let read = ok(core, "card_read", json!({"target": target}));
    read["body"].as_str().unwrap_or_default().to_string()
}

fn exists(core: &Core, target: &str) -> bool {
    let (failed, _) = call(
        core,
        "card_read",
        json!({"project": PROJECT, "target": target}),
    );
    !failed
}

fn batch(core: &Core, arguments: Value) -> Result<Value, String> {
    let mut arguments = arguments;
    if arguments.get("target").is_none() {
        arguments["project"] = PROJECT.into();
    }
    let (failed, text) = call(core, "batch", arguments);
    if failed {
        return Err(text);
    }
    Ok(serde_json::from_str(&text).unwrap())
}

fn batch_ok(core: &Core, arguments: Value) -> Value {
    let shown = arguments.to_string();
    batch(core, arguments).unwrap_or_else(|e| panic!("batch {shown}: {e}"))
}

// MARK: - bodyFrom in rows and versions

#[test]
fn text_rows_at_any_depth_are_read_from_files_and_the_answer_says_where_each_went() {
    let core = core();
    let root = working_directory(&core);
    write(&root, "spec/intro.md", "はじめに");
    write(&root, "spec/deep.md", "深い所の文");
    write(&root, "spec/other.md", "もう一つ");
    let made = ok(
        &core,
        "card_create",
        json!({"target": "Docs:Plan", "value": {"kind": "formatdoc", "rows": [
            {"name": "intro", "kind": "text", "bodyFrom": from("spec/intro.md")},
            {"name": "part", "heading": "部", "rows": [
                {"name": "sub", "heading": "節", "rows": [
                    {"name": "deep", "kind": "text", "bodyFrom": from("spec/deep.md")}
                ]}
            ]},
            {"name": "plain", "kind": "text", "body": "そのまま"}
        ]}}),
    );
    assert_eq!(
        made["created"][0]["bodyFrom"],
        json!([
            {"at": "Docs:Plan.intro", "directory": "repo", "path": "spec/intro.md", "chars": 4},
            {"at": "Docs:Plan.part.sub.deep", "directory": "repo", "path": "spec/deep.md", "chars": 5}
        ]),
        "{made}"
    );
    assert_eq!(body(&core, "Docs:Plan.intro"), "はじめに");
    assert_eq!(body(&core, "Docs:Plan.part.sub.deep"), "深い所の文");
    assert_eq!(body(&core, "Docs:Plan.plain"), "そのまま");
    // Rows replaced: read again, the answer is the card's own.
    let changed = ok(
        &core,
        "card_update",
        json!({"target": "Docs:Plan", "value": {"rows": [
            {"name": "intro", "kind": "text", "bodyFrom": from("spec/other.md")}
        ]}}),
    );
    assert_eq!(
        changed["bodyFrom"],
        json!([{"at": "Docs:Plan.intro", "directory": "repo", "path": "spec/other.md", "chars": 4}]),
        "{changed}"
    );
    assert_eq!(body(&core, "Docs:Plan.intro"), "もう一つ");
    // The file changed after: the card does not follow.
    write(&root, "spec/other.md", "書き換えた");
    assert_eq!(body(&core, "Docs:Plan.intro"), "もう一つ");
}

#[test]
fn versions_of_a_stackdoc_are_read_from_files() {
    let core = core();
    let root = working_directory(&core);
    write(&root, "log/v1.md", "最初の版");
    write(&root, "log/v2.md", "次の版");
    let made = ok(
        &core,
        "card_create",
        json!({"target": "Docs:Notes", "value": {"kind": "stackdoc", "versions": [
            {"name": "v1", "key": "1", "bodyFrom": from("log/v1.md")}
        ]}}),
    );
    assert_eq!(
        made["created"][0]["bodyFrom"],
        json!([{"at": "Docs:Notes.v1", "directory": "repo", "path": "log/v1.md", "chars": 4}]),
        "{made}"
    );
    assert_eq!(body(&core, "Docs:Notes.v1"), "最初の版");
    let changed = ok(
        &core,
        "card_update",
        json!({"target": "Docs:Notes", "value": {"versions": [
            {"name": "v1", "key": "1", "bodyFrom": from("log/v1.md")},
            {"name": "v2", "key": "2", "bodyFrom": from("log/v2.md")}
        ]}}),
    );
    assert_eq!(
        changed["bodyFrom"].as_array().unwrap().len(),
        2,
        "{changed}"
    );
    assert_eq!(body(&core, "Docs:Notes.v2"), "次の版");
}

#[test]
fn a_row_or_a_version_named_by_its_path_takes_bodyfrom_and_answers_with_one_entry() {
    let core = core();
    let root = working_directory(&core);
    write(&root, "a.md", "行の文");
    write(&root, "b.md", "版の文");
    ok(
        &core,
        "card_create",
        json!({"value": {
            "Docs:Plan": {"kind": "formatdoc", "rows": [{"name": "r", "kind": "text", "body": "前"}]},
            "Docs:Notes": {"kind": "stackdoc", "versions": [{"name": "v1", "key": "1", "body": "前"}]}
        }}),
    );
    let made = ok(
        &core,
        "card_create",
        json!({"target": "Docs:Plan.fresh", "value": {"kind": "text", "bodyFrom": from("a.md")}}),
    );
    assert_eq!(
        made["bodyFrom"],
        json!([{"at": "Docs:Plan.fresh", "directory": "repo", "path": "a.md", "chars": 3}]),
        "{made}"
    );
    let changed = ok(
        &core,
        "card_update",
        json!({"target": "Docs:Plan.r", "value": {"bodyFrom": from("a.md")}}),
    );
    assert_eq!(changed["bodyFrom"][0]["at"], "Docs:Plan.r", "{changed}");
    assert_eq!(body(&core, "Docs:Plan.r"), "行の文");
    let changed = ok(
        &core,
        "card_update",
        json!({"target": "Docs:Notes.v1", "value": {"bodyFrom": from("b.md")}}),
    );
    assert_eq!(changed["bodyFrom"][0]["at"], "Docs:Notes.v1", "{changed}");
    assert_eq!(body(&core, "Docs:Notes.v1"), "版の文");
}

#[test]
fn every_refusal_of_a_call_is_said_with_its_place_and_nothing_is_written() {
    let core = core();
    let root = working_directory(&core);
    write(&root, "ok.md", "通る");
    let why = refused(
        &core,
        "card_create",
        json!({"value": {
            "Docs:A": {"kind": "formatdoc", "rows": [
                {"name": "first", "kind": "text", "body": "x", "bodyFrom": from("ok.md")},
                {"name": "second", "kind": "text", "bodyFrom": from("ok.md")},
                {"name": "third", "kind": "text", "bodyFrom": from("missing.md")}
            ]},
            "Docs:B": {"kind": "stackdoc", "versions": [
                {"name": "v1", "key": "1", "bodyFrom": from("../secret.md")}
            ]}
        }}),
    );
    for said in [
        "Docs:A: value.rows[0].bodyFrom: ",
        "body と一緒には書けません",
        "Docs:A: value.rows[2].bodyFrom: ",
        "ファイルがありません",
        "Docs:B: value.versions[0].bodyFrom: ",
        "外へ出ます",
    ] {
        assert!(why.contains(said), "{said}: {why}");
    }
    assert!(!why.contains("value.rows[1]"), "{why}");
    assert!(!exists(&core, "Docs:A") && !exists(&core, "Docs:B"));
    // The same for an update: every place, and the card left as it was.
    ok(
        &core,
        "card_create",
        json!({"target": "Docs:Plan", "value": {"kind": "formatdoc", "rows": [
            {"name": "r", "kind": "text", "body": "前"}]}}),
    );
    let why = refused(
        &core,
        "card_update",
        json!({"target": "Docs:Plan", "value": {"rows": [
            {"name": "r", "kind": "text", "bodyFrom": from("missing.md")},
            {"name": "s", "kind": "text", "bodyFrom": from("../secret.md")}
        ]}}),
    );
    assert!(
        why.contains("value.rows[0].bodyFrom") && why.contains("value.rows[1].bodyFrom"),
        "{why}"
    );
    assert_eq!(body(&core, "Docs:Plan.r"), "前");
}

#[test]
fn a_place_with_no_body_is_refused_before_its_file_is_read() {
    let core = core();
    working_directory(&core);
    let gone = from("not/there.md");
    // The path points at nothing, so the refusal is the place's, not the file's.
    let no_body = |why: &str| {
        assert!(why.contains("には本文がありません"), "{why}");
        assert!(!why.contains("ファイルがありません"), "{why}");
    };
    no_body(&refused(
        &core,
        "card_create",
        json!({"target": "Docs:Plan", "value": {"kind": "formatdoc", "bodyFrom": gone}}),
    ));
    no_body(&refused(
        &core,
        "card_create",
        json!({"target": "Docs:Plan", "value": {"kind": "formatdoc", "rows": [
            {"name": "t", "kind": "table", "columns": ["a"], "bodyFrom": gone}]}}),
    ));
    no_body(&refused(
        &core,
        "card_create",
        json!({"target": "Docs:Plan", "value": {"kind": "formatdoc", "rows": [
            {"name": "i", "heading": "項目", "bodyFrom": gone}]}}),
    ));
    ok(
        &core,
        "card_create",
        json!({"target": "Docs:Plan", "value": {"kind": "formatdoc", "rows": [
            {"name": "t", "kind": "table", "columns": ["a"], "cells": [["x"]]}]}}),
    );
    no_body(&refused(
        &core,
        "card_update",
        json!({"target": "Docs:Plan", "value": {"bodyFrom": gone}}),
    ));
    no_body(&refused(
        &core,
        "card_update",
        json!({"target": "Docs:Plan.t", "value": {"bodyFrom": gone}}),
    ));
    // An entity has no body either.
    no_body(&refused(
        &core,
        "card_create",
        json!({"target": "Schemas:Plain", "value": {"kind": "entity", "bodyFrom": gone}}),
    ));
}

#[test]
fn what_one_call_reads_in_all_is_held_to_the_most_it_could_carry_inline() {
    let core = core();
    let root = working_directory(&core);
    // 200,000 characters of three bytes: 600,000 bytes a file, as much as a body may be.
    let big = "あ".repeat(200_000);
    let rows = |n: usize| -> Vec<Value> {
        (0..n)
            .map(|i| {
                write(&root, &format!("big/{i}.md"), &big);
                json!({"name": format!("r{i}"), "kind": "text", "bodyFrom": from(&format!("big/{i}.md"))})
            })
            .collect()
    };
    let why = refused(
        &core,
        "card_create",
        json!({"target": "Docs:Many", "value": {"kind": "formatdoc", "rows": rows(15)}}),
    );
    assert!(why.contains("合わせて 8388608 バイトまで"), "{why}");
    assert!(!exists(&core, "Docs:Many"));
    let made = ok(
        &core,
        "card_create",
        json!({"target": "Docs:Many", "value": {"kind": "formatdoc", "rows": rows(13)}}),
    );
    assert_eq!(made["created"][0]["bodyFrom"].as_array().unwrap().len(), 13);
    assert_eq!(
        specboard_core::MAX_REQUEST_BYTES,
        8 << 20,
        "the most a request may carry is the most a call may read"
    );
}

// MARK: - stepsFrom

/// Steps in a JSON file of the working directory: a card made with its body from another file.
fn steps_file(root: &Path) -> Value {
    write(root, "notes/body.md", "ファイルの本文");
    let steps = json!([
        {"tool": "card_create", "arguments": {"target": "Free:FromFile", "value": {"kind": "note", "body": "手の中の本文"}}},
        {"tool": "card_create", "arguments": {"target": "Free:ReadLater", "value": {"kind": "note", "bodyFrom": from("notes/body.md")}}}
    ]);
    write(root, "steps.json", &steps.to_string());
    steps
}

#[test]
fn steps_are_read_from_a_file_to_try_to_open_to_add_and_to_run_at_once() {
    let core = core();
    let root = working_directory(&core);
    steps_file(&root);
    let said = json!({"directory": "repo", "path": "steps.json", "steps": 2});
    // Tried: nothing written, the file and the body in it said.
    let tried = batch_ok(&core, json!({"value": {"stepsFrom": from("steps.json")}}));
    assert_eq!(tried["stepsFrom"], said, "{tried}");
    assert_eq!(tried["steps"].as_array().unwrap().len(), 2);
    assert_eq!(
        tried["steps"][1]["bodyFrom"],
        json!([{"at": "Free:ReadLater", "directory": "repo", "path": "notes/body.md", "chars": 7}]),
        "{tried}"
    );
    assert!(!exists(&core, "Free:FromFile"));
    // Opened with them, and more added to the batch from a file.
    let opened = batch_ok(
        &core,
        json!({"value": {"stepsFrom": from("steps.json"), "open": true}}),
    );
    assert_eq!(opened["stepsFrom"], said, "{opened}");
    assert_eq!(opened["staged"], 2);
    let token = opened["batch"].as_str().unwrap().to_string();
    write(
        &root,
        "more.json",
        &json!([{"tool": "card_create", "arguments": {"target": "Free:More", "value": {"kind": "note", "body": "足す"}}}])
            .to_string(),
    );
    let added = batch_ok(
        &core,
        json!({"target": token, "value": {"stepsFrom": from("more.json")}}),
    );
    assert_eq!(
        added["stepsFrom"],
        json!({"directory": "repo", "path": "more.json", "steps": 1}),
        "{added}"
    );
    assert_eq!(added["staged"], 3);
    // Run: the batch's steps, none read again.
    let ran = batch_ok(&core, json!({"target": token, "execute": true}));
    assert_eq!(ran["executed"], 3, "{ran}");
    assert_eq!(body(&core, "Free:FromFile"), "手の中の本文");
    assert_eq!(body(&core, "Free:ReadLater"), "ファイルの本文");
    assert_eq!(body(&core, "Free:More"), "足す");
    // Run at once.
    write(
        &root,
        "once.json",
        &json!([{"tool": "card_create", "arguments": {"target": "Free:Once", "value": {"kind": "note", "body": "その場"}}}])
            .to_string(),
    );
    let ran = batch_ok(
        &core,
        json!({"execute": true, "value": {"stepsFrom": from("once.json")}}),
    );
    assert_eq!(ran["executed"], 1, "{ran}");
    assert_eq!(ran["stepsFrom"]["steps"], 1, "{ran}");
    assert_eq!(body(&core, "Free:Once"), "その場");
}

#[test]
fn a_steps_file_that_does_not_fit_is_refused_with_why() {
    let core = core();
    let root = working_directory(&core);
    write(&root, "text.json", "[{\"tool\": \"card_create\",\n  oops}]");
    write(&root, "object.json", "{\"steps\": []}");
    write(&root, "empty.json", "[]");
    write(&root, "notool.json", "[{\"arguments\": {}}]");
    write(
        &root,
        "badtool.json",
        &json!([{"tool": "card_read", "arguments": {"target": "Free:Memo"}}]).to_string(),
    );
    let many: Vec<Value> = (0..201)
        .map(|i| json!({"tool": "card_create", "arguments": {"target": format!("Free:N{i}"), "value": {"kind": "note"}}}))
        .collect();
    write(&root, "many.json", &Value::Array(many).to_string());
    let huge = std::fs::File::create(root.join("huge.json")).unwrap();
    huge.set_len(9 << 20).unwrap();
    let steps_from = |path: &str| json!({"value": {"stepsFrom": from(path)}});
    for (path, said) in [
        ("text.json", "2 行"),
        ("text.json", "JSON として読めません"),
        ("object.json", "手の配列"),
        ("empty.json", "手の配列"),
        ("notool.json", "steps の要素は"),
        ("badtool.json", "steps[0]"),
        ("many.json", "200 手まで"),
        ("huge.json", "読める 8388608 バイトを超えます"),
        ("../x.json", "外へ出ます"),
        ("none.json", "ファイルがありません"),
    ] {
        let why = batch(&core, steps_from(path)).unwrap_err();
        assert!(why.contains(said), "{path} {said}: {why}");
    }
    // Not with steps, remove or discard.
    let why = batch(
        &core,
        json!({"value": {"stepsFrom": from("empty.json"), "steps": [
            {"tool": "card_delete", "arguments": {"target": "Free:Memo"}}]}}),
    )
    .unwrap_err();
    assert!(why.contains("steps・stepsFrom・remove・discard"), "{why}");
    let opened = batch_ok(&core, json!({"value": {"open": true}}));
    let token = opened["batch"].as_str().unwrap().to_string();
    let why = batch(
        &core,
        json!({"target": token, "value": {"stepsFrom": from("empty.json"), "remove": 0}}),
    )
    .unwrap_err();
    assert!(why.contains("steps・stepsFrom・remove・discard"), "{why}");
}

#[test]
fn a_staged_step_holds_the_text_it_was_tried_with_and_no_file_is_read_again() {
    let core = core();
    let root = working_directory(&core);
    write(&root, "a.md", "試した文");
    // An inline step, and one from a file, both reading a.md.
    let opened = batch_ok(
        &core,
        json!({"value": {"open": true, "steps": [
            {"tool": "card_create", "arguments": {"target": "Free:Inline", "value": {"kind": "note", "bodyFrom": from("a.md")}}}
        ]}}),
    );
    let token = opened["batch"].as_str().unwrap().to_string();
    write(
        &root,
        "s.json",
        &json!([{"tool": "card_create", "arguments": {"target": "Free:Filed", "value": {"kind": "note", "bodyFrom": from("a.md")}}}])
            .to_string(),
    );
    batch_ok(
        &core,
        json!({"target": token, "value": {"stepsFrom": from("s.json")}}),
    );
    write(&root, "a.md", "書き換えた文");
    // Tried again after the file is gone: nothing is read.
    std::fs::remove_file(root.join("a.md")).unwrap();
    let retried = batch_ok(&core, json!({"target": token}));
    assert_eq!(retried["staged"], 2, "{retried}");
    batch_ok(&core, json!({"target": token, "execute": true}));
    assert_eq!(body(&core, "Free:Inline"), "試した文");
    assert_eq!(body(&core, "Free:Filed"), "試した文");
}

#[test]
fn one_value_for_several_targets_in_a_step_is_read_once() {
    let core = core();
    let root = working_directory(&core);
    write(&root, "n.md", "同じ文");
    let tried = batch_ok(
        &core,
        json!({"value": {"steps": [
            {"tool": "card_create", "arguments": {"target": ["Free:One", "Free:Two"], "value": {"kind": "note", "bodyFrom": from("n.md")}}}
        ]}}),
    );
    let said = tried["steps"][0]["bodyFrom"].as_array().unwrap();
    assert_eq!(said.len(), 2, "{tried}");
    assert_eq!(said[0]["at"], "Free:One");
    assert_eq!(said[1]["at"], "Free:Two");
    // A step whose body cannot be read is told by its place among the steps.
    let why = batch(
        &core,
        json!({"value": {"steps": [
            {"tool": "card_create", "arguments": {"target": "Free:Gone", "value": {"kind": "note", "bodyFrom": from("none.md")}}}
        ]}}),
    )
    .unwrap_err();
    assert!(
        why.contains("steps[0]（card_create）: ") && why.contains("ファイルがありません"),
        "{why}"
    );
}

/// A window that says yes once to what it is asked, and keeps what it was asked.
#[derive(Default)]
struct Person {
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
        json!({"answer": "once", "client": "MacBook"}).to_string()
    }
}

#[test]
fn the_consent_window_lists_the_steps_of_the_file() {
    let core = core();
    let root = working_directory(&core);
    steps_file(&root);
    let person = Arc::new(Person::default());
    core.set_app(person.clone());
    let project = rusqlite::Connection::open(core.place())
        .unwrap()
        .query_row::<String, _, _>("SELECT id FROM projects WHERE name = ?", [PROJECT], |r| {
            r.get(0)
        })
        .unwrap();
    let set = op(
        &core,
        json!({"op": "mcp_writes_set", "project": project, "item": "card_create", "state": "ask"}),
    );
    assert!(set.get("error").is_none(), "{set}");
    let ran = batch_ok(
        &core,
        json!({"execute": true, "request": true, "value": {"stepsFrom": from("steps.json")}}),
    );
    assert_eq!(ran["executed"], 2, "{ran}");
    let asked = person.asked.lock().unwrap().clone();
    assert_eq!(asked.len(), 1, "{asked:?}");
    let what = asked[0]["what"].as_str().unwrap();
    assert!(
        what.contains("0. card_create")
            && what.contains("Free:FromFile")
            && what.contains("1. card_create"),
        "{what}"
    );
}

// MARK: - The tools say them

#[test]
fn the_guide_and_the_tool_list_say_where_bodyfrom_and_stepsfrom_stand() {
    let core = core();
    // The tool definitions: the keys sit in rows, in versions and in batch's value.
    let tools: Value = serde_json::from_str(include_str!("../src/mcp/tools.json")).unwrap();
    let find = |name: &str| {
        tools
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["name"] == name)
            .unwrap()
            .clone()
    };
    for tool in ["card_create", "card_update"] {
        let definition = find(tool).to_string();
        for place in ["rows", "versions"] {
            let at = definition.find(&format!("\"{place}\":")).unwrap();
            assert!(definition[at..].contains("\"bodyFrom\""), "{tool} {place}");
        }
    }
    assert!(find("batch").to_string().contains("\"stepsFrom\""));
    let body = ok(&core, "guide", json!({"about": "bodyFrom"})).to_string();
    assert!(body.contains("rows") && body.contains("versions"), "{body}");
    let batch = ok(&core, "guide", json!({"about": "batch"})).to_string();
    assert!(batch.contains("stepsFrom"), "{batch}");
}
