//! A branch that sees main as it was made: what main does since is not the branch's until it takes
//! main in; merged into main, a branch is set aside unless kept, a revert of the merge brings it
//! back, and the cut of the merge from main's history lets it go; main is gone back through a
//! branch merged in.

use crate::common;

use common::{ok, project_id, refused, script_store};
use serde_json::{Value, json};
use specboard_core::Core;

fn read(core: &Core, branch: &str, path: &str) -> Value {
    ok(core, "card_read", json!({"branch": branch, "target": path}))
}

fn op(core: &Core, request: Value) -> Value {
    serde_json::from_str(&core.call(&request.to_string())).unwrap()
}

fn branches(core: &Core) -> Vec<String> {
    ok(core, "branch_list", json!({}))["branches"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(|b| b["branch"].as_str().unwrap().to_string())
        .collect()
}

/// main's newest commit's number.
fn last_commit(core: &Core) -> i64 {
    ok(
        core,
        "branch_read",
        json!({"target": "main", "value": {"as": "history", "limit": 1}}),
    )["commits"][0]["seq"]
        .as_i64()
        .unwrap()
}

#[test]
fn a_branch_sees_main_as_it_was_made_until_it_takes_main_in() {
    let core = Core::open(script_store()).unwrap();
    ok(&core, "branch_create", json!({"target": "work"}));
    let label = read(&core, "main", "Schemas:Customer")["label"].clone();
    // What main does after: a label changed, a card made, a card let go of.
    ok(
        &core,
        "card_update",
        json!({"target": "Schemas:Customer", "value": {"label": "main の客"}}),
    );
    ok(
        &core,
        "card_create",
        json!({"target": "Schemas:Voucher", "value": {"kind": "entity"}}),
    );
    ok(&core, "card_delete", json!({"target": "Schemas:Rule"}));
    // The branch: as main was.
    assert_eq!(read(&core, "work", "Schemas:Customer")["label"], label);
    refused(
        &core,
        "card_read",
        json!({"branch": "work", "target": "Schemas:Voucher"}),
    );
    read(&core, "work", "Schemas:Rule");
    // Told as main's changes not taken in yet.
    let diff = ok(&core, "branch_read", json!({"target": "work"}));
    let said = diff.to_string();
    assert!(said.contains("Schemas:Customer"), "{diff}");
    assert!(diff.get("changed").is_none(), "{diff}");
    // Written in the branch: its base is main as it was made.
    ok(
        &core,
        "card_update",
        json!({"branch": "work", "target": "Schemas:Customer", "value": {"description": "branch で書いた"}}),
    );
    let customer = read(&core, "work", "Schemas:Customer");
    assert_eq!(customer["label"], label);
    assert_eq!(customer["description"], "branch で書いた");
    // Taken in: what main did since comes in, the branch's own with it.
    let preview = ok(
        &core,
        "branch_read",
        json!({"target": "main", "value": {"against": "work"}}),
    );
    let shown = preview.to_string();
    assert!(shown.contains("Schemas:Voucher"), "{preview}");
    assert!(shown.contains("Schemas:Rule"), "{preview}");
    ok(
        &core,
        "branch_merge",
        json!({"target": "main", "value": {"into": "work"}}),
    );
    let customer = read(&core, "work", "Schemas:Customer");
    assert_eq!(customer["label"], "main の客");
    assert_eq!(customer["description"], "branch で書いた");
    read(&core, "work", "Schemas:Voucher");
    refused(
        &core,
        "card_read",
        json!({"branch": "work", "target": "Schemas:Rule"}),
    );
}

/// The section a branch's card is in gone from main, the card read all the same.
#[test]
fn what_a_branch_wrote_is_read_though_main_lets_its_section_go() {
    let core = Core::open(script_store()).unwrap();
    ok(&core, "section_create", json!({"target": "Schemas/core"}));
    ok(
        &core,
        "card_create",
        json!({"target": "Schemas/core:Emoji", "value": {"kind": "entity"}}),
    );
    ok(&core, "branch_create", json!({"target": "work"}));
    ok(
        &core,
        "card_update",
        json!({"branch": "work", "target": "Schemas/core:Emoji", "value": {"label": "絵文字"}}),
    );
    // main lets the card and its section go.
    ok(
        &core,
        "card_delete",
        json!({"target": "Schemas/core:Emoji"}),
    );
    ok(&core, "section_delete", json!({"target": "Schemas/core"}));
    assert_eq!(read(&core, "work", "Schemas/core:Emoji")["label"], "絵文字");
    // Into main, it is the choice the two sides leave.
    let preview = ok(
        &core,
        "branch_read",
        json!({"target": "work", "value": {"against": "main"}}),
    );
    assert!(preview.to_string().contains("Schemas/core"), "{preview}");
}

#[test]
fn a_branch_merged_into_main_is_set_aside_and_its_name_is_free() {
    let core = Core::open(script_store()).unwrap();
    ok(&core, "branch_create", json!({"target": "work"}));
    ok(
        &core,
        "card_update",
        json!({"branch": "work", "target": "Schemas:Customer", "value": {"label": "branch の客"}}),
    );
    ok(
        &core,
        "card_create",
        json!({"branch": "_project_board_", "target": "Goals:Work", "value": {"kind": "goal", "workBranch": "work"}}),
    );
    let merged = ok(&core, "branch_merge", json!({"target": "work"}));
    assert_eq!(merged["setAside"], true, "{merged}");
    assert!(
        merged["warnings"].to_string().contains("Goals:Work"),
        "{merged}"
    );
    assert_eq!(
        read(&core, "main", "Schemas:Customer")["label"],
        "branch の客"
    );
    assert!(!branches(&core).contains(&"work".to_string()));
    refused(&core, "branch_read", json!({"target": "work"}));
    ok(&core, "branch_create", json!({"target": "work"}));
    // Kept: there as main is, merged.
    ok(&core, "branch_create", json!({"target": "kept"}));
    ok(
        &core,
        "card_update",
        json!({"branch": "kept", "target": "Schemas:Customer", "value": {"label": "残す客"}}),
    );
    ok(
        &core,
        "branch_merge",
        json!({"target": "kept", "value": {"keep": true}}),
    );
    assert!(branches(&core).contains(&"kept".to_string()));
    let read_kept = ok(&core, "branch_read", json!({"target": "kept"}));
    assert!(read_kept["merged"].is_string(), "{read_kept}");
}

#[test]
fn main_goes_back_through_a_branch_and_brings_back_what_its_merges_set_aside() {
    let core = Core::open(script_store()).unwrap();
    let before = last_commit(&core);
    let label = read(&core, "main", "Schemas:Customer")["label"].clone();
    ok(&core, "branch_create", json!({"target": "work"}));
    ok(
        &core,
        "card_update",
        json!({"branch": "work", "target": "Schemas:Customer", "value": {"label": "branch の客"}}),
    );
    ok(&core, "branch_merge", json!({"target": "work"}));
    ok(
        &core,
        "card_create",
        json!({"target": "Schemas:Voucher", "value": {"kind": "entity"}}),
    );
    // The name taken again while it is set aside.
    ok(&core, "branch_create", json!({"target": "work"}));
    // Gone back: a branch made, main as it is.
    let made = ok(
        &core,
        "branch_revert",
        json!({"target": "main", "value": {"to": before}}),
    );
    let back = made["made"].as_str().unwrap().to_string();
    assert_eq!(back, format!("revert-{before}"));
    assert_eq!(
        read(&core, "main", "Schemas:Customer")["label"],
        "branch の客"
    );
    read(&core, "main", "Schemas:Voucher");
    // The branch: main as it was then.
    assert_eq!(read(&core, &back, "Schemas:Customer")["label"], label);
    refused(
        &core,
        "card_read",
        json!({"branch": back, "target": "Schemas:Voucher"}),
    );
    // Merged in: main as it was, the branch the undone merge set aside back by its name, numbered.
    ok(&core, "branch_merge", json!({"target": back}));
    assert_eq!(read(&core, "main", "Schemas:Customer")["label"], label);
    refused(&core, "card_read", json!({"target": "Schemas:Voucher"}));
    let now = branches(&core);
    assert!(now.contains(&"work (2)".to_string()), "{now:?}");
    assert_eq!(
        read(&core, "work (2)", "Schemas:Customer")["label"],
        "branch の客"
    );
    // While main is locked too: a branch made, a merge request to take it in.
    let project = project_id(&core);
    let locked = op(
        &core,
        json!({"op": "main_lock", "project": project, "on": true}),
    );
    assert!(locked.get("error").is_none(), "{locked}");
    let made = ok(
        &core,
        "branch_revert",
        json!({"target": "main", "value": {"to": before, "name": "やり直し"}}),
    );
    assert_eq!(made["made"], "やり直し");
}

#[test]
fn a_branch_set_aside_goes_once_its_merge_is_cut_from_mains_history() {
    let core = Core::open(script_store()).unwrap();
    ok(&core, "branch_create", json!({"target": "work"}));
    ok(
        &core,
        "card_update",
        json!({"branch": "work", "target": "Schemas:Customer", "value": {"label": "branch の客"}}),
    );
    ok(&core, "branch_merge", json!({"target": "work"}));
    let count = || -> i64 {
        rusqlite::Connection::open(core.place())
            .unwrap()
            .query_row(
                "SELECT count(*) FROM branches WHERE archived_at IS NOT NULL",
                [],
                |r| r.get(0),
            )
            .unwrap()
    };
    assert_eq!(count(), 1);
    ok(
        &core,
        "card_update",
        json!({"target": "Schemas:Customer", "value": {"description": "あと"}}),
    );
    let project = project_id(&core);
    let cut = op(
        &core,
        json!({"op": "cut", "project": project, "before": last_commit(&core)}),
    );
    assert!(cut.get("error").is_none(), "{cut}");
    assert_eq!(count(), 0);
}

#[test]
fn a_merge_request_keeps_its_branch_only_when_it_asks_and_only_once_approved() {
    let core = Core::open(script_store()).unwrap();
    let project = project_id(&core);
    // Each changes a card of its own: none conflicts with another merged before it.
    for (name, card) in [
        ("asked", "Schemas:Customer"),
        ("kept", "Schemas/Ordering:Order"),
        ("refused", "Schemas:Orders"),
    ] {
        ok(&core, "branch_create", json!({"target": name}));
        ok(
            &core,
            "card_update",
            json!({"branch": name, "target": card, "value": {"description": name}}),
        );
    }
    let locked = op(
        &core,
        json!({"op": "main_lock", "project": project, "on": true}),
    );
    assert!(locked.get("error").is_none(), "{locked}");
    let asked = ok(&core, "branch_merge", json!({"target": "asked"}));
    let number = asked["mr"].as_i64().unwrap();
    // Out: the branch is there.
    assert!(branches(&core).contains(&"asked".to_string()));
    let done = op(
        &core,
        json!({"op": "merge_request_approve", "project": project, "number": number}),
    );
    assert!(done.get("error").is_none(), "{done}");
    assert!(!branches(&core).contains(&"asked".to_string()));
    let kept = ok(
        &core,
        "branch_merge",
        json!({"target": "kept", "value": {"keep": true}}),
    );
    assert_eq!(kept["keep"], true, "{kept}");
    let kept_number = kept["mr"].as_i64().unwrap();
    let done = op(
        &core,
        json!({"op": "merge_request_approve", "project": project, "number": kept_number}),
    );
    assert!(done.get("error").is_none(), "{done}");
    assert!(branches(&core).contains(&"kept".to_string()));
    let refused_one = ok(&core, "branch_merge", json!({"target": "refused"}));
    let rejected = op(
        &core,
        json!({"op": "merge_request_reject", "project": project, "number": refused_one["mr"], "reason": "まだ"}),
    );
    assert!(rejected.get("error").is_none(), "{rejected}");
    assert!(branches(&core).contains(&"refused".to_string()));
}

/// The project's tree is the project's: renamed in main, every branch has it renamed, what the branch
/// kept of it as main was let go of.
#[test]
fn the_projects_tree_reaches_a_branch_made_before() {
    let core = Core::open(script_store()).unwrap();
    ok(&core, "branch_create", json!({"target": "work"}));
    ok(
        &core,
        "section_update",
        json!({"target": "Docs", "value": {"name": "Specs"}}),
    );
    let sections = ok(&core, "section_list", json!({"branch": "work"})).to_string();
    assert!(sections.contains("\"Specs\""), "{sections}");
    assert!(!sections.contains("\"Docs\""), "{sections}");
}
