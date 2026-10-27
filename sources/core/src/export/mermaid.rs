//! Diagrams put into a document with `![[path]]` (`NoteEmbed` in the Swift app's Docs),
//! and the mermaid `classDiagram` of the project, of a section, or of some cards and what
//! they touch (`MermaidExport` in Export): structure, never values, with a rev of exactly
//! what is drawn — as it is written out, or as the app draws it for MCP.

use super::paths::landing;
use super::{Project, charts, ident};
use crate::model::{Card, Detail, Relation, TagGroup};
use crate::notetext::{exported_path, plain_path};
use crate::ojson;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

// MARK: - What `![[…]]` puts in

/// What `![[path]]` puts in: a card, or a section by its id.
pub enum Embed<'a> {
    Card(&'a Card),
    Section(String),
}

/// What `path` puts in, read from the section `base` — or why it cannot, in words to
/// act on (`NoteEmbed.find`).
pub fn find_embed<'a>(
    path: &str,
    option: Option<&str>,
    base: &str,
    project: &Project<'a>,
) -> Result<Embed<'a>, String> {
    let (section, rest) = match crate::pathtext::split_section(path) {
        (Some(s), rest) => (Some(s.trim()), rest),
        (None, rest) => (None, rest),
    };
    if rest.is_empty() {
        let found = section.and_then(|s| project.directory.section_from(s, base));
        let Some(found) = found else {
            return Err(text!(
                "export.embed.noSection",
                name = section.unwrap_or(path)
            ));
        };
        // A field to say beside each data card's name, when the section's picture is its data.
        if let Some(o) = option {
            if !super::datachart::holds_data(project, &found) {
                return Err(text!("export.embed.sectionOption", option = o));
            }
            let scope = super::datachart::Scope::Section(found.clone());
            if let Some(why) = super::datachart::field_problem(project, &scope, o) {
                return Err(why);
            }
        }
        return Ok(Embed::Section(found));
    }
    if crate::pathtext::dots(rest).len() > 1 || !crate::pathtext::step(rest).1.is_empty() {
        return Err(text!("export.embed.notAField"));
    }
    let Some(card) = project.find_seeing(path, base) else {
        if project.directory.section_from(path, base).is_some() {
            return Err(text!("export.embed.sectionColon", path));
        }
        let names: Vec<String> = project
            .detail
            .schemas
            .iter()
            .map(|c| c.name.clone())
            .collect();
        return Err(project.directory.find_problem(path, base, &names));
    };
    if card.is_writing() {
        return Err(text!("export.embed.notDiagram", kind = card.kind));
    }
    match (card.kind.as_str(), option) {
        (_, None)
        | ("flow", Some("flowchart"))
        | ("flow", Some("sequence"))
        | ("action", Some("sequence")) => Ok(Embed::Card(card)),
        (crate::kinds::INSTANCE, Some(o)) => {
            match super::datachart::field_problem(project, &super::datachart::Scope::Card(card), o)
            {
                Some(why) => Err(why),
                None => Ok(Embed::Card(card)),
            }
        }
        ("flow", Some(o)) => Err(text!("export.embed.flowShape", option = o)),
        (kind, Some(o)) => Err(text!("export.embed.kindOption", kind, option = o)),
    }
}

/// The mermaid for `path`, read from the section `base`, as it is written out — or why
/// it cannot be drawn.
pub fn embedded(
    path: &str,
    option: Option<&str>,
    base: &str,
    project: &Project,
) -> Result<String, String> {
    embedded_as(path, option, base, project, true)
}

/// The same, as it is written out (`exported`) or as the window draws it in a document —
/// the head saying what it was drawn from, the cards named with their sections
/// (`NoteEmbed.mermaid`): the window asks for it (`embed_diagram`) rather than drawing a
/// project's lines of its own.
pub fn embedded_as(
    path: &str,
    option: Option<&str>,
    base: &str,
    project: &Project,
    exported: bool,
) -> Result<String, String> {
    match find_embed(path, option, base, project)? {
        // Data, and the lines between: its values, not its shapes.
        Embed::Section(found) if super::datachart::holds_data(project, &found) => {
            super::datachart::render(
                project,
                &super::datachart::Scope::Section(found),
                option,
                exported,
            )
        }
        Embed::Card(card) if card.kind == crate::kinds::INSTANCE => super::datachart::render(
            project,
            &super::datachart::Scope::Card(card),
            option,
            exported,
        ),
        Embed::Section(found) => Ok(class_diagram(
            project.detail,
            Spec {
                section: Some(found),
                ..Spec::default()
            },
            exported,
        )),
        Embed::Card(card) if card.kind == "flow" || card.kind == "action" => Ok(if exported {
            charts::render(card, project, option)
        } else {
            charts::render_drawn(card, project, &charts::shape(card, option))
        }),
        Embed::Card(card) => Ok(class_diagram(
            project.detail,
            Spec {
                roots: vec![project.exported(card)],
                section: Some(project.section_of(card)),
                ..Spec::default()
            },
            exported,
        )),
    }
}

/// Which way a diagram walks from its roots.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    /// To what they point at.
    Out,
    /// To what points at them.
    In,
    Both,
}

impl Direction {
    pub const ALL: [Direction; 3] = [Direction::Out, Direction::In, Direction::Both];

    pub fn word(self) -> &'static str {
        match self {
            Direction::Out => "out",
            Direction::In => "in",
            Direction::Both => "both",
        }
    }

    pub fn named(word: &str) -> Option<Direction> {
        Self::ALL.into_iter().find(|d| d.word() == word)
    }
}

/// What a diagram is cut to.
#[derive(Clone, Debug)]
pub struct Spec {
    /// The cards to start from, as written; none: the whole project, or the section.
    pub roots: Vec<String>,
    /// Hops followed from the roots.
    pub depth: usize,
    /// The kinds of line that take part; none: every one.
    pub kinds: Vec<String>,
    /// Only cards with one of these tags (or tag groups); none: no filter.
    pub tags: Vec<String>,
    /// The top-level fields too.
    pub fields: bool,
    pub direction: Direction,
    /// The section a bare root is read from — with no roots, the section drawn with what
    /// it touches of the others.
    pub section: Option<String>,
    /// The section's own cards drawn whether or not a line reaches them, as the relation panel
    /// draws them (what the window shows as mermaid and MCP's `as: diagram`).
    /// Not where a diagram is written out: there a card with no line says nothing.
    pub whole_section: bool,
}

impl Default for Spec {
    /// The whole project, one hop out, every kind of line, no tags, no field lines.
    fn default() -> Spec {
        Spec {
            roots: Vec::new(),
            depth: 1,
            kinds: Vec::new(),
            tags: Vec::new(),
            fields: false,
            direction: Direction::Out,
            section: None,
            whole_section: false,
        }
    }
}

/// The lines a diagram draws as the relations they were (what a field embeds and names);
/// every other line of a card is drawn as the project declares it (`declared_edges`).
fn drawn<'p>(project: &'p Project) -> impl Iterator<Item = &'p Relation> + 'p {
    project.detail.relations.iter().filter(|r| r.legacy)
}

/// The project's data alone: what a schema section holds.
fn data(detail: &Detail) -> Detail {
    let mut out = detail.clone();
    out.schemas.retain(|c| {
        matches!(
            c.kind.as_str(),
            "entity" | "value" | "enum" | "store" | "note"
        )
    });
    let kept: HashSet<String> = out.schemas.iter().map(|c| c.id.clone()).collect();
    out.relations
        .retain(|r| kept.contains(&r.from_schema_id) && kept.contains(&r.to_schema_id));
    out
}

