//! Card kinds as definitions: every kind of card, the built-in ones too, is a
//! definition of its fields (one value each) and its rows (as many as are written, each a
//! line of fields, with rows under it). A card's document is checked against its kind
//! (`check`) and carried to and from MCP's shape by it (`convert`).
//!
//! The built-in definitions are this crate's data (`builtin.json`), as the MCP catalog
//! is: they change with the core's version and nothing in a project changes them. A kind
//! may take another's fields and rows and add its own (`extends`, one parent); the
//! abstract ones (`base`, `shape`, `record`) are only there to be taken from.
//!
//! The format is written out in `specboard://SpecBoard/Docs/Design:CardKinds`.

mod check;
mod connections;
mod convert;
mod defs;
pub(crate) mod overlay;
mod svg;

pub use check::{
    Problem, check_card, drop_strays, identifier, path_name, rule_problems, rules_held, strays,
};
pub use connections::{
    check_connections, connections_from_wire, connections_of, connections_to_wire, discriminated,
    writable_names,
};
pub use convert::{Names, Parts, from_wire, no_card, to_wire};
pub use defs::{
    At, Base, DIRECTORY, Defs, EVACUATION, End, Export, ExportAs, FIELD_ROWS, KINDS_KEYS, LineDef,
    LinePlace, Look, ProjectTree, SECTION_ACTIONS, SectionDef, TopSection, Tree, WORKDIR,
    builtin_defs, builtin_written, icon_over, is_record, project_tree_shape, read_icon,
    reserved_name,
};

use serde_json::{Map, Value};
use std::collections::HashSet;
use std::sync::OnceLock;

const BUILTIN: &str = include_str!("builtin.json");

/// One for each language, read the first time a call in that language asks.
type ByLanguage<T> = [OnceLock<T>; crate::text::LANGUAGES.len()];

/// The built-in definitions as `builtin.json` writes them, in the words of the call's language
/// (`overlay`).
pub(crate) fn builtin_root() -> &'static Value {
    static READ: ByLanguage<Value> = [const { OnceLock::new() }; crate::text::LANGUAGES.len()];
    let index = crate::text::current_index();
    READ[index].get_or_init(|| {
        let root: Value = serde_json::from_str(BUILTIN).expect("builtin.json is JSON");
        overlay::said_in(root, crate::text::LANGUAGES[index])
    })
}

/// The built-in kinds, read once for each language.
pub fn builtin() -> &'static Kinds {
    static PARSED: ByLanguage<Kinds> = [const { OnceLock::new() }; crate::text::LANGUAGES.len()];
    PARSED[crate::text::current_index()].get_or_init(|| match Kinds::from_root(builtin_root()) {
        Ok(kinds) => kinds,
        Err(problems) => panic!("builtin.json: {}", problems.join(" / ")),
    })
}

/// How a card opens: a
/// container on the cards it holds, drawn on its own diagram (`card-map`); a card of the project's own
/// on its fields and rows (`fields`, when not said) or on its field `body` written as a note's
/// (`markdown`).
pub const CARD_OPENS: [&str; 3] = ["card-map", "fields", "markdown"];

/// Whether a card of `kind` is put only inside a container: a kind a
/// container holds that no kind of section names (a flow's nodes). The kinds of section a
/// project defines may not name one (`Defs::checked`), and a section that holds anything (`*`)
/// does not hold it.
pub fn inner_only(kind: &str) -> bool {
    static INNER: OnceLock<Vec<String>> = OnceLock::new();
    INNER
        .get_or_init(|| {
            let sections = defs::builtin_written("sections");
            let named = |k: &str| {
                sections.iter().any(|s| {
                    s["holds"]
                        .as_array()
                        .is_some_and(|h| h.iter().any(|v| v.as_str() == Some(k)))
                })
            };
            builtin()
                .all()
                .iter()
                .flat_map(|k| k.holds.iter())
                .filter(|k| !named(k))
                .cloned()
                .collect()
        })
        .iter()
        .any(|k| k == kind)
}

/// Whether a card of `kind` goes only in a section that names it, not in one that holds any
/// (`*`): a frame draws another section's mock, which only a ui section's mock has room for.
pub fn named_only(kind: &str) -> bool {
    kind == FRAME
}

/// The kind of card that puts a ui section's mock into another's.
pub const FRAME: &str = "frame";

/// Whether a card of `kind` is one the app alone draws (`system_only`: a section ghost), never
/// made by a person or an AI, nor held by a section that holds anything (`*`).
pub fn system_only(kind: &str) -> bool {
    builtin().get(kind).is_some_and(|k| k.system_only)
}

/// The kinds of container that hold a card of `kind`, in the definitions' order.
pub fn holders_of(kind: &str) -> Vec<&'static str> {
    builtin()
        .all()
        .iter()
        .filter(|k| k.holds.iter().any(|h| h == kind))
        .map(|k| k.name.as_str())
        .collect()
}

/// Whether a card of `kind` holds cards inside it.
pub fn is_container(kind: &str) -> bool {
    builtin().get(kind).is_some_and(|k| !k.holds.is_empty())
}

/// The kinds of a flow's node whose field `via` (how what they pass goes) takes a project's values
/// beside the built-in ones: a begin on receive, a process node's rows, a run.
pub const VIA_KINDS: [&str; 3] = ["begin", "process", "run"];

/// The built-in values of a flow's `via`, as `builtin.json` writes them (each of `VIA_KINDS` the
/// same, which its tests hold to).
pub fn builtin_vias() -> &'static [String] {
    static VIAS: OnceLock<Vec<String>> = OnceLock::new();
    VIAS.get_or_init(|| {
        builtin()
            .get("run")
            .and_then(|k| k.field("via"))
            .map(|f| f.options.clone())
            .expect("builtin.json: run has a via")
    })
}

/// What a value of `via` a project adds is named with: lowercase letters, digits and `-`.
pub fn via_name_fits(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 40
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// A built-in kind's definition as `builtin.json` writes it (in the call's language), for `guide`.
pub fn builtin_definition(name: &str) -> Option<&'static serde_json::Value> {
    builtin_root()["kinds"]
        .as_array()?
        .iter()
        .find(|k| k["kind"] == name)
}

/// Every kind a card may be, the built-in ones (`builtin: true`) and then a project's own
/// (`projects.kinds.cards`), as written but whole: the fields and rows it takes from its
/// parents first (`extends`; a project's own takes `base` when it names none) and its
/// `context` over theirs, without `semantics` (the code's names) and `abstract`. What the
/// screen draws and writes a card by (`project_detail`'s `kinds.cards`). A project's own that
/// cannot be read (not in `kinds`) is left out.
pub fn written_whole(own: Option<&Value>, kinds: &Kinds) -> Vec<Value> {
    let builtin = builtin_root();
    let mut written: Vec<(Value, bool)> = builtin["kinds"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|k| (k.clone(), true))
        .collect();
    written.extend(
        own.and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(|k| (k.clone(), false)),
    );
    let find = |name: &str| written.iter().find(|(k, _)| k["kind"] == name);
    fn list(raw: &Value, key: &str) -> Vec<Value> {
        raw[key].as_array().cloned().unwrap_or_default()
    }
    let mut out = Vec::new();
    for (raw, is_builtin) in &written {
        let Some(name) = raw["kind"].as_str() else {
            continue;
        };
        let Some(kind) = kinds.get(name) else {
            continue;
        };
        if kind.is_abstract || kind.builtin != *is_builtin {
            continue;
        }
        // The parents, nearest last.
        let mut chain = vec![raw.clone()];
        let mut parent = kind.extends.clone();
        while let Some(p) = parent {
            let Some((above, _)) = find(&p) else {
                break;
            };
            chain.insert(0, above.clone());
            parent = above["extends"].as_str().map(str::to_string);
        }
        let mut whole = Map::new();
        let mut fields = Vec::new();
        let mut rows = Vec::new();
        let mut context = Map::new();
        for one in &chain {
            fields.extend(list(one, "fields"));
            rows.extend(list(one, "rows"));
            if let Some(c) = one["context"].as_object() {
                context.extend(c.clone());
            }
        }
        for (key, value) in raw.as_object().into_iter().flatten() {
            if !["semantics", "abstract", "fields", "rows", "context"].contains(&key.as_str()) {
                whole.insert(key.clone(), value.clone());
            }
        }
        if !context.is_empty() {
            whole.insert("context".into(), Value::Object(context));
        }
        whole.insert("fields".into(), Value::Array(fields));
        if !rows.is_empty() {
            whole.insert("rows".into(), Value::Array(rows));
        }
        if *is_builtin {
            whole.insert("builtin".into(), true.into());
        }
        out.push(Value::Object(whole));
    }
    out
}

