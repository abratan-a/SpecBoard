//! A card's lines written (`connections`): read from MCP's shape into
//! the card's document, held to the project's kinds of line — where each may leave and what it
//! may reach (`from`, `to`), how many one place may have (`max`), what an embedding asks of
//! its field — and the keys a line was written in before refused, with how to write it now.
//!
//! Every write that changes a card's lines comes here, the window's and MCP's alike, so
//! what one lets through the other does.

use super::Seen;
use super::cards::{Resolver, Scope};
use super::walk::{Place, walk};
use crate::export::Project;
use crate::kinds::{Base, FIELD_ROWS, INSTANCE, LineDef, LinePlace};
use crate::model::Card;
use crate::write::Tx;
use serde_json::{Map, Value};

// MARK: - The keys of before

/// What a key a line was written in before is told now: how to write the line.
pub(crate) fn gone(key: &str) -> Option<String> {
    Some(match key {
        "ref" => text!("lines.gone.ref"),
        "pointsAt" => text!("lines.gone.pointsAt"),
        "holds" => text!("lines.gone.holds"),
        "notes" => text!("lines.gone.notes"),
        "links" => text!("lines.gone.links"),
        "payloads" => text!("lines.gone.payloads"),
        _ => return None,
    })
}

/// A field's type `ref`, refused: an embedding is a line.
pub(crate) fn ref_type() -> String {
    text!("lines.refType")
}

/// A field's value refused for what it says of its lines: `ref`, `pointsAt`, `type: ref`.
pub(crate) fn refuse_field(value: &Map<String, Value>) -> Result<(), String> {
    for key in ["ref", "pointsAt"] {
        if value.contains_key(key) {
            return Err(gone(key).unwrap_or_default());
        }
    }
    if value.get("type").and_then(Value::as_str) == Some("ref") {
        return Err(ref_type());
    }
    Ok(())
}

/// A card's value refused for what it says of its lines in the keys of before — its own
/// (`holds`, `notes`, `links`, `payloads`) and its fields'.
pub(crate) fn refuse_card(value: &Map<String, Value>) -> Result<(), String> {
    for key in ["holds", "notes", "links", "payloads"] {
        if value.contains_key(key) {
            return Err(gone(key).unwrap_or_default());
        }
    }
    for field in value
        .get("fields")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_object)
    {
        refuse_field(field)?;
    }
    Ok(())
}

// MARK: - Lines to the board

/// The words around the path in the refusal of a path that names no card, in the language of the
/// call: said once with a mark where the path goes, so a refusal of that kind is found in any
/// language.
fn no_card_frame() -> Option<(String, String)> {
    const MARK: &str = "\u{1}";
    crate::text::render(
        "kindConvert.problem.noCard",
        &[("path", &MARK as &dyn std::fmt::Display)],
    )
    .split_once(MARK)
    .filter(|(open, close)| !open.is_empty() && !close.is_empty())
    .map(|(open, close)| (open.to_string(), close.to_string()))
}

/// The paths a refusal says have no card, each with where its words begin and end in it.
fn unfound_paths<'e>(error: &'e str, open: &str, close: &str) -> Vec<(usize, usize, &'e str)> {
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(i) = error[from..].find(open).map(|i| from + i) {
        let after = i + open.len();
        let Some(end) = error[after..].find(close).map(|j| after + j) else {
            break;
        };
        // The last opening before a closing is the one it closes.
        let start = error[i..end].rfind(open).map_or(i, |k| i + k);
        out.push((start, end + close.len(), &error[start + open.len()..end]));
        from = end + close.len();
    }
    out
}

/// What a line from main (or a branch) to a card of the board is refused with: a card the board
/// has is no card missing — main's lines do not reach the board, the board's reach main.
/// The board is read only
/// when a write was refused for a card it could not find.
pub(crate) fn to_the_board(tx: &Tx, scope: &Scope, error: String) -> String {
    if scope.tree == crate::store::BOARD {
        return error;
    }
    let Some((open, close)) = no_card_frame() else {
        return error;
    };
    let unfound = unfound_paths(&error, &open, &close);
    if unfound.is_empty() {
        return error;
    }
    let Ok(board) = crate::detail::read(&tx.reader(), &scope.project, Some(crate::store::BOARD))
        .and_then(|raw| crate::model::typed(&raw))
    else {
        return error;
    };
    let board = Project::new(&board);
    let mut out = String::new();
    let mut done = 0;
    for (start, end, path) in unfound {
        if crate::pathtext::has_section(path) && board.written_id(path, board.home()).is_some() {
            out.push_str(&error[done..start]);
            out.push_str(&if scope.tree.is_empty() {
                text!("lines.toBoard.fromMain", path)
            } else {
                text!("lines.toBoard.fromBranch", path, name = scope.tree_name)
            });
            done = end;
        }
    }
    out.push_str(&error[done..]);
    out
}

// MARK: - Written into the document

