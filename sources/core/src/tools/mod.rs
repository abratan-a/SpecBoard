//! The MCP tools, run by the core: `project`・`section`・`card`・`tag`・`branch`・
//! `batch` and the export, each taking `project`, `target` and `value` as the Swift app's
//! surface does (`MCPSurface`, `MCPTools`, `MCPViews`) and answering as it answers —
//! except where the kinds' definitions changed the form on purpose (a card reads in
//! the form it is written in).
//!
//! The protocol around them — the list, `guide`, the arguments checked against each
//! tool's schema — is `crate::mcp`; this is what runs once a call has passed that.

mod actions;
mod app;
mod args;
pub(crate) mod asking;
mod batch;
mod board;
mod body_from;
mod boxes;
pub(crate) mod branches;
mod brought;
mod card_map;
mod cards;
mod cells;
mod check;
pub(crate) mod code;
pub(crate) mod code_review;
mod commit_view;
mod containers;
mod context;
pub(crate) mod coverage;
mod create;
pub(crate) mod cross;
mod diagram;
pub(crate) mod directories;
mod enum_values;
pub(crate) mod exports;
pub(crate) mod facts;
mod fields;
pub(crate) mod files;
pub(crate) mod follow;
pub(crate) mod frames;
mod ghosts;
pub(crate) mod grid;
mod impact;
mod in_part;
mod inner;
pub(crate) mod inspection;
pub(crate) mod instances;
mod kind_tools;
mod lines;
pub(crate) mod markdown_height;
pub(crate) mod mcp_writes;
mod merges;
pub(crate) mod mocks;
mod own_kinds;
pub(crate) mod paging;
pub(crate) mod picks;
pub(crate) mod picture;
mod project_tree;
pub(crate) mod projects;
mod reads;
pub(crate) mod requests;
pub(crate) mod review;
pub(crate) mod revs;
mod rules;
mod screen;
mod sections;
mod steps;
mod summary;
pub(crate) mod tables;
mod tags;
mod texts;
mod update;
mod views;
pub(crate) mod walk;
pub(crate) mod workdirs;
mod writes;

pub(crate) use args::Args;

use crate::Core;
use crate::mcp::Output;
use crate::store::{BOARD, BOARD_NAME, DIRECTORY_NAME, Store};
use crate::write::Who;
use rusqlite::OptionalExtension;
use serde_json::{Map, Value};
use std::rc::Rc;

/// A project as a call names it.
#[derive(Clone, Debug)]
pub(crate) struct ProjectSummary {
    pub id: String,
    pub name: String,
    pub slug: String,
    pub description: String,
    pub updated_at: String,
    pub archived_at: Option<String>,
    pub card_count: i64,
    pub section_count: i64,
}

/// The tree a call reads and writes in: main, the board, or a branch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Named {
    pub project: String,
    /// `""` for main, `~board` for the board, else the branch's id.
    pub tree: String,
    /// How it is called: `main`, `board`, or the branch's name.
    pub name: String,
}

/// A project as one tree of it reads: the project, the tree, and its detail.
pub(crate) struct Seen {
    pub summary: ProjectSummary,
    pub named: Named,
    pub detail: crate::model::Detail,
    /// The detail as `project_detail` answers it, for what the typed one leaves out
    /// (a node or a row as it is written).
    pub raw: Value,
    /// The project's exports (`export::settings`), in order.
    pub exports: Vec<crate::export::settings::Export>,
    /// main, as the board's lines reach it (`Project::outside`): read for the
    /// board alone.
    pub outside: Option<crate::model::Detail>,
    /// The working directories as they are read, as main's and a branch's lines and paths reach
    /// them (`Project::files`): put beside main and a branch alone, when a line or a path of the
    /// tree reaches into one, or the call reads or writes in one (`workdir::whole`).
    pub files: Option<Rc<crate::workdir::Overlay>>,
    /// The cards gone that its lines still reach, by id: the path each had (`Project::gone`).
    pub gone: std::collections::HashMap<String, String>,
    /// Every card's document as the tree sees it, by id (`Scope::doc`): what the detail was made
    /// from, and each card's rev is (`revs`).
    pub docs: std::collections::HashMap<String, Map<String, Value>>,
    /// What `project` looks things up by — of `detail`, `outside` and `files` —, made the first
    /// time it is asked for and kept while this stands (what is brought up changes none of it).
    looked: std::cell::OnceCell<Looked>,
    /// Every field's name by its id (`field_names`), made the first time it is asked for and kept
    /// as `looked` is.
    fields: std::cell::OnceCell<Rc<std::collections::HashMap<String, String>>>,
    /// Counted while it is held (`held_on_this_thread`).
    held: Held,
    /// What a write's reading was made of, to bring it up to what the write writes
    /// (`brought::Kept`); none in a read's.
    kept: Option<brought::Kept>,
    /// Every path declared that does not resolve (`update::broken`), made the first time it is
    /// asked for and let go of when it is brought up.
    broken: std::cell::OnceCell<Vec<(String, check::DeclaredPath)>>,
    /// Each card's part of `broken` (`update::card_broken`), by id: kept while what it says can
    /// be as it was (`brought`), so `broken` is not worked out for every card again.
    broken_by_card:
        std::cell::RefCell<std::collections::HashMap<String, Vec<(String, check::DeclaredPath)>>>,
    /// Which cards' documents say which words (`brought::Words`), made the first time asked for
    /// and kept as the documents are written.
    words: std::cell::RefCell<Option<brought::Words>>,
    /// Whether each card's paths are written as following them through no change writes them
    /// (`follow::follow`), by id: let go of when the card is written, or a change may make one of
    /// its paths read otherwise.
    written_as: std::cell::RefCell<std::collections::HashMap<String, bool>>,
}

/// A tree as it read before a change (`Seen::was`), to follow the change through (`follow::follow`).
pub(crate) struct Was {
    pub detail: crate::model::Detail,
    pub index: Option<Rc<crate::export::Index>>,
    pub fields: Option<Rc<std::collections::HashMap<String, String>>>,
}

/// What a project is looked up by (`export::Index`): of the tree, and of main beside the board
/// (the working directories' is their overlay's).
type Looked = (Rc<crate::export::Index>, Option<Rc<crate::export::Index>>);

