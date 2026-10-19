//! Writing: a project's documents written straight, one transaction at a time, each
//! leaving a commit in the history of every tree it wrote (`specboard://SpecBoard/Docs/Design:DesignBranches` §7).
//!
//! The core keeps no working tables: a write puts a card's or a section's whole document
//! where the tree keeps it. main and the board keep their own rows; a branch keeps only
//! what it changed — a copy of main's document, with what main held as its base, the first
//! time it writes one of main's cards (§11 "書き方"), and a mark where it removed one.
//! What each row was before the transaction first wrote it goes in the commit, which is
//! what going back (`revert`) puts back.
//!
//! What sits apart from the documents — where boxes sit on a diagram (`places`) and how
//! cards look (`card_looks`) — is written too, but makes no commit.
//!
//! main's lock lets through only how main is arranged and tagged (`specboard://SpecBoard/Docs/Design:DesignBranches`
//! §8): a card's own box, its size and how it looks, a flow's nodes, and in a card's
//! document its place among the cards (`order`) and its tags (`fields.tags`). Everything
//! else of main's — the rest of a card, a section whole, and what sits on a section's
//! diagram for another section — changes only by a merge.
//! An archived project refuses every write, whichever way it comes (`guard`).

mod branches;
mod bundle;
mod history;
mod holds;
mod merge;
mod projectfile;

pub use branches::{GROUP_BRANCH, is_branch_tree, parsed, valid_name, valid_section_name};
pub use bundle::{bundle, other_form};
pub use history::{
    Around, Author, Commit, Line, UndoConflict, UndoOne, around as history_around, author_said,
    commit_at as history_commit, commit_count as history_count, commits as history_of,
    commits_before as history_before,
};
pub(crate) use history::{insert_line, paths_over};
pub use holds::{Batch, Hold, batch, holds as holds_on};
pub use merge::{
    Change as MergeChange, Conflict as MergeConflict, Preview as MergePreview, Side as MergeSide,
    Take as MergeTake, UnitChange, WHOLE as MergeWhole, places_from_main as merge_places_from_main,
    unit_changes,
};
pub(crate) use projectfile::free_slug;
pub use projectfile::{Landing, peek};

use crate::store::Store;
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};
use serde_json::{Map, Value};
use std::any::Any;
use std::cell::RefCell;
use std::collections::BTreeSet;
use std::rc::Rc;

/// What an archived project's writes are told.
pub fn archived() -> String {
    text!("write.archived")
}

/// What the guards raise for an archived project's writes: the key of `archived`, said in the
/// language of the write that meets it (the guards are made once per connection).
const ARCHIVED_RAISED: &str = "write.archived";

/// What a write to a locked main is told (`specboard://SpecBoard/Docs/Design:DesignBranches` §8).
pub fn locked_main() -> String {
    text!("write.lockedMain")
}

/// Who writes: the window, or MCP (with the access key's name, when it has one).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Who {
    pub side: Side,
    pub name: Option<String>,
    /// The window's: the Mac it is on, as the agent's core is told it (`client` in a request the
    /// agent hands on). Empty for the
    /// app that holds the store itself, and for MCP. The windows of two Macs are two hands: what
    /// one holds, the other waits for.
    pub client: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Screen,
    Mcp,
}

impl Side {
    pub fn word(self) -> &'static str {
        match self {
            Side::Screen => "screen",
            Side::Mcp => "mcp",
        }
    }
}

impl Who {
    pub fn screen() -> Who {
        Who::screen_of("")
    }

    /// The window of the Mac `client`.
    pub fn screen_of(client: &str) -> Who {
        Who {
            side: Side::Screen,
            name: None,
            client: client.to_string(),
        }
    }

    /// The window that made `request`: of the Mac its `client` names, when the agent handed it on.
    pub fn screen_for(request: &serde_json::Value) -> Who {
        Who::screen_of(
            request
                .get("client")
                .and_then(serde_json::Value::as_str)
                .unwrap_or(""),
        )
    }

    pub fn mcp(name: Option<&str>) -> Who {
        Who {
            side: Side::Mcp,
            name: name.map(str::to_string),
            client: String::new(),
        }
    }
}

/// A database error from a write: a guard's refusal as it is said, anything else as what
/// it is.
pub(crate) fn db(e: rusqlite::Error) -> String {
    if let rusqlite::Error::SqliteFailure(failure, Some(message)) = &e
        && failure.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_TRIGGER
    {
        if message == ARCHIVED_RAISED {
            return archived();
        }
        return message.clone();
    }
    // The database's own words name its columns, not what was written.
    if name_taken(&e) {
        return text!("write.nameTaken");
    }
    text!("write.dbWrite", e)
}

/// Whether a write was refused because a card or section already holds that name where it
/// goes (`card_docs_name`, `section_docs_name`; SQLite names the first by its columns, the
/// second, an index on an expression, by the index).
pub(crate) fn name_taken(e: &rusqlite::Error) -> bool {
    matches!(
        e,
        rusqlite::Error::SqliteFailure(failure, Some(message))
            if failure.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_UNIQUE
                && (message.contains("card_docs") || message.contains("section_docs"))
    )
}

