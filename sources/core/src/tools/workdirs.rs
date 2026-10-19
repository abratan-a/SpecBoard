//! The working directories' sections as the tools read and write them: what is read in them is
//! the core's (a directory is a section of the kind `directory`, a file a card of the kind `file`)
//! and is not written — only notes are put there, and the places of what is on their maps —;
//! each working directory's section says how and from where it is read (`section_read`), is set
//! by `section_update` (`readType`, `exclude`, `description`: the project's setting; `folder`:
//! this machine's) and is read again by its action (`section_action`'s `read`).

use super::Seen;
use super::args::Args;
use super::cards::Scope;
use crate::export::Project;
use crate::write::Tx;
use rusqlite::Connection;
use serde_json::{Map, Value, json};

/// The keys of `section_update` that set a working directory's section, not the section itself.
pub(crate) const SETTINGS: [&str; 5] = ["readType", "exclude", "parsers", "description", "folder"];

/// The keys of `section_update` that only a working directory's section takes.
const OWN_SETTINGS: [&str; 4] = ["readType", "exclude", "parsers", "folder"];

/// The working directory a section is, by the section's id: its id (`dir:<id>`), when the
/// section is one of the kind `workdir`.
fn dir_of(project: &Project, section: &str) -> Option<String> {
    (project.section_kind(section) == Some(crate::kinds::WORKDIR))
        .then(|| section.strip_prefix("dir:").map(str::to_string))
        .flatten()
}

/// The section a path names, by id.
fn section_at(project: &Project, path: &str) -> Option<String> {
    project
        .directory
        .section_named(path.trim_matches([' ', '\t']), None)
}

/// Whether a section is the core's, read from a working directory: one of the kind `directory`.
fn generated(project: &Project, section: &str) -> bool {
    project.section_kind(section) == Some(crate::kinds::DIRECTORY)
}

/// What a write is refused with before it is tried, when what it writes is read from a working
/// directory: a file (a card of the kind `file`) is not made, changed, tagged or let go of — but
/// for where it sits on its map (`topLeft`) —, a directory is not renamed, moved or let go of, no
/// section is made inside a working directory's section, and the only cards put there are notes.
/// The settings of a working directory are the project's, and a batch does not write them.
pub(crate) fn refuse_in_workdir(
    tx: &Tx,
    scope: &Scope,
    tool: &str,
    args: &Args,
    in_batch: bool,
) -> Result<(), String> {
    if !matches!(
        tool,
        "card_create"
            | "card_update"
            | "card_delete"
            | "tag_update"
            | "section_create"
            | "section_update"
            | "section_delete"
    ) || !crate::workdir::whole()
    {
        if in_batch && tool == "section_update" {
            refuse_settings_in_batch(tx, scope, args)?;
        }
        return Ok(());
    }
    if in_batch && tool == "section_update" {
        refuse_settings_in_batch(tx, scope, args)?;
    }
    let seen = Seen::in_tx(tx, scope)?;
    let project = seen.project();
    let pairs = args.pairs().unwrap_or_default();
    for (path, value) in &pairs {
        match tool {
            "card_update" | "card_delete" | "tag_update" => {
                if let Ok(target) = super::walk::locate(&project, path, None) {
                    let card = match target {
                        super::walk::Target::Card(c)
                        | super::walk::Target::Field(c, ..)
                        | super::walk::Target::FormatRow(c, _)
                        | super::walk::Target::CustomRow(c, _)
                        | super::walk::Target::EnumValue(c, _)
                        | super::walk::Target::Step(c, _) => Some(c),
                        super::walk::Target::Symbol(..) => {
                            return Err(text!("code.symbol.readOnly", path));
                        }
                        super::walk::Target::Ghost(_) => None,
                    };
                    if let Some(card) = card.filter(|c| c.kind == "file") {
                        let placing_only = tool == "card_update"
                            && !value.is_empty()
                            && value.keys().all(|k| k == "topLeft");
                        if !placing_only {
                            return Err(text!("workdirs.write.file", path = project.full(card)));
                        }
                        continue;
                    }
                    if tool == "card_update"
                        && let Some(card) = card
                        && let Some(to) = value.get("section").and_then(Value::as_str)
                        && let Some(section) = section_at(&project, to)
                        && in_workdir(&project, &section)
                        && card.kind != "note"
                    {
                        return Err(text!(
                            "workdirs.write.onlyNote",
                            path = to,
                            kind = card.kind
                        ));
                    }
                }
            }
            "card_create" => {
                let (section, _) = crate::pathtext::split_section(path);
                let Some(section) = section.and_then(|s| section_at(&project, s)) else {
                    continue;
                };
                if !in_workdir(&project, &section) {
                    continue;
                }
                let kind = value
                    .get("kind")
                    .and_then(Value::as_str)
                    .unwrap_or("entity");
                if kind != "note" {
                    return Err(text!(
                        "workdirs.write.onlyNote",
                        path = project.directory.full_section(&section),
                        kind
                    ));
                }
            }
            "section_create" => {
                let parent = crate::pathtext::last_slash(path.trim()).map(|(p, _)| p);
                if let Some(parent) = parent.and_then(|p| section_at(&project, p))
                    && in_workdir(&project, &parent)
                {
                    return Err(text!(
                        "workdirs.write.sectionInside",
                        path = project.directory.full_section(&parent)
                    ));
                }
            }
            "section_update" | "section_delete" => {
                let Some(section) = section_at(&project, path) else {
                    continue;
                };
                let edits = tool == "section_delete"
                    || ["name", "description", "position", "parent"]
                        .iter()
                        .any(|k| value.contains_key(*k));
                if generated(&project, &section) && edits {
                    return Err(text!(
                        "workdirs.write.directory",
                        path = project.directory.full_section(&section)
                    ));
                }
            }
            _ => {}
        }
    }
    Ok(())
}

