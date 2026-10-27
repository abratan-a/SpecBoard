//! The check of a tree, item by item (`inspection_list`, `inspection_read`, `inspection_update`;
//! `specboard://SpecBoard/Docs/Design:CoreApi` 点検). The check is worked out on each read
//! (`summary::check_rows` and `grid::overlaps`) and never kept, so an item is known by what it is:
//! its id comes from its group, where it is and what tells it from the others there, and is the same
//! in every tree and on every read; its print comes from all it says but the hints at how to fix it.
//! A mark the project keeps (`inspection_marks`) says an item is as designed while both match: one
//! table for the project, not a tree's content, so no history keeps it and a locked main takes it.

use super::{Args, Seen, cards::Scope};
use crate::mcp::Output;
use crate::text::Keyed;
use crate::write::{Side, Tx, Who};
use rusqlite::{Connection, params};
use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};

/// A group of items: the check that finds them, how it is said, and how its items are fixed.
pub(crate) struct Group {
    pub name: &'static str,
    /// Said for the reader to know (`info`), not to fix: it can be so by design.
    pub info: bool,
    /// The key of its heading in the text tables (`text`): said in the language of the call.
    label: &'static str,
    /// The key of how its items are fixed in the text tables.
    fix: &'static str,
    /// What, besides where it is, tells one item of the group from another there.
    keys: &'static [&'static str],
}

fn kind_of(info: bool) -> &'static str {
    if info { "info" } else { "fix" }
}

impl Group {
    fn kind(&self) -> &'static str {
        kind_of(self.info)
    }

    /// Its heading, in the language of the call.
    pub fn label(&self) -> String {
        crate::text::render(self.label, &[])
    }

    /// How its items are fixed, in the language of the call.
    pub fn fix(&self) -> String {
        crate::text::render(self.fix, &[])
    }
}

/// Every group, in the order they are said.
pub(crate) const GROUPS: [Group; 21] = [
    Group {
        name: "brokenPaths",
        info: false,
        label: "inspection.group.brokenPaths.label",
        fix: "inspection.group.brokenPaths.fix",
        keys: &["via", "path"],
    },
    Group {
        name: "missingFolders",
        info: false,
        label: "inspection.group.missingFolders.label",
        fix: "inspection.group.missingFolders.fix",
        keys: &["path"],
    },
    Group {
        name: "ambiguousPointers",
        info: false,
        label: "inspection.group.ambiguousPointers.label",
        fix: "inspection.group.ambiguousPointers.fix",
        keys: &[],
    },
    Group {
        name: "unresolvedPointers",
        info: false,
        label: "inspection.group.unresolvedPointers.label",
        fix: "inspection.group.unresolvedPointers.fix",
        keys: &["by"],
    },
    Group {
        name: "lineProblems",
        info: false,
        label: "inspection.group.lineProblems.label",
        fix: "inspection.group.lineProblems.fix",
        keys: &["problem"],
    },
    Group {
        name: "sectionProblems",
        info: false,
        label: "inspection.group.sectionProblems.label",
        fix: "inspection.group.sectionProblems.fix",
        keys: &["problem"],
    },
    Group {
        name: "cardProblems",
        info: false,
        label: "inspection.group.cardProblems.label",
        fix: "inspection.group.cardProblems.fix",
        keys: &["problem"],
    },
    Group {
        name: "strayFields",
        info: false,
        label: "inspection.group.strayFields.label",
        fix: "inspection.group.strayFields.fix",
        keys: &[],
    },
    Group {
        name: "flowProblems",
        info: false,
        label: "inspection.group.flowProblems.label",
        fix: "inspection.group.flowProblems.fix",
        keys: &["problem"],
    },
    Group {
        name: "peerMismatches",
        info: false,
        label: "inspection.group.peerMismatches.label",
        fix: "inspection.group.peerMismatches.fix",
        keys: &["peer", "carries"],
    },
    Group {
        name: "missingPartners",
        info: false,
        label: "inspection.group.missingPartners.label",
        fix: "inspection.group.missingPartners.fix",
        keys: &[],
    },
    Group {
        name: "missingStores",
        info: false,
        label: "inspection.group.missingStores.label",
        fix: "inspection.group.missingStores.fix",
        keys: &[],
    },
    Group {
        name: "unlinkedPartners",
        info: false,
        label: "inspection.group.unlinkedPartners.label",
        fix: "inspection.group.unlinkedPartners.fix",
        keys: &[],
    },
    Group {
        name: "tagGroupViolations",
        info: false,
        label: "inspection.group.tagGroupViolations.label",
        fix: "inspection.group.tagGroupViolations.fix",
        keys: &["tagGroup"],
    },
    Group {
        name: "freeCards",
        info: true,
        label: "inspection.group.freeCards.label",
        fix: "inspection.group.freeCards.fix",
        keys: &[],
    },
    Group {
        name: "storeActors",
        info: true,
        label: "inspection.group.storeActors.label",
        fix: "inspection.group.storeActors.fix",
        keys: &[],
    },
    Group {
        name: "unreferenced",
        info: true,
        label: "inspection.group.unreferenced.label",
        fix: "inspection.group.unreferenced.fix",
        keys: &[],
    },
    Group {
        name: "uncheckedPatterns",
        info: true,
        label: "inspection.group.uncheckedPatterns.label",
        fix: "inspection.group.uncheckedPatterns.fix",
        keys: &["regex"],
    },
    Group {
        name: "overlaps",
        info: true,
        label: "inspection.group.overlaps.label",
        fix: "inspection.group.overlaps.fix",
        keys: &[],
    },
    Group {
        name: "review",
        info: true,
        label: "inspection.group.review.label",
        fix: "inspection.group.review.fix",
        keys: &[],
    },
    Group {
        name: "codeChanged",
        info: true,
        label: "inspection.group.codeChanged.label",
        fix: "inspection.group.codeChanged.fix",
        keys: &["line"],
    },
];

