//! The project checked against itself (`declaredPaths`, `declaredProblem`, `PathCheck`,
//! `resolvePath` in the Swift app's Model/Semantics, `FlowGraph` and `FlowNode.problems`,
//! and the diagnostics `MCPViews.summary` reads): the paths each card declares and why
//! one is broken, a path resolved with a hint at what is there, and what the project's
//! summary says about its tags, pointers, shapes nobody uses and flows.

use super::facts;
use super::walk::{self, MissKind, Place};
use crate::export::Project;
use crate::export::paths::{WrittenPath, format_row, inserted_value, landing};
use crate::model::{Card, Field, FlowNode, PointsAt, Step, TagGroup};
use crate::notetext::{self, Kind};
use crate::ojson;
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeSet, HashMap, HashSet};

// MARK: - Walking the field tree

/// Every field, depth first, with the names from the card to it (`walkFields`).
pub(crate) fn walk_fields<'a>(fields: &'a [Field], visit: &mut dyn FnMut(&'a Field, &[String])) {
    fn go<'a>(
        fields: &'a [Field],
        trail: &mut Vec<String>,
        visit: &mut dyn FnMut(&'a Field, &[String]),
    ) {
        for field in fields {
            trail.push(field.name.clone());
            visit(field, trail);
            go(&field.children, trail, visit);
            trail.pop();
        }
    }
    go(fields, &mut Vec::new(), visit);
}

/// How many fields, at any depth (`countFields`).
pub(crate) fn count_fields(fields: &[Field]) -> usize {
    fields.iter().map(|f| 1 + count_fields(&f.children)).sum()
}

/// A card in full with the names inside it: `Core:User.profile.email`.
fn full_with(project: &Project, card: &Card, names: &[String]) -> String {
    names.iter().fold(project.full(card), |at, name| {
        crate::pathtext::member(&at, name)
    })
}

/// The cards in the same section as this one: what a discriminated pointer picks its
/// candidates from (`neighbours`).
fn neighbours<'a>(project: &Project<'a>, card: &Card) -> Vec<&'a Card> {
    project
        .directory
        .members(&project.section_of(card))
        .iter()
        .map(|&i| project.nth(i))
        .collect()
}

/// The field a pointer's candidates are told apart by: the one over there when it is
/// said, else its own (`targetDiscriminator`).
fn target_discriminator(points: &PointsAt) -> Option<&str> {
    points
        .discriminator_field
        .as_deref()
        .or(points.discriminated_by.as_deref())
}

/// The cards that pin a value on a top-level field of this name: what a type-tagged
/// pointer resolves against (`discriminatedCandidates`).
pub(crate) fn discriminated_candidates<'a>(cards: &[&'a Card], field: &str) -> Vec<&'a Card> {
    cards
        .iter()
        .filter(|c| {
            c.fields
                .iter()
                .find(|f| f.name == field)
                .and_then(Field::fixed)
                .is_some()
        })
        .copied()
        .collect()
}

// MARK: - Resolving a path

/// What a path resolves to, or what is there at the level that failed (`PathHit`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct PathHit {
    pub ok: bool,
    /// `schema`, `field`, `row` or `node`.
    pub kind: Option<String>,
    /// The card's kind, the field's type, the row's or the node's kind.
    pub type_name: Option<String>,
    pub hint: Option<String>,
    /// The kind of row a field or a row is one of (`Walk::row`): what a line's end names.
    pub row: Option<String>,
}

fn found(kind: &str, type_name: &str) -> PathHit {
    PathHit {
        ok: true,
        kind: Some(kind.to_string()),
        type_name: Some(type_name.to_string()),
        hint: None,
        row: None,
    }
}

fn missing(hint: String) -> PathHit {
    PathHit {
        ok: false,
        hint: Some(hint),
        ..PathHit::default()
    }
}

/// Without Swift's `.whitespaces` at either end: spaces and tabs, not line breaks.
pub(crate) fn trim_spaces(s: &str) -> &str {
    s.trim_matches(|c: char| {
        c == '\t' || (c.is_whitespace() && !"\n\r\u{0B}\u{0C}\u{85}\u{2028}\u{2029}".contains(c))
    })
}

/// The section written before a card's name, and the rest (`splitSection`).
fn split_section(raw: &str) -> (Option<&str>, &str) {
    match crate::pathtext::split_section(raw) {
        (Some(section), rest) => (Some(trim_spaces(section)), rest),
        (None, rest) => (None, rest),
    }
}

/// A walk's answer, the place it stopped said from `shown` (`resolvePath`'s `hit`).
fn hit(walked: Result<walk::Walk, walk::Miss>, shown: &str) -> PathHit {
    match walked {
        Ok(w) => {
            let mut out = match &w.place {
                // A flow's node is a card of its own, and still a node to what reads paths.
                Place::Card if crate::kinds::inner_only(&w.head.kind) => {
                    found("node", &w.head.kind)
                }
                Place::Card => found("schema", &w.head.kind),
                Place::Field(field) => found("field", &field.kind),
                Place::FormatRow(names) => found(
                    "row",
                    format_row(names, &w.head.rows)
                        .map(|r| r.kind.as_str())
                        .unwrap_or("row"),
                ),
                Place::OwnField(_) => found("field", "own"),
                Place::EnumValue(_) => found("row", "enumValue"),
                Place::Step(_) => found("row", "step"),
                Place::CustomRow(names) => found(
                    "row",
                    facts::custom_row(
                        &crate::pathtext::join_names(names.iter().map(String::as_str)),
                        &w.head.custom_rows,
                    )
                    .map(|r| r.kind.as_str())
                    .unwrap_or("row"),
                ),
            };
            out.row = w.row.clone();
            out
        }
        Err(miss) => {
            let mut at = vec![shown.to_string()];
            at.extend(walk::said_names(miss.holder, &miss.from_head));
            let at = at.join(".");
            let there = if miss.kind == MissKind::Pick {
                miss.there.join(" / ")
            } else {
                miss.there
                    .iter()
                    .map(|n| crate::pathtext::quote(n).into_owned())
                    .collect::<Vec<_>>()
                    .join(" / ")
            };
            let or = |none: String| {
                if there.is_empty() {
                    none
                } else {
                    there.clone()
                }
            };
            missing(match miss.kind {
                MissKind::FormatRow => {
                    if miss.walked.is_empty()
                        || format_row(&miss.walked, &miss.holder.rows)
                            .is_some_and(|r| r.kind == "item")
                    {
                        text!(
                            "check.path.rowsIn",
                            at,
                            there = or(text!("check.path.noRows"))
                        )
                    } else {
                        text!("check.path.notItemRow", at)
                    }
                }
                MissKind::Inner => text!(
                    "check.path.inside",
                    at,
                    there = or(text!("check.path.nothing"))
                ),
                MissKind::Field => text!(
                    "check.path.fieldsUnder",
                    at,
                    there = or(text!("check.path.noFields"))
                ),
                MissKind::CustomRow | MissKind::EnumValue | MissKind::Step => text!(
                    "check.path.rowsUnder",
                    at,
                    there = or(text!("check.path.noRows"))
                ),
                MissKind::Value if miss.there.is_empty() => text!("check.path.noNames", at),
                MissKind::Value => text!(
                    "check.path.fieldsBelow",
                    at,
                    there = or(text!("check.path.nothing"))
                ),
                MissKind::Pick => there,
            })
        }
    }
}

/// Resolves one dotted path (`resolvePath`): `[]` is notation, a `ref` is a step into
/// its target, and a path written in a card (`from`) may be relative to that card.
/// A path starts in the section it names, else in `from`'s, else in `base` — with
/// neither, a project of several sections cannot say which card is meant.
pub(crate) fn resolve_path(
    project: &Project,
    raw: &str,
    from: Option<&Card>,
    base: Option<&str>,
) -> PathHit {
    let here = resolve_in(project, raw, from, base);
    // A path in full written on the board that names main's card: found there.
    if !here.ok
        && crate::pathtext::has_section(raw)
        && let Some(outside) = project.outside.as_deref()
    {
        let there = resolve_in(outside, raw, None, None);
        // Not found there either: said as main says it when its section is main's.
        let mains = crate::pathtext::split_section(raw)
            .0
            .is_some_and(|section| outside.directory.section_named(section, None).is_some());
        if there.ok || mains {
            return there;
        }
    }
    // A path in full into a working directory's section, when the tree was read without what the
    // working directories hold: found there — main's and a branch's, and the board's through main.
    let files = files_beside(project);
    if !here.ok
        && crate::pathtext::has_section(raw)
        && let Some(files) = files
        && let Some(mount) = files.mount_named(&crate::pathtext::first_name(raw))
    {
        let there = resolve_in(files, raw, None, None);
        // Not said missing where the working directory was not read here.
        if !there.ok && mount.kind.is_none() {
            return found("schema", "file");
        }
        return there;
    }
    here
}

/// The working directories as read beside a tree: its own, or main's beside the board.
fn files_beside<'p, 'a>(project: &'p Project<'a>) -> Option<&'p Project<'a>> {
    project
        .files
        .as_deref()
        .or_else(|| project.outside.as_deref().and_then(|o| o.files.as_deref()))
}

/// What a `[[…]]` in a text names: a section when written with its `:` and nothing after it
/// (`Store/Ordering:`), read from the writer's section as a card path's section is, and main's
/// from the board; else a card or a place in one (`resolve_path`). A section is not a place a
/// line, a path field or a rule may name, so `resolve_path` stays cards only.
pub(crate) fn resolve_place(
    project: &Project,
    raw: &str,
    from: Option<&Card>,
    base: Option<&str>,
) -> PathHit {
    let reader = from
        .map(|c| project.section_of(c))
        .or(base.map(str::to_string));
    let section_named = |name: &str| {
        project
            .directory
            .section_named(name, reader.as_deref())
            .map(|id| (project, id))
            .or_else(|| {
                let outside = project.outside.as_deref()?;
                Some((outside, outside.directory.section_named(name, None)?))
            })
            .or_else(|| {
                let files = files_beside(project)?;
                Some((files, files.directory.section_named(name, None)?))
            })
    };
    if let Some(name) = crate::pathtext::section_only(raw) {
        // A folder of a working directory not read here: not said missing, as its files are not.
        if section_named(name).is_none()
            && let Some(files) = files_beside(project)
            && files
                .mount_named(&crate::pathtext::first_name(name))
                .is_some_and(|m| m.kind.is_none())
        {
            return found("section", crate::kinds::WORKDIR);
        }
        return match section_named(name) {
            Some((tree, id)) => PathHit {
                ok: true,
                kind: Some("section".to_string()),
                type_name: tree.section_kind(&id).map(str::to_string),
                hint: None,
                row: None,
            },
            None => missing(text!("check.path.noSection", name)),
        };
    }
    let hit = resolve_path(project, raw, from, base);
    let raw = raw.trim();
    if !hit.ok && !crate::pathtext::has_section(raw) && section_named(raw).is_some() {
        return missing(text!("check.path.sectionColon", path = raw));
    }
    hit
}

/// A path in full resolved in this tree alone: not in main beside the board, as a path written on
/// the board is (`resolve_path`) — what the window opens in a tree is that tree's.
pub(crate) fn resolve_here(project: &Project, raw: &str) -> PathHit {
    resolve_in(project, raw, None, None)
}

