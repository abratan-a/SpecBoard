//! A project moved whole from one store to another: from this Mac to an agent, or back. Unlike a project file
//! (`projectfile`), everything the project has goes — main, the branches, the board, the history and the
//! merge requests, the check's marks, the code its lines were recorded at, where boxes sit and how cards look, its settings — its rows as they are, its ids
//! too. What is the store's own and not the project's stays: the folders its working directories are
//! bound to on this machine, the access keys and what they were given, whether it is public, what
//! MCP may write without asking (a project comes in with the defaults), the holds and batches going on. Between two stores of one
//! machine (this Mac's own and the agent it runs for itself) the folders go as well, when asked for. Only between stores of the same form (the same marks): the app and its
//! agents are of one build. What a commit kept of a long row goes as its text, not as the list of
//! its pieces (`chunks`): a piece's number is its store's, and the store it lands in cuts the row anew.

use super::{Tx, db};
use rusqlite::{Connection, params_from_iter};
use serde_json::{Map, Value, json};
use std::collections::HashSet;

/// The tables a project is kept in, and how its rows are found (`?1` the project's id), in the order they
/// are put in (a row after what it refers to).
const TABLES: [(&str, &str); 16] = [
    ("projects", "id = ?1"),
    ("default_view", "project_id = ?1"),
    ("branches", "project_id = ?1"),
    ("card_docs", "project_id = ?1"),
    ("section_docs", "project_id = ?1"),
    ("card_looks", "project_id = ?1"),
    ("places", "project_id = ?1"),
    ("commits", "project_id = ?1"),
    (
        "commit_lines",
        "commit_id IN (SELECT id FROM commits WHERE project_id = ?1)",
    ),
    (
        "commit_rows",
        "commit_id IN (SELECT id FROM commits WHERE project_id = ?1)",
    ),
    ("merge_requests", "project_id = ?1"),
    (
        "branch_prints",
        "branch_id IN (SELECT id FROM branches WHERE project_id = ?1)",
    ),
    ("branch_frozen", "project_id = ?1"),
    ("inspection_marks", "project_id = ?1"),
    ("code_marks", "project_id = ?1"),
    ("images", "project_id = ?1"),
];

/// What a store keeps of its machine for a project: the folders its exports' defaults and its
/// working directories are bound to. They go with the project only between two stores of one
/// machine (`bundle`'s `machine`): on another machine the folders are not these.
const MACHINE_TABLES: [(&str, &str); 3] = [
    ("project_export_root", "project_id = ?1"),
    ("export_dir_folders", "project_id = ?1"),
    ("export_default_folders", "project_id = ?1"),
];

/// What a bundle from a store of another form is told.
pub fn other_form() -> String {
    text!("write.bundle.otherForm")
}

fn columns(conn: &Connection, table: &str) -> Result<Vec<String>, String> {
    let mut stmt = conn
        .prepare(&format!("PRAGMA table_info({table})"))
        .map_err(db)?;
    stmt.query_map([], |r| r.get::<_, String>(1))
        .map_err(db)?
        .collect::<Result<_, _>>()
        .map_err(db)
}

/// The marks of the store's form.
fn form(conn: &Connection) -> Result<Vec<String>, String> {
    let mut stmt = conn
        .prepare("SELECT name FROM _migrations ORDER BY name")
        .map_err(db)?;
    stmt.query_map([], |r| r.get::<_, String>(0))
        .map_err(db)?
        .collect::<Result<_, _>>()
        .map_err(db)
}