/// What a field holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldType {
    Text,
    LongText,
    Markdown,
    Number,
    Checkbox,
    Date,
    Url,
    Radio,
    DropdownList,
    Person,
    Branch,
    Mr,
    /// A card put inside this one: its id in the document, its path on the wire.
    Card,
    /// A place in the project, written as a path (or held by the card's id, `store: id`).
    Path,
    /// A section of the project: its id in the document, so it follows a rename or a move, and
    /// its full path on the wire.
    Section,
    /// Fields of its own, together.
    Group,
    /// Any JSON, shaped by the kind's own checks (a rule's parameters).
    Json,
    /// Text by name (metadata).
    Map,
}

impl FieldType {
    const ALL: [(&'static str, FieldType); 18] = [
        ("text", FieldType::Text),
        ("longtext", FieldType::LongText),
        ("markdown", FieldType::Markdown),
        ("number", FieldType::Number),
        ("checkbox", FieldType::Checkbox),
        ("date", FieldType::Date),
        ("url", FieldType::Url),
        ("radio", FieldType::Radio),
        ("dropdownlist", FieldType::DropdownList),
        ("person", FieldType::Person),
        ("branch", FieldType::Branch),
        ("mr", FieldType::Mr),
        ("card", FieldType::Card),
        ("path", FieldType::Path),
        ("section", FieldType::Section),
        ("group", FieldType::Group),
        ("json", FieldType::Json),
        ("map", FieldType::Map),
    ];

    fn parse(name: &str) -> Option<FieldType> {
        Self::ALL.iter().find(|(n, _)| *n == name).map(|(_, t)| *t)
    }

    pub fn name(self) -> &'static str {
        Self::ALL
            .iter()
            .find(|(_, t)| *t == self)
            .map(|(n, _)| *n)
            .unwrap_or_default()
    }

    /// Whether it is chosen from `options`.
    pub fn chosen(self) -> bool {
        matches!(self, FieldType::Radio | FieldType::DropdownList)
    }
}

/// When a field or a row applies, or when a field is required: always, never, or when
/// any one of the alternatives holds — each naming fields beside it and the values
/// that let it in (a field not written counts as its default).
#[derive(Debug, Clone, PartialEq)]
pub enum Cond {
    Always,
    Never,
    AnyOf(Vec<Vec<(String, Vec<Value>)>>),
}

impl Cond {
    fn parse(value: Option<&Value>, absent: Cond, at: &str, problems: &mut Vec<String>) -> Cond {
        match value {
            None | Some(Value::Null) => absent,
            Some(Value::Bool(true)) => Cond::Always,
            Some(Value::Bool(false)) => Cond::Never,
            Some(Value::Object(one)) => Cond::AnyOf(vec![Self::all(one, at, problems)]),
            Some(Value::Array(many)) => Cond::AnyOf(
                many.iter()
                    .map(|alt| match alt {
                        Value::Object(one) => Self::all(one, at, problems),
                        _ => {
                            problems.push(text!("kinds.cond.altNotObject", at));
                            Vec::new()
                        }
                    })
                    .collect(),
            ),
            Some(_) => {
                problems.push(text!("kinds.cond.shape", at));
                absent
            }
        }
    }

    fn all(
        one: &Map<String, Value>,
        at: &str,
        problems: &mut Vec<String>,
    ) -> Vec<(String, Vec<Value>)> {
        one.iter()
            .map(|(field, allowed)| match allowed {
                Value::Array(values) => (field.clone(), values.clone()),
                other => {
                    problems.push(text!("kinds.cond.valuesNotList", at, field));
                    (field.clone(), vec![other.clone()])
                }
            })
            .collect()
    }

    /// Whether it holds for the fields written beside it, read through `value`.
    pub fn holds(&self, value: &dyn Fn(&str) -> Option<Value>) -> bool {
        match self {
            Cond::Always => true,
            Cond::Never => false,
            Cond::AnyOf(alternatives) => alternatives.iter().any(|all| {
                all.iter()
                    .all(|(field, allowed)| value(field).is_some_and(|v| allowed.contains(&v)))
            }),
        }
    }

    /// The fields it reads.
    fn reads(&self) -> impl Iterator<Item = (&str, &[Value])> {
        let alternatives: &[Vec<(String, Vec<Value>)>] = match self {
            Cond::AnyOf(alternatives) => alternatives,
            _ => &[],
        };
        alternatives
            .iter()
            .flatten()
            .map(|(field, values)| (field.as_str(), values.as_slice()))
    }
}

/// How a row is named in a path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Address {
    /// By its field `name` (a field, an item, a node).
    Name,
    /// By its field `value` (an enum's value).
    Value,
    /// By where it stands (a process node's step).
    Index,
}

/// One field: its value's kind, how many, and what holds it in.
#[derive(Debug, Clone)]
pub struct Field {
    pub name: String,
    pub label: String,
    pub description: String,
    pub fieldtype: FieldType,
    /// 0: one value, 1: a list of them, 2: a list of lists.
    pub many: u8,
    pub required: Cond,
    pub when: Cond,
    pub default: Option<Value>,
    /// Text: no two the same among the values of a list, or among the rows beside it.
    pub unique: bool,
    /// Text: characters; number: the value.
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub min_count: Option<usize>,
    pub max_count: Option<usize>,
    pub options: Vec<String>,
    /// How a value of `options` is shown, when not as it is (`optionLabels`).
    pub option_labels: Vec<(String, String)>,
    /// `identifier`: a name in a path.
    pub identifier: bool,
    /// Characters the text may not hold.
    pub forbid: String,
    /// How it looks rather than what it says: kept outside the document.
    pub look: bool,
    /// A path held by the card's id (`store: id`), so it follows a rename.
    pub by_id: bool,
    /// Text read for `[[path]]`, as Markdown is.
    pub paths: bool,
    /// A card field's kinds.
    pub kinds: Vec<String>,
    /// The values that count as done.
    pub done: Vec<Value>,
    pub show: Option<String>,
    /// A group's own fields.
    pub fields: Vec<Field>,
}

/// One kind of row.
#[derive(Debug, Clone)]
pub struct Row {
    pub name: String,
    pub label: String,
    pub address: Address,
    /// How deep rows of this kind may go under one another (5 at most).
    pub depth: Option<usize>,
    /// How many rows of this kind a card may hold, at every depth.
    pub max_total: Option<usize>,
    /// When rows of this kind may be under a row (read from the row above's fields).
    pub when: Cond,
    pub fields: Vec<Field>,
    pub children: Vec<Child>,
}

/// A kind of row allowed under a row.
#[derive(Debug, Clone)]
pub enum Child {
    Row(Row),
    /// The kind of an ancestor row again (a field's fields, an item's rows).
    Again {
        name: String,
        when: Cond,
    },
}

impl Child {
    pub fn name(&self) -> &str {
        match self {
            Child::Row(row) => &row.name,
            Child::Again { name, .. } => name,
        }
    }

