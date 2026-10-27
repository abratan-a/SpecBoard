//! What `project_read`, the check of a tree (`inspection_list`) and `card_read`'s `as: impact` and
//! `as: exists` answer with (`MCPViews.summary`, and the operations `get_impact` and
//! `resolve_paths` in the Swift app's MCP) — before the surface heads them with the
//! tree they are of (`Seen::headed`) and puts the project's settings beside.

use super::Seen;
use super::check::{self, DeclaredPath};
use super::facts;
use super::impact::{self, RowLink};
use super::walk::{Target, locate};
use crate::export::paths::format_item;
use crate::text::{Keyed, List};
use serde_json::{Map, Value};

/// The section a call reads names without one from: the one `section` names, or none
/// when it names none (`MCPTools.base`).
pub(crate) fn base(seen: &Seen, section: Option<&str>) -> Result<Option<String>, String> {
    let Some(name) = section else {
        return Ok(None);
    };
    let project = seen.project();
    if let Some(found) = project
        .directory
        .section_named(check::trim_spaces(name), None)
    {
        return Ok(Some(found));
    }
    if seen.detail.sections.is_empty() {
        if crate::pathtext::same(name.trim(), "main") {
            return Ok(Some(project.directory.home.clone()));
        }
        return Err(text!("summary.section.noneButMain", name));
    }
    Err(project
        .directory
        .section_problem(name, &project.directory.home))
}

/// One finding of the check, as its group says it.
pub(crate) type Row = Map<String, Value>;

/// The paths that do not resolve, as rows; on a goal's branch (`planned`, the goal's path) a line to
/// code not there yet says it is that goal's plan.
fn broken_rows(broken: &[DeclaredPath], planned: Option<&str>) -> Vec<Row> {
    broken
        .iter()
        .map(|entry| {
            let mut row = Map::new();
            row.insert("at".into(), entry.at.clone().into());
            row.insert("via".into(), entry.via.clone().into());
            row.insert("path".into(), entry.path.clone().into());
            if let Some(hint) = &entry.hint {
                row.insert("hint".into(), hint.clone().into());
            }
            if let Some(goal) = planned.filter(|_| entry.to_code) {
                row.insert("planned".into(), goal.into());
            }
            row
        })
        .collect()
}

fn tag_rows(tags: &[(String, usize)]) -> Value {
    Value::Array(
        tags.iter()
            .map(|(tag, count)| {
                let mut row = Map::new();
                row.insert("tag".into(), tag.clone().into());
                row.insert("count".into(), (*count).into());
                Value::Object(row)
            })
            .collect(),
    )
}

/// The kinds `counts` lists: the built-in kinds of main's cards (`action`
/// and `custom_goal` were the Swift app's, and are no longer).
const COUNTED: [&str; 14] = [
    "entity",
    "value",
    "enum",
    "note",
    "flow",
    "store",
    "freedoc",
    "formatdoc",
    "stackdoc",
    "instance",
    "table",
    "svg",
    "html",
    "frame",
];

