//! The project's card size and where cards sit on the grid (§12, `LayoutSettings`,
//! `CardSize`, `Grid` and the card heights of `CardGeometry`): how many grid rows a card
//! takes as it first shows — what an agent is told, so it can place cards that do not
//! overlap — and the project's layout as `project_read` and `project_update` say it.

use crate::kinds::{Face, Kinds};
use crate::model::{Card, Field};
use rusqlite::{Connection, OptionalExtension};
use serde_json::{Map, Value};

/// A card's columns: the project's ten.
pub(crate) const CARD_COLUMNS: i64 = 10;
/// A card's row, in points.
const UNIT_HEIGHT: f64 = 27.0;
/// A card's frame around its rows at its tallest: border, header, padding, the strips.
const FRAME: f64 = 2.0 + 40.0 + 12.0 + 2.0 * MORE_HEIGHT;
const CARD_BORDER: f64 = 1.0;
const HEADER_HEIGHT: f64 = 40.0;
const LIST_PADDING: f64 = 12.0;
/// The strip that says rows are out of sight: one row of the grid, so a
/// card with one ends on a grid line as one without does.
const MORE_HEIGHT: f64 = 27.0;
const ROW_HEIGHT: f64 = 27.0;
/// A note shows this many lines at the most.
const NOTE_LINES: usize = 200;

/// The project's layout: how many rows a card shows, how wide it is, the margin around it,
/// and how a formatdoc is drawn by default.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Layout {
    pub rows: i64,
    /// None for `auto`.
    pub width: Option<i64>,
    pub margin: i64,
    pub doc_view: String,
}

impl Default for Layout {
    fn default() -> Self {
        Layout {
            rows: 15,
            width: Some(352),
            margin: 1,
            doc_view: "document".to_string(),
        }
    }
}

impl Layout {
    /// The project's, as stored (`defaultSettings`).
    pub fn read(conn: &Connection, project: &str) -> Result<Layout, String> {
        type Stored = (Option<i64>, Option<String>, Option<i64>, Option<String>);
        let row: Option<Stored> = conn
            .query_row(
                "SELECT rows, width, grid_gap, doc_view FROM default_view WHERE project_id = ?",
                [project],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .optional()
            .map_err(crate::store::sql)?;
        let mut out = Layout::default();
        if let Some((rows, width, margin, view)) = row {
            if let Some(r) = rows {
                out.rows = r;
            }
            if let Some(w) = width {
                out.width = if w == "auto" {
                    None
                } else {
                    Some(w.parse().unwrap_or(352))
                };
            }
            if let Some(m) = margin {
                out.margin = m;
            }
            if let Some(v) = view.filter(|v| v == "rows" || v == "document") {
                out.doc_view = v;
            }
        }
        Ok(out)
    }

    /// As `project_read` says it (`layoutView`).
    pub fn view(&self) -> Value {
        let mut out = Map::new();
        out.insert("cardRows".into(), self.rows.into());
        out.insert(
            "cardWidth".into(),
            self.width.map_or_else(|| Value::from("auto"), Value::from),
        );
        out.insert("margin".into(), self.margin.into());
        out.insert("docView".into(), self.doc_view.clone().into());
        Value::Object(out)
    }
}

/// How many points one grid unit is across and down, as the screen draws a section's diagram
/// (`Grid.unitWidth`, `Grid.unitHeight`): a tenth of the card's width — 352 points when the project
/// sizes every card to the widest (`auto`), which only the screen can see — and one card row. A
/// mock is laid out in the same units, a point a CSS pixel, so it lines up with the map.
pub(crate) fn unit_points(layout: &Layout) -> (f64, f64) {
    let width = layout.width.map_or(CARD_WIDTH, |w| w as f64);
    (width / CARD_COLUMNS as f64, UNIT_HEIGHT)
}

/// Units down for a card `points` tall.
fn rows_for_height(points: f64) -> i64 {
    (points / UNIT_HEIGHT).ceil() as i64
}

/// Units down for a card showing `rows` rows at the most.
pub(crate) fn rows_for_rows(rows: i64) -> i64 {
    rows_for_height(FRAME + rows as f64 * UNIT_HEIGHT)
}

/// Whether a card is drawn as a document rather than rows: a freedoc and a stackdoc
/// always — a table too, as the table it builds (`drawnAsDocument`) —, a formatdoc unless it, or else the project,
/// says rows.
fn drawn_as_document(card: &Card, doc_view: &str) -> bool {
    match card.kind.as_str() {
        "freedoc" | "stackdoc" | crate::kinds::TABLE => true,
        "formatdoc" => card.doc_view.as_deref().unwrap_or(doc_view) == "document",
        _ => false,
    }
}

/// A size of a card's own: its width, and its height when one is written.
pub(crate) type OwnSize = (i64, Option<i64>);

/// The size written for a card, when its kind may carry one (`Kinds::sizable`).
pub(crate) fn own_size(card: &Card, kinds: &Kinds) -> Option<OwnSize> {
    card.size
        .filter(|_| kinds.sizable(&card.kind))
        .map(|s| (s.w, s.h))
}

/// A size as the answers and the screen say it: `{w}`, or `{w, h}` when a height is written.
pub(crate) fn size_value((w, h): OwnSize) -> Value {
    let mut out = Map::new();
    out.insert("w".into(), w.into());
    if let Some(h) = h {
        out.insert("h".into(), h.into());
    }
    Value::Object(out)
}

/// A card's width in grid units: its own when it has one; the project's ten otherwise.
pub(crate) fn card_columns(card: &Card, kinds: &Kinds) -> i64 {
    own_size(card, kinds).map_or(CARD_COLUMNS, |s| s.0)
}

/// How wide in points a card `w` grid units wide is drawn; the project's card width when it has
/// no width of its own.
pub(crate) fn width_points(layout: &Layout, w: Option<i64>) -> f64 {
    match w {
        Some(w) => w as f64 * unit_points(layout).0,
        None => layout.width.map_or(CARD_WIDTH, |w| w as f64),
    }
}

/// What the field a face draws as Markdown has to draw: none when nothing is written there.
fn face_text<'a>(card: &'a Card, face: &Face) -> Option<&'a str> {
    let name = face.markdown.as_ref()?;
    card.form
        .as_ref()
        .and_then(|f| f["fields"].get(name.as_str()))
        .and_then(Value::as_str)
        .filter(|t| !t.trim().is_empty())
}

