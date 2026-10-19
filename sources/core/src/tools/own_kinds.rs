//! The project's own kinds of card, of section and of line: read with `kind_list`・`kind_read`, written by
//! `kind_create`・`kind_update`・`kind_delete` (`kind_tools`) and the root's sections by
//! `project_update`'s `project_tree` through `set_kinds` (each list given replaces the one there),
//! renamed with `renames` (a kind of card, a field, a kind of row or a row's field, the values
//! moving with it), and checked without writing — what the screen's tab for kinds asks while a
//! definition is written.
//!
//! What a change leaves behind is not refused: a field taken out of a definition (or renamed
//! without its values) stays in the cards as 「定義に無い欄」 (`kinds::strays`), and a card
//! that no longer fits is said by the tree's check (`inspection_list`: `cardProblems`,
//! `lineProblems`, `sectionProblems`). A kind still in use is not let go of.

use crate::store::sql;
use crate::write::{Tx, db};
use rusqlite::{Connection, params};
use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};

/// The lists a project's own kinds are kept in.
const LISTS: [&str; 3] = ["cards", "sections", "lines"];

// MARK: - Read

/// The kinds' definitions as `kind_list`・`kind_read` read them: the built-in ones
/// (`builtin: true`), then the project's own — of cards, sections and lines, and what is directly
/// under each root (`project_tree`), or those `of` names. Of the built-in kinds of card and of line,
/// those the project has a place for; the project's own always, one with no place
/// marked (`noPlace: true`) — written, it would go out of sight before its section is.
pub(crate) fn kinds_view(
    project: &str,
    own: Option<&str>,
    of: Option<&Value>,
) -> Result<Value, String> {
    const READ: [&str; 4] = ["cards", "sections", "lines", "project_tree"];
    let asked: Vec<String> = match of {
        None | Some(Value::Null) => READ.iter().map(|s| s.to_string()).collect(),
        Some(Value::String(one)) => vec![one.clone()],
        Some(Value::Array(many)) => many
            .iter()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect(),
        Some(_) => {
            return Err(text!("ownKinds.view.ofShape"));
        }
    };
    if let Some(wrong) = asked.iter().find(|a| !READ.contains(&a.as_str())) {
        return Err(text!("ownKinds.view.ofUnreadable", wrong));
    }
    let defs = crate::kinds::Defs::of_project(own);
    let own: Value = own
        .and_then(|t| serde_json::from_str(t).ok())
        .unwrap_or(Value::Null);
    let reachable: Vec<String> = defs.reachable().iter().map(|d| d.name.clone()).collect();
    let mut kinds = Map::new();
    for list in LISTS.iter().filter(|l| asked.iter().any(|a| a == *l)) {
        let builtin: Vec<Value> = match *list {
            // The app's own (a section ghost) shown too, never placed.
            "cards" => crate::kinds::builtin()
                .all()
                .iter()
                .filter(|k| !k.is_abstract && defs.has_place(&k.name))
                .filter_map(|k| crate::kinds::builtin_definition(&k.name).cloned())
                .collect(),
            "lines" => crate::kinds::builtin_written(list)
                .iter()
                .filter(|d| {
                    d["line"]
                        .as_str()
                        .and_then(|l| defs.line(l))
                        .is_none_or(|l| defs.line_has_place(l))
                })
                .cloned()
                .collect(),
            _ => crate::kinds::builtin_written(list).to_vec(),
        };
        let key = name_key(list);
        let mut all: Vec<Value> = builtin
            .into_iter()
            .map(|d| {
                let mut d = d.as_object().cloned().unwrap_or_default();
                d.insert("builtin".into(), true.into());
                let name = d
                    .get(key)
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                let name = name.as_str();
                if let Some(context) = context_over(d.get("context"), own["contexts"].get(name)) {
                    d.insert("context".into(), context);
                    d.insert("contextChanged".into(), true.into());
                }
                // A built-in kind of section's mark as the project writes it over (`icons`).
                if *list == "sections"
                    && let Some(icon) =
                        crate::kinds::icon_over(d.get("icon"), own["icons"].get(name))
                {
                    d.insert("icon".into(), icon);
                    d.insert("iconChanged".into(), true.into());
                }
                // What a flow's `via` may be, the project's values too (`vias`): said on the flow,
                // and as the choices of the nodes' field.
                if *list == "cards" {
                    let changed = defs.vias.len() > crate::kinds::builtin_vias().len();
                    if name == "flow" {
                        d.insert("vias".into(), defs.vias.clone().into());
                        if changed {
                            d.insert("viasChanged".into(), true.into());
                        }
                    } else if changed && crate::kinds::VIA_KINDS.contains(&name) {
                        let mut whole = Value::Object(d);
                        vias_written(&mut whole, &defs.vias);
                        return whole;
                    }
                }
                Value::Object(d)
            })
            .collect();
        for written in own[*list].as_array().cloned().unwrap_or_default() {
            let mut d = written.as_object().cloned().unwrap_or_default();
            let placed = match *list {
                "cards" => d["kind"].as_str().is_none_or(|k| defs.has_place(k)),
                "sections" => d["section"]
                    .as_str()
                    .is_none_or(|k| reachable.iter().any(|r| r == k)),
                _ => d["line"]
                    .as_str()
                    .and_then(|l| defs.line(l))
                    .is_none_or(|l| defs.line_has_place(l)),
            };
            if !placed {
                d.insert("noPlace".into(), true.into());
            }
            all.push(Value::Object(d));
        }
        kinds.insert(list.to_string(), Value::Array(all));
    }
    if asked.iter().any(|a| a == "project_tree") {
        kinds.insert("project_tree".into(), defs.tree.json());
    }
    let mut out = Map::new();
    out.insert("project".into(), project.into());
    out.insert("kinds".into(), Value::Object(kinds));
    Ok(Value::Object(out))
}