/// The diagram of the project's data as `spec` cuts it, as it is written out (`exported`) or
/// as the window draws it.
fn class_diagram(detail: &Detail, spec: Spec, exported: bool) -> String {
    let data = data(detail);
    let project = Project::new(&data);
    render(&project, &spec, exported)
}

// MARK: - Lines the project declares in a path

fn relation(
    from: &str,
    to: &str,
    field: &str,
    path: String,
    kind: &str,
    target: Option<String>,
    array: bool,
) -> Relation {
    Relation {
        from_schema_id: from.to_string(),
        to_schema_id: to.to_string(),
        field_id: field.to_string(),
        path,
        cardinality: None,
        ref_kind: kind.to_string(),
        ref_target: target,
        is_array: array,
        via_ghost_id: None,
        line: kind.to_string(),
        from: String::new(),
        legacy: false,
    }
}

/// Edges the project declares in a path instead of storing as a relation (`declaredEdges`).
pub fn declared_edges(project: &Project) -> Vec<Relation> {
    let mut out = Vec::new();
    for card in &project.detail.schemas {
        let base = project.section_of(card);
        for link in crate::tools::facts::links(card) {
            if let Some((target, landed)) = landing(&link.path, &base, project)
                && target.id != card.id
            {
                out.push(relation(
                    &card.id,
                    &target.id,
                    "",
                    link.row,
                    link.kind.ref_kind(),
                    Some(landed),
                    false,
                ));
            }
        }
        // A flow's rows' lines are its node cards' lines.
        for line in crate::tools::facts::row_lines(project, card) {
            if let Some((target, landed)) = landing(&line.path, &base, project)
                && target.id != card.id
            {
                let landed = line.landing_row(target, &landed);
                out.push(relation(
                    &card.id,
                    &target.id,
                    "",
                    line.row,
                    line.kind,
                    Some(landed),
                    false,
                ));
            }
        }
        for payload in &card.payloads {
            for (keys, kind) in [
                (&payload.signed_by, "signedBy"),
                (&payload.verified_by, "verifiedBy"),
            ] {
                for path in keys {
                    if let Some((target, landed)) = landing(path, &base, project)
                        && target.id != card.id
                    {
                        out.push(relation(
                            &card.id,
                            &target.id,
                            "",
                            String::new(),
                            kind,
                            Some(landed),
                            false,
                        ));
                    }
                }
            }
        }
        // The lines no diagram drew before the definitions: the project's
        // own that are not written as `x-pointsAt` or `x-payloads`, and the built-in `link`;
        // each from its card (its field, when it leaves one) to the card it reaches.
        for line in &card.connections {
            let Some(def) = project.defs.line(&line.line) else {
                continue;
            };
            let drawn_before = def.base == crate::kinds::Base::Embed
                || matches!(def.annotation(), Some("x-pointsAt") | Some("x-payloads"))
                || (def.builtin && def.name != "link");
            if drawn_before {
                continue;
            }
            let Some(target) = line.to.as_deref().and_then(|id| project.card(id)) else {
                continue;
            };
            if target.id == card.id {
                continue;
            }
            let field = (!line.from.is_empty())
                .then(|| super::field_at(&card.fields, &line.from))
                .flatten();
            out.push(relation(
                &card.id,
                &target.id,
                field.map(|f| f.id.as_str()).unwrap_or_default(),
                line.from.clone(),
                &line.line,
                line.at.clone(),
                field.is_some_and(|f| f.is_array),
            ));
        }
        fn walk(
            fields: &[crate::model::Field],
            trail: &mut Vec<String>,
            visit: &mut dyn FnMut(&crate::model::Field, &[String]),
        ) {
            for f in fields {
                trail.push(f.name.clone());
                visit(f, trail);
                walk(&f.children, trail, visit);
                trail.pop();
            }
        }
        walk(&card.fields, &mut Vec::new(), &mut |field, path| {
            for points in &field.points_at {
                let Some(declared) = &points.exists_in else {
                    continue;
                };
                let Some((target, landed)) = landing(declared, &base, project) else {
                    continue;
                };
                if target.id == card.id {
                    continue;
                }
                let tag = points
                    .discriminator_field
                    .as_ref()
                    .or(points.discriminated_by.as_ref());
                let designated: HashSet<String> = match tag {
                    Some(tag) => project
                        .directory
                        .members(&base)
                        .iter()
                        .map(|&i| project.nth(i))
                        .filter(|c| {
                            c.fields
                                .iter()
                                .find(|f| &f.name == tag)
                                .and_then(|f| f.fixed())
                                .is_some()
                        })
                        .map(|c| c.id.clone())
                        .collect(),
                    None => points
                        .schema
                        .as_deref()
                        .and_then(|s| project.find(s, &base))
                        .map(|c| c.id.clone())
                        .into_iter()
                        .collect(),
                };
                if designated.contains(&target.id) {
                    continue;
                }
                out.push(relation(
                    &card.id,
                    &target.id,
                    &field.id,
                    crate::pathtext::join_names(path.iter().map(String::as_str)),
                    "in",
                    Some(landed),
                    field.is_array,
                ));
            }
        });
    }
    out
}

// MARK: - The section around

struct Around {
    references: Vec<String>,
    consumers: Vec<String>,
    /// The cards of other sections the section's card ghosts lead to, drawn whole.
    card_ghosts: Vec<String>,
    /// Its own values and enums drawn as cards of their own (`standalone`).
    here: Vec<String>,
    /// The names the section's ghosts give the cards they lead to, where they differ from
    /// the card's own: by card.
    aliases: HashMap<String, Vec<String>>,
}

/// What a section draws from the others (`ProjectDetail.surroundings`).
fn surroundings(project: &Project, section: &str, declared: &[Relation]) -> Around {
    let mut out = Around {
        references: Vec::new(),
        consumers: Vec::new(),
        card_ghosts: Vec::new(),
        here: Vec::new(),
        aliases: HashMap::new(),
    };
    if !project.directory.is_sectioned() {
        return out;
    }
    let outside: Vec<&Card> = project
        .detail
        .schemas
        .iter()
        .map(|c| &**c)
        .filter(|c| project.section_of(c) != section)
        .collect();
    let mut where_: HashMap<&str, String> = HashMap::new();
    for c in &project.detail.schemas {
        where_.insert(c.id.as_str(), project.section_of(c));
    }
    let (mut references, mut consumers) = (HashSet::new(), HashSet::new());
    for r in drawn(project).chain(declared.iter()) {
        let (Some(from), Some(to)) = (
            where_.get(r.from_schema_id.as_str()),
            where_.get(r.to_schema_id.as_str()),
        ) else {
            continue;
        };
        if from == to {
            continue;
        }
        if from == section {
            references.insert(r.to_schema_id.clone());
        }
        if to == section {
            consumers.insert(r.from_schema_id.clone());
        }
    }
    for id in &references {
        consumers.remove(id);
    }
    out.references = outside
        .iter()
        .filter(|c| references.contains(&c.id))
        .map(|c| c.id.clone())
        .collect();
    out.consumers = outside
        .iter()
        .filter(|c| consumers.contains(&c.id))
        .map(|c| c.id.clone())
        .collect();
    // The cards its card ghosts lead to, each once, in the order the ghosts are listed.
    let ghosts = project.ghosts_in(section);
    for ghost in &ghosts {
        let Some(card) = project.ghost_card(ghost) else {
            continue;
        };
        if project.section_of(card) == section {
            continue;
        }
        if !out.card_ghosts.contains(&card.id) {
            out.card_ghosts.push(card.id.clone());
        }
        if ghost.name != card.name {
            out.aliases
                .entry(card.id.clone())
                .or_default()
                .push(project.full_ghost(ghost));
        }
    }
    out.card_ghosts.sort_by_key(|id| {
        outside
            .iter()
            .position(|c| &c.id == id)
            .unwrap_or(usize::MAX)
    });
    out.here = project
        .directory
        .members(section)
        .iter()
        .map(|&i| project.nth(i))
        .filter(|c| (c.kind == "value" || c.kind == "enum") && c.standalone)
        .map(|c| c.id.clone())
        .collect();
    out
}

