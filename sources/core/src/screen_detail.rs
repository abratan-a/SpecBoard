//! A project read whole as the screen reads it (`project_detail`, `specboard://SpecBoard/Docs/Design:CoreApi`「読む」): what
//! `detail::read` composes, without the keys the core's own typed reading takes the lines and
//! the containers in (a field's `refSchemaId` and `pointsAt`, a card's `payloads`, `holds`,
//! `notes` and `nodes`, a task's `form.fields.links`, a goal's `form.fields.items`), and with
//! every line of every card in `relations`. The screen draws from these, the cards' `connections` and the
//! kinds of line (`kinds`), and makes no line of its own.

use crate::export::Project;
use crate::export::paths::{WrittenPath, landing};
use crate::model::{Card, Connection, Field};
use crate::store::Reader;
use crate::tools::facts::{self, LinkKind, TOP_ROW};
use serde_json::{Map, Value};

/// The project `project_id` as `branch` sees it, as the screen reads it.
pub fn read(store: &Reader, project_id: &str, branch: Option<&str>) -> Result<Value, String> {
    let whole = crate::detail::read(store, project_id, branch)?;
    let mut out = dress(store, project_id, branch, whole, true)?;
    out["filePlaces"] = file_places(store, project_id, branch)?;
    Ok(out)
}

/// A file's place as the tree keeps it: its id, where, pinned, its size.
type Placed = (
    String,
    Option<i64>,
    Option<i64>,
    bool,
    Option<i64>,
    Option<i64>,
);

/// Where the working directories' files sit on their sections' maps in the tree, by the file's id
/// (`{x, y, pinned}`, `size` when one is written): the files are not in the tree's reading
/// (`workdir_detail` says them), where they were moved is — the branch's over main's.
fn file_places(store: &Reader, project_id: &str, branch: Option<&str>) -> Result<Value, String> {
    let mut out = Map::new();
    let trees: Vec<&str> = match branch {
        Some(crate::store::BOARD) => return Ok(Value::Object(out)),
        Some(b) => vec!["", b],
        None => vec![""],
    };
    let mut stmt = store
        .conn
        .prepare(
            "SELECT item, x, y, pinned, w, h FROM places WHERE project_id = ? AND branch_id = ? \
             AND view = '' AND item_kind = 'card' AND item LIKE 'file:%' ORDER BY item",
        )
        .map_err(crate::store::sql)?;
    for tree in trees {
        let rows: Vec<Placed> = stmt
            .query_map(rusqlite::params![project_id, tree], |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get::<_, i64>(3)? != 0,
                    r.get(4)?,
                    r.get(5)?,
                ))
            })
            .map_err(crate::store::sql)?
            .collect::<Result<_, _>>()
            .map_err(crate::store::sql)?;
        for (item, x, y, pinned, w, h) in rows {
            let (Some(x), Some(y)) = (x, y) else {
                continue;
            };
            let mut place = Map::new();
            place.insert("x".into(), x.into());
            place.insert("y".into(), y.into());
            place.insert("pinned".into(), pinned.into());
            if let Some(w) = w {
                place.insert("size".into(), crate::tools::grid::size_value((w, h)));
            }
            out.insert(item, Value::Object(place));
        }
    }
    Ok(Value::Object(out))
}

