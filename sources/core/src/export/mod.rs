//! What a project is written out as: every file the exports ask for, by export and path — the
//! types of a file in TypeScript, Zod or JSON Schema, a page of charts, a document — and the
//! problems found in the settings. Where an export is written to is not here: it is given when it
//! is written (`tools::files`). The screen's preview reads it too (`export_read`, `card_read`'s
//! `writtenTo`).

pub mod charts;
pub(crate) mod data;
pub mod datachart;
pub(crate) mod documents;
pub(crate) mod flowdoc;
pub mod ident;
mod jsonschema;
pub mod mermaid;
pub mod mock;
pub mod pages;
pub mod paths;
pub mod settings;
mod typescript;
mod zod;

pub(crate) use documents::rows_markdown;
pub(crate) use mermaid::find_embed;

use crate::model::{Card, Detail, ExportSetting, Field, Ghost, Rule};
use crate::notetext::{self, plain_path};
use crate::ojson;
use serde_json::{Map, Value};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::rc::Rc;

/// What a file holds.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Content {
    Types(String),
    Chart,
    Document,
    /// The data cards (`json-data`): a list of their data, as JSON.
    Data,
    /// An SVG card's picture, as it is.
    Svg,
    /// An HTML card's page.
    Html,
    /// A ui section's mock (`mock`), as one page.
    Mock,
}

impl Content {
    /// What it holds, as `export_read` says it (`Content.word`).
    pub fn word(&self) -> String {
        match self {
            Content::Types(format) => format.clone(),
            Content::Chart => "chart".into(),
            Content::Document => "document".into(),
            Content::Data => crate::model::JSON_DATA.into(),
            Content::Svg => "svg".into(),
            Content::Html => "html".into(),
            Content::Mock => "mock".into(),
        }
    }

    fn label(&self) -> String {
        match self {
            Content::Types(format) => format.clone(),
            Content::Chart => text!("export.content.chart"),
            Content::Document => text!("export.content.document"),
            Content::Data => text!("export.content.data"),
            Content::Svg => text!("export.content.svg"),
            Content::Html => text!("export.content.html"),
            Content::Mock => text!("export.content.mock"),
        }
    }
}

/// One file, the export that writes it (by its place in the list), and the cards in it in the
/// project's order — or the one version of a stackdoc it holds (its row's id).
#[derive(Clone, Debug)]
pub struct Planned {
    pub export: usize,
    pub path: String,
    pub content: Content,
    pub cards: Vec<String>,
    pub row: Option<String>,
    /// A ui section's page: the section, by id (its parts are `cards`).
    pub section: Option<String>,
}

/// The project, and what the writers look up in it.
pub struct Project<'a> {
    pub detail: &'a Detail,
    /// What its cards, ghosts and sections are looked up by (`Index`).
    index: Rc<Index>,
    pub directory: Rc<crate::directory::Directory>,
    /// The kinds of card, of section and of line the project has: the built-in ones and its own.
    pub defs: Rc<crate::kinds::Defs>,
    /// The cards a line reaches that this project leaves out (a file of code holds only shapes
    /// and notes): by the id the line holds, the card's name and its path in full.
    pub beyond: HashMap<String, (String, String)>,
    /// The tree beside this one that its lines may reach: main's, for the board's — a link to one of its cards, a path
    /// written in a body naming one.
    pub outside: Option<Box<Project<'a>>>,
    /// The working directories as they are read, as a tree of their own beside this one, that its
    /// lines and its paths into a working directory's section reach (`repo/src:'a.rs'`) when what
    /// they hold is not read into the tree itself.
    pub files: Option<Box<Project<'a>>>,
    /// Where the working directories' files are on this machine and how they map to languages,
    /// with `files`: what reading a file's functions and types takes (`code::files`).
    pub code: Option<Rc<crate::code::files::Dirs>>,
    /// The cards gone that a line still reaches: by
    /// the id the line holds, the path each had as the history last kept it — what the check says
    /// of the line, as it has no card to name.
    pub gone: HashMap<String, String>,
}

