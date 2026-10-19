//! The working directories read: each a
//! project's directory bound to a folder on this machine (`export_dir_folders`), read as it says
//! (`readType`) and what it leaves out besides (`tools::directories::Directory`). What was read is kept
//! in tables of the connection's own (`temp.work_files`, `temp.work_reads`), gone when the core stops:
//! read the first time it is asked for, and again when asked to (`section_action`'s `read`, the
//! window's 「読み込む」), or when how or where it is read changes (`section_update`). Each working
//! directory is a section directly under main's root and every branch's (`dir:<id>`, of the kind
//! `workdir`); what was read is put in it (`Overlay`): a directory a section of the kind `directory`,
//! a file a card of the kind `file`. Nothing of that is the project's: it is not written, merged,
//! nor kept in the project's file, and its ids are made of the directory's id and the path, the same
//! on every machine and for a file not there yet.

use crate::store::{Composition, Item};
use crate::text::Keyed;
use crate::tools::directories::Directory;
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Map, Value, json};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// The most a working directory is read to: past it, reading stops and says so.
pub const LIMIT: usize = 20_000;

/// The tree's root.
pub const ROOT_ID: &str = "dir:";
pub const ROOT_NAME: &str = "Directory";

/// A file's id: its working directory's and its path in it.
pub fn file_id(dir: &str, path: &str) -> String {
    format!("file:{dir}/{path}")
}

/// A directory's id: its working directory's and its path in it (none for the working directory).
pub fn dir_id(dir: &str, path: &str) -> String {
    if path.is_empty() {
        format!("dir:{dir}")
    } else {
        format!("dir:{dir}/{path}")
    }
}

/// The working directory and the path a file's id is made of.
pub fn of_file_id(id: &str) -> Option<(&str, &str)> {
    id.strip_prefix("file:")?.split_once('/')
}

fn sql(e: rusqlite::Error) -> String {
    text!("workdir.unreadable", e)
}

/// How reading came out, as the tables keep it: by key, so the connections that share it say it in
/// the language of each call that reads it.
fn kept(said: Option<Keyed>) -> Option<String> {
    said.map(|k| k.json().to_string())
}

/// What `kept` kept, said in the language of the call.
fn said(kept: Option<String>) -> Option<String> {
    kept.map(|raw| {
        serde_json::from_str::<Value>(&raw)
            .ok()
            .and_then(|v| Keyed::from_json(&v))
            .map(|k| k.render())
            .unwrap_or(raw)
    })
}

/// The tables of the connection's own what is read is kept in, and which of what the core's
/// connections share it holds of each project (`work_seen`: the turn it was given out at, and
/// whether this one read it again since).
fn ensure(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "CREATE TEMP TABLE IF NOT EXISTS work_files (
           project_id TEXT NOT NULL, dir_id TEXT NOT NULL, path TEXT NOT NULL, is_dir INTEGER NOT NULL,
           PRIMARY KEY (project_id, dir_id, path));
         CREATE TEMP TABLE IF NOT EXISTS work_reads (
           project_id TEXT NOT NULL, dir_id TEXT NOT NULL, count INTEGER NOT NULL,
           dirs INTEGER NOT NULL DEFAULT 0, problem TEXT, note TEXT,
           PRIMARY KEY (project_id, dir_id));
         CREATE TEMP TABLE IF NOT EXISTS work_seen (
           project_id TEXT PRIMARY KEY, turn INTEGER NOT NULL, dirty INTEGER NOT NULL);",
    )
    .map_err(sql)
}

/// What the core's connections have read of the working directories, shared among them:
/// reads go apart from the writes, on
/// connections of their own, and each keeps what was read in tables of its own. What one reads
/// anew — the first time, or read again when asked to or when how a directory is read changes —
/// it gives out once it is done (`give_out`; a write's once it is committed, so what a write
/// undoes is not), and every connection takes in what was given out before it is used next
/// (`take_in`): they all see the same reading, as the one connection did before.
#[derive(Default)]
pub struct Shared {
    held: std::sync::Mutex<Given>,
}

/// The turns readings are given out at, one count for every core of the process: a turn names one
/// reading of one core's, so what is made of it and kept by its turn (`overlay`, on a thread every core
/// may read on) is never taken for another core's reading of the same project.
static TURNS: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(0);

fn next_turn() -> i64 {
    TURNS.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1
}

#[derive(Default)]
struct Given {
    /// The turn of the reading given out last (`next_turn`): later each time one is given out.
    turn: i64,
    projects: HashMap<String, std::sync::Arc<Reading>>,
}

/// One project's working directories as read: their files and how reading each came out (as
/// `kept` keeps it).
struct Reading {
    turn: i64,
    files: Vec<(String, String, bool)>,
    reads: Vec<DirRead>,
}

/// One working directory's reading as given out: its id, its files, its folders, why it holds none,
/// what was read otherwise.
type DirRead = (String, i64, i64, Option<String>, Option<String>);