/// The cards, card ghosts, cards inside a container and sections a branch removed (`cards`,
/// `sections`, by id), as the screen reads them (`read`'s `schemas`, `ghosts`, `inner` and
/// `sections`): each as main held it when the branch removed it, where it was — read from the
/// branch with what it removed put back. Their lines are left out: nothing is drawn to or from
/// what is gone.
pub fn gone(
    store: &Reader,
    project_id: &str,
    branch: &str,
    cards: &std::collections::BTreeSet<String>,
    sections: &std::collections::BTreeSet<String>,
) -> Result<Value, String> {
    let mut back: Vec<crate::store::OverRow> = Vec::new();
    for (table, head) in [("card_docs", "section_id"), ("section_docs", "parent_id")] {
        let mut stmt = store
            .conn
            .prepare(&format!(
                "SELECT id, base FROM {table} WHERE project_id = ? AND branch_id = ? AND removed = 1 AND base IS NOT NULL ORDER BY rowid"
            ))
            .map_err(crate::store::sql)?;
        let rows = stmt
            .query_map([project_id, branch], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })
            .map_err(crate::store::sql)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(crate::store::sql)?;
        for (id, base) in rows {
            let Some(doc) = crate::store::parse(Some(&base)) else {
                continue;
            };
            let at = if table == "card_docs" {
                crate::write::holder(&doc).to_string()
            } else {
                doc.get("parent_id")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string()
            };
            let mut row = Map::new();
            row.insert(head.into(), at.into());
            row.insert("name".into(), doc.get("name").cloned().unwrap_or_default());
            row.insert("removed".into(), 0.into());
            row.insert("doc".into(), base.into());
            back.push((table.to_string(), id, Some(row)));
        }
    }
    if back.is_empty() {
        return Ok(none_gone());
    }
    let project = store
        .project(project_id)?
        .ok_or_else(|| text!("detail.error.projectMissing", project_id))?;
    let over = crate::store::Over {
        tree: branch,
        rows: &back,
    };
    let made = store.compose_over(project_id, Some(branch), Some(&over))?;
    let whole = crate::detail::of_made(&project, &made);
    let whole = dress(store, project_id, Some(branch), whole, false)?;
    let mut out = none_gone();
    for key in GONE {
        let wanted = if key == "sections" { sections } else { cards };
        out[key] = Value::Array(
            whole[key]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|e| e["id"].as_str().is_some_and(|id| wanted.contains(id)))
                .cloned()
                .collect(),
        );
    }
    Ok(out)
}

/// The parts of a tree read whole that say what a branch removed (`gone`).
const GONE: [&str; 4] = ["schemas", "ghosts", "inner", "sections"];

/// Nothing removed, as `gone` says it.
pub fn none_gone() -> Value {
    Value::Object(
        GONE.iter()
            .map(|key| (key.to_string(), Value::Array(Vec::new())))
            .collect(),
    )
}

/// A tree read whole (`detail`'s) as the screen reads it; with `lines`, every line of it in
/// `relations`, and none without.
fn dress(
    store: &Reader,
    project_id: &str,
    branch: Option<&str>,
    mut whole: Value,
    lines: bool,
) -> Result<Value, String> {
    let typed = crate::model::typed(&whole)?;
    // The board's lines and text reach main's cards: read with main beside it, their lines in
    // `relations` with `outside`. The screen reads main's detail itself for the cards they reach.
    let main = match branch {
        Some(crate::store::BOARD) if lines => Some(crate::model::typed(&crate::detail::read(
            store, project_id, None,
        )?)?),
        _ => None,
    };
    // main's and a branch's lines reach the working directories' files: read
    // beside them, their lines in `relations` with `outside` too.
    let files = if lines {
        crate::tools::files_of(store, project_id, &whole, false)?
    } else {
        None
    };
    let files_of = || {
        files.as_ref().map(|o| {
            let (d, i) = o.typed();
            Project::indexed(d, i.clone())
        })
    };
    let code = || files.as_ref().map(|o| o.code.clone());
    // The board reaches the working directories' files as main does: beside main.
    let board = branch == Some(crate::store::BOARD);
    let project = Project::new(&typed)
        .with_outside(
            main.as_ref()
                .map(|m| Project::new(m).with_files(files_of()).with_code(code())),
        )
        .with_files(if board { None } else { files_of() })
        .with_code(if board { None } else { code() });
    let relations = if lines {
        relations(&project)
    } else {
        Vec::new()
    };
    let strays = strays(&project);
    // Each table's Markdown, as its rules build it now: the card drawn and the editor's preview.
    let tables: std::collections::HashMap<String, String> = project
        .detail
        .schemas
        .iter()
        .filter(|c| c.kind == crate::kinds::TABLE)
        .map(|c| {
            (
                c.id.clone(),
                crate::tools::tables::markdown(&crate::tools::tables::build(&project, c)),
            )
        })
        .collect();
    // The parts to look at again, by the cards their lines reach that changed after them: the
    // tree's history read once for every part.
    let review = if lines {
        crate::tools::review::marks(
            store.conn,
            project_id,
            branch.unwrap_or_default(),
            &project,
            None,
        )?
    } else {
        crate::tools::review::Marks::new()
    };
    // The implementations' lines whose code changed since it was recorded, or was never recorded,
    // on this machine, by the card they leave: the card's mark and the rows it is drawn at.
    let code = if lines && !board {
        crate::tools::code_review::by_card(crate::tools::code_review::lines(
            store.conn, project_id, &project, None,
        )?)
    } else {
        Default::default()
    };
    let faces = face_rows(
        &project,
        &crate::tools::grid::Layout::read(store.conn, project_id)?,
    );
    strip(&mut whole);
    whole["relations"] = Value::Array(relations);
    // How tall a card its kind's face draws with its Markdown under its fields is, with no size
    // of its own: as tall as the core places it (`grid::face_rows`), the page's height estimated
    // from the text.
    for card in whole["schemas"].as_array_mut().into_iter().flatten() {
        if let Some(rows) = card["id"].as_str().and_then(|id| faces.get(id))
            && let Some(c) = card.as_object_mut()
        {
            c.insert("faceRows".into(), (*rows).into());
        }
    }
    for card in whole["schemas"].as_array_mut().into_iter().flatten() {
        if let Some(marks) = card["id"].as_str().and_then(|id| review.get(id))
            && let Some(c) = card.as_object_mut()
        {
            c.insert("review".into(), crate::tools::review::listed(marks));
        }
    }
    for key in ["schemas", "inner"] {
        for card in whole[key].as_array_mut().into_iter().flatten() {
            if let Some(found) = card["id"].as_str().and_then(|id| code.get(id))
                && found
                    .iter()
                    .any(crate::tools::code_review::CodeLine::flagged)
                && let Some(c) = card.as_object_mut()
            {
                c.insert(
                    "codeReview".into(),
                    crate::tools::code_review::listed(found, true),
                );
            }
        }
    }
    for card in whole["schemas"].as_array_mut().into_iter().flatten() {
        if let Some(markdown) = card["id"].as_str().and_then(|id| tables.get(id))
            && let Some(c) = card.as_object_mut()
        {
            c.insert("tableMarkdown".into(), markdown.clone().into());
        }
    }
    // What a card of a kind the project defines holds that its definition no longer has, by the
    // card: drawn faint, and let go of by hand.
    for card in whole["schemas"].as_array_mut().into_iter().flatten() {
        if let Some(found) = card["id"].as_str().and_then(|id| strays.get(id))
            && let Some(c) = card.as_object_mut()
        {
            c.insert("strayFields".into(), found.clone());
        }
    }
    Ok(whole)
}

