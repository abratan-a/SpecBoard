//! A project as the core reads it (`specboard://SpecBoard/Docs/Design:CoreApi`「読む」), typed: what the writers
//! (the exports) work from. Made from the JSON `detail::read` answers with, so the two
//! can never say different things.

use serde::{Deserialize, Deserializer};
use serde_json::{Map, Value};
use std::rc::Rc;

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Detail {
    pub project: Project,
    pub schemas: Vec<std::rc::Rc<Card>>,
    #[serde(default)]
    pub relations: Vec<Relation>,
    #[serde(default)]
    pub sections: Vec<Section>,
    /// The card ghosts — shortcuts to a card of another section — apart from the cards:
    /// they name a card, and are in the name table, but nothing else counts them.
    #[serde(default)]
    pub ghosts: Vec<Ghost>,
    /// The cards put only inside a container (a flow's nodes), apart from the cards: named
    /// in their container, read by what reads containers, and nothing else counts them.
    #[serde(default)]
    pub inner: Vec<std::rc::Rc<Card>>,
}

/// A card ghost: a shortcut a section holds to a card elsewhere, as
/// an alias does to a file. It has a name of its own in its section and holds what it
/// points at by id — a card, or another ghost — so it follows a rename or a move.
#[derive(Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Ghost {
    pub id: String,
    pub name: String,
    pub section_id: Option<String>,
    /// What it points at, by id; none when its document holds nothing.
    pub target: Option<String>,
    pub sort_order: i64,
    /// Where it sits on its own section's diagram, in grid units.
    pub pos_x: Option<f64>,
    pub pos_y: Option<f64>,
    pub pinned: bool,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub name: String,
    pub slug: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub tag_groups: Vec<TagGroup>,
    /// The project's own kinds of card, section and line (`projects.kinds`), as kept: the text
    /// of `{cards, sections, lines}`; none when it has none.
    #[serde(default, deserialize_with = "kept_text")]
    pub kinds: Option<String>,
    /// The language the words an export, a diagram and a document make of their own are said in
    /// (`projects.export_language`).
    #[serde(default = "japanese")]
    pub export_language: String,
}

impl Project {
    /// `f` run in the project's export language: what it writes of its own (a diagram's words, a
    /// document's headings) is said as the project's exports say it, whatever language the call is
    /// in and whoever makes it.
    pub fn in_export_language<T>(&self, f: impl FnOnce() -> T) -> T {
        let _lang = crate::text::bind(Some(&self.export_language));
        f()
    }
}

/// A project's export language when its reading says none: the language of before.
fn japanese() -> String {
    "ja".to_string()
}

/// A JSON value kept as its text.
fn kept_text<'de, D: Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    Ok(Option::<Value>::deserialize(d)?
        .filter(|v| !v.is_null())
        .map(|v| v.to_string()))
}

/// A named bundle of tags; an exclusive one asks each tagged card for exactly one.
#[derive(Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct TagGroup {
    pub name: String,
    pub tags: Vec<String>,
    pub exclusive: bool,
}

/// A rule: its `kind`, then its parameters in the order they were written.
pub type Rule = Map<String, Value>;

