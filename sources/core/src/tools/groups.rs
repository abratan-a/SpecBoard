//! Groups on a diagram (`specboard://SpecBoard/Docs/Design:Overview` §12): named frames put on a
//! section's map or a container's card map, each a row of `places` (`item_kind` `group`, its name the
//! `item`, how it looks in `look`). A group holds nothing: what is in it is worked out each time from
//! the boxes as they are — a box whose top-left cell is inside the frame, and a group whose top-left
//! is inside and whose frame is smaller — so a card growing out of the frame stays in it, and no list
//! can go stale against the boxes. Its name is its key, so a branch's group and main's of one name
//! are one group, as a card's place is; a rename lets go of the old name (on a branch over main's, a
//! row with no cells says it is gone there). Moving a group's frame moves what is in it by as much,
//! in the same write, kept together clear of the boxes outside as `move` keeps a set. Tidying
//! (`tidy`) and arranging along the lines (`arrange` by `lines`) take a frame as one box with what it
//! holds, and arranging fits the frame round it; nothing else (a drop, `arrange` by `order`, the
//! checks) looks at a frame, but a card placed with no place said keeps out of one, so it never
//! joins a group unasked.

use super::Seen;
use super::boxes::point;
use super::cards::Scope;
use super::grid::{Layout, Rect};
use crate::store::Reader;
use crate::wire::GRID_REACH;
use crate::write::{Spot, Tx, db};
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};

/// The kind of item a group is kept as in `places`.
pub(crate) const KIND: &str = "group";
/// The smallest frame: room for its name's strip and a row under it.
pub(crate) const LEAST: (i64, i64) = (6, 3);
/// The widest and tallest frame.
const MOST_SIDE: i64 = 1000;
/// A frame made with no size said.
const NEW_SIZE: (i64, i64) = (6, 4);
/// The most groups one diagram holds.
const MOST: usize = 200;
const NAME_MOST: usize = 80;
const LABEL_MOST: usize = 120;
/// The colours a frame takes: the kinds' accents, named as a line's colours are.
const COLORS: [&str; 5] = ["blue", "green", "amber", "violet", "red"];
const FILLS: [&str; 7] = [
    "none",
    "check",
    "stripes",
    "diagonal",
    "dots",
    "zebra",
    "zebraDiagonal",
];
/// The corner the name is shown in, as a ui part's `headerPlace`.
const HEADER_PLACES: [&str; 5] = ["topLeft", "topRight", "bottomLeft", "bottomRight", "hidden"];
/// The keys a group's value takes, and those a read says of it that are let be, so a read group
/// can be written back with one thing changed.
const KEYS: [&str; 10] = [
    "topLeft",
    "size",
    "around",
    "label",
    "headerPlace",
    "color",
    "fill",
    "name",
    "carry",
    "new",
];
const READ_KEYS: [&str; 3] = ["bottomRight", "members", "groups"];

/// How a group looks, what is not the default (`look`'s JSON keeps only that).
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Look {
    pub label: Option<String>,
    pub header_place: Option<String>,
    pub color: Option<String>,
    pub fill: Option<String>,
}

impl Look {
    pub(crate) fn read(text: Option<&str>) -> Look {
        let Some(Value::Object(o)) = text.and_then(|t| serde_json::from_str::<Value>(t).ok())
        else {
            return Look::default();
        };
        let said = |key: &str| o.get(key).and_then(Value::as_str).map(str::to_string);
        Look {
            label: said("label"),
            header_place: said("headerPlace"),
            color: said("color"),
            fill: said("fill"),
        }
    }

    /// The keys said of it, in the order a read says them.
    pub(crate) fn said(&self) -> Map<String, Value> {
        let mut out = Map::new();
        for (key, value) in [
            ("label", &self.label),
            ("headerPlace", &self.header_place),
            ("color", &self.color),
            ("fill", &self.fill),
        ] {
            if let Some(v) = value {
                out.insert(key.into(), v.clone().into());
            }
        }
        out
    }

    /// `look` as it is kept: none when everything is the default.
    fn text(&self) -> Option<String> {
        let said = self.said();
        (!said.is_empty()).then(|| Value::Object(said).to_string())
    }
}

/// A group as a tree sees it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Group {
    /// The diagram: a section's id, or a container card's id for its card map.
    pub view: String,
    pub name: String,
    pub at: (i64, i64),
    pub size: (i64, i64),
    pub look: Look,
}

impl Group {
    pub(crate) fn rect(&self) -> Rect {
        (self.at.0, self.at.1, self.size.0, self.size.1)
    }