/// The key a definition of a list is named by: a kind of card's `kind`, of section's `section`,
/// of line's `line`.
pub(crate) fn name_key(list: &str) -> &'static str {
    match list {
        "cards" => "kind",
        "sections" => "section",
        _ => "line",
    }
}

/// A built-in kind's context as the project writes it over (`contexts`): each key it writes in place of the built-in one's. None
/// when the project writes nothing over it.
pub(crate) fn context_over(builtin: Option<&Value>, over: Option<&Value>) -> Option<Value> {
    let over = over?.as_object()?;
    let mut out = builtin
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    for (key, value) in over {
        if value.is_null() {
            out.remove(key);
        } else {
            out.insert(key.clone(), value.clone());
        }
    }
    Some(Value::Object(out))
}

/// A kind of card of the project's own as `guide`'s `about: <name>` says it: its definition as
/// written, the fields and rows it holds whole (with what it takes from its parents), and where
/// how a card of it is written is said (the same for every such kind, so said once there). None
/// when the project has no such kind.
pub(crate) fn kind_guide(project: &str, own: Option<&str>, name: &str) -> Option<Value> {
    let defs = crate::kinds::Defs::of_project(own);
    if !defs.cards().is_own(name) {
        return None;
    }
    let own: Value = own
        .and_then(|t| serde_json::from_str(t).ok())
        .unwrap_or(Value::Null);
    let written = own["cards"]
        .as_array()?
        .iter()
        .find(|k| k["kind"] == name)?
        .clone();
    let whole = crate::kinds::written_whole(own.get("cards"), defs.cards())
        .into_iter()
        .find(|k| k["kind"] == name)?;
    let mut out = Map::new();
    out.insert("kind".into(), name.into());
    out.insert("project".into(), project.into());
    out.insert("definition".into(), written);
    out.insert("fields".into(), whole["fields"].clone());
    if let Some(rows) = whole.get("rows") {
        out.insert("rows".into(), rows.clone());
    }
    out.insert("more".into(), text!("mcp.guide.ownKindsMore").into());
    Some(Value::Object(out))
}

/// The project's own kinds as kept: `{cards, sections, lines}`, each there only when written.
pub(crate) fn kept(conn: &Connection, project: &str) -> Result<Map<String, Value>, String> {
    let current: Option<String> = conn
        .query_row("SELECT kinds FROM projects WHERE id = ?", [project], |r| {
            r.get(0)
        })
        .map_err(db)?;
    Ok(current
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default())
}

// MARK: - Renames

/// One rename `project_update`'s `renames` says: a kind of card of
/// the project's own, a field of one, a kind of row of one, or a field of its rows — the values
/// the cards hold moving with it. `kind` names the kind as the new definitions do.
#[derive(Debug, Clone, PartialEq)]
enum Rename {
    Kind {
        from: String,
        to: String,
    },
    Field {
        kind: String,
        from: String,
        to: String,
    },
    Row {
        kind: String,
        from: String,
        to: String,
    },
    RowField {
        kind: String,
        row: String,
        from: String,
        to: String,
    },
}

impl Rename {
    /// As the history and the answer say it.
    fn said(&self) -> crate::text::Keyed {
        match self {
            Rename::Kind { from, to } => keyed!("ownKinds.rename.kind", from, to),
            Rename::Field { kind, from, to } => keyed!("ownKinds.rename.field", kind, from, to),
            Rename::Row { kind, from, to } => keyed!("ownKinds.rename.row", kind, from, to),
            Rename::RowField {
                kind,
                row,
                from,
                to,
            } => keyed!("ownKinds.rename.rowField", kind, row, from, to),
        }
    }

    fn json(&self, cards: usize) -> Value {
        let mut out = Map::new();
        match self {
            Rename::Kind { from, .. } => {
                out.insert("kind".into(), from.clone().into());
            }
            Rename::Field { kind, from, .. } => {
                out.insert("kind".into(), kind.clone().into());
                out.insert("field".into(), from.clone().into());
            }
            Rename::Row { kind, from, .. } => {
                out.insert("kind".into(), kind.clone().into());
                out.insert("row".into(), from.clone().into());
            }
            Rename::RowField {
                kind, row, from, ..
            } => {
                out.insert("kind".into(), kind.clone().into());
                out.insert("row".into(), row.clone().into());
                out.insert("field".into(), from.clone().into());
            }
        }
        let to = match self {
            Rename::Kind { to, .. }
            | Rename::Field { to, .. }
            | Rename::Row { to, .. }
            | Rename::RowField { to, .. } => to,
        };
        out.insert("to".into(), to.clone().into());
        out.insert("cards".into(), cards.into());
        Value::Object(out)
    }
}

