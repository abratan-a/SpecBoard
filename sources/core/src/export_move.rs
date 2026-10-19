//! The cards' and sections' export settings moved into the project's exports, once (the store's
//! mark `core_0015_exports`, a project file before version 19).
//!
//! Until then each card and section held its own (`exportTo`; a section's `export_to`): a section's
//! were the default of the cards under it that took one — not a flow, a note only in a schema
//! section — down to a section with its own; a card's own stood in for its section's default (`none`:
//! not written at all; `default`: its section's too). Each working directory written to becomes an
//! export of its name, main's settings its entries: a section's from the section, a card's from the
//! card. What the defaults no longer reach is left out (`exclude`): a section with settings of its own
//! from the exports it does not write to, a card with its own from those it does not, a card that said
//! `none` from every one its section's default wrote to. Every tree's documents let go of the settings
//! — main's, the board's, each branch's, a branch's prints of main and the history's rows: a branch's
//! settings were never written out.
//!
//! The exports held the working directory they wrote into (`dir`) before (the store's mark
//! `core_0016_export_targets`, a project file before version 20): now they hold none, and the
//! folder that directory was bound to on this Mac is each export's default one (`carry_all`,
//! `carry_exports`).

use crate::export::settings::{Entry, Export};
use crate::model::ExportSetting;
use rusqlite::{Connection, params};
use serde_json::{Map, Value};
use std::collections::HashMap;

fn sql(e: rusqlite::Error) -> String {
    text!("exportMove.error.sql", e)
}

/// One setting as a document kept it.
fn settings_of(doc: &Map<String, Value>, key: &str) -> Vec<ExportSetting> {
    doc.get(key)
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(|s| {
                    let s = s.as_object()?;
                    let text = |k: &str| s.get(k).and_then(Value::as_str).map(str::to_string);
                    Some(ExportSetting {
                        kind: text("kind").unwrap_or_else(|| "file".into()),
                        dir: text("dir"),
                        path: text("path").unwrap_or_default(),
                        format: text("format"),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn parse(text: &str) -> Option<Map<String, Value>> {
    match serde_json::from_str::<Value>(text).ok()? {
        Value::Object(o) => Some(o),
        _ => None,
    }
}

/// A section of main, as the move reads it.
struct Section {
    id: String,
    parent: Option<String>,
    kind: Option<String>,
    settings: Vec<ExportSetting>,
}

/// A card of main, as the move reads it.
struct Card {
    id: String,
    section: String,
    kind: String,
    settings: Vec<ExportSetting>,
}

/// Whether a card took its section's default: a flow never, a note only in a schema section — and
/// where the setting may write it.
fn took(card: &Card, setting: &ExportSetting, sections: &HashMap<String, &Section>) -> bool {
    // Its own section's kind, as the writer reads it (`Project::section_kind_of`).
    let schema = sections.get(&card.section).and_then(|s| s.kind.as_deref()) == Some("schema");
    setting.kind == "file"
        && setting.problem_for(&card.kind).is_none()
        && match card.kind.as_str() {
            "action" | "flow" => false,
            "note" => schema,
            _ => true,
        }
}

/// An export as the move makes it, with the working directory its settings wrote into (an export
/// holds none now: only the move still knows it).
pub(crate) struct Moved {
    pub export: Export,
    pub dir: String,
}

/// The exports a project's main says in the store, by its working directories (id → name) in order.
pub(crate) fn exports_of(
    conn: &Connection,
    project: &str,
    dirs: &[(String, String)],
) -> Result<Vec<Moved>, String> {
    let mut sections: Vec<Section> = Vec::new();
    {
        let mut stmt = conn
            .prepare(
                "SELECT id, parent_id, doc FROM section_docs WHERE project_id = ? AND branch_id = '' AND removed = 0 ORDER BY rowid",
            )
            .map_err(sql)?;
        let rows = stmt
            .query_map([project], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, Option<String>>(1)?,
                    r.get::<_, Option<String>>(2)?,
                ))
            })
            .map_err(sql)?;
        for row in rows {
            let (id, parent, doc) = row.map_err(sql)?;
            let doc = doc.as_deref().and_then(parse).unwrap_or_default();
            sections.push(Section {
                id,
                parent,
                kind: doc.get("kind").and_then(Value::as_str).map(str::to_string),
                settings: settings_of(&doc, "export_to"),
            });
        }
    }
    let mut cards: Vec<Card> = Vec::new();
    {
        let mut stmt = conn
            .prepare(
                "SELECT id, section_id, kind, doc FROM card_docs WHERE project_id = ? AND branch_id = '' AND removed = 0 ORDER BY rowid",
            )
            .map_err(sql)?;
        let rows = stmt
            .query_map([project], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, Option<String>>(3)?,
                ))
            })
            .map_err(sql)?;
        for row in rows {
            let (id, section, kind, doc) = row.map_err(sql)?;
            let doc = doc.as_deref().and_then(parse).unwrap_or_default();
            // A card inside a container is written with it, never on its own.
            if doc.get("in").is_some_and(|i| !i.is_null()) {
                continue;
            }
            let settings = settings_of(&doc, "exportTo");
            cards.push(Card {
                id,
                section,
                kind,
                settings,
            });
        }
    }
    Ok(exports_from(&sections, &cards, dirs))
}