    pub(crate) fn holds(&self, (x, y): (i64, i64)) -> bool {
        x >= self.at.0
            && x < self.at.0 + self.size.0
            && y >= self.at.1
            && y < self.at.1 + self.size.1
    }

    pub(crate) fn area(&self) -> i64 {
        self.size.0 * self.size.1
    }

    /// What an answer says of where it is.
    fn placed(&self) -> Value {
        let mut out = Map::new();
        out.insert("topLeft".into(), point(self.at.0, self.at.1));
        out.insert(
            "bottomRight".into(),
            point(self.at.0 + self.size.0 - 1, self.at.1 + self.size.1 - 1),
        );
        out.insert("size".into(), size_value(self.size));
        Value::Object(out)
    }
}

pub(crate) fn size_value((w, h): (i64, i64)) -> Value {
    let mut out = Map::new();
    out.insert("w".into(), w.into());
    out.insert("h".into(), h.into());
    Value::Object(out)
}

/// Whether two names are one group's: compared ignoring case, as a card's name is.
pub(crate) fn same(a: &str, b: &str) -> bool {
    a.to_lowercase() == b.to_lowercase()
}

/// The trees a tree's groups are read from, in order: main beneath a branch, then its own. The
/// board stands alone.
fn trees(tree: &str) -> Vec<&str> {
    let mut out = Vec::new();
    if !crate::store::is_own_tree(tree) {
        out.push("");
    }
    if !tree.is_empty() {
        out.push(tree);
    }
    out
}

/// Every group `tree` sees, by diagram then name: a branch's row over main's, a row with no cells
/// (one gone on the branch) leaving none.
pub(crate) fn of_tree(conn: &Connection, project: &str, tree: &str) -> Result<Vec<Group>, String> {
    let mut seen: BTreeMap<(String, String), Option<Group>> = BTreeMap::new();
    let mut stmt = conn
        .prepare(
            "SELECT view, item, x, y, w, h, look FROM places WHERE project_id = ? AND branch_id = ? AND item_kind = 'group'",
        )
        .map_err(db)?;
    for at in trees(tree) {
        type Row = (
            String,
            String,
            Option<i64>,
            Option<i64>,
            Option<i64>,
            Option<i64>,
            Option<String>,
        );
        let rows: Vec<Row> = stmt
            .query_map(params![project, at], |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                    r.get(6)?,
                ))
            })
            .map_err(db)?
            .collect::<Result<_, _>>()
            .map_err(db)?;
        for (view, name, x, y, w, h, look) in rows {
            let group = match (x, y, w, h) {
                // Held within what MCP writes: a project file's places, or another store's, come in
                // as they were written there.
                (Some(x), Some(y), Some(w), Some(h)) => Some(Group {
                    view: view.clone(),
                    name: name.clone(),
                    at: (
                        x.clamp(-GRID_REACH, GRID_REACH),
                        y.clamp(-GRID_REACH, GRID_REACH),
                    ),
                    size: (w.clamp(LEAST.0, MOST_SIDE), h.clamp(LEAST.1, MOST_SIDE)),
                    look: Look::read(look.as_deref()),
                }),
                _ => None,
            };
            seen.insert((view, name), group);
        }
    }
    Ok(seen.into_values().flatten().collect())
}

/// The groups on one diagram, by name.
pub(crate) fn on_view(
    conn: &Connection,
    project: &str,
    tree: &str,
    view: &str,
) -> Result<Vec<Group>, String> {
    Ok(of_tree(conn, project, tree)?
        .into_iter()
        .filter(|g| g.view == view)
        .collect())
}

/// The frames on each diagram, as rects: what a card placed with no place said keeps out of.
pub(crate) fn frames(
    conn: &Connection,
    project: &str,
    tree: &str,
) -> Result<BTreeMap<String, Vec<Rect>>, String> {
    let mut out: BTreeMap<String, Vec<Rect>> = BTreeMap::new();
    for g in of_tree(conn, project, tree)? {
        out.entry(g.view.clone()).or_default().push(g.rect());
    }
    Ok(out)
}

/// The groups of a tree for the window (`section_positions`' `groups`): the diagram → the name →
/// `{x, y, w, h}` and how it looks where that is not the default.
pub(crate) fn positions(conn: &Connection, project: &str, tree: &str) -> Result<Value, String> {
    let mut out: BTreeMap<String, Map<String, Value>> = BTreeMap::new();
    for g in of_tree(conn, project, tree)? {
        let mut row = Map::new();
        row.insert("x".into(), g.at.0.into());
        row.insert("y".into(), g.at.1.into());
        row.insert("w".into(), g.size.0.into());
        row.insert("h".into(), g.size.1.into());
        row.extend(g.look.said());
        out.entry(g.view)
            .or_default()
            .insert(g.name, Value::Object(row));
    }
    Ok(serde_json::to_value(out).unwrap_or_default())
}

