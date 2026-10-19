//! A card's rev: what a card is in a tree, said short, so that a write made from what was read is refused when
//! another wrote the card in between (`specboard://SpecBoard/Docs/Design:CoreApi`, `write.rev`).
//!
//! It is made from the card's document as the tree sees it (`Seen::docs`, what a read is made
//! from) — its section or container, name and kind are its own keys (`write::Table::keys`), the rest
//! of it with them — not from where its box sits (`places`) nor how it looks (`card_looks`): a
//! history cut short, or the agent started again, makes the same. MCP's `card_read` says it of each
//! card read (of the card a field, a row or a node is in), `card_create` and `card_update` of what
//! they wrote, and a batch run of every card it wrote (`revs`). `card_update`'s and `card_delete`'s
//! value may give it back (`rev`): held to the card as it was before the call — a batch's steps all
//! to the tree before they run, so a step does not refuse what an earlier one wrote — and refused,
//! nothing written, when it is not, with who wrote the card last. The window's writes give none.

use super::Seen;
use super::cards::Scope;
use super::walk::{Target, locate};
use crate::write::Tx;
use rusqlite::OptionalExtension;
use serde_json::{Map, Value};

/// The tools whose value may give a rev.
const TAKES_REV: [&str; 2] = ["card_update", "card_delete"];

/// How many hexadecimal digits a rev is.
const DIGITS: usize = 12;

/// A card's rev: its document, hashed (SHA-256), the first digits.
pub(crate) fn rev_of(doc: &Map<String, Value>) -> String {
    use sha2::{Digest, Sha256};
    let text = serde_json::to_string(doc).unwrap_or_default();
    Sha256::digest(text.as_bytes())
        .iter()
        .take(DIGITS / 2)
        .map(|b| format!("{b:02x}"))
        .collect()
}

impl Seen {
    /// The rev of the card `id` (a card ghost's too) as this tree sees it; none for one it does not
    /// keep (a working directory's file).
    pub fn rev(&self, id: &str) -> Option<String> {
        self.docs.get(id).map(rev_of)
    }

    /// The rev of the card a path is in (the card itself, or the card a field, a row or a node is
    /// in), as `locate` reads it; none when the path does not resolve.
    pub fn rev_at(&self, path: &str, base: Option<&str>) -> Option<String> {
        let project = self.project();
        let target = locate(&project, path, base).ok()?;
        self.rev(owner(&target))
    }
}

/// The card a target is in: the card itself (a card ghost, a node), or the card holding the field,
/// the row, the version or the value.
pub(crate) fn owner<'t>(target: &'t Target) -> &'t str {
    match target {
        Target::Card(card)
        | Target::Field(card, ..)
        | Target::FormatRow(card, _)
        | Target::CustomRow(card, _)
        | Target::EnumValue(card, _)
        | Target::Step(card, _)
        | Target::Symbol(card, _) => &card.id,
        Target::Ghost(ghost) => &ghost.id,
    }
}

/// `rev` put in an answer, after its `path` and the `form` it is written in (at its end when it has
/// no path).
pub(crate) fn stamp(answer: &mut Value, rev: &str) {
    let Value::Object(out) = answer else {
        return;
    };
    let mut put = Map::new();
    let mut said = false;
    let mut after_path = false;
    for (key, value) in std::mem::take(out) {
        if key == "rev" {
            continue;
        }
        if after_path && key != "form" {
            put.insert("rev".into(), rev.into());
            said = true;
            after_path = false;
        }
        let path = key == "path";
        let form = key == "form";
        put.insert(key, value);
        if path {
            after_path = true;
        } else if form && after_path {
            put.insert("rev".into(), rev.into());
            said = true;
            after_path = false;
        }
    }
    if !said {
        put.insert("rev".into(), rev.into());
    }
    *out = put;
}

/// The revs a call of `tool` gives, each with the path it is given for: with a target, the value's
/// for each target; without, each path's own. None for a tool that takes none.
pub(crate) fn given(
    tool: &str,
    arguments: &Map<String, Value>,
) -> Result<Vec<(String, String)>, String> {
    if !TAKES_REV.contains(&tool) {
        return Ok(Vec::new());
    }
    let Some(Value::Object(value)) = arguments.get("value") else {
        return Ok(Vec::new());
    };
    let read = |raw: &Value, at: &str| -> Result<String, String> {
        raw.as_str()
            .map(str::trim)
            .filter(|r| !r.is_empty())
            .map(str::to_string)
            .ok_or_else(|| text!("revs.given.shape", at))
    };
    // A target written wrong is said by the tool.
    let targets = super::Args::new(arguments).targets().ok().flatten();
    match targets {
        Some(list) => match value.get("rev") {
            None => Ok(Vec::new()),
            Some(raw) => {
                let rev = read(raw, "")?;
                Ok(list.into_iter().map(|t| (t, rev.clone())).collect())
            }
        },
        None => value
            .iter()
            .filter_map(|(path, entry)| {
                entry.get("rev").map(|raw| {
                    read(raw, &text!("revs.at.path", path)).map(|rev| (path.clone(), rev))
                })
            })
            .collect(),
    }
}

