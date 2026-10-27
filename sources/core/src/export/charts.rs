//! A flow's diagram: its flowchart — each node in the shape
//! of what it is, each way labelled — or its sequence diagram — who takes part, then
//! what passes between them in order. The first line says what it was drawn from, with
//! a rev that changes exactly when the picture would.

use super::ident;
use super::paths::identify_head;
use super::{Project, relative};
use crate::model::{Card, Exit, FlowNode, Step};
use crate::notetext::{self, exported_path, plain_path};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet, VecDeque};

/// Whether `begin` receives without saying from whom: no peer, no program, and not open to anyone.
/// A gap in the design, drawn and checked as one (`missingPartners`), never filled in from who calls.
pub fn names_no_partner(begin: &FlowNode) -> bool {
    begin.kind == "begin"
        && begin.trigger.as_deref() == Some("receive")
        && begin.peers.is_empty()
        && begin.program.is_none()
        && !begin.any_caller
}

/// Whom a begin that names no partner receives from, said in words: anyone, or not written.
pub fn begin_partner(begin: &FlowNode) -> String {
    if begin.any_caller {
        text!("charts.partner.anyone")
    } else {
        text!("charts.partner.notWritten")
    }
}

/// Whom a row or an end that names no one deals with: the one its flow's begin receives from.
pub fn start_partner() -> String {
    text!("charts.partner.start")
}

/// Twelve hex characters of SHA-256 of the lines.
fn revision(lines: &[String]) -> String {
    let digest = Sha256::digest(lines.join("\n").as_bytes());
    digest
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>()[..12]
        .to_string()
}

/// The shape it is drawn in: `asked` for a flow, else the one its card keeps.
pub fn shape(card: &Card, asked: Option<&str>) -> String {
    if card.kind == "action" {
        "sequence".into()
    } else {
        asked.unwrap_or(card.diagram_shape()).to_string()
    }
}

/// As it is written out: the head only the mark and the rev (of the picture as the app
/// draws it), the paths as the cards' names.
pub fn render(card: &Card, project: &Project, asked: Option<&str>) -> String {
    let shape = shape(card, asked);
    let written = if shape == "sequence" {
        sequence_lines(card, project, true).unwrap_or_else(|why| too_large(card, project, why))
    } else {
        flow_body(card, true)
    };
    let mut lines = vec![format!(
        "%% specboard rev={}",
        revision_in(card, project, &shape)
    )];
    lines.extend(written);
    lines.join("\n") + "\n"
}

/// The picture as the app draws it, in `shape` (`flowchart` or `sequence`), or why a sequence
/// diagram is too large to draw.
fn drawn_checked(card: &Card, project: &Project, shape: &str) -> Result<Vec<String>, TooLarge> {
    if shape == "sequence" {
        sequence_lines(card, project, false)
    } else {
        Ok(flow_body(card, false))
    }
}

/// The picture as the app draws it, in `shape`: one too large to draw says so in its place.
fn drawn(card: &Card, project: &Project, shape: &str) -> Vec<String> {
    drawn_checked(card, project, shape).unwrap_or_else(|why| too_large(card, project, why))
}

/// The rev of the picture in `shape`: it changes exactly when the picture would
/// (`FlowDiagram.revision`, `ActionDiagram.revision`).
pub fn revision_in(card: &Card, project: &Project, shape: &str) -> String {
    revision(&drawn(card, project, shape))
}

/// `revision_in`, or why the picture is too large to draw.
pub fn revision_checked(card: &Card, project: &Project, shape: &str) -> Result<String, TooLarge> {
    drawn_checked(card, project, shape).map(|lines| revision(&lines))
}

/// As the app draws it in `shape`, the head saying what it was drawn from: the project, the
/// flow, the shape and the rev (`FlowDiagram.render`, `ActionDiagram.render`).
pub fn render_drawn(card: &Card, project: &Project, shape: &str) -> String {
    headed_drawn(card, project, shape, drawn(card, project, shape))
}

/// `render_drawn`, or why the picture is too large to draw.
pub fn render_drawn_checked(
    card: &Card,
    project: &Project,
    shape: &str,
) -> Result<String, TooLarge> {
    Ok(headed_drawn(
        card,
        project,
        shape,
        drawn_checked(card, project, shape)?,
    ))
}

/// `render_drawn_checked` with the actors drawn as lifelines where they hold a partner (`lifelines`).
pub fn render_drawn_seen(
    card: &Card,
    project: &Project,
    shape: &str,
    chosen: &[String],
) -> Result<String, TooLarge> {
    if chosen.is_empty() || shape != "sequence" {
        return render_drawn_checked(card, project, shape);
    }
    Ok(headed_drawn(
        card,
        project,
        shape,
        sequence_lines_seen(card, project, false, chosen)?,
    ))
}

/// `revision_checked` with the actors drawn as lifelines where they hold a partner (`lifelines`).
pub fn revision_seen(
    card: &Card,
    project: &Project,
    shape: &str,
    chosen: &[String],
) -> Result<String, TooLarge> {
    if chosen.is_empty() || shape != "sequence" {
        return revision_checked(card, project, shape);
    }
    sequence_lines_seen(card, project, false, chosen).map(|lines| revision(&lines))
}

fn headed_drawn(card: &Card, project: &Project, shape: &str, body: Vec<String>) -> String {
    let what = if shape == "sequence" && card.kind != "flow" {
        "action"
    } else {
        "flow"
    };
    let mut lines = vec![format!(
        "%% specboard project={} {what}={} shape={shape} rev={}",
        project.detail.project.slug,
        ident::line_comment(&project.exported(card)),
        revision(&body)
    )];
    lines.extend(body);
    lines.join("\n") + "\n"
}

// MARK: - The flowchart

fn node_label(kind: &str) -> String {
    match kind {
        "begin" => text!("charts.node.begin"),
        "process" => text!("charts.node.process"),
        "run" => text!("charts.node.run"),
        "eval" => text!("charts.node.eval"),
        "wait" => text!("charts.node.wait"),
        "join" => text!("charts.node.join"),
        "end" => text!("charts.node.end"),
        other => other.to_string(),
    }
}

/// How many of a process node's rows its box says before counting the rest.
const ROWS_SHOWN: usize = 5;

/// Whom a start, a row or an answer deals with, said in one: the flows it names, then the
/// program; none when it names no one.
fn partners(
    peers: &[String],
    program: Option<&str>,
    path: &dyn Fn(&str) -> String,
) -> Option<String> {
    let names: Vec<String> = peers
        .iter()
        .map(|p| path(p))
        .chain(program.map(str::to_string))
        .collect();
    (!names.is_empty()).then(|| crate::text::list(&names))
}

fn title(node: &FlowNode, path: &dyn Fn(&str) -> String) -> String {
    let head = node_label(&node.kind);
    let event = |n: &FlowNode| {
        text!(
            "charts.title.event",
            flow = n.event.as_ref().map(|e| path(&e.flow)).unwrap_or_default(),
            outcome = n
                .event
                .as_ref()
                .map(|e| e.outcome.clone())
                .unwrap_or_default()
        )
    };
    match node.kind.as_str() {
        "begin" => match node.trigger.as_deref() {
            Some("event") => format!("{head}: {}", event(node)),
            Some("schedule") => format!(
                "{head}: {}",
                node.every
                    .clone()
                    .unwrap_or_else(|| text!("charts.title.periodic"))
            ),
            Some("receive") => {
                let from = partners(&node.peers, node.program.as_deref(), path)
                    .unwrap_or_else(|| begin_partner(node));
                if node.carries.is_empty() {
                    text!("charts.title.receive", head, from)
                } else {
                    let carries = node
                        .carries
                        .iter()
                        .map(|c| path(c))
                        .collect::<Vec<_>>()
                        .join(", ");
                    text!("charts.title.receiveCarrying", head, from, carries)
                }
            }
            Some("startup") => text!("charts.title.startup", head),
            _ => text!("charts.title.manual", head),
        },
        "process" => {
            let mut lines = vec![head];
            for step in node.steps.iter().take(ROWS_SHOWN) {
                let other = partners(&step.peers, step.program.as_deref(), path);
                lines.push(match step.op.as_str() {
                    "read" | "write" => {
                        format!(
                            "{} {}",
                            step.op,
                            step.paths
                                .iter()
                                .map(|p| path(p))
                                .collect::<Vec<_>>()
                                .join(", ")
                        )
                    }
                    "sign" | "verify" => format!(
                        "{} {}",
                        step.op,
                        step.key.as_deref().map(path).unwrap_or_default()
                    ),
                    // A row that passes nothing to anyone says no one it goes to.
                    "compute" => step.op.clone(),
                    _ => format!(
                        "{} {}{}",
                        step.op,
                        other.unwrap_or_else(start_partner),
                        if step.carries.is_empty() {
                            String::new()
                        } else {
                            format!(
                                ": {}",
                                step.carries
                                    .iter()
                                    .map(|c| path(c))
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            )
                        }
                    ),
                });
            }
            if node.steps.len() > ROWS_SHOWN {
                lines.push(text!(
                    "charts.title.moreRows",
                    count = node.steps.len() - ROWS_SHOWN
                ));
            }
            lines.join("\n")
        }
        "run" => match &node.target {
            Some(target) => format!("{head}: {}", path(target)),
            None => text!(
                "charts.title.runParallel",
                head,
                join = node.join.clone().unwrap_or_else(|| "all".into()),
                targets = node
                    .targets
                    .iter()
                    .map(|t| path(t))
                    .collect::<Vec<_>>()
                    .join(" ∥ ")
            ),
        },
        "eval" => format!(
            "{head}{}",
            node.value
                .as_deref()
                .map(|v| format!(": {}", path(v)))
                .unwrap_or_default()
        ),
        "wait" => {
            let what = if node.wait_for.as_deref() == Some("event") {
                event(node)
            } else {
                node.wait_for.clone().unwrap_or_default()
            };
            match node.timeout.as_deref() {
                Some(timeout) => text!("charts.title.waitUntil", head, what, timeout),
                None => format!("{head}: {what}"),
            }
        }
        "join" => text!(
            "charts.title.join",
            head,
            join = node.join.clone().unwrap_or_else(|| "all".into())
        ),
        _ => {
            let outcome = node.outcome.clone().unwrap_or_default();
            match node.reply.as_ref() {
                Some(r) => {
                    let to = partners(&r.peers, r.program.as_deref(), path)
                        .unwrap_or_else(start_partner);
                    let carries = if r.carries.is_empty() {
                        String::new()
                    } else {
                        format!(
                            ": {}",
                            r.carries
                                .iter()
                                .map(|c| path(c))
                                .collect::<Vec<_>>()
                                .join(", ")
                        )
                    };
                    text!("charts.title.endReply", head, outcome, to, carries)
                }
                None => format!("{head}: {outcome}"),
            }
        }
    }
}

fn node_id(name: &str) -> String {
    format!("n_{}", ident::safe(name))
}

/// Each node's id in a flow's chart, by its name: `node_id`, told apart with a number where two
/// names come to one (`a b` and `a_b`, two names of letters of other scripts).
fn node_ids(flow: &Card) -> HashMap<&str, String> {
    let mut taken: HashSet<String> = HashSet::new();
    let mut out = HashMap::new();
    for node in &flow.nodes {
        let base = node_id(&node.name);
        let mut id = base.clone();
        let mut n = 2;
        while !taken.insert(id.clone()) {
            id = format!("{base}_{n}");
            n += 1;
        }
        out.insert(node.name.as_str(), id);
    }
    out
}

fn flow_body(flow: &Card, exported: bool) -> Vec<String> {
    let path: &dyn Fn(&str) -> String = if exported {
        &|p: &str| plain_path(p)
    } else {
        &|p: &str| exported_path(p)
    };
    let mut lines = vec!["flowchart LR".to_string()];
    let ids = node_ids(flow);
    let id = |name: &str| ids.get(name).cloned().unwrap_or_else(|| node_id(name));
    if flow.nodes.is_empty() {
        lines.push(format!("    empty([\"{}\"])", text!("charts.flow.noNodes")));
        return lines;
    }
    for node in &flow.nodes {
        let mut parts: Vec<String> = node
            .label
            .iter()
            .filter(|l| !l.is_empty())
            .cloned()
            .collect();
        parts.extend(title(node, path).split('\n').map(str::to_string));
        let text = parts
            .iter()
            .map(|p| ident::mermaid_text(p))
            .collect::<Vec<_>>()
            .join("<br/>");
        let shaped = match node.kind.as_str() {
            "begin" | "end" => format!("([\"{text}\"])"),
            "eval" => format!("{{\"{text}\"}}"),
            "wait" => format!("{{{{\"{text}\"}}}}"),
            "join" => format!("[[\"{text}\"]]"),
            _ => format!("[\"{text}\"]"),
        };
        lines.push(format!("    {}{shaped}", id(&node.name)));
    }
    for node in &flow.nodes {
        for (to, label, kind) in node.exits() {
            let arrow = if kind == Exit::Error || kind == Exit::Timeout {
                "-.->"
            } else {
                "-->"
            };
            let label = label
                .map(|l| {
                    format!(
                        "|\"{}\"|",
                        ident::mermaid_text(&notetext::transform(&l, |r| {
                            Some(r.shown.clone().unwrap_or_else(|| path(&r.path)))
                        }))
                    )
                })
                .unwrap_or_default();
            lines.push(format!("    {} {arrow}{label} {}", id(&node.name), id(&to)));
        }
    }
    lines
}

// MARK: - The sequence diagram

/// What a note is: a plain one, a key's (sign, verify), a check that goes on, how a way ends, or a
/// mark — a way that only fails (`✕`) or the end of a flow nobody waits for (`■`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum NoteKind {
    Plain,
    Key,
    Check,
    End,
    Mark,
}

impl NoteKind {
    /// As the window's request names it (`sequence_chart`).
    fn said(self) -> &'static str {
        match self {
            NoteKind::Plain => "plain",
            NoteKind::Key => "key",
            NoteKind::Check => "check",
            NoteKind::End => "end",
            NoteKind::Mark => "mark",
        }
    }
}

/// Where an item of a chart comes from: its card, the node in a flow, and the row of a process
/// node by its place — what picking it on the screen picks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Origin {
    pub card: String,
    pub node: Option<String>,
    pub row: Option<usize>,
}

impl Origin {
    fn json(&self) -> Value {
        let mut out = Map::new();
        out.insert("card".into(), self.card.clone().into());
        if let Some(node) = &self.node {
            out.insert("node".into(), node.clone().into());
        }
        if let Some(row) = self.row {
            out.insert("row".into(), row.into());
        }
        Value::Object(out)
    }
}

/// A flow a bar stands for — called on its own lifeline, or reached on another in an actor's
/// chart: the window draws it from this, not from the bar's words.
#[derive(Clone, Debug)]
struct Called {
    /// How it is reached (`run`, `send`, `call`, `receive`…); empty for an answer.
    op: String,
    flow: String,
    /// The flow's section, in full.
    place: String,
    /// A call into a flow of the same lifeline: the key the window opens it by (`open`).
    call: Option<String>,
    /// A call not opened: its bar holds nothing, to be asked for by its key.
    folded: bool,
    /// A call into a flow drawn above already: never opened, as it would go on for ever.
    cycle: bool,
}

/// A flow drawn in an actor's chart, in a frame of its own: the flow, its label, and the frame's head
/// as mermaid writes it (its name and label).
#[derive(Clone, Debug)]
struct Frame {
    flow: String,
    label: String,
    head: String,
}

