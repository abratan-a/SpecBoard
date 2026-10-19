//! The boxes on a section's diagram (DiagramBoxes): its own cards and its card
//! ghosts (cards of its own that stand for another section's card, drawn whole there), the
//! other sections' cards put there by hand, and its section ghosts (the cards that sum up
//! another section) — each where it sits and how big it is — read in one go
//! (`section_read` with `include: ["places"]`) and put in one go (`section_update`'s
//! `places`). A box put where it overlaps another, or comes
//! within the margin of one, goes to the nearest clear cell, as a hand's drop does; the
//! boxes of one call are looked at once all of them are put.

use super::Seen;
use super::cards::Scope;
use super::grid::{
    CARD_COLUMNS, Layout, OwnSize, Rect, card_columns, card_rows, own_size, rows_for_rows,
    rows_sized, size_value,
};
use crate::store::Reader;
use crate::write::{Spot, Tx};
use serde_json::{Map, Value};

/// What a box is: a card (its own or a card ghost), or a section ghost — the card that
/// sums up another section.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Of {
    Card,
    SectionGhost,
}

impl Of {
    /// The kind of item its place is kept as (`places`' `item_kind`, as the stored form
    /// has it).
    fn kept_as(self) -> &'static str {
        match self {
            Of::Card => "card",
            Of::SectionGhost => "section",
        }
    }

    /// The key MCP names its path by in an answer.
    fn key(self) -> &'static str {
        match self {
            Of::Card => "card",
            Of::SectionGhost => "sectionGhost",
        }
    }
}

/// One box: what it is, its path, where it sits, its size, and the size written for it
/// (none: the card's own, or the project's).
struct Boxed {
    of: Of,
    id: String,
    path: String,
    /// The section's own card: its place is the card's own (`view` `""`).
    own: bool,
    at: (i64, i64),
    size: (i64, i64),
    written: Option<OwnSize>,
    /// Drawn as a card of its own: not a value object or an enum folded into the cards holding
    /// it (`grid::folded`), which blocks nothing.
    drawn: bool,
}

impl Boxed {
    fn rect(&self) -> Rect {
        (self.at.0, self.at.1, self.size.0, self.size.1)
    }
}

/// A cell as the boxes say it: `{x, y}`.
pub(crate) fn point(x: i64, y: i64) -> Value {
    let mut p = Map::new();
    p.insert("x".into(), x.into());
    p.insert("y".into(), y.into());
    Value::Object(p)
}

