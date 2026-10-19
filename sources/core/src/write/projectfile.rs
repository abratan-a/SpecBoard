//! A project file read in (`readProjectFile` in the Swift app's Store; `specboard://SpecBoard/Docs/Design:Overview` §10):
//! in place of the project with its id, or beside everything as a project of its own with every
//! id made anew — and archived, when it is the copy kept of a project about to be written
//! over (`archive_copy`).
//!
//! A file of version 11 or later is the core's own (`crate::projectfile`): its documents go in as
//! they are. Older ones — the Swift app's, and the core's first named by the name before — are
//! refused. One before 12 has
//! its cards' lines moved into their connections, as a store's are (`line_move`), one before 13 what a container holds inside it
//! (`container_move`), one before 14 a goal's fields by their names in English (`key_move`), one before 15 a goal's `PR` named `MR`, and a designating line of any that lands in another card
//! than the one it reaches split in two (`line_move::split_project`); then
//! every card is checked by the definitions, and a file with one that does not fit is
//! refused whole. The form is named `specboard/project`.
//! Each tree's history starts again from one commit that says the file was read in.

use super::{Tx, db, now};
use crate::store::BOARD;
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Map, Value};
use std::collections::HashMap;

/// Where this Mac writes a project out, kept over reading a file in its place.
struct MacSettings {
    folders: Vec<(String, String)>,
    /// The default folder of each export (`export_default_folders`), by the export's id.
    defaults: Vec<(String, String)>,
    root: Option<(String, i64)>,
}

/// Where a file's project goes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Landing {
    /// In place of the project with its id, if there is one.
    Replacing,
    /// Beside everything, as a project of its own named so: every id made anew.
    Copy { name: String },
}

/// A branch of a file: its id, and its entry (its rows and prints).
type FileBranch<'a> = (String, &'a Map<String, Value>);

/// Which project a file holds, read before deciding where it goes: its id and name.
pub fn peek(text: &str) -> Option<(String, String)> {
    let mut top: Value = serde_json::from_str(text).ok()?;
    crate::projectfile::named_now(&mut top);
    if !crate::projectfile::is_project_format(top.get("format")?.as_str()) {
        return None;
    }
    let project = top.get("project")?.as_object()?;
    Some((
        project.get("id")?.as_str()?.to_string(),
        project
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
    ))
}

fn object(value: Option<&Value>) -> Option<&Map<String, Value>> {
    value.and_then(Value::as_object)
}

fn list(value: Option<&Value>) -> &[Value] {
    value
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

fn text<'a>(doc: &'a Map<String, Value>, key: &str) -> &'a str {
    doc.get(key).and_then(Value::as_str).unwrap_or_default()
}

/// A row from a file's object: the columns the table has, JSON columns as their text,
/// `owner` set to the project.
fn insert(
    conn: &Connection,
    table: &str,
    row: &Map<String, Value>,
    owner: Option<(&str, &str)>,
    json: &[&str],
) -> Result<(), String> {
    let columns: Vec<String> = crate::rows::columns(conn, table)?
        .into_iter()
        .map(|c| c.name)
        .collect();
    let mut names = Vec::new();
    let mut binds: Vec<rusqlite::types::Value> = Vec::new();
    for column in &columns {
        if let Some((key, value)) = owner
            && key == column
        {
            names.push(column.clone());
            binds.push(value.to_string().into());
            continue;
        }
        let Some(value) = row.get(column) else {
            continue;
        };
        names.push(column.clone());
        binds.push(match value {
            Value::Null => rusqlite::types::Value::Null,
            _ if json.contains(&column.as_str()) => value.to_string().into(),
            Value::String(s) => s.clone().into(),
            Value::Bool(b) => (*b as i64).into(),
            Value::Number(n) => n
                .as_i64()
                .map(rusqlite::types::Value::from)
                .or_else(|| n.as_f64().map(rusqlite::types::Value::from))
                .unwrap_or(rusqlite::types::Value::Null),
            other => other.to_string().into(),
        });
    }
    conn.execute(
        &format!(
            "INSERT INTO {table} ({}) VALUES ({})",
            names.join(", "),
            vec!["?"; names.len()].join(", ")
        ),
        rusqlite::params_from_iter(binds),
    )
    .map_err(db)?;
    Ok(())
}