enum Item {
    /// An arrow: what it does (`op`), what it carries, the flow it reaches on another actor's
    /// lifeline (`flow`), and what the row it comes from says of itself (`description`: shown on the
    /// screen, not on the arrow).
    Message {
        from: usize,
        to: usize,
        op: Option<String>,
        text: String,
        flow: Option<String>,
        reply: bool,
        origin: Option<Origin>,
        description: Option<String>,
    },
    Note {
        over: usize,
        text: String,
        kind: NoteKind,
        origin: Option<Origin>,
        description: Option<String>,
    },
    /// A bar on a lifeline: a process or run node (`node`, its heading the `label`) and what its rows
    /// do inside it, or a flow of the same lifeline called (no node, the flow named in the `label`, what
    /// it does inside the bar where the call opens: `Builder::unfolded`).
    Step {
        over: usize,
        node: Option<String>,
        label: String,
        items: Vec<Item>,
        origin: Option<Origin>,
        /// The flow it stands for, where it has no node: how it is reached, its card and section.
        called: Option<Called>,
    },
    Block {
        kind: &'static str,
        sections: Vec<(String, Vec<Item>)>,
        origin: Option<Origin>,
        /// A flow's frame in an actor's chart (`kind` `flow`).
        frame: Option<Frame>,
    },
}

/// One who takes part, as drawn: the name it is drawn under (with its section where two share one),
/// its name alone, where it is, what it is (`own`, `flow`, `actor`, `store`, `records`, `program`,
/// `caller`), the card and the actor section it is, how an actor is drawn (`figure`), the actor it
/// is bracketed in with its siblings (`group`, and that one's name; itself, for an actor drawn beside
/// the actors inside it), and whether opening it would draw an actor inside it that is not drawn now
/// (`open`: one the chart deals with lies inside it; never once open).
#[derive(Clone, Debug)]
pub struct Drawn {
    pub label: String,
    pub name: String,
    pub place: Option<String>,
    pub role: &'static str,
    pub card: Option<String>,
    pub section: Option<String>,
    pub figure: Option<String>,
    pub group: Option<String>,
    group_name: Option<String>,
    pub open: bool,
    /// A gap's lane: where its first gap is, what pressing its head picks.
    pub origin: Option<Origin>,
}

impl Drawn {
    /// Drawn as mermaid's `actor`: an actor whose figure is a person.
    fn person(&self) -> bool {
        self.role == "actor" && self.figure.as_deref() == Some("person")
    }
}

pub struct Sequence {
    participants: Vec<Drawn>,
    items: Vec<Item>,
    /// The lifeline of the flow opened (none for an actor's chart).
    me: Option<usize>,
}

/// The tint of a flow's frame in an actor's chart: the actor's blue, faint.
const FRAME_FILL: &str = "rgba(10, 132, 255, 0.06)";

fn safe(text: &str) -> String {
    ident::mermaid_text(text).replace(';', ",")
}

fn words(list: &[Option<String>]) -> String {
    list.iter()
        .flatten()
        .filter(|w| !w.is_empty())
        .cloned()
        .collect::<Vec<_>>()
        .join(" ")
}

impl Sequence {
    /// The diagram as mermaid's lines, from `sequenceDiagram` on.
    pub fn mermaid(&self) -> Vec<String> {
        let mut lines: Vec<String> = Vec::new();
        let mut open_box: Option<&str> = None;
        for (i, p) in self.participants.iter().enumerate() {
            let group = p.group.as_deref();
            if open_box.is_some() && open_box != group {
                lines.push("    end".into());
                open_box = None;
            }
            if open_box.is_none()
                && let (Some(id), Some(name)) = (group, &p.group_name)
            {
                lines.push(format!("    box {}", safe(name)));
                open_box = Some(id);
            }
            lines.push(format!(
                "    {} P{} as {}",
                if p.person() { "actor" } else { "participant" },
                i + 1,
                safe(&p.label)
            ));
        }
        if open_box.is_some() {
            lines.push("    end".into());
        }
        fn emit(items: &[Item], indent: &str, lines: &mut Vec<String>) {
            for item in items {
                match item {
                    Item::Message {
                        from,
                        to,
                        op,
                        text,
                        flow,
                        reply,
                        ..
                    } => lines.push(format!(
                        "{indent}P{}{}P{}: {}",
                        from + 1,
                        if *reply { "-->>" } else { "->>" },
                        to + 1,
                        safe(&words(&[op.clone(), Some(text.clone()), flow.clone()]))
                    )),
                    Item::Note {
                        over, text, kind, ..
                    } => {
                        let said = safe(&if *kind == NoteKind::Check {
                            text!("charts.note.check", text)
                        } else {
                            text.clone()
                        });
                        lines.push(if *kind == NoteKind::Mark && said.starts_with('✕') {
                            format!("{indent}Note right of P{}: {said}", over + 1)
                        } else {
                            format!("{indent}Note over P{}: {said}", over + 1)
                        });
                    }
                    Item::Step {
                        over,
                        node,
                        label,
                        items,
                        ..
                    } => {
                        lines.push(format!(
                            "{indent}Note over P{}: {}",
                            over + 1,
                            safe(&words(&[node.clone(), Some(label.clone())]))
                        ));
                        lines.push(format!("{indent}activate P{}", over + 1));
                        emit(items, &format!("{indent}    "), lines);
                        lines.push(format!("{indent}deactivate P{}", over + 1));
                    }
                    Item::Block {
                        frame: Some(frame),
                        sections,
                        ..
                    } => {
                        lines.push(format!("{indent}rect {FRAME_FILL}"));
                        let inside = format!("{indent}    ");
                        let over = sections
                            .iter()
                            .flat_map(|(_, inner)| inner.iter())
                            .find_map(|i| match i {
                                Item::Message { to, .. } => Some(*to),
                                Item::Note { over, .. } | Item::Step { over, .. } => Some(*over),
                                Item::Block { .. } => None,
                            })
                            .unwrap_or(0);
                        lines.push(format!(
                            "{inside}Note over P{}: {}",
                            over + 1,
                            safe(&frame.head)
                        ));
                        for (_, inner) in sections {
                            emit(inner, &inside, lines);
                        }
                        lines.push(format!("{indent}end"));
                    }
                    Item::Block { kind, sections, .. } => {
                        for (index, (label, inner)) in sections.iter().enumerate() {
                            if *kind == "par" {
                                lines.push(format!(
                                    "{indent}{}{}",
                                    if index == 0 { "par" } else { "and" },
                                    if label.is_empty() {
                                        String::new()
                                    } else {
                                        format!(" {}", safe(label))
                                    }
                                ));
                            } else {
                                lines.push(format!(
                                    "{indent}{} {}",
                                    if index == 0 { *kind } else { "else" },
                                    safe(label)
                                ));
                            }
                            emit(inner, &format!("{indent}    "), lines);
                        }
                        if !sections.is_empty() {
                            lines.push(format!("{indent}end"));
                        }
                    }
                }
            }
        }
        emit(&self.items, "    ", &mut lines);
        let mut out = vec!["sequenceDiagram".to_string()];
        out.extend(lines);
        out
    }
}

/// What starts an action, or a flow's begin as one.
struct Trigger {
    on: String,
    peers: Vec<String>,
    program: Option<String>,
    carries: Vec<String>,
    via: Option<String>,
    initiator: Option<String>,
    verify_with: Option<String>,
    every: Option<String>,
    /// What the begin says of itself: on the screen, not on the arrow.
    description: Option<String>,
    /// What is said with it in the note of a begin that receives nothing (an event's flow and outcome).
    said: Option<String>,
}

/// A way out of a node that parts: what it is called, where it goes, and whether it is
/// left out when it has nothing of its own (the way on: a decision's else, a wait's next).
struct Way {
    label: String,
    to: String,
    quiet: bool,
}

/// Whether what passes with several partners comes from them or goes to them.
#[derive(Clone, Copy)]
enum Direction {
    From,
    To,
}

/// One who takes part: what tells it from the others (`key`), the name it is drawn under, the name
/// with its section for when two who take part have the one name, and what it is (`Drawn`).
struct Participant {
    key: String,
    label: String,
    with_section: Option<String>,
    name: String,
    place: Option<String>,
    role: &'static str,
    card: Option<String>,
    section: Option<String>,
    figure: Option<String>,
    origin: Option<Origin>,
}

impl Participant {
    /// One known by its label alone (a program, a lane of what is not written).
    fn named(label: String, role: &'static str) -> Participant {
        Participant {
            key: format!("{role}:{label}"),
            name: label.clone(),
            label,
            with_section: None,
            place: None,
            role,
            card: None,
            section: None,
            figure: None,
            origin: None,
        }
    }
}

/// One a row deals with: how a way to it is named, the participant it is drawn as, and the flow it
/// is, when it is one.
struct Other<'a> {
    said: String,
    at: usize,
    flow: Option<&'a Card>,
}

struct Builder<'p, 'a> {
    card: &'a Card,
    project: &'p Project<'a>,
    actors: super::actors::Actors<'p, 'a>,
    exported: bool,
    participants: Vec<Participant>,
    ids: HashMap<String, usize>,
    /// The actor the chart is seen from: the flow's own, or the one whose chart it is.
    seen: Option<String>,
    /// The actors drawn as lifelines where they hold a partner (`lifelines`).
    chosen: Vec<String>,
    /// The actors the chart deals with, each as deep as it is (a flow's, a store's), whatever lifeline
    /// draws it: what opening a lifeline would draw inside it.
    dealt: HashSet<String>,
    /// The flow's own lifeline.
    me: usize,
    /// The flow's actor, what its stores are looked for near.
    own_actor: Option<String>,
    /// Whether the flow is started by someone who waits for it (its begin receives): each of its
    /// ends answers them.
    has_caller: bool,
    /// Who it answers when an end names no one: those its begin receives from.
    answered: Vec<String>,
    answered_program: Option<String>,
    /// Whether its begin receives from anyone (`anyCaller`).
    answered_anyone: bool,
    /// The lifeline of the flow whose bar this flow is drawn in (`reached`, `same_lifeline`): its begin is the
    /// arrow from there, so what names no one goes back there.
    reacher: Option<usize>,
    by_name: HashMap<&'a str, &'a FlowNode>,
    /// The end the last walk stopped at on its own way, if it did (`parting`).
    ended: Option<String>,
    /// Where each node's ways go, in the order `FlowNode::exits` says them.
    outs: HashMap<String, Vec<String>>,
    /// Where each node is come to from.
    ins: HashMap<String, Vec<String>>,
    /// The ways that go back to a node on the way to them from the begin: (from, to).
    back: HashSet<(String, String)>,
    /// Each node a way goes back to, and the nodes those ways go back from, in the order found.
    heads: HashMap<String, Vec<String>>,
    /// The loops drawn, or being drawn, by the node they go back to.
    looping: HashSet<String>,
    /// The ways back a loop's block says, so that the way itself is not said again.
    silent: HashSet<(String, String)>,
    /// The nodes drawn: each is drawn once, a way that comes to one again saying where it goes.
    /// Ends are not kept, as each way that stops says how.
    drawn: HashSet<String>,
    /// Nodes and rows looked at so far, against `MOST_STEPS`.
    steps: std::cell::Cell<usize>,
    /// Where what is drawn now comes from.
    at: Option<Origin>,
    /// The names of two cards or more the flow's rows read or write: said with their sections.
    clashing: HashSet<String>,
    /// An actor's chart: the actor, and the flows it provides (each its own frame). A flow inside the
    /// actor that is no frame is drawn where it is reached, as a bar on its lifeline.
    expand: Option<(String, HashSet<String>)>,
    /// The flows being drawn, the one the chart opens first and each flow drawn inside a bar of it — one
    /// reached on another lifeline, one called on its own —, so that none is drawn inside itself.
    expanding: Vec<String>,
    /// The calls on one lifeline that the window opened (by `Called::call`'s key).
    opened: HashSet<String>,
    /// Whether the flow drawn now is drawn inside the bar of a call on its caller's own lifeline: what
    /// it answers there is the end of the bar, not an arrow back.
    unfolded: bool,
    /// What the lane of the places not written says, a shape at a time: its name, and how it is
    /// said there (with the stores it reaches, when it reaches more than one).
    kept_nowhere: Vec<(String, String)>,
}

/// What a builder works out of the flow it draws, set aside while a flow it reaches is drawn inside it.
struct FlowState<'a> {
    card: &'a Card,
    own_actor: Option<String>,
    has_caller: bool,
    answered: Vec<String>,
    answered_program: Option<String>,
    answered_anyone: bool,
    reacher: Option<usize>,
    unfolded: bool,
    by_name: HashMap<&'a str, &'a FlowNode>,
    ended: Option<String>,
    outs: HashMap<String, Vec<String>>,
    ins: HashMap<String, Vec<String>>,
    back: HashSet<(String, String)>,
    heads: HashMap<String, Vec<String>>,
    looping: HashSet<String>,
    silent: HashSet<(String, String)>,
    drawn: HashSet<String>,
    at: Option<Origin>,
    clashing: HashSet<String>,
    me: usize,
}

