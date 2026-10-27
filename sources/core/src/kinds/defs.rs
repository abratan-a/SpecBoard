//! The kinds of section and of line, as definitions beside the kinds of card.
//!
//! A section's kind says which cards it holds (`holds`), which kinds of section are made
//! inside it (`sections`), how it opens (`opens`), which trees it may be put directly under
//! the root of (`trees`) and whether it may have an export default (`export`). A line's kind says what
//! it means (`base`: the field's type becomes the other card, the value names it, or only
//! that they are joined), how it is drawn (`look`), what it leaves and what it reaches
//! (`from`, `to`), how many one place may have (`max`), the fields each line holds
//! (`fields`, written as a card's are) and how it is written out (`export`).
//!
//! The built-in ones are the crate's data (`builtin.json`'s `sections` and `lines`); a project
//! keeps its own beside them, one set for the whole project as its writing rules are
//! (`projects.kinds`: `{"cards": […], "sections": […], "lines": […], "project_tree": {…}}`), read by
//! `Defs::of_project`. What is directly under each root is the project's too (`project_tree`): the sections of main and every branch
//! (`main_sections`) and of the board (`board_sections`), each a kind and a name, in order.
//! The format is written out in `specboard://SpecBoard/Docs/Design:CardKinds`.

use super::{Field, FieldType, Kinds, read_fields};
use serde_json::{Map, Value};
use std::sync::OnceLock;

// MARK: - Lines

/// What a line means.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Base {
    /// The field's type is the other card (a `ref` before).
    Embed,
    /// The value names the other card; its type stays.
    Designate,
    /// Only that they are joined; no type is touched.
    Link,
}

impl Base {
    const ALL: [(&'static str, Base); 3] = [
        ("embed", Base::Embed),
        ("designate", Base::Designate),
        ("link", Base::Link),
    ];

    fn parse(name: &str) -> Option<Base> {
        Self::ALL.iter().find(|(n, _)| *n == name).map(|(_, b)| *b)
    }

    pub fn name(self) -> &'static str {
        Self::ALL
            .iter()
            .find(|(_, b)| *b == self)
            .map(|(n, _)| *n)
            .unwrap_or_default()
    }
}

/// Where in a card a line leaves or reaches: its head, or a row inside it.
/// A record's field (entity・value) is a row too, of its kind's kind of row `fields`; whether a
/// line touches what is written out is its `base` and `export`, not where it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum At {
    /// The card's head.
    Card,
    /// A row inside it: a record's field, a formatdoc's item, a process node's step, a row of
    /// a project's own kind (and a field of its card).
    Row,
    /// A function or a type inside a working directory's file: made of the file each time, so
    /// only a line whose end names files lands at one.
    Symbol,
}

impl At {
    const ALL: [(&'static str, At); 3] =
        [("card", At::Card), ("row", At::Row), ("symbol", At::Symbol)];

    fn parse(name: &str) -> Option<At> {
        Self::ALL.iter().find(|(n, _)| *n == name).map(|(_, a)| *a)
    }

    pub fn name(self) -> &'static str {
        Self::ALL
            .iter()
            .find(|(_, a)| *a == self)
            .map(|(n, _)| *n)
            .unwrap_or_default()
    }
}

/// The kind of row a record's (entity・value) fields are: a row a person calls 欄.
pub const FIELD_ROWS: &str = "fields";

/// Whether a card of `kind` is a record (entity・value): its rows are fields. A project's own
/// kind is never one (it extends `base` or one of the project's, and no kind of row of its own
/// may be called `fields`).
pub fn is_record(kind: &str) -> bool {
    super::builtin()
        .get(kind)
        .is_some_and(|k| !k.is_abstract && k.row(FIELD_ROWS).is_some())
}

/// Every kind of row a kind of card has, at every depth.
fn row_names(kind: &super::Kind) -> Vec<String> {
    fn walk(rows: &[super::Row], out: &mut Vec<String>) {
        for row in rows {
            if !out.contains(&row.name) {
                out.push(row.name.clone());
            }
            let inner: Vec<super::Row> = row
                .children
                .iter()
                .filter_map(|c| match c {
                    super::Child::Row(r) => Some(r.clone()),
                    super::Child::Again { .. } => None,
                })
                .collect();
            walk(&inner, out);
        }
    }
    let mut out = Vec::new();
    walk(&kind.rows, &mut out);
    out
}

/// A place in a card a line leaves or lands at, found: its head, or a row with the kind of
/// row it is one of (`fields` for a record's field, `rows` for a formatdoc's item, `steps`
/// for a process node's step, the project's own name for a row of a project's own kind); none for
/// a field of a project's own kind's card itself (and a goal's row of before).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinePlace {
    pub at: At,
    pub row: Option<String>,
}

impl LinePlace {
    pub const HEAD: LinePlace = LinePlace {
        at: At::Card,
        row: None,
    };

    pub fn row(kind: Option<&str>) -> LinePlace {
        LinePlace {
            at: At::Row,
            row: kind.map(str::to_string),
        }
    }

    /// As a person calls it: 頭, 欄 for a record's field (and a field of a project's own kind's
    /// card), 行 otherwise — with its kind of row when `named` (what a line that names its
    /// kinds of row is told).
    pub fn said(&self, named: bool) -> String {
        match (self.at, self.row.as_deref()) {
            (At::Card, _) => text!("kindDefs.place.head"),
            (At::Symbol, _) => text!("kindDefs.place.symbol"),
            (At::Row, Some(FIELD_ROWS) | None) => text!("kindDefs.place.field"),
            (At::Row, Some(row)) if named => text!("kindDefs.place.rowNamed", row),
            (At::Row, Some(_)) => text!("kindDefs.place.row"),
        }
    }
}

/// One end of a line: the kinds of card (`*` for any), where in them, and — for a kind of
/// card with more than one kind of row — which kinds of row.
#[derive(Debug, Clone, PartialEq)]
pub struct End {
    pub kinds: Vec<String>,
    pub at: Vec<At>,
    /// The kinds of row it is at, when `at` has `row`; empty for any row (and a field of a
    /// project's own kind's card, which is a row of no kind).
    pub rows: Vec<String>,
}

impl End {
    /// Whether a card of `kind`, at `place`, is this end.
    pub fn takes(&self, kind: &str, place: &LinePlace) -> bool {
        self.takes_kind(kind) && self.takes_place(place)
    }

    pub fn takes_kind(&self, kind: &str) -> bool {
        self.kinds.iter().any(|k| k == "*" || k == kind)
    }

    /// Whether it names `kind` itself, not as any kind (`*`): a working directory's file is
    /// reached only by a kind of line that names it.
    pub fn names_kind(&self, kind: &str) -> bool {
        self.kinds.iter().any(|k| k == kind)
    }

    /// Whether `place` is where in a card this end is: its head, or a row (of its kinds of
    /// row, when it names them).
    pub fn takes_place(&self, place: &LinePlace) -> bool {
        self.at.contains(&place.at)
            && (place.at != At::Row
                || self.rows.is_empty()
                || place.row.as_ref().is_some_and(|r| self.rows.contains(r)))
    }

