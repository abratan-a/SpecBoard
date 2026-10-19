//! Links into a working directory written out and drawn, the guide of the tools a top-level `op`
//! shapes, a batch's step that points at a later step's card, exports that block only themselves,
//! the counts of a working directory, a board line to a function, a goal's branch set aside, and
//! the functions and types TypeScript and JavaScript files hold.

use crate::common;

use common::{ok, op, refused};
use serde_json::{Value, json};
use specboard_core::Core;
use std::fs;
use std::path::{Path, PathBuf};

const PROJECT_TS: &str = r#"export interface Project {
  id: string;
}

export function canEdit(p: Project, user: string): boolean {
  return p.id === user;
}

export class Store {
  save = (x: number): number => x;
  load(id: string) {
    return id;
  }
}
"#;

/// A repository of 6 files in 3 folders.
fn repository(name: &str) -> PathBuf {
    let root = common::place(name);
    for (path, text) in [
        ("src/domain/project.ts", PROJECT_TS),
        ("src/api/tasks.ts", "export function list() {}\n"),
        ("src/api/more.ts", "export const one = 1;\n"),
        ("README.md", "# todo\n"),
        (".gitignore", "node_modules\n"),
        ("package.json", "{}\n"),
    ] {
        let file = root.join(path);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(file, text).unwrap();
    }
    root
}

/// What the window asks of a tool (`op: tool`), as it is answered.
fn window(core: &Core, tool: &str, mut arguments: Value) -> Value {
    arguments["project"] = "見本".into();
    let said = op(
        core,
        json!({"op": "tool", "name": tool, "arguments": arguments}),
    );
    assert!(said.get("error").is_none(), "{tool}: {said}");
    said["result"]["json"].clone()
}

/// Working directories `names` under main's root, the first bound to `folder` on this machine.
fn mount(core: &Core, names: &[&str], folder: &Path) {
    let mut tree = ok(core, "project_read", json!({}))["projectTree"].clone();
    for name in names {
        tree["main_sections"]
            .as_array_mut()
            .unwrap()
            .push(json!({"type": "workdir", "name": name}));
    }
    ok(
        core,
        "project_update",
        json!({"value": {"project_tree": tree}}),
    );
    window(
        core,
        "section_update",
        json!({"target": names[0], "value": {"folder": folder.to_string_lossy()}}),
    );
}

fn core_with_repo(name: &str) -> (Core, PathBuf) {
    let core = Core::open(common::script_store()).unwrap();
    let repo = repository(name);
    mount(&core, &["repo", "other"], &repo);
    (core, repo)
}

fn allow_exports(core: &Core) {
    let project = common::project_id(core);
    let set = op(
        core,
        json!({"op": "mcp_writes_set", "project": project, "group": "export", "state": "allow"}),
    );
    assert!(set.get("error").is_none(), "{set}");
}

/// A card's id, by its name, in the tree the window names `branch` (main when none).
fn card_id(core: &Core, branch: Option<&str>, name: &str) -> String {
    let project = common::project_id(core);
    let mut request = json!({"op": "project_detail", "project": project});
    if let Some(b) = branch {
        request["branch"] = b.into();
    }
    let detail = op(core, request);
    detail["result"]["schemas"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == name)
        .unwrap_or_else(|| panic!("{name}: {detail}"))["id"]
        .as_str()
        .unwrap()
        .to_string()
}

// MARK: - Links into a working directory

const LINKS: &str = "関数: [[repo/src/domain:'project.ts'.canEdit]] / ファイル: [[repo/src/domain:'project.ts']] / フォルダ: [[repo/src/domain:]] / 根: [[repo:]] / 見せる: [[repo/src/domain:'project.ts'.Store.save|保存]]";

