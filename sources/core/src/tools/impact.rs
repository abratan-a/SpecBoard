//! Which flows a change reaches, which notes and documents talk about what changed,
//! and what a key can be used to write (`Impact` in the Swift app's Model) — and, for an
//! item of a formatdoc, the documents and items connected to it (`RowLinks`).
//!
//! A flow says where it reads and writes with paths, and a change is a path too: the
//! two meet when one lies inside the other. Containment (`ref`) is followed, designation
//! (`pointsAt`) is not. A change is matched by the way it is written: from its holder it
//! is that holder's, as the type itself it is every holder's.

use super::facts::{self, Role, Use};
use super::walk::{self, Place};
use crate::export::Project;
use crate::export::paths::{WrittenPath, format_item, format_row, landing};
use crate::model::{Card, Field, FormatRow};
use crate::notetext::{self, Kind};
use std::collections::{HashMap, HashSet};

/// A place in a document: a card, and a dotted path inside it (`""` for all of it). The
/// path stops at a card's boundary — what a `ref` holds is addressed from its own card.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct Location {
    pub card: String,
    pub path: String,
}

fn at(card: &str, path: &str) -> Location {
    Location {
        card: card.to_string(),
        path: path.to_string(),
    }
}

/// A flow a change reaches, or a note or a document naming what changed (`Impact.Hit`).
#[derive(Clone, Debug)]
pub(crate) struct Hit<'a> {
    pub card: &'a Card,
    /// What it reads or writes — a note's `[[path]]` read like a use that reads, the
    /// item that names it as the note for a formatdoc's.
    pub used: Use,
    /// How the change reaches it: the use's own path, and the documents on the way.
    pub via: String,
    /// The change encloses what is used, rather than lying inside it.
    pub encloses: bool,
}

/// The card a hit is, in full — for a formatdoc, the item that names what changed, or
/// the card when a row at the top does (`Impact.written`).
pub(crate) fn written(hit: &Hit, project: &Project) -> String {
    let mut out = project.full(hit.card);
    if let Some(row) = &hit.used.note
        && hit.card.kind == "formatdoc"
        && row != facts::TOP_ROW
    {
        for name in crate::pathtext::names(row).iter().filter(|n| !n.is_empty()) {
            out = crate::pathtext::member(&out, name);
        }
    }
    out
}

/// Every location a path passes through, outermost first (`Impact.Index.chain`); none
/// when it does not resolve to a card or a field. `enter_final_ref`: whether a path
/// ending on a `ref` goes on into its target — reading the field reads what it holds;
/// changing the field does not change every card of that kind.
fn chain(
    project: &Project,
    raw: &str,
    base: Option<&str>,
    enter_final_ref: bool,
) -> Option<Vec<Location>> {
    let written = WrittenPath::new(raw)?;
    let head = project.nth(project.directory.find(&written.card, base)?);
    let w = walk::walk(head, &written.names, project, true).ok()?;
    if !matches!(w.place, Place::Card | Place::Field(_)) {
        return None;
    }
    let mut out = Vec::new();
    for (card, names) in &w.legs {
        out.push(at(&card.id, ""));
        for end in 0..names.len() {
            out.push(at(
                &card.id,
                &crate::pathtext::join_names(names[..=end].iter().map(String::as_str)),
            ));
        }
    }
    if enter_final_ref
        && let Place::Field(field) = w.place
        && field.is_ref()
        && let Some(target) = field
            .ref_schema_id
            .as_deref()
            .and_then(|id| project.card(id))
    {
        out.push(at(&target.id, ""));
    }
    Some(out)
}

