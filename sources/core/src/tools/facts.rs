//! What a card names and does, one list each (`SchemaNode.links` in the Swift app's
//! Model/Actions, `rowLines`, `usesInEffect` and the keys in effect in StepEffects) — read
//! by the check of declared paths, the impact of a change and the diagrams, so they cannot
//! disagree about what is there. Actions are gone from the data: only a flow has rows,
//! in its nodes — node cards inside it, whose lines say what its rows read,
//! write, sign with and hand on to (`row_lines`, `keys_used`, `writes_of`).

use crate::model::{Card, CustomRow, FlowNode, FormatRow, Step};
use crate::notetext;
use std::collections::HashSet;

/// The row a formatdoc's rows at the top draw from: the card itself.
pub(crate) const TOP_ROW: &str = "#top";

/// What a link is (`SchemaLink.Kind`, without an action's keys and what follows it).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LinkKind {
    /// A note, a document or an item of one that sets a flow's rules.
    Rules,
    /// A place a note's or a document's text names, or a formatdoc's row draws.
    Mentions,
    /// A custom card's row joined to another card.
    Connects,
}

impl LinkKind {
    /// The kind of line it draws (`RefKind`).
    pub fn ref_kind(self) -> &'static str {
        match self {
            LinkKind::Rules => "note",
            LinkKind::Mentions => "mentions",
            LinkKind::Connects => "connect",
        }
    }
}

/// What a card names besides the places its rows read and write (`SchemaLink`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Link {
    pub kind: LinkKind,
    /// The row it is drawn from: `#b2`, `#m1`, an item's names, `#top`; empty for the card's head.
    pub row: String,
    pub path: String,
    /// When it is taken: only an action's `next` said one.
    pub text: Option<String>,
}

fn link(kind: LinkKind, row: String, path: String) -> Link {
    Link {
        kind,
        row,
        path,
        text: None,
    }
}

/// In the order the card draws them (`SchemaNode.links`): a flow's notes, from its head
/// (a flow has no row for a rule); for a note, every place its body names, once each, from
/// the line of the body that names it first (`#b<line>`); for a freedoc, the same from a row each
/// (`#m<n>`); for a formatdoc, what each item's rows name and draw, from the item's row
/// (`#top` for the rows at the top); for a stackdoc, what each version's body names, from the
/// version; for a custom card, what each row is joined to.
pub(crate) fn links(card: &Card) -> Vec<Link> {
    match card.kind.as_str() {
        "flow" => card
            .notes
            .iter()
            .map(|path| link(LinkKind::Rules, String::new(), path.clone()))
            .collect(),
        "formatdoc" => {
            let mut out = Vec::new();
            let from = |item: &[String], row: String, out: &mut Vec<Link>| {
                let mut seen = HashSet::new();
                for (_, place, text) in said(item, &card.rows) {
                    let paths: Vec<String> = if place == "target" {
                        vec![text]
                    } else {
                        notetext::refs(&text).into_iter().map(|r| r.path).collect()
                    };
                    for path in paths {
                        if seen.insert(path.clone()) {
                            out.push(link(LinkKind::Mentions, row.clone(), path));
                        }
                    }
                }
            };
            from(&[], TOP_ROW.to_string(), &mut out);
            walk_format(&card.rows, &mut |row, path, _| {
                if row.kind == "item" {
                    from(
                        path,
                        crate::pathtext::join_names(path.iter().map(String::as_str)),
                        &mut out,
                    );
                }
            });
            out
        }
        "note" => {
            let mut seen = HashSet::new();
            notetext::refs(&card.body)
                .into_iter()
                .filter(|r| seen.insert(r.path.clone()))
                .map(|r| {
                    let line = lines_before(&card.body, r.start);
                    link(LinkKind::Mentions, format!("#b{line}"), r.path)
                })
                .collect()
        }
        // A stackdoc: what each version's body names, from that version (its name).
        "stackdoc" => {
            let mut out = Vec::new();
            for version in card.versions() {
                let mut seen = HashSet::new();
                for r in notetext::refs(&version.body) {
                    if seen.insert(r.path.clone()) {
                        out.push(link(
                            LinkKind::Mentions,
                            crate::pathtext::quote(&version.name).into_owned(),
                            r.path,
                        ));
                    }
                }
            }
            out
        }
        "freedoc" => {
            let mut seen = HashSet::new();
            notetext::refs(&card.body)
                .into_iter()
                .filter(|r| seen.insert(r.path.clone()))
                .enumerate()
                .map(|(i, r)| link(LinkKind::Mentions, format!("#m{i}"), r.path))
                .collect()
        }
        "custom_goal" => {
            let mut out = Vec::new();
            walk_custom(&card.custom_rows, &mut |row, path| {
                for to in &row.links {
                    out.push(link(LinkKind::Connects, path.to_string(), to.clone()));
                }
            });
            out
        }
        _ => Vec::new(),
    }
}