/// What a row says to help fix it, not what it is: left out of its print, so a hint that changes
/// does not bring a marked item back. A kind of line's label is said in the language of the call,
/// and the item is the same in every language; the flows that call a begin with no partner now
/// change as other flows do.
const HINTS: [&str; 4] = ["hint", "moveTo", "lineLabel", "callers"];

fn group_named(name: &str) -> Option<&'static Group> {
    GROUPS.iter().find(|g| g.name == name)
}

fn group_names() -> String {
    crate::text::list(&GROUPS.iter().map(|g| g.name).collect::<Vec<_>>())
}

/// One thing the check found.
pub(crate) struct Item {
    pub id: String,
    pub group: &'static Group,
    /// Said for the reader to know, not to fix: its group's are, and so is what was found in a card
    /// done — a goal finished or cancelled, a task done, or inside one (`in_done`) — which is kept as
    /// a record and not worked on again.
    pub info: bool,
    pub at: String,
    pub problem: String,
    /// What the group says of it besides where it is and the sentence, in the row's order.
    pub content: Map<String, Value>,
    pub print: String,
}

/// A mark that an item is as designed.
#[derive(Clone, Debug)]
pub(crate) struct Mark {
    pub id: String,
    pub group: String,
    pub print: String,
    pub reason: String,
    pub by: String,
    pub at: String,
}

impl Mark {
    fn said(&self) -> Value {
        let mut out = Map::new();
        out.insert("reason".into(), self.reason.clone().into());
        out.insert("by".into(), by_said(&self.by).into());
        out.insert("at".into(), self.at.clone().into());
        Value::Object(out)
    }
}

/// How an item stands against the marks.
enum Standing<'a> {
    Open,
    /// Marked as it is now.
    Accepted(&'a Mark),
    /// Marked when it said something else: shown again, with the mark of before.
    Before(&'a Mark),
}

fn standing<'a>(item: &Item, marks: &'a HashMap<String, Mark>) -> Standing<'a> {
    match marks.get(&item.id) {
        None => Standing::Open,
        Some(m) if m.print == item.print => Standing::Accepted(m),
        Some(m) => Standing::Before(m),
    }
}

/// A value with every object's keys in order, as text: the same whatever order it was built in.
fn canonical(value: &Value) -> String {
    fn sorted(value: &Value) -> Value {
        match value {
            Value::Object(o) => {
                let mut keys: Vec<&String> = o.keys().collect();
                keys.sort();
                Value::Object(
                    keys.into_iter()
                        .map(|k| (k.clone(), sorted(&o[k])))
                        .collect(),
                )
            }
            Value::Array(a) => Value::Array(a.iter().map(sorted).collect()),
            other => other.clone(),
        }
    }
    crate::ojson::compact(&sorted(value))
}

/// All a row says but its hints, digested.
fn print_of(row: &Map<String, Value>) -> String {
    let mut bare = row.clone();
    for hint in HINTS {
        bare.shift_remove(hint);
    }
    crate::schema::print(&canonical(&Value::Object(bare)))
}

/// `<group>-<10 hex>`: the group, where the item is and what tells it apart there, digested —
/// with `more` when that is not enough to tell two apart.
fn id_of(group: &Group, row: &Map<String, Value>, more: Option<&str>) -> String {
    let mut key = String::from(group.name);
    key.push('\u{1}');
    key.push_str(row.get("at").and_then(Value::as_str).unwrap_or(""));
    for name in group.keys {
        key.push('\u{1}');
        key.push_str(&canonical(row.get(*name).unwrap_or(&Value::Null)));
    }
    if let Some(more) = more {
        key.push('\u{1}');
        key.push_str(more);
    }
    format!("{}-{}", group.name, &crate::schema::print(&key)[..10])
}

