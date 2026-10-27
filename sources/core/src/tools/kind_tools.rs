//! The kinds' tools: the kinds of card,
//! of section and of line the project has, each in a line of what it is for (`kind_list`), one's
//! definition whole (`kind_read`), and the project's own made, changed and let go (`kind_create`,
//! `kind_update`, `kind_delete`) — a built-in kind's context (what it is for, when to use it and
//! not) written over by the project with `kind_update` too. A kind is named by its name alone: a
//! name is one kind's among the three lists.
//!
//! The project's settings, as its working directories are: not a branch's, a line in main's
//! history, written by what wrote them before (`own_kinds::set_kinds`), checked whole beside the
//! built-in ones; `dryRun` says what writing would be, nothing written.

use super::args::Args;
use super::cards::Scope;
use super::own_kinds::{kept, kinds_view, name_key, set_kinds};
use crate::kinds::Defs;
use crate::store::sql;
use crate::write::Tx;
use rusqlite::Connection;
use serde_json::{Map, Value, json};

/// The lists, each with what `of` calls a kind of it.
pub(crate) const OF: [(&str, &str); 3] = [
    ("card", "cards"),
    ("section", "sections"),
    ("line", "lines"),
];

/// How a refusal or a history line calls a kind of a list.
fn said(list: &str) -> crate::text::Keyed {
    match list {
        "cards" => keyed!("kindTools.kind.cards"),
        "sections" => keyed!("kindTools.kind.sections"),
        _ => keyed!("kindTools.kind.lines"),
    }
}

/// The project's own kinds as its row keeps them, as text.
fn own_text(conn: &Connection, project: &str) -> Result<Option<String>, String> {
    conn.query_row("SELECT kinds FROM projects WHERE id = ?", [project], |r| {
        r.get(0)
    })
    .map_err(sql)
}

/// Whether `name` is a built-in kind of the list (a kind of card others only take after is none).
pub(crate) fn builtin_in(list: &str, name: &str) -> bool {
    match list {
        "cards" => crate::kinds::builtin_definition(name).is_some_and(|d| d["abstract"] != true),
        _ => crate::kinds::builtin_written(list)
            .iter()
            .any(|d| d[name_key(list)] == name),
    }
}

/// Where a name is: its list, whether it is built in, and (the project's own) its place there.
struct Found {
    list: &'static str,
    builtin: bool,
    at: usize,
}

/// Every kind a name names, in the lists' order.
fn found(own: &Map<String, Value>, name: &str) -> Vec<Found> {
    let mut out = Vec::new();
    for (_, list) in OF {
        if builtin_in(list, name) {
            out.push(Found {
                list,
                builtin: true,
                at: 0,
            });
        }
        if let Some(at) = own
            .get(list)
            .and_then(Value::as_array)
            .and_then(|items| items.iter().position(|d| d[name_key(list)] == name))
        {
            out.push(Found {
                list,
                builtin: false,
                at,
            });
        }
    }
    out
}

/// The one kind a name names, or why not.
fn one(own: &Map<String, Value>, name: &str) -> Result<Found, String> {
    let mut all = found(own, name);
    match all.len() {
        0 => Err(text!("kindTools.none", names = name)),
        1 => Ok(all.remove(0)),
        _ => {
            let join = text!("kindTools.bothJoin");
            Err(text!(
                "kindTools.both",
                name,
                kinds = all
                    .iter()
                    .map(|f| said(f.list).to_string())
                    .collect::<Vec<_>>()
                    .join(join.as_str())
            ))
        }
    }
}

/// A name no kind has yet, in any list; or whose it is.
pub(crate) fn free(own: &Map<String, Value>, name: &str) -> Result<(), String> {
    match found(own, name).first() {
        None => Ok(()),
        Some(f) if f.builtin => Err(text!("kindTools.takenBuiltin", name, kind = said(f.list))),
        Some(f) => Err(text!("kindTools.taken", name, kind = said(f.list))),
    }
}

/// The one name a tool's target names.
fn one_target(args: &Args, tool: &str) -> Result<String, String> {
    match (args.raw.get("target"), args.targets()?) {
        (Some(Value::String(_)), Some(names)) => Ok(names[0].clone()),
        _ => Err(text!("kindTools.target.one", tool)),
    }
}