// MARK: - The boxes of a diagram

/// A diagram a group is put on: a section's map, or a container card's card map.
#[derive(Clone, Copy)]
pub(crate) enum MapOf<'a> {
    Section(&'a str),
    Card(&'a str),
}

impl MapOf<'_> {
    fn view(&self) -> &str {
        match self {
            MapOf::Section(id) | MapOf::Card(id) => id,
        }
    }
}

/// A box on a diagram, as a group takes it in: its path, where it sits and how big it is, and the row
/// it is kept as (`places`' view, kind and item).
pub(crate) struct OnMap {
    pub path: String,
    pub at: (i64, i64),
    pub size: (i64, i64),
    /// Drawn as a card of its own: a value object or an enum folded into the cards holding it
    /// blocks nothing, but is carried with the group its place is in.
    pub drawn: bool,
    /// Grows as rows of it are opened (`grid::grows`): tidying leaves room under it.
    pub grows: bool,
    pub view: String,
    pub kind: &'static str,
    pub item: String,
    /// The key an answer names its path by (`card`, `sectionGhost`).
    pub key: &'static str,
}

impl OnMap {
    pub(crate) fn rect(&self) -> Rect {
        (self.at.0, self.at.1, self.size.0, self.size.1)
    }
}

/// Every box placed on the diagram, as the tree reads it.
fn on_map(reader: &Reader, seen: &Seen, layout: &Layout, map: MapOf) -> Result<Vec<OnMap>, String> {
    match map {
        MapOf::Section(section) => super::boxes::on_map(reader, seen, layout, section),
        MapOf::Card(card) => {
            let project = seen.project();
            let owner = project
                .card(card)
                .ok_or_else(|| text!("cardMap.card.missing"))?;
            Ok(super::card_map::on_map(&project, owner, layout))
        }
    }
}

/// Whether the diagram can hold groups: a section's map but a ui section's (a mock, its frames
/// drawn with frame parts) or a working directory's (its files are not boxes the core moves), and a
/// goal's or a container kind's card map.
fn refuse_map(seen: &Seen, map: MapOf) -> Result<(), String> {
    let project = seen.project();
    match map {
        MapOf::Section(section) => {
            let kind = project.section_kind(section);
            if kind == Some(super::frames::UI) || kind == Some(crate::kinds::WORKDIR) {
                return Err(text!(
                    "groups.notHere",
                    path = project.directory.full_section(section)
                ));
            }
        }
        MapOf::Card(card) => {
            let owner = project
                .card(card)
                .ok_or_else(|| text!("cardMap.card.missing"))?;
            if !project.card_mapped(&owner.kind) {
                return Err(text!("groups.notHere", path = project.full(owner)));
            }
        }
    }
    Ok(())
}

/// The refusal for a card that is not a container, said before anything of it is written.
pub(crate) fn not_here(path: &str) -> String {
    text!("groups.notHere", path)
}

/// The boxes whose top-left cell is inside `g`, as indexes into `boxes`.
fn members(g: &Group, boxes: &[OnMap]) -> Vec<usize> {
    (0..boxes.len()).filter(|&i| g.holds(boxes[i].at)).collect()
}

/// The groups inside `g`: their top-left inside it, their frame smaller (one of the same size is
/// inside neither, so no two groups hold each other).
fn nested(g: &Group, groups: &[Group]) -> Vec<usize> {
    (0..groups.len())
        .filter(|&i| groups[i] != *g && g.holds(groups[i].at) && groups[i].area() < g.area())
        .collect()
}

/// What moving the groups `picked` carries: the boxes in them and the groups inside them, and
/// those groups' own, all the way down — as indexes into `boxes` and `groups` (`picked` among them).
fn carried(
    picked: &[usize],
    groups: &[Group],
    boxes: &[OnMap],
) -> (BTreeSet<usize>, BTreeSet<usize>) {
    let mut held_boxes = BTreeSet::new();
    let mut held_groups: BTreeSet<usize> = picked.iter().copied().collect();
    let mut todo: Vec<usize> = picked.to_vec();
    while let Some(i) = todo.pop() {
        held_boxes.extend(members(&groups[i], boxes));
        for n in nested(&groups[i], groups) {
            if held_groups.insert(n) {
                todo.push(n);
            }
        }
    }
    (held_boxes, held_groups)
}

