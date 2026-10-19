//! The built-in kind of section `instances` replaced by `definition` (the store's mark
//! `core_0021_definition_kind`, a project file before version 23).
//!
//! A definition section holds all an instances section held and the shapes beside them, so each
//! section of the kind of before becomes one of `definition` and loses nothing it holds: in every
//! tree's documents and their bases (main, the board, each branch), what a branch keeps frozen of
//! main, the history's rows and the project tree's `type`. None is left of the kind of before: a
//! kind of section no definition names would hold anything. What a project wrote over the
//! built-in kind by its name (`icons`, `contexts`) goes by the name of now, and the open requests to
//! merge still as their branch is are printed again, as the documents they were tried on changed.
//!
//! A project's own kind of section named `definition` stood where the built-in one is now, and two
//! kinds of one name break every write of the project's kinds. When the built-in one stands in for
//! it wholly — it holds all the project's held, makes nothing but itself inside it, and the board
//! does not put it directly under its root, which the built-in one may not be — the project's own
//! is let go of and its sections are the built-in one's. Otherwise it is kept, named
//! `definition_project` (numbered when the project has that name already), and its sections, the
//! kinds of section that make it inside them and the project tree follow it.

use crate::store::{parse, sql, table_exists};
use rusqlite::{Connection, params};
use serde_json::{Map, Value};

/// The built-in kind of before.
const BEFORE: &str = "instances";

/// The built-in kind of now.
const NOW: &str = "definition";

/// The name a project's own kind named `definition` is kept by, when the built-in one cannot
/// stand in for it.
const OWN_KEPT: &str = "definition_project";

/// How the kinds of section of a project are named anew: each `(from, to)`, all at once — a
/// project's own `definition` goes to its name of now before the built-in one takes the name.
struct Renames(Vec<(String, String)>);

