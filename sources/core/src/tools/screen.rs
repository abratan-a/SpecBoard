//! The window's own requests, for when it reads and writes through the core in place of
//! the Swift app's `Repository` (as `AppModel` calls it): the projects and the branches,
//! what a branch marks on its cards, the project's layout, export directories, writing rules
//! and what MCP may write in it (`mcp_writes`), what a commit changed and what the history
//! takes, where boxes sit on a diagram, and the MCP tools run as the window. The window
//! holds things by id, so these read and write by id; every write is the window's
//! (`Who::screen`).

use super::args::{Args, Refusals};
use super::cards::Scope;
use super::walk::{Target, locate};
use crate::Core;
use crate::mcp::Output;
use crate::ops::{number, text, tree};
use crate::store::{BOARD, Composition, Over, OverRow, Reader, Store, sql};
use crate::write::{Spot, Tx, Who, db};
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

/// The requests of this module.
const KNOWN: [&str; 24] = [
    "projects",
    "embed_diagram",
    "inserted_values",
    "document_parts",
    "branches",
    "branch_marks",
    "layout",
    "workdir_detail",
    "writing_rules",
    "mcp_writes",
    "commit_changes",
    "cut_cost",
    "project_size",
    "section_positions",
    "tool",
    "project_rename",
    "writing_rules_set",
    "mcp_writes_set",
    "places",
    "place_unplaced",
    "prune",
    "release_screen_holds",
    "file_text",
    "file_outline",
];

/// The widest a place reaches from the origin, in cells (`Grid.reach`).
const REACH: f64 = 1_000_000.0;

impl Core {
    /// The requests of this module; none when `op` is not one of them.
    pub(crate) fn screen_op(&self, op: &str, request: &Value) -> Option<Result<Value, String>> {
        KNOWN
            .contains(&op)
            .then(|| self.screen_request(op, request))
    }

    fn screen_request(&self, op: &str, request: &Value) -> Result<Value, String> {
        match op {
            // What only reads, read apart from the writes (`Core::reading`).
            "projects" => projects(&self.reading()?.conn),
            "release_screen_holds" => self.plain_writing(Who::screen_for(request), |tx| {
                tx.release_screen_holds()?;
                Ok(json!({}))
            }),
            "tool" => self.screen_tool(request),
            // A working directory's file shown: read, never written.
            "file_text" => {
                let store = self.reading()?;
                let project = project_of(&store, request, op)?;
                let id = text(request, "id", op, &text!("screen.what.fileId"))?;
                super::code::file_text(&store.conn, project, id, request)
            }
            // The functions and types of the files shown, read now.
            "file_outline" => {
                let store = self.reading()?;
                let project = project_of(&store, request, op)?;
                super::code::file_outline(&store.conn, project, request)
            }
            // What the working directories hold as read, put over every tree by the window; read
            // where its reading is given out, so its digest is every connection's.
            "workdir_detail" => {
                let store = self.reading()?;
                let project = project_of(&store, request, op)?.to_string();
                crate::workdir::overlay(&store.conn, &project)?;
                crate::workdir::give_out(&store, &self.work, false)?;
                workdir_detail(
                    &store.conn,
                    &project,
                    request.get("since").and_then(Value::as_str),
                )
            }
            "branches" | "layout" | "writing_rules" | "mcp_writes" | "project_size" => {
                let store = self.reading()?;
                let project = project_of(&store, request, op)?;
                match op {
                    "branches" => branches(&store.conn, project),
                    "layout" => Ok(layout(&super::grid::Layout::read(&store.conn, project)?)),
                    "writing_rules" => Ok(json!({
                        "rules": super::projects::writing_rules(&store.conn, project)?,
                    })),
                    "mcp_writes" => super::mcp_writes::view(&store.conn, project),
                    _ => {
                        let (content, trees) = super::reads::project_size_of(&store.conn, project)?;
                        Ok(json!({
                            "content": content,
                            "branches": trees.iter().map(|(id, name, history)| json!({
                                "id": id,
                                "name": name,
                                "history": history,
                            })).collect::<Vec<_>>(),
                        }))
                    }
                }
            }
            "embed_diagram" => {
                let store = self.reading()?;
                let (project, tree) = tree(&store, request, op)?;
                let branch = (!tree.is_empty()).then_some(tree);
                let detail =
                    crate::model::typed(&crate::detail::read(&store.reader(), project, branch)?)?;
                embed_diagram(&detail, request)
            }
            "inserted_values" => {
                let store = self.reading()?;
                let (project, tree) = tree(&store, request, op)?;
                let branch = (!tree.is_empty()).then_some(tree);
                let detail =
                    crate::model::typed(&crate::detail::read(&store.reader(), project, branch)?)?;
                inserted_values(&detail, request)
            }
            "document_parts" => {
                let store = self.reading()?;
                let (project, tree) = tree(&store, request, op)?;
                let branch = (!tree.is_empty()).then_some(tree);
                let detail =
                    crate::model::typed(&crate::detail::read(&store.reader(), project, branch)?)?;
                // The board's paths reach main's cards: read with main beside it, as its editor
                // reads them.
                let main = match tree {
                    BOARD => Some(crate::model::typed(&crate::detail::read(
                        &store.reader(),
                        project,
                        None,
                    )?)?),
                    _ => None,
                };
                // The working directories' files and what is in them, as the tools read a tree's
                // paths (`Seen::project`): a link into one is a link, not struck through.
                let files = crate::workdir::overlay(&store.conn, project)?;
                document_parts(&detail, main.as_ref(), files.as_deref(), request)
            }
            "branch_marks" | "commit_changes" | "cut_cost" | "section_positions" => {
                let store = self.reading()?;
                let (project, tree) = tree(&store, request, op)?;
                match op {
                    "branch_marks" => branch_marks(&store.reader(), project, tree),
                    "commit_changes" => {
                        let seq = number(request, "seq", op, &text!("screen.what.commitSeq"))?;
                        commit_changes(&store, project, tree, seq)
                    }
                    "cut_cost" => {
                        let before =
                            number(request, "before", op, &text!("screen.what.oldestKept"))?;
                        cut_cost(&store.conn, project, tree, before)
                    }
                    _ => section_positions(&store.conn, project, tree),
                }
            }
            "places" => {
                let boxes = request
                    .get("boxes")
                    .and_then(Value::as_array)
                    .ok_or_else(|| text!("screen.places.boxesNeeded"))?;
                self.writing(
                    Who::screen_for(request),
                    request,
                    op,
                    |tx, project, tree| {
                        for one in boxes {
                            put_box(tx, project, tree, one)?;
                        }
                        Ok(json!({}))
                    },
                )
            }
            "place_unplaced" => self.writing(
                Who::screen_for(request),
                request,
                op,
                |tx, project, tree| {
                    let scope = scope_of(tx, project, tree)?;
                    super::update::place_unplaced(tx, &scope)?;
                    Ok(json!({}))
                },
            ),
            _ => {
                let project = project_of(&self.lock(), request, op)?.to_string();
                self.project_write(op, request, &project)
            }
        }
    }