/// `dryRun` taken out of a value: whether nothing is to be written.
fn dry_run(given: &mut Map<String, Value>) -> Result<bool, String> {
    match given.remove("dryRun") {
        None | Some(Value::Bool(false)) => Ok(false),
        Some(Value::Bool(true)) => Ok(true),
        Some(_) => Err(text!("kindTools.dryRun.shape")),
    }
}

/// What writing would be, as `dryRun` says it: whether it may be written, every problem with it,
/// and what each rename moves.
fn dried(checked: Value) -> Value {
    let mut out = Map::new();
    out.insert("dryRun".into(), true.into());
    for key in ["ok", "problems", "renamed", "projectTree"] {
        if let Some(v) = checked.get(key) {
            out.insert(key.into(), v.clone());
        }
    }
    Value::Object(out)
}

// MARK: - Read

/// `kind_list`: every kind the project has — the built-in ones it has a place for, then its own
/// (`own: true`) — each in a line: a kind of card or section by its name, how people call it, the
/// first sentence of what it is for (`purpose`, a kind of section's `description` when its context
/// says none) and where it goes (a kind of card the sections and containers that name it, `free`
/// taking any; a kind of section the sections it is made in and the roots it sits under); a kind of
/// line by its name and the kinds it leaves and reaches. When to use each and the rest of what it is
/// for are `kind_read`'s and `guide`'s, so the list stays short enough to read whole.
pub(crate) fn kind_list(conn: &Connection, project: &str, name: &str) -> Result<Value, String> {
    let own = own_text(conn, project)?;
    let view = kinds_view(name, own.as_deref(), None)?;
    let defs = Defs::of_project(own.as_deref());
    let mut out = Map::new();
    out.insert("project".into(), name.into());
    for (_, list) in OF {
        let items: Vec<Value> = view["kinds"][list]
            .as_array()
            .into_iter()
            .flatten()
            .map(|d| listed(list, d, &defs))
            .collect();
        out.insert(list.into(), Value::Array(items));
    }
    Ok(Value::Object(out))
}

/// One kind in `kind_list`'s line.
fn listed(list: &str, d: &Value, defs: &Defs) -> Value {
    let key = name_key(list);
    let name = d[key].as_str().unwrap_or_default();
    let mut out = Map::new();
    out.insert(key.into(), name.into());
    if d.get("builtin") != Some(&Value::Bool(true)) {
        out.insert("own".into(), true.into());
    }
    if let Some(label) = d.get("label") {
        out.insert("label".into(), label.clone());
    }
    let context = d.get("context").and_then(Value::as_object);
    let purpose = context
        .and_then(|c| c.get("purpose"))
        .or_else(|| d.get("description"))
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty());
    if let Some(purpose) = purpose {
        out.insert("purpose".into(), crate::mcp::brief(purpose).into());
    }
    if made_by_core(list, name) {
        out.insert("systemOnly".into(), true.into());
    }
    for key in ["contextChanged", "iconChanged", "viasChanged", "noPlace"] {
        if d.get(key) == Some(&Value::Bool(true)) {
            out.insert(key.into(), true.into());
        }
    }
    match list {
        "cards" => {
            let mut held: Vec<String> = defs
                .sections
                .iter()
                .filter(|s| s.holds.iter().any(|h| h == name))
                .map(|s| s.name.clone())
                .collect();
            held.extend(
                defs.cards()
                    .all()
                    .iter()
                    .filter(|k| k.holds.iter().any(|h| h == name))
                    .map(|k| k.name.clone()),
            );
            if !held.is_empty() {
                out.insert("in".into(), json!(held));
            }
        }
        "sections" => {
            if let Some(section) = defs.section(name) {
                let parents: Vec<&str> = defs
                    .sections
                    .iter()
                    .filter(|p| p.sections.iter().any(|s| s == name))
                    .map(|p| p.name.as_str())
                    .collect();
                if !parents.is_empty() {
                    out.insert("in".into(), json!(parents));
                }
                let trees: Vec<&str> = section.trees.iter().map(|t| t.name()).collect();
                if !trees.is_empty() {
                    out.insert("trees".into(), json!(trees));
                }
            }
        }
        _ => {
            if let Some(line) = defs.line(name) {
                let mut from: Vec<&str> = Vec::new();
                for kind in line.from.iter().flat_map(|e| e.kinds.iter()) {
                    if !from.contains(&kind.as_str()) {
                        from.push(kind);
                    }
                }
                out.insert("from".into(), json!(from));
                out.insert("to".into(), json!(line.to.kinds));
            }
        }
    }
    Value::Object(out)
}

