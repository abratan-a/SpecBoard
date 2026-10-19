//! The card ghosts of before moved into cards of their own (the store's
//! mark `core_0003_card_ghosts`, a project file before version 11).
//!
//! A section's document kept its card ghosts as a list (`ghosts`: `{schema_id, sort_order}`)
//! of cards it showed whole: another section's cards, and its own values and enums. Now a
//! card ghost is a card of the kind `ghost` the section holds, with a name of its own and
//! what it points at (`specboard://SpecBoard/Docs/Design:DesignSections` §3), and a value or an enum shown on its own
//! section's diagram says so of itself (`standalone`, how it looks). Each tree is moved as
//! it sees its sections:
//!
//! - Another section's card: a ghost of it in the section, named as the card — or, when that
//!   name is taken there, as one that is not (said in the record) — and the box the card
//!   had on the section's diagram (`places`, the section as the view) is the ghost's own.
//! - Its own value or enum: `standalone`. Anything else of its own, or a card that is gone,
//!   is let go (it was drawn nowhere).
//! - A branch's own copy of a section: what it adds to main's list is the branch's own
//!   ghost; what it leaves out of main's, the branch's mark that it removed main's ghost.
//!   main's ghosts are given to each branch's prints, as if they were there when it was
//!   made; an open pull request still current is printed again.
//! - `ghosts` goes from every section's document, base and the history's rows.
//!
//! Each tree moved has a commit of one record (no rows: going back does not undo it).
//! Ghost ids are made from where they stand (`ids::made_id`), so a branch and main name
//! the same ghost alike.

use crate::directory::{Directory, SectionRef};
use crate::store::BOARD;
use rusqlite::{Connection, params};
use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};

fn sql(e: rusqlite::Error) -> String {
    text!("ghostMove.error.sql", e)
}

/// A row of `card_docs` or `section_docs` of one tree.
#[derive(Clone)]
struct Row {
    id: String,
    removed: bool,
    doc: Option<Map<String, Value>>,
    base: Option<String>,
}

fn rows(conn: &Connection, table: &str, project: &str, tree: &str) -> Result<Vec<Row>, String> {
    let mut stmt = conn
        .prepare(&format!(
            "SELECT id, removed, doc, base FROM {table} WHERE project_id = ? AND branch_id = ?"
        ))
        .map_err(sql)?;
    stmt.query_map(params![project, tree], |r| {
        Ok(Row {
            id: r.get(0)?,
            removed: r.get::<_, i64>(1)? != 0,
            doc: crate::store::parse(r.get::<_, Option<String>>(2)?.as_deref()),
            base: r.get(3)?,
        })
    })
    .map_err(sql)?
    .collect::<Result<_, _>>()
    .map_err(sql)
}

/// A section's card ghosts as its document listed them: the cards, in their order.
fn listed(doc: &Map<String, Value>) -> Vec<String> {
    let mut out: Vec<(i64, String)> = doc
        .get("ghosts")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|g| {
            let card = g.get("schema_id")?.as_str()?.to_string();
            let order = g.get("sort_order").and_then(Value::as_i64).unwrap_or(0);
            Some((order, card))
        })
        .collect();
    out.sort_by_key(|g| g.0);
    let mut seen = HashSet::new();
    out.into_iter()
        .map(|g| g.1)
        .filter(|c| seen.insert(c.clone()))
        .collect()
}

/// A document's text with `ghosts` gone; none when it had none.
fn stripped(text: &str) -> Option<String> {
    let mut doc = crate::store::parse(Some(text))?;
    doc.shift_remove("ghosts")?;
    Some(Value::Object(doc).to_string())
}

/// A card as a tree sees it: its section, name and kind.
#[derive(Clone)]
struct Card {
    section: String,
    name: String,
    kind: String,
}

fn card_of(doc: &Map<String, Value>) -> Card {
    let text = |k: &str| {
        doc.get(k)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    Card {
        section: text("section"),
        name: text("name"),
        kind: text("kind"),
    }
}

/// What a tree sees: its sections (id → document) and its cards (id → card), main's
/// beneath a branch's own rows.
struct View {
    sections: HashMap<String, Map<String, Value>>,
    cards: HashMap<String, Card>,
    orders: i64,
}

fn view(main: (&[Row], &[Row]), own: Option<(&[Row], &[Row])>) -> View {
    let mut sections: HashMap<String, Map<String, Value>> = HashMap::new();
    let mut cards: HashMap<String, Card> = HashMap::new();
    let mut orders = 0;
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
                orders = orders.max(doc.get("order").and_then(Value::as_i64).unwrap_or(0));
                cards.insert(row.id.clone(), card_of(doc));
            }
        }
    }
    View {
        sections,
        cards,
        orders,
    }
}

