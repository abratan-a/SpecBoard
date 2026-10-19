//! Access keys, and what each may touch through MCP (`specboard://SpecBoard/Docs/Design:DesignAccess`).
//!
//! A request to `mcp` carries its key as `Authorization: Bearer <key>`; the key is found by
//! its digest (`Core::caller`), and a tool runs with who it is and what the tool needs bound
//! (`as_caller`), so the one place a project is looked up by name (`tools::project_summary_in`)
//! keeps to the projects the key may see, and refuses a write to one it may only read. Nothing
//! bound is the window — or a test inside this crate that calls a tool itself — which sees
//! every project and writes it.
//!
//! Besides the projects, a key says what else it may do — allowed, asked of a person each time, or
//! denied (`tools::mcp_writes::KEY_ITEMS`): choose and change how the window looks (`app_control`),
//! take pictures of it (`app_control`'s `capture`), and make a project (`project_create`). What MCP may
//! write in a project it may write is the project's own setting, apart from the key
//! (`tools::mcp_writes`): a call goes through when both let it, judged before it runs
//! (`mcp_writes::judge`).
//!
//! The window's requests (`Core::access_op`) list, make, rename, make again and revoke keys, give
//! each projects to read or write and what else it may do, and make projects public. No tool
//! changes any of it, so an agent cannot give itself more. This store's settings, like where a
//! project is written out: none of it goes into a project's file. A key's value is said once,
//! when it is made or made again; only its digest is kept.

use crate::Core;
use crate::store::sql;
use crate::tools::ProjectSummary;
use crate::tools::mcp_writes::KEY_ITEMS;
pub use crate::tools::mcp_writes::State;
use crate::write::{db, new_id};
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

/// What a request with no key, or with one this store does not have, is turned away with,
/// as HTTP 401 — by the agent's daemon too, before the body, when there is no key (`agent/src/http.rs`).
pub fn key_needed() -> String {
    text!("access.keyNeeded")
}

/// What a project the key may not see is told: the same as one that is not there
/// (`MCPTools.admit`).
fn no_project() -> String {
    text!("access.noProject")
}

/// What a write to a project the key may only read is told.
pub fn read_only(project: &str) -> String {
    text!("access.readOnly", project)
}

/// What a key may do with a project (`AccessLevel`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Read,
    Write,
}

impl Level {
    pub fn word(self) -> &'static str {
        match self {
            Level::Read => "read",
            Level::Write => "write",
        }
    }

    fn of(word: &str) -> Option<Level> {
        match word {
            "read" => Some(Level::Read),
            "write" => Some(Level::Write),
            _ => None,
        }
    }
}

/// Who is asking, as the entrance found them by their key (`Caller`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Caller {
    /// The key's id.
    pub key: String,
    pub name: String,
    pub permits: Permits,
}

/// What a key may do besides the projects it was given (`mcp_writes::KEY_ITEMS`). The window shows
/// any project, so what may be done with it is given apart from the projects.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Permits {
    /// Choose and change how the window looks (`app_control`).
    pub control: State,
    /// Take pictures of the window (`app_control`'s `capture`).
    pub capture: State,
    /// Make a project (`project_create`), which it then writes.
    pub create: State,
}

impl Permits {
    /// What a key made with nothing said may do: make a project, as every key could before the
    /// window's permits were split, and touch the window when a person says so each time.
    pub const MADE: Permits = Permits {
        control: State::Ask,
        capture: State::Ask,
        create: State::Allow,
    };

    /// Everything: the code from before keys, which could.
    pub const ALL: Permits = Permits {
        control: State::Allow,
        capture: State::Allow,
        create: State::Allow,
    };

    /// As the window reads them: `{"app_control": "ask", …}`, in the order of the items.
    fn said(self) -> Value {
        json!({
            "app_control": self.control.word(),
            "app_control.capture": self.capture.word(),
            "project_create": self.create.word(),
        })
    }
}

/// A key's column for one of its items.
fn column(item: &str) -> Option<&'static str> {
    match item {
        "app_control" => Some("can_control"),
        "app_control.capture" => Some("can_capture"),
        "project_create" => Some("can_create"),
        _ => None,
    }
}

