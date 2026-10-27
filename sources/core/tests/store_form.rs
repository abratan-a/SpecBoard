//! The store's form once it is in use: a store made before a change to it is brought up to
//! it once, when it is opened (`core_0002_mcp_writes`: what MCP may write in each project, and
//! an access key's permits split; `core_0004_pr_places`: whether a request to merge into
//! main asks for the boxes the branch moved too; `core_0005_connections`: a project's own kinds
//! in its row, a card's lines as its connections; `core_0006_containers`: a flow's nodes and a
//! goal's items cards inside them; `core_0007_landings`: a designating line landing inside
//! the card it reaches; `core_0008_board_keys`: a goal's fields named in English;
//! `core_0009_merge_requests`: a request to merge called a merge request), keeping
//! what it holds; opened again, nothing is done. A store that
//! holds a project is copied beside it first, as it was (`<name>.before-upgrade-from-<the mark
//! before the first it lacks>.db`: how far it had been brought up),
//! and not opened if it cannot be. And the keys of the Swift app's form, moved with what they
//! could do.

use crate::common;

use common::op;
use rusqlite::{Connection, OpenFlags};
use serde_json::{Value, json};
use specboard_core::Core;
use std::path::{Path, PathBuf};

const MARK: &str = "core_0002_mcp_writes";

/// The card ghosts made cards of their own (`card_ghosts.rs` tries it).
const GHOSTS: &str = "core_0003_card_ghosts";

/// A request to merge into main asking for the boxes the branch moved too
/// (`mr.rs` tries it): the requests' `places`.
const PR_PLACES: &str = "core_0004_pr_places";

/// A card's lines as its connections, and the project's own kinds in its row
/// (`connections.rs` tries it).
const CONNECTIONS: &str = "core_0005_connections";

/// A flow's nodes and a goal's items as cards inside them (`containers.rs`
/// tries it).
const CONTAINERS: &str = "core_0006_containers";

/// A designating line landing inside the card it reaches (`connections.rs`
/// tries it).
const LANDINGS: &str = "core_0007_landings";

/// A goal's fields named in English (below).
const BOARD_KEYS: &str = "core_0008_board_keys";

/// A request to merge called a merge request (below): the table
/// `merge_requests`, a goal's `MR`.
const MERGE_REQUESTS: &str = "core_0009_merge_requests";

/// What is directly under each root written into the project (below): its kinds' `project_tree`.
const PROJECT_TREE: &str = "core_0010_plan_tree";

/// A goal's `branch` named `workBranch` (below).
const WORK_BRANCH: &str = "core_0011_work_branch";

/// The window's holds by the Mac it is on (below): `holds.client`.
const HOLD_CLIENTS: &str = "core_0012_hold_clients";

/// A branch that sees main as it was made (below): `branch_frozen`,
/// `branches.reverts`, `merge_requests.keep`.
const BRANCH_SNAPSHOTS: &str = "core_0013_branch_snapshots";

/// A plan called a project (below): `projects`, every `project_id`.
const PROJECTS: &str = "core_0014_projects";
const EXPORTS: &str = "core_0015_exports";
/// An export holding no working directory: `export_default_folders`.
const EXPORT_TARGETS: &str = "core_0016_export_targets";

/// The change notes gone (below): no `changelog`.
const NO_CHANGELOG: &str = "core_0017_no_changelog";

/// The check's marks (below): `inspection_marks`.
const INSPECTION_MARKS: &str = "core_0018_inspection_marks";

/// The history's lines kept by key (below): `commit_lines`' `text_key` and `text_values`.
const COMMIT_KEYS: &str = "core_0019_commit_keys";

/// The project's export language (below): `projects.export_language`.
const EXPORT_LANGUAGE: &str = "core_0020_export_language";

/// The built-in kind of section `instances` replaced by `definition` (below).
const DEFINITION_KIND: &str = "core_0021_definition_kind";

/// The board's sections the card actions move cards into (below).
const BOARD_SECTIONS: &str = "core_0022_board_sections";

/// The working directories as sections of main and every branch (below).
const WORKDIR_SECTIONS: &str = "core_0023_workdir_sections";

/// The code the implementations' lines were recorded at (below): `code_marks`.
const CODE_MARKS: &str = "core_0024_code_marks";

/// The groups on the diagrams (below): `places` made anew to hold them.
const MAP_GROUPS: &str = "core_0025_map_groups";

/// The built-in names of now (`tests/name_move.rs`).
const BUILTIN_NAMES: &str = "core_0026_builtin_names";

/// A store of the form now named as before `core_0014_projects`: `plans`, every `plan_id`, and the
/// mark let go. What turns a store back into one before an older mark does this too: a store of
/// before was named so.
fn named_before(place: &Path) {
    let conn = Connection::open(place).unwrap();
    let tables: Vec<String> = conn
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table'")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    for table in tables {
        if columns(&conn, &table).iter().any(|c| c == "project_id") {
            conn.execute_batch(&format!(
                "ALTER TABLE {table} RENAME COLUMN project_id TO plan_id"
            ))
            .unwrap();
        }
    }
    conn.execute_batch(&format!(
        "ALTER TABLE projects RENAME TO plans;
         ALTER TABLE project_export_root RENAME TO plan_export_root;
         ALTER TABLE project_public RENAME TO plan_public;
         DELETE FROM _migrations WHERE name = '{PROJECTS}';"
    ))
    .unwrap();
}

/// What turns a store of the form now back into one before `core_0009_merge_requests`: its
/// requests' table by the name of before, and no switch for an AI's merges into a locked main
/// (nor whether a request keeps its branch, `core_0013_branch_snapshots`, unmarked with it).
const REQUESTS_BEFORE: &str = "ALTER TABLE merge_requests DROP COLUMN keep;
             ALTER TABLE merge_requests RENAME TO pull_requests;
             DROP INDEX IF EXISTS merge_requests_number;
             CREATE UNIQUE INDEX pull_requests_number ON pull_requests(project_id, number);
             ALTER TABLE projects DROP COLUMN ai_merge;";

/// Every mark a store of the form has now.
const EVERY: [&str; 26] = [
    "core_0001_kinds",
    MARK,
    GHOSTS,
    PR_PLACES,
    CONNECTIONS,
    CONTAINERS,
    LANDINGS,
    BOARD_KEYS,
    MERGE_REQUESTS,
    PROJECT_TREE,
    WORK_BRANCH,
    HOLD_CLIENTS,
    BRANCH_SNAPSHOTS,
    PROJECTS,
    EXPORTS,
    EXPORT_TARGETS,
    NO_CHANGELOG,
    INSPECTION_MARKS,
    COMMIT_KEYS,
    EXPORT_LANGUAGE,
    DEFINITION_KIND,
    BOARD_SECTIONS,
    WORKDIR_SECTIONS,
    CODE_MARKS,
    MAP_GROUPS,
    BUILTIN_NAMES,
];

/// Where the copy of the store at `place`, as it was before it was brought up from `mark`
/// on, is kept: named by the mark before it, how far the store had been brought up.
fn kept(place: &Path, mark: &str) -> PathBuf {
    let at = EVERY.iter().position(|m| *m == mark).unwrap();
    let done = at.checked_sub(1).map_or("none", |before| EVERY[before]);
    let mut name = place.file_name().unwrap().to_os_string();
    name.push(format!(".before-upgrade-from-{done}.db"));
    place.with_file_name(name)
}

/// The copies kept beside the store at `place`, by the mark each was kept before.
fn copies(place: &Path) -> Vec<&'static str> {
    EVERY
        .into_iter()
        .filter(|mark| kept(place, mark).exists())
        .collect()
}

/// Everything a store holds: each table's and index's statement, and each table's rows
/// (in no order: a copy may number them anew).
fn everything(conn: &Connection) -> Vec<String> {
    let things: Vec<(String, String, Option<String>)> = {
        let mut stmt = conn
            .prepare("SELECT type, name, sql FROM sqlite_master ORDER BY type, name")
            .unwrap();
        stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    };
    let mut out = Vec::new();
    for (kind, name, statement) in things {
        out.push(format!("{kind} {name}: {statement:?}"));
        if kind != "table" {
            continue;
        }
        let mut stmt = conn.prepare(&format!("SELECT * FROM \"{name}\"")).unwrap();
        let n = stmt.column_count();
        let mut rows: Vec<String> = stmt
            .query_map([], |r| {
                (0..n)
                    .map(|i| r.get::<_, rusqlite::types::Value>(i))
                    .collect::<Result<Vec<_>, _>>()
            })
            .unwrap()
            .map(|row| format!("{name} {:?}", row.unwrap()))
            .collect();
        rows.sort();
        out.extend(rows);
    }
    out
}

/// A store at `place` in use, of the form before `core_0004_pr_places` and
/// `core_0007_landings`: a project in it, and its board. Kept as a store of then kept what it
/// wrote: in a rollback journal, not in a write-ahead log.
fn in_use_before(place: &Path) {
    let core = Core::create(place).unwrap();
    let (refused, said) = common::call(&core, "project_create", json!({"project": "前からの"}));
    assert!(!refused, "{said}");
    core.close();
    drop(core);
    Connection::open(place)
        .unwrap()
        .execute_batch(&format!(
            "PRAGMA journal_mode = DELETE;
             {REQUESTS_BEFORE}
             ALTER TABLE pull_requests DROP COLUMN places;
             ALTER TABLE projects DROP COLUMN export_language;
             DELETE FROM _migrations WHERE name IN ('{PR_PLACES}', '{LANDINGS}', '{MERGE_REQUESTS}', '{BRANCH_SNAPSHOTS}', '{EXPORT_LANGUAGE}');"
        ))
        .unwrap();
    named_before(place);
}

/// Why the store at `place` was not opened.
fn not_opened(place: &Path) -> String {
    match Core::open(place) {
        Ok(_) => panic!("開いた: {}", place.display()),
        Err(e) => e.to_string(),
    }
}

