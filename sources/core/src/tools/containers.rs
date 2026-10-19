//! Containers: a card whose
//! kind holds cards (`holds`: a flow its nodes, a goal its goals and tasks), and the cards
//! inside it — real cards that keep their container by id (`in`), named in it
//! (`Behavior:Checkout.review`, `Goals:Launch.Design`).
//!
//! Here: what may go inside what, a card moved into a container or out of it (`card_update`'s
//! `in`), what is inside a container that is let go of, a flow held to its nodes joining up,
//! the keys of before refused (`nodes`, `items`, `card_create`'s `in`), and a flow's nodes
//! placed on its card map.

use super::Seen;
use super::cards::Scope;
use crate::export::Project;
use crate::write::{Spot, Tx};
use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};

type Obj = Map<String, Value>;

/// `nodes`, refused: a flow's nodes are cards inside it.
pub(crate) fn nodes_gone() -> String {
    text!("containers.gone.nodes")
}

/// `items`, refused: a goal's cards are inside it.
pub(crate) fn items_gone() -> String {
    text!("containers.gone.items")
}

/// `card_create`'s `in`, refused: where a card goes is its path.
pub(crate) fn in_gone() -> String {
    text!("containers.gone.in")
}

/// A node's keys of before (a flow's `nodes`) that are lines now, and the line each is.
fn node_keys_gone() -> [(&'static str, String); 11] {
    [
        ("next", text!("containers.nodeKey.lineNext")),
        ("branches", text!("containers.nodeKey.lineBranch")),
        ("else", text!("containers.nodeKey.lineElse")),
        ("onError", text!("containers.nodeKey.lineOnError")),
        ("onTimeout", text!("containers.nodeKey.lineOnTimeout")),
        ("target", text!("containers.nodeKey.lineRuns")),
        ("event", text!("containers.nodeKey.lineEvent")),
        ("peer", text!("containers.nodeKey.linePeer")),
        ("peers", text!("containers.nodeKey.linePeer")),
        ("signWith", text!("containers.nodeKey.lineSign")),
        ("verifyWith", text!("containers.nodeKey.lineVerify")),
    ]
}

/// A node's value written in the keys of before (a flow's `nodes`), refused with the line
/// each is now: its ways, the flows it runs, waits for and talks to, its keys, and what
/// its process rows read, write, talk to and sign with.
pub(crate) fn refuse_node_keys(value: &Obj) -> Result<(), String> {
    let mut said: Vec<String> = Vec::new();
    let mut gone = |key: String, line: String| {
        said.push(text!("containers.nodeKey.gone", key, line));
    };
    for (key, line) in node_keys_gone() {
        if value.contains_key(key) {
            gone(key.to_string(), line);
        }
    }
    if let Some(reply) = value.get("return").and_then(Value::as_object) {
        for key in ["peer", "peers"] {
            if reply.contains_key(key) {
                gone(
                    text!("containers.nodeKey.ofReturn", key),
                    text!("containers.nodeKey.lineReturnPeer"),
                );
            }
        }
        if reply.contains_key("signWith") {
            gone(
                text!("containers.nodeKey.ofReturn", key = "signWith"),
                text!("containers.nodeKey.lineSign"),
            );
        }
    }
    for (i, step) in value
        .get("steps")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .enumerate()
    {
        let Some(step) = step.as_object() else {
            continue;
        };
        let op = step.get("do").and_then(Value::as_str).unwrap_or_default();
        let here = format!("steps[{i}]");
        if step.contains_key("paths") && matches!(op, "read" | "write") {
            let line = if op == "read" { "reads" } else { "writes" };
            gone(
                text!("containers.nodeKey.ofReadWrite", here, op),
                text!("containers.nodeKey.lineReadWrite", line, here),
            );
        }
        for key in ["peer", "peers"] {
            if step.contains_key(key) {
                gone(
                    text!("containers.nodeKey.ofStep", here, key),
                    text!("containers.nodeKey.lineStepPeer", here),
                );
            }
        }
        for (key, usage) in [("key", op), ("signWith", "sign"), ("verifyWith", "verify")] {
            if step.contains_key(key) {
                gone(
                    text!("containers.nodeKey.ofStep", here, key),
                    text!("containers.nodeKey.lineStepKey", here, usage),
                );
            }
        }
    }
    let Some(last) = said.last_mut() else {
        return Ok(());
    };
    last.push_str(&text!("containers.nodeKey.shapeNote"));
    Err(said.join("\n"))
}

/// Whether a container of `container_kind` may hold a card of `kind`.
pub(crate) fn require_holds(
    kinds: &crate::kinds::Kinds,
    container_kind: &str,
    container_path: &str,
    kind: &str,
) -> Result<(), crate::text::Keyed> {
    if kind == crate::detail::GHOST {
        return Err(keyed!("containers.holds.ghost", container_path));
    }
    let holds = kinds
        .get(container_kind)
        .map(|k| k.holds.clone())
        .unwrap_or_default();
    if holds.is_empty() {
        return Err(keyed!(
            "containers.holds.notContainer",
            container_path,
            container_kind,
            containers = crate::text::List::of(containers(kinds))
        ));
    }
    if !holds.iter().any(|h| h == kind) {
        return Err(keyed!(
            "containers.holds.onlyKinds",
            container_path,
            container_kind,
            holds = crate::text::List::of(holds),
            kind
        ));
    }
    Ok(())
}

/// The kinds that hold cards.
fn containers(kinds: &crate::kinds::Kinds) -> Vec<String> {
    kinds
        .placeable()
        .filter(|k| !k.holds.is_empty())
        .map(|k| k.name.clone())
        .collect()
}

/// A kind put only inside a container, refused where a section would hold it.
pub(crate) fn only_inside(kind: &str) -> String {
    text!(
        "containers.holds.onlyInside",
        kind,
        holders = crate::text::List::of(crate::kinds::holders_of(kind)),
        first = crate::kinds::holders_of(kind)
            .first()
            .copied()
            .unwrap_or("flow")
    )
}

/// What a card holds inside it, all the way down, by id: the cards in its own order.
pub(crate) fn inside_all<'a>(project: &Project<'a>, id: &str) -> Vec<&'a crate::model::Card> {
    let mut out = Vec::new();
    let mut queue = vec![id.to_string()];
    let mut seen = HashSet::new();
    while let Some(at) = queue.pop() {
        if !seen.insert(at.clone()) {
            continue;
        }
        for card in project.inner_of(&at) {
            queue.push(card.id.clone());
            out.push(card);
        }
    }
    out
}