/// A path resolved in this tree alone.
fn resolve_in(project: &Project, raw: &str, from: Option<&Card>, base: Option<&str>) -> PathHit {
    if let Some(why) = crate::pathtext::unreadable(raw) {
        return missing(why);
    }
    let Some(written) = WrittenPath::new(raw) else {
        return missing(text!("check.path.empty"));
    };
    let reader = from
        .map(|c| project.section_of(c))
        .or(base.map(str::to_string));
    // Every card's name, for what is said of a path that leads nowhere: made only then, not for
    // each path that resolves (a project
    // of 3,000 cards would copy them all for each of its paths).
    let names = || -> Vec<String> {
        project
            .detail
            .schemas
            .iter()
            .map(|c| c.name.clone())
            .collect()
    };
    // Held to the trees the reader sees when it is a path written in a card.
    let (head, failure) = match (from.is_some(), reader.as_deref()) {
        (true, Some(r)) => match project.directory.find_seeing(&written.card, r) {
            Some(i) => (Some(project.nth(i)), None),
            None => (
                None,
                Some(project.directory.find_problem(&written.card, r, &names())),
            ),
        },
        _ => match project.directory.find(&written.card, reader.as_deref()) {
            Some(i) => (Some(project.nth(i)), None),
            None => (
                None,
                Some(
                    project
                        .directory
                        .find_refusal(&written.card, reader.as_deref(), &names()),
                ),
            ),
        },
    };
    // Past a working directory's file: a function or a type in it, read from the file.
    if let Some(file) = head.filter(|h| h.kind == "file" && !written.names.is_empty()) {
        return symbol_hit(project, file, &written.names);
    }
    let (section, card) = split_section(&written.card);
    let absolute = head.map(|h| hit(walk::walk(h, &written.names, project, true), card));
    if let Some(a) = &absolute
        && a.ok
    {
        return a.clone();
    }
    // Rules and payload exclusions are written from inside their own card.
    if let Some(from) = from
        && section.is_none()
    {
        let mut inside = vec![written.card.clone()];
        inside.extend(written.names.iter().cloned());
        let relative = hit(
            walk::walk(from, &inside, project, true),
            &crate::pathtext::quote(&from.name),
        );
        if relative.ok {
            return relative;
        }
        // Found nowhere, and not inside its own card either: what the directory says of
        // the name (the sections that have it, and a card ghost to write it by its name
        // alone) is what helps, not what the card itself holds.
        if head.is_none() {
            return missing(failure.unwrap_or_else(|| {
                relative
                    .hint
                    .clone()
                    .unwrap_or_else(|| text!("check.path.notFound"))
            }));
        }
    }
    if let Some(a) = absolute {
        return a;
    }
    missing(failure.unwrap_or_default())
}

/// A function or a type in a working directory's file, by its names there: found when the file has
/// it now; not said missing where the file could not be read for its functions and types (its
/// working directory not bound here, a language this machine lacks).
fn symbol_hit(project: &Project, file: &Card, names: &[String]) -> PathHit {
    let plain: Vec<String> = names
        .iter()
        .map(|n| crate::pathtext::plain(n).into_owned())
        .collect();
    let Some(read) = project.code.as_ref().map(|c| c.outline(&file.id, false)) else {
        return found("symbol", "symbol");
    };
    if !read.read_whole() {
        return found("symbol", "symbol");
    }
    match crate::code::files::find(read.symbols(), &plain) {
        Some(symbol) => found("symbol", symbol.of.word()),
        None => {
            let at = crate::pathtext::join_names(plain.iter().map(String::as_str));
            let near = crate::code::files::nearest(read.symbols(), &at, 5);
            missing(text!(
                "check.line.noSymbol",
                file = file.name,
                symbol = at,
                near = if near.is_empty() {
                    text!("code.symbol.noneNear")
                } else {
                    crate::text::list(&near)
                }
            ))
        }
    }
}

// MARK: - Paths the project declares about itself

/// A path a card declares about itself (`DeclaredPath`; an action's `expects` went
/// with the actions).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct DeclaredPath {
    /// Where the declaration lives: `Core:User`, `Core:User.profile`.
    pub at: String,
    /// Which key declared it: `pointsAt.in`, `body`, `nodes.save.steps[0].paths[0]`.
    pub via: String,
    pub path: String,
    /// Why it is broken, once checked.
    pub hint: Option<String>,
    /// A flow's `notes`: a note, a document, or an item of a formatdoc.
    pub writing: bool,
    /// A section of this kind, written with or without the `:` after it.
    pub section: Option<&'static str>,
    /// `{{path}}`: it has to hold a value — a fixed one, or an enum's.
    pub needs_value: bool,
    /// The kinds it has to land on, when it may be one of several.
    pub one_of: Vec<&'static str>,
    /// `![[path]]`: a card or a section to draw, and how.
    pub embeds: bool,
    pub option: Option<String>,
    /// `[[path]]` in a text: a section may be named too (`Store/Ordering:`).
    pub place: bool,
    /// A line to a working directory's file, or a function or type in one, that is not there: code
    /// that may be still to write.
    pub to_code: bool,
}

fn declared(at: &str, via: String, path: &str) -> DeclaredPath {
    DeclaredPath {
        at: at.to_string(),
        via,
        path: path.to_string(),
        ..DeclaredPath::default()
    }
}

/// A reference in Markdown, as a declared path: a value has to hold one, an embed
/// has to draw.
fn from_ref(at: &str, via: String, r: &notetext::Ref) -> DeclaredPath {
    DeclaredPath {
        needs_value: r.kind == Kind::Value,
        embeds: r.kind == Kind::Embed,
        place: r.kind == Kind::Place,
        option: r.option.clone(),
        ..declared(at, via, &r.path)
    }
}

/// What a reference is told apart by, so a text naming one twice declares it once: no
/// option and an empty one are the same.
fn said_once(r: &notetext::Ref) -> (Kind, String, String) {
    (r.kind, r.path.clone(), r.option.clone().unwrap_or_default())
}

/// Every path a card asserts about itself (`declaredPaths`): rule parameters, what a
/// flow's nodes name and decide by, what a note's or a document's Markdown names, a
/// formatdoc's rows' targets and a custom card's joins. Its lines (a field's embedding and
/// pointer, a signature's keys, what a store holds, a flow's rules, a task's joins) are its
/// connections now, and are checked as such (`line_paths`). `as_name` is how `at` spells
/// the card — `Core:User` in full; its own name by default.
pub(crate) fn declared_paths(
    kinds: &crate::kinds::Kinds,
    card: &Card,
    as_name: Option<&str>,
) -> Vec<DeclaredPath> {
    let own = as_name.map_or_else(
        || crate::pathtext::quote(&card.name).into_owned(),
        str::to_string,
    );
    let mut out = Vec::new();
    let from_rules = |at: &str, rules: &[crate::model::Rule], out: &mut Vec<DeclaredPath>| {
        for rule in rules {
            let kind = crate::export::rule_kind(rule);
            for key in crate::export::rule_path_params(kind) {
                if let Some(value) = crate::export::rule_string(rule, key)
                    && value.contains('.')
                {
                    out.push(declared(at, format!("rules.{kind}.{key}"), value));
                }
            }
        }
    };
    from_rules(&own, &card.rules, &mut out);
    // What a description names (the
    // links in a description are checked as a body's are): each reference once, by where it is.
    let in_text = |at: &str, place: &str, text: &str, out: &mut Vec<DeclaredPath>| {
        let mut seen = HashSet::new();
        for r in notetext::refs(text) {
            if !seen.insert(said_once(&r)) {
                continue;
            }
            let via = match r.kind {
                Kind::Place => place.to_string(),
                Kind::Value => format!("{place}{{{{}}}}"),
                Kind::Embed => format!("{place}![[]]"),
            };
            out.push(from_ref(at, via, &r));
        }
    };
    in_text(&own, "description", &card.description, &mut out);
    for line in &card.connections {
        let Some(said) = line.description.as_deref() else {
            continue;
        };
        let at = if line.from.is_empty() {
            own.clone()
        } else {
            format!("{own}.{}", line.from)
        };
        in_text(
            &at,
            &format!("connections.{}.description", line.line),
            said,
            &mut out,
        );
    }
    // A flow's node is a card of its own: the paths its fields and rows
    // write (what it carries and returns, what an eval reads, what a signature covers), and
    // the conditions its lines say. What it runs, waits for, reads and writes are its lines.
    // A stackdoc's versions are its definition's rows too: what each body names.
    if crate::kinds::inner_only(&card.kind) || kinds.is_own(&card.kind) || card.kind == "stackdoc" {
        for (via, path) in form_paths(kinds, card) {
            out.push(declared(&own, via, &path));
        }
        for (via, text) in form_texts(kinds, card) {
            in_text(&own, &via, &text, &mut out);
        }
        for line in card.connections.iter().filter(|l| l.line == "condition") {
            let Some(text) = line.fields.get("when").and_then(Value::as_str) else {
                continue;
            };
            for r in notetext::refs(text) {
                out.push(from_ref(
                    &own,
                    "connections.branch.fields.when".to_string(),
                    &r,
                ));
            }
        }
    }
    let mut mentioned = HashSet::new();
    for r in notetext::refs(&card.body) {
        if !mentioned.insert(said_once(&r)) {
            continue;
        }
        let via = match r.kind {
            Kind::Place => "body",
            Kind::Value => "body{{}}",
            Kind::Embed => "body![[]]",
        };
        out.push(from_ref(&own, via.to_string(), &r));
    }
    // A formatdoc: each row's text, said as where it is, and what its diagrams and
    // tables are of.
    facts::walk_format(&card.rows, &mut |row, path, _| {
        let at = format!(
            "rows.{}",
            crate::pathtext::join_names(path.iter().map(String::as_str))
        );
        for (place, text) in facts::texts(row) {
            let mut seen = HashSet::new();
            let place = format!("{at}.{place}");
            for r in notetext::refs(&text) {
                if !seen.insert(said_once(&r)) {
                    continue;
                }
                let via = match r.kind {
                    Kind::Place => place.clone(),
                    Kind::Value => format!("{place}{{{{}}}}"),
                    Kind::Embed => format!("{place}![[]]"),
                };
                out.push(from_ref(&own, via, &r));
            }
        }
        if row.target.is_empty() {
            return;
        }
        let via = format!("{at}.target");
        match row.kind.as_str() {
            "diagram" => out.push(DeclaredPath {
                embeds: true,
                ..declared(&own, via, &row.target)
            }),
            "diagrams" => out.push(DeclaredPath {
                section: Some(crate::kinds::CHART),
                ..declared(&own, via, &row.target)
            }),
            "card" => out.push(DeclaredPath {
                one_of: vec!["entity", "value", "enum", "instance", "table"],
                ..declared(&own, via, &row.target)
            }),
            _ => {}
        }
    });
    // A custom card: what each row is joined to.
    facts::walk_custom(&card.custom_rows, &mut |row, path| {
        for (index, to) in row.links.iter().enumerate() {
            out.push(declared(&own, format!("rows.{path}.links[{index}]"), to));
        }
    });
    walk_fields(&card.fields, &mut |field, path| {
        let at = format!(
            "{own}.{}",
            crate::pathtext::join_names(path.iter().map(String::as_str))
        );
        from_rules(&at, &field.rules, &mut out);
        in_text(&at, "description", &field.description, &mut out);
    });
    out
}

/// The text a card read by its kind's definition writes in its fields and rows that names places
/// (`paths`: a node's step's description, a field of the project's own kind), each with where
/// (`steps[0].description`). Its own body and description are the card's, read apart.
fn form_texts(kinds: &crate::kinds::Kinds, card: &Card) -> Vec<(String, String)> {
    use crate::kinds::{Field, FieldType};
    let Some(kind) = kinds.get(&card.kind) else {
        return Vec::new();
    };
    let Some(form) = card.form.as_ref() else {
        return Vec::new();
    };
    fn fields(defs: &[Field], values: &Value, at: &str, out: &mut Vec<(String, String)>) {
        for def in defs {
            let Some(v) = values.get(&def.name) else {
                continue;
            };
            if at.is_empty() && (def.name == "body" || def.name == "description") {
                continue;
            }
            let here = if at.is_empty() {
                def.name.clone()
            } else {
                format!("{at}.{}", def.name)
            };
            match def.fieldtype {
                FieldType::Text | FieldType::LongText | FieldType::Markdown
                    if def.paths || def.fieldtype == FieldType::Markdown =>
                {
                    if let Some(text) = v.as_str() {
                        out.push((here, text.to_string()));
                    }
                }
                FieldType::Group => fields(&def.fields, v, &here, out),
                _ => {}
            }
        }
    }
    let mut out = Vec::new();
    fields(&kind.fields, &form["fields"], "", &mut out);
    for row in &kind.rows {
        for (i, item) in form["rows"][&row.name]
            .as_array()
            .into_iter()
            .flatten()
            .enumerate()
        {
            fields(
                &row.fields,
                &item["fields"],
                &format!("{}[{i}]", row.name),
                &mut out,
            );
        }
    }
    out
}

