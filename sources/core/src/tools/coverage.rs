//! `card_list`'s `implementation`: the cards by their lines to working directories' files
//! (`specboard://SpecBoard/Docs/Design:Overview`「仕様と実装のずれ」). `none` finds what is not
//! linked to code yet — a card no such line leaves, the rows of a card none leaves when others do
//! (`unlinked`), a file or a function or type no line reaches —, and `changed` and `unrecorded` the
//! cards (or files, functions and types) whose lines' code changed since it was recorded, or was
//! never recorded (`code_review`), each with those lines (`codeReview`).

use super::code_review::{self, CodeLine};
use crate::export::Project;
use crate::model::{Card, Connection as Line};
use rusqlite::Connection;
use serde_json::{Map, Value};
use std::collections::HashMap;

/// What `implementation` asks for: any of them.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Wanted {
    none: bool,
    changed: bool,
    unrecorded: bool,
}

impl Wanted {
    /// As written: `none`, `changed` or `unrecorded`, or a list of them.
    pub(crate) fn of(raw: &Value) -> Result<Wanted, String> {
        let words: Vec<&Value> = match raw {
            Value::Array(items) if !items.is_empty() => items.iter().collect(),
            Value::String(_) => vec![raw],
            _ => return Err(text!("reads.implementation.shape", other = raw.to_string())),
        };
        let mut out = Wanted::default();
        for word in words {
            match word.as_str() {
                Some("none") => out.none = true,
                Some("changed") => out.changed = true,
                Some("unrecorded") => out.unrecorded = true,
                _ => return Err(text!("reads.implementation.shape", other = raw.to_string())),
            }
        }
        Ok(out)
    }

    /// Whether the code is to be read: the lines' states are asked for.
    fn states(&self) -> bool {
        self.changed || self.unrecorded
    }

    fn takes(&self, line: &CodeLine) -> bool {
        match line.state {
            code_review::State::Changed => self.changed,
            code_review::State::Unrecorded => self.unrecorded,
            _ => false,
        }
    }
}

/// A row of a card a line may leave: its names from the card's top, and the rows under it.
struct Row {
    names: Vec<String>,
    rows: Vec<Row>,
}

/// The rows of a card a line may leave, as a tree: its fields, a document's rows (but its prose,
/// `text`), the rows of a card of the project's own kind.
fn rows_of(card: &Card) -> Vec<Row> {
    fn fields(list: &[crate::model::Field], above: &[String]) -> Vec<Row> {
        list.iter()
            .map(|f| {
                let names = [above, std::slice::from_ref(&f.name)].concat();
                Row {
                    rows: fields(&f.children, &names),
                    names,
                }
            })
            .collect()
    }
    fn written(list: &[crate::model::FormatRow], above: &[String]) -> Vec<Row> {
        list.iter()
            .filter(|r| r.kind != "text")
            .map(|r| {
                let names = [above, std::slice::from_ref(&r.name)].concat();
                Row {
                    rows: written(&r.rows, &names),
                    names,
                }
            })
            .collect()
    }
    fn own(list: &[crate::model::CustomRow], above: &[String]) -> Vec<Row> {
        list.iter()
            .map(|r| {
                let names = [above, std::slice::from_ref(&r.name)].concat();
                Row {
                    rows: own(&r.rows, &names),
                    names,
                }
            })
            .collect()
    }
    let mut out = fields(&card.fields, &[]);
    out.extend(written(&card.rows, &[]));
    out.extend(own(&card.custom_rows, &[]));
    out
}

/// The top-most rows no line covers (a row is covered by a line from it or from a row above it):
/// a row with a covered one under it is not itself listed, the ones under it not covered are.
fn uncovered(rows: &[Row], covered: &[Vec<String>], out: &mut Vec<String>) {
    for row in rows {
        if covered.iter().any(|c| row.names.starts_with(c)) {
            continue;
        }
        let under = covered
            .iter()
            .any(|c| c.len() > row.names.len() && c.starts_with(&row.names));
        if !under {
            out.push(crate::pathtext::join_names(
                row.names.iter().map(String::as_str),
            ));
            continue;
        }
        uncovered(&row.rows, covered, out);
    }
}

/// Whether a card is not linked to code: `None` when it is (a line leaves its head, or one leaves
/// each of its rows), the rows none leaves when some do (empty when none leaves it at all).
fn unlinked(card: &Card, lines: &[&Line]) -> Option<Vec<String>> {
    if lines.is_empty() {
        return Some(Vec::new());
    }
    if lines.iter().any(|l| l.from.is_empty()) {
        return None;
    }
    let covered: Vec<Vec<String>> = lines.iter().map(|l| super::picks::split(&l.from)).collect();
    let mut out = Vec::new();
    uncovered(&rows_of(card), &covered, &mut out);
    (!out.is_empty()).then_some(out)
}

/// A line that lands in a file: where in it (none for the file itself), where it leaves (the
/// card's path, and the row's), and how its code stands when that was asked for.
type Landed = (Option<String>, String, Option<CodeLine>);