    /// The writes to a project's own settings, and its history trimmed.
    fn project_write(&self, op: &str, request: &Value, project: &str) -> Result<Value, String> {
        match op {
            "project_rename" => {
                let raw = text(request, "name", op, &text!("screen.what.newName"))?;
                self.plain_writing(Who::screen_for(request), |tx| {
                    super::projects::rename(tx, project, raw)?;
                    Ok(json!({}))
                })
            }
            "writing_rules_set" => {
                let place = text(request, "place", op, &text!("screen.what.writingPlace"))?;
                let said = text(request, "text", op, &text!("screen.what.rulesText"))?;
                self.plain_writing(Who::screen_for(request), |tx| {
                    let rules = super::projects::set_writing_rules(
                        tx,
                        project,
                        &[(place.to_string(), said.to_string())],
                    )?;
                    Ok(json!({ "rules": rules }))
                })
            }
            "mcp_writes_set" => self.plain_writing(Who::screen_for(request), |tx| {
                super::mcp_writes::set(tx, project, request)
            }),
            _ => self.plain_writing(Who::screen_for(request), |tx| {
                Ok(json!({ "pruned": tx.prune(project)? }))
            }),
        }
    }

    /// An MCP tool run as the window: `{"json": …}` or `{"blocks": […]}`, as the tool
    /// answers. `moves` (the window's alone) goes with a formatdoc's rows written whole.
    fn screen_tool(&self, request: &Value) -> Result<Value, String> {
        let name = text(request, "name", "tool", &text!("screen.what.toolName"))?;
        let arguments = match request.get("arguments") {
            None => Map::new(),
            Some(Value::Object(o)) => o.clone(),
            Some(_) => return Err(text!("screen.tool.argumentsShape")),
        };
        if name == "card_update"
            && let Some(moves) = arguments.get("moves")
        {
            let moves = moves_of(moves)?;
            return self
                .card_update_moving(&Who::screen_for(request), &arguments, &moves)
                .map(|v| json!({ "json": v }));
        }
        Ok(
            match self.tool(&Who::screen_for(request), name, &arguments)? {
                Output::Json(v) => json!({ "json": v }),
                Output::Blocks(texts) => json!({ "blocks": texts }),
                Output::Image {
                    data,
                    mime_type,
                    note,
                } => json!({ "image": { "data": data, "mimeType": mime_type }, "note": note }),
            },
        )
    }