/// The guards every connection keeps (TEMP triggers): what an archived project holds is
/// never written, whatever writes it. Taking a project out of the archive (and deleting one,
/// which takes it out first) passes.
pub(crate) fn guard(conn: &Connection) -> Result<(), String> {
    let archived = |project: &str| {
        format!("(SELECT archived_at FROM main.projects WHERE id = {project}) IS NOT NULL")
    };
    let said = ARCHIVED_RAISED;
    type Project = fn(&str) -> String;
    let tables: [(&str, Project); 8] = [
        ("card_docs", |row| format!("{row}.project_id")),
        ("inspection_marks", |row| format!("{row}.project_id")),
        ("code_marks", |row| format!("{row}.project_id")),
        ("section_docs", |row| format!("{row}.project_id")),
        ("places", |row| format!("{row}.project_id")),
        ("card_looks", |row| format!("{row}.project_id")),
        ("branches", |row| format!("{row}.project_id")),
        ("branch_prints", |row| {
            format!("(SELECT project_id FROM main.branches WHERE id = {row}.branch_id)")
        }),
    ];
    let mut sql = String::new();
    for (table, project) in tables {
        for (event, row) in [("INSERT", "NEW"), ("UPDATE", "OLD"), ("DELETE", "OLD")] {
            sql += &format!(
                "CREATE TEMP TRIGGER IF NOT EXISTS archived_{table}_{e} BEFORE {event} ON main.{table} \
                 WHEN {when} BEGIN SELECT RAISE(ABORT, '{said}'); END;\n",
                e = event.to_lowercase(),
                when = archived(&project(row)),
            );
        }
    }
    sql += &format!(
        "CREATE TEMP TRIGGER IF NOT EXISTS archived_projects_update \
         BEFORE UPDATE OF name, slug, description, tag_groups, rules, export_dirs, locked_at, kinds, ai_merge, mcp_writes, exports ON main.projects \
         WHEN OLD.archived_at IS NOT NULL AND NEW.archived_at IS NOT NULL \
         BEGIN SELECT RAISE(ABORT, '{said}'); END;\n"
    );
    conn.execute_batch(&sql).map_err(db)
}

/// Now, as every row of the store writes it (`Repository.timestamp`).
pub(crate) fn now(conn: &Connection) -> Result<String, String> {
    conn.query_row("SELECT strftime('%Y-%m-%dT%H:%M:%fZ', 'now')", [], |r| {
        r.get(0)
    })
    .map_err(db)
}

/// A new id: a random UUID, lowercased (`Repository.newId`).
pub fn new_id() -> String {
    let mut b = [0u8; 16];
    getrandom::fill(&mut b).expect("the system's random numbers");
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let hex: String = b.iter().map(|x| format!("{x:02x}")).collect();
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

/// A project's slug from its name (`slugify`): lowercased, letters and digits kept, every
/// run of anything else one dash between them.
pub fn slugify(name: &str) -> String {
    let mut out = String::new();
    let mut dash = false;
    for c in name.trim().to_lowercase().chars() {
        if c.is_alphabetic() || c.is_numeric() {
            if dash && !out.is_empty() {
                out.push('-');
            }
            dash = false;
            out.push(c);
        } else {
            dash = true;
        }
    }
    if out.is_empty() {
        "project".to_string()
    } else {
        out
    }
}

/// A slug read from elsewhere (a project file, a bundle) as one this store keeps: as it is when it
/// names one folder — not empty, no `/` nor `\`, not starting with `.`, no control character, as the
/// folders an export keeps its records in and the app keeps a moved project's file in are named by it
/// — else made again as a new project's is (`slugify`).
pub fn safe_slug(raw: &str) -> String {
    let fine = !raw.trim().is_empty()
        && raw.trim() == raw
        && !raw.starts_with('.')
        && !raw.chars().any(|c| c == '/' || c == '\\' || c.is_control());
    if fine { raw.to_string() } else { slugify(raw) }
}

/// A card's document less how it is arranged and tagged — its place among its section's
/// cards (`order`) and its tags (`fields.tags`) —, what main's lock lets change.
fn unarranged(doc: &Map<String, Value>) -> Map<String, Value> {
    let mut doc = doc.clone();
    doc.remove("order");
    if let Some(Value::Object(fields)) = doc.get_mut("fields") {
        fields.remove("tags");
        if fields.is_empty() {
            doc.remove("fields");
        }
    }
    doc
}

/// Whether a tree is a branch: neither main (`""`) nor the board, which keep their own.
fn is_branch(tree: &str) -> bool {
    !tree.is_empty() && !crate::store::is_own_tree(tree)
}

/// The two tables of documents.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Table {
    Cards,
    Sections,
}

