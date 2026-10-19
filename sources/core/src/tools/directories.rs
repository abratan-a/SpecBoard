//! The working directories: a project's setting, as its name
//! is — not a branch's, and not held by main's lock — each a name, a description, how it is read
//! (`readType`: `none`, `basic`, `git`) and what it leaves out besides (`exclude`); and the folder it is
//! on this machine (`export_dir_folders`), apart from the project and its file. Each is a section
//! directly under main's root and every branch's (`project_tree`'s `workdir` entries, which hold it by
//! id): listing one there makes it, taking it out lets it go (`linked`, `settle`). What was read of
//! it is what that section holds (`workdir`).

use crate::write::{db, new_id};
use rusqlite::{Connection, params};
use serde_json::{Map, Value};
use std::collections::HashMap;

/// A working directory as the project keeps it.
#[derive(Clone)]
pub(crate) struct Directory {
    pub id: String,
    pub name: String,
    pub description: String,
    /// How it is read (`readType`): `none` (not read: its tree is its root alone, and the lines to its
    /// files are not checked), `basic` (all but `.git/` and `.DS_Store`) or `git` (those and what
    /// `.gitignore` says; `basic` where there is no git).
    pub read_type: String,
    /// What it leaves out besides (`exclude`): a directory by its name (`target/`) or its path from
    /// the top (`sources/core/target/`), a file by its name or path, or an extension (`.png`).
    pub exclude: Vec<String>,
    /// How its files map to languages besides the table's (`parsers`): an extension (`.h`) or a
    /// file's name (`Makefile`) to a language's id, or to none.
    pub parsers: crate::code::registry::Parsers,
}

/// The ways a working directory is read, and the one it has unless it says.
pub(crate) const READ_TYPES: [&str; 3] = ["none", "basic", "git"];
pub(crate) const DEFAULT_READ_TYPE: &str = "git";

/// A working directory's way of being read, as written.
pub(crate) fn read_type_of(raw: &Value) -> Result<String, String> {
    raw.as_str()
        .filter(|p| READ_TYPES.contains(p))
        .map(str::to_string)
        .ok_or_else(|| {
            text!(
                "directories.readType.oneOf",
                types = crate::text::List::of(READ_TYPES),
                raw
            )
        })
}

/// A working directory's `exclude`, as written: each a name, a path from the top or an extension,
/// none outside it.
pub(crate) fn exclude_of(raw: &Value) -> Result<Vec<String>, String> {
    let Some(items) = raw.as_array() else {
        return Err(text!("directories.exclude.list"));
    };
    if items.len() > 500 {
        return Err(text!("directories.exclude.most"));
    }
    let mut out: Vec<String> = Vec::new();
    for item in items {
        let Some(text) = item.as_str().map(str::trim).filter(|t| !t.is_empty()) else {
            return Err(text!("directories.exclude.item", item));
        };
        if text.starts_with('/') || text.contains('\\') || text.split('/').any(|part| part == "..")
        {
            return Err(text!("directories.exclude.inside", text));
        }
        if !out.iter().any(|o| o == text) {
            out.push(text.to_string());
        }
    }
    Ok(out)
}

/// A working directory's `parsers`, as written: each key an extension (`.h`) or a file's name
/// (`Makefile`), each value a language's id or `null` (none); `null` for all of it is the table's
/// alone. A language this machine does not have may be named (another machine may have it).
pub(crate) fn parsers_of(raw: &Value) -> Result<crate::code::registry::Parsers, String> {
    let mut out = crate::code::registry::Parsers::new();
    let items = match raw {
        Value::Null => return Ok(out),
        Value::Object(items) => items,
        _ => return Err(text!("directories.parsers.object")),
    };
    if items.len() > 500 {
        return Err(text!("directories.parsers.most"));
    }
    for (key, value) in items {
        if !crate::code::registry::parsers_key_ok(key) {
            return Err(text!("directories.parsers.key", key));
        }
        let id = match value {
            Value::Null => None,
            Value::String(id)
                if !id.is_empty()
                    && id.chars().all(|c| {
                        c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-'
                    }) =>
            {
                Some(id.clone())
            }
            other => return Err(text!("directories.parsers.language", key, other)),
        };
        out.insert(key.clone(), id);
    }
    Ok(out)
}

/// Unicode's control and format characters (`CharacterSet.controlCharacters`: Cc and Cf): none of
/// them is in a folder's path as a person means it.
fn control(c: char) -> bool {
    c.is_control()
        || matches!(c as u32,
            0xAD | 0x600..=0x605 | 0x61C | 0x6DD | 0x70F | 0x890..=0x891 | 0x8E2 | 0x180E
            | 0x200B..=0x200F | 0x202A..=0x202E | 0x2060..=0x2064 | 0x2066..=0x206F
            | 0xFEFF | 0xFFF9..=0xFFFB | 0x110BD | 0x110CD | 0x13430..=0x1343F
            | 0x1BCA0..=0x1BCA3 | 0x1D173..=0x1D17A | 0xE0001 | 0xE0020..=0xE007F)
}

