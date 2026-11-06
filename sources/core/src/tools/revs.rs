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
    if TAKES_META_REV.contains(&tool) {
        let Some(Value::Object(value)) = arguments.get("value") else {
            return None;
        };
        let mut value = value.clone();
        value.shift_remove("rev")?;
        let mut out = arguments.clone();
        out.insert("value".into(), Value::Object(value));
        return Some(out);
    }
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

/// The tools whose value may give the rev of a section, the project's settings or a kind: what
/// several sessions write side by side as they do cards, so a write made from what was read is
/// held to it in the same way.
const TAKES_META_REV: [&str; 5] = [
    "section_update",
    "section_delete",
    "project_update",
    "kind_update",
    "kind_delete",
];

/// What a rev other than a card's is of.
pub(crate) enum Meta {
    /// A section, by its path as the call writes it.
    Section(String),
    /// The project's settings.
    Project,
    /// A kind, by its name.
    Kind(String),
}

impl Meta {
    fn said(&self) -> String {
        match self {
            Meta::Section(path) => text!("revs.what.section", path),
            Meta::Project => text!("revs.what.project"),
            Meta::Kind(name) => text!("revs.what.kind", name),
        }
    }
}

/// The revs of sections, the project or kinds a call of `tool` gives (`value.rev`), each with what it
/// is of: for each target, or the project's. None for a tool that takes none.
pub(crate) fn given_meta(
    tool: &str,
    arguments: &Map<String, Value>,
) -> Result<Vec<(Meta, String)>, String> {
    if !TAKES_META_REV.contains(&tool) {
        return Ok(Vec::new());
    }
    let Some(Value::Object(value)) = arguments.get("value") else {
        return Ok(Vec::new());
    };
    let Some(raw) = value.get("rev") else {
        return Ok(Vec::new());
    };
    let rev = raw
        .as_str()
        .map(str::trim)
        .filter(|r| !r.is_empty())
        .map(str::to_string)
        .ok_or_else(|| text!("revs.given.metaShape", tool))?;
    if tool == "project_update" {
        return Ok(vec![(Meta::Project, rev)]);
    }
    let targets = super::Args::new(arguments)
        .targets()
        .ok()
        .flatten()
        .unwrap_or_default();
    Ok(targets
        .into_iter()
        .map(|t| {
            let of = if tool.starts_with("section") {
                Meta::Section(t)
            } else {
                Meta::Kind(t)
            };
            (of, rev.clone())
        })
        .collect())
}

/// A section's rev: its own record — name, description, parent, place among its siblings, kind,
/// figure and mark — not the cards in it.
pub(crate) fn section_rev(detail: &crate::model::Detail, id: &str) -> Option<String> {
    let section = detail.sections.iter().find(|s| s.id == id)?;
    let position = detail
        .sections
        .iter()
        .filter(|s| s.parent_id == section.parent_id)
        .position(|s| s.id == id);
    let mut doc = Map::new();
    doc.insert("name".into(), section.name.clone().into());
    doc.insert("description".into(), section.description.clone().into());
    doc.insert("parent".into(), section.parent_id.clone().into());
    doc.insert("position".into(), position.into());
    doc.insert("kind".into(), section.kind.clone().into());
    doc.insert("figure".into(), section.figure.clone().into());
    let icon = section.icon.as_ref();
    doc.insert(
        "icon".into(),
        serde_json::json!([
            icon.and_then(|i| i.name.clone()),
            icon.and_then(|i| i.color.clone())
        ]),
    );
    Some(rev_of(&doc))
}