/// A row as the tree has it, its own or main's beneath a branch.
fn row(tx: &Tx, scope: &Scope, view: &str, kind: &str, item: &str) -> Result<Spot, String> {
    Ok(tx
        .place_row(&scope.project, &scope.tree, view, kind, item)?
        .unwrap_or_default())
}

/// A box put at `at` by hand, as it were, its size as the tree has it.
pub(crate) fn put_box(tx: &mut Tx, scope: &Scope, b: &OnMap, at: (i64, i64)) -> Result<(), String> {
    let mut s = row(tx, scope, &b.view, b.kind, &b.item)?;
    s.x = Some(at.0);
    s.y = Some(at.1);
    s.pinned = true;
    tx.place(
        &scope.project,
        &scope.tree,
        &b.view,
        b.kind,
        &b.item,
        Some(&s),
    )
}

/// A group's row written as it is.
pub(crate) fn put_group(tx: &mut Tx, scope: &Scope, g: &Group) -> Result<(), String> {
    let s = Spot {
        x: Some(g.at.0),
        y: Some(g.at.1),
        w: Some(g.size.0),
        h: Some(g.size.1),
        pinned: true,
        look: g.look.text(),
    };
    tx.place(
        &scope.project,
        &scope.tree,
        &g.view,
        KIND,
        &g.name,
        Some(&s),
    )
}

/// A group let go of: on a branch over main's group of that name, a row with no cells, so main's does
/// not show through; else its row gone.
fn drop_group(tx: &mut Tx, scope: &Scope, view: &str, name: &str) -> Result<(), String> {
    let on_branch = !scope.tree.is_empty() && !crate::store::is_own_tree(&scope.tree);
    let in_main = on_branch
        && tx
            .conn()
            .query_row(
                "SELECT 1 FROM places WHERE project_id = ? AND branch_id = '' AND view = ? AND item_kind = 'group' AND item = ? AND x IS NOT NULL",
                params![scope.project, view, name],
                |_| Ok(()),
            )
            .optional()
            .map_err(db)?
            .is_some();
    let gone = Spot::default();
    tx.place(
        &scope.project,
        &scope.tree,
        view,
        KIND,
        name,
        in_main.then_some(&gone),
    )
}

/// A box as an answer says where it went.
fn went_row(key: &str, path: &str, at: (i64, i64)) -> Value {
    let mut row = Map::new();
    row.insert(key.into(), path.into());
    row.insert("topLeft".into(), point(at.0, at.1));
    Value::Object(row)
}

/// What moving the groups `picked` by `by` writes: every box and group they carry moved by one
/// offset, kept together clear of the boxes outside them (`boxes::clear_offset`; a frame blocks
/// nothing). Also `extra` boxes moved with them (picked by `move`). Returns where they went, as
/// `moved` says it, and the offset taken.
#[allow(clippy::too_many_arguments)]
fn carry(
    tx: &mut Tx,
    scope: &Scope,
    margin: i64,
    clear: bool,
    boxes: &[OnMap],
    groups: &[Group],
    picked: &[usize],
    extra: &[usize],
    by: (i64, i64),
) -> Result<(Vec<Value>, (i64, i64)), String> {
    let (mut held_boxes, held_groups) = carried(picked, groups, boxes);
    held_boxes.extend(extra.iter().copied());
    let moving: Vec<Rect> = held_boxes
        .iter()
        .filter(|&&i| boxes[i].drawn)
        .map(|&i| boxes[i].rect())
        .collect();
    let others: Vec<Rect> = (0..boxes.len())
        .filter(|i| !held_boxes.contains(i) && boxes[*i].drawn)
        .map(|i| boxes[i].rect())
        .collect();
    let went = if clear {
        super::boxes::clear_offset(&moving, &others, margin, by)
    } else {
        by
    };
    let mut moved = Vec::new();
    let mut order: Vec<usize> = held_boxes.into_iter().collect();
    order.sort_by_key(|&i| (boxes[i].at.1, boxes[i].at.0));
    for i in order {
        let b = &boxes[i];
        let at = (b.at.0 + went.0, b.at.1 + went.1);
        put_box(tx, scope, b, at)?;
        moved.push(went_row(b.key, &b.path, at));
    }
    for i in held_groups {
        let mut g = groups[i].clone();
        g.at = (g.at.0 + went.0, g.at.1 + went.1);
        put_group(tx, scope, &g)?;
        if !picked.contains(&i) {
            moved.push(went_row("group", &g.name, g.at));
        }
    }
    Ok((moved, went))
}

