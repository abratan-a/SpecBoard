//! A card's actions (`card_action`, `specboard://SpecBoard/Docs/Design:DesignBoard` §9): what a goal
//! or a task is made to do in one go — a goal started (its branch made), finished or cancelled (its
//! branch let go of, the goal moved to where finished or cancelled goals are kept), a task done — in
//! one transaction, so the branch, the card's fields and where it is never disagree. A part of a
//! mock (html, svg, frame) is marked reviewed, written as its own change so its review marks go; a
//! card with lines to working directories' files (`implementedBy`) is reviewed too, whatever its
//! kind, recording their code as it is now (`code_review`) — no change of the card, so no commit.
//!
//! Which a kind has is its definition's (`actions`); what each does is the core's. Whether one can be
//! used now is worked out by `usable` alone, for `card_action` and for what a read says
//! (`card_read`'s `include: ["actions"]`), so the window and MCP never tell it apart.

use super::Seen;
use super::args::Args;
use super::cards::Scope;
use super::code_review::CodeLine;
use crate::export::Project;
use crate::model::Card;
use crate::store::{BOARD, BOARD_NAME, Reader};
use crate::text::Keyed;
use crate::write::Tx;
use serde_json::{Map, Value, json};

/// A goal's `status` at its place among the choices its definition writes (not started, in
/// progress, finished, cancelled): the values a card keeps, the same in every language.
#[derive(Clone, Copy)]
enum Status {
    NotStarted = 0,
    InProgress = 1,
    Finished = 2,
    Cancelled = 3,
}

impl Status {
    fn value(self) -> String {
        crate::kinds::builtin()
            .get("goal")
            .and_then(|k| k.field("status"))
            .and_then(|f| f.options.get(self as usize))
            .cloned()
            .unwrap_or_default()
    }

    fn is(self, said: &str) -> bool {
        self.value() == said
    }
}

/// The longest name a branch may have (`write::valid_name`).
const BRANCH_MOST: usize = 40;

/// The branch a goal's `workBranch` names, as it stands.
#[derive(Debug, Clone)]
enum Work {
    /// It names none.
    Unset,
    /// A branch of the project still worked in: its id and name, how many of its cards and sections
    /// differ from main, and the number of its request to merge still open.
    Live {
        id: String,
        name: String,
        changes: usize,
        request: Option<i64>,
    },
    /// A branch set aside by a merge, or gone: nothing of it is left to work in.
    Gone(String),
}

/// The branches goals name, worked out once however many goals are looked at: a branch compared
/// with main costs as much as a project's reading, and many goals name the same branch or none.
/// The parts' review marks too, from one reading of the tree's history when many cards are looked
/// at (`many`), else of the one part's.
pub(crate) struct Works {
    /// The tree the cards are read in: `""` main, a branch's id, the board.
    tree: String,
    many: bool,
    branches: std::cell::RefCell<Option<Vec<(String, String)>>>,
    named: std::cell::RefCell<std::collections::HashMap<String, Work>>,
    review: std::cell::RefCell<Option<super::review::Marks>>,
    /// The tree's lines to files and how each one's code stands, by card: read once when many
    /// cards are looked at.
    code: std::cell::RefCell<Option<std::collections::HashMap<String, Vec<CodeLine>>>>,
}

impl Works {
    pub(crate) fn new(tree: &str, many: bool) -> Works {
        Works {
            tree: tree.to_string(),
            many,
            branches: Default::default(),
            named: Default::default(),
            review: Default::default(),
            code: Default::default(),
        }
    }

    /// The card's lines to files and how each one's code stands: those of every card read once
    /// when many are looked at.
    fn code_of(
        &self,
        reader: &Reader,
        project_id: &str,
        project: &Project,
        card: &Card,
    ) -> Result<Vec<CodeLine>, String> {
        if !self.many {
            return super::code_review::lines(reader.conn, project_id, project, Some(&card.id));
        }
        if self.code.borrow().is_none() {
            let all = super::code_review::by_card(super::code_review::lines(
                reader.conn,
                project_id,
                project,
                None,
            )?);
            *self.code.borrow_mut() = Some(all);
        }
        Ok(self
            .code
            .borrow()
            .as_ref()
            .and_then(|all| all.get(&card.id))
            .cloned()
            .unwrap_or_default())
    }

