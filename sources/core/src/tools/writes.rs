//! The tools that write cards (`card_create`, `card_update`, `card_delete`; the Swift
//! app's `MCPSurface.cards`): every target of a call tried in the call's one transaction,
//! each alone so the next is tried against the project as the others leave it, what was
//! wrong with any said together, and nothing kept while anything is.

use super::args::{Args, Reason, Refusals, said};
use super::cards::Scope;
use super::create::{Making, make_cards};
use super::update::{card_written, is_unresolved, unresolved};
use super::views::Projection;
use super::walk::{Target, locate};
use super::{Seen, inner, update};
use crate::write::Tx;
use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};

/// The kinds whose value is read by their definition alone: the Swift app had no form
/// for them (a flow's nodes are cards of these).
const BOARD_KINDS: [&str; 3] = ["goal", "task", "custom_goal"];

/// Whether a kind's value is read by its definition alone: the board's, a flow's nodes, and
/// the kinds a project defines.
fn defined_only(kind: &str, project: &crate::export::Project) -> bool {
    BOARD_KINDS.contains(&kind) || crate::kinds::inner_only(kind) || project.is_own_kind(kind)
}

/// A path's section in full and the rest as written: what one card's path is told apart by.
fn normal(project: &crate::export::Project, path: &str) -> Option<String> {
    let (section, rest) = split_section(path);
    let id = super::section_id(project, section?).ok()?;
    Some(format!(
        "{}:{}",
        project.directory.full_section(&id),
        crate::pathtext::canonical(rest.trim_matches([' ', '\t']))
    ))
}

/// Where a `bodyFrom` may stand in a card of `kind`: on the card when its kind has a body, in its
/// rows (a formatdoc's) and in its versions (a stackdoc's).
fn holds_of_kind(project: &crate::export::Project, kind: &str) -> super::body_from::Holds {
    super::body_from::Holds {
        body: project
            .defs
            .cards()
            .get(kind)
            .map(|def| def.field("body").is_some()),
        rows: kind == "formatdoc",
        versions: kind == "stackdoc",
    }
}

/// Where a `bodyFrom` may stand in a row of a formatdoc of `kind` (a text row has a body; an item
/// holds rows).
fn holds_of_row(kind: Option<&str>) -> super::body_from::Holds {
    super::body_from::Holds {
        body: Some(kind == Some("text")),
        rows: matches!(kind, None | Some("item")),
        versions: false,
    }
}

/// Where a `bodyFrom` may stand in the value of a target `card_create` makes.
fn create_holds(
    project: &crate::export::Project,
    path: &str,
    value: &Map<String, Value>,
) -> super::body_from::Holds {
    let kind = value.get("kind").and_then(Value::as_str);
    match inner::card_holding(project, path).map(|holder| holder.kind.as_str()) {
        Some("formatdoc") => holds_of_row(kind),
        Some("stackdoc") => super::body_from::Holds {
            body: Some(true),
            rows: false,
            versions: false,
        },
        _ => holds_of_kind(project, kind.unwrap_or("entity")),
    }
}

/// Where a `bodyFrom` may stand in the value of a target `card_update` writes, and the target's
/// full path; none when the path names nothing (the write says so).
fn update_holds(
    tx: &Tx,
    scope: &Scope,
    path: &str,
    value: &Map<String, Value>,
) -> Option<(super::body_from::Holds, String)> {
    let seen = Seen::in_tx(tx, scope).ok()?;
    let project = seen.project();
    let full = normal(&project, path).unwrap_or_else(|| path.to_string());
    let holds = match locate(&project, path, None).ok()? {
        Target::Card(card) => holds_of_kind(&project, &card.kind),
        Target::FormatRow(doc, names) => {
            let kind = match value.get("kind") {
                Some(kind) => kind.as_str().map(str::to_string),
                None => inner::row_kind(tx, scope, &doc.id, &names),
            };
            holds_of_row(kind.as_deref())
        }
        Target::CustomRow(card, _) if card.kind == "stackdoc" => super::body_from::Holds {
            body: Some(true),
            rows: false,
            versions: false,
        },
        _ => super::body_from::Holds {
            body: Some(false),
            rows: false,
            versions: false,
        },
    };
    Some((holds, full))
}

/// How many names a path has after its card's: a card's own is none; a card inside a
/// container, one for each container.
fn depth(path: &str) -> usize {
    super::picks::split(split_section(path).1).len() - 1
}

/// `Core:User` → (`Core`, `User`); no section, the path as it is (`splitSection`).
fn split_section(raw: &str) -> (Option<&str>, &str) {
    match crate::pathtext::split_section(raw) {
        (Some(section), rest) => (Some(section.trim_matches([' ', '\t'])), rest),
        (None, rest) => (None, rest),
    }
}

/// The card a path is in, its section as written: `Core:Order` for `Core:Order.lines[].sku`
/// (`cardOf`).
fn card_of(path: &str) -> String {
    let (section, rest) = split_section(path);
    let card = crate::pathtext::unquote(crate::pathtext::dots(rest)[0]);
    match section {
        Some(s) => crate::pathtext::card_path(s, &card),
        None => crate::pathtext::quote(&card).into_owned(),
    }
}

/// The paths that still do not resolve on the cards a write touched, said once all is
/// in (`unresolved(on:)`): a step may name what a later one makes.
fn unresolved_on(
    tx: &Tx,
    scope: &Scope,
    ids: &[String],
    before: &HashSet<String>,
) -> Result<Vec<String>, String> {
    let seen = Seen::in_tx(tx, scope)?;
    let project = seen.project();
    let mut done = HashSet::new();
    // A card ghost declares no paths of its own: what is wrong with it is where it points.
    let ghosts: Vec<String> = ids
        .iter()
        .filter_map(|id| project.ghost(id))
        .filter_map(|g| {
            let declared = super::check::ghost_problem(&project, g)?;
            Some(format!(
                "{}: {}",
                declared.at,
                declared.hint.unwrap_or_default()
            ))
        })
        .collect();
    Ok(ghosts
        .into_iter()
        .chain(
            ids.iter()
                .filter(|id| done.insert(id.as_str()))
                .filter(|id| project.ghost(id).is_none())
                .filter_map(|id| project.card(id))
                .flat_map(|card| {
                    let full = project.full(card);
                    super::check::problems(&project, card)
                        .into_iter()
                        .filter(|(b, _)| !before.contains(&update::unresolved_key(&card.id, b)))
                        .map(move |(b, n)| format!("{full}: {}", unresolved(&b, &n)))
                }),
        )
        .collect())
}