/// The project `project` whole: `{form, project, tables: {<table>: [{<column>: <value>}]}}`. With
/// `machine`, what the store keeps of this machine for it as well (`MACHINE_TABLES`).
pub fn bundle(conn: &Connection, project: &str, machine: bool) -> Result<Value, String> {
    let there: bool = conn
        .query_row(
            "SELECT EXISTS (SELECT 1 FROM projects WHERE id = ?)",
            [project],
            |r| r.get(0),
        )
        .map_err(db)?;
    if !there {
        return Err(text!("write.bundle.projectNotFound"));
    }
    let mut tables = Map::new();
    let machine: &[(&str, &str)] = if machine { &MACHINE_TABLES } else { &[] };
    for (table, which) in TABLES.iter().chain(machine).copied() {
        let names = columns(conn, table)?;
        let mut stmt = conn
            .prepare(&format!(
                "SELECT {} FROM {table} WHERE {which}",
                names.join(", ")
            ))
            .map_err(db)?;
        // Each row, and the list of pieces where a commit kept a long row so: read whole below, as
        // its text.
        let rows = stmt
            .query_map([project], |r| {
                let mut row = Map::new();
                let mut list = None;
                for (i, name) in names.iter().enumerate() {
                    let value = match r.get_ref(i)? {
                        rusqlite::types::ValueRef::Null => Value::Null,
                        rusqlite::types::ValueRef::Integer(n) => n.into(),
                        rusqlite::types::ValueRef::Real(f) => f.into(),
                        rusqlite::types::ValueRef::Text(t) => {
                            String::from_utf8_lossy(t).into_owned().into()
                        }
                        rusqlite::types::ValueRef::Blob(b)
                            if table == "commit_rows" && name == "before" =>
                        {
                            list = Some(b.to_vec());
                            Value::Null
                        }
                        rusqlite::types::ValueRef::Blob(b) => {
                            String::from_utf8_lossy(b).into_owned().into()
                        }
                    };
                    row.insert(name.clone(), value);
                }
                Ok((row, list))
            })
            .map_err(db)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db)?;
        let rows = rows
            .into_iter()
            .map(|(mut row, list)| {
                if let Some(list) = list {
                    let text = super::chunks::Before::Pieces(list).text(conn)?;
                    row.insert("before".into(), text.map_or(Value::Null, Value::from));
                }
                Ok(Value::Object(row))
            })
            .collect::<Result<Vec<_>, String>>()?;
        tables.insert(table.to_string(), Value::Array(rows));
    }
    Ok(json!({ "form": form(conn)?, "project": project, "tables": tables }))
}

fn sql_value(value: &Value) -> rusqlite::types::Value {
    use rusqlite::types::Value as Sql;
    match value {
        Value::Null => Sql::Null,
        Value::Bool(b) => Sql::Integer(i64::from(*b)),
        Value::Number(n) => n
            .as_i64()
            .map(Sql::Integer)
            .unwrap_or_else(|| Sql::Real(n.as_f64().unwrap_or(0.0))),
        Value::String(s) => Sql::Text(s.clone()),
        other => Sql::Text(other.to_string()),
    }
}

/// A history row's values as they go into this store: what it kept of a long row cut into the
/// project's pieces here (`chunks::pack`). The project's own row is in already — it comes first.
fn cut_anew(
    conn: &Connection,
    project: &str,
    names: &[String],
    mut values: Vec<rusqlite::types::Value>,
) -> Result<Vec<rusqlite::types::Value>, String> {
    if let Some(at) = names.iter().position(|name| name == "before")
        && let rusqlite::types::Value::Text(text) = &values[at]
    {
        values[at] = super::chunks::pack(conn, project, text)?;
    }
    Ok(values)
}

