//! The tools that read: the projects, a project's check, its sections, its cards, its tags
//! (`MCPSurface` and the operations it hands on to in `MCPTools`).

use super::grid::Layout;
use super::mcp_writes::State;
use super::paging::{self, LIST_LIMIT, MOST_CHARS, MOST_LIMIT, Order, Page};
use super::views::Projection;
use super::{Args, Seen, check, visible_projects_in};
use crate::access::Sight;
use crate::mcp::Output;
use crate::store::Store;
use serde_json::{Map, Value};

/// The projects the call's access key may see, each with what it may do with it, their card
/// and section counts and when each was last written (`list_projects`).
pub(crate) fn project_list(store: &Store) -> Result<Output, String> {
    let sight = Sight::of_call(&store.conn)?;
    let rows: Vec<Value> = visible_projects_in(&store.conn)?
        .into_iter()
        .map(|p| {
            let mut row = Map::new();
            row.insert("project".into(), p.name.into());
            if let Some(level) = sight.level(&p.id) {
                row.insert("access".into(), level.word().into());
            }
            row.insert("cardCount".into(), p.card_count.into());
            if p.section_count > 1 {
                row.insert("sections".into(), p.section_count.into());
            }
            row.insert("updatedAt".into(), p.updated_at.into());
            if let Some(archived) = p.archived_at {
                row.insert("archived".into(), archived.into());
            }
            if !p.description.is_empty() {
                row.insert("description".into(), p.description.into());
            }
            Value::Object(row)
        })
        .collect();
    Ok(Output::Json(Value::Array(rows)))
}

/// The sections in tree order, each with its card count, kind and export default
/// (`section_list`).
pub(crate) fn section_list(store: &Store, args: &Args) -> Result<Output, String> {
    let seen = Seen::read(
        store,
        args.string("project")?,
        args.optional_string("branch"),
    )?;
    let given = args.value()?;
    check(&given, "section_list", "list")?;
    // Under one section alone (it, and every section inside it), when it names one.
    let under = match given.get("section") {
        None => None,
        Some(Value::String(path)) => {
            let project = seen.project();
            let id = super::section_id(&project, path)?;
            let mut inside = vec![id.clone()];
            let mut at = 0;
            while at < inside.len() {
                let more: Vec<String> = seen
                    .children(Some(&inside[at]))
                    .iter()
                    .map(|s| s.id.clone())
                    .collect();
                inside.extend(more);
                at += 1;
            }
            Some(
                inside
                    .into_iter()
                    .map(|id| project.directory.full_section(&id))
                    .collect::<std::collections::HashSet<String>>(),
            )
        }
        Some(_) => return Err(text!("reads.sectionList.sectionPath")),
    };
    // The directories a working directory holds are listed under it alone; the working directory
    // says how many it read.
    let counts: std::collections::HashMap<String, (usize, usize)> =
        if crate::workdir::mounts(&store.conn, &seen.summary.id)?.is_empty() {
            Default::default()
        } else {
            crate::workdir::reads(&store.conn, &seen.summary.id)?
                .into_iter()
                .map(|r| (crate::workdir::dir_id(&r.dir, ""), (r.files, r.dirs)))
                .collect()
        };
    let project = seen.project();
    let rows: Vec<Value> = seen
        .listed()
        .into_iter()
        .zip(seen.section_rows(None))
        .filter(|(s, row)| match &under {
            Some(paths) => row["section"].as_str().is_some_and(|p| paths.contains(p)),
            None => s.kind.as_deref() != Some(crate::kinds::DIRECTORY),
        })
        .map(|(s, mut row)| {
            if let Some((files, dirs)) = counts.get(&s.id)
                && project.section_kind(&s.id) == Some(crate::kinds::WORKDIR)
            {
                row.insert("files".into(), (*files).into());
                row.insert("directories".into(), (*dirs).into());
            }
            Value::Object(row)
        })
        .collect();
    let mut out = Map::new();
    out.insert("project".into(), seen.summary.name.clone().into());
    out.insert("sections".into(), Value::Array(rows));
    Ok(seen.headed(Output::Json(Value::Object(out))))
}