/// The project itself, as `project_read` answers it before the settings are put beside:
/// its description, how many
/// cards of each kind main has, its tags and what is directly under each root. What is wrong in a
/// tree is `check_rows`'.
pub(crate) fn project_view(seen: &Seen) -> Value {
    let project = seen.project();
    let detail = &seen.detail;
    let mut out = Map::new();
    out.insert("project".into(), detail.project.name.clone().into());
    if !detail.project.description.is_empty() {
        out.insert(
            "description".into(),
            detail.project.description.clone().into(),
        );
    }
    // The built-in kinds, then any other main's cards are of (a kind of the project's own),
    // where they are first.
    let mut counts = Map::new();
    let others = detail
        .schemas
        .iter()
        .map(|c| c.kind.as_str())
        .filter(|k| !COUNTED.contains(k));
    for kind in COUNTED.into_iter().chain(others) {
        if counts.contains_key(kind) {
            continue;
        }
        counts.insert(
            kind.into(),
            detail
                .schemas
                .iter()
                .filter(|c| c.kind == kind)
                .count()
                .into(),
        );
    }
    // The card ghosts apart from the cards, when there are any.
    if !detail.ghosts.is_empty() {
        counts.insert(crate::detail::GHOST.into(), detail.ghosts.len().into());
    }
    out.insert("counts".into(), Value::Object(counts));
    let tags = check::project_tags(&detail.schemas);
    if !tags.is_empty() {
        out.insert("tags".into(), tag_rows(&tags));
    }
    let groups = &detail.project.tag_groups;
    if !groups.is_empty() {
        out.insert(
            "tagGroups".into(),
            Value::Array(
                groups
                    .iter()
                    .map(|g| {
                        let mut row = Map::new();
                        row.insert("group".into(), g.name.clone().into());
                        row.insert("tags".into(), g.tags.clone().into());
                        if g.exclusive {
                            row.insert("exclusive".into(), true.into());
                        }
                        Value::Object(row)
                    })
                    .collect(),
            ),
        );
        // Declared in a group, on nothing yet.
        let mut declared: Vec<String> = groups
            .iter()
            .flat_map(|g| g.tags.iter())
            .filter(|t| !tags.iter().any(|(u, _)| u == *t) && !groups.iter().any(|g| &g.name == *t))
            .cloned()
            .collect();
        declared.sort();
        declared.dedup();
        if !declared.is_empty() {
            out.insert("declaredTags".into(), declared.into());
        }
    }
    // What is directly under each root (the project's, read here and written with
    // project_update's project_tree; the kinds are kind_*'s).
    out.insert("projectTree".into(), project.defs.tree.json());
    Value::Object(out)
}

