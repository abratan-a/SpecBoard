//! Branches made, renamed and let go (`Repository+Branches` in the Swift app): a branch
//! sees all of main and keeps only what it writes; what it remembers of main's cards
//! (`branch_prints`) is taken when it is made.

use super::{Tx, db};
use rusqlite::{OptionalExtension, params};
use serde_json::Value;

/// The name of a branch the app makes to show a merge's result (`create_result_branch`), and of
/// none other: a person's or an AI's may not begin with `~`.
pub const RESULT_PREFIX: &str = "~result-";

/// The name of the branch the window puts a locked main right in, in one go (`create_group_branch`).
pub const GROUP_BRANCH: &str = "~group";

/// A section's name (`validSectionName`): held to the rule of the names a path is made of, as a
/// card's is (`pathtext::name_problem`), its surrounding whitespace dropped.
pub fn valid_section_name(raw: &str) -> Result<String, String> {
    let what = text!("write.branches.what.section");
    let name = raw.trim().to_string();
    match crate::pathtext::name_problem(&name) {
        None => Ok(name),
        Some(crate::pathtext::NameProblem::Empty) => Err(text!("write.branches.nameEmpty", what)),
        Some(crate::pathtext::NameProblem::TooLong) => Err(text!(
            "write.names.tooLong",
            what,
            max = crate::pathtext::NAME_MAX
        )),
        Some(_) => Err(text!("write.names.control", what)),
    }
}

/// A name held to the rule of a branch's, an export's and a working directory's — not a path's
/// names: not empty, no path separators, 40 characters at most, one line — told as `what` names
/// it ("the branch's name", "the export's name"): the messages are made whole for each, never cut
/// and pasted.
pub fn valid_name(raw: &str, what: &str) -> Result<String, String> {
    let name = raw
        .trim_matches(|c: char| c == ' ' || c == '\t')
        .to_string();
    if name.is_empty() {
        return Err(text!("write.branches.nameEmpty", what));
    }
    if ['@', ':', '.', '/'].iter().any(|c| name.contains(*c)) {
        return Err(text!("write.branches.nameSeparators", what));
    }
    if name.chars().count() > 40 {
        return Err(text!("write.branches.nameTooLong", what));
    }
    if name.contains(['\n', '\r']) {
        return Err(text!("write.branches.nameNewline", what));
    }
    Ok(name)
}