/// A card's width in points as the project draws it: its own, or 352 when the project sizes every
/// card to the widest (`auto`), which only the screen can see.
const CARD_WIDTH: f64 = 352.0;

/// How many grid rows a card of a kind with a face takes as the screen draws it, when it has no height of its own: a row each of the fields above, and, when its
/// Markdown has anything to draw, as tall as the page of it is drawn under them at the card's
/// width `width` in points (its height estimated from the text, `markdown_height`) — the project's
/// whole card at the most, a longer page scrolled in the card. The screen draws it as tall as
/// this says (`faceRows` in `project_detail`). One drawing a page of its own (SVG, HTML) is as
/// tall as a document card: its text is no measure of the picture or the page it makes. A frame
/// is too: it is a room for another section's mock, whose size its text does not say either.
pub(crate) fn face_rows(card: &Card, face: &Face, layout: &Layout, width: f64) -> i64 {
    if face.page.is_some() || card.kind == crate::kinds::FRAME {
        return rows_for_rows(layout.rows);
    }
    let above = rows_for_count(face.top.len(), layout);
    let Some(text) = face_text(card, face) else {
        return above;
    };
    // The page under the fields above, as the screen puts it (`CardRenderer.documentArea`):
    // 8 points below them, 12 in from either side, 14 above the card's foot.
    let top =
        CARD_BORDER + HEADER_HEIGHT + LIST_PADDING / 2.0 + face.top.len() as f64 * ROW_HEIGHT + 8.0;
    let page = super::markdown_height::page_height(text, width - 24.0);
    rows_for_height(top + page + 14.0)
        .max(above)
        .min(rows_for_rows(layout.rows))
}

/// One box of a section's own diagram: a card of the section's or one of its card ghosts.
pub(crate) struct OwnBox {
    pub id: String,
    pub path: String,
    /// Where it sits; none when no one placed it.
    pub at: Option<(i64, i64)>,
    pub size: (i64, i64),
    /// Drawn on the diagram as a card of its own: not a value object or an enum folded into the
    /// cards holding it (`folded`), whose box blocks nothing.
    pub drawn: bool,
}

