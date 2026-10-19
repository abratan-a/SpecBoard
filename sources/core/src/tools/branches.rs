//! The branch tools (`MCPSurface.branches`, `branch_diff`, `MCPViews.branchesView`):
//! the branches listed, what one changed against main and what main did since, what is
//! wrong in a tree, its diagram and its history, and a branch made, renamed, let go or taken back to a commit.
//! A merge and its preview are the merge's.

use super::{Args, Seen, check, project_summary};
use crate::Core;
use crate::mcp::Output;
use crate::store::{Reader, Store};
use crate::write::{self, Who};
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

/// What `branch_merge` says when it merged into a locked main without asking (the project's settings).
pub(crate) fn ai_merged() -> String {
    text!("branches.aiMerged")
}

/// What a branch changed, made and removed, and what main did since it was made.
#[derive(Default, Debug)]
pub(crate) struct BranchDiff {
    pub changed: Vec<(String, Vec<String>)>,
    pub made: Vec<String>,
    pub removed: Vec<String>,
    pub sections_changed: Vec<String>,
    pub sections_made: Vec<String>,
    pub sections_removed: Vec<String>,
    pub main_changed: Vec<(String, Vec<String>)>,
    pub main_made: Vec<String>,
    pub main_removed: Vec<String>,
    pub overlaps: Vec<String>,
    pub elsewhere: Vec<(String, Vec<String>)>,
    /// The same, by id: what the screen marks (`branch_marks`).
    pub ids: DiffIds,
}

/// What a branch changed, made and removed, by id, as `BranchDiff` lists them by path; and the
/// cards main changed, made or removed since (`main_moved`).
#[derive(Default, Debug)]
pub(crate) struct DiffIds {
    pub changed: BTreeSet<String>,
    pub made: BTreeSet<String>,
    pub removed: BTreeSet<String>,
    pub sections_changed: BTreeSet<String>,
    pub sections_made: BTreeSet<String>,
    pub sections_removed: BTreeSet<String>,
    pub main_moved: BTreeSet<String>,
}

/// A tree read: its detail, typed.
fn typed(
    reader: &Reader,
    project: &str,
    tree: Option<&str>,
) -> Result<crate::model::Detail, String> {
    crate::model::typed(&crate::detail::read(reader, project, tree)?)
}

struct Own {
    id: String,
    removed: bool,
    doc: Option<Value>,
    base: Option<Value>,
}

fn own_rows(reader: &Reader, table: &str, project: &str, tree: &str) -> Result<Vec<Own>, String> {
    let mut stmt = reader
        .conn
        .prepare(&format!(
            "SELECT id, removed, doc, base FROM {table} WHERE project_id = ? AND branch_id = ? ORDER BY rowid"
        ))
        .map_err(crate::store::sql)?;
    stmt.query_map([project, tree], |r| {
        Ok(Own {
            id: r.get(0)?,
            removed: r.get::<_, i64>(1)? != 0,
            doc: write::parsed(r.get::<_, Option<String>>(2)?.as_deref()),
            base: write::parsed(r.get::<_, Option<String>>(3)?.as_deref()),
        })
    })
    .map_err(crate::store::sql)?
    .collect::<Result<Vec<_>, _>>()
    .map_err(crate::store::sql)
}

/// Where a document of a card says it is, with the names of `names`: its section's path
/// and its name — or its container's path and its name, inside one.
fn doc_path(doc: &Value, names: &crate::export::Project) -> String {
    let section = doc["section"].as_str().unwrap_or_default();
    let name = doc["name"].as_str().unwrap_or_default();
    if let Some(container) = doc["in"].as_str().and_then(|c| names.named(c)) {
        return crate::pathtext::member(&container, name);
    }
    crate::pathtext::card_path(&names.directory.full_section(section), name)
}