    /// The part's marks: those of every part read once when many are looked at.
    fn review_of(
        &self,
        reader: &Reader,
        project_id: &str,
        project: &Project,
        card: &Card,
    ) -> Result<Vec<super::review::Mark>, String> {
        if !self.many {
            let mut one =
                super::review::marks(reader.conn, project_id, &self.tree, project, Some(&card.id))?;
            return Ok(one.remove(&card.id).unwrap_or_default());
        }
        if self.review.borrow().is_none() {
            let all = super::review::marks(reader.conn, project_id, &self.tree, project, None)?;
            *self.review.borrow_mut() = Some(all);
        }
        Ok(self
            .review
            .borrow()
            .as_ref()
            .and_then(|all| all.get(&card.id))
            .cloned()
            .unwrap_or_default())
    }
}

/// What an action looks at of a card.
struct Facts<'a> {
    card: &'a Card,
    /// The actions its kind has.
    ops: Vec<String>,
    /// A goal's `status` (its default when not written).
    status: Option<String>,
    /// A task's `done`.
    done: Option<bool>,
    work: Work,
    /// A part's review marks: the cards its lines reach that changed after it.
    review: Vec<super::review::Mark>,
    /// Its lines to working directories' files, and how each one's code stands here.
    code: Vec<CodeLine>,
}

fn field<'c>(card: &'c Card, name: &str) -> Option<&'c Value> {
    card.form.as_ref()?.get("fields")?.get(name)
}

/// What the call refuses with when the action may not be used now: its words, and what is said
/// beside them (`mcp::refusal`).
struct Why {
    said: String,
    beside: Value,
}

impl Why {
    fn plain(said: String) -> Why {
        Why {
            said,
            beside: Value::Null,
        }
    }
}

/// The card's facts, its branch read through `reader`.
fn facts<'a>(
    reader: &Reader,
    project_id: &str,
    project: &Project,
    card: &'a Card,
    works: &Works,
) -> Result<Facts<'a>, String> {
    let kind = project.defs.cards().get(&card.kind);
    let own_ops = kind.map(|k| k.actions.clone()).unwrap_or_default();
    // A line to a file may leave a card of any kind: reviewing it follows the line.
    let code = works.code_of(reader, project_id, project, card)?;
    let mut ops = own_ops.clone();
    if !code.is_empty() && !ops.iter().any(|o| o == "reviewed") {
        ops.push("reviewed".to_string());
    }
    let status = kind.and_then(|k| k.field("status")).map(|f| {
        field(card, "status")
            .and_then(Value::as_str)
            .map(str::to_string)
            .or_else(|| {
                f.default
                    .as_ref()
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .unwrap_or_else(|| Status::NotStarted.value())
    });
    let done = kind.and_then(|k| k.field("done")).map(|_| {
        field(card, "done")
            .and_then(Value::as_bool)
            .unwrap_or(false)
    });
    let named = kind
        .and_then(|k| k.field("workBranch"))
        .and_then(|_| field(card, "workBranch"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|n| !n.is_empty());
    let work = match named {
        None => Work::Unset,
        Some(named) => work_of(reader, project_id, named, works)?,
    };
    let review = if own_ops.iter().any(|o| o == "reviewed") {
        works.review_of(reader, project_id, project, card)?
    } else {
        Vec::new()
    };
    Ok(Facts {
        card,
        ops,
        status,
        done,
        work,
        review,
        code,
    })
}

/// The branch `named` names, as it stands.
fn work_of(reader: &Reader, project_id: &str, named: &str, works: &Works) -> Result<Work, String> {
    if let Some(known) = works.named.borrow().get(named) {
        return Ok(known.clone());
    }
    let work = work_now(reader, project_id, named, works)?;
    works
        .named
        .borrow_mut()
        .insert(named.to_string(), work.clone());
    Ok(work)
}

fn work_now(reader: &Reader, project_id: &str, named: &str, works: &Works) -> Result<Work, String> {
    if works.branches.borrow().is_none() {
        let all = super::branches(reader.conn, project_id)?;
        *works.branches.borrow_mut() = Some(all);
    }
    let live = works
        .branches
        .borrow()
        .iter()
        .flatten()
        .find(|(_, n)| crate::pathtext::same(n, named))
        .cloned();
    let Some((id, name)) = live else {
        return Ok(Work::Gone(named.to_string()));
    };
    let own: i64 = reader
        .conn
        .query_row(
            "SELECT (SELECT COUNT(*) FROM card_docs WHERE branch_id = ?1) + (SELECT COUNT(*) FROM section_docs WHERE branch_id = ?1)",
            [&id],
            |r| r.get(0),
        )
        .map_err(crate::store::sql)?;
    // What it keeps of its own may be as main has it: only what differs is a change.
    let changes = if own == 0 {
        0
    } else {
        super::branches::branch_diff(reader, project_id, &id, false)?.changes()
    };
    let request: Option<i64> = reader
        .conn
        .query_row(
            "SELECT number FROM merge_requests WHERE project_id = ? AND branch_id = ? AND state = 'open' ORDER BY number DESC LIMIT 1",
            [project_id, &id],
            |r| r.get(0),
        )
        .map(Some)
        .or_else(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            other => Err(crate::store::sql(other)),
        })?;
    Ok(Work::Live {
        id,
        name,
        changes,
        request,
    })
}

/// An action as people call it, in the call's language (`builtin.json`'s `actions`).
pub(crate) fn written(op: &str) -> Option<&'static Value> {
    crate::kinds::builtin_root()["actions"]
        .as_array()?
        .iter()
        .find(|a| a["action"] == op)
}

