//! The tools that write sections (`section_create`, `section_update`, `section_delete`;
//! the Swift app's `MCPSurface.sections`, `MCPTools`' `create_section` / `update_section`
//! / `delete_section` and `Repository+Sections`): a section's document written where the
//! tree keeps it, the paths written in the project following a new name or a new place, and
//! every target of a call tried as the card tools try them.

use super::Seen;
use super::args::{Args, Refusals};
use super::cards::{Scope, section_label, text};
use super::follow::{InnerMoves, follow};
use super::reads::refuse_settings_as_section;
use crate::kinds::{Defs, EVACUATION, Tree, reserved_name};
use crate::model::Section;
use crate::store::BOARD;
use crate::write::{Tx, new_id, valid_section_name};
use serde_json::{Map, Value};
use std::collections::HashMap;

/// What `type` on a section is told (`MCPTools.noSectionType`).
pub(crate) fn no_section_type() -> String {
    text!("sections.error.noType")
}

/// The root's own sections, one of each, in their order (`SectionKind.creatable`).
/// What `cardGhosts` is told now that a card ghost is a card of its own.
pub(crate) fn card_ghosts_gone() -> String {
    text!("sections.error.cardGhostsGone")
}

// MARK: - Several targets

/// Each target written as one, with its own value: every target tried, and what was
/// wrong with any of them said together, with nothing kept (`eachAtomically` over
/// `pairs`).
pub(crate) fn each_pair(
    tx: &mut Tx,
    scope: &Scope,
    pairs: &[(String, Map<String, Value>)],
    body: impl Fn(&mut Tx, &Scope, &str, &Map<String, Value>) -> Result<Value, String>,
) -> Result<Value, String> {
    if let [(target, value)] = pairs {
        return body(tx, scope, target, value);
    }
    let mut refusals = Refusals::new(pairs.len());
    let mut results = Vec::new();
    for (target, value) in pairs {
        match tx.alone(|tx| body(tx, scope, target, value))? {
            Ok(result) => {
                let mut row = Map::new();
                row.insert("target".into(), target.clone().into());
                row.insert("result".into(), super::under_head(result));
                results.push(Value::Object(row));
            }
            Err(e) => refusals.note(Some(target), &e),
        }
    }
    refusals.check()?;
    let mut out = Map::new();
    out.insert("project".into(), scope.project_name.clone().into());
    out.insert("results".into(), Value::Array(results));
    Ok(Value::Object(out))
}

// MARK: - The sections as the tree keeps them

fn branch(scope: &Scope) -> Option<&str> {
    (!scope.tree.is_empty()).then_some(scope.tree.as_str())
}

/// Every section's document as this tree sees it, by id.
fn section_docs(tx: &Tx, scope: &Scope) -> Result<HashMap<String, Map<String, Value>>, String> {
    Ok(tx
        .reader()
        .compose(&scope.project, branch(scope))?
        .sections
        .into_iter()
        .map(|i| (i.id, i.doc))
        .collect())
}

/// A section's document, as this tree sees it.
fn section_doc(tx: &Tx, scope: &Scope, id: &str) -> Result<Map<String, Value>, String> {
    section_docs(tx, scope)?
        .remove(id)
        .ok_or_else(|| text!("sections.error.missing"))
}

/// A key of a section's document, left out when it holds what its column falls back to
/// (`RowCodec.object`): no value, no text, zero, an empty list.
fn set(doc: &mut Map<String, Value>, key: &str, value: Value) {
    let fallback = match &value {
        Value::Null => true,
        Value::String(s) => s.is_empty() && key == "description",
        Value::Number(n) => n.as_i64() == Some(0),
        Value::Array(a) => a.is_empty(),
        _ => false,
    };
    if fallback {
        doc.shift_remove(key);
    } else {
        doc.insert(key.to_string(), value);
    }
}

/// Each section's place among its siblings, by id (`sort_order`).
fn orders(seen: &Seen) -> HashMap<String, i64> {
    seen.raw["sections"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|s| Some((s["id"].as_str()?.to_string(), s["sortOrder"].as_i64()?)))
        .collect()
}

/// The place after the last of the sections in `parent` (`max(sortOrder) + 1`).
fn next_order(seen: &Seen, parent: Option<&str>) -> i64 {
    let orders = orders(seen);
    seen.detail
        .sections
        .iter()
        .filter(|s| s.parent_id.as_deref() == parent)
        .filter_map(|s| orders.get(&s.id).copied())
        .max()
        .map_or(0, |n| n + 1)
}

fn root(sections: &[Section]) -> Option<String> {
    sections
        .iter()
        .find(|s| s.parent_id.is_none())
        .map(|s| s.id.clone())
}

/// One of the root's own sections: directly under it — the project's (`project_tree`) and
/// `_evacuation_` — fixed there (`isRootOwn`).
fn is_root_own(section: &Section, root: Option<&str>) -> bool {
    section.parent_id.is_some() && section.parent_id.as_deref() == root
}

/// Whether a section of `kind` is one the card actions move cards into (`complete`, `cancelled`):
/// made by the core alone, one of each in a Goals section, never moved out of it nor deleted.
fn action_place(kind: Option<&str>) -> bool {
    matches!(kind, Some(crate::kinds::COMPLETE | crate::kinds::CANCELLED))
}

/// What is said to a section the card actions move cards into, made, moved or deleted.
fn action_place_fixed(defs: &Defs, name: &str, kind: &str) -> String {
    text!(
        "sections.error.actionPlace",
        name,
        kind = section_label(defs, kind)
    )
}

/// The tree a scope writes, as `project_tree` names it: the board's, or main's and a branch's.
fn tree_of(board: bool) -> Tree {
    if board { Tree::Board } else { Tree::Main }
}

/// A section kind as people call it, or nothing for none (`kind?.label ?? ""`).
fn label_of(defs: &Defs, kind: Option<&str>) -> String {
    kind.map_or(String::new(), |k| section_label(defs, k))
}

/// Why nothing more is made or put directly under the root, and where it goes
/// (`notAtTheTop`): what is there is the project's (`project_tree`).
fn not_at_the_top(defs: &Defs, sections: &[Section], board: bool) -> String {
    let root = root(sections);
    let here: Vec<String> = sections
        .iter()
        .filter(|s| s.parent_id.is_some() && s.parent_id == root)
        .map(|s| {
            text!(
                "sections.part.nameWithKind",
                name = s.name,
                kind = label_of(defs, s.kind.as_deref())
            )
        })
        .collect();
    text!(
        "sections.error.notAtTheTop",
        tree = tree_of(board).said(),
        here = crate::text::list(&here)
    )
}