/// Every box placed on `section`'s diagram, as the tree reads them: its own cards, then
/// its card ghosts, then its section ghosts.
fn boxes(
    reader: &Reader,
    seen: &Seen,
    layout: &Layout,
    section: &str,
) -> Result<Vec<Boxed>, String> {
    let project = seen.project();
    let branch = (!seen.named.tree.is_empty()).then_some(seen.named.tree.as_str());
    let made = reader.compose(&seen.summary.id, branch)?;
    let folded = super::grid::folded(&project, section);
    let mut out = Vec::new();
    for &i in project.directory.members(section) {
        let card = project.nth(i);
        let (Some(x), Some(y)) = (card.pos_x, card.pos_y) else {
            continue;
        };
        out.push(Boxed {
            of: Of::Card,
            id: card.id.clone(),
            path: project.full(card),
            own: true,
            at: (x.round() as i64, y.round() as i64),
            size: (
                card_columns(card, project.defs.cards()),
                card_rows(card, &project, layout),
            ),
            written: own_size(card, project.defs.cards()),
            drawn: !folded.contains(&card.id),
        });
    }
    // Its card ghosts: cards of its own, each where it sits as a card does.
    for ghost in project.ghosts_in(section) {
        let (Some(x), Some(y)) = (ghost.pos_x, ghost.pos_y) else {
            continue;
        };
        out.push(Boxed {
            of: Of::Card,
            id: ghost.id.clone(),
            path: project.full_ghost(ghost),
            own: true,
            at: (x.round() as i64, y.round() as i64),
            size: super::grid::ghost_size(&project, ghost, layout),
            written: None,
            drawn: true,
        });
    }
    let mut outside: Vec<Boxed> = made
        .places
        .iter()
        .filter(|((view, _), _)| view == section)
        .filter_map(|((_, id), place)| {
            let card = project.card(id)?;
            Some(Boxed {
                of: Of::Card,
                id: id.clone(),
                path: project.full(card),
                own: false,
                at: (place.x?.round() as i64, place.y?.round() as i64),
                size: match place.size {
                    Some(own) => (own.0, rows_sized(card, &project, layout, Some(own))),
                    None => (
                        card_columns(card, project.defs.cards()),
                        card_rows(card, &project, layout),
                    ),
                },
                written: place.size,
                drawn: true,
            })
        })
        .collect();
    outside.sort_by_key(|b| {
        project
            .detail
            .schemas
            .iter()
            .position(|c| c.id == b.id)
            .unwrap_or(usize::MAX)
    });
    out.extend(outside);
    let mut section_ghosts: Vec<Boxed> = made
        .section_ghost_places
        .iter()
        .filter(|((view, _), _)| view == section)
        .filter_map(|((_, id), place)| {
            seen.section(id)?;
            Some(Boxed {
                of: Of::SectionGhost,
                id: id.clone(),
                path: project.directory.full_section(id),
                own: false,
                at: (place.x?.round() as i64, place.y?.round() as i64),
                size: match place.size {
                    Some((w, h)) => (w, h.unwrap_or_else(|| rows_for_rows(layout.rows))),
                    None => (CARD_COLUMNS, rows_for_rows(layout.rows)),
                },
                written: place.size,
                drawn: true,
            })
        })
        .collect();
    section_ghosts.sort_by(|a, b| a.path.cmp(&b.path));
    out.extend(section_ghosts);
    Ok(out)
}

/// The boxes on `section`'s diagram as `section_read` returns them, in the shape `places` writes them:
/// by the path a box is put by (a card's or card ghost's, a section ghost's), each with `topLeft` and
/// `bottomRight`, and `size` when one is written.
pub(crate) fn read_places(
    reader: &Reader,
    seen: &Seen,
    layout: &Layout,
    section: &str,
) -> Result<Value, String> {
    let mut out = Map::new();
    for b in boxes(reader, seen, layout, section)? {
        let mut row = Map::new();
        row.insert("topLeft".into(), point(b.at.0, b.at.1));
        row.insert(
            "bottomRight".into(),
            point(b.at.0 + b.size.0 - 1, b.at.1 + b.size.1 - 1),
        );
        if let Some(size) = b.written {
            row.insert("size".into(), size_value(size));
        }
        // Folded into the cards holding it: where it would go, shown on its own.
        if !b.drawn {
            row.insert("drawn".into(), false.into());
        }
        out.insert(b.path, Value::Object(row));
    }
    Ok(Value::Object(out))
}

/// A box a call puts: what, where it is drawn from, and what was said for it.
struct Wanted {
    of: Of,
    id: String,
    path: String,
    own: bool,
    at: Option<(i64, i64)>,
    size: Option<Value>,
}

/// A box's row in the places, as the tree has it.
fn spot(tx: &Tx, scope: &Scope, view: &str, of: Of, id: &str) -> Result<Spot, String> {
    let found = tx.conn().query_row(
        "SELECT x, y, w, h, pinned FROM places WHERE project_id = ? AND branch_id = ? AND view = ? AND item_kind = ? AND item = ?",
        rusqlite::params![scope.project, scope.tree, view, of.kept_as(), id],
        |r| {
            Ok(Spot {
                x: r.get(0)?,
                y: r.get(1)?,
                w: r.get(2)?,
                h: r.get(3)?,
                pinned: r.get::<_, i64>(4)? != 0,
            })
        },
    );
    match found {
        Ok(s) => Ok(s),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(Spot::default()),
        Err(e) => Err(crate::write::db(e)),
    }
}