pub(crate) fn label(op: &str) -> String {
    written(op)
        .and_then(|a| a["label"].as_str())
        .unwrap_or(op)
        .to_string()
}

/// A kind's actions as `kind_read` and `guide` say them: `[{op, label, description, value}]`, `value`
/// the keys the action's value takes. None for a kind with none.
pub(crate) fn described(ops: &[String]) -> Option<Value> {
    if ops.is_empty() {
        return None;
    }
    Some(Value::Array(
        ops.iter()
            .map(|op| {
                let mut out = Map::new();
                out.insert("op".into(), op.clone().into());
                if let Some(a) = written(op) {
                    out.insert("label".into(), a["label"].clone());
                    out.insert("description".into(), a["description"].clone());
                    out.insert("value".into(), a["value"].clone());
                }
                Value::Object(out)
            })
            .collect(),
    ))
}

/// The name `start` gives a goal's branch when it names none (`branch_name_of`) — but not the name of
/// the branch the goal still names that was set aside or is gone: another branch of that name would
/// be taken for it. Then `-2`, `-3`… the first no branch has.
fn start_name(
    conn: &rusqlite::Connection,
    project_id: &str,
    card: &str,
    work: &Work,
) -> Result<String, String> {
    let base = branch_name_of(card);
    let Work::Gone(gone) = work else {
        return Ok(base);
    };
    if !crate::pathtext::same(&base, gone) {
        return Ok(base);
    }
    let names: Vec<String> = super::branches(conn, project_id)?
        .into_iter()
        .map(|(_, n)| n)
        .collect();
    Ok((2..)
        .map(|n| {
            let tail = format!("-{n}");
            let room = BRANCH_MOST.saturating_sub(tail.chars().count());
            let head: String = base.chars().take(room).collect();
            format!("{}{tail}", head.trim_end_matches('-'))
        })
        .find(|c| {
            !crate::pathtext::same(c, gone) && !names.iter().any(|n| crate::pathtext::same(n, c))
        })
        .expect("a free name"))
}

/// The name a goal's branch is given when `start` names none: the card's name cut into words (at
/// spaces and marks, and where a capital begins one — `McpPictures`, `MCPTools`), lower-case, joined
/// with `-`, at most as long as a branch's name may be.
pub(crate) fn branch_name_of(name: &str) -> String {
    let chars: Vec<char> = name.chars().collect();
    let mut words: Vec<String> = Vec::new();
    let mut word = String::new();
    for (i, &c) in chars.iter().enumerate() {
        if !c.is_alphanumeric() {
            if !word.is_empty() {
                words.push(std::mem::take(&mut word));
            }
            continue;
        }
        if c.is_uppercase() && !word.is_empty() {
            let before = chars[i - 1];
            let next_lower = chars.get(i + 1).is_some_and(|n| n.is_lowercase());
            if !before.is_uppercase() || next_lower {
                words.push(std::mem::take(&mut word));
            }
        }
        word.extend(c.to_lowercase());
    }
    if !word.is_empty() {
        words.push(word);
    }
    let mut out = String::new();
    for w in words {
        let more = if out.is_empty() { 0 } else { 1 } + w.chars().count();
        if out.chars().count() + more > BRANCH_MOST {
            break;
        }
        if !out.is_empty() {
            out.push('-');
        }
        out.push_str(&w);
    }
    if out.is_empty() {
        out = name
            .chars()
            .take(BRANCH_MOST)
            .collect::<String>()
            .trim()
            .to_string();
    }
    out
}