/// What is wrong in the tree read, each finding a row of its group (`inspection` opens them into
/// items): only what was found — nothing said is nothing wrong. The overlaps on the sections'
/// diagrams (`grid::overlaps`) are the caller's, which reads the layout, and so is the goal whose
/// branch the tree is (`planned`), which reads the board.
pub(crate) fn check_rows(
    seen: &Seen,
    overlaps: Vec<Row>,
    planned: Option<&str>,
) -> Vec<(&'static str, Row)> {
    let project = seen.project();
    let detail = &seen.detail;
    let mut out: Vec<(&'static str, Row)> = Vec::new();
    let mut put = |group: &'static str, rows: Vec<Row>| {
        out.extend(rows.into_iter().map(|row| (group, row)));
    };
    put(
        "brokenPaths",
        broken_rows(&check::broken_paths(&project), planned),
    );
    // A note put in a working directory's folder that is not in what was read here.
    put(
        "missingFolders",
        detail
            .schemas
            .iter()
            .filter(|c| c.inside.is_none())
            .filter_map(|c| {
                let section = project.section_of(c);
                detail
                    .sections
                    .iter()
                    .find(|s| s.id == section && s.missing)?;
                let mut row = Map::new();
                row.insert("at".into(), project.full(c).into());
                row.insert(
                    "path".into(),
                    project.directory.full_section(&section).into(),
                );
                Some(row)
            })
            .collect(),
    );
    put(
        "ambiguousPointers",
        check::ambiguous_pointers(&project)
            .into_iter()
            .map(|(at, count)| {
                let mut row = Map::new();
                row.insert("at".into(), at.into());
                row.insert("candidates".into(), count.into());
                row
            })
            .collect(),
    );
    put(
        "unresolvedPointers",
        check::unresolved_pointers(&project)
            .into_iter()
            .map(|(at, by)| {
                let mut row = Map::new();
                row.insert("at".into(), at.into());
                row.insert("by".into(), by.into());
                row
            })
            .collect(),
    );
    // A problem kept by key: an item is told apart by what it is, in every language (`inspection`).
    let said = |at: String, problem: Keyed| {
        let mut row = Map::new();
        row.insert("at".into(), at.into());
        row.insert("problem".into(), problem.json());
        row
    };
    // A card's lines that do not fit their kind of line: a definition
    // changed under them, or the card they reach became another kind.
    put(
        "lineProblems",
        detail
            .schemas
            .iter()
            .chain(detail.inner.iter())
            .flat_map(|c| {
                super::lines::misfits(&project, c)
                    .into_iter()
                    .map(|problem| said(project.full(c), problem))
            })
            .collect(),
    );
    // A card in a section whose kind does not hold it (a kind of section of the project's own
    // changed under it).
    put(
        "sectionProblems",
        detail
            .schemas
            .iter()
            // A card inside a container is held by the container's kind (`cardProblems`).
            .filter(|c| c.inside.is_none())
            .filter_map(|c| {
                let section = project.section_kind_of(c)?;
                (!project.defs.holds(section, &c.kind)).then(|| {
                    said(
                        project.full(c),
                        keyed!(
                            "summary.sectionProblem",
                            section = project.defs.section_label(section),
                            kind = c.kind,
                            holds = List::of(
                                project
                                    .defs
                                    .section(section)
                                    .map(|d| d.holds.clone())
                                    .unwrap_or_default()
                            )
                        ),
                    )
                })
            })
            .collect(),
    );
    // A card of a kind the project defines that no longer fits it, and a card inside a container
    // of the project's own kind that no longer holds it: its
    // definition changed under it. What the definition no longer has is kept and said apart
    // (`strayFields`).
    let (misfit, strays) = own_kind_problems(&project);
    put("cardProblems", misfit);
    put("strayFields", strays);
    // Each at its flow, so a reader opens the flow without reading the sentence.
    put(
        "flowProblems",
        detail
            .schemas
            .iter()
            .filter(|c| c.kind == "flow")
            .flat_map(|c| {
                let at = project.full(c);
                check::flow_warnings(c, &project)
                    .into_iter()
                    // Their own groups say them, at the begin, the row or the end.
                    .filter(|problem| {
                        !matches!(
                            problem.key(),
                            "check.flow.noPartner" | "check.flow.unlinkedPartner"
                        )
                    })
                    .map(move |problem| said(at.clone(), problem))
            })
            .collect(),
    );
    put(
        "peerMismatches",
        check::peer_mismatches(&project)
            .into_iter()
            .map(|m| {
                let mut row = Map::new();
                row.insert("at".into(), m.at.into());
                row.insert("peer".into(), m.peer.into());
                if !m.carries.is_empty() {
                    row.insert("carries".into(), m.carries.into());
                }
                row.insert("problem".into(), m.problem.json());
                row
            })
            .collect(),
    );
    // The gaps the sequence charts draw: each at the place it is, where pressing it shows the gap.
    put(
        "missingPartners",
        crate::export::charts::partner_gaps(&project)
            .into_iter()
            .map(|gap| {
                let mut row = Map::new();
                row.insert("at".into(), gap.at.into());
                if !gap.callers.is_empty() {
                    row.insert("callers".into(), gap.callers.into());
                }
                if let Some(op) = gap.op {
                    row.insert("op".into(), op.into());
                }
                row
            })
            .collect(),
    );
    put(
        "missingStores",
        crate::export::charts::store_gaps(&project)
            .into_iter()
            .map(|gap| {
                let mut row = Map::new();
                row.insert("at".into(), gap.at.into());
                row.insert("shapes".into(), gap.shapes.into());
                if !gap.candidates.is_empty() {
                    row.insert(
                        "candidates".into(),
                        Value::Object(
                            gap.candidates
                                .into_iter()
                                .map(|(shape, stores)| (shape, stores.into()))
                                .collect(),
                        ),
                    );
                }
                row.insert("op".into(), gap.op.into());
                row.insert("data".into(), gap.data.into());
                row
            })
            .collect(),
    );
    // The partners written as a program that has the name of a flow or an actor: at the start, the
    // row or the end that writes it.
    put(
        "unlinkedPartners",
        check::unlinked_partners(&project)
            .into_iter()
            .map(|partner| {
                let mut row = Map::new();
                row.insert("at".into(), partner.at.into());
                row.insert("op".into(), partner.op.into());
                row.insert("program".into(), partner.program.into());
                if !partner.flows.is_empty() {
                    row.insert("flows".into(), partner.flows.into());
                }
                if let Some(actor) = partner.actor {
                    row.insert("actor".into(), actor.into());
                }
                row
            })
            .collect(),
    );
    put(
        "tagGroupViolations",
        check::tag_group_violations(&project)
            .into_iter()
            .map(|(group, at, count)| {
                let mut row = Map::new();
                row.insert("at".into(), at.into());
                row.insert("tagGroup".into(), group.into());
                row.insert("count".into(), count.into());
                row
            })
            .collect(),
    );
    // A card left in a free section, each on its own: one inside a container is the container's
    // to place.
    put(
        "freeCards",
        detail
            .schemas
            .iter()
            .filter(|c| c.inside.is_none() && project.section_kind_of(c) == Some("free"))
            .map(|c| {
                let mut row = Map::new();
                row.insert("at".into(), project.full(c).into());
                row.insert(
                    "section".into(),
                    project
                        .directory
                        .full_section(&project.section_of(c))
                        .into(),
                );
                row
            })
            .collect(),
    );
    // A store with ghosts in two actors or more: each sequence diagram takes the one nearer the
    // flow, which a reader may not expect.
    let actors = crate::export::actors::Actors::new(&project);
    put(
        "storeActors",
        detail
            .schemas
            .iter()
            .filter(|c| c.kind == "store")
            .filter_map(|c| {
                let holders = actors.store_holders(c);
                (holders.len() > 1).then(|| {
                    let mut row = Map::new();
                    row.insert("at".into(), project.full(c).into());
                    row.insert(
                        "actors".into(),
                        holders
                            .iter()
                            .map(|a| Value::from(project.directory.full_section(a)))
                            .collect(),
                    );
                    row
                })
            })
            .collect(),
    );
    put(
        "unreferenced",
        check::unreferenced(&project)
            .into_iter()
            .map(|at| {
                let mut row = Map::new();
                row.insert("at".into(), at.into());
                row
            })
            .collect(),
    );
    // The patterns data is held to that cannot be read, so hold nothing.
    put(
        "uncheckedPatterns",
        super::instances::unread_patterns(&project)
            .into_iter()
            .map(|(at, regex, why)| {
                let mut row = Map::new();
                row.insert("at".into(), at.into());
                row.insert("regex".into(), regex.into());
                row.insert(
                    "problem".into(),
                    keyed!("summary.uncheckedPattern", why).json(),
                );
                row
            })
            .collect(),
    );
    put("overlaps", overlaps);
    out
}