#[derive(Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Card {
    pub id: String,
    pub name: String,
    pub label: String,
    pub kind: String,
    pub description: String,
    pub tags: Vec<String>,
    pub payloads: Vec<Payload>,
    pub metadata: Map<String, Value>,
    pub rules: Vec<Rule>,
    pub absent_fields: Vec<AbsentField>,
    pub discriminator: Option<String>,
    pub enum_open: bool,
    pub sort_order: i64,
    pub fields: Vec<Field>,
    pub values: Vec<EnumValue>,
    pub notes: Vec<String>,
    pub body: String,
    pub rows: Vec<FormatRow>,
    pub nodes: Vec<FlowNode>,
    pub holds: Vec<String>,
    pub diagram: Option<String>,
    pub section_id: Option<String>,
    pub custom_rows: Vec<CustomRow>,
    /// Where it sits on its own section's diagram, in grid units.
    pub pos_x: Option<f64>,
    pub pos_y: Option<f64>,
    pub pinned: bool,
    /// The card's own size, in grid units: a width, and a height when one is written.
    pub size: Option<GridSize>,
    /// How a formatdoc is drawn: `rows` or `document`; none for the project's default.
    pub doc_view: Option<String>,
    /// A stackdoc's version open (how it looks), by its name; none for the one on top.
    pub version: Option<String>,
    /// A card of a defined kind (goal, task): its fields and rows in the kind's form.
    pub form: Option<Value>,
    /// A goal's card map: where each card it holds sits on it, by the card's id, in grid
    /// units. Only the cards placed there.
    pub item_places: std::collections::HashMap<String, GridPoint>,
    /// A value or an enum drawn as a card of its own on its section's diagram, not
    /// folded into what holds it (how it looks, apart from the card).
    pub standalone: bool,
    /// Its lines, as its document keeps them (`specboard://SpecBoard/Docs/Design:CardKinds`「線」).
    pub connections: Vec<Connection>,
    /// The container it is inside, by id: a flow for a node, a goal for what it holds.
    #[serde(rename = "in")]
    pub inside: Option<String>,
}

/// A version of a stackdoc: a row of its `versions` — its name for paths, its key for the
/// selector, its heading (the key when none) and its body.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Version {
    pub id: String,
    pub name: String,
    pub key: String,
    pub heading: String,
    pub body: String,
}

impl Version {
    /// Its heading as it is shown: the key when none is written.
    pub fn shown_heading(&self) -> &str {
        if self.heading.trim().is_empty() {
            &self.key
        } else {
            &self.heading
        }
    }
}

/// One line of a card: of a kind of line the project defines, from a row of the card (`from`,
/// empty for its head) to a card or a card ghost (`to`, by id; none for a pointer told
/// apart by its discriminator), landing in it at `at` (its names there, or a path in full
/// elsewhere).
#[derive(Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct Connection {
    pub id: String,
    pub line: String,
    pub from: String,
    pub to: Option<String>,
    pub at: Option<String>,
    pub description: Option<String>,
    pub fields: Map<String, Value>,
}

/// A place in grid units.
#[derive(Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(default)]
pub struct GridPoint {
    pub x: i64,
    pub y: i64,
}

/// A size in grid units: a width, and a height when one is written (none: the height follows
/// the contents).
#[derive(Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(default)]
pub struct GridSize {
    pub w: i64,
    pub h: Option<i64>,
}

/// A custom card's row: its name, the cards it is joined to, and the rows under it.
#[derive(Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct CustomRow {
    #[serde(rename = "type")]
    pub kind: String,
    pub name: String,
    pub links: Vec<String>,
    pub rows: Vec<CustomRow>,
}

impl Card {
    pub fn is_shape(&self) -> bool {
        matches!(self.kind.as_str(), "entity" | "value" | "enum")
    }
    pub fn is_writing(&self) -> bool {
        matches!(
            self.kind.as_str(),
            "note" | "freedoc" | "formatdoc" | "stackdoc"
        )
    }

    /// A stackdoc's versions, in its order (the newest on top; `specboard://SpecBoard/Docs/Design:CardKinds`
    /// 「積み重ね文書」): none for any other kind.
    pub fn versions(&self) -> Vec<Version> {
        if self.kind != "stackdoc" {
            return Vec::new();
        }
        self.form
            .as_ref()
            .and_then(|f| f.get("rows"))
            .and_then(|r| r.get("versions"))
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(|row| {
                let text = |key: &str| row["fields"][key].as_str().unwrap_or_default().to_string();
                Version {
                    id: row["id"].as_str().unwrap_or_default().to_string(),
                    name: text("name"),
                    key: text("key"),
                    heading: text("heading"),
                    body: text("body"),
                }
            })
            .collect()
    }

