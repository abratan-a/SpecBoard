//! The tools that write a project's own settings, or make a project (`project_create`,
//! `project_update`; the Swift app's `MCPSurface.projects`, `MCPTools`' `create_project` /
//! `update_project` / `exportDirectoryEdits` and `Repository.createProject`,
//! `updateProjectDescription`, `setDefaultSettings` and the export directories of
//! `Repository+Exports`): what a project holds once, whichever tree is read — not a branch's.
//! Its name and its writing rules are written here for the window too (`project_rename`,
//! `writing_rules_set`), in the same words.

use super::args::Args;
use super::cards::{Scope, text};
use super::grid::Layout;
use crate::store::{BOARD, sql};
use crate::write::{Tx, db, new_id};
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Map, Value};
use std::collections::BTreeMap;

/// Text that has to be there, held to a length (`Validate.required`).
pub(crate) fn required(raw: &str, field: &str, max: usize) -> Result<String, String> {
    let value = text(raw, field, max)?;
    if value.is_empty() {
        return Err(text!("cards.field.required", field));
    }
    Ok(value)
}

/// A section's document as a new project writes it: no parent for a root, the place among
/// its siblings left out at zero (`RowCodec.object`).
fn section_doc(
    id: &str,
    parent: Option<&str>,
    name: &str,
    order: i64,
    at: &str,
    kind: Option<&str>,
) -> Map<String, Value> {
    let mut doc = Map::new();
    doc.insert("id".into(), id.into());
    if let Some(parent) = parent {
        doc.insert("parent_id".into(), parent.into());
    }
    doc.insert("name".into(), name.into());
    if order != 0 {
        doc.insert("sort_order".into(), order.into());
    }
    doc.insert("created_at".into(), at.into());
    if let Some(kind) = kind {
        doc.insert("kind".into(), kind.into());
    }
    doc
}

/// A project's name, refused when another project of the store has it — in any case, as a call
/// names a project (`project_summary_in`): two of one name and a call by it would reach either.
/// Names are one to a store, whoever may see the projects; a key that may not see the one that has
/// it is told only that the name cannot be used — not which project has it, nor its slug.
fn unique_name(conn: &Connection, name: &str, excluding: Option<&str>) -> Result<(), String> {
    let mut stmt = conn
        .prepare("SELECT id, name, slug FROM projects")
        .map_err(sql)?;
    let taken = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })
        .map_err(sql)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sql)?
        .into_iter()
        .find(|(id, other, _)| {
            Some(id.as_str()) != excluding && crate::pathtext::same(other, name)
        });
    match taken {
        Some((id, _, _)) if !crate::access::sees(conn, &id)? => {
            Err(text!("projects.name.unavailable", name))
        }
        Some((_, other, slug)) => Err(text!("projects.name.taken", other, slug)),
        None => Ok(()),
    }
}

// MARK: - project_create