/// Everything inside a location, through containment, each with the first way it was
/// reached (`Impact.Index.covered`).
fn covered(project: &Project, start: &Location, trail: &str) -> HashMap<Location, String> {
    let out: HashMap<Location, String> = HashMap::from([(start.clone(), trail.to_string())]);
    let Some(card) = project.card(&start.card) else {
        return out;
    };
    let mut fields: &[Field] = &card.fields;
    if !start.path.is_empty() {
        let mut list: &[Field] = &card.fields;
        let mut found = None;
        for step in crate::pathtext::names(&start.path)
            .iter()
            .filter(|s| !s.is_empty())
        {
            found = list.iter().find(|f| &f.name == step);
            list = found.map(|f| f.children.as_slice()).unwrap_or(&[]);
        }
        let Some(found) = found else {
            return out;
        };
        fields = &found.children;
    }
    struct Walker<'p, 'a> {
        project: &'p Project<'a>,
        entered: HashSet<String>,
        out: HashMap<Location, String>,
    }
    impl Walker<'_, '_> {
        fn walk(&mut self, fields: &[Field], card: &str, prefix: &str, trail: &str) {
            for field in fields {
                let path = if prefix.is_empty() {
                    crate::pathtext::quote(&field.name).into_owned()
                } else {
                    crate::pathtext::member(prefix, &field.name)
                };
                self.out
                    .entry(at(card, &path))
                    .or_insert_with(|| trail.to_string());
                if !field.children.is_empty() {
                    self.walk(&field.children, card, &path, trail);
                }
                if field.is_ref()
                    && let Some(id) = &field.ref_schema_id
                    && let Some(target) = self.project.card(id)
                    && self.entered.insert(id.clone())
                {
                    let into = format!("{trail} → {}", self.project.full(target));
                    self.out.entry(at(id, "")).or_insert_with(|| into.clone());
                    self.walk(&target.fields, id, "", &into);
                }
            }
        }
    }
    let mut walker = Walker {
        project,
        entered: HashSet::from([start.card.clone()]),
        out,
    };
    walker.walk(fields, &start.card, &start.path, trail);
    walker.out
}

/// The use's way ends where the change's way begins or passes: the last few places of
/// one are the first few of the other (`Impact.runsOn`).
fn runs_on(used: &[Location], wanted: &[Location]) -> bool {
    (1..=used.len().min(wanted.len())).any(|n| used[used.len() - n..] == wanted[..n])
}

/// `part` is a stretch of `route` (`Impact.isRun`).
fn is_run(part: &[Location], route: &[Location]) -> bool {
    if route.len() < part.len() || part.is_empty() {
        return false;
    }
    (0..=route.len() - part.len()).any(|i| route[i..i + part.len()] == *part)
}

/// The flows a change at `path` reaches, and the notes and documents naming what
/// changed (`Impact.of`); none when the path does not resolve. `path` is read from
/// `base`; each card's uses and references from its own section.
pub(crate) fn of<'a>(
    path: &str,
    project: &Project<'a>,
    base: Option<&str>,
) -> Option<Vec<Hit<'a>>> {
    let wanted = chain(project, path, base, false)?;
    let first = wanted.first()?.clone();
    let mut hits = Vec::new();
    for card in project
        .detail
        .schemas
        .iter()
        .filter(|c| c.kind == "action" || c.kind == "flow" || c.is_writing())
    {
        let home = project.section_of(card);
        let uses: Vec<Use> = if card.kind == "action" || card.kind == "flow" {
            facts::uses_in_effect(card)
        } else {
            // A formatdoc's: the item a mention is drawn from, said as the use's note.
            facts::links(card)
                .into_iter()
                .map(|l| Use {
                    path: l.path,
                    role: Role::Read,
                    note: matches!(card.kind.as_str(), "formatdoc" | "stackdoc").then_some(l.row),
                })
                .collect()
        };
        for used in uses {
            let Some(route) = chain(project, &used.path, Some(&home), true) else {
                continue;
            };
            // A use that ends on a `ref` reads what it holds; said in the trail.
            let mut start = used.path.clone();
            if route.len() > 1
                && let Some(last) = route.last()
                && last.path.is_empty()
                && let Some(held) = project.card(&last.card)
            {
                start += &format!(" → {}", project.full(held));
            }
            if runs_on(&route, &wanted) {
                hits.push(Hit {
                    card,
                    used,
                    via: start,
                    encloses: false,
                });
                continue;
            }
            let last = route.last().expect("a route has a place");
            if let Some(trail) = covered(project, last, &start).remove(&first) {
                hits.push(Hit {
                    card,
                    used,
                    via: trail,
                    encloses: false,
                });
                continue;
            }
            if is_run(&wanted, &route) {
                let via = used.path.clone();
                hits.push(Hit {
                    card,
                    used,
                    via,
                    encloses: true,
                });
            }
        }
    }
    Some(hits)
}