/// Whether a section is a working directory's, or a directory in one.
fn in_workdir(project: &Project, section: &str) -> bool {
    matches!(
        project.section_kind(section),
        Some(crate::kinds::WORKDIR | crate::kinds::DIRECTORY)
    )
}

/// A batch's `section_update` that sets a working directory — how it is read, its folder, or its
/// description, which is the project's —: refused, as the project's settings are written apart from
/// any tree.
fn refuse_settings_in_batch(tx: &Tx, scope: &Scope, args: &Args) -> Result<(), String> {
    let pairs = args.pairs().unwrap_or_default();
    let mounts = crate::workdir::mounts(tx.conn(), &scope.project)?;
    let a_mount = |path: &str| {
        let path = path.trim();
        crate::pathtext::slashes(path).len() == 1
            && mounts
                .iter()
                .any(|m| crate::pathtext::same(&m.name, &crate::pathtext::unquote(path)))
    };
    if pairs.iter().any(|(path, v)| {
        OWN_SETTINGS.iter().any(|k| v.contains_key(*k))
            || (v.contains_key("description") && a_mount(path))
    }) {
        return Err(text!("workdirs.settings.inBatch"));
    }
    Ok(())
}

/// `section_update`'s keys that set a working directory's section (`SETTINGS`), written: how it is
/// read and what it leaves out and its description, the project's (whichever tree is named, and
/// not held by main's lock); the folder it is read from, this machine's. Read again when how or
/// where it is read changed. None when the section is no working directory's and nothing of it
/// is a working directory's alone — the description is then the section's own.
pub(crate) fn update_settings(
    tx: &mut Tx,
    scope: &Scope,
    path: &str,
    given: &Map<String, Value>,
) -> Result<Option<String>, String> {
    let dir = {
        let seen = Seen::in_tx(tx, scope)?;
        let project = seen.project();
        let section = section_at(&project, path);
        let dir = section.as_deref().and_then(|s| dir_of(&project, s));
        if dir.is_none() {
            if let Some(key) = OWN_SETTINGS.iter().find(|k| given.contains_key(**k)) {
                return Err(text!("workdirs.settings.notWorkdir", key, path));
            }
            return Ok(None);
        }
        dir.expect("a working directory's section")
    };
    let read_type = given
        .get("readType")
        .map(super::directories::read_type_of)
        .transpose()?;
    let exclude = given
        .get("exclude")
        .map(super::directories::exclude_of)
        .transpose()?;
    let folder = given
        .get("folder")
        .map(super::directories::folder_of)
        .transpose()?;
    let parsers = given
        .get("parsers")
        .map(super::directories::parsers_of)
        .transpose()?;
    let description = match given.get("description") {
        None => None,
        Some(Value::String(s)) => Some(super::cards::text(
            s,
            &text!("common.label.description"),
            2000,
        )?),
        Some(_) => return Err(text!("workdirs.settings.descriptionString")),
    };
    let conn = tx.conn();
    let mut dirs = super::directories::directories(conn, &scope.project)?;
    let at = dirs
        .iter()
        .position(|d| d.id == dir)
        .ok_or_else(|| text!("workdirs.settings.notWorkdir", key = "folder", path))?;
    if let Some(said) = &description {
        dirs[at].description = said.clone();
    }
    if let Some(read_type) = &read_type {
        dirs[at].read_type = read_type.clone();
    }
    if let Some(exclude) = &exclude {
        dirs[at].exclude = exclude.clone();
    }
    // Written whole: what it lists in place of what it listed.
    let parsers = parsers.filter(|p| *p != dirs[at].parsers);
    if let Some(parsers) = &parsers {
        dirs[at].parsers = parsers.clone();
    }
    let name = dirs[at].name.clone();
    if read_type.is_some() || exclude.is_some() || description.is_some() || parsers.is_some() {
        super::directories::set_directories(conn, &scope.project, &dirs)?;
        tx.line(
            &scope.project,
            "",
            "project.update",
            "project",
            Some(&scope.project),
            keyed!("workdirs.line.settings", name),
        )?;
    }
    if let Some(folder) = &folder {
        super::directories::bind(tx.conn(), &scope.project, &dir, folder.as_deref())?;
    }
    if parsers.is_some() {
        crate::code::cache::forget_dir(&dir);
    }
    // Read again, so what is made of the reading (the files' languages) is made anew.
    if read_type.is_some() || exclude.is_some() || folder.is_some() || parsers.is_some() {
        crate::workdir::read(tx.conn(), &scope.project, Some(std::slice::from_ref(&dir)))?;
    }
    Ok(Some(dir))
}

