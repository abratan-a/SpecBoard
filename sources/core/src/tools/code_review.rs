//! What an implementation's line says of the code it reaches (`specboard://SpecBoard/Docs/Design:Overview`
//! 「仕様と実装のずれ」): a line from a card to a working directory's file — to a function or type in
//! it, or to the file itself — records the code's fingerprint when it is drawn or pointed
//! elsewhere, and when its card is reviewed (`card_action`'s `reviewed`). When the code there now
//! differs from what was recorded, the line has `changed`, and its card is marked to be looked at
//! again; a line nothing was recorded of has `unrecorded`, said and not marked.
//!
//! What is recorded is one table for the project, by the line's id (`code_marks`): not a tree's
//! content, so recording is no commit, a locked main takes it, and a line has one record whatever
//! tree reads it (a branch's line keeps its id when it goes into main). The code is read only
//! through this machine's folders (`code::files::Dirs`): where a working directory is bound to none,
//! or the file cannot be read here, nothing is said of the line. Where the folder is read and the
//! file or the function is not there, the line is `broken`, with why (as the check's
//! `brokenPaths` says it): said beside the others, never recorded — there is nothing to record.
//!
//! A fingerprint is made of the code's words (`symbols::tokens_digest`), so a comment or the
//! spaces between words changed say nothing; one recorded the way before (`symbols::digest_of`) is
//! held to the code made that way, until the line is recorded again.

use crate::code::files::{Dirs, Reached};
use crate::export::Project;
use crate::model::{Card, Connection as Line};
use crate::text::Keyed;
use crate::write::{FileLine, Who};
use rusqlite::{Connection, params};
use serde_json::{Map, Value};
use std::collections::HashMap;

/// How a line's code stands against what was recorded of it, on this machine.
#[derive(Clone, Debug)]
pub(crate) enum State {
    /// As recorded.
    Same,
    /// It differs from what was recorded for the place it reaches now.
    Changed,
    /// Nothing was recorded of it, or what was is of another place (the line was pointed elsewhere
    /// in another tree) or made another way.
    Unrecorded,
    /// What it points at is not in the code read here: why.
    Broken(Keyed),
    /// Its code is not read here: why.
    NotRead(Keyed),
}

impl State {
    fn word(&self) -> Option<&'static str> {
        match self {
            State::Changed => Some("changed"),
            State::Unrecorded => Some("unrecorded"),
            State::Broken(_) => Some("broken"),
            _ => None,
        }
    }
}

/// One of a card's lines to a working directory's file, and how its code stands.
#[derive(Clone, Debug)]
pub(crate) struct CodeLine {
    pub line_id: String,
    /// The kind of line.
    pub kind: String,
    /// The card it leaves, by id.
    pub card: String,
    /// The row of the card it leaves, as the line writes it; empty for the card's head.
    pub from: String,
    pub file: String,
    pub at: Option<String>,
    /// Where it lands, as a path writes it (`repo/src:'a.rs'.Order.total`).
    pub to: String,
    pub state: State,
    /// The code's fingerprint now, where it is read.
    pub digest: Option<String>,
    /// When what it differs from was recorded (`Changed`).
    pub recorded_at: Option<String>,
}

impl CodeLine {
    /// Whether it is to be looked at: changed, not recorded, or pointing at what is not there.
    pub fn flagged(&self) -> bool {
        self.state.word().is_some()
    }

    /// Whether `reviewed` records it: its code is read here and changed, or was never recorded.
    pub fn to_record(&self) -> bool {
        matches!(self.state, State::Changed | State::Unrecorded)
    }

    pub fn changed(&self) -> bool {
        matches!(self.state, State::Changed)
    }

    /// As a read says it: `{line, from?, to, state, recordedAt?, why?}`; with its id when `with_id` (the
    /// screen's, which finds the line by it). `from` is `from_said` when given (where the line
    /// leaves, for a list of what it reaches).
    pub fn json(&self, with_id: bool, from_said: Option<&str>) -> Value {
        let mut out = Map::new();
        if with_id {
            out.insert("id".into(), self.line_id.clone().into());
        }
        out.insert("line".into(), self.kind.clone().into());
        match from_said {
            Some(from) => {
                out.insert("from".into(), from.into());
            }
            None if !self.from.is_empty() => {
                out.insert("from".into(), self.from.clone().into());
            }
            None => {}
        }
        out.insert("to".into(), self.to.clone().into());
        if let Some(word) = self.state.word() {
            out.insert("state".into(), word.into());
        }
        if let Some(at) = &self.recorded_at {
            out.insert("recordedAt".into(), at.clone().into());
        }
        if let State::Broken(why) = &self.state {
            out.insert("why".into(), why.render().into());
        }
        Value::Object(out)
    }
}