/// One section: its row, its cards and the sections in it (`section_read`).
pub(crate) fn section_read(store: &Store, args: &Args) -> Result<Output, String> {
    let path = args.target()?;
    let given = args.value()?;
    let seen = Seen::read(
        store,
        args.string("project")?,
        args.optional_string("branch"),
    )?;
    // Each part's page is a mock's alone: no other read draws parts.
    if given.contains_key("faces") && given.get("as").and_then(Value::as_str) != Some("mock") {
        return Err(text!("mocks.facesOnlyMock"));
    }
    if given.contains_key("as") {
        let out = super::diagram::diagram(
            &seen,
            &given,
            super::diagram::At::Section(&path),
            args.paged.then_some(MOST_CHARS),
        )?;
        return Ok(seen.headed(out));
    }
    let project = seen.project();
    refuse_settings_as_section(&seen, &path)?;
    let id = project
        .directory
        .section_named(&path, None)
        .ok_or_else(|| {
            project
                .directory
                .section_problem(&path, &project.directory.home)
        })?;
    let mut row = seen
        .section_rows(Some(&id))
        .into_iter()
        .next()
        .ok_or_else(|| text!("reads.section.unreadable", path))?;
    // Its cards, then its card ghosts (named among them, `kind: ghost` in card_read).
    let cards: Vec<Value> = project
        .directory
        .members(&id)
        .iter()
        .map(|&i| project.full(project.nth(i)).into())
        .chain(
            project
                .ghosts_in(&id)
                .into_iter()
                .map(|g| project.full_ghost(g).into()),
        )
        .collect();
    row.insert("cards".into(), Value::Array(cards));
    let inside: Vec<Value> = seen
        .children(Some(&id))
        .iter()
        .map(|s| project.directory.full_section(&s.id).into())
        .collect();
    if !inside.is_empty() {
        row.insert("sections".into(), Value::Array(inside));
    }
    // The boxes on its diagram (DiagramBoxes).
    if let Some(include) = given.get("include") {
        let items: Vec<&str> = include
            .as_array()
            .map(|l| l.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();
        if let Some(other) = items.iter().find(|i| **i != "places") {
            return Err(text!("reads.include.placesOnly", other));
        }
        if items.contains(&"places") {
            let layout = Layout::read(&store.conn, &seen.summary.id)?;
            row.insert(
                "places".into(),
                super::boxes::read_places(&store.reader(), &seen, &layout, &id)?,
            );
        }
    }
    let mut out = Map::new();
    out.insert("project".into(), seen.summary.name.clone().into());
    out.extend(row);
    // A working directory's section: how and from where it is read, and what it can be made to do.
    let mut warnings = Vec::new();
    if project.section_kind(&id) == Some(crate::kinds::WORKDIR)
        && let Some(dir) = id.strip_prefix("dir:")
    {
        let (said, notes) = super::workdirs::said(&store.conn, &seen.summary.id, dir)?;
        out.extend(said);
        warnings = notes;
    }
    Ok(seen.headed(Output::Json(super::warned(Value::Object(out), &warnings))))
}

/// Where Rules and the export directories went: the project's settings.
fn not_a_section() -> String {
    text!("reads.settingsNotSection")
}

/// A path that starts with Rules or Export, where neither is a section any more.
pub(crate) fn refuse_settings_as_section(seen: &Seen, path: &str) -> Result<(), String> {
    let rest = crate::pathtext::tree_prefix(path).map_or(path, |(_, rest)| rest);
    let section = crate::pathtext::split_section(rest).0.unwrap_or(rest);
    let head = crate::pathtext::fold(&crate::pathtext::unquote(
        crate::pathtext::slashes(section)[0],
    ));
    if !["export", "rules"].contains(&head.as_str()) {
        return Ok(());
    }
    let home = seen.project().directory.home.clone();
    if seen
        .children(Some(&home))
        .iter()
        .any(|s| crate::pathtext::fold(&s.name) == head)
    {
        return Ok(());
    }
    Err(not_a_section())
}

/// The section `section` names, as the base unqualified names are read from
/// (`MCPTools.base` and `section`).
pub(crate) fn base(seen: &Seen, given: &Map<String, Value>) -> Result<Option<String>, String> {
    let Some(name) = given.get("section").and_then(Value::as_str) else {
        return Ok(None);
    };
    let project = seen.project();
    project
        .directory
        .section_named(name.trim(), None)
        .map(Some)
        .ok_or_else(|| {
            project
                .directory
                .section_problem(name, &project.directory.home)
        })
}

/// The sections a list takes its cards from: `base` alone, or with `recursive` it and every
/// section under it, each once in the tree's order (a section, then what is in it) so the cards
/// come grouped as the sections nest. None when no section is named.
fn scope(
    project: &crate::export::Project,
    given: &Map<String, Value>,
    base: Option<&String>,
) -> Result<Option<Vec<String>>, String> {
    let recursive = given
        .get("recursive")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let Some(base) = base else {
        if recursive {
            return Err(text!("reads.recursive.needsSection"));
        }
        return Ok(None);
    };
    if !recursive {
        return Ok(Some(vec![base.clone()]));
    }
    fn walk(project: &crate::export::Project, id: &str, out: &mut Vec<String>) {
        if out.iter().any(|s| s == id) {
            return;
        }
        out.push(id.to_string());
        for inner in project.directory.sections_in(id) {
            walk(project, &inner, out);
        }
    }
    let mut out = Vec::new();
    walk(project, base, &mut out);
    Ok(Some(out))
}

/// What `include` asks of a goal's card map: its boxes (`card_read` only).
const PLACES: &str = "places";
/// card_read's own: the board's cards still to do that reach the card (`fromBoard`).
const FROM_BOARD: &str = "from_board";
/// A goal's or a task's actions, each with whether it can be used now (`actions`): `card_read` and
/// `card_list`.
const ACTIONS: &str = "actions";

/// `include`: what to return of each card. When none are named, a card read is given every part
/// and a list what each card holds itself (`Projection::own`): where the cards sit and what points
/// back at them weigh as much again over many cards, and are asked for by name. `places` (a goal's
/// card map, `card_read` only) is not a part: with it alone, none of them.
fn projection(given: &Map<String, Value>, places: bool) -> Result<Projection, String> {
    let unnamed = || {
        if places {
            Projection::everything()
        } else {
            Projection::own()
        }
    };
    let Some(raw) = given.get("include").and_then(Value::as_array) else {
        return Ok(unnamed());
    };
    let items: Vec<String> = raw
        .iter()
        .filter_map(|v| v.as_str().map(str::to_string))
        .collect();
    let unknown: Vec<&str> = items
        .iter()
        .map(String::as_str)
        .filter(|i| {
            !(Projection::KNOWN.contains(i)
                || *i == ACTIONS
                || places && (*i == PLACES || *i == FROM_BOARD))
        })
        .collect();
    if !unknown.is_empty() {
        let mut known: Vec<&str> = Projection::KNOWN.to_vec();
        if places {
            known.push(PLACES);
            known.push(FROM_BOARD);
        }
        known.push(ACTIONS);
        known.sort();
        return Err(text!(
            "reads.include.unknown",
            unknown = unknown.join(", "),
            known = known.join(" / ")
        ));
    }
    if items.is_empty() {
        return Ok(unnamed());
    }
    let parts: Vec<String> = items
        .into_iter()
        .filter(|i| i != PLACES && i != FROM_BOARD && i != ACTIONS)
        .collect();
    Ok(Projection::of(&parts))
}

/// One path read (`get_schema` with `path`).
/// The words `find` looks through in what `path` names when it has no body of its own (`read`,
/// as read): the whole of a formatdoc, a stackdoc or a flow (its nodes too), or of an item of a
/// formatdoc. None for anything else, read in part by its body.
fn find_places(
    seen: &Seen,
    path: &str,
    given: &Map<String, Value>,
    read: &Value,
) -> Result<Option<Vec<super::texts::Place>>, String> {
    if !given.contains_key("find") || read.get("body").is_some_and(Value::is_string) {
        return Ok(None);
    }
    let base = base(seen, given)?;
    let project = seen.project();
    Ok(
        match super::walk::locate(&project, path, base.as_deref())? {
            super::walk::Target::Card(card)
                if ["formatdoc", "stackdoc", "flow"].contains(&card.kind.as_str()) =>
            {
                Some(super::texts::whole(&project, card))
            }
            super::walk::Target::FormatRow(card, names)
                if super::walk::format_row(&names, &card.rows)
                    .is_some_and(|r| r.kind == "item") =>
            {
                let row = format!(
                    "{}.{}",
                    project.full(card),
                    crate::pathtext::join_names(names.iter().map(String::as_str))
                );
                let under = format!("{row}.");
                let mut places = super::texts::inside(&project, card);
                places.retain(|p| p.path == row || p.path.starts_with(&under));
                Some(places)
            }
            _ => None,
        },
    )
}

#[allow(clippy::too_many_arguments)]
fn read_one(
    reader: &crate::store::Reader,
    seen: &Seen,
    path: &str,
    given: &Map<String, Value>,
    layout: &Layout,
    board: Option<&crate::model::Detail>,
    main: Option<&crate::model::Detail>,
) -> Result<Value, String> {
    let projection = projection(given, true)?;
    let base = base(seen, given)?;
    let project = seen.project();
    let target = super::walk::locate(&project, path, base.as_deref())?;
    // An element of a data card's array: one there, picked.
    if let super::walk::Target::CustomRow(card, names) = &target
        && card.kind == crate::kinds::INSTANCE
        && let Err((i, why)) = super::instances::walked(card.instance_data(), names)
    {
        return Err(format!(
            "{}.{}: {why}",
            project.full(card),
            names[..=i].join(".")
        ));
    }
    // What is read through a card ghost is said through it.
    let through = super::walk::through_ghost(&project, path, base.as_deref());
    let mut read =
        super::walk::shown_through(seen.level(&target, &projection, Some(layout)), &through);
    // The form it is written in, at the same path (`card_update`'s keys are the ones read).
    super::views::put_form(&mut read, super::views::form_of(&target));
    // A working directory's file: the tree's cards whose lines say they are carried out in it, the
    // other way round (with the function or type each lands at, when it lands inside), and what it
    // holds as code.
    if let (super::walk::Target::Card(card), Some(main)) = (&target, main)
        && card.kind == "file"
    {
        let found = super::code::landing_here(&crate::export::Project::new(main), &card.id, None);
        if let Value::Object(object) = &mut read {
            object.extend(super::code::file_said(&project, card, given));
            if !found.is_empty() {
                object.insert("referencedBy".into(), Value::Array(found));
            }
        }
    }
    // main's card: the board's cards still to do that reach it, or the row read in it
    // (「関わるやること」).
    if let Some(board) = board {
        let (card, inside) = match &target {
            super::walk::Target::Card(card) => (Some(*card), None),
            super::walk::Target::Field(card, _, names)
            | super::walk::Target::FormatRow(card, names) => (
                Some(*card),
                Some(crate::pathtext::join_names(
                    names.iter().map(String::as_str),
                )),
            ),
            _ => (None, None),
        };
        if let Some(card) = card {
            let found: Vec<Value> = super::cross::reaching(board, card, &project.full(card), false)
                .into_iter()
                .filter(|r| match (&inside, &r.at) {
                    (None, _) => true,
                    (Some(row), Some(at)) => at == row || at.starts_with(&format!("{row}.")),
                    (Some(_), None) => false,
                })
                .map(|r| r.json())
                .collect();
            if !found.is_empty()
                && let Value::Object(object) = &mut read
            {
                object.insert("fromBoard".into(), Value::Array(found));
            }
        }
    }
    // A part read whole: the cards its lines reach that changed after it, to look at again.
    if !given.contains_key("include")
        && let super::walk::Target::Card(card) = &target
        && super::review::PARTS.contains(&card.kind.as_str())
        && let Some(marks) = super::review::marks(
            reader.conn,
            &seen.summary.id,
            &seen.named.tree,
            &project,
            Some(&card.id),
        )?
        .remove(&card.id)
        && let Value::Object(object) = &mut read
    {
        object.insert("review".into(), super::review::listed(&marks));
    }
    // A card read whole: its implementation's lines whose code changed since it was recorded, or
    // was never recorded, on this machine.
    if !given.contains_key("include")
        && let super::walk::Target::Card(card) = &target
        && card.kind != "file"
    {
        let lines =
            super::code_review::lines(reader.conn, &seen.summary.id, &project, Some(&card.id))?;
        if lines.iter().any(super::code_review::CodeLine::flagged)
            && let Value::Object(object) = &mut read
        {
            object.insert(
                "codeReview".into(),
                super::code_review::listed(&lines, false),
            );
        }
    }
    // A goal's or a task's actions, worked out as `card_action` decides them.
    let actions = given
        .get("include")
        .and_then(Value::as_array)
        .is_some_and(|l| l.iter().any(|v| v.as_str() == Some(ACTIONS)));
    if actions
        && let super::walk::Target::Card(card) = target
        && let Some(listed) = super::actions::listed(
            reader,
            &seen.summary.id,
            &project,
            card,
            &super::actions::Works::new(&seen.named.tree, false),
        )?
        && let Value::Object(object) = &mut read
    {
        object.insert(ACTIONS.into(), listed);
    }
    // A goal's card map: the boxes of the cards it holds.
    let places = given
        .get("include")
        .and_then(Value::as_array)
        .is_some_and(|l| l.iter().any(|v| v.as_str() == Some(PLACES)));
    if places {
        let super::walk::Target::Card(card) = target else {
            // A ghost has a box of its own; a field or anything else has none.
            let has_box = matches!(target, super::walk::Target::Ghost(_));
            let said = if has_box {
                text!("reads.places.ghost", path)
            } else {
                text!("reads.places.notCard", path)
            };
            return Err(crate::mcp::refusal(
                said,
                serde_json::json!({ "fix": [{ "at": "/value/include", "to": include_without_places(given, has_box) }] }),
            ));
        };
        if !project.card_mapped(&card.kind) {
            return Err(crate::mcp::refusal(
                text!(
                    "reads.places.notMapped",
                    card = project.full(card),
                    kind = card.kind
                ),
                serde_json::json!({ "fix": [{ "at": "/value/include", "to": include_without_places(given, true) }] }),
            ));
        }
        if let Value::Object(object) = &mut read {
            object.insert(
                PLACES.into(),
                super::card_map::read_places(seen, layout, card),
            );
        }
    }
    // A card: where it is written out (§10).
    if projection.has("exports")
        && let super::walk::Target::Card(card) = target
        && (card.is_shape() || card.kind == "flow" || card.is_writing())
        && let Value::Object(object) = &mut read
    {
        // Only where it is written: a file the export leaves it out of (a problem, as
        // export_read tells) is not one.
        let resolved = crate::export::settings::resolve(&seen.exports, &project);
        let (planned, _) = crate::export::layout_named(&project, &resolved, &seen.exports);
        let files: Vec<Value> = crate::export::placed(card, &resolved)
            .iter()
            .filter(|placed| {
                let path = crate::export::file_path(card, placed);
                planned.iter().any(|f| {
                    f.export == placed.export && f.path == path && f.cards.contains(&card.id)
                })
            })
            .map(|placed| {
                let setting = &placed.setting;
                let mut row = Map::new();
                row.insert(
                    "export".into(),
                    seen.exports[placed.export].name.clone().into(),
                );
                // Its path from where the export is written to (that is given when it is written).
                row.insert("file".into(), crate::export::file_path(card, placed).into());
                if card.is_shape()
                    && let Some(format) = setting.schema_format()
                {
                    row.insert("format".into(), format.into());
                }
                Value::Object(row)
            })
            .collect();
        if !files.is_empty() {
            object.insert("writtenTo".into(), Value::Array(files));
        }
    }
    if let Value::Object(object) = &mut read
        && let Some(hint) = body_elsewhere(&project, &target, object)
    {
        object.insert("hint".into(), hint.into());
    }
    Ok(read)
}

/// Where a document's text is, when its read leaves it out (a formatdoc read as it is gave
/// its items' headings and counts, and its text was found only once `as: markdown` was): a
/// formatdoc's or an item's rows with rows inside, a stackdoc's versions.
fn body_elsewhere(
    project: &crate::export::Project,
    target: &super::walk::Target,
    read: &Map<String, Value>,
) -> Option<String> {
    use super::walk::Target;
    let counted = read
        .get("rows")
        .and_then(Value::as_array)
        .is_some_and(|rows| rows.iter().any(|r| r.get("rowCount").is_some()));
    let versions = read
        .get("versions")
        .and_then(Value::as_array)
        .is_some_and(|v| !v.is_empty());
    match target {
        Target::Card(card) if card.kind == "formatdoc" && counted => {
            Some(text!("reads.hint.formatdocBody"))
        }
        Target::Card(card) if card.kind == "stackdoc" && versions => {
            Some(text!("reads.hint.stackdocBody"))
        }
        Target::FormatRow(doc, names) if counted => {
            let doc = project.full(doc);
            let item = format!(
                "{doc}.{}",
                crate::pathtext::join_names(names.iter().map(String::as_str))
            );
            Some(text!("reads.hint.itemBody", item, doc))
        }
        _ => None,
    }
}

/// Cards, fields, rows or nodes read (`card_read`); with `as`, read otherwise.
/// `include` with `places` taken out, and `at` put in its place when the target has a box of its own.
fn include_without_places(given: &Map<String, Value>, own_box: bool) -> Value {
    let mut out: Vec<Value> = Vec::new();
    for key in given
        .get("include")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let key = if own_box && key.as_str() == Some(PLACES) {
            Value::from("at")
        } else {
            key.clone()
        };
        if key.as_str() != Some(PLACES) && !out.contains(&key) {
            out.push(key);
        }
    }
    Value::Array(out)
}