/// The value objects and enums of `section` its diagram draws folded into the cards that hold
/// them, not as cards, as the screen
/// decides it (`withoutValueCards`): one an embedding reaches from another kind of card of the
/// section — through others it holds too — and not shown on its own (`standalone`). One that no
/// card of the section holds keeps a card: of a ring of them, the first nothing else holds.
pub(crate) fn folded(
    project: &crate::export::Project,
    section: &str,
) -> std::collections::HashSet<String> {
    use std::collections::{HashMap, HashSet};
    let members: Vec<&Card> = project
        .directory
        .members(section)
        .iter()
        .map(|&i| project.nth(i))
        .collect();
    let here: HashSet<&str> = members.iter().map(|c| c.id.as_str()).collect();
    let is_type = |c: &Card| c.kind == "value" || c.kind == "enum";
    let embeds = |line: &str| {
        project
            .defs
            .line(line)
            .is_some_and(|d| d.base == crate::kinds::Base::Embed)
    };
    let mut holds: HashMap<&str, Vec<String>> = HashMap::new();
    for card in &members {
        for line in card.connections.iter().filter(|l| embeds(&l.line)) {
            if let Some(to) = line.to.as_deref().and_then(|t| project.card(t))
                && here.contains(to.id.as_str())
            {
                holds
                    .entry(card.id.as_str())
                    .or_default()
                    .push(to.id.clone());
            }
        }
    }
    let mut reached: HashSet<String> = HashSet::new();
    fn reach(id: &str, holds: &HashMap<&str, Vec<String>>, reached: &mut HashSet<String>) {
        for held in holds.get(id).into_iter().flatten() {
            if reached.insert(held.clone()) {
                reach(held, holds, reached);
            }
        }
    }
    // What keeps a card: every other kind's, then of the value objects and enums no card holds,
    // one at a time, the first nothing still waiting holds.
    let mut kept: HashSet<String> = HashSet::new();
    for card in members.iter().filter(|c| !is_type(c)) {
        kept.insert(card.id.clone());
        reach(&card.id, &holds, &mut reached);
    }
    let mut waiting: Vec<&Card> = members
        .iter()
        .copied()
        .filter(|c| is_type(c) && !reached.contains(&c.id))
        .collect();
    while !waiting.is_empty() {
        let held_by_waiting: HashSet<&String> = waiting
            .iter()
            .flat_map(|c| holds.get(c.id.as_str()).into_iter().flatten())
            .collect();
        let next = waiting
            .iter()
            .find(|c| !held_by_waiting.contains(&c.id))
            .copied()
            .unwrap_or(waiting[0]);
        kept.insert(next.id.clone());
        reached.insert(next.id.clone());
        reach(&next.id, &holds, &mut reached);
        waiting.retain(|c| !reached.contains(&c.id));
    }
    members
        .iter()
        .filter(|c| is_type(c) && !kept.contains(&c.id) && !c.standalone)
        .map(|c| c.id.clone())
        .collect()
}

/// The boxes of a section's own diagram (`view` `""`): its cards, then its card ghosts.
pub(crate) fn own_boxes(
    project: &crate::export::Project,
    layout: &Layout,
    section: &str,
) -> Vec<OwnBox> {
    let at = |x: Option<f64>, y: Option<f64>| Some((x?.round() as i64, y?.round() as i64));
    let folded = folded(project, section);
    let mut out: Vec<OwnBox> = project
        .directory
        .members(section)
        .iter()
        .map(|&i| project.nth(i))
        .map(|c| OwnBox {
            id: c.id.clone(),
            path: project.full(c),
            at: at(c.pos_x, c.pos_y),
            size: (
                card_columns(c, project.defs.cards()),
                card_rows(c, project, layout),
            ),
            drawn: !folded.contains(&c.id),
        })
        .collect();
    out.extend(project.ghosts_in(section).into_iter().map(|g| OwnBox {
        id: g.id.clone(),
        path: project.full_ghost(g),
        at: at(g.pos_x, g.pos_y),
        size: ghost_size(project, g, layout),
        drawn: true,
    }));
    out
}