/// The sentence an item is said in, in the language of the call: the check's own where it has one
/// (kept by key in the row, so the item is the same in every language), else made from the row.
fn problem_of(group: &Group, row: &Map<String, Value>) -> String {
    let text = |key: &str| {
        row.get(key)
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string()
    };
    let number = |key: &str| row.get(key).map(crate::ojson::compact).unwrap_or_default();
    let joined = |key: &str| {
        row.get(key)
            .and_then(Value::as_array)
            .map(|a| crate::text::list(&a.iter().filter_map(Value::as_str).collect::<Vec<_>>()))
            .unwrap_or_default()
    };
    match group.name {
        "brokenPaths" if row.contains_key("planned") => text!(
            "inspection.problem.planned",
            via = text("via"),
            path = text("path"),
            goal = text("planned")
        ),
        "brokenPaths" => text!(
            "inspection.problem.brokenPath",
            via = text("via"),
            path = text("path")
        ),
        "ambiguousPointers" => text!(
            "inspection.problem.ambiguousPointer",
            count = number("candidates")
        ),
        "unresolvedPointers" => text!("inspection.problem.unresolvedPointer", by = text("by")),
        "missingFolders" => text!("inspection.problem.missingFolder", path = text("path")),
        "strayFields" => text!("inspection.problem.strayFields", fields = joined("fields")),
        "tagGroupViolations" => text!(
            "inspection.problem.tagGroupViolation",
            group = text("tagGroup"),
            count = number("count")
        ),
        "freeCards" => text!("inspection.problem.freeCard", section = text("section")),
        "unreferenced" => text!("inspection.problem.unreferenced"),
        "storeActors" => text!(
            "inspection.problem.storeActors",
            name = crate::pathtext::split_section(&text("at")).1.to_string(),
            actors = joined("actors")
        ),
        "overlaps" => text!("inspection.problem.overlap", over = joined("over")),
        "missingPartners" if row.contains_key("op") => {
            text!("inspection.problem.missingPartnerRow", op = text("op"))
        }
        "missingPartners" if row.contains_key("callers") => {
            text!(
                "inspection.problem.missingPartner",
                callers = joined("callers")
            )
        }
        "missingPartners" => text!("inspection.problem.missingPartnerNoCaller"),
        "missingStores" => missing_store(row, &text("op"), &text("data")),
        "unlinkedPartners" => {
            let flows = joined("flows");
            let actor = text("actor");
            let names = match (row.contains_key("flows"), row.contains_key("actor")) {
                (true, true) => text!("check.partner.flowsAndActor", flows, actor),
                (true, false) => text!("check.partner.flows", flows),
                _ => text!("check.partner.actor", actor),
            };
            text!(
                "inspection.problem.unlinkedPartner",
                op = text("op"),
                program = text("program"),
                names
            )
        }
        "review" => text!(
            "inspection.problem.review",
            cards = crate::text::list(
                &row.get("changed")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(|c| c["path"].as_str())
                    .collect::<Vec<_>>()
            )
        ),
        "codeChanged" => text!(
            "inspection.problem.codeChanged",
            kind = text("lineLabel"),
            lines = crate::text::list(
                &row.get("lines")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(|l| l["to"].as_str())
                    .collect::<Vec<_>>()
            )
        ),
        _ => row
            .get("problem")
            .and_then(Keyed::from_json)
            .map(|k| k.render())
            .unwrap_or_default(),
    }
}

/// What a `missingStores` item says: the shapes no store keeps, and those whose store is not
/// written as they reach two or more (`candidates`), each list by the cards' names alone.
fn missing_store(row: &Map<String, Value>, op: &str, data: &str) -> String {
    let named = |paths: Vec<&str>| {
        crate::text::list(
            &paths
                .into_iter()
                .map(|p| crate::pathtext::split_section(p).1)
                .collect::<Vec<_>>(),
        )
    };
    let candidates = row.get("candidates").and_then(Value::as_object);
    let shapes: Vec<&str> = row
        .get("shapes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    let (undecided, none): (Vec<&str>, Vec<&str>) = shapes
        .into_iter()
        .partition(|s| candidates.is_some_and(|c| c.contains_key(*s)));
    if undecided.is_empty() {
        return text!(
            "inspection.problem.missingStore",
            shapes = named(none),
            op = op,
            data = data
        );
    }
    let mut stores: Vec<&str> = Vec::new();
    for shape in &undecided {
        for store in candidates
            .and_then(|c| c.get(*shape))
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            if !stores.contains(&store) {
                stores.push(store);
            }
        }
    }
    if none.is_empty() {
        return text!(
            "inspection.problem.missingStoreUndecided",
            shapes = named(undecided),
            stores = named(stores),
            op = op,
            data = data
        );
    }
    text!(
        "inspection.problem.missingStoreBoth",
        none = named(none),
        shapes = named(undecided),
        stores = named(stores),
        op = op,
        data = data
    )
}

/// The paths of the cards of the tree that are done (`cross::finished`): a goal finished or
/// cancelled, a task done, and every card inside one.
fn done_cards(seen: &Seen) -> HashSet<String> {
    let project = seen.project();
    seen.detail
        .schemas
        .iter()
        .chain(seen.detail.inner.iter())
        .filter(|c| super::cross::finished(&project, c))
        .map(|c| project.full(c))
        .collect()
}