/// The refusal for a path that exists but is a form `as: <mode>` does not take (an enum value, a
/// node's step, a row of an own kind, a ghost), saying what it is instead of "not found". None for
/// a form the mode takes, whose own refusal stands. For an enum value under `impact`, the call that
/// finds where the value is used.
fn wrong_form(
    seen: &Seen,
    args: &Args,
    path: &str,
    base: Option<&str>,
    mode: &str,
) -> Option<String> {
    use super::walk::Target;
    let project = seen.project();
    let located = super::walk::locate(&project, path, base).ok()?;
    let what = match &located {
        Target::EnumValue(..) => text!("reads.as.what.enumValue"),
        Target::Step(..) => text!("reads.as.what.step"),
        Target::CustomRow(..) => text!("reads.as.what.customRow"),
        Target::Ghost(_) => text!("reads.as.what.ghost"),
        _ => return None,
    };
    let takes = match mode {
        "impact" => text!("reads.as.takes.impact"),
        _ => text!("reads.as.takes.markdown"),
    };
    let mut message = text!("reads.as.form", mode, takes, path, what);
    let mut beside = serde_json::json!({});
    if let (Target::EnumValue(_, name), "impact") = (&located, mode) {
        message = text!(
            "reads.as.sentences",
            first = message,
            second = text!("reads.as.impactEnumUses")
        );
        let mut arguments = Map::new();
        arguments.insert("project".into(), args.string("project").ok()?.into());
        if let Some(branch) = args.optional_string("branch") {
            arguments.insert("branch".into(), branch.into());
        }
        arguments.insert("value".into(), serde_json::json!({ "query": name }));
        beside = serde_json::json!({ "use": { "tool": "card_list", "arguments": arguments } });
    }
    Some(crate::mcp::refusal(message, beside))
}

pub(crate) fn card_read(store: &Store, args: &Args) -> Result<Output, String> {
    let list = args
        .targets()?
        .ok_or_else(|| text!("reads.target.needed"))?;
    let mut given = args.value()?;
    let seen = Seen::read(
        store,
        args.string("project")?,
        args.optional_string("branch"),
    )?;
    if given.get("as").and_then(Value::as_str) == Some("diagram") {
        let out = super::diagram::diagram(
            &seen,
            &given,
            super::diagram::At::Cards(&list),
            args.paged.then_some(MOST_CHARS),
        )?;
        return Ok(seen.headed(out));
    }
    let way = given.remove("as");
    // `lines` is the body's range (`lines: {from, to}`): the kinds of line are read by another name.
    if way.as_ref().and_then(Value::as_str) == Some("lines") {
        return Err(crate::mcp::refusal(
            text!("reads.as.linesRenamed"),
            serde_json::json!({ "fix": [{ "at": "/value/as", "to": "lineKinds" }] }),
        ));
    }
    if way.as_ref().and_then(Value::as_str) == Some("lineKinds") {
        check(&given, "card_read", "as: lineKinds")?;
        let [one] = list.as_slice() else {
            return Err(text!("reads.lines.oneTarget", count = list.len()));
        };
        return Ok(seen.headed(Output::Json(super::lines::lines_view(&seen, one, &given)?)));
    }
    match way.as_ref().map(|w| w.as_str()) {
        None => {
            check(&given, "card_read", "card・field")?;
            let layout = Layout::read(&store.conn, &seen.summary.id)?;
            // main's cards (and a branch's) are read with the board's that reach them, unless
            // `include` leaves them out.
            let asked = given
                .get("include")
                .and_then(Value::as_array)
                .is_none_or(|keys| keys.is_empty() || keys.iter().any(|k| k == FROM_BOARD));
            let board = if asked && seen.named.tree != crate::store::BOARD {
                Some(super::cross::board(&store.reader(), &seen.summary.id)?)
            } else {
                None
            };
            // A working directory's files are read with the tree's cards that point at them.
            let main = Some(&seen.detail);
            let read: Vec<Value> = list
                .iter()
                .map(|p| {
                    // A function or a type inside a file: read from the file now.
                    if let Some(one) = read_symbol(&seen, p, &given, args.paged)? {
                        return Ok(one);
                    }
                    // A fix is for a call with the one target; with several it would change them all.
                    let mut one = read_one(
                        &store.reader(),
                        &seen,
                        p,
                        &given,
                        &layout,
                        board.as_ref(),
                        main,
                    )
                    .map_err(|said| {
                        if list.len() > 1 {
                            crate::mcp::rebase_fix(&said, None)
                        } else {
                            said
                        }
                    })?;
                    // Its body in part (`in_part`): where a text is, or the lines asked for. A
                    // document or a flow with no body of its own is looked through whole.
                    match find_places(&seen, p, &given, &one)? {
                        Some(places) => super::in_part::find_in_places(&mut one, &given, &places)?,
                        None => super::in_part::read(
                            &mut one,
                            &given,
                            args.paged.then_some(paging::BODY_LINES),
                        )?,
                    }
                    // Through MCP, the rev of the card it is in (`revs`): what a write made from it
                    // gives back.
                    if args.paged
                        && let Some(rev) = seen.rev_at(p, base(&seen, &given)?.as_deref())
                    {
                        super::revs::stamp(&mut one, &rev);
                    }
                    Ok::<_, String>(one)
                })
                .collect::<Result<_, _>>()?;
            if read.len() == 1 {
                return Ok(seen.headed(Output::Json(read.into_iter().next().unwrap())));
            }
            let mut out = Map::new();
            out.insert("project".into(), seen.summary.name.clone().into());
            out.insert(
                "cards".into(),
                Value::Array(read.into_iter().map(super::under_head).collect()),
            );
            Ok(seen.headed(Output::Json(Value::Object(out))))
        }
        // A document, a note, a flow: its Markdown whole, as written out but with its paths as
        // written.
        Some(Some("markdown")) => {
            check(&given, "card_read", "as: markdown")?;
            let project = seen.project();
            // Written as the project writes it out: its words in its export language; what is
            // refused, in the call's.
            let written = &seen.detail.project;
            let range = markdown_lines(&given)?;
            let most = args.paged.then_some(paging::BODY_LINES);
            let mut read: Vec<Value> = list
                .iter()
                .map(|p| {
                    let (card, text, id) = {
                        // A stackdoc's version: that version alone, as it is written out.
                        if let Ok(super::walk::Target::CustomRow(card, names)) =
                            super::walk::locate(&project, p, None)
                            && card.kind == "stackdoc"
                            && let Some(text) = written.in_export_language(|| {
                                crate::export::documents::version_as_written(
                                    card, &names[0], &project,
                                )
                            })
                        {
                            (
                                format!("{}.{}", project.full(card), names[0]),
                                text,
                                &card.id,
                            )
                        // A formatdoc's item: its part of the document's Markdown alone.
                        } else if let Ok(super::walk::Target::FormatRow(doc, names)) =
                            super::walk::locate(&project, p, None)
                            && let Some(text) = written.in_export_language(|| {
                                crate::export::documents::row_as_written(doc, &names, &project)
                            })
                        {
                            (
                                format!(
                                    "{}.{}",
                                    project.full(doc),
                                    crate::pathtext::join_names(names.iter().map(String::as_str))
                                ),
                                text,
                                &doc.id,
                            )
                        } else {
                            let card =
                                super::walk::find_card(&project, p, None).map_err(|said| {
                                    wrong_form(&seen, args, p, None, "markdown").unwrap_or(said)
                                })?;
                            // A table: the table it builds.
                            if card.kind == crate::kinds::TABLE {
                                (
                                    project.full(card),
                                    written.in_export_language(|| {
                                        super::tables::markdown(&super::tables::build(
                                            &project, card,
                                        ))
                                    }),
                                    &card.id,
                                )
                            } else if card.kind == "flow" {
                                // A flow: its picture and every node, in one text.
                                (
                                    project.full(card),
                                    written.in_export_language(|| {
                                        crate::export::flowdoc::as_written(card, &project)
                                    }),
                                    &card.id,
                                )
                            } else if !card.is_writing() {
                                return Err(text!(
                                    "reads.markdown.cannotRead",
                                    card = project.full(card),
                                    kind = card.kind
                                ));
                            } else {
                                (
                                    project.full(card),
                                    written.in_export_language(|| {
                                        crate::export::documents::markdown_as_written(
                                            card, &project,
                                        )
                                    }),
                                    &card.id,
                                )
                            }
                        }
                    };
                    // Through MCP, the card's rev (`revs`).
                    let rev = seen.rev(id).filter(|_| args.paged);
                    markdown_part(&card, rev.as_deref(), &text, range, most)
                })
                .collect::<Result<_, String>>()?;
            let mut out = Map::new();
            out.insert("project".into(), seen.summary.name.clone().into());
            if let [one] = &mut read[..]
                && let Value::Object(one) = one
            {
                out.extend(std::mem::take(one));
            } else {
                out.insert("cards".into(), Value::Array(read));
            }
            Ok(seen.headed(Output::Json(Value::Object(out))))
        }
        Some(Some("context")) => super::context::read(store, &seen, &list, &given, args),
        Some(Some(other)) if !["impact", "diagram", "exists"].contains(&other) => {
            Err(text!("reads.as.unknown", other))
        }
        Some(Some("impact")) => {
            check(&given, "card_read", "as: impact")?;
            let base = super::summary::base(&seen, given.get("section").and_then(Value::as_str))?;
            let read: Vec<Value> = list
                .iter()
                .map(|p| {
                    super::summary::impact_view(&seen, p, base.as_deref()).map_err(|said| {
                        wrong_form(&seen, args, p, base.as_deref(), "impact").unwrap_or(said)
                    })
                })
                .collect::<Result<_, _>>()?;
            if read.len() == 1 {
                return Ok(seen.headed(Output::Json(read.into_iter().next().unwrap())));
            }
            let mut out = Map::new();
            out.insert("impacts".into(), Value::Array(read));
            Ok(seen.headed(Output::Json(Value::Object(out))))
        }
        Some(Some(_)) => {
            check(&given, "card_read", "as: exists")?;
            let base = super::summary::base(&seen, given.get("section").and_then(Value::as_str))?;
            let only = given
                .get("unresolvedOnly")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            Ok(seen.headed(Output::Json(super::summary::exists_view(
                &seen,
                &list,
                base.as_deref(),
                only,
            )?)))
        }
        Some(None) => Err(text!("reads.as.oneOf")),
    }
}