/// How `renames` is written.
fn rename_shape() -> String {
    text!("ownKinds.rename.shape")
}

/// The renames as written, every problem said: of the kinds and of the sections directly under a
/// root (`top_section`, from → to).
fn renames_given(
    raw: Option<&Value>,
    problems: &mut Vec<String>,
) -> (Vec<Rename>, Vec<(String, String)>) {
    let items = match raw {
        None | Some(Value::Null) => return (Vec::new(), Vec::new()),
        Some(Value::Array(items)) => items,
        Some(_) => {
            problems.push(rename_shape());
            return (Vec::new(), Vec::new());
        }
    };
    let mut out = Vec::new();
    let mut tops = Vec::new();
    for (i, item) in items.iter().enumerate() {
        let Some(item) = item.as_object() else {
            problems.push(text!(
                "ownKinds.rename.itemShape",
                i,
                shape = rename_shape()
            ));
            continue;
        };
        let text = |key: &str| {
            item.get(key)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
        };
        if item.contains_key("top_section") {
            if let Some(key) = item
                .keys()
                .find(|k| !["top_section", "to"].contains(&k.as_str()))
            {
                problems.push(text!("ownKinds.rename.topSectionKey", i, key));
                continue;
            }
            match (text("top_section"), text("to")) {
                (Some(from), Some(to)) => tops.push((from, to)),
                _ => problems.push(text!("ownKinds.rename.topSectionNeeded", i)),
            }
            continue;
        }
        if let Some(key) = item
            .keys()
            .find(|k| !["kind", "field", "row", "to"].contains(&k.as_str()))
        {
            problems.push(text!("ownKinds.rename.keyNotAllowed", i, key));
            continue;
        }
        let (Some(kind), Some(to)) = (text("kind"), text("to")) else {
            problems.push(text!(
                "ownKinds.rename.kindToNeeded",
                i,
                shape = rename_shape()
            ));
            continue;
        };
        out.push(match (text("row"), text("field")) {
            (None, None) => Rename::Kind { from: kind, to },
            (None, Some(field)) => Rename::Field {
                kind,
                from: field,
                to,
            },
            (Some(row), None) => Rename::Row {
                kind,
                from: row,
                to,
            },
            (Some(row), Some(field)) => Rename::RowField {
                kind,
                row,
                from: field,
                to,
            },
        });
    }
    (out, tops)
}

/// What a rename asks of the new definitions: the old name gone from them, the new one there.
fn rename_problem(rename: &Rename, defs: &crate::kinds::Defs) -> Option<String> {
    let cards = defs.cards();
    let own = |kind: &str| cards.get(kind).filter(|k| !k.builtin && !k.is_abstract);
    let said = rename.said();
    match rename {
        Rename::Kind { from, to } => {
            if cards.get(from).is_some_and(|k| k.builtin) {
                return Some(text!("ownKinds.rename.isBuiltin", said, from));
            }
            if own(from).is_some() {
                return Some(text!("ownKinds.rename.kindStillDefined", said, from, to));
            }
            if own(to).is_none() {
                return Some(text!("ownKinds.rename.noKindTo", said, to));
            }
            None
        }
        Rename::Field { kind, from, to } => {
            let Some(def) = own(kind) else {
                return Some(text!("ownKinds.rename.noOwnKind", said, kind));
            };
            if def.field(from).is_some() {
                return Some(text!(
                    "ownKinds.rename.fieldStillDefined",
                    said,
                    kind,
                    from,
                    to
                ));
            }
            if def.field(to).is_none() {
                return Some(text!("ownKinds.rename.noFieldTo", said, kind, to));
            }
            None
        }
        Rename::Row { kind, from, to } => {
            let Some(def) = own(kind) else {
                return Some(text!("ownKinds.rename.noOwnKind", said, kind));
            };
            if def.row(from).is_some() {
                return Some(text!(
                    "ownKinds.rename.rowStillDefined",
                    said,
                    kind,
                    from,
                    to
                ));
            }
            if def.row(to).is_none() {
                return Some(text!("ownKinds.rename.noRowTo", said, kind, to));
            }
            None
        }
        Rename::RowField {
            kind,
            row,
            from,
            to,
        } => {
            let Some(def) = own(kind) else {
                return Some(text!("ownKinds.rename.noOwnKind", said, kind));
            };
            let Some(r) = def.row(row) else {
                return Some(text!("ownKinds.rename.noRow", said, kind, row));
            };
            if r.fields.iter().any(|f| &f.name == from) {
                return Some(text!(
                    "ownKinds.rename.rowFieldStillDefined",
                    said,
                    kind,
                    row,
                    from
                ));
            }
            if r.fields.iter().all(|f| &f.name != to) {
                return Some(text!("ownKinds.rename.noRowFieldTo", said, kind, row, to));
            }
            None
        }
    }
}

