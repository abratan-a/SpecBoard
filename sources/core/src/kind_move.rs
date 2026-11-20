//! The built-in kind of card `formatdoc` named `structdoc` (the store's mark
//! `core_0031_struct_doc`, a project file before version 35), as the screen calls it a structured
//! document.
//!
//! Every card of the kind of before takes the name of now wherever a card is kept: in every tree's
//! documents, their bases and the column `kind` (main, the board, each branch), what a branch keeps
//! frozen of main, and the history's rows — a long one read whole from its pieces and kept in pieces
//! again (`write::chunks`), the pieces no row points at any more let go of. None is left of the name
//! of before: no kind of card has it. What the project's kinds, exports and writing rules name it by
//! goes by the name of now, a branch's prints of main's cards and the open requests to merge still
//! as their branch is are taken again, and each tree that moved any card says so in one commit of
//! one record.
//!
//! A project's own kind of card named `structdoc` stood where the built-in one is now, and two kinds
//! of one name break every write of the project's kinds: it is kept as `structdoc_project` (numbered
//! when the project has that name already), its cards and what names it with it.

use crate::store::{parse, sql};
use crate::write::chunks::{self, Before};
use rusqlite::{Connection, params};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, HashMap, HashSet};

type Obj = Map<String, Value>;

/// The built-in kind of before.
const BEFORE: &str = "formatdoc";

/// The built-in kind of now.
const NOW: &str = "structdoc";

/// The name a project's own kind named `structdoc` is kept by.
const OWN_KEPT: &str = "structdoc_project";

