//! What a key, a file read in and an export reach stays inside what they were given: a working
//! directory's link that leads outside its folder is listed but not read, an export writes nothing
//! through a link in the folder it writes to, and a project read in from a file or moved from
//! another store brings neither what MCP may write without asking nor a slug that names another
//! folder.

use crate::common;

use common::{PROJECT, mcp, ok, op, place, project_id, refused, script_store};
use serde_json::{Value, json};
use specboard_core::Core;
use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

fn folder(name: &str) -> PathBuf {
    let p = place(name);
    fs::create_dir_all(&p).unwrap();
    p
}

/// The working directory `repo` listed in the project tree, and bound to `root` as the window binds it.
fn bind(core: &Core, root: &Path) {
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
            "project": PROJECT, "target": "repo", "value": {"folder": root.to_string_lossy()}}}),
    );
    assert!(bound.get("error").is_none(), "{bound}");
}

/// The window's view of a working directory's file (`file_text`), by its path in `repo`.
fn window_text(core: &Core, path: &str) -> Value {
    let tree = ok(core, "project_read", json!({}))["projectTree"].clone();
    let dir = tree["main_sections"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "repo")
        .unwrap()["directory"]
        .as_str()
        .unwrap()
        .to_string();
    let said = op(
        core,
        json!({"op": "file_text", "project": project_id(core), "id": format!("file:{dir}/{path}")}),
    );
    said["result"].clone()
}

const SECRET: &str = "TOPSECRET";

#[test]
fn a_link_leading_outside_the_folder_is_listed_but_what_it_points_at_is_not_read() {
    let core = Core::open(script_store()).unwrap();
    let outside = folder("outside");
    fs::write(outside.join("secret.txt"), format!("{SECRET} line\n")).unwrap();
    fs::write(outside.join("secret.rs"), "fn topsecret() {}\n").unwrap();
    let repo = folder("repo");
    fs::write(repo.join("plain.txt"), "plain\n").unwrap();
    symlink(repo.join("plain.txt"), repo.join("inside.txt")).unwrap();
    symlink(outside.join("secret.txt"), repo.join("leak.txt")).unwrap();
    symlink(outside.join("secret.rs"), repo.join("leak.rs")).unwrap();
    symlink(&outside, repo.join("ldir")).unwrap();
    symlink(outside.join("made-later.txt"), repo.join("later.txt")).unwrap();
    bind(&core, &repo);

    let listed = ok(
        &core,
        "card_list",
        json!({"value": {"section": "repo", "recursive": true, "limit": 100}}),
    );
    let names: Vec<&str> = listed["cards"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["label"].as_str().unwrap())
        .collect();
    for name in ["leak.txt", "leak.rs", "ldir", "inside.txt"] {
        assert!(names.contains(&name), "{name}: {names:?}");
    }

    let refusal = |path: &str| {
        format!(
            "{path} は作業ディレクトリのフォルダの外を指すシンボリックリンクなので、中身は読みません"
        )
    };
    for (target, path) in [
        ("repo:'leak.txt'", "leak.txt"),
        ("repo:'leak.rs'", "leak.rs"),
        ("repo:'later.txt'", "later.txt"),
    ] {
        // Its card says nothing of what it points at.
        let read = ok(&core, "card_read", json!({"target": target}));
        assert!(!read.to_string().contains("topsecret"), "{read}");
        if path.ends_with(".rs") {
            assert_eq!(read["problem"], refusal(path), "{read}");
        }
        // Its text is refused, and says why.
        for value in [
            json!({"lines": {"from": 1, "to": 3}}),
            json!({"find": SECRET}),
            json!({"find": "topsecret"}),
        ] {
            let why = refused(
                &core,
                "card_read",
                json!({"target": target, "value": value}),
            );
            assert_eq!(why, refusal(path), "{target} {value}");
        }
    }
    // A link whose target is in the folder is read as that file.
    let inside = ok(
        &core,
        "card_read",
        json!({"target": "repo:'inside.txt'", "value": {"lines": {"from": 1, "to": 1}}}),
    );
    assert_eq!(inside["body"], "plain", "{inside}");

    // The functions and types of what the folder holds, none of what lies outside.
    let symbols = ok(
        &core,
        "card_list",
        json!({"value": {"kind": "symbol", "section": "repo"}}),
    );
    assert!(!symbols.to_string().contains("topsecret"), "{symbols}");

    // The window reads through the core: the same.
    for path in ["leak.txt", "ldir/secret.txt", "later.txt"] {
        let shown = window_text(&core, path);
        assert!(shown.get("text").is_none(), "{path}: {shown}");
        assert_eq!(shown["problem"], refusal(path), "{shown}");
    }
    assert_eq!(window_text(&core, "inside.txt")["text"], "plain\n");
}