/// Whether `at` is one of the cards `done` or in one: the card's path itself, or it followed by a
/// field, a row or a card inside (`.`, `[`).
fn in_done(at: &str, done: &HashSet<String>) -> bool {
    done.contains(at)
        || at
            .char_indices()
            .any(|(i, c)| (c == '.' || c == '[') && done.contains(&at[..i]))
}

/// The check of the tree `seen` read, item by item, in the groups' order. `conn` reads the
/// project's layout, which the overlaps are worked out on.
pub(crate) fn items(seen: &Seen, conn: &Connection) -> Result<Vec<Item>, String> {
    let layout = super::grid::Layout::read(conn, &seen.summary.id)?;
    let overlaps = super::tidy::overlaps(&crate::store::Reader { conn }, seen, &layout)?;
    // A branch an open goal works in is its plan: code its lines name may be still to write.
    let planned = if crate::write::is_branch_tree(&seen.named.tree) {
        let board = super::cross::board(&crate::store::Reader { conn }, &seen.summary.id)?;
        super::cross::open_goal_of(&board, &seen.named.name)
    } else {
        None
    };
    let mut rows = super::summary::check_rows(seen, overlaps, planned.as_deref());
    let project = seen.project();
    let marks = super::review::marks(conn, &seen.summary.id, &seen.named.tree, &project, None)?;
    // In the project's order, as the other groups are found.
    for card in &seen.detail.schemas {
        let Some(found) = marks.get(&card.id) else {
            continue;
        };
        let mut row = Map::new();
        row.insert("at".into(), project.full(card).into());
        row.insert("changed".into(), super::review::listed(found));
        rows.push(("review", row));
    }
    // The cards whose implementation's code changed since it was recorded, on this machine.
    let code = super::code_review::by_card(super::code_review::lines(
        conn,
        &seen.summary.id,
        &project,
        None,
    )?);
    for card in seen.detail.schemas.iter().chain(seen.detail.inner.iter()) {
        let Some(found) = code.get(&card.id) else {
            continue;
        };
        // One item a kind of line: an implementation's and a test's function read apart.
        let mut kinds: Vec<&str> = Vec::new();
        for l in found.iter().filter(|l| l.changed()) {
            if !kinds.contains(&l.kind.as_str()) {
                kinds.push(&l.kind);
            }
        }
        for kind in kinds {
            let changed: Vec<Value> = found
                .iter()
                .filter(|l| l.changed() && l.kind == kind)
                .map(|l| {
                    let mut line = Map::new();
                    if !l.from.is_empty() {
                        line.insert("from".into(), l.from.clone().into());
                    }
                    line.insert("to".into(), l.to.clone().into());
                    if let Some(at) = &l.recorded_at {
                        line.insert("recordedAt".into(), at.clone().into());
                    }
                    Value::Object(line)
                })
                .collect();
            let label = project
                .defs
                .line(kind)
                .map_or_else(|| kind.to_string(), |d| d.said());
            let mut row = Map::new();
            row.insert("at".into(), project.full(card).into());
            row.insert("line".into(), kind.into());
            row.insert("lineLabel".into(), label.into());
            row.insert("lines".into(), Value::Array(changed));
            rows.push(("codeChanged", row));
        }
    }
    let done = done_cards(seen);
    // Stable: in a group, as the check found them.
    rows.sort_by_key(|(name, _)| GROUPS.iter().position(|g| g.name == *name));
    let mut given: HashMap<String, String> = HashMap::new();
    let mut out = Vec::new();
    for (name, row) in rows {
        let Some(group) = group_named(name) else {
            continue;
        };
        let print = print_of(&row);
        let mut id = id_of(group, &row, None);
        match given.get(&id) {
            // Found twice (the same path written twice in one text): one item.
            Some(had) if *had == print => continue,
            Some(_) => id = id_of(group, &row, Some(&print)),
            None => {}
        }
        if given.contains_key(&id) {
            continue;
        }
        given.insert(id.clone(), print.clone());
        let at = row
            .get("at")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let problem = problem_of(group, &row);
        let content: Map<String, Value> = row
            .into_iter()
            .filter(|(k, _)| k != "at" && k != "problem")
            .collect();
        let info = group.info || in_done(&at, &done) || content.contains_key("planned");
        out.push(Item {
            id,
            group,
            info,
            at,
            problem,
            content,
            print,
        });
    }
    Ok(out)
}

/// The project's marks, by item id.
pub(crate) fn marks(conn: &Connection, project: &str) -> Result<HashMap<String, Mark>, String> {
    let mut stmt = conn
        .prepare("SELECT id, grp, print, reason, by, at FROM inspection_marks WHERE project_id = ?")
        .map_err(crate::store::sql)?;
    let rows = stmt
        .query_map([project], |r| {
            Ok(Mark {
                id: r.get(0)?,
                group: r.get(1)?,
                print: r.get(2)?,
                reason: r.get(3)?,
                by: r.get(4)?,
                at: r.get(5)?,
            })
        })
        .map_err(crate::store::sql)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(crate::store::sql)?;
    Ok(rows.into_iter().map(|m| (m.id.clone(), m)).collect())
}