/// The cards a tree's lines reach that are not there, by id: the path each had (its section's path
/// and its name, or its container's path and its name, as the tree names them now) — as another
/// tree has it (main's, for one a branch removed), as a branch's row kept it before it removed it,
/// or as the history kept it last; none for one whose history was let go of.
fn gone_of(
    reader: &crate::store::Reader,
    project: &str,
    detail: &crate::model::Detail,
) -> Result<std::collections::HashMap<String, String>, String> {
    let there: std::collections::HashSet<&str> = detail
        .schemas
        .iter()
        .chain(detail.inner.iter())
        .map(|c| c.id.as_str())
        .chain(detail.ghosts.iter().map(|g| g.id.as_str()))
        .collect();
    let mut missing: Vec<&str> = detail
        .schemas
        .iter()
        .chain(detail.inner.iter())
        .flat_map(|c| c.connections.iter())
        .filter_map(|l| l.to.as_deref())
        .filter(|to| !to.starts_with("file:") && !there.contains(to))
        .collect();
    missing.sort();
    missing.dedup();
    let mut out = std::collections::HashMap::new();
    if missing.is_empty() {
        return Ok(out);
    }
    let names = crate::export::Project::new(detail);
    let mut kept = reader
        .conn
        .prepare(
            "SELECT COALESCE(CASE WHEN removed = 0 THEN doc END, base) FROM card_docs \
             WHERE project_id = ? AND id = ? AND COALESCE(CASE WHEN removed = 0 THEN doc END, base) IS NOT NULL \
             ORDER BY removed, branch_id <> '' LIMIT 1",
        )
        .map_err(crate::store::sql)?;
    let mut history = reader
        .conn
        .prepare(
            "SELECT r.before FROM commit_rows r JOIN commits c ON c.id = r.commit_id \
             WHERE c.project_id = ? AND r.tbl = 'card_docs' AND r.row_id = ? AND r.before IS NOT NULL \
             ORDER BY c.at DESC, c.seq DESC LIMIT 1",
        )
        .map_err(crate::store::sql)?;
    for id in missing {
        let kept: Option<String> = kept
            .query_row([project, id], |r| r.get(0))
            .optional()
            .map_err(crate::store::sql)?;
        let text = match kept {
            Some(text) => Some(text),
            None => history
                .query_row([project, id], |r| r.get::<_, String>(0))
                .optional()
                .map_err(crate::store::sql)?
                .and_then(|before| crate::store::parse(Some(&before)))
                .and_then(|row| row.get("doc").and_then(Value::as_str).map(str::to_string)),
        };
        let Some(doc) = text.and_then(|t| serde_json::from_str::<Value>(&t).ok()) else {
            continue;
        };
        let name = doc["name"].as_str().unwrap_or_default();
        if name.is_empty() {
            continue;
        }
        let path = match doc["in"].as_str().and_then(|c| names.named(c)) {
            Some(container) => crate::pathtext::member(&container, name),
            None => match doc["section"].as_str() {
                Some(section) if detail.sections.iter().any(|s| s.id == section) => {
                    crate::pathtext::card_path(&names.directory.full_section(section), name)
                }
                _ => crate::pathtext::quote(name).into_owned(),
            },
        };
        out.insert(id.to_string(), path);
    }
    Ok(out)
}

/// The working directories as read (`workdir::Overlay`), when the project has any and the tree
/// reaches into them: a call that reads or writes in one (`workdir::whole`), a line to a file, or a
/// path written in a card that starts at one. A tree that does none of it is read without them,
/// however many files they hold. The board's lines reach them through main (`Seen::project`).
pub(crate) fn files_of(
    reader: &crate::store::Reader,
    project: &str,
    raw: &Value,
    whole: bool,
) -> Result<Option<Rc<crate::workdir::Overlay>>, String> {
    let mounts = crate::workdir::mounts(reader.conn, project)?;
    if mounts.is_empty()
        || !(whole || crate::workdir::files_wanted() || reaches_files(raw, &mounts))
    {
        return Ok(None);
    }
    crate::workdir::overlay(reader.conn, project)
}

/// Whether a tree's reading (`project_detail`'s JSON) reaches into a working directory: a line to
/// a file (`file:` ids), or a path written in `[[…]]` whose head is a working directory's section.
pub(crate) fn reaches_files(raw: &Value, mounts: &[crate::workdir::Mount]) -> bool {
    fn said(value: &Value, heads: &[String]) -> bool {
        match value {
            Value::String(s) => {
                s.starts_with("file:")
                    || s.match_indices("[[").any(|(at, _)| {
                        let rest = &s[at + 2..];
                        let rest = &rest[..rest.find("]]").unwrap_or(rest.len())];
                        let head = crate::pathtext::first_name(rest);
                        heads.contains(&crate::pathtext::fold(&head))
                    })
            }
            Value::Array(items) => items.iter().any(|v| said(v, heads)),
            Value::Object(o) => o.values().any(|v| said(v, heads)),
            _ => false,
        }
    }
    let heads: Vec<String> = mounts
        .iter()
        .map(|m| crate::pathtext::fold(&m.name))
        .collect();
    ["schemas", "inner", "relations"]
        .iter()
        .any(|key| said(&raw[*key], &heads))
}