/// A step's answer without what it said about unresolved paths: said again, as they
/// stand at the end (`droppingUnresolved`).
fn dropping_unresolved(value: Value) -> Value {
    let Value::Object(mut body) = value else {
        return value;
    };
    if let Some(Value::Array(list)) = body.get("warnings") {
        let kept: Vec<Value> = list
            .iter()
            .filter(|w| !w.as_str().is_some_and(is_unresolved))
            .cloned()
            .collect();
        if kept.is_empty() {
            body.shift_remove("warnings");
        } else {
            body.insert("warnings".into(), Value::Array(kept));
        }
    }
    Value::Object(body)
}

/// What a path names, by id, so nothing of the project is held while it is written.
enum Named {
    Card(String),
    Ghost(String),
    Field(String),
    Row(String, Vec<String>),
    /// A stackdoc's version: the card's id and the version's name.
    Version(String, String),
    /// A data card's value: the card's id and the names to it.
    Value(String, Vec<String>),
    /// An enum's value: the enum's id and the value's name.
    EnumValue(String, String),
    /// A process node's step: the node's id and the step's place.
    Step(String, usize),
    Other(String),
}

/// A path written through a card ghost: the card's path and the ghost's, read before
/// the write, so its answer can say it as it was written (`walk::shown_through`).
fn through(tx: &Tx, scope: &Scope, path: &str) -> Option<(String, String)> {
    let seen = Seen::in_tx(tx, scope).ok()?;
    super::walk::through_ghost(&seen.project(), path, None)
}

/// An answer said through the ghost its path was written through.
fn shown(answer: Value, through: &Option<(String, String)>) -> Value {
    super::walk::shown_through(answer, through)
}

fn named(tx: &Tx, scope: &Scope, path: &str) -> Result<Named, String> {
    let seen = Seen::in_tx(tx, scope)?;
    let project = seen.project();
    Ok(match locate(&project, path, None)? {
        Target::Card(card) => Named::Card(card.id.clone()),
        Target::Ghost(ghost) => Named::Ghost(ghost.id.clone()),
        Target::Symbol(..) => Named::Other(text!("code.symbol.readOnly", path)),
        Target::Field(holder, ..) => Named::Field(holder.id.clone()),
        Target::FormatRow(doc, names) => Named::Row(doc.id.clone(), names),
        Target::EnumValue(card, name) => Named::EnumValue(card.id.clone(), name),
        Target::Step(node, index) => Named::Step(node.id.clone(), index),
        Target::CustomRow(card, names) if card.kind == "stackdoc" => {
            Named::Version(card.id.clone(), names[0].clone())
        }
        Target::CustomRow(card, names) if card.kind == crate::kinds::INSTANCE => {
            Named::Value(card.id.clone(), names)
        }
        // Inside a card of a kind the project defines: its fields and rows are its value's.
        Target::CustomRow(card, names) if project.is_own_kind(&card.kind) => Named::Other(text!(
            "writes.inside.ownKind",
            path = path,
            name = card.name,
            kind = card.kind,
            full = project.full(card),
            row = names.first().map(String::as_str).unwrap_or_default()
        )),
        Target::CustomRow(card, _) => Named::Other(text!(
            "writes.inside.noRows",
            name = card.name,
            kind = card.kind
        )),
    })
}

/// Whether a card's value is checked against the Swift app's form of it.
fn formed(
    value: &Map<String, Value>,
    kind: Option<&str>,
    project: &crate::export::Project,
) -> bool {
    let kind = value
        .get("kind")
        .and_then(Value::as_str)
        .or(kind)
        .unwrap_or("entity");
    !defined_only(kind, project)
}

/// A kind named that is neither built in nor the project's own, refused with the ones there are
/// (the project's too) before the value's form is.
fn known_kind(value: &Map<String, Value>, project: &crate::export::Project) -> Result<(), String> {
    match value.get("kind").and_then(Value::as_str) {
        Some(kind) if kind != "action" && crate::kinds::builtin().get(kind).is_none() => {
            super::cards::made_kind(kind, project).map(|_| ())
        }
        _ => Ok(()),
    }
}

// MARK: - card_create

/// What a path of `card_create` makes: a card — inside a container when the path goes on
/// from one (its full path) —, a field, a formatdoc's row, a stackdoc's version.
enum Makes {
    Card(Option<String>),
    Field,
    Row,
    Version,
    EnumValue,
    Step,
}