/// Whether a kind is one only the core makes, never through `card_create` or `section_create`:
/// a card the app alone draws or reads off a working directory, a section the card actions or a
/// working directory make.
fn made_by_core(list: &str, name: &str) -> bool {
    use crate::kinds::{CANCELLED, COMPLETE, DIRECTORY, WORKDIR};
    match list {
        "cards" => crate::kinds::system_only(name),
        "sections" => [COMPLETE, CANCELLED, WORKDIR, DIRECTORY].contains(&name),
        _ => false,
    }
}

/// One kind's definition whole as `kind_read` gives it, with which list it is of (`of`): the
/// built-in ones as built (`builtin: true`, a context the project writes over in place and
/// `contextChanged: true`), the project's own as written (`noPlace: true` for one with no place).
fn definition(view: &Value, name: &str) -> Option<Value> {
    for (of, list) in OF {
        let in_view = view["kinds"][list]
            .as_array()
            .into_iter()
            .flatten()
            .find(|d| d[name_key(list)] == name)
            .cloned();
        // A built-in one the project has no place for: as built.
        let built = || {
            builtin_in(list, name).then(|| {
                let mut d = match list {
                    "cards" => crate::kinds::builtin_definition(name).cloned(),
                    _ => crate::kinds::builtin_written(list)
                        .iter()
                        .find(|d| d[name_key(list)] == name)
                        .cloned(),
                }
                .unwrap_or_default();
                d["builtin"] = true.into();
                d["noPlace"] = true.into();
                d
            })
        };
        if let Some(Value::Object(d)) = in_view.or_else(built) {
            let mut out = Map::new();
            out.insert("of".into(), of.into());
            out.extend(d);
            return Some(Value::Object(out));
        }
    }
    None
}

/// `kind_read`: the definitions `target` names (one, or a list), or every one when it names none.
/// A built-in kind's actions as people read them (`[{op, label, description, value}]`), in place of
/// their names.
fn said_actions(d: &mut Value) {
    let ops: Vec<String> = d["actions"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|o| o.as_str().map(str::to_string))
        .collect();
    if let Some(said) = super::actions::described(&ops)
        && let Value::Object(d) = d
    {
        d.insert("actions".into(), said);
    }
}

pub(crate) fn kind_read(
    conn: &Connection,
    project: &str,
    name: &str,
    args: &Args,
) -> Result<Value, String> {
    if let Some(key) = args.value()?.keys().next() {
        return Err(text!("kindTools.read.valueKey", key));
    }
    let own = own_text(conn, project)?;
    let view = kinds_view(name, own.as_deref(), None)?;
    let mut out = Map::new();
    out.insert("project".into(), name.into());
    let Some(names) = args.targets()? else {
        for (_, list) in OF {
            let mut all = view["kinds"][list].clone();
            if let Value::Array(items) = &mut all {
                items.iter_mut().for_each(said_actions);
            }
            out.insert(list.into(), all);
        }
        return Ok(Value::Object(out));
    };
    let mut read = Vec::new();
    let mut missing = Vec::new();
    for n in &names {
        match definition(&view, n) {
            Some(mut d) => {
                said_actions(&mut d);
                read.push(d)
            }
            None => missing.push(n.clone()),
        }
    }
    if !missing.is_empty() {
        let join = text!("kindTools.read.nameJoin");
        return Err(text!("kindTools.none", names = missing.join(join.as_str())));
    }
    if matches!(args.raw.get("target"), Some(Value::String(_))) {
        if let Some(Value::Object(one)) = read.pop() {
            out.extend(one);
        }
    } else {
        out.insert("kinds".into(), Value::Array(read));
    }
    Ok(Value::Object(out))
}