fn short(paths: &[String]) -> String {
    paths
        .iter()
        .map(|p| crate::pathtext::split_section(p).1.to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

impl<'p, 'a> Builder<'p, 'a> {
    fn new(
        card: &'a Card,
        project: &'p Project<'a>,
        exported: bool,
        seen: Option<String>,
        chosen: Vec<String>,
    ) -> Builder<'p, 'a> {
        Builder {
            card,
            project,
            actors: super::actors::Actors::new(project),
            exported,
            participants: Vec::new(),
            ids: HashMap::new(),
            seen,
            chosen,
            dealt: HashSet::new(),
            me: 0,
            own_actor: None,
            has_caller: false,
            answered: Vec::new(),
            answered_program: None,
            answered_anyone: false,
            reacher: None,
            by_name: HashMap::new(),
            ended: None,
            outs: HashMap::new(),
            ins: HashMap::new(),
            back: HashSet::new(),
            heads: HashMap::new(),
            looping: HashSet::new(),
            silent: HashSet::new(),
            drawn: HashSet::new(),
            steps: std::cell::Cell::new(0),
            at: None,
            clashing: HashSet::new(),
            expand: None,
            expanding: Vec::new(),
            opened: HashSet::new(),
            unfolded: false,
            kept_nowhere: Vec::new(),
        }
    }

    /// What is worked out of the flow drawn now, taken out to be put back (`put_back`).
    fn set_aside(&mut self) -> FlowState<'a> {
        FlowState {
            card: self.card,
            own_actor: self.own_actor.take(),
            has_caller: self.has_caller,
            answered: std::mem::take(&mut self.answered),
            answered_program: self.answered_program.take(),
            answered_anyone: self.answered_anyone,
            reacher: self.reacher.take(),
            unfolded: self.unfolded,
            by_name: std::mem::take(&mut self.by_name),
            ended: self.ended.take(),
            outs: std::mem::take(&mut self.outs),
            ins: std::mem::take(&mut self.ins),
            back: std::mem::take(&mut self.back),
            heads: std::mem::take(&mut self.heads),
            looping: std::mem::take(&mut self.looping),
            silent: std::mem::take(&mut self.silent),
            drawn: std::mem::take(&mut self.drawn),
            at: self.at.take(),
            clashing: std::mem::take(&mut self.clashing),
            me: self.me,
        }
    }

    fn put_back(&mut self, state: FlowState<'a>) {
        self.card = state.card;
        self.own_actor = state.own_actor;
        self.has_caller = state.has_caller;
        self.answered = state.answered;
        self.answered_program = state.answered_program;
        self.answered_anyone = state.answered_anyone;
        self.reacher = state.reacher;
        self.unfolded = state.unfolded;
        self.by_name = state.by_name;
        self.ended = state.ended;
        self.outs = state.outs;
        self.ins = state.ins;
        self.back = state.back;
        self.heads = state.heads;
        self.looping = state.looping;
        self.silent = state.silent;
        self.drawn = state.drawn;
        self.at = state.at;
        self.clashing = state.clashing;
        self.me = state.me;
    }

    /// In an actor's chart, a flow inside the actor that it provides no frame for — run, called or
    /// sent to from another of its lifelines — drawn where it is reached: a bar on its lifeline with what
    /// it does inside (its start being the arrow that reaches it). None for anything else: a frame of
    /// its own, another actor's flow (never opened), a flow on this very lifeline, one being drawn.
    fn reached(&mut self, to: &Other<'a>, op: &str) -> Option<Item> {
        let flow = to.flow?;
        let (actor, provided) = self.expand.as_ref()?;
        if to.at == self.me
            || provided.contains(&flow.id)
            || !self.actors.contains(actor, &self.project.section_of(flow))
            || flow.id == self.card.id
            || self.expanding.contains(&flow.id)
        {
            return None;
        }
        let (items, over) = self.drawn_inside(flow, false);
        Some(Item::Step {
            over,
            node: None,
            label: words(&[Some(flow.name.clone()), Some(flow.label.clone())]),
            items,
            origin: Some(Origin {
                card: flow.id.clone(),
                node: None,
                row: None,
            }),
            called: Some(self.called(op, flow)),
        })
    }

    /// What `flow` does, drawn inside the bar of the call that reaches it from the lifeline drawn now: its
    /// start is that call itself, so what its begin and its ends name no one for is that lifeline (`reacher`).
    /// `unfolded`: the call is on the caller's own lifeline. Its items, and the lifeline it is drawn on.
    fn drawn_inside(&mut self, flow: &'a Card, unfolded: bool) -> (Vec<Item>, usize) {
        let reacher = self.me;
        let state = self.set_aside();
        self.expanding.push(flow.id.clone());
        self.open(flow);
        self.reacher = Some(reacher);
        self.unfolded = unfolded;
        let items = flow_walk(self, false);
        let over = self.me;
        self.expanding.pop();
        self.put_back(state);
        (items, over)
    }

    /// The flow `flow` a bar stands for, reached by `op`.
    fn called(&self, op: &str, flow: &Card) -> Called {
        Called {
            op: op.to_string(),
            flow: flow.id.clone(),
            place: self
                .project
                .directory
                .full_section(&self.project.section_of(flow)),
            call: None,
            folded: false,
            cycle: false,
        }
    }

    /// The key of the call drawn now into `flow`: the row it is made by (its flow, node and row) and the
    /// flow called, whichever place of the chart it is drawn at.
    fn call_key(&self, flow: &Card) -> String {
        let at = self.at.as_ref();
        format!(
            "{}/{}/{}/{}",
            self.card.id,
            at.and_then(|o| o.node.as_deref()).unwrap_or_default(),
            at.and_then(|o| o.row)
                .map(|r| r.to_string())
                .unwrap_or_default(),
            flow.id
        )
    }

    /// A call from this flow into `flow`, a flow of this very lifeline: a bar on it named with that flow.
    /// What a forward call (a run, a send, a call) does is drawn inside the bar only where the window asked
    /// for it by the call's key, at any depth; the calls inside one not asked for are not drawn at all. A
    /// flow being drawn above is never opened again. An answer or a receive is a bar of its own and holds
    /// nothing.
    fn same_lifeline(&mut self, flow: &'a Card, op: &str, forward: bool) -> Item {
        let mut called = self.called(op, flow);
        let mut items = Vec::new();
        if forward {
            if self.expanding.contains(&flow.id) || flow.id == self.card.id {
                called.cycle = true;
            } else {
                let key = self.call_key(flow);
                if self.opened.contains(&key) {
                    items = self.drawn_inside(flow, true).0;
                } else {
                    called.folded = true;
                }
                called.call = Some(key);
            }
        }
        Item::Step {
            over: self.me,
            node: None,
            label: words(&[
                (op != "run").then(|| op.to_string()),
                Some(flow.name.clone()),
                Some(flow.label.clone()),
            ]),
            items,
            origin: self.at.clone(),
            called: Some(called),
        }
    }

    /// Ready to draw `card`: what was worked out of the one before let go, the participants kept.
    fn open(&mut self, card: &'a Card) {
        self.card = card;
        self.own_actor = self.actors.of_card(card);
        self.has_caller = false;
        self.answered = Vec::new();
        self.answered_program = None;
        self.answered_anyone = false;
        self.reacher = None;
        self.unfolded = false;
        self.by_name = HashMap::new();
        self.ended = None;
        self.outs = HashMap::new();
        self.ins = HashMap::new();
        self.back = HashSet::new();
        self.heads = HashMap::new();
        self.looping = HashSet::new();
        self.silent = HashSet::new();
        self.drawn = HashSet::new();
        self.at = None;
        self.clashing = self.clashing_in(card);
        self.me = self.flow_participant(card);
        if let Some(begin) = card.nodes.iter().find(|n| n.kind == "begin")
            && begin.trigger.as_deref() == Some("receive")
        {
            self.has_caller = true;
            self.answered = begin.peers.clone();
            self.answered_program = begin.program.clone();
            self.answered_anyone = begin.any_caller;
        }
    }

    fn participant(&mut self, label: String) -> usize {
        self.add(Participant::named(label, "program"))
    }

    /// The one it is, known by its key once added.
    fn add(&mut self, participant: Participant) -> usize {
        if let Some(&known) = self.ids.get(&participant.key) {
            return known;
        }
        self.ids
            .insert(participant.key.clone(), self.participants.len());
        self.participants.push(participant);
        self.participants.len() - 1
    }

    /// An actor's lifeline.
    fn actor_participant(&mut self, actor: &str) -> usize {
        let project = self.project;
        let section = project.detail.sections.iter().find(|s| s.id == actor);
        let name = section.map(|s| s.name.clone()).unwrap_or_default();
        let full = project.directory.full_section(actor);
        self.add(Participant {
            key: format!("actor:{actor}"),
            label: name.clone(),
            with_section: Some(full.clone()),
            name,
            place: Some(full),
            role: "actor",
            card: None,
            section: Some(actor.to_string()),
            figure: Some(
                section
                    .and_then(|s| s.figure.clone())
                    .unwrap_or_else(|| "system".into()),
            ),
            origin: None,
        })
    }

    /// The lifeline `actor` is drawn on at the depth the chart is seen from, `actor` kept as dealt with.
    fn actor_lifeline(&mut self, actor: String) -> usize {
        let line = self
            .actors
            .lifeline(&actor, self.seen.as_deref(), &self.chosen);
        self.dealt.insert(actor);
        self.actor_participant(&line)
    }

    /// The lifeline a flow is drawn on: its actor's at the depth the chart is seen from, else the
    /// flow itself.
    fn flow_participant(&mut self, target: &Card) -> usize {
        if let Some(actor) = self.actors.of_card(target) {
            return self.actor_lifeline(actor);
        }
        let place = self
            .project
            .exported_section(&self.project.section_of(target));
        let own = target.id == self.card.id;
        self.add(Participant {
            key: format!("flow:{}", target.id),
            label: text!("charts.participant.card", place, name = target.name),
            with_section: None,
            name: target.name.clone(),
            place: Some(place),
            role: if own { "own" } else { "flow" },
            card: Some(target.id.clone()),
            section: None,
            figure: None,
            origin: None,
        })
    }

    /// The participants as drawn: a name two of them share carries the section; the actors in the
    /// tree's order, the flow opened first when it is no actor, the rest as they came. `order` maps
    /// each participant as built to its place.
    fn finish(&self) -> (Vec<Drawn>, Vec<usize>) {
        let mut shared: HashMap<&str, usize> = HashMap::new();
        for p in &self.participants {
            *shared.entry(p.label.as_str()).or_default() += 1;
        }
        let mut places: Vec<usize> = (0..self.participants.len()).collect();
        let me_first = !self.charting()
            && self
                .participants
                .get(self.me)
                .is_some_and(|p| p.role != "actor");
        places.sort_by_key(|&i| {
            let p = &self.participants[i];
            let first = me_first && i == self.me;
            // The partners not written and anyone at the left edge, where a start comes from; the
            // places not written at the right edge, past every store.
            match (first, p.role) {
                (_, "noPartner") => (0, 0, i),
                (_, "anyone") => (1, 0, i),
                (true, _) => (2, 0, i),
                (_, "actor") => (
                    3,
                    p.section
                        .as_deref()
                        .map_or(usize::MAX, |s| self.actors.order(s)),
                    i,
                ),
                (_, "noStore") => (5, 0, i),
                _ => (4, 0, i),
            }
        });
        let mut order = vec![0; places.len()];
        for (at, &i) in places.iter().enumerate() {
            order[i] = at;
        }
        let drawn_actors: Vec<&str> = self
            .participants
            .iter()
            .filter(|p| p.role == "actor")
            .filter_map(|p| p.section.as_deref())
            .collect();
        // An actor drawn beside actors inside it is already open: its own lifeline goes first in its
        // own bracket, so that one actor is never drawn folded and open at once.
        let opened = |s: &str| {
            drawn_actors
                .iter()
                .any(|&o| o != s && self.actors.contains(s, o))
        };
        let drawn = places
            .iter()
            .map(|&i| {
                let p = &self.participants[i];
                let own = p.role == "actor" && p.section.as_deref().is_some_and(opened);
                let group = (p.role == "actor")
                    .then(|| {
                        p.section.as_deref().and_then(|s| {
                            if own {
                                Some(s.to_string())
                            } else {
                                self.actors.parent_actor(s)
                            }
                        })
                    })
                    .flatten();
                Drawn {
                    label: match (&p.with_section, shared[p.label.as_str()]) {
                        (Some(longer), n) if n > 1 => longer.clone(),
                        _ => p.label.clone(),
                    },
                    name: p.name.clone(),
                    place: p.place.clone(),
                    role: p.role,
                    card: p.card.clone(),
                    section: p.section.clone(),
                    figure: p.figure.clone(),
                    group_name: group.as_deref().and_then(|g| {
                        self.project
                            .detail
                            .sections
                            .iter()
                            .find(|s| s.id == g)
                            .map(|s| s.name.clone())
                    }),
                    group,
                    // Folded, nothing inside it is drawn: whatever it deals with inside comes out on opening.
                    open: !own
                        && p.section.as_deref().is_some_and(|s| {
                            self.dealt
                                .iter()
                                .any(|a| a != s && self.actors.contains(s, a))
                        }),
                    origin: p.origin.clone(),
                }
            })
            .collect();
        (drawn, order)
    }

    /// Whether this is an actor's chart (of flows in frames), not one flow's.
    fn charting(&self) -> bool {
        self.card.kind != "flow"
    }

    /// A flow it names, as the participant it is drawn as, and the flow; a name not there, as a
    /// program.
    fn peer(&mut self, peer: &str) -> (usize, Option<&'a Card>) {
        if let Some(target) = self
            .project
            .find_seeing(peer, &self.project.section_of(self.card))
        {
            return (self.flow_participant(target), Some(target));
        }
        (self.participant(peer.to_string()), None)
    }

    /// The one lane of the partners not written, said where its first gap is: the begin when the
    /// flow receives, else the row or the end drawn now.
    fn partner_gap(&mut self) -> Other<'a> {
        let begin = self
            .card
            .nodes
            .iter()
            .find(|n| n.kind == "begin")
            .filter(|_| self.has_caller)
            .map(|n| n.name.clone());
        let origin = match &begin {
            Some(name) => self.origin_of(name),
            None => self.at.clone(),
        };
        let place = origin.as_ref().map(|o| {
            let mut said = self.card.name.clone();
            if let Some(node) = &o.node {
                said = format!("{said}.{node}");
            }
            if let Some(row) = o.row {
                said = format!("{said}.steps[{row}]");
            }
            said
        });
        let label = text!("charts.participant.noPartner");
        let at = self.add(Participant {
            key: "nopartner".into(),
            place,
            origin,
            ..Participant::named(label.clone(), "noPartner")
        });
        Other {
            said: label,
            at,
            flow: None,
        }
    }

    /// The one lane of whoever may call a begin open to anyone.
    fn anyone(&mut self) -> Other<'a> {
        let label = text!("charts.participant.anyone");
        let at = self.add(Participant {
            key: "anyone".into(),
            ..Participant::named(label.clone(), "anyone")
        });
        Other {
            said: label,
            at,
            flow: None,
        }
    }

    /// Whom the begin receives from, as what names no one deals with: those it names, the flow whose
    /// bar this one is drawn in, anyone, or the gap of a begin that names no one.
    fn begins_partners(&mut self) -> Vec<Other<'a>> {
        if !self.answered.is_empty() || self.answered_program.is_some() {
            let (peers, program) = (self.answered.clone(), self.answered_program.clone());
            return self.named(&peers, &program);
        }
        if let Some(at) = self.reacher {
            return vec![Other {
                said: self.participants[at].label.clone(),
                at,
                flow: None,
            }];
        }
        vec![if self.answered_anyone {
            self.anyone()
        } else {
            self.partner_gap()
        }]
    }

    /// Each one it deals with, how a way to it is named and the participant it is drawn as: the
    /// flows it names, then the program. Naming no one, the begin's partners when the flow receives;
    /// in a flow that does not, a gap of its own.
    fn others(&mut self, peers: &[String], program: &Option<String>) -> Vec<Other<'a>> {
        let out = self.named(peers, program);
        if !out.is_empty() {
            return out;
        }
        if self.has_caller {
            self.begins_partners()
        } else {
            vec![self.partner_gap()]
        }
    }

    /// The flows named, then the program, each as it is drawn.
    fn named(&mut self, peers: &[String], program: &Option<String>) -> Vec<Other<'a>> {
        let mut out: Vec<Other<'a>> = peers
            .iter()
            .map(|p| {
                let (at, flow) = self.peer(p);
                Other {
                    said: short(std::slice::from_ref(p)),
                    at,
                    flow,
                }
            })
            .collect();
        if let Some(program) = program {
            out.push(Other {
                said: program.clone(),
                at: self.participant(program.clone()),
                flow: None,
            });
        }
        out
    }

    fn others_of(&mut self, step: &Step) -> Vec<Other<'a>> {
        self.others(&step.peers, &step.program)
    }

    /// What passes with each of `others`: as it is with one, a way of its own with each of
    /// several (from the partner, to the partner), as only one of them is dealt with at a time.
    fn each_of(
        &mut self,
        others: Vec<Other<'a>>,
        direction: Direction,
        mut items: impl FnMut(&mut Self, &Other<'a>) -> Vec<Item>,
    ) -> Vec<Item> {
        if others.len() == 1 {
            return items(self, &others[0]);
        }
        let sections = others
            .iter()
            .map(|other| {
                let label = match direction {
                    Direction::From => text!("charts.way.from", said = other.said),
                    Direction::To => text!("charts.way.to", said = other.said),
                };
                (label, items(self, other))
            })
            .collect();
        vec![Item::Block {
            kind: "alt",
            sections,
            origin: self.at.clone(),
            frame: None,
        }]
    }

    /// Where a path is kept (`store_of`).
    fn store_of(&self, head: &'a Card) -> Kept<'a> {
        store_of(self.project, head, &|| self.tick())
    }

    /// Where a path is kept: the actor that holds its store, the store, or the one lane of the
    /// places not written, which says the shapes no one store keeps — each that reaches several
    /// with them, as none is picked — and where the first of them is.
    fn place(&mut self, path: &str) -> usize {
        let head = identify_head(self.project, path, self.card);
        let kept = head.map(|h| (h, self.store_of(h)));
        let Some((_, Kept::In(store))) = kept else {
            let at = self.add(Participant {
                key: "nostore".into(),
                origin: self.at.clone(),
                ..Participant::named(text!("charts.participant.noStore"), "noStore")
            });
            if let Some((head, kept)) = kept
                && !self.kept_nowhere.iter().any(|(name, _)| *name == head.name)
            {
                let said = match kept {
                    Kept::Undecided(stores) => text!(
                        "charts.participant.noStoreUndecided",
                        shape = head.name,
                        stores = crate::text::list(
                            &stores.iter().map(|s| s.name.as_str()).collect::<Vec<_>>()
                        )
                    ),
                    _ => head.name.clone(),
                };
                self.kept_nowhere.push((head.name.clone(), said));
                self.participants[at].place = Some(
                    self.kept_nowhere
                        .iter()
                        .map(|(_, said)| said.as_str())
                        .collect::<Vec<_>>()
                        .join(", "),
                );
            }
            return at;
        };
        if let Some(actor) = self.actors.store_actor(store, self.own_actor.as_deref()) {
            return self.actor_lifeline(actor);
        }
        let kept_in = {
            let place = self
                .project
                .exported_section(&self.project.section_of(store));
            crate::pathtext::card_path(&place, &store.name)
        };
        self.add(Participant {
            key: format!("store:{}", store.id),
            label: text!("charts.participant.store", name = store.name),
            with_section: Some(text!("charts.participant.store", name = kept_in)),
            name: store.name.clone(),
            place: None,
            role: "store",
            card: Some(store.id.clone()),
            section: None,
            figure: None,
            origin: None,
        })
    }

    /// The names that two cards or more of those `flow`'s rows read or write have.
    fn clashing_in(&self, flow: &'a Card) -> HashSet<String> {
        fn paths<'s>(steps: &'s [Step], out: &mut Vec<&'s String>) {
            for step in steps {
                if step.op == "read" || step.op == "write" {
                    out.extend(step.paths.iter());
                }
                paths(&step.steps, out);
                paths(&step.cases, out);
            }
        }
        let mut written = Vec::new();
        for node in &flow.nodes {
            paths(&node.steps, &mut written);
        }
        let mut ids: HashMap<&str, HashSet<&str>> = HashMap::new();
        for path in written {
            if let Some(card) = identify_head(self.project, path, flow) {
                ids.entry(card.name.as_str())
                    .or_default()
                    .insert(card.id.as_str());
            }
        }
        ids.into_iter()
            .filter(|(_, cards)| cards.len() > 1)
            .map(|(name, _)| name.to_string())
            .collect()
    }

    /// What a row reads or writes, as its arrow says it: each card by its name (with its section
    /// where another card the flow reads or writes has the name), its fields after it, one card
    /// after another.
    fn data(&self, paths: &[&String]) -> String {
        let mut said: Vec<String> = Vec::new();
        let mut last: Option<String> = None;
        for path in paths {
            let rest = crate::pathtext::split_section(path).1;
            let names = crate::pathtext::dots(rest);
            let (head, fields) = names.split_first().map_or(("", &[][..]), |(h, f)| (*h, f));
            let card = identify_head(self.project, path, self.card);
            let shared = card.is_some_and(|c| self.clashing.contains(&c.name));
            let named = match card {
                Some(c) if shared => self.project.exported(c),
                _ => head.to_string(),
            };
            let fields = fields.join(".");
            if last.as_deref() == Some(named.as_str()) && !fields.is_empty() {
                said.push(fields);
                continue;
            }
            said.push(if fields.is_empty() {
                named.clone()
            } else {
                format!("{named}.{fields}")
            });
            last = Some(named);
        }
        said.join(", ")
    }

    fn plain(&self, text: &str) -> String {
        if self.exported {
            let mounts = self.project.work_dir_names();
            notetext::transform(text, |r| Some(notetext::shown(r, &mounts)))
        } else {
            notetext::transform(text, |r| {
                Some(r.shown.clone().unwrap_or_else(|| exported_path(&r.path)))
            })
        }
    }

    /// What a row says of itself, as the screen shows it beside what it draws.
    fn described(&self, description: &Option<String>) -> Option<String> {
        description
            .as_deref()
            .filter(|d| !d.trim().is_empty())
            .map(|d| self.plain(d))
    }

    #[allow(clippy::too_many_arguments)]
    fn message(
        &self,
        from: usize,
        to: usize,
        op: Option<&str>,
        text: String,
        flow: Option<String>,
        reply: bool,
        description: &Option<String>,
    ) -> Item {
        Item::Message {
            from,
            to,
            op: op.map(str::to_string),
            text: self.plain(&text),
            flow,
            reply,
            origin: self.at.clone(),
            description: self.described(description),
        }
    }

    /// What passes to `other` from this flow: an arrow — naming the flow it reaches on another
    /// actor's lifeline when it goes on (`forward`: a run, a send, a call) — or, the flow on this very
    /// lifeline, a bar on it named with that flow (`same_lifeline`), holding what that flow does where a
    /// call goes on.
    #[allow(clippy::too_many_arguments)]
    fn outgoing(
        &mut self,
        other: &Other<'a>,
        op: &str,
        text: String,
        forward: bool,
        reply: bool,
        description: &Option<String>,
    ) -> Item {
        if other.at == self.me
            && let Some(flow) = other.flow
        {
            return self.same_lifeline(flow, op, forward);
        }
        let named = forward
            && self
                .participants
                .get(other.at)
                .is_some_and(|p| p.role == "actor");
        self.message(
            self.me,
            other.at,
            Some(op),
            text,
            other.flow.filter(|_| named).map(|f| f.name.clone()),
            reply,
            description,
        )
    }

    /// What comes from `other` to this flow: an arrow, or the flow on this very lifeline a bar that holds
    /// nothing.
    fn incoming(
        &self,
        other: &Other<'a>,
        op: Option<&str>,
        text: String,
        reply: bool,
        description: &Option<String>,
    ) -> Item {
        if other.at == self.me
            && let Some(flow) = other.flow
        {
            return Item::Step {
                over: self.me,
                node: None,
                label: words(&[
                    op.map(str::to_string),
                    Some(flow.name.clone()),
                    Some(flow.label.clone()),
                ]),
                items: Vec::new(),
                origin: self.at.clone(),
                called: Some(self.called(op.unwrap_or_default(), flow)),
            };
        }
        self.message(other.at, self.me, op, text, None, reply, description)
    }

    fn note(&self, text: String, kind: NoteKind) -> Item {
        Item::Note {
            over: self.me,
            text: self.plain(&text),
            kind,
            origin: self.at.clone(),
            description: None,
        }
    }

    fn trigger_of(&self, begin: &FlowNode) -> Option<Trigger> {
        let blank = |on: &str| Trigger {
            on: on.to_string(),
            peers: Vec::new(),
            program: None,
            carries: Vec::new(),
            via: None,
            initiator: None,
            verify_with: None,
            every: None,
            description: begin.description.clone(),
            said: None,
        };
        match begin.trigger.as_deref() {
            Some("receive") => {
                let mut t = blank("receive");
                t.peers = begin.peers.clone();
                t.program = begin.program.clone();
                t.carries = begin.carries.clone();
                t.via = begin.via.clone();
                t.initiator = begin.initiator.clone();
                t.verify_with = begin.verify_with.clone();
                Some(t)
            }
            Some("schedule") => {
                let mut t = blank("schedule");
                t.every = begin.every.clone();
                Some(t)
            }
            Some(other) => {
                let mut t = blank(other);
                if other == "event"
                    && let Some(e) = &begin.event
                {
                    t.said = Some(text!("charts.event.of", flow = e.flow, outcome = e.outcome));
                }
                Some(t)
            }
            None => None,
        }
    }

    fn starting(&mut self, trigger: &Trigger) -> Vec<Item> {
        let mut items = Vec::new();
        let described = trigger.description.clone();
        match trigger.on.as_str() {
            "receive" => {
                let others = self.others(&trigger.peers, &trigger.program);
                items.extend(self.each_of(others, Direction::From, |me, from| {
                    if trigger.initiator.as_deref() == Some("self") {
                        let fetch = text!("charts.op.fetch");
                        vec![
                            me.outgoing(
                                from,
                                &fetch,
                                words(std::slice::from_ref(&trigger.via)),
                                false,
                                false,
                                &described,
                            ),
                            me.incoming(from, None, short(&trigger.carries), true, &described),
                        ]
                    } else {
                        vec![me.incoming(
                            from,
                            Some("receive"),
                            words(&[Some(short(&trigger.carries)), trigger.via.clone()]),
                            false,
                            &described,
                        )]
                    }
                }));
                if let Some(key) = &trigger.verify_with {
                    items.push(self.note(
                        words(&[
                            Some("verify".into()),
                            Some(short(std::slice::from_ref(key))),
                        ]),
                        NoteKind::Key,
                    ));
                }
            }
            "schedule" => {
                items.push(self.note(
                    words(&[Some("schedule".into()), trigger.every.clone()]),
                    NoteKind::Plain,
                ));
            }
            _ => {
                items.push(self.note(
                    words(&[Some(trigger.on.clone()), trigger.said.clone()]),
                    NoteKind::Plain,
                ));
            }
        }
        if let Some(Item::Note { description, .. }) = items.last_mut()
            && trigger.on != "receive"
        {
            *description = self.described(&trigger.description);
        }
        items
    }

    fn rows(&mut self, steps: &[Step]) -> Vec<Item> {
        let mut items = Vec::new();
        for step in steps {
            let description = &step.description;
            match step.op.as_str() {
                "receive" => {
                    let others = self.others_of(step);
                    items.extend(self.each_of(others, Direction::From, |me, from| {
                        vec![me.incoming(
                            from,
                            Some("receive"),
                            words(&[Some(short(&step.carries)), step.via.clone()]),
                            false,
                            description,
                        )]
                    }));
                }
                "return" => {
                    let others = self.others_of(step);
                    items.extend(self.each_of(others, Direction::To, |me, to| {
                        vec![me.outgoing(
                            to,
                            "return",
                            short(&step.carries),
                            false,
                            true,
                            description,
                        )]
                    }));
                }
                // A send goes to each it names, one after another.
                "send" => {
                    for to in self.others_of(step) {
                        items.push(self.outgoing(
                            &to,
                            "send",
                            words(&[Some(short(&step.carries)), step.via.clone()]),
                            true,
                            false,
                            description,
                        ));
                        items.extend(self.reached(&to, "send"));
                    }
                }
                "call" => {
                    let to = self.others_of(step).swap_remove(0);
                    items.push(self.outgoing(
                        &to,
                        "call",
                        words(&[Some(short(&step.carries)), step.via.clone()]),
                        true,
                        false,
                        description,
                    ));
                    items.extend(self.reached(&to, "call"));
                    if !step.returns.is_empty() && to.at != self.me {
                        items.push(self.message(
                            to.at,
                            self.me,
                            None,
                            short(&step.returns),
                            None,
                            true,
                            &None,
                        ));
                    }
                }
                // One arrow to each place the row reads or writes, saying the fields there.
                "read" | "write" => {
                    let mut by_place: Vec<(usize, Vec<&String>)> = Vec::new();
                    for path in &step.paths {
                        let at = self.place(path);
                        match by_place.iter_mut().find(|(p, _)| *p == at) {
                            Some((_, paths)) => paths.push(path),
                            None => by_place.push((at, vec![path])),
                        }
                    }
                    for (at, paths) in by_place {
                        let said = self.data(&paths);
                        items.push(self.message(
                            self.me,
                            at,
                            Some(&step.op),
                            said,
                            None,
                            false,
                            description,
                        ));
                    }
                }
                "sign" | "verify" => {
                    let mut note = self.note(
                        words(&[
                            Some(step.op.clone()),
                            step.key.as_ref().map(|k| short(std::slice::from_ref(k))),
                            Some(short(&step.paths)),
                        ]),
                        NoteKind::Key,
                    );
                    if let Item::Note { description: d, .. } = &mut note {
                        *d = self.described(description);
                    }
                    items.push(note);
                }
                "note" => {
                    items.push(self.note(words(std::slice::from_ref(description)), NoteKind::Plain))
                }
                // What the flow works out by itself, over itself: what the row says it does.
                "compute" => items.push(
                    self.note(
                        description
                            .as_deref()
                            .filter(|d| !d.trim().is_empty())
                            .map_or_else(|| step.op.clone(), str::to_string),
                        NoteKind::Plain,
                    ),
                ),
                "sequence" => items.extend(self.rows(&step.steps)),
                "unordered" => {
                    if !step.steps.is_empty() {
                        let sections = step
                            .steps
                            .iter()
                            .map(|child| (String::new(), self.rows(std::slice::from_ref(child))))
                            .collect();
                        items.push(Item::Block {
                            kind: "par",
                            sections,
                            origin: self.at.clone(),
                            frame: None,
                        });
                    }
                }
                "switch" => {
                    if !step.cases.is_empty() {
                        let mut sections = Vec::new();
                        for case in &step.cases {
                            let label = self.plain(case.when.as_deref().unwrap_or(""));
                            let inner = if case.steps.is_empty() {
                                vec![self.note(text!("charts.note.nothing"), NoteKind::Plain)]
                            } else {
                                self.rows(&case.steps)
                            };
                            sections.push((label, inner));
                        }
                        items.push(Item::Block {
                            kind: "alt",
                            sections,
                            origin: self.at.clone(),
                            frame: None,
                        });
                    }
                }
                "loop" => {
                    let label = self.plain(step.over.as_deref().unwrap_or(""));
                    let inner = self.rows(&step.steps);
                    items.push(Item::Block {
                        kind: "loop",
                        sections: vec![(label, inner)],
                        origin: self.at.clone(),
                        frame: None,
                    });
                }
                _ => {}
            }
        }
        items
    }

    /// Where an item drawn now of the node `node` comes from.
    fn origin_of(&self, node: &str) -> Option<Origin> {
        Some(Origin {
            card: self.card.id.clone(),
            node: Some(node.to_string()),
            row: None,
        })
    }

    /// One more node or row looked at: false once there have been more than `MOST_STEPS`.
    fn tick(&self) -> bool {
        let steps = self.steps.get() + 1;
        self.steps.set(steps);
        steps <= MOST_STEPS
    }

    /// The ways out of `name` a walk forward follows: not those that go back.
    fn forward(&self, name: &str) -> impl Iterator<Item = &String> {
        self.outs
            .get(name)
            .into_iter()
            .flatten()
            .filter(move |to| !self.back.contains(&(name.to_string(), (*to).clone())))
    }

    /// The ways that go back (`back`) and where to (`heads`), as a walk from `begin` that
    /// goes each way once finds them: a way to a node it is still on the way from.
    fn ways_back(&mut self, begin: &str) {
        for node in &self.card.nodes {
            let outs: Vec<String> = node.exits().into_iter().map(|e| e.0).collect();
            for to in &outs {
                self.ins
                    .entry(to.clone())
                    .or_default()
                    .push(node.name.clone());
            }
            self.outs.entry(node.name.clone()).or_insert(outs);
        }
        let mut seen: HashSet<String> = HashSet::from([begin.to_string()]);
        let mut on: HashSet<String> = seen.clone();
        let mut stack: Vec<(String, usize)> = vec![(begin.to_string(), 0)];
        while let Some((name, at)) = stack.last().cloned() {
            let to = self.outs.get(&name).and_then(|o| o.get(at)).cloned();
            let Some(to) = to else {
                on.remove(&name);
                stack.pop();
                continue;
            };
            if let Some(top) = stack.last_mut() {
                top.1 += 1;
            }
            if on.contains(&to) {
                if self.back.insert((name.clone(), to.clone())) {
                    self.heads.entry(to).or_default().push(name);
                }
            } else if self.by_name.contains_key(to.as_str()) && seen.insert(to.clone()) {
                on.insert(to.clone());
                stack.push((to, 0));
            }
        }
    }

    /// The nodes of the loop that goes back to `head`: it, and each node a way back to it is
    /// come to from without passing it.
    fn body(&self, head: &str) -> HashSet<String> {
        let mut body = HashSet::from([head.to_string()]);
        let mut queue: Vec<String> = self.heads.get(head).cloned().unwrap_or_default();
        while let Some(name) = queue.pop() {
            if !self.tick() {
                break;
            }
            if body.insert(name.clone()) {
                queue.extend(self.ins.get(&name).into_iter().flatten().cloned());
            }
        }
        body
    }

    /// A loop, come to at `head`: a decision whose one way goes round and back to it while the
    /// others go on is a `loop` of that way's condition; any other, a `loop` from `head` to the way
    /// back, of the way back's condition. Each way out of the loop but the one it goes on by is
    /// drawn where it leaves. Where it goes on after the loop: the way out of the node the way back
    /// is from, or else the first way out of it.
    fn looped(
        &mut self,
        head: &'a FlowNode,
        stops: &HashSet<String>,
    ) -> (Vec<Item>, Option<String>) {
        let body = self.body(&head.name);
        if head.kind == "eval"
            && let [way] = &head.branches[..]
            && body.contains(&way.next)
            && head.otherwise.as_ref().is_none_or(|o| !body.contains(o))
        {
            self.drawn.insert(head.name.clone());
            let mut until = stops.clone();
            until.insert(head.name.clone());
            until.extend(head.otherwise.clone());
            let inner = self.walk(Some(way.next.clone()), &until);
            let label = self.plain(&way.when);
            return (
                vec![Item::Block {
                    kind: "loop",
                    sections: vec![(label, inner)],
                    origin: self.origin_of(&head.name),
                    frame: None,
                }],
                head.otherwise.clone(),
            );
        }
        let tails = self.heads.get(&head.name).cloned().unwrap_or_default();
        let tail = match &tails[..] {
            [one] => Some(one.clone()),
            _ => None,
        };
        let mut leaving: Vec<(String, String)> = Vec::new();
        for node in &self.card.nodes {
            if !body.contains(&node.name) {
                continue;
            }
            for to in self.outs.get(&node.name).into_iter().flatten() {
                if !body.contains(to) && self.by_name.contains_key(to.as_str()) {
                    leaving.push((node.name.clone(), to.clone()));
                }
            }
        }
        let from_tail: Vec<&String> = leaving
            .iter()
            .filter(|(from, _)| Some(from) == tail.as_ref())
            .map(|(_, to)| to)
            .collect();
        let on = match &from_tail[..] {
            [one] => Some((*one).clone()),
            _ => leaving
                .iter()
                .find(|(_, to)| !self.ends(to))
                .or(leaving.first())
                .map(|(_, to)| to.clone()),
        };
        // Named by the way back: its condition, or what way of its node it is.
        let label = tail
            .as_deref()
            .and_then(|t| self.by_name.get(t).copied())
            .and_then(|t| {
                if let Some(b) = t.branches.iter().find(|b| b.next == head.name) {
                    return Some(self.plain(&b.when));
                }
                let back = Some(head.name.as_str());
                if t.otherwise.as_deref() == back {
                    // Back unless the one condition holds: until it does.
                    Some(match &t.branches[..] {
                        [one] => text!("charts.loop.until", when = self.plain(&one.when)),
                        _ => text!("charts.way.otherwise"),
                    })
                } else if t.on_timeout.as_deref() == back {
                    Some(text!("charts.way.timeout"))
                } else if t.on_error.as_deref() == back {
                    Some(text!("charts.way.failure"))
                } else if t.kind == "wait" && t.on_timeout.is_some() {
                    Some(text!("charts.way.inTime"))
                } else {
                    None
                }
            })
            .unwrap_or_else(|| text!("charts.loop.repeat"));
        if let Some(t) = tail {
            self.silent.insert((t, head.name.clone()));
        }
        let mut until = stops.clone();
        until.extend(on.clone());
        let inner = self.walk(Some(head.name.clone()), &until);
        (
            vec![Item::Block {
                kind: "loop",
                sections: vec![(label, inner)],
                origin: self.origin_of(&head.name),
                frame: None,
            }],
            on,
        )
    }

    /// Where ways that part meet: the first join every one of them reaches.
    fn meeting(&self, ways: &[String]) -> Option<String> {
        let joins = |start: &str| -> Vec<String> {
            let mut seen: HashSet<String> = HashSet::new();
            let mut queue: VecDeque<String> = VecDeque::from([start.to_string()]);
            let mut found = Vec::new();
            while let Some(name) = queue.pop_front() {
                if !self.tick() {
                    break;
                }
                if !seen.insert(name.clone()) {
                    continue;
                }
                let Some(node) = self.by_name.get(name.as_str()) else {
                    continue;
                };
                if node.kind == "join" {
                    found.push(name);
                    continue;
                }
                queue.extend(self.forward(&name).cloned());
            }
            found
        };
        let first = ways.first()?;
        let rest: Vec<HashSet<String>> = ways[1..]
            .iter()
            .map(|w| joins(w).into_iter().collect())
            .collect();
        joins(first)
            .into_iter()
            .find(|j| rest.iter().all(|r| r.contains(j)))
    }

    /// Each node `start` reaches — not going back through `from`, nor on past `stops`, nor by a
    /// way back — and how far along it is, in the order reached.
    fn reach(&self, start: &str, from: &str, stops: &HashSet<String>) -> Vec<(String, usize)> {
        let mut seen: HashSet<String> = HashSet::new();
        let mut queue: VecDeque<(String, usize)> = VecDeque::from([(start.to_string(), 0)]);
        let mut found = Vec::new();
        while let Some((name, far)) = queue.pop_front() {
            if !self.tick() {
                break;
            }
            if name == from || !seen.insert(name.clone()) {
                continue;
            }
            found.push((name.clone(), far));
            if stops.contains(&name) {
                continue;
            }
            queue.extend(self.forward(&name).map(|to| (to.clone(), far + 1)));
        }
        found
    }

    /// Where ways that part at `from` come together again: of the nodes every one of them
    /// reaches — not going back through `from`, nor on past `stops` — the nearest, the one
    /// the farthest of them reaches soonest. None for fewer than two ways.
    fn rejoin(&self, from: &str, ways: &[&str], stops: &HashSet<String>) -> Option<String> {
        if ways.len() < 2 {
            return None;
        }
        let first = self.reach(ways[0], from, stops);
        let rest: Vec<HashMap<String, usize>> = ways[1..]
            .iter()
            .map(|w| self.reach(w, from, stops).into_iter().collect())
            .collect();
        first
            .iter()
            .filter_map(|(name, far)| {
                let fars: Option<Vec<usize>> = rest.iter().map(|r| r.get(name).copied()).collect();
                let fars = fars?;
                let farthest = fars.iter().copied().chain([*far]).max().unwrap_or(0);
                let all: usize = fars.iter().sum::<usize>() + far;
                Some((farthest, all, name))
            })
            .enumerate()
            .min_by_key(|(order, (farthest, all, _))| (*farthest, *all, *order))
            .map(|(_, (_, _, name))| name.clone())
    }

    /// Ways that part at `from` (a decision's, a wait's), each its own until they come
    /// together again, as an `alt`, a quiet way with nothing of its own (the way on) left out.
    /// Where they do not all come together, those that come together with none of the others
    /// (a way to an end of its own) are left to stop where they stop, and the others still
    /// come together. A way that stops, where the ways go on after the alt, says how: what it
    /// answers, or — it answers nothing, or it went wrong — its end too. Where it goes on:
    /// where they come together, none when they do not.
    fn parting(
        &mut self,
        from: &str,
        ways: &[Way],
        stops: &HashSet<String>,
    ) -> (Option<Item>, Option<String>) {
        let all: Vec<&str> = ways.iter().map(|w| w.to.as_str()).collect();
        let meet = self.rejoin(from, &all, stops).or_else(|| {
            let reached: Vec<HashSet<String>> = all
                .iter()
                .map(|w| {
                    self.reach(w, from, stops)
                        .into_iter()
                        .map(|(n, _)| n)
                        .collect()
                })
                .collect();
            let on: Vec<&str> = all
                .iter()
                .enumerate()
                .filter(|(i, _)| {
                    reached
                        .iter()
                        .enumerate()
                        .any(|(j, other)| j != *i && !other.is_disjoint(&reached[*i]))
                })
                .map(|(_, w)| *w)
                .collect();
            self.rejoin(from, &on, stops)
        });
        let mut until = stops.clone();
        if let Some(m) = &meet {
            until.insert(m.clone());
        }
        let mut sections = Vec::new();
        for way in ways {
            let inner = if !until.contains(&way.to) && self.ends(&way.to) {
                self.stopping(&way.to)
            } else {
                let mut inner = self.walk(Some(way.to.clone()), &until);
                // Stopped at an end of its own while the others go on: how, when it is not said.
                if meet.is_some()
                    && let Some(end) = self.ended.take()
                    && self.unanswered(&end)
                {
                    inner.extend(self.stop_note(&end));
                }
                inner
            };
            if inner.is_empty() {
                if way.quiet {
                    continue;
                }
                // The place it goes on to is drawn after the alt: the way says where that is.
                let on = self
                    .by_name
                    .get(way.to.as_str())
                    .and_then(|n| n.label.clone())
                    .filter(|l| !l.trim().is_empty())
                    .unwrap_or_else(|| way.to.clone());
                sections.push((
                    way.label.clone(),
                    vec![self.note(text!("charts.note.goesOn", name = on), NoteKind::Plain)],
                ));
            } else {
                sections.push((way.label.clone(), inner));
            }
        }
        let block = (!sections.is_empty()).then_some(Item::Block {
            kind: "alt",
            sections,
            origin: self.origin_of(from),
            frame: None,
        });
        (block, meet)
    }

    /// Whether an end says nothing of how it ends by what it answers: it answers no one — it says
    /// nothing to answer, and nobody who waits started the flow.
    fn unanswered(&self, name: &str) -> bool {
        self.by_name
            .get(name)
            .is_some_and(|n| n.reply.is_none() && !self.has_caller)
    }

    /// An end's note: what it is called, or how it ends.
    fn stop_note(&mut self, name: &str) -> Option<Item> {
        let node = self.by_name.get(name).copied()?;
        self.at = self.origin_of(name);
        Some(self.note(Self::ending(node), NoteKind::End))
    }

    /// What an end is called, else how it ends.
    fn ending(node: &FlowNode) -> String {
        node.label
            .clone()
            .or_else(|| node.outcome.clone())
            .unwrap_or_else(|| "end".into())
    }

    /// A way that goes straight to the end `name`: what it answers, how it ends where it answers no one.
    fn stopping(&mut self, name: &str) -> Vec<Item> {
        self.stop(name)
    }

    /// Whether the node is an end.
    fn ends(&self, name: &str) -> bool {
        self.by_name.get(name).is_some_and(|n| n.kind == "end")
    }

    fn failing(&self, name: Option<&str>) -> bool {
        let Some(node) = name.and_then(|n| self.by_name.get(n)) else {
            return false;
        };
        node.kind == "end" && matches!(node.outcome.as_deref(), Some("failure") | Some("cancelled"))
    }

    /// Whether a way to `name` only fails: straight to an end that fails and says nothing it
    /// answers — drawn as one mark, not a frame.
    fn fails_bare(&self, name: Option<&str>) -> bool {
        self.failing(name)
            && name
                .and_then(|n| self.by_name.get(n))
                .is_some_and(|n| n.reply.is_none())
    }

    /// A way that only fails, as one line on this flow's lifeline: the way, how it ends and — when
    /// someone waits for the flow — to whom that goes.
    fn mark(&mut self, way: String, end: &str) -> Item {
        let node = self.by_name.get(end).copied();
        let outcome = node
            .and_then(|n| n.outcome.clone())
            .unwrap_or_else(|| "end".into());
        // Inside the bar of a call on its caller's own lifeline, the way back is the end of that bar.
        let back = if self.has_caller {
            let to: Vec<String> = self
                .begins_partners()
                .iter()
                .filter(|o| !(self.unfolded && o.at == self.me))
                .map(|o| self.participants[o.at].label.clone())
                .collect();
            if to.is_empty() {
                String::new()
            } else {
                text!("charts.mark.back", to = crate::text::list(&to))
            }
        } else {
            String::new()
        };
        self.note(
            text!("charts.mark.failed", way, outcome, back),
            NoteKind::Mark,
        )
    }

    /// What an end answers, to those it names — or, naming none, whoever called — saying how the
    /// flow ended; one that says nothing to answer answers those its begin received from, when
    /// someone waits for the flow. None when it answers no one. Drawn inside the bar of a call on its
    /// caller's own lifeline, an answer to that lifeline is no arrow and no bar back: the end's own note.
    fn answer(&mut self, node: &FlowNode) -> Option<Vec<Item>> {
        if node.kind != "end" {
            return None;
        }
        let outcome = node.outcome.clone().unwrap_or_default();
        let (carries, others) = match &node.reply {
            Some(reply) => (
                reply.carries.clone(),
                self.others(&reply.peers, &reply.program),
            ),
            None if self.has_caller => (Vec::new(), self.begins_partners()),
            None => return None,
        };
        let others = if self.unfolded {
            let (_, away): (Vec<_>, Vec<_>) = others.into_iter().partition(|o| o.at == self.me);
            if away.is_empty() {
                return Some(self.stop_note(&node.name).into_iter().collect());
            }
            away
        } else {
            others
        };
        let said = if carries.is_empty() {
            outcome
        } else {
            text!("charts.answer", carried = short(&carries), outcome)
        };
        let description = node.description.clone();
        Some(self.each_of(others, Direction::To, |me, to| {
            vec![me.outgoing(to, "return", said.clone(), false, true, &description)]
        }))
    }

    fn call_row(node: &FlowNode) -> Option<Step> {
        if node.kind != "run" {
            return None;
        }
        let target = node.target.clone()?;
        Some(Step {
            op: "call".into(),
            peers: vec![target],
            carries: node.carries.clone(),
            returns: node.returns.clone(),
            via: node.via.clone(),
            sign_with: node.sign_with.clone(),
            verify_with: node.verify_with.clone(),
            description: node.description.clone(),
            ..Default::default()
        })
    }

    /// An end: what it answers, else how it ends.
    fn stop(&mut self, name: &str) -> Vec<Item> {
        let Some(node) = self.by_name.get(name).copied() else {
            return Vec::new();
        };
        self.at = self.origin_of(name);
        match self.answer(node) {
            Some(items) => items,
            None => self.stop_note(name).into_iter().collect(),
        }
    }

    /// A process node: a bar named by the node, its rows inside it.
    fn process(&mut self, node: &FlowNode) -> Vec<Item> {
        let mut items = Vec::new();
        for (index, step) in node.steps.iter().enumerate() {
            self.at = Some(Origin {
                card: self.card.id.clone(),
                node: Some(node.name.clone()),
                row: Some(index),
            });
            items.extend(self.rows(std::slice::from_ref(step)));
        }
        vec![self.bar(node, items)]
    }

    /// The bar of a node on this flow's lifeline: its name and heading, what it does inside it.
    fn bar(&mut self, node: &FlowNode, items: Vec<Item>) -> Item {
        Item::Step {
            over: self.me,
            node: Some(node.name.clone()),
            label: node.label.clone().unwrap_or_default(),
            items,
            origin: self.origin_of(&node.name),
            called: None,
        }
    }

    /// A run node: a bar, the call inside it — to the flow run, named by its heading and name — and
    /// what comes back.
    fn run(&mut self, node: &FlowNode) -> Vec<Item> {
        self.at = self.origin_of(&node.name);
        let Some(call) = Self::call_row(node) else {
            return vec![self.bar(node, Vec::new())];
        };
        let to = self.others_of(&call).swap_remove(0);
        let said = words(&[
            to.flow.map(|f| f.label.clone()),
            Some(short(&call.carries)),
            call.via.clone(),
        ]);
        let mut items = vec![self.outgoing(&to, "run", said, true, false, &call.description)];
        items.extend(self.reached(&to, "run"));
        if !call.returns.is_empty() && to.at != self.me {
            items.push(self.message(
                to.at,
                self.me,
                None,
                short(&call.returns),
                None,
                true,
                &None,
            ));
        }
        vec![self.bar(node, items)]
    }

    /// The nodes from `start` on, until a node of `stops`: each drawn once — a way to one drawn
    /// already says where it goes — ways that part an `alt` (or a `par`) until they meet, a loop a
    /// `loop` (`looped`).
    fn walk(&mut self, start: Option<String>, stops: &HashSet<String>) -> Vec<Item> {
        let mut items = Vec::new();
        let mut current = start;
        let mut came_from: Option<String> = None;
        let mut ended = None;
        while let Some(name) = current.clone() {
            if stops.contains(&name) || !self.tick() {
                break;
            }
            let Some(node) = self.by_name.get(name.as_str()).copied() else {
                break;
            };
            self.at = self.origin_of(&name);
            if node.kind != "end" && self.drawn.contains(&name) {
                let said_by_loop = came_from
                    .as_ref()
                    .is_some_and(|from| self.silent.contains(&(from.clone(), name.clone())));
                if !said_by_loop {
                    items.push(self.note(text!("charts.note.back", name), NoteKind::Plain));
                }
                break;
            }
            if self.heads.contains_key(&name) && self.looping.insert(name.clone()) {
                let (block, on) = self.looped(node, stops);
                items.extend(block);
                came_from = None;
                current = on;
                continue;
            }
            if node.kind != "end" {
                self.drawn.insert(name.clone());
            }
            came_from = Some(name.clone());
            if node.nexts.len() > 1 {
                // Ways that part, each its own until they meet.
                if node.kind == "process" {
                    items.extend(self.process(node));
                }
                if node.kind == "run" {
                    items.extend(self.run(node));
                }
                let meet = self.meeting(&node.nexts);
                let mut until = stops.clone();
                if let Some(m) = &meet {
                    until.insert(m.clone());
                }
                let sections = node
                    .nexts
                    .iter()
                    .map(|way| (String::new(), self.walk(Some(way.clone()), &until)))
                    .collect();
                items.push(Item::Block {
                    kind: "par",
                    sections,
                    origin: self.origin_of(&name),
                    frame: None,
                });
                current = meet
                    .as_deref()
                    .and_then(|m| self.by_name.get(m))
                    .and_then(|n| n.nexts.first().cloned());
                if let Some(m) = meet {
                    came_from = Some(m.clone());
                    self.drawn.insert(m);
                }
                continue;
            }
            let next = node.nexts.first().cloned();
            match node.kind.as_str() {
                "process" => {
                    items.extend(self.process(node));
                    current = next;
                }
                "run" => {
                    items.extend(self.run(node));
                    current = next.clone();
                    if let Some(failed) = node.on_error.clone().filter(|f| Some(f) != next.as_ref())
                    {
                        if self.failing(next.as_deref()) {
                            let on = next.clone().unwrap_or_default();
                            if self.fails_bare(Some(&on)) {
                                items.push(self.mark(text!("charts.way.success"), &on));
                            } else {
                                let stop = self.stop(&on);
                                self.at = self.origin_of(&name);
                                items.push(Item::Block {
                                    kind: "break",
                                    sections: vec![(text!("charts.way.success"), stop)],
                                    origin: self.origin_of(&name),
                                    frame: None,
                                });
                            }
                            current = Some(failed);
                        } else if self.failing(Some(&failed)) {
                            if self.fails_bare(Some(&failed)) {
                                items.push(self.mark(text!("charts.way.failure"), &failed));
                            } else {
                                let stop = self.stop(&failed);
                                items.push(Item::Block {
                                    kind: "break",
                                    sections: vec![(text!("charts.way.failure"), stop)],
                                    origin: self.origin_of(&name),
                                    frame: None,
                                });
                            }
                        } else {
                            let mut until = stops.clone();
                            if let Some(n) = &next {
                                until.insert(n.clone());
                            }
                            let inner = self.walk(Some(failed), &until);
                            items.push(Item::Block {
                                kind: "alt",
                                sections: vec![(text!("charts.way.failure"), inner)],
                                origin: self.origin_of(&name),
                                frame: None,
                            });
                        }
                    }
                }
                "end" => {
                    if let Some(answer) = self.answer(node) {
                        items.extend(answer);
                    }
                    ended = Some(name.clone());
                    current = None;
                }
                "wait" => {
                    let what = if node.wait_for.as_deref() == Some("event") {
                        node.event
                            .as_ref()
                            .map(|e| text!("charts.event.of", flow = e.flow, outcome = e.outcome))
                    } else {
                        node.wait_for.clone()
                    };
                    let what = what.unwrap_or_default();
                    items.push(self.note(
                        match node.timeout.as_deref() {
                            Some(timeout) => text!("charts.note.waitUntil", what, timeout),
                            None => format!("wait {what}"),
                        },
                        NoteKind::Plain,
                    ));
                    current = next.clone();
                    let back = |to: &String| self.silent.contains(&(name.clone(), to.clone()));
                    let late = node
                        .on_timeout
                        .clone()
                        .filter(|t| Some(t) != next.as_ref() && !back(t));
                    // The way back a loop's block says: the time running out is the way on.
                    if next.as_ref().is_some_and(back) && late.is_some() {
                        current = late;
                        continue;
                    }
                    // Where it goes when the time runs out: a break where it stops at
                    // once in failure, else its own way beside the one on.
                    if let Some(late) = late {
                        if self.fails_bare(Some(&late)) {
                            items.push(self.mark(text!("charts.way.timeout"), &late));
                        } else if self.failing(Some(&late)) {
                            let stop = self.stop(&late);
                            items.push(Item::Block {
                                kind: "break",
                                sections: vec![(text!("charts.way.timeout"), stop)],
                                origin: self.origin_of(&name),
                                frame: None,
                            });
                        } else {
                            let mut ways: Vec<Way> = next
                                .iter()
                                .map(|n| Way {
                                    label: text!("charts.way.inTime"),
                                    to: n.clone(),
                                    quiet: true,
                                })
                                .collect();
                            ways.push(Way {
                                label: text!("charts.way.timeout"),
                                to: late,
                                quiet: false,
                            });
                            let (block, meet) = self.parting(&node.name, &ways, stops);
                            items.extend(block);
                            came_from = None;
                            current = meet;
                        }
                    }
                }
                "eval" => {
                    let mut ways: Vec<(String, String, bool)> = node
                        .branches
                        .iter()
                        .map(|b| (self.plain(&b.when), b.next.clone(), false))
                        .collect();
                    if let Some(o) = &node.otherwise {
                        ways.push((text!("charts.way.otherwise"), o.clone(), true));
                    }
                    // The way back a loop's block says is not a way of its own here.
                    let all = ways.len();
                    ways.retain(|w| !self.silent.contains(&(node.name.clone(), w.1.clone())));
                    let stopping: Vec<&(String, String, bool)> =
                        ways.iter().filter(|w| self.failing(Some(&w.1))).collect();
                    let going: Vec<&(String, String, bool)> =
                        ways.iter().filter(|w| !self.failing(Some(&w.1))).collect();
                    if ways.len() < all && ways.len() <= 1 {
                        current = ways.pop().map(|w| w.1);
                    } else if !stopping.is_empty() && going.len() == 1 {
                        // A check: the way on, and a break where it stops.
                        let on = going[0].clone();
                        if !on.2 {
                            items.push(self.note(on.0.clone(), NoteKind::Check));
                        }
                        let stopping: Vec<(String, String, bool)> =
                            stopping.into_iter().cloned().collect();
                        for way in stopping {
                            self.at = self.origin_of(&name);
                            if self.fails_bare(Some(&way.1)) {
                                let said = text!("charts.mark.way", node = name, way = way.0);
                                items.push(self.mark(said, &way.1));
                                continue;
                            }
                            let stop = self.stop(&way.1);
                            items.push(Item::Block {
                                kind: "break",
                                sections: vec![(way.0.clone(), stop)],
                                origin: self.origin_of(&name),
                                frame: None,
                            });
                        }
                        current = Some(on.1);
                    } else if ways.iter().all(|w| w.2) {
                        current = ways.pop().map(|w| w.1);
                    } else {
                        // One way or the other, the else in the alt's else where it has
                        // a way of its own, until they come together again.
                        let ways: Vec<Way> = ways
                            .into_iter()
                            .map(|(label, to, otherwise)| Way {
                                label,
                                to,
                                quiet: otherwise,
                            })
                            .collect();
                        let (block, meet) = self.parting(&node.name, &ways, stops);
                        items.extend(block);
                        came_from = None;
                        current = meet;
                    }
                }
                _ => current = next,
            }
        }
        // Said once this walk is done, after the walks inside it.
        self.ended = ended;
        items
    }
}

