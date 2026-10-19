//! A goal's card map: the cards
//! inside the goal drawn as cards of their own on the goal's own diagram, each
//! where it sits there — kept apart from the goal as a flow's nodes are (`places`: the
//! goal's id as the view, the card as the item) and read with the goal (`project_detail`'s
//! `itemPlaces`). MCP reads its boxes in one go (`card_read` with `include: ["places"]`)
//! and puts them in one go (`card_update`'s `places`), in the shape of a section's
//! diagram's (`boxes`). A card the goal holds with no place there is put there in the
//! order the goal holds them (`unplaced`, with `update::place_unplaced`). The cards a goal
//! holds are cards inside it (`in`; `containers`).

use super::Seen;
use super::board::items;
use super::boxes::point;
use super::cards::Scope;
use super::grid::{CARD_COLUMNS, Layout, Rect, clear_spot, intersects, next_spot, rows_for_count};
use crate::export::Project;
use crate::model::Card;
use crate::write::{Spot, Tx, db};
use rusqlite::OptionalExtension;
use serde_json::{Map, Value};

/// How many rows a card draws on a goal's card map: a goal inside, as the relation panel
/// draws a goal — its choices (status・priority) and the rest of its fields that are written, as a
/// section's diagram counts them from its kind (`grid::goal_row_count`), and a row a card it holds.
/// (A task is drawn by its kind's face: `size`.)
fn map_rows(card: &Card, project: &Project) -> usize {
    let written = |names: &[&str]| {
        names
            .iter()
            .filter(|name| {
                card.form
                    .as_ref()
                    .and_then(|f| f["fields"].get(**name))
                    .is_some_and(|v| !v.is_null() && v.as_str() != Some(""))
            })
            .count()
    };
    match card.kind.as_str() {
        "goal" => {
            let fields = project
                .defs
                .cards()
                .get("goal")
                .map_or(2, |kind| super::grid::goal_row_count(card, kind));
            fields + items(project, card).len()
        }
        // A kind the project defines, as the screen draws it from its definition: a choice and a
        // check a row each always, its other fields a row each when written (not those in its
        // header, nor those for the card editor alone), and a row each of its rows.
        kind if project.is_own_kind(kind) => {
            let Some(def) = project.defs.cards().get(kind) else {
                return 1;
            };
            let shown = def.fields.iter().filter(|f| {
                !["label", "description", "tags", "metadata"].contains(&f.name.as_str())
                    && !def.header.contains(&f.name)
                    && f.show.as_deref() != Some("editor")
            });
            let mut rows = 0;
            for field in shown {
                use crate::kinds::FieldType;
                if matches!(field.fieldtype, FieldType::Radio | FieldType::Checkbox) {
                    rows += 1;
                } else {
                    rows += written(&[field.name.as_str()]);
                }
            }
            fn count(rows: &Value) -> usize {
                rows.as_object()
                    .into_iter()
                    .flat_map(|r| r.values())
                    .filter_map(Value::as_array)
                    .flatten()
                    .map(|item| 1 + count(&item["rows"]))
                    .sum()
            }
            rows += card.form.as_ref().map_or(0, |f| count(&f["rows"]));
            rows.max(1)
        }
        _ => 1,
    }
}

/// A card's box on a card map: columns and grid rows. Its own width and height there when it
/// has them; otherwise the project's width, and as tall as its face draws it at its width
/// (`grid::face_rows`) or as its rows on the map.
pub(crate) fn size(card: &Card, project: &Project, layout: &Layout) -> (i64, i64) {
    let kinds = project.defs.cards();
    let own = super::grid::own_size(card, kinds);
    let w = own.map_or(CARD_COLUMNS, |s| s.0);
    if let Some((w, Some(h))) = own {
        return (w, h);
    }
    if let Some(face) = kinds.get(&card.kind).and_then(|k| k.face.as_ref()) {
        let width = super::grid::width_points(layout, own.map(|s| s.0));
        return (w, super::grid::face_rows(card, face, layout, width));
    }
    (w, rows_for_count(map_rows(card, project), layout))
}

/// One box on a goal's card map: the card's id and path, where it sits, its size, and the
/// size it has of its own.
struct Boxed {
    id: String,
    path: String,
    at: (i64, i64),
    size: (i64, i64),
    written: Option<super::grid::OwnSize>,
}

impl Boxed {
    fn rect(&self) -> Rect {
        (self.at.0, self.at.1, self.size.0, self.size.1)
    }
}

