//! The working directories as sections of main and every branch: one listed in the project tree
//! (`{"type": "workdir", "name": "repo"}`) is a section directly under every root but the board's,
//! read from the folder bound to it on this machine — a directory a section of the kind
//! `directory`, a file a card of the kind `file` labelled by its own name — the same in every tree,
//! never written there but for notes and places, set by `section_update` and read again by
//! `section_action`.

use crate::common;

use common::{call, op, project_id, script_store};
use serde_json::{Value, json};
use specboard_core::Core;
use std::fs;
use std::path::{Path, PathBuf};

const PROJECT: &str = "見本";

fn ok(core: &Core, tool: &str, mut arguments: Value) -> Value {
    arguments["project"] = PROJECT.into();
    let (refused, text) = call(core, tool, arguments.clone());
    assert!(!refused, "{tool} {arguments}: {text}");
    serde_json::from_str(&text).unwrap()
}

fn refused(core: &Core, tool: &str, mut arguments: Value) -> String {
    arguments["project"] = PROJECT.into();
    let (refused, text) = call(core, tool, arguments.clone());
    assert!(refused, "{tool} {arguments} が通った: {text}");
    text
}

/// A tool called as the window calls it (`tool`): held to no item of MCP's.
fn window(core: &Core, tool: &str, mut arguments: Value) -> Value {
    arguments["project"] = PROJECT.into();
    let said = op(
        core,
        json!({"op": "tool", "name": tool, "arguments": arguments}),
    );
    assert!(said.get("error").is_none(), "{tool}: {said}");
    said["result"]["json"].clone()
}

/// A repository of a few files: what git leaves out (`target/`), an image, the Finder's file.
fn repository() -> PathBuf {
    let root = common::place("work");
    for (path, text) in [
        ("src/a.rs", "fn a() {}"),
        ("src/b.rs", "fn b() {}"),
        ("src/設計.md", "# 設計"),
        ("target/debug/x.o", "?"),
        ("docs/icon.png", "?"),
        ("README.md", "# readme"),
        (".gitignore", "target/\n"),
        (".DS_Store", "?"),
        (".git/HEAD", "ref: refs/heads/main"),
    ] {
        let file = root.join(path);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(file, text).unwrap();
    }
    root
}

/// The project tree with `extra` put after main's sections.
fn tree_with(core: &Core, extra: &[Value]) -> Value {
    let mut tree = ok(core, "project_read", json!({}))["projectTree"].clone();
    for e in extra {
        tree["main_sections"]
            .as_array_mut()
            .unwrap()
            .push(e.clone());
    }
    tree
}

/// The working directory `repo` listed in the project tree, and bound to `folder` on this machine.
fn bind(core: &Core, folder: &Path) {
    ok(
        core,
        "project_update",
        json!({"value": {"project_tree": tree_with(core, &[json!({"type": "workdir", "name": "repo"})])}}),
    );
    window(
        core,
        "section_update",
        json!({"target": "repo", "value": {"folder": folder.to_string_lossy()}}),
    );
}

/// The files of the working directory `repo` as `branch` reads them, by their paths.
fn paths(core: &Core, branch: &str) -> Vec<String> {
    let listed = ok(
        core,
        "card_list",
        json!({"branch": branch, "value": {"section": "repo", "recursive": true, "kind": "file", "limit": 200}}),
    );
    let mut out: Vec<String> = listed["cards"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| {
            let read = ok(
                core,
                "card_read",
                json!({"branch": branch, "target": c["path"]}),
            );
            read["filePath"]
                .as_str()
                .map(str::to_string)
                .unwrap_or_default()
        })
        .collect();
    out.sort();
    out
}

