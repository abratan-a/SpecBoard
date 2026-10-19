//! Card ghosts written: a shortcut a section holds to a card of
//! another section, as an alias does to a file — the card is in one place only, and the
//! ghost is a name of its own in the section it is put in. It is a card of the kind
//! `ghost` (its document is a card's: its name, its section, and `target`, the id of what
//! it points at — a card, or another ghost), so it is named, renamed, moved, locked,
//! merged and kept in the history as a card is. What it points at is held by id: it
//! follows a rename or a move, and a card removed leaves the ghost pointing nowhere.
//!
//! Made by `card_create` (`kind: ghost`, `target`); here, what `card_update` changes of
//! one (`name`, `target`, `topLeft`; `section` and `position` as a card's) and
//! `card_delete`.

use super::Seen;
use super::cards::{Resolver, Scope, identifier};
use super::follow::{InnerMoves, follow};
use super::update::{broken_warning, newly_broken};
use super::views::Projection;
use crate::directory::Entry;
use crate::export::Project;
use crate::write::Tx;
use serde_json::{Map, Value};
use std::collections::HashMap;

/// What a card ghost's value may say to `card_update`: its name, what it points at and
/// where it sits — and, as any card, its section and its place among its section's ghosts.
const KEYS: [&str; 6] = ["name", "kind", "target", "topLeft", "section", "position"];

/// The section a card or a ghost is in, by its id.
fn section_of_id(project: &Project, id: &str) -> Option<String> {
    Some(match project.directory.entry_of(id)? {
        Entry::Card(i) => project.section_of(project.nth(i)),
        Entry::Ghost(g) => project.section_of_ghost(&project.detail.ghosts[g]),
    })
}

/// Why a ghost in `section` — `own`, or one not made yet — cannot point at `target` (an
/// id): itself, what is in its own section (named there by its name already), a chain
/// that comes back to it, or nothing there at all.
pub(crate) fn target_problem(
    project: &Project,
    section: &str,
    target: &str,
    own: Option<&str>,
) -> Option<String> {
    if own == Some(target) {
        return Some(text!("ghosts.target.self"));
    }
    let Some(named) = project.named(target) else {
        return Some(text!("ghosts.target.none"));
    };
    if section_of_id(project, target).as_deref() == Some(section) {
        return Some(text!("ghosts.target.sameSection", named = named));
    }
    let own = own?;
    // A chain that comes back to it goes round.
    let mut chain = vec![project.named(own).unwrap_or_default(), named];
    let mut at = target.to_string();
    let mut passed: Vec<String> = vec![at.clone()];
    while let Some(next) = project.ghost(&at).and_then(|g| g.target.clone()) {
        chain.push(project.named(&next).unwrap_or_else(|| next.clone()));
        if next == own {
            return Some(text!("ghosts.target.circle", chain = chain.join(" → ")));
        }
        if passed.contains(&next) {
            break;
        }
        passed.push(next.clone());
        at = next;
    }
    None
}

/// The card ghost `id`'s document, as the tree sees it.
fn doc_of(tx: &Tx, scope: &Scope, id: &str) -> Result<Map<String, Value>, String> {
    scope
        .docs(tx)?
        .remove(id)
        .ok_or_else(|| text!("ghosts.gone"))
}