/// One of a key's items set to a state (a person's "always" when asked; `mcp_writes::always`).
pub(crate) fn set_key_item(
    conn: &Connection,
    key: &str,
    item: &str,
    state: State,
) -> Result<(), String> {
    let column = column(item).ok_or_else(|| text!("access.keyItemUnknown", item))?;
    conn.execute(
        &format!("UPDATE access_keys SET {column} = ? WHERE id = ?"),
        params![state.column(), key],
    )
    .map(|_| ())
    .map_err(db)
}

/// A key's items as its row keeps them.
fn permits_of(r: &rusqlite::Row, at: usize) -> rusqlite::Result<Permits> {
    Ok(Permits {
        control: State::of_column(r.get(at)?),
        capture: State::of_column(r.get(at + 1)?),
        create: State::of_column(r.get(at + 2)?),
    })
}

/// What a tool needs of the project it names (`MCPAccess.Need`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Need {
    /// Project-free (`guide`), or answering only with what may be seen (`project_list`), or
    /// making what the key then writes (`project_create`).
    None,
    Read,
    Write,
    /// The window, where any project can show (`app_control`): what the key may do with it
    /// goes by the call's `op` (`Permits`).
    Drive,
}

/// What a tool needs (`specboard://SpecBoard/Docs/Design:DesignAccess` §3). A tool not listed
/// writes, so a new one is guarded until it says otherwise.
pub fn need(tool: &str) -> Need {
    match tool {
        "guide" | "project_list" | "project_create" => Need::None,
        "app_control" => Need::Drive,
        "project_read" | "section_list" | "section_read" | "card_list" | "card_read"
        | "tag_list" | "branch_list" | "branch_read" | "mr_list" | "mr_read" | "export_read"
        | "section_action" | "kind_list" | "kind_read" | "inspection_list" | "inspection_read" => {
            Need::Read
        }
        _ => Need::Write,
    }
}

// MARK: - The call

thread_local! {
    /// The tool call running on this thread: who asked, and what its tool needs
    /// (`MCPAccess.context`). A call runs on the thread that took it, start to end, so the
    /// tools it calls inside — a batch's steps — run with it too.
    static CONTEXT: RefCell<Option<(Caller, Need)>> = const { RefCell::new(None) };
}

/// What was bound before a call, put back when it is over — however it ends.
struct Bound(Option<(Caller, Need)>);

impl Drop for Bound {
    fn drop(&mut self) {
        let before = self.0.take();
        CONTEXT.with(|c| *c.borrow_mut() = before);
    }
}

fn context() -> Option<(Caller, Need)> {
    CONTEXT.with(|c| c.borrow().clone())
}

/// Who the call running on this thread is from; none for the window.
pub(crate) fn caller() -> Option<Caller> {
    context().map(|(caller, _)| caller)
}

/// A tool run for `caller`, with who it is and what it needs bound. What the
/// key may do besides the project the tool names is judged with the project's items, as the tool
/// starts (`mcp_writes::judge`).
pub(crate) fn as_caller<T>(
    caller: &Caller,
    tool: &str,
    run: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    let need = need(tool);
    let before = CONTEXT.with(|c| c.borrow_mut().replace((caller.clone(), need)));
    let _bound = Bound(before);
    run()
}

/// What the call's key may do with each project (`access(keyId:projectId:)`, for all of them at
/// once).
pub(crate) enum Sight {
    /// No key bound: the window, which writes every project.
    Whole,
    Key {
        /// Project → what the key was given.
        grants: HashMap<String, Level>,
        public: HashSet<String>,
    },
}

impl Sight {
    /// As the call running now sees the projects.
    pub(crate) fn of_call(conn: &Connection) -> Result<Sight, String> {
        let Some((caller, _)) = context() else {
            return Ok(Sight::Whole);
        };
        Ok(Sight::Key {
            grants: grants(conn, &caller.key)?,
            public: public_projects(conn)?.into_iter().collect(),
        })
    }

    /// What was given, or reading, if the project is public; none when the key may not see
    /// it at all.
    pub(crate) fn level(&self, project: &str) -> Option<Level> {
        match self {
            Sight::Whole => Some(Level::Write),
            Sight::Key { grants, public } => grants
                .get(project)
                .copied()
                .or_else(|| public.contains(project).then_some(Level::Read)),
        }
    }
}