/// What a name the app keeps for itself is told (`_evacuation_`, `_section_ghosts_`).
fn reserved_refused(name: &str) -> String {
    text!("sections.error.reservedName", name)
}

/// Once among its siblings, in any case (`requireSectionNameFree`).
fn require_name_free(
    name: &str,
    siblings: &[&Section],
    excluding: Option<&str>,
) -> Result<(), String> {
    match siblings
        .iter()
        .find(|s| Some(s.id.as_str()) != excluding && crate::pathtext::same(&s.name, name))
    {
        Some(clash) => Err(text!("sections.error.nameTaken", name = clash.name)),
        None => Ok(()),
    }
}

/// Not a root's name directly under a root, which would hide the root from every path
/// (`requireNoShadowing`).
fn require_no_shadowing(
    name: &str,
    parent: Option<&str>,
    sections: &[Section],
    excluding: Option<&str>,
) -> Result<(), String> {
    let same =
        |s: &Section| Some(s.id.as_str()) != excluding && crate::pathtext::same(&s.name, name);
    let roots: Vec<&Section> = sections.iter().filter(|s| s.parent_id.is_none()).collect();
    if let Some(parent) = parent
        && roots.iter().any(|r| r.id == parent)
        && let Some(clash) = roots.iter().find(|r| same(r))
    {
        return Err(text!("sections.error.rootName", name = clash.name));
    }
    if parent.is_none()
        && let Some(clash) = sections.iter().find(|s| {
            s.parent_id
                .as_deref()
                .is_some_and(|p| roots.iter().any(|r| r.id == p))
                && same(s)
        })
    {
        return Err(text!("sections.error.hidesRoot", name = clash.name));
    }
    Ok(())
}

/// A section's row as `section_list` has it, without its own path: what a write to it
/// answers with (`sectionRow`).
fn row(seen: &Seen, id: &str) -> Map<String, Value> {
    let mut row = seen
        .section_rows(Some(id))
        .into_iter()
        .next()
        .unwrap_or_default();
    row.shift_remove("section");
    row
}

/// `main@Store` → (`main`, `Store`): the tree written before trees left paths, and the
/// rest (`splitTree`).
fn split_tree(raw: &str) -> (Option<&str>, &str) {
    match crate::pathtext::tree_prefix(raw) {
        Some((tree, rest)) => (
            Some(tree.trim_matches([' ', '\t'])),
            rest.trim_matches([' ', '\t']),
        ),
        None => (None, raw),
    }
}

// MARK: - section_create

pub(crate) fn section_create(tx: &mut Tx, scope: &Scope, args: &Args) -> Result<Value, String> {
    let pairs = args.pairs()?;
    each_pair(tx, scope, &pairs, |tx, scope, path, given| {
        // The kind is one the parent's kind lists (`sections`), its first unless `type` names another.
        let wanted = match given.get("type") {
            None => None,
            Some(Value::String(t)) => {
                let defs = Seen::in_tx(tx, scope)?.project().defs;
                if defs.section(t).is_none() {
                    return Err(type_refused(t));
                }
                Some(t.clone())
            }
            Some(_) => return Err(no_section_type()),
        };
        let mut given = given.clone();
        given.remove("type");
        // What an actor is and a section's own mark: said here, in the words their checks have.
        let figure = given.remove("figure");
        let icon = given.remove("icon");
        let given = &given;
        super::check(given, "section_create", "section")?;
        // A tree from before stays with the parent: `main@Store` is Store under the root.
        let (tree, rest) = split_tree(path);
        let head = tree.map(|t| format!("{t}@")).unwrap_or_default();
        let (parent, name) = match crate::pathtext::last_slash(rest) {
            Some((parent, name)) => (Some(format!("{head}{parent}")), name),
            None => ((!head.is_empty()).then_some(head), rest),
        };
        let name = crate::pathtext::unquote(name);
        let description = given
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or_default();
        create_section(
            tx,
            scope,
            parent.as_deref(),
            &name,
            description,
            Made {
                kind: wanted.as_deref(),
                figure: figure.as_ref(),
                icon: icon.as_ref(),
                path,
            },
        )
    })
}

/// What `type` is told when it names no kind of section there is.
fn type_refused(written: &str) -> String {
    text!("sections.error.unknownType", written)
}

/// What a section is made as besides its name: the kind `type` names, what an actor is (`figure`),
/// its own mark (`icon`), and the path it was asked at (what a refusal of these names).
struct Made<'a> {
    kind: Option<&'a str>,
    figure: Option<&'a Value>,
    icon: Option<&'a Value>,
    path: &'a str,
}

/// `figure` as a section of `kind` keeps it: none for a system (the default), refused on a section
/// that is not an actor and for anything but the figures there are.
fn figure_of(
    defs: &Defs,
    path: &str,
    kind: Option<&str>,
    given: &Value,
) -> Result<Option<String>, String> {
    if given.is_null() {
        return Ok(None);
    }
    if kind != Some(crate::kinds::ACTOR) {
        return Err(text!(
            "sections.error.figureNotActor",
            path,
            kind = label_of(defs, kind)
        ));
    }
    match given.as_str() {
        Some(figure) if crate::kinds::FIGURES.contains(&figure) => {
            Ok((figure != "system").then(|| figure.to_string()))
        }
        _ => Err(text!(
            "sections.error.figureValue",
            given = crate::ojson::compact(given),
            figures = crate::text::List::of(crate::kinds::FIGURES)
        )),
    }
}

/// A section's own mark as written over `had`: each part given in place of the one it had, `null`
/// taking it away — checked as a kind's `icon` is (`kinds::read_icon`), the same words for what is
/// wrong. None when no part is left. Not on a working directory's sections: their mark is their
/// kind's (`kind_update`).
fn icon_of(
    path: &str,
    kind: Option<&str>,
    had: Option<&Map<String, Value>>,
    given: &Value,
) -> Result<Option<Map<String, Value>>, String> {
    if matches!(kind, Some(crate::kinds::WORKDIR | crate::kinds::DIRECTORY)) {
        return Err(text!("sections.error.iconWorkdir", path));
    }
    let given = match given {
        Value::Null => return Ok(None),
        Value::Object(o) => o,
        other => {
            let mut problems = Vec::new();
            crate::kinds::read_icon(Some(other), path, &mut problems);
            return Err(problems.join("\n"));
        }
    };
    // Each part null takes the part away; the check reads what is written.
    let mut mark = had.cloned().unwrap_or_default();
    let mut written = Map::new();
    for (key, value) in given {
        if value.is_null() {
            mark.shift_remove(key);
        } else {
            written.insert(key.clone(), value.clone());
            mark.insert(key.clone(), value.clone());
        }
    }
    let mut problems = Vec::new();
    crate::kinds::read_icon(Some(&Value::Object(written)), path, &mut problems);
    if !problems.is_empty() {
        return Err(problems.join("\n"));
    }
    // The keys it may have only, in the order a kind's mark says them.
    let mut out = Map::new();
    for key in ["name", "color"] {
        if let Some(v) = mark.get(key) {
            out.insert(key.into(), v.clone());
        }
    }
    Ok((!out.is_empty()).then_some(out))
}

