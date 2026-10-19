//! Paths read against a project (`WrittenPath`, `CardWalk`, `resolvePath`, `landing` and
//! `projectValue` in the Swift app's Model): a path taken apart, the walk of its names
//! inside a card — a field, on through a `ref`; a formatdoc's row; a flow's node; a
//! custom card's row — and what it lands on.

use super::Project;
use crate::model::{Card, Field, FormatRow};
use crate::ojson;
use serde_json::Value;

/// A path as it is written, taken apart: the card it starts from, as written, and the
/// names inside the card as written (quoted where they were), their `[]` dropped.
pub struct WrittenPath {
    pub card: String,
    pub names: Vec<String>,
    pub well_formed: bool,
}

fn trim_spaces(s: &str) -> &str {
    s.trim_matches(|c: char| c == ' ' || c == '\t')
}

impl WrittenPath {
    pub fn new(raw: &str) -> Option<WrittenPath> {
        let raw = trim_spaces(raw);
        let (section, rest) = match crate::pathtext::split_section(raw) {
            (Some(s), rest) => (Some(trim_spaces(s).to_string()), rest),
            (None, rest) => (None, rest),
        };
        // A data card's array element picked holds dots of its own (`hosts[ip=192.0.2.1]`), and a
        // quoted name its own.
        let parts: Vec<String> = crate::pathtext::dots(rest)
            .into_iter()
            .map(|p| crate::pathtext::without_all(p).to_string())
            .collect();
        let well_formed = !parts.iter().any(String::is_empty);
        let named: Vec<String> = parts.into_iter().filter(|p| !p.is_empty()).collect();
        let head = named.first()?.clone();
        Some(WrittenPath {
            card: match section {
                Some(s) => format!("{s}:{head}"),
                None => head,
            },
            names: named[1..].to_vec(),
            well_formed,
        })
    }
}

/// Where the names inside a card lead.
pub enum Place<'a> {
    Card,
    Field(&'a Field),
    FormatRow(Vec<String>),
    Node,
    CustomRow,
}

/// A walk that found its place: the place, and the names walked in each card entered.
pub struct Walk<'a> {
    pub place: Place<'a>,
    pub legs: Vec<Vec<String>>,
}

/// The row of a formatdoc a path names, through items only.
pub fn format_row<'r>(path: &[String], rows: &'r [FormatRow]) -> Option<&'r FormatRow> {
    let found = rows.iter().find(|r| Some(&r.name) == path.first())?;
    if path.len() == 1 {
        return Some(found);
    }
    if found.kind != "item" {
        return None;
    }
    format_row(&path[1..], &found.rows)
}

/// The item a path lands in: the row itself when it is an item, else the one it is in.
pub fn format_item(path: &[String], rows: &[FormatRow]) -> Vec<String> {
    match format_row(path, rows) {
        None => Vec::new(),
        Some(r) if r.kind == "item" => path.to_vec(),
        Some(_) => path[..path.len() - 1].to_vec(),
    }
}

fn format_deepest(path: &[String], rows: &[FormatRow]) -> bool {
    let mut list = rows;
    for (i, name) in path.iter().enumerate() {
        let Some(next) = list.iter().find(|r| &r.name == name) else {
            return false;
        };
        if next.kind != "item" {
            return i == path.len() - 1;
        }
        list = &next.rows;
    }
    true
}

