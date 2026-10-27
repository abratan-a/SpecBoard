//! Arranging a section's map along its lines (`section_update`'s `arrange` with `by: "lines"`, the
//! screen's 「線に沿って並べ直す」; `specboard://SpecBoard/Docs/Design:Overview` §12).
//!
//! Each set of boxes its lines join is laid out in layers from left to right: a line whose base is
//! `embed` or `designate` (a field naming a shape) puts the box it points at on the left, any other
//! line puts the box it reaches on the right. Rings are broken in card order, a box goes one layer
//! right of the furthest box it comes after, and the layers are reordered by where their neighbours
//! sit, four sweeps each way. A layer taller than the set is wide wraps into another column, so a
//! box many others point at does not make one long pillar; and a set wider than the map is packed
//! across has its layers cut into bands stacked top down, so a long chain does not make one long
//! strip either. The sets, then the boxes no line
//! reaches — those that grow last — are packed from the top left, each into the highest place
//! there is room for, across a width near the square of what they cover.
//!
//! What a group holds is arranged first and its frame fitted round it; outside, the frame is one box
//! with the lines of what it holds. Arranging inside one group keeps its frame's top-left and moves
//! down only what the fitted frame now covers.
//!
//! Every order comes from the order of the boxes on the map (cards, card ghosts, section ghosts) and
//! never from where they sit, so the same map is arranged the same way every time, and a second
//! arrangement moves nothing.

use super::Seen;
use super::boxes::point;
use super::cards::Scope;
use super::grid::{CARD_COLUMNS, Layout, Rect, intersects, rows_for_rows};
use super::groups::{Group, LEAST, OnMap};
use super::tidy::{Framed, Plan};
use crate::kinds::Base;
use crate::write::Tx;
use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet, VecDeque};

/// The cells between two layers: room for the lines between them to pass and bend.
const LANE: i64 = 4;

/// How many times the layers are reordered by their neighbours, each way.
const SWEEPS: usize = 4;

/// A line between two boxes, as indexes into the map's boxes: the second goes right of the first.
pub(crate) type Edge = (usize, usize);

/// What is arranged: the whole map, from a top-left cell (none: where the top left of its boxes is
/// now), or what one group holds.
#[derive(Clone, Copy)]
pub(crate) enum Over {
    Map(Option<(i64, i64)>),
    Group(usize),
}

/// One thing laid out at a level: a box drawn as a card, or a group's frame with what it holds.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum Unit {
    Box(usize),
    Group(usize),
}

/// What a level's units take, laid out from (0, 0): where each goes, and how far they reach across,
/// down as drawn, and down with the room under the cards that grow.
struct Laid {
    at: Vec<(Unit, (i64, i64))>,
    width: i64,
    drawn: i64,
    taken: i64,
}

