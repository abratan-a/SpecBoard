//! Data drawn (`specboard://SpecBoard/Docs/Design:CardKinds`「データ（instance）と定義のセクション」):
//! the data cards of a section — the sections in it each a subgraph — or one data card, with the
//! lines between them and to the cards they meet outside, as a mermaid flowchart. A box is a data
//! card's name, and a field's value under it when one is asked (`![[Data:|ip]]`); a line is drawn in
//! its kind's look, said by where it lands (`ports[number=3000,protocol=tcp]` → `3000/tcp`; none at
//! the card's head). Values, not structure, are the point here, so a class diagram would draw none
//! of it.
//!
//! Written out, in the window's documents and through MCP the picture is one: only the head
//! differs, and the rev is of the picture, so a document pasted with it can be told stale.

use super::{Project, ident};
use crate::kinds::{INSTANCE, OF_LINE};
use crate::model::Card;
use crate::tools::picks::{self, Pick};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::HashMap;

/// What a data diagram is of.
pub enum Scope<'a> {
    /// A section by its id: its data cards and those of the sections in it.
    Section(String),
    /// One data card.
    Card(&'a Card),
}

/// The kinds a class diagram draws: a section holding one is drawn so, data or not.
const SCHEMA_KINDS: [&str; 4] = ["entity", "value", "enum", "store"];

/// The section and the sections in it, each once, the section first.
fn sections_under(project: &Project, section: &str) -> Vec<String> {
    let mut all = vec![section.to_string()];
    let mut i = 0;
    while i < all.len() {
        for inner in project.directory.sections_in(&all[i]) {
            if !all.contains(&inner) {
                all.push(inner);
            }
        }
        i += 1;
    }
    all
}

/// Whether a section's picture is its data: data cards in it or in the sections in it, and none of
/// the cards a class diagram draws — where the two are mixed, the shapes are what is drawn.
pub fn holds_data(project: &Project, section: &str) -> bool {
    let mut data = false;
    for id in sections_under(project, section) {
        for &i in project.directory.members(&id) {
            let card = project.nth(i);
            if SCHEMA_KINDS.contains(&card.kind.as_str()) {
                return false;
            }
            data |= card.kind == INSTANCE;
        }
    }
    data
}

/// The colour a line's look names, as the window draws it on a light page (`EdgeInk`).
fn ink(color: &str) -> &'static str {
    match color {
        "amber" => "#A9781F",
        "blue" => "#1F5FA9",
        "green" => "#1E7A44",
        "teal" => "#0E6469",
        "violet" => "#6A3FA0",
        "red" => "#A83232",
        _ => "#6B7787",
    }
}

/// A line's look as mermaid's `linkStyle` says it: its colour, and its dash (`EdgeDash`).
fn style(project: &Project, line: &str) -> String {
    let Some(look) = project.defs.line(line).map(|d| &d.look) else {
        return format!("stroke:{}", ink("grey"));
    };
    let dash = match look.dash.as_str() {
        "dashed" => ",stroke-dasharray:7 5",
        "dotted" => ",stroke-dasharray:2 4",
        _ => "",
    };
    format!("stroke:{}{dash}", ink(&look.color))
}

/// Whether a line's look ends with nothing at its far end (`---`, not `-->`).
fn open_end(project: &Project, line: &str) -> bool {
    project
        .defs
        .line(line)
        .is_some_and(|d| d.look.end == "none")
}

/// Where a line lands, said short: the values its picks name (`ports[number=3000,protocol=tcp]`
/// → `3000/tcp`), else the path as written; nothing at the card's head.
pub fn landing_label(at: &str) -> String {
    let at = at.trim_matches([' ', '\t']);
    let inside: Vec<String> = match crate::pathtext::split_section(at) {
        // A full path: past its card.
        (Some(_), rest) => picks::split(rest).into_iter().skip(1).collect(),
        (None, _) => picks::split(at),
    };
    let mut said: Vec<String> = Vec::new();
    for name in &inside {
        match picks::step(name) {
            Ok((_, Some(Pick::Where(keys)))) => {
                said.extend(keys.into_iter().map(|(_, lit)| lit.text));
            }
            Ok((_, Some(Pick::Is(lit)))) => said.push(lit.text),
            _ => {}
        }
    }
    if said.is_empty() {
        inside
            .iter()
            .filter(|n| !n.is_empty())
            .cloned()
            .collect::<Vec<_>>()
            .join(".")
    } else {
        said.join("/")
    }
}