/// One kind read again after a write, as `kind_read` gives it.
fn read_back(tx: &Tx, scope: &Scope, name: &str) -> Result<Value, String> {
    let own = own_text(tx.conn(), &scope.project)?;
    let view = kinds_view(&scope.project_name, own.as_deref(), None)?;
    let mut out = Map::new();
    out.insert("project".into(), scope.project_name.clone().into());
    if let Some(Value::Object(d)) = definition(&view, name) {
        out.extend(d);
    }
    Ok(Value::Object(out))
}

// MARK: - Write

/// The project's other definitions where they name a kind of card (`kind`): a container's and a
/// section's `holds`, a kind's `extends`, a line's ends' `kinds` — named `to` instead, or let go of
/// (`to` none; a kind that extends it stays, for the check to say). Which definitions changed, each
/// with its list.
fn retarget(
    own: &mut Map<String, Value>,
    kind: &str,
    to: Option<&str>,
) -> Vec<(&'static str, String)> {
    fn in_list(list: Option<&mut Value>, kind: &str, to: Option<&str>) -> bool {
        let Some(Value::Array(items)) = list else {
            return false;
        };
        let before = items.len();
        let mut changed = false;
        match to {
            Some(to) => {
                for item in items.iter_mut().filter(|i| *i == kind) {
                    *item = to.into();
                    changed = true;
                }
            }
            None => items.retain(|i| i != kind),
        }
        changed || items.len() != before
    }
    let mut changed = Vec::new();
    for (_, list) in OF {
        let Some(Value::Array(items)) = own.get_mut(list) else {
            continue;
        };
        for item in items.iter_mut() {
            let Value::Object(d) = item else {
                continue;
            };
            let name = d
                .get(name_key(list))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let mut touched = false;
            match list {
                "cards" | "sections" => {
                    touched |= in_list(d.get_mut("holds"), kind, to);
                    if list == "cards"
                        && d.get("extends").and_then(Value::as_str) == Some(kind)
                        && let Some(to) = to
                    {
                        d.insert("extends".into(), to.into());
                        touched = true;
                    }
                }
                _ => {
                    if let Some(Value::Array(ends)) = d.get_mut("from") {
                        for end in ends.iter_mut() {
                            touched |= in_list(end.get_mut("kinds"), kind, to);
                        }
                    }
                    if let Some(end) = d.get_mut("to") {
                        touched |= in_list(end.get_mut("kinds"), kind, to);
                    }
                }
            }
            if touched {
                changed.push((list, name));
            }
        }
    }
    changed
}