/// The most a sequence diagram is drawn in: the characters of its mermaid, and the nodes and rows
/// looked at on the way. Far above what a flow drawn by hand comes to, so that what passes them is
/// refused, named, rather than drawn on until the memory runs out.
pub const MOST_CHARS: usize = 200_000;
/// Also the most lines a class diagram follows through the value objects and enums folded into
/// what holds them (`mermaid::collect_edges`), for the same reason.
pub const MOST_STEPS: usize = 1_000_000;

/// Why a sequence diagram is not drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TooLarge {
    Chars,
    Steps,
}

impl TooLarge {
    /// In words, in the language of the moment.
    pub fn said(self) -> String {
        match self {
            TooLarge::Chars => text!("charts.tooLarge.chars", count = MOST_CHARS),
            TooLarge::Steps => text!("charts.tooLarge.steps", count = MOST_STEPS),
        }
    }
}

/// In place of a diagram too large to draw: the flow alone, saying why.
fn too_large(card: &Card, project: &Project, why: TooLarge) -> Vec<String> {
    let place = project.exported_section(&project.section_of(card));
    Sequence {
        participants: vec![Drawn {
            label: text!("charts.participant.card", place, name = card.name),
            name: card.name.clone(),
            place: Some(place),
            role: "own",
            card: Some(card.id.clone()),
            section: None,
            figure: None,
            group: None,
            group_name: None,
            open: false,
            origin: None,
        }],
        items: vec![Item::Note {
            over: 0,
            text: text!("charts.note.tooLarge", why = why.said()),
            kind: NoteKind::Plain,
            origin: None,
            description: None,
        }],
        me: Some(0),
    }
    .mermaid()
}