impl Table {
    pub fn name(self) -> &'static str {
        match self {
            Table::Cards => "card_docs",
            Table::Sections => "section_docs",
        }
    }

    fn of(name: &str) -> Option<Table> {
        match name {
            "card_docs" => Some(Table::Cards),
            "section_docs" => Some(Table::Sections),
            _ => None,
        }
    }

    /// The columns a row is kept by in a commit, and put back by (`History.columns`).
    fn columns(self) -> &'static [&'static str] {
        match self {
            Table::Cards => &[
                "section_id",
                "name",
                "kind",
                "removed",
                "doc",
                "base",
                "based_at",
            ],
            Table::Sections => &["parent_id", "name", "removed", "doc", "base", "based_at"],
        }
    }

    /// The key columns of a document: where it sits and its name (and a card's kind).
    fn keys(self, doc: &Map<String, Value>) -> Vec<(&'static str, Value)> {
        let text = |key: &str| doc.get(key).and_then(Value::as_str).unwrap_or_default();
        match self {
            Table::Cards => vec![
                ("section_id", holder(doc).into()),
                ("name", text("name").into()),
                ("kind", text("kind").into()),
            ],
            Table::Sections => vec![
                (
                    "parent_id",
                    doc.get("parent_id")
                        .and_then(Value::as_str)
                        .map_or(Value::Null, Value::from),
                ),
                ("name", text("name").into()),
            ],
        }
    }
}

/// Where a card's name is its own: the container it is inside (`in`, a card's id), else its
/// section. Names are one each there (`card_docs.section_id` and its unique index), so a
/// flow's nodes and a goal's cards are named apart from the section's.
pub(crate) fn holder(doc: &Map<String, Value>) -> &str {
    doc.get("in")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .or_else(|| doc.get("section").and_then(Value::as_str))
        .unwrap_or_default()
}

/// A row as a tree sees it: its own, or main's beneath a branch.
#[derive(Clone, Debug)]
pub struct Seen {
    /// The tree's own row (not main's beneath it).
    pub own: bool,
    pub removed: bool,
    pub doc: Option<String>,
    /// What main held when the branch first wrote it; none for main's own and for what
    /// the branch made.
    pub base: Option<String>,
}

impl Seen {
    pub fn doc(&self) -> Option<Map<String, Value>> {
        crate::store::parse(self.doc.as_deref())
    }
}

/// A box on a diagram: where it sits, its size, and whether it was put there by hand.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Spot {
    pub x: Option<i64>,
    pub y: Option<i64>,
    pub w: Option<i64>,
    pub h: Option<i64>,
    pub pinned: bool,
}

/// One transaction of writes (`Store::write`). Commits with `commit`; dropped, it is
/// undone with all it gathered.
pub struct Tx<'s> {
    tx: Transaction<'s>,
    who: Who,
    history: history::Gathered,
    /// Every tree written, for the notices once it commits.
    touched: BTreeSet<(String, String)>,
    /// A merge writes main's documents under the lock.
    merging: bool,
    /// What is done outside the store once this commits (files moved beside what was written out),
    /// in order; dropped undone with the rest.
    after: Vec<Box<dyn FnOnce() + Send>>,
    /// What was last worked out from the documents as they stood, kept while no row changes
    /// but those it can be brought up to (`remembered_or_brought_up`).
    memo: RefCell<Option<Memo>>,
    /// The writes made since `memo` was worked out that it may be brought up to, each with how
    /// many rows it changed (`Written`).
    since: RefCell<Vec<(u64, Written)>>,
    /// The lines to a working directory's file a card's write made or pointed elsewhere: their code
    /// is recorded once this commits (`tools::code_review::record_written`), outside it, so
    /// reading the code never holds the write.
    code_lines: Vec<FileLine>,
}

/// A card's line to a working directory's file, as a write left it: the project, the line's id
/// and kind, the file's id and where in it (`at`, none for the file itself).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileLine {
    pub project: String,
    pub line_id: String,
    pub kind: String,
    pub file: String,
    pub at: Option<String>,
}

/// A card's document's lines to files, by their ids: each with its kind, file and where in it.
fn file_lines(doc: &Map<String, Value>) -> Vec<(String, String, String, Option<String>)> {
    doc.get("connections")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|line| {
            let to = line.get("to")?.as_str()?;
            if !to.starts_with("file:") {
                return None;
            }
            let id = line.get("id")?.as_str()?.to_string();
            let kind = line.get("line")?.as_str()?.to_string();
            let at = line
                .get("at")
                .and_then(Value::as_str)
                .filter(|a| !a.is_empty())
                .map(str::to_string);
            Some((id, kind, to.to_string(), at))
        })
        .collect()
}

/// One thing worked out from the documents (`Tx::remembered_or_brought_up`): by what it was asked
/// for, and how many rows this connection had changed when it was.
struct Memo {
    key: String,
    changes: u64,
    value: Rc<dyn Any>,
}