fn row_links(links: &[RowLink]) -> Value {
    Value::Array(
        links
            .iter()
            .map(|link| {
                let mut row = Map::new();
                row.insert("path".into(), link.written.clone().into());
                row.insert("heading".into(), link.title.clone().into());
                if !link.head.is_empty() {
                    row.insert("head".into(), link.head.clone().into());
                }
                Value::Object(row)
            })
            .collect(),
    )
}

/// What a change at `path` reaches (the operation `get_impact`): the flows whose rows
/// read or write inside it or around it, the notes and documents naming it, and — for
/// a key — who signs and checks with it. For an item of a formatdoc: what its rows
/// name, and the documents and items connected to it.
pub(crate) fn impact_view(seen: &Seen, path: &str, base: Option<&str>) -> Result<Value, String> {
    let project = seen.project();
    // A function or a type in a working directory's file: what implements it and what calls it.
    if let Ok(Target::Symbol(file, names)) = locate(&project, path, base) {
        return super::code::symbol_impact(&project, &seen.detail.project.name, file, &names);
    }
    let mut out = Map::new();
    out.insert("project".into(), seen.detail.project.name.clone().into());
    if let Ok(Target::FormatRow(note, row_names)) = locate(&project, path, base) {
        let names = format_item(&row_names, &note.rows);
        let row = if names.is_empty() {
            facts::TOP_ROW.to_string()
        } else {
            crate::pathtext::join_names(names.iter().map(String::as_str))
        };
        let mut named: Vec<String> = Vec::new();
        for link in facts::links(note).into_iter().filter(|l| l.row == row) {
            if !named.contains(&link.path) {
                named.push(link.path);
            }
        }
        let (mentions, mentioned_by) = if names.is_empty() {
            (Vec::new(), Vec::new())
        } else {
            impact::connected(&project, note, &names)
        };
        let mut written = project.full(note);
        for n in &names {
            written = crate::pathtext::member(&written, n);
        }
        out.insert("path".into(), written.into());
        out.insert("names".into(), named.into());
        let mut items = Map::new();
        items.insert("mentions".into(), row_links(&mentions));
        items.insert("mentionedBy".into(), row_links(&mentioned_by));
        out.insert("items".into(), Value::Object(items));
        return Ok(Value::Object(out));
    }
    let Some(hits) = impact::of(path, &project, base) else {
        let hit = check::resolve_path(&project, path, None, base);
        return Err(text!(
            "summary.impact.notFound",
            path,
            hint = hit.hint.unwrap_or_default()
        ));
    };
    let (notes, flows): (Vec<_>, Vec<_>) = hits.iter().partition(|h| h.card.is_writing());
    out.insert("path".into(), path.into());
    out.insert(
        "flows".into(),
        Value::Array(
            flows
                .iter()
                .map(|hit| {
                    let mut row = Map::new();
                    row.insert("flow".into(), project.full(hit.card).into());
                    row.insert("role".into(), hit.used.role.name().into());
                    row.insert("uses".into(), hit.used.path.clone().into());
                    if hit.encloses {
                        row.insert("encloses".into(), true.into());
                    } else {
                        row.insert("via".into(), hit.via.clone().into());
                    }
                    if let Some(note) = &hit.used.note {
                        row.insert("note".into(), note.clone().into());
                    }
                    Value::Object(row)
                })
                .collect(),
        ),
    );
    if !notes.is_empty() {
        out.insert(
            "notes".into(),
            Value::Array(
                notes
                    .iter()
                    .map(|hit| {
                        let mut row = Map::new();
                        row.insert("note".into(), impact::written(hit, &project).into());
                        row.insert("mentions".into(), hit.used.path.clone().into());
                        if hit.encloses {
                            row.insert("encloses".into(), true.into());
                        }
                        Value::Object(row)
                    })
                    .collect(),
            ),
        );
    }
    // The files to change with it: its
    // own — where its lines of the kind that reaches files go — those of the cards that embed it,
    // and those of the flows that read or write it.
    let card = match locate(&project, path, base) {
        Ok(Target::Card(card))
        | Ok(Target::Field(card, _, _))
        | Ok(Target::FormatRow(card, _))
        | Ok(Target::CustomRow(card, _)) => Some(card),
        _ => None,
    };
    if let Some(card) = card {
        let mut files: Vec<Value> = Vec::new();
        let mut add = |of: &crate::model::Card| {
            for line in of.connections.iter().filter(|l| l.line == "implementedBy") {
                let Some(to) = line.to.as_deref() else {
                    continue;
                };
                let written = project.file_written(to).unwrap_or_else(|| to.to_string());
                let at = line.at.as_deref().filter(|a| !a.is_empty());
                if !files.iter().any(|f| {
                    f["file"] == written.as_str() && f.get("symbol").and_then(Value::as_str) == at
                }) {
                    let mut row = Map::new();
                    row.insert("file".into(), written.into());
                    row.insert("for".into(), project.full(of).into());
                    // Inside it: the function or type, and its lines now.
                    if let Some(at) = at {
                        row.insert("symbol".into(), at.into());
                        if let Some(lines) = super::code::landing_lines(&project, to, at) {
                            row.insert("lines".into(), lines);
                        }
                    }
                    files.push(Value::Object(row));
                }
            }
        };
        add(card);
        for other in project
            .detail
            .schemas
            .iter()
            .chain(project.detail.inner.iter())
        {
            if other.id != card.id
                && other
                    .connections
                    .iter()
                    .any(|l| l.line == "embed" && l.to.as_deref() == Some(card.id.as_str()))
            {
                add(other);
            }
        }
        for hit in &flows {
            add(hit.card);
        }
        if !files.is_empty() {
            out.insert("files".into(), Value::Array(files));
        }
    }
    if let Some(keys) = impact::key_use(path, &project, base)
        && !keys.is_empty()
    {
        let mut row = Map::new();
        if !keys.signers.is_empty() {
            row.insert(
                "signers".into(),
                Value::Array(
                    keys.signers
                        .iter()
                        .map(|(flow, writes)| {
                            let mut entry = Map::new();
                            entry.insert("flow".into(), project.full(flow).into());
                            entry.insert("writes".into(), writes.clone().into());
                            Value::Object(entry)
                        })
                        .collect(),
                ),
            );
        }
        if !keys.checkers.is_empty() {
            row.insert(
                "checkers".into(),
                keys.checkers
                    .iter()
                    .map(|c| project.full(c))
                    .collect::<Vec<_>>()
                    .into(),
            );
        }
        out.insert("keys".into(), Value::Object(row));
    }
    if flows.is_empty() && notes.is_empty() && !out.contains_key("keys") {
        let count = seen
            .detail
            .schemas
            .iter()
            .filter(|c| c.kind == "flow")
            .count();
        out.insert(
            "note".into(),
            if count == 0 {
                text!("summary.impact.noFlows")
            } else {
                text!("summary.impact.noFlowRows")
            }
            .into(),
        );
    }
    Ok(Value::Object(out))
}