/// The diagram of `card` as mermaid's lines, or why it is too large to draw.
pub fn sequence_lines(
    card: &Card,
    project: &Project,
    exported: bool,
) -> Result<Vec<String>, TooLarge> {
    sequence_lines_seen(card, project, exported, &[])
}

/// `sequence_lines` with the actors drawn as lifelines where they hold a partner (`lifelines`, by id).
pub fn sequence_lines_seen(
    card: &Card,
    project: &Project,
    exported: bool,
    chosen: &[String],
) -> Result<Vec<String>, TooLarge> {
    checked_lines(sequence_seen(card, project, exported, chosen, &[])?)
}

/// A sequence's mermaid, or why it is too large: more characters than `MOST_CHARS`.
fn checked_lines(sequence: Sequence) -> Result<Vec<String>, TooLarge> {
    let lines = sequence.mermaid();
    let chars: usize = lines.iter().map(|l| l.chars().count() + 1).sum();
    if chars > MOST_CHARS {
        return Err(TooLarge::Chars);
    }
    Ok(lines)
}

/// The diagram of `card`, seen from its actor, `chosen` drawn as lifelines where they hold a partner and
/// the calls on one lifeline that `opened` names (by their keys) opened.
pub fn sequence_seen(
    card: &Card,
    project: &Project,
    exported: bool,
    chosen: &[String],
    opened: &[String],
) -> Result<Sequence, TooLarge> {
    let actors = super::actors::Actors::new(project);
    let seen = actors.of_card(card);
    let mut b = Builder::new(card, project, exported, seen, chosen.to_vec());
    b.opened = opened.iter().cloned().collect();
    b.expanding.push(card.id.clone());
    b.open(card);
    let mut items = flow_items(&mut b);
    // An end nobody waits for: a mark at the end of the flow's lifeline.
    if let Some(end) = b.ended.clone()
        && b.unanswered(&end)
        && let Some(node) = b.by_name.get(end.as_str()).copied()
    {
        b.at = b.origin_of(&end);
        items.push(b.note(
            text!("charts.note.ended", end = Builder::ending(node)),
            NoteKind::End,
        ));
    }
    if b.steps.get() > MOST_STEPS {
        return Err(TooLarge::Steps);
    }
    if items.is_empty() {
        items.push(b.note(text!("charts.note.noRows"), NoteKind::Plain));
    }
    let me = b.me;
    Ok(finished(&b, items, Some(me)))
}