// MARK: - The class names

fn short_digest(text: &str) -> String {
    Sha256::digest(text.as_bytes())[..4]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Where one section's files go: its name when it is a plain ASCII segment, else what
/// is left of it and a short digest.
fn section_base(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
        .collect();
    let cleaned = cleaned.trim_matches(|c| c == '-' || c == '_').to_string();
    if cleaned == name {
        return cleaned;
    }
    let mark = short_digest(name);
    if cleaned.chars().count() >= 3 {
        format!("{cleaned}-{mark}")
    } else {
        format!("section-{mark}")
    }
}

/// Each section's base, its parents' before it, the root left out.
fn section_bases(detail: &Detail) -> HashMap<String, String> {
    let mut own: HashMap<&str, String> = HashMap::new();
    let mut used: HashMap<String, HashSet<String>> = HashMap::new();
    for s in &detail.sections {
        let siblings = s.parent_id.clone().unwrap_or_default();
        let mut base = section_base(&s.name);
        if !used
            .entry(siblings.clone())
            .or_default()
            .insert(base.to_lowercase())
        {
            base = format!("{base}-{}", short_digest(&s.id));
            used.entry(siblings)
                .or_default()
                .insert(base.to_lowercase());
        }
        own.insert(s.id.as_str(), base);
    }
    let by_id: HashMap<&str, &crate::model::Section> =
        detail.sections.iter().map(|s| (s.id.as_str(), s)).collect();
    let mut out = HashMap::new();
    for s in &detail.sections {
        let mut parts = vec![
            own.get(s.id.as_str())
                .cloned()
                .unwrap_or_else(|| "_".into()),
        ];
        let mut parent = s.parent_id.clone();
        let mut seen: HashSet<String> = HashSet::from([s.id.clone()]);
        while let Some(id) = parent {
            let Some(above) = by_id.get(id.as_str()) else {
                break;
            };
            if !seen.insert(id.clone()) || above.parent_id.is_none() {
                break;
            }
            parts.insert(
                0,
                own.get(id.as_str()).cloned().unwrap_or_else(|| "_".into()),
            );
            parent = above.parent_id.clone();
        }
        out.insert(s.id.clone(), parts.join("/"));
    }
    out
}

/// The class name mermaid sees: the card's name, or with sections `Section__Name` — of what a
/// class name may hold, and told apart with a number where two cards come to one (`a b` and
/// `a_b`, two names of letters of other scripts).
fn identifiers(project: &Project) -> HashMap<String, String> {
    let sectioned = project.directory.is_sectioned();
    let bases = if sectioned {
        section_bases(project.detail)
    } else {
        HashMap::new()
    };
    let mut taken: HashSet<String> = HashSet::new();
    project
        .detail
        .schemas
        .iter()
        .map(|c| {
            let base = if sectioned {
                let section = bases
                    .get(&project.section_of(c))
                    .cloned()
                    .unwrap_or_else(|| "_".into());
                format!("{}__{}", ident::safe(&section), ident::safe(&c.name))
            } else {
                ident::safe(&c.name)
            };
            let mut id = base.clone();
            let mut n = 2;
            while !taken.insert(id.clone()) {
                id = format!("{base}_{n}");
                n += 1;
            }
            (c.id.clone(), id)
        })
        .collect()
}

/// `localeCompare`'s order for names: punctuation, then digits, then letters without
/// case — lower before upper where that is all that differs.
fn collate(a: &str, b: &str) -> Ordering {
    fn primary(c: char) -> (u8, u32) {
        if c.is_ascii_alphabetic() {
            (3, c.to_ascii_lowercase() as u32)
        } else if c.is_ascii_digit() {
            (2, c as u32)
        } else if c.is_ascii() {
            (1, c as u32)
        } else {
            (4, c as u32)
        }
    }
    let pa: Vec<(u8, u32)> = a.chars().map(primary).collect();
    let pb: Vec<(u8, u32)> = b.chars().map(primary).collect();
    pa.cmp(&pb).then_with(|| {
        for (x, y) in a.chars().zip(b.chars()) {
            if x != y {
                // Lower case first.
                return y.is_ascii_lowercase().cmp(&x.is_ascii_lowercase());
            }
        }
        Ordering::Equal
    })
}

// MARK: - The subgraph

struct Node {
    /// The class name mermaid sees (`identifiers`).
    name: String,
    kind: String,
    /// The top-level field lines, when they are asked for.
    members: Vec<String>,
    /// With sections: what the class is labelled with (`Core:User`), and its section.
    title: Option<String>,
    section: Option<String>,
    /// Without sections: the card's name, where its class name is not it (`release notes.md`).
    label: Option<String>,
    /// The card ghosts it is drawn for, where their names are not the card's (in full;
    /// by name alone as it is written out).
    aliases: Vec<String>,
}

struct Edge {
    from: String,
    to: String,
    path: String,
    kind: String,
    cardinality: Option<String>,
    array: bool,
    landing: Option<String>,
}

struct Fan {
    at: String,
    path: String,
    /// The kind of line, by its name.
    kind: String,
    /// The cards it reaches, by their paths (a name alone, without sections).
    targets: Vec<String>,
}

/// What a spec draws (`Subgraph`): the classes and the lines, a field naming many cards
/// said as one note — and the roots that are not there, or that the tags left out.
pub struct Subgraph {
    nodes: Vec<Node>,
    edges: Vec<Edge>,
    fans: Vec<Fan>,
    /// The kinds of line that took part.
    kinds: Vec<String>,
    pub missing_roots: Vec<String>,
    pub excluded_roots: Vec<String>,
    /// Too large to draw: nothing in it, drawn as a note saying why.
    pub too_large: Option<TooLarge>,
}

impl Subgraph {
    /// How many cards it draws.
    pub fn cards(&self) -> usize {
        self.nodes.len()
    }
}

const KNOWN: [&str; 11] = [
    "embed",
    "digest",
    "lookup",
    "in",
    "signedBy",
    "verifiedBy",
    "reads",
    "writes",
    "next",
    "note",
    "mentions",
];

/// Every kind this project could draw, in the legend's order.
fn line_kinds(project: &Project, declared: &[Relation]) -> Vec<String> {
    let used: HashSet<&str> = drawn(project)
        .chain(declared.iter())
        .map(|r| r.ref_kind.as_str())
        .collect();
    let mut out: Vec<String> = KNOWN
        .iter()
        .filter(|k| used.contains(*k))
        .map(|k| k.to_string())
        .collect();
    let mut rest: Vec<&str> = used
        .iter()
        .filter(|k| !KNOWN.contains(k))
        .copied()
        .collect();
    rest.sort();
    out.extend(rest.into_iter().map(str::to_string));
    out
}

/// Every kind of line this project could draw, in the legend's order (`availableKinds`).
pub fn available_kinds(project: &Project) -> Vec<String> {
    line_kinds(project, &declared_edges(project))
}

/// A kind of line as MCP says it: the line's own name, `points` for what the drawing calls
/// `lookup` (the line is `points` everywhere else).
pub fn said_kind(kind: &str) -> &str {
    if kind == "lookup" { "points" } else { kind }
}

/// A kind of line MCP named, as the drawing keeps it.
pub fn kept_kind(kind: &str) -> &str {
    if kind == "points" { "lookup" } else { kind }
}

/// The tags names stand for, groups followed to the tags in them (`expandTags`).
fn expand_tags(groups: &[TagGroup], selected: &[String]) -> Vec<String> {
    fn walk(
        groups: &[TagGroup],
        names: &[String],
        seen: &HashSet<String>,
        out: &mut BTreeSet<String>,
    ) {
        for name in names {
            let Some(group) = groups.iter().find(|g| &g.name == name) else {
                out.insert(name.clone());
                continue;
            };
            // A cycle resolves to what it reached.
            if seen.contains(name) {
                continue;
            }
            let mut deeper = seen.clone();
            deeper.insert(name.clone());
            walk(groups, &group.tags, &deeper, out);
        }
    }
    let mut out = BTreeSet::new();
    walk(groups, selected, &HashSet::new(), &mut out);
    out.into_iter().collect()
}

/// A field that designates many cards is said once, as a note. A card's own line (leaving
/// no row) or any other kind of line is drawn one by one: a note could not tell them apart.
fn fans(project: &Project, r: &Relation) -> bool {
    r.ref_kind != "embed"
        && r.ref_kind != "mentions"
        && r.ref_kind != "signedBy"
        && r.ref_kind != "verifiedBy"
        && !r.path.is_empty()
        && project
            .defs
            .line(&r.line)
            .is_some_and(|d| d.base == crate::kinds::Base::Designate)
}

/// Edges between the cards that are drawn — through value objects and enums that are
/// not, which only carry (`collectEdges`). Only those: a flow, a document or any other card
/// left undrawn ends the way, else a line would be drawn for every route through a web of
/// them, and those routes grow as the web's lines to the sixth power.
struct Collector<'r, 'f> {
    from: HashMap<&'r str, Vec<&'r Relation>>,
    keep: &'r HashSet<String>,
    /// Whether a card is folded into what holds it (`select`'s own test, so the cards a hop
    /// goes through and the cards a line is carried through are the same).
    folded: &'f dyn Fn(&str) -> bool,
    out: BTreeMap<String, Relation>,
    /// Lines looked at so far, against `most`.
    steps: usize,
    most: usize,
}

/// More lines to follow through folded cards than a diagram is drawn with: even value objects
/// and enums alone, each holding many of the next, multiply their ways.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TooLarge;

impl TooLarge {
    /// In words, in the language of the moment.
    pub fn said(self) -> String {
        text!("exportDocs.mermaid.tooLarge", count = charts::MOST_STEPS)
    }
}

impl Collector<'_, '_> {
    fn carrier(&self, id: &str) -> bool {
        !self.keep.contains(id) && (self.folded)(id)
    }

    /// The lines from `at` on to the cards drawn, through folded cards; false once more than
    /// `most` lines have been looked at.
    fn walk(
        &mut self,
        origin: &str,
        at: &str,
        path: &str,
        seen: &HashSet<String>,
        hops: usize,
    ) -> bool {
        let list: Vec<&Relation> = self.from.get(at).cloned().unwrap_or_default();
        for r in list {
            self.steps += 1;
            if self.steps > self.most {
                return false;
            }
            let next = format!("{path}.{}", r.path);
            if self.keep.contains(&r.to_schema_id) {
                let key = format!(
                    "{origin}|{}|{next}|{}|{}",
                    r.to_schema_id,
                    r.ref_kind,
                    r.ref_target.clone().unwrap_or_default()
                );
                self.out.entry(key).or_insert_with(|| {
                    let mut carried = r.clone();
                    carried.from_schema_id = origin.to_string();
                    carried.path = next.strip_prefix('.').unwrap_or(&next).to_string();
                    carried
                });
                continue;
            }
            if !self.carrier(&r.to_schema_id) || seen.contains(&r.to_schema_id) || hops >= 6 {
                continue;
            }
            let mut deeper = seen.clone();
            deeper.insert(r.to_schema_id.clone());
            if !self.walk(origin, &r.to_schema_id, &next, &deeper, hops + 1) {
                return false;
            }
        }
        true
    }
}

/// The lines between the cards of `keep`, carried through the cards `folded` says are folded —
/// or `TooLarge` once more than `most` lines have been looked at.
fn collect_edges(
    project: &Project,
    relations: &[Relation],
    keep: &HashSet<String>,
    folded: &dyn Fn(&str) -> bool,
    most: usize,
) -> Result<Vec<Relation>, TooLarge> {
    let mut from: HashMap<&str, Vec<&Relation>> = HashMap::new();
    for r in relations {
        from.entry(r.from_schema_id.as_str()).or_default().push(r);
    }
    let mut collector = Collector {
        from,
        keep,
        folded,
        out: BTreeMap::new(),
        steps: 0,
        most,
    };
    for card in &project.detail.schemas {
        if keep.contains(&card.id)
            && !collector.walk(&card.id, &card.id, "", &HashSet::from([card.id.clone()]), 0)
        {
            return Err(TooLarge);
        }
    }
    Ok(collector.out.into_values().collect())
}

/// A card's top-level field lines — an enum's values: each type and name, a reference
/// typed by the card it holds (`members`).
fn members(card: &Card, project: &Project, plain: bool) -> Vec<String> {
    if card.kind == "enum" {
        return card.values.iter().map(|v| v.value.clone()).collect();
    }
    let here = project.section_of(card);
    card.fields
        .iter()
        .map(|f| {
            let kind = if f.is_ref() {
                // `Core.Address`: on a member line mermaid reads a colon as a label.
                f.ref_schema_id
                    .as_deref()
                    .and_then(|id| project.card(id))
                    .map(|t| {
                        if plain {
                            ident::member_text(&t.name)
                        } else {
                            let written = project.exported_from(t, &here);
                            ident::member_text(&match crate::pathtext::split_section(&written) {
                                (Some(section), name) => format!("{section}.{name}"),
                                (None, name) => name.to_string(),
                            })
                        }
                    })
                    .unwrap_or_else(|| "unknown".into())
            } else {
                f.kind.clone()
            };
            format!(
                "{kind}{} {}",
                if f.is_array { "[]" } else { "" },
                ident::member_text(&f.name)
            )
        })
        .collect()
}

/// What `spec` draws of the project (`selectSubgraph`).
pub fn select(project: &Project, spec: &Spec) -> Subgraph {
    let ids = identifiers(project);
    let name = |id: &str| ids.get(id).cloned().unwrap_or_else(|| id.to_string());
    let kind_of: HashMap<&str, &str> = project
        .detail
        .schemas
        .iter()
        .map(|c| (c.id.as_str(), c.kind.as_str()))
        .collect();

    // A tag filter takes cards out of the graph, so a hop never goes through one left out.
    let wanted = expand_tags(&project.detail.project.tag_groups, &spec.tags);
    let eligible: Option<HashSet<&str>> = (!wanted.is_empty()).then(|| {
        project
            .detail
            .schemas
            .iter()
            .filter(|c| c.tags.iter().any(|t| wanted.contains(t)))
            .map(|c| c.id.as_str())
            .collect()
    });
    let allowed = |id: &str| eligible.as_ref().is_none_or(|e| e.contains(id));

    let declared = declared_edges(project);
    let available = line_kinds(project, &declared);
    let kinds: Vec<String> = if spec.kinds.is_empty() {
        available
    } else {
        available
            .into_iter()
            .filter(|k| spec.kinds.contains(k))
            .collect()
    };
    let by_kind: Vec<Relation> = drawn(project)
        .chain(declared.iter())
        .filter(|r| kinds.contains(&r.ref_kind))
        .cloned()
        .collect();
    let relations: Vec<&Relation> = by_kind
        .iter()
        .filter(|r| allowed(&r.from_schema_id) && allowed(&r.to_schema_id))
        .collect();

    // A bare root is read from the section asked for.
    let found: Vec<(&String, Option<&Card>)> = spec
        .roots
        .iter()
        .map(|r| {
            (
                r,
                project
                    .directory
                    .find(r, spec.section.as_deref())
                    .map(|i| project.nth(i)),
            )
        })
        .collect();
    let missing_roots: Vec<String> = found
        .iter()
        .filter(|(_, c)| c.is_none())
        .map(|(r, _)| (*r).clone())
        .collect();
    let excluded_roots: Vec<String> = found
        .iter()
        .filter(|(_, c)| c.is_some_and(|c| !allowed(&c.id)))
        .map(|(r, _)| (*r).clone())
        .collect();
    let root_ids: Vec<String> = found
        .iter()
        .filter_map(|(_, c)| c.map(|c| c.id.clone()))
        .filter(|id| allowed(id))
        .collect();

    // Values and enums are drawn inside what holds them: a class of their own only when
    // asked for — a root, or a card ghost of the section drawn.
    let mut explicit: HashSet<String> = root_ids.iter().cloned().collect();
    let in_section = spec
        .section
        .as_ref()
        .filter(|_| spec.roots.is_empty() && project.directory.is_sectioned());
    let around = in_section.map(|s| surroundings(project, s, &declared));
    let aliases: HashMap<String, Vec<String>> = around
        .as_ref()
        .map(|a| a.aliases.clone())
        .unwrap_or_default();
    if let Some(around) = &around {
        explicit.extend(around.card_ghosts.iter().cloned());
        explicit.extend(around.here.iter().cloned());
    }
    let folded = |id: &str| {
        matches!(kind_of.get(id), Some(&"value") | Some(&"enum")) && !explicit.contains(id)
    };

    // The section and what it touches of the others, lines only with an end in it.
    let mut home: Option<HashSet<String>> = None;
    let keep: HashSet<String> = if let (Some(section), Some(around)) = (in_section, around) {
        let members: Vec<String> = project
            .directory
            .members(section)
            .iter()
            .map(|&i| project.nth(i).id.clone())
            .collect();
        home = Some(members.iter().cloned().collect());
        members
            .into_iter()
            .chain(around.references)
            .chain(around.consumers)
            .chain(around.card_ghosts)
            .filter(|id| allowed(id) && !folded(id))
            .collect()
    } else if spec.roots.is_empty() {
        project
            .detail
            .schemas
            .iter()
            .map(|c| c.id.clone())
            .filter(|id| allowed(id) && !folded(id))
            .collect()
    } else {
        let mut keep: HashSet<String> = root_ids.iter().cloned().collect();
        let mut frontier = keep.clone();
        for _ in 0..spec.depth {
            // A folded one is inside what holds it, so a hop goes through it to whatever
            // it holds or is held by.
            let mut next: HashSet<String> = HashSet::new();
            let mut pending: Vec<String> = frontier.iter().cloned().collect();
            let mut through: HashSet<String> = HashSet::new();
            while let Some(at) = pending.pop() {
                for r in &relations {
                    let mut reached = Vec::new();
                    if spec.direction != Direction::In && r.from_schema_id == at {
                        reached.push(&r.to_schema_id);
                    }
                    if spec.direction != Direction::Out && r.to_schema_id == at {
                        reached.push(&r.from_schema_id);
                    }
                    for id in reached {
                        if keep.contains(id) {
                            continue;
                        }
                        if folded(id) {
                            if through.insert(id.clone()) {
                                pending.push(id.clone());
                            }
                        } else {
                            next.insert(id.clone());
                        }
                    }
                }
            }
            if next.is_empty() {
                break;
            }
            keep.extend(next.iter().cloned());
            frontier = next;
        }
        keep
    };

    let collected = match collect_edges(project, &by_kind, &keep, &folded, charts::MOST_STEPS) {
        Ok(collected) => collected,
        Err(why) => {
            return Subgraph {
                nodes: Vec::new(),
                edges: Vec::new(),
                fans: Vec::new(),
                kinds,
                missing_roots,
                excluded_roots,
                too_large: Some(why),
            };
        }
    };
    let collected: Vec<Relation> = collected
        .into_iter()
        .filter(|r| {
            home.as_ref()
                .map(|h| h.contains(&r.from_schema_id) || h.contains(&r.to_schema_id))
                .unwrap_or(true)
        })
        .collect();
    // Counted before the filter, so a note says how many there really are.
    let fan_key = |r: &Relation| format!("{}|{}|{}", r.from_schema_id, r.path, r.line);
    let mut counts: HashMap<String, usize> = HashMap::new();
    for r in collected.iter().filter(|r| fans(project, r)) {
        *counts.entry(fan_key(r)).or_default() += 1;
    }
    let wide = |r: &Relation| fans(project, r) && counts.get(&fan_key(r)).is_some_and(|n| *n >= 4);
    let mut groups: BTreeMap<String, Vec<&Relation>> = BTreeMap::new();
    for r in collected.iter().filter(|r| wide(r)) {
        groups.entry(fan_key(r)).or_default().push(r);
    }
    let sectioned = project.directory.is_sectioned();
    let mut fan_list: Vec<Fan> = groups
        .values()
        .filter_map(|g| {
            let first = g.first()?;
            project.card(&first.from_schema_id)?;
            Some(Fan {
                at: name(&first.from_schema_id),
                path: first.path.clone() + if first.is_array { "[]" } else { "" },
                kind: first.line.clone(),
                targets: {
                    let mut targets: Vec<String> = g
                        .iter()
                        .filter_map(|r| project.card(&r.to_schema_id))
                        .map(|c| {
                            if sectioned {
                                project.exported(c)
                            } else {
                                c.name.clone()
                            }
                        })
                        .collect();
                    targets.sort_by(|a, b| collate(a, b));
                    targets
                },
            })
        })
        .collect();
    fan_list.sort_by(|a, b| {
        if a.at == b.at {
            a.path.cmp(&b.path)
        } else {
            collate(&a.at, &b.at)
        }
    });

    let mut edges: Vec<Edge> = collected
        .iter()
        .filter(|r| !wide(r))
        .map(|r| Edge {
            from: name(&r.from_schema_id),
            to: name(&r.to_schema_id),
            path: r.path.clone(),
            kind: r.ref_kind.clone(),
            cardinality: r.cardinality.clone(),
            array: r.is_array,
            landing: r.ref_target.clone(),
        })
        .collect();
    edges.sort_by(|a, b| {
        if a.from != b.from {
            return collate(&a.from, &b.from);
        }
        if a.path != b.path {
            return collate(&a.path, &b.path);
        }
        collate(&a.to, &b.to)
    });

    // A card with no line says nothing in a diagram of lines — unless it was asked for,
    // by a root or by a tag. A fan's note still counts as a line.
    let mut connected: HashSet<String> = edges
        .iter()
        .flat_map(|e| [e.from.clone(), e.to.clone()])
        .collect();
    connected.extend(fan_list.iter().map(|f| f.at.clone()));
    let root_set: HashSet<&String> = root_ids.iter().collect();
    let asked = !wanted.is_empty();
    let mut nodes: Vec<Node> = project
        .detail
        .schemas
        .iter()
        .filter(|c| keep.contains(&c.id))
        .filter(|c| {
            asked
                || connected.contains(&name(&c.id))
                || root_set.contains(&c.id)
                || (spec.whole_section && home.as_ref().is_some_and(|h| h.contains(&c.id)))
        })
        .map(|c| Node {
            name: name(&c.id),
            kind: c.kind.clone(),
            members: if spec.fields {
                members(c, project, false)
            } else {
                Vec::new()
            },
            title: sectioned.then(|| project.exported(c)),
            section: sectioned.then(|| project.section_of(c)),
            label: (!sectioned && name(&c.id) != c.name).then(|| c.name.clone()),
            aliases: aliases.get(&c.id).cloned().unwrap_or_default(),
        })
        .collect();
    nodes.sort_by(|a, b| collate(&a.name, &b.name));

    Subgraph {
        nodes,
        edges,
        fans: fan_list,
        kinds,
        missing_roots,
        excluded_roots,
        too_large: None,
    }
}

// MARK: - The rev

/// A digest of what `graph` draws as `spec` asked: the same picture, the same rev — of
/// the graph, not of how it is worded, and nothing the picture does not show (`revision`).
pub fn revision(project: &Project, spec: &Spec, graph: &Subgraph) -> String {
    let tags = expand_tags(&project.detail.project.tag_groups, &spec.tags);
    let mut spec_json = serde_json::Map::new();
    spec_json.insert("roots".into(), json!(spec.roots));
    spec_json.insert("depth".into(), json!(spec.depth));
    spec_json.insert("kinds".into(), json!(graph.kinds));
    spec_json.insert("fields".into(), json!(spec.fields));
    spec_json.insert("direction".into(), json!(spec.direction.word()));
    // Left out when empty, so the revs from before tags and sections stand.
    if !tags.is_empty() {
        spec_json.insert("tags".into(), json!(tags));
    }
    if let Some(section) = &spec.section
        && project.detail.sections.iter().any(|s| &s.id == section)
    {
        spec_json.insert("section".into(), json!(project.exported_section(section)));
    }
    let mut canonical = json!({
        "project": project.detail.project.slug,
        "spec": Value::Object(spec_json),
        "nodes": graph.nodes.iter().map(|n| {
            let mut row = vec![json!(n.name), json!(n.kind)];
            row.extend(n.members.iter().map(|m| json!(m)));
            Value::Array(row)
        }).collect::<Vec<_>>(),
        "edges": graph.edges.iter().map(|e| json!([e.from, e.to, e.path, e.kind,
            e.cardinality.clone().map(Value::String).unwrap_or(Value::Null), if e.array { 1 } else { 0 }])).collect::<Vec<_>>(),
    });
    // Left out when it is drawn, so the revs of the diagrams drawn stand.
    if graph.too_large.is_some() {
        canonical["tooLarge"] = Value::Bool(true);
    }
    let digest = Sha256::digest(ojson::compact(&canonical).as_bytes());
    digest
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>()[..12]
        .to_string()
}

// MARK: - Drawing

/// The diagram as `spec` cuts it. `exported`: as it is written out — the classes named
/// as the cards are, no sections, the head only the mark and the rev; else as the app
/// draws it, the head saying what it was drawn from.
pub fn render(project: &Project, spec: &Spec, exported: bool) -> String {
    render_graph(project, spec, select(project, spec), exported)
}

/// `render`, of the graph `select` made of `spec`.
pub fn render_graph(project: &Project, spec: &Spec, graph: Subgraph, exported: bool) -> String {
    if exported {
        let rev = revision(project, spec, &graph);
        let graph = plainly(graph, project, spec);
        return draw(&graph, &format!("%% specboard rev={rev}"), project, true);
    }
    render_drawn(project, spec, &graph, None)
}

/// The graph `select` made of `spec` as the app draws it — or, with `cut`, drawn in place of a
/// bigger one: headed with what `cut` says of that one.
pub fn render_drawn(project: &Project, spec: &Spec, graph: &Subgraph, cut: Option<&Cut>) -> String {
    let rev = match cut {
        Some(cut) => cut.rev.clone(),
        None => revision(project, spec, graph),
    };
    draw(
        graph,
        &head(project, spec, &graph.kinds, &rev, cut),
        project,
        false,
    )
}

/// The head of a diagram as the app draws it: what it was drawn from, for a reader to ask whether
/// the rev still matches — and, drawn in place of a bigger one, that it was (`truncated=true`), that
/// one's characters and cards, and on the next line what to ask to draw less (`hint`).
fn head(project: &Project, spec: &Spec, kinds: &[String], rev: &str, cut: Option<&Cut>) -> String {
    let mut parts = vec![
        format!("project={}", project.detail.project.slug),
        format!(
            "roots={}",
            if spec.roots.is_empty() {
                "*".to_string()
            } else {
                spec.roots.join(",")
            }
        ),
        format!("depth={}", spec.depth),
        format!(
            "kinds={}",
            kinds
                .iter()
                .map(|k| said_kind(k))
                .collect::<Vec<_>>()
                .join(",")
        ),
    ];
    if !spec.tags.is_empty() {
        parts.push(format!("tags={}", spec.tags.join(",")));
    }
    if let Some(section) = &spec.section
        && project.detail.sections.iter().any(|s| &s.id == section)
    {
        parts.push(format!("section={}", project.exported_section(section)));
    }
    parts.push(format!("fields={}", spec.fields));
    // The way followed from the roots: none without them (a section is drawn with what it
    // points at and what points at it).
    if !spec.roots.is_empty() {
        parts.push(format!("direction={}", spec.direction.word()));
    }
    // Before the rev, which stays last on the line.
    if let Some(cut) = cut {
        parts.push("truncated=true".into());
        parts.push(format!("chars={}", cut.chars));
        parts.push(format!("cards={}", cut.cards));
    }
    parts.push(format!("rev={rev}"));
    let head = format!("%% specboard {}", ident::line_comment(&parts.join(" ")));
    match cut {
        Some(cut) => format!("{head}\n%% hint={}", ident::line_comment(&cut.hint)),
        None => head,
    }
}

/// The graph named as it is written out: each class the card's name, told apart with a
/// number where two share one; members typed by the cards' names; no sections.
fn plainly(graph: Subgraph, project: &Project, spec: &Spec) -> Subgraph {
    let ids = identifiers(project);
    let by_name: HashMap<String, &Card> = project
        .detail
        .schemas
        .iter()
        .map(|c| &**c)
        .map(|c| (ids.get(&c.id).cloned().unwrap_or_else(|| c.id.clone()), c))
        .collect();
    let mut renamed: HashMap<String, String> = HashMap::new();
    let mut taken: HashSet<String> = HashSet::new();
    for node in &graph.nodes {
        let base = by_name
            .get(&node.name)
            .map(|c| ident::safe(&c.name))
            .unwrap_or_else(|| node.name.clone());
        let mut n = base.clone();
        let mut next = 2;
        while taken.contains(&n) {
            n = format!("{base}_{next}");
            next += 1;
        }
        taken.insert(n.clone());
        renamed.insert(node.name.clone(), n);
    }
    let rename = |n: &str| renamed.get(n).cloned().unwrap_or_else(|| n.to_string());
    let Subgraph {
        nodes,
        edges,
        fans,
        kinds,
        missing_roots,
        excluded_roots,
        too_large,
    } = graph;
    Subgraph {
        nodes: nodes
            .into_iter()
            .map(|node| Node {
                members: match (spec.fields, by_name.get(&node.name)) {
                    (true, Some(card)) => members(card, project, true),
                    _ => node.members,
                },
                label: by_name
                    .get(&node.name)
                    .filter(|card| rename(&node.name) != card.name)
                    .map(|card| card.name.clone()),
                name: rename(&node.name),
                kind: node.kind,
                title: None,
                section: None,
                // Written out, a ghost by its name alone.
                aliases: node
                    .aliases
                    .iter()
                    .map(|a| crate::pathtext::split_section(a).1.to_string())
                    .collect(),
            })
            .collect(),
        edges: edges
            .into_iter()
            .map(|e| Edge {
                from: rename(&e.from),
                to: rename(&e.to),
                ..e
            })
            .collect(),
        fans: fans
            .into_iter()
            .map(|f| Fan {
                at: rename(&f.at),
                targets: f
                    .targets
                    .iter()
                    .map(|t| crate::pathtext::split_section(t).1.to_string())
                    .collect(),
                ..f
            })
            .collect(),
        kinds,
        missing_roots,
        excluded_roots,
        too_large,
    }
}

/// The graph as mermaid, under its head line.
fn draw(graph: &Subgraph, head: &str, project: &Project, plain: bool) -> String {
    let mut lines = vec![head.to_string(), "classDiagram".to_string()];
    if let Some(why) = graph.too_large {
        lines.push(too_large_note(why));
        return lines.join("\n") + "\n";
    }
    // With sections: each section a namespace, each class labelled with its path; the
    // members and the stereotypes after, where mermaid reads them for any class.
    if graph.nodes.iter().any(|n| n.title.is_some()) {
        let bases = section_bases(project.detail);
        for section in &project.detail.sections {
            let inside: Vec<&Node> = graph
                .nodes
                .iter()
                .filter(|n| n.section.as_deref() == Some(section.id.as_str()))
                .collect();
            if inside.is_empty() {
                continue;
            }
            lines.push(format!(
                "    namespace {} {{",
                ident::safe(bases.get(&section.id).map_or("_", String::as_str))
            ));
            for node in inside {
                lines.push(format!(
                    "        class {}[\"{}\"]",
                    node.name,
                    ident::mermaid_text(node.title.as_deref().unwrap_or(&node.name))
                ));
            }
            lines.push("    }".into());
        }
        for node in &graph.nodes {
            for member in &node.members {
                lines.push(format!("    {} : {member}", node.name));
            }
            if node.kind != "entity" {
                lines.push(format!("    <<{}>> {}", node.kind, node.name));
            }
        }
    }
    for node in graph.nodes.iter().filter(|n| n.title.is_none()) {
        // A card whose name is no class name: labelled with it, its members after.
        if let Some(label) = &node.label {
            lines.push(format!(
                "    class {}[\"{}\"]",
                node.name,
                ident::mermaid_text(label)
            ));
            for member in &node.members {
                lines.push(format!("    {} : {member}", node.name));
            }
        } else if node.members.is_empty() {
            lines.push(format!("    class {}", node.name));
        } else {
            lines.push(format!("    class {} {{", node.name));
            for member in &node.members {
                lines.push(format!("        {member}"));
            }
            lines.push("    }".into());
        }
        if node.kind != "entity" {
            lines.push(format!("    <<{}>> {}", node.kind, node.name));
        }
    }
    for edge in &graph.edges {
        let ends = edge.cardinality.as_deref().and_then(|c| c.split_once(':'));
        let (head, tail) = match ends {
            Some((a, b)) => (format!(" \"{a}\" "), format!(" \"{b}\" ")),
            None => (" ".to_string(), " ".to_string()),
        };
        lines.push(format!(
            "    {}{head}{}{tail}{} : {}",
            edge.from,
            arrow(&edge.kind),
            edge.to,
            ident::mermaid_text(&label(edge, plain))
        ));
    }
    for fan in &graph.fans {
        lines.push(format!(
            "    note for {} \"{}\"",
            fan.at,
            text!(
                "exportDocs.mermaid.anySchemas",
                path = ident::mermaid_text(&fan.path),
                kind = ident::mermaid_text(&fan.kind),
                count = fan.targets.len(),
                targets = ident::mermaid_text(&crate::text::list(&fan.targets))
            )
        ));
    }
    // A card drawn for a card ghost of another name says the ghost's.
    for node in graph.nodes.iter().filter(|n| !n.aliases.is_empty()) {
        lines.push(format!(
            "    note for {} \"{}\"",
            node.name,
            text!(
                "exportDocs.mermaid.cardGhost",
                names = ident::mermaid_text(&crate::text::list(&node.aliases))
            )
        ));
    }
    if graph.nodes.is_empty() {
        lines.push(format!("    %% {}", text!("exportDocs.mermaid.noSchemas")));
    }
    lines.join("\n") + "\n"
}

/// In place of a diagram too large to draw: a note saying why.
fn too_large_note(why: TooLarge) -> String {
    format!(
        "    note \"{}\"",
        ident::mermaid_text(&text!("charts.note.tooLarge", why = why.said()))
    )
}

/// How a line of a kind is drawn.
fn arrow(kind: &str) -> &'static str {
    match kind {
        "embed" => "*--",
        "digest" => "..>",
        "lookup" => "o--",
        _ => "..>",
    }
}