/// What a name names as written: a card, or a card ghost as itself.
#[derive(Clone, Copy, Debug)]
pub enum Named<'a> {
    Card(&'a Card),
    Ghost(&'a Ghost),
}

/// What a project's reading is looked up by — its cards and card ghosts by id, its sections'
/// kinds, its names (`Directory`) and its kinds (`Defs`) —, made once for the reading
/// (`Index::of`) and kept with it while nothing it is made of changes: a write's reading keeps it
/// (`tools::Seen`), so a card is not looked up by making them all again.
pub struct Index {
    /// Into the cards, then those put only inside a container (`Project::nth`).
    by_id: HashMap<String, usize>,
    ghosts_by_id: HashMap<String, usize>,
    home: String,
    section_kind: HashMap<String, Option<String>>,
    directory: Rc<crate::directory::Directory>,
    defs: Rc<crate::kinds::Defs>,
}

impl Index {
    pub fn of(detail: &Detail) -> Index {
        let sections: Vec<crate::directory::SectionRef> = detail
            .sections
            .iter()
            .map(|s| crate::directory::SectionRef {
                id: s.id.clone(),
                name: s.name.clone(),
                parent_id: s.parent_id.clone(),
            })
            .collect();
        let cards: Vec<crate::directory::CardRef> = detail
            .schemas
            .iter()
            .chain(detail.inner.iter())
            .map(|c| crate::directory::CardRef {
                id: c.id.clone(),
                name: c.name.clone(),
                section_id: c.section_id.clone(),
                inside: c.inside.clone(),
            })
            .collect();
        let ghosts: Vec<crate::directory::GhostRef> = detail
            .ghosts
            .iter()
            .map(|g| crate::directory::GhostRef {
                id: g.id.clone(),
                name: g.name.clone(),
                section_id: g.section_id.clone(),
                target: g.target.clone(),
            })
            .collect();
        Index {
            by_id: detail
                .schemas
                .iter()
                .chain(detail.inner.iter())
                .enumerate()
                .map(|(i, c)| (c.id.clone(), i))
                .collect(),
            ghosts_by_id: detail
                .ghosts
                .iter()
                .enumerate()
                .map(|(i, g)| (g.id.clone(), i))
                .collect(),
            home: detail
                .sections
                .first()
                .map(|s| s.id.clone())
                .unwrap_or_default(),
            section_kind: detail
                .sections
                .iter()
                .map(|s| (s.id.clone(), s.kind.clone()))
                .collect(),
            directory: Rc::new(crate::directory::Directory::new(&sections, &cards, &ghosts)),
            defs: Rc::new(crate::kinds::Defs::of_project(
                detail.project.kinds.as_deref(),
            )),
        }
    }
}

impl<'a> Project<'a> {
    pub fn new(detail: &'a Detail) -> Project<'a> {
        Project::indexed(detail, Rc::new(Index::of(detail)))
    }

    /// The project as `index`, made of `detail` as it is (`Index::of`), looks it up.
    pub fn indexed(detail: &'a Detail, index: Rc<Index>) -> Project<'a> {
        Project {
            detail,
            directory: index.directory.clone(),
            defs: index.defs.clone(),
            index,
            beyond: HashMap::new(),
            outside: None,
            files: None,
            code: None,
            gone: HashMap::new(),
        }
    }

    /// The same, with where the working directories' files are and how they map to languages.
    pub fn with_code(mut self, code: Option<Rc<crate::code::files::Dirs>>) -> Project<'a> {
        self.code = code;
        self
    }

    /// The same, with the paths the cards gone that its lines reach had (`gone`).
    pub fn with_gone(mut self, gone: HashMap<String, String>) -> Project<'a> {
        self.gone = gone;
        self
    }

    /// The same, with the tree beside it its lines may reach (main's, for the board's).
    pub fn with_outside(mut self, outside: Option<Project<'a>>) -> Project<'a> {
        self.outside = outside.map(Box::new);
        self
    }

    /// The same, with the working directories its lines may reach (main's and a branch's).
    pub fn with_files(mut self, files: Option<Project<'a>>) -> Project<'a> {
        self.files = files.map(Box::new);
        self
    }

    /// A working directory's section among the working directories as read, by its name (in any
    /// case).
    fn work_dir_named(&self, name: &str) -> Option<&'a crate::model::Section> {
        self.detail.sections.iter().find(|s| {
            s.parent_id.as_deref() == Some(crate::workdir::ROOT_ID)
                && crate::pathtext::same(&s.name, name)
        })
    }

    /// A working directory's section among the working directories as read, by its name (in any
    /// case): of the kind `workdir` where it was read here.
    pub fn mount_named(&self, name: &str) -> Option<&'a crate::model::Section> {
        self.work_dir_named(name)
    }

    /// A section of the working directories as read inside `parent`, by its name (in any case).
    fn work_dir_child(&self, parent: &str, name: &str) -> Option<&'a crate::model::Section> {
        self.detail.sections.iter().find(|s| {
            s.parent_id.as_deref() == Some(parent) && crate::pathtext::same(&s.name, name)
        })
    }

    /// A file's id as a line holds it, from its path in the tree (`repo/src:'a.rs'`) — there, or
    /// not yet (its id made of the working directory's and the path the names say) — when this
    /// tree's lines reach files. None for a path that does not start at a working directory's
    /// section, or that goes into a file.
    pub fn file_id(&self, raw: &str) -> Option<String> {
        let files = self.files.as_deref()?;
        let raw = raw.trim();
        if raw.starts_with("file:") {
            return None;
        }
        let (Some(section), card) = crate::pathtext::split_section(raw) else {
            return None;
        };
        let names = crate::pathtext::names(card);
        let [name] = names.as_slice() else {
            return None;
        };
        let segs: Vec<String> = crate::pathtext::slashes(section)
            .into_iter()
            .map(|s| crate::pathtext::unquote(s).into_owned())
            .collect();
        let (head, rest) = segs.split_first()?;
        let mount = files.work_dir_named(head)?;
        let dir = mount.id.strip_prefix("dir:")?;
        // Down the directories read, as far as they are there; the rest by the names written.
        let mut at = Some(mount.id.clone());
        let mut parts: Vec<String> = Vec::new();
        for seg in rest {
            match at
                .as_deref()
                .and_then(|parent| files.work_dir_child(parent, seg))
            {
                Some(child) => {
                    parts = child
                        .id
                        .split_once('/')
                        .map(|(_, p)| p.split('/').map(str::to_string).collect())
                        .unwrap_or_default();
                    at = Some(child.id.clone());
                }
                None => {
                    parts.push(seg.clone());
                    at = None;
                }
            }
        }
        if let Some(section) = &at {
            if let Some(found) = files.directory.entry(section, name).and_then(|e| {
                files
                    .directory
                    .follow(e)
                    .map(|i| files.nth(i))
                    .filter(|c| c.kind == "file")
            }) {
                return Some(found.id.clone());
            }
            // A name the tree made of a file's before (`CardKinds_md`) is that file's, written as
            // it is named now: not one more file.
            let before = |n: &str| -> String {
                n.chars()
                    .map(|c| if c.is_alphanumeric() { c } else { '_' })
                    .collect()
            };
            if files
                .directory
                .members(section)
                .iter()
                .map(|&i| files.nth(i))
                .any(|c| c.name != *name && before(&c.name) == *name)
            {
                return None;
            }
        }
        parts.push(name.clone());
        if parts
            .iter()
            .any(|p| p.is_empty() || p == "." || p == ".." || p.contains('/') || p.contains('\\'))
        {
            return None;
        }
        Some(crate::workdir::file_id(dir, &parts.join("/")))
    }

    /// A path that goes on past a working directory's file (`repo/src:'a.rs'.Order.total`): the
    /// file's id, there or not yet (`file_id`), and the names past it as a path writes them (the
    /// function or type it names, read from the file when it is read).
    pub fn inside_file(&self, raw: &str) -> Option<(String, String)> {
        let raw = raw.trim();
        let (Some(section), card) = crate::pathtext::split_section(raw) else {
            return None;
        };
        let names = crate::pathtext::names(card);
        let (file, inside) = names.split_first()?;
        if inside.is_empty() || inside.iter().any(String::is_empty) {
            return None;
        }
        let file_path = crate::pathtext::card_path(section, file);
        let id = self.file_id(&file_path)?;
        Some((
            id,
            crate::pathtext::join_names(inside.iter().map(String::as_str)),
        ))
    }

    /// A path written as files were before (`file:<作業ディレクトリ>/<パス>`) as it is written now, in the
    /// tree (`repo/src:'a.rs'`); none for one not written so, or of no working directory here.
    pub fn file_form_now(&self, raw: &str) -> Option<String> {
        let files = self.files.as_deref()?;
        let rest = raw.trim().strip_prefix("file:")?;
        let (dir, path) = rest.split_once('/')?;
        let mount = files.work_dir_named(dir)?;
        let id = crate::workdir::file_id(mount.id.strip_prefix("dir:")?, path.trim_matches('/'));
        self.file_written(&id)
    }

    /// Why a path into a working directory names no file: the names in its directory that look
    /// like the one written.
    pub fn file_problem(&self, raw: &str) -> Option<String> {
        let files = self.files.as_deref()?;
        let section = crate::pathtext::split_section(raw.trim()).0?;
        let home = &files.index.home;
        files.directory.section_named(section.trim(), Some(home))?;
        let names: Vec<String> = files
            .detail
            .schemas
            .iter()
            .map(|c| c.name.clone())
            .collect();
        Some(files.directory.find_problem(raw, home, &names))
    }

    /// A file a line holds by its id, as it is written: its path in the tree (`repo/src:'a.rs'`) —
    /// its card's where it is there, else made of the names in its id.
    pub fn file_written(&self, id: &str) -> Option<String> {
        let files = self.files.as_deref()?;
        if let Some(card) = files.card(id) {
            return Some(files.full(card));
        }
        let (dir, path) = crate::workdir::of_file_id(id)?;
        let mount_id = crate::workdir::dir_id(dir, "");
        let mount = files.detail.sections.iter().find(|s| s.id == mount_id)?;
        let parts: Vec<&str> = path.split('/').collect();
        let (file, dirs) = parts.split_last()?;
        let mut names = vec![mount.name.clone()];
        let mut at = Some(mount_id);
        for (i, seg) in dirs.iter().enumerate() {
            let id = crate::workdir::dir_id(dir, &dirs[..=i].join("/"));
            let named = at
                .as_deref()
                .and_then(|_| files.detail.sections.iter().find(|s| s.id == id))
                .map(|s| s.name.clone());
            at = named.as_ref().map(|_| id);
            names.push(named.unwrap_or_else(|| (*seg).to_string()));
        }
        Some(crate::pathtext::card_path(
            &crate::pathtext::join_sections(names.iter().map(String::as_str)),
            file,
        ))
    }

    /// Whether the working directory a file's id names was read on this machine: where it was,
    /// a file not found in it is not there.
    pub fn file_dir_read(&self, id: &str) -> bool {
        let Some(files) = self.files.as_deref() else {
            return false;
        };
        let Some(mount) = crate::workdir::mount_of(id) else {
            return false;
        };
        files
            .detail
            .sections
            .iter()
            .any(|s| s.id == mount && s.kind.as_deref() == Some(crate::kinds::WORKDIR))
    }

    /// A file's card among the working directories as read, by its id: none when it is not there.
    pub fn file_card(&self, id: &str) -> Option<&'a Card> {
        self.files.as_ref()?.card(id)
    }

    /// A card of the tree beside this one, by its id: what a board's line reaches in main — a
    /// working directory's file too.
    pub fn outside_card(&self, id: &str) -> Option<&'a Card> {
        let outside = self.outside.as_ref()?;
        outside.card(id).or_else(|| outside.file_card(id))
    }

    /// The card a path names in the tree beside this one, when it says its section (a name
    /// alone is read here): a board's line written to main's `Docs/Design:CardKinds`. Names
    /// directly under the roots are one each across both trees (`project_tree`), so a path is the
    /// one tree's or the other's.
    pub fn outside_id(&self, raw: &str) -> Option<String> {
        let outside = self.outside.as_ref()?;
        if !crate::pathtext::has_section(raw) {
            return None;
        }
        match outside.written_id(raw, &outside.index.home) {
            Some(id) => outside.card(&id).map(|c| c.id.clone()),
            // A working directory's file, there or not yet.
            None => outside.file_id(raw),
        }
    }

    /// Where a path in full lands among the working directories as read (`paths::landing` there): a
    /// file, and the names past it — a function or a type in it — that a text names.
    pub fn files_landing(&self, raw: &str) -> Option<(&'a Card, String)> {
        let files = self.files.as_deref()?;
        if !crate::pathtext::has_section(raw) {
            return None;
        }
        paths::landing(raw, &files.index.home, files)
    }

    /// The root of the tree this is (its section, by id).
    pub fn home(&self) -> &str {
        &self.index.home
    }

    /// Where a path in full lands in the tree beside this one (`paths::landing` there): what a
    /// board's text names of main's.
    pub fn outside_landing(&self, raw: &str) -> Option<(&'a Card, String)> {
        let outside = self.outside.as_deref()?;
        if !crate::pathtext::has_section(raw) {
            return None;
        }
        paths::landing(raw, &outside.index.home, outside)
    }

    /// Whether a card of `kind` holds cards inside it (a container): a flow, a goal, or one
    /// of the project's own kinds with `holds`.
    pub fn is_container(&self, kind: &str) -> bool {
        self.defs.cards().holds_cards(kind)
    }

    /// Whether a card of `kind` opens as a card map of the cards it holds, placed one by one
    /// (`itemPlaces`): a goal, and a container of the project's own kind. A flow's nodes sit on
    /// its own card map apart.
    pub fn card_mapped(&self, kind: &str) -> bool {
        kind == "goal" || (self.is_own_kind(kind) && self.is_container(kind))
    }

    /// Whether `kind` is one the project defines (`projects.kinds.cards`): read and written by its
    /// definition alone, and never written out as a type.
    pub fn is_own_kind(&self, kind: &str) -> bool {
        self.defs.cards().is_own(kind)
    }

    /// The card an id names: a card, or the card a card ghost's chain leads to — what a
    /// reference held by id stands for, written through a ghost or not.
    pub fn card(&self, id: &str) -> Option<&'a Card> {
        if let Some(&i) = self.index.by_id.get(id) {
            return Some(self.nth(i));
        }
        let entry = self.directory.entry_of(id)?;
        self.directory.follow(entry).map(|i| self.nth(i))
    }

    /// The card the directory numbers `i`: the cards, then those put only inside a
    /// container (`inner`).
    pub fn nth(&self, i: usize) -> &'a Card {
        let cards = &self.detail.schemas;
        if i < cards.len() {
            &cards[i]
        } else {
            &self.detail.inner[i - cards.len()]
        }
    }

    /// The cards inside a container, in project order: a flow's nodes, a goal's cards.
    pub fn inner_of(&self, container: &str) -> Vec<&'a Card> {
        self.directory
            .inside(container)
            .iter()
            .map(|&i| self.nth(i))
            .collect()
    }

    /// The container a card is inside, when it is there.
    pub fn container_of(&self, card: &Card) -> Option<&'a Card> {
        card.inside
            .as_deref()
            .and_then(|c| self.index.by_id.get(c).map(|&i| self.nth(i)))
    }

    /// The card inside `container` named `name`, a card ghost followed.
    pub fn inner_named(&self, container: &str, name: &str) -> Option<&'a Card> {
        let entry = self.directory.entry(container, name)?;
        self.directory.follow(entry).map(|i| self.nth(i))
    }

    /// A card ghost by its id.
    pub fn ghost(&self, id: &str) -> Option<&'a Ghost> {
        self.index
            .ghosts_by_id
            .get(id)
            .map(|&g| &self.detail.ghosts[g])
    }

    /// The card a ghost's chain leads to; none when it leads nowhere.
    pub fn ghost_card(&self, ghost: &Ghost) -> Option<&'a Card> {
        self.card(&ghost.id)
    }

    /// A ghost's section.
    pub fn section_of_ghost(&self, ghost: &Ghost) -> String {
        ghost
            .section_id
            .clone()
            .unwrap_or_else(|| self.index.home.clone())
    }

    /// A card ghost in full: `Core:User`.
    pub fn full_ghost(&self, ghost: &Ghost) -> String {
        crate::pathtext::card_path(
            &self.directory.full_section(&self.section_of_ghost(ghost)),
            &ghost.name,
        )
    }

    /// What an id names, in full as it was written: a card's path, or a ghost's own.
    pub fn named(&self, id: &str) -> Option<String> {
        if let Some(&i) = self.index.by_id.get(id) {
            return Some(self.full(self.nth(i)));
        }
        self.ghost(id)
            .map(|g| self.full_ghost(g))
            .or_else(|| self.outside.as_ref().and_then(|o| o.named(id)))
            .or_else(|| self.file_written(id))
    }

    /// The card ghosts of a section, in project order.
    pub fn ghosts_in(&self, section: &str) -> Vec<&'a Ghost> {
        self.directory
            .ghosts_in(section)
            .iter()
            .map(|&g| &self.detail.ghosts[g])
            .collect()
    }

    /// What a name names read from `base`, a card ghost as itself (not followed).
    pub fn find_named(&self, raw: &str, base: &str) -> Option<Named<'a>> {
        Some(match self.directory.find_entry(raw, Some(base))? {
            crate::directory::Entry::Card(i) => Named::Card(self.nth(i)),
            crate::directory::Entry::Ghost(g) => Named::Ghost(&self.detail.ghosts[g]),
        })
    }

    /// The id a name is held by, as written — a card ghost's own, not its card's — when it
    /// leads to a card (`ref`, a pointer's card, what a goal holds).
    pub fn written_id(&self, raw: &str, base: &str) -> Option<String> {
        let entry = self.directory.find_entry(raw, Some(base))?;
        self.directory.follow(entry)?;
        Some(match entry {
            crate::directory::Entry::Card(i) => self.nth(i).id.clone(),
            crate::directory::Entry::Ghost(g) => self.detail.ghosts[g].id.clone(),
        })
    }

    /// The names of the project's working directories (the project tree's `workdir` entries):
    /// what a path written out into one starts with (`notetext::work_dir_path`), known without
    /// reading any folder.
    pub fn work_dir_names(&self) -> Vec<String> {
        self.defs
            .tree
            .main
            .iter()
            .filter(|t| t.kind == crate::kinds::WORKDIR)
            .map(|t| t.name.clone())
            .collect()
    }

    pub fn section_of(&self, card: &Card) -> String {
        card.section_id
            .clone()
            .unwrap_or_else(|| self.index.home.clone())
    }

    /// The kind of the section `id`; none for one of no kind or none there.
    pub fn section_kind(&self, id: &str) -> Option<&str> {
        self.index.section_kind.get(id).and_then(Option::as_deref)
    }

    pub fn section_kind_of(&self, card: &Card) -> Option<&str> {
        self.index
            .section_kind
            .get(self.section_of(card).as_str())
            .and_then(Option::as_deref)
    }

    /// In full, as stored and as MCP answers: `Core:User`; a card inside a container on
    /// from the container's (`Behavior:Checkout.review`, `Goals:Launch.Design`).
    pub fn full(&self, card: &Card) -> String {
        let mut names = vec![card.name.as_str()];
        let mut head = card;
        let mut seen = 0;
        while let Some(container) = self.container_of(head)
            && seen < 32
        {
            names.push(container.name.as_str());
            head = container;
            seen += 1;
        }
        names.reverse();
        let names = crate::pathtext::join_names(names);
        if self.detail.sections.is_empty() {
            return names;
        }
        format!(
            "{}:{names}",
            self.directory.full_section(&self.section_of(head))
        )
    }

    /// The card a stored name refers to, read from `base`.
    pub fn find(&self, raw: &str, base: &str) -> Option<&'a Card> {
        self.directory.find(raw, Some(base)).map(|i| self.nth(i))
    }

    /// The card a name in a path written in a card refers to: held to its reader's trees.
    pub fn find_seeing(&self, raw: &str, base: &str) -> Option<&'a Card> {
        self.directory.find_seeing(raw, base).map(|i| self.nth(i))
    }

    /// A section as what is exported names it: a root by its name, else under its root.
    pub fn exported_section(&self, section: &str) -> String {
        self.directory.full_section(section)
    }

    /// A card as what is exported names it: `Core:User` with sections, `User` without.
    pub fn exported(&self, card: &Card) -> String {
        if !self.directory.is_sectioned() {
            return crate::pathtext::quote(&card.name).into_owned();
        }
        crate::pathtext::card_path(&self.exported_section(&self.section_of(card)), &card.name)
    }

    /// As what is exported names `target` from a card in `base`: bare in its own
    /// section, `Core:Address` anywhere else.
    pub fn exported_from(&self, target: &Card, base: &str) -> String {
        let section = self.section_of(target);
        if section == base {
            return crate::pathtext::quote(&target.name).into_owned();
        }
        crate::pathtext::card_path(&self.exported_section(&section), &target.name)
    }
}