/// The names `move`'s `groups` gives, each a group on `section`'s map: as indexes into `groups`.
fn named(raw: &Value, groups: &[Group]) -> Result<Vec<usize>, String> {
    let list = raw
        .as_array()
        .filter(|l| !l.is_empty())
        .ok_or_else(|| text!("groups.moveShape"))?;
    let mut out = Vec::new();
    for one in list {
        let name = one.as_str().ok_or_else(|| text!("groups.moveShape"))?;
        let at = groups
            .iter()
            .position(|g| same(&g.name, name.trim()))
            .ok_or_else(|| no_such(name, groups))?;
        if !out.contains(&at) {
            out.push(at);
        }
    }
    Ok(out)
}

fn no_such(name: &str, groups: &[Group]) -> String {
    let names: Vec<&str> = groups.iter().map(|g| g.name.as_str()).collect();
    text!(
        "groups.noSuch",
        name,
        names = if names.is_empty() {
            "-".to_string()
        } else {
            names.join(", ")
        }
    )
}

/// The groups `move` names on `section`'s map, with what they carry, moved with the boxes it picked
/// (`picked`, indexes into `boxes::on_map`'s order): `by`, or `to` for the top left of all of them.
/// Returns where each went (`moved`) and `shifted` when they went further than asked.
#[allow(clippy::too_many_arguments)]
pub(crate) fn move_with(
    tx: &mut Tx,
    scope: &Scope,
    section: &str,
    names: Option<&Value>,
    picked: &[usize],
    by: Option<(i64, i64)>,
    to: Option<(i64, i64)>,
    clear: bool,
) -> Result<Map<String, Value>, String> {
    let (layout, boxes, groups) = {
        let seen = Seen::in_tx(tx, scope)?;
        let layout = Layout::read(tx.conn(), &scope.project)?;
        let boxes = super::boxes::on_map(&tx.reader(), &seen, &layout, section)?;
        let groups = on_view(tx.conn(), &scope.project, &scope.tree, section)?;
        (layout, boxes, groups)
    };
    let picked_groups = match names {
        Some(raw) => named(raw, &groups)?,
        None => Vec::new(),
    };
    let by = match (by, to) {
        (Some(by), _) => by,
        (None, Some((x, y))) => {
            let (held_boxes, held_groups) = carried(&picked_groups, &groups, &boxes);
            let corners: Vec<(i64, i64)> = picked
                .iter()
                .chain(held_boxes.iter())
                .map(|&i| boxes[i].at)
                .chain(held_groups.iter().map(|&i| groups[i].at))
                .collect();
            let left = corners.iter().map(|c| c.0).min().unwrap_or(x);
            let top = corners.iter().map(|c| c.1).min().unwrap_or(y);
            (x - left, y - top)
        }
        (None, None) => return Err(text!("boxes.move.byOrTo")),
    };
    let (moved, went) = carry(
        tx,
        scope,
        layout.margin,
        clear,
        &boxes,
        &groups,
        &picked_groups,
        picked,
        by,
    )?;
    let mut moved = moved;
    for &i in &picked_groups {
        let g = &groups[i];
        moved.push(went_row(
            "group",
            &g.name,
            (g.at.0 + went.0, g.at.1 + went.1),
        ));
    }
    let mut out = Map::new();
    out.insert("moved".into(), Value::Array(moved));
    if went != by {
        out.insert("shifted".into(), point(went.0 - by.0, went.1 - by.1));
    }
    Ok(out)
}

// MARK: - Writing groups

/// A group's name, checked: trimmed, 1 to 80 characters, no control characters.
fn valid_name(raw: &str) -> Result<String, String> {
    let name = raw.trim();
    if name.is_empty() || name.chars().count() > NAME_MOST || name.chars().any(char::is_control) {
        return Err(text!("groups.name", name = raw));
    }
    Ok(name.to_string())
}

/// A size `{w, h}`, both whole numbers in the frame's range.
fn valid_size(name: &str, raw: &Value) -> Result<(i64, i64), String> {
    let shape = || text!("groups.sizeShape", name);
    let o = raw.as_object().ok_or_else(shape)?;
    if o.keys().any(|k| k != "w" && k != "h") {
        return Err(shape());
    }
    let whole = |key: &str| {
        o.get(key)
            .and_then(Value::as_f64)
            .filter(|f| *f == f.trunc())
            .map(|f| f as i64)
            .ok_or_else(shape)
    };
    let (w, h) = (whole("w")?, whole("h")?);
    in_range(w, h)?;
    Ok((w, h))
}

fn in_range(w: i64, h: i64) -> Result<(), String> {
    if !(LEAST.0..=MOST_SIDE).contains(&w) || !(LEAST.1..=MOST_SIDE).contains(&h) {
        return Err(text!("groups.size", w, h));
    }
    Ok(())
}