    /// As a person reads it: `entity・valueの欄`, `どのカードの頭・欄・行`, `riskの行（steps）`.
    /// A record's rows are called 欄, the others 行.
    pub fn said(&self) -> String {
        let any = self.kinds.iter().any(|k| k == "*");
        let kinds = if any {
            text!("kindDefs.end.anyCard")
        } else {
            crate::text::list(&self.kinds)
        };
        let rows = if !self.rows.is_empty() {
            let named: Vec<&str> = self
                .rows
                .iter()
                .map(String::as_str)
                .filter(|r| *r != FIELD_ROWS)
                .collect();
            let mut said = Vec::new();
            if self.rows.iter().any(|r| r == FIELD_ROWS) {
                said.push(text!("kindDefs.place.field"));
            }
            if !named.is_empty() {
                said.push(text!(
                    "kindDefs.place.rowNamed",
                    row = crate::text::list(&named)
                ));
            }
            crate::text::list(&said)
        } else {
            let records = self.kinds.iter().filter(|k| is_record(k)).count();
            if !any && records == self.kinds.len() {
                text!("kindDefs.place.field")
            } else if any || records > 0 {
                crate::text::list(&[text!("kindDefs.place.field"), text!("kindDefs.place.row")])
            } else {
                text!("kindDefs.place.row")
            }
        };
        let at: Vec<String> = self
            .at
            .iter()
            .map(|a| match a {
                At::Card => text!("kindDefs.place.head"),
                At::Row => rows.clone(),
                At::Symbol => text!("kindDefs.place.symbol"),
            })
            .collect();
        text!("kindDefs.end.of", kinds, at = crate::text::list(&at))
    }

    /// Whether it names its kinds of row.
    pub fn names_rows(&self) -> bool {
        !self.rows.is_empty()
    }
}

/// How a line is written out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportAs {
    /// Not at all (drawn on the diagrams still).
    None,
    /// In the comments: TypeScript's and Zod's JSDoc, JSON Schema's description.
    Comment,
    /// The comments, and an `x-` annotation of JSON Schema named by `key`.
    Annotation,
    /// In a data card's JSON (the export format `json-data`): the form the line reaches as a JSON
    /// Schema, in the place the line leaves — a field's value, or the element's `key` (`schema` when
    /// none) when it leaves an element of an array.
    Schema,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Export {
    pub as_: ExportAs,
    /// The annotation's name (`x-…`), for `Annotation`; the key an element holds the schema under, for
    /// `Schema`.
    pub key: Option<String>,
    /// The word the annotation says it designates by (`x-pointsAt`'s `as`); the line's
    /// name when none.
    pub word: Option<String>,
}

/// How a line is drawn.
#[derive(Debug, Clone, PartialEq)]
pub struct Look {
    pub color: String,
    pub dash: String,
    pub begin: String,
    pub end: String,
}

/// The colours a line is drawn in (the screen's names, `EdgeInk`).
pub const COLORS: [&str; 7] = ["amber", "blue", "green", "teal", "violet", "red", "grey"];
pub const DASHES: [&str; 3] = ["solid", "dashed", "dotted"];
pub const CAPS: [&str; 5] = [
    "none",
    "arrow",
    "triangle",
    "open-diamond",
    "filled-diamond",
];

/// One kind of line.
#[derive(Debug, Clone)]
pub struct LineDef {
    pub name: String,
    pub label: String,
    pub description: String,
    pub base: Base,
    pub look: Look,
    pub from: Vec<End>,
    pub to: End,
    /// How many one place (the same `from`) may have.
    pub max: Option<usize>,
    /// Each line's own fields.
    pub fields: Vec<Field>,
    /// How it is written out; none for an embedding (its type is what is written).
    pub export: Option<Export>,
    /// The card kinds that hold it themselves; none for a line any card may have.
    pub owner: Vec<String>,
    /// It reaches only a card inside the same container as the card it leaves (a flow's
    /// node to another of the flow's).
    pub siblings: bool,
    /// Picked from a card's text (`[[ ]]`), not written as connections.
    pub from_text: bool,
    pub builtin: bool,
    /// Drawn from the start (`visible`, true unless it says false): a kind
    /// that says false is hidden on every diagram until shown, the cards of other sections it
    /// alone reaches with it, and left out of MCP's diagrams unless asked for.
    pub visible: bool,
}

impl LineDef {
    /// What a person reads it by in the language of the call: a built-in one's words in that
    /// language (the definitions a project was read with may be in another), a project's own as it
    /// wrote them; its name when it has no label.
    pub fn said(&self) -> String {
        let label = if self.builtin {
            builtin_defs()
                .line(&self.name)
                .map_or_else(|| self.label.clone(), |l| l.label.clone())
        } else {
            self.label.clone()
        };
        if label.is_empty() {
            self.name.clone()
        } else {
            label
        }
    }

    pub fn field(&self, name: &str) -> Option<&Field> {
        self.fields.iter().find(|f| f.name == name)
    }

    /// The annotation it is written out in, when it is one.
    pub fn annotation(&self) -> Option<&str> {
        self.export
            .as_ref()
            .filter(|e| e.as_ == ExportAs::Annotation)
            .and_then(|e| e.key.as_deref())
    }

    /// The word it designates by, in what is written out.
    pub fn word(&self) -> &str {
        self.export
            .as_ref()
            .and_then(|e| e.word.as_deref())
            .unwrap_or(&self.name)
    }

    /// Whether it leaves a card of `kind` at `place`.
    pub fn leaves(&self, kind: &str, place: &LinePlace) -> bool {
        self.from.iter().any(|e| e.takes(kind, place))
    }

    /// Whether where it may leave names kinds of row.
    pub fn from_names_rows(&self) -> bool {
        self.from.iter().any(End::names_rows)
    }

    /// Where it may leave, as a person reads it.
    pub fn from_said(&self) -> String {
        self.from
            .iter()
            .map(End::said)
            .collect::<Vec<_>>()
            .join(" / ")
    }
}

// MARK: - Sections

/// A tree whose root a section may be put directly under (`trees` of a kind of section).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tree {
    /// main's and every branch's (a branch is made from main, its top the same).
    Main,
    /// The board's.
    Board,
}

impl Tree {
    pub const ALL: [Tree; 2] = [Tree::Main, Tree::Board];

    pub fn name(self) -> &'static str {
        match self {
            Tree::Main => "main",
            Tree::Board => "board",
        }
    }

    fn parse(name: &str) -> Option<Tree> {
        Self::ALL.into_iter().find(|t| t.name() == name)
    }

    /// The key of `project_tree` that lists its sections.
    pub fn key(self) -> &'static str {
        match self {
            Tree::Main => "main_sections",
            Tree::Board => "board_sections",
        }
    }

    /// As people call where its sections are.
    pub fn said(self) -> String {
        match self {
            Tree::Main => text!("kindDefs.tree.main"),
            Tree::Board => text!("kindDefs.tree.board"),
        }
    }
}

/// One section directly under a root, as the project says it (`project_tree`): its kind and its
/// name — and for a working directory's (`workdir`), the working directory it is, by id
/// (`directory`: the core writes it, a call may leave it out and is matched by name).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TopSection {
    pub kind: String,
    pub name: String,
    pub directory: Option<String>,
}

impl TopSection {
    pub fn new(kind: &str, name: &str) -> TopSection {
        TopSection {
            kind: kind.to_string(),
            name: name.to_string(),
            directory: None,
        }
    }

