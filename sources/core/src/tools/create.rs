//! Cards made by one call (`create_schemas` and `buildSchemas`): every card named first,
//! so the cards of a call point at each other whichever section they are in; then each
//! read from MCP's shape into its document and checked; nothing written while anything
//! is refused, every refusal said with the card it is about.

use super::args::Reason;
use super::cards::{
    Resolver, Scope, changed_label, identifier, made_kind, nest_fields, next_order, require_fits,
    text,
};
use crate::export::Project;
use crate::write::Tx;
use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};

/// One card a call makes: how it is said in a refusal, its section, its name, its value,
/// and the container it goes inside, by its full path (a card there, or one the call makes).
pub(crate) struct Making {
    pub label: String,
    pub section: String,
    pub name: String,
    pub value: Map<String, Value>,
    pub inside: Option<String>,
}

/// What a card's value holds besides what makes it: the keys that are its document's. Its
/// lines (`connections`) are read apart (`lines::put_into`), a data card's shape (`of`) with them.
const MAKING_KEYS: [&str; 6] = ["name", "kind", "label", "topLeft", "connections", "of"];

/// The cards made, and what was said about them. `unmade` are the cards (by their full paths) the
/// call was to make too and could not: a path to one is refused as that card's refusal,
/// and so is a path to one of these refused before it is read.
pub(crate) fn make_cards(
    tx: &mut Tx,
    scope: &Scope,
    entries: &[Making],
    unmade: &HashSet<String>,
) -> Result<(Vec<String>, Vec<String>), Vec<Reason>> {
    let mut refused: Vec<(usize, Reason)> = Vec::new();
    let reason = |at: &str, text: String| Reason {
        at: Some(at.to_string()),
        text,
    };
    let mut seen = std::collections::HashSet::new();
    for (i, e) in entries.iter().enumerate() {
        let place = e.inside.clone().unwrap_or_else(|| e.section.clone());
        if !seen.insert(format!("{place}\u{0}{}", crate::pathtext::fold(&e.name))) {
            refused.push((i, reason(&e.label, text!("create.duplicate.name"))));
        }
    }
    if !refused.is_empty() {
        return Err(refused.into_iter().map(|r| r.1).collect());
    }
    let fail = |e: String| vec![Reason { at: None, text: e }];
    // The project as the transaction keeps it worked out (`Seen::in_tx`), not read again.
    let seen = super::Seen::in_tx(tx, scope).map_err(fail)?;
    let files = || {
        seen.files.as_ref().map(|o| {
            let (d, i) = o.typed();
            Project::indexed(d, i.clone())
        })
    };
    let code = || seen.files.as_ref().map(|o| o.code.clone());
    // The board reaches the working directories' files as main does: beside main.
    let board = seen.named.tree == crate::store::BOARD;
    let project = Project::new(&seen.detail)
        .with_outside(
            seen.outside
                .as_ref()
                .map(|d| Project::new(d).with_files(files()).with_code(code())),
        )
        .with_files(if board { None } else { files() })
        .with_code(if board { None } else { code() });
    let docs = &seen.docs;
    // Named first: each made card's id and full path — a container before what goes inside
    // it, so a card inside one made in the call finds it.
    let mut made: HashMap<String, String> = HashMap::new();
    // The containers made in the call: by full path, their id, kind and section.
    let mut made_containers: HashMap<String, (String, String, String)> = HashMap::new();
    let mut named: Vec<(usize, String, String, String)> = Vec::new();
    // Each card's container (its id) and section, as it is made.
    let mut placed: HashMap<usize, (Option<String>, String)> = HashMap::new();
    // The cards not made: those refused before, and those refused here as they are named.
    let mut not_made = unmade.clone();
    let mut order_of: Vec<usize> = (0..entries.len()).collect();
    order_of.sort_by_key(|&i| {
        entries[i]
            .inside
            .as_ref()
            .map_or(0, |p| crate::pathtext::dots(p).len())
    });
    for i in order_of {
        let e = &entries[i];
        let checked = (|| -> Result<(String, String, Option<String>, String, String), String> {
            let name = identifier(&e.name, &text!("create.label.cardName"))?;
            let kind = made_kind(
                e.value
                    .get("kind")
                    .and_then(Value::as_str)
                    .unwrap_or("entity"),
                &project,
            )?;
            let kind = kind.as_str();
            super::cards::require_home(&project, kind)?;
            text(
                e.value.get("label").and_then(Value::as_str).unwrap_or(""),
                &text!("common.label.displayName"),
                120,
            )?;
            let (container, section, full) = match &e.inside {
                None => {
                    require_fits(&project, kind, &e.section)?;
                    super::cards::require_not_evacuation(&project, &e.section)?;
                    let full = crate::pathtext::card_path(
                        &project.directory.full_section(&e.section),
                        &name,
                    );
                    (None, e.section.clone(), full)
                }
                Some(container_path) => {
                    let (container, container_kind, section) = match made_containers
                        .get(crate::pathtext::canonical(container_path).as_ref())
                    {
                        Some((id, k, s)) => (id.clone(), k.to_string(), s.clone()),
                        None => {
                            let card = super::walk::find_card(&project, container_path, None)?;
                            (card.id.clone(), card.kind.clone(), project.section_of(card))
                        }
                    };
                    super::containers::require_holds(
                        project.defs.cards(),
                        &container_kind,
                        container_path,
                        kind,
                    )?;
                    let full =
                        crate::pathtext::member(&crate::pathtext::canonical(container_path), &name);
                    (Some(container), section, full)
                }
            };
            let place = container.clone().unwrap_or_else(|| section.clone());
            let taken = docs.values().any(|d| {
                crate::write::holder(d) == place
                    && d.get("name")
                        .and_then(Value::as_str)
                        .is_some_and(|n| crate::pathtext::same(n, &name))
            });
            if taken {
                return Err(text!("create.card.exists", name));
            }
            Ok((name, kind.to_string(), container, section, full))
        })();
        match checked {
            Ok((name, kind, container, section, full)) => {
                let id = crate::write::new_id();
                if project.is_container(&kind) {
                    made_containers
                        .insert(full.clone(), (id.clone(), kind.clone(), section.clone()));
                }
                made.insert(id.clone(), full);
                placed.insert(i, (container, section));
                named.push((i, id, kind, name));
            }
            Err(text) => {
                refused.push((i, reason(&e.label, text)));
                let name = e.name.trim();
                not_made.insert(match &e.inside {
                    Some(container) => crate::pathtext::member(container, name),
                    None => crate::pathtext::card_path(
                        &project.directory.full_section(&e.section),
                        name,
                    ),
                });
            }
        }
    }
    named.sort_by_key(|(i, ..)| *i);
    // Then each read into its document and checked.
    let kinds = project.defs.cards();
    let mut order = next_order(docs);
    // Each card made: its entry, id and document.
    let mut built: Vec<(usize, String, Map<String, Value>)> = Vec::new();
    // How each looks (`look` fields: a value's or an enum's `standalone`), apart from it.
    let mut looks: Vec<(String, Map<String, Value>)> = Vec::new();
    for (i, id, kind, name) in &named {
        let e = &entries[*i];
        let (container, section) = placed[i].clone();
        let resolver = Resolver {
            project: &project,
            base: section.clone(),
            made: &made,
            inside: e.inside.clone(),
            unmade: Some(&not_made),
        };
        let doc = (|| -> Result<Map<String, Value>, Vec<Reason>> {
            let mut wire = Map::new();
            for (key, value) in &e.value {
                if MAKING_KEYS.contains(&key.as_str()) {
                    continue;
                }
                let value = match key.as_str() {
                    "fields" => Value::Array(nest_fields(
                        &e.label,
                        value.as_array().map(Vec::as_slice).unwrap_or(&[]),
                    )?),
                    "values" => super::enum_values::wired(value),
                    _ => value.clone(),
                };
                wire.insert(key.clone(), value);
            }
            if let Some(label) = e
                .value
                .get("label")
                .and_then(Value::as_str)
                .filter(|l| !l.trim().is_empty())
            {
                wire.insert("label".into(), label.trim().into());
            }
            let def = kinds
                .get(kind.as_str())
                .ok_or_else(|| vec![reason(&e.label, text!("create.kind.cannotMake", kind))])?;
            // A goal's field by its name of before, one the board's kinds no longer have.
            let before: Vec<Reason> = wire
                .keys()
                .filter_map(|key| crate::kinds::board_key_before(kind, key))
                .map(|said| reason(&e.label, said))
                .collect();
            if !before.is_empty() {
                return Err(before);
            }
            let parts =
                crate::kinds::from_wire(def, &wire, None, &resolver, &mut crate::write::new_id)
                    .map_err(|problems| {
                        problems
                            .into_iter()
                            .map(|p| reason(&e.label, p.to_string()))
                            .collect::<Vec<_>>()
                    })?;
            if !parts.looks.is_empty() {
                looks.push((id.clone(), parts.looks.clone()));
            }
            let mut doc = Map::new();
            doc.insert("id".into(), id.clone().into());
            doc.insert("kind".into(), kind.clone().into());
            doc.insert("name".into(), name.clone().into());
            doc.insert("section".into(), section.clone().into());
            if let Some(container) = &container {
                doc.insert("in".into(), container.clone().into());
            }
            doc.insert("v".into(), kinds.version.into());
            if order != 0 {
                doc.insert("order".into(), order.into());
            }
            if !parts.fields.is_empty() {
                doc.insert("fields".into(), Value::Object(parts.fields));
            }
            if !parts.rows.is_empty() {
                doc.insert("rows".into(), Value::Object(parts.rows));
            }
            // Its lines: what each path names, among the cards the call makes too. A data
            // card's shape is one of them (`of`), written apart from them.
            let lines = super::instances::lines_made(kind, &e.value)
                .map_err(|t| vec![reason(&e.label, t)])?;
            if let Some(written) = &lines {
                super::lines::put_into(&mut doc, written, &resolver).map_err(|t| {
                    t.split('\n')
                        .map(|l| reason(&e.label, l.to_string()))
                        .collect::<Vec<_>>()
                })?;
            }
            let problems = crate::kinds::check_card(kinds, &Value::Object(doc.clone()));
            if !problems.is_empty() {
                return Err(problems
                    .into_iter()
                    .map(|p| reason(&e.label, p.to_string()))
                    .collect());
            }
            Ok(doc)
        })();
        order += 1;
        match doc {
            Ok(doc) => built.push((*i, id.clone(), doc)),
            Err(reasons) => refused.extend(reasons.into_iter().map(|r| (*i, r))),
        }
    }
    if !refused.is_empty() {
        refused.sort_by_key(|(i, _)| *i);
        return Err(refused.into_iter().map(|r| r.1).collect());
    }
    // What each one's history says it made — a card ghost, what it points at, as the project was
    // before the call —, said before any is written: the project as it was is let go of then
    // (`Seen::in_tx`).
    let saids: Vec<crate::text::Keyed> = built
        .iter()
        .map(|(_, id, doc)| {
            let full = &made[id];
            let target = doc
                .get("fields")
                .and_then(|f| f.get("target"))
                .and_then(Value::as_str);
            match target {
                Some(target)
                    if doc.get("kind").and_then(Value::as_str) == Some(crate::detail::GHOST) =>
                {
                    keyed!(
                        "create.line.ghostCreated",
                        full,
                        target = made
                            .get(target)
                            .cloned()
                            .or_else(|| project.named(target))
                            .unwrap_or_default()
                    )
                }
                _ => keyed!("create.line.created", full),
            }
        })
        .collect();
    // The containers there before the call that the cards go inside (`put_checked`'s, as the
    // project is before the first is written).
    let there: HashSet<String> = built
        .iter()
        .filter_map(|(_, _, doc)| doc.get("in").and_then(Value::as_str))
        .filter(|c| project.card(c).is_some())
        .map(str::to_string)
        .collect();
    drop(project);
    drop(seen);
    // Written, each with the lines its history says: the quick way when it can be, else a card at a
    // time as `put_checked` holds each to the project.
    let failed = |e: String| vec![Reason { at: None, text: e }];
    let quick = tx
        .alone(|tx| written_at_once(tx, scope, entries, &built, &saids, &made, there))
        .map_err(failed)?;
    if quick.is_err() {
        for ((i, id, doc), said) in built.iter().zip(&saids) {
            super::update::put_checked(tx, scope, doc).map_err(|e| {
                e.split('\n')
                    .map(|line| Reason {
                        at: Some(entries[*i].label.clone()),
                        text: line.to_string(),
                    })
                    .collect::<Vec<_>>()
            })?;
            said_made(tx, scope, &entries[*i], id, doc, &made[id], said).map_err(failed)?;
        }
    }
    let ids: Vec<String> = built.iter().map(|b| b.1.clone()).collect();
    // The card ghosts and the frames made, against the project with every card of the call in it:
    // not one in its own section, and no chain round (two made pointing at each other).
    {
        let now = super::Seen::in_tx(tx, scope).map_err(failed)?;
        let project = Project::new(&now.detail);
        let mut wrong = Vec::new();
        for (i, id, _) in &built {
            if let Some(frame) = project.card(id)
                && let Some(problem) = super::frames::problem(&project, frame)
            {
                wrong.push(reason(&entries[*i].label, problem));
            }
        }
        for (i, id, doc) in &built {
            let target = doc
                .get("fields")
                .and_then(|f| f.get("target"))
                .and_then(Value::as_str);
            let (Some(ghost), Some(target)) = (project.ghost(id), target) else {
                continue;
            };
            let section = project.section_of_ghost(ghost);
            if let Some(problem) =
                super::ghosts::target_problem(&project, &section, target, Some(id))
            {
                wrong.push(reason(&entries[*i].label, problem));
            }
        }
        if !wrong.is_empty() {
            return Err(wrong);
        }
    }
    // Their lines held to the kinds of line, against the project with every card of the call
    // in it (a line may reach one made after it).
    {
        let mut wrong = Vec::new();
        for (i, id, doc) in &built {
            if doc.contains_key("connections")
                && let Err(text) = super::lines::require_fit(tx, scope, id)
            {
                wrong.extend(
                    text.split('\n')
                        .map(|l| reason(&entries[*i].label, l.to_string())),
                );
            }
        }
        if !wrong.is_empty() {
            return Err(wrong);
        }
    }
    // Data cards' values held to their shapes, against the project with every card of the
    // call in it (a shape may be made after it).
    let mut warnings = Vec::new();
    let faced = |d: &Map<String, Value>| {
        (d["kind"] == "entity" || d["kind"] == "value")
            && d.get("fields").and_then(|f| f.get("face")).is_some()
    };
    if built.iter().any(|(_, _, d)| {
        d["kind"] == crate::kinds::INSTANCE || d["kind"] == crate::kinds::TABLE || faced(d)
    }) {
        let now = scope.detail(tx).map_err(failed)?;
        let project = Project::new(&now);
        let mut wrong = Vec::new();
        for (i, id, doc) in &built {
            let Some(card) = project.card(id) else {
                continue;
            };
            // A shape's face held to its fields.
            if faced(doc) {
                if let Err(text) = super::faces::said(&project, card, &doc["fields"]["face"]) {
                    wrong.extend(
                        text.split('\n')
                            .map(|l| reason(&entries[*i].label, l.to_string())),
                    );
                }
                continue;
            }
            // A table's rules held to its shape.
            if doc["kind"] == crate::kinds::TABLE {
                if let Err(text) = super::tables::said(&project, card) {
                    wrong.extend(
                        text.split('\n')
                            .map(|l| reason(&entries[*i].label, l.to_string())),
                    );
                }
                continue;
            }
            if doc["kind"] != crate::kinds::INSTANCE {
                continue;
            }
            match super::instances::said(&project, card, None, super::instances::Pointer::Data) {
                Ok(said) => warnings.extend(said),
                Err(text) => wrong.extend(
                    text.split('\n')
                        .map(|l| reason(&entries[*i].label, l.to_string())),
                ),
            }
        }
        if !wrong.is_empty() {
            return Err(wrong);
        }
    }
    // How they look, apart from them (no commit).
    for (id, parts) in &looks {
        for (field, value) in parts {
            let value = (!value.is_null() && value != &Value::Bool(false)).then_some(value);
            tx.look(&scope.project, &scope.tree, id, "", field, value)
                .map_err(failed)?;
        }
    }
    super::follow::store_in_full(tx, scope, &ids).map_err(failed)?;
    Ok((ids, warnings))
}