/// `section_update`'s `places`: each box put where it says, then, with all of them put,
/// the ones that overlap another or come within the margin of one moved to the nearest
/// clear cell. Returns how many were put, and where the moved ones went.
pub(crate) fn put_places(
    tx: &mut Tx,
    scope: &Scope,
    section: &str,
    raw: &Value,
) -> Result<(usize, Vec<Value>), String> {
    let shape = || text!("boxes.places.shape");
    let given = raw.as_object().ok_or_else(shape)?;
    // What each path names, read before anything is put.
    let mut wanted: Vec<Wanted> = Vec::new();
    {
        let seen = Seen::in_tx(tx, scope)?;
        let project = seen.project();
        for (path, spec) in given {
            let spec = spec.as_object().ok_or_else(shape)?;
            let stray: Vec<&str> = spec
                .keys()
                .map(String::as_str)
                .filter(|k| !["topLeft", "size", "bottomRight", "drawn"].contains(k))
                .collect();
            if !stray.is_empty() {
                return Err(text!(
                    "boxes.places.strayKeys",
                    path,
                    keys = stray.join(", ")
                ));
            }
            // What `section_read` also says of a box (`bottomRight`, `drawn`) is let be, so a read map
            // can be written back with a box changed.
            if !spec.contains_key("topLeft") && !spec.contains_key("size") {
                return Err(text!("boxes.places.needsOne", path));
            }
            let at = spec
                .get("topLeft")
                .map(super::update::grid_point)
                .transpose()
                .map_err(|e| text!("boxes.places.pathError", path, e))?;
            let size = spec.get("size").cloned();
            // A card ghost of this section is a box of its own, as a card of it is.
            if crate::pathtext::has_section(path)
                && let Some(crate::export::Named::Ghost(ghost)) = project.find_named(path, section)
                && project.section_of_ghost(ghost) == section
            {
                if size.is_some() {
                    return Err(text!("boxes.places.ghostSize", path));
                }
                wanted.push(Wanted {
                    of: Of::Card,
                    id: ghost.id.clone(),
                    path: project.full_ghost(ghost),
                    own: true,
                    at,
                    size: None,
                });
            } else if crate::pathtext::has_section(path) {
                let card = super::walk::find_card(&project, path, Some(section))
                    .map_err(|e| text!("boxes.places.pathError", path, e))?;
                if size.is_some() && !project.defs.cards().sizable(&card.kind) {
                    return Err(text!(
                        "boxes.places.pathError",
                        path,
                        e = super::update::not_sizable(&card.kind)
                    ));
                }
                let own = project.section_of(card) == section;
                wanted.push(Wanted {
                    of: Of::Card,
                    id: card.id.clone(),
                    path: project.full(card),
                    own,
                    at,
                    size,
                });
            } else {
                let id = super::section_id(&project, path)
                    .map_err(|e| text!("boxes.places.pathError", path, e))?;
                let target = seen
                    .section(&id)
                    .ok_or_else(|| text!("sections.error.missing"))?;
                if id == section || target.parent_id.is_none() {
                    return Err(text!("boxes.places.sectionGhost", path));
                }
                wanted.push(Wanted {
                    of: Of::SectionGhost,
                    path: project.directory.full_section(&id),
                    id,
                    own: false,
                    at,
                    size,
                });
            }
        }
    }
    // Each put where it says.
    for Wanted {
        of,
        id,
        path,
        own,
        at,
        size,
    } in &wanted
    {
        let view = if *own { "" } else { section };
        let mut s = spot(tx, scope, view, *of, id)?;
        if let Some((x, y)) = at {
            s.x = Some(*x);
            s.y = Some(*y);
            s.pinned = true;
        }
        if let Some(raw) = size {
            let size = if raw.is_null() {
                None
            } else {
                Some(
                    super::update::grid_size(raw)
                        .map_err(|e| text!("boxes.places.pathError", path, e))?,
                )
            };
            s.w = size.map(|s| s.0);
            s.h = size.and_then(|s| s.1);
        }
        if s.x.is_none() || s.y.is_none() {
            return Err(text!("boxes.places.notPlaced", path));
        }
        tx.place(
            &scope.project,
            &scope.tree,
            view,
            of.kept_as(),
            id,
            Some(&s),
        )?;
    }
    // A section that keeps its places as written (`grid::keeps_places`) has nothing jump.
    if super::grid::keeps_places(&Seen::in_tx(tx, scope)?.project(), section) {
        return Ok((wanted.len(), Vec::new()));
    }
    // Then, all of them put, what still overlaps goes to the nearest clear cell.
    let (layout, mut all) = {
        let seen = Seen::in_tx(tx, scope)?;
        let layout = Layout::read(tx.conn(), &scope.project)?;
        let all = boxes(&tx.reader(), &seen, &layout, section)?;
        (layout, all)
    };
    let mut jumped = Vec::new();
    for Wanted {
        of, id, path, own, ..
    } in &wanted
    {
        let Some(at) = all.iter().position(|b| b.of == *of && &b.id == id) else {
            continue;
        };
        // A value object or an enum folded into the cards holding it is put where it says, and
        // blocks nothing.
        if !all[at].drawn {
            continue;
        }
        let m = layout.margin;
        let (x, y, w, h) = all[at].rect();
        let room = (x - m, y - m, w + 2 * m, h + 2 * m);
        let others: Vec<Rect> = all
            .iter()
            .enumerate()
            .filter(|(i, b)| *i != at && b.drawn)
            .map(|(_, b)| b.rect())
            .collect();
        if !others.iter().any(|o| super::grid::intersects(*o, room)) {
            continue;
        }
        let landed = super::grid::clear_spot(m, (x, y), w, h, &others);
        all[at].at = landed;
        let view = if *own { "" } else { section };
        let mut s = spot(tx, scope, view, *of, id)?;
        s.x = Some(landed.0);
        s.y = Some(landed.1);
        tx.place(
            &scope.project,
            &scope.tree,
            view,
            of.kept_as(),
            id,
            Some(&s),
        )?;
        let mut row = Map::new();
        row.insert(of.key().into(), path.clone().into());
        row.insert("topLeft".into(), point(landed.0, landed.1));
        jumped.push(Value::Object(row));
    }
    Ok((wanted.len(), jumped))
}