/// What the branch changed against main, and what main did since (`branchDiff`); with `whole`,
/// what it sees hidden and shares with the other branches too (`overlaps`, `elsewhere`), which
/// a list of the branches leaves out.
pub(crate) fn branch_diff(
    reader: &Reader,
    project: &str,
    branch: &str,
    whole: bool,
) -> Result<BranchDiff, String> {
    let main = typed(reader, project, None)?;
    let mine = typed(reader, project, Some(branch))?;
    let main_names = crate::export::Project::new(&main);
    let my_names = crate::export::Project::new(&mine);
    let cards = own_rows(reader, "card_docs", project, branch)?;
    let sections = own_rows(reader, "section_docs", project, branch)?;
    let bases: HashMap<&str, &Value> = cards
        .iter()
        .filter_map(|r| r.base.as_ref().map(|b| (r.id.as_str(), b)))
        .collect();
    // A card, or a card ghost by its own path.
    let path_mine = |id: &str| my_names.named(id);
    // Where a change names a card that is not there now (a line to a card removed): by the path it
    // had — in main, as the branch's own row of it had it, or as the branch saw it when it was
    // made — never by its id.
    let printed_paths: HashMap<String, String> = {
        let mut stmt = reader
            .conn
            .prepare("SELECT card_id, path FROM branch_prints WHERE branch_id = ? AND path <> ''")
            .map_err(crate::store::sql)?;
        stmt.query_map([branch], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(crate::store::sql)?
            .collect::<Result<_, _>>()
            .map_err(crate::store::sql)?
    };
    let removed_paths: HashMap<&str, String> = cards
        .iter()
        .filter(|r| r.removed)
        .filter_map(|r| {
            r.base
                .as_ref()
                .map(|b| (r.id.as_str(), doc_path(b, &main_names)))
        })
        .collect();
    let path_any = |id: &str| {
        my_names
            .named(id)
            .or_else(|| main_names.named(id))
            .or_else(|| removed_paths.get(id).cloned())
            .or_else(|| printed_paths.get(id).cloned())
    };
    // A section by its path: as the branch has it, or as main had it (one the branch removed).
    let section_any = |id: &str| {
        if !mine.sections.iter().any(|s| s.id == id) && main.sections.iter().any(|s| s.id == id) {
            main_names.directory.full_section(id)
        } else {
            my_names.directory.full_section(id)
        }
    };
    let mut out = BranchDiff::default();
    for row in &cards {
        if row.removed {
            let path = row
                .base
                .as_ref()
                .map(|b| doc_path(b, &main_names))
                .or_else(|| main_names.named(&row.id));
            if let Some(p) = path {
                out.removed.push(p);
                out.ids.removed.insert(row.id.clone());
            }
        } else if row.base.is_none() {
            if let Some(p) = path_mine(&row.id) {
                out.made.push(p);
                out.ids.made.insert(row.id.clone());
            }
        } else if let (Some(now), Some(then)) = (&row.doc, &row.base) {
            let said = crate::diff::changes(then, now, &path_any, Some(&section_any));
            if !said.is_empty()
                && let Some(p) = path_mine(&row.id)
            {
                out.changed.push((p, said));
                out.ids.changed.insert(row.id.clone());
            }
        }
    }
    for row in &sections {
        let id = row
            .doc
            .as_ref()
            .or(row.base.as_ref())
            .and_then(|d| d["id"].as_str())
            .unwrap_or(&row.id);
        let names = if row.removed { &main_names } else { &my_names };
        let path = names.directory.full_section(id);
        if row.removed {
            out.sections_removed.push(path);
            out.ids.sections_removed.insert(row.id.clone());
        } else if row.base.is_none() {
            out.sections_made.push(path);
            out.ids.sections_made.insert(row.id.clone());
        } else {
            out.sections_changed.push(path);
            out.ids.sections_changed.insert(row.id.clone());
        }
    }
    // What main did since: a card the branch wrote, against its base; any other, against
    // what the branch remembers of it.
    let mut stmt = reader
        .conn
        .prepare("SELECT card_id, print, path FROM branch_prints WHERE branch_id = ?")
        .map_err(crate::store::sql)?;
    let printed: Vec<(String, String, String)> = stmt
        .query_map([branch], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .map_err(crate::store::sql)?
        .collect::<Result<_, _>>()
        .map_err(crate::store::sql)?;
    let prints: HashMap<&str, &str> = printed
        .iter()
        .map(|p| (p.0.as_str(), p.1.as_str()))
        .collect();
    let printed_at: HashMap<&str, &str> = printed
        .iter()
        .map(|p| (p.0.as_str(), p.2.as_str()))
        .collect();
    let touched: HashSet<&str> = cards.iter().map(|r| r.id.as_str()).collect();
    let main_rows = own_rows(reader, "card_docs", project, "")?;
    let main_docs: HashMap<&str, (&Value, String)> = main_rows
        .iter()
        .filter(|r| !r.removed)
        .filter_map(|r| r.doc.as_ref().map(|d| (r.id.as_str(), (d, d.to_string()))))
        .collect();
    let mut stmt = reader
        .conn
        .prepare(
            "SELECT id, doc FROM card_docs WHERE project_id = ? AND branch_id = '' AND removed = 0",
        )
        .map_err(crate::store::sql)?;
    let main_texts: HashMap<String, String> = stmt
        .query_map([project], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?))
        })
        .map_err(crate::store::sql)?
        .filter_map(|r| r.ok())
        .filter_map(|(id, doc)| doc.map(|d| (id, d)))
        .collect();
    let path_main = |id: &str| path_any(id);
    // A section main moved a card from or to: by its path in main, or as the branch has it.
    let section_main = |id: &str| {
        if main.sections.iter().any(|s| s.id == id) {
            main_names.directory.full_section(id)
        } else {
            my_names.directory.full_section(id)
        }
    };
    // main's cards and card ghosts, each by its own path.
    let main_entries: Vec<(&str, String)> = main
        .schemas
        .iter()
        .chain(main.inner.iter())
        .map(|c| (c.id.as_str(), main_names.full(c)))
        .chain(
            main.ghosts
                .iter()
                .map(|g| (g.id.as_str(), main_names.full_ghost(g))),
        )
        .collect();
    for (id, full) in &main_entries {
        let full = full.clone();
        let Some((now, _)) = main_docs.get(id) else {
            continue;
        };
        if let Some(base) = bases.get(id) {
            if base == now {
                continue;
            }
            out.main_changed.push((
                full,
                crate::diff::changes(base, now, &path_main, Some(&section_main)),
            ));
        } else if touched.contains(id) {
            continue;
        } else if let Some(print) = prints.get(id) {
            let text = main_texts.get(*id).cloned().unwrap_or_default();
            if *print == crate::schema::print(&text) {
                continue;
            }
            out.main_changed.push((full, Vec::new()));
        } else {
            out.main_made.push(full);
        }
        out.ids.main_moved.insert(id.to_string());
    }
    let in_main: HashSet<&str> = main_entries.iter().map(|(id, _)| *id).collect();
    let mut gone: Vec<&str> = prints
        .keys()
        .chain(bases.keys())
        .copied()
        .filter(|id| !in_main.contains(id))
        .collect();
    gone.sort();
    gone.dedup();
    for id in gone {
        out.ids.main_moved.insert(id.to_string());
        if let Some(base) = bases.get(id) {
            out.main_removed.push(doc_path(base, &main_names));
        } else if let Some(at) = printed_at.get(id).filter(|a| !a.is_empty()) {
            out.main_removed.push(at.to_string());
        } else {
            out.main_removed.push(id.to_string());
        }
    }
    if !whole {
        return Ok(out);
    }
    let composed = reader.compose(project, Some(branch))?;
    for hidden in &composed.hidden {
        if let Some(path) = main_names.named(hidden) {
            out.overlaps.push(path);
        } else if main.sections.iter().any(|s| &s.id == hidden) {
            out.overlaps.push(main_names.directory.full_section(hidden));
        }
    }
    // Other branches at the same paths: names and paths only.
    let own_paths: HashSet<String> = cards
        .iter()
        .filter(|r| !r.removed)
        .filter_map(|r| path_mine(&r.id))
        .collect();
    let mut others: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (other, name) in super::branches(reader.conn, project)? {
        if other == branch {
            continue;
        }
        let theirs = typed(reader, project, Some(&other))?;
        let names = crate::export::Project::new(&theirs);
        let touched_there: HashSet<String> = own_rows(reader, "card_docs", project, &other)?
            .into_iter()
            .filter(|r| !r.removed)
            .map(|r| r.id)
            .collect();
        let entries = theirs
            .schemas
            .iter()
            .chain(theirs.inner.iter())
            .map(|c| (&c.id, names.full(c)))
            .chain(theirs.ghosts.iter().map(|g| (&g.id, names.full_ghost(g))));
        for (_, path) in entries.filter(|(id, _)| touched_there.contains(*id)) {
            if own_paths.contains(&path) {
                others.entry(path).or_default().push(name.clone());
            }
        }
    }
    out.elsewhere = others.into_iter().collect();
    Ok(out)
}

