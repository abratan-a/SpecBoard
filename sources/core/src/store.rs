//! The store: a project database of the core's form (`schema`) — a card or a section as one
//! JSON document each, main's and each branch's own (`card_docs`, `section_docs`), every
//! box on a diagram apart (`places`), and how cards look (`card_looks`).
//!
//! A database of the Swift app's form is not opened.

use rusqlite::{Connection, OpenFlags, OptionalExtension, TransactionBehavior, params};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::path::Path;

/// The board's rows' `branch_id` (`Board.id`): a tree beside main that is never merged.
pub const BOARD: &str = "~board";

/// What MCP's `branch` names the board by: wrapped in `_`, as the app's own names are, so that no branch is
/// called so. The window says プロジェクトボード.
pub const BOARD_NAME: &str = "_project_board_";

/// What MCP's `branch` named the working directories by when they were a tree of their own: no
/// branch is called so, and a call naming it is told how they are read now.
pub const DIRECTORY_NAME: &str = "_directory_";

/// Whether a tree keeps its own and reads nothing of main's beneath it: the board.
pub fn is_own_tree(tree: &str) -> bool {
    tree == BOARD
}

/// The tables the core's form keeps the documents in.
const CORE_TABLES: [&str; 6] = [
    "projects",
    "branches",
    "card_docs",
    "section_docs",
    "places",
    "card_looks",
];

pub struct Store {
    pub conn: Connection,
    /// Which of the working directories' readings shared by the core's connections this one's
    /// tables hold (`workdir::Shared`).
    pub(crate) work_seen: std::cell::Cell<i64>,
}

/// What reads the documents, over a connection it borrows: the store's own, or a write
/// transaction's (which sees what it has written).
pub struct Reader<'c> {
    pub conn: &'c Connection,
}

impl Store {
    pub fn reader(&self) -> Reader<'_> {
        Reader { conn: &self.conn }
    }

    pub fn project(&self, project_id: &str) -> Result<Option<ProjectRow>, String> {
        self.reader().project(project_id)
    }

    pub fn branch_exists(&self, project_id: &str, branch_id: &str) -> Result<bool, String> {
        self.reader().branch_exists(project_id, branch_id)
    }

    /// The project's exports (`projects.exports`), in order.
    pub fn exports(
        &self,
        project_id: &str,
    ) -> Result<Vec<crate::export::settings::Export>, String> {
        self.reader().exports(project_id)
    }
}

pub(crate) fn table_exists(conn: &Connection, name: &str) -> Result<bool, String> {
    conn.query_row(
        "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?",
        [name],
        |_| Ok(()),
    )
    .optional()
    .map(|found| found.is_some())
    .map_err(sql)
}

fn marked(conn: &Connection, mark: &str) -> Result<bool, String> {
    if !table_exists(conn, "_migrations")? {
        return Ok(false);
    }
    conn.query_row("SELECT 1 FROM _migrations WHERE name = ?", [mark], |_| {
        Ok(())
    })
    .optional()
    .map(|found| found.is_some())
    .map_err(sql)
}

/// The marks of the core's changes to the form (`core_…`) the store has and this core does not
/// know (`schema::MARKS`): a newer core's.
fn unknown_marks(conn: &Connection) -> Result<Vec<String>, String> {
    if !table_exists(conn, "_migrations")? {
        return Ok(Vec::new());
    }
    let mut stmt = conn
        .prepare("SELECT name FROM _migrations WHERE name LIKE 'core\\_%' ESCAPE '\\'")
        .map_err(sql)?;
    let names = stmt
        .query_map([], |r| r.get::<_, String>(0))
        .map_err(sql)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sql)?;
    Ok(names
        .into_iter()
        .filter(|name| !crate::schema::MARKS.contains(&name.as_str()))
        .collect())
}

impl Store {
    /// A new, empty database of the core's form at `path` (where there is none yet).
    pub fn create(path: &Path) -> Result<(), String> {
        if path.exists() {
            return Err(text!("store.error.exists", path = path.display()));
        }
        let made = (|| -> Result<(), String> {
            let mut conn = Connection::open(path).map_err(|e| text!("store.error.create", e))?;
            let tx = conn.transaction().map_err(sql)?;
            tx.execute_batch(crate::schema::DDL).map_err(sql)?;
            for mark in crate::schema::MARKS {
                tx.execute("INSERT INTO _migrations (name) VALUES (?1)", [mark])
                    .map_err(sql)?;
            }
            tx.commit().map_err(sql)
        })();
        if made.is_err() {
            let _ = std::fs::remove_file(path);
        }
        made
    }

    /// Open the database at `path`, of the core's form, to read and write it.
    pub fn open(path: &Path) -> Result<Store, String> {
        if !path.is_file() {
            return Err(text!("store.error.missing", path = path.display()));
        }
        let mut conn = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(|e| text!("store.error.open", e))?;
        if !marked(&conn, crate::schema::MARK)? {
            return Err(text!("store.error.notCoreForm"));
        }
        // A change to the form this core does not know was made by a newer core: what it moved,
        // this one would read wrongly and write over.
        if !unknown_marks(&conn)?.is_empty() {
            return Err(text!("store.error.newer", path = path.display()));
        }
        for table in CORE_TABLES {
            // `projects` by its name of before `core_0014_projects` too: brought up below.
            let there = table_exists(&conn, table)?
                || (table == "projects" && table_exists(&conn, "plans")?);
            if !there {
                return Err(text!("store.error.tableMissing", table));
            }
        }
        conn.busy_timeout(std::time::Duration::from_secs(5))
            .map_err(sql)?;
        conn.execute_batch("PRAGMA foreign_keys = ON;")
            .map_err(sql)?;
        upgrade(&mut conn, path)?;
        crate::write::guard(&conn)?;
        Ok(Store::of(conn))
    }

    fn of(conn: Connection) -> Store {
        Store {
            conn,
            work_seen: std::cell::Cell::new(0),
        }
    }

    /// Whether what is written is kept in a write-ahead log beside the store, set so if it is not
    /// yet: then a connection of its own
    /// reads what was committed while a write goes on, and is not kept waiting for it
    /// (`open_reader`). The tables and the documents are as they were; only how SQLite keeps a
    /// write until it is done changes, and stays so for the file. A store where it cannot be (a
    /// file system that cannot share the log) is read under the write's lock, as before.
    pub fn logged_ahead(&self) -> Result<bool, String> {
        let mode: String = self
            .conn
            .query_row("PRAGMA journal_mode = WAL", [], |r| r.get(0))
            .map_err(sql)?;
        Ok(mode.eq_ignore_ascii_case("wal"))
    }