/// A function or a type inside a working directory's file, read (`code::symbol_read`) — through MCP
/// with the rev of its file's card; none when the path names something else.
fn read_symbol(
    seen: &Seen,
    path: &str,
    given: &Map<String, Value>,
    paged: bool,
) -> Result<Option<Value>, String> {
    let base = base(seen, given)?;
    let project = seen.project();
    let Ok(super::walk::Target::Symbol(file, names)) =
        super::walk::locate(&project, path, base.as_deref())
    else {
        return Ok(None);
    };
    check_symbol_read(given)?;
    let mut one = super::code::symbol_read(
        &project,
        &seen.summary.name,
        file,
        &names,
        given,
        paged.then_some(paging::BODY_LINES),
    )?;
    if paged && let Some(rev) = seen.rev(&file.id) {
        super::revs::stamp(&mut one, &rev);
    }
    Ok(Some(one))
}

/// What a function or a type is read with: its lines or a word in it, as a body is; `include` names
/// nothing there is of one.
fn check_symbol_read(given: &Map<String, Value>) -> Result<(), String> {
    if let Some(key) = given
        .keys()
        .find(|k| !["lines", "find", "context", "section"].contains(&k.as_str()))
    {
        return Err(text!("code.symbol.readKey", key));
    }
    Ok(())
}

/// The lines `as: markdown` is read in (`lines: {from, to}`), the Markdown's own, from 1: from, and to when it is said.
fn markdown_lines(given: &Map<String, Value>) -> Result<Option<(usize, Option<usize>)>, String> {
    let Some(raw) = given.get("lines") else {
        return Ok(None);
    };
    let Some(range) = raw.as_object() else {
        return Err(text!("reads.lines.shape"));
    };
    if let Some(stray) = range.keys().find(|k| !["from", "to"].contains(&k.as_str())) {
        return Err(text!("reads.lines.stray", stray));
    }
    let number = |key: &str| -> Result<Option<usize>, String> {
        match range.get(key) {
            None => Ok(None),
            Some(n) => n
                .as_u64()
                .filter(|n| *n >= 1)
                .map(|n| Some(n as usize))
                .ok_or_else(|| text!("reads.lines.number", key)),
        }
    };
    let from = number("from")?.ok_or_else(|| text!("reads.lines.fromNeeded"))?;
    let to = number("to")?;
    if to.is_some_and(|to| to < from) {
        return Err(text!("reads.lines.toBefore"));
    }
    Ok(Some((from, to)))
}

/// A card's Markdown as `as: markdown` gives it: by its path (with the card's `rev` read through
/// MCP) and how many lines it has, whole, or the lines `range` says — up to `most` lines when it says
/// no end, and when it says nothing, read through MCP — then saying which (`lines: {from, to}`) and,
/// stopped short of the end it was not told, `truncated`.
fn markdown_part(
    card: &str,
    rev: Option<&str>,
    text: &str,
    range: Option<(usize, Option<usize>)>,
    most: Option<usize>,
) -> Result<Value, String> {
    let lines: Vec<&str> = text.split('\n').collect();
    let count = lines.len();
    let mut out = Map::new();
    out.insert("path".into(), card.into());
    if let Some(rev) = rev {
        out.insert("rev".into(), rev.into());
    }
    let (from, to, told) = match range {
        Some((from, to)) => (from, to, to.is_some()),
        None if most.is_some_and(|m| count > m) => (1, None, false),
        None => {
            out.insert("lineCount".into(), count.into());
            out.insert("markdown".into(), text.into());
            return Ok(Value::Object(out));
        }
    };
    if from > count {
        return Err(text!("reads.lines.beyond", card, count));
    }
    let to = to
        .or_else(|| most.map(|m| from + m - 1))
        .unwrap_or(count)
        .min(count);
    let mut said = Map::new();
    said.insert("from".into(), from.into());
    said.insert("to".into(), to.into());
    out.insert("lines".into(), Value::Object(said));
    out.insert("lineCount".into(), count.into());
    if !told && to < count {
        out.insert("truncated".into(), true.into());
    }
    out.insert("markdown".into(), lines[from - 1..to].join("\n").into());
    Ok(Value::Object(out))
}

/// Cards gathered (`card_list`: `get_schema` without a path, or `find` with `query`).
/// Around the first place `query` is in `text`: 40 characters before, 60 after
/// (`MCPTools.excerpt`).
fn excerpt(text: &str, query: &str) -> Option<String> {
    let lower: Vec<char> = text.to_lowercase().chars().collect();
    let wanted: Vec<char> = query.chars().collect();
    let at = (0..=lower.len().checked_sub(wanted.len())?)
        .find(|&i| lower[i..i + wanted.len()] == wanted[..])?;
    let chars: Vec<char> = text.chars().collect();
    let from = at.saturating_sub(40);
    let to = (at + wanted.len() + 60).min(chars.len());
    let cut: String = chars[from.min(chars.len())..to].iter().collect();
    Some(format!(
        "{}{}{}",
        if from > 0 { "…" } else { "" },
        cut.trim(),
        if to < chars.len() { "…" } else { "" }
    ))
}

