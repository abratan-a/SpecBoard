//! What a container holds moved inside it (the store's mark `core_0006_containers`, a
//! project file before version 13).
//!
//! A flow kept its nodes as rows (`rows.nodes`), their ways by node name and the places they
//! name by path, and where each sat on its card map as a box of its own kind (`places` with
//! `item_kind` `node`, by name). A goal kept the cards it holds as a list of ids
//! (`fields.items`). Now both hold cards of their own inside them (`in`, the container's id):
//!
//! - A flow's node → a card of the node's kind in the flow (`nodes::card`: its id the row's,
//!   its ways and paths the flow's lines, what names nothing at the end of its description).
//!   Where it sat → its box on the flow's card map (`card`, the node card's id).
//! - A goal's items → each card put inside the goal (`in`, the goal's section, its place
//!   among the goal's cards the item's), in the order of the goal's list; what cannot go in —
//!   a card ghost, a card of a kind the goal does not hold, one another goal took first, one
//!   that would hold the goal, one that is not there — is the goal's `link` line instead (or
//!   let go when it names nothing), and said. Every path written as text that named what went
//!   in is written again on from its goal (`Goals:Design` → `Goals:Launch.Design`).
//!
//! Each tree is moved as it sees its cards: main and the board their own rows, a branch its
//! own over main's — its copies of a flow give copies of its nodes (a base from main's), and
//! a goal's list it changed gives copies of the cards that go in or come out. A base, a
//! branch's prints of main's cards, the history's rows (a flow's nodes as rows of their own,
//! a goal's cards inside it as the goal's list has them now) and an open request's print are
//! moved or taken again, as `line_move` does. A project's own kind of line named as one of the
//! flow's lines now is renamed first (`_2`). Each tree moved has a commit of one record.

use crate::directory::{CardRef, Directory, Entry, GhostRef, SectionRef};
use crate::store::BOARD;
use rusqlite::{Connection, params};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, HashMap, HashSet};

type Obj = Map<String, Value>;

fn sql(e: rusqlite::Error) -> String {
    text!("containerMove.error.sql", e)
}