/// The card's lines as `value` writes them (`connections`, in its place), read into the
/// document: each path by the id it names, an id kept for a line written as one there is.
/// A field an embedding leaves goes back to no type of its own (the line says it; taken
/// off, it is a `string`).
pub(crate) fn put_into(
    doc: &mut Map<String, Value>,
    written: &Value,
    resolver: &Resolver,
) -> Result<(), String> {
    let prior = Value::Object(doc.clone());
    // The window's lines written in the document's form (ids, kept from what was there)
    // and MCP's in paths both read: an id that is a card's is kept as it is.
    let names = Either { resolver };
    let lines = crate::kinds::connections_from_wire(
        written,
        Some(&prior),
        &names,
        &mut crate::write::new_id,
    )
    .map_err(|problems| {
        problems
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n")
    })?;
    // An embedding: its field holds nothing of its own beside it.
    for line in &lines {
        let from = line.get("from").and_then(Value::as_str).unwrap_or_default();
        if line.get("line").and_then(Value::as_str).is_none() || from.is_empty() {
            continue;
        }
        let embeds = resolver
            .project
            .defs
            .line(line["line"].as_str().unwrap_or_default())
            .is_some_and(|d| d.base == Base::Embed);
        if !embeds {
            continue;
        }
        let names = crate::pathtext::names(from);
        let names: Vec<&str> = names.iter().map(String::as_str).collect();
        if let Some(fields) = field_at(doc, &names) {
            match fields.get("type").and_then(Value::as_str) {
                Some("object") => {
                    return Err(text!("lines.embedObject", from));
                }
                Some(_) => {
                    fields.shift_remove("type");
                }
                None => {}
            }
        }
    }
    if lines.is_empty() {
        doc.shift_remove("connections");
    } else {
        doc.insert("connections".into(), Value::Array(lines));
    }
    Ok(())
}

/// How `card_update` writes a card's lines, said where they are written otherwise.
pub(crate) fn shape() -> String {
    text!("lines.shape")
}

/// The keys a line is told apart by when it is added or taken away.
const PICKED_BY: [&str; 4] = ["line", "from", "to", "at"];