/// What a flow's chart draws below its participants: what starts it, then each node from its begin
/// on.
fn flow_items(b: &mut Builder) -> Vec<Item> {
    flow_walk(b, true)
}

/// Each node of the flow open from its begin on — after what starts it, when `start`.
fn flow_walk(b: &mut Builder, start: bool) -> Vec<Item> {
    let card = b.card;
    let mut items = Vec::new();
    if card.kind != "flow" {
        return items;
    }
    for node in &card.nodes {
        b.by_name.entry(node.name.as_str()).or_insert(node);
    }
    if let Some(begin) = card.nodes.iter().find(|n| n.kind == "begin") {
        b.ways_back(&begin.name);
        b.at = b.origin_of(&begin.name);
        if start && let Some(trigger) = b.trigger_of(begin) {
            items.extend(b.starting(&trigger));
        }
        // From the begin itself, so ways that part right there are drawn too.
        items.extend(b.walk(Some(begin.name.clone()), &HashSet::new()));
    }
    items
}

/// The chart as drawn: the participants in their order, each item pointing at them there.
fn finished(b: &Builder, items: Vec<Item>, me: Option<usize>) -> Sequence {
    let (participants, order) = b.finish();
    fn moved(items: Vec<Item>, order: &[usize]) -> Vec<Item> {
        items
            .into_iter()
            .map(|item| match item {
                Item::Message {
                    from,
                    to,
                    op,
                    text,
                    flow,
                    reply,
                    origin,
                    description,
                } => Item::Message {
                    from: order[from],
                    to: order[to],
                    op,
                    text,
                    flow,
                    reply,
                    origin,
                    description,
                },
                Item::Note {
                    over,
                    text,
                    kind,
                    origin,
                    description,
                } => Item::Note {
                    over: order[over],
                    text,
                    kind,
                    origin,
                    description,
                },
                Item::Step {
                    over,
                    node,
                    label,
                    items,
                    origin,
                    called,
                } => Item::Step {
                    over: order[over],
                    node,
                    label,
                    items: moved(items, order),
                    origin,
                    called,
                },
                Item::Block {
                    kind,
                    sections,
                    origin,
                    frame,
                } => Item::Block {
                    kind,
                    sections: sections
                        .into_iter()
                        .map(|(label, inner)| (label, moved(inner, order)))
                        .collect(),
                    origin,
                    frame,
                },
            })
            .collect()
    }
    Sequence {
        participants,
        items: moved(items, &order),
        me: me.map(|m| order[m]),
    }
}