/// The cards the goal holds that are placed on its card map, in the order it holds them.
fn boxes(project: &Project, goal: &Card, layout: &Layout) -> Vec<Boxed> {
    items(project, goal)
        .iter()
        .filter_map(|id| {
            let card = project.card(id)?;
            let at = goal.item_places.get(id)?;
            Some(Boxed {
                id: id.clone(),
                path: project.named(id).unwrap_or_else(|| project.full(card)),
                at: (at.x, at.y),
                size: size(card, project, layout),
                written: super::grid::own_size(card, project.defs.cards()),
            })
        })
        .collect()
}

/// The boxes placed on `goal`'s card map: each card's id, path and box.
pub(crate) fn rects(
    project: &Project,
    goal: &Card,
    layout: &Layout,
) -> Vec<(String, String, Rect)> {
    boxes(project, goal, layout)
        .into_iter()
        .map(|b| {
            let rect = b.rect();
            (b.id, b.path, rect)
        })
        .collect()
}

/// The boxes on `goal`'s card map as `card_read` returns them (`include: ["places"]`), in the shape
/// `places` writes them: by the card's path, each with `topLeft` and `bottomRight` — as `section_read`
/// returns a section's — and `size` when it has one of its own.
pub(crate) fn read_places(seen: &Seen, layout: &Layout, goal: &Card) -> Value {
    let project = seen.project();
    let mut out = Map::new();
    for b in boxes(&project, goal, layout) {
        let mut row = Map::new();
        row.insert("topLeft".into(), point(b.at.0, b.at.1));
        row.insert(
            "bottomRight".into(),
            point(b.at.0 + b.size.0 - 1, b.at.1 + b.size.1 - 1),
        );
        if let Some(size) = b.written {
            row.insert("size".into(), super::grid::size_value(size));
        }
        out.insert(b.path, Value::Object(row));
    }
    Value::Object(out)
}

/// A card's row on a goal's card map, as the tree has it.
pub(crate) fn spot(tx: &Tx, scope: &Scope, goal: &str, id: &str) -> Result<Spot, String> {
    let found = tx
        .conn()
        .query_row(
            "SELECT x, y, w, h, pinned FROM places WHERE project_id = ? AND branch_id = ? AND view = ? AND item_kind = 'card' AND item = ?",
            rusqlite::params![scope.project, scope.tree, goal, id],
            |r| {
                Ok(Spot {
                    x: r.get(0)?,
                    y: r.get(1)?,
                    w: r.get(2)?,
                    h: r.get(3)?,
                    pinned: r.get::<_, i64>(4)? != 0,
                })
            },
        )
        .optional()
        .map_err(db)?;
    Ok(found.unwrap_or_default())
}