/// What `card_update` writes of a card's lines: `{add: [...], remove: [...]}` — the lines `remove` names taken
/// away first, then `add`'s put after them — as the whole list `put_into` writes. A line
/// `remove` names is told by the keys it gives (`line`, `from`, `to`, `at`, `description`, and
/// of `fields` the ones it writes; any left out match all), and is to be one of the card's
/// alone. A line `add` writes is one the card has already only when its fields are the same
/// too: two lines between the same rows may differ in what they say. The window writes its
/// whole list (`whole_ok`); MCP writes the lines it adds or takes away, not the list again.
pub(crate) fn edited(
    doc: &Map<String, Value>,
    written: &Value,
    resolver: &Resolver,
    whole_ok: bool,
) -> Result<Value, String> {
    let edits = match written {
        Value::Array(_) if whole_ok => return Ok(written.clone()),
        Value::Object(edits) => edits,
        _ => return Err(shape()),
    };
    if let Some(other) = edits.keys().find(|k| *k != "add" && *k != "remove") {
        return Err(text!("lines.edit.noKey", other, shape = shape()));
    }
    let list = |key: &str| -> Result<Vec<Map<String, Value>>, String> {
        match edits.get(key) {
            None => Ok(Vec::new()),
            Some(Value::Array(items)) => items
                .iter()
                .enumerate()
                .map(|(i, item)| {
                    item.as_object()
                        .cloned()
                        .ok_or_else(|| text!("lines.edit.notObject", key, i))
                })
                .collect(),
            Some(_) => Err(text!("lines.edit.notList", key, shape = shape())),
        }
    };
    let (adding, removing) = (list("add")?, list("remove")?);
    let names = Either { resolver };
    // A line as the document keeps it: its card by id, where it lands as written there.
    let read = |line: &Map<String, Value>| -> Result<Map<String, Value>, String> {
        let mut bare = line.clone();
        let gone = bare.get("to").is_some_and(Value::is_null);
        if gone {
            bare.shift_remove("to");
        }
        let said = crate::kinds::connections_from_wire(
            &Value::Array(vec![Value::Object(bare)]),
            None,
            &names,
            &mut || String::new(),
        )
        .map_err(|problems| {
            problems
                .iter()
                .map(|p| {
                    p.to_string().replacen("connections[0].", "", 1).replacen(
                        "connections[0]: ",
                        "",
                        1,
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        })?;
        let mut out = said
            .into_iter()
            .next()
            .and_then(|v| v.as_object().cloned())
            .unwrap_or_default();
        if gone {
            out.insert("to".into(), Value::Null);
        }
        Ok(out)
    };
    let text = |line: &Map<String, Value>, key: &str| -> String {
        line.get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    let gone_to = |line: &Map<String, Value>| {
        line.get("to")
            .and_then(Value::as_str)
            .is_none_or(|to| crate::kinds::Names::path(&names, to).is_none())
    };
    let mut kept: Vec<Map<String, Value>> =
        crate::kinds::connections_of(&Value::Object(doc.clone()))
            .iter()
            .filter_map(|l| l.as_object().cloned())
            .collect();
    let said_of = |line: &Map<String, Value>| -> String {
        let shown = crate::kinds::connections_to_wire(
            &serde_json::json!({ "connections": [line] }),
            &names,
        )
        .and_then(|l| l.get(0).cloned())
        .unwrap_or(Value::Null);
        crate::ojson::compact(&shown)
    };
    for (i, wanted) in removing.iter().enumerate() {
        if wanted.keys().all(|k| !PICKED_BY.contains(&k.as_str())) {
            return Err(text!("lines.edit.removeNeeds", i));
        }
        let as_kept = read(wanted).map_err(|e| format!("connections.remove[{i}]: {e}"))?;
        let matches: Vec<usize> = kept
            .iter()
            .enumerate()
            .filter(|(_, line)| {
                PICKED_BY.iter().all(|key| {
                    if !wanted.contains_key(*key) {
                        return true;
                    }
                    if *key == "to" && as_kept.get("to").is_some_and(Value::is_null) {
                        return gone_to(line);
                    }
                    text(line, key) == text(&as_kept, key)
                }) && (!wanted.contains_key("description")
                    || text(line, "description") == text(&as_kept, "description"))
                    && fields_given(line, &as_kept)
            })
            .map(|(j, _)| j)
            .collect();
        match matches.as_slice() {
            [one] => {
                kept.remove(*one);
            }
            [] => {
                return Err(text!(
                    "lines.edit.removeNone",
                    i,
                    lines = crate::text::List::of(kept.iter().map(said_of))
                ));
            }
            many => {
                return Err(text!(
                    "lines.edit.removeMany",
                    i,
                    n = many.len(),
                    lines = crate::text::List::of(many.iter().map(|j| said_of(&kept[*j])))
                ));
            }
        }
    }
    let mut whole: Vec<Value> = crate::kinds::connections_to_wire(
        &serde_json::json!({ "connections": kept.clone() }),
        &names,
    )
    .and_then(|l| l.as_array().cloned())
    .unwrap_or_default();
    for (i, line) in adding.iter().enumerate() {
        let as_kept = read(line).map_err(|e| format!("connections.add[{i}]: {e}"))?;
        if kept.iter().any(|k| {
            PICKED_BY
                .iter()
                .all(|key| text(k, key) == text(&as_kept, key))
                && k.get("fields") == as_kept.get("fields")
        }) {
            return Err(text!(
                "lines.edit.alreadyThere",
                i,
                line = said_of(&as_kept)
            ));
        }
        kept.push(as_kept);
        whole.push(Value::Object(line.clone()));
    }
    Ok(Value::Array(whole))
}

/// Whether a line has each of the fields `wanted` writes, as written there: the fields a
/// line to take away is told by are the ones the call names, the rest match all. A field
/// written `null` matches a line without it.
fn fields_given(line: &Map<String, Value>, wanted: &Map<String, Value>) -> bool {
    let Some(Value::Object(said)) = wanted.get("fields") else {
        return true;
    };
    let has = line.get("fields").and_then(Value::as_object);
    said.iter().all(|(key, value)| {
        let there = has.and_then(|f| f.get(key)).unwrap_or(&Value::Null);
        there == value
    })
}

/// Names for lines written either way: a path, or the id of a card the project has.
struct Either<'r, 'p, 'a> {
    resolver: &'r Resolver<'p, 'a>,
}

impl crate::kinds::Names for Either<'_, '_, '_> {
    fn path(&self, id: &str) -> Option<String> {
        crate::kinds::Names::path(self.resolver, id)
    }

    fn unmade(&self, path: &str) -> Option<String> {
        crate::kinds::Names::unmade(self.resolver, path)
    }

    fn why_not(&self, path: &str) -> Option<String> {
        if path.trim().starts_with("file:") {
            let now = self.resolver.project.file_form_now(path);
            return Some(text!(
                "lines.fileForm",
                path,
                now = now.unwrap_or_else(|| text!("lines.fileFormShape"))
            ));
        }
        self.resolver.project.file_problem(path)
    }

    fn section_path(&self, id: &str) -> Option<String> {
        crate::kinds::Names::section_path(self.resolver, id)
    }

    fn section_id(&self, path: &str) -> Option<String> {
        crate::kinds::Names::section_id(self.resolver, path)
    }

    fn inside_file(&self, path: &str) -> Option<(String, String)> {
        // The board's card's line reaches a function in a file as main's does: through main.
        self.resolver.project.inside_file(path).or_else(|| {
            self.resolver
                .project
                .outside
                .as_deref()
                .and_then(|o| o.inside_file(path))
        })
    }

    fn id(&self, path: &str) -> Option<String> {
        if self.resolver.project.named(path).is_some() || self.resolver.made.contains_key(path) {
            return Some(path.to_string());
        }
        // A card ghost that leads nowhere now is still the ghost a line was written to.
        crate::kinds::Names::id(self.resolver, path)
            .or_else(|| {
                match self
                    .resolver
                    .project
                    .find_named(path, &self.resolver.base)?
                {
                    crate::export::Named::Ghost(g) => Some(g.id.clone()),
                    crate::export::Named::Card(_) => None,
                }
            })
            // A board's card's line to main's card, by its path in full.
            .or_else(|| self.resolver.project.outside_id(path))
            // main's or a branch's to a working directory's file, there or not yet (`file:…`).
            .or_else(|| self.resolver.project.file_id(path))
    }
}

/// The fields of the field row at `names` in a record's document.
fn field_at<'d>(
    doc: &'d mut Map<String, Value>,
    names: &[&str],
) -> Option<&'d mut Map<String, Value>> {
    let mut list = doc.get_mut("rows")?.get_mut("fields")?.as_array_mut()?;
    let (last, above) = names.split_last()?;
    for name in above {
        let row = list
            .iter_mut()
            .find(|r| r["fields"]["name"].as_str() == Some(*name))?;
        list = row.get_mut("rows")?.get_mut("fields")?.as_array_mut()?;
    }
    list.iter_mut()
        .find(|r| r["fields"]["name"].as_str() == Some(*last))?
        .get_mut("fields")?
        .as_object_mut()
}