/// The names inside `head` as written, walked (`CardWalk.walk`). The legs and the place hold the
/// names as they are.
pub fn walk<'a>(head: &'a Card, written: &[String], project: &Project<'a>) -> Option<Walk<'a>> {
    let names: Vec<String> = written
        .iter()
        .map(|n| crate::pathtext::plain(n).into_owned())
        .collect();
    let names = names.as_slice();
    if names.is_empty() {
        return Some(Walk {
            place: Place::Card,
            legs: vec![Vec::new()],
        });
    }
    match head.kind.as_str() {
        "formatdoc" => {
            return format_deepest(names, &head.rows).then(|| Walk {
                place: Place::FormatRow(names.to_vec()),
                legs: vec![names.to_vec()],
            });
        }
        // A stackdoc's version by its name, and nothing inside it.
        "stackdoc" => {
            return (names.len() == 1 && head.versions().iter().any(|v| v.name == names[0])).then(
                || Walk {
                    place: Place::CustomRow,
                    legs: vec![names.to_vec()],
                },
            );
        }
        "custom_goal" => {
            let mut list = &head.custom_rows;
            for name in names {
                let row = list.iter().find(|r| &r.name == name)?;
                list = &row.rows;
            }
            return Some(Walk {
                place: Place::CustomRow,
                legs: vec![names.to_vec()],
            });
        }
        // An enum's value by its name, and nothing inside it.
        "enum" => {
            return (names.len() == 1 && head.values.iter().any(|v| v.value == names[0])).then(
                || Walk {
                    place: Place::CustomRow,
                    legs: vec![names.to_vec()],
                },
            );
        }
        // A flow's node card is drawn as a row of the flow's; a process node's step by its place
        // (`steps[1]`) is drawn in its node.
        "flow" => {
            let node = project.inner_named(&head.id, &written[0])?;
            let step = |w: &String| {
                node.kind == "process"
                    && crate::detail::step_index(w.trim())
                        .is_some_and(|i| i < crate::tools::walk::step_count(node))
            };
            return match &written[1..] {
                [] => Some(Walk {
                    place: Place::Node,
                    legs: vec![names[..1].to_vec()],
                }),
                [w] if step(w) => Some(Walk {
                    place: Place::Node,
                    legs: vec![names[..1].to_vec()],
                }),
                _ => None,
            };
        }
        // Inside another container (a goal), the card inside it, and the rest in that card.
        kind if project.is_container(kind) => {
            let inner = project.inner_named(&head.id, &written[0])?;
            return walk(inner, &written[1..], project);
        }
        _ => {}
    }
    let mut list: &[Field] = &head.fields;
    let mut legs: Vec<Vec<String>> = vec![Vec::new()];
    let mut place = Place::Card;
    for (index, name) in names.iter().enumerate() {
        let next = list.iter().find(|f| &f.name == name)?;
        legs.last_mut().unwrap().push(name.clone());
        place = Place::Field(next);
        // A reference is a step of its own: the rest of the path lives over there.
        if index < names.len() - 1
            && next.is_ref()
            && let Some(target) = next
                .ref_schema_id
                .as_deref()
                .and_then(|id| project.card(id))
        {
            legs.push(Vec::new());
            list = &target.fields;
        } else {
            list = &next.children;
        }
    }
    Some(Walk { place, legs })
}

impl Walk<'_> {
    /// The row a line to this place lands on in the head's card.
    pub fn row(&self, head: &Card) -> String {
        match &self.place {
            Place::Card => String::new(),
            Place::Node => format!("#n.{}", crate::pathtext::quote(&self.legs[0][0])),
            Place::Field(_) => {
                crate::pathtext::join_names(self.legs.iter().flatten().map(String::as_str))
            }
            Place::FormatRow(names) => {
                let item = format_item(names, &head.rows);
                if item.is_empty() {
                    "#top".to_string()
                } else {
                    crate::pathtext::join_names(item.iter().map(String::as_str))
                }
            }
            Place::CustomRow => {
                crate::pathtext::join_names(self.legs[0].iter().map(String::as_str))
            }
        }
    }
}

/// Whether a path resolves, read in `note`'s section (`resolvePath(…).ok`): from where
/// it is written, or — with no section — from inside its own card.
pub fn resolves(project: &Project, raw: &str, from: &Card) -> bool {
    let Some(written) = WrittenPath::new(raw) else {
        return false;
    };
    let base = project.section_of(from);
    let head = project.find_seeing(&written.card, &base);
    if let Some(h) = head
        && walk(h, &written.names, project).is_some()
    {
        return true;
    }
    if !crate::pathtext::has_section(&written.card) {
        let mut names = vec![written.card.clone()];
        names.extend(written.names.iter().cloned());
        if walk(from, &names, project).is_some() {
            return true;
        }
    }
    false
}