/// The line breaks before `at` (a byte offset), as Swift counts `"\n"` characters:
/// `\r\n` is one character, and not one of them.
fn lines_before(text: &str, at: usize) -> usize {
    let mut count = 0;
    let mut previous = None;
    for c in text[..at].chars() {
        if c == '\n' && previous != Some('\r') {
            count += 1;
        }
        previous = Some(c);
    }
    count
}

// MARK: - A formatdoc's rows

/// Every row, an item before what is inside it, with its path from the top and how
/// deep it is (`FormatRows.walk`).
pub(crate) fn walk_format(rows: &[FormatRow], visit: &mut dyn FnMut(&FormatRow, &[String], usize)) {
    fn go(
        list: &[FormatRow],
        trail: &[String],
        depth: usize,
        visit: &mut dyn FnMut(&FormatRow, &[String], usize),
    ) {
        for row in list {
            let mut path = trail.to_vec();
            path.push(row.name.clone());
            visit(row, &path, depth);
            if row.kind == "item" {
                go(&row.rows, &path, depth + 1, visit);
            }
        }
    }
    go(rows, &[], 0, visit);
}

/// The Markdown a row holds, each with where it is in the row (`FormatRow.texts`): a
/// text's body, a table's headings and cells.
pub(crate) fn texts(row: &FormatRow) -> Vec<(String, String)> {
    match row.kind.as_str() {
        "text" => vec![("body".to_string(), row.body.clone())],
        "table" => {
            let mut out: Vec<(String, String)> = row
                .columns
                .iter()
                .enumerate()
                .map(|(i, c)| (format!("columns[{i}]"), c.clone()))
                .collect();
            for (r, line) in row.cells.iter().enumerate() {
                for (c, cell) in line.iter().enumerate() {
                    out.push((format!("cells[{r}][{c}]"), cell.clone()));
                }
            }
            out
        }
        _ => Vec::new(),
    }
}

/// What an item says itself, not counting the items inside it: its rows' Markdown and
/// targets, each with the row's path and where it is in the row (`FormatRows.said`).
pub(crate) fn said(path: &[String], rows: &[FormatRow]) -> Vec<(Vec<String>, String, String)> {
    let mut out = Vec::new();
    for row in super::walk::format_children(path, rows)
        .iter()
        .filter(|r| r.kind != "item")
    {
        let mut at = path.to_vec();
        at.push(row.name.clone());
        for (place, text) in texts(row) {
            out.push((at.clone(), place, text));
        }
        if !row.target.is_empty() {
            out.push((at, "target".to_string(), row.target.clone()));
        }
    }
    out
}

// MARK: - A custom card's rows

/// Every row, depth first, with its path: the names from the top joined by `.`
/// (`CustomRows.walk`).
pub(crate) fn walk_custom(rows: &[CustomRow], visit: &mut dyn FnMut(&CustomRow, &str)) {
    fn go(list: &[CustomRow], prefix: &str, visit: &mut dyn FnMut(&CustomRow, &str)) {
        for row in list {
            let path = if prefix.is_empty() {
                crate::pathtext::quote(&row.name).into_owned()
            } else {
                crate::pathtext::member(prefix, &row.name)
            };
            visit(row, &path);
            go(&row.rows, &path, visit);
        }
    }
    go(rows, "", visit);
}

/// The row at a path, if there is one (`CustomRows.row`).
pub(crate) fn custom_row<'r>(path: &str, rows: &'r [CustomRow]) -> Option<&'r CustomRow> {
    fn go<'r>(list: &'r [CustomRow], prefix: &str, path: &str) -> Option<&'r CustomRow> {
        let mut found = None;
        for row in list {
            let at = if prefix.is_empty() {
                crate::pathtext::quote(&row.name).into_owned()
            } else {
                crate::pathtext::member(prefix, &row.name)
            };
            if at == path {
                found = Some(row);
            }
            if let Some(deeper) = go(&row.rows, &at, path) {
                found = Some(deeper);
            }
        }
        found
    }
    go(rows, "", path)
}

// MARK: - A flow's rows

/// A line a row draws (`RowLine`): from the row, of a kind, to the path it names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RowLine {
    /// The row it leaves from: `#n.<node>`, `#n.<node>.<index>` for a process node's row.
    pub row: String,
    pub kind: &'static str,
    pub path: String,
    /// For a line to another flow's end: the outcome it waits for.
    pub outcome: Option<String>,
}