/// Whether a section of `wanted` may be inside one of `parent_kind`: a kind the parent's kind lists
/// in `sections`; refused naming those it lists.
fn allowed_inside(defs: &Defs, parent_kind: Option<&str>, wanted: &str) -> Result<(), String> {
    let inside = parent_kind.unwrap_or_default();
    let listed: Vec<String> = defs
        .section(inside)
        .map(|d| d.sections.clone())
        .unwrap_or_default();
    if listed.iter().any(|s| s == wanted) {
        return Ok(());
    }
    Err(text!(
        "sections.error.notInside",
        kind = section_label(defs, inside),
        allowed = crate::text::List::of(&listed),
        wanted
    ))
}

/// A section made inside `parent`, last among its siblings, of its parent's kind
/// (`create_section`, `Repository.createSection`): nothing is made directly under a root,
/// whose sections are the project's (`project_tree`).
fn create_section(
    tx: &mut Tx,
    scope: &Scope,
    parent: Option<&str>,
    raw_name: &str,
    raw_description: &str,
    made: Made,
) -> Result<Value, String> {
    let own_type = made.kind;
    let seen = Seen::in_tx(tx, scope)?;
    let defs = seen.project().defs;
    let given = match parent {
        Some(raw) => {
            refuse_settings_as_section(&seen, raw)?;
            Some(super::section_id(&seen.project(), raw)?)
        }
        None => None,
    };
    let name = valid_section_name(raw_name)?;
    if reserved_name(&name) {
        return Err(reserved_refused(&name));
    }
    let description = text(raw_description, &text!("common.label.description"), 2000)?;
    let sections = &seen.detail.sections;
    let root = root(sections);
    let parent_id = given.or_else(|| root.clone());
    // What is directly under a root is the project's (`project_tree`).
    let board = scope.tree == BOARD;
    if parent_id == root {
        return Err(not_at_the_top(&defs, sections, board));
    }
    let siblings: Vec<&Section> = sections
        .iter()
        .filter(|s| s.parent_id == parent_id)
        .collect();
    require_name_free(&name, &siblings, None)?;
    require_no_shadowing(&name, parent_id.as_deref(), sections, None)?;
    let mut kind: Option<String> = None;
    let parent_kind = sections
        .iter()
        .find(|s| Some(&s.id) == parent_id.as_ref())
        .and_then(|s| s.kind.clone());
    if let Some(inherited) = &parent_kind {
        // What a kind of the project's own makes inside it: its `sections`, the first unless
        // `type` names another.
        kind = Some(match defs.section(inherited) {
            Some(def) => def
                .sections
                .first()
                .cloned()
                .unwrap_or_else(|| inherited.clone()),
            None => inherited.clone(),
        });
    }
    if let Some(wanted) = own_type {
        // The places the card actions move cards into are the core's to make, never a call's.
        if action_place(Some(wanted)) {
            return Err(text!(
                "sections.error.typeActionPlace",
                kind = section_label(&defs, wanted)
            ));
        }
        allowed_inside(&defs, parent_kind.as_deref(), wanted)?;
        kind = Some(wanted.to_string());
    }
    let figure = made
        .figure
        .map(|f| figure_of(&defs, made.path, kind.as_deref(), f))
        .transpose()?
        .flatten();
    let icon = made
        .icon
        .map(|i| icon_of(made.path, kind.as_deref(), None, i))
        .transpose()?
        .flatten();
    if let Some(k) = kind.as_deref().filter(|k| action_place(Some(k))) {
        return Err(action_place_fixed(&defs, &name, k));
    }
    // Not sections any more: the project's settings.
    if matches!(kind.as_deref(), Some("export") | Some("rules")) {
        return Err(text!("sections.error.settingsNotSections"));
    }
    let id = new_id();
    let order = next_order(&seen, parent_id.as_deref());
    drop(seen);
    let mut doc = Map::new();
    doc.insert("id".into(), id.clone().into());
    set(
        &mut doc,
        "parent_id",
        parent_id.map_or(Value::Null, Value::from),
    );
    doc.insert("name".into(), name.into());
    set(&mut doc, "description", description.into());
    set(&mut doc, "sort_order", order.into());
    doc.insert("created_at".into(), tx.now()?.into());
    set(&mut doc, "kind", kind.map_or(Value::Null, Value::from));
    set(&mut doc, "figure", figure.map_or(Value::Null, Value::from));
    set(&mut doc, "icon", icon.map_or(Value::Null, Value::Object));
    tx.put_section(&scope.project, &scope.tree, &doc)?;
    let after = Seen::in_tx(tx, scope)?;
    let path = after.project().directory.full_section(&id);
    tx.line(
        &scope.project,
        &scope.tree,
        "section.create",
        "section",
        Some(&id),
        keyed!("sections.line.create", path),
    )?;
    let mut out = Map::new();
    out.insert("project".into(), scope.project_name.clone().into());
    out.insert("created".into(), path.into());
    out.extend(row(&after, &id));
    Ok(Value::Object(out))
}

// MARK: - section_update

/// What a call changes of a section (`Repository.SectionEdit`).
#[derive(Default)]
struct Edit {
    name: Option<String>,
    description: Option<String>,
    /// Where it goes among its siblings, counting from zero.
    position: Option<i64>,
    /// Another section to move it into — `Some(None)` for directly under the root.
    parent: Option<Option<String>>,
    /// The kind it is made (`type`): one its parent's kind lists.
    kind: Option<String>,
    /// What an actor is (`figure`), `null` for a system.
    figure: Option<Value>,
    /// Its own mark (`icon`): the parts written over those it has, `null` taking one or all away.
    icon: Option<Value>,
}