/// The projects of `all` the call's key may see (`MCPTools.visibleProjects`).
pub(crate) fn visible(
    conn: &Connection,
    all: Vec<ProjectSummary>,
) -> Result<Vec<ProjectSummary>, String> {
    let sight = Sight::of_call(conn)?;
    Ok(all
        .into_iter()
        .filter(|p| sight.level(&p.id).is_some())
        .collect())
}

/// Refuses a project the call may not touch as its tool needs to: missing, for one its key may
/// not see; read-only, for a write to one it may only read (`MCPTools.admit`).
pub(crate) fn admit(conn: &Connection, project: &str, project_name: &str) -> Result<(), String> {
    let Some((_, need)) = context() else {
        return Ok(());
    };
    match Sight::of_call(conn)?.level(project) {
        None => Err(no_project()),
        Some(Level::Read) if need == Need::Write => Err(read_only(project_name)),
        Some(_) => Ok(()),
    }
}

/// Whether the call's key may see a project at all.
pub(crate) fn sees(conn: &Connection, project: &str) -> Result<bool, String> {
    Ok(Sight::of_call(conn)?.level(project).is_some())
}

/// A project made through a key is that key's to write (`specboard://SpecBoard/Docs/Design:DesignAccess` §2; the
/// `project_create` tool).
pub(crate) fn grant_made(conn: &Connection, project: &str) -> Result<(), String> {
    match context() {
        Some((caller, _)) => set_grant(conn, &caller.key, project, Some(Level::Write)),
        None => Ok(()),
    }
}

// MARK: - The entrance

/// The access key a request carries as `Authorization: Bearer …`, or none. Headers are by their
/// lower-case names.
fn bearer(headers: &Map<String, Value>) -> Option<&str> {
    let header = headers.get("authorization")?.as_str()?;
    const PREFIX: &str = "bearer ";
    if !header.get(..PREFIX.len())?.eq_ignore_ascii_case(PREFIX) {
        return None;
    }
    let value = header[PREFIX.len()..].trim();
    (!value.is_empty()).then_some(value)
}

/// A key's value as it is kept: its SHA-256, in hex (`AccessKeys.digest`).
pub fn digest(value: &str) -> String {
    Sha256::digest(value.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// `spk_` and 128 bits as hex: long enough not to be guessed, plain enough to paste, and
/// recognisable as this app's for anything that scans for secrets (`AccessKeys.freshValue`).
fn fresh_value() -> Result<String, String> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).map_err(|_| text!("access.noRandom"))?;
    Ok(format!(
        "spk_{}",
        bytes.iter().map(|b| format!("{b:02x}")).collect::<String>()
    ))
}

impl Core {
    /// Who a request is from, by the key it carries; none for no key, or a value no key of
    /// this store has (never made, or revoked) — to be turned away with `key_needed`. A key
    /// found is noted as used, at most once a minute, so a busy session does not write on
    /// every request.
    pub(crate) fn caller(&self, headers: &Map<String, Value>) -> Result<Option<Caller>, String> {
        let Some(value) = bearer(headers) else {
            return Ok(None);
        };
        let store = self.reading()?;
        let found = store
            .conn
            .query_row(
                "SELECT id, name, can_control, can_capture, can_create FROM access_keys WHERE digest = ?",
                [digest(value)],
                |r| {
                    Ok(Caller {
                        key: r.get(0)?,
                        name: r.get(1)?,
                        permits: permits_of(r, 2)?,
                    })
                },
            )
            .optional()
            .map_err(sql)?;
        drop(store);
        if let Some(caller) = &found {
            // Noted as used while the store is free; while a write holds it, once the write lets
            // it go (`Locked`): a request does not wait for a write to be told who it is from.
            match self.store.try_lock() {
                Ok(store) => note_used(&store.conn, &caller.key),
                Err(std::sync::TryLockError::Poisoned(e)) => {
                    note_used(&e.into_inner().conn, &caller.key)
                }
                Err(std::sync::TryLockError::WouldBlock) => self
                    .used
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .push(caller.key.clone()),
            }
        }
        Ok(found)
    }
}

/// A key noted as used, at most once a minute. Not being able to note it is no reason to turn the
/// request away (`markUsed`).
pub(crate) fn note_used(conn: &Connection, key: &str) {
    let _ = conn.execute(
        "UPDATE access_keys SET used_at = strftime('%Y-%m-%dT%H:%M:%fZ','now') \
         WHERE id = ? AND (used_at IS NULL OR used_at <= strftime('%Y-%m-%dT%H:%M:%fZ','now','-60 seconds'))",
        [key],
    );
}

