//! Paths written in the project, followed through a rename or a move
//! (`Repository.rewritePaths`, `followRenames`, `storeInFull` and `PathIdentities` in the
//! Swift app).
//!
//! A path held by a card's id (`store: id`: a `ref`, a pointer's card, a goal's items)
//! follows by itself. A path written as text — in a `path` field, a rule's parameter, or
//! as `[[…]]` in text that names places — is read into what it lands on (its identity:
//! the card, and the field at each step, or the row or node by its place) against the
//! project as it was, and written again against the project as it is, in full. A path that
//! does not resolve stays as it was written. A section named alone (`[[Store/Ordering:]]`,
//! `![[Store/Ordering:]]`, a formatdoc's diagram target) follows its section's renames and moves
//! the same way. Descriptions are text that names places too:
//! a card's, a field's, a node's
//! and a step's by their definitions (`paths`), a line's, a section's (read from the section) and,
//! in main, the project's.

use super::picks::Pick;
use super::walk::{Place, walk};
use crate::export::paths::WrittenPath;
use crate::export::{Named, Project};
use crate::kinds::{Child, FieldType, Kind, Kinds, Row};
use crate::model::Card;
use serde_json::Value;
use std::rc::Rc;

/// What a written path lands on, by identity rather than by name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Identity {
    /// The card it starts from — or the card ghost it was written through, which it is
    /// written through again; none when written from inside its own card.
    pub head: Option<String>,
    /// The field at each step.
    pub steps: Vec<String>,
    /// For a formatdoc's row (its names from the top).
    pub inner: Option<Vec<String>>,
    /// How many names of the path its head takes: one for a card, more for a card inside a
    /// container (`Behavior:Checkout.review` is two).
    pub depth: usize,
    /// For a data card's value, how each step picks an element of its array: a pick by fields'
    /// values holds the fields by id, so it follows their renames. Empty for anything else.
    pub picks: Vec<Option<Pick>>,
    /// A section named alone (`Store/Ordering:`), by its id: written again by its path now.
    pub section: Option<String>,
}

/// Whether a path has a section (`Core:User`).
fn has_section(raw: &str) -> bool {
    crate::pathtext::has_section(raw.trim())
}

/// A path read into what it lands on; none when it does not resolve.
pub(crate) fn identify(project: &Project, raw: &str, owner: Option<&Card>) -> Option<Identity> {
    identify_in(project, raw, owner, None)
}

/// The same, read from the section `base` when no card writes it (a section's description).
pub(crate) fn identify_in(
    project: &Project,
    raw: &str,
    owner: Option<&Card>,
    base: Option<&str>,
) -> Option<Identity> {
    if let Some(name) = crate::pathtext::section_only(raw) {
        let reader = owner
            .map(|o| project.section_of(o))
            .or_else(|| base.map(str::to_string));
        let id = project.directory.section_named(name, reader.as_deref())?;
        return Some(Identity {
            head: None,
            steps: Vec::new(),
            inner: None,
            depth: 0,
            picks: Vec::new(),
            section: Some(id),
        });
    }
    let written = WrittenPath::new(raw)?;
    if !written.well_formed {
        return None;
    }
    let reader = owner
        .map(|o| project.section_of(o))
        .or_else(|| base.map(str::to_string));
    let named = match &reader {
        Some(r) => project.find_named(&written.card, r),
        None => project
            .directory
            .find_entry(&written.card, None)
            .map(|e| match e {
                crate::directory::Entry::Card(i) => Named::Card(project.nth(i)),
                crate::directory::Entry::Ghost(g) => Named::Ghost(&project.detail.ghosts[g]),
            }),
    };
    // Through a card ghost: the ghost is the head, and the names go on in its card.
    let found = match named {
        Some(Named::Card(card)) => Some((card.id.clone(), card)),
        Some(Named::Ghost(ghost)) => project.ghost_card(ghost).map(|c| (ghost.id.clone(), c)),
        None => None,
    };
    if let Some((head_id, head)) = found
        && let Ok(w) = walk(head, &written.names, project, true)
    {
        // Into a container, the card inside it is the head (its id follows a rename).
        let head_id = if w.entered > 0 {
            w.head.id.clone()
        } else {
            head_id
        };
        let depth = 1 + w.entered;
        return Some(match w.place {
            // A data card's value, by its shape's fields: it follows their renames.
            Place::CustomRow(names) if w.head.kind == crate::kinds::INSTANCE => Identity {
                head: Some(head_id),
                steps: w.fields.iter().map(|f| f.id.clone()).collect(),
                inner: None,
                depth,
                picks: super::instances::picks_by_id(project, &w.fields, &names),
                section: None,
            },
            Place::FormatRow(names) | Place::CustomRow(names) | Place::OwnField(names) => {
                Identity {
                    head: Some(head_id),
                    steps: Vec::new(),
                    inner: Some(names),
                    depth,
                    picks: Vec::new(),
                    section: None,
                }
            }
            // An enum's value by its name, a process node's step by its place: a rename or a
            // move takes them along (`InnerMoves`).
            Place::EnumValue(name) => Identity {
                head: Some(head_id),
                steps: Vec::new(),
                inner: Some(vec![name]),
                depth,
                picks: Vec::new(),
                section: None,
            },
            Place::Step(i) => Identity {
                head: Some(head_id),
                steps: Vec::new(),
                inner: Some(vec![format!("steps[{i}]")]),
                depth,
                picks: Vec::new(),
                section: None,
            },
            Place::Card | Place::Field(_) => Identity {
                head: Some(head_id),
                steps: w.fields.iter().map(|f| f.id.clone()).collect(),
                inner: None,
                depth,
                picks: Vec::new(),
                section: None,
            },
        });
    }
    // A rule's path, or a payload's, from inside its own card: fields only.
    if let Some(owner) = owner
        && !has_section(raw)
        && let Some(card) = project.card(&owner.id)
    {
        let mut names = vec![written.card.clone()];
        names.extend(written.names.iter().cloned());
        if let Ok(w) = walk(card, &names, project, true)
            && matches!(w.place, Place::Field(_))
        {
            return Some(Identity {
                head: None,
                steps: w.fields.iter().map(|f| f.id.clone()).collect(),
                inner: None,
                depth: 1,
                picks: Vec::new(),
                section: None,
            });
        }
    }
    None
}