struct Sheet<'a> {
    boxes: &'a [OnMap],
    groups: &'a [Group],
    edges: Vec<Edge>,
    margin: i64,
    room: i64,
    box_in: Vec<Option<usize>>,
    group_in: Vec<Option<usize>>,
    /// The first box (in the map's order) each group holds at any depth: where it stands among the
    /// units beside it.
    first: Vec<Option<usize>>,
    /// Each group's inside as laid out, from the top-left of the room inside its frame.
    inside: Vec<Vec<(Unit, (i64, i64))>>,
    /// Each group's fitted frame size, and the height it takes with the room under what grows in it.
    fitted: Vec<((i64, i64), i64)>,
    at: Vec<(i64, i64)>,
    frames: Vec<((i64, i64), (i64, i64))>,
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

    /// A unit's width, its height as drawn and the height it takes, as laid out.
    fn measure(&self, unit: Unit) -> (i64, i64, i64) {
        match unit {
            Unit::Box(k) => (self.boxes[k].size.0, self.boxes[k].size.1, self.taken(k)),
            Unit::Group(g) => {
                let ((w, h), taken) = self.fitted[g];
                (w, h, taken)
            }
        }
    }

    /// Where a unit stands among those beside it.
    fn rank(&self, unit: Unit) -> (usize, usize) {
        match unit {
            Unit::Box(k) => (k, 0),
            Unit::Group(g) => match self.first[g] {
                Some(k) => (k, 0),
                None => (usize::MAX, g),
            },
        }
    }

    /// The unit at `level` a box is, or is held in: none when it is not at or under that level.
    fn lift(&self, k: usize, level: Option<usize>) -> Option<Unit> {
        let mut unit = Unit::Box(k);
        let mut at = self.box_in[k];
        for _ in 0..=self.groups.len() {
            if at == level {
                return Some(unit);
            }
            let g = at?;
            unit = Unit::Group(g);
            at = self.group_in[g];
        }
        None
    }

    /// The units of a level — the map's own (`None`) or a group's — in their order.
    fn units(&self, level: Option<usize>) -> Vec<Unit> {
        let mut units: Vec<Unit> = (0..self.boxes.len())
            .filter(|&k| self.boxes[k].drawn && self.box_in[k] == level)
            .map(Unit::Box)
            .chain(
                (0..self.groups.len())
                    .filter(|&g| self.group_in[g] == level)
                    .map(Unit::Group),
            )
            .collect();
        units.sort_by_key(|&u| self.rank(u));
        units
    }

    /// A level's units laid out from (0, 0), what each group among them holds already laid out.
    fn lay(&self, level: Option<usize>) -> Laid {
        let units = self.units(level);
        let index: HashMap<Unit, usize> = units.iter().enumerate().map(|(i, &u)| (u, i)).collect();
        let sizes: Vec<(i64, i64, i64)> = units.iter().map(|&u| self.measure(u)).collect();
        let mut edges: Vec<Edge> = self
            .edges
            .iter()
            .filter_map(|&(a, b)| {
                let a = index.get(&self.lift(a, level)?)?;
                let b = index.get(&self.lift(b, level)?)?;
                (a != b).then_some((*a, *b))
            })
            .collect();
        edges.sort_unstable();
        edges.dedup();
        let at = lay_out(&units, &sizes, &edges, self.margin, |u| match u {
            Unit::Box(k) => self.boxes[k].grows,
            Unit::Group(_) => false,
        });
        let mut laid = Laid {
            at: Vec::with_capacity(units.len()),
            width: 0,
            drawn: 0,
            taken: 0,
        };
        for (i, &unit) in units.iter().enumerate() {
            let (x, y) = at[i];
            let (w, h, taken) = sizes[i];
            laid.width = laid.width.max(x + w);
            laid.drawn = laid.drawn.max(y + h);
            laid.taken = laid.taken.max(y + taken);
            laid.at.push((unit, (x, y)));
        }
        laid
    }

    /// A group's inside laid out and its frame fitted round it: a cell left, right and below, two
    /// above for the name's strip, no smaller than the smallest frame. A group holding nothing drawn
    /// keeps its size.
    fn fit(&mut self, g: usize) {
        let laid = self.lay(Some(g));
        let size = if laid.at.is_empty() {
            self.groups[g].size
        } else {
            ((laid.width + 2).max(LEAST.0), (laid.drawn + 3).max(LEAST.1))
        };
        self.fitted[g] = (size, size.1.max(laid.taken + 2));
        self.inside[g] = laid.at;
    }

    /// A unit put with its top-left at `(x, y)`, and what a group holds where its inside says.
    fn put(&mut self, unit: Unit, (x, y): (i64, i64)) {
        match unit {
            Unit::Box(k) => self.at[k] = (x, y),
            Unit::Group(g) => {
                self.frames[g] = ((x, y), self.fitted[g].0);
                for (inner, (dx, dy)) in std::mem::take(&mut self.inside[g]) {
                    self.put(inner, (x + 1 + dx, y + 2 + dy));
                }
            }
        }
    }

    /// The groups innermost first: a group is laid out once what it holds is.
    fn innermost_first(&self, under: Option<usize>) -> Vec<usize> {
        let depth = |g: usize| {
            let mut n = 0;
            let mut at = self.group_in[g];
            while let Some(up) = at {
                n += 1;
                at = self.group_in[up];
                if n > self.groups.len() {
                    break;
                }
            }
            n
        };
        let mut order: Vec<usize> = (0..self.groups.len())
            .filter(|&g| under.is_none_or(|top| self.within(Some(g), top)))
            .collect();
        order.sort_by_key(|&g| (std::cmp::Reverse(depth(g)), g));
        order
    }

    /// Whether `inner` is `outer` or inside it, at any depth.
    fn within(&self, mut inner: Option<usize>, outer: usize) -> bool {
        for _ in 0..=self.groups.len() {
            match inner {
                Some(g) if g == outer => return true,
                Some(g) => inner = self.group_in[g],
                None => return false,
            }
        }
        false
    }

    /// A unit as it sits now: where, its width, and the height it takes — a frame's down to the
    /// deepest of its own bottom and the room of what it holds.
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

    /// The units of a level moved down off `pusher` — only those it now covers, and those they
    /// then cover — each to the first place clear of every unit above it; then the group holding
    /// them grown to keep them, and that pushed in turn.
    fn push(&mut self, level: Option<usize>, pusher: Unit) {
        let mut units: Vec<(Unit, Rect)> = self
            .units(level)
            .into_iter()
            .filter(|&u| u != pusher)
            .map(|u| (u, self.span(u)))
            .collect();
        units.sort_by_key(|&(u, (x, y, _, _))| (y, x, self.rank(u)));
        let first = (self.span(pusher), matches!(pusher, Unit::Group(_)));
        let mut pushers: Vec<(Rect, bool)> = vec![first];
        let mut placed: Vec<(Rect, bool)> = vec![first];
        let margin = self.margin;
        let hits = |list: &[(Rect, bool)], rect: Rect, frame: bool| {
            list.iter().find_map(|&((px, py, pw, ph), other)| {
                let m = if frame && other { 0 } else { margin };
                intersects((px - m, py - m, pw + 2 * m, ph + 2 * m), rect).then_some(py + ph + m)
            })
        };
        for (unit, (x, y0, w, h)) in units {
            let frame = matches!(unit, Unit::Group(_));
            if hits(&pushers, (x, y0, w, h), frame).is_none() {
                placed.push(((x, y0, w, h), frame));
                continue;
            }
            let mut y = y0;
            while let Some(below) = hits(&placed, (x, y, w, h), frame) {
                y = below;
            }
            self.shift(unit, y - y0);
            pushers.push(((x, y, w, h), frame));
            placed.push(((x, y, w, h), frame));
        }
        // What a group holds stays in it: its frame reaches past the lowest and the rightmost of them.
        let Some(g) = level else {
            return;
        };
        let ((gx, gy), (mut w, mut h)) = self.frames[g];
        let before = (w, h);
        for k in 0..self.boxes.len() {
            if self.boxes[k].drawn && self.box_in[k] == level {
                w = w.max(self.at[k].0 + self.boxes[k].size.0 + 1 - gx);
                h = h.max(self.at[k].1 + self.boxes[k].size.1 + 1 - gy);
            }
        }
        for inner in 0..self.groups.len() {
            if self.group_in[inner] == level {
                let ((ix, iy), (iw, ih)) = self.frames[inner];
                w = w.max(ix + iw + 1 - gx);
                h = h.max(iy + ih + 1 - gy);
            }
        }
        if (w, h) != before {
            self.frames[g].1 = (w, h);
            self.push(self.group_in[g], Unit::Group(g));
        }
    }
}