/// A path inside a card written again for a rename of the name it starts with (`sev`,
/// `sev.x`, `items[0]`): none when it does not start with it.
fn renamed_path(path: &str, from: &str, to: &str) -> Option<String> {
    let (from, to) = (crate::pathtext::quote(from), crate::pathtext::quote(to));
    let (from, to) = (from.as_ref(), to.as_ref());
    if path == from {
        return Some(to.to_string());
    }
    for sep in [".", "["] {
        if let Some(rest) = path.strip_prefix(&format!("{from}{sep}")) {
            return Some(format!("{to}{sep}{rest}"));
        }
    }
    None
}

/// One card's document (or the base it was branched from) with the renames made: what moved,
/// or why it cannot (the new name holds a value already). `ids` are the cards of each kind,
/// by kind, for the lines that land in one.
fn rename_doc(
    doc: &mut Map<String, Value>,
    renames: &[Rename],
    landing_in: &HashMap<String, String>,
) -> Result<Vec<usize>, String> {
    let mut moved = vec![0; renames.len()];
    let name = doc
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    for (i, rename) in renames.iter().enumerate() {
        let kind = doc.get("kind").and_then(Value::as_str).map(str::to_string);
        match rename {
            Rename::Kind { from, to } => {
                if kind.as_deref() == Some(from.as_str()) {
                    doc.insert("kind".into(), to.clone().into());
                    moved[i] += 1;
                }
            }
            Rename::Field { kind: of, from, to } => {
                if kind.as_deref() == Some(of.as_str())
                    && let Some(Value::Object(fields)) = doc.get_mut("fields")
                    && fields.contains_key(from)
                {
                    if fields.contains_key(to) {
                        return Err(text!(
                            "ownKinds.rename.fieldHasValue",
                            said = rename.said(),
                            name,
                            to,
                            from
                        ));
                    }
                    let value = fields.shift_remove(from).unwrap_or(Value::Null);
                    fields.insert(to.clone(), value);
                    moved[i] += 1;
                }
                if kind.as_deref() == Some(of.as_str()) {
                    rename_lines_from(doc, from, to);
                }
            }
            Rename::Row { kind: of, from, to } => {
                if kind.as_deref() == Some(of.as_str())
                    && let Some(Value::Object(rows)) = doc.get_mut("rows")
                    && rows.contains_key(from)
                {
                    if rows.contains_key(to) {
                        return Err(text!(
                            "ownKinds.rename.rowHasRow",
                            said = rename.said(),
                            name,
                            to,
                            from
                        ));
                    }
                    let value = rows.shift_remove(from).unwrap_or(Value::Null);
                    rows.insert(to.clone(), value);
                    moved[i] += 1;
                }
                if kind.as_deref() == Some(of.as_str()) {
                    rename_lines_from(doc, from, to);
                }
            }
            Rename::RowField {
                kind: of,
                row,
                from,
                to,
            } => {
                if kind.as_deref() != Some(of.as_str()) {
                    continue;
                }
                let Some(items) = doc
                    .get_mut("rows")
                    .and_then(|r| r.get_mut(row))
                    .and_then(Value::as_array_mut)
                else {
                    continue;
                };
                let mut any = false;
                for item in items.iter_mut() {
                    if let Some(Value::Object(fields)) = item.get_mut("fields")
                        && fields.contains_key(from)
                    {
                        if fields.contains_key(to) {
                            return Err(text!(
                                "ownKinds.rename.rowFieldHasValue",
                                said = rename.said(),
                                name,
                                to,
                                from
                            ));
                        }
                        let value = fields.shift_remove(from).unwrap_or(Value::Null);
                        fields.insert(to.clone(), value);
                        any = true;
                    }
                }
                if any {
                    moved[i] += 1;
                }
            }
        }
        // The lines of any card that land in a field or a row renamed.
        let (of, from, to) = match rename {
            Rename::Field { kind, from, to } | Rename::Row { kind, from, to } => (kind, from, to),
            _ => continue,
        };
        if let Some(Value::Array(lines)) = doc.get_mut("connections") {
            for line in lines.iter_mut() {
                let lands_in = line
                    .get("to")
                    .and_then(Value::as_str)
                    .and_then(|id| landing_in.get(id));
                if lands_in != Some(of) {
                    continue;
                }
                if let Some(at) = line.get("at").and_then(Value::as_str)
                    && let Some(next) = renamed_path(at, from, to)
                {
                    line["at"] = next.into();
                }
            }
        }
    }
    Ok(moved)
}

/// A card's own lines that leave a field or a row renamed, from it again.
fn rename_lines_from(doc: &mut Map<String, Value>, from: &str, to: &str) {
    if let Some(Value::Array(lines)) = doc.get_mut("connections") {
        for line in lines.iter_mut() {
            if let Some(path) = line.get("from").and_then(Value::as_str)
                && let Some(next) = renamed_path(path, from, to)
            {
                line["from"] = next.into();
            }
        }
    }
}