/// The paths a card read by its kind's definition writes as text in its fields and rows
/// (a flow's node): each with where (`carries[0]`, `steps[1].paths[0]`, `return.carries[0]`).
fn form_paths(kinds: &crate::kinds::Kinds, card: &Card) -> Vec<(String, String)> {
    use crate::kinds::{Field, FieldType};
    let Some(kind) = kinds.get(&card.kind) else {
        return Vec::new();
    };
    let Some(form) = card.form.as_ref() else {
        return Vec::new();
    };
    fn fields(defs: &[Field], values: &Value, at: &str, out: &mut Vec<(String, String)>) {
        for def in defs {
            let Some(v) = values.get(&def.name) else {
                continue;
            };
            let here = if at.is_empty() {
                def.name.clone()
            } else {
                format!("{at}.{}", def.name)
            };
            match def.fieldtype {
                FieldType::Path if !def.by_id => match v {
                    Value::String(s) => out.push((here, s.clone())),
                    Value::Array(list) => {
                        for (i, s) in list.iter().enumerate() {
                            if let Some(s) = s.as_str() {
                                out.push((format!("{here}[{i}]"), s.to_string()));
                            }
                        }
                    }
                    _ => {}
                },
                FieldType::Group => fields(&def.fields, v, &here, out),
                _ => {}
            }
        }
    }
    let mut out = Vec::new();
    fields(&kind.fields, &form["fields"], "", &mut out);
    for row in &kind.rows {
        for (i, item) in form["rows"][&row.name]
            .as_array()
            .into_iter()
            .flatten()
            .enumerate()
        {
            fields(
                &row.fields,
                &item["fields"],
                &format!("{}[{i}]", row.name),
                &mut out,
            );
        }
    }
    out
}

/// Why a line's landing inside a working directory's file (`at`) is not there: said only where the
/// file was read for its functions and types now, and none of that path is in it — with the names
/// nearest it there.
pub(crate) fn no_symbol(project: &Project, file_id: &str, at: &str) -> Option<String> {
    let read = project.code.as_ref()?.outline(file_id, false);
    if !read.read_whole() {
        return None;
    }
    let names = crate::pathtext::names(at);
    if crate::code::files::find(read.symbols(), &names).is_some() {
        return None;
    }
    let file = project
        .file_card(file_id)
        .map(|c| c.name.clone())
        .unwrap_or_default();
    let near = crate::code::files::nearest(read.symbols(), at, 5);
    Some(text!(
        "check.line.noSymbol",
        file,
        symbol = at,
        near = if near.is_empty() {
            text!("code.symbol.noneNear")
        } else {
            crate::text::list(&near)
        }
    ))
}

/// A card's lines that lead nowhere, each as written (`bare`, the card by its name) and
/// in full (`named`, with why): one whose card is gone, or a card ghost that leads to none;
/// a row it leaves or a place it lands on that is not there; a path its fields hold that
/// does not resolve. Whether it fits its kind of line is apart (`lines::misfits`).
pub(crate) fn line_paths(project: &Project, card: &Card) -> Vec<(DeclaredPath, DeclaredPath)> {
    let full = project.full(card);
    let name = crate::pathtext::quote(&card.name).into_owned();
    let mut out = Vec::new();
    let mut push = |via: String, path: String, why: String, bare_at: &str, named_at: &str| {
        out.push((
            declared(bare_at, via.clone(), &path),
            DeclaredPath {
                hint: Some(why),
                ..declared(named_at, via, &path)
            },
        ));
        out.len() - 1
    };
    let mut to_code: Vec<usize> = Vec::new();
    for line in &card.connections {
        let (bare_at, named_at) = if line.from.is_empty() {
            (name.clone(), full.clone())
        } else {
            (
                format!("{name}.{}", line.from),
                format!("{full}.{}", line.from),
            )
        };
        let via = format!("connections.{}", line.line);
        // A data card's shape is its key `of`, not one of its lines.
        let to_key = if line.line == crate::kinds::OF_LINE {
            "of".to_string()
        } else {
            format!("{via}.to")
        };
        // The row it leaves: a field, a formatdoc's row, or a node's row by its place.
        // (A data card's `steps[0]` is an element of its array `steps`.)
        if let Some(i) =
            crate::detail::step_index(&line.from).filter(|_| card.kind != crate::kinds::INSTANCE)
        {
            let rows = card
                .form
                .as_ref()
                .and_then(|f| f["rows"]["steps"].as_array())
                .map_or(0, Vec::len);
            if i >= rows {
                let path = format!("{full}.{}", line.from);
                let why = text!("check.line.noRow", name = card.name, rows, from = line.from);
                push(format!("{via}.from"), path, why, &bare_at, &named_at);
            }
        } else if !line.from.is_empty() {
            let names: Vec<String> = super::picks::split(&line.from);
            if let Err(miss) = walk::walk(card, &names, project, false) {
                let path = format!("{full}.{}", line.from);
                let why = hit(Err(miss), &name).hint.unwrap_or_default();
                push(format!("{via}.from"), path, why, &bare_at, &named_at);
            } else if let Some(why) = unpicked(card, &names) {
                let path = format!("{full}.{}", line.from);
                push(format!("{via}.from"), path, why, &bare_at, &named_at);
            }
        }
        // The card it reaches.
        let Some(to) = line.to.as_deref() else {
            continue;
        };
        // A board's card's line to a working directory's file, through main: there — or said not
        // there, where its working directory was read on this machine.
        if to.starts_with("file:")
            && project.card(to).is_none()
            && let Some(outside) = project.outside.as_deref()
            && outside.files.is_some()
        {
            if outside.file_card(to).is_none() && outside.file_dir_read(to) {
                let written = outside.file_written(to).unwrap_or_else(|| to.to_string());
                push(
                    format!("{via}.to"),
                    written,
                    text!("check.line.noFile"),
                    &bare_at,
                    &named_at,
                );
            } else if let Some(at) = line.at.as_deref().filter(|a| !a.is_empty())
                && outside.file_card(to).is_some()
                && let Some(why) = no_symbol(outside, to, at)
            {
                let written = outside.file_written(to).unwrap_or_else(|| to.to_string());
                push(
                    format!("{via}.to"),
                    format!("{written}.{at}"),
                    why,
                    &bare_at,
                    &named_at,
                );
            }
            continue;
        }
        // A board's card's line to main's card: found there.
        if project.card(to).is_none() && project.outside_card(to).is_some() {
            continue;
        }
        // main's or a branch's line to a working directory's file: there — or said not there, where its working directory was read
        // on this machine (one not bound here says nothing of its files).
        if to.starts_with("file:") && project.files.is_some() {
            if project.file_card(to).is_none() && project.file_dir_read(to) {
                let written = project.file_written(to).unwrap_or_else(|| to.to_string());
                to_code.push(push(
                    format!("{via}.to"),
                    written,
                    text!("check.line.noFile"),
                    &bare_at,
                    &named_at,
                ));
            } else if let Some(at) = line.at.as_deref().filter(|a| !a.is_empty())
                && project.file_card(to).is_some()
                && let Some(why) = no_symbol(project, to, at)
            {
                let written = project.file_written(to).unwrap_or_else(|| to.to_string());
                to_code.push(push(
                    format!("{via}.to"),
                    format!("{written}.{at}"),
                    why,
                    &bare_at,
                    &named_at,
                ));
            }
            continue;
        }
        let Some(target) = project.card(to) else {
            let (path, why) = match project.ghost(to) {
                Some(ghost) => (
                    project.full_ghost(ghost),
                    project
                        .directory
                        .entry_of(&ghost.id)
                        .map(|e| project.directory.dead_end(e, &project.full_ghost(ghost)))
                        .unwrap_or_default(),
                ),
                // By the path it had, as the history kept it
                // (an empty path would say nothing of which line).
                None if line.line == crate::kinds::OF_LINE => (
                    project.gone.get(to).cloned().unwrap_or_default(),
                    text!("check.line.noShape"),
                ),
                None => (
                    project.gone.get(to).cloned().unwrap_or_default(),
                    text!("check.line.noPartner"),
                ),
            };
            push(to_key, path, why, &bare_at, &named_at);
            continue;
        };
        // Where it lands.
        if let Some(at) = &line.at {
            let path = if crate::pathtext::has_section(at) {
                at.clone()
            } else {
                format!(
                    "{}.{at}",
                    project.named(to).unwrap_or_else(|| project.full(target))
                )
            };
            let hit = resolve_path(project, &path, Some(card), None);
            if !hit.ok {
                push(
                    format!("{via}.at"),
                    path,
                    hit.hint.unwrap_or_else(|| text!("check.path.notFound")),
                    &bare_at,
                    &named_at,
                );
            } else if !crate::pathtext::has_section(at)
                && let Some(why) = unpicked(target, &super::picks::split(at))
            {
                push(format!("{via}.at"), path, why, &bare_at, &named_at);
            }
        }
        // What its fields hold that are paths.
        let Some(def) = project.defs.line(&line.line) else {
            continue;
        };
        for field in def
            .fields
            .iter()
            .filter(|f| f.fieldtype == crate::kinds::FieldType::Path && !f.by_id)
        {
            let written: Vec<String> = match line.fields.get(&field.name) {
                Some(Value::String(one)) => vec![one.clone()],
                Some(Value::Array(many)) => many
                    .iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect(),
                _ => Vec::new(),
            };
            for (index, path) in written.iter().enumerate() {
                let hit = resolve_path(project, path, Some(card), None);
                if !hit.ok {
                    let where_ = if field.many > 0 {
                        format!("{via}.fields.{}[{index}]", field.name)
                    } else {
                        format!("{via}.fields.{}", field.name)
                    };
                    push(
                        where_,
                        path.clone(),
                        hit.hint.unwrap_or_else(|| text!("check.path.notFound")),
                        &bare_at,
                        &named_at,
                    );
                }
            }
        }
    }
    for i in to_code {
        out[i].0.to_code = true;
        out[i].1.to_code = true;
    }
    out
}

/// Why a line's place in a data card picks no one element of an array there (none of them, two
/// or more, a place past the end), or none when it picks one or picks none at all. Written all the same, as a value not
/// written yet is: said by the check, and by the write as a path that does not resolve.
fn unpicked(card: &Card, names: &[String]) -> Option<String> {
    if card.kind != crate::kinds::INSTANCE || !names.iter().any(|n| n.contains('[')) {
        return None;
    }
    super::instances::walked(card.instance_data(), names)
        .err()
        .map(|(_, why)| why)
}

/// Why a declared path is broken, or none when it is not (`declaredProblem`).
pub(crate) fn declared_problem(
    declared: &DeclaredPath,
    project: &Project,
    card: &Card,
) -> Option<String> {
    declared_problem_from(declared, project, Some(card), &project.section_of(card))
}