/// The lines leaving a field gone with it (and those leaving any field inside it).
pub(crate) fn drop_from(doc: &mut Map<String, Value>, field: &str) -> usize {
    let Some(Value::Array(list)) = doc.get_mut("connections") else {
        return 0;
    };
    let before = list.len();
    list.retain(|c| {
        let from = c.get("from").and_then(Value::as_str).unwrap_or_default();
        from != field && !from.starts_with(&format!("{field}."))
    });
    let gone = before - list.len();
    if list.is_empty() {
        doc.shift_remove("connections");
    }
    gone
}

// MARK: - Held to the kinds of line

/// Where a path inside a card leads: its head (a flow's node too: a card of its own), or a
/// row with its kind of row (a record's field, a formatdoc's item, a process node's step, a
/// row or a field of a project's own kind); none when it does not.
fn place_of(project: &Project, card: &Card, path: &str, through_refs: bool) -> Option<LinePlace> {
    // A process node's row, by its place: `steps[0]`.
    if let Some(i) = crate::detail::step_index(path.trim()).filter(|_| card.kind != INSTANCE) {
        return (i < super::walk::step_count(card)).then(|| LinePlace::row(Some("steps")));
    }
    let names: Vec<String> = super::picks::split(path)
        .iter()
        .map(|n| crate::pathtext::without_all(n).to_string())
        .filter(|n| !n.is_empty())
        .collect();
    if names.is_empty() {
        return Some(LinePlace::HEAD);
    }
    let walked = walk(card, &names, project, through_refs).ok()?;
    Some(match walked.place {
        Place::Card => LinePlace::HEAD,
        _ => LinePlace::row(walked.row.as_deref()),
    })
}

/// Where a line lands: in the card it reaches by the names there, or a path in full
/// elsewhere (a flow's node there is a card's head, as it is when named in its flow).
fn landing<'a>(project: &Project<'a>, card: &Card, to: &'a Card, at: &str) -> Option<LinePlace> {
    // Inside a working directory's file: a function or a type, read from the file (`code`).
    if to.kind == "file" && !at.is_empty() {
        return Some(LinePlace {
            at: crate::kinds::At::Symbol,
            row: None,
        });
    }
    if !crate::pathtext::has_section(at) {
        return place_of(project, to, at, true);
    }
    let hit = super::check::resolve_path(project, at, Some(card), None);
    if !hit.ok {
        return None;
    }
    Some(match hit.kind.as_deref() {
        Some("field") | Some("row") => LinePlace::row(hit.row.as_deref()),
        _ => LinePlace::HEAD,
    })
}

/// Where a landing written in full leads when it is not inside the card the line reaches
/// (`to`; none for a pointer told apart by its discriminator): the card there (a card ghost
/// followed) and the names inside it, or none when it names no card. None when it is
/// inside, or written as names there.
fn elsewhere<'a>(
    project: &Project<'a>,
    card: &Card,
    to: Option<&Card>,
    at: &str,
) -> Option<Option<(&'a Card, String)>> {
    if !crate::pathtext::has_section(at) {
        return None;
    }
    let found = project
        .directory
        .split_entry(at, Some(&project.section_of(card)))
        .and_then(|(entry, rest)| Some((project.nth(project.directory.follow(entry)?), rest)));
    match found {
        Some((there, _)) if to.is_some_and(|t| t.id == there.id) => None,
        other => Some(other),
    }
}

/// A designating line's landing outside the card it reaches, refused:
/// a value is told by the card it names, and where it is found elsewhere is a line of its
/// own — said with how to write that one.
fn landing_elsewhere(
    project: &Project,
    said: &crate::text::Keyed,
    to: Option<&Card>,
    from: &str,
    at: &str,
    there: Option<(&Card, String)>,
) -> crate::text::Keyed {
    let reached = match to {
        Some(t) => keyed!("lines.misfit.insidePartner", card = project.full(t)),
        None => keyed!("lines.misfit.insideThePartner"),
    };
    let out = keyed!("lines.misfit.landingElsewhere", said, reached, at);
    let Some((there, names)) = there else {
        return out;
    };
    let mut line = Map::new();
    line.insert("line".into(), "link".into());
    if !from.is_empty() {
        line.insert("from".into(), from.into());
    }
    line.insert("to".into(), project.full(there).into());
    let names = crate::pathtext::without_alls(&names);
    if !names.is_empty() {
        line.insert("at".into(), names.into());
    }
    keyed!(
        "lines.misfit.withLine",
        text = out,
        line = Value::Object(line)
    )
}