    pub fn json(&self) -> Value {
        let mut out = Map::new();
        out.insert("type".into(), self.kind.clone().into());
        out.insert("name".into(), self.name.clone().into());
        if let Some(directory) = &self.directory {
            out.insert("directory".into(), directory.clone().into());
        }
        Value::Object(out)
    }
}

/// The kind of section a working directory is, directly under main's root (`project_tree`).
pub const WORKDIR: &str = "workdir";

/// The kind of section a directory read inside a working directory is: the core's alone.
pub const DIRECTORY: &str = "directory";

/// What is directly under each root of the project (`project_tree`): main's and
/// every branch's sections, and the board's, each in its order. Fixed there: made, taken away,
/// put in order and renamed only by writing it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectTree {
    pub main: Vec<TopSection>,
    pub board: Vec<TopSection>,
}

impl Default for ProjectTree {
    /// A new project's: Docs・Schemas・Definitions・Charts・Free under main, Goals under the board.
    fn default() -> ProjectTree {
        ProjectTree {
            main: vec![
                TopSection::new("document", "Docs"),
                TopSection::new("schema", "Schemas"),
                TopSection::new("definition", "Definitions"),
                TopSection::new("chart", "Charts"),
                TopSection::new("free", "Free"),
            ],
            board: vec![TopSection::new("goals", "Goals")],
        }
    }
}

impl ProjectTree {
    pub fn of(&self, tree: Tree) -> &[TopSection] {
        match tree {
            Tree::Main => &self.main,
            Tree::Board => &self.board,
        }
    }

    pub fn json(&self) -> Value {
        let mut out = Map::new();
        for tree in Tree::ALL {
            out.insert(
                tree.key().into(),
                Value::Array(self.of(tree).iter().map(TopSection::json).collect()),
            );
        }
        Value::Object(out)
    }

    /// Where a section of this name sits directly under a root, in either tree, by its name in
    /// any case: its tree and its place there.
    pub fn place_of(&self, name: &str) -> Option<(Tree, usize)> {
        Tree::ALL.into_iter().find_map(|tree| {
            self.of(tree)
                .iter()
                .position(|t| crate::pathtext::same(&t.name, name))
                .map(|at| (tree, at))
        })
    }
}

/// Whether a section's name is the app's (`_evacuation_`, `_section_ghosts_`): one between `_`
/// and `_`, not a section a person names.
pub fn reserved_name(name: &str) -> bool {
    name.chars().count() >= 2 && name.starts_with('_') && name.ends_with('_')
}

/// The section a section taken out of `project_tree` with something in it goes into, last under
/// its root: of the kind free, made when one is first needed.
pub const EVACUATION: &str = "_evacuation_";

/// One kind of section.
#[derive(Debug, Clone)]
pub struct SectionDef {
    pub name: String,
    pub label: String,
    pub description: String,
    /// The card kinds it holds (`*` for any). A card ghost goes anywhere.
    pub holds: Vec<String>,
    /// The kinds of section made inside it; the first when none is said.
    pub sections: Vec<String>,
    /// How it opens (`OPENS`): the relations panel.
    pub opens: String,
    /// The trees whose root it may be put directly under in `project_tree`: main (and every
    /// branch), the board. A project's own kind that says none, both.
    pub trees: Vec<Tree>,
    pub builtin: bool,
    /// The mark the nav panel puts before a section of this kind (an SF Symbols name) and its
    /// colour (`#RRGGBB`), as `icon: {name, color}` says: a built-in kind's from `builtin.json` — a project writes over it
    /// with `icons`, by the kind's name — and a kind the project defines' from its definition. None
    /// of either: the app draws a folder in grey.
    pub icon: Option<String>,
    pub color: Option<String>,
    /// What a section of this kind can be made to do (`section_action`): the core's, named by a
    /// built-in kind alone.
    pub actions: Vec<String>,
}

impl SectionDef {
    /// Whether a card of `kind` may be put in it: a kind it names, or any (`*`) but one
    /// put only inside a container (a flow's node), one the app alone draws (`system_only`) and
    /// one that has a meaning only where it is named (`named_only`).
    /// A card ghost goes anywhere.
    pub fn holds_kind(&self, kind: &str) -> bool {
        kind == crate::detail::GHOST
            || self.holds.iter().any(|k| k == kind)
            || (self.holds.iter().any(|k| k == "*")
                && !super::inner_only(kind)
                && !super::system_only(kind)
                && !super::named_only(kind))
    }
}

/// How a section opens: the relations panel. A ui section opens there too, its parts drawn on its
/// map as its mock draws them.
pub const OPENS: [&str; 1] = ["relations"];

// MARK: - The set

/// The kinds of card, of section and of line a project has: the built-in ones, then its own.
#[derive(Debug, Clone)]
pub struct Defs {
    pub sections: Vec<SectionDef>,
    pub lines: Vec<LineDef>,
    /// What is directly under each root (`project_tree`): the project's, or a new project's when it
    /// says none.
    pub tree: ProjectTree,
    /// The kinds of card with the project's own after the built-in ones; none when the project
    /// has none (the built-in ones alone, `cards`).
    own_cards: Option<Kinds>,
    /// What a flow's `via` may be, one list for a begin, a process node's rows and a run: the
    /// built-in values, then the project's own (`projects.kinds.vias`).
    pub vias: Vec<String>,
}

/// The kinds of section a card may be put in before the definitions, read still: the
/// settings of before and the board's name for its sections then.
pub const LEGACY_SECTIONS: [&str; 3] = ["export", "rules", "board"];

impl Defs {
    /// The kinds of card: the built-in ones, then the project's own.
    pub fn cards(&self) -> &Kinds {
        self.own_cards.as_ref().unwrap_or_else(|| super::builtin())
    }

    pub fn section(&self, name: &str) -> Option<&SectionDef> {
        self.sections.iter().find(|s| s.name == name)
    }

    pub fn line(&self, name: &str) -> Option<&LineDef> {
        self.lines.iter().find(|l| l.name == name)
    }

    /// Where a section directly under a root sits among them: its place in `project_tree` (by its
    /// name in any case), then anything else there, `_evacuation_` last.
    pub fn root_place(&self, name: &str) -> i64 {
        match self.tree.place_of(name) {
            Some((_, at)) => at as i64,
            None if name == EVACUATION => 1_000_001,
            None => 1_000_000,
        }
    }

    /// The kinds of section the project has a place for: those directly under a root
    /// (`project_tree`, main's then the board's), then those made inside them (`sections`), each
    /// once, in that order.
    pub fn reachable(&self) -> Vec<&SectionDef> {
        let mut out: Vec<&SectionDef> = Vec::new();
        let mut queue: Vec<&str> = Tree::ALL
            .into_iter()
            .flat_map(|t| self.tree.of(t).iter().map(|s| s.kind.as_str()))
            .collect();
        let mut at = 0;
        while at < queue.len() {
            if let Some(def) = self.section(queue[at])
                && !out.iter().any(|d| d.name == def.name)
            {
                out.push(def);
                queue.extend(def.sections.iter().map(String::as_str));
            }
            at += 1;
        }
        out
    }