/// The legend's word for a line kind: the name of the kind of line it is drawn for.
fn word(kind: &str) -> &str {
    match kind {
        "run" => "runs",
        "flow.next" => "next",
        "flow.when" => "condition",
        "flow.else" => "else",
        "flow.onError" => "onError",
        "flow.onTimeout" => "onTimeout",
        "note" => "rules",
        other => other,
    }
}

/// What goes after the colon on an edge. `plain`: the landing as it is written out.
fn label(edge: &Edge, plain: bool) -> String {
    let path = edge.path.clone() + if edge.array { "[]" } else { "" };
    if matches!(edge.kind.as_str(), "embed" | "digest" | "lookup") {
        return path;
    }
    let landing = edge.landing.as_ref().filter(|l| !l.is_empty()).map(|l| {
        format!(
            "← {}",
            if plain {
                plain_path(l)
            } else {
                exported_path(l)
            }
        )
    });
    let where_ = [if path.is_empty() { None } else { Some(path) }, landing]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" ");
    if where_.is_empty() {
        word(&edge.kind).to_string()
    } else {
        format!("{} {where_}", word(&edge.kind))
    }
}

// MARK: - A diagram too big for MCP

/// What a diagram drawn in place of a bigger one says of that one: its rev — so a diagram put
/// in a document is told stale as before — how many characters it had and how many cards it drew, and what to ask to draw less.
#[derive(Clone, Debug)]
pub struct Cut {
    pub rev: String,
    pub chars: usize,
    pub cards: usize,
    pub hint: String,
}