impl Renames {
    fn of(&self, kind: &str) -> Option<&str> {
        self.0
            .iter()
            .find(|(from, _)| from == kind)
            .map(|(_, to)| to.as_str())
    }
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

/// One project moved, in every tree: a store's, or a project file's of before once it is read into
/// the store.
pub fn move_project(conn: &Connection, project: &str) -> Result<(), String> {
    let requests = crate::store::requests_table(conn)?;
    // The open requests still current, to print again once the documents are moved.
    let open: Vec<(String, String, String)> = {
        let mut stmt = conn
            .prepare(&format!(
                "SELECT id, branch_id, print FROM {requests} WHERE project_id = ? AND state = 'open'"
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

    let kept: Option<String> = conn
        .query_row("SELECT kinds FROM projects WHERE id = ?", [project], |r| {
            r.get(0)
        })
        .map_err(sql)?;
    let mut kinds = parse(kept.as_deref());
    let renames = match kinds.as_mut() {
        Some(kinds) => {
            let (renames, changed) = kinds_moved(kinds);
            if changed {
                conn.execute(
                    "UPDATE projects SET kinds = ? WHERE id = ?",
                    params![Value::Object(kinds.clone()).to_string(), project],
                )
                .map_err(sql)?;
            }
            renames
        }
        None => Renames(vec![(BEFORE.into(), NOW.into())]),
    };

    let mut moved_any = false;
    for (table, column, of_project) in [
        ("section_docs", "doc", "project_id = ?1"),
        ("section_docs", "base", "project_id = ?1"),
        (
            "branch_frozen",
            "doc",
            "project_id = ?1 AND tbl = 'section_docs'",
        ),
    ] {
        if !table_exists(conn, table)? {
            continue;
        }
        for (rowid, text) in rows_naming(conn, table, column, of_project, project, &renames)? {
            if let Some(moved) = doc_moved(&text, &renames) {
                conn.execute(
                    &format!("UPDATE {table} SET {column} = ? WHERE rowid = ?"),
                    params![moved, rowid],
                )
                .map_err(sql)?;
                moved_any = true;
            }
        }
    }
    // The history's rows of section documents, so what a commit kept compares with what is
    // written now and puts back a kind there is.
    for (rowid, text) in rows_naming(
        conn,
        "commit_rows",
        "before",
        "tbl = 'section_docs' AND commit_id IN (SELECT id FROM commits WHERE project_id = ?1)",
        project,
        &renames,
    )? {
        let Some(mut columns) = parse(Some(&text)) else {
            continue;
        };
        let mut changed = false;
        for key in ["doc", "base"] {
            if let Some(next) = columns
                .get(key)
                .and_then(Value::as_str)
                .and_then(|t| doc_moved(t, &renames))
            {
                columns.insert(key.into(), next.into());
                changed = true;
            }
        }
        if changed {
            conn.execute(
                "UPDATE commit_rows SET before = ? WHERE rowid = ?",
                params![Value::Object(columns).to_string(), rowid],
            )
            .map_err(sql)?;
        }
    }
    if moved_any {
        for (id, branch) in current {
            let print = crate::tools::requests::request_print(conn, project, &branch)?;
            conn.execute(
                &format!("UPDATE {requests} SET print = ? WHERE id = ?"),
                params![print, id],
            )
            .map_err(sql)?;
        }
    }
    Ok(())
}

/// The rows of `table` of the project whose `column` names a kind that is named anew somewhere in
/// it (the documents are read only where they may say one).
fn rows_naming(
    conn: &Connection,
    table: &str,
    column: &str,
    of_project: &str,
    project: &str,
    renames: &Renames,
) -> Result<Vec<(i64, String)>, String> {
    let mut out: Vec<(i64, String)> = Vec::new();
    for (from, _) in &renames.0 {
        let mut stmt = conn
            .prepare(&format!(
                "SELECT rowid, {column} FROM {table} WHERE {of_project} AND {column} IS NOT NULL AND instr({column}, ?2) > 0"
            ))
            .map_err(sql)?;
        let rows = stmt
            .query_map(params![project, from], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(sql)?
            .collect::<Result<Vec<(i64, String)>, _>>()
            .map_err(sql)?;
        for row in rows {
            if !out.iter().any(|(id, _)| *id == row.0) {
                out.push(row);
            }
        }
    }
    Ok(out)
}

/// A section's document with its kind named anew; none when its kind stays.
fn doc_moved(text: &str, renames: &Renames) -> Option<String> {
    let mut doc = parse(Some(text))?;
    let to = renames.of(doc.get("kind")?.as_str()?)?.to_string();
    doc.insert("kind".into(), to.into());
    Some(Value::Object(doc).to_string())
}

/// The project's kinds (`projects.kinds`) with the kinds of section named anew: how they were
/// named, and whether anything in them changed.
fn kinds_moved(kinds: &mut Map<String, Value>) -> (Renames, bool) {
    let mut changed = false;
    let mut renames = vec![(BEFORE.to_string(), NOW.to_string())];
    let board_puts_it = kinds
        .get("project_tree")
        .and_then(|t| t.get("board_sections"))
        .and_then(Value::as_array)
        .is_some_and(|list| list.iter().any(|t| t["type"] == NOW));
    if let Some(sections) = kinds.get_mut("sections").and_then(Value::as_array_mut)
        && let Some(at) = sections.iter().position(|s| s["section"] == NOW)
    {
        if stands_in_for(&sections[at]) && !board_puts_it {
            sections.remove(at);
        } else {
            let name = free_name(sections);
            sections[at]["section"] = name.clone().into();
            renames.insert(0, (NOW.to_string(), name));
        }
        changed = true;
    }
    let renames = Renames(renames);
    let rename = |value: &mut Value| -> bool {
        let Some(to) = value.as_str().and_then(|k| renames.of(k)) else {
            return false;
        };
        *value = to.into();
        true
    };
    // The kinds of section made inside each of the project's own.
    if let Some(sections) = kinds.get_mut("sections").and_then(Value::as_array_mut) {
        for section in sections {
            for inner in section
                .get_mut("sections")
                .and_then(Value::as_array_mut)
                .into_iter()
                .flatten()
            {
                changed |= rename(inner);
            }
        }
    }
    if let Some(tree) = kinds.get_mut("project_tree").and_then(Value::as_object_mut) {
        for key in ["main_sections", "board_sections"] {
            for top in tree
                .get_mut(key)
                .and_then(Value::as_array_mut)
                .into_iter()
                .flatten()
            {
                if let Some(kind) = top.get_mut("type") {
                    changed |= rename(kind);
                }
            }
        }
    }
    // What the project wrote over the built-in kind, by its name: only a built-in kind is named
    // there.
    for key in ["icons", "contexts"] {
        if let Some(over) = kinds.get_mut(key).and_then(Value::as_object_mut)
            && over.contains_key(BEFORE)
            && !over.contains_key(NOW)
        {
            let named: Map<String, Value> = std::mem::take(over)
                .into_iter()
                .map(|(k, v)| (if k == BEFORE { NOW.to_string() } else { k }, v))
                .collect();
            *over = named;
            changed = true;
        }
    }
    (renames, changed)
}

/// Whether the built-in kind stands in for a project's own of its name: it holds every kind of
/// card the project's held, and makes nothing but itself inside it.
fn stands_in_for(own: &Value) -> bool {
    let builtin = crate::kinds::builtin_defs()
        .section(NOW)
        .expect("the built-in definition kind");
    let names = |key: &str| -> Vec<String> {
        own.get(key)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect()
    };
    names("holds")
        .iter()
        .all(|k| builtin.holds.iter().any(|b| b == k))
        && names("sections").iter().all(|k| k == NOW)
}

/// The name the project's own kind is kept by: `definition_project`, numbered when the project has
/// a kind of that name already.
fn free_name(sections: &[Value]) -> String {
    let taken = |name: &str| sections.iter().any(|s| s["section"] == name);
    let mut name = OWN_KEPT.to_string();
    let mut n = 2;
    while taken(&name) {
        name = format!("{OWN_KEPT}_{n}");
        n += 1;
    }
    name
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn moved(kinds: Value) -> (Value, Vec<(String, String)>, bool) {
        let mut kinds = kinds.as_object().unwrap().clone();
        let (renames, changed) = kinds_moved(&mut kinds);
        (Value::Object(kinds), renames.0, changed)
    }

    #[test]
    fn the_kind_of_before_goes_by_the_name_of_now_wherever_the_kinds_name_it() {
        let (kinds, renames, changed) = moved(json!({
            "sections": [{ "section": "api", "holds": ["entity"], "sections": ["api", "instances"] }],
            "project_tree": {
                "main_sections": [{ "type": "document", "name": "Docs" }, { "type": "instances", "name": "Data" }],
                "board_sections": [{ "type": "goals", "name": "Goals" }]
            },
            "icons": { "schema": { "color": "#112233" }, "instances": { "name": "star" } },
            "contexts": { "instances": { "use_when": "例" } }
        }));
        assert!(changed);
        assert_eq!(renames, [("instances".into(), "definition".into())]);
        assert_eq!(
            kinds["sections"][0]["sections"],
            json!(["api", "definition"])
        );
        assert_eq!(
            kinds["project_tree"]["main_sections"][1]["type"],
            "definition"
        );
        // The order the project wrote them in stays.
        let icons: Vec<&String> = kinds["icons"].as_object().unwrap().keys().collect();
        assert_eq!(icons, ["schema", "definition"]);
        assert_eq!(kinds["icons"]["definition"]["name"], "star");
        assert_eq!(kinds["contexts"]["definition"]["use_when"], "例");
        // Moved once: nothing more the second time.
        let (_, _, again) = moved(kinds);
        assert!(!again);
    }

    #[test]
    fn a_projects_own_definition_the_built_in_one_stands_in_for_is_let_go_of() {
        let (kinds, renames, changed) = moved(json!({
            "sections": [
                { "section": "definition", "label": "定義", "holds": ["entity", "value", "enum", "store", "instance", "note"], "sections": ["definition"] },
                { "section": "api", "holds": ["entity"], "sections": ["definition"] }
            ],
            "project_tree": { "main_sections": [{ "type": "definition", "name": "Definition" }] }
        }));
        assert!(changed);
        assert_eq!(renames, [("instances".into(), "definition".into())]);
        let names: Vec<&Value> = kinds["sections"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| &s["section"])
            .collect();
        assert_eq!(names, [&json!("api")]);
        assert_eq!(kinds["sections"][0]["sections"], json!(["definition"]));
        assert_eq!(
            kinds["project_tree"]["main_sections"][0]["type"],
            "definition"
        );
    }

    #[test]
    fn a_projects_own_definition_holding_more_is_kept_by_a_name_of_its_own() {
        let (kinds, renames, _) = moved(json!({
            "sections": [
                { "section": "definition", "holds": ["entity", "flow"], "sections": ["definition"] },
                { "section": "api", "holds": ["entity"], "sections": ["definition", "instances"] }
            ],
            "project_tree": {
                "main_sections": [{ "type": "definition", "name": "Defs" }, { "type": "instances", "name": "Data" }]
            }
        }));
        assert_eq!(
            renames,
            [
                ("definition".into(), "definition_project".into()),
                ("instances".into(), "definition".into())
            ]
        );
        assert_eq!(kinds["sections"][0]["section"], "definition_project");
        assert_eq!(
            kinds["sections"][0]["sections"],
            json!(["definition_project"])
        );
        assert_eq!(
            kinds["sections"][1]["sections"],
            json!(["definition_project", "definition"])
        );
        assert_eq!(
            kinds["project_tree"]["main_sections"],
            json!([{ "type": "definition_project", "name": "Defs" }, { "type": "definition", "name": "Data" }])
        );
        // Made inside something but itself, or put directly under the board's root: kept too.
        let (_, inside, _) = moved(json!({
            "sections": [{ "section": "definition", "holds": ["entity"], "sections": ["definition", "chart"] }]
        }));
        assert_eq!(inside[0].1, "definition_project");
        let (_, board, _) = moved(json!({
            "sections": [{ "section": "definition", "holds": ["entity"] }],
            "project_tree": { "board_sections": [{ "type": "definition", "name": "Specs" }] }
        }));
        assert_eq!(board[0].1, "definition_project");
        // A name taken already: numbered.
        let (kinds, taken, _) = moved(json!({
            "sections": [
                { "section": "definition_project", "holds": ["note"] },
                { "section": "definition", "holds": ["*"] }
            ]
        }));
        assert_eq!(taken[0].1, "definition_project_2");
        assert_eq!(kinds["sections"][1]["section"], "definition_project_2");
    }

    #[test]
    fn a_section_document_takes_the_kind_of_now_and_nothing_else_of_it_moves() {
        let renames = Renames(vec![
            ("definition".into(), "definition_project".into()),
            ("instances".into(), "definition".into()),
        ]);
        let before =
            r#"{"id":"s","parent_id":"r","name":"Data","kind":"instances","description":"例"}"#;
        assert_eq!(
            doc_moved(before, &renames).unwrap(),
            r#"{"id":"s","parent_id":"r","name":"Data","kind":"definition","description":"例"}"#
        );
        let own = r#"{"id":"t","name":"Defs","kind":"definition"}"#;
        assert_eq!(
            doc_moved(own, &renames).unwrap(),
            r#"{"id":"t","name":"Defs","kind":"definition_project"}"#
        );
        assert!(doc_moved(r#"{"id":"u","name":"instances","kind":"schema"}"#, &renames).is_none());
        assert!(doc_moved(r#"{"id":"v","name":"Main"}"#, &renames).is_none());
    }
}