// MARK: - Notes and rules as they are written out

/// An enum's values that have a description, one line each as code comments say them: the
/// value, its label, what it stands for. None when no value has one.
pub fn described_values(card: &Card) -> Vec<String> {
    card.values
        .iter()
        .filter(|v| !v.description.is_empty())
        .map(|v| {
            if v.label.is_empty() {
                text!(
                    "exportCode.enumValue.described",
                    value = v.value,
                    description = v.description
                )
            } else {
                text!(
                    "exportCode.enumValue.labelled",
                    value = v.value,
                    label = v.label,
                    description = v.description
                )
            }
        })
        .collect()
}

/// A note as it is written into code: its name — and label — then its body, each
/// reference the card's name and field, a diagram said in words.
pub fn note_text(note: &Card, mounts: &[String]) -> String {
    let body = notetext::transform(&note.body, |r| {
        Some(if r.kind == notetext::Kind::Embed {
            text!("exportDocs.note.diagram", plain = plain_path(&r.path))
        } else {
            notetext::shown(r, mounts)
        })
    });
    let body = body.trim();
    let title = if note.label.is_empty() {
        note.name.clone()
    } else {
        format!("{} — {}", note.name, note.label)
    };
    if body.is_empty() {
        title
    } else {
        format!("{title}\n\n{body}")
    }
}

