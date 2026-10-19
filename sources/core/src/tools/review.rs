//! A part's review marks (`specboard://SpecBoard/Docs/Design:CardKinds`「UI のセクションとはめ込み」): a
//! part — an html, svg or frame card — is drawn from the cards its lines reach (an entity it shows, a
//! flow it starts), so when one of them changes after the part last did, the part may no longer say
//! what that card says, and is marked to be looked at again. Writing the part — its action
//! `reviewed` too — clears it.
//!
//! When a card last changed is its tree's history's: the newest commit that wrote its document
//! (`commit_rows`; where a card sits is no commit, so a move is no change). A branch's history is
//! main's up to where the branch sees main from — when it was made, or last took main in — and the
//! branch's own commits after it. Commits are ordered by their place in the history (`seq`), the
//! branch's after main's, never by their times alone: two writes may share a millisecond. Every
//! part's marks are worked out together, from one reading of the tree's history of the cards
//! concerned.

use crate::export::Project;
use crate::model::Card;
use crate::store::BOARD;
use rusqlite::{Connection, OptionalExtension};
use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};

/// The kinds of card a mock is made of, which a mark is worked out for.
pub(crate) const PARTS: [&str; 3] = ["html", "svg", crate::kinds::FRAME];

/// A card a part's line reaches that changed after the part did: its full path, and when it
/// changed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mark {
    pub path: String,
    pub at: String,
}

impl Mark {
    pub(crate) fn json(&self) -> Value {
        let mut out = Map::new();
        out.insert("path".into(), self.path.clone().into());
        out.insert("at".into(), self.at.clone().into());
        Value::Object(out)
    }
}

/// Each part's marks by the part's id; a part with none is not there.
pub type Marks = HashMap<String, Vec<Mark>>;

/// The marks as a list says them: `[{path, at}]`.
pub(crate) fn listed(marks: &[Mark]) -> Value {
    Value::Array(marks.iter().map(Mark::json).collect())
}

/// When a card last changed in a tree: its commit's place, ordered as the tree's history is, and
/// its time.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Changed {
    /// 0 for main's commit, 1 for a branch's own: a branch's come after the main it sees.
    own: u8,
    seq: i64,
    at: String,
}

/// The marks of the parts of `project`, the tree `tree` of the project `project_id` (`""` main,
/// a branch's id): of every part, or of the one card `only`. The board holds no parts.
pub(crate) fn marks(
    conn: &Connection,
    project_id: &str,
    tree: &str,
    project: &Project,
    only: Option<&str>,
) -> Result<Marks, String> {
    if tree == BOARD {
        return Ok(Marks::new());
    }
    let parts: Vec<(&Card, Vec<&Card>)> = project
        .detail
        .schemas
        .iter()
        .filter(|c| PARTS.contains(&c.kind.as_str()))
        .filter(|c| only.is_none_or(|id| c.id == id))
        .filter_map(|part| {
            let reached = reached(project, part);
            (!reached.is_empty()).then_some((&**part, reached))
        })
        .collect();
    if parts.is_empty() {
        return Ok(Marks::new());
    }
    let mut ids: HashSet<&str> = HashSet::new();
    for (part, reached) in &parts {
        ids.insert(&part.id);
        ids.extend(reached.iter().map(|c| c.id.as_str()));
    }
    let changed = last_changed(conn, project_id, tree, &ids)?;
    let mut out = Marks::new();
    for (part, reached) in parts {
        let since = changed.get(part.id.as_str());
        let found: Vec<Mark> = reached
            .into_iter()
            .filter_map(|card| {
                let when = changed.get(card.id.as_str())?;
                since.is_none_or(|s| when > s).then(|| Mark {
                    path: project.full(card),
                    at: when.at.clone(),
                })
            })
            .collect();
        if !found.is_empty() {
            out.insert(part.id.clone(), found);
        }
    }
    Ok(out)
}

/// The cards a part's lines reach in its own tree, a card ghost followed to its card, each once, in
/// the order of its lines: not itself, nor a working directory's file or another tree's card.
fn reached<'a>(project: &Project<'a>, part: &Card) -> Vec<&'a Card> {
    let mut out: Vec<&'a Card> = Vec::new();
    for line in &part.connections {
        let Some(card) = line.to.as_deref().and_then(|to| project.card(to)) else {
            continue;
        };
        if card.id != part.id && !out.iter().any(|c| c.id == card.id) {
            out.push(card);
        }
    }
    out
}

/// When each of `ids` last changed in the tree, as its history says; one its history keeps no
/// commit of is not there.
fn last_changed(
    conn: &Connection,
    project_id: &str,
    tree: &str,
    ids: &HashSet<&str>,
) -> Result<HashMap<String, Changed>, String> {
    let wanted = Value::Array(ids.iter().map(|&id| Value::from(id)).collect()).to_string();
    let mut out: HashMap<String, Changed> = HashMap::new();
    let mut take = |own: u8, until: Option<&str>| -> Result<(), String> {
        let mut stmt = conn
            .prepare_cached(
                "SELECT r.row_id, c.seq, c.at FROM commits c JOIN commit_rows r ON r.commit_id = c.id \
                 WHERE c.project_id = ?1 AND c.branch_id = ?2 AND r.tbl = 'card_docs' \
                 AND r.row_id IN (SELECT value FROM json_each(?3)) \
                 AND (?4 IS NULL OR c.at <= ?4)",
            )
            .map_err(crate::store::sql)?;
        let branch = if own == 1 { tree } else { "" };
        let rows = stmt
            .query_map(rusqlite::params![project_id, branch, wanted, until], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })
            .map_err(crate::store::sql)?;
        for row in rows {
            let (id, seq, at) = row.map_err(crate::store::sql)?;
            let now = Changed { own, seq, at };
            match out.get(&id) {
                Some(had) if *had >= now => {}
                _ => {
                    out.insert(id, now);
                }
            }
        }
        Ok(())
    };
    if tree.is_empty() {
        take(0, None)?;
    } else {
        let from = seen_main_from(conn, project_id, tree)?;
        take(0, Some(&from))?;
        take(1, None)?;
    }
    Ok(out)
}

/// When the branch began to see main as it is: when it was made, or when it last took main in
/// (or went into main and was kept), whichever came last. main's commits after it are not the
/// branch's history.
fn seen_main_from(conn: &Connection, project_id: &str, branch: &str) -> Result<String, String> {
    let made: String = conn
        .query_row(
            "SELECT created_at FROM branches WHERE id = ?",
            [branch],
            |r| r.get(0),
        )
        .optional()
        .map_err(crate::store::sql)?
        .unwrap_or_default();
    let took: Option<String> = conn
        .query_row(
            "SELECT MAX(c.at) FROM commits c JOIN commit_lines l ON l.commit_id = c.id \
             WHERE c.project_id = ?1 AND c.branch_id = ?2 AND l.kind = 'branch.merge' \
             AND l.target_id IN (?1, ?2)",
            [project_id, branch],
            |r| r.get(0),
        )
        .map_err(crate::store::sql)?;
    Ok(took.filter(|t| *t > made).unwrap_or(made))
}