pub(crate) fn section_update(tx: &mut Tx, scope: &Scope, args: &Args) -> Result<Value, String> {
    let pairs = args.pairs()?;
    each_pair(tx, scope, &pairs, |tx, scope, path, given| {
        if given.get("type").is_some_and(|t| !t.is_string()) {
            return Err(no_section_type());
        }
        // A section's card ghosts are cards of their own.
        if given.contains_key("cardGhosts") {
            return Err(card_ghosts_gone());
        }
        // What sets a working directory's section, apart from what changes the section.
        let settings: Map<String, Value> = given
            .iter()
            .filter(|(k, _)| super::workdirs::SETTINGS.contains(&k.as_str()))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        if !settings.is_empty()
            && let Some(dir) = super::workdirs::update_settings(tx, scope, path, &settings)?
        {
            let rest: Map<String, Value> = given
                .iter()
                .filter(|(k, _)| !super::workdirs::SETTINGS.contains(&k.as_str()))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            let mut out = if rest.is_empty() {
                let seen = Seen::in_tx(tx, scope)?;
                let id = super::section_id(&seen.project(), path)?;
                let mut out = Map::new();
                out.insert("project".into(), scope.project_name.clone().into());
                out.insert(
                    "section".into(),
                    seen.project().directory.full_section(&id).into(),
                );
                out.extend(row(&seen, &id));
                out
            } else {
                super::check(&rest, "section_update", "section")?;
                match update_one(tx, scope, path, &rest)? {
                    Value::Object(o) => o,
                    _ => Map::new(),
                }
            };
            let (said, warnings) = super::workdirs::said(tx.conn(), &scope.project, &dir)?;
            out.extend(said);
            return Ok(super::warned(Value::Object(out), &warnings));
        }
        // What an actor is and a section's own mark: said in the words their checks have.
        let mut checked = given.clone();
        checked.shift_remove("figure");
        checked.shift_remove("icon");
        super::check(&checked, "section_update", "section")?;
        update_one(tx, scope, path, given)
    })
}

/// One section changed as `update_section` reads the call.
fn update_one(
    tx: &mut Tx,
    scope: &Scope,
    path: &str,
    given: &Map<String, Value>,
) -> Result<Value, String> {
    let seen = Seen::in_tx(tx, scope)?;
    let project = seen.project();
    refuse_settings_as_section(&seen, path)?;
    let target = super::section_id(&project, path)?;
    let text_of = |key: &str| given.get(key).and_then(Value::as_str).map(str::to_string);
    // A section directly under a root is the project's: renamed in `project_tree` and in every tree.
    let top_rename = match (seen.section(&target), text_of("name")) {
        (Some(current), Some(raw))
            if is_root_own(current, root(&seen.detail.sections).as_deref()) =>
        {
            let name = valid_section_name(&raw)?;
            (name != current.name).then(|| (current.name.clone(), name))
        }
        _ => None,
    };
    // Named in a branch, it would change main and every branch: not taken there, said where it is done.
    if let Some((from, _)) = &top_rename
        && crate::write::is_branch_tree(&scope.tree)
    {
        return Err(text!("sections.error.renameInBranch", from));
    }
    let mut edit = Edit {
        name: if top_rename.is_some() {
            None
        } else {
            text_of("name")
        },
        description: text_of("description"),
        position: given
            .get("position")
            .map(|v| v.as_f64().map_or(0, |n| n.trunc() as i64)),
        kind: text_of("type"),
        figure: given.get("figure").cloned(),
        icon: given.get("icon").cloned(),
        ..Edit::default()
    };
    if let Some(kind) = &edit.kind
        && project.defs.section(kind).is_none()
    {
        return Err(type_refused(kind));
    }
    // A working directory's sections are read from a folder: their mark is their kind's.
    if edit.icon.is_some()
        && matches!(
            project.section_kind(&target),
            Some(crate::kinds::WORKDIR | crate::kinds::DIRECTORY)
        )
    {
        return Err(text!(
            "sections.error.iconWorkdir",
            path = project.directory.full_section(&target)
        ));
    }
    if let Some(raw) = given.get("parent").and_then(Value::as_str) {
        edit.parent = Some(if raw.trim_matches([' ', '\t']).is_empty() {
            None
        } else {
            Some(super::section_id(&project, raw)?)
        });
    }
    let places = given.get("places");
    // The diagram's groups: made, changed, moved with what they hold, or let go of.
    let grouping = given.get("groups");
    // The boxes named moved together, or arranged in columns.
    let moving = given.get("move");
    let arranging = given.get("arrange");
    let tidy = match given.get("tidy") {
        None | Some(Value::Bool(false)) => false,
        Some(Value::Bool(true)) => true,
        Some(_) => return Err(text!("sections.error.tidyBool")),
    };
    let edits = edit.name.is_some()
        || edit.description.is_some()
        || edit.position.is_some()
        || edit.parent.is_some()
        || edit.kind.is_some()
        || edit.figure.is_some()
        || edit.icon.is_some();
    if !edits
        && top_rename.is_none()
        && places.is_none()
        && grouping.is_none()
        && moving.is_none()
        && arranging.is_none()
        && !tidy
    {
        return Err(text!("sections.error.nothingToChange"));
    }
    drop(seen);
    // A kind changed too: what it says is that no path had to follow.
    let renamed_or_moved = edit.name.is_some() || edit.parent.is_some() || edit.kind.is_some();
    let rewritten = if edits {
        update_section(tx, scope, &target, &edit)?
    } else {
        0
    };
    if let Some((from, to)) = &top_rename {
        if from == EVACUATION {
            return Err(text!(
                "sections.error.evacuationRename",
                evacuation = EVACUATION
            ));
        }
        super::own_kinds::rename_top(tx, &scope.project, from, to)?;
    }
    // The boxes on its diagram, put in one go (DiagramBoxes).
    let placed = places
        .map(|raw| super::boxes::put_places(tx, scope, &target, raw))
        .transpose()?;
    let grouped = grouping
        .map(|raw| {
            super::groups::put_groups(tx, scope, super::groups::MapOf::Section(&target), raw)
        })
        .transpose()?;
    let moved = moving
        .map(|raw| super::boxes::move_boxes(tx, scope, &target, raw))
        .transpose()?;
    let arranged = arranging
        .map(|raw| super::boxes::arrange_boxes(tx, scope, &target, raw))
        .transpose()?;
    // Then the boxes kept where they are, moved down only where they overlap or a card grows.
    let tidied = if tidy {
        super::tidy::tidy(tx, scope, &target)?
    } else {
        Vec::new()
    };
    let after = Seen::in_tx(tx, scope)?;
    let mut out = Map::new();
    out.insert("project".into(), scope.project_name.clone().into());
    out.insert(
        "section".into(),
        after.project().directory.full_section(&target).into(),
    );
    if renamed_or_moved {
        out.insert("rewrittenPaths".into(), rewritten.into());
    }
    out.extend(row(&after, &target));
    if let Some((count, jumped)) = placed {
        out.insert("placed".into(), count.into());
        if !jumped.is_empty() {
            out.insert("jumped".into(), Value::Array(jumped));
        }
    }
    for said in [grouped, moved, arranged].into_iter().flatten() {
        for (key, value) in said {
            // What groups carried and what `move` moved, said together.
            match (out.get_mut(&key), value) {
                (Some(Value::Array(had)), Value::Array(more)) => had.extend(more),
                (_, value) => {
                    out.insert(key, value);
                }
            }
        }
    }
    if tidy {
        out.insert("tidied".into(), Value::Array(tidied));
    }
    Ok(Value::Object(out))
}