    /// A data card's values (`instance`;
    /// `specboard://SpecBoard/Docs/Design:CardKinds`「データ（instance）」): the JSON of its shape's
    /// fields, none for any other kind or none written yet.
    /// One of a table's fields (`where`, `columns`, `sort`, `unnest`, `within`), as written.
    pub fn table_field(&self, name: &str) -> Option<&Value> {
        if self.kind != "table" {
            return None;
        }
        self.form
            .as_ref()
            .and_then(|f| f.get("fields"))
            .and_then(|f| f.get(name))
            .filter(|d| !d.is_null())
    }

    pub fn instance_data(&self) -> Option<&Value> {
        if self.kind != "instance" {
            return None;
        }
        self.form
            .as_ref()
            .and_then(|f| f.get("fields"))
            .and_then(|f| f.get("data"))
            .filter(|d| !d.is_null())
    }

    /// A data card's shape, by id: where its line `of` goes.
    pub fn instance_of(&self) -> Option<&str> {
        if self.kind != "instance" {
            return None;
        }
        self.shape_of()
    }

    /// The shape a data card is of, or whose data a table gathers: where its line `of` goes.
    pub fn shape_of(&self) -> Option<&str> {
        if self.kind != "instance" && self.kind != "table" {
            return None;
        }
        self.connections
            .iter()
            .find(|c| c.line == crate::kinds::OF_LINE)
            .and_then(|c| c.to.as_deref())
    }

    /// A stackdoc's version open: the one its look names, else the one on top.
    pub fn open_version(&self) -> Option<Version> {
        let versions = self.versions();
        self.version
            .as_ref()
            .and_then(|name| versions.iter().find(|v| &v.name == name).cloned())
            .or_else(|| versions.into_iter().next())
    }
    /// The shape its flow is drawn in unless asked otherwise.
    pub fn diagram_shape(&self) -> &str {
        self.diagram.as_deref().unwrap_or("flowchart")
    }

    /// Its fields and rows as its document keeps them (`form`), without what the typed
    /// reading adds to a goal's and a task's in the keys of before (the cards a goal holds,
    /// `items`; a task's lines, `links`): what its definition reads.
    pub fn kept_form(&self) -> Value {
        let mut form = self.form.clone().unwrap_or(Value::Null);
        if matches!(self.kind.as_str(), "goal" | "task")
            && let Some(fields) = form.get_mut("fields").and_then(Value::as_object_mut)
        {
            fields.shift_remove("items");
            fields.shift_remove("links");
        }
        form
    }
}

#[derive(Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Field {
    pub id: String,
    pub name: String,
    pub label: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub ref_schema_id: Option<String>,
    pub points_at: Vec<PointsAt>,
    pub is_array: bool,
    pub is_required: bool,
    pub is_nullable: bool,
    /// A fixed value; none, or JSON null, for none.
    pub const_value: Option<Value>,
    pub is_unique: bool,
    pub is_pk: bool,
    pub description: String,
    pub rules: Vec<Rule>,
    pub variants: Vec<String>,
    /// The other JSON types it takes, besides its own (`string`, `integer`, `number`, `boolean`,
    /// `object`; `array` for its own type's array too): what a tool's `target` that is one name or
    /// a list is.
    pub also: Vec<String>,
    pub sort_order: i64,
    pub children: Vec<Field>,
}

impl Field {
    pub fn is_ref(&self) -> bool {
        self.kind == "ref"
    }
    pub fn is_object(&self) -> bool {
        self.kind == "object"
    }
    /// Its fixed value, when it has one that is not null.
    pub fn fixed(&self) -> Option<&Value> {
        self.const_value.as_ref().filter(|v| !v.is_null())
    }
}

#[derive(Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct PointsAt {
    pub schema: Option<String>,
    #[serde(rename = "as")]
    pub designation: String,
    #[serde(rename = "in")]
    pub exists_in: Option<String>,
    pub when: Option<When>,
    pub discriminated_by: Option<String>,
    pub discriminator_field: Option<String>,
    pub cardinality: Option<String>,
}

#[derive(Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct When {
    pub field: String,
    pub equals: String,
}

#[derive(Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct EnumValue {
    pub id: String,
    pub value: String,
    pub label: String,
    /// What the value stands for, in one line.
    pub description: String,
}