/// Cards and fields where `query` is, by part (`find`): a name, a label, what is written
/// about it, the paths it declares, and the words it writes inside itself — a document's
/// rows and versions, a node's steps (`texts`), each by the path of its row and where in it, and a
/// data card's values (`data.status`). Each place found carries the rev of the card it is in.
/// The cards inside containers (a flow's nodes, a goal's tasks) are looked through too, each
/// after its container; `kind` and `in` narrow them as `card_list` does. Prose stops at the
/// first hit of a card's head or a field; declarations and the words inside do not — a
/// rename has to follow every one of them.
fn find(
    seen: &Seen,
    given: &Map<String, Value>,
    excerpts: bool,
    page: Option<&Page>,
) -> Result<Output, String> {
    let base = base(seen, given)?;
    let raw = given
        .get("query")
        .and_then(Value::as_str)
        .ok_or_else(|| text!("reads.query.string"))?;
    let query = raw.to_lowercase();
    if query.is_empty() {
        return Err(text!("reads.query.empty"));
    }
    let project = seen.project();
    let scope = scope(&project, given, base.as_ref())?;
    // Each hit is stamped with the rev of the card it is in once that card is looked through.
    let found: std::cell::RefCell<Vec<Value>> = std::cell::RefCell::new(Vec::new());
    let probe = |path: &str, kind: &str, place: &str, text: Option<&str>| -> bool {
        let Some(text) = text.filter(|t| t.to_lowercase().contains(&query)) else {
            return false;
        };
        let mut row = Map::new();
        row.insert("path".into(), path.into());
        row.insert("type".into(), kind.into());
        row.insert("where".into(), place.into());
        if place != "name"
            && let Some(said) = excerpt(text, &query)
        {
            row.insert("excerpt".into(), said.into());
        }
        found.borrow_mut().push(Value::Object(row));
        true
    };
    // The places found in a card, gathered under it once it is looked through: the card by its path,
    // its kind and its rev, and each place by its path (`at`), the part (`where`) and the words around.
    let mut cards: Vec<Value> = Vec::new();
    let mut gather = |path: &str, kind: &str, id: &str| {
        let hits: Vec<Value> = found.borrow_mut().drain(..).collect();
        if hits.is_empty() {
            return;
        }
        let mut entry = Map::new();
        entry.insert("path".into(), path.into());
        entry.insert("kind".into(), kind.into());
        if let Some(rev) = seen.rev(id) {
            entry.insert("rev".into(), rev.into());
        }
        let matches: Vec<Value> = hits
            .into_iter()
            .map(|hit| {
                let mut one = Map::new();
                one.insert("at".into(), hit["path"].clone());
                one.insert("where".into(), hit["where"].clone());
                if excerpts && let Some(said) = hit.get("excerpt") {
                    one.insert("excerpt".into(), said.clone());
                }
                Value::Object(one)
            })
            .collect();
        entry.insert("matches".into(), Value::Array(matches));
        cards.push(Value::Object(entry));
    };
    let rule_text = |rule: &crate::model::Rule| {
        crate::ojson::compact(&check::rules_json(std::slice::from_ref(rule))[0])
    };
    let mut chosen: Vec<&crate::model::Card> = seen
        .detail
        .schemas
        .iter()
        .chain(seen.detail.inner.iter())
        .map(|c| &**c)
        .filter(|c| {
            scope
                .as_ref()
                .is_none_or(|s| s.contains(&project.section_of(c)))
        })
        .collect();
    let narrowed = given.contains_key("kind") || given.contains_key("in");
    if let Some(kinds) = kinds_given(given) {
        chosen.retain(|c| kinds.contains(&c.kind));
    }
    if let Some(path) = given.get("in") {
        let path = path.as_str().ok_or_else(|| text!("reads.in.container"))?;
        let held = super::board::inside(seen, path, base.as_deref())?;
        chosen.retain(|c| held.contains(&c.id));
    }
    by_section(&project, scope.as_deref(), &mut chosen);
    in_tree_order(&project, &mut chosen);
    for card in chosen {
        let kind = card.kind.as_str();
        let own = project.full(card);
        let tags = card.tags.join(" ");
        let mut metadata: Vec<(&String, &Value)> = card.metadata.iter().collect();
        metadata.sort_by(|a, b| a.0.cmp(b.0));
        let _ = probe(&own, kind, "name", Some(&card.name))
            || probe(&own, kind, "label", Some(&card.label))
            || probe(&own, kind, "tag", Some(&tags))
            || probe(&own, kind, "description", Some(&card.description))
            || card.absent_fields.iter().any(|a| {
                probe(
                    &own,
                    kind,
                    &format!("absentFields[{}]", a.name),
                    Some(&format!("{} {}", a.name, a.reason)),
                )
            })
            || metadata.iter().any(|(key, value)| {
                let text = value
                    .as_str()
                    .map_or_else(|| value.to_string(), str::to_string);
                probe(&own, kind, &format!("metadata.{key}"), Some(&text))
            })
            || card.values.iter().any(|v| {
                probe(
                    &own,
                    kind,
                    &format!("values.{}", v.value),
                    Some(&format!("{} {}", v.value, v.label)),
                )
            });
        for (index, rule) in card.rules.iter().enumerate() {
            probe(
                &own,
                kind,
                &format!("rules[{index}]"),
                Some(&rule_text(rule)),
            );
        }
        // Its lines: each by its kind, its row, the card it reaches and where, and what it
        // says.
        for (index, line) in card.connections.iter().enumerate() {
            let to = line.to.as_deref().and_then(|id| project.named(id));
            let fields =
                (!line.fields.is_empty()).then(|| Value::Object(line.fields.clone()).to_string());
            let said: Vec<String> = [
                Some(line.line.clone()),
                (!line.from.is_empty()).then(|| line.from.clone()),
                to,
                line.at.clone(),
                line.description.clone(),
                fields,
            ]
            .into_iter()
            .flatten()
            .collect();
            probe(
                &own,
                kind,
                &format!("connections[{index}]"),
                Some(&said.join(" ")),
            );
        }
        for line in super::facts::row_lines(&project, card) {
            probe(&own, kind, &line.place(), Some(&line.path));
        }
        probe(
            &own,
            kind,
            "body",
            (!card.body.is_empty()).then_some(card.body.as_str()),
        );
        for place in super::texts::inside(&project, card) {
            probe(&place.path, &place.kind, &place.place, Some(&place.text));
        }
        check::walk_fields(&card.fields, &mut |field, trail| {
            let path = format!(
                "{own}.{}",
                crate::pathtext::join_names(trail.iter().map(String::as_str))
            );
            let kind = field.kind.as_str();
            let _ = probe(&path, kind, "name", Some(&field.name))
                || probe(&path, kind, "label", Some(&field.label))
                || probe(&path, kind, "description", Some(&field.description));
            for (index, rule) in field.rules.iter().enumerate() {
                probe(
                    &path,
                    kind,
                    &format!("rules[{index}]"),
                    Some(&rule_text(rule)),
                );
            }
        });
        // A data card's values, each by where it is in its data (`data.lines[0].sku`).
        if let Some(data) = card.instance_data() {
            data_leaves(data, "data", &mut |at, text| {
                probe(&own, kind, at, Some(text));
            });
        }
        gather(&own, kind, &card.id);
    }
    // A card ghost is found by its name, unless the cards are narrowed to a kind or a container.
    for ghost in seen.detail.ghosts.iter().filter(|_| !narrowed) {
        if scope
            .as_ref()
            .is_some_and(|s| !s.contains(&project.section_of_ghost(ghost)))
        {
            continue;
        }
        let path = project.full_ghost(ghost);
        probe(&path, crate::detail::GHOST, "name", Some(&ghost.name));
        gather(&path, crate::detail::GHOST, &ghost.id);
    }
    let mut out = Map::new();
    out.insert("project".into(), seen.summary.name.clone().into());
    out.insert("matched".into(), cards.len().into());
    // Through MCP: from after the last card read, up to the page's count and size (`paging`).
    let Some(page) = page else {
        out.insert("cards".into(), Value::Array(cards));
        return Ok(seen.headed(Output::Json(Value::Object(out))));
    };
    let keys: Vec<String> = cards
        .iter()
        .map(|c| paging::key_of(c, Some("path")))
        .collect();
    let cut = paging::window(&keys, page, Order::Listed, MOST_CHARS, |i| {
        paging::chars(&cards[i])
    });
    for (key, value) in cut.said() {
        if key != "matched" {
            out.insert(key, value);
        }
    }
    out.insert(
        "cards".into(),
        Value::Array(cards.drain(cut.start..cut.end).collect()),
    );
    let out = Value::Object(out);
    let out = match cut.warning() {
        Some(w) => super::warned(out, &[w]),
        None => out,
    };
    Ok(seen.headed(Output::Json(out)))
}

/// Each value a data card holds, with where it is under `at` (`data.lines[0].sku`), as text: a
/// number or a truth as JSON writes it.
fn data_leaves(value: &Value, at: &str, each: &mut dyn FnMut(&str, &str)) {
    match value {
        Value::Object(map) => {
            for (key, inner) in map {
                data_leaves(inner, &crate::pathtext::member(at, key), each);
            }
        }
        Value::Array(items) => {
            for (i, inner) in items.iter().enumerate() {
                data_leaves(inner, &format!("{at}[{i}]"), each);
            }
        }
        Value::String(text) => each(at, text),
        Value::Null => {}
        other => each(at, &other.to_string()),
    }
}

/// `guide`'s `about` with `project`: a kind of card the project defines — its
/// definition, the fields and rows it holds whole, and how a card of it is written.
pub(crate) fn own_kind_guide(store: &Store, args: &Args) -> Result<Output, String> {
    let about = args
        .optional_string("about")
        .map(str::trim)
        .unwrap_or_default()
        .to_string();
    let summary = super::project_summary(store, args.string("project")?)?;
    let own = store.project(&summary.id)?.and_then(|p| p.kinds);
    if let Some(found) = super::own_kinds::kind_guide(&summary.name, own.as_deref(), &about) {
        return Ok(Output::Json(found));
    }
    let defs = crate::kinds::Defs::of_project(own.as_deref());
    let names: Vec<String> = defs
        .cards()
        .placeable()
        .filter(|k| !k.builtin)
        .map(|k| k.name.clone())
        .collect();
    let said = crate::mcp::guide(Some(&about))
        .err()
        .unwrap_or_else(|| text!("reads.guide.noDescription", about));
    Err(text!(
        "reads.guide.projectKinds",
        said,
        project = summary.name,
        kinds = if names.is_empty() {
            text!("walk.miss.none")
        } else {
            crate::text::list(&names)
        }
    ))
}

