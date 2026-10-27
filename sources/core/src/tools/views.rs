//! What the tools answer with (`MCPViews`): a card, a field, a row, a node; the project's
//! check; its sections.

use super::Seen;
use super::check::{count_fields, rules_json};
use crate::model::{Card, Field, Section};
use serde_json::{Map, Value};
use std::collections::HashMap;

/// The key of `refersTo` for the lines leaving a card's head. A field or a row of this very name
/// is written quoted there (`'(card)'`), as a path may quote any name.
pub(crate) const HEAD: &str = "(card)";

/// Where a section sits among the root's: in the order the project says (`project_tree`, by its name
/// in any case), then anything else there — `_evacuation_` last (`SectionKind.rootOrder`).
pub(crate) fn root_order(defs: &crate::kinds::Defs, name: &str) -> i64 {
    defs.root_place(name)
}

impl Seen {
    pub fn section(&self, id: &str) -> Option<&Section> {
        self.detail.sections.iter().find(|s| s.id == id)
    }

    /// The sections directly inside one (none for the top), the root's by their kind.
    pub fn children(&self, parent: Option<&str>) -> Vec<&Section> {
        let known = |id: &str| self.section(id).is_some();
        let inside: Vec<&Section> = self
            .detail
            .sections
            .iter()
            .filter(|s| s.parent_id.as_deref().filter(|p| known(p)) == parent)
            .collect();
        let at_root =
            parent.is_some_and(|p| self.section(p).is_some_and(|s| s.parent_id.is_none()));
        if !at_root {
            return inside;
        }
        let defs = crate::kinds::Defs::of_project(self.detail.project.kinds.as_deref());
        let mut ordered: Vec<(usize, &Section)> = inside.into_iter().enumerate().collect();
        ordered.sort_by_key(|(i, s)| (root_order(&defs, &s.name), *i));
        ordered.into_iter().map(|(_, s)| s).collect()
    }

    /// Every section, parents before children, as the nav panel lists them.
    pub fn listed(&self) -> Vec<&Section> {
        fn visit<'a>(
            seen: &'a Seen,
            id: Option<&str>,
            out: &mut Vec<&'a Section>,
            done: &mut std::collections::HashSet<String>,
        ) {
            for s in seen.children(id) {
                if done.insert(s.id.clone()) {
                    out.push(s);
                    visit(seen, Some(&s.id), out, done);
                }
            }
        }
        let mut out = Vec::new();
        visit(self, None, &mut out, &mut Default::default());
        out
    }

    /// One row a section, as `section_list` has them (`sectionRowList`).
    pub fn section_rows(&self, only: Option<&str>) -> Vec<Map<String, Value>> {
        let project = self.project();
        let mut members: HashMap<&str, usize> = HashMap::new();
        for card in &self.detail.schemas {
            *members
                .entry(
                    card.section_id
                        .as_deref()
                        .unwrap_or(&project.directory.home),
                )
                .or_default() += 1;
        }
        let _ = &mut members;
        self.listed()
            .into_iter()
            .filter(|s| only.is_none_or(|o| o == s.id))
            .map(|s| {
                let mut row = Map::new();
                row.insert(
                    "section".into(),
                    project.directory.full_section(&s.id).into(),
                );
                row.insert(
                    "cardCount".into(),
                    project.directory.members(&s.id).len().into(),
                );
                let inside = self.children(Some(&s.id)).len();
                if inside > 0 {
                    row.insert("sections".into(), inside.into());
                }
                if s.parent_id.is_none() {
                    row.insert("main".into(), true.into());
                }
                if let Some(kind) = &s.kind {
                    row.insert("type".into(), kind.clone().into());
                }
                if let Some(figure) = &s.figure {
                    row.insert("figure".into(), figure.clone().into());
                }
                if let Some(icon) = &s.icon {
                    row.insert(
                        "icon".into(),
                        icon_json(icon.name.as_deref(), icon.color.as_deref()),
                    );
                }
                // Its card ghosts are named among its cards, not counted with them.
                let ghosts = project.directory.ghosts_in(&s.id).len();
                if ghosts > 0 {
                    row.insert("ghostCount".into(), ghosts.into());
                }
                if !s.description.is_empty() {
                    row.insert("description".into(), s.description.clone().into());
                }
                if s.missing {
                    row.insert("missing".into(), true.into());
                }
                row
            })
            .collect()
    }
}

/// A mark as it is written, `{name, color}`, each part only when there is one.
pub(crate) fn icon_json(name: Option<&str>, color: Option<&str>) -> Value {
    let mut out = Map::new();
    if let Some(name) = name {
        out.insert("name".into(), name.into());
    }
    if let Some(color) = color {
        out.insert("color".into(), color.into());
    }
    Value::Object(out)
}

// MARK: - A card and what is in it

/// Which parts of a card to return (`MCPViews.Projection`).
#[derive(Clone, Debug)]
pub(crate) struct Projection(std::collections::HashSet<String>);

impl Projection {
    pub const KNOWN: [&'static str; 12] = [
        "fields",
        "descriptions",
        "tags",
        "rules",
        "absentFields",
        "connections",
        "metadata",
        "ref_to",
        "ref_by",
        "used_by",
        "at",
        "exports",
    ];