/// The grid rows of each card drawn by its kind's face with Markdown to draw, a page or a frame, and no
/// height of its own, by its id: the Markdown estimated at the card's own width when it has one.
fn face_rows(
    project: &Project,
    layout: &crate::tools::grid::Layout,
) -> std::collections::HashMap<String, i64> {
    use crate::tools::grid::{own_size, width_points};
    let kinds = project.defs.cards();
    project
        .detail
        .schemas
        .iter()
        .chain(project.detail.inner.iter())
        .filter_map(|card| {
            let own = own_size(card, kinds);
            if own.is_some_and(|s| s.1.is_some()) {
                return None;
            }
            let width = width_points(layout, own.map(|s| s.0));
            let face = kinds.get(&card.kind)?.face.as_ref()?;
            if face.page.is_some() || card.kind == crate::kinds::FRAME {
                return Some((
                    card.id.clone(),
                    crate::tools::grid::face_rows(card, face, layout, width),
                ));
            }
            face.markdown.as_ref()?;
            let written = card
                .form
                .as_ref()
                .and_then(|f| f["fields"].get(face.markdown.as_deref()?))
                .and_then(Value::as_str)
                .is_some_and(|t| !t.trim().is_empty());
            written.then(|| {
                (
                    card.id.clone(),
                    crate::tools::grid::face_rows(card, face, layout, width),
                )
            })
        })
        .collect()
}

/// The strays of each card of a kind the project defines, or of the board's goal and task, that
/// has any: by its id, where each is → its value (`kinds::strays`); and a data card's values whose
/// fields its shape has lost (`instances::strays`).
fn strays(project: &Project) -> std::collections::HashMap<String, Value> {
    let kinds = project.defs.cards();
    project
        .detail
        .schemas
        .iter()
        .filter_map(|card| {
            let found = if card.kind == crate::kinds::INSTANCE {
                crate::tools::instances::strays(project, card)
            } else {
                let def = kinds
                    .get(&card.kind)
                    .filter(|_| kinds.keeps_strays(&card.kind))?;
                crate::kinds::strays(def, &card.kept_form())
            };
            (!found.is_empty()).then(|| {
                (
                    card.id.clone(),
                    Value::Object(found.into_iter().collect::<Map<String, Value>>()),
                )
            })
        })
        .collect()
}