/// `section_update`'s `tidy`: the cards of `section` that lie over another
/// moved to the nearest clear cell (`grid::untangled`), nothing else touched. Where each went,
/// as `jumped` says it.
pub(crate) fn tidy(tx: &mut Tx, scope: &Scope, section: &str) -> Result<Vec<Value>, String> {
    let moves = {
        let seen = Seen::in_tx(tx, scope)?;
        let layout = Layout::read(tx.conn(), &scope.project)?;
        super::grid::untangled(&seen.project(), &layout, section)
    };
    let mut out = Vec::new();
    for (b, (x, y)) in moves {
        let mut s = spot(tx, scope, "", Of::Card, &b.id)?;
        s.x = Some(x);
        s.y = Some(y);
        s.pinned = true;
        tx.place(
            &scope.project,
            &scope.tree,
            "",
            Of::Card.kept_as(),
            &b.id,
            Some(&s),
        )?;
        let mut row = Map::new();
        row.insert(Of::Card.key().into(), b.path.into());
        row.insert("topLeft".into(), point(x, y));
        out.push(Value::Object(row));
    }
    Ok(out)
}

// MARK: - A region, and boxes moved and arranged together

/// A region of a section's diagram, in cells (`card_list`'s and `section_update`'s `region`):
/// from one corner to the other, both in it; a box is in it when any of it is
/// (`intersect`, the default) or the whole of it is (`contain`).
#[derive(Clone, Copy, Debug)]
pub(crate) struct Region {
    left: i64,
    top: i64,
    right: i64,
    bottom: i64,
    contain: bool,
}

fn region_shape() -> String {
    text!("boxes.region.shape")
}

