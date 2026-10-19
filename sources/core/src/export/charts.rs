//! A flow's diagram: its flowchart — each node in the shape
//! of what it is, each way labelled — or its sequence diagram — who takes part, then
//! what passes between them in order. The first line says what it was drawn from, with
//! a rev that changes exactly when the picture would.

use super::ident;
use super::paths::identify_head;
use super::{Project, relative};
use crate::model::{Card, Exit, FlowNode, Step};
use crate::notetext::{self, exported_path, plain_path};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet, VecDeque};

/// The other end of a receive or a return that names none.
pub fn caller() -> String {
    text!("charts.caller")
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
        sequence(card, project, true).mermaid()
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

/// The picture as the app draws it, in `shape` (`flowchart` or `sequence`).
fn drawn(card: &Card, project: &Project, shape: &str) -> Vec<String> {
    if shape == "sequence" {
        sequence(card, project, false).mermaid()
    } else {
        flow_body(card, false)
    }
}

/// The rev of the picture in `shape`: it changes exactly when the picture would
/// (`FlowDiagram.revision`, `ActionDiagram.revision`).
pub fn revision_in(card: &Card, project: &Project, shape: &str) -> String {
    revision(&drawn(card, project, shape))
}

/// As the app draws it in `shape`, the head saying what it was drawn from: the project, the
/// flow, the shape and the rev (`FlowDiagram.render`, `ActionDiagram.render`).
pub fn render_drawn(card: &Card, project: &Project, shape: &str) -> String {
    let body = drawn(card, project, shape);
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
                let from =
                    partners(&node.peers, node.program.as_deref(), path).unwrap_or_else(caller);
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
                        other.unwrap_or_else(caller),
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
                    let to = partners(&r.peers, r.program.as_deref(), path).unwrap_or_else(caller);
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum NoteKind {
    Plain,
    Key,
    Check,
    End,
    Heading,
}

enum Item {
    Message {
        from: usize,
        to: usize,
        op: Option<String>,
        text: String,
        reply: bool,
    },
    Note {
        over: usize,
        text: String,
        kind: NoteKind,
    },
    Block {
        kind: &'static str,
        sections: Vec<(String, Vec<Item>)>,
    },
}

pub struct Sequence {
    participants: Vec<String>,
    items: Vec<Item>,
}

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
        let mut lines: Vec<String> = self
            .participants
            .iter()
            .enumerate()
            .map(|(i, p)| format!("    participant P{} as {}", i + 1, safe(p)))
            .collect();
        fn emit(items: &[Item], indent: &str, lines: &mut Vec<String>) {
            for item in items {
                match item {
                    Item::Message {
                        from,
                        to,
                        op,
                        text,
                        reply,
                    } => lines.push(format!(
                        "{indent}P{}{}P{}: {}",
                        from + 1,
                        if *reply { "-->>" } else { "->>" },
                        to + 1,
                        safe(&words(&[op.clone(), Some(text.clone())]))
                    )),
                    Item::Note { over, text, kind } => lines.push(format!(
                        "{indent}Note over P{}: {}",
                        over + 1,
                        safe(&if *kind == NoteKind::Check {
                            text!("charts.note.check", text)
                        } else {
                            text.clone()
                        })
                    )),
                    Item::Block { kind, sections } => {
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
    description: Option<String>,
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

/// One who takes part: the name it is drawn under, and the name with its section for when two
/// who take part have the one name.
struct Participant {
    label: String,
    with_section: Option<String>,
}

struct Builder<'p, 'a> {
    card: &'a Card,
    project: &'p Project<'a>,
    exported: bool,
    participants: Vec<Participant>,
    ids: HashMap<String, usize>,
    me: usize,
    by_name: HashMap<&'a str, &'a FlowNode>,
    /// The end the last walk stopped at on its own way, if it did (`parting`).
    ended: Option<String>,
}

fn short(paths: &[String]) -> String {
    paths
        .iter()
        .map(|p| crate::pathtext::split_section(p).1.to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

fn aside(comment: &Option<String>) -> Option<String> {
    comment.as_ref().map(|c| text!("charts.aside", comment = c))
}

impl<'a> Builder<'_, 'a> {
    fn participant(&mut self, label: String) -> usize {
        self.participant_of(label.clone(), label, None)
    }

    /// The one told apart by `key` (not by its name: cards of two sections may have one), drawn
    /// as `label`.
    fn participant_of(
        &mut self,
        key: String,
        label: String,
        with_section: Option<String>,
    ) -> usize {
        if let Some(&known) = self.ids.get(&key) {
            return known;
        }
        self.participants.push(Participant {
            label,
            with_section,
        });
        self.ids.insert(key, self.participants.len() - 1);
        self.participants.len() - 1
    }

    /// The names the participants are drawn under: a name two of them share carries the section.
    fn names(&self) -> Vec<String> {
        let mut shared: HashMap<&str, usize> = HashMap::new();
        for p in &self.participants {
            *shared.entry(p.label.as_str()).or_default() += 1;
        }
        self.participants
            .iter()
            .map(|p| match (&p.with_section, shared[p.label.as_str()]) {
                (Some(longer), n) if n > 1 => longer.clone(),
                _ => p.label.clone(),
            })
            .collect()
    }

    /// A flow it names, as the participant it is drawn as.
    fn peer(&mut self, peer: &str) -> usize {
        if let Some(target) = self
            .project
            .find_seeing(peer, &self.project.section_of(self.card))
        {
            let place = self
                .project
                .exported_section(&self.project.section_of(target));
            return self.participant(text!("charts.participant.card", place, name = target.name));
        }
        self.participant(peer.to_string())
    }

    /// Each one it deals with, how a way to it is named and the participant it is drawn as: the
    /// flows it names, then the program; whoever called it when it names no one.
    fn others(&mut self, peers: &[String], program: &Option<String>) -> Vec<(String, usize)> {
        let mut out: Vec<(String, usize)> = peers
            .iter()
            .map(|p| (short(std::slice::from_ref(p)), self.peer(p)))
            .collect();
        if let Some(program) = program {
            out.push((program.clone(), self.participant(program.clone())));
        }
        if out.is_empty() {
            out.push((caller(), self.participant(caller())));
        }
        out
    }

    fn others_of(&mut self, step: &Step) -> Vec<(String, usize)> {
        self.others(&step.peers, &step.program)
    }

    /// The first one a row deals with: a call has the one.
    fn other_step(&mut self, step: &Step) -> usize {
        self.others_of(step)[0].1
    }

    /// What passes with each of `others`: as it is with one, a way of its own with each of
    /// several (from the partner, to the partner), as only one of them is dealt with at a time.
    fn each_of(
        &mut self,
        others: Vec<(String, usize)>,
        direction: Direction,
        mut items: impl FnMut(&mut Self, usize) -> Vec<Item>,
    ) -> Vec<Item> {
        if others.len() == 1 {
            return items(self, others[0].1);
        }
        let sections = others
            .into_iter()
            .map(|(said, at)| {
                let label = match direction {
                    Direction::From => text!("charts.way.from", said),
                    Direction::To => text!("charts.way.to", said),
                };
                (label, items(self, at))
            })
            .collect();
        vec![Item::Block {
            kind: "alt",
            sections,
        }]
    }

    /// Where a path is kept: the store holding its record, or the record.
    fn place(&mut self, path: &str) -> usize {
        let Some(head) = identify_head(self.project, path, self.card) else {
            let rest = crate::pathtext::split_section(path).1;
            let name = crate::pathtext::unquote(crate::pathtext::dots(rest)[0]).into_owned();
            return self.participant(name);
        };
        let kept_in = |card: &Card| {
            let place = self
                .project
                .exported_section(&self.project.section_of(card));
            crate::pathtext::card_path(&place, &card.name)
        };
        if head.kind == "store" {
            let longer = text!("charts.participant.store", name = kept_in(head));
            return self.participant_of(
                head.id.clone(),
                text!("charts.participant.store", name = head.name),
                Some(longer),
            );
        }
        let store = self.project.detail.schemas.iter().find(|store| {
            store.kind == "store"
                && store.holds.iter().any(|held| {
                    self.project
                        .find_seeing(held, &self.project.section_of(store))
                        .map(|c| c.id.as_str())
                        == Some(head.id.as_str())
                })
        });
        match store {
            Some(s) => {
                let longer = text!("charts.participant.store", name = kept_in(s));
                self.participant_of(
                    s.id.clone(),
                    text!("charts.participant.store", name = s.name),
                    Some(longer),
                )
            }
            None => self.participant_of(head.id.clone(), head.name.clone(), Some(kept_in(head))),
        }
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

    fn message(&self, from: usize, to: usize, op: Option<&str>, text: String, reply: bool) -> Item {
        Item::Message {
            from,
            to,
            op: op.map(str::to_string),
            // What a row says of itself may name cards in `[[ ]]`.
            text: self.plain(&text),
            reply,
        }
    }

    fn note(&self, text: String, kind: NoteKind) -> Item {
        Item::Note {
            over: self.me,
            text: self.plain(&text),
            kind,
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
            description: None,
        };
        let mut trigger = match begin.trigger.as_deref() {
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
                    t.description =
                        Some(text!("charts.event.of", flow = e.flow, outcome = e.outcome));
                }
                Some(t)
            }
            None => None,
        };
        if let (Some(t), Some(said)) = (trigger.as_mut(), &begin.description) {
            t.description = Some(said.clone());
        }
        trigger
    }

    fn starting(&mut self, trigger: &Trigger) -> Vec<Item> {
        let mut items = Vec::new();
        match trigger.on.as_str() {
            "receive" => {
                let others = self.others(&trigger.peers, &trigger.program);
                items.extend(self.each_of(others, Direction::From, |me, from| {
                    if trigger.initiator.as_deref() == Some("self") {
                        let fetch = text!("charts.op.fetch");
                        vec![
                            me.message(
                                me.me,
                                from,
                                Some(&fetch),
                                words(std::slice::from_ref(&trigger.via)),
                                false,
                            ),
                            me.message(
                                from,
                                me.me,
                                None,
                                words(&[
                                    Some(short(&trigger.carries)),
                                    aside(&trigger.description),
                                ]),
                                true,
                            ),
                        ]
                    } else {
                        vec![me.message(
                            from,
                            me.me,
                            Some("receive"),
                            words(&[
                                Some(short(&trigger.carries)),
                                trigger.via.clone(),
                                aside(&trigger.description),
                            ]),
                            false,
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
                    words(&[
                        Some("schedule".into()),
                        trigger.every.clone(),
                        aside(&trigger.description),
                    ]),
                    NoteKind::Plain,
                ));
            }
            _ => {
                items.push(self.note(
                    words(&[Some(trigger.on.clone()), aside(&trigger.description)]),
                    NoteKind::Plain,
                ));
            }
        }
        items
    }

    fn rows(&mut self, steps: &[Step]) -> Vec<Item> {
        let mut items = Vec::new();
        for step in steps {
            match step.op.as_str() {
                "receive" => {
                    let others = self.others_of(step);
                    items.extend(self.each_of(others, Direction::From, |me, from| {
                        vec![me.message(
                            from,
                            me.me,
                            Some("receive"),
                            words(&[
                                Some(short(&step.carries)),
                                step.via.clone(),
                                aside(&step.description),
                            ]),
                            false,
                        )]
                    }));
                }
                "return" => {
                    let others = self.others_of(step);
                    items.extend(self.each_of(others, Direction::To, |me, to| {
                        vec![me.message(
                            me.me,
                            to,
                            Some("return"),
                            words(&[Some(short(&step.carries)), aside(&step.description)]),
                            true,
                        )]
                    }));
                }
                // A send goes to each it names, one after another.
                "send" => {
                    for (_, to) in self.others_of(step) {
                        items.push(self.message(
                            self.me,
                            to,
                            Some("send"),
                            words(&[
                                Some(short(&step.carries)),
                                step.via.clone(),
                                aside(&step.description),
                            ]),
                            false,
                        ));
                    }
                }
                "call" => {
                    let to = self.other_step(step);
                    items.push(self.message(
                        self.me,
                        to,
                        Some("call"),
                        words(&[
                            Some(short(&step.carries)),
                            step.via.clone(),
                            aside(&step.description),
                        ]),
                        false,
                    ));
                    if !step.returns.is_empty() {
                        items.push(self.message(to, self.me, None, short(&step.returns), true));
                    }
                }
                "read" | "write" => {
                    for path in &step.paths {
                        let at = self.place(path);
                        items.push(self.message(
                            self.me,
                            at,
                            Some(&step.op),
                            words(&[
                                Some(short(std::slice::from_ref(path))),
                                aside(&step.description),
                            ]),
                            false,
                        ));
                    }
                }
                "sign" | "verify" => {
                    items.push(self.note(
                        words(&[
                            Some(step.op.clone()),
                            step.key.as_ref().map(|k| short(std::slice::from_ref(k))),
                            Some(short(&step.paths)),
                            aside(&step.description),
                        ]),
                        NoteKind::Key,
                    ));
                }
                "note" => items.push(self.note(
                    words(std::slice::from_ref(&step.description)),
                    NoteKind::Plain,
                )),
                // What the flow works out by itself, over itself: what the row says it does.
                "compute" => items.push(
                    self.note(
                        step.description
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
                        });
                    }
                }
                "loop" => {
                    let label = self.plain(step.over.as_deref().unwrap_or(""));
                    let inner = self.rows(&step.steps);
                    items.push(Item::Block {
                        kind: "loop",
                        sections: vec![(label, inner)],
                    });
                }
                _ => {}
            }
        }
        items
    }

    fn reaches(&self, from: &str, target: &str, avoiding: Option<&str>) -> bool {
        let mut seen: HashSet<String> = HashSet::new();
        let mut queue = vec![from.to_string()];
        while let Some(name) = queue.pop() {
            if name == target {
                return true;
            }
            if Some(name.as_str()) == avoiding || !seen.insert(name.clone()) {
                continue;
            }
            let Some(node) = self.by_name.get(name.as_str()) else {
                continue;
            };
            queue.extend(node.exits().into_iter().map(|e| e.0));
        }
        false
    }

    /// Where ways that part meet: the first join every one of them reaches.
    fn meeting(&self, ways: &[String]) -> Option<String> {
        let joins = |start: &str| -> Vec<String> {
            let mut seen: HashSet<String> = HashSet::new();
            let mut queue: VecDeque<String> = VecDeque::from([start.to_string()]);
            let mut found = Vec::new();
            while let Some(name) = queue.pop_front() {
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
                queue.extend(node.exits().into_iter().map(|e| e.0));
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

    /// Each node `start` reaches — not going back through `from`, nor on past `stops` — and how
    /// far along it is, in the order reached.
    fn reach(&self, start: &str, from: &str, stops: &HashSet<String>) -> Vec<(String, usize)> {
        let mut seen: HashSet<String> = HashSet::new();
        let mut queue: VecDeque<(String, usize)> = VecDeque::from([(start.to_string(), 0)]);
        let mut found = Vec::new();
        while let Some((name, far)) = queue.pop_front() {
            if name == from || !seen.insert(name.clone()) {
                continue;
            }
            found.push((name.clone(), far));
            if stops.contains(&name) {
                continue;
            }
            if let Some(node) = self.by_name.get(name.as_str()) {
                queue.extend(node.exits().into_iter().map(|e| (e.0, far + 1)));
            }
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
        on_way: &HashSet<String>,
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
                let mut inner = self.walk(Some(way.to.clone()), &until, on_way);
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
                sections.push((
                    way.label.clone(),
                    vec![self.note(text!("charts.note.nothing"), NoteKind::Plain)],
                ));
            } else {
                sections.push((way.label.clone(), inner));
            }
        }
        let block = (!sections.is_empty()).then_some(Item::Block {
            kind: "alt",
            sections,
        });
        (block, meet)
    }

    /// Whether an end says nothing of how it ends by what it answers: it answers nothing, or it
    /// went wrong.
    fn unanswered(&self, name: &str) -> bool {
        self.by_name
            .get(name)
            .is_some_and(|n| n.reply.is_none() || self.failing(Some(name)))
    }

    /// An end's note: what it is called, or how it ends.
    fn stop_note(&self, name: &str) -> Option<Item> {
        let node = self.by_name.get(name)?;
        Some(
            self.note(
                node.label
                    .clone()
                    .or_else(|| node.outcome.clone())
                    .unwrap_or_else(|| "end".into()),
                NoteKind::End,
            ),
        )
    }

    /// A way that goes straight to the end `name`: what it answers — and how it ends, where
    /// that is not said by it (`unanswered`).
    fn stopping(&mut self, name: &str) -> Vec<Item> {
        if self.unanswered(name) {
            return self.stop(name);
        }
        let Some(node) = self.by_name.get(name).copied() else {
            return Vec::new();
        };
        Self::return_row(node)
            .map(|r| self.rows(&[r]))
            .unwrap_or_default()
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

    fn return_row(node: &FlowNode) -> Option<Step> {
        if node.kind != "end" {
            return None;
        }
        let reply = node.reply.as_ref()?;
        Some(Step {
            op: "return".into(),
            carries: reply.carries.clone(),
            peers: reply.peers.clone(),
            program: reply.program.clone(),
            sign_with: reply.sign_with.clone(),
            description: node.description.clone(),
            ..Default::default()
        })
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

    fn stop(&mut self, name: &str) -> Vec<Item> {
        let Some(node) = self.by_name.get(name).copied() else {
            return Vec::new();
        };
        let mut items = Self::return_row(node)
            .map(|r| self.rows(&[r]))
            .unwrap_or_default();
        items.extend(self.stop_note(name));
        items
    }

    fn process(&mut self, node: &FlowNode) -> Vec<Item> {
        let mut items = Vec::new();
        if let Some(heading) = node.label.as_ref().filter(|h| !h.is_empty()) {
            items.push(self.note(heading.clone(), NoteKind::Heading));
        }
        items.extend(self.rows(&node.steps));
        items
    }

    fn run(&mut self, node: &FlowNode) -> Vec<Item> {
        let Some(call) = Self::call_row(node) else {
            return Vec::new();
        };
        let Some(heading) = node.label.as_ref().filter(|h| !h.is_empty()) else {
            return self.rows(&[call]);
        };
        let to = self.other_step(&call);
        let mut items = vec![self.message(self.me, to, None, heading.clone(), false)];
        if !call.returns.is_empty() {
            items.push(self.message(to, self.me, None, short(&call.returns), true));
        }
        items
    }

    fn walk(
        &mut self,
        start: Option<String>,
        stops: &HashSet<String>,
        on_way_in: &HashSet<String>,
    ) -> Vec<Item> {
        let mut items = Vec::new();
        let mut current = start;
        let mut on_way = on_way_in.clone();
        let mut ended = None;
        while let Some(name) = current.clone() {
            if stops.contains(&name) {
                break;
            }
            let Some(node) = self.by_name.get(name.as_str()).copied() else {
                break;
            };
            if !on_way.insert(name.clone()) {
                items.push(self.note(text!("charts.note.back", name), NoteKind::Plain));
                break;
            }
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
                    .map(|way| (String::new(), self.walk(Some(way.clone()), &until, &on_way)))
                    .collect();
                items.push(Item::Block {
                    kind: "par",
                    sections,
                });
                current = meet
                    .as_deref()
                    .and_then(|m| self.by_name.get(m))
                    .and_then(|n| n.nexts.first().cloned());
                if let Some(m) = meet {
                    on_way.insert(m);
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
                            let stop = self.stop(next.as_deref().unwrap_or_default());
                            items.push(Item::Block {
                                kind: "break",
                                sections: vec![(text!("charts.way.success"), stop)],
                            });
                            current = Some(failed);
                        } else if self.failing(Some(&failed)) {
                            let stop = self.stop(&failed);
                            items.push(Item::Block {
                                kind: "break",
                                sections: vec![(text!("charts.way.failure"), stop)],
                            });
                        } else {
                            let mut until = stops.clone();
                            if let Some(n) = &next {
                                until.insert(n.clone());
                            }
                            let inner = self.walk(Some(failed), &until, &on_way);
                            items.push(Item::Block {
                                kind: "alt",
                                sections: vec![(text!("charts.way.failure"), inner)],
                            });
                        }
                    }
                }
                "end" => {
                    if let Some(answer) = Self::return_row(node) {
                        items.extend(self.rows(&[answer]));
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
                    // Where it goes when the time runs out: a break where it stops at
                    // once in failure, else its own way beside the one on.
                    if let Some(late) = node.on_timeout.clone().filter(|t| Some(t) != next.as_ref())
                    {
                        if self.failing(Some(&late)) {
                            let stop = self.stop(&late);
                            items.push(Item::Block {
                                kind: "break",
                                sections: vec![(text!("charts.way.timeout"), stop)],
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
                            let (block, meet) = self.parting(&node.name, &ways, stops, &on_way);
                            items.extend(block);
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
                    let stopping: Vec<&(String, String, bool)> =
                        ways.iter().filter(|w| self.failing(Some(&w.1))).collect();
                    let going: Vec<&(String, String, bool)> =
                        ways.iter().filter(|w| !self.failing(Some(&w.1))).collect();
                    if node.branches.len() == 1
                        && self.reaches(
                            &node.branches[0].next,
                            &node.name,
                            node.otherwise.as_deref(),
                        )
                    {
                        let way = &node.branches[0];
                        let mut until = stops.clone();
                        until.insert(node.name.clone());
                        let inner = self.walk(Some(way.next.clone()), &until, &on_way);
                        let label = self.plain(&way.when);
                        items.push(Item::Block {
                            kind: "loop",
                            sections: vec![(label, inner)],
                        });
                        current = node.otherwise.clone();
                    } else if !stopping.is_empty() && going.len() == 1 {
                        // A check: the way on, and a break where it stops.
                        let on = going[0].clone();
                        if !on.2 {
                            items.push(self.note(on.0.clone(), NoteKind::Check));
                        }
                        let stopping: Vec<(String, String, bool)> =
                            stopping.into_iter().cloned().collect();
                        for way in stopping {
                            let stop = self.stop(&way.1);
                            items.push(Item::Block {
                                kind: "break",
                                sections: vec![(way.0.clone(), stop)],
                            });
                        }
                        current = Some(on.1);
                    } else if node.branches.is_empty() {
                        current = node.otherwise.clone();
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
                        let (block, meet) = self.parting(&node.name, &ways, stops, &on_way);
                        items.extend(block);
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

/// The diagram of `card` (`SequenceDiagram.make`). `exported`: conditions with their
/// paths as written out.
pub fn sequence(card: &Card, project: &Project, exported: bool) -> Sequence {
    let mut b = Builder {
        card,
        project,
        exported,
        participants: Vec::new(),
        ids: HashMap::new(),
        me: 0,
        by_name: HashMap::new(),
        ended: None,
    };
    let home = project.section_of(card);
    let place = project.exported_section(&home);
    b.me = b.participant(text!("charts.participant.card", place, name = card.name));
    let mut items = Vec::new();
    if card.kind == "flow" {
        for node in &card.nodes {
            b.by_name.entry(node.name.as_str()).or_insert(node);
        }
        if let Some(begin) = card.nodes.iter().find(|n| n.kind == "begin") {
            if let Some(started) = b.trigger_of(begin) {
                items.extend(b.starting(&started));
            }
            // From the begin itself, so ways that part right there are drawn too.
            items.extend(b.walk(Some(begin.name.clone()), &HashSet::new(), &HashSet::new()));
        }
    }
    if items.is_empty() {
        items.push(b.note(text!("charts.note.noRows"), NoteKind::Plain));
    }
    Sequence {
        participants: b.names(),
        items,
    }
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