thread_local! {
    /// How many times a project was worked out whole in a write on this thread (`Seen::in_tx`
    /// asked, and the documents had changed since the last): what the tests hold the cost of a
    /// write to.
    static REBUILDS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// The projects worked out whole in writes on this thread so far (see `REBUILDS`).
pub(crate) fn rebuilds_on_this_thread() -> u64 {
    REBUILDS.with(std::cell::Cell::get)
}

thread_local! {
    /// How many times a write on this thread brought what it had worked out up to cards made, let
    /// go of, renamed or put elsewhere since (`Seen::brought_up_whole`): each goes over every card
    /// of the project, so a call that makes a card at a time and asks after each pays the
    /// project's size for each.
    static BROUGHT_WHOLE: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// The bringings up over the whole project in writes on this thread so far (see `BROUGHT_WHOLE`).
pub(crate) fn brought_whole_on_this_thread() -> u64 {
    BROUGHT_WHOLE.with(std::cell::Cell::get)
}

/// One bringing up over the whole project more (`Seen::brought_up_whole`).
fn count_brought_whole() {
    BROUGHT_WHOLE.with(|n| n.set(n.get() + 1));
}

thread_local! {
    /// How many projects worked out whole (`Seen`) are held on this thread now: what the tests hold
    /// a write to letting go of what it kept once it is done.
    static HELD: std::cell::Cell<i64> = const { std::cell::Cell::new(0) };
}

/// The projects worked out whole held on this thread now (see `HELD`).
pub(crate) fn held_on_this_thread() -> i64 {
    HELD.with(std::cell::Cell::get)
}

/// One project worked out whole, held (`HELD`): counted while it is.
pub(crate) struct Held(());

impl Held {
    fn new() -> Held {
        HELD.with(|n| n.set(n.get() + 1));
        Held(())
    }
}

impl Drop for Held {
    fn drop(&mut self) {
        HELD.with(|n| n.set(n.get() - 1));
    }
}

/// main's detail, when `tree` is the board's: what its lines may reach.
fn outside_of(
    reader: &crate::store::Reader,
    project: &str,
    tree: Option<&str>,
) -> Result<Option<crate::model::Detail>, String> {
    if tree != Some(crate::store::BOARD) {
        return Ok(None);
    }
    Ok(Some(crate::model::typed(&crate::detail::read(
        reader, project, None,
    )?)?))
}

impl Seen {
    /// The project `project` names, as the tree `branch` names reads it.
    pub fn read(store: &Store, project: &str, branch: Option<&str>) -> Result<Seen, String> {
        let summary = project_summary(store, project)?;
        let named = named(store, &summary.id, branch)?;
        let tree = (!named.tree.is_empty()).then_some(named.tree.as_str());
        let whole = crate::workdir::whole();
        let crate::detail::Kept { raw, made, .. } =
            crate::detail::read_kept_as(&store.reader(), &summary.id, tree, whole)?;
        let detail = crate::model::typed(&raw)?;
        let exports = store.exports(&summary.id)?;
        let outside = outside_of(&store.reader(), &summary.id, tree)?;
        let files = files_of(&store.reader(), &summary.id, &raw, whole)?;
        let gone = gone_of(&store.reader(), &summary.id, &detail)?;
        Ok(Seen {
            summary,
            named,
            detail,
            raw,
            exports,
            outside,
            files,
            gone,
            // What each card's rev is made from (`revs`).
            docs: made.cards.into_iter().map(|i| (i.id, i.doc)).collect(),
            looked: std::cell::OnceCell::new(),
            fields: std::cell::OnceCell::new(),
            held: Held::new(),
            kept: None,
            broken: std::cell::OnceCell::new(),
            broken_by_card: Default::default(),
            words: Default::default(),
            written_as: Default::default(),
        })
    }

    /// The tree a write is in, as its transaction sees it now. The project is worked out whole
    /// each time it has changed (`Tx::remembered_or_brought_up`): asked again with nothing written
    /// between, it is the one made before — a write asks for it a dozen times, and a batch does for
    /// each step. What was written since
    /// that is said in a card's own entry alone (a label, a description, a body:
    /// `detail::bring_card`) is brought into the one made before instead, so a call that writes the
    /// labels of 200 cards works the project out once, not 200 times.
    pub fn in_tx(tx: &crate::write::Tx, scope: &cards::Scope) -> Result<Rc<Seen>, String> {
        let key = format!(
            "seen\u{1}{}\u{1}{}\u{1}{}\u{1}{}\u{1}{}\u{1}{}",
            scope.project,
            scope.project_name,
            scope.tree,
            scope.tree_name,
            crate::workdir::whole(),
            crate::workdir::files_wanted()
        );
        tx.remembered_or_brought_up(
            &key,
            || {
                REBUILDS.with(|n| n.set(n.get() + 1));
                Seen::made_in_tx(tx, scope)
            },
            |seen, written| {
                if !seen.brought_up(written, tx) {
                    return false;
                }
                // Built for the tests (debug), it is worked out again as well and held to be the
                // same: every test that writes holds what is brought up to what a reading makes.
                #[cfg(debug_assertions)]
                seen.same_as_made(tx, scope);
                true
            },
        )
    }

    /// What was brought up, held to what working it out again makes (debug builds: the tests).
    #[cfg(debug_assertions)]
    fn same_as_made(&self, tx: &crate::write::Tx, scope: &cards::Scope) {
        let made = Seen::made_in_tx(tx, scope).expect("the project could not be rebuilt");
        assert_eq!(
            self.raw.to_string(),
            made.raw.to_string(),
            "the brought-up state differs from the rebuilt one"
        );
        assert!(
            self.docs == made.docs,
            "the brought-up documents differ from the rebuilt ones"
        );
        let typed = |d: &crate::model::Detail| {
            let mut d = d.clone();
            for c in d.schemas.iter_mut().chain(d.inner.iter_mut()) {
                std::rc::Rc::make_mut(c).item_places.clear();
            }
            format!("{d:?}")
        };
        assert_eq!(typed(&self.detail), typed(&made.detail));
        assert_eq!(self.gone, made.gone);
        assert_eq!(
            format!("{:?}", self.broken()),
            format!("{:?}", update::broken(&made.project())),
            "the brought-up unresolved paths differ from the rebuilt ones"
        );
        assert_eq!(format!("{:?}", self.exports), format!("{:?}", made.exports));
        if let (Some(kept), Some(again)) = (&self.kept, &made.kept) {
            assert!(
                kept.paths == again.paths,
                "the brought-up paths differ from the rebuilt ones"
            );
            if let Some(refs) = &kept.refs {
                assert!(
                    *refs == brought::Refs::of(&made.docs),
                    "the brought-up card references differ from the rebuilt ones"
                );
            }
            if let Some(order) = &kept.order {
                assert!(
                    Some(order) == again.order.as_ref(),
                    "the brought-up card order differs from the rebuilt one"
                );
            }
        }
        if let Some(words) = self.words.borrow().as_ref() {
            assert!(
                *words == brought::Words::of(&made.docs),
                "the brought-up card words differ from the rebuilt ones"
            );
        }
    }

    fn made_in_tx(tx: &crate::write::Tx, scope: &cards::Scope) -> Result<Seen, String> {
        let reader = tx.reader();
        let branch = (!scope.tree.is_empty()).then_some(scope.tree.as_str());
        let whole = crate::workdir::whole();
        let crate::detail::Kept {
            raw,
            mut made,
            project,
            named: paths,
        } = crate::detail::read_kept_as(&reader, &scope.project, branch, whole)?;
        let detail = crate::model::typed(&raw)?;
        let order: Vec<String>;
        let exports = reader.exports(&scope.project)?;
        let outside = outside_of(&reader, &scope.project, branch)?;
        let files = files_of(&reader, &scope.project, &raw, whole)?;
        let gone = gone_of(&reader, &scope.project, &detail)?;
        Ok(Seen {
            summary: ProjectSummary {
                id: scope.project.clone(),
                name: scope.project_name.clone(),
                slug: String::new(),
                description: String::new(),
                updated_at: String::new(),
                archived_at: None,
                card_count: 0,
                section_count: 0,
            },
            named: Named {
                project: scope.project.clone(),
                tree: scope.tree.clone(),
                name: scope.tree_name.clone(),
            },
            detail,
            raw,
            exports,
            outside,
            files,
            gone,
            docs: {
                let cards = std::mem::take(&mut made.cards);
                order = cards.iter().map(|i| i.id.clone()).collect();
                cards.into_iter().map(|i| (i.id, i.doc)).collect()
            },
            looked: std::cell::OnceCell::new(),
            fields: std::cell::OnceCell::new(),
            held: Held::new(),
            kept: Some(brought::Kept {
                made,
                row: project,
                paths,
                refs: None,
                order: Some(order),
            }),
            broken: std::cell::OnceCell::new(),
            broken_by_card: Default::default(),
            words: Default::default(),
            written_as: Default::default(),
        })
    }

    /// The tree this reads, as a write names it.
    pub fn scope(&self) -> cards::Scope {
        cards::Scope {
            project: self.summary.id.clone(),
            project_name: self.summary.name.clone(),
            tree: self.named.tree.clone(),
            tree_name: self.named.name.clone(),
        }
    }

    /// Every path the project declares that does not resolve, by the card that holds it and its
    /// place (`update::broken`): worked out once while this stands — a write asks before and after
    /// it writes, and the next one's before is the last one's after.
    pub fn broken(&self) -> &[(String, check::DeclaredPath)] {
        self.broken.get_or_init(|| {
            let project = self.project();
            let mut by_card = self.broken_by_card.borrow_mut();
            let mut out = Vec::new();
            for card in self.detail.schemas.iter().chain(self.detail.inner.iter()) {
                let part = by_card
                    .entry(card.id.clone())
                    .or_insert_with(|| update::card_broken(&project, card));
                out.extend(part.iter().cloned());
            }
            out.extend(update::ghosts_broken(&project));
            out
        })
    }

    /// `broken`, by its keys.
    pub fn broken_keys(&self) -> std::collections::HashSet<String> {
        self.broken().iter().map(|(k, _)| k.clone()).collect()
    }

    /// Every card of the tree by id, in the order the store composes them (`Reader::compose`):
    /// the order a change followed is written in (`follow::follow`).
    pub fn composed_order(&self, tx: &crate::write::Tx) -> Result<Vec<String>, String> {
        if let Some(kept) = &self.kept
            && let Some(order) = &kept.order
        {
            return Ok(order.clone());
        }
        let branch = (!self.named.tree.is_empty()).then_some(self.named.tree.as_str());
        Ok(tx
            .reader()
            .compose(&self.summary.id, branch)?
            .cards
            .into_iter()
            .map(|i| i.id)
            .collect())
    }

    /// The cards whose documents say any of `names` (in lower case: `brought::Words`); none when
    /// one is not a word (then any card may).
    pub(crate) fn saying(&self, names: &[String]) -> Option<std::collections::HashSet<String>> {
        if names.is_empty() {
            return Some(Default::default());
        }
        let mut words = self.words.borrow_mut();
        words
            .get_or_insert_with(|| brought::Words::of(&self.docs))
            .saying(names)
    }

    /// Whether a card's paths are written as following them through no change writes them, as
    /// `check` says the first time it is asked (`follow::follow`).
    pub(crate) fn written_as(&self, id: &str, check: impl FnOnce() -> bool) -> bool {
        if let Some(said) = self.written_as.borrow().get(id) {
            return *said;
        }
        let said = check();
        self.written_as.borrow_mut().insert(id.to_string(), said);
        said
    }

    /// What `written_as` said of these cards, let go of.
    pub(crate) fn forget_written_as(&self, ids: &[String]) {
        let mut written_as = self.written_as.borrow_mut();
        for id in ids {
            written_as.remove(id);
        }
    }

    /// The tree's sections as the store composes them, when this was worked out in a write.
    pub fn kept_sections(&self) -> &[crate::store::Item] {
        self.kept
            .as_ref()
            .map(|k| k.made.sections.as_slice())
            .unwrap_or(&[])
    }

    /// Every field of the project, by id (`follow::field_names`).
    pub fn field_names(&self) -> &std::collections::HashMap<String, String> {
        self.fields
            .get_or_init(|| Rc::new(follow::field_names(&self.project())))
    }

    /// The tree as it reads now, kept to follow a change through (`follow::follow`): its typed
    /// reading (its cards shared, not copied), and what it is looked up by and every field's name,
    /// when they were made.
    pub fn was(&self) -> Was {
        Was {
            detail: self.detail.clone(),
            index: self.looked.get().map(|(here, _)| here.clone()),
            fields: self.fields.get().cloned(),
        }
    }

    pub fn project(&self) -> crate::export::Project<'_> {
        use crate::export::{Index, Project};
        let (here, outside) = self.looked.get_or_init(|| {
            (
                Rc::new(Index::of(&self.detail)),
                self.outside.as_ref().map(|d| Rc::new(Index::of(d))),
            )
        });
        let files = || {
            self.files.as_ref().map(|o| {
                let (d, i) = o.typed();
                Project::indexed(d, i.clone()).with_code(Some(o.code.clone()))
            })
        };
        let code = || self.files.as_ref().map(|o| o.code.clone());
        // The board reaches the working directories' files as main does: beside main.
        let board = self.named.tree == crate::store::BOARD;
        Project::indexed(&self.detail, here.clone())
            .with_outside(
                self.outside
                    .as_ref()
                    .zip(outside.clone())
                    .map(|(d, i)| Project::indexed(d, i).with_files(files()).with_code(code())),
            )
            .with_files(if board { None } else { files() })
            .with_code(if board { None } else { code() })
            .with_gone(self.gone.clone())
    }

    /// A card as `project_detail` answers it (raw JSON), by id.
    pub fn raw_card(&self, id: &str) -> Option<&Map<String, Value>> {
        self.raw["schemas"]
            .as_array()?
            .iter()
            .filter_map(Value::as_object)
            .find(|c| c.get("id").and_then(Value::as_str) == Some(id))
    }

    /// An answer headed with the tree it is of, once: after `project`, behind `warnings`
    /// (`headed`).
    pub fn headed(&self, output: Output) -> Output {
        headed_as(output, &self.named.name)
    }
}

/// An answer headed with the tree `name` (`headed`).
pub(crate) fn headed_as(output: Output, name: &str) -> Output {
    match output {
        Output::Json(Value::Object(body)) => {
            let mut out = Map::new();
            if let Some(w) = body.get("warnings") {
                out.insert("warnings".into(), w.clone());
            }
            if let Some(p) = body.get("project") {
                out.insert("project".into(), p.clone());
            }
            out.insert("branch".into(), name.into());
            for (k, v) in body {
                if !["warnings", "project", "branch"].contains(&k.as_str()) {
                    out.insert(k, v);
                }
            }
            Output::Json(Value::Object(out))
        }
        other => other,
    }
}

pub(crate) use actions::described as described_actions;
pub(crate) use args::action_gone;

/// The value against one of the tool's forms (its `label`), every complaint at once
/// (`Surface.check`).
pub(crate) fn check(value: &Map<String, Value>, tool: &str, label: &str) -> Result<(), String> {
    args::check(value, args::form(tool, label), tool, label)
}

/// One of several answers under one head: the project and the tree are said once, above them, not again in each.
pub(crate) fn under_head(mut answer: Value) -> Value {
    if let Value::Object(o) = &mut answer {
        o.shift_remove("project");
        o.shift_remove("branch");
    }
    answer
}

/// `warnings` put first, where it cannot be scrolled past, with any the answer had
/// (`Surface.warned`).
pub(crate) fn warned(value: Value, lines: &[String]) -> Value {
    let Value::Object(body) = value else {
        return value;
    };
    if lines.is_empty() {
        return Value::Object(body);
    }
    let mut had: Vec<Value> = body
        .get("warnings")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    had.extend(lines.iter().map(|l| Value::from(l.clone())));
    let mut out = Map::new();
    out.insert("warnings".into(), Value::Array(had));
    for (k, v) in body {
        if k != "warnings" {
            out.insert(k, v);
        }
    }
    Value::Object(out)
}

/// A section by its path, as a call names it (`MCPTools.section`).
pub(crate) fn section_id(project: &crate::export::Project, name: &str) -> Result<String, String> {
    let trimmed = name.trim_matches([' ', '\t']);
    if let Some(id) = project.directory.section_named(trimmed, None) {
        return Ok(id);
    }
    if project.detail.sections.is_empty() {
        if crate::pathtext::same(name.trim(), "main") {
            return Ok(project.directory.home.clone());
        }
        return Err(text!("tools.section.gone", name = name));
    }
    Err(project
        .directory
        .section_problem(name, &project.directory.home))
}

/// The tools whose `branch` names the tree they read and write in (`branched`): `export_execute` reads
/// it and marks its history.
pub(crate) const BRANCHED: [&str; 19] = [
    "export_read",
    "export_execute",
    "section_list",
    "section_read",
    "section_create",
    "section_update",
    "section_delete",
    "card_list",
    "card_read",
    "card_create",
    "card_update",
    "card_delete",
    "tag_list",
    "tag_update",
    "inspection_list",
    "inspection_read",
    "inspection_update",
    "card_action",
    "section_action",
];

/// The tools that write, run here: in a tree, or the project's own settings.
const WRITES: [&str; 16] = [
    "card_create",
    "card_update",
    "card_delete",
    "section_create",
    "section_update",
    "section_delete",
    "tag_update",
    "project_create",
    "project_update",
    "export_create",
    "export_update",
    "export_delete",
    "kind_create",
    "kind_update",
    "kind_delete",
    "card_action",
];

/// A path does not name its branch: a target written in a branch's tree
/// (`refunds@Store:Order`) is refused with the argument that names it (`treesNamed`).
fn trees_named(store: &Store, args: &Args) -> Result<(), String> {
    let mut written: Vec<String> = args.targets().ok().flatten().unwrap_or_default();
    if let Some(Value::Object(given)) = args.raw.get("value") {
        if !args.raw.contains_key("target") {
            written.extend(given.keys().cloned());
        }
        for key in ["section", "parent", "to"] {
            if let Some(text) = given.get(key).and_then(Value::as_str) {
                written.push(text.to_string());
            }
        }
    }
    let mut trees: Vec<String> = written
        .iter()
        .filter_map(|w| {
            crate::pathtext::tree_prefix(w)
                .map(|(t, _)| crate::pathtext::fold(&crate::pathtext::unquote(t)))
        })
        .filter(|t| t != "main")
        .collect();
    trees.sort();
    trees.dedup();
    let Some(project) = args.optional_string("project") else {
        return Ok(());
    };
    if trees.is_empty() {
        return Ok(());
    }
    let Ok(summary) = project_summary(store, project) else {
        return Ok(());
    };
    let all = branches(&store.conn, &summary.id)?;
    for tree in trees {
        if let Some((_, found)) = all.iter().find(|(_, n)| crate::pathtext::fold(n) == tree) {
            return Err(text!("tools.branchInPath", found = found));
        }
    }
    Ok(())
}

/// How a diagram's first line begins.
pub(crate) const DIAGRAM_MARK: &str = "%% specboard ";

/// At most a dozen, then a count (`cappedNames`).
pub(crate) fn capped(names: &[String], limit: usize) -> String {
    if names.len() <= limit {
        return names.join(" / ");
    }
    text!(
        "tools.capped",
        names = names[..limit].join(" / "),
        count = names.len() - limit
    )
}

/// Changes said one after another, joined by " / " in every language.
pub(crate) fn slashed(items: Vec<crate::text::Keyed>) -> crate::text::Keyed {
    let mut items = items.into_iter();
    let first = items.next().expect("at least one item to join");
    items.fold(first, |head, tail| {
        keyed!("tools.line.slashed", head = head, tail = tail)
    })
}

/// Every project, as a connection (a transaction's, say) sees them: by name.
pub(crate) fn projects_in(conn: &rusqlite::Connection) -> Result<Vec<ProjectSummary>, String> {
    let count = |query: &str| -> Result<std::collections::HashMap<String, i64>, String> {
        let mut stmt = conn.prepare(query).map_err(crate::store::sql)?;
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))
            .map_err(crate::store::sql)?
            .collect::<Result<_, _>>()
            .map_err(crate::store::sql)?;
        Ok(rows)
    };
    // A project's cards as a section counts them: not its card ghosts, nor a flow's nodes
    // (cards inside the flow, counted as the flow was before).
    let cards = count(
        "SELECT project_id, COUNT(*) FROM card_docs WHERE branch_id = '' AND removed = 0 AND kind NOT IN ('ghost', 'begin', 'process', 'run', 'eval', 'wait', 'join', 'end') GROUP BY project_id",
    )?;
    let sections = count(
        "SELECT project_id, COUNT(*) FROM section_docs WHERE branch_id = '' AND removed = 0 GROUP BY project_id",
    )?;
    let mut stmt = conn
        .prepare("SELECT id, name, slug, description, updated_at, archived_at FROM projects ORDER BY name ASC")
        .map_err(crate::store::sql)?;
    let out = stmt
        .query_map([], |r| {
            Ok(ProjectSummary {
                id: r.get(0)?,
                name: r.get(1)?,
                slug: r.get(2)?,
                description: r.get(3)?,
                updated_at: r.get(4)?,
                archived_at: r.get(5)?,
                card_count: 0,
                section_count: 0,
            })
        })
        .map_err(crate::store::sql)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(crate::store::sql)?;
    Ok(out
        .into_iter()
        .map(|mut p| {
            p.card_count = cards.get(&p.id).copied().unwrap_or(0);
            p.section_count = sections.get(&p.id).copied().unwrap_or(1);
            p
        })
        .collect())
}

