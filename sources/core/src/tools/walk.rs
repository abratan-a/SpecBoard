//! A path walked inside a card, saying where it stopped when a name is not there
//! (`CardWalk` with its `Miss`), and an MCP target found (`MCPViews.locate`).

use crate::export::paths::WrittenPath;
use crate::export::{Named, Project};
use crate::model::{Card, Field, FormatRow, Ghost};
use serde_json::Value;

/// Where the names inside a card lead.
#[derive(Clone, Debug)]
pub(crate) enum Place<'a> {
    Card,
    Field(&'a Field),
    FormatRow(Vec<String>),
    /// A row by its names: a card of a kind the project defines (`items.a`, its kind of row and
    /// its address), or a goal's of before.
    CustomRow(Vec<String>),
    /// A field of a card of a kind the project defines, by its names (`owner.name` in a group).
    OwnField(Vec<String>),
    /// An enum's value, by its name (`Schemas:Status.shipped`).
    EnumValue(String),
    /// A process node's step, by its place (`steps[1]`): the text a line's `from` names it by.
    Step(usize),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MissKind {
    Field,
    FormatRow,
    /// A card inside a container (a flow's node, a goal's card).
    Inner,
    CustomRow,
    /// A value of a data card (`instance`): a field of its shape, an array's element picked.
    Value,
    /// An element of a data card's array picked wrong: why, as the one name there is.
    Pick,
    /// An enum's value: the values there are.
    EnumValue,
    /// A process node's step: the steps there are, by their places.
    Step,
}

/// Where a walk stopped, when a name is not there.
#[derive(Clone, Debug)]
pub(crate) struct Miss<'a> {
    pub kind: MissKind,
    /// The card it stopped in, and the names walked in it before the missing one.
    pub holder: &'a Card,
    pub walked: Vec<String>,
    /// The names walked from the head before the missing one.
    pub from_head: Vec<String>,
    /// What is there instead.
    pub there: Vec<String>,
}

/// A walk that found its place.
#[derive(Clone, Debug)]
pub(crate) struct Walk<'a> {
    pub head: &'a Card,
    /// The card the last name is in: `head`, or one a `ref` on the way embeds.
    pub holder: &'a Card,
    /// The names from `holder` to the place.
    pub within: Vec<String>,
    pub place: Place<'a>,
    /// The fields passed, in order, across `ref`s.
    pub fields: Vec<&'a Field>,
    /// Each card entered, with the names walked in it, outermost first.
    pub legs: Vec<(&'a Card, Vec<String>)>,
    /// How many names took the walk into a card inside a container (a flow's node, a
    /// goal's card): `head` is that card, and the path's head is that many names longer.
    pub entered: usize,
    /// The kind of row the place is a row of (`kinds::LinePlace`): a record's field's
    /// `fields`, a formatdoc's item's `rows`, a row of a project's own kind by its kind of row
    /// (the last); none for the head, a field of a project's own kind's card, a goal's row of
    /// before.
    pub row: Option<String>,
}

/// The one kind of row a built-in kind of card has (a record's `fields`, a formatdoc's
/// `rows`, a process node's `steps`), as its definition names it.
pub(crate) fn builtin_row(kind: &str) -> Option<String> {
    crate::kinds::builtin()
        .get(kind)
        .and_then(|k| k.rows.first())
        .map(|r| r.name.clone())
}

/// How many steps a process node has.
pub(crate) fn step_count(card: &Card) -> usize {
    card.form
        .as_ref()
        .and_then(|f| f.get("rows"))
        .and_then(|r| r.get("steps"))
        .and_then(Value::as_array)
        .map_or(0, Vec::len)
}

/// What a process node's row `index` does (`read`, `send`, …), when the row is there.
pub(crate) fn step_op(card: &Card, index: usize) -> Option<String> {
    card.form
        .as_ref()?
        .get("rows")?
        .get("steps")?
        .as_array()?
        .get(index)?
        .get("fields")?
        .get("do")?
        .as_str()
        .map(str::to_string)
}