    /// What a card holds itself, without where it sits, what points back at it or where it is
    /// written out: a list's parts when none are named, the rest left to be asked for.
    pub const OWN: [&'static str; 8] = [
        "fields",
        "descriptions",
        "tags",
        "rules",
        "absentFields",
        "connections",
        "metadata",
        "ref_to",
    ];

    pub fn everything() -> Projection {
        Projection(Self::KNOWN.iter().map(|s| s.to_string()).collect())
    }

    pub fn own() -> Projection {
        Projection(Self::OWN.iter().map(|s| s.to_string()).collect())
    }

    pub fn none() -> Projection {
        Projection(Default::default())
    }

    pub fn of(items: &[String]) -> Projection {
        Projection(items.iter().cloned().collect())
    }

    pub fn has(&self, item: &str) -> bool {
        self.0.contains(item)
    }
}

fn strings(list: &[String]) -> Value {
    Value::Array(list.iter().map(|s| Value::from(s.clone())).collect())
}

/// A card's lines as MCP writes them (`connections`): the card each reaches by its path,
/// as written (a card ghost by its own); `null` when it is gone.
pub(crate) fn connections_json(project: &crate::export::Project, card: &Card) -> Value {
    Value::Array(
        card.connections
            .iter()
            // A data card's shape is its key `of`, not one of the lines MCP writes.
            .filter(|c| c.line != crate::kinds::OF_LINE)
            .map(|c| {
                let mut out = Map::new();
                out.insert("line".into(), c.line.clone().into());
                if !c.from.is_empty() {
                    out.insert("from".into(), c.from.clone().into());
                }
                // What is inside a working directory's file is said past the file's path.
                let inside =
                    c.at.as_deref()
                        .filter(|a| !a.is_empty())
                        .filter(|_| c.to.as_deref().is_some_and(|t| t.starts_with("file:")));
                if let Some(to) = &c.to {
                    out.insert(
                        "to".into(),
                        project
                            .named(to)
                            .map(|path| match inside {
                                Some(at) => Value::from(format!("{path}.{at}")),
                                None => Value::from(path),
                            })
                            .unwrap_or(Value::Null),
                    );
                }
                if let Some(at) = c.at.as_ref().filter(|_| inside.is_none()) {
                    out.insert("at".into(), at.clone().into());
                }
                if let Some(d) = &c.description {
                    out.insert("description".into(), d.clone().into());
                }
                if !c.fields.is_empty() {
                    out.insert("fields".into(), sorted(&Value::Object(c.fields.clone())));
                }
                Value::Object(out)
            })
            .collect(),
    )
}

/// A free-form value, its objects' keys sorted (`OJSON(JSONValue)`).
fn sorted(value: &Value) -> Value {
    crate::export::sorted(value)
}

impl Seen {
    /// One field, from its parent's point of view (`MCPViews.field`). `holder` is the card
    /// it is in; `owner`/`at` only for a card's own top-level fields. What a payload left
    /// out of what it covers (`outsideOf`) went with the payloads (a
    /// signature's keys are lines of the project's own, whose fields say it).
    pub fn field_view(
        &self,
        field: &Field,
        base_path: &str,
        _holder: &Card,
        _owner_at: Option<(&Card, &str)>,
        projection: &Projection,
    ) -> Value {
        let mut out = Map::new();
        out.insert("name".into(), field.name.clone().into());
        out.insert("type".into(), field.kind.clone().into());
        if !field.label.is_empty() {
            out.insert("label".into(), field.label.clone().into());
        }
        if projection.has("descriptions") && !field.description.is_empty() {
            out.insert("description".into(), field.description.clone().into());
        }
        if field.is_required {
            out.insert("required".into(), true.into());
        }
        if field.is_nullable {
            out.insert("nullable".into(), true.into());
        }
        if let Some(fixed) = field.fixed() {
            out.insert("const".into(), sorted(fixed));
        }
        if field.is_array {
            out.insert("array".into(), true.into());
        }
        if field.is_unique {
            out.insert("unique".into(), true.into());
        }
        if field.is_pk {
            out.insert("primaryKey".into(), true.into());
        }
        if field.is_object() {
            out.insert(
                "path".into(),
                crate::pathtext::member(base_path, &field.name).into(),
            );
            out.insert("fieldCount".into(), field.children.len().into());
        }
        // What it embeds and what its value names are the card's lines (`connections`):
        // an embedded field reads as `type: ref`, the card it holds in its line.
        if projection.has("rules") && !field.rules.is_empty() {
            out.insert("rules".into(), rules_json(&field.rules));
        }
        if !field.variants.is_empty() {
            out.insert("variants".into(), strings(&field.variants));
        }
        if !field.also.is_empty() {
            out.insert("also".into(), strings(&field.also));
        }
        Value::Object(out)
    }
}