/// Every card's document in every tree (and the base a branch's copy was branched from),
/// with the renames made: how many cards each moved, or every reason one cannot. Written
/// back when `write`.
fn rename_all(
    conn: &Connection,
    project: &str,
    renames: &[Rename],
    write: bool,
) -> Result<Vec<usize>, Vec<String>> {
    let fail = |e: String| vec![e];
    let mut stmt = conn
        .prepare("SELECT branch_id, id, doc, base FROM card_docs WHERE project_id = ?")
        .map_err(|e| fail(sql(e)))?;
    let rows: Vec<(String, String, Option<String>, Option<String>)> = stmt
        .query_map([project], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
        })
        .map_err(|e| fail(sql(e)))?
        .collect::<Result<_, _>>()
        .map_err(|e| fail(sql(e)))?;
    let parse = |t: &Option<String>| {
        t.as_deref()
            .and_then(|t| serde_json::from_str::<Value>(t).ok())
            .and_then(|v| v.as_object().cloned())
    };
    // The kind of each card, as the renames leave it: what a line lands in.
    let mut landing_in: HashMap<String, String> = HashMap::new();
    for (_, id, doc, base) in &rows {
        if let Some(d) = parse(doc).or_else(|| parse(base)) {
            let mut kind = d
                .get("kind")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            for rename in renames {
                if let Rename::Kind { from, to } = rename
                    && &kind == from
                {
                    kind = to.clone();
                }
            }
            landing_in.insert(id.clone(), kind);
        }
    }
    let mut counted: Vec<HashSet<String>> = vec![HashSet::new(); renames.len()];
    let mut problems = Vec::new();
    for (branch, id, doc, base) in &rows {
        let mut next: [Option<Map<String, Value>>; 2] = [None, None];
        for (slot, text) in [doc, base].into_iter().enumerate() {
            let Some(mut d) = parse(text) else {
                continue;
            };
            let before = d.clone();
            match rename_doc(&mut d, renames, &landing_in) {
                Ok(moved) => {
                    for (i, n) in moved.into_iter().enumerate() {
                        if n > 0 {
                            counted[i].insert(id.clone());
                        }
                    }
                }
                Err(e) => {
                    if !problems.contains(&e) {
                        problems.push(e);
                    }
                    continue;
                }
            }
            if d != before {
                next[slot] = Some(d);
            }
        }
        if !write {
            continue;
        }
        let [doc_now, base_now] = next;
        if let Some(d) = &doc_now {
            conn.execute(
                "UPDATE card_docs SET doc = ?, kind = ? WHERE project_id = ? AND branch_id = ? AND id = ?",
                params![
                    Value::Object(d.clone()).to_string(),
                    d.get("kind").and_then(Value::as_str).unwrap_or_default(),
                    project,
                    branch,
                    id
                ],
            )
            .map_err(|e| fail(db(e)))?;
        }
        if let Some(b) = &base_now {
            conn.execute(
                "UPDATE card_docs SET base = ? WHERE project_id = ? AND branch_id = ? AND id = ?",
                params![Value::Object(b.clone()).to_string(), project, branch, id],
            )
            .map_err(|e| fail(db(e)))?;
        }
        // A copy taken away keeps its kind in its row.
        if doc_now.is_none()
            && let Some(kind) = landing_in.get(id)
        {
            conn.execute(
                "UPDATE card_docs SET kind = ? WHERE project_id = ? AND branch_id = ? AND id = ? AND doc IS NULL",
                params![kind, project, branch, id],
            )
            .map_err(|e| fail(db(e)))?;
        }
    }
    if !problems.is_empty() {
        return Err(problems);
    }
    // What the branches keep of main's cards as they were made: renamed alike, the project's kinds
    // being the project's.
    if write {
        let mut stmt = conn
            .prepare("SELECT branch_id, id, doc FROM branch_frozen WHERE project_id = ? AND tbl = 'card_docs' AND doc IS NOT NULL")
            .map_err(|e| fail(sql(e)))?;
        let kept: Vec<(String, String, Option<String>)> = stmt
            .query_map([project], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .map_err(|e| fail(sql(e)))?
            .collect::<Result<_, _>>()
            .map_err(|e| fail(sql(e)))?;
        for (branch, id, doc) in kept {
            let Some(mut d) = parse(&doc) else {
                continue;
            };
            let before = d.clone();
            if rename_doc(&mut d, renames, &landing_in).is_ok() && d != before {
                conn.execute(
                    "UPDATE branch_frozen SET doc = ? WHERE project_id = ? AND branch_id = ? AND tbl = 'card_docs' AND id = ?",
                    params![Value::Object(d).to_string(), project, branch, id],
                )
                .map_err(|e| fail(db(e)))?;
            }
        }
    }
    Ok(counted.into_iter().map(|c| c.len()).collect())
}

// MARK: - Write

/// Where a kind of the project's own is used, in every tree: the cards of a kind (`cards`), the
/// lines of a kind (`lines`) or the sections of a kind (`sections`), each said as its card's or
/// section's name.
fn kind_uses(
    conn: &Connection,
    project: &str,
    list: &str,
    name: &str,
) -> Result<Vec<String>, String> {
    let table = if list == "sections" {
        "section_docs"
    } else {
        "card_docs"
    };
    let mut stmt = conn
        .prepare(&format!(
            "SELECT name, doc FROM {table} WHERE project_id = ? AND removed = 0 AND doc IS NOT NULL ORDER BY branch_id, name"
        ))
        .map_err(sql)?;
    let rows: Vec<(String, String)> = stmt
        .query_map([project], |r| Ok((r.get(0)?, r.get(1)?)))
        .map_err(sql)?
        .collect::<Result<_, _>>()
        .map_err(sql)?;
    let mut out = Vec::new();
    for (owner, doc) in rows {
        let Ok(doc) = serde_json::from_str::<Value>(&doc) else {
            continue;
        };
        if list == "lines" {
            let n = crate::kinds::connections_of(&doc)
                .iter()
                .filter(|c| c["line"] == name)
                .count();
            out.extend(std::iter::repeat_n(owner.clone(), n));
        } else if doc["kind"] == name {
            out.push(owner);
        }
    }
    Ok(out)
}

