//! What is directly under each root written into the project (the store's mark
//! `core_0010_plan_tree`).
//!
//! Before, main's root (and every branch's) held one section of each built-in kind fixed there
//! (document・schema・chart・free) and the project's own kinds made there, the board's its goals
//! section; their names were each project's. Now the project says them (`projects.kinds`' `project_tree`): each
//! root's sections as they are, in the order they were shown — the built-in kinds in their order,
//! the project's own after them — their names as they are. A section a branch made itself directly
//! under the root goes after main's, into main (a name one already has, numbered: `Tests_2`), as
//! one of the project's now; what it holds stays the branch's. A section there whose kind no tree takes
//! (one of before, a project's own kind gone) is left as it is, not in the list.
//!
//! Their order is read from `project_tree` now (`sort_order` is left as it was). Nothing of the
//! tables changes.

use crate::kinds::{Defs, ProjectTree, TopSection, Tree};
use crate::store::{BOARD, Reader, sql};
use rusqlite::{Connection, params};
use serde_json::{Map, Value};

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

/// The order a section directly under a root was shown in before: the built-in kinds fixed there
/// in their order — a definition section where a new project has it, after the schemas — the
/// settings of before after them, the rest last (`SectionKind.rootOrder`).
fn rank(kind: Option<&str>) -> i64 {
    match kind {
        Some("document") => 0,
        Some("schema") => 1,
        Some("definition") => 2,
        Some("chart") => 3,
        Some("free") => 4,
        Some("goals") => 4,
        Some("export") => 5,
        Some("rules") => 6,
        _ => 8,
    }
}

/// A section directly under a root: its id, name and kind.
type Under = (String, String, Option<String>);

fn text<'d>(doc: &'d Map<String, Value>, key: &str) -> &'d str {
    doc.get(key).and_then(Value::as_str).unwrap_or_default()
}

/// A root's sections as the tree sees them, in the order they were shown: id, name, kind, and
/// whether the tree made it itself (a branch's own, not main's).
fn under_root(
    conn: &Connection,
    project: &str,
    tree: Option<&str>,
) -> Result<(Option<String>, Vec<Under>), String> {
    let seen = Reader { conn }.compose(project, tree)?;
    let root = seen
        .sections
        .iter()
        .find(|s| s.doc.get("parent_id").and_then(Value::as_str).is_none())
        .map(|s| s.id.clone());
    let mut under: Vec<(i64, i64, usize, String, String, Option<String>)> = seen
        .sections
        .iter()
        .enumerate()
        .filter(|(_, s)| {
            root.is_some() && s.doc.get("parent_id").and_then(Value::as_str) == root.as_deref()
        })
        .map(|(i, s)| {
            let kind = s
                .doc
                .get("kind")
                .and_then(Value::as_str)
                .map(str::to_string);
            (
                rank(kind.as_deref()),
                s.doc.get("sort_order").and_then(Value::as_i64).unwrap_or(0),
                i,
                s.id.clone(),
                text(&s.doc, "name").to_string(),
                kind,
            )
        })
        .collect();
    under.sort_by_key(|(rank, order, i, ..)| (*rank, *order, *i));
    Ok((
        root,
        under
            .into_iter()
            .map(|(.., id, name, kind)| (id, name, kind))
            .collect(),
    ))
}

/// A section named anew in its row (a branch's moved into main by a name taken there).
fn rename(
    conn: &Connection,
    project: &str,
    tree: &str,
    id: &str,
    name: &str,
) -> Result<(), String> {
    let doc: Option<String> = conn
        .query_row(
            "SELECT doc FROM section_docs WHERE project_id = ? AND branch_id = ? AND id = ?",
            params![project, tree, id],
            |r| r.get(0),
        )
        .map_err(sql)?;
    let Some(mut doc) = doc
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .and_then(|v| v.as_object().cloned())
    else {
        return Ok(());
    };
    doc.insert("name".into(), name.into());
    conn.execute(
        "UPDATE section_docs SET doc = ?, name = ? WHERE project_id = ? AND branch_id = ? AND id = ?",
        params![
            Value::Object(doc.clone()).to_string(),
            text(&doc, "name"),
            project,
            tree,
            id
        ],
    )
    .map_err(sql)?;
    Ok(())
}