/// Every field of the project, by id.
pub(super) fn field_names(project: &Project) -> std::collections::HashMap<String, String> {
    fn walk(fields: &[crate::model::Field], out: &mut std::collections::HashMap<String, String>) {
        for f in fields {
            out.insert(f.id.clone(), f.name.clone());
            walk(&f.children, out);
        }
    }
    let mut out = std::collections::HashMap::new();
    for card in &project.detail.schemas {
        walk(&card.fields, &mut out);
    }
    out
}

/// `raw` again, naming what `identity` names in `project`, in full; its `[]` markers kept.
/// A path from inside its own card takes that card (`owner`) as its head.
pub(crate) fn write(
    project: &Project,
    fields: &std::collections::HashMap<String, String>,
    raw: &str,
    identity: &Identity,
    owner: Option<&Card>,
) -> Option<String> {
    if let Some(id) = &identity.section {
        return project
            .directory
            .has_section(id)
            .then(|| format!("{}:", project.directory.full_section(id)));
    }
    let rest = crate::pathtext::split_section(raw).1;
    let mut segments: Vec<String> = super::picks::split(rest);
    // Written through a card ghost, written through it again: its name and path.
    let ghost = identity.head.as_deref().and_then(|id| project.ghost(id));
    let head = match &identity.head {
        Some(id) => project.card(id)?,
        None => {
            let card = project.card(&owner?.id)?;
            segments.insert(0, crate::pathtext::quote(&card.name).into_owned());
            card
        }
    };
    let depth = identity.depth.max(1);
    if segments.len() < depth {
        return None;
    }
    let tail = segments.split_off(depth);
    let marker = |segment: &str| {
        if segment.trim().ends_with("[]") {
            "[]"
        } else {
            ""
        }
    };
    let head_marker = marker(segments.last().map(String::as_str).unwrap_or_default());
    let full = ghost.map_or_else(|| project.full(head), |g| project.full_ghost(g));
    let mut out = vec![format!("{full}{head_marker}")];
    match &identity.inner {
        // A row's names are its own now: where it moved to.
        Some(inner) => out.extend(super::walk::said_names(head, inner)),
        None => {
            if identity.steps.len() != tail.len() {
                return None;
            }
            for (i, (id, segment)) in identity.steps.iter().zip(&tail).enumerate() {
                // An element picked: picked again, its fields named as they are now.
                let picked = match identity.picks.get(i) {
                    Some(Some(pick)) => super::instances::pick_named(pick, fields),
                    _ => marker(segment).to_string(),
                };
                out.push(format!(
                    "{}{picked}",
                    crate::pathtext::quote(fields.get(id)?)
                ));
            }
        }
    }
    Some(out.join("."))
}

/// Every path a card's document writes as text, and every `[[…]]` in its text that names
/// places, passed through `moved` (a new path, or none to leave it): the document
/// rewritten in place. Returns how many changed.
pub(crate) fn rewrite(
    kinds: &Kinds,
    doc: &mut Value,
    moved: &mut dyn FnMut(&str) -> Option<String>,
) -> usize {
    let Some(kind) = doc["kind"].as_str().and_then(|k| kinds.get(k)).cloned() else {
        return 0;
    };
    let mut count = 0;
    if let Some(fields) = doc.get_mut("fields").and_then(Value::as_object_mut) {
        for field in &kind.fields {
            if let Some(v) = fields.get_mut(&field.name) {
                count += value(field, v, moved);
            }
        }
    }
    if let Some(rows) = doc.get_mut("rows").and_then(Value::as_object_mut) {
        for row in &kind.rows {
            if let Some(list) = rows.get_mut(&row.name).and_then(Value::as_array_mut) {
                for r in list {
                    count += row_rewrite(&kind, &[row], r, moved);
                }
            }
        }
    }
    count
}