impl BranchDiff {
    /// How many of its own cards and sections it changed, made or removed.
    pub fn changes(&self) -> usize {
        self.changed.len()
            + self.made.len()
            + self.removed.len()
            + self.sections_changed.len()
            + self.sections_made.len()
            + self.sections_removed.len()
    }

    pub fn main_moved(&self) -> usize {
        self.main_changed.len() + self.main_made.len() + self.main_removed.len()
    }

    /// As `branch_read` answers it (`branch_diff`).
    pub fn view(&self, name: &str) -> Value {
        fn strings(list: &[String]) -> Value {
            Value::Array(list.iter().map(|s| Value::from(s.clone())).collect())
        }
        fn cards(list: &[(String, Vec<String>)]) -> Value {
            Value::Array(
                list.iter()
                    .map(|(p, said)| {
                        let mut row = Map::new();
                        row.insert("card".into(), p.clone().into());
                        if !said.is_empty() {
                            row.insert("changes".into(), strings(said));
                        }
                        Value::Object(row)
                    })
                    .collect(),
            )
        }
        let mut out = Map::new();
        out.insert("branch".into(), name.into());
        let lists: [(&str, Value, bool); 10] = [
            ("changed", cards(&self.changed), self.changed.is_empty()),
            ("made", strings(&self.made), self.made.is_empty()),
            ("removed", strings(&self.removed), self.removed.is_empty()),
            (
                "sectionsChanged",
                strings(&self.sections_changed),
                self.sections_changed.is_empty(),
            ),
            (
                "sectionsMade",
                strings(&self.sections_made),
                self.sections_made.is_empty(),
            ),
            (
                "sectionsRemoved",
                strings(&self.sections_removed),
                self.sections_removed.is_empty(),
            ),
            (
                "mainChanged",
                cards(&self.main_changed),
                self.main_changed.is_empty(),
            ),
            (
                "mainMade",
                strings(&self.main_made),
                self.main_made.is_empty(),
            ),
            (
                "mainRemoved",
                strings(&self.main_removed),
                self.main_removed.is_empty(),
            ),
            (
                "overlaps",
                strings(&self.overlaps),
                self.overlaps.is_empty(),
            ),
        ];
        for (key, value, empty) in lists {
            if !empty {
                out.insert(key.into(), value);
            }
        }
        if !self.elsewhere.is_empty() {
            out.insert(
                "elsewhere".into(),
                Value::Array(
                    self.elsewhere
                        .iter()
                        .map(|(p, b)| {
                            let mut row = Map::new();
                            row.insert("path".into(), p.clone().into());
                            row.insert("branches".into(), strings(b));
                            Value::Object(row)
                        })
                        .collect(),
                ),
            );
        }
        Value::Object(out)
    }
}

/// The paths `path` names: one, or an array of them (none left out).
fn paths_of(path: Option<&Value>) -> Result<Vec<String>, String> {
    match path {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(Value::String(p)) => Ok(vec![p.clone()]),
        Some(Value::Array(items)) if !items.is_empty() => items
            .iter()
            .map(|i| {
                i.as_str()
                    .map(str::to_string)
                    .ok_or_else(|| text!("branches.read.pathShape"))
            })
            .collect(),
        Some(_) => Err(text!("branches.read.pathShape")),
    }
}

/// When the branch was last merged into main, if nothing was written in it since: its last
/// commit is that merge (a merged branch would read as one that changed nothing).
fn merged_at(reader: &Reader, project: &str, branch: &str) -> Result<Option<String>, String> {
    reader
        .conn
        .query_row(
            "SELECT c.at FROM commits c WHERE c.project_id = ?1 AND c.branch_id = ?2 AND c.reverted = 0 \
             AND c.seq = (SELECT MAX(seq) FROM commits WHERE project_id = ?1 AND branch_id = ?2 AND reverted = 0) \
             AND c.kind = 'merge' AND EXISTS (SELECT 1 FROM commit_lines l WHERE l.commit_id = c.id \
             AND l.kind = 'branch.merge' AND l.target_id = ?2)",
            [project, branch],
            |r| r.get::<_, String>(0),
        )
        .map(Some)
        .or_else(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            other => Err(crate::store::sql(other)),
        })
}

/// What `branch_read`'s `as: verify` is told: the check of a tree is read item by item.
pub(crate) fn verify_gone() -> String {
    text!("branches.verifyGone")
}

/// Each branch, a row apiece: how much it changed, how much main did since it was made, and
/// when it was merged if nothing was written in it since (`branchesView`: what
/// it sees hidden and shares is `branch_read`'s, what is wrong in it `inspection_list`'s).
pub(crate) fn branches_view(reader: &Reader, project: &str) -> Result<Value, String> {
    let mut rows = Vec::new();
    for (id, name) in super::branches(reader.conn, project)? {
        let diff = branch_diff(reader, project, &id, false)?;
        let mut row = Map::new();
        row.insert("branch".into(), name.into());
        row.insert("changes".into(), diff.changes().into());
        if diff.changes() == 0
            && let Some(at) = merged_at(reader, project, &id)?
        {
            row.insert("merged".into(), at.into());
        }
        if diff.main_moved() > 0 {
            row.insert("mainMoved".into(), diff.main_moved().into());
        }
        rows.push(Value::Object(row));
    }
    Ok(Value::Array(rows))
}

