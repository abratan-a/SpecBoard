//! The stacked document (`stackdoc`; `specboard://SpecBoard/Docs/Design:CardKinds`「積み重ね文書」): one card
//! of versions, the newest on top, each with a path of its own (`Docs:Changelog.v0_2_21`) — made,
//! read, changed and let go of one at a time as a formatdoc's rows are; read without their bodies
//! unless a version is the target; the version open kept as how the card looks; written out whole or
//! one version alone; merged version by version.

use crate::common;

use common::{checked, ok, op, place, project_id, refused, script_store};
use serde_json::{Value, json};
use specboard_core::Core;

const PROJECT: &str = "見本";

fn core() -> Core {
    Core::open(script_store()).unwrap()
}

/// The changelog: two versions, the newer on top.
fn changelog(core: &Core) {
    ok(
        core,
        "card_create",
        json!({"target": "Docs:Changelog", "value": {"kind": "stackdoc", "label": "変更履歴",
        "description": "出した版ごとの変更",
        "versions": [
            {"name": "v0_2", "key": "0.2", "heading": "0.2 (2026-10-10)", "body": "- [[Schemas/Ordering:Order]] に欄を足した"},
            {"name": "v0_1", "key": "0.1", "body": "- 初めて出した"},
        ]}}),
    );
}