/// The exports main's sections and cards say, by the working directories (id → name) in order.
fn exports_from(sections: &[Section], cards: &[Card], dirs: &[(String, String)]) -> Vec<Moved> {
    let by_id: HashMap<String, &Section> = sections.iter().map(|s| (s.id.clone(), s)).collect();
    // The nearest section from `at` up (itself first) with settings of its own.
    let owner = |at: Option<&str>| -> Option<&Section> {
        let mut at = at.map(str::to_string);
        let mut seen = 0;
        while let Some(id) = at {
            let section = by_id.get(&id)?;
            if !section.settings.is_empty() {
                return Some(section);
            }
            at = section.parent.clone();
            seen += 1;
            if seen > 64 {
                return None;
            }
        }
        None
    };

    // An export for each working directory written to, in the order of the directories; one no
    // longer there by its id.
    let mut order: Vec<String> = Vec::new();
    let mut exports: HashMap<String, Export> = HashMap::new();
    let export_for = |dir: &str, exports: &mut HashMap<String, Export>, order: &mut Vec<String>| {
        if !exports.contains_key(dir) {
            let name = dirs
                .iter()
                .find(|(id, _)| id == dir)
                .map(|(_, n)| n.clone())
                .unwrap_or_else(|| format!("dir-{}", dir.chars().take(8).collect::<String>()));
            exports.insert(
                dir.to_string(),
                Export {
                    id: crate::write::new_id(),
                    name,
                    entries: Vec::new(),
                    exclude: Vec::new(),
                },
            );
            order.push(dir.to_string());
        }
    };
    let entry = |from: &str, s: &ExportSetting| Entry {
        from: from.to_string(),
        row: None,
        to: s.path.clone(),
        format: s.format.clone(),
        kinds: Vec::new(),
    };
    let mut adds: Vec<(String, Entry)> = Vec::new();
    let mut leaves: Vec<(String, String)> = Vec::new();
    for section in sections {
        for s in section.settings.iter().filter(|s| s.kind == "file") {
            if let Some(dir) = &s.dir {
                adds.push((dir.clone(), entry(&section.id, s)));
            }
        }
        // Its own stood in for the defaults above it: where those wrote and it does not, left out.
        if section.settings.is_empty() {
            continue;
        }
        if let Some(above) = owner(section.parent.as_deref()) {
            for s in above.settings.iter().filter(|s| s.kind == "file") {
                let Some(dir) = &s.dir else { continue };
                if !section.settings.iter().any(|o| o.dir.as_ref() == Some(dir)) {
                    leaves.push((dir.clone(), section.id.clone()));
                }
            }
        }
    }
    for card in cards {
        let defaults: Vec<&ExportSetting> = owner(Some(&card.section))
            .map(|o| {
                o.settings
                    .iter()
                    .filter(|s| took(card, s, &by_id))
                    .collect()
            })
            .unwrap_or_default();
        if card.settings.is_empty() {
            continue;
        }
        let own: Vec<&ExportSetting> = card.settings.iter().filter(|s| s.kind == "file").collect();
        let none = card.settings.iter().any(|s| s.kind == "none");
        let and_default = card.settings.iter().any(|s| s.kind == "default");
        if none {
            for s in &defaults {
                if let Some(dir) = &s.dir {
                    leaves.push((dir.clone(), card.id.clone()));
                }
            }
            continue;
        }
        for s in &own {
            if let Some(dir) = &s.dir {
                adds.push((dir.clone(), entry(&card.id, s)));
            }
        }
        for s in &defaults {
            let Some(dir) = &s.dir else { continue };
            let writes_there = own.iter().any(|o| o.dir.as_ref() == Some(dir));
            match (and_default, writes_there) {
                // Its own and its section's, in a directory it writes to itself: both its own.
                (true, true) => {
                    if !own.iter().any(|o| o.dir == s.dir && o.path == s.path) {
                        adds.push((dir.clone(), entry(&card.id, s)));
                    }
                }
                // Its section's alone there: as it is.
                (true, false) => {}
                // Its own stood in for its section's: where it does not write, left out.
                (false, false) => leaves.push((dir.clone(), card.id.clone())),
                (false, true) => {}
            }
        }
    }
    for (dir, entry) in adds {
        export_for(&dir, &mut exports, &mut order);
        let export = exports.get_mut(&dir).expect("made");
        if !export.entries.contains(&entry) {
            export.entries.push(entry);
        }
    }
    for (dir, id) in leaves {
        // Left out of an export that writes nothing: none to make.
        if let Some(export) = exports.get_mut(&dir)
            && !export.exclude.contains(&id)
        {
            export.exclude.push(id);
        }
    }
    let mut out: Vec<Moved> = Vec::new();
    for (id, _) in dirs {
        if let Some(export) = exports.remove(id) {
            out.push(Moved {
                export,
                dir: id.clone(),
            });
        }
    }
    for dir in order {
        if let Some(export) = exports.remove(&dir) {
            out.push(Moved { export, dir });
        }
    }
    out
}