/// What `show` lets through.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Show {
    Fix,
    Info,
    /// What is not marked, of either kind: a group named for its information alone.
    Open,
    Accepted,
    All,
}

fn shown(show: Show, item: &Item, standing: &Standing) -> bool {
    match (show, standing) {
        (Show::All, _) => true,
        (Show::Accepted, Standing::Accepted(_)) => true,
        (Show::Accepted, _) | (_, Standing::Accepted(_)) => false,
        (Show::Fix, _) => !item.info,
        (Show::Info, _) => item.info,
        (Show::Open, _) => true,
    }
}

/// Where `in` keeps the items to: every section it is or holds, or a card and what is in it.
enum Within {
    Sections(HashSet<String>),
    Card(String),
}

impl Within {
    fn read(seen: &Seen, raw: &str) -> Result<Within, String> {
        let project = seen.project();
        let raw = raw.trim();
        if let Some(found) = project.directory.section_named(raw, None) {
            let parents: HashMap<&str, Option<&str>> = seen
                .detail
                .sections
                .iter()
                .map(|s| (s.id.as_str(), s.parent_id.as_deref()))
                .collect();
            let holds = |id: &str| {
                let mut at = Some(id);
                let mut steps = 0;
                while let Some(here) = at
                    && steps < 64
                {
                    if here == found {
                        return true;
                    }
                    at = parents.get(here).copied().flatten();
                    steps += 1;
                }
                false
            };
            let mut paths: HashSet<String> = seen
                .detail
                .sections
                .iter()
                .filter(|s| holds(&s.id))
                .map(|s| project.directory.full_section(&s.id))
                .collect();
            paths.insert(project.directory.full_section(&found));
            return Ok(Within::Sections(paths));
        }
        if crate::pathtext::has_section(raw) || seen.detail.sections.is_empty() {
            let card = super::walk::find_card(&project, raw, None)?;
            return Ok(Within::Card(project.full(card)));
        }
        Err(project
            .directory
            .section_problem(raw, &project.directory.home))
    }

    fn holds(&self, at: &str) -> bool {
        match self {
            Within::Sections(paths) => {
                let section = crate::pathtext::split_section(at).0.unwrap_or(at);
                paths.contains(section)
            }
            Within::Card(card) => {
                at == card
                    || at
                        .strip_prefix(card.as_str())
                        .is_some_and(|rest| rest.starts_with('.') || rest.starts_with('['))
            }
        }
    }
}

/// An item as `inspection_list` lists it: its kind said only when it is not its group's.
fn listed(item: &Item, standing: &Standing) -> Value {
    let mut out = Map::new();
    out.insert("id".into(), item.id.clone().into());
    out.insert("group".into(), item.group.name.into());
    if item.info != item.group.info {
        out.insert("kind".into(), kind_of(item.info).into());
    }
    out.insert("at".into(), item.at.clone().into());
    out.insert("problem".into(), item.problem.clone().into());
    if let Some(hint) = item.content.get("hint") {
        out.insert("hint".into(), hint.clone());
    }
    marked(&mut out, standing);
    Value::Object(out)
}

/// An item as `inspection_read` reads it: all the group says of it and how to fix it.
fn read_whole(item: &Item, standing: &Standing) -> Value {
    let mut out = Map::new();
    out.insert("id".into(), item.id.clone().into());
    out.insert("group".into(), item.group.name.into());
    out.insert("kind".into(), kind_of(item.info).into());
    out.insert("label".into(), item.group.label().into());
    out.insert("at".into(), item.at.clone().into());
    out.insert("problem".into(), item.problem.clone().into());
    for (k, v) in &item.content {
        out.insert(k.clone(), v.clone());
    }
    out.insert("fix".into(), item.group.fix().into());
    marked(&mut out, standing);
    Value::Object(out)
}

fn marked(out: &mut Map<String, Value>, standing: &Standing) {
    match standing {
        Standing::Open => {}
        Standing::Accepted(m) => {
            out.insert("accepted".into(), m.said());
        }
        Standing::Before(m) => {
            out.insert("acceptedBefore".into(), m.said());
        }
    }
}