    /// The kind of section a card of `kind` belongs in: the first the project has a place for
    /// that names it, else one that holds anything — none when the project has no section for it
    /// (such a card is not made).
    pub fn home_of(&self, kind: &str) -> Option<String> {
        let reachable = self.reachable();
        reachable
            .iter()
            .find(|s| s.holds.iter().any(|k| k == kind))
            .or_else(|| reachable.iter().find(|s| s.holds_kind(kind)))
            .map(|s| s.name.clone())
    }

    /// Whether a card of `kind` has a place in the project (what the lists of kinds
    /// show of the built-in ones): a section for it, or a container with one that holds it. A card
    /// ghost and one the app alone draws are always shown.
    pub fn has_place(&self, kind: &str) -> bool {
        if kind == crate::detail::GHOST || super::system_only(kind) {
            return true;
        }
        if super::inner_only(kind) {
            return super::holders_of(kind)
                .into_iter()
                .any(|holder| self.home_of(holder).is_some());
        }
        self.home_of(kind).is_some()
    }

    /// Whether a kind of line has somewhere to be drawn: a kind with a place at each end.
    pub fn line_has_place(&self, line: &LineDef) -> bool {
        let end = |kinds: &[String]| {
            kinds.iter().any(|k| {
                k == "*"
                    || self
                        .cards()
                        .get(k)
                        .is_some_and(|kind| !kind.is_abstract && self.has_place(k))
            })
        };
        line.from.iter().any(|f| end(&f.kinds)) && end(&line.to.kinds)
    }

    /// Whether a section of `section` may hold a card of `kind`. A kind of section that is
    /// not defined (one of before) holds anything.
    pub fn holds(&self, section: &str, kind: &str) -> bool {
        self.section(section).is_none_or(|s| s.holds_kind(kind))
    }

    /// A section kind as people call it.
    pub fn section_label(&self, kind: &str) -> String {
        match self.section(kind) {
            Some(s) => s.label.clone(),
            None => match kind {
                "export" => text!("kindDefs.sectionLabel.export"),
                "rules" => text!("kindDefs.sectionLabel.rules"),
                "board" => text!("kindDefs.sectionLabel.board"),
                other => other.to_string(),
            },
        }
    }

    /// The mark a section is shown with: each part it writes of its own (`icon`), the other from its
    /// kind's — an actor's by its figure (`figure_mark`) while the project keeps the built-in mark of
    /// the kind. None of either: the app draws a folder in grey.
    pub fn shown_mark(
        &self,
        kind: Option<&str>,
        figure: Option<&str>,
        own: (Option<&str>, Option<&str>),
    ) -> (Option<String>, Option<String>) {
        let def = kind.and_then(|k| self.section(k));
        let mut name = def.and_then(|d| d.icon.clone());
        let color = def.and_then(|d| d.color.clone());
        if kind == Some(super::ACTOR) {
            let built_in = builtin_defs()
                .section(super::ACTOR)
                .and_then(|d| d.icon.clone());
            if name == built_in {
                name = Some(super::figure_mark(figure).to_string());
            }
        }
        (
            own.0.map(str::to_string).or(name),
            own.1.map(str::to_string).or(color),
        )
    }

    /// Whether a section's document may name this kind.
    pub fn knows_section(&self, kind: &str) -> bool {
        self.section(kind).is_some() || LEGACY_SECTIONS.contains(&kind)
    }

    /// The built-in ones with a project's own beside them, from what the project keeps
    /// (`projects.kinds`). What cannot be read of its own is left out: the project was written
    /// with definitions that were checked when they were (`project_update`).
    pub fn of_project(own: Option<&str>) -> Defs {
        let mut out = builtin_defs().clone();
        let Some(own) = own.and_then(|t| serde_json::from_str::<Value>(t).ok()) else {
            return out;
        };
        let mut ignored = Vec::new();
        let tree_given = own.get("project_tree").filter(|t| !t.is_null()).cloned();
        if own
            .get("cards")
            .is_some_and(|c| c.as_array().is_some_and(|a| !a.is_empty()))
        {
            out.own_cards = Some(super::builtin().with_own(own.get("cards"), &mut ignored));
        }
        let sections = read_sections(own.get("sections"), false, &out, out.cards(), &mut ignored);
        out.sections.extend(sections);
        apply_icons(&mut out, own.get("icons"), &mut ignored);
        apply_vias(&mut out, own.get("vias"), &mut ignored);
        let lines = read_lines(own.get("lines"), false, &out, out.cards(), &mut ignored);
        out.lines.extend(lines);
        if let Some(tree) = tree_given {
            let mut problems = Vec::new();
            let read = read_project_tree(&tree, &out, &mut problems);
            if problems.is_empty() {
                out.tree = read;
            }
        }
        out
    }

    /// A project's own definitions checked whole, with every problem found: the built-in
    /// ones with them beside, or why not.
    pub fn checked(own: &Value) -> Result<Defs, Vec<String>> {
        let mut out = builtin_defs().clone();
        let mut problems = Vec::new();
        let Some(own) = own.as_object() else {
            return Err(vec![text!("kindDefs.problem.kindsObject")]);
        };
        for key in own.keys() {
            if !KINDS_KEYS.contains(&key.as_str()) {
                problems.push(text!(
                    "kindDefs.problem.kindsKey",
                    key,
                    allowed = crate::text::list(&KINDS_KEYS)
                ));
            }
        }
        // The kinds of card first: a section and a line may name one of the project's own.
        if own.get("cards").is_some_and(|c| !c.is_null()) {
            out.own_cards = Some(super::builtin().with_own(own.get("cards"), &mut problems));
        }
        let sections = read_sections(own.get("sections"), false, &out, out.cards(), &mut problems);
        out.sections.extend(sections);
        apply_icons(&mut out, own.get("icons"), &mut problems);
        apply_vias(&mut out, own.get("vias"), &mut problems);
        let lines = read_lines(own.get("lines"), false, &out, out.cards(), &mut problems);
        out.lines.extend(lines);
        // Inside a section of one's own, kinds of section the project has.
        for section in out.sections.iter().filter(|s| !s.builtin) {
            for inner in &section.sections {
                if out.section(inner).is_none() {
                    problems.push(text!(
                        "kindDefs.problem.innerSection",
                        at = text!("kindDefs.at.sections", name = section.name),
                        inner
                    ));
                }
            }
        }
        if let Some(tree) = own.get("project_tree").filter(|t| !t.is_null()) {
            out.tree = read_project_tree(tree, &out, &mut problems);
        }
        // The built-in kinds' contexts as the project writes them over: each by a built-in
        // kind's name.
        match own.get("contexts") {
            None | Some(Value::Null) => {}
            Some(Value::Object(contexts)) => {
                for (name, context) in contexts {
                    let at = text!("kindDefs.at.contexts", name);
                    if !super::is_builtin_name(name) {
                        problems.push(text!("kindDefs.problem.contextBuiltinOnly", at));
                    } else if !context.is_object() {
                        problems.push(text!("kindDefs.problem.contextObject", at));
                    } else {
                        super::context_problems(Some(context), &at, &mut problems);
                    }
                }
            }
            Some(_) => problems.push(text!("kindDefs.problem.contextsObject")),
        }
        if problems.is_empty() {
            Ok(out)
        } else {
            Err(problems)
        }
    }
}