    pub fn when(&self) -> &Cond {
        match self {
            Child::Row(row) => &row.when,
            Child::Again { when, .. } => when,
        }
    }
}

/// How a card of a kind is drawn on a diagram: under its header its
/// fields `top`, each a control pressed there (a check a switch, a choice its options side by
/// side or a menu, a date or a text as written); below them the field `markdown` drawn as
/// Markdown, as a writing card's body is, or a field drawn as a page of its own (`page`: an SVG
/// picture or an HTML page). A kind with none is drawn as its rows. Not taken from a parent: a
/// kind says its own. How big a card is drawn is the card's own (`Kind::sizable`), not its face's.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Face {
    pub top: Vec<String>,
    pub markdown: Option<String>,
    pub page: Option<Page>,
}

/// A field a face draws as a page of its own, written `{"svg": <field>}` or `{"html": <field>}`:
/// not Markdown, so the screen builds the page from the text itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page {
    pub field: String,
    pub drawn: PageKind,
}

/// What a face's page is drawn as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageKind {
    /// A picture: the text read as SVG, so it must be one (`check_card`).
    Svg,
    /// A page laid out at the box's width.
    Html,
}

impl PageKind {
    /// The key a face names it by, and the extension of the file it is written out as.
    pub const ALL: [(&'static str, PageKind); 2] =
        [("svg", PageKind::Svg), ("html", PageKind::Html)];

    pub fn name(self) -> &'static str {
        Self::ALL
            .iter()
            .find(|(_, k)| *k == self)
            .map(|(n, _)| *n)
            .unwrap_or_default()
    }
}

impl Face {
    /// The fields drawn below the ones at the top: its Markdown, or its page.
    fn drawn_below(&self) -> impl Iterator<Item = &String> {
        self.markdown
            .iter()
            .chain(self.page.iter().map(|p| &p.field))
    }
}

/// The kinds of field a face's `top` may hold, as they are said.
pub const FACE_TOP: [FieldType; 6] = [
    FieldType::Checkbox,
    FieldType::Radio,
    FieldType::DropdownList,
    FieldType::Date,
    FieldType::Text,
    FieldType::Section,
];

/// A kind of card, with what it takes from its parent already in it.
#[derive(Debug, Clone)]
pub struct Kind {
    pub name: String,
    pub label: String,
    pub is_abstract: bool,
    pub extends: Option<String>,
    pub color: Option<String>,
    pub header: Vec<String>,
    pub as_row: Option<Value>,
    pub connectable: bool,
    pub on_double_click: Option<String>,
    /// The kinds of card it holds inside it: a container. A card
    /// inside one keeps it by id (`in`), and its path goes on from it (`Behavior:Checkout.review`).
    pub holds: Vec<String>,
    /// How it opens (`CARD_OPENS`): `card-map` (the cards it holds, drawn on its own diagram), or
    /// for a card of the project's own `fields` or `markdown`.
    pub opens: Option<String>,
    pub context: Value,
    pub semantics: Value,
    pub fields: Vec<Field>,
    pub rows: Vec<Row>,
    /// How a card of it is drawn on a diagram, when it says (`face`); its rows otherwise.
    pub face: Option<Face>,
    /// What a card of it can be made to do in one go (`card_action`'s `op`), by name: what each
    /// does is the core's (`ACTIONS`), never a project's to write.
    pub actions: Vec<String>,
    /// One of the crate's (`builtin.json`); a project's own otherwise (`projects.kinds.cards`).
    pub builtin: bool,
    /// A card the app alone draws, never made (`system_only`): a section
    /// ghost (`section`). Built-in kinds alone say it.
    pub system_only: bool,
}

impl Field {
    /// How `value`, one of its options, is shown: as its kind says it (`optionLabels`), else as it is.
    pub fn option_label<'a>(&'a self, value: &'a str) -> &'a str {
        self.option_labels
            .iter()
            .find(|(v, _)| v == value)
            .map_or(value, |(_, label)| label.as_str())
    }
}

impl Kind {
    pub fn field(&self, name: &str) -> Option<&Field> {
        self.fields.iter().find(|f| f.name == name)
    }

    pub fn row(&self, name: &str) -> Option<&Row> {
        self.rows.iter().find(|r| r.name == name)
    }

    /// Whether a card of it may carry a size of its own (`size`): every boxed card. Not a flow's
    /// node, drawn by its node's shape; not a card ghost, drawn at the size of the card it leads
    /// to; not one the app alone draws (a section ghost keeps its size as a place, a working
    /// directory's file is laid out by the window). See
    /// `specboard://SpecBoard/Docs/Design:Overview` §12.
    pub fn sizable(&self) -> bool {
        !self.is_abstract
            && !self.system_only
            && self.name != "ghost"
            && self.name != "node"
            && self.extends.as_deref() != Some("node")
    }
}

/// A set of kinds: the built-in ones, and later a project's own beside them.
#[derive(Debug, Clone)]
pub struct Kinds {
    pub version: u64,
    kinds: Vec<Kind>,
}

impl Kinds {
    /// Reads a definitions file, refusing it whole with every problem found.
    pub fn parse(text: &str) -> Result<Kinds, Vec<String>> {
        let root: Value = serde_json::from_str(text)
            .map_err(|e| vec![text!("kinds.parse.badJson", error = e)])?;
        Kinds::from_root(&root)
    }

    /// Reads a definitions file read as JSON (`parse`).
    fn from_root(root: &Value) -> Result<Kinds, Vec<String>> {
        let mut problems = Vec::new();
        let Some(root) = root.as_object() else {
            return Err(vec![text!("kinds.parse.notObject")]);
        };
        let version = root
            .get("version")
            .and_then(Value::as_u64)
            .unwrap_or_else(|| {
                problems.push(text!("kinds.parse.noVersion"));
                0
            });
        let written = root
            .get("kinds")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_else(|| {
                problems.push(text!("kinds.parse.noKinds"));
                Vec::new()
            });
        let mut kinds: Vec<Kind> = Vec::new();
        for (i, raw) in written.iter().enumerate() {
            let Some(raw) = raw.as_object() else {
                problems.push(text!("kinds.parse.kindNotObject", i));
                continue;
            };
            let kind = read_kind(raw, &kinds, true, &mut problems);
            if kinds.iter().any(|k| k.name == kind.name) {
                problems.push(text!("kinds.parse.nameTaken", kind = kind.name));
            }
            kinds.push(kind);
        }
        for kind in &kinds {
            check_kind(kind, &mut problems);
            for held in &kind.holds {
                match kinds.iter().find(|k| &k.name == held) {
                    None => {
                        problems.push(text!("kinds.parse.holdsUnknown", kind = kind.name, held))
                    }
                    Some(k) if k.is_abstract => {
                        problems.push(text!("kinds.parse.holdsAbstract", kind = kind.name, held))
                    }
                    Some(_) => {}
                }
            }
            if !kind.holds.is_empty() && kind.opens.is_none() {
                problems.push(text!("kinds.parse.holdsNeedsOpens", kind = kind.name));
            }
        }
        if problems.is_empty() {
            Ok(Kinds { version, kinds })
        } else {
            Err(problems)
        }
    }

    pub fn get(&self, name: &str) -> Option<&Kind> {
        self.kinds.iter().find(|k| k.name == name)
    }

    /// Every kind, the abstract ones too, in the order written.
    pub fn all(&self) -> &[Kind] {
        &self.kinds
    }

    /// The kinds a card may be.
    /// The kinds a card may be: not an abstract one, nor one the app alone draws (`system_only`).
    pub fn placeable(&self) -> impl Iterator<Item = &Kind> {
        self.kinds
            .iter()
            .filter(|k| !k.is_abstract && !k.system_only)
    }

    /// Whether `name` is a kind of card a project defines (`projects.kinds.cards`), one a card may
    /// be: not one of the crate's.
    pub fn is_own(&self, name: &str) -> bool {
        self.get(name).is_some_and(|k| !k.builtin && !k.is_abstract)
    }