/// A row and the rows under it. `above`: the row kinds from the top to this one, for a
/// kind that comes again.
fn row_rewrite(
    kind: &Kind,
    above: &[&Row],
    row: &mut Value,
    moved: &mut dyn FnMut(&str) -> Option<String>,
) -> usize {
    let here = above[above.len() - 1];
    let mut count = 0;
    if let Some(fields) = row.get_mut("fields").and_then(Value::as_object_mut) {
        for field in &here.fields {
            if let Some(v) = fields.get_mut(&field.name) {
                count += value(field, v, moved);
            }
        }
    }
    let _ = kind;
    if let Some(rows) = row.get_mut("rows").and_then(Value::as_object_mut) {
        for child in &here.children {
            let def: &Row = match child {
                Child::Row(r) => r,
                Child::Again { name, .. } => match above.iter().rev().find(|r| &r.name == name) {
                    Some(r) => r,
                    None => continue,
                },
            };
            if let Some(list) = rows.get_mut(child.name()).and_then(Value::as_array_mut) {
                let mut deeper: Vec<&Row> = above.to_vec();
                deeper.push(def);
                for r in list {
                    count += row_rewrite(kind, &deeper, r, moved);
                }
            }
        }
    }
    count
}

/// One field's value, by its definition.
fn value(
    field: &crate::kinds::Field,
    v: &mut Value,
    moved: &mut dyn FnMut(&str) -> Option<String>,
) -> usize {
    let mut count = 0;
    let mut path = |s: &mut String, count: &mut usize| {
        if let Some(next) = moved(s)
            && &next != s
        {
            *s = next;
            *count += 1;
        }
    };
    match field.fieldtype {
        FieldType::Path if !field.by_id => {
            each_string(v, &mut |s| path(s, &mut count));
        }
        FieldType::Markdown => {
            each_string(v, &mut |s| text(s, moved, &mut count));
        }
        FieldType::Text | FieldType::LongText if field.paths => {
            each_string(v, &mut |s| text(s, moved, &mut count));
        }
        FieldType::Json if field.name == "rules" => {
            for rule in v.as_array_mut().into_iter().flatten() {
                let kind = rule["kind"].as_str().unwrap_or("").to_string();
                for key in crate::export::rule_path_params(&kind) {
                    if let Some(Value::String(s)) = rule.get_mut(*key) {
                        path(s, &mut count);
                    }
                }
            }
        }
        FieldType::Group => {
            let mut one = |o: &mut Value, count: &mut usize| {
                if let Some(map) = o.as_object_mut() {
                    for inner in &field.fields {
                        if let Some(x) = map.get_mut(&inner.name) {
                            *count += value(inner, x, moved);
                        }
                    }
                }
            };
            match v {
                Value::Array(items) => items.iter_mut().for_each(|o| one(o, &mut count)),
                other => one(other, &mut count),
            }
        }
        _ => {}
    }
    count
}

fn text(s: &mut String, moved: &mut dyn FnMut(&str) -> Option<String>, count: &mut usize) {
    let mut changed = 0;
    let out = crate::notetext::transform(s, |r| {
        let next = moved(&r.path)?;
        if next == r.path {
            return None;
        }
        changed += 1;
        Some(r.written(&next))
    });
    if changed > 0 {
        *s = out;
        *count += changed;
    }
}

/// Every string of a value: itself, or those in a list, or in a list of lists.
fn each_string(v: &mut Value, f: &mut dyn FnMut(&mut String)) {
    match v {
        Value::String(s) => f(s),
        Value::Array(items) => items.iter_mut().for_each(|x| each_string(x, f)),
        _ => {}
    }
}

/// The rows of a formatdoc or the nodes of a flow a change moved or renamed, by card:
/// from where to where (their names from the top).
pub(crate) type InnerMoves = std::collections::HashMap<String, Vec<(Vec<String>, Vec<String>)>>;

/// An identity with the rows and nodes a change moved, moved with them.
fn moved_inner(mut identity: Identity, inner_moves: &InnerMoves) -> Identity {
    if let (Some(inner), Some(head)) = (&identity.inner, &identity.head)
        && let Some((from, to)) = inner_moves
            .get(head)
            .and_then(|moves| moves.iter().find(|(from, _)| inner.starts_with(from)))
    {
        let mut moved = to.clone();
        moved.extend(inner[from.len()..].iter().cloned());
        identity.inner = Some(moved);
    }
    identity
}

/// The names inside a card or a card ghost (`id`) after a change: what `tail` named in it
/// before, named again as it is now (its `[]` markers kept). None when it does not resolve.
fn tail_again(
    old: &Project,
    new: &Project,
    fields: &std::collections::HashMap<String, String>,
    id: &str,
    tail: &str,
    inner_moves: &InnerMoves,
) -> Option<String> {
    let raw = format!("{}.{tail}", old.named(id)?);
    let identity = moved_inner(identify(old, &raw, None)?, inner_moves);
    if identity.head.as_deref() != Some(id) {
        return None;
    }
    let written = write(new, fields, &raw, &identity, None)?;
    written
        .strip_prefix(&format!("{}.", new.named(id)?))
        .map(str::to_string)
}