/// A write a transaction keeps account of, for what it worked out before to be brought up to it
/// instead of worked out again (`Tx::remembered_or_brought_up`). Any other write — or one of these undone — has it worked out again.
#[derive(Clone, Debug)]
pub(crate) enum Written {
    /// A card's document written (`put_card`): as the tree saw it before (none: made now), and as
    /// it is now.
    Card {
        project: String,
        tree: String,
        id: String,
        before: Option<Map<String, Value>>,
        after: Map<String, Value>,
    },
    /// A card gone from the tree (`remove_card`), with where it sat and how it looked: its
    /// document as the tree saw it before.
    Removed {
        project: String,
        tree: String,
        id: String,
        before: Map<String, Value>,
    },
    /// A project marked as written (`line`): when.
    Marked { project: String, at: String },
}

impl Store {
    /// A transaction of writes by `who`.
    pub fn write(&mut self, who: Who) -> Result<Tx<'_>, String> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(db)?;
        Ok(Tx {
            tx,
            who,
            history: history::Gathered::default(),
            touched: BTreeSet::new(),
            merging: false,
            after: Vec::new(),
            memo: RefCell::new(None),
            since: RefCell::new(Vec::new()),
            code_lines: Vec::new(),
        })
    }
}

impl<'s> Tx<'s> {
    pub fn conn(&self) -> &Connection {
        &self.tx
    }

