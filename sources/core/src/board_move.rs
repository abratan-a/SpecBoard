//! The board's sections for what is finished and what was cancelled (the store's mark
//! `core_0022_board_sections`, a project file before version 24): each goals section directly under
//! the board's root holds one section of the kind `complete` and one of `cancelled`, where the card
//! actions move a goal or a task they finish or cancel
//! (`specboard://SpecBoard/Docs/Design:DesignBoard` §9).
//!
//! A board of before kept such cards in sections of its own, of the kind goals, named for them: one
//! of those directly inside the Goals section (its name in any case: `Complete`, `Completed`,
//! `Cancelled`, `Canceled`) becomes the kind it stood for, with all it holds. A Goals section
//! without one is given one, after its other sections. Nothing else of the board changes.
//!
//! The same is done for a project made now and for a goals section put directly under the board's
//! root (`project_tree`), through the transaction that makes it.

use crate::kinds::{CANCELLED, COMPLETE, GOALS};
use crate::store::{BOARD, Composition, Reader, sql};
use crate::write::{Tx, new_id};
use rusqlite::{Connection, params};
use serde_json::{Map, Value};

type Doc = Map<String, Value>;

/// Each kind the actions move cards into, the name a section of it is given, and the names of
/// before a goals section of it had.
const KINDS: [(&str, &str, &[&str]); 2] = [
    (COMPLETE, "Complete", &["complete", "completed"]),
    (CANCELLED, "Cancelled", &["cancelled", "canceled"]),
];

fn text<'d>(doc: &'d Doc, key: &str) -> &'d str {
    doc.get(key).and_then(Value::as_str).unwrap_or_default()
}

/// The board's section documents to write, so that each goals section directly under its root
/// holds one of each kind: a section of before turned into it, or a new one.
fn planned(seen: &Composition, at: &str) -> Vec<Doc> {
    let docs: Vec<&Doc> = seen.sections.iter().map(|s| &s.doc).collect();
    let Some(root) = docs
        .iter()
        .find(|d| d.get("parent_id").is_none_or(Value::is_null))
        .map(|d| text(d, "id").to_string())
    else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for goals in docs
        .iter()
        .filter(|d| text(d, "parent_id") == root && text(d, "kind") == GOALS)
    {
        let id = text(goals, "id");
        let inside: Vec<&&Doc> = docs.iter().filter(|d| text(d, "parent_id") == id).collect();
        let mut next = inside
            .iter()
            .filter_map(|d| d.get("sort_order").and_then(Value::as_i64))
            .max()
            .map_or(inside.len() as i64, |n| n + 1);
        for (kind, name, before) in KINDS {
            if inside.iter().any(|d| text(d, "kind") == kind) {
                continue;
            }
            let of_before = inside.iter().find(|d| {
                text(d, "kind") == GOALS
                    && before
                        .iter()
                        .any(|b| text(d, "name").eq_ignore_ascii_case(b))
            });
            if let Some(found) = of_before {
                let mut doc = (**found).clone();
                doc.insert("kind".into(), kind.into());
                out.push(doc);
                continue;
            }
            // A name no section beside it has (a section of another kind may have this one).
            let taken = |n: &str| {
                inside
                    .iter()
                    .any(|d| crate::pathtext::same(text(d, "name"), n))
            };
            let mut named = name.to_string();
            let mut number = 2;
            while taken(&named) {
                named = format!("{name}_{number}");
                number += 1;
            }
            let mut doc = Map::new();
            doc.insert("id".into(), new_id().into());
            doc.insert("parent_id".into(), id.into());
            doc.insert("name".into(), named.into());
            if next != 0 {
                doc.insert("sort_order".into(), next.into());
            }
            doc.insert("created_at".into(), at.into());
            doc.insert("kind".into(), kind.into());
            next += 1;
            out.push(doc);
        }
    }
    out
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

/// One project's board moved: a store's, or a project file's of before once it is read into the
/// store. Written in place, as the documents of before were; no commit says it.
pub fn move_project(conn: &Connection, project: &str) -> Result<(), String> {
    let seen = Reader { conn }.compose(project, Some(BOARD))?;
    let at = crate::write::now(conn)?;
    for doc in planned(&seen, &at) {
        let parent = doc.get("parent_id").and_then(Value::as_str);
        conn.execute(
            "INSERT INTO section_docs (project_id, branch_id, id, parent_id, name, removed, doc) VALUES (?1, ?2, ?3, ?4, ?5, 0, ?6)
             ON CONFLICT (project_id, branch_id, id) DO UPDATE SET doc = excluded.doc",
            params![
                project,
                BOARD,
                text(&doc, "id"),
                parent,
                text(&doc, "name"),
                Value::Object(doc.clone()).to_string()
            ],
        )
        .map_err(sql)?;
    }
    Ok(())
}

/// The board's goals sections directly under its root given what they lack, in the transaction
/// that made them (a new project, `project_tree`): in the board's commit.
pub(crate) fn made(tx: &mut Tx, project: &str) -> Result<(), String> {
    let seen = tx.reader().compose(project, Some(BOARD))?;
    let at = tx.now()?;
    for doc in planned(&seen, &at) {
        tx.put_section(project, BOARD, &doc)?;
    }
    Ok(())
}