/// `region`, read and checked.
pub(crate) fn region(raw: &Value) -> Result<Region, String> {
    let o = raw.as_object().ok_or_else(region_shape)?;
    let stray: Vec<&str> = o
        .keys()
        .map(String::as_str)
        .filter(|k| !["from", "to", "mode"].contains(k))
        .collect();
    if !stray.is_empty() {
        return Err(text!(
            "boxes.region.strayKeys",
            keys = stray.join(", "),
            shape = region_shape()
        ));
    }
    let corner = |key: &str| {
        o.get(key)
            .ok_or_else(|| text!("boxes.region.keyMissing", key, shape = region_shape()))
            .and_then(|v| {
                cell(v).ok_or_else(|| text!("boxes.region.keyShape", key, shape = region_shape()))
            })
    };
    let (from, to) = (corner("from")?, corner("to")?);
    let contain = match o.get("mode").map(|m| m.as_str()) {
        None | Some(Some("intersect")) => false,
        Some(Some("contain")) => true,
        _ => return Err(text!("boxes.region.modeShape", shape = region_shape())),
    };
    Ok(Region {
        left: from.0.min(to.0),
        top: from.1.min(to.1),
        right: from.0.max(to.0),
        bottom: from.1.max(to.1),
        contain,
    })
}

/// A cell as a call writes it, `{x, y}` in whole numbers, within the diagram's reach.
fn cell(value: &Value) -> Option<(i64, i64)> {
    super::update::grid_point(value).ok()
}

impl Region {
    /// Whether a box at `rect` is in the region.
    pub(crate) fn takes(&self, (x, y, w, h): Rect) -> bool {
        let (right, bottom) = (x + w - 1, y + h - 1);
        if self.contain {
            x >= self.left && right <= self.right && y >= self.top && bottom <= self.bottom
        } else {
            x <= self.right && right >= self.left && y <= self.bottom && bottom >= self.top
        }
    }
}

/// The boxes a call names on `section`'s diagram, as indexes into `all`: by path, in the order
/// given (a card, a card ghost, or a section ghost's section), or those a region takes, from the
/// top down and left to right. A value object or an enum folded into the cards holding it is not
/// a box a region takes.
fn picked(
    seen: &Seen,
    all: &[Boxed],
    section: &str,
    spec: &Map<String, Value>,
    what: &str,
) -> Result<Vec<usize>, String> {
    match (spec.get("cards"), spec.get("region")) {
        (Some(cards), None) => {
            let paths = cards
                .as_array()
                .filter(|a| !a.is_empty())
                .ok_or_else(|| text!("boxes.picked.cardsShape", what))?;
            let project = seen.project();
            let mut out: Vec<usize> = Vec::new();
            for raw in paths {
                let path = raw
                    .as_str()
                    .ok_or_else(|| text!("boxes.picked.cardsStrings", what, raw))?;
                let (of, id) = if crate::pathtext::has_section(path) {
                    match super::walk::find_named(&project, path, Some(section))
                        .map_err(|e| text!("boxes.picked.pathError", what, path, e))?
                    {
                        crate::export::Named::Card(card) => (Of::Card, card.id.clone()),
                        crate::export::Named::Ghost(ghost) => (Of::Card, ghost.id.clone()),
                    }
                } else {
                    let id = super::section_id(&project, path)
                        .map_err(|e| text!("boxes.picked.pathError", what, path, e))?;
                    (Of::SectionGhost, id)
                };
                let at = all
                    .iter()
                    .position(|b| b.of == of && b.id == id)
                    .ok_or_else(|| text!("boxes.picked.notPlaced", what, path))?;
                if !out.contains(&at) {
                    out.push(at);
                }
            }
            Ok(out)
        }
        (None, Some(raw)) => {
            let region = region(raw)?;
            let mut out: Vec<usize> = all
                .iter()
                .enumerate()
                .filter(|(_, b)| b.drawn && region.takes(b.rect()))
                .map(|(i, _)| i)
                .collect();
            out.sort_by_key(|&i| (all[i].at.1, all[i].at.0));
            if out.is_empty() {
                return Err(text!("boxes.picked.emptyRegion", what));
            }
            Ok(out)
        }
        _ => Err(text!("boxes.picked.oneOf", what)),
    }
}