/// A project file of before (version 18 and before) with its settings moved into the project's exports
/// and let go of everywhere in it — read as a store made before is brought up.
pub(crate) fn move_file(file: &mut Value) {
    let rows = |key: &str| -> Vec<Map<String, Value>> {
        file.get(key)
            .and_then(Value::as_array)
            .map(|list| list.iter().filter_map(|v| v.as_object().cloned()).collect())
            .unwrap_or_default()
    };
    let text =
        |o: &Map<String, Value>, k: &str| o.get(k).and_then(Value::as_str).map(str::to_string);
    let sections: Vec<Section> = rows("sections")
        .iter()
        .filter_map(|o| {
            Some(Section {
                id: text(o, "id")?,
                parent: text(o, "parent_id"),
                kind: text(o, "kind"),
                settings: settings_of(o, "export_to"),
            })
        })
        .collect();
    let cards: Vec<Card> = rows("cards")
        .iter()
        .filter(|o| o.get("in").is_none_or(Value::is_null))
        .filter_map(|o| {
            Some(Card {
                id: text(o, "id")?,
                section: text(o, "section")?,
                kind: text(o, "kind").unwrap_or_default(),
                settings: settings_of(o, "exportTo"),
            })
        })
        .collect();
    let dirs: Vec<(String, String)> = file["project"]["export_dirs"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|d| {
            Some((
                d["id"].as_str()?.to_string(),
                d["name"].as_str()?.to_string(),
            ))
        })
        .collect();
    let exports: Vec<Export> = exports_from(&sections, &cards, &dirs)
        .into_iter()
        .map(|m| m.export)
        .collect();
    if let Some(project) = file.get_mut("project").and_then(Value::as_object_mut)
        && !project.contains_key("exports")
        && let Some(kept) = crate::export::settings::kept(&exports)
        && let Ok(value) = serde_json::from_str::<Value>(&kept)
    {
        project.insert("exports".into(), value);
    }
    // A file before version 20 names the working directory each export wrote into: it holds none
    // now, and the file carries no folder to put as the default.
    if let Some(list) = file
        .get_mut("project")
        .and_then(|p| p.get_mut("exports"))
        .and_then(Value::as_array_mut)
    {
        for export in list.iter_mut().filter_map(Value::as_object_mut) {
            export.shift_remove("dir");
        }
    }
    let mut doc = std::mem::take(file);
    strip(&mut doc);
    *file = doc;
}