    /// `card_update` of one card's `rows`, written whole, with where its rows moved to: the
    /// paths written into them follow (`SchemaEdit.rowMoves`; `AppModel.setRows`). A flow's
    /// node is renamed as a card (`card_update` of its `name`), its lines following by id.
    fn card_update_moving(
        &self,
        who: &Who,
        arguments: &Map<String, Value>,
        moves: &[Move],
    ) -> Result<Value, String> {
        let args = Args::new(arguments);
        let target = args.target()?;
        let value = args.value()?;
        if let Some(stray) = value.keys().find(|k| k.as_str() != "rows") {
            return Err(text!("screen.moves.withRows", stray = stray));
        }
        self.plain_writing(who.clone(), |tx| {
            let summary = super::project_summary_in(tx.conn(), args.string("project")?)?;
            let named = super::named_in(tx.conn(), &summary.id, args.optional_string("branch"))?;
            let scope = Scope {
                project: summary.id,
                project_name: summary.name,
                tree: named.tree,
                tree_name: named.name,
            };
            let written = (|| -> Result<Value, String> {
                let id = {
                    let seen = super::Seen::in_tx(tx, &scope)?;
                    match locate(&seen.project(), &target, None)? {
                        Target::Card(card) => card.id.clone(),
                        _ => {
                            return Err(text!("screen.moves.notCard", target = target));
                        }
                    }
                };
                super::check(&value, "card_update", "card")?;
                let said = super::update::edit_card(tx, &scope, &id, &value, moves)?;
                super::update::card_written(tx, &scope, &id, said)
            })()
            .map_err(|e| {
                let mut refusals = Refusals::new(1);
                refusals.note(Some(&target), &e);
                refusals.check().err().unwrap_or(e)
            })?;
            super::update::place_unplaced(tx, &scope)?;
            Ok(super::headed_json(written, &scope.tree_name))
        })
    }
}

/// What `![[path]]` in a card of the section `section` draws, as the window shows it in a
/// document (`exported: false`, the head saying what it was drawn from) or as it is written
/// out (`exported: true`): `{"mermaid": …}`, or why it cannot be drawn (`{"problem": …}`).
/// The window draws a document's diagrams from this (`NoteEmbed`), not from lines of its own.
fn embed_diagram(detail: &crate::model::Detail, request: &Value) -> Result<Value, String> {
    let path = text(
        request,
        "path",
        "embed_diagram",
        &text!("screen.what.diagramPath"),
    )?;
    let option = request.get("option").and_then(Value::as_str);
    let base = request
        .get("section")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let exported = request
        .get("exported")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let project = crate::export::Project::new(detail);
    let base = if base.is_empty() {
        home_of(detail)
    } else {
        base.to_string()
    };
    Ok(embedded(&project, path, option, &base, exported))
}

/// The first section: where a document of no section known is read from.
fn home_of(detail: &crate::model::Detail) -> String {
    detail
        .sections
        .first()
        .map(|s| s.id.clone())
        .unwrap_or_default()
}

/// One `![[path|option]]` of a document in `base`, drawn: `{"mermaid": …}`, a table's
/// `{"markdown": …}`, or why it cannot be drawn (`{"problem": …}`).
fn embedded(
    project: &crate::export::Project,
    path: &str,
    option: Option<&str>,
    base: &str,
    exported: bool,
) -> Value {
    // A table: its Markdown, put in as it is.
    if let Ok(crate::export::mermaid::Embed::Card(card)) =
        crate::export::mermaid::find_embed(path, option, base, project)
        && card.kind == crate::kinds::TABLE
    {
        return json!({
            "markdown": super::tables::markdown(&super::tables::build(project, card))
        });
    }
    match crate::export::mermaid::embedded_as(path, option, base, project, exported) {
        Ok(text) => json!({ "mermaid": text }),
        Err(why) => json!({ "problem": why }),
    }
}

/// What the `{{path}}`s of a document in the section `section` put in, as the export puts them
/// in: `{"values": {path: text}, "problems": {path: why}}`, the paths of one document read in one
/// request. The window fills a document's `{{…}}` from this, not from values of its own.
fn inserted_values(detail: &crate::model::Detail, request: &Value) -> Result<Value, String> {
    let paths: Vec<&str> = request
        .get("paths")
        .and_then(Value::as_array)
        .ok_or_else(|| text!("screen.inserted.pathsNeeded"))?
        .iter()
        .filter_map(Value::as_str)
        .collect();
    let base = request
        .get("section")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let project = crate::export::Project::new(detail);
    let base = if base.is_empty() {
        home_of(detail)
    } else {
        base.to_string()
    };
    let (values, problems) = put_in(&project, paths, &base);
    Ok(json!({ "values": values, "problems": problems }))
}

/// What each of `paths` puts in, read from `base` as the export puts it in: the texts, and why
/// one puts in nothing.
fn put_in<'a>(
    project: &crate::export::Project,
    paths: impl IntoIterator<Item = &'a str>,
    base: &str,
) -> (Map<String, Value>, Map<String, Value>) {
    let mut values = Map::new();
    let mut problems = Map::new();
    for path in paths {
        match crate::export::paths::inserted_value(project, path, base) {
            Ok(text) => values.insert(path.to_string(), text.into()),
            Err(why) => problems.insert(path.to_string(), why.into()),
        };
    }
    (values, problems)
}