/// A folder on this machine, as written: from the root or the home, no control characters; none
/// for `null` (let go).
pub(crate) fn folder_of(raw: &Value) -> Result<Option<String>, String> {
    let folder = match raw {
        Value::Null => return Ok(None),
        Value::String(s) => super::check::trim_spaces(s),
        _ => return Err(text!("directories.folder.path")),
    };
    if folder.is_empty() {
        return Ok(None);
    }
    if !(folder.starts_with('/') || folder.starts_with("~/")) {
        return Err(text!("directories.folder.absolute"));
    }
    if folder.chars().any(control) {
        return Err(text!("directories.folder.control"));
    }
    Ok(Some(folder.to_string()))
}

/// The project's working directories, in order; none when they cannot be read. A project of before
/// says how one is read as `preset`.
pub(crate) fn directories(conn: &Connection, project: &str) -> Result<Vec<Directory>, String> {
    let text: Option<String> = conn
        .query_row(
            "SELECT export_dirs FROM projects WHERE id = ?",
            [project],
            |r| r.get(0),
        )
        .map_err(db)?;
    let Some(Value::Array(items)) = text.and_then(|t| serde_json::from_str::<Value>(&t).ok())
    else {
        return Ok(Vec::new());
    };
    let read: Option<Vec<Directory>> = items
        .iter()
        .map(|item| {
            Some(Directory {
                id: item.get("id")?.as_str()?.to_string(),
                name: item.get("name")?.as_str()?.to_string(),
                description: match item.get("description") {
                    None => String::new(),
                    Some(d) => d.as_str()?.to_string(),
                },
                read_type: item
                    .get("readType")
                    .or_else(|| item.get("preset"))
                    .and_then(Value::as_str)
                    .filter(|p| READ_TYPES.contains(p))
                    .unwrap_or(DEFAULT_READ_TYPE)
                    .to_string(),
                exclude: item
                    .get("exclude")
                    .and_then(Value::as_array)
                    .map(|list| {
                        list.iter()
                            .filter_map(|e| e.as_str().map(str::to_string))
                            .collect()
                    })
                    .unwrap_or_default(),
                parsers: item
                    .get("parsers")
                    .and_then(|p| parsers_of(p).ok())
                    .unwrap_or_default(),
            })
        })
        .collect();
    Ok(read.unwrap_or_default())
}

pub(crate) fn set_directories(
    conn: &Connection,
    project: &str,
    dirs: &[Directory],
) -> Result<(), String> {
    let text = (!dirs.is_empty()).then(|| {
        Value::Array(
            dirs.iter()
                .map(|d| {
                    let mut row = Map::new();
                    row.insert("id".into(), d.id.clone().into());
                    row.insert("name".into(), d.name.clone().into());
                    if !d.description.is_empty() {
                        row.insert("description".into(), d.description.clone().into());
                    }
                    // What a directory has by default is not written.
                    if d.read_type != DEFAULT_READ_TYPE {
                        row.insert("readType".into(), d.read_type.clone().into());
                    }
                    if !d.exclude.is_empty() {
                        row.insert("exclude".into(), d.exclude.clone().into());
                    }
                    if !d.parsers.is_empty() {
                        row.insert(
                            "parsers".into(),
                            Value::Object(
                                d.parsers
                                    .iter()
                                    .map(|(k, v)| {
                                        (k.clone(), v.clone().map_or(Value::Null, Value::from))
                                    })
                                    .collect(),
                            ),
                        );
                    }
                    Value::Object(row)
                })
                .collect(),
        )
        .to_string()
    });
    conn.execute(
        "UPDATE projects SET export_dirs = ? WHERE id = ?",
        params![text, project],
    )
    .map_err(db)?;
    Ok(())
}

/// The directory a name names, in any case; or what there is.
pub(crate) fn named<'a>(dirs: &'a [Directory], name: &str) -> Result<&'a Directory, String> {
    dirs.iter()
        .find(|d| crate::pathtext::same(&d.name, name.trim()))
        .ok_or_else(|| {
            let there: Vec<&str> = dirs.iter().map(|d| d.name.as_str()).collect();
            if there.is_empty() {
                text!("directories.named.none", name)
            } else {
                text!(
                    "directories.named.missing",
                    name,
                    there = crate::text::List::of(&there)
                )
            }
        })
}

/// The folder a working directory is on this machine set, or let go (none).
pub(crate) fn bind(
    conn: &Connection,
    project: &str,
    dir: &str,
    folder: Option<&str>,
) -> Result<(), String> {
    conn.execute(
        "DELETE FROM export_dir_folders WHERE project_id = ? AND dir_id = ?",
        params![project, dir],
    )
    .map_err(db)?;
    if let Some(folder) = folder {
        conn.execute(
            "INSERT INTO export_dir_folders (project_id, dir_id, folder) VALUES (?, ?, ?)",
            params![project, dir, folder],
        )
        .map_err(db)?;
    }
    Ok(())
}