/// The same, written by `card` or, with none, in the section `base` (a section's description, or
/// the project's from the root).
fn declared_problem_from(
    declared: &DeclaredPath,
    project: &Project,
    card: Option<&Card>,
    base: &str,
) -> Option<String> {
    let base = base.to_string();
    let path = &declared.path;
    if declared.embeds {
        return crate::export::find_embed(path, declared.option.as_deref(), &base, project).err();
    }
    if let Some(kind) = declared.section {
        let name = path.strip_suffix(':').unwrap_or(path);
        let Some(found) = project.directory.section_from(name, &base) else {
            return Some(text!("check.path.noSection", name));
        };
        let found_kind = project
            .detail
            .sections
            .iter()
            .find(|s| s.id == found)
            .and_then(|s| s.kind.as_deref());
        // Where a chart's section is asked for, an actor's holds flows as one does.
        let fits = found_kind == Some(kind)
            || (kind == crate::kinds::CHART && crate::kinds::holds_flows(found_kind));
        if !fits {
            return Some(text!(
                "check.path.notSectionOf",
                name,
                label = project.defs.section_label(kind),
                found = found_kind
                    .map(|k| text!(
                        "check.path.sectionIs",
                        label = project.defs.section_label(k)
                    ))
                    .unwrap_or_default()
            ));
        }
        return None;
    }
    let base_said = card.is_none().then_some(base.as_str());
    let hit = if declared.place {
        resolve_place(project, path, card, base_said)
    } else {
        resolve_path(project, path, card, base_said)
    };
    if !hit.ok {
        return Some(hit.hint.unwrap_or_else(|| text!("check.path.notFound")));
    }
    if hit.kind.as_deref() == Some("section") {
        return None;
    }
    if !declared.one_of.is_empty() {
        let target = project.find(path, &base);
        if !target.is_some_and(|t| declared.one_of.contains(&t.kind.as_str())) {
            let mut kinds = declared.one_of.clone();
            kinds.sort();
            return Some(text!(
                "check.path.notKinds",
                path,
                kinds = kinds.join(text!("check.path.kindOr").as_str()),
                found = target
                    .map(|t| text!("check.path.kindIs", kind = t.kind))
                    .unwrap_or_else(|| text!("check.path.writeCardName"))
            ));
        }
    }
    if declared.writing {
        let Some((landed, _)) = landing(path, &base, project).filter(|(c, _)| c.is_writing())
        else {
            return Some(text!("check.path.notWriting", path));
        };
        let names = WrittenPath::new(path).map(|w| w.names).unwrap_or_default();
        let an_item = landed.kind == "formatdoc"
            && format_row(&names, &landed.rows).is_some_and(|r| r.kind == "item");
        if !names.is_empty() && !an_item {
            return Some(text!("check.path.onlyItems", path));
        }
    }
    if declared.needs_value
        && let Err(why) = inserted_value(project, path, &base)
    {
        return Some(why);
    }
    None
}

/// Each declared path of `card` that does not resolve, as written (`bare`) and with
/// the card's section (`named`, whose `hint` says why) — `PathCheck.problems`.
pub(crate) fn problems(project: &Project, card: &Card) -> Vec<(DeclaredPath, DeclaredPath)> {
    let full = project.full(card);
    declared_paths(project.defs.cards(), card, None)
        .into_iter()
        .zip(declared_paths(project.defs.cards(), card, Some(&full)))
        .filter_map(|(bare, mut named)| {
            let why = declared_problem(&named, project, card)?;
            named.hint = Some(why);
            Some((bare, named))
        })
        .chain(line_paths(project, card))
        .chain(table_paths(project, card))
        .collect()
}

/// A table's paths that do not resolve against its shape now: each where it is written (`where[0].path`) and why.
fn table_paths(project: &Project, card: &Card) -> Vec<(DeclaredPath, DeclaredPath)> {
    let full = project.full(card);
    super::tables::broken(project, card)
        .into_iter()
        .map(|(via, path, why)| {
            (
                declared(&card.name, via.clone(), &path),
                DeclaredPath {
                    hint: Some(why),
                    ..declared(&full, via, &path)
                },
            )
        })
        .collect()
}

/// Declared paths that no longer resolve, each with its card's section and why
/// (`brokenPaths`); and the card ghosts and the frames that lead nowhere. Empty is the healthy
/// state.
pub(crate) fn broken_paths(project: &Project) -> Vec<DeclaredPath> {
    project
        .detail
        .schemas
        .iter()
        .chain(project.detail.inner.iter())
        .flat_map(|c| problems(project, c).into_iter().map(|(_, named)| named))
        .chain(
            project
                .detail
                .ghosts
                .iter()
                .filter_map(|g| ghost_problem(project, g)),
        )
        .chain(project.detail.schemas.iter().filter_map(|c| {
            let (target, why) = super::frames::broken(project, c)?;
            Some(DeclaredPath {
                hint: Some(why),
                ..declared(&project.full(c), "target".to_string(), &target)
            })
        }))
        .chain(description_problems(project))
        .collect()
}

/// What the sections' descriptions and the project's name that does not resolve: each read from its section, the project's from the
/// root.
pub(crate) fn description_problems(project: &Project) -> Vec<DeclaredPath> {
    let mut out = Vec::new();
    let mut check = |at: String, base: &str, text: &str| {
        let mut seen = HashSet::new();
        for r in notetext::refs(text) {
            if !seen.insert(said_once(&r)) {
                continue;
            }
            let via = match r.kind {
                Kind::Place => "description".to_string(),
                Kind::Value => "description{{}}".to_string(),
                Kind::Embed => "description![[]]".to_string(),
            };
            let mut declared = from_ref(&at, via, &r);
            if let Some(why) = declared_problem_from(&declared, project, None, base) {
                declared.hint = Some(why);
                out.push(declared);
            }
        }
    };
    for section in &project.detail.sections {
        if !section.description.is_empty() {
            check(
                project.directory.full_section(&section.id),
                &section.id,
                &section.description,
            );
        }
    }
    let home = project.directory.home.clone();
    if !project.detail.project.description.is_empty() {
        check(
            project.detail.project.name.clone(),
            &home,
            &project.detail.project.description,
        );
    }
    out
}

/// A card ghost that leads to no card, as a broken path: where it is, what it points at
/// (`target`, in full; empty when that is gone) and why — what it pointed at was removed,
/// or the ghosts it passes go round.
pub(crate) fn ghost_problem(
    project: &Project,
    ghost: &crate::model::Ghost,
) -> Option<DeclaredPath> {
    if project.ghost_card(ghost).is_some() {
        return None;
    }
    let full = project.full_ghost(ghost);
    let target = ghost
        .target
        .as_deref()
        .and_then(|t| project.named(t))
        .unwrap_or_default();
    let why = match project.directory.entry_of(&ghost.id) {
        Some(crate::directory::Entry::Ghost(g)) if project.directory.goes_round(g) => {
            text!("check.ghost.round")
        }
        _ if target.is_empty() => text!("check.ghost.none"),
        _ => text!("check.ghost.noCard", target),
    };
    Some(DeclaredPath {
        hint: Some(why),
        ..declared(&full, "target".to_string(), &target)
    })
}

// MARK: - Tags

/// Every tag in use, most-used first, then by name (`projectTags`).
pub(crate) fn project_tags(cards: &[std::rc::Rc<Card>]) -> Vec<(String, usize)> {
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for card in cards {
        for tag in &card.tags {
            *counts.entry(tag.as_str()).or_default() += 1;
        }
    }
    let mut out: Vec<(String, usize)> = counts
        .into_iter()
        .map(|(t, n)| (t.to_string(), n))
        .collect();
    out.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    out
}

/// The tags a selection of tags and groups stands for, groups opened up (`expandTags`).
pub(crate) fn expand_tags(groups: &[TagGroup], selected: &[String]) -> Vec<String> {
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

/// Tagged cards breaking an exclusive group's rule — none of its tags, or several —
/// and how many they hold (`tagGroupViolations`). Untagged cards take no part.
pub(crate) fn tag_group_violations(project: &Project) -> Vec<(String, String, usize)> {
    let groups = &project.detail.project.tag_groups;
    let mut out = Vec::new();
    for group in groups.iter().filter(|g| g.exclusive) {
        let members: HashSet<String> = expand_tags(groups, std::slice::from_ref(&group.name))
            .into_iter()
            .collect();
        for card in project.detail.schemas.iter().filter(|c| !c.tags.is_empty()) {
            let held = card.tags.iter().filter(|t| members.contains(*t)).count();
            if held != 1 {
                out.push((group.name.clone(), project.full(card), held));
            }
        }
    }
    out
}

// MARK: - Pointers and shapes

/// Pointers with several candidates and nothing to choose between them: no `when`,
/// no `discriminatedBy` (`ambiguousPointers`). Where, and how many.
pub(crate) fn ambiguous_pointers(project: &Project) -> Vec<(String, usize)> {
    let mut out = Vec::new();
    for card in &project.detail.schemas {
        walk_fields(&card.fields, &mut |field, path| {
            let loose = field
                .points_at
                .iter()
                .filter(|p| p.when.is_none() && p.discriminated_by.is_none())
                .count();
            if loose > 1 {
                out.push((full_with(project, card, path), loose));
            }
        });
    }
    out
}

/// The discriminated pointers no card can be picked for, and what they are told apart by
/// (`unresolvedPointers`): no card of the section pins a value on the field the candidates are
/// told apart by, so whatever the pointer holds leads nowhere. One with candidates resolves as
/// its value says, and is not said.
pub(crate) fn unresolved_pointers(project: &Project) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for card in &project.detail.schemas {
        let near = neighbours(project, card);
        walk_fields(&card.fields, &mut |field, path| {
            for points in &field.points_at {
                let (Some(by), Some(there)) = (
                    points.discriminated_by.as_deref(),
                    target_discriminator(points),
                ) else {
                    continue;
                };
                if !discriminated_candidates(&near, there).is_empty() {
                    continue;
                }
                let shown = if there == by {
                    by.to_string()
                } else {
                    format!("{by} → {there}")
                };
                out.push((full_with(project, card, path), shown));
            }
        });
    }
    out
}

/// Value objects and enums nothing points at — by a relation, by any path written
/// anywhere, or as a discriminated pointer's candidate (`unreferenced`). A flow's nodes are
/// cards of their own, which the relations leave out: what they carry and return, and where
/// their rows read and write by their lines, count too.
pub(crate) fn unreferenced(project: &Project) -> Vec<String> {
    let mut used: HashSet<String> = project
        .detail
        .relations
        .iter()
        .map(|r| r.to_schema_id.clone())
        .collect();
    for card in project.detail.schemas.iter().chain(&project.detail.inner) {
        let base = project.section_of(card);
        let lines = card
            .connections
            .iter()
            .filter_map(|line| facts::line_path(project, line));
        for path in declared_paths(project.defs.cards(), card, None)
            .into_iter()
            .map(|d| d.path)
            .chain(lines)
        {
            if let Some((landed, _)) = landing(&path, &base, project)
                && landed.id != card.id
            {
                used.insert(landed.id.clone());
            }
        }
    }
    for card in &project.detail.schemas {
        let near = neighbours(project, card);
        walk_fields(&card.fields, &mut |field, _| {
            for points in &field.points_at {
                let Some(there) = target_discriminator(points) else {
                    continue;
                };
                for candidate in discriminated_candidates(&near, there) {
                    used.insert(candidate.id.clone());
                }
            }
        });
    }
    project
        .detail
        .schemas
        .iter()
        .filter(|c| (c.kind == "value" || c.kind == "enum") && !used.contains(&c.id))
        .map(|c| project.full(c))
        .collect()
}

// MARK: - The rev

/// A digest of everything the project says but where its cards sit: SHA-256 of the
/// canonical JSON, the first 6 bytes in hex (`MCPViews.revision`).
pub(crate) fn revision(project: &Project) -> String {
    let mut canonical = Map::new();
    canonical.insert("project".into(), project.detail.project.slug.clone().into());
    let schemas: Vec<Value> = project
        .detail
        .schemas
        .iter()
        .map(|card| {
            let mut row = Map::new();
            row.insert("name".into(), project.full(card).into());
            row.insert("kind".into(), card.kind.clone().into());
            row.insert("fields".into(), Value::Array(field_digest(&card.fields)));
            if !card.values.is_empty() {
                row.insert(
                    "values".into(),
                    card.values
                        .iter()
                        .map(|v| v.value.clone())
                        .collect::<Vec<_>>()
                        .into(),
                );
            }
            if !card.tags.is_empty() {
                row.insert("tags".into(), card.tags.clone().into());
            }
            if !card.rules.is_empty() {
                row.insert("rules".into(), rules_json(&card.rules));
            }
            if let Some(d) = &card.discriminator {
                row.insert("discriminator".into(), d.clone().into());
            }
            Value::Object(row)
        })
        .collect();
    canonical.insert("schemas".into(), Value::Array(schemas));
    let digest = Sha256::digest(ojson::compact(&Value::Object(canonical)).as_bytes());
    digest[..6].iter().map(|b| format!("{b:02x}")).collect()
}