    /// Whether a card of `kind` holds cards inside it (a container).
    pub fn holds_cards(&self, kind: &str) -> bool {
        self.get(kind).is_some_and(|k| !k.holds.is_empty())
    }

    /// Whether a card of `kind` keeps what its definition no longer has (「定義に無い欄」),
    /// not refused for it: a kind the project defines, and the board's goal and task — read by
    /// their definitions alone, so a field taken out of them stays with the cards that have a
    /// value in it.
    pub fn keeps_strays(&self, kind: &str) -> bool {
        self.is_own(kind) || BOARD_KINDS.contains(&kind)
    }

    /// Whether a card of `kind` may carry a size of its own (`Kind::sizable`).
    pub fn sizable(&self, kind: &str) -> bool {
        self.get(kind).is_some_and(Kind::sizable)
    }

    /// These kinds with `vias` (the built-in values and a project's own after them) as what a
    /// flow's node may say in its field `via`, wherever it is (`VIA_KINDS`, a row's too).
    pub fn with_vias(&self, vias: &[String]) -> Kinds {
        fn over(fields: &mut [Field], vias: &[String]) {
            for field in fields.iter_mut().filter(|f| f.name == "via") {
                field.options = vias.to_vec();
            }
        }
        fn rows(rows: &mut [Row], vias: &[String]) {
            for row in rows {
                over(&mut row.fields, vias);
            }
        }
        let mut kinds = self.kinds.clone();
        for kind in kinds
            .iter_mut()
            .filter(|k| k.builtin && VIA_KINDS.contains(&k.name.as_str()))
        {
            over(&mut kind.fields, vias);
            rows(&mut kind.rows, vias);
        }
        Kinds {
            version: self.version,
            kinds,
        }
    }

    /// These kinds with a project's own after them (`projects.kinds.cards`):
    /// each read as the built-in ones are, from those before it, and held to what only the
    /// built-in ones may (`own_problems`). What is wrong goes into `problems`, said as
    /// `cards の <name>: …`; a kind that cannot be read (no name, a name taken) is left out.
    pub fn with_own(&self, own: Option<&Value>, problems: &mut Vec<String>) -> Kinds {
        let mut kinds = self.kinds.clone();
        let items = match own {
            None | Some(Value::Null) => {
                return Kinds {
                    version: self.version,
                    kinds,
                };
            }
            Some(Value::Array(items)) => items.as_slice(),
            Some(_) => {
                problems.push(text!("kinds.own.notList"));
                &[]
            }
        };
        let first = kinds.len();
        for (i, raw) in items.iter().enumerate() {
            let Some(raw) = raw.as_object() else {
                problems.push(text!("kinds.own.kindNotObject", i));
                continue;
            };
            let mut found = Vec::new();
            let kind = read_kind(raw, &kinds, false, &mut found);
            let taken = kinds.iter().find(|k| k.name == kind.name);
            if let Some(taken) = taken {
                found.push(if taken.builtin {
                    text!("kinds.own.nameTakenBuiltin", kind = kind.name)
                } else {
                    text!("kinds.parse.nameTaken", kind = kind.name)
                });
            }
            own_problems(raw, &kind, &kinds, &mut found);
            check_kind(&kind, &mut found);
            let unreadable = kind.name.is_empty() || taken.is_some();
            problems.extend(
                found
                    .into_iter()
                    .map(|p| text!("kinds.own.problem", problem = p)),
            );
            if !unreadable {
                kinds.push(kind);
            }
        }
        // What a container of the project's own holds: read once all are, so one may hold a
        // kind written after it (or itself).
        for kind in &kinds[first..] {
            for held in &kind.holds {
                let problem = match kinds.iter().find(|k| &k.name == held) {
                    None => Some(text!("kinds.own.holdsUnknown", kind = kind.name, held)),
                    Some(k) if k.is_abstract => {
                        Some(text!("kinds.own.holdsAbstract", kind = kind.name, held))
                    }
                    Some(_) if held == crate::detail::GHOST => {
                        Some(text!("kinds.own.holdsGhost", kind = kind.name))
                    }
                    Some(_) if inner_only(held) => Some(text!(
                        "kinds.own.holdsInnerOnly",
                        kind = kind.name,
                        held,
                        holders = crate::text::List::of(holders_of(held))
                    )),
                    Some(_) => None,
                };
                if let Some(problem) = problem {
                    problems.push(problem);
                }
            }
            if !kind.holds.is_empty() && kind.opens.is_none() {
                problems.push(text!("kinds.own.holdsNeedsOpens", kind = kind.name));
            }
        }
        Kinds {
            version: self.version,
            kinds,
        }
    }
}

/// The board's kinds of card, read by their definitions alone.
pub const BOARD_KINDS: [&str; 2] = ["goal", "task"];

/// The kind of section the board's goals and tasks are put in, directly under its root.
pub const GOALS: &str = "goals";

/// The kinds of section inside a goals section directly under the board's root that the card
/// actions move what they finish (`complete`) and what they cancel (`cancelled`) into: one of each
/// there, found by its kind, whatever it is named.
pub const COMPLETE: &str = "complete";
pub const CANCELLED: &str = "cancelled";

/// The kind of section that holds flows as one more part of the system: `chart` is the plain one,
/// and `actor` (`specboard://SpecBoard/Docs/Design:Overview` §9「アクターのセクション」) one that
/// provides and uses, nested in charts and actors alike.
pub const CHART: &str = "chart";
pub const ACTOR: &str = "actor";

/// What an actor is (`figure`): a part the system is built of — when none is written —, a person,
/// or something outside whose inside is not written.
pub const FIGURES: [&str; 3] = ["system", "person", "outside"];

/// Whether a section of `kind` holds flows as a chart does: a chart's or an actor's, where a
/// section's flows are listed (`![[Section:]]`) and a document's `diagrams` row may point.
pub fn holds_flows(kind: Option<&str>) -> bool {
    matches!(kind, Some(CHART | ACTOR))
}

/// The mark an actor of `figure` is drawn with while the project leaves the kind's mark as built in.
pub fn figure_mark(figure: Option<&str>) -> &'static str {
    match figure {
        Some("person") => "person",
        Some("outside") => "cloud",
        _ => "cube",
    }
}

/// A data card's kind, and its line to its shape (`specboard://SpecBoard/Docs/Design:CardKinds`
/// 「データ（instance）」): MCP writes the line with the value's key `of`, not in `connections`.
pub const INSTANCE: &str = "instance";
pub const OF_LINE: &str = "shapeOf";

/// The built-in kinds of line named otherwise before (`name_move`), each by its name of before and
/// of now: named now as the screen says them.
pub const LINE_NAMES_BEFORE: [(&str, &str); 5] = [
    ("notes", "rules"),
    ("of", "shapeOf"),
    ("branch", "condition"),
    ("event", "awaits"),
    ("peer", "partner"),
];

/// A goal's values of before (`name_move`), each by its field, its value of before and of now: in
/// English now, the Japanese their labels (`optionLabels`).
pub const GOAL_VALUES_BEFORE: [(&str, &str, &str); 7] = [
    ("status", "未着手", "notStarted"),
    ("status", "進行中", "inProgress"),
    ("status", "完了", "done"),
    ("status", "打ち切り", "cancelled"),
    ("priority", "高", "high"),
    ("priority", "中", "medium"),
    ("priority", "低", "low"),
];

/// The kind of a table of data cards (`specboard://SpecBoard/Docs/Design:CardKinds`「表（table）」): its shape is the line `of` too.
pub const TABLE: &str = "table";

/// A goal's fields by the names they had before they were named in English: the name of
/// before, and the name now.
pub const GOAL_KEYS_BEFORE: [(&str, &str); 3] =
    [("状態", "status"), ("優先度", "priority"), ("期限", "due")];

