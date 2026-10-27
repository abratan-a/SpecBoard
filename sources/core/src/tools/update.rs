//! A card changed as a whole (`update_schema`, `set_enum_values`, `move_schemas`,
//! `delete_schema`; `Repository.updateSchema`, `moveSchemas` and `deleteSchema` in the
//! Swift app), in the kinds' form: what the value says is read from MCP's shape over the
//! document as it is, checked by the kind's definition and written whole; the paths that
//! named what moved are written again, and what that leaves broken is said.

use super::Seen;
use super::cards::{Resolver, Scope, identifier, made_kind, require_fits, text};
use super::check::{DeclaredPath, flow_warnings, node_problems, problems};
use super::follow::{InnerMoves, follow, store_in_full};
use super::grid::Rect;
use super::views::Projection;
use crate::export::Project;
use crate::write::{Side, Spot, Tx};
use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};

/// What a card's value says that is not its kind's: what it is called and is, how it
/// looks (no commit), and its lines (`connections`).
const OWN_KEYS: [&str; 11] = [
    "name",
    "kind",
    "topLeft",
    "size",
    "view",
    "version",
    "standalone",
    "exportTo",
    "connections",
    "strayFields",
    "of",
];

/// The order a change is said in its history line, after its name, label and kind
/// (`updateSchema`'s `changed`).
const SAID_ORDER: [&str; 17] = [
    "of",
    "data",
    "fields",
    "values",
    "body",
    "rows",
    "versions",
    "diagram",
    "description",
    "tags",
    "metadata",
    "rules",
    "absentFields",
    "discriminator",
    "face",
    "open",
    "connections",
];

/// What a card holds, as a person calls it when it is in the way of another kind.
fn held_label(key: &str) -> String {
    match key {
        "fields" => text!("update.held.fields"),
        "values" => text!("update.held.values"),
        other => other.to_string(),
    }
}

/// The tags a card has after `{add, remove}`: what `remove` names taken away first, then what `add`
/// names put after the rest, each once. A tag not there to remove is no error: the aim is that it
/// is gone.
fn tags_edited(mut tags: Vec<Value>, edits: &Map<String, Value>) -> Result<Vec<Value>, String> {
    if let Some(other) = edits.keys().find(|k| *k != "add" && *k != "remove") {
        return Err(text!("update.tags.noKey", other));
    }
    if edits.is_empty() {
        return Err(text!("update.tags.noWay"));
    }
    let list = |key: &str| -> Result<Vec<Value>, String> {
        match edits.get(key) {
            None => Ok(Vec::new()),
            Some(Value::Array(items)) if items.iter().all(Value::is_string) => Ok(items.clone()),
            Some(_) => Err(text!("update.tags.notList", key)),
        }
    };
    let (adding, removing) = (list("add")?, list("remove")?);
    tags.retain(|tag| !removing.contains(tag));
    for tag in adding {
        if !tags.contains(&tag) {
            tags.push(tag);
        }
    }
    Ok(tags)
}

/// Whether a value says nothing: what a write lets go of.
fn empty(value: &Value) -> bool {
    match value {
        Value::Null => true,
        Value::String(s) => s.is_empty(),
        Value::Array(a) => a.is_empty(),
        Value::Object(o) => o.is_empty(),
        _ => false,
    }
}

/// A key a kind of the project's own does not hold, said with what it does.
pub(crate) fn own_kind_lacks(def: &crate::kinds::Kind, key: &str, card: &str) -> String {
    let names: Vec<&str> = def
        .fields
        .iter()
        .map(|f| f.name.as_str())
        .chain(def.rows.iter().map(|r| r.name.as_str()))
        .collect();
    text!(
        "update.error.ownKindLacks",
        key,
        kind = def.name,
        card,
        names = crate::text::list(&names)
    )
}

/// The kinds that hold `key`, said for a value written where it cannot go.
fn holders(key: &str) -> String {
    crate::kinds::builtin()
        .placeable()
        .filter(|k| k.field(key).is_some() || k.row(key).is_some())
        .map(|k| k.name.clone())
        .collect::<Vec<_>>()
        .join(" / ")
}

// MARK: - Paths that stop resolving

/// Every declared path that does not resolve, by the card that holds it and its place
/// (`Repository.broken`): the name of what moved is not the same before and after.
pub(crate) fn broken(project: &Project) -> Vec<(String, DeclaredPath)> {
    project
        .detail
        .schemas
        .iter()
        .chain(project.detail.inner.iter())
        .flat_map(|card| card_broken(project, card))
        .chain(ghosts_broken(project))
        .collect()
}

/// One card's part of `broken`.
pub(crate) fn card_broken(
    project: &Project,
    card: &crate::model::Card,
) -> Vec<(String, DeclaredPath)> {
    problems(project, card)
        .into_iter()
        .map(|(bare, named)| {
            (
                format!("{}|{}|{}|{}", card.id, bare.at, bare.via, bare.path),
                named,
            )
        })
        .collect()
}

/// The card ghosts' part of `broken`: a card ghost that leads nowhere, by the ghost.
pub(crate) fn ghosts_broken(project: &Project) -> Vec<(String, DeclaredPath)> {
    project
        .detail
        .ghosts
        .iter()
        .filter_map(|g| {
            let named = super::check::ghost_problem(project, g)?;
            Some((format!("{}|target", g.id), named))
        })
        .collect()
}

/// What is broken now that was not (`newlyBroken`), leaving out one card's own.
pub(crate) fn newly_broken(
    tx: &Tx,
    scope: &Scope,
    before: &HashSet<String>,
    excluding: Option<&str>,
) -> Result<Vec<DeclaredPath>, String> {
    let now = Seen::in_tx(tx, scope)?;
    Ok(now
        .broken()
        .iter()
        .filter(|(key, _)| {
            !before.contains(key) && !excluding.is_some_and(|id| key.starts_with(&format!("{id}|")))
        })
        .map(|(_, named)| named.clone())
        .collect())
}

/// How a path written elsewhere that stopped resolving is said (`brokenWarning`).
pub(crate) fn broken_warning(declared: &DeclaredPath) -> String {
    // A card ghost whose card is gone has no path left to say.
    if declared.path.is_empty() {
        return text!(
            "update.broken.targetGone",
            at = declared.at,
            via = declared.via
        );
    }
    text!(
        "update.broken.noLongerResolves",
        at = declared.at,
        via = declared.via,
        path = declared.path
    )
}

/// How a path that stopped resolving when something moved is said.
fn broken_by_move(declared: &DeclaredPath) -> String {
    text!(
        "update.broken.byMove",
        at = declared.at,
        via = declared.via,
        path = declared.path,
        hint = declared.hint.as_deref().unwrap_or("")
    )
}

/// Whether a warning is one `broken_warning` or a move wrote: a path written elsewhere that stopped
/// resolving.
pub(crate) fn is_broken_warning(warning: &str) -> bool {
    crate::text::is_said("update.broken.targetGone", warning)
        || crate::text::is_said("update.broken.noLongerResolves", warning)
        || crate::text::is_said("update.broken.byMove", warning)
}

/// How a path a card declares is said not to resolve (`Repository.unresolved`).
pub(crate) fn unresolved(bare: &DeclaredPath, named: &DeclaredPath) -> String {
    text!(
        "update.unresolved",
        via = bare.via,
        path = bare.path,
        hint = named.hint.as_deref().unwrap_or("")
    )
}

/// Whether a warning is one `unresolved` wrote (`isUnresolved`).
pub(crate) fn is_unresolved(warning: &str) -> bool {
    crate::text::is_said("update.unresolved", warning)
}

/// The paths a card declares that do not resolve (`pathWarnings`), but those it had
/// before the write (`before`, by `unresolved_keys`): a write tells what it did, what was
/// so already is project_read's brokenPaths.
pub(crate) fn path_warnings(
    tx: &Tx,
    scope: &Scope,
    id: &str,
    before: &HashSet<String>,
) -> Result<Vec<String>, String> {
    let now = Seen::in_tx(tx, scope)?;
    let project = now.project();
    Ok(project
        .card(id)
        .map(|card| {
            problems(&project, card)
                .iter()
                .filter(|(b, _)| !before.contains(&unresolved_key(id, b)))
                .map(|(b, n)| unresolved(b, n))
                .collect()
        })
        .unwrap_or_default())
}

/// A path a card declares that does not resolve, told apart from the others: the card,
/// where in it, and the path.
pub(crate) fn unresolved_key(id: &str, bare: &DeclaredPath) -> String {
    format!("{id}|{}|{}", bare.via, bare.path)
}