impl RowLine {
    /// The row as an argument path: `nodes.save.0` (`RowLine.place`).
    pub fn place(&self) -> String {
        match self.row.strip_prefix("#n.") {
            Some(rest) => format!("nodes.{rest}"),
            None => self.row.clone(),
        }
    }

    /// The row it lands on in `target`, the card `path` names: the end with the outcome
    /// waited for, or where the path itself lands.
    pub fn landing_row(&self, target: &Card, landed: &str) -> String {
        if let Some(outcome) = &self.outcome
            && let Some(end) = target
                .nodes
                .iter()
                .find(|n| n.kind == "end" && n.outcome.as_ref() == Some(outcome))
        {
            return format!("#n.{}", crate::pathtext::quote(&end.name));
        }
        landed.to_string()
    }
}

/// Whether it says what it does in rows: a flow with nodes (`speaksInRows`).
fn speaks_in_rows(card: &Card) -> bool {
    card.kind == "flow" && !card.nodes.is_empty()
}

/// A begin on receive, as the row it would be (`FlowNode.receiveRow`).
pub(crate) fn receive_row(node: &FlowNode) -> Option<Step> {
    if node.kind != "begin" || node.trigger.as_deref() != Some("receive") {
        return None;
    }
    Some(Step {
        op: "receive".into(),
        peers: node.peers.clone(),
        program: node.program.clone(),
        carries: node.carries.clone(),
        via: node.via.clone(),
        initiator: node.initiator.clone(),
        verify_with: node.verify_with.clone(),
        ..Step::default()
    })
}

/// An end's answer, as the return row it would be (`FlowNode.returnRow`).
pub(crate) fn return_row(node: &FlowNode) -> Option<Step> {
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
        ..Step::default()
    })
}

/// Every row with the step it is, in card order: what a begin receives, each process
/// node's rows, and what an end answers with (`eachRow`).
fn each_row(card: &Card, body: &mut dyn FnMut(&str, &Step)) {
    for node in &card.nodes {
        let row = format!("#n.{}", crate::pathtext::quote(&node.name));
        if let Some(step) = receive_row(node) {
            body(&row, &step);
        }
        if node.kind == "process" {
            for (index, step) in node.steps.iter().enumerate() {
                body(&format!("{row}.{index}"), step);
            }
        }
        if let Some(step) = return_row(node) {
            body(&row, &step);
        }
    }
}

/// Where a line lands, as its path in full as written (a card ghost by its own): the card it
/// reaches and the names there, or a landing written in full elsewhere.
pub(crate) fn line_path(
    project: &crate::export::Project,
    line: &crate::model::Connection,
) -> Option<String> {
    let at = line.at.as_deref().unwrap_or_default();
    if crate::pathtext::has_section(at) {
        return Some(at.to_string());
    }
    let head = project.named(line.to.as_deref()?)?;
    Some(if at.is_empty() {
        head
    } else {
        format!("{head}.{at}")
    })
}

/// What a node's row does (`do`), by its place.
fn step_op(node: &Card, index: usize) -> String {
    node.form
        .as_ref()
        .and_then(|f| f["rows"]["steps"].get(index))
        .and_then(|s| s["fields"]["do"].as_str())
        .unwrap_or_default()
        .to_string()
}

/// Whether a key line signs (`use: sign`) rather than checks.
fn signs(line: &crate::model::Connection) -> bool {
    line.fields.get("use").and_then(serde_json::Value::as_str) == Some("sign")
}