/// The arguments without the revs `given` reads (the tools write with what is left); none when
/// there are none.
pub(crate) fn without(tool: &str, arguments: &Map<String, Value>) -> Option<Map<String, Value>> {
    if !TAKES_REV.contains(&tool) {
        return None;
    }
    let Some(Value::Object(value)) = arguments.get("value") else {
        return None;
    };
    let mut value = value.clone();
    let took = if arguments.contains_key("target") {
        value.shift_remove("rev").is_some()
    } else {
        let mut took = false;
        for entry in value.values_mut() {
            if let Value::Object(entry) = entry {
                took |= entry.shift_remove("rev").is_some();
            }
        }
        took
    };
    if !took {
        return None;
    }
    let mut out = arguments.clone();
    out.insert("value".into(), Value::Object(value));
    Some(out)
}

/// The revs `given` held to the cards as `seen` (the tree before the call) has them: what is not
/// as it was read, said a line each — none when every one is. A path the tree did not have is
/// refused too: what the same call makes or renames to was not read.
pub(crate) fn stale(tx: &Tx, seen: &Seen, given: &[(String, String)]) -> Vec<String> {
    let project = seen.project();
    let mut out: Vec<String> = Vec::new();
    for (path, rev) in given {
        let Ok(target) = locate(&project, path, None) else {
            out.push(text!("revs.stale.notBefore", path));
            continue;
        };
        let id = owner(&target);
        let Some(now) = seen.rev(id) else {
            continue;
        };
        if now == *rev {
            continue;
        }
        let last = last_writer(tx, &seen.named.project, &seen.named.tree, id)
            .map(|said| text!("revs.stale.lastBy", said))
            .unwrap_or_default();
        let said = text!("revs.stale.changed", path, rev, now, last);
        if !out.contains(&said) {
            out.push(said);
        }
    }
    out
}

/// Who last wrote the card `id` in the tree, and when, as its history says it: the window, or MCP
/// with the access key's name (said to be this call's own when it is); none when the history keeps
/// no commit of it (cut short).
fn last_writer(tx: &Tx, project: &str, tree: &str, id: &str) -> Option<String> {
    let (kind, author, at): (String, Option<String>, String) = tx
        .conn()
        .query_row(
            "SELECT c.author_kind, c.author, c.at FROM commits c JOIN commit_rows r ON r.commit_id = c.id \
             WHERE c.project_id = ?1 AND c.branch_id = ?2 AND r.tbl = 'card_docs' AND r.row_id = ?3 \
             ORDER BY c.seq DESC LIMIT 1",
            [project, tree, id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()
        .ok()
        .flatten()?;
    let named = crate::write::author_said(&kind, author.as_deref());
    let who = if kind == "mcp" {
        let me = tx.who();
        if me.side == crate::write::Side::Mcp && author.is_some() && me.name == author {
            text!("revs.writer.thisKey", named)
        } else {
            named
        }
    } else {
        named
    };
    Some(text!("revs.writer.at", who, at))
}

/// A call's revs held to its tree before it writes (`stale`): refused, nothing written, when one
/// is not as it was read. Only MCP's: the window's writes give none, and any it gives is not read.
pub(crate) fn refuse_stale(
    tx: &Tx,
    scope: &Scope,
    given: &[(String, String)],
) -> Result<(), String> {
    if given.is_empty() || tx.who().side != crate::write::Side::Mcp {
        return Ok(());
    }
    let seen = Seen::in_tx(tx, scope)?;
    let lines = stale(tx, &seen, given);
    let mut refusals = super::args::Refusals::new(lines.len());
    for line in &lines {
        refusals.note(None, line);
    }
    refusals.check()
}

/// The cards a transaction wrote in a tree, by their paths as it is now, each with its rev: what a
/// batch run answers (`revs`). The cards it let go of are not in it.
pub(crate) fn written(tx: &Tx, scope: &Scope) -> Result<Map<String, Value>, String> {
    let ids = tx.cards_written(&scope.project, &scope.tree);
    let mut out = Map::new();
    if ids.is_empty() {
        return Ok(out);
    }
    let seen = Seen::in_tx(tx, scope)?;
    let project = seen.project();
    for id in ids {
        let path = match (project.card(&id), project.ghost(&id)) {
            (Some(card), _) => project.full(card),
            (None, Some(ghost)) => project.full_ghost(ghost),
            (None, None) => continue,
        };
        if let Some(rev) = seen.rev(&id) {
            out.insert(path, rev.into());
        }
    }
    Ok(out)
}
