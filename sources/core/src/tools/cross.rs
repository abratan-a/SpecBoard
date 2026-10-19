//! What reaches across the trees: the
//! board's cards that reach one of main's — a `link` to it in their `connections`, or a path in
//! full written in their text (`[[Docs/Design:CardKinds]]`) — read from main's side: a card's
//! 「関わるやること」 (`card_read`'s `fromBoard`, those not done yet) and what its deletion leaves
//! broken (`card_delete`'s warnings, every one).

use crate::export::Project;
use crate::model::{Card, Detail};
use crate::store::{BOARD, Reader};
use serde_json::{Map, Value};

/// The board, read whole.
pub(crate) fn board(reader: &Reader, project: &str) -> Result<Detail, String> {
    crate::model::typed(&crate::detail::read(reader, project, Some(BOARD))?)
}

/// One of the board's cards reaching one of main's: where it is, what it is, how far it has come,
/// and where in main's card it lands (a row's path, none for the card itself).
pub(crate) struct Reach {
    pub path: String,
    pub kind: String,
    pub label: String,
    pub state: Option<(String, Value)>,
    pub at: Option<String>,
    pub done: bool,
}

impl Reach {
    pub(crate) fn json(&self) -> Value {
        let mut out = Map::new();
        out.insert("path".into(), self.path.clone().into());
        out.insert("kind".into(), self.kind.clone().into());
        if !self.label.is_empty() {
            out.insert("label".into(), self.label.clone().into());
        }
        if let Some((field, value)) = &self.state {
            out.insert(field.clone(), value.clone());
        }
        if let Some(at) = &self.at {
            out.insert("at".into(), at.clone().into());
        }
        Value::Object(out)
    }
}

/// A card's value of a field, as its document holds it.
fn value<'c>(card: &'c Card, field: &str) -> Option<&'c Value> {
    card.form.as_ref()?.get("fields")?.get(field)
}

/// The field a card's kind says it is done by (a check, or a choice with values meaning done), and
/// whether it is: a check one of its done values, a choice at the first meaning done or after it
/// (a goal's 完了 and 打ち切り, as the board's head folds them).
fn done_of(project: &Project, card: &Card) -> Option<(String, Value, bool)> {
    use crate::kinds::FieldType;
    let kind = project.defs.cards().get(&card.kind)?;
    let field = kind.fields.iter().find(|f| {
        !f.done.is_empty() && matches!(f.fieldtype, FieldType::Checkbox | FieldType::Radio)
    })?;
    let now = value(card, &field.name)
        .cloned()
        .or_else(|| field.default.clone())
        .unwrap_or(match field.fieldtype {
            FieldType::Checkbox => Value::Bool(false),
            _ => Value::Null,
        });
    let done = match field.fieldtype {
        FieldType::Radio => {
            let first = field
                .options
                .iter()
                .position(|o| field.done.iter().any(|d| d.as_str() == Some(o.as_str())));
            let at = now
                .as_str()
                .and_then(|n| field.options.iter().position(|o| o == n));
            matches!((first, at), (Some(first), Some(at)) if at >= first)
        }
        _ => field.done.contains(&now),
    };
    Some((field.name.clone(), now, done))
}

/// Whether a card is done, or inside a container that is (a task in a goal finished).
pub(crate) fn finished(project: &Project, card: &Card) -> bool {
    let mut at = Some(card);
    let mut seen = 0;
    while let Some(here) = at {
        if done_of(project, here).is_some_and(|(_, _, done)| done) {
            return true;
        }
        seen += 1;
        if seen > 64 {
            break;
        }
        at = here.inside.as_deref().and_then(|id| project.card(id));
    }
    false
}

/// The text a card holds where a path may be written: its body, and a body its kind keeps as a
/// field (a task's, a goal's).
fn texts(card: &Card) -> Vec<&str> {
    let mut out = Vec::new();
    if !card.body.is_empty() {
        out.push(card.body.as_str());
    }
    if let Some(Value::String(body)) = value(card, "body")
        && body != &card.body
    {
        out.push(body.as_str());
    }
    out
}

/// The board's cards that reach `card` of main's (`full` its path there): a link to it, or a path
/// written in their text naming it or a row in it — each once, with where it lands. `done_too`:
/// those done as well (what a deletion breaks), else only those still to do (「関わるやること」).
/// The board's cards whose field of a branch (a goal's `workBranch`) names `branch`: by their
/// paths, for a branch let go to say who is left naming it.
pub(crate) fn naming_branch(board: &Detail, branch: &str) -> Vec<String> {
    let project = Project::new(board);
    let wanted = branch.trim();
    board
        .schemas
        .iter()
        .chain(board.inner.iter())
        .filter(|card| {
            let Some(kind) = project.defs.cards().get(&card.kind) else {
                return false;
            };
            kind.fields
                .iter()
                .filter(|f| f.fieldtype == crate::kinds::FieldType::Branch)
                .any(|f| {
                    card.form
                        .as_ref()
                        .and_then(|form| form["fields"].get(&f.name))
                        .and_then(Value::as_str)
                        .is_some_and(|v| crate::pathtext::same(v.trim(), wanted))
                })
        })
        .map(|card| project.full(card))
        .collect()
}

/// The goal of the board, not done, that works in `branch` (its field of a branch names it): its
/// path. Such a branch is that goal's plan, so code it names may be still to write.
pub(crate) fn open_goal_of(board: &Detail, branch: &str) -> Option<String> {
    let project = Project::new(board);
    let naming = naming_branch(board, branch);
    board
        .schemas
        .iter()
        .chain(board.inner.iter())
        .filter(|c| c.kind == "goal" && !finished(&project, c))
        .map(|c| project.full(c))
        .find(|path| naming.contains(path))
}

pub(crate) fn reaching(board: &Detail, card: &Card, full: &str, done_too: bool) -> Vec<Reach> {
    let project = Project::new(board);
    let mut out: Vec<Reach> = Vec::new();
    for there in board.schemas.iter().chain(board.inner.iter()) {
        let mut at: Option<Option<String>> = None;
        for line in &there.connections {
            if line.to.as_deref() == Some(card.id.as_str()) {
                at = Some(line.at.clone().filter(|a| !a.is_empty()));
                break;
            }
        }
        if at.is_none() {
            'texts: for text in texts(there) {
                for written in crate::notetext::refs(text) {
                    let path = crate::pathtext::canonical(written.path.trim());
                    let path = path.as_ref();
                    if path == full {
                        at = Some(None);
                        break 'texts;
                    }
                    if let Some(rest) = path.strip_prefix(&format!("{full}.")) {
                        at = Some(Some(rest.to_string()));
                        break 'texts;
                    }
                }
            }
        }
        let Some(at) = at else {
            continue;
        };
        let done = finished(&project, there);
        if done && !done_too {
            continue;
        }
        out.push(Reach {
            path: project.full(there),
            kind: there.kind.clone(),
            label: there.label.clone(),
            state: done_of(&project, there).map(|(field, now, _)| (field, now)),
            at,
            done,
        });
    }
    out
}