/// `card_update`'s `places` on a goal: each card it holds put where it says, then, with all
/// of them put, the ones that overlap another or come within the margin of one moved to the
/// nearest clear cell (`boxes::put_places`). A path that is not a card the goal holds is
/// refused. Returns how many were put, and where the moved ones went.
pub(crate) fn put_places(
    tx: &mut Tx,
    scope: &Scope,
    goal: &str,
    raw: &Value,
) -> Result<(usize, Vec<Value>), String> {
    let shape = || text!("cardMap.places.shape", top_left = "{topLeft}");
    let given = raw.as_object().ok_or_else(shape)?;
    // What each path names, read before anything is put.
    let mut wanted: Vec<(String, String, (i64, i64))> = Vec::new();
    {
        let seen = Seen::in_tx(tx, scope)?;
        let project = seen.project();
        let owner = project
            .card(goal)
            .ok_or_else(|| text!("cardMap.card.missing"))?;
        let held = items(&project, owner);
        let base = project.section_of(owner);
        for (path, spec) in given {
            let spec = spec.as_object().ok_or_else(shape)?;
            let stray: Vec<&str> = spec
                .keys()
                .map(String::as_str)
                .filter(|k| !["topLeft", "size", "bottomRight"].contains(k))
                .collect();
            if !stray.is_empty() {
                return Err(text!("cardMap.places.stray", path, keys = stray.join(", ")));
            }
            // A card inside the goal, by its path, or by its name alone there.
            let card = match project.inner_named(&owner.id, path.trim()) {
                Some(card) => card,
                None => super::walk::find_card(&project, path, Some(&base))
                    .map_err(|e| text!("cardMap.places.at", path, why = e))?,
            };
            let id = card.id.clone();
            if !held.contains(&id) {
                return Err(text!(
                    "cardMap.places.outside",
                    path,
                    goal = project.full(owner)
                ));
            }
            if spec.contains_key("size") {
                return Err(if project.defs.cards().sizable(&card.kind) {
                    text!("cardMap.places.sizeNotHere", path, kind = card.kind)
                } else {
                    text!(
                        "cardMap.places.at",
                        path,
                        why = super::update::not_sizable(&card.kind)
                    )
                });
            }
            let at = spec
                .get("topLeft")
                .ok_or_else(|| text!("cardMap.places.needTopLeft", path))?;
            let at = super::update::grid_point(at)
                .map_err(|e| text!("cardMap.places.at", path, why = e))?;
            wanted.push((id, project.full(card), at));
        }
    }
    // Each put where it says.
    for (id, _, (x, y)) in &wanted {
        let mut s = spot(tx, scope, goal, id)?;
        s.x = Some(*x);
        s.y = Some(*y);
        s.pinned = true;
        tx.place(&scope.project, &scope.tree, goal, "card", id, Some(&s))?;
    }
    // Then, all of them put, what still overlaps goes to the nearest clear cell.
    let (margin, mut all) = {
        let seen = Seen::in_tx(tx, scope)?;
        let layout = Layout::read(tx.conn(), &scope.project)?;
        let project = seen.project();
        let owner = project
            .card(goal)
            .ok_or_else(|| text!("cardMap.card.missing"))?;
        (layout.margin, boxes(&project, owner, &layout))
    };
    let mut jumped = Vec::new();
    for (id, path, _) in &wanted {
        let Some(at) = all.iter().position(|b| &b.id == id) else {
            continue;
        };
        let (x, y, w, h) = all[at].rect();
        let room = (x - margin, y - margin, w + 2 * margin, h + 2 * margin);
        let others: Vec<Rect> = all
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != at)
            .map(|(_, b)| b.rect())
            .collect();
        if !others.iter().any(|o| intersects(*o, room)) {
            continue;
        }
        let landed = clear_spot(margin, (x, y), w, h, &others);
        all[at].at = landed;
        let mut s = spot(tx, scope, goal, id)?;
        s.x = Some(landed.0);
        s.y = Some(landed.1);
        tx.place(&scope.project, &scope.tree, goal, "card", id, Some(&s))?;
        let mut row = Map::new();
        row.insert("card".into(), path.clone().into());
        row.insert("topLeft".into(), point(landed.0, landed.1));
        jumped.push(Value::Object(row));
    }
    Ok((wanted.len(), jumped))
}

/// Whether every card a goal holds has a place on its card map.
pub(crate) fn placed(seen: &Seen) -> bool {
    let project = seen.project();
    seen.detail
        .schemas
        .iter()
        .filter(|c| project.card_mapped(&c.kind))
        .all(|goal| {
            items(&project, goal)
                .iter()
                .all(|id| goal.item_places.contains_key(id))
        })
}

/// The cards goals hold with no place on their card maps, each given one in the order the
/// goal holds them — under the shortest of four columns, as a section's cards are given
/// theirs (`update::place_unplaced`): (the goal, the card, its cell).
pub(crate) fn unplaced(seen: &Seen, layout: &Layout) -> Vec<(String, String, (i64, i64))> {
    let project = seen.project();
    let mut out = Vec::new();
    for goal in seen
        .detail
        .schemas
        .iter()
        .filter(|c| project.card_mapped(&c.kind))
    {
        // Each by what the goal holds it as (a card ghost's id, when it holds one), drawn as
        // the card it leads to.
        let held: Vec<(&String, &Card)> = project
            .inner_of(&goal.id)
            .into_iter()
            .map(|c| (&c.id, c))
            .collect();
        if held
            .iter()
            .all(|(id, _)| goal.item_places.contains_key(*id))
        {
            continue;
        }
        let mut taken: Vec<Rect> = held
            .iter()
            .filter_map(|(id, c)| {
                let at = goal.item_places.get(*id)?;
                let (w, h) = size(c, &project, layout);
                Some((at.x, at.y, w, h))
            })
            .collect();
        for (id, card) in held
            .iter()
            .filter(|(id, _)| !goal.item_places.contains_key(*id))
        {
            let (w, h) = size(card, &project, layout);
            let at = next_spot(&taken, w, layout.margin);
            taken.push((at.0, at.1, w, h));
            out.push((goal.id.clone(), (*id).clone(), at));
        }
    }
    out
}

/// What `unplaced` gave a place, put there: not pinned, and not a change to the project.
pub(crate) fn put_unplaced(
    tx: &mut Tx,
    scope: &Scope,
    places: &[(String, String, (i64, i64))],
) -> Result<(), String> {
    for (goal, id, (x, y)) in places {
        let mut at = spot(tx, scope, goal, id)?;
        at.x = Some(*x);
        at.y = Some(*y);
        tx.place(&scope.project, &scope.tree, goal, "card", id, Some(&at))?;
    }
    Ok(())
}