/// `branch_create`, `branch_update` and `branch_delete`, written in a transaction already
/// open: a call's own, or a batch's.
pub(crate) fn branch_write(tx: &mut write::Tx, name: &str, args: &Args) -> Result<Value, String> {
    let mut out = Map::new();
    match name {
        "branch_create" => {
            let name = args.target()?;
            let given = args.value()?;
            if given.contains_key("copy") {
                return Err(text!("branches.copyGone.create"));
            }
            check(&given, "branch_create", "branch")?;
            let summary = super::project_summary_in(tx.conn(), args.string("project")?)?;
            tx.create_branch(&summary.id, &name)?;
            let made = tx
                .branch_named(&summary.id, &name)?
                .map(|b| b.1)
                .unwrap_or(name);
            out.insert("project".into(), summary.name.into());
            out.insert("branch".into(), made.into());
        }
        "branch_update" => {
            let summary = super::project_summary_in(tx.conn(), args.string("project")?)?;
            let (id, was) = branch_in(tx.conn(), &summary.id, &args.target()?)?;
            let given = args.value()?;
            if given.contains_key("copy") {
                return Err(text!("branches.copyGone.update"));
            }
            check(&given, "branch_update", "branch")?;
            let name = given
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let now = tx.rename_branch(&summary.id, &id, &name)?;
            out.insert("project".into(), summary.name.into());
            out.insert("branch".into(), now.into());
            out.insert("was".into(), was.into());
        }
        _ => {
            let summary = super::project_summary_in(tx.conn(), args.string("project")?)?;
            let (id, name) = branch_in(tx.conn(), &summary.id, &args.target()?)?;
            let given = args.value()?;
            check(&given, "branch_delete", "branch")?;
            let discard = given.get("discard").and_then(Value::as_bool) == Some(true);
            let changes = tx.branch_change_count(&id)?;
            if changes > 0 && !discard {
                return Err(text!(
                    "branches.delete.hasChanges",
                    name = name,
                    changes = changes
                ));
            }
            tx.delete_branch(&summary.id, &id)?;
            // Who still names it as the branch they work in: kept, and said.
            let board = super::cross::board(&tx.reader(), &summary.id)?;
            let naming = super::cross::naming_branch(&board, &name);
            if !naming.is_empty() {
                out.insert(
                    "warnings".into(),
                    vec![text!(
                        "branches.delete.stillNamed",
                        cards = crate::text::list(&naming),
                        name = name
                    )]
                    .into(),
                );
            }
            out.insert("project".into(), summary.name.into());
            out.insert("deleted".into(), name.into());
            if changes > 0 {
                out.insert("discarded".into(), changes.into());
            }
        }
    }
    Ok(Value::Object(out))
}

/// A branch of the project by its name (`refunds`, or `refunds@`), in any case.
fn branch(store: &Store, project: &str, raw: &str) -> Result<(String, String), String> {
    branch_in(&store.conn, project, raw)
}

fn branch_in(
    conn: &rusqlite::Connection,
    project: &str,
    raw: &str,
) -> Result<(String, String), String> {
    let raw = raw.trim();
    let bare = raw.strip_suffix('@').unwrap_or(raw);
    super::branches(conn, project)?
        .into_iter()
        .find(|(_, n)| crate::pathtext::same(n, bare))
        .ok_or_else(|| text!("branches.notBranch", raw = raw))
}

/// The tree an end names: a branch's id, or `""` for main.
fn tree_of(end: &Option<(String, String)>) -> String {
    end.as_ref().map(|b| b.0.clone()).unwrap_or_default()
}

/// One end of a merge or a history: a branch by its name, or none for `main`.
fn endpoint(store: &Store, project: &str, raw: &str) -> Result<Option<(String, String)>, String> {
    if crate::pathtext::same(raw.trim(), "main") {
        return Ok(None);
    }
    branch(store, project, raw).map(Some)
}

/// A history as MCP answers it: the first lines of each commit said, the rest counted.
pub(crate) fn commits_view(commits: &[write::Commit]) -> Value {
    const SHOWN: usize = 10;
    Value::Array(
        commits
            .iter()
            .map(|c| {
                let mut row = Map::new();
                row.insert("seq".into(), c.seq.into());
                row.insert("at".into(), c.at.clone().into());
                let author = crate::write::author_said(&c.author_kind, c.author.as_deref());
                row.insert("author".into(), author.into());
                if c.kind != "edit" {
                    row.insert("kind".into(), c.kind.clone().into());
                }
                if let Some(n) = &c.name {
                    row.insert("name".into(), n.clone().into());
                }
                if c.reverted {
                    row.insert("reverted".into(), true.into());
                }
                row.insert(
                    "lines".into(),
                    Value::Array(
                        c.lines
                            .iter()
                            .take(SHOWN)
                            .map(|l| Value::from(l.summary.clone()))
                            .collect(),
                    ),
                );
                if c.lines.len() > SHOWN {
                    row.insert("moreLines".into(), (c.lines.len() - SHOWN).into());
                }
                Value::Object(row)
            })
            .collect(),
    )
}