/// A working directory's section as `section_read`, `section_update` and `section_action` say it:
/// the folder it is read from on this machine (null when none), how it is read and what it leaves
/// out, how many directories and files were read or why none were, and its actions — with what was
/// read otherwise than it says, to be warned of.
pub(crate) fn said(
    conn: &Connection,
    project: &str,
    dir: &str,
) -> Result<(Map<String, Value>, Vec<String>), String> {
    let dirs = super::directories::directories(conn, project)?;
    let Some(d) = dirs.iter().find(|d| d.id == dir) else {
        return Ok((Map::new(), Vec::new()));
    };
    let folders = super::directories::folders(conn, project)?;
    let reads = crate::workdir::reads(conn, project)?;
    let read = reads.iter().find(|r| r.dir == dir);
    let mut out = Map::new();
    out.insert(
        "folder".into(),
        folders.get(dir).map_or(Value::Null, |f| f.clone().into()),
    );
    out.insert("readType".into(), d.read_type.clone().into());
    out.insert("exclude".into(), json!(d.exclude));
    out.insert("files".into(), read.map_or(0, |r| r.files).into());
    out.insert("directories".into(), read.map_or(0, |r| r.dirs).into());
    out.insert(
        "problem".into(),
        read.and_then(|r| r.problem.clone())
            .map_or(Value::Null, Value::from),
    );
    out.insert(
        "actions".into(),
        actions(why_not_read(folders.contains_key(dir), &d.read_type)),
    );
    let (languages, problems) = languages_said(&d.parsers);
    out.insert("parsers".into(), parsers_said(&d.parsers));
    out.insert("languages".into(), languages);
    if !problems.is_empty() {
        out.insert("problems".into(), json!(problems));
    }
    let mut warnings: Vec<String> = read.and_then(|r| r.note.clone()).into_iter().collect();
    // A language another machine may have, named where this one does not.
    warnings.extend(
        d.parsers
            .values()
            .flatten()
            .filter(|id| crate::code::registry::registry().find(id).is_none())
            .map(|id| text!("code.noLanguage", language = id)),
    );
    Ok((out, warnings))
}

/// The mapping of a working directory's files to languages as it is in effect: each extension and
/// file's name with its language (null for none), those the working directory sets marked.
fn parsers_said(parsers: &crate::code::registry::Parsers) -> Value {
    Value::Array(
        crate::code::registry::effective(parsers)
            .into_iter()
            .map(|(key, language, set)| {
                let mut row = Map::new();
                row.insert("match".into(), key.into());
                row.insert("language".into(), language.map_or(Value::Null, Value::from));
                if set {
                    row.insert("override".into(), true.into());
                }
                Value::Object(row)
            })
            .collect(),
    )
}

/// The languages this machine reads files in, each with where its grammar comes from and what is
/// read of it; and what keeps one from being read: a language the mapping names that this machine
/// does not have, a grammar added that could not be loaded.
fn languages_said(parsers: &crate::code::registry::Parsers) -> (Value, Vec<String>) {
    let registry = crate::code::registry::registry();
    let languages: Vec<Value> = registry
        .all()
        .iter()
        .map(|l| {
            let mut row = Map::new();
            row.insert("id".into(), l.id.clone().into());
            row.insert("source".into(), l.source.word().into());
            row.insert("symbols".into(), l.has_symbols().into());
            row.insert("highlight".into(), l.has_highlight().into());
            Value::Object(row)
        })
        .collect();
    let mut problems: Vec<String> = Vec::new();
    for id in parsers.values().flatten() {
        if registry.find(id).is_none() {
            let said = text!("code.noLanguage", language = id);
            if !problems.contains(&said) {
                problems.push(said);
            }
        }
    }
    #[cfg(feature = "dynamic-grammars")]
    for (name, why) in crate::code::loader::problems() {
        problems.push(text!("code.loader.problem", name, why));
    }
    (Value::Array(languages), problems)
}