/// Each section's path, as a tree's sections name it (`Directory::full_section`).
fn paths(sections: &HashMap<String, Map<String, Value>>) -> Directory {
    let refs: Vec<SectionRef> = sections
        .iter()
        .map(|(id, doc)| SectionRef {
            id: id.clone(),
            name: doc
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            parent_id: doc
                .get("parent_id")
                .and_then(Value::as_str)
                .map(str::to_string),
        })
        .collect();
    Directory::new(&refs, &[], &[])
}

/// A name free in a section of the view: the card's own, else with `_ghost` (and a number).
fn free_name(view: &View, made: &[(String, String)], section: &str, name: &str) -> String {
    let taken = |n: &str| {
        view.cards
            .values()
            .any(|c| c.section == section && crate::pathtext::same(&c.name, n))
            || made
                .iter()
                .any(|(s, m)| s == section && crate::pathtext::same(m, n))
    };
    if !taken(name) {
        return name.to_string();
    }
    let mut n = 1;
    loop {
        let tried = if n == 1 {
            format!("{name}_ghost")
        } else {
            format!("{name}_ghost{n}")
        };
        if !taken(&tried) {
            return tried;
        }
        n += 1;
    }
}

/// What moving one tree made: the record's lines, and the ghost documents written (id →
/// document text) by section and card.
#[derive(Default)]
struct Moved {
    lines: Vec<crate::text::Keyed>,
    made: HashMap<(String, String), (String, String)>,
}

/// One tree's sections moved: `lists` are the sections whose card ghosts it decides (its
/// own rows), each with main's list beneath (for a branch).
#[allow(clippy::too_many_arguments)]
fn move_tree(
    conn: &Connection,
    project: &str,
    tree: &str,
    view: &mut View,
    lists: &[(String, Vec<String>, Vec<String>)],
    main_made: &HashMap<(String, String), (String, String)>,
    now: &str,
) -> Result<Moved, String> {
    let mut out = Moved::default();
    let directory = paths(&view.sections);
    let full = |section: &str, name: &str| format!("{}:{name}", directory.full_section(section));
    let mut made_names: Vec<(String, String)> = Vec::new();
    let mut count = 0;
    for (section, list, beneath) in lists {
        let is_root = view
            .sections
            .get(section)
            .is_some_and(|d| d.get("parent_id").and_then(Value::as_str).is_none());
        for card_id in list.iter().filter(|c| !beneath.contains(c)) {
            let Some(card) = view.cards.get(card_id).cloned() else {
                out.lines.push(keyed!(
                    "ghostMove.line.targetMissing",
                    section = directory.full_section(section)
                ));
                continue;
            };
            if &card.section == section {
                if card.kind == "value" || card.kind == "enum" {
                    look(conn, project, tree, card_id, Some(true))?;
                    out.lines.push(keyed!(
                        "ghostMove.line.standalone",
                        path = full(section, &card.name)
                    ));
                }
                continue;
            }
            if is_root {
                out.lines.push(keyed!(
                    "ghostMove.line.rootGhost",
                    path = full(&card.section, &card.name)
                ));
                continue;
            }
            let id = crate::ids::made_id(&["ghost", project, section, card_id]);
            let name = free_name(view, &made_names, section, &card.name);
            view.orders += 1;
            let mut doc = Map::new();
            doc.insert("id".into(), id.clone().into());
            doc.insert("kind".into(), crate::detail::GHOST.into());
            doc.insert("name".into(), name.clone().into());
            doc.insert("section".into(), section.clone().into());
            doc.insert("v".into(), crate::kinds::builtin().version.into());
            doc.insert("order".into(), view.orders.into());
            let mut fields = Map::new();
            fields.insert("target".into(), card_id.clone().into());
            doc.insert("fields".into(), Value::Object(fields));
            let text = Value::Object(doc).to_string();
            conn.execute(
                "INSERT INTO card_docs (project_id, branch_id, id, section_id, name, kind, removed, doc) VALUES (?, ?, ?, ?, ?, ?, 0, ?)",
                params![project, tree, id, section, name, crate::detail::GHOST, text],
            )
            .map_err(sql)?;
            // The box the card had on the section's diagram is the ghost's own.
            conn.execute(
                "UPDATE places SET view = '', item = ?1 WHERE project_id = ?2 AND branch_id = ?3 AND view = ?4 AND item_kind = 'card' AND item = ?5",
                params![id, project, tree, section, card_id],
            )
            .map_err(sql)?;
            made_names.push((section.clone(), name.clone()));
            view.cards.insert(
                id.clone(),
                Card {
                    section: section.clone(),
                    name: name.clone(),
                    kind: crate::detail::GHOST.to_string(),
                },
            );
            if name != card.name {
                out.lines.push(keyed!(
                    "ghostMove.line.renamed",
                    path = full(section, &card.name),
                    name,
                    target = full(&card.section, &card.name)
                ));
            }
            out.made
                .insert((section.clone(), card_id.clone()), (id, text));
            count += 1;
        }
        // In a branch: what it left out of main's list.
        for card_id in beneath.iter().filter(|c| !list.contains(c)) {
            if let Some((id, text)) = main_made.get(&(section.clone(), card_id.clone())) {
                let doc = crate::store::parse(Some(text)).unwrap_or_default();
                let name = doc.get("name").and_then(Value::as_str).unwrap_or_default();
                conn.execute(
                    "INSERT INTO card_docs (project_id, branch_id, id, section_id, name, kind, removed, doc, base, based_at) VALUES (?, ?, ?, ?, ?, ?, 1, NULL, ?, ?)
                     ON CONFLICT(project_id, branch_id, id) DO NOTHING",
                    params![project, tree, id, section, name, crate::detail::GHOST, text, now],
                )
                .map_err(sql)?;
                view.cards.remove(id);
            } else if view
                .cards
                .get(card_id)
                .is_some_and(|c| &c.section == section && (c.kind == "value" || c.kind == "enum"))
            {
                look(conn, project, tree, card_id, Some(false))?;
            }
        }
        // In a branch: a box it gave a card main's ghost stands for is the ghost's.
        for card_id in list.iter().filter(|c| beneath.contains(c)) {
            if let Some((id, _)) = main_made.get(&(section.clone(), card_id.clone())) {
                conn.execute(
                    "UPDATE OR IGNORE places SET view = '', item = ?1 WHERE project_id = ?2 AND branch_id = ?3 AND view = ?4 AND item_kind = 'card' AND item = ?5",
                    params![id, project, tree, section, card_id],
                )
                .map_err(sql)?;
            }
        }
    }
    if count > 0 {
        out.lines.insert(0, keyed!("ghostMove.line.moved", count));
    }
    Ok(out)
}