/// A document holding `LINKS` written out by the export `name` into a new folder: the file.
fn exported(core: &Core, name: &str) -> String {
    allow_exports(core);
    let out = common::place(&format!("{name}-out"));
    fs::create_dir_all(&out).unwrap();
    ok(
        core,
        "export_create",
        json!({"target": name, "value": {"entries": [{"from": "Docs:CodeNote", "to": "docs/CodeNote.md"}]}}),
    );
    ok(
        core,
        "export_execute",
        json!({"targetdir": out.to_str().unwrap(), "target": name}),
    );
    fs::read_to_string(out.join("docs/CodeNote.md")).unwrap()
}

#[test]
fn a_link_into_a_working_directory_is_written_out_as_the_path_inside_it() {
    let (core, _repo) = core_with_repo("i32-export");
    ok(
        &core,
        "card_create",
        json!({"value": {"Docs:CodeNote": {"kind": "note", "body": LINKS}}}),
    );
    let written = exported(&core, "links");
    for want in [
        "関数: `src/domain/project.ts#canEdit`",
        "ファイル: `src/domain/project.ts`",
        "フォルダ: `src/domain/`",
        "根: `./`",
        "見せる: 保存 (`src/domain/project.ts#Store.save`)",
    ] {
        assert!(written.contains(want), "{want}\n{written}");
    }
    assert!(!written.contains("~~"), "{written}");
    // As written, read back.
    let md = ok(
        &core,
        "card_read",
        json!({"target": "Docs:CodeNote", "value": {"as": "markdown"}}),
    );
    assert!(
        md.to_string()
            .contains("[[repo/src/domain:'project.ts'.canEdit]]"),
        "{md}"
    );
}

#[test]
fn a_link_into_a_working_directory_is_written_out_the_same_whether_it_is_read_or_not() {
    let (bound, _repo) = core_with_repo("i32-same");
    let unbound = Core::open(common::script_store()).unwrap();
    let mut tree = ok(&unbound, "project_read", json!({}))["projectTree"].clone();
    for name in ["repo", "other"] {
        tree["main_sections"]
            .as_array_mut()
            .unwrap()
            .push(json!({"type": "workdir", "name": name}));
    }
    ok(
        &unbound,
        "project_update",
        json!({"value": {"project_tree": tree}}),
    );
    let mut written = Vec::new();
    for core in [&bound, &unbound] {
        ok(
            core,
            "card_create",
            json!({"value": {"Docs:CodeNote": {"kind": "note", "body": LINKS}}}),
        );
        written.push(exported(core, "same"));
    }
    assert_eq!(written[0], written[1]);
}

#[test]
fn a_link_into_a_working_directory_is_written_into_code_as_the_path_inside_it() {
    let (core, _repo) = core_with_repo("i32-code");
    ok(
        &core,
        "card_create",
        json!({"value": {"Schemas:Owner": {"kind": "entity",
            "description": "権限は [[repo/src/domain:'project.ts'.canEdit|canEdit]] で決まる",
            "fields": [{"name": "id", "type": "string",
                "description": "[[repo/src/domain:'project.ts']] の id"}]}}}),
    );
    allow_exports(&core);
    let out = common::place("i32-code-out");
    fs::create_dir_all(&out).unwrap();
    ok(
        &core,
        "export_create",
        json!({"target": "code", "value": {"entries": [
            {"from": "Schemas:Owner", "to": "ts/owner.ts", "format": "typescript"},
            {"from": "Schemas:Owner", "to": "zod/owner.ts", "format": "zod"},
            {"from": "Schemas:Owner", "to": "json/owner.json", "format": "json-schema"}
        ]}}),
    );
    ok(
        &core,
        "export_execute",
        json!({"targetdir": out.to_str().unwrap(), "target": "code"}),
    );
    for file in ["ts/owner.ts", "zod/owner.ts", "json/owner.json"] {
        let text = fs::read_to_string(out.join(file)).unwrap();
        assert!(
            text.contains("canEdit (src/domain/project.ts#canEdit)"),
            "{file}\n{text}"
        );
        assert!(
            text.contains("src/domain/project.ts の id"),
            "{file}\n{text}"
        );
        assert!(!text.contains("'project.ts'"), "{file}\n{text}");
    }
}