fn makes(
    project: &crate::export::Project,
    path: &str,
    value: &Map<String, Value>,
    made_containers: &HashSet<String>,
    unmade_containers: &HashSet<String>,
) -> Result<Makes, String> {
    let (_, rest) = split_section(path);
    if crate::pathtext::dots(rest).len() > 1 {
        // Inside a container: one the call makes, or one there.
        let parent = crate::pathtext::last_dot(path).map_or(path, |(p, _)| p);
        // A container the call was to make and was refused: what goes inside it would be read as
        // fields of a card that is not there.
        if let Some(normal_path) = normal(project, path)
            && let Some(container) = unmade_containers
                .iter()
                .find(|c| normal_path.starts_with(&format!("{c}.")))
        {
            return Err(text!(
                "writes.create.containerUnmade",
                container = container,
                path = path
            ));
        }
        // A process node's step, by the place it goes to (`steps[1]`).
        if let Ok(Target::Card(c)) = locate(project, parent, None)
            && c.kind == "process"
            && crate::pathtext::last_dot(path)
                .and_then(|(_, last)| crate::detail::step_index(last.trim()))
                .is_some()
        {
            super::check(value, "card_create", "step")?;
            return Ok(Makes::Step);
        }
        let inside = match normal(project, parent).filter(|p| made_containers.contains(p)) {
            Some(p) => Some(p),
            None => match locate(project, parent, None) {
                Ok(Target::Card(c)) if project.is_container(&c.kind) => Some(project.full(c)),
                _ => None,
            },
        };
        if let Some(container) = inside {
            super::lines::refuse_card(value)?;
            super::enum_values::refuse_old(value)?;
            let kind = value
                .get("kind")
                .and_then(Value::as_str)
                .unwrap_or("entity");
            if crate::kinds::inner_only(kind) {
                super::containers::refuse_node_keys(value)?;
            }
            known_kind(value, project)?;
            if formed(value, None, project) {
                super::check(value, "card_create", "card")?;
            }
            return Ok(Makes::Card(Some(container)));
        }
        // Inside a card that holds none: a goal's task, a flow's node.
        if let Ok(Target::Card(c)) = locate(project, parent, None)
            && (c.inside.is_some() || defined_only(&c.kind, project))
        {
            let kind = value
                .get("kind")
                .and_then(Value::as_str)
                .unwrap_or("entity");
            super::containers::require_holds(
                project.defs.cards(),
                &c.kind,
                &project.full(c),
                kind,
            )?;
        }
        if let Some(holder) = inner::card_holding(project, path) {
            match holder.kind.as_str() {
                "formatdoc" => {
                    super::check(value, "card_create", "row")?;
                    return Ok(Makes::Row);
                }
                "stackdoc" => {
                    super::check(value, "card_create", "version")?;
                    return Ok(Makes::Version);
                }
                "enum" => {
                    super::check(value, "card_create", "enumValue")?;
                    return Ok(Makes::EnumValue);
                }
                crate::kinds::INSTANCE => {
                    return Err(text!(
                        "writes.create.instanceValue",
                        name = holder.name,
                        path = path
                    ));
                }
                "note" | "freedoc" | "task" | "svg" | "html" => {
                    return Err(text!(
                        "writes.create.bodyOnly",
                        name = holder.name,
                        kind = holder.kind
                    ));
                }
                kind if project.is_container(kind) || crate::kinds::inner_only(kind) => {
                    // The container the path goes on from is not there.
                    locate(project, parent, None)?;
                    return Err(text!("writes.create.notContainer", parent = parent));
                }
                _ => {}
            }
        }
        super::lines::refuse_field(value)?;
        super::check(value, "card_create", "field")?;
        return Ok(Makes::Field);
    }
    super::lines::refuse_card(value)?;
    super::enum_values::refuse_old(value)?;
    known_kind(value, project)?;
    if formed(value, None, project) {
        super::check(value, "card_create", "card")?;
    }
    Ok(Makes::Card(None))
}

/// The cards of a call made in one go, each naming its section (`create_schemas`): the
/// cards of a call point at each other whichever section they are in.
/// A card the call makes: its section as written, its name, its value, and the container
/// it goes inside (its full path).
type Entry = (String, String, Map<String, Value>, Option<String>);

/// How a card the call makes is said: its path.
fn entry_label(e: &Entry) -> String {
    match &e.3 {
        Some(container) => crate::pathtext::member(container, &e.1),
        None => crate::pathtext::card_path(&e.0, &e.1),
    }
}

/// `refused_before`: the cards the call was to make and could not, as written (`make_cards`'s
/// `unmade`).
fn create_schemas(
    tx: &mut Tx,
    scope: &Scope,
    entries: &[Entry],
    refused_before: &HashSet<String>,
) -> Result<Value, Vec<Reason>> {
    let fail = |e: String| vec![Reason { at: None, text: e }];
    let seen = Seen::in_tx(tx, scope).map_err(fail)?;
    let project = seen.project();
    let unmade: HashSet<String> = refused_before
        .iter()
        .filter_map(|p| normal(&project, p))
        .collect();
    let mut refused = Vec::new();
    let mut makings = Vec::new();
    for entry in entries {
        let (section, name, value, inside) = entry;
        let label = entry_label(entry);
        match super::section_id(&project, section) {
            Ok(id) => makings.push(Making {
                label,
                section: id,
                name: name.clone(),
                value: value.clone(),
                inside: inside.clone(),
            }),
            Err(text) => refused.push(Reason {
                at: Some(label),
                text,
            }),
        }
    }
    if !refused.is_empty() {
        return Err(refused);
    }
    // Let go of before writing (`Seen::in_tx`): not held beside the project worked out again.
    drop(project);
    drop(seen);
    let (ids, warnings) = make_cards(tx, scope, &makings, &unmade)?;
    let fresh = Seen::in_tx(tx, scope).map_err(fail)?;
    let now = fresh.project();
    let created: Vec<Value> = ids
        .iter()
        .map(|id| match (now.ghost(id), now.card(id)) {
            (Some(ghost), _) => {
                let mut row = fresh.ghost_view(ghost, &Projection::none(), None);
                if let Value::Object(o) = &mut row {
                    o.shift_remove("project");
                }
                row
            }
            (None, Some(card)) => {
                let mut row = fresh.card_view(card, &Projection::none(), None);
                if let Value::Object(o) = &mut row {
                    o.shift_remove("project");
                }
                row
            }
            (None, None) => Value::Null,
        })
        .collect();
    let mut out = Map::new();
    if !warnings.is_empty() {
        out.insert(
            "warnings".into(),
            Value::Array(warnings.into_iter().map(Value::from).collect()),
        );
    }
    out.insert("project".into(), scope.project_name.clone().into());
    out.insert("created".into(), Value::Array(created));
    Ok(Value::Object(out))
}

