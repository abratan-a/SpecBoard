//! A goal's fields named in English (Japanese keys were confusing to use; the store's mark
//! `core_0008_board_keys`, a project file before version 14), and its `PR` named
//! `MR` when a request to merge was called a merge request (the mark
//! `core_0009_merge_requests`, a project file before version 15: `move_mr_all`), and its `branch` named
//! `workBranch` when MCP's answers were seen to hide it under their own `branch`, the tree they are
//! of (the mark `core_0011_work_branch`, a project file before version 17:
//! `move_work_branch_all`).
//!
//! A goal kept its fields by their Japanese names (`状態`, `優先度`, `期限`); they are `status`,
//! `priority` and `due` now, their labels (the names people read) Japanese as before. Each goal's
//! document is moved, its values as they were (`kinds::GOAL_KEYS_BEFORE`). A field the board's
//! kinds no longer have (a goal's and a task's `担当`, a task's `優先度`, `branch` and `PR`, held
//! to one person's use) is not moved: a value written in one stays where it is, as 「定義に無い欄」
//! (`strayFields`), to be let go of by hand. A name of now a document holds already is left as
//! it is, and the one of before beside it then stays too.
//!
//! As the lines were (`line_move::move_card_docs`): every tree — main, the board, each branch —
//! its documents and bases, the history's rows, a branch's prints of main's cards and the open
//! requests' prints; a tree that moved any says so in one commit of one record.

use rusqlite::Connection;
use serde_json::{Map, Value};

type Obj = Map<String, Value>;

fn sql(e: rusqlite::Error) -> String {
    text!("keyMove.error.sql", e)
}

/// A goal's document with its fields by their names of now, or none when it has none of before
/// to move (or is no goal).
fn move_doc(doc: &Obj) -> Option<Obj> {
    move_doc_by(doc, &crate::kinds::GOAL_KEYS_BEFORE)
}

/// A goal's document with its `PR` named `MR`, or none.
fn move_mr_doc(doc: &Obj) -> Option<Obj> {
    move_doc_by(doc, &crate::kinds::MR_KEYS_BEFORE)
}

/// A goal's document with its `branch` named `workBranch`, or none.
fn move_work_branch_doc(doc: &Obj) -> Option<Obj> {
    move_doc_by(doc, &crate::kinds::WORK_BRANCH_KEYS_BEFORE)
}

/// A goal's document with the fields of `before` by their names of now.
fn move_doc_by(doc: &Obj, before: &[(&str, &str)]) -> Option<Obj> {
    if doc.get("kind").and_then(Value::as_str) != Some("goal") {
        return None;
    }
    let fields = doc.get("fields").and_then(Value::as_object)?;
    let moves: Vec<(&str, &str)> = before
        .iter()
        .copied()
        .filter(|(before, now)| fields.contains_key(*before) && !fields.contains_key(*now))
        .collect();
    if moves.is_empty() {
        return None;
    }
    // Each where the one of before was, so a document reads in the order it was written.
    let mut renamed = Map::new();
    for (key, value) in fields {
        let key = moves
            .iter()
            .find(|(before, _)| before == key)
            .map(|(_, now)| (*now).to_string())
            .unwrap_or_else(|| key.clone());
        renamed.insert(key, value.clone());
    }
    let mut out = doc.clone();
    out.insert("fields".into(), Value::Object(renamed));
    out.insert("v".into(), crate::kinds::builtin().version.into());
    Some(out)
}

/// Every project of the store moved.
pub fn move_all(conn: &Connection) -> Result<(), String> {
    let projects: Vec<String> = {
        let mut stmt = conn.prepare("SELECT id FROM projects").map_err(sql)?;
        stmt.query_map([], |r| r.get(0))
            .map_err(sql)?
            .collect::<Result<_, _>>()
            .map_err(sql)?
    };
    for project in projects {
        move_project(conn, &project)?;
    }
    Ok(())
}