/// The kinds a call narrows to: a kind's name or a list of them (a card of any matches). A goal
/// was a custom card before.
fn kinds_given(given: &Map<String, Value>) -> Option<Vec<String>> {
    let named = |v: &Value| {
        let name = v.as_str()?;
        Some(if name == "custom_goal" { "goal" } else { name }.to_string())
    };
    match given.get("kind")? {
        Value::Array(list) => Some(list.iter().filter_map(named).collect()),
        one => named(one).map(|k| vec![k]),
    }
}

pub(crate) fn card_list(store: &Store, args: &Args) -> Result<Output, String> {
    let mut given = args.value()?;
    // From after the last one read (`paging`): a card's path, a place found's.
    let after = paging::after_of(given.shift_remove("after"))?;
    // The functions and types in the files: read from them now (`code`).
    if kinds_given(&given).is_some_and(|k| k.iter().any(|k| k == "symbol")) {
        return symbol_list(store, args, given, after);
    }
    // A kind the project defines is not among the form's: it is looked
    // for in the project once that is read.
    let own_kinds: Vec<String> = kinds_given(&given)
        .into_iter()
        .flatten()
        .filter(|k| k != "goal" && crate::kinds::builtin().get(k).is_none())
        .collect();
    let mut checked = given.clone();
    if !own_kinds.is_empty() {
        checked.shift_remove("kind");
    }
    check(&checked, "card_list", "list")?;
    let seen = Seen::read(
        store,
        args.string("project")?,
        args.optional_string("branch"),
    )?;
    if let Some(kind) = own_kinds.iter().find(|k| !seen.project().is_own_kind(k)) {
        let own: Vec<String> = seen
            .project()
            .defs
            .cards()
            .placeable()
            .filter(|k| !k.builtin)
            .map(|k| k.name.clone())
            .collect();
        return Err(text!(
            "reads.kind.unknown",
            kind,
            own = if own.is_empty() {
                text!("reads.kind.noOwn")
            } else {
                text!("reads.kind.orOwn", kinds = crate::text::list(&own))
            }
        ));
    }
    if given.contains_key("query") {
        let others: Vec<&str> = given
            .keys()
            .map(String::as_str)
            .filter(|k| {
                ![
                    "query",
                    "section",
                    "recursive",
                    "limit",
                    "kind",
                    "in",
                    "body",
                ]
                .contains(k)
            })
            .collect();
        if !others.is_empty() {
            return Err(text!(
                "reads.query.alone",
                others = crate::text::list(&others)
            ));
        }
        let mut given = given;
        // Without the bodies, the places found are said without the words around them.
        let excerpts = given.shift_remove("body").and_then(|b| b.as_bool()) != Some(false);
        let mut page = Page::take(&mut given, LIST_LIMIT, MOST_LIMIT)?;
        page.after = after.filter(|_| args.paged);
        return find(&seen, &given, excerpts, args.paged.then_some(&page));
    }
    // `body: false` lists the cards without their text: a list of many large bodies is cut to few
    // cards, and a list is often only to find a path.
    let with_body = given.shift_remove("body").and_then(|b| b.as_bool()) != Some(false);
    let projection = projection(&given, false)?;
    let base = base(&seen, &given)?;
    let project = seen.project();
    let layout = Layout::read(&store.conn, &seen.summary.id)?;
    let roots: Vec<String> = given
        .get("roots")
        .and_then(Value::as_array)
        .map(|r| {
            r.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    // The cards put only inside a container (a flow's nodes) are listed when they are asked
    // for: what a container holds (`in`), or their kind.
    let inner_asked = given.contains_key("in")
        || kinds_given(&given)
            .is_some_and(|kinds| kinds.iter().any(|k| crate::kinds::inner_only(k)));
    let mut chosen: Vec<&crate::model::Card> = seen
        .detail
        .schemas
        .iter()
        .map(|c| &**c)
        .chain(
            seen.detail
                .inner
                .iter()
                .filter(|_| inner_asked)
                .map(|c| &**c),
        )
        .collect();
    let scope = scope(&project, &given, base.as_ref())?;
    if given.get("recursive").and_then(Value::as_bool) == Some(true) {
        // Both read one section alone: roots by its names, region on its diagram.
        for alone in ["roots", "region"] {
            if given.contains_key(alone) {
                return Err(text!("reads.recursive.alone", alone));
            }
        }
    }
    if let Some(scope) = &scope
        && roots.is_empty()
    {
        chosen.retain(|c| scope.contains(&project.section_of(c)));
    }
    // A region of the section's diagram: the cards and card ghosts whose
    // boxes it takes — any of them, or the whole (`mode`).
    let in_region: Option<std::collections::HashSet<String>> = match given.get("region") {
        None => None,
        Some(raw) => {
            let region = super::boxes::region(raw)?;
            let Some(base) = &base else {
                return Err(text!("reads.region.needsSection"));
            };
            Some(
                super::grid::own_boxes(&project, &layout, base)
                    .into_iter()
                    .filter(|b| b.drawn)
                    .filter(|b| {
                        b.at.is_some_and(|(x, y)| region.takes((x, y, b.size.0, b.size.1)))
                    })
                    .map(|b| b.id)
                    .collect(),
            )
        }
    };
    if let Some(ids) = &in_region {
        chosen.retain(|c| ids.contains(&c.id));
    }
    if let Some(tags) = given.get("tags").and_then(Value::as_array)
        && !tags.is_empty()
    {
        let selected: Vec<String> = tags
            .iter()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect();
        let wanted: std::collections::HashSet<String> =
            super::check::expand_tags(&seen.detail.project.tag_groups, &selected)
                .into_iter()
                .collect();
        chosen.retain(|c| c.tags.iter().any(|t| wanted.contains(t)));
    }
    if let Some(kinds) = kinds_given(&given) {
        chosen.retain(|c| kinds.contains(&c.kind));
    }
    // goal and task as a list of issues: by the values of their fields, and what a goal
    // holds.
    if let Some(wanted) = given.get("fields") {
        super::board::matching(project.defs.cards(), &mut chosen, wanted)?;
    }
    if let Some(path) = given.get("in") {
        let path = path.as_str().ok_or_else(|| text!("reads.in.container"))?;
        let held = super::board::inside(&seen, path, base.as_deref())?;
        chosen.retain(|c| held.contains(&c.id));
    }
    super::board::in_planned_order(&mut chosen);
    by_section(&project, scope.as_deref(), &mut chosen);
    // A card inside a container right after its container, in the container's order: the
    // cards as a tree, as the containers hold them.
    in_tree_order(&project, &mut chosen);
    if !roots.is_empty() {
        let depth = given
            .get("depth")
            .and_then(Value::as_f64)
            .map_or(1, |d| d.trunc() as i64);
        if !(0..=10).contains(&depth) {
            return Err(text!("reads.depth.range", depth));
        }
        let mut ids = std::collections::HashSet::new();
        for name in &roots {
            let card = super::walk::find_card(&project, name, base.as_deref())
                .map_err(|why| text!("reads.roots.why", name, why))?;
            ids.insert(card.id.clone());
        }
        let reach = neighbourhood(&seen, ids, depth);
        chosen.retain(|c| reach.contains(&c.id));
    }
    // By the lines to working directories' files: the cards none leaves (or rows of them none
    // leaves), the files none reaches, or those whose code changed or was never recorded.
    let implementation = match given.get("implementation") {
        Some(raw) => Some(super::coverage::Wanted::of(raw)?),
        None => None,
    };
    let said_code: std::collections::HashMap<String, Map<String, Value>> = match &implementation {
        Some(wanted) => {
            let found =
                super::coverage::cards(&store.conn, &seen.summary.id, &project, &chosen, wanted)?;
            chosen.retain(|c| found.contains_key(&c.id));
            found
        }
        None => Default::default(),
    };
    // The card ghosts, after the cards: by section and kind alone (they have no tags, no
    // fields, no lines of their own).
    let narrowed = ["tags", "fields", "in", "roots", "implementation"]
        .iter()
        .any(|k| {
            given
                .get(*k)
                .is_some_and(|v| !v.as_array().is_some_and(Vec::is_empty))
        });
    let wanted_kind = kinds_given(&given);
    let ghosts: Vec<&crate::model::Ghost> =
        if narrowed || wanted_kind.is_some_and(|k| !k.iter().any(|k| k == crate::detail::GHOST)) {
            Vec::new()
        } else {
            let mut ghosts: Vec<&crate::model::Ghost> = seen
                .detail
                .ghosts
                .iter()
                .filter(|g| {
                    scope
                        .as_ref()
                        .is_none_or(|s| s.contains(&project.section_of_ghost(g)))
                })
                .filter(|g| in_region.as_ref().is_none_or(|ids| ids.contains(&g.id)))
                .collect();
            if let Some(scope) = scope.as_ref().filter(|s| s.len() > 1) {
                ghosts.sort_by_key(|g| {
                    let at = project.section_of_ghost(g);
                    scope.iter().position(|s| *s == at)
                });
            }
            ghosts
        };
    let limit = given
        .get("limit")
        .and_then(Value::as_f64)
        .map_or(20, |d| d.trunc() as i64);
    if !(1..=200).contains(&limit) {
        return Err(text!("reads.limit.range", limit));
    }
    let matched = chosen.len() + ghosts.len();
    let mut out = Map::new();
    out.insert("project".into(), seen.summary.name.clone().into());
    out.insert("matched".into(), matched.into());
    // The actions of the goals and tasks listed, worked out over the one project read for the whole
    // list (a branch a goal names is compared with main once), for the cards given back only.
    let with_actions = given
        .get("include")
        .and_then(Value::as_array)
        .is_some_and(|l| l.iter().any(|v| v.as_str() == Some(ACTIONS)));
    let works = super::actions::Works::new(&seen.named.tree, true);
    let reader = store.reader();
    let failed: std::cell::RefCell<Option<String>> = std::cell::RefCell::new(None);
    // Each with its rev, so a card listed is written without reading it again.
    let view = |i: usize| {
        let (mut made, id) = match chosen.get(i) {
            Some(c) => {
                let mut made = seen.card_view(c, &projection, Some(&layout));
                if let Some(extra) = said_code.get(&c.id)
                    && let Value::Object(object) = &mut made
                {
                    object.extend(extra.clone());
                }
                if with_actions {
                    match super::actions::listed(&reader, &seen.summary.id, &project, c, &works) {
                        Ok(Some(listed)) => {
                            if let Value::Object(object) = &mut made {
                                object.insert(ACTIONS.into(), listed);
                            }
                        }
                        Ok(None) => {}
                        Err(e) => *failed.borrow_mut() = Some(e),
                    }
                }
                (made, &c.id)
            }
            None => {
                let g = ghosts[i - chosen.len()];
                (seen.ghost_view(g, &projection, Some(&layout)), &g.id)
            }
        };
        if let Some(rev) = seen.rev(id) {
            super::revs::stamp(&mut made, &rev);
        }
        if !with_body && let Value::Object(object) = &mut made {
            object.shift_remove("body");
        }
        made
    };
    // A list is read on from after the last one read (`after`) whoever asks; MCP's is also cut by the
    // characters it is answered in (`paging`), and its notes' bodies.
    let keys: Vec<String> = chosen
        .iter()
        .map(|c| project.full(c))
        .chain(ghosts.iter().map(|g| project.full_ghost(g)))
        .collect();
    let page = Page {
        limit: limit as usize,
        after,
    };
    if !args.paged {
        let cut = paging::window(&keys, &page, Order::Listed, usize::MAX, |_| 0);
        if let Some(next) = &cut.next {
            out.insert("returned".into(), (cut.end - cut.start).into());
            out.insert("truncated".into(), true.into());
            out.insert("next".into(), next.clone().into());
        }
        out.insert(
            "cards".into(),
            Value::Array((cut.start..cut.end).map(view).collect()),
        );
        if let Some(e) = failed.into_inner() {
            return Err(e);
        }
        let out = Value::Object(out);
        let out = match cut.warning() {
            Some(w) => super::warned(out, &[w]),
            None => out,
        };
        return Ok(seen.headed(Output::Json(out)));
    }
    // Through MCP: from after `after`, up to `limit` cards and the characters an answer is given in
    // (`paging`), each card's view made once.
    let mut views: Vec<Option<Value>> = vec![None; matched];
    let cut = paging::window(&keys, &page, Order::Listed, MOST_CHARS, |i| {
        let mut made = view(i);
        // A note's body as card_read gives it: its first lines (`in_part::cut`).
        super::in_part::cut(&mut made, paging::BODY_LINES);
        let size = paging::chars(&made);
        views[i] = Some(made);
        size
    });
    if let Some(next) = &cut.next {
        out.insert("returned".into(), (cut.end - cut.start).into());
        out.insert("truncated".into(), true.into());
        out.insert("next".into(), next.clone().into());
    }
    out.insert(
        "cards".into(),
        Value::Array(views.drain(cut.start..cut.end).flatten().collect()),
    );
    if let Some(e) = failed.into_inner() {
        return Err(e);
    }
    let out = Value::Object(out);
    let out = match cut.warning() {
        Some(w) => super::warned(out, &[w]),
        None => out,
    };
    Ok(seen.headed(Output::Json(out)))
}

/// `card_list` of `kind: symbol`: the functions and types of the working directories' files — in the
/// section named and every one inside it, or in every working directory — each file read now, those
/// of `of` alone and those whose name has `name` in it; paged as cards are.
fn symbol_list(
    store: &Store,
    args: &Args,
    mut given: Map<String, Value>,
    after: Option<String>,
) -> Result<Output, String> {
    if let Some(key) = given.keys().find(|k| {
        ![
            "kind",
            "section",
            "recursive",
            "of",
            "name",
            "limit",
            "implementation",
        ]
        .contains(&k.as_str())
    }) {
        return Err(text!("code.list.key", key));
    }
    if kinds_given(&given).is_some_and(|k| k.len() > 1) {
        return Err(text!("code.list.kindAlone"));
    }
    let of = match given.shift_remove("of") {
        None => None,
        Some(Value::String(word)) => {
            Some(crate::code::symbols::Of::of_word(&word).ok_or_else(|| {
                text!(
                    "code.list.of",
                    word,
                    words = crate::text::List::of(
                        crate::code::symbols::Of::ALL.iter().map(|o| o.word())
                    )
                )
            })?)
        }
        Some(other) => return Err(text!("code.list.ofString", other)),
    };
    let name = match given.shift_remove("name") {
        None => None,
        Some(Value::String(n)) if !n.is_empty() => Some(n),
        Some(other) => return Err(text!("code.list.nameString", other)),
    };
    let limit = given
        .get("limit")
        .and_then(Value::as_f64)
        .map_or(20, |d| d.trunc() as i64);
    if !(1..=200).contains(&limit) {
        return Err(text!("reads.limit.range", limit));
    }
    let _whole = crate::workdir::Whole::at_least(crate::workdir::Reach::In);
    let seen = Seen::read(
        store,
        args.string("project")?,
        args.optional_string("branch"),
    )?;
    let project = seen.project();
    let base = base(&seen, &given)?;
    // In the section named and every one inside it.
    let mut given_scope = given.clone();
    if base.is_some() {
        given_scope.insert("recursive".into(), true.into());
    }
    let scope = scope(&project, &given_scope, base.as_ref())?;
    let files: Vec<&crate::model::Card> = seen
        .detail
        .schemas
        .iter()
        .map(|c| &**c)
        .filter(|c| c.kind == "file")
        .filter(|c| {
            scope
                .as_ref()
                .is_none_or(|s| s.contains(&project.section_of(c)))
        })
        .collect();
    let implementation = match given.get("implementation") {
        Some(raw) => Some(super::coverage::Wanted::of(raw)?),
        None => None,
    };
    let landing = match &implementation {
        Some(wanted) => Some(super::coverage::Landing::of(
            &store.conn,
            &seen.summary.id,
            &project,
            wanted,
        )?),
        None => None,
    };
    let (rows, warnings) =
        super::code::listed_symbols(&project, &files, of, name.as_deref(), landing.as_ref());
    let keys: Vec<String> = rows
        .iter()
        .map(|r| r["path"].as_str().unwrap_or_default().to_string())
        .collect();
    let page = Page {
        limit: limit as usize,
        after,
    };
    let most = if args.paged { MOST_CHARS } else { usize::MAX };
    let cut = paging::window(&keys, &page, Order::Listed, most, |i| {
        paging::chars(&rows[i])
    });
    let mut out = Map::new();
    out.insert("project".into(), seen.summary.name.clone().into());
    out.insert("matched".into(), rows.len().into());
    if let Some(next) = &cut.next {
        out.insert("returned".into(), (cut.end - cut.start).into());
        out.insert("truncated".into(), true.into());
        out.insert("next".into(), next.clone().into());
    }
    out.insert(
        "cards".into(),
        Value::Array(rows[cut.start..cut.end].to_vec()),
    );
    let mut said = warnings;
    said.extend(cut.warning());
    Ok(seen.headed(Output::Json(super::warned(Value::Object(out), &said))))
}

/// Cards taken from more sections than one, grouped by their sections in `scope`'s order, each
/// group as it was.
fn by_section(
    project: &crate::export::Project,
    scope: Option<&[String]>,
    cards: &mut [&crate::model::Card],
) {
    if let Some(scope) = scope.filter(|s| s.len() > 1) {
        cards.sort_by_key(|c| {
            let at = project.section_of(c);
            scope.iter().position(|s| *s == at)
        });
    }
}

/// Cards in tree order: each as it was, but a card inside a container is put right after
/// its container (or after the cards before it inside it), when its container is listed too.
fn in_tree_order<'a>(
    project: &crate::export::Project<'a>,
    cards: &mut Vec<&'a crate::model::Card>,
) {
    let listed: std::collections::HashSet<&str> = cards.iter().map(|c| c.id.as_str()).collect();
    let held = |c: &crate::model::Card| {
        c.inside
            .as_deref()
            .is_some_and(|container| listed.contains(container))
    };
    let tops: Vec<&crate::model::Card> = cards.iter().copied().filter(|c| !held(c)).collect();
    let mut out: Vec<&crate::model::Card> = Vec::new();
    fn put<'a>(
        card: &'a crate::model::Card,
        project: &crate::export::Project<'a>,
        listed: &std::collections::HashSet<&str>,
        out: &mut Vec<&'a crate::model::Card>,
    ) {
        if out.iter().any(|c| c.id == card.id) {
            return;
        }
        out.push(card);
        for inner in project.inner_of(&card.id) {
            if listed.contains(inner.id.as_str()) {
                put(inner, project, listed, out);
            }
        }
    }
    for card in tops {
        put(card, project, &listed, &mut out);
    }
    *cards = out;
}

