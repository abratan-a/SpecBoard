//! The history (`specboard://SpecBoard/Docs/Design:DesignBranches` §7, `History` in the Swift app): what a
//! transaction changed, kept as one commit per tree it wrote — each row as it was before
//! the transaction first wrote it, a line for each thing done, and who did it — and going
//! back to a commit, which puts those rows back as one commit of its own.

use super::{Seen, Side, Table, Tx, Who, db, is_branch};
use crate::text::Keyed;
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};

type Key = (String, String);

/// A line of what was done, as the history shows it: `summary` said in the language of the call
/// that reads it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    pub kind: String,
    pub target_type: String,
    pub target_id: Option<String>,
    pub summary: String,
}

/// A line of what was done, as a transaction gathers it: its text kept by key, to be said in the
/// language of whoever reads the history.
#[derive(Clone, Debug)]
struct Said {
    kind: String,
    target_type: String,
    target_id: Option<String>,
    said: Keyed,
}

/// What a transaction has gathered so far.
#[derive(Clone, Default)]
pub(super) struct Gathered {
    order: Vec<Key>,
    lines: HashMap<Key, Vec<Said>>,
    rows: HashMap<Key, Vec<(Table, String, Option<String>)>>,
    seen: HashSet<(Key, Table, String)>,
    kinds: HashMap<Key, String>,
    pub(super) written: Vec<(String, Keyed)>,
}

/// The language a history line's `summary` is kept in: what an app of before reads, and what a line
/// is said in once its key is gone from the tables.
const KEPT_IN: &str = "ja";

/// The name of a commit a move of the store's form writes (`kind` `migrate`), kept as a sentence in
/// `KEPT_IN` — what an app of before shows — and said again by `commit_name_said` in the language of
/// whoever reads it.
pub(crate) fn migrated(said: Keyed) -> String {
    said.render_in(KEPT_IN)
}

/// The names the moves of the store's form give their commits (`migrated`).
const MIGRATED: &[&str] = &[
    "history.migrate.lineForm",
    "history.migrate.lineEnds",
    "history.migrate.containers",
    "history.migrate.ghosts",
    "history.migrate.goalKeys",
    "history.migrate.goalMr",
    "history.migrate.goalWorkBranch",
];

/// A commit's name as the call reading it says it: a move's of the store's form in its language, any
/// other as it was written.
pub(crate) fn commit_name_said(kind: &str, name: Option<String>) -> Option<String> {
    match name {
        Some(name) if kind == "migrate" => {
            Some(crate::text::said_as(MIGRATED, &name).unwrap_or(name))
        }
        other => other,
    }
}

/// A history line written as a row of `commit_lines` (`idx` its place in the commit): its text kept
/// by key and values, and as a sentence in `KEPT_IN`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn insert_line(
    conn: &Connection,
    commit: &str,
    idx: i64,
    kind: &str,
    target_type: &str,
    target_id: Option<&str>,
    said: &Keyed,
) -> rusqlite::Result<usize> {
    let key = (!said.key().is_empty()).then(|| said.key().to_string());
    let values = key
        .as_ref()
        .map(|_| said.values_json())
        .filter(|v| v.as_object().is_some_and(|o| !o.is_empty()))
        .map(|v| v.to_string());
    conn.execute(
        "INSERT INTO commit_lines (commit_id, idx, kind, target_type, target_id, summary, text_key, text_values) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        params![
            commit,
            idx,
            kind,
            target_type,
            target_id,
            said.render_in(KEPT_IN),
            key,
            values
        ],
    )
}

/// A history line's text as the call reading it says it: by its key, when it was kept with one the
/// tables still have; else its sentence (kept in `KEPT_IN`) read back by the table that made it
/// (`text::said_again`), or as it was kept — so one history is said in one language.
pub(crate) fn line_said(summary: String, key: Option<String>, values: Option<String>) -> String {
    let Some(key) = key.filter(|k| crate::text::has(k)) else {
        return crate::text::said_again(&summary).unwrap_or(summary);
    };
    let values: Option<Value> = match values {
        Some(raw) => match serde_json::from_str(&raw) {
            Ok(v) => Some(v),
            Err(_) => return summary,
        },
        None => None,
    };
    Keyed::from_parts(&key, values.as_ref())
        .map(|k| k.render())
        .unwrap_or(summary)
}

/// Who made a commit, as people call them: the window, or MCP with the access key's name.
pub fn author_said(kind: &str, name: Option<&str>) -> String {
    match (kind, name) {
        ("mcp", Some(name)) => text!("history.author.mcpNamed", name),
        ("mcp", None) => "MCP".to_string(),
        ("screen", _) => text!("history.author.screen"),
        (other, _) => other.to_string(),
    }
}

/// Kinds of commit that are made even when nothing in the documents changed.
const STANDS_ALONE: [&str; 1] = ["export"];

/// A row as one JSON object, by the columns it is kept and put back by; none when there
/// is none.
fn row(
    conn: &Connection,
    table: Table,
    project: &str,
    tree: &str,
    id: &str,
) -> Result<Option<String>, String> {
    let names = table.columns();
    conn.query_row(
        &format!(
            "SELECT {} FROM {} WHERE project_id = ? AND branch_id = ? AND id = ?",
            names.join(", "),
            table.name()
        ),
        params![project, tree, id],
        |r| {
            let mut out = Map::new();
            for (i, name) in names.iter().enumerate() {
                let value = match r.get_ref(i)? {
                    rusqlite::types::ValueRef::Null => Value::Null,
                    rusqlite::types::ValueRef::Integer(n) => n.into(),
                    rusqlite::types::ValueRef::Real(f) => f.into(),
                    rusqlite::types::ValueRef::Text(t) => {
                        String::from_utf8_lossy(t).into_owned().into()
                    }
                    rusqlite::types::ValueRef::Blob(_) => Value::Null,
                };
                out.insert(name.to_string(), value);
            }
            Ok(Value::Object(out).to_string())
        },
    )
    .optional()
    .map_err(db)
}

impl Gathered {
    fn touch(&mut self, key: &Key) {
        if !self.order.contains(key) {
            self.order.push(key.clone());
        }
    }

    pub(super) fn line(
        &mut self,
        project: &str,
        tree: &str,
        kind: &str,
        target_type: &str,
        target_id: Option<&str>,
        said: Keyed,
    ) {
        let key = (project.to_string(), tree.to_string());
        self.touch(&key);
        self.written.push((project.to_string(), said.clone()));
        self.lines.entry(key).or_default().push(Said {
            kind: kind.to_string(),
            target_type: target_type.to_string(),
            target_id: target_id.map(str::to_string),
            said,
        });
    }