/// The keys a project's own kinds are kept under (`projects.kinds`).
pub const KINDS_KEYS: [&str; 7] = [
    "cards",
    "sections",
    "lines",
    "project_tree",
    "contexts",
    "icons",
    "vias",
];

/// A project's own values of a flow's `via` (`vias`) after the built-in ones, on the kinds of
/// card too (their field `via`'s choices), or what is wrong with them: each named as
/// `via_name_fits` says, not a built-in one, once.
fn apply_vias(defs: &mut Defs, given: Option<&Value>, problems: &mut Vec<String>) {
    let items = match given {
        None | Some(Value::Null) => return,
        Some(Value::Array(items)) => items,
        Some(_) => {
            problems.push(text!("kindDefs.problem.viasList"));
            return;
        }
    };
    let mut own: Vec<String> = Vec::new();
    for item in items {
        let Some(name) = item.as_str() else {
            problems.push(text!("kindDefs.problem.viasItem", item));
            continue;
        };
        if !super::via_name_fits(name) {
            problems.push(text!("kindDefs.problem.viasName", name));
        } else if super::builtin_vias().iter().any(|v| v == name) {
            problems.push(text!("kindDefs.problem.viasBuiltin", name));
        } else if own.iter().any(|v| v == name) {
            problems.push(text!("kindDefs.problem.viasTwice", name));
        } else {
            own.push(name.to_string());
        }
    }
    if own.is_empty() {
        return;
    }
    defs.vias.extend(own);
    defs.own_cards = Some(defs.cards().with_vias(&defs.vias));
}

/// What `project_tree` is: `{"main_sections": [{type, name}…], "board_sections": [{type, name}…]}`.
pub fn project_tree_shape() -> String {
    text!("kindDefs.projectTree.shape")
}

/// `project_tree` read, every problem found: each tree's sections there, of a kind that may be put
/// under its root (`trees`), each named as a section is, once across both trees in any case, not
/// the app's (`reserved_name`) nor a root's.
fn read_project_tree(raw: &Value, defs: &Defs, problems: &mut Vec<String>) -> ProjectTree {
    let mut out = ProjectTree {
        main: Vec::new(),
        board: Vec::new(),
    };
    let Some(raw) = raw.as_object() else {
        problems.push(project_tree_shape());
        return out;
    };
    for key in raw.keys() {
        if !Tree::ALL.iter().any(|t| t.key() == key) {
            problems.push(text!("kindDefs.problem.treeKey", key));
        }
    }
    let mut seen: Vec<String> = Vec::new();
    for tree in Tree::ALL {
        let at = text!("kindDefs.at.treeKey", key = tree.key());
        let Some(items) = raw.get(tree.key()).and_then(Value::as_array) else {
            problems.push(text!(
                "kindDefs.problem.treeMissing",
                at,
                shape = project_tree_shape()
            ));
            continue;
        };
        if items.is_empty() {
            problems.push(text!("kindDefs.problem.treeEmpty", at, tree = tree.said()));
        }
        let list = match tree {
            Tree::Main => &mut out.main,
            Tree::Board => &mut out.board,
        };
        for (i, item) in items.iter().enumerate() {
            let here = text!("kindDefs.at.treeItem", key = tree.key(), i);
            let Some(item) = item.as_object() else {
                problems.push(text!("kindDefs.problem.treeItemObject", here));
                continue;
            };
            let kind = item.get("type").and_then(Value::as_str).unwrap_or_default();
            if let Some(key) = item.keys().find(|k| {
                !(["type", "name"].contains(&k.as_str()) || (kind == WORKDIR && *k == "directory"))
            }) {
                problems.push(text!("kindDefs.problem.treeItemKey", here, key));
            }
            let directory = match item.get("directory") {
                None | Some(Value::Null) => None,
                Some(Value::String(id)) if !id.trim().is_empty() => Some(id.trim().to_string()),
                Some(_) => {
                    problems.push(text!("kindDefs.problem.treeItemDirectory", here));
                    None
                }
            };
            let name = item
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .trim();
            if kind.is_empty() || name.is_empty() {
                problems.push(text!("kindDefs.problem.treeItemNeeds", here));
                continue;
            }
            match defs.section(kind) {
                None => problems.push(text!("kindDefs.problem.treeItemKind", here, kind)),
                Some(def) if !def.trees.contains(&tree) => problems.push(text!(
                    "kindDefs.problem.treeItemPlace",
                    here,
                    label = def.label,
                    kind,
                    tree = tree.said(),
                    trees =
                        crate::text::list(&def.trees.iter().map(|t| t.name()).collect::<Vec<_>>())
                )),
                Some(_) => {}
            }
            if let Err(problem) = crate::write::valid_section_name(name) {
                problems.push(format!("{here}: {problem}"));
            } else if reserved_name(name) {
                problems.push(text!("kindDefs.problem.treeItemReserved", here, name));
            } else if ["main", "board"].contains(&crate::pathtext::fold(name).as_str()) {
                problems.push(text!("kindDefs.problem.treeItemTreeName", here, name));
            } else if seen.contains(&crate::pathtext::fold(name)) {
                problems.push(text!("kindDefs.problem.treeItemTwice", here, name));
            }
            seen.push(crate::pathtext::fold(name));
            let mut top = TopSection::new(kind, name);
            top.directory = directory.filter(|_| kind == WORKDIR);
            list.push(top);
        }
    }
    out
}

/// The built-in kinds of section and of line, read once for each language.
pub fn builtin_defs() -> &'static Defs {
    static PARSED: [OnceLock<Defs>; crate::text::LANGUAGES.len()] =
        [const { OnceLock::new() }; crate::text::LANGUAGES.len()];
    PARSED[crate::text::current_index()].get_or_init(|| {
        let root = super::builtin_root();
        let cards = super::builtin();
        let mut problems = Vec::new();
        let mut defs = Defs {
            sections: Vec::new(),
            lines: Vec::new(),
            tree: ProjectTree::default(),
            own_cards: None,
            vias: super::builtin_vias().to_vec(),
        };
        defs.sections = read_sections(root.get("sections"), true, &defs, cards, &mut problems);
        defs.lines = read_lines(root.get("lines"), true, &defs, cards, &mut problems);
        assert!(
            problems.is_empty(),
            "builtin.json: {}",
            problems.join(" / ")
        );
        defs
    })
}

/// A built-in definition as `builtin.json` writes it (in the call's language): a section's
/// (`sections`) or a line's (`lines`), for what MCP reads.
pub fn builtin_written(list: &str) -> &'static [Value] {
    super::builtin_root()[list]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

// MARK: - Reading

fn keys_known(raw: &Map<String, Value>, known: &[&str], at: &str, problems: &mut Vec<String>) {
    for key in raw.keys() {
        if !known.contains(&key.as_str()) {
            problems.push(text!(
                "kindDefs.problem.unknownKey",
                at,
                key,
                allowed = crate::text::list(known)
            ));
        }
    }
}

fn string(raw: &Map<String, Value>, key: &str) -> Option<String> {
    raw.get(key).and_then(Value::as_str).map(str::to_string)
}