/// Every way a card's lines do not fit the project's kinds of line: each said with the line
/// it is about. A line whose card is gone, or whose landing leads nowhere, is a broken
/// path (`check::line_paths`), not this — but a designating line's landing written outside
/// the card it reaches is this, wherever it leads.
pub(crate) fn misfits(project: &Project, card: &Card) -> Vec<crate::text::Keyed> {
    let mut out = Vec::new();
    let mut counts: std::collections::HashMap<(String, String), usize> = Default::default();
    // A place names each flow it deals with once: a second line to it would say nothing more.
    let mut peers: std::collections::HashSet<(String, String)> = Default::default();
    for (i, line) in card.connections.iter().enumerate() {
        let Some(def) = project.defs.line(&line.line) else {
            out.push(keyed!("lines.misfit.noKind", i, line = line.line));
            continue;
        };
        let said = if line.from.is_empty() {
            keyed!("lines.said.whole", label = def.label, name = def.name)
        } else {
            keyed!(
                "lines.said.from",
                from = line.from,
                label = def.label,
                name = def.name
            )
        };
        *counts
            .entry((def.name.clone(), line.from.clone()))
            .or_default() += 1;
        if def.name == "partner"
            && let Some(to) = line.to.as_deref()
            && !peers.insert((line.from.clone(), to.to_string()))
        {
            let here = if line.from.is_empty() {
                keyed!("lines.place.card", card = project.full(card))
            } else {
                keyed!(
                    "lines.place.in",
                    card = project.full(card),
                    from = line.from
                )
            };
            let there = project
                .card(to)
                .map(|c| project.full(c))
                .unwrap_or_else(|| to.to_string());
            out.push(keyed!(
                "lines.misfit.dupPeer",
                here,
                there,
                label = def.label
            ));
        }
        // Where it leaves.
        match place_of(project, card, &line.from, false) {
            None => out.push(keyed!(
                "lines.misfit.noOrigin",
                said,
                from = line.from,
                card = project.full(card)
            )),
            Some(place) => {
                if !def.leaves(&card.kind, &place) {
                    out.push(keyed!(
                        "lines.misfit.leaves",
                        said,
                        allowed = def.from_said(),
                        kind = card.kind,
                        place = place.said(def.from_names_rows())
                    ));
                }
                if def.base == Base::Embed
                    && place.row.as_deref() == Some(FIELD_ROWS)
                    && super::walk::walk(card, &super::picks::split(&line.from), project, false)
                        .ok()
                        .and_then(|w| match w.place {
                            Place::Field(f) => Some(!f.children.is_empty()),
                            _ => None,
                        })
                        .unwrap_or(false)
                {
                    out.push(keyed!("lines.misfit.embedWithFields", said));
                }
            }
        }
        // What it reaches, and where.
        let reached = line.to.as_deref().and_then(|id| project.card(id));
        // main's or a branch's card's line to a working directory's file: of a kind that reaches files, onto the file itself — there or not yet.
        if let Some(id) = line.to.as_deref().filter(|id| id.starts_with("file:"))
            && (reached.is_some_and(|c| c.kind == "file")
                || (reached.is_none() && project.files.is_some()))
        {
            let written = project
                .file_written(id)
                .or_else(|| reached.map(|c| project.full(c)))
                .unwrap_or_else(|| id.to_string());
            if !def.to.names_kind("file") {
                out.push(keyed!("lines.misfit.fileKind", said, written));
            } else if line.at.as_deref().is_some_and(|a| !a.is_empty())
                && !def.to.at.contains(&crate::kinds::At::Symbol)
            {
                out.push(keyed!("lines.misfit.fileAt", said, written));
            }
            continue;
        }
        // A board's card's line to a working directory's file: a link alone, onto the file itself.
        if reached.is_none()
            && let Some(outside) = project.outside.as_deref()
            && outside.files.is_some()
            && let Some(id) = line.to.as_deref().filter(|id| id.starts_with("file:"))
        {
            // `at` names a function or a type in the file, as main's lines to one do.
            let written = outside.file_written(id).unwrap_or_else(|| id.to_string());
            if def.base != Base::Link || def.to.names_kind("file") {
                out.push(keyed!("lines.misfit.boardToFile", said, written));
            }
            continue;
        }
        // A board's card's line to main's card: a
        // link alone, to a kind it may reach, landing where main's card has it.
        if reached.is_none()
            && let Some(outside) = project.outside.as_deref()
            && let Some(to) = line.to.as_deref().and_then(|id| outside.card(id))
        {
            if def.base != Base::Link {
                out.push(keyed!(
                    "lines.misfit.boardToMain",
                    said,
                    card = outside.full(to)
                ));
                continue;
            }
            if !def.to.takes_kind(&to.kind) {
                out.push(keyed!(
                    "lines.misfit.partner",
                    said,
                    to = def.to.said(),
                    card = outside.full(to),
                    kind = to.kind
                ));
                continue;
            }
            let at = line.at.as_deref().unwrap_or_default();
            if !at.is_empty() {
                match landing(outside, to, to, at) {
                    None => out.push(keyed!(
                        "lines.misfit.noLanding",
                        said,
                        at,
                        card = outside.full(to)
                    )),
                    Some(landed) if !def.to.takes_place(&landed) => out.push(keyed!(
                        "lines.misfit.landsOn",
                        said,
                        to = def.to.said(),
                        at,
                        place = landed.said(true)
                    )),
                    Some(_) => {}
                }
            }
            continue;
        }
        // A designating line lands inside the card it reaches.
        if def.base == Base::Designate
            && (reached.is_some() || line.to.is_none())
            && let Some(at) = line.at.as_deref()
            && let Some(there) = elsewhere(project, card, reached, at)
        {
            out.push(landing_elsewhere(
                project, &said, reached, &line.from, at, there,
            ));
            continue;
        }
        let Some(to) = reached else {
            continue;
        };
        if !def.to.takes_kind(&to.kind) {
            out.push(keyed!(
                "lines.misfit.partner",
                said,
                to = def.to.said(),
                card = project.full(to),
                kind = to.kind
            ));
            continue;
        }
        // A way to a card beside it, in the same container (a flow's node to another).
        if def.siblings && (card.inside.is_none() || to.inside != card.inside) {
            out.push(match project.container_of(card) {
                Some(container) => keyed!(
                    "lines.misfit.sameContainer",
                    said,
                    container = project.full(container),
                    card = project.full(to)
                ),
                None => keyed!(
                    "lines.misfit.sameContainerNone",
                    said,
                    card = project.full(to)
                ),
            });
            continue;
        }
        if def.base == Base::Embed && line.at.is_some() {
            out.push(keyed!("lines.misfit.embedAt", said));
            continue;
        }
        let at = line.at.as_deref().unwrap_or_default();
        if let Some(landed) = landing(project, card, to, at)
            && !def.to.takes_place(&landed)
        {
            out.push(keyed!(
                "lines.misfit.landsOn",
                said,
                to = def.to.said(),
                at = if at.is_empty() {
                    project.full(to)
                } else {
                    at.to_string()
                },
                place = landed.said(def.to.names_rows())
            ));
        }
    }
    for ((name, from), n) in counts {
        if let Some(max) = project.defs.line(&name).and_then(|d| d.max)
            && n > max
        {
            let def = project.defs.line(&name).unwrap();
            out.push(if from.is_empty() {
                keyed!(
                    "lines.misfit.tooManyAtHead",
                    label = def.label,
                    name,
                    max,
                    n
                )
            } else {
                keyed!(
                    "lines.misfit.tooMany",
                    label = def.label,
                    name,
                    max,
                    from,
                    n
                )
            });
        }
    }
    out
}

