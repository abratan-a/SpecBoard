//! The exports (書き出し) a project keeps: each a name, what goes where (`entries`) and what is left out (`exclude`) — the project's
//! settings, as its working directories are: not a branch's, and not held by main's lock. An export
//! holds no place to write to: that is given when it is written (`targetdir`), and each Mac keeps one default for it
//! (`export_default_folders`). The cards and the sections held their export settings
//! themselves before (`exportTo`); a project of before has them moved here once (`export_move`).
//!
//! An entry is `{from, to, format, kinds}`: `from` a card, or a section and every card under it (its
//! sections below too); `to` a file, or a folder (`/` at its end) a document a file of its own in;
//! `format` the language of the types (`typescript`, `zod`, `json-schema`; else said by the file's
//! extension); `kinds` the kinds of card a section's entry takes. A section's entry with no kinds takes
//! what a section's export setting took before: the cards that may be written to `to`, but a flow,
//! and a note only in a schema section. In one export a card goes by the entries from what is nearest
//! it — itself, then its section, then the sections above — so a section's own entries stand in for
//! those of the sections above it. `exclude` names cards and sections the entries of the sections
//! above them leave out (an entry from one of them still writes it). A ui section under an entry's
//! section is written as one page, its mock (`mock`), by its nearest entry the same way: in a folder by
//! the names of the sections from the entry's down, or to the entry's `.html` file when the entry is
//! its own; its parts are not files of their own. Kept by id, so a card or a section renamed or moved
//! is followed; said by path.

use crate::model::{Card, ExportSetting};
use serde_json::{Map, Value, json};
use std::collections::{HashMap, HashSet};

/// One export.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Export {
    pub id: String,
    pub name: String,
    pub entries: Vec<Entry>,
    /// Card and section ids.
    pub exclude: Vec<String>,
}

/// What goes where: a card or a section's cards — or one version of a stackdoc — into a file
/// or a folder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// A card's id or a section's.
    pub from: String,
    /// A stackdoc's version, by its row's id: that version alone. None for the card whole.
    pub row: Option<String>,
    pub to: String,
    pub format: Option<String>,
    /// The kinds of card a section's entry takes; none: as a section's export setting did before.
    pub kinds: Vec<String>,
}

impl Entry {
    /// As a card's export setting was: the file it writes, in the place of the export `export`
    /// (its id; what a file belongs to — the place itself is given when it is written).
    pub fn setting(&self, export: &str) -> ExportSetting {
        ExportSetting {
            kind: "file".into(),
            dir: Some(export.to_string()),
            path: self.to.clone(),
            format: self.format.clone(),
        }
    }