    /// How many lines the tree has gathered so far.
    pub(super) fn said_in(&self, project: &str, tree: &str) -> usize {
        self.lines
            .get(&(project.to_string(), tree.to_string()))
            .map_or(0, Vec::len)
    }

    /// The tree's lines after the first `kept` let go of, and taken out of what was written too.
    pub(super) fn unsay(&mut self, project: &str, tree: &str, kept: usize) {
        let Some(lines) = self.lines.get_mut(&(project.to_string(), tree.to_string())) else {
            return;
        };
        if lines.len() <= kept {
            return;
        }
        for gone in lines.split_off(kept) {
            if let Some(at) = self
                .written
                .iter()
                .rposition(|(p, said)| p == project && *said == gone.said)
            {
                self.written.remove(at);
            }
        }
    }

    pub(super) fn mark(&mut self, project: &str, tree: &str, kind: &str) {
        let key = (project.to_string(), tree.to_string());
        self.touch(&key);
        self.kinds.insert(key, kind.to_string());
    }

    /// The rows of `table` this transaction wrote in the tree, in the order it first wrote them.
    pub(super) fn ids(&self, project: &str, tree: &str, table: Table) -> Vec<String> {
        self.rows
            .get(&(project.to_string(), tree.to_string()))
            .into_iter()
            .flatten()
            .filter(|(t, _, _)| *t == table)
            .map(|(_, id, _)| id.clone())
            .collect()
    }

    /// A row as it is now, before this transaction writes it — only the first time.
    pub(super) fn before(
        &mut self,
        conn: &Connection,
        table: Table,
        project: &str,
        tree: &str,
        id: &str,
    ) -> Result<(), String> {
        let key = (project.to_string(), tree.to_string());
        if !self.seen.insert((key.clone(), table, id.to_string())) {
            return Ok(());
        }
        self.touch(&key);
        let was = row(conn, table, project, tree, id)?;
        self.rows
            .entry(key)
            .or_default()
            .push((table, id.to_string(), was));
        Ok(())
    }

    /// The commits, written: one per tree the transaction wrote. A write to a tree the
    /// other side holds is refused, and so the whole transaction; the window's writes to
    /// a tree it holds are added to the one commit it makes.
    pub(super) fn flush(self, conn: &Connection, who: &Who) -> Result<(), String> {
        let at = super::now(conn)?;
        let side = who.side.word();
        for key in &self.order {
            let (project, tree) = key;
            let changed = self.rows.get(key).map(Vec::as_slice).unwrap_or(&[]);
            let said = self.lines.get(key).map(Vec::as_slice).unwrap_or(&[]);
            let kind = self.kinds.get(key);
            let wrote = !changed.is_empty() || !said.is_empty();
            // A mark alone makes a commit only when it is what happened (an export).
            if !wrote && !kind.is_some_and(|k| STANDS_ALONE.contains(&k.as_str())) {
                continue;
            }
            // A project or a branch gone in this transaction keeps no history.
            if !exists(conn, "SELECT 1 FROM projects WHERE id = ?", project)? {
                continue;
            }
            if is_branch(tree) && !exists(conn, "SELECT 1 FROM branches WHERE id = ?", tree)? {
                continue;
            }
            let holds = super::holds::holds(conn, project, tree)?;
            // What is held is the tree's documents: a line alone (an export, the project's
            // own settings) goes through.
            if !changed.is_empty()
                && let Some(other) = holds.iter().find(|h| !h.is_of(who))
            {
                return Err(super::holds::refusal(conn, other));
            }
            let latest: Option<(String, i64)> = conn
                .query_row(
                    "SELECT id, seq FROM commits WHERE project_id = ? AND branch_id = ? ORDER BY seq DESC LIMIT 1",
                    params![project, tree],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()
                .map_err(db)?;
            let group = (who.side == Side::Screen && kind.is_none())
                .then(|| holds.iter().find(|h| h.holder == "screen" && h.is_of(who)))
                .flatten();
            let joined = group
                .and_then(|g| g.commit_id.clone())
                .filter(|id| latest.as_ref().is_some_and(|l| &l.0 == id));
            let id = match joined {
                Some(id) => id,
                None => {
                    let id = super::new_id();
                    let seq = latest.as_ref().map_or(0, |l| l.1) + 1;
                    let kind = if group.is_some() {
                        "batch"
                    } else {
                        kind.map_or("edit", String::as_str)
                    };
                    conn.execute(
                        "INSERT INTO commits (id, project_id, branch_id, seq, at, author_kind, author, kind) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
                        params![id, project, tree, seq, at, side, who.name, kind],
                    )
                    .map_err(db)?;
                    if group.is_some() {
                        conn.execute(
                            "UPDATE holds SET commit_id = ? WHERE project_id = ? AND branch_id = ? AND holder = 'screen' AND client = ?",
                            params![id, project, tree, who.client],
                        )
                        .map_err(db)?;
                    }
                    id
                }
            };
            let first: i64 = conn
                .query_row(
                    "SELECT COALESCE(MAX(idx) + 1, 0) FROM commit_lines WHERE commit_id = ?",
                    [&id],
                    |r| r.get(0),
                )
                .map_err(db)?;
            for (i, line) in said.iter().enumerate() {
                insert_line(
                    conn,
                    &id,
                    first + i as i64,
                    &line.kind,
                    &line.target_type,
                    line.target_id.as_deref(),
                    &line.said,
                )
                .map_err(db)?;
            }
            // Added to a commit that has the row already: what it was before that stays.
            for (table, row_id, before) in changed {
                conn.execute(
                    "INSERT OR IGNORE INTO commit_rows (commit_id, tbl, row_id, before) VALUES (?, ?, ?, ?)",
                    params![id, table.name(), row_id, before],
                )
                .map_err(db)?;
            }
        }
        Ok(())
    }
}

fn exists(conn: &Connection, query: &str, id: &str) -> Result<bool, String> {
    conn.query_row(query, [id], |_| Ok(()))
        .optional()
        .map(|found| found.is_some())
        .map_err(db)
}

/// A commit, as the history lists it.
#[derive(Clone, Debug, PartialEq)]
pub struct Commit {
    pub id: String,
    pub seq: i64,
    pub at: String,
    pub author_kind: String,
    pub author: Option<String>,
    pub kind: String,
    pub name: Option<String>,
    pub reverted: bool,
    pub lines: Vec<Line>,
}

/// Who made a commit: the window, or MCP with or without a key's name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Author {
    pub kind: String,
    pub name: Option<String>,
}