/// What the window's document cards put in their text, many at once over one reading of the
/// project, so that a section's documents cost one request. Each document is
/// `{card, places, values, embeds}`: its card's id, and the `[[…]]`, `{{…}}` and
/// `![[…|option]]` (`{path, option}`) its text holds. Each is answered, in the order asked, with
/// `{card, missing, values, problems, embeds}`: the places that do not resolve from the card and
/// why (`missing`, as a path's check says it), the values put in and why one puts in nothing (as
/// `inserted_values`), and each embed drawn as `embed_diagram` draws it for the window, in the
/// order asked — a table's Markdown with its own places looked at too, as the window reads them.
fn document_parts(
    detail: &crate::model::Detail,
    main: Option<&crate::model::Detail>,
    files: Option<&crate::workdir::Overlay>,
    request: &Value,
) -> Result<Value, String> {
    use crate::export::Project;
    let documents = request
        .get("documents")
        .and_then(Value::as_array)
        .ok_or_else(|| text!("screen.documents.documentsNeeded"))?;
    let files_project = || {
        files.map(|o| {
            let (d, i) = o.typed();
            Project::indexed(d, i.clone()).with_code(Some(o.code.clone()))
        })
    };
    let code = || files.map(|o| o.code.clone());
    // The board reaches the working directories as main does: beside main.
    let project = match main {
        Some(main) => Project::new(detail).with_outside(Some(
            Project::new(main)
                .with_files(files_project())
                .with_code(code()),
        )),
        None => Project::new(detail)
            .with_files(files_project())
            .with_code(code()),
    };
    let home = home_of(detail);
    // The same diagram in the same section drawn once, however many documents put it in.
    let mut drawn: HashMap<(String, Option<String>, String), Value> = HashMap::new();
    let mut out = Vec::with_capacity(documents.len());
    for document in documents {
        let id = document
            .get("card")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let card = project.card(id);
        let base = card.map_or_else(|| home.clone(), |c| project.section_of(c));
        let listed = |key: &str| -> Vec<&str> {
            document
                .get(key)
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .collect()
        };
        let mut places: Vec<String> = listed("places").into_iter().map(str::to_string).collect();
        let mut embeds = Vec::new();
        for embed in document
            .get("embeds")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let path = embed
                .get("path")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let option = embed.get("option").and_then(Value::as_str);
            let said = drawn
                .entry((path.to_string(), option.map(str::to_string), base.clone()))
                .or_insert_with(|| embedded(&project, path, option, &base, false))
                .clone();
            if let Some(table) = said.get("markdown").and_then(Value::as_str) {
                places.extend(
                    crate::notetext::refs(table)
                        .into_iter()
                        .filter(|r| r.kind == crate::notetext::Kind::Place)
                        .map(|r| r.path),
                );
            }
            embeds.push(said);
        }
        let mut missing = Map::new();
        for path in places {
            if missing.contains_key(&path) {
                continue;
            }
            let hit = super::check::resolve_place(&project, &path, card, Some(&base));
            if !hit.ok {
                missing.insert(path, hit.hint.unwrap_or_default().into());
            }
        }
        let (values, problems) = put_in(&project, listed("values"), &base);
        out.push(json!({
            "card": id,
            "missing": missing,
            "values": values,
            "problems": problems,
            "embeds": embeds,
        }));
    }
    Ok(json!({ "documents": out }))
}

/// The project a request names by its id, there.
fn project_of<'a>(store: &Store, request: &'a Value, op: &str) -> Result<&'a str, String> {
    let project = text(request, "project", op, &text!("screen.what.projectId"))?;
    if store.project(project)?.is_none() {
        return Err(text!("screen.projectNotFound"));
    }
    Ok(project)
}

/// The tree a write is in, as the tools name it.
fn scope_of(tx: &Tx, project: &str, tree: &str) -> Result<Scope, String> {
    let project_name: String = tx
        .conn()
        .query_row("SELECT name FROM projects WHERE id = ?", [project], |r| {
            r.get(0)
        })
        .map_err(db)?;
    let tree_name = match tree {
        "" => "main".to_string(),
        BOARD => crate::store::BOARD_NAME.to_string(),
        id => tx
            .conn()
            .query_row("SELECT name FROM branches WHERE id = ?", [id], |r| r.get(0))
            .map_err(db)?,
    };
    Ok(Scope {
        project: project.to_string(),
        project_name,
        tree: tree.to_string(),
        tree_name,
    })
}

// MARK: - Reading