/// How far a group of boxes moves: `by`, when none of them comes within the margin of a box
/// left where it is; else the nearest offset where all of them are clear, the group kept as it
/// is (looked for out to 40 cells; failing that, `by`).
fn clear_offset(moving: &[Rect], others: &[Rect], margin: i64, by: (i64, i64)) -> (i64, i64) {
    let fits = |(dx, dy): (i64, i64)| {
        moving.iter().all(|&(x, y, w, h)| {
            let room = (
                x + dx - margin,
                y + dy - margin,
                w + 2 * margin,
                h + 2 * margin,
            );
            !others.iter().any(|o| super::grid::intersects(*o, room))
        })
    };
    if fits(by) {
        return by;
    }
    for ring in 1..=40i64 {
        let mut best = None;
        let mut best_distance = i64::MAX;
        for dx in -ring..=ring {
            for dy in -ring..=ring {
                if dx.abs().max(dy.abs()) != ring {
                    continue;
                }
                let offset = (by.0 + dx, by.1 + dy);
                let distance = dx * dx + dy * dy;
                if distance < best_distance && fits(offset) {
                    best = Some(offset);
                    best_distance = distance;
                }
            }
        }
        if let Some(offset) = best {
            return offset;
        }
    }
    by
}

/// A box put at `at`, by hand as it were.
fn put_box(
    tx: &mut Tx,
    scope: &Scope,
    section: &str,
    b: &Boxed,
    at: (i64, i64),
) -> Result<(), String> {
    let view = if b.own { "" } else { section };
    let mut s = spot(tx, scope, view, b.of, &b.id)?;
    s.x = Some(at.0);
    s.y = Some(at.1);
    s.pinned = true;
    tx.place(
        &scope.project,
        &scope.tree,
        view,
        b.of.kept_as(),
        &b.id,
        Some(&s),
    )
}

/// A box as an answer says where it went.
fn went_row(b: &Boxed, at: (i64, i64)) -> Value {
    let mut row = Map::new();
    row.insert(b.of.key().into(), b.path.clone().into());
    row.insert("topLeft".into(), point(at.0, at.1));
    Value::Object(row)
}

/// The keys a value of `what` takes; refused when it has others.
fn only_keys(spec: &Map<String, Value>, what: &str, keys: &[&str]) -> Result<(), String> {
    let stray: Vec<&str> = spec
        .keys()
        .map(String::as_str)
        .filter(|k| !keys.contains(k))
        .collect();
    if stray.is_empty() {
        return Ok(());
    }
    Err(text!(
        "boxes.only.strayKeys",
        what,
        stray = stray.join(", "),
        keys = keys.join(" / ")
    ))
}

/// The diagram's boxes, the project's layout, and the boxes a call names.
fn read_picked(
    tx: &Tx,
    scope: &Scope,
    section: &str,
    spec: &Map<String, Value>,
    what: &str,
) -> Result<(Layout, Vec<Boxed>, Vec<usize>), String> {
    let seen = Seen::in_tx(tx, scope)?;
    let layout = Layout::read(tx.conn(), &scope.project)?;
    let all = boxes(&tx.reader(), &seen, &layout, section)?;
    let picked = picked(&seen, &all, section, spec, what)?;
    Ok((layout, all, picked))
}

/// The boxes that stay where they are, and block: every drawn one not among `picked`.
fn staying(all: &[Boxed], picked: &[usize]) -> Vec<Rect> {
    all.iter()
        .enumerate()
        .filter(|(i, b)| !picked.contains(i) && b.drawn)
        .map(|(_, b)| b.rect())
        .collect()
}