pub(crate) fn card_create(tx: &mut Tx, scope: &Scope, args: &Args) -> Result<Value, String> {
    let given = args.pairs()?;
    let seen = Seen::in_tx(tx, scope)?;
    let project = seen.project();
    // Every target tried, and all that was wrong said together. What goes inside a
    // card whose making was refused is not tried: it would be refused for that alone.
    let mut refusals = Refusals::new(given.len()).in_call(args);
    let mut refused_cards: HashSet<String> = HashSet::new();
    // The targets refused, as written: a card the call makes that points at one is refused as
    // that one's refusal.
    let mut unmade: HashSet<String> = HashSet::new();
    let (mut fields, mut rows, mut placements) = (Vec::new(), Vec::new(), Vec::new());
    let mut versions: Vec<(String, Map<String, Value>)> = Vec::new();
    let mut values: Vec<(String, Map<String, Value>)> = Vec::new();
    let mut steps: Vec<(String, Map<String, Value>)> = Vec::new();
    let mut entries: Vec<Entry> = Vec::new();
    let mut targets: Vec<(String, Map<String, Value>)> = Vec::new();
    // The bodies read from files, by the path written and the card's full path: said in the answer.
    let mut read_from: HashMap<String, Vec<Value>> = HashMap::new();
    let mut budget = super::body_from::Budget::default();
    let mut files = super::body_from::Files::new(tx.conn(), &scope.project, &mut budget);
    for (path, value) in &given {
        let mut value = value.clone();
        if super::body_from::wanted(&value) {
            match files.take(path, create_holds(&project, path, &value), &mut value) {
                Ok(reads) => {
                    let full = normal(&project, path);
                    let said: Vec<Value> = reads
                        .iter()
                        .map(|r| r.said(full.as_deref().unwrap_or(path)))
                        .collect();
                    if let Some(full) = full {
                        read_from.insert(full, said.clone());
                    }
                    read_from.insert(path.clone(), said);
                }
                Err(errors) => {
                    for e in &errors {
                        refusals.note(Some(path), e);
                    }
                    refused_cards.insert(card_of(path));
                    unmade.insert(path.clone());
                    continue;
                }
            }
        }
        // A card is made inside a container by its path (`<container>.<name>`); the keys of
        // before say how.
        if value.shift_remove("in").is_some() {
            refusals.note(Some(path), &super::containers::in_gone());
            refused_cards.insert(card_of(path));
            unmade.insert(path.clone());
            continue;
        }
        for (key, gone) in [
            ("nodes", super::containers::nodes_gone()),
            ("items", super::containers::items_gone()),
        ] {
            if value.shift_remove(key).is_some() {
                refusals.note(Some(path), &gone);
            }
        }
        targets.push((path.clone(), value));
    }
    // A container the call makes, before what goes inside it: by its path in full.
    targets.sort_by_key(|(p, _)| depth(p));
    let mut made_containers: HashSet<String> = HashSet::new();
    let mut unmade_containers: HashSet<String> = HashSet::new();
    for (path, value) in &targets {
        let (section, rest) = split_section(path);
        match makes(&project, path, value, &made_containers, &unmade_containers) {
            Err(e) => {
                refusals.note(Some(path), &e);
                unmade.insert(path.clone());
                if crate::pathtext::dots(rest).len() == 1 {
                    refused_cards.insert(card_of(path));
                }
                // Counted as declared though refused, so what goes inside it can say why it is not made.
                if value
                    .get("kind")
                    .and_then(Value::as_str)
                    .is_some_and(|k| project.is_container(k))
                    && let Some(full) = normal(&project, path)
                {
                    unmade_containers.insert(full);
                }
            }
            Ok(Makes::Row) => rows.push((path.clone(), value.clone())),
            Ok(Makes::Version) => versions.push((path.clone(), value.clone())),
            Ok(Makes::EnumValue) => values.push((path.clone(), value.clone())),
            Ok(Makes::Step) => steps.push((path.clone(), value.clone())),
            Ok(Makes::Field) => fields.push((path.clone(), value.clone())),
            Ok(Makes::Card(inside)) => {
                let Some(section) = section.map(str::to_string) else {
                    let sections: Vec<String> = seen
                        .detail
                        .sections
                        .iter()
                        .filter(|s| s.parent_id.is_some())
                        .map(|s| project.directory.full_section(&s.id))
                        .collect();
                    refusals.note(
                        None,
                        &text!(
                            "writes.create.sectionNeeded",
                            sections = super::capped(&sections, 12)
                        ),
                    );
                    refused_cards.insert(path.clone());
                    continue;
                };
                let kind = value
                    .get("kind")
                    .and_then(Value::as_str)
                    .unwrap_or("entity");
                if project.is_container(kind)
                    && let Some(full) = normal(&project, path)
                {
                    made_containers.insert(full);
                }
                // Where it goes is said once it is there, the way card_update places one.
                let mut made = value.clone();
                let mut look = Map::new();
                for key in ["topLeft", "size"] {
                    if let Some(given) = made.shift_remove(key) {
                        look.insert(key.into(), given);
                    }
                }
                if !look.is_empty() {
                    placements.push((path.clone(), look));
                }
                let last = crate::pathtext::last_dot(rest).map_or(rest, |(_, name)| name);
                let name = crate::pathtext::unquote(last);
                entries.push((section, name.into_owned(), made, inside));
            }
        }
    }
    drop(seen);
    // Each answer, with the path written when it is of a field, a row or a version (the cards made
    // say theirs).
    let mut out: Vec<(Value, Option<String>)> = Vec::new();
    let mut written: Vec<String> = Vec::new();
    // A card refused there is left out and the rest made again, so what goes inside
    // them is still tried.
    while !entries.is_empty() {
        match tx.alone(|tx| create_schemas(tx, scope, &entries, &unmade))? {
            Ok(answer) => {
                out.push((answer, None));
                written.extend(entries.iter().map(entry_label));
                break;
            }
            Err(reasons) => {
                refusals.note_reasons(None, &reasons);
                let named: HashSet<String> = reasons.iter().filter_map(|r| r.at.clone()).collect();
                let left: Vec<_> = entries
                    .iter()
                    .filter(|e| {
                        let label = entry_label(e);
                        !named.contains(&label)
                            && !named.iter().any(|n| label.starts_with(&format!("{n}.")))
                    })
                    .cloned()
                    .collect();
                if left.len() < entries.len() {
                    unmade.extend(named.iter().cloned());
                    refused_cards.extend(named);
                    entries = left;
                } else {
                    refused_cards.extend(entries.iter().map(entry_label));
                    entries.clear();
                }
            }
        }
    }
    for (path, look) in &placements {
        if refused_cards.contains(&card_of(path)) {
            continue;
        }
        let tried = tx.alone(|tx| -> Result<(), String> {
            let Named::Card(id) = named(tx, scope, path)? else {
                return Err(text!("writes.create.notCard", path = path));
            };
            update::update_card(tx, scope, &id, look).map(|_| ())
        })?;
        if let Err(e) = tried {
            refusals.note(Some(path), &e);
        }
    }
    for (path, value) in &fields {
        if refused_cards.contains(&card_of(path)) {
            continue;
        }
        let via = through(tx, scope, path);
        match tx.alone(|tx| super::fields::create_field(tx, scope, path, value))? {
            Ok(answer) => {
                out.push((shown(answer, &via), Some(path.clone())));
                written.push(path.clone());
            }
            Err(reasons) => refusals.note_reasons(Some(path), &reasons),
        }
    }
    for (path, value) in &rows {
        if refused_cards.contains(&card_of(path)) {
            continue;
        }
        let via = through(tx, scope, path);
        match tx.alone(|tx| inner::create_row(tx, scope, path, value))? {
            Ok(answer) => {
                out.push((shown(answer, &via), Some(path.clone())));
                written.push(path.clone());
            }
            Err(e) => refusals.note(Some(path), &e),
        }
    }
    for (path, value) in &versions {
        if refused_cards.contains(&card_of(path)) {
            continue;
        }
        match tx.alone(|tx| inner::create_version(tx, scope, path, value))? {
            Ok(answer) => {
                out.push((answer, Some(path.clone())));
                written.push(path.clone());
            }
            Err(e) => refusals.note(Some(path), &e),
        }
    }
    for (path, value) in &values {
        if refused_cards.contains(&card_of(path)) {
            continue;
        }
        let via = through(tx, scope, path);
        match tx.alone(|tx| super::enum_values::create(tx, scope, path, value))? {
            Ok(answer) => {
                out.push((shown(answer, &via), Some(path.clone())));
                written.push(path.clone());
            }
            Err(e) => refusals.note(Some(path), &e),
        }
    }
    for (path, value) in &steps {
        if refused_cards.contains(&card_of(path)) {
            continue;
        }
        match tx.alone(|tx| super::steps::create(tx, scope, path, value))? {
            Ok(answer) => {
                out.push((answer, Some(path.clone())));
                written.push(path.clone());
            }
            Err(e) => refusals.note(Some(path), &e),
        }
    }
    refusals.check()?;
    let ids: Vec<String> = {
        let seen = Seen::in_tx(tx, scope)?;
        let project = seen.project();
        written
            .iter()
            .filter_map(|p| {
                // A card ghost made is said as itself; what is made through one, in its card.
                if let Some(w) = crate::export::paths::WrittenPath::new(p)
                    && w.names.is_empty()
                    && let Some(crate::export::Named::Ghost(g)) =
                        project.find_named(&w.card, &project.directory.home)
                {
                    return Some(g.id.clone());
                }
                // A card inside a container is itself.
                if let Ok(Target::Card(c)) = locate(&project, p, None) {
                    return Some(c.id.clone());
                }
                inner::card_holding(&project, p).map(|c| c.id.clone())
            })
            .collect()
    };
    let mut left = unresolved_on(tx, scope, &ids, &HashSet::new())?;
    // The flows the call made or put nodes in: what they lack yet, said once each (a flow
    // is written a node at a time).
    {
        let seen = Seen::in_tx(tx, scope)?;
        let project = seen.project();
        let mut flows: Vec<String> = Vec::new();
        for id in &ids {
            if let Some(flow) = super::containers::flow_of(&project, id)
                && !flows.contains(&flow)
            {
                flows.push(flow);
            }
        }
        for flow in flows.iter().filter_map(|f| project.card(f)) {
            for warning in super::check::flow_warnings(flow, &project) {
                let warning = warning.to_string();
                if !left.contains(&warning) {
                    left.push(warning);
                }
            }
        }
    }
    // Each card as it is once all is written, through MCP (`revs`): a card made, and the card a
    // field, a row or a version was made in.
    if tx.who().side == crate::write::Side::Mcp {
        let seen = Seen::in_tx(tx, scope)?;
        for (answer, path) in &mut out {
            match path {
                Some(path) => {
                    if let Some(rev) = seen.rev_at(path, None) {
                        super::revs::stamp(answer, &rev);
                    }
                }
                None => {
                    for made in answer
                        .get_mut("created")
                        .and_then(Value::as_array_mut)
                        .into_iter()
                        .flatten()
                    {
                        let rev = made
                            .get("path")
                            .and_then(Value::as_str)
                            .and_then(|p| seen.rev_at(p, None));
                        if let Some(rev) = rev {
                            super::revs::stamp(made, &rev);
                        }
                    }
                }
            }
        }
    }
    if !read_from.is_empty() {
        for (answer, path) in &mut out {
            said_read_from(answer, path.as_deref(), &read_from);
        }
    }
    let answers: Vec<Value> = out
        .into_iter()
        .map(|(answer, _)| dropping_unresolved(answer))
        .collect();
    if answers.len() == 1 {
        return Ok(super::warned(answers.into_iter().next().unwrap(), &left));
    }
    let answers: Vec<Value> = answers.into_iter().map(super::under_head).collect();
    let mut body = Map::new();
    body.insert("project".into(), scope.project_name.clone().into());
    body.insert("results".into(), Value::Array(answers));
    Ok(super::warned(Value::Object(body), &left))
}