/// Why the cards of a call were not written at once (`written_at_once`): they are written again a
/// card at a time, as `put_checked` holds each to the project.
struct OneAtATime;

/// The cards of a call written at once: each held to
/// what `put_checked` holds it to by itself, and to the project as it is then not worked out after
/// each but once all are written — the container a card goes inside there before it (`there`, the
/// ones there before the call, and each written goes among them), and the flows they are of or
/// are, their nodes not wrong. As a flow's node is written, the flow holds the nodes written
/// before it (in their order) and what their lines reach then; the call only adds, so what is
/// wrong in it then is wrong once all are written too (a name twice, a second begin, a key a node
/// does not take, …) — but for a line to a card written after it, not a node of the same flow
/// (a row's `peer` or `key`: wrong without it, not with), and a node that cannot be read (the
/// flow holds none then). Those, and anything not as `put_checked` would have it, refused or
/// not, are `OneAtATime`: undone (`Tx::alone`) and written that way.
fn written_at_once(
    tx: &mut Tx,
    scope: &Scope,
    entries: &[Making],
    built: &[(usize, String, Map<String, Value>)],
    saids: &[crate::text::Keyed],
    made: &HashMap<String, String>,
    mut there: HashSet<String>,
) -> Result<(), OneAtATime> {
    let written_at: HashMap<&str, usize> = built
        .iter()
        .enumerate()
        .map(|(n, (_, id, _))| (id.as_str(), n))
        .collect();
    for (n, (_, _, doc)) in built.iter().enumerate() {
        let kind = doc.get("kind").and_then(Value::as_str).unwrap_or_default();
        if kind != "flow" && !crate::kinds::inner_only(kind) {
            continue;
        }
        let beside = doc.get("in");
        let reaches_later = doc
            .get("connections")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|line| line.get("to").and_then(Value::as_str))
            .any(|to| {
                written_at
                    .get(to)
                    .is_some_and(|&m| m > n && built[m].2.get("in") != beside)
            });
        if reaches_later {
            return Err(OneAtATime);
        }
    }
    let mut of_flows: Vec<&str> = Vec::new();
    for ((i, id, doc), said) in built.iter().zip(saids) {
        let (flow, node) = super::update::doc_checked(tx, scope, doc).map_err(|_| OneAtATime)?;
        match doc.get("in").and_then(Value::as_str) {
            Some(container) if there.contains(container) => {}
            None if !node => {}
            _ => return Err(OneAtATime),
        }
        tx.put_card(&scope.project, &scope.tree, doc)
            .map_err(|_| OneAtATime)?;
        there.insert(id.clone());
        if flow || node {
            of_flows.push(id);
        }
        said_made(tx, scope, &entries[*i], id, doc, &made[id], said).map_err(|_| OneAtATime)?;
    }
    if of_flows.is_empty() {
        return Ok(());
    }
    let seen = super::Seen::in_tx(tx, scope).map_err(|_| OneAtATime)?;
    let project = seen.project();
    let mut held: HashSet<String> = HashSet::new();
    for id in of_flows {
        let Some(flow) = super::containers::flow_of(&project, id) else {
            continue;
        };
        if !held.insert(flow.clone()) {
            continue;
        }
        // Each node read (none is when one cannot be), and none wrong.
        let read = project
            .card(&flow)
            .is_some_and(|c| c.nodes.len() == project.inner_of(&flow).len());
        if !read || super::update::flow_wrong(&project, &flow).is_some() {
            return Err(OneAtATime);
        }
    }
    Ok(())
}

