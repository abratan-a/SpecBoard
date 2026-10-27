//! Tidying a section's map (`section_update`'s `tidy`, the screen's 「整える」;
//! `specboard://SpecBoard/Docs/Design:Overview` §12「整える」): every box kept where it is, and moved
//! down only — never sideways or up — where it overlaps a box above it, comes within the margin of
//! one, or sits in the room a card above it takes when its rows are opened. The boxes are looked at
//! from the top down, each against those already looked at, so a box that is clear stays, and a
//! second tidy moves nothing. A group is one box with what it holds: what is inside it is tidied
//! first, its frame grown down to keep it, then the frame moved with all of it. The check's
//! `overlaps` says what this would move, and where to.

use super::Seen;
use super::boxes::point;
use super::cards::Scope;
use super::grid::{Layout, Rect, intersects, rows_for_rows};
use super::groups::{Group, OnMap};
use crate::store::Reader;
use crate::write::Tx;
use serde_json::{Map, Value};

/// A group's frame as tidying leaves it: its index among the map's, its top-left and its size.
pub(crate) type Framed = (usize, (i64, i64), (i64, i64));

/// What tidying a map moves: the boxes, by their index among the map's, each to where it goes; and
/// the groups whose frame moves or grows.
pub(crate) struct Plan {
    pub boxes: Vec<(usize, (i64, i64))>,
    pub groups: Vec<Framed>,
}

/// One thing tidied at a level: a box drawn as a card, or a group's frame with what it holds.
#[derive(Clone, Copy)]
enum Unit {
    Box(usize),
    Group(usize),
}

/// How the boxes and groups of a map sit while they are tidied.
struct Sheet<'a> {
    boxes: &'a [OnMap],
    groups: &'a [Group],
    margin: i64,
    /// How tall a card that grows is kept room for.
    room: i64,
    at: Vec<(i64, i64)>,
    frames: Vec<((i64, i64), (i64, i64))>,
    /// The group each box is in (the smallest whose frame holds its top-left), and the group each
    /// group is in (the smallest holding it as a group inside), as they were before tidying.
    box_in: Vec<Option<usize>>,
    group_in: Vec<Option<usize>>,
}

impl Sheet<'_> {
    /// The height a box takes: a card that grows the project's whole card, at the least.
    fn taken(&self, k: usize) -> i64 {
        let h = self.boxes[k].size.1;
        if self.boxes[k].grows {
            h.max(self.room)
        } else {
            h
        }
    }

    /// Whether `inner` is `outer` or inside it, at any depth.
    fn within(&self, mut inner: Option<usize>, outer: usize) -> bool {
        while let Some(g) = inner {
            if g == outer {
                return true;
            }
            inner = self.group_in[g];
        }
        false
    }

    /// The box a unit takes: where it sits, its width, and the height it takes — a frame's down to
    /// the deepest of its own bottom and what it holds.
    fn span(&self, unit: Unit) -> Rect {
        match unit {
            Unit::Box(k) => {
                let (x, y) = self.at[k];
                (x, y, self.boxes[k].size.0, self.taken(k))
            }
            Unit::Group(g) => {
                let ((x, y), (w, h)) = self.frames[g];
                let mut bottom = y + h;
                for k in 0..self.boxes.len() {
                    if self.boxes[k].drawn && self.within(self.box_in[k], g) {
                        bottom = bottom.max(self.at[k].1 + self.taken(k));
                    }
                }
                for inner in 0..self.groups.len() {
                    if inner != g && self.within(Some(inner), g) {
                        let ((_, iy), (_, ih)) = self.frames[inner];
                        bottom = bottom.max(iy + ih);
                    }
                }
                (x, y, w, bottom - y)
            }
        }
    }

    /// A unit moved down by `dy`, a group with all it holds.
    fn shift(&mut self, unit: Unit, dy: i64) {
        match unit {
            Unit::Box(k) => self.at[k].1 += dy,
            Unit::Group(g) => {
                for k in 0..self.boxes.len() {
                    if self.within(self.box_in[k], g) {
                        self.at[k].1 += dy;
                    }
                }
                for inner in 0..self.groups.len() {
                    if self.within(Some(inner), g) {
                        self.frames[inner].0.1 += dy;
                    }
                }
            }
        }
    }

    /// The units of one level — the map's own (`None`) or a group's — tidied from the top down.
    fn sweep(&mut self, level: Option<usize>) {
        let mut units: Vec<(Unit, usize)> = Vec::new();
        for k in 0..self.boxes.len() {
            if self.boxes[k].drawn && self.box_in[k] == level {
                units.push((Unit::Box(k), k));
            }
        }
        for g in 0..self.groups.len() {
            if self.group_in[g] == level {
                units.push((Unit::Group(g), self.boxes.len() + g));
            }
        }
        units.sort_by_key(|&(unit, order)| {
            let (x, y, _, _) = self.span(unit);
            (y, x, order)
        });
        let mut placed: Vec<(Rect, bool)> = Vec::new();
        for (unit, _) in units {
            let frame = matches!(unit, Unit::Group(_));
            let (x, y0, w, h) = self.span(unit);
            let mut y = y0;
            loop {
                let mut moved = false;
                for &((px, py, pw, ph), other_frame) in &placed {
                    // Two frames may touch: the margin is kept between a frame and a box, and two boxes.
                    let m = if frame && other_frame { 0 } else { self.margin };
                    if intersects((px - m, py - m, pw + 2 * m, ph + 2 * m), (x, y, w, h)) {
                        y = py + ph + m;
                        moved = true;
                    }
                }
                if !moved {
                    break;
                }
            }
            if y != y0 {
                self.shift(unit, y - y0);
            }
            placed.push(((x, y, w, h), frame));
        }
        // What a group holds stays in it: its frame reaches a cell below the lowest of them.
        if let Some(g) = level {
            let ((_, gy), (_, gh)) = self.frames[g];
            let mut h = gh;
            for k in 0..self.boxes.len() {
                if self.boxes[k].drawn && self.box_in[k] == level {
                    h = h.max(self.at[k].1 + self.boxes[k].size.1 + 1 - gy);
                }
            }
            for inner in 0..self.groups.len() {
                if self.group_in[inner] == level {
                    let ((_, iy), (_, ih)) = self.frames[inner];
                    h = h.max(iy + ih + 1 - gy);
                }
            }
            self.frames[g].1.1 = h;
        }
    }
}