/// What was read from a file into a body (`bodyFrom`), put in the answer of what it was written to:
/// a target's own answer, or each card made that read one.
fn said_read_from(answer: &mut Value, path: Option<&str>, read_from: &HashMap<String, Vec<Value>>) {
    if let Some(said) = path.and_then(|p| read_from.get(p))
        && let Value::Object(o) = answer
    {
        o.insert("bodyFrom".into(), Value::Array(said.clone()));
    }
    for made in answer
        .get_mut("created")
        .and_then(Value::as_array_mut)
        .into_iter()
        .flatten()
    {
        let said = made
            .get("path")
            .and_then(Value::as_str)
            .and_then(|p| read_from.get(p))
            .cloned();
        if let (Some(said), Value::Object(o)) = (said, made) {
            o.insert("bodyFrom".into(), Value::Array(said));
        }
    }
}

// MARK: - card_update

/// What a target of `card_update` wrote, with the card it is in (its id).
enum Done {
    Json(String, Value),
    Card(String, Vec<String>),
    Ghost(String, Vec<String>),
}

impl Done {
    /// The card the target is in.
    fn owner(&self) -> &str {
        match self {
            Done::Json(id, _) | Done::Card(id, _) | Done::Ghost(id, _) => id,
        }
    }
}

pub(crate) fn card_update(tx: &mut Tx, scope: &Scope, args: &Args) -> Result<Value, String> {
    let mut given = args.pairs()?;
    // What does not resolve already: not said again. Of several targets alone:
    // one target's answer says its own paths (`path_warnings`), and a batch's step says none
    // (the whole project is not gone through for what is not said).
    let unresolved_before = if given.len() > 1 {
        update::unresolved_keys(tx, scope)?
    } else {
        HashSet::new()
    };
    let mut refusals = Refusals::new(given.len()).in_call(args);
    let mut done: Vec<(String, Done)> = Vec::new();
    // What a goal's `places` put: how many, and where the moved ones went.
    let mut placed: HashMap<String, (usize, Vec<Value>)> = HashMap::new();
    // Moved last and together, a destination at a time: cards that name each other
    // without a section still find each other where they land.
    let mut moves: Vec<(String, Vec<String>, Vec<String>)> = Vec::new();
    // Put among their sections' cards once the moves are made: a card moved and given a
    // position goes there among the cards of the section it moved to.
    let mut positions: Vec<(String, String, i64)> = Vec::new();
    let mut touched: Vec<String> = Vec::new();
    // The bodies changed in part, each what it was: answered short through MCP (`in_part::quiet`).
    let mut quiet: HashMap<String, Vec<(String, String)>> = HashMap::new();
    // The bodies read from files, by the path written: said in the answer. A target whose file
    // cannot be read is not written.
    let mut read_from: HashMap<String, Vec<Value>> = HashMap::new();
    let mut unread: HashSet<String> = HashSet::new();
    let mut budget = super::body_from::Budget::default();
    {
        let mut files = super::body_from::Files::new(tx.conn(), &scope.project, &mut budget);
        for (path, value) in &mut given {
            if !super::body_from::wanted(value) {
                continue;
            }
            // A path that names nothing is refused by the write, in its own words.
            let Some((holds, full)) = update_holds(tx, scope, path, value) else {
                continue;
            };
            match files.take(&full, holds, value) {
                Ok(reads) => {
                    read_from.insert(path.clone(), reads.iter().map(|r| r.said(&full)).collect());
                }
                Err(errors) => {
                    for e in &errors {
                        refusals.note(Some(path), e);
                    }
                    unread.insert(path.clone());
                }
            }
        }
    }
    for (path, value) in given.iter().filter(|(p, _)| !unread.contains(p)) {
        let tried = tx.alone(|tx| -> Result<(), String> {
            match named(tx, scope, path)? {
                Named::Row(id, names) => {
                    touched.push(id.clone());
                    super::check(value, "card_update", "row")?;
                    let via = through(tx, scope, path);
                    let out = inner::update_row(tx, scope, &id, &names, value)?;
                    done.push((path.clone(), Done::Json(id, shown(out, &via))));
                }
                Named::Version(id, name) => {
                    touched.push(id.clone());
                    super::check(value, "card_update", "version")?;
                    let out = inner::update_version(tx, scope, &id, &name, value)?;
                    done.push((path.clone(), Done::Json(id, out)));
                }
                Named::Value(id, names) => {
                    touched.push(id.clone());
                    super::check(value, "card_update", "value")?;
                    let out = super::instances::write_value(
                        tx,
                        scope,
                        &id,
                        &names,
                        Some(value["value"].clone()),
                    )?;
                    done.push((path.clone(), Done::Json(id, out)));
                }
                Named::EnumValue(id, name) => {
                    touched.push(id.clone());
                    super::check(value, "card_update", "enumValue")?;
                    let via = through(tx, scope, path);
                    let out = super::enum_values::update(tx, scope, &id, &name, value)?;
                    done.push((path.clone(), Done::Json(id, shown(out, &via))));
                }
                Named::Step(id, index) => {
                    touched.push(id.clone());
                    super::check(value, "card_update", "step")?;
                    let out = super::steps::update(tx, scope, &id, index, value)?;
                    done.push((path.clone(), Done::Json(id, out)));
                }
                Named::Field(id) => {
                    touched.push(id.clone());
                    super::lines::refuse_field(value)?;
                    super::check(value, "card_update", "field")?;
                    let via = through(tx, scope, path);
                    let out = super::fields::update_field(tx, scope, path, value)
                        .map_err(|r| said(&r))?;
                    done.push((path.clone(), Done::Json(id, shown(out, &via))));
                }
                Named::Other(e) => return Err(e),
                Named::Ghost(id) => {
                    touched.push(id.clone());
                    super::check(value, "card_update", "card")?;
                    let mut rest = value.clone();
                    let section = rest.shift_remove("section");
                    let position = rest.shift_remove("position");
                    if section.is_none() && position.is_none() && rest.is_empty() {
                        return Err(text!("writes.update.nothing"));
                    }
                    let said = if rest.is_empty() {
                        Vec::new()
                    } else {
                        super::ghosts::update(tx, scope, &id, &rest)?
                    };
                    if let Some(to) = section {
                        let to = to.as_str().map(str::to_string).unwrap_or_default();
                        match moves.iter_mut().find(|m| m.0 == to) {
                            Some(m) => {
                                m.1.push(id.clone());
                                m.2.push(path.clone());
                            }
                            None => moves.push((to, vec![id.clone()], vec![path.clone()])),
                        }
                    }
                    if let Some(at) = position.as_ref().and_then(Value::as_f64) {
                        positions.push((path.clone(), id.clone(), at.trunc() as i64));
                    }
                    done.push((path.clone(), Done::Ghost(id, said)));
                }
                Named::Card(id) => {
                    touched.push(id.clone());
                    let (kind, inside, checked, card_mapped, in_part) = {
                        let seen = Seen::in_tx(tx, scope)?;
                        let project = seen.project();
                        let card = project.card(&id);
                        let kind = card.map(|c| c.kind.clone());
                        known_kind(value, &project)?;
                        (
                            kind.clone(),
                            card.and_then(|c| c.inside.clone()),
                            formed(value, kind.as_deref(), &project),
                            kind.as_deref().is_some_and(|k| project.card_mapped(k)),
                            card.map(|c| bodies_before(c, value)).unwrap_or_default(),
                        )
                    };
                    // What a container holds is cards inside it: the keys of before refused.
                    let mut value = value.clone();
                    let mut said_before = Vec::new();
                    let mut written_before = false;
                    for (key, gone) in [
                        ("nodes", super::containers::nodes_gone()),
                        ("items", super::containers::items_gone()),
                    ] {
                        if value.contains_key(key) {
                            return Err(gone.to_string());
                        }
                    }
                    if inside.is_some() && value.contains_key("section") {
                        return Err(text!("writes.update.inContainer", path = path));
                    }
                    if let Some(into) = value.shift_remove("in") {
                        written_before = true;
                        said_before.extend(super::containers::move_into(tx, scope, &id, &into)?);
                    }
                    if value.is_empty() && written_before {
                        done.push((path.clone(), Done::Card(id, said_before)));
                        return Ok(());
                    }
                    let value = &value;
                    // Lines in the keys of before refused.
                    super::lines::refuse_card(value)?;
                    super::enum_values::refuse_old(value)?;
                    if kind.as_deref().is_some_and(crate::kinds::inner_only) {
                        super::containers::refuse_node_keys(value)?;
                    }
                    let mut rest = value.clone();
                    let values = rest
                        .shift_remove("values")
                        .map(|given| super::enum_values::wired(&given));
                    let section = rest.shift_remove("section");
                    // The boxes on a goal's card map, put in one go.
                    let places = rest.shift_remove("places");
                    // Where it goes among its section's cards.
                    let position = rest.shift_remove("position");
                    if checked {
                        // The window writes a card's lines whole (its list), MCP adds and takes
                        // them away: the form MCP is held to says the latter.
                        if tx.who().side == crate::write::Side::Screen
                            && value.get("connections").is_some_and(Value::is_array)
                        {
                            let mut rest = value.clone();
                            rest.shift_remove("connections");
                            super::check(&rest, "card_update", "card")?;
                        } else if value.get("connections").is_some_and(Value::is_array) {
                            return Err(super::lines::shape());
                        } else {
                            super::check(value, "card_update", "card")?;
                        }
                    } else if let Some(given) = &position {
                        // A board's card is read by its definition; its place among the
                        // cards is the card's, and checked as any card's.
                        let mut alone = Map::new();
                        alone.insert("position".into(), given.clone());
                        super::check(&alone, "card_update", "card")?;
                    }
                    if values.is_none()
                        && section.is_none()
                        && places.is_none()
                        && position.is_none()
                        && rest.is_empty()
                    {
                        return Err(text!("writes.update.nothing"));
                    }
                    if places.is_some() && !card_mapped {
                        return Err(text!(
                            "writes.update.placesNotMap",
                            path = path,
                            kind = kind.as_deref().unwrap_or("?")
                        ));
                    }
                    let mut said = Vec::new();
                    if let Some(values) = &values {
                        update::set_values(tx, scope, &id, values)?;
                    }
                    said.extend(said_before);
                    if !rest.is_empty() {
                        said.extend(update::update_card(tx, scope, &id, &rest)?);
                    }
                    if let Some(raw) = &places {
                        let put = super::card_map::put_places(tx, scope, &id, raw)?;
                        placed.insert(id.clone(), put);
                    }
                    if let Some(to) = section {
                        let to = to.as_str().map(str::to_string).unwrap_or_default();
                        match moves.iter_mut().find(|m| m.0 == to) {
                            Some(m) => {
                                m.1.push(id.clone());
                                m.2.push(path.clone());
                            }
                            None => moves.push((to, vec![id.clone()], vec![path.clone()])),
                        }
                    }
                    if let Some(at) = position.as_ref().and_then(Value::as_f64) {
                        positions.push((path.clone(), id.clone(), at.trunc() as i64));
                    }
                    done.push((path.clone(), Done::Card(id, said)));
                    if tx.who().side == crate::write::Side::Mcp && !in_part.is_empty() {
                        quiet.insert(path.clone(), in_part);
                    }
                }
            }
            Ok(())
        })?;
        if let Err(e) = tried {
            refusals.note(Some(path), &e);
        }
    }
    let mut move_said: HashMap<String, Vec<String>> = HashMap::new();
    // A card not moved has no place in the section it was to go to.
    let mut unmoved: HashSet<String> = HashSet::new();
    for (to, ids, paths) in &moves {
        match tx.alone(|tx| update::move_cards(tx, scope, ids, to))? {
            Ok(said) => {
                for id in ids {
                    move_said.insert(id.clone(), said.clone());
                }
            }
            Err(e) => {
                refusals.note(Some(&crate::text::list(paths)), &e);
                unmoved.extend(ids.iter().cloned());
            }
        }
    }
    for (path, id, at) in positions.iter().filter(|(_, id, _)| !unmoved.contains(id)) {
        if let Err(e) = tx.alone(|tx| update::put_position(tx, scope, id, *at))? {
            refusals.note(Some(path), &e);
        }
    }
    refusals.check()?;
    // The bodies changed in part as they are now, read once.
    let quiet: HashMap<String, Vec<(String, String, String)>> = if quiet.is_empty() {
        HashMap::new()
    } else {
        let seen = Seen::in_tx(tx, scope)?;
        let project = seen.project();
        done.iter()
            .filter_map(|(path, what)| match what {
                Done::Card(id, _) => Some((path, project.card(id)?)),
                _ => None,
            })
            .filter_map(|(path, card)| {
                let bodies = quiet.get(path)?;
                Some((
                    path.clone(),
                    bodies
                        .iter()
                        .map(|(key, before)| {
                            (
                                key.clone(),
                                before.clone(),
                                body_of(card, key).unwrap_or_default(),
                            )
                        })
                        .collect(),
                ))
            })
            .collect()
    };
    // Each target's card as it is now, through MCP (`revs`): what a write made from it is held to.
    let written_revs: HashMap<String, String> = if tx.who().side == crate::write::Side::Mcp {
        let seen = Seen::in_tx(tx, scope)?;
        done.iter()
            .filter_map(|(_, what)| Some((what.owner().to_string(), seen.rev(what.owner())?)))
            .collect()
    } else {
        HashMap::new()
    };
    let mut results = Vec::new();
    for (path, what) in &done {
        let mut result = match what {
            Done::Json(_, v) => v.clone(),
            Done::Card(id, said) => {
                let mut all = said.clone();
                all.extend(move_said.get(id).cloned().unwrap_or_default());
                let mut written = card_written(tx, scope, id, all)?;
                for (key, before, after) in quiet.get(path).into_iter().flatten() {
                    super::in_part::quieted(&mut written, key, before, after);
                }
                if let (Some((count, jumped)), Value::Object(body)) = (placed.get(id), &mut written)
                {
                    body.insert("placed".into(), (*count).into());
                    if !jumped.is_empty() {
                        body.insert("jumped".into(), Value::Array(jumped.clone()));
                    }
                }
                written
            }
            Done::Ghost(id, said) => {
                let mut all = said.clone();
                all.extend(move_said.get(id).cloned().unwrap_or_default());
                super::ghosts::written(tx, scope, id, all)?
            }
        };
        if let Some(rev) = written_revs.get(what.owner()) {
            super::revs::stamp(&mut result, rev);
        }
        said_read_from(&mut result, Some(path), &read_from);
        results.push(result);
    }
    if results.len() == 1 {
        return Ok(results.into_iter().next().unwrap());
    }
    // Several targets: one may name what a later one makes, so the paths are said as
    // they stand once all are written.
    let rows: Vec<Value> = done
        .iter()
        .zip(results)
        .map(|((path, _), result)| {
            let mut row = Map::new();
            row.insert("target".into(), path.clone().into());
            row.insert(
                "result".into(),
                super::under_head(dropping_unresolved(result)),
            );
            Value::Object(row)
        })
        .collect();
    let mut body = Map::new();
    body.insert("project".into(), scope.project_name.clone().into());
    body.insert("results".into(), Value::Array(rows));
    let left = unresolved_on(tx, scope, &touched, &unresolved_before)?;
    Ok(super::warned(Value::Object(body), &left))
}