// MARK: - Keys, grants and public projects

/// Project → what the key was given. Public projects it was given nothing for are not here
/// (`grants(keyId:)`).
fn grants(conn: &Connection, key: &str) -> Result<HashMap<String, Level>, String> {
    let mut stmt = conn
        .prepare("SELECT project_id, level FROM access_grants WHERE key_id = ?")
        .map_err(sql)?;
    let rows = stmt
        .query_map([key], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })
        .map_err(sql)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sql)?;
    Ok(rows
        .into_iter()
        .map(|(project, level)| (project, Level::of(&level).unwrap_or(Level::Read)))
        .collect())
}

/// None takes the grant away (`setGrant`).
fn set_grant(
    conn: &Connection,
    key: &str,
    project: &str,
    level: Option<Level>,
) -> Result<(), String> {
    match level {
        None => conn
            .execute(
                "DELETE FROM access_grants WHERE key_id = ? AND project_id = ?",
                [key, project],
            )
            .map(|_| ())
            .map_err(db),
        Some(level) => conn
            .execute(
                "INSERT INTO access_grants (key_id, project_id, level) VALUES (?1, ?2, ?3) \
                 ON CONFLICT (key_id, project_id) DO UPDATE SET level = excluded.level",
                params![key, project, level.word()],
            )
            .map(|_| ())
            .map_err(db),
    }
}

/// The projects every key may read (`publicProjects`).
fn public_projects(conn: &Connection) -> Result<Vec<String>, String> {
    let mut stmt = conn
        .prepare("SELECT project_id FROM project_public ORDER BY project_id")
        .map_err(sql)?;
    stmt.query_map([], |r| r.get(0))
        .map_err(sql)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sql)
}

fn set_public(conn: &Connection, project: &str, on: bool) -> Result<(), String> {
    conn.execute("DELETE FROM project_public WHERE project_id = ?", [project])
        .map_err(db)?;
    if on {
        conn.execute(
            "INSERT INTO project_public (project_id) VALUES (?)",
            [project],
        )
        .map_err(db)?;
    }
    Ok(())
}

/// A project's grants and whether it is public: this store's, not a project file's, so kept over
/// a project read in from a file in its place (`accessOf(projectId:)`).
pub(crate) struct Kept {
    grants: Vec<(String, Level)>,
    public: bool,
}

pub(crate) fn kept(conn: &Connection, project: &str) -> Result<Kept, String> {
    let mut stmt = conn
        .prepare("SELECT key_id, level FROM access_grants WHERE project_id = ? ORDER BY key_id")
        .map_err(sql)?;
    let grants = stmt
        .query_map([project], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })
        .map_err(sql)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sql)?
        .into_iter()
        .map(|(key, level)| (key, Level::of(&level).unwrap_or(Level::Read)))
        .collect();
    let public = conn
        .query_row(
            "SELECT 1 FROM project_public WHERE project_id = ?",
            [project],
            |_| Ok(()),
        )
        .optional()
        .map_err(sql)?
        .is_some();
    Ok(Kept { grants, public })
}

/// What `kept` took, put back on the project (`restoreAccess`).
pub(crate) fn restore(conn: &Connection, project: &str, kept: &Kept) -> Result<(), String> {
    for (key, level) in &kept.grants {
        set_grant(conn, key, project, Some(*level))?;
    }
    set_public(conn, project, kept.public)
}

/// A key's name: there, and 80 characters at the most (`Validate.required`).
fn key_name(raw: &str) -> Result<String, String> {
    let value = raw.trim().to_string();
    if value.chars().count() > 80 {
        return Err(text!("access.keyNameTooLong"));
    }
    if value.is_empty() {
        return Err(text!("access.keyNameEmpty"));
    }
    Ok(value)
}