/// A built-in kind's definition with `vias` as its field `via`'s choices, its rows' too.
fn vias_written(definition: &mut Value, vias: &[String]) {
    if let Some(fields) = definition.get_mut("fields").and_then(Value::as_array_mut) {
        for field in fields.iter_mut().filter(|f| f["name"] == "via") {
            field["options"] = vias.to_vec().into();
        }
    }
    if let Some(rows) = definition.get_mut("rows").and_then(Value::as_array_mut) {
        for row in rows {
            vias_written(row, vias);
        }
    }
}

/// Where a value of a flow's `via` is said, in every tree: a begin's or a run's, a process node's
/// rows' — each said as its node's name.
fn via_uses(conn: &Connection, project: &str, via: &str) -> Result<Vec<String>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT name, doc FROM card_docs WHERE project_id = ? AND removed = 0 AND doc IS NOT NULL ORDER BY branch_id, name",
        )
        .map_err(sql)?;
    let rows: Vec<(String, String)> = stmt
        .query_map([project], |r| Ok((r.get(0)?, r.get(1)?)))
        .map_err(sql)?
        .collect::<Result<_, _>>()
        .map_err(sql)?;
    let mut out = Vec::new();
    for (owner, doc) in rows {
        let Ok(doc) = serde_json::from_str::<Value>(&doc) else {
            continue;
        };
        let kind = doc["kind"].as_str().unwrap_or_default();
        if !crate::kinds::VIA_KINDS.contains(&kind) {
            continue;
        }
        let rows = doc["rows"]["steps"].as_array().map_or(0, |steps| {
            steps.iter().filter(|r| r["fields"]["via"] == via).count()
        });
        let own = usize::from(doc["fields"]["via"] == via);
        out.extend(std::iter::repeat_n(owner, own + rows));
    }
    Ok(out)
}

/// Whether `project_update`'s `value` (its `project_tree` and `renames`) would change main's sections
/// directly under its root. Nothing is written; a change that has problems is not one (it is refused
/// by the call itself, in its own words).
pub(crate) fn changes_main_tree(
    conn: &Connection,
    project: &str,
    value: &Map<String, Value>,
) -> bool {
    let tree = value
        .get("project_tree")
        .map(|t| Value::Object(Map::from_iter([("project_tree".to_string(), t.clone())])));
    let renames = value.get("renames");
    if tree.is_none() && renames.is_none() {
        return false;
    }
    planned(conn, project, tree.as_ref(), renames).is_ok_and(|c| c.main_changes)
}

/// What a change to the project's own kinds would be: the definitions it leaves, the renames,
/// and what it says it changed — or every problem with it.
struct Change {
    own: Map<String, Value>,
    renames: Vec<Rename>,
    moved: Vec<usize>,
    counts: Vec<crate::text::Keyed>,
    /// What writing `project_tree` does under each root; none when it is not written.
    tree: Vec<super::project_tree::TreeProject>,
    /// Whether main's sections directly under its root change, their order too.
    main_changes: bool,
    /// The working directories `project_tree` lists (`directories::linked`); none when it is not
    /// written.
    dirs: Option<Vec<super::directories::Directory>>,
}