/// The bodies `value` changes in part (`{edit, append, prepend}`), each with what it is now in
/// `card`: its own `body`, or a field of its kind's of Markdown or long text.
fn bodies_before(card: &crate::model::Card, value: &Map<String, Value>) -> Vec<(String, String)> {
    value
        .iter()
        .filter(|(_, given)| super::in_part::in_part(given))
        .filter_map(|(key, _)| Some((key.clone(), body_of(card, key)?)))
        .collect()
}

/// A card's body (`body`) or a field of its kind's of Markdown or long text, as it is.
fn body_of(card: &crate::model::Card, key: &str) -> Option<String> {
    card.form
        .as_ref()
        .and_then(|f| f["fields"].get(key))
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| (key == "body").then(|| card.body.clone()))
}

// MARK: - card_delete

fn delete_one(tx: &mut Tx, scope: &Scope, path: &str) -> Result<Value, String> {
    let via = through(tx, scope, path);
    let answer = match named(tx, scope, path)? {
        Named::Card(id) => {
            // main's card the board reaches: deleted all the same, the lines said to break.
            let broken = if scope.tree.is_empty() {
                let seen = Seen::in_tx(tx, scope)?;
                let project = seen.project();
                match project.card(&id) {
                    Some(card) => {
                        let board = super::cross::board(&tx.reader(), &scope.project)?;
                        super::cross::reaching(&board, card, &project.full(card), true)
                    }
                    None => Vec::new(),
                }
            } else {
                Vec::new()
            };
            let mut answer = update::delete_card(tx, scope, &id)?;
            if !broken.is_empty()
                && let Value::Object(out) = &mut answer
            {
                let shown: Vec<String> = broken.iter().map(|r| r.path.clone()).collect();
                let said = text!(
                    "writes.delete.boardReached",
                    count = broken.len(),
                    paths = super::capped(&shown, 5)
                );
                match out.get_mut("warnings").and_then(Value::as_array_mut) {
                    Some(list) => list.push(said.into()),
                    None => {
                        out.insert("warnings".into(), Value::Array(vec![said.into()]));
                    }
                }
                // The board's cards that reached it count among what now points nowhere.
                let dangling = out.get("danglingRefs").and_then(Value::as_u64).unwrap_or(0);
                out.insert(
                    "danglingRefs".into(),
                    (dangling + broken.len() as u64).into(),
                );
            }
            Ok(answer)
        }
        Named::Ghost(id) => super::ghosts::delete(tx, scope, &id),
        Named::Field(_) => super::fields::delete_field(tx, scope, path).map_err(|r| said(&r)),
        Named::Row(id, names) => inner::delete_row(tx, scope, &id, &names),
        Named::Version(id, name) => inner::delete_version(tx, scope, &id, &name),
        Named::Value(id, names) => super::instances::write_value(tx, scope, &id, &names, None),
        Named::EnumValue(id, name) => super::enum_values::delete(tx, scope, &id, &name),
        Named::Step(id, index) => super::steps::delete(tx, scope, &id, index),
        Named::Other(e) => Err(e),
    }?;
    Ok(shown(answer, &via))
}