    /// A connection of its own to the store at `path`, to read it apart from the writes: it reads
    /// what was committed when it began (`Reading`), and is refused any write to the store — it may
    /// keep only what the working directories read is kept in (`temp`).
    pub fn open_reader(path: &Path) -> Result<Store, String> {
        let conn = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(|e| text!("store.error.open", e))?;
        conn.busy_timeout(std::time::Duration::from_secs(5))
            .map_err(sql)?;
        conn.authorizer(Some(|context: rusqlite::hooks::AuthContext<'_>| {
            use rusqlite::hooks::{AuthAction, Authorization};
            let store = context.database_name != Some("temp");
            match context.action {
                AuthAction::Insert { .. }
                | AuthAction::Update { .. }
                | AuthAction::Delete { .. }
                    if store =>
                {
                    Authorization::Deny
                }
                AuthAction::CreateTable { .. }
                | AuthAction::CreateIndex { .. }
                | AuthAction::CreateTrigger { .. }
                | AuthAction::CreateView { .. }
                | AuthAction::DropTable { .. }
                | AuthAction::DropIndex { .. }
                | AuthAction::DropTrigger { .. }
                | AuthAction::DropView { .. }
                | AuthAction::AlterTable { .. }
                | AuthAction::Reindex { .. }
                | AuthAction::Analyze { .. }
                | AuthAction::CreateVtable { .. }
                | AuthAction::DropVtable { .. }
                | AuthAction::Attach { .. }
                | AuthAction::Detach { .. } => Authorization::Deny,
                _ => Authorization::Allow,
            }
        }))
        .map_err(sql)?;
        Ok(Store::of(conn))
    }
}

fn has_column(conn: &Connection, table: &str, column: &str) -> Result<bool, String> {
    conn.query_row(
        &format!("SELECT 1 FROM pragma_table_info('{table}') WHERE name = ?"),
        [column],
        |_| Ok(()),
    )
    .optional()
    .map(|found| found.is_some())
    .map_err(sql)
}

/// The first of the form's marks (`schema::MARKS`, in the order they came) the store does not
/// have yet: the first change `upgrade` makes to it. None for a store of the form now.
fn first_unmarked(conn: &Connection) -> Result<Option<&'static str>, String> {
    for mark in crate::schema::MARKS {
        if !marked(conn, mark)? {
            return Ok(Some(mark));
        }
    }
    Ok(None)
}

/// Whether the store holds anything of a project or a board (a board is a project's tree beside
/// main, so a project's row is there for it too).
fn in_use(conn: &Connection) -> Result<bool, String> {
    // A store of before `core_0014_projects` keeps them in `plans`.
    let projects = if table_exists(conn, "projects")? {
        "projects"
    } else {
        "plans"
    };
    conn.query_row(
        &format!(
            "SELECT EXISTS (SELECT 1 FROM {projects}) OR EXISTS (SELECT 1 FROM section_docs) OR EXISTS (SELECT 1 FROM card_docs)"
        ),
        [],
        |r| r.get(0),
    )
    .map_err(sql)
}

/// A store of before `core_0014_projects` named by the project (`schema::PROJECTS`): its tables and
/// columns, its history's lines, its kinds' tree and the items of what MCP may write.
fn named_projects(tx: &Connection) -> Result<(), String> {
    if !table_exists(tx, "plans")? {
        return Ok(());
    }
    tx.execute_batch("ALTER TABLE plans RENAME TO projects")
        .map_err(sql)?;
    for (before, now) in [
        ("plan_export_root", "project_export_root"),
        ("plan_public", "project_public"),
    ] {
        if table_exists(tx, before)? {
            tx.execute_batch(&format!("ALTER TABLE {before} RENAME TO {now}"))
                .map_err(sql)?;
        }
    }
    let tables: Vec<String> = tx
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table'")
        .map_err(sql)?
        .query_map([], |r| r.get(0))
        .map_err(sql)?
        .collect::<Result<_, _>>()
        .map_err(sql)?;
    for table in tables {
        if has_column(tx, &table, "plan_id")? {
            tx.execute_batch(&format!(
                "ALTER TABLE {table} RENAME COLUMN plan_id TO project_id"
            ))
            .map_err(sql)?;
        }
    }
    tx.execute_batch(
        "UPDATE commit_lines SET target_type = 'project' WHERE target_type = 'plan';
         UPDATE commit_lines SET kind = 'project' || substr(kind, 5) WHERE kind LIKE 'plan.%';",
    )
    .map_err(sql)?;
    let named = |text: &Option<String>, key: &dyn Fn(&str) -> Option<String>| -> Option<String> {
        let mut value: serde_json::Value = serde_json::from_str(text.as_deref()?).ok()?;
        let object = value.as_object_mut()?;
        let before: Vec<String> = object
            .keys()
            .filter(|k| key(k).is_some())
            .cloned()
            .collect();
        if before.is_empty() {
            return None;
        }
        for old in before {
            let held = object.shift_remove(&old)?;
            object.insert(key(&old)?, held);
        }
        Some(value.to_string())
    };
    // The kinds' tree, the items of what MCP may write and the places of the writing rules
    // (`plan.description`), in each project's row. A column a store of before has not got yet
    // comes with its own change, below, named so already.
    let key_now = |column: &str, k: &str| match column {
        "kinds" => (k == "plan_tree").then(|| "project_tree".to_string()),
        _ => k
            .strip_prefix("plan.")
            .map(|rest| format!("project.{rest}")),
    };
    for column in ["kinds", "mcp_writes", "rules"] {
        if !has_column(tx, "projects", column)? {
            continue;
        }
        let rows: Vec<(String, Option<String>)> = tx
            .prepare(&format!("SELECT id, {column} FROM projects"))
            .map_err(sql)?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(sql)?
            .collect::<Result<_, _>>()
            .map_err(sql)?;
        for (id, text) in rows {
            if let Some(now) = named(&text, &|k| key_now(column, k)) {
                tx.execute(
                    &format!("UPDATE projects SET {column} = ? WHERE id = ?"),
                    [&now, &id],
                )
                .map_err(sql)?;
            }
        }
    }
    Ok(())
}