/// A value's or an enum's `standalone`, as how it looks.
fn look(
    conn: &Connection,
    project: &str,
    tree: &str,
    card: &str,
    on: Option<bool>,
) -> Result<(), String> {
    match on {
        Some(on) => conn.execute(
            "INSERT INTO card_looks (project_id, branch_id, card_id, row_path, field, value) VALUES (?, ?, ?, '', 'standalone', ?)
             ON CONFLICT(project_id, branch_id, card_id, row_path, field) DO UPDATE SET value = excluded.value",
            params![project, tree, card, Value::Bool(on).to_string()],
        ),
        None => conn.execute(
            "DELETE FROM card_looks WHERE project_id = ? AND branch_id = ? AND card_id = ? AND row_path = '' AND field = 'standalone'",
            params![project, tree, card],
        ),
    }
    .map_err(sql)?;
    Ok(())
}

/// `ghosts` gone from a tree's section documents and their bases.
fn strip_sections(
    conn: &Connection,
    project: &str,
    tree: &str,
    list: &[Row],
) -> Result<(), String> {
    for row in list {
        let doc = row
            .doc
            .as_ref()
            .and_then(|d| stripped(&Value::Object(d.clone()).to_string()));
        let base = row.base.as_deref().and_then(stripped);
        if let Some(doc) = doc {
            conn.execute(
                "UPDATE section_docs SET doc = ? WHERE project_id = ? AND branch_id = ? AND id = ?",
                params![doc, project, tree, row.id],
            )
            .map_err(sql)?;
        }
        if let Some(base) = base {
            conn.execute(
                "UPDATE section_docs SET base = ? WHERE project_id = ? AND branch_id = ? AND id = ?",
                params![base, project, tree, row.id],
            )
            .map_err(sql)?;
        }
    }
    Ok(())
}