/// A card's lines followed through a change (`connections`): the row each leaves
/// (`from`, its names in the card), where it lands (`at`: its names in the card it reaches,
/// or a path in full) and the paths its fields hold, each named again as it is now. The
/// card it reaches is held by id and follows by itself. Returns how many changed.
#[allow(clippy::too_many_arguments)]
fn rewrite_lines(
    old: &Project,
    new: &Project,
    fields: &std::collections::HashMap<String, String>,
    doc: &mut Value,
    owner_old: Option<&Card>,
    owner_new: Option<&Card>,
    inner_moves: &InnerMoves,
) -> usize {
    let Some(id) = doc["id"].as_str().map(str::to_string) else {
        return 0;
    };
    let Some(list) = doc.get_mut("connections").and_then(Value::as_array_mut) else {
        return 0;
    };
    let mut count = 0;
    let text_path = |path: &str| -> Option<String> {
        let identity = moved_inner(identify(old, path, owner_old)?, inner_moves);
        write(new, fields, path, &identity, owner_new)
    };
    for line in list.iter_mut().filter_map(Value::as_object_mut) {
        let mut set =
            |line: &mut serde_json::Map<String, Value>, key: &str, next: Option<String>| {
                if let Some(next) = next
                    && line.get(key).and_then(Value::as_str) != Some(next.as_str())
                {
                    line.insert(key.to_string(), next.into());
                    count += 1;
                }
            };
        if let Some(from) = line.get("from").and_then(Value::as_str).map(str::to_string)
            && !from.is_empty()
        {
            let next = tail_again(old, new, fields, &id, &from, inner_moves);
            set(line, "from", next);
        }
        if let Some(at) = line.get("at").and_then(Value::as_str).map(str::to_string) {
            let next = if crate::pathtext::has_section(&at) {
                text_path(&at)
            } else {
                line.get("to")
                    .and_then(Value::as_str)
                    .and_then(|to| tail_again(old, new, fields, to, &at, inner_moves))
            };
            set(line, "at", next);
        }
        // What its description names, as the card's text is.
        if let Some(Value::String(said)) = line.get_mut("description") {
            text(said, &mut |p| text_path(p), &mut count);
        }
        // The paths its fields hold, as a card's fields' paths are.
        let Some(def) = line
            .get("line")
            .and_then(Value::as_str)
            .and_then(|n| new.defs.line(n))
            .cloned()
        else {
            continue;
        };
        if let Some(values) = line.get_mut("fields").and_then(Value::as_object_mut) {
            for field in &def.fields {
                if let Some(v) = values.get_mut(&field.name) {
                    count += value(field, v, &mut |p| text_path(p));
                }
            }
        }
    }
    count
}

/// One card's document followed through a change: `doc` again with every path that named
/// something as it was (`old`) named as it is now (`new`), and how many — none when none changed.
fn follow_doc(
    old: &Project,
    new: &Project,
    new_fields: &std::collections::HashMap<String, String>,
    id: &str,
    kept: &serde_json::Map<String, Value>,
    inner_moves: &InnerMoves,
) -> Option<(Value, usize)> {
    let kinds = new.defs.cards();
    let mut doc = Value::Object(kept.clone());
    let owner_old = old.card(id);
    let owner_new = new.card(id);
    let mut count = rewrite(kinds, &mut doc, &mut |path| {
        let identity = moved_inner(identify(old, path, owner_old)?, inner_moves);
        write(new, new_fields, path, &identity, owner_new)
    });
    count += rewrite_lines(
        old,
        new,
        new_fields,
        &mut doc,
        owner_old,
        owner_new,
        inner_moves,
    );
    // A data card's values, keyed as its shape's fields are named now.
    if let Some(card) = owner_old.filter(|c| c.kind == crate::kinds::INSTANCE)
        && let Some(shape) = super::instances::shape(old, card)
        && let Some(data) = doc.pointer("/fields/data")
        && let Some((next, renamed)) = super::instances::rekeyed(old, shape, data, new_fields)
    {
        doc["fields"]["data"] = next;
        count += renamed;
    }
    // A table's rules, their paths named as its shape's fields are now; the section it reads,
    // written from the root without the `:` a section's path ends in, as that section is named now.
    if let Some(card) = owner_old.filter(|c| c.kind == crate::kinds::TABLE) {
        count += super::tables::renamed(old, card, new_fields, &mut doc);
        if let Some(within) = doc.pointer("/fields/within").and_then(Value::as_str)
            && let Some(identity) = identify(old, &format!("{within}:"), None)
            && let Some(now) = write(new, new_fields, within, &identity, None)
            && let Some(now) = now.strip_suffix(':')
            && now != within
        {
            doc["fields"]["within"] = now.into();
            count += 1;
        }
    }
    // A shape's face, its names as its fields are named now.
    if let Some(card) = owner_old.filter(|c| c.kind == "entity" || c.kind == "value") {
        count += super::faces::followed(card, &mut doc, new_fields);
    }
    (count > 0).then_some((doc, count))
}

/// A section's document followed through a change (its description, read from the section).
fn follow_section(
    old: &Project,
    new: &Project,
    new_fields: &std::collections::HashMap<String, String>,
    item: &crate::store::Item,
    inner_moves: &InnerMoves,
) -> Option<(serde_json::Map<String, Value>, usize)> {
    let mut doc = item.doc.clone();
    let Some(Value::String(said)) = doc.get_mut("description") else {
        return None;
    };
    let mut count = 0;
    text(
        said,
        &mut |path| {
            let identity = moved_inner(identify_in(old, path, None, Some(&item.id))?, inner_moves);
            write(new, new_fields, path, &identity, None)
        },
        &mut count,
    );
    (count > 0).then_some((doc, count))
}