#[derive(Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct AbsentField {
    pub name: String,
    pub reason: String,
}

#[derive(Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Payload {
    pub name: String,
    pub excludes: Vec<String>,
    pub value_at: Option<String>,
    pub signed_by: Vec<String>,
    pub verified_by: Vec<String>,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct ExportSetting {
    /// `file`, `default` (the section's), or `none`.
    pub kind: String,
    pub dir: Option<String>,
    pub path: String,
    pub format: Option<String>,
}

impl Default for ExportSetting {
    fn default() -> Self {
        ExportSetting {
            kind: "file".into(),
            dir: None,
            path: String::new(),
            format: None,
        }
    }
}

pub const CODE_FORMATS: [&str; 3] = ["typescript", "zod", "json-schema"];

/// The format of an export that writes the data cards (`instance`) as JSON, a list of their data, the
/// forms their lines of `export: {as: schema}` reach put in as JSON Schema (`export::data`).
/// Not a language, so not in `CODE_FORMATS`; never said by an extension, so always written.
pub const JSON_DATA: &str = "json-data";

impl ExportSetting {
    /// The format a path's extension says, for a schema.
    pub fn format_of(path: &str) -> Option<&'static str> {
        let lower = path.to_lowercase();
        if lower.ends_with(".zod.ts") {
            Some("zod")
        } else if lower.ends_with(".ts") {
            Some("typescript")
        } else if lower.ends_with(".json") {
            Some("json-schema")
        } else {
            None
        }
    }

    /// The format this setting writes a schema in.
    pub fn schema_format(&self) -> Option<String> {
        self.format
            .clone()
            .or_else(|| Self::format_of(&self.path).map(str::to_string))
    }

    /// Why a path cannot be written to, or none: relative, under the directory, plain.
    pub fn path_problem(raw: &str) -> Option<String> {
        let path = raw.trim_matches(|c: char| c == ' ' || c == '\t');
        if path.is_empty() {
            return Some(text!("model.path.empty"));
        }
        if path.starts_with('/') || path.starts_with('~') {
            return Some(text!("model.path.notRelative"));
        }
        let parts: Vec<&str> = path.split('/').collect();
        if parts.iter().any(|p| *p == ".." || *p == ".") {
            return Some(text!("model.path.dots"));
        }
        if parts[..parts.len() - 1].iter().any(|p| p.is_empty()) {
            return Some(text!("model.path.emptyFolder"));
        }
        if path.contains('\\') || path.chars().any(char::is_control) {
            return Some(text!("model.path.backslash"));
        }
        None
    }

    /// Why this setting cannot be a card's of `kind`, or none.
    pub fn problem_for(&self, kind: &str) -> Option<String> {
        match self.kind.as_str() {
            "none" | "default" => return None,
            _ => {}
        }
        if self.dir.is_none() {
            return Some(text!("model.problem.noDirectory"));
        }
        if let Some(problem) = Self::path_problem(&self.path) {
            return Some(problem);
        }
        let folder = self.path.ends_with('/');
        let markdown = self.path.to_lowercase().ends_with(".md");
        match kind {
            "note" => {
                if folder || markdown {
                    return self
                        .format
                        .as_ref()
                        .map(|_| text!("model.problem.noteMarkdownFormat"));
                }
                let Some(format) = self.schema_format() else {
                    return Some(text!("model.problem.noteTarget"));
                };
                if !CODE_FORMATS.contains(&format.as_str()) {
                    return Some(text!("model.problem.codeFormat", format));
                }
                None
            }
            "freedoc" | "formatdoc" | "stackdoc" => {
                if !folder && !markdown {
                    return Some(text!("model.problem.docTarget"));
                }
                self.format
                    .as_ref()
                    .map(|_| text!("model.problem.docFormat"))
            }
            "instance" => {
                if self.schema_format().as_deref() != Some(JSON_DATA) {
                    return Some(text!("model.problem.instanceFormat"));
                }
                if folder || !self.path.to_lowercase().ends_with(".json") {
                    return Some(text!("model.problem.instanceFile"));
                }
                None
            }
            "svg" | "html" => {
                let extension = format!(".{kind}");
                if !folder && !self.path.to_lowercase().ends_with(&extension) {
                    return Some(text!("model.problem.pageFile", kind, extension));
                }
                self.format
                    .as_ref()
                    .map(|_| text!("model.problem.pageFormat", kind))
            }
            "action" | "flow" => {
                if folder || !markdown {
                    return Some(text!("model.problem.chartFile"));
                }
                self.format
                    .as_ref()
                    .map(|_| text!("model.problem.chartFormat"))
            }
            _ => {
                if self.schema_format().as_deref() == Some(JSON_DATA) {
                    return Some(text!("model.problem.jsonDataOnlyInstance"));
                }
                if folder {
                    return Some(text!("model.problem.schemaOneFile"));
                }
                let Some(format) = self.schema_format() else {
                    return Some(text!("model.problem.schemaFormat"));
                };
                if !CODE_FORMATS.contains(&format.as_str()) {
                    return Some(text!("model.problem.codeFormat", format));
                }
                None
            }
        }
    }
}