/// The projects the call's access key may see; every project for the window
/// (`MCPTools.visibleProjects`).
pub(crate) fn visible_projects_in(
    conn: &rusqlite::Connection,
) -> Result<Vec<ProjectSummary>, String> {
    crate::access::visible(conn, projects_in(conn)?)
}

/// The project a call names: by id, by name, by slug, then by name in any case.
pub(crate) fn project_summary(store: &Store, name: &str) -> Result<ProjectSummary, String> {
    project_summary_in(&store.conn, name)
}

/// The project a call names, as a connection sees the projects. Only among the projects the call's
/// access key may see, so one it may not is missing in the same words as one that is not
/// there; and a call that writes is refused a project the key may only read
/// (`MCPTools.projectSummary`, `specboard://SpecBoard/Docs/Design:DesignAccess` §3).
pub(crate) fn project_summary_in(
    conn: &rusqlite::Connection,
    name: &str,
) -> Result<ProjectSummary, String> {
    let projects = visible_projects_in(conn)?;
    // By a name two have (made before names were held apart): neither, said with their slugs and
    // ids — even when it is one's slug too, for a call by the name means either. The one
    // whose slug is the name is called by its id.
    let two = |many: &[&ProjectSummary]| {
        text!(
            "tools.project.twice",
            name = name,
            count = many.len(),
            projects = many
                .iter()
                .map(|p| text!("tools.project.slugAndId", slug = p.slug, id = p.id))
                .collect::<Vec<_>>()
                .join(text!("tools.project.twiceSeparator").as_str())
        )
    };
    let named: Vec<&ProjectSummary> = projects.iter().filter(|p| p.name == name).collect();
    let in_any_case: Vec<&ProjectSummary> = projects
        .iter()
        .filter(|p| crate::pathtext::same(&p.name, name))
        .collect();
    let found = if let Some(p) = projects.iter().find(|p| p.id == name) {
        Some(p)
    } else if named.len() > 1 {
        return Err(two(&named));
    } else if let [p] = named.as_slice() {
        Some(*p)
    } else if let Some(p) = projects.iter().find(|p| p.slug == name) {
        Some(p)
    } else if in_any_case.len() > 1 {
        return Err(two(&in_any_case));
    } else {
        in_any_case.first().copied()
    };
    match found {
        Some(p) => {
            crate::access::admit(conn, &p.id, &p.name)?;
            Ok(p.clone())
        }
        None if projects.is_empty() => Err(text!("tools.project.noneVisible", name = name)),
        None => Err(text!(
            "tools.project.gone",
            name = name,
            listed = capped(
                &projects.iter().map(|p| p.name.clone()).collect::<Vec<_>>(),
                12
            )
        )),
    }
}