/// Rules as SP writes them: `kind` first, then the parameters in the order written,
/// an object among them with its keys sorted (`OJSON.rules`).
pub(crate) fn rules_json(rules: &[crate::model::Rule]) -> Value {
    Value::Array(
        rules
            .iter()
            .map(|rule| {
                let mut out = Map::new();
                out.insert("kind".into(), crate::export::rule_kind(rule).into());
                for (key, value) in crate::export::rule_params(rule) {
                    out.insert(key.clone(), crate::export::sorted(value));
                }
                Value::Object(out)
            })
            .collect(),
    )
}

fn field_digest(fields: &[Field]) -> Vec<Value> {
    fields
        .iter()
        .map(|field| {
            let mut row = Map::new();
            row.insert("name".into(), field.name.clone().into());
            row.insert("type".into(), field.kind.clone().into());
            if field.is_array {
                row.insert("array".into(), true.into());
            }
            if field.is_required {
                row.insert("required".into(), true.into());
            }
            if !field.points_at.is_empty() {
                row.insert(
                    "pointsAt".into(),
                    field
                        .points_at
                        .iter()
                        .map(|p| {
                            format!(
                                "{}:{}",
                                p.schema
                                    .as_deref()
                                    .or(p.discriminated_by.as_deref())
                                    .unwrap_or("?"),
                                p.designation
                            )
                        })
                        .collect::<Vec<_>>()
                        .into(),
                );
            }
            if !field.children.is_empty() {
                row.insert(
                    "children".into(),
                    Value::Array(field_digest(&field.children)),
                );
            }
            Value::Object(row)
        })
        .collect()
}

// MARK: - Flows

/// Nodes no way reaches from the start: written, and never run (`FlowGraph.unreachable`).
pub(crate) fn unreachable(flow: &Card) -> Vec<String> {
    let Some(begin) = flow.nodes.iter().find(|n| n.kind == "begin") else {
        return Vec::new();
    };
    let by_name: HashMap<&str, &FlowNode> =
        flow.nodes.iter().map(|n| (n.name.as_str(), n)).collect();
    let mut seen: HashSet<String> = HashSet::new();
    let mut queue = vec![begin.name.clone()];
    while let Some(name) = queue.pop() {
        if !seen.insert(name.clone()) {
            continue;
        }
        let Some(node) = by_name.get(name.as_str()) else {
            continue;
        };
        queue.extend(node.exits().into_iter().map(|(to, _, _)| to));
    }
    flow.nodes
        .iter()
        .map(|n| n.name.clone())
        .filter(|n| !seen.contains(n))
        .collect()
}

/// Whether `warning` is one `flow_warnings` says: how a batch knows them
/// (`batch::step_warnings`), said of the whole batch as the flow stands at the end.
pub(crate) fn is_flow_warning(warning: &str) -> bool {
    crate::text::is_said("check.flow.unfinished", warning)
        || crate::text::is_said("check.flow.unreachable", warning)
        || crate::text::is_said("check.flow.selfLoop", warning)
        || crate::text::is_said("check.flow.noPartner", warning)
        || crate::text::is_said("check.flow.unlinkedPartner", warning)
}

/// What a writer of `flow` is told: what is not finished yet, and nodes nothing leads
/// to (`FlowGraph.warnings`).
pub(crate) fn flow_warnings(flow: &Card, project: &Project) -> Vec<crate::text::Keyed> {
    let name = project.full(flow);
    let mut out: Vec<crate::text::Keyed> = node_problems(&flow.nodes, &project.defs.vias)
        .1
        .into_iter()
        .map(|problem| keyed!("check.flow.unfinished", name, problem))
        .collect();
    let lost = unreachable(flow);
    if !lost.is_empty() {
        out.push(keyed!(
            "check.flow.unreachable",
            name,
            nodes = crate::text::List::of(lost)
        ));
    }
    // A node whose next is itself: run over and over with no way out.
    let looped: Vec<&str> = flow
        .nodes
        .iter()
        .filter(|n| n.nexts.iter().any(|next| next == &n.name))
        .map(|n| n.name.as_str())
        .collect();
    if !looped.is_empty() {
        out.push(keyed!(
            "check.flow.selfLoop",
            name,
            nodes = crate::text::List::of(looped)
        ));
    }
    // Told at once to whoever writes it; the check lists it as a gap of its own (`missingPartners`).
    if let Some(begin) = flow.nodes.iter().find(|n| n.kind == "begin")
        && crate::export::charts::names_no_partner(begin)
    {
        out.push(keyed!("check.flow.noPartner", name, node = begin.name));
    }
    // The same: the check lists it as a group of its own (`unlinkedPartners`).
    for partner in unlinked_partners_of(flow, project) {
        out.push(keyed!(
            "check.flow.unlinkedPartner",
            name,
            place = partner.place,
            program = partner.program,
            names = partner.named()
        ));
    }
    out
}

// MARK: - A partner written as text that names a flow or an actor

/// What a program's text is held against: the flows of the project by the text of their name and
/// of their heading, and the actor sections with a flow in them (or in an actor inside them) by
/// their name. Made once for a reading (`Project::partner_names`).
pub(crate) struct PartnerNames {
    /// The ids of the flows a text names, each once, in the tree's order.
    flows: HashMap<String, Vec<String>>,
    /// Each flow's place in the tree's order: its section's, then its own.
    rank: HashMap<String, usize>,
    /// The ids of the actor sections a text names, in the tree's order.
    actors: HashMap<String, Vec<String>>,
}

impl PartnerNames {
    fn of(project: &Project) -> PartnerNames {
        let sections: HashMap<&str, usize> = project
            .detail
            .sections
            .iter()
            .enumerate()
            .map(|(i, s)| (s.id.as_str(), i))
            .collect();
        let mut flows: Vec<(usize, usize, &Card)> = project
            .detail
            .schemas
            .iter()
            .enumerate()
            .filter(|(_, c)| c.kind == "flow")
            .map(|(i, c)| {
                let section = sections
                    .get(project.section_of(c).as_str())
                    .copied()
                    .unwrap_or(usize::MAX);
                (section, i, c.as_ref())
            })
            .collect();
        flows.sort_by_key(|(section, i, _)| (*section, *i));
        let around = crate::export::actors::Actors::new(project);
        let mut names = PartnerNames {
            flows: HashMap::new(),
            rank: HashMap::new(),
            actors: HashMap::new(),
        };
        let mut holding: HashSet<String> = HashSet::new();
        for (rank, (_, _, flow)) in flows.iter().enumerate() {
            names.rank.insert(flow.id.clone(), rank);
            for text in [flow.name.trim(), flow.label.trim()] {
                if text.is_empty() {
                    continue;
                }
                let ids = names.flows.entry(text.to_string()).or_default();
                if !ids.contains(&flow.id) {
                    ids.push(flow.id.clone());
                }
            }
            holding.extend(around.chain(&project.section_of(flow)));
        }
        for section in &project.detail.sections {
            if holding.contains(&section.id) {
                names
                    .actors
                    .entry(section.name.trim().to_string())
                    .or_default()
                    .push(section.id.clone());
            }
        }
        names
    }
}

/// A place in a flow that writes its partner as a program whose text is the name of a flow or of an
/// actor of the project (`unlinkedPartners`): the diagram draws a lifeline of its own for it, not
/// the flow's or the actor's.
pub(crate) struct UnlinkedPartner {
    /// Where, in full: `…:Flow.start`, `…:Flow.node.steps[i]`, `…:Flow.end`.
    pub at: String,
    /// The same inside the flow: `start`, `node.steps[i]`, `end`.
    pub place: String,
    /// What it does there: `receive` (a start, or a row), `send`, `call`, `return` (an end).
    pub op: String,
    /// The program, without the spaces around it.
    pub program: String,
    /// The flows it is named like (by name, by heading or as a path), in the tree's order.
    pub flows: Vec<String>,
    /// The actor section it is named like, by its path.
    pub actor: Option<String>,
}

impl UnlinkedPartner {
    /// Whom it is named like, as a text kept by its key (said in the language of whoever reads it).
    pub fn named(&self) -> crate::text::Keyed {
        let flows = || crate::text::List::of(&self.flows);
        match (&self.actor, self.flows.is_empty()) {
            (Some(actor), false) => keyed!(
                "check.partner.flowsAndActor",
                flows = flows(),
                actor = actor.as_str()
            ),
            (Some(actor), true) => keyed!("check.partner.actor", actor = actor.as_str()),
            (None, _) => keyed!("check.partner.flows", flows = flows()),
        }
    }
}

/// The places of `flow` that write their partner as a program named like a flow or an actor of the
/// project, in the flow's order: a start that receives, the rows that send, receive and call, an end
/// that returns. The flow itself and the actors around it are not held against: a program of the
/// same name is the other processes of the same program. A text is held as written but for the
/// spaces around it, and a capital is not a small letter.
pub(crate) fn unlinked_partners_of(flow: &Card, project: &Project) -> Vec<UnlinkedPartner> {
    let full = project.full(flow);
    let mut asked: Vec<(String, String, String, String)> = Vec::new();
    for node in &flow.nodes {
        let here = crate::pathtext::member(&full, &node.name);
        if let Some(Step {
            program: Some(program),
            ..
        }) = facts::receive_row(node)
        {
            asked.push((here.clone(), node.name.clone(), "receive".into(), program));
        }
        if node.kind == "process" {
            for (index, step) in node.steps.iter().enumerate() {
                if let ("send" | "receive" | "call", Some(program)) =
                    (step.op.as_str(), &step.program)
                {
                    asked.push((
                        format!("{here}.steps[{index}]"),
                        format!("{}.steps[{index}]", node.name),
                        step.op.clone(),
                        program.clone(),
                    ));
                }
            }
        }
        if let Some(Step {
            program: Some(program),
            ..
        }) = facts::return_row(node)
        {
            asked.push((here, node.name.clone(), "return".into(), program));
        }
    }
    // The names are looked up only for a flow that names a program.
    if asked.is_empty() {
        return Vec::new();
    }
    let names = project
        .partner_names
        .get_or_init(|| PartnerNames::of(project));
    let base = project.section_of(flow);
    let mut around: Option<Vec<String>> = None;
    asked
        .into_iter()
        .filter_map(|(at, place, op, program)| {
            let text = program.trim();
            if text.is_empty() {
                return None;
            }
            let mut ids: Vec<String> = names.flows.get(text).cloned().unwrap_or_default();
            if let Some(found) = project
                .find_seeing(text, &base)
                .filter(|c| c.kind == "flow")
                && !ids.contains(&found.id)
            {
                ids.push(found.id.clone());
            }
            ids.retain(|id| *id != flow.id);
            ids.sort_by_key(|id| names.rank.get(id).copied().unwrap_or(usize::MAX));
            let flows: Vec<String> = ids
                .iter()
                .filter_map(|id| project.card(id))
                .map(|c| project.full(c))
                .collect();
            let actor = names.actors.get(text).and_then(|sections| {
                let around = around.get_or_insert_with(|| {
                    crate::export::actors::Actors::new(project).chain(&base)
                });
                sections
                    .iter()
                    .find(|s| !around.contains(s))
                    .map(|s| project.directory.full_section(s))
            });
            if flows.is_empty() && actor.is_none() {
                return None;
            }
            Some(UnlinkedPartner {
                at,
                place,
                op,
                program: text.to_string(),
                flows,
                actor,
            })
        })
        .collect()
}

/// The unlinked partners of every flow, in the tree's order (`unlinkedPartners`).
pub(crate) fn unlinked_partners(project: &Project) -> Vec<UnlinkedPartner> {
    project
        .detail
        .schemas
        .iter()
        .filter(|c| c.kind == "flow")
        .flat_map(|c| unlinked_partners_of(c, project))
        .collect()
}

/// A send or a receive whose other flow says nothing back: the place in its flow, the flow it
/// names, what it carries, and what does not fit, in a sentence.
#[derive(Debug)]
pub(crate) struct PeerMismatch {
    pub at: String,
    pub peer: String,
    pub carries: Vec<String>,
    pub problem: crate::text::Keyed,
}

