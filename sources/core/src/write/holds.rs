//! Holding a tree while it is put right in one go (`specboard://SpecBoard/Docs/Design:DesignBranches` §5): the
//! window's "まとめて直す", or an MCP batch. While one side holds a tree, the other side's
//! writes to its documents are refused where the commit is made (`Gathered::flush`); the
//! same side's writes, and writes to other trees, go through. And main's lock (§8), and
//! the batches MCP projects before it runs them.

use super::branches::GROUP_BRANCH;
use super::merge::{Side, Take};
use super::{Tx, db};
use crate::store::BOARD;
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Map, Value};

/// A hold on a tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hold {
    pub project: String,
    /// `""` for main.
    pub tree: String,
    /// `screen` or `mcp`.
    pub holder: String,
    /// The batch's token, for MCP; empty for the window.
    pub token: String,
    pub since: String,
    /// The window's: the last commit before it began, which "やめる" goes back to.
    pub from_seq: i64,
    /// The window's: the commit its writes are added to.
    pub commit_id: Option<String>,
    /// The window's: the Mac it is on (`Who::client`); empty for the app that holds the store.
    pub client: String,
}

impl Hold {
    /// Whether `who` is the hand that holds it: the same side, and for the window the same Mac.
    pub fn is_of(&self, who: &super::Who) -> bool {
        self.holder == who.side.word() && (self.holder != "screen" || self.client == who.client)
    }
}

/// The holds on a tree now. An MCP one holds only while its batch is there (a batch left
/// untouched is cleared, `BATCH_LIFETIME_DAYS`).
pub fn holds(conn: &Connection, project: &str, tree: &str) -> Result<Vec<Hold>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT project_id, branch_id, holder, token, since, from_seq, commit_id, client FROM holds
              WHERE project_id = ? AND branch_id = ? AND (holder = 'screen' OR token IN (SELECT token FROM batches))
              ORDER BY since",
        )
        .map_err(db)?;
    stmt.query_map(params![project, tree], |r| {
        Ok(Hold {
            project: r.get(0)?,
            tree: r.get(1)?,
            holder: r.get(2)?,
            token: r.get(3)?,
            since: r.get(4)?,
            from_seq: r.get(5)?,
            commit_id: r.get(6)?,
            client: r.get(7)?,
        })
    })
    .map_err(db)?
    .collect::<Result<Vec<_>, _>>()
    .map_err(db)
}

/// The tree's name as people call it: main, the board (as the window says it: told to both
/// sides), or the branch's name.
pub(crate) fn tree_name(conn: &Connection, tree: &str) -> String {
    match tree {
        "" => "main".to_string(),
        BOARD => text!("write.holds.boardName"),
        id => conn
            .query_row("SELECT name FROM branches WHERE id = ?", [id], |r| r.get(0))
            .unwrap_or_else(|_| "main".to_string()),
    }
}

/// When a hold began, as the banner says it: the time today, the day and time before.
fn since_label(conn: &Connection, since: &str) -> String {
    conn.query_row(
        "SELECT CASE WHEN date(?1, 'localtime') = date('now', 'localtime') THEN ''
                ELSE CAST(strftime('%m', ?1, 'localtime') AS INTEGER) || '/' || CAST(strftime('%d', ?1, 'localtime') AS INTEGER) || ' ' END
                || CAST(strftime('%H', ?1, 'localtime') AS INTEGER) || ':' || strftime('%M', ?1, 'localtime')",
        [since],
        |r| r.get::<_, Option<String>>(0),
    )
    .ok()
    .flatten()
    .unwrap_or_else(|| since.to_string())
}

/// What a write from the other side is told.
pub fn refusal(conn: &Connection, hold: &Hold) -> String {
    let name = tree_name(conn, &hold.tree);
    let since = since_label(conn, &hold.since);
    if hold.holder == "mcp" {
        text!("write.holds.refusalMcp", name, since)
    } else if hold.client.is_empty() {
        text!("write.holds.refusalApp", name, since)
    } else {
        text!(
            "write.holds.refusalAppOn",
            name,
            since,
            client = hold.client
        )
    }
}