/// A slug no other project has: `keeping` is the one the project already had.
pub(crate) fn free_slug(
    conn: &Connection,
    name: &str,
    keeping: Option<&str>,
) -> Result<String, String> {
    let mut stmt = conn.prepare("SELECT slug FROM projects").map_err(db)?;
    let taken: std::collections::HashSet<String> = stmt
        .query_map([], |r| r.get(0))
        .map_err(db)?
        .collect::<Result<_, _>>()
        .map_err(db)?;
    let base = keeping
        .map(str::to_string)
        .unwrap_or_else(|| super::slugify(name));
    if !taken.contains(&base) {
        return Ok(base);
    }
    let mut n = 2;
    while taken.contains(&format!("{base}-{n}")) {
        n += 1;
    }
    Ok(format!("{base}-{n}"))
}

/// Whether `text` at `at` begins a UUID (8-4-4-4-12 hex digits).
fn uuid_at(text: &[u8], at: usize) -> bool {
    let groups = [8, 4, 4, 4, 12];
    let mut i = at;
    for (n, len) in groups.iter().enumerate() {
        for _ in 0..*len {
            if i >= text.len() || !text[i].is_ascii_hexdigit() {
                return false;
            }
            i += 1;
        }
        if n < 4 {
            if text.get(i) != Some(&b'-') {
                return false;
            }
            i += 1;
        }
    }
    true
}

/// Every id in the file made anew, wherever it is written — as a value, or inside one.
fn renewing_ids(file: &Value) -> Value {
    fn collect(value: &Value, ids: &mut HashMap<String, String>) {
        match value {
            Value::Object(o) => {
                // A working directory's section and its files are known by the directory's id
                // and a path (`workdir::dir_id`): renewed with the directory's id inside them.
                if let Some(Value::String(id)) = o.get("id")
                    && !id.starts_with("dir:")
                    && !id.starts_with("file:")
                {
                    ids.entry(id.clone()).or_insert_with(super::new_id);
                }
                o.values().for_each(|v| collect(v, ids));
            }
            Value::Array(a) => a.iter().for_each(|v| collect(v, ids)),
            _ => {}
        }
    }
    let mut ids = HashMap::new();
    collect(file, &mut ids);
    let renew = |text: &str| -> String {
        if let Some(whole) = ids.get(text) {
            return whole.clone();
        }
        let bytes = text.as_bytes();
        let mut out = String::new();
        let mut i = 0;
        while i < bytes.len() {
            if uuid_at(bytes, i)
                && let Some(next) = ids.get(&text[i..i + 36])
            {
                out.push_str(next);
                i += 36;
                continue;
            }
            let c = text[i..].chars().next().unwrap();
            out.push(c);
            i += c.len_utf8();
        }
        out
    };
    fn walk(value: &Value, renew: &dyn Fn(&str) -> String) -> Value {
        match value {
            Value::String(s) => Value::String(renew(s)),
            Value::Array(a) => Value::Array(a.iter().map(|v| walk(v, renew)).collect()),
            Value::Object(o) => {
                Value::Object(o.iter().map(|(k, v)| (renew(k), walk(v, renew))).collect())
            }
            other => other.clone(),
        }
    }
    walk(file, &renew)
}