/// Every project, by name, as the project list shows it (`listProjects`, `ProjectSummary`): its
/// cards and sections counted in main as it is kept, whether it is archived or main is
/// locked, and whether it is public.
fn projects(conn: &Connection) -> Result<Value, String> {
    let count = |query: &str| -> Result<HashMap<String, i64>, String> {
        let mut stmt = conn.prepare(query).map_err(sql)?;
        stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))
            .map_err(sql)?
            .collect::<Result<_, _>>()
            .map_err(sql)
    };
    // A project's cards as a section counts them: not its card ghosts, nor a flow's nodes
    // (cards inside the flow, counted as the flow was before).
    let cards = count(
        "SELECT project_id, COUNT(*) FROM card_docs WHERE branch_id = '' AND removed = 0 AND kind NOT IN ('ghost', 'begin', 'process', 'run', 'eval', 'wait', 'join', 'end') GROUP BY project_id",
    )?;
    let sections = count(
        "SELECT project_id, COUNT(*) FROM section_docs WHERE branch_id = '' AND removed = 0 GROUP BY project_id",
    )?;
    let public: HashSet<String> = {
        let mut stmt = conn
            .prepare("SELECT project_id FROM project_public")
            .map_err(sql)?;
        stmt.query_map([], |r| r.get::<_, String>(0))
            .map_err(sql)?
            .collect::<Result<_, _>>()
            .map_err(sql)?
    };
    let mut stmt = conn
        .prepare(
            "SELECT id, name, slug, description, created_at, updated_at, tag_groups, archived_at, locked_at, ai_merge, export_language FROM projects ORDER BY name ASC",
        )
        .map_err(sql)?;
    type Kept = (
        crate::store::ProjectRow,
        Option<String>,
        Option<String>,
        bool,
    );
    let rows: Vec<Kept> = stmt
        .query_map([], |r| {
            Ok((
                crate::store::ProjectRow {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    slug: r.get(2)?,
                    description: r.get(3)?,
                    created_at: r.get(4)?,
                    updated_at: r.get(5)?,
                    tag_groups: r.get(6)?,
                    kinds: None,
                    export_language: r.get(10)?,
                },
                r.get(7)?,
                r.get(8)?,
                r.get::<_, i64>(9)? != 0,
            ))
        })
        .map_err(sql)?
        .collect::<Result<_, _>>()
        .map_err(sql)?;
    let list: Vec<Value> = rows
        .into_iter()
        .map(|(project, archived, locked, ai_merge)| {
            let Value::Object(mut out) = crate::detail::project_json(&project) else {
                return Value::Null;
            };
            out.insert(
                "schemaCount".into(),
                cards.get(&project.id).copied().unwrap_or(0).into(),
            );
            out.insert(
                "sectionCount".into(),
                sections.get(&project.id).copied().unwrap_or(1).into(),
            );
            out.insert("archivedAt".into(), archived.into());
            out.insert("lockedAt".into(), locked.into());
            out.insert("aiMerge".into(), ai_merge.into());
            out.insert("public".into(), public.contains(&project.id).into());
            Value::Object(out)
        })
        .collect();
    Ok(json!({ "projects": list }))
}

/// The project's branches, in their order (`listBranches`, `BranchInfo`).
fn branches(conn: &Connection, project: &str) -> Result<Value, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, project_id, name, sort_order, created_at, archived_at FROM branches WHERE project_id = ? AND name NOT LIKE '~%' ORDER BY sort_order, created_at, name",
        )
        .map_err(sql)?;
    let list = stmt
        .query_map([project], |r| {
            Ok(json!({
                "id": r.get::<_, String>(0)?,
                "projectId": r.get::<_, String>(1)?,
                "name": r.get::<_, String>(2)?,
                "sortOrder": r.get::<_, i64>(3)?,
                "createdAt": r.get::<_, String>(4)?,
                "archivedAt": r.get::<_, Option<String>>(5)?,
            }))
        })
        .map_err(sql)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sql)?;
    Ok(json!({ "branches": list }))
}

/// What a branch marks on its cards and sections (`branchMarks`, `BranchMarks`), by id: those it
/// changed, made and removed as `branch_read` lists them (`branch_diff`) — changed, one it says
/// at least one change of, a move to another section too —, and the cards it shows that main
/// changed since; what it removed as the screen reads cards (`gone`), to be drawn where it was;
/// and how many cards main changed, made or removed since (`mainMoved`, as `branch_read` and
/// `branch_list` count them). Nothing for main, nor for the board or the directory, which have
/// nothing of main's to differ from. main changed one the branch wrote when main's is not what
/// the copy was made from; one it never wrote, when main's is not what it remembers of it (or it
/// remembers nothing: made in main after the branch was).
fn branch_marks(reader: &Reader, project: &str, tree: &str) -> Result<Value, String> {
    let mut main_changed = BTreeSet::new();
    let mut ids = super::branches::DiffIds::default();
    let mut gone = crate::screen_detail::none_gone();
    let mut main_moved = 0;
    if !tree.is_empty() && !crate::store::is_own_tree(tree) {
        let diff = super::branches::branch_diff(reader, project, tree, false)?;
        let composed = reader.compose(project, Some(tree))?;
        main_changed.extend(
            composed
                .cards
                .iter()
                .map(|c| &c.id)
                .filter(|id| diff.ids.main_moved.contains(*id))
                .cloned(),
        );
        gone = crate::screen_detail::gone(
            reader,
            project,
            tree,
            &diff.ids.removed,
            &diff.ids.sections_removed,
        )?;
        main_moved = diff.main_moved();
        ids = diff.ids;
    }
    Ok(json!({
        "changed": ids.changed,
        "made": ids.made,
        "removed": ids.removed,
        "mainChanged": main_changed,
        "sectionsChanged": ids.sections_changed,
        "sectionsMade": ids.sections_made,
        "sectionsRemoved": ids.sections_removed,
        "gone": gone,
        "mainMoved": main_moved,
    }))
}