/// A process node's steps by their places, as a path names them (`steps[0]`).
pub(crate) fn step_names(card: &Card) -> Vec<String> {
    (0..step_count(card))
        .map(|i| format!("steps[{i}]"))
        .collect()
}

/// Whether `path` names a row, or the start of one (`FormatRows.deepest`).
pub(crate) fn deepest(path: &[String], rows: &[FormatRow]) -> (Vec<String>, Vec<String>) {
    let mut list = rows;
    let mut found = Vec::new();
    for (index, name) in path.iter().enumerate() {
        let Some(next) = list.iter().find(|r| &r.name == name) else {
            return (found, path[index..].to_vec());
        };
        found.push(name.clone());
        if next.kind != "item" {
            return (found, path[index + 1..].to_vec());
        }
        list = &next.rows;
    }
    (found, Vec::new())
}

/// The row at a path, through items only (`FormatRows.row`).
pub(crate) fn format_row<'r>(path: &[String], rows: &'r [FormatRow]) -> Option<&'r FormatRow> {
    crate::export::paths::format_row(path, rows)
}

/// The rows directly in the item at `path`; the top's for none (`FormatRows.children`).
pub(crate) fn format_children<'r>(path: &[String], rows: &'r [FormatRow]) -> &'r [FormatRow] {
    if path.is_empty() {
        return rows;
    }
    format_row(path, rows)
        .map(|r| r.rows.as_slice())
        .unwrap_or(&[])
}