/// One string or a list of them.
fn strings(value: Option<&Value>, at: &str, key: &str, problems: &mut Vec<String>) -> Vec<String> {
    match value {
        None => Vec::new(),
        Some(Value::String(one)) => vec![one.clone()],
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(|v| {
                let s = v.as_str().map(str::to_string);
                if s.is_none() {
                    problems.push(text!("kindDefs.problem.stringList", at, key));
                }
                s
            })
            .collect(),
        Some(_) => {
            problems.push(text!("kindDefs.problem.stringList", at, key));
            Vec::new()
        }
    }
}

/// A name a project's own kind may have: not empty, no space at either end, 40 characters at
/// most, nothing that a path or a list would read as its own (`:` `.` `/` `,` `"` a line
/// break).
fn own_name(name: &str) -> bool {
    !name.is_empty()
        && name.trim() == name
        && name.chars().count() <= 40
        && !name.chars().any(|c| ":./,\"\n\r\t".contains(c))
}

const SECTION_KEYS: [&str; 12] = [
    "section",
    "label",
    "description",
    "context",
    "holds",
    "sections",
    "opens",
    "trees",
    "export",
    "icon",
    "color",
    "actions",
];

/// The actions a kind of section may name (`actions`), each done by the core (`section_action`).
pub const SECTION_ACTIONS: [&str; 1] = ["read"];

fn read_sections(
    value: Option<&Value>,
    builtin: bool,
    before: &Defs,
    cards: &Kinds,
    problems: &mut Vec<String>,
) -> Vec<SectionDef> {
    let items = match value {
        None => return Vec::new(),
        Some(Value::Array(items)) => items,
        Some(_) => {
            problems.push(text!("kindDefs.problem.sectionsList"));
            return Vec::new();
        }
    };
    let mut out: Vec<SectionDef> = Vec::new();
    for (i, raw) in items.iter().enumerate() {
        let Some(raw) = raw.as_object() else {
            problems.push(text!(
                "kindDefs.problem.mustBeObject",
                at = format!("sections[{i}]")
            ));
            continue;
        };
        let name = string(raw, "section").unwrap_or_default();
        let at = if name.is_empty() {
            format!("sections[{i}]")
        } else {
            text!("kindDefs.at.sections", name)
        };
        keys_known(raw, &SECTION_KEYS, &at, problems);
        super::context_problems(raw.get("context"), &at, problems);
        if name.is_empty() {
            problems.push(text!("kindDefs.problem.sectionMissing", at));
            continue;
        }
        if !builtin && !super::identifier(&name) {
            problems.push(text!("kindDefs.problem.sectionName", at));
        }
        if before.section(&name).is_some() || out.iter().any(|s| s.name == name) {
            problems.push(text!(
                "kindDefs.problem.sectionSame",
                at,
                builtin = if before.section(&name).is_some_and(|s| s.builtin) {
                    text!("kindDefs.note.builtin")
                } else {
                    String::new()
                }
            ));
            continue;
        }
        let holds = strings(raw.get("holds"), &at, "holds", problems);
        if holds.is_empty() {
            problems.push(text!("kindDefs.problem.holdsMissing", at));
        }
        for kind in &holds {
            if kind != "*" && cards.get(kind).is_none_or(|k| k.is_abstract) {
                problems.push(text!("kindDefs.problem.holdsKind", at, kind));
            } else if !builtin && super::inner_only(kind) {
                problems.push(text!(
                    "kindDefs.problem.holdsInner",
                    at,
                    kind,
                    holders = crate::text::list(&super::holders_of(kind))
                ));
            }
        }
        let mut sections = strings(raw.get("sections"), &at, "sections", problems);
        if sections.is_empty() {
            sections.push(name.clone());
        }
        let opens = string(raw, "opens").unwrap_or_else(|| "relations".to_string());
        if !OPENS.contains(&opens.as_str()) {
            problems.push(text!(
                "kindDefs.problem.opensOne",
                at,
                allowed = crate::text::list(&OPENS),
                opens
            ));
        }
        // Which roots it may be put directly under in `project_tree`: both when not said.
        let mut trees: Vec<Tree> = Vec::new();
        for name in strings(raw.get("trees"), &at, "trees", problems) {
            match Tree::parse(&name) {
                Some(tree) if !trees.contains(&tree) => trees.push(tree),
                Some(_) => {}
                None => problems.push(text!("kindDefs.problem.treesList", at, name)),
            }
        }
        // An empty list: put directly under no root, only made inside another (`sections`).
        if raw.get("trees").is_none() {
            trees = Tree::ALL.to_vec();
        }
        // `export` (whether it had a default of where its cards were written out) is read and let go
        // (the exports are the project's).
        // The nav panel's mark: `icon: {name, color}`. The one before it, `icon` a
        // string beside a `color`, is read and let go (the mark falls to the default) and refused
        // when written.
        let (icon, color) = read_icon(raw.get("icon"), &at, problems);
        if raw.get("color").is_some_and(|c| !c.is_null()) {
            problems.push(text!("kindDefs.problem.colorInIcon", at));
        }
        let actions = strings(raw.get("actions"), &at, "actions", problems);
        if !builtin && !actions.is_empty() {
            problems.push(text!("kindDefs.problem.sectionActionsBuiltinOnly", at));
        }
        for name in actions
            .iter()
            .filter(|n| !SECTION_ACTIONS.contains(&n.as_str()))
        {
            problems.push(text!(
                "kindDefs.problem.sectionActionUnknown",
                at,
                name,
                known = crate::text::list(&SECTION_ACTIONS)
            ));
        }
        out.push(SectionDef {
            label: string(raw, "label").unwrap_or_else(|| name.clone()),
            description: string(raw, "description").unwrap_or_default(),
            name,
            holds,
            sections,
            opens,
            trees,
            builtin,
            icon,
            color,
            actions,
        });
    }
    // Inside one of the built-in ones, built-in kinds.
    if builtin {
        let names: Vec<String> = out.iter().map(|s| s.name.clone()).collect();
        for s in &out {
            for inner in &s.sections {
                if !names.contains(inner) {
                    problems.push(text!(
                        "kindDefs.problem.innerSection",
                        at = text!("kindDefs.at.sections", name = s.name),
                        inner
                    ));
                }
            }
        }
    }
    out
}

/// A mark as it is written, `{name, color}`: an SF Symbols name (lower-case
/// letters, digits and `.`, up to 80) and a colour (`#RRGGBB`), either left out. What is wrong is
/// said in `problems`, and the mark of it left as none: so a definition written the way before
/// (`icon` a string) reads as having no mark, and is refused only when it is written.
pub fn read_icon(
    value: Option<&Value>,
    at: &str,
    problems: &mut Vec<String>,
) -> (Option<String>, Option<String>) {
    let object = match value {
        None | Some(Value::Null) => return (None, None),
        Some(Value::Object(object)) => object,
        Some(Value::String(_)) => {
            problems.push(text!("kindDefs.problem.iconForm", at));
            return (None, None);
        }
        Some(other) => {
            problems.push(text!("kindDefs.problem.iconFormOther", at, other));
            return (None, None);
        }
    };
    for key in object.keys() {
        if key != "name" && key != "color" {
            problems.push(text!("kindDefs.problem.iconKey", at, key));
        }
    }
    let name = match object.get("name") {
        None | Some(Value::Null) => None,
        Some(Value::String(i))
            if !i.is_empty()
                && i.len() <= 80
                && i.chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '.') =>
        {
            Some(i.clone())
        }
        Some(other) => {
            problems.push(text!("kindDefs.problem.iconName", at, other));
            None
        }
    };
    let color = match object.get("color") {
        None | Some(Value::Null) => None,
        Some(Value::String(c))
            if c.len() == 7
                && c.starts_with('#')
                && c[1..].chars().all(|h| h.is_ascii_hexdigit()) =>
        {
            Some(c.clone())
        }
        Some(other) => {
            problems.push(text!("kindDefs.problem.iconColor", at, other));
            None
        }
    };
    (name, color)
}

