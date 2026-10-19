//! A project as one JSON file, in the form of version 28: the project's row, main's sections in tree order
//! and its cards by section, order and name, the branches as the store keeps them, the
//! board, the diagram's settings — with the cards in their kinds' form, and last every box
//! on a diagram (`places`), how cards look (`looks`), the check's marks (`inspectionMarks`) and the
//! code the implementations' lines were recorded at (`codeMarks`).
//! A file's change notes (`changelog`, a
//! table nothing wrote, gone: `schema::NO_CHANGELOG`) are let go when it is read; no
//! version is given for it. Version 28 has the code the lines to working directories' files were
//! recorded at (`codeMarks`: by the line's id, of the lines some tree holds); a file of before has
//! none, and its lines read as not recorded. Version 27 has, in an enum's values, what each stands for
//! (`description`); a file of before has none. An app of before refuses a file of 27 as newer
//! rather than as cards that do not fit its kinds. Version 26 has, in a working directory's settings (`export_dirs`), how its
//! files map to languages besides the table's (`parsers`); a file of before has none, and its files
//! are read as the table says. Version 24 has, in each goals section directly under the board's root, a
//! section of the kind `complete` and one of `cancelled` (`board_move`, once a file of before is in the
//! store). Version 23 has the built-in kind of section `definition` where a file
//! of before has `instances`; a project's own kind of that name, which a file of before may have,
//! is let go of or named anew (`definition_move`, once the file is in the store). Version 22 has
//! names of sections, cards, fields, rows, versions and nodes of any characters but control ones
//! (a path quotes what needs it,
//! `specboard://SpecBoard/Docs/Design:DesignSections`); a file of before holds only names a path
//! writes bare, read as they are. Version 21 has the check's marks (`inspectionMarks`: the items
//! said to be as designed); a file of before has none. Version 20 has the project's exports hold no working directory
//! (`dir`): where one is written to is given when it is written, and each Mac keeps one default
//! folder for it, which no file carries (`export_move::move_file` lets go of `dir`, for a file of
//! before). Version 19 has the project's exports in its row (`exports`), and
//! no card nor section its own export settings (`export_move::move_file` moves them, for a file of
//! before). Version 16 has what is directly under each root in the
//! project's kinds (`project_tree`). Version 15 has a goal's merge request as its `MR`, of the
//! kind of field `mr` (a request to merge called a merge request); 14 has a goal's fields by their names in
//! English (`status`・`priority`・`due`); 13 a flow's nodes and a goal's cards inside them
//! (`in`). Version 12 has a card's lines as its `connections`
//! and the project's own kinds of card, section and line in its row (`kinds`); 11 had a card
//! ghost as a card of the kind `ghost` (a section's document no longer lists `ghosts`), and
//! the form named for SpecBoard (`specboard/project`), the oldest read.

use crate::ojson;
use crate::rows::{self, Table};
use crate::store::{BOARD, parse, sql};
use rusqlite::Connection;
use rusqlite::params;
use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};

pub const FORMAT: &str = "specboard/project";
pub const VERSION: i64 = 28;

/// The form's name before a plan was called a project (versions 11 to 17), read still.
pub const FORMAT_BEFORE: &str = "specboard/plan";

/// Whether a file's `format` is the project file's, by either name.
pub fn is_project_format(format: Option<&str>) -> bool {
    matches!(format, Some(FORMAT) | Some(FORMAT_BEFORE))
}