impl Shared {
    fn given(&self) -> std::sync::MutexGuard<'_, Given> {
        self.held.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// What was given out since the connection last took in, put in its tables in place of what it
/// held of those projects. Before a transaction (it is one of its own).
pub(crate) fn take_in(store: &crate::store::Store, shared: &Shared) -> Result<(), String> {
    let (turn, projects) = {
        let given = shared.given();
        if store.work_seen.get() == given.turn {
            return Ok(());
        }
        let projects: Vec<(String, std::sync::Arc<Reading>)> = given
            .projects
            .iter()
            .map(|(p, r)| (p.clone(), r.clone()))
            .collect();
        (given.turn, projects)
    };
    let conn = &store.conn;
    ensure(conn)?;
    conn.execute_batch("BEGIN").map_err(sql)?;
    let taken = (|| -> Result<(), String> {
        for (project, reading) in &projects {
            let had: Option<i64> = conn
                .query_row(
                    "SELECT turn FROM temp.work_seen WHERE project_id = ?",
                    [project],
                    |r| r.get(0),
                )
                .optional()
                .map_err(sql)?;
            if had == Some(reading.turn) {
                continue;
            }
            for table in ["work_files", "work_reads"] {
                conn.execute(
                    &format!("DELETE FROM temp.{table} WHERE project_id = ?"),
                    [project],
                )
                .map_err(sql)?;
            }
            let mut files = conn
                .prepare(
                    "INSERT INTO temp.work_files (project_id, dir_id, path, is_dir) VALUES (?, ?, ?, ?)",
                )
                .map_err(sql)?;
            for (dir, path, is_dir) in &reading.files {
                files
                    .execute(params![project, dir, path, *is_dir as i64])
                    .map_err(sql)?;
            }
            let mut reads = conn
                .prepare(
                    "INSERT INTO temp.work_reads (project_id, dir_id, count, dirs, problem, note) VALUES (?, ?, ?, ?, ?, ?)",
                )
                .map_err(sql)?;
            for (dir, count, dirs, problem, note) in &reading.reads {
                reads
                    .execute(params![project, dir, count, dirs, problem, note])
                    .map_err(sql)?;
            }
            conn.execute(
                "INSERT INTO temp.work_seen (project_id, turn, dirty) VALUES (?, ?, 0)
                 ON CONFLICT(project_id) DO UPDATE SET turn = excluded.turn, dirty = 0",
                params![project, reading.turn],
            )
            .map_err(sql)?;
        }
        Ok(())
    })();
    match taken {
        Ok(()) => {
            conn.execute_batch("COMMIT").map_err(sql)?;
            store.work_seen.set(turn);
            Ok(())
        }
        Err(e) => {
            let _ = conn.execute_batch("ROLLBACK");
            Err(e)
        }
    }
}

/// What the connection read anew since it took in, given out to the others. A connection that
/// reads apart (`fresher: false`) leaves a project another gave out meanwhile to that one (it is
/// as new); a write's reading (`fresher: true`) is given out whatever was before it. After the
/// transaction it was read in.
pub(crate) fn give_out(
    store: &crate::store::Store,
    shared: &Shared,
    fresher: bool,
) -> Result<(), String> {
    let conn = &store.conn;
    let dirty: Vec<(String, i64)> =
        match conn.prepare("SELECT project_id, turn FROM temp.work_seen WHERE dirty = 1") {
            // Nothing was ever read on it.
            Err(_) => return Ok(()),
            Ok(mut stmt) => stmt
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
                .map_err(sql)?
                .collect::<Result<_, _>>()
                .map_err(sql)?,
        };
    if dirty.is_empty() {
        return Ok(());
    }
    let mut given = shared.given();
    let whole = store.work_seen.get() == given.turn;
    for (project, had) in dirty {
        let theirs = given.projects.get(&project).map(|r| r.turn);
        if fresher || theirs.is_none() || theirs == Some(had) {
            let files: Vec<(String, String, bool)> = conn
                .prepare(
                    "SELECT dir_id, path, is_dir FROM temp.work_files WHERE project_id = ? ORDER BY dir_id, path",
                )
                .map_err(sql)?
                .query_map([&project], |r| {
                    Ok((r.get(0)?, r.get(1)?, r.get::<_, i64>(2)? != 0))
                })
                .map_err(sql)?
                .collect::<Result<_, _>>()
                .map_err(sql)?;
            let reads: Vec<DirRead> = conn
                .prepare(
                    "SELECT dir_id, count, dirs, problem, note FROM temp.work_reads WHERE project_id = ? ORDER BY dir_id",
                )
                .map_err(sql)?
                .query_map([&project], |r| {
                    Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
                })
                .map_err(sql)?
                .collect::<Result<_, _>>()
                .map_err(sql)?;
            given.turn = next_turn();
            let turn = given.turn;
            given.projects.insert(
                project.clone(),
                std::sync::Arc::new(Reading { turn, files, reads }),
            );
            conn.execute(
                "UPDATE temp.work_seen SET turn = ?, dirty = 0 WHERE project_id = ?",
                params![turn, project],
            )
            .map_err(sql)?;
        } else {
            // Another gave it out since: that one is taken in next time.
            conn.execute(
                "UPDATE temp.work_seen SET dirty = 0 WHERE project_id = ?",
                [&project],
            )
            .map_err(sql)?;
        }
    }
    if whole {
        store.work_seen.set(given.turn);
    }
    Ok(())
}

/// What reading one working directory came to: how many files and folders it holds (`.git` not
/// counted), or why it holds none — and what was read otherwise than it says (`note`: a folder read
/// as git that is no git repository).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Read {
    pub dir: String,
    pub name: String,
    pub files: usize,
    pub dirs: usize,
    pub problem: Option<String>,
    pub note: Option<String>,
}

/// The folder each working directory is bound to on this machine, by its id.
fn folders(conn: &Connection, project: &str) -> Result<HashMap<String, String>, String> {
    let mut stmt = conn
        .prepare("SELECT dir_id, folder FROM export_dir_folders WHERE project_id = ?")
        .map_err(sql)?;
    stmt.query_map([project], |r| Ok((r.get(0)?, r.get(1)?)))
        .map_err(sql)?
        .collect::<Result<_, _>>()
        .map_err(sql)
}

/// The project's working directories read again — those named (`only`, by id), or all — and what
/// was read of one the project no longer has let go.
pub fn read(
    conn: &Connection,
    project: &str,
    only: Option<&[String]>,
) -> Result<Vec<Read>, String> {
    ensure(conn)?;
    let dirs = crate::tools::directories::directories(conn, project)?;
    let folders = folders(conn, project)?;
    let known: HashSet<&str> = dirs.iter().map(|d| d.id.as_str()).collect();
    for table in ["work_files", "work_reads"] {
        let mut stmt = conn
            .prepare(&format!(
                "SELECT DISTINCT dir_id FROM temp.{table} WHERE project_id = ?"
            ))
            .map_err(sql)?;
        let held: Vec<String> = stmt
            .query_map([project], |r| r.get(0))
            .map_err(sql)?
            .collect::<Result<_, _>>()
            .map_err(sql)?;
        for gone in held.iter().filter(|d| !known.contains(d.as_str())) {
            conn.execute(
                &format!("DELETE FROM temp.{table} WHERE project_id = ? AND dir_id = ?"),
                params![project, gone],
            )
            .map_err(sql)?;
            mark_read(conn, project)?;
        }
    }
    let mut out = Vec::new();
    for dir in dirs
        .iter()
        .filter(|d| only.is_none_or(|ids| ids.contains(&d.id)))
    {
        let (entries, problem, note) = match folders.get(&dir.id) {
            // Not read, as it says: its root alone, and the lines to it not checked.
            _ if dir.read_type == "none" => (Vec::new(), Some(keyed!("workdir.read.none")), None),
            None => (Vec::new(), Some(keyed!("workdir.read.unbound")), None),
            Some(folder) => walk(&crate::tools::files::absolute(folder, ""), dir),
        };
        conn.execute(
            "DELETE FROM temp.work_files WHERE project_id = ? AND dir_id = ?",
            params![project, dir.id],
        )
        .map_err(sql)?;
        {
            let mut stmt = conn
                .prepare(
                    "INSERT INTO temp.work_files (project_id, dir_id, path, is_dir) VALUES (?, ?, ?, ?)",
                )
                .map_err(sql)?;
            for (path, is_dir) in &entries {
                stmt.execute(params![project, dir.id, path, *is_dir as i64])
                    .map_err(sql)?;
            }
        }
        let dirs = entries.iter().filter(|(_, is_dir)| *is_dir).count();
        let files = entries.len() - dirs;
        conn.execute(
            "INSERT OR REPLACE INTO temp.work_reads (project_id, dir_id, count, dirs, problem, note) VALUES (?, ?, ?, ?, ?, ?)",
            params![
                project,
                dir.id,
                files as i64,
                dirs as i64,
                kept(problem.clone()),
                kept(note.clone())
            ],
        )
        .map_err(sql)?;
        mark_read(conn, project)?;
        out.push(Read {
            dir: dir.id.clone(),
            name: dir.name.clone(),
            files,
            dirs,
            problem: problem.map(|k| k.render()),
            note: note.map(|k| k.render()),
        });
    }
    Ok(out)
}