/// One of `allowed`, or none for null; refused otherwise with `key`'s refusal.
fn one_of(name: &str, raw: &Value, allowed: &[&str], key: &str) -> Result<Option<String>, String> {
    match raw {
        Value::Null => Ok(None),
        Value::String(s) if allowed.contains(&s.as_str()) => Ok(Some(s.clone())),
        other => {
            let value = other
                .as_str()
                .map_or_else(|| other.to_string(), str::to_string);
            Err(match key {
                "headerPlace" => text!("groups.headerPlace", name, value),
                "color" => text!("groups.color", name, value),
                _ => text!("groups.fill", name, value),
            })
        }
    }
}

/// How a group looks with what `spec` says of it put over `look`: the default kept as none.
fn looked(name: &str, spec: &Map<String, Value>, mut look: Look) -> Result<Look, String> {
    if let Some(raw) = spec.get("label") {
        look.label = match raw {
            Value::Null => None,
            Value::String(s) if s.chars().count() <= LABEL_MOST => {
                let s = s.trim();
                (!s.is_empty()).then(|| s.to_string())
            }
            _ => return Err(text!("groups.label", name)),
        };
    }
    if let Some(raw) = spec.get("headerPlace") {
        look.header_place =
            one_of(name, raw, &HEADER_PLACES, "headerPlace")?.filter(|p| p != "topLeft");
    }
    if let Some(raw) = spec.get("color") {
        look.color = one_of(name, raw, &COLORS, "color")?;
    }
    if let Some(raw) = spec.get("fill") {
        look.fill = one_of(name, raw, &FILLS, "fill")?.filter(|f| f != "none");
    }
    Ok(look)
}

/// A frame: its top-left cell and its size.
type Frame = ((i64, i64), (i64, i64));

/// `around`: the frame round the boxes the paths name — a cell left, right and below, two above
/// for the name's strip — grown to the smallest frame. A side whose room would lie over the frame
/// of another group (one holding none of the boxes, so not the group it goes in) has its room cut
/// down to none: the frames do not overlap, and the new group's top-left is not inside the other,
/// which would make it a group inside that one.
fn around(
    seen: &Seen,
    map: MapOf,
    boxes: &[OnMap],
    others: &[Group],
    name: &str,
    raw: &Value,
) -> Result<Frame, String> {
    let paths = raw
        .as_array()
        .filter(|l| !l.is_empty())
        .ok_or_else(|| text!("groups.aroundShape", name))?;
    let project = seen.project();
    let mut picked: Vec<&OnMap> = Vec::new();
    for raw in paths {
        let path = raw
            .as_str()
            .ok_or_else(|| text!("groups.aroundShape", name))?;
        let wrong = |e: String| text!("groups.aroundPath", name, path, e);
        let (kind, id) = match map {
            MapOf::Section(section) => {
                if crate::pathtext::has_section(path) {
                    match super::walk::find_named(&project, path, Some(section)).map_err(wrong)? {
                        crate::export::Named::Card(card) => ("card", card.id.clone()),
                        crate::export::Named::Ghost(ghost) => ("card", ghost.id.clone()),
                    }
                } else {
                    ("section", super::section_id(&project, path).map_err(wrong)?)
                }
            }
            MapOf::Card(goal) => {
                let owner = project
                    .card(goal)
                    .ok_or_else(|| text!("cardMap.card.missing"))?;
                let card = match project.inner_named(&owner.id, path.trim()) {
                    Some(card) => card,
                    None => {
                        super::walk::find_card(&project, path, Some(&project.section_of(owner)))
                            .map_err(wrong)?
                    }
                };
                ("card", card.id.clone())
            }
        };
        let b = boxes
            .iter()
            .find(|b| b.kind == kind && b.item == id)
            .ok_or_else(|| text!("groups.aroundNotOnMap", name, path))?;
        picked.push(b);
    }
    let left = picked.iter().map(|b| b.at.0).min().unwrap_or(0);
    let top = picked.iter().map(|b| b.at.1).min().unwrap_or(0);
    let right = picked.iter().map(|b| b.at.0 + b.size.0).max().unwrap_or(0);
    let bottom = picked.iter().map(|b| b.at.1 + b.size.1).max().unwrap_or(0);
    let beside: Vec<Rect> = others
        .iter()
        .filter(|g| !picked.iter().any(|b| g.holds(b.at)))
        .map(Group::rect)
        .collect();
    let (at, size) = framed((left, top, right - left, bottom - top), &beside);
    in_range(size.0, size.1)?;
    Ok((at, size))
}