#[derive(Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Section {
    pub id: String,
    pub name: String,
    pub description: String,
    pub parent_id: Option<String>,
    pub kind: Option<String>,
    /// A working directory's directory that a note is put in but was not read here
    /// (`workdir::compose_part`): kept in its place so that the note stays where it was put.
    pub missing: bool,
}

#[derive(Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Relation {
    pub from_schema_id: String,
    pub to_schema_id: String,
    pub field_id: String,
    pub path: String,
    pub cardinality: Option<String>,
    pub ref_kind: String,
    pub ref_target: Option<String>,
    pub is_array: bool,
    /// The card ghost the reference was written through (its target is the card the
    /// ghosts lead to); none when it names the card itself.
    pub via_ghost_id: Option<String>,
    /// Its kind of line, by name.
    pub line: String,
    /// The row it leaves; empty for the card's head.
    pub from: String,
    /// One the diagrams draw in the form it had before the definitions: what a field
    /// embeds or names as `x-pointsAt` says (the exports' mermaid reads it so).
    pub legacy: bool,
}

/// One, or several to part the way.
fn nexts<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<String>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Next {
        One(String),
        Many(Vec<String>),
    }
    Ok(match Option::<Next>::deserialize(d)? {
        None => Vec::new(),
        Some(Next::One(s)) => vec![s],
        Some(Next::Many(v)) => v,
    })
}

#[derive(Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct FlowNode {
    pub name: String,
    pub kind: String,
    pub label: Option<String>,
    pub description: Option<String>,
    pub trigger: Option<String>,
    pub event: Option<FlowEvent>,
    pub every: Option<String>,
    pub target: Option<String>,
    pub targets: Vec<String>,
    pub join: Option<String>,
    pub partial: Option<String>,
    pub on_error: Option<String>,
    pub value: Option<String>,
    pub branches: Vec<FlowBranch>,
    #[serde(rename = "else")]
    pub otherwise: Option<String>,
    #[serde(rename = "for")]
    pub wait_for: Option<String>,
    pub timeout: Option<String>,
    pub on_timeout: Option<String>,
    pub outcome: Option<String>,
    #[serde(rename = "return")]
    pub reply: Option<FlowReturn>,
    pub steps: Vec<Step>,
    /// The flows its peer lines name, in the lines' order.
    pub peers: Vec<String>,
    pub program: Option<String>,
    pub carries: Vec<String>,
    pub returns: Vec<String>,
    pub via: Option<String>,
    pub initiator: Option<String>,
    pub verify_with: Option<String>,
    pub sign_with: Option<String>,
    #[serde(rename = "next", deserialize_with = "nexts")]
    pub nexts: Vec<String>,
    /// Where it sits on its flow's card map, in grid units.
    #[serde(rename = "topLeft")]
    pub top_left: Option<GridPoint>,
}

/// How a node goes on.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Exit {
    Next,
    Branch,
    Otherwise,
    Error,
    Timeout,
}