/// A goal's field by the name it had before a request to merge was called a merge request:
/// the name of before, and the name now.
pub const MR_KEYS_BEFORE: [(&str, &str); 1] = [("PR", "MR")];

/// A goal's field by the name it had before it was told apart from the tree an answer is of
/// (MCP's answers head with `branch`, which hid it): the name of before, and the name now.
pub const WORK_BRANCH_KEYS_BEFORE: [(&str, &str); 1] = [("branch", "workBranch")];

/// The fields the board's kinds had before they were held to one person's use: a goal's and a task's of before that they have no more.
const BOARD_KEYS_GONE: [(&str, &str); 5] = [
    ("goal", "担当"),
    ("task", "担当"),
    ("task", "優先度"),
    ("task", "branch"),
    ("task", "PR"),
];

/// What is said to a key the board's kinds had before and have no more or by another name,
/// written on a card of `kind`: how it is written now. None for any other.
pub fn board_key_before(kind: &str, key: &str) -> Option<String> {
    if kind == "goal"
        && let Some((_, now)) = GOAL_KEYS_BEFORE.iter().find(|(before, _)| *before == key)
    {
        return Some(text!("kinds.board.keyRenamed", key, now));
    }
    if kind == "goal"
        && let Some((_, now)) = MR_KEYS_BEFORE.iter().find(|(before, _)| *before == key)
    {
        return Some(text!("kinds.board.keyMr", key, now));
    }
    if kind == "goal"
        && let Some((_, now)) = WORK_BRANCH_KEYS_BEFORE
            .iter()
            .find(|(before, _)| *before == key)
    {
        return Some(text!("kinds.board.keyWorkBranch", key, now));
    }
    BOARD_KEYS_GONE.contains(&(kind, key)).then(|| {
        let left = match kind {
            "goal" => text!("kinds.board.goalKeysLeft"),
            _ => text!("kinds.board.taskKeysLeft"),
        };
        text!("kinds.board.keyGone", kind, key, left)
    })
}

/// The names a project's own kind of card may not have: the card ghost's, and those of before.
const RESERVED_KINDS: [&str; 4] = ["ghost", "action", "module", "custom_goal"];

/// What a card's value holds besides its kind's fields and rows (`card_create`,
/// `card_update`, and the keys of before they refuse): a project's own kind may not name a
/// field or a kind of row so.
pub const VALUE_KEYS: [&str; 23] = [
    "name",
    "kind",
    "topLeft",
    "size",
    "view",
    "version",
    "standalone",
    "exportTo",
    "connections",
    "in",
    "section",
    "position",
    "places",
    "target",
    "strayFields",
    "moves",
    "values",
    "fields",
    "nodes",
    "items",
    "holds",
    "notes",
    "links",
];

/// A name of a field or of a kind of row a project's own kind may have: not empty, no space at
/// either end, 40 characters at most, nothing a path reads as its own (`.` `:` `/` `[` `]`
/// `,` `"` a line break).
fn own_field_name(name: &str) -> bool {
    !name.is_empty()
        && name.trim() == name
        && name.chars().count() <= 40
        && !name.chars().any(|c| ".:/[],\"\n\r\t".contains(c))
}

/// What only a built-in kind may be or say, and what a project's own must:
/// its name, no `semantics`, a parent that is `base` or the project's own, a colour `#RRGGBB`,
/// no field that is how it looks (`look`), names a card's value can carry, what it does when
/// pressed, and its `context`.
fn own_problems(
    raw: &Map<String, Value>,
    kind: &Kind,
    earlier: &[Kind],
    problems: &mut Vec<String>,
) {
    let at = &kind.name;
    if !kind.name.is_empty() && !identifier(&kind.name) {
        problems.push(text!("kinds.def.nameForm", at));
    }
    if RESERVED_KINDS.contains(&kind.name.as_str()) {
        problems.push(text!(
            "kinds.def.nameReserved",
            at,
            reserved = crate::text::list(&RESERVED_KINDS)
        ));
    }
    if raw.contains_key("semantics") {
        problems.push(text!("kinds.def.semanticsBuiltinOnly", at));
    }
    // An action reads and writes the fields of the built-in kind that names it.
    if raw.contains_key("actions") {
        problems.push(text!("kinds.def.actionsBuiltinOnly", at));
    }
    if let Some(parent) = &kind.extends
        && let Some(found) = earlier.iter().find(|k| &k.name == parent)
        && found.builtin
        && parent != "base"
    {
        problems.push(text!("kinds.def.extendsForm", at, parent));
    }
    if let Some(color) = raw.get("color") {
        let fine = color.as_str().is_some_and(|c| {
            c.len() == 7 && c.starts_with('#') && c[1..].chars().all(|h| h.is_ascii_hexdigit())
        });
        if !fine {
            problems.push(text!("kinds.def.colorForm", at, color));
        }
    }
    fn fields_said(fields: &[Field], at: &str, problems: &mut Vec<String>) {
        for field in fields {
            if field.look {
                problems.push(text!("kinds.def.lookBuiltinOnly", at, name = field.name));
            }
            if !field.name.is_empty() && !own_field_name(&field.name) {
                problems.push(text!("kinds.def.fieldNameForm", at, name = field.name));
            }
            fields_said(&field.fields, &format!("{at}.{}", field.name), problems);
        }
    }
    fields_said(&kind.fields, at, problems);
    for field in &kind.fields {
        if VALUE_KEYS.contains(&field.name.as_str()) {
            problems.push(text!("kinds.def.fieldNameValueKey", at, name = field.name));
        }
    }
    fn rows_said(rows: &[Row], at: &str, problems: &mut Vec<String>) {
        for row in rows {
            if !row.name.is_empty() && !own_field_name(&row.name) {
                problems.push(text!("kinds.def.rowNameForm", at, name = row.name));
            }
            fields_said(&row.fields, &format!("{at}.{}", row.name), problems);
            let inner: Vec<Row> = row
                .children
                .iter()
                .filter_map(|c| match c {
                    Child::Row(r) => Some(r.clone()),
                    Child::Again { .. } => None,
                })
                .collect();
            rows_said(&inner, &format!("{at}.{}", row.name), problems);
        }
    }
    rows_said(&kind.rows, at, problems);
    for row in &kind.rows {
        if VALUE_KEYS.contains(&row.name.as_str()) {
            problems.push(text!("kinds.def.rowNameValueKey", at, name = row.name));
        }
    }
    match &kind.as_row {
        None | Some(Value::Null) => {}
        Some(Value::String(s)) if s == "unfold" => {}
        Some(Value::Array(names)) => {
            for name in names {
                match name.as_str() {
                    Some(n) if kind.field(n).is_some() => {}
                    _ => problems.push(text!("kinds.def.asRowNoField", at, name)),
                }
            }
        }
        Some(other) => problems.push(text!("kinds.def.asRowForm", at, other)),
    }
    match kind.on_double_click.as_deref() {
        None => {}
        Some("edit_in_place") => {
            if !kind
                .field("body")
                .is_some_and(|f| matches!(f.fieldtype, FieldType::LongText | FieldType::Markdown))
            {
                problems.push(text!("kinds.def.doubleClickNeedsBody", at));
            }
        }
        Some(other) => problems.push(text!("kinds.def.doubleClickForm", at, other)),
    }
    // How it opens: on its card map only when it holds cards, as a note only with a body to write.
    match kind.opens.as_deref() {
        Some("card-map") if kind.holds.is_empty() => {
            problems.push(text!("kinds.def.opensCardMapNeedsHolds", at))
        }
        Some(other @ ("fields" | "markdown")) if !kind.holds.is_empty() => {
            problems.push(text!("kinds.def.opensContainerCardMap", at, other))
        }
        Some("markdown")
            if !kind.field("body").is_some_and(|f| {
                matches!(f.fieldtype, FieldType::LongText | FieldType::Markdown)
            }) =>
        {
            problems.push(text!("kinds.def.opensMarkdownNeedsBody", at))
        }
        _ => {}
    }
    context_problems(raw.get("context"), at, problems);
    // `nest` is read and let go: what goes inside a container is the container's `holds`,
    // what a card may embed its embed line's `to`. So is `on_click`: what a press changes is a field's, by its type, where the card
    // draws it (`face`'s `top`, its rows, a container's `as_row`), and a press picks the card.
    for key in ["connectable", "abstract"] {
        if raw.get(key).is_some_and(|v| !v.is_boolean()) {
            problems.push(text!("kinds.def.flagForm", at, key));
        }
    }
}