/// One end of a flow's exchange with another: a row that sends (a process row's send or call,
/// an end's return) or a place that receives (a begin on receive, a process row's receive), one
/// for each flow it names, as each is answered on its own; and the two sides of a run node, which
/// starts the flow it runs and takes that flow's end back.
struct PeerEnd<'a> {
    at: String,
    sends: bool,
    /// The flow it names, when it names one that is there.
    peer: Option<&'a Card>,
    /// Whether it names no one, no flow and no program: a receive that takes from whoever calls.
    open: bool,
    /// A process row's receive: it waits mid-flow for an answer, which a return naming no one gives
    /// to whoever called (a begin is a new run, which no return starts).
    waits: bool,
    /// An end's return.
    returns: bool,
    /// An end's return naming no one and no program: it goes back to whoever called this flow.
    to_caller: bool,
    /// A run node's side. It answers what the flow it runs says of this one, and is not said itself:
    /// a run names that flow by its runs line, not by a peer, and the check is of what peers say.
    /// Its receiving side takes only an end's return, as a run waits for the end.
    run: bool,
    carries: Vec<String>,
}

/// Every send and receive in `flow`, in node order.
fn peer_ends<'a>(flow: &Card, project: &Project<'a>) -> Vec<PeerEnd<'a>> {
    let base = project.section_of(flow);
    let name = project.full(flow);
    let flow_of = |raw: &str| project.find_seeing(raw, &base).filter(|c| c.kind == "flow");
    let mut out = Vec::new();
    let mut end = |at: String, step: &Step, sends: bool, waits: bool, returns: bool| {
        let open = step.peers.is_empty() && step.program.is_none();
        let side = |peer: Option<&'a Card>| PeerEnd {
            at: at.clone(),
            sends,
            peer,
            open,
            waits,
            returns,
            to_caller: returns && open,
            run: false,
            carries: step.carries.clone(),
        };
        if open {
            out.push(side(None));
        }
        // A peer that leads nowhere, or to what is not a flow, is said by the path and line checks;
        // a program is outside the project and answers itself.
        for peer in step.peers.iter().filter_map(|p| flow_of(p)) {
            out.push(side(Some(peer)));
        }
    };
    for node in &flow.nodes {
        let here = crate::pathtext::member(&name, &node.name);
        if let Some(step) = facts::receive_row(node) {
            end(here.clone(), &step, false, false, false);
        }
        if node.kind == "process" {
            for (index, step) in node.steps.iter().enumerate() {
                let at = format!("{here}.steps[{index}]");
                match step.op.as_str() {
                    "send" | "call" if !step.peers.is_empty() => end(at, step, true, false, false),
                    "receive" => end(at, step, false, true, false),
                    _ => {}
                }
            }
        }
        if let Some(step) = facts::return_row(node) {
            end(here, &step, true, false, true);
        }
    }
    // A run starts the flow it runs with what it carries, and waits for that flow's end, which
    // gives back what it returns.
    for node in flow.nodes.iter().filter(|n| n.kind == "run") {
        let Some(ran) = node.target.as_deref().and_then(flow_of) else {
            continue;
        };
        let here = crate::pathtext::member(&name, &node.name);
        for (sends, carries) in [(true, &node.carries), (false, &node.returns)] {
            out.push(PeerEnd {
                at: here.clone(),
                sends,
                peer: Some(ran),
                open: false,
                waits: !sends,
                returns: false,
                to_caller: false,
                run: true,
                carries: carries.clone(),
            });
        }
    }
    out
}

/// The paths of `one` that no path of `other` is the same as, inside, or around. Either side saying
/// nothing leaves nothing out.
fn left_out<'s>(one: &'s [String], other: &[String]) -> Vec<&'s String> {
    fn plain(path: &str) -> String {
        path.replace("[]", "")
    }
    fn near(a: &str, b: &str) -> bool {
        a == b || a.starts_with(&format!("{b}.")) || b.starts_with(&format!("{a}."))
    }
    if one.is_empty() || other.is_empty() {
        return Vec::new();
    }
    let other: Vec<String> = other.iter().map(|p| plain(p)).collect();
    one.iter()
        .filter(|a| {
            let a = plain(a);
            !other.iter().any(|b| near(&a, b))
        })
        .collect()
}

/// Whether what one side carries is what the other takes, path for path both ways.
fn carries_fit(one: &[String], other: &[String]) -> bool {
    left_out(one, other).is_empty() && left_out(other, one).is_empty()
}

impl PeerEnd<'_> {
    /// A begin on receive: a new run that any of its senders may start.
    fn begins(&self) -> bool {
        !self.sends && !self.waits && !self.run
    }

    /// Whether this receiving place takes what `sent` carries. A begin, a run waiting for the end
    /// and a receive waiting for the return each take one of several things that may come, so what
    /// is sent need only be among what they take; a receive mid-exchange takes one send, which must
    /// be what it takes.
    fn takes(&self, sent: &PeerEnd) -> bool {
        if !self.waits || self.run || sent.returns {
            left_out(&sent.carries, &self.carries).is_empty()
        } else {
            carries_fit(&sent.carries, &self.carries)
        }
    }
}

/// The places among `theirs` that take what `sent` sends from the flow `from`: a receive naming that
/// flow or no one; a run only a return, as it waits for the end; and from a run only a begin, as a
/// run starts a new run.
fn takers<'e, 'a>(sent: &PeerEnd, from: &str, theirs: &'e [PeerEnd<'a>]) -> Vec<&'e PeerEnd<'a>> {
    theirs
        .iter()
        .filter(|t| {
            !t.sends
                && (t.open || t.peer.is_some_and(|p| p.id == from))
                && (!t.run || sent.returns)
                && (!sent.run || !t.waits)
        })
        .collect()
}

/// Where each side is and what it carries, joined by " / ": nested from the last, so each is said
/// as it is.
fn carried_at<'e, 'a: 'e>(
    sides: impl DoubleEndedIterator<Item = &'e PeerEnd<'a>>,
) -> Option<crate::text::Keyed> {
    sides
        .rev()
        .map(|t| {
            keyed!(
                "check.peer.carriesAt",
                at = t.at,
                carries = crate::text::List::of(t.carries.iter())
            )
        })
        .reduce(|rest, one| keyed!("check.peer.alsoAt", first = one, rest))
}

/// Paths once each, in the order first met.
fn once<'s>(paths: impl IntoIterator<Item = &'s String>) -> Vec<&'s String> {
    let mut seen = HashSet::new();
    paths.into_iter().filter(|p| seen.insert(*p)).collect()
}

/// What a place taking any of `senders` leaves out: for each sender, its paths the place does not
/// take; and the place's paths no sender sends, which are not known while a sender says nothing of
/// what it carries.
fn gaps<'e>(
    carries: &'e [String],
    senders: &[&'e PeerEnd],
) -> (Vec<(usize, Vec<&'e String>)>, Vec<&'e String>) {
    let untaken = senders
        .iter()
        .enumerate()
        .filter_map(|(i, s)| {
            let out = left_out(&s.carries, carries);
            (!out.is_empty()).then_some((i, out))
        })
        .collect();
    let unsent = if senders.is_empty() || senders.iter().any(|s| s.carries.is_empty()) {
        Vec::new()
    } else {
        let sent: Vec<String> = senders.iter().flat_map(|s| s.carries.clone()).collect();
        left_out(carries, &sent)
    };
    (untaken, unsent)
}

/// The flows' exchanges that the other side does not answer (`peerMismatches`): a send whose peer
/// has no place that takes it from this flow (a begin on receive or a receive row, naming this
/// flow or no one; for a return, a run of this flow too), or takes something else; and a receive
/// naming a peer that sends nothing here (a send or call row, a return, or for a begin a run of
/// this flow), or sends something else. Each side is said where it is written, so a mismatch shows
/// from both of its flows.
///
/// A receiving place that may be sent different things takes what all of them send: a begin, from
/// every flow that sends to it (those it names, or for a begin naming no one, every flow naming its
/// flow), and a receive waiting for the return of a call, from every end that returns. Each thing
/// sent must be among what it takes, and each thing it takes must be sent by one of them.
pub(crate) fn peer_mismatches(project: &Project) -> Vec<PeerMismatch> {
    let flows: Vec<&Card> = project
        .detail
        .schemas
        .iter()
        .map(|c| &**c)
        .filter(|c| c.kind == "flow")
        .collect();
    let ends: HashMap<&str, Vec<PeerEnd>> = flows
        .iter()
        .map(|f| (f.id.as_str(), peer_ends(f, project)))
        .collect();
    let mut out = Vec::new();
    for flow in &flows {
        let mine = &ends[flow.id.as_str()];
        for (index, end) in mine.iter().enumerate() {
            if end.run {
                continue;
            }
            if let Some(peer) = end.peer {
                out.extend(one_side(project, flow, end, peer, &ends));
            }
            let last_of_place = mine
                .get(index + 1)
                .is_none_or(|next| next.at != end.at || next.sends != end.sends || next.run);
            if end.begins() && last_of_place {
                out.extend(begin_side(project, &flows, flow, end, &ends));
            }
        }
    }
    out
}

/// What one side of an exchange with `peer` leaves unanswered, said where `end` is written. A begin
/// is checked here only for each peer sending to it; what it takes is checked against all its
/// senders at once (`begin_side`).
fn one_side(
    project: &Project,
    flow: &Card,
    end: &PeerEnd,
    peer: &Card,
    ends: &HashMap<&str, Vec<PeerEnd>>,
) -> Vec<PeerMismatch> {
    let name = project.full(flow);
    let there = project.full(peer);
    let mine = &ends[flow.id.as_str()];
    let theirs = ends.get(peer.id.as_str()).map(Vec::as_slice).unwrap_or(&[]);
    let said = |problem| PeerMismatch {
        at: end.at.clone(),
        peer: there.clone(),
        carries: end.carries.clone(),
        problem,
    };
    let carries = crate::text::List::of(end.carries.iter());
    if end.sends {
        let answers = takers(end, &flow.id, theirs);
        if answers.is_empty() {
            return vec![said(if end.returns {
                keyed!("check.peer.noReceiverReturn", there, name)
            } else {
                keyed!("check.peer.noReceiverSend", there, name)
            })];
        }
        if answers.iter().any(|t| t.takes(end)) {
            return Vec::new();
        }
        let taken: Vec<String> = answers.iter().flat_map(|t| t.carries.clone()).collect();
        let missing = left_out(&end.carries, &taken);
        let Some(others) = carried_at(answers.iter().copied()) else {
            return Vec::new();
        };
        return vec![said(if missing.is_empty() {
            keyed!("check.peer.sendsMismatch", there, carries, others)
        } else {
            let missing = crate::text::List::of(missing);
            keyed!("check.peer.sendsNotTaken", there, carries, missing, others)
        })];
    }
    if end.begins() {
        let sends = theirs
            .iter()
            .any(|t| t.sends && t.peer.is_some_and(|p| p.id == flow.id));
        return if sends {
            Vec::new()
        } else {
            vec![said(keyed!("check.peer.noSenderReceive", there, name))]
        };
    }
    // This flow calls the peer when it sends there or runs it; the peer's return naming no one
    // then answers a receive that waits here. A run starts a new run of this flow, which a begin
    // takes, not a receive mid-flow.
    let calls = mine
        .iter()
        .any(|e| e.sends && e.peer.is_some_and(|p| p.id == peer.id));
    let answers: Vec<&PeerEnd> = theirs
        .iter()
        .filter(|t| {
            t.sends
                && ((t.peer.is_some_and(|p| p.id == flow.id) && !t.run)
                    || (t.to_caller && end.waits && calls))
        })
        .collect();
    if answers.is_empty() {
        return vec![said(keyed!("check.peer.noSenderWait", there, name))];
    }
    // A return ends the peer's run, so the receive waiting for it takes whichever end returns; a
    // send mid-exchange is one of several turns, and the receive takes one of them.
    let (returns, turns): (Vec<&PeerEnd>, Vec<&PeerEnd>) =
        answers.iter().copied().partition(|t| t.returns);
    if turns.iter().any(|t| carries_fit(&end.carries, &t.carries)) {
        return Vec::new();
    }
    let (untaken, unsent) = gaps(&end.carries, &returns);
    if !returns.is_empty() && untaken.is_empty() && unsent.is_empty() {
        return Vec::new();
    }
    if !turns.is_empty() {
        let Some(others) = carried_at(answers.iter().copied()) else {
            return Vec::new();
        };
        return vec![said(keyed!(
            "check.peer.receivesMismatch",
            there,
            carries,
            others
        ))];
    }
    let mut out = Vec::new();
    if let Some(others) = carried_at(untaken.iter().map(|(i, _)| returns[*i])) {
        let missing =
            crate::text::List::of(once(untaken.iter().flat_map(|(_, m)| m.iter().copied())));
        out.push(said(keyed!(
            "check.peer.returnsNotReceived",
            there,
            carries = carries.clone(),
            missing,
            others
        )));
    }
    if !unsent.is_empty()
        && let Some(others) = carried_at(returns.iter().copied())
    {
        let missing = crate::text::List::of(unsent);
        out.push(said(keyed!(
            "check.peer.notReturned",
            there,
            carries,
            missing,
            others
        )));
    }
    out
}

/// What a begin on receive leaves unanswered by all that send to it: the flows sending something it
/// does not take (unless another place of its flow takes it), and what it takes that none sends.
fn begin_side(
    project: &Project,
    flows: &[&Card],
    flow: &Card,
    begin: &PeerEnd,
    ends: &HashMap<&str, Vec<PeerEnd>>,
) -> Vec<PeerMismatch> {
    let mine = &ends[flow.id.as_str()];
    let named: Vec<&str> = mine
        .iter()
        .filter(|e| e.at == begin.at && e.begins())
        .filter_map(|e| e.peer.map(|p| p.id.as_str()))
        .collect();
    let senders: Vec<(&Card, &PeerEnd)> = flows
        .iter()
        .filter(|f| begin.open || named.contains(&f.id.as_str()))
        .flat_map(|f| {
            ends[f.id.as_str()]
                .iter()
                .filter(|t| t.sends && t.peer.is_some_and(|p| p.id == flow.id))
                .map(move |t| (*f, t))
        })
        .collect();
    let sides: Vec<&PeerEnd> = senders.iter().map(|(_, s)| *s).collect();
    let (untaken, unsent) = gaps(&begin.carries, &sides);
    let carries = crate::text::List::of(begin.carries.iter());
    let mut out = Vec::new();
    // A sender another place of this flow takes is answered there.
    let untaken: Vec<(usize, Vec<&String>)> = untaken
        .into_iter()
        .filter(|(i, _)| {
            let (from, sent) = senders[*i];
            !takers(sent, &from.id, mine).iter().any(|t| t.takes(sent))
        })
        .collect();
    let mut by_flow: Vec<&Card> = Vec::new();
    for (i, _) in &untaken {
        let from = senders[*i].0;
        if !by_flow.iter().any(|f| f.id == from.id) {
            by_flow.push(from);
        }
    }
    for from in by_flow {
        let theirs: Vec<&(usize, Vec<&String>)> = untaken
            .iter()
            .filter(|(i, _)| senders[*i].0.id == from.id)
            .collect();
        let Some(others) = carried_at(theirs.iter().map(|(i, _)| sides[*i])) else {
            continue;
        };
        let there = project.full(from);
        let missing =
            crate::text::List::of(once(theirs.iter().flat_map(|(_, m)| m.iter().copied())));
        out.push(PeerMismatch {
            at: begin.at.clone(),
            peer: there.clone(),
            carries: begin.carries.clone(),
            problem: keyed!(
                "check.peer.sendsNotReceived",
                there,
                carries = carries.clone(),
                missing,
                others
            ),
        });
    }
    if !unsent.is_empty()
        && let Some(others) = carried_at(sides.iter().copied())
    {
        let mut from: Vec<String> = Vec::new();
        for (f, _) in &senders {
            let name = project.full(f);
            if !from.contains(&name) {
                from.push(name);
            }
        }
        let missing = crate::text::List::of(unsent);
        out.push(PeerMismatch {
            at: begin.at.clone(),
            peer: from.join(", "),
            carries: begin.carries.clone(),
            problem: keyed!("check.peer.notSent", carries, missing, others),
        });
    }
    out
}

const NODE_KINDS: [&str; 7] = ["begin", "process", "run", "eval", "wait", "join", "end"];
const TRIGGERS: [&str; 5] = ["manual", "receive", "event", "schedule", "startup"];
const PROCESS_OPS: [&str; 8] = [
    "read", "write", "sign", "verify", "call", "send", "receive", "compute",
];
const RECEIVING: [&str; 7] = [
    "peer",
    "program",
    "carries",
    "via",
    "initiator",
    "anyCaller",
    "verifyWith",
];
const JOINS: [&str; 3] = ["all", "any", "first"];
const PARTIALS: [&str; 2] = ["fail", "continue"];
const WAITS: [&str; 3] = ["approval", "timer", "event"];
const OUTCOMES: [&str; 3] = ["success", "failure", "cancelled"];
const INITIATORS: [&str; 2] = ["self", "peer"];
const GROUPS: [&str; 4] = ["sequence", "unordered", "switch", "loop"];

/// The keys a kind of node may have beside `name`, `kind`, `label`, `description` and
/// `topLeft` (`FlowNode.Vocabulary.keys`).
fn node_keys(kind: &str) -> Option<Vec<&'static str>> {
    Some(match kind {
        "begin" => {
            let mut keys = vec!["trigger", "event", "every", "next"];
            keys.extend(RECEIVING);
            keys
        }
        "process" => vec!["steps", "next"],
        "run" => vec![
            "target",
            "carries",
            "returns",
            "via",
            "signWith",
            "verifyWith",
            "onError",
            "next",
        ],
        "eval" => vec!["value", "branches", "else"],
        "wait" => vec!["for", "event", "timeout", "onTimeout", "next"],
        "join" => vec!["join", "partial", "next"],
        "end" => vec!["outcome", "return"],
        _ => return None,
    })
}