/// Whether each path still resolves, with a hint at what is there where one does not
/// (the operation `resolve_paths`): at most 5000, refused rather than cut.
pub(crate) fn exists_view(
    seen: &Seen,
    paths: &[String],
    base: Option<&str>,
    unresolved_only: bool,
) -> Result<Value, String> {
    if paths.len() > 5000 {
        return Err(text!("summary.exists.tooMany", count = paths.len()));
    }
    let project = seen.project();
    let results: Vec<(String, check::PathHit)> = paths
        .iter()
        .map(|raw| {
            let path = check::trim_spaces(raw).to_string();
            let hit = check::resolve_place(&project, &path, None, base);
            (path, hit)
        })
        .collect();
    let unresolved = results.iter().filter(|(_, h)| !h.ok).count();
    let mut out = Map::new();
    out.insert("checked".into(), results.len().into());
    out.insert("unresolved".into(), unresolved.into());
    out.insert(
        "results".into(),
        Value::Array(
            results
                .into_iter()
                .filter(|(_, h)| !unresolved_only || !h.ok)
                .map(|(path, hit)| {
                    let mut row = Map::new();
                    row.insert("path".into(), path.into());
                    row.insert("resolved".into(), hit.ok.into());
                    if let Some(kind) = hit.kind {
                        let kind = if kind == "schema" {
                            "card".to_string()
                        } else {
                            kind
                        };
                        row.insert("kind".into(), kind.into());
                    }
                    if let Some(t) = hit.type_name {
                        row.insert("type".into(), t.into());
                    }
                    if let Some(hint) = hit.hint {
                        row.insert("hint".into(), hint.into());
                    }
                    Value::Object(row)
                })
                .collect(),
        ),
    );
    Ok(Value::Object(out))
}