/// The folder each working directory is on this machine, by its id.
pub(crate) fn folders(conn: &Connection, project: &str) -> Result<HashMap<String, String>, String> {
    let mut stmt = conn
        .prepare("SELECT dir_id, folder FROM export_dir_folders WHERE project_id = ?")
        .map_err(db)?;
    stmt.query_map([project], |r| Ok((r.get(0)?, r.get(1)?)))
        .map_err(db)?
        .collect::<Result<_, _>>()
        .map_err(db)
}

/// The working directories `project_tree` lists (its `workdir` entries under main's root), each
/// linked to the one it is by id: the id it says, else the one listed by the same name or renamed
/// into it (`renames`, `from` → `to`) before, else one of the project's by that name, else one made
/// now. The entries are given their id (`directory`) in `own` as it will be kept. The directories
/// the project will have, in the order listed, each named as its entry is — or none when `own`
/// does not write `project_tree`. What is wrong is said in `problems`.
pub(crate) fn linked(
    conn: &Connection,
    project: &str,
    before: &crate::kinds::ProjectTree,
    own: &mut Map<String, Value>,
    renames: &[(String, String)],
    problems: &mut Vec<String>,
) -> Result<Option<Vec<Directory>>, String> {
    let Some(items) = own
        .get_mut("project_tree")
        .and_then(|t| t.get_mut("main_sections"))
        .and_then(Value::as_array_mut)
    else {
        return Ok(None);
    };
    let dirs = directories(conn, project)?;
    let listed_before = |name: &str| -> Option<String> {
        before
            .main
            .iter()
            .filter(|t| t.kind == crate::kinds::WORKDIR)
            .find(|t| crate::pathtext::same(&t.name, name))
            .and_then(|t| t.directory.clone())
    };
    let mut out: Vec<Directory> = Vec::new();
    for item in items.iter_mut() {
        let Some(entry) = item.as_object_mut() else {
            continue;
        };
        if entry.get("type").and_then(Value::as_str) != Some(crate::kinds::WORKDIR) {
            continue;
        }
        let name = entry
            .get("name")
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or_default()
            .to_string();
        if name.is_empty() {
            continue;
        }
        let said = entry
            .get("directory")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .map(str::to_string);
        let id = match said {
            Some(id) => {
                let known = dirs.iter().any(|d| d.id == id)
                    || before
                        .main
                        .iter()
                        .any(|t| t.directory.as_deref() == Some(&id));
                if !known {
                    problems.push(text!("directories.tree.unknown", name, id));
                    continue;
                }
                id
            }
            None => listed_before(&name)
                .or_else(|| {
                    renames
                        .iter()
                        .filter(|(_, to)| crate::pathtext::same(to, &name))
                        .find_map(|(from, _)| listed_before(from))
                })
                .or_else(|| {
                    dirs.iter()
                        .find(|d| {
                            crate::pathtext::same(&d.name, &name)
                                && !out.iter().any(|o| o.id == d.id)
                                && !before.main.iter().any(|t| {
                                    t.directory.as_deref() == Some(&d.id)
                                        && !crate::pathtext::same(&t.name, &name)
                                })
                        })
                        .map(|d| d.id.clone())
                })
                .unwrap_or_else(new_id),
        };
        if out.iter().any(|o| o.id == id) {
            problems.push(text!("directories.tree.twice", name));
            continue;
        }
        entry.insert("directory".into(), id.clone().into());
        let mut dir = dirs
            .iter()
            .find(|d| d.id == id)
            .cloned()
            .unwrap_or(Directory {
                id,
                name: String::new(),
                description: String::new(),
                read_type: DEFAULT_READ_TYPE.to_string(),
                exclude: Vec::new(),
                parsers: Default::default(),
            });
        dir.name = name;
        out.push(dir);
    }
    Ok(Some(out))
}

/// The project's working directories made what `project_tree` lists (`linked`): one no longer
/// listed let go of with its folder on this machine (the folder itself is not touched) and what
/// was read of it, one made read.
pub(crate) fn settle(conn: &Connection, project: &str, after: &[Directory]) -> Result<(), String> {
    let before = directories(conn, project)?;
    set_directories(conn, project, after)?;
    for gone in before
        .iter()
        .filter(|b| !after.iter().any(|a| a.id == b.id))
    {
        bind(conn, project, &gone.id, None)?;
    }
    crate::workdir::read(conn, project, Some(&[]))?;
    let made: Vec<String> = after
        .iter()
        .filter(|a| !before.iter().any(|b| b.id == a.id))
        .map(|a| a.id.clone())
        .collect();
    if !made.is_empty() {
        crate::workdir::read(conn, project, Some(&made))?;
    }
    Ok(())
}