/// The flagged lines of a card as a read lists them (`codeReview`).
pub(crate) fn listed(lines: &[CodeLine], with_id: bool) -> Value {
    Value::Array(
        lines
            .iter()
            .filter(|l| l.flagged())
            .map(|l| l.json(with_id, None))
            .collect(),
    )
}

/// What was recorded of a line's code.
#[derive(Clone, Debug)]
pub(crate) struct Record {
    pub file: String,
    /// The function or type it was recorded at; empty for the whole file.
    pub symbol: String,
    pub digest: String,
    pub at: String,
}

/// What the project recorded, by line id.
pub(crate) fn records(conn: &Connection, project: &str) -> Result<HashMap<String, Record>, String> {
    let mut stmt = conn
        .prepare_cached(
            "SELECT line_id, file_id, symbol, digest, at FROM code_marks WHERE project_id = ?",
        )
        .map_err(crate::store::sql)?;
    stmt.query_map([project], |r| {
        Ok((
            r.get::<_, String>(0)?,
            Record {
                file: r.get(1)?,
                symbol: r.get(2)?,
                digest: r.get(3)?,
                at: r.get(4)?,
            },
        ))
    })
    .map_err(crate::store::sql)?
    .collect::<Result<_, _>>()
    .map_err(crate::store::sql)
}

/// Whether a line of the kind `kind` reaches files: the project's kinds of line say.
fn reaches_files(project: &Project, kind: &str) -> bool {
    project
        .defs
        .line(kind)
        .is_some_and(|d| d.to.names_kind("file"))
}

/// The tree's lines to working directories' files (of the kinds that reach them), each with the
/// card it leaves: of every card and every card inside a container, or of the one card `only`.
pub(crate) fn file_lines<'a>(
    project: &Project<'a>,
    only: Option<&str>,
) -> Vec<(&'a Card, &'a Line)> {
    let detail = project.detail;
    detail
        .schemas
        .iter()
        .chain(detail.inner.iter())
        .filter(|c| only.is_none_or(|id| c.id == id))
        .flat_map(|card| {
            card.connections
                .iter()
                .filter(|l| l.to.as_deref().is_some_and(|t| t.starts_with("file:")))
                .filter(|l| reaches_files(project, &l.line))
                .map(move |l| (&**card, l))
        })
        .collect()
}

/// Where a line to a file lands, as a path writes it.
pub(crate) fn written_to(project: &Project, file: &str, at: Option<&str>) -> String {
    let file = project
        .file_written(file)
        .unwrap_or_else(|| file.to_string());
    match at {
        Some(at) => format!("{file}.{at}"),
        None => file,
    }
}

/// The tag a fingerprint was made with (`t1:`, `w1:`).
fn tag_of(digest: &str) -> &str {
    digest.split_once(':').map_or("", |(t, _)| t)
}

/// Why a line is broken: the file, or the function or type in it, is not there.
fn broken(project: &Project, file: &str, symbol: Option<&str>, near: &[String]) -> Keyed {
    let file = project
        .file_written(file)
        .unwrap_or_else(|| file.to_string());
    match symbol {
        None => keyed!("code.file.missing", file),
        Some(symbol) => keyed!(
            "code.symbol.missing",
            file,
            symbol = symbol.to_string(),
            near = if near.is_empty() {
                text!("code.symbol.noneNear")
            } else {
                crate::text::list(near)
            }
        ),
    }
}

/// The tree's lines to files and how each one's code stands, on this machine: of every card, or of
/// the one card `only`. Each file is read once (`Dirs::digests`).
pub(crate) fn lines(
    conn: &Connection,
    project_id: &str,
    project: &Project,
    only: Option<&str>,
) -> Result<Vec<CodeLine>, String> {
    let found = file_lines(project, only);
    if found.is_empty() {
        return Ok(Vec::new());
    }
    let dirs = Dirs::of(conn, project_id)?;
    let made = |card: &Card, line: &Line, state: State| CodeLine {
        line_id: line.id.clone(),
        kind: line.line.clone(),
        card: card.id.clone(),
        from: line.from.clone(),
        file: line.to.clone().unwrap_or_default(),
        at: line.at.clone().filter(|a| !a.is_empty()),
        to: written_to(
            project,
            line.to.as_deref().unwrap_or_default(),
            line.at.as_deref().filter(|a| !a.is_empty()),
        ),
        state,
        digest: None,
        recorded_at: None,
    };
    // No folder here: nothing of them is read.
    if !dirs.any_bound() {
        return Ok(found
            .into_iter()
            .map(|(card, line)| made(card, line, State::NotRead(keyed!("workdir.read.unbound"))))
            .collect());
    }
    let kept = records(conn, project_id)?;
    let wanted: Vec<(String, Option<String>)> = found
        .iter()
        .map(|(_, l)| {
            (
                l.to.clone().unwrap_or_default(),
                l.at.clone().filter(|a| !a.is_empty()),
            )
        })
        .collect();
    let now = dirs.digests(&wanted);
    Ok(found
        .into_iter()
        .zip(now)
        .map(|((card, line), now)| {
            let mut out = made(card, line, State::Same);
            match now {
                Reached::NotRead(why) => out.state = State::NotRead(why),
                Reached::Missing { symbol, near } => {
                    out.state = State::Broken(broken(project, &out.file, symbol.as_deref(), &near));
                }
                Reached::Read { digest, plain } => {
                    let symbol = out.at.clone().unwrap_or_default();
                    // Held to the fingerprint made the way the record was.
                    let record = kept
                        .get(&line.id)
                        .filter(|r| r.file == out.file && r.symbol == symbol);
                    let now = record.and_then(|r| {
                        [&digest, &plain]
                            .into_iter()
                            .find(|d| tag_of(d) == tag_of(&r.digest))
                            .map(|d| (r, d))
                    });
                    out.state = match now {
                        Some((r, d)) if &r.digest == d => State::Same,
                        Some((r, _)) => {
                            out.recorded_at = Some(r.at.clone());
                            State::Changed
                        }
                        None => State::Unrecorded,
                    };
                    out.digest = Some(digest);
                }
            }
            out
        })
        .collect())
}