/// A value with every export setting in it let go of; whether it had any. A document held as text
/// inside it is let go of too.
fn strip(value: &mut Value) -> bool {
    match value {
        Value::Object(o) => {
            let mut changed = o.shift_remove("exportTo").is_some();
            changed |= o.shift_remove("export_to").is_some();
            for inner in o.values_mut() {
                changed |= strip(inner);
            }
            changed
        }
        Value::Array(list) => list.iter_mut().fold(false, |c, v| strip(v) || c),
        Value::String(s) if s.starts_with('{') => {
            let Ok(mut inner) = serde_json::from_str::<Value>(s) else {
                return false;
            };
            if !strip(&mut inner) {
                return false;
            }
            *s = crate::ojson::compact(&inner);
            true
        }
        _ => false,
    }
}

/// A document's text with the export settings let go of; none when it had none. A row of the history
/// holds a document as text inside its own: let go of there too.
fn without(text: &str) -> Option<String> {
    let mut value: Value = serde_json::from_str(text).ok()?;
    strip(&mut value).then(|| crate::ojson::compact(&value))
}

/// Every tree's documents of a project let go of their export settings.
fn let_go(conn: &Connection, project: &str) -> Result<(), String> {
    for (table, column, of_project) in [
        ("card_docs", "doc", "project_id = ?1"),
        ("card_docs", "base", "project_id = ?1"),
        ("section_docs", "doc", "project_id = ?1"),
        ("section_docs", "base", "project_id = ?1"),
        ("branch_frozen", "doc", "project_id = ?1"),
        (
            "commit_rows",
            "before",
            "commit_id IN (SELECT id FROM commits WHERE project_id = ?1)",
        ),
    ] {
        if !crate::store::table_exists(conn, table)? {
            continue;
        }
        let rows: Vec<(i64, String)> = {
            let mut stmt = conn
                .prepare(&format!(
                    "SELECT rowid, {column} FROM {table} WHERE {of_project} AND ({column} LIKE '%exportTo%' OR {column} LIKE '%export\\_to%' ESCAPE '\\')"
                ))
                .map_err(sql)?;
            stmt.query_map([project], |r| Ok((r.get(0)?, r.get(1)?)))
                .map_err(sql)?
                .collect::<Result<_, _>>()
                .map_err(sql)?
        };
        for (rowid, text) in rows {
            if let Some(moved) = without(&text) {
                conn.execute(
                    &format!("UPDATE {table} SET {column} = ? WHERE rowid = ?"),
                    params![moved, rowid],
                )
                .map_err(sql)?;
            }
        }
    }
    Ok(())
}

/// One project's settings moved into its exports, and let go of everywhere; exports it has already
/// are kept, and none are made.
pub(crate) fn move_project(conn: &Connection, project: &str) -> Result<(), String> {
    let kept: Option<String> = conn
        .query_row(
            "SELECT exports FROM projects WHERE id = ?",
            [project],
            |r| r.get(0),
        )
        .map_err(sql)?;
    if kept.is_none() {
        let dirs: Vec<(String, String)> = crate::tools::directories::directories(conn, project)?
            .into_iter()
            .map(|d| (d.id, d.name))
            .collect();
        let moved = exports_of(conn, project, &dirs)?;
        let exports: Vec<Export> = moved.iter().map(|m| m.export.clone()).collect();
        conn.execute(
            "UPDATE projects SET exports = ? WHERE id = ?",
            params![crate::export::settings::kept(&exports), project],
        )
        .map_err(sql)?;
        // The folder each working directory was bound to is where its export is written by default.
        conn.execute_batch(crate::schema::EXPORT_DEFAULT_FOLDERS)
            .map_err(sql)?;
        for m in &moved {
            carry_folder(conn, project, &m.dir, &m.export.id)?;
        }
    }
    let_go(conn, project)
}

