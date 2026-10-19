//! The exports: made, changed and let go by name (`export_create`, `export_update`, `export_delete`),
//! what each writes and how it stands (`export_read`), and written (`export_execute`) — main into the
//! folder a call gives (`targetdir`: a folder, or `[default]`, each export's own on this machine) — or a
//! branch into a folder given —, what was written kept beside it (`.specboard/<project>/<export>.json`),
//! only what the export wrote before and no longer does taken away, a mark in the tree's history.

use crate::common;

use common::{call, ok, op, place, refused, script_store};
use serde_json::{Value, json};
use specboard_core::Core;
use std::path::{Path, PathBuf};

const PROJECT: &str = "見本";

/// The sample project, each working directory bound to a folder of its own, each export with the
/// same one as its default, and MCP let set the exports (asked each time by default).
fn bound() -> (Core, String, Vec<(String, PathBuf)>) {
    let store = script_store();
    let conn = rusqlite::Connection::open(&store).unwrap();
    let project: String = conn
        .query_row("SELECT id FROM projects WHERE name = ?", [PROJECT], |r| {
            r.get(0)
        })
        .unwrap();
    let text: String = conn
        .query_row(
            "SELECT export_dirs FROM projects WHERE id = ?",
            [&project],
            |r| r.get(0),
        )
        .unwrap();
    let dirs: Vec<Value> = serde_json::from_str(&text).unwrap();
    let mut folders = Vec::new();
    for dir in &dirs {
        // A working directory named as a section directly under main's root was named apart
        // (`charts_2`) when it became a section; its export kept the name it had.
        let name = dir["name"]
            .as_str()
            .unwrap()
            .trim_end_matches("_2")
            .to_string();
        let folder = place(&name);
        std::fs::create_dir_all(&folder).unwrap();
        conn.execute(
            "INSERT INTO export_dir_folders (project_id, dir_id, folder) VALUES (?, ?, ?)",
            [
                &project,
                dir["id"].as_str().unwrap(),
                folder.to_str().unwrap(),
            ],
        )
        .unwrap();
        folders.push((name, folder));
    }
    // Each export's default folder: the sample's exports are named as their working directories.
    let exports: Vec<Value> = serde_json::from_str(
        &conn
            .query_row(
                "SELECT exports FROM projects WHERE id = ?",
                [&project],
                |r| r.get::<_, String>(0),
            )
            .unwrap(),
    )
    .unwrap();
    for export in &exports {
        let name = export["name"].as_str().unwrap();
        let folder = &folders.iter().find(|(n, _)| n == name).unwrap().1;
        conn.execute(
            "INSERT INTO export_default_folders (project_id, export_id, folder) VALUES (?, ?, ?)",
            [
                &project,
                export["id"].as_str().unwrap(),
                folder.to_str().unwrap(),
            ],
        )
        .unwrap();
    }
    drop(conn);
    let core = Core::open(&store).unwrap();
    let set = op(
        &core,
        json!({"op": "mcp_writes_set", "project": project, "group": "export", "state": "allow"}),
    );
    assert!(set.get("error").is_none(), "{set}");
    (core, project, folders)
}

fn folder<'a>(folders: &'a [(String, PathBuf)], name: &str) -> &'a Path {
    &folders.iter().find(|(n, _)| n == name).unwrap().1
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap()
}

/// How each file of an export stands, by its path: never none, so that what is said of them all
/// is said of some.
fn standing(core: &Core, export: &str) -> Vec<(String, String)> {
    let read = ok(
        core,
        "export_read",
        json!({"targetdir": "[default]", "target": export}),
    );
    let files = read["exports"][0]["files"].as_array().unwrap();
    assert!(!files.is_empty(), "{read}");
    files
        .iter()
        .map(|f| {
            (
                f["path"].as_str().unwrap().to_string(),
                f["status"].as_str().unwrap_or("").to_string(),
            )
        })
        .collect()
}