impl Core {
    /// The branch tools.
    pub(crate) fn branch_tool(
        &self,
        who: &Who,
        name: &str,
        args: &Args,
        granted: &[&'static str],
    ) -> Result<Output, String> {
        match name {
            // What only reads, read apart from the writes (`Core::reading`).
            "branch_list" => {
                let store = self.reading()?;
                let summary = project_summary(&store, args.string("project")?)?;
                let mut out = Map::new();
                out.insert("project".into(), summary.name.into());
                out.insert(
                    "branches".into(),
                    branches_view(&store.reader(), &summary.id)?,
                );
                Ok(Output::Json(Value::Object(out)))
            }
            "branch_read" => {
                let given = args.value()?;
                let target = args.target()?;
                // A branch of the app's own (`~…`) is the window's: to MCP it is not there.
                if who.side == write::Side::Mcp && target.trim().starts_with('~') {
                    let store = self.reading()?;
                    let summary = project_summary(&store, args.string("project")?)?;
                    return super::no_branch(&store.conn, &summary.id, target.trim());
                }
                let asked = match given.get("as") {
                    None => "diff",
                    Some(Value::String(a)) => a.as_str(),
                    Some(other) => {
                        return Err(text!("branches.read.asOneOf", other = other));
                    }
                };
                let store = self.reading()?;
                match asked {
                    "verify" => Err(verify_gone()),
                    "diagram" => {
                        let seen = Seen::read(&store, args.string("project")?, Some(&target))?;
                        let out = super::diagram::diagram(
                            &seen,
                            &given,
                            super::diagram::At::Project,
                            args.paged.then_some(super::paging::MOST_CHARS),
                        )?;
                        Ok(seen.headed(out))
                    }
                    "history" => {
                        // From after a commit, through MCP (`paging`): its seq.
                        let mut given = given;
                        let after_said = super::paging::after_of(given.shift_remove("after"))?;
                        check(&given, "branch_read", "branch（as: history）")?;
                        // One commit, what it changed: after the change `after` names (a path).
                        if let Some(seq) = given.get("seq") {
                            if given.contains_key("limit") {
                                return Err(text!("branches.read.seqWithLimit"));
                            }
                            let Some(seq) = seq.as_f64().filter(|f| f.fract() == 0.0) else {
                                return Err(text!("branches.read.seqShape"));
                            };
                            let asked = super::commit_view::Asked {
                                seq: seq as i64,
                                paths: paths_of(given.get("path"))?,
                                full: given.get("full").and_then(Value::as_bool) == Some(true),
                                after: after_said.filter(|_| args.paged),
                            };
                            let summary = project_summary(&store, args.string("project")?)?;
                            let named = super::named(&store, &summary.id, Some(&target))?;
                            let said = super::commit_view::commit_view(
                                &store,
                                &summary.id,
                                &named.tree,
                                &asked,
                                args.paged,
                            )?;
                            let mut out = Map::new();
                            if let Some(w) = said.get("warnings") {
                                out.insert("warnings".into(), w.clone());
                            }
                            out.insert("project".into(), summary.name.into());
                            out.insert("branch".into(), named.name.into());
                            out.extend(said.into_iter().filter(|(k, _)| k != "warnings"));
                            return Ok(Output::Json(Value::Object(out)));
                        }
                        if given.contains_key("path") || given.contains_key("full") {
                            return Err(text!("branches.read.withSeq"));
                        }
                        let after = match after_said {
                            Some(said) if args.paged => Some(
                                said.trim()
                                    .parse::<i64>()
                                    .map_err(|_| text!("branches.read.afterShape"))?,
                            ),
                            _ => None,
                        };
                        let limit = match given.get("limit") {
                            None => 50,
                            Some(Value::Number(n))
                                if n.as_f64().is_some_and(|f| {
                                    f.fract() == 0.0 && (1.0..=1000.0).contains(&f)
                                }) =>
                            {
                                n.as_f64().unwrap() as i64
                            }
                            Some(_) => return Err(text!("branches.read.limitRange")),
                        };
                        let summary = project_summary(&store, args.string("project")?)?;
                        let named = super::named(&store, &summary.id, Some(&target))?;
                        let mut out = Map::new();
                        out.insert("project".into(), summary.name.into());
                        out.insert("branch".into(), named.name.into());
                        if !args.paged {
                            let commits = write::history_of(
                                &store.conn,
                                &summary.id,
                                &named.tree,
                                limit + 1,
                            )?;
                            let shown: Vec<write::Commit> =
                                commits.iter().take(limit as usize).cloned().collect();
                            out.insert("commits".into(), commits_view(&shown));
                            if commits.len() as i64 > limit {
                                out.insert("more".into(), true.into());
                            }
                            return Ok(Output::Json(Value::Object(out)));
                        }
                        // Through MCP: the commits before `after`, up to `limit` and the characters an
                        // answer is given in (`paging`); cut, it says how many there are and the seq
                        // to go on from.
                        let commits = write::history_before(
                            &store.conn,
                            &summary.id,
                            &named.tree,
                            after,
                            limit + 1,
                        )?;
                        let rows = match commits_view(&commits) {
                            Value::Array(rows) => rows,
                            _ => Vec::new(),
                        };
                        let keys: Vec<String> = commits.iter().map(|c| c.seq.to_string()).collect();
                        let page = super::paging::Page {
                            limit: limit as usize,
                            after: None,
                        };
                        let cut = super::paging::window(
                            &keys,
                            &page,
                            super::paging::Order::Descending,
                            super::paging::MOST_CHARS,
                            |i| super::paging::chars(&rows[i]),
                        );
                        if let Some(next) = &cut.next {
                            out.insert("truncated".into(), true.into());
                            out.insert(
                                "matched".into(),
                                write::history_count(&store.conn, &summary.id, &named.tree)?.into(),
                            );
                            out.insert("returned".into(), (cut.end - cut.start).into());
                            out.insert("next".into(), next.clone().into());
                        }
                        out.insert(
                            "commits".into(),
                            Value::Array(rows[cut.start..cut.end].to_vec()),
                        );
                        Ok(Output::Json(Value::Object(out)))
                    }
                    "diff" => {
                        let mut given = given;
                        let after_said = super::paging::after_of(given.shift_remove("after"))?;
                        check(&given, "branch_read", "branch")?;
                        let summary = project_summary(&store, args.string("project")?)?;
                        let paths = paths_of(given.get("path"))?;
                        if paths.is_empty() && (given.contains_key("full") || after_said.is_some())
                        {
                            return Err(text!("branches.read.fullWithoutPath"));
                        }
                        if !paths.is_empty() && given.contains_key("against") {
                            return Err(text!("branches.read.pathWithAgainst"));
                        }
                        if let Some(against) = given.get("against").and_then(Value::as_str) {
                            let source = endpoint(&store, &summary.id, &target)?;
                            let into = endpoint(&store, &summary.id, against)?;
                            drop(store);
                            let into_name = into
                                .as_ref()
                                .map_or_else(|| "main".to_string(), |b| b.1.clone());
                            let mut store = self.lock();
                            // Worked out in a transaction that is let go: nothing is written.
                            let tx = store.write(who.clone())?;
                            let preview = tx.merge_preview(
                                &summary.id,
                                &tree_of(&source),
                                &tree_of(&into),
                                &write::MergeTake::new(),
                            )?;
                            let seen = super::merges::seen_in(
                                &tx,
                                &summary.id,
                                &summary.name,
                                &tree_of(&into),
                                &into_name,
                            )?;
                            return Ok(Output::Json(Value::Object(super::merges::preview_view(
                                &preview, &seen,
                            ))));
                        }
                        // main, the board and the working directories have no main to differ from.
                        let named = super::named(&store, &summary.id, Some(&target))?;
                        if super::branches(&store.conn, &summary.id)?
                            .iter()
                            .all(|(id, _)| *id != named.tree)
                        {
                            return Err(text!("branches.read.noDiff", name = named.name));
                        }
                        if !paths.is_empty() {
                            let asked = super::commit_view::Contents {
                                paths,
                                full: given.get("full").and_then(Value::as_bool) == Some(true),
                                after: after_said.filter(|_| args.paged),
                            };
                            let said = super::commit_view::branch_contents(
                                &store,
                                &summary.id,
                                &named.tree,
                                &asked,
                                args.paged,
                            )?;
                            let mut out = Map::new();
                            if let Some(w) = said.get("warnings") {
                                out.insert("warnings".into(), w.clone());
                            }
                            out.insert("project".into(), summary.name.into());
                            out.insert("branch".into(), named.name.into());
                            out.extend(said.into_iter().filter(|(k, _)| k != "warnings"));
                            return Ok(Output::Json(Value::Object(out)));
                        }
                        let diff = branch_diff(&store.reader(), &summary.id, &named.tree, true)?;
                        let mut said = diff.view(&named.name);
                        if diff.changes() == 0
                            && let Some(at) = merged_at(&store.reader(), &summary.id, &named.tree)?
                        {
                            said["merged"] = at.into();
                        }
                        Ok(Output::Json(said))
                    }
                    other => Err(text!("branches.read.asOneOf", other = other)),
                }
            }
            "branch_create" | "branch_update" | "branch_delete" => self
                .plain_writing(who.clone(), |tx| branch_write(tx, name, args))
                .map(Output::Json),
            "branch_merge" => {
                let given = args.value()?;
                check(&given, "branch_merge", "branch")?;
                let (summary, source, into) = {
                    let store = self.lock();
                    let summary = project_summary(&store, args.string("project")?)?;
                    let source = endpoint(&store, &summary.id, &args.target()?)?;
                    let into = endpoint(
                        &store,
                        &summary.id,
                        given.get("into").and_then(Value::as_str).unwrap_or("main"),
                    )?;
                    (summary, source, into)
                };
                let (from, to) = (tree_of(&source), tree_of(&into));
                if from == to {
                    return Err(text!("branches.merge.sameEnds"));
                }
                let into_name = into
                    .as_ref()
                    .map_or_else(|| "main".to_string(), |b| b.1.clone());
                let take = super::merges::take(given.get("take"))?;
                let places = super::merges::places(&given)?;
                // Merged into main, the branch is set aside unless kept.
                let keep = match given.get("keep") {
                    None => false,
                    Some(Value::Bool(b)) => *b,
                    Some(_) => return Err(text!("branches.merge.keepShape")),
                };
                if places && from.is_empty() {
                    return Err(write::merge_places_from_main());
                }
                let text = |key: &str| {
                    given
                        .get(key)
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string()
                };
                let (title, description) = (text("title"), text("description"));
                // Into a locked main the project lets an AI merge into without being approved: merged,
                // and the window told who did it.
                let mut unasked = false;
                let out = self.plain_writing(who.clone(), |tx| {
                    if !take.is_empty() {
                        let bare =
                            tx.merge_preview(&summary.id, &from, &to, &write::MergeTake::new())?;
                        super::merges::unmatched_takes(&bare, &take)?;
                    }
                    // Into a locked main, not merged: a merge request for a person to approve
                    // — unless the project lets an AI's merges in without (its settings, the
                    // window's only).
                    let locked = to.is_empty() && tx.locked_at(&summary.id)?.is_some();
                    unasked = locked && tx.ai_merge(&summary.id)?;
                    if locked && !unasked {
                        let Some(branch) = source.clone() else {
                            return Err(text!("branches.merge.sameEnds"));
                        };
                        let title = if title.trim().is_empty() {
                            branch.1.clone()
                        } else {
                            title.clone()
                        };
                        let made = super::requests::create(
                            tx,
                            &summary.id,
                            &summary.name,
                            &branch,
                            &title,
                            &description,
                            &take,
                            places,
                            keep,
                            who.name.as_deref(),
                        )?;
                        return super::requests::opened(tx, &summary, made);
                    }
                    let preview = tx.merge_preview(&summary.id, &from, &to, &take)?;
                    let before: HashSet<String> = {
                        let seen = super::merges::seen_in(
                            tx,
                            &summary.id,
                            &summary.name,
                            &to,
                            &into_name,
                        )?;
                        if !preview.conflicts.is_empty() || !preview.overlaps.is_empty() {
                            let shown = crate::ojson::compact(&Value::Object(
                                super::merges::preview_view(&preview, &seen),
                            ));
                            return Err(if preview.overlaps.is_empty() {
                                text!(
                                    "branches.merge.conflicts",
                                    count = preview.conflicts.len(),
                                    shown = shown
                                )
                            } else {
                                text!("branches.merge.overlaps", shown = shown)
                            });
                        }
                        super::merges::broken(&seen).into_iter().collect()
                    };
                    let done = tx.merge(&summary.id, &from, &to, &take, places, keep)?;
                    let seen =
                        super::merges::seen_in(tx, &summary.id, &summary.name, &to, &into_name)?;
                    let mut warnings: Vec<Value> = super::merges::broken(&seen)
                        .into_iter()
                        .filter(|w| !before.contains(w))
                        .map(Value::from)
                        .collect();
                    // Set aside: who works in it is told, as when it is let go of, with the call
                    // that finishes the goal — the merge does not write the board itself.
                    let aside = !from.is_empty() && to.is_empty() && !keep;
                    let mut finish = None;
                    if aside && !done.source.starts_with('~') {
                        let board = super::cross::board(&tx.reader(), &summary.id)?;
                        let naming = super::cross::naming_branch(&board, &done.source);
                        if let Some(first) = naming.first() {
                            warnings.push(
                                text!(
                                    "branches.merge.stillNamed",
                                    cards = crate::text::list(&naming),
                                    name = done.source
                                )
                                .into(),
                            );
                            finish = Some(json!({"tool": "card_action", "arguments": {
                                "branch": crate::store::BOARD_NAME,
                                "target": first,
                                "op": "finish",
                            }}));
                        }
                    }
                    let mut out = Map::new();
                    if aside && !done.source.starts_with('~') {
                        out.insert("setAside".into(), true.into());
                    }
                    if !warnings.is_empty() {
                        out.insert("warnings".into(), Value::Array(warnings));
                    }
                    if let Some(finish) = finish {
                        out.insert("use".into(), finish);
                    }
                    out.insert("project".into(), summary.name.clone().into());
                    out.insert("merged".into(), done.source.clone().into());
                    out.insert("into".into(), done.target.clone().into());
                    for (key, change) in [
                        ("added", write::MergeChange::Add),
                        ("changed", write::MergeChange::Change),
                        ("removed", write::MergeChange::Remove),
                    ] {
                        let paths: Vec<Value> = done
                            .items
                            .iter()
                            .filter(|i| i.change == change)
                            .map(|i| i.path.clone().into())
                            .collect();
                        if !paths.is_empty() {
                            out.insert(key.into(), Value::Array(paths));
                        }
                    }
                    if places {
                        out.insert("places".into(), done.places.into());
                    } else {
                        super::requests::say_places_left(
                            &mut out,
                            done.places,
                            text!("branches.merge.placesLeft"),
                        );
                    }
                    if unasked {
                        out.insert("said".into(), ai_merged().into());
                    }
                    Ok(Value::Object(out))
                })?;
                if unasked {
                    let count =
                        |key: &str| out.get(key).and_then(Value::as_array).map_or(0, Vec::len);
                    self.tell(&json!({
                        "op": "merged",
                        "project": summary.id,
                        "branch": out["merged"],
                        "by": who.name,
                        "added": count("added"),
                        "changed": count("changed"),
                        "removed": count("removed"),
                    }));
                }
                Ok(Output::Json(out))
            }
            "branch_revert" => {
                let given = args.value()?;
                check(&given, "branch_revert", "branch")?;
                // A commit's number: a whole number, 0 or more.
                let number = |key: &str, shape: &str| -> Result<Option<i64>, String> {
                    match given.get(key) {
                        None | Some(Value::Null) => Ok(None),
                        Some(v) => v
                            .as_f64()
                            .filter(|t| t.fract() == 0.0 && *t >= 0.0)
                            .map(|t| Some(t as i64))
                            .ok_or_else(|| crate::text::render(shape, &[])),
                    }
                };
                let to = number("to", "branches.revert.toShape")?;
                let undo = number("undo", "branches.revert.undoShape")?;
                let target = args.target()?;
                // A branch of the app's own (`~…`) is the window's: to MCP it is not there.
                if who.side == write::Side::Mcp && target.trim().starts_with('~') {
                    let store = self.reading()?;
                    let summary = project_summary(&store, args.string("project")?)?;
                    return super::no_branch(&store.conn, &summary.id, target.trim());
                }
                let (summary, named) = {
                    let store = self.lock();
                    let summary = project_summary(&store, args.string("project")?)?;
                    let named = super::named(&store, &summary.id, Some(&target))?;
                    (summary, named)
                };
                let tree = named.tree.clone();
                let author = write::Author::of(who);
                let name = given.get("name").and_then(Value::as_str);
                let (to, undo) = match (to, undo) {
                    (Some(to), None) => (to, None),
                    (None, Some(undo)) => (0, Some(undo)),
                    _ => return Err(text!("branches.revert.toOrUndo")),
                };
                if let Some(seq) = undo {
                    // Another writer's commit, let through by the project's item (`judge`): read the
                    // same way here, so what the item judged is what is undone.
                    let others = {
                        let store = self.reading()?;
                        super::mcp_writes::undoes_others(&store.conn, who, &summary.id, &tree, seq)?
                    };
                    let only_by = (!others).then_some(&author);
                    let asked = granted.contains(&"branch_revert.others");
                    return self
                        .plain_writing(who.clone(), |tx| {
                            undo_one(tx, &summary, &named, seq, name, only_by, asked)
                        })
                        .map(Output::Json);
                }
                // main: gone back through a branch that a merge takes in.
                if tree.is_empty() {
                    let (_, made, undone) = self.plain_writing(who.clone(), |tx| {
                        tx.revert_into_branch(&summary.id, to, name, Some(&author))
                    })?;
                    let mut out = Map::new();
                    out.insert("project".into(), summary.name.into());
                    out.insert("branch".into(), "main".into());
                    out.insert("made".into(), made.clone().into());
                    out.insert("reverted".into(), undone.into());
                    out.insert("to".into(), to.into());
                    out.insert(
                        "said".into(),
                        text!("branches.revert.said", seq = to, made = made).into(),
                    );
                    return Ok(Output::Json(Value::Object(out)));
                }
                let (undone, next) = self.plain_writing(who.clone(), |tx| {
                    let next = write::history_of(tx.conn(), &summary.id, &tree, 1)?
                        .first()
                        .map_or(0, |c| c.seq)
                        + 1;
                    let undone = tx.revert(&summary.id, &tree, to, Some(&author))?;
                    Ok((undone, next))
                })?;
                let mut out = Map::new();
                out.insert("project".into(), summary.name.into());
                out.insert("branch".into(), named.name.into());
                out.insert("reverted".into(), undone.into());
                out.insert("to".into(), to.into());
                out.insert("commit".into(), next.into());
                Ok(Output::Json(Value::Object(out)))
            }
            _ => Err(text!("tools.notYet", name = name)),
        }
    }
}

/// What undoing one commit alone came to (`undo_in`).
pub(crate) enum Undid {
    /// Undone: in a branch or the board by a commit of its own (`commit`); main by the branch `made`
    /// (its id and name). `changed`: the paths written.
    Done {
        undone: i64,
        commit: Option<i64>,
        made: Option<(String, String)>,
        changed: Vec<String>,
    },
    /// Refused, nothing written: each unit a later commit changed too, as `{path, unit, before,
    /// after, now}` in MCP's shape, and the later commits that wrote them.
    Conflicts {
        conflicts: Vec<Value>,
        later: Vec<i64>,
    },
}

/// One commit of the tree `tree` (`""` for main, `~board`, a branch's id; `tree_name` as it is called)
/// undone alone: in a branch or the board, a commit of its own; main through a branch made for it.
/// Where a later commit changed what it changed too, nothing is written and each such unit is said in
/// MCP's shape: as it was before the commit, right after it and now. One builder for what MCP
/// refuses with and what the window is answered, so a person and an AI see the same units.
#[allow(clippy::too_many_arguments)]
pub(crate) fn undo_in(
    tx: &mut write::Tx,
    project: &str,
    project_name: &str,
    tree: &str,
    tree_name: &str,
    seq: i64,
    name: Option<&str>,
    only_by: Option<&write::Author>,
    asked: bool,
) -> Result<Undid, String> {
    let next = write::history_of(tx.conn(), project, tree, 1)?
        .first()
        .map_or(0, |c| c.seq)
        + 1;
    let done = if tree.is_empty() {
        tx.undo_one_into_branch(project, seq, name, only_by, asked)?
    } else {
        tx.undo_one(project, tree, seq, only_by, asked)?
    };
    let names_before = tx.paths_before(project, tree, seq)?;
    let (changed, made) = match done {
        write::UndoOne::Done { changed, made } => (changed, made),
        write::UndoOne::Conflicts(conflicts) => {
            let seen = super::merges::seen_in(tx, project, project_name, tree, tree_name)?;
            let now = seen.project();
            let mut later: Vec<i64> = Vec::new();
            let said: Vec<Value> = conflicts
                .iter()
                .map(|c| {
                    later.extend(c.later.iter().copied());
                    let section = c.table == write::Table::Sections;
                    let said = |value: Option<&Value>| {
                        super::merges::said_change(section, c.kind.as_deref(), &c.unit, value, &now)
                    };
                    json!({
                        "path": names_before.get(&c.id).cloned().unwrap_or_else(|| c.id.clone()),
                        "unit": c.label,
                        "before": said(c.before.as_ref()),
                        "after": said(c.after.as_ref()),
                        "now": said(c.now.as_ref()),
                    })
                })
                .collect();
            later.sort_unstable();
            later.dedup();
            return Ok(Undid::Conflicts {
                conflicts: said,
                later,
            });
        }
    };
    let written = made.as_ref().map_or(tree, |(id, _)| id.as_str());
    let names_now = write::paths_over(&tx.reader(), project, written, &[])?;
    let mut paths: Vec<String> = changed
        .iter()
        .map(|(_, id)| {
            names_now
                .get(id)
                .or_else(|| names_before.get(id))
                .cloned()
                .unwrap_or_else(|| id.clone())
        })
        .collect();
    paths.sort();
    paths.dedup();
    Ok(Undid::Done {
        undone: seq,
        commit: made.is_none().then_some(next),
        made,
        changed: paths,
    })
}

/// MCP's answer for `undo_in`: refused where it conflicts, the project, branch and what was written
/// where it went through.
fn undo_one(
    tx: &mut write::Tx,
    summary: &super::ProjectSummary,
    named: &super::Named,
    seq: i64,
    name: Option<&str>,
    only_by: Option<&write::Author>,
    asked: bool,
) -> Result<Value, String> {
    let done = undo_in(
        tx,
        &summary.id,
        &summary.name,
        &named.tree,
        &named.name,
        seq,
        name,
        only_by,
        asked,
    )?;
    let (undone, commit, made, paths) = match done {
        Undid::Done {
            undone,
            commit,
            made,
            changed,
        } => (undone, commit, made, changed),
        Undid::Conflicts { conflicts, later } => {
            let count = conflicts.len();
            let shown = crate::ojson::compact(&json!({ "conflicts": conflicts, "later": later }));
            return Err(text!("branches.undo.conflicts", seq, count, shown));
        }
    };
    let mut out = Map::new();
    out.insert("project".into(), summary.name.clone().into());
    out.insert("branch".into(), named.name.clone().into());
    match made {
        Some((_, made)) => {
            out.insert("made".into(), made.clone().into());
            out.insert("undone".into(), undone.into());
            out.insert("changed".into(), paths.into());
            out.insert(
                "said".into(),
                text!("branches.revert.undoSaid", seq, made).into(),
            );
        }
        None => {
            out.insert("undone".into(), undone.into());
            out.insert("commit".into(), commit.unwrap_or(0).into());
            out.insert("changed".into(), paths.into());
        }
    }
    Ok(Value::Object(out))
}