/// What the window's documents are told of `places` written in the card `name`.
fn missing(core: &Core, branch: Option<&str>, name: &str, places: &[&str]) -> Value {
    let project = common::project_id(core);
    let id = card_id(core, branch, name);
    let mut request = json!({"op": "document_parts", "project": project,
        "documents": [{"card": id, "places": places}]});
    if let Some(b) = branch {
        request["branch"] = b.into();
    }
    let said = op(core, request);
    assert!(said.get("error").is_none(), "{said}");
    said["result"]["documents"][0]["missing"].clone()
}

#[test]
fn the_window_draws_a_link_into_a_working_directory_as_a_link() {
    let (core, _repo) = core_with_repo("i32-parts");
    ok(
        &core,
        "card_create",
        json!({"value": {"Docs:CodeNote": {"kind": "note", "body": LINKS}}}),
    );
    let nope = "repo/src/domain:'project.ts'.nope";
    let said = missing(
        &core,
        None,
        "CodeNote",
        &[
            "repo/src/domain:'project.ts'.canEdit",
            "repo/src/domain:'project.ts'.Store.save",
            "repo/src/domain:'project.ts'",
            "repo/src/domain:",
            "repo:",
            "other/src:'a.ts'.f",
            "other/src:",
            nope,
        ],
    );
    let said = said.as_object().unwrap();
    assert_eq!(said.keys().collect::<Vec<_>>(), vec![nope], "{said:?}");
    let hint = said[nope].as_str().unwrap();
    assert!(hint.contains("nope") && hint.contains("の中に"), "{hint}");
}

#[test]
fn a_board_document_draws_a_link_into_a_working_directory_through_main() {
    let (core, _repo) = core_with_repo("i32-board-parts");
    ok(
        &core,
        "card_create",
        json!({"branch": "_project_board_", "value": {"Goals:Doc": {"kind": "task", "body": LINKS}}}),
    );
    let said = missing(
        &core,
        Some("~board"),
        "Doc",
        &[
            "repo/src/domain:'project.ts'.canEdit",
            "repo/src/domain:'project.ts'",
            "repo/src/domain:",
        ],
    );
    assert_eq!(said, json!({}));
}

// MARK: - The guide of a tool a top-level op shapes

#[test]
fn the_guide_says_op_beside_target_and_value_for_each_op() {
    let core = Core::open(common::script_store()).unwrap();
    let said = ok(&core, "guide", json!({"about": "card_action"}));
    let op = said["op"].as_str().unwrap();
    assert!(op.starts_with("start|finish|cancel|done|reviewed"), "{op}");
    assert!(op.contains("target"), "{op}");
    assert!(said["value (op: start)"]["branch"].is_string(), "{said}");
    assert!(said["value (op: cancel)"]["discard"].is_string(), "{said}");
    // An op that takes no value has no table, and nothing is said under value.
    assert!(said.get("value (op: finish)").is_none(), "{said}");
    assert!(said.get("value").is_none(), "{said}");
    assert!(!said.to_string().contains("\"op: "), "{said}");

    let app = ok(&core, "guide", json!({"about": "app_control"}));
    assert!(
        app["op"]
            .as_str()
            .unwrap()
            .starts_with("state|select|view|capture|windows")
    );
    assert!(app.get("value (op: windows)").is_none(), "{app}");
    // What one op's value shares with another's points at it by the same key.
    let text = app.to_string();
    assert!(text.contains("value (op: state)"), "{text}");
    assert!(!text.contains("value.op: "), "{text}");

    let section = ok(&core, "guide", json!({"about": "section_action"}));
    assert!(
        section["op"].as_str().unwrap().starts_with("read"),
        "{section}"
    );
    assert!(section.get("value").is_none(), "{section}");

    // op inside value is still not taken: one way to write it.
    ok(
        &core,
        "card_create",
        json!({"branch": "_project_board_", "value": {"Goals:G": {"kind": "goal"}}}),
    );
    refused(
        &core,
        "card_action",
        json!({"branch": "_project_board_", "target": "Goals:G", "value": {"op": "start"}}),
    );
}