/// `inspection_list`: the counts and groups of the whole tree, and its items as `value` keeps them.
pub(crate) fn list_view(
    seen: &Seen,
    items: &[Item],
    marks: &HashMap<String, Mark>,
    value: &Map<String, Value>,
) -> Result<Value, String> {
    // A group of information named without `show` is asked for, so its items are listed.
    let names_information = value.get("group").is_some_and(|given| {
        let named: Vec<&str> = match given {
            Value::String(s) => vec![s.as_str()],
            Value::Array(a) => a.iter().filter_map(Value::as_str).collect(),
            _ => Vec::new(),
        };
        named.iter().any(|n| group_named(n).is_some_and(|g| g.info))
    });
    let show = match value.get("show") {
        None if names_information => Show::Open,
        None => Show::Fix,
        Some(Value::String(s)) => match s.as_str() {
            "fix" => Show::Fix,
            "info" => Show::Info,
            "accepted" => Show::Accepted,
            "all" => Show::All,
            other => {
                return Err(text!("inspection.list.showOneOf", other));
            }
        },
        Some(other) => {
            return Err(text!("inspection.list.showOneOf", other));
        }
    };
    let groups: Option<HashSet<&str>> = match value.get("group") {
        None => None,
        Some(given) => {
            let named: Vec<&str> = match given {
                Value::String(s) => vec![s.as_str()],
                Value::Array(a) if !a.is_empty() && a.iter().all(Value::is_string) => {
                    a.iter().filter_map(Value::as_str).collect()
                }
                _ => return Err(text!("inspection.list.groupShape")),
            };
            let unknown: Vec<&str> = named
                .iter()
                .copied()
                .filter(|n| group_named(n).is_none())
                .collect();
            if !unknown.is_empty() {
                return Err(text!(
                    "inspection.list.noGroup",
                    unknown = crate::text::list(&unknown),
                    groups = group_names()
                ));
            }
            Some(named.into_iter().collect())
        }
    };
    let within = match value.get("in") {
        None => None,
        Some(Value::String(s)) => Some(Within::read(seen, s)?),
        Some(_) => return Err(text!("inspection.list.inShape")),
    };
    let mut counts = [0usize; 3];
    // A group's items not marked, those of them said as another kind than the group's, and those marked.
    let mut by_group: Vec<(&Group, [usize; 3])> = Vec::new();
    let mut listed_items = Vec::new();
    for item in items {
        let standing = standing(item, marks);
        let accepted = matches!(standing, Standing::Accepted(_));
        let slot = if accepted {
            2
        } else if item.info {
            1
        } else {
            0
        };
        counts[slot] += 1;
        let tally = match by_group.last_mut() {
            Some((g, tally)) if g.name == item.group.name => tally,
            _ => {
                by_group.push((item.group, [0; 3]));
                &mut by_group.last_mut().expect("a group pushed above").1
            }
        };
        if accepted {
            tally[2] += 1;
        } else {
            tally[0] += 1;
            if item.info != item.group.info {
                tally[1] += 1;
            }
        }
        if !shown(show, item, &standing)
            || groups
                .as_ref()
                .is_some_and(|g| !g.contains(item.group.name))
            || within.as_ref().is_some_and(|w| !w.holds(&item.at))
        {
            continue;
        }
        listed_items.push(listed(item, &standing));
    }
    let mut out = Map::new();
    out.insert("project".into(), seen.summary.name.clone().into());
    let mut said_counts = Map::new();
    said_counts.insert("fix".into(), counts[0].into());
    said_counts.insert("info".into(), counts[1].into());
    said_counts.insert("accepted".into(), counts[2].into());
    out.insert("counts".into(), Value::Object(said_counts));
    let mut said_groups = Map::new();
    for (group, [count, other, accepted]) in by_group {
        let mut row = Map::new();
        row.insert("kind".into(), group.kind().into());
        row.insert("label".into(), group.label().into());
        row.insert("count".into(), count.into());
        if other > 0 {
            row.insert(kind_of(!group.info).into(), other.into());
        }
        row.insert("accepted".into(), accepted.into());
        said_groups.insert(group.name.into(), Value::Object(row));
    }
    out.insert("groups".into(), Value::Object(said_groups));
    out.insert("items".into(), Value::Array(listed_items));
    Ok(Value::Object(out))
}

/// The ids a call names, each once, in the order given.
fn ids_of(args: &Args) -> Result<Vec<String>, String> {
    let mut ids = args
        .targets()?
        .ok_or_else(|| text!("inspection.ids.needed"))?;
    let mut seen = HashSet::new();
    ids.retain(|id| seen.insert(id.clone()));
    Ok(ids)
}

/// What an id no item of the tree has is told.
fn not_found(id: &str) -> String {
    text!("inspection.ids.notFound", id)
}

/// `inspection_read`: the items named, whole; the ids not found said apart, refused when none is.
pub(crate) fn read_view(
    seen: &Seen,
    items: &[Item],
    marks: &HashMap<String, Mark>,
    args: &Args,
) -> Result<Value, String> {
    let ids = ids_of(args)?;
    let mut found = Vec::new();
    let mut missing = Vec::new();
    for id in &ids {
        match items.iter().find(|i| i.id == *id) {
            Some(item) => found.push(read_whole(item, &standing(item, marks))),
            None => missing.push(id.clone()),
        }
    }
    if found.is_empty() {
        return Err(missing
            .iter()
            .map(|id| not_found(id))
            .collect::<Vec<_>>()
            .join("\n"));
    }
    let mut out = Map::new();
    out.insert("project".into(), seen.summary.name.clone().into());
    out.insert("items".into(), Value::Array(found));
    if !missing.is_empty() {
        out.insert("notFound".into(), missing.into());
        out.insert("hint".into(), text!("inspection.ids.notFoundHint").into());
    }
    Ok(Value::Object(out))
}