/// A file of the name before (`FORMAT_BEFORE`), named as one of now: its `plan` the `project`, the
/// kinds' `plan_tree` the `project_tree`, the items of what MCP may write and the places of the
/// writing rules `project.…`. Its version stays, which says how it is read otherwise.
pub fn named_now(file: &mut Value) {
    let Some(top) = file.as_object_mut() else {
        return;
    };
    if top.get("format").and_then(Value::as_str) != Some(FORMAT_BEFORE) {
        return;
    }
    top.insert("format".into(), FORMAT.into());
    let Some(Value::Object(mut project)) = top.shift_remove("plan") else {
        return;
    };
    if let Some(Value::Object(kinds)) = project.get_mut("kinds")
        && let Some(tree) = kinds.shift_remove("plan_tree")
    {
        kinds.insert("project_tree".into(), tree);
    }
    // The items of what MCP may write, and the places of the writing rules (`plan.description`).
    for column in ["mcp_writes", "rules"] {
        if let Some(Value::Object(held)) = project.get_mut(column) {
            let items: Vec<(String, Value)> = std::mem::take(held).into_iter().collect();
            for (key, value) in items {
                let key = match key.strip_prefix("plan.") {
                    Some(rest) => format!("project.{rest}"),
                    None => key,
                };
                held.insert(key, value);
            }
        }
    }
    // In its place, first after the format and the version.
    let rest: Vec<(String, Value)> = std::mem::take(top).into_iter().collect();
    for (key, value) in rest {
        top.insert(key.clone(), value);
        if key == "version" {
            top.insert("project".into(), Value::Object(project.clone()));
        }
    }
    if !top.contains_key("project") {
        top.insert("project".into(), Value::Object(project));
    }
}

const PROJECTS: Table = Table {
    owner: None,
    skip: &["updated_at", "archived_at"],
    json: &[
        "tag_groups",
        "rules",
        "export_dirs",
        "mcp_writes",
        "kinds",
        "exports",
    ],
};
const DEFAULT_VIEW: Table = Table {
    owner: Some("project_id"),
    skip: &[],
    json: &[],
};
const BRANCHES: Table = Table {
    owner: Some("project_id"),
    skip: &[],
    json: &[],
};

struct Kept {
    id: String,
    removed: bool,
    doc: Option<String>,
    base: Option<String>,
    based_at: Option<String>,
}

type Doc = Map<String, Value>;

fn number(doc: &Doc, key: &str) -> i64 {
    doc.get(key)
        .and_then(Value::as_f64)
        .map(|n| n as i64)
        .unwrap_or(0)
}

fn string(doc: &Doc, key: &str) -> String {
    doc.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// Sections' ranks in tree order, from their documents: by order, when made, name.
fn tree_rank(docs: &[Doc]) -> HashMap<String, usize> {
    let mut flat: Vec<(String, Option<String>, i64, String, String)> = docs
        .iter()
        .map(|d| {
            (
                string(d, "id"),
                d.get("parent_id")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                number(d, "sort_order"),
                string(d, "created_at"),
                string(d, "name"),
            )
        })
        .collect();
    flat.sort_by(|a, b| (a.2, &a.3, &a.4).cmp(&(b.2, &b.3, &b.4)));
    // Parents before children, siblings in that order.
    let known: HashSet<&str> = flat.iter().map(|s| s.0.as_str()).collect();
    let mut children: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, s) in flat.iter().enumerate() {
        let parent =
            s.1.clone()
                .filter(|p| known.contains(p.as_str()))
                .unwrap_or_default();
        children.entry(parent).or_default().push(i);
    }
    let mut order = Vec::new();
    let mut seen = HashSet::new();
    fn visit(
        parent: &str,
        flat: &[(String, Option<String>, i64, String, String)],
        children: &HashMap<String, Vec<usize>>,
        seen: &mut HashSet<usize>,
        order: &mut Vec<usize>,
    ) {
        for &i in children.get(parent).map(Vec::as_slice).unwrap_or(&[]) {
            if seen.insert(i) {
                order.push(i);
                visit(&flat[i].0, flat, children, seen, order);
            }
        }
    }
    visit("", &flat, &children, &mut seen, &mut order);
    for i in 0..flat.len() {
        if !seen.contains(&i) {
            order.push(i);
        }
    }
    let mut rank = HashMap::new();
    for (position, i) in order.into_iter().enumerate() {
        rank.entry(flat[i].0.clone()).or_insert(position);
    }
    rank
}