/// `card_update` of a card ghost, less its section and its place among the ghosts (the
/// caller's, as a card's): a new name — the paths written through it follow — and a new
/// target, one that leads to a card of the kind it leads to now (any, when it leads
/// nowhere now), not round and not in its own section; and where it sits. Returns what
/// was said.
pub(crate) fn update(
    tx: &mut Tx,
    scope: &Scope,
    id: &str,
    value: &Map<String, Value>,
) -> Result<Vec<String>, String> {
    let seen = Seen::in_tx(tx, scope)?;
    let project = seen.project();
    let ghost = project.ghost(id).ok_or_else(|| text!("ghosts.gone"))?;
    let display = project.full_ghost(ghost);
    let stray: Vec<&str> = value
        .keys()
        .map(String::as_str)
        .filter(|k| !KEYS.contains(k))
        .collect();
    if !stray.is_empty() {
        let card = project
            .ghost_card(ghost)
            .map(|c| text!("ghosts.update.cardHint", card = project.full(c)))
            .unwrap_or_default();
        return Err(text!(
            "ghosts.update.strayKeys",
            display = display,
            keys = crate::text::list(&stray),
            card = card
        ));
    }
    if let Some(kind) = value.get("kind")
        && kind.as_str() != Some(crate::detail::GHOST)
    {
        return Err(text!("ghosts.update.notGhostKind", display = display));
    }
    let section = project.section_of_ghost(ghost);
    let mut doc = doc_of(tx, scope, id)?;
    let mut changed: Vec<crate::text::Keyed> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();

    // Its name, as a card's: one of its own in its section.
    let renamed = match value.get("name").and_then(Value::as_str) {
        Some(raw) => {
            let name = identifier(raw, &text!("ghosts.field.name"))?;
            (name != ghost.name).then_some(name)
        }
        None => None,
    };
    if let Some(name) = &renamed {
        let taken = scope.docs(tx)?.iter().any(|(other, d)| {
            other != id
                && d.get("section").and_then(Value::as_str) == Some(section.as_str())
                && d.get("name")
                    .and_then(Value::as_str)
                    .is_some_and(|n| crate::pathtext::same(n, name))
        });
        if taken {
            return Err(text!("ghosts.update.nameTaken", name = name));
        }
        doc.insert("name".into(), name.clone().into());
        changed.push(keyed!("ghosts.change.name", name = name));
    }

    // What it points at: the card it leads to stays of one kind.
    if let Some(raw) = value.get("target") {
        let raw = raw
            .as_str()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| text!("ghosts.update.targetShape"))?;
        let made = HashMap::new();
        let resolver = Resolver {
            project: &project,
            base: section.clone(),
            made: &made,
            inside: None,
            unmade: None,
        };
        let next = crate::kinds::Names::id(&resolver, raw).ok_or_else(|| {
            let names: Vec<String> = project
                .detail
                .schemas
                .iter()
                .map(|c| c.name.clone())
                .collect();
            project.directory.find_refusal(raw, Some(&section), &names)
        })?;
        if let Some(problem) = target_problem(&project, &section, &next, Some(id)) {
            return Err(problem);
        }
        let reached = project
            .card(&next)
            .ok_or_else(|| text!("ghosts.update.noTarget", raw = raw))?;
        if let Some(now) = project.ghost_card(ghost)
            && now.kind != reached.kind
        {
            return Err(text!(
                "ghosts.update.kindChange",
                kind = now.kind,
                raw = raw,
                reached = reached.kind
            ));
        }
        if ghost.target.as_deref() != Some(next.as_str()) {
            let fields = doc
                .entry("fields")
                .or_insert_with(|| Value::Object(Map::new()))
                .as_object_mut()
                .ok_or_else(|| text!("tools.doc.fieldsNotObject"))?;
            fields.insert("target".into(), next.clone().into());
            changed.push(keyed!(
                "ghosts.change.target",
                target = project.named(&next).unwrap_or_default()
            ));
        }
    }

    let placing = value
        .get("topLeft")
        .map(super::update::grid_point)
        .transpose()?;
    if changed.is_empty() && placing.is_none() {
        // Nothing of it changes: a name or a target as they are.
        return Ok(warnings);
    }
    let broken_before = (!changed.is_empty()).then(|| seen.broken_keys());
    let before = renamed.as_ref().map(|_| seen.was());
    drop(seen);
    if !changed.is_empty() {
        tx.put_card(&scope.project, &scope.tree, &doc)?;
    }
    // The paths written through it, by its old name, say its new one.
    if let Some(before) = before {
        let rewritten = follow(tx, scope, &before, &InnerMoves::new())?;
        if rewritten > 0 {
            changed.push(keyed!("ghosts.change.paths", count = rewritten));
            warnings.push(text!(
                "ghosts.update.rewritten",
                count = rewritten,
                name = renamed.unwrap_or_default()
            ));
        }
    }
    if !changed.is_empty() {
        tx.line(
            &scope.project,
            &scope.tree,
            "schema.update",
            "schema",
            Some(id),
            keyed!(
                "ghosts.line.update",
                display = display,
                changes = super::slashed(changed)
            ),
        )?;
    }
    if let Some(before) = broken_before {
        warnings.extend(
            newly_broken(tx, scope, &before, None)?
                .iter()
                .map(broken_warning),
        );
    }
    if let Some(at) = placing {
        warnings.extend(super::update::place_at(tx, scope, id, at)?);
    }
    Ok(warnings)
}

/// What a write of a card ghost answers with: the ghost, found again, with what was said.
pub(crate) fn written(
    tx: &Tx,
    scope: &Scope,
    id: &str,
    warnings: Vec<String>,
) -> Result<Value, String> {
    let seen = Seen::in_tx(tx, scope)?;
    let project = seen.project();
    let ghost = project
        .ghost(id)
        .ok_or_else(|| text!("ghosts.written.reread"))?;
    let view = seen.ghost_view(ghost, &Projection::none(), None);
    Ok(super::warned(view, &warnings))
}

/// A card ghost let go (`card_delete`), as a card is: answered with its path, how many
/// references written through it now point nowhere (`danglingRefs`), and the paths it
/// leaves unresolved — those written through it, and the ghosts that pointed at it.
pub(crate) fn delete(tx: &mut Tx, scope: &Scope, id: &str) -> Result<Value, String> {
    let seen = Seen::in_tx(tx, scope)?;
    let project = seen.project();
    let ghost = project.ghost(id).ok_or_else(|| text!("ghosts.gone"))?;
    let full = project.full_ghost(ghost);
    let Some(Entry::Ghost(at)) = project.directory.entry_of(id) else {
        return Err(text!("ghosts.gone"));
    };
    // A reference counts when the ghosts it went through pass this one.
    let passes = |via: &str| match project.directory.entry_of(via) {
        Some(Entry::Ghost(g)) => project.directory.chain(g).contains(&at),
        _ => false,
    };
    let referrers = seen
        .detail
        .relations
        .iter()
        .filter(|r| r.via_ghost_id.as_deref().is_some_and(passes))
        .count();
    let before = seen.broken_keys();
    drop(seen);
    tx.remove_card(&scope.project, &scope.tree, id)?;
    tx.line(
        &scope.project,
        &scope.tree,
        "schema.delete",
        "schema",
        Some(id),
        keyed!("ghosts.line.delete", full = full),
    )?;
    let warnings: Vec<String> = newly_broken(tx, scope, &before, None)?
        .iter()
        .map(broken_warning)
        .collect();
    let mut out = Map::new();
    if !warnings.is_empty() {
        out.insert(
            "warnings".into(),
            Value::Array(warnings.into_iter().map(Value::from).collect()),
        );
    }
    out.insert("deleted".into(), full.into());
    out.insert("danglingRefs".into(), referrers.into());
    Ok(Value::Object(out))
}