/// The kind `id` and each section inside it come to when `id` becomes `kind`: a section inside keeps
/// its own where the kind of the one it is in lists it (`sections`), else takes that one's first. The
/// sections as they were (`before`); `id` first, then each level below it.
fn kinds_inside(defs: &Defs, before: &[Section], id: &str, kind: &str) -> Vec<(String, String)> {
    let mut out = vec![(id.to_string(), kind.to_string())];
    let mut at = 0;
    while at < out.len() {
        let (parent, parent_kind) = out[at].clone();
        let made_inside: Vec<String> = defs
            .section(&parent_kind)
            .map(|d| d.sections.clone())
            .unwrap_or_else(|| vec![parent_kind.clone()]);
        for child in before
            .iter()
            .filter(|s| s.parent_id.as_deref() == Some(parent.as_str()))
        {
            if out.iter().any(|(s, _)| *s == child.id) {
                continue;
            }
            let own = child
                .kind
                .clone()
                .filter(|k| made_inside.contains(k))
                .or_else(|| made_inside.first().cloned())
                .unwrap_or_else(|| parent_kind.clone());
            out.push((child.id.clone(), own));
        }
        at += 1;
    }
    out
}

/// A section changed (`Repository.updateSection`): moved, renamed, described, put
/// elsewhere among its siblings, made its new parent's kind, its export default replaced;
/// the paths that named it follow. Returns how many paths were rewritten.
fn update_section(tx: &mut Tx, scope: &Scope, id: &str, edit: &Edit) -> Result<usize, String> {
    let seen = Seen::in_tx(tx, scope)?;
    let project = seen.project();
    let current = seen
        .section(id)
        .cloned()
        .ok_or_else(|| text!("sections.error.missing"))?;
    let before = seen.detail.sections.clone();
    let before_detail = seen.was();
    let defs = project.defs.clone();
    let old_full = project.directory.full_section(id);
    let main_root = root(&before);
    let board = scope.tree == BOARD;
    let mut doc = section_doc(tx, scope, id)?;
    let mut changed: Vec<crate::text::Keyed> = Vec::new();

    let mut parent_id = current.parent_id.clone();
    // `Some(None)` is "directly under the root": the top is only roots.
    if let Some(wanted) = &edit.parent {
        let target = wanted.clone().or_else(|| main_root.clone());
        if target != current.parent_id {
            if Some(id) == main_root.as_deref() {
                return Err(text!("sections.error.mainRootFixed"));
            }
            if is_root_own(&current, main_root.as_deref()) {
                return Err(text!(
                    "sections.error.rootOwnMove",
                    name = current.name,
                    tree = tree_of(board).said(),
                    kind = label_of(&defs, current.kind.as_deref())
                ));
            }
            if target == main_root {
                return Err(not_at_the_top(&defs, &before, board));
            }
            if let Some(t) = &target {
                if !before.iter().any(|s| &s.id == t) {
                    return Err(text!("sections.error.parentMissing"));
                }
                // Not into itself, nor into anything inside it.
                let mut at = Some(t.clone());
                while let Some(here) = at {
                    if here == id {
                        return Err(text!("sections.error.intoItself"));
                    }
                    at = before
                        .iter()
                        .find(|s| s.id == here)
                        .and_then(|s| s.parent_id.clone());
                }
            }
            if action_place(current.kind.as_deref()) {
                return Err(action_place_fixed(
                    &defs,
                    &current.name,
                    current.kind.as_deref().unwrap_or_default(),
                ));
            }
            let order = next_order(&seen, target.as_deref());
            set(
                &mut doc,
                "parent_id",
                target.clone().map_or(Value::Null, Value::from),
            );
            set(&mut doc, "sort_order", order.into());
            changed.push(match target.as_deref() {
                Some(t) => keyed!(
                    "sections.change.movedInto",
                    place = project.directory.full_section(t)
                ),
                None => keyed!("sections.change.movedToTop"),
            });
            parent_id = target;
        }
    }
    let mut name = current.name.clone();
    if let Some(raw) = &edit.name {
        name = valid_section_name(raw)?;
        if name != current.name {
            if reserved_name(&name) {
                return Err(reserved_refused(&name));
            }
            doc.insert("name".into(), name.clone().into());
            changed.push(keyed!("sections.change.renamed", name));
        }
    }
    if parent_id != current.parent_id || name != current.name {
        let siblings: Vec<&Section> = before.iter().filter(|s| s.parent_id == parent_id).collect();
        require_name_free(&name, &siblings, Some(id))?;
        require_no_shadowing(&name, parent_id.as_deref(), &before, Some(id))?;
    }
    if let Some(raw) = &edit.description {
        let described = text(raw, &text!("common.label.description"), 2000)?;
        if described != current.description {
            set(&mut doc, "description", described.into());
            changed.push(keyed!("common.label.description"));
        }
    }
    // The kind it comes to: the one `type` names, which its parent's kind lists; else, moved, its own
    // still where its new parent's kind lists that (`sections`), else the parent's first.
    let parent_kind = before
        .iter()
        .find(|s| s.id.as_str() == parent_id.as_deref().unwrap_or_default())
        .and_then(|s| s.kind.clone());
    let mut kind = current.kind.clone();
    if let Some(wanted) = &edit.kind {
        if Some(wanted) != current.kind.as_ref() {
            if parent_id.is_none() {
                return Err(text!("sections.error.rootKind"));
            }
            if is_root_own(&current, main_root.as_deref()) {
                return Err(text!(
                    "sections.error.rootOwnKind",
                    name = current.name,
                    tree = tree_of(board).said(),
                    kind = label_of(&defs, current.kind.as_deref())
                ));
            }
        }
        allowed_inside(&defs, parent_kind.as_deref(), wanted)?;
        kind = Some(wanted.clone());
    } else if let Some(parent_kind) = &parent_kind {
        let made_inside: Vec<String> = defs
            .section(parent_kind)
            .map(|d| d.sections.clone())
            .unwrap_or_else(|| vec![parent_kind.clone()]);
        let keeps = kind.as_ref().is_some_and(|k| made_inside.contains(k));
        if !keeps {
            kind = made_inside.first().cloned().or(Some(parent_kind.clone()));
        }
    }
    if let Some(given) = &edit.figure {
        let figure = figure_of(&defs, &old_full, kind.as_deref(), given)?;
        if figure != current.figure {
            changed.push(keyed!(
                "sections.change.figure",
                figure = figure.as_deref().unwrap_or("system")
            ));
        }
        set(&mut doc, "figure", figure.map_or(Value::Null, Value::from));
    }
    if let Some(given) = &edit.icon {
        let had = doc.get("icon").and_then(Value::as_object).cloned();
        let icon = icon_of(&old_full, kind.as_deref(), had.as_ref(), given)?;
        if icon != had {
            changed.push(keyed!("sections.change.icon"));
        }
        set(&mut doc, "icon", icon.map_or(Value::Null, Value::Object));
    }
    drop(seen);
    tx.put_section(&scope.project, &scope.tree, &doc)?;

    if let Some(position) = edit.position {
        // What is directly under a root keeps the project's order (`project_tree`).
        if parent_id == main_root {
            return Err(text!(
                "sections.error.topOrder",
                tree = tree_of(board).said()
            ));
        }
        let now = Seen::in_tx(tx, scope)?;
        let mut siblings: Vec<String> = now
            .detail
            .sections
            .iter()
            .filter(|s| s.parent_id == parent_id)
            .map(|s| s.id.clone())
            .collect();
        if position < 0 || position >= siblings.len() as i64 {
            return Err(text!(
                "sections.error.position",
                max = siblings.len() as i64 - 1,
                position
            ));
        }
        if let Some(from) = siblings.iter().position(|s| s == id)
            && from as i64 != position
        {
            let moving = siblings.remove(from);
            siblings.insert(position as usize, moving);
            drop(now);
            let mut docs = section_docs(tx, scope)?;
            for (index, sibling) in siblings.iter().enumerate() {
                if let Some(mut sibling_doc) = docs.remove(sibling) {
                    set(&mut sibling_doc, "sort_order", (index as i64).into());
                    tx.put_section(&scope.project, &scope.tree, &sibling_doc)?;
                }
            }
            changed.push(keyed!("sections.change.order"));
        }
    }

    // What it holds: each section inside keeps its kind where the kind of the section it is in
    // lists it (`sections`), else takes that one's first — level by level, from the top down.
    if let Some(k) = &kind
        && kind != current.kind
    {
        if action_place(Some(k)) {
            return Err(action_place_fixed(&defs, &current.name, k));
        }
        if parent_id.is_none() {
            return Err(text!("sections.error.rootKind"));
        }
        if is_root_own(&current, main_root.as_deref()) {
            return Err(text!(
                "sections.error.rootOwnKind",
                name = current.name,
                tree = tree_of(board).said(),
                kind = label_of(&defs, current.kind.as_deref())
            ));
        }
        let kinds = kinds_inside(&defs, &before, id, k);
        let misfits: Vec<String> = before_detail
            .detail
            .schemas
            .iter()
            .filter_map(|c| {
                let at = c.section_id.as_deref().unwrap_or_default();
                let (_, kind) = kinds.iter().find(|(s, _)| s == at)?;
                (!defs.holds(kind, &c.kind))
                    .then(|| text!("sections.part.nameWithKind", name = c.name, kind = c.kind))
            })
            .collect();
        if !misfits.is_empty() {
            let more = if misfits.len() > 5 {
                text!("sections.part.more", count = misfits.len() - 5)
            } else {
                String::new()
            };
            return Err(text!(
                "sections.error.misfits",
                path = old_full,
                held = crate::text::List::of(&misfits[..misfits.len().min(5)]),
                more,
                kind = section_label(&defs, k)
            ));
        }
        let mut docs = section_docs(tx, scope)?;
        for (section, inner_kind) in &kinds {
            if let Some(mut inner) = docs.remove(section) {
                let had = inner.get("kind").and_then(Value::as_str) == Some(inner_kind.as_str());
                let figured = inner.contains_key("figure") && inner_kind != crate::kinds::ACTOR;
                if had && !figured {
                    continue;
                }
                inner.insert("kind".into(), inner_kind.clone().into());
                // What an actor is goes with its being one.
                if inner_kind != crate::kinds::ACTOR {
                    inner.shift_remove("figure");
                }
                tx.put_section(&scope.project, &scope.tree, &inner)?;
            }
        }
        changed.push(if edit.kind.is_some() {
            keyed!("sections.change.type", kind = section_label(&defs, k))
        } else {
            keyed!("sections.change.kind", kind = section_label(&defs, k))
        });
    }

    // A new name or a new parent changes the path of this section and of everything
    // inside it; the paths written against the old ones follow.
    let mut rewritten = 0;
    let new_full = Seen::in_tx(tx, scope)?.project().directory.full_section(id);
    if old_full != new_full {
        rewritten = follow(tx, scope, &before_detail, &InnerMoves::new())?;
        if rewritten > 0 {
            changed.push(keyed!("sections.change.rewritten", rewritten));
        }
    }
    // The pieces joined with " / " in every language, so the one line stays a single kept text.
    if let Some(changed) = changed
        .into_iter()
        .reduce(|head, rest| keyed!("sections.line.joined", head, rest))
    {
        tx.line(
            &scope.project,
            &scope.tree,
            "section.update",
            "section",
            Some(id),
            keyed!("sections.line.update", path = old_full, changed),
        )?;
    }
    Ok(rewritten)
}

