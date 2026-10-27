//! What the exports write and how it stands (`export_read`), and writing it (`export_execute`).
//!
//! Each export writes main's cards — or a branch's the call names — into the folder a call gives
//! (`targetdir`: a folder, or `[default]`, the folder each export has by default on this machine,
//! which is main's to write), and keeps what it wrote beside them in that folder: `.specboard/<project's slug>/<export>.json` — the project, the export,
//! the branch written from (none for main), when, that tree's newest commit, the app's version, and
//! each file's path, what it holds, the sha256 of what was written and the cards in it with a hash of
//! each as it was then. A card keeps its id through a merge, so what a branch wrote reads as current
//! from main once that branch is merged, where the merge left its cards as they were. Writing again takes away
//! what that export wrote before and does not now — only that: a file written by hand beside them, or
//! by another export, is never touched. Read against it, each file is current, changed since (a card
//! of it changed, or what it would write now differs), edited by hand (the file is not what was
//! written), missing, or not written yet; and what writing again takes away is said. What is read
//! from the folder needs it to be known: a `[default]` an export has none for is said not to have
//! been read.

use super::Seen;
use super::args::Args;
use crate::export::settings::Export;
use crate::export::{Built, Planned};
use crate::mcp::Output;
use crate::store::Store;
use crate::write::Tx;
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;

/// Paths in the order the Finder shows them: numbers by their value, letters in any case
/// (`localizedStandardCompare`, near enough).
fn standard_order(a: &str, b: &str) -> Ordering {
    fn chunks(s: &str) -> Vec<(bool, String)> {
        let mut out: Vec<(bool, String)> = Vec::new();
        for c in s.chars() {
            let digit = c.is_ascii_digit();
            match out.last_mut() {
                Some((d, chunk)) if *d == digit => chunk.push(c),
                _ => out.push((digit, c.to_string())),
            }
        }
        out
    }
    let (x, y) = (chunks(a), chunks(b));
    for ((dx, cx), (dy, cy)) in x.iter().zip(&y) {
        let order = if *dx && *dy {
            let (nx, ny) = (cx.trim_start_matches('0'), cy.trim_start_matches('0'));
            nx.len().cmp(&ny.len()).then_with(|| nx.cmp(ny))
        } else {
            cx.to_lowercase().cmp(&cy.to_lowercase())
        };
        if order != Ordering::Equal {
            return order;
        }
    }
    x.len().cmp(&y.len()).then_with(|| a.cmp(b))
}

/// What a call says to write to or to read against (`targetdir`, always given): each export's
/// default folder on this machine (`[default]`), or one folder.
#[derive(Clone, Debug)]
pub(crate) enum Target {
    Default,
    Folder(String),
}

/// What `targetdir` says for the default folders.
pub(crate) const DEFAULT: &str = "[default]";

/// Why a branch is not written into the default folders: they are main's to write.
fn branch_to_default() -> String {
    text!("files.target.branchToDefault")
}

impl Target {
    /// `targetdir` of a call: required, so that where a thing is written is always said.
    pub(crate) fn of(args: &Args) -> Result<Target, String> {
        let raw = args
            .optional_string("targetdir")
            .map(super::check::trim_spaces)
            .filter(|t| !t.is_empty())
            .ok_or_else(|| text!("files.target.needed", marker = DEFAULT))?;
        if raw == DEFAULT {
            return Ok(Target::Default);
        }
        match super::directories::folder_of(&Value::String(raw.to_string())) {
            Ok(Some(folder)) => Ok(Target::Folder(folder)),
            Ok(None) => Err(text!("files.target.empty")),
            Err(problem) => Err(text!("files.target.folder", problem, marker = DEFAULT)),
        }
    }

    /// Whether a folder is named (a path is given, rather than the defaults).
    pub(crate) fn is_folder(&self) -> bool {
        matches!(self, Target::Folder(_))
    }
}

/// Where each export of a plan is written to or read against, by its place in the list: the chosen
/// ones to what the call says, the others to their default folder (none where there is none).
fn destinations(
    plan: &Plan,
    defaults: &HashMap<String, String>,
    target: &Target,
    chosen: &[usize],
) -> Vec<Option<String>> {
    plan.exports
        .iter()
        .enumerate()
        .map(|(at, export)| match target {
            Target::Folder(folder) if chosen.contains(&at) => Some(folder.clone()),
            _ => defaults.get(&export.id).cloned(),
        })
        .collect()
}