// MARK: - A batch's step that points at a later step's card

fn batch(core: &Core, steps: Value) -> Result<Value, String> {
    common::mcp(
        core,
        "batch",
        json!({"value": {"steps": steps}, "execute": true}),
    )
}

#[test]
fn a_step_pointing_at_a_node_a_later_step_makes_is_told_of_that_step() {
    let core = Core::open(common::script_store()).unwrap();
    let why = batch(
        &core,
        json!([
            {"tool": "card_create", "arguments": {"target": "Charts/Shop:Inv", "value": {"kind": "flow"}}},
            {"tool": "card_create", "arguments": {"target": "Charts/Shop:Inv.open", "value": {"kind": "begin", "trigger": "manual", "connections": [{"line": "next", "to": "done"}]}}},
            {"tool": "card_create", "arguments": {"target": "Charts/Shop:Inv.done", "value": {"kind": "end", "outcome": "success"}}},
        ]),
    )
    .unwrap_err();
    assert!(why.contains("steps[1]"), "{why}");
    assert!(
        why.contains("「done」はあとの steps[2]（card_create）で作ります"),
        "{why}"
    );
    assert!(why.contains("何も書いていません"), "{why}");
    // The same by its path in full.
    let why = batch(
        &core,
        json!([
            {"tool": "card_create", "arguments": {"target": "Charts/Shop:Inv", "value": {"kind": "flow"}}},
            {"tool": "card_create", "arguments": {"target": "Charts/Shop:Inv.open", "value": {"kind": "begin", "trigger": "manual", "connections": [{"line": "next", "to": "Charts/Shop:Inv.done"}]}}},
            {"tool": "card_create", "arguments": {"target": "Charts/Shop:Inv.done", "value": {"kind": "end", "outcome": "success"}}},
        ]),
    )
    .unwrap_err();
    assert!(why.contains("あとの steps[2]"), "{why}");
}

#[test]
fn a_step_pointing_at_a_card_no_step_makes_is_told_nothing_more() {
    let core = Core::open(common::script_store()).unwrap();
    let why = batch(
        &core,
        json!([
            {"tool": "card_create", "arguments": {"target": "Charts/Shop:Inv", "value": {"kind": "flow"}}},
            {"tool": "card_create", "arguments": {"target": "Charts/Shop:Inv.open", "value": {"kind": "begin", "trigger": "manual", "connections": [{"line": "next", "to": "nowhere"}]}}},
            {"tool": "card_create", "arguments": {"target": "Charts/Shop:Inv.done", "value": {"kind": "end", "outcome": "success"}}},
        ]),
    )
    .unwrap_err();
    assert!(why.contains("nowhere"), "{why}");
    assert!(!why.contains("あとの steps"), "{why}");
}

#[test]
fn nodes_made_in_one_step_point_at_each_other_in_any_order() {
    let core = Core::open(common::script_store()).unwrap();
    batch(
        &core,
        json!([{"tool": "card_create", "arguments": {"value": {
            "Charts/Shop:Inv": {"kind": "flow"},
            "Charts/Shop:Inv.open": {"kind": "begin", "trigger": "manual", "connections": [{"line": "next", "to": "done"}]},
            "Charts/Shop:Inv.done": {"kind": "end", "outcome": "success"}
        }}}]),
    )
    .unwrap();
}

// MARK: - Exports

fn export_out(name: &str) -> String {
    let out = common::place(name);
    fs::create_dir_all(&out).unwrap();
    out.to_string_lossy().into_owned()
}