/// How big a card ghost's box is: the card it leads to drawn whole there; one that leads
/// nowhere, a card with no rows.
pub(crate) fn ghost_size(
    project: &crate::export::Project,
    ghost: &crate::model::Ghost,
    layout: &Layout,
) -> (i64, i64) {
    let kinds = project.defs.cards();
    match project.ghost_card(ghost) {
        Some(card) => (card_columns(card, kinds), card_rows(card, project, layout)),
        None => (CARD_COLUMNS, rows_for_count(0, layout)),
    }
}

/// How many grid rows a card takes as it first shows, at `layout.rows` rows at the most — a
/// container's, with a row for each card inside it as its kind's `as_row` says (`inside_rows`).
pub(crate) fn card_rows(card: &Card, project: &crate::export::Project, layout: &Layout) -> i64 {
    rows_sized(card, project, layout, own_size(card, project.defs.cards()))
}

/// How many grid rows a card takes when its own size is `own`: its height when one is written;
/// otherwise as its contents say, a face's Markdown estimated at its own width.
pub(crate) fn rows_sized(
    card: &Card,
    project: &crate::export::Project,
    layout: &Layout,
    own: Option<OwnSize>,
) -> i64 {
    let kinds = project.defs.cards();
    if let Some((_, Some(h))) = own {
        return h;
    }
    if let Some(face) = kinds.get(&card.kind).and_then(|k| k.face.as_ref()) {
        return face_rows(card, face, layout, width_points(layout, own.map(|s| s.0)));
    }
    if drawn_as_document(card, &layout.doc_view) {
        return rows_for_rows(layout.rows);
    }
    if let Some(kind) = kinds.get(&card.kind).filter(|k| !k.builtin) {
        let inside = if kind.holds.is_empty() {
            0
        } else {
            inside_rows(project, card, 0)
        };
        return rows_for_count(own_row_count(card, kind) + inside, layout);
    }
    if card.kind == "goal"
        && let Some(kind) = kinds.get("goal")
    {
        return rows_for_count(
            goal_row_count(card, kind) + inside_rows(project, card, 0),
            layout,
        );
    }
    if card.kind == crate::kinds::INSTANCE {
        return rows_for_count(instance_rows(card, project), layout);
    }
    rows_for_count(row_count(card), layout)
}

/// The rows a container draws for the cards inside it, as each one's kind's `as_row` says it
/// (`ContainerRows` on screen): a
/// row a card, a row of buttons for each choice it names but the one that says done, and — where
/// its kind unfolds it — what it holds, or its fields written, under it. The ones done are one
/// row 「済み」, folded as it first shows:
/// the cards inside it, not what one of them unfolds.
fn inside_rows(project: &crate::export::Project, container: &Card, depth: usize) -> usize {
    use crate::kinds::FieldType;
    let kinds = project.defs.cards();
    let mut finished = false;
    let open: usize = project
        .inner_of(&container.id)
        .into_iter()
        .map(|card| {
            let Some(kind) = kinds.get(&card.kind) else {
                return 1;
            };
            let named: Vec<&crate::kinds::Field> = match &kind.as_row {
                Some(Value::Array(names)) => names
                    .iter()
                    .filter_map(Value::as_str)
                    .filter_map(|n| kind.field(n))
                    .collect(),
                _ => Vec::new(),
            };
            let check = named.iter().find(|f| {
                !f.done.is_empty() && matches!(f.fieldtype, FieldType::Checkbox | FieldType::Radio)
            });
            if depth == 0
                && check.is_some_and(|f| {
                    card.form
                        .as_ref()
                        .and_then(|form| form["fields"].get(&f.name))
                        .is_some_and(|v| f.done.contains(v))
                })
            {
                finished = true;
                return 0;
            }
            let choices = named
                .iter()
                .filter(|f| f.name != "label" && check.is_none_or(|c| c.name != f.name))
                .filter(|f| f.fieldtype == FieldType::Radio)
                .count();
            let unfolds = matches!(&kind.as_row, Some(Value::String(s)) if s == "unfold");
            let under = if unfolds && depth < 5 {
                if project.inner_of(&card.id).is_empty() {
                    let fields = card.form.as_ref().map(|f| &f["fields"]);
                    kind.fields
                        .iter()
                        .filter(|f| !BASE_FIELDS.contains(&f.name.as_str()))
                        .filter(|f| {
                            fields
                                .and_then(|v| v.get(&f.name))
                                .is_some_and(|v| match v {
                                    Value::Null => false,
                                    Value::String(s) => !s.is_empty(),
                                    Value::Bool(b) => *b,
                                    _ => true,
                                })
                        })
                        .count()
                } else {
                    inside_rows(project, card, depth + 1)
                }
            } else {
                0
            };
            1 + choices + under
        })
        .sum();
    open + usize::from(finished)
}