/// A file's place on this machine, in full: its folder (`~` for the home folder) and its path in it,
/// `.` and `..` worked out.
pub(crate) fn absolute(root: &str, path: &str) -> String {
    let home = std::env::var("HOME").unwrap_or_default();
    let root = match root.strip_prefix('~') {
        Some(rest) if rest.is_empty() || rest.starts_with('/') => format!("{home}{rest}"),
        _ => root.to_string(),
    };
    let mut parts: Vec<&str> = Vec::new();
    for part in root.split('/').chain(path.split('/')) {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            other => parts.push(other),
        }
    }
    format!("/{}", parts.join("/"))
}

/// A path to write, or to take away: relative, each part a name (not empty, `.` or `..`), and no
/// control characters.
fn is_inside(path: &str) -> bool {
    !path.starts_with('~')
        && path.split('/').all(|p| !["", ".", ".."].contains(&p))
        && !path.chars().any(char::is_control)
}

/// The first part of `path` (relative, its parts names) under `folder` that is a symbolic link, as a
/// path from the folder: writing there would follow the link, and a link may point anywhere on the
/// machine. None when no part on the way is one — the parts not there yet are made as folders.
pub(crate) fn link_on_the_way(folder: &str, path: &str) -> Option<String> {
    let mut at = PathBuf::from(absolute(folder, ""));
    let mut walked: Vec<&str> = Vec::new();
    for part in path.split('/').filter(|p| !p.is_empty()) {
        at.push(part);
        walked.push(part);
        match std::fs::symlink_metadata(&at) {
            Ok(meta) if meta.file_type().is_symlink() => return Some(walked.join("/")),
            Ok(_) => {}
            Err(_) => return None,
        }
    }
    None
}

/// Why an export may not write `path` in `folder`, or none: a link on the way (`link_on_the_way`).
fn through_link(folder: &str, path: &str) -> Option<String> {
    link_on_the_way(folder, path).map(|link| text!("files.write.link", folder, link))
}

/// The folder, under a working directory's folder, that keeps what was written there.
const MARK_FOLDER: &str = ".specboard";

