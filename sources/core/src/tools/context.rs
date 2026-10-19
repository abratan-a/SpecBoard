//! What a goal or a task needs, read in one call (`card_read` with `as: context`,
//! `specboard://SpecBoard/Docs/Design:CoreApi`「材料を集める」): its work (its body, and a goal's
//! open cards inside), the spec its bodies and links point at (`refs`), the shapes those point at
//! (`shapes`) and the code that carries them out (`code`) — resolved in the tree the goal works in
//! (its `workBranch` while that branch is there, else main), since the board points at main by id
//! and at a branch's cards only by the paths written in its bodies. One hop each, every item once
//! with where it was reached from, the whole within the characters an answer is given in; what does
//! not fit is said with a call that reads it.

use super::args::Args;
use super::views::Projection;
use super::walk::{self, Target};
use super::{Seen, paging};
use crate::export::Project;
use crate::mcp::Output;
use crate::model::Card;
use crate::store::{BOARD, BOARD_NAME, Store};
use serde_json::{Map, Value, json};

/// The most characters one text of an item is given in before it is cut at a line.
const ITEM_CHARS: usize = 8_000;

/// The most items said to be left out; the rest are counted.
const MOST_LEFT: usize = 100;

/// What is kept free of the answer's room for the counts said after the items.
const TAIL_ROOM: usize = 200;

/// One thing gathered: where it is, what it is, where it was reached from, and what it says.
struct Entry {
    path: String,
    kind: String,
    via: Vec<String>,
    said: Map<String, Value>,
    /// The call that reads it whole.
    read: Value,
    /// Said as a card's JSON, which is not cut: too long, it is left out whole.
    whole: bool,
}

/// The layers an answer gathers, in the order they are put in.
#[derive(Default)]
struct Gathered {
    refs: Vec<Entry>,
    shapes: Vec<Entry>,
    code: Vec<Entry>,
    unresolved: Vec<(String, String)>,
    /// Not followed: a board's card, or code whose working directory was not read here.
    left: Vec<Value>,
}

impl Gathered {
    /// Whether `path` is gathered already in any layer; if so, `via` is added to where it was
    /// reached from.
    fn seen(&mut self, path: &str, via: &str) -> bool {
        for entry in self
            .refs
            .iter_mut()
            .chain(self.shapes.iter_mut())
            .chain(self.code.iter_mut())
        {
            if entry.path == path {
                if !entry.via.iter().any(|v| v == via) {
                    entry.via.push(via.to_string());
                }
                return true;
            }
        }
        self.unresolved.iter().any(|(p, _)| p == path)
            || self.left.iter().any(|l| l["path"] == path)
    }
}

/// How the tree is called in a call that reads it: none for main.
fn branch_argument(tree: &str) -> Option<String> {
    (tree != "main").then(|| tree.to_string())
}

fn call(project: &str, tree: Option<&str>, target: &str, value: Option<Value>) -> Value {
    let mut arguments = Map::new();
    arguments.insert("project".into(), project.into());
    if let Some(tree) = tree {
        arguments.insert("branch".into(), tree.into());
    }
    arguments.insert("target".into(), target.into());
    if let Some(value) = value {
        arguments.insert("value".into(), value);
    }
    json!({"tool": "card_read", "arguments": arguments})
}

/// A card's value of a field of its kind's, as its document holds it.
fn field<'c>(card: &'c Card, name: &str) -> Option<&'c Value> {
    card.form.as_ref()?.get("fields")?.get(name)
}