impl Tx<'_> {
    /// A project bundled from another store (`bundle`) put in this one, as it was: its id. Refused
    /// when this store is of another form, has the project already, or has another project by its slug.
    ///
    /// With `machine`, what the bundle holds of its machine for the project is put in as well
    /// (`MACHINE_TABLES`): asked for by who reads it in, never taken because a bundle has it —
    /// another machine's folders are not this one's.
    pub fn read_bundle(&mut self, bundle: &Value, machine: bool) -> Result<String, String> {
        let conn = self.conn();
        let theirs: Vec<String> = bundle["form"]
            .as_array()
            .map(|marks| {
                marks
                    .iter()
                    .filter_map(|m| m.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        if theirs != form(conn)? {
            return Err(other_form());
        }
        let project = bundle["project"]
            .as_str()
            .ok_or_else(|| text!("write.bundle.noId"))?
            .to_string();
        let tables = bundle["tables"]
            .as_object()
            .ok_or_else(|| text!("write.bundle.noTables"))?;
        let row = tables
            .get("projects")
            .and_then(Value::as_array)
            .and_then(|rows| rows.first())
            .filter(|row| row["id"] == project.as_str())
            .ok_or_else(|| text!("write.bundle.noRow"))?
            .clone();
        let there: bool = conn
            .query_row(
                "SELECT EXISTS (SELECT 1 FROM projects WHERE id = ?)",
                [&project],
                |r| r.get(0),
            )
            .map_err(db)?;
        if there {
            return Err(text!(
                "write.bundle.thereAlready",
                name = row["name"].as_str().unwrap_or("")
            ));
        }
        let slug = super::safe_slug(row["slug"].as_str().unwrap_or(""));
        let taken: Option<String> = conn
            .query_row("SELECT name FROM projects WHERE slug = ?", [&slug], |r| {
                r.get(0)
            })
            .ok();
        if let Some(name) = taken {
            return Err(text!("write.bundle.slugTaken", slug, name));
        }
        // What the history's rows and the branches' prints name is the bundle's own: a row naming a
        // commit or a branch this store holds for another project would be written into that one.
        let ids = |table: &str| -> HashSet<&str> {
            tables
                .get(table)
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|row| row["id"].as_str())
                .collect()
        };
        let (commits, branches) = (ids("commits"), ids("branches"));
        for (table, names, held) in [
            ("commit_lines", "commit_id", &commits),
            ("commit_rows", "commit_id", &commits),
            ("branch_prints", "branch_id", &branches),
        ] {
            let stray = tables
                .get(table)
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .any(|row| !row[names].as_str().is_some_and(|id| held.contains(id)));
            if stray {
                return Err(text!("write.bundle.strayRow", table));
            }
        }
        // An archived project's rows cannot be written: put in as worked on, archived last.
        let archived = row["archived_at"].clone();
        let machine: &[(&str, &str)] = if machine { &MACHINE_TABLES } else { &[] };
        for (table, _) in TABLES.iter().chain(machine).copied() {
            let Some(rows) = tables.get(table).and_then(Value::as_array) else {
                continue;
            };
            let names = columns(conn, table)?;
            for row in rows {
                let Some(row) = row.as_object() else { continue };
                let values: Vec<rusqlite::types::Value> = names
                    .iter()
                    .map(|name| match (table, name.as_str()) {
                        ("projects", "archived_at" | "mcp_writes") => rusqlite::types::Value::Null,
                        // Every row is the project read in: one naming another written into it.
                        (_, "project_id") => rusqlite::types::Value::Text(project.clone()),
                        ("projects", "slug") => rusqlite::types::Value::Text(slug.clone()),
                        _ => sql_value(row.get(name).unwrap_or(&Value::Null)),
                    })
                    .collect();
                let values = match table {
                    "commit_rows" => cut_anew(conn, &project, &names, values)?,
                    _ => values,
                };
                conn.execute(
                    &format!(
                        "INSERT INTO {table} ({}) VALUES ({})",
                        names.join(", "),
                        vec!["?"; names.len()].join(", ")
                    ),
                    params_from_iter(values),
                )
                .map_err(db)?;
            }
        }
        if !archived.is_null() {
            conn.execute(
                "UPDATE projects SET archived_at = ? WHERE id = ?",
                rusqlite::params![sql_value(&archived), project],
            )
            .map_err(db)?;
        }
        self.touch(&project, "");
        Ok(project)
    }
}
