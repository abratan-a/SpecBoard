//! The exports (書き出し) a project keeps, made, changed and let go by name (`export::settings`): `export_create`, `export_update`,
//! `export_delete`. The project's settings, as its working directories are: not a branch's, not held
//! by main's lock, a line in main's history. Its cards and sections said by their paths in main, kept
//! by id. What one writes, and how it stands, is `export_read`; writing it, `export_execute` (`files`).
//!
//! An export holds no place to write to: a call gives it (`targetdir`), and each Mac keeps one default for it
//! (`export_default_folders`), set by `path` here as a working directory's folder is (the project's
//! item `export.dir`, asked each time by default: `mcp_writes`).

use super::Seen;
use super::args::Args;
use super::cards::Scope;
use crate::export::settings::{self, Entry, Export};
use crate::model::{CODE_FORMATS, ExportSetting};
use crate::write::{Tx, db, new_id};
use serde_json::{Map, Value, json};
use std::collections::HashMap;

/// The folder each export is written to by default on this machine, by the export's id.
pub(crate) fn default_folders(
    conn: &rusqlite::Connection,
    project: &str,
) -> Result<HashMap<String, String>, String> {
    let mut stmt = conn
        .prepare("SELECT export_id, folder FROM export_default_folders WHERE project_id = ?")
        .map_err(db)?;
    stmt.query_map([project], |r| Ok((r.get(0)?, r.get(1)?)))
        .map_err(db)?
        .collect::<Result<_, _>>()
        .map_err(db)
}

/// The default folder of an export on this machine, set, or let go (none).
fn set_default(
    tx: &mut Tx,
    project: &str,
    export: &str,
    folder: Option<&str>,
) -> Result<(), String> {
    tx.conn()
        .execute(
            "DELETE FROM export_default_folders WHERE project_id = ? AND export_id = ?",
            rusqlite::params![project, export],
        )
        .map_err(db)?;
    if let Some(folder) = folder {
        tx.conn()
            .execute(
                "INSERT INTO export_default_folders (project_id, export_id, folder) VALUES (?, ?, ?)",
                rusqlite::params![project, export, folder],
            )
            .map_err(db)?;
    }
    Ok(())
}

/// The project's exports, as its row keeps them.
pub(crate) fn exports(conn: &rusqlite::Connection, project: &str) -> Result<Vec<Export>, String> {
    let text: Option<String> = conn
        .query_row(
            "SELECT exports FROM projects WHERE id = ?",
            [project],
            |r| r.get(0),
        )
        .map_err(db)?;
    Ok(settings::parse(text.as_deref()))
}

fn set_exports(tx: &mut Tx, project: &str, list: &[Export]) -> Result<(), String> {
    tx.conn()
        .execute(
            "UPDATE projects SET exports = ? WHERE id = ?",
            rusqlite::params![settings::kept(list), project],
        )
        .map_err(db)?;
    Ok(())
}

/// The export a name names, in any case; or what there is.
pub(crate) fn named<'a>(list: &'a [Export], name: &str) -> Result<&'a Export, String> {
    list.iter()
        .find(|e| crate::pathtext::same(&e.name, name.trim()))
        .ok_or_else(|| {
            if list.is_empty() {
                text!("exports.named.none", name)
            } else {
                text!(
                    "exports.named.missing",
                    name,
                    there = crate::text::List::of(list.iter().map(|e| e.name.as_str()))
                )
            }
        })
}

/// Named as a section is, once among the project's exports in any case.
fn export_name(raw: &str, list: &[Export], excluding: Option<&str>) -> Result<String, String> {
    let name = crate::write::valid_name(raw, &text!("exports.what.name"))?;
    if list
        .iter()
        .any(|e| Some(e.id.as_str()) != excluding && crate::pathtext::same(&e.name, &name))
    {
        return Err(text!("exports.name.exists", name));
    }
    Ok(name)
}

