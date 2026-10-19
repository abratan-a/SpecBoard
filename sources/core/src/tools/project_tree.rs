//! What is directly under each root, written (`project_tree`): the sections there compared by name with the project's new list, the same in
//! every tree — main and each branch for `main_sections`, the board for `board_sections`.
//!
//! - One there by the same name is kept, put in the new order. Its kind is its own: one of the
//!   same name and another kind is refused (add it by another name and move what it holds).
//! - One renamed (`renames`' `{"top_section": "Spec", "to": "Specs"}`, or `section_update`'s
//!   `name` on it) keeps everything, its paths followed.
//! - One new is made, empty.
//! - One taken out goes, when it holds nothing in any tree (no card, no card ghost, no section
//!   inside); else it is moved, with what it holds, into the root's last section `_evacuation_`
//!   (of the kind free, made then), named apart there (`Charts_2`), of the kind free with every
//!   section inside it and without an export default.
//!
//! main's and the board's documents are written as every write is (in their history); a branch's
//! copies of them (and the base they were copied from) are written again in place, as a kind of
//! card renamed is (`own_kinds::rename_all`), so a later merge finds nothing changed.

use crate::kinds::{EVACUATION, ProjectTree, TopSection, Tree};
use crate::store::{BOARD, Composition, Reader, sql};
use crate::write::{Tx, db, new_id};
use rusqlite::{Connection, params};
use serde_json::{Map, Value};
use std::collections::HashSet;

type Doc = Map<String, Value>;

/// A section directly under a root, as main's or the board's documents have it.
#[derive(Debug, Clone)]
struct Top {
    id: String,
    name: String,
    kind: Option<String>,
}

/// What writing `project_tree` does to one root's sections.
#[derive(Debug, Clone, Default)]
pub(crate) struct TreeProject {
    tree: Option<Tree>,
    /// Kept, by id, in the new order (the new name where renamed).
    keep: Vec<(String, String)>,
    /// Made: their kind and name, and their place.
    create: Vec<(TopSection, usize)>,
    /// Moved into `_evacuation_`: id, name, the name it has there.
    evacuate: Vec<(String, String, String)>,
    /// Gone, empty everywhere: id and name.
    delete: Vec<(String, String)>,
    /// Renamed: id, from, to.
    rename: Vec<(String, String, String)>,
    /// The new order, by name.
    order: Vec<String>,
    /// `_evacuation_` as it is, when there.
    evacuation: Option<String>,
}

impl TreeProject {
    fn tree(&self) -> Tree {
        self.tree.unwrap_or(Tree::Main)
    }

    /// Whether anything is made, taken away or renamed (an order alone changes nothing kept).
    pub(crate) fn changes(&self) -> bool {
        !self.create.is_empty()
            || !self.evacuate.is_empty()
            || !self.delete.is_empty()
            || !self.rename.is_empty()
    }

    /// As `check` and the answer say it: kept, made, moved away, gone, renamed.
    pub(crate) fn json(&self) -> Value {
        let mut out = Map::new();
        out.insert("tree".into(), self.tree().name().into());
        let names = |list: Vec<String>| Value::Array(list.into_iter().map(Value::from).collect());
        out.insert(
            "keep".into(),
            names(self.keep.iter().map(|(_, n)| n.clone()).collect()),
        );
        out.insert(
            "create".into(),
            names(self.create.iter().map(|(t, _)| t.name.clone()).collect()),
        );
        out.insert(
            "evacuate".into(),
            Value::Array(
                self.evacuate
                    .iter()
                    .map(|(_, name, there)| {
                        let mut row = Map::new();
                        row.insert("section".into(), name.clone().into());
                        row.insert(
                            "to".into(),
                            format!("{EVACUATION}/{}", crate::pathtext::quote(there)).into(),
                        );
                        Value::Object(row)
                    })
                    .collect(),
            ),
        );
        out.insert(
            "delete".into(),
            names(self.delete.iter().map(|(_, n)| n.clone()).collect()),
        );
        out.insert(
            "rename".into(),
            Value::Array(
                self.rename
                    .iter()
                    .map(|(_, from, to)| {
                        let mut row = Map::new();
                        row.insert("from".into(), from.clone().into());
                        row.insert("to".into(), to.clone().into());
                        Value::Object(row)
                    })
                    .collect(),
            ),
        );
        Value::Object(out)
    }