/// The cards of the project's own kinds that do not fit their definitions now (`cardProblems`:
/// `[{at, problem}]`), and those that hold what their definitions no longer have
/// (`strayFields`: `[{at, fields}]`, where in the card each is).
fn own_kind_problems(project: &crate::export::Project) -> (Vec<Row>, Vec<Row>) {
    let kinds = project.defs.cards();
    let mut misfit = Vec::new();
    let mut strays = Vec::new();
    let row = |at: String, problem: Keyed| {
        let mut row = Map::new();
        row.insert("at".into(), at.into());
        row.insert("problem".into(), problem.json());
        row
    };
    for card in project.detail.schemas.iter() {
        let at = project.full(card);
        // A data card: its shape gone, values its fields do not take, required fields with no
        // value; and the values whose fields its shape has lost.
        if card.kind == crate::kinds::INSTANCE {
            for (inside, problem) in super::instances::problems(project, card) {
                let place = if inside.is_empty() {
                    at.clone()
                } else {
                    format!("{at}.{inside}")
                };
                misfit.push(row(place, problem));
            }
            let left: Vec<Value> = super::instances::strays(project, card)
                .into_iter()
                .map(|(where_, _)| Value::from(where_))
                .collect();
            if !left.is_empty() {
                let mut one = Map::new();
                one.insert("at".into(), at.into());
                one.insert("fields".into(), Value::Array(left));
                strays.push(one);
            }
            continue;
        }
        // Inside a container of the project's own kind: one it holds.
        if let Some(container) = project.container_of(card)
            && kinds.is_own(&container.kind)
            && let Err(why) = super::containers::require_holds(
                kinds,
                &container.kind,
                &project.full(container),
                &card.kind,
            )
        {
            misfit.push(row(at.clone(), why));
        }
        let Some(def) = kinds
            .get(&card.kind)
            .filter(|_| kinds.keeps_strays(&card.kind))
        else {
            continue;
        };
        let form = card.kept_form();
        // The board's goal and task: what their definitions no longer have alone; whether they fit is the writing's to say.
        if def.builtin {
            let left: Vec<Value> = crate::kinds::strays(def, &form)
                .into_iter()
                .map(|(where_, _)| Value::from(where_))
                .collect();
            if !left.is_empty() {
                let mut one = Map::new();
                one.insert("at".into(), at.into());
                one.insert("fields".into(), Value::Array(left));
                strays.push(one);
            }
            continue;
        }
        let mut doc = Map::new();
        doc.insert("id".into(), card.id.clone().into());
        doc.insert("kind".into(), card.kind.clone().into());
        doc.insert("name".into(), card.name.clone().into());
        doc.insert(
            "section".into(),
            card.section_id
                .clone()
                .unwrap_or_else(|| "-".to_string())
                .into(),
        );
        doc.insert("v".into(), kinds.version.into());
        for key in ["fields", "rows"] {
            if let Some(part) = form.get(key).filter(|p| !p.is_null()) {
                doc.insert(key.into(), part.clone());
            }
        }
        for problem in crate::kinds::check_card(kinds, &Value::Object(doc))
            .into_iter()
            .filter(|p| !p.stray)
        {
            misfit.push(row(at.clone(), problem.keyed()));
        }
        let left: Vec<Value> = crate::kinds::strays(def, &form)
            .into_iter()
            .map(|(where_, _)| Value::from(where_))
            .collect();
        if !left.is_empty() {
            let mut one = Map::new();
            one.insert("at".into(), at.into());
            one.insert("fields".into(), Value::Array(left));
            strays.push(one);
        }
    }
    (misfit, strays)
}