/// Who a mark is kept to be by: the access key's name through MCP, the window as `SCREEN` — said
/// as the window is in the language of whoever reads the mark (`by_said`).
pub(crate) fn by_of(who: &Who) -> String {
    match who.side {
        Side::Mcp => who.name.clone().unwrap_or_else(|| "MCP".to_string()),
        Side::Screen => SCREEN.to_string(),
    }
}

/// The window, as a mark keeps who it is by: no access key's name starts with `@`.
const SCREEN: &str = "@screen";

/// The window as marks made before kept it: the same as `SCREEN`.
const SCREEN_BEFORE: &str = "画面";

/// Who a mark is by, as the call reading it says it.
fn by_said(by: &str) -> String {
    if by == SCREEN || by == SCREEN_BEFORE {
        crate::write::author_said("screen", None)
    } else {
        by.to_string()
    }
}

/// What `accept` asks for: a mark with its reason, or none.
enum Accept {
    Mark(String),
    Unmark,
}

fn accept_of(value: &Map<String, Value>) -> Result<Accept, String> {
    match value.get("accept") {
        None => Err(text!("inspection.accept.needed")),
        Some(Value::String(s)) if s.trim().is_empty() => Err(text!("inspection.accept.empty")),
        Some(Value::String(s)) => Ok(Accept::Mark(s.trim().to_string())),
        Some(Value::Bool(false)) => Ok(Accept::Unmark),
        Some(Value::Bool(true)) => Err(text!("inspection.accept.notTrue")),
        Some(other) => Err(text!("inspection.accept.shape", other)),
    }
}

/// `inspection_update`, in a transaction already open: the marks of the items named in the tree
/// `scope` names written, or let go of — every id or none.
pub(crate) fn update(tx: &mut Tx, scope: &Scope, args: &Args, by: &str) -> Result<Value, String> {
    let value = args.value()?;
    super::check(&value, "inspection_update", "inspection")?;
    let accept = accept_of(&value)?;
    let ids = ids_of(args)?;
    let archived: Option<String> = tx
        .conn()
        .query_row(
            "SELECT archived_at FROM projects WHERE id = ?",
            [&scope.project],
            |r| r.get(0),
        )
        .map_err(crate::store::sql)?;
    if archived.is_some() {
        return Err(crate::write::archived());
    }
    let kept = marks(tx.conn(), &scope.project)?;
    let mut updated = Vec::new();
    match accept {
        Accept::Mark(reason) => {
            let seen = Seen::in_tx(tx, scope)?;
            let found = items(&seen, tx.conn())?;
            let missing: Vec<String> = ids
                .iter()
                .filter(|id| !found.iter().any(|i| i.id == **id))
                .map(|id| not_found(id))
                .collect();
            if !missing.is_empty() {
                return Err(missing.join("\n"));
            }
            let now = tx.now()?;
            for id in &ids {
                let item = found
                    .iter()
                    .find(|i| i.id == *id)
                    .expect("an item found above");
                let mark = Mark {
                    id: id.clone(),
                    group: item.group.name.to_string(),
                    print: item.print.clone(),
                    reason: reason.clone(),
                    by: by.to_string(),
                    at: now.clone(),
                };
                tx.conn()
                    .execute(
                        "INSERT OR REPLACE INTO inspection_marks (project_id, id, grp, print, reason, by, at) VALUES (?, ?, ?, ?, ?, ?, ?)",
                        params![scope.project, mark.id, mark.group, mark.print, mark.reason, mark.by, mark.at],
                    )
                    .map_err(crate::write::db)?;
                let mut row = Map::new();
                row.insert("id".into(), id.clone().into());
                row.insert("accepted".into(), mark.said());
                updated.push(Value::Object(row));
            }
        }
        Accept::Unmark => {
            let unmarked: Vec<String> = ids
                .iter()
                .filter(|id| !kept.contains_key(*id))
                .map(|id| text!("inspection.unmark.none", id))
                .collect();
            if !unmarked.is_empty() {
                return Err(unmarked.join("\n"));
            }
            for id in &ids {
                tx.conn()
                    .execute(
                        "DELETE FROM inspection_marks WHERE project_id = ? AND id = ?",
                        params![scope.project, id],
                    )
                    .map_err(crate::write::db)?;
                let mut row = Map::new();
                row.insert("id".into(), id.clone().into());
                row.insert("accepted".into(), Value::Null);
                updated.push(Value::Object(row));
            }
        }
    }
    // Every tree's check reads the marks: a window on main, and one on the tree marked in.
    tx.tell_of(&scope.project, "");
    if !scope.tree.is_empty() {
        tx.tell_of(&scope.project, &scope.tree);
    }
    let mut out = Map::new();
    out.insert("project".into(), scope.project_name.clone().into());
    out.insert("branch".into(), scope.tree_name.clone().into());
    out.insert("updated".into(), Value::Array(updated));
    Ok(Value::Object(out))
}