impl Tx<'_> {
    /// A project file into the store. Returns the id of the project it became.
    pub fn read_project_file(
        &mut self,
        file_text: &str,
        landing: &Landing,
        archived: bool,
    ) -> Result<String, String> {
        let mut file: Value =
            serde_json::from_str(file_text).map_err(|_| text!("write.projectFile.notJson"))?;
        crate::projectfile::named_now(&mut file);
        crate::export_move::move_file(&mut file);
        let top = file
            .as_object()
            .ok_or_else(|| text!("write.projectFile.notJson"))?;
        if !crate::projectfile::is_project_format(top.get("format").and_then(Value::as_str)) {
            return Err(text!(
                "write.projectFile.notProjectFile",
                format = crate::projectfile::FORMAT
            ));
        }
        let version = top.get("version").and_then(Value::as_i64).unwrap_or(0);
        if version > crate::projectfile::VERSION {
            return Err(text!("write.projectFile.newerApp"));
        }
        if version < 11 {
            return Err(text!("write.projectFile.oldVersion", version));
        }
        if object(top.get("project"))
            .and_then(|p| p.get("id"))
            .and_then(Value::as_str)
            .is_none()
        {
            return Err(text!("write.projectFile.noProject"));
        }
        // The check's marks name their items by what they are, not by any id the file has: kept
        // as they are when the ids are made anew.
        let marks: Vec<Value> = list(top.get("inspectionMarks")).to_vec();
        if let Landing::Copy { .. } = landing {
            file = renewing_ids(&file);
        }
        let mut body = file.as_object().cloned().unwrap_or_default();
        let mut project = object(body.get("project")).cloned().unwrap_or_default();
        let project_id = text(&project, "id").to_string();
        if text(&project, "name").trim().is_empty() {
            return Err(text!("write.projectFile.noName"));
        }
        // This Mac's settings for the project are not the file's: in place of the project, they
        // stay as they were (`Repository.replace`).
        let here = match landing {
            Landing::Replacing => Some(self.mac_settings(&project_id)?),
            Landing::Copy { .. } => None,
        };
        // What MCP may write without asking is this store's, as the keys that reach it are: a
        // file's is not read — a file could hand itself any permission. In place of a project it
        // stays as it was; a project read in anew starts from the defaults.
        let writes_here: Option<Value> = match landing {
            Landing::Replacing => self
                .conn()
                .query_row(
                    "SELECT mcp_writes FROM projects WHERE id = ?",
                    [&project_id],
                    |r| r.get::<_, Option<String>>(0),
                )
                .optional()
                .map_err(db)?
                .flatten()
                .and_then(|t| serde_json::from_str(&t).ok()),
            Landing::Copy { .. } => None,
        };
        project.remove("mcp_writes");
        if let Some(writes) = writes_here {
            project.insert("mcp_writes".into(), writes);
        }
        match landing {
            Landing::Copy { name } => {
                project.insert("name".into(), name.clone().into());
                let slug = free_slug(self.conn(), name, None)?;
                project.insert("slug".into(), slug.into());
            }
            Landing::Replacing => {
                self.conn()
                    .execute(
                        "UPDATE projects SET archived_at = NULL WHERE id = ?",
                        [&project_id],
                    )
                    .map_err(db)?;
                self.conn()
                    .execute("DELETE FROM projects WHERE id = ?", [&project_id])
                    .map_err(db)?;
                let slug = text(&project, "slug").to_string();
                if !slug.is_empty() {
                    let slug = super::safe_slug(&slug);
                    let free = free_slug(self.conn(), &slug, Some(&slug))?;
                    project.insert("slug".into(), free.into());
                }
            }
        }
        // Not kept: when it was archived (a file read in is worked on, or archived anew).
        project.remove("archived_at");
        let conn = self.conn();
        insert(
            conn,
            "projects",
            &project,
            None,
            &[
                "tag_groups",
                "rules",
                "export_dirs",
                "mcp_writes",
                "kinds",
                "exports",
            ],
        )?;
        if let Some(view) = object(body.get("default_view")) {
            insert(
                conn,
                "default_view",
                view,
                Some(("project_id", &project_id)),
                &[],
            )?;
        }
        for mark in marks.iter().filter_map(Value::as_object) {
            let said = |key: &str| mark.get(key).and_then(Value::as_str).unwrap_or("");
            if said("id").is_empty() {
                continue;
            }
            conn.execute(
                "INSERT OR REPLACE INTO inspection_marks (project_id, id, grp, print, reason, by, at) VALUES (?, ?, ?, ?, ?, ?, ?)",
                params![
                    project_id,
                    said("id"),
                    said("group"),
                    said("print"),
                    said("reason"),
                    said("by"),
                    said("at")
                ],
            )
            .map_err(db)?;
        }
        // What was recorded of the lines' code, by the line's id (made anew with the rest when the
        // ids are).
        for mark in list(body.get("codeMarks"))
            .iter()
            .filter_map(Value::as_object)
        {
            let said = |key: &str| mark.get(key).and_then(Value::as_str).unwrap_or("");
            if said("line").is_empty() || said("file").is_empty() || said("digest").is_empty() {
                continue;
            }
            conn.execute(
                "INSERT OR REPLACE INTO code_marks (project_id, line_id, file_id, symbol, digest, by, at) VALUES (?, ?, ?, ?, ?, ?, ?)",
                params![
                    project_id,
                    said("line"),
                    said("file"),
                    said("symbol"),
                    said("digest"),
                    said("by"),
                    said("at")
                ],
            )
            .map_err(db)?;
        }
        body.insert("project".into(), Value::Object(project));
        self.read_documents(&body, &project_id, version)?;
        // Before 12, a card's fields kept its lines: its connections now.
        if version < 12 {
            crate::line_move::move_project(self.conn(), &project_id)?;
        }
        // Before 13, a flow's nodes were its rows and a goal held its cards as a list: cards
        // inside them now.
        if version < 13 {
            crate::container_move::move_project(self.conn(), &project_id)?;
        }
        // Before 14, a goal kept its fields by their Japanese names: in English now.
        if version < 14 {
            crate::key_move::move_project(self.conn(), &project_id)?;
        }
        // Before 15, a goal's merge request was its `PR`, of the kind of field `pr`: `MR` and `mr` now, in the project's own kinds too.
        if version < 15 {
            crate::key_move::move_mr_project(self.conn(), &project_id)?;
        }
        // Before 17, a goal's working branch was its `branch`, hidden under the `branch` MCP's
        // answers head with: `workBranch` now.
        if version < 17 {
            crate::key_move::move_work_branch_project(self.conn(), &project_id)?;
        }
        // Before 23, the built-in kind of section `definition` was `instances`, and the name a
        // project's own kind's.
        if version < 23 {
            crate::definition_move::move_project(self.conn(), &project_id)?;
        }
        // Before 16, what is directly under each root was each tree's: the project's now
        // (`project_tree`), written from the roots as they are.
        crate::tree_move::move_project(self.conn(), &project_id)?;
        // Before 24, the board's goals sections held no sections for what is finished and what
        // was cancelled.
        if version < 24 {
            crate::board_move::move_project(self.conn(), &project_id)?;
        }
        // Before 25, the working directories were a tree of their own beside main: sections of
        // main and every branch now.
        if version < 25 {
            crate::workdir_move::move_project(self.conn(), &project_id)?;
        }
        // A designating line lands inside the card it reaches: one a
        // file of before keeps landing in another card, split in two.
        crate::line_move::split_project(self.conn(), &project_id)?;
        // Each said by its card, and its tree when that is not main.
        let problems: Vec<String> = crate::line_move::problems(self.conn(), Some(&project_id))?
            .into_iter()
            .map(|p| {
                let rest = &p[project_id.len() + 1..];
                rest.strip_prefix('/').unwrap_or(rest).to_string()
            })
            .collect();
        if !problems.is_empty() {
            return Err(text!(
                "write.projectFile.cardsMismatch",
                problems = problems.join("\n")
            ));
        }
        if let Some(here) = here {
            self.restore_mac_settings(&project_id, &here)?;
        }
        if archived {
            let at = now(self.conn())?;
            self.conn()
                .execute(
                    "UPDATE projects SET archived_at = ? WHERE id = ?",
                    params![at, project_id],
                )
                .map_err(db)?;
        }
        self.touch(&project_id, "");
        Ok(project_id)
    }

    /// Where this Mac writes the project out: the folder of each export directory, and the
    /// export root (`export_dir_folders`, `project_export_root`).
    fn mac_settings(&self, project: &str) -> Result<MacSettings, String> {
        let conn = self.conn();
        let mut folders = Vec::new();
        {
            let mut stmt = conn
                .prepare("SELECT dir_id, folder FROM export_dir_folders WHERE project_id = ?")
                .map_err(db)?;
            let rows = stmt
                .query_map([project], |r| {
                    Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
                })
                .map_err(db)?;
            for row in rows {
                folders.push(row.map_err(db)?);
            }
        }
        let mut defaults = Vec::new();
        {
            let mut stmt = conn
                .prepare(
                    "SELECT export_id, folder FROM export_default_folders WHERE project_id = ?",
                )
                .map_err(db)?;
            let rows = stmt
                .query_map([project], |r| {
                    Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
                })
                .map_err(db)?;
            for row in rows {
                defaults.push(row.map_err(db)?);
            }
        }
        let root = conn
            .query_row(
                "SELECT folder, with_project_file FROM project_export_root WHERE project_id = ?",
                [project],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)),
            )
            .optional()
            .map_err(db)?;
        Ok(MacSettings {
            folders,
            defaults,
            root,
        })
    }

    /// This Mac's settings put back for the project read in: a folder only for a directory
    /// the file still has.
    fn restore_mac_settings(&self, project: &str, here: &MacSettings) -> Result<(), String> {
        let conn = self.conn();
        let dirs: Vec<String> = conn
            .query_row(
                "SELECT export_dirs FROM projects WHERE id = ?",
                [project],
                |r| r.get::<_, Option<String>>(0),
            )
            .optional()
            .map_err(db)?
            .flatten()
            .and_then(|t| serde_json::from_str::<Value>(&t).ok())
            .and_then(|v| v.as_array().cloned())
            .unwrap_or_default()
            .iter()
            .filter_map(|d| d.get("id").and_then(Value::as_str).map(str::to_string))
            .collect();
        for (dir, folder) in &here.folders {
            if dirs.contains(dir) {
                conn.execute(
                    "INSERT OR REPLACE INTO export_dir_folders (project_id, dir_id, folder) VALUES (?, ?, ?)",
                    params![project, dir, folder],
                )
                .map_err(db)?;
            }
        }
        // A default folder only for an export the file still has.
        let exports: Vec<String> = conn
            .query_row(
                "SELECT exports FROM projects WHERE id = ?",
                [project],
                |r| r.get::<_, Option<String>>(0),
            )
            .optional()
            .map_err(db)?
            .flatten()
            .map(|t| crate::export::settings::parse(Some(&t)))
            .unwrap_or_default()
            .into_iter()
            .map(|e| e.id)
            .collect();
        for (export, folder) in &here.defaults {
            if exports.contains(export) {
                conn.execute(
                    "INSERT OR REPLACE INTO export_default_folders (project_id, export_id, folder) VALUES (?, ?, ?)",
                    params![project, export, folder],
                )
                .map_err(db)?;
            }
        }
        if let Some((folder, with_project_file)) = &here.root {
            conn.execute(
                "INSERT OR REPLACE INTO project_export_root (project_id, folder, with_project_file) VALUES (?, ?, ?)",
                params![project, folder, with_project_file],
            )
            .map_err(db)?;
        }
        Ok(())
    }

    /// The branches' rows of a file (both versions keep them alike): each branch's own row,
    /// returned with what it holds.
    fn read_branches<'a>(
        &self,
        body: &'a Map<String, Value>,
        project_id: &str,
    ) -> Result<Vec<FileBranch<'a>>, String> {
        let mut out = Vec::new();
        for branch in list(body.get("branches")) {
            let Some(branch) = branch.as_object() else {
                continue;
            };
            let id = text(branch, "id").to_string();
            if id.is_empty() {
                continue;
            }
            let mut row = branch.clone();
            for key in ["sections", "cards", "prints"] {
                row.remove(key);
            }
            insert(
                self.conn(),
                "branches",
                &row,
                Some(("project_id", project_id)),
                &[],
            )?;
            out.push((id, branch));
        }
        Ok(out)
    }

    /// A file of 10, 11 or 12: its documents as the core keeps them (checked by the
    /// definitions once they are in, and their lines moved: `read_project_file`).
    fn read_documents(
        &mut self,
        body: &Map<String, Value>,
        project_id: &str,
        version: i64,
    ) -> Result<(), String> {
        let main_cards: Vec<&Map<String, Value>> = list(body.get("cards"))
            .iter()
            .filter_map(Value::as_object)
            .collect();
        let board = object(body.get("board"));
        let board_cards: Vec<&Map<String, Value>> = list(board.and_then(|b| b.get("cards")))
            .iter()
            .filter_map(Value::as_object)
            .collect();
        let branches = self.read_branches(body, project_id)?;
        let conn = self.conn();
        let section = |tree: &str,
                       doc: &Map<String, Value>,
                       removed: bool,
                       base: Option<String>,
                       based_at: Option<&str>| {
            conn.execute(
                "INSERT INTO section_docs (project_id, branch_id, id, parent_id, name, removed, doc, base, based_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
                params![project_id, tree, text(doc, "id"), doc.get("parent_id").and_then(Value::as_str), text(doc, "name"), removed as i64, (!removed).then(|| Value::Object(doc.clone()).to_string()), base, based_at],
            )
            .map_err(db)
        };
        let card = |tree: &str,
                    doc: &Map<String, Value>,
                    removed: bool,
                    base: Option<String>,
                    based_at: Option<&str>| {
            conn.execute(
                "INSERT INTO card_docs (project_id, branch_id, id, section_id, name, kind, removed, doc, base, based_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                params![project_id, tree, text(doc, "id"), super::holder(doc), text(doc, "name"), text(doc, "kind"), removed as i64, (!removed).then(|| Value::Object(doc.clone()).to_string()), base, based_at],
            )
            .map_err(db)
        };
        for doc in list(body.get("sections"))
            .iter()
            .filter_map(Value::as_object)
        {
            section("", doc, false, None, None)?;
        }
        for doc in &main_cards {
            card("", doc, false, None, None)?;
        }
        let mut trees: Vec<String> = vec![String::new()];
        for (id, branch) in &branches {
            trees.push(id.clone());
            for (key, is_card) in [("sections", false), ("cards", true)] {
                for row in list(branch.get(key)).iter().filter_map(Value::as_object) {
                    let removed = row.get("removed").and_then(Value::as_bool) == Some(true);
                    let base =
                        object(row.get("base")).map(|b| Value::Object(b.clone()).to_string());
                    // A removed row keeps where it was in its base.
                    let Some(doc) = object(row.get("doc")).or(object(row.get("base"))) else {
                        continue;
                    };
                    let mut shaped = doc.clone();
                    if let Some(row_id) = row.get("id") {
                        shaped.insert("id".into(), row_id.clone());
                    }
                    let based_at = row.get("based_at").and_then(Value::as_str);
                    if is_card {
                        card(id, &shaped, removed, base, based_at)?;
                    } else {
                        section(id, &shaped, removed, base, based_at)?;
                    }
                }
            }
            for print in list(branch.get("prints"))
                .iter()
                .filter_map(Value::as_object)
            {
                conn.execute(
                    "INSERT INTO branch_prints (branch_id, card_id, print, path) VALUES (?, ?, ?, ?)",
                    params![id, text(print, "card"), text(print, "print"), text(print, "path")],
                )
                .map_err(db)?;
            }
        }
        if let Some(board) = board {
            for doc in list(board.get("sections"))
                .iter()
                .filter_map(Value::as_object)
            {
                section(BOARD, doc, false, None, None)?;
            }
            for doc in &board_cards {
                card(BOARD, doc, false, None, None)?;
            }
            trees.push(BOARD.to_string());
        }
        // A file before 12 kept the project's own kinds of card apart (`kinds`, the tables of
        // before): its row's kinds of card now.
        let before: Vec<Value> = list(body.get("kinds"))
            .iter()
            .filter_map(Value::as_object)
            .map(|kind| {
                serde_json::json!({
                    "kind": text(kind, "name"),
                    "version": kind.get("version").and_then(Value::as_i64).unwrap_or(1),
                    "doc": kind.get("doc").cloned().unwrap_or(Value::Null),
                    "fields": kind.get("fields").cloned().unwrap_or_else(|| Value::Array(Vec::new())),
                })
            })
            .collect();
        if !before.is_empty() {
            let own: Option<String> = conn
                .query_row(
                    "SELECT kinds FROM projects WHERE id = ?",
                    [project_id],
                    |r| r.get(0),
                )
                .map_err(db)?;
            let mut own = own
                .and_then(|t| serde_json::from_str::<Value>(&t).ok())
                .and_then(|v| v.as_object().cloned())
                .unwrap_or_default();
            own.insert("cards".into(), Value::Array(before));
            conn.execute(
                "UPDATE projects SET kinds = ? WHERE id = ?",
                params![Value::Object(own).to_string(), project_id],
            )
            .map_err(db)?;
        }
        for place in list(body.get("places")).iter().filter_map(Value::as_object) {
            let n = |key: &str| place.get(key).and_then(Value::as_i64);
            conn.execute(
                "INSERT OR REPLACE INTO places (project_id, branch_id, view, item_kind, item, x, y, w, h, pinned) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                params![
                    project_id,
                    text(place, "branch_id"),
                    text(place, "view"),
                    place.get("item_kind").and_then(Value::as_str).unwrap_or("card"),
                    text(place, "item"),
                    n("x"),
                    n("y"),
                    n("w"),
                    n("h"),
                    (n("pinned").unwrap_or(0) != 0) as i64
                ],
            )
            .map_err(db)?;
        }
        for look in list(body.get("looks")).iter().filter_map(Value::as_object) {
            conn.execute(
                "INSERT OR REPLACE INTO card_looks (project_id, branch_id, card_id, row_path, field, value) VALUES (?, ?, ?, ?, ?, ?)",
                params![
                    project_id,
                    text(look, "branch_id"),
                    text(look, "card_id"),
                    text(look, "row_path"),
                    text(look, "field"),
                    look.get("value").cloned().unwrap_or(Value::Null).to_string()
                ],
            )
            .map_err(db)?;
        }
        // A file's change notes (`changelog`) are let go: the table went (`schema::NO_CHANGELOG`).
        self.start_histories(project_id, &trees, version)
    }

    /// One commit in each tree that says the file was read in.
    fn start_histories(
        &mut self,
        project_id: &str,
        trees: &[String],
        version: i64,
    ) -> Result<(), String> {
        let at = now(self.conn())?;
        let author = self.who().side.word();
        let name = self.who().name.clone();
        for tree in trees {
            let id = super::new_id();
            self.conn()
                .execute(
                    "INSERT INTO commits (id, project_id, branch_id, seq, at, author_kind, author, kind) VALUES (?, ?, ?, 1, ?, ?, ?, 'import')",
                    params![id, project_id, tree, at, author, name],
                )
                .map_err(db)?;
            super::insert_line(
                self.conn(),
                &id,
                0,
                "import",
                "project",
                Some(project_id),
                &keyed!("write.projectFile.imported", version),
            )
            .map_err(db)?;
        }
        Ok(())
    }

    /// Whether there is a project with this id.
    pub fn project_exists(&self, project: &str) -> Result<bool, String> {
        self.conn()
            .query_row("SELECT 1 FROM projects WHERE id = ?", [project], |_| Ok(()))
            .map(|_| true)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(false),
                e => Err(db(e)),
            })
    }

    /// A copy of the project as it is now, archived, and the project itself left alone — for
    /// when it is about to be written over (a file read in in its place).
    pub fn archive_copy(&mut self, project: &str) -> Result<String, String> {
        let name: String = self
            .conn()
            .query_row("SELECT name FROM projects WHERE id = ?", [project], |r| {
                r.get(0)
            })
            .map_err(|_| text!("write.projectFile.projectNotFound"))?;
        let at: String = self
            .conn()
            .query_row(
                "SELECT strftime('%Y-%m-%d %H:%M', 'now', 'localtime')",
                [],
                |r| r.get(0),
            )
            .map_err(db)?;
        let text = crate::projectfile::write(self.conn(), project)?;
        self.read_project_file(
            &text,
            &Landing::Copy {
                name: text!("write.projectFile.archiveCopyName", name, at),
            },
            true,
        )
    }

    /// Set a project aside, whole, to be read and not written.
    pub fn archive_project(&mut self, project: &str) -> Result<(), String> {
        let at = now(self.conn())?;
        self.conn()
            .execute(
                "UPDATE projects SET archived_at = ? WHERE id = ? AND archived_at IS NULL",
                params![at, project],
            )
            .map_err(db)?;
        self.touch(project, "");
        Ok(())
    }

    /// Back among the projects that are worked on.
    pub fn restore_project(&mut self, project: &str) -> Result<(), String> {
        self.conn()
            .execute(
                "UPDATE projects SET archived_at = NULL WHERE id = ?",
                [project],
            )
            .map_err(db)?;
        self.touch(project, "");
        Ok(())
    }

    /// The project gone, with everything under it (out of the archive first).
    pub fn delete_project(&mut self, project: &str) -> Result<(), String> {
        self.conn()
            .execute(
                "UPDATE projects SET archived_at = NULL WHERE id = ?",
                [project],
            )
            .map_err(db)?;
        self.conn()
            .execute("DELETE FROM projects WHERE id = ?", [project])
            .map_err(db)?;
        self.touch(project, "");
        Ok(())
    }
}