impl FlowNode {
    /// Where it goes, each way with its label (`FlowNode.exits`).
    pub fn exits(&self) -> Vec<(String, Option<String>, Exit)> {
        let mut out = Vec::new();
        for next in &self.nexts {
            out.push((next.clone(), None, Exit::Next));
        }
        for branch in &self.branches {
            out.push((branch.next.clone(), Some(branch.when.clone()), Exit::Branch));
        }
        if let Some(o) = &self.otherwise {
            out.push((o.clone(), Some("else".into()), Exit::Otherwise));
        }
        if let Some(e) = &self.on_error {
            out.push((e.clone(), Some(text!("model.exit.onError")), Exit::Error));
        }
        if let Some(t) = &self.on_timeout {
            out.push((
                t.clone(),
                Some(text!("model.exit.onTimeout")),
                Exit::Timeout,
            ));
        }
        out
    }
}

#[derive(Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct FlowEvent {
    pub flow: String,
    pub outcome: String,
}

#[derive(Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct FlowBranch {
    pub when: String,
    pub next: String,
}

#[derive(Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct FlowReturn {
    pub carries: Vec<String>,
    pub peers: Vec<String>,
    pub program: Option<String>,
    pub sign_with: Option<String>,
}

#[derive(Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Step {
    #[serde(rename = "do")]
    pub op: String,
    pub peers: Vec<String>,
    pub program: Option<String>,
    pub carries: Vec<String>,
    pub returns: Vec<String>,
    pub paths: Vec<String>,
    pub key: Option<String>,
    pub sign_with: Option<String>,
    pub verify_with: Option<String>,
    pub via: Option<String>,
    pub initiator: Option<String>,
    pub action: Option<String>,
    pub when: Option<String>,
    pub over: Option<String>,
    pub description: Option<String>,
    pub steps: Vec<Step>,
    pub cases: Vec<Step>,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(default)]
pub struct FormatRow {
    pub name: String,
    pub kind: String,
    pub heading: String,
    pub body: String,
    pub rows: Vec<FormatRow>,
    pub columns: Vec<String>,
    pub cells: Vec<Vec<String>>,
    pub target: String,
}

impl FormatRow {
    /// The Markdown it says, each with where (`FormatRow.texts`): a text's body, a
    /// table's columns and cells.
    pub fn texts(&self) -> Vec<(String, String)> {
        match self.kind.as_str() {
            "text" => vec![("body".to_string(), self.body.clone())],
            "table" => {
                let mut out: Vec<(String, String)> = self
                    .columns
                    .iter()
                    .enumerate()
                    .map(|(i, c)| (format!("columns[{i}]"), c.clone()))
                    .collect();
                for (r, line) in self.cells.iter().enumerate() {
                    for (c, cell) in line.iter().enumerate() {
                        out.push((format!("cells[{r}][{c}]"), cell.clone()));
                    }
                }
                out
            }
            _ => Vec::new(),
        }
    }
}

impl Default for FormatRow {
    fn default() -> Self {
        FormatRow {
            name: String::new(),
            kind: "item".into(),
            heading: String::new(),
            body: String::new(),
            rows: Vec::new(),
            columns: Vec::new(),
            cells: Vec::new(),
            target: String::new(),
        }
    }
}

/// The project as typed, from what `detail::read` answers.
pub fn typed(detail: &Value) -> Result<Detail, String> {
    // Read from the JSON where it is, not from a copy of it (a large project's copy would be as
    // large again, and take as long to make).
    Detail::deserialize(detail).map_err(|e| text!("model.error.read", e))
}