/// What `graph` draws, a section to a box (its path and how many of the cards are in it) and the
/// lines between two sections to one a kind, said with how many (`points ×12`); the lines inside a
/// section are left out. Headed as `cut` says, and in `room` characters: the sections that do not fit
/// — the last in the tree's order — are left out, said so.
pub fn render_sections(
    project: &Project,
    spec: &Spec,
    graph: &Subgraph,
    cut: &Cut,
    room: usize,
) -> String {
    let home = project.directory.home.as_str();
    let section_of = |node: &Node| node.section.clone().unwrap_or_else(|| home.to_string());
    let mut counts: HashMap<String, usize> = HashMap::new();
    let mut of_node: HashMap<&str, String> = HashMap::new();
    for node in &graph.nodes {
        let section = section_of(node);
        *counts.entry(section.clone()).or_default() += 1;
        of_node.insert(node.name.as_str(), section);
    }
    // The sections in the tree's order; one not in the tree (the root, without sections) first.
    let mut order: Vec<String> = counts
        .keys()
        .filter(|s| !project.detail.sections.iter().any(|t| &t.id == *s))
        .cloned()
        .collect();
    order.sort();
    order.extend(
        project
            .detail
            .sections
            .iter()
            .filter(|s| counts.contains_key(&s.id))
            .map(|s| s.id.clone()),
    );
    let place: HashMap<&str, usize> = order
        .iter()
        .enumerate()
        .map(|(i, s)| (s.as_str(), i))
        .collect();
    let kind_at = |kind: &str| {
        graph
            .kinds
            .iter()
            .position(|k| k == kind)
            .unwrap_or(graph.kinds.len())
    };
    // The lines between two sections, by their ends and kind, counted.
    let mut bundles: BTreeMap<(usize, usize, usize, String), usize> = BTreeMap::new();
    for edge in &graph.edges {
        let (Some(from), Some(to)) = (
            of_node.get(edge.from.as_str()).map(|s| place[s.as_str()]),
            of_node.get(edge.to.as_str()).map(|s| place[s.as_str()]),
        ) else {
            continue;
        };
        if from != to {
            *bundles
                .entry((from, to, kind_at(&edge.kind), edge.kind.clone()))
                .or_default() += 1;
        }
    }
    let line = |(from, to, _, kind): &(usize, usize, usize, String), count: usize| {
        format!(
            "    s{} {} s{} : {} ×{count}",
            from + 1,
            arrow(kind),
            to + 1,
            said_kind(kind)
        )
    };
    let size = |text: &str| text.chars().count() + 1;
    // What the lines take, each under the later of its ends: drawn when that section's box is.
    let mut under: Vec<usize> = vec![0; order.len()];
    for (key, count) in &bundles {
        under[key.0.max(key.1)] += size(&line(key, *count));
    }
    let head = head(project, spec, &graph.kinds, &cut.rev, Some(cut));
    // Room kept for the note of what is left out.
    let mut used = size(&head) + size("classDiagram") + 40;
    let mut boxes: Vec<String> = Vec::new();
    for (i, section) in order.iter().enumerate() {
        let shown = format!(
            "    class s{}[\"{}\"]",
            i + 1,
            text!(
                "exportDocs.mermaid.sectionBox",
                name = ident::mermaid_text(&project.directory.full_section(section)),
                count = counts[section]
            )
        );
        let cost = size(&shown) + under[i];
        if i > 0 && used + cost > room {
            break;
        }
        used += cost;
        boxes.push(shown);
    }
    let left = order.len() - boxes.len();
    let mut out = vec![head, "classDiagram".to_string()];
    out.extend(boxes);
    // The lines in the order of their ends, then of their kinds.
    out.extend(
        bundles
            .iter()
            .filter(|(key, _)| key.0.max(key.1) < order.len() - left)
            .map(|(key, count)| line(key, *count)),
    );
    if left > 0 {
        out.push(format!(
            "    note \"{}\"",
            text!("exportDocs.mermaid.moreSections", left)
        ));
    }
    if graph.nodes.is_empty() {
        out.push(format!("    %% {}", text!("exportDocs.mermaid.noSchemas")));
    }
    out.join("\n") + "\n"
}