/// A goal's or a task's body: its own, or the one its kind keeps as a field.
fn body_of(card: &Card) -> String {
    if !card.body.is_empty() {
        return card.body.clone();
    }
    field(card, "body")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn is_work(card: &Card) -> bool {
    card.kind == "goal" || card.kind == "task"
}

fn is_shape(card: &Card) -> bool {
    matches!(card.kind.as_str(), "entity" | "value" | "enum")
}

/// `card_read` with `as: context`: the board's goal or task `list` names, its work and what that
/// points at, resolved in the tree its goal works in.
pub(crate) fn read(
    store: &Store,
    seen: &Seen,
    list: &[String],
    given: &Map<String, Value>,
    args: &Args,
) -> Result<Output, String> {
    super::check(given, "card_read", "as: context")?;
    let board_project = seen.project();
    if seen.named.tree != BOARD {
        let board = super::cross::board(&store.reader(), &seen.summary.id)?;
        let on_board = Project::new(&board);
        let found = list
            .iter()
            .any(|p| matches!(walk::locate(&on_board, p, None), Ok(Target::Card(c)) if is_work(c)));
        let message = text!("reads.context.notBoard", tree = seen.named.name.clone());
        if !found {
            return Err(message);
        }
        let mut arguments = args.raw.clone();
        arguments.insert("branch".into(), BOARD_NAME.into());
        return Err(crate::mcp::refusal(
            message,
            json!({ "use": { "tool": "card_read", "arguments": arguments } }),
        ));
    }
    let [path] = list else {
        return Err(text!("reads.context.oneTarget", count = list.len()));
    };
    let card = match walk::locate(&board_project, path, None)? {
        Target::Card(card) if is_work(card) => card,
        other => {
            let (path, kind) = match &other {
                Target::Card(c) => (board_project.full(c), c.kind.clone()),
                _ => (path.clone(), String::new()),
            };
            return Err(text!("reads.context.notWork", path, kind));
        }
    };
    let project_name = seen.summary.name.clone();

    let goal = enclosing(card, &board_project);
    let named_branch = goal
        .and_then(|g| field(g, "workBranch"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|b| !b.is_empty());
    let live = match named_branch {
        Some(name) => super::branches(&store.conn, &seen.summary.id)?
            .into_iter()
            .find(|(_, n)| crate::pathtext::same(n, name))
            .map(|(_, n)| n),
        None => None,
    };
    let tree_note = match (named_branch, &live) {
        (_, Some(_)) => None,
        (None, None) => Some(text!("reads.context.treeUnset")),
        (Some(name), None) => Some(
            if super::actions::aside_of(&store.reader(), &seen.summary.id, name)?.is_some() {
                text!("reads.context.treeAside", name)
            } else {
                text!("reads.context.treeGone", name)
            },
        ),
    };
    let tree_name = live.clone().unwrap_or_else(|| "main".to_string());
    // The code its lines reach is read from the files: the tree with its working directories whole.
    let _whole = crate::workdir::Whole::at_least(crate::workdir::Reach::In);
    let tree = Seen::read(store, &seen.summary.id, live.as_deref())?;
    let tp = tree.project();
    let at_tree = branch_argument(&tree_name);

    // Work: the card, and a goal's open cards inside by their order; a task's goal.
    let (cards, done_tasks) = work_cards(card, &board_project);
    let work_entry = |c: &Card| -> Entry {
        let mut said = Map::new();
        if !c.label.is_empty() {
            said.insert("label".into(), c.label.clone().into());
        }
        if c.kind == "goal" {
            if let Some(status) = field(c, "status") {
                said.insert("status".into(), status.clone());
            }
        } else {
            said.insert(
                "done".into(),
                field(c, "done").cloned().unwrap_or(Value::Bool(false)),
            );
        }
        said.insert("body".into(), body_of(c).into());
        let path = board_project.full(c);
        Entry {
            read: call(&project_name, Some(BOARD_NAME), &path, None),
            path,
            kind: c.kind.clone(),
            via: Vec::new(),
            said,
            whole: false,
        }
    };
    let work: Vec<Entry> = cards.iter().map(|c| work_entry(c)).collect();
    let goal_said = (card.kind == "task").then_some(goal).flatten().map(|g| {
        let mut out = Map::new();
        out.insert("path".into(), board_project.full(g).into());
        if !g.label.is_empty() {
            out.insert("label".into(), g.label.clone().into());
        }
        for key in ["status", "workBranch"] {
            if let Some(v) = field(g, key) {
                out.insert(key.into(), v.clone());
            }
        }
        Value::Object(out)
    });

    let mut gathered = Gathered::default();
    let ctx = Context {
        tp: &tp,
        tree: &tree,
        board: &board_project,
        project: &project_name,
        at_tree: at_tree.as_deref(),
    };
    // Refs: the paths written in the work's bodies and its links, in the order they come.
    for (entry, c) in work.iter().zip(&cards) {
        let via = entry.path.clone();
        let body = entry.said["body"].as_str().unwrap_or_default().to_string();
        for written in crate::notetext::refs(&body) {
            let path = crate::pathtext::canonical(written.path.trim()).into_owned();
            ctx.follow(&mut gathered, &path, &via, Layer::Refs);
        }
        for line in c.connections.iter().filter(|l| l.line == "link") {
            let Some(to) = line.to.as_deref() else {
                continue;
            };
            match ctx.path_of_id(to, line.at.as_deref()) {
                Some(path) => ctx.follow(&mut gathered, &path, &via, Layer::Refs),
                None => gathered.unresolved.push((to.to_string(), via.clone())),
            }
        }
    }
    // Shapes and code: what the refs point at, once.
    let reached: Vec<(String, Vec<Pointed>)> = gathered
        .refs
        .iter()
        .map(|e| (e.path.clone(), ctx.pointed_from(e)))
        .collect();
    for (via, pointed) in reached {
        for (line, path) in pointed {
            let layer = if line.as_deref() == Some("implementedBy") {
                Layer::Code
            } else {
                Layer::Shapes
            };
            ctx.follow(&mut gathered, &path, &via, layer);
        }
    }
    let from_shapes: Vec<(String, Vec<Pointed>)> = gathered
        .shapes
        .iter()
        .map(|e| (e.path.clone(), ctx.pointed_from(e)))
        .collect();
    for (via, pointed) in from_shapes {
        for (line, path) in pointed {
            if line.as_deref() == Some("implementedBy") {
                ctx.follow(&mut gathered, &path, &via, Layer::Code);
            }
        }
    }

    // Put in by the room the answer has: work, refs, shapes, code.
    let mut out = Map::new();
    out.insert("project".into(), project_name.clone().into());
    out.insert("path".into(), board_project.full(card).into());
    out.insert("kind".into(), card.kind.clone().into());
    if !card.label.is_empty() {
        out.insert("label".into(), card.label.clone().into());
    }
    out.insert("tree".into(), tree_name.clone().into());
    if let Some(note) = tree_note {
        out.insert("treeNote".into(), note.into());
    }
    if let Some(goal) = goal_said {
        out.insert("goal".into(), goal);
    }
    if !done_tasks.is_empty() {
        out.insert("doneTasks".into(), done_tasks.into());
    }
    if !gathered.unresolved.is_empty() {
        out.insert(
            "unresolved".into(),
            Value::Array(
                gathered
                    .unresolved
                    .iter()
                    .take(MOST_LEFT)
                    .map(|(p, v)| json!({"path": p, "via": v}))
                    .collect(),
            ),
        );
    }
    let mut left: Vec<Value> = std::mem::take(&mut gathered.left);
    let mut used = paging::chars(&Value::Object(out.clone())) + TAIL_ROOM;
    used += left.iter().map(paging::chars).sum::<usize>();
    let mut left_more = 0usize;
    let mut lost = false;
    let layers = [
        ("work", work),
        ("refs", gathered.refs),
        ("shapes", gathered.shapes),
        ("code", gathered.code),
    ];
    for (name, entries) in layers {
        let mut put: Vec<Value> = Vec::new();
        for entry in entries {
            let said = entry.said_cut();
            let size = said.as_ref().map_or(usize::MAX, |s| paging::chars(s) + 1);
            if let Some(said) = said
                && used + size <= paging::MOST_CHARS
            {
                used += size;
                put.push(said);
                continue;
            }
            lost = true;
            let row = json!({
                "path": entry.path,
                "kind": entry.kind,
                "chars": paging::chars(&entry.whole_said()),
                "via": entry.via,
                "read": entry.read,
            });
            let size = paging::chars(&row) + 1;
            if left.len() < MOST_LEFT && used + size <= paging::MOST_CHARS {
                used += size;
                left.push(row);
            } else {
                left_more += 1;
            }
        }
        out.insert(name.into(), Value::Array(put));
    }
    if !left.is_empty() {
        out.insert("left".into(), Value::Array(left));
    }
    if left_more > 0 {
        out.insert("leftMore".into(), left_more.into());
    }
    if lost {
        out.insert("truncated".into(), true.into());
    }
    Ok(seen.headed(Output::Json(Value::Object(out))))
}

/// The goal `from` is worked under: itself (or the goal holding a task), else the nearest one out
/// that names a branch to work in; the nearest goal when none does.
fn enclosing<'a>(from: &'a Card, board: &Project<'a>) -> Option<&'a Card> {
    let mut at = if from.kind == "goal" {
        Some(from)
    } else {
        from.inside.as_deref().and_then(|i| board.card(i))
    };
    let mut steps = 0;
    let mut first: Option<&Card> = None;
    while let Some(here) = at
        && steps < 64
    {
        if here.kind == "goal" {
            first.get_or_insert(here);
            if field(here, "workBranch")
                .and_then(Value::as_str)
                .is_some_and(|b| !b.trim().is_empty())
            {
                return Some(here);
            }
        }
        at = here.inside.as_deref().and_then(|i| board.card(i));
        steps += 1;
    }
    first
}

/// The cards of the work: the card, and a goal's open goals and tasks inside, by their order; and
/// the paths of those inside that are done (not looked into).
fn work_cards<'a>(card: &'a Card, board: &Project<'a>) -> (Vec<&'a Card>, Vec<String>) {
    let mut out = vec![card];
    let mut done = Vec::new();
    if card.kind == "goal" {
        let mut stack: Vec<&Card> = board.inner_of(&card.id);
        stack.reverse();
        while let Some(inner) = stack.pop() {
            if super::cross::finished(board, inner) {
                done.push(board.full(inner));
                continue;
            }
            if is_work(inner) {
                out.push(inner);
            }
            let mut deeper = board.inner_of(&inner.id);
            deeper.reverse();
            stack.extend(deeper);
        }
    }
    (out, done)
}

impl Entry {
    /// As it is put in the answer.
    fn whole_said(&self) -> Value {
        let mut out = Map::new();
        out.insert("path".into(), self.path.clone().into());
        out.insert("kind".into(), self.kind.clone().into());
        if !self.via.is_empty() {
            out.insert("via".into(), self.via.clone().into());
        }
        for (k, v) in &self.said {
            out.insert(k.clone(), v.clone());
        }
        Value::Object(out)
    }

    /// As it is put in the answer, its long text cut at a line with a call that reads on; none
    /// when it is a card's JSON too long to be given.
    fn said_cut(&self) -> Option<Value> {
        let mut out = self.whole_said();
        let text_key = ["markdown", "body"]
            .into_iter()
            .find(|k| out.get(*k).and_then(Value::as_str).is_some());
        if let Some(key) = text_key {
            let text = out[key].as_str().unwrap_or_default().to_string();
            if text.chars().count() > ITEM_CHARS {
                let lines: Vec<&str> = text.split('\n').collect();
                let mut kept = 0;
                let mut chars = 0;
                for line in &lines {
                    let more = line.chars().count() + 1;
                    if chars + more > ITEM_CHARS && kept > 0 {
                        break;
                    }
                    chars += more;
                    kept += 1;
                }
                out[key] = lines[..kept].join("\n").into();
                let mut read = self.read.clone();
                let mut value = read["arguments"]
                    .get("value")
                    .cloned()
                    .unwrap_or_else(|| json!({}));
                value["lines"] = json!({"from": kept + 1});
                read["arguments"]["value"] = value;
                out["cut"] = json!({"lineCount": lines.len(), "read": read});
            }
        } else if self.whole && paging::chars(&out) > ITEM_CHARS {
            return None;
        }
        Some(out)
    }
}

/// A path an item points at, with the kind of line it is drawn by (none: written in its text).
type Pointed = (Option<String>, String);

#[derive(Clone, Copy, PartialEq, Eq)]
enum Layer {
    Refs,
    Shapes,
    Code,
}

/// What the gathering reads in: the tree resolved in, and the board beside it.
struct Context<'a> {
    tp: &'a Project<'a>,
    tree: &'a Seen,
    board: &'a Project<'a>,
    project: &'a str,
    at_tree: Option<&'a str>,
}