/// The lines, by the card they leave.
pub(crate) fn by_card(lines: Vec<CodeLine>) -> HashMap<String, Vec<CodeLine>> {
    let mut out: HashMap<String, Vec<CodeLine>> = HashMap::new();
    for line in lines {
        out.entry(line.card.clone()).or_default().push(line);
    }
    out
}

/// The code of `lines` recorded as it is now (those read here), by `by` at `at`.
pub(crate) fn record(
    conn: &Connection,
    project_id: &str,
    lines: &[&CodeLine],
    by: &str,
    at: &str,
) -> Result<(), String> {
    for line in lines {
        let Some(digest) = &line.digest else {
            continue;
        };
        put(
            conn,
            project_id,
            &line.line_id,
            &line.file,
            line.at.as_deref().unwrap_or_default(),
            digest,
            by,
            at,
        )?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn put(
    conn: &Connection,
    project_id: &str,
    line: &str,
    file: &str,
    symbol: &str,
    digest: &str,
    by: &str,
    at: &str,
) -> Result<(), String> {
    conn.execute(
        "INSERT OR REPLACE INTO code_marks (project_id, line_id, file_id, symbol, digest, by, at) VALUES (?, ?, ?, ?, ?, ?, ?)",
        params![project_id, line, file, symbol, digest, by, at],
    )
    .map_err(crate::write::db)?;
    Ok(())
}

/// The code of the lines to files a write made or pointed elsewhere (`Tx::take_code_lines`),
/// recorded once it committed: of the kinds of line that reach files, where it is read on this
/// machine. The files are read with the store let go of; what cannot be recorded stays
/// unrecorded, which is said and not marked, so nothing here refuses the write that was made.
pub(crate) fn record_written(core: &crate::Core, who: &Who, written: &[FileLine]) {
    let mut projects: Vec<&str> = written.iter().map(|l| l.project.as_str()).collect();
    projects.sort();
    projects.dedup();
    let by = super::inspection::by_of(who);
    for project in projects {
        let read = {
            let store = core.lock();
            let kinds: Option<String> = store
                .conn
                .query_row("SELECT kinds FROM projects WHERE id = ?", [project], |r| {
                    r.get(0)
                })
                .ok()
                .flatten();
            Dirs::of(&store.conn, project).map(|dirs| (dirs, kinds))
        };
        let Ok((dirs, kinds)) = read else {
            continue;
        };
        if !dirs.any_bound() {
            continue;
        }
        let defs = crate::kinds::Defs::of_project(kinds.as_deref());
        let lines: Vec<&FileLine> = written
            .iter()
            .filter(|l| l.project == project)
            .filter(|l| defs.line(&l.kind).is_some_and(|d| d.to.names_kind("file")))
            .collect();
        if lines.is_empty() {
            continue;
        }
        let wanted: Vec<(String, Option<String>)> = lines
            .iter()
            .map(|l| (l.file.clone(), l.at.clone()))
            .collect();
        let now = dirs.digests(&wanted);
        let store = core.lock();
        let Ok(at) = crate::write::now(&store.conn) else {
            continue;
        };
        for (line, reached) in lines.into_iter().zip(now) {
            if let Reached::Read { digest, .. } = reached {
                let _ = put(
                    &store.conn,
                    project,
                    &line.line_id,
                    &line.file,
                    line.at.as_deref().unwrap_or_default(),
                    &digest,
                    &by,
                    &at,
                );
            }
        }
    }
}