/// What an export's paths name in main: a card's or a section's id, by the path written.
struct Paths<'a> {
    project: crate::export::Project<'a>,
}

impl<'a> Paths<'a> {
    /// A card (`Schemas:Order`) or a section (`Schemas/Ordering`, or with `:` at its end).
    fn id(&self, raw: &str) -> Result<String, String> {
        let raw = raw.trim();
        let section = raw.strip_suffix(':').unwrap_or(raw);
        if !crate::pathtext::has_section(raw) || raw.ends_with(':') {
            if let Some(id) = self.project.directory.section_named(section, None) {
                return Ok(id);
            }
            return Err(text!("exports.path.noSection", section));
        }
        self.project
            .find(raw, self.project.home())
            .map(|c| c.id.clone())
            .ok_or_else(|| text!("exports.path.noCard", raw))
    }

    /// What an entry's `from` names: a card or a section by its id, or a stackdoc's version
    /// (`Docs:Changelog.v0_2_24`) by its card's id and its row's.
    fn from(&self, raw: &str) -> Result<(String, Option<String>), String> {
        if let Some(written) = crate::export::paths::WrittenPath::new(raw)
            && let [name] = written.names.as_slice()
            && let Some(card) = self.project.find(&written.card, self.project.home())
        {
            if card.kind != "stackdoc" {
                return Err(text!("exports.path.inCard", raw));
            }
            let versions = card.versions();
            let name = crate::pathtext::unquote(name);
            let Some(version) = versions.iter().find(|v| v.name == name) else {
                return Err(text!(
                    "exports.path.noVersion",
                    card = self.project.full(card),
                    name,
                    versions = crate::text::List::of(versions.iter().map(|v| v.name.as_str()))
                ));
            };
            return Ok((card.id.clone(), Some(version.id.clone())));
        }
        Ok((self.id(raw)?, None))
    }

    /// An entry's `from` said as a path in main: its version's too.
    fn said_from(&self, entry: &Entry) -> String {
        let card_path = self.path(&entry.from);
        let Some(row) = &entry.row else {
            return card_path;
        };
        self.project
            .card(&entry.from)
            .and_then(|c| c.versions().into_iter().find(|v| &v.id == row))
            .map_or_else(
                || format!("{card_path}.{}", text!("exports.path.versionGone", row)),
                |v| crate::pathtext::member(&card_path, &v.name),
            )
    }

    /// An id said as a path in main; a card or section no longer there, by its id.
    fn path(&self, id: &str) -> String {
        if let Some(card) = self.project.card(id) {
            return self.project.full(card);
        }
        if self.project.detail.sections.iter().any(|s| s.id == id) {
            return self.project.directory.full_section(id);
        }
        text!("exports.path.gone", id)
    }
}

/// An export as MCP and the window read it: its cards and sections by their paths in main.
pub(crate) fn said(export: &Export, project: &crate::export::Project) -> Value {
    let paths = Paths {
        project: crate::export::Project::new(project.detail),
    };
    let mut out = Map::new();
    out.insert("name".into(), export.name.clone().into());
    out.insert(
        "entries".into(),
        Value::Array(
            export
                .entries
                .iter()
                .map(|e| {
                    let mut row = Map::new();
                    row.insert("from".into(), paths.said_from(e).into());
                    row.insert("to".into(), e.to.clone().into());
                    if let Some(format) = &e.format {
                        row.insert("format".into(), format.clone().into());
                    }
                    if !e.kinds.is_empty() {
                        row.insert("kinds".into(), json!(e.kinds));
                    }
                    Value::Object(row)
                })
                .collect(),
        ),
    );
    if !export.exclude.is_empty() {
        out.insert(
            "exclude".into(),
            json!(
                export
                    .exclude
                    .iter()
                    .map(|id| paths.path(id))
                    .collect::<Vec<_>>()
            ),
        );
    }
    Value::Object(out)
}