/// Whether the action `op` can be used on the card now, and why not.
fn usable(op: &str, facts: &Facts) -> Result<(), Why> {
    let status = facts.status.as_deref().unwrap_or_default();
    let open = || Status::NotStarted.is(status) || Status::InProgress.is(status);
    let not_open = || {
        Why::plain(text!(
            "actions.why.status",
            status = status,
            label = label(op)
        ))
    };
    match op {
        "start" if Status::NotStarted.is(status) => Ok(()),
        "start" if Status::InProgress.is(status) => match &facts.work {
            Work::Live { name, .. } => Err(Why::plain(text!("actions.why.working", name = name))),
            _ => Ok(()),
        },
        "start" => Err(not_open()),
        "finish" => {
            if !open() {
                return Err(not_open());
            }
            match &facts.work {
                Work::Live {
                    name,
                    request: Some(number),
                    ..
                } => Err(Why {
                    said: text!("actions.why.requestOpen", name = name, number = *number),
                    beside: json!({ "use": { "tool": "mr_read", "arguments": { "target": number } } }),
                }),
                Work::Live { name, changes, .. } if *changes > 0 => Err(Why {
                    said: text!("actions.why.notMerged", name = name, changes = *changes),
                    beside: json!({ "use": { "tool": "branch_merge", "arguments": { "target": name } } }),
                }),
                _ => Ok(()),
            }
        }
        "cancel" => {
            if !open() {
                return Err(not_open());
            }
            match &facts.work {
                Work::Live {
                    name,
                    request: Some(number),
                    ..
                } => Err(Why {
                    said: text!(
                        "actions.why.requestOpenCancel",
                        name = name,
                        number = *number
                    ),
                    beside: json!({ "use": { "tool": "mr_delete", "arguments": { "target": number } } }),
                }),
                _ => Ok(()),
            }
        }
        "done" => match facts.done {
            Some(false) => Ok(()),
            _ => Err(Why::plain(text!("actions.why.done"))),
        },
        "reviewed" if facts.review.is_empty() && !facts.code.iter().any(CodeLine::flagged) => {
            Err(Why::plain(text!("actions.why.noReview")))
        }
        "reviewed" => Ok(()),
        _ => Err(Why::plain(text!("actions.why.unknown", op))),
    }
}

/// The card's actions as a read says them (`card_read`'s `include: ["actions"]`): each with whether it
/// can be used now and why not, `start` with the branch name it would make, `cancel` with how many
/// changes its branch would lose (given `discard: true`).
pub(crate) fn listed(
    reader: &Reader,
    project_id: &str,
    project: &Project,
    card: &Card,
    works: &Works,
) -> Result<Option<Value>, String> {
    let facts = facts(reader, project_id, project, card, works)?;
    if facts.ops.is_empty() {
        return Ok(None);
    }
    let mut out = Vec::new();
    for op in &facts.ops {
        let mut row = Map::new();
        row.insert("op".into(), op.clone().into());
        row.insert("label".into(), label(op).into());
        if let Some(a) = written(op) {
            row.insert("description".into(), a["description"].clone());
        }
        match usable(op, &facts) {
            Ok(()) => {
                row.insert("available".into(), true.into());
            }
            Err(why) => {
                row.insert("available".into(), false.into());
                row.insert("why".into(), why.said.into());
            }
        }
        if op == "start" {
            row.insert(
                "suggestedBranch".into(),
                start_name(reader.conn, project_id, &card.name, &facts.work)?.into(),
            );
        }
        if op == "cancel"
            && let Work::Live { changes, .. } = &facts.work
            && *changes > 0
        {
            row.insert("discardChanges".into(), (*changes).into());
        }
        out.push(Value::Object(row));
    }
    Ok(Some(Value::Array(out)))
}