/// A note as a block comment in TypeScript or Zod.
pub fn note_comment(note: &Card, mounts: &[String]) -> String {
    let mut lines = vec!["/**".to_string()];
    for line in note_text(note, mounts).split('\n') {
        lines.push(if line.is_empty() {
            " *".to_string()
        } else {
            format!(" * {}", line.replace("*/", "*\\/"))
        });
    }
    lines.push(" */".to_string());
    lines.join("\n")
}

/// Rule params that name a path.
pub fn rule_path_params(kind: &str) -> &'static [&'static str] {
    match kind {
        "lteRef" | "gteRef" | "memberOf" => &["target"],
        "derivedFrom" => &["from"],
        _ => &[],
    }
}

fn known_rule_note(kind: &str) -> Option<String> {
    Some(match kind {
        "sortedBy" => text!("exportDocs.rule.sortedBy"),
        "uniqueBy" => text!("exportDocs.rule.uniqueBy"),
        "requiredIf" => text!("exportDocs.rule.requiredIf"),
        "absentIf" => text!("exportDocs.rule.absentIf"),
        "derivedFrom" => text!("exportDocs.rule.derivedFrom"),
        "monotonicPerKey" => text!("exportDocs.rule.monotonicPerKey"),
        "chainedBy" => text!("exportDocs.rule.chainedBy"),
        "memberOf" => text!("exportDocs.rule.memberOf"),
        "lteRef" => text!("exportDocs.rule.lteRef"),
        "gteRef" => text!("exportDocs.rule.gteRef"),
        "pattern" => text!("exportDocs.rule.pattern"),
        _ => return None,
    })
}

pub const COMPILABLE_RULES: [&str; 3] = ["sortedBy", "uniqueBy", "pattern"];

pub fn rule_kind(rule: &Rule) -> &str {
    rule.get("kind").and_then(Value::as_str).unwrap_or_default()
}

/// A rule's parameters, in the order written.
pub fn rule_params(rule: &Rule) -> impl Iterator<Item = (&String, &Value)> {
    rule.iter().filter(|(k, _)| k.as_str() != "kind")
}

pub fn rule_string<'r>(rule: &'r Rule, key: &str) -> Option<&'r str> {
    rule.get(key).and_then(Value::as_str)
}

/// A value the tool never interprets, as SP writes it: an object's keys sorted.
pub fn sorted(value: &Value) -> Value {
    match value {
        Value::Object(o) => {
            let mut keys: Vec<&String> = o.keys().collect();
            keys.sort();
            Value::Object(
                keys.into_iter()
                    .map(|k| (k.clone(), sorted(&o[k])))
                    .collect::<Map<_, _>>(),
            )
        }
        Value::Array(items) => Value::Array(items.iter().map(sorted).collect()),
        other => other.clone(),
    }
}

/// `kind(params) — what it means`, the paths in it as written out.
pub fn rule_comment(rule: &Rule) -> String {
    let kind = rule_kind(rule);
    let paths = rule_path_params(kind);
    let detail = rule_params(rule)
        .map(|(key, value)| match value {
            Value::String(text) => {
                let text = if paths.contains(&key.as_str()) {
                    plain_path(text)
                } else {
                    text.clone()
                };
                format!("{key}={text}")
            }
            other => format!("{key}={}", ojson::compact(&sorted(other))),
        })
        .collect::<Vec<_>>()
        .join(", ");
    let note = known_rule_note(kind).unwrap_or_else(|| text!("exportDocs.rule.unknown"));
    if detail.is_empty() {
        format!("{kind} — {note}")
    } else {
        format!("{kind}({detail}) — {note}")
    }
}

// MARK: - A card's lines as their kinds write them out

/// One of a card's lines written out as its kind of line says (`export`: a comment, or an
/// annotation too) — not what is written as it was before the definitions: an embedding
/// (the type says it), and the lines written in `x-pointsAt` and `x-payloads` (read into a
/// field's pointers and a card's payloads, `detail::legacy_lines`).
#[derive(Clone, Debug)]
pub struct OutLine {
    /// What the comments say: its own description, else its kind's label, and what it
    /// reaches (`注文した人 → User.id`).
    pub note: String,
    /// The annotation it is written in too (`x-…`), for one written as an annotation.
    pub key: Option<String>,
    /// The card it reaches (a card ghost followed to its card), and where it lands there in
    /// full (none for the card itself).
    pub to: Option<String>,
    pub landing: Option<String>,
    pub description: Option<String>,
    pub fields: Map<String, Value>,
}

/// A card's lines written out: from its head, and from each field (by the field's id).
#[derive(Clone, Debug, Default)]
pub struct CardLines {
    pub head: Vec<OutLine>,
    pub fields: HashMap<String, Vec<OutLine>>,
}

impl CardLines {
    pub fn notes_of(&self, field: &str) -> Vec<String> {
        self.fields
            .get(field)
            .map(|l| l.iter().map(|o| o.note.clone()).collect())
            .unwrap_or_default()
    }