fn text(doc: &Obj, key: &str) -> String {
    doc.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// A row of `card_docs` or `section_docs` of one tree.
#[derive(Clone)]
struct Row {
    id: String,
    removed: bool,
    doc: Option<Obj>,
    base: Option<Obj>,
    based_at: Option<String>,
}

fn rows(conn: &Connection, table: &str, project: &str, tree: &str) -> Result<Vec<Row>, String> {
    let mut stmt = conn
        .prepare(&format!(
            "SELECT id, removed, doc, base, based_at FROM {table} WHERE project_id = ? AND branch_id = ? ORDER BY id"
        ))
        .map_err(sql)?;
    stmt.query_map(params![project, tree], |r| {
        Ok(Row {
            id: r.get(0)?,
            removed: r.get::<_, i64>(1)? != 0,
            doc: crate::store::parse(r.get::<_, Option<String>>(2)?.as_deref()),
            base: crate::store::parse(r.get::<_, Option<String>>(3)?.as_deref()),
            based_at: r.get(4)?,
        })
    })
    .map_err(sql)?
    .collect::<Result<_, _>>()
    .map_err(sql)
}

/// What a tree sees of its sections and cards: main's under its own.
fn seen(
    main: (&[Row], &[Row]),
    own: Option<(&[Row], &[Row])>,
) -> (BTreeMap<String, Obj>, BTreeMap<String, Obj>) {
    let mut sections: BTreeMap<String, Obj> = BTreeMap::new();
    let mut cards: BTreeMap<String, Obj> = BTreeMap::new();
    for (section_rows, card_rows) in std::iter::once(main).chain(own) {
        for row in section_rows {
            if row.removed {
                sections.remove(&row.id);
            } else if let Some(doc) = &row.doc {
                sections.insert(row.id.clone(), doc.clone());
            }
        }
        for row in card_rows {
            if row.removed {
                cards.remove(&row.id);
            } else if let Some(doc) = &row.doc {
                cards.insert(row.id.clone(), doc.clone());
            }
        }
    }
    (sections, cards)
}

/// A flow's nodes of before, as its document keeps them.
fn node_rows(doc: &Obj) -> Vec<Value> {
    doc.get("rows")
        .and_then(|r| r.get("nodes"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

fn has_nodes(doc: &Obj) -> bool {
    doc.get("kind").and_then(Value::as_str) == Some("flow")
        && doc
            .get("rows")
            .and_then(Value::as_object)
            .is_some_and(|r| r.contains_key("nodes"))
}

fn has_items(doc: &Obj) -> bool {
    doc.get("fields")
        .and_then(Value::as_object)
        .is_some_and(|f| f.contains_key("items"))
}

/// A node row's id: its own, or one made from its flow and name.
fn node_id(flow: &str, row: &Value) -> String {
    row.get("id")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| {
            let name = row["fields"]["name"].as_str().unwrap_or_default();
            crate::ids::made_id(&["node", flow, name])
        })
}

/// A view to read paths in: the tree's sections and cards, and a flow's nodes of before as
/// the cards they become (inside the flow, by the row's id).
struct View {
    directory: Directory,
    ids: Vec<String>,
}

fn view(sections: &BTreeMap<String, Obj>, cards: &BTreeMap<String, Obj>) -> View {
    let section_refs: Vec<SectionRef> = sections
        .iter()
        .map(|(id, doc)| SectionRef {
            id: id.clone(),
            name: text(doc, "name"),
            parent_id: doc
                .get("parent_id")
                .and_then(Value::as_str)
                .map(str::to_string),
        })
        .collect();
    let mut card_refs = Vec::new();
    let mut ghost_refs = Vec::new();
    for (id, doc) in cards {
        let section = text(doc, "section");
        if text(doc, "kind") == crate::detail::GHOST {
            ghost_refs.push(GhostRef {
                id: id.clone(),
                name: text(doc, "name"),
                section_id: Some(section),
                target: doc
                    .get("fields")
                    .and_then(|f| f.get("target"))
                    .and_then(Value::as_str)
                    .map(str::to_string),
            });
            continue;
        }
        card_refs.push(CardRef {
            id: id.clone(),
            name: text(doc, "name"),
            section_id: Some(section.clone()),
            inside: doc
                .get("in")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .map(str::to_string),
        });
        if has_nodes(doc) {
            for row in node_rows(doc) {
                card_refs.push(CardRef {
                    id: node_id(id, &row),
                    name: row["fields"]["name"]
                        .as_str()
                        .unwrap_or_default()
                        .to_string(),
                    section_id: Some(section.clone()),
                    inside: Some(id.clone()),
                });
            }
        }
    }
    let ids = card_refs.iter().map(|c| c.id.clone()).collect();
    View {
        directory: Directory::new(&section_refs, &card_refs, &ghost_refs),
        ids,
    }
}

impl View {
    /// A path written in a card of `base`: the id it starts in, as written (a card ghost's
    /// own), and the names after it there.
    fn resolve(&self, raw: &str, base: &str) -> Option<(String, String)> {
        let (entry, rest) = self.directory.split_entry(raw, Some(base))?;
        let id = match entry {
            Entry::Card(i) => self.ids.get(i)?.clone(),
            Entry::Ghost(g) => self.directory.ghost_id(g)?,
        };
        Some((id, rest))
    }
}

// MARK: - A flow

/// A flow's document without its nodes, and the node cards they become.
struct Flowed {
    doc: Obj,
    nodes: Vec<Obj>,
    lost: Vec<crate::text::Keyed>,
}

/// A flow moved: its nodes made cards inside it, read in `view` (the tree it is written in).
fn flow(doc: &Obj, view: &View) -> Flowed {
    let id = text(doc, "id");
    let section = text(doc, "section");
    let rows = node_rows(doc);
    let ids: HashMap<String, String> = rows
        .iter()
        .map(|r| {
            (
                r["fields"]["name"].as_str().unwrap_or_default().to_string(),
                node_id(&id, r),
            )
        })
        .collect();
    let mut nodes = Vec::new();
    let mut lost = Vec::new();
    for (order, row) in rows.iter().enumerate() {
        let (node, row_id, step_ids) = crate::nodes::read_row(row);
        let card_id = if row_id.is_empty() {
            node_id(&id, row)
        } else {
            row_id
        };
        let mut seen: HashMap<String, usize> = HashMap::new();
        let resolve = |path: &str| view.resolve(path, &section);
        let line_card = card_id.clone();
        let mut line_id = |line: &str, from: &str, to: &str, at: &str| {
            let key = format!("{line}\u{1}{from}\u{1}{to}\u{1}{at}");
            let n = seen.entry(key).or_insert(0);
            *n += 1;
            crate::nodes::line_id(&line_card, line, from, to, at, *n)
        };
        let mut steps_made = 0usize;
        let step_card = card_id.clone();
        let mut new_id = || {
            steps_made += 1;
            crate::ids::made_id(&["step", &step_card, &steps_made.to_string()])
        };
        let made = crate::nodes::card(
            &node,
            &mut crate::nodes::Making {
                flow: &id,
                section: &section,
                order: order as i64,
                id: &card_id,
                step_ids: &step_ids,
                nodes: &ids,
                resolve: &resolve,
                line_id: &mut line_id,
                new_id: &mut new_id,
            },
        );
        lost.extend(made.lost);
        nodes.push(made.doc);
    }
    let mut out = doc.clone();
    if let Some(Value::Object(r)) = out.get_mut("rows") {
        r.shift_remove("nodes");
        if r.is_empty() {
            out.shift_remove("rows");
        }
    }
    out.insert("v".into(), crate::kinds::builtin().version.into());
    let name = text(doc, "name");
    Flowed {
        doc: out,
        nodes,
        lost: lost
            .into_iter()
            .map(|l| keyed!("containerMove.line.inCard", name = name.as_str(), line = l))
            .collect(),
    }
}

// MARK: - A goal's items

/// Where each card a goal's list held goes: inside which goal, at which place; and the ids
/// that could not go in, by goal, with why.
#[derive(Default, Clone)]
struct Assigned {
    /// card → (goal, place among the goal's cards)
    into: HashMap<String, (String, i64)>,
    /// goal → [(id, why)] the goal links to instead (or lets go of, when it names nothing)
    left: HashMap<String, Vec<(String, crate::text::Keyed, bool)>>,
}

/// The goals of a tree's cards, each list read in project order: a card goes inside the first
/// goal that lists it.
fn assign(cards: &BTreeMap<String, Obj>) -> Assigned {
    let mut goals: Vec<(&String, &Obj)> = cards
        .iter()
        .filter(|(_, d)| d.get("kind").and_then(Value::as_str) == Some("goal") && has_items(d))
        .collect();
    goals.sort_by(|a, b| {
        let order = |d: &Obj| d.get("order").and_then(Value::as_i64).unwrap_or(0);
        order(a.1)
            .cmp(&order(b.1))
            .then_with(|| text(a.1, "name").cmp(&text(b.1, "name")))
            .then_with(|| a.0.cmp(b.0))
    });
    let holds = crate::kinds::builtin()
        .get("goal")
        .map(|g| g.holds.clone())
        .unwrap_or_default();
    let mut out = Assigned::default();
    // Whether `card` is `goal` or holds it, by what is assigned so far.
    let holds_goal = |into: &HashMap<String, (String, i64)>, card: &str, goal: &str| {
        let mut at = goal.to_string();
        for _ in 0..64 {
            if at == card {
                return true;
            }
            match into.get(&at) {
                Some((up, _)) => at = up.clone(),
                None => return false,
            }
        }
        true
    };
    for (goal, doc) in goals {
        let items: Vec<String> = doc["fields"]["items"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect();
        let mut place = 0i64;
        // A name is one card's inside its goal.
        let mut names: HashSet<String> = HashSet::new();
        for item in items {
            let why = match cards.get(&item) {
                None => Some((keyed!("containerMove.reason.noTarget"), false)),
                Some(d) if text(d, "kind") == crate::detail::GHOST => {
                    Some((keyed!("containerMove.reason.ghost"), true))
                }
                Some(d) if !holds.contains(&text(d, "kind")) => {
                    Some((keyed!("containerMove.reason.kind"), true))
                }
                Some(_) if out.into.contains_key(&item) => {
                    Some((keyed!("containerMove.reason.otherGoal"), true))
                }
                Some(_) if holds_goal(&out.into, &item, goal) => {
                    Some((keyed!("containerMove.reason.cycle"), true))
                }
                Some(d) if names.contains(&crate::pathtext::fold(&text(d, "name"))) => {
                    Some((keyed!("containerMove.reason.sameName"), true))
                }
                Some(_) => None,
            };
            match why {
                None => {
                    if let Some(d) = cards.get(&item) {
                        names.insert(crate::pathtext::fold(&text(d, "name")));
                    }
                    out.into.insert(item, (goal.clone(), place));
                    place += 1;
                }
                Some((why, linked)) => out
                    .left
                    .entry(goal.clone())
                    .or_default()
                    .push((item, why, linked)),
            }
        }
    }
    out
}

/// A card's document with what a goal's list says of it: inside the goal (its section, its
/// place), or inside nothing.
fn put_inside(doc: &Obj, into: Option<&(String, i64)>, sections: &HashMap<String, String>) -> Obj {
    let mut out = doc.clone();
    match into {
        Some((goal, place)) => {
            out.insert("in".into(), goal.clone().into());
            if let Some(section) = sections.get(goal) {
                out.insert("section".into(), section.clone().into());
            }
            put_order(&mut out, *place);
        }
        None => {
            out.shift_remove("in");
        }
    }
    out.insert("v".into(), crate::kinds::builtin().version.into());
    out
}

fn put_order(doc: &mut Obj, order: i64) {
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

/// A goal's document without its list, what it could not hold its line to it instead.
fn goal_doc(doc: &Obj, left: &[(String, crate::text::Keyed, bool)]) -> Obj {
    let mut out = doc.clone();
    if let Some(Value::Object(fields)) = out.get_mut("fields") {
        fields.shift_remove("items");
        if fields.is_empty() {
            out.shift_remove("fields");
        }
    }
    let goal = text(doc, "id");
    let mut lines: Vec<Value> = out
        .get("connections")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    for (item, _, linked) in left {
        if !*linked {
            continue;
        }
        let already = lines.iter().any(|l| {
            l.get("line").and_then(Value::as_str) == Some("link")
                && l.get("from").is_none()
                && l.get("to").and_then(Value::as_str) == Some(item.as_str())
        });
        if already {
            continue;
        }
        let mut line = Obj::new();
        line.insert(
            "id".into(),
            crate::nodes::line_id(&goal, "link", "", item, "", 1).into(),
        );
        line.insert("line".into(), "link".into());
        line.insert("to".into(), item.clone().into());
        lines.push(Value::Object(line));
    }
    if !lines.is_empty() {
        out.insert("connections".into(), Value::Array(lines));
    }
    out.insert("v".into(), crate::kinds::builtin().version.into());
    out
}

// MARK: - Paths written as text

/// Every path a document writes as text that names a card that went inside a goal, written
/// on from its goal: `Goals:Design` → `Goals:Launch.Design`. `paths`: card → its path now.
fn rewrite_paths(doc: &mut Value, view: &View, moved: &HashMap<String, String>) -> usize {
    if moved.is_empty() {
        return 0;
    }
    let base = doc
        .get("section")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let again = |path: &str| -> Option<String> {
        let (entry, rest) = view.directory.split_entry(path, Some(&base))?;
        let Entry::Card(i) = entry else {
            return None;
        };
        let now = moved.get(view.ids.get(i)?)?;
        Some(if rest.is_empty() {
            now.clone()
        } else {
            format!("{now}.{rest}")
        })
    };
    let mut count = crate::tools::follow::rewrite(crate::kinds::builtin(), doc, &mut |p| again(p));
    // A line's landing written in full elsewhere, and the paths its fields hold.
    if let Some(Value::Array(lines)) = doc.get_mut("connections") {
        for line in lines.iter_mut().filter_map(Value::as_object_mut) {
            if let Some(at) = line.get("at").and_then(Value::as_str).map(str::to_string)
                && at.contains(':')
                && let Some(next) = again(&at)
                && next != at
            {
                line.insert("at".into(), next.into());
                count += 1;
            }
        }
    }
    count
}

// MARK: - A project

/// The flow's lines a project's own kind of line may have been named as before they were built
/// in: renamed (`_2`, `_3` …) with every connection of that kind.
fn rename_own_lines(conn: &Connection, project: &str) -> Result<Vec<crate::text::Keyed>, String> {
    let own: Option<String> = conn
        .query_row("SELECT kinds FROM projects WHERE id = ?", [project], |r| {
            r.get(0)
        })
        .map_err(sql)?;
    let Some(mut own) = own
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .and_then(|v| v.as_object().cloned())
    else {
        return Ok(Vec::new());
    };
    let Some(mut lines) = own.get("lines").and_then(Value::as_array).cloned() else {
        return Ok(Vec::new());
    };
    let builtin = crate::kinds::builtin_defs();
    let taken = |name: &str, lines: &[Value]| {
        builtin.line(name).is_some() || lines.iter().any(|l| l["line"] == name)
    };
    let mut renamed: HashMap<String, String> = HashMap::new();
    for i in 0..lines.len() {
        let name = lines[i]["line"].as_str().unwrap_or_default().to_string();
        if builtin.line(&name).is_none() {
            continue;
        }
        let mut n = 2;
        while taken(&format!("{name}_{n}"), &lines) {
            n += 1;
        }
        let next = format!("{name}_{n}");
        lines[i]["line"] = next.clone().into();
        renamed.insert(name, next);
    }
    if renamed.is_empty() {
        return Ok(Vec::new());
    }
    own.insert("lines".into(), Value::Array(lines));
    conn.execute(
        "UPDATE projects SET kinds = ? WHERE id = ?",
        params![Value::Object(own).to_string(), project],
    )
    .map_err(sql)?;
    let rename = |doc: &mut Obj| -> bool {
        let mut changed = false;
        if let Some(Value::Array(list)) = doc.get_mut("connections") {
            for line in list.iter_mut() {
                if let Some(next) = line
                    .get("line")
                    .and_then(Value::as_str)
                    .and_then(|n| renamed.get(n))
                {
                    line["line"] = next.clone().into();
                    changed = true;
                }
            }
        }
        changed
    };
    let all: Vec<(String, String, Option<String>, Option<String>)> = {
        let mut stmt = conn
            .prepare("SELECT branch_id, id, doc, base FROM card_docs WHERE project_id = ?")
            .map_err(sql)?;
        stmt.query_map([project], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
        })
        .map_err(sql)?
        .collect::<Result<_, _>>()
        .map_err(sql)?
    };
    for (tree, id, doc, base) in all {
        for (column, text) in [("doc", doc), ("base", base)] {
            let Some(mut parsed) = crate::store::parse(text.as_deref()) else {
                continue;
            };
            if rename(&mut parsed) {
                conn.execute(
                    &format!(
                        "UPDATE card_docs SET {column} = ? WHERE project_id = ? AND branch_id = ? AND id = ?"
                    ),
                    params![Value::Object(parsed).to_string(), project, tree, id],
                )
                .map_err(sql)?;
            }
        }
    }
    for ((commit, row), mut columns) in history(conn, project)? {
        let mut changed = false;
        for key in ["doc", "base"] {
            if let Some(mut doc) = columns
                .get(key)
                .and_then(Value::as_str)
                .and_then(|t| crate::store::parse(Some(t)))
                && rename(&mut doc)
            {
                columns.insert(key.into(), Value::Object(doc).to_string().into());
                changed = true;
            }
        }
        if changed {
            conn.execute(
                "UPDATE commit_rows SET before = ? WHERE commit_id = ? AND tbl = 'card_docs' AND row_id = ?",
                params![Value::Object(columns).to_string(), commit, row],
            )
            .map_err(sql)?;
        }
    }
    let mut said: Vec<(String, String)> = renamed.into_iter().collect();
    said.sort();
    Ok(said
        .into_iter()
        .map(|(from, to)| keyed!("containerMove.line.lineKindRenamed", from, to))
        .collect())
}

/// Every project of the store moved.
pub fn move_all(conn: &Connection) -> Result<(), String> {
    let projects: Vec<String> = {
        let mut stmt = conn.prepare("SELECT id FROM projects").map_err(sql)?;
        stmt.query_map([], |r| r.get(0))
            .map_err(sql)?
            .collect::<Result<_, _>>()
            .map_err(sql)?
    };
    for project in projects {
        move_project(conn, &project)?;
    }
    Ok(())
}

/// A row written into a tree's `card_docs`: inserted, or put over what is there.
#[allow(clippy::too_many_arguments)]
fn write_row(
    conn: &Connection,
    project: &str,
    tree: &str,
    id: &str,
    removed: bool,
    doc: Option<&Obj>,
    base: Option<&Obj>,
    based_at: Option<&str>,
) -> Result<(), String> {
    let keyed = doc.or(base).cloned().unwrap_or_default();
    conn.execute(
        "INSERT INTO card_docs (project_id, branch_id, id, section_id, name, kind, removed, doc, base, based_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(project_id, branch_id, id) DO UPDATE SET section_id = excluded.section_id, name = excluded.name, kind = excluded.kind, removed = excluded.removed, doc = excluded.doc, base = excluded.base, based_at = excluded.based_at",
        params![
            project,
            tree,
            id,
            crate::write::holder(&keyed),
            text(&keyed, "name"),
            text(&keyed, "kind"),
            removed as i64,
            doc.map(|d| Value::Object(d.clone()).to_string()),
            base.map(|b| Value::Object(b.clone()).to_string()),
            based_at,
        ],
    )
    .map_err(sql)?;
    Ok(())
}

/// What one tree's move did.
#[derive(Default)]
struct Done {
    flows: usize,
    nodes: usize,
    goals: usize,
    held: usize,
    rewritten: usize,
    said: Vec<crate::text::Keyed>,
    /// main's documents moved: id → (before, after), for a branch's prints.
    changed: HashMap<String, (String, String)>,
    /// main's node cards made: id → (its document, its flow's id, its path).
    made: Vec<(String, String, String, String)>,
}

/// One project's containers moved, in every tree.
pub fn move_project(conn: &Connection, project: &str) -> Result<(), String> {
    let open: Vec<(String, String, String)> = {
        let mut stmt = conn
            .prepare(&format!(
                "SELECT id, branch_id, print FROM {} WHERE project_id = ? AND state = 'open'",
                crate::store::requests_table(conn)?
            ))
            .map_err(sql)?;
        stmt.query_map([project], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .map_err(sql)?
            .collect::<Result<_, _>>()
            .map_err(sql)?
    };
    let mut current = Vec::new();
    for (id, branch, print) in &open {
        if crate::tools::requests::request_print(conn, project, branch)? == *print {
            current.push((id.clone(), branch.clone()));
        }
    }
    let renamed = rename_own_lines(conn, project)?;
    let branches: Vec<String> = {
        let mut stmt = conn
            .prepare("SELECT id FROM branches WHERE project_id = ? ORDER BY id")
            .map_err(sql)?;
        stmt.query_map([project], |r| r.get(0))
            .map_err(sql)?
            .collect::<Result<_, _>>()
            .map_err(sql)?
    };

    let main_sections = rows(conn, "section_docs", project, "")?;
    let main_cards = rows(conn, "card_docs", project, "")?;
    let (main_seen_sections, main_seen_cards) = seen((&main_sections, &main_cards), None);
    let main_view = view(&main_seen_sections, &main_seen_cards);
    let main_assigned = assign(&main_seen_cards);
    let main_paths = paths_after(&main_seen_sections, &main_seen_cards, &main_assigned);

    let mut done_main = Done::default();
    let mut trees: Vec<String> = vec![String::new(), BOARD.to_string()];
    trees.extend(branches.iter().cloned());
    for tree in &trees {
        let done = if tree.is_empty() {
            move_tree(
                conn,
                project,
                tree,
                &main_cards,
                (&main_seen_sections, &main_seen_cards),
                &main_view,
                &main_assigned,
                &main_paths,
                None,
            )?
        } else {
            let sections = rows(conn, "section_docs", project, tree)?;
            let cards = rows(conn, "card_docs", project, tree)?;
            let (tree_sections, tree_cards) = if tree == BOARD {
                seen((&sections, &cards), None)
            } else {
                seen((&main_sections, &main_cards), Some((&sections, &cards)))
            };
            let tree_view = view(&tree_sections, &tree_cards);
            let assigned = assign(&tree_cards);
            let paths = paths_after(&tree_sections, &tree_cards, &assigned);
            let main = (tree != BOARD).then_some(Mains {
                view: &main_view,
                assigned: &main_assigned,
                paths: &main_paths,
                cards: &main_seen_cards,
            });
            move_tree(
                conn,
                project,
                tree,
                &cards,
                (&tree_sections, &tree_cards),
                &tree_view,
                &assigned,
                &paths,
                main,
            )?
        };
        let mut lines = Vec::new();
        if tree.is_empty() {
            lines.extend(renamed.iter().cloned());
        }
        if done.flows > 0 {
            lines.push(keyed!(
                "containerMove.line.flows",
                flows = done.flows,
                nodes = done.nodes
            ));
        }
        if done.goals > 0 {
            lines.push(keyed!(
                "containerMove.line.goals",
                goals = done.goals,
                held = done.held
            ));
        }
        if done.rewritten > 0 {
            lines.push(keyed!(
                "containerMove.line.pathsRewritten",
                count = done.rewritten
            ));
        }
        lines.extend(done.said.iter().cloned());
        record(conn, project, tree, &lines)?;
        if tree.is_empty() {
            done_main = done;
        }
    }

    // A branch's prints of main's cards moved: what it remembered is what main had; a node
    // card of a flow it remembered as it was is remembered as it is made.
    for branch in &branches {
        let prints: Vec<(String, String)> = {
            let mut stmt = conn
                .prepare("SELECT card_id, print FROM branch_prints WHERE branch_id = ?")
                .map_err(sql)?;
            stmt.query_map([branch], |r| Ok((r.get(0)?, r.get(1)?)))
                .map_err(sql)?
                .collect::<Result<_, _>>()
                .map_err(sql)?
        };
        let kept_as_it_was: HashSet<String> = prints
            .iter()
            .filter(|(card, print)| {
                done_main
                    .changed
                    .get(card)
                    .is_some_and(|(before, _)| crate::schema::print(before) == *print)
            })
            .map(|(card, _)| card.clone())
            .collect();
        for (card, _) in &prints {
            if kept_as_it_was.contains(card)
                && let Some((_, after)) = done_main.changed.get(card)
            {
                conn.execute(
                    "UPDATE branch_prints SET print = ? WHERE branch_id = ? AND card_id = ?",
                    params![crate::schema::print(after), branch, card],
                )
                .map_err(sql)?;
            }
        }
        for (node, doc, flow, path) in &done_main.made {
            if kept_as_it_was.contains(flow) {
                conn.execute(
                    "INSERT OR REPLACE INTO branch_prints (branch_id, card_id, print, path) VALUES (?, ?, ?, ?)",
                    params![branch, node, crate::schema::print(doc), path],
                )
                .map_err(sql)?;
            }
        }
    }

    // The history's rows of card documents moved too.
    move_history(conn, project, &main_view, &main_assigned, &main_paths)?;

    for (id, branch) in current {
        let print = crate::tools::requests::request_print(conn, project, &branch)?;
        conn.execute(
            &format!(
                "UPDATE {} SET print = ? WHERE id = ?",
                crate::store::requests_table(conn)?
            ),
            params![print, id],
        )
        .map_err(sql)?;
    }
    Ok(())
}

/// main's, for a branch: how main is moved.
struct Mains<'a> {
    view: &'a View,
    assigned: &'a Assigned,
    paths: &'a HashMap<String, String>,
    cards: &'a BTreeMap<String, Obj>,
}

/// The paths now of the cards that go inside a goal (and what is inside them), by id.
fn paths_after(
    sections: &BTreeMap<String, Obj>,
    cards: &BTreeMap<String, Obj>,
    assigned: &Assigned,
) -> HashMap<String, String> {
    let section_refs: Vec<SectionRef> = sections
        .iter()
        .map(|(id, doc)| SectionRef {
            id: id.clone(),
            name: text(doc, "name"),
            parent_id: doc
                .get("parent_id")
                .and_then(Value::as_str)
                .map(str::to_string),
        })
        .collect();
    let paths = Directory::new(&section_refs, &[], &[]);
    let mut out: HashMap<String, String> = HashMap::new();
    fn path_of(
        id: &str,
        cards: &BTreeMap<String, Obj>,
        assigned: &Assigned,
        paths: &Directory,
        depth: usize,
    ) -> Option<String> {
        let doc = cards.get(id)?;
        let name = text(doc, "name");
        match assigned.into.get(id) {
            Some((goal, _)) if depth < 64 => Some(format!(
                "{}.{name}",
                path_of(goal, cards, assigned, paths, depth + 1)?
            )),
            _ => Some(format!(
                "{}:{name}",
                paths.full_section(&text(doc, "section"))
            )),
        }
    }
    let mut moved: Vec<&String> = assigned.into.keys().collect();
    moved.sort();
    for id in moved {
        if let Some(p) = path_of(id, cards, assigned, &paths, 0) {
            out.insert(id.clone(), p);
        }
    }
    out
}

/// One tree's card rows moved: its flows' nodes made cards, its goals' cards put inside them,
/// the paths that named those written again, the boxes of its nodes moved.
#[allow(clippy::too_many_arguments)]
fn move_tree(
    conn: &Connection,
    project: &str,
    tree: &str,
    own: &[Row],
    (_, tree_cards): (&BTreeMap<String, Obj>, &BTreeMap<String, Obj>),
    tree_view: &View,
    assigned: &Assigned,
    paths: &HashMap<String, String>,
    main: Option<Mains>,
) -> Result<Done, String> {
    let mut done = Done::default();
    // The goals' sections, for what goes inside them.
    let sections_of = |cards: &BTreeMap<String, Obj>| -> HashMap<String, String> {
        cards
            .iter()
            .map(|(id, d)| (id.clone(), text(d, "section")))
            .collect()
    };
    let goal_sections = sections_of(tree_cards);
    let main_goal_sections = main.as_ref().map(|m| sections_of(m.cards));
    let own_ids: HashSet<&str> = own.iter().map(|r| r.id.as_str()).collect();
    let main_view = main.as_ref().map(|m| m.view).unwrap_or(tree_view);
    let main_assigned = main.as_ref().map(|m| m.assigned).unwrap_or(assigned);
    let main_paths = main.as_ref().map(|m| m.paths).unwrap_or(paths);
    let main_sections_of = main_goal_sections.as_ref().unwrap_or(&goal_sections);

    // Each own row: its document and its base moved.
    for row in own {
        let doc = row.doc.as_ref();
        let base = row.base.as_ref();
        let is_flow = doc.or(base).is_some_and(has_nodes);
        if is_flow {
            let moved_doc = doc.map(|d| flow(d, tree_view));
            let moved_base = base.map(|b| flow(b, main_view));
            if let Some(m) = &moved_doc {
                done.said.extend(m.lost.iter().cloned());
            }
            write_row(
                conn,
                project,
                tree,
                &row.id,
                row.removed,
                moved_doc.as_ref().map(|m| &m.doc),
                moved_base.as_ref().map(|m| &m.doc),
                row.based_at.as_deref(),
            )?;
            if tree.is_empty()
                && let (Some(before), Some(m)) = (doc, &moved_doc)
            {
                done.changed.insert(
                    row.id.clone(),
                    (
                        Value::Object(before.clone()).to_string(),
                        Value::Object(m.doc.clone()).to_string(),
                    ),
                );
            }
            // Its nodes: a card each, a base from the base's node of the same id; a node the
            // base has and the document has not is removed (the branch let go of it).
            let doc_nodes: Vec<&Obj> = moved_doc.iter().flat_map(|m| m.nodes.iter()).collect();
            let base_nodes: Vec<&Obj> = moved_base.iter().flat_map(|m| m.nodes.iter()).collect();
            let base_of = |id: &str| base_nodes.iter().find(|n| text(n, "id") == id).copied();
            done.flows += usize::from(moved_doc.is_some() && !row.removed);
            for node in &doc_nodes {
                let id = text(node, "id");
                write_row(
                    conn,
                    project,
                    tree,
                    &id,
                    row.removed,
                    (!row.removed).then_some(*node),
                    base_of(&id),
                    base_of(&id).and(row.based_at.as_deref()),
                )?;
                if !row.removed {
                    done.nodes += 1;
                }
                if tree.is_empty()
                    && let Some(flow_doc) = doc
                {
                    done.made.push((
                        id.clone(),
                        Value::Object((*node).clone()).to_string(),
                        row.id.clone(),
                        node_path(tree_view, flow_doc, node),
                    ));
                }
            }
            if !row.removed {
                for node in &base_nodes {
                    let id = text(node, "id");
                    if doc_nodes.iter().any(|n| text(n, "id") == id) {
                        continue;
                    }
                    write_row(
                        conn,
                        project,
                        tree,
                        &id,
                        true,
                        None,
                        Some(node),
                        row.based_at.as_deref(),
                    )?;
                }
            } else {
                for node in &base_nodes {
                    let id = text(node, "id");
                    write_row(
                        conn,
                        project,
                        tree,
                        &id,
                        true,
                        None,
                        Some(node),
                        row.based_at.as_deref(),
                    )?;
                }
            }
        }
    }

    // Goals and what they hold: the tree's own rows; a card of main's whose place the tree's
    // lists change is copied into the tree.
    let mut goals_moved = HashSet::new();
    for (goal, left) in &assigned.left {
        for (item, why, linked) in left {
            let name = tree_cards
                .get(goal)
                .map(|d| text(d, "name"))
                .unwrap_or_default();
            let what = tree_cards
                .get(item)
                .map(|d| text(d, "name"))
                .unwrap_or_else(|| item.clone());
            if *linked {
                done.said.push(keyed!(
                    "containerMove.line.itemLinked",
                    name,
                    what,
                    why = why.clone()
                ));
            } else {
                done.said.push(keyed!(
                    "containerMove.line.itemLeft",
                    name,
                    what,
                    why = why.clone()
                ));
            }
        }
    }
    let empty: Vec<(String, crate::text::Keyed, bool)> = Vec::new();
    for row in own {
        let base = row.base.as_ref();
        let Some(doc) = row.doc.as_ref().or(base) else {
            continue;
        };
        if doc.get("kind").and_then(Value::as_str) == Some(crate::detail::GHOST) {
            continue;
        }
        let is_goal_list = row.doc.as_ref().is_some_and(has_items) || base.is_some_and(has_items);
        let goes = assigned.into.get(&row.id);
        let main_goes = main_assigned.into.get(&row.id);
        if !is_goal_list && goes.is_none() && main_goes.is_none() {
            continue;
        }
        let new_doc = row.doc.as_ref().map(|d| {
            let d = if has_items(d) {
                goals_moved.insert(row.id.clone());
                goal_doc(d, assigned.left.get(&row.id).unwrap_or(&empty))
            } else {
                d.clone()
            };
            put_inside(&d, goes, &goal_sections)
        });
        let new_base = base.map(|b| {
            let b = if has_items(b) {
                goal_doc(b, main_assigned.left.get(&row.id).unwrap_or(&empty))
            } else {
                b.clone()
            };
            put_inside(&b, main_goes, main_sections_of)
        });
        if new_doc.as_ref() == row.doc.as_ref() && new_base.as_ref() == base {
            continue;
        }
        write_row(
            conn,
            project,
            tree,
            &row.id,
            row.removed,
            new_doc.as_ref(),
            new_base.as_ref(),
            row.based_at.as_deref(),
        )?;
        if tree.is_empty()
            && let (Some(before), Some(after)) = (row.doc.as_ref(), new_doc.as_ref())
        {
            done.changed.insert(
                row.id.clone(),
                (
                    Value::Object(before.clone()).to_string(),
                    Value::Object(after.clone()).to_string(),
                ),
            );
        }
        if goes.is_some() && !row.removed {
            done.held += 1;
        }
    }
    // A card of main's the tree does not keep, whose place the tree's lists say apart from
    // main's: a copy of its own, main's (moved) as its base.
    if let Some(m) = &main {
        for (id, doc) in tree_cards {
            if own_ids.contains(id.as_str()) {
                continue;
            }
            let goes = assigned.into.get(id);
            let main_goes = m.assigned.into.get(id);
            if goes == main_goes {
                continue;
            }
            let base = put_inside(doc, main_goes, main_sections_of);
            let copy = put_inside(doc, goes, &goal_sections);
            write_row(
                conn,
                project,
                tree,
                id,
                false,
                Some(&copy),
                Some(&base),
                None,
            )?;
            if goes.is_some() {
                done.held += 1;
            }
        }
    }
    done.goals = goals_moved.len();

    // Every path written as text that named a card that went inside a goal, written again.
    if !paths.is_empty() {
        let now = rows(conn, "card_docs", project, tree)?;
        for row in now {
            let mut changed = false;
            let mut doc = row.doc.clone().map(Value::Object);
            if let Some(d) = doc.as_mut() {
                let n = rewrite_paths(d, tree_view, paths);
                done.rewritten += n;
                changed |= n > 0;
            }
            let mut base = row.base.clone().map(Value::Object);
            if let Some(b) = base.as_mut() {
                changed |= rewrite_paths(b, main_view, main_paths) > 0;
            }
            if changed {
                write_row(
                    conn,
                    project,
                    tree,
                    &row.id,
                    row.removed,
                    doc.as_ref().and_then(Value::as_object),
                    base.as_ref().and_then(Value::as_object),
                    row.based_at.as_deref(),
                )?;
                if tree.is_empty()
                    && let (Some(before), Some(after)) = (row.doc.as_ref(), doc.as_ref())
                {
                    let before_text = done
                        .changed
                        .get(&row.id)
                        .map(|(b, _)| b.clone())
                        .unwrap_or_else(|| Value::Object(before.clone()).to_string());
                    done.changed
                        .insert(row.id.clone(), (before_text, after.to_string()));
                }
            }
        }
    }
    if !main_paths.is_empty() && !tree.is_empty() && tree != BOARD && paths.is_empty() {
        let now = rows(conn, "card_docs", project, tree)?;
        for row in now {
            let mut base = row.base.clone().map(Value::Object);
            if let Some(b) = base.as_mut()
                && rewrite_paths(b, main_view, main_paths) > 0
            {
                write_row(
                    conn,
                    project,
                    tree,
                    &row.id,
                    row.removed,
                    row.doc.as_ref(),
                    base.as_ref().and_then(Value::as_object),
                    row.based_at.as_deref(),
                )?;
            }
        }
    }

    // Where the nodes sat: their boxes on the flow's card map.
    let boxes: Vec<(String, String)> = {
        let mut stmt = conn
            .prepare(
                "SELECT view, item FROM places WHERE project_id = ? AND branch_id = ? AND item_kind = 'node'",
            )
            .map_err(sql)?;
        stmt.query_map(params![project, tree], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(sql)?
            .collect::<Result<_, _>>()
            .map_err(sql)?
    };
    for (flow_id, name) in boxes {
        let card = tree_view
            .directory
            .entry(&flow_id, &name)
            .and_then(|e| match e {
                Entry::Card(i) => tree_view.ids.get(i).cloned(),
                Entry::Ghost(_) => None,
            });
        if let Some(card) = card {
            conn.execute(
                "INSERT OR REPLACE INTO places (project_id, branch_id, view, item_kind, item, x, y, w, h, pinned)
                 SELECT project_id, branch_id, view, 'card', ?, x, y, w, h, pinned FROM places
                 WHERE project_id = ? AND branch_id = ? AND view = ? AND item_kind = 'node' AND item = ?",
                params![card, project, tree, flow_id, name],
            )
            .map_err(sql)?;
        }
        conn.execute(
            "DELETE FROM places WHERE project_id = ? AND branch_id = ? AND view = ? AND item_kind = 'node' AND item = ?",
            params![project, tree, flow_id, name],
        )
        .map_err(sql)?;
    }
    // What a goal links to instead of holding does not sit on its card map.
    for (goal, left) in &assigned.left {
        for (item, _, _) in left {
            conn.execute(
                "DELETE FROM places WHERE project_id = ? AND branch_id = ? AND view = ? AND item_kind = 'card' AND item = ?",
                params![project, tree, goal, item],
            )
            .map_err(sql)?;
        }
    }
    Ok(done)
}

/// A node card's path: its flow's and its name.
fn node_path(view: &View, flow: &Obj, node: &Obj) -> String {
    format!(
        "{}:{}.{}",
        view.directory.full_section(&text(flow, "section")),
        text(flow, "name"),
        text(node, "name")
    )
}

// MARK: - The history

/// The history's rows of card documents of a project: by commit and row, its tree and seq, and
/// its columns.
type Kept = (String, String, String, i64, Obj);

/// A history's row by its commit and row id, and its columns.
type KeptRow = ((String, String), Obj);

/// A flow's rows in a tree, by the tree and the flow: each commit's seq, id and columns.
type FlowRows = BTreeMap<(String, String), Vec<(i64, String, Obj)>>;

fn history(conn: &Connection, project: &str) -> Result<Vec<KeptRow>, String> {
    Ok(history_full(conn, project)?
        .into_iter()
        .map(|(c, r, _, _, o)| ((c, r), o))
        .collect())
}

fn history_full(conn: &Connection, project: &str) -> Result<Vec<Kept>, String> {
    let kept: Vec<(String, String, String, i64, String)> = {
        let mut stmt = conn
            .prepare(
                "SELECT r.commit_id, r.row_id, c.branch_id, c.seq, r.before FROM commit_rows r JOIN commits c ON c.id = r.commit_id WHERE c.project_id = ? AND r.tbl = 'card_docs' AND r.before IS NOT NULL ORDER BY c.branch_id, c.seq",
            )
            .map_err(sql)?;
        stmt.query_map([project], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
        })
        .map_err(sql)?
        .collect::<Result<_, _>>()
        .map_err(sql)?
    };
    Ok(kept
        .into_iter()
        .filter_map(|(commit, row, tree, seq, before)| {
            Some((commit, row, tree, seq, crate::store::parse(Some(&before))?))
        })
        .collect())
}

/// The history's rows moved: a flow's row without its nodes, and a row of each node card
/// its commit changed (what the node was before, or none when it was not there yet); a
/// goal's row without its list, and a card's with the goal it went inside as main's lists
/// say now. Read in main's view, as `line_move` reads them.
fn move_history(
    conn: &Connection,
    project: &str,
    view: &View,
    assigned: &Assigned,
    paths: &HashMap<String, String>,
) -> Result<(), String> {
    let full = history_full(conn, project)?;
    // Each flow's rows in a tree, in commit order: what it was after each commit is what
    // the next one kept, or what the tree has now.
    let mut by_flow: FlowRows = BTreeMap::new();
    for (commit, row, tree, seq, columns) in &full {
        let is_flow = ["doc", "base"].iter().any(|k| {
            columns
                .get(*k)
                .and_then(Value::as_str)
                .and_then(|t| crate::store::parse(Some(t)))
                .is_some_and(|d| has_nodes(&d))
        });
        if is_flow {
            by_flow
                .entry((tree.clone(), row.clone()))
                .or_default()
                .push((*seq, commit.clone(), columns.clone()));
        }
    }
    let nodes_of = |columns: &Obj| -> (Vec<Obj>, Vec<Obj>) {
        let read = |key: &str| -> Vec<Obj> {
            columns
                .get(key)
                .and_then(Value::as_str)
                .and_then(|t| crate::store::parse(Some(t)))
                .filter(has_nodes)
                .map(|d| flow(&d, view).nodes)
                .unwrap_or_default()
        };
        (read("doc"), read("base"))
    };
    for ((tree, flow_id), list) in &by_flow {
        for (i, (_, commit, columns)) in list.iter().enumerate() {
            let (before_doc, before_base) = nodes_of(columns);
            // What the flow's nodes were after this commit.
            let after: Vec<String> = match list.get(i + 1) {
                Some((_, _, next)) => {
                    let (d, b) = nodes_of(next);
                    d.iter().chain(b.iter()).map(|n| text(n, "id")).collect()
                }
                None => {
                    let mut stmt = conn
                        .prepare(
                            "SELECT id FROM card_docs WHERE project_id = ? AND branch_id = ? AND section_id = ?",
                        )
                        .map_err(sql)?;
                    stmt.query_map(params![project, tree, flow_id], |r| r.get(0))
                        .map_err(sql)?
                        .collect::<Result<_, _>>()
                        .map_err(sql)?
                }
            };
            let removed = columns.get("removed").and_then(Value::as_i64).unwrap_or(0) != 0;
            let based_at = columns.get("based_at").cloned().unwrap_or(Value::Null);
            let mut ids: Vec<String> = before_doc
                .iter()
                .chain(before_base.iter())
                .map(|n| text(n, "id"))
                .collect();
            for id in after {
                if !ids.contains(&id) {
                    ids.push(id);
                }
            }
            for id in ids {
                let doc = before_doc.iter().find(|n| text(n, "id") == id);
                let base = before_base.iter().find(|n| text(n, "id") == id);
                let before: Option<String> = match (doc, base) {
                    (None, None) => None,
                    _ => {
                        let keyed = doc.or(base).cloned().unwrap_or_default();
                        let mut row = Obj::new();
                        row.insert("section_id".into(), crate::write::holder(&keyed).into());
                        row.insert("name".into(), text(&keyed, "name").into());
                        row.insert("kind".into(), text(&keyed, "kind").into());
                        row.insert(
                            "removed".into(),
                            if removed || doc.is_none() { 1 } else { 0 }.into(),
                        );
                        row.insert(
                            "doc".into(),
                            doc.filter(|_| !removed)
                                .map(|d| Value::String(Value::Object(d.clone()).to_string()))
                                .unwrap_or(Value::Null),
                        );
                        row.insert(
                            "base".into(),
                            base.map(|b| Value::String(Value::Object(b.clone()).to_string()))
                                .unwrap_or(Value::Null),
                        );
                        row.insert(
                            "based_at".into(),
                            if base.is_some() {
                                based_at.clone()
                            } else {
                                Value::Null
                            },
                        );
                        Some(Value::Object(row).to_string())
                    }
                };
                conn.execute(
                    "INSERT OR IGNORE INTO commit_rows (commit_id, tbl, row_id, before) VALUES (?, 'card_docs', ?, ?)",
                    params![commit, id, before],
                )
                .map_err(sql)?;
            }
        }
    }
    let sections: HashMap<String, String> = {
        let mut stmt = conn
            .prepare("SELECT id, doc FROM card_docs WHERE project_id = ? AND branch_id = ''")
            .map_err(sql)?;
        let all: Vec<(String, Option<String>)> = stmt
            .query_map([project], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(sql)?
            .collect::<Result<_, _>>()
            .map_err(sql)?;
        all.into_iter()
            .filter_map(|(id, doc)| {
                Some((id, text(&crate::store::parse(doc.as_deref())?, "section")))
            })
            .collect()
    };
    let empty: Vec<(String, crate::text::Keyed, bool)> = Vec::new();
    for (commit, row, _, _, mut columns) in full {
        let mut changed = false;
        for key in ["doc", "base"] {
            let Some(doc) = columns
                .get(key)
                .and_then(Value::as_str)
                .and_then(|t| crate::store::parse(Some(t)))
            else {
                continue;
            };
            let mut next = doc.clone();
            if has_nodes(&next) {
                next = flow(&next, view).doc;
            }
            if has_items(&next) {
                next = goal_doc(&next, assigned.left.get(&row).unwrap_or(&empty));
            }
            if let Some(goes) = assigned.into.get(&row) {
                next = put_inside(&next, Some(goes), &sections);
            }
            let mut as_value = Value::Object(next);
            rewrite_paths(&mut as_value, view, paths);
            let next = as_value.as_object().cloned().unwrap_or_default();
            if next != doc {
                columns.insert(key.into(), Value::Object(next).to_string().into());
                changed = true;
            }
        }
        if changed {
            if let Some(keyed) = columns
                .get("doc")
                .or_else(|| columns.get("base"))
                .and_then(Value::as_str)
                .and_then(|t| crate::store::parse(Some(t)))
            {
                columns.insert(
                    "section_id".into(),
                    crate::write::holder(&keyed).to_string().into(),
                );
            }
            conn.execute(
                "UPDATE commit_rows SET before = ? WHERE commit_id = ? AND tbl = 'card_docs' AND row_id = ?",
                params![Value::Object(columns).to_string(), commit, row],
            )
            .map_err(sql)?;
        }
    }
    Ok(())
}

// MARK: - A tree's record

/// A tree's record: one commit of lines, after the last.
fn record(
    conn: &Connection,
    project: &str,
    tree: &str,
    lines: &[crate::text::Keyed],
) -> Result<(), String> {
    if lines.is_empty() {
        return Ok(());
    }
    let seq: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(seq), 0) + 1 FROM commits WHERE project_id = ? AND branch_id = ?",
            params![project, tree],
            |r| r.get(0),
        )
        .map_err(sql)?;
    let id = crate::write::new_id();
    conn.execute(
        "INSERT INTO commits (id, project_id, branch_id, seq, at, author_kind, author, kind, name) VALUES (?, ?, ?, ?, strftime('%Y-%m-%dT%H:%M:%fZ','now'), 'screen', NULL, 'migrate', ?)",
        params![
            id,
            project,
            tree,
            seq,
            crate::write::history::migrated(keyed!("history.migrate.containers"))
        ],
    )
    .map_err(sql)?;
    for (i, line) in lines.iter().enumerate() {
        crate::write::insert_line(
            conn,
            &id,
            i as i64,
            "migrate",
            "project",
            Some(project),
            line,
        )
        .map_err(sql)?;
    }
    Ok(())
}