/// The lines its nodes draw on the diagrams (`rowLines`), from the lines its node cards hold
/// (in the order they were drawn from the nodes' rows before): to what a
/// run runs (`runs`), to another flow's end a begin or a wait waits for (`event`), the keys a
/// run signs and checks with; then, node by node, the key a begin's receive checks with, each
/// process row's reads and writes, its key, the flow a send or a call hands on to (`peer`),
/// what it signs and checks with, and the key an end's answer is signed with. What a row
/// carries draws nothing, nor the peer a receive or an answer names.
pub(crate) fn row_lines(project: &crate::export::Project, flow: &Card) -> Vec<RowLine> {
    if flow.kind != "flow" {
        return Vec::new();
    }
    let nodes = project.inner_of(&flow.id);
    let mut out = Vec::new();
    let mut push = |row: String, kind: &'static str, line: &crate::model::Connection| {
        if let Some(path) = line_path(project, line) {
            out.push(RowLine {
                row,
                kind,
                path,
                outcome: (kind == "event")
                    .then(|| {
                        line.fields
                            .get("outcome")
                            .and_then(serde_json::Value::as_str)
                            .map(str::to_string)
                    })
                    .flatten(),
            });
        }
    };
    fn head(node: &Card) -> Vec<&crate::model::Connection> {
        node.connections
            .iter()
            .filter(|l| l.from.is_empty())
            .collect()
    }
    for node in &nodes {
        let row = format!("#n.{}", crate::pathtext::quote(&node.name));
        for line in head(node) {
            match line.line.as_str() {
                "runs" => push(row.clone(), "run", line),
                "awaits" => push(row.clone(), "event", line),
                _ => {}
            }
        }
        if node.kind == "run" {
            for line in head(node).into_iter().filter(|l| l.line == "key") {
                push(
                    row.clone(),
                    if signs(line) {
                        "signedBy"
                    } else {
                        "verifiedBy"
                    },
                    line,
                );
            }
        }
    }
    for node in &nodes {
        let row = format!("#n.{}", crate::pathtext::quote(&node.name));
        // What a begin receives, checked with its key.
        if node.kind == "begin" {
            for line in head(node).into_iter().filter(|l| l.line == "key") {
                push(row.clone(), "verifiedBy", line);
            }
        }
        if node.kind == "process" {
            for line in node.connections.iter() {
                let Some(index) = crate::detail::step_index(&line.from) else {
                    continue;
                };
                let step_row = format!("{row}.{index}");
                match line.line.as_str() {
                    "reads" => push(step_row, "reads", line),
                    "writes" => push(step_row, "writes", line),
                    "key" => push(
                        step_row,
                        if signs(line) {
                            "signedBy"
                        } else {
                            "verifiedBy"
                        },
                        line,
                    ),
                    "partner" if ["send", "call"].contains(&step_op(node, index).as_str()) => {
                        push(step_row, "next", line)
                    }
                    _ => {}
                }
            }
        }
        // What an end answers with, signed with its key.
        if node.kind == "end" {
            for line in head(node).into_iter().filter(|l| l.line == "key") {
                push(row.clone(), "signedBy", line);
            }
        }
    }
    out
}

/// The keys a flow's nodes use: to sign (`use: sign`) or to check, by their paths — what its
/// node cards' key lines name.
pub(crate) fn keys_used(project: &crate::export::Project, flow: &Card, sign: bool) -> Vec<String> {
    project
        .inner_of(&flow.id)
        .iter()
        .flat_map(|n| n.connections.iter())
        .filter(|l| l.line == "key" && signs(l) == sign)
        .filter_map(|l| line_path(project, l))
        .collect()
}

/// What a flow's rows write, by their paths: its node cards' `writes` lines.
pub(crate) fn writes_of(project: &crate::export::Project, flow: &Card) -> Vec<String> {
    project
        .inner_of(&flow.id)
        .iter()
        .flat_map(|n| n.connections.iter())
        .filter(|l| l.line == "writes")
        .filter_map(|l| line_path(project, l))
        .collect()
}

/// Which way a use goes (`ModuleUse.Role`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Role {
    /// Parses it: a change means the reading code changes.
    Read,
    /// Produces it: a change means what it puts out changes.
    Write,
}

impl Role {
    pub fn name(self) -> &'static str {
        match self {
            Role::Read => "read",
            Role::Write => "write",
        }
    }
}

/// One place a flow reads or writes (`ModuleUse`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Use {
    pub path: String,
    pub role: Role,
    /// Why, or what for.
    pub note: Option<String>,
}

/// The places it reads and writes, from its rows (`usesInEffect`). What it receives,
/// sends and gets back counts as read — it has to read it to pass it on — and so does
/// what it signs or checks.
pub(crate) fn uses_in_effect(card: &Card) -> Vec<Use> {
    if !speaks_in_rows(card) {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut add = |paths: &[String], role: Role, note: &Option<String>| {
        out.extend(paths.iter().map(|path| Use {
            path: path.clone(),
            role,
            note: note.clone(),
        }));
    };
    for node in card.nodes.iter().filter(|n| n.kind == "run") {
        add(&node.carries, Role::Read, &node.description);
        add(&node.returns, Role::Read, &node.description);
    }
    each_row(card, &mut |_, step| match step.op.as_str() {
        "read" => add(&step.paths, Role::Read, &step.description),
        "write" => add(&step.paths, Role::Write, &step.description),
        "receive" | "send" | "return" => add(&step.carries, Role::Read, &step.description),
        "call" => {
            add(&step.carries, Role::Read, &step.description);
            add(&step.returns, Role::Read, &step.description);
        }
        "sign" | "verify" => add(&step.paths, Role::Read, &step.description),
        _ => {}
    });
    out
}