/// A project's reading changed on this connection: to be given out (`give_out`).
fn mark_read(conn: &Connection, project: &str) -> Result<(), String> {
    conn.execute(
        "INSERT INTO temp.work_seen (project_id, turn, dirty) VALUES (?, 0, 1)
         ON CONFLICT(project_id) DO UPDATE SET dirty = 1",
        [project],
    )
    .map(|_| ())
    .map_err(sql)
}

/// What reading each working directory came to, as last read; none read yet, all read now.
pub fn reads(conn: &Connection, project: &str) -> Result<Vec<Read>, String> {
    ensure(conn)?;
    if !was_read(conn, project)? {
        return read(conn, project, None);
    }
    let dirs = crate::tools::directories::directories(conn, project)?;
    // One made since: read now.
    let mut unread = Vec::new();
    for dir in &dirs {
        let read: bool = conn
            .query_row(
                "SELECT EXISTS (SELECT 1 FROM temp.work_reads WHERE project_id = ? AND dir_id = ?)",
                params![project, dir.id],
                |r| r.get(0),
            )
            .map_err(sql)?;
        if !read {
            unread.push(dir.id.clone());
        }
    }
    if !unread.is_empty() {
        read(conn, project, Some(&unread))?;
    }
    let mut out = Vec::new();
    for dir in dirs {
        let row: Option<(i64, i64, Option<String>, Option<String>)> = conn
            .query_row(
                "SELECT count, dirs, problem, note FROM temp.work_reads WHERE project_id = ? AND dir_id = ?",
                params![project, dir.id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .optional()
            .map_err(sql)?;
        let (files, dirs, problem, note) = match row {
            Some((files, dirs, problem, note)) => (files, dirs, said(problem), said(note)),
            None => (0, 0, Some(text!("workdir.read.notYet")), None),
        };
        out.push(Read {
            dir: dir.id,
            name: dir.name,
            files: files as usize,
            dirs: dirs as usize,
            problem,
            note,
        });
    }
    Ok(out)
}

fn was_read(conn: &Connection, project: &str) -> Result<bool, String> {
    conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM temp.work_reads WHERE project_id = ?)",
        [project],
        |r| r.get(0),
    )
    .map_err(sql)
}

/// What a working directory leaves out besides how it is read (`exclude`): a directory by its name
/// (`target/`) or its path from the top (`sources/core/target/`), a file by its name or path, an
/// extension (`.png`).
struct Excludes {
    dir_names: Vec<String>,
    dir_paths: Vec<String>,
    file_names: Vec<String>,
    file_paths: Vec<String>,
    endings: Vec<String>,
}

impl Excludes {
    fn new(rules: &[String]) -> Excludes {
        let mut out = Excludes {
            dir_names: Vec::new(),
            dir_paths: Vec::new(),
            file_names: Vec::new(),
            file_paths: Vec::new(),
            endings: Vec::new(),
        };
        for rule in rules {
            let rule = rule.trim();
            if let Some(dir) = rule.strip_suffix('/') {
                if dir.contains('/') {
                    out.dir_paths.push(dir.to_string());
                } else {
                    out.dir_names.push(dir.to_string());
                }
            } else if rule.contains('/') {
                out.file_paths.push(rule.to_string());
            } else if rule.starts_with('.') && rule.len() > 1 {
                // `.png`, and a name that starts so (`.DS_Store`): what a file's name ends with.
                out.endings.push(rule.to_string());
            } else {
                out.file_names.push(rule.to_string());
            }
        }
        out
    }

    /// Whether `path` (from the top) is left out: a directory, or a file.
    fn leaves_out(&self, path: &str, name: &str, is_dir: bool) -> bool {
        if is_dir {
            self.dir_names.iter().any(|n| n == name) || self.dir_paths.iter().any(|p| p == path)
        } else {
            self.file_names.iter().any(|n| n == name)
                || self.file_paths.iter().any(|p| p == path)
                || self.endings.iter().any(|e| name.ends_with(e.as_str()))
        }
    }
}

/// Whether a folder is in a git repository: it, or a folder it is in, has a `.git` (a folder, or a
/// worktree's file).
fn in_git(top: &Path) -> bool {
    top.ancestors().any(|a| a.join(".git").exists())
}