fn paths(read: &Value) -> Vec<String> {
    read["versions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["path"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn a_stackdoc_keeps_its_versions_the_newest_on_top() {
    let core = core();
    changelog(&core);
    // The card says its versions, not their bodies.
    let read = ok(&core, "card_read", json!({"target": "Docs:Changelog"}));
    assert_eq!(read["kind"], "stackdoc");
    assert_eq!(
        read["versions"],
        json!([
            {"path": "Docs:Changelog.v0_2", "key": "0.2", "heading": "0.2 (2026-10-10)"},
            {"path": "Docs:Changelog.v0_1", "key": "0.1"},
        ])
    );
    assert_eq!(read["mentions"], json!(["Schemas/Ordering:Order"]));
    // A version by its path: its key, heading and body.
    let version = ok(&core, "card_read", json!({"target": "Docs:Changelog.v0_1"}));
    assert_eq!(
        (&version["key"], &version["body"]),
        (&json!("0.1"), &json!("- 初めて出した"))
    );
    assert!(version.get("heading").is_none(), "{version}");

    // One made goes on top.
    let made = ok(
        &core,
        "card_create",
        json!({"target": "Docs:Changelog.v0_3", "value": {"key": "0.3", "body": "- 版を足した"}}),
    );
    assert_eq!(made["path"], "Docs:Changelog.v0_3");
    let read = ok(&core, "card_read", json!({"target": "Docs:Changelog"}));
    assert_eq!(
        paths(&read),
        [
            "Docs:Changelog.v0_3",
            "Docs:Changelog.v0_2",
            "Docs:Changelog.v0_1"
        ]
    );

    // Changed in part, and renamed: what names it follows.
    ok(
        &core,
        "card_create",
        json!({"target": "Docs:Notes", "value": {"kind": "freedoc", "body": "出たのは [[Docs:Changelog.v0_3]]"}}),
    );
    ok(
        &core,
        "card_update",
        json!({"target": "Docs:Changelog.v0_3", "value": {"name": "v0_3_0", "heading": "0.3.0",
            "body": {"append": "- 名前を変えた"}}}),
    );
    let version = ok(
        &core,
        "card_read",
        json!({"target": "Docs:Changelog.v0_3_0"}),
    );
    assert_eq!(version["heading"], "0.3.0");
    assert_eq!(version["body"], "- 版を足した\n- 名前を変えた");
    let notes = ok(&core, "card_read", json!({"target": "Docs:Notes"}));
    assert_eq!(notes["body"], "出たのは [[Docs:Changelog.v0_3_0]]");

    // Let go of.
    ok(
        &core,
        "card_delete",
        json!({"target": "Docs:Changelog.v0_1"}),
    );
    let read = ok(&core, "card_read", json!({"target": "Docs:Changelog"}));
    assert_eq!(
        paths(&read),
        ["Docs:Changelog.v0_3_0", "Docs:Changelog.v0_2"]
    );

    // What does not fit is refused: a key twice, none, a version made twice, a name inside one.
    let said = refused(
        &core,
        "card_create",
        json!({"target": "Docs:Changelog.v9", "value": {"key": "0.2"}}),
    );
    assert!(said.contains("0.2"), "{said}");
    let said = refused(
        &core,
        "card_create",
        json!({"target": "Docs:Changelog.v8", "value": {"body": "x"}}),
    );
    assert!(said.contains("key"), "{said}");
    let said = refused(
        &core,
        "card_create",
        json!({"target": "Docs:Changelog.v0_2", "value": {"key": "0.2.1"}}),
    );
    assert!(said.contains("もうあります"), "{said}");
    let said = refused(
        &core,
        "card_read",
        json!({"target": "Docs:Changelog.v0_2.body"}),
    );
    assert!(said.contains("見つかりません"), "{said}");
}

#[test]
fn the_version_open_is_how_the_card_looks() {
    let core = core();
    changelog(&core);
    // The newest commit of main's history, by its number.
    let history = |core: &Core| {
        ok(
            core,
            "branch_read",
            json!({"target": "main", "value": {"as": "history", "limit": 1}}),
        )["commits"][0]["seq"]
            .as_i64()
            .unwrap()
    };
    let before = history(&core);
    ok(
        &core,
        "card_update",
        json!({"target": "Docs:Changelog", "value": {"version": "v0_1"}}),
    );
    let read = ok(&core, "card_read", json!({"target": "Docs:Changelog"}));
    assert_eq!(read["version"], "v0_1");
    // No commit: it is how the card looks.
    assert_eq!(history(&core), before);
    // The screen reads it beside the versions.
    let detail = op(
        &core,
        json!({"op": "project_detail", "project": project_id(&core)}),
    );
    let card = detail["result"]["schemas"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "Changelog")
        .unwrap()
        .clone();
    assert_eq!(card["version"], "v0_1");
    assert_eq!(
        card["form"]["rows"]["versions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v["fields"]["key"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["0.2", "0.1"]
    );
    // A rename takes the version open along; one not there is refused; none is the one on top.
    ok(
        &core,
        "card_update",
        json!({"target": "Docs:Changelog.v0_1", "value": {"name": "v0_1_0"}}),
    );
    let read = ok(&core, "card_read", json!({"target": "Docs:Changelog"}));
    assert_eq!(read["version"], "v0_1_0");
    let said = refused(
        &core,
        "card_update",
        json!({"target": "Docs:Changelog", "value": {"version": "v9"}}),
    );
    assert!(said.contains("版 v9 がありません"), "{said}");
    ok(
        &core,
        "card_update",
        json!({"target": "Docs:Changelog", "value": {"version": null}}),
    );
    let read = ok(&core, "card_read", json!({"target": "Docs:Changelog"}));
    assert!(read.get("version").is_none(), "{read}");
    let said = refused(
        &core,
        "card_update",
        json!({"target": "Schemas/Ordering:Order", "value": {"version": "v0_2"}}),
    );
    assert!(said.contains("stackdoc"), "{said}");
}

#[test]
fn a_stackdoc_is_written_out_whole_or_one_version_alone() {
    let store = script_store();
    let conn = rusqlite::Connection::open(&store).unwrap();
    let project: String = conn
        .query_row("SELECT id FROM projects WHERE name = ?", [PROJECT], |r| {
            r.get(0)
        })
        .unwrap();
    let folder = place("stackdoc-docs");
    std::fs::create_dir_all(&folder).unwrap();
    drop(conn);
    let core = Core::open(&store).unwrap();
    let set = op(
        &core,
        json!({"op": "mcp_writes_set", "project": project, "group": "export", "state": "allow"}),
    );
    assert!(set.get("error").is_none(), "{set}");
    changelog(&core);

    ok(
        &core,
        "export_create",
        json!({"target": "log", "value": {"path": folder.to_str().unwrap(), "entries": [
            {"from": "Docs:Changelog", "to": "CHANGELOG.md"},
            {"from": "Docs:Changelog.v0_2", "to": "notes/"},
        ]}}),
    );
    let read = ok(
        &core,
        "export_read",
        json!({"targetdir": "[default]", "target": "log"}),
    );
    assert_eq!(
        read["exports"][0]["entries"],
        json!([
            {"from": "Docs:Changelog", "to": "CHANGELOG.md"},
            {"from": "Docs:Changelog.v0_2", "to": "notes/"},
        ])
    );
    ok(
        &core,
        "export_execute",
        json!({"targetdir": "[default]", "target": "log"}),
    );
    let whole = std::fs::read_to_string(folder.join("CHANGELOG.md")).unwrap();
    let body = whole.split_once("\n\n").unwrap().1;
    assert_eq!(
        body,
        "# 変更履歴\n\n出した版ごとの変更\n\n## 0.2 (2026-10-10)\n\n- 注文 に欄を足した\n\n## 0.1\n\n- 初めて出した\n"
    );
    let one = std::fs::read_to_string(folder.join("notes/v0_2.md")).unwrap();
    assert_eq!(
        one.split_once("\n\n").unwrap().1,
        "## 0.2 (2026-10-10)\n\n- 注文 に欄を足した\n"
    );
    // As MCP reads it: its paths as written.
    let said = ok(
        &core,
        "card_read",
        json!({"target": "Docs:Changelog.v0_2", "value": {"as": "markdown"}}),
    );
    assert!(
        said["markdown"]
            .as_str()
            .unwrap()
            .ends_with("## 0.2 (2026-10-10)\n\n- [[Schemas/Ordering:Order]] に欄を足した"),
        "{said}"
    );
    // A version that is gone is said, and its file not written.
    ok(
        &core,
        "card_delete",
        json!({"target": "Docs:Changelog.v0_2"}),
    );
    let read = ok(&core, "export_read", json!({"targetdir": "[default]", }));
    assert!(
        read["problems"].to_string().contains("消えた版"),
        "{}",
        read["problems"]
    );
    let said = refused(
        &core,
        "export_update",
        json!({"target": "log", "value": {"entries": {"add": [{"from": "Docs:Changelog.v7", "to": "v7.md"}]}}}),
    );
    assert!(said.contains("版 v7 はありません"), "{said}");
}

#[test]
fn what_a_version_names_is_checked_and_followed() {
    let core = core();
    changelog(&core);
    ok(
        &core,
        "card_create",
        json!({"target": "Docs:Changelog.v0_3", "value": {"key": "0.3", "body": "- [[Schemas:Nowhere]] は無い"}}),
    );
    let after = checked(&core, "main");
    assert!(
        after["brokenPaths"].to_string().contains("Schemas:Nowhere"),
        "{after}"
    );
    // A card renamed: the version's body follows it.
    ok(
        &core,
        "card_update",
        json!({"target": "Schemas/Ordering:Order", "value": {"name": "Purchase"}}),
    );
    let version = ok(&core, "card_read", json!({"target": "Docs:Changelog.v0_2"}));
    assert_eq!(
        version["body"],
        "- [[Schemas/Ordering:Purchase]] に欄を足した"
    );
}

#[test]
fn versions_are_merged_one_by_one() {
    let core = core();
    changelog(&core);
    ok(&core, "branch_create", json!({"target": "next"}));
    ok(
        &core,
        "card_create",
        json!({"branch": "next", "target": "Docs:Changelog.v0_3", "value": {"key": "0.3", "body": "- branch で足した"}}),
    );
    ok(
        &core,
        "card_update",
        json!({"target": "Docs:Changelog.v0_1", "value": {"body": "- main で直した"}}),
    );
    let diff = ok(&core, "branch_read", json!({"target": "next"}));
    assert_eq!(
        diff["changed"][0]["changes"],
        json!(["行を足す v0_3"]),
        "{diff}"
    );
    ok(&core, "branch_merge", json!({"target": "next"}));
    let read = ok(&core, "card_read", json!({"target": "Docs:Changelog"}));
    assert_eq!(
        paths(&read),
        [
            "Docs:Changelog.v0_3",
            "Docs:Changelog.v0_2",
            "Docs:Changelog.v0_1"
        ]
    );
    let version = ok(&core, "card_read", json!({"target": "Docs:Changelog.v0_1"}));
    assert_eq!(version["body"], "- main で直した");
}

#[test]
fn the_guide_says_how_a_stackdoc_and_a_version_are_written() {
    let Ok(specboard_core::mcp::Output::Json(said)) = specboard_core::mcp::guide(Some("stackdoc"))
    else {
        panic!("stackdoc の guide がありません");
    };
    let keys = |table: &Value| -> Vec<String> {
        table
            .as_object()
            .map(|t| t.keys().filter(|k| *k != "same").cloned().collect())
            .unwrap_or_default()
    };
    assert_eq!(keys(&said["keys"]["card_create"]), ["versions"]);
    assert_eq!(keys(&said["keys"]["card_update"]), ["version"]);
    // The versions' keys are said once, in card_create's; card_update's says where.
    assert_eq!(
        keys(&said["keys"]["card_create"]["versions"]),
        ["", "name", "key", "heading", "body", "bodyFrom"]
    );
    assert!(
        said["keys"]["card_update"]["same"]
            .as_str()
            .unwrap()
            .contains("keys.card_create"),
        "{said}"
    );
    // A version as target: the same keys, and card_update's name renames it.
    assert!(said["version"]["card_create"]["same"].is_string(), "{said}");
    assert!(said["version"]["card_update"]["name"].is_string(), "{said}");
}