/// Whether any field is named otherwise, made or let go of between two readings: each card's
/// fields by id and name, compared in place.
fn fields_changed(before: &crate::model::Detail, after: &crate::model::Detail) -> bool {
    fn same(a: &[crate::model::Field], b: &[crate::model::Field]) -> bool {
        a.len() == b.len()
            && a.iter()
                .zip(b)
                .all(|(x, y)| x.id == y.id && x.name == y.name && same(&x.children, &y.children))
    }
    if before.schemas.len() != after.schemas.len() {
        return true;
    }
    let old: std::collections::HashMap<&str, &crate::model::Card> = before
        .schemas
        .iter()
        .map(|c| (c.id.as_str(), &**c))
        .collect();
    after.schemas.iter().any(|c| {
        old.get(c.id.as_str())
            .is_none_or(|o| !same(&o.fields, &c.fields))
    })
}

/// What a change named otherwise (`before`, the tree as it was; `after`, as it is): the names,
/// in lower case, of the cards, card ghosts, sections and fields whose paths are not as they were
/// — as they were and as they are —, and of the rows and nodes it moved; and the cards whose own
/// path is not (the paths from inside them are written from their names).
fn renamed(
    before: &crate::model::Detail,
    after: &crate::model::Detail,
    old_fields: &std::collections::HashMap<String, String>,
    new_fields: &std::collections::HashMap<String, String>,
    inner_moves: &InnerMoves,
) -> (Vec<String>, std::collections::HashSet<String>) {
    use std::collections::{HashMap, HashSet};
    let mut names: HashSet<String> = HashSet::new();
    let mut cards: HashSet<String> = HashSet::new();
    type Placed<'d> = (&'d str, Option<&'d str>, Option<&'d str>);
    // The sections, by their names and parents: one whose name or parent changed, or whose
    // parent's path did, is named otherwise, with every card in it.
    type Sectioned<'d> = HashMap<&'d str, (&'d str, Option<&'d str>)>;
    fn sections(d: &crate::model::Detail) -> Sectioned<'_> {
        d.sections
            .iter()
            .map(|s| (s.id.as_str(), (s.name.as_str(), s.parent_id.as_deref())))
            .collect()
    }
    let (old_sections, new_sections) = (sections(before), sections(after));
    let mut moved_sections: HashSet<&str> = HashSet::new();
    for (id, was) in &old_sections {
        if new_sections.get(id) != Some(was) {
            moved_sections.insert(id);
        }
    }
    moved_sections.extend(
        new_sections
            .keys()
            .filter(|id| !old_sections.contains_key(*id)),
    );
    loop {
        let more: Vec<&str> = new_sections
            .iter()
            .chain(old_sections.iter())
            .filter(|(id, (_, parent))| {
                !moved_sections.contains(*id) && parent.is_some_and(|p| moved_sections.contains(p))
            })
            .map(|(id, _)| *id)
            .collect();
        if more.is_empty() {
            break;
        }
        moved_sections.extend(more);
    }
    for id in &moved_sections {
        for map in [&old_sections, &new_sections] {
            if let Some((name, _)) = map.get(id) {
                names.insert(crate::pathtext::fold(name));
            }
        }
    }
    // The cards, by their names, sections and containers.
    fn placed(d: &crate::model::Detail) -> HashMap<&str, Placed<'_>> {
        d.schemas
            .iter()
            .chain(d.inner.iter())
            .map(|c| {
                (
                    c.id.as_str(),
                    (
                        c.name.as_str(),
                        c.section_id.as_deref(),
                        c.inside.as_deref(),
                    ),
                )
            })
            .collect()
    }
    let (old_cards, new_cards) = (placed(before), placed(after));
    let mut moved: HashSet<&str> = HashSet::new();
    for (id, was) in &old_cards {
        if new_cards.get(id) != Some(was) {
            moved.insert(id);
        }
    }
    moved.extend(new_cards.keys().filter(|id| !old_cards.contains_key(*id)));
    if !moved_sections.is_empty() {
        for map in [&old_cards, &new_cards] {
            for (id, (_, section, _)) in map {
                if section.is_some_and(|s| moved_sections.contains(s)) {
                    moved.insert(id);
                }
            }
        }
    }
    // What is inside one named otherwise is too.
    if !moved.is_empty() {
        loop {
            let more: Vec<&str> = new_cards
                .iter()
                .chain(old_cards.iter())
                .filter(|(id, (_, _, inside))| {
                    !moved.contains(*id) && inside.is_some_and(|c| moved.contains(c))
                })
                .map(|(id, _)| *id)
                .collect();
            if more.is_empty() {
                break;
            }
            moved.extend(more);
        }
    }
    for id in &moved {
        for map in [&old_cards, &new_cards] {
            if let Some((name, _, _)) = map.get(id) {
                names.insert(crate::pathtext::fold(name));
            }
        }
        cards.insert(id.to_string());
    }
    // The card ghosts, by their names, sections and what they point at.
    fn ghosts(d: &crate::model::Detail) -> HashMap<&str, Placed<'_>> {
        d.ghosts
            .iter()
            .map(|g| {
                (
                    g.id.as_str(),
                    (
                        g.name.as_str(),
                        g.section_id.as_deref(),
                        g.target.as_deref(),
                    ),
                )
            })
            .collect()
    }
    let (old_ghosts, new_ghosts) = (ghosts(before), ghosts(after));
    for (id, was) in old_ghosts.iter().chain(new_ghosts.iter()) {
        let other = if old_ghosts.get(id) == Some(was) {
            new_ghosts.get(id)
        } else {
            old_ghosts.get(id)
        };
        if other != Some(was)
            || was.1.is_some_and(|s| moved_sections.contains(s))
            || was.2.is_some_and(|t| moved.contains(t))
        {
            names.insert(crate::pathtext::fold(was.0));
        }
    }
    // The fields, by their names.
    if !std::ptr::eq(old_fields, new_fields) {
        for (id, name) in old_fields {
            if new_fields.get(id) != Some(name) {
                names.insert(crate::pathtext::fold(name));
                if let Some(now) = new_fields.get(id) {
                    names.insert(crate::pathtext::fold(now));
                }
            }
        }
        for (id, name) in new_fields {
            if !old_fields.contains_key(id) {
                names.insert(crate::pathtext::fold(name));
            }
        }
    }
    // The rows and nodes moved.
    for (card, moves) in inner_moves {
        cards.insert(card.clone());
        for (from, to) in moves {
            names.extend(
                from.iter()
                    .chain(to.iter())
                    .map(|n| crate::pathtext::fold(n)),
            );
        }
    }
    names.retain(|n| !n.is_empty());
    let mut names: Vec<String> = names.into_iter().collect();
    names.sort();
    (names, cards)
}