impl Author {
    pub fn of(who: &Who) -> Author {
        Author {
            kind: who.side.word().to_string(),
            name: who.name.clone(),
        }
    }
}

/// What undoing a tree's commits after one takes: the commits (id and number), and each row
/// they wrote, in the order first written, as it was before.
struct Undo {
    range: Vec<(String, i64)>,
    order: Vec<(Table, String)>,
    before: HashMap<(Table, String), Option<String>>,
}

/// What a revert of a locked main is told.
fn locked_revert() -> String {
    text!("history.revert.locked")
}

/// The stop for a history no one cuts: per tree, the commits past the newest
/// `KEPT_COMMITS` that are also older than `KEPT_DAYS` — never by one of the two alone.
pub const KEPT_COMMITS: i64 = 1_000;
pub const KEPT_DAYS: i64 = 90;

/// The commits of a tree, newest first.
pub fn commits(
    conn: &Connection,
    project: &str,
    tree: &str,
    limit: i64,
) -> Result<Vec<Commit>, String> {
    commits_before(conn, project, tree, None, limit)
}

/// How many commits a tree has.
pub fn commit_count(conn: &Connection, project: &str, tree: &str) -> Result<i64, String> {
    conn.query_row(
        "SELECT COUNT(*) FROM commits WHERE project_id = ? AND branch_id = ?",
        params![project, tree],
        |r| r.get(0),
    )
    .map_err(db)
}

/// The tree's commits, the newest first, up to `limit` — those before `before` (its seq), when it is
/// said (a history read on through MCP).
pub fn commits_before(
    conn: &Connection,
    project: &str,
    tree: &str,
    before: Option<i64>,
    limit: i64,
) -> Result<Vec<Commit>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, seq, at, author_kind, author, kind, name, reverted FROM commits WHERE project_id = ? AND branch_id = ? AND seq < ? ORDER BY seq DESC LIMIT ?",
        )
        .map_err(db)?;
    let mut out = stmt
        .query_map(
            params![project, tree, before.unwrap_or(i64::MAX), limit],
            |r| {
                Ok(Commit {
                    id: r.get(0)?,
                    seq: r.get(1)?,
                    at: r.get(2)?,
                    author_kind: r.get(3)?,
                    author: r.get(4)?,
                    kind: r.get(5)?,
                    name: commit_name_said(&r.get::<_, String>(5)?, r.get(6)?),
                    reverted: r.get::<_, i64>(7)? != 0,
                    lines: Vec::new(),
                })
            },
        )
        .map_err(db)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(db)?;
    let mut stmt = conn
        .prepare("SELECT kind, target_type, target_id, summary, text_key, text_values FROM commit_lines WHERE commit_id = ? ORDER BY idx")
        .map_err(db)?;
    for commit in &mut out {
        commit.lines = stmt
            .query_map([&commit.id], |r| {
                Ok(Line {
                    kind: r.get(0)?,
                    target_type: r.get(1)?,
                    target_id: r.get(2)?,
                    summary: line_said(r.get(3)?, r.get(4)?, r.get(5)?),
                })
            })
            .map_err(db)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db)?;
    }
    Ok(out)
}