#[test]
fn a_section_entry_whose_kinds_cannot_go_to_its_to_is_refused_when_written() {
    let core = Core::open(common::script_store()).unwrap();
    allow_exports(&core);
    let why = refused(
        &core,
        "export_create",
        json!({"target": "flows", "value": {"entries": [{"from": "Charts/Shop", "to": "flows/", "kinds": ["flow"]}]}}),
    );
    assert!(
        why.contains("kinds の flow は to「flows/」に書けません"),
        "{why}"
    );
    assert!(why.contains(".md"), "{why}");
    ok(
        &core,
        "export_create",
        json!({"target": "flows", "value": {"entries": [{"from": "Charts/Shop", "to": "flows.md", "kinds": ["flow"]}]}}),
    );
    let why = refused(
        &core,
        "export_update",
        json!({"target": "flows", "value": {"entries": {"add": [{"from": "Charts", "to": "more/", "kinds": ["flow"]}]}}}),
    );
    assert!(
        why.contains("kinds の flow は to「more/」に書けません"),
        "{why}"
    );
}

#[test]
fn a_section_entry_that_writes_no_card_is_told() {
    let core = Core::open(common::script_store()).unwrap();
    allow_exports(&core);
    let made = ok(
        &core,
        "export_create",
        json!({"target": "none", "value": {"entries": [{"from": "Charts/Shop", "to": "flows/"}]}}),
    );
    let warnings = made["warnings"].to_string();
    assert!(warnings.contains("この行は今は 1 枚も書きません"), "{made}");
    assert!(warnings.contains("Charts/Shop → flows/"), "{made}");
    let fine = ok(
        &core,
        "export_create",
        json!({"target": "cards", "value": {"entries": [{"from": "Charts/Shop:Checkout", "to": "checkout.md"}]}}),
    );
    assert!(fine.get("warnings").is_none(), "{fine}");
    let updated = ok(
        &core,
        "export_update",
        json!({"target": "cards", "value": {"entries": {"add": [{"from": "Charts", "to": "more/"}]}}}),
    );
    assert!(
        updated["warnings"].to_string().contains("Charts → more/"),
        "{updated}"
    );
}

/// Two exports, `good` writing a flow and `bad` two documents into one file: a problem of `bad`.
fn good_and_bad(core: &Core) {
    allow_exports(core);
    ok(
        core,
        "export_create",
        json!({"target": "good", "value": {"entries": [{"from": "Charts/Shop:Checkout", "to": "checkout.md"}]}}),
    );
    ok(
        core,
        "card_create",
        json!({"value": {"Docs:One": {"kind": "note", "body": "1"}, "Docs:Two": {"kind": "note", "body": "2"}}}),
    );
    ok(
        core,
        "export_create",
        json!({"target": "bad", "value": {"entries": [
            {"from": "Docs:One", "to": "both.md"}, {"from": "Docs:Two", "to": "both.md"}]}}),
    );
}

#[test]
fn an_export_is_written_whatever_another_export_has_wrong() {
    let core = Core::open(common::script_store()).unwrap();
    good_and_bad(&core);
    let out = export_out("i32-good");
    let wrote = ok(
        &core,
        "export_execute",
        json!({"targetdir": out, "target": "good"}),
    );
    assert!(wrote.to_string().contains("checkout.md"), "{wrote}");
    assert!(Path::new(&out).join("checkout.md").exists());
    // All of them: stopped, by bad's problem, which says so.
    let out = export_out("i32-all");
    let why = refused(&core, "export_execute", json!({"targetdir": out}));
    assert!(why.contains("bad"), "{why}");
    assert!(!Path::new(&out).join("checkout.md").exists());
    // Read: each export's own problems, all of them when none is named.
    let good = ok(
        &core,
        "export_read",
        json!({"targetdir": out, "target": "good"}),
    );
    assert!(good.get("problems").is_none(), "{good}");
    let bad = ok(
        &core,
        "export_read",
        json!({"targetdir": out, "target": "bad"}),
    );
    assert!(bad["problems"].to_string().contains("bad"), "{bad}");
    let all = ok(&core, "export_read", json!({"targetdir": out}));
    assert!(all["problems"].to_string().contains("bad"), "{all}");
}