/// How the project's cards are drawn (`defaultSettings`, `LayoutSettings`): how big a card
/// is (the rows it shows, its width in points or null for `auto`), the grid units kept
/// clear around it, and how a formatdoc reads when it does not say.
fn layout(layout: &super::grid::Layout) -> Value {
    json!({
        "size": { "rows": layout.rows, "width": layout.width },
        "margin": layout.margin,
        "docView": layout.doc_view,
    })
}

/// What the project's working directories hold as read (`workdir_detail`): the directories (`sections`,
/// of the kind `directory`) and files (`schemas`, of the kind `file`) as `project_detail` says
/// entries, made once for each reading — none of a working directory's own section, which every
/// tree holds — and each working directory as set and as read here (`dirs`). Its `digest` is made
/// of the reading and the working directories; one the reader holds already (`since`) is answered
/// `unchanged`.
fn workdir_detail(conn: &Connection, project: &str, since: Option<&str>) -> Result<Value, String> {
    use sha2::{Digest, Sha256};
    let mounts = crate::workdir::mounts(conn, project)?;
    let dirs = super::directories::directories(conn, project)?;
    let folders = super::directories::folders(conn, project)?;
    let reads = if mounts.is_empty() {
        Vec::new()
    } else {
        crate::workdir::reads(conn, project)?
    };
    let turn = crate::workdir::turn(conn, project)?;
    let rows: Vec<Value> = mounts
        .iter()
        .map(|m| {
            let dir = dirs.iter().find(|d| d.id == m.dir);
            let read = reads.iter().find(|r| r.dir == m.dir);
            json!({
                "id": m.dir,
                "section": m.section,
                "name": m.name,
                "description": m.description,
                "readType": dir.map(|d| d.read_type.clone()),
                "exclude": dir.map(|d| d.exclude.clone()).unwrap_or_default(),
                "parsers": dir.map(|d| d.parsers.clone()).unwrap_or_default(),
                "folder": folders.get(&m.dir),
                "files": read.map(|r| r.files),
                "directories": read.map(|r| r.dirs),
                "problem": read.and_then(|r| r.problem.clone()),
                "warning": read.and_then(|r| r.note.clone()),
            })
        })
        .collect();
    let mut hasher = Sha256::new();
    hasher.update(format!("{turn:?}\u{1}{rows:?}").as_bytes());
    let digest: String = hasher.finalize()[..8]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    if since == Some(digest.as_str()) {
        return Ok(json!({ "digest": digest, "unchanged": true }));
    }
    let (sections, schemas) = match crate::workdir::overlay(conn, project)? {
        Some(overlay) => {
            let raw = &overlay.made().raw;
            let sections: Vec<Value> = raw["sections"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|s| s["kind"] == crate::kinds::DIRECTORY)
                .cloned()
                .collect();
            (
                sections,
                raw["schemas"].as_array().cloned().unwrap_or_default(),
            )
        }
        None => (Vec::new(), Vec::new()),
    };
    Ok(json!({
        "digest": digest,
        "dirs": rows,
        "sections": sections,
        "schemas": schemas,
    }))
}