/// The smallest of `groups` that `holds` says holds a thing, the first by name of a size.
fn smallest(groups: &[Group], holds: impl Fn(usize) -> bool) -> Option<usize> {
    (0..groups.len()).filter(|&g| holds(g)).min_by(|&a, &b| {
        (groups[a].area(), &groups[a].name).cmp(&(groups[b].area(), &groups[b].name))
    })
}

/// Where each box and each group of a map is: `nesting`.
pub(crate) type Nesting = (Vec<Option<usize>>, Vec<Option<usize>>);

/// The group each box is in (the smallest whose frame holds its top-left), and the group each group
/// is in (the smallest holding its top-left with a larger frame), as they sit now.
pub(crate) fn nesting(boxes: &[OnMap], groups: &[Group]) -> Nesting {
    let box_in = boxes
        .iter()
        .map(|b| smallest(groups, |g| groups[g].holds(b.at)))
        .collect();
    let group_in = (0..groups.len())
        .map(|inner| {
            smallest(groups, |g| {
                g != inner
                    && groups[g].holds(groups[inner].at)
                    && groups[inner].area() < groups[g].area()
            })
        })
        .collect();
    (box_in, group_in)
}

/// What tidying moves on a map of `boxes` and `groups`, `margin` cells kept around each box, room
/// kept under a card that grows down to `room` cells.
pub(crate) fn plan(boxes: &[OnMap], groups: &[Group], margin: i64, room: i64) -> Plan {
    let (box_in, group_in) = nesting(boxes, groups);
    let mut sheet = Sheet {
        boxes,
        groups,
        margin,
        room,
        at: boxes.iter().map(|b| b.at).collect(),
        frames: groups.iter().map(|g| (g.at, g.size)).collect(),
        box_in,
        group_in,
    };
    // The innermost groups first: a group is tidied once what it holds is.
    let depth = |g: usize| {
        let mut n = 0;
        let mut at = sheet.group_in[g];
        while let Some(up) = at {
            n += 1;
            at = sheet.group_in[up];
            if n > groups.len() {
                break;
            }
        }
        n
    };
    let mut order: Vec<usize> = (0..groups.len()).collect();
    order.sort_by_key(|&g| (std::cmp::Reverse(depth(g)), g));
    for g in order {
        sheet.sweep(Some(g));
    }
    sheet.sweep(None);
    Plan {
        boxes: (0..boxes.len())
            .filter(|&k| sheet.at[k] != boxes[k].at)
            .map(|k| (k, sheet.at[k]))
            .collect(),
        groups: (0..groups.len())
            .filter(|&g| sheet.frames[g] != (groups[g].at, groups[g].size))
            .map(|g| (g, sheet.frames[g].0, sheet.frames[g].1))
            .collect(),
    }
}