/// The keys the core's typed reading takes the lines and the containers in, left out: what
/// the screen reads of them is the cards' `connections`, the cards inside a container
/// (`in`, `inner`) and `relations`.
fn strip(whole: &mut Value) {
    fn fields(list: &mut Value) {
        for field in list.as_array_mut().into_iter().flatten() {
            if let Some(f) = field.as_object_mut() {
                f.shift_remove("refSchemaId");
                f.shift_remove("pointsAt");
                if let Some(children) = f.get_mut("children") {
                    fields(children);
                }
            }
        }
    }
    for card in whole["schemas"].as_array_mut().into_iter().flatten() {
        let Some(c) = card.as_object_mut() else {
            continue;
        };
        for key in ["payloads", "notes", "holds", "nodes"] {
            c.shift_remove(key);
        }
        if let Some(list) = c.get_mut("fields") {
            fields(list);
        }
        if let Some(form) = c.get_mut("form").and_then(|f| f.get_mut("fields"))
            && let Some(f) = form.as_object_mut()
        {
            f.shift_remove("links");
            f.shift_remove("items");
        }
    }
}

/// Every line of every card — the cards and those inside a container (a flow's nodes) —
/// and what a note's or a document's text names (`mentions`), each from the card that holds
/// it to the card it reaches (through a card ghost, the card the ghost leads to, saying the
/// ghost: `viaGhostId`). A card's lines come in the order of its fields, then its other
/// lines, then what its text names.
fn relations(project: &Project) -> Vec<Value> {
    let mut out = Vec::new();
    for card in project
        .detail
        .schemas
        .iter()
        .chain(project.detail.inner.iter())
    {
        lines_of(project, card, &mut out);
        mentions_of(project, card, &mut out);
    }
    out
}

/// Every card's lines as `relations` has them — the cards inside a container's too — without
/// what a writing's text mentions: what `card_read`'s `refersTo` and `referencedBy` are read
/// from.
pub(crate) fn connection_lines(project: &Project) -> Vec<Value> {
    let mut out = Vec::new();
    for card in project
        .detail
        .schemas
        .iter()
        .chain(project.detail.inner.iter())
    {
        lines_of(project, card, &mut out);
    }
    out
}

/// A card's fields depth first, each with its names from the card down (`a.b`).
fn walk_fields<'a>(fields: &'a [Field], above: &str, visit: &mut dyn FnMut(&'a Field, String)) {
    for field in fields {
        let path = if above.is_empty() {
            crate::pathtext::quote(&field.name).into_owned()
        } else {
            crate::pathtext::member(above, &field.name)
        };
        visit(field, path.clone());
        walk_fields(&field.children, &path, visit);
    }
}