/// Where a declared path lands, read from `base`: the card it starts from, and the row. A
/// card inside a container that is not a flow (a goal) is the card itself; a flow's node is
/// the flow's row.
pub fn landing<'a>(path: &str, base: &str, project: &Project<'a>) -> Option<(&'a Card, String)> {
    let written = WrittenPath::new(path)?;
    let mut target = project.find(&written.card, base)?;
    let mut names: &[String] = &written.names;
    while let Some(first) = names.first()
        && target.kind != "flow"
        && project.is_container(&target.kind)
        && let Some(inner) = project.inner_named(&target.id, first)
    {
        target = inner;
        names = &names[1..];
    }
    Some(match walk(target, names, project) {
        Some(w) => (target, w.row(target)),
        None => (target, names.join(".")),
    })
}

/// What `{{path}}` stands for: a field's fixed value, the values an enum — named, or held by
/// the field — can take, or a data card's value.
pub fn project_value(project: &Project, raw: &str, base: &str) -> Option<String> {
    inserted_value(project, raw, base).ok()
}

/// What `{{path}}` puts in, or why it puts in nothing — the check says the why, the export and
/// the window strike the path through.
pub fn inserted_value(project: &Project, raw: &str, base: &str) -> Result<String, String> {
    let none = || text!("export.insert.none", raw);
    let written = WrittenPath::new(raw).ok_or_else(none)?;
    let card = project.find(&written.card, base).ok_or_else(none)?;
    if card.kind == crate::kinds::INSTANCE {
        return data_value(project, card, &written.names);
    }
    let values = |target: &Card| {
        (target.kind == "enum" && !target.values.is_empty()).then(|| {
            target
                .values
                .iter()
                .map(|v| v.value.clone())
                .collect::<Vec<_>>()
                .join(" / ")
        })
    };
    if written.names.is_empty() {
        return values(card).ok_or_else(none);
    }
    let w = walk(card, &written.names, project).ok_or_else(none)?;
    let Place::Field(field) = w.place else {
        return Err(none());
    };
    if let Some(fixed) = field.fixed() {
        return Ok(match fixed {
            Value::String(s) => s.clone(),
            other => ojson::compact(&super::sorted(other)),
        });
    }
    if field.is_ref() {
        return field
            .ref_schema_id
            .as_deref()
            .and_then(|id| project.card(id))
            .and_then(values)
            .ok_or_else(none);
    }
    Err(none())
}

/// A data card's value at `names`, as a document says it: a text as it is, a number or a truth
/// as its text, an array's elements as a table's cell joins them, an object as JSON. The path is
/// held to the card's shape, and each pick on the way finds one element, or it is said which
/// does not.
fn data_value(project: &Project, card: &Card, names: &[String]) -> Result<String, String> {
    use crate::tools::{instances, tables};
    let full = project.full(card);
    let upto = |i: usize| format!("{full}.{}", names[..=i].join("."));
    if !names.is_empty()
        && let Some(shape) = instances::shape(project, card)
        && instances::field_at(project, shape, names).is_err()
    {
        return Err(text!(
            "export.insert.notInShape",
            full,
            names = names.join("."),
            shape = project.full(shape)
        ));
    }
    let data = card.instance_data();
    if let Err((i, why)) = instances::walked(data, names) {
        return Err(format!("{}: {why}", upto(i)));
    }
    match instances::value_at(data, names) {
        None | Some(Value::Null) => Err(text!(
            "export.insert.noValue",
            place = if names.is_empty() {
                full.clone()
            } else {
                upto(names.len() - 1)
            }
        )),
        Some(Value::String(text)) => Ok(text.clone()),
        Some(Value::Array(items)) => Ok(items
            .iter()
            .filter(|v| !v.is_null())
            .map(tables::cell)
            .collect::<Vec<_>>()
            .join(", ")),
        Some(other) => Ok(tables::cell(other)),
    }
}

/// The card a path starts from, read as the project's identities read it: from `owner`'s
/// section, or from inside `owner` for a field of its own.
pub fn identify_head<'a>(project: &Project<'a>, raw: &str, owner: &'a Card) -> Option<&'a Card> {
    let written = WrittenPath::new(raw)?;
    if !written.well_formed {
        return None;
    }
    let base = project.section_of(owner);
    if let Some(head) = project.find(&written.card, &base)
        && walk(head, &written.names, project).is_some()
    {
        return Some(head);
    }
    None
}