fn section_names(listed: &Value) -> Vec<String> {
    listed["sections"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["section"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn a_working_directory_is_a_section_of_main_and_of_every_branch() {
    let core = Core::open(script_store()).unwrap();
    ok(&core, "branch_create", json!({"target": "work"}));
    let repo = repository();
    bind(&core, &repo);
    // As git reads it: what .gitignore leaves out, and git's own and the Finder's file, are not
    // there. The same in main and in a branch made before it.
    for branch in ["main", "work"] {
        assert_eq!(
            paths(&core, branch),
            [
                ".gitignore",
                "README.md",
                "docs/icon.png",
                "src/a.rs",
                "src/b.rs",
                "src/設計.md"
            ],
            "{branch}"
        );
    }
    // Directly under the root, with how many it read; what is in it only when it is named.
    let listed = ok(&core, "section_list", json!({}));
    let repo_row = listed["sections"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["section"] == "repo")
        .unwrap()
        .clone();
    assert_eq!(repo_row["type"], "workdir", "{repo_row}");
    assert_eq!(repo_row["files"], 6, "{repo_row}");
    assert_eq!(repo_row["directories"], 2, "{repo_row}");
    let names = section_names(&listed);
    assert!(!names.iter().any(|n| n.starts_with("repo/")), "{names:?}");
    let inside = section_names(&ok(
        &core,
        "section_list",
        json!({"value": {"section": "repo"}}),
    ));
    assert_eq!(inside, ["repo", "repo/docs", "repo/src"]);
    // card_list leaves the files out unless a working directory's section is named.
    let all = ok(&core, "card_list", json!({"value": {"limit": 200}}));
    assert!(
        !all["cards"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["kind"] == "file"),
        "{all}"
    );
    // A file's card: its own name its label, its path from the top.
    let a = ok(&core, "card_read", json!({"target": "repo/src:'a.rs'"}));
    assert_eq!(
        (&a["kind"], &a["label"], &a["filePath"]),
        (&json!("file"), &json!("a.rs"), &json!("src/a.rs")),
        "{a}"
    );
    let design = ok(&core, "card_read", json!({"target": "repo/src:'設計.md'"}));
    assert_eq!(design["label"], "設計.md", "{design}");
    // The section says how and from where it is read, and what it can be made to do.
    let read = ok(&core, "section_read", json!({"target": "repo"}));
    assert_eq!(read["type"], "workdir", "{read}");
    assert_eq!(read["folder"], repo.to_string_lossy().as_ref());
    assert_eq!(read["readType"], "git");
    assert_eq!(read["exclude"], json!([]));
    assert_eq!(read["files"], 6);
    assert_eq!(read["problem"], Value::Null);
    assert_eq!(read["actions"][0]["op"], "read", "{read}");
    assert_eq!(read["actions"][0]["available"], true, "{read}");
    assert_eq!(read["sections"], json!(["repo/docs", "repo/src"]), "{read}");
    // The history of main and of the branch says nothing of what was read, nor does the branch's
    // difference from main.
    let diff = ok(&core, "branch_read", json!({"target": "work"}));
    assert!(!diff.to_string().contains("repo/"), "{diff}");
    fs::remove_dir_all(&repo).unwrap();
}

#[test]
fn a_working_directory_is_read_as_it_is_set_and_again_when_asked() {
    let core = Core::open(script_store()).unwrap();
    let repo = repository();
    bind(&core, &repo);
    // Read otherwise: basic keeps what .gitignore leaves out; exclude leaves out besides. Read again
    // as it changes, whichever tree is named.
    ok(&core, "branch_create", json!({"target": "work"}));
    let set = ok(
        &core,
        "section_update",
        json!({"branch": "work", "target": "repo", "value": {"readType": "basic", "exclude": [".png", "debug/"]}}),
    );
    assert_eq!(set["readType"], "basic", "{set}");
    assert_eq!(
        paths(&core, "main"),
        [
            ".gitignore",
            "README.md",
            "src/a.rs",
            "src/b.rs",
            "src/設計.md"
        ]
    );
    ok(
        &core,
        "section_update",
        json!({"target": "repo", "value": {"exclude": [".png"]}}),
    );
    assert!(paths(&core, "main").contains(&"target/debug/x.o".to_string()));
    // Not read: the section alone, and why.
    let none = ok(
        &core,
        "section_update",
        json!({"target": "repo", "value": {"readType": "none"}}),
    );
    assert_eq!(none["files"], 0, "{none}");
    assert!(
        none["problem"].as_str().unwrap().contains("readType: none"),
        "{none}"
    );
    assert_eq!(none["actions"][0]["available"], false, "{none}");
    assert!(paths(&core, "main").is_empty());
    let said = refused(
        &core,
        "section_action",
        json!({"target": "repo", "op": "read"}),
    );
    assert!(said.contains("今読み込めません"), "{said}");
    ok(
        &core,
        "section_update",
        json!({"target": "repo", "value": {"readType": "basic", "exclude": [".png"]}}),
    );

    // Read again when asked: a file made since is there, in main and in the branch.
    fs::write(repo.join("src/c.rs"), "fn c() {}").unwrap();
    assert!(!paths(&core, "main").contains(&"src/c.rs".to_string()));
    let again = ok(
        &core,
        "section_action",
        json!({"target": "repo", "op": "read"}),
    );
    assert_eq!(again["section"], "repo", "{again}");
    assert_eq!(again["files"], 7, "{again}");
    assert_eq!(again["directories"], 4, "{again}");
    assert!(paths(&core, "main").contains(&"src/c.rs".to_string()));
    assert!(paths(&core, "work").contains(&"src/c.rs".to_string()));
    // Another section has no such action; nor an op its kind does not have.
    let said = refused(
        &core,
        "section_action",
        json!({"target": "Schemas", "op": "read"}),
    );
    assert!(said.contains("アクションはありません"), "{said}");
    let said = refused(
        &core,
        "section_action",
        json!({"target": "repo", "op": "start"}),
    );
    assert!(said.contains("read"), "{said}");
    // The settings are the project's: the history of main says it once, whichever tree was named.
    let commits = op(
        &core,
        json!({"op": "commits", "project": project_id(&core), "limit": 1}),
    );
    assert_eq!(
        commits["result"]["commits"][0]["lines"][0]["summary"],
        "作業ディレクトリ repo の設定を変更"
    );
    // A batch's step does not write them.
    let (_, said) = call(
        &core,
        "batch",
        json!({"project": PROJECT, "value": {"steps": [{"tool": "section_update", "arguments": {"target": "repo", "value": {"readType": "git"}}}]}}),
    );
    assert!(said.contains("batch の手では書けません"), "{said}");
    fs::remove_dir_all(&repo).unwrap();
}

#[test]
fn a_working_directory_not_there_says_why() {
    let core = Core::open(script_store()).unwrap();
    let gone = common::place("nowhere");
    bind(&core, &gone);
    let read = ok(&core, "section_read", json!({"target": "repo"}));
    assert_eq!(read["files"], 0, "{read}");
    assert!(
        read["problem"].as_str().unwrap().contains("ありません"),
        "{read}"
    );
    // One with no folder on this machine says so too, and cannot be read.
    let unbound = ok(&core, "section_read", json!({"target": "ts"}));
    assert_eq!(unbound["folder"], Value::Null, "{unbound}");
    assert!(
        unbound["problem"]
            .as_str()
            .unwrap()
            .contains("結び付いていません"),
        "{unbound}"
    );
    assert_eq!(unbound["actions"][0]["available"], false, "{unbound}");
    assert!(paths(&core, "main").is_empty());
}

#[test]
fn a_working_directory_s_file_is_shown_as_it_is() {
    let core = Core::open(script_store()).unwrap();
    let repo = repository();
    fs::write(repo.join("docs/big.bin"), [0u8, 159, 146, 150]).unwrap();
    bind(&core, &repo);
    let project = project_id(&core);
    let tree = ok(&core, "project_read", json!({}))["projectTree"].clone();
    let dir = tree["main_sections"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "repo")
        .unwrap()["directory"]
        .as_str()
        .unwrap()
        .to_string();
    let read = |path: &str| -> Value {
        op(
            &core,
            json!({"op": "file_text", "project": project, "id": format!("file:{dir}/{path}")}),
        )["result"]
            .clone()
    };
    assert_eq!(read("src/a.rs")["text"], "fn a() {}");
    assert_eq!(read("src/a.rs")["cut"], false);
    assert!(
        read("docs/big.bin")["problem"]
            .as_str()
            .unwrap()
            .contains("文字のファイルではありません")
    );
    assert!(
        read("src/gone.rs")["problem"]
            .as_str()
            .unwrap()
            .contains("読めません")
    );
    let refused = op(
        &core,
        json!({"op": "file_text", "project": project, "id": format!("file:{dir}/../etc/passwd")}),
    );
    assert!(
        refused["error"]
            .as_str()
            .unwrap()
            .contains("パスではありません"),
        "{refused}"
    );
    fs::remove_dir_all(&repo).unwrap();
}

#[test]
fn a_folder_read_as_git_that_is_no_repository_has_its_gitignore_read_and_says_so() {
    // With no `.git`, what its `.gitignore` leaves out is read all the same, and it is said.
    let core = Core::open(script_store()).unwrap();
    let repo = repository();
    fs::remove_dir_all(repo.join(".git")).unwrap();
    for (path, text) in [
        ("node_modules/x/index.js", "?"),
        ("src/.gitignore", "*.tmp\n"),
        ("src/c.tmp", "?"),
    ] {
        let file = repo.join(path);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(file, text).unwrap();
    }
    fs::write(repo.join(".gitignore"), "target/\nnode_modules/\n").unwrap();
    bind(&core, &repo);
    assert_eq!(
        paths(&core, "main"),
        [
            ".gitignore",
            "README.md",
            "docs/icon.png",
            "src/.gitignore",
            "src/a.rs",
            "src/b.rs",
            "src/設計.md"
        ]
    );
    let said = |answer: &Value| -> String {
        answer["warnings"]
            .as_array()
            .unwrap_or_else(|| panic!("{answer}"))
            .iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
            .join("\n")
    };
    let read = ok(&core, "section_read", json!({"target": "repo"}));
    assert!(
        said(&read).contains("は git のリポジトリではありません（.git がありません）"),
        "{read}"
    );
    let updated = ok(
        &core,
        "section_update",
        json!({"target": "repo", "value": {"exclude": [".png"]}}),
    );
    assert!(
        said(&updated).contains("git のリポジトリではありません"),
        "{updated}"
    );
    let action = ok(
        &core,
        "section_action",
        json!({"target": "repo", "op": "read"}),
    );
    assert!(
        said(&action).contains("git のリポジトリではありません"),
        "{action}"
    );
    // Read otherwise, or once it is one: nothing to say.
    let basic = ok(
        &core,
        "section_update",
        json!({"target": "repo", "value": {"readType": "basic"}}),
    );
    assert!(basic.get("warnings").is_none(), "{basic}");
    fs::create_dir_all(repo.join(".git")).unwrap();
    fs::write(repo.join(".git/HEAD"), "ref: refs/heads/main").unwrap();
    let again = ok(
        &core,
        "section_update",
        json!({"target": "repo", "value": {"readType": "git"}}),
    );
    assert!(again.get("warnings").is_none(), "{again}");
    assert!(
        !paths(&core, "main")
            .iter()
            .any(|p| p.starts_with("node_modules"))
    );
}

#[test]
fn a_folder_read_as_git_whose_files_are_all_left_out_is_not_a_section() {
    let core = Core::open(script_store()).unwrap();
    let repo = repository();
    for (path, text) in [
        (".pytest_cache/.gitignore", "*\n"),
        (".pytest_cache/v/cache/lastfailed", "{}"),
        ("src/deep/.gitignore", "*\n"),
        ("src/deep/inner/x.o", "?"),
    ] {
        let file = repo.join(path);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(file, text).unwrap();
    }
    fs::create_dir_all(repo.join("empty/nothing")).unwrap();
    bind(&core, &repo);
    let inside = section_names(&ok(
        &core,
        "section_list",
        json!({"value": {"section": "repo"}}),
    ));
    assert_eq!(inside, ["repo", "repo/docs", "repo/src"], "{inside:?}");
    // Read otherwise, an empty folder is still what is there.
    window(
        &core,
        "section_update",
        json!({"target": "repo", "value": {"readType": "basic"}}),
    );
    let inside = section_names(&ok(
        &core,
        "section_list",
        json!({"value": {"section": "repo"}}),
    ));
    assert!(inside.contains(&"repo/empty".to_string()), "{inside:?}");
}

#[test]
fn a_file_s_and_a_directory_s_names_are_their_own_and_a_path_quotes_them() {
    let core = Core::open(script_store()).unwrap();
    let root = common::place("named");
    for path in [
        ".claude/rules/ci.md",
        "docs/design/CardKinds.md",
        "docs/release notes.md",
    ] {
        let file = root.join(path);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(file, "?").unwrap();
    }
    bind(&core, &root);
    let names = section_names(&ok(
        &core,
        "section_list",
        json!({"value": {"section": "repo"}}),
    ));
    for name in ["repo/'.claude'", "repo/'.claude'/rules", "repo/docs/design"] {
        assert!(names.contains(&name.to_string()), "{name}: {names:?}");
    }
    for (target, file_path) in [
        ("repo/'.claude'/rules:'ci.md'", ".claude/rules/ci.md"),
        (
            "repo/docs/design:'CardKinds.md'",
            "docs/design/CardKinds.md",
        ),
        ("repo/docs:'release notes.md'", "docs/release notes.md"),
    ] {
        let read = ok(&core, "card_read", json!({"target": target}));
        assert_eq!(read["path"], target, "{read}");
        assert_eq!(read["filePath"], file_path, "{read}");
    }
    // A line to a file by its path in the tree, read back so.
    ok(
        &core,
        "card_update",
        json!({"target": "Schemas/Ordering:Order", "value": {"connections": {"add": [
            {"line": "implementedBy", "to": "repo/docs/design:'CardKinds.md'"}
        ]}}}),
    );
    let order = ok(
        &core,
        "card_read",
        json!({"target": "Schemas/Ordering:Order", "value": {"include": ["connections"]}}),
    );
    assert!(
        order["connections"]
            .as_array()
            .unwrap()
            .contains(&json!({"line": "implementedBy", "to": "repo/docs/design:'CardKinds.md'"})),
        "{}",
        order["connections"]
    );
    // A path written with a name the tree gave a file before is not rewritten: the line refused
    // names the file's name now among those that look like it.
    let said = refused(
        &core,
        "card_update",
        json!({"target": "Schemas/Ordering:Order", "value": {"connections": {"add": [
            {"line": "implementedBy", "to": "repo/docs/design:CardKinds_md"}
        ]}}}),
    );
    assert!(said.contains("'CardKinds.md'"), "{said}");
}

#[test]
fn what_is_read_is_not_written_but_notes_and_places() {
    let core = Core::open(script_store()).unwrap();
    let repo = repository();
    bind(&core, &repo);
    // A file is read only, but for where it sits on its map.
    for (tool, arguments, said) in [
        (
            "card_update",
            json!({"target": "repo/src:'a.rs'", "value": {"label": "x"}}),
            "読むだけです",
        ),
        (
            "card_delete",
            json!({"target": "repo/src:'a.rs'"}),
            "読むだけです",
        ),
        (
            "tag_update",
            json!({"target": "core", "value": {"add": ["repo/src:'a.rs'"]}}),
            "",
        ),
        (
            "section_create",
            json!({"target": "repo/src/new"}),
            "セクションは作れません",
        ),
        (
            "section_update",
            json!({"target": "repo/src", "value": {"name": "lib"}}),
            "ディレクトリで、読むだけです",
        ),
        (
            "section_delete",
            json!({"target": "repo/src"}),
            "ディレクトリで、読むだけです",
        ),
        (
            "card_create",
            json!({"target": "repo/src:Order", "value": {"kind": "entity"}}),
            "置けるのはメモ（kind: note）だけです",
        ),
        (
            "card_update",
            json!({"target": "Schemas/Ordering:Order", "value": {"section": "repo/src"}}),
            "置けるのはメモ（kind: note）だけです",
        ),
    ] {
        let refusal = refused(&core, tool, arguments.clone());
        assert!(refusal.contains(said), "{tool} {arguments}: {refusal}");
    }
    let placed = ok(
        &core,
        "card_update",
        json!({"target": "repo/src:'a.rs'", "value": {"topLeft": {"x": 4, "y": 8}}}),
    );
    assert!(placed.get("error").is_none(), "{placed}");
    let read = ok(
        &core,
        "section_read",
        json!({"target": "repo/src", "value": {"include": ["places"]}}),
    );
    assert_eq!(
        read["places"]["repo/src:'a.rs'"]["topLeft"],
        json!({"x": 4, "y": 8}),
        "{read}"
    );

    // A note is an ordinary card of the tree it is put in: in main's history, not in the branch's.
    ok(&core, "branch_create", json!({"target": "work"}));
    ok(
        &core,
        "card_create",
        json!({"target": "repo/src:設計メモ", "value": {"kind": "note", "body": "a.rs と b.rs"}}),
    );
    let note = ok(&core, "card_read", json!({"target": "repo/src:設計メモ"}));
    assert_eq!(note["kind"], "note", "{note}");
    let listed = ok(&core, "card_list", json!({}));
    assert!(
        listed["cards"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["path"] == "repo/src:設計メモ"),
        "{listed}"
    );
    let said = refused(
        &core,
        "card_read",
        json!({"branch": "work", "target": "repo/src:設計メモ"}),
    );
    assert!(said.contains("設計メモ"), "{said}");

    // Its folder renamed and read again: the note stays, in a folder said missing, and the check
    // says so; moved to the folder of now, nothing is said.
    fs::rename(repo.join("src"), repo.join("lib")).unwrap();
    ok(
        &core,
        "section_action",
        json!({"target": "repo", "op": "read"}),
    );
    let still = ok(&core, "card_read", json!({"target": "repo/src:設計メモ"}));
    assert_eq!(still["kind"], "note", "{still}");
    let missing = ok(&core, "section_read", json!({"target": "repo/src"}));
    assert_eq!(missing["missing"], true, "{missing}");
    let checked = common::checked(&core, "main");
    let folders = checked["missingFolders"]
        .as_array()
        .unwrap_or_else(|| panic!("{checked}"));
    assert_eq!(folders.len(), 1, "{checked}");
    assert_eq!(folders[0]["at"], "repo/src:設計メモ", "{checked}");
    ok(
        &core,
        "card_update",
        json!({"target": "repo/src:設計メモ", "value": {"section": "repo/lib"}}),
    );
    assert!(
        common::checked(&core, "main")
            .get("missingFolders")
            .is_none()
    );
    let names = section_names(&ok(
        &core,
        "section_list",
        json!({"value": {"section": "repo"}}),
    ));
    assert!(!names.contains(&"repo/src".to_string()), "{names:?}");

    // Taken out of the project tree: the note goes where what is taken out with something in it
    // goes, and the folder is bound no more.
    let tree = tree_with(&core, &[]);
    let kept: Vec<Value> = tree["main_sections"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|t| t["name"] != "repo")
        .cloned()
        .collect();
    let mut tree = tree.clone();
    tree["main_sections"] = Value::Array(kept);
    let taken = ok(
        &core,
        "project_update",
        json!({"value": {"project_tree": tree}}),
    );
    assert!(taken.to_string().contains("_evacuation_"), "{taken}");
    let evacuated = ok(
        &core,
        "card_read",
        json!({"target": "_evacuation_/repo/lib:設計メモ"}),
    );
    assert_eq!(evacuated["kind"], "note", "{evacuated}");
    let read = ok(&core, "project_read", json!({}));
    assert!(
        !read["projectTree"].to_string().contains("\"repo\""),
        "{read}"
    );
    fs::remove_dir_all(&repo).unwrap();
}

#[test]
fn a_working_directory_is_made_renamed_and_let_go_of_in_the_project_tree() {
    let core = Core::open(script_store()).unwrap();
    // Listed: made, with what it has by default, in main and every branch.
    ok(
        &core,
        "project_update",
        json!({"value": {"project_tree": tree_with(&core, &[json!({"type": "workdir", "name": "code"})])}}),
    );
    let read = ok(&core, "section_read", json!({"target": "code"}));
    assert_eq!(
        (&read["type"], &read["readType"], &read["folder"]),
        (&json!("workdir"), &json!("git"), &Value::Null),
        "{read}"
    );
    let tree = ok(&core, "project_read", json!({}))["projectTree"].clone();
    let entry = tree["main_sections"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "code")
        .unwrap()
        .clone();
    let id = entry["directory"].as_str().unwrap().to_string();
    // Its description is the project's, in every tree.
    ok(&core, "branch_create", json!({"target": "work"}));
    ok(
        &core,
        "section_update",
        json!({"target": "code", "value": {"description": "サーバーのコード"}}),
    );
    let in_branch = ok(
        &core,
        "section_read",
        json!({"branch": "work", "target": "code"}),
    );
    assert_eq!(in_branch["description"], "サーバーのコード", "{in_branch}");
    // Renamed as any section directly under the root: the same working directory.
    ok(
        &core,
        "project_update",
        json!({"value": {"renames": [{"top_section": "code", "to": "server"}], "project_tree": {
            "main_sections": tree["main_sections"].as_array().unwrap().iter().map(|t| {
                if t["name"] == "code" { json!({"type": "workdir", "name": "server"}) } else { t.clone() }
            }).collect::<Vec<_>>(),
            "board_sections": tree["board_sections"].clone()
        }}}),
    );
    let tree = ok(&core, "project_read", json!({}))["projectTree"].clone();
    let renamed = tree["main_sections"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "server")
        .unwrap()
        .clone();
    assert_eq!(renamed["directory"], id.as_str(), "{tree}");
    let read = ok(&core, "section_read", json!({"target": "server"}));
    assert_eq!(read["description"], "サーバーのコード", "{read}");
    // A workdir entry's directory names one of the project's, or none.
    let mut bad = tree.clone();
    bad["main_sections"]
        .as_array_mut()
        .unwrap()
        .push(json!({"type": "workdir", "name": "x", "directory": "nope"}));
    let said = refused(
        &core,
        "project_update",
        json!({"value": {"project_tree": bad}}),
    );
    assert!(said.contains("作業ディレクトリではありません"), "{said}");
    // Under the board's root, none.
    let mut board = tree.clone();
    board["board_sections"]
        .as_array_mut()
        .unwrap()
        .push(json!({"type": "workdir", "name": "y"}));
    refused(
        &core,
        "project_update",
        json!({"value": {"project_tree": board}}),
    );
    // Taken out with nothing in it: gone, from main and every branch.
    let kept: Vec<Value> = tree["main_sections"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|t| t["name"] != "server")
        .cloned()
        .collect();
    ok(
        &core,
        "project_update",
        json!({"value": {"project_tree": {"main_sections": kept, "board_sections": tree["board_sections"].clone()}}}),
    );
    for branch in ["main", "work"] {
        let names = section_names(&ok(&core, "section_list", json!({"branch": branch})));
        assert!(!names.contains(&"server".to_string()), "{names:?}");
    }
}

#[test]
fn the_tree_of_before_is_named_no_more() {
    let core = Core::open(script_store()).unwrap();
    let said = refused(&core, "card_list", json!({"branch": "_directory_"}));
    assert!(
        said.contains("main と各 branch の根の直下のセクションになりました"),
        "{said}"
    );
    let said = refused(&core, "inspection_list", json!({"branch": "_directory_"}));
    assert!(said.contains("branch を省いて"), "{said}");
    // Nor a branch named so: a name wrapped in _ is the app's, and "directory" is any branch's.
    let said = refused(&core, "branch_create", json!({"target": "_directory_"}));
    assert!(said.contains("アプリが使う名前"), "{said}");
    ok(&core, "branch_create", json!({"target": "directory"}));
    // The tools of before are gone, each saying what took its place.
    let message = json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call",
        "params": {"name": "directory_read", "arguments": {"project": PROJECT}}});
    let reply = op(
        &core,
        json!({"op": "mcp", "message": message, "headers": common::headers(&core)}),
    );
    assert!(reply.to_string().contains("section_read"), "{reply}");
}

#[test]
fn a_note_put_in_a_working_directory_goes_with_its_branch_and_is_merged_as_any_card() {
    let core = Core::open(script_store()).unwrap();
    let repo = repository();
    bind(&core, &repo);
    ok(&core, "branch_create", json!({"target": "work"}));
    ok(
        &core,
        "card_create",
        json!({"branch": "work", "target": "repo/src:メモ", "value": {"kind": "note", "body": "a.rs を分ける"}}),
    );
    // What the branch changed: the note alone, not what was read.
    let diff = ok(&core, "branch_read", json!({"target": "work"}));
    assert_eq!(diff["made"], json!(["repo/src:メモ"]), "{diff}");
    assert!(diff.get("sectionsMade").is_none(), "{diff}");
    let said = refused(&core, "card_read", json!({"target": "repo/src:メモ"}));
    assert!(said.contains("メモ"), "{said}");
    // Merged into main: there, in the folder it was put in.
    ok(&core, "branch_merge", json!({"target": "work"}));
    let note = ok(&core, "card_read", json!({"target": "repo/src:メモ"}));
    assert_eq!(note["body"], "a.rs を分ける", "{note}");
    // Under main's lock, a note is held as any card is.
    let locked = op(
        &core,
        json!({"op": "main_lock", "project": project_id(&core), "on": true}),
    );
    assert!(locked.get("error").is_none(), "{locked}");
    let said = refused(
        &core,
        "card_update",
        json!({"target": "repo/src:メモ", "value": {"body": "書かない"}}),
    );
    assert!(said.contains("ロック"), "{said}");
    fs::remove_dir_all(&repo).unwrap();
}

#[test]
fn a_project_file_before_25_has_its_working_directories_made_sections() {
    let mut file: Value = serde_json::from_str(&common::project_file("sample")).unwrap();
    // As version 24 wrote it: the working directories in the project's row alone.
    file["version"] = 24.into();
    let kinds = &mut file["project"]["kinds"]["project_tree"]["main_sections"];
    let kept: Vec<Value> = kinds
        .as_array()
        .unwrap()
        .iter()
        .filter(|t| t["type"] != "workdir")
        .cloned()
        .collect();
    *kinds = Value::Array(kept);
    for dir in file["project"]["export_dirs"].as_array_mut().unwrap() {
        let name = dir["name"]
            .as_str()
            .unwrap()
            .trim_end_matches("_2")
            .to_string();
        dir["name"] = name.into();
    }
    let sections: Vec<Value> = file["sections"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|s| !s["id"].as_str().unwrap().starts_with("dir:"))
        .cloned()
        .collect();
    file["sections"] = Value::Array(sections);
    file["project"]["mcp_writes"] = json!({"directory.path": "allow"});
    let place = common::place("before-25.sqlite");
    let core = Core::create(&place).unwrap();
    let read = op(
        &core,
        json!({"op": "project_file_read", "text": file.to_string()}),
    );
    assert!(read.get("error").is_none(), "{read}");
    let tree = ok(&core, "project_read", json!({}))["projectTree"]["main_sections"].clone();
    let workdirs: Vec<&str> = tree
        .as_array()
        .unwrap()
        .iter()
        .filter(|t| t["type"] == "workdir")
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert_eq!(workdirs, ["ts", "zod", "schema", "charts_2", "docs_2"]);
    let names = section_names(&ok(&core, "section_list", json!({})));
    assert!(names.contains(&"charts_2".to_string()), "{names:?}");
    // What the file allowed is not read in (a file could hand itself any permission): binding a
    // folder is asked each time, as in any project read in.
    let mcp = ok(&core, "project_read", json!({}));
    assert!(
        mcp["mcpAsk"].to_string().contains("section_update.folder"),
        "{mcp}"
    );
}

#[test]
fn a_project_file_read_as_a_copy_keeps_its_working_directories_whole() {
    let core = Core::open(script_store()).unwrap();
    ok(
        &core,
        "section_update",
        json!({"target": "ts", "value": {"readType": "basic", "description": "型"}}),
    );
    let text = op(
        &core,
        json!({"op": "project_file", "project": project_id(&core)}),
    )["result"]["text"]
        .as_str()
        .unwrap()
        .to_string();
    let copy = op(
        &core,
        json!({"op": "project_file_read", "text": text, "landing": "copy", "name": "写し"}),
    );
    assert!(copy.get("error").is_none(), "{copy}");
    let read = op(
        &core,
        json!({"op": "tool", "name": "section_read", "arguments": {"project": "写し", "target": "ts"}}),
    )["result"]["json"]
        .clone();
    assert_eq!(
        (&read["type"], &read["readType"], &read["description"]),
        (&json!("workdir"), &json!("basic"), &json!("型")),
        "{read}"
    );
    let detail = op(
        &core,
        json!({"op": "workdir_detail", "project": copy["result"]["project"]}),
    );
    let ts = &detail["result"]["dirs"][0];
    assert_eq!(
        ts["section"],
        format!("dir:{}", ts["id"].as_str().unwrap()).as_str(),
        "{detail}"
    );
}

#[test]
fn a_core_opened_after_another_on_the_same_thread_reads_its_own_reading() {
    // What one core made of its readings is kept on the thread by the reading's turn: a second core
    // of the process, its readings counted from the start again, must not be given the first one's.
    let store = script_store();
    let repo = repository();
    {
        let first = Core::open(&store).unwrap();
        bind(&first, &repo);
        assert!(paths(&first, "main").contains(&"src/a.rs".to_string()));
    }
    let second = Core::open(&store).unwrap();
    let read = paths(&second, "main");
    assert!(read.contains(&"src/a.rs".to_string()), "{read:?}");
    let listed = ok(
        &second,
        "section_list",
        json!({"value": {"section": "repo"}}),
    );
    assert!(
        section_names(&listed).contains(&"repo/src".to_string()),
        "{listed}"
    );
    fs::remove_dir_all(&repo).unwrap();
}