/// The order a card made inside a container goes after the ones there: one past the
/// greatest of the tree's (`next_order`).
fn next_order(docs: &HashMap<String, Obj>) -> i64 {
    super::cards::next_order(docs)
}

fn put_order(doc: &mut Obj, order: i64) {
    if order == 0 {
        doc.shift_remove("order");
    } else {
        doc.insert("order".into(), order.into());
    }
}

// MARK: - A flow held to its nodes joining up

/// The flow a node card is in, or the flow itself: what a write of either is checked by.
pub(crate) fn flow_of(project: &Project, id: &str) -> Option<String> {
    let card = project.card(id)?;
    if card.kind == "flow" {
        return Some(card.id.clone());
    }
    let container = project.container_of(card)?;
    (container.kind == "flow").then(|| container.id.clone())
}

/// A flow's nodes held together: what is wrong refuses the write; what is only unfinished
/// is said (`check::flow_warnings`), as a flow is now written a node at a time.
pub(crate) fn require_flow(tx: &Tx, scope: &Scope, flow: &str) -> Result<(), String> {
    let seen = Seen::in_tx(tx, scope)?;
    let project = seen.project();
    let Some(card) = project.card(flow).filter(|c| c.kind == "flow") else {
        return Ok(());
    };
    match super::check::node_problems(&card.nodes, &project.defs.vias) {
        (Some(wrong), _) => Err(format!("{}: {wrong}", project.full(card))),
        _ => Ok(()),
    }
}

// MARK: - Moved in and out