#[cfg(test)]
mod tests {
    //! The views of the sample project (`tests/fixtures/projects/sample.json`), against the answers
    //! kept (`tests/fixtures/kept/reads/`).

    use super::*;
    use crate::store::Store;
    use std::path::PathBuf;

    /// The sample project, in a store of its own once in a run.
    fn store_path() -> &'static PathBuf {
        crate::fixtures::read_only_sample()
    }

    fn seen() -> Seen {
        let store = Store::open(store_path()).unwrap();
        Seen::read(&store, "見本", None).unwrap()
    }

    /// A view kept (`tests/fixtures/kept/reads/`), as JSON with its keys in order.
    fn kept(name: &str, got: &Value) {
        crate::fixtures::kept(
            &format!("reads/{name}.json"),
            &serde_json::to_string_pretty(got).unwrap(),
        );
    }

    /// The sample project itself, as kept (the checks are `inspection`'s).
    #[test]
    fn the_summary_is_as_kept() {
        kept("summary", &project_view(&seen()));
    }

    #[test]
    fn the_impact_is_as_kept() {
        let got = impact_view(&seen(), "Schemas/Ordering:Order.total", None).unwrap();
        kept("impact", &got);
    }

    #[test]
    fn the_paths_resolve_as_kept() {
        let paths: Vec<String> = [
            "Schemas/Ordering:Order.total",
            "Schemas:Status",
            "Nope:Gone",
            "Schemas/Ordering:Order.nothing",
        ]
        .map(String::from)
        .to_vec();
        let got = exists_view(&seen(), &paths, None, false).unwrap();
        kept("exists", &got);
        let only = exists_view(&seen(), &paths, None, true).unwrap();
        assert_eq!(only["results"].as_array().map(Vec::len), Some(2));
        let many = vec![String::new(); 5001];
        assert_eq!(
            exists_view(&seen(), &many, None, false).unwrap_err(),
            "paths は 5000 件までです（5001 件）"
        );
    }
}