impl crate::Core {
    /// `inspection_list`, `inspection_read` and `inspection_update`.
    pub(crate) fn inspection_tool(
        &self,
        who: &Who,
        name: &str,
        args: &Args,
    ) -> Result<Output, String> {
        if name == "inspection_update" {
            let by = by_of(who);
            return self
                .plain_writing(who.clone(), |tx| {
                    let scope = super::scope_of(tx, args, None)?;
                    update(tx, &scope, args, &by)
                })
                .map(Output::Json);
        }
        let store = self.reading()?;
        let seen = Seen::read(
            &store,
            args.string("project")?,
            args.optional_string("branch"),
        )?;
        let found = items(&seen, &store.conn)?;
        let kept = marks(&store.conn, &seen.summary.id)?;
        let out = if name == "inspection_list" {
            let value = args.value()?;
            super::check(&value, "inspection_list", "project")?;
            list_view(&seen, &found, &kept, &value)?
        } else {
            if let Some(key) = args.value()?.keys().next() {
                return Err(text!("inspection.read.noValue", key));
            }
            read_view(&seen, &found, &kept, args)?
        };
        Ok(seen.headed(Output::Json(out)))
    }
}

#[cfg(test)]
mod tests {
    //! The check of the sample project (`tests/fixtures/projects/sample.json`), item by item, against
    //! the answer kept (`tests/fixtures/kept/reads/inspection.json`).

    use super::*;
    use crate::store::Store;

    /// Every item of the sample project's main, whole, as kept: the ids too, so a change to how
    /// they are made is seen (a mark made before would no longer meet its item).
    #[test]
    fn the_check_is_as_kept() {
        let store = Store::open(crate::fixtures::read_only_sample()).unwrap();
        let seen = Seen::read(&store, "見本", None).unwrap();
        let found = items(&seen, &store.conn).unwrap();
        let none = HashMap::new();
        let whole: Vec<Value> = found
            .iter()
            .map(|i| read_whole(i, &standing(i, &none)))
            .collect();
        crate::fixtures::kept(
            "reads/inspection.json",
            &serde_json::to_string_pretty(&Value::Array(whole)).unwrap(),
        );
    }

    #[test]
    fn every_group_is_said_in_the_order_of_the_table() {
        let names: Vec<&str> = GROUPS.iter().map(|g| g.name).collect();
        assert_eq!(
            names,
            [
                "brokenPaths",
                "missingFolders",
                "ambiguousPointers",
                "unresolvedPointers",
                "lineProblems",
                "sectionProblems",
                "cardProblems",
                "strayFields",
                "flowProblems",
                "peerMismatches",
                "missingPartners",
                "missingStores",
                "unlinkedPartners",
                "tagGroupViolations",
                "freeCards",
                "storeActors",
                "unreferenced",
                "uncheckedPatterns",
                "overlaps",
                "review",
                "codeChanged",
            ]
        );
        let info: Vec<&str> = GROUPS.iter().filter(|g| g.info).map(|g| g.name).collect();
        assert_eq!(
            info,
            [
                "freeCards",
                "storeActors",
                "unreferenced",
                "uncheckedPatterns",
                "overlaps",
                "review",
                "codeChanged"
            ]
        );
    }

    /// A hint changes nothing of what an item is; anything else it says does.
    #[test]
    fn a_print_leaves_out_the_hints_alone() {
        let row = |hint: &str, path: &str| {
            let mut row = Map::new();
            row.insert("at".into(), "Docs:Overview".into());
            row.insert("via".into(), "body".into());
            row.insert("path".into(), path.into());
            row.insert("hint".into(), hint.into());
            row
        };
        assert_eq!(print_of(&row("a", "X:Y")), print_of(&row("b", "X:Y")));
        assert_ne!(print_of(&row("a", "X:Y")), print_of(&row("a", "X:Z")));
        let group = group_named("brokenPaths").unwrap();
        assert_eq!(
            id_of(group, &row("a", "X:Y"), None),
            id_of(group, &row("b", "X:Y"), None)
        );
        assert_ne!(
            id_of(group, &row("a", "X:Y"), None),
            id_of(group, &row("a", "X:Z"), None)
        );
        assert!(id_of(group, &row("a", "X:Y"), None).starts_with("brokenPaths-"));
        assert_eq!(
            id_of(group, &row("a", "X:Y"), None).len(),
            "brokenPaths-".len() + 10
        );
    }

    /// Keys in another order are the same row.
    #[test]
    fn a_print_does_not_hang_on_the_order_of_keys() {
        let mut one = Map::new();
        one.insert("at".into(), "A:B".into());
        one.insert("over".into(), serde_json::json!(["A:C"]));
        let mut two = Map::new();
        two.insert("over".into(), serde_json::json!(["A:C"]));
        two.insert("at".into(), "A:B".into());
        assert_eq!(print_of(&one), print_of(&two));
    }
}