/// `card_update`'s `in`: the card put inside the container the value names, at the end of
/// what is there — or, `null`, out of the one it is in, into its section. What is inside it
/// goes with it; the paths that named it are written again. Returns what was said.
pub(crate) fn move_into(
    tx: &mut Tx,
    scope: &Scope,
    id: &str,
    given: &Value,
) -> Result<Vec<String>, String> {
    let seen = Seen::in_tx(tx, scope)?;
    let project = seen.project();
    let card = project
        .card(id)
        .ok_or_else(|| text!("containers.card.missing"))?;
    let path = project.full(card);
    let before = seen.was();
    let mut docs = scope.docs(tx)?;
    let (container, section, said) = match given {
        Value::Null => {
            let Some(was) = project.container_of(card) else {
                return Err(text!("containers.move.notInside", path));
            };
            if crate::kinds::inner_only(&card.kind) {
                return Err(text!(
                    "containers.move.onlyInside",
                    path,
                    kind = card.kind,
                    holder = project.full(was),
                    only = only_inside(&card.kind)
                ));
            }
            let section = project.section_of(card);
            super::cards::require_fits(&project, &card.kind, &section)?;
            (
                None,
                section.clone(),
                keyed!(
                    "containers.line.moveOut",
                    path,
                    holder = project.full(was),
                    section = project.directory.full_section(&section)
                ),
            )
        }
        Value::String(raw) => {
            // A node's ways are to the nodes beside it: it is made anew in another flow.
            if crate::kinds::inner_only(&card.kind) {
                return Err(text!("containers.move.innerOnly", path, kind = card.kind));
            }
            let base = project.section_of(card);
            let target = super::walk::locate(&project, raw.trim(), Some(&base))
                .map_err(|e| text!("containers.move.inError", raw, e))?;
            let super::walk::Target::Card(into) = target else {
                return Err(text!("containers.move.notACard", raw));
            };
            let into_path = project.full(into);
            require_holds(project.defs.cards(), &into.kind, &into_path, &card.kind)?;
            if into.id == card.id
                || inside_all(&project, &card.id)
                    .iter()
                    .any(|c| c.id == into.id)
            {
                return Err(text!("containers.move.intoItself", into_path, path));
            }
            if card.inside.as_deref() == Some(into.id.as_str()) {
                return Ok(Vec::new());
            }
            (
                Some(into.id.clone()),
                project.section_of(into),
                keyed!("containers.line.moveInto", path, into_path),
            )
        }
        _ => return Err(text!("containers.move.inShape")),
    };
    // A name is one card's in its place.
    let place = container.clone().unwrap_or_else(|| section.clone());
    let taken = docs.iter().any(|(other, d)| {
        other != id
            && crate::write::holder(d) == place
            && d.get("name")
                .and_then(Value::as_str)
                .is_some_and(|n| n.eq_ignore_ascii_case(&card.name))
    });
    if taken {
        return Err(text!("containers.move.nameTaken", name = card.name));
    }
    let order = next_order(&docs);
    let moving: Vec<String> = std::iter::once(card.id.clone())
        .chain(inside_all(&project, &card.id).iter().map(|c| c.id.clone()))
        .collect();
    let old_container = card.inside.clone();
    for (i, moved) in moving.iter().enumerate() {
        let Some(mut doc) = docs.remove(moved) else {
            continue;
        };
        doc.insert("section".into(), section.clone().into());
        if i == 0 {
            match &container {
                Some(c) => {
                    doc.insert("in".into(), c.clone().into());
                }
                None => {
                    doc.shift_remove("in");
                }
            }
            put_order(&mut doc, order);
        }
        super::update::put_checked(tx, scope, &doc)?;
    }
    // Its box on the card map it left goes; a new one is found for it.
    if let Some(was) = &old_container {
        tx.place(&scope.project, &scope.tree, was, "card", id, None)?;
    }
    let rewritten = super::follow::follow(tx, scope, &before, &super::follow::InnerMoves::new())?;
    tx.line(
        &scope.project,
        &scope.tree,
        "schema.update",
        "schema",
        Some(id),
        said,
    )?;
    let mut out = Vec::new();
    if rewritten > 0 {
        out.push(text!("containers.move.rewritten", rewritten));
    }
    Ok(out)
}