/// A block packed whole: its units, each where it goes from the block's top-left, and its width
/// and the height it takes.
type Block = (Vec<(usize, (i64, i64))>, i64, i64);

/// Units laid out from (0, 0): each set the lines join in layers, then all of them packed.
/// `sizes`: each unit's width, drawn height and the height it takes. `grows`: whether a unit no line
/// reaches goes after the others.
fn lay_out(
    units: &[Unit],
    sizes: &[(i64, i64, i64)],
    edges: &[Edge],
    margin: i64,
    grows: impl Fn(Unit) -> bool,
) -> Vec<(i64, i64)> {
    let n = units.len();
    let mut parent: Vec<usize> = (0..n).collect();
    fn root(parent: &mut [usize], mut a: usize) -> usize {
        while parent[a] != a {
            parent[a] = parent[parent[a]];
            a = parent[a];
        }
        a
    }
    for &(a, b) in edges {
        let (ra, rb) = (root(&mut parent, a), root(&mut parent, b));
        if ra != rb {
            parent[ra.max(rb)] = ra.min(rb);
        }
    }
    // The sets in the order of their first unit; the units of each in theirs.
    let mut sets: Vec<Vec<usize>> = Vec::new();
    let mut set_of: HashMap<usize, usize> = HashMap::new();
    for i in 0..n {
        let r = root(&mut parent, i);
        let s = *set_of.entry(r).or_insert_with(|| {
            sets.push(Vec::new());
            sets.len() - 1
        });
        sets[s].push(i);
    }
    let mut set_edges: Vec<Vec<Edge>> = vec![Vec::new(); sets.len()];
    for &(a, b) in edges {
        set_edges[set_of[&root(&mut parent, a)]].push((a, b));
    }
    // Blocks: the sets of more than one and the groups first, then the units no line reaches, those
    // that grow last.
    // The width the map is packed across, from what its units cover: a set laid out in layers is cut
    // into bands no wider.
    let covered: f64 = sizes
        .iter()
        .map(|&(w, _, taken)| ((w + margin) * (taken + margin)) as f64)
        .sum();
    let pack = sizes
        .iter()
        .map(|s| s.0)
        .max()
        .unwrap_or(0)
        .max(4 * (CARD_COLUMNS + margin))
        .max((1.25 * covered.sqrt()).ceil() as i64);
    let mut blocks: Vec<Block> = Vec::new();
    let mut lone: Vec<usize> = Vec::new();
    let mut lone_growing: Vec<usize> = Vec::new();
    for (s, set) in sets.iter().enumerate() {
        if set.len() == 1 {
            let i = set[0];
            if matches!(units[i], Unit::Group(_)) {
                blocks.push((vec![(i, (0, 0))], sizes[i].0, sizes[i].2));
            } else if grows(units[i]) {
                lone_growing.push(i);
            } else {
                lone.push(i);
            }
            continue;
        }
        let placed = layers(set, &set_edges[s], sizes, margin, pack);
        let w = placed
            .iter()
            .map(|&(i, (x, _))| x + sizes[i].0)
            .max()
            .unwrap_or(0);
        let h = placed
            .iter()
            .map(|&(i, (_, y))| y + sizes[i].2)
            .max()
            .unwrap_or(0);
        blocks.push((placed, w, h));
    }
    for i in lone.into_iter().chain(lone_growing) {
        blocks.push((vec![(i, (0, 0))], sizes[i].0, sizes[i].2));
    }
    let widest = blocks.iter().map(|b| b.1).max().unwrap_or(0);
    let area: f64 = blocks
        .iter()
        .map(|b| ((b.1 + margin) * (b.2 + margin)) as f64)
        .sum();
    let width = widest.max(pack).max((1.25 * area.sqrt()).ceil() as i64);
    let spots = skyline(
        &blocks.iter().map(|b| (b.1, b.2)).collect::<Vec<_>>(),
        width,
        margin,
    );
    let mut out = vec![(0, 0); n];
    for ((placed, _, _), (bx, by)) in blocks.into_iter().zip(spots) {
        for (i, (x, y)) in placed {
            out[i] = (bx + x, by + y);
        }
    }
    out
}

