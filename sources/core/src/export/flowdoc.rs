//! A flow as Markdown: what it is, its picture, then each node in the flow's order — what starts
//! it, the rows it does, the ways it parts, what it waits for, the flow it runs, what it gives
//! back — and where each goes on. A reader is given the whole flow in one text, not a node at a
//! time. `card_read`'s `as: markdown` and a page of charts are made by the same hands: the one
//! gives the paths `[[…]]` as written, the other as a document writes them out.

use super::Project;
use super::charts::{self, begin_partner, code, start_partner};
use super::documents;
use crate::model::{Card, FlowNode, Step};
use crate::notetext::exported_path;
use std::collections::HashMap;

/// A path as the text says it, for `r` to write as the reader is given it.
fn place(path: &str) -> String {
    format!("[[{}]]", exported_path(path))
}

fn places(paths: &[String]) -> String {
    crate::text::list_comma(&paths.iter().map(|p| place(p)).collect::<Vec<_>>())
}

/// One line of a node's list: what it is called and what it says.
fn labelled(label: String, value: impl std::fmt::Display) -> String {
    format!("{label}: {value}")
}

/// Who a row, a start or an end deals with: the flows it names and the program, or — naming
/// none — `none`.
fn other(peers: &[String], program: Option<&str>, none: impl FnOnce() -> String) -> String {
    let names: Vec<String> = peers
        .iter()
        .map(|p| place(p))
        .chain(program.map(code))
        .collect();
    if names.is_empty() {
        none()
    } else {
        crate::text::list(&names)
    }
}

/// A node's own path, from the flow it is in.
fn node_path(flow: &str, name: &str) -> String {
    format!("[[{}]]", crate::pathtext::member(flow, name))
}

/// A row of a process node as a line of its table: where it is, what it does, with whom or on
/// what, what it carries, what it says.
fn step_cells(index: usize, step: &Step) -> Vec<String> {
    let mut with = match step.op.as_str() {
        "read" | "write" => places(&step.paths),
        "sign" | "verify" => {
            let mut said = places(&step.paths);
            if let Some(key) = &step.key {
                if !said.is_empty() {
                    said.push(' ');
                }
                said += &text!("flowdoc.step.key", key = place(key));
            }
            said
        }
        _ => other(&step.peers, step.program.as_deref(), start_partner),
    };
    let mut about = Vec::new();
    if let Some(via) = &step.via {
        about.push(format!("via {}", code(via)));
    }
    if let Some(initiator) = &step.initiator {
        about.push(text!("flowdoc.step.initiator", name = code(initiator)));
    }
    if let Some(key) = &step.sign_with {
        about.push(text!("flowdoc.step.sign", key = place(key)));
    }
    if let Some(key) = &step.verify_with {
        about.push(text!("flowdoc.step.verify", key = place(key)));
    }
    if !about.is_empty() {
        with = text!(
            "flowdoc.step.about",
            subject = with,
            items = crate::text::list(&about)
        );
    }
    let mut carries = places(&step.carries);
    if !step.returns.is_empty() {
        carries += &format!(" → {}", places(&step.returns));
    }
    vec![
        code(&format!("steps[{index}]")),
        step.op.clone(),
        with,
        carries.trim().to_string(),
        step.description.clone().unwrap_or_default(),
    ]
}