/// `section_update`'s `move`: the boxes named — `cards` or a `region` — moved
/// together, `by` so many cells or `to` where the top left of them goes, kept as they are among
/// themselves; where they would come within the margin of a box left where it is, the group goes
/// to the nearest place clear of them all (`shifted`, how much further than asked). Where each
/// went, as `moved` says it.
pub(crate) fn move_boxes(
    tx: &mut Tx,
    scope: &Scope,
    section: &str,
    raw: &Value,
) -> Result<Map<String, Value>, String> {
    let spec = raw.as_object().ok_or_else(|| text!("boxes.move.shape"))?;
    only_keys(spec, "move", &["cards", "region", "by", "to"])?;
    let (layout, all, picked) = read_picked(tx, scope, section, spec, "move")?;
    let by = match (spec.get("by"), spec.get("to")) {
        (Some(by), None) => cell(by).ok_or_else(|| text!("boxes.move.byShape"))?,
        (None, Some(to)) => {
            let (x, y) = cell(to).ok_or_else(|| text!("boxes.move.toShape"))?;
            let left = picked.iter().map(|&i| all[i].at.0).min().unwrap_or(x);
            let top = picked.iter().map(|&i| all[i].at.1).min().unwrap_or(y);
            (x - left, y - top)
        }
        _ => {
            return Err(text!("boxes.move.byOrTo"));
        }
    };
    let moving: Vec<Rect> = picked
        .iter()
        .filter(|&&i| all[i].drawn)
        .map(|&i| all[i].rect())
        .collect();
    let went = if super::grid::keeps_places(&Seen::in_tx(tx, scope)?.project(), section) {
        by
    } else {
        clear_offset(&moving, &staying(&all, &picked), layout.margin, by)
    };
    let mut moved = Vec::new();
    for &i in &picked {
        let at = (all[i].at.0 + went.0, all[i].at.1 + went.1);
        put_box(tx, scope, section, &all[i], at)?;
        moved.push(went_row(&all[i], at));
    }
    let mut out = Map::new();
    out.insert("moved".into(), Value::Array(moved));
    if went != by {
        out.insert("shifted".into(), point(went.0 - by.0, went.1 - by.1));
    }
    Ok(out)
}

/// `section_update`'s `arrange`: the boxes named — `cards` in their order, or
/// those a `region` takes, from the top down — put in `columns` columns (4 when not said) from `at`
/// (the top left of them when not said), left to right and each under the one above it in its
/// column, the project's margin between them, each as tall as the screen draws it; the group kept clear
/// of the boxes left where they are as `move` keeps it. Where each went, as `arranged` says it.
pub(crate) fn arrange_boxes(
    tx: &mut Tx,
    scope: &Scope,
    section: &str,
    raw: &Value,
) -> Result<Map<String, Value>, String> {
    let spec = raw
        .as_object()
        .ok_or_else(|| text!("boxes.arrange.shape"))?;
    only_keys(spec, "arrange", &["cards", "region", "columns", "at"])?;
    let (layout, all, picked) = read_picked(tx, scope, section, spec, "arrange")?;
    // A value object or an enum folded into the cards holding it takes no room: left as it is.
    let picked: Vec<usize> = picked.into_iter().filter(|&i| all[i].drawn).collect();
    let columns = match spec.get("columns") {
        None => 4,
        Some(v) => match v.as_f64() {
            Some(n) if n == n.trunc() && (1.0..=12.0).contains(&n) => n as i64,
            _ => {
                return Err(text!("boxes.arrange.columns", v));
            }
        },
    };
    let at = match spec.get("at") {
        Some(v) => cell(v).ok_or_else(|| text!("boxes.arrange.atShape"))?,
        None => (
            picked.iter().map(|&i| all[i].at.0).min().unwrap_or(0),
            picked.iter().map(|&i| all[i].at.1).min().unwrap_or(0),
        ),
    };
    let margin = layout.margin;
    let step = picked
        .iter()
        .map(|&i| all[i].size.0)
        .max()
        .unwrap_or(CARD_COLUMNS)
        + margin;
    let mut bottoms = vec![at.1; columns as usize];
    let mut places = Vec::new();
    for (n, &i) in picked.iter().enumerate() {
        let column = n % columns as usize;
        let place = (at.0 + column as i64 * step, bottoms[column]);
        bottoms[column] += all[i].size.1 + margin;
        places.push((i, place));
    }
    let moving: Vec<Rect> = places
        .iter()
        .map(|&(i, (x, y))| (x, y, all[i].size.0, all[i].size.1))
        .collect();
    let went = clear_offset(&moving, &staying(&all, &picked), margin, (0, 0));
    let mut arranged = Vec::new();
    for (i, (x, y)) in places {
        let place = (x + went.0, y + went.1);
        put_box(tx, scope, section, &all[i], place)?;
        arranged.push(went_row(&all[i], place));
    }
    let mut out = Map::new();
    out.insert("arranged".into(), Value::Array(arranged));
    if went != (0, 0) {
        out.insert("shifted".into(), point(went.0, went.1));
    }
    Ok(out)
}