// MARK: - A container's card map

/// A container's card map as mermaid (a goal's, a container kind's the project defines): the cards it holds, numbered in the
/// order they are done in, each said by how far it has come — a check that says done, a choice that
/// says where it is — and its label, then the lines between them (of `kinds`; when none, every kind drawn at first),
/// as the window's card map draws them. `head` is the comment above it.
pub fn card_map(project: &Project, container: &Card, kinds: &[String], head: &str) -> String {
    let inside = project.inner_of(&container.id);
    let ids: HashMap<&str, String> = inside
        .iter()
        .enumerate()
        .map(|(i, c)| (c.id.as_str(), format!("n{}", i + 1)))
        .collect();
    let mut out = vec![head.to_string(), "flowchart LR".to_string()];
    if inside.is_empty() {
        out.push(format!("    %% {}", text!("exportDocs.mermaid.noCards")));
    }
    for (i, card) in inside.iter().enumerate() {
        let name = if card.label.is_empty() {
            card.name.clone()
        } else {
            text!(
                "exportDocs.mermaid.labelAndName",
                label = card.label,
                name = card.name
            )
        };
        out.push(format!(
            "    {}[\"{}. {}{}\"]",
            ids[card.id.as_str()],
            i + 1,
            state_word(project, card),
            ident::mermaid_text(&name)
        ));
    }
    for card in &inside {
        for line in &card.connections {
            // Unnamed, a kind drawn only when asked (`visible: false`) is left out.
            let shown = if kinds.is_empty() {
                project.defs.line(&line.line).is_none_or(|d| d.visible)
            } else {
                kinds.contains(&line.line)
            };
            if !shown {
                continue;
            }
            let Some(to) = line
                .to
                .as_deref()
                .filter(|t| *t != card.id)
                .and_then(|t| ids.get(t))
            else {
                continue;
            };
            let label = project
                .defs
                .line(&line.line)
                .map_or_else(|| line.line.clone(), |d| d.said());
            out.push(format!(
                "    {} -->|{}| {}",
                ids[card.id.as_str()],
                ident::mermaid_text(&label),
                to
            ));
        }
    }
    out.join("\n") + "\n"
}