/// A card about to be let go of: what is inside it goes too when it can be only inside
/// one (a flow's nodes), and out into the container's own place otherwise (a goal's cards
/// stay in its section, as they did when a goal held them as a list). Returns the paths
/// taken out, and those let go of.
pub(crate) fn empty_out(
    tx: &mut Tx,
    scope: &Scope,
    id: &str,
) -> Result<(Vec<String>, Vec<String>), String> {
    let seen = Seen::in_tx(tx, scope)?;
    let project = seen.project();
    let Some(card) = project.card(id) else {
        return Ok((Vec::new(), Vec::new()));
    };
    let inner = project.inner_of(&card.id);
    if inner.is_empty() {
        return Ok((Vec::new(), Vec::new()));
    }
    let mut docs = scope.docs(tx)?;
    let mut out = Vec::new();
    let mut gone = Vec::new();
    let place = card.inside.clone();
    let section = project.section_of(card);
    for held in inner {
        if crate::kinds::inner_only(&held.kind) {
            for deeper in inside_all(&project, &held.id) {
                tx.remove_card(&scope.project, &scope.tree, &deeper.id)?;
            }
            tx.remove_card(&scope.project, &scope.tree, &held.id)?;
            gone.push(project.full(held));
            continue;
        }
        let Some(mut doc) = docs.remove(&held.id) else {
            continue;
        };
        let to = place.clone().unwrap_or_else(|| section.clone());
        let clash = docs.iter().any(|(other, d)| {
            other != &held.id
                && other != id
                && crate::write::holder(d) == to
                && d.get("name")
                    .and_then(Value::as_str)
                    .is_some_and(|n| n.eq_ignore_ascii_case(&held.name))
        });
        if clash {
            return Err(text!(
                "containers.release.clash",
                container = project.full(card),
                name = held.name
            ));
        }
        match &place {
            Some(c) => {
                doc.insert("in".into(), c.clone().into());
            }
            None => {
                doc.shift_remove("in");
            }
        }
        super::update::put_checked(tx, scope, &doc)?;
        tx.place(&scope.project, &scope.tree, id, "card", &held.id, None)?;
        out.push(project.full(held));
    }
    Ok((out, gone))
}

/// The ways to a card about to be let go of from the cards beside it in its container (a
/// flow's nodes: `next` and the rest), let go of with it: the nodes they came from are left
/// unfinished rather than refused.
pub(crate) fn drop_ways_to(tx: &mut Tx, scope: &Scope, id: &str) -> Result<(), String> {
    let seen = Seen::in_tx(tx, scope)?;
    let project = seen.project();
    let Some(card) = project.card(id) else {
        return Ok(());
    };
    let Some(container) = card.inside.clone() else {
        return Ok(());
    };
    let siblings: Vec<String> = project
        .inner_of(&container)
        .iter()
        .filter(|c| c.id != id)
        .map(|c| c.id.clone())
        .collect();
    let defs = project.defs.clone();
    let mut docs = scope.docs(tx)?;
    for sibling in siblings {
        let Some(mut doc) = docs.remove(&sibling) else {
            continue;
        };
        let Some(Value::Array(lines)) = doc.get_mut("connections") else {
            continue;
        };
        let before = lines.len();
        lines.retain(|l| {
            let to_it = l.get("to").and_then(Value::as_str) == Some(id);
            let beside = l
                .get("line")
                .and_then(Value::as_str)
                .and_then(|n| defs.line(n))
                .is_some_and(|d| d.siblings);
            !(to_it && beside)
        });
        if lines.len() == before {
            continue;
        }
        if lines.is_empty() {
            doc.shift_remove("connections");
        }
        tx.put_card(&scope.project, &scope.tree, &doc)?;
    }
    Ok(())
}

// MARK: - Placed on a flow's card map

/// Where a flow's nodes with no place on its card map go (`grid::place_flow`), as (node id, spot):
/// worked out from the project as it is, with the layout's `margin`. Each flow's nodes are placed
/// by its own nodes alone, so the flows a write made are all worked out from one view of the
/// project and written after (`update::place_unplaced`).
pub(crate) fn nodes_placed(
    project: &Project,
    flow_id: &str,
    margin: i64,
) -> Vec<(String, (i64, i64))> {
    let Some(flow) = project.card(flow_id).filter(|c| c.kind == "flow") else {
        return Vec::new();
    };
    let by_name: HashMap<String, String> = project
        .inner_of(flow_id)
        .iter()
        .map(|c| (c.name.clone(), c.id.clone()))
        .collect();
    let at: HashMap<String, (i64, i64)> = flow
        .nodes
        .iter()
        .filter_map(|n| Some((n.name.clone(), n.top_left.map(|p| (p.x, p.y))?)))
        .collect();
    super::grid::place_flow(&flow.nodes, &at, margin)
        .into_iter()
        .filter_map(|(name, spot)| Some((by_name.get(&name)?.clone(), spot)))
        .collect()
}

/// A flow's nodes given the spots `nodes_placed` worked out: not pinned, and not a change to the
/// project.
pub(crate) fn place_nodes(
    tx: &mut Tx,
    scope: &Scope,
    flow_id: &str,
    spots: &[(String, (i64, i64))],
) -> Result<(), String> {
    for (id, (x, y)) in spots {
        tx.place(
            &scope.project,
            &scope.tree,
            flow_id,
            "card",
            id,
            Some(&Spot {
                x: Some(*x),
                y: Some(*y),
                ..Spot::default()
            }),
        )?;
    }
    Ok(())
}