/// A value as a box says it: a text as it is, an array's elements joined by「, 」, anything else
/// as JSON; nothing for null.
fn said(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        Value::Array(items) => items
            .iter()
            .map(said)
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(", "),
        other => serde_json::to_string(&super::sorted(other)).unwrap_or_default(),
    }
}

/// One box: a card by the name `name`, a data card's value at `field` under it.
fn boxed(id: &str, name: &str, card: &Card, field: Option<&[String]>) -> String {
    let mut text = ident::mermaid_text(name);
    if let Some(names) = field
        && card.kind == INSTANCE
        && let Some(value) = crate::tools::instances::value_at(card.instance_data(), names)
    {
        let value = said(value);
        if !value.is_empty() {
            text.push_str("<br/>");
            text.push_str(&ident::mermaid_text(&value));
        }
    }
    format!("{id}[\"{text}\"]")
}

/// The data cards drawn, by section in the order the sections are read, and the section they are
/// read from.
fn gathered<'a>(
    project: &Project<'a>,
    scope: &Scope<'a>,
) -> (String, Vec<(String, Vec<&'a Card>)>) {
    match scope {
        Scope::Card(card) => (project.section_of(card), vec![(String::new(), vec![*card])]),
        Scope::Section(section) => (
            section.clone(),
            sections_under(project, section)
                .into_iter()
                .map(|id| {
                    let cards: Vec<&Card> = project
                        .directory
                        .members(&id)
                        .iter()
                        .map(|&i| project.nth(i))
                        .filter(|c| c.kind == INSTANCE)
                        .collect();
                    (id, cards)
                })
                .collect(),
        ),
    }
}

/// The field a box says beside a data card's name, by its names (`net.ip`) — or why not: a field
/// none of the data's shapes has. None when none is asked.
fn field_of(
    project: &Project,
    drawn: &[&Card],
    field: Option<&str>,
) -> Result<Option<Vec<String>>, String> {
    let Some(f) = field
        .map(|f| f.trim_matches([' ', '\t']))
        .filter(|f| !f.is_empty())
    else {
        return Ok(None);
    };
    let names = picks::split(f);
    let head = names
        .first()
        .and_then(|n| picks::step(n).ok())
        .map(|(b, _)| b.to_string())
        .unwrap_or_default();
    let mut shapes: Vec<&Card> = Vec::new();
    for card in drawn {
        if let Some(shape) = crate::tools::instances::shape(project, card)
            && !shapes.iter().any(|s| s.id == shape.id)
        {
            shapes.push(shape);
        }
    }
    if !shapes
        .iter()
        .any(|s| s.fields.iter().any(|x| x.name == head))
    {
        return Err(text!(
            "export.chart.fieldNotInShape",
            field = f,
            shapes = crate::text::List::of(shapes.iter().map(|s| s.name.as_str()))
        ));
    }
    Ok(Some(names))
}

/// Why `field` cannot be said beside the data a scope draws, when it cannot (`find_embed`, and
/// with it the check's broken paths).
pub fn field_problem(project: &Project, scope: &Scope, field: &str) -> Option<String> {
    let (_, groups) = gathered(project, scope);
    let drawn: Vec<&Card> = groups.iter().flat_map(|(_, c)| c.iter().copied()).collect();
    field_of(project, &drawn, Some(field)).err()
}

/// The picture: `flowchart LR`, the boxes (each section in the scope a subgraph), the lines, and
/// their looks. Or why it cannot be drawn: a field none of the data's shapes has.
fn body(project: &Project, scope: &Scope, field: Option<&str>) -> Result<Vec<String>, String> {
    let (base, groups) = gathered(project, scope);
    let drawn: Vec<&Card> = groups.iter().flat_map(|(_, c)| c.iter().copied()).collect();
    let field_names = field_of(project, &drawn, field)?;
    let field = field_names.as_deref();
    let mut ids: HashMap<&str, String> = HashMap::new();
    for (i, card) in drawn.iter().enumerate() {
        ids.insert(card.id.as_str(), format!("n{}", i + 1));
    }
    // The lines: each data card's own, and those of the cards outside that reach one.
    struct Line<'c> {
        from: &'c Card,
        to: &'c Card,
        kind: &'c str,
        at: String,
    }
    let mut lines: Vec<Line> = Vec::new();
    for card in &drawn {
        for line in &card.connections {
            if line.line == OF_LINE {
                continue;
            }
            let Some(to) = line.to.as_deref().and_then(|t| project.card(t)) else {
                continue;
            };
            lines.push(Line {
                from: card,
                to,
                kind: &line.line,
                at: line.at.as_deref().map(landing_label).unwrap_or_default(),
            });
        }
    }
    for card in project.detail.schemas.iter() {
        if ids.contains_key(card.id.as_str()) {
            continue;
        }
        for line in &card.connections {
            if line.line == OF_LINE {
                continue;
            }
            let Some(to) = line
                .to
                .as_deref()
                .and_then(|t| project.card(t))
                .filter(|t| ids.contains_key(t.id.as_str()))
            else {
                continue;
            };
            lines.push(Line {
                from: card,
                to,
                kind: &line.line,
                at: line.at.as_deref().map(landing_label).unwrap_or_default(),
            });
        }
    }
    // The cards outside met, in the order they are first met, by their paths from the scope.
    let mut outside: Vec<&Card> = Vec::new();
    for line in &lines {
        for end in [line.from, line.to] {
            if !ids.contains_key(end.id.as_str()) && !outside.iter().any(|c| c.id == end.id) {
                outside.push(end);
            }
        }
    }
    for (i, card) in outside.iter().enumerate() {
        ids.insert(card.id.as_str(), format!("n{}", drawn.len() + i + 1));
    }
    let mut out = vec!["flowchart LR".to_string()];
    let mut subgraphs = 0;
    for (section, cards) in &groups {
        let inner = !section.is_empty() && section != &base;
        if inner && cards.is_empty() {
            continue;
        }
        let indent = if inner { "        " } else { "    " };
        if inner {
            subgraphs += 1;
            let name = project.exported_section(section);
            let shown = name
                .strip_prefix(&format!("{}/", project.exported_section(&base)))
                .unwrap_or(&name)
                .to_string();
            out.push(format!(
                "    subgraph s{subgraphs}[\"{}\"]",
                ident::mermaid_text(&shown)
            ));
        }
        for card in cards {
            out.push(format!(
                "{indent}{}",
                boxed(&ids[card.id.as_str()], &card.name, card, field)
            ));
        }
        if inner {
            out.push("    end".into());
        }
    }
    for card in &outside {
        out.push(format!(
            "    {}",
            boxed(
                &ids[card.id.as_str()],
                &project.exported_from(card, &base),
                card,
                field
            )
        ));
    }
    let mut looks: Vec<(String, Vec<usize>)> = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        let arrow = if open_end(project, line.kind) {
            "---"
        } else {
            "-->"
        };
        let label = if line.at.is_empty() {
            String::new()
        } else {
            format!("|\"{}\"|", ident::mermaid_text(&line.at))
        };
        out.push(format!(
            "    {} {arrow}{label} {}",
            ids[line.from.id.as_str()],
            ids[line.to.id.as_str()]
        ));
        let look = style(project, line.kind);
        match looks.iter_mut().find(|(l, _)| *l == look) {
            Some((_, at)) => at.push(i),
            None => looks.push((look, vec![i])),
        }
    }
    for (look, at) in looks {
        out.push(format!(
            "    linkStyle {} {look}",
            at.iter()
                .map(usize::to_string)
                .collect::<Vec<_>>()
                .join(",")
        ));
    }
    Ok(out)
}