/// The branches of a project, in their order: (id, name).
pub(crate) fn branches(
    conn: &rusqlite::Connection,
    project: &str,
) -> Result<Vec<(String, String)>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, name FROM branches WHERE project_id = ? AND name NOT LIKE '~%' ORDER BY sort_order, created_at, name",
        )
        .map_err(crate::store::sql)?;
    stmt.query_map([project], |r| Ok((r.get(0)?, r.get(1)?)))
        .map_err(crate::store::sql)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(crate::store::sql)
}

/// What a call's `branch` names in a project: main for none or `main`; the board for
/// `_project_board_`; a branch by its name, in any case; refused, listing what there is.
pub(crate) fn named(store: &Store, project: &str, raw: Option<&str>) -> Result<Named, String> {
    named_in(&store.conn, project, raw)
}

/// What a call's `branch` names, as a connection sees the branches.
pub(crate) fn named_in(
    conn: &rusqlite::Connection,
    project: &str,
    raw: Option<&str>,
) -> Result<Named, String> {
    let name = raw.unwrap_or("").trim();
    if name.is_empty() || crate::pathtext::same(name, "main") {
        return Ok(Named {
            project: project.to_string(),
            tree: String::new(),
            name: "main".to_string(),
        });
    }
    if crate::pathtext::same(name, BOARD_NAME) {
        return Ok(Named {
            project: project.to_string(),
            tree: BOARD.to_string(),
            name: BOARD_NAME.to_string(),
        });
    }
    // The working directories are sections of main and every branch now: read there.
    if crate::pathtext::same(name, DIRECTORY_NAME) {
        return Err(text!("tools.branch.directoryGone", name = DIRECTORY_NAME));
    }
    let all = branches(conn, project)?;
    if let Some((id, found)) = all.iter().find(|(_, n)| crate::pathtext::same(n, name)) {
        return Ok(Named {
            project: project.to_string(),
            tree: id.clone(),
            name: found.clone(),
        });
    }
    // The window's group on a locked main (`begin_group`): named by the window only — MCP's
    // calls naming a branch of the app's own are refused before (`Core::tool`).
    if name == crate::write::GROUP_BRANCH
        && let Some(id) = conn
            .query_row(
                "SELECT id FROM branches WHERE project_id = ? AND name = ?",
                [project, name],
                |r| r.get::<_, String>(0),
            )
            .optional()
            .map_err(crate::store::sql)?
    {
        return Ok(Named {
            project: project.to_string(),
            tree: id,
            name: name.to_string(),
        });
    }
    no_branch(conn, project, name)
}