// MARK: - Reading a definition

/// What a kind's `context` must be (a kind of card's, section's or line's, and a built-in one's
/// as a project writes it over): what it is for (`purpose`), when to use it (`use_when`) and what
/// it is not for (`not_for`), each a sentence.
pub(crate) fn context_problems(raw: Option<&Value>, at: &str, problems: &mut Vec<String>) {
    match raw {
        None | Some(Value::Null) => {}
        Some(Value::Object(context)) => {
            for (key, value) in context {
                if !CONTEXT_KEYS.contains(&key.as_str()) {
                    problems.push(text!("kinds.context.keyNotAllowed", at, key));
                } else if !value.is_string() {
                    problems.push(text!("kinds.context.notText", at, key));
                }
            }
        }
        Some(_) => problems.push(text!("kinds.context.shape", at)),
    }
}

/// Whether `name` is a built-in kind of card (not one only its kinds take after), of section or of
/// line: a name a project may write the context of over (`contexts`).
pub fn is_builtin_name(name: &str) -> bool {
    builtin_definition(name).is_some_and(|d| d["abstract"] != true)
        || ["sections", "lines"].into_iter().any(|list| {
            let key = if list == "sections" {
                "section"
            } else {
                "line"
            };
            builtin_written(list).iter().any(|d| d[key] == name)
        })
}

/// The keys of a kind's `context`.
pub const CONTEXT_KEYS: [&str; 3] = ["purpose", "use_when", "not_for"];

fn keys_known(raw: &Map<String, Value>, known: &[&str], at: &str, problems: &mut Vec<String>) {
    for key in raw.keys() {
        if !known.contains(&key.as_str()) {
            problems.push(text!("kinds.read.keyNotAllowed", at, key));
        }
    }
}

fn string(raw: &Map<String, Value>, key: &str) -> Option<String> {
    raw.get(key).and_then(Value::as_str).map(str::to_string)
}

fn strings(
    raw: &Map<String, Value>,
    key: &str,
    at: &str,
    problems: &mut Vec<String>,
) -> Vec<String> {
    match raw.get(key) {
        None => Vec::new(),
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(|v| {
                let s = v.as_str().map(str::to_string);
                if s.is_none() {
                    problems.push(text!("kinds.read.notStringList", at, key));
                }
                s
            })
            .collect(),
        Some(_) => {
            problems.push(text!("kinds.read.notStringList", at, key));
            Vec::new()
        }
    }
}

fn flag(raw: &Map<String, Value>, key: &str) -> bool {
    raw.get(key).and_then(Value::as_bool).unwrap_or(false)
}

const KIND_KEYS: [&str; 20] = [
    "kind",
    "label",
    "abstract",
    "system_only",
    "extends",
    "color",
    "header",
    "nest",
    "as_row",
    "connectable",
    "on_click",
    "on_double_click",
    "holds",
    "opens",
    "context",
    "semantics",
    "fields",
    "rows",
    "face",
    "actions",
];

/// The actions a kind may name (`actions`), each done by the core (`card_action`).
pub const ACTIONS: [&str; 5] = ["start", "finish", "cancel", "done", "reviewed"];

/// How a kind is drawn on a diagram, as written (`face`): its shape alone here; what it names
/// is checked with the kind's fields (`check_face`).
fn read_face(raw: Option<&Value>, at: &str, problems: &mut Vec<String>) -> Option<Face> {
    let shape = |problems: &mut Vec<String>| problems.push(text!("kinds.read.faceShape", at));
    let raw = match raw {
        None | Some(Value::Null) => return None,
        Some(Value::Object(raw)) => raw,
        Some(_) => {
            shape(problems);
            return None;
        }
    };
    for key in raw.keys() {
        if !FACE_KEYS.contains(&key.as_str()) {
            problems.push(text!("kinds.read.faceKeyNotAllowed", at, key));
        }
    }
    let top = match raw.get("top") {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Array(items)) if items.iter().all(Value::is_string) => items
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect(),
        Some(_) => {
            problems.push(text!("kinds.read.faceTopNotList", at));
            Vec::new()
        }
    };
    let markdown = match raw.get("markdown") {
        None | Some(Value::Null) => None,
        Some(Value::String(name)) => Some(name.clone()),
        Some(_) => {
            problems.push(text!("kinds.read.faceMarkdownNotName", at));
            None
        }
    };
    let mut pages = Vec::new();
    for (key, drawn) in PageKind::ALL {
        match raw.get(key) {
            None | Some(Value::Null) => {}
            Some(Value::String(name)) => pages.push(Page {
                field: name.clone(),
                drawn,
            }),
            Some(_) => problems.push(text!("kinds.read.facePageNotName", at, key)),
        }
    }
    // One thing drawn below the fields at the top: Markdown, or one page.
    if pages.len() + usize::from(markdown.is_some()) > 1 {
        problems.push(text!("kinds.read.faceOneBelow", at));
    }
    // `resizable` is read and let go: every boxed card may carry a size, whatever its face.
    Some(Face {
        top,
        markdown,
        page: pages.into_iter().next(),
    })
}

/// The keys a kind's face is written with (`resizable` read and let go).
const FACE_KEYS: [&str; 5] = ["top", "markdown", "svg", "html", "resizable"];

fn read_kind(
    raw: &Map<String, Value>,
    earlier: &[Kind],
    builtin: bool,
    problems: &mut Vec<String>,
) -> Kind {
    let name = string(raw, "kind").unwrap_or_default();
    let at = if name.is_empty() {
        "kinds".to_string()
    } else {
        name.clone()
    };
    if name.is_empty() {
        problems.push(text!("kinds.read.noKind", at));
    }
    keys_known(raw, &KIND_KEYS, &at, problems);
    // A project's own kind takes the fields every kind has (`base`) when it says no other.
    let extends = string(raw, "extends").or_else(|| (!builtin).then(|| "base".to_string()));
    // The parent's fields and rows come first, then this kind's own.
    let parent = match &extends {
        None => None,
        Some(parent) => {
            let found = earlier.iter().find(|k| &k.name == parent);
            if found.is_none() {
                problems.push(text!("kinds.read.parentMissing", at, parent));
            }
            found
        }
    };
    let mut fields = parent.map(|p| p.fields.clone()).unwrap_or_default();
    let mut rows = parent.map(|p| p.rows.clone()).unwrap_or_default();
    fields.extend(read_fields(
        raw.get("fields"),
        &format!("{at}.fields"),
        problems,
    ));
    rows.extend(read_rows(raw.get("rows"), &format!("{at}.rows"), problems));
    Kind {
        label: string(raw, "label").unwrap_or_else(|| name.clone()),
        name,
        is_abstract: flag(raw, "abstract"),
        extends,
        color: string(raw, "color"),
        header: strings(raw, "header", &at, problems),
        as_row: raw.get("as_row").cloned(),
        connectable: flag(raw, "connectable"),
        on_double_click: string(raw, "on_double_click"),
        holds: strings(raw, "holds", &at, problems),
        opens: {
            let opens = string(raw, "opens");
            if opens.as_deref().is_some_and(|o| !CARD_OPENS.contains(&o)) {
                problems.push(text!(
                    "kinds.read.opensForm",
                    at,
                    opens = crate::text::List::of(CARD_OPENS)
                ));
            }
            opens
        },
        context: merged(parent.map(|p| &p.context), raw.get("context")),
        semantics: merged(parent.map(|p| &p.semantics), raw.get("semantics")),
        face: read_face(raw.get("face"), &at, problems),
        actions: {
            let named = strings(raw, "actions", &at, problems);
            for name in named.iter().filter(|n| !ACTIONS.contains(&n.as_str())) {
                problems.push(text!(
                    "kinds.read.actionUnknown",
                    at,
                    name,
                    known = crate::text::List::of(ACTIONS)
                ));
            }
            named
        },
        fields,
        rows,
        builtin,
        system_only: {
            let said = flag(raw, "system_only");
            if said && !builtin {
                problems.push(text!("kinds.read.systemOnlyBuiltin", at));
            }
            said && builtin
        },
    }
}

