//! What a branch changed, read with its contents (`specboard://SpecBoard/Docs/Design:DesignBranches`
//! `readWrite.diffContents`): `branch_read` with `as: diff` and `path` says each card or section named,
//! before (as the branch copied it from main) and after (the branch now), unit by unit in the shape a
//! commit's changes are said in — so an agent writing back what it implemented reads its own changes
//! without reading main and the branch apart and comparing them.

use crate::common;

use common::{ok, refused, script_store};
use serde_json::{Value, json};
use specboard_core::Core;

fn core() -> Core {
    Core::open(script_store()).unwrap()
}

/// The branch `work`, with a row of the spec rewritten, a note made, the memo removed and the status
/// moved to the ordering section.
fn worked() -> Core {
    let core = core();
    ok(&core, "branch_create", json!({"target": "work"}));
    ok(
        &core,
        "card_update",
        json!({"branch": "work", "target": "Docs:Spec.intro.body", "value": {"body": "[[Schemas/Ordering:Order]] を返金まで扱う"}}),
    );
    ok(
        &core,
        "card_create",
        json!({"branch": "work", "target": "Free:Plan", "value": {"kind": "note", "body": "返金の段取り"}}),
    );
    ok(
        &core,
        "card_delete",
        json!({"branch": "work", "target": "Free:Memo"}),
    );
    core
}

fn contents(core: &Core, value: Value) -> Value {
    let mut value = value;
    value["as"] = "diff".into();
    ok(
        core,
        "branch_read",
        json!({"target": "work", "value": value}),
    )
}

#[test]
fn a_changed_row_is_said_by_its_unit_before_and_after() {
    let core = worked();
    let read = contents(&core, json!({"path": "Docs:Spec"}));
    assert_eq!(read["branch"], "work", "{read}");
    let changes = read["changes"].as_array().unwrap();
    assert_eq!(changes.len(), 1, "{read}");
    assert_eq!(changes[0]["path"], "Docs:Spec");
    assert_eq!(changes[0]["change"], "change");
    let units = changes[0]["units"].to_string();
    assert!(
        units.contains("[[Schemas/Ordering:Order]] を扱う"),
        "{units}"
    );
    assert!(units.contains("返金まで扱う"), "{units}");
    assert!(changes[0].get("mainChanged").is_none(), "{read}");
}

#[test]
fn what_the_branch_made_or_removed_is_said_whole_and_several_are_read_at_once() {
    let core = worked();
    let read = contents(&core, json!({"path": ["Free:Plan", "Free:Memo"]}));
    let changes = read["changes"].as_array().unwrap();
    assert_eq!(changes.len(), 2, "{read}");
    let by = |path: &str| changes.iter().find(|c| c["path"] == path).unwrap().clone();
    let made = by("Free:Plan");
    assert_eq!(made["change"], "add");
    assert!(made["after"].to_string().contains("返金の段取り"), "{made}");
    assert!(made.get("before").is_none());
    let removed = by("Free:Memo");
    assert_eq!(removed["change"], "remove");
    assert!(removed["before"].is_object(), "{removed}");
    assert!(removed.get("after").is_none());
}

#[test]
fn a_card_main_changed_too_since_the_branch_copied_it_is_marked() {
    let core = worked();
    ok(
        &core,
        "card_update",
        json!({"target": "Docs:Spec", "value": {"label": "仕様書"}}),
    );
    let read = contents(&core, json!({"path": "Docs:Spec"}));
    assert_eq!(read["changes"][0]["mainChanged"], true, "{read}");
    // Before is what the branch copied, not main now.
    assert!(!read["changes"][0].to_string().contains("仕様書"), "{read}");
}