    pub fn head_notes(&self) -> Vec<String> {
        self.head.iter().map(|o| o.note.clone()).collect()
    }
}

/// The field a path of names inside a card leads to.
pub(crate) fn field_at<'a>(fields: &'a [Field], path: &str) -> Option<&'a Field> {
    let mut level = fields;
    let mut found = None;
    for name in crate::pathtext::names(path) {
        let field = level.iter().find(|f| f.name == name)?;
        level = &field.children;
        found = Some(field);
    }
    found
}

/// A card's lines that its kinds of line write out in comments or annotations.
pub fn card_lines(project: &Project, card: &Card) -> CardLines {
    let mut out = CardLines::default();
    for line in &card.connections {
        let Some(def) = project.defs.line(&line.line) else {
            continue;
        };
        let Some(export) = &def.export else {
            continue;
        };
        if def.base == crate::kinds::Base::Embed
            || matches!(
                export.as_,
                crate::kinds::ExportAs::None | crate::kinds::ExportAs::Schema
            )
            || matches!(def.annotation(), Some("x-pointsAt") | Some("x-payloads"))
        {
            continue;
        }
        let target = line.to.as_deref().and_then(|id| project.card(id));
        // A card this project leaves out, by its name and path.
        let beyond = line.to.as_deref().and_then(|id| project.beyond.get(id));
        let (name, full) = match (target, beyond) {
            (Some(t), _) => (Some(t.name.clone()), Some(project.full(t))),
            (None, Some((name, full))) => (Some(name.clone()), Some(full.clone())),
            (None, None) => (None, None),
        };
        let landing = match (&line.at, &full) {
            (Some(at), _) if crate::pathtext::has_section(at) => Some(at.clone()),
            (Some(at), Some(full)) => Some(format!("{full}.{at}")),
            _ => None,
        };
        let reached = match (&landing, name) {
            (Some(l), _) => plain_path(l),
            (None, Some(name)) => name,
            (None, None) => text!("exportDocs.reference.deleted"),
        };
        let said = line
            .description
            .clone()
            .filter(|d| !d.is_empty())
            .unwrap_or_else(|| def.said());
        let one = OutLine {
            note: format!("{said} → {reached}"),
            key: def.annotation().map(str::to_string),
            to: target.map(|t| t.id.clone()),
            landing,
            description: line.description.clone(),
            fields: line.fields.clone(),
        };
        if line.from.is_empty() {
            out.head.push(one);
        } else if let Some(field) = field_at(&card.fields, &line.from) {
            out.fields.entry(field.id.clone()).or_default().push(one);
        }
    }
    out
}

/// What a field designates, for a comment. Each is said whole, since where the pieces go differs
/// between languages.
pub fn points_at_notes(field: &Field) -> Vec<String> {
    field
        .points_at
        .iter()
        .map(|p| {
            let target = match &p.discriminated_by {
                Some(here) => match &p.discriminator_field {
                    None => text!("exportDocs.pointsAt.byField", here),
                    Some(there) => text!("exportDocs.pointsAt.byFieldMatched", here, there),
                },
                None => p
                    .schema
                    .as_deref()
                    .map(plain_path)
                    .unwrap_or_else(|| text!("exportDocs.reference.deleted")),
            };
            let when = p
                .when
                .as_ref()
                .map(|w| {
                    text!(
                        "exportDocs.pointsAt.when",
                        field = w.field,
                        equals = w.equals
                    )
                })
                .unwrap_or_default();
            match p.designation.as_str() {
                "digest" => text!("exportDocs.pointsAt.digest", target, when),
                "id" => match p.exists_in.as_deref() {
                    Some(place) => text!(
                        "exportDocs.pointsAt.idIn",
                        target,
                        place = plain_path(place),
                        when
                    ),
                    None => text!("exportDocs.pointsAt.idAny", target, when),
                },
                other => text!(
                    "exportDocs.pointsAt.other",
                    target,
                    designation = other,
                    when
                ),
            }
        })
        .collect()
}

/// A schema whose shape depends on the value of one of its own fields.
pub struct Union<'a> {
    pub discriminator: String,
    pub values: Vec<String>,
    pub shared: Vec<&'a Field>,
    pub by_value: HashMap<String, Vec<&'a Field>>,
}

impl<'a> Union<'a> {
    pub fn of(card: &'a Card, project: &Project) -> Option<Union<'a>> {
        let discriminator = card.discriminator.clone()?;
        let field = card.fields.iter().find(|f| f.name == discriminator)?;
        let mut values: Vec<String> = Vec::new();
        let mut add = |v: &str| {
            if !values.iter().any(|x| x == v) {
                values.push(v.to_string());
            }
        };
        if field.is_ref()
            && let Some(target) = field
                .ref_schema_id
                .as_deref()
                .and_then(|id| project.card(id))
            && target.kind == "enum"
        {
            for v in &target.values {
                add(&v.value);
            }
        }
        for f in &card.fields {
            for v in &f.variants {
                add(v);
            }
        }
        if values.is_empty() {
            return None;
        }
        let mut shared = Vec::new();
        let mut by_value: HashMap<String, Vec<&Field>> =
            values.iter().map(|v| (v.clone(), Vec::new())).collect();
        for f in card.fields.iter().filter(|f| f.name != discriminator) {
            if f.variants.is_empty() {
                shared.push(f);
            } else {
                for v in &f.variants {
                    if let Some(list) = by_value.get_mut(v) {
                        list.push(f);
                    }
                }
            }
        }
        Some(Union {
            discriminator,
            values,
            shared,
            by_value,
        })
    }
}

/// The line at the head of every file written.
pub fn mark(detail: &Detail) -> String {
    text!("exportDocs.file.mark", project = detail.project.name)
}

// MARK: - One file of code

/// What one generated file of code needs to know about the others in its export
/// directory (`CodeFile`): the cards it holds, which references are another file's,
/// what to call them here, and the imports that bring them in.
pub struct CodeFile {
    imports: BTreeMap<String, Vec<(String, String)>>,
    alias_by_id: HashMap<String, String>,
    local: HashSet<String>,
    pub members: HashSet<String>,
    here: String,
    homes: HashMap<String, String>,
}

impl CodeFile {
    pub fn new(
        project: &Project,
        cards: &HashSet<String>,
        here: &str,
        homes: HashMap<String, String>,
    ) -> CodeFile {
        CodeFile {
            imports: BTreeMap::new(),
            alias_by_id: HashMap::new(),
            local: project
                .detail
                .schemas
                .iter()
                .filter(|c| cards.contains(&c.id))
                .map(|c| ident::pascal(&c.name))
                .collect(),
            members: cards.clone(),
            here: here.to_string(),
            homes,
        }
    }

    pub fn contains(&self, card: &Card) -> bool {
        self.members.contains(&card.id)
    }

    /// The import of another file: the path from this file to that one, without `.ts`.
    fn module(&self, file: &str) -> String {
        let stem = file.strip_suffix(".ts").unwrap_or(file);
        let path = relative_plain(&self.here, stem);
        if path.starts_with('.') {
            path
        } else {
            format!("./{path}")
        }
    }

    /// The name a reference to `target` is written with in this file.
    pub fn type_name(&mut self, target: &Card) -> String {
        let name = ident::pascal(&target.name);
        if self.contains(target) {
            return name;
        }
        if let Some(known) = self.alias_by_id.get(&target.id) {
            return known.clone();
        }
        let file = self
            .homes
            .get(&target.id)
            .cloned()
            .unwrap_or_else(|| "_".into());
        let taken = self.local.contains(&name) || self.alias_by_id.values().any(|a| *a == name);
        let alias = if taken {
            format!("{}_{name}", ident::safe(&stem(&file)))
        } else {
            name.clone()
        };
        self.alias_by_id.insert(target.id.clone(), alias.clone());
        self.imports
            .entry(file)
            .or_default()
            .push((name, alias.clone()));
        alias
    }