fn lines_of(project: &Project, card: &Card, out: &mut Vec<Value>) {
    let lines: Vec<&Connection> = card.connections.iter().collect();
    // The fields in their order, then what leaves elsewhere.
    let mut ordered: Vec<(&Connection, Option<&Field>)> = Vec::new();
    let mut placed = vec![false; lines.len()];
    walk_fields(&card.fields, "", &mut |field, path| {
        for (i, line) in lines.iter().enumerate() {
            if !placed[i] && line.from == path {
                placed[i] = true;
                ordered.push((line, Some(field)));
            }
        }
    });
    for (i, line) in lines.iter().enumerate() {
        if !placed[i] {
            ordered.push((line, None));
        }
    }
    let base = project.section_of(card);
    for (line, field) in ordered {
        // What it reaches: the card it names (a card ghost followed), or — a pointer told
        // apart by its discriminator — each card of its card's section that pins a value on
        // the field the discriminator names.
        let targets: Vec<(&Card, Option<String>)> = match &line.to {
            Some(to) => project
                .card(to)
                .map(|c| (c, project.ghost(to).map(|g| g.id.clone())))
                .or_else(|| project.outside_card(to).map(|c| (c, None)))
                .or_else(|| project.file_card(to).map(|c| (c, None)))
                .into_iter()
                .collect(),
            None => {
                let tag = line
                    .fields
                    .get("discriminatorField")
                    .or_else(|| line.fields.get("discriminatedBy"))
                    .and_then(Value::as_str);
                match tag {
                    Some(tag) => project
                        .directory
                        .members(&base)
                        .iter()
                        .map(|&i| project.nth(i))
                        .filter(|c| {
                            c.fields
                                .iter()
                                .any(|f| f.name == tag && f.fixed().is_some())
                        })
                        .map(|c| (c, None))
                        .collect(),
                    None => Vec::new(),
                }
            }
        };
        for (target, via) in targets {
            let mut r = Map::new();
            r.insert("fromSchemaId".into(), card.id.clone().into());
            r.insert("toSchemaId".into(), target.id.clone().into());
            r.insert("line".into(), line.line.clone().into());
            // An element of a data card's array picked by its values: the row it is drawn as.
            r.insert(
                "from".into(),
                crate::tools::instances::drawn_at(card, &line.from).into(),
            );
            if let Some(at) = line.at.as_ref().filter(|a| !a.is_empty()) {
                let at = if crate::pathtext::has_section(at) {
                    at.clone()
                } else {
                    crate::tools::instances::drawn_at(target, at)
                };
                r.insert("at".into(), at.into());
            }
            if let Some(via) = via {
                r.insert("viaGhostId".into(), via.into());
            }
            if let Some(field) = field {
                r.insert("fieldId".into(), field.id.clone().into());
                r.insert("isArray".into(), field.is_array.into());
            }
            if project.card(&target.id).is_none() {
                r.insert("outside".into(), true.into());
            }
            if let Some(c) = line.fields.get("cardinality").filter(|c| !c.is_null()) {
                r.insert("cardinality".into(), c.clone());
            }
            r.insert("connection".into(), line.id.clone().into());
            out.push(Value::Object(r));
        }
    }
}

/// What a note's, a freedoc's, a formatdoc's or a stackdoc's text names (`mentions`, read from
/// the text and not written in `connections`): from the card — a formatdoc's from the item whose
/// rows name it (none for the rows at the top), a stackdoc's from the version — to where the path lands, the path as it is
/// written in the text beside it (`written`), so the screen can put the line on the text's
/// own row. Not to the card itself.
fn mentions_of(project: &Project, card: &Card, out: &mut Vec<Value>) {
    if !card.is_writing() {
        return;
    }
    let base = project.section_of(card);
    for link in facts::links(card) {
        if link.kind != LinkKind::Mentions {
            continue;
        }
        let Some((target, row)) = landing(&link.path, &base, project)
            .or_else(|| project.outside_landing(&link.path))
            .or_else(|| project.files_landing(&link.path))
        else {
            continue;
        };
        let outside = project.card(&target.id).is_none();
        if target.id == card.id {
            continue;
        }
        let via = WrittenPath::new(&link.path)
            .and_then(|w| project.find_named(&w.card, &base))
            .and_then(|n| match n {
                crate::export::Named::Ghost(g) => Some(g.id.clone()),
                crate::export::Named::Card(_) => None,
            });
        let from = if (card.kind == "formatdoc" && link.row != TOP_ROW) || card.kind == "stackdoc" {
            link.row.clone()
        } else {
            String::new()
        };
        // Where it lands, in the card's own names: a field's names, a flow's node, a
        // formatdoc's item; none for the card itself.
        let at = match row.as_str() {
            "" => None,
            TOP_ROW => None,
            r => Some(r.strip_prefix("#n.").unwrap_or(r).to_string()),
        };
        let mut r = Map::new();
        r.insert("fromSchemaId".into(), card.id.clone().into());
        r.insert("toSchemaId".into(), target.id.clone().into());
        r.insert("line".into(), "mentions".into());
        r.insert("from".into(), from.into());
        if let Some(at) = at {
            r.insert("at".into(), at.into());
        }
        if let Some(via) = via {
            r.insert("viaGhostId".into(), via.into());
        }
        r.insert("written".into(), link.path.into());
        if outside {
            r.insert("outside".into(), true.into());
        }
        out.push(Value::Object(r));
    }
}