/// An entry as written: `{from, to, format, kinds}`.
fn entry_of(raw: &Value, at: &str, paths: &Paths) -> Result<Entry, String> {
    let Some(row) = raw.as_object() else {
        return Err(text!("exports.entry.shape", at));
    };
    if let Some(stray) = row
        .keys()
        .find(|k| !["from", "to", "format", "kinds"].contains(&k.as_str()))
    {
        return Err(text!("exports.entry.stray", at, stray));
    }
    let from = row
        .get("from")
        .and_then(Value::as_str)
        .ok_or_else(|| text!("exports.entry.needFrom", at))?;
    let (from, version) = paths.from(from).map_err(|e| format!("{at}: {e}"))?;
    // What a working directory's section holds is read from a folder, not written out.
    let section = paths
        .project
        .card(&from)
        .map(|c| paths.project.section_of(c))
        .unwrap_or_else(|| from.clone());
    if matches!(
        paths.project.section_kind(&section),
        Some(crate::kinds::WORKDIR | crate::kinds::DIRECTORY)
    ) {
        return Err(text!("exports.entry.workdir", at));
    }
    let to = row
        .get("to")
        .and_then(Value::as_str)
        .ok_or_else(|| text!("exports.entry.needTo", at))?
        .trim_matches(|c: char| c == ' ' || c == '\t')
        .to_string();
    if let Some(problem) = ExportSetting::path_problem(&to) {
        return Err(format!("{at}: {problem}"));
    }
    let format = match row.get("format") {
        None | Some(Value::Null) => None,
        Some(Value::String(f))
            if CODE_FORMATS.contains(&f.as_str()) || f == crate::model::JSON_DATA =>
        {
            Some(f.clone())
        }
        Some(other) => {
            return Err(text!(
                "exports.entry.formatOneOf",
                at,
                formats = crate::text::List::of(
                    CODE_FORMATS
                        .iter()
                        .copied()
                        .chain(std::iter::once(crate::model::JSON_DATA))
                ),
                other
            ));
        }
    };
    // Every format writes one file; no card is written to a folder as one.
    if let Some(format) = format.as_deref()
        && to.ends_with('/')
    {
        return Err(text!("exports.entry.formatFolder", at, format, to));
    }
    let kinds = match row.get("kinds") {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Array(list)) => {
            let mut out = Vec::new();
            for kind in list {
                let Some(kind) = kind.as_str() else {
                    return Err(text!("exports.entry.kindsList", at));
                };
                if paths.project.defs.cards().get(kind).is_none() {
                    return Err(text!("exports.entry.noKind", at, kind));
                }
                if !out.iter().any(|k| k == kind) {
                    out.push(kind.to_string());
                }
            }
            out
        }
        Some(_) => return Err(text!("exports.entry.kindsList", at)),
    };
    // A section's entry with kinds writes each of them where it may be written: refused here, not
    // only when it is written.
    if paths.project.card(&from).is_none() {
        let setting = ExportSetting {
            kind: "file".into(),
            dir: Some(String::new()),
            path: to.clone(),
            format: format.clone(),
        };
        for kind in &kinds {
            if let Some(problem) = setting.problem_for(kind) {
                return Err(text!("exports.entry.kindTo", at, kind, to, problem));
            }
        }
    }
    // A card's own entry writes where it may be written.
    if let Some(card) = paths.project.card(&from) {
        let setting = ExportSetting {
            kind: "file".into(),
            dir: Some(String::new()),
            path: to.clone(),
            format: format.clone(),
        };
        if let Some(problem) = setting.problem_for(&card.kind) {
            return Err(format!("{at}: {}: {problem}", paths.project.full(card)));
        }
        if !kinds.is_empty() {
            return Err(text!("exports.entry.kindsOnlySection", at));
        }
    }
    Ok(Entry {
        from,
        row: version,
        to,
        format,
        kinds,
    })
}