/// The paths that do not resolve on the tree's cards now, by `unresolved_key`: what a
/// write is not to say again.
pub(crate) fn unresolved_keys(tx: &Tx, scope: &Scope) -> Result<HashSet<String>, String> {
    let now = Seen::in_tx(tx, scope)?;
    let project = now.project();
    Ok(project
        .detail
        .schemas
        .iter()
        .flat_map(|card| {
            problems(&project, card)
                .into_iter()
                .map(|(b, _)| unresolved_key(&card.id, &b))
                .collect::<Vec<_>>()
        })
        .collect())
}

// MARK: - The card written

/// What a write of a card answers with: the card, found again by id, in a line
/// (`schemaWritten`); a flow is told what it lacks.
pub(crate) fn card_written(
    tx: &Tx,
    scope: &Scope,
    id: &str,
    mut warnings: Vec<String>,
) -> Result<Value, String> {
    let seen = Seen::in_tx(tx, scope)?;
    let project = seen.project();
    let card = project
        .card(id)
        .ok_or_else(|| text!("update.error.writtenNotReadBack"))?;
    // A flow, or a node of one: what the flow lacks yet.
    if let Some(flow) = super::containers::flow_of(&project, id).and_then(|f| project.card(&f)) {
        for warning in flow_warnings(flow, &project) {
            let warning = warning.to_string();
            if !warnings.contains(&warning) {
                warnings.push(warning);
            }
        }
    }
    let view = seen.card_view(card, &Projection::none(), None);
    Ok(super::warned(view, &warnings))
}

// MARK: - Writing a card's document