    /// `import … from` lines, one per file, in a stable order.
    pub fn import_lines(&self, type_only: bool) -> Vec<String> {
        self.imports
            .iter()
            .map(|(file, names)| {
                let names = names
                    .iter()
                    .map(|(n, a)| {
                        if n == a {
                            n.clone()
                        } else {
                            format!("{n} as {a}")
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                format!(
                    "import {}{{ {names} }} from {};",
                    if type_only { "type " } else { "" },
                    ojson::quote(&self.module(file))
                )
            })
            .collect()
    }
}

/// A file's name without its folders and extensions: `src/core.zod.ts` → `core`.
fn stem(file: &str) -> String {
    let name = file.rsplit('/').next().unwrap_or(file);
    name.split('.').next().unwrap_or(name).to_string()
}

/// `to` as a link written in `from` — both relative to where the files go
/// (`DocumentExport.relative`), percent-encoded as a URL's path is.
pub fn relative(from: &str, to: &str) -> String {
    percent_encode(&relative_plain(from, to))
}

/// The same way, as it reads.
fn relative_plain(from: &str, to: &str) -> String {
    let mut here: Vec<&str> = from.split('/').filter(|s| !s.is_empty()).collect();
    here.pop();
    let mut there: Vec<&str> = to.split('/').filter(|s| !s.is_empty()).collect();
    while !here.is_empty() && there.len() > 1 && here[0] == there[0] {
        here.remove(0);
        there.remove(0);
    }
    let mut parts: Vec<&str> = vec![".."; here.len()];
    parts.extend(there);
    parts.join("/")
}

/// What `CharacterSet.urlPathAllowed` keeps, and every other byte as `%XX`.
fn percent_encode(text: &str) -> String {
    const ALLOWED: &str = "!$&'()*+,-./:;=@_~";
    let mut out = String::new();
    for c in text.chars() {
        if c.is_ascii_alphanumeric() || ALLOWED.contains(c) {
            out.push(c);
        } else {
            let mut buf = [0u8; 4];
            for b in c.encode_utf8(&mut buf).bytes() {
                out.push_str(&format!("%{b:02X}"));
            }
        }
    }
    out
}

// MARK: - Where each card goes

/// Each place a card is written by the exports, with the export's name — for the screen and MCP
/// (`card_read`'s `writtenTo`).
pub(crate) fn placed<'r>(card: &Card, resolved: &'r settings::Resolved) -> &'r [settings::Placed] {
    resolved
        .by_card
        .get(&card.id)
        .map(Vec::as_slice)
        .unwrap_or_default()
}

/// A card's file where it is placed: a writing card in a folder is its own file there, and a
/// stackdoc's version is too, by the version's name.
pub(crate) fn file_path(card: &Card, placed: &settings::Placed) -> String {
    let Some(row) = &placed.row else {
        return path_of(card, &placed.setting);
    };
    let path = placed
        .setting
        .path
        .trim_matches(|c: char| c == ' ' || c == '\t')
        .to_string();
    if !path.ends_with('/') {
        return path;
    }
    let name = card
        .versions()
        .into_iter()
        .find(|v| &v.id == row)
        .map_or_else(|| row.clone(), |v| v.name);
    format!("{path}{}.md", folder_name(&name))
}

/// What a card of `kind` is written as, if at all.
fn content(card: &Card, setting: &ExportSetting) -> Option<Content> {
    if card.kind == "instance" {
        return (setting.schema_format().as_deref() == Some(crate::model::JSON_DATA))
            .then_some(Content::Data);
    }
    if card.is_shape() {
        return setting.schema_format().map(Content::Types);
    }
    if card.kind == "flow" || card.kind == "action" {
        return Some(Content::Chart);
    }
    if card.kind == "note"
        && !setting.path.ends_with('/')
        && !setting.path.to_lowercase().ends_with(".md")
    {
        return setting.schema_format().map(Content::Types);
    }
    if card.is_writing() {
        return Some(Content::Document);
    }
    match card.kind.as_str() {
        "svg" => Some(Content::Svg),
        "html" => Some(Content::Html),
        _ => None,
    }
}

/// The extension of the file a card is written to in a folder, when it is a file of its own there:
/// a document's, an SVG's or an HTML's.
fn own_file(card: &Card) -> Option<&'static str> {
    if card.is_writing() {
        return Some("md");
    }
    match card.kind.as_str() {
        "svg" => Some("svg"),
        "html" => Some("html"),
        _ => None,
    }
}

/// A name as a folder holds it.
pub fn folder_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if "/\\:*?\"<>|".contains(c) || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect();
    let cleaned = cleaned
        .trim_matches(|c: char| c == ' ' || c == '\t')
        .to_string();
    if cleaned.is_empty() || cleaned.chars().all(|c| c == '.') {
        "_".into()
    } else {
        cleaned
    }
}

/// The file a setting puts a card in: a document's folder takes the document's name, an SVG's and
/// an HTML's too.
fn path_of(card: &Card, setting: &ExportSetting) -> String {
    let path = setting
        .path
        .trim_matches(|c: char| c == ' ' || c == '\t')
        .to_string();
    match own_file(card) {
        Some(extension) if path.ends_with('/') => {
            format!("{path}{}.{extension}", folder_name(&card.name))
        }
        _ => path,
    }
}

/// What stops an export from being written, with the exports it stops (by their place in the
/// list): writing some exports is stopped only by their own problems.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Problem {
    pub exports: Vec<usize>,
    pub text: String,
}

impl Problem {
    /// A problem of the export at `export`.
    pub fn of(export: usize, text: String) -> Problem {
        Problem {
            exports: vec![export],
            text,
        }
    }

    /// Whether it stops any of `chosen`.
    pub fn stops(&self, chosen: &[usize]) -> bool {
        self.exports.iter().any(|e| chosen.contains(e))
    }

    /// The texts of `problems`.
    pub fn texts(problems: &[Problem]) -> Vec<String> {
        problems.iter().map(|p| p.text.clone()).collect()
    }
}

/// Every file the exports write, in a stable order; the problems found in them with them.
pub fn layout(project: &Project, exports: &[settings::Export]) -> (Vec<Planned>, Vec<Problem>) {
    let resolved = settings::resolve(exports, project);
    let (planned, mut problems) = layout_named(project, &resolved, exports);
    let mut out = resolved.problems;
    out.append(&mut problems);
    (planned, out)
}