impl Seen {
    /// Fields as `card_create` writes them: one list, every field down to the last one nested, each
    /// named by its names from `fields`' holder joined with dots — cut once the list passes the
    /// characters an answer is given in (`paging::MOST_CHARS`), which the second says.
    pub fn flat_fields(
        &self,
        fields: &[Field],
        holder: &Card,
        projection: &Projection,
    ) -> (Vec<Value>, bool) {
        let mut out = Vec::new();
        let mut size = 0;
        let mut cut = false;
        super::check::walk_fields(fields, &mut |field, trail| {
            if cut {
                return;
            }
            let mut entry = match self.field_view(field, "", holder, None, projection) {
                Value::Object(entry) => entry,
                _ => return,
            };
            entry.shift_remove("path");
            entry.shift_remove("fieldCount");
            entry.insert("name".into(), dotted(trail).into());
            let entry = Value::Object(entry);
            size += super::paging::chars(&entry);
            if size > super::paging::MOST_CHARS && !out.is_empty() {
                cut = true;
                return;
            }
            out.push(entry);
        });
        (out, cut)
    }
}

/// A field's names joined with dots as `card_create`'s `fields` reads them back: a name quoted only
/// where it would read otherwise (a dot, a bracket or a quote in it, a space at an end).
fn dotted(names: &[String]) -> String {
    names
        .iter()
        .map(|n| {
            if n.contains(['.', '[', ']', '\''])
                || n.starts_with(char::is_whitespace)
                || n.ends_with(char::is_whitespace)
            {
                crate::pathtext::quote(n).into_owned()
            } else {
                n.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(".")
}

/// The form a target is read and written in: a card, a field of an entity or a value, a value of an
/// enum, a value of a data card, or a row (a formatdoc's, a stackdoc's version, a process node's step,
/// a row of a kind the project defines).
pub(crate) fn form_of(target: &super::walk::Target) -> &'static str {
    use super::walk::Target;
    match target {
        Target::Card(_) | Target::Ghost(_) | Target::Symbol(..) => "card",
        Target::Field(..) => "field",
        Target::EnumValue(..) => "enumValue",
        Target::CustomRow(card, _) if card.kind == crate::kinds::INSTANCE => "datum",
        Target::FormatRow(..) | Target::CustomRow(..) | Target::Step(..) => "row",
    }
}

/// `form` put in an answer, after its `path` (at its end when it has none).
pub(crate) fn put_form(answer: &mut Value, form: &str) {
    let Value::Object(out) = answer else {
        return;
    };
    let at = out
        .keys()
        .position(|k| k == "path")
        .map_or(out.len(), |i| i + 1);
    out.shift_insert(at, "form".into(), form.into());
}

impl Seen {
    /// One card, projected (`MCPViews.schemaView`). Nested objects are named and
    /// counted, not expanded.
    pub fn card_view(
        &self,
        card: &Card,
        projection: &Projection,
        layout: Option<&super::grid::Layout>,
    ) -> Value {
        let project = self.project();
        let mut out = Map::new();
        out.insert("project".into(), self.summary.name.clone().into());
        out.insert("path".into(), project.full(card).into());
        out.insert("kind".into(), card.kind.clone().into());
        if !card.label.is_empty() {
            out.insert("label".into(), card.label.clone().into());
        }
        // A card inside a container: its box on the container's card map.
        let container = project.container_of(card);
        let at = match container {
            Some(c) if project.card_mapped(&c.kind) => {
                c.item_places.get(&card.id).map(|p| (p.x, p.y))
            }
            _ => card
                .pos_x
                .zip(card.pos_y)
                .map(|(x, y)| (x.round() as i64, y.round() as i64)),
        };
        if let Some(layout) = layout
            && projection.has("at")
            && let Some((x, y)) = at
        {
            let (w, h) = match container {
                Some(c) if c.kind == "flow" => (
                    super::grid::CARD_COLUMNS,
                    c.nodes
                        .iter()
                        .find(|n| n.name == card.name)
                        .map_or(1, super::grid::node_rows),
                ),
                Some(_) => super::card_map::size(card, &project, layout),
                None => (
                    super::grid::card_columns(card, project.defs.cards()),
                    super::grid::card_rows(card, &project, layout),
                ),
            };
            let point = |x: i64, y: i64| {
                let mut p = Map::new();
                p.insert("x".into(), x.into());
                p.insert("y".into(), y.into());
                Value::Object(p)
            };
            out.insert("topLeft".into(), point(x, y));
            out.insert("bottomRight".into(), point(x + w - 1, y + h - 1));
            out.insert("size".into(), size_of(w, h));
            // The size written for it, apart from the box drawn: `{w}` leaves the height to the
            // contents.
            if let Some(own) = super::grid::own_size(card, project.defs.cards()) {
                out.insert("ownSize".into(), super::grid::size_value(own));
            }
        }
        if projection.has("at")
            && let Some(view) = &card.doc_view
        {
            out.insert("view".into(), view.clone().into());
        }
        if projection.has("at")
            && let Some(version) = &card.version
        {
            out.insert("version".into(), version.clone().into());
        }
        if projection.has("at") && card.standalone {
            out.insert("standalone".into(), true.into());
        }
        if projection.has("descriptions") && !card.description.is_empty() {
            out.insert("description".into(), card.description.clone().into());
        }
        if projection.has("tags") && !card.tags.is_empty() {
            out.insert("tags".into(), strings(&card.tags));
        }

        if projection.has("metadata") && !card.metadata.is_empty() {
            out.insert(
                "metadata".into(),
                sorted(&Value::Object(card.metadata.clone())),
            );
        }
        if projection.has("rules") && !card.rules.is_empty() {
            out.insert("rules".into(), rules_json(&card.rules));
        }
        if projection.has("absentFields") && !card.absent_fields.is_empty() {
            out.insert(
                "absentFields".into(),
                Value::Array(
                    card.absent_fields
                        .iter()
                        .map(|a| {
                            let mut m = Map::new();
                            m.insert("name".into(), a.name.clone().into());
                            m.insert("reason".into(), a.reason.clone().into());
                            Value::Object(m)
                        })
                        .collect(),
                ),
            );
        }
        if let Some(d) = &card.discriminator {
            out.insert("discriminator".into(), d.clone().into());
        }
        if let Some(face) = &card.face {
            out.insert("face".into(), face.clone());
        }
        // In the kinds' form, an open enum reads as it is written: `open`.
        if card.enum_open {
            out.insert("open".into(), true.into());
        }
        let fields = projection.has("fields");
        // A flow's nodes: the cards inside it, one level (`cards` below), and those no way
        // reaches from its start.
        if fields && card.kind == "flow" {
            let lost = super::check::unreachable(card);
            if !lost.is_empty() {
                out.insert("unreachable".into(), strings(&lost));
            }
        }
        // A flow's node: its fields and rows as its kind writes them.
        if fields && crate::kinds::inner_only(&card.kind) {
            for (k, v) in self.defined_wire(card) {
                out.insert(k, v);
            }
        }

        if fields && card.is_writing() {
            if card.kind == "formatdoc" {
                out.insert("rows".into(), row_list(&card.rows, &project.full(card)));
            } else if card.kind == "stackdoc" {
                // Its versions without their bodies: a body is read with the version's path.
                out.insert("versions".into(), version_list(card, &project.full(card)));
            } else {
                out.insert("body".into(), card.body.clone().into());
            }
            let mut places: Vec<String> = Vec::new();
            for link in super::facts::links(card) {
                if !places.contains(&link.path) {
                    places.push(link.path);
                }
            }
            if !places.is_empty() {
                out.insert("mentions".into(), strings(&places));
            }
        } else if fields && project.is_own_kind(&card.kind) {
            // A kind the project defines: its fields and rows as its
            // definition writes them, and what the definition no longer has.
            for (k, v) in self.defined_wire(card) {
                if !["tags", "metadata"].contains(&k.as_str()) {
                    out.insert(k, v);
                }
            }
            let strays = self.strays(card);
            if !strays.is_empty() {
                out.insert(
                    "strayFields".into(),
                    Value::Object(strays.into_iter().collect()),
                );
            }
        } else if card.kind == crate::kinds::INSTANCE {
            // A data card: its shape,
            // and its values with those whose fields the shape has lost said apart.
            out.insert(
                "of".into(),
                card.instance_of()
                    .and_then(|id| project.named(id))
                    .map_or(Value::Null, Value::from),
            );
            if fields {
                let strays = super::instances::strays(&project, card);
                out.insert(
                    "data".into(),
                    super::instances::without_strays(card.instance_data(), &strays),
                );
                if !strays.is_empty() {
                    out.insert(
                        "strayFields".into(),
                        Value::Object(strays.into_iter().collect()),
                    );
                }
            }
        } else if card.kind == crate::kinds::TABLE {
            // A table: its shape, its
            // rules as written, and the table they build.
            out.insert(
                "of".into(),
                card.shape_of()
                    .and_then(|id| project.named(id))
                    .map_or(Value::Null, Value::from),
            );
            if fields {
                for key in ["within", "where", "columns", "sort", "unnest"] {
                    if let Some(v) = card.table_field(key) {
                        out.insert(key.into(), v.clone());
                    }
                }
                out.insert("table".into(), super::tables::read(&project, card));
            }
        } else if fields
            && matches!(
                card.kind.as_str(),
                "goal" | "task" | "file" | "svg" | "html" | "frame"
            )
        {
            if let Some(form) = &card.form {
                for (k, v) in self.form_wire(card, form) {
                    out.insert(k, v);
                }
            }
            // Nothing inside to count says nothing.
            if let Some(progress) = super::board::progress(self, card)
                && progress["of"] != 0
            {
                out.insert("progress".into(), progress);
            }
            // What the board's kinds no longer have: kept apart, let go of
            // with `card_update`'s `strayFields`.
            let strays = self.strays(card);
            if !strays.is_empty() {
                out.insert(
                    "strayFields".into(),
                    Value::Object(strays.into_iter().collect()),
                );
            }
        } else if fields && crate::kinds::inner_only(&card.kind) {
            // A node's fields and rows are its kind's, written above.
        } else if fields {
            if card.kind == "enum" {
                out.insert(
                    "values".into(),
                    Value::Array(
                        card.values
                            .iter()
                            .map(|v| {
                                let mut e = Map::new();
                                e.insert("name".into(), v.value.clone().into());
                                if !v.label.is_empty() {
                                    e.insert("label".into(), v.label.clone().into());
                                }
                                if !v.description.is_empty() {
                                    e.insert("description".into(), v.description.clone().into());
                                }
                                Value::Object(e)
                            })
                            .collect(),
                    ),
                );
            } else {
                let (fields, cut) = self.flat_fields(&card.fields, card, projection);
                if !fields.is_empty() {
                    out.insert("fields".into(), Value::Array(fields));
                }
                if cut {
                    out.insert("truncated".into(), true.into());
                }
            }
        } else if !fields {
            if card.kind == "enum" {
                out.insert("valueCount".into(), card.values.len().into());
            } else if matches!(card.kind.as_str(), "entity" | "value" | "enum") {
                out.insert("fieldCount".into(), count_fields(&card.fields).into());
            }
        }
        // A container: the cards inside it, one level, by their paths (as a section's).
        if fields && project.is_container(&card.kind) {
            let inside: Vec<Value> = project
                .inner_of(&card.id)
                .iter()
                .map(|c| Value::from(project.full(c)))
                .collect();
            if !inside.is_empty() {
                out.insert("cards".into(), Value::Array(inside));
            }
        }
        if projection.has("used_by") && card.is_shape() {
            for (key, value) in self.used_by(card) {
                out.insert(key, value);
            }
        }
        // Its lines as they are written.
        if projection.has("connections")
            && card
                .connections
                .iter()
                .any(|c| c.line != crate::kinds::OF_LINE)
        {
            out.insert("connections".into(), connections_json(&project, card));
        }
        // What the card points at, the other side of
        // `referencedBy`: where each of its lines leaves — a field or a row by its path in the
        // card, the head as `(card)` — and the cards they reach, as they were written: a card
        // ghost by its own path. From every line, a flow's node's and a goal's card's too.
        // With or without the fields.
        let lines = if projection.has("ref_to") || projection.has("ref_by") {
            crate::screen_detail::connection_lines(&project)
        } else {
            Vec::new()
        };
        let text = |line: &Value, key: &str| line[key].as_str().unwrap_or_default().to_string();
        if projection.has("ref_to") {
            let mut refers: Map<String, Value> = Map::new();
            for line in lines
                .iter()
                .filter(|l| l["fromSchemaId"] == card.id.as_str())
            {
                let to = line["viaGhostId"]
                    .as_str()
                    .and_then(|g| project.named(g))
                    .or_else(|| {
                        project
                            .card(&text(line, "toSchemaId"))
                            .map(|c| project.full(c))
                    })
                    // A board's card's line to main's card, by main's path.
                    .or_else(|| {
                        let outside = project.outside.as_deref()?;
                        outside
                            .card(&text(line, "toSchemaId"))
                            .map(|c| outside.full(c))
                    });
                let Some(to) = to.map(Value::from) else {
                    continue;
                };
                let from = text(line, "from");
                let key = if from.is_empty() {
                    HEAD.to_string()
                } else if from == HEAD {
                    format!("'{HEAD}'")
                } else {
                    from
                };
                if let Value::Array(list) = refers
                    .entry(key)
                    .or_insert_with(|| Value::Array(Vec::new()))
                    && !list.contains(&to)
                {
                    list.push(to);
                }
            }
            if !refers.is_empty() {
                out.insert("refersTo".into(), Value::Object(refers));
            }
        }
        if projection.has("ref_by") {
            let mut incoming: Vec<String> = Vec::new();
            for line in lines.iter().filter(|l| {
                l["toSchemaId"] == card.id.as_str() && l["fromSchemaId"] != card.id.as_str()
            }) {
                let from = project
                    .card(&text(line, "fromSchemaId"))
                    .map(|c| project.full(c))
                    .unwrap_or_else(|| "?".to_string());
                let path = text(line, "from");
                // A line from the card's head is the card itself.
                let said = if path.is_empty() {
                    from
                } else {
                    format!("{from}.{path}")
                };
                incoming.push(said);
            }
            if !incoming.is_empty() {
                out.insert("referencedBy".into(), strings(&incoming));
            }
        }
        Value::Object(out)
    }

    /// Which flows a change to this card reaches, and which writings mention it: the
    /// `usedBy` and `mentionedBy` of a card.
    pub fn used_by(&self, card: &Card) -> Vec<(String, Value)> {
        let project = self.project();
        let base = project.section_of(card);
        let Some(hits) = super::impact::of(&card.name, &project, Some(&base)) else {
            return Vec::new();
        };
        let (notes, flows): (Vec<_>, Vec<_>) = hits.into_iter().partition(|h| h.card.is_writing());
        let mut out = Vec::new();
        if !flows.is_empty() {
            out.push((
                "usedBy".to_string(),
                Value::Array(
                    flows
                        .iter()
                        .map(|h| {
                            let mut row = Map::new();
                            row.insert("flow".into(), project.full(h.card).into());
                            row.insert("role".into(), h.used.role.name().into());
                            row.insert("uses".into(), h.used.path.clone().into());
                            Value::Object(row)
                        })
                        .collect(),
                ),
            ));
        }
        if !notes.is_empty() {
            out.push((
                "mentionedBy".to_string(),
                Value::Array(
                    notes
                        .iter()
                        .map(|h| {
                            let mut row = Map::new();
                            row.insert("note".into(), super::impact::written(h, &project).into());
                            row.insert("mentions".into(), h.used.path.clone().into());
                            Value::Object(row)
                        })
                        .collect(),
                ),
            ));
        }
        out
    }

    /// A defined kind's card (goal, task) in its MCP form: its fields and rows by name,
    /// a card's id written as its path (a card ghost's own).
    pub fn form_wire(&self, card: &Card, form: &Value) -> Vec<(String, Value)> {
        let project = self.project();
        let path_of = |id: &str| project.named(id);
        let def = project.defs.cards().get(&card.kind);
        let mut out = Vec::new();
        if let Some(fields) = form["fields"].as_object() {
            for (k, v) in fields {
                // What the core's typed reading takes a task's lines and a goal's cards in
                // (`detail`) is not the card's to write.
                if k == "label" || k == "links" || k == "items" {
                    continue;
                }
                // What its definition no longer has is said apart (`strayFields`).
                if def.is_some_and(|d| d.field(k).is_none()) {
                    continue;
                }
                let v = match (k.as_str(), v) {
                    // A section held by its id, by its path now.
                    (_, Value::String(id))
                        if def
                            .and_then(|d| d.field(k))
                            .is_some_and(|f| f.fieldtype == crate::kinds::FieldType::Section)
                            && project.directory.has_section(id) =>
                    {
                        project.directory.full_section(id).into()
                    }
                    ("items", Value::Array(ids)) => Value::Array(
                        ids.iter()
                            .map(|id| {
                                id.as_str()
                                    .and_then(path_of)
                                    .map(Value::from)
                                    .unwrap_or_else(|| id.clone())
                            })
                            .collect(),
                    ),
                    _ => v.clone(),
                };
                out.push((k.clone(), v));
            }
        }
        out
    }
}

impl Seen {
    /// A card read by its kind's definition alone (a flow's node): its fields and rows as
    /// MCP writes them, a path held by id as its path; its label and description are the
    /// card's own keys.
    pub fn defined_wire(&self, card: &Card) -> Vec<(String, Value)> {
        let project = self.project();
        let Some(def) = project.defs.cards().get(&card.kind) else {
            return Vec::new();
        };
        let resolver = super::cards::Resolver {
            project: &project,
            base: project.section_of(card),
            made: &HashMap::new(),
            inside: None,
            unmade: None,
        };
        let doc = card.form.clone().unwrap_or(Value::Null);
        let wire = crate::kinds::to_wire(def, &doc, &resolver);
        wire.as_object()
            .into_iter()
            .flatten()
            .filter(|(k, _)| !["kind", "label", "description"].contains(&k.as_str()))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }
}

impl Seen {
    /// What a card of a kind the project defines, or the board's goal and task,
    /// holds that its definition no longer has (「定義に無い欄」): by where it is, its value as MCP reads it.
    pub fn strays(&self, card: &Card) -> Vec<(String, Value)> {
        let project = self.project();
        let kinds = project.defs.cards();
        let Some(def) = kinds
            .get(&card.kind)
            .filter(|_| kinds.keeps_strays(&card.kind))
        else {
            return Vec::new();
        };
        crate::kinds::strays(def, &card.kept_form())
    }
}

impl Seen {
    /// A card ghost (`MCPViews` has none): its path, `kind: ghost`, its own name, what it points at
    /// (`target`, in full as written; null when that is gone), the card its chain leads to
    /// (`resolvesTo`; null when none), and where it sits.
    pub fn ghost_view(
        &self,
        ghost: &crate::model::Ghost,
        projection: &Projection,
        layout: Option<&super::grid::Layout>,
    ) -> Value {
        let project = self.project();
        let mut out = Map::new();
        out.insert("project".into(), self.summary.name.clone().into());
        out.insert("path".into(), project.full_ghost(ghost).into());
        out.insert("kind".into(), crate::detail::GHOST.into());
        // Its own name where it sits, which need not be its target's: what `name` renames.
        out.insert("name".into(), ghost.name.clone().into());
        let target = ghost.target.as_deref().and_then(|id| project.named(id));
        out.insert(
            "target".into(),
            target.map(Value::from).unwrap_or(Value::Null),
        );
        let reached = project.ghost_card(ghost);
        out.insert(
            "resolvesTo".into(),
            reached
                .map(|c| Value::from(project.full(c)))
                .unwrap_or(Value::Null),
        );
        if let Some(card) = reached {
            out.insert("targetKind".into(), card.kind.clone().into());
        } else {
            let entry = project.directory.entry_of(&ghost.id);
            let why = entry
                .map(|e| project.directory.dead_end(e, &project.full_ghost(ghost)))
                .unwrap_or_default();
            out.insert("problem".into(), why.into());
        }
        if let Some(layout) = layout
            && projection.has("at")
            && let (Some(x), Some(y)) = (ghost.pos_x, ghost.pos_y)
        {
            let (x, y) = (x.round() as i64, y.round() as i64);
            let (w, h) = super::grid::ghost_size(&project, ghost, layout);
            out.insert("topLeft".into(), super::boxes::point(x, y));
            out.insert(
                "bottomRight".into(),
                super::boxes::point(x + w - 1, y + h - 1),
            );
            out.insert("size".into(), size_of(w, h));
        }
        Value::Object(out)
    }
}

/// A box's size in cells, as `size` writes it.
fn size_of(w: i64, h: i64) -> Value {
    let mut size = Map::new();
    size.insert("w".into(), w.into());
    size.insert("h".into(), h.into());
    Value::Object(size)
}

/// What a formatdoc's row holds, as `rows` writes it — an item without its rows.
fn row_entry(row: &crate::model::FormatRow) -> Map<String, Value> {
    let mut out = Map::new();
    out.insert("name".into(), row.name.clone().into());
    out.insert("kind".into(), row.kind.clone().into());
    if !row.heading.is_empty() {
        out.insert("heading".into(), row.heading.clone().into());
    }
    if row.kind == "text" {
        out.insert("body".into(), row.body.clone().into());
    }
    if row.kind == "table" {
        out.insert("columns".into(), strings(&row.columns));
        out.insert(
            "cells".into(),
            Value::Array(row.cells.iter().map(|c| strings(c)).collect()),
        );
    }
    if !row.target.is_empty() {
        out.insert("target".into(), row.target.clone().into());
    }
    out
}

/// A stackdoc's versions in its order, each its path, key and heading — not its body.
pub(crate) fn version_list(card: &Card, under: &str) -> Value {
    Value::Array(
        card.versions()
            .iter()
            .map(|v| {
                let mut entry = Map::new();
                entry.insert(
                    "path".into(),
                    crate::pathtext::member(under, &v.name).into(),
                );
                entry.insert("key".into(), v.key.clone().into());
                if !v.heading.is_empty() {
                    entry.insert("heading".into(), v.heading.clone().into());
                }
                Value::Object(entry)
            })
            .collect(),
    )
}

/// How many rows are in rows, all the way down (`FormatRows.count`).
fn format_count(rows: &[crate::model::FormatRow]) -> usize {
    rows.iter()
        .map(|r| {
            1 + if r.kind == "item" {
                format_count(&r.rows)
            } else {
                0
            }
        })
        .sum()
}

/// Rows at one level, each by its path and with what `rows` writes of it, an item with a count of
/// what is in it (read on by its path).
pub(crate) fn row_list(rows: &[crate::model::FormatRow], under: &str) -> Value {
    Value::Array(
        rows.iter()
            .map(|row| {
                let mut entry = Map::new();
                entry.insert(
                    "path".into(),
                    crate::pathtext::member(under, &row.name).into(),
                );
                entry.extend(row_entry(row));
                if row.kind == "item" && !row.rows.is_empty() {
                    entry.insert("rowCount".into(), format_count(&row.rows).into());
                }
                Value::Object(entry)
            })
            .collect(),
    )
}

impl Seen {
    /// One level of a target (`MCPViews.level`): a field from its parent's point of
    /// view, an object with the fields directly in it, a row, a node, or a card.
    pub fn level(
        &self,
        target: &super::walk::Target,
        projection: &Projection,
        layout: Option<&super::grid::Layout>,
    ) -> Value {
        use super::walk::Target;
        let project = self.project();
        let mut out = Map::new();
        match target {
            Target::Field(holder, field, walked) => {
                let path = walked.iter().fold(project.full(holder), |at, n| {
                    crate::pathtext::member(&at, n)
                });
                let parent = crate::pathtext::last_dot(&path)
                    .map(|(p, _)| p.to_string())
                    .unwrap_or_default();
                out.insert("project".into(), self.summary.name.clone().into());
                out.insert("path".into(), path.clone().into());
                // The keys `card_update` writes at this path, as it takes them; an object's own fields
                // as `card_create` writes them, named from it.
                if let Value::Object(own) =
                    self.field_view(field, &parent, holder, None, projection)
                {
                    for (k, v) in own {
                        if k != "path" && k != "fieldCount" {
                            out.insert(k, v);
                        }
                    }
                }
                if field.is_object() {
                    let (fields, cut) = self.flat_fields(&field.children, holder, projection);
                    out.insert("fields".into(), Value::Array(fields));
                    if cut {
                        out.insert("truncated".into(), true.into());
                    }
                }
                Value::Object(out)
            }
            Target::FormatRow(doc, names) => self.row_view(doc, names),
            Target::CustomRow(card, names) if card.kind == "stackdoc" => {
                self.version_view(card, &names[0])
            }
            Target::CustomRow(card, names) if card.kind == crate::kinds::INSTANCE => {
                self.value_view(card, names)
            }
            Target::CustomRow(card, names) => {
                out.insert("project".into(), self.summary.name.clone().into());
                out.insert(
                    "path".into(),
                    format!(
                        "{}.{}",
                        project.full(card),
                        super::walk::said_names(card, names).join(".")
                    )
                    .into(),
                );
                Value::Object(out)
            }
            Target::EnumValue(card, name) => self.enum_value_view(card, name),
            Target::Step(node, i) => self.step_view(node, *i),
            Target::Card(card) => self.card_view(card, projection, layout),
            Target::Ghost(ghost) => self.ghost_view(ghost, projection, layout),
            // What is in a file is read from it (`code`): here, its path alone.
            Target::Symbol(file, names) => {
                out.insert("project".into(), self.summary.name.clone().into());
                let mut path = project.full(file);
                for n in names {
                    path.push('.');
                    path.push_str(&crate::pathtext::quote(n));
                }
                out.insert("path".into(), path.into());
                out.insert("kind".into(), "symbol".into());
                Value::Object(out)
            }
        }
    }

    /// One value of an enum: its name, and its label and description when it has them — what
    /// `card_create` and `card_update` write at its path.
    pub fn enum_value_view(&self, card: &Card, name: &str) -> Value {
        let project = self.project();
        let mut out = Map::new();
        out.insert("project".into(), self.summary.name.clone().into());
        out.insert(
            "path".into(),
            crate::pathtext::member(&project.full(card), name).into(),
        );
        out.insert("name".into(), name.into());
        if let Some(v) = card.values.iter().find(|v| v.value == name) {
            if !v.label.is_empty() {
                out.insert("label".into(), v.label.clone().into());
            }
            if !v.description.is_empty() {
                out.insert("description".into(), v.description.clone().into());
            }
        }
        Value::Object(out)
    }

    /// One step of a process node, as one element of the node's `steps` reads.
    pub fn step_view(&self, node: &Card, index: usize) -> Value {
        let project = self.project();
        let mut out = Map::new();
        out.insert("project".into(), self.summary.name.clone().into());
        out.insert(
            "path".into(),
            format!("{}.steps[{index}]", project.full(node)).into(),
        );
        let steps = self
            .defined_wire(node)
            .into_iter()
            .find(|(k, _)| k == "steps")
            .map(|(_, v)| v);
        if let Some(Value::Object(step)) = steps.as_ref().and_then(|s| s.get(index)) {
            out.extend(step.clone());
        }
        Value::Object(out)
    }

    /// One value of a data card: its type (the field's, as its badge says it) and the value —
    /// none when it is not written.
    pub fn value_view(&self, card: &Card, names: &[String]) -> Value {
        let project = self.project();
        let mut out = Map::new();
        out.insert("project".into(), self.summary.name.clone().into());
        out.insert(
            "path".into(),
            format!("{}.{}", project.full(card), names.join(".")).into(),
        );
        if let Some(shape) = super::instances::shape(&project, card)
            && let Ok(fields) = super::instances::field_at(&project, shape, names)
            && let Some(field) = fields.last()
        {
            // An element picked: its own type (`Port`, not `Port[]`).
            let label = super::instances::type_label(&project, field);
            let label = if super::instances::picks_last(names) {
                label.trim_end_matches("[]").to_string()
            } else {
                label
            };
            out.insert("type".into(), label.into());
            if field.is_required {
                out.insert("required".into(), true.into());
            }
            if field.is_nullable {
                out.insert("nullable".into(), true.into());
            }
        }
        if let Some(value) = super::instances::value_at(card.instance_data(), names) {
            out.insert("value".into(), value.clone());
        }
        Value::Object(out)
    }

    /// One version of a stackdoc: its key, heading and body, and what its body names.
    pub fn version_view(&self, card: &Card, name: &str) -> Value {
        let project = self.project();
        let mut out = Map::new();
        out.insert("project".into(), self.summary.name.clone().into());
        out.insert(
            "path".into(),
            crate::pathtext::member(&project.full(card), name).into(),
        );
        let Some(version) = card.versions().into_iter().find(|v| v.name == name) else {
            return Value::Object(out);
        };
        out.insert("key".into(), version.key.clone().into());
        if !version.heading.is_empty() {
            out.insert("heading".into(), version.heading.clone().into());
        }
        out.insert("body".into(), version.body.clone().into());
        let mut mentions: Vec<String> = crate::notetext::refs(&version.body)
            .into_iter()
            .map(|r| r.path)
            .collect();
        mentions.sort();
        mentions.dedup();
        if !mentions.is_empty() {
            out.insert("mentions".into(), strings(&mentions));
        }
        Value::Object(out)
    }

    /// One row of a formatdoc: what it holds, the rows directly in an item, and what it
    /// points at (`MCPViews.rowView`).
    pub fn row_view(&self, doc: &Card, names: &[String]) -> Value {
        let project = self.project();
        let path = format!(
            "{}.{}",
            project.full(doc),
            crate::pathtext::join_names(names.iter().map(String::as_str))
        );
        let Some(row) = super::walk::format_row(names, &doc.rows) else {
            return Value::Object(Map::new());
        };
        let mut out = Map::new();
        out.insert("project".into(), self.summary.name.clone().into());
        out.insert("path".into(), path.clone().into());
        out.extend(row_entry(row));
        if row.kind == "item" && !row.rows.is_empty() {
            out.insert("rows".into(), row_list(&row.rows, &path));
        }
        // What the item says itself (or the row, when it is not an item).
        let item: &[String] = if row.kind == "item" {
            names
        } else {
            &names[..names.len() - 1]
        };
        let mut mentions: Vec<String> = Vec::new();
        for inner in super::walk::format_children(item, &doc.rows)
            .iter()
            .filter(|r| r.kind != "item")
        {
            let mut at = item.to_vec();
            at.push(inner.name.clone());
            if row.kind != "item" && at != names {
                continue;
            }
            for (_, text) in inner.texts() {
                for r in crate::notetext::refs(&text) {
                    mentions.push(r.path);
                }
            }
            if !inner.target.is_empty() {
                mentions.push(inner.target.clone());
            }
        }
        mentions.sort();
        mentions.dedup();
        if !mentions.is_empty() {
            out.insert("mentions".into(), strings(&mentions));
        }
        Value::Object(out)
    }
}