/// A parent's object with the child's keys written over it.
fn merged(parent: Option<&Value>, own: Option<&Value>) -> Value {
    match (parent, own) {
        (Some(Value::Object(parent)), Some(Value::Object(own))) => {
            let mut out = parent.clone();
            for (key, value) in own {
                out.insert(key.clone(), value.clone());
            }
            Value::Object(out)
        }
        (_, Some(own)) => own.clone(),
        (Some(parent), None) => parent.clone(),
        (None, None) => Value::Null,
    }
}

const FIELD_KEYS: [&str; 25] = [
    "name",
    "label",
    "description",
    "fieldtype",
    "many",
    "required",
    "when",
    "default",
    "unique",
    "min",
    "max",
    "min_count",
    "max_count",
    "options",
    "optionLabels",
    "pattern",
    "forbid",
    "look",
    "store",
    "paths",
    "kinds",
    "done",
    "show",
    "fields",
    "writing",
];

fn read_fields(value: Option<&Value>, at: &str, problems: &mut Vec<String>) -> Vec<Field> {
    match value {
        None => Vec::new(),
        Some(Value::Array(items)) => items
            .iter()
            .enumerate()
            .filter_map(|(i, raw)| match raw.as_object() {
                Some(raw) => Some(read_field(raw, &format!("{at}[{i}]"), problems)),
                None => {
                    problems.push(text!("kinds.read.fieldNotObject", at, i));
                    None
                }
            })
            .collect(),
        Some(_) => {
            problems.push(text!("kinds.read.fieldsNotList", at));
            Vec::new()
        }
    }
}

fn read_field(raw: &Map<String, Value>, at: &str, problems: &mut Vec<String>) -> Field {
    let name = string(raw, "name").unwrap_or_default();
    let at = if name.is_empty() {
        at.to_string()
    } else {
        text!("kinds.read.placeNamed", at, name)
    };
    if name.is_empty() {
        problems.push(text!("kinds.read.noName", at));
    }
    keys_known(raw, &FIELD_KEYS, &at, problems);
    let typed = string(raw, "fieldtype").unwrap_or_default();
    let fieldtype = FieldType::parse(&typed).unwrap_or_else(|| {
        problems.push(if typed == "pr" {
            // Named anew: a request to merge is a merge request.
            text!("kinds.read.typePr", at)
        } else {
            text!("kinds.read.typeUnknown", at, typed)
        });
        FieldType::Text
    });
    let many = match raw.get("many") {
        None | Some(Value::Bool(false)) => 0,
        Some(Value::Bool(true)) => 1,
        Some(Value::Number(n)) if n.as_u64() == Some(2) => 2,
        Some(_) => {
            problems.push(text!("kinds.read.manyForm", at));
            0
        }
    };
    let number = |key: &str| raw.get(key).and_then(Value::as_f64);
    let count = |key: &str| raw.get(key).and_then(Value::as_u64).map(|n| n as usize);
    let pattern = string(raw, "pattern");
    if pattern.as_deref().is_some_and(|p| p != "identifier") {
        problems.push(text!("kinds.read.patternForm", at));
    }
    let store = string(raw, "store");
    if store.as_deref().is_some_and(|s| s != "id") {
        problems.push(text!("kinds.read.storeForm", at));
    }
    let options = strings(raw, "options", &at, problems);
    // How each value of a choice is shown, the values being what cards keep.
    let option_labels = match raw.get("optionLabels") {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Object(labels))
            if labels
                .iter()
                .all(|(value, label)| label.is_string() && options.contains(value)) =>
        {
            labels
                .iter()
                .map(|(value, label)| (value.clone(), label.as_str().unwrap_or_default().into()))
                .collect()
        }
        Some(_) => {
            problems.push(text!("kinds.read.optionLabelsForm", at));
            Vec::new()
        }
    };
    Field {
        label: string(raw, "label").unwrap_or_else(|| name.clone()),
        description: string(raw, "description").unwrap_or_default(),
        fieldtype,
        many,
        required: Cond::parse(raw.get("required"), Cond::Never, &at, problems),
        when: Cond::parse(raw.get("when"), Cond::Always, &at, problems),
        default: raw.get("default").cloned(),
        unique: flag(raw, "unique"),
        min: number("min"),
        max: number("max"),
        min_count: count("min_count"),
        max_count: count("max_count"),
        options,
        option_labels,
        identifier: pattern.is_some(),
        forbid: string(raw, "forbid").unwrap_or_default(),
        look: flag(raw, "look"),
        by_id: store.is_some(),
        paths: flag(raw, "paths"),
        kinds: strings(raw, "kinds", &at, problems),
        done: raw
            .get("done")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default(),
        show: string(raw, "show"),
        fields: read_fields(raw.get("fields"), &format!("{at}.fields"), problems),
        name,
    }
}

const ROW_KEYS: [&str; 10] = [
    "name",
    "label",
    "address",
    "depth",
    "max_total",
    "when",
    "fields",
    "rows",
    "ref",
    "description",
];

fn read_rows(value: Option<&Value>, at: &str, problems: &mut Vec<String>) -> Vec<Row> {
    read_children(value, at, problems)
        .into_iter()
        .filter_map(|child| match child {
            Child::Row(row) => Some(row),
            Child::Again { name, .. } => {
                problems.push(text!("kinds.read.refOnlyUnderRow", at, name));
                None
            }
        })
        .collect()
}

fn read_children(value: Option<&Value>, at: &str, problems: &mut Vec<String>) -> Vec<Child> {
    let items = match value {
        None => return Vec::new(),
        Some(Value::Array(items)) => items,
        Some(_) => {
            problems.push(text!("kinds.read.rowsNotList", at));
            return Vec::new();
        }
    };
    let mut children = Vec::new();
    for (i, raw) in items.iter().enumerate() {
        let Some(raw) = raw.as_object() else {
            problems.push(text!("kinds.read.rowNotObject", at, i));
            continue;
        };
        let here = format!("{at}[{i}]");
        keys_known(raw, &ROW_KEYS, &here, problems);
        if let Some(name) = string(raw, "ref") {
            children.push(Child::Again {
                when: Cond::parse(raw.get("when"), Cond::Always, &here, problems),
                name,
            });
            continue;
        }
        let name = string(raw, "name").unwrap_or_default();
        if name.is_empty() {
            problems.push(text!("kinds.read.noName", at = here));
        }
        let here = text!("kinds.read.placeNamed", at, name);
        let address = match string(raw, "address").as_deref() {
            Some("name") | None => Address::Name,
            Some("value") => Address::Value,
            Some("index") => Address::Index,
            Some(other) => {
                problems.push(text!("kinds.read.addressForm", here, other));
                Address::Name
            }
        };
        let depth = raw.get("depth").and_then(Value::as_u64).map(|n| n as usize);
        if depth.is_some_and(|d| d == 0 || d > 5) {
            problems.push(text!("kinds.read.depthForm", here));
        }
        children.push(Child::Row(Row {
            label: string(raw, "label").unwrap_or_else(|| name.clone()),
            address,
            depth,
            max_total: raw
                .get("max_total")
                .and_then(Value::as_u64)
                .map(|n| n as usize),
            when: Cond::parse(raw.get("when"), Cond::Always, &here, problems),
            fields: read_fields(raw.get("fields"), &format!("{here}.fields"), problems),
            children: read_children(raw.get("rows"), &format!("{here}.rows"), problems),
            name,
        }));
    }
    children
}