/// A list of entries as written.
fn entries_of(raw: &Value, key: &str, paths: &Paths) -> Result<Vec<Entry>, String> {
    let Some(list) = raw.as_array() else {
        return Err(text!("exports.entries.list", key));
    };
    list.iter()
        .enumerate()
        .map(|(i, e)| entry_of(e, &format!("{key}[{i}]"), paths))
        .collect()
}

/// What the entries of an export say of themselves, as main is now: each section's entry with no
/// kinds that writes no card (it takes no flow, and notes only from a schema section), told — not
/// refused, as the section may get cards to write.
fn takes_nothing(export: &Export, paths: &Paths) -> Vec<String> {
    let mut out = Vec::new();
    for entry in &export.entries {
        if !entry.kinds.is_empty() || paths.project.card(&entry.from).is_some() {
            continue;
        }
        let alone = Export {
            id: export.id.clone(),
            name: export.name.clone(),
            entries: vec![entry.clone()],
            exclude: export.exclude.clone(),
        };
        let resolved = crate::export::settings::resolve(&[alone], &paths.project);
        if resolved.by_card.is_empty() && resolved.pages.is_empty() {
            out.push(text!(
                "exports.entry.takesNothing",
                at = format!("{} → {}", paths.said_from(entry), entry.to)
            ));
        }
    }
    out
}

/// A list of cards and sections as written: their ids.
fn ids_of(raw: &Value, key: &str, paths: &Paths) -> Result<Vec<String>, String> {
    let Some(list) = raw.as_array() else {
        return Err(text!("exports.ids.list", key));
    };
    let mut out = Vec::new();
    for (i, item) in list.iter().enumerate() {
        let path = item
            .as_str()
            .ok_or_else(|| text!("exports.ids.item", key, i))?;
        let id = paths.id(path).map_err(|e| format!("{key}[{i}]: {e}"))?;
        if !out.contains(&id) {
            out.push(id);
        }
    }
    Ok(out)
}

fn keys_of(given: &Map<String, Value>, tool: &str, allowed: &[&str]) -> Result<(), String> {
    if given.contains_key("dir") && !allowed.contains(&"dir") {
        return Err(text!("exports.keys.dir", tool));
    }
    if let Some(stray) = given.keys().find(|k| !allowed.contains(&k.as_str())) {
        return Err(text!(
            "exports.keys.stray",
            tool,
            stray,
            allowed = crate::text::List::of(allowed)
        ));
    }
    Ok(())
}

/// main as it is now, to say paths in.
fn main_of(tx: &Tx, scope: &Scope) -> Result<std::rc::Rc<Seen>, String> {
    let main = Scope {
        project: scope.project.clone(),
        project_name: scope.project_name.clone(),
        tree: String::new(),
        tree_name: "main".into(),
    };
    Seen::in_tx(tx, &main)
}

fn line(tx: &mut Tx, project: &str, said: crate::text::Keyed) -> Result<(), String> {
    tx.line(
        project,
        "",
        "project.update",
        "project",
        Some(project),
        said,
    )
}

/// `export_create`: an export named `target`, writing what `entries` say but what `exclude` names,
/// and where it is written to by default on this machine (`path`).
pub(crate) fn export_create(tx: &mut Tx, scope: &Scope, args: &Args) -> Result<Value, String> {
    let given = args.value()?;
    keys_of(&given, "export_create", &["entries", "exclude", "path"])?;
    let target = args
        .targets()?
        .filter(|t| t.len() == 1)
        .map(|t| t[0].clone())
        .ok_or_else(|| text!("exports.target.one", tool = "export_create"))?;
    let seen = main_of(tx, scope)?;
    let paths = Paths {
        project: crate::export::Project::new(&seen.detail),
    };
    let folder = given
        .get("path")
        .map(super::directories::folder_of)
        .transpose()?
        .flatten();
    let entries = match given.get("entries") {
        Some(raw) => entries_of(raw, "entries", &paths)?,
        None => Vec::new(),
    };
    let exclude = match given.get("exclude") {
        Some(raw) => ids_of(raw, "exclude", &paths)?,
        None => Vec::new(),
    };
    let mut list = exports(tx.conn(), &scope.project)?;
    let name = export_name(&target, &list, None)?;
    let id = new_id();
    let export = Export {
        id: id.clone(),
        name: name.clone(),
        entries,
        exclude,
    };
    let warnings = takes_nothing(&export, &paths);
    list.push(export);
    set_exports(tx, &scope.project, &list)?;
    if let Some(folder) = &folder {
        set_default(tx, &scope.project, &id, Some(folder))?;
    }
    line(tx, &scope.project, keyed!("exports.line.created", name))?;
    let mut out = Map::new();
    if !warnings.is_empty() {
        out.insert("warnings".into(), json!(warnings));
    }
    out.insert("created".into(), name.into());
    Ok(Value::Object(out))
}