/// How the kinds of card of a project are named anew: each `(from, to)`, all at once — a project's
/// own `structdoc` goes to its name of now before the built-in one takes the name.
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

    let renames = project_moved(conn, project)?;
    let moved = cards_moved(conn, project, &renames)?;
    frozen_moved(conn, project, &renames)?;
    history_moved(conn, project, &renames)?;
    for (tree, said) in &moved.said {
        let lines: Vec<crate::text::Keyed> = renames
            .0
            .iter()
            .filter_map(|(from, to)| {
                let cards = said.get(from)?;
                Some(keyed!(
                    "kindMove.line.renamed",
                    from = from.clone(),
                    to = to.clone(),
                    count = cards.len(),
                    cards = crate::text::List::of(cards.iter().map(String::as_str))
                ))
            })
            .collect();
        crate::line_move::record(
            conn,
            project,
            tree,
            &crate::write::history::migrated(keyed!("history.migrate.structDoc")),
            &lines,
        )?;
    }
    if moved.any {
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

/// The project's row moved — its kinds, its exports and its writing rules: how its kinds of card
/// are named anew.
fn project_moved(conn: &Connection, project: &str) -> Result<Renames, String> {
    let (kinds, exports, rules): (Option<String>, Option<String>, Option<String>) = conn
        .query_row(
            "SELECT kinds, exports, rules FROM projects WHERE id = ?",
            [project],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .map_err(sql)?;
    let mut renames = Renames(vec![(BEFORE.into(), NOW.into())]);
    if let Some(mut kinds) = parse(kinds.as_deref()) {
        let changed;
        (renames, changed) = kinds_moved(&mut kinds);
        if changed {
            conn.execute(
                "UPDATE projects SET kinds = ? WHERE id = ?",
                params![Value::Object(kinds).to_string(), project],
            )
            .map_err(sql)?;
        }
    }
    if let Some(mut exports) = exports.and_then(|t| serde_json::from_str::<Value>(&t).ok())
        && exports_moved(&mut exports, &renames)
    {
        conn.execute(
            "UPDATE projects SET exports = ? WHERE id = ?",
            params![exports.to_string(), project],
        )
        .map_err(sql)?;
    }
    if let Some(mut rules) = parse(rules.as_deref())
        && rules_moved(&mut rules)
    {
        conn.execute(
            "UPDATE projects SET rules = ? WHERE id = ?",
            params![Value::Object(rules).to_string(), project],
        )
        .map_err(sql)?;
    }
    Ok(renames)
}

/// The project's kinds (`projects.kinds`) with the kinds of card named anew wherever they name one:
/// its own kinds of card, and in the definitions of its kinds of card, section and line what one
/// holds (`holds`), takes after (`extends`) and names as kinds (`kinds`: a field that points at
/// cards, the ends of a line) — at any depth, in fields, rows and groups —, and what it wrote over
/// the built-in kind by its name (`icons`, `contexts`). How they were named, and whether anything in
/// them changed.
fn kinds_moved(kinds: &mut Obj) -> (Renames, bool) {
    let mut changed = false;
    let mut renames = vec![(BEFORE.to_string(), NOW.to_string())];
    if let Some(cards) = kinds.get_mut("cards").and_then(Value::as_array_mut)
        && let Some(at) = cards.iter().position(|c| c["kind"] == NOW)
    {
        let taken = |name: &str| cards.iter().any(|c| c["kind"] == name);
        let mut name = OWN_KEPT.to_string();
        let mut n = 2;
        while taken(&name) {
            name = format!("{OWN_KEPT}_{n}");
            n += 1;
        }
        cards[at]["kind"] = name.clone().into();
        renames.insert(0, (NOW.to_string(), name));
        changed = true;
    }
    let renames = Renames(renames);
    for key in ["cards", "sections", "lines"] {
        if let Some(definitions) = kinds.get_mut(key) {
            changed |= named_in(definitions, &renames);
        }
    }
    // What the project wrote over the built-in kind, by its name: only a built-in kind is named
    // there, each where it was.
    for key in ["icons", "contexts"] {
        if let Some(over) = kinds.get_mut(key).and_then(Value::as_object_mut)
            && over.contains_key(BEFORE)
            && !over.contains_key(NOW)
        {
            let named: Obj = std::mem::take(over)
                .into_iter()
                .map(|(k, v)| (if k == BEFORE { NOW.to_string() } else { k }, v))
                .collect();
            *over = named;
            changed = true;
        }
    }
    (renames, changed)
}

/// A definition with the kinds of card it names named anew, at any depth: the names in `holds`
/// and `kinds`, and `extends`. Whether any was.
fn named_in(value: &mut Value, renames: &Renames) -> bool {
    let mut changed = false;
    match value {
        Value::Object(map) => {
            for (key, inner) in map.iter_mut() {
                match (key.as_str(), inner) {
                    ("holds" | "kinds", Value::Array(names)) => {
                        for name in names {
                            if let Some(to) = name.as_str().and_then(|k| renames.of(k)) {
                                *name = to.into();
                                changed = true;
                            }
                        }
                    }
                    ("extends", Value::String(name)) => {
                        if let Some(to) = renames.of(name) {
                            *name = to.to_string();
                            changed = true;
                        }
                    }
                    (_, inner) => changed |= named_in(inner, renames),
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                changed |= named_in(item, renames);
            }
        }
        _ => {}
    }
    changed
}

/// The project's exports (`projects.exports`) with the kinds of card each entry writes named anew:
/// whether any was.
fn exports_moved(exports: &mut Value, renames: &Renames) -> bool {
    let mut changed = false;
    match exports {
        Value::Object(map) => {
            for (key, value) in map.iter_mut() {
                if key == "kinds"
                    && let Value::Array(kinds) = value
                {
                    for kind in kinds {
                        if let Some(to) = kind.as_str().and_then(|k| renames.of(k)) {
                            *kind = to.into();
                            changed = true;
                        }
                    }
                } else {
                    changed |= exports_moved(value, renames);
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                changed |= exports_moved(item, renames);
            }
        }
        _ => {}
    }
    changed
}

/// The project's writing rules (`projects.rules`) with the place of the built-in kind's description
/// named anew, where it was: whether it was.
fn rules_moved(rules: &mut Obj) -> bool {
    let before = format!("card.{BEFORE}.description");
    let now = format!("card.{NOW}.description");
    if !rules.contains_key(&before) || rules.contains_key(&now) {
        return false;
    }
    let named: Obj = std::mem::take(rules)
        .into_iter()
        .map(|(k, v)| (if k == before { now.clone() } else { k }, v))
        .collect();
    *rules = named;
    true
}

/// A card's document with its kind named anew, in the kinds of now; none when its kind stays. Only
/// the card's own kind: a row of a structured document has a kind of its own.
fn doc_moved(doc: &Obj, renames: &Renames) -> Option<Obj> {
    let to = renames.of(doc.get("kind")?.as_str()?)?;
    let mut out = doc.clone();
    out.insert("kind".into(), to.into());
    out.insert("v".into(), crate::kinds::builtin().version.into());
    Some(out)
}

/// A document kept as text, moved: none when it stays.
fn text_moved(text: &str, renames: &Renames) -> Option<String> {
    doc_moved(&parse(Some(text))?, renames).map(|d| Value::Object(d).to_string())
}

/// What the cards' move did: whether any document moved, and for each tree (main's the empty one)
/// that moved a card that is there, its names by the kind of before.
#[derive(Default)]
struct Moved {
    any: bool,
    said: BTreeMap<String, HashMap<String, Vec<String>>>,
}

/// Every card of the project in every tree with its kind named anew — its document, its base and
/// its column — and a branch's prints of main's cards taken again where they were of main's
/// document as it was.
fn cards_moved(conn: &Connection, project: &str, renames: &Renames) -> Result<Moved, String> {
    type Row = (
        i64,
        String,
        String,
        String,
        bool,
        Option<String>,
        Option<String>,
    );
    let rows: Vec<Row> = {
        let mut stmt = conn
            .prepare(
                "SELECT rowid, branch_id, id, kind, removed, doc, base FROM card_docs WHERE project_id = ?",
            )
            .map_err(sql)?;
        stmt.query_map([project], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get::<_, i64>(4)? != 0,
                r.get(5)?,
                r.get(6)?,
            ))
        })
        .map_err(sql)?
        .collect::<Result<_, _>>()
        .map_err(sql)?
    };
    let mut moved = Moved::default();
    // main's documents that moved, as they were and are: what a branch's print was taken of.
    let mut main_changed: HashMap<String, (String, String)> = HashMap::new();
    for (rowid, tree, id, kind, removed, doc, base) in rows {
        let kind_to = renames.of(&kind).map(str::to_string);
        let parsed = parse(doc.as_deref());
        let doc_to = parsed.as_ref().and_then(|d| doc_moved(d, renames));
        let base_to = base.as_deref().and_then(|b| text_moved(b, renames));
        if kind_to.is_none() && doc_to.is_none() && base_to.is_none() {
            continue;
        }
        moved.any = true;
        if let Some(to) = &kind_to {
            conn.execute(
                "UPDATE card_docs SET kind = ? WHERE rowid = ?",
                params![to, rowid],
            )
            .map_err(sql)?;
        }
        if let Some(next) = &doc_to {
            let after = Value::Object(next.clone()).to_string();
            conn.execute(
                "UPDATE card_docs SET doc = ? WHERE rowid = ?",
                params![after, rowid],
            )
            .map_err(sql)?;
            if tree.is_empty()
                && let Some(before) = &doc
            {
                main_changed.insert(id.clone(), (before.clone(), after));
            }
            if !removed {
                let name = next
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or(&id)
                    .to_string();
                moved
                    .said
                    .entry(tree.clone())
                    .or_default()
                    .entry(kind_of_before(&parsed, &kind))
                    .or_default()
                    .push(name);
            }
        }
        if let Some(next) = base_to {
            conn.execute(
                "UPDATE card_docs SET base = ? WHERE rowid = ?",
                params![next, rowid],
            )
            .map_err(sql)?;
        }
    }
    // A branch's prints of main's cards: what it remembered is what main had.
    if !main_changed.is_empty() {
        let prints: Vec<(String, String, String)> = {
            let mut stmt = conn
                .prepare(
                    "SELECT p.branch_id, p.card_id, p.print FROM branch_prints p JOIN branches b ON b.id = p.branch_id WHERE b.project_id = ?",
                )
                .map_err(sql)?;
            stmt.query_map([project], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
                .map_err(sql)?
                .collect::<Result<_, _>>()
                .map_err(sql)?
        };
        for (branch, card, print) in prints {
            if let Some((before, after)) = main_changed.get(&card)
                && crate::schema::print(before) == print
            {
                conn.execute(
                    "UPDATE branch_prints SET print = ? WHERE branch_id = ? AND card_id = ?",
                    params![crate::schema::print(after), branch, card],
                )
                .map_err(sql)?;
            }
        }
    }
    Ok(moved)
}

/// The kind a card was of before it moved: its document's, else its column's.
fn kind_of_before(doc: &Option<Obj>, column: &str) -> String {
    doc.as_ref()
        .and_then(|d| d.get("kind"))
        .and_then(Value::as_str)
        .unwrap_or(column)
        .to_string()
}

/// What each branch keeps frozen of main's cards, with its kind named anew.
fn frozen_moved(conn: &Connection, project: &str, renames: &Renames) -> Result<(), String> {
    let rows: Vec<(i64, String)> = {
        let mut stmt = conn
            .prepare(
                "SELECT rowid, doc FROM branch_frozen WHERE project_id = ? AND tbl = 'card_docs' AND doc IS NOT NULL",
            )
            .map_err(sql)?;
        stmt.query_map([project], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(sql)?
            .collect::<Result<_, _>>()
            .map_err(sql)?
    };
    for (rowid, doc) in rows {
        if let Some(next) = text_moved(&doc, renames) {
            conn.execute(
                "UPDATE branch_frozen SET doc = ? WHERE rowid = ?",
                params![next, rowid],
            )
            .map_err(sql)?;
        }
    }
    Ok(())
}

/// The history's rows of cards and of what branches froze of them, with the kind named anew, so
/// what a commit kept compares with what is written now and puts back a kind there is. A long row
/// is read whole from its pieces and kept in pieces again; the pieces it pointed at that no row
/// points at any more are let go of. A row that cannot be read stays as it is: it is refused to who
/// reads it, as it was.
fn history_moved(conn: &Connection, project: &str, renames: &Renames) -> Result<(), String> {
    let rows: Vec<(i64, String, Before)> = {
        let mut stmt = conn
            .prepare(
                "SELECT r.rowid, r.tbl, r.before FROM commit_rows r JOIN commits c ON c.id = r.commit_id
                  WHERE c.project_id = ? AND r.tbl IN ('card_docs', 'frozen:card_docs') AND r.before IS NOT NULL",
            )
            .map_err(sql)?;
        stmt.query_map([project], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .map_err(sql)?
            .collect::<Result<_, _>>()
            .map_err(sql)?
    };
    let mut replaced: HashSet<i64> = HashSet::new();
    for (rowid, tbl, before) in rows {
        let pieces: Vec<i64> = match &before {
            Before::Pieces(list) => chunks::listed(list)
                .unwrap_or_default()
                .into_iter()
                .map(|(id, _)| id)
                .collect(),
            _ => Vec::new(),
        };
        // Read only where it may name a kind named anew.
        if let Before::Text(text) = &before
            && !renames
                .0
                .iter()
                .any(|(from, _)| text.contains(from.as_str()))
        {
            continue;
        }
        let Ok(Some(text)) = before.text(conn) else {
            continue;
        };
        let Some(mut columns) = parse(Some(&text)) else {
            continue;
        };
        let mut changed = false;
        if tbl == "card_docs"
            && let Some(to) = columns
                .get("kind")
                .and_then(Value::as_str)
                .and_then(|k| renames.of(k))
        {
            columns.insert("kind".into(), to.into());
            changed = true;
        }
        for key in ["doc", "base"] {
            if let Some(next) = columns
                .get(key)
                .and_then(Value::as_str)
                .and_then(|t| text_moved(t, renames))
            {
                columns.insert(key.into(), next.into());
                changed = true;
            }
        }
        if !changed {
            continue;
        }
        let kept = chunks::pack(conn, project, &Value::Object(columns).to_string())?;
        conn.execute(
            "UPDATE commit_rows SET before = ? WHERE rowid = ?",
            params![kept, rowid],
        )
        .map_err(sql)?;
        replaced.extend(pieces);
    }
    chunks::let_go(conn, project, &replaced)?;
    Ok(())
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
            "sections": [{ "section": "papers", "holds": ["formatdoc", "note"] }],
            "lines": [
                { "line": "cites", "from": { "kinds": ["formatdoc"] }, "to": { "kinds": ["spec", "formatdoc"] } },
                { "line": "quotes", "from": [{ "kinds": ["note"] }, { "kinds": ["formatdoc"] }] }
            ],
            "icons": { "note": { "color": "#112233" }, "formatdoc": { "name": "star" } },
            "contexts": { "formatdoc": { "use_when": "例" } }
        }));
        assert!(changed);
        assert_eq!(renames, [("formatdoc".into(), "structdoc".into())]);
        assert_eq!(kinds["sections"][0]["holds"], json!(["structdoc", "note"]));
        assert_eq!(kinds["lines"][0]["from"]["kinds"], json!(["structdoc"]));
        assert_eq!(
            kinds["lines"][0]["to"]["kinds"],
            json!(["spec", "structdoc"])
        );
        assert_eq!(kinds["lines"][1]["from"][1]["kinds"], json!(["structdoc"]));
        // The order the project wrote them in stays.
        let icons: Vec<&String> = kinds["icons"].as_object().unwrap().keys().collect();
        assert_eq!(icons, ["note", "structdoc"]);
        assert_eq!(kinds["contexts"]["structdoc"]["use_when"], "例");
        // Moved once: nothing more the second time.
        let (_, _, again) = moved(kinds);
        assert!(!again);
    }

    #[test]
    fn a_projects_own_structdoc_is_kept_by_a_name_of_its_own() {
        let (kinds, renames, changed) = moved(json!({
            "cards": [
                { "kind": "structdoc", "label": "紙" },
                { "kind": "structdoc_project" },
                { "kind": "sheet", "extends": "formatdoc" },
                { "kind": "page", "extends": "structdoc" },
                { "kind": "binder", "holds": ["formatdoc", "structdoc"], "fields": [
                    { "name": "pick", "fieldtype": "group", "fields": [{ "name": "doc", "fieldtype": "card", "kinds": ["formatdoc"] }] }
                ] }
            ],
            "sections": [{ "section": "papers", "holds": ["structdoc", "formatdoc"] }]
        }));
        assert!(changed);
        assert_eq!(
            renames,
            [
                ("structdoc".into(), "structdoc_project_2".into()),
                ("formatdoc".into(), "structdoc".into())
            ]
        );
        assert_eq!(kinds["cards"][0]["kind"], "structdoc_project_2");
        assert_eq!(kinds["cards"][2]["extends"], "structdoc");
        assert_eq!(kinds["cards"][3]["extends"], "structdoc_project_2");
        assert_eq!(
            kinds["cards"][4]["holds"],
            json!(["structdoc", "structdoc_project_2"])
        );
        assert_eq!(
            kinds["cards"][4]["fields"][0]["fields"][0]["kinds"],
            json!(["structdoc"])
        );
        assert_eq!(
            kinds["sections"][0]["holds"],
            json!(["structdoc_project_2", "structdoc"])
        );
    }

    #[test]
    fn a_card_takes_the_kind_of_now_and_its_rows_keep_theirs() {
        let renames = Renames(vec![("formatdoc".into(), "structdoc".into())]);
        let before = r#"{"name":"Spec","kind":"formatdoc","rows":[{"name":"a","kind":"text"}]}"#;
        let v = crate::kinds::builtin().version;
        assert_eq!(
            text_moved(before, &renames).unwrap(),
            format!(
                r#"{{"name":"Spec","kind":"structdoc","rows":[{{"name":"a","kind":"text"}}],"v":{v}}}"#
            )
        );
        assert!(text_moved(r#"{"name":"formatdoc","kind":"note"}"#, &renames).is_none());
    }

    #[test]
    fn an_export_and_a_rule_name_the_kind_of_now() {
        let renames = Renames(vec![("formatdoc".into(), "structdoc".into())]);
        let mut exports = json!({"docs": {"entries": [{"from": "Docs", "to": "docs", "kinds": ["formatdoc", "spec"]}]}});
        assert!(exports_moved(&mut exports, &renames));
        assert_eq!(
            exports["docs"]["entries"][0]["kinds"],
            json!(["structdoc", "spec"])
        );
        assert!(!exports_moved(&mut exports, &renames));
        let mut rules = json!({"card.description": "a", "card.formatdoc.description": "b"})
            .as_object()
            .unwrap()
            .clone();
        assert!(rules_moved(&mut rules));
        let keys: Vec<&String> = rules.keys().collect();
        assert_eq!(keys, ["card.description", "card.structdoc.description"]);
    }
}