/// Twelve hex characters of SHA-256 of the picture.
fn revision_of(lines: &[String]) -> String {
    let digest = Sha256::digest(lines.join("\n").as_bytes());
    digest
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>()[..12]
        .to_string()
}

/// The rev of the picture: it changes exactly when the picture would.
pub fn revision(project: &Project, scope: &Scope, field: Option<&str>) -> Result<String, String> {
    body(project, scope, field).map(|b| revision_of(&b))
}

/// What a scope is said as in the head: a section with its `:`, a card by its path.
pub fn named(project: &Project, scope: &Scope) -> String {
    match scope {
        Scope::Section(id) => format!("{}:", project.exported_section(id)),
        Scope::Card(card) => project.exported(card),
    }
}

/// The diagram, as it is written out (`exported`: the head only the mark and the rev) or as the
/// window and MCP are given it (the head saying what it was drawn from).
pub fn render(
    project: &Project,
    scope: &Scope,
    field: Option<&str>,
    exported: bool,
) -> Result<String, String> {
    let lines = body(project, scope, field)?;
    let rev = revision_of(&lines);
    let head = if exported {
        format!("%% specboard rev={rev}")
    } else {
        let asked = field
            .map(|f| f.trim_matches([' ', '\t']))
            .filter(|f| !f.is_empty())
            .map(|f| format!(" field={}", ident::line_comment(f)))
            .unwrap_or_default();
        format!(
            "%% specboard project={} data={}{asked} rev={rev}",
            project.detail.project.slug,
            ident::line_comment(&named(project, scope)),
        )
    };
    let mut out = vec![head];
    out.extend(lines);
    Ok(out.join("\n") + "\n")
}