/// Each block put where its top is highest, then leftmost, across `width` cells; a block keeps the
/// margin clear to its right and below it.
fn skyline(blocks: &[(i64, i64)], width: i64, margin: i64) -> Vec<(i64, i64)> {
    let cells = width.max(1) as usize;
    let mut sky = vec![0i64; cells];
    let mut out = Vec::with_capacity(blocks.len());
    for &(w, h) in blocks {
        let w = (w.max(1) as usize).min(cells);
        // The highest top over each run of `w` columns, kept as the run slides right.
        let mut best = (0usize, i64::MAX);
        let mut window: VecDeque<usize> = VecDeque::new();
        for x in 0..cells {
            while window.back().is_some_and(|&b| sky[b] <= sky[x]) {
                window.pop_back();
            }
            window.push_back(x);
            if let Some(&front) = window.front()
                && front + w <= x
            {
                window.pop_front();
            }
            if x + 1 >= w {
                let top = sky[*window.front().unwrap_or(&x)];
                if top < best.1 {
                    best = (x + 1 - w, top);
                }
            }
        }
        let (x, top) = best;
        // The margin's columns to its right may lie under a taller block beside it, which stays.
        let reach = (x + w + margin as usize).min(cells);
        for cell in &mut sky[x..reach] {
            *cell = (*cell).max(top + h + margin);
        }
        out.push((x as i64, top));
    }
    out
}