/// What a key is used for (`Impact.KeyUse`): which flows sign with it and what they
/// write, which check with it. The documents a key signs or checks were their payloads'
/// keys; they are lines of the project's own now, read as any line is
/// (`referencedBy`), not here.
#[derive(Default)]
pub(crate) struct KeyUse<'a> {
    pub signers: Vec<(&'a Card, Vec<String>)>,
    pub checkers: Vec<&'a Card>,
}

impl KeyUse<'_> {
    pub fn is_empty(&self) -> bool {
        self.signers.is_empty() && self.checkers.is_empty()
    }
}

/// What the key at `path` is used for; none when the path does not resolve
/// (`Impact.keyUse`). A key is matched by the way to it: from its holder, the keys on
/// that route; to what holds it, every key under it; as its type, every key of it.
pub(crate) fn key_use<'a>(
    path: &str,
    project: &Project<'a>,
    base: Option<&str>,
) -> Option<KeyUse<'a>> {
    let wanted = chain(project, path, base, false)?;
    if wanted.is_empty() {
        return None;
    }
    let matches = |key: &str, owner: &Card| {
        chain(project, key, Some(&project.section_of(owner)), false)
            .is_some_and(|route| is_run(&wanted, &route))
    };
    let mut out = KeyUse::default();
    // What its node cards' key lines name: to sign (`use: sign`) — with
    // what its rows write —, and to check.
    for card in &project.detail.schemas {
        if card.kind == "flow" {
            if facts::keys_used(project, card, true)
                .iter()
                .any(|k| matches(k, card))
            {
                out.signers.push((card, facts::writes_of(project, card)));
            }
            if facts::keys_used(project, card, false)
                .iter()
                .any(|k| matches(k, card))
            {
                out.checkers.push(card);
            }
        }
    }
    Some(out)
}

// MARK: - What is connected to an item

/// A row of a formatdoc, or a note or a freedoc taken whole, as the list of what is
/// connected to an item shows it (`RowLink`).
#[derive(Clone, Debug)]
pub(crate) struct RowLink<'a> {
    pub note: &'a Card,
    /// The item's names in the formatdoc; empty for a card taken whole.
    pub path: Vec<String>,
    /// As the project writes it, from the top: `Docs:Spec.auth.login`.
    pub written: String,
    pub title: String,
    /// The first lines of what it says.
    pub head: String,
}

/// The first lines of a text, as Swift splits it at `"\n"` characters (`\r\n` is one
/// character, and not one of them), empty lines left out (`FormatRows.head`).
fn head(body: &str, lines: usize) -> String {
    let mut parts: Vec<&str> = Vec::new();
    let mut start = 0;
    let bytes = body.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'\n' && !(i > 0 && bytes[i - 1] == b'\r') {
            parts.push(&body[start..i]);
            start = i + 1;
        }
    }
    parts.push(&body[start..]);
    parts
        .into_iter()
        .filter(|p| !p.is_empty())
        .take(lines)
        .collect::<Vec<_>>()
        .join("\n")
}

/// What an item says, for a glance: its text rows' first lines (`FormatRows.head(of:)`).
fn item_head(path: &[String], rows: &[FormatRow]) -> String {
    let texts: Vec<&str> = walk::format_children(path, rows)
        .iter()
        .filter(|r| r.kind == "text")
        .map(|r| r.body.as_str())
        .collect();
    head(&texts.join("\n"), 3)
}

/// What a reader sees for a row: an item's heading, or its name (`FormatRow.title`).
fn title(row: &FormatRow) -> String {
    if row.kind == "item" && !row.heading.is_empty() {
        row.heading.clone()
    } else {
        row.name.clone()
    }
}