// MARK: - section_delete

pub(crate) fn section_delete(tx: &mut Tx, scope: &Scope, args: &Args) -> Result<Value, String> {
    let list = args
        .targets()?
        .ok_or_else(|| text!("sections.error.targetRequired"))?;
    super::writes::each_atomically(tx, scope, &list, |tx, scope, path| {
        let seen = Seen::in_tx(tx, scope)?;
        refuse_settings_as_section(&seen, path)?;
        let project = seen.project();
        let id = super::section_id(&project, path)?;
        let full = project.directory.full_section(&id);
        drop(seen);
        delete_section(tx, scope, &id)?;
        let mut out = Map::new();
        out.insert("project".into(), scope.project_name.clone().into());
        out.insert("deleted".into(), full.into());
        Ok(Value::Object(out))
    })
}

/// Only an empty one — no cards, no sections inside — and none of the root's own
/// (`Repository.deleteSection`): what is in a section is the project's substance. Its
/// siblings close up.
fn delete_section(tx: &mut Tx, scope: &Scope, id: &str) -> Result<(), String> {
    let seen = Seen::in_tx(tx, scope)?;
    let project = seen.project();
    let sections = &seen.detail.sections;
    let Some(current) = seen.section(id).cloned() else {
        return Ok(());
    };
    let path = project.directory.full_section(id);
    if sections.len() <= 1 {
        return Err(text!("sections.error.lastSection"));
    }
    if current.parent_id.is_none() {
        return Err(text!("sections.error.rootDelete"));
    }
    // What is directly under a root is the project's: taken out of `project_tree`, not deleted here —
    // but `_evacuation_` once it is empty.
    if is_root_own(&current, root(sections).as_deref()) && current.name != EVACUATION {
        return Err(text!(
            "sections.error.rootOwnDelete",
            name = current.name,
            tree = tree_of(scope.tree == BOARD).said(),
            kind = label_of(&project.defs, current.kind.as_deref()),
            evacuation = EVACUATION
        ));
    }
    if action_place(current.kind.as_deref()) {
        return Err(action_place_fixed(
            &project.defs,
            &current.name,
            current.kind.as_deref().unwrap_or_default(),
        ));
    }
    let count = project.directory.members(id).len();
    if count > 0 {
        return Err(text!("sections.error.hasSchemas", path, count));
    }
    // Its card ghosts are cards it holds too.
    let ghosts = project.directory.ghosts_in(id).len();
    if ghosts > 0 {
        return Err(text!("sections.error.hasGhosts", path, ghosts));
    }
    let inside = seen.children(Some(id)).len();
    if inside > 0 {
        return Err(text!("sections.error.hasSections", path, inside));
    }
    let siblings: Vec<String> = sections
        .iter()
        .filter(|s| s.id != id && s.parent_id == current.parent_id)
        .map(|s| s.id.clone())
        .collect();
    drop(seen);
    tx.remove_section(&scope.project, &scope.tree, id)?;
    let mut docs = section_docs(tx, scope)?;
    for (index, sibling) in siblings.iter().enumerate() {
        if let Some(mut doc) = docs.remove(sibling) {
            set(&mut doc, "sort_order", (index as i64).into());
            tx.put_section(&scope.project, &scope.tree, &doc)?;
        }
    }
    tx.line(
        &scope.project,
        &scope.tree,
        "section.delete",
        "section",
        Some(id),
        keyed!("sections.line.delete", path),
    )
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use crate::Core;
    use crate::mcp::Output;
    use crate::write::Who;
    use serde_json::json;

    /// The sample project (`tests/fixtures/projects/sample.json`), in a store of its own.
    pub(in crate::tools) fn script_core() -> Core {
        Core::open(crate::fixtures::sample()).unwrap()
    }

    /// A tool called as MCP calls it, its answer as JSON.
    pub(in crate::tools) fn call(
        core: &Core,
        tool: &str,
        arguments: Value,
    ) -> Result<Value, String> {
        let arguments = arguments.as_object().cloned().unwrap_or_default();
        core.tool(&Who::mcp(None), tool, &arguments)
            .map(|out| match out {
                Output::Json(v) => v,
                Output::Blocks(b) => Value::String(b.join("\n")),
                Output::Image { note, .. } => note,
            })
    }

    /// The paths of the sections a tree has.
    fn listed(core: &Core, branch: &str) -> Vec<String> {
        call(
            core,
            "section_list",
            json!({"project": "見本", "branch": branch}),
        )
        .unwrap()["sections"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s["section"].as_str().unwrap().to_string())
            .collect()
    }

    #[test]
    fn a_rename_follows_the_paths_that_named_it() {
        let core = script_core();
        let out = call(
            &core,
            "section_update",
            json!({"project": "見本", "target": "Schemas/Ordering", "value": {"name": "Orderings"}}),
        )
        .unwrap();
        assert_eq!(out["section"], "Schemas/Orderings");
        let rewritten = out["rewrittenPaths"].as_u64().unwrap();
        assert!(rewritten > 0);
        // The flow's nodes are cards inside it: what their lines and paths name.
        let checkout = call(
            &core,
            "card_list",
            json!({"project": "見本", "value": {"in": "Charts/Shop:Checkout", "limit": 200}}),
        )
        .unwrap()
        .to_string();
        assert!(checkout.contains("Schemas/Orderings:Order"));
        assert!(!checkout.contains("Schemas/Ordering:"));
        let history = call(
            &core,
            "branch_read",
            json!({"project": "見本", "target": "main", "value": {"as": "history", "limit": 1}}),
        )
        .unwrap();
        assert_eq!(
            history["commits"][0]["lines"][0],
            format!(
                "セクション Schemas/Ordering を更新（名前を Orderings に / パス {rewritten} か所を書き換え）"
            )
        );
    }

    #[test]
    fn the_roots_own_sections_stay_where_they_are() {
        let core = script_core();
        let refused = |tool: &str, value: Value| call(&core, tool, value).unwrap_err();
        assert_eq!(
            refused(
                "section_update",
                json!({"project": "見本", "target": "Schemas", "value": {"parent": "Charts"}})
            ),
            "Schemas は main・branch の直下のスキーマセクションなので動かせません（名前と説明は変えられます。並びと外すのは project_tree で）"
        );
        assert_eq!(
            refused(
                "section_delete",
                json!({"project": "見本", "target": "Docs"})
            ),
            "Docs は main・branch の直下のドキュメントセクションなので、ここでは消せません（名前は変えられます。外すのは project_tree で: 空なら消え、中身があれば _evacuation_ へ退避します）"
        );
        assert_eq!(
            refused(
                "section_update",
                json!({"project": "見本", "target": "Schemas", "value": {"position": 0}})
            ),
            "main・branch の直下のセクションの並びはプロジェクトの project_tree で決まります（project_update の value の project_tree で並べ替えます）"
        );
        assert_eq!(
            refused(
                "section_update",
                json!({"project": "見本", "target": "Schemas/Ordering", "value": {"position": 5}})
            ),
            "position は 0〜0 です（5）"
        );
        assert_eq!(
            refused(
                "section_update",
                json!({"project": "見本", "target": "Schemas/Ordering", "value": {"type": "chart"}})
            ),
            "スキーマセクションの中に作れるのは schema です（chart は作れません）"
        );
        assert_eq!(
            refused(
                "section_update",
                json!({"project": "見本", "target": "Schemas/Ordering", "value": {}})
            ),
            "変更する項目がありません（name / parent / description / position / type / figure / icon / places / move / arrange / tidy）"
        );
        assert_eq!(
            refused(
                "section_update",
                json!({"project": "見本", "target": "Schemas/Ordering", "value": {"parent": "Schemas/Ordering"}})
            ),
            "セクションを自分の中へは移せません"
        );
    }

    #[test]
    fn a_section_moved_takes_its_new_parents_kind() {
        let core = script_core();
        assert_eq!(
            call(
                &core,
                "section_update",
                json!({"project": "見本", "target": "Schemas/Ordering", "value": {"parent": "Charts"}}),
            )
            .unwrap_err(),
            "セクション Schemas/Ordering には Order（entity）・Line（value） があるのでチャートセクションにできません。先にほかのセクションへ移してください"
        );
        call(
            &core,
            "section_create",
            json!({"project": "見本", "target": "Free/Draft"}),
        )
        .unwrap();
        let moved = call(
            &core,
            "section_update",
            json!({"project": "見本", "target": "Free/Draft", "value": {"parent": "Charts"}}),
        )
        .unwrap();
        assert_eq!(moved["section"], "Charts/Draft");
        assert_eq!(moved["type"], "chart");
        assert_eq!(moved["rewrittenPaths"], 0);
        assert!(listed(&core, "main").contains(&"Charts/Draft".to_string()));
    }

    #[test]
    fn card_ghosts_are_cards_now_and_a_section_keeps_no_list_of_them() {
        let core = script_core();
        let update = |value: Value| {
            call(
                &core,
                "section_update",
                json!({"project": "見本", "target": "Schemas/Ordering", "value": value}),
            )
        };
        // A card ghost is a card of its own now: the list a section
        // kept is refused, with how to make one.
        assert_eq!(
            update(json!({"cardGhosts": ["Schemas:Customer"]})).unwrap_err(),
            card_ghosts_gone()
        );
        // The key before the names were settled on is refused, the keys it takes
        // said.
        let before = update(json!({"ghosts": ["Schemas:Customer"]})).unwrap_err();
        assert_eq!(
            before,
            "value に使えないキーです: ghosts。使えるのは: name / parent / description / position / places / move / arrange / tidy / readType / exclude / folder / parsers / groups / type / figure / icon（形は guide の about: section_update）"
        );
    }

    #[test]
    fn several_targets_are_tried_together_and_kept_only_all_together() {
        let core = script_core();
        let refused = call(
            &core,
            "section_create",
            json!({"project": "見本", "target": ["Schemas/A", "Billing", "Schemas/A"]}),
        )
        .unwrap_err();
        assert_eq!(
            refused,
            "2 件の誤りがあるので、どれも書いていません\n1. Billing: main・branch の直下にはセクションを作れません。直下のセクション（Docs（ドキュメント）・Schemas（スキーマ）・Charts（チャート）・Free（フリー）・ts（作業ディレクトリ）・zod（作業ディレクトリ）・schema（作業ディレクトリ）・charts_2（作業ディレクトリ）・docs_2（作業ディレクトリ））はプロジェクトの project_tree で決まり（作る・外す・並べ替える・名前を変えるのは project_update の value の project_tree）、ほかのセクションはその中に作ります。種類は入れるセクションで決まります\n2. Schemas/A: セクション「A」は同じ所にもうあります"
        );
        assert!(!listed(&core, "main").contains(&"Schemas/A".to_string()));
        let made = call(
            &core,
            "section_create",
            json!({"project": "見本", "target": ["Schemas/A", "Schemas/A/B"]}),
        )
        .unwrap();
        assert_eq!(made["results"][1]["result"]["created"], "Schemas/A/B");
        let deleted = call(
            &core,
            "section_delete",
            json!({"project": "見本", "target": "Schemas/A"}),
        )
        .unwrap_err();
        assert_eq!(
            deleted,
            "セクション Schemas/A の中にセクションが 1 つあります。先に消すか、別のセクションへ移してください"
        );
    }

    #[test]
    fn a_branch_keeps_the_sections_it_makes() {
        let core = script_core();
        call(
            &core,
            "branch_create",
            json!({"project": "見本", "target": "work"}),
        )
        .unwrap();
        let made = call(
            &core,
            "section_create",
            json!({"project": "見本", "branch": "work", "target": "Schemas/Billing"}),
        )
        .unwrap();
        assert_eq!(made["branch"], "work");
        assert!(listed(&core, "work").contains(&"Schemas/Billing".to_string()));
        assert!(!listed(&core, "main").contains(&"Schemas/Billing".to_string()));
        call(
            &core,
            "section_delete",
            json!({"project": "見本", "branch": "work", "target": "Schemas/Billing"}),
        )
        .unwrap();
        assert!(!listed(&core, "work").contains(&"Schemas/Billing".to_string()));
    }

    #[test]
    fn the_board_holds_its_goals_section_alone_at_the_top() {
        let core = script_core();
        let board =
            |name: &str| json!({"project": "見本", "branch": "_project_board_", "target": name});
        let refused = call(&core, "section_create", board("Projects")).unwrap_err();
        assert!(
            refused.starts_with(
                "ボードの直下にはセクションを作れません。直下のセクション（Goals（ボード））はプロジェクトの project_tree で決まり"
            ),
            "{refused}"
        );
        let made = call(&core, "section_create", board("Goals/Projects")).unwrap();
        assert_eq!(made["created"], "Goals/Projects");
        assert_eq!(made["type"], "goals");
        let refused = call(&core, "section_delete", board("Goals")).unwrap_err();
        assert_eq!(
            refused,
            "Goals は ボードの直下のボードセクションなので、ここでは消せません（名前は変えられます。外すのは project_tree で: 空なら消え、中身があれば _evacuation_ へ退避します）"
        );
        let mut moved = board("Goals/Projects");
        moved["value"] = json!({"parent": "Board"});
        let refused = call(&core, "section_update", moved).unwrap_err();
        assert!(
            refused.starts_with("ボードの直下にはセクションを作れません"),
            "{refused}"
        );
    }

    #[test]
    fn a_tree_written_before_stays_with_the_parent() {
        assert_eq!(
            split_tree("main@Store/Ordering"),
            (Some("main"), "Store/Ordering")
        );
        assert_eq!(split_tree("Store/Ordering"), (None, "Store/Ordering"));
    }

    #[test]
    fn a_key_at_its_fallback_is_left_out() {
        let mut doc = Map::new();
        set(&mut doc, "sort_order", 0.into());
        set(&mut doc, "description", "".into());
        set(&mut doc, "ghosts", Value::Array(Vec::new()));
        set(&mut doc, "parent_id", Value::Null);
        assert!(doc.is_empty());
        set(&mut doc, "sort_order", 2.into());
        assert_eq!(doc["sort_order"], 2);
    }
}