/// One set's units laid out in layers from left to right: rings broken in their order, each unit
/// a layer right of the furthest it comes after and pulled right up to what comes after it, the
/// layers reordered by their neighbours, and each stacked top down, wrapped into another column
/// where it grows taller than the set is wide; the layers cut into bands `width` wide at the most.
fn layers(
    set: &[usize],
    edges: &[Edge],
    sizes: &[(i64, i64, i64)],
    margin: i64,
    width: i64,
) -> Vec<(usize, (i64, i64))> {
    let m = set.len();
    let local: HashMap<usize, usize> = set.iter().enumerate().map(|(i, &u)| (u, i)).collect();
    let mut outs: Vec<Vec<usize>> = vec![Vec::new(); m];
    for &(a, b) in edges {
        outs[local[&a]].push(local[&b]);
    }
    for list in &mut outs {
        list.sort_unstable();
        list.dedup();
    }
    // Rings broken: a line back to a unit still being walked is turned round.
    let mut state = vec![0u8; m];
    let mut back: HashSet<(usize, usize)> = HashSet::new();
    for start in 0..m {
        if state[start] != 0 {
            continue;
        }
        state[start] = 1;
        let mut stack: Vec<(usize, usize)> = vec![(start, 0)];
        while let Some(top) = stack.last_mut() {
            let (node, next) = *top;
            if next < outs[node].len() {
                top.1 += 1;
                let to = outs[node][next];
                match state[to] {
                    0 => {
                        state[to] = 1;
                        stack.push((to, 0));
                    }
                    1 => {
                        back.insert((node, to));
                    }
                    _ => {}
                }
            } else {
                state[node] = 2;
                stack.pop();
            }
        }
    }
    let mut dag: Vec<Edge> = Vec::new();
    for (a, list) in outs.iter().enumerate() {
        for &b in list {
            dag.push(if back.contains(&(a, b)) {
                (b, a)
            } else {
                (a, b)
            });
        }
    }
    dag.sort_unstable();
    dag.dedup();
    let mut after: Vec<Vec<usize>> = vec![Vec::new(); m];
    let mut before: Vec<Vec<usize>> = vec![Vec::new(); m];
    for &(a, b) in &dag {
        after[a].push(b);
        before[b].push(a);
    }
    // Layers: the longest way in, then each pulled up to just before the nearest it leads to.
    let mut waiting: Vec<usize> = before.iter().map(Vec::len).collect();
    let mut ready: VecDeque<usize> = (0..m).filter(|&i| waiting[i] == 0).collect();
    let mut layer = vec![0usize; m];
    let mut settled: Vec<usize> = Vec::with_capacity(m);
    while let Some(i) = ready.pop_front() {
        settled.push(i);
        layer[i] = before[i].iter().map(|&p| layer[p] + 1).max().unwrap_or(0);
        for &next in &after[i] {
            waiting[next] -= 1;
            if waiting[next] == 0 {
                ready.push_back(next);
            }
        }
    }
    for &i in settled.iter().rev() {
        if let Some(nearest) = after[i].iter().map(|&b| layer[b]).min()
            && nearest > layer[i] + 1
        {
            layer[i] = nearest - 1;
        }
    }
    let count = layer.iter().max().map_or(0, |l| l + 1);
    let mut columns: Vec<Vec<usize>> = vec![Vec::new(); count];
    for i in 0..m {
        columns[layer[i]].push(i);
    }
    // Reordered by the middle of their neighbours in the layer beside, sweeping each way.
    let neighbours: Vec<Vec<usize>> = (0..m)
        .map(|i| after[i].iter().chain(&before[i]).copied().collect())
        .collect();
    let mut place = vec![0usize; m];
    let mark = |column: &[usize], place: &mut [usize]| {
        for (j, &i) in column.iter().enumerate() {
            place[i] = j;
        }
    };
    for column in &columns {
        mark(column, &mut place);
    }
    let reorder = |c: usize, beside: usize, columns: &mut [Vec<usize>], place: &mut [usize]| {
        let mut keyed: Vec<(f64, usize, usize)> = columns[c]
            .iter()
            .enumerate()
            .map(|(j, &i)| {
                let near: Vec<usize> = neighbours[i]
                    .iter()
                    .filter(|&&o| layer[o] == beside)
                    .map(|&o| place[o])
                    .collect();
                let key = if near.is_empty() {
                    j as f64
                } else {
                    near.iter().sum::<usize>() as f64 / near.len() as f64
                };
                (key, j, i)
            })
            .collect();
        keyed.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        columns[c] = keyed.into_iter().map(|(_, _, i)| i).collect();
        mark(&columns[c], place);
    };
    for _ in 0..SWEEPS {
        for c in 1..count {
            reorder(c, c - 1, &mut columns, &mut place);
        }
        for c in (0..count.saturating_sub(1)).rev() {
            reorder(c, c + 1, &mut columns, &mut place);
        }
    }
    // Stacked top down, a column wrapped where it would grow taller than the set is wide.
    let tallest = set.iter().map(|&u| sizes[u].2).max().unwrap_or(0);
    let area: f64 = set
        .iter()
        .map(|&u| (sizes[u].0 * (sizes[u].2 + margin)) as f64)
        .sum();
    let limit = tallest.max((1.1 * area.sqrt()).ceil() as i64);
    let gap = LANE.max(margin);
    // Each layer from its own top-left, with its width and the height it takes.
    let mut stacked: Vec<Block> = Vec::with_capacity(count);
    for column in &columns {
        let (mut left, mut y, mut widest, mut deepest) = (0, 0, 0, 0);
        let mut at = Vec::with_capacity(column.len());
        for &i in column {
            let u = set[i];
            let (w, _, taken) = sizes[u];
            if y > 0 && y + taken > limit {
                left += widest + margin + 1;
                y = 0;
                widest = 0;
            }
            at.push((u, (left, y)));
            deepest = deepest.max(y + taken);
            y += taken + margin;
            widest = widest.max(w);
        }
        stacked.push((at, left + widest, deepest));
    }
    // The layers left to right, cut into bands no wider than `width` (a layer wider stays alone),
    // the bands stacked top down: a set is read along a band, then from the next one's left, and is
    // no more a long strip than a long pillar.
    let mut out = Vec::with_capacity(m);
    let (mut x, mut top, mut band) = (0, 0, 0);
    for (at, w, h) in stacked {
        if x > 0 && x + w > width {
            top += band + gap;
            x = 0;
            band = 0;
        }
        out.extend(at.into_iter().map(|(u, (dx, dy))| (u, (x + dx, top + dy))));
        band = band.max(h);
        x += w + gap;
    }
    out
}