    /// As main's history says it, or none when nothing but the order changes.
    pub(crate) fn said(&self) -> Option<crate::text::Keyed> {
        use crate::text::{Keyed, List};
        let mut parts: Vec<Keyed> = Vec::new();
        if !self.create.is_empty() {
            parts.push(keyed!(
                "projectTree.line.created",
                names = List::of(self.create.iter().map(|(t, _)| t.name.clone()))
            ));
        }
        if !self.rename.is_empty() {
            parts.push(keyed!(
                "projectTree.line.renamed",
                names = List::of(self.rename.iter().map(|(_, f, t)| format!("{f} → {t}")))
            ));
        }
        if !self.evacuate.is_empty() {
            parts.push(keyed!(
                "projectTree.line.evacuated",
                evacuation = EVACUATION,
                names = List::of(self.evacuate.iter().map(|(_, n, _)| n.clone()))
            ));
        }
        if !self.delete.is_empty() {
            parts.push(keyed!(
                "projectTree.line.deleted",
                names = List::of(self.delete.iter().map(|(_, n)| n.clone()))
            ));
        }
        // The parts one after another, the last one first.
        let mut said = parts.pop()?;
        while let Some(head) = parts.pop() {
            said = keyed!("projectTree.line.then", head, rest = said);
        }
        let tree = match self.tree() {
            Tree::Main => keyed!("kindDefs.tree.main"),
            Tree::Board => keyed!("kindDefs.tree.board"),
        };
        Some(keyed!("projectTree.line.changed", tree, parts = said))
    }
}

/// The tree's id as the documents keep it: `""` for main, `board` for the board.
fn tree_id(tree: Tree) -> &'static str {
    match tree {
        Tree::Main => "",
        Tree::Board => BOARD,
    }
}

/// The branches of a project, by id.
fn branches(conn: &Connection, project: &str) -> Result<Vec<String>, String> {
    let mut stmt = conn
        .prepare("SELECT id FROM branches WHERE project_id = ? ORDER BY id")
        .map_err(sql)?;
    let rows = stmt
        .query_map([project], |r| r.get::<_, String>(0))
        .map_err(sql)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sql)?;
    Ok(rows)
}

/// What a tree sees, `""` for main.
fn compose(conn: &Connection, project: &str, tree: &str) -> Result<Composition, String> {
    Reader { conn }.compose(project, (!tree.is_empty()).then_some(tree))
}

fn text<'d>(doc: &'d Doc, key: &str) -> &'d str {
    doc.get(key).and_then(Value::as_str).unwrap_or_default()
}

/// The root of a tree as it sees it, and what is directly under it.
fn tops(seen: &Composition) -> (Option<String>, Vec<Top>) {
    let root = seen
        .sections
        .iter()
        .find(|s| s.doc.get("parent_id").and_then(Value::as_str).is_none())
        .map(|s| s.id.clone());
    let under = seen
        .sections
        .iter()
        .filter(|s| {
            root.is_some() && s.doc.get("parent_id").and_then(Value::as_str) == root.as_deref()
        })
        .map(|s| Top {
            id: s.id.clone(),
            name: text(&s.doc, "name").to_string(),
            kind: s
                .doc
                .get("kind")
                .and_then(Value::as_str)
                .map(str::to_string),
        })
        .collect();
    (root, under)
}

/// A section and every section inside it, as the tree sees them.
fn subtree(seen: &Composition, id: &str) -> HashSet<String> {
    let mut inside: HashSet<String> = HashSet::from([id.to_string()]);
    let mut grew = true;
    while grew {
        grew = false;
        for s in &seen.sections {
            if let Some(parent) = s.doc.get("parent_id").and_then(Value::as_str)
                && inside.contains(parent)
                && inside.insert(s.id.clone())
            {
                grew = true;
            }
        }
    }
    inside
}