/// One project's goals moved, in every tree; each tree that had any says how many in a commit of
/// one record 「goal の欄の名前を英語にした」.
pub fn move_project(conn: &Connection, project: &str) -> Result<(), String> {
    let said = |cards: &[String]| {
        let mut names: Vec<&str> = Vec::new();
        for card in cards {
            if !names.contains(&card.as_str()) {
                names.push(card);
            }
        }
        vec![keyed!(
            "keyMove.line.goalKeys",
            count = cards.len(),
            cards = crate::text::List::of(names)
        )]
    };
    crate::line_move::move_card_docs(
        conn,
        project,
        &crate::write::history::migrated(keyed!("history.migrate.goalKeys")),
        &move_doc,
        &said,
    )
}

/// Every project of the store with a request to merge called a merge request (the store's mark
/// `core_0009_merge_requests`): a goal's `PR` named `MR`, and the kind of field `pr` named `mr` in
/// the project's own kinds.
pub fn move_mr_all(conn: &Connection) -> Result<(), String> {
    let projects: Vec<String> = {
        let mut stmt = conn.prepare("SELECT id FROM projects").map_err(sql)?;
        stmt.query_map([], |r| r.get(0))
            .map_err(sql)?
            .collect::<Result<_, _>>()
            .map_err(sql)?
    };
    for project in projects {
        move_mr_project(conn, &project)?;
    }
    Ok(())
}

/// One project moved so (`move_mr_all`; a project file before version 15 too): each tree that had a
/// goal with a `PR` says how many in a commit of one record.
pub fn move_mr_project(conn: &Connection, project: &str) -> Result<(), String> {
    kinds_mr(conn, project)?;
    let said = |cards: &[String]| {
        let mut names: Vec<&str> = Vec::new();
        for card in cards {
            if !names.contains(&card.as_str()) {
                names.push(card);
            }
        }
        vec![keyed!(
            "keyMove.line.goalMr",
            count = cards.len(),
            cards = crate::text::List::of(names)
        )]
    };
    crate::line_move::move_card_docs(
        conn,
        project,
        &crate::write::history::migrated(keyed!("history.migrate.goalMr")),
        &move_mr_doc,
        &said,
    )
}

/// Every project of the store with a goal's `branch` named `workBranch` (the store's mark
/// `core_0011_work_branch`).
pub fn move_work_branch_all(conn: &Connection) -> Result<(), String> {
    let projects: Vec<String> = {
        let mut stmt = conn.prepare("SELECT id FROM projects").map_err(sql)?;
        stmt.query_map([], |r| r.get(0))
            .map_err(sql)?
            .collect::<Result<_, _>>()
            .map_err(sql)?
    };
    for project in projects {
        move_work_branch_project(conn, &project)?;
    }
    Ok(())
}

/// One project moved so (`move_work_branch_all`; a project file before version 17 too): each tree that
/// had a goal with a `branch` says how many in a commit of one record.
pub fn move_work_branch_project(conn: &Connection, project: &str) -> Result<(), String> {
    let said = |cards: &[String]| {
        let mut names: Vec<&str> = Vec::new();
        for card in cards {
            if !names.contains(&card.as_str()) {
                names.push(card);
            }
        }
        vec![keyed!(
            "keyMove.line.goalWorkBranch",
            count = cards.len(),
            cards = crate::text::List::of(names)
        )]
    };
    crate::line_move::move_card_docs(
        conn,
        project,
        &crate::write::history::migrated(keyed!("history.migrate.goalWorkBranch")),
        &move_work_branch_doc,
        &said,
    )
}