/// A key as the window lists it, with what it may do and the projects it was given.
fn key_row(conn: &Connection, id: &str) -> Result<Value, String> {
    type Row = (String, Permits, String, Option<String>);
    let (name, permits, created_at, used_at): Row = conn
        .query_row(
            "SELECT name, can_control, can_capture, can_create, created_at, used_at FROM access_keys WHERE id = ?",
            [id],
            |r| Ok((r.get(0)?, permits_of(r, 1)?, r.get(4)?, r.get(5)?)),
        )
        .map_err(sql)?;
    let mut given: Vec<(String, Level)> = grants(conn, id)?.into_iter().collect();
    given.sort();
    let given: Map<String, Value> = given
        .into_iter()
        .map(|(project, level)| (project, level.word().into()))
        .collect();
    Ok(json!({
        "id": id,
        "name": name,
        "permits": permits.said(),
        "createdAt": created_at,
        "usedAt": used_at,
        "grants": given,
    }))
}

/// A new key for `value`, with its record (`adoptAccessKey`).
fn adopt(conn: &Connection, name: &str, value: &str, permits: Permits) -> Result<String, String> {
    let name = key_name(name)?;
    let id = new_id();
    conn.execute(
        "INSERT INTO access_keys (id, name, digest, can_control, can_capture, can_create) VALUES (?, ?, ?, ?, ?, ?)",
        params![
            id,
            name,
            digest(value),
            permits.control.column(),
            permits.capture.column(),
            permits.create.column()
        ],
    )
    .map_err(db)?;
    Ok(id)
}

/// The key's items a request names (`permits`: `{"app_control": "allow" | "ask" | "deny", …}`), each
/// over what `over` says: none named, none changed. An item or a state not known is refused.
fn permits_over(request: &Value, op: &str, over: Permits) -> Result<(Permits, usize), String> {
    let mut out = over;
    let mut named = 0;
    let given = match request.get("permits") {
        None | Some(Value::Null) => return Ok((out, 0)),
        Some(Value::Object(given)) => given,
        Some(other) => {
            return Err(text!("access.permitsShape", op, other));
        }
    };
    for (item, said) in given {
        let state = said
            .as_str()
            .and_then(State::of)
            .ok_or_else(|| text!("access.permitsState", op, item, said))?;
        let slot = match item.as_str() {
            "app_control" => &mut out.control,
            "app_control.capture" => &mut out.capture,
            "project_create" => &mut out.create,
            _ => {
                return Err(text!(
                    "access.keyItemUnknownAmong",
                    item,
                    known = crate::text::List::of(KEY_ITEMS.iter().map(|i| i.id))
                ));
            }
        };
        *slot = state;
        named += 1;
    }
    Ok((out, named))
}

fn text<'a>(request: &'a Value, key: &str, op: &str, what: &str) -> Result<&'a str, String> {
    request
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| text!("access.needs", op, key, what))
}

fn flag(request: &Value, key: &str, op: &str, what: &str) -> Result<bool, String> {
    request
        .get(key)
        .and_then(Value::as_bool)
        .ok_or_else(|| text!("access.needs", op, key, what))
}

fn key_there(conn: &Connection, key: &str) -> Result<(), String> {
    let there = conn
        .query_row("SELECT 1 FROM access_keys WHERE id = ?", [key], |_| Ok(()))
        .optional()
        .map_err(sql)?
        .is_some();
    if there {
        Ok(())
    } else {
        Err(text!("access.keyNotFound"))
    }
}

fn project_there(conn: &Connection, project: &str) -> Result<(), String> {
    let there = conn
        .query_row("SELECT 1 FROM projects WHERE id = ?", [project], |_| Ok(()))
        .optional()
        .map_err(sql)?
        .is_some();
    if there {
        Ok(())
    } else {
        Err(text!("access.projectNotFound"))
    }
}

impl Core {
    /// The window's requests about access keys and public projects; none when `op` is not one
    /// of them.
    pub(crate) fn access_op(&self, op: &str, request: &Value) -> Option<Result<Value, String>> {
        let known = [
            "access_keys",
            "access_key_create",
            "access_key_rename",
            "access_key_permit",
            "access_key_revoke",
            "access_key_regenerate",
            "access_grant",
            "project_public",
            "project_public_set",
        ];
        known
            .contains(&op)
            .then(|| self.access_request(op, request))
    }