impl Tx<'_> {
    /// What undoing the tree's commits after `seq` takes (`revert`, `revert_into_branch`): the
    /// commits, and each row they wrote as it was before the oldest of them that wrote it.
    /// `only_by`: a range holding anyone else's commits is refused, naming them.
    fn undo(
        &self,
        project: &str,
        tree: &str,
        seq: i64,
        only_by: Option<&Author>,
    ) -> Result<Undo, String> {
        if !exists_seq(&self.tx, project, tree, seq)? {
            return Err(text!("history.commit.noneOrCut", seq));
        }
        struct Undone {
            id: String,
            seq: i64,
            author: Author,
        }
        let range: Vec<Undone> = {
            let mut stmt = self
                .tx
                .prepare("SELECT id, seq, author_kind, author FROM commits WHERE project_id = ? AND branch_id = ? AND seq > ? ORDER BY seq")
                .map_err(db)?;
            stmt.query_map(params![project, tree, seq], |r| {
                Ok(Undone {
                    id: r.get(0)?,
                    seq: r.get(1)?,
                    author: Author {
                        kind: r.get(2)?,
                        name: r.get(3)?,
                    },
                })
            })
            .map_err(db)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db)?
        };
        if range.is_empty() {
            return Err(text!("history.commit.noneAfter", seq));
        }
        if let Some(author) = only_by {
            // Another's that wrote no card nor section (the lock put on or taken off) is not
            // undone by going back: not counted.
            let wrote = |commit: &str| -> bool {
                self.tx
                    .query_row(
                        "SELECT 1 FROM commit_rows WHERE commit_id = ? LIMIT 1",
                        [commit],
                        |_| Ok(()),
                    )
                    .optional()
                    .ok()
                    .flatten()
                    .is_some()
            };
            let others: Vec<String> = range
                .iter()
                .filter(|c| &c.author != author && wrote(&c.id))
                .map(|c| {
                    let who = author_said(&c.author.kind, c.author.name.as_deref());
                    text!("history.revert.otherCommit", seq = c.seq, who)
                })
                .collect();
            if !others.is_empty() {
                return Err(text!(
                    "history.revert.othersIn",
                    commits = crate::text::list_comma(&others)
                ));
            }
        }
        // Each row as it was before the oldest commit in the range that wrote it.
        let mut order: Vec<(Table, String)> = Vec::new();
        let mut before: HashMap<(Table, String), Option<String>> = HashMap::new();
        {
            let mut stmt = self
                .tx
                .prepare("SELECT tbl, row_id, before FROM commit_rows WHERE commit_id = ?")
                .map_err(db)?;
            for commit in &range {
                let rows = stmt
                    .query_map([&commit.id], |r| {
                        Ok((
                            r.get::<_, String>(0)?,
                            r.get::<_, String>(1)?,
                            r.get::<_, Option<String>>(2)?,
                        ))
                    })
                    .map_err(db)?
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(db)?;
                for (table, id, row) in rows {
                    let Some(table) = Table::of(&table) else {
                        continue;
                    };
                    let key = (table, id);
                    if before.contains_key(&key) {
                        continue;
                    }
                    before.insert(key.clone(), row);
                    order.push(key);
                }
            }
        }
        Ok(Undo {
            range: range.into_iter().map(|c| (c.id, c.seq)).collect(),
            order,
            before,
        })
    }

    /// The tree as it was right after its commit `seq`: every commit after it undone,
    /// newest first, as one commit of its own — git's revert of the range (§7). The
    /// undone ones stay, marked. `only_by`: a range holding anyone else's commits is
    /// refused, naming them (what MCP may undo). Returns how many were undone.
    pub fn revert(
        &mut self,
        project: &str,
        tree: &str,
        seq: i64,
        only_by: Option<&Author>,
    ) -> Result<usize, String> {
        if tree.is_empty() && self.locked_at(project)?.is_some() {
            return Err(locked_revert());
        }
        let Undo {
            range,
            order,
            before,
        } = self.undo(project, tree, seq, only_by)?;
        // Out of the way first, so names that change hands do not meet.
        for (table, id) in &order {
            self.history.before(&self.tx, *table, project, tree, id)?;
            self.tx
                .execute(
                    &format!(
                        "DELETE FROM {} WHERE project_id = ? AND branch_id = ? AND id = ?",
                        table.name()
                    ),
                    params![project, tree, id],
                )
                .map_err(db)?;
        }
        for (table, id) in &order {
            let Some(Some(kept)) = before.get(&(*table, id.clone())) else {
                continue;
            };
            let Some(values) = crate::store::parse(Some(kept)) else {
                continue;
            };
            let names = table.columns();
            let binds: Vec<rusqlite::types::Value> = [project, tree, id.as_str()]
                .iter()
                .map(|s| rusqlite::types::Value::from(s.to_string()))
                .chain(names.iter().map(|name| {
                    match values.get(*name) {
                        Some(Value::Number(n)) => n
                            .as_i64()
                            .map(rusqlite::types::Value::from)
                            .or_else(|| n.as_f64().map(rusqlite::types::Value::from))
                            .unwrap_or(rusqlite::types::Value::Null),
                        Some(Value::String(t)) => t.clone().into(),
                        Some(Value::Bool(b)) => (*b as i64).into(),
                        _ => rusqlite::types::Value::Null,
                    }
                }))
                .collect();
            self.tx
                .execute(
                    &format!(
                        "INSERT INTO {} (project_id, branch_id, id, {}) VALUES ({})",
                        table.name(),
                        names.join(", "),
                        vec!["?"; names.len() + 3].join(", ")
                    ),
                    rusqlite::params_from_iter(binds),
                )
                .map_err(db)?;
        }
        for commit in &range {
            self.tx
                .execute("UPDATE commits SET reverted = 1 WHERE id = ?", [&commit.0])
                .map_err(db)?;
        }
        self.mark(project, tree, "revert");
        self.line(
            project,
            tree,
            "revert",
            "project",
            None,
            keyed!("history.revert.line", count = range.len(), seq),
        )?;
        Ok(range.len())
    }

    /// main as it was right after its commit `seq`, as a branch rather than written into main:
    /// each row main's later commits wrote, as it was before them, written in a new branch (`name`, else
    /// `revert-<seq>`) for a merge to take into main — through a merge request while main is
    /// locked. main is not touched. The branches set aside by the merges it undoes are named on
    /// it, to come back when it is merged (`bring_back`). Returns the branch's id and name, and
    /// how many commits it undoes.
    pub fn revert_into_branch(
        &mut self,
        project: &str,
        seq: i64,
        name: Option<&str>,
        only_by: Option<&Author>,
    ) -> Result<(String, String, usize), String> {
        let Undo {
            range,
            order,
            before,
        } = self.undo(project, "", seq, only_by)?;
        let (id, named) = self.branch_for(project, name, &format!("revert-{seq}"))?;
        for (table, row) in &order {
            let then = before
                .get(&(*table, row.clone()))
                .cloned()
                .flatten()
                .and_then(|kept| crate::store::parse(Some(&kept)))
                .filter(|kept| match kept.get("removed") {
                    Some(Value::Bool(b)) => !b,
                    Some(v) => v.as_i64().unwrap_or(0) == 0,
                    None => true,
                })
                .and_then(|kept| kept.get("doc").and_then(Value::as_str).map(str::to_string));
            let now: Option<String> = self
                .tx
                .query_row(
                    &format!(
                        "SELECT doc FROM {} WHERE project_id = ? AND branch_id = '' AND id = ? AND removed = 0",
                        table.name()
                    ),
                    params![project, row],
                    |r| r.get(0),
                )
                .optional()
                .map_err(db)?
                .flatten();
            let as_doc = |text: &str| crate::store::parse(Some(text)).unwrap_or_default();
            match (then, now) {
                (Some(then), Some(now)) if as_doc(&then) == as_doc(&now) => {}
                // As it was, over main's now: a change for the merge to take in.
                (Some(then), Some(now)) => {
                    let keys = table.keys(&as_doc(&then));
                    self.upsert(
                        *table,
                        project,
                        &id,
                        row,
                        keys,
                        false,
                        Some(then),
                        Some(now),
                    )?;
                }
                // main let it go since: made again, as the branch's own.
                (Some(then), None) => {
                    let keys = table.keys(&as_doc(&then));
                    self.upsert(*table, project, &id, row, keys, false, Some(then), None)?;
                }
                // main made it since: taken away.
                (None, Some(now)) => {
                    let keys = table.keys(&as_doc(&now));
                    self.upsert(*table, project, &id, row, keys, true, None, Some(now))?;
                }
                (None, None) => {}
            }
        }
        // The branches the undone merges set aside.
        let ids: Vec<String> = range.iter().map(|(c, _)| c.clone()).collect();
        self.asides_of(project, &id, &ids)?;
        self.mark(project, &id, "revert");
        self.line(
            project,
            &id,
            "revert",
            "project",
            None,
            keyed!("history.revertIntoBranch.line", count = range.len(), seq),
        )?;
        Ok((id, named, range.len()))
    }

    /// Every commit of the tree before `seq` let go — there is no going back past it any
    /// more (§7). The bases merges compare by and the prints are kept apart from the
    /// history, so merging and checking do not change. Returns how many went.
    pub fn cut(&mut self, project: &str, tree: &str, seq: i64) -> Result<usize, String> {
        if !exists_seq(&self.tx, project, tree, seq)? {
            return Err(text!("history.commit.none", seq));
        }
        let cut = self
            .tx
            .execute(
                "DELETE FROM commits WHERE project_id = ? AND branch_id = ? AND seq < ?",
                params![project, tree, seq],
            )
            .map_err(db)?;
        // The branches set aside by merges cut from main's history go with them.
        if tree.is_empty() {
            self.let_go_of_asides(project)?;
        }
        Ok(cut)
    }

    /// The history no one cut, trimmed when a project is opened (`KEPT_COMMITS`,
    /// `KEPT_DAYS`). Returns how many went.
    pub fn prune(&mut self, project: &str) -> Result<usize, String> {
        let pruned = self
            .tx
            .execute(
                "DELETE FROM commits WHERE at < strftime('%Y-%m-%dT%H:%M:%fZ', 'now', ?) AND id IN (
                   SELECT id FROM (SELECT id, ROW_NUMBER() OVER (PARTITION BY branch_id ORDER BY seq DESC) AS n
                                     FROM commits WHERE project_id = ?) WHERE n > ?)",
                params![format!("-{KEPT_DAYS} days"), project, KEPT_COMMITS],
            )
            .map_err(db)?;
        // The branches set aside by merges cut from main's history go with them.
        if pruned > 0 {
            self.let_go_of_asides(project)?;
        }
        Ok(pruned)
    }

    /// A name on a commit, by hand: a mark on the history. Empty takes it off.
    pub fn name_commit(
        &mut self,
        project: &str,
        tree: &str,
        seq: i64,
        name: &str,
    ) -> Result<(), String> {
        let name = name.trim();
        if name.chars().count() > 80 {
            return Err(text!("history.commit.nameTooLong"));
        }
        let changed = self
            .tx
            .execute(
                "UPDATE commits SET name = ? WHERE project_id = ? AND branch_id = ? AND seq = ?",
                params![(!name.is_empty()).then_some(name), project, tree, seq],
            )
            .map_err(db)?;
        if changed == 0 {
            return Err(text!("history.commit.none", seq));
        }
        Ok(())
    }

    /// The rows a commit kept, as they were before it: (table, id, row).
    pub fn kept(&self, commit: &str) -> Result<Vec<(Table, String, Option<Seen>)>, String> {
        let mut stmt = self
            .tx
            .prepare("SELECT tbl, row_id, before FROM commit_rows WHERE commit_id = ?")
            .map_err(db)?;
        let rows = stmt
            .query_map([commit], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, Option<String>>(2)?,
                ))
            })
            .map_err(db)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db)?;
        Ok(rows
            .into_iter()
            .filter_map(|(table, id, row)| {
                let table = Table::of(&table)?;
                let seen = crate::store::parse(row.as_deref()).map(|values| Seen {
                    own: true,
                    removed: values.get("removed").and_then(Value::as_i64).unwrap_or(0) != 0,
                    doc: values
                        .get("doc")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    base: values
                        .get("base")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                });
                Some((table, id, seen))
            })
            .collect())
    }
}