/// A project made (`create_project`, `Repository.createProject`): main's root with one section of
/// each of the root's own kinds, the project's layout, and its board beside main
/// (`ensureBoard`). Not opened: the window stays where it is.
pub(crate) fn project_create(tx: &mut Tx, args: &Args) -> Result<Value, String> {
    let given = args.value()?;
    super::check(&given, "project_create", "project")?;
    let name = required(
        args.string("project")?,
        &text!("projects.label.projectName"),
        120,
    )?;
    unique_name(tx.conn(), &name, None)?;
    let description = text(
        given
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or_default(),
        &text!("common.label.description"),
        2000,
    )?;
    let id = new_id();
    let now = tx.now()?;
    let slug = crate::write::free_slug(tx.conn(), &name, None)?;
    tx.conn()
        .execute(
            "INSERT INTO projects (id, name, slug, description, created_at, updated_at, export_language) VALUES (?, ?, ?, ?, ?, ?, ?)",
            // Its exports in the language of whoever made it: the window's, or English through MCP.
            params![id, name, slug, description, now, now, crate::text::current()],
        )
        .map_err(db)?;
    // A project made through a key is that key's to write (`specboard://SpecBoard/Docs/Design:DesignAccess` §2).
    crate::access::grant_made(tx.conn(), &id)?;
    // What is directly under each root, a new project's (`project_tree`): its own
    // to write again, kept in its kinds.
    let tree = crate::kinds::ProjectTree::default();
    let mut kinds = Map::new();
    kinds.insert("project_tree".into(), tree.json());
    tx.conn()
        .execute(
            "UPDATE projects SET kinds = ? WHERE id = ?",
            params![Value::Object(kinds).to_string(), id],
        )
        .map_err(db)?;
    // Somewhere for a first card of each kind: none goes in the root.
    let root = new_id();
    tx.put_section(&id, "", &section_doc(&root, None, "Main", 0, &now, None))?;
    for (order, top) in tree.main.iter().enumerate() {
        let made = new_id();
        let at = tx.now()?;
        let doc = section_doc(
            &made,
            Some(&root),
            &top.name,
            order as i64,
            &at,
            Some(&top.kind),
        );
        tx.put_section(&id, "", &doc)?;
        tx.line(
            &id,
            "",
            "section.create",
            "section",
            Some(&made),
            keyed!("projects.line.sectionCreated", name = top.name),
        )?;
    }
    tx.conn()
        .execute(
            "INSERT OR IGNORE INTO default_view (project_id, nested, grid_gap) VALUES (?, 0, 1)",
            [&id],
        )
        .map_err(db)?;
    tx.line(
        &id,
        "",
        "project.create",
        "project",
        Some(&id),
        keyed!("projects.line.created", name),
    )?;
    // Its board, beside main: a root, and its sections to put things in.
    let board_root = new_id();
    let at = tx.now()?;
    tx.put_section(
        &id,
        BOARD,
        &section_doc(&board_root, None, "Board", 0, &at, None),
    )?;
    for (order, top) in tree.board.iter().enumerate() {
        let made = new_id();
        tx.put_section(
            &id,
            BOARD,
            &section_doc(
                &made,
                Some(&board_root),
                &top.name,
                order as i64,
                &at,
                Some(&top.kind),
            ),
        )?;
        tx.line(
            &id,
            BOARD,
            "section.create",
            "section",
            Some(&made),
            keyed!("projects.line.sectionCreated", name = top.name),
        )?;
    }
    // Inside Goals, where the card actions move what they finish and what they cancel.
    crate::board_move::made(tx, &id)?;
    tx.line(
        &id,
        BOARD,
        "project.update",
        "project",
        None,
        keyed!("projects.line.boardCreated"),
    )?;
    let mut out = Map::new();
    out.insert("created".into(), name.into());
    out.insert("slug".into(), slug.into());
    Ok(Value::Object(out))
}

// MARK: - project_update