/// What arranging moves on a map of `boxes` and `groups` along `edges`, `margin` cells kept round
/// each box and room kept under a card that grows down to `room` cells.
pub(crate) fn plan(
    boxes: &[OnMap],
    groups: &[Group],
    edges: &[Edge],
    margin: i64,
    room: i64,
    over: Over,
) -> Plan {
    let (box_in, group_in) = super::tidy::nesting(boxes, groups);
    let mut sheet = Sheet {
        boxes,
        groups,
        edges: edges.to_vec(),
        margin,
        room,
        box_in,
        group_in,
        first: vec![None; groups.len()],
        inside: vec![Vec::new(); groups.len()],
        fitted: groups.iter().map(|g| (g.size, g.size.1)).collect(),
        at: boxes.iter().map(|b| b.at).collect(),
        frames: groups.iter().map(|g| (g.at, g.size)).collect(),
    };
    for k in (0..boxes.len()).filter(|&k| boxes[k].drawn) {
        let mut at = sheet.box_in[k];
        for _ in 0..=groups.len() {
            let Some(g) = at else { break };
            if sheet.first[g].is_none() {
                sheet.first[g] = Some(k);
            }
            at = sheet.group_in[g];
        }
    }
    match over {
        Over::Map(at) => {
            for g in sheet.innermost_first(None) {
                sheet.fit(g);
            }
            let laid = sheet.lay(None);
            let anchor = at.unwrap_or_else(|| {
                let corners = laid.at.iter().map(|&(u, _)| match u {
                    Unit::Box(k) => boxes[k].at,
                    Unit::Group(g) => groups[g].at,
                });
                corners
                    .reduce(|a, b| (a.0.min(b.0), a.1.min(b.1)))
                    .unwrap_or((0, 0))
            });
            for (unit, (x, y)) in laid.at {
                sheet.put(unit, (anchor.0 + x, anchor.1 + y));
            }
        }
        Over::Group(g) => {
            for inner in sheet.innermost_first(Some(g)) {
                sheet.fit(inner);
            }
            sheet.put(Unit::Group(g), groups[g].at);
            sheet.push(sheet.group_in[g], Unit::Group(g));
        }
    }
    // What is not drawn goes with the group it is in.
    for k in (0..boxes.len()).filter(|&k| !boxes[k].drawn) {
        if let Some(g) = sheet.box_in[k] {
            let (from, to) = (groups[g].at, sheet.frames[g].0);
            sheet.at[k] = (boxes[k].at.0 + to.0 - from.0, boxes[k].at.1 + to.1 - from.1);
        }
    }
    Plan {
        boxes: (0..boxes.len())
            .filter(|&k| sheet.at[k] != boxes[k].at)
            .map(|k| (k, sheet.at[k]))
            .collect(),
        groups: (0..groups.len())
            .filter(|&g| sheet.frames[g] != (groups[g].at, groups[g].size))
            .map(|g| -> Framed { (g, sheet.frames[g].0, sheet.frames[g].1) })
            .collect(),
    }
}