/// `layout`, with where each card goes worked out.
pub(crate) fn layout_named(
    project: &Project,
    resolved: &settings::Resolved,
    exports: &[settings::Export],
) -> (Vec<Planned>, Vec<Problem>) {
    let mut files: HashMap<String, Planned> = HashMap::new();
    let mut order: Vec<String> = Vec::new();
    let mut problems = Vec::new();
    for card in &project.detail.schemas {
        for placed in placed(card, resolved) {
            let setting = &placed.setting;
            let Some(content) = content(card, setting) else {
                continue;
            };
            if let Some(problem) = ExportSetting::path_problem(&setting.path) {
                problems.push(Problem::of(
                    placed.export,
                    format!("{}: {problem}", project.full(card)),
                ));
                continue;
            }
            let path = file_path(card, placed);
            // A file is its export's: two exports write what they write apart, and where they
            // meet is known when they are written (`tools::files`).
            let key = format!("{}\u{1}{path}", exports[placed.export].id);
            let name = &exports[placed.export].name;
            if let Some(file) = files.get_mut(&key) {
                let notes_only = |ids: &[String]| {
                    ids.iter()
                        .all(|id| project.card(id).is_some_and(|c| c.kind == "note"))
                };
                if file.content == Content::Chart
                    && content == Content::Document
                    && card.kind == "note"
                {
                    // A note on the page: as the page is.
                } else if file.content == Content::Document
                    && content == Content::Chart
                    && notes_only(&file.cards)
                {
                    file.content = Content::Chart;
                } else if file.content != content {
                    problems.push(Problem::of(
                        placed.export,
                        text!(
                            "export.layout.twoContents",
                            name,
                            path,
                            first = file.content.label(),
                            second = content.label()
                        ),
                    ));
                    continue;
                }
                // A version of a stackdoc is a file of its own: neither it nor what is in it
                // shares one.
                if file.row.is_some() || placed.row.is_some() {
                    problems.push(Problem::of(
                        placed.export,
                        text!("export.layout.twoRows", name, path),
                    ));
                    continue;
                }
                if !file.cards.contains(&card.id) {
                    file.cards.push(card.id.clone());
                }
            } else {
                files.insert(
                    key.clone(),
                    Planned {
                        export: placed.export,
                        path,
                        content,
                        cards: vec![card.id.clone()],
                        row: placed.row.clone(),
                        section: None,
                    },
                );
                order.push(key);
            }
        }
    }
    // Each ui section's page, its parts as its cards.
    for page in &resolved.pages {
        let key = format!("{}\u{1}{}", exports[page.export].id, page.path);
        let name = &exports[page.export].name;
        if let Some(file) = files.get(&key) {
            problems.push(Problem::of(
                page.export,
                text!(
                    "export.layout.twoContents",
                    name,
                    path = page.path,
                    first = file.content.label(),
                    second = Content::Mock.label()
                ),
            ));
            continue;
        }
        let cards = project
            .directory
            .members(&page.section)
            .iter()
            .map(|&i| project.nth(i))
            .filter(|c| matches!(c.kind.as_str(), "html" | "svg" | crate::kinds::FRAME))
            .map(|c| c.id.clone())
            .collect();
        files.insert(
            key.clone(),
            Planned {
                export: page.export,
                path: page.path.clone(),
                content: Content::Mock,
                cards,
                row: None,
                section: Some(page.section.clone()),
            },
        );
        order.push(key);
    }
    let planned: Vec<Planned> = order.iter().filter_map(|k| files.get(k).cloned()).collect();
    // Files of one export whose paths differ by ASCII case alone are one file where case is not
    // told apart (a Mac's): none of them is written.
    let mut by_fold: HashMap<(usize, String), Vec<&Planned>> = HashMap::new();
    for file in &planned {
        by_fold
            .entry((file.export, crate::pathtext::fold(&file.path)))
            .or_default()
            .push(file);
    }
    let mut clashing: Vec<&Vec<&Planned>> = by_fold.values().filter(|f| f.len() > 1).collect();
    clashing.sort_by_key(|f| (f[0].export, f[0].path.clone()));
    for same in clashing {
        let cards: Vec<String> = same
            .iter()
            .flat_map(|f| f.cards.iter())
            .filter_map(|id| project.card(id))
            .map(|c| project.full(c))
            .collect();
        problems.push(Problem::of(
            same[0].export,
            text!(
                "export.layout.sameFileName",
                name = exports[same[0].export].name,
                paths = crate::text::List::of(same.iter().map(|f| f.path.as_str())),
                cards = crate::text::List::of(&cards)
            ),
        ));
    }
    for file in &planned {
        if matches!(
            file.content,
            Content::Document | Content::Svg | Content::Html
        ) && file.cards.len() > 1
        {
            let names: Vec<String> = file
                .cards
                .iter()
                .filter_map(|id| project.card(id))
                .map(|c| project.full(c))
                .collect();
            problems.push(Problem::of(
                file.export,
                text!(
                    "export.layout.manyDocuments",
                    name = exports[file.export].name,
                    path = file.path,
                    count = file.cards.len(),
                    cards = crate::text::List::of(&names)
                ),
            ));
        }
    }
    (planned, problems)
}

/// The cards a schema holds by `ref`, at any depth of its fields.
fn embedded(card: &Card) -> Vec<String> {
    fn walk(fields: &[Field], out: &mut Vec<String>) {
        for f in fields {
            if f.is_ref()
                && let Some(id) = &f.ref_schema_id
            {
                out.push(id.clone());
            }
            walk(&f.children, out);
        }
    }
    let mut out = Vec::new();
    walk(&card.fields, &mut out);
    out
}

/// What was written: file contents by the id of the export that writes it and the path, and the
/// problems.
pub struct Built {
    pub files: BTreeMap<String, BTreeMap<String, String>>,
    pub problems: Vec<Problem>,
    /// Each file, the export that writes it and the cards in it.
    pub planned: Vec<Planned>,
}

/// Every file the exports write, and every problem found: a ui section's page laid out on the
/// project's grid (`layout`).
pub fn build(
    detail: &Detail,
    exports: &[settings::Export],
    grid: &crate::tools::grid::Layout,
) -> Built {
    // Whoever writes it out, in whatever language: the same files, in the project's language.
    detail
        .project
        .in_export_language(|| build_in_language(detail, exports, grid))
}