#[test]
fn main_is_written_as_export_read_says() {
    let (core, project, folders) = bound();
    let answer = ok(&core, "export_execute", json!({"targetdir": "[default]", }));
    let exports = answer["exports"].as_array().unwrap();
    assert_eq!(
        exports
            .iter()
            .map(|e| e["name"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["ts", "zod", "schema", "charts", "docs"]
    );
    for export in exports {
        let name = export["name"].as_str().unwrap();
        assert_eq!(export["removed"], json!([]));
        let written: Vec<&str> = export["written"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p.as_str().unwrap())
            .collect();
        assert!(!written.is_empty(), "{name}");
        for path in &written {
            let said = ok(
                &core,
                "export_read",
                json!({"targetdir": "[default]", "target": name, "value": {"files": [path]}}),
            );
            assert_eq!(said["export"], *name);
            assert_eq!(said["files"][0]["path"], *path);
            assert_eq!(
                said["files"][0]["text"].as_str().unwrap(),
                read(&folder(&folders, name).join(path))
            );
        }
        // What it wrote is kept beside it, by the project's slug and the export's name.
        let kept: Value = serde_json::from_str(&read(
            &folder(&folders, name)
                .join(".specboard")
                .join(PROJECT)
                .join(format!("{name}.json")),
        ))
        .unwrap();
        assert_eq!(kept["projectId"], project.as_str());
        assert_eq!(kept["export"], name);
        assert_eq!(kept["files"].as_array().unwrap().len(), written.len());
        assert_eq!(kept["files"][0]["sha256"].as_str().unwrap().len(), 64);
        assert!(
            !kept["files"][0]["cards"][0]["hash"]
                .as_str()
                .unwrap()
                .is_empty()
        );
    }
    // Each file now as it was written.
    assert!(
        standing(&core, "ts")
            .iter()
            .all(|(_, status)| status == "current")
    );
    // Written into a working directory's folder, a file says which and where in it (the tree's
    // file it marks); written to another folder, it does not.
    let said = ok(
        &core,
        "export_read",
        json!({"targetdir": "[default]", "target": "ts"}),
    );
    for file in said["exports"][0]["files"].as_array().unwrap() {
        assert_eq!(
            file["directory"],
            json!({"name": "ts", "path": file["path"]}),
            "{file}"
        );
    }
    let elsewhere = place("elsewhere");
    std::fs::create_dir_all(&elsewhere).unwrap();
    let said = ok(
        &core,
        "export_read",
        json!({"targetdir": elsewhere.to_str().unwrap(), "target": "ts"}),
    );
    assert!(said["exports"][0]["files"][0].get("directory").is_none());
    // A mark in main's history, a line an export.
    let history = ok(
        &core,
        "branch_read",
        json!({"target": "main", "value": {"as": "history", "limit": 1}}),
    );
    assert!(
        history.to_string().contains("エクスポート ts を")
            && history.to_string().contains("に書いた（"),
        "{history}"
    );
}

#[test]
fn only_what_it_wrote_before_and_no_longer_does_goes() {
    let (core, _, folders) = bound();
    ok(&core, "export_execute", json!({"targetdir": "[default]", }));
    let ts = folder(&folders, "ts");
    std::fs::write(ts.join("by-hand.ts"), "// mine").unwrap();
    // Customer's own file is no longer written once its own entry goes.
    ok(
        &core,
        "export_update",
        json!({"target": "ts", "value": {"entries": {"remove": [{"from": "Schemas:Customer", "to": "customer.ts"}]}}}),
    );
    let read = ok(
        &core,
        "export_read",
        json!({"targetdir": "[default]", "target": "ts"}),
    );
    assert_eq!(read["exports"][0]["removing"], json!(["customer.ts"]));
    let dry = ok(
        &core,
        "export_execute",
        json!({"targetdir": "[default]", "target": "ts", "value": {"dryRun": true}}),
    );
    assert_eq!(dry["exports"][0]["remove"], json!(["customer.ts"]));
    assert!(ts.join("customer.ts").exists(), "下見では消さない");
    let answer = ok(
        &core,
        "export_execute",
        json!({"targetdir": "[default]", "target": "ts"}),
    );
    assert_eq!(answer["exports"][0]["removed"], json!(["customer.ts"]));
    assert!(!ts.join("customer.ts").exists());
    assert!(ts.join("by-hand.ts").exists());
}

#[test]
fn a_branch_is_not_written_into_the_default_folders() {
    let (core, _, _) = bound();
    // The default folders are main's to write: a branch is written into a folder
    // the call gives.
    let said = refused(
        &core,
        "export_execute",
        json!({"targetdir": "[default]", "branch": "refunds"}),
    );
    assert!(
        said.contains("branch は既定の先（[default]）へはエクスポートできません"),
        "{said}"
    );
    // Nor is the board written.
    let elsewhere = place("not-a-tree");
    std::fs::create_dir_all(&elsewhere).unwrap();
    let said = refused(
        &core,
        "export_execute",
        json!({"targetdir": elsewhere.to_str().unwrap(), "branch": "_project_board_"}),
    );
    assert!(
        said.contains("はエクスポートできません（エクスポートするのは main か branch）"),
        "{said}"
    );
    assert!(std::fs::read_dir(&elsewhere).unwrap().next().is_none());
}

#[test]
fn a_branch_is_written_into_a_folder_it_is_given() {
    let (core, _, _) = bound();
    // A goal's branch, with a card the export ts writes put right on it.
    ok(&core, "branch_create", json!({"target": "goal"}));
    let listed = ok(
        &core,
        "export_read",
        json!({"targetdir": "[default]", "target": "ts"}),
    );
    let card = listed["exports"][0]["files"][0]["cards"][0]
        .as_str()
        .unwrap()
        .to_string();
    ok(
        &core,
        "card_update",
        json!({"branch": "goal", "target": card, "value": {"description": "goal の branch で直した"}}),
    );
    let worktree = place("worktree");
    std::fs::create_dir_all(&worktree).unwrap();
    let at = worktree.to_str().unwrap();
    // Seen first, as it would be written.
    let dry = ok(
        &core,
        "export_execute",
        json!({"targetdir": at, "branch": "goal", "target": "ts", "value": {"dryRun": true}}),
    );
    assert_eq!(dry["branch"], "goal");
    assert!(std::fs::read_dir(&worktree).unwrap().next().is_none());
    let answer = ok(
        &core,
        "export_execute",
        json!({"targetdir": at, "branch": "goal", "target": "ts"}),
    );
    assert_eq!(answer["branch"], "goal");
    let written = answer["exports"][0]["written"][0].as_str().unwrap();
    // The branch's cards, as export_read says from the branch.
    let said = ok(
        &core,
        "export_read",
        json!({"targetdir": at, "branch": "goal", "target": "ts", "value": {"files": [written]}}),
    );
    let text = read(&worktree.join(written));
    assert_eq!(said["files"][0]["path"], written);
    assert_eq!(said["files"][0]["text"].as_str().unwrap(), text);
    assert!(text.contains("goal の branch で直した"), "{text}");
    // What was written says the branch it was written from, and the branch's newest commit.
    let kept: Value = serde_json::from_str(&read(
        &worktree.join(".specboard").join(PROJECT).join("ts.json"),
    ))
    .unwrap();
    assert_eq!(kept["branch"], "goal");
    assert!(!kept["commit"].as_str().unwrap().is_empty());
    // The mark is the branch's, not main's.
    let history = |tree: &str| {
        ok(
            &core,
            "branch_read",
            json!({"target": tree, "value": {"as": "history", "limit": 3}}),
        )
        .to_string()
    };
    assert!(
        history("goal").contains("エクスポート ts を"),
        "{}",
        history("goal")
    );
    assert!(
        !history("main").contains("エクスポート ts を"),
        "{}",
        history("main")
    );
    // Read from the branch against the worktree: each file as it was written.
    let stands = |tree: &str| {
        let read = ok(
            &core,
            "export_read",
            json!({"targetdir": at, "branch": tree, "target": "ts"}),
        );
        let files = read["exports"][0]["files"].as_array().unwrap();
        assert!(!files.is_empty(), "{read}");
        files
            .iter()
            .map(|f| {
                (
                    f["status"].as_str().unwrap().to_string(),
                    f["changedCards"].clone(),
                )
            })
            .collect::<Vec<_>>()
    };
    assert!(stands("goal").iter().all(|(s, _)| s == "current"));
    // From main, before the merge: the card the branch put right is changed since.
    let before = stands("main");
    assert!(
        before
            .iter()
            .any(|(s, cards)| s == "changed" && cards.to_string().contains(&card)),
        "{before:?}"
    );
    // Merged into main as it was: main reads it as current, nothing to write again.
    ok(&core, "branch_merge", json!({"target": "goal"}));
    assert!(stands("main").iter().all(|(s, _)| s == "current"));
    // The record of the main's own default folder is not touched by any of this.
    assert!(
        standing(&core, "ts")
            .iter()
            .all(|(_, status)| status == "unwritten")
    );
}

#[test]
fn how_each_file_stands_is_read_against_what_was_written() {
    let (core, _, folders) = bound();
    // Not written yet.
    assert!(
        standing(&core, "ts")
            .iter()
            .all(|(_, status)| status == "unwritten")
    );
    ok(&core, "export_execute", json!({"targetdir": "[default]", }));
    let ts = folder(&folders, "ts");
    // A card changed since: the files it is in, and which.
    ok(
        &core,
        "card_update",
        json!({"target": "Schemas:Customer", "value": {"description": "変えた"}}),
    );
    let read = ok(
        &core,
        "export_read",
        json!({"targetdir": "[default]", "target": "ts"}),
    );
    let files = read["exports"][0]["files"].as_array().unwrap();
    let customer = files.iter().find(|f| f["path"] == "customer.ts").unwrap();
    assert_eq!(customer["status"], "changed", "{customer}");
    assert_eq!(customer["changedCards"], json!(["Schemas:Customer"]));
    // Changed by hand, and gone.
    std::fs::write(ts.join("types.ts"), "// 手で直した").unwrap();
    std::fs::remove_file(ts.join("customer.ts")).unwrap();
    let stands = standing(&core, "ts");
    assert!(
        stands.contains(&("types.ts".to_string(), "edited".to_string())),
        "{stands:?}"
    );
    assert!(
        stands.contains(&("customer.ts".to_string(), "missing".to_string())),
        "{stands:?}"
    );
    // The list says how many stand each way.
    let listed = ok(&core, "export_read", json!({"targetdir": "[default]", }));
    let row = &listed["exports"][0];
    assert_eq!(
        (&row["name"], &row["edited"], &row["missing"]),
        (&json!("ts"), &json!(1), &json!(1))
    );
    // An export with no default folder here: nothing read from the folder, said so; and not
    // written, unless a folder is given.
    ok(
        &core,
        "export_update",
        json!({"target": "ts", "value": {"path": null}}),
    );
    let read = ok(
        &core,
        "export_read",
        json!({"targetdir": "[default]", "target": "ts"}),
    );
    assert!(read["exports"][0]["files"][0].get("status").is_none());
    assert_eq!(read["exports"][0]["path"], Value::Null);
    assert!(
        read["exports"][0]["note"]
            .as_str()
            .unwrap()
            .contains("決まっていない"),
        "{read}"
    );
    let said = refused(
        &core,
        "export_execute",
        json!({"targetdir": "[default]", "target": "ts"}),
    );
    assert!(
        said.contains("既定のエクスポート先が、この機械で決まっていません"),
        "{said}"
    );
    // The working directory's folder is not the export's: it is still bound.
    let still: i64 = rusqlite::Connection::open(core.place())
        .unwrap()
        .query_row(
            "SELECT count(*) FROM export_dir_folders WHERE folder = ?",
            [folder(&folders, "ts").to_str().unwrap()],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(still, 1);
}

#[test]
fn an_export_is_made_changed_and_let_go_by_name() {
    let (core, _, folders) = bound();
    let made = ok(
        &core,
        "export_create",
        json!({"target": "api", "value": {"path": folder(&folders, "ts").to_str().unwrap(), "entries": [
            {"from": "Schemas", "to": "api/all.ts"},
            {"from": "Schemas/Ordering", "to": "api/ordering.ts"},
        ], "exclude": ["Schemas:Rule"]}}),
    );
    assert_eq!(made, json!({"created": "api"}));
    // Said by name and path; each card by the entry nearest it, what exclude names left out.
    let read = ok(
        &core,
        "export_read",
        json!({"targetdir": "[default]", "target": "api"}),
    );
    let api = &read["exports"][0];
    // Where it is written by default, on this machine; and no working directory.
    assert_eq!(api["path"], folder(&folders, "ts").to_str().unwrap());
    assert!(api.get("dir").is_none());
    assert_eq!(
        api["entries"][1],
        json!({"from": "Schemas/Ordering", "to": "api/ordering.ts"})
    );
    assert_eq!(api["exclude"], json!(["Schemas:Rule"]));
    let cards = |path: &str| -> Vec<String> {
        api["files"]
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["path"] == path)
            .unwrap()["cards"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c.as_str().unwrap().to_string())
            .collect()
    };
    let all = cards("api/all.ts");
    assert!(all.contains(&"Schemas:Customer".to_string()), "{all:?}");
    assert!(
        !all.iter().any(|c| c.starts_with("Schemas/Ordering")),
        "{all:?}"
    );
    assert!(!all.contains(&"Schemas:Rule".to_string()), "{all:?}");
    assert_eq!(
        cards("api/ordering.ts"),
        ["Schemas/Ordering:Order", "Schemas/Ordering:Line"]
    );
    // Written, renamed with what it kept, let go of: what it wrote stays.
    ok(
        &core,
        "export_execute",
        json!({"targetdir": "[default]", "target": "api"}),
    );
    let ts = folder(&folders, "ts");
    let kept = |name: &str| {
        ts.join(".specboard")
            .join(PROJECT)
            .join(format!("{name}.json"))
    };
    assert!(kept("api").exists());
    let renamed = ok(
        &core,
        "export_update",
        json!({"target": "api", "value": {"name": "types", "exclude": {"remove": ["Schemas:Rule"]}}}),
    );
    assert_eq!(renamed["name"], "types");
    assert!(renamed.get("exclude").is_none());
    assert!(!kept("api").exists() && kept("types").exists());
    assert_eq!(
        ok(&core, "export_delete", json!({"target": "types"})),
        json!({"deleted": ["types"]})
    );
    assert!(!kept("types").exists());
    assert!(ts.join("api/all.ts").exists());

    // What is written is checked.
    for (value, said) in [
        (
            json!({"dir": "ts"}),
            "export_create の value に dir は書けません（エクスポートは作業ディレクトリを指しません。エクスポート先は export_execute の targetdir で渡し、この Mac の既定の先は path で決めます）",
        ),
        (
            json!({"path": "relative/dir"}),
            "フォルダは / か ~/ から書いてください",
        ),
        (
            json!({"entries": [{"from": "Schemas:Nope", "to": "a.ts"}]}),
            "entries[0]: カード「Schemas:Nope」は main にありません",
        ),
        (
            json!({"entries": [{"from": "Charts/Shop:Checkout", "to": "a.ts"}]}),
            "entries[0]: Charts/Shop:Checkout: チャートは .md のファイルに書きます",
        ),
        (
            json!({"entries": [{"from": "Schemas", "to": "../a.ts"}]}),
            "entries[0]: エクスポートするファイルのパスに .. や . は使えません（エクスポート先の外には書きません）",
        ),
        (
            json!({"entries": [{"from": "Schemas", "to": "a.ts", "kinds": ["nope"]}]}),
            "entries[0]: カードの種類「nope」はありません",
        ),
    ] {
        assert_eq!(
            refused(
                &core,
                "export_create",
                json!({"target": "x", "value": value.clone()})
            ),
            said,
            "{value}"
        );
    }
    assert_eq!(
        refused(&core, "export_create", json!({"target": "TS", "value": {}})),
        "エクスポート TS はもうあります"
    );
    assert_eq!(
        refused(&core, "export_delete", json!({"target": "nope"})),
        "エクスポート「nope」はありません（ある: ts・zod・schema・charts・docs）"
    );
}

#[test]
fn two_exports_may_not_write_one_file_and_a_section_takes_the_kinds_it_says() {
    let (core, _, folders) = bound();
    // The same file of the same folder: known where they are written to.
    ok(
        &core,
        "export_create",
        json!({"target": "again", "value": {"path": folder(&folders, "ts").to_str().unwrap(), "entries": [{"from": "Schemas", "to": "types.ts"}]}}),
    );
    let read = ok(&core, "export_read", json!({"targetdir": "[default]", }));
    let problems = read["problems"].to_string();
    assert!(
        problems.contains("ts/types.ts と again/types.ts が同じファイル"),
        "{problems}"
    );
    let said = refused(
        &core,
        "export_execute",
        json!({"targetdir": "[default]", "target": "again"}),
    );
    assert!(said.contains("エクスポート先が重なっていて"), "{said}");
    // Given another folder, they do not meet.
    let apart = place("apart");
    std::fs::create_dir_all(&apart).unwrap();
    let written = ok(
        &core,
        "export_execute",
        json!({"targetdir": apart.to_str().unwrap(), "target": "again"}),
    );
    assert_eq!(written["exports"][0]["path"], apart.to_str().unwrap());
    ok(&core, "export_delete", json!({"target": "again"}));
    // A section's flows, by their kind: a page of charts.
    ok(
        &core,
        "export_create",
        json!({"target": "flows", "value": {"entries": [{"from": "Charts", "to": "all.md", "kinds": ["flow"]}]}}),
    );
    let read = ok(
        &core,
        "export_read",
        json!({"targetdir": "[default]", "target": "flows"}),
    );
    assert_eq!(
        read["exports"][0]["files"][0]["cards"],
        json!(["Charts/Shop:Checkout", "Charts/Shop:Refund"])
    );
    assert_eq!(read["exports"][0]["files"][0]["content"], "chart");
}

/// Where a call says (`targetdir`): always given, a folder or `[default]`; a folder not there
/// is not made; what is written there is kept there, and the default's is not.
#[test]
fn where_to_write_is_given_by_the_call() {
    let (core, project, folders) = bound();
    // Always given.
    for tool in ["export_execute", "export_read"] {
        let said = refused(&core, tool, json!({}));
        assert!(said.contains("targetdir は必須です"), "{tool}: {said}");
    }
    let said = refused(
        &core,
        "export_execute",
        json!({"targetdir": "relative/dir"}),
    );
    assert!(
        said.contains("フォルダは / か ~/ から書いてください"),
        "{said}"
    );
    // A folder that is not there is not made, and nothing is written.
    let missing = place("missing");
    let said = refused(
        &core,
        "export_execute",
        json!({"targetdir": missing.to_str().unwrap(), "target": "ts"}),
    );
    assert!(said.contains("がありません"), "{said}");
    assert!(!missing.exists());
    // A folder: written there, its record there; the default folder is untouched.
    let tree = place("tree");
    std::fs::create_dir_all(&tree).unwrap();
    let ts = folder(&folders, "ts");
    let answer = ok(
        &core,
        "export_execute",
        json!({"targetdir": tree.to_str().unwrap(), "target": "ts"}),
    );
    assert_eq!(answer["exports"][0]["path"], tree.to_str().unwrap());
    assert!(tree.join("types.ts").exists());
    assert!(
        tree.join(".specboard")
            .join(PROJECT)
            .join("ts.json")
            .exists(),
        "the record is kept where it was written"
    );
    assert!(!ts.join("types.ts").exists());
    // Read against that folder it is current; against the default, not written yet.
    let there = ok(
        &core,
        "export_read",
        json!({"targetdir": tree.to_str().unwrap(), "target": "ts"}),
    );
    let files = there["exports"][0]["files"].as_array().unwrap();
    assert!(
        !files.is_empty() && files.iter().all(|f| f["status"] == "current"),
        "{there}"
    );
    assert_eq!(there["exports"][0]["path"], tree.to_str().unwrap());
    assert!(
        standing(&core, "ts")
            .iter()
            .all(|(_, status)| status == "unwritten")
    );
    // What it took away there is only what it wrote there.
    std::fs::write(tree.join("by-hand.ts"), "// mine").unwrap();
    ok(
        &core,
        "export_update",
        json!({"target": "ts", "value": {"entries": {"remove": [{"from": "Schemas:Customer", "to": "customer.ts"}]}}}),
    );
    let answer = ok(
        &core,
        "export_execute",
        json!({"targetdir": tree.to_str().unwrap(), "target": "ts"}),
    );
    assert_eq!(answer["exports"][0]["removed"], json!(["customer.ts"]));
    assert!(tree.join("by-hand.ts").exists());
    // The window may write the same (`tool`): its history says where.
    let said = op(
        &core,
        json!({"op": "tool", "name": "export_execute", "arguments": {"project": project, "targetdir": "[default]", "target": "ts"}}),
    );
    assert!(said.get("error").is_none(), "{said}");
    assert!(ts.join("types.ts").exists());
    let history = ok(
        &core,
        "branch_read",
        json!({"target": "main", "value": {"as": "history", "limit": 1}}),
    );
    assert!(
        history
            .to_string()
            .contains(&format!("エクスポート ts を {} に書いた（", ts.display())),
        "{history}"
    );
}

/// A store whose exports named a working directory (before `core_0016_export_targets`): the folder
/// the directory was bound to is the export's default, and the exports name none.
#[test]
fn the_folder_a_working_directory_was_bound_to_becomes_its_exports_default() {
    let (core, project, folders) = bound();
    let place_of = core.place().to_path_buf();
    drop(core);
    let conn = rusqlite::Connection::open(&place_of).unwrap();
    // Put the store back as it was before: `dir` in each export, no default folders, no mark.
    let dirs: Vec<Value> = serde_json::from_str(
        &conn
            .query_row(
                "SELECT export_dirs FROM projects WHERE id = ?",
                [&project],
                |r| r.get::<_, String>(0),
            )
            .unwrap(),
    )
    .unwrap();
    let mut exports: Vec<Value> = serde_json::from_str(
        &conn
            .query_row(
                "SELECT exports FROM projects WHERE id = ?",
                [&project],
                |r| r.get::<_, String>(0),
            )
            .unwrap(),
    )
    .unwrap();
    for export in &mut exports {
        let named = |d: &&Value| {
            d["name"].as_str().unwrap().trim_end_matches("_2") == export["name"].as_str().unwrap()
        };
        let dir = dirs.iter().find(named).unwrap()["id"].clone();
        export["dir"] = dir;
    }
    conn.execute(
        "UPDATE projects SET exports = ? WHERE id = ?",
        [serde_json::to_string(&exports).unwrap(), project.clone()],
    )
    .unwrap();
    conn.execute("DELETE FROM export_default_folders", [])
        .unwrap();
    conn.execute(
        "DELETE FROM _migrations WHERE name = 'core_0016_export_targets'",
        [],
    )
    .unwrap();
    drop(conn);
    // Opened again: brought up once.
    let core = Core::open(&place_of).unwrap();
    let set = op(
        &core,
        json!({"op": "mcp_writes_set", "project": project, "group": "export", "state": "allow"}),
    );
    assert!(set.get("error").is_none(), "{set}");
    let read = ok(&core, "export_read", json!({"targetdir": "[default]", }));
    for (name, folder) in &folders {
        let row = read["exports"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["name"] == name.as_str())
            .unwrap();
        assert_eq!(row["path"], folder.to_str().unwrap(), "{name}");
    }
    let conn = rusqlite::Connection::open(&place_of).unwrap();
    let kept: String = conn
        .query_row(
            "SELECT exports FROM projects WHERE id = ?",
            [&project],
            |r| r.get(0),
        )
        .unwrap();
    assert!(!kept.contains("\"dir\""), "{kept}");
}

#[test]
fn an_archived_projects_exports_are_not_written() {
    let (core, project, _) = bound();
    ok(
        &core,
        "export_create",
        json!({"target": "kept", "value": {"entries": [{"from": "Schemas", "to": "kept.ts"}]}}),
    );
    // Archived, it is read only, its exports too.
    let archived = op(&core, json!({"op": "project_archive", "project": project}));
    assert!(archived.get("error").is_none(), "{archived}");
    for (tool, value) in [
        (
            "export_create",
            json!({"target": "more", "value": {"entries": [{"from": "Schemas", "to": "more.ts"}]}}),
        ),
        (
            "export_update",
            json!({"target": "kept", "value": {"name": "renamed"}}),
        ),
        ("export_delete", json!({"target": "kept"})),
    ] {
        let said = refused(&core, tool, value);
        assert!(said.contains("アーカイブ"), "{tool}: {said}");
    }
    // Nor from the window.
    let said = op(
        &core,
        json!({"op": "tool", "name": "export_delete", "arguments": {"project": project, "target": "kept"}}),
    );
    assert!(
        said["error"]
            .as_str()
            .is_some_and(|e| e.contains("アーカイブ")),
        "{said}"
    );
    let read = ok(
        &core,
        "export_read",
        json!({"targetdir": "[default]", "target": "kept"}),
    );
    assert_eq!(read["exports"][0]["name"], "kept");
    // Nor written out: no file, no mark in the history, by MCP and from the window; what it would write
    // may still be looked at.
    let commits = || -> i64 {
        rusqlite::Connection::open(core.place())
            .unwrap()
            .query_row(
                "SELECT count(*) FROM commits WHERE project_id = ?",
                [&project],
                |r| r.get(0),
            )
            .unwrap()
    };
    let before = commits();
    let out = place("archived-out");
    std::fs::create_dir_all(&out).unwrap();
    let to = json!({"targetdir": out.to_str().unwrap(), "target": "kept"});
    let said = refused(&core, "export_execute", to.clone());
    assert!(said.contains("アーカイブ"), "{said}");
    let mut arguments = to.clone();
    arguments["project"] = json!(project);
    let said = op(
        &core,
        json!({"op": "tool", "name": "export_execute", "arguments": arguments}),
    );
    assert!(
        said["error"]
            .as_str()
            .is_some_and(|e| e.contains("アーカイブ")),
        "{said}"
    );
    assert!(!out.join("kept.ts").exists());
    assert_eq!(std::fs::read_dir(&out).unwrap().count(), 0);
    assert_eq!(commits(), before);
    let mut looked = to;
    looked["value"] = json!({"dryRun": true});
    let looked = ok(&core, "export_execute", looked);
    assert!(!out.join("kept.ts").exists(), "{looked}");
    assert_eq!(commits(), before);
}

#[test]
fn a_new_slug_takes_what_each_export_kept_in_its_default_folder() {
    let (core, project, folders) = bound();
    ok(&core, "export_execute", json!({"targetdir": "[default]"}));
    let elsewhere = place("written-elsewhere");
    std::fs::create_dir_all(&elsewhere).unwrap();
    ok(
        &core,
        "export_execute",
        json!({"targetdir": elsewhere.to_str().unwrap(), "target": "ts"}),
    );
    let kept = |root: &Path, slug: &str, export: &str| {
        root.join(".specboard")
            .join(slug)
            .join(format!("{export}.json"))
    };
    let names = ["ts", "zod", "schema", "charts", "docs"];
    // Asked each time by default.
    let said = refused(
        &core,
        "project_update",
        json!({"value": {"slug": "Sample Project"}}),
    );
    assert!(said.contains("project_update.slug"), "{said}");
    let set = op(
        &core,
        json!({"op": "mcp_writes_set", "project": project, "item": "project_update.slug", "state": "allow"}),
    );
    assert!(set.get("error").is_none(), "{set}");
    // Tried in a batch and let go, nothing moves.
    let (failed, text) = call(
        &core,
        "batch",
        json!({"project": PROJECT, "value": {"steps": [{"tool": "project_update", "arguments": {"value": {"slug": "Sample Project"}}}]}}),
    );
    assert!(!failed, "{text}");
    for name in names {
        assert!(
            kept(folder(&folders, name), PROJECT, name).exists(),
            "{name}"
        );
    }

    // Made as a new project's is: lowercased, every run of anything else one dash.
    let answer = ok(
        &core,
        "project_update",
        json!({"value": {"slug": "  Sample Project! "}}),
    );
    assert_eq!(answer["slug"], "sample-project");
    for name in names {
        let root = folder(&folders, name);
        assert!(kept(root, "sample-project", name).exists(), "{name}");
        assert!(!root.join(".specboard").join(PROJECT).exists(), "{name}");
    }
    // What was kept in another folder written to is not known, and stays.
    assert!(kept(&elsewhere, PROJECT, "ts").exists());
    assert!(!kept(&elsewhere, "sample-project", "ts").exists());
    // What was written is read against what was kept, as it was: current, but for a document
    // with diagrams in it, whose rev is made with the slug — written again by the next write.
    for name in names {
        for (path, status) in standing(&core, name) {
            let drawn = name == "docs" && path == "notes/Overview.md";
            assert_eq!(
                status,
                if drawn { "changed" } else { "current" },
                "{name}/{path}"
            );
        }
    }
    let history = ok(
        &core,
        "branch_read",
        json!({"target": "main", "value": {"as": "history", "limit": 1}}),
    );
    assert!(
        history
            .to_string()
            .contains("slug を「見本」から「sample-project」に変更"),
        "{history}"
    );
    // The name and the new slug name it (the old slug was the name).
    let (failed, _) = call(&core, "project_read", json!({"project": PROJECT}));
    assert!(!failed);
    let (failed, _) = call(&core, "project_read", json!({"project": "sample-project"}));
    assert!(!failed);

    // The same again: nothing.
    ok(
        &core,
        "project_update",
        json!({"value": {"slug": "sample-project"}}),
    );
    let again = ok(
        &core,
        "branch_read",
        json!({"target": "main", "value": {"as": "history", "limit": 1}}),
    );
    assert_eq!(again, history);
    // A slug let go of names nothing now.
    ok(
        &core,
        "project_update",
        json!({"value": {"slug": "second-slug"}}),
    );
    let (failed, _) = call(&core, "project_read", json!({"project": "sample-project"}));
    assert!(failed, "前の slug でまだ呼べる");
    // Nothing of it left, another project's, or too long: refused.
    assert!(
        refused(&core, "project_update", json!({"value": {"slug": " -/- "}}))
            .contains("文字も数字もありません")
    );
    let made = op(
        &core,
        json!({"op": "tool", "name": "project_create", "arguments": {"project": "Other One"}}),
    );
    assert!(made.get("error").is_none(), "{made}");
    assert_eq!(
        refused(
            &core,
            "project_update",
            json!({"value": {"slug": "OTHER one"}})
        ),
        "slug「other-one」はプロジェクト「Other One」が使っています"
    );
    assert!(
        refused(
            &core,
            "project_update",
            json!({"value": {"slug": "x".repeat(121)}})
        )
        .contains("120 文字まで")
    );
}

#[test]
fn a_documents_front_matter_stays_the_head_of_the_file_it_is_written_to() {
    // The mark after the front matter of a document that opens with one (a skill's `SKILL.md`),
    // at the head of any other.
    let (core, _, _) = bound();
    ok(
        &core,
        "card_create",
        json!({"value": {
            "Docs:Ci": {"kind": "freedoc", "body": "---\nname: ci\ndescription: CI の決まり\nuser-invocable: false\n---\n\n# CI\n\n本文"},
            "Docs:Plain": {"kind": "freedoc", "body": "# 見出し\n\n---\n\n本文"}
        }}),
    );
    let folder = place("skills");
    std::fs::create_dir_all(&folder).unwrap();
    ok(
        &core,
        "export_create",
        json!({"target": "skills", "value": {"path": folder.to_str().unwrap(), "entries": [
            {"from": "Docs:Ci", "to": "skills/ci/SKILL.md"},
            {"from": "Docs:Plain", "to": "plain.md"},
        ]}}),
    );
    ok(
        &core,
        "export_execute",
        json!({"targetdir": "[default]", "target": "skills"}),
    );
    let mark =
        "<!-- SpecBoard がプロジェクト「見本」から作ったファイルです。書き換えはプロジェクトで -->";
    assert_eq!(
        read(&folder.join("skills/ci/SKILL.md")),
        format!(
            "---\nname: ci\ndescription: CI の決まり\nuser-invocable: false\n---\n\n{mark}\n\n# CI\n\n本文\n"
        )
    );
    assert_eq!(
        read(&folder.join("plain.md")),
        format!("{mark}\n\n# 見出し\n\n---\n\n本文\n")
    );
    // As it was written, it stands.
    assert!(
        standing(&core, "skills")
            .iter()
            .all(|(_, s)| s == "current"),
        "{:?}",
        standing(&core, "skills")
    );
}