// MARK: - Checking a definition

fn check_kind(kind: &Kind, problems: &mut Vec<String>) {
    let at = &kind.name;
    check_fields(&kind.fields, at, problems);
    let mut names: HashSet<&str> = kind.fields.iter().map(|f| f.name.as_str()).collect();
    for row in &kind.rows {
        if !names.insert(&row.name) {
            problems.push(text!("kinds.check.fieldRowClash", at, name = row.name));
        }
        check_row(row, &[], at, problems);
    }
    for header in &kind.header {
        if kind.field(header).is_none() {
            problems.push(text!("kinds.check.headerNoField", at, header));
        }
    }
    if let Some(face) = &kind.face {
        check_face(kind, face, problems);
    }
}

/// What a kind's face names: fields of the kind (its parents' too), each
/// once; in `top` one a press changes or one written as it is (`FACE_TOP`, one value each), and
/// as `markdown` one of Markdown or of long text.
fn check_face(kind: &Kind, face: &Face, problems: &mut Vec<String>) {
    let at = &kind.name;
    let said = |types: &[FieldType]| crate::text::List::of(types.iter().map(|t| t.name()));
    let mut seen = HashSet::new();
    for name in &face.top {
        if !seen.insert(name.as_str()) {
            problems.push(text!("kinds.check.faceTopDuplicate", at, name));
            continue;
        }
        match kind.field(name) {
            None => problems.push(if kind.row(name).is_some() {
                text!("kinds.check.faceTopIsRow", at, name)
            } else {
                text!("kinds.check.faceTopNoField", at, name)
            }),
            Some(field) if !FACE_TOP.contains(&field.fieldtype) => problems.push(text!(
                "kinds.check.faceTopTypes",
                at,
                types = said(&FACE_TOP),
                name,
                fieldtype = field.fieldtype.name()
            )),
            Some(field) if field.many > 0 => {
                problems.push(text!("kinds.check.faceTopMany", at, name))
            }
            Some(field) if field.look => problems.push(text!("kinds.check.faceTopLook", at, name)),
            Some(_) => {}
        }
    }
    if let Some(name) = &face.markdown {
        let markdown = [FieldType::Markdown, FieldType::LongText];
        match kind.field(name) {
            None => problems.push(text!("kinds.check.faceMarkdownNoField", at, name)),
            Some(field) if !markdown.contains(&field.fieldtype) || field.many > 0 => {
                problems.push(if field.many > 0 {
                    text!(
                        "kinds.check.faceMarkdownTypesMany",
                        at,
                        types = said(&markdown),
                        name,
                        fieldtype = field.fieldtype.name()
                    )
                } else {
                    text!(
                        "kinds.check.faceMarkdownTypes",
                        at,
                        types = said(&markdown),
                        name,
                        fieldtype = field.fieldtype.name()
                    )
                })
            }
            Some(_) => {}
        }
    }
    if let Some(page) = &face.page {
        let key = page.drawn.name();
        let name = &page.field;
        match kind.field(name) {
            None => problems.push(text!("kinds.check.facePageNoField", at, key, name)),
            Some(field) if field.fieldtype != FieldType::LongText || field.many > 0 => problems
                .push(text!(
                    "kinds.check.facePageTypes",
                    at,
                    key,
                    name,
                    fieldtype = field.fieldtype.name()
                )),
            Some(_) => {}
        }
    }
    for name in face.drawn_below() {
        if face.top.contains(name) {
            problems.push(text!("kinds.check.faceBoth", at, name));
        }
    }
}

/// A line's fields, checked as a card's are, and what only a card may hold.
fn check_line_fields(fields: &[Field], at: &str, problems: &mut Vec<String>) {
    check_fields(fields, at, problems);
    defs::line_field_problems(fields, at, problems);
}

fn check_fields(fields: &[Field], at: &str, problems: &mut Vec<String>) {
    let mut seen = HashSet::new();
    for field in fields {
        let here = format!("{at}.{}", field.name);
        if !seen.insert(field.name.as_str()) {
            problems.push(text!("kinds.check.nameDuplicate", here));
        }
        if field.fieldtype.chosen() && field.options.is_empty() {
            problems.push(text!(
                "kinds.check.optionsRequired",
                here,
                fieldtype = field.fieldtype.name()
            ));
        }
        if !field.fieldtype.chosen() && !field.options.is_empty() {
            problems.push(text!("kinds.check.optionsOnlyChosen", here));
        }
        if let (Some(Value::String(default)), true) = (&field.default, field.fieldtype.chosen())
            && !field.options.contains(default)
        {
            problems.push(text!("kinds.check.defaultNotOption", here, default));
        }
        if field.fieldtype == FieldType::Group && field.fields.is_empty() {
            problems.push(text!("kinds.check.groupNeedsFields", here));
        }
        if field.fieldtype != FieldType::Group && !field.fields.is_empty() {
            problems.push(text!("kinds.check.fieldsOnlyGroup", here));
        }
        if field.by_id && field.fieldtype != FieldType::Path {
            problems.push(text!("kinds.check.storeOnlyPath", here));
        }
        if !field.kinds.is_empty() && field.fieldtype != FieldType::Card {
            problems.push(text!("kinds.check.kindsOnlyCard", here));
        }
        for cond in [&field.when, &field.required] {
            check_cond(cond, fields, &here, problems);
        }
        check_fields(&field.fields, &here, problems);
    }
}

/// A condition reads fields beside it that have a set of values to choose from.
fn check_cond(cond: &Cond, fields: &[Field], at: &str, problems: &mut Vec<String>) {
    for (name, values) in cond.reads() {
        match fields.iter().find(|f| f.name == name) {
            None => problems.push(text!("kinds.check.condNoField", at, name)),
            Some(field) if field.fieldtype.chosen() => {
                for value in values {
                    if !value
                        .as_str()
                        .is_some_and(|v| field.options.iter().any(|o| o == v))
                    {
                        problems.push(text!("kinds.check.condValueNotOption", at, name, value));
                    }
                }
            }
            Some(field) if field.fieldtype == FieldType::Checkbox => {
                if values.iter().any(|v| !v.is_boolean()) {
                    problems.push(text!("kinds.check.condBoolean", at, name));
                }
            }
            Some(_) => problems.push(text!("kinds.check.condChosenField", at, name)),
        }
    }
}

fn check_row(row: &Row, ancestors: &[&str], at: &str, problems: &mut Vec<String>) {
    let here = format!("{at}.{}", row.name);
    check_fields(&row.fields, &here, problems);
    match row.address {
        Address::Name if row.fields.iter().all(|f| f.name != "name") => {
            problems.push(text!("kinds.check.addressNameNeedsField", here));
        }
        Address::Value if row.fields.iter().all(|f| f.name != "value") => {
            problems.push(text!("kinds.check.addressValueNeedsField", here));
        }
        _ => {}
    }
    let mut lineage: Vec<&str> = ancestors.to_vec();
    lineage.push(&row.name);
    let mut names = HashSet::new();
    for child in &row.children {
        if !names.insert(child.name()) {
            problems.push(text!(
                "kinds.check.childDuplicate",
                here,
                name = child.name()
            ));
        }
        check_cond(child.when(), &row.fields, &here, problems);
        match child {
            Child::Row(inner) => check_row(inner, &lineage, &here, problems),
            Child::Again { name, .. } => {
                if !lineage.contains(&name.as_str()) {
                    problems.push(text!("kinds.check.refNotAncestor", here, name));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