/// The frame round a box `(x, y, w, h)`: a cell of room left, right and below, two above, each
/// side's room cut down (to none at the least) where it would lie over one of `beside`; grown to
/// the smallest frame.
pub(crate) fn framed((x, y, w, h): Rect, beside: &[Rect]) -> Frame {
    let (mut l, mut t, mut r, mut b) = (1, 2, 1, 1);
    for &(gx, gy, gw, gh) in beside {
        let frame = (x - l, y - t, w + l + r, h + t + b);
        if !super::grid::intersects(frame, (gx, gy, gw, gh)) {
            continue;
        }
        if gx + gw <= x {
            l = l.min(x - (gx + gw)).max(0);
        }
        if gx >= x + w {
            r = r.min(gx - (x + w)).max(0);
        }
        if gy + gh <= y {
            t = t.min(y - (gy + gh)).max(0);
        }
        if gy >= y + h {
            b = b.min(gy - (y + h)).max(0);
        }
    }
    (
        (x - l, y - t),
        ((w + l + r).max(LEAST.0), (h + t + b).max(LEAST.1)),
    )
}

/// `groups` of `section_update` (a section's map) or `card_update` (a container's card map): each
/// group made, changed, renamed, moved with what it holds, or let go of, in the order given, each
/// seeing the ones before it. Returns `groups` (where each is now, or null when let go of), with
/// `moved` and `shifted` when what they held moved.
pub(crate) fn put_groups(
    tx: &mut Tx,
    scope: &Scope,
    map: MapOf,
    raw: &Value,
) -> Result<Map<String, Value>, String> {
    let given = raw.as_object().ok_or_else(|| text!("groups.shape"))?;
    {
        let seen = Seen::in_tx(tx, scope)?;
        refuse_map(&seen, map)?;
    }
    let view = map.view().to_string();
    let mut answer = Map::new();
    let mut moved: Vec<Value> = Vec::new();
    let mut shifted = None;
    for (key, spec) in given {
        let groups = on_view(tx.conn(), &scope.project, &scope.tree, &view)?;
        let found = groups.iter().position(|g| same(&g.name, key.trim()));
        let spec = match spec {
            Value::Null => {
                let at = found.ok_or_else(|| no_such(key, &groups))?;
                drop_group(tx, scope, &view, &groups[at].name)?;
                answer.insert(groups[at].name.clone(), Value::Null);
                continue;
            }
            Value::Object(o) => o,
            _ => return Err(text!("groups.valueShape", name = key)),
        };
        let stray: Vec<&str> = spec
            .keys()
            .map(String::as_str)
            .filter(|k| !KEYS.contains(k) && !READ_KEYS.contains(k))
            .collect();
        if !stray.is_empty() {
            return Err(text!(
                "groups.strayKeys",
                name = key,
                keys = stray.join(", ")
            ));
        }
        if spec.contains_key("around")
            && (spec.contains_key("topLeft") || spec.contains_key("size"))
        {
            return Err(text!("groups.aroundWithPlace", name = key));
        }
        let carry_on = match spec.get("carry") {
            None => true,
            Some(Value::Bool(b)) => *b,
            Some(_) => return Err(text!("groups.carry", name = key)),
        };
        // Made only: a name another hand made first is refused, not moved as if it were this one's.
        let made_only = match spec.get("new") {
            None => false,
            Some(Value::Bool(b)) => *b,
            Some(_) => return Err(text!("groups.new", name = key)),
        };
        if let (true, Some(at)) = (made_only, found) {
            return Err(text!("groups.duplicate", other = groups[at].name));
        }
        let top_left = spec
            .get("topLeft")
            .map(super::update::grid_point)
            .transpose()
            .map_err(|e| text!("groups.topLeft", name = key, e))?;
        let size = spec.get("size").map(|s| valid_size(key, s)).transpose()?;
        let (layout, boxes) = {
            let seen = Seen::in_tx(tx, scope)?;
            let layout = Layout::read(tx.conn(), &scope.project)?;
            let boxes = on_map(&tx.reader(), &seen, &layout, map)?;
            (layout, boxes)
        };
        let framed = match spec.get("around") {
            Some(raw) => {
                let seen = Seen::in_tx(tx, scope)?;
                let others: Vec<Group> = groups
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| Some(*i) != found)
                    .map(|(_, g)| g.clone())
                    .collect();
                Some(around(&seen, map, &boxes, &others, key, raw)?)
            }
            None => None,
        };
        // The name it ends with: a new one said, else the one it has, else the key.
        let renamed = spec
            .get("name")
            .map(|n| {
                n.as_str()
                    .map(valid_name)
                    .unwrap_or_else(|| Err(text!("groups.name", name = n)))
            })
            .transpose()?;
        let name = match (&renamed, found) {
            (Some(n), _) => n.clone(),
            (None, Some(at)) => groups[at].name.clone(),
            (None, None) => valid_name(key)?,
        };
        if let Some(other) = groups
            .iter()
            .enumerate()
            .find(|(i, g)| Some(*i) != found && same(&g.name, &name))
        {
            return Err(text!("groups.duplicate", other = other.1.name));
        }
        let group = match found {
            None => {
                if groups.len() >= MOST {
                    return Err(text!("groups.tooMany"));
                }
                let (at, size) = match (framed, top_left) {
                    (Some(framed), _) => framed,
                    (None, Some(at)) => (at, size.unwrap_or(NEW_SIZE)),
                    (None, None) => return Err(text!("groups.needsPlace", name)),
                };
                Group {
                    view: view.clone(),
                    name: name.clone(),
                    at,
                    size,
                    look: looked(&name, spec, Look::default())?,
                }
            }
            Some(at_index) => {
                let old = groups[at_index].clone();
                let mut group = Group {
                    look: looked(&name, spec, old.look.clone())?,
                    name: name.clone(),
                    ..old.clone()
                };
                if let Some((at, size)) = framed {
                    group.at = at;
                    group.size = size;
                } else {
                    if let Some(size) = size {
                        group.size = size;
                    }
                    if let Some(at) = top_left {
                        let by = (at.0 - old.at.0, at.1 - old.at.1);
                        group.at = at;
                        if carry_on && by != (0, 0) {
                            let clear = matches!(map, MapOf::Card(_)) || {
                                let seen = Seen::in_tx(tx, scope)?;
                                !super::grid::keeps_places(&seen.project(), &view)
                            };
                            let (went_rows, went) = carry(
                                tx,
                                scope,
                                layout.margin,
                                clear,
                                &boxes,
                                &groups,
                                &[at_index],
                                &[],
                                by,
                            )?;
                            moved.extend(went_rows);
                            group.at = (old.at.0 + went.0, old.at.1 + went.1);
                            if went != by {
                                shifted = Some(point(went.0 - by.0, went.1 - by.1));
                            }
                        }
                    }
                }
                if old.name != group.name {
                    drop_group(tx, scope, &view, &old.name)?;
                }
                group
            }
        };
        put_group(tx, scope, &group)?;
        answer.insert(group.name.clone(), group.placed());
    }
    let mut out = Map::new();
    out.insert("groups".into(), Value::Object(answer));
    if !moved.is_empty() {
        out.insert("moved".into(), Value::Array(moved));
    }
    if let Some(shifted) = shifted {
        out.insert("shifted".into(), shifted);
    }
    Ok(out)
}