fn build_in_language(
    detail: &Detail,
    exports: &[settings::Export],
    grid: &crate::tools::grid::Layout,
) -> Built {
    let project = Project::new(detail);
    let (planned, found) = layout(&project, exports);
    let mut problems = found;
    let named = |file: &Planned| format!("{}/{}", exports[file.export].name, file.path);
    let mark = mark(detail);
    let mut out: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();

    // Code: only what is a shape is a type; a note written there is a comment.
    let mut types = detail.clone();
    types.schemas.retain(|c| c.is_shape() || c.kind == "note");
    let mut types_project = Project::new(&types);
    // What the shapes' lines reach that is not a shape, named from the whole project.
    for card in &types.schemas {
        for to in card.connections.iter().filter_map(|c| c.to.as_deref()) {
            if let Some(reached) = project.card(to) {
                types_project.beyond.insert(
                    to.to_string(),
                    (reached.name.clone(), project.full(reached)),
                );
            }
        }
    }
    for file in &planned {
        let Content::Types(format) = &file.content else {
            continue;
        };
        // Where each card written in this format sits in this export.
        let mut homes: HashMap<String, String> = HashMap::new();
        for other in planned
            .iter()
            .filter(|o| o.export == file.export && o.content == file.content)
        {
            for id in &other.cards {
                homes
                    .entry(id.clone())
                    .or_insert_with(|| other.path.clone());
            }
        }
        for id in &file.cards {
            homes.insert(id.clone(), file.path.clone());
        }
        // What its cards hold that no file of the format here does comes in with them.
        let mut members: HashSet<String> = file.cards.iter().cloned().collect();
        let mut queue = file.cards.clone();
        while let Some(id) = queue.pop() {
            for target in project.card(&id).map(embedded).unwrap_or_default() {
                if !homes.contains_key(&target) && members.insert(target.clone()) {
                    queue.push(target);
                }
            }
        }
        for id in &members {
            homes.insert(id.clone(), file.path.clone());
        }
        let held: Vec<&Card> = detail
            .schemas
            .iter()
            .map(|c| &**c)
            .filter(|c| members.contains(&c.id) && c.is_shape())
            .collect();
        let notes: Vec<&Card> = detail
            .schemas
            .iter()
            .map(|c| &**c)
            .filter(|c| members.contains(&c.id) && c.kind == "note")
            .collect();
        let mut by_name: BTreeMap<String, Vec<&Card>> = BTreeMap::new();
        for c in &held {
            by_name.entry(ident::pascal(&c.name)).or_default().push(c);
        }
        if let Some((name, same)) = by_name.iter().find(|(_, v)| v.len() > 1) {
            problems.push(Problem::of(
                file.export,
                text!(
                    "export.layout.sameSchema",
                    file = named(file),
                    name,
                    count = same.len(),
                    cards = crate::text::List::of(same.iter().map(|c| project.full(c)))
                ),
            ));
            continue;
        }
        let text = match format.as_str() {
            "typescript" | "zod" => {
                let mut unit = CodeFile::new(&types_project, &members, &file.path, homes.clone());
                let content = if format == "zod" {
                    zod::build(&types_project, &mut unit)
                } else {
                    typescript::build(&types_project, &mut unit)
                };
                format!("// {}\n{content}", ident::line_comment(&mark))
            }
            _ => {
                let mut defs = Map::new();
                for card in &held {
                    let by_id = &project;
                    let refer = |target: &str| -> Option<String> {
                        let node = by_id.card(target)?;
                        let there = homes.get(target)?;
                        if !node.is_shape() {
                            return None;
                        }
                        let anchor = format!("#/$defs/{}", ident::pointer_token(&node.name));
                        Some(if *there == file.path {
                            anchor
                        } else {
                            relative(&file.path, there) + &anchor
                        })
                    };
                    let placement = jsonschema::Placement {
                        id: file.path.clone(),
                        refer: &refer,
                    };
                    if let Value::Object(mut def) =
                        jsonschema::build(card, &types_project, Some(&placement))
                    {
                        def.shift_remove("$schema");
                        def.shift_remove("$id");
                        // Its data cards, from any section, as examples by their names: their
                        // values its fields have. One the check finds wanting (a required field
                        // with no value, a value its field does not take) is no example of it.
                        let mut data: Vec<&Card> = project
                            .detail
                            .schemas
                            .iter()
                            .map(|c| &**c)
                            .filter(|c| c.kind == crate::kinds::INSTANCE)
                            .filter(|c| {
                                crate::tools::instances::shape(&project, c)
                                    .is_some_and(|s| s.id == card.id)
                            })
                            .filter(|c| crate::tools::instances::problems(&project, c).is_empty())
                            .collect();
                        data.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));
                        let examples: Vec<Value> = data
                            .into_iter()
                            .map(|c| {
                                crate::tools::instances::without_strays(
                                    c.instance_data(),
                                    &crate::tools::instances::strays(&project, c),
                                )
                            })
                            .collect();
                        if !examples.is_empty() {
                            def.insert("examples".into(), Value::Array(examples));
                        }
                        defs.insert(card.name.clone(), Value::Object(def));
                    }
                }
                let mut head = Map::new();
                head.insert(
                    "$schema".into(),
                    "https://json-schema.org/draft/2020-12/schema".into(),
                );
                head.insert("$id".into(), file.path.clone().into());
                head.insert("$comment".into(), mark.clone().into());
                // JSON has no comments: the notes written here sit apart, by name.
                if !notes.is_empty() {
                    head.insert(
                        "x-notes".into(),
                        Value::Object(
                            notes
                                .iter()
                                .map(|n| {
                                    (
                                        n.name.clone(),
                                        Value::String(note_text(n, &project.work_dir_names())),
                                    )
                                })
                                .collect(),
                        ),
                    );
                }
                head.insert("$defs".into(), Value::Object(defs));
                ojson::pretty(&Value::Object(head)) + "\n"
            }
        };
        out.entry(exports[file.export].id.clone())
            .or_default()
            .insert(file.path.clone(), text);
    }

    // The data cards, in the project's order, as one JSON: a list of their data.
    for file in planned.iter().filter(|f| f.content == Content::Data) {
        let cards: Vec<&Card> = file
            .cards
            .iter()
            .filter_map(|id| project.card(id))
            .collect();
        match data::build(&project, &cards) {
            Ok(text) => {
                out.entry(exports[file.export].id.clone())
                    .or_default()
                    .insert(file.path.clone(), text);
            }
            Err(found) => {
                for problem in found {
                    problems.push(Problem::of(
                        file.export,
                        format!("{}: {problem}", named(file)),
                    ));
                }
            }
        }
    }

    // Charts, notes and documents, linking to what is written in the same export.
    let mut used: Vec<usize> = planned.iter().map(|f| f.export).collect();
    used.sort();
    used.dedup();
    for at in used {
        let dir = &exports[at].id;
        let mut paths: HashMap<String, String> = HashMap::new();
        // A link to a stackdoc goes to the file of it whole, not of one version.
        // A ui section's page, by the section's id.
        let pages: HashMap<String, String> = planned
            .iter()
            .filter(|f| f.export == at)
            .filter_map(|f| Some((f.section.clone()?, f.path.clone())))
            .collect();
        for file in planned.iter().filter(|f| {
            f.export == at
                && f.row.is_none()
                && matches!(
                    f.content,
                    Content::Chart | Content::Document | Content::Svg | Content::Html
                )
        }) {
            for id in &file.cards {
                paths.entry(id.clone()).or_insert_with(|| file.path.clone());
            }
        }
        for file in planned.iter().filter(|f| f.export == at) {
            match file.content {
                Content::Chart => {
                    let cards: Vec<&Card> = file
                        .cards
                        .iter()
                        .filter_map(|id| project.card(id))
                        .collect();
                    let page = charts::chart_page(&cards, &project, &file.path, &paths, &mark);
                    out.entry(dir.clone())
                        .or_default()
                        .insert(file.path.clone(), page);
                }
                Content::Document => {
                    // Two notes in one file is a problem, said above: neither is written.
                    if file.cards.len() != 1 {
                        continue;
                    }
                    let id = &file.cards[0];
                    let Some(note) = project.card(id).filter(|c| c.is_writing()) else {
                        continue;
                    };
                    let mut here = paths.clone();
                    here.insert(id.clone(), file.path.clone());
                    // A section's page, by the section's id: what a `[[Section:]]` links to.
                    for (section, page) in &pages {
                        here.entry(section.clone()).or_insert_with(|| page.clone());
                    }
                    let text = match &file.row {
                        Some(row) => {
                            let Some(name) = note
                                .versions()
                                .into_iter()
                                .find(|v| &v.id == row)
                                .map(|v| v.name)
                            else {
                                continue;
                            };
                            match documents::version_file(
                                note, &name, &project, &file.path, &here, &mark,
                            ) {
                                Some(text) => text,
                                None => continue,
                            }
                        }
                        None => documents::file(note, &project, &file.path, &here, &mark),
                    };
                    out.entry(dir.clone())
                        .or_default()
                        .insert(file.path.clone(), text);
                }
                // A picture or a page: one card a file (two are a problem, said above).
                Content::Svg | Content::Html => {
                    let [id] = file.cards.as_slice() else {
                        continue;
                    };
                    let Some(card) = project.card(id) else {
                        continue;
                    };
                    let text = if file.content == Content::Svg {
                        pages::svg_file(card)
                    } else {
                        pages::html_file(card, &mark)
                    };
                    out.entry(dir.clone())
                        .or_default()
                        .insert(file.path.clone(), text);
                }
                Content::Mock => {
                    let Some(section) = &file.section else {
                        continue;
                    };
                    let page = mock::assemble(&project, grid, section);
                    let html = mock_links(&project, &page.html, &file.path, &paths, &pages);
                    let html = html.replacen(
                        "<!doctype html>\n",
                        &format!("<!doctype html>\n{}\n", charts::marker(&mark)),
                        1,
                    );
                    out.entry(dir.clone())
                        .or_default()
                        .insert(file.path.clone(), html);
                }
                Content::Types(_) | Content::Data => {}
            }
        }
    }
    Built {
        files: out,
        problems,
        planned,
    }
}

/// A ui section's page with its links to `sb-nav:<path>` as the files the same export writes:
/// a ui section's to its page, a card's to the file it is written in (`paths`); a path to neither
/// keeps its link as it is.
fn mock_links(
    project: &Project,
    html: &str,
    file: &str,
    paths: &HashMap<String, String>,
    pages: &HashMap<String, String>,
) -> String {
    mock::rewrite_nav(html, |path| {
        let home = &project.directory.home;
        let to = match project.directory.section_named(path, None) {
            Some(section) => pages.get(&section)?,
            None => paths.get(&project.find(path, home)?.id)?,
        };
        Some(relative(file, to))
    })
}
