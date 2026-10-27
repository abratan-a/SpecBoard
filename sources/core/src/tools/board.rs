//! goal and task read as a list of issues: `card_list`'s
//! `fields` (by the values of a kind's fields; a custom card's before) and `in` (what a
//! goal holds, all the way down), a goal's progress, and goals in the order their labels
//! number them.

use super::Seen;
use crate::model::Card;
use serde_json::{Map, Value};
use std::collections::{HashSet, VecDeque};

/// A card's value of one of its kind's fields, as MCP writes it: what it holds, or the
/// field's default. None when its kind has no such field.
fn field_value(kinds: &crate::kinds::Kinds, card: &Card, name: &str) -> Option<Value> {
    let kind = kinds.get(&card.kind)?;
    let def = kind.field(name)?;
    let held = match &card.form {
        Some(form) => form["fields"].get(name).cloned(),
        None => match name {
            "label" => Some(card.label.clone().into()),
            "description" => Some(card.description.clone().into()),
            "tags" => Some(card.tags.clone().into()),
            "body" => Some(card.body.clone().into()),
            "open" => Some(card.enum_open.into()),
            "diagram" => card.diagram.clone().map(Value::from),
            _ => None,
        },
    };
    // A box not ticked is not written: it reads as false.
    let unwritten = match def.fieldtype {
        crate::kinds::FieldType::Checkbox => Value::Bool(false),
        _ => Value::Null,
    };
    held.filter(|v| !v.is_null())
        .or_else(|| def.default.clone())
        .or(Some(unwritten))
}

/// Whether a card counts as done: a field of its kind that says what done is holds one
/// of those values (a task's `done`, a goal's `status` of 完了).
pub(crate) fn is_done(kinds: &crate::kinds::Kinds, card: &Card) -> bool {
    let Some(kind) = kinds.get(&card.kind) else {
        return false;
    };
    kind.fields
        .iter()
        .filter(|f| !f.done.is_empty())
        .any(|f| field_value(kinds, card, &f.name).is_some_and(|v| f.done.contains(&v)))
}

/// The cards inside a container, by id, in their order: a goal's goals and tasks.
pub(crate) fn items(project: &crate::export::Project, card: &Card) -> Vec<String> {
    project
        .inner_of(&card.id)
        .iter()
        .map(|c| c.id.clone())
        .collect()
}

/// A goal's progress: of the cards inside it, how many are done (a goal inside counts as
/// one, done when it is complete).
pub(crate) fn progress(seen: &Seen, goal: &Card) -> Option<Value> {
    if goal.kind != "goal" {
        return None;
    }
    let project = seen.project();
    let held: Vec<&Card> = project.inner_of(&goal.id);
    let mut out = Map::new();
    out.insert(
        "done".into(),
        held.iter()
            .filter(|c| is_done(project.defs.cards(), c))
            .count()
            .into(),
    );
    out.insert("of".into(), held.len().into());
    Some(Value::Object(out))
}

/// `fields`: the cards whose fields hold what it says — each value, or any of a list of
/// them; a field of many values holding it.
pub(crate) fn matching(
    kinds: &crate::kinds::Kinds,
    cards: &mut Vec<&Card>,
    given: &Value,
) -> Result<(), String> {
    let shape = || text!("board.fields.shape");
    let wanted = given.as_object().ok_or_else(shape)?;
    // A goal's field by its name of before, one the board's kinds no longer have: said how it
    // is written now, not picked by nothing.
    for name in wanted.keys() {
        let held = cards
            .iter()
            .any(|c| kinds.get(&c.kind).is_some_and(|k| k.field(name).is_some()));
        if held {
            continue;
        }
        if let Some(before) = crate::kinds::BOARD_KINDS
            .iter()
            .find_map(|kind| crate::kinds::board_key_before(kind, name))
        {
            return Err(before);
        }
    }
    // A goal's value of before (`進行中`), said with its value of now.
    for (name, value) in wanted {
        let values: Vec<&Value> = match value {
            Value::Array(list) => list.iter().collect(),
            other => vec![other],
        };
        for v in values.into_iter().filter_map(Value::as_str) {
            if let Some((_, _, now)) = crate::kinds::GOAL_VALUES_BEFORE
                .iter()
                .find(|(key, before, _)| key == name && *before == v)
            {
                return Err(text!("board.fields.valueBefore", name, before = v, now));
            }
        }
    }
    for value in wanted.values() {
        let scalar = |v: &Value| !v.is_array() && !v.is_object();
        let fine = match value {
            Value::Array(list) => list.iter().all(scalar),
            other => scalar(other),
        };
        if !fine {
            return Err(shape());
        }
    }
    cards.retain(|card| {
        wanted.iter().all(|(name, value)| {
            let Some(held) = field_value(kinds, card, name) else {
                return false;
            };
            let alternatives: Vec<&Value> = match value {
                Value::Array(list) => list.iter().collect(),
                one => vec![one],
            };
            match &held {
                Value::Array(list) => list.iter().any(|h| alternatives.contains(&h)),
                one => alternatives.contains(&one),
            }
        })
    });
    Ok(())
}

/// `in`: what the container at `path` holds, all the way down (a goal's goals and tasks, a
/// flow's nodes).
pub(crate) fn inside(
    seen: &Seen,
    path: &str,
    base: Option<&str>,
) -> Result<HashSet<String>, String> {
    let project = seen.project();
    let container = super::walk::find_card(&project, path, base)
        .map_err(|why| text!("board.in.at", path, why))?;
    if !project.is_container(&container.kind) {
        return Err(text!(
            "board.in.notContainer",
            path = project.full(container),
            kind = container.kind
        ));
    }
    let mut out = HashSet::new();
    let mut queue: VecDeque<String> = items(&project, container).into();
    while let Some(id) = queue.pop_front() {
        if !out.insert(id.clone()) {
            continue;
        }
        if let Some(card) = project.card(&id) {
            queue.extend(items(&project, card));
        }
    }
    Ok(out)
}

/// The number a label begins with: a goal's place in the order it is planned in.
fn planned(card: &Card) -> Option<u64> {
    let digits: String = card
        .label
        .trim_start()
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    digits.parse().ok()
}

/// Goals in the order their labels number them, each in a place a goal held: the other
/// cards stay where they were.
pub(crate) fn in_planned_order(cards: &mut [&Card]) {
    let at: Vec<usize> = (0..cards.len())
        .filter(|&i| cards[i].kind == "goal")
        .collect();
    let mut goals: Vec<&Card> = at.iter().map(|&i| cards[i]).collect();
    goals.sort_by_key(|g| planned(g).unwrap_or(u64::MAX));
    for (i, goal) in at.into_iter().zip(goals) {
        cards[i] = goal;
    }
}
