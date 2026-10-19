//! The window's requests about the history, holding and whole projects, through
//! `Core::call`: the commits of a tree, going back, cutting the history short, a name on
//! a commit, main's lock, putting a tree right in one go and the holds on it; a project file
//! read in, the archive, and a project deleted. Everything here is the window's
//! (`Who::screen`); what MCP does goes through its tools.

use crate::Core;
use crate::store::Store;
use crate::tools::branches::Undid;
use crate::write::{self, Who};
use rusqlite::OptionalExtension;
use serde_json::{Value, json};

/// How a tree is called (`main`, the board's name, a branch's name) for saying a unit of it.
fn tree_called(conn: &rusqlite::Connection, project: &str, tree: &str) -> Result<String, String> {
    if tree.is_empty() {
        return Ok("main".to_string());
    }
    if tree == crate::store::BOARD {
        return Ok(crate::store::BOARD_NAME.to_string());
    }
    let found = conn
        .query_row(
            "SELECT name FROM branches WHERE project_id = ? AND id = ?",
            [project, tree],
            |r| r.get::<_, String>(0),
        )
        .optional()
        .map_err(crate::store::sql)?;
    Ok(found.unwrap_or_else(|| tree.to_string()))
}

pub(crate) fn text<'a>(
    request: &'a Value,
    key: &str,
    op: &str,
    what: &str,
) -> Result<&'a str, String> {
    request
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| text!("ops.error.needs", op, key, what))
}

pub(crate) fn number(request: &Value, key: &str, op: &str, what: &str) -> Result<i64, String> {
    request
        .get(key)
        .and_then(Value::as_i64)
        .ok_or_else(|| text!("ops.error.needs", op, key, what))
}

/// The project and the tree a request is about: `branch` left out or `""` for main, a
/// branch's id, or `~board`.
pub(crate) fn tree<'a>(
    store: &Store,
    request: &'a Value,
    op: &str,
) -> Result<(&'a str, &'a str), String> {
    let project = text(request, "project", op, &text!("ops.what.projectId"))?;
    if store.project(project)?.is_none() {
        return Err(text!("ops.error.projectNotFound"));
    }
    let tree = request.get("branch").and_then(Value::as_str).unwrap_or("");
    if !tree.is_empty()
        && !crate::store::is_own_tree(tree)
        && !store.branch_exists(project, tree)?
    {
        return Err(text!("ops.error.branchNotFound", tree));
    }
    Ok((project, tree))
}

impl Core {
    /// The requests of this module; none when `op` is not one of them.
    pub(crate) fn history_op(&self, op: &str, request: &Value) -> Option<Result<Value, String>> {
        let known = [
            "commits",
            "revert",
            "undo_commit",
            "cut",
            "name_commit",
            "main_lock",
            "main_ai_merge",
            "group_begin",
            "group_end",
            "group_abandon",
            "batches_cancel",
            "holds",
            "project_file_peek",
            "project_file_read",
            "project_bundle",
            "project_bundle_read",
            "project_archive_copy",
            "project_archive",
            "project_restore",
            "project_delete",
        ];
        known
            .contains(&op)
            .then(|| self.history_request(op, request))
    }

