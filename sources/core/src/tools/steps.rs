//! A process node's steps one at a time, by their places (`Charts/Shop:Checkout.save.steps[1]`,
//! the text a line's `from` names a step by): added at a place, changed, moved, and let go of.
//! A step has no name, only its place, so whatever names a step by its place is renumbered with
//! it — the node's lines that leave a step, and the paths written elsewhere that name one — through
//! the same following a formatdoc's moved rows take (`follow`). The node is held to its kind once
//! all of that is written, not between: a step moved away from its lines is not a node to check.

use super::Seen;
use super::cards::{Resolver, Scope};
use super::follow::{InnerMoves, follow, store_in_full};
use super::inner::wire_list;
use super::update::{broken_warning, newly_broken, put_checked};
use super::walk::{Target, locate, step_count};
use crate::write::Tx;
use serde_json::{Map, Value};
use std::collections::HashMap;

/// A step's place as a path names it.
fn step_name(index: usize) -> String {
    format!("steps[{index}]")
}

/// Where `position` puts a step among `count` places: from 0, past the end at the end.
fn place(given: &Value, count: usize) -> Result<usize, String> {
    given
        .as_i64()
        .filter(|p| *p >= 0)
        .map(|p| (p as usize).min(count))
        .ok_or_else(|| text!("steps.error.position"))
}

/// A line of the node as `card_read` reads it: its kind, the step it leaves, where it lands.
fn line_read(project: &crate::export::Project, line: &Value) -> Value {
    let mut out = Map::new();
    for key in ["line", "from"] {
        if let Some(said) = line.get(key) {
            out.insert(key.into(), said.clone());
        }
    }
    if let Some(to) = line.get("to").and_then(Value::as_str) {
        out.insert(
            "to".into(),
            project.named(to).map(Value::from).unwrap_or(Value::Null),
        );
    }
    if let Some(at) = line.get("at") {
        out.insert("at".into(), at.clone());
    }
    Value::Object(out)
}

/// What a write of the steps came to: what was said, and the lines let go of with a step.
struct Written {
    said: Vec<String>,
    removed: Vec<Value>,
}