/// The names inside `head` as written (quoted where they were), walked. `through_refs`: whether a
/// name after a `ref` is looked for in the card it embeds (an MCP target does not go through).
/// What the walk holds — the names walked, the place's — are the names as they are, but a data
/// card's, which keep how each picks an element (`ports[0]`) as written.
pub(crate) fn walk<'a>(
    head: &'a Card,
    steps: &[String],
    project: &Project<'a>,
    through_refs: bool,
) -> Result<Walk<'a>, Miss<'a>> {
    let plain: Vec<String> = steps
        .iter()
        .map(|s| crate::pathtext::plain(s).into_owned())
        .collect();
    let names = plain.as_slice();
    let mut out = Walk {
        head,
        holder: head,
        within: Vec::new(),
        place: Place::Card,
        fields: Vec::new(),
        legs: vec![(head, Vec::new())],
        entered: 0,
        row: None,
    };
    if names.is_empty() {
        return Ok(out);
    }
    match head.kind.as_str() {
        "formatdoc" => {
            let (found, missing) = deepest(names, &head.rows);
            if !missing.is_empty() {
                return Err(Miss {
                    kind: MissKind::FormatRow,
                    holder: head,
                    there: format_children(&found, &head.rows)
                        .iter()
                        .map(|r| r.name.clone())
                        .collect(),
                    walked: found.clone(),
                    from_head: found,
                });
            }
            out.within = names.to_vec();
            out.legs = vec![(head, names.to_vec())];
            out.place = Place::FormatRow(names.to_vec());
            out.row = builtin_row(&head.kind);
            return Ok(out);
        }
        // A stackdoc's version by its name (`Docs:Changelog.v0_2_21`), and nothing inside it.
        "stackdoc" => {
            let versions = head.versions();
            let there = versions.iter().any(|v| v.name == names[0]);
            if !there || names.len() > 1 {
                let walked: Vec<String> = if there {
                    vec![names[0].clone()]
                } else {
                    Vec::new()
                };
                return Err(Miss {
                    kind: MissKind::CustomRow,
                    holder: head,
                    walked: walked.clone(),
                    from_head: walked,
                    there: if there {
                        Vec::new()
                    } else {
                        versions.into_iter().map(|v| v.name).collect()
                    },
                });
            }
            out.within = names.to_vec();
            out.legs = vec![(head, names.to_vec())];
            out.place = Place::CustomRow(names.to_vec());
            out.row = builtin_row(&head.kind);
            return Ok(out);
        }
        // An enum's value by its name (`Schemas:Status.shipped`), and nothing inside it.
        "enum" => {
            let there = head.values.iter().any(|v| v.value == names[0]);
            if !there || names.len() > 1 {
                let walked: Vec<String> = if there {
                    vec![names[0].clone()]
                } else {
                    Vec::new()
                };
                return Err(Miss {
                    kind: if there {
                        MissKind::CustomRow
                    } else {
                        MissKind::EnumValue
                    },
                    holder: head,
                    walked: walked.clone(),
                    from_head: walked,
                    there: if there {
                        Vec::new()
                    } else {
                        head.values.iter().map(|v| v.value.clone()).collect()
                    },
                });
            }
            out.within = names.to_vec();
            out.legs = vec![(head, names.to_vec())];
            out.place = Place::EnumValue(names[0].clone());
            out.row = builtin_row(&head.kind);
            return Ok(out);
        }
        // A process node's step by its place (`steps[1]`), and nothing inside it.
        "process" => {
            let count = step_count(head);
            let (name, index) = split_index(&steps[0]);
            match index.filter(|i| name == "steps" && *i < count && names.len() == 1) {
                Some(i) => {
                    let written = vec![format!("steps[{i}]")];
                    out.within = written.clone();
                    out.legs = vec![(head, written)];
                    out.place = Place::Step(i);
                    out.row = builtin_row(&head.kind);
                    return Ok(out);
                }
                None => {
                    return Err(Miss {
                        kind: MissKind::Step,
                        holder: head,
                        walked: Vec::new(),
                        from_head: Vec::new(),
                        there: step_names(head),
                    });
                }
            }
        }
        // A data card's value by its shape's names (`Data:Order1.customer.name`), an array's
        // element picked (`Data:Host1.ports[number=8200,protocol=tcp].number`, `ports[0]`).
        crate::kinds::INSTANCE => {
            let Some(shape) = super::instances::shape(project, head) else {
                return Err(Miss {
                    kind: MissKind::Value,
                    holder: head,
                    walked: Vec::new(),
                    from_head: Vec::new(),
                    there: Vec::new(),
                });
            };
            let names = steps;
            out.fields = super::instances::field_at(project, shape, names).map_err(|miss| {
                use super::instances::ValueMiss;
                let (i, kind, there) = match miss {
                    ValueMiss::Name(i, there) => (i, MissKind::Value, there),
                    ValueMiss::InArray(i) => (i, MissKind::Value, Vec::new()),
                    ValueMiss::Pick(i, why) => (i, MissKind::Pick, vec![why]),
                };
                Miss {
                    kind,
                    holder: head,
                    walked: names[..i].to_vec(),
                    from_head: names[..i].to_vec(),
                    there,
                }
            })?;
            out.within = names.to_vec();
            out.legs = vec![(head, names.to_vec())];
            out.place = Place::CustomRow(names.to_vec());
            return Ok(out);
        }
        "custom_goal" => {
            let mut list = &head.custom_rows;
            let mut found = Vec::new();
            for name in names {
                let Some(row) = list.iter().find(|r| &r.name == name) else {
                    return Err(Miss {
                        kind: MissKind::CustomRow,
                        holder: head,
                        walked: found.clone(),
                        from_head: found,
                        there: list.iter().map(|r| r.name.clone()).collect(),
                    });
                };
                found.push(name.clone());
                list = &row.rows;
            }
            out.within = names.to_vec();
            out.legs = vec![(head, names.to_vec())];
            out.place = Place::CustomRow(names.to_vec());
            return Ok(out);
        }
        // A kind the project defines: its fields by name, its rows by their
        // kind and address (`items.a`, `steps[0]`) — or, for a container, the card inside it.
        kind if project.is_own_kind(kind)
            && !(project.is_container(kind)
                && project.inner_named(&head.id, &steps[0]).is_some()) =>
        {
            let def = project.defs.cards().get(kind).expect("is_own_kind");
            (out.place, out.row) = walk_own(head, def, steps)?;
            let written = match &out.place {
                Place::OwnField(w) | Place::CustomRow(w) => w.clone(),
                _ => names.to_vec(),
            };
            out.within = written.clone();
            out.legs = vec![(head, written)];
            return Ok(out);
        }
        // A container: the card inside it by its name, and the rest of the path in that
        // card (a flow's node, a goal's card).
        kind if project.is_container(kind) => {
            return match project.inner_named(&head.id, &steps[0]) {
                Some(inner) => walk(inner, &steps[1..], project, through_refs).map(|mut w| {
                    w.entered += 1;
                    w
                }),
                None => Err(Miss {
                    kind: MissKind::Inner,
                    holder: head,
                    walked: Vec::new(),
                    from_head: Vec::new(),
                    there: project
                        .inner_of(&head.id)
                        .iter()
                        .map(|c| c.name.clone())
                        .collect(),
                }),
            };
        }
        _ => {}
    }
    let mut list: &[Field] = &head.fields;
    let mut from_head = Vec::new();
    for (index, name) in names.iter().enumerate() {
        let Some(next) = list.iter().find(|f| &f.name == name) else {
            return Err(Miss {
                kind: MissKind::Field,
                holder: out.holder,
                walked: out.within.clone(),
                from_head,
                there: list.iter().map(|f| f.name.clone()).collect(),
            });
        };
        from_head.push(name.clone());
        out.within.push(name.clone());
        out.fields.push(next);
        if let Some(leg) = out.legs.last_mut() {
            leg.1.push(name.clone());
        }
        out.place = Place::Field(next);
        out.row = Some(crate::kinds::FIELD_ROWS.to_string());
        if through_refs
            && index < names.len() - 1
            && next.is_ref()
            && let Some(target) = next
                .ref_schema_id
                .as_deref()
                .and_then(|id| project.card(id))
        {
            out.holder = target;
            out.within = Vec::new();
            out.legs.push((target, Vec::new()));
            list = &target.fields;
        } else {
            list = &next.children;
        }
    }
    Ok(out)
}