/// The commit of the tree numbered `seq`, as the history lists it; none when there is none (never
/// made, or cut).
pub fn commit_at(
    conn: &Connection,
    project: &str,
    tree: &str,
    seq: i64,
) -> Result<Option<Commit>, String> {
    Ok(
        commits_before(conn, project, tree, Some(seq.saturating_add(1)), 1)?
            .into_iter()
            .find(|c| c.seq == seq),
    )
}

/// One row a commit kept, around it: as it was before the commit, as it was right after it, and the
/// later commits of the tree that wrote it too.
#[derive(Clone, Debug)]
pub struct Around {
    pub table: Table,
    pub id: String,
    /// The row before the commit; none for no row there.
    pub before: Option<Map<String, Value>>,
    /// The row right after the commit: the before of the next commit that kept it (`Some(None)`: no
    /// row there then), or none when no later commit wrote it — the row as it is now.
    pub after: Option<Option<Map<String, Value>>>,
    /// The later commits that wrote the row, oldest first: what an undo of the commit meets.
    pub later: Vec<i64>,
}

/// The rows the commit `seq` kept, in the order it first wrote them, each around it: what a commit's
/// changes are read from (the screen's and MCP's), and what undoing it alone compares.
pub fn around(
    conn: &Connection,
    project: &str,
    tree: &str,
    seq: i64,
) -> Result<Vec<Around>, String> {
    let commit: Option<String> = conn
        .query_row(
            "SELECT id FROM commits WHERE project_id = ? AND branch_id = ? AND seq = ?",
            params![project, tree, seq],
            |r| r.get(0),
        )
        .optional()
        .map_err(db)?;
    let Some(commit) = commit else {
        return Ok(Vec::new());
    };
    let kept: Vec<(String, String, Option<String>)> = {
        let mut stmt = conn
            .prepare(
                "SELECT tbl, row_id, before FROM commit_rows WHERE commit_id = ? ORDER BY rowid",
            )
            .map_err(db)?;
        stmt.query_map([&commit], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .map_err(db)?
            .collect::<Result<_, _>>()
            .map_err(db)?
    };
    let mut stmt = conn
        .prepare(
            "SELECT c.seq, r.before FROM commit_rows r JOIN commits c ON c.id = r.commit_id
              WHERE c.project_id = ? AND c.branch_id = ? AND c.seq > ? AND r.tbl = ? AND r.row_id = ?
              ORDER BY c.seq",
        )
        .map_err(db)?;
    let mut out = Vec::new();
    for (table, id, row) in kept {
        let Some(table_of) = Table::of(&table) else {
            continue;
        };
        let later: Vec<(i64, Option<String>)> = stmt
            .query_map(params![project, tree, seq, table, id], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .map_err(db)?
            .collect::<Result<_, _>>()
            .map_err(db)?;
        out.push(Around {
            table: table_of,
            id,
            before: crate::store::parse(row.as_deref()),
            after: later
                .first()
                .map(|(_, next)| crate::store::parse(next.as_deref())),
            later: later.iter().map(|(s, _)| *s).collect(),
        });
    }
    Ok(out)
}

/// A unit undoing one commit would change that a later commit changed too: refused whole, so a
/// commit is never undone in part.
#[derive(Clone, Debug)]
pub struct UndoConflict {
    pub table: Table,
    pub id: String,
    /// The card's kind, for saying the unit in MCP's shape; none for a section.
    pub kind: Option<String>,
    /// The unit as it is kept apart, and as a person names it (`MergeConflict`).
    pub unit: String,
    pub label: String,
    /// Before the commit, right after it, and now.
    pub before: Option<Value>,
    pub after: Option<Value>,
    pub now: Option<Value>,
    /// The later commits that wrote the row.
    pub later: Vec<i64>,
}

/// What undoing one commit came to.
#[derive(Clone, Debug)]
pub enum UndoOne {
    /// Undone: the rows written, by table and id, and the branch it was written in (main's undo).
    Done {
        changed: Vec<(Table, String)>,
        made: Option<(String, String)>,
    },
    /// Refused, nothing written: the units a later commit changed too.
    Conflicts(Vec<UndoConflict>),
}

/// One row of an undo worked out: what it becomes in the tree (none: gone), and how the tree sees it
/// now.
struct Undoing {
    table: Table,
    id: String,
    result: Option<Map<String, Value>>,
    now: Option<Map<String, Value>>,
}

/// What undoing one commit comes to before anything is written: the rows, or the units a later
/// commit changed too.
type Worked = Result<Vec<Undoing>, Vec<UndoConflict>>;

/// A row as a commit keeps it (`Table::columns`), as the tree sees its document: none when there is
/// no row or it is a branch's mark that it removed one.
fn doc_of(row: &Map<String, Value>) -> Option<Map<String, Value>> {
    let removed = match row.get("removed") {
        Some(Value::Bool(b)) => *b,
        Some(v) => v.as_i64().unwrap_or(0) != 0,
        None => false,
    };
    if removed {
        return None;
    }
    crate::store::parse(row.get("doc").and_then(Value::as_str))
}

impl Tx<'_> {
    /// What undoing the commit `seq` alone would write, row by row: for each row it kept, the row
    /// right after it (the base), before it (what goes back) and now merged unit by unit as a merge
    /// does — a unit only the commit changed goes back, one only a later commit changed stays as it
    /// is now, one both changed is a conflict. A branch's row it kept as none is main's beneath it,
    /// as the branch sees it now. Refused: no such commit, one that wrote no row, one by another
    /// writer when `only_by` says whose alone may be undone, and one whose changes are undone
    /// already.
    fn undo_work(
        &self,
        project: &str,
        tree: &str,
        seq: i64,
        only_by: Option<&Author>,
    ) -> Result<(Commit, Worked), String> {
        let Some(commit) = commit_at(&self.tx, project, tree, seq)? else {
            return Err(text!("history.commit.noneOrCut", seq));
        };
        let rows = around(&self.tx, project, tree, seq)?;
        if rows.is_empty() {
            return Err(text!("history.undo.wroteNothing", seq));
        }
        let by = Author {
            kind: commit.author_kind.clone(),
            name: commit.author.clone(),
        };
        if let Some(only) = only_by
            && &by != only
        {
            return Err(text!(
                "history.undo.others",
                seq,
                who = author_said(&by.kind, by.name.as_deref())
            ));
        }
        let branch = is_branch(tree);
        // The document as the tree sees it where the row is none: main's beneath a branch, as the
        // branch was made where main moved since.
        let beneath = |table: Table, id: &str| -> Result<Option<Map<String, Value>>, String> {
            if !branch {
                return Ok(None);
            }
            if let Some(kept) = self.frozen(table, project, tree, id)? {
                return Ok(kept.and_then(|d| crate::store::parse(Some(&d))));
            }
            Ok(self
                .own(table, project, "", id)?
                .filter(|s| !s.removed)
                .and_then(|s| s.doc()))
        };
        // A branch's row as none: what it saw beneath — main as the branch's next row of it was based
        // on, when there is one, else as the branch sees main now.
        let seen = |table: Table,
                    id: &str,
                    row: Option<&Map<String, Value>>,
                    based: Option<&str>|
         -> Result<Option<Map<String, Value>>, String> {
            match (row, based) {
                (Some(row), _) => Ok(doc_of(row)),
                (None, Some(base)) if branch => Ok(crate::store::parse(Some(base))),
                (None, _) => beneath(table, id),
            }
        };
        let base_of =
            |row: &Map<String, Value>| row.get("base").and_then(Value::as_str).map(str::to_string);
        let mut undoing = Vec::new();
        let mut conflicts = Vec::new();
        for kept in &rows {
            let own = self.own(kept.table, project, tree, &kept.id)?;
            let now = self
                .seen(kept.table, project, tree, &kept.id)?
                .filter(|s| !s.removed)
                .and_then(|s| s.doc());
            // The base of the row right after the commit: what main held when the branch copied it.
            let based = match &kept.after {
                Some(next) => next.as_ref().and_then(base_of),
                None => own.as_ref().and_then(|o| o.base.clone()),
            };
            let before = seen(kept.table, &kept.id, kept.before.as_ref(), based.as_deref())?;
            let after = match &kept.after {
                Some(next) => seen(kept.table, &kept.id, next.as_ref(), None)?,
                None => now.clone(),
            };
            let section = kept.table == Table::Sections;
            let outcome =
                super::merge::three_way(section, after.as_ref(), before.as_ref(), now.as_ref());
            let kind = (!section)
                .then(|| {
                    [&before, &after, &now]
                        .into_iter()
                        .flatten()
                        .find_map(|d| d.get("kind").and_then(Value::as_str).map(str::to_string))
                })
                .flatten();
            for (unit, label, base, source, target) in outcome.conflicts {
                conflicts.push(UndoConflict {
                    table: kept.table,
                    id: kept.id.clone(),
                    kind: kind.clone(),
                    unit,
                    label,
                    before: source,
                    after: base,
                    now: target,
                    later: kept.later.clone(),
                });
            }
            if outcome.changes {
                undoing.push(Undoing {
                    table: kept.table,
                    id: kept.id.clone(),
                    result: outcome.result,
                    now,
                });
            }
        }
        if !conflicts.is_empty() {
            return Ok((commit, Err(conflicts)));
        }
        if undoing.is_empty() {
            return Err(text!("history.undo.already", seq));
        }
        Ok((commit, Ok(undoing)))
    }

    /// The commit `seq` of the tree undone alone, git's revert of one commit (`specboard://SpecBoard/Docs/Design:DesignBranches`
    /// §7): what it changed goes back unit by unit, what later commits changed stays, and any unit both
    /// changed refuses the whole of it, nothing written. In a branch or the board, written as one commit
    /// of its own (`revert`), the commit undone marked; main is undone through a branch
    /// (`undo_one_into_branch`). `asked`: a person allowed it on the window, said in the line.
    pub fn undo_one(
        &mut self,
        project: &str,
        tree: &str,
        seq: i64,
        only_by: Option<&Author>,
        asked: bool,
    ) -> Result<UndoOne, String> {
        let (commit, work) = self.undo_work(project, tree, seq, only_by)?;
        let undoing = match work {
            Ok(rows) => rows,
            Err(conflicts) => return Ok(UndoOne::Conflicts(conflicts)),
        };
        let hidden = self.hidden(project, tree)?;
        let names = self.paths_before(project, tree, seq)?;
        // Out of the way first, so names that change hands within the commit do not meet: a row of
        // the tree's own that comes back renamed is set apart by its id until it is written.
        for row in &undoing {
            if row.result.is_some() && self.own(row.table, project, tree, &row.id)?.is_some() {
                self.history
                    .before(&self.tx, row.table, project, tree, &row.id)?;
                self.tx
                    .execute(
                        &format!(
                            "UPDATE {} SET name = name || char(1) || id WHERE project_id = ? AND branch_id = ? AND id = ?",
                            row.table.name()
                        ),
                        params![project, tree, row.id],
                    )
                    .map_err(db)?;
            }
        }
        // Gone first, then made and changed: a section before what is in it.
        for row in undoing.iter().filter(|r| r.result.is_none()) {
            let gone = match row.table {
                Table::Cards => self.remove_card(project, tree, &row.id)?,
                Table::Sections => self.remove_section(project, tree, &row.id)?,
            };
            debug_assert!(gone || row.now.is_none());
        }
        let mut puts: Vec<&Undoing> = undoing.iter().filter(|r| r.result.is_some()).collect();
        puts.sort_by_key(|r| r.table != Table::Sections);
        for row in &puts {
            let doc = row.result.as_ref().expect("a document");
            let path = names
                .get(&row.id)
                .cloned()
                .unwrap_or_else(|| row.id.clone());
            let written = match row.table {
                Table::Cards => self.put_card(project, tree, doc),
                Table::Sections => self.put_section(project, tree, doc),
            };
            written.map_err(|e| {
                if e == text!("write.nameTaken") {
                    text!("history.undo.taken", path)
                } else {
                    e
                }
            })?;
        }
        self.undo_holds(project, tree, &puts, &names)?;
        self.undo_overlaps(project, tree, &hidden, &names)?;
        self.tx
            .execute("UPDATE commits SET reverted = 1 WHERE id = ?", [&commit.id])
            .map_err(db)?;
        self.mark(project, tree, "revert");
        let who = author_said(&commit.author_kind, commit.author.as_deref());
        let said = if asked {
            keyed!("history.undo.lineAsked", seq, who)
        } else {
            keyed!("history.undo.line", seq, who)
        };
        self.line(project, tree, "revert", "project", None, said)?;
        Ok(UndoOne::Done {
            changed: undoing.iter().map(|r| (r.table, r.id.clone())).collect(),
            made: None,
        })
    }

    /// main's commit `seq` undone alone as a branch rather than written into main (`undo_one` for
    /// main): what undoing it comes to, written over main as it is now in a new branch (`name`, else
    /// `undo-<seq>`) for a merge to take in — through a merge request while main is locked, and it is
    /// made while locked too. main is not touched nor its commit marked. The branches the commit set
    /// aside, when it is a merge, are named on the branch, to come back when it is merged.
    pub fn undo_one_into_branch(
        &mut self,
        project: &str,
        seq: i64,
        name: Option<&str>,
        only_by: Option<&Author>,
        asked: bool,
    ) -> Result<UndoOne, String> {
        let (commit, work) = self.undo_work(project, "", seq, only_by)?;
        let undoing = match work {
            Ok(rows) => rows,
            Err(conflicts) => return Ok(UndoOne::Conflicts(conflicts)),
        };
        let names = self.paths_before(project, "", seq)?;
        let (id, named) = self.branch_for(project, name, &format!("undo-{seq}"))?;
        for row in &undoing {
            let now = row
                .now
                .as_ref()
                .map(|d| Value::Object(d.clone()).to_string());
            match &row.result {
                Some(doc) => {
                    let keys = row.table.keys(doc);
                    let text = Value::Object(doc.clone()).to_string();
                    self.upsert(
                        row.table,
                        project,
                        &id,
                        &row.id,
                        keys,
                        false,
                        Some(text),
                        now,
                    )
                    .map_err(|e| {
                        if e == text!("write.nameTaken") {
                            text!(
                                "history.undo.taken",
                                path = names.get(&row.id).cloned().unwrap_or_default()
                            )
                        } else {
                            e
                        }
                    })?;
                }
                None => {
                    let Some(now_doc) = &row.now else {
                        continue;
                    };
                    let keys = row.table.keys(now_doc);
                    self.upsert(row.table, project, &id, &row.id, keys, true, None, now)?;
                }
            }
        }
        let puts: Vec<&Undoing> = undoing.iter().filter(|r| r.result.is_some()).collect();
        self.undo_holds(project, &id, &puts, &names)?;
        self.undo_overlaps(project, &id, &HashSet::new(), &names)?;
        self.asides_of(project, &id, std::slice::from_ref(&commit.id))?;
        self.mark(project, &id, "revert");
        let who = author_said(&commit.author_kind, commit.author.as_deref());
        let said = if asked {
            keyed!("history.undoIntoBranch.lineAsked", seq, who)
        } else {
            keyed!("history.undoIntoBranch.line", seq, who)
        };
        self.line(project, &id, "revert", "project", None, said)?;
        Ok(UndoOne::Done {
            changed: undoing.iter().map(|r| (r.table, r.id.clone())).collect(),
            made: Some((id, named)),
        })
    }

    /// The paths of the tree's cards and sections as they were right before its commit `seq`, by id —
    /// each row the commits since wrote as the first of them kept it — over their paths now: what an
    /// undo names what it could not bring back by.
    pub(crate) fn paths_before(
        &self,
        project: &str,
        tree: &str,
        seq: i64,
    ) -> Result<HashMap<String, String>, String> {
        let mut rows: Vec<crate::store::OverRow> = Vec::new();
        {
            let mut stmt = self
                .tx
                .prepare(
                    "SELECT r.tbl, r.row_id, r.before FROM commit_rows r JOIN commits c ON c.id = r.commit_id
                      WHERE c.project_id = ? AND c.branch_id = ? AND c.seq >= ? ORDER BY c.seq, r.rowid",
                )
                .map_err(db)?;
            let kept = stmt
                .query_map(params![project, tree, seq], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, Option<String>>(2)?,
                    ))
                })
                .map_err(db)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(db)?;
            let mut seen: HashSet<(String, String)> = HashSet::new();
            for (table, id, before) in kept {
                if seen.insert((table.clone(), id.clone())) {
                    rows.push((table, id, crate::store::parse(before.as_deref())));
                }
            }
        }
        let mut out = paths_over(&self.reader(), project, tree, &[])?;
        for (id, path) in paths_over(&self.reader(), project, tree, &rows)? {
            out.insert(id, path);
        }
        Ok(out)
    }

    /// The ids main's cards and sections the tree hides now, its own being at their paths.
    fn hidden(&self, project: &str, tree: &str) -> Result<HashSet<String>, String> {
        if !is_branch(tree) {
            return Ok(HashSet::new());
        }
        Ok(self
            .reader()
            .compose(project, Some(tree))?
            .hidden
            .into_iter()
            .collect())
    }

    /// Refused when what an undo brought back now hides another card or section of main's at its
    /// path — one the tree did not hide before (`hidden`); main and the board refuse it as they write.
    fn undo_overlaps(
        &self,
        project: &str,
        tree: &str,
        hidden: &HashSet<String>,
        names: &HashMap<String, String>,
    ) -> Result<(), String> {
        for id in self.hidden(project, tree)? {
            if !hidden.contains(&id) {
                let path = names.get(&id).cloned().unwrap_or(id);
                return Err(text!("history.undo.taken", path));
            }
        }
        Ok(())
    }

    /// Refused when a card or a section an undo brought back has nothing to be in any more: its
    /// section, the container it is inside, or its parent section gone from the tree since.
    fn undo_holds(
        &self,
        project: &str,
        tree: &str,
        puts: &[&Undoing],
        names: &HashMap<String, String>,
    ) -> Result<(), String> {
        for row in puts {
            let Some(doc) = &row.result else {
                continue;
            };
            let text = |key: &str| {
                doc.get(key)
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
            };
            let holders: Vec<(Table, String)> = match row.table {
                Table::Cards => [
                    text("section").map(|s| (Table::Sections, s)),
                    text("in").map(|c| (Table::Cards, c)),
                ]
                .into_iter()
                .flatten()
                .collect(),
                Table::Sections => text("parent_id")
                    .map(|p| (Table::Sections, p))
                    .into_iter()
                    .collect(),
            };
            for (table, holder) in holders {
                let there = self
                    .seen(table, project, tree, &holder)?
                    .is_some_and(|s| !s.removed);
                if !there {
                    let path = names.get(&row.id).cloned().unwrap_or(row.id.clone());
                    let holder = names.get(&holder).cloned().unwrap_or(holder);
                    return Err(text!("history.undo.holderGone", path, holder));
                }
            }
        }
        Ok(())
    }

    /// A new branch for what goes back into main: `name` as given, else `wanted` numbered until it is
    /// free. Its id and its name.
    fn branch_for(
        &mut self,
        project: &str,
        name: Option<&str>,
        wanted: &str,
    ) -> Result<(String, String), String> {
        let wanted = name
            .map(str::trim)
            .filter(|n| !n.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| wanted.to_string());
        let mut free = wanted.clone();
        let mut n = 2;
        while name.is_none() && self.branch_named(project, &free)?.is_some() {
            free = format!("{wanted} ({n})");
            n += 1;
        }
        let id = self.create_branch(project, &free)?;
        let named: String = self
            .tx
            .query_row("SELECT name FROM branches WHERE id = ?", [&id], |r| {
                r.get(0)
            })
            .map_err(db)?;
        Ok((id, named))
    }

    /// The branches the merges among `commits` set aside, named on the branch `id` that undoes them:
    /// they come back when it is merged (`bring_back`).
    fn asides_of(&mut self, project: &str, id: &str, commits: &[String]) -> Result<(), String> {
        let mut asides: Vec<String> = Vec::new();
        {
            let mut stmt = self
                .tx
                .prepare(
                    "SELECT l.target_id FROM commit_lines l JOIN branches b ON b.id = l.target_id
                     WHERE l.commit_id = ? AND l.kind = 'branch.merge' AND b.project_id = ? AND b.archived_at IS NOT NULL",
                )
                .map_err(db)?;
            for commit in commits {
                let found: Vec<String> = stmt
                    .query_map(params![commit, project], |r| r.get(0))
                    .map_err(db)?
                    .collect::<Result<_, _>>()
                    .map_err(db)?;
                for f in found {
                    if !asides.contains(&f) {
                        asides.push(f);
                    }
                }
            }
        }
        if !asides.is_empty() {
            self.tx
                .execute(
                    "UPDATE branches SET reverts = ? WHERE id = ?",
                    params![Value::from(asides).to_string(), id],
                )
                .map_err(db)?;
        }
        Ok(())
    }
}