/// The project's rev: its settings as `project_update` writes them — name, slug, description, card
/// size, writing rules, the order of the sections under the root, export language — not its cards.
pub(crate) fn project_rev(
    conn: &rusqlite::Connection,
    project: &str,
    detail: &crate::model::Detail,
) -> Result<String, String> {
    let (name, slug, description, rules, language): (
        String,
        String,
        String,
        Option<String>,
        String,
    ) = conn
        .query_row(
            "SELECT name, slug, description, rules, export_language FROM projects WHERE id = ?",
            [project],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )
        .map_err(crate::store::sql)?;
    let root = detail.sections.iter().find(|s| s.parent_id.is_none());
    let order: Vec<Value> = detail
        .sections
        .iter()
        .filter(|s| root.is_some_and(|r| s.parent_id.as_deref() == Some(r.id.as_str())))
        .map(|s| Value::from(s.id.clone()))
        .collect();
    let mut doc = Map::new();
    doc.insert("name".into(), name.into());
    doc.insert("slug".into(), slug.into());
    doc.insert("description".into(), description.into());
    doc.insert("rules".into(), rules.into());
    doc.insert("exportLanguage".into(), language.into());
    doc.insert(
        "layout".into(),
        super::grid::Layout::read(conn, project)?.view(),
    );
    doc.insert("order".into(), Value::Array(order));
    Ok(rev_of(&doc))
}

/// The rev now of what `of` names, in `seen`; none when it is not there (the tool says so).
fn meta_rev(tx: &Tx, seen: &Seen, of: &Meta) -> Option<String> {
    match of {
        Meta::Section(path) => {
            let id = seen.project().directory.section_named(path.trim(), None)?;
            section_rev(&seen.detail, &id)
        }
        Meta::Project => project_rev(tx.conn(), &seen.summary.id, &seen.detail).ok(),
        Meta::Kind(name) => super::kind_tools::kind_rev(tx.conn(), &seen.summary.id, name),
    }
}

/// The revs `given_meta` read, held to the tree before the call: what is not as it was read, said
/// a line each.
pub(crate) fn stale_meta(tx: &Tx, seen: &Seen, given: &[(Meta, String)]) -> Vec<String> {
    let mut out = Vec::new();
    for (of, rev) in given {
        let Some(now) = meta_rev(tx, seen, of) else {
            continue;
        };
        if now != *rev {
            let said = text!("revs.stale.meta", what = of.said(), rev, now);
            if !out.contains(&said) {
                out.push(said);
            }
        }
    }
    out
}

/// A call's revs of sections, the project or kinds held to its tree before it writes: refused,
/// nothing written, when one is not as it was read. Only MCP's.
pub(crate) fn refuse_stale_meta(
    tx: &Tx,
    scope: &Scope,
    given: &[(Meta, String)],
) -> Result<(), String> {
    if given.is_empty() || tx.who().side != crate::write::Side::Mcp {
        return Ok(());
    }
    let seen = Seen::in_tx(tx, scope)?;
    let lines = stale_meta(tx, &seen, given);
    let mut refusals = super::args::Refusals::new(lines.len());
    for line in &lines {
        refusals.note(None, line);
    }
    refusals.check()
}

/// The rev of what a write of a section, the project or a kind leaves, put in its answer (MCP's
/// only): the section or kind it names (by its new name when it renames it), the project's.
pub(crate) fn stamp_meta(
    tx: &Tx,
    scope: &Scope,
    tool: &str,
    arguments: &Map<String, Value>,
    mut answer: Value,
) -> Result<Value, String> {
    if tx.who().side != crate::write::Side::Mcp {
        return Ok(answer);
    }
    let renamed = arguments
        .get("value")
        .and_then(|v| v.get("name"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let target = arguments
        .get("target")
        .and_then(Value::as_str)
        .map(str::to_string);
    let of = match tool {
        "project_update" => Meta::Project,
        "section_create" | "section_update" => match (tool, renamed, target) {
            // A section renamed keeps its parent: the new name in the place of the last part.
            ("section_update", Some(name), Some(path)) => Meta::Section(
                path.rsplit_once('/')
                    .map(|(parent, _)| format!("{parent}/{name}"))
                    .unwrap_or(name),
            ),
            (_, _, Some(path)) => Meta::Section(path),
            _ => return Ok(answer),
        },
        "kind_create" | "kind_update" => match renamed.or(target) {
            Some(name) => Meta::Kind(name),
            None => return Ok(answer),
        },
        _ => return Ok(answer),
    };
    let seen = Seen::in_tx(tx, scope)?;
    if let (Some(rev), Value::Object(out)) = (meta_rev(tx, &seen, &of), &mut answer) {
        out.insert("rev".into(), rev.into());
    }
    Ok(answer)
}