#[test]
fn a_long_text_is_said_by_its_lines_unless_asked_whole() {
    let core = core();
    let body: String = (1..=200)
        .map(|n| format!("{n} 行目の文。説明を足して長くする"))
        .collect::<Vec<_>>()
        .join("\n");
    ok(
        &core,
        "card_create",
        json!({"target": "Free:Long", "value": {"kind": "note", "body": body}}),
    );
    ok(&core, "branch_create", json!({"target": "work"}));
    let changed = body.replace("100 行目の文", "100 行目を直した");
    ok(
        &core,
        "card_update",
        json!({"branch": "work", "target": "Free:Long", "value": {"body": changed}}),
    );
    let read = contents(&core, json!({"path": "Free:Long"}));
    assert_eq!(
        read["changes"][0]["units"],
        json!([{"unit": "body", "key": "body", "lines": [{"line": 100, "before": "100 行目の文。説明を足して長くする", "after": "100 行目を直した。説明を足して長くする"}]}]),
        "{read}"
    );
    let whole = contents(&core, json!({"path": "Free:Long", "full": true}));
    assert_eq!(whole["changes"][0]["units"][0]["before"], body);
    assert_eq!(whole["changes"][0]["units"][0]["after"], changed);
}

#[test]
fn changes_over_an_answers_size_are_read_on_from_the_path_they_stopped_at() {
    let core = core();
    ok(&core, "branch_create", json!({"target": "work"}));
    let text = "長い本文。".repeat(600);
    let made: serde_json::Map<String, Value> = (0..30)
        .map(|n| {
            (
                format!("Free:Memo{n:02}"),
                json!({"kind": "note", "body": format!("{n} {text}")}),
            )
        })
        .collect();
    ok(
        &core,
        "card_create",
        json!({"branch": "work", "value": made}),
    );
    let paths: Vec<String> = (0..30).map(|n| format!("Free:Memo{n:02}")).collect();
    let first = contents(&core, json!({"path": paths}));
    assert_eq!(first["truncated"], true, "{}", first["changes"]);
    let shown = first["changes"].as_array().unwrap().len();
    assert!(shown < 30);
    assert_eq!(first["next"], first["changes"][shown - 1]["path"]);
    let rest = contents(&core, json!({"path": paths, "after": first["next"]}));
    assert_eq!(
        rest["changes"][0]["path"],
        format!("Free:Memo{shown:02}"),
        "{rest}"
    );
}

#[test]
fn a_path_the_branch_did_not_change_is_refused_with_what_it_did() {
    let core = worked();
    let said = refused(
        &core,
        "branch_read",
        json!({"target": "work", "value": {"as": "diff", "path": "Schemas:Status"}}),
    );
    assert!(said.contains("Schemas:Status"), "{said}");
    assert!(said.contains("Free:Plan"), "{said}");
    // One main changed since the branch was made is main's.
    ok(
        &core,
        "card_update",
        json!({"target": "Schemas:Customer", "value": {"label": "顧客さま"}}),
    );
    let said = refused(
        &core,
        "branch_read",
        json!({"target": "work", "value": {"as": "diff", "path": "Schemas:Customer"}}),
    );
    assert!(said.contains("main"), "{said}");
    for (value, words) in [
        (json!({"as": "diff", "full": true}), "path"),
        (
            json!({"as": "diff", "path": "Docs:Spec", "against": "main"}),
            "against",
        ),
        (json!({"as": "diff", "path": 3}), "path"),
        (json!({"as": "diff", "path": ["Docs:Spec", 3]}), "path"),
    ] {
        let said = refused(
            &core,
            "branch_read",
            json!({"target": "work", "value": value}),
        );
        assert!(said.contains(words), "{value}: {said}");
    }
}

#[test]
fn a_commit_is_read_for_several_paths_at_once() {
    let core = core();
    ok(
        &core,
        "card_create",
        json!({"value": {
            "Free:A": {"kind": "note", "body": "a"},
            "Free:B": {"kind": "note", "body": "b"},
            "Free:C": {"kind": "note", "body": "c"}
        }}),
    );
    let seq = ok(
        &core,
        "branch_read",
        json!({"target": "main", "value": {"as": "history", "limit": 1}}),
    )["commits"][0]["seq"]
        .clone();
    let read = ok(
        &core,
        "branch_read",
        json!({"target": "main", "value": {"as": "history", "seq": seq, "path": ["Free:A", "Free:C"]}}),
    );
    let paths: Vec<&str> = read["changes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["path"].as_str().unwrap())
        .collect();
    assert_eq!(paths, ["Free:A", "Free:C"], "{read}");
}