    fn access_request(&self, op: &str, request: &Value) -> Result<Value, String> {
        let mut store = self.lock();
        let conn = &mut store.conn;
        match op {
            "access_keys" => {
                let ids: Vec<String> = {
                    let mut stmt = conn
                        .prepare("SELECT id FROM access_keys ORDER BY created_at, name")
                        .map_err(sql)?;
                    stmt.query_map([], |r| r.get(0))
                        .map_err(sql)?
                        .collect::<Result<_, _>>()
                        .map_err(sql)?
                };
                let keys = ids
                    .iter()
                    .map(|id| key_row(conn, id))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(json!({ "keys": keys }))
            }
            "access_key_create" => {
                let name = text(request, "name", op, &text!("access.what.keyName"))?;
                let (permits, _) = permits_over(request, op, Permits::MADE)?;
                let value = fresh_value()?;
                let id = adopt(conn, name, &value, permits)?;
                Ok(json!({ "key": key_row(conn, &id)?, "value": value }))
            }
            "access_key_rename" => {
                let key = text(request, "key", op, &text!("access.what.keyId"))?;
                let name = key_name(text(request, "name", op, &text!("access.what.keyName"))?)?;
                key_there(conn, key)?;
                conn.execute("UPDATE access_keys SET name = ? WHERE id = ?", [&name, key])
                    .map_err(db)?;
                Ok(json!({}))
            }
            "access_key_permit" => {
                // Only what is named changes.
                let key = text(request, "key", op, &text!("access.what.keyId"))?;
                key_there(conn, key)?;
                let had = conn
                    .query_row(
                        "SELECT can_control, can_capture, can_create FROM access_keys WHERE id = ?",
                        [key],
                        |r| permits_of(r, 0),
                    )
                    .map_err(sql)?;
                let (permits, named) = permits_over(request, op, had)?;
                if named == 0 {
                    return Err(text!(
                        "access.permitsNeeded",
                        op,
                        known = crate::text::List::of(KEY_ITEMS.iter().map(|i| i.id))
                    ));
                }
                conn.execute(
                    "UPDATE access_keys SET can_control = ?, can_capture = ?, can_create = ? WHERE id = ?",
                    params![
                        permits.control.column(),
                        permits.capture.column(),
                        permits.create.column(),
                        key
                    ],
                )
                .map_err(db)?;
                Ok(json!({ "key": key_row(conn, key)? }))
            }
            "access_key_revoke" => {
                // Gone at once, with what it was given: the next request with it is refused.
                let key = text(request, "key", op, &text!("access.what.keyId"))?;
                key_there(conn, key)?;
                conn.execute("DELETE FROM access_keys WHERE id = ?", [key])
                    .map_err(db)?;
                Ok(json!({}))
            }
            "access_key_regenerate" => {
                // The same key — its name, what it may do and the projects it was given — with a
                // new value, handed out this once: the one before is refused from the next request on,
                // as a revoked one is, and it has not been used yet.
                let key = text(request, "key", op, &text!("access.what.keyId"))?;
                key_there(conn, key)?;
                let value = fresh_value()?;
                conn.execute(
                    "UPDATE access_keys SET digest = ?, used_at = NULL WHERE id = ?",
                    params![digest(&value), key],
                )
                .map_err(db)?;
                Ok(json!({ "key": key_row(conn, key)?, "value": value }))
            }
            "access_grant" => {
                let key = text(request, "key", op, &text!("access.what.keyId"))?;
                let project = text(request, "project", op, &text!("access.what.projectId"))?;
                let level = match request.get("level") {
                    None | Some(Value::Null) => None,
                    Some(Value::String(word)) => Some(
                        Level::of(word).ok_or_else(|| text!("access.levelWord", value = word))?,
                    ),
                    Some(other) => {
                        return Err(text!("access.levelWord", value = other));
                    }
                };
                key_there(conn, key)?;
                project_there(conn, project)?;
                set_grant(conn, key, project, level)?;
                Ok(json!({}))
            }
            "project_public" => Ok(json!({ "projects": public_projects(conn)? })),
            "project_public_set" => {
                let project = text(request, "project", op, &text!("access.what.projectId"))?;
                let on = flag(request, "on", op, &text!("access.what.on"))?;
                project_there(conn, project)?;
                set_public(conn, project, on)?;
                Ok(json!({}))
            }
            _ => unreachable!(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers(value: &str) -> Map<String, Value> {
        let mut out = Map::new();
        out.insert("authorization".into(), value.into());
        out
    }

    #[test]
    fn a_bearer_is_read_as_the_swift_app_reads_it() {
        assert_eq!(bearer(&headers("Bearer spk_1")), Some("spk_1"));
        assert_eq!(bearer(&headers("bearer  spk_1 ")), Some("spk_1"));
        assert_eq!(bearer(&headers("BEARER spk_1")), Some("spk_1"));
        assert_eq!(bearer(&headers("Bearer ")), None);
        assert_eq!(bearer(&headers("Bearer    ")), None);
        assert_eq!(bearer(&headers("Basic spk_1")), None);
        assert_eq!(bearer(&headers("spk_1")), None);
        assert_eq!(bearer(&Map::new()), None);
    }

    #[test]
    fn a_value_is_spk_and_32_hex_digits_and_kept_as_its_digest() {
        let value = fresh_value().unwrap();
        assert!(value.starts_with("spk_") && value.len() == 36, "{value}");
        assert!(
            value[4..]
                .chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
        );
        assert_ne!(fresh_value().unwrap(), value);
        // SHA-256 in lower-case hex, as the Swift app keeps it (`%02x`).
        assert_eq!(
            digest("abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn what_each_tool_needs_is_the_swift_apps() {
        assert_eq!(need("guide"), Need::None);
        assert_eq!(need("project_list"), Need::None);
        assert_eq!(need("project_create"), Need::None);
        assert_eq!(need("app_control"), Need::Drive);
        for tool in [
            "project_read",
            "section_list",
            "section_read",
            "card_list",
            "card_read",
            "tag_list",
            "branch_list",
            "branch_read",
            "mr_list",
            "mr_read",
            "export_read",
            "section_action",
            "kind_list",
            "kind_read",
            "inspection_list",
            "inspection_read",
        ] {
            assert_eq!(need(tool), Need::Read, "{tool}");
        }
        for tool in [
            "project_update",
            "export_create",
            "export_update",
            "export_delete",
            "export_execute",
            "card_update",
            "branch_merge",
            "branch_revert",
            "batch",
            "mr_delete",
            "inspection_update",
            "a_tool_not_made_yet",
        ] {
            assert_eq!(need(tool), Need::Write, "{tool}");
        }
    }

    #[test]
    fn a_call_is_bound_while_it_runs_and_let_go_after_even_when_refused() {
        let caller = Caller {
            key: "k".into(),
            name: "キー".into(),
            permits: Permits {
                control: State::Deny,
                capture: State::Deny,
                create: State::Deny,
            },
        };
        assert!(context().is_none());
        let seen = as_caller(&caller, "card_update", || Ok(context())).unwrap();
        assert_eq!(seen, Some((caller.clone(), Need::Write)));
        assert!(context().is_none());
        let refused: Result<(), String> = as_caller(&caller, "card_read", || Err("断る".into()));
        assert_eq!(refused.unwrap_err(), "断る");
        assert!(context().is_none());
    }

    #[test]
    fn a_keys_items_are_named_by_their_ids_and_kept_as_one_zero_and_minus_one() {
        let (permits, named) = permits_over(
            &json!({"permits": {"app_control.capture": "allow", "project_create": "deny"}}),
            "access_key_permit",
            Permits::MADE,
        )
        .unwrap();
        assert_eq!(named, 2);
        assert_eq!(
            permits.said(),
            json!({"app_control": "ask", "app_control.capture": "allow", "project_create": "deny"})
        );
        assert_eq!(
            permits_over(&json!({}), "access_key_permit", Permits::ALL).unwrap(),
            (Permits::ALL, 0)
        );
        for (request, said) in [
            (
                json!({"permits": {"app_control": true}}),
                "access_key_permit の permits の app_control は allow・ask・deny のどれかです（true ではなく）",
            ),
            (
                json!({"permits": {"card_update": "allow"}}),
                "アクセスキーの項目「card_update」はありません（ある: app_control・app_control.capture・project_create）",
            ),
            (
                json!({"permits": ["allow"]}),
                "access_key_permit の permits は {項目: allow・ask・deny} です（[\"allow\"] ではなく）",
            ),
        ] {
            assert_eq!(
                permits_over(&request, "access_key_permit", Permits::MADE).unwrap_err(),
                said
            );
        }
        assert_eq!(column("app_control"), Some("can_control"));
        assert_eq!(column("app_control.capture"), Some("can_capture"));
        assert_eq!(column("project_create"), Some("can_create"));
        assert_eq!(column("card_update"), None);
    }
}