    fn history_request(&self, op: &str, request: &Value) -> Result<Value, String> {
        match op {
            "commits" => {
                let store = self.reading()?;
                let (project, tree) = tree(&store, request, op)?;
                let limit = request.get("limit").and_then(Value::as_i64).unwrap_or(200);
                let commits = write::history_of(&store.conn, project, tree, limit)?;
                Ok(json!({ "commits": commits.iter().map(|c| json!({
                    "seq": c.seq,
                    "at": c.at,
                    "authorKind": c.author_kind,
                    "author": c.author,
                    "kind": c.kind,
                    "name": c.name,
                    "reverted": c.reverted,
                    "lines": c.lines.iter().map(|l| json!({
                        "kind": l.kind,
                        "targetType": l.target_type,
                        "targetId": l.target_id,
                        "summary": l.summary,
                    })).collect::<Vec<_>>(),
                })).collect::<Vec<_>>() }))
            }
            "holds" => {
                let store = self.reading()?;
                let (project, tree) = tree(&store, request, op)?;
                let holds = write::holds_on(&store.conn, project, tree)?;
                Ok(json!({ "holds": holds.iter().map(|h| json!({
                    "holder": h.holder,
                    "token": h.token,
                    "since": h.since,
                    "fromSeq": h.from_seq,
                    "client": h.client,
                })).collect::<Vec<_>>() }))
            }
            "project_file_peek" => {
                let text = text(request, "text", op, &text!("ops.what.fileText"))?;
                let (id, name) =
                    write::peek(text).ok_or_else(|| text!("ops.error.notProjectFile"))?;
                let there = self.reading()?.project(&id)?.is_some();
                Ok(json!({ "id": id, "name": name, "there": there }))
            }
            "project_file_read" => {
                let file = text(request, "text", op, &text!("ops.what.fileText"))?;
                let landing = match request.get("landing").and_then(Value::as_str) {
                    Some("copy") => write::Landing::Copy {
                        name: text(request, "name", op, &text!("ops.what.copyName"))?.to_string(),
                    },
                    Some("replace") | None => write::Landing::Replacing,
                    Some(other) => {
                        return Err(text!("ops.error.landing", other));
                    }
                };
                let archive_first = request
                    .get("archiveFirst")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                self.plain_writing(Who::screen_for(request), |tx| {
                    // Over a project that is there: a copy of it kept, archived, first.
                    let mut archived = None;
                    if archive_first
                        && landing == write::Landing::Replacing
                        && let Some((id, _)) = write::peek(file)
                        && tx.project_exists(&id)?
                    {
                        archived = Some(tx.archive_copy(&id)?);
                    }
                    // Reached by the same access keys as before: this store's, not the
                    // file's (`Repository.replace`).
                    let kept = match write::peek(file) {
                        Some((id, _)) if landing == write::Landing::Replacing => {
                            Some(crate::access::kept(tx.conn(), &id)?)
                        }
                        _ => None,
                    };
                    let project = tx.read_project_file(file, &landing, false)?;
                    if let Some(kept) = kept {
                        crate::access::restore(tx.conn(), &project, &kept)?;
                    }
                    Ok(json!({ "project": project, "archived": archived }))
                })
            }
            // A project whole, to move it to another store and back (`write::bundle`).
            "project_bundle" => {
                let project = text(request, "project", op, &text!("ops.what.projectId"))?;
                write::bundle(&self.reading()?.conn, project)
            }
            "project_bundle_read" => {
                let bundle = request
                    .get("bundle")
                    .filter(|b| b.is_object())
                    .ok_or_else(|| {
                        text!(
                            "ops.error.needs",
                            op,
                            key = "bundle",
                            what = text!("ops.what.bundle")
                        )
                    })?;
                self.plain_writing(Who::screen_for(request), |tx| {
                    tx.read_bundle(bundle)
                        .map(|project| json!({ "project": project }))
                })
            }
            "project_archive_copy" | "project_archive" | "project_restore" | "project_delete" => {
                let project =
                    text(request, "project", op, &text!("ops.what.projectId"))?.to_string();
                if self.lock().project(&project)?.is_none() {
                    return Err(text!("ops.error.projectNotFound"));
                }
                self.plain_writing(Who::screen_for(request), |tx| match op {
                    "project_archive_copy" => Ok(json!({ "project": tx.archive_copy(&project)? })),
                    "project_archive" => tx.archive_project(&project).map(|_| json!({})),
                    "project_restore" => tx.restore_project(&project).map(|_| json!({})),
                    _ => tx.delete_project(&project).map(|_| json!({})),
                })
            }
            _ => self.writing(
                Who::screen_for(request),
                request,
                op,
                |tx, project, tree| match op {
                    "revert" => {
                        let to = number(request, "to", op, &text!("ops.what.revertTo"))?;
                        // main is gone back through a branch merged in.
                        if tree.is_empty() {
                            let name = request.get("name").and_then(Value::as_str);
                            let (id, named, undone) =
                                tx.revert_into_branch(project, to, name, None)?;
                            return Ok(json!({ "undone": undone, "branch": id, "name": named }));
                        }
                        Ok(json!({ "undone": tx.revert(project, tree, to, None)? }))
                    }
                    // One commit taken back alone; where a later commit changed what it changed
                    // too the answer says so (nothing written) for the window to show.
                    "undo_commit" => {
                        let seq = number(request, "seq", op, &text!("ops.what.commitSeq"))?;
                        let name = request.get("name").and_then(Value::as_str);
                        let project_name = tx
                            .reader()
                            .project(project)?
                            .map(|p| p.name)
                            .unwrap_or_default();
                        let tree_name = tree_called(tx.conn(), project, tree)?;
                        let undid = crate::tools::branches::undo_in(
                            tx,
                            project,
                            &project_name,
                            tree,
                            &tree_name,
                            seq,
                            name,
                            None,
                            false,
                        )?;
                        Ok(match undid {
                            Undid::Done {
                                undone,
                                made: Some((id, named)),
                                changed,
                                ..
                            } => json!({
                                "undone": undone, "branch": id, "name": named, "changed": changed,
                            }),
                            Undid::Done {
                                undone,
                                commit,
                                changed,
                                ..
                            } => json!({ "undone": undone, "commit": commit, "changed": changed }),
                            Undid::Conflicts { conflicts, later } => {
                                json!({ "conflicts": conflicts, "later": later })
                            }
                        })
                    }
                    "cut" => {
                        let before = number(request, "before", op, &text!("ops.what.cutBefore"))?;
                        Ok(json!({ "cut": tx.cut(project, tree, before)? }))
                    }
                    "name_commit" => {
                        let seq = number(request, "seq", op, &text!("ops.what.commitSeq"))?;
                        let name = request.get("name").and_then(Value::as_str).unwrap_or("");
                        tx.name_commit(project, tree, seq, name)?;
                        Ok(json!({}))
                    }
                    "main_lock" => {
                        let on = request.get("on").and_then(Value::as_bool).ok_or_else(|| {
                            text!(
                                "ops.error.needs",
                                op = "main_lock",
                                key = "on",
                                what = text!("ops.what.lockOn")
                            )
                        })?;
                        tx.set_main_lock(project, on)?;
                        Ok(json!({}))
                    }
                    "main_ai_merge" => {
                        let on = request.get("on").and_then(Value::as_bool).ok_or_else(|| {
                            text!(
                                "ops.error.needs",
                                op = "main_ai_merge",
                                key = "on",
                                what = text!("ops.what.aiMergeOn")
                            )
                        })?;
                        tx.set_ai_merge(project, on)?;
                        Ok(json!({}))
                    }
                    // On a locked main, the branch of the app's own it is put right in.
                    "group_begin" => tx.begin_group(project, tree).map(|branch| match branch {
                        Some(id) => json!({ "branch": id }),
                        None => json!({}),
                    }),
                    "group_end" => tx.end_group(project, tree).map(|_| json!({})),
                    "group_abandon" => Ok(json!({ "undone": tx.abandon_group(project, tree)? })),
                    "batches_cancel" => {
                        // A person's hand comes first: MCP's batches holding the tree go.
                        let tokens: Vec<String> = tx
                            .holds(project, tree)?
                            .into_iter()
                            .filter(|h| h.holder == "mcp")
                            .map(|h| h.token)
                            .collect();
                        for token in &tokens {
                            tx.close_batch(token)?;
                        }
                        Ok(json!({ "cancelled": tokens.len() }))
                    }
                    _ => unreachable!(),
                },
            ),
        }
    }