    /// The documents as this transaction sees them, what it has written included.
    pub fn reader(&self) -> crate::store::Reader<'_> {
        crate::store::Reader { conn: &self.tx }
    }

    /// What `work` makes of the documents, kept for `key` until a row changes: asked for again with
    /// the same `key` and no row changed through this connection since (`total_changes` counts
    /// every insert, update and delete it ran — a trigger's and an undone one's too — so what is
    /// written is never answered from before), the one made then is given back and `work` is not
    /// run. One thing is kept: asked for another `key`, it takes the place. When every row changed
    /// since was changed by a write this keeps account of (`Written`, in order), what was made then
    /// is brought up to them by `bring` instead of made again — when nothing else holds it, and
    /// `bring` can (false: it is made again).
    pub(crate) fn remembered_or_brought_up<T: Any>(
        &self,
        key: &str,
        work: impl FnOnce() -> Result<T, String>,
        bring: impl FnOnce(&mut T, &[Written]) -> bool,
    ) -> Result<Rc<T>, String> {
        let changes = self.tx.total_changes();
        let kept = self.memo.borrow_mut().take();
        if let Some(memo) = kept
            && memo.key == key
            && let Ok(mut value) = memo.value.downcast::<T>()
        {
            if memo.changes == changes {
                *self.memo.borrow_mut() = Some(Memo {
                    key: memo.key,
                    changes,
                    value: value.clone(),
                });
                return Ok(value);
            }
            let since: Vec<Written> = {
                let since = self.since.borrow();
                let accounted: u64 = since.iter().map(|(n, _)| n).sum();
                if memo.changes + accounted == changes {
                    since.iter().map(|(_, w)| w.clone()).collect()
                } else {
                    Vec::new()
                }
            };
            if !since.is_empty()
                && let Some(held) = Rc::get_mut(&mut value)
                && bring(held, &since)
            {
                self.since.borrow_mut().clear();
                *self.memo.borrow_mut() = Some(Memo {
                    key: memo.key,
                    changes,
                    value: value.clone(),
                });
                return Ok(value);
            }
        }
        self.since.borrow_mut().clear();
        let value = Rc::new(work()?);
        // Kept only if `work` changed no row (it reads; one that wrote is not trusted to have seen
        // what it left).
        if self.tx.total_changes() == changes {
            *self.memo.borrow_mut() = Some(Memo {
                key: key.to_string(),
                changes,
                value: value.clone(),
            });
        }
        self.since.borrow_mut().clear();
        Ok(value)
    }

    /// A write made, kept account of for what was worked out before (`Written`): `rows`, how many
    /// rows it changed. Nothing is kept while nothing is remembered.
    fn kept_account(&self, rows: u64, written: impl FnOnce() -> Written) {
        if rows > 0 && self.memo.borrow().is_some() {
            self.since.borrow_mut().push((rows, written()));
        }
    }

    pub fn who(&self) -> &Who {
        &self.who
    }

    /// From here on, main's documents are written by a merge: the lock lets them through.
    pub fn merging(&mut self) {
        self.merging = true;
    }

    pub fn now(&self) -> Result<String, String> {
        now(&self.tx)
    }

    /// The trees this transaction wrote, as (project, tree).
    pub fn touched(&self) -> &BTreeSet<(String, String)> {
        &self.touched
    }

    fn touch(&mut self, project: &str, tree: &str) {
        self.touched.insert((project.to_string(), tree.to_string()));
    }

    /// A tree told of once this commits, though nothing in it was written: what is kept beside
    /// it changed (a project's merge requests, under main: a window would show the one withdrawn
    /// until it read the project again).
    pub fn tell_of(&mut self, project: &str, tree: &str) {
        self.touch(project, tree);
    }

    /// Something done outside the store once this commits, and not at all if it is undone (a batch
    /// tried and let go, a later step refused).
    pub fn after_commit(&mut self, then: impl FnOnce() + Send + 'static) {
        self.after.push(Box::new(then));
    }

    /// When main was locked; none when it is not.
    pub fn locked_at(&self, project: &str) -> Result<Option<String>, String> {
        Ok(self
            .tx
            .query_row(
                "SELECT locked_at FROM projects WHERE id = ?",
                [project],
                |r| r.get::<_, Option<String>>(0),
            )
            .optional()
            .map_err(db)?
            .flatten())
    }

    /// Whether main's lock holds a write to the tree: the tree is main, main is locked,
    /// and this is not a merge.
    fn under_lock(&self, project: &str, tree: &str) -> Result<bool, String> {
        Ok(tree.is_empty() && !self.merging && self.locked_at(project)?.is_some())
    }

    /// Refused when main's lock holds the tree.
    fn refuse_locked(&self, project: &str, tree: &str) -> Result<(), String> {
        if self.under_lock(project, tree)? {
            return Err(locked_main());
        }
        Ok(())
    }

    /// The tree's own row.
    fn own(
        &self,
        table: Table,
        project: &str,
        tree: &str,
        id: &str,
    ) -> Result<Option<Seen>, String> {
        self.tx
            .query_row(
                &format!(
                    "SELECT removed, doc, base FROM {} WHERE project_id = ? AND branch_id = ? AND id = ?",
                    table.name()
                ),
                params![project, tree, id],
                |r| {
                    Ok(Seen {
                        own: true,
                        removed: r.get::<_, i64>(0)? != 0,
                        doc: r.get(1)?,
                        base: r.get(2)?,
                    })
                },
            )
            .optional()
            .map_err(db)
    }

    /// A card or a section as the tree sees it: its own row, or main's beneath a branch.
    pub fn seen(
        &self,
        table: Table,
        project: &str,
        tree: &str,
        id: &str,
    ) -> Result<Option<Seen>, String> {
        if let Some(own) = self.own(table, project, tree, id)? {
            return Ok(Some(own));
        }
        if !is_branch(tree) {
            return Ok(None);
        }
        // main as the branch was made, where main has changed it since (`branch_frozen`).
        if let Some(kept) = self.frozen(table, project, tree, id)? {
            return Ok(kept.map(|doc| Seen {
                own: false,
                removed: false,
                doc: Some(doc),
                base: None,
            }));
        }
        Ok(self
            .own(table, project, "", id)?
            .filter(|s| !s.removed)
            .map(|s| Seen {
                own: false,
                removed: false,
                doc: s.doc,
                base: None,
            }))
    }

    /// What a branch keeps of main's row as main held it when the branch was made, main having
    /// changed it since: `Some(None)` for one main made since, none when main has not changed it.
    fn frozen(
        &self,
        table: Table,
        project: &str,
        tree: &str,
        id: &str,
    ) -> Result<Option<Option<String>>, String> {
        self.tx
            .query_row(
                "SELECT doc FROM branch_frozen WHERE project_id = ? AND branch_id = ? AND tbl = ? AND id = ?",
                params![project, tree, table.name(), id],
                |r| r.get::<_, Option<String>>(0),
            )
            .optional()
            .map_err(db)
    }

    /// main's row as it is, kept for each branch that has not written it and keeps nothing of
    /// it yet, before main writes it: a branch sees main as it was made until it takes main
    /// in. Not for a branch set aside.
    pub(crate) fn freeze(&mut self, table: Table, project: &str, id: &str) -> Result<(), String> {
        let name = table.name();
        self.tx
            .execute(
                &format!(
                    "INSERT OR IGNORE INTO branch_frozen (project_id, branch_id, tbl, id, doc)
                     SELECT ?1, b.id, ?2, ?3,
                            (SELECT d.doc FROM {name} d WHERE d.project_id = ?1 AND d.branch_id = '' AND d.id = ?3 AND d.removed = 0)
                     FROM branches b
                     WHERE b.project_id = ?1 AND b.archived_at IS NULL
                       AND NOT EXISTS (SELECT 1 FROM {name} o WHERE o.project_id = ?1 AND o.branch_id = b.id AND o.id = ?3)"
                ),
                params![project, name, id],
            )
            .map_err(db)?;
        Ok(())
    }

    /// What a branch keeps of main let go of: it sees main as it is now (it took main in, or it
    /// went into main).
    pub(crate) fn thaw(&mut self, project: &str, branch: &str) -> Result<(), String> {
        self.tx
            .execute(
                "DELETE FROM branch_frozen WHERE project_id = ? AND branch_id = ?",
                params![project, branch],
            )
            .map_err(db)?;
        Ok(())
    }

    /// A card's document written where the tree keeps it.
    pub fn put_card(
        &mut self,
        project: &str,
        tree: &str,
        doc: &Map<String, Value>,
    ) -> Result<(), String> {
        self.put(Table::Cards, project, tree, doc)
    }

    /// A section's document written where the tree keeps it.
    pub fn put_section(
        &mut self,
        project: &str,
        tree: &str,
        doc: &Map<String, Value>,
    ) -> Result<(), String> {
        self.put(Table::Sections, project, tree, doc)
    }

    fn put(
        &mut self,
        table: Table,
        project: &str,
        tree: &str,
        doc: &Map<String, Value>,
    ) -> Result<(), String> {
        let id = doc
            .get("id")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty())
            .ok_or_else(|| text!("write.docWithoutId"))?
            .to_string();
        let was = self.seen(table, project, tree, &id)?;
        let then = was.as_ref().filter(|w| !w.removed).and_then(Seen::doc);
        if then.as_ref() == Some(doc) {
            return Ok(());
        }
        if self.under_lock(project, tree)? {
            // A card of main's moves among its section's cards and has its tags put on and
            // taken off under the lock too; nothing else of it, and nothing of a section.
            let arranged = table == Table::Cards
                && then
                    .as_ref()
                    .is_some_and(|then| unarranged(then) == unarranged(doc));
            if !arranged {
                return Err(locked_main());
            }
        }
        // A copy keeps the base it was made with; one of main's becomes a copy now.
        let base = match &was {
            _ if !is_branch(tree) => None,
            Some(w) if !w.own => w.doc.clone(),
            Some(w) => w.base.clone(),
            None => None,
        };
        let text = Value::Object(doc.clone()).to_string();
        let rows = self.tx.total_changes();
        self.upsert(
            table,
            project,
            tree,
            &id,
            table.keys(doc),
            false,
            Some(text),
            base,
        )?;
        // A line to a file made, or pointed elsewhere: its code is recorded once this commits. A
        // merge carries the lines as they were, with what was recorded of them.
        if table == Table::Cards && !self.merging {
            let before = then.as_ref().map(file_lines).unwrap_or_default();
            for (line_id, kind, file, at) in file_lines(doc) {
                let same = before
                    .iter()
                    .any(|(i, _, f, a)| *i == line_id && *f == file && *a == at);
                if !same {
                    self.code_lines.push(FileLine {
                        project: project.to_string(),
                        line_id,
                        kind,
                        file,
                        at,
                    });
                }
            }
        }
        // A card's document: what was worked out before may be brought up to it.
        if table == Table::Cards {
            self.kept_account(self.tx.total_changes() - rows, || Written::Card {
                project: project.to_string(),
                tree: tree.to_string(),
                id,
                before: then,
                after: doc.clone(),
            });
        }
        Ok(())
    }

    /// A card gone from the tree: main's and the board's, and what a branch made, go;
    /// one of main's, or a branch's copy, stays as the branch's mark that it removed it.
    /// Where it sat and how it looked in the tree go with it, and so do the boxes on its
    /// own diagram (a flow's nodes, a goal's card map). False when it was not there.
    pub fn remove_card(&mut self, project: &str, tree: &str, id: &str) -> Result<bool, String> {
        let before = self
            .seen(Table::Cards, project, tree, id)?
            .filter(|w| !w.removed)
            .and_then(|w| w.doc());
        let rows = self.tx.total_changes();
        let gone = self.remove(Table::Cards, project, tree, id)?;
        if gone {
            self.tx
                .execute(
                    "DELETE FROM places WHERE project_id = ?1 AND branch_id = ?2 AND ((item_kind = 'card' AND (item = ?3 OR view = ?3)) OR (item_kind = 'node' AND view = ?3))",
                    params![project, tree, id],
                )
                .map_err(db)?;
            self.tx
                .execute(
                    "DELETE FROM card_looks WHERE project_id = ? AND branch_id = ? AND card_id = ?",
                    params![project, tree, id],
                )
                .map_err(db)?;
            // What was worked out before may be brought up to it going, its boxes and looks too.
            if let Some(before) = before {
                self.kept_account(self.tx.total_changes() - rows, || Written::Removed {
                    project: project.to_string(),
                    tree: tree.to_string(),
                    id: id.to_string(),
                    before,
                });
            }
        }
        Ok(gone)
    }

    /// A section gone from the tree, as a card goes (`remove_card`); its own diagram's
    /// boxes in the tree go with it. What is in it is the caller's to move or remove first.
    pub fn remove_section(&mut self, project: &str, tree: &str, id: &str) -> Result<bool, String> {
        let gone = self.remove(Table::Sections, project, tree, id)?;
        if gone {
            self.tx
                .execute(
                    "DELETE FROM places WHERE project_id = ?1 AND branch_id = ?2 AND (view = ?3 OR (item_kind = 'section' AND item = ?3))",
                    params![project, tree, id],
                )
                .map_err(db)?;
        }
        Ok(gone)
    }

    fn remove(
        &mut self,
        table: Table,
        project: &str,
        tree: &str,
        id: &str,
    ) -> Result<bool, String> {
        let Some(was) = self.seen(table, project, tree, id)?.filter(|w| !w.removed) else {
            return Ok(false);
        };
        self.refuse_locked(project, tree)?;
        if !is_branch(tree) || (was.own && was.base.is_none()) {
            if tree.is_empty() {
                self.freeze(table, project, id)?;
            }
            self.history.before(&self.tx, table, project, tree, id)?;
            self.tx
                .execute(
                    &format!(
                        "DELETE FROM {} WHERE project_id = ? AND branch_id = ? AND id = ?",
                        table.name()
                    ),
                    params![project, tree, id],
                )
                .map_err(db)?;
            self.touch(project, tree);
        } else {
            // The branch says it removed it, and what main held then — where it was.
            let base = if was.own { was.base } else { was.doc };
            let keys = table.keys(&crate::store::parse(base.as_deref()).unwrap_or_default());
            self.upsert(table, project, tree, id, keys, true, None, base)?;
        }
        Ok(true)
    }

    /// A row written, with what it was kept for the commit first. A base is stamped with
    /// when it was taken, once: the same base keeps its time (§6).
    #[allow(clippy::too_many_arguments)]
    fn upsert(
        &mut self,
        table: Table,
        project: &str,
        tree: &str,
        id: &str,
        keys: Vec<(&'static str, Value)>,
        removed: bool,
        doc: Option<String>,
        base: Option<String>,
    ) -> Result<(), String> {
        if tree.is_empty() {
            self.freeze(table, project, id)?;
        }
        self.history.before(&self.tx, table, project, tree, id)?;
        let at = self.now()?;
        let mut names: Vec<&str> = vec!["project_id", "branch_id", "id"];
        names.extend(keys.iter().map(|k| k.0));
        names.extend(["removed", "doc", "base", "based_at"]);
        let mut binds: Vec<rusqlite::types::Value> = vec![
            project.to_string().into(),
            tree.to_string().into(),
            id.to_string().into(),
        ];
        for (_, value) in &keys {
            binds.push(match value {
                Value::String(s) => s.clone().into(),
                _ => rusqlite::types::Value::Null,
            });
        }
        let based_at = base.as_ref().map(|_| at.clone());
        binds.push((removed as i64).into());
        binds.push(doc.into());
        binds.push(base.into());
        binds.push(based_at.into());
        let updates: Vec<String> = keys
            .iter()
            .map(|k| k.0)
            .chain(["removed", "doc", "base"])
            .map(|c| format!("{c} = excluded.{c}"))
            .chain([
                "based_at = CASE WHEN excluded.base IS NULL THEN NULL WHEN base IS excluded.base THEN based_at ELSE excluded.based_at END".to_string(),
            ])
            .collect();
        self.tx
            .execute(
                &format!(
                    "INSERT INTO {} ({}) VALUES ({}) ON CONFLICT(project_id, branch_id, id) DO UPDATE SET {}",
                    table.name(),
                    names.join(", "),
                    vec!["?"; names.len()].join(", "),
                    updates.join(", ")
                ),
                rusqlite::params_from_iter(binds),
            )
            .map_err(db)?;
        self.touch(project, tree);
        Ok(())
    }

    /// Where a box sits on a diagram (`view`: the section whose diagram, `""` for a card's
    /// own section's; a flow's card for its nodes, a goal's for the cards on its card
    /// map), or none to let it go. No commit. main's lock lets every box through — a card's
    /// own, a flow's nodes, what sits on a goal's card map, and what sits on a section's
    /// diagram for another section (one of its cards, or the card that sums it up): where a
    /// thing is drawn is how it looks (`specboard://SpecBoard/Docs/Design:DesignBranches` §8).
    pub fn place(
        &mut self,
        project: &str,
        tree: &str,
        view: &str,
        item_kind: &str,
        item: &str,
        spot: Option<&Spot>,
    ) -> Result<(), String> {
        match spot {
            Some(s) => self
                .tx
                .execute(
                    "INSERT INTO places (project_id, branch_id, view, item_kind, item, x, y, w, h, pinned) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                     ON CONFLICT(project_id, branch_id, view, item_kind, item) DO UPDATE SET x = excluded.x, y = excluded.y, w = excluded.w, h = excluded.h, pinned = excluded.pinned",
                    params![project, tree, view, item_kind, item, s.x, s.y, s.w, s.h, s.pinned as i64],
                )
                .map_err(db)?,
            None => self
                .tx
                .execute(
                    "DELETE FROM places WHERE project_id = ? AND branch_id = ? AND view = ? AND item_kind = ? AND item = ?",
                    params![project, tree, view, item_kind, item],
                )
                .map_err(db)?,
        };
        self.touch(project, tree);
        Ok(())
    }

    /// How a card (or a row of it, by its address) looks, or none to let it go back to
    /// the kind's default. No commit, and it passes main's lock.
    pub fn look(
        &mut self,
        project: &str,
        tree: &str,
        card: &str,
        row_path: &str,
        field: &str,
        value: Option<&Value>,
    ) -> Result<(), String> {
        match value {
            Some(v) => self
                .tx
                .execute(
                    "INSERT INTO card_looks (project_id, branch_id, card_id, row_path, field, value) VALUES (?, ?, ?, ?, ?, ?)
                     ON CONFLICT(project_id, branch_id, card_id, row_path, field) DO UPDATE SET value = excluded.value",
                    params![project, tree, card, row_path, field, v.to_string()],
                )
                .map_err(db)?,
            None => self
                .tx
                .execute(
                    "DELETE FROM card_looks WHERE project_id = ? AND branch_id = ? AND card_id = ? AND row_path = ? AND field = ?",
                    params![project, tree, card, row_path, field],
                )
                .map_err(db)?,
        };
        self.touch(project, tree);
        Ok(())
    }

    /// A line of what was done, in the commit this transaction makes in the tree; the
    /// project is marked as written now.
    pub fn line(
        &mut self,
        project: &str,
        tree: &str,
        kind: &str,
        target_type: &str,
        target_id: Option<&str>,
        said: crate::text::Keyed,
    ) -> Result<(), String> {
        self.history
            .line(project, tree, kind, target_type, target_id, said);
        let at = self.now()?;
        let rows = self
            .tx
            .execute(
                "UPDATE projects SET updated_at = ? WHERE id = ?",
                params![at, project],
            )
            .map_err(db)?;
        self.kept_account(rows as u64, || Written::Marked {
            project: project.to_string(),
            at,
        });
        self.touch(project, tree);
        Ok(())
    }

    /// `body`'s writes kept only when it goes through: refused, what it wrote and what it
    /// said are undone and the rest of the transaction goes on (`alone`: each target of a
    /// call is tried against the project as the others leave it).
    /// The outer result is the store's own failure.
    pub fn alone<T, E>(
        &mut self,
        body: impl FnOnce(&mut Tx<'s>) -> Result<T, E>,
    ) -> Result<Result<T, E>, String> {
        self.tx.execute_batch("SAVEPOINT alone").map_err(db)?;
        let (history, touched) = (self.history.clone(), self.touched.clone());
        let out = body(self);
        if out.is_ok() {
            self.tx.execute_batch("RELEASE alone").map_err(db)?;
        } else {
            self.tx
                .execute_batch("ROLLBACK TO alone; RELEASE alone")
                .map_err(db)?;
            self.history = history;
            self.touched = touched;
            // What was worked out after `body` wrote is of rows undone now, and the count of rows
            // changed does not go back with them (`remembered_or_brought_up`): let go of it, and of
            // the writes it would have been brought up to.
            self.memo.borrow_mut().take();
            self.since.borrow_mut().clear();
        }
        Ok(out)
    }

    /// `body` run with the lines it says in the tree let go of: what it writes is kept, said by the
    /// one line its caller writes for it (a card action says what it did as one, not as the
    /// writes it is made of).
    pub fn unsaid<T>(
        &mut self,
        project: &str,
        tree: &str,
        body: impl FnOnce(&mut Tx<'s>) -> Result<T, String>,
    ) -> Result<T, String> {
        let kept = self.history.said_in(project, tree);
        let out = body(self)?;
        self.history.unsay(project, tree, kept);
        Ok(out)
    }

    /// What kind of commit this transaction makes in the tree: a merge, a revert, an
    /// export… Plain edits need not say.
    pub fn mark(&mut self, project: &str, tree: &str, kind: &str) {
        self.history.mark(project, tree, kind);
        self.touch(project, tree);
    }

    /// Every line of the transaction so far, in order — what a batch's steps wrote.
    pub fn written(&self) -> &[(String, crate::text::Keyed)] {
        &self.history.written
    }

    /// The cards this transaction has written in the tree so far, by id, in the order it first
    /// wrote each (what its commit will keep): what a batch run says the revs of.
    pub fn cards_written(&self, project: &str, tree: &str) -> Vec<String> {
        self.history.ids(project, tree, Table::Cards)
    }

    /// The lines to files this transaction made or pointed elsewhere so far, let go of here: what
    /// is recorded of their code once it commits.
    pub fn take_code_lines(&mut self) -> Vec<FileLine> {
        let mut out = std::mem::take(&mut self.code_lines);
        // Written twice in one transaction: recorded once, as it was last.
        let mut seen = std::collections::HashSet::new();
        out.reverse();
        out.retain(|l| seen.insert((l.project.clone(), l.line_id.clone())));
        out.reverse();
        out
    }

    /// Done: the commits written, then the transaction. Returns the trees it wrote.
    pub fn commit(mut self) -> Result<BTreeSet<(String, String)>, String> {
        let gathered = std::mem::take(&mut self.history);
        gathered.flush(&self.tx, &self.who)?;
        let touched = std::mem::take(&mut self.touched);
        let after = std::mem::take(&mut self.after);
        self.tx.commit().map_err(db)?;
        for then in after {
            then();
        }
        Ok(touched)
    }
}

#[cfg(test)]
mod tests;