/// How far a card on a card map has come, said before its name: a check its kind counts as done
/// (a task's 済み) as ☑ or ☐; else a choice that has a value meaning done (a goal's 状態) as its value.
fn state_word(project: &Project, card: &Card) -> String {
    let Some(kind) = project.defs.cards().get(&card.kind) else {
        return String::new();
    };
    let fields = card.form.as_ref().map(|f| &f["fields"]);
    let value = |name: &str| fields.and_then(|f| f.get(name)).cloned();
    use crate::kinds::FieldType;
    if let Some(check) = kind
        .fields
        .iter()
        .find(|f| f.fieldtype == FieldType::Checkbox && !f.done.is_empty())
    {
        let on = value(&check.name).is_some_and(|v| check.done.contains(&v));
        return if on { "☑ ".into() } else { "☐ ".into() };
    }
    if let Some(choice) = kind
        .fields
        .iter()
        .find(|f| f.fieldtype == FieldType::Radio && !f.done.is_empty())
    {
        let said = value(&choice.name)
            .and_then(|v| v.as_str().map(str::to_string))
            .or_else(|| {
                choice
                    .default
                    .as_ref()
                    .and_then(|d| d.as_str().map(str::to_string))
            });
        if let Some(said) = said.filter(|s| !s.is_empty()) {
            // A built-in kind's choice shown in the words of the language the map is drawn in.
            let shown = crate::kinds::builtin()
                .get(&card.kind)
                .filter(|k| k.builtin)
                .and_then(|k| k.field(&choice.name))
                .unwrap_or(choice)
                .option_label(&said)
                .to_string();
            return text!(
                "exportDocs.mermaid.stateValue",
                value = ident::mermaid_text(&shown)
            );
        }
    }
    String::new()
}