/// One project's roots written into its kinds, unless they are there already.
pub fn move_project(conn: &Connection, project: &str) -> Result<(), String> {
    let kept: Option<String> = conn
        .query_row("SELECT kinds FROM projects WHERE id = ?", [project], |r| {
            r.get(0)
        })
        .map_err(sql)?;
    let mut kinds: Map<String, Value> = kept
        .as_deref()
        .and_then(|t| serde_json::from_str::<Value>(t).ok())
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default();
    if kinds.contains_key("project_tree") {
        return Ok(());
    }
    let defs = Defs::of_project(kept.as_deref());
    let mut tree = ProjectTree {
        main: Vec::new(),
        board: Vec::new(),
    };
    let mut names: Vec<String> = Vec::new();
    let takes = |kind: &Option<String>, on: Tree| {
        kind.as_deref()
            .and_then(|k| defs.section(k))
            .is_some_and(|d| d.trees.contains(&on))
    };
    // main's, then what a branch made itself there, into main.
    let (main_root, main_under) = under_root(conn, project, None)?;
    let mut main_ids: Vec<(String, Option<String>)> = Vec::new();
    for (id, name, kind) in &main_under {
        if takes(kind, Tree::Main) && !names.contains(&crate::pathtext::fold(name)) {
            names.push(crate::pathtext::fold(name));
            tree.main
                .push(TopSection::new(&kind.clone().unwrap_or_default(), name));
            main_ids.push((id.clone(), None));
        }
    }
    let branches: Vec<String> = {
        let mut stmt = conn
            .prepare(
                "SELECT id FROM branches WHERE project_id = ? ORDER BY sort_order, created_at, id",
            )
            .map_err(sql)?;
        stmt.query_map([project], |r| r.get(0))
            .map_err(sql)?
            .collect::<Result<_, _>>()
            .map_err(sql)?
    };
    for branch in &branches {
        let (_, under) = under_root(conn, project, Some(branch))?;
        for (id, name, kind) in under {
            if main_under.iter().any(|(m, ..)| *m == id) || !takes(&kind, Tree::Main) {
                continue;
            }
            let own_made: bool = conn
                .query_row(
                    "SELECT base IS NULL FROM section_docs WHERE project_id = ? AND branch_id = ? AND id = ?",
                    params![project, branch, id],
                    |r| r.get(0),
                )
                .unwrap_or(false);
            if !own_made {
                continue;
            }
            let mut named = name.clone();
            let mut n = 2;
            while names.contains(&crate::pathtext::fold(&named)) {
                named = format!("{name}_{n}");
                n += 1;
            }
            names.push(crate::pathtext::fold(&named));
            // Into main, as one of the project's: what it holds stays the branch's.
            conn.execute(
                "UPDATE section_docs SET branch_id = '' WHERE project_id = ? AND branch_id = ? AND id = ?",
                params![project, branch, id],
            )
            .map_err(sql)?;
            tree.main
                .push(TopSection::new(&kind.clone().unwrap_or_default(), &named));
            main_ids.push((id, (named != name).then_some(named)));
        }
    }
    // The board's.
    let (_, board_under) = under_root(conn, project, Some(BOARD))?;
    for (_, name, kind) in &board_under {
        if takes(kind, Tree::Board) && !names.contains(&crate::pathtext::fold(name)) {
            names.push(crate::pathtext::fold(name));
            tree.board
                .push(TopSection::new(&kind.clone().unwrap_or_default(), name));
        }
    }
    // A root with nothing it may hold keeps a new project's (it gets them when written next).
    if tree.main.is_empty() {
        tree.main = ProjectTree::default().main;
    }
    if tree.board.is_empty() {
        tree.board = ProjectTree::default().board;
    }
    if main_root.is_some() {
        for (id, renamed) in &main_ids {
            if let Some(name) = renamed {
                rename(conn, project, "", id, name)?;
            }
        }
    }
    kinds.insert("project_tree".into(), tree.json());
    conn.execute(
        "UPDATE projects SET kinds = ? WHERE id = ?",
        params![Value::Object(kinds).to_string(), project],
    )
    .map_err(sql)?;
    Ok(())
}