/// MCP let write the exports without asking.
fn allow_exports(core: &Core) {
    let set = op(
        core,
        json!({"op": "mcp_writes_set", "project": project_id(core), "group": "export", "state": "allow"}),
    );
    assert!(set.get("error").is_none(), "{set}");
}

fn listing(root: &Path) -> Vec<String> {
    let mut out = Vec::new();
    fn walk(base: &Path, at: &Path, out: &mut Vec<String>) {
        for e in fs::read_dir(at).unwrap().flatten() {
            let p = e.path();
            let rel = p.strip_prefix(base).unwrap().to_string_lossy().to_string();
            let meta = fs::symlink_metadata(&p).unwrap();
            if meta.is_dir() {
                walk(base, &p, out);
            } else {
                out.push(rel);
            }
        }
    }
    walk(root, root, &mut out);
    out.sort();
    out
}

fn export(core: &Core, folder: &Path, name: &str, dry_run: bool) -> Result<Value, String> {
    let mut arguments = json!({"targetdir": folder.to_string_lossy(), "target": name});
    if dry_run {
        arguments["value"] = json!({"dryRun": true});
    }
    mcp(core, "export_execute", arguments)
}

#[test]
fn an_export_writes_nothing_through_a_link_in_the_folder_it_writes_to() {
    let core = Core::open(script_store()).unwrap();
    allow_exports(&core);
    let outside = folder("outside");
    fs::write(outside.join("victim.ts"), "ORIGINAL").unwrap();
    let said = |folder: &Path, link: &str| {
        format!(
            "{} の中の {link} がシンボリックリンクなので、エクスポートしませんでした（リンクの先はフォルダの外かもしれないので、リンクを通しては書きません）。リンクを外すか、別のフォルダにエクスポートしてください",
            folder.to_string_lossy()
        )
    };

    // A file it writes is a link.
    let target = folder("file-link");
    symlink(outside.join("victim.ts"), target.join("types.ts")).unwrap();
    let why = export(&core, &target, "ts", false).unwrap_err();
    assert_eq!(why, said(&target, "types.ts"));
    assert_eq!(
        fs::read_to_string(outside.join("victim.ts")).unwrap(),
        "ORIGINAL"
    );
    assert_eq!(listing(&target), ["types.ts"], "nothing else written");
    let dry = export(&core, &target, "ts", true).unwrap();
    assert_eq!(dry["problems"], json!([said(&target, "types.ts")]), "{dry}");

    // A folder on the way to the files it writes is a link.
    let target = folder("dir-link");
    let elsewhere = folder("elsewhere");
    symlink(&elsewhere, target.join("notes")).unwrap();
    let why = export(&core, &target, "docs", false).unwrap_err();
    assert_eq!(why, said(&target, "notes"));
    assert!(listing(&elsewhere).is_empty());

    // The folder it keeps what it wrote in is a link.
    let target = folder("mark-link");
    let marks = folder("marks");
    symlink(&marks, target.join(".specboard")).unwrap();
    let why = export(&core, &target, "ts", false).unwrap_err();
    assert_eq!(why, said(&target, ".specboard"));
    assert!(listing(&marks).is_empty());
    assert_eq!(listing(&target), [".specboard"], "nothing else written");

    // The folder given is the one written to, wherever it really is.
    let real = folder("real");
    let given = place("given");
    symlink(&real, &given).unwrap();
    export(&core, &given, "ts", false).unwrap();
    assert!(listing(&real).contains(&"types.ts".to_string()));
}

/// The sample's project file with `edit` made to it.
fn sample_with(edit: impl Fn(&mut Value)) -> String {
    let mut file: Value = serde_json::from_str(&common::project_file("sample")).unwrap();
    edit(&mut file);
    file.to_string()
}

/// A file that lets MCP write anywhere without asking.
fn letting_everything(file: &mut Value) {
    file["project"]["mcp_writes"] = json!({
        "export.dir": "allow", "export.default": "allow", "section_update.folder": "allow",
        "export_create": "allow", "export_update": "allow", "card_delete": "allow"});
}