/// Where the copy of the store at `path`, as it was before `upgrade` made the change `mark`
/// and the ones after it, is kept: beside it, named by how far the store had been brought up — the
/// change before `mark` — so a person reads which form it was moved from, however many changes
/// were made at once (`<its name>.before-upgrade-from-<that change>.db`).
fn kept_before(path: &Path, mark: &str) -> std::path::PathBuf {
    let marks = crate::schema::MARKS;
    let done = marks
        .iter()
        .position(|m| *m == mark)
        .and_then(|at| at.checked_sub(1))
        .map_or("none", |at| marks[at]);
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(format!(".before-upgrade-from-{done}.db"));
    path.with_file_name(name)
}

/// A copy of the store at `path` as it is, kept beside it before `upgrade` changes it from
/// the change `mark` on (`kept_before`): what a move does to the documents cannot be undone in
/// the store itself. A copy there already is left as it is (the one of before). The copy is
/// taken over a connection of its own, which sees what is committed, what is in the WAL as
/// well (the store's write lock, held by `upgrade`, lets no other write in meanwhile), into a
/// file beside it that takes its name only once it is whole. Nothing deletes it.
fn keep(path: &Path, mark: &str) -> Result<(), String> {
    let kept = kept_before(path, mark);
    let refused = |reason: String| {
        text!(
            "store.error.keepRefused",
            database = path.display(),
            copy = kept.display(),
            reason
        )
    };
    match kept.try_exists() {
        Ok(true) => return Ok(()),
        Ok(false) => {}
        Err(e) => return Err(refused(text!("store.error.keepLook", e))),
    }
    let mut partial = kept.clone().into_os_string();
    partial.push(".partial");
    let partial = std::path::PathBuf::from(partial);
    let made = (|| -> Result<(), String> {
        // One left by a copy that stopped part way.
        if partial.try_exists().unwrap_or(false) {
            std::fs::remove_file(&partial)
                .map_err(|e| text!("store.error.keepPartial", path = partial.display(), e))?;
        }
        let to = partial
            .to_str()
            .ok_or_else(|| text!("store.error.keepName"))?;
        let from = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(|e| text!("store.error.read", e))?;
        from.busy_timeout(std::time::Duration::from_secs(5))
            .map_err(|e| text!("store.error.read", e))?;
        from.execute("VACUUM INTO ?1", [to])
            .map_err(|e| text!("store.error.keepWrite", e))?;
        drop(from);
        std::fs::File::open(&partial)
            .and_then(|f| f.sync_all())
            .map_err(|e| text!("store.error.keepFinish", e))?;
        std::fs::rename(&partial, &kept).map_err(|e| text!("store.error.keepRename", e))
    })();
    if let Err(reason) = made {
        let _ = std::fs::remove_file(&partial);
        return Err(refused(reason));
    }
    Ok(())
}