/// The section a card is moved into: of `kind` (`complete`, `cancelled`), inside the goals section
/// directly under the root that holds the card. None when the card is not under one, or that one
/// has none of the kind.
fn destination(project: &Project, card: &Card, kind: &str) -> Option<String> {
    let sections = &project.detail.sections;
    let parent_of = |id: &str| {
        sections
            .iter()
            .find(|s| s.id == id)
            .and_then(|s| s.parent_id.clone())
    };
    let mut at = card.section_id.clone()?;
    let mut steps = 0;
    // Up to the section directly under the root.
    loop {
        let parent = parent_of(&at)?;
        if parent_of(&parent).is_none() {
            break;
        }
        at = parent;
        steps += 1;
        if steps > 64 {
            return None;
        }
    }
    let top = sections.iter().find(|s| s.id == at)?;
    if top.kind.as_deref() != Some(crate::kinds::GOALS) {
        return None;
    }
    sections
        .iter()
        .find(|s| s.parent_id.as_deref() == Some(&at) && s.kind.as_deref() == Some(kind))
        .map(|s| s.id.clone())
}

/// The lines in main to a working directory's file, or a function or type in one, not there, from
/// the cards the goal's branch wrote (its own rows, live or set aside by its merge): each as `where →
/// what`. None when the branch's rows are gone, or no working directory was read here.
fn planned_left(tx: &Tx, scope: &Scope, work: &Work) -> Result<Vec<String>, String> {
    let reader = tx.reader();
    let branch = match work {
        Work::Unset => return Ok(Vec::new()),
        Work::Live { id, .. } => Some(id.clone()),
        Work::Gone(name) => aside_of(&reader, &scope.project, name)?,
    };
    let Some(branch) = branch else {
        return Ok(Vec::new());
    };
    let mut stmt = reader
        .conn
        .prepare("SELECT id FROM card_docs WHERE project_id = ? AND branch_id = ? AND removed = 0")
        .map_err(crate::store::sql)?;
    let written: std::collections::HashSet<String> = stmt
        .query_map([scope.project.as_str(), branch.as_str()], |r| r.get(0))
        .map_err(crate::store::sql)?
        .collect::<Result<_, _>>()
        .map_err(crate::store::sql)?;
    if written.is_empty() {
        return Ok(Vec::new());
    }
    let main = super::merges::seen_in(tx, &scope.project, &scope.project_name, "", "main")?;
    let project = main.project();
    Ok(main
        .detail
        .schemas
        .iter()
        .chain(main.detail.inner.iter())
        .filter(|c| written.contains(&c.id))
        .flat_map(|c| super::check::line_paths(&project, c))
        .filter(|(_, named)| named.to_code)
        .map(|(_, named)| format!("{} → {}", named.at, named.path))
        .collect())
}

/// The newest branch named `name` that a merge set aside: its id.
pub(crate) fn aside_of(
    reader: &Reader,
    project: &str,
    name: &str,
) -> Result<Option<String>, String> {
    let mut stmt = reader
        .conn
        .prepare(
            "SELECT id, name FROM branches WHERE project_id = ? AND name LIKE '~aside-%' ORDER BY archived_at DESC",
        )
        .map_err(crate::store::sql)?;
    let rows: Vec<(String, String)> = stmt
        .query_map([project], |r| Ok((r.get(0)?, r.get(1)?)))
        .map_err(crate::store::sql)?
        .collect::<Result<_, _>>()
        .map_err(crate::store::sql)?;
    // `~aside-<the id's first 8>-<name>`.
    Ok(rows
        .into_iter()
        .find(|(_, aside)| {
            aside
                .strip_prefix("~aside-")
                .and_then(|rest| rest.get(9..))
                .is_some_and(|was| crate::pathtext::same(was, name))
        })
        .map(|(id, _)| id))
}

/// What `target` is told when it is not one card.
fn one_card() -> String {
    text!("actions.error.oneCard")
}

/// A goal or a task looked for on main or a branch and not there, but on the board: the same call on
/// the board.
fn on_the_board(tx: &Tx, scope: &Scope, path: &str, error: String, args: &Args) -> String {
    if scope.tree == BOARD {
        return error;
    }
    let Ok(board) = super::cross::board(&tx.reader(), &scope.project) else {
        return error;
    };
    let board = Project::new(&board);
    let found = super::walk::locate(&board, path, None).is_ok_and(|t| {
        matches!(t, super::walk::Target::Card(c) if !board.defs.cards().get(&c.kind).is_none_or(|k| k.actions.is_empty()))
    });
    if !found {
        return error;
    }
    let mut arguments = args.raw.clone();
    arguments.insert("branch".into(), BOARD_NAME.into());
    crate::mcp::refusal(
        text!("actions.error.onTheBoard", path, name = scope.tree_name),
        json!({ "use": { "tool": "card_action", "arguments": arguments } }),
    )
}