/// Whether a section's map is tidied: not a ui section's (its parts sit as the mock lays them)
/// nor a working directory's (its files are not boxes the core moves).
pub(crate) fn tidied_here(project: &crate::export::Project, section: &str) -> bool {
    !matches!(
        project.section_kind(section),
        Some(super::frames::UI | crate::kinds::WORKDIR | crate::kinds::DIRECTORY)
    )
}

/// `section_update`'s `tidy`: what `plan` moves written in the call's one transaction, each box
/// moved marked as put by hand. Where each went, as `tidied` says it (a group's `size` when its
/// frame grew). Refused in a ui section and a working directory.
pub(crate) fn tidy(tx: &mut Tx, scope: &Scope, section: &str) -> Result<Vec<Value>, String> {
    let (boxes, groups, plan) = {
        let seen = Seen::in_tx(tx, scope)?;
        let project = seen.project();
        if !tidied_here(&project, section) {
            return Err(text!(
                "boxes.tidy.notHere",
                path = project.directory.full_section(section)
            ));
        }
        let layout = Layout::read(tx.conn(), &scope.project)?;
        let boxes = super::boxes::on_map(&tx.reader(), &seen, &layout, section)?;
        let groups = super::groups::on_view(tx.conn(), &scope.project, &scope.tree, section)?;
        let plan = plan(&boxes, &groups, layout.margin, rows_for_rows(layout.rows));
        (boxes, groups, plan)
    };
    let mut out = Vec::new();
    for &(k, at) in &plan.boxes {
        super::groups::put_box(tx, scope, &boxes[k], at)?;
        let mut row = Map::new();
        row.insert(boxes[k].key.into(), boxes[k].path.clone().into());
        row.insert("topLeft".into(), point(at.0, at.1));
        out.push(Value::Object(row));
    }
    for &(g, at, size) in &plan.groups {
        let mut group = groups[g].clone();
        group.at = at;
        group.size = size;
        super::groups::put_group(tx, scope, &group)?;
        let mut row = Map::new();
        row.insert("group".into(), group.name.clone().into());
        row.insert("topLeft".into(), point(at.0, at.1));
        if size != groups[g].size {
            row.insert("size".into(), super::groups::size_value(size));
        }
        out.push(Value::Object(row));
    }
    Ok(out)
}

/// The boxes that overlap on each section's diagram (the check's `overlaps`): for each box tidying
/// would move that lies over another where they all sit now — a card, a card ghost or a section
/// ghost — its section, the boxes it lies over, and where tidying puts it (`moveTo`). A group's
/// frame drawn over boxes is not an overlap. Not a ui section (parts lie over one another as the
/// mock lays them) nor a working directory, which are not tidied.
pub(crate) fn overlaps(
    reader: &Reader,
    seen: &Seen,
    layout: &Layout,
) -> Result<Vec<Map<String, Value>>, String> {
    let project = seen.project();
    let branch = (!seen.named.tree.is_empty()).then_some(seen.named.tree.as_str());
    let made = reader.compose(&seen.summary.id, branch)?;
    let lines = super::surround::Lines::default();
    let mut out = Vec::new();
    for section in &project.detail.sections {
        if !tidied_here(&project, &section.id) {
            continue;
        }
        let boxes = super::boxes::on_map_in(&made, seen, layout, &section.id, &lines);
        if boxes.len() < 2 {
            continue;
        }
        let groups =
            super::groups::on_view(reader.conn, &seen.summary.id, &seen.named.tree, &section.id)?;
        let plan = plan(&boxes, &groups, layout.margin, rows_for_rows(layout.rows));
        for (k, to) in plan.boxes {
            let b = &boxes[k];
            if !b.drawn {
                continue;
            }
            let over: Vec<Value> = boxes
                .iter()
                .enumerate()
                .filter(|(j, o)| *j != k && o.drawn && intersects(o.rect(), b.rect()))
                .map(|(_, o)| Value::from(o.path.clone()))
                .collect();
            // A box clear where it sits, moved only off where another one lands, is not said.
            if over.is_empty() {
                continue;
            }
            let mut row = Map::new();
            row.insert("at".into(), b.path.clone().into());
            row.insert(
                "section".into(),
                project.directory.full_section(&section.id).into(),
            );
            row.insert("over".into(), Value::Array(over));
            row.insert("moveTo".into(), point(to.0, to.1));
            out.push(row);
        }
    }
    Ok(out)
}