/// What one commit changed (`commitChanges`, `CommitChange`): the project composed with the
/// rows it kept as they were before it, against as they were after it — the next commit's
/// before, or the row now. Sections first, then cards, each by path.
fn commit_changes(store: &Store, project: &str, tree: &str, seq: i64) -> Result<Value, String> {
    if crate::write::history_commit(&store.conn, project, tree, seq)?.is_none() {
        return Err(text!("history.commit.none", seq = seq));
    }
    let around = crate::write::history_around(&store.conn, project, tree, seq)?;
    if around.is_empty() {
        return Ok(json!({ "changes": [] }));
    }
    let kept: Vec<(String, String)> = around
        .iter()
        .map(|a| (a.table.name().to_string(), a.id.clone()))
        .collect();
    let before: Vec<OverRow> = around
        .iter()
        .map(|a| (a.table.name().to_string(), a.id.clone(), a.before.clone()))
        .collect();
    // None: the row now, as it is kept.
    let after: Vec<OverRow> = around
        .iter()
        .filter_map(|a| {
            a.after
                .clone()
                .map(|next| (a.table.name().to_string(), a.id.clone(), next))
        })
        .collect();
    let row = store
        .project(project)?
        .ok_or_else(|| text!("screen.projectNotFound"))?;
    let reader = store.reader();
    let branch = (!tree.is_empty()).then_some(tree);
    let compose =
        |rows: &[OverRow]| reader.compose_over(project, branch, Some(&Over { tree, rows }));
    let was_made = compose(&before)?;
    let is_made = compose(&after)?;
    let was = crate::model::typed(&crate::detail::of_made(&row, &was_made))?;
    let is = crate::model::typed(&crate::detail::of_made(&row, &is_made))?;
    let (was_project, is_project) = (
        crate::export::Project::new(&was),
        crate::export::Project::new(&is),
    );
    let docs = |made: &Composition| -> HashMap<String, Value> {
        made.cards
            .iter()
            .map(|c| (c.id.clone(), Value::Object(c.doc.clone())))
            .collect()
    };
    let (was_docs, is_docs) = (docs(&was_made), docs(&is_made));
    let path_of = |id: &str| is_project.named(id);
    let mut out: Vec<(bool, String, &str, Vec<String>)> = Vec::new();
    for (table, id) in &kept {
        if table == "section_docs" {
            let then = was.sections.iter().find(|s| &s.id == id);
            let now = is.sections.iter().find(|s| &s.id == id);
            match (then, now) {
                (None, Some(_)) => out.push((
                    true,
                    is_project.directory.full_section(id),
                    "add",
                    Vec::new(),
                )),
                (Some(_), None) => out.push((
                    true,
                    was_project.directory.full_section(id),
                    "remove",
                    Vec::new(),
                )),
                (Some(then), Some(now)) => {
                    let mut said = Vec::new();
                    let from = was_project.directory.full_section(id);
                    let to = is_project.directory.full_section(id);
                    if from != to {
                        said.push(format!("{from} → {to}"));
                    }
                    if then.description != now.description {
                        said.push(text!("screen.change.description"));
                    }
                    if then.kind != now.kind {
                        said.push(text!(
                            "screen.change.kind",
                            from = then.kind.as_deref().unwrap_or("-"),
                            to = now.kind.as_deref().unwrap_or("-")
                        ));
                    }
                    out.push((true, to, "change", said));
                }
                (None, None) => {}
            }
        } else {
            // A card, or a card ghost by its own path.
            match (was_project.named(id), is_project.named(id)) {
                (None, Some(now)) => out.push((false, now, "add", Vec::new())),
                (Some(then), None) => out.push((false, then, "remove", Vec::new())),
                (Some(from), Some(to)) => {
                    let mut said = Vec::new();
                    if from != to {
                        said.push(format!("{from} → {to}"));
                    }
                    if let (Some(a), Some(b)) = (was_docs.get(id), is_docs.get(id)) {
                        said.extend(crate::diff::changes(a, b, &path_of, None));
                    }
                    out.push((false, to, "change", said));
                }
                (None, None) => {}
            }
        }
    }
    out.sort_by(|a, b| (!a.0, &a.1).cmp(&(!b.0, &b.1)));
    Ok(
        json!({ "changes": out.iter().map(|(section, path, change, details)| json!({
        "path": path,
        "change": change,
        "section": section,
        "details": details,
    })).collect::<Vec<_>>() }),
    )
}

/// What a cut before `before` would let go (`cutCost`): how many commits, and how many
/// bytes of the history, as the project's size counts them.
fn cut_cost(conn: &Connection, project: &str, tree: &str, before: i64) -> Result<Value, String> {
    let one = |query: &str| -> Result<i64, String> {
        conn.query_row(query, params![project, tree, before], |r| r.get(0))
            .map_err(sql)
    };
    let commits =
        one("SELECT COUNT(*) FROM commits WHERE project_id = ? AND branch_id = ? AND seq < ?")?;
    let rows = one(
        "SELECT COALESCE(SUM(COALESCE(LENGTH(CAST(r.before AS BLOB)), 0)), 0) FROM commit_rows r JOIN commits c ON c.id = r.commit_id
          WHERE c.project_id = ? AND c.branch_id = ? AND c.seq < ?",
    )?;
    let lines = one(
        "SELECT COALESCE(SUM(LENGTH(CAST(l.summary AS BLOB))), 0) FROM commit_lines l JOIN commits c ON c.id = l.commit_id
          WHERE c.project_id = ? AND c.branch_id = ? AND c.seq < ?",
    )?;
    Ok(json!({ "commits": commits, "bytes": rows + lines }))
}