/// The lines of `section`'s map as edges between its boxes: each line between the boxes drawn at
/// its ends, the box a data reference points at on the left, the box any other line reaches on
/// the right; once for a pair.
fn edges_of(
    project: &crate::export::Project,
    section: &str,
    boxes: &[OnMap],
    lines: &super::surround::Lines,
) -> Vec<Edge> {
    use super::surround::End;
    let placed_here: HashSet<String> = boxes
        .iter()
        .filter(|b| !b.view.is_empty() && b.kind == "card")
        .map(|b| b.item.clone())
        .collect();
    let index: HashMap<(&str, &str), usize> = boxes
        .iter()
        .enumerate()
        .filter(|(_, b)| b.drawn)
        .map(|(i, b)| ((b.kind, b.item.as_str()), i))
        .collect();
    let of = |end: &End| match end {
        End::Card(id) => index.get(&("card", id.as_str())).copied(),
        End::Section(id) => index.get(&("section", id.as_str())).copied(),
    };
    let mut out: Vec<Edge> = Vec::new();
    for (from, to, line) in super::surround::ends(project, section, &placed_here, lines) {
        let (Some(a), Some(b)) = (of(&from), of(&to)) else {
            continue;
        };
        if a == b {
            continue;
        }
        let points = project
            .defs
            .line(&line)
            .is_some_and(|d| matches!(d.base, Base::Embed | Base::Designate));
        out.push(if points { (b, a) } else { (a, b) });
    }
    out.sort_unstable();
    out.dedup();
    out
}