impl Context<'_> {
    /// A card's path in the tree by its id (a board's line keeps main's), with the place in it.
    fn path_of_id(&self, id: &str, at: Option<&str>) -> Option<String> {
        let base = if let Some(card) = self.tp.card(id) {
            self.tp.full(card)
        } else if let Some(card) = self.board.card(id) {
            self.board.full(card)
        } else if self.tp.file_card(id).is_some() || id.starts_with("file:") {
            self.tp.file_written(id)?
        } else {
            return None;
        };
        Some(match at.filter(|a| !a.is_empty()) {
            Some(at) => format!("{base}.{at}"),
            None => base,
        })
    }

    /// `path`, reached from `via`, put in its layer — or with what is left or unresolved.
    fn follow(&self, gathered: &mut Gathered, path: &str, via: &str, layer: Layer) {
        if path.is_empty() || gathered.seen(path, via) {
            return;
        }
        let target = match walk::locate(self.tp, path, None) {
            Ok(target) => target,
            Err(_) => {
                // A board's card: not followed.
                if let Ok(Target::Card(c)) = walk::locate(self.board, path, None) {
                    gathered.left.push(json!({
                        "path": self.board.full(c),
                        "kind": c.kind,
                        "via": [via],
                        "read": call(self.project, Some(BOARD_NAME), &self.board.full(c), None),
                    }));
                    return;
                }
                // Code whose working directory was not read here.
                if let Some(row) = self.not_read(path, via) {
                    gathered.left.push(row);
                    return;
                }
                gathered
                    .unresolved
                    .push((path.to_string(), via.to_string()));
                return;
            }
        };
        let entry = match &target {
            Target::Symbol(file, names) => self.symbol(file, names),
            Target::Card(c) if self.tp.file_card(&c.id).is_some() => self.file(c),
            Target::Card(c) | Target::Field(c, ..) | Target::EnumValue(c, _) if is_shape(c) => {
                self.card(c, Layer::Shapes)
            }
            Target::Card(c) => self.writing(c).or_else(|| self.card(c, Layer::Refs)),
            Target::FormatRow(doc, names) => self.row(doc, names),
            Target::CustomRow(c, names) if c.kind == "stackdoc" => self.version(c, &names[0]),
            Target::Field(c, ..)
            | Target::EnumValue(c, _)
            | Target::Step(c, _)
            | Target::CustomRow(c, _) => self.card(c, Layer::Refs),
            Target::Ghost(_) => None,
        };
        let Some((into, mut entry)) = entry else {
            gathered
                .unresolved
                .push((path.to_string(), via.to_string()));
            return;
        };
        // One hop: past the refs, only shapes and code are followed.
        let wanted = match layer {
            Layer::Refs => true,
            Layer::Shapes => into != Layer::Refs,
            Layer::Code => into == Layer::Code,
        };
        if !wanted {
            return;
        }
        if gathered.seen(&entry.path, via) {
            return;
        }
        entry.via.push(via.to_string());
        match into {
            Layer::Refs => gathered.refs.push(entry),
            Layer::Shapes => gathered.shapes.push(entry),
            Layer::Code => gathered.code.push(entry),
        }
    }

    /// A path into a working directory not read on this machine: left, with why.
    fn not_read(&self, path: &str, via: &str) -> Option<Value> {
        let row = || {
            json!({
                "path": path,
                "kind": "file",
                "via": [via],
                "why": text!("reads.context.notRead"),
                "read": call(self.project, self.at_tree, path, None),
            })
        };
        // A line's file kept by its id, where no working directory was read to name it.
        if path.starts_with("file:") {
            return Some(row());
        }
        let (Some(section), _) = crate::pathtext::split_section(path) else {
            return None;
        };
        let top = crate::pathtext::slashes(section)
            .first()
            .map(|s| crate::pathtext::unquote(s).into_owned())?;
        let workdir = self
            .tp
            .defs
            .tree
            .main
            .iter()
            .any(|t| t.kind == crate::kinds::WORKDIR && crate::pathtext::same(&t.name, &top));
        let read_here = self.tp.files.as_deref().is_some_and(|f| {
            f.detail.sections.iter().any(|s| {
                s.kind.as_deref() == Some(crate::kinds::WORKDIR)
                    && crate::pathtext::same(&s.name, &top)
            })
        });
        if !workdir || read_here {
            return None;
        }
        Some(row())
    }

    fn entry(&self, path: String, kind: &str, said: Map<String, Value>, read: Value) -> Entry {
        Entry {
            path,
            kind: kind.to_string(),
            via: Vec::new(),
            said,
            read,
            whole: false,
        }
    }

    /// A document, a note, a flow or a table as `as: markdown` reads it.
    fn writing(&self, card: &Card) -> Option<(Layer, Entry)> {
        let written = &self.tree.detail.project;
        let text = if card.kind == crate::kinds::TABLE {
            written.in_export_language(|| {
                super::tables::markdown(&super::tables::build(self.tp, card))
            })
        } else if card.kind == "flow" {
            written.in_export_language(|| crate::export::flowdoc::as_written(card, self.tp))
        } else if card.is_writing() {
            written
                .in_export_language(|| crate::export::documents::markdown_as_written(card, self.tp))
        } else {
            return None;
        };
        let path = self.tp.full(card);
        let mut said = Map::new();
        said.insert("markdown".into(), text.into());
        let read = call(
            self.project,
            self.at_tree,
            &path,
            Some(json!({"as": "markdown"})),
        );
        Some((Layer::Refs, self.entry(path, &card.kind, said, read)))
    }

    /// A formatdoc's item as it is written out, with the lines that leave it.
    fn row(&self, doc: &Card, names: &[String]) -> Option<(Layer, Entry)> {
        let written = &self.tree.detail.project;
        let text = written
            .in_export_language(|| crate::export::documents::row_as_written(doc, names, self.tp))?;
        let from = crate::pathtext::join_names(names.iter().map(String::as_str));
        let path = format!("{}.{from}", self.tp.full(doc));
        let mut said = Map::new();
        said.insert("markdown".into(), text.into());
        let lines: Vec<Value> = doc
            .connections
            .iter()
            .filter(|l| l.from == from || l.from.starts_with(&format!("{from}.")))
            .filter_map(|l| {
                let to = l.to.as_deref()?;
                let mut row = Map::new();
                row.insert("line".into(), l.line.clone().into());
                row.insert(
                    "to".into(),
                    self.path_of_id(to, None)
                        .unwrap_or_else(|| to.to_string())
                        .into(),
                );
                if let Some(at) = l.at.as_deref().filter(|a| !a.is_empty()) {
                    row.insert("at".into(), at.into());
                }
                Some(Value::Object(row))
            })
            .collect();
        if !lines.is_empty() {
            said.insert("lines".into(), lines.into());
        }
        let read = call(
            self.project,
            self.at_tree,
            &path,
            Some(json!({"as": "markdown"})),
        );
        Some((Layer::Refs, self.entry(path, "row", said, read)))
    }

    /// A stackdoc's version as it is written out.
    fn version(&self, card: &Card, name: &str) -> Option<(Layer, Entry)> {
        let written = &self.tree.detail.project;
        let text = written.in_export_language(|| {
            crate::export::documents::version_as_written(card, name, self.tp)
        })?;
        let path = format!("{}.{name}", self.tp.full(card));
        let mut said = Map::new();
        said.insert("markdown".into(), text.into());
        let read = call(
            self.project,
            self.at_tree,
            &path,
            Some(json!({"as": "markdown"})),
        );
        Some((Layer::Refs, self.entry(path, "version", said, read)))
    }

    /// A card as one of a list: what it holds itself.
    fn card(&self, card: &Card, layer: Layer) -> Option<(Layer, Entry)> {
        let Value::Object(mut said) = self.tree.card_view(card, &Projection::own(), None) else {
            return None;
        };
        for key in ["project", "path", "kind"] {
            said.shift_remove(key);
        }
        let path = self.tp.full(card);
        let read = call(self.project, self.at_tree, &path, None);
        let mut entry = self.entry(path, &card.kind, said, read);
        entry.whole = true;
        Some((layer, entry))
    }

    /// A function or a type in a file, read from the file now.
    fn symbol(&self, file: &Card, names: &[String]) -> Option<(Layer, Entry)> {
        let read =
            super::code::symbol_read(self.tp, self.project, file, names, &Map::new(), None).ok()?;
        let path = read["path"].as_str()?.to_string();
        let mut said = Map::new();
        for key in ["language", "lines", "declaration", "body"] {
            if let Some(v) = read.get(key) {
                said.insert(key.into(), v.clone());
            }
        }
        let call = call(self.project, self.at_tree, &path, None);
        Some((Layer::Code, self.entry(path, "symbol", said, call)))
    }

    /// A file: its language and its functions and types.
    fn file(&self, file: &Card) -> Option<(Layer, Entry)> {
        let said = super::code::file_said(self.tp, file, &Map::new());
        let path = self.tp.full(file);
        let read = call(self.project, self.at_tree, &path, None);
        Some((Layer::Code, self.entry(path, "file", said, read)))
    }

    /// What an item points at: the paths written in its text, and its lines (by their kind).
    fn pointed_from(&self, entry: &Entry) -> Vec<Pointed> {
        let mut out: Vec<Pointed> = Vec::new();
        for key in ["markdown", "body"] {
            if let Some(text) = entry.said.get(key).and_then(Value::as_str) {
                for r in crate::notetext::refs(text) {
                    out.push((None, crate::pathtext::canonical(r.path.trim()).into_owned()));
                }
            }
        }
        // A row's own lines; a card's, and a flow's nodes' too.
        if let Some(lines) = entry.said.get("lines").and_then(Value::as_array) {
            for line in lines {
                let (Some(kind), Some(to)) = (line["line"].as_str(), line["to"].as_str()) else {
                    continue;
                };
                let path = match line["at"].as_str() {
                    Some(at) => format!("{to}.{at}"),
                    None => to.to_string(),
                };
                out.push((Some(kind.to_string()), path));
            }
            return out;
        }
        let Ok(Target::Card(card)) = walk::locate(self.tp, &entry.path, None) else {
            return out;
        };
        let mut cards = vec![card];
        if card.kind == "flow" {
            cards.extend(self.tp.inner_of(&card.id));
        }
        for c in cards {
            if c.id != card.id {
                for r in crate::notetext::refs(&c.body) {
                    out.push((None, crate::pathtext::canonical(r.path.trim()).into_owned()));
                }
            }
            for line in &c.connections {
                let Some(to) = line.to.as_deref() else {
                    continue;
                };
                if let Some(path) = self.path_of_id(to, line.at.as_deref()) {
                    out.push((Some(line.line.clone()), path));
                }
            }
        }
        out
    }
}