/// The name a locked main's group is kept under when the app opens and it cannot be merged.
fn left_group() -> String {
    text!("write.holds.leftGroup")
}

/// Batches left untouched this long are cleared when a new one is opened.
pub const BATCH_LIFETIME_DAYS: i64 = 7;

/// A batch being planned: a project to write against, the tree its steps write in, and the
/// steps put together so far. Nothing here changes a project: the steps are only run inside
/// a transaction that is undone, until the batch is run for real.
#[derive(Clone, Debug, PartialEq)]
pub struct Batch {
    pub token: String,
    pub project: String,
    /// `""` for main.
    pub tree: String,
    pub steps: Vec<(String, Map<String, Value>)>,
    pub checked_at: String,
}

impl Tx<'_> {
    /// The holds on a tree now.
    pub fn holds(&self, project: &str, tree: &str) -> Result<Vec<Hold>, String> {
        holds(self.conn(), project, tree)
    }

    /// Refused while another hand holds the tree: the other side, or another Mac's window.
    fn require_free(&self, project: &str, tree: &str) -> Result<(), String> {
        if let Some(other) = self
            .holds(project, tree)?
            .iter()
            .find(|h| !h.is_of(&self.who))
        {
            return Err(refusal(self.conn(), other));
        }
        Ok(())
    }

    /// The Mac whose window writes (`Who::client`).
    fn client(&self) -> String {
        self.who.client.clone()
    }

    fn last_seq(&self, project: &str, tree: &str) -> Result<i64, String> {
        self.conn()
            .query_row(
                "SELECT COALESCE(MAX(seq), 0) FROM commits WHERE project_id = ? AND branch_id = ?",
                params![project, tree],
                |r| r.get(0),
            )
            .map_err(db)
    }

    /// The window starts putting the tree right in one go: what it writes there, until
    /// it ends, is one commit, and MCP may not write there. A locked main is put right in a
    /// branch of the app's own (`~group`), held with main, and merged into main when it ends: its id.
    pub fn begin_group(&mut self, project: &str, tree: &str) -> Result<Option<String>, String> {
        if tree.is_empty() && self.locked_at(project)?.is_some() {
            self.require_free(project, "")?;
            if let Some(id) = self.group_branch(project)? {
                return Ok(Some(id));
            }
            let id = self.create_group_branch(project)?;
            for held in ["", id.as_str()] {
                self.hold_for_screen(project, held)?;
            }
            return Ok(Some(id));
        }
        self.require_free(project, tree)?;
        if self
            .holds(project, tree)?
            .iter()
            .any(|h| h.holder == "screen")
        {
            return Ok(None);
        }
        self.hold_for_screen(project, tree)?;
        Ok(None)
    }

    fn hold_for_screen(&mut self, project: &str, tree: &str) -> Result<(), String> {
        let last = self.last_seq(project, tree)?;
        let at = self.now()?;
        self.conn()
            .execute(
                "INSERT INTO holds (project_id, branch_id, holder, token, since, from_seq, client) VALUES (?, ?, 'screen', '', ?, ?, ?)",
                params![project, tree, at, last, self.client()],
            )
            .map_err(db)?;
        self.touch(project, tree);
        Ok(())
    }

    /// The group on a locked main the window means by `tree`: its branch, or main itself — its
    /// own (another Mac's is that one's to end).
    fn locked_group(&self, project: &str, tree: &str) -> Result<Option<String>, String> {
        let Some(id) = self
            .group_branch(project)?
            .filter(|id| tree.is_empty() || tree == id)
        else {
            return Ok(None);
        };
        let holds = self.holds(project, &id)?;
        Ok((holds.is_empty() || holds.iter().any(|h| h.is_of(&self.who))).then_some(id))
    }

    /// The window is done: what it wrote stays, as the commit it made. On a locked main, what
    /// it wrote in its branch is merged into main, one commit there, and the branch goes.
    pub fn end_group(&mut self, project: &str, tree: &str) -> Result<(), String> {
        if let Some(id) = self.locked_group(project, tree)? {
            return self.merge_group(project, &id);
        }
        self.conn()
            .execute(
                "DELETE FROM holds WHERE project_id = ? AND branch_id = ? AND holder = 'screen' AND client = ?",
                params![project, tree, self.client()],
            )
            .map_err(db)?;
        self.touch(project, tree);
        Ok(())
    }

    /// What the window put right in a locked main's group branch, merged into main: where both
    /// changed a unit, the group's is taken, and the boxes it moved go along. How cards look
    /// goes too (it makes no commit). Then the branch and the holds go.
    fn merge_group(&mut self, project: &str, id: &str) -> Result<(), String> {
        let preview = self.merge_preview(project, id, "", &Take::new())?;
        let mut take = Take::new();
        for conflict in &preview.conflicts {
            take.entry(conflict.path.clone())
                .or_default()
                .insert(conflict.label.clone(), Side::Source);
        }
        if !preview.is_empty() || preview.places > 0 {
            self.merge(project, id, "", &take, true, true)?;
        }
        self.conn()
            .execute(
                "INSERT INTO card_looks (project_id, branch_id, card_id, row_path, field, value)
                 SELECT project_id, '', card_id, row_path, field, value FROM card_looks WHERE project_id = ? AND branch_id = ?
                 ON CONFLICT(project_id, branch_id, card_id, row_path, field) DO UPDATE SET value = excluded.value",
                params![project, id],
            )
            .map_err(db)?;
        self.let_group_go(project, id)
    }

    fn let_group_go(&mut self, project: &str, id: &str) -> Result<(), String> {
        self.conn()
            .execute(
                "DELETE FROM holds WHERE project_id = ? AND branch_id IN ('', ?) AND holder = 'screen' AND client = ?",
                params![project, id, self.client()],
            )
            .map_err(db)?;
        self.drop_own_branch(project, id)?;
        self.touch(project, "");
        Ok(())
    }

    /// The window gives up: the tree goes back to where it began, as a revert, and is let
    /// go. Returns how many commits were undone. On a locked main, its branch just goes, and
    /// main is as it was.
    pub fn abandon_group(&mut self, project: &str, tree: &str) -> Result<usize, String> {
        if let Some(id) = self.locked_group(project, tree)? {
            let undone = self.last_seq(project, &id)? as usize;
            self.let_group_go(project, &id)?;
            return Ok(undone);
        }
        let Some(hold) = self
            .holds(project, tree)?
            .into_iter()
            .find(|h| h.holder == "screen" && h.is_of(&self.who))
        else {
            return Ok(0);
        };
        let last = self.last_seq(project, tree)?;
        let undone = if last > hold.from_seq {
            self.revert(project, tree, hold.from_seq, None)?
        } else {
            0
        };
        self.end_group(project, tree)?;
        Ok(undone)
    }

    /// When the app opens: what the window held when it last closed is let go (its writes
    /// are already one commit). A locked main's group is closed as the window would have: merged
    /// into main; one that cannot be (it would hold two things at a path) is kept as a branch a
    /// person can see, to be merged by hand. Only this Mac's (`Who::client`): another's window
    /// may be putting things right now.
    pub fn release_screen_holds(&mut self) -> Result<(), String> {
        let client = self.client();
        let groups: Vec<(String, String)> = {
            let mut stmt = self
                .conn()
                .prepare(
                    "SELECT project_id, id FROM branches WHERE name = ?1
                       AND (id IN (SELECT branch_id FROM holds WHERE holder = 'screen' AND client = ?2)
                            OR id NOT IN (SELECT branch_id FROM holds WHERE holder = 'screen'))",
                )
                .map_err(db)?;
            stmt.query_map(params![GROUP_BRANCH, client], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .map_err(db)?
            .collect::<Result<_, _>>()
            .map_err(db)?
        };
        for (project, id) in groups {
            if self.merge_group(&project, &id).is_err() {
                let name = self.free_branch_name(&project, &left_group())?;
                self.conn()
                    .execute(
                        "UPDATE branches SET name = ? WHERE id = ?",
                        params![name, id],
                    )
                    .map_err(db)?;
                self.line(
                    &project,
                    "",
                    "branch.create",
                    "project",
                    Some(&id),
                    keyed!("write.holds.line.leftGroup", name),
                )?;
            }
        }
        self.conn()
            .execute(
                "DELETE FROM holds WHERE holder = 'screen' AND client = ?",
                [client],
            )
            .map_err(db)?;
        Ok(())
    }

    /// `base`, or `base 2`, `base 3`… — the first no branch of the project has.
    fn free_branch_name(&self, project: &str, base: &str) -> Result<String, String> {
        let mut name = base.to_string();
        let mut n = 2;
        while self.branch_named(project, &name)?.is_some() {
            name = format!("{base} {n}");
            n += 1;
        }
        Ok(name)
    }

    /// A batch opened against a project's tree, holding it from now until it is run,
    /// discarded or cleared — refused while the window is putting that tree right in one
    /// go. Batches left untouched for `BATCH_LIFETIME_DAYS` are cleared first.
    pub fn open_batch(&mut self, project: &str, tree: &str) -> Result<String, String> {
        let conn = self.conn();
        conn.execute(
            "DELETE FROM batches WHERE updated_at < strftime('%Y-%m-%dT%H:%M:%fZ', 'now', ?)",
            [format!("-{BATCH_LIFETIME_DAYS} days")],
        )
        .map_err(db)?;
        conn.execute(
            "DELETE FROM holds WHERE holder = 'mcp' AND token NOT IN (SELECT token FROM batches)",
            [],
        )
        .map_err(db)?;
        let token = format!("batch-{}", super::new_id());
        conn.execute(
            "INSERT INTO batches (token, project_id, branch_id) VALUES (?, ?, ?)",
            params![token, project, (!tree.is_empty()).then_some(tree)],
        )
        .map_err(db)?;
        self.require_free(project, tree)?;
        let at = self.now()?;
        self.conn()
            .execute(
                "INSERT INTO holds (project_id, branch_id, holder, token, since) VALUES (?, ?, 'mcp', ?, ?)",
                params![project, tree, token, at],
            )
            .map_err(db)?;
        self.touch(project, tree);
        Ok(token)
    }

    /// A batch, if it is still there.
    pub fn batch(&self, token: &str) -> Result<Option<Batch>, String> {
        batch(self.conn(), token)
    }

    /// The steps replaced with these, and noted as last known to run now.
    pub fn save_batch(
        &mut self,
        token: &str,
        steps: &[(String, Map<String, Value>)],
    ) -> Result<(), String> {
        self.conn()
            .execute("DELETE FROM batch_steps WHERE token = ?", [token])
            .map_err(db)?;
        for (i, (tool, arguments)) in steps.iter().enumerate() {
            self.conn()
                .execute(
                    "INSERT INTO batch_steps (token, idx, tool, arguments) VALUES (?, ?, ?, ?)",
                    params![
                        token,
                        i as i64,
                        tool,
                        Value::Object(arguments.clone()).to_string()
                    ],
                )
                .map_err(db)?;
        }
        self.mark_checked(token)
    }

    /// The batch noted as last known to run now.
    pub fn mark_checked(&mut self, token: &str) -> Result<(), String> {
        let at = self.now()?;
        self.conn()
            .execute(
                "UPDATE batches SET updated_at = ?1, checked_at = ?1 WHERE token = ?2",
                params![at, token],
            )
            .map_err(db)?;
        Ok(())
    }

    /// The batch goes, and with it its hold on the tree.
    pub fn close_batch(&mut self, token: &str) -> Result<(), String> {
        if let Some(b) = self.batch(token)? {
            self.touch(&b.project, &b.tree);
        }
        self.conn()
            .execute("DELETE FROM batches WHERE token = ?", [token])
            .map_err(db)?;
        self.conn()
            .execute(
                "DELETE FROM holds WHERE holder = 'mcp' AND token = ?",
                [token],
            )
            .map_err(db)?;
        Ok(())
    }

    /// main locked or let go, with a line in main's history. Only from the window.
    pub fn set_main_lock(&mut self, project: &str, on: bool) -> Result<(), String> {
        let row: Option<(String, Option<String>, Option<String>)> = self
            .conn()
            .query_row(
                "SELECT name, archived_at, locked_at FROM projects WHERE id = ?",
                [project],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()
            .map_err(db)?;
        let Some((name, archived, locked)) = row else {
            return Err(text!("write.holds.projectNotFound"));
        };
        if archived.is_some() {
            return Err(text!("write.holds.projectArchived", name));
        }
        if locked.is_some() == on {
            return Ok(());
        }
        if !on && self.group_branch(project)?.is_some() {
            return Err(text!("write.holds.groupUnlock"));
        }
        // Another Mac's window putting main right in one go: that one's to end first.
        if on
            && let Some(other) = self
                .holds(project, "")?
                .iter()
                .find(|h| h.holder == "screen" && !h.is_of(&self.who))
        {
            return Err(refusal(self.conn(), other));
        }
        let at = self.now()?;
        self.conn()
            .execute(
                "UPDATE projects SET locked_at = ? WHERE id = ?",
                params![on.then_some(at), project],
            )
            .map_err(db)?;
        // What the window was putting right in main in one go ends here: its "やめる" goes
        // back, which a locked main refuses. What it wrote stays, as its commit.
        if on {
            self.conn()
                .execute(
                    "DELETE FROM holds WHERE project_id = ? AND branch_id = '' AND holder = 'screen' AND client = ?",
                    params![project, self.client()],
                )
                .map_err(db)?;
        }
        let (kind, summary) = if on {
            ("lock", keyed!("write.holds.line.lock"))
        } else {
            ("unlock", keyed!("write.holds.line.unlock"))
        };
        self.line(project, "", kind, "project", None, summary)
    }

    /// Whether an AI's merge into a locked main goes in without being approved.
    pub fn ai_merge(&self, project: &str) -> Result<bool, String> {
        ai_merge(self.conn(), project)
    }

    /// An AI's merges into a locked main let in without being approved, or held for a person
    /// again, with a line in
    /// main's history. Only from the window: what the project lets MCP do is not MCP's to change.
    pub fn set_ai_merge(&mut self, project: &str, on: bool) -> Result<(), String> {
        let row: Option<(String, Option<String>)> = self
            .conn()
            .query_row(
                "SELECT name, archived_at FROM projects WHERE id = ?",
                [project],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(db)?;
        let Some((name, archived)) = row else {
            return Err(text!("write.holds.projectNotFound"));
        };
        if archived.is_some() {
            return Err(text!("write.holds.projectArchived", name));
        }
        if self.ai_merge(project)? == on {
            return Ok(());
        }
        self.conn()
            .execute(
                "UPDATE projects SET ai_merge = ? WHERE id = ?",
                params![on as i64, project],
            )
            .map_err(db)?;
        let summary = if on {
            keyed!("write.holds.line.aiMergeOn")
        } else {
            keyed!("write.holds.line.aiMergeOff")
        };
        self.line(
            project,
            "",
            "project.update",
            "project",
            Some(project),
            summary,
        )
    }
}

/// Whether an AI's merge into a locked main goes in without being approved.
pub fn ai_merge(conn: &Connection, project: &str) -> Result<bool, String> {
    Ok(conn
        .query_row(
            "SELECT ai_merge FROM projects WHERE id = ?",
            [project],
            |r| r.get::<_, i64>(0),
        )
        .optional()
        .map_err(db)?
        .unwrap_or(0)
        != 0)
}

/// A batch, if it is still there.
pub fn batch(conn: &Connection, token: &str) -> Result<Option<Batch>, String> {
    let Some((project, checked_at, tree)) = conn
        .query_row(
            "SELECT project_id, checked_at, branch_id FROM batches WHERE token = ?",
            [token],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, Option<String>>(2)?,
                ))
            },
        )
        .optional()
        .map_err(db)?
    else {
        return Ok(None);
    };
    let mut stmt = conn
        .prepare("SELECT tool, arguments FROM batch_steps WHERE token = ? ORDER BY idx")
        .map_err(db)?;
    let steps = stmt
        .query_map([token], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })
        .map_err(db)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(db)?
        .into_iter()
        .map(|(tool, arguments)| {
            (
                tool,
                crate::store::parse(Some(&arguments)).unwrap_or_default(),
            )
        })
        .collect();
    Ok(Some(Batch {
        token: token.to_string(),
        project,
        tree: tree.unwrap_or_default(),
        steps,
        checked_at,
    }))
}