impl Tx<'_> {
    /// A branch of the project by its name, in any case: (id, name).
    pub fn branch_named(
        &self,
        project: &str,
        name: &str,
    ) -> Result<Option<(String, String)>, String> {
        let mut stmt = self
            .conn()
            .prepare("SELECT id, name FROM branches WHERE project_id = ? ORDER BY sort_order, created_at, name")
            .map_err(db)?;
        let all = stmt
            .query_map([project], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })
            .map_err(db)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db)?;
        let wanted = name.trim();
        Ok(all
            .into_iter()
            .find(|(_, n)| crate::pathtext::same(n, wanted)))
    }

    /// A name a branch can have: one a section could, not `main` nor one wrapped in `_` (the
    /// board's and the working directories'), not one the app gives its own (`~`), and no other
    /// branch's of the project (§11).
    fn branch_name(
        &self,
        project: &str,
        raw: &str,
        excluding: Option<&str>,
    ) -> Result<String, String> {
        let name = valid_name(raw, &text!("write.branches.what.branch"))?;
        if crate::pathtext::same(&name, "main") {
            return Err(text!("write.branches.nameMain"));
        }
        // The board's, and what named the working directories before they were sections
        // (`_project_board_`, `_directory_`): the app's.
        if crate::kinds::reserved_name(&name) {
            return Err(text!(
                "write.branches.nameReserved",
                name,
                board = crate::store::BOARD_NAME,
                directory = crate::store::DIRECTORY_NAME
            ));
        }
        if name.starts_with('~') {
            return Err(text!("write.branches.nameTilde"));
        }
        if let Some((id, clash)) = self.branch_named(project, &name)?
            && Some(id.as_str()) != excluding
        {
            return Err(text!("write.branches.nameTaken", clash));
        }
        Ok(name)
    }

    /// A branch made: last in the project's order, remembering main's cards as they are now.
    pub fn create_branch(&mut self, project: &str, raw: &str) -> Result<String, String> {
        let exists = self
            .conn()
            .query_row("SELECT 1 FROM projects WHERE id = ?", [project], |_| Ok(()))
            .optional()
            .map_err(db)?
            .is_some();
        if !exists {
            return Err(text!("write.branches.projectNotFound"));
        }
        let name = self.branch_name(project, raw, None)?;
        let id = super::new_id();
        let order: i64 = self
            .conn()
            .query_row(
                "SELECT COALESCE(MAX(sort_order), -1) + 1 FROM branches WHERE project_id = ?",
                [project],
                |r| r.get(0),
            )
            .map_err(db)?;
        let at = self.now()?;
        self.conn()
            .execute(
                "INSERT INTO branches (id, project_id, name, sort_order, created_at) VALUES (?, ?, ?, ?, ?)",
                params![id, project, name, order, at],
            )
            .map_err(db)?;
        self.take_prints(&id, project)?;
        self.line(
            project,
            &id,
            "branch.create",
            "project",
            Some(&id),
            keyed!("write.branches.line.created", name),
        )?;
        Ok(id)
    }

    /// What the branch remembers of main's cards, taken now: each card's print and where
    /// it is (`takePrints`).
    pub fn take_prints(&mut self, branch: &str, project: &str) -> Result<(), String> {
        let detail = crate::detail::read(&self.reader(), project, None)?;
        let typed = crate::model::typed(&detail)?;
        let names = crate::export::Project::new(&typed);
        self.conn()
            .execute("DELETE FROM branch_prints WHERE branch_id = ?", [branch])
            .map_err(db)?;
        let mut stmt = self
            .conn()
            .prepare("SELECT id, doc FROM card_docs WHERE project_id = ? AND branch_id = '' AND removed = 0")
            .map_err(db)?;
        let rows = stmt
            .query_map([project], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?))
            })
            .map_err(db)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db)?;
        drop(stmt);
        for (card, doc) in rows {
            let Some(doc) = doc else { continue };
            let path = names.named(&card).unwrap_or_default();
            self.conn()
                .execute(
                    "INSERT INTO branch_prints (branch_id, card_id, print, path) VALUES (?, ?, ?, ?)",
                    params![branch, card, crate::schema::print(&doc), path],
                )
                .map_err(db)?;
        }
        Ok(())
    }

    /// The branch called something else, with a line in its history.
    pub fn rename_branch(&mut self, project: &str, id: &str, raw: &str) -> Result<String, String> {
        let was: String = self
            .conn()
            .query_row("SELECT name FROM branches WHERE id = ?", [id], |r| r.get(0))
            .map_err(|_| text!("write.branches.notFound"))?;
        let name = self.branch_name(project, raw, Some(id))?;
        if name == was {
            return Ok(name);
        }
        self.conn()
            .execute(
                "UPDATE branches SET name = ? WHERE id = ?",
                params![name, id],
            )
            .map_err(db)?;
        self.line(
            project,
            id,
            "branch.update",
            "project",
            Some(id),
            keyed!("write.branches.line.renamed", was, name),
        )?;
        Ok(name)
    }

    /// A branch the app makes to show what a merge into main would leave: made from main as it is now, its name the app's own
    /// (`~result-…`, no person's nor MCP's: not listed, not named), no line in any history. One at
    /// a time: one left before (a window gone before it let it go) goes first.
    pub fn create_result_branch(&mut self, project: &str) -> Result<String, String> {
        self.drop_result_branches(project)?;
        let order = self.next_branch_order(project)?;
        self.own_branch(project, &format!("{RESULT_PREFIX}{order}"))
    }

    /// Every branch the app made to show a merge's result, let go: what it holds and its history,
    /// with no line in main's.
    pub fn drop_result_branches(&mut self, project: &str) -> Result<(), String> {
        let ids: Vec<String> = {
            let mut stmt = self
                .conn()
                .prepare("SELECT id FROM branches WHERE project_id = ? AND name LIKE ?")
                .map_err(db)?;
            stmt.query_map(params![project, format!("{RESULT_PREFIX}%")], |r| r.get(0))
                .map_err(db)?
                .collect::<Result<_, _>>()
                .map_err(db)?
        };
        for id in ids {
            self.drop_own_branch(project, &id)?;
        }
        Ok(())
    }

    /// The branch the window puts a locked main right in, in one go, if it is there: its id.
    pub fn group_branch(&self, project: &str) -> Result<Option<String>, String> {
        self.conn()
            .query_row(
                "SELECT id FROM branches WHERE project_id = ? AND name = ?",
                params![project, GROUP_BRANCH],
                |r| r.get(0),
            )
            .optional()
            .map_err(db)
    }

    /// The branch for putting a locked main right in one go, made from main as it is now
    /// (`~group`, the app's own: not listed, named by the window only), no line in any history.
    pub fn create_group_branch(&mut self, project: &str) -> Result<String, String> {
        if let Some(id) = self.group_branch(project)? {
            return Ok(id);
        }
        self.own_branch(project, GROUP_BRANCH)
    }

    fn next_branch_order(&self, project: &str) -> Result<i64, String> {
        self.conn()
            .query_row(
                "SELECT COALESCE(MAX(sort_order), -1) + 1 FROM branches WHERE project_id = ?",
                [project],
                |r| r.get(0),
            )
            .map_err(db)
    }

    /// A branch of the app's own, made from main as it is now, with no line in any history.
    fn own_branch(&mut self, project: &str, name: &str) -> Result<String, String> {
        let id = super::new_id();
        let order = self.next_branch_order(project)?;
        let at = self.now()?;
        self.conn()
            .execute(
                "INSERT INTO branches (id, project_id, name, sort_order, created_at) VALUES (?, ?, ?, ?, ?)",
                params![id, project, name, order, at],
            )
            .map_err(db)?;
        self.take_prints(&id, project)?;
        Ok(id)
    }

    /// A branch of the app's own let go: what it holds and its history, with no line in main's.
    pub(crate) fn drop_own_branch(&mut self, project: &str, id: &str) -> Result<(), String> {
        for table in [
            "card_docs",
            "section_docs",
            "places",
            "card_looks",
            "commits",
            "holds",
            "branch_frozen",
        ] {
            self.conn()
                .execute(
                    &format!("DELETE FROM {table} WHERE project_id = ? AND branch_id = ?"),
                    params![project, id],
                )
                .map_err(db)?;
        }
        self.conn()
            .execute("DELETE FROM branch_prints WHERE branch_id = ?", [id])
            .map_err(db)?;
        self.conn()
            .execute("DELETE FROM branches WHERE id = ?", [id])
            .map_err(db)?;
        self.touch(project, id);
        Ok(())
    }

    /// The branch let go, with all it changed and its history; main says it went.
    pub fn delete_branch(&mut self, project: &str, id: &str) -> Result<(), String> {
        let Some(name) = self
            .conn()
            .query_row("SELECT name FROM branches WHERE id = ?", [id], |r| {
                r.get::<_, String>(0)
            })
            .optional()
            .map_err(db)?
        else {
            return Ok(());
        };
        for table in [
            "card_docs",
            "section_docs",
            "places",
            "card_looks",
            "commits",
            "holds",
            "branch_frozen",
        ] {
            self.conn()
                .execute(
                    &format!("DELETE FROM {table} WHERE project_id = ? AND branch_id = ?"),
                    params![project, id],
                )
                .map_err(db)?;
        }
        self.conn()
            .execute("DELETE FROM branches WHERE id = ?", [id])
            .map_err(db)?;
        self.touch(project, id);
        self.line(
            project,
            "",
            "branch.delete",
            "project",
            Some(id),
            keyed!("write.branches.line.deleted", name),
        )
    }

    /// How many cards and sections the branch has changed, made or removed.
    pub fn branch_change_count(&self, id: &str) -> Result<i64, String> {
        let count = |table: &str| -> Result<i64, String> {
            self.conn()
                .query_row(
                    &format!("SELECT COUNT(*) FROM {table} WHERE branch_id = ?"),
                    [id],
                    |r| r.get(0),
                )
                .map_err(db)
        };
        Ok(count("card_docs")? + count("section_docs")?)
    }
}