/// The heading a person sees for a kind of node (`FlowNode.Vocabulary.label`).
fn node_label(kind: &str) -> crate::text::Keyed {
    match kind {
        "begin" => keyed!("check.flow.labelBegin"),
        "process" => keyed!("check.flow.labelProcess"),
        "run" => keyed!("check.flow.labelRun"),
        "eval" => keyed!("check.flow.labelEval"),
        "wait" => keyed!("check.flow.labelWait"),
        "join" => keyed!("check.flow.labelJoin"),
        "end" => keyed!("check.flow.labelEnd"),
        other => keyed!("check.flow.labelOther", kind = other),
    }
}

/// What a node lacks to be finished: a key it should set, or a line it should draw, each with
/// what to put there.
enum Lacking {
    Key(&'static str, crate::text::Keyed),
    Line(crate::text::Keyed, crate::text::Keyed),
    Next,
}

/// A node's lack, as an unfinished flow says it.
fn lacks(node: &FlowNode, what: Lacking) -> crate::text::Keyed {
    let (name, label) = (&node.name, node_label(&node.kind));
    match what {
        Lacking::Key(key, why) => keyed!("check.flow.lacksKey", name, label, key, why),
        Lacking::Line(line, why) => keyed!("check.flow.lacksLine", name, label, line, why),
        Lacking::Next => keyed!("check.flow.lacksNext", name, label),
    }
}

/// The keys a node was written with, as the wire names them (`FlowNode.givenKeys`).
fn given_keys(node: &FlowNode) -> Vec<&'static str> {
    let mut out = Vec::new();
    let mut put = |present: bool, key: &'static str| {
        if present {
            out.push(key);
        }
    };
    put(node.trigger.is_some(), "trigger");
    put(node.event.is_some(), "event");
    put(node.every.is_some(), "every");
    put(node.target.is_some(), "target");
    put(!node.targets.is_empty(), "targets");
    put(node.join.is_some(), "join");
    put(node.partial.is_some(), "partial");
    put(node.on_error.is_some(), "onError");
    put(node.value.is_some(), "value");
    put(!node.branches.is_empty(), "branches");
    put(node.otherwise.is_some(), "else");
    put(node.wait_for.is_some(), "for");
    put(node.timeout.is_some(), "timeout");
    put(node.on_timeout.is_some(), "onTimeout");
    put(node.outcome.is_some(), "outcome");
    put(node.reply.is_some(), "return");
    put(!node.steps.is_empty(), "steps");
    put(!node.peers.is_empty(), "peer");
    put(node.program.is_some(), "program");
    put(!node.carries.is_empty(), "carries");
    put(!node.returns.is_empty(), "returns");
    put(node.via.is_some(), "via");
    put(node.initiator.is_some(), "initiator");
    put(node.any_caller, "anyCaller");
    put(node.verify_with.is_some(), "verifyWith");
    put(node.sign_with.is_some(), "signWith");
    put(!node.nexts.is_empty(), "next");
    out
}

/// Why a name cannot be a node's (the rule of the names a path is made of), or none.
fn identifier_problem(raw: &str, field: &str) -> Option<String> {
    match crate::pathtext::name_problem(raw.trim())? {
        crate::pathtext::NameProblem::Empty => Some(text!("check.id.empty", field)),
        crate::pathtext::NameProblem::TooLong => Some(text!(
            "check.id.long",
            field,
            max = crate::pathtext::NAME_MAX
        )),
        _ => Some(text!("check.id.control", field)),
    }
}

/// The keys a row has set, beside `do` and `description` (`Step.setKeys`).
fn set_keys(step: &Step) -> BTreeSet<&'static str> {
    let mut out = BTreeSet::new();
    let mut put = |present: bool, key: &'static str| {
        if present {
            out.insert(key);
        }
    };
    put(!step.peers.is_empty(), "peer");
    put(step.program.is_some(), "program");
    put(!step.carries.is_empty(), "carries");
    put(!step.returns.is_empty(), "returns");
    put(!step.paths.is_empty(), "paths");
    put(step.key.is_some(), "key");
    put(step.sign_with.is_some(), "signWith");
    put(step.verify_with.is_some(), "verifyWith");
    put(step.via.is_some(), "via");
    put(step.initiator.is_some(), "initiator");
    put(step.action.is_some(), "action");
    put(step.when.is_some(), "when");
    put(step.over.is_some(), "over");
    put(!step.steps.is_empty(), "steps");
    put(!step.cases.is_empty(), "cases");
    out
}

/// The keys each kind of row a process node may have takes (`Step.Vocabulary.keys`).
fn step_keys(op: &str) -> &'static [&'static str] {
    match op {
        "receive" => &[
            "peer",
            "program",
            "carries",
            "verifyWith",
            "via",
            "initiator",
        ],
        "call" => &[
            "peer",
            "program",
            "carries",
            "returns",
            "signWith",
            "verifyWith",
            "via",
            "initiator",
        ],
        "send" => &["peer", "program", "carries", "signWith", "via", "initiator"],
        "read" | "write" => &["paths"],
        "sign" | "verify" => &["key", "paths"],
        _ => &[],
    }
}

/// The kinds of line a row of `op` takes of those drawn from a process node's rows (`reads`,
/// `writes`, `peer`, `key`): the lines its keys are written by. None for a kind of line that
/// is not one of these, which any row may draw.
pub(crate) fn step_lines(op: &str) -> &'static [&'static str] {
    match op {
        "read" => &["reads"],
        "write" => &["writes"],
        "sign" | "verify" => &["key"],
        "send" | "receive" => &["partner", "key"],
        "call" => &["key"],
        _ => &[],
    }
}

/// The kinds of line only a process node's row of the right kind takes.
pub(crate) const STEP_LINES: [&str; 4] = ["reads", "writes", "partner", "key"];