/// Where other sections' cards were put on each section's diagram (`sectionPositions`):
/// the diagram's section → the card → its cell, as the tree sees them (a branch's over
/// main's); and its section ghosts the same way, by the section each sums up
/// (`sectionGhosts`). Not a goal's card map: it is read with the goal (`itemPlaces`).
fn section_positions(conn: &Connection, project: &str, tree: &str) -> Result<Value, String> {
    let mut trees = Vec::new();
    if !crate::store::is_own_tree(tree) {
        trees.push("");
    }
    if !tree.is_empty() {
        trees.push(tree);
    }
    // Another section's card, and a section ghost (the card summing up another section), by
    // the section whose diagram they are on.
    let mut cards: BTreeMap<String, BTreeMap<String, Value>> = BTreeMap::new();
    let mut ghosts: BTreeMap<String, BTreeMap<String, Value>> = BTreeMap::new();
    let mut stmt = conn
        .prepare(
            "SELECT view, item_kind, item, x, y FROM places WHERE project_id = ?1 AND branch_id = ?2 AND item_kind IN ('card', 'section') AND view <> '' AND x IS NOT NULL AND y IS NOT NULL AND view NOT IN (SELECT id FROM card_docs WHERE project_id = ?1)",
        )
        .map_err(sql)?;
    for tree in trees {
        let rows = stmt
            .query_map(params![project, tree], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, i64>(3)?,
                    r.get::<_, i64>(4)?,
                ))
            })
            .map_err(sql)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(sql)?;
        for (section, kind, item, x, y) in rows {
            let to = if kind == "section" {
                &mut ghosts
            } else {
                &mut cards
            };
            to.entry(section)
                .or_default()
                .insert(item, json!({ "x": x, "y": y }));
        }
    }
    Ok(json!({ "positions": cards, "sectionGhosts": ghosts }))
}

// MARK: - Writing

/// Where the window put a box, as it says — whatever else is there: what it worked out
/// clear of the rest itself (`setPlaces`, the arrangement, `moveByHand`,
/// `setSectionPosition`). Its size stays as the tree has it. No history.
fn put_box(tx: &mut Tx, project: &str, tree: &str, given: &Value) -> Result<(), String> {
    let text = |key: &str| given.get(key).and_then(Value::as_str);
    let view = text("view").unwrap_or_default();
    let kind = text("kind").unwrap_or("card");
    if !["card", "section"].contains(&kind) {
        return Err(text!("screen.box.kind", kind = kind));
    }
    let item = text("item")
        .filter(|i| !i.is_empty())
        .ok_or_else(|| text!("screen.box.itemNeeded"))?;
    // Whole units, within the reach (`Grid.units`).
    let unit = |key: &str| -> Result<i64, String> {
        let n = given
            .get(key)
            .and_then(Value::as_f64)
            .ok_or_else(|| text!("screen.box.cellsNeeded", item = item))?;
        Ok(if n.is_nan() {
            0
        } else {
            n.round().clamp(-REACH, REACH) as i64
        })
    };
    let (x, y) = (unit("x")?, unit("y")?);
    let pinned = given
        .get("pinned")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    // Its size as the tree has it: its own box, or main's beneath a branch.
    let mut trees = vec![tree];
    if !tree.is_empty() && !crate::store::is_own_tree(tree) {
        trees.push("");
    }
    let mut spot = Spot::default();
    for at in trees {
        let found: Option<(Option<i64>, Option<i64>)> = tx
            .conn()
            .query_row(
                "SELECT w, h FROM places WHERE project_id = ? AND branch_id = ? AND view = ? AND item_kind = ? AND item = ?",
                params![project, at, view, kind, item],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(db)?;
        if let Some((w, h)) = found {
            spot.w = w;
            spot.h = h;
            break;
        }
    }
    spot.x = Some(x);
    spot.y = Some(y);
    spot.pinned = pinned;
    tx.place(project, tree, view, kind, item, Some(&spot))
}

/// Where a row or a node moved to: its names from the card down, before and after.
type Move = (Vec<String>, Vec<String>);

/// `moves`: where rows or nodes moved to, by their names from the card down
/// (`[{"from": ["intro"], "to": ["opening"]}]`).
fn moves_of(raw: &Value) -> Result<Vec<Move>, String> {
    let names = |m: &Value, key: &str| -> Option<Vec<String>> {
        let list = m.get(key)?.as_array()?;
        let out: Option<Vec<String>> = list
            .iter()
            .map(|n| n.as_str().filter(|s| !s.is_empty()).map(str::to_string))
            .collect();
        out.filter(|o| !o.is_empty())
    };
    raw.as_array()
        .and_then(|list| {
            list.iter()
                .map(|m| Some((names(m, "from")?, names(m, "to")?)))
                .collect::<Option<Vec<_>>>()
        })
        .ok_or_else(|| text!("screen.moves.shape"))
}