/// The lines a card made says in its history: made (`said`), its fields added, its values set and
/// what else its value changed.
fn said_made(
    tx: &mut Tx,
    scope: &Scope,
    e: &Making,
    id: &str,
    doc: &Map<String, Value>,
    full: &str,
    said: &crate::text::Keyed,
) -> Result<(), String> {
    tx.line(
        &scope.project,
        &scope.tree,
        "schema.create",
        "schema",
        Some(id),
        said.clone(),
    )?;
    let name = doc["name"].as_str().unwrap_or_default();
    for field in field_names(doc) {
        tx.line(
            &scope.project,
            &scope.tree,
            "field.create",
            "field",
            None,
            keyed!("create.line.fieldAdded", name, field),
        )?;
    }
    if let Some(values) = doc
        .get("rows")
        .and_then(|r| r.get("values"))
        .and_then(Value::as_array)
        && !values.is_empty()
    {
        tx.line(
            &scope.project,
            &scope.tree,
            "schema.update",
            "schema",
            Some(id),
            keyed!("create.line.valuesSet", name, count = values.len()),
        )?;
    }
    let changed: Vec<crate::text::Keyed> = e
        .value
        .iter()
        .filter(|(k, _)| {
            ![
                "name", "kind", "label", "topLeft", "fields", "values", "versions",
            ]
            .contains(&k.as_str())
        })
        .filter_map(|(k, v)| changed_label(k, v))
        .collect();
    if !changed.is_empty() {
        tx.line(
            &scope.project,
            &scope.tree,
            "schema.update",
            "schema",
            Some(id),
            keyed!(
                "create.line.updated",
                full,
                changed = super::slashed(changed)
            ),
        )?;
    }
    Ok(())
}

/// A record's fields as they were made: shallowest first, each by its own name.
fn field_names(doc: &Map<String, Value>) -> Vec<String> {
    let mut levels: Vec<Vec<String>> = Vec::new();
    fn walk(rows: Option<&Value>, depth: usize, levels: &mut Vec<Vec<String>>) {
        for row in rows.and_then(Value::as_array).into_iter().flatten() {
            if levels.len() <= depth {
                levels.push(Vec::new());
            }
            levels[depth].push(
                row["fields"]["name"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string(),
            );
            walk(row["rows"].get("fields"), depth + 1, levels);
        }
    }
    walk(
        doc.get("rows").and_then(|r| r.get("fields")),
        0,
        &mut levels,
    );
    levels.into_iter().flatten().collect()
}