#[test]
fn an_entry_is_taken_away_by_its_from_alone() {
    let core = Core::open(common::script_store()).unwrap();
    allow_exports(&core);
    ok(
        &core,
        "export_create",
        json!({"target": "two", "value": {"entries": [
            {"from": "Charts/Shop:Checkout", "to": "a.md"},
            {"from": "Charts/Shop:Checkout", "to": "b.md"},
            {"from": "Charts/Shop:Refund", "to": "c.md"}
        ]}}),
    );
    let left = ok(
        &core,
        "export_update",
        json!({"target": "two", "value": {"entries": {"remove": ["Charts/Shop:Refund"]}}}),
    );
    assert_eq!(left["entries"].as_array().unwrap().len(), 2, "{left}");
    let left = ok(
        &core,
        "export_update",
        json!({"target": "two", "value": {"entries": {"remove": [{"from": "Charts/Shop:Checkout", "to": "a.md"}]}}}),
    );
    assert_eq!(left["entries"].as_array().unwrap().len(), 1, "{left}");
    // What is not there: the entries there are said.
    let why = refused(
        &core,
        "export_update",
        json!({"target": "two", "value": {"entries": {"remove": [{"from": "Charts/Shop:Checkout", "to": "a.md"}]}}}),
    );
    assert!(
        why.contains("from「Charts/Shop:Checkout」 → a.md の行はありません"),
        "{why}"
    );
    assert!(why.contains("Charts/Shop:Checkout → b.md"), "{why}");
    // A whole list of entries: the shape said with an example.
    let why = refused(
        &core,
        "export_update",
        json!({"target": "two", "value": {"entries": [{"from": "Charts/Shop:Refund", "to": "c.md"}]}}),
    );
    assert!(why.contains("remove: [{from, to?}]"), "{why}");
    let why = refused(
        &core,
        "export_update",
        json!({"target": "two", "value": {"entries": {"remove": [3]}}}),
    );
    assert!(why.contains("{from, to?} か from の字"), "{why}");
}

// MARK: - card_list with query and body

#[test]
fn a_query_without_the_bodies_says_where_it_found_without_the_words() {
    let core = Core::open(common::script_store()).unwrap();
    let with = ok(
        &core,
        "card_list",
        json!({"value": {"query": "Order", "body": true}}),
    );
    let without = ok(
        &core,
        "card_list",
        json!({"value": {"query": "Order", "body": false}}),
    );
    let matches = |v: &Value| -> Vec<Value> {
        v["cards"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|c| c["matches"].as_array().unwrap().clone())
            .collect()
    };
    let (with, without) = (matches(&with), matches(&without));
    assert!(with.iter().any(|m| m.get("excerpt").is_some()), "{with:?}");
    assert_eq!(with.len(), without.len());
    for m in &without {
        assert!(m.get("excerpt").is_none() && m["at"].is_string() && m["where"].is_string());
    }
}

// MARK: - How many files and folders a working directory holds