/// What MCP may write without asking in the project `project`, as the window reads it: the items
/// not allowed.
fn asked_or_denied(core: &Core, project: &str) -> Vec<String> {
    let view = op(core, json!({"op": "mcp_writes", "project": project}))["result"].clone();
    let mut out = Vec::new();
    for group in view["groups"].as_array().into_iter().flatten() {
        for item in group["items"].as_array().into_iter().flatten() {
            if item["state"] != "allow" {
                out.push(item["id"].as_str().unwrap().to_string());
            }
        }
    }
    out
}

#[test]
fn a_project_file_brings_no_permission_to_write_without_asking() {
    let defaults = {
        let core = Core::create(place("defaults.sqlite")).unwrap();
        op(
            &core,
            json!({"op": "project_file_read", "text": common::project_file("sample")}),
        );
        asked_or_denied(&core, &project_id(&core))
    };
    assert!(defaults.contains(&"export.dir".to_string()), "{defaults:?}");

    // A project new to the store.
    let core = Core::create(place("new.sqlite")).unwrap();
    let read = op(
        &core,
        json!({"op": "project_file_read", "text": sample_with(letting_everything)}),
    );
    assert!(read.get("error").is_none(), "{read}");
    assert_eq!(asked_or_denied(&core, &project_id(&core)), defaults);
    let why = refused(
        &core,
        "export_execute",
        json!({"targetdir": folder("anywhere").to_string_lossy(), "target": "ts"}),
    );
    assert!(why.contains("export.dir"), "{why}");

    // A copy beside it.
    let copy = op(
        &core,
        json!({"op": "project_file_read", "text": sample_with(letting_everything), "landing": "copy", "name": "写し"}),
    );
    let copy = copy["result"]["project"].as_str().unwrap().to_string();
    assert_eq!(asked_or_denied(&core, &copy), defaults);

    // In place of the project: what this store let it do stays.
    let set = op(
        &core,
        json!({"op": "mcp_writes_set", "project": project_id(&core), "item": "card_update", "state": "deny"}),
    );
    assert!(set.get("error").is_none(), "{set}");
    let here = asked_or_denied(&core, &project_id(&core));
    assert!(here.contains(&"card_update".to_string()), "{here:?}");
    let read = op(
        &core,
        json!({"op": "project_file_read", "text": sample_with(letting_everything), "landing": "replace"}),
    );
    assert!(read.get("error").is_none(), "{read}");
    assert_eq!(asked_or_denied(&core, &project_id(&core)), here);
}

/// The slug the store keeps for the project `id`.
fn slug_of(core: &Core, id: &str) -> String {
    rusqlite::Connection::open(core.place())
        .unwrap()
        .query_row("SELECT slug FROM projects WHERE id = ?", [id], |r| r.get(0))
        .unwrap()
}

#[test]
fn a_slug_read_in_names_one_folder() {
    for (given, kept) in [
        ("../../escape", "escape"),
        ("a/b", "a-b"),
        (".hidden", "hidden"),
        ("tab\there", "tab-here"),
        ("ok-slug", "ok-slug"),
        ("見本", "見本"),
    ] {
        let core = Core::create(place("slug.sqlite")).unwrap();
        let read = op(
            &core,
            json!({"op": "project_file_read", "text": sample_with(|f| f["project"]["slug"] = given.into())}),
        );
        assert!(read.get("error").is_none(), "{given}: {read}");
        assert_eq!(slug_of(&core, &project_id(&core)), kept, "{given}");
    }
}

#[test]
fn a_project_moved_from_another_store_brings_no_permission_and_no_slug_naming_another_folder() {
    let here = Core::open(script_store()).unwrap();
    allow_exports(&here);
    let project = project_id(&here);
    let mut bundle =
        op(&here, json!({"op": "project_bundle", "project": project}))["result"].clone();
    let row = &mut bundle["tables"]["projects"][0];
    assert!(
        row["mcp_writes"].as_str().unwrap().contains("allow"),
        "{row}"
    );
    row["slug"] = "../../escape".into();

    let there = Core::create(place("there.sqlite")).unwrap();
    let read = op(
        &there,
        json!({"op": "project_bundle_read", "bundle": bundle}),
    );
    assert!(read.get("error").is_none(), "{read}");
    assert_eq!(slug_of(&there, &project), "escape");
    let why = refused(
        &there,
        "export_execute",
        json!({"targetdir": folder("anywhere").to_string_lossy(), "target": "ts"}),
    );
    assert!(why.contains("export.dir"), "{why}");
}