/// What a node says of itself as its kind has it, line by line, before where it goes on.
fn own_lines(node: &FlowNode) -> Vec<String> {
    let mut lines = Vec::new();
    let event = |node: &FlowNode| {
        node.event.as_ref().map(|e| {
            if e.outcome.is_empty() {
                text!("flowdoc.event.ended", flow = place(&e.flow))
            } else {
                text!(
                    "flowdoc.event.outcome",
                    flow = place(&e.flow),
                    outcome = code(&e.outcome)
                )
            }
        })
    };
    match node.kind.as_str() {
        "begin" => {
            lines.push(labelled(
                text!("flowdoc.label.trigger"),
                code(node.trigger.as_deref().unwrap_or("manual")),
            ));
            if node.trigger.as_deref() == Some("receive") {
                lines.push(labelled(
                    text!("flowdoc.label.partner"),
                    other(&node.peers, node.program.as_deref(), || begin_partner(node)),
                ));
            }
            if let Some(said) = event(node) {
                lines.push(format!("event: {said}"));
            }
            if let Some(every) = &node.every {
                lines.push(labelled(text!("flowdoc.label.interval"), every));
            }
        }
        "run" => {
            if let Some(target) = &node.target {
                lines.push(labelled(text!("flowdoc.label.runFlow"), place(target)));
            }
            if !node.targets.is_empty() {
                lines.push(labelled(
                    text!(
                        "flowdoc.label.runFlows",
                        join = code(node.join.as_deref().unwrap_or("all"))
                    ),
                    places(&node.targets),
                ));
            }
        }
        "eval" => {
            if let Some(value) = &node.value {
                lines.push(labelled(text!("flowdoc.label.decisionValue"), place(value)));
            }
        }
        "wait" => {
            let what = if node.wait_for.as_deref() == Some("event") {
                event(node)
            } else {
                None
            };
            if let Some(what) = what.or_else(|| node.wait_for.as_deref().map(code)) {
                lines.push(labelled(text!("flowdoc.label.waitFor"), what));
            }
            if let Some(timeout) = &node.timeout {
                lines.push(labelled(text!("flowdoc.label.timeout"), timeout));
            }
        }
        "join" => {
            lines.push(labelled(
                text!("flowdoc.label.join"),
                code(node.join.as_deref().unwrap_or("all")),
            ));
            if let Some(partial) = &node.partial {
                lines.push(labelled(text!("flowdoc.label.missing"), code(partial)));
            }
        }
        "end" => {
            if let Some(outcome) = &node.outcome {
                lines.push(labelled(text!("flowdoc.label.outcome"), code(outcome)));
            }
            if let Some(reply) = &node.reply {
                lines.push(labelled(
                    text!("flowdoc.label.replyTo"),
                    other(&reply.peers, reply.program.as_deref(), start_partner),
                ));
                if !reply.carries.is_empty() {
                    lines.push(labelled(
                        text!("flowdoc.label.returns"),
                        places(&reply.carries),
                    ));
                }
                if let Some(key) = &reply.sign_with {
                    lines.push(labelled(text!("flowdoc.label.sign"), place(key)));
                }
            }
        }
        _ => {}
    }
    // What a start receives, a run passes and is given back: on the node itself.
    if node.kind != "process" {
        if !node.carries.is_empty() {
            lines.push(labelled(
                text!("flowdoc.label.carries"),
                places(&node.carries),
            ));
        }
        if !node.returns.is_empty() {
            lines.push(labelled(
                text!("flowdoc.label.receives"),
                places(&node.returns),
            ));
        }
        if let Some(via) = &node.via {
            lines.push(format!("via: {}", code(via)));
        }
        if let Some(initiator) = &node.initiator {
            lines.push(labelled(text!("flowdoc.label.initiator"), code(initiator)));
        }
        if let Some(key) = &node.verify_with {
            lines.push(labelled(text!("flowdoc.label.verifyKey"), place(key)));
        }
        if let Some(key) = &node.sign_with {
            lines.push(labelled(text!("flowdoc.label.signKey"), place(key)));
        }
    }
    lines
}

/// Where a node goes on, each way to the node's path: its next ones, an eval's branches in the
/// order they are tried and its else, a run's failing, a wait's time running out.
fn way_lines(node: &FlowNode, flow: &str) -> Vec<String> {
    let to = |name: &str| node_path(flow, name);
    let mut lines = Vec::new();
    if !node.nexts.is_empty() {
        let label = if node.nexts.len() > 1 {
            text!("flowdoc.label.nextParallel")
        } else {
            text!("flowdoc.label.next")
        };
        lines.push(labelled(
            label,
            crate::text::list_comma(&node.nexts.iter().map(|n| to(n)).collect::<Vec<_>>()),
        ));
    }
    for branch in &node.branches {
        lines.push(labelled(
            text!("flowdoc.label.condition", when = branch.when),
            to(&branch.next),
        ));
    }
    if let Some(otherwise) = &node.otherwise {
        lines.push(labelled(text!("flowdoc.label.otherwise"), to(otherwise)));
    }
    if let Some(error) = &node.on_error {
        lines.push(labelled(text!("flowdoc.label.onFailure"), to(error)));
    }
    if let Some(timeout) = &node.on_timeout {
        lines.push(labelled(text!("flowdoc.label.onTimeout"), to(timeout)));
    }
    lines
}