/// Entries added and taken away (`{add: [...], remove: [...]}`); an entry taken away by its from and
/// to.
fn entries_over(raw: &Value, now: &mut Vec<Entry>, paths: &Paths) -> Result<(), String> {
    let Some(edits) = raw.as_object() else {
        return Err(text!("exports.entries.edits"));
    };
    keys_of(edits, "entries", &["add", "remove"])?;
    if let Some(gone) = edits.get("remove") {
        let Some(list) = gone.as_array() else {
            return Err(text!("exports.entries.removeList"));
        };
        for (i, item) in list.iter().enumerate() {
            let at = format!("entries.remove[{i}]");
            // A from alone takes away every entry of it, as {from} does.
            let (written, to) = match item {
                Value::String(from) => (from.as_str(), None),
                Value::Object(o) => (
                    o.get("from")
                        .and_then(Value::as_str)
                        .ok_or_else(|| text!("exports.entries.removeItem", at))?,
                    o.get("to").and_then(Value::as_str),
                ),
                _ => return Err(text!("exports.entries.removeItem", at)),
            };
            let (from, version) = paths.from(written).map_err(|e| format!("{at}: {e}"))?;
            let before = now.len();
            now.retain(|e| {
                !(e.from == from && e.row == version && to.is_none_or(|t| e.to == t.trim()))
            });
            if now.len() == before {
                const SHOWN: usize = 10;
                let mut rows: Vec<String> = now
                    .iter()
                    .take(SHOWN)
                    .map(|e| format!("{} → {}", paths.said_from(e), e.to))
                    .collect();
                if now.len() > SHOWN {
                    rows.push(text!("exports.entries.more", count = now.len() - SHOWN));
                }
                return Err(text!(
                    "exports.entries.noRow",
                    at,
                    from = written.trim(),
                    to = to.map(|t| format!(" → {}", t.trim())).unwrap_or_default(),
                    rows = if rows.is_empty() {
                        text!("exports.entries.noRows")
                    } else {
                        crate::text::list(&rows)
                    }
                ));
            }
        }
    }
    if let Some(add) = edits.get("add") {
        for entry in entries_of(add, "entries.add", paths)? {
            if !now.contains(&entry) {
                now.push(entry);
            }
        }
    }
    Ok(())
}

/// What is left out added and taken away (`{add: [...], remove: [...]}`).
fn exclude_over(raw: &Value, now: &mut Vec<String>, paths: &Paths) -> Result<(), String> {
    let Some(edits) = raw.as_object() else {
        return Err(text!("exports.exclude.edits"));
    };
    keys_of(edits, "exclude", &["add", "remove"])?;
    if let Some(gone) = edits.get("remove") {
        for id in ids_of(gone, "exclude.remove", paths)? {
            now.retain(|e| *e != id);
        }
    }
    if let Some(add) = edits.get("add") {
        for id in ids_of(add, "exclude.add", paths)? {
            if !now.contains(&id) {
                now.push(id);
            }
        }
    }
    Ok(())
}