/// The steps written back whole. `order[i]` is where the step now at `i` was (none for a step
/// made); `gone` is a step let go of, whose lines go with it. Whatever names a step by its place is
/// named again by its place now; a path elsewhere that named the step let go of is left naming
/// none, so it is said broken rather than landing on the step after it.
fn write_back(
    tx: &mut Tx,
    scope: &Scope,
    id: &str,
    list: Vec<Value>,
    order: &[Option<usize>],
    gone: Option<usize>,
    said: crate::text::Keyed,
) -> Result<Written, String> {
    let (was, broken_before, removed, doc) = {
        let seen = Seen::in_tx(tx, scope)?;
        let project = seen.project();
        let card = project
            .card(id)
            .ok_or_else(|| text!("containers.card.missing"))?;
        let def = project
            .defs
            .cards()
            .get(&card.kind)
            .ok_or_else(|| text!("inner.error.noDefinition", kind = card.kind))?;
        let mut doc = scope
            .doc(tx, id)?
            .ok_or_else(|| text!("containers.card.missing"))?;
        let resolver = Resolver {
            project: &project,
            base: project.section_of(card),
            made: &HashMap::new(),
            inside: project.container_of(card).map(|c| project.full(c)),
            unmade: None,
        };
        let mut wire = Map::new();
        wire.insert("steps".into(), Value::Array(list));
        let prior = Value::Object(doc.clone());
        let parts = crate::kinds::from_wire(
            def,
            &wire,
            Some(&prior),
            &resolver,
            &mut crate::write::new_id,
        )
        .map_err(|problems| {
            problems
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("\n")
        })?;
        let rows = doc
            .entry("rows")
            .or_insert_with(|| Value::Object(Map::new()))
            .as_object_mut()
            .ok_or_else(|| text!("update.doc.rowsNotObject"))?;
        match parts.rows.get("steps") {
            Some(Value::Array(steps)) if !steps.is_empty() => {
                rows.insert("steps".into(), Value::Array(steps.clone()));
            }
            _ => {
                rows.shift_remove("steps");
            }
        }
        if rows.is_empty() {
            doc.shift_remove("rows");
        }
        // The lines that leave the step let go of, with it.
        let mut removed = Vec::new();
        if let Some(index) = gone
            && let Some(lines) = doc.get_mut("connections").and_then(Value::as_array_mut)
        {
            let from = step_name(index);
            lines.retain(|line| {
                let leaves = line.get("from").and_then(Value::as_str) == Some(from.as_str());
                if leaves {
                    removed.push(line_read(&project, line));
                }
                !leaves
            });
            if lines.is_empty() {
                doc.shift_remove("connections");
            }
        }
        (seen.was(), seen.broken_keys(), removed, doc)
    };
    // Each step whose place changed, from where to where; the one let go of to a place past the
    // end, which names nothing.
    let mut moves: Vec<(Vec<String>, Vec<String>)> = order
        .iter()
        .enumerate()
        .filter_map(|(now, before)| {
            before
                .filter(|b| *b != now)
                .map(|b| (vec![step_name(b)], vec![step_name(now)]))
        })
        .collect();
    if let Some(index) = gone {
        moves.push((vec![step_name(index)], vec![step_name(order.len())]));
    }
    let mut out = Written {
        said: Vec::new(),
        removed,
    };
    if moves.is_empty() {
        put_checked(tx, scope, &doc)?;
    } else {
        // Written as it is first, its lines and the paths elsewhere renumbered after, and only
        // then held to its kind as a whole.
        tx.put_card(&scope.project, &scope.tree, &doc)?;
        let mut inner = InnerMoves::new();
        inner.insert(id.to_string(), moves);
        let rewritten = follow(tx, scope, &was, &inner)?;
        let now = scope
            .doc(tx, id)?
            .ok_or_else(|| text!("containers.card.missing"))?;
        put_checked(tx, scope, &now)?;
        if rewritten > 0 {
            out.said
                .push(text!("steps.warn.renumbered", count = rewritten));
        }
    }
    super::lines::require_fit(tx, scope, id)?;
    store_in_full(tx, scope, &[id.to_string()])?;
    tx.line(
        &scope.project,
        &scope.tree,
        "schema.update",
        "schema",
        Some(id),
        said,
    )?;
    out.said.extend(
        newly_broken(tx, scope, &broken_before, Some(id))?
            .iter()
            .map(broken_warning),
    );
    Ok(out)
}

/// The step at `index` as `card_read` reads it, with what was said.
fn answer(
    tx: &Tx,
    scope: &Scope,
    id: &str,
    index: usize,
    said: &[String],
) -> Result<Value, String> {
    let seen = Seen::in_tx(tx, scope)?;
    let project = seen.project();
    let card = project
        .card(id)
        .ok_or_else(|| text!("inner.error.unreadable"))?;
    Ok(super::warned(seen.step_view(card, index), said))
}

/// The node a step's path is in, and the place the path names: `<node>.steps[n]`.
fn node_at(
    project: &crate::export::Project,
    path: &str,
) -> Result<(String, String, usize), String> {
    let (parent, last) =
        crate::pathtext::last_dot(path.trim()).ok_or_else(|| text!("steps.error.path", path))?;
    let node = match locate(project, parent, None)? {
        Target::Card(card) if card.kind == "process" => card,
        _ => return Err(text!("steps.error.noProcess", path)),
    };
    let index =
        crate::detail::step_index(last.trim()).ok_or_else(|| text!("steps.error.path", path))?;
    Ok((node.id.clone(), project.full(node), index))
}