/// The lists `retarget` changed, as `set_kinds` is given them, beside `kinds`.
fn with_retargeted(
    kinds: &mut Map<String, Value>,
    own: &Map<String, Value>,
    changed: &[(&'static str, String)],
) {
    for (list, _) in changed {
        if let Some(items) = own.get(*list) {
            kinds.insert(list.to_string(), items.clone());
        }
    }
}

/// The other definitions a write changed, as its answer says them.
fn changed_said(changed: &[(&'static str, String)]) -> Value {
    Value::Array(
        changed
            .iter()
            .map(|(_, name)| name.clone().into())
            .collect(),
    )
}

/// `kind_create`: a kind of the project's own, named by `target`, of the list `of` says (`card`,
/// `section`, `line`), its definition the rest of `value` — last in its list.
pub(crate) fn kind_create(tx: &mut Tx, scope: &Scope, args: &Args) -> Result<Value, String> {
    let mut given = args.value()?;
    let name = one_target(args, "kind_create")?;
    let dry = dry_run(&mut given)?;
    let of = given
        .remove("of")
        .ok_or_else(|| text!("kindTools.create.ofNeeded"))?;
    let Some(list) = OF
        .iter()
        .find(|(o, _)| Some(*o) == of.as_str())
        .map(|(_, l)| *l)
    else {
        return Err(text!("kindTools.create.ofShape", of));
    };
    let key = name_key(list);
    if let Some(written) = given.remove(key)
        && written.as_str() != Some(name.as_str())
    {
        return Err(text!("kindTools.create.nameInTarget", key));
    }
    let own = kept(tx.conn(), &scope.project)?;
    free(&own, &name)?;
    let mut definition = Map::new();
    definition.insert(key.into(), name.clone().into());
    definition.extend(given);
    let mut items = own
        .get(list)
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    items.push(Value::Object(definition));
    let mut kinds = Map::new();
    kinds.insert(list.into(), Value::Array(items));
    let line = keyed!("kindTools.line.created", kind = said(list), name);
    let out = set_kinds(
        tx,
        &scope.project,
        Some(&Value::Object(kinds)),
        None,
        dry,
        Some(&line),
    )?;
    if dry {
        return Ok(dried(out));
    }
    Ok(json!({ "created": [name] }))
}

/// The project's own values of `via` as `kind_update`'s `vias` leaves them: a list in place of
/// those there, `{add, remove}` changing those there (a value added once, one removed that is not
/// there said), or none with a null. The values are checked with the definitions
/// (`Defs::checked`).
fn vias_given(own: &Map<String, Value>, value: &Value) -> Result<Value, String> {
    let shape = text!("kindTools.vias.shape");
    let names = |v: Option<&Value>| -> Result<Vec<String>, String> {
        match v {
            None | Some(Value::Null) => Ok(Vec::new()),
            Some(Value::Array(items)) => items
                .iter()
                .map(|i| i.as_str().map(str::to_string).ok_or_else(|| shape.clone()))
                .collect(),
            Some(_) => Err(shape.clone()),
        }
    };
    match value {
        Value::Null | Value::Array(_) => Ok(value.clone()),
        Value::Object(edit) => {
            if let Some(key) = edit.keys().find(|k| *k != "add" && *k != "remove") {
                return Err(text!("kindTools.vias.keyNotAllowed", key, shape));
            }
            let mut list = names(own.get("vias"))?;
            for gone in names(edit.get("remove"))? {
                let Some(at) = list.iter().position(|v| *v == gone) else {
                    return Err(if list.is_empty() {
                        text!("kindTools.vias.removeNone", gone)
                    } else {
                        text!(
                            "kindTools.vias.removeAmong",
                            gone,
                            have = crate::text::List::of(&list)
                        )
                    });
                };
                list.remove(at);
            }
            for added in names(edit.get("add"))? {
                if !list.contains(&added) {
                    list.push(added);
                }
            }
            Ok(list.into())
        }
        _ => Err(shape.clone()),
    }
}

/// `kind_update`: a kind of the project's own changed — each key of `value` written in place of its
/// definition's (`null` takes it away), renamed with `name` (a kind of card; its cards follow), and
/// its fields and rows renamed with `renames` (their values moving with them) — or a built-in kind's
/// `context` (and a built-in kind of section's `icon`) written over (`null` gives it back its own).
pub(crate) fn kind_update(tx: &mut Tx, scope: &Scope, args: &Args) -> Result<Value, String> {
    let mut given = args.value()?;
    let name = one_target(args, "kind_update")?;
    let dry = dry_run(&mut given)?;
    if given.is_empty() {
        return Err(text!("kindTools.update.nothing"));
    }
    let own = kept(tx.conn(), &scope.project)?;
    let at = one(&own, &name)?;
    let list = at.list;
    if at.builtin {
        // What a project writes over a built-in kind with: its context (any kind), a kind of
        // section's mark, `icon`, and the flow's values of `via` its nodes may say, `vias`.
        let allowed: &[&str] = if list == "sections" {
            &["context", "icon"]
        } else if name == "flow" {
            &["context", "vias"]
        } else {
            &["context"]
        };
        if let Some(key) = given.keys().find(|k| !allowed.contains(&k.as_str())) {
            let kind = said(list);
            return Err(if list == "sections" {
                text!("kindTools.update.builtinSections", kind, key)
            } else if name == "flow" {
                text!("kindTools.update.builtinFlow", kind, key)
            } else {
                text!("kindTools.update.builtinOther", kind, key)
            });
        }
        let mut kinds = Map::new();
        let mut said_lines: Vec<crate::text::Keyed> = Vec::new();
        if let Some(value) = given.get("vias") {
            kinds.insert("vias".into(), vias_given(&own, value)?);
            said_lines.push(if value.is_null() {
                keyed!("kindTools.line.viasReset")
            } else {
                keyed!("kindTools.line.viasChanged")
            });
        }
        // Each is written over with its value, and given back its own with a null.
        for (key, over, label) in [
            ("context", "contexts", keyed!("kindTools.label.context")),
            ("icon", "icons", keyed!("kindTools.label.icon")),
        ] {
            let Some(value) = given.get(key) else {
                continue;
            };
            let mut written = own
                .get(over)
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            if value.is_null() {
                written.remove(&name);
                said_lines.push(keyed!(
                    "kindTools.line.builtinReset",
                    kind = said(list),
                    name,
                    label
                ));
            } else {
                written.insert(name.clone(), value.clone());
                said_lines.push(keyed!(
                    "kindTools.line.builtinChanged",
                    kind = said(list),
                    name,
                    label
                ));
            }
            kinds.insert(over.into(), Value::Object(written));
        }
        let line = keyed!(
            "kindTools.line.joined",
            items = crate::text::List::of_keyed(said_lines)
        );
        let out = set_kinds(
            tx,
            &scope.project,
            Some(&Value::Object(kinds)),
            None,
            dry,
            Some(&line),
        )?;
        if dry {
            return Ok(dried(out));
        }
        return read_back(tx, scope, &name);
    }
    let key = name_key(list);
    if given.contains_key("of") {
        return Err(text!("kindTools.update.ofFixed"));
    }
    if given.contains_key(key) {
        return Err(text!("kindTools.update.nameKey", key));
    }
    let mut items = own[list].as_array().cloned().unwrap_or_default();
    let mut definition = items[at.at].as_object().cloned().unwrap_or_default();
    let renamed_to = match given.remove("name") {
        None => None,
        Some(Value::String(to)) if to.trim() == name => None,
        Some(Value::String(to)) => Some(to.trim().to_string()),
        Some(_) => return Err(text!("kindTools.update.nameShape")),
    };
    let field_renames = given.remove("renames");
    for (k, v) in given {
        if v.is_null() {
            definition.remove(&k);
        } else {
            definition.insert(k, v);
        }
    }
    let mut renames: Vec<Value> = Vec::new();
    let now = renamed_to.clone().unwrap_or_else(|| name.clone());
    if let Some(to) = &renamed_to {
        if list != "cards" {
            return Err(text!("kindTools.update.renameKind", kind = said(list)));
        }
        free(&own, to)?;
        definition.insert(key.into(), to.clone().into());
        renames.push(json!({ "kind": name, "to": to }));
    }
    match field_renames {
        None | Some(Value::Null) => {}
        Some(_) if list != "cards" => {
            return Err(text!("kindTools.update.renamesCardsOnly"));
        }
        Some(Value::Array(asked)) => {
            for (i, item) in asked.into_iter().enumerate() {
                let Value::Object(mut item) = item else {
                    return Err(text!("kindTools.update.renameItem", i));
                };
                if item.contains_key("kind") || item.contains_key("top_section") {
                    return Err(text!("kindTools.update.renamesKind", i));
                }
                renamed_in(&mut definition, &item);
                item.insert("kind".into(), now.clone().into());
                renames.push(Value::Object(item));
            }
        }
        Some(_) => {
            return Err(text!("kindTools.update.renamesShape"));
        }
    }
    items[at.at] = Value::Object(definition);
    let mut written = own.clone();
    written.insert(list.into(), Value::Array(items.clone()));
    let follow = match &renamed_to {
        Some(to) => retarget(&mut written, &name, Some(to)),
        None => Vec::new(),
    };
    let mut kinds = Map::new();
    kinds.insert(list.into(), written[list].clone());
    with_retargeted(&mut kinds, &written, &follow);
    let line = match &renamed_to {
        Some(to) => keyed!("kindTools.line.updatedRenamed", kind = said(list), name, to),
        None => keyed!("kindTools.line.updated", kind = said(list), name),
    };
    let renames = (!renames.is_empty()).then_some(Value::Array(renames));
    let out = set_kinds(
        tx,
        &scope.project,
        Some(&Value::Object(kinds)),
        renames.as_ref(),
        dry,
        Some(&line),
    )?;
    if dry {
        return Ok(dried(out));
    }
    let mut answer = read_back(tx, scope, &now)?;
    if let Value::Object(answer) = &mut answer {
        if let Some(renamed) = out.get("renamed") {
            answer.insert("renamed".into(), renamed.clone());
        }
        if !follow.is_empty() {
            answer.insert("alsoChanged".into(), changed_said(&follow));
        }
    }
    Ok(answer)
}

/// A rename of `renames` written into the definition too, where it still has the name of before
/// and not the new one: a field (and where the definition names it: `header`, `as_row`, `face`), a
/// kind of row, or a field of a row's.
fn renamed_in(definition: &mut Map<String, Value>, rename: &Map<String, Value>) {
    let text = |key: &str| rename.get(key).and_then(Value::as_str).map(str::to_string);
    let Some(to) = text("to") else {
        return;
    };
    fn rename_named(list: Option<&mut Value>, from: &str, to: &str) {
        let Some(Value::Array(items)) = list else {
            return;
        };
        if items.iter().any(|i| i["name"] == to) {
            return;
        }
        for item in items.iter_mut() {
            if item["name"] == from {
                item["name"] = to.into();
            }
        }
    }
    fn rename_listed(list: Option<&mut Value>, from: &str, to: &str) {
        if let Some(Value::Array(items)) = list {
            for item in items.iter_mut() {
                if item == from {
                    *item = to.into();
                }
            }
        }
    }
    match (text("row"), text("field")) {
        (None, Some(field)) => {
            rename_named(definition.get_mut("fields"), &field, &to);
            rename_listed(definition.get_mut("header"), &field, &to);
            rename_listed(definition.get_mut("as_row"), &field, &to);
            if let Some(Value::Object(face)) = definition.get_mut("face") {
                rename_listed(face.get_mut("top"), &field, &to);
                for key in ["markdown", "svg", "html"] {
                    if face.get(key).and_then(Value::as_str) == Some(field.as_str()) {
                        face.insert(key.into(), to.clone().into());
                    }
                }
            }
        }
        (Some(row), None) => rename_named(definition.get_mut("rows"), &row, &to),
        (Some(row), Some(field)) => {
            if let Some(Value::Array(rows)) = definition.get_mut("rows") {
                for r in rows.iter_mut().filter(|r| r["name"] == row.as_str()) {
                    rename_named(r.get_mut("fields"), &field, &to);
                }
            }
        }
        (None, None) => {}
    }
}

/// `kind_delete`: the project's own kinds `target` names (one, or a list) let go — refused for one
/// still in use, and for a built-in one.
pub(crate) fn kind_delete(tx: &mut Tx, scope: &Scope, args: &Args) -> Result<Value, String> {
    let mut given = args.value()?;
    let dry = dry_run(&mut given)?;
    if let Some(key) = given.keys().next() {
        return Err(text!("kindTools.delete.valueKey", key));
    }
    let names = args
        .targets()?
        .ok_or_else(|| text!("kindTools.delete.targetNeeded"))?;
    let own = kept(tx.conn(), &scope.project)?;
    let mut written = own.clone();
    let mut lists: Vec<&'static str> = Vec::new();
    let mut follow = Vec::new();
    for name in &names {
        let at = one(&own, name)?;
        if at.builtin {
            return Err(text!(
                "kindTools.delete.builtin",
                name,
                kind = said(at.list)
            ));
        }
        if let Some(Value::Array(items)) = written.get_mut(at.list) {
            items.retain(|d| d[name_key(at.list)] != name.as_str());
        }
        if !lists.contains(&at.list) {
            lists.push(at.list);
        }
        if at.list == "cards" {
            follow.extend(retarget(&mut written, name, None));
        }
    }
    follow.retain(|(_, n)| !names.contains(n));
    let mut kinds = Map::new();
    for list in lists {
        kinds.insert(list.into(), written[list].clone());
    }
    with_retargeted(&mut kinds, &written, &follow);
    let line = keyed!(
        "kindTools.line.deleted",
        names = crate::text::List::of(&names)
    );
    let out = set_kinds(
        tx,
        &scope.project,
        Some(&Value::Object(kinds)),
        None,
        dry,
        Some(&line),
    )?;
    if dry {
        return Ok(dried(out));
    }
    let mut answer = json!({ "deleted": names });
    if !follow.is_empty() {
        answer["alsoChanged"] = changed_said(&follow);
    }
    Ok(answer)
}