/// `card_action`: the card's action `op`, with its `value`, in the call's transaction.
pub(crate) fn card_action(tx: &mut Tx, scope: &Scope, args: &Args) -> Result<Value, String> {
    let targets = args.targets()?.ok_or_else(one_card)?;
    let [path] = targets.as_slice() else {
        return Err(one_card());
    };
    let op = args.string("op")?.trim().to_string();
    let value = args.value()?;
    let seen = Seen::in_tx(tx, scope)?;
    let project = seen.project();
    let card = match super::walk::locate(&project, path, None) {
        Ok(super::walk::Target::Card(card)) => card,
        Ok(_) => return Err(one_card()),
        Err(e) => return Err(on_the_board(tx, scope, path, e, args)),
    };
    let full = project.full(card);
    let facts = facts(
        &tx.reader(),
        &scope.project,
        &project,
        card,
        &Works::new(&scope.tree, false),
    )?;
    let now: Vec<String> = facts
        .ops
        .iter()
        .filter(|o| usable(o, &facts).is_ok())
        .cloned()
        .collect();
    if !facts.ops.contains(&op) {
        return Err(if facts.ops.is_empty() {
            text!("actions.error.none", kind = card.kind.clone())
        } else {
            text!(
                "actions.error.notOfKind",
                op,
                kind = card.kind.clone(),
                ops = crate::text::List::of(&facts.ops),
                now = said_now(&now)
            )
        });
    }
    super::check(&value, "card_action", &format!("op: {op}"))?;
    if let Err(why) = usable(&op, &facts) {
        let said = text!(
            "actions.error.notNow",
            path = full.clone(),
            label = label(&op),
            why = why.said,
            now = said_now(&now)
        );
        let beside = with_project(why.beside, args);
        return Err(crate::mcp::refusal(said, beside));
    }
    let id = card.id.clone();
    let inside = card.inside.is_some();
    let in_goals = project.section_kind_of(card) == Some(crate::kinds::GOALS);
    let work = facts.work.clone();
    let name = card.name.clone();
    let complete = destination(&project, card, crate::kinds::COMPLETE);
    let cancelled = destination(&project, card, crate::kinds::CANCELLED);
    let section_now = card.section_id.clone();
    let reviewed: Vec<String> = facts.review.iter().map(|m| m.path.clone()).collect();
    let code = facts.code.clone();
    // A goal finished with tasks inside not done: said, not refused.
    let open_tasks: Vec<String> = if op == "finish" {
        super::containers::inside_all(&project, &id)
            .into_iter()
            .filter(|c| c.kind == "task" && field(c, "done").and_then(Value::as_bool) != Some(true))
            .map(|c| project.full(c))
            .collect()
    } else {
        Vec::new()
    };
    let planned_left = if op == "finish" {
        planned_left(tx, scope, &facts.work)?
    } else {
        Vec::new()
    };
    drop(seen);

    let mut changes: Vec<Keyed> = Vec::new();
    let mut fields = Map::new();
    let mut moved_to: Option<String> = None;
    let mut created: Option<String> = None;
    let mut deleted: Option<String> = None;
    let mut warnings: Vec<String> = Vec::new();
    let mut recorded: Vec<Value> = Vec::new();
    let mut not_read: Vec<Value> = Vec::new();
    match op.as_str() {
        "start" => {
            let wanted = value
                .get("branch")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|b| !b.is_empty())
                .map(str::to_string)
                .map_or_else(|| start_name(tx.conn(), &scope.project, &name, &work), Ok)?;
            let made_id = tx.create_branch(&scope.project, &wanted)?;
            let made = tx
                .branch_named(&scope.project, &wanted)?
                .filter(|(i, _)| *i == made_id)
                .map_or(wanted, |(_, n)| n);
            changes.push(keyed!("actions.change.branchCreated", name = made.clone()));
            fields.insert("status".into(), Status::InProgress.value().into());
            fields.insert("workBranch".into(), made.clone().into());
            created = Some(made);
        }
        "finish" | "cancel" => {
            let discard = value.get("discard").and_then(Value::as_bool) == Some(true);
            if let Work::Live {
                id: branch,
                name: branch_name,
                changes: left,
                ..
            } = &work
            {
                if *left > 0 && !discard {
                    // Only cancel comes here with changes (`usable` refuses finish).
                    let mut arguments = args.raw.clone();
                    let mut given = value.clone();
                    given.insert("discard".into(), true.into());
                    arguments.insert("value".into(), Value::Object(given));
                    return Err(crate::mcp::refusal(
                        text!(
                            "actions.error.cancelChanges",
                            name = branch_name.clone(),
                            changes = *left
                        ),
                        json!({ "use": { "tool": "card_action", "arguments": arguments } }),
                    ));
                }
                tx.delete_branch(&scope.project, branch)?;
                changes.push(if *left > 0 {
                    keyed!(
                        "actions.change.branchDiscarded",
                        name = branch_name.clone(),
                        changes = *left
                    )
                } else {
                    keyed!("actions.change.branchDeleted", name = branch_name.clone())
                });
                deleted = Some(branch_name.clone());
            }
            let status = if op == "finish" {
                Status::Finished
            } else {
                Status::Cancelled
            };
            fields.insert("status".into(), status.value().into());
            if !matches!(work, Work::Unset) {
                fields.insert("workBranch".into(), Value::Null);
            }
            let to = if op == "finish" { complete } else { cancelled };
            if !inside && let Some(to) = to.filter(|t| Some(t) != section_now.as_ref()) {
                moved_to = Some(to);
            }
            if !open_tasks.is_empty() {
                warnings.push(text!(
                    "actions.warn.tasksOpen",
                    count = open_tasks.len(),
                    tasks = crate::text::List::of(&open_tasks)
                ));
            }
            if !planned_left.is_empty() {
                let shown: Vec<String> = planned_left.iter().take(10).cloned().collect();
                warnings.push(text!(
                    "actions.warn.plannedLeft",
                    count = planned_left.len(),
                    lines = crate::text::List::of(&shown)
                ));
            }
        }
        "reviewed" => {
            // Written as the part's own change, the newest of its history: its marks are of
            // changes before it.
            if !reviewed.is_empty() {
                fields.insert("reviewedAt".into(), tx.now()?.into());
                changes.push(keyed!(
                    "actions.change.reviewed",
                    cards = crate::text::List::of(&reviewed)
                ));
            }
            // The code of its changed and unrecorded lines recorded as it is now: kept beside the
            // tree, not in it.
            let flagged: Vec<&CodeLine> = code.iter().filter(|l| l.flagged()).collect();
            if !flagged.is_empty() {
                let by = super::inspection::by_of(tx.who());
                let at = tx.now()?;
                super::code_review::record(tx.conn(), &scope.project, &flagged, &by, &at)?;
                changes.push(keyed!("actions.change.recorded", count = flagged.len()));
                recorded = flagged.iter().map(|l| said_line(l, None)).collect();
            }
            for line in &code {
                if let super::code_review::State::NotRead(why) = &line.state {
                    not_read.push(said_line(line, Some(why.render())));
                }
            }
            if !not_read.is_empty() {
                warnings.push(text!("actions.warn.notRead", count = not_read.len()));
            }
        }
        _ => {
            fields.insert("done".into(), true.into());
            if !inside
                && in_goals
                && let Some(to) = complete.filter(|t| Some(t) != section_now.as_ref())
            {
                moved_to = Some(to);
            }
        }
    }
    for (key, value) in &fields {
        changes.push(match value {
            Value::Null => keyed!("actions.change.cleared", field = key.clone()),
            Value::String(s) => keyed!(
                "actions.change.field",
                field = key.clone(),
                value = s.clone()
            ),
            other => keyed!(
                "actions.change.field",
                field = key.clone(),
                value = other.to_string()
            ),
        });
    }
    // Nothing of the card written (its code alone recorded): no change of the tree, no line in
    // its history; the windows on it are told all the same.
    let writes = !fields.is_empty() || moved_to.is_some();
    if !writes {
        tx.tell_of(&scope.project, "");
        tx.tell_of(&scope.project, &scope.tree);
    }
    let moved_path = if !writes {
        None
    } else {
        tx.unsaid(&scope.project, &scope.tree, |tx| {
            super::update::update_card(tx, scope, &id, &fields)?;
            let Some(to) = &moved_to else {
                return Ok(None);
            };
            let path = Seen::in_tx(tx, scope)?.project().directory.full_section(to);
            super::update::move_cards(tx, scope, std::slice::from_ref(&id), &path)?;
            Ok(Some(path))
        })?
    };
    if let Some(path) = &moved_path {
        changes.push(keyed!("actions.change.moved", section = path.clone()));
    }
    let after = Seen::in_tx(tx, scope)?;
    let now_path = after
        .project()
        .card(&id)
        .map(|c| after.project().full(c))
        .unwrap_or_else(|| full.clone());
    let rev = after.rev(&id);
    drop(after);
    let said = super::slashed(changes.clone());
    let line = match op.as_str() {
        "start" => keyed!("actions.line.start", path = full.clone(), changes = said),
        "finish" => keyed!("actions.line.finish", path = full.clone(), changes = said),
        "cancel" => keyed!("actions.line.cancel", path = full.clone(), changes = said),
        "reviewed" => keyed!("actions.line.reviewed", path = full.clone(), changes = said),
        _ => keyed!("actions.line.done", path = full.clone(), changes = said),
    };
    if writes {
        tx.line(
            &scope.project,
            &scope.tree,
            "card.action",
            "schema",
            Some(&id),
            line,
        )?;
    }
    let mut out = Map::new();
    if !warnings.is_empty() {
        out.insert("warnings".into(), warnings.into());
    }
    out.insert("project".into(), scope.project_name.clone().into());
    out.insert("path".into(), now_path.into());
    out.insert("op".into(), op.clone().into());
    if let Some(rev) = rev {
        out.insert("rev".into(), rev.into());
    }
    out.insert(
        "changes".into(),
        changes.iter().map(Keyed::render).collect::<Vec<_>>().into(),
    );
    if let Some(name) = created {
        out.insert("branchCreated".into(), name.into());
    }
    if let Some(name) = deleted {
        out.insert("branchDeleted".into(), name.into());
    }
    if let Some(path) = moved_path {
        out.insert("movedTo".into(), path.into());
    }
    if !recorded.is_empty() {
        out.insert("recorded".into(), Value::Array(recorded));
    }
    if !not_read.is_empty() {
        out.insert("notRead".into(), Value::Array(not_read));
    }
    Ok(Value::Object(out))
}