/// How many rows a goal shows, as the screen draws it (`cardRows`, `CustomKind.goal`): a row for
/// each choice — its state and its priority, written or not, the state too though the header
/// says it — one for each other field it holds something in (期限・作業する branch・MR), but what is
/// written only in its editor (its body) — the cards inside it counted apart (`inside_rows`;
/// counted short, the cards an AI placed sat closer than the project's margin on screen).
pub(super) fn goal_row_count(card: &Card, kind: &crate::kinds::Kind) -> usize {
    use crate::kinds::FieldType;
    let fields = card.form.as_ref().map(|f| &f["fields"]);
    let written = |name: &str| {
        fields.and_then(|f| f.get(name)).is_some_and(|v| match v {
            Value::Null => false,
            Value::String(s) => !s.is_empty(),
            Value::Array(a) => !a.is_empty(),
            Value::Object(o) => !o.is_empty(),
            _ => true,
        })
    };
    kind.fields
        .iter()
        .filter(|f| f.show.as_deref() != Some("editor") && !f.look)
        .filter(|f| !BASE_FIELDS.contains(&f.name.as_str()))
        .filter(|f| f.fieldtype == FieldType::Radio || written(&f.name))
        .count()
}

/// The fields every kind takes from `base` (表示名・説明・タグ・メタデータ): said in the card's own
/// places, never a row of it (`CardKind.ownFields`).
const BASE_FIELDS: [&str; 4] = ["label", "description", "tags", "metadata"];

/// How many rows a card of a kind the project defines shows, as the screen draws it (`cardRows`):
/// its own fields (not those every kind takes from `base`, its 表示名
/// included) that are neither in its header nor only in its editor — a choice and a check
/// always, any other when it holds something — then its rows, every one at every depth.
fn own_row_count(card: &Card, kind: &crate::kinds::Kind) -> usize {
    use crate::kinds::FieldType;
    let Some(form) = card.form.as_ref() else {
        return 0;
    };
    let written = |name: &str| {
        form["fields"].get(name).is_some_and(|v| match v {
            Value::Null => false,
            Value::String(s) => !s.is_empty(),
            Value::Array(a) => !a.is_empty(),
            Value::Object(o) => !o.is_empty(),
            _ => true,
        })
    };
    let fields = kind
        .fields
        .iter()
        .filter(|f| !BASE_FIELDS.contains(&f.name.as_str()))
        .filter(|f| {
            !kind.header.contains(&f.name) && f.show.as_deref() != Some("editor") && !f.look
        })
        .filter(|f| {
            matches!(f.fieldtype, FieldType::Radio | FieldType::Checkbox) || written(&f.name)
        })
        .count();
    fn rows(by_kind: &Value) -> usize {
        by_kind.as_object().map_or(0, |kinds| {
            kinds
                .values()
                .filter_map(Value::as_array)
                .flatten()
                .map(|row| 1 + rows(&row["rows"]))
                .sum()
        })
    }
    fields + rows(&form["rows"])
}

/// How many grid rows a card of `count` rows takes, at `layout.rows` rows at the most.
pub(crate) fn rows_for_count(count: usize, layout: &Layout) -> i64 {
    let count = count as f64;
    let fit = layout.rows.max(1) as f64;
    let shown = count.min(fit);
    let below = if count > fit { 1.0 } else { 0.0 };
    let height =
        CARD_BORDER * 2.0 + HEADER_HEIGHT + LIST_PADDING + shown * ROW_HEIGHT + below * MORE_HEIGHT;
    rows_for_height(height)
}