    /// One transaction of writes by `who` in the project and tree `request` names, committed
    /// when `act` goes through; then each tree it wrote is told about
    /// (`{"op": "changed", "project", "branch", "rev"}`).
    pub(crate) fn writing<T>(
        &self,
        who: Who,
        request: &Value,
        op: &str,
        act: impl FnOnce(&mut write::Tx<'_>, &str, &str) -> Result<T, String>,
    ) -> Result<T, String> {
        let (project, tree) = {
            let store = self.lock();
            let (project, tree) = tree(&store, request, op)?;
            (project.to_string(), tree.to_string())
        };
        self.plain_writing(who, |tx| act(tx, &project, &tree))
    }

    /// One transaction of writes by `who`, committed when `act` goes through; then each
    /// tree it wrote is told about.
    pub(crate) fn plain_writing<T>(
        &self,
        who: Who,
        act: impl FnOnce(&mut write::Tx<'_>) -> Result<T, String>,
    ) -> Result<T, String> {
        let (out, touched, code_lines) = {
            let mut store = self.lock();
            let mut tx = store.write(who.clone())?;
            let out = act(&mut tx)?;
            let code_lines = tx.take_code_lines();
            (out, tx.commit()?, code_lines)
        };
        // The code of the lines to files it made or pointed elsewhere, recorded now (before the
        // trees are told of, so a reading after the notice finds it).
        if !code_lines.is_empty() {
            crate::tools::code_review::record_written(self, &who, &code_lines);
        }
        for (project, branch) in touched {
            let rev = self.next_rev();
            self.tell(
                &json!({ "op": "changed", "project": project, "branch": branch, "rev": rev }),
            );
        }
        Ok(out)
    }
}