/// Whether the section holds anything as the tree sees it: a card or a card ghost, or a section.
fn holds_anything(seen: &Composition, id: &str) -> bool {
    let inside = subtree(seen, id);
    inside.len() > 1
        || seen
            .cards
            .iter()
            .any(|c| inside.contains(text(&c.doc, "section")))
}

/// What writing `tree` (with the renames of the sections directly under a root, `from` → `to`)
/// would do in each root, or every reason it cannot: read from the documents, nothing written.
pub(crate) fn planned(
    conn: &Connection,
    project: &str,
    tree: &ProjectTree,
    renames: &[(String, String)],
) -> Result<Vec<TreeProject>, Vec<String>> {
    let fail = |e: String| vec![e];
    let branches = branches(conn, project).map_err(fail)?;
    let mut problems = Vec::new();
    let mut out = Vec::new();
    let lower = |s: &str| crate::pathtext::fold(s);
    // A rename names one there now, to a name in the new list.
    for (from, to) in renames {
        if tree.place_of(to).is_none() {
            problems.push(text!("projectTree.rename.targetMissing", from, to));
        }
        if tree.place_of(from).is_some() && lower(from) != lower(to) {
            problems.push(text!("projectTree.rename.sourceStays", from, to));
        }
    }
    for kind in Tree::ALL {
        let id = tree_id(kind);
        let seen = compose(conn, project, id).map_err(fail)?;
        let (_, current) = tops(&seen);
        let evacuation = current
            .iter()
            .find(|t| t.name == EVACUATION)
            .map(|t| t.id.clone());
        let current: Vec<Top> = current
            .into_iter()
            .filter(|t| t.name != EVACUATION)
            .collect();
        let mut project_here = TreeProject {
            tree: Some(kind),
            evacuation,
            ..TreeProject::default()
        };
        let mut matched: HashSet<String> = HashSet::new();
        for (at, wanted) in tree.of(kind).iter().enumerate() {
            project_here.order.push(wanted.name.clone());
            // There by this name, or renamed into it.
            let found = current
                .iter()
                .find(|t| lower(&t.name) == lower(&wanted.name))
                .or_else(|| {
                    renames
                        .iter()
                        .filter(|(_, to)| lower(to) == lower(&wanted.name))
                        .find_map(|(from, _)| {
                            current.iter().find(|t| lower(&t.name) == lower(from))
                        })
                });
            match found {
                Some(top) => {
                    if top.kind.as_deref() != Some(wanted.kind.as_str()) {
                        problems.push(text!(
                            "projectTree.kind.cannotChange",
                            tree = kind.key(),
                            at,
                            name = top.name,
                            kind = top
                                .kind
                                .clone()
                                .unwrap_or_else(|| text!("projectTree.kind.none"))
                        ));
                    }
                    matched.insert(top.id.clone());
                    if top.name != wanted.name {
                        project_here.rename.push((
                            top.id.clone(),
                            top.name.clone(),
                            wanted.name.clone(),
                        ));
                    }
                    project_here
                        .keep
                        .push((top.id.clone(), wanted.name.clone()));
                }
                None => {
                    // Not one a branch made there itself by the same name.
                    if kind == Tree::Main {
                        for branch in &branches {
                            let theirs = compose(conn, project, branch).map_err(fail)?;
                            let (_, under) = tops(&theirs);
                            if let Some(clash) = under.iter().find(|t| {
                                lower(&t.name) == lower(&wanted.name)
                                    && !current.iter().any(|c| c.id == t.id)
                            }) {
                                problems.push(text!(
                                    "projectTree.branch.hasIt",
                                    tree = kind.key(),
                                    at,
                                    name = clash.name
                                ));
                            }
                        }
                    }
                    project_here.create.push((wanted.clone(), at));
                }
            }
        }
        // What is taken out: gone when empty in every tree, else moved away.
        let mut away_names: Vec<String> = match &project_here.evacuation {
            Some(evac) => seen
                .sections
                .iter()
                .filter(|s| s.doc.get("parent_id").and_then(Value::as_str) == Some(evac))
                .map(|s| lower(text(&s.doc, "name")))
                .collect(),
            None => Vec::new(),
        };
        for top in current.iter().filter(|t| !matched.contains(&t.id)) {
            let mut held = holds_anything(&seen, &top.id);
            if kind == Tree::Main && !held {
                for branch in &branches {
                    let theirs = compose(conn, project, branch).map_err(fail)?;
                    if holds_anything(&theirs, &top.id) {
                        held = true;
                        break;
                    }
                }
            }
            if held {
                let mut name = top.name.clone();
                let mut n = 2;
                while away_names.contains(&lower(&name)) {
                    name = format!("{}_{n}", top.name);
                    n += 1;
                }
                away_names.push(lower(&name));
                project_here
                    .evacuate
                    .push((top.id.clone(), top.name.clone(), name));
            } else {
                project_here.delete.push((top.id.clone(), top.name.clone()));
            }
        }
        out.push(project_here);
    }
    if problems.is_empty() {
        Ok(out)
    } else {
        Err(problems)
    }
}