/// The tree's lines to files, where they leave and what they reach, and how each one's code stands
/// when that was asked for.
pub(crate) struct Landing {
    wanted: Wanted,
    /// By the file's id: where in it each lands (none for the file itself), where it leaves (the
    /// card's path, and the row's), and its code's state.
    by_file: HashMap<String, Vec<Landed>>,
    /// By the card it leaves.
    by_card: HashMap<String, Vec<Line>>,
    /// Its code's state, by the card it leaves.
    code: HashMap<String, Vec<CodeLine>>,
}

impl Landing {
    pub(crate) fn of(
        conn: &Connection,
        project_id: &str,
        project: &Project,
        wanted: &Wanted,
    ) -> Result<Landing, String> {
        let code: Vec<CodeLine> = if wanted.states() {
            code_review::lines(conn, project_id, project, None)?
        } else {
            Vec::new()
        };
        let state: HashMap<&str, &CodeLine> =
            code.iter().map(|l| (l.line_id.as_str(), l)).collect();
        let mut by_file: HashMap<String, Vec<Landed>> = HashMap::new();
        let mut by_card: HashMap<String, Vec<Line>> = HashMap::new();
        for (card, line) in code_review::file_lines(project, None) {
            let full = project.full(card);
            let from = if line.from.is_empty() {
                full
            } else {
                format!("{full}.{}", line.from)
            };
            by_file
                .entry(line.to.clone().unwrap_or_default())
                .or_default()
                .push((
                    line.at.clone().filter(|a| !a.is_empty()),
                    from,
                    state.get(line.id.as_str()).map(|l| (*l).clone()),
                ));
            by_card
                .entry(card.id.clone())
                .or_default()
                .push(line.clone());
        }
        let code = code_review::by_card(code);
        Ok(Landing {
            wanted: *wanted,
            by_file,
            by_card,
            code,
        })
    }

    /// What a function or type of a file is listed with, when it is listed: none when it is not.
    pub(crate) fn symbol(&self, file: &str, path: &str) -> Option<Map<String, Value>> {
        let landing = self.by_file.get(file).map(Vec::as_slice).unwrap_or(&[]);
        let covered = landing.iter().any(|(at, _, _)| match at {
            None => true,
            Some(at) => {
                at == path
                    || path
                        .strip_prefix(at.as_str())
                        .is_some_and(|r| r.starts_with('.'))
            }
        });
        let flagged: Vec<Value> = landing
            .iter()
            .filter(|(at, _, _)| at.as_deref() == Some(path))
            .filter_map(|(_, from, line)| {
                line.as_ref()
                    .filter(|l| self.wanted.takes(l))
                    .map(|l| l.json(false, Some(from)))
            })
            .collect();
        self.kept(!covered, flagged, None)
    }

    /// What a card of the tree is listed with, when it is listed: a file by the lines that reach
    /// it, any other by those that leave it.
    fn card(&self, card: &Card) -> Option<Map<String, Value>> {
        if card.kind == "file" {
            let landing = self.by_file.get(&card.id).map(Vec::as_slice).unwrap_or(&[]);
            let flagged: Vec<Value> = landing
                .iter()
                .filter_map(|(_, from, line)| {
                    line.as_ref()
                        .filter(|l| self.wanted.takes(l))
                        .map(|l| l.json(false, Some(from)))
                })
                .collect();
            return self.kept(landing.is_empty(), flagged, None);
        }
        let lines: Vec<&Line> = self
            .by_card
            .get(&card.id)
            .map(|l| l.iter().collect())
            .unwrap_or_default();
        let bare = unlinked(card, &lines);
        let flagged: Vec<Value> = self
            .code
            .get(&card.id)
            .into_iter()
            .flatten()
            .filter(|l| self.wanted.takes(l))
            .map(|l| l.json(false, None))
            .collect();
        let rows = bare.as_ref().filter(|r| !r.is_empty()).cloned();
        self.kept(bare.is_some(), flagged, rows)
    }

    /// Kept when it is not linked and that is asked for, or has lines of the states asked for:
    /// with the rows not linked, and those lines.
    fn kept(
        &self,
        not_linked: bool,
        flagged: Vec<Value>,
        rows: Option<Vec<String>>,
    ) -> Option<Map<String, Value>> {
        let by_none = self.wanted.none && not_linked;
        if !by_none && flagged.is_empty() {
            return None;
        }
        let mut out = Map::new();
        if by_none && let Some(rows) = rows {
            out.insert("unlinked".into(), rows.into());
        }
        if !flagged.is_empty() {
            out.insert("codeReview".into(), Value::Array(flagged));
        }
        Some(out)
    }
}

/// The cards of `chosen` `implementation` keeps, each with what it is listed with.
pub(crate) fn cards(
    conn: &Connection,
    project_id: &str,
    project: &Project,
    chosen: &[&Card],
    wanted: &Wanted,
) -> Result<HashMap<String, Map<String, Value>>, String> {
    let landing = Landing::of(conn, project_id, project, wanted)?;
    Ok(chosen
        .iter()
        .filter_map(|c| landing.card(c).map(|extra| (c.id.clone(), extra)))
        .collect())
}