/// How many rows a card has as it first shows (`cardRows(…, inlineReuse: true)` with
/// its resting folds: every value type and enum it holds folded).
fn row_count(card: &Card) -> usize {
    match card.kind.as_str() {
        "enum" => card.values.len(),
        "flow" | "store" => {
            let mut n = 0;
            for node in &card.nodes {
                n += 1;
                if node.kind == "process" {
                    n += node.steps.len();
                }
            }
            n + card.holds.len()
        }
        "formatdoc" => {
            fn items(rows: &[crate::model::FormatRow]) -> usize {
                rows.iter()
                    .filter(|r| r.kind == "item")
                    .map(|r| 1 + items(&r.rows))
                    .sum()
            }
            usize::from(top_points(card)) + items(&card.rows)
        }
        "note" => card
            .body
            .split('\n')
            .filter(|l| !l.trim().is_empty())
            .count()
            .min(NOTE_LINES + 1),
        "entity" | "value" => fields(&card.fields),
        _ => 0,
    }
}

/// How deep a data card's rows unfold what its fields hold (`InstanceRows.maxDepth`).
const INSTANCE_DEPTH: usize = 6;

/// The fields an element of a data card's field has, and the shape it embeds
/// (`InstanceRows.element`): an object's own, or what a `ref` embeds when that is an entity or a
/// value.
fn element<'a>(
    project: &crate::export::Project<'a>,
    field: &'a Field,
) -> (&'a [Field], Option<&'a str>) {
    if field.is_object() {
        return (&field.children, None);
    }
    match field
        .ref_schema_id
        .as_deref()
        .and_then(|id| project.card(id))
        .filter(|t| field.is_ref() && (t.kind == "entity" || t.kind == "value"))
    {
        Some(target) => (&target.fields, Some(target.id.as_str())),
        None => (&[], None),
    }
}

/// How many rows a data card draws as it first shows (`InstanceRows.rows`, nothing folded): a row each of its shape's
/// fields, nested as the shape is — what an object or an embedded shape holds under it, an
/// array's elements each a row under it with their fields — a branch's fields only where written.
/// None when its shape is gone.
fn instance_rows(card: &Card, project: &crate::export::Project) -> usize {
    use std::collections::HashSet;
    let Some(shape) = super::instances::shape(project, card) else {
        return 0;
    };
    fn count(
        project: &crate::export::Project,
        fields: &[Field],
        values: Option<&Map<String, Value>>,
        depth: usize,
        seen: &HashSet<String>,
    ) -> usize {
        let mut n = 0;
        for field in fields {
            let value = values.and_then(|v| v.get(&field.name));
            if !field.variants.is_empty() && value.is_none() {
                continue;
            }
            n += 1;
            let (inner, embedded) = element(project, field);
            let unseen = embedded.is_none_or(|e| !seen.contains(e));
            let mut seen = seen.clone();
            seen.extend(embedded.map(str::to_string));
            if field.is_array {
                let items = value
                    .and_then(Value::as_array)
                    .map_or(&[][..], Vec::as_slice);
                if items.is_empty() || depth + 1 >= INSTANCE_DEPTH {
                    continue;
                }
                for item in items {
                    n += 1;
                    if !inner.is_empty()
                        && depth + 2 < INSTANCE_DEPTH
                        && unseen
                        && let Some(object) = item.as_object()
                    {
                        n += count(project, inner, Some(object), depth + 2, &seen);
                    }
                }
                continue;
            }
            if !inner.is_empty()
                && depth + 1 < INSTANCE_DEPTH
                && unseen
                && value.is_none_or(Value::is_object)
            {
                n += count(
                    project,
                    inner,
                    value.and_then(Value::as_object),
                    depth + 1,
                    &seen,
                );
            }
        }
        n
    }
    count(
        project,
        &shape.fields,
        card.instance_data().and_then(Value::as_object),
        0,
        &HashSet::from([shape.id.clone()]),
    )
}

/// Whether a formatdoc's rows at the top point at anything (its `#top` row).
fn top_points(card: &Card) -> bool {
    card.rows.iter().filter(|r| r.kind != "item").any(|r| {
        !r.target.is_empty()
            || r.texts()
                .iter()
                .any(|(_, text)| !crate::notetext::refs(text).is_empty())
    })
}

/// Field rows: every field and what an object holds; a value type or an enum a field
/// refers to is folded at rest, so it is one row.
fn fields(list: &[Field]) -> usize {
    list.iter().map(|f| 1 + fields(&f.children)).sum()
}

// MARK: - A flow's nodes