/// What an item's rows point at that is a document card or an item, and the items — or
/// notes and freedocs — whose text points at it or at something inside it
/// (`RowLinks.connected`).
pub(crate) fn connected<'a>(
    project: &Project<'a>,
    note: &'a Card,
    path: &[String],
) -> (Vec<RowLink<'a>>, Vec<RowLink<'a>>) {
    if format_row(path, &note.rows).is_none() {
        return (Vec::new(), Vec::new());
    }
    let link = |holder: &'a Card, names: &[String]| -> Option<RowLink<'a>> {
        let mut written = project.full(holder);
        for n in names {
            written.push('.');
            written.push_str(n);
        }
        if names.is_empty() {
            let text = if holder.kind == "formatdoc" {
                crate::export::rows_markdown(&holder.rows, false, &mut |r| {
                    (r.kind == "text").then(|| r.body.clone())
                })
            } else if holder.kind == "stackdoc" {
                holder.open_version().map(|v| v.body).unwrap_or_default()
            } else {
                holder.body.clone()
            };
            return Some(RowLink {
                note: holder,
                path: Vec::new(),
                written,
                title: if holder.label.is_empty() {
                    holder.name.clone()
                } else {
                    holder.label.clone()
                },
                head: head(&text, 3),
            });
        }
        let found = format_row(names, &holder.rows)?;
        Some(RowLink {
            note: holder,
            path: names.to_vec(),
            written,
            title: title(found),
            head: item_head(names, &holder.rows),
        })
    };
    // Where a `[[path]]` written in `holder` lands, when that is a document card or an item.
    let target = |raw: &str, holder: &Card| -> Option<(&'a Card, Vec<String>)> {
        let (landed, row) = landing(raw, &project.section_of(holder), project)?;
        if !landed.is_writing() {
            return None;
        }
        let names: Vec<String> = crate::pathtext::names(&row)
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect();
        if landed.kind != "formatdoc" || names.is_empty() {
            return Some((landed, Vec::new()));
        }
        let (found, _) = walk::deepest(&names, &landed.rows);
        Some((landed, format_item(&found, &landed.rows)))
    };

    let mut mentions = Vec::new();
    let mut seen = HashSet::new();
    for (_, _, text) in facts::said(path, &note.rows) {
        for r in notetext::refs(&text)
            .into_iter()
            .filter(|r| r.kind == Kind::Place)
        {
            let Some((holder, names)) = target(&r.path, note) else {
                continue;
            };
            if holder.id == note.id && names == path {
                continue;
            }
            if let Some(found) = link(holder, &names)
                && seen.insert(found.written.clone())
            {
                mentions.push(found);
            }
        }
    }

    let points = |text: &str, holder: &Card| {
        notetext::refs(text).into_iter().any(|r| {
            r.kind == Kind::Place
                && target(&r.path, holder)
                    .is_some_and(|(landed, names)| landed.id == note.id && names.starts_with(path))
        })
    };
    let mut mentioned_by = Vec::new();
    let mut cited = HashSet::new();
    for other in project.detail.schemas.iter().filter(|c| c.is_writing()) {
        if other.kind != "formatdoc" {
            let text = if other.kind == "stackdoc" {
                other
                    .versions()
                    .into_iter()
                    .map(|v| v.body)
                    .collect::<Vec<_>>()
                    .join("\n")
            } else {
                other.body.clone()
            };
            if other.id != note.id
                && points(&text, other)
                && let Some(found) = link(other, &[])
                && cited.insert(found.written.clone())
            {
                mentioned_by.push(found);
            }
            continue;
        }
        facts::walk_format(&other.rows, &mut |row, names, _| {
            if row.kind != "item" || (other.id == note.id && names.starts_with(path)) {
                return;
            }
            if !facts::said(names, &other.rows)
                .iter()
                .any(|(_, _, text)| points(text, other))
            {
                return;
            }
            if let Some(found) = link(other, names)
                && cited.insert(found.written.clone())
            {
                mentioned_by.push(found);
            }
        });
    }
    (mentions, mentioned_by)
}