/// The names inside a card of a kind the project defines: a field (a group's fields inside
/// it), or a row — its kind of row, then the row by its address (`items.a`, or `items[0]`
/// for rows named by place), then the rows under it the same way. With the kind of row the
/// row found is one of (none for a field).
fn walk_own<'a>(
    head: &'a Card,
    def: &crate::kinds::Kind,
    steps: &[String],
) -> Result<(Place<'a>, Option<String>), Miss<'a>> {
    let miss = |kind: MissKind, walked: &[String], there: Vec<String>| Miss {
        kind,
        holder: head,
        walked: walked.to_vec(),
        from_head: walked.to_vec(),
        there,
    };
    let names: Vec<String> = steps
        .iter()
        .map(|s| crate::pathtext::plain(s).into_owned())
        .collect();
    let names = names.as_slice();
    // What the walk holds is written as a path writes it: a row by its place (`items[0]`) beside
    // rows by their names.
    let written: Vec<String> = steps
        .iter()
        .map(|s| crate::pathtext::canonical(s.trim()).into_owned())
        .collect();
    let first = &steps[0];
    // A row named by its place, written with it: `steps[0]`.
    let (first_name, first_index) = split_index(first);
    if let Some(field) = def.field(&first_name).filter(|_| first_index.is_none()) {
        let mut fields: &[crate::kinds::Field] = &field.fields;
        for (i, name) in names.iter().enumerate().skip(1) {
            match fields.iter().find(|f| &f.name == name) {
                Some(inner) => fields = &inner.fields,
                None => {
                    return Err(miss(
                        MissKind::Field,
                        &written[..i],
                        fields.iter().map(|f| f.name.clone()).collect(),
                    ));
                }
            }
        }
        return Ok((Place::OwnField(written.clone()), None));
    }
    let Some(row) = def.row(&first_name) else {
        return Err(miss(
            MissKind::Field,
            &[],
            def.fields
                .iter()
                .map(|f| f.name.clone())
                .chain(def.rows.iter().map(|r| r.name.clone()))
                .collect(),
        ));
    };
    let form = head.form.as_ref();
    let mut items: Vec<Value> = form
        .and_then(|f| f.get("rows"))
        .and_then(|r| r.get(&row.name))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut kind = row.clone();
    let mut lineage = vec![row.clone()];
    let mut at = 1;
    let mut index = first_index;
    loop {
        // The row by its address: the next name, or the index written with the kind.
        let item = match (index, kind.address) {
            (Some(i), _) => items.get(i).cloned(),
            (None, crate::kinds::Address::Index) => None,
            (None, address) => {
                let key = if address == crate::kinds::Address::Value {
                    "value"
                } else {
                    "name"
                };
                let wanted = names.get(at);
                at += 1;
                wanted.and_then(|w| {
                    items
                        .iter()
                        .find(|i| i["fields"][key].as_str() == Some(w.as_str()))
                        .cloned()
                })
            }
        };
        let Some(item) = item else {
            let key = if kind.address == crate::kinds::Address::Value {
                "value"
            } else {
                "name"
            };
            return Err(miss(
                MissKind::CustomRow,
                &written[..at.saturating_sub(1).min(names.len())],
                items
                    .iter()
                    .enumerate()
                    .map(|(i, r)| {
                        r["fields"][key]
                            .as_str()
                            .map(str::to_string)
                            .unwrap_or_else(|| format!("{}[{i}]", kind.name))
                    })
                    .collect(),
            ));
        };
        if at >= names.len() {
            return Ok((Place::CustomRow(written), Some(kind.name.clone())));
        }
        // A kind of row under it, and so on.
        let (next_name, next_index) = split_index(&steps[at]);
        let child = kind.children.iter().find(|c| c.name() == next_name);
        let Some(inner) = child.and_then(|c| match c {
            crate::kinds::Child::Row(r) => Some(r.clone()),
            crate::kinds::Child::Again { name, .. } => {
                lineage.iter().rev().find(|r| &r.name == name).cloned()
            }
        }) else {
            return Err(miss(
                MissKind::CustomRow,
                &written[..at],
                kind.children.iter().map(|c| c.name().to_string()).collect(),
            ));
        };
        items = item["rows"][&*next_name]
            .as_array()
            .cloned()
            .unwrap_or_default();
        lineage.push(inner.clone());
        kind = inner;
        index = next_index;
        at += 1;
    }
}

/// `steps[0]` → (`steps`, 0); a name without an index as it is (its quotes taken off).
fn split_index(written: &str) -> (std::borrow::Cow<'_, str>, Option<usize>) {
    let (name, rest) = crate::pathtext::step(written);
    if let Some(inner) = rest.strip_prefix('[').and_then(|r| r.strip_suffix(']'))
        && let Ok(i) = inner.trim().parse::<usize>()
    {
        return (name, Some(i));
    }
    (crate::pathtext::plain(written), None)
}

/// Whether the names a walk holds in a card are written as a path writes them: a data card's
/// (with their picks), a process node's (its steps by their place, `steps[0]`) and a card of a kind
/// the project defines (its rows by their place, `items[0]`) are; any other card's are the names as
/// they are.
pub(crate) fn holds_written(holder: &Card) -> bool {
    holder.kind == crate::kinds::INSTANCE
        || holder.kind == "process"
        || crate::kinds::builtin().get(&holder.kind).is_none()
}

/// Names a walk holds (the names walked, the place's), said as a path writes them.
pub(crate) fn said_names(holder: &Card, names: &[String]) -> Vec<String> {
    names
        .iter()
        .map(|n| {
            if holds_written(holder) {
                n.clone()
            } else {
                crate::pathtext::quote(n).into_owned()
            }
        })
        .collect()
}

/// What an MCP target is.
#[derive(Clone, Debug)]
pub(crate) enum Target<'a> {
    Card(&'a Card),
    /// A card ghost itself; a path that goes on past one is in the card it leads to.
    Ghost(&'a Ghost),
    /// A field, with the card that holds it and the names from that card to it.
    Field(&'a Card, &'a Field, Vec<String>),
    FormatRow(&'a Card, Vec<String>),
    CustomRow(&'a Card, Vec<String>),
    /// An enum's value, with the enum: its name.
    EnumValue(&'a Card, String),
    /// A process node's step, with the node: its place.
    Step(&'a Card, usize),
    /// A function or a type inside a working directory's file, with the file's card: its names
    /// from the file's top. Made of the file as it is now each time it is read, so a name is not
    /// known to be there until the file is read (`code`).
    Symbol(&'a Card, Vec<String>),
}

/// The card a path names, read from `base` (none: the project's context) — the directory's
/// refusal when it is not there (`SchemaDirectory.find`).
pub(crate) fn find_card<'a>(
    project: &Project<'a>,
    raw: &str,
    base: Option<&str>,
) -> Result<&'a Card, String> {
    match project.directory.find(raw, base) {
        Some(i) => Ok(project.nth(i)),
        None => {
            let names: Vec<String> = project
                .detail
                .schemas
                .iter()
                .map(|c| c.name.clone())
                .collect();
            Err(project.directory.find_refusal(raw, base, &names))
        }
    }
}

/// What a path names at its head, read from `base`: a card, or a card ghost as itself —
/// the directory's refusal when it is neither.
pub(crate) fn find_named<'a>(
    project: &Project<'a>,
    raw: &str,
    base: Option<&str>,
) -> Result<Named<'a>, String> {
    let found = match base {
        Some(b) => project.find_named(raw, b),
        None => project.directory.find_entry(raw, None).map(|e| match e {
            crate::directory::Entry::Card(i) => Named::Card(project.nth(i)),
            crate::directory::Entry::Ghost(g) => Named::Ghost(&project.detail.ghosts[g]),
        }),
    };
    found.ok_or_else(|| {
        let names: Vec<String> = project
            .detail
            .schemas
            .iter()
            .map(|c| c.name.clone())
            .collect();
        project.directory.find_refusal(raw, base, &names)
    })
}

/// A path whose head is a card ghost and goes on inside the card the ghost leads to:
/// the card's path and the ghost's, so what is answered can say it as it was written.
pub(crate) fn through_ghost(
    project: &Project,
    path: &str,
    base: Option<&str>,
) -> Option<(String, String)> {
    let written = WrittenPath::new(path)?;
    if written.names.is_empty() {
        return None;
    }
    let Ok(Named::Ghost(ghost)) = find_named(project, &written.card, base) else {
        return None;
    };
    let card = project.ghost_card(ghost)?;
    Some((project.full(card), project.full_ghost(ghost)))
}

/// An answer's paths (`path`, `parent`, `deleted`) said through the ghost they were written through:
/// the card's path at their head put back as the ghost's (`through_ghost`).
pub(crate) fn shown_through(value: Value, through: &Option<(String, String)>) -> Value {
    let Some((card, ghost)) = through else {
        return value;
    };
    fn go(value: Value, card: &str, ghost: &str) -> Value {
        match value {
            Value::Object(o) => Value::Object(
                o.into_iter()
                    .map(|(k, v)| {
                        let v = match (k.as_str(), v) {
                            ("path" | "parent" | "deleted", Value::String(s)) => {
                                Value::String(if s == card {
                                    ghost.to_string()
                                } else if let Some(rest) = s.strip_prefix(&format!("{card}.")) {
                                    format!("{ghost}.{rest}")
                                } else {
                                    s
                                })
                            }
                            (_, v) => go(v, card, ghost),
                        };
                        (k, v)
                    })
                    .collect(),
            ),
            Value::Array(a) => Value::Array(a.into_iter().map(|v| go(v, card, ghost)).collect()),
            other => other,
        }
    }
    go(value, card, ghost)
}

/// A target found, saying what is there at the level that failed (`MCPViews.locate`).
/// A reference is not stepped through.
pub(crate) fn locate<'a>(
    project: &Project<'a>,
    path: &str,
    base: Option<&str>,
) -> Result<Target<'a>, String> {
    if let Some(why) = crate::pathtext::unreadable(path) {
        return Err(why);
    }
    let written = WrittenPath::new(path).ok_or_else(|| text!("walk.path.empty"))?;
    let card = match find_named(project, &written.card, base)? {
        Named::Card(card) => card,
        Named::Ghost(ghost) if written.names.is_empty() => return Ok(Target::Ghost(ghost)),
        // Past a ghost, the path goes on in the card the ghost leads to.
        Named::Ghost(ghost) => project.ghost_card(ghost).ok_or_else(|| {
            let full = project.full_ghost(ghost);
            project
                .directory
                .entry_of(&ghost.id)
                .map(|e| project.directory.dead_end(e, &full))
                .unwrap_or_default()
        })?,
    };
    // Past a file's card, the names are of what is inside the file.
    if card.kind == "file" && !written.names.is_empty() {
        let names = written
            .names
            .iter()
            .map(|n| crate::pathtext::plain(n).into_owned())
            .collect();
        return Ok(Target::Symbol(card, names));
    }
    let full = |c: &Card, names: &[String]| {
        let mut out = project.full(c);
        for n in said_names(c, names) {
            out.push('.');
            out.push_str(&n);
        }
        out
    };
    match walk(card, &written.names, project, false) {
        Ok(w) => Ok(match w.place {
            Place::Card => Target::Card(w.head),
            Place::FormatRow(names) => Target::FormatRow(w.head, names),
            Place::Field(field) => Target::Field(w.holder, field, w.within),
            Place::CustomRow(names) | Place::OwnField(names) => Target::CustomRow(w.head, names),
            Place::EnumValue(name) => Target::EnumValue(w.head, name),
            Place::Step(i) => Target::Step(w.head, i),
        }),
        Err(miss) => {
            let at = full(miss.holder, &miss.walked);
            let there = |none: String| {
                if miss.there.is_empty() {
                    none
                } else {
                    let quoted: Vec<String> = miss
                        .there
                        .iter()
                        .map(|n| crate::pathtext::quote(n).into_owned())
                        .collect();
                    super::capped(&quoted, 12)
                }
            };
            Err(match miss.kind {
                MissKind::FormatRow => {
                    if miss.walked.is_empty()
                        || format_row(&miss.walked, &miss.holder.rows)
                            .is_some_and(|r| r.kind == "item")
                    {
                        text!(
                            "walk.miss.rows",
                            path,
                            at,
                            there = there(text!("walk.miss.noRows"))
                        )
                    } else {
                        text!("walk.miss.notItemRow", path, at)
                    }
                }
                MissKind::Inner => text!(
                    "walk.miss.inner",
                    path,
                    at,
                    there = there(text!("walk.miss.none"))
                ),
                MissKind::Field => text!(
                    "walk.miss.field",
                    path,
                    at,
                    there = there(text!("walk.miss.noFields"))
                ),
                MissKind::CustomRow => text!(
                    "walk.miss.customRow",
                    path,
                    at,
                    there = there(text!("walk.miss.noRows"))
                ),
                MissKind::Value => value_miss(&miss, &at, path),
                MissKind::EnumValue => text!(
                    "walk.miss.enumValue",
                    path,
                    at,
                    there = there(text!("walk.miss.noValues"))
                ),
                // The steps by their places, as a path writes them.
                MissKind::Step => text!(
                    "walk.miss.step",
                    path,
                    at,
                    count = miss.there.len(),
                    there = if miss.there.is_empty() {
                        text!("walk.miss.noSteps")
                    } else {
                        super::capped(&miss.there, 12)
                    }
                ),
                MissKind::Pick => text!(
                    "walk.miss.pick",
                    path,
                    hint = miss.there.first().cloned().unwrap_or_default()
                ),
            })
        }
    }
}

/// Why a data card's value is not there: its shape gone, an array passed with no element
/// picked, or a name its shape does not have.
pub(crate) fn value_miss(miss: &Miss, at: &str, path: &str) -> String {
    if miss.walked.is_empty() && miss.there.is_empty() {
        return text!("walk.value.noShape", path, at);
    }
    if miss.there.is_empty() {
        return text!("walk.value.isArray", path, at);
    }
    let there: Vec<String> = miss
        .there
        .iter()
        .map(|n| crate::pathtext::quote(n).into_owned())
        .collect();
    text!(
        "walk.value.missing",
        path,
        at,
        there = super::capped(&there, 12)
    )
}