/// A folder read as the directory says: its directories and files, by their paths from the top
/// (`/` between), in order — or why none were read; and what was read otherwise than it says. A
/// folder read as git that is in no git repository has its own `.gitignore`s read still — not those
/// of the folders it is in — and says so.
fn walk(root: &str, dir: &Directory) -> (Vec<(String, bool)>, Option<Keyed>, Option<Keyed>) {
    let top = Path::new(root);
    if !top.is_dir() {
        return (
            Vec::new(),
            Some(keyed!("workdir.read.noFolder", root)),
            None,
        );
    }
    let git = dir.read_type == "git";
    let repo = git && in_git(top);
    let note = (git && !repo).then(|| keyed!("workdir.read.notGit", name = dir.name, root));
    let excludes = Excludes::new(&dir.exclude);
    let base = top.to_path_buf();
    let mut builder = ignore::WalkBuilder::new(top);
    builder
        .hidden(false)
        .ignore(false)
        .parents(repo)
        .git_ignore(git)
        .git_exclude(repo)
        .git_global(false)
        .require_git(repo)
        .follow_links(false)
        .sort_by_file_name(|a, b| a.cmp(b))
        .filter_entry(move |entry| {
            let name = entry.file_name().to_string_lossy();
            let is_dir = entry.file_type().is_some_and(|t| t.is_dir());
            // What every way of reading leaves out: git's own, and the Finder's.
            if (is_dir && name == ".git") || (!is_dir && name == ".DS_Store") {
                return false;
            }
            let Ok(rel) = entry.path().strip_prefix(&base) else {
                return true;
            };
            let rel = rel.to_string_lossy().replace('\\', "/");
            rel.is_empty() || !excludes.leaves_out(&rel, &name, is_dir)
        });
    let mut out = Vec::new();
    for entry in builder.build().flatten() {
        if entry.depth() == 0 {
            continue;
        }
        let Ok(rel) = entry.path().strip_prefix(top) else {
            continue;
        };
        let is_dir = entry.file_type().is_some_and(|t| t.is_dir());
        out.push((rel.to_string_lossy().replace('\\', "/"), is_dir));
        if out.len() > LIMIT {
            return (
                Vec::new(),
                Some(keyed!("workdir.read.overLimit", limit = LIMIT)),
                note,
            );
        }
    }
    if git {
        // Git has no directory of its own, only the files in it: a folder whose files are all
        // left out is not there.
        let held: HashSet<&str> = out
            .iter()
            .filter(|(_, is_dir)| !is_dir)
            .flat_map(|(path, _)| path.match_indices('/').map(|(at, _)| &path[..at]))
            .collect();
        let held: HashSet<String> = held.into_iter().map(str::to_string).collect();
        out.retain(|(path, is_dir)| !is_dir || held.contains(path.as_str()));
    }
    (out, None, note)
}

/// The most of a file read to show it (`text`).
pub const TEXT_LIMIT: usize = 256 * 1024;

/// Where a working directory's file really is, by its path in the folder bound (`/` between, each
/// part a name): with every link on the way followed, refused when that is outside the folder — a
/// link in the folder may point anywhere on the machine, and what a project's reader reaches is the
/// folder alone. Every read of a working directory's file goes through here, as `bodyFrom` does
/// (`tools::body_from::read`). A file not there is given where it would be, for the read to say so.
pub(crate) fn real_file(folder: &str, path: &str) -> Result<PathBuf, Keyed> {
    if path
        .split('/')
        .any(|p| p.is_empty() || p == "." || p == "..")
    {
        return Err(keyed!("workdir.text.outside", path));
    }
    let file = PathBuf::from(crate::tools::files::absolute(folder, path));
    let Ok(top) = PathBuf::from(crate::tools::files::absolute(folder, "")).canonicalize() else {
        return Ok(file);
    };
    match file.canonicalize() {
        Ok(real) if real.starts_with(&top) => Ok(real),
        Ok(_) => Err(keyed!("workdir.read.linkOutside", path)),
        // A link that leads nowhere may lead outside once its target is made.
        Err(_) if std::fs::symlink_metadata(&file).is_ok_and(|m| m.file_type().is_symlink()) => {
            Err(keyed!("workdir.read.linkOutside", path))
        }
        Err(_) => Ok(file),
    }
}

/// A working directory's file as it is now, to be shown: its text, the start of it when it is long; or why it cannot be
/// shown — its working directory not bound here, the file gone, not text. Read only.
pub fn text(conn: &Connection, project: &str, id: &str) -> Result<Value, String> {
    let (dir, path) = of_file_id(id).ok_or_else(|| text!("workdir.text.notAFile", id))?;
    if path
        .split('/')
        .any(|p| p.is_empty() || p == "." || p == "..")
    {
        return Err(text!("workdir.text.outside", path));
    }
    let folders = folders(conn, project)?;
    let Some(folder) = folders.get(dir) else {
        return Ok(json!({"problem": text!("workdir.read.unbound")}));
    };
    let full = match real_file(folder, path) {
        Ok(full) => full,
        Err(why) => return Ok(json!({"problem": why.render()})),
    };
    let bytes = match std::fs::read(&full) {
        Ok(bytes) => bytes,
        Err(_) => {
            return Ok(json!({"problem": text!("workdir.text.unreadable", path)}));
        }
    };
    let size = bytes.len();
    let cut = size > TEXT_LIMIT;
    let mut head = &bytes[..size.min(TEXT_LIMIT)];
    // Cut where a character ends.
    while !head.is_empty() && std::str::from_utf8(head).is_err() && cut {
        head = &head[..head.len() - 1];
    }
    let Ok(text) = std::str::from_utf8(head) else {
        return Ok(json!({"problem": text!("workdir.text.notText"), "size": size}));
    };
    if text.contains('\0') {
        return Ok(json!({"problem": text!("workdir.text.notText"), "size": size}));
    }
    Ok(json!({"text": text, "size": size, "cut": cut}))
}

/// A file's or a directory's name as the name of its card or its section: as it is (a path quotes
/// what needs it), but for what a name may not hold — a control character, put as `_`; spaces
/// around it, dropped — and cut to the most a name has. The file's own name stays its label.
fn name_of(raw: &str) -> String {
    let cleaned: String = raw
        .chars()
        .map(|c| if c.is_control() { '_' } else { c })
        .collect();
    let cut: String = cleaned
        .trim()
        .chars()
        .take(crate::pathtext::NAME_MAX)
        .collect();
    let cut = cut.trim_end();
    if cut.is_empty() {
        "_".to_string()
    } else {
        cut.to_string()
    }
}

/// `name`, or with ` (2)`, ` (3)`… after it where another beside it has it in any ASCII case (a
/// repository on a file system that tells `Read.md` from `README.md` by case alone).
fn unique(name: String, taken: &mut HashSet<String>) -> String {
    let mut out = name.clone();
    let mut n = 2;
    while !taken.insert(crate::pathtext::fold(&out)) {
        let tail = format!(" ({n})");
        let room = crate::pathtext::NAME_MAX - tail.chars().count();
        let head: String = name.chars().take(room).collect();
        out = format!("{}{tail}", head.trim_end());
        n += 1;
    }
    out
}