/// A key of a section's document, left out at what it falls back to (`sections::set`).
fn set(doc: &mut Doc, key: &str, value: Value) {
    let fallback = match &value {
        Value::Null => true,
        Value::Number(n) => n.as_i64() == Some(0),
        Value::Array(a) => a.is_empty(),
        _ => false,
    };
    if fallback {
        doc.shift_remove(key);
    } else {
        doc.insert(key.to_string(), value);
    }
}

/// What the branches kept of a section of main as they were made, let go of: the project's tree is
/// the project's, and every branch sees it as main has it now.
fn shared(conn: &Connection, project: &str, id: &str) -> Result<(), String> {
    conn.execute(
        "DELETE FROM branch_frozen WHERE project_id = ? AND tbl = 'section_docs' AND id = ?",
        params![project, id],
    )
    .map_err(sql)?;
    Ok(())
}

/// A branch's own rows of a section (its copy and the base it was copied from) written again in
/// place with `change`, its columns with them; what the branches kept of main's let go of.
fn rewrite_copies(
    conn: &Connection,
    project: &str,
    id: &str,
    change: &dyn Fn(&mut Doc),
) -> Result<(), String> {
    shared(conn, project, id)?;
    let mut stmt = conn
        .prepare(
            "SELECT branch_id, doc, base FROM section_docs WHERE project_id = ? AND id = ? AND branch_id NOT IN ('', ?)",
        )
        .map_err(sql)?;
    let rows: Vec<(String, Option<String>, Option<String>)> = stmt
        .query_map(params![project, id, BOARD], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })
        .map_err(sql)?
        .collect::<Result<_, _>>()
        .map_err(sql)?;
    let changed = |t: &Option<String>| -> Option<Doc> {
        let mut doc = t
            .as_deref()
            .and_then(|t| serde_json::from_str::<Value>(t).ok())?
            .as_object()?
            .clone();
        change(&mut doc);
        Some(doc)
    };
    for (branch, doc, base) in rows {
        let doc_now = changed(&doc);
        let base_now = changed(&base);
        let head = doc_now.as_ref().or(base_now.as_ref());
        let name = head
            .map(|d| text(d, "name").to_string())
            .unwrap_or_default();
        let parent = head.and_then(|d| {
            d.get("parent_id")
                .and_then(Value::as_str)
                .map(str::to_string)
        });
        conn.execute(
            "UPDATE section_docs SET doc = ?, base = ?, name = ?, parent_id = ? WHERE project_id = ? AND branch_id = ? AND id = ?",
            params![
                doc_now.map(|d| Value::Object(d).to_string()),
                base_now.map(|d| Value::Object(d).to_string()),
                name,
                parent,
                project,
                branch,
                id
            ],
        )
        .map_err(db)?;
    }
    Ok(())
}