/// A store made before a change to the form, brought up to it once (`schema::MARKS`): what
/// it does not have yet is added, and the change marked done. Opened again, nothing is
/// done. Such a store is in use on the owner's Mac, so nothing it holds is let go of: one
/// that holds a project or a board is copied beside it first (`keep`), and not opened if the
/// copy cannot be taken.
fn upgrade(conn: &mut Connection, path: &Path) -> Result<(), String> {
    use crate::schema::{
        BOARD_KEYS, BRANCH_SNAPSHOTS, CARD_GHOSTS, CONNECTIONS, CONTAINERS, HOLD_CLIENTS, LANDINGS,
        MCP_WRITES, MERGE_REQUESTS, MR_PLACES, PROJECT_TREE, WORK_BRANCH,
    };
    if first_unmarked(conn)?.is_none() {
        return Ok(());
    }
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(sql)?;
    // Another opening may have done it while this one waited.
    let Some(first) = first_unmarked(&tx)? else {
        return Ok(());
    };
    if in_use(&tx)? {
        keep(path, first)?;
    }
    // The names first: every change below reads the tables by the names of now.
    if !marked(&tx, crate::schema::PROJECTS)? {
        named_projects(&tx)?;
    }
    // The history's lines kept by key before anything below writes one (a move of before says
    // what it did as a line).
    commit_keys(&tx)?;
    if !marked(&tx, MCP_WRITES)? {
        if !has_column(&tx, "projects", "mcp_writes")? {
            tx.execute_batch("ALTER TABLE projects ADD COLUMN mcp_writes TEXT")
                .map_err(sql)?;
        }
        // "May move the window" split in two: a key that could, can do both.
        if !has_column(&tx, "access_keys", "can_control")? {
            tx.execute_batch("ALTER TABLE access_keys RENAME COLUMN can_drive TO can_control")
                .map_err(sql)?;
        }
        if !has_column(&tx, "access_keys", "can_capture")? {
            tx.execute_batch(
                "ALTER TABLE access_keys ADD COLUMN can_capture INTEGER NOT NULL DEFAULT 0;
                 UPDATE access_keys SET can_capture = can_control;",
            )
            .map_err(sql)?;
        }
        // Every key could make a project before: each still may.
        if !has_column(&tx, "access_keys", "can_create")? {
            tx.execute_batch(
                "ALTER TABLE access_keys ADD COLUMN can_create INTEGER NOT NULL DEFAULT 1",
            )
            .map_err(sql)?;
        }
        tx.execute("INSERT INTO _migrations (name) VALUES (?1)", [MCP_WRITES])
            .map_err(sql)?;
    }
    // A section's card ghosts, cards of their own (`ghost_move`).
    if !marked(&tx, CARD_GHOSTS)? {
        crate::ghost_move::move_all(&tx)?;
        tx.execute("INSERT INTO _migrations (name) VALUES (?1)", [CARD_GHOSTS])
            .map_err(sql)?;
    }
    // A request to merge into main asks for the boxes the branch moved, or not: one made
    // before did not.
    if !marked(&tx, MR_PLACES)? {
        let table = requests_table(&tx)?;
        if !has_column(&tx, table, "places")? {
            tx.execute_batch(&format!(
                "ALTER TABLE {table} ADD COLUMN places INTEGER NOT NULL DEFAULT 0"
            ))
            .map_err(sql)?;
        }
        tx.execute("INSERT INTO _migrations (name) VALUES (?1)", [MR_PLACES])
            .map_err(sql)?;
    }
    // A project's own kinds in its row, and a card's lines as its connections.
    if !marked(&tx, CONNECTIONS)? {
        if !has_column(&tx, "projects", "kinds")? {
            tx.execute_batch("ALTER TABLE projects ADD COLUMN kinds TEXT")
                .map_err(sql)?;
        }
        own_kinds_into_projects(&tx)?;
        crate::line_move::move_all(&tx)?;
        tx.execute("INSERT INTO _migrations (name) VALUES (?1)", [CONNECTIONS])
            .map_err(sql)?;
    }
    // What a container holds, inside it: a flow's nodes and a goal's items cards of their own.
    if !marked(&tx, CONTAINERS)? {
        crate::container_move::move_all(&tx)?;
        tx.execute("INSERT INTO _migrations (name) VALUES (?1)", [CONTAINERS])
            .map_err(sql)?;
    }
    // A designating line lands inside the card it reaches: one moved before that lands in
    // another card, split in two (a store moved since makes none).
    if !marked(&tx, LANDINGS)? {
        crate::line_move::split_all(&tx)?;
        tx.execute("INSERT INTO _migrations (name) VALUES (?1)", [LANDINGS])
            .map_err(sql)?;
    }
    // A goal's fields by their names in English, their values as they were.
    if !marked(&tx, BOARD_KEYS)? {
        crate::key_move::move_all(&tx)?;
        tx.execute("INSERT INTO _migrations (name) VALUES (?1)", [BOARD_KEYS])
            .map_err(sql)?;
    }
    // A request to merge called a merge request: its table, a goal's field and the kind of field;
    // and the project's switch for an AI's merges into a locked main.
    if !marked(&tx, MERGE_REQUESTS)? {
        if table_exists(&tx, "pull_requests")? {
            tx.execute_batch(
                "ALTER TABLE pull_requests RENAME TO merge_requests;
                 DROP INDEX IF EXISTS pull_requests_number;
                 CREATE UNIQUE INDEX IF NOT EXISTS merge_requests_number ON merge_requests(project_id, number);",
            )
            .map_err(sql)?;
        }
        crate::key_move::move_mr_all(&tx)?;
        // Whether an AI's merge goes into a locked main without being approved.
        if !has_column(&tx, "projects", "ai_merge")? {
            tx.execute_batch("ALTER TABLE projects ADD COLUMN ai_merge INTEGER NOT NULL DEFAULT 0")
                .map_err(sql)?;
        }
        tx.execute(
            "INSERT INTO _migrations (name) VALUES (?1)",
            [MERGE_REQUESTS],
        )
        .map_err(sql)?;
    }
    // What is directly under each root, the project's (`project_tree`).
    if !marked(&tx, PROJECT_TREE)? {
        crate::tree_move::move_all(&tx)?;
        tx.execute("INSERT INTO _migrations (name) VALUES (?1)", [PROJECT_TREE])
            .map_err(sql)?;
    }
    // A goal's `branch` named `workBranch`, its value as it was.
    if !marked(&tx, WORK_BRANCH)? {
        crate::key_move::move_work_branch_all(&tx)?;
        tx.execute("INSERT INTO _migrations (name) VALUES (?1)", [WORK_BRANCH])
            .map_err(sql)?;
    }
    // The window's holds by the Mac it is on: those there were, this Mac's own.
    if !marked(&tx, HOLD_CLIENTS)? {
        if !has_column(&tx, "holds", "client")? {
            tx.execute_batch("ALTER TABLE holds ADD COLUMN client TEXT NOT NULL DEFAULT ''")
                .map_err(sql)?;
        }
        tx.execute("INSERT INTO _migrations (name) VALUES (?1)", [HOLD_CLIENTS])
            .map_err(sql)?;
    }
    // A branch sees main as it was made: what main changes kept for it; the branches there
    // were, main as it is now (nothing kept yet).
    if !marked(&tx, BRANCH_SNAPSHOTS)? {
        if !has_column(&tx, "branches", "reverts")? {
            tx.execute_batch("ALTER TABLE branches ADD COLUMN reverts TEXT")
                .map_err(sql)?;
        }
        if !has_column(&tx, "merge_requests", "keep")? {
            tx.execute_batch(
                "ALTER TABLE merge_requests ADD COLUMN keep INTEGER NOT NULL DEFAULT 0",
            )
            .map_err(sql)?;
        }
        if !table_exists(&tx, "branch_frozen")? {
            tx.execute_batch(
                "CREATE TABLE branch_frozen (
  project_id   TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  branch_id TEXT NOT NULL REFERENCES branches(id) ON DELETE CASCADE,
  tbl       TEXT NOT NULL,
  id        TEXT NOT NULL,
  doc       TEXT,
  PRIMARY KEY (branch_id, tbl, id)
);",
            )
            .map_err(sql)?;
        }
        tx.execute(
            "INSERT INTO _migrations (name) VALUES (?1)",
            [BRANCH_SNAPSHOTS],
        )
        .map_err(sql)?;
    }
    if !marked(&tx, crate::schema::PROJECTS)? {
        tx.execute(
            "INSERT INTO _migrations (name) VALUES (?1)",
            [crate::schema::PROJECTS],
        )
        .map_err(sql)?;
    }
    // The cards' and sections' export settings, the project's exports (`export_move`).
    if !marked(&tx, crate::schema::EXPORTS)? {
        if !has_column(&tx, "projects", "exports")? {
            tx.execute_batch("ALTER TABLE projects ADD COLUMN exports TEXT")
                .map_err(sql)?;
        }
        crate::export_move::move_all(&tx)?;
        tx.execute(
            "INSERT INTO _migrations (name) VALUES (?1)",
            [crate::schema::EXPORTS],
        )
        .map_err(sql)?;
    }
    // An export holding no working directory: the folder it was bound to, its default one.
    if !marked(&tx, crate::schema::EXPORT_TARGETS)? {
        tx.execute_batch(crate::schema::EXPORT_DEFAULT_FOLDERS)
            .map_err(sql)?;
        crate::export_move::carry_all(&tx)?;
        tx.execute(
            "INSERT INTO _migrations (name) VALUES (?1)",
            [crate::schema::EXPORT_TARGETS],
        )
        .map_err(sql)?;
    }
    // The change notes nothing wrote (`schema::NO_CHANGELOG`).
    if !marked(&tx, crate::schema::NO_CHANGELOG)? {
        tx.execute_batch("DROP INDEX IF EXISTS changelog_entry; DROP TABLE IF EXISTS changelog;")
            .map_err(sql)?;
        tx.execute(
            "INSERT INTO _migrations (name) VALUES (?1)",
            [crate::schema::NO_CHANGELOG],
        )
        .map_err(sql)?;
    }
    // The history's lines kept by key (`schema::COMMIT_KEYS`): the columns are added above.
    if !marked(&tx, crate::schema::COMMIT_KEYS)? {
        tx.execute(
            "INSERT INTO _migrations (name) VALUES (?1)",
            [crate::schema::COMMIT_KEYS],
        )
        .map_err(sql)?;
    }
    // The project's export language (`schema::EXPORT_LANGUAGE`): a project of before is said in
    // Japanese, as its exports were.
    if !marked(&tx, crate::schema::EXPORT_LANGUAGE)? {
        if !has_column(&tx, "projects", "export_language")? {
            tx.execute_batch(
                "ALTER TABLE projects ADD COLUMN export_language TEXT NOT NULL DEFAULT 'ja'",
            )
            .map_err(sql)?;
        }
        tx.execute(
            "INSERT INTO _migrations (name) VALUES (?1)",
            [crate::schema::EXPORT_LANGUAGE],
        )
        .map_err(sql)?;
    }
    // The check's marks (`schema::INSPECTION_MARKS`): none yet.
    if !marked(&tx, crate::schema::INSPECTION_MARKS)? {
        tx.execute_batch(crate::schema::INSPECTION_MARKS_TABLE)
            .map_err(sql)?;
        tx.execute(
            "INSERT INTO _migrations (name) VALUES (?1)",
            [crate::schema::INSPECTION_MARKS],
        )
        .map_err(sql)?;
    }
    // The built-in `instances` replaced by `definition` (`definition_move`).
    if !marked(&tx, crate::schema::DEFINITION_KIND)? {
        crate::definition_move::move_all(&tx)?;
        tx.execute(
            "INSERT INTO _migrations (name) VALUES (?1)",
            [crate::schema::DEFINITION_KIND],
        )
        .map_err(sql)?;
    }
    // The board's sections for what is finished and what was cancelled (`board_move`).
    if !marked(&tx, crate::schema::BOARD_SECTIONS)? {
        crate::board_move::move_all(&tx)?;
        tx.execute(
            "INSERT INTO _migrations (name) VALUES (?1)",
            [crate::schema::BOARD_SECTIONS],
        )
        .map_err(sql)?;
    }
    // The working directories as sections of main and every branch (`workdir_move`).
    if !marked(&tx, crate::schema::WORKDIR_SECTIONS)? {
        crate::workdir_move::move_all(&tx)?;
        tx.execute(
            "INSERT INTO _migrations (name) VALUES (?1)",
            [crate::schema::WORKDIR_SECTIONS],
        )
        .map_err(sql)?;
    }
    // The code the implementations' lines were recorded at (`schema::CODE_MARKS`): none yet.
    if !marked(&tx, crate::schema::CODE_MARKS)? {
        tx.execute_batch(crate::schema::CODE_MARKS_TABLE)
            .map_err(sql)?;
        tx.execute(
            "INSERT INTO _migrations (name) VALUES (?1)",
            [crate::schema::CODE_MARKS],
        )
        .map_err(sql)?;
    }
    // A diagram's groups, rows of `places` (`schema::MAP_GROUPS`): the table made anew, its rows kept.
    if !marked(&tx, crate::schema::MAP_GROUPS)? {
        if !has_column(&tx, "places", "look")? {
            tx.execute_batch(crate::schema::PLACES_NEW).map_err(sql)?;
        }
        tx.execute(
            "INSERT INTO _migrations (name) VALUES (?1)",
            [crate::schema::MAP_GROUPS],
        )
        .map_err(sql)?;
    }
    // The built-in names of now (`schema::BUILTIN_NAMES`): after every move of before, which
    // writes the names of before.
    if !marked(&tx, crate::schema::BUILTIN_NAMES)? {
        crate::name_move::move_all(&tx)?;
        tx.execute(
            "INSERT INTO _migrations (name) VALUES (?1)",
            [crate::schema::BUILTIN_NAMES],
        )
        .map_err(sql)?;
    }
    tx.commit().map_err(sql)
}