/// `typed` of a reading made from the one before (`detail::of_made_over`): the cards, the inner
/// cards and the lines of those not worked out again (`dirty` says those that are) taken from the
/// typed reading before (`before`) in their order — those of the cards worked out again or gone
/// (`written`) passed over —, the rest read from the JSON: what `typed` of it all makes.
pub fn typed_over(
    detail: &Value,
    before: &mut Detail,
    dirty: &std::collections::HashSet<String>,
    written: &std::collections::HashSet<String>,
) -> Result<Detail, String> {
    use std::collections::VecDeque;
    let said = |e: serde_json::Error| text!("model.error.read", e);
    let list = |key: &str| detail[key].as_array().map(Vec::as_slice).unwrap_or(&[]);
    let passed = |id: &str| dirty.contains(id) || written.contains(id);
    let taken = |entries: &[Value], kept: Vec<Rc<Card>>| -> Result<Vec<Rc<Card>>, String> {
        let mut kept: VecDeque<Rc<Card>> = kept.into();
        entries
            .iter()
            .map(|v| {
                let id = v["id"].as_str().unwrap_or_default();
                if dirty.contains(id) {
                    return Card::deserialize(v).map(Rc::new).map_err(said);
                }
                while let Some(front) = kept.front() {
                    if front.id == id {
                        return Ok(kept.pop_front().expect("the front was just seen"));
                    }
                    if !passed(&front.id) {
                        break;
                    }
                    kept.pop_front();
                }
                Err(text!("model.error.cardNotBefore", id))
            })
            .collect()
    };
    let schemas = taken(list("schemas"), std::mem::take(&mut before.schemas))?;
    let inner = taken(list("inner"), std::mem::take(&mut before.inner))?;
    let mut lines: VecDeque<Relation> = std::mem::take(&mut before.relations).into();
    let relations = list("relations")
        .iter()
        .map(|v| {
            let from = v["fromSchemaId"].as_str().unwrap_or_default();
            if dirty.contains(from) {
                return Relation::deserialize(v).map_err(said);
            }
            while let Some(front) = lines.front() {
                if front.from_schema_id == from {
                    return Ok(lines.pop_front().expect("the front was just seen"));
                }
                if !passed(&front.from_schema_id) {
                    break;
                }
                lines.pop_front();
            }
            Err(text!("model.error.linesNotBefore", from))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Detail {
        project: Project::deserialize(&detail["project"]).map_err(said)?,
        schemas,
        relations,
        sections: Vec::<Section>::deserialize(&detail["sections"]).map_err(said)?,
        ghosts: Vec::<Ghost>::deserialize(&detail["ghosts"]).map_err(said)?,
        inner,
    })
}

/// `typed` of a reading brought up in place (`detail::bring_over`): the cards and inner cards of
/// `dirty` read again from the JSON where they are, and the lines of the cards they leave; the
/// rest as they were.
pub fn typed_in_place(
    detail: &Value,
    typed: &mut Detail,
    dirty: &std::collections::HashSet<String>,
) -> Result<(), String> {
    use std::collections::{HashMap, VecDeque};
    let said = |e: serde_json::Error| text!("model.error.read", e);
    let list = |key: &str| detail[key].as_array().map(Vec::as_slice).unwrap_or(&[]);
    for (entries, cards) in [
        (list("schemas"), &mut typed.schemas),
        (list("inner"), &mut typed.inner),
    ] {
        if entries.len() != cards.len() {
            return Err(text!("model.error.cardCount"));
        }
        for (entry, card) in entries.iter().zip(cards.iter_mut()) {
            let id = entry["id"].as_str().unwrap_or_default();
            if card.id != id {
                return Err(text!("model.error.cardOrder"));
            }
            if dirty.contains(id) {
                *card = Rc::new(Card::deserialize(entry).map_err(said)?);
            }
        }
    }
    let mut lines: HashMap<String, VecDeque<Relation>> = HashMap::new();
    for r in typed.relations.drain(..) {
        lines
            .entry(r.from_schema_id.clone())
            .or_default()
            .push_back(r);
    }
    typed.relations = list("relations")
        .iter()
        .map(|v| {
            let from = v["fromSchemaId"].as_str().unwrap_or_default();
            match dirty.contains(from) {
                true => Relation::deserialize(v).map_err(said),
                false => lines
                    .get_mut(from)
                    .and_then(VecDeque::pop_front)
                    .ok_or_else(|| text!("model.error.linesNotBefore", from)),
            }
        })
        .collect::<Result<Vec<_>, _>>()?;
    typed.project = Project::deserialize(&detail["project"]).map_err(said)?;
    Ok(())
}