/// The project's settings changed (`update_project`): its export directories first, then its
/// name, its description, its writing rules, its layout and its slug. The layout is read whole before
/// anything is written: a wrong one leaves the description as it was, too. The name and the
/// rules are checked as the window's are (`project_rename`, `writing_rules_set`), and pass
/// main's lock as the description does.
pub(crate) fn project_update(tx: &mut Tx, scope: &Scope, args: &Args) -> Result<Value, String> {
    let given = args.value()?;
    // The kinds are kind_*'s; what is
    // directly under each root and its sections' names stay the project's.
    if let Some(Value::Array(renames)) = given.get("renames")
        && let Some(i) = renames.iter().position(|r| r.get("top_section").is_none())
    {
        return Err(text!("projects.update.renamesShape", i));
    }
    super::check(&given, "project_update", "project")?;
    if ![
        "name",
        "description",
        "layout",
        "rules",
        "project_tree",
        "renames",
        "slug",
        "exportLanguage",
    ]
    .iter()
    .any(|k| given.contains_key(*k))
    {
        return Err(text!("projects.update.nothingToChange"));
    }
    let tree = given
        .get("project_tree")
        .map(|t| Value::Object(Map::from_iter([("project_tree".to_string(), t.clone())])));
    // What is directly under each root checked, nothing written (the screen's tab for kinds, as
    // it is written).
    let dry = match given.get("dryRun") {
        None | Some(Value::Bool(false)) => false,
        Some(Value::Bool(true)) => true,
        Some(_) => return Err(text!("projects.update.dryRunShape")),
    };
    if dry {
        if let Some(other) = given
            .keys()
            .find(|k| !["project_tree", "renames", "dryRun"].contains(&k.as_str()))
        {
            return Err(text!("projects.update.dryRunOnly", other));
        }
        let mut out = Map::new();
        out.insert("project".into(), scope.project_name.clone().into());
        out.insert("dryRun".into(), true.into());
        if let Value::Object(said) = super::own_kinds::set_kinds(
            tx,
            &scope.project,
            tree.as_ref(),
            given.get("renames"),
            true,
            None,
        )? {
            for key in ["ok", "problems", "projectTree"] {
                if let Some(v) = said.get(key) {
                    out.insert(key.into(), v.clone());
                }
            }
        }
        return Ok(Value::Object(out));
    }
    let tree_now = (given.contains_key("project_tree") || given.contains_key("renames"))
        .then(|| {
            super::own_kinds::set_kinds(
                tx,
                &scope.project,
                tree.as_ref(),
                given.get("renames"),
                false,
                Some(&keyed!("projects.line.treeChanged")),
            )
        })
        .transpose()?;
    let current = Layout::read(tx.conn(), &scope.project)?;
    let layout = given
        .get("layout")
        .map(|raw| layout_over(raw, &current))
        .transpose()?;
    let rules = given.get("rules").map(rules_given).transpose()?;
    if let Some(raw) = given.get("name") {
        let raw = raw
            .as_str()
            .ok_or_else(|| text!("projects.update.nameShape"))?;
        rename(tx, &scope.project, raw)?;
    }
    if let Some(raw) = given.get("description") {
        let raw = raw
            .as_str()
            .ok_or_else(|| text!("projects.update.descriptionShape"))?;
        set_description(tx, &scope.project, raw)?;
    }
    let rules_now = match &rules {
        Some(changes) => Some(set_writing_rules(tx, &scope.project, changes)?),
        None => None,
    };
    if let Some(layout) = &layout {
        set_layout(tx, &scope.project, layout)?;
    }
    if let Some(raw) = given.get("slug") {
        let raw = raw
            .as_str()
            .ok_or_else(|| text!("projects.update.slugShape"))?;
        set_slug(tx, &scope.project, raw)?;
    }
    if let Some(raw) = given.get("exportLanguage") {
        set_export_language(tx, &scope.project, raw)?;
    }
    let (name, description, slug, language): (String, String, String, String) = tx
        .conn()
        .query_row(
            "SELECT name, description, slug, export_language FROM projects WHERE id = ?",
            [&scope.project],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .map_err(db)?;
    let mut out = Map::new();
    out.insert("project".into(), name.into());
    out.insert("description".into(), description.into());
    if given.contains_key("slug") {
        out.insert("slug".into(), slug.into());
    }
    if given.contains_key("exportLanguage") {
        out.insert("exportLanguage".into(), language.into());
    }
    out.insert(
        "layout".into(),
        Layout::read(tx.conn(), &scope.project)?.view(),
    );
    if let Some(rules) = rules_now {
        out.insert(
            "rules".into(),
            Value::Object(rules.into_iter().map(|(k, v)| (k, v.into())).collect()),
        );
    }
    if let Some(Value::Object(said)) = tree_now
        && let Some(tree) = said.get("projectTree")
    {
        out.insert("projectTree".into(), tree.clone());
    }
    Ok(Value::Object(out))
}

/// The project renamed (`renameProject`). The slug stays: it is changed by itself (`set_slug`).
/// A line in main's history: the project's settings are not a branch's.
pub(crate) fn rename(tx: &mut Tx, project: &str, raw: &str) -> Result<(), String> {
    let name = required(raw, &text!("projects.label.projectName"), 120)?;
    unique_name(tx.conn(), &name, Some(project))?;
    tx.conn()
        .execute(
            "UPDATE projects SET name = ? WHERE id = ?",
            params![name, project],
        )
        .map_err(db)?;
    tx.line(
        project,
        "",
        "project.update",
        "project",
        Some(project),
        keyed!("projects.line.renamed", name),
    )
}

/// The language the project's exports say their own words in (`schema::EXPORT_LANGUAGE`): one of
/// `text::LANGUAGES`, never `system`, so that whoever writes the project out on whatever machine gets
/// the same files. Nothing when it is the language now; a line in main's history otherwise.
pub(crate) fn set_export_language(tx: &mut Tx, project: &str, raw: &Value) -> Result<(), String> {
    let choices = crate::text::List::of(crate::text::LANGUAGES);
    let language = raw
        .as_str()
        .filter(|l| crate::text::LANGUAGES.contains(l))
        .ok_or_else(|| text!("projects.exportLanguage.refused", given = raw, choices))?;
    let was: String = tx
        .conn()
        .query_row(
            "SELECT export_language FROM projects WHERE id = ?",
            [project],
            |r| r.get(0),
        )
        .map_err(db)?;
    if was == language {
        return Ok(());
    }
    tx.conn()
        .execute(
            "UPDATE projects SET export_language = ? WHERE id = ?",
            params![language, project],
        )
        .map_err(db)?;
    tx.line(
        project,
        "",
        "project.update",
        "project",
        Some(project),
        keyed!("projects.line.exportLanguageChanged", was, language),
    )
}

/// The project's slug changed: what is given
/// made one as a new project's is (`slugify`), refused when nothing of it is left or another project
/// has it, nothing when it is the slug now. What each export kept in its default folder goes with
/// it (`files::slug_changed`). A line in main's history.
pub(crate) fn set_slug(tx: &mut Tx, project: &str, raw: &str) -> Result<(), String> {
    if !raw.chars().any(|c| c.is_alphabetic() || c.is_numeric()) {
        return Err(text!("projects.slug.noLetters"));
    }
    let slug = crate::write::slugify(raw);
    if slug.chars().count() > 120 {
        return Err(text!("projects.slug.tooLong"));
    }
    let was: String = tx
        .conn()
        .query_row("SELECT slug FROM projects WHERE id = ?", [project], |r| {
            r.get(0)
        })
        .map_err(db)?;
    if was == slug {
        return Ok(());
    }
    let taken: Option<(String, String)> = tx
        .conn()
        .query_row(
            "SELECT id, name FROM projects WHERE slug = ? AND id <> ?",
            params![slug, project],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .map_err(db)?;
    if let Some((id, other)) = taken {
        // Not the name of a project the key may not see.
        if !crate::access::sees(tx.conn(), &id)? {
            return Err(text!("projects.slug.unavailable", slug));
        }
        return Err(text!("projects.slug.taken", slug, other));
    }
    tx.conn()
        .execute(
            "UPDATE projects SET slug = ? WHERE id = ?",
            params![slug, project],
        )
        .map_err(db)?;
    super::files::slug_changed(tx, project, &was, &slug)?;
    tx.line(
        project,
        "",
        "project.update",
        "project",
        Some(project),
        keyed!("projects.line.slugChanged", was, slug),
    )
}

/// The project's own rule for each place it has set (`writingRules`): the text of each,
/// by the place's name.
pub(crate) fn writing_rules(
    conn: &Connection,
    project: &str,
) -> Result<BTreeMap<String, String>, String> {
    let text: Option<String> = conn
        .query_row("SELECT rules FROM projects WHERE id = ?", [project], |r| {
            r.get(0)
        })
        .map_err(sql)?;
    let Some(Value::Object(rules)) = text.and_then(|t| serde_json::from_str::<Value>(&t).ok())
    else {
        return Ok(BTreeMap::new());
    };
    Ok(rules
        .into_iter()
        .filter_map(|(place, said)| said.as_str().map(|s| (place, s.to_string())))
        .collect())
}

/// `rules` of project_update: the text for each place it names, in order; an empty text (or
/// null) lets the place take the app's rule again.
fn rules_given(raw: &Value) -> Result<Vec<(String, String)>, String> {
    let shape = text!("projects.rules.shape");
    let Some(given) = raw.as_object() else {
        return Err(shape);
    };
    if given.is_empty() {
        return Err(text!("projects.rules.noPlaces", shape));
    }
    given
        .iter()
        .map(|(place, said)| match said {
            Value::String(text) => Ok((place.clone(), text.clone())),
            Value::Null => Ok((place.clone(), String::new())),
            _ => Err(text!("projects.rules.notText", place)),
        })
        .collect()
}

/// The project's rules at the places given, set — or, empty, let go so the place takes the
/// app's rule again (`setWritingRules`, `WritingRules.problem`). A person's to change
/// under main's lock too. One line in main's history. Returns the project's rules now.
pub(crate) fn set_writing_rules(
    tx: &mut Tx,
    project: &str,
    changes: &[(String, String)],
) -> Result<BTreeMap<String, String>, String> {
    let mut rules = writing_rules(tx.conn(), project)?;
    for (place, said) in changes {
        if said.trim().is_empty() {
            rules.remove(place);
        } else {
            rules.insert(place.clone(), said.clone());
        }
    }
    rules.retain(|_, text| !text.trim().is_empty());
    let places: Vec<String> = super::rules::places().into_iter().map(|p| p.name).collect();
    let unknown: Vec<&str> = rules
        .keys()
        .filter(|k| !places.contains(k))
        .map(String::as_str)
        .collect();
    if !unknown.is_empty() {
        return Err(text!(
            "projects.rules.unknownPlaces",
            unknown = crate::text::list(&unknown),
            places = places.join(" / ")
        ));
    }
    if let Some((long, _)) = rules.iter().find(|(_, text)| text.chars().count() > 4000) {
        return Err(text!("projects.rules.tooLong", long));
    }
    let stored = (!rules.is_empty()).then(|| {
        crate::ojson::compact(&Value::Object(
            rules
                .iter()
                .map(|(k, v)| (k.clone(), Value::from(v.clone())))
                .collect(),
        ))
    });
    tx.conn()
        .execute(
            "UPDATE projects SET rules = ? WHERE id = ?",
            params![stored, project],
        )
        .map_err(db)?;
    tx.line(
        project,
        "",
        "project.update",
        "project",
        Some(project),
        keyed!("projects.line.rulesUpdated", count = rules.len()),
    )?;
    Ok(rules)
}

/// The project's description, replaced whole (`updateProjectDescription`). The slug stays: it
/// is what files written out are named by.
fn set_description(tx: &mut Tx, project: &str, raw: &str) -> Result<(), String> {
    let description = text(raw, &text!("common.label.description"), 2000)?;
    tx.conn()
        .execute(
            "UPDATE projects SET description = ? WHERE id = ?",
            params![description, project],
        )
        .map_err(db)?;
    tx.line(
        project,
        "",
        "project.update",
        "project",
        Some(project),
        keyed!("projects.line.descriptionUpdated"),
    )
}

/// A number as a whole one, toward zero (`Arguments.whole`).
fn whole(n: f64) -> i64 {
    n.trunc() as i64
}

/// How many rows a card shows, checked (`CardSize.rows`).
fn card_rows(value: i64) -> Result<i64, String> {
    if !(3..=40).contains(&value) {
        return Err(text!("projects.layout.rowsRange", value));
    }
    Ok(value)
}

/// A card's width from `auto` or a number; why not, when neither (`CardSize.width`).
fn card_width(raw: &str) -> Result<Option<i64>, String> {
    let trimmed = raw.trim_matches([' ', '\t']).to_lowercase();
    if trimmed == "auto" {
        return Ok(None);
    }
    match trimmed.replace("px", "").parse::<i64>() {
        Ok(n) if (200..=800).contains(&n) => Ok(Some(n)),
        _ => Err(text!("projects.layout.widthRange", raw)),
    }
}

/// The grid units kept clear around a card, checked (`LayoutSettings.margin`).
fn margin(value: i64) -> Result<i64, String> {
    if !(0..=10).contains(&value) {
        return Err(text!("projects.layout.marginRange", value));
    }
    Ok(value)
}

/// `layout` over the layout there is now: only what it names changes
/// (`layoutSettings(_:over:)`).
fn layout_over(given: &Value, current: &Layout) -> Result<Layout, String> {
    let Some(object) = given.as_object() else {
        return Err(text!("projects.layout.shape"));
    };
    let known = ["cardRows", "cardWidth", "margin", "docView"];
    if let Some(stray) = object.keys().find(|k| !known.contains(&k.as_str())) {
        return Err(text!(
            "projects.layout.unknownKey",
            stray,
            known = known.join(" / ")
        ));
    }
    let mut out = current.clone();
    match object.get("cardRows") {
        None => {}
        Some(Value::Number(n)) => out.rows = card_rows(whole(n.as_f64().unwrap_or_default()))?,
        Some(_) => return Err(text!("projects.layout.cardRowsShape")),
    }
    match object.get("cardWidth") {
        None => {}
        Some(Value::String(raw)) => out.width = card_width(raw)?,
        Some(Value::Number(n)) => {
            out.width = card_width(&whole(n.as_f64().unwrap_or_default()).to_string())?
        }
        Some(_) => return Err(text!("projects.layout.cardWidthShape")),
    }
    match object.get("margin") {
        None => {}
        Some(Value::Number(n)) => out.margin = margin(whole(n.as_f64().unwrap_or_default()))?,
        Some(_) => return Err(text!("projects.layout.marginShape")),
    }
    match object.get("docView") {
        None => {}
        Some(Value::String(raw)) if raw == "rows" || raw == "document" => {
            out.doc_view = raw.clone()
        }
        Some(_) => {
            return Err(text!("projects.layout.docViewShape"));
        }
    }
    Ok(out)
}

/// How big the project's cards are and the margin around them (`setDefaultSettings`).
/// Where the cards are does not change: they are in grid units.
fn set_layout(tx: &mut Tx, project: &str, layout: &Layout) -> Result<(), String> {
    card_rows(layout.rows)?;
    if let Some(width) = layout.width {
        card_width(&width.to_string())?;
    }
    margin(layout.margin)?;
    if layout.doc_view != "rows" && layout.doc_view != "document" {
        return Err(text!(
            "projects.layout.docViewValue",
            value = layout.doc_view
        ));
    }
    let width = layout
        .width
        .map_or_else(|| "auto".to_string(), |w| w.to_string());
    tx.conn()
        .execute(
            "INSERT INTO default_view (project_id, nested, rows, width, grid_gap, doc_view) VALUES (?, 0, ?, ?, ?, ?)
             ON CONFLICT(project_id) DO UPDATE SET rows = excluded.rows, width = excluded.width,
                                                grid_gap = excluded.grid_gap, doc_view = excluded.doc_view",
            params![project, layout.rows, width, layout.margin, layout.doc_view],
        )
        .map_err(db)?;
    tx.line(
        project,
        "",
        "layout.update",
        "layout",
        None,
        keyed!(
            "projects.line.layoutUpdated",
            rows = layout.rows,
            width,
            margin = layout.margin,
            view = if layout.doc_view == "rows" {
                keyed!("projects.layout.viewRows")
            } else {
                keyed!("projects.layout.viewDocument")
            }
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::sections::tests::{call, script_core};
    use serde_json::json;

    #[test]
    fn a_new_project_has_its_sections_and_its_board() {
        let core = script_core();
        let made = call(
            &core,
            "project_create",
            json!({"project": "新しいプロジェクト", "value": {"description": "説明"}}),
        )
        .unwrap();
        assert_eq!(
            made,
            json!({"created": "新しいプロジェクト", "slug": "新しいプロジェクト"})
        );
        let rows = |branch: &str| {
            call(
                &core,
                "section_list",
                json!({"project": "新しいプロジェクト", "branch": branch}),
            )
            .unwrap()["sections"]
                .as_array()
                .unwrap()
                .iter()
                .map(|s| (s["section"].clone(), s["type"].clone()))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            rows("main"),
            vec![
                (json!("Main"), Value::Null),
                (json!("Docs"), json!("document")),
                (json!("Schemas"), json!("schema")),
                (json!("Definitions"), json!("definition")),
                (json!("Charts"), json!("chart")),
                (json!("Free"), json!("free")),
            ]
        );
        assert_eq!(
            rows("_project_board_"),
            vec![
                (json!("Board"), Value::Null),
                (json!("Goals"), json!("goals")),
                (json!("Goals/Complete"), json!("complete")),
                (json!("Goals/Cancelled"), json!("cancelled"))
            ]
        );
        let read = call(
            &core,
            "project_read",
            json!({"project": "新しいプロジェクト"}),
        )
        .unwrap();
        assert_eq!(read["layout"]["margin"], 1);
        assert_eq!(read["description"], "説明");
        // The same name again is refused; a name of the same slug
        // gives the slug a number.
        assert!(
            call(
                &core,
                "project_create",
                json!({"project": "新しいプロジェクト"}),
            )
            .unwrap_err()
            .contains("はもうあります"),
        );
        let again = call(
            &core,
            "project_create",
            json!({"project": "新しいプロジェクト!"}),
        )
        .unwrap();
        assert_eq!(again["slug"], "新しいプロジェクト-2");
        assert_eq!(
            call(&core, "project_create", json!({"project": "  "})).unwrap_err(),
            "プロジェクト名 を入力してください"
        );
        let lines: Vec<(String, String)> = {
            let store = core.lock();
            let mut stmt = store
                .conn
                .prepare(
                    "SELECT c.branch_id, l.summary FROM commit_lines l JOIN commits c ON c.id = l.commit_id
                      JOIN projects p ON p.id = c.project_id WHERE p.slug = '新しいプロジェクト' ORDER BY c.branch_id, l.idx",
                )
                .unwrap();
            stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
                .unwrap()
                .collect::<Result<_, _>>()
                .unwrap()
        };
        let said = |tree: &str| -> Vec<&str> {
            lines
                .iter()
                .filter(|l| l.0 == tree)
                .map(|l| l.1.as_str())
                .collect()
        };
        assert_eq!(
            said(""),
            vec![
                "セクション Docs を作成",
                "セクション Schemas を作成",
                "セクション Definitions を作成",
                "セクション Charts を作成",
                "セクション Free を作成",
                "プロジェクト「新しいプロジェクト」を作成",
            ]
        );
        assert_eq!(
            said(BOARD),
            vec!["セクション Goals を作成", "プロジェクトボードを作成"]
        );
    }

    #[test]
    fn a_layout_changes_only_what_it_names() {
        let current = Layout::default();
        let out = layout_over(&serde_json::json!({"cardRows": 12}), &current).unwrap();
        assert_eq!(out.rows, 12);
        assert_eq!(out.width, Some(352));
        let auto = layout_over(&serde_json::json!({"cardWidth": "auto"}), &current).unwrap();
        assert_eq!(auto.width, None);
        assert_eq!(
            layout_over(&serde_json::json!({"cardRows": 2}), &current).unwrap_err(),
            "行数は 3〜40 です（2 ではなく）"
        );
        assert_eq!(
            layout_over(&serde_json::json!({"rows": 2}), &current).unwrap_err(),
            "layout に rows はありません。使えるのは cardRows / cardWidth / margin / docView です"
        );
    }
}