/// Where a shape is kept, as the stores say it.
pub(crate) enum Kept<'a> {
    /// In one store.
    In(&'a Card),
    /// In no store, nor in what has it inside.
    Nowhere,
    /// Held by two stores or more, or — held by none — what has it inside comes to two or more:
    /// those, in the tree's order. Which one is not written, and none is picked.
    Undecided(Vec<&'a Card>),
}

/// Stores in the tree's order: by their sections' order, then the cards' order.
fn in_tree_order<'a>(project: &Project<'a>, mut stores: Vec<&'a Card>) -> Vec<&'a Card> {
    let sections: HashMap<&str, usize> = project
        .detail
        .sections
        .iter()
        .enumerate()
        .map(|(i, s)| (s.id.as_str(), i))
        .collect();
    let cards: HashMap<&str, usize> = project
        .detail
        .schemas
        .iter()
        .enumerate()
        .map(|(i, c)| (c.id.as_str(), i))
        .collect();
    stores.sort_by_key(|store| {
        (
            sections
                .get(project.section_of(store).as_str())
                .copied()
                .unwrap_or(usize::MAX),
            cards.get(store.id.as_str()).copied().unwrap_or(usize::MAX),
        )
    });
    stores
}

/// Where a path is kept: the store its record is in, the store itself, or — held by none — the one
/// that holds what has it inside (`embed`), when that comes to one store. Two stores or more, either
/// way, are said as such and none is picked. `tick` counts each card looked at against what a chart
/// may look at.
fn store_of<'a>(project: &Project<'a>, head: &'a Card, tick: &dyn Fn() -> bool) -> Kept<'a> {
    if head.kind == "store" {
        return Kept::In(head);
    }
    let holding = |card: &Card| -> Vec<&'a Card> {
        project
            .detail
            .schemas
            .iter()
            .map(|c| &**c)
            .filter(|store| {
                store.kind == "store"
                    && store.holds.iter().any(|held| {
                        project
                            .find_seeing(held, &project.section_of(store))
                            .map(|c| c.id.as_str())
                            == Some(card.id.as_str())
                    })
            })
            .collect()
    };
    match holding(head)[..] {
        [] => {}
        [one] => return Kept::In(one),
        ref several => return Kept::Undecided(in_tree_order(project, several.to_vec())),
    }
    let mut found: Vec<&'a Card> = Vec::new();
    let mut seen: HashSet<String> = HashSet::from([head.id.clone()]);
    let mut queue: Vec<String> = vec![head.id.clone()];
    while let Some(inner) = queue.pop() {
        if !tick() {
            break;
        }
        for outer in project.detail.schemas.iter() {
            let embeds = outer
                .connections
                .iter()
                .any(|c| c.line == "embed" && c.to.as_deref() == Some(inner.as_str()));
            if !embeds || !seen.insert(outer.id.clone()) {
                continue;
            }
            for store in holding(outer) {
                if !found.iter().any(|f| f.id == store.id) {
                    found.push(store);
                }
            }
            queue.push(outer.id.clone());
        }
    }
    match found[..] {
        [] => Kept::Nowhere,
        [one] => Kept::In(one),
        _ => Kept::Undecided(in_tree_order(project, found)),
    }
}

// MARK: - What the charts draw as gaps

/// A place a flow leaves out whom it deals with (`missingPartners`): a begin that receives and names no
/// one — with the flows that run it or send to it now, a hint for whoever writes it — or, in a flow
/// whose begin receives nothing, a receive row or an end's return that names no one.
pub(crate) struct PartnerGap {
    pub at: String,
    pub callers: Vec<String>,
    /// What a row or an end does (`receive`, `return`); none for a begin.
    pub op: Option<String>,
}

/// A row that reads or writes shapes no one store keeps (`missingStores`): their paths, the stores
/// each of those that reaches two or more reaches (by its path, in the tree's order), and the row's
/// word and data as its arrow says them.
pub(crate) struct StoreGap {
    pub at: String,
    pub shapes: Vec<String>,
    pub candidates: Vec<(String, Vec<String>)>,
    pub op: String,
    pub data: String,
}

/// The partners every flow leaves out, in the tree's order.
pub(crate) fn partner_gaps(project: &Project) -> Vec<PartnerGap> {
    let mut out = Vec::new();
    for flow in project.detail.schemas.iter().filter(|c| c.kind == "flow") {
        let Some(begin) = flow.nodes.iter().find(|n| n.kind == "begin") else {
            continue;
        };
        let name = project.full(flow);
        if begin.trigger.as_deref() == Some("receive") {
            if names_no_partner(begin) {
                out.push(PartnerGap {
                    at: crate::pathtext::member(&name, &begin.name),
                    callers: callers(flow, project)
                        .iter()
                        .map(|c| project.full(c))
                        .collect(),
                    op: None,
                });
            }
            continue;
        }
        let open =
            |peers: &[String], program: &Option<String>| peers.is_empty() && program.is_none();
        for node in &flow.nodes {
            let here = crate::pathtext::member(&name, &node.name);
            if node.kind == "process" {
                for (index, step) in node.steps.iter().enumerate() {
                    if step.op == "receive" && open(&step.peers, &step.program) {
                        out.push(PartnerGap {
                            at: format!("{here}.steps[{index}]"),
                            callers: Vec::new(),
                            op: Some(step.op.clone()),
                        });
                    }
                }
            }
            if node.kind == "end"
                && let Some(reply) = &node.reply
                && open(&reply.peers, &reply.program)
            {
                out.push(PartnerGap {
                    at: here,
                    callers: Vec::new(),
                    op: Some("return".into()),
                });
            }
        }
    }
    out
}

/// The rows of every flow that read or write a shape no store keeps, in the tree's order. A path
/// that does not resolve is a broken path, said there alone.
pub(crate) fn store_gaps(project: &Project) -> Vec<StoreGap> {
    let mut out = Vec::new();
    for flow in project.detail.schemas.iter().filter(|c| c.kind == "flow") {
        let name = project.full(flow);
        for node in flow.nodes.iter().filter(|n| n.kind == "process") {
            for (index, step) in node.steps.iter().enumerate() {
                if step.op != "read" && step.op != "write" {
                    continue;
                }
                let mut shapes: Vec<String> = Vec::new();
                let mut candidates: Vec<(String, Vec<String>)> = Vec::new();
                for path in &step.paths {
                    let Some(head) = identify_head(project, path, flow) else {
                        continue;
                    };
                    let shape = project.full(head);
                    if shapes.contains(&shape) {
                        continue;
                    }
                    match store_of(project, head, &|| true) {
                        Kept::In(_) => continue,
                        Kept::Nowhere => {}
                        Kept::Undecided(stores) => candidates.push((
                            shape.clone(),
                            stores.iter().map(|s| project.full(s)).collect(),
                        )),
                    }
                    shapes.push(shape);
                }
                if shapes.is_empty() {
                    continue;
                }
                out.push(StoreGap {
                    at: format!(
                        "{}.steps[{index}]",
                        crate::pathtext::member(&name, &node.name)
                    ),
                    shapes,
                    candidates,
                    op: step.op.clone(),
                    data: step
                        .paths
                        .iter()
                        .map(|p| crate::pathtext::split_section(p).1.to_string())
                        .collect::<Vec<_>>()
                        .join(", "),
                });
            }
        }
    }
    out
}

/// The flows that run `card` in one of their nodes or send to it in a row, each once: who calls it.
fn callers<'a>(card: &Card, project: &Project<'a>) -> Vec<&'a Card> {
    let mut out: Vec<&'a Card> = Vec::new();
    for flow in project
        .detail
        .schemas
        .iter()
        .map(|c| &**c)
        .filter(|f| f.kind == "flow")
    {
        let home = project.section_of(flow);
        let names = |path: &String| {
            project
                .find_seeing(path, &home)
                .is_some_and(|c| c.id == card.id)
        };
        fn sends(steps: &[Step], names: &dyn Fn(&String) -> bool) -> bool {
            steps.iter().any(|s| {
                (s.op == "send" && s.peers.iter().any(names))
                    || sends(&s.steps, names)
                    || sends(&s.cases, names)
            })
        }
        let calls = flow.nodes.iter().any(|n| {
            n.target.iter().chain(n.targets.iter()).any(names)
                || (n.kind == "process" && sends(&n.steps, &names))
        });
        if calls && !out.iter().any(|f| f.id == flow.id) {
            out.push(flow);
        }
    }
    out
}

// MARK: - An actor's chart

/// The flows an actor provides: the flows inside it started by hand, by a schedule or at startup, and
/// those not reached only from inside it — run, sent to or waited on by a flow outside it, received by
/// its begin from a flow outside it or a program, or receiving with no caller named anywhere —, in the
/// tree's order. A flow reached only from inside the actor is what one of these does, drawn inside its
/// frame (`Builder::reached`).
fn provided<'a>(actor: &str, project: &Project<'a>) -> Vec<&'a Card> {
    let actors = super::actors::Actors::new(project);
    let mut flows: Vec<(usize, usize, &'a Card)> = project
        .detail
        .schemas
        .iter()
        .enumerate()
        .map(|(i, c)| (i, &**c))
        .filter(|(_, c)| c.kind == "flow" && c.inside.is_none())
        .filter(|(_, c)| actors.contains(actor, &project.section_of(c)))
        .map(|(i, c)| (actors.order(&project.section_of(c)), i, c))
        .collect();
    flows.sort_by_key(|(order, i, _)| (*order, *i));
    let waits_on = |flow: &Card, target: &Card| {
        let home = project.section_of(flow);
        flow.nodes.iter().any(|n| {
            n.event.as_ref().is_some_and(|e| {
                project
                    .find_seeing(&e.flow, &home)
                    .is_some_and(|c| c.id == target.id)
            })
        })
    };
    flows
        .into_iter()
        .map(|(_, _, c)| c)
        .filter(|flow| {
            let started = flow.nodes.iter().any(|n| {
                n.kind == "begin"
                    && matches!(
                        n.trigger.as_deref(),
                        Some("manual" | "schedule" | "startup") | None
                    )
            });
            if started {
                return true;
            }
            let outside = |other: &Card| !actors.contains(actor, &project.section_of(other));
            let begin = flow.nodes.iter().find(|n| n.kind == "begin");
            let receives = begin.is_some_and(|b| b.trigger.as_deref() == Some("receive"));
            // Who reaches it: the flows that run it, send to it or wait on it, and those its begin
            // says it receives from; a program its begin names is outside the project.
            let home = project.section_of(flow);
            let mut reachers: Vec<&Card> = callers(flow, project);
            reachers.extend(
                project
                    .detail
                    .schemas
                    .iter()
                    .map(|o| &**o)
                    .filter(|o| o.kind == "flow" && waits_on(o, flow)),
            );
            if receives && let Some(begin) = begin {
                if begin
                    .program
                    .as_deref()
                    .is_some_and(|p| !p.trim().is_empty())
                {
                    return true;
                }
                reachers.extend(
                    begin
                        .peers
                        .iter()
                        .filter_map(|p| project.find_seeing(p, &home))
                        .filter(|c| c.kind == "flow"),
                );
            }
            reachers.iter().any(|o| outside(o)) || (receives && reachers.is_empty())
        })
        .collect()
}

/// An actor's chart (`specboard://SpecBoard/Docs/Design:Overview` §11「シーケンス図の生命線と帯」
/// アクターの図): a named frame for each flow it provides, each drawn as its own chart is — seen from
/// the actor, so its lifelines are whoever calls from outside, the actors inside it (or itself) and
/// the partners outside — a flow inside the actor that it reaches drawn as a bar on that flow's
/// lifeline, a flow of its own lifeline that it calls drawn inside the call (`Builder::same_lifeline`), the
/// partners' flows never opened. `chosen` and `opened` as a flow's chart takes them.
pub fn actor_sequence(
    actor: &str,
    project: &Project,
    exported: bool,
    chosen: &[String],
    opened: &[String],
) -> Result<Sequence, TooLarge> {
    let flows = provided(actor, project);
    // The section itself stands for the chart's card: it is drawn as no flow.
    static NONE: std::sync::OnceLock<Card> = std::sync::OnceLock::new();
    let blank = NONE.get_or_init(|| Card {
        kind: "actor".into(),
        ..Default::default()
    });
    let mut b = Builder::new(
        blank,
        project,
        exported,
        Some(actor.to_string()),
        chosen.to_vec(),
    );
    b.opened = opened.iter().cloned().collect();
    b.expand = Some((
        actor.to_string(),
        flows.iter().map(|f| f.id.clone()).collect(),
    ));
    let mut items = Vec::new();
    for flow in flows {
        b.expanding.push(flow.id.clone());
        b.open(flow);
        let inner = flow_items(&mut b);
        b.expanding.pop();
        items.push(Item::Block {
            kind: "flow",
            sections: vec![(flow.label.clone(), inner)],
            origin: Some(Origin {
                card: flow.id.clone(),
                node: None,
                row: None,
            }),
            frame: Some(Frame {
                flow: flow.id.clone(),
                label: flow.label.clone(),
                head: words(&[Some(flow.name.clone()), Some(flow.label.clone())]),
            }),
        });
        if b.steps.get() > MOST_STEPS {
            return Err(TooLarge::Steps);
        }
    }
    b.card = blank;
    if items.is_empty() {
        let me = b.actor_participant(actor);
        b.me = me;
        items.push(b.note(text!("charts.note.noFlows"), NoteKind::Plain));
    }
    Ok(finished(&b, items, None))
}

/// An actor's chart as mermaid's lines, headed with what it was drawn from, or why it is too large.
pub fn actor_lines(
    actor: &str,
    project: &Project,
    exported: bool,
    chosen: &[String],
) -> Result<Vec<String>, TooLarge> {
    checked_lines(actor_sequence(actor, project, exported, chosen, &[])?)
}