/// A working directory's section's actions, as `section_read` says them: each op with its name,
/// what it does, and whether it can be used now (or why not).
fn actions(why_not: Option<String>) -> Value {
    let defs = crate::kinds::builtin_defs();
    let ops = defs
        .section(crate::kinds::WORKDIR)
        .map(|s| s.actions.clone())
        .unwrap_or_default();
    Value::Array(
        ops.iter()
            .map(|op| {
                let mut row = Map::new();
                row.insert("op".into(), op.clone().into());
                row.insert("label".into(), super::actions::label(op).into());
                if let Some(a) = super::actions::written(op) {
                    row.insert("description".into(), a["description"].clone());
                }
                match &why_not {
                    None => {
                        row.insert("available".into(), true.into());
                    }
                    Some(why) => {
                        row.insert("available".into(), false.into());
                        row.insert("why".into(), why.clone().into());
                    }
                }
                Value::Object(row)
            })
            .collect(),
    )
}

/// Why a working directory cannot be read now: no folder bound here, or read as none.
fn why_not_read(bound: bool, read_type: &str) -> Option<String> {
    if !bound {
        Some(text!("workdir.read.unbound"))
    } else if read_type == "none" {
        Some(text!("workdir.read.none"))
    } else {
        None
    }
}

/// `section_action`: what a section's kind can be made to do (`actions` of its kind), done when it
/// can be now. A working directory's `read`: its folder read again, answered as `section_read`
/// says the section. Nothing of the project is written, so a key that may only read the project
/// does it too.
pub(crate) fn section_action(conn: &Connection, args: &Args) -> Result<Value, String> {
    let store_named = super::project_summary_in(conn, args.string("project")?)?;
    let named = super::named_in(conn, &store_named.id, args.optional_string("branch"))?;
    let path = args.target()?;
    let op = args.string("op")?.trim().to_string();
    let value = args.value()?;
    if let Some(key) = value.keys().next() {
        return Err(text!("workdirs.action.noValue", key));
    }
    let tree = (!named.tree.is_empty()).then_some(named.tree.as_str());
    let detail = crate::model::typed(&crate::detail::read(
        &crate::store::Reader { conn },
        &store_named.id,
        tree,
    )?)?;
    let project = Project::new(&detail);
    let section = super::section_id(&project, &path)?;
    let kind = project
        .section_kind(&section)
        .unwrap_or_default()
        .to_string();
    let ops = project
        .defs
        .section(&kind)
        .map(|s| s.actions.clone())
        .unwrap_or_default();
    if !ops.contains(&op) {
        return Err(if ops.is_empty() {
            text!(
                "workdirs.action.none",
                path = project.directory.full_section(&section),
                kind
            )
        } else {
            text!(
                "workdirs.action.notOfKind",
                op,
                kind,
                ops = crate::text::List::of(&ops)
            )
        });
    }
    let dir = dir_of(&project, &section).ok_or_else(|| {
        text!(
            "workdirs.action.none",
            path = project.directory.full_section(&section),
            kind
        )
    })?;
    let bound = super::directories::folders(conn, &store_named.id)?.contains_key(&dir);
    let read_type = super::directories::directories(conn, &store_named.id)?
        .into_iter()
        .find(|d| d.id == dir)
        .map(|d| d.read_type)
        .unwrap_or_default();
    if let Some(why) = why_not_read(bound, &read_type) {
        return Err(text!(
            "workdirs.action.notNow",
            path = project.directory.full_section(&section),
            why
        ));
    }
    // The grammars this machine adds are looked for again with the files.
    #[cfg(feature = "dynamic-grammars")]
    crate::code::loader::reload();
    crate::workdir::read(conn, &store_named.id, Some(std::slice::from_ref(&dir)))?;
    let (said, warnings) = said(conn, &store_named.id, &dir)?;
    let mut out = Map::new();
    out.insert("project".into(), store_named.name.clone().into());
    out.insert(
        "section".into(),
        project.directory.full_section(&section).into(),
    );
    out.extend(said);
    Ok(super::headed_json(
        super::warned(Value::Object(out), &warnings),
        &named.name,
    ))
}