/// The project's tag groups: name, tags, exclusive.
pub(crate) fn tag_groups(seen: &Seen) -> Vec<(String, Vec<String>, bool)> {
    seen.raw["project"]["tagGroups"]
        .as_array()
        .map(|groups| {
            groups
                .iter()
                .map(|g| {
                    (
                        g["name"].as_str().unwrap_or_default().to_string(),
                        g["tags"]
                            .as_array()
                            .map(|t| {
                                t.iter()
                                    .filter_map(|v| v.as_str().map(str::to_string))
                                    .collect()
                            })
                            .unwrap_or_default(),
                        g["exclusive"].as_bool().unwrap_or(false),
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Cards within `depth` hops of the roots, following relations either way
/// (`neighbourhood`).
pub(crate) fn neighbourhood(
    seen: &Seen,
    roots: std::collections::HashSet<String>,
    depth: i64,
) -> std::collections::HashSet<String> {
    if depth <= 0 || roots.is_empty() {
        return roots;
    }
    let mut keep = roots.clone();
    let mut frontier = roots;
    for _ in 0..depth {
        let mut next = std::collections::HashSet::new();
        for r in &seen.detail.relations {
            if frontier.contains(&r.from_schema_id) && !keep.contains(&r.to_schema_id) {
                next.insert(r.to_schema_id.clone());
            }
            if frontier.contains(&r.to_schema_id) && !keep.contains(&r.from_schema_id) {
                next.insert(r.from_schema_id.clone());
            }
        }
        if next.is_empty() {
            break;
        }
        keep.extend(next.iter().cloned());
        frontier = next;
    }
    keep
}

/// The project's tags with how often each is used, and its groups (`list_tags`).
pub(crate) fn tag_list(store: &Store, args: &Args) -> Result<Output, String> {
    let seen = Seen::read(
        store,
        args.string("project")?,
        args.optional_string("branch"),
    )?;
    let used = super::check::project_tags(&seen.detail.schemas);
    let groups = tag_groups(&seen);
    let grouped: std::collections::HashSet<&str> = groups
        .iter()
        .flat_map(|(_, t, _)| t.iter().map(String::as_str))
        .collect();
    let row = |tag: &str, count: usize| {
        let mut m = Map::new();
        m.insert("tag".into(), tag.into());
        m.insert("count".into(), count.into());
        Value::Object(m)
    };
    let mut out = Map::new();
    out.insert(
        "tags".into(),
        Value::Array(used.iter().map(|(t, n)| row(t, *n)).collect()),
    );
    out.insert(
        "groups".into(),
        Value::Array(
            groups
                .iter()
                .map(|(name, tags, _)| {
                    let mut g = Map::new();
                    g.insert("name".into(), name.clone().into());
                    g.insert(
                        "tags".into(),
                        Value::Array(
                            tags.iter()
                                .map(|t| {
                                    row(t, used.iter().find(|(u, _)| u == t).map_or(0, |u| u.1))
                                })
                                .collect(),
                        ),
                    );
                    Value::Object(g)
                })
                .collect(),
        ),
    );
    let used_names: std::collections::HashSet<&str> =
        used.iter().map(|(t, _)| t.as_str()).collect();
    let mut declared: Vec<&str> = grouped
        .iter()
        .copied()
        .filter(|t| !used_names.contains(t))
        .collect();
    declared.sort();
    out.insert(
        "declared".into(),
        Value::Array(declared.into_iter().map(Value::from).collect()),
    );
    out.insert(
        "ungrouped".into(),
        Value::Array(
            used.iter()
                .filter(|(t, _)| !grouped.contains(t.as_str()))
                .map(|(t, _)| Value::from(t.clone()))
                .collect(),
        ),
    );
    Ok(seen.headed(Output::Json(Value::Object(out))))
}

/// A tree's history in bytes: its id (none for main), its name, the bytes.
pub(crate) type TreeSize = (Option<String>, String, i64);

/// How much of the store a project takes (`projectSize`, `ProjectSize`): the bytes its documents
/// hold, main's and the branches', and its history per tree — main first, then each
/// branch as (its id, its name, bytes) — the lengths of what its rows hold, not the
/// file's size. The board's history is not counted, as the Swift app counts it.
pub(crate) fn project_size_of(
    conn: &rusqlite::Connection,
    project: &str,
) -> Result<(i64, Vec<TreeSize>), String> {
    let one = |query: &str| -> Result<i64, String> {
        conn.query_row(query, [project], |r| r.get(0))
            .map_err(crate::store::sql)
    };
    let mut content = 0;
    for table in ["card_docs", "section_docs"] {
        content += one(&format!(
            "SELECT COALESCE(SUM(COALESCE(LENGTH(CAST(doc AS BLOB)), 0) + COALESCE(LENGTH(CAST(base AS BLOB)), 0)), 0) FROM {table} WHERE project_id = ?"
        ))?;
    }
    let mut history: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
    for query in [
        "SELECT c.branch_id, SUM(COALESCE(LENGTH(CAST(r.before AS BLOB)), 0)) FROM commit_rows r JOIN commits c ON c.id = r.commit_id WHERE c.project_id = ? GROUP BY c.branch_id",
        "SELECT c.branch_id, SUM(LENGTH(CAST(l.summary AS BLOB))) FROM commit_lines l JOIN commits c ON c.id = l.commit_id WHERE c.project_id = ? GROUP BY c.branch_id",
    ] {
        let mut stmt = conn.prepare(query).map_err(crate::store::sql)?;
        let rows = stmt
            .query_map([project], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
            })
            .map_err(crate::store::sql)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(crate::store::sql)?;
        for (tree, bytes) in rows {
            *history.entry(tree).or_default() += bytes;
        }
    }
    let mut trees = vec![(
        None,
        "main".to_string(),
        history.get("").copied().unwrap_or(0),
    )];
    for (id, name) in super::branches(conn, project)? {
        let bytes = history.get(&id).copied().unwrap_or(0);
        trees.push((Some(id), name, bytes));
    }
    Ok((content, trees))
}

/// How much of the store the project takes, as `project_read` says it.
fn project_size(store: &Store, project: &str) -> Result<Value, String> {
    let (content, trees) = project_size_of(&store.conn, project)?;
    let mut by = Map::new();
    for (_, name, bytes) in &trees {
        by.insert(name.clone(), (*bytes).into());
    }
    let mut out = Map::new();
    out.insert("content".into(), content.into());
    out.insert(
        "history".into(),
        trees.iter().map(|t| t.2).sum::<i64>().into(),
    );
    out.insert("historyByBranch".into(), Value::Object(by));
    Ok(Value::Object(out))
}

/// The project itself — its description, settings, how many cards of each kind main has, its
/// tags and what is directly under each root — or its writing rules (`project_read`). What is wrong in a tree, and its
/// diagram, are `branch_read`'s.
pub(crate) fn project_read(store: &Store, args: &Args) -> Result<Output, String> {
    let given = args.value()?;
    let seen = Seen::read(store, args.string("project")?, None)?;
    if given.get("as").and_then(Value::as_str) == Some("rules") {
        check(&given, "project_read", "project（as: rules）")?;
        let rules: Map<String, Value> = store
            .conn
            .query_row(
                "SELECT rules FROM projects WHERE id = ?",
                [&seen.summary.id],
                |r| r.get::<_, Option<String>>(0),
            )
            .map_err(crate::store::sql)?
            .and_then(|t| serde_json::from_str::<Value>(&t).ok())
            .and_then(|v| v.as_object().cloned())
            .unwrap_or_default();
        let place = given.get("place").and_then(Value::as_str);
        return Ok(Output::Json(super::rules::view(
            &seen.summary.name,
            &rules,
            place,
        )?));
    }
    check(&given, "project_read", "project")?;
    let view = super::summary::project_view(&seen);
    let Value::Object(object) = view else {
        return Ok(Output::Json(view));
    };
    let (archived, locked, ai_merge, export_language): (
        Option<String>,
        Option<String>,
        bool,
        String,
    ) = store
        .conn
        .query_row(
            "SELECT archived_at, locked_at, ai_merge, export_language FROM projects WHERE id = ?",
            [&seen.summary.id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get::<_, i64>(2)? != 0, r.get(3)?)),
        )
        .map_err(crate::store::sql)?;
    let layout = Layout::read(&store.conn, &seen.summary.id)?;
    // What MCP may do only when a person says so each time, and what it may not: none said when it
    // may do everything.
    let asked = super::mcp_writes::in_state(&store.conn, &seen.summary.id, State::Ask)?;
    let denied = super::mcp_writes::in_state(&store.conn, &seen.summary.id, State::Deny)?;
    let mut out = Map::new();
    for (key, value) in object {
        let at = key == "description"
            || (key == "project" && seen.detail.project.description.is_empty());
        out.insert(key, value);
        if !at {
            continue;
        }
        if let Some(a) = &archived {
            out.insert(
                "archived".into(),
                text!("reads.archived.readOnly", archived = a).into(),
            );
        }
        if locked.is_some() {
            out.insert("locked".into(), true.into());
            // An AI's merge into it goes in without being approved (the project's settings).
            if ai_merge {
                out.insert("aiMerge".into(), true.into());
            }
        }
        if let Some(level) = Sight::of_call(&store.conn)?.level(&seen.summary.id) {
            out.insert("access".into(), level.word().into());
        }
        if !asked.is_empty() {
            out.insert("mcpAsk".into(), asked.clone().into());
        }
        if !denied.is_empty() {
            out.insert("mcpDenied".into(), denied.clone().into());
        }
        out.insert("exportLanguage".into(), export_language.clone().into());
    }
    out.insert("layout".into(), layout.view());
    out.insert("size".into(), project_size(store, &seen.summary.id)?);
    Ok(Output::Json(Value::Object(out)))
}
