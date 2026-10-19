//! The working directories made sections of main and every branch (the store's mark
//! `core_0023_workdir_sections`, a project file before version 25).
//!
//! Before, a project's working directories (`projects.export_dirs`) were read into a tree of their
//! own beside main. Now each is a section directly under main's root and every branch's: an entry
//! `{"type": "workdir", "name", "directory"}` after the others in `project_tree`'s `main_sections`,
//! named as the directory was (`_2` after it where a section there has that name), and the section's
//! document in main by the directory's id (`workdir::dir_id`), which every branch sees. The folder
//! each is bound to on this machine stays as it was; the item MCP binds it under is
//! `section_update.folder` now (`directory.path` before). Lines to its files hold them by id, as
//! they did.

use crate::kinds::{Defs, TopSection, Tree, WORKDIR};
use crate::store::{Reader, sql};
use rusqlite::{Connection, OptionalExtension, params};
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

/// One project moved: a store's, or a project file's of before once it is read into the store.
/// Written in place, as the documents of before were; no commit says it. A directory already
/// listed is left as it is.
pub fn move_project(conn: &Connection, project: &str) -> Result<(), String> {
    folder_item(conn, project)?;
    let mut dirs = crate::tools::directories::directories(conn, project)?;
    if dirs.is_empty() {
        return Ok(());
    }
    let kinds: Option<String> = conn
        .query_row("SELECT kinds FROM projects WHERE id = ?", [project], |r| {
            r.get(0)
        })
        .map_err(sql)?;
    let mut own: Map<String, Value> = kinds
        .as_deref()
        .and_then(|t| serde_json::from_str::<Value>(t).ok())
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default();
    let mut tree = Defs::of_project(kinds.as_deref()).tree;
    let seen = Reader { conn }.compose(project, None)?;
    let root = seen
        .sections
        .iter()
        .find(|s| s.doc.get("parent_id").is_none_or(Value::is_null))
        .map(|s| s.id.clone());
    let Some(root) = root else {
        return Ok(());
    };
    // The names directly under the roots: the project tree's, and any other section there.
    let mut taken: Vec<String> = Tree::ALL
        .into_iter()
        .flat_map(|t| tree.of(t).iter().map(|s| crate::pathtext::fold(&s.name)))
        .collect();
    taken.extend(
        seen.sections
            .iter()
            .filter(|s| s.doc.get("parent_id").and_then(Value::as_str) == Some(&root))
            .filter_map(|s| s.doc.get("name").and_then(Value::as_str))
            .map(crate::pathtext::fold),
    );
    let at = crate::write::now(conn)?;
    let mut changed = false;
    for dir in dirs.iter_mut() {
        if tree
            .main
            .iter()
            .any(|t| t.directory.as_deref() == Some(dir.id.as_str()))
        {
            continue;
        }
        let mut name = dir.name.clone();
        let mut n = 2;
        while taken.contains(&crate::pathtext::fold(&name)) {
            name = format!("{}_{n}", dir.name);
            n += 1;
        }
        taken.push(crate::pathtext::fold(&name));
        dir.name = name.clone();
        let mut top = TopSection::new(WORKDIR, &name);
        top.directory = Some(dir.id.clone());
        let place = tree.main.len() as i64;
        tree.main.push(top);
        let id = crate::workdir::dir_id(&dir.id, "");
        let mut doc = Map::new();
        doc.insert("id".into(), id.clone().into());
        doc.insert("parent_id".into(), root.clone().into());
        doc.insert("name".into(), name.clone().into());
        doc.insert("sort_order".into(), place.into());
        doc.insert("created_at".into(), at.clone().into());
        doc.insert("kind".into(), WORKDIR.into());
        conn.execute(
            "INSERT INTO section_docs (project_id, branch_id, id, parent_id, name, removed, doc) VALUES (?1, '', ?2, ?3, ?4, 0, ?5)
             ON CONFLICT (project_id, branch_id, id) DO NOTHING",
            params![project, id, root, name, Value::Object(doc).to_string()],
        )
        .map_err(sql)?;
        changed = true;
    }
    if !changed {
        return Ok(());
    }
    own.insert("project_tree".into(), tree.json());
    conn.execute(
        "UPDATE projects SET kinds = ? WHERE id = ?",
        params![Value::Object(own).to_string(), project],
    )
    .map_err(sql)?;
    crate::tools::directories::set_directories(conn, project, &dirs)
}

/// The project's state of the item MCP binds a folder under, by the id it has now.
fn folder_item(conn: &Connection, project: &str) -> Result<(), String> {
    let text: Option<String> = conn
        .query_row(
            "SELECT mcp_writes FROM projects WHERE id = ?",
            [project],
            |r| r.get(0),
        )
        .optional()
        .map_err(sql)?
        .flatten();
    let Some(Value::Object(mut said)) = text.and_then(|t| serde_json::from_str::<Value>(&t).ok())
    else {
        return Ok(());
    };
    let Some(state) = said.shift_remove("directory.path") else {
        return Ok(());
    };
    let now = crate::tools::mcp_writes::renamed("directory.path").unwrap_or_default();
    if !said.contains_key(now) {
        said.insert(now.to_string(), state);
    }
    conn.execute(
        "UPDATE projects SET mcp_writes = ? WHERE id = ?",
        params![Value::Object(said).to_string(), project],
    )
    .map_err(sql)?;
    Ok(())
}