/// The columns of the history's lines kept by key (`schema::COMMIT_KEYS`), where they are not yet.
fn commit_keys(conn: &Connection) -> Result<(), String> {
    for column in crate::schema::COMMIT_KEYS_COLUMNS {
        if !has_column(conn, "commit_lines", column)? {
            conn.execute_batch(&format!(
                "ALTER TABLE commit_lines ADD COLUMN {column} TEXT"
            ))
            .map_err(sql)?;
        }
    }
    Ok(())
}

/// The table the requests to merge are kept in: `merge_requests`, or `pull_requests` in a store
/// not yet brought up to `core_0009_merge_requests` — what a move of before reads and writes.
pub(crate) fn requests_table(conn: &Connection) -> Result<&'static str, String> {
    Ok(if table_exists(conn, "merge_requests")? {
        "merge_requests"
    } else {
        "pull_requests"
    })
}

/// The tables a project's own kinds of card were kept in before (`card_kinds`, `card_fields`)
/// let go of: what they held (they were never written but by a project file read in) goes into
/// the project's row, its kinds of card as they were written (`projects.kinds`' `cards`).
fn own_kinds_into_projects(conn: &Connection) -> Result<(), String> {
    if !table_exists(conn, "card_kinds")? {
        return Ok(());
    }
    let kinds: Vec<(String, String, i64, String)> = {
        let mut stmt = conn
            .prepare(
                "SELECT project_id, name, version, doc FROM card_kinds ORDER BY project_id, name",
            )
            .map_err(sql)?;
        stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
            .map_err(sql)?
            .collect::<Result<_, _>>()
            .map_err(sql)?
    };
    let mut by_project: HashMap<String, Vec<Value>> = HashMap::new();
    for (project, name, version, doc) in kinds {
        let fields: Vec<Value> = if table_exists(conn, "card_fields")? {
            let mut stmt = conn
                .prepare("SELECT parent, name, doc FROM card_fields WHERE project_id = ? AND kind = ? ORDER BY parent, sort_order, name")
                .map_err(sql)?;
            stmt.query_map(params![project, name], |r| {
                let (parent, name, doc): (String, String, String) =
                    (r.get(0)?, r.get(1)?, r.get(2)?);
                Ok(serde_json::json!({
                    "parent": parent,
                    "name": name,
                    "doc": serde_json::from_str::<Value>(&doc).unwrap_or(Value::Null),
                }))
            })
            .map_err(sql)?
            .collect::<Result<_, _>>()
            .map_err(sql)?
        } else {
            Vec::new()
        };
        by_project
            .entry(project)
            .or_default()
            .push(serde_json::json!({
                "kind": name,
                "version": version,
                "doc": serde_json::from_str::<Value>(&doc).unwrap_or(Value::Null),
                "fields": fields,
            }));
    }
    for (project, cards) in by_project {
        let mut own: serde_json::Map<String, Value> = conn
            .query_row("SELECT kinds FROM projects WHERE id = ?", [&project], |r| {
                r.get::<_, Option<String>>(0)
            })
            .optional()
            .map_err(sql)?
            .flatten()
            .and_then(|t| serde_json::from_str::<Value>(&t).ok())
            .and_then(|v| v.as_object().cloned())
            .unwrap_or_default();
        own.insert("cards".into(), Value::Array(cards));
        conn.execute(
            "UPDATE projects SET kinds = ? WHERE id = ?",
            params![Value::Object(own).to_string(), project],
        )
        .map_err(sql)?;
    }
    conn.execute_batch("DROP TABLE IF EXISTS card_fields; DROP TABLE IF EXISTS card_kinds;")
        .map_err(sql)
}