/// The folder a working directory is bound to on this Mac made the default folder of an export
/// (kept if it has one already).
fn carry_folder(conn: &Connection, project: &str, dir: &str, export: &str) -> Result<(), String> {
    conn.execute(
        "INSERT OR IGNORE INTO export_default_folders (project_id, export_id, folder)
         SELECT project_id, ?3, folder FROM export_dir_folders WHERE project_id = ?1 AND dir_id = ?2",
        params![project, dir, export],
    )
    .map_err(sql)?;
    Ok(())
}

/// Every project of a store whose exports still name a working directory (`dir`): the folder it is
/// bound to the default folder of the export, and the exports let go of `dir`. Once.
pub(crate) fn carry_all(conn: &Connection) -> Result<(), String> {
    let rows: Vec<(String, String)> = {
        let mut stmt = conn
            .prepare("SELECT id, exports FROM projects WHERE exports IS NOT NULL AND exports LIKE '%\"dir\"%'")
            .map_err(sql)?;
        stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(sql)?
            .collect::<Result<_, _>>()
            .map_err(sql)?
    };
    for (project, text) in rows {
        let Ok(Value::Array(mut list)) = serde_json::from_str::<Value>(&text) else {
            continue;
        };
        for export in list.iter_mut().filter_map(Value::as_object_mut) {
            let Some(dir) = export.shift_remove("dir") else {
                continue;
            };
            if let (Some(dir), Some(id)) = (
                dir.as_str(),
                export.get("id").and_then(Value::as_str).map(str::to_string),
            ) {
                carry_folder(conn, &project, dir, &id)?;
            }
        }
        conn.execute(
            "UPDATE projects SET exports = ? WHERE id = ?",
            params![crate::ojson::compact(&Value::Array(list)), project],
        )
        .map_err(sql)?;
    }
    Ok(())
}