    fn of(value: &Value) -> Option<Entry> {
        Some(Entry {
            from: value.get("from")?.as_str()?.to_string(),
            row: value.get("row").and_then(Value::as_str).map(str::to_string),
            to: value.get("to")?.as_str()?.to_string(),
            format: value
                .get("format")
                .and_then(Value::as_str)
                .map(str::to_string),
            kinds: value
                .get("kinds")
                .and_then(Value::as_array)
                .map(|list| {
                    list.iter()
                        .filter_map(|k| k.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default(),
        })
    }

    fn kept(&self) -> Value {
        let mut out = Map::new();
        out.insert("from".into(), self.from.clone().into());
        if let Some(row) = &self.row {
            out.insert("row".into(), row.clone().into());
        }
        out.insert("to".into(), self.to.clone().into());
        if let Some(format) = &self.format {
            out.insert("format".into(), format.clone().into());
        }
        if !self.kinds.is_empty() {
            out.insert("kinds".into(), json!(self.kinds));
        }
        Value::Object(out)
    }
}

impl Export {
    fn of(value: &Value) -> Option<Export> {
        Some(Export {
            id: value.get("id")?.as_str()?.to_string(),
            name: value.get("name")?.as_str()?.to_string(),
            entries: value
                .get("entries")
                .and_then(Value::as_array)
                .map(|list| list.iter().filter_map(Entry::of).collect())
                .unwrap_or_default(),
            exclude: value
                .get("exclude")
                .and_then(Value::as_array)
                .map(|list| {
                    list.iter()
                        .filter_map(|e| e.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default(),
        })
    }

    fn kept(&self) -> Value {
        let mut out = Map::new();
        out.insert("id".into(), self.id.clone().into());
        out.insert("name".into(), self.name.clone().into());
        out.insert(
            "entries".into(),
            Value::Array(self.entries.iter().map(Entry::kept).collect()),
        );
        if !self.exclude.is_empty() {
            out.insert("exclude".into(), json!(self.exclude));
        }
        Value::Object(out)
    }
}

/// The exports as the project's row keeps them (`projects.exports`); none when there are none or
/// they cannot be read.
pub fn parse(text: Option<&str>) -> Vec<Export> {
    text.and_then(|t| serde_json::from_str::<Value>(t).ok())
        .and_then(|v| v.as_array().cloned())
        .map(|list| list.iter().filter_map(Export::of).collect())
        .unwrap_or_default()
}

/// The exports as the project's row keeps them; none for none.
pub fn kept(exports: &[Export]) -> Option<String> {
    (!exports.is_empty())
        .then(|| crate::ojson::compact(&Value::Array(exports.iter().map(Export::kept).collect())))
}

/// One card's place in an export: which export (by its place in the list) and the file — of
/// one version of it, for a stackdoc's version's entry (its row's id).
#[derive(Clone, Debug)]
pub struct Placed {
    pub export: usize,
    pub setting: ExportSetting,
    pub row: Option<String>,
}

/// Where each card of a tree goes, by its id, the page of each ui section, and what is wrong with
/// the exports there.
#[derive(Default)]
pub struct Resolved {
    pub by_card: HashMap<String, Vec<Placed>>,
    pub pages: Vec<Page>,
    pub problems: Vec<super::Problem>,
}

/// A ui section's mock written as a page of its own: by which export (its place in the list), the
/// section's id and the file.
#[derive(Clone, Debug)]
pub struct Page {
    pub export: usize,
    pub section: String,
    pub path: String,
}

/// The file a ui section's page is written to by an entry from `depth` sections above it (`names`
/// the sections' names from the entry's down to it): in a folder, a page named by the sections
/// below the folder's entry, so pages of sections with one name in different places do not meet; a
/// `.html` file holds the page of the entry's own section alone. None: the entry writes no page.
fn page_path(to: &str, names: &[&str], depth: usize) -> Option<String> {
    let to = to.trim_matches(|c: char| c == ' ' || c == '\t');
    if to.ends_with('/') {
        let named: Vec<String> = names.iter().map(|n| super::folder_name(n)).collect();
        return Some(format!("{to}{}.html", named.join("/")));
    }
    (depth == 0 && to.to_lowercase().ends_with(".html")).then(|| to.to_string())
}

/// Whether a section's entry with no kinds takes a card: as a section's export setting did — a
/// flow never, a note only in a schema section — and where the card may be written to.
fn takes(card: &Card, entry: &Entry, export: &str, project: &super::Project) -> bool {
    let fits = entry.setting(export).problem_for(&card.kind).is_none();
    if !entry.kinds.is_empty() {
        return entry.kinds.contains(&card.kind);
    }
    fits && match card.kind.as_str() {
        "action" | "flow" => false,
        "note" => project.section_kind_of(card) == Some("schema"),
        _ => true,
    }
}

/// Where each card of the project's tree goes by the exports, and what is wrong with them: an entry
/// whose card or section is not there, a card written where it may not be.
pub fn resolve(exports: &[Export], project: &super::Project) -> Resolved {
    let detail = project.detail;
    let parent: HashMap<&str, Option<&str>> = detail
        .sections
        .iter()
        .map(|s| (s.id.as_str(), s.parent_id.as_deref()))
        .collect();
    // A section's sections above it, nearest first, itself first.
    let chain = |section: &str| -> Vec<String> {
        let mut out = Vec::new();
        let mut at = Some(section.to_string());
        while let Some(id) = at {
            if out.contains(&id) {
                break;
            }
            at = parent
                .get(id.as_str())
                .copied()
                .flatten()
                .map(str::to_string);
            out.push(id);
        }
        out
    };
    let cards: HashMap<&str, &Card> = detail
        .schemas
        .iter()
        .map(|c| (c.id.as_str(), &**c))
        .collect();
    let mut out = Resolved::default();
    for (at, export) in exports.iter().enumerate() {
        let left_out: HashSet<&str> = export.exclude.iter().map(String::as_str).collect();
        // Each card's nearest entries: how near (0 the card itself, then each section up), and
        // which.
        let mut nearest: HashMap<String, (usize, Vec<usize>)> = HashMap::new();
        // Each ui section's nearest entry, as a card's: how near, and which.
        let mut nearest_page: HashMap<String, (usize, usize)> = HashMap::new();
        for (n, entry) in export.entries.iter().enumerate() {
            if !parent.contains_key(entry.from.as_str()) {
                continue;
            }
            for section in &detail.sections {
                if project.section_kind(&section.id) != Some(crate::tools::frames::UI) {
                    continue;
                }
                let above = chain(&section.id);
                let Some(depth) = above.iter().position(|s| *s == entry.from) else {
                    continue;
                };
                // Left out: the section, or one between it and the entry's.
                if above[..depth].iter().any(|s| left_out.contains(s.as_str())) {
                    continue;
                }
                let row = nearest_page.entry(section.id.clone()).or_insert((depth, n));
                if depth < row.0 {
                    *row = (depth, n);
                }
            }
        }
        for section in &detail.sections {
            let Some(&(depth, n)) = nearest_page.get(&section.id) else {
                continue;
            };
            let above = chain(&section.id);
            let names: Vec<&str> = above[..=depth]
                .iter()
                .rev()
                .filter_map(|id| detail.sections.iter().find(|s| &s.id == id))
                .map(|s| s.name.as_str())
                .collect();
            if let Some(path) = page_path(&export.entries[n].to, &names, depth) {
                out.pages.push(Page {
                    export: at,
                    section: section.id.clone(),
                    path,
                });
            }
        }
        let mut put = |card: &Card, depth: usize, entry: usize| {
            let row = nearest
                .entry(card.id.clone())
                .or_insert((depth, Vec::new()));
            if depth < row.0 {
                *row = (depth, vec![entry]);
            } else if depth == row.0 && !row.1.contains(&entry) {
                row.1.push(entry);
            }
        };
        for (n, entry) in export.entries.iter().enumerate() {
            // A format writes one file: an entry of a section to a folder takes no card.
            if entry.kinds.is_empty()
                && !cards.contains_key(entry.from.as_str())
                && let Some(format) = &entry.format
                && entry
                    .to
                    .trim_matches(|c: char| c == ' ' || c == '\t')
                    .ends_with('/')
            {
                out.problems.push(super::Problem::of(
                    at,
                    text!(
                        "export.settings.formatFolder",
                        export = export.name,
                        row = n + 1,
                        format,
                        to = entry.to
                    ),
                ));
            }
            if let Some(card) = cards.get(entry.from.as_str()) {
                put(card, 0, n);
                continue;
            }
            if !parent.contains_key(entry.from.as_str()) {
                out.problems.push(super::Problem::of(
                    at,
                    text!(
                        "export.settings.fromGone",
                        export = export.name,
                        row = n + 1,
                        to = entry.to
                    ),
                ));
                continue;
            }
            for card in &detail.schemas {
                let above = chain(&project.section_of(card));
                let Some(depth) = above.iter().position(|s| *s == entry.from) else {
                    continue;
                };
                // What a working directory's section holds is read from a folder, not written
                // out — the notes put there with it.
                if above.iter().any(|s| {
                    matches!(
                        project.section_kind(s),
                        Some(crate::kinds::WORKDIR | crate::kinds::DIRECTORY)
                    )
                }) {
                    continue;
                }
                // A ui section's parts are its page, not files of their own.
                if project.section_kind_of(card) == Some(crate::tools::frames::UI) {
                    continue;
                }
                // Left out: the card, or a section between it and the entry's.
                if left_out.contains(card.id.as_str())
                    || above[..depth].iter().any(|s| left_out.contains(s.as_str()))
                {
                    continue;
                }
                if takes(card, entry, &export.id, project) {
                    put(card, depth + 1, n);
                }
            }
        }
        // In the project's order.
        for card in &detail.schemas {
            let Some((_, entries)) = nearest.get(&card.id) else {
                continue;
            };
            for n in entries {
                let entry = &export.entries[*n];
                let setting = entry.setting(&export.id);
                if let Some(problem) = setting.problem_for(&card.kind) {
                    out.problems.push(super::Problem::of(
                        at,
                        text!(
                            "export.settings.problem",
                            export = export.name,
                            card = project.full(card),
                            problem
                        ),
                    ));
                    continue;
                }
                if let Some(row) = &entry.row
                    && !card.versions().iter().any(|v| &v.id == row)
                {
                    out.problems.push(super::Problem::of(
                        at,
                        text!(
                            "export.settings.versionGone",
                            export = export.name,
                            row = n + 1,
                            to = entry.to,
                            card = project.full(card)
                        ),
                    ));
                    continue;
                }
                out.by_card
                    .entry(card.id.clone())
                    .or_default()
                    .push(Placed {
                        export: at,
                        setting,
                        row: entry.row.clone(),
                    });
            }
        }
    }
    out
}