impl<'c> Reader<'c> {
    /// The project's row, as SP reads it: id, name, slug, description, created, updated,
    /// and its tag groups' JSON.
    pub fn project(&self, project_id: &str) -> Result<Option<ProjectRow>, String> {
        self.conn
            .query_row(
                "SELECT id, name, slug, description, created_at, updated_at, tag_groups, kinds, export_language FROM projects WHERE id = ?",
                [project_id],
                |r| {
                    Ok(ProjectRow {
                        id: r.get(0)?,
                        name: r.get(1)?,
                        slug: r.get(2)?,
                        description: r.get(3)?,
                        created_at: r.get(4)?,
                        updated_at: r.get(5)?,
                        tag_groups: r.get(6)?,
                        kinds: r.get(7)?,
                        export_language: r.get(8)?,
                    })
                },
            )
            .optional()
            .map_err(sql)
    }

    /// The branch by its id, for a project.
    pub fn branch_exists(&self, project_id: &str, branch_id: &str) -> Result<bool, String> {
        self.conn
            .query_row(
                "SELECT 1 FROM branches WHERE project_id = ? AND id = ?",
                params![project_id, branch_id],
                |_| Ok(()),
            )
            .optional()
            .map(|found| found.is_some())
            .map_err(sql)
    }

    /// The project's exports (`projects.exports`), in order.
    pub fn exports(
        &self,
        project_id: &str,
    ) -> Result<Vec<crate::export::settings::Export>, String> {
        let text: Option<String> = self
            .conn
            .query_row(
                "SELECT exports FROM projects WHERE id = ?",
                [project_id],
                |r| r.get(0),
            )
            .optional()
            .map_err(sql)?
            .flatten();
        Ok(crate::export::settings::parse(text.as_deref()))
    }
}

pub fn sql(e: rusqlite::Error) -> String {
    text!("store.error.read", e)
}

pub struct ProjectRow {
    pub id: String,
    pub name: String,
    pub slug: String,
    pub description: String,
    pub created_at: String,
    pub updated_at: String,
    pub tag_groups: Option<String>,
    /// The project's own kinds of card, section and line (`{cards, sections, lines}`), as kept.
    pub kinds: Option<String>,
    /// The language its exports say their own words in (`schema::EXPORT_LANGUAGE`).
    pub export_language: String,
}

/// Where a card sits on a diagram, its size, and how it is drawn.
#[derive(Clone, Debug, PartialEq)]
pub struct Place {
    pub x: Option<f64>,
    pub y: Option<f64>,
    pub pinned: bool,
    /// Its own size: a width, and a height when one is written.
    pub size: Option<(i64, Option<i64>)>,
    pub view: Option<String>,
    /// A stackdoc's version open, by its name (how it looks); none for the one on top.
    pub version: Option<String>,
}

/// A card or a section in a view, as its document.
#[derive(Clone)]
pub struct Item {
    pub id: String,
    pub doc: serde_json::Map<String, Value>,
}

/// What a tree sees of a project, from the documents (`WorkingCopy.compose`): main's cards
/// and sections it has no row for, then its own.
#[derive(Default)]
pub struct Composition {
    pub sections: Vec<Item>,
    pub cards: Vec<Item>,
    /// Where each card sits, by (section, card) — `""` for its own section's.
    pub places: HashMap<(String, String), Place>,
    /// Where each flow's nodes sit, by card and node.
    pub node_places: HashMap<String, HashMap<String, (i64, i64)>>,
    /// Where each section ghost — the card that sums up a section on another section's
    /// diagram — sits, by (the diagram's section, the section summed up).
    pub section_ghost_places: HashMap<(String, String), Place>,
    /// The values and enums drawn as cards of their own on their section's diagram
    /// (`standalone`, how they look), by card: the tree's word over main's.
    pub standalone: HashMap<String, bool>,
    /// main's cards and sections left out because the branch has one at the same path,
    /// and main's cards whose section the branch removed.
    pub hidden: Vec<String>,
    /// What the working directories as read put in, by id (`workdir::compose_part`): no row
    /// holds it, so nothing compares it, writes it or carries it.
    pub generated: Vec<String>,
}

struct Row {
    id: String,
    key: String,
    removed: bool,
    doc: Option<String>,
}

/// A document as it is stored: a JSON object, or nothing.
pub fn parse(text: Option<&str>) -> Option<serde_json::Map<String, Value>> {
    match serde_json::from_str::<Value>(text?) {
        Ok(Value::Object(o)) => Some(o),
        _ => None,
    }
}