/// The paths of a tree's cards and sections by id, the tree composed with `rows` in place of its own.
pub(crate) fn paths_over(
    reader: &crate::store::Reader,
    project: &str,
    tree: &str,
    rows: &[crate::store::OverRow],
) -> Result<HashMap<String, String>, String> {
    let branch = (!tree.is_empty()).then_some(tree);
    let made = reader.compose_over(project, branch, Some(&crate::store::Over { tree, rows }))?;
    let Some(row) = reader.project(project)? else {
        return Ok(HashMap::new());
    };
    let detail = crate::model::typed(&crate::detail::of_made(&row, &made))?;
    let named = crate::export::Project::new(&detail);
    let mut out: HashMap<String, String> = detail
        .sections
        .iter()
        .map(|s| (s.id.clone(), named.directory.full_section(&s.id)))
        .collect();
    for card in detail.schemas.iter().chain(detail.inner.iter()) {
        out.insert(card.id.clone(), named.full(card));
    }
    Ok(out)
}

fn exists_seq(conn: &Connection, project: &str, tree: &str, seq: i64) -> Result<bool, String> {
    conn.query_row(
        "SELECT 1 FROM commits WHERE project_id = ? AND branch_id = ? AND seq = ?",
        params![project, tree, seq],
        |_| Ok(()),
    )
    .optional()
    .map(|found| found.is_some())
    .map_err(db)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_kept_before_lines_had_keys_is_said_in_the_readers_language_as_the_newer_are() {
        let old = keyed!("create.line.created", full = "Docs:Rules");
        let kept = old.render_in(KEPT_IN);
        let _en = crate::text::bind(Some("en"));
        let newer = line_said(
            String::new(),
            Some("create.line.created".into()),
            Some(old.values_json().to_string()),
        );
        assert_eq!(line_said(kept.clone(), None, None), newer);
        // A key the tables no longer have: its sentence read back all the same.
        assert_eq!(line_said(kept, Some("gone.key".into()), None), newer);
        // What no table makes stays as it was written.
        assert_eq!(line_said("手で書いた行".into(), None, None), "手で書いた行");
    }
}