thread_local! {
    /// How the call running on this thread reaches into the working directories' sections
    /// (`Whole`): what it reads or writes is in one (its trees are read with every directory and
    /// file in them), or what it writes points into one (its trees are read with them beside).
    static REACH: std::cell::Cell<Reach> = const { std::cell::Cell::new(Reach::None) };
}

/// How a call reaches into the working directories' sections (`touches`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Reach {
    None,
    /// A line it writes, or a path in a text, points at a file.
    Points,
    /// It reads or writes in one.
    In,
}

/// Whether the trees read on this thread now are read with the working directories' sections
/// whole (`Whole::during`).
pub(crate) fn whole() -> bool {
    REACH.with(std::cell::Cell::get) == Reach::In
}

/// Whether the trees read on this thread now are read with the working directories beside them,
/// whole or not.
pub(crate) fn files_wanted() -> bool {
    REACH.with(std::cell::Cell::get) != Reach::None
}

/// How the trees read on this thread reach into the working directories' sections while this is
/// held: a call that names a path in one, as a call runs on its thread from start to end.
pub(crate) struct Whole(Reach);

impl Whole {
    pub(crate) fn during(reach: Reach) -> Whole {
        Whole(REACH.with(|w| w.replace(reach)))
    }

    /// The reach of the call now, at least `reach`.
    pub(crate) fn at_least(reach: Reach) -> Whole {
        Whole(REACH.with(|w| w.replace(w.get().max(reach))))
    }
}

impl Drop for Whole {
    fn drop(&mut self) {
        let before = self.0;
        REACH.with(|w| w.set(before));
    }
}

/// How a call reaches into a working directory's section of the project: it reads or writes in
/// one (`Reach::In`) — a path it is given (`target`, the paths `value` is keyed by, `value`'s
/// `section`, `in` and `roots`) that starts at one, an id of a directory or a file, or a list of
/// files (`kind: file`) —, or what it writes points into one (`Reach::Points`: a path anywhere in
/// `value` that starts at one, or written in `[[…]]`).
pub(crate) fn touches(conn: &Connection, project: &str, raw: &Map<String, Value>) -> Reach {
    let Ok(mounts) = mounts(conn, project) else {
        return Reach::None;
    };
    if mounts.is_empty() {
        return Reach::None;
    }
    let heads: Vec<String> = mounts
        .iter()
        .map(|m| crate::pathtext::fold(&m.name))
        .collect();
    let mut said: Vec<&str> = Vec::new();
    match raw.get("target") {
        Some(Value::String(s)) => said.push(s),
        Some(Value::Array(items)) => said.extend(items.iter().filter_map(Value::as_str)),
        _ => {}
    }
    if let Some(Value::Object(value)) = raw.get("value") {
        if !raw.contains_key("target") {
            said.extend(value.keys().map(String::as_str));
        }
        for key in ["section", "in"] {
            if let Some(Value::String(s)) = value.get(key) {
                said.push(s);
            }
        }
        if let Some(Value::Array(roots)) = value.get("roots") {
            said.extend(roots.iter().filter_map(Value::as_str));
        }
        if value.get("kind").and_then(Value::as_str) == Some("file") {
            return Reach::In;
        }
    }
    let starts_at = |s: &str| {
        let s = s.trim();
        s.starts_with("file:")
            || s.starts_with("dir:")
            || heads.contains(&crate::pathtext::fold(&crate::pathtext::first_name(s)))
    };
    if said.iter().any(|s| starts_at(s)) {
        return Reach::In;
    }
    fn points(value: &Value, starts_at: &dyn Fn(&str) -> bool) -> bool {
        match value {
            Value::String(s) => {
                starts_at(s)
                    || s.match_indices("[[").any(|(at, _)| {
                        let rest = &s[at + 2..];
                        starts_at(&rest[..rest.find("]]").unwrap_or(rest.len())])
                    })
            }
            Value::Array(items) => items.iter().any(|v| points(v, starts_at)),
            Value::Object(o) => o.values().any(|v| points(v, starts_at)),
            _ => false,
        }
    }
    if raw.get("value").is_some_and(|v| points(v, &starts_at)) {
        return Reach::Points;
    }
    Reach::None
}

/// A working directory as main's and every branch's root holds it: its section (`dir:<id>`), the
/// name the project tree gives it, the description its settings give it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Mount {
    /// The working directory's id.
    pub dir: String,
    /// Its section's id (`dir_id(dir, "")`).
    pub section: String,
    pub name: String,
    pub description: String,
}

/// The project's working directories as sections, in the project tree's order: those it lists
/// (`workdir` entries) that are the project's.
pub(crate) fn mounts(conn: &Connection, project: &str) -> Result<Vec<Mount>, String> {
    let dirs = crate::tools::directories::directories(conn, project)?;
    if dirs.is_empty() {
        return Ok(Vec::new());
    }
    let kinds: Option<String> = conn
        .query_row("SELECT kinds FROM projects WHERE id = ?", [project], |r| {
            r.get(0)
        })
        .map_err(sql)?;
    let tree = crate::kinds::Defs::of_project(kinds.as_deref()).tree;
    Ok(tree
        .main
        .iter()
        .filter(|t| t.kind == crate::kinds::WORKDIR)
        .filter_map(|t| {
            let dir = dirs.iter().find(|d| Some(&d.id) == t.directory.as_ref())?;
            Some(Mount {
                dir: dir.id.clone(),
                section: dir_id(&dir.id, ""),
                name: t.name.clone(),
                description: dir.description.clone(),
            })
        })
        .collect())
}

/// What the working directories as read make, for one project: each directory a section (of the
/// kind `directory`, in its working directory's section or the directory above it), each file a
/// card (of the kind `file`) — named as names may be, each file's own name its label. Made once for
/// each reading (`overlay`) and put over every tree that reads it: never stored, so no history,
/// merge, branch, lock or export sees it.
pub(crate) struct Overlay {
    /// The project's id.
    pub project: String,
    pub mounts: Vec<Mount>,
    /// The working directories read on this machine with nothing in the way, by id: where a file
    /// or a folder not found is not there.
    pub read: HashSet<String>,
    pub sections: Vec<Item>,
    pub cards: Vec<Item>,
    /// Where each section is in `sections`, by id.
    section_at: HashMap<String, usize>,
    /// Where each working directory's files are on this machine and how they map to languages:
    /// what reading one as code takes (`code::files`).
    pub code: std::rc::Rc<crate::code::files::Dirs>,
    /// The working directories' sections with what is in them, typed and looked up as a tree of
    /// its own (its root `ROOT_ID`): what a tree's lines to files and its paths into them are read
    /// through (`export::Project::files`). Made the first time it is asked for.
    typed: std::cell::OnceCell<Typed>,
}