/// The section kinds only main has (`SectionKind.isMainOnly`).
fn main_only(doc: &serde_json::Map<String, Value>) -> bool {
    matches!(
        doc.get("kind").and_then(Value::as_str),
        Some("export") | Some("rules")
    )
}

/// Rows read in place of what one tree keeps (`WorkingCopy.compose`'s `rows`): the project
/// as it stood before or after a commit. Each is (table, id, the row as a commit keeps
/// it — its columns as one JSON object — or none for no row there).
pub struct Over<'a> {
    pub tree: &'a str,
    pub rows: &'a [OverRow],
}

/// A row put in place: its table, its id, and the row or none.
pub type OverRow = (String, String, Option<serde_json::Map<String, Value>>);

impl Over<'_> {
    /// `rows` with what is put in place of them, for the tree `owner`.
    fn apply(&self, table: &str, owner: &str, rows: &mut Vec<Row>) {
        if owner != self.tree {
            return;
        }
        for (t, id, row) in self.rows {
            if t != table {
                continue;
            }
            let at = rows.iter().position(|r| &r.id == id);
            let Some(values) = row else {
                if let Some(at) = at {
                    rows.remove(at);
                }
                continue;
            };
            let text = |k: &str| values.get(k).and_then(Value::as_str).unwrap_or_default();
            let head = if table == "card_docs" {
                text("section_id")
            } else {
                text("parent_id")
            };
            // As the store's key is read: SQLite's lower() folds ASCII alone.
            let put = Row {
                id: id.clone(),
                key: format!("{head}\u{1}{}", text("name").to_ascii_lowercase()),
                removed: values.get("removed").and_then(Value::as_i64).unwrap_or(0) != 0,
                doc: values
                    .get("doc")
                    .and_then(Value::as_str)
                    .map(str::to_string),
            };
            match at {
                Some(at) => rows[at] = put,
                None => rows.push(put),
            }
        }
    }
}

impl<'c> Reader<'c> {
    fn rows(
        &self,
        table: &str,
        key: &str,
        project_id: &str,
        owner: &str,
        over: Option<&Over>,
    ) -> Result<Vec<Row>, String> {
        let mut stmt = self
            .conn
            .prepare(&format!(
                "SELECT id, {key}, removed, doc, base FROM {table} WHERE project_id = ? AND branch_id = ?"
            ))
            .map_err(sql)?;
        let mut rows = stmt
            .query_map(params![project_id, owner], |r| {
                Ok(Row {
                    id: r.get(0)?,
                    key: r.get(1)?,
                    removed: r.get::<_, i64>(2)? != 0,
                    doc: r.get(3)?,
                })
            })
            .map_err(sql)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(sql)?;
        if let Some(over) = over {
            over.apply(table, owner, &mut rows);
        }
        Ok(rows)
    }