/// Every path of the tree that named something as it was before, named again as it is
/// now (`followRenames`). Written into each card that changed; returns how many paths. The tree
/// as it is now is the one the transaction keeps worked out (`Seen::in_tx`), not read again, and of
/// its cards, only those whose documents can change are followed through: those that say a name the change named otherwise, whose own
/// path changed, that reach a card whose path changed, a data card or a table, and those whose
/// paths are not written as following them through no change writes them (`Note`). Built for the
/// tests, every card is followed through as well, and the two held to be the same.
pub(crate) fn follow(
    tx: &mut crate::write::Tx,
    scope: &super::cards::Scope,
    was: &super::Was,
    inner_moves: &InnerMoves,
) -> Result<usize, String> {
    let before = &was.detail;
    let (cards, sections, described, board, total) = {
        let seen = super::Seen::in_tx(tx, scope)?;
        let old = match &was.index {
            Some(index) => Project::indexed(before, index.clone()),
            None => Project::new(before),
        };
        let new = seen.project();
        let new_fields = seen.field_names();
        // The fields as they were: as they are, unless one was named otherwise, made or let go of
        // (the same names the reading kept then and keeps now are as they were).
        let kept_alike = was
            .fields
            .as_ref()
            .zip(seen.fields.get())
            .is_some_and(|(a, b)| Rc::ptr_eq(a, b));
        let changed_fields = (!kept_alike && fields_changed(before, &seen.detail)).then(|| {
            was.fields
                .as_deref()
                .cloned()
                .unwrap_or_else(|| field_names(&old))
        });
        let old_fields = changed_fields.as_ref().unwrap_or(new_fields);
        let (names, moved) = renamed(before, &seen.detail, old_fields, new_fields, inner_moves);
        let mut total = 0;
        let mut cards = Vec::new();
        let mut looked: Vec<String> = Vec::new();
        let order = seen.composed_order(tx)?;
        // The cards that say a name named otherwise; all of them when one is not a word.
        let saying = seen.saying(&names);
        for id in &order {
            let Some(kept) = seen.docs.get(id) else {
                continue;
            };
            let reaches_moved = || {
                kept.get("connections")
                    .and_then(Value::as_array)
                    .is_some_and(|lines| {
                        lines.iter().any(|l| {
                            l.get("to")
                                .and_then(Value::as_str)
                                .is_some_and(|to| moved.contains(to))
                        })
                    })
            };
            let kind = kept.get("kind").and_then(Value::as_str).unwrap_or_default();
            let follows = moved.contains(id)
                || kind == crate::kinds::INSTANCE
                || kind == crate::kinds::TABLE
                || kept.get("fields").and_then(|f| f.get("face")).is_some()
                || saying.as_ref().is_none_or(|s| s.contains(id))
                || reaches_moved()
                || !seen.written_as(id, || {
                    follow_doc(&old, &old, old_fields, id, kept, &InnerMoves::new()).is_none()
                });
            if !follows {
                continue;
            }
            looked.push(id.clone());
            if let Some((doc, count)) = follow_doc(&old, &new, new_fields, id, kept, inner_moves) {
                cards.push(doc);
                total += count;
            }
        }
        // Built for the tests: every card followed through, the same.
        #[cfg(debug_assertions)]
        {
            let mut every = Vec::new();
            for id in &order {
                if let Some(kept) = seen.docs.get(id)
                    && let Some((doc, _)) =
                        follow_doc(&old, &new, new_fields, id, kept, inner_moves)
                {
                    every.push(doc);
                }
            }
            assert_eq!(
                cards, every,
                "the cards that rewriting the paths changed differ from those found by looking at every card"
            );
        }
        // How the cards looked at are written is asked again when next asked: a path in one may
        // land elsewhere now.
        seen.forget_written_as(&looked);
        let mut sections = Vec::new();
        for item in seen.kept_sections() {
            if let Some((doc, count)) = follow_section(&old, &new, new_fields, item, inner_moves) {
                sections.push(doc);
                total += count;
            }
        }
        let (described, board) = if scope.tree.is_empty() {
            let described =
                project_followed(tx, &scope.project, &old, &new, new_fields, inner_moves)?;
            let board = board_followed(tx, &scope.project, &old, &new, new_fields, inner_moves)?;
            (described, board)
        } else {
            (None, ((Vec::new(), Vec::new()), 0))
        };
        (cards, sections, described, board, total)
    };
    put_followed(tx, &scope.project, &scope.tree, cards, sections)?;
    let mut total = total;
    if let Some((said, count)) = described {
        tx.conn()
            .execute(
                "UPDATE projects SET description = ? WHERE id = ?",
                rusqlite::params![said, scope.project],
            )
            .map_err(crate::store::sql)?;
        total += count;
    }
    let ((board_cards, board_sections), board_total) = board;
    total += board_total;
    put_followed(
        tx,
        &scope.project,
        crate::store::BOARD,
        board_cards,
        board_sections,
    )?;
    Ok(total)
}