/// The working directories with what is in them as a tree of their own: as `project_detail` would
/// say it, typed, and looked up.
pub(crate) struct Typed {
    pub raw: Value,
    pub detail: crate::model::Detail,
    pub index: std::rc::Rc<crate::export::Index>,
}

impl Overlay {
    pub fn section(&self, id: &str) -> Option<&Item> {
        self.section_at.get(id).map(|&i| &self.sections[i])
    }

    /// The working directories with what is in them, as a tree of their own (`typed`): what
    /// it is looked up by.
    pub fn typed(&self) -> (&crate::model::Detail, &std::rc::Rc<crate::export::Index>) {
        let made = self.made();
        (&made.detail, &made.index)
    }

    /// The working directories with what is in them as a tree of their own, made the first time.
    pub fn made(&self) -> &Typed {
        self.typed.get_or_init(|| {
            let mut sections = vec![Item {
                id: ROOT_ID.to_string(),
                doc: doc(json!({"id": ROOT_ID, "name": ROOT_NAME})),
            }];
            // Of the kind `workdir` where it was read here (`Project::file_dir_read`), of none
            // where it could not be.
            for (i, m) in self.mounts.iter().enumerate() {
                let mut doc = mount_doc(m, ROOT_ID, i as i64);
                if !self.read.contains(&m.dir) {
                    doc.shift_remove("kind");
                }
                sections.push(Item {
                    id: m.section.clone(),
                    doc,
                });
            }
            sections.extend(self.sections.iter().cloned());
            let made = Composition {
                sections,
                cards: self.cards.clone(),
                ..Composition::default()
            };
            let row = crate::store::ProjectRow {
                id: self.project.clone(),
                name: ROOT_NAME.to_string(),
                slug: String::new(),
                description: String::new(),
                created_at: String::new(),
                updated_at: String::new(),
                tag_groups: None,
                kinds: None,
                export_language: String::new(),
            };
            let raw = crate::detail::of_made(&row, &made);
            let detail = crate::model::typed(&raw)
                .expect("what the working directories make reads as a project");
            let index = std::rc::Rc::new(crate::export::Index::of(&detail));
            Typed { raw, detail, index }
        })
    }
}

/// A working directory's section as a tree holds it: of the kind `workdir`, its description its
/// settings'.
fn mount_doc(m: &Mount, parent: &str, order: i64) -> Map<String, Value> {
    let mut out = doc(json!({
        "id": m.section, "parent_id": parent, "name": m.name, "kind": crate::kinds::WORKDIR,
    }));
    if order != 0 {
        out.insert("sort_order".into(), order.into());
    }
    if !m.description.is_empty() {
        out.insert("description".into(), m.description.clone().into());
    }
    out
}