/// One node: its heading (its name and kind) at `depth`, its label and description, what it
/// does, and where it goes on.
fn node_markdown(node: &FlowNode, flow: &str, depth: usize, r: &dyn Fn(&str) -> String) -> String {
    let mut parts = vec![format!(
        "{} {}",
        "#".repeat(depth.min(6)),
        text!("flowdoc.node.heading", name = node.name, kind = node.kind)
    )];
    if let Some(label) = node.label.as_deref().filter(|l| !l.trim().is_empty()) {
        parts.push(format!("**{}**", documents::text(label)));
    }
    if let Some(description) = node.description.as_deref().filter(|d| !d.trim().is_empty()) {
        parts.push(r(description).trim().to_string());
    }
    // One list of what it is and where it goes on; a process's rows between them as a table.
    let mut lines = own_lines(node);
    if node.kind == "process" && !node.steps.is_empty() {
        if !lines.is_empty() {
            parts.push(list(&std::mem::take(&mut lines), r));
        }
        let columns = [
            text!("flowdoc.column.row"),
            "do".to_string(),
            text!("flowdoc.column.partnerOrPath"),
            text!("flowdoc.column.carries"),
            text!("common.label.description"),
        ];
        let cells: Vec<Vec<String>> = node
            .steps
            .iter()
            .enumerate()
            .map(|(i, s)| step_cells(i, s))
            .collect();
        parts.push(documents::table(&columns, &cells, r));
    }
    lines.extend(way_lines(node, flow));
    if !lines.is_empty() {
        parts.push(list(&lines, r));
    }
    parts.join("\n\n")
}

fn list(lines: &[String], r: &dyn Fn(&str) -> String) -> String {
    lines
        .iter()
        .map(|l| format!("- {}", r(l).replace('\n', " ")))
        .collect::<Vec<_>>()
        .join("\n")
}

/// A flow below its own heading: its description, its picture (`diagram`, the mermaid text), then
/// its nodes in the flow's order, each a heading at `depth`. `r` writes a text with its paths
/// `[[…]]` as the reader is given them.
pub fn body(
    flow: &Card,
    project: &Project,
    diagram: &str,
    depth: usize,
    r: &dyn Fn(&str) -> String,
) -> String {
    let path = exported_path(&project.full(flow));
    let mut parts = Vec::new();
    if !flow.description.trim().is_empty() {
        parts.push(r(&flow.description).trim().to_string());
    }
    parts.push(format!(
        "```mermaid\n{}\n```",
        diagram.trim_end_matches('\n')
    ));
    for node in &flow.nodes {
        parts.push(node_markdown(node, &path, depth, r));
    }
    parts.join("\n\n")
}

/// A flow as `card_read`'s `as: markdown` gives it: its label or name as the title, then
/// `body` — its flowchart as `as: diagram` draws it, its nodes, the paths `[[…]]` as written.
pub fn as_written(flow: &Card, project: &Project) -> String {
    let title = if flow.label.is_empty() {
        &flow.name
    } else {
        &flow.label
    };
    let diagram = charts::render_drawn(flow, project, "flowchart");
    format!(
        "# {title}\n\n{}",
        body(
            flow,
            project,
            &diagram,
            2,
            &|t: &str| documents::as_written(t, flow, project)
        )
    )
}

/// A flow on a page of charts at `here`: `body` with its picture as the page draws it and its
/// paths written out as a document's are, its nodes a heading below the flow's.
pub fn on_page(
    flow: &Card,
    project: &Project,
    here: &str,
    paths: &HashMap<String, String>,
) -> String {
    let diagram = charts::render(flow, project, None);
    body(flow, project, &diagram, 3, &|t: &str| {
        documents::render(t, flow, project, here, paths)
    })
}