// MARK: - What a place may draw

/// One kind of line a place may draw, and how many of its kind the place has now.
pub(crate) struct Drawable<'d> {
    pub def: &'d LineDef,
    pub count: usize,
}

/// The kinds of line the place `from` in `card` may draw — to `to` (the card it reaches,
/// a card ghost followed, and where it lands there) when one is named — held to the
/// project's kinds of line as a write is (`misfits`): where it leaves (`from`), what it
/// reaches and where (`to`; a designating line inside the card it reaches), a node's way to
/// a node beside it (`siblings`), an embedding's field. Split into those it may draw now and
/// those it has as many of as its kind lets it (`max`). The one rule the window's つなぐ and
/// MCP read (`card_read`'s `as: lines`). A kind read from a card's text
/// (`mentions`) is never drawn.
pub(crate) fn drawable<'d>(
    project: &'d Project,
    card: &Card,
    from: &str,
    to: Option<(&Card, &str)>,
) -> Result<(Vec<Drawable<'d>>, Vec<Drawable<'d>>), String> {
    let Some(leaves) = place_of(project, card, from, false) else {
        return Err(text!("lines.noOrigin", from, card = project.full(card)));
    };
    // An embedding leaves a field with nothing inside it.
    let holds_inside = leaves.row.as_deref() == Some(FIELD_ROWS)
        && walk(card, &super::picks::split(from), project, false)
            .ok()
            .and_then(|w| match w.place {
                Place::Field(f) => Some(!f.children.is_empty() || f.kind == "object"),
                _ => None,
            })
            .unwrap_or(false);
    // main's card, from the board: where it lands is read there, and only a link reaches it. A
    // working directory's file, from main or a branch: read there, and only a kind of line that
    // names files reaches it.
    let in_files = to.is_some_and(|(target, _)| target.kind == "file");
    let elsewhere_file = to.is_some_and(|(target, _)| {
        project.card(&target.id).is_none() && project.file_card(&target.id).is_some()
    });
    // From the board, through main.
    let board_file = in_files && project.outside.is_some();
    let across = !in_files && to.is_some_and(|(target, _)| project.card(&target.id).is_none());
    let outside_files = project.outside.as_deref().and_then(|o| o.files.as_deref());
    let site = match (project.outside.as_deref(), project.files.as_deref()) {
        (_, Some(files)) if elsewhere_file => files,
        (Some(_), _) if board_file => outside_files.unwrap_or(project),
        (Some(outside), _) if across => outside,
        _ => project,
    };
    let landed = match to {
        Some((target, at)) => Some(landing(site, card, target, at).ok_or_else(|| {
            text!(
                "lines.notFound",
                path = project.full(target),
                at = if at.is_empty() {
                    String::new()
                } else {
                    format!(".{at}")
                }
            )
        })?),
        None => None,
    };
    // A process node's row takes only the lines its kind of row is written by: a write, no peer.
    let step_lines = crate::detail::step_index(from.trim())
        .filter(|_| card.kind == "process")
        .and_then(|i| super::walk::step_op(card, i))
        .map(|op| super::check::step_lines(&op));
    let (mut now, mut full) = (Vec::new(), Vec::new());
    for def in &project.defs.lines {
        if def.from_text || !def.leaves(&card.kind, &leaves) {
            continue;
        }
        if let Some(allowed) = step_lines
            && super::check::STEP_LINES.contains(&def.name.as_str())
            && !allowed.contains(&def.name.as_str())
        {
            continue;
        }
        if def.base == Base::Embed && holds_inside {
            continue;
        }
        if let (Some((target, at)), Some(landed)) = (to, &landed) {
            if across && def.base != Base::Link {
                continue;
            }
            if board_file {
                if def.base != Base::Link || def.to.names_kind("file") {
                    continue;
                }
            } else if in_files != def.to.names_kind("file") {
                continue;
            }
            if !def.to.takes(&target.kind, landed) {
                continue;
            }
            if def.siblings && (card.inside.is_none() || target.inside != card.inside) {
                continue;
            }
            if def.base == Base::Embed && !at.is_empty() {
                continue;
            }
            // A designating line lands inside the card it reaches.
            if def.base == Base::Designate && elsewhere(project, card, Some(target), at).is_some() {
                continue;
            }
        }
        let count = card
            .connections
            .iter()
            .filter(|c| c.line == def.name && c.from == from)
            .count();
        let drawable = Drawable { def, count };
        if def.max.is_some_and(|max| count >= max) {
            full.push(drawable);
        } else {
            now.push(drawable);
        }
    }
    Ok((now, full))
}