pub(crate) fn card_delete(tx: &mut Tx, scope: &Scope, args: &Args) -> Result<Value, String> {
    // The value says only the rev each was read at (`revs`, taken out before): by target, or with no
    // target by path.
    let given = args.pairs()?;
    if let Some((path, key)) = given
        .iter()
        .find_map(|(path, value)| value.keys().next().map(|k| (path, k)))
    {
        return Err(if args.raw.contains_key("target") {
            text!("writes.delete.valueKey", key = key)
        } else {
            text!("writes.delete.valueKeyAt", path = path, key = key)
        });
    }
    let list: Vec<String> = given.into_iter().map(|(path, _)| path).collect();
    each_atomically(tx, scope, &list, delete_one)
}

/// Each target written as one: every target tried, and what was wrong with any of them
/// said together, with nothing kept (`eachAtomically`).
pub(crate) fn each_atomically(
    tx: &mut Tx,
    scope: &Scope,
    targets: &[String],
    body: impl Fn(&mut Tx, &Scope, &str) -> Result<Value, String>,
) -> Result<Value, String> {
    if let [one] = targets {
        return body(tx, scope, one);
    }
    let mut refusals = Refusals::new(targets.len());
    let mut results = Vec::new();
    for target in targets {
        match tx.alone(|tx| body(tx, scope, target))? {
            Ok(result) => {
                let mut row = Map::new();
                row.insert("target".into(), target.clone().into());
                row.insert("result".into(), super::under_head(result));
                results.push(Value::Object(row));
            }
            Err(e) => refusals.note(Some(target), &e),
        }
    }
    refusals.check()?;
    let mut out = Map::new();
    out.insert("project".into(), scope.project_name.clone().into());
    out.insert("results".into(), Value::Array(results));
    Ok(Value::Object(out))
}