/// An actor's chart as MCP's `as: diagram` gives it: under the head that says what it was drawn
/// from and its rev.
pub fn render_actor(actor: &str, project: &Project, chosen: &[String]) -> Result<String, TooLarge> {
    let body = actor_lines(actor, project, false, chosen)?;
    let mut lines = vec![format!(
        "%% specboard project={} actor={} shape=sequence rev={}",
        project.detail.project.slug,
        ident::line_comment(&project.directory.full_section(actor)),
        revision(&body)
    )];
    lines.extend(body);
    Ok(lines.join("\n") + "\n")
}

// MARK: - The chart as the window reads it

/// The chart as the window's request `sequence_chart` answers it: who takes part, then each item with
/// where it comes from; or, too large to draw, why.
pub fn chart_json(drawn: Result<Sequence, TooLarge>) -> Value {
    let mut out = Map::new();
    let checked = drawn.and_then(|s| {
        let chars: usize = s.mermaid().iter().map(|l| l.chars().count() + 1).sum();
        if chars > MOST_CHARS {
            Err(TooLarge::Chars)
        } else {
            Ok(s)
        }
    });
    match checked {
        Ok(chart) => {
            out.insert(
                "participants".into(),
                chart
                    .participants
                    .iter()
                    .map(|p| {
                        let mut one = Map::new();
                        one.insert("label".into(), p.label.clone().into());
                        one.insert("name".into(), p.name.clone().into());
                        if let Some(place) = &p.place {
                            one.insert("place".into(), place.clone().into());
                        }
                        one.insert("role".into(), p.role.into());
                        if let Some(card) = &p.card {
                            one.insert("card".into(), card.clone().into());
                        }
                        if let Some(section) = &p.section {
                            one.insert("section".into(), section.clone().into());
                        }
                        if let Some(figure) = &p.figure {
                            one.insert("figure".into(), figure.clone().into());
                        }
                        if let Some(group) = &p.group {
                            one.insert("group".into(), group.clone().into());
                            one.insert("fold".into(), true.into());
                        }
                        if p.open {
                            one.insert("open".into(), true.into());
                        }
                        if let Some(origin) = &p.origin {
                            one.insert("origin".into(), origin.json());
                        }
                        Value::Object(one)
                    })
                    .collect(),
            );
            if let Some(me) = chart.me {
                out.insert("me".into(), me.into());
            }
            out.insert("items".into(), items_json(&chart.items));
        }
        Err(why) => {
            out.insert("tooLarge".into(), why.said().into());
        }
    }
    Value::Object(out)
}

fn items_json(items: &[Item]) -> Value {
    items
        .iter()
        .map(|item| {
            let mut one = Map::new();
            let (key, origin) = match item {
                Item::Message {
                    from,
                    to,
                    op,
                    text,
                    flow,
                    reply,
                    origin,
                    description,
                } => {
                    one.insert("from".into(), (*from).into());
                    one.insert("to".into(), (*to).into());
                    if let Some(op) = op {
                        one.insert("op".into(), op.clone().into());
                    }
                    one.insert("text".into(), text.clone().into());
                    if let Some(flow) = flow {
                        one.insert("flow".into(), flow.clone().into());
                    }
                    one.insert("reply".into(), (*reply).into());
                    if let Some(d) = description {
                        one.insert("description".into(), d.clone().into());
                    }
                    ("message", origin)
                }
                Item::Note {
                    over,
                    text,
                    kind,
                    origin,
                    description,
                } => {
                    one.insert("over".into(), (*over).into());
                    one.insert("text".into(), text.clone().into());
                    one.insert("kind".into(), kind.said().into());
                    if let Some(d) = description {
                        one.insert("description".into(), d.clone().into());
                    }
                    ("note", origin)
                }
                Item::Step {
                    over,
                    node,
                    label,
                    items,
                    origin,
                    called,
                } => {
                    one.insert("over".into(), (*over).into());
                    if let Some(node) = node {
                        one.insert("node".into(), node.clone().into());
                    }
                    one.insert("label".into(), label.clone().into());
                    if let Some(called) = called {
                        one.insert("flow".into(), called.flow.clone().into());
                        one.insert("op".into(), called.op.clone().into());
                        one.insert("place".into(), called.place.clone().into());
                        if let Some(key) = &called.call {
                            one.insert("call".into(), key.clone().into());
                        }
                        if called.folded {
                            one.insert("folded".into(), true.into());
                        }
                        if called.cycle {
                            one.insert("cycle".into(), true.into());
                        }
                    }
                    one.insert("items".into(), items_json(items));
                    ("step", origin)
                }
                Item::Block {
                    kind,
                    sections,
                    origin,
                    frame,
                } => {
                    one.insert("kind".into(), (*kind).into());
                    one.insert(
                        "sections".into(),
                        sections
                            .iter()
                            .map(|(label, inner)| {
                                let mut section = Map::new();
                                section.insert("label".into(), label.clone().into());
                                section.insert("items".into(), items_json(inner));
                                Value::Object(section)
                            })
                            .collect(),
                    );
                    if let Some(frame) = frame {
                        one.insert("flow".into(), frame.flow.clone().into());
                        one.insert("label".into(), frame.label.clone().into());
                    }
                    ("block", origin)
                }
            };
            if let Some(origin) = origin {
                one.insert("origin".into(), origin.json());
            }
            let mut wrapped = Map::new();
            wrapped.insert(key.into(), Value::Object(one));
            Value::Object(wrapped)
        })
        .collect()
}

// MARK: - A page of charts

/// What a flow runs, node by node: each run's targets.
fn runs<'a>(flow: &'a Card, project: &Project<'a>) -> Vec<(&'a FlowNode, Vec<&'a Card>)> {
    let home = project.section_of(flow);
    flow.nodes
        .iter()
        .filter(|n| n.kind == "run")
        .map(|n| {
            let paths: Vec<&String> = n.target.iter().chain(n.targets.iter()).collect();
            (
                n,
                paths
                    .into_iter()
                    .filter_map(|p| project.find_seeing(p, &home))
                    .collect(),
            )
        })
        .collect()
}

/// The flows that run `card` in one of their nodes.
fn runners<'a>(card: &Card, project: &Project<'a>) -> Vec<&'a Card> {
    project
        .detail
        .schemas
        .iter()
        .map(|c| &**c)
        .filter(|f| f.kind == "flow")
        .filter(|f| {
            runs(f, project)
                .iter()
                .any(|(_, targets)| targets.iter().any(|t| t.id == card.id))
        })
        .collect()
}

/// A heading's anchor as Markdown renderers make it: in lower case, a space a `-`, and what is
/// neither a letter, a digit, `-` nor `_` left out.
pub fn anchor(heading: &str) -> String {
    heading
        .to_lowercase()
        .chars()
        .filter_map(|c| match c {
            ' ' => Some('-'),
            c if c.is_alphanumeric() || c == '-' || c == '_' => Some(c),
            _ => None,
        })
        .collect()
}

/// The anchors of the cards' headings on a page of charts — the cards `paths` puts in `page`, in
/// the project's order, as the page writes them — each one of its own on the page, as renderers
/// tell two of one heading apart (`-1`, `-2` after the second and the third).
pub fn page_anchors(
    project: &Project,
    paths: &HashMap<String, String>,
    page: &str,
) -> HashMap<String, String> {
    let mut seen: HashMap<String, usize> = HashMap::new();
    let mut out = HashMap::new();
    for card in project
        .detail
        .schemas
        .iter()
        .filter(|c| paths.get(&c.id).map(String::as_str) == Some(page))
    {
        let base = anchor(&card.name);
        let count = seen.entry(base.clone()).or_insert(0);
        out.insert(
            card.id.clone(),
            if *count == 0 {
                base.clone()
            } else {
                format!("{base}-{count}")
            },
        );
        *count += 1;
    }
    out
}

/// An HTML comment that stays one, on one line.
pub fn marker(text: &str) -> String {
    let mut text = ident::line_comment(text);
    while text.contains("--") {
        text = text.replace("--", "- -");
    }
    format!("<!-- {text} -->")
}

/// The way from `from` to the file `target` is written in: to its heading on a page of
/// charts (`page_anchors`; `paths`: where each card is written).
pub fn link(
    from: &str,
    there: &str,
    target: &Card,
    project: &Project,
    paths: &HashMap<String, String>,
) -> String {
    relative(from, there)
        + &if target.kind == "flow" || target.kind == "action" {
            let anchors = page_anchors(project, paths, there);
            format!(
                "#{}",
                anchors
                    .get(&target.id)
                    .cloned()
                    .unwrap_or_else(|| anchor(&target.name))
            )
        } else {
            String::new()
        }
}

/// Inline code that survives any backticks in it.
pub fn code(text: &str) -> String {
    let longest = if text.contains('`') {
        text.split(|c: char| c != '`')
            .map(str::len)
            .max()
            .unwrap_or(0)
    } else {
        0
    };
    let fence = "`".repeat(longest + 1);
    let pad = if text.starts_with('`') || text.ends_with('`') {
        " "
    } else {
        ""
    };
    format!("{fence}{pad}{text}{pad}{fence}")
}

/// The actor a page of charts is of: the nearest one around every flow on it. None when a flow on it
/// is in no actor, or there is no flow.
fn page_actor(cards: &[&Card], project: &Project) -> Option<String> {
    let actors = super::actors::Actors::new(project);
    let mut common: Option<Vec<String>> = None;
    for card in cards.iter().filter(|c| c.kind == "flow") {
        let chain = actors.chain(&project.section_of(card));
        common = Some(match common {
            None => chain,
            Some(had) => had
                .into_iter()
                .zip(chain)
                .take_while(|(a, b)| a == b)
                .map(|(a, _)| a)
                .collect(),
        });
    }
    common?.pop()
}

/// The head of an actor's page: what it is, the flows it provides, the actors it uses and those that
/// use it, and its chart — read off the chart itself, so the page says what the chart draws.
fn actor_head(actor: &str, project: &Project, link_to: &dyn Fn(&Card) -> String) -> Vec<String> {
    let actors = super::actors::Actors::new(project);
    let figure = project
        .detail
        .sections
        .iter()
        .find(|s| s.id == actor)
        .and_then(|s| s.figure.clone());
    let mut lines = vec![
        text!(
            "charts.page.figure",
            figure = match figure.as_deref() {
                Some("person") => text!("charts.figure.person"),
                Some("outside") => text!("charts.figure.outside"),
                _ => text!("charts.figure.system"),
            }
        ),
        String::new(),
    ];
    let flows: Vec<String> = provided(actor, project).into_iter().map(link_to).collect();
    if !flows.is_empty() {
        lines.push(text!(
            "charts.page.provides",
            flows = crate::text::list(&flows)
        ));
        lines.push(String::new());
    }
    if let Ok(chart) = actor_sequence(actor, project, true, &[], &[]) {
        let inside = |at: usize| {
            chart.participants.get(at).is_some_and(|p| {
                p.section
                    .as_deref()
                    .is_some_and(|s| actors.contains(actor, s))
            })
        };
        let mut uses: Vec<String> = Vec::new();
        let mut used: Vec<String> = Vec::new();
        fn each(items: &[Item], out: &mut dyn FnMut(&Item)) {
            for item in items {
                out(item);
                match item {
                    Item::Step { items, .. } => each(items, out),
                    Item::Block { sections, .. } => {
                        for (_, inner) in sections {
                            each(inner, out);
                        }
                    }
                    _ => {}
                }
            }
        }
        each(&chart.items, &mut |item| {
            if let Item::Message {
                from,
                to,
                reply: false,
                op,
                ..
            } = item
            {
                let label = |at: usize| chart.participants[at].label.clone();
                let comes = op.as_deref() == Some("receive");
                if inside(*from) && !inside(*to) && !comes {
                    let l = label(*to);
                    if !uses.contains(&l) {
                        uses.push(l);
                    }
                } else if !inside(*from) && inside(*to) && comes {
                    let l = label(*from);
                    if !used.contains(&l) {
                        used.push(l);
                    }
                }
            }
        });
        if !uses.is_empty() {
            lines.push(text!("charts.page.uses", actors = crate::text::list(&uses)));
            lines.push(String::new());
        }
        if !used.is_empty() {
            lines.push(text!(
                "charts.page.usedBy",
                actors = crate::text::list(&used)
            ));
            lines.push(String::new());
        }
        if let Ok(body) = checked_lines(chart) {
            let mut chart_lines = vec![format!(
                "%% specboard rev={}",
                revision(&actor_lines(actor, project, false, &[]).unwrap_or_default())
            )];
            chart_lines.extend(body);
            lines.push("```mermaid".into());
            lines.push(chart_lines.join("\n") + "\n```");
            lines.push(String::new());
        }
    }
    lines
}

/// Flows on one page, each its diagram and the way to the charts it is made of or part of.
pub fn chart_page(
    cards: &[&Card],
    project: &Project,
    at: &str,
    paths: &HashMap<String, String>,
    mark: &str,
) -> String {
    let here: HashSet<&str> = cards.iter().map(|c| c.id.as_str()).collect();
    let anchors = page_anchors(project, paths, at);
    let link_to = |target: &Card| -> String {
        let name = code(&target.name);
        if here.contains(target.id.as_str()) {
            return format!(
                "[{name}](#{})",
                anchors
                    .get(&target.id)
                    .cloned()
                    .unwrap_or_else(|| anchor(&target.name))
            );
        }
        match paths.get(&target.id) {
            Some(there) => format!("[{name}]({})", link(at, there, target, project, paths)),
            None => name,
        }
    };
    let mut lines = vec![marker(mark), String::new()];
    if let Some(actor) = page_actor(cards, project) {
        lines.extend(actor_head(&actor, project, &link_to));
    }
    for card in cards {
        lines.push(format!("## {}", card.name));
        lines.push(String::new());
        if !card.label.is_empty() {
            lines.push(card.label.clone());
            lines.push(String::new());
        }
        if card.kind == "note" {
            let body = super::documents::markdown(card, project, at, paths);
            let body = body.trim();
            if !body.is_empty() {
                lines.push(body.to_string());
                lines.push(String::new());
            }
            continue;
        }
        if card.kind == "flow" {
            // Its picture and every node, as `card_read`'s `as: markdown` gives them.
            lines.push(super::flowdoc::on_page(card, project, at, paths));
            lines.push(String::new());
        } else {
            if !card.description.is_empty() {
                lines.push(card.description.clone());
                lines.push(String::new());
            }
            lines.push("```mermaid".into());
            lines.push(render(card, project, None) + "```");
            lines.push(String::new());
        }
        if card.kind == "flow" && shape(card, None) == "flowchart" {
            let runs: Vec<_> = runs(card, project)
                .into_iter()
                .filter(|(_, t)| !t.is_empty())
                .collect();
            if !runs.is_empty() {
                lines.push(format!("### {}", text!("charts.page.runs")));
                lines.push(String::new());
                for (node, targets) in runs {
                    lines.push(format!(
                        "- {}: {}",
                        node.name,
                        crate::text::list_comma(
                            &targets.iter().map(|t| link_to(t)).collect::<Vec<_>>()
                        )
                    ));
                }
                lines.push(String::new());
            }
        } else {
            let flows = runners(card, project);
            if !flows.is_empty() {
                lines.push(format!(
                    "### {}",
                    if card.kind == "flow" {
                        text!("charts.page.runnersOfFlow")
                    } else {
                        text!("charts.page.runnersOfAction")
                    }
                ));
                lines.push(String::new());
                for flow in flows {
                    lines.push(format!("- {}", link_to(flow)));
                }
                lines.push(String::new());
            }
        }
    }
    lines.join("\n")
}