/// A call's `branch` that names none of the project's: refused, listing what there is.
pub(crate) fn no_branch<T>(
    conn: &rusqlite::Connection,
    project: &str,
    name: &str,
) -> Result<T, String> {
    let all = branches(conn, project)?;
    let mut listed = vec!["main".to_string()];
    listed.extend(all.into_iter().map(|(_, n)| n));
    listed.push(BOARD_NAME.to_string());
    Err(text!(
        "tools.branch.gone",
        name = name,
        listed = listed.join(" / ")
    ))
}

impl Core {
    /// A tool other than `guide`, run for `who`. Through MCP, a list is cut to its default size
    /// after it is made (`paging`), the
    /// value's `after` and `limit` taken out before the tool reads it; the window's requests read
    /// whole.
    pub(crate) fn tool(
        &self,
        who: &Who,
        name: &str,
        arguments: &Map<String, Value>,
    ) -> Result<Output, String> {
        let spec = (who.side == crate::write::Side::Mcp)
            .then(|| paging::spec_of(name, &Args::new(arguments)))
            .flatten();
        let Some(spec) = spec else {
            return self.tool_whole(who, name, arguments);
        };
        let mut arguments = arguments.clone();
        let page = match arguments.get_mut("value") {
            Some(Value::Object(value)) => paging::Page::take(value, spec.limit, spec.most)?,
            _ => paging::Page {
                limit: spec.limit,
                after: None,
            },
        };
        let mut out = self.tool_whole(who, name, &arguments)?;
        // The projects, a list at the top, put under a name of their own to be cut.
        if name == "project_list"
            && let Output::Json(Value::Array(rows)) = out
        {
            let mut wrapped = Map::new();
            wrapped.insert("projects".into(), Value::Array(rows));
            out = Output::Json(Value::Object(wrapped));
        }
        Ok(paging::cut_output(out, &spec, &page))
    }