/// Why a process node's rows cannot be (`Validate.processRows`, with the checks of
/// `Validate.steps` its rows can reach: no group, no return), or none. `vias`: what `via` may be,
/// the project's values too (`Defs::vias`).
fn process_rows_problem(rows: &[Step], place: &str, vias: &[String]) -> Option<String> {
    for (index, row) in rows.iter().enumerate() {
        let here = format!("{place}[{index}]");
        if !PROCESS_OPS.contains(&row.op.as_str()) {
            if row.op == "return" {
                return Some(text!("check.rows.noReturn", here));
            }
            if GROUPS.contains(&row.op.as_str()) {
                return Some(text!("check.rows.noGroup", here, op = row.op));
            }
            return Some(text!(
                "check.rows.doIs",
                here,
                ops = PROCESS_OPS.join(" / "),
                given = if row.op.is_empty() {
                    text!("check.rows.noneGiven")
                } else {
                    text!("check.rows.given", op = row.op)
                }
            ));
        }
        if row.op == "call" && !row.peers.is_empty() {
            return Some(text!("check.rows.callFlow", here));
        }
    }
    // The rows as an action's (`Validate.steps`), said from where they are.
    let prefix = place.replace(".steps", ".");
    for (index, row) in rows.iter().enumerate() {
        let here = format!("steps[{index}]");
        let keys = step_keys(&row.op);
        // A row's partners are written as lines of the kind `partner`: said by that name.
        fn said(k: &&'static str) -> &'static str {
            if *k == "peer" { "partner" } else { k }
        }
        let stray: Vec<&str> = set_keys(row)
            .into_iter()
            .filter(|k| !keys.contains(k))
            .map(|k| said(&k))
            .collect();
        let problem = if index >= 300 {
            Some(text!("check.rows.tooMany"))
        } else if !stray.is_empty() {
            let mut allowed: Vec<&str> = keys.iter().map(said).collect();
            allowed.sort();
            Some(if allowed.is_empty() {
                text!(
                    "check.rows.strayNone",
                    here,
                    op = row.op,
                    stray = stray.join(", ")
                )
            } else {
                text!(
                    "check.rows.strayAllowed",
                    here,
                    op = row.op,
                    stray = stray.join(", "),
                    allowed = allowed.join(", ")
                )
            })
        } else if let Some(via) = row.via.as_deref().filter(|v| !vias.iter().any(|w| w == v)) {
            Some(text!(
                "check.rows.viaIs",
                here,
                vias = vias.join(" / "),
                via
            ))
        } else if let Some(who) = row.initiator.as_deref().filter(|w| !INITIATORS.contains(w)) {
            Some(text!("check.rows.initiatorIs", here, who))
        } else {
            match row.op.as_str() {
                "call" if row.program.is_none() => Some(text!("check.rows.callNeedsProgram", here)),
                "send" if row.peers.is_empty() && row.program.is_none() => {
                    Some(text!("check.rows.sendNeedsPartner", here))
                }
                "sign" | "verify" if row.key.is_none() => {
                    Some(text!("check.rows.needsKey", here, op = row.op))
                }
                _ => None,
            }
        };
        if let Some(problem) = problem {
            return Some(format!("{prefix}{problem}"));
        }
    }
    None
}

/// What is wrong with a flow's nodes — the first thing — and what is not finished yet
/// (`FlowNode.problems`). Something wrong leaves the unfinished unsaid. `vias`: what `via` may
/// be, the project's values too (`Defs::vias`).
pub(crate) fn node_problems(
    nodes: &[FlowNode],
    project_vias: &[String],
) -> (Option<String>, Vec<crate::text::Keyed>) {
    let vias: Vec<&str> = project_vias.iter().map(String::as_str).collect();
    let wrong = |message: String| (Some(message), Vec::new());
    let mut unfinished: Vec<crate::text::Keyed> = Vec::new();
    if nodes.is_empty() {
        return (None, Vec::new());
    }
    let mut names: HashSet<String> = HashSet::new();
    for node in nodes {
        if let Some(problem) =
            identifier_problem(&node.name, &text!("check.id.nameField", name = node.name))
        {
            return wrong(problem);
        }
        if !names.insert(crate::pathtext::fold(&node.name)) {
            return wrong(text!("check.node.duplicate", name = node.name));
        }
    }
    let begins: Vec<&FlowNode> = nodes.iter().filter(|n| n.kind == "begin").collect();
    if begins.len() > 1 {
        return wrong(text!(
            "check.node.manyBegins",
            count = begins.len(),
            names = crate::text::List::of(begins.iter().map(|n| n.name.as_str()))
        ));
    }
    if begins.is_empty() {
        unfinished.push(keyed!("check.flow.noBegin"));
    }
    let one = |value: Option<&str>, list: &[&str], key: &str, node: &FlowNode| {
        let value = value?;
        (!list.contains(&value)).then(|| {
            text!(
                "check.node.oneOf",
                name = node.name,
                key,
                list = list.join(" / "),
                value
            )
        })
    };
    for node in nodes {
        let mut needs = |present: bool, what: &dyn Fn() -> Lacking| {
            if !present {
                unfinished.push(lacks(node, what()));
            }
        };
        let Some(allowed) = node_keys(&node.kind) else {
            return wrong(text!(
                "check.node.kindIs",
                name = node.name,
                kinds = NODE_KINDS.join(" / "),
                kind = node.kind
            ));
        };
        if !node.targets.is_empty() {
            return wrong(text!("check.node.noTargets", name = node.name));
        }
        let given = given_keys(node);
        let stray: Vec<&str> = given
            .iter()
            .filter(|k| !allowed.contains(k))
            .copied()
            .collect();
        if !stray.is_empty() {
            return wrong(text!(
                "check.node.strayKeys",
                name = node.name,
                kind = node.kind,
                stray = crate::text::List::of(stray.iter()),
                allowed = crate::text::List::of(allowed.iter())
            ));
        }
        if let Some(bad) = [
            one(node.trigger.as_deref(), &TRIGGERS, "trigger", node),
            one(node.join.as_deref(), &JOINS, "join", node),
            one(node.partial.as_deref(), &PARTIALS, "partial", node),
            one(node.wait_for.as_deref(), &WAITS, "for", node),
            one(node.outcome.as_deref(), &OUTCOMES, "outcome", node),
            one(
                node.event.as_ref().map(|e| e.outcome.as_str()),
                &OUTCOMES,
                "event.outcome",
                node,
            ),
        ]
        .into_iter()
        .flatten()
        .next()
        {
            return wrong(bad);
        }
        if node.kind == "begin"
            && node.trigger.as_deref() != Some("receive")
            && let Some(stray) = RECEIVING.iter().find(|k| given.contains(k))
        {
            return wrong(text!("check.node.receiveOnly", name = node.name, stray));
        }
        // Anyone, or the ones named: both at once would say two things of who calls.
        if node.any_caller && (!node.peers.is_empty() || node.program.is_some()) {
            return wrong(text!("check.node.anyCallerAlone", name = node.name));
        }
        if let Some(bad) = one(node.via.as_deref(), &vias, "via", node)
            .or_else(|| one(node.initiator.as_deref(), &INITIATORS, "initiator", node))
        {
            return wrong(bad);
        }
        if node.kind == "process"
            && let Some(problem) =
                process_rows_problem(&node.steps, &format!("{}.steps", node.name), project_vias)
        {
            return wrong(problem);
        }
        // A read or write row not joined yet to what it reads or writes is not finished
        // (the node is written before its lines), not wrong.
        if node.kind == "process" {
            for (index, row) in node.steps.iter().enumerate() {
                if (row.op == "read" || row.op == "write") && row.paths.is_empty() {
                    let reads = row.op == "read";
                    let line = if reads { "reads" } else { "writes" };
                    needs(false, &|| {
                        Lacking::Line(
                            keyed!("check.flow.lineOfRow", index, op = row.op, line),
                            if reads {
                                keyed!("check.flow.whyReads", index)
                            } else {
                                keyed!("check.flow.whyWrites", index)
                            },
                        )
                    });
                }
            }
        }
        let has_next = !node.nexts.is_empty();
        match node.kind.as_str() {
            "begin" => {
                needs(node.trigger.is_some(), &|| {
                    Lacking::Key("trigger", keyed!("check.flow.whyTrigger"))
                });
                needs(
                    node.trigger.as_deref() != Some("event") || node.event.is_some(),
                    &|| {
                        Lacking::Line(
                            keyed!("check.flow.lineEvent"),
                            keyed!("check.flow.whyEventBegin"),
                        )
                    },
                );
                needs(has_next, &|| Lacking::Next);
            }
            "process" => {
                needs(!node.steps.is_empty(), &|| {
                    Lacking::Key("steps", keyed!("check.flow.whySteps"))
                });
                needs(has_next, &|| Lacking::Next);
            }
            "run" => {
                needs(node.target.is_some(), &|| {
                    Lacking::Line(keyed!("check.flow.lineRuns"), keyed!("check.flow.whyRuns"))
                });
                needs(has_next, &|| Lacking::Next);
            }
            "join" => {
                needs(node.join.is_some(), &|| {
                    Lacking::Key("join", keyed!("check.flow.whyJoin"))
                });
                needs(has_next, &|| Lacking::Next);
            }
            "eval" => {
                if let Some(index) = node
                    .branches
                    .iter()
                    .position(|b| trim_spaces(&b.when).is_empty())
                {
                    return wrong(text!(
                        "check.node.emptyWhen",
                        name = node.name,
                        n = index + 1
                    ));
                }
                needs(!node.branches.is_empty(), &|| {
                    Lacking::Line(
                        keyed!("check.flow.lineBranch"),
                        keyed!("check.flow.whyBranch"),
                    )
                });
                needs(node.otherwise.is_some(), &|| {
                    Lacking::Line(keyed!("check.flow.lineElse"), keyed!("check.flow.whyElse"))
                });
            }
            "wait" => {
                if node.on_timeout.is_some() && node.timeout.is_none() {
                    return wrong(text!("check.node.timeoutPair", name = node.name));
                }
                needs(node.wait_for.is_some(), &|| {
                    Lacking::Key("for", keyed!("check.flow.whyFor"))
                });
                needs(
                    node.wait_for.as_deref() != Some("event") || node.event.is_some(),
                    &|| {
                        Lacking::Line(
                            keyed!("check.flow.lineEvent"),
                            keyed!("check.flow.whyEventWait"),
                        )
                    },
                );
                needs(has_next, &|| Lacking::Next);
            }
            _ => needs(node.outcome.is_some(), &|| {
                Lacking::Key("outcome", keyed!("check.flow.whyOutcome"))
            }),
        }
        if let Some((to, _, _)) = node
            .exits()
            .into_iter()
            .find(|(to, _, _)| !names.contains(&crate::pathtext::fold(to)))
        {
            return wrong(text!(
                "check.node.noTarget",
                name = node.name,
                to,
                names = crate::text::List::of(nodes.iter().map(|n| n.name.as_str()))
            ));
        }
        if node.nexts.iter().collect::<HashSet<_>>().len() != node.nexts.len() {
            return wrong(text!("check.node.dupNext", name = node.name));
        }
    }
    unfinished.extend(partings(nodes));
    (None, unfinished)
}

/// Ways that part and do not meet again at one and the same join (`FlowNode.partings`).
fn partings(nodes: &[FlowNode]) -> Vec<crate::text::Keyed> {
    let mut by_name: HashMap<&str, &FlowNode> = HashMap::new();
    for node in nodes {
        by_name.entry(node.name.as_str()).or_insert(node);
    }
    let mut out: Vec<crate::text::Keyed> = Vec::new();
    for split in nodes.iter().filter(|n| n.nexts.len() >= 2) {
        let mut met: BTreeSet<String> = BTreeSet::new();
        let mut told = false;
        for start in &split.nexts {
            let mut seen: HashSet<String> = HashSet::new();
            let mut queue = vec![start.clone()];
            let mut ends = false;
            while let Some(name) = queue.pop() {
                if !seen.insert(name.clone()) {
                    continue;
                }
                let Some(node) = by_name.get(name.as_str()) else {
                    continue;
                };
                if node.kind == "join" {
                    met.insert(name);
                    continue;
                }
                if node.kind == "end" {
                    ends = true;
                    continue;
                }
                queue.extend(node.nexts.iter().cloned());
                queue.extend(node.branches.iter().map(|b| b.next.clone()));
                queue.extend(node.otherwise.iter().cloned());
            }
            if ends {
                told = true;
                out.push(keyed!("check.flow.endsOutside", name = split.name, start));
            }
        }
        if met.len() > 1 {
            out.push(keyed!(
                "check.flow.otherJoins",
                name = split.name,
                joins = crate::text::List::of(met)
            ));
        } else if met.is_empty() && !told {
            out.push(keyed!("check.flow.noJoin", name = split.name));
        }
    }
    out
}