/// Every project of a store made before, once.
pub(crate) fn move_all(conn: &Connection) -> Result<(), String> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures;

    /// The exports as said apart from their ids, which each move makes anew.
    fn said(exports: &[Export]) -> Vec<(String, Vec<Entry>, Vec<String>)> {
        exports
            .iter()
            .map(|e| (e.name.clone(), e.entries.clone(), e.exclude.clone()))
            .collect()
    }

    /// A store made before (`core_0015_exports`) — the cards' and sections' settings of the project
    /// file of before put back into its documents, every tree, and no exports — moved once: the exports
    /// the project file of now has, and no document keeping a setting.
    #[test]
    fn a_store_made_before_has_its_settings_moved_into_its_exports_once() {
        let place = fixtures::store_with(&["large"]);
        let project = fixtures::project_id("large");
        let before: Value = serde_json::from_str(
            &std::fs::read_to_string(
                std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("tests/fixtures/projects/before/large.json"),
            )
            .unwrap(),
        )
        .unwrap();
        let now: Value = serde_json::from_str(&fixtures::project_file("large")).unwrap();
        let conn = Connection::open(&place).unwrap();
        let mut put = 0;
        for (table, list, key) in [
            ("card_docs", "cards", "exportTo"),
            ("section_docs", "sections", "export_to"),
        ] {
            for row in before[list].as_array().unwrap() {
                let Some(settings) = row.get(key) else {
                    continue;
                };
                let id = row["id"].as_str().unwrap();
                let doc: String = conn
                    .query_row(
                        &format!(
                            "SELECT doc FROM {table} WHERE project_id = ? AND branch_id = '' AND id = ?"
                        ),
                        [&project, id],
                        |r| r.get(0),
                    )
                    .unwrap();
                let mut doc: Value = serde_json::from_str(&doc).unwrap();
                doc[key] = settings.clone();
                conn.execute(
                    &format!(
                        "UPDATE {table} SET doc = ? WHERE project_id = ? AND branch_id = '' AND id = ?"
                    ),
                    params![doc.to_string(), project, id],
                )
                .unwrap();
                put += 1;
            }
        }
        assert!(put > 100, "{put}");
        conn.execute(
            "UPDATE projects SET exports = NULL WHERE id = ?",
            [&project],
        )
        .unwrap();

        move_project(&conn, &project).unwrap();
        let moved = crate::export::settings::parse(
            conn.query_row(
                "SELECT exports FROM projects WHERE id = ?",
                [&project],
                |r| r.get::<_, Option<String>>(0),
            )
            .unwrap()
            .as_deref(),
        );
        let wanted = crate::export::settings::parse(Some(&now["project"]["exports"].to_string()));
        assert_eq!(said(&moved), said(&wanted));
        let left: i64 = conn
            .query_row(
                "SELECT count(*) FROM card_docs WHERE project_id = ? AND (doc LIKE '%exportTo%' OR base LIKE '%exportTo%')",
                [&project],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(left, 0);
        // Once: the exports there are kept as they are.
        conn.execute(
            "UPDATE projects SET exports = '[]' WHERE id = ?",
            [&project],
        )
        .unwrap();
        move_project(&conn, &project).unwrap();
        let again: Option<String> = conn
            .query_row(
                "SELECT exports FROM projects WHERE id = ?",
                [&project],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(again.as_deref(), Some("[]"));
    }

    /// What the defaults of before no longer reach is left out: a section with settings of its own from
    /// the exports it does not write to, a card with its own or `none` likewise; `default`, its section's
    /// too.
    #[test]
    fn what_the_defaults_no_longer_reach_is_left_out() {
        let file = |kind: &str, dir: Option<&str>, path: &str| ExportSetting {
            kind: kind.into(),
            dir: dir.map(str::to_string),
            path: path.into(),
            format: None,
        };
        let section = |id: &str, parent: Option<&str>, settings: Vec<ExportSetting>| Section {
            id: id.into(),
            parent: parent.map(str::to_string),
            kind: Some("schema".into()),
            settings,
        };
        let card = |id: &str, section: &str, settings: Vec<ExportSetting>| Card {
            id: id.into(),
            section: section.into(),
            kind: "entity".into(),
            settings,
        };
        let sections = vec![
            section("top", None, vec![]),
            section(
                "s",
                Some("top"),
                vec![
                    file("file", Some("a"), "all.ts"),
                    file("file", Some("b"), "all.json"),
                ],
            ),
            section(
                "inner",
                Some("s"),
                vec![file("file", Some("a"), "inner.ts")],
            ),
        ];
        let cards = vec![
            card("plain", "s", vec![]),
            card("own", "s", vec![file("file", Some("a"), "own.ts")]),
            card(
                "both",
                "s",
                vec![
                    file("default", None, ""),
                    file("file", Some("a"), "both.ts"),
                ],
            ),
            card("none", "s", vec![file("none", None, "")]),
        ];
        let dirs = vec![
            ("a".to_string(), "A".to_string()),
            ("b".to_string(), "B".to_string()),
        ];
        let out = exports_from(&sections, &cards, &dirs);
        let a = &out[0].export;
        let b = &out[1].export;
        assert_eq!((a.name.as_str(), b.name.as_str()), ("A", "B"));
        assert_eq!((out[0].dir.as_str(), out[1].dir.as_str()), ("a", "b"));
        let entries = |e: &Export| -> Vec<(String, String)> {
            e.entries
                .iter()
                .map(|x| (x.from.clone(), x.to.clone()))
                .collect()
        };
        let pair = |f: &str, t: &str| (f.to_string(), t.to_string());
        assert_eq!(
            entries(a),
            [
                pair("s", "all.ts"),
                pair("inner", "inner.ts"),
                pair("own", "own.ts"),
                pair("both", "both.ts"),
                pair("both", "all.ts"),
            ]
        );
        assert_eq!(entries(b), [pair("s", "all.json")]);
        // B: the inner section writes nowhere there; the card with its own neither; none, nowhere.
        assert_eq!(b.exclude, ["inner", "own", "none"]);
        assert_eq!(a.exclude, ["none"]);
    }
}