// MARK: - Reading groups

/// A diagram's groups as `section_read` and `card_read` return them (`include: ["groups"]`): by
/// name, each where it is, how it looks where that is not the default, the paths of the boxes in it
/// (from the top down, then left to right) and the groups inside it.
pub(crate) fn read_groups(
    reader: &Reader,
    seen: &Seen,
    layout: &Layout,
    map: MapOf,
) -> Result<Value, String> {
    let groups = on_view(reader.conn, &seen.summary.id, &seen.named.tree, map.view())?;
    if groups.is_empty() {
        return Ok(Value::Object(Map::new()));
    }
    let boxes = on_map(reader, seen, layout, map)?;
    let mut out = Map::new();
    for g in &groups {
        let Value::Object(mut row) = g.placed() else {
            continue;
        };
        row.extend(g.look.said());
        let mut inside = members(g, &boxes);
        inside.sort_by(|&a, &b| {
            (boxes[a].at.1, boxes[a].at.0, &boxes[a].path).cmp(&(
                boxes[b].at.1,
                boxes[b].at.0,
                &boxes[b].path,
            ))
        });
        row.insert(
            "members".into(),
            Value::Array(
                inside
                    .iter()
                    .map(|&i| boxes[i].path.clone().into())
                    .collect(),
            ),
        );
        let mut held: Vec<&str> = nested(g, &groups)
            .into_iter()
            .map(|i| groups[i].name.as_str())
            .collect();
        held.sort();
        if !held.is_empty() {
            row.insert(
                "groups".into(),
                Value::Array(held.into_iter().map(Value::from).collect()),
            );
        }
        out.insert(g.name.clone(), Value::Object(row));
    }
    Ok(Value::Object(out))
}