thread_local! {
    /// The overlays made on this thread, the last few, by what they were made of (`overlay`): every
    /// tree read in the same reading puts the same one over itself.
    static OVERLAYS: std::cell::RefCell<Vec<(String, std::rc::Rc<Overlay>)>> =
        const { std::cell::RefCell::new(Vec::new()) };
    /// How many overlays were made on this thread: what the tests hold the cost of a reading to.
    static OVERLAYS_MADE: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// The overlays made on this thread so far (`OVERLAYS_MADE`).
#[cfg(test)]
pub(crate) fn overlays_made_on_this_thread() -> u64 {
    OVERLAYS_MADE.with(std::cell::Cell::get)
}

/// What the project's working directories as read make (`Overlay`), the one made before for the
/// same reading and the same sections; none when the project has none. Read first when they never
/// were on this connection.
pub(crate) fn overlay(
    conn: &Connection,
    project: &str,
) -> Result<Option<std::rc::Rc<Overlay>>, String> {
    let mounts = mounts(conn, project)?;
    if mounts.is_empty() {
        return Ok(None);
    }
    ensure(conn)?;
    if !was_read(conn, project)? {
        read(conn, project, None)?;
    }
    // A reading given out is the same on every connection (its turn); one read here and not given
    // out yet is this connection's alone, and made each time.
    let seen: Option<(i64, i64)> = conn
        .query_row(
            "SELECT turn, dirty FROM temp.work_seen WHERE project_id = ?",
            [project],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .map_err(sql)?;
    let key = match seen {
        Some((turn, 0)) => Some(format!("{project}\u{1}{turn}\u{1}{mounts:?}")),
        _ => None,
    };
    if let Some(key) = &key
        && let Some(found) = OVERLAYS.with(|o| {
            o.borrow()
                .iter()
                .find(|(k, _)| k == key)
                .map(|(_, v)| v.clone())
        })
    {
        return Ok(Some(found));
    }
    let made = std::rc::Rc::new(made_overlay(conn, project, mounts)?);
    OVERLAYS_MADE.with(|n| n.set(n.get() + 1));
    if let Some(key) = key {
        OVERLAYS.with(|o| {
            let mut o = o.borrow_mut();
            o.retain(|(k, _)| k != &key);
            o.push((key, made.clone()));
            if o.len() > 4 {
                o.remove(0);
            }
        });
    }
    Ok(Some(made))
}

/// What the project's working directories were last read as on this connection: the turn the reading
/// was given out at (the same on every connection), or none while it is this connection's alone.
pub(crate) fn turn(conn: &Connection, project: &str) -> Result<Option<i64>, String> {
    ensure(conn)?;
    let seen: Option<(i64, i64)> = conn
        .query_row(
            "SELECT turn, dirty FROM temp.work_seen WHERE project_id = ?",
            [project],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .map_err(sql)?;
    Ok(seen.and_then(|(turn, dirty)| (dirty == 0).then_some(turn)))
}

fn made_overlay(conn: &Connection, project: &str, mounts: Vec<Mount>) -> Result<Overlay, String> {
    let code = crate::code::files::Dirs::of(conn, project)?;
    let mut sections = Vec::new();
    let mut cards = Vec::new();
    let mut read = HashSet::new();
    for mount in &mounts {
        let clean: bool = conn
            .query_row(
                "SELECT EXISTS (SELECT 1 FROM temp.work_reads WHERE project_id = ? AND dir_id = ? AND problem IS NULL)",
                params![project, mount.dir],
                |r| r.get(0),
            )
            .map_err(sql)?;
        if clean {
            read.insert(mount.dir.clone());
        }
        let mut names: HashMap<String, HashSet<String>> = HashMap::new();
        let mut order: HashMap<String, i64> = HashMap::new();
        let mut stmt = conn
            .prepare(
                "SELECT path, is_dir FROM temp.work_files WHERE project_id = ? AND dir_id = ? ORDER BY path",
            )
            .map_err(sql)?;
        let rows: Vec<(String, bool)> = stmt
            .query_map(params![project, mount.dir], |r| {
                Ok((r.get(0)?, r.get::<_, i64>(1)? != 0))
            })
            .map_err(sql)?
            .collect::<Result<_, _>>()
            .map_err(sql)?;
        for (path, is_dir) in rows {
            let (above, own) = match path.rsplit_once('/') {
                Some((above, own)) => (above, own),
                None => ("", path.as_str()),
            };
            let parent = dir_id(&mount.dir, above);
            let taken = names.entry(parent.clone()).or_default();
            let at = order.entry(parent.clone()).or_default();
            *at += 1;
            if is_dir {
                let id = dir_id(&mount.dir, &path);
                sections.push(Item {
                    id: id.clone(),
                    doc: doc(json!({
                        "id": id, "parent_id": parent, "name": unique(name_of(own), taken),
                        "sort_order": *at, "kind": crate::kinds::DIRECTORY, "description": path,
                    })),
                });
            } else {
                let id = file_id(&mount.dir, &path);
                let mut fields = json!({"label": own, "filePath": path});
                // Its language by its name, not by reading it: known before it is opened.
                if let (_, Some(language)) = code.language(&mount.dir, &path) {
                    fields["language"] = language.into();
                }
                cards.push(Item {
                    id: id.clone(),
                    doc: doc(json!({
                        "id": id, "kind": "file", "name": unique(name_of(own), taken),
                        "section": parent, "order": *at,
                        "fields": fields,
                    })),
                });
            }
        }
    }
    let section_at = sections
        .iter()
        .enumerate()
        .map(|(i, s)| (s.id.clone(), i))
        .collect();
    Ok(Overlay {
        project: project.to_string(),
        mounts,
        read,
        sections,
        cards,
        section_at,
        code: std::rc::Rc::new(code),
        typed: std::cell::OnceCell::new(),
    })
}

/// The working directory's section a section or a file of one is in, by its id: `dir:<id>` of
/// `dir:<id>/<path>` and `file:<id>/<path>`.
pub(crate) fn mount_of(id: &str) -> Option<String> {
    let rest = id
        .strip_prefix("dir:")
        .or_else(|| id.strip_prefix("file:"))?;
    let dir = rest.split('/').next().filter(|d| !d.is_empty())?;
    Some(dir_id(dir, ""))
}

/// The directory above a directory's section, by id: `dir:<id>/a` of `dir:<id>/a/b`, the working
/// directory's own section of `dir:<id>/a`; none for that one.
fn above(id: &str) -> Option<String> {
    let rest = id.strip_prefix("dir:")?;
    let (dir, path) = rest.split_once('/')?;
    Some(match path.rsplit_once('/') {
        Some((up, _)) => dir_id(dir, up),
        None => dir_id(dir, ""),
    })
}

/// A tree's working directories' sections as it holds them (`Reader::compose`): each of the kind
/// `workdir`, described as its settings say; and the sections the notes put in its directories are
/// in, with those above them — each as read (`Overlay`), or, where it was not read, put in its
/// place as missing (`missing: true`) so that the note stays where it was put. `holders` are where
/// the tree's cards are (a section's id, or a container's); what is added is said in
/// `out.generated`. Nothing is read unless a note is in a directory.
pub(crate) fn compose_part(
    conn: &Connection,
    project: &str,
    out: &mut Composition,
    holders: &HashSet<String>,
) -> Result<(), String> {
    let has_mount = out.sections.iter().any(|s| {
        s.doc.get("kind").and_then(Value::as_str) == Some(crate::kinds::WORKDIR)
            && s.id.starts_with("dir:")
    });
    if !has_mount {
        return Ok(());
    }
    let mounts = mounts(conn, project)?;
    for section in out.sections.iter_mut() {
        if let Some(m) = mounts.iter().find(|m| m.section == section.id) {
            if m.description.is_empty() {
                section.doc.shift_remove("description");
            } else {
                section
                    .doc
                    .insert("description".into(), m.description.clone().into());
            }
        }
    }
    let present: HashSet<String> = out.sections.iter().map(|s| s.id.clone()).collect();
    let wanted: Vec<&String> = holders
        .iter()
        .filter(|h| {
            h.starts_with("dir:")
                && !present.contains(*h)
                && mount_of(h).is_some_and(|m| present.contains(&m))
        })
        .collect();
    if wanted.is_empty() {
        return Ok(());
    }
    let overlay = overlay(conn, project)?;
    let mut added: HashSet<String> = HashSet::new();
    for holder in wanted {
        let mut at = Some(holder.clone());
        while let Some(id) = at {
            if present.contains(&id) || added.contains(&id) || !id.contains('/') {
                break;
            }
            let up = above(&id);
            let item = match overlay.as_ref().and_then(|o| o.section(&id)) {
                Some(found) => found.clone(),
                None => {
                    let path = id.split_once('/').map(|(_, p)| p).unwrap_or_default();
                    let own = path.rsplit('/').next().unwrap_or(path);
                    let mut made = doc(json!({
                        "id": id, "parent_id": up, "name": name_of(own),
                        "kind": crate::kinds::DIRECTORY, "description": path,
                    }));
                    // Known not there only where the working directory was read here.
                    let read = mount_of(&id)
                        .and_then(|m| m.strip_prefix("dir:").map(str::to_string))
                        .is_some_and(|d| overlay.as_ref().is_some_and(|o| o.read.contains(&d)));
                    if read {
                        made.insert("missing".into(), true.into());
                    }
                    Item {
                        id: id.clone(),
                        doc: made,
                    }
                }
            };
            out.sections.push(item);
            added.insert(id.clone());
            at = up;
        }
    }
    out.generated.extend(added);
    Ok(())
}

/// A tree's composition with the working directories' sections whole (`Overlay`) put in: every
/// directory and file read, beside what the tree holds of its own in them.
pub(crate) fn put_whole(
    conn: &Connection,
    project: &str,
    out: &mut Composition,
) -> Result<(), String> {
    let Some(overlay) = overlay(conn, project)? else {
        return Ok(());
    };
    let present: HashSet<String> = out.sections.iter().map(|s| s.id.clone()).collect();
    let mounted = |id: &str| mount_of(id).is_some_and(|m| present.contains(&m));
    for s in overlay
        .sections
        .iter()
        .filter(|s| !present.contains(&s.id) && mounted(&s.id))
    {
        out.generated.push(s.id.clone());
        out.sections.push(s.clone());
    }
    for c in overlay.cards.iter().filter(|c| mounted(&c.id)) {
        out.generated.push(c.id.clone());
        out.cards.push(c.clone());
    }
    Ok(())
}

fn doc(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(map) => map,
        _ => Map::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_s_name_is_its_card_s_name_but_what_a_name_cannot_hold() {
        for name in [
            "cross.rs",
            ".gitignore",
            "0001.sql",
            "設計.md",
            "SpecBoard.xcodeproj",
        ] {
            assert_eq!(name_of(name), name);
        }
        assert_eq!(name_of(" a\tb.md "), "a_b.md");
        assert_eq!(name_of("\n"), "_");
        assert_eq!(name_of(&"x".repeat(300)).chars().count(), 255);
        let mut taken = HashSet::new();
        assert_eq!(unique("README.md".into(), &mut taken), "README.md");
        assert_eq!(unique("ReadMe.md".into(), &mut taken), "ReadMe.md (2)");
        assert_eq!(unique("readme.MD".into(), &mut taken), "readme.MD (3)");
        assert_eq!(unique("É.md".into(), &mut taken), "É.md");
        assert_eq!(unique("é.md".into(), &mut taken), "é.md");
        assert_eq!(
            of_file_id(&file_id("d1", "src/a.rs")),
            Some(("d1", "src/a.rs"))
        );
    }

    #[test]
    fn what_is_left_out_is_a_directory_a_file_or_an_ending() {
        let rules = Excludes::new(&[
            "target/".into(),
            "sources/core/swift-check/.build/".into(),
            ".png".into(),
            "Package.resolved".into(),
            "docs/icon/big.svg".into(),
        ]);
        assert!(rules.leaves_out("sources/core/target", "target", true));
        assert!(rules.leaves_out("target", "target", true));
        assert!(!rules.leaves_out("target", "target", false));
        assert!(rules.leaves_out("sources/core/swift-check/.build", ".build", true));
        assert!(!rules.leaves_out("other/.build", ".build", true));
        assert!(rules.leaves_out("docs/a.png", "a.png", false));
        assert!(rules.leaves_out("x/Package.resolved", "Package.resolved", false));
        assert!(rules.leaves_out("docs/icon/big.svg", "big.svg", false));
        assert!(!rules.leaves_out("docs/big.svg", "big.svg", false));
    }

    /// A working directory of some 19,000 files bound here: main read as the window reads it and
    /// its cards listed as MCP lists them take no longer than with none, and make nothing of what
    /// the working directory holds; what is in it is made once, when a call reads in it, and kept for
    /// the next.
    #[test]
    fn what_a_working_directory_holds_costs_nothing_to_a_tree_that_does_not_reach_into_it() {
        use serde_json::json;
        let core = crate::Core::open(crate::fixtures::store_with(&["sample"])).unwrap();
        let project: String = rusqlite::Connection::open(core.place())
            .unwrap()
            .query_row("SELECT id FROM projects WHERE name = '見本'", [], |r| {
                r.get(0)
            })
            .unwrap();
        let call = |request: serde_json::Value| -> serde_json::Value {
            let said: serde_json::Value =
                serde_json::from_str(&core.call(&request.to_string())).unwrap();
            assert!(said.get("error").is_none(), "{request}: {said}");
            said
        };
        let detail = json!({"op": "project_detail", "project": project});
        let list = json!({"op": "tool", "name": "card_list", "arguments": {"project": "見本", "value": {"limit": 200}}});
        // The least of a few runs: what one call costs.
        let least = |request: &serde_json::Value| -> std::time::Duration {
            (0..4)
                .map(|_| {
                    let at = std::time::Instant::now();
                    call(request.clone());
                    at.elapsed()
                })
                .min()
                .unwrap()
        };
        let (detail_before, list_before) = (least(&detail), least(&list));
        let folder = crate::fixtures::folder("many");
        for d in 0..190 {
            let dir = folder.join(format!("d{d:03}"));
            std::fs::create_dir_all(&dir).unwrap();
            for f in 0..100 {
                std::fs::write(dir.join(format!("f{f:03}.rs")), "").unwrap();
            }
        }
        call(
            json!({"op": "tool", "name": "section_update", "arguments": {
                "project": "見本", "target": "ts", "value": {"folder": folder.to_string_lossy()}
            }}),
        );
        let read = call(
            json!({"op": "tool", "name": "section_read", "arguments": {"project": "見本", "target": "ts"}}),
        );
        assert_eq!(read["result"]["json"]["files"], 19_000, "{read}");
        assert_eq!(read["result"]["json"]["directories"], 190, "{read}");
        let made = overlays_made_on_this_thread();
        let (detail_after, list_after) = (least(&detail), least(&list));
        assert_eq!(overlays_made_on_this_thread(), made);
        eprintln!(
            "project_detail {detail_before:?} → {detail_after:?}, card_list {list_before:?} → {list_after:?}"
        );
        let room = |before: std::time::Duration| before * 2 + std::time::Duration::from_millis(50);
        assert!(
            detail_after <= room(detail_before),
            "{detail_before:?} → {detail_after:?}"
        );
        assert!(
            list_after <= room(list_before),
            "{list_before:?} → {list_after:?}"
        );
        // Read in it: made once, kept for the next call of the same reading.
        let file = json!({"op": "tool", "name": "card_read", "arguments": {"project": "見本", "target": "ts/d007:'f042.rs'"}});
        let at = std::time::Instant::now();
        assert_eq!(
            call(file.clone())["result"]["json"]["filePath"],
            "d007/f042.rs"
        );
        let first = at.elapsed();
        let once = overlays_made_on_this_thread();
        assert!(once <= made + 1);
        let at = std::time::Instant::now();
        call(file);
        eprintln!("card_read of a file: {first:?}, then {:?}", at.elapsed());
        assert_eq!(overlays_made_on_this_thread(), once);
        let _ = std::fs::remove_dir_all(&folder);
    }
}