fn columns(conn: &Connection, table: &str) -> Vec<String> {
    let mut stmt = conn
        .prepare(&format!("PRAGMA table_info({table})"))
        .unwrap();
    stmt.query_map([], |r| r.get(1))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

fn marks(conn: &Connection) -> Vec<String> {
    let mut stmt = conn
        .prepare("SELECT name FROM _migrations ORDER BY name")
        .unwrap();
    stmt.query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

/// Key name → its items (choose and change how the window looks, take pictures of it, make a
/// project), as the window lists them.
fn permits(core: &Core) -> Vec<(String, String, String, String)> {
    op(core, json!({"op": "access_keys"}))["result"]["keys"]
        .as_array()
        .unwrap()
        .iter()
        .map(|k| {
            (
                k["name"].as_str().unwrap().to_string(),
                k["permits"]["app_control"].as_str().unwrap().to_string(),
                k["permits"]["app_control.capture"]
                    .as_str()
                    .unwrap()
                    .to_string(),
                k["permits"]["project_create"].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

#[test]
fn a_new_store_is_of_the_form_now() {
    let place = common::place("new.sqlite");
    Core::create(&place).unwrap().close();
    let conn = Connection::open(&place).unwrap();
    assert_eq!(
        marks(&conn),
        [
            "core_0001_kinds",
            MARK,
            GHOSTS,
            PR_PLACES,
            CONNECTIONS,
            CONTAINERS,
            LANDINGS,
            BOARD_KEYS,
            MERGE_REQUESTS,
            PROJECT_TREE,
            WORK_BRANCH,
            HOLD_CLIENTS,
            BRANCH_SNAPSHOTS,
            PROJECTS,
            EXPORTS,
            EXPORT_TARGETS,
            NO_CHANGELOG,
            INSPECTION_MARKS,
            COMMIT_KEYS,
            EXPORT_LANGUAGE,
            DEFINITION_KIND,
            BOARD_SECTIONS,
            WORKDIR_SECTIONS,
            CODE_MARKS,
            MAP_GROUPS,
            BUILTIN_NAMES
        ]
    );
    assert!(columns(&conn, "projects").ends_with(&[
        "mcp_writes".to_string(),
        "kinds".to_string(),
        "ai_merge".to_string(),
        "exports".to_string(),
        "export_language".to_string()
    ]));
    assert!(
        columns(&conn, "merge_requests").ends_with(&["places".to_string(), "keep".to_string()])
    );
    // The project's own kinds are its row's: the tables of before are not there.
    assert!(columns(&conn, "card_kinds").is_empty());
    assert!(columns(&conn, "card_fields").is_empty());
    assert_eq!(
        columns(&conn, "access_keys"),
        [
            "id",
            "name",
            "digest",
            "can_control",
            "created_at",
            "used_at",
            "can_capture",
            "can_create"
        ]
    );
}

#[test]
fn a_store_made_before_is_brought_up_once() {
    // A store of the form as it was: no column for what MCP may write, one permit a key.
    let place = common::place("before.sqlite");
    let core = Core::create(&place).unwrap();
    let (made, _) = common::key_for_every_project(&core, "画面の係");
    common::key_for_every_project(&core, "読むだけの係");
    let (refused, said) = common::call(&core, "project_create", json!({"project": "前からの"}));
    assert!(!refused, "{said}");
    core.close();
    drop(core);
    let new_form = columns(&Connection::open(&place).unwrap(), "access_keys");
    let project: String = Connection::open(&place)
        .unwrap()
        .query_row(
            "SELECT id FROM projects WHERE name = '前からの'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    {
        let conn = Connection::open(&place).unwrap();
        // A card of the form before: a field that embeds by its type, a pointer in its field.
        let schemas: String = conn
            .query_row(
                "SELECT id FROM section_docs WHERE project_id = ? AND name = 'Schemas'",
                [&project],
                |r| r.get(0),
            )
            .unwrap();
        let old = json!({ "id": "c-old", "kind": "entity", "name": "Old", "section": schemas, "v": 1,
            "rows": { "fields": [
                { "id": "f1", "fields": { "name": "self", "type": "ref", "ref": "c-old" } },
                { "id": "f2", "fields": { "name": "twin", "pointsAt": [{ "schema": "c-old", "as": "digest" }] } }
            ] } });
        conn.execute(
            "INSERT INTO card_docs (project_id, branch_id, id, section_id, name, kind, doc) VALUES (?, '', 'c-old', ?, 'Old', 'entity', ?)",
            rusqlite::params![project, schemas, old.to_string()],
        )
        .unwrap();
        // A flow of the form before: its nodes its rows, where one sits a box of its own kind.
        let charts: String = conn
            .query_row(
                "SELECT id FROM section_docs WHERE project_id = ? AND name = 'Charts'",
                [&project],
                |r| r.get(0),
            )
            .unwrap();
        let flow = json!({ "id": "c-flow", "kind": "flow", "name": "Save", "section": charts, "v": 2,
            "rows": { "nodes": [
                { "id": "n-start", "fields": { "name": "start", "kind": "begin", "trigger": "manual", "next": ["work"] } },
                { "id": "n-work", "fields": { "name": "work", "kind": "process", "next": ["done"] }, "rows": { "steps": [
                    { "id": "s1", "fields": { "do": "read", "paths": ["Schemas:Old.self"] } }
                ] } },
                { "id": "n-done", "fields": { "name": "done", "kind": "end", "outcome": "success" } }
            ] } });
        conn.execute(
            "INSERT INTO card_docs (project_id, branch_id, id, section_id, name, kind, doc) VALUES (?, '', 'c-flow', ?, 'Save', 'flow', ?)",
            rusqlite::params![project, charts, flow.to_string()],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO places (project_id, branch_id, view, item_kind, item, x, y, pinned) VALUES (?, '', 'c-flow', 'node', 'start', 3, 4, 1)",
            [&project],
        )
        .unwrap();
        // A goal of the form before, holding a task in its list.
        let goals: String = conn
            .query_row(
                "SELECT id FROM section_docs WHERE project_id = ? AND branch_id = '~board' AND name = 'Goals'",
                [&project],
                |r| r.get(0),
            )
            .unwrap();
        for (id, kind, name, fields) in [
            (
                "c-goal",
                "goal",
                "Launch",
                json!({ "状態": "進行中", "items": ["c-task"] }),
            ),
            (
                "c-task",
                "task",
                "Design",
                json!({ "done": false, "body": "[[Goals:Design]] を決める" }),
            ),
        ] {
            let doc = json!({ "id": id, "kind": kind, "name": name, "section": goals, "v": 2, "fields": fields });
            conn.execute(
                "INSERT INTO card_docs (project_id, branch_id, id, section_id, name, kind, doc) VALUES (?, '~board', ?, ?, ?, ?, ?)",
                rusqlite::params![project, id, goals, name, kind, doc.to_string()],
            )
            .unwrap();
        }
        conn.execute_batch(&format!(
            "ALTER TABLE projects DROP COLUMN mcp_writes;
             ALTER TABLE access_keys DROP COLUMN can_capture;
             ALTER TABLE access_keys DROP COLUMN can_create;
             ALTER TABLE access_keys RENAME COLUMN can_control TO can_drive;
             UPDATE access_keys SET can_drive = (id = '{made}');
             DELETE FROM _migrations WHERE name = '{MARK}';
             {REQUESTS_BEFORE}
             ALTER TABLE pull_requests DROP COLUMN places;
             DELETE FROM _migrations WHERE name IN ('{PR_PLACES}', '{BRANCH_SNAPSHOTS}');
             ALTER TABLE projects DROP COLUMN kinds;
             CREATE TABLE card_kinds (project_id TEXT NOT NULL, name TEXT NOT NULL, version INTEGER NOT NULL DEFAULT 1, doc TEXT NOT NULL, PRIMARY KEY (project_id, name));
             CREATE TABLE card_fields (project_id TEXT NOT NULL, kind TEXT NOT NULL, parent TEXT NOT NULL DEFAULT '', sort_order INTEGER NOT NULL DEFAULT 0, name TEXT NOT NULL, doc TEXT NOT NULL, PRIMARY KEY (project_id, kind, parent, name));
             INSERT INTO card_kinds VALUES ('{project}', 'memo', 1, '{{\"label\": \"メモ\"}}');
             DELETE FROM _migrations WHERE name = '{CONNECTIONS}';
             DELETE FROM _migrations WHERE name = '{CONTAINERS}';
             DELETE FROM _migrations WHERE name = '{LANDINGS}';
             DELETE FROM _migrations WHERE name = '{BOARD_KEYS}';
             DELETE FROM _migrations WHERE name = '{MERGE_REQUESTS}';
             ALTER TABLE projects DROP COLUMN exports;
             DROP TABLE export_default_folders;
             DELETE FROM _migrations WHERE name IN ('{EXPORTS}', '{EXPORT_TARGETS}');
             ALTER TABLE projects DROP COLUMN export_language;
             DELETE FROM _migrations WHERE name = '{EXPORT_LANGUAGE}';
             DELETE FROM _migrations WHERE name = '{BUILTIN_NAMES}';"
        ))
        .unwrap();
        assert!(!columns(&conn, "projects").contains(&"mcp_writes".to_string()));
        assert!(!columns(&conn, "pull_requests").contains(&"places".to_string()));
    }

    // Opened: brought up to it, what each key could do carried over.
    let core = Core::open(&place).unwrap();
    {
        let conn = Connection::open(&place).unwrap();
        assert_eq!(
            marks(&conn),
            [
                "core_0001_kinds",
                MARK,
                GHOSTS,
                PR_PLACES,
                CONNECTIONS,
                CONTAINERS,
                LANDINGS,
                BOARD_KEYS,
                MERGE_REQUESTS,
                PROJECT_TREE,
                WORK_BRANCH,
                HOLD_CLIENTS,
                BRANCH_SNAPSHOTS,
                PROJECTS,
                EXPORTS,
                EXPORT_TARGETS,
                NO_CHANGELOG,
                INSPECTION_MARKS,
                COMMIT_KEYS,
                EXPORT_LANGUAGE,
                DEFINITION_KIND,
                BOARD_SECTIONS,
                WORKDIR_SECTIONS,
                CODE_MARKS,
                MAP_GROUPS,
                BUILTIN_NAMES
            ]
        );
        assert_eq!(columns(&conn, "access_keys"), new_form);
        assert!(columns(&conn, "projects").ends_with(&[
            "mcp_writes".to_string(),
            "kinds".to_string(),
            "ai_merge".to_string(),
            "exports".to_string(),
            "export_language".to_string()
        ]));
        assert!(
            columns(&conn, "merge_requests").ends_with(&["places".to_string(), "keep".to_string()])
        );
        assert!(columns(&conn, "pull_requests").is_empty());
        // The project's own kinds of card in its row, the tables let go of; the card's lines
        // its connections, a line of the project's own made for the pointer's word.
        assert!(columns(&conn, "card_kinds").is_empty());
        let kinds: String = conn
            .query_row("SELECT kinds FROM projects WHERE id = ?", [&project], |r| {
                r.get(0)
            })
            .unwrap();
        let kinds: Value = serde_json::from_str(&kinds).unwrap();
        assert_eq!(kinds["cards"][0]["kind"], "memo");
        assert_eq!(kinds["cards"][0]["doc"]["label"], "メモ");
        assert_eq!(kinds["lines"][0]["line"], "digest");
        let doc: String = conn
            .query_row("SELECT doc FROM card_docs WHERE id = 'c-old'", [], |r| {
                r.get(0)
            })
            .unwrap();
        let doc: Value = serde_json::from_str(&doc).unwrap();
        let lines: Vec<(&str, &str)> = doc["connections"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| (c["line"].as_str().unwrap(), c["from"].as_str().unwrap()))
            .collect();
        assert_eq!(lines, [("embed", "self"), ("digest", "twin")]);
        assert!(doc["rows"]["fields"][0]["fields"].get("type").is_none());
        assert!(doc["rows"]["fields"][0]["fields"].get("ref").is_none());
        // The flow's nodes cards inside it, their ways and paths its lines, the box where
        // one sat its card's on the flow's card map.
        let card = |id: &str| -> Value {
            let text: String = conn
                .query_row("SELECT doc FROM card_docs WHERE id = ?", [id], |r| r.get(0))
                .unwrap();
            serde_json::from_str(&text).unwrap()
        };
        let flow = card("c-flow");
        assert!(flow.get("rows").is_none());
        let work = card("n-work");
        assert_eq!(
            (&work["kind"], &work["name"], &work["in"]),
            (&json!("process"), &json!("work"), &json!("c-flow"))
        );
        let lines: Vec<(&str, &str, &str, &str)> = work["connections"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| {
                (
                    c["line"].as_str().unwrap(),
                    c["from"].as_str().unwrap_or_default(),
                    c["to"].as_str().unwrap(),
                    c["at"].as_str().unwrap_or_default(),
                )
            })
            .collect();
        assert_eq!(
            lines,
            [
                ("next", "", "n-done", ""),
                ("reads", "steps[0]", "c-old", "self")
            ]
        );
        assert_eq!(work["rows"]["steps"][0]["id"], "s1");
        let section_id: String = conn
            .query_row(
                "SELECT section_id FROM card_docs WHERE id = 'n-work'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(section_id, "c-flow", "a node's name is its own in its flow");
        let boxes: Vec<(String, String, i64)> = {
            let mut stmt = conn
                .prepare("SELECT item_kind, item, x FROM places WHERE view = 'c-flow'")
                .unwrap();
            stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
                .unwrap()
                .collect::<Result<_, _>>()
                .unwrap()
        };
        assert_eq!(boxes, [("card".to_string(), "n-start".to_string(), 3)]);
        // The goal's task inside it, and the path that named it on from the goal; its fields
        // by their names in English.
        let goal = card("c-goal");
        assert!(goal["fields"].get("items").is_none());
        assert_eq!(goal["fields"], json!({ "status": "inProgress" }));
        let task = card("c-task");
        assert_eq!(task["in"], "c-goal");
        assert_eq!(task["fields"]["body"], "[[Goals:Launch.Design]] を決める");
    }
    let mut keys = permits(&core);
    keys.sort();
    assert_eq!(
        keys,
        [
            ("テストのキー", "ask", "ask", "allow"),
            ("画面の係", "allow", "allow", "allow"),
            ("読むだけの係", "ask", "ask", "allow"),
        ]
        .map(|(n, a, b, c)| (n.to_string(), a.to_string(), b.to_string(), c.to_string()))
    );
    // Every project as it was, and what MCP may write in it the default.
    let view = op(&core, json!({"op": "mcp_writes", "project": project}))["result"].clone();
    let denied: Vec<&str> = view["groups"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|g| g["items"].as_array().unwrap().iter())
        .filter(|i| i["state"] != "allow")
        .map(|i| i["id"].as_str().unwrap())
        .collect();
    assert_eq!(
        denied,
        [
            "project_update.rules",
            "project_update.slug",
            "project_update.main_tree",
            "section_update.folder",
            "export_create",
            "export_update",
            "export_delete",
            "export.dir",
            "export.default",
            "branch_revert.others"
        ]
    );

    // Changed after: opened again, nothing is done over it.
    op(
        &core,
        json!({"op": "access_key_permit", "key": made, "permits": {"app_control.capture": "ask"}}),
    );
    op(
        &core,
        json!({"op": "mcp_writes_set", "project": project, "item": "card_update", "state": "deny"}),
    );
    core.close();
    drop(core);
    let core = Core::open(&place).unwrap();
    let conn = Connection::open(&place).unwrap();
    assert_eq!(
        marks(&conn),
        [
            "core_0001_kinds",
            MARK,
            GHOSTS,
            PR_PLACES,
            CONNECTIONS,
            CONTAINERS,
            LANDINGS,
            BOARD_KEYS,
            MERGE_REQUESTS,
            PROJECT_TREE,
            WORK_BRANCH,
            HOLD_CLIENTS,
            BRANCH_SNAPSHOTS,
            PROJECTS,
            EXPORTS,
            EXPORT_TARGETS,
            NO_CHANGELOG,
            INSPECTION_MARKS,
            COMMIT_KEYS,
            EXPORT_LANGUAGE,
            DEFINITION_KIND,
            BOARD_SECTIONS,
            WORKDIR_SECTIONS,
            CODE_MARKS,
            MAP_GROUPS,
            BUILTIN_NAMES
        ]
    );
    assert_eq!(columns(&conn, "access_keys"), new_form);
    let mine = permits(&core)
        .into_iter()
        .find(|k| k.0 == "画面の係")
        .unwrap();
    assert_eq!(
        mine,
        (
            "画面の係".to_string(),
            "allow".to_string(),
            "ask".to_string(),
            "allow".to_string()
        )
    );
    let kept: Option<String> = conn
        .query_row(
            "SELECT mcp_writes FROM projects WHERE id = ?",
            [&project],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(kept.as_deref(), Some(r#"{"card_update":"deny"}"#));
}

#[test]
fn a_store_in_use_is_kept_as_it_was_before_it_is_brought_up() {
    let place = common::place("kept.sqlite");
    in_use_before(&place);
    // What was written last is in the WAL alone, not in the file yet.
    let writer = Connection::open(&place).unwrap();
    let mode: String = writer
        .query_row("PRAGMA journal_mode = WAL", [], |r| r.get(0))
        .unwrap();
    assert_eq!(mode, "wal");
    writer
        .execute_batch(
            "PRAGMA wal_autocheckpoint = 0; UPDATE plans SET description = 'WAL の中だけ';",
        )
        .unwrap();
    let before = everything(&writer);
    let alone = common::place("kept-alone.sqlite");
    std::fs::copy(&place, &alone).unwrap();
    let in_the_file: String = Connection::open(&alone)
        .unwrap()
        .query_row("SELECT description FROM plans", [], |r| r.get(0))
        .unwrap();
    assert_ne!(in_the_file, "WAL の中だけ");

    // Opened: copied beside it as it was, the WAL's too, before it is brought up.
    let core = Core::open(&place).unwrap();
    assert_eq!(copies(&place), [PR_PLACES]);
    let copy = kept(&place, PR_PLACES);
    assert_eq!(
        copy.file_name().unwrap().to_str().unwrap(),
        format!(
            "{}.before-upgrade-from-core_0003_card_ghosts.db",
            place.file_name().unwrap().to_str().unwrap()
        )
    );
    {
        let kept = Connection::open_with_flags(&copy, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        assert_eq!(everything(&kept), before);
        assert!(!marks(&kept).contains(&PR_PLACES.to_string()));
        let description: String = kept
            .query_row("SELECT description FROM plans", [], |r| r.get(0))
            .unwrap();
        assert_eq!(description, "WAL の中だけ");
    }
    let mut partial = copy.clone().into_os_string();
    partial.push(".partial");
    assert!(!PathBuf::from(partial).exists());
    assert_eq!(marks(&writer), EVERY);
    assert!(
        columns(&writer, "merge_requests").ends_with(&["places".to_string(), "keep".to_string()])
    );

    // Opened again, of the form now: nothing more is kept, the copy left as it was.
    core.close();
    drop(core);
    let made = std::fs::read(&copy).unwrap();
    Core::open(&place).unwrap().close();
    assert_eq!(copies(&place), [PR_PLACES]);
    assert_eq!(std::fs::read(&copy).unwrap(), made);
}

#[test]
fn a_new_store_or_one_of_the_form_now_is_not_kept() {
    // New: nothing in it, every mark there.
    let new = common::place("new-kept.sqlite");
    Core::create(&new).unwrap().close();
    Core::open(&new).unwrap().close();
    // Of the form now, a project in it.
    let now = common::place("now-kept.sqlite");
    let core = Core::create(&now).unwrap();
    let (refused, said) = common::call(&core, "project_create", json!({"project": "今の"}));
    assert!(!refused, "{said}");
    core.close();
    drop(core);
    Core::open(&now).unwrap().close();
    // Of a form before, with nothing in it: brought up, nothing to keep.
    let empty = common::place("empty-kept.sqlite");
    Core::create(&empty).unwrap().close();
    Connection::open(&empty)
        .unwrap()
        .execute_batch(&format!(
            "{REQUESTS_BEFORE}
             ALTER TABLE pull_requests DROP COLUMN places;
             ALTER TABLE projects DROP COLUMN export_language;
             DELETE FROM _migrations WHERE name IN ('{PR_PLACES}', '{LANDINGS}', '{MERGE_REQUESTS}', '{BRANCH_SNAPSHOTS}', '{EXPORT_LANGUAGE}');"
        ))
        .unwrap();
    named_before(&empty);
    Core::open(&empty).unwrap().close();
    assert_eq!(marks(&Connection::open(&empty).unwrap()), EVERY);
    for place in [&new, &now, &empty] {
        assert!(copies(place).is_empty(), "{}", place.display());
    }
}

#[test]
fn a_copy_kept_already_is_not_written_over() {
    let place = common::place("kept-twice.sqlite");
    in_use_before(&place);
    let copy = kept(&place, PR_PLACES);
    std::fs::write(&copy, "前の控え").unwrap();
    Core::open(&place).unwrap().close();
    assert_eq!(std::fs::read_to_string(&copy).unwrap(), "前の控え");
    assert_eq!(marks(&Connection::open(&place).unwrap()), EVERY);
}

/// A store of the form before `core_0008_board_keys`: its goals' fields by their Japanese
/// names, in every tree — main's free section, a branch's copy and its base, the board — and in
/// the history. Opened, it is kept beside it first, then each goal's fields go by their names
/// in English, their values as they were; what a goal has no more (`担当`) stays, 「定義に無い欄」; each
/// tree that had any says so in one commit. Opened again, nothing more is done.
#[test]
fn a_store_before_the_board_keys_has_its_goals_fields_named_in_english_once() {
    let place = common::place("board-keys.sqlite");
    let core = Core::create(&place).unwrap();
    let mcp = |tool: &str, arguments: Value| {
        let mut arguments = arguments;
        arguments["project"] = "前からの".into();
        let (refused, said) = common::call(&core, tool, arguments);
        assert!(!refused, "{tool}: {said}");
        serde_json::from_str::<Value>(&said).unwrap_or(Value::Null)
    };
    let (refused, said) = common::call(&core, "project_create", json!({"project": "前からの"}));
    assert!(!refused, "{said}");
    mcp(
        "card_create",
        json!({"branch": "_project_board_", "value": {
            "Goals:Ship": {"kind": "goal", "status": "inProgress", "priority": "high", "due": "2026-10-31"},
            "Goals:Ship.Design": {"kind": "task", "done": true, "body": "決めた"}
        }}),
    );
    mcp(
        "card_update",
        json!({"branch": "_project_board_", "target": "Goals:Ship", "value": {"label": "1. 出す"}}),
    );
    mcp(
        "card_create",
        json!({"target": "Free:Aim", "value": {"kind": "goal", "status": "notStarted"}}),
    );
    mcp("branch_create", json!({"target": "project-b"}));
    mcp(
        "card_update",
        json!({"branch": "project-b", "target": "Free:Aim", "value": {"label": "狙い"}}),
    );
    let project: String = Connection::open(&place)
        .unwrap()
        .query_row("SELECT id FROM projects", [], |r| r.get(0))
        .unwrap();
    let branch: String =
        op(&core, json!({"op": "branches", "project": project}))["result"]["branches"][0]["id"]
            .as_str()
            .unwrap()
            .to_string();
    core.close();
    drop(core);
    // As it was before: the names of before everywhere a goal is kept, and who had it.
    {
        let conn = Connection::open(&place).unwrap();
        let before = |column: &str| {
            format!(
                "replace(replace(replace(replace({column}, '\"status\":\"inProgress\"', '\"状態\":\"進行中\",\"担当\":\"Claude\"'), '\"status\":', '\"状態\":'), '\"priority\":', '\"優先度\":'), '\"due\":', '\"期限\":')"
            )
        };
        conn.execute_batch(&format!(
            "UPDATE card_docs SET doc = {}, base = {};
             UPDATE commit_rows SET before = {};
             DELETE FROM _migrations WHERE name IN ('{BOARD_KEYS}', '{BUILTIN_NAMES}');",
            before("doc"),
            before("base"),
            before("before")
        ))
        .unwrap();
        let held: i64 = conn
            .query_row(
                "SELECT count(*) FROM card_docs WHERE doc LIKE '%\"状態\"%' OR base LIKE '%\"状態\"%'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(held, 3, "main・branch の写しと基点・ボードの goal");
    }

    let core = Core::open(&place).unwrap();
    assert_eq!(copies(&place), [BOARD_KEYS]);
    let conn = Connection::open(&place).unwrap();
    assert_eq!(marks(&conn), EVERY);
    let docs = |branch: &str, name: &str| -> (Value, Option<Value>) {
        let (doc, base): (String, Option<String>) = conn
            .query_row(
                "SELECT doc, base FROM card_docs WHERE branch_id = ? AND name = ?",
                [branch, name],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        (
            serde_json::from_str(&doc).unwrap(),
            base.map(|b| serde_json::from_str(&b).unwrap()),
        )
    };
    let (ship, _) = docs("~board", "Ship");
    assert_eq!(
        ship["fields"],
        json!({"status": "inProgress", "担当": "Claude", "priority": "high", "due": "2026-10-31", "label": "1. 出す"})
    );
    // The kinds' version now.
    assert_eq!(ship["v"], specboard_core::kinds::builtin().version);
    let (task, _) = docs("~board", "Design");
    assert_eq!(task["fields"], json!({"done": true, "body": "決めた"}));
    let (aim, _) = docs("", "Aim");
    assert_eq!(aim["fields"]["status"], "notStarted");
    let (copy, base) = docs(&branch, "Aim");
    assert_eq!(copy["fields"]["status"], "notStarted");
    assert_eq!(base.unwrap()["fields"]["status"], "notStarted");
    let left: i64 = conn
        .query_row(
            "SELECT count(*) FROM commit_rows WHERE before LIKE '%\"状態\"%' OR before LIKE '%\"優先度\"%' OR before LIKE '%\"期限\"%'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(left, 0, "履歴の前の形も今の名前に");
    // Each tree that had any says so in one commit; the history before it, as it was.
    let newest = |tree: &str| -> (String, Vec<String>) {
        let read = op(
            &core,
            json!({"op": "commits", "project": project, "branch": tree}),
        );
        // Past the built-in names' move, which runs after every other, in a tree it moved.
        let commits = read["result"]["commits"].as_array().unwrap().clone();
        let names = commits[0]["lines"][0]["summary"]
            .as_str()
            .unwrap()
            .contains("組み込みの線の名前");
        let commit = commits[usize::from(names)].clone();
        (
            commit["kind"].as_str().unwrap_or_default().to_string(),
            commit["lines"]
                .as_array()
                .unwrap()
                .iter()
                .map(|l| l["summary"].as_str().unwrap().to_string())
                .collect(),
        )
    };
    for (tree, name) in [("~board", "Ship"), ("", "Aim"), (branch.as_str(), "Aim")] {
        assert_eq!(
            newest(tree),
            (
                "migrate".to_string(),
                vec![format!(
                    "goal 1 枚の欄 状態・優先度・期限 を status・priority・due にした（値はそのまま。{name}）"
                )]
            ),
            "{tree}"
        );
    }
    // Read as MCP reads it: by the names of now, what it has no more apart.
    let (refused, said) = common::call(
        &core,
        "card_read",
        json!({"project": "前からの", "branch": "_project_board_", "target": "Goals:Ship"}),
    );
    assert!(!refused, "{said}");
    let read: Value = serde_json::from_str(&said).unwrap();
    assert_eq!(read["status"], "inProgress");
    assert_eq!(read["strayFields"], json!({"担当": "Claude"}));

    // Opened again: nothing more.
    let count = || -> i64 {
        Connection::open(&place)
            .unwrap()
            .query_row("SELECT count(*) FROM commits", [], |r| r.get(0))
            .unwrap()
    };
    let commits = count();
    core.close();
    drop(core);
    Core::open(&place).unwrap().close();
    assert_eq!(count(), commits);
    assert_eq!(copies(&place), [BOARD_KEYS]);
}

#[test]
fn a_store_that_cannot_be_kept_is_not_opened() {
    // Nothing moved: the store as it was.
    let as_it_was = |place: &Path| {
        let conn = Connection::open(place).unwrap();
        assert!(!marks(&conn).contains(&PR_PLACES.to_string()));
        assert!(!columns(&conn, "pull_requests").contains(&"places".to_string()));
        assert!(copies(place).is_empty());
    };

    // A name the copy's, longer by its ending, is too long for beside it.
    let short = common::place("unkept");
    let mut name = short.file_name().unwrap().to_str().unwrap().to_string();
    name.push_str(&"x".repeat(240 - name.len()));
    let place = short.with_file_name(name);
    in_use_before(&place);
    let said = not_opened(&place);
    assert!(
        said.contains("移す前の控えを取れないので、開きません"),
        "{said}"
    );
    assert!(said.contains(&place.display().to_string()), "{said}");
    assert!(said.contains("控えの所を見られません: "), "{said}");
    as_it_was(&place);

    // A folder that cannot be written in (one run as root writes in it all the same).
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let folder = common::place("unwritable");
        std::fs::create_dir(&folder).unwrap();
        let place = folder.join("specboard-core.db");
        in_use_before(&place);
        std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o555)).unwrap();
        let probe = folder.join("probe");
        let writable = std::fs::write(&probe, "").is_ok();
        let said = (!writable).then(|| not_opened(&place));
        let _ = std::fs::remove_file(&probe);
        std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o755)).unwrap();
        match said {
            Some(said) => {
                assert!(
                    said.contains("移す前の控えを取れないので、開きません"),
                    "{said}"
                );
                assert!(said.contains(&place.display().to_string()), "{said}");
                assert!(said.contains("控えを書けません"), "{said}");
                as_it_was(&place);
            }
            None => eprintln!(
                "書けないフォルダの確かめは飛ばした: {} に書けた（root で流れている）",
                folder.display()
            ),
        }
    }

    // One the copy cannot be taken into, whoever runs it: a folder where it is begun.
    let place = common::place("partial-folder");
    in_use_before(&place);
    let mut partial = kept(&place, PR_PLACES).into_os_string();
    partial.push(".partial");
    std::fs::create_dir(&partial).unwrap();
    let said = not_opened(&place);
    assert!(
        said.contains("移す前の控えを取れないので、開きません"),
        "{said}"
    );
    assert!(said.contains("取りかけの控え"), "{said}");
    assert!(said.contains("を消せません"), "{said}");
    as_it_was(&place);
}

#[test]
fn a_store_a_newer_core_moved_on_is_not_opened_and_left_as_it_is() {
    let place = common::place("newer.sqlite");
    let core = Core::create(&place).unwrap();
    let (refused, said) = common::call(&core, "project_create", json!({"project": "机"}));
    assert!(!refused, "{said}");
    core.close();
    let newer = "core_9999_from_a_newer_core";
    Connection::open(&place)
        .unwrap()
        .execute("INSERT INTO _migrations (name) VALUES (?1)", [newer])
        .unwrap();
    let before = everything(&Connection::open(&place).unwrap());
    let said = not_opened(&place);
    assert_eq!(
        said,
        format!(
            "{} のデータは新しい版の SpecBoard で更新されていて、この版では開けません。SpecBoard を新しくしてください",
            place.display()
        )
    );
    let conn = Connection::open(&place).unwrap();
    assert!(marks(&conn).contains(&newer.to_string()));
    assert_eq!(everything(&conn), before);
    assert!(copies(&place).is_empty());
}

/// A store before `core_0009_merge_requests`: its requests' table named `pull_requests` and a
/// goal's merge request kept as its `PR`. Opened, the table is `merge_requests` with the requests
/// in it, the goal's `PR` its `MR` with the value it had, said in one commit of the board's; the
/// store is kept as it was first, and nothing more is done the second time.
#[test]
fn a_store_before_merge_requests_has_its_table_and_goals_named_once() {
    let place = common::place("merge-requests.sqlite");
    let core = Core::create(&place).unwrap();
    let mcp = |tool: &str, arguments: Value| {
        let mut arguments = arguments;
        arguments["project"] = "前からの".into();
        let (refused, said) = common::call(&core, tool, arguments);
        assert!(!refused, "{tool}: {said}");
        serde_json::from_str::<Value>(&said).unwrap_or(Value::Null)
    };
    let (refused, said) = common::call(&core, "project_create", json!({"project": "前からの"}));
    assert!(!refused, "{said}");
    mcp(
        "card_create",
        json!({"branch": "_project_board_", "target": "Goals:Ship", "value": {"kind": "goal", "status": "inProgress", "MR": 3}}),
    );
    mcp("branch_create", json!({"target": "work"}));
    mcp(
        "card_create",
        json!({"branch": "work", "target": "Free:Memo", "value": {"kind": "note", "body": "メモ"}}),
    );
    let project: String = Connection::open(&place)
        .unwrap()
        .query_row("SELECT id FROM projects", [], |r| r.get(0))
        .unwrap();
    let locked = op(
        &core,
        json!({"op": "main_lock", "project": project, "on": true}),
    );
    assert!(locked.get("error").is_none(), "{locked}");
    let asked = mcp("branch_merge", json!({"target": "work"}));
    assert_eq!(asked["mr"], 1, "{asked}");
    core.close();
    drop(core);
    // As it was before: the table and the goal's field by the names of before.
    Connection::open(&place)
        .unwrap()
        .execute_batch(&format!(
            "{REQUESTS_BEFORE}
             UPDATE card_docs SET doc = replace(doc, '\"MR\":', '\"PR\":');
             DELETE FROM _migrations WHERE name IN ('{MERGE_REQUESTS}', '{BRANCH_SNAPSHOTS}');"
        ))
        .unwrap();
    named_before(&place);

    let core = Core::open(&place).unwrap();
    assert_eq!(copies(&place), [MERGE_REQUESTS]);
    let conn = Connection::open(&place).unwrap();
    assert_eq!(marks(&conn), EVERY);
    assert!(columns(&conn, "pull_requests").is_empty());
    let kept: i64 = conn
        .query_row("SELECT count(*) FROM merge_requests", [], |r| r.get(0))
        .unwrap();
    assert_eq!(kept, 1);
    let (refused, said) = common::call(
        &core,
        "card_read",
        json!({"project": "前からの", "branch": "_project_board_", "target": "Goals:Ship"}),
    );
    assert!(!refused, "{said}");
    let read: Value = serde_json::from_str(&said).unwrap();
    assert_eq!(read["MR"], 3, "{read}");
    assert!(read.get("PR").is_none(), "{read}");
    let commits = op(
        &core,
        json!({"op": "commits", "project": project, "branch": "~board"}),
    );
    assert_eq!(
        commits["result"]["commits"][0]["lines"][0]["summary"],
        "goal 1 枚の欄 PR を MR にした（値はそのまま。Ship）"
    );
    let (refused, said) = common::call(
        &core,
        "mr_read",
        json!({"project": "前からの", "target": "1"}),
    );
    assert!(!refused, "{said}");
    // Opened again: nothing more.
    core.close();
    drop(core);
    Core::open(&place).unwrap().close();
    assert_eq!(copies(&place), [MERGE_REQUESTS]);
}

/// A store before `core_0010_plan_tree`: its projects say nothing of what is directly under each
/// root. Opened, each project's `project_tree` is its roots as they are — their names and the order
/// they were shown in — kept as it was first; opened again, nothing more.
#[test]
fn a_store_before_the_project_tree_has_its_roots_written_once() {
    let place = common::place("project-tree.sqlite");
    let core = Core::create(&place).unwrap();
    let mcp = |tool: &str, arguments: Value| {
        let mut arguments = arguments;
        arguments["project"] = "前からの".into();
        let (refused, said) = common::call(&core, tool, arguments);
        assert!(!refused, "{tool}: {said}");
        serde_json::from_str::<Value>(&said).unwrap_or(Value::Null)
    };
    let (refused, said) = common::call(&core, "project_create", json!({"project": "前からの"}));
    assert!(!refused, "{said}");
    mcp(
        "section_update",
        json!({"target": "Charts", "value": {"name": "Flows"}}),
    );
    core.close();
    drop(core);
    // As it was before: no project_tree.
    Connection::open(&place)
        .unwrap()
        .execute_batch(&format!(
            "UPDATE projects SET kinds = NULL;
             DELETE FROM _migrations WHERE name = '{PROJECT_TREE}';"
        ))
        .unwrap();
    named_before(&place);

    let core = Core::open(&place).unwrap();
    assert_eq!(copies(&place), [PROJECT_TREE]);
    assert_eq!(marks(&Connection::open(&place).unwrap()), EVERY);
    let (refused, said) = common::call(&core, "project_read", json!({"project": "前からの"}));
    assert!(!refused, "{said}");
    let read: Value = serde_json::from_str(&said).unwrap();
    assert_eq!(
        read["projectTree"],
        json!({
            "main_sections": [
                {"type": "document", "name": "Docs"}, {"type": "schema", "name": "Schemas"},
                {"type": "definition", "name": "Definitions"},
                {"type": "chart", "name": "Flows"}, {"type": "free", "name": "Free"}
            ],
            "board_sections": [{"type": "goals", "name": "Goals"}]
        })
    );
    // Opened again: nothing more.
    core.close();
    drop(core);
    let _again = Core::open(&place).unwrap();
    assert_eq!(copies(&place), [PROJECT_TREE]);
}

/// A store before `core_0011_work_branch`: a goal's working branch kept as its `branch`. Opened,
/// it is its `workBranch`, its value as it was, said in one commit of the board's; the store is
/// kept as it was first, and nothing more is done the second time.
#[test]
fn a_store_before_work_branch_has_its_goals_named_once() {
    let place = common::place("work-branch.sqlite");
    let core = Core::create(&place).unwrap();
    let (refused, said) = common::call(&core, "project_create", json!({"project": "前からの"}));
    assert!(!refused, "{said}");
    let (refused, said) = common::call(
        &core,
        "card_create",
        json!({"project": "前からの", "branch": "_project_board_", "target": "Goals:Ship",
               "value": {"kind": "goal", "status": "inProgress", "workBranch": "work"}}),
    );
    assert!(!refused, "{said}");
    let project: String = Connection::open(&place)
        .unwrap()
        .query_row("SELECT id FROM projects", [], |r| r.get(0))
        .unwrap();
    core.close();
    drop(core);
    // As it was before: the goal's field by the name of before.
    Connection::open(&place)
        .unwrap()
        .execute_batch(&format!(
            "UPDATE card_docs SET doc = replace(doc, '\"workBranch\":', '\"branch\":');
             DELETE FROM _migrations WHERE name = '{WORK_BRANCH}';"
        ))
        .unwrap();
    named_before(&place);

    let core = Core::open(&place).unwrap();
    assert_eq!(copies(&place), [WORK_BRANCH]);
    // Brought up from core_0011 on at once (with core_0014 too): the copy says the form it was
    // moved from, how far it had been brought up.
    assert_eq!(
        kept(&place, WORK_BRANCH)
            .file_name()
            .unwrap()
            .to_str()
            .unwrap(),
        format!(
            "{}.before-upgrade-from-core_0010_plan_tree.db",
            place.file_name().unwrap().to_str().unwrap()
        )
    );
    assert!(kept(&place, WORK_BRANCH).exists());
    assert_eq!(marks(&Connection::open(&place).unwrap()), EVERY);
    let (refused, said) = common::call(
        &core,
        "card_read",
        json!({"project": "前からの", "branch": "_project_board_", "target": "Goals:Ship"}),
    );
    assert!(!refused, "{said}");
    let read: Value = serde_json::from_str(&said).unwrap();
    assert_eq!(read["workBranch"], "work", "{read}");
    assert!(read.get("strayFields").is_none(), "{read}");
    let commits = op(
        &core,
        json!({"op": "commits", "project": project, "branch": "~board"}),
    );
    assert_eq!(
        commits["result"]["commits"][0]["lines"][0]["summary"],
        "goal 1 枚の欄 branch を workBranch にした（値はそのまま。Ship）"
    );
    // Opened again: nothing more.
    core.close();
    drop(core);
    Core::open(&place).unwrap().close();
    assert_eq!(copies(&place), [WORK_BRANCH]);
}

#[test]
fn a_store_before_hold_clients_has_the_column_once() {
    let place = common::place("hold-clients.sqlite");
    let core = Core::create(&place).unwrap();
    let (refused, said) = common::call(&core, "project_create", json!({"project": "前からの"}));
    assert!(!refused, "{said}");
    core.close();
    drop(core);
    // As it was before: holds with no Mac.
    Connection::open(&place)
        .unwrap()
        .execute_batch(&format!(
            "ALTER TABLE holds DROP COLUMN client;
             DELETE FROM _migrations WHERE name = '{HOLD_CLIENTS}';"
        ))
        .unwrap();
    named_before(&place);

    let core = Core::open(&place).unwrap();
    assert_eq!(copies(&place), [HOLD_CLIENTS]);
    let conn = Connection::open(&place).unwrap();
    assert_eq!(marks(&conn), EVERY);
    assert!(columns(&conn, "holds").ends_with(&["client".to_string()]));
    // Opened again: nothing more.
    core.close();
    drop(core);
    Core::open(&place).unwrap().close();
    assert_eq!(copies(&place), [HOLD_CLIENTS]);
}

#[test]
fn a_store_with_the_change_notes_has_their_table_let_go_once() {
    let place = common::place("no-changelog.sqlite");
    let core = Core::create(&place).unwrap();
    let (refused, said) = common::call(&core, "project_create", json!({"project": "前からの"}));
    assert!(!refused, "{said}");
    core.close();
    drop(core);
    // As it was before: the table of change notes, which nothing wrote.
    Connection::open(&place)
        .unwrap()
        .execute_batch(&format!(
            "CREATE TABLE changelog (id TEXT PRIMARY KEY, project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE, branch_id TEXT NOT NULL DEFAULT '', target_kind TEXT NOT NULL, target_id TEXT NOT NULL DEFAULT '', generation INTEGER, text TEXT NOT NULL, at TEXT NOT NULL, author_kind TEXT NOT NULL, author TEXT, from_branch TEXT, pr INTEGER);
             CREATE UNIQUE INDEX changelog_entry ON changelog(project_id, branch_id, target_kind, target_id, ifnull(generation, -1));
             DELETE FROM _migrations WHERE name = '{NO_CHANGELOG}';"
        ))
        .unwrap();

    let core = Core::open(&place).unwrap();
    assert_eq!(copies(&place), [NO_CHANGELOG]);
    let conn = Connection::open(&place).unwrap();
    assert_eq!(marks(&conn), EVERY);
    let left: i64 = conn
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE name IN ('changelog', 'changelog_entry')",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(left, 0);
    // Opened again: nothing more.
    core.close();
    drop(core);
    Core::open(&place).unwrap().close();
    assert_eq!(copies(&place), [NO_CHANGELOG]);
}

/// A store of before the check's marks has their table made, empty, and a project in it keeps
/// what it had.
#[test]
fn a_store_before_the_inspection_marks_has_their_table_made_once() {
    let place = common::place("inspection-marks.sqlite");
    let core = Core::create(&place).unwrap();
    let (refused, said) = common::call(&core, "project_create", json!({"project": "前からの"}));
    assert!(!refused, "{said}");
    core.close();
    drop(core);
    Connection::open(&place)
        .unwrap()
        .execute_batch(&format!(
            "DROP TABLE inspection_marks;
             DELETE FROM _migrations WHERE name = '{INSPECTION_MARKS}';"
        ))
        .unwrap();

    let core = Core::open(&place).unwrap();
    assert_eq!(copies(&place), [INSPECTION_MARKS]);
    let conn = Connection::open(&place).unwrap();
    assert_eq!(marks(&conn), EVERY);
    assert_eq!(
        columns(&conn, "inspection_marks"),
        ["project_id", "id", "grp", "print", "reason", "by", "at"]
    );
    let (refused, said) = common::call(
        &core,
        "inspection_list",
        json!({"project": "前からの", "value": {"show": "all"}}),
    );
    assert!(!refused, "{said}");
    // Opened again: nothing more.
    core.close();
    drop(core);
    Core::open(&place).unwrap().close();
    assert_eq!(copies(&place), [INSPECTION_MARKS]);
}

/// A store of before the code's records has their table made, empty, and a project in it keeps
/// what it had.
#[test]
fn a_store_before_the_code_marks_has_their_table_made_once() {
    let place = common::place("code-marks.sqlite");
    let core = Core::create(&place).unwrap();
    let (refused, said) = common::call(&core, "project_create", json!({"project": "前からの"}));
    assert!(!refused, "{said}");
    core.close();
    drop(core);
    Connection::open(&place)
        .unwrap()
        .execute_batch(&format!(
            "DROP TABLE code_marks;
             DELETE FROM _migrations WHERE name = '{CODE_MARKS}';"
        ))
        .unwrap();

    let core = Core::open(&place).unwrap();
    assert_eq!(copies(&place), [CODE_MARKS]);
    let conn = Connection::open(&place).unwrap();
    assert_eq!(marks(&conn), EVERY);
    assert_eq!(
        columns(&conn, "code_marks"),
        [
            "project_id",
            "line_id",
            "file_id",
            "symbol",
            "digest",
            "by",
            "at"
        ]
    );
    let (refused, said) = common::call(
        &core,
        "card_list",
        json!({"project": "前からの", "value": {"implementation": "none"}}),
    );
    assert!(!refused, "{said}");
    // Opened again: nothing more.
    core.close();
    drop(core);
    Core::open(&place).unwrap().close();
    assert_eq!(copies(&place), [CODE_MARKS]);
}

/// A store of before the groups has `places` made anew: every box it had kept as it was, a group's row
/// taken, and what a group looks like kept in a column of its own.
#[test]
fn a_store_before_the_map_groups_has_its_places_made_anew_once() {
    let place = common::place("map-groups.sqlite");
    let core = Core::create(&place).unwrap();
    let (refused, said) = common::call(&core, "project_create", json!({"project": "前からの"}));
    assert!(!refused, "{said}");
    core.close();
    drop(core);
    let project: String = {
        let conn = Connection::open(&place).unwrap();
        let project: String = conn
            .query_row("SELECT id FROM projects", [], |r| r.get(0))
            .unwrap();
        conn.execute_batch(&format!(
            "CREATE TABLE places_before (
               project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
               branch_id TEXT NOT NULL DEFAULT '',
               view      TEXT NOT NULL DEFAULT '',
               item_kind TEXT NOT NULL DEFAULT 'card' CHECK (item_kind IN ('card', 'section', 'node')),
               item      TEXT NOT NULL,
               x INTEGER, y INTEGER, w INTEGER, h INTEGER,
               pinned INTEGER NOT NULL DEFAULT 0,
               PRIMARY KEY (project_id, branch_id, view, item_kind, item)
             );
             INSERT INTO places_before SELECT project_id, branch_id, view, item_kind, item, x, y, w, h, pinned FROM places;
             DROP TABLE places;
             ALTER TABLE places_before RENAME TO places;
             DELETE FROM _migrations WHERE name = '{MAP_GROUPS}';"
        ))
        .unwrap();
        conn.execute(
            "INSERT INTO places (project_id, view, item_kind, item, x, y, w, h, pinned) VALUES (?, 'v', 'card', 'c1', 3, 4, 12, 6, 1)",
            [&project],
        )
        .unwrap();
        // The form of before takes no group.
        assert!(
            conn.execute(
                "INSERT INTO places (project_id, view, item_kind, item, x, y) VALUES (?, 'v', 'group', 'G', 0, 0)",
                [&project],
            )
            .is_err()
        );
        project
    };

    let core = Core::open(&place).unwrap();
    assert_eq!(copies(&place), [MAP_GROUPS]);
    let conn = Connection::open(&place).unwrap();
    assert_eq!(marks(&conn), EVERY);
    assert_eq!(
        columns(&conn, "places"),
        [
            "project_id",
            "branch_id",
            "view",
            "item_kind",
            "item",
            "x",
            "y",
            "w",
            "h",
            "pinned",
            "look"
        ]
    );
    let kept: (i64, i64, i64, i64, i64, Option<String>) = conn
        .query_row(
            "SELECT x, y, w, h, pinned, look FROM places WHERE item = 'c1'",
            [],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                ))
            },
        )
        .unwrap();
    assert_eq!(kept, (3, 4, 12, 6, 1, None));
    conn.execute(
        "INSERT INTO places (project_id, view, item_kind, item, x, y, w, h, look) VALUES (?, 'v', 'group', 'G', 0, 0, 6, 3, '{\"color\":\"red\"}')",
        [&project],
    )
    .unwrap();
    // Opened again: nothing more, the group kept.
    core.close();
    drop(core);
    Core::open(&place).unwrap().close();
    assert_eq!(copies(&place), [MAP_GROUPS]);
    let groups: i64 = Connection::open(&place)
        .unwrap()
        .query_row(
            "SELECT count(*) FROM places WHERE item_kind = 'group'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(groups, 1);
}

#[test]
fn a_store_before_branch_snapshots_has_their_table_and_columns_once() {
    let place = common::place("branch-snapshots.sqlite");
    let core = Core::create(&place).unwrap();
    let (refused, said) = common::call(&core, "project_create", json!({"project": "前からの"}));
    assert!(!refused, "{said}");
    let (refused, said) = common::call(
        &core,
        "branch_create",
        json!({"project": "前からの", "target": "work"}),
    );
    assert!(!refused, "{said}");
    core.close();
    drop(core);
    // As it was before: no snapshots, no reverts, no keep.
    Connection::open(&place)
        .unwrap()
        .execute_batch(&format!(
            "DROP TABLE branch_frozen;
             ALTER TABLE branches DROP COLUMN reverts;
             ALTER TABLE merge_requests DROP COLUMN keep;
             DELETE FROM _migrations WHERE name = '{BRANCH_SNAPSHOTS}';"
        ))
        .unwrap();

    let core = Core::open(&place).unwrap();
    assert_eq!(copies(&place), [BRANCH_SNAPSHOTS]);
    let conn = Connection::open(&place).unwrap();
    assert_eq!(marks(&conn), EVERY);
    assert!(columns(&conn, "branches").ends_with(&["reverts".to_string()]));
    assert!(columns(&conn, "merge_requests").ends_with(&["keep".to_string()]));
    assert_eq!(
        columns(&conn, "branch_frozen"),
        ["project_id", "branch_id", "tbl", "id", "doc"]
    );
    // The branch there was: still there, seeing main as it is.
    let (refused, said) = common::call(&core, "branch_list", json!({"project": "前からの"}));
    assert!(!refused && said.contains("work"), "{said}");
    core.close();
    drop(core);
    Core::open(&place).unwrap().close();
    assert_eq!(copies(&place), [BRANCH_SNAPSHOTS]);
}

/// A store before `core_0014_projects`: its tables and columns named by the project, what its
/// history's lines say they touched, its kinds' tree, the items of what MCP may write and the
/// places of its writing rules — once, kept before.
#[test]
fn a_store_before_projects_is_named_by_the_project_once() {
    let place = common::place("projects-before.sqlite");
    let core = Core::create(&place).unwrap();
    let (refused, said) = common::call(&core, "project_create", json!({"project": "前からの"}));
    assert!(!refused, "{said}");
    core.close();
    drop(core);
    named_before(&place);
    Connection::open(&place)
        .unwrap()
        .execute_batch(
            "UPDATE plans SET mcp_writes = '{\"plan.rules\":true}', rules = '{\"plan.description\":\"地図を書く\"}', kinds = replace(kinds, '\"project_tree\"', '\"plan_tree\"');
             UPDATE commit_lines SET target_type = 'plan' WHERE target_type = 'project';
             UPDATE commit_lines SET kind = 'plan.update' WHERE idx = 0;",
        )
        .unwrap();
    Core::open(&place).unwrap().close();
    let conn = Connection::open(&place).unwrap();
    assert_eq!(marks(&conn), EVERY);
    assert_eq!(copies(&place), [PROJECTS]);
    let tables: Vec<String> = conn
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert!(tables.iter().all(|t| !t.starts_with("plan")), "{tables:?}");
    for table in &tables {
        assert!(
            !columns(&conn, table).iter().any(|c| c == "plan_id"),
            "{table}"
        );
    }
    let (kinds, writes, rules): (String, String, String) = conn
        .query_row("SELECT kinds, mcp_writes, rules FROM projects", [], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })
        .unwrap();
    assert!(
        kinds.contains("\"project_tree\"") && !kinds.contains("plan_tree"),
        "{kinds}"
    );
    assert_eq!(writes, "{\"project.rules\":true}");
    assert_eq!(rules, "{\"project.description\":\"地図を書く\"}");
    let said: Vec<(String, String)> = conn
        .prepare("SELECT kind, target_type FROM commit_lines")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert!(
        said.iter()
            .all(|(kind, target)| !kind.starts_with("plan.") && target != "plan"),
        "{said:?}"
    );
    // What was `plan.update` is `project.update`, and what said the plan says the project.
    assert!(
        said.iter().any(|(kind, _)| kind == "project.update"),
        "{said:?}"
    );
    assert!(
        said.iter().any(|(_, target)| target == "project"),
        "{said:?}"
    );
}

/// A store of before the history's lines were kept by key has the columns added once: its lines are
/// read as they were kept, and the lines written after it are kept by key.
#[test]
fn a_store_before_the_commit_keys_has_their_columns_added_once() {
    let place = common::place("commit-keys.sqlite");
    let core = Core::create(&place).unwrap();
    let (refused, said) = common::call(&core, "project_create", json!({"project": "前からの"}));
    assert!(!refused, "{said}");
    core.close();
    drop(core);
    Connection::open(&place)
        .unwrap()
        .execute_batch(&format!(
            "ALTER TABLE commit_lines DROP COLUMN text_key;
             ALTER TABLE commit_lines DROP COLUMN text_values;
             UPDATE commit_lines SET summary = '前からの行';
             DELETE FROM _migrations WHERE name = '{COMMIT_KEYS}';"
        ))
        .unwrap();

    let core = Core::open(&place).unwrap();
    assert_eq!(copies(&place), [COMMIT_KEYS]);
    let conn = Connection::open(&place).unwrap();
    assert_eq!(marks(&conn), EVERY);
    assert!(columns(&conn, "commit_lines").ends_with(&[
        "summary".to_string(),
        "text_key".to_string(),
        "text_values".to_string()
    ]));
    let project: String = conn
        .query_row("SELECT id FROM projects", [], |r| r.get(0))
        .unwrap();
    let read = |lang: &str| -> Value {
        op(
            &core,
            json!({"op": "commits", "project": project, "lang": lang}),
        )["result"]["commits"]
            .clone()
    };
    let before = read("en");
    let lines = before[0]["lines"].as_array().unwrap();
    assert!(
        lines.iter().all(|l| l["summary"] == "前からの行"),
        "{before}"
    );
    // Written after it: kept by key, and said in English when read in English.
    let (refused, said) = common::call(
        &core,
        "section_create",
        json!({"project": "前からの", "target": "Docs/Later"}),
    );
    assert!(!refused, "{said}");
    let keys: Vec<Option<String>> = conn
        .prepare("SELECT text_key FROM commit_lines WHERE summary <> '前からの行'")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert!(
        !keys.is_empty() && keys.iter().all(Option::is_some),
        "{keys:?}"
    );
    let after = read("en");
    let newest = after[0]["lines"][0]["summary"].as_str().unwrap();
    assert!(newest.contains("Later"), "{after}");
    assert!(!newest.contains("セクション"), "{after}");
    // Opened again: nothing more.
    core.close();
    drop(core);
    Core::open(&place).unwrap().close();
    assert_eq!(copies(&place), [COMMIT_KEYS]);
}

/// A store of before the export language has the column added once, and its projects are said in
/// Japanese, as their exports were; a project made after it takes the language of whoever made it.
#[test]
fn a_store_before_the_export_language_has_its_projects_exported_in_japanese() {
    let place = common::place("export-language.sqlite");
    let core = Core::create(&place).unwrap();
    let (refused, said) = common::call(&core, "project_create", json!({"project": "前からの"}));
    assert!(!refused, "{said}");
    core.close();
    drop(core);
    Connection::open(&place)
        .unwrap()
        .execute_batch(&format!(
            "ALTER TABLE projects DROP COLUMN export_language;
             DELETE FROM _migrations WHERE name = '{EXPORT_LANGUAGE}';"
        ))
        .unwrap();

    let core = Core::open(&place).unwrap();
    assert_eq!(copies(&place), [EXPORT_LANGUAGE]);
    let conn = Connection::open(&place).unwrap();
    assert_eq!(marks(&conn), EVERY);
    assert!(columns(&conn, "projects").ends_with(&["export_language".to_string()]));
    let language: String = conn
        .query_row("SELECT export_language FROM projects", [], |r| r.get(0))
        .unwrap();
    assert_eq!(language, "ja");
    // Opened again: nothing more.
    core.close();
    drop(core);
    Core::open(&place).unwrap().close();
    assert_eq!(copies(&place), [EXPORT_LANGUAGE]);
}

/// The print a request to merge keeps of what it was tried on (`requests::request_print`): the
/// branch's documents and main's, each the digest of its sections' and cards' rows. A store of
/// before kept it of its documents as they were.
fn request_print(conn: &Connection, project: &str, branch: &str) -> String {
    use sha2::{Digest, Sha256};
    let print_of = |tree: &str| -> String {
        let mut hash = Sha256::new();
        for table in ["section_docs", "card_docs"] {
            hash.update(format!("\u{2}{table}\u{1}{tree}").as_bytes());
            let rows: Vec<String> = conn
                .prepare(&format!(
                    "SELECT id, removed, ifnull(doc, ''), ifnull(base, '') FROM {table} WHERE project_id = ? AND branch_id = ? ORDER BY id"
                ))
                .unwrap()
                .query_map(rusqlite::params![project, tree], |r| {
                    Ok(format!(
                        "{}\u{1}{}\u{1}{}\u{1}{}",
                        r.get::<_, String>(0)?,
                        r.get::<_, i64>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, String>(3)?
                    ))
                })
                .unwrap()
                .collect::<Result<_, _>>()
                .unwrap();
            for row in rows {
                hash.update(format!("{row}\u{0}").as_bytes());
            }
        }
        hash.finalize().iter().map(|b| format!("{b:02x}")).collect()
    };
    format!("{}.{}", print_of(branch), print_of(""))
}

/// How many of the documents, kept anywhere — every tree's and their bases, a branch's frozen ones,
/// the history's rows — still say the built-in kind of section of before.
fn instances_left(conn: &Connection) -> i64 {
    conn.query_row(
        "SELECT (SELECT count(*) FROM section_docs WHERE instr(ifnull(doc, '') || ifnull(base, ''), '\"kind\":\"instances\"') > 0)
              + (SELECT count(*) FROM branch_frozen WHERE instr(ifnull(doc, ''), '\"kind\":\"instances\"') > 0)
              + (SELECT count(*) FROM commit_rows WHERE instr(ifnull(before, ''), 'instances') > 0)
              + (SELECT count(*) FROM projects WHERE instr(ifnull(kinds, ''), 'instances') > 0)",
        [],
        |r| r.get(0),
    )
    .unwrap()
}

/// A store before `core_0021_definition_kind`: sections of the built-in kind `instances` in main,
/// in a branch's copy and its base, frozen for the branch, on the board and in the history, the
/// project tree naming the kind, and a request to merge tried on them; a project with a kind of its
/// own named `definition` that the built-in one stands in for, and one whose own holds more.
/// Opened, the sections go by `definition` everywhere and the request stays open, as the branch
/// did not move; the own kind the built-in one stands in for is let go of and its sections are the
/// built-in one's; the other is `definition_project` with its sections and its place in the tree,
/// and every project's kinds write again. Kept as it was first; opened again, nothing more.
#[test]
fn a_store_before_the_definition_kind_has_its_instances_sections_moved_once() {
    let place = common::place("definition-kind.sqlite");
    let core = Core::create(&place).unwrap();
    let on = |project: &str, tool: &str, arguments: Value| {
        let mut arguments = arguments;
        arguments["project"] = project.into();
        let (refused, said) = common::call(&core, tool, arguments);
        assert!(!refused, "{tool}: {said}");
        serde_json::from_str::<Value>(&said).unwrap_or(Value::Null)
    };
    for project in ["前からの", "同じ定義", "広い定義"] {
        let (refused, said) = common::call(&core, "project_create", json!({"project": project}));
        assert!(!refused, "{said}");
    }
    let a = |tool: &str, arguments: Value| on("前からの", tool, arguments);
    a(
        "card_create",
        json!({"target": "Schemas:Order", "value": {"kind": "entity"}}),
    );
    a(
        "section_create",
        json!({"target": "Definitions/Inner", "value": {}}),
    );
    a(
        "card_create",
        json!({"target": "Definitions:first", "value": {"kind": "instance", "of": "Schemas:Order", "data": {}}}),
    );
    // Kept by the history as it was before.
    a(
        "section_update",
        json!({"target": "Definitions", "value": {"description": "main の説明"}}),
    );
    a("branch_create", json!({"target": "work"}));
    a(
        "section_update",
        json!({"branch": "work", "target": "Definitions", "value": {"description": "branch の説明"}}),
    );
    a(
        "card_create",
        json!({"branch": "work", "target": "Definitions:Memo", "value": {"kind": "note", "body": "メモ"}}),
    );
    // Kept frozen for the branch, which has not written it.
    a(
        "section_update",
        json!({"target": "Definitions/Inner", "value": {"description": "main で直した"}}),
    );
    let ids: Vec<(String, String)> = {
        let conn = Connection::open(&place).unwrap();
        let mut stmt = conn.prepare("SELECT name, id FROM projects").unwrap();
        stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    };
    let id_of = |name: &str| ids.iter().find(|(n, _)| n == name).unwrap().1.clone();
    let (first, same, wide) = (id_of("前からの"), id_of("同じ定義"), id_of("広い定義"));
    let locked = op(
        &core,
        json!({"op": "main_lock", "project": first, "on": true}),
    );
    assert!(locked.get("error").is_none(), "{locked}");
    let asked = a("branch_merge", json!({"target": "work"}));
    assert_eq!(asked["mr"], 1, "{asked}");
    core.close();
    drop(core);

    // As it was before: the built-in kind by its name of before wherever the first project says it,
    // a section of it on the board, and the request's print of the documents as they were; the
    // others' definition sections of their own kinds.
    {
        let conn = Connection::open(&place).unwrap();
        conn.execute_batch(&format!(
            "UPDATE section_docs SET doc = replace(doc, '\"kind\":\"definition\"', '\"kind\":\"instances\"'),
                                     base = replace(base, '\"kind\":\"definition\"', '\"kind\":\"instances\"') WHERE project_id = '{first}';
             UPDATE branch_frozen SET doc = replace(doc, '\"kind\":\"definition\"', '\"kind\":\"instances\"') WHERE project_id = '{first}' AND tbl = 'section_docs';
             UPDATE commit_rows SET before = replace(before, '\\\"kind\\\":\\\"definition\\\"', '\\\"kind\\\":\\\"instances\\\"')
               WHERE tbl = 'section_docs' AND commit_id IN (SELECT id FROM commits WHERE project_id = '{first}');
             UPDATE projects SET kinds = replace(kinds, '\"type\":\"definition\"', '\"type\":\"instances\"') WHERE id = '{first}';
             DELETE FROM _migrations WHERE name = '{DEFINITION_KIND}';"
        ))
        .unwrap();
        let goals: String = conn
            .query_row(
                "SELECT id FROM section_docs WHERE project_id = ? AND branch_id = '~board' AND name = 'Goals'",
                [&first],
                |r| r.get(0),
            )
            .unwrap();
        let board =
            json!({"id": "s-board-data", "parent_id": goals, "name": "Data", "kind": "instances"});
        conn.execute(
            "INSERT INTO section_docs (project_id, branch_id, id, parent_id, name, doc) VALUES (?, '~board', 's-board-data', ?, 'Data', ?)",
            rusqlite::params![first, goals, board.to_string()],
        )
        .unwrap();
        let print = request_print(&conn, &first, &branch_of(&conn, &first, "work"));
        conn.execute(
            "UPDATE merge_requests SET print = ? WHERE project_id = ?",
            rusqlite::params![print, first],
        )
        .unwrap();
        for (project, holds) in [
            (
                &same,
                json!(["entity", "value", "enum", "store", "instance", "note"]),
            ),
            (&wide, json!(["entity", "flow"])),
        ] {
            let kept: String = conn
                .query_row("SELECT kinds FROM projects WHERE id = ?", [project], |r| {
                    r.get(0)
                })
                .unwrap();
            let mut kinds: Value = serde_json::from_str(&kept).unwrap();
            kinds["sections"] = json!([{"section": "definition", "label": "定義", "holds": holds, "sections": ["definition"]}]);
            conn.execute(
                "UPDATE projects SET kinds = ? WHERE id = ?",
                rusqlite::params![kinds.to_string(), project],
            )
            .unwrap();
        }
        assert!(instances_left(&conn) >= 6, "{}", instances_left(&conn));
    }

    let core = Core::open(&place).unwrap();
    assert_eq!(copies(&place), [DEFINITION_KIND]);
    let conn = Connection::open(&place).unwrap();
    assert_eq!(marks(&conn), EVERY);
    assert_eq!(instances_left(&conn), 0);
    let on = |project: &str, tool: &str, arguments: Value| {
        let mut arguments = arguments;
        arguments["project"] = project.into();
        let (refused, said) = common::call(&core, tool, arguments);
        assert!(!refused, "{tool}: {said}");
        serde_json::from_str::<Value>(&said).unwrap_or(Value::Null)
    };
    let kinds_of = |project: &str, branch: &str| -> Vec<(String, String)> {
        on(project, "section_list", json!({"branch": branch}))["sections"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|s| s["section"].as_str().unwrap().starts_with("Definitions"))
            .map(|s| {
                (
                    s["section"].as_str().unwrap().to_string(),
                    s["type"].as_str().unwrap().to_string(),
                )
            })
            .collect()
    };
    let definitions = |kind: &str| {
        vec![
            ("Definitions".to_string(), kind.to_string()),
            ("Definitions/Inner".to_string(), kind.to_string()),
        ]
    };
    // The first: every tree by the kind of now, what it holds as it was, and more it may hold.
    assert_eq!(kinds_of("前からの", "main"), definitions("definition"));
    assert_eq!(kinds_of("前からの", "work"), definitions("definition"));
    let board: String = conn
        .query_row(
            "SELECT doc FROM section_docs WHERE id = 's-board-data'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&board).unwrap()["kind"],
        "definition"
    );
    let read = on("前からの", "project_read", json!({}));
    assert_eq!(
        read["projectTree"]["main_sections"][2],
        json!({"type": "definition", "name": "Definitions"})
    );
    assert_eq!(
        on(
            "前からの",
            "card_read",
            json!({"target": "Definitions:first"})
        )["of"],
        "Schemas:Order"
    );
    assert_eq!(
        on("前からの", "mr_read", json!({"target": "1"}))["state"],
        "open"
    );
    // The project with a kind of its own the built-in one stands in for: let go of.
    let own = |project: &str| -> Vec<Value> {
        on(project, "kind_list", json!({}))["sections"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|s| s["own"] == true)
            .cloned()
            .collect()
    };
    assert!(own("同じ定義").is_empty(), "{:?}", own("同じ定義"));
    assert_eq!(
        kinds_of("同じ定義", "main"),
        [("Definitions".to_string(), "definition".to_string())]
    );
    // The one whose own holds more: kept by a name of its own, its sections and tree with it.
    let wide_own = own("広い定義");
    assert_eq!(wide_own.len(), 1, "{wide_own:?}");
    assert_eq!(wide_own[0]["section"], "definition_project");
    assert_eq!(
        kinds_of("広い定義", "main"),
        [("Definitions".to_string(), "definition_project".to_string())]
    );
    on(
        "広い定義",
        "card_create",
        json!({"target": "Definitions:Steps", "value": {"kind": "flow"}}),
    );
    // Every project's kinds write again: no two kinds of one name.
    for project in ["前からの", "同じ定義", "広い定義"] {
        let tree = on(project, "project_read", json!({}))["projectTree"].clone();
        on(
            project,
            "project_update",
            json!({"value": {"project_tree": tree}}),
        );
    }
    let wide_tree = on("広い定義", "project_read", json!({}))["projectTree"].clone();
    assert_eq!(
        wide_tree["main_sections"][2],
        json!({"type": "definition_project", "name": "Definitions"})
    );
    // Opened again: nothing more.
    core.close();
    drop(core);
    Core::open(&place).unwrap().close();
    assert_eq!(copies(&place), [DEFINITION_KIND]);
}

/// The id of the branch named `name` of a project.
fn branch_of(conn: &Connection, project: &str, name: &str) -> String {
    conn.query_row(
        "SELECT id FROM branches WHERE project_id = ? AND name = ?",
        [project, name],
        |r| r.get(0),
    )
    .unwrap()
}

/// The board's sections as `section_list` reads them: each path and its kind.
fn board_sections(core: &Core, project: &str) -> Vec<(String, String)> {
    let (refused, said) = common::call(
        core,
        "section_list",
        json!({"project": project, "branch": "_project_board_"}),
    );
    assert!(!refused, "{said}");
    serde_json::from_str::<Value>(&said).unwrap()["sections"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| {
            (
                s["section"].as_str().unwrap().to_string(),
                s["type"].as_str().unwrap_or_default().to_string(),
            )
        })
        .collect()
}

/// A store before `core_0022_board_sections`: one project's board with no sections for what is
/// finished and what was cancelled, another's with goals sections of its own named for them (one
/// holding a goal). Opened, the first is given both and the second's become them, the goal still
/// inside; opened again, nothing more.
#[test]
fn a_store_before_the_board_sections_has_them_added_or_named_once() {
    let place = common::place("board-sections.sqlite");
    let core = Core::create(&place).unwrap();
    for project in ["足す", "使う"] {
        let (refused, said) = common::call(&core, "project_create", json!({"project": project}));
        assert!(!refused, "{said}");
    }
    core.close();
    drop(core);
    {
        let conn = Connection::open(&place).unwrap();
        conn.execute_batch(
            "DELETE FROM section_docs WHERE branch_id = '~board' AND (doc LIKE '%\"kind\":\"complete\"%' OR doc LIKE '%\"kind\":\"cancelled\"%');",
        )
        .unwrap();
    }
    let core = Core::open(&place).unwrap();
    let on = |tool: &str, arguments: Value| {
        let mut arguments = arguments;
        arguments["project"] = "使う".into();
        arguments["branch"] = "_project_board_".into();
        let (refused, said) = common::call(&core, tool, arguments);
        assert!(!refused, "{tool}: {said}");
    };
    on("section_create", json!({"target": "Goals/Completed"}));
    on("section_create", json!({"target": "Goals/cancelled"}));
    on(
        "card_create",
        json!({"target": "Goals/Completed:Done", "value": {"kind": "goal", "status": "done"}}),
    );
    core.close();
    drop(core);
    Connection::open(&place)
        .unwrap()
        .execute("DELETE FROM _migrations WHERE name = ?", [BOARD_SECTIONS])
        .unwrap();

    let core = Core::open(&place).unwrap();
    assert_eq!(copies(&place), [BOARD_SECTIONS]);
    let conn = Connection::open(&place).unwrap();
    assert_eq!(marks(&conn), EVERY);
    let pairs = |list: &[(&str, &str)]| -> Vec<(String, String)> {
        list.iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect()
    };
    assert_eq!(
        board_sections(&core, "足す"),
        pairs(&[
            ("Board", ""),
            ("Goals", "goals"),
            ("Goals/Complete", "complete"),
            ("Goals/Cancelled", "cancelled")
        ])
    );
    assert_eq!(
        board_sections(&core, "使う"),
        pairs(&[
            ("Board", ""),
            ("Goals", "goals"),
            ("Goals/Completed", "complete"),
            ("Goals/cancelled", "cancelled")
        ])
    );
    let (refused, said) = common::call(
        &core,
        "card_read",
        json!({"project": "使う", "branch": "_project_board_", "target": "Goals/Completed:Done"}),
    );
    assert!(!refused, "{said}");
    // Opened again: nothing more.
    let count = |conn: &Connection| -> i64 {
        conn.query_row(
            "SELECT count(*) FROM section_docs WHERE branch_id = '~board'",
            [],
            |r| r.get(0),
        )
        .unwrap()
    };
    let before = count(&conn);
    core.close();
    drop(core);
    Core::open(&place).unwrap().close();
    assert_eq!(count(&conn), before);
    assert_eq!(copies(&place), [BOARD_SECTIONS]);
}

/// A store before `core_0023_workdir_sections`: a project with two working directories (one named
/// as a section directly under main's root is), one bound to a folder here, MCP let bind folders,
/// and a branch made before. Opened, each is a section directly under main's root, listed in the
/// project tree by its id, the clashing one named apart; the branch sees them; the folder stays
/// bound; the item is the one of now. Opened again, nothing more.
#[test]
fn a_store_before_the_workdir_sections_has_them_made_once() {
    let place = common::place("workdir-sections.sqlite");
    let core = Core::create(&place).unwrap();
    let (refused, said) = common::call(&core, "project_create", json!({"project": "作業"}));
    assert!(!refused, "{said}");
    let (refused, said) = common::call(
        &core,
        "branch_create",
        json!({"project": "作業", "target": "work"}),
    );
    assert!(!refused, "{said}");
    core.close();
    drop(core);
    let project: String = Connection::open(&place)
        .unwrap()
        .query_row("SELECT id FROM projects WHERE name = '作業'", [], |r| {
            r.get(0)
        })
        .unwrap();
    {
        let conn = Connection::open(&place).unwrap();
        conn.execute(
            "UPDATE projects SET export_dirs = ?, mcp_writes = ? WHERE id = ?",
            rusqlite::params![
                r#"[{"id":"d-repo","name":"repo","readType":"basic"},{"id":"d-docs","name":"docs"}]"#,
                r#"{"directory.path":"allow"}"#,
                project
            ],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO export_dir_folders (project_id, dir_id, folder) VALUES (?, 'd-repo', '/tmp/nowhere')",
            [&project],
        )
        .unwrap();
        conn.execute("DELETE FROM _migrations WHERE name = ?", [WORKDIR_SECTIONS])
            .unwrap();
    }
    let core = Core::open(&place).unwrap();
    assert_eq!(copies(&place), [WORKDIR_SECTIONS]);
    let conn = Connection::open(&place).unwrap();
    assert_eq!(marks(&conn), EVERY);
    let read = |tool: &str, arguments: Value| -> Value {
        let mut arguments = arguments;
        arguments["project"] = "作業".into();
        let (refused, said) = common::call(&core, tool, arguments);
        assert!(!refused, "{tool}: {said}");
        serde_json::from_str(&said).unwrap()
    };
    let tree = read("project_read", json!({}))["projectTree"]["main_sections"].clone();
    assert_eq!(
        tree.as_array().unwrap()[5..],
        [
            json!({"type": "workdir", "name": "repo", "directory": "d-repo"}),
            json!({"type": "workdir", "name": "docs_2", "directory": "d-docs"})
        ]
    );
    for branch in ["main", "work"] {
        let listed = read("section_list", json!({"branch": branch}));
        let tops: Vec<(String, String)> = listed["sections"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|s| s["type"] == "workdir")
            .map(|s| {
                (
                    s["section"].as_str().unwrap().to_string(),
                    s["type"].as_str().unwrap().to_string(),
                )
            })
            .collect();
        assert_eq!(
            tops,
            [
                ("repo".to_string(), "workdir".to_string()),
                ("docs_2".to_string(), "workdir".to_string())
            ],
            "{branch}: {listed}"
        );
    }
    let repo = read("section_read", json!({"target": "repo"}));
    assert_eq!(repo["folder"], "/tmp/nowhere", "{repo}");
    assert_eq!(repo["readType"], "basic", "{repo}");
    let writes: String = conn
        .query_row(
            "SELECT mcp_writes FROM projects WHERE id = ?",
            [&project],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(writes, r#"{"section_update.folder":"allow"}"#);
    // Opened again: nothing more.
    let count = |conn: &Connection| -> i64 {
        conn.query_row(
            "SELECT count(*) FROM section_docs WHERE id LIKE 'dir:%'",
            [],
            |r| r.get(0),
        )
        .unwrap()
    };
    assert_eq!(count(&conn), 2);
    core.close();
    drop(core);
    Core::open(&place).unwrap().close();
    assert_eq!(count(&conn), 2);
    assert_eq!(copies(&place), [WORKDIR_SECTIONS]);
}