/// What `card_read`'s `as: lines` answers of the place `target` names (a card, or a field,
/// a formatdoc's item, a node inside it; `from` for a process node's row, `steps[0]`): the
/// kinds of line it may draw — to `to` when that is given, with the card it reaches and
/// where, as `connections` writes them — each by its definition's words, and the ones it
/// has as many of as its kind lets it (`full`).
pub(crate) fn lines_view(
    seen: &super::Seen,
    target: &str,
    given: &Map<String, Value>,
) -> Result<Value, String> {
    use super::walk::{Target, locate};
    let project = seen.project();
    let base = super::summary::base(seen, given.get("section").and_then(Value::as_str))?;
    let written_from = given
        .get("from")
        .and_then(Value::as_str)
        .map(|f| crate::pathtext::without_alls(f.trim()))
        .filter(|f| !f.is_empty());
    let (card, from) = match locate(&project, target, base.as_deref())? {
        Target::Card(card) => (card, written_from.unwrap_or_default()),
        Target::Symbol(..) => return Err(text!("code.symbol.noLines", path = target)),
        Target::Ghost(ghost) => {
            return Err(text!(
                "lines.ghostNoLines",
                ghost = project.full_ghost(ghost),
                real = project
                    .ghost_card(ghost)
                    .map(|c| project.full(c))
                    .unwrap_or_else(|| text!("lines.noTarget"))
            ));
        }
        Target::Field(card, _, names)
        | Target::FormatRow(card, names)
        | Target::CustomRow(card, names) => {
            if let Some(from) = written_from {
                return Err(text!("lines.fromWithTarget", from, target));
            }
            (card, super::walk::said_names(card, &names).join("."))
        }
        Target::EnumValue(card, name) => {
            if let Some(from) = written_from {
                return Err(text!("lines.fromWithTarget", from, target));
            }
            (card, crate::pathtext::quote(&name).into_owned())
        }
        Target::Step(card, i) => {
            if let Some(from) = written_from {
                return Err(text!("lines.fromWithTarget", from, target));
            }
            (card, format!("steps[{i}]"))
        }
    };
    let to = match given.get("to").and_then(Value::as_str).map(str::trim) {
        Some(path) if !path.is_empty() => {
            Some(line_end(&project, path, &project.section_of(card))?)
        }
        _ => None,
    };
    let (now, full) = drawable(
        &project,
        card,
        &from,
        to.as_ref().map(|(_, real, at)| (*real, at.as_str())),
    )?;
    // Its name and words first, then how many it may have and has, and what it reaches.
    let said = |d: &Drawable, out: &mut Map<String, Value>, words: bool| {
        out.insert("line".into(), d.def.name.clone().into());
        out.insert("label".into(), d.def.label.clone().into());
        if words {
            out.insert("description".into(), d.def.description.clone().into());
            out.insert("base".into(), d.def.base.name().into());
        }
        if let Some(max) = d.def.max {
            out.insert("max".into(), max.into());
        }
        if d.count > 0 {
            out.insert("count".into(), d.count.into());
        }
        if to.is_none() {
            out.insert("reaches".into(), d.def.to.said().into());
        }
    };
    let mut out = Map::new();
    out.insert("project".into(), seen.detail.project.name.clone().into());
    let mut path = project.full(card);
    if !from.is_empty() && crate::detail::step_index(&from).is_none() {
        path = format!("{path}.{from}");
    }
    out.insert("path".into(), path.into());
    if !from.is_empty() {
        out.insert("from".into(), from.clone().into());
    }
    if let Some((written, _, at)) = &to {
        out.insert("to".into(), written.clone().into());
        if !at.is_empty() {
            out.insert("at".into(), at.clone().into());
        }
    }
    out.insert(
        "lines".into(),
        Value::Array(
            now.iter()
                .map(|d| {
                    let mut one = Map::new();
                    said(d, &mut one, true);
                    let required: Vec<Value> = d
                        .def
                        .fields
                        .iter()
                        .filter(|f| matches!(f.required, crate::kinds::Cond::Always))
                        .map(|f| f.name.clone().into())
                        .collect();
                    if !required.is_empty() {
                        one.insert("required".into(), Value::Array(required));
                    }
                    Value::Object(one)
                })
                .collect(),
        ),
    );
    if !full.is_empty() {
        out.insert(
            "full".into(),
            Value::Array(
                full.iter()
                    .map(|d| {
                        let mut one = Map::new();
                        said(d, &mut one, false);
                        Value::Object(one)
                    })
                    .collect(),
            ),
        );
    }
    Ok(Value::Object(out))
}