/// How many units down a node's card is: its frame and a row each (`FlowLayout.rows`).
pub(crate) fn node_rows(node: &crate::model::FlowNode) -> i64 {
    let ways = node.nexts.len().max(1);
    let count = match node.kind.as_str() {
        "begin" => 1 + usize::from(node.trigger.as_deref() == Some("receive")) + ways,
        "process" => node.steps.len().max(1) + ways,
        "run" => {
            1 + usize::from(!(node.carries.is_empty() && node.returns.is_empty()))
                + ways
                + usize::from(node.on_error.is_some())
        }
        "eval" => usize::from(node.value.is_some()) + node.branches.len().max(1) + 1,
        "wait" => 1 + usize::from(node.timeout.is_some()) + ways,
        "join" => 1 + usize::from(node.partial.is_some()) + ways,
        _ => 1 + usize::from(node.reply.is_some()),
    };
    rows_for_height(2.0 + 40.0 + 12.0 + count as f64 * UNIT_HEIGHT)
}

/// A box on the grid: x, y, w, h.
pub(crate) type Rect = (i64, i64, i64, i64);

pub(crate) fn intersects(a: Rect, b: Rect) -> bool {
    a.0 < b.0 + b.2 && b.0 < a.0 + a.2 && a.1 < b.1 + b.3 && b.1 < a.1 + a.3
}

/// Whether the diagram of `section` keeps a box where it is written: a ui section's parts sit
/// edge to edge and one over another (a badge over a button), so no margin is kept and none
/// jumps. The one place the rule lives; the screen asks the same of the section's kind.
pub(crate) fn keeps_places(project: &crate::export::Project, section: &str) -> bool {
    project.section_kind(section) == Some(super::frames::UI)
}

/// The place nearest `wanted` where a box keeps its margin clear of every one in
/// `taken`, looked for out to 40 units; failing that, `wanted` (`clearSpot`).
pub(crate) fn clear_spot(
    margin: i64,
    wanted: (i64, i64),
    w: i64,
    h: i64,
    taken: &[Rect],
) -> (i64, i64) {
    let fits = |(x, y): (i64, i64)| {
        let room = (x - margin, y - margin, w + 2 * margin, h + 2 * margin);
        !taken.iter().any(|t| intersects(*t, room))
    };
    if fits(wanted) {
        return wanted;
    }
    for ring in 1..=40i64 {
        let mut best = None;
        let mut best_distance = i64::MAX;
        for dx in -ring..=ring {
            for dy in -ring..=ring {
                if dx.abs().max(dy.abs()) != ring {
                    continue;
                }
                let p = (wanted.0 + dx, wanted.1 + dy);
                let distance = dx * dx + dy * dy;
                if distance < best_distance && fits(p) {
                    best = Some(p);
                    best_distance = distance;
                }
            }
        }
        if let Some(p) = best {
            return p;
        }
    }
    wanted
}

/// Where a box `w` wide goes when no one placed it: under the shortest of four columns,
/// each a card wide and the margin apart (`AppModel.placeUnplaced`, `nextSpot`).
pub(crate) fn next_spot(taken: &[Rect], w: i64, margin: i64) -> (i64, i64) {
    let step = CARD_COLUMNS + margin;
    let mut best: Option<(i64, i64)> = None;
    for column in 0..4 {
        let x = column * step;
        let bottom = taken
            .iter()
            .filter(|t| t.0 < x + w && x < t.0 + t.2)
            .map(|t| t.1 + t.3 + margin)
            .max()
            .unwrap_or(0);
        if best.is_none_or(|b| bottom < b.1) {
            best = Some((x, bottom));
        }
    }
    best.unwrap_or((0, 0))
}