/// What a branch set aside is named by while it is: its id's head before its name, `~` first so
/// no list shows it and its name is free (`Tx::set_aside`).
const ASIDE: &str = "~aside-";

impl Tx<'_> {
    /// A branch merged into main set aside: in no list, its name free, its rows and history kept — a
    /// revert of that merge brings it back (`bring_back`), and it goes for good once the merge's
    /// commit is cut from main's history (`let_go_of_asides`).
    pub(crate) fn set_aside(&mut self, project: &str, id: &str) -> Result<(), String> {
        let now = self.now()?;
        self.conn()
            .execute(
                &format!(
                    "UPDATE branches SET archived_at = ?, name = '{ASIDE}' || substr(id, 1, 8) || '-' || name WHERE project_id = ? AND id = ? AND archived_at IS NULL"
                ),
                params![now, project, id],
            )
            .map_err(db)?;
        self.conn()
            .execute(
                "DELETE FROM branch_frozen WHERE project_id = ? AND branch_id = ?",
                params![project, id],
            )
            .map_err(db)?;
        self.touch(project, id);
        Ok(())
    }

    /// The branches a revert merged into main brings back (`branches.reverts` of `revert`): each
    /// set aside by a merge the revert undid, under its name again — another's now, with a number
    /// after it.
    pub(crate) fn bring_back(&mut self, project: &str, revert: &str) -> Result<(), String> {
        let listed: Option<String> = self
            .conn()
            .query_row(
                "SELECT reverts FROM branches WHERE project_id = ? AND id = ?",
                params![project, revert],
                |r| r.get(0),
            )
            .optional()
            .map_err(db)?
            .flatten();
        let ids: Vec<String> = listed
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        for id in ids {
            let aside: Option<String> = self
                .conn()
                .query_row(
                    "SELECT name FROM branches WHERE project_id = ? AND id = ? AND archived_at IS NOT NULL",
                    params![project, id],
                    |r| r.get(0),
                )
                .optional()
                .map_err(db)?;
            let Some(aside) = aside else {
                continue;
            };
            let name = aside
                .strip_prefix(ASIDE)
                .and_then(|rest| rest.get(9..))
                .unwrap_or(&aside)
                .to_string();
            let mut free = name.clone();
            let mut n = 2;
            while self.branch_named(project, &free)?.is_some() {
                free = format!("{name} ({n})");
                n += 1;
            }
            self.conn()
                .execute(
                    "UPDATE branches SET archived_at = NULL, name = ? WHERE project_id = ? AND id = ?",
                    params![free, project, id],
                )
                .map_err(db)?;
            self.touch(project, &id);
        }
        Ok(())
    }

    /// The branches set aside whose merge main no longer remembers (its commit cut from main's
    /// history, by the stop or by hand), let go of for good: there is nothing left to bring them
    /// back by. How many.
    pub(crate) fn let_go_of_asides(&mut self, project: &str) -> Result<usize, String> {
        let gone: Vec<String> = {
            let mut stmt = self
                .conn()
                .prepare(
                    "SELECT b.id FROM branches b WHERE b.project_id = ?1 AND b.archived_at IS NOT NULL
                     AND NOT EXISTS (SELECT 1 FROM commits c JOIN commit_lines l ON l.commit_id = c.id
                                     WHERE c.project_id = ?1 AND c.branch_id = '' AND c.reverted = 0
                                       AND l.kind = 'branch.merge' AND l.target_id = b.id)",
                )
                .map_err(db)?;
            stmt.query_map([project], |r| r.get(0))
                .map_err(db)?
                .collect::<Result<_, _>>()
                .map_err(db)?
        };
        for id in &gone {
            self.delete_branch(project, id)?;
        }
        Ok(gone.len())
    }
}

/// Whether `tree` names a branch (not main, not the board).
pub fn is_branch_tree(tree: &str) -> bool {
    !tree.is_empty() && !crate::store::is_own_tree(tree)
}

/// A document's text as stored, parsed.
pub fn parsed(text: Option<&str>) -> Option<Value> {
    text.and_then(|t| serde_json::from_str(t).ok())
}