/// The marks a project writes over the built-in kinds of section with (`icons`: `{<the kind's
/// name>: {name, color}}`, what it leaves out staying the built-in one's), set into `defs`. Only a
/// built-in kind of section is named: a kind of the project's own says its mark in its definition.
fn apply_icons(defs: &mut Defs, icons: Option<&Value>, problems: &mut Vec<String>) {
    let icons = match icons {
        None | Some(Value::Null) => return,
        Some(Value::Object(icons)) => icons,
        Some(_) => {
            problems.push(text!("kindDefs.problem.iconsObject"));
            return;
        }
    };
    for (name, mark) in icons {
        let at = text!("kindDefs.at.icons", name);
        if !defs.section(name).is_some_and(|s| s.builtin) {
            problems.push(text!("kindDefs.problem.iconsBuiltin", at));
            continue;
        }
        let (icon, color) = read_icon(Some(mark), &at, problems);
        if let Some(section) = defs.sections.iter_mut().find(|s| s.name == *name) {
            if icon.is_some() {
                section.icon = icon;
            }
            if color.is_some() {
                section.color = color;
            }
        }
    }
}

/// A built-in kind of section's `icon` as the project writes it over (`icons`): each of `name` and
/// `color` it writes in place of the built-in one's (a null gives it back). None when it writes
/// nothing over it.
pub fn icon_over(builtin: Option<&Value>, over: Option<&Value>) -> Option<Value> {
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

const LINE_KEYS: [&str; 16] = [
    "line",
    "label",
    "description",
    "context",
    "base",
    "look",
    "from",
    "to",
    "max",
    "fields",
    "export",
    "owner",
    "source",
    "siblings",
    "builtin",
    "visible",
];

const LOOK_KEYS: [&str; 4] = ["color", "dash", "begin", "end"];

fn read_end(
    raw: Option<&Value>,
    at: &str,
    key: &str,
    cards: &Kinds,
    problems: &mut Vec<String>,
) -> Option<End> {
    let Some(raw) = raw.and_then(Value::as_object) else {
        problems.push(text!("kindDefs.problem.endForm", at, key));
        return None;
    };
    let here = text!("kindDefs.at.end", at, key);
    keys_known(raw, &["kinds", "at", "rows"], &here, problems);
    let kinds = strings(raw.get("kinds"), &here, "kinds", problems);
    if kinds.is_empty() {
        problems.push(text!("kindDefs.problem.endKindsMissing", here));
    }
    for kind in &kinds {
        if kind != "*" && cards.get(kind).is_none_or(|k| k.is_abstract) {
            problems.push(text!("kindDefs.problem.endKind", here, kind));
        }
    }
    let mut ats = Vec::new();
    for name in strings(raw.get("at"), &here, "at", problems) {
        match At::parse(&name) {
            Some(a) => ats.push(a),
            None => problems.push(text!(
                "kindDefs.problem.endAt",
                here,
                name,
                hint = if name == "field" {
                    text!("kindDefs.note.fieldIsRow")
                } else {
                    String::new()
                }
            )),
        }
    }
    if ats.is_empty() {
        ats.push(At::Card);
    }
    // A function or a type is inside a file: an end lands at one only where it reaches files.
    if ats.contains(&At::Symbol) && !kinds.iter().any(|k| k == "file") {
        problems.push(text!("kindDefs.problem.endSymbolFile", here));
    }
    // Which kinds of row, for a kind of card with more than one: each
    // one a kind of the end's kinds has.
    let rows = strings(raw.get("rows"), &here, "rows", problems);
    if !rows.is_empty() && !ats.contains(&At::Row) {
        problems.push(text!("kindDefs.problem.endRowsAt", here));
    }
    let any = kinds.iter().any(|k| k == "*");
    let named: Vec<&super::Kind> = if any {
        cards.placeable().collect()
    } else {
        kinds.iter().filter_map(|k| cards.get(k)).collect()
    };
    let mut known: Vec<String> = Vec::new();
    for name in named.iter().flat_map(|k| row_names(k)) {
        if !known.contains(&name) {
            known.push(name);
        }
    }
    for row in &rows {
        if known.contains(row) {
            continue;
        }
        problems.push(if any {
            text!("kindDefs.problem.endRowsNone", here, row)
        } else {
            text!(
                "kindDefs.problem.endRowsMissing",
                here,
                row,
                kinds = crate::text::list(&kinds),
                known = if known.is_empty() {
                    text!("kindDefs.note.noRowKinds")
                } else {
                    text!(
                        "kindDefs.note.rowKindsAre",
                        rows = crate::text::list(&known)
                    )
                }
            )
        });
    }
    Some(End {
        kinds,
        at: ats,
        rows,
    })
}

fn read_lines(
    value: Option<&Value>,
    builtin: bool,
    before: &Defs,
    cards: &Kinds,
    problems: &mut Vec<String>,
) -> Vec<LineDef> {
    let items = match value {
        None => return Vec::new(),
        Some(Value::Array(items)) => items,
        Some(_) => {
            problems.push(text!("kindDefs.problem.linesList"));
            return Vec::new();
        }
    };
    let mut out: Vec<LineDef> = Vec::new();
    for (i, raw) in items.iter().enumerate() {
        let Some(raw) = raw.as_object() else {
            problems.push(text!(
                "kindDefs.problem.mustBeObject",
                at = format!("lines[{i}]")
            ));
            continue;
        };
        let name = string(raw, "line").unwrap_or_default();
        let at = if name.is_empty() {
            format!("lines[{i}]")
        } else {
            text!("kindDefs.at.lines", name)
        };
        let mut known: Vec<&str> = LINE_KEYS.to_vec();
        known.retain(|k| *k != "builtin");
        keys_known(raw, &known, &at, problems);
        super::context_problems(raw.get("context"), &at, problems);
        if name.is_empty() {
            problems.push(text!("kindDefs.problem.lineMissing", at));
            continue;
        }
        if !builtin && !own_name(&name) {
            problems.push(text!("kindDefs.problem.lineName", at));
        }
        if before.line(&name).is_some() || out.iter().any(|l| l.name == name) {
            problems.push(text!(
                "kindDefs.problem.lineSame",
                at,
                builtin = if before.line(&name).is_some_and(|l| l.builtin) {
                    text!("kindDefs.note.builtin")
                } else {
                    String::new()
                }
            ));
            continue;
        }
        let base = match string(raw, "base").as_deref().map(Base::parse) {
            Some(Some(b)) => b,
            Some(None) | None => {
                problems.push(text!("kindDefs.problem.lineBase", at));
                Base::Link
            }
        };
        let look = {
            let raw_look = raw.get("look").and_then(Value::as_object);
            if raw.get("look").is_some() && raw_look.is_none() {
                problems.push(text!("kindDefs.problem.lookObject", at));
            }
            let empty = Map::new();
            let l = raw_look.unwrap_or(&empty);
            keys_known(l, &LOOK_KEYS, &text!("kindDefs.at.look", at), problems);
            let pick = |key: &str, allowed: &[&str], default: &str, problems: &mut Vec<String>| {
                let v = string(l, key).unwrap_or_else(|| default.to_string());
                if !allowed.contains(&v.as_str()) {
                    problems.push(text!(
                        "kindDefs.problem.lookValue",
                        at,
                        key,
                        allowed = crate::text::list(allowed),
                        value = v
                    ));
                }
                v
            };
            Look {
                color: pick("color", &COLORS, "grey", problems),
                dash: pick("dash", &DASHES, "solid", problems),
                begin: pick("begin", &CAPS, "none", problems),
                end: pick("end", &CAPS, "arrow", problems),
            }
        };
        let from = match raw.get("from") {
            Some(Value::Array(ends)) => ends
                .iter()
                .enumerate()
                .filter_map(|(j, e)| read_end(Some(e), &at, &format!("from[{j}]"), cards, problems))
                .collect(),
            Some(one @ Value::Object(_)) => read_end(Some(one), &at, "from", cards, problems)
                .into_iter()
                .collect(),
            _ => {
                problems.push(text!("kindDefs.problem.fromMissing", at));
                Vec::new()
            }
        };
        let to = read_end(raw.get("to"), &at, "to", cards, problems).unwrap_or(End {
            kinds: vec!["*".to_string()],
            at: vec![At::Card],
            rows: Vec::new(),
        });
        let max = match raw.get("max") {
            None | Some(Value::Null) => None,
            Some(Value::Number(n)) if n.as_u64().is_some_and(|n| n >= 1) => {
                n.as_u64().map(|n| n as usize)
            }
            Some(other) => {
                problems.push(text!("kindDefs.problem.maxValue", at, other));
                None
            }
        };
        let fields = read_fields(raw.get("fields"), &format!("{at}.fields"), problems);
        super::check_line_fields(&fields, &at, problems);
        let export = match raw.get("export") {
            None | Some(Value::Null) => None,
            Some(Value::Object(e)) => {
                keys_known(
                    e,
                    &["as", "key", "word"],
                    &text!("kindDefs.at.export", at),
                    problems,
                );
                let as_ = match string(e, "as").as_deref() {
                    Some("none") | None => ExportAs::None,
                    Some("comment") => ExportAs::Comment,
                    Some("annotation") => ExportAs::Annotation,
                    Some("schema") => ExportAs::Schema,
                    Some(other) => {
                        problems.push(text!("kindDefs.problem.exportAs", at, other));
                        ExportAs::None
                    }
                };
                let key = string(e, "key");
                if as_ == ExportAs::Annotation
                    && !key
                        .as_deref()
                        .is_some_and(|k| k.starts_with("x-") && k.len() > 2)
                {
                    problems.push(text!("kindDefs.problem.exportAnnotationKey", at));
                }
                if !matches!(as_, ExportAs::Annotation | ExportAs::Schema) && key.is_some() {
                    problems.push(text!("kindDefs.problem.exportKeyUse", at));
                }
                if as_ == ExportAs::Schema && key.as_deref().is_some_and(|k| k.trim().is_empty()) {
                    problems.push(text!("kindDefs.problem.exportSchemaKey", at));
                }
                Some(Export {
                    as_,
                    key,
                    word: string(e, "word"),
                })
            }
            Some(_) => {
                problems.push(text!("kindDefs.problem.exportObject", at));
                None
            }
        };
        if base == Base::Embed && export.as_ref().is_some_and(|e| e.as_ != ExportAs::None) {
            problems.push(text!("kindDefs.problem.exportEmbed", at));
        }
        // A form written as a JSON Schema in a data card's JSON: from a data card's field or element,
        // to a form's head.
        if export.as_ref().is_some_and(|e| e.as_ == ExportAs::Schema) {
            let from_ok = !from.is_empty()
                && from.iter().all(|e| {
                    e.at == [At::Row]
                        && !e.kinds.is_empty()
                        && e.kinds.iter().all(|k| k == "instance")
                });
            if !from_ok {
                problems.push(text!("kindDefs.problem.exportSchemaFrom", at));
            }
            let to_ok = to.at == [At::Card]
                && !to.kinds.is_empty()
                && to.kinds.iter().all(|k| k == "entity" || k == "value");
            if !to_ok {
                problems.push(text!("kindDefs.problem.exportSchemaTo", at));
            }
        }
        let owner = strings(raw.get("owner"), &at, "owner", problems);
        if !owner.is_empty() && !builtin {
            problems.push(text!("kindDefs.problem.ownerBuiltin", at));
        }
        for kind in &owner {
            if cards.get(kind).is_none_or(|k| k.is_abstract) {
                problems.push(text!("kindDefs.problem.ownerKind", at, kind));
            }
        }
        let from_text = match string(raw, "source").as_deref() {
            None | Some("connections") => false,
            Some("text") if builtin => true,
            Some("text") => {
                problems.push(text!("kindDefs.problem.sourceText", at));
                false
            }
            Some(other) => {
                problems.push(text!("kindDefs.problem.sourceOne", at, other));
                false
            }
        };
        let siblings = match raw.get("siblings") {
            None | Some(Value::Bool(false)) => false,
            Some(Value::Bool(true)) if builtin => true,
            Some(Value::Bool(true)) => {
                problems.push(text!("kindDefs.problem.siblingsBuiltin", at));
                false
            }
            Some(_) => {
                problems.push(text!("kindDefs.problem.siblingsBool", at));
                false
            }
        };
        let visible = match raw.get("visible") {
            None | Some(Value::Bool(true)) => true,
            Some(Value::Bool(false)) => false,
            Some(_) => {
                problems.push(text!("kindDefs.problem.visibleBool", at));
                true
            }
        };
        // An embedding makes a field's type the card it reaches: it leaves a record's field.
        if base == Base::Embed
            && !from.iter().all(|e| {
                e.at == [At::Row] && !e.kinds.is_empty() && e.kinds.iter().all(|k| is_record(k))
            })
        {
            problems.push(text!("kindDefs.problem.embedFrom", at));
        }
        out.push(LineDef {
            label: string(raw, "label").unwrap_or_else(|| name.clone()),
            description: string(raw, "description").unwrap_or_default(),
            name,
            base,
            look,
            from,
            to,
            max,
            fields,
            export,
            owner,
            siblings,
            from_text,
            builtin,
            visible,
        });
    }
    out
}

/// What a line's fields may hold: everything a card's may, but a card put inside or how
/// it looks (a line is not drawn apart from itself).
pub(super) fn line_field_problems(fields: &[Field], at: &str, problems: &mut Vec<String>) {
    for field in fields {
        if field.fieldtype == FieldType::Card {
            problems.push(text!("kindDefs.problem.fieldCard", at, name = field.name));
        }
        if field.look {
            problems.push(text!("kindDefs.problem.fieldLook", at, name = field.name));
        }
        line_field_problems(&field.fields, at, problems);
    }
}