fn kept(
    conn: &Connection,
    table: &str,
    project_id: &str,
    branch: &str,
) -> Result<Vec<Kept>, String> {
    let mut stmt = conn
            .prepare(&format!(
                "SELECT id, removed, doc, base, based_at FROM {table} WHERE project_id = ? AND branch_id = ?"
            ))
            .map_err(sql)?;
    let out = stmt
        .query_map(params![project_id, branch], |r| {
            Ok(Kept {
                id: r.get(0)?,
                removed: r.get::<_, i64>(1)? != 0,
                doc: r.get(2)?,
                base: r.get(3)?,
                based_at: r.get(4)?,
            })
        })
        .map_err(sql)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sql)?;
    Ok(out)
}

/// The project as a file: its main, its branches as the store keeps them, its board.
pub fn write(conn: &Connection, project_id: &str) -> Result<String, String> {
    let project_columns = rows::columns(conn, "projects")?;
    let project = rows::rows(conn, "projects", "id = ?", &[&project_id])?
        .into_iter()
        .next()
        .ok_or_else(|| text!("projectFile.error.projectNotFound"))?;

    // main's sections as the tree goes, its cards by section, order and name.
    let main_sections: Vec<Doc> = kept(conn, "section_docs", project_id, "")?
        .iter()
        .filter_map(|k| parse(k.doc.as_deref()))
        .collect();
    let rank = tree_rank(&main_sections);
    let section_key = |d: &Doc| {
        (
            rank.get(&string(d, "id")).copied().unwrap_or(usize::MAX),
            string(d, "id"),
        )
    };
    let card_key = |d: &Doc| {
        (
            rank.get(&string(d, "section"))
                .copied()
                .unwrap_or(usize::MAX),
            number(d, "order"),
            string(d, "name"),
            string(d, "id"),
        )
    };
    let mut sections = main_sections.clone();
    sections.sort_by_key(section_key);
    let mut cards: Vec<Doc> = kept(conn, "card_docs", project_id, "")?
        .iter()
        .filter_map(|k| parse(k.doc.as_deref()))
        .collect();
    cards.sort_by_key(card_key);

    let mut body = Map::new();
    body.insert("format".into(), FORMAT.into());
    body.insert("version".into(), VERSION.into());
    body.insert(
        "project".into(),
        Value::Object(rows::object(&project, &PROJECTS, &project_columns)),
    );
    body.insert(
        "sections".into(),
        Value::Array(sections.into_iter().map(Value::Object).collect()),
    );
    body.insert(
        "cards".into(),
        Value::Array(cards.into_iter().map(Value::Object).collect()),
    );

    // The branches, as the store keeps them: in their order, each one's rows by id.
    let branch_columns = rows::columns(conn, "branches")?;
    let mut branches = Vec::new();
    for row in rows::by_order(
        // Not the app's own, made to show a merge's result for a while (`~result-…`).
        rows::rows(
            conn,
            "branches",
            "project_id = ? AND name NOT LIKE '~%'",
            &[&project_id],
        )?,
        &["sort_order", "name", "id"],
    ) {
        let id = rows::text(&row, "id");
        let mut out = rows::object(&row, &BRANCHES, &branch_columns);
        let listed = |mut list: Vec<Kept>| -> Vec<Value> {
            list.sort_by(|a, b| a.id.cmp(&b.id));
            list.into_iter()
                .map(|k| {
                    let mut entry = Map::new();
                    entry.insert("id".into(), k.id.into());
                    if k.removed {
                        entry.insert("removed".into(), true.into());
                    }
                    if let Some(doc) = k.doc {
                        entry.insert(
                            "doc".into(),
                            serde_json::from_str(&doc).unwrap_or(Value::Null),
                        );
                    }
                    if let Some(base) = k.base {
                        entry.insert(
                            "base".into(),
                            serde_json::from_str(&base).unwrap_or(Value::Null),
                        );
                    }
                    if let Some(at) = k.based_at {
                        entry.insert("based_at".into(), at.into());
                    }
                    Value::Object(entry)
                })
                .collect()
        };
        let own_sections = kept(conn, "section_docs", project_id, &id)?;
        let own_cards = kept(conn, "card_docs", project_id, &id)?;
        if !own_sections.is_empty() {
            out.insert("sections".into(), Value::Array(listed(own_sections)));
        }
        if !own_cards.is_empty() {
            out.insert("cards".into(), Value::Array(listed(own_cards)));
        }
        let mut stmt = conn
            .prepare("SELECT card_id, print, path FROM branch_prints WHERE branch_id = ? ORDER BY card_id")
            .map_err(sql)?;
        let prints: Vec<Value> = stmt
            .query_map([&id], |r| {
                let mut print = Map::new();
                print.insert("card".into(), r.get::<_, String>(0)?.into());
                print.insert("print".into(), r.get::<_, String>(1)?.into());
                let path: String = r.get(2)?;
                if !path.is_empty() {
                    print.insert("path".into(), path.into());
                }
                Ok(Value::Object(print))
            })
            .map_err(sql)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(sql)?;
        if !prints.is_empty() {
            out.insert("prints".into(), Value::Array(prints));
        }
        branches.push(Value::Object(out));
    }
    if !branches.is_empty() {
        body.insert("branches".into(), Value::Array(branches));
    }

    // The board: its sections as its tree goes, its cards by section, order and name.
    let board_sections: Vec<Doc> = kept(conn, "section_docs", project_id, BOARD)?
        .into_iter()
        .filter(|k| !k.removed)
        .filter_map(|k| parse(k.doc.as_deref()))
        .collect();
    let board_rank = tree_rank(&board_sections);
    let mut board_cards: Vec<Doc> = kept(conn, "card_docs", project_id, BOARD)?
        .into_iter()
        .filter(|k| !k.removed)
        .filter_map(|k| parse(k.doc.as_deref()))
        .collect();
    if !board_sections.is_empty() || !board_cards.is_empty() {
        let mut sections = board_sections.clone();
        sections.sort_by_key(|d| {
            (
                board_rank
                    .get(&string(d, "id"))
                    .copied()
                    .unwrap_or(usize::MAX),
                string(d, "id"),
            )
        });
        board_cards.sort_by_key(|d| {
            (
                board_rank
                    .get(&string(d, "section"))
                    .copied()
                    .unwrap_or(usize::MAX),
                number(d, "order"),
                string(d, "name"),
                string(d, "id"),
            )
        });
        let mut board = Map::new();
        board.insert(
            "sections".into(),
            Value::Array(sections.into_iter().map(Value::Object).collect()),
        );
        board.insert(
            "cards".into(),
            Value::Array(board_cards.into_iter().map(Value::Object).collect()),
        );
        body.insert("board".into(), Value::Object(board));
    }

    if let Some(view) = rows::rows(conn, "default_view", "project_id = ?", &[&project_id])?
        .into_iter()
        .next()
    {
        let columns = rows::columns(conn, "default_view")?;
        body.insert(
            "default_view".into(),
            Value::Object(rows::object(&view, &DEFAULT_VIEW, &columns)),
        );
    }

    // Where every box sits and how cards look, last: main's, then the branches' in id
    // order; each tree's views as main's tree goes, the project's own first.
    let view_rank = |view: &str| -> i64 {
        match rank.get(view) {
            Some(&r) => r as i64,
            None if view.is_empty() => -1,
            None => i64::MAX,
        }
    };
    let mut stmt = conn
        .prepare("SELECT branch_id, view, item_kind, item, x, y, w, h, pinned FROM places WHERE project_id = ?")
        .map_err(sql)?;
    type Place = (String, String, String, String, [Option<i64>; 4], bool);
    let mut places: Vec<Place> = stmt
        .query_map([project_id], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                [r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?],
                r.get::<_, i64>(8)? != 0,
            ))
        })
        .map_err(sql)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sql)?;
    places.sort_by(|a, b| {
        (&a.0, view_rank(&a.1), &a.1, &a.2, &a.3).cmp(&(&b.0, view_rank(&b.1), &b.1, &b.2, &b.3))
    });
    let places: Vec<Value> = places
        .into_iter()
        .map(|(branch, view, item_kind, item, at, pinned)| {
            let mut out = Map::new();
            if !branch.is_empty() {
                out.insert("branch_id".into(), branch.into());
            }
            if !view.is_empty() {
                out.insert("view".into(), view.into());
            }
            out.insert("item_kind".into(), item_kind.into());
            out.insert("item".into(), item.into());
            for (key, value) in ["x", "y", "w", "h"].into_iter().zip(at) {
                if let Some(value) = value {
                    out.insert(key.into(), value.into());
                }
            }
            if pinned {
                out.insert("pinned".into(), 1.into());
            }
            Value::Object(out)
        })
        .collect();
    body.insert("places".into(), Value::Array(places));
    let mut stmt = conn
        .prepare("SELECT branch_id, card_id, row_path, field, value FROM card_looks WHERE project_id = ? ORDER BY branch_id, card_id, row_path, field")
        .map_err(sql)?;
    let looks: Vec<Value> = stmt
        .query_map([project_id], |r| {
            let (branch, row): (String, String) = (r.get(0)?, r.get(2)?);
            let mut out = Map::new();
            if !branch.is_empty() {
                out.insert("branch_id".into(), branch.into());
            }
            out.insert("card_id".into(), r.get::<_, String>(1)?.into());
            if !row.is_empty() {
                out.insert("row_path".into(), row.into());
            }
            out.insert("field".into(), r.get::<_, String>(3)?.into());
            let value: String = r.get(4)?;
            out.insert(
                "value".into(),
                serde_json::from_str(&value).unwrap_or(Value::String(value)),
            );
            Ok(Value::Object(out))
        })
        .map_err(sql)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sql)?;
    if !looks.is_empty() {
        body.insert("looks".into(), Value::Array(looks));
    }
    let mut stmt = conn
        .prepare("SELECT id, grp, print, reason, by, at FROM inspection_marks WHERE project_id = ? ORDER BY id")
        .map_err(sql)?;
    let marks: Vec<Value> = stmt
        .query_map([project_id], |r| {
            let mut out = Map::new();
            for (at, key) in ["id", "group", "print", "reason", "by", "at"]
                .into_iter()
                .enumerate()
            {
                out.insert(key.into(), r.get::<_, String>(at)?.into());
            }
            Ok(Value::Object(out))
        })
        .map_err(sql)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sql)?;
    if !marks.is_empty() {
        body.insert("inspectionMarks".into(), Value::Array(marks));
    }
    // What was recorded of the lines some tree holds: a line gone from every tree is let go.
    let mut stmt = conn
        .prepare(
            "SELECT m.line_id, m.file_id, m.symbol, m.digest, m.by, m.at FROM code_marks m \
             WHERE m.project_id = ?1 AND m.line_id IN (SELECT json_extract(l.value, '$.id') \
             FROM card_docs d, json_each(d.doc, '$.connections') l \
             WHERE d.project_id = ?1 AND d.doc IS NOT NULL) ORDER BY m.line_id",
        )
        .map_err(sql)?;
    let code: Vec<Value> = stmt
        .query_map([project_id], |r| {
            let mut out = Map::new();
            out.insert("line".into(), r.get::<_, String>(0)?.into());
            out.insert("file".into(), r.get::<_, String>(1)?.into());
            let symbol: String = r.get(2)?;
            if !symbol.is_empty() {
                out.insert("symbol".into(), symbol.into());
            }
            for (at, key) in [(3, "digest"), (4, "by"), (5, "at")] {
                out.insert(key.into(), r.get::<_, String>(at)?.into());
            }
            Ok(Value::Object(out))
        })
        .map_err(sql)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sql)?;
    if !code.is_empty() {
        body.insert("codeMarks".into(), Value::Array(code));
    }
    Ok(ojson::pretty(&Value::Object(body)) + "\n")
}