#[test]
fn a_working_directory_counts_its_files_and_its_folders_apart() {
    let (core, repo) = core_with_repo("i32-counts");
    fs::create_dir_all(repo.join(".git/objects")).unwrap();
    fs::write(repo.join(".git/HEAD"), "ref: refs/heads/main\n").unwrap();
    let read = window(
        &core,
        "section_action",
        json!({"target": "repo", "op": "read"}),
    );
    assert_eq!(
        (read["files"].clone(), read["directories"].clone()),
        (json!(6), json!(3)),
        "{read}"
    );
    let read = ok(&core, "section_read", json!({"target": "repo"}));
    assert_eq!(
        (read["files"].clone(), read["directories"].clone()),
        (json!(6), json!(3)),
        "{read}"
    );
    let list = ok(&core, "section_list", json!({}));
    let row = list["sections"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["section"] == "repo")
        .unwrap()
        .clone();
    assert_eq!(
        (row["files"].clone(), row["directories"].clone()),
        (json!(6), json!(3)),
        "{row}"
    );
    let project = common::project_id(&core);
    let detail = op(&core, json!({"op": "workdir_detail", "project": project}));
    let dir = detail["result"]["dirs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["name"] == "repo")
        .unwrap()
        .clone();
    assert_eq!(
        (dir["files"].clone(), dir["directories"].clone()),
        (json!(6), json!(3)),
        "{dir}"
    );
}

// MARK: - A board line to a function

#[test]
fn a_board_card_links_to_a_function_in_a_working_directory_file() {
    let (core, _repo) = core_with_repo("i32-board-link");
    let board = |v: Value| {
        let mut v = v;
        v["branch"] = "_project_board_".into();
        v
    };
    ok(
        &core,
        "card_create",
        board(json!({"value": {"Goals:T1": {"kind": "task"}}})),
    );
    for to in [
        "repo/src/domain:'project.ts'.canEdit",
        "repo/src/domain:'project.ts'.nope",
        "repo/src/domain:'project.ts'",
    ] {
        ok(
            &core,
            "card_update",
            board(
                json!({"target": "Goals:T1", "value": {"connections": {"add": [{"line": "link", "to": to}]}}}),
            ),
        );
    }
    let read = ok(
        &core,
        "card_read",
        board(json!({"target": "Goals:T1", "value": {"include": ["connections"]}})),
    );
    let tos: Vec<String> = read["connections"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["to"].as_str().unwrap().to_string())
        .collect();
    assert!(
        tos.contains(&"repo/src/domain:'project.ts'.canEdit".to_string()),
        "{read}"
    );
    assert!(
        tos.contains(&"repo/src/domain:'project.ts'".to_string()),
        "{read}"
    );
    // Kept as main's lines to a function are: the file, and where in it.
    let raw = op(
        &core,
        json!({"op": "project_detail", "project": common::project_id(&core), "branch": "~board"}),
    );
    let task = raw["result"]["schemas"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "T1")
        .unwrap()
        .clone();
    let kept = task["connections"].to_string();
    assert!(kept.contains("\"at\":\"canEdit\""), "{kept}");
    assert!(kept.contains("file:"), "{kept}");
    // The one not in the file is said by the board's check.
    let checked = common::checked(&core, "_project_board_").to_string();
    assert!(checked.contains("nope"), "{checked}");
    assert!(!checked.contains("canEdit が"), "{checked}");
}

// MARK: - A goal whose branch was set aside

#[test]
fn a_goal_whose_branch_was_set_aside_is_told_how_to_finish_and_starts_anew_by_another_name() {
    let core = Core::open(common::script_store()).unwrap();
    let board = |v: Value| {
        let mut v = v;
        v["branch"] = "_project_board_".into();
        v
    };
    ok(
        &core,
        "card_create",
        board(json!({"value": {"Goals:G1": {"kind": "goal"}}})),
    );
    let started = ok(
        &core,
        "card_action",
        board(json!({"target": "Goals:G1", "op": "start"})),
    );
    let branch = started["branchCreated"]
        .as_str()
        .unwrap_or("g1")
        .to_string();
    ok(
        &core,
        "card_create",
        json!({"branch": branch, "value": {"Docs:New": {"kind": "note", "body": "x"}}}),
    );
    let merged = ok(&core, "branch_merge", json!({"target": branch}));
    assert!(
        merged["warnings"].to_string().contains("op: finish"),
        "{merged}"
    );
    assert_eq!(
        merged["use"],
        json!({"tool": "card_action", "arguments": {"branch": "_project_board_", "target": "Goals:G1", "op": "finish"}}),
        "{merged}"
    );
    let read = ok(
        &core,
        "card_read",
        board(json!({"target": "Goals:G1", "value": {"include": ["actions"]}})),
    );
    let start = read["actions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["op"] == "start")
        .unwrap()
        .clone();
    assert_eq!(
        start["suggestedBranch"],
        json!(format!("{branch}-2")),
        "{read}"
    );
    let again = ok(
        &core,
        "card_action",
        board(json!({"target": "Goals:G1", "op": "start"})),
    );
    assert_eq!(
        again["branchCreated"],
        json!(format!("{branch}-2")),
        "{again}"
    );
}