    /// A tool run as it is made: through MCP, `card_list`, a history and a body cut as they are
    /// made (`Args::paged`).
    fn tool_whole(
        &self,
        who: &Who,
        name: &str,
        arguments: &Map<String, Value>,
    ) -> Result<Output, String> {
        let args = Args::new(arguments).paged(who.side == crate::write::Side::Mcp);
        // A branch of the app's own (`~…`) is the window's: to MCP it is not there.
        if who.side == crate::write::Side::Mcp
            && let Some(own) = args
                .optional_string("branch")
                .map(str::trim)
                .filter(|b| b.starts_with('~'))
        {
            let store = self.reading()?;
            let summary = project_summary(&store, args.string("project")?)?;
            return no_branch(&store.conn, &summary.id, own);
        }
        if !name.starts_with("branch_") && name != "batch" {
            trees_named(&*self.reading()?, &args)?;
            if !BRANCHED.contains(&name) && name != "app_control" && args.raw.contains_key("branch")
            {
                return Err(text!("tools.branch.notTaken", name = name));
            }
        }
        // What the project and the key let MCP do, apart from main's lock, and what a person says
        // to what they ask about each time (`asking`); a batch's steps are judged one by one
        // (`batch`), and asked about once, for the run.
        let granted = self.admitted(who, name, &args)?;
        // A call that reads or writes in a working directory's section reads its trees with them
        // whole; any other, without what is read in them.
        let _whole = crate::workdir::Whole::during(self.touches_workdir(&args));
        // What only reads, read apart from the writes: what was committed, not waiting for a write
        // going on (`Core::reading`).
        match name {
            "project_list" => reads::project_list(&*self.reading()?),
            "project_read" => reads::project_read(&*self.reading()?, &args),
            "section_list" => reads::section_list(&*self.reading()?, &args),
            // A picture is drawn by the app, with no store lock held while it draws.
            "section_read" | "card_read" if picture::asked(arguments) => {
                self.picture(name, arguments)
            }
            "section_read" if mocks::asked(arguments) => mocks::mock_read(&*self.reading()?, &args),
            "section_read" => reads::section_read(&*self.reading()?, &args),
            "card_read" => reads::card_read(&*self.reading()?, &args),
            "card_list" => reads::card_list(&*self.reading()?, &args),
            "tag_list" => reads::tag_list(&*self.reading()?, &args),
            "guide" => reads::own_kind_guide(&*self.reading()?, &args),
            "export_read" => files::export_read(&*self.reading()?, &args),
            // Read again where the writes are: what it reads is given out to every connection
            // (`workdir::give_out`).
            "section_action" => {
                let store = self.lock();
                workdirs::section_action(&store.conn, &args).map(Output::Json)
            }
            "kind_list" | "kind_read" => {
                let store = self.reading()?;
                let project = project_summary(&store, args.string("project")?)?;
                if name == "kind_list" {
                    if let Some(key) = args.value()?.keys().next() {
                        return Err(text!("tools.kindList.noKey", key = key));
                    }
                    kind_tools::kind_list(&store.conn, &project.id, &project.name)
                } else {
                    kind_tools::kind_read(&store.conn, &project.id, &project.name, &args)
                }
                .map(Output::Json)
            }
            "export_execute" => self.export_execute(who, &args),
            n if n.starts_with("branch_") => self.branch_tool(who, n, &args, &granted),
            n if WRITES.contains(&n) => self.write_tool(who, n, &args),
            "batch" => self.batch_tool(who, &args, &granted),
            "mr_list" | "mr_read" | "mr_delete" => self.mr_tool(who, name, &args),
            "inspection_list" | "inspection_read" | "inspection_update" => {
                self.inspection_tool(who, name, &args)
            }
            "app_control" => self.app_control(arguments),
            _ => {
                let _ = who;
                Err(text!("tools.notYet", name = name))
            }
        }
    }
}