/// `arrange` with `by: "lines"`: the whole map (from `at`, or where the top left of its boxes is),
/// or what the `group` named holds, laid out by `plan` and written in the call's one transaction,
/// each box moved marked as put by hand. Where each went, as `arranged` says it (a group's `size`
/// when its frame changed). Refused in a ui section and a working directory.
pub(crate) fn by_lines(
    tx: &mut Tx,
    scope: &Scope,
    section: &str,
    spec: &Map<String, Value>,
) -> Result<Map<String, Value>, String> {
    let said: Vec<&str> = ["cards", "region", "columns"]
        .into_iter()
        .filter(|k| spec.contains_key(*k))
        .collect();
    if !said.is_empty() {
        return Err(text!("boxes.arrange.linesKeys", keys = said.join(", ")));
    }
    super::boxes::only_keys(spec, "arrange", &["by", "group", "at"])?;
    if spec.contains_key("group") && spec.contains_key("at") {
        return Err(text!("boxes.arrange.groupWithAt"));
    }
    let at = spec
        .get("at")
        .map(|v| super::update::grid_point(v).map_err(|_| text!("boxes.arrange.atShape")))
        .transpose()?;
    let (boxes, groups, plan) = {
        let seen = Seen::in_tx(tx, scope)?;
        let project = seen.project();
        if !super::tidy::tidied_here(&project, section) {
            return Err(text!(
                "boxes.arrange.notHere",
                path = project.directory.full_section(section)
            ));
        }
        let layout = Layout::read(tx.conn(), &scope.project)?;
        let groups = super::groups::on_view(tx.conn(), &scope.project, &scope.tree, section)?;
        let over = match spec.get("group") {
            None => Over::Map(at),
            Some(raw) => {
                let name = raw.as_str().unwrap_or_default().trim();
                let found = groups
                    .iter()
                    .position(|g| !name.is_empty() && super::groups::same(&g.name, name));
                match found {
                    Some(g) => Over::Group(g),
                    None => {
                        let names: Vec<&str> = groups.iter().map(|g| g.name.as_str()).collect();
                        return Err(text!(
                            "boxes.arrange.noGroup",
                            name = raw.as_str().map_or_else(|| raw.to_string(), str::to_string),
                            names = if names.is_empty() {
                                "-".to_string()
                            } else {
                                names.join(", ")
                            }
                        ));
                    }
                }
            }
        };
        let lines = super::surround::Lines::default();
        let boxes = super::boxes::on_map_with(&tx.reader(), &seen, &layout, section, &lines)?;
        let edges = edges_of(&project, section, &boxes, &lines);
        let plan = plan(
            &boxes,
            &groups,
            &edges,
            layout.margin,
            rows_for_rows(layout.rows),
            over,
        );
        (boxes, groups, plan)
    };
    let mut arranged = Vec::new();
    for &(k, at) in &plan.boxes {
        super::groups::put_box(tx, scope, &boxes[k], at)?;
        let mut row = Map::new();
        row.insert(boxes[k].key.into(), boxes[k].path.clone().into());
        row.insert("topLeft".into(), point(at.0, at.1));
        arranged.push(Value::Object(row));
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
        arranged.push(Value::Object(row));
    }
    let mut out = Map::new();
    out.insert("arranged".into(), Value::Array(arranged));
    Ok(out)
}

/// Where `plan` puts boxes of the sizes given, joined by lines each from a card to the card one of
/// its fields points at, on a map with no groups: for the tests that hold what arranging costs.
pub(crate) fn pointed(
    sizes: &[(i64, i64)],
    pointers: &[(usize, usize)],
    margin: i64,
) -> Vec<(i64, i64)> {
    let boxes: Vec<OnMap> = sizes
        .iter()
        .enumerate()
        .map(|(i, &size)| OnMap {
            path: i.to_string(),
            at: (0, 0),
            size,
            drawn: true,
            grows: false,
            view: String::new(),
            kind: "card",
            item: i.to_string(),
            key: "card",
        })
        .collect();
    let mut edges: Vec<Edge> = pointers
        .iter()
        .filter(|(a, b)| a != b)
        .map(|&(a, b)| (b, a))
        .collect();
    edges.sort_unstable();
    edges.dedup();
    let plan = plan(&boxes, &[], &edges, margin, 0, Over::Map(Some((0, 0))));
    let mut out = vec![(0, 0); sizes.len()];
    for (k, at) in plan.boxes {
        out[k] = at;
    }
    out
}