/// Places for the nodes that have none, column by column from the start, clear of the
/// placed ones (`FlowLayout.place`).
pub(crate) fn place_flow(
    nodes: &[crate::model::FlowNode],
    placed: &std::collections::HashMap<String, (i64, i64)>,
    margin: i64,
) -> Vec<(String, (i64, i64))> {
    use std::collections::{HashMap, VecDeque};
    if nodes.iter().all(|n| placed.contains_key(&n.name)) {
        return Vec::new();
    }
    let index: HashMap<&str, usize> = nodes
        .iter()
        .enumerate()
        .rev()
        .map(|(i, n)| (n.name.as_str(), i))
        .collect();
    let mut column: HashMap<usize, i64> = HashMap::new();
    let mut order = Vec::new();
    let mut queue: VecDeque<usize> = (0..nodes.len())
        .filter(|&i| nodes[i].kind == "begin")
        .collect();
    for &i in &queue {
        column.insert(i, 0);
    }
    while let Some(i) = queue.pop_front() {
        order.push(i);
        for (to, _, _) in nodes[i].exits() {
            let Some(&j) = index.get(to.as_str()) else {
                continue;
            };
            if column.contains_key(&j) {
                continue;
            }
            column.insert(j, column[&i] + 1);
            queue.push_back(j);
        }
    }
    let beyond = column.values().max().copied().unwrap_or(-1) + 1;
    for i in 0..nodes.len() {
        if let std::collections::hash_map::Entry::Vacant(e) = column.entry(i) {
            e.insert(beyond);
            order.push(i);
        }
    }
    let mut taken: Vec<Rect> = nodes
        .iter()
        .filter_map(|n| {
            placed
                .get(&n.name)
                .map(|&(x, y)| (x, y, CARD_COLUMNS, node_rows(n)))
        })
        .collect();
    let mut depth: HashMap<i64, i64> = HashMap::new();
    let mut out = Vec::new();
    for i in order {
        let c = column[&i];
        let node = &nodes[i];
        let h = node_rows(node);
        let wanted = (
            c * (CARD_COLUMNS + 2 * margin),
            depth.get(&c).copied().unwrap_or(0),
        );
        *depth.entry(c).or_insert(0) += h + 2 * margin;
        if placed.contains_key(&node.name) {
            continue;
        }
        let at = clear_spot(margin, wanted, CARD_COLUMNS, h, &taken);
        taken.push((at.0, at.1, CARD_COLUMNS, h));
        out.push((node.name.clone(), at));
    }
    out
}

/// Where the cards of `section` that overlap the ones before them go to be clear: its boxes drawn as cards, in their order —
/// a value object or an enum folded into the cards holding it blocks nothing — each that lies
/// over one kept before it moved to the nearest clear cell. By id: the box, where it sits, and
/// where it goes.
pub(crate) fn untangled(
    project: &crate::export::Project,
    layout: &Layout,
    section: &str,
) -> Vec<(OwnBox, (i64, i64))> {
    let mut kept: Vec<Rect> = Vec::new();
    let mut out = Vec::new();
    for b in own_boxes(project, layout, section)
        .into_iter()
        .filter(|b| b.drawn)
    {
        let Some((x, y)) = b.at else { continue };
        let (w, h) = b.size;
        let rect = (x, y, w, h);
        if kept.iter().any(|k| intersects(*k, rect)) {
            let landed = clear_spot(layout.margin, (x, y), w, h, &kept);
            kept.push((landed.0, landed.1, w, h));
            out.push((b, landed));
        } else {
            kept.push(rect);
        }
    }
    out
}

/// The cards that overlap on each section's diagram (the check's `overlaps`): for each, the card,
/// its section, the cards it lies over where they all sit now, and where it would go to be clear
/// (`untangled`), as `section_update`'s `tidy` puts it.
pub(crate) fn overlaps(
    project: &crate::export::Project,
    layout: &Layout,
) -> Vec<Map<String, Value>> {
    let mut out = Vec::new();
    for section in &project.detail.sections {
        if keeps_places(project, &section.id) {
            continue;
        }
        let boxes = own_boxes(project, layout, &section.id);
        let moved = untangled(project, layout, &section.id);
        for (b, to) in moved {
            let Some((x, y)) = b.at else { continue };
            let rect = (x, y, b.size.0, b.size.1);
            let over: Vec<Value> = boxes
                .iter()
                .filter(|o| o.drawn && o.id != b.id)
                .filter(|o| {
                    o.at.is_some_and(|(ox, oy)| intersects((ox, oy, o.size.0, o.size.1), rect))
                })
                .map(|o| Value::from(o.path.clone()))
                .collect();
            // Untangling moves a card off where an earlier one lands, not only off one it lies over
            // now: a card clear where it sits is not an overlap to say.
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
            row.insert("moveTo".into(), super::boxes::point(to.0, to.1));
            out.push(row);
        }
    }
    out
}