/// What following wrote, put in place: the cards, then the sections.
fn put_followed(
    tx: &mut crate::write::Tx,
    project: &str,
    tree: &str,
    cards: Vec<Value>,
    sections: Vec<serde_json::Map<String, Value>>,
) -> Result<(), String> {
    for doc in cards {
        tx.put_card(project, tree, doc.as_object().unwrap())?;
    }
    for doc in sections {
        tx.put_section(project, tree, &doc)?;
    }
    Ok(())
}

/// `follow` of a tree no call names (a project's kinds reshaped, every tree): read again.
pub(crate) fn follow_unscoped(
    tx: &mut crate::write::Tx,
    project_id: &str,
    tree: &str,
    before: &crate::model::Detail,
    inner_moves: &InnerMoves,
) -> Result<usize, String> {
    let branch = (!tree.is_empty()).then_some(tree);
    let after = crate::model::typed(&crate::detail::read(&tx.reader(), project_id, branch)?)?;
    let old = Project::new(before);
    let new = Project::new(&after);
    let new_fields = field_names(&new);
    let made = tx.reader().compose(project_id, branch)?;
    let mut total = 0;
    let mut cards = Vec::new();
    for item in &made.cards {
        if let Some((doc, count)) =
            follow_doc(&old, &new, &new_fields, &item.id, &item.doc, inner_moves)
        {
            cards.push(doc);
            total += count;
        }
    }
    let mut sections = Vec::new();
    for item in &made.sections {
        if let Some((doc, count)) = follow_section(&old, &new, &new_fields, item, inner_moves) {
            sections.push(doc);
            total += count;
        }
    }
    let (described, board) = if tree.is_empty() {
        (
            project_followed(tx, project_id, &old, &new, &new_fields, inner_moves)?,
            board_followed(tx, project_id, &old, &new, &new_fields, inner_moves)?,
        )
    } else {
        (None, ((Vec::new(), Vec::new()), 0))
    };
    put_followed(tx, project_id, tree, cards, sections)?;
    if let Some((said, count)) = described {
        tx.conn()
            .execute(
                "UPDATE projects SET description = ? WHERE id = ?",
                rusqlite::params![said, project_id],
            )
            .map_err(crate::store::sql)?;
        total += count;
    }
    let ((board_cards, board_sections), board_total) = board;
    total += board_total;
    put_followed(
        tx,
        project_id,
        crate::store::BOARD,
        board_cards,
        board_sections,
    )?;
    Ok(total)
}

/// What the project's description names, named again as main is now: written in full, as it is
/// read from the root. Its description is the project's, not a tree's: followed only through main.
/// The description again and how many, when any changed.
fn project_followed(
    tx: &crate::write::Tx,
    project_id: &str,
    old: &Project,
    new: &Project,
    fields: &std::collections::HashMap<String, String>,
    inner_moves: &InnerMoves,
) -> Result<Option<(String, usize)>, String> {
    let mut said: String = tx
        .conn()
        .query_row(
            "SELECT description FROM projects WHERE id = ?",
            [project_id],
            |r| r.get::<_, Option<String>>(0),
        )
        .map_err(crate::store::sql)?
        .unwrap_or_default();
    let mut count = 0;
    text(
        &mut said,
        &mut |path| {
            let identity = moved_inner(identify(old, path, None)?, inner_moves);
            write(new, fields, path, &identity, None)
        },
        &mut count,
    );
    Ok((count > 0).then_some((said, count)))
}