/// The change `kinds` and `renames` ask for, checked whole — nothing written.
fn planned(
    conn: &Connection,
    project: &str,
    kinds: Option<&Value>,
    renames: Option<&Value>,
) -> Result<Change, Vec<String>> {
    let mut problems = Vec::new();
    let mut own = kept(conn, project).map_err(|e| vec![e])?;
    let before = own.clone();
    match kinds {
        None => {}
        Some(Value::Object(given)) => {
            for (key, list) in given {
                if key == "contexts" || key == "icons" {
                    if list.is_null() || list.as_object().is_some_and(|o| o.is_empty()) {
                        own.remove(key);
                    } else {
                        own.insert(key.clone(), list.clone());
                    }
                } else if key == "vias" {
                    if list.is_null() || list.as_array().is_some_and(|a| a.is_empty()) {
                        own.remove(key);
                    } else {
                        own.insert(key.clone(), list.clone());
                    }
                } else if key == "project_tree" {
                    if list.is_object() {
                        own.insert(key.clone(), list.clone());
                    } else {
                        problems.push(crate::kinds::project_tree_shape());
                    }
                } else if !LISTS.contains(&key.as_str()) {
                    problems.push(text!("ownKinds.kinds.keyNotAllowed", key));
                } else if !list.is_array() {
                    problems.push(text!("ownKinds.kinds.notList", key));
                } else {
                    own.insert(key.clone(), list.clone());
                }
            }
        }
        Some(_) => problems.push(text!("ownKinds.kinds.shape")),
    }
    // The working directories `project_tree` lists, each given the id it is by.
    let dirs = {
        let (_, tops) = renames_given(renames, &mut Vec::new());
        if kinds.is_some_and(|k| k.get("project_tree").is_some()) || !tops.is_empty() {
            let was =
                crate::kinds::Defs::of_project(Some(&Value::Object(before.clone()).to_string()));
            super::directories::linked(conn, project, &was.tree, &mut own, &tops, &mut problems)
                .map_err(|e| vec![e])?
        } else {
            None
        }
    };
    let defs = match crate::kinds::Defs::checked(&Value::Object(own.clone())) {
        Ok(defs) => Some(defs),
        Err(found) => {
            problems.extend(found);
            None
        }
    };
    let (renames, top_renames) = renames_given(renames, &mut problems);
    if let Some(defs) = &defs {
        for rename in &renames {
            if let Some(problem) = rename_problem(rename, defs) {
                problems.push(problem);
            }
        }
    }
    // One taken away that is still in use stays; a kind of card renamed takes its cards along.
    let renamed_kinds: Vec<&str> = renames
        .iter()
        .filter_map(|r| match r {
            Rename::Kind { from, .. } => Some(from.as_str()),
            _ => None,
        })
        .collect();
    let named = |list: &Map<String, Value>, key: &str, by: &str| -> Vec<String> {
        list.get(key)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|d| d[by].as_str().map(str::to_string))
            .collect()
    };
    let mut counts = Vec::new();
    for (key, by) in [
        ("lines", "line"),
        ("sections", "section"),
        ("cards", "kind"),
    ] {
        let now = named(&own, key, by);
        for gone in named(&before, key, by)
            .into_iter()
            .filter(|n| !now.contains(n))
            .filter(|n| key != "cards" || !renamed_kinds.contains(&n.as_str()))
        {
            let uses = kind_uses(conn, project, key, &gone).map_err(|e| vec![e])?;
            if !uses.is_empty() {
                let mut shown: Vec<String> = uses.clone();
                shown.dedup();
                let (count, places) = (uses.len(), super::capped(&shown, 5));
                problems.push(match key {
                    "lines" => text!("ownKinds.inUse.lines", gone, count, places),
                    "sections" => text!("ownKinds.inUse.sections", gone, count, places),
                    _ => text!("ownKinds.inUse.cards", gone, count, places),
                });
            }
        }
        let n = now.len();
        counts.push(match key {
            "lines" => keyed!("ownKinds.count.lines", n),
            "sections" => keyed!("ownKinds.count.sections", n),
            _ => keyed!("ownKinds.count.cards", n),
        });
    }
    // A value of `via` taken away that a node still says stays.
    let vias = |list: &Map<String, Value>| -> Vec<String> {
        list.get("vias")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect()
    };
    let now = vias(&own);
    for gone in vias(&before).into_iter().filter(|v| !now.contains(v)) {
        let uses = via_uses(conn, project, &gone).map_err(|e| vec![e])?;
        if !uses.is_empty() {
            let mut shown = uses.clone();
            shown.dedup();
            problems.push(text!(
                "ownKinds.inUse.via",
                gone,
                count = uses.len(),
                places = super::capped(&shown, 5)
            ));
        }
    }
    // What the renames move, and what is in their way.
    let moved = if renames.is_empty() || !problems.is_empty() {
        vec![0; renames.len()]
    } else {
        match rename_all(conn, project, &renames, false) {
            Ok(moved) => moved,
            Err(found) => {
                problems.extend(found);
                Vec::new()
            }
        }
    };
    // What is directly under each root, when it is written: compared with the trees as they are.
    let tree_written =
        kinds.is_some_and(|k| k.get("project_tree").is_some()) || !top_renames.is_empty();
    let mut tree = Vec::new();
    let mut main_changes = false;
    if tree_written
        && problems.is_empty()
        && let Some(defs) = &defs
    {
        if !own.contains_key("project_tree") {
            problems.push(text!(
                "ownKinds.tree.needed",
                shape = crate::kinds::project_tree_shape()
            ));
        } else {
            match super::project_tree::planned(conn, project, &defs.tree, &top_renames) {
                Ok(found) => tree = found,
                Err(found) => problems.extend(found),
            }
            let was =
                crate::kinds::Defs::of_project(Some(&Value::Object(before.clone()).to_string()));
            main_changes = was.tree.main != defs.tree.main;
        }
    }
    if !problems.is_empty() {
        return Err(problems);
    }
    Ok(Change {
        own,
        renames,
        moved,
        counts,
        tree,
        main_changes,
        dirs,
    })
}