/// The project's own kinds with every field of the kind `pr` of the kind `mr`.
fn kinds_mr(conn: &Connection, project: &str) -> Result<(), String> {
    use rusqlite::OptionalExtension;
    let text: Option<String> = conn
        .query_row("SELECT kinds FROM projects WHERE id = ?", [project], |r| {
            r.get(0)
        })
        .optional()
        .map_err(sql)?
        .flatten();
    let Some(mut kinds) = text.and_then(|t| serde_json::from_str::<Value>(&t).ok()) else {
        return Ok(());
    };
    fn walk(value: &mut Value) -> bool {
        let mut moved = false;
        match value {
            Value::Object(map) => {
                if map.get("fieldtype").and_then(Value::as_str) == Some("pr") {
                    map.insert("fieldtype".into(), "mr".into());
                    moved = true;
                }
                for inner in map.values_mut() {
                    moved |= walk(inner);
                }
            }
            Value::Array(list) => {
                for inner in list {
                    moved |= walk(inner);
                }
            }
            _ => {}
        }
        moved
    }
    if walk(&mut kinds) {
        conn.execute(
            "UPDATE projects SET kinds = ? WHERE id = ?",
            rusqlite::params![kinds.to_string(), project],
        )
        .map_err(sql)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn obj(v: Value) -> Obj {
        v.as_object().unwrap().clone()
    }

    #[test]
    fn a_goals_fields_go_by_their_names_of_now_and_nothing_else_moves() {
        let doc = obj(
            json!({ "id": "g", "kind": "goal", "name": "Ship", "section": "s", "v": 3,
            "fields": { "label": "1. 出す", "状態": "進行中", "担当": "Claude", "優先度": "高", "期限": "2026-10-31", "body": "完成系" } }),
        );
        let moved = move_doc(&doc).unwrap();
        assert_eq!(
            Value::Object(moved["fields"].as_object().unwrap().clone()),
            json!({ "label": "1. 出す", "status": "進行中", "担当": "Claude", "priority": "高", "due": "2026-10-31", "body": "完成系" })
        );
        let keys: Vec<&String> = moved["fields"].as_object().unwrap().keys().collect();
        assert_eq!(keys, ["label", "status", "担当", "priority", "due", "body"]);
        assert_eq!(moved["v"], crate::kinds::builtin().version);
        // Moved once: nothing more the second time.
        assert!(move_doc(&moved).is_none());
        // A task keeps what it had (a field it no longer has stays, 「定義に無い欄」).
        let task = obj(
            json!({ "id": "t", "kind": "task", "name": "a", "section": "s", "v": 3,
            "fields": { "done": true, "優先度": "高", "担当": "Claude" } }),
        );
        assert!(move_doc(&task).is_none());
        // A name of now held already: the one of before stays beside it.
        let both = obj(
            json!({ "id": "g", "kind": "goal", "name": "B", "section": "s", "v": 4,
            "fields": { "status": "完了", "状態": "進行中" } }),
        );
        assert!(move_doc(&both).is_none());
    }

    #[test]
    fn a_goals_pr_goes_by_mr_and_nothing_else_moves() {
        let doc = obj(
            json!({ "id": "g", "kind": "goal", "name": "Ship", "section": "s", "v": 4,
            "fields": { "status": "進行中", "branch": "b", "PR": 3, "body": "完成系" } }),
        );
        let moved = move_mr_doc(&doc).unwrap();
        let keys: Vec<&String> = moved["fields"].as_object().unwrap().keys().collect();
        assert_eq!(keys, ["status", "branch", "MR", "body"]);
        assert_eq!(moved["fields"]["MR"], 3);
        assert!(move_mr_doc(&moved).is_none());
        // A task's PR of before stays as it was, 「定義に無い欄」.
        let task = obj(
            json!({ "id": "t", "kind": "task", "name": "a", "section": "s", "v": 4, "fields": { "PR": 2 } }),
        );
        assert!(move_mr_doc(&task).is_none());
    }

    #[test]
    fn a_projects_own_kinds_take_mr_for_pr() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE projects (id TEXT PRIMARY KEY, kinds TEXT)")
            .unwrap();
        let kinds = json!({ "cards": [{ "kind": "work", "fields": [
            { "name": "req", "fieldtype": "pr" },
            { "name": "more", "fieldtype": "group", "fields": [{ "name": "inner", "fieldtype": "pr" }] },
            { "name": "note", "fieldtype": "text" }
        ] }] });
        conn.execute(
            "INSERT INTO projects (id, kinds) VALUES ('p', ?)",
            [kinds.to_string()],
        )
        .unwrap();
        kinds_mr(&conn, "p").unwrap();
        let now: String = conn
            .query_row("SELECT kinds FROM projects WHERE id = 'p'", [], |r| {
                r.get(0)
            })
            .unwrap();
        let now: Value = serde_json::from_str(&now).unwrap();
        let fields = &now["cards"][0]["fields"];
        assert_eq!(fields[0]["fieldtype"], "mr");
        assert_eq!(fields[1]["fields"][0]["fieldtype"], "mr");
        assert_eq!(fields[2]["fieldtype"], "text");
    }
}