impl Core {
    /// How a call reaches into a working directory's section (`workdir::touches`).
    fn touches_workdir(&self, args: &Args) -> crate::workdir::Reach {
        let none = crate::workdir::Reach::None;
        let Ok(store) = self.reading() else {
            return none;
        };
        let Ok(name) = args.string("project") else {
            return none;
        };
        project_summary_in(&store.conn, name)
            .map(|p| crate::workdir::touches(&store.conn, &p.id, args.raw))
            .unwrap_or(none)
    }

    /// A tool that writes: one transaction, nothing of it kept when any of it is refused — nor when
    /// a card it gives the rev of is not as it was read (`revs`), held to the tree before it writes.
    fn write_tool(&self, who: &Who, name: &str, args: &Args) -> Result<Output, String> {
        // The window's writes give none, and any it gives is not read.
        let given = if who.side == crate::write::Side::Mcp {
            revs::given(name, args.raw)?
        } else {
            Vec::new()
        };
        self.plain_writing(who.clone(), |tx| {
            if !given.is_empty() {
                revs::refuse_stale(tx, &scope_of(tx, args, None)?, &given)?;
            }
            write_in(tx, name, args, None)
        })
        .map(Output::Json)
    }
}

/// An answer headed with the tree `name`, as JSON (`headed`).
pub(crate) fn headed_json(value: Value, name: &str) -> Value {
    match headed_as(Output::Json(value), name) {
        Output::Json(v) => v,
        _ => Value::Null,
    }
}

/// The tree a write tool's call writes in: the one its `branch` names, else the one `default` names
/// (a batch's), else main.
pub(crate) fn scope_of(
    tx: &crate::write::Tx,
    args: &Args,
    default: Option<&Named>,
) -> Result<cards::Scope, String> {
    let summary = project_summary_in(tx.conn(), args.string("project")?)?;
    let named = match (args.optional_string("branch"), default) {
        (None, Some(d)) if d.project == summary.id => d.clone(),
        (raw, _) => named_in(tx.conn(), &summary.id, raw)?,
    };
    Ok(cards::Scope {
        project: summary.id,
        project_name: summary.name,
        tree: named.tree,
        tree_name: named.name,
    })
}

/// A write tool run in a transaction already open — a call's own or a batch's — in the
/// tree its `branch` names, else the tree `default` names (a batch's), else main.
pub(crate) fn write_in(
    tx: &mut crate::write::Tx,
    name: &str,
    args: &Args,
    default: Option<&Named>,
) -> Result<Value, String> {
    if ["branch_create", "branch_update", "branch_delete"].contains(&name) {
        return branches::branch_write(tx, name, args);
    }
    if name == "project_create" {
        return projects::project_create(tx, args);
    }
    let scope = scope_of(tx, args, default)?;
    // A step of a batch that reads or writes in a working directory's section sees it whole.
    let _whole = crate::workdir::Whole::at_least(crate::workdir::touches(
        tx.conn(),
        &scope.project,
        args.raw,
    ));
    // A card's rev is held to the tree before the call (`revs`): what is written goes without it.
    let unrevved = revs::without(name, args.raw);
    let args = &match &unrevved {
        Some(raw) => Args::new(raw).paged(args.paged),
        None => Args::new(args.raw).paged(args.paged),
    };
    workdirs::refuse_in_workdir(tx, &scope, name, args, default.is_some())?;
    let out =
        match name {
            "card_create" => writes::card_create(tx, &scope, args)
                .map_err(|e| lines::to_the_board(tx, &scope, e))?,
            "card_update" => writes::card_update(tx, &scope, args)
                .map_err(|e| lines::to_the_board(tx, &scope, e))?,
            "card_delete" => writes::card_delete(tx, &scope, args)?,
            "section_create" => sections::section_create(tx, &scope, args)?,
            "section_update" => sections::section_update(tx, &scope, args)?,
            "section_delete" => sections::section_delete(tx, &scope, args)?,
            "tag_update" => tags::tag_update(tx, &scope, args)?,
            "project_update" => projects::project_update(tx, &scope, args)?,
            "export_create" => exports::export_create(tx, &scope, args)?,
            "export_update" => exports::export_update(tx, &scope, args)?,
            "export_delete" => exports::export_delete(tx, &scope, args)?,
            "kind_create" => kind_tools::kind_create(tx, &scope, args)?,
            "kind_update" => kind_tools::kind_update(tx, &scope, args)?,
            "kind_delete" => kind_tools::kind_delete(tx, &scope, args)?,
            "card_action" => actions::card_action(tx, &scope, args)?,
            _ => {
                return Err(text!("tools.notYet", name = name));
            }
        };
    // The project's own settings take no tree, and their answer no heading.
    if !BRANCHED.contains(&name) {
        return Ok(out);
    }
    // A card made or moved with no place is given one.
    update::place_unplaced(tx, &scope)?;
    Ok(headed_json(out, &scope.tree_name))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What the tests of a write's cost hold it to (`tests/write_cost.rs`) is counted: a project
    /// worked out is held while it is, and let go with it.
    #[test]
    fn a_project_worked_out_is_counted_as_held_while_it_is() {
        let store = Store::open(crate::fixtures::read_only_sample()).unwrap();
        let before = held_on_this_thread();
        let seen = Seen::read(&store, "見本", None).unwrap();
        assert_eq!(held_on_this_thread(), before + 1);
        drop(seen);
        assert_eq!(held_on_this_thread(), before);
    }
}