/// `export_update`: the export `target` names changed — a new name, where it is written to by
/// default on this machine (`path`; null lets it go), entries and what is left out added and taken
/// away. Answered as `export_read` says it.
pub(crate) fn export_update(tx: &mut Tx, scope: &Scope, args: &Args) -> Result<Value, String> {
    let given = args.value()?;
    keys_of(
        &given,
        "export_update",
        &["name", "path", "entries", "exclude"],
    )?;
    if given.is_empty() {
        return Err(text!("exports.update.nothing"));
    }
    let target = args
        .targets()?
        .filter(|t| t.len() == 1)
        .map(|t| t[0].clone())
        .ok_or_else(|| text!("exports.target.one", tool = "export_update"))?;
    let seen = main_of(tx, scope)?;
    let paths = Paths {
        project: crate::export::Project::new(&seen.detail),
    };
    let folder = given
        .get("path")
        .map(super::directories::folder_of)
        .transpose()?;
    let mut list = exports(tx.conn(), &scope.project)?;
    let id = named(&list, &target)?.id.clone();
    let at = list.iter().position(|e| e.id == id).expect("named");
    let was = list[at].name.clone();
    if let Some(raw) = given.get("name") {
        let to = raw
            .as_str()
            .ok_or_else(|| text!("exports.update.nameString"))?;
        list[at].name = export_name(to, &list, Some(&id))?;
    }
    let mut warnings = Vec::new();
    if let Some(raw) = given.get("entries") {
        let kept = list[at].entries.clone();
        entries_over(raw, &mut list[at].entries, &paths)?;
        // Said of the entries added alone: an entry kept as it was is not said again.
        let added = Export {
            entries: list[at]
                .entries
                .iter()
                .filter(|e| !kept.contains(e))
                .cloned()
                .collect(),
            ..list[at].clone()
        };
        warnings = takes_nothing(&added, &paths);
    }
    if let Some(raw) = given.get("exclude") {
        exclude_over(raw, &mut list[at].exclude, &paths)?;
    }
    set_exports(tx, &scope.project, &list)?;
    if let Some(folder) = &folder {
        set_default(tx, &scope.project, &id, folder.as_deref())?;
    }
    line(
        tx,
        &scope.project,
        keyed!("exports.line.updated", name = was),
    )?;
    if list[at].name != was {
        super::files::renamed(tx, &scope.project, &list[at], &was);
    }
    let mut out = said(&list[at], &paths.project);
    if let Value::Object(o) = &mut out {
        o.insert(
            "path".into(),
            default_folders(tx.conn(), &scope.project)?
                .get(&id)
                .map_or(Value::Null, |f| f.clone().into()),
        );
        if !warnings.is_empty() {
            o.insert("warnings".into(), json!(warnings));
        }
    }
    Ok(out)
}

/// `export_delete`: the exports `target` names let go. What they wrote stays where it is; what each
/// kept of it (`.specboard/<project>/<export>.json`) goes, from its default folder on this machine
/// (a record in another folder written to is not touched), and the default with it.
pub(crate) fn export_delete(tx: &mut Tx, scope: &Scope, args: &Args) -> Result<Value, String> {
    let names = args
        .targets()?
        .filter(|t| !t.is_empty())
        .ok_or_else(|| text!("exports.target.needed"))?;
    let mut list = exports(tx.conn(), &scope.project)?;
    let mut gone = Vec::new();
    for name in &names {
        let export = named(&list, name)?.clone();
        list.retain(|e| e.id != export.id);
        gone.push(export);
    }
    set_exports(tx, &scope.project, &list)?;
    for export in &gone {
        line(
            tx,
            &scope.project,
            keyed!("exports.line.deleted", name = export.name),
        )?;
        super::files::forget(tx, &scope.project, export);
        set_default(tx, &scope.project, &export.id, None)?;
    }
    Ok(json!({ "deleted": gone.into_iter().map(|e| e.name).collect::<Vec<_>>() }))
}
