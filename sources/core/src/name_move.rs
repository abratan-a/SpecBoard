//! The built-in names of now (the store's mark `core_0026_builtin_names`, a project file before
//! version 33): the kinds of line named as the screen says them, an enum's value keyed by `name`
//! as MCP writes it, a goal's status and priority by values in English (their Japanese the
//! labels, `optionLabels`).
//!
//! A line of a kind named so before (`kinds::LINE_NAMES_BEFORE`) is named as it is now. A project's
//! own kind of line named as a built-in one is now steps aside (`rules_2`, `_3` …), its lines with
//! it, and a built-in kind's context the project wrote over by its name of before goes by its name
//! of now. An enum's value keyed by `value` is keyed by `name`; a goal's values of before
//! (`kinds::GOAL_VALUES_BEFORE`) are those of now.
//!
//! As the lines were (`line_move::move_card_docs`): every tree — main, the board, each branch —
//! its documents and bases, the history's rows, a branch's prints of main's cards and the open
//! requests' prints; a tree that moved any says so in one commit of one record.

use rusqlite::{Connection, params};
use serde_json::{Map, Value};
use std::collections::HashMap;

type Obj = Map<String, Value>;

fn sql(e: rusqlite::Error) -> String {
    text!("nameMove.error.sql", e)
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

/// One project moved: its own kinds first, then its cards in every tree.
pub fn move_project(conn: &Connection, project: &str) -> Result<(), String> {
    let own = own_kinds(conn, project)?;
    let one = |doc: &Obj| move_doc(doc, &own);
    let said = |cards: &[String]| {
        let mut names: Vec<&str> = Vec::new();
        for card in cards {
            if !names.contains(&card.as_str()) {
                names.push(card);
            }
        }
        vec![keyed!(
            "nameMove.line.builtinNames",
            count = cards.len(),
            cards = crate::text::List::of(names)
        )]
    };
    crate::line_move::move_card_docs(
        conn,
        project,
        &crate::write::history::migrated(keyed!("history.migrate.builtinNames")),
        &one,
        &said,
    )
}

/// The project's own kinds of line named as a built-in one is now, each renamed (`_2`, `_3` …)
/// and written back, with the contexts it writes over the built-in kinds by their names of now;
/// what each own kind was renamed to.
fn own_kinds(conn: &Connection, project: &str) -> Result<HashMap<String, String>, String> {
    let kept: Option<String> = conn
        .query_row("SELECT kinds FROM projects WHERE id = ?", [project], |r| {
            r.get(0)
        })
        .map_err(sql)?;
    let Some(mut kinds) = kept
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .and_then(|v| v.as_object().cloned())
    else {
        return Ok(HashMap::new());
    };
    let now: Vec<&str> = crate::kinds::LINE_NAMES_BEFORE
        .iter()
        .map(|(_, now)| *now)
        .collect();
    let mut renamed: HashMap<String, String> = HashMap::new();
    let mut changed = false;
    if let Some(Value::Array(lines)) = kinds.get_mut("lines") {
        let names: Vec<String> = lines
            .iter()
            .filter_map(|l| l["line"].as_str().map(str::to_string))
            .collect();
        let builtin = crate::kinds::builtin_defs();
        for line in lines.iter_mut() {
            let name = line["line"].as_str().unwrap_or_default().to_string();
            if !now.contains(&name.as_str()) {
                continue;
            }
            let taken = |name: &str| {
                builtin.line(name).is_some()
                    || names.iter().any(|n| n == name)
                    || renamed.values().any(|n| n == name)
            };
            let mut n = 2;
            while taken(&format!("{name}_{n}")) {
                n += 1;
            }
            let next = format!("{name}_{n}");
            line["line"] = next.clone().into();
            renamed.insert(name, next);
            changed = true;
        }
    }
    if let Some(Value::Object(contexts)) = kinds.get_mut("contexts") {
        for (before, now) in crate::kinds::LINE_NAMES_BEFORE {
            if !contexts.contains_key(now)
                && let Some(said) = contexts.remove(before)
            {
                contexts.insert(now.into(), said);
                changed = true;
            }
        }
    }
    if changed {
        conn.execute(
            "UPDATE projects SET kinds = ? WHERE id = ?",
            params![Value::Object(kinds).to_string(), project],
        )
        .map_err(sql)?;
    }
    Ok(renamed)
}

/// A card's document with the names of now, or none when it has none of before: its lines, an
/// enum's values, a goal's status and priority. `own`: the project's own kinds of line renamed.
fn move_doc(doc: &Obj, own: &HashMap<String, String>) -> Option<Obj> {
    let mut out = doc.clone();
    let mut changed = false;
    if let Some(Value::Array(lines)) = out.get_mut("connections") {
        for line in lines.iter_mut() {
            let Some(name) = line.get("line").and_then(Value::as_str) else {
                continue;
            };
            let next = own.get(name).cloned().or_else(|| {
                crate::kinds::LINE_NAMES_BEFORE
                    .iter()
                    .find(|(before, _)| *before == name)
                    .map(|(_, now)| (*now).to_string())
            });
            if let Some(next) = next {
                line["line"] = next.into();
                changed = true;
            }
        }
    }
    let kind = doc.get("kind").and_then(Value::as_str).unwrap_or_default();
    if kind == "enum"
        && let Some(Value::Array(values)) = out.get_mut("rows").and_then(|r| r.get_mut("values"))
    {
        for value in values.iter_mut() {
            let Some(fields) = value.get_mut("fields").and_then(Value::as_object_mut) else {
                continue;
            };
            if fields.contains_key("value") && !fields.contains_key("name") {
                // Each where the one of before was, so a document reads in the order it was written.
                *fields = fields
                    .iter()
                    .map(|(k, v)| {
                        let k = if k == "value" { "name" } else { k.as_str() };
                        (k.to_string(), v.clone())
                    })
                    .collect();
                changed = true;
            }
        }
    }
    if kind == "goal"
        && let Some(fields) = out.get_mut("fields").and_then(Value::as_object_mut)
    {
        for (key, before, now) in crate::kinds::GOAL_VALUES_BEFORE {
            if fields.get(key).and_then(Value::as_str) == Some(before) {
                fields.insert(key.into(), now.into());
                changed = true;
            }
        }
    }
    if !changed {
        return None;
    }
    out.insert("v".into(), crate::kinds::builtin().version.into());
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn obj(v: Value) -> Obj {
        v.as_object().unwrap().clone()
    }

    #[test]
    fn a_document_of_before_is_named_as_now() {
        let own: HashMap<String, String> = [("rules".to_string(), "rules_2".to_string())].into();
        let flow = obj(json!({"kind": "flow", "name": "F", "connections": [
            {"line": "notes", "to": "a"}, {"line": "rules", "to": "b"}, {"line": "next", "to": "c"}
        ]}));
        let moved = move_doc(&flow, &own).unwrap();
        let names: Vec<&str> = moved["connections"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| l["line"].as_str().unwrap())
            .collect();
        assert_eq!(names, ["rules", "rules_2", "next"]);
        let status = obj(json!({"kind": "enum", "name": "S", "rows": {"values": [
            {"id": "1", "fields": {"value": "open", "label": "受付"}}
        ]}}));
        let moved = move_doc(&status, &HashMap::new()).unwrap();
        assert_eq!(
            moved["rows"]["values"][0]["fields"],
            json!({"name": "open", "label": "受付"})
        );
        let goal = obj(
            json!({"kind": "goal", "name": "G", "fields": {"status": "完了", "priority": "低"}}),
        );
        let moved = move_doc(&goal, &HashMap::new()).unwrap();
        assert_eq!(
            moved["fields"],
            json!({"status": "done", "priority": "low"})
        );
        // Moved once: nothing more the second time.
        assert!(move_doc(&moved, &HashMap::new()).is_none());
    }
}