/// The rules a write brings, held to their kinds: those the card held
/// before as they were are not asked again.
pub(crate) fn require_new_rules(
    tx: &Tx,
    scope: &Scope,
    doc: &Map<String, Value>,
) -> Result<(), String> {
    let id = doc.get("id").and_then(Value::as_str).unwrap_or_default();
    let before: Vec<Value> = tx
        .seen(crate::write::Table::Cards, &scope.project, &scope.tree, id)?
        .and_then(|s| s.doc())
        .map(|old| crate::kinds::rules_held(&Value::Object(old)))
        .unwrap_or_default();
    let problems =
        crate::kinds::rule_problems(&Value::Object(doc.clone()), &|rule| !before.contains(rule));
    if problems.is_empty() {
        return Ok(());
    }
    Err(problems
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n"))
}

/// What a flow's node is only unfinished without, not wrong (`FlowNode.problems`' `needs`):
/// the fields of its own a node card is kept without by the window.
const UNFINISHED_KEYS: [&str; 4] = ["trigger", "join", "for", "outcome"];

/// Whether a problem the kind's definition finds in a node card is only the node not
/// finished yet: one of its own fields it lacks, not what is wrong in it.
fn only_unfinished(problem: &crate::kinds::Problem) -> bool {
    problem.message.key() == "kindCheck.field.required"
        && UNFINISHED_KEYS.contains(&problem.at.as_str())
}

/// A card's document written after it was checked: by its kind's definition, then what
/// the kind's own code checks (a card inside a container its container holds; a flow's nodes
/// joining up). A flow is written a node at a time, so what is only unfinished across its
/// nodes is said, not refused (`check::flow_warnings`); a node card lacking a field of its
/// own is refused to MCP, and kept for the window, which adds a node on the canvas before
/// it is said what it is (`nodesUnfinished`).
pub(crate) fn put_checked(
    tx: &mut Tx,
    scope: &Scope,
    doc: &Map<String, Value>,
) -> Result<(), String> {
    let (flow, node) = doc_checked(tx, scope, doc)?;
    // Its container is there, and holds it.
    if let Some(container) = doc.get("in").and_then(Value::as_str) {
        let seen = Seen::in_tx(tx, scope)?;
        let project = seen.project();
        let Some(held_by) = project.card(container) else {
            return Err(text!("update.error.noContainerCard"));
        };
        super::containers::require_holds(
            project.defs.cards(),
            &held_by.kind,
            &project.full(held_by),
            doc.get("kind").and_then(Value::as_str).unwrap_or_default(),
        )?;
    } else if node {
        return Err(super::containers::only_inside(
            doc.get("kind").and_then(Value::as_str).unwrap_or_default(),
        ));
    }
    tx.put_card(&scope.project, &scope.tree, doc)?;
    // A flow's nodes join up: what is wrong is refused; what is only unfinished is said.
    let id = doc["id"].as_str().unwrap_or_default();
    if doc.get("kind").and_then(Value::as_str) == Some(crate::kinds::FRAME) {
        super::frames::refuse(tx, scope, &[id])?;
    }
    if flow || node {
        let seen = Seen::in_tx(tx, scope)?;
        if let Some(wrong) = flow_wrong(&seen.project(), id) {
            return Err(wrong);
        }
    }
    Ok(())
}

/// What `put_checked` holds a card's document to by itself, before anything of the project: its
/// kind's definition and the project's kinds of line, and its rules. Whether it is a flow, and
/// whether a flow's node.
pub(crate) fn doc_checked(
    tx: &Tx,
    scope: &Scope,
    doc: &Map<String, Value>,
) -> Result<(bool, bool), String> {
    let kind = doc.get("kind").and_then(Value::as_str).unwrap_or_default();
    let flow = kind == "flow";
    let node = crate::kinds::inner_only(kind);
    let unfinished_kept = node && tx.who().side == Side::Screen;
    // By the project's kinds of card and of line: the built-in ones and the project's own. What a
    // kind of the project's own, or the board's goal and task, no longer has is kept, not refused
    // (「定義に無い欄」).
    let own = tx.reader().project(&scope.project)?.and_then(|p| p.kinds);
    let defs = crate::kinds::Defs::of_project(own.as_deref());
    let custom = defs.cards().keeps_strays(kind);
    let problems: Vec<_> = crate::kinds::check_card(defs.cards(), &Value::Object(doc.clone()))
        .into_iter()
        .filter(|p| !(custom && p.stray))
        .chain(crate::kinds::check_connections(
            &defs,
            &Value::Object(doc.clone()),
        ))
        .filter(|p| !(unfinished_kept && only_unfinished(p)))
        .collect();
    if !problems.is_empty() {
        return Err(problems
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n"));
    }
    require_new_rules(tx, scope, doc)?;
    Ok((flow, node))
}

/// What is wrong with the nodes of the flow the card `id` is (or is a node of), as the project
/// is: the first thing (`check::node_problems`); none when it is not of a flow.
pub(crate) fn flow_wrong(project: &crate::export::Project, id: &str) -> Option<String> {
    let of = super::containers::flow_of(project, id)?;
    let card = project.card(&of)?;
    node_problems(&card.nodes, &project.defs.vias).0
}

/// A change to a card's document by what MCP writes: the kind's keys of `value`, a new
/// name or kind, and where its rows or nodes moved to (`Repository.updateSchema`).
/// Returns what was said.
pub(crate) fn edit_card(
    tx: &mut Tx,
    scope: &Scope,
    id: &str,
    value: &Map<String, Value>,
    inner: &[(Vec<String>, Vec<String>)],
) -> Result<Vec<String>, String> {
    let seen = Seen::in_tx(tx, scope)?;
    let project = seen.project();
    let card = project
        .card(id)
        .ok_or_else(|| text!("update.error.noSchema"))?;
    let mut doc = scope
        .doc(tx, id)?
        .ok_or_else(|| text!("update.error.noSchema"))?;
    let section = project.section_of(card);
    let current_kind = card.kind.clone();
    let mut changed: Vec<crate::text::Keyed> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();

    // Its name.
    let next_name = match value.get("name").and_then(Value::as_str) {
        Some(raw) => identifier(raw, &text!("update.label.cardName"))?,
        None => card.name.clone(),
    };
    let renamed = next_name != card.name;
    if renamed {
        // Its section's cards, and its card ghosts: a name is one thing's there. Inside a
        // container, the cards beside it there.
        let clash = match &card.inside {
            Some(container) => project
                .inner_of(container)
                .iter()
                .any(|c| c.id != id && crate::pathtext::same(&c.name, &next_name)),
            None => {
                project
                    .directory
                    .members(&section)
                    .iter()
                    .map(|&i| project.nth(i))
                    .any(|c| c.id != id && crate::pathtext::same(&c.name, &next_name))
                    || project
                        .ghosts_in(&section)
                        .iter()
                        .any(|g| crate::pathtext::same(&g.name, &next_name))
            }
        };
        if clash {
            return Err(text!("update.error.cardExists", name = next_name));
        }
        doc.insert("name".into(), next_name.clone().into());
        changed.push(keyed!("update.changed.name", name = next_name));
    }

    // Its kind.
    let next_kind = match value.get("kind").and_then(Value::as_str) {
        Some(raw) => made_kind(raw, &project)?,
        None => current_kind.clone(),
    };
    let kinds = project.defs.cards();
    let def = kinds
        .get(&next_kind)
        .ok_or_else(|| text!("update.error.kindNotCreatable", kind = next_kind))?;
    let reworked = next_kind != current_kind;
    if reworked && next_kind == crate::detail::GHOST {
        return Err(text!("update.error.toGhost", card = card.name));
    }
    if reworked {
        let board = |k: &str| k == "goal" || k == "task";
        if board(&next_kind) != board(&current_kind) {
            return Err(text!(
                "update.error.switchBoard",
                card = card.name,
                current = current_kind,
                next = next_kind
            ));
        }
        let data = |k: &str| k == crate::kinds::INSTANCE;
        if data(&next_kind) != data(&current_kind) {
            return Err(text!(
                "update.error.switchData",
                card = card.name,
                current = current_kind,
                next = next_kind
            ));
        }
        let table = |k: &str| k == crate::kinds::TABLE;
        if table(&next_kind) != table(&current_kind) {
            return Err(text!(
                "update.error.switchTable",
                card = card.name,
                current = current_kind,
                next = next_kind
            ));
        }
        // Inside a container, one its container holds; else one its section holds.
        match project.container_of(card) {
            Some(container) => super::containers::require_holds(
                project.defs.cards(),
                &container.kind,
                &project.full(container),
                &next_kind,
            )?,
            None => require_fits(&project, &next_kind, &section)?,
        }
        // A container that becomes another kind lets go of nothing it holds.
        if project.is_container(&current_kind) && !project.inner_of(id).is_empty() {
            let holds = kinds
                .get(&next_kind)
                .map(|k| k.holds.clone())
                .unwrap_or_default();
            if let Some(held) = project
                .inner_of(id)
                .iter()
                .find(|c| !holds.contains(&c.kind))
            {
                return Err(text!(
                    "update.error.containerHolds",
                    card = card.name,
                    held = project.full(held),
                    heldKind = held.kind,
                    next = next_kind
                ));
            }
        }
        doc.insert("kind".into(), next_kind.clone().into());
    }

    // A data card's shape (`of`, a line of its own) and values (`data`, written whole: the
    // values whose fields its shape has lost stay until they are let go of).
    let is_data = next_kind == crate::kinds::INSTANCE;
    // A table's shape is its line `of` too.
    let is_table = next_kind == crate::kinds::TABLE;
    let screen = tx.who().side == Side::Screen;
    let of_given = value.get("of").filter(|v| !v.is_null());
    if !is_data && !is_table && of_given.is_some() {
        return Err(text!(
            "update.error.ofOnlyDataTable",
            card = card.name,
            kind = next_kind
        ));
    }
    if !screen {
        super::instances::no_of_line(value.get("connections"))?;
    }
    let of_given = match of_given {
        Some(Value::String(of)) => Some(of.clone()),
        Some(_) => {
            return Err(text!("update.error.ofPath"));
        }
        None => None,
    };
    let mut data_given: Option<Value> = None;
    if is_data && let Some(given) = value.get("data") {
        super::instances::data_shape(given)?;
        if !given.is_null() {
            let (kept, lost) = super::instances::with_strays(
                given.clone(),
                &super::instances::strays(&project, card),
            );
            for at in lost {
                warnings.push(text!(
                    "update.warn.dataStrayDropped",
                    card = project.full(card),
                    at
                ));
            }
            data_given = Some(kept);
        }
    }

    // What the kind holds, read from MCP's shape over what it was.
    let mut wire = Map::new();
    for (key, given) in value {
        if OWN_KEYS.contains(&key.as_str()) {
            continue;
        }
        if key == "data"
            && let Some(kept) = &data_given
        {
            wire.insert(key.clone(), kept.clone());
            continue;
        }
        if def.field(key).is_none() && def.row(key).is_none() {
            if empty(given) {
                continue;
            }
            // A goal's field by its name of before, one the board's kinds no longer have.
            if let Some(before) = crate::kinds::board_key_before(&next_kind, key) {
                return Err(before);
            }
            if !def.builtin {
                return Err(own_kind_lacks(def, key, &card.name));
            }
            return Err(text!(
                "update.error.keyOnlyOn",
                key,
                holders = holders(key),
                card = card.name,
                kind = next_kind
            ));
        }
        let given = match (key.as_str(), given) {
            ("label", Value::String(s)) => {
                Value::String(text(s, &text!("common.label.displayName"), 120)?)
            }
            ("description", Value::String(s)) => {
                Value::String(text(s, &text!("common.label.description"), 2000)?)
            }
            // Tags added or taken away, the rest left as they are.
            ("tags", Value::Object(edits)) => {
                let before: Vec<Value> = doc
                    .get("fields")
                    .and_then(|f| f.get("tags"))
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                Value::Array(tags_edited(before, edits)?)
            }
            // A body changed in part, over what it is (`in_part`).
            (_, Value::Object(_))
                if def.field(key).is_some_and(|f| {
                    matches!(
                        f.fieldtype,
                        crate::kinds::FieldType::Markdown | crate::kinds::FieldType::LongText
                    )
                }) =>
            {
                let before = doc
                    .get("fields")
                    .and_then(|f| f.get(key))
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                Value::String(super::in_part::changed(before, given, key)?.unwrap_or_default())
            }
            _ => given.clone(),
        };
        wire.insert(key.clone(), given);
    }
    let resolver = Resolver {
        project: &project,
        base: section.clone(),
        made: &HashMap::new(),
        inside: project.container_of(card).map(|c| project.full(c)),
        unmade: None,
    };
    // Its lines, in their place; a data card's shape among them, as `of` says or as it was.
    let lines_written = value.get("connections");
    if lines_written.is_some() || of_given.is_some() {
        let mut whole = match lines_written {
            Some(written) => super::lines::edited(&doc, written, &resolver, screen)?,
            None => doc
                .get("connections")
                .cloned()
                .unwrap_or(Value::Array(Vec::new())),
        };
        if is_data || is_table {
            super::instances::shape_line(&doc, &mut whole, of_given.as_deref());
        }
        super::lines::put_into(&mut doc, &whole, &resolver)?;
    }
    let prior = Value::Object(doc.clone());
    let parts = crate::kinds::from_wire(
        def,
        &wire,
        Some(&prior),
        &resolver,
        &mut crate::write::new_id,
    )
    .map_err(|problems| {
        problems
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n")
    })?;
    let label_before = doc
        .get("fields")
        .and_then(|f| f.get("label"))
        .cloned()
        .unwrap_or(Value::Null);
    {
        let fields = doc
            .entry("fields")
            .or_insert_with(|| Value::Object(Map::new()))
            .as_object_mut()
            .ok_or_else(|| text!("update.doc.fieldsNotObject"))?;
        for (key, read) in parts.fields {
            if empty(&read) {
                fields.remove(&key);
            } else {
                fields.insert(key, read);
            }
        }
        if fields.is_empty() {
            doc.remove("fields");
        }
    }
    {
        let rows = doc
            .entry("rows")
            .or_insert_with(|| Value::Object(Map::new()))
            .as_object_mut()
            .ok_or_else(|| text!("update.doc.rowsNotObject"))?;
        for (key, read) in parts.rows {
            if empty(&read) {
                rows.remove(&key);
            } else {
                rows.insert(key, read);
            }
        }
        if rows.is_empty() {
            doc.remove("rows");
        }
    }
    // A data card's values whose fields its shape has lost, let go of.
    if is_data && let Some(raw) = value.get("strayFields") {
        let dropped = super::instances::drop_strays(&project, card, &mut doc, raw)?;
        if !dropped.is_empty() {
            changed.push(keyed!(
                "update.changed.strayDropped",
                names = crate::text::List::of(&dropped)
            ));
        }
    }
    // What its kind no longer has (「定義に無い欄」), let go of: those
    // named (`{"<where>": null}`), or every one (`null`).
    else if let Some(raw) = value.get("strayFields") {
        let which: Option<HashSet<String>> = match raw {
            Value::Null => None,
            Value::Object(named) => {
                if let Some((key, _)) = named.iter().find(|(_, v)| !v.is_null()) {
                    return Err(text!("update.error.strayOnlyDelete", key));
                }
                Some(named.keys().cloned().collect())
            }
            _ => {
                return Err(text!("update.error.strayShape"));
            }
        };
        let there: Vec<String> = crate::kinds::strays(def, &Value::Object(doc.clone()))
            .into_iter()
            .map(|(at, _)| at)
            .collect();
        if let Some(which) = &which
            && let Some(missing) = which.iter().find(|w| !there.contains(w))
        {
            return Err(if there.is_empty() {
                text!("update.error.strayMissingNone", card = card.name, missing)
            } else {
                text!(
                    "update.error.strayMissingSome",
                    card = card.name,
                    missing,
                    names = crate::text::list(&there)
                )
            });
        }
        let dropped = crate::kinds::drop_strays(def, &mut doc, which.as_ref());
        if !dropped.is_empty() {
            changed.push(keyed!(
                "update.changed.strayUndefined",
                names = crate::text::List::of(&dropped)
            ));
        }
    }

    // What it holds that the new kind cannot show would be seen nowhere.
    if reworked {
        let mut stranded: Vec<String> = Vec::new();
        for (part, holds) in [("fields", false), ("rows", true)] {
            for key in doc
                .get(part)
                .and_then(Value::as_object)
                .into_iter()
                .flat_map(|o| o.keys())
            {
                let kept = if holds {
                    def.row(key).is_some()
                } else {
                    def.field(key).is_some()
                };
                if !kept {
                    stranded.push(held_label(key));
                }
            }
        }
        if !stranded.is_empty() {
            return Err(text!(
                "update.error.stranded",
                card = card.name,
                held = crate::text::list(&stranded),
                kind = next_kind
            ));
        }
    }

    // What the history says, in the Swift app's order.
    let label_now = doc
        .get("fields")
        .and_then(|f| f.get("label"))
        .cloned()
        .unwrap_or(Value::Null);
    if value.contains_key("label") && label_now != label_before {
        changed.push(keyed!("common.label.displayName"));
    }
    if reworked {
        changed.push(keyed!("update.changed.kind", kind = next_kind));
    }
    for key in SAID_ORDER {
        let Some(given) = value.get(key) else {
            continue;
        };
        if let Some(label) = super::cards::changed_label(key, given) {
            changed.push(label);
        }
    }
    // The kind's own fields and rows written, by name: what has no wording of its own above
    // (a trigger, `steps`, a goal's status), or the card would change with no history line.
    let own: Vec<&str> = value
        .keys()
        .map(String::as_str)
        .filter(|k| {
            !OWN_KEYS.contains(k)
                && !SAID_ORDER.contains(k)
                && *k != "label"
                && (def.field(k).is_some() || def.row(k).is_some())
        })
        .collect();
    if !own.is_empty() {
        changed.push(keyed!(
            "update.changed.fields",
            names = crate::text::List::of(&own)
        ));
    }

    // A rename, a new kind or rows that moved can break a path written elsewhere.
    let follows = renamed || !inner.is_empty();
    let broken_before = (follows || reworked).then(|| seen.broken_keys());
    let before = follows.then(|| seen.was());
    let display = project.full(card);
    let unresolved_before: HashSet<String> = problems(&project, card)
        .iter()
        .map(|(b, _)| unresolved_key(id, b))
        .collect();
    // Let go of before writing: what is worked out is brought up to the write only while nothing
    // else holds it (`Seen::in_tx`).
    drop(resolver);
    drop(project);
    drop(seen);
    put_checked(tx, scope, &doc)?;
    // Its lines held to the kinds of line once it is written (its kind may be new).
    if lines_written.is_some() || reworked || of_given.is_some() {
        super::lines::require_fit(tx, scope, id)?;
    }
    // A data card's values held to its shape once they are written: those written, not those
    // its shape changed under.
    if is_data && (value.contains_key("data") || of_given.is_some()) {
        let now = scope.detail(tx)?;
        let project = Project::new(&now);
        if let Some(card) = project.card(id) {
            let nothing = Value::Null;
            let written = value.get("data").unwrap_or(&nothing);
            warnings.extend(super::instances::said(
                &project,
                card,
                Some(written),
                super::instances::Pointer::Data,
            )?);
        }
    }
    // A shape's face held to its fields once it is written.
    if (next_kind == "entity" || next_kind == "value")
        && let Some(face) = value.get("face").filter(|f| !f.is_null())
    {
        let now = scope.detail(tx)?;
        let project = Project::new(&now);
        if let Some(card) = project.card(id) {
            super::faces::said(&project, card, face)?;
        }
    }
    // A table's rules held to its shape once they are written.
    if is_table
        && (of_given.is_some()
            || ["where", "columns", "sort", "unnest", "within"]
                .iter()
                .any(|k| value.contains_key(*k)))
    {
        let now = scope.detail(tx)?;
        let project = Project::new(&now);
        if let Some(card) = project.card(id) {
            super::tables::said(&project, card)?;
        }
    }
    if let Some(before) = before {
        let mut moves = InnerMoves::new();
        if !inner.is_empty() {
            moves.insert(id.to_string(), inner.to_vec());
        }
        let rewritten = follow(tx, scope, &before, &moves)?;
        if rewritten > 0 {
            changed.push(keyed!("update.changed.paths", rewritten));
            warnings.push(if !renamed && !inner.is_empty() {
                text!("update.warn.pathsRewrittenByRows", rewritten)
            } else {
                text!(
                    "update.warn.pathsRewrittenByName",
                    rewritten,
                    name = next_name
                )
            });
        }
    }
    store_in_full(tx, scope, &[id.to_string()])?;
    if !changed.is_empty() {
        tx.line(
            &scope.project,
            &scope.tree,
            "schema.update",
            "schema",
            Some(id),
            keyed!(
                "update.line.cardUpdated",
                display,
                changed = super::slashed(changed)
            ),
        )?;
    }
    if let Some(before) = broken_before {
        warnings.extend(
            newly_broken(tx, scope, &before, Some(id))?
                .iter()
                .map(broken_warning),
        );
    }
    warnings.extend(path_warnings(tx, scope, id, &unresolved_before)?);
    Ok(warnings)
}

// MARK: - How it looks

/// A grid cell or size as a call writes it: whole numbers, `{x, y}` or `{w, h}`.
fn whole(value: &Value, a: &str, b: &str) -> Option<(i64, i64)> {
    let o = value.as_object()?;
    let n = |k: &str| {
        let f = o.get(k)?.as_f64()?;
        (f == f.round()).then_some(f as i64)
    };
    Some((n(a)?, n(b)?))
}

/// The widest a place reaches from the origin, in cells (`Grid.reach`).
const REACH: i64 = 1_000_000;

/// Where `topLeft` puts a card, checked (`gridPoint`).
pub(crate) fn grid_point(value: &Value) -> Result<(i64, i64), String> {
    let (x, y) = whole(value, "x", "y").ok_or_else(|| text!("update.error.topLeftShape"))?;
    if x.abs() > REACH || y.abs() > REACH {
        return Err(text!("update.error.topLeftRange", reach = REACH));
    }
    Ok((x, y))
}

/// The size `size` gives a card, checked (`gridSize`, `GridSize.checked`): `{w}` sets the width
/// alone and leaves the height to the contents, `{w, h}` both. What is sent is the size whole, so
/// one form has one meaning whatever was there before.
pub(crate) fn grid_size(value: &Value) -> Result<super::grid::OwnSize, String> {
    let shape = || text!("update.error.sizeShape");
    let o = value.as_object().ok_or_else(shape)?;
    if o.keys().any(|k| k != "w" && k != "h") {
        return Err(shape());
    }
    let number = |v: &Value| {
        let f = v.as_f64()?;
        (f == f.round()).then_some(f as i64)
    };
    let w = o.get("w").and_then(number).ok_or_else(shape)?;
    let h = match o.get("h") {
        None | Some(Value::Null) => None,
        Some(v) => Some(number(v).ok_or_else(shape)?),
    };
    if !(4..=60).contains(&w) || h.is_some_and(|h| !(2..=120).contains(&h)) {
        let size = match h {
            Some(h) => format!("{w}×{h}"),
            None => format!("{{\"w\": {w}}}"),
        };
        return Err(text!("update.error.sizeRange", size));
    }
    Ok((w, h))
}

/// A card's box on its section's diagram, as the tree has it (main's beneath a branch that has
/// none).
fn spot(tx: &Tx, scope: &Scope, id: &str) -> Result<Spot, String> {
    Ok(tx
        .place_row(&scope.project, &scope.tree, "", "card", id)?
        .unwrap_or_default())
}

/// What is said to a size asked of a card that cannot carry one (`Kinds::sizable`): a flow's
/// node, drawn by its node's shape, or a card ghost, drawn at the size of the card it leads to.
pub(crate) fn not_sizable(kind: &str) -> String {
    text!("update.error.notResizable", kind)
}

/// Where a card's box sits on the map it is drawn on, beside the drawn boxes of the others there
/// (each by its path), and the margin kept round a box: its own section's diagram, or, inside a
/// container, the container's card map. None when it is not placed, is folded into the cards
/// holding it, or sits where boxes may overlap (a ui section, `grid::keeps_places`).
type Neighbours = (Rect, Vec<(String, Rect)>, i64);

fn neighbours(tx: &Tx, scope: &Scope, id: &str) -> Result<Option<Neighbours>, String> {
    let seen = Seen::in_tx(tx, scope)?;
    let project = seen.project();
    let layout = super::grid::Layout::read(tx.conn(), &scope.project)?;
    let card = project
        .card(id)
        .ok_or_else(|| text!("update.error.noCard"))?;
    if let Some(container) = project
        .container_of(card)
        .filter(|c| project.card_mapped(&c.kind))
    {
        let boxes = super::card_map::rects(&project, container, &layout);
        let Some(mine) = boxes.iter().find(|b| b.0 == id).map(|b| b.2) else {
            return Ok(None);
        };
        let others = boxes
            .into_iter()
            .filter(|b| b.0 != id)
            .map(|(_, path, rect)| (path, rect))
            .collect();
        return Ok(Some((mine, others, layout.margin)));
    }
    let section = project.section_of(card);
    if super::grid::keeps_places(&project, &section) {
        return Ok(None);
    }
    let boxes = super::grid::own_boxes(&project, &layout, &section);
    let Some(mine) = boxes
        .iter()
        .find(|b| b.id == id && b.drawn)
        .and_then(|b| Some((b.at?.0, b.at?.1, b.size.0, b.size.1)))
    else {
        return Ok(None);
    };
    let others = boxes
        .iter()
        .filter(|b| b.id != id && b.drawn)
        .filter_map(|b| {
            let (x, y) = b.at?;
            Some((b.path.clone(), (x, y, b.size.0, b.size.1)))
        })
        .collect();
    Ok(Some((mine, others, layout.margin)))
}

/// The boxes `mine` overlaps, or comes within `margin` of, by their paths.
fn crowded(mine: Rect, others: &[(String, Rect)], margin: i64) -> Vec<String> {
    let room = (
        mine.0 - margin,
        mine.1 - margin,
        mine.2 + 2 * margin,
        mine.3 + 2 * margin,
    );
    others
        .iter()
        .filter(|(_, r)| super::grid::intersects(*r, room))
        .map(|(path, _)| path.clone())
        .collect()
}

/// A card's size of its own, or none to go back to the project's (`setSize`). How it looks, kept
/// with where it sits: on its section's diagram, or, inside a container, on the container's card
/// map. The card stays where it is; with `warn`, what is said when its new box overlaps another
/// card, or the margin round one, that it did not before (`update.warn.sizeOverlaps`) — the
/// inspection's `overlaps` lists it too, and a tidy clears it.
fn set_size(
    tx: &mut Tx,
    scope: &Scope,
    id: &str,
    kind: &str,
    raw: &Value,
    warn: bool,
) -> Result<Vec<String>, String> {
    let container = {
        let seen = Seen::in_tx(tx, scope)?;
        let project = seen.project();
        if !project.defs.cards().sizable(kind) {
            return Err(not_sizable(kind));
        }
        project
            .card(id)
            .and_then(|c| project.container_of(c))
            .filter(|c| project.card_mapped(&c.kind))
            .map(|c| c.id.clone())
    };
    let size = if raw.is_null() {
        None
    } else {
        Some(grid_size(raw)?)
    };
    let before = if warn {
        neighbours(tx, scope, id)?
    } else {
        None
    };
    let (view, mut at) = match &container {
        Some(container) => (
            container.as_str(),
            super::card_map::spot(tx, scope, container, id)?,
        ),
        None => ("", spot(tx, scope, id)?),
    };
    at.w = size.map(|s| s.0);
    at.h = size.and_then(|s| s.1);
    tx.place(&scope.project, &scope.tree, view, "card", id, Some(&at))?;
    let Some((was, _, _)) = before else {
        return Ok(Vec::new());
    };
    let Some((mine, others, margin)) = neighbours(tx, scope, id)? else {
        return Ok(Vec::new());
    };
    let already = crowded(was, &others, margin);
    let over: Vec<String> = crowded(mine, &others, margin)
        .into_iter()
        .filter(|path| !already.contains(path))
        .collect();
    Ok(if over.is_empty() {
        Vec::new()
    } else {
        vec![text!(
            "update.warn.sizeOverlaps",
            over = crate::text::list(&over)
        )]
    })
}

/// How a formatdoc is drawn, or none for the project's default (`setDocView`).
fn set_doc_view(
    tx: &mut Tx,
    scope: &Scope,
    id: &str,
    kind: &str,
    raw: &Value,
) -> Result<(), String> {
    if kind != "formatdoc" {
        return Err(text!("update.error.docViewKind", kind));
    }
    match raw {
        Value::Null => tx.look(&scope.project, &scope.tree, id, "", "view", None),
        Value::String(v) if v == "rows" || v == "document" => {
            tx.look(&scope.project, &scope.tree, id, "", "view", Some(raw))
        }
        _ => Err(text!("update.error.viewValue")),
    }
}

/// The version a stackdoc has open, by its name, or none for the one on top (how it looks:
/// no commit, and it passes main's lock).
fn set_version(
    tx: &mut Tx,
    scope: &Scope,
    id: &str,
    kind: &str,
    raw: &Value,
) -> Result<(), String> {
    if kind != "stackdoc" {
        return Err(text!("update.error.versionKind", kind));
    }
    match raw {
        Value::Null => tx.look(&scope.project, &scope.tree, id, "", "version", None),
        Value::String(name) => {
            let names: Vec<String> = Seen::in_tx(tx, scope)?
                .project()
                .card(id)
                .map(|c| c.versions().into_iter().map(|v| v.name).collect())
                .unwrap_or_default();
            if !names.contains(name) {
                return Err(text!(
                    "update.error.noVersion",
                    name,
                    names = if names.is_empty() {
                        text!("update.version.none")
                    } else {
                        crate::text::list(&names)
                    }
                ));
            }
            tx.look(&scope.project, &scope.tree, id, "", "version", Some(raw))
        }
        _ => Err(text!("update.error.versionValue")),
    }
}

/// Whether a value or an enum is drawn as a card of its own on its section's diagram
/// (`standalone`, how it looks: no commit, and it passes main's lock). Off, it goes back to
/// being folded into what holds it; a branch says so over main's.
pub(crate) fn set_standalone(
    tx: &mut Tx,
    scope: &Scope,
    id: &str,
    kind: &str,
    raw: &Value,
) -> Result<(), String> {
    if kind != "value" && kind != "enum" {
        return Err(text!("update.error.standaloneKind", kind));
    }
    let on = match raw {
        Value::Bool(b) => *b,
        Value::Null => false,
        _ => return Err(text!("update.error.standaloneValue")),
    };
    let kept = if on {
        Some(Value::Bool(true))
    } else if crate::write::is_branch_tree(&scope.tree) {
        Some(Value::Bool(false))
    } else {
        None
    };
    tx.look(
        &scope.project,
        &scope.tree,
        id,
        "",
        "standalone",
        kept.as_ref(),
    )
}

/// The cards a placing would have overlapped, and the cell it went to instead.
type Jumped = (Vec<String>, (i64, i64));

/// A card put where a call says; when that overlaps another card of its section or comes
/// within the margin of one, it goes to the nearest cell clear of them instead
/// (`LayoutSettings.clearSpot`, as a hand's drop does). Returns the cards it would have
/// overlapped and where it went, when it went elsewhere.
fn place(
    tx: &mut Tx,
    scope: &Scope,
    id: &str,
    wanted: (i64, i64),
) -> Result<Option<Jumped>, String> {
    let (taken, w, h, margin, keeps) = {
        let seen = Seen::in_tx(tx, scope)?;
        let project = seen.project();
        let layout = super::grid::Layout::read(tx.conn(), &scope.project)?;
        // A card, or a card ghost: its box on its own section's diagram.
        let section = match project.ghost(id) {
            Some(ghost) => project.section_of_ghost(ghost),
            None => project.section_of(
                project
                    .card(id)
                    .ok_or_else(|| text!("update.error.noCard"))?,
            ),
        };
        let boxes = super::grid::own_boxes(&project, &layout, &section);
        let (w, h) = boxes
            .iter()
            .find(|b| b.id == id)
            .map(|b| b.size)
            .ok_or_else(|| text!("update.error.noCard"))?;
        // A value object or an enum folded into the cards holding it blocks nothing.
        let taken: Vec<(String, (i64, i64, i64, i64))> = boxes
            .iter()
            .filter(|b| b.id != id && b.drawn)
            .filter_map(|b| {
                let (x, y) = b.at?;
                Some((b.path.clone(), (x, y, b.size.0, b.size.1)))
            })
            .collect();
        let keeps = super::grid::keeps_places(&project, &section);
        (taken, w, h, layout.margin, keeps)
    };
    let room = (
        wanted.0 - margin,
        wanted.1 - margin,
        w + 2 * margin,
        h + 2 * margin,
    );
    let over: Vec<String> = taken
        .iter()
        .filter(|_| !keeps)
        .filter(|(_, t)| super::grid::intersects(*t, room))
        .map(|(name, _)| name.clone())
        .collect();
    let landed = if over.is_empty() {
        wanted
    } else {
        let rects: Vec<_> = taken.iter().map(|(_, t)| *t).collect();
        super::grid::clear_spot(margin, wanted, w, h, &rects)
    };
    let mut at = spot(tx, scope, id)?;
    at.x = Some(landed.0);
    at.y = Some(landed.1);
    at.pinned = true;
    tx.place(&scope.project, &scope.tree, "", "card", id, Some(&at))?;
    Ok((!over.is_empty()).then_some((over, landed)))
}

/// Cards no one placed, given room in their section: under the shortest of four columns,
/// each a card wide and its margin apart (`AppModel.placeUnplaced`, `nextSpot`); and the
/// cards a goal holds with no place on its card map, given one there in the order the goal
/// holds them (`card_map::unplaced`). Not a change to the project: no history, and not pinned.
pub(crate) fn place_unplaced(tx: &mut Tx, scope: &Scope) -> Result<(), String> {
    let (places, inside, nodes_unplaced) = {
        let seen = Seen::in_tx(tx, scope)?;
        // The cards and the card ghosts, each a box on its own section's diagram. A working
        // directory's file sits where the window lays it out until it is moved.
        let cards_placed = seen
            .detail
            .schemas
            .iter()
            .filter(|c| c.kind != "file")
            .all(|c| c.pos_x.is_some() && c.pos_y.is_some())
            && seen
                .detail
                .ghosts
                .iter()
                .all(|g| g.pos_x.is_some() && g.pos_y.is_some());
        let maps_placed = super::card_map::placed(&seen);
        // A flow's node cards each have a place on its card map.
        let flows: Vec<String> = seen
            .detail
            .schemas
            .iter()
            .filter(|c| c.kind == "flow" && c.nodes.iter().any(|n| n.top_left.is_none()))
            .map(|c| c.id.clone())
            .collect();
        if cards_placed && maps_placed && flows.is_empty() {
            return Ok(());
        }
        let project = seen.project();
        let layout = super::grid::Layout::read(tx.conn(), &scope.project)?;
        // A card placed with no place said keeps out of the groups' frames, so it joins none unasked.
        let frames = super::groups::frames(tx.conn(), &scope.project, &scope.tree)?;
        // Each flow's nodes are placed by its own alone: all worked out from this one view.
        let nodes_unplaced: Vec<_> = flows
            .into_iter()
            .map(|flow| {
                let spots = super::containers::nodes_placed(&project, &flow, layout.margin);
                (flow, spots)
            })
            .collect();
        let inside = if maps_placed {
            Vec::new()
        } else {
            super::card_map::unplaced(&seen, &layout, &frames)
        };
        let mut sections: Vec<String> = Vec::new();
        if !cards_placed {
            for section in seen
                .detail
                .schemas
                .iter()
                .filter(|c| c.kind != "file")
                .map(|c| project.section_of(c))
                .chain(
                    seen.detail
                        .ghosts
                        .iter()
                        .map(|g| project.section_of_ghost(g)),
                )
            {
                if !sections.contains(&section) {
                    sections.push(section);
                }
            }
        }
        let mut out = Vec::new();
        for section in sections {
            let boxes = super::grid::own_boxes(&project, &layout, &section);
            // What is drawn takes room; a value object or an enum folded into the cards
            // holding it is given a spot, but takes none.
            let mut taken: Vec<(i64, i64, i64, i64)> = boxes
                .iter()
                .filter(|b| b.drawn)
                .filter_map(|b| {
                    let (x, y) = b.at?;
                    Some((x, y, b.size.0, b.size.1))
                })
                .collect();
            for b in boxes
                .iter()
                .filter(|b| b.at.is_none())
                .filter(|b| project.card(&b.id).is_none_or(|c| c.kind != "file"))
            {
                let (w, h) = b.size;
                let blocked: Vec<(i64, i64, i64, i64)> = taken
                    .iter()
                    .chain(frames.get(&section).into_iter().flatten())
                    .copied()
                    .collect();
                let at = super::grid::next_spot(&blocked, w, layout.margin);
                if b.drawn {
                    taken.push((at.0, at.1, w, h));
                }
                out.push((b.id.clone(), at));
            }
        }
        (out, inside, nodes_unplaced)
    };
    for (flow, spots) in &nodes_unplaced {
        super::containers::place_nodes(tx, scope, flow, spots)?;
    }
    for (id, (x, y)) in places {
        let mut at = spot(tx, scope, &id)?;
        at.x = Some(x);
        at.y = Some(y);
        tx.place(&scope.project, &scope.tree, "", "card", &id, Some(&at))?;
    }
    super::card_map::put_unplaced(tx, scope, &inside)
}

// MARK: - The tools' writes

/// `card_update` of a card, less its enum's values and where it goes (`update_schema`).
pub(crate) fn update_card(
    tx: &mut Tx,
    scope: &Scope,
    id: &str,
    value: &Map<String, Value>,
) -> Result<Vec<String>, String> {
    let placing = value.get("topLeft").map(grid_point).transpose()?;
    let mut warnings = Vec::new();
    if value
        .keys()
        .any(|k| !["topLeft", "size", "view", "version", "standalone"].contains(&k.as_str()))
    {
        warnings.extend(edit_card(tx, scope, id, value, &[])?);
    }
    let kind = Seen::in_tx(tx, scope)?
        .project()
        .card(id)
        .map(|c| c.kind.clone())
        .ok_or_else(|| text!("update.error.noCard"))?;
    if let Some(raw) = value.get("size") {
        // Placed in the same call, the card goes clear of the others with its new size.
        warnings.extend(set_size(tx, scope, id, &kind, raw, placing.is_none())?);
    }
    if let Some(raw) = value.get("view") {
        set_doc_view(tx, scope, id, &kind, raw)?;
    }
    if let Some(raw) = value.get("version") {
        set_version(tx, scope, id, &kind, raw)?;
    }
    if let Some(raw) = value.get("standalone") {
        set_standalone(tx, scope, id, &kind, raw)?;
    }
    if let Some(at) = placing {
        warnings.extend(place_at(tx, scope, id, at)?);
    }
    Ok(warnings)
}

/// A card — or a card ghost — put where `topLeft` says on its section's diagram, and what
/// is said when it went to a clear cell nearby instead (`place`).
pub(crate) fn place_at(
    tx: &mut Tx,
    scope: &Scope,
    id: &str,
    at: (i64, i64),
) -> Result<Vec<String>, String> {
    // A card inside a container sits on the container's card map: a flow's node where it is
    // said (as a node was), a goal's card clear of the others.
    let (container, in_flow) = {
        let seen = Seen::in_tx(tx, scope)?;
        let project = seen.project();
        let container = project.card(id).and_then(|c| project.container_of(c));
        (
            container.map(|c| c.id.clone()),
            container.is_some_and(|c| c.kind == "flow"),
        )
    };
    if let Some(container) = &container
        && in_flow
    {
        let spot = Spot {
            x: Some(at.0),
            y: Some(at.1),
            w: None,
            h: None,
            pinned: true,
            look: None,
        };
        tx.place(
            &scope.project,
            &scope.tree,
            container,
            "card",
            id,
            Some(&spot),
        )?;
        return Ok(Vec::new());
    }
    if let Some(container) = container {
        let mut value = Map::new();
        let mut spot = Map::new();
        spot.insert(
            "topLeft".into(),
            serde_json::json!({ "x": at.0, "y": at.1 }),
        );
        let path = {
            let seen = Seen::in_tx(tx, scope)?;
            let project = seen.project();
            project
                .card(id)
                .map(|c| project.full(c))
                .unwrap_or_default()
        };
        value.insert(path, Value::Object(spot));
        let (_, jumped) =
            super::card_map::put_places(tx, scope, &container, &Value::Object(value))?;
        return Ok(jumped
            .iter()
            .map(|j| text!("update.warn.placedNearbyInContainer", at = j["topLeft"]))
            .collect());
    }
    Ok(match place(tx, scope, id, at)? {
        Some((over, (x, y))) => vec![text!(
            "update.warn.placedNearby",
            over = crate::text::list(&over),
            x,
            y
        )],
        None => Vec::new(),
    })
}

/// An enum's values replaced by those given (`set_enum_values`, `setEnumValues`).
pub(crate) fn set_values(tx: &mut Tx, scope: &Scope, id: &str, raw: &Value) -> Result<(), String> {
    let seen = Seen::in_tx(tx, scope)?;
    let project = seen.project();
    let card = project
        .card(id)
        .filter(|c| c.kind == "enum")
        .ok_or_else(|| text!("update.error.enumNeeded"))?;
    let full = project.full(card);
    let items = raw
        .as_array()
        .ok_or_else(|| text!("update.error.valuesNotArray"))?;
    let mut seen_values = HashSet::new();
    let mut rows = Vec::new();
    for item in items {
        let (value, label, description) = match item {
            Value::String(v) => (v.clone(), String::new(), None),
            Value::Object(o) => match o.get("name") {
                Some(Value::String(v)) => (
                    v.clone(),
                    o.get("label")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    o.get("description")
                        .map(super::enum_values::description)
                        .transpose()?
                        .flatten(),
                ),
                _ => return Err(text!("update.error.valuesItem")),
            },
            _ => return Err(text!("update.error.valuesItem")),
        };
        let value = value.trim().to_string();
        if value.is_empty() {
            return Err(text!("update.error.valueEmpty"));
        }
        if value.chars().count() > 80 {
            return Err(text!("update.error.valueTooLong"));
        }
        if !seen_values.insert(value.clone()) {
            return Err(text!("update.error.valueDuplicate", value));
        }
        let label = text(&label, &text!("common.label.displayName"), 120)?;
        let mut row = Map::new();
        row.insert("name".into(), value.into());
        if !label.is_empty() {
            row.insert("label".into(), label.into());
        }
        if let Some(description) = description {
            row.insert("description".into(), description.into());
        }
        rows.push(Value::Object(row));
    }
    let count = rows.len();
    let mut docs = scope.docs(tx)?;
    let mut doc = docs
        .remove(id)
        .ok_or_else(|| text!("update.error.noSchema"))?;
    let def = crate::kinds::builtin()
        .get("enum")
        .ok_or_else(|| text!("update.error.noEnumDef"))?;
    let mut wire = Map::new();
    wire.insert("values".into(), Value::Array(rows));
    let resolver = Resolver {
        project: &project,
        base: project.section_of(card),
        made: &HashMap::new(),
        inside: None,
        unmade: None,
    };
    let prior = Value::Object(doc.clone());
    let parts = crate::kinds::from_wire(
        def,
        &wire,
        Some(&prior),
        &resolver,
        &mut crate::write::new_id,
    )
    .map_err(|p| {
        p.iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n")
    })?;
    let doc_rows = doc
        .entry("rows")
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .ok_or_else(|| text!("update.doc.rowsNotObject"))?;
    match parts.rows.get("values") {
        Some(v) if !empty(v) => {
            doc_rows.insert("values".into(), v.clone());
        }
        _ => {
            doc_rows.remove("values");
        }
    }
    if doc_rows.is_empty() {
        doc.remove("rows");
    }
    put_checked(tx, scope, &doc)?;
    tx.line(
        &scope.project,
        &scope.tree,
        "enumValue.set",
        "enumValue",
        Some(id),
        keyed!("update.line.valuesSet", full, count),
    )
}

/// The most names a history line of a move lists, then a count.
const MOST_NAMED: usize = 12;

/// Cards moved to one section together (`move_schemas`, `moveSchemas`): what named them
/// — and what they named from where they were — follows; returns what was said.
pub(crate) fn move_cards(
    tx: &mut Tx,
    scope: &Scope,
    ids: &[String],
    to: &str,
) -> Result<Vec<String>, String> {
    let seen = Seen::in_tx(tx, scope)?;
    let project = seen.project();
    let target = super::section_id(&project, to)?;
    // The card ghosts among them move as cards do, to any section but where they point.
    let ghosts: Vec<&crate::model::Ghost> = ids.iter().filter_map(|id| project.ghost(id)).collect();
    let moving: Vec<&crate::model::Card> = ids
        .iter()
        .filter(|id| project.ghost(id).is_none())
        .map(|id| {
            project
                .card(id)
                .ok_or_else(|| text!("update.error.noSchema"))
        })
        .collect::<Result<_, _>>()?;
    super::cards::require_not_evacuation(&project, &target)?;
    for card in &moving {
        require_fits(&project, &card.kind, &target)?;
    }
    for ghost in &ghosts {
        require_fits(&project, crate::detail::GHOST, &target)?;
        if let Some(problem) = ghost
            .target
            .as_deref()
            .and_then(|t| super::ghosts::target_problem(&project, &target, t, Some(&ghost.id)))
        {
            return Err(format!("{}: {problem}", project.full_ghost(ghost)));
        }
    }
    let target_name = project.directory.full_section(&target);
    // What a card ghost points at does not end up in its own section by a move either: a
    // card (or a ghost) moved where a ghost that points at it is, or moved there with it.
    let after = |id: &str| -> Option<String> {
        if ids.iter().any(|i| i == id) {
            return Some(target.clone());
        }
        Some(match project.directory.entry_of(id)? {
            crate::directory::Entry::Card(i) => project.section_of(project.nth(i)),
            crate::directory::Entry::Ghost(g) => {
                project.section_of_ghost(&project.detail.ghosts[g])
            }
        })
    };
    for ghost in &project.detail.ghosts {
        let Some(pointed) = ghost.target.as_deref() else {
            continue;
        };
        if !ids.iter().any(|i| i == pointed) || after(&ghost.id) != after(pointed) {
            continue;
        }
        return Err(text!(
            "update.error.ghostPointsHere",
            card = project.named(pointed).unwrap_or_default(),
            section = target_name,
            ghost = project.full_ghost(ghost)
        ));
    }
    let mut taken: HashSet<String> = project
        .directory
        .members(&target)
        .iter()
        .map(|&i| project.nth(i))
        .filter(|c| !ids.contains(&c.id))
        .map(|c| c.name.clone())
        .chain(
            project
                .ghosts_in(&target)
                .into_iter()
                .filter(|g| !ids.contains(&g.id))
                .map(|g| g.name.clone()),
        )
        .collect();
    for name in moving
        .iter()
        .map(|c| &c.name)
        .chain(ghosts.iter().map(|g| &g.name))
    {
        if !taken.insert(name.clone()) {
            return Err(text!("update.error.nameTaken", section = target_name, name));
        }
    }
    let names: Vec<String> = moving
        .iter()
        .map(|c| project.full(c))
        .chain(ghosts.iter().map(|g| project.full_ghost(g)))
        .collect();
    let count = names.len();
    let before_broken: HashSet<String> = seen.broken_keys();
    let before = seen.was();
    // What is inside a container moved goes with it, into the same section.
    let inside: Vec<String> = ids
        .iter()
        .flat_map(|id| super::containers::inside_all(&project, id))
        .map(|c| c.id.clone())
        .collect();
    let mut docs = scope.docs(tx)?;
    for id in &inside {
        if let Some(mut doc) = docs.remove(id) {
            doc.insert("section".into(), target.clone().into());
            tx.put_card(&scope.project, &scope.tree, &doc)?;
        }
    }
    for id in ids {
        let mut doc = docs
            .remove(id)
            .ok_or_else(|| text!("update.error.noSchema"))?;
        doc.insert("section".into(), target.clone().into());
        tx.put_card(&scope.project, &scope.tree, &doc)?;
        // Its place was in the section it left: it finds room in the new one.
        let mut at = spot(tx, scope, id)?;
        if at.x.is_some() || at.y.is_some() {
            at.x = None;
            at.y = None;
            at.pinned = false;
            // On a branch a row with no cells stays, so main's place in the section left does
            // not show through.
            let keep =
                at.w.is_some() || at.h.is_some() || crate::write::is_branch_tree(&scope.tree);
            tx.place(
                &scope.project,
                &scope.tree,
                "",
                "card",
                id,
                keep.then_some(&at),
            )?;
        }
    }
    let moved: Vec<&str> = ids.iter().map(|id| id.as_str()).collect();
    super::frames::refuse(tx, scope, &moved)?;
    let rewritten = follow(tx, scope, &before, &InnerMoves::new())?;
    tx.line(
        &scope.project,
        &scope.tree,
        "schema.update",
        "section",
        Some(&target),
        if names.len() <= MOST_NAMED {
            keyed!(
                "update.line.cardsMoved",
                count,
                section = target_name,
                names = names.join(" / ")
            )
        } else {
            keyed!(
                "update.line.cardsMovedCapped",
                count,
                section = target_name,
                names = names[..MOST_NAMED].join(" / "),
                more = names.len() - MOST_NAMED
            )
        },
    )?;
    let mut said = Vec::new();
    if rewritten > 0 {
        said.push(text!("update.warn.pathsRewrittenByMove", rewritten));
    }
    said.extend(
        newly_broken(tx, scope, &before_broken, None)?
            .iter()
            .map(broken_by_move),
    );
    Ok(said)
}

/// A card's document given its place among the cards, where a card made with one keeps it
/// (after its definition's version); a document keeps no order of 0 (`next_order`).
fn put_order(doc: &mut Map<String, Value>, order: i64) {
    if order == 0 {
        doc.shift_remove("order");
    } else if let Some(kept) = doc.get_mut("order") {
        *kept = order.into();
    } else {
        let at = doc
            .keys()
            .position(|k| k == "v")
            .map_or(doc.len(), |i| i + 1);
        doc.shift_insert(at, "order".into(), order.into());
    }
}

/// The `order`s a section's cards had, as they are listed, told apart: one no greater than
/// the one before it is raised to one past that one. Orders that were apart stay as they
/// were.
fn told_apart(orders: impl IntoIterator<Item = i64>) -> Vec<i64> {
    let mut out: Vec<i64> = Vec::new();
    for order in orders {
        let next = match out.last() {
            Some(&last) if order <= last => last + 1,
            _ => order,
        };
        out.push(next);
    }
    out
}

/// A card put at `position` among its section's cards (`card_update`'s `position`),
/// counting from zero as they are listed — by `order`, then name — and leaving out the
/// cards of the sections inside it. The section's cards are given the `order`s they had,
/// handed out again in the new arrangement, so where they stand among the project's other
/// cards changes as little as it can; orders two of them shared are first told apart
/// (`told_apart`). No other card's `order` changes. Only `order` is written, which main's
/// lock lets through; a new arrangement is a line of the history, none when the card stays
/// where it was.
pub(crate) fn put_position(
    tx: &mut Tx,
    scope: &Scope,
    id: &str,
    position: i64,
) -> Result<(), String> {
    let (listed, display) = {
        let seen = Seen::in_tx(tx, scope)?;
        let project = seen.project();
        // A card ghost among its section's ghosts.
        if let Some(ghost) = project.ghost(id) {
            let listed: Vec<(String, i64)> = project
                .ghosts_in(&project.section_of_ghost(ghost))
                .into_iter()
                .map(|g| (g.id.clone(), g.sort_order))
                .collect();
            (listed, project.full_ghost(ghost))
        } else {
            let card = project
                .card(id)
                .ok_or_else(|| text!("update.error.noCard"))?;
            // A card inside a container among the cards beside it there.
            let listed: Vec<(String, i64)> = match &card.inside {
                Some(container) => project
                    .inner_of(container)
                    .iter()
                    .map(|c| (c.id.clone(), c.sort_order))
                    .collect(),
                None => project
                    .directory
                    .members(&project.section_of(card))
                    .iter()
                    .map(|&i| project.nth(i))
                    .map(|c| (c.id.clone(), c.sort_order))
                    .collect(),
            };
            (listed, project.full(card))
        }
    };
    if position < 0 || position >= listed.len() as i64 {
        return Err(text!(
            "update.error.positionRange",
            last = listed.len() as i64 - 1,
            position
        ));
    }
    let Some(from) = listed.iter().position(|(c, _)| c == id) else {
        return Err(text!("update.error.noCard"));
    };
    if from as i64 == position {
        return Ok(());
    }
    let orders = told_apart(listed.iter().map(|(_, order)| *order));
    let mut arranged: Vec<String> = listed.into_iter().map(|(c, _)| c).collect();
    let moving = arranged.remove(from);
    arranged.insert(position as usize, moving);
    let mut docs = scope.docs(tx)?;
    for (card, order) in arranged.iter().zip(orders) {
        let Some(mut doc) = docs.remove(card) else {
            continue;
        };
        put_order(&mut doc, order);
        tx.put_card(&scope.project, &scope.tree, &doc)?;
    }
    tx.line(
        &scope.project,
        &scope.tree,
        "schema.update",
        "schema",
        Some(id),
        keyed!("update.line.cardOrder", display),
    )
}

/// A card let go (`delete_schema`, `deleteSchema`): answered with its path, how many
/// references now point nowhere, and the paths elsewhere it leaves unresolved.
pub(crate) fn delete_card(tx: &mut Tx, scope: &Scope, id: &str) -> Result<Value, String> {
    let seen = Seen::in_tx(tx, scope)?;
    let project = seen.project();
    let card = project
        .card(id)
        .ok_or_else(|| text!("update.error.noSchema"))?;
    let full = project.full(card);
    let flow = super::containers::flow_of(&project, id).filter(|f| f != id);
    let before = seen.broken_keys();
    drop(seen);
    // The ways to it from the cards beside it go with it; what is inside it goes too, or
    // out into its place.
    super::containers::drop_ways_to(tx, scope, id)?;
    let (out_of_it, gone_with_it) = super::containers::empty_out(tx, scope, id)?;
    tx.remove_card(&scope.project, &scope.tree, id)?;
    tx.line(
        &scope.project,
        &scope.tree,
        "schema.delete",
        "schema",
        Some(id),
        keyed!("update.line.cardDeleted", full),
    )?;
    let newly = newly_broken(tx, scope, &before, None)?;
    // Every reference it leaves pointing nowhere — a line, a path in a body, a card ghost —
    // as the warnings say them.
    let referrers = newly.len();
    let mut warnings: Vec<String> = newly.iter().map(broken_warning).collect();
    if !out_of_it.is_empty() {
        warnings.push(text!(
            "update.warn.movedOut",
            full,
            names = crate::text::list(&out_of_it)
        ));
    }
    if let Some(flow) = flow {
        super::containers::require_flow(tx, scope, &flow)?;
    }
    let mut out = Map::new();
    if !warnings.is_empty() {
        out.insert(
            "warnings".into(),
            Value::Array(warnings.into_iter().map(Value::from).collect()),
        );
    }
    out.insert("deleted".into(), full.into());
    if !gone_with_it.is_empty() {
        out.insert(
            "deletedInside".into(),
            Value::Array(gone_with_it.into_iter().map(Value::from).collect()),
        );
    }
    out.insert("danglingRefs".into(), referrers.into());
    Ok(Value::Object(out))
}

#[cfg(test)]
mod position_tests {
    use super::{put_order, told_apart};
    use serde_json::{Map, Value, json};

    #[test]
    fn orders_apart_stay_and_shared_ones_are_raised_past_the_one_before() {
        assert_eq!(told_apart([2, 3, 4, 5]), [2, 3, 4, 5]);
        assert_eq!(told_apart([0, 0, 1]), [0, 1, 2]);
        assert_eq!(told_apart([3, 3, 3, 9]), [3, 4, 5, 9]);
        assert!(told_apart([]).is_empty());
    }

    #[test]
    fn an_order_goes_where_a_card_made_with_one_keeps_it_and_0_is_none() {
        let doc = |v: Value| v.as_object().cloned().unwrap_or_else(Map::new);
        let mut made = doc(json!({"id": "c", "kind": "entity", "name": "A", "v": 1, "fields": {}}));
        put_order(&mut made, 4);
        let keys: Vec<&str> = made.keys().map(String::as_str).collect();
        assert_eq!(keys, ["id", "kind", "name", "v", "order", "fields"]);
        put_order(&mut made, 7);
        assert_eq!(made["order"], 7);
        put_order(&mut made, 0);
        assert!(!made.contains_key("order"));
        let keys: Vec<&str> = made.keys().map(String::as_str).collect();
        assert_eq!(keys, ["id", "kind", "name", "v", "fields"]);
    }
}