/// The history's rows of section documents with `ghosts` gone, so what a commit kept
/// compares with what is written now.
fn strip_history(conn: &Connection, project: &str) -> Result<(), String> {
    let kept: Vec<(String, String, String)> = {
        let mut stmt = conn
            .prepare(
                "SELECT r.commit_id, r.row_id, r.before FROM commit_rows r JOIN commits c ON c.id = r.commit_id WHERE c.project_id = ? AND r.tbl = 'section_docs' AND r.before IS NOT NULL",
            )
            .map_err(sql)?;
        stmt.query_map([project], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .map_err(sql)?
            .collect::<Result<_, _>>()
            .map_err(sql)?
    };
    for (commit, row, before) in kept {
        let Some(mut columns) = crate::store::parse(Some(&before)) else {
            continue;
        };
        let mut changed = false;
        for key in ["doc", "base"] {
            if let Some(text) = columns.get(key).and_then(Value::as_str)
                && let Some(next) = stripped(text)
            {
                columns.insert(key.into(), next.into());
                changed = true;
            }
        }
        if changed {
            conn.execute(
                "UPDATE commit_rows SET before = ? WHERE commit_id = ? AND tbl = 'section_docs' AND row_id = ?",
                params![Value::Object(columns).to_string(), commit, row],
            )
            .map_err(sql)?;
        }
    }
    Ok(())
}

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
        "INSERT INTO commits (id, project_id, branch_id, seq, at, author_kind, author, kind, name) VALUES (?, ?, ?, ?, strftime('%Y-%m-%dT%H:%M:%fZ','now'), 'screen', NULL, 'migrate', 'カードゴーストを移した')",
        params![id, project, tree, seq],
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

/// One project's card ghosts moved, in every tree.
pub fn move_project(conn: &Connection, project: &str) -> Result<(), String> {
    // The open requests still current, to print again once the documents are moved.
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
    let now: String = conn
        .query_row("SELECT strftime('%Y-%m-%dT%H:%M:%fZ','now')", [], |r| {
            r.get(0)
        })
        .map_err(sql)?;
    let branches: Vec<String> = {
        let mut stmt = conn
            .prepare("SELECT id FROM branches WHERE project_id = ?")
            .map_err(sql)?;
        stmt.query_map([project], |r| r.get(0))
            .map_err(sql)?
            .collect::<Result<_, _>>()
            .map_err(sql)?
    };

    // main and the board: each stands on its own rows.
    let mut main_lists: HashMap<String, Vec<String>> = HashMap::new();
    let mut main_made = HashMap::new();
    for tree in ["", BOARD] {
        let sections = rows(conn, "section_docs", project, tree)?;
        let cards = rows(conn, "card_docs", project, tree)?;
        let lists: Vec<(String, Vec<String>, Vec<String>)> = sections
            .iter()
            .filter(|r| !r.removed)
            .filter_map(|r| {
                let list = listed(r.doc.as_ref()?);
                (!list.is_empty()).then(|| (r.id.clone(), list, Vec::new()))
            })
            .collect();
        if tree.is_empty() {
            main_lists = lists
                .iter()
                .map(|(s, l, _)| (s.clone(), l.clone()))
                .collect();
        }
        let mut seen = view((&sections, &cards), None);
        let moved = move_tree(
            conn,
            project,
            tree,
            &mut seen,
            &lists,
            &HashMap::new(),
            &now,
        )?;
        if tree.is_empty() {
            main_made = moved.made.clone();
        }
        strip_sections(conn, project, tree, &sections)?;
        record(conn, project, tree, &moved.lines)?;
    }

    // Each branch: its own copies of sections decide; main's rows beneath the rest.
    let main_sections = rows(conn, "section_docs", project, "")?;
    let main_cards = rows(conn, "card_docs", project, "")?;
    for branch in &branches {
        let sections = rows(conn, "section_docs", project, branch)?;
        let cards = rows(conn, "card_docs", project, branch)?;
        let lists: Vec<(String, Vec<String>, Vec<String>)> = sections
            .iter()
            .filter(|r| !r.removed)
            .filter_map(|r| {
                let list = listed(r.doc.as_ref()?);
                let beneath = main_lists.get(&r.id).cloned().unwrap_or_default();
                (!list.is_empty() || !beneath.is_empty()).then(|| (r.id.clone(), list, beneath))
            })
            .collect();
        let mut seen = view((&main_sections, &main_cards), Some((&sections, &cards)));
        let moved = move_tree(conn, project, branch, &mut seen, &lists, &main_made, &now)?;
        strip_sections(conn, project, branch, &sections)?;
        record(conn, project, branch, &moved.lines)?;
        // main's ghosts, as if they were there when the branch was made.
        let directory = paths(&seen.sections);
        for ((section, _), (id, text)) in &main_made {
            let doc = crate::store::parse(Some(text)).unwrap_or_default();
            let name = doc.get("name").and_then(Value::as_str).unwrap_or_default();
            conn.execute(
                "INSERT OR IGNORE INTO branch_prints (branch_id, card_id, print, path) VALUES (?, ?, ?, ?)",
                params![
                    branch,
                    id,
                    crate::schema::print(text),
                    format!("{}:{name}", directory.full_section(section))
                ],
            )
            .map_err(sql)?;
        }
    }
    strip_history(conn, project)?;
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