/// `project_update`'s `kinds` and `renames`: each list given in place of the one there, checked
/// whole beside the built-in ones, the renames made in every card of every tree, and a line in
/// main's history. With `check`, nothing is written: what it would be is said, problems and
/// all (`ok`, `problems`).
pub(crate) fn set_kinds(
    tx: &mut Tx,
    project: &str,
    kinds: Option<&Value>,
    renames: Option<&Value>,
    check: bool,
    said: Option<&crate::text::Keyed>,
) -> Result<Value, String> {
    let change = planned(tx.conn(), project, kinds, renames);
    if check {
        let mut out = Map::new();
        match change {
            Ok(change) => {
                out.insert("ok".into(), true.into());
                out.insert("problems".into(), Value::Array(Vec::new()));
                out.insert("kinds".into(), Value::Object(change.own));
                if !change.tree.is_empty() {
                    out.insert(
                        "projectTree".into(),
                        Value::Array(change.tree.iter().map(|t| t.json()).collect()),
                    );
                }
                if !change.renames.is_empty() {
                    out.insert(
                        "renamed".into(),
                        Value::Array(
                            change
                                .renames
                                .iter()
                                .zip(change.moved)
                                .map(|(r, n)| r.json(n))
                                .collect(),
                        ),
                    );
                }
            }
            Err(problems) => {
                out.insert("ok".into(), false.into());
                out.insert(
                    "problems".into(),
                    Value::Array(problems.into_iter().map(Value::from).collect()),
                );
            }
        }
        return Ok(Value::Object(out));
    }
    let change =
        change.map_err(|problems| text!("ownKinds.set.invalid", problems = problems.join("\n")))?;
    // main's sections directly under its root are main's, and no merge carries them: under its lock, the
    // window's, and MCP's once the project's item `project_update.main_tree` has let the call through
    // (it is judged before the call runs).
    if change.main_changes && tx.locked_at(project)?.is_some() {
        tx.merging();
    }
    let empty = change
        .own
        .values()
        .all(|v| v.as_array().is_some_and(Vec::is_empty));
    let text = (!empty).then(|| Value::Object(change.own.clone()).to_string());
    tx.conn()
        .execute(
            "UPDATE projects SET kinds = ? WHERE id = ?",
            params![text, project],
        )
        .map_err(db)?;
    let moved = if change.renames.is_empty() {
        Vec::new()
    } else {
        rename_all(tx.conn(), project, &change.renames, true).map_err(|p| p.join("\n"))?
    };
    if !change.tree.is_empty() {
        super::project_tree::apply(tx, project, &change.tree)?;
    }
    if let Some(dirs) = &change.dirs {
        super::directories::settle(tx.conn(), project, dirs)?;
    }
    let mut line = match said {
        Some(said) => said.clone(),
        None => keyed!(
            "ownKinds.line.changed",
            counts = crate::text::List::of_keyed(change.counts.iter().cloned())
        ),
    };
    if !change.renames.is_empty() {
        let names = crate::text::List::of_keyed(
            change
                .renames
                .iter()
                .zip(&moved)
                .map(|(r, n)| keyed!("ownKinds.line.renamedOne", said = r.said(), n)),
        );
        line = keyed!("ownKinds.line.withRenames", said = line, renames = names);
    }
    tx.line(
        project,
        "",
        "project.update",
        "project",
        Some(project),
        line,
    )?;
    let mut out = Map::new();
    out.insert("kinds".into(), Value::Object(change.own));
    if change
        .tree
        .iter()
        .any(super::project_tree::TreeProject::changes)
    {
        out.insert(
            "projectTree".into(),
            Value::Array(change.tree.iter().map(|t| t.json()).collect()),
        );
    }
    if !change.renames.is_empty() {
        out.insert(
            "renamed".into(),
            Value::Array(
                change
                    .renames
                    .iter()
                    .zip(moved)
                    .map(|(r, n)| r.json(n))
                    .collect(),
            ),
        );
    }
    Ok(Value::Object(out))
}

/// A section directly under a root renamed (`section_update`'s `name` on it, the nav panel's 名前を
/// 変更): its name in `project_tree` and in every tree, as `renames`' `top_section` does.
pub(crate) fn rename_top(tx: &mut Tx, project: &str, from: &str, to: &str) -> Result<(), String> {
    let own = kept(tx.conn(), project)?;
    let defs = crate::kinds::Defs::of_project(Some(&Value::Object(own).to_string()));
    let mut tree = defs.tree.clone();
    for list in [&mut tree.main, &mut tree.board] {
        for top in list.iter_mut() {
            if crate::pathtext::same(&top.name, from) {
                top.name = to.to_string();
            }
        }
    }
    let mut kinds = Map::new();
    kinds.insert("project_tree".into(), tree.json());
    let mut rename = Map::new();
    rename.insert("top_section".into(), from.into());
    rename.insert("to".into(), to.into());
    set_kinds(
        tx,
        project,
        Some(&Value::Object(kinds)),
        Some(&Value::Array(vec![Value::Object(rename)])),
        false,
        None,
    )
    .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::renamed_path;

    #[test]
    fn a_path_renamed_by_its_first_name() {
        assert_eq!(
            renamed_path("sev", "sev", "severity").as_deref(),
            Some("severity")
        );
        assert_eq!(
            renamed_path("sev.x", "sev", "severity").as_deref(),
            Some("severity.x")
        );
        assert_eq!(
            renamed_path("items[0]", "items", "steps").as_deref(),
            Some("steps[0]")
        );
        assert_eq!(renamed_path("severe", "sev", "severity"), None);
    }
}