/// A line to a file as `reviewed` answers it: where it leaves the card (none for its head), where
/// it lands, and why it was not read, when it was not.
fn said_line(line: &CodeLine, why: Option<String>) -> Value {
    let mut out = Map::new();
    if !line.from.is_empty() {
        out.insert("from".into(), line.from.clone().into());
    }
    out.insert("to".into(), line.to.clone().into());
    if let Some(why) = why {
        out.insert("why".into(), why.into());
    }
    Value::Object(out)
}

/// The actions usable now, as a refusal lists them.
fn said_now(now: &[String]) -> String {
    if now.is_empty() {
        text!("actions.error.noneNow")
    } else {
        crate::text::list(now)
    }
}

/// What a refusal says beside its words, its `use` given the call's project.
fn with_project(mut beside: Value, args: &Args) -> Value {
    if let Some(arguments) = beside.pointer_mut("/use/arguments")
        && let Some(project) = args.raw.get("project")
    {
        arguments["project"] = project.clone();
    }
    beside
}

#[cfg(test)]
mod tests {
    use super::branch_name_of;

    #[test]
    fn a_branch_is_named_from_the_cards_name_in_lower_case_words() {
        for (name, branch) in [
            ("McpPictures", "mcp-pictures"),
            ("MCPTools", "mcp-tools"),
            ("Git worktree export", "git-worktree-export"),
            ("v2Fix", "v2-fix"),
            ("snake_case_name", "snake-case-name"),
            ("画面の調査", "画面の調査"),
            (
                "AVeryLongGoalNameThatGoesOnAndOnPastTheLimitOfANameForABranch",
                "a-very-long-goal-name-that-goes-on-and",
            ),
        ] {
            assert_eq!(branch_name_of(name), branch, "{name}");
        }
    }
}
