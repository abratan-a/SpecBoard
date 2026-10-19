//! main's and a branch's cards carried out in the working directories' files: a line of the kind
//! `implementedBy` to a file written by its path in the tree (`repo/src:'a.rs'`), kept by an id
//! made of the directory and the path, so a file not made yet can be named; said not there by the
//! check where its working directory was read; the files to change with a card in its impact; the
//! cards a file carries out read from it; the lines in what the window reads. No other kind of line
//! of main's reaches a file, the form of before is refused with the path of now, and the board's
//! cards reach one by a link alone.

use crate::common;

use common::{checked, mcp, ok, op, project_id, refused, script_store};
use serde_json::{Value, json};
use specboard_core::Core;
use std::fs;
use std::path::PathBuf;

const ORDER: &str = "Schemas/Ordering:Order";

/// The project tree with the working directory `repo` listed.
fn list_repo(core: &Core) {
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

/// A folder of two files, the project's working directory `repo` bound to it.
fn bound(core: &Core) -> PathBuf {
    let root = common::place("files");
    for path in ["src/a.rs", "README.md"] {
        let file = root.join(path);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(file, "?").unwrap();
    }
    list_repo(core);
    let said = op(
        core,
        json!({"op": "tool", "name": "section_update", "arguments": {
            "project": common::PROJECT, "target": "repo", "value": {"folder": root.to_string_lossy()}
        }}),
    );
    assert!(said.get("error").is_none(), "{said}");
    root
}

/// `extra` drawn from the card at `path`, after the lines it has.
fn add_lines(core: &Core, branch: Option<&str>, path: &str, extra: Value) -> Result<Value, String> {
    // Added, the lines it has kept (`card_update`'s `{add, remove}`).
    let mut write = json!({"target": path, "value": {"connections": {"add": extra}}});
    if let Some(b) = branch {
        write["branch"] = b.into();
    }
    mcp(core, "card_update", write)
}

fn lines_of(core: &Core, branch: &str, path: &str) -> Value {
    ok(
        core,
        "card_read",
        json!({"branch": branch, "target": path, "value": {"include": ["connections"]}}),
    )["connections"]
        .clone()
}

/// The paths the check says do not resolve.
fn broken(core: &Core, tree: &str) -> String {
    checked(core, tree)["brokenPaths"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|i| i["path"].as_str().unwrap_or_default().to_string())
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn a_card_is_carried_out_in_a_file_kept_by_its_path() {
    let core = Core::open(script_store()).unwrap();
    let root = bound(&core);
    // Written by its path in the tree, there or not yet: read back so.
    add_lines(
        &core,
        None,
        ORDER,
        json!([
            {"line": "implementedBy", "to": "repo/src:'a.rs'"},
            {"line": "implementedBy", "from": "id", "to": "repo:'README.md'"},
            {"line": "implementedBy", "to": "repo/src:'new.rs'"}
        ]),
    )
    .unwrap();
    let lines = lines_of(&core, "main", ORDER);
    let tos: Vec<&str> = lines
        .as_array()
        .unwrap()
        .iter()
        .filter(|l| l["line"] == "implementedBy")
        .map(|l| l["to"].as_str().unwrap())
        .collect();
    assert_eq!(
        tos,
        ["repo/src:'a.rs'", "repo:'README.md'", "repo/src:'new.rs'"],
        "{lines}"
    );
    // Kept by an id made of the directory and the path, as before.
    let kept: String = rusqlite::Connection::open(core.place())
        .unwrap()
        .query_row(
            "SELECT doc FROM card_docs WHERE branch_id = '' AND name = 'Order'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(kept.contains("\"to\":\"file:"), "{kept}");
    assert!(kept.contains("/src/new.rs\""), "{kept}");
    // One not made yet: said so where its working directory was read; made and read again, gone.
    assert!(
        broken(&core, "main").contains("repo/src:'new.rs'"),
        "{}",
        broken(&core, "main")
    );
    fs::write(root.join("src/new.rs"), "?").unwrap();
    ok(
        &core,
        "section_action",
        json!({"target": "repo", "op": "read"}),
    );
    assert!(
        !broken(&core, "main").contains("new.rs"),
        "{}",
        broken(&core, "main")
    );

    // What a place may draw to a file: the kind that reaches files alone (the window's つなぐ).
    let lines = ok(
        &core,
        "card_read",
        json!({"target": ORDER, "value": {"as": "lineKinds", "to": "repo/src:'a.rs'"}}),
    );
    let kinds: Vec<&str> = lines["lines"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|l| l["line"].as_str())
        .collect();
    assert_eq!(kinds, ["implementedBy"], "{lines}");
    assert_eq!(lines["to"], "repo/src:'a.rs'", "{lines}");
    // And to a card: no line that reaches files.
    let lines = ok(
        &core,
        "card_read",
        json!({"target": ORDER, "value": {"as": "lineKinds", "to": "Schemas:Status"}}),
    );
    assert!(
        !lines["lines"].to_string().contains("implementedBy"),
        "{lines}"
    );

    // The files to change with it, in its impact.
    let impact = ok(
        &core,
        "card_read",
        json!({"target": ORDER, "value": {"as": "impact"}}),
    );
    let files: Vec<&str> = impact["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["file"].as_str().unwrap())
        .collect();
    assert_eq!(
        files,
        ["repo/src:'a.rs'", "repo:'README.md'", "repo/src:'new.rs'"],
        "{impact}"
    );
    // A card embedded in it: the files of the card that embeds it too.
    let line = ok(
        &core,
        "card_read",
        json!({"target": "Schemas/Ordering:Line", "value": {"as": "impact"}}),
    );
    assert_eq!(line["files"][0]["file"], "repo/src:'a.rs'", "{line}");
    assert_eq!(line["files"][0]["for"], ORDER, "{line}");

    // The other way round: the file read with the tree's cards it carries out.
    let a = ok(&core, "card_read", json!({"target": "repo/src:'a.rs'"}));
    assert_eq!(a["referencedBy"], json!([ORDER]), "{a}");
    let readme = ok(&core, "card_read", json!({"target": "repo:'README.md'"}));
    assert_eq!(
        readme["referencedBy"],
        json!([format!("{ORDER}.id")]),
        "{readme}"
    );

    // What the window reads: main's lines to the files, the files not in it.
    let project = project_id(&core);
    let detail = op(&core, json!({"op": "project_detail", "project": project}));
    let to_file = detail["result"]["relations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["line"] == "implementedBy")
        .unwrap()
        .clone();
    assert!(
        to_file["toSchemaId"].as_str().unwrap().starts_with("file:"),
        "{to_file}"
    );
    assert_eq!(to_file["outside"], true, "{to_file}");
    assert!(
        !detail["result"]["schemas"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["kind"] == "file"),
        "the files are workdir_detail's"
    );
    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn only_a_line_that_reaches_files_reaches_one_and_the_board_links_alone() {
    let core = Core::open(script_store()).unwrap();
    let root = bound(&core);
    // Another kind of line of main's to a file.
    let said = add_lines(
        &core,
        None,
        ORDER,
        json!([{"line": "link", "to": "repo/src:'a.rs'"}]),
    )
    .unwrap_err();
    assert!(said.contains("implementedBy"), "{said}");
    // The kind that reaches files, to a card.
    let said = add_lines(
        &core,
        None,
        ORDER,
        json!([{"line": "implementedBy", "to": "Schemas:Status"}]),
    )
    .unwrap_err();
    assert!(
        said.contains(
            "実装（implementedBy）: 相手は fileの頭・関数と型 です（Schemas:Status は enum）"
        ),
        "{said}"
    );
    // Inside a file: at a function or a type in it (`code_symbols`), said past the file's path.
    add_lines(
        &core,
        None,
        ORDER,
        json!([{"line": "implementedBy", "to": "repo/src:'a.rs'", "at": "a"}]),
    )
    .unwrap();
    assert!(
        lines_of(&core, "main", ORDER)
            .as_array()
            .unwrap()
            .contains(&json!({"line": "implementedBy", "to": "repo/src:'a.rs'.a"})),
        "{}",
        lines_of(&core, "main", ORDER)
    );
    // The form of before: refused, with the path of now.
    let said = add_lines(
        &core,
        None,
        ORDER,
        json!([{"line": "implementedBy", "to": "file:repo/src/a.rs"}]),
    )
    .unwrap_err();
    assert!(said.contains("repo/src:'a.rs'"), "{said}");
    assert!(said.contains("前の書き方"), "{said}");
    // A working directory the project does not have.
    let said = add_lines(
        &core,
        None,
        ORDER,
        json!([{"line": "implementedBy", "to": "nowhere/src:'a.rs'"}]),
    )
    .unwrap_err();
    assert!(said.contains("nowhere/src:'a.rs'"), "{said}");
    // The board's cards reach a file by a link, and by no line that is the implementation's.
    let said = refused(
        &core,
        "card_create",
        json!({"branch": "_project_board_", "target": "Goals:Code", "value": {"kind": "goal",
            "connections": [{"line": "implementedBy", "to": "repo/src:'a.rs'"}]}}),
    );
    assert!(said.contains("「つながる」の線（link）だけです"), "{said}");
    ok(
        &core,
        "card_create",
        json!({"branch": "_project_board_", "target": "Goals:Code", "value": {"kind": "goal",
            "connections": [{"line": "link", "to": "repo/src:'a.rs'"}]}}),
    );
    let lines = lines_of(&core, "_project_board_", "Goals:Code");
    assert!(lines.to_string().contains("repo/src:'a.rs'"), "{lines}");
    assert!(
        !broken(&core, "_project_board_").contains("repo/src"),
        "{}",
        broken(&core, "_project_board_")
    );
    // A branch's cards reach files too.
    ok(&core, "branch_create", json!({"target": "work"}));
    add_lines(
        &core,
        Some("work"),
        ORDER,
        json!([{"line": "implementedBy", "to": "repo/src:'a.rs'"}]),
    )
    .unwrap();
    let lines = lines_of(&core, "work", ORDER);
    assert!(lines.to_string().contains("repo/src:'a.rs'"), "{lines}");
    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn a_file_of_a_working_directory_not_read_here_is_not_said_missing() {
    let core = Core::open(script_store()).unwrap();
    list_repo(&core);
    // Not bound on this machine: named all the same, and not said missing.
    add_lines(
        &core,
        None,
        ORDER,
        json!([{"line": "implementedBy", "to": "repo/src:'a.rs'"}]),
    )
    .unwrap();
    let lines = lines_of(&core, "main", ORDER);
    assert!(lines.to_string().contains("repo/src:'a.rs'"), "{lines}");
    assert!(
        !broken(&core, "main").contains("repo"),
        "{}",
        broken(&core, "main")
    );
    // Bound then: the line holds the id the file is read as, and the window puts it on the file's card.
    let project = project_id(&core);
    let kept = |core: &Core| -> String {
        let detail = op(core, json!({"op": "project_detail", "project": project}));
        let order = detail["result"]["schemas"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["name"] == "Order")
            .unwrap()
            .clone();
        let line = order["connections"]
            .as_array()
            .unwrap()
            .iter()
            .find(|l| l["line"] == "implementedBy")
            .unwrap()
            .clone();
        line["to"].as_str().unwrap().to_string()
    };
    let before = kept(&core);
    assert!(before.starts_with("file:"), "{before}");
    let root = common::place("files");
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src/a.rs"), "?").unwrap();
    let said = op(
        &core,
        json!({"op": "tool", "name": "section_update", "arguments": {
            "project": common::PROJECT, "target": "repo", "value": {"folder": root.to_string_lossy()}
        }}),
    );
    assert!(said.get("error").is_none(), "{said}");
    assert_eq!(kept(&core), before);
    let read = op(&core, json!({"op": "workdir_detail", "project": project}));
    let files: Vec<&str> = read["result"]["schemas"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|c| c["id"].as_str())
        .collect();
    assert!(files.contains(&before.as_str()), "{before} {files:?}");
    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn a_text_names_a_file_by_its_path_in_the_tree() {
    let core = Core::open(script_store()).unwrap();
    let root = bound(&core);
    ok(
        &core,
        "card_create",
        json!({"target": "Free:Where", "value": {"kind": "note", "body": "[[repo/src:'a.rs']] と [[repo/src:'gone.rs']]"}}),
    );
    let said = broken(&core, "main");
    assert!(!said.contains("'a.rs'"), "{said}");
    assert!(said.contains("repo/src:'gone.rs'"), "{said}");
    fs::remove_dir_all(&root).unwrap();
}