    /// main's rows of `table` as a branch sees them: where main has changed one since the branch was made,
    /// as main held it then (`branch_frozen`) — gone when main made it since, there when main
    /// let it go since.
    fn as_made(
        &self,
        table: &str,
        project_id: &str,
        branch: &str,
        rows: &mut Vec<Row>,
    ) -> Result<(), String> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, doc FROM branch_frozen WHERE project_id = ? AND branch_id = ? AND tbl = ?",
            )
            .map_err(sql)?;
        let mut kept: HashMap<String, Option<String>> = stmt
            .query_map(params![project_id, branch, table], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .map_err(sql)?
            .collect::<Result<_, _>>()
            .map_err(sql)?;
        if kept.is_empty() {
            return Ok(());
        }
        // Where it is told apart, as the tables' keys say it (`section_key`, `card_key`).
        let key_of = |doc: &str| -> String {
            let doc = parse(Some(doc)).unwrap_or_default();
            let name = doc
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_ascii_lowercase();
            let at = if table == "section_docs" {
                doc.get("parent_id")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string()
            } else {
                crate::write::holder(&doc).to_string()
            };
            format!("{at}\u{1}{name}")
        };
        rows.retain_mut(|row| match kept.remove(&row.id) {
            None => true,
            Some(None) => false,
            Some(Some(doc)) => {
                row.key = key_of(&doc);
                row.doc = Some(doc);
                true
            }
        });
        for (id, doc) in kept {
            if let Some(doc) = doc {
                rows.push(Row {
                    id,
                    key: key_of(&doc),
                    removed: false,
                    doc: Some(doc),
                });
            }
        }
        Ok(())
    }

    /// What `branch` (None for main, `~board` for the board) sees of the project.
    pub fn compose(&self, project_id: &str, branch: Option<&str>) -> Result<Composition, String> {
        self.compose_over(project_id, branch, None)
    }

    /// What `branch` sees of the project with `over`'s rows in place of its own.
    pub fn compose_over(
        &self,
        project_id: &str,
        branch: Option<&str>,
        over: Option<&Over>,
    ) -> Result<Composition, String> {
        // The board stands alone: nothing of main's beneath it.
        let alone = branch == Some(BOARD);
        let section_key = "ifnull(parent_id, '') || char(1) || lower(name)";
        let card_key = "section_id || char(1) || lower(name)";

        let mut out = Composition::default();

        // Sections: the branch's rows over main's. One of main's at a path the branch
        // has one at too is left out.
        let mut main_sections = if alone {
            Vec::new()
        } else {
            self.rows("section_docs", section_key, project_id, "", over)?
        };
        if let Some(b) = branch.filter(|_| !alone) {
            self.as_made("section_docs", project_id, b, &mut main_sections)?;
        }
        let own_sections = match branch {
            Some(b) => self.rows("section_docs", section_key, project_id, b, over)?,
            None => Vec::new(),
        };
        let own_section_ids: HashSet<&str> = own_sections.iter().map(|r| r.id.as_str()).collect();
        let own_section_keys: HashSet<&str> = own_sections
            .iter()
            .filter(|r| !r.removed)
            .map(|r| r.key.as_str())
            .collect();
        for row in &main_sections {
            if own_section_ids.contains(row.id.as_str()) {
                continue;
            }
            if own_section_keys.contains(row.key.as_str()) {
                out.hidden.push(row.id.clone());
                continue;
            }
            let Some(doc) = parse(row.doc.as_deref()) else {
                continue;
            };
            // Where things are written out is main's alone.
            if branch.is_some() && main_only(&doc) {
                continue;
            }
            out.sections.push(Item {
                id: row.id.clone(),
                doc,
            });
        }
        for row in own_sections.iter().filter(|r| !r.removed) {
            let Some(doc) = parse(row.doc.as_deref()) else {
                continue;
            };
            out.sections.push(Item {
                id: row.id.clone(),
                doc,
            });
        }

        // Cards: the branch's rows over main's, in a section of the view.
        let mut main_cards = if alone {
            Vec::new()
        } else {
            self.rows("card_docs", card_key, project_id, "", over)?
        };
        if let Some(b) = branch.filter(|_| !alone) {
            self.as_made("card_docs", project_id, b, &mut main_cards)?;
        }
        let own_cards = match branch {
            Some(b) => self.rows("card_docs", card_key, project_id, b, over)?,
            None => Vec::new(),
        };
        // The working directories' sections, and the directories in them notes are put in.
        if !alone {
            let holders: HashSet<String> = main_cards
                .iter()
                .chain(own_cards.iter())
                .filter(|r| !r.removed)
                .map(|r| r.key.split('\u{1}').next().unwrap_or("").to_string())
                .collect();
            crate::workdir::compose_part(self.conn, project_id, &mut out, &holders)?;
        }
        let present: HashSet<String> = out.sections.iter().map(|s| s.id.clone()).collect();
        let own_card_ids: HashSet<&str> = own_cards.iter().map(|r| r.id.as_str()).collect();
        let own_card_keys: HashSet<&str> = own_cards
            .iter()
            .filter(|r| !r.removed)
            .map(|r| r.key.as_str())
            .collect();
        // Where a card's name is its own: its section, or the container it is inside (a
        // flow's nodes, a goal's cards), which has to be there as well.
        let holder_of = |key: &str| key.split('\u{1}').next().unwrap_or("").to_string();
        let mut there: HashSet<String> = own_cards
            .iter()
            .filter(|r| !r.removed)
            .map(|r| r.id.clone())
            .collect();
        let mut waiting: Vec<&Row> = Vec::new();
        for row in &main_cards {
            if own_card_ids.contains(row.id.as_str()) {
                continue;
            }
            if own_card_keys.contains(row.key.as_str()) {
                out.hidden.push(row.id.clone());
                continue;
            }
            waiting.push(row);
        }
        let mut taken: HashSet<String> = HashSet::new();
        loop {
            let before = waiting.len();
            waiting.retain(|row| {
                let holder = holder_of(&row.key);
                if present.contains(&holder) || there.contains(&holder) {
                    there.insert(row.id.clone());
                    taken.insert(row.id.clone());
                    false
                } else {
                    true
                }
            });
            if waiting.len() == before {
                break;
            }
        }
        out.hidden.extend(waiting.iter().map(|r| r.id.clone()));
        for row in main_cards.iter().filter(|r| taken.contains(&r.id)) {
            let Some(doc) = parse(row.doc.as_deref()) else {
                continue;
            };
            out.cards.push(Item {
                id: row.id.clone(),
                doc,
            });
        }
        for row in own_cards.iter().filter(|r| !r.removed) {
            let Some(doc) = parse(row.doc.as_deref()) else {
                continue;
            };
            out.cards.push(Item {
                id: row.id.clone(),
                doc,
            });
        }

        // Where the cards and their nodes sit, and how they look: the branch's over main's.
        let mut trees: Vec<&str> = Vec::new();
        if !alone {
            trees.push("");
        }
        if let Some(b) = branch {
            trees.push(b);
        }
        for tree in trees {
            self.boxes(project_id, tree, &mut out)?;
        }
        Ok(out)
    }

    /// One tree's boxes and looks, over what is there already.
    fn boxes(&self, project_id: &str, tree: &str, out: &mut Composition) -> Result<(), String> {
        type Boxed = (
            String,
            String,
            String,
            Option<i64>,
            Option<i64>,
            bool,
            Option<i64>,
            Option<i64>,
        );
        let mut stmt = self
            .conn
            .prepare(
                "SELECT view, item_kind, item, x, y, pinned, w, h FROM places WHERE project_id = ? AND branch_id = ?",
            )
            .map_err(sql)?;
        let boxes: Vec<Boxed> = stmt
            .query_map(params![project_id, tree], |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get::<_, i64>(5)? != 0,
                    r.get(6)?,
                    r.get(7)?,
                ))
            })
            .map_err(sql)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(sql)?;
        for (view, item_kind, item, x, y, pinned, w, h) in boxes {
            match item_kind.as_str() {
                "card" => {
                    let place = Place {
                        x: x.map(|n| n as f64),
                        y: y.map(|n| n as f64),
                        pinned,
                        size: w.map(|w| (w, h)),
                        view: None,
                        version: None,
                    };
                    out.places.insert((view, item), place);
                }
                "section" => {
                    let place = Place {
                        x: x.map(|n| n as f64),
                        y: y.map(|n| n as f64),
                        pinned,
                        size: w.map(|w| (w, h)),
                        view: None,
                        version: None,
                    };
                    out.section_ghost_places.insert((view, item), place);
                }
                "node" => {
                    if let (Some(x), Some(y)) = (x, y) {
                        out.node_places
                            .entry(view)
                            .or_default()
                            .insert(item, (x, y));
                    }
                }
                _ => {}
            }
        }
        let mut stmt = self
            .conn
            .prepare(
                "SELECT card_id, field, value FROM card_looks WHERE project_id = ? AND branch_id = ? AND row_path = '' AND field IN ('view', 'standalone', 'version')",
            )
            .map_err(sql)?;
        let looks: Vec<(String, String, String)> = stmt
            .query_map(params![project_id, tree], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })
            .map_err(sql)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(sql)?;
        for (card, field, value) in looks {
            let read = serde_json::from_str::<Value>(&value).ok();
            if field == "standalone" {
                out.standalone
                    .insert(card, read.and_then(|v| v.as_bool()).unwrap_or(false));
                continue;
            }
            let said = read.and_then(|v| v.as_str().map(str::to_string));
            let place = out.places.entry((String::new(), card)).or_insert(Place {
                x: None,
                y: None,
                pinned: false,
                size: None,
                view: None,
                version: None,
            });
            if field == "version" {
                place.version = said;
            } else {
                place.view = said;
            }
        }
        Ok(())
    }
}