fn sha256(text: &[u8]) -> String {
    Sha256::digest(text)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// A card as it is written out, hashed: what it holds, not where it sits; the cards inside it with it.
fn card_hash(raw: &Value, id: &str) -> String {
    const PLACE: [&str; 6] = ["posX", "posY", "pinned", "sortOrder", "size", "docView"];
    let strip = |card: &Map<String, Value>| {
        let mut card = card.clone();
        for key in PLACE {
            card.shift_remove(key);
        }
        Value::Object(card)
    };
    let mut parts = Vec::new();
    for list in ["schemas", "inner"] {
        for card in raw[list].as_array().into_iter().flatten() {
            let Some(card) = card.as_object() else {
                continue;
            };
            let own = card.get("id").and_then(Value::as_str) == Some(id);
            let inside = card.get("inside").and_then(Value::as_str) == Some(id);
            if own || inside {
                parts.push(strip(card));
            }
        }
    }
    sha256(crate::ojson::compact(&Value::Array(parts)).as_bytes())
}

/// What an export kept of what it wrote last.
#[derive(Default)]
struct Kept {
    written_at: String,
    files: Vec<KeptFile>,
}

struct KeptFile {
    path: String,
    sha256: String,
    /// By id: the path then, and the hash.
    cards: Vec<(String, String, String)>,
}

/// Where an export keeps what it wrote, in its folder.
fn kept_path(slug: &str, export: &str) -> String {
    format!(
        "{MARK_FOLDER}/{}/{}.json",
        crate::export::folder_name(slug),
        crate::export::folder_name(export)
    )
}

fn read_kept(folder: &str, slug: &str, export: &str) -> Option<Kept> {
    let text = std::fs::read_to_string(absolute(folder, &kept_path(slug, export))).ok()?;
    let value: Value = serde_json::from_str(&text).ok()?;
    Some(Kept {
        written_at: value["writtenAt"].as_str().unwrap_or("").to_string(),
        files: value["files"]
            .as_array()?
            .iter()
            .filter_map(|f| {
                Some(KeptFile {
                    path: f["path"].as_str()?.to_string(),
                    sha256: f["sha256"].as_str().unwrap_or("").to_string(),
                    cards: f["cards"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(|c| {
                            Some((
                                c["id"].as_str()?.to_string(),
                                c["path"].as_str().unwrap_or("").to_string(),
                                c["hash"].as_str().unwrap_or("").to_string(),
                            ))
                        })
                        .collect(),
                })
            })
            .collect(),
    })
}

/// The project's slug, which names its folder of what was written.
fn slug_of(conn: &rusqlite::Connection, project: &str) -> String {
    conn.query_row("SELECT slug FROM projects WHERE id = ?", [project], |r| {
        r.get(0)
    })
    .unwrap_or_default()
}

/// What an export kept, renamed with it in the export's default folder on this machine once the
/// transaction commits (best done: a folder not there leaves it; what is kept in another folder
/// written to is not touched).
pub(crate) fn renamed(tx: &mut Tx, project: &str, export: &Export, was: &str) {
    let Ok(defaults) = super::exports::default_folders(tx.conn(), project) else {
        return;
    };
    let Some(folder) = defaults.get(&export.id) else {
        return;
    };
    let slug = slug_of(tx.conn(), project);
    let (from_at, to_at) = (kept_path(&slug, was), kept_path(&slug, &export.name));
    if link_on_the_way(folder, &from_at).is_some() || link_on_the_way(folder, &to_at).is_some() {
        return;
    }
    let from = absolute(folder, &from_at);
    let to = absolute(folder, &to_at);
    tx.after_commit(move || {
        if std::path::Path::new(&from).exists() {
            let _ = std::fs::rename(from, to);
        }
    });
}

/// What an export kept taken away from its default folder on this machine once the transaction
/// commits; what it wrote stays.
pub(crate) fn forget(tx: &mut Tx, project: &str, export: &Export) {
    let Ok(defaults) = super::exports::default_folders(tx.conn(), project) else {
        return;
    };
    if let Some(folder) = defaults.get(&export.id) {
        let slug = slug_of(tx.conn(), project);
        let kept_at = kept_path(&slug, &export.name);
        if link_on_the_way(folder, &kept_at).is_some() {
            return;
        }
        let kept = absolute(folder, &kept_at);
        tx.after_commit(move || {
            let _ = std::fs::remove_file(kept);
        });
    }
}

/// What each export kept in its default folder on this machine, moved to the folder of the
/// project's new slug once the transaction commits, and the old slug's folder let go when that
/// leaves it empty. What is kept in
/// another folder written to is not touched: it is not known.
pub(crate) fn slug_changed(tx: &mut Tx, project: &str, was: &str, now: &str) -> Result<(), String> {
    let defaults = super::exports::default_folders(tx.conn(), project)?;
    let mut moves: Vec<(String, String, String)> = Vec::new();
    for export in super::exports::exports(tx.conn(), project)? {
        if let Some(folder) = defaults.get(&export.id)
            && link_on_the_way(folder, &kept_path(was, &export.name)).is_none()
            && link_on_the_way(folder, &kept_path(now, &export.name)).is_none()
        {
            moves.push((
                absolute(folder, &kept_path(was, &export.name)),
                absolute(folder, &kept_path(now, &export.name)),
                absolute(
                    folder,
                    &format!("{MARK_FOLDER}/{}", crate::export::folder_name(was)),
                ),
            ));
        }
    }
    tx.after_commit(move || {
        for (from, to, old_folder) in moves {
            if !std::path::Path::new(&from).exists() {
                continue;
            }
            if let Some(parent) = std::path::Path::new(&to).parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if std::fs::rename(&from, &to).is_ok() {
                // Only an empty folder goes.
                let _ = std::fs::remove_dir(old_folder);
            }
        }
    });
    Ok(())
}

/// Everything the exports write in a tree: the cards as read, the exports and what they write.
struct Plan {
    seen: Seen,
    exports: Vec<Export>,
    built: Built,
}

impl Plan {
    fn of(store: &Store, project: &str, branch: Option<&str>) -> Result<Plan, String> {
        let seen = Seen::read(store, project, branch)?;
        let exports = store.exports(&seen.summary.id)?;
        let grid = crate::tools::grid::Layout::read(&store.conn, &seen.summary.id)?;
        let built = crate::export::build(&seen.detail, &exports, &grid);
        Ok(Plan {
            seen,
            exports,
            built,
        })
    }

    /// The files export `at` writes, in the Finder's order.
    fn files_of(&self, at: usize) -> Vec<&Planned> {
        let mut out: Vec<&Planned> = self
            .built
            .planned
            .iter()
            .filter(|f| f.export == at)
            .collect();
        out.sort_by(|a, b| standard_order(&a.path, &b.path));
        out
    }

    fn content(&self, file: &Planned) -> Option<&String> {
        self.built
            .files
            .get(&self.exports.get(file.export)?.id)?
            .get(&file.path)
    }
}

/// How one file stands against what its export kept and the folder.
struct Standing {
    status: &'static str,
    changed_cards: Vec<String>,
    output_changed: bool,
}

fn standing(plan: &Plan, file: &Planned, kept: Option<&Kept>, folder: Option<&str>) -> Standing {
    let project = plan.seen.project();
    let now = plan.content(file).map(|c| sha256(c.as_bytes()));
    let Some(before) = kept.and_then(|k| k.files.iter().find(|f| f.path == file.path)) else {
        return Standing {
            status: "unwritten",
            changed_cards: Vec::new(),
            output_changed: false,
        };
    };
    let mut changed_cards: Vec<String> = file
        .cards
        .iter()
        .filter(|id| {
            let hash = card_hash(&plan.seen.raw, id);
            !before
                .cards
                .iter()
                .any(|(was, _, then)| was == *id && *then == hash)
        })
        .filter_map(|id| project.card(id).map(|c| project.full(c)))
        .collect();
    // A card written then and not now.
    for (id, path, _) in &before.cards {
        if !file.cards.contains(id) {
            changed_cards.push(path.clone());
        }
    }
    let output_changed = now.as_deref() != Some(before.sha256.as_str());
    let on_disk = folder.map(|f| std::fs::read(absolute(f, &file.path)).ok());
    let status = match on_disk {
        Some(None) => "missing",
        Some(Some(bytes)) if sha256(&bytes) != before.sha256 => "edited",
        _ if !changed_cards.is_empty() || output_changed => "changed",
        _ => "current",
    };
    Standing {
        status,
        changed_cards,
        output_changed,
    }
}

/// Why a folder given to write to cannot be, or none: not there, or not a folder.
fn folder_missing(folder: &str) -> Option<String> {
    let full = absolute(folder, "");
    if std::path::Path::new(&full).is_dir() {
        None
    } else {
        Some(text!("files.folder.missing", folder))
    }
}

/// The working directories bound on this machine, each by its name and its folder in full: where a
/// file written into one of them is a file of its tree.
fn homes_of(conn: &rusqlite::Connection, project: &str) -> Result<Vec<(String, String)>, String> {
    let folders = super::directories::folders(conn, project)?;
    Ok(super::directories::directories(conn, project)?
        .into_iter()
        .filter_map(|d| {
            let folder = folders.get(&d.id)?;
            Some((d.name, absolute(folder, "")))
        })
        .collect())
}

/// Where a file written to `folder` is in the tree of a working directory — its name and the path
/// in it — when it is written into the folder of one (the deepest, if folders are in folders).
fn in_directory(homes: &[(String, String)], folder: &str, path: &str) -> Option<(String, String)> {
    let full = absolute(folder, path);
    homes
        .iter()
        .filter(|(_, home)| {
            full.strip_prefix(home.as_str())
                .is_some_and(|rest| home.ends_with('/') || rest.starts_with('/'))
        })
        .max_by_key(|(_, home)| home.len())
        .map(|(name, home)| {
            (
                name.clone(),
                full[home.len()..].trim_start_matches('/').to_string(),
            )
        })
}

/// One export as `export_read` says it: its settings, its files and how each stands, what writing
/// again takes away — against `folder`, where it is written to, when that is known; from main's cards
/// or a branch's. A file that is
/// written into a working directory's folder says which and where in it (`directory`).
fn export_said(plan: &Plan, at: usize, folder: Option<&str>, homes: &[(String, String)]) -> Value {
    let export = &plan.exports[at];
    let project = plan.seen.project();
    let mut out = match super::exports::said(export, &project) {
        Value::Object(o) => o,
        _ => Map::new(),
    };
    out.insert(
        "path".into(),
        folder.map_or(Value::Null, |f| f.to_string().into()),
    );
    let slug = plan.seen.summary.slug.clone();
    let kept = folder.and_then(|f| read_kept(f, &slug, &export.name));
    let files = plan.files_of(at);
    let mut rows = Vec::new();
    for file in &files {
        let mut row = Map::new();
        row.insert("path".into(), file.path.clone().into());
        row.insert("content".into(), file.content.word().into());
        row.insert(
            "cards".into(),
            Value::Array(
                file.cards
                    .iter()
                    .filter_map(|id| project.card(id))
                    .map(|c| project.full(c).into())
                    .collect(),
            ),
        );
        if let Some(place) = folder.and_then(|f| in_directory(homes, f, &file.path)) {
            row.insert(
                "directory".into(),
                json!({"name": place.0, "path": place.1}),
            );
        }
        if folder.is_some() {
            let stands = standing(plan, file, kept.as_ref(), folder);
            row.insert("status".into(), stands.status.into());
            if !stands.changed_cards.is_empty() {
                row.insert("changedCards".into(), json!(stands.changed_cards));
            }
            if stands.output_changed && stands.status != "unwritten" {
                row.insert("outputChanged".into(), true.into());
            }
        }
        rows.push(Value::Object(row));
    }
    out.insert("files".into(), Value::Array(rows));
    if let Some(kept) = &kept {
        if !kept.written_at.is_empty() {
            out.insert("writtenAt".into(), kept.written_at.clone().into());
        }
        let removing: Vec<&str> = kept
            .files
            .iter()
            .filter(|k| !files.iter().any(|f| f.path == k.path))
            .map(|k| k.path.as_str())
            .collect();
        if !removing.is_empty() {
            out.insert("removing".into(), json!(removing));
        }
    }
    if let Some(folder) = folder {
        if let Some(missing) = folder_missing(folder) {
            out.insert("note".into(), missing.into());
        }
    } else {
        out.insert(
            "note".into(),
            text!("files.note.noDefault", name = export.name).into(),
        );
    }
    Value::Object(out)
}

/// How many of an export's files stand each way.
fn counts(plan: &Plan, at: usize, folder: Option<&str>) -> Value {
    let export = &plan.exports[at];
    let files = plan.files_of(at);
    let mut out = Map::new();
    out.insert("name".into(), export.name.clone().into());
    out.insert("files".into(), files.len().into());
    let Some(folder) = folder else {
        out.insert("path".into(), Value::Null);
        return Value::Object(out);
    };
    out.insert("path".into(), folder.to_string().into());
    if let Some(missing) = folder_missing(folder) {
        out.insert("note".into(), missing.into());
    }
    let kept = read_kept(folder, &plan.seen.summary.slug, &export.name);
    let mut by: BTreeMap<&str, usize> = BTreeMap::new();
    for file in &files {
        *by.entry(standing(plan, file, kept.as_ref(), Some(folder)).status)
            .or_default() += 1;
    }
    for (status, n) in by {
        if status != "current" {
            out.insert(status.into(), n.into());
        }
    }
    if let Some(kept) = &kept {
        let removing = kept
            .files
            .iter()
            .filter(|k| !files.iter().any(|f| f.path == k.path))
            .count();
        if removing > 0 {
            out.insert("removing".into(), removing.into());
        }
    }
    Value::Object(out)
}

/// What stops the writing: two exports reaching the same file in the folders they are written to.
fn folder_problems(plan: &Plan, dests: &[Option<String>]) -> Vec<String> {
    let mut owner: HashMap<String, String> = HashMap::new();
    let mut clashes = Vec::new();
    for file in &plan.built.planned {
        let Some(Some(folder)) = dests.get(file.export) else {
            continue;
        };
        let full = absolute(folder, &file.path);
        let said = format!("{}/{}", plan.exports[file.export].name, file.path);
        match owner.get(&full) {
            Some(other) if *other != said => {
                clashes.push(text!("files.problem.clash", other, said, full))
            }
            Some(_) => {}
            None => {
                owner.insert(full, said);
            }
        }
    }
    if clashes.is_empty() {
        Vec::new()
    } else {
        vec![text!("files.problem.overlap", clashes = clashes.join("\n"))]
    }
}

/// `export_read`: the exports — each with how many of its files stand each way — and the problems;
/// those `target` names in full; with `value.files`, those files of the one named, each by its path
/// with its text (`files: [{path, text}]`); with
/// `branch`, what main's exports would write from that branch's cards and how that stands against what
/// was written. `targetdir` is always given:
/// `[default]` (each export's folder on this machine) or the folder to read them against.
pub(crate) fn export_read(store: &Store, args: &Args) -> Result<Output, String> {
    let given = args.value()?;
    if let Some(stray) = given.keys().find(|k| *k != "files") {
        return Err(text!("files.read.strayKey", stray));
    }
    let target = Target::of(args)?;
    let branch = args.optional_string("branch");
    let plan = Plan::of(store, args.string("project")?, branch)?;
    let defaults = super::exports::default_folders(&store.conn, &plan.seen.summary.id)?;
    let wanted = args.targets()?.filter(|t| !t.is_empty());
    if let Some(files) = given.get("files") {
        let Some(names) = &wanted else {
            return Err(text!("files.read.filesOne"));
        };
        let [name] = names.as_slice() else {
            return Err(text!("files.read.filesOne"));
        };
        let at = position(&plan.exports, name)?;
        let paths: Vec<String> = files
            .as_array()
            .ok_or_else(|| text!("files.read.filesList"))?
            .iter()
            .filter_map(|p| p.as_str().map(|p| p.trim().to_string()))
            .collect();
        let mine = plan.files_of(at);
        let mut read = Vec::new();
        for path in paths {
            let Some(file) = mine.iter().find(|f| f.path == path) else {
                return Err(text!(
                    "files.read.notWritten",
                    name = plan.exports[at].name,
                    path,
                    files = if mine.is_empty() {
                        text!("files.read.noFiles")
                    } else {
                        crate::text::list(&mine.iter().map(|f| f.path.as_str()).collect::<Vec<_>>())
                    }
                ));
            };
            let text = plan.content(file).cloned().unwrap_or_default();
            let mut one = Map::new();
            one.insert("path".into(), path.into());
            one.insert("text".into(), text.into());
            read.push(Value::Object(one));
        }
        let mut out = Map::new();
        out.insert("project".into(), plan.seen.summary.name.clone().into());
        out.insert("export".into(), plan.exports[at].name.clone().into());
        out.insert("files".into(), Value::Array(read));
        return Ok(Output::Json(Value::Object(out)));
    }
    let chosen: Vec<usize> = match &wanted {
        Some(names) => names
            .iter()
            .map(|n| position(&plan.exports, n))
            .collect::<Result<_, _>>()?,
        None => (0..plan.exports.len()).collect(),
    };
    let dests = destinations(&plan, &defaults, &target, &chosen);
    let homes = homes_of(&store.conn, &plan.seen.summary.id)?;
    let mut out = Map::new();
    out.insert("project".into(), plan.seen.summary.name.clone().into());
    match &wanted {
        None => {
            out.insert(
                "exports".into(),
                Value::Array(
                    (0..plan.exports.len())
                        .map(|at| counts(&plan, at, dests[at].as_deref()))
                        .collect(),
                ),
            );
            if plan.exports.is_empty() {
                out.insert("hint".into(), text!("files.read.noExports").into());
            }
        }
        Some(_) => {
            let mut said = Vec::new();
            for at in &chosen {
                said.push(export_said(&plan, *at, dests[*at].as_deref(), &homes));
            }
            out.insert("exports".into(), Value::Array(said));
        }
    }
    // With exports named, only theirs: what stops another export does not stop these.
    let mut problems: Vec<String> = plan
        .built
        .problems
        .iter()
        .filter(|p| p.stops(&chosen))
        .map(crate::export::Problem::text)
        .collect();
    problems.extend(folder_problems(&plan, &dests));
    if !problems.is_empty() {
        out.insert("problems".into(), json!(problems));
    }
    Ok(plan.seen.headed(Output::Json(Value::Object(out))))
}

/// Why an export may not be written into `folder`: each link on the way to a file it writes or to
/// what it keeps there, once.
fn links_in_the_way(plan: &Plan, at: usize, folder: &str) -> Vec<String> {
    let kept_at = kept_path(&plan.seen.summary.slug, &plan.exports[at].name);
    let mut out: Vec<String> = Vec::new();
    for path in plan
        .files_of(at)
        .iter()
        .map(|f| f.path.as_str())
        .chain(std::iter::once(kept_at.as_str()))
        .filter(|p| is_inside(p))
    {
        if let Some(why) = through_link(folder, path)
            && !out.contains(&why)
        {
            out.push(why);
        }
    }
    out
}

fn position(list: &[Export], name: &str) -> Result<usize, String> {
    let found = super::exports::named(list, name)?;
    Ok(list.iter().position(|e| e.id == found.id).expect("named"))
}

/// What writing one export does in its folder.
struct Wrote {
    written: Vec<String>,
    removed: Vec<String>,
}

/// One export's files written under its folder, and what it kept of them — the branch written from,
/// for one; what it wrote before and does not now taken away — not what another export writes there
/// (`others`, in full).
fn write_export(
    plan: &Plan,
    at: usize,
    folder: &str,
    others: &HashSet<String>,
    commit: Option<&str>,
    now: &str,
) -> Result<Wrote, String> {
    let export = &plan.exports[at];
    let slug = &plan.seen.summary.slug;
    let files = plan.files_of(at);
    let kept_at = kept_path(slug, &export.name);
    let before = read_kept(folder, slug, &export.name).unwrap_or_default();
    if let Some(stray) = files
        .iter()
        .map(|f| f.path.as_str())
        .chain(std::iter::once(kept_at.as_str()))
        .find(|p| !is_inside(p))
    {
        return Err(text!("files.write.outside", stray = format!("{stray:?}")));
    }
    let base = PathBuf::from(absolute(folder, ""));
    let failed = |e: std::io::Error| text!("files.write.failed", folder, e);
    let project = plan.seen.project();
    let mut kept_files = Vec::new();
    let mut written = Vec::new();
    if let Some(why) = files
        .iter()
        .map(|f| f.path.as_str())
        .chain(std::iter::once(kept_at.as_str()))
        .find_map(|p| through_link(folder, p))
    {
        return Err(why);
    }
    for file in &files {
        let content = plan.content(file).cloned().unwrap_or_default();
        let at = base.join(&file.path);
        if let Some(parent) = at.parent() {
            std::fs::create_dir_all(parent).map_err(failed)?;
        }
        std::fs::write(&at, &content).map_err(failed)?;
        written.push(file.path.clone());
        kept_files.push(json!({
            "path": file.path,
            "content": file.content.word(),
            "sha256": sha256(content.as_bytes()),
            "cards": file.cards.iter().filter_map(|id| project.card(id)).map(|c| json!({
                "path": project.full(c),
                "id": c.id,
                "hash": card_hash(&plan.seen.raw, &c.id),
            })).collect::<Vec<_>>(),
        }));
    }
    let mut kept = Map::new();
    kept.insert("project".into(), plan.seen.summary.name.clone().into());
    kept.insert("projectId".into(), plan.seen.summary.id.clone().into());
    kept.insert("export".into(), export.name.clone().into());
    kept.insert("exportId".into(), export.id.clone().into());
    if !plan.seen.named.tree.is_empty() {
        kept.insert("branch".into(), plan.seen.named.name.clone().into());
    }
    kept.insert("writtenAt".into(), now.into());
    kept.insert("commit".into(), commit.into());
    kept.insert("app".into(), crate::VERSION.into());
    kept.insert("files".into(), Value::Array(kept_files));
    let kept = Value::Object(kept);
    let kept_file = base.join(&kept_at);
    if let Some(parent) = kept_file.parent() {
        std::fs::create_dir_all(parent).map_err(failed)?;
    }
    std::fs::write(&kept_file, crate::ojson::pretty(&kept) + "\n").map_err(failed)?;
    // What it wrote before and does not now: taken away where the folder it is in really is under the
    // folder written (a folder in the path may be a link to somewhere else).
    let here = std::fs::canonicalize(&base).map_err(failed)?;
    let mut removed = Vec::new();
    let mut gone: Vec<&str> = before
        .files
        .iter()
        .map(|f| f.path.as_str())
        .filter(|p| !written.iter().any(|w| w == p) && is_inside(p))
        .filter(|p| !others.contains(&absolute(folder, p)))
        .collect();
    gone.sort();
    gone.dedup();
    for path in gone {
        let file = base.join(path);
        let inside = file
            .parent()
            .and_then(|p| std::fs::canonicalize(p).ok())
            .is_some_and(|d| d.starts_with(&here));
        if inside && file.exists() && std::fs::remove_file(&file).is_ok() {
            removed.push(path.to_string());
        }
    }
    Ok(Wrote { written, removed })
}

impl crate::Core {
    /// `export_execute`: the exports `target` names (every one, when it names none) written from
    /// main — or the branch `branch` names — into `targetdir`: a folder, or `[default]`, each into its default
    /// folder on this machine, which is main's alone to write. Everything checked first, nothing
    /// written when anything is wrong. `dryRun` says what it would write and take away, and writes
    /// nothing. A mark in the history of the tree written from, a line for each.
    pub(crate) fn export_execute(
        &self,
        who: &crate::write::Who,
        args: &Args,
    ) -> Result<Output, String> {
        let given = args.value()?;
        if let Some(stray) = given.keys().find(|k| *k != "dryRun") {
            return Err(text!("files.execute.strayKey", stray));
        }
        let dry_run = match given.get("dryRun") {
            None | Some(Value::Bool(false)) => false,
            Some(Value::Bool(true)) => true,
            Some(_) => return Err(text!("files.execute.dryRunBool")),
        };
        let target = Target::of(args)?;
        let (plan, defaults, commit, now) = {
            let store = self.lock();
            let plan = Plan::of(
                &store,
                args.string("project")?,
                args.optional_string("branch"),
            )?;
            let tree = &plan.seen.named.tree;
            if tree == crate::store::BOARD {
                return Err(text!(
                    "files.execute.notWritable",
                    name = plan.seen.named.name
                ));
            }
            if !tree.is_empty() && !target.is_folder() {
                return Err(branch_to_default());
            }
            // An archived project is read only: nothing written out, no mark in its history — by MCP
            // and from the window alike; what it would write may still be looked at (`dryRun`).
            if plan.seen.summary.archived_at.is_some() && !dry_run {
                return Err(crate::write::archived());
            }
            let defaults = super::exports::default_folders(&store.conn, &plan.seen.summary.id)?;
            let commit: Option<String> = store
                .conn
                .query_row(
                    "SELECT id FROM commits WHERE project_id = ? AND branch_id = ? ORDER BY at DESC LIMIT 1",
                    [&plan.seen.summary.id, tree],
                    |r| r.get(0),
                )
                .ok();
            let now = crate::write::now(&store.conn)?;
            (plan, defaults, commit, now)
        };
        if plan.exports.is_empty() {
            return Err(text!("files.execute.noExports"));
        }
        let chosen: Vec<usize> = match args.targets()?.filter(|t| !t.is_empty()) {
            Some(names) => names
                .iter()
                .map(|n| position(&plan.exports, n))
                .collect::<Result<_, _>>()?,
            None => (0..plan.exports.len()).collect(),
        };
        let dests = destinations(&plan, &defaults, &target, &chosen);
        // Only what stops the exports written: another export's problem is that export's.
        let mut problems: Vec<String> = plan
            .built
            .problems
            .iter()
            .filter(|p| p.stops(&chosen))
            .map(crate::export::Problem::text)
            .collect();
        problems.extend(folder_problems(&plan, &dests));
        for at in &chosen {
            match &dests[*at] {
                None => problems.push(text!(
                    "files.execute.noDefault",
                    name = plan.exports[*at].name
                )),
                Some(folder) => match folder_missing(folder) {
                    Some(missing) => problems.push(missing),
                    None => problems.extend(links_in_the_way(&plan, *at, folder)),
                },
            }
        }
        problems.dedup();
        let slug = plan.seen.summary.slug.clone();
        if dry_run {
            let mut out = Map::new();
            out.insert("project".into(), plan.seen.summary.name.clone().into());
            if !plan.seen.named.tree.is_empty() {
                out.insert("branch".into(), plan.seen.named.name.clone().into());
            }
            out.insert("dryRun".into(), true.into());
            out.insert(
                "exports".into(),
                Value::Array(
                    chosen
                        .iter()
                        .map(|at| {
                            let export = &plan.exports[*at];
                            let files = plan.files_of(*at);
                            let mut row = Map::new();
                            row.insert("name".into(), export.name.clone().into());
                            row.insert(
                                "path".into(),
                                dests[*at]
                                    .as_ref()
                                    .map_or(Value::Null, |f| f.clone().into()),
                            );
                            row.insert(
                                "write".into(),
                                json!(files.iter().map(|f| f.path.as_str()).collect::<Vec<_>>()),
                            );
                            if let Some(kept) = dests[*at]
                                .as_deref()
                                .and_then(|f| read_kept(f, &slug, &export.name))
                            {
                                let remove: Vec<&str> = kept
                                    .files
                                    .iter()
                                    .filter(|k| !files.iter().any(|f| f.path == k.path))
                                    .map(|k| k.path.as_str())
                                    .collect();
                                if !remove.is_empty() {
                                    row.insert("remove".into(), json!(remove));
                                }
                            }
                            Value::Object(row)
                        })
                        .collect(),
                ),
            );
            if !problems.is_empty() {
                out.insert("problems".into(), json!(problems));
            }
            return Ok(Output::Json(Value::Object(out)));
        }
        if !problems.is_empty() {
            return Err(problems.join("\n"));
        }
        // Every file every export writes, in full: what one writes, another does not take away.
        let everything: Vec<(usize, String)> = plan
            .built
            .planned
            .iter()
            .filter_map(|f| {
                let folder = dests.get(f.export)?.as_ref()?;
                Some((f.export, absolute(folder, &f.path)))
            })
            .collect();
        let mut out_rows = Vec::new();
        let mut lines = Vec::new();
        for at in &chosen {
            let export = &plan.exports[*at];
            let folder = dests[*at].as_ref().expect("checked");
            let others: HashSet<String> = everything
                .iter()
                .filter(|(e, _)| e != at)
                .map(|(_, p)| p.clone())
                .collect();
            let done = write_export(&plan, *at, folder, &others, commit.as_deref(), &now)?;
            // What was kept before exports had names, under the folder: let go of.
            let prefix = format!("exports-{}-", plan.seen.summary.id);
            if link_on_the_way(folder, MARK_FOLDER).is_none()
                && let Ok(listing) = std::fs::read_dir(absolute(folder, MARK_FOLDER))
            {
                for entry in listing.flatten() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if name.starts_with(&prefix) && name.ends_with(".json") {
                        let _ = std::fs::remove_file(entry.path());
                    }
                }
            }
            lines.push(if done.removed.is_empty() {
                keyed!(
                    "files.line.wrote",
                    name = export.name,
                    folder,
                    count = done.written.len()
                )
            } else {
                keyed!(
                    "files.line.wroteRemoved",
                    name = export.name,
                    folder,
                    count = done.written.len(),
                    removed = done.removed.len()
                )
            });
            out_rows.push(json!({
                "name": export.name,
                "path": folder,
                "written": done.written,
                "removed": done.removed,
            }));
        }
        let project = plan.seen.summary.id.clone();
        let tree = plan.seen.named.tree.clone();
        self.plain_writing(who.clone(), |tx| {
            tx.mark(&project, &tree, "export");
            for said in &lines {
                tx.line(&project, &tree, "export", "project", None, said.clone())?;
            }
            Ok(())
        })?;
        let mut out = Map::new();
        out.insert("project".into(), plan.seen.summary.name.clone().into());
        if !tree.is_empty() {
            out.insert("branch".into(), plan.seen.named.name.clone().into());
        }
        out.insert("exports".into(), Value::Array(out_rows));
        Ok(Output::Json(Value::Object(out)))
    }
}