/// The sections a branch made itself inside one (no base: not main's), by branch.
fn made_inside(
    conn: &Connection,
    project: &str,
    id: &str,
) -> Result<Vec<(String, String)>, String> {
    let mut out = Vec::new();
    for branch in branches(conn, project)? {
        let seen = compose(conn, project, &branch)?;
        let inside = subtree(&seen, id);
        let mut stmt = conn
            .prepare(
                "SELECT id FROM section_docs WHERE project_id = ? AND branch_id = ? AND base IS NULL AND removed = 0",
            )
            .map_err(sql)?;
        let own: Vec<String> = stmt
            .query_map(params![project, branch], |r| r.get::<_, String>(0))
            .map_err(sql)?
            .collect::<Result<_, _>>()
            .map_err(sql)?;
        out.extend(
            own.into_iter()
                .filter(|s| s != id && inside.contains(s))
                .map(|s| (branch.clone(), s)),
        );
    }
    Ok(out)
}

/// What `planned` found, written: in main (and through it every branch) and on the board, the
/// paths written in the project following what was renamed or moved away, in every tree.
pub(crate) fn apply(tx: &mut Tx, project: &str, projects: &[TreeProject]) -> Result<(), String> {
    let branch_ids = branches(tx.conn(), project)?;
    // Each tree as it was, for the paths that follow.
    let mut before: Vec<(String, crate::model::Detail)> = Vec::new();
    let moves = projects
        .iter()
        .any(|p| !p.rename.is_empty() || !p.evacuate.is_empty());
    if moves {
        for tree in projects
            .iter()
            .map(|p| tree_id(p.tree()).to_string())
            .chain(branch_ids.iter().cloned())
        {
            let branch = (!tree.is_empty()).then_some(tree.as_str());
            let detail = crate::model::typed(&crate::detail::read(&tx.reader(), project, branch)?)?;
            before.push((tree, detail));
        }
    }
    for p in projects {
        let tree = tree_id(p.tree());
        let seen = compose(tx.conn(), project, tree)?;
        let (Some(root), _) = tops(&seen) else {
            continue;
        };
        let doc_of = |id: &str| -> Option<Doc> {
            seen.sections
                .iter()
                .find(|s| s.id == id)
                .map(|s| s.doc.clone())
        };
        // Renamed, and every kept one in the new order.
        for (id, name) in &p.keep {
            let Some(mut doc) = doc_of(id) else { continue };
            let at = p
                .order
                .iter()
                .position(|n| crate::pathtext::same(n, name))
                .unwrap_or_default() as i64;
            doc.insert("name".into(), name.clone().into());
            set(&mut doc, "sort_order", at.into());
            tx.put_section(project, tree, &doc)?;
            let name = name.clone();
            rewrite_copies(tx.conn(), project, id, &|d: &mut Doc| {
                d.insert("name".into(), name.clone().into());
                set(d, "sort_order", at.into());
            })?;
        }
        for (top, at) in &p.create {
            // A working directory's section is known by the directory's id (`workdir::dir_id`),
            // the same in every tree.
            let made = match &top.directory {
                Some(dir) if top.kind == crate::kinds::WORKDIR => crate::workdir::dir_id(dir, ""),
                _ => new_id(),
            };
            let mut doc = Map::new();
            doc.insert("id".into(), made.clone().into());
            doc.insert("parent_id".into(), root.clone().into());
            doc.insert("name".into(), top.name.clone().into());
            set(&mut doc, "sort_order", (*at as i64).into());
            doc.insert("created_at".into(), tx.now()?.into());
            doc.insert("kind".into(), top.kind.clone().into());
            tx.put_section(project, tree, &doc)?;
            shared(tx.conn(), project, &made)?;
        }
        // A goals section made on the board holds where the card actions move cards.
        if p.tree() == Tree::Board
            && p.create
                .iter()
                .any(|(top, _)| top.kind == crate::kinds::GOALS)
        {
            crate::board_move::made(tx, project)?;
        }
        // Moved away into `_evacuation_`, made first when it is not there.
        if !p.evacuate.is_empty() {
            let evacuation = match &p.evacuation {
                Some(id) => id.clone(),
                None => {
                    let made = new_id();
                    let mut doc = Map::new();
                    doc.insert("id".into(), made.clone().into());
                    doc.insert("parent_id".into(), root.clone().into());
                    doc.insert("name".into(), EVACUATION.into());
                    set(&mut doc, "sort_order", (p.order.len() as i64).into());
                    doc.insert("created_at".into(), tx.now()?.into());
                    doc.insert("kind".into(), "free".into());
                    tx.put_section(project, tree, &doc)?;
                    shared(tx.conn(), project, &made)?;
                    made
                }
            };
            let free = |d: &mut Doc| {
                d.insert("kind".into(), "free".into());
                d.shift_remove("export_to");
                d.shift_remove("missing");
            };
            let mut next = seen
                .sections
                .iter()
                .filter(|s| s.doc.get("parent_id").and_then(Value::as_str) == Some(&evacuation))
                .count() as i64;
            for (id, _, there) in &p.evacuate {
                let inside = subtree(&seen, id);
                for inner in &inside {
                    let Some(mut doc) = doc_of(inner) else {
                        continue;
                    };
                    free(&mut doc);
                    let mut moved_too: Option<(String, String, i64)> = None;
                    if inner == id {
                        doc.insert("parent_id".into(), evacuation.clone().into());
                        doc.insert("name".into(), there.clone().into());
                        set(&mut doc, "sort_order", next.into());
                        moved_too = Some((evacuation.clone(), there.clone(), next));
                    }
                    tx.put_section(project, tree, &doc)?;
                    rewrite_copies(tx.conn(), project, inner, &|d: &mut Doc| {
                        free(d);
                        if let Some((parent, name, order)) = &moved_too {
                            d.insert("parent_id".into(), parent.clone().into());
                            d.insert("name".into(), name.clone().into());
                            set(d, "sort_order", (*order).into());
                        }
                    })?;
                }
                // What a branch made inside it, free too.
                if p.tree() == Tree::Main {
                    for (branch, inner) in made_inside(tx.conn(), project, id)? {
                        let doc: Option<String> = tx
                            .conn()
                            .query_row(
                                "SELECT doc FROM section_docs WHERE project_id = ? AND branch_id = ? AND id = ?",
                                params![project, branch, inner],
                                |r| r.get(0),
                            )
                            .map_err(db)?;
                        if let Some(mut d) = doc
                            .and_then(|t| serde_json::from_str::<Value>(&t).ok())
                            .and_then(|v| v.as_object().cloned())
                        {
                            free(&mut d);
                            tx.conn()
                                .execute(
                                    "UPDATE section_docs SET doc = ? WHERE project_id = ? AND branch_id = ? AND id = ?",
                                    params![Value::Object(d).to_string(), project, branch, inner],
                                )
                                .map_err(db)?;
                        }
                    }
                }
                next += 1;
            }
        }
        // Gone: empty in every tree. A branch's copies of it go too.
        for (id, _) in &p.delete {
            tx.remove_section(project, tree, id)?;
            shared(tx.conn(), project, id)?;
            if p.tree() == Tree::Main {
                tx.conn()
                    .execute(
                        "DELETE FROM section_docs WHERE project_id = ? AND id = ? AND branch_id NOT IN ('', ?)",
                        params![project, id, BOARD],
                    )
                    .map_err(db)?;
                tx.conn()
                    .execute(
                        "DELETE FROM places WHERE project_id = ?1 AND (view = ?2 OR (item_kind = 'section' AND item = ?2))",
                        params![project, id],
                    )
                    .map_err(db)?;
            }
        }
        if let Some(said) = p.said() {
            tx.line(
                project,
                tree,
                "project.update",
                "project",
                Some(project),
                said,
            )?;
        }
    }
    // The paths written in the project, after what was renamed or moved away, in every tree.
    for (tree, detail) in &before {
        super::follow::follow_unscoped(
            tx,
            project,
            tree,
            detail,
            &super::follow::InnerMoves::new(),
        )?;
    }
    Ok(())
}