/// Where a path written for a line's other end leads, read from `base`: the card or card
/// ghost it names as `connections` writes it (`to`, in full; a card inside a container on
/// into it), the card it reaches (a ghost followed), and the names there (`at`).
fn line_end<'a>(
    project: &Project<'a>,
    path: &str,
    base: &str,
) -> Result<(String, &'a Card, String), String> {
    let here = line_end_in(project, path, base);
    // main's card, from the board:
    // found there by its path in full.
    if here.is_err()
        && crate::pathtext::has_section(path)
        && let Some(outside) = project.outside.as_deref()
    {
        if let Ok(there) = line_end_in(outside, path, outside.home()) {
            return Ok(there);
        }
        // A working directory's file, from the board, through main.
        if let Some(id) = outside.file_id(path)
            && let Some(card) = outside.file_card(&id)
        {
            return Ok((outside.file_written(&id).unwrap_or(id), card, String::new()));
        }
        // A function or a type in one, the same way: the file, and where in it.
        if let Some((id, at)) = outside.inside_file(path)
            && let Some(card) = outside.file_card(&id)
        {
            return Ok((outside.file_written(&id).unwrap_or(id), card, at));
        }
    }
    // A working directory's file, from main or a branch, by its path in the tree, when the tree
    // was read without what the working directories hold.
    if here.is_err()
        && let Some(id) = project.file_id(path)
        && let Some(card) = project.file_card(&id)
    {
        return Ok((project.file_written(&id).unwrap_or(id), card, String::new()));
    }
    // A function or a type in one: the file, and where in it.
    if here.is_err()
        && let Some((id, at)) = project.inside_file(path)
        && let Some(card) = project.file_card(&id)
    {
        return Ok((project.file_written(&id).unwrap_or(id), card, at));
    }
    // A path written as files were before: as it is written now.
    if let Err(e) = &here
        && let Some(now) = project.file_form_now(path)
    {
        return Err(text!("lines.fileFormBefore", path, now, e));
    }
    here
}

/// `line_end` in one tree.
fn line_end_in<'a>(
    project: &Project<'a>,
    path: &str,
    base: &str,
) -> Result<(String, &'a Card, String), String> {
    use crate::export::Named;
    use crate::export::paths::WrittenPath;
    // Said as a target is when it leads nowhere.
    super::walk::locate(project, path, Some(base))?;
    let written = WrittenPath::new(path).ok_or_else(|| text!("lines.toEmpty"))?;
    let mut names = written.names.clone();
    Ok(
        match super::walk::find_named(project, &written.card, Some(base))? {
            Named::Card(mut card) => {
                while project.is_container(&card.kind)
                    && let Some(inner) =
                        names.first().and_then(|n| project.inner_named(&card.id, n))
                {
                    card = inner;
                    names.remove(0);
                }
                (project.full(card), card, names.join("."))
            }
            Named::Ghost(ghost) => {
                let card = project.ghost_card(ghost).ok_or_else(|| {
                    text!("lines.ghostNoTarget", ghost = project.full_ghost(ghost))
                })?;
                (project.full_ghost(ghost), card, names.join("."))
            }
        },
    )
}

/// A card's lines as they are written now, held to the project's kinds of line: refused with
/// every way they do not fit.
pub(crate) fn require_fit(tx: &Tx, scope: &Scope, id: &str) -> Result<(), String> {
    let seen = Seen::in_tx(tx, scope)?;
    let project = seen.project();
    let Some(card) = project.card(id).filter(|c| c.id == id) else {
        return Ok(());
    };
    let wrong = misfits(&project, card);
    if wrong.is_empty() {
        Ok(())
    } else {
        Err(wrong
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n"))
    }
}

/// A card's lines' document checked by the project's kinds of line: what each holds.
pub(crate) fn require_shape(project: &Project, doc: &Map<String, Value>) -> Result<(), String> {
    let problems = crate::kinds::check_connections(&project.defs, &Value::Object(doc.clone()));
    if problems.is_empty() {
        Ok(())
    } else {
        Err(problems
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n"))
    }
}