/// main changed (`before`, main as it was): what the board's cards name of main's, named
/// again as main is now — a
/// path in full written in their text, and where a line to one of main's cards lands
/// (`at`). The card a line reaches is held by id and follows by itself. Returns how many.
/// main changed (`before`, main as it was): what the board's cards name of main's followed and
/// written (`board_followed`), main as it is read again. Returns how many.
pub(crate) fn follow_board(
    tx: &mut crate::write::Tx,
    project_id: &str,
    before: &crate::model::Detail,
    inner_moves: &InnerMoves,
) -> Result<usize, String> {
    if tx
        .reader()
        .compose(project_id, Some(crate::store::BOARD))?
        .cards
        .is_empty()
    {
        return Ok(0);
    }
    let after = crate::model::typed(&crate::detail::read(&tx.reader(), project_id, None)?)?;
    let old = Project::new(before);
    let new = Project::new(&after);
    let new_fields = field_names(&new);
    let ((cards, sections), total) =
        board_followed(tx, project_id, &old, &new, &new_fields, inner_moves)?;
    put_followed(tx, project_id, crate::store::BOARD, cards, sections)?;
    Ok(total)
}

type BoardFollowed = ((Vec<Value>, Vec<serde_json::Map<String, Value>>), usize);

fn board_followed(
    tx: &crate::write::Tx,
    project_id: &str,
    old: &Project,
    new: &Project,
    new_fields: &std::collections::HashMap<String, String>,
    inner_moves: &InnerMoves,
) -> Result<BoardFollowed, String> {
    let board = Some(crate::store::BOARD);
    let made = tx.reader().compose(project_id, board)?;
    let mut cards = Vec::new();
    let mut sections = Vec::new();
    if made.cards.is_empty() {
        return Ok(((cards, sections), 0));
    }
    let kinds = new.defs.cards();
    let mut total = 0;
    for item in made.cards {
        let mut doc = Value::Object(item.doc.clone());
        let again = |path: &str| -> Option<String> {
            if !has_section(path) {
                return None;
            }
            let identity = moved_inner(identify(old, path, None)?, inner_moves);
            write(new, new_fields, path, &identity, None)
        };
        let mut count = rewrite(kinds, &mut doc, &mut |path| again(path));
        if let Some(list) = doc.get_mut("connections").and_then(Value::as_array_mut) {
            for line in list.iter_mut().filter_map(Value::as_object_mut) {
                let Some(to) = line.get("to").and_then(Value::as_str).map(str::to_string) else {
                    continue;
                };
                let Some(at) = line.get("at").and_then(Value::as_str).map(str::to_string) else {
                    continue;
                };
                if old.card(&to).is_none() || at.is_empty() {
                    continue;
                }
                let next = if crate::pathtext::has_section(&at) {
                    again(&at)
                } else {
                    tail_again(old, new, new_fields, &to, &at, inner_moves)
                };
                if let Some(next) = next
                    && next != at
                {
                    line.insert("at".into(), next.into());
                    count += 1;
                }
            }
            for line in list.iter_mut().filter_map(Value::as_object_mut) {
                if let Some(Value::String(said)) = line.get_mut("description") {
                    text(said, &mut |p| again(p), &mut count);
                }
            }
        }
        if count > 0 {
            cards.push(doc);
            total += count;
        }
    }
    // The board's sections' descriptions, where they name main's in full.
    for item in made.sections {
        let mut doc = item.doc.clone();
        let Some(Value::String(said)) = doc.get_mut("description") else {
            continue;
        };
        let mut count = 0;
        text(
            said,
            &mut |path| {
                if !has_section(path) {
                    return None;
                }
                let identity = moved_inner(identify(old, path, None)?, inner_moves);
                write(new, new_fields, path, &identity, None)
            },
            &mut count,
        );
        if count > 0 {
            sections.push(doc);
            total += count;
        }
    }
    Ok(((cards, sections), total))
}

/// The paths of the cards just written, stored in full as every path is
/// (`storeInFull` for its owners).
/// The project as the transaction keeps it worked out (`Seen::in_tx`): not read again for each card
/// written.
pub(crate) fn store_in_full(
    tx: &mut crate::write::Tx,
    scope: &super::cards::Scope,
    owners: &[String],
) -> Result<(), String> {
    let mut stored: Vec<Value> = Vec::new();
    {
        let seen = super::Seen::in_tx(tx, scope)?;
        let project = seen.project();
        let fields = seen.field_names();
        let kinds = project.defs.cards();
        // Each was written in this transaction before (its history kept then): the order they
        // are stored in again says nothing.
        for id in owners {
            let Some(kept) = seen.docs.get(id) else {
                continue;
            };
            let mut doc = Value::Object(kept.clone());
            let owner = project.card(id);
            let mut count = rewrite(kinds, &mut doc, &mut |path| {
                let identity = identify(&project, path, owner)?;
                write(&project, fields, path, &identity, owner)
            });
            count += rewrite_lines(
                &project,
                &project,
                fields,
                &mut doc,
                owner,
                owner,
                &InnerMoves::new(),
            );
            if count > 0 {
                stored.push(doc);
            }
        }
    }
    for doc in stored {
        tx.put_card(&scope.project, &scope.tree, doc.as_object().unwrap())?;
    }
    Ok(())
}