/// A step added at the place its path says (`steps[1]`): the steps from there on move one place
/// on, and what names them with them. The place after the last adds it at the end.
pub(crate) fn create(
    tx: &mut Tx,
    scope: &Scope,
    path: &str,
    value: &Map<String, Value>,
) -> Result<Value, String> {
    let (id, full, index, count) = {
        let seen = Seen::in_tx(tx, scope)?;
        let project = seen.project();
        let (id, full, index) = node_at(&project, path)?;
        let count = project.card(&id).map_or(0, step_count);
        (id, full, index, count)
    };
    if index > count {
        return Err(text!("steps.error.beyond", path, node = full, count));
    }
    let mut list = wire_list(tx, scope, &id, "steps")?;
    list.insert(index, Value::Object(value.clone()));
    let order: Vec<Option<usize>> = (0..=count)
        .map(|now| match now.cmp(&index) {
            std::cmp::Ordering::Less => Some(now),
            std::cmp::Ordering::Equal => None,
            std::cmp::Ordering::Greater => Some(now - 1),
        })
        .collect();
    let written = write_back(
        tx,
        scope,
        &id,
        list,
        &order,
        None,
        keyed!("steps.line.added", full, step = step_name(index)),
    )?;
    answer(tx, scope, &id, index, &written.said)
}

/// What the value says, over what the step was (null takes a key away); `position` moves it, and
/// what names the steps by their places follows them.
pub(crate) fn update(
    tx: &mut Tx,
    scope: &Scope,
    id: &str,
    index: usize,
    value: &Map<String, Value>,
) -> Result<Value, String> {
    if value.is_empty() {
        return Err(text!("inner.error.nothingToChange"));
    }
    let full = {
        let seen = Seen::in_tx(tx, scope)?;
        let project = seen.project();
        project
            .card(id)
            .map(|c| project.full(c))
            .unwrap_or_default()
    };
    let mut list = wire_list(tx, scope, id, "steps")?;
    let count = list.len();
    let step = list
        .get_mut(index)
        .and_then(Value::as_object_mut)
        .ok_or_else(|| text!("inner.error.unreadable"))?;
    for (key, given) in value.iter().filter(|(k, _)| *k != "position") {
        if given.is_null() {
            step.shift_remove(key);
        } else {
            step.insert(key.clone(), given.clone());
        }
    }
    let mut order: Vec<Option<usize>> = (0..count).map(Some).collect();
    let mut now = index;
    if let Some(given) = value.get("position") {
        now = place(given, count - 1)?;
        let moved = list.remove(index);
        list.insert(now, moved);
        let was = order.remove(index);
        order.insert(now, was);
    }
    let said = if now == index {
        keyed!("steps.line.changed", full, step = step_name(index))
    } else {
        keyed!(
            "steps.line.moved",
            full,
            step = step_name(index),
            now = step_name(now)
        )
    };
    let written = write_back(tx, scope, id, list, &order, None, said)?;
    answer(tx, scope, id, now, &written.said)
}

/// A step let go of, with the node's lines that leave it (said in the answer); the steps after it
/// move one place back, and what names them with them.
pub(crate) fn delete(tx: &mut Tx, scope: &Scope, id: &str, index: usize) -> Result<Value, String> {
    let full = {
        let seen = Seen::in_tx(tx, scope)?;
        let project = seen.project();
        project
            .card(id)
            .map(|c| project.full(c))
            .unwrap_or_default()
    };
    let mut list = wire_list(tx, scope, id, "steps")?;
    let count = list.len();
    list.remove(index);
    let order: Vec<Option<usize>> = (0..count).filter(|i| *i != index).map(Some).collect();
    let written = write_back(
        tx,
        scope,
        id,
        list,
        &order,
        Some(index),
        keyed!(
            "steps.line.removed",
            full = full.clone(),
            step = step_name(index)
        ),
    )?;
    let mut out = Map::new();
    out.insert("project".into(), scope.project_name.clone().into());
    out.insert(
        "deleted".into(),
        format!("{full}.{}", step_name(index)).into(),
    );
    out.insert("stepCount".into(), (count - 1).into());
    if !written.removed.is_empty() {
        out.insert("linesRemoved".into(), Value::Array(written.removed));
    }
    Ok(super::warned(Value::Object(out), &written.said))
}
