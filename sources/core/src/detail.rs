//! A project read whole, as the core's typed reading takes it (`model::typed`: the exports, the
//! checks and the tools; the screen reads `screen_detail`, made from this) —
//! (`Repository.getProjectDetail` over what
//! `WorkingCopy` spreads out, in the Swift app): the project, its cards, the relations
//! between them, and its sections — everything a `ProjectDetail` holds. A part that is
//! absent is left out; every other is written, empty or not.

use crate::directory::{CardRef, Directory, Entry, GhostRef, SectionRef};
use crate::store::{Composition, Place, Reader};
use crate::wire::{self, Obj};
use serde_json::{Map, Value, json};
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

/// The project `project_id` as `branch` sees it: None for main, a branch's id, or the board.
pub fn read(store: &Reader, project_id: &str, branch: Option<&str>) -> Result<Value, String> {
    read_kept(store, project_id, branch).map(|kept| kept.raw)
}

/// A tree's reading, with what it was made of — what the tree saw of the project
/// (`Reader::compose`), the project's row and every path in full (`of_made_named`) —: what a
/// write keeps to bring it up to what it writes (`tools::Seen::in_tx`, `of_made_over`).
pub(crate) struct Kept {
    pub raw: Value,
    pub made: Composition,
    pub project: crate::store::ProjectRow,
    pub named: HashMap<String, String>,
}

/// `read`, and what it was made of (`Kept`).
pub(crate) fn read_kept(
    store: &Reader,
    project_id: &str,
    branch: Option<&str>,
) -> Result<Kept, String> {
    read_kept_as(store, project_id, branch, false)
}

/// `read_kept`, with the working directories' sections whole when `whole` says so
/// (`workdir::put_whole`): every directory and file read, as a call that reads or writes in them
/// sees the tree.
pub(crate) fn read_kept_as(
    store: &Reader,
    project_id: &str,
    branch: Option<&str>,
    whole: bool,
) -> Result<Kept, String> {
    let project = store
        .project(project_id)?
        .ok_or_else(|| text!("detail.error.projectMissing", project_id))?;
    if let Some(b) = branch
        && b != crate::store::BOARD
        && !store.branch_exists(project_id, b)?
    {
        return Err(text!("detail.error.branchMissing", b));
    }
    let mut made = store.compose(project_id, branch)?;
    if whole && branch != Some(crate::store::BOARD) {
        crate::workdir::put_whole(store.conn, project_id, &mut made)?;
    }
    let (raw, named) = of_made_named(&project, &made);
    Ok(Kept {
        raw,
        made,
        project,
        named,
    })
}

/// The keys of a card's `fields` that are said in its own entry alone (`card_json`'s `label`,
/// `description` and `body`, `model::Card`'s too): nothing else of the project is read from them.
const SAID_ALONE: [&str; 3] = ["label", "description", "body"];

/// The kinds whose entry says `SAID_ALONE` as they are and nothing more of them: no form (the
/// definitions' kinds and the project's own keep their fields whole in it), no container whose
/// entry says what it holds by them (a flow's nodes are not cards of these), not a node, not a ghost.
const BROUGHT_UP: [&str; 8] = [
    "entity",
    "value",
    "enum",
    "note",
    "flow",
    "store",
    "freedoc",
    "formatdoc",
];

/// A card's entry in the project read whole (`read`'s JSON, its typed reading and what the tree
/// saw of it) brought up to its document written anew, when all that changed is said in its entry
/// alone (`SAID_ALONE`): what reading it all again would make, without reading it all again.
/// False, with nothing changed, when it cannot be: the document changed more, or is not the one read.
pub(crate) fn bring_card(
    raw: &mut Value,
    detail: &mut crate::model::Detail,
    docs: &mut HashMap<String, Obj>,
    id: &str,
    before: &Obj,
    after: &Obj,
) -> bool {
    if docs.get(id) != Some(before) {
        return false;
    }
    // The rest of it, as it is kept: `fields` with nothing else in it is as none (a card's first
    // label makes it).
    let rest = |doc: &Obj| -> String {
        let mut doc = doc.clone();
        if let Some(Value::Object(fields)) = doc.get_mut("fields") {
            for key in SAID_ALONE {
                fields.shift_remove(key);
            }
            if fields.is_empty() {
                doc.shift_remove("fields");
            }
        }
        Value::Object(doc).to_string()
    };
    if rest(before) != rest(after) {
        return false;
    }
    let Some(card) = detail.schemas.iter_mut().find(|c| c.id == id) else {
        return false;
    };
    if !BROUGHT_UP.contains(&card.kind.as_str()) || card.form.is_some() {
        return false;
    }
    let Some(entry) = raw["schemas"]
        .as_array_mut()
        .and_then(|all| all.iter_mut().find(|c| c["id"] == id))
        .and_then(Value::as_object_mut)
    else {
        return false;
    };
    let fields = fields_of(after);
    // Its own: one shared with a reading kept from before is copied first.
    let card = std::rc::Rc::make_mut(card);
    for key in SAID_ALONE {
        let said = text(&fields, key).unwrap_or_default();
        match key {
            "label" => card.label = said.clone(),
            "description" => card.description = said.clone(),
            _ => card.body = said.clone(),
        }
        entry.insert(key.to_string(), said.into());
    }
    docs.insert(id.to_string(), after.clone());
    true
}

/// The project as what a tree sees of it (`made`) reads, whole.
pub(crate) fn of_made(project: &crate::store::ProjectRow, made: &Composition) -> Value {
    of_made_named(project, made).0
}

/// `of_made`, and every card's and card ghost's path in full by id (`full_paths`): what a reading
/// kept to be brought up to writes later is told apart by (`of_made_over`).
pub(crate) fn of_made_named(
    project: &crate::store::ProjectRow,
    made: &Composition,
) -> (Value, HashMap<String, String>) {
    let project_id = project.id.as_str();
    let defs = crate::kinds::Defs::of_project(project.kinds.as_deref());
    let sections = sections(made, project_id, &defs);
    let (cards, ghosts, inner) = cards(made, project_id, defs.cards());
    let named = names(&sections, &cards, &ghosts, &inner);
    let raw = assemble(
        project,
        &defs,
        &sections,
        cards,
        &ghosts,
        inner,
        made,
        &named,
        &|_| true,
        None,
    )
    .expect("assembling with nothing kept to reuse cannot fail");
    (raw, named)
}

/// What a pointer's target is called: in full, from the root of its tree — a card ghost by its
/// own path, as it was written through it; a card inside a container on from the container's.
fn names(
    sections: &[Section],
    cards: &[Card],
    ghosts: &[Ghost],
    inner: &[Card],
) -> HashMap<String, String> {
    let home = sections.first().map(|s| s.id.clone()).unwrap_or_default();
    let paths = Directory::new(&section_refs(sections), &[], &[]);
    let section_path = |section: &Option<String>| {
        paths.full_section(&section.clone().unwrap_or_else(|| home.clone()))
    };
    full_paths(cards, ghosts, inner, &section_path)
}

/// The project read whole, from its cards in their order (`cards`, each whole — or, where `full`
/// says not, in outline, its entry taken from `before`), its card ghosts and its sections.
/// None when an entry to be taken from `before` is not there.
#[allow(clippy::too_many_arguments)]
fn assemble(
    project: &crate::store::ProjectRow,
    defs: &crate::kinds::Defs,
    sections: &[Section],
    mut cards: Vec<Card>,
    ghosts: &[Ghost],
    inner: Vec<Card>,
    made: &Composition,
    named: &HashMap<String, String>,
    full: &dyn Fn(&str) -> bool,
    mut before: Option<&mut Before>,
) -> Option<Value> {
    // Every card and card ghost there is named (`full_paths`).
    let known = |id: &str| named.contains_key(id);
    // The keys the core's typed reading takes a card's lines in (the exports and the checks
    // read the form of before): a field's `ref` and `pointsAt`, a store's `holds`, a flow's
    // `notes`, a task's `links`, a shape's `payloads`.
    for card in cards.iter_mut().filter(|c| full(&c.id)) {
        legacy_lines(card, defs, named);
        card.fields = build_fields(&card.field_rows, &known, named);
    }
    // And what a container holds, in the keys the typed reading takes it in: a flow's
    // `nodes`, a goal's `items` and where they sit on its card map.
    let items = goal_items(&cards);
    held_before(&mut cards, &inner, made, named, defs.cards(), full, &items);

    let refs: Vec<CardRef> = card_refs(&cards)
        .into_iter()
        .chain(card_refs(&inner))
        .collect();
    let directory = Directory::new(&section_refs(sections), &refs, &ghost_refs(ghosts));
    let all: Vec<&Card> = cards.iter().chain(inner.iter()).collect();
    let target = |i: usize| Target {
        id: all[i].id.as_str(),
        fields: all[i].fields.as_slice(),
    };
    let mut relations = Vec::new();
    let mut schemas = Vec::with_capacity(cards.len());
    let there = |id: &str| named.contains_key(id);
    for card in &cards {
        if full(&card.id) {
            relations.extend(relations_of(card, &target, &directory, defs, named));
            schemas.push(card_json(card));
        } else {
            let before = before.as_deref_mut()?;
            relations.extend(before.lines_of(&card.id, full, &there));
            schemas.push(Before::take(&mut before.schemas, &card.id, full, &there)?);
        }
    }
    let mut inner_entries = Vec::with_capacity(inner.len());
    for card in &inner {
        if full(&card.id) {
            inner_entries.push(inner_json(card));
        } else {
            let before = before.as_deref_mut()?;
            inner_entries.push(Before::take(&mut before.inner, &card.id, full, &there)?);
        }
    }

    let mut out = Map::new();
    out.insert("project".into(), project_json(project));
    out.insert("schemas".into(), Value::Array(schemas));
    out.insert("relations".into(), Value::Array(relations));
    out.insert(
        "sections".into(),
        Value::Array(sections.iter().map(section_json).collect()),
    );
    out.insert(
        "ghosts".into(),
        Value::Array(
            ghosts
                .iter()
                .map(|g| ghost_json(g, &all, &directory))
                .collect(),
        ),
    );
    out.insert("inner".into(), Value::Array(inner_entries));
    out.insert("kinds".into(), kinds_json(project.kinds.as_deref()));
    Some(Value::Object(out))
}

/// The project read whole as `of_made` reads it (`raw`), brought up in place to cards written since
/// that are where they were and named as they were — none made or let go of, renamed, moved or put
/// elsewhere among the cards: the
/// entries of `dirty` (the written cards, their containers, …) worked out again from their
/// documents, each where it was, and their lines; the names (`named`), the order and the directory
/// (`directory`, as `export::Index` makes it of this reading) are as they were. None when a card
/// of `dirty` has no entry to be put in.
pub(crate) fn bring_over(
    project: &crate::store::ProjectRow,
    made: &Composition,
    docs: &HashMap<String, Obj>,
    raw: &mut Value,
    named: &HashMap<String, String>,
    directory: &Directory,
    dirty: &HashSet<String>,
) -> Option<()> {
    let project_id = project.id.as_str();
    let defs = crate::kinds::Defs::of_project(project.kinds.as_deref());
    let kinds = defs.cards();
    let places = |key: &str| -> HashMap<String, usize> {
        raw[key]
            .as_array()
            .into_iter()
            .flatten()
            .enumerate()
            .map(|(i, e)| (e["id"].as_str().unwrap_or_default().to_string(), i))
            .collect()
    };
    let at = places("schemas");
    let inner_at = places("inner");
    let mut cards: Vec<Card> = Vec::new();
    let mut inner: Vec<Card> = Vec::new();
    for id in dirty {
        let doc = docs.get(id)?;
        match class_of(doc) {
            Class::Card if at.contains_key(id) => {
                cards.push(card_of(id, doc, made, project_id, kinds))
            }
            Class::Inner if inner_at.contains_key(id) => {
                inner.push(inner_of(id, doc, made, project_id))
            }
            _ => return None,
        }
    }
    cards.sort_by_key(|c| at[&c.id]);
    let known = |id: &str| named.contains_key(id);
    for card in &mut cards {
        legacy_lines(card, &defs, named);
        card.fields = build_fields(&card.field_rows, &known, named);
    }
    // What the containers among them hold: the cards inside each in the cards' order, a flow's
    // nodes whole.
    fn entries<'v>(raw: &'v Value, key: &str) -> &'v [Value] {
        raw[key].as_array().map(Vec::as_slice).unwrap_or(&[])
    }
    let mut items: HashMap<String, Vec<String>> = HashMap::new();
    for e in entries(raw, "schemas") {
        if let Some(container) = e["in"].as_str() {
            items
                .entry(container.to_string())
                .or_default()
                .push(e["id"].as_str().unwrap_or_default().to_string());
        }
    }
    let flows: HashSet<&str> = cards
        .iter()
        .filter(|c| c.kind == "flow")
        .map(|c| c.id.as_str())
        .collect();
    let nodes: Vec<Card> = entries(raw, "inner")
        .iter()
        .filter(|e| e["in"].as_str().is_some_and(|f| flows.contains(f)))
        .map(|e| {
            let id = e["id"].as_str().unwrap_or_default();
            docs.get(id).map(|doc| inner_of(id, doc, made, project_id))
        })
        .collect::<Option<_>>()?;
    held_before(&mut cards, &nodes, made, named, kinds, &|_| true, &items);
    for card in &cards {
        raw["schemas"][at[&card.id]] = card_json(card);
    }
    for card in &inner {
        raw["inner"][inner_at[&card.id]] = inner_json(card);
    }
    // Their lines anew, read against the entries as they are now; every other card's as it was.
    let fresh: HashMap<String, Vec<Value>> = {
        let schemas = entries(raw, "schemas");
        let inner_entries = entries(raw, "inner");
        let target = |i: usize| match schemas.get(i) {
            Some(e) => Target {
                id: e["id"].as_str().unwrap_or_default(),
                fields: e["fields"].as_array().map(Vec::as_slice).unwrap_or(&[]),
            },
            None => Target {
                id: inner_entries[i - schemas.len()]["id"]
                    .as_str()
                    .unwrap_or_default(),
                fields: &[],
            },
        };
        cards
            .iter()
            .map(|c| {
                (
                    c.id.clone(),
                    relations_of(c, &target, directory, &defs, named),
                )
            })
            .collect()
    };
    let mut before: HashMap<String, Vec<Value>> = HashMap::new();
    if let Some(Value::Array(lines)) = raw.get_mut("relations").map(std::mem::take) {
        for r in lines {
            let from = r["fromSchemaId"].as_str().unwrap_or_default().to_string();
            before.entry(from).or_default().push(r);
        }
    }
    let mut lines = Vec::new();
    for e in entries(raw, "schemas") {
        let id = e["id"].as_str().unwrap_or_default();
        match fresh.get(id) {
            Some(block) => lines.extend(block.iter().cloned()),
            None => lines.extend(before.remove(id).unwrap_or_default()),
        }
    }
    raw["relations"] = Value::Array(lines);
    raw["project"] = project_json(project);
    Some(())
}

/// A reading's entries taken apart to be used again (`of_made_over`): its cards', its inner
/// cards' and its lines, each in their order (`assemble` takes them as it goes, passing over those
/// of the cards worked out again or gone), and the order its cards were in.
pub(crate) struct Before {
    schemas: std::collections::VecDeque<Value>,
    inner: std::collections::VecDeque<Value>,
    relations: std::collections::VecDeque<Value>,
    order: Vec<String>,
    inner_order: Vec<String>,
    ghost_order: Vec<String>,
}

impl Before {
    pub(crate) fn of(raw: Value) -> Before {
        let Value::Object(mut raw) = raw else {
            return Before {
                schemas: Default::default(),
                inner: Default::default(),
                relations: Default::default(),
                order: Vec::new(),
                inner_order: Vec::new(),
                ghost_order: Vec::new(),
            };
        };
        let mut list = |key: &str| match raw.shift_remove(key) {
            Some(Value::Array(list)) => list,
            _ => Vec::new(),
        };
        let id_of = |v: &Value| v["id"].as_str().unwrap_or_default().to_string();
        let schemas = list("schemas");
        let inner = list("inner");
        let relations = list("relations");
        let ghosts = list("ghosts");
        Before {
            order: schemas.iter().map(id_of).collect(),
            inner_order: inner.iter().map(id_of).collect(),
            ghost_order: ghosts.iter().map(id_of).collect(),
            schemas: schemas.into(),
            inner: inner.into(),
            relations: relations.into(),
        }
    }

    /// The entry of `id` from `list`, the next of those kept: the ones before it of cards worked out
    /// again (`full`) or gone (not `there`) passed over. None when the next kept one is another's.
    fn take(
        list: &mut std::collections::VecDeque<Value>,
        id: &str,
        full: &dyn Fn(&str) -> bool,
        there: &dyn Fn(&str) -> bool,
    ) -> Option<Value> {
        loop {
            let front = list.front()?["id"].as_str().unwrap_or_default();
            if front == id {
                return list.pop_front();
            }
            if !full(front) && there(front) {
                return None;
            }
            list.pop_front();
        }
    }

    /// The lines `id` leaves, the next of those kept (as `take`).
    fn lines_of(
        &mut self,
        id: &str,
        full: &dyn Fn(&str) -> bool,
        there: &dyn Fn(&str) -> bool,
    ) -> Vec<Value> {
        let mut out = Vec::new();
        while let Some(front) = self.relations.front() {
            let from = front["fromSchemaId"].as_str().unwrap_or_default();
            if from == id {
                out.extend(self.relations.pop_front());
            } else if out.is_empty() && (full(from) || !there(from)) {
                self.relations.pop_front();
            } else {
                break;
            }
        }
        out
    }
}

/// Told the cards whose paths are not as they were (their own or a container's named otherwise or
/// put elsewhere, made or let go of), the cards whose entries are worked out again (`of_made_over`).
pub(crate) type DirtyOf<'a> = dyn Fn(&HashSet<String>) -> Option<HashSet<String>> + 'a;

/// The cards whose documents say they are inside the card `id` (`in`), as the documents are now.
pub(crate) type HeldBy<'a> = dyn Fn(&str) -> Vec<String> + 'a;

/// What `of_made_over` makes: the project read whole, every card's and card ghost's path in full,
/// the cards whose entries were worked out again and the cards whose paths changed or went.
pub(crate) struct MadeOver {
    pub raw: Value,
    pub paths: HashMap<String, String>,
    pub dirty: HashSet<String>,
    pub repathed: HashSet<String>,
}

/// The project read whole as `of_made` reads it, made from the reading before (`raw`, what the
/// tree saw then) and the cards written since (`written`, by id: their documents as they are now
/// in `docs`, or gone from it), not worked out whole again: the order and the directory are made again of every card in outline, and
/// the paths as they were (`paths`, what `of_made_named` says of the reading before) brought up to
/// the cards written and what they hold (`held`, `names_over`); then `dirty_of`, told the cards
/// whose paths changed, says the cards whose entries are worked out again — the written ones and
/// those whose entries say anything of them — and every other card's entry is the one before. None
/// when it cannot be so (a card ghost written, a written card's place among the cards tied with
/// another's, or what `dirty_of` cannot say): the project is read whole again instead. Also the
/// paths, the cards worked out again and the cards whose paths changed.
#[allow(clippy::too_many_arguments)]
pub(crate) fn of_made_over(
    project: &crate::store::ProjectRow,
    made: &Composition,
    docs: &HashMap<String, Obj>,
    raw: Value,
    written: &HashSet<String>,
    mut paths: HashMap<String, String>,
    held: &HeldBy,
    dirty_of: &DirtyOf,
) -> Option<MadeOver> {
    let project_id = project.id.as_str();
    let defs = crate::kinds::Defs::of_project(project.kinds.as_deref());
    let kinds = defs.cards();
    let sections = sections(made, project_id, &defs);
    let mut before = Before::of(raw);
    // The card ghosts as they were (none of them is written).
    let ghosts: Vec<Ghost> = before
        .ghost_order
        .iter()
        .map(|id| docs.get(id).map(|doc| ghost_of(id, doc, made, project_id)))
        .collect::<Option<_>>()?;
    // The cards in their order: those not written where they were, each written one where its
    // place among the cards puts it — a place another has too leaves the order to the store.
    let placed = |list: &[String], class: Class| -> Option<Vec<Card>> {
        let mut out: Vec<Card> = list
            .iter()
            .filter(|id| !written.contains(*id))
            .map(|id| docs.get(id).map(|doc| outline_of(id, doc, class, kinds)))
            .collect::<Option<_>>()?;
        let mut ids: Vec<&String> = written
            .iter()
            .filter(|id| docs.get(*id).is_some_and(|d| class_of(d) == class))
            .collect();
        ids.sort();
        for id in ids {
            let card = outline_of(id, &docs[id], class, kinds);
            if out.iter().any(|c| by_place(c, &card) == Ordering::Equal) {
                return None;
            }
            let at = out.partition_point(|c| by_place(c, &card) == Ordering::Less);
            out.insert(at, card);
        }
        Some(out)
    };
    let outline = placed(&before.order, Class::Card)?;
    let inner_outline = placed(&before.inner_order, Class::Inner)?;
    // Every card of the tree is in one of them.
    if outline.len() + inner_outline.len() + ghosts.len() != docs.len() {
        return None;
    }
    // The paths: those of the written cards and what they hold made again, every other as it was;
    // all of them made again when a container's chain runs too deep to say so.
    let repathed = match names_over(&mut paths, &sections, docs, written, held) {
        Some(repathed) => repathed,
        None => {
            let now = names(&sections, &outline, &ghosts, &inner_outline);
            let repathed = now
                .iter()
                .filter(|(id, p)| paths.get(*id) != Some(*p))
                .map(|(id, _)| id.clone())
                .chain(paths.keys().filter(|id| !now.contains_key(*id)).cloned())
                .collect();
            paths = now;
            repathed
        }
    };
    let named = paths;
    let dirty = dirty_of(&repathed)?;
    let cards: Vec<Card> = outline
        .into_iter()
        .map(|c| match dirty.contains(&c.id) {
            true => card_of(&c.id, &docs[&c.id], made, project_id, kinds),
            false => c,
        })
        .collect();
    let inner: Vec<Card> = inner_outline
        .into_iter()
        .map(|c| match dirty.contains(&c.id) {
            true => inner_of(&c.id, &docs[&c.id], made, project_id),
            false => c,
        })
        .collect();
    let raw = assemble(
        project,
        &defs,
        &sections,
        cards,
        &ghosts,
        inner,
        made,
        &named,
        &|id| dirty.contains(id),
        Some(&mut before),
    )?;
    Some(MadeOver {
        raw,
        paths: named,
        dirty,
        repathed,
    })
}

/// The paths as they were (`paths`, as `names` made them), brought up in place to the cards written
/// since (`written`, their documents as they are now in `docs`, or gone from it): only theirs and
/// those of what they hold (`held`, on down) can be otherwise — the sections are as they were, a
/// card let go of holds none, and no other card is named, put or held otherwise. Each made as
/// `full_paths` makes it. The cards whose paths changed or went; None, with `paths` as it was,
/// when a container's chain runs too deep to say (`full_paths` cuts it short).
fn names_over(
    paths: &mut HashMap<String, String>,
    sections: &[Section],
    docs: &HashMap<String, Obj>,
    written: &HashSet<String>,
    held: &HeldBy,
) -> Option<HashSet<String>> {
    // The written cards and what they hold, on down (a card ghost's path is its own).
    let mut touched: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut queue: Vec<String> = written.iter().cloned().collect();
    while let Some(id) = queue.pop() {
        if !seen.insert(id.clone()) {
            continue;
        }
        if docs.get(&id).is_some_and(|d| class_of(d) == Class::Ghost) {
            continue;
        }
        queue.extend(held(&id).into_iter().filter(|c| !seen.contains(c)));
        touched.push(id);
    }
    let made = Remade {
        docs,
        was: paths,
        touched: touched.iter().map(String::as_str).collect(),
        sections: Directory::new(&section_refs(sections), &[], &[]),
        home: sections.first().map(|s| s.id.clone()).unwrap_or_default(),
    };
    let mut memo: HashMap<String, String> = HashMap::new();
    let mut now: Vec<(&String, Option<String>)> = Vec::with_capacity(touched.len());
    for id in &touched {
        let p = match docs.contains_key(id) {
            true => Some(made.path(id, &mut memo, 0)?),
            false => None,
        };
        now.push((id, p));
    }
    let mut repathed = HashSet::new();
    for (id, p) in now {
        let changed = match p {
            Some(p) => paths.insert(id.clone(), p.clone()).as_ref() != Some(&p),
            None => paths.remove(id).is_some(),
        };
        if changed {
            repathed.insert(id.clone());
        }
    }
    Some(repathed)
}

/// What `names_over` makes a touched card's path again of: the documents as they are now, the
/// paths as they were, the cards touched and the sections' paths.
struct Remade<'a> {
    docs: &'a HashMap<String, Obj>,
    was: &'a HashMap<String, String>,
    touched: HashSet<&'a str>,
    sections: Directory,
    home: String,
}

impl Remade<'_> {
    /// The card `id`'s path as `full_paths` makes it: a touched container's made again too, any
    /// other's as it was. None past 32 containers deep, or when one is not named.
    fn path(&self, id: &str, memo: &mut HashMap<String, String>, depth: usize) -> Option<String> {
        if let Some(p) = memo.get(id) {
            return Some(p.clone());
        }
        let doc = self.docs.get(id)?;
        let name = text(doc, "name").unwrap_or_default();
        // A container is there when it is a card of the tree (not a card ghost).
        let there = |c: &str| {
            self.docs
                .get(c)
                .is_some_and(|d| class_of(d) != Class::Ghost)
        };
        let p = match text(doc, "in").filter(|c| !c.is_empty() && there(c)) {
            Some(_) if depth >= 32 => return None,
            Some(container) if self.touched.contains(container.as_str()) => {
                crate::pathtext::member(&self.path(&container, memo, depth + 1)?, &name)
            }
            Some(container) => crate::pathtext::member(self.was.get(&container)?, &name),
            None => {
                let section = text(doc, "section").unwrap_or_else(|| self.home.clone());
                crate::pathtext::card_path(&self.sections.full_section(&section), &name)
            }
        };
        memo.insert(id.to_string(), p.clone());
        Some(p)
    }
}

/// Every card's and card ghost's path in full, by id: `Section:Name`, and a card inside a
/// container its container's and its own name (`Behavior:Checkout.review`). A container
/// that is not there leaves the card named in its section.
fn full_paths(
    cards: &[Card],
    ghosts: &[Ghost],
    inner: &[Card],
    section: &dyn Fn(&Option<String>) -> String,
) -> HashMap<String, String> {
    let by_id: HashMap<&str, &Card> = cards
        .iter()
        .chain(inner.iter())
        .map(|c| (c.id.as_str(), c))
        .collect();
    fn path(
        id: &str,
        by_id: &HashMap<&str, &Card>,
        section: &dyn Fn(&Option<String>) -> String,
        memo: &mut HashMap<String, String>,
        depth: usize,
    ) -> Option<String> {
        if let Some(p) = memo.get(id) {
            return Some(p.clone());
        }
        let card = by_id.get(id)?;
        let p = match card.inside.as_deref().filter(|c| by_id.contains_key(c)) {
            Some(container) if depth < 32 => crate::pathtext::member(
                &path(container, by_id, section, memo, depth + 1)?,
                &card.name,
            ),
            _ => crate::pathtext::card_path(&section(&card.section_id), &card.name),
        };
        memo.insert(id.to_string(), p.clone());
        Some(p)
    }
    let mut out: HashMap<String, String> = ghosts
        .iter()
        .map(|g| {
            (
                g.id.clone(),
                crate::pathtext::card_path(&section(&g.section_id), &g.name),
            )
        })
        .collect();
    for id in by_id.keys().copied().collect::<Vec<_>>() {
        path(id, &by_id, section, &mut out, 0);
    }
    out
}

/// The kinds of card, of section and of line the project has, as their definitions write them:
/// the built-in ones (`builtin: true`), then the project's own (`projects.kinds`).
pub(crate) fn kinds_json(own: Option<&str>) -> Value {
    let defs = crate::kinds::Defs::of_project(own);
    let own: Value = own
        .and_then(|t| serde_json::from_str(t).ok())
        .unwrap_or(Value::Null);
    let mut out = Map::new();
    // The kinds of card whole (each with what it takes from its parents), as the screen
    // draws and writes a card by them.
    out.insert(
        "cards".into(),
        Value::Array(crate::kinds::written_whole(own.get("cards"), defs.cards())),
    );
    for list in ["sections", "lines"] {
        let mut all: Vec<Value> = crate::kinds::builtin_written(list)
            .iter()
            .map(|d| {
                let mut d = d.as_object().cloned().unwrap_or_default();
                d.insert("builtin".into(), true.into());
                // A built-in kind of section's mark as the project writes it over (`icons`).
                if list == "sections" {
                    let name = d.get("section").and_then(Value::as_str).unwrap_or_default();
                    if let Some(icon) =
                        crate::kinds::icon_over(d.get("icon"), own["icons"].get(name))
                    {
                        d.insert("icon".into(), icon);
                        d.insert("iconChanged".into(), true.into());
                    }
                }
                Value::Object(d)
            })
            .collect();
        all.extend(own[list].as_array().cloned().unwrap_or_default());
        out.insert(list.into(), Value::Array(all));
    }
    // What a flow's `via` may be — the built-in values, then the project's — as the screen's choices.
    out.insert("vias".into(), defs.vias.clone().into());
    Value::Object(out)
}

// MARK: - The project

pub(crate) fn project_json(project: &crate::store::ProjectRow) -> Value {
    let groups = json_text(project.tag_groups.as_deref());
    let mut out = json!({
        "id": project.id,
        "name": project.name,
        "slug": project.slug,
        "description": project.description,
        "tagGroups": wire::list_or_empty(groups.as_ref(), wire::tag_group),
        "createdAt": project.created_at,
        "updatedAt": project.updated_at,
        "exportLanguage": project.export_language,
    });
    // The project's own kinds of card, section and line, as it keeps them.
    if let Some(kinds) = json_text(project.kinds.as_deref()).filter(|k| !k.is_null()) {
        out["kinds"] = kinds;
    }
    out
}

/// A JSON column's text, as the JSON it holds.
fn json_text(text: Option<&str>) -> Option<Value> {
    serde_json::from_str(text?).ok()
}

// MARK: - Reading a document's columns

fn text(doc: &Obj, key: &str) -> Option<String> {
    match doc.get(key)? {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

fn int(doc: &Obj, key: &str) -> i64 {
    match doc.get(key) {
        Some(Value::Number(n)) => n
            .as_i64()
            .or_else(|| n.as_f64().map(|f| f as i64))
            .unwrap_or(0),
        Some(Value::Bool(b)) => *b as i64,
        _ => 0,
    }
}

// MARK: - Sections

struct Section {
    id: String,
    project_id: String,
    name: String,
    description: String,
    sort_order: i64,
    parent_id: Option<String>,
    kind: Option<String>,
    created_at: String,
    missing: bool,
}

fn sections(made: &Composition, project_id: &str, defs: &crate::kinds::Defs) -> Vec<Section> {
    let mut flat: Vec<Section> = made
        .sections
        .iter()
        .map(|item| {
            let doc = &item.doc;
            Section {
                id: item.id.clone(),
                project_id: project_id.to_string(),
                name: text(doc, "name").unwrap_or_default(),
                description: text(doc, "description").unwrap_or_default(),
                sort_order: int(doc, "sort_order"),
                parent_id: text(doc, "parent_id"),
                kind: text(doc, "kind").filter(|k| defs.knows_section(k)),
                created_at: text(doc, "created_at").unwrap_or_default(),
                missing: doc.get("missing").and_then(Value::as_bool) == Some(true),
            }
        })
        .collect();
    // Directly under the root, in the project's order (`project_tree`): what the
    // window lists them by.
    let root = flat
        .iter()
        .find(|s| s.parent_id.is_none())
        .map(|s| s.id.clone());
    for section in flat.iter_mut() {
        if root.is_some() && section.parent_id == root {
            section.sort_order = defs.root_place(&section.name);
        }
    }
    // As SP lists them: tops first, by order, made, name (without case), then as a tree.
    flat.sort_by(|a, b| {
        (a.parent_id.is_some())
            .cmp(&b.parent_id.is_some())
            .then(a.sort_order.cmp(&b.sort_order))
            .then(a.created_at.cmp(&b.created_at))
            .then_with(|| nocase(&a.name, &b.name))
    });
    tree_order(flat)
}

/// SQLite's NOCASE: ASCII letters without case, everything else as bytes.
fn nocase(a: &str, b: &str) -> Ordering {
    a.bytes()
        .map(|c| c.to_ascii_lowercase())
        .cmp(b.bytes().map(|c| c.to_ascii_lowercase()))
}

/// Parents before children, siblings in the order given (`ProjectSection.treeOrder`).
fn tree_order(flat: Vec<Section>) -> Vec<Section> {
    let known: HashSet<String> = flat.iter().map(|s| s.id.clone()).collect();
    let mut children: HashMap<String, Vec<usize>> = HashMap::new();
    for (index, section) in flat.iter().enumerate() {
        let parent = section
            .parent_id
            .clone()
            .filter(|p| known.contains(p))
            .unwrap_or_default();
        children.entry(parent).or_default().push(index);
    }
    let mut order: Vec<usize> = Vec::new();
    let mut seen: HashSet<usize> = HashSet::new();
    fn visit(
        parent: &str,
        flat: &[Section],
        children: &HashMap<String, Vec<usize>>,
        seen: &mut HashSet<usize>,
        order: &mut Vec<usize>,
    ) {
        for &index in children.get(parent).map(Vec::as_slice).unwrap_or(&[]) {
            if seen.insert(index) {
                order.push(index);
                visit(&flat[index].id, flat, children, seen, order);
            }
        }
    }
    visit("", &flat, &children, &mut seen, &mut order);
    for index in 0..flat.len() {
        if !seen.contains(&index) {
            order.push(index);
        }
    }
    let mut slots: Vec<Option<Section>> = flat.into_iter().map(Some).collect();
    order.into_iter().filter_map(|i| slots[i].take()).collect()
}

fn section_refs(sections: &[Section]) -> Vec<SectionRef> {
    sections
        .iter()
        .map(|s| SectionRef {
            id: s.id.clone(),
            name: s.name.clone(),
            parent_id: s.parent_id.clone(),
        })
        .collect()
}

fn section_json(s: &Section) -> Value {
    let mut out = Map::new();
    out.insert("id".into(), s.id.clone().into());
    out.insert("projectId".into(), s.project_id.clone().into());
    out.insert("name".into(), s.name.clone().into());
    out.insert("description".into(), s.description.clone().into());
    out.insert("sortOrder".into(), s.sort_order.into());
    if let Some(p) = &s.parent_id {
        out.insert("parentId".into(), p.clone().into());
    }
    if let Some(k) = &s.kind {
        out.insert("kind".into(), k.clone().into());
    }
    if s.missing {
        out.insert("missing".into(), true.into());
    }
    Value::Object(out)
}

// MARK: - Cards

#[derive(Default)]
struct Card {
    id: String,
    project_id: String,
    name: String,
    label: String,
    kind: String,
    description: String,
    tags: Vec<String>,
    payloads: Vec<Value>,
    metadata: Obj,
    rules: Vec<Value>,
    absent_fields: Vec<Value>,
    discriminator: Option<String>,
    enum_open: bool,
    place: Option<Place>,
    sort_order: i64,
    field_rows: Vec<FieldRow>,
    fields: Vec<Value>,
    values: Vec<Value>,
    uses: Vec<Value>,
    signed_by: Vec<String>,
    verified_by: Vec<String>,
    next: Vec<Value>,
    notes: Vec<String>,
    body: String,
    rows: Vec<Value>,
    trigger: Option<Value>,
    steps: Vec<Value>,
    nodes: Vec<Value>,
    holds: Vec<String>,
    diagram: Option<String>,
    section_id: Option<String>,
    custom_fields: Obj,
    custom_rows: Vec<Value>,
    /// A kind the screen reads in the definitions' form: its fields and rows as kept.
    form: Option<Value>,
    /// A goal's card map: where the cards it holds sit on it, in the order it holds them.
    item_places: Vec<(String, (i64, i64))>,
    /// A value or an enum drawn as a card of its own (how it looks).
    standalone: bool,
    /// Its lines, as its document keeps them.
    connections: Vec<Value>,
    /// The container it is inside (a card's id): a flow's for a node, a goal's for what it
    /// holds.
    inside: Option<String>,
}

/// A card ghost (`kind: ghost`), read apart from the cards: its name in its section, what
/// it points at (by id), its place among the cards and where it sits on its section's
/// diagram.
struct Ghost {
    id: String,
    project_id: String,
    name: String,
    section_id: Option<String>,
    target: Option<String>,
    sort_order: i64,
    place: Option<Place>,
}

/// The kind of a card ghost's document.
pub(crate) const GHOST: &str = "ghost";

/// The kinds a card may be. One a document names that is not known reads as an entity.
const CARD_KINDS: [&str; 17] = [
    "entity",
    "value",
    "enum",
    "note",
    "flow",
    "store",
    "instance",
    "table",
    "freedoc",
    "formatdoc",
    "stackdoc",
    "svg",
    "html",
    "frame",
    "goal",
    "task",
    "file",
];

fn card_kind(raw: Option<String>, kinds: &crate::kinds::Kinds) -> String {
    match raw.as_deref() {
        Some(k) if CARD_KINDS.contains(&k) || kinds.is_own(k) => k.to_string(),
        _ => "entity".to_string(),
    }
}

/// The kinds the screen reads in the definitions' form (`form`), not in SP's keys.
const DEFINED: [&str; 9] = [
    "goal", "task", "file", "stackdoc", "instance", "table", "svg", "html", "frame",
];

/// A document's fields (`fields`), and the rows of one kind (`rows`).
fn fields_of(doc: &Obj) -> Obj {
    doc.get("fields")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default()
}

fn rows_of(doc: &Obj, kind: &str) -> Vec<Value> {
    doc.get("rows")
        .and_then(|r| r.get(kind))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

/// A row's fields, and the rows of one kind under it.
fn row_fields(row: &Value) -> Obj {
    row.get("fields")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default()
}

fn row_children(row: &Value, kind: &str) -> Vec<Value> {
    row.get("rows")
        .and_then(|r| r.get(kind))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

/// Which of the project's lists a document's card is in: the cards, those put only inside a
/// container (a flow's nodes: `inner`), or the card ghosts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Class {
    Card,
    Inner,
    Ghost,
}

pub(crate) fn class_of(doc: &Obj) -> Class {
    match text(doc, "kind") {
        Some(k) if k == GHOST => Class::Ghost,
        Some(k) if crate::kinds::inner_only(&k) => Class::Inner,
        _ => Class::Card,
    }
}

fn ghost_of(id: &str, doc: &Obj, made: &Composition, project_id: &str) -> Ghost {
    Ghost {
        id: id.to_string(),
        project_id: project_id.to_string(),
        name: text(doc, "name").unwrap_or_default(),
        section_id: text(doc, "section"),
        target: text(&fields_of(doc), "target").filter(|t| !t.is_empty()),
        sort_order: int(doc, "order"),
        place: made.places.get(&(String::new(), id.to_string())).cloned(),
    }
}

fn card_of(
    id: &str,
    doc: &Obj,
    made: &Composition,
    project_id: &str,
    kinds: &crate::kinds::Kinds,
) -> Card {
    let f = fields_of(doc);
    let kind = card_kind(text(doc, "kind"), kinds);
    let mut place = made.places.get(&(String::new(), id.to_string())).cloned();
    // A card inside a container is sized where it sits, on its container's card map.
    if let Some(container) = text(doc, "in").filter(|i| !i.is_empty()) {
        let size = made
            .places
            .get(&(container, id.to_string()))
            .and_then(|p| p.size);
        place = match place {
            Some(mut p) => {
                p.size = size;
                Some(p)
            }
            None => size.map(|size| crate::store::Place {
                x: None,
                y: None,
                pinned: false,
                size: Some(size),
                view: None,
                version: None,
            }),
        };
    }
    // A kind the project defines is read in the definitions' form
    // alone: its label, description, tags and metadata as every kind's, the rest as
    // kept — none of it in the keys of before.
    if kinds.is_own(&kind) {
        return Card {
            id: id.to_string(),
            project_id: project_id.to_string(),
            name: text(doc, "name").unwrap_or_default(),
            label: text(&f, "label").unwrap_or_default(),
            description: text(&f, "description").unwrap_or_default(),
            tags: wire::strings_or_empty(f.get("tags")),
            metadata: wire::string_map(f.get("metadata")),
            place,
            sort_order: int(doc, "order"),
            section_id: text(doc, "section"),
            form: Some(form_of(doc)),
            connections: crate::kinds::connections_of(&Value::Object(doc.clone())).to_vec(),
            inside: text(doc, "in").filter(|i| !i.is_empty()),
            kind,
            ..Card::default()
        };
    }
    let form = DEFINED.contains(&kind.as_str()).then(|| form_of(doc));
    let format_rows: Vec<Value> = rows_of(doc, "rows").iter().map(format_doc).collect();
    Card {
        id: id.to_string(),
        project_id: project_id.to_string(),
        name: text(doc, "name").unwrap_or_default(),
        label: text(&f, "label").unwrap_or_default(),
        description: text(&f, "description").unwrap_or_default(),
        tags: wire::strings_or_empty(f.get("tags")),
        metadata: wire::string_map(f.get("metadata")),
        rules: wire::rules(f.get("rules")),
        absent_fields: wire::list_or_empty(f.get("absentFields"), wire::absent_field),
        discriminator: text(&f, "discriminator"),
        enum_open: f.get("open").and_then(Value::as_bool).unwrap_or(false),
        place,
        sort_order: int(doc, "order"),
        field_rows: field_rows(&rows_of(doc, "fields"), None),
        values: values(&rows_of(doc, "values")),
        body: text(&f, "body").unwrap_or_default(),
        rows: wire::list_or_empty(Some(&Value::Array(format_rows)), wire::format_row),
        diagram: text(&f, "diagram").filter(|d| d != "flowchart"),
        section_id: text(doc, "section"),
        form,
        standalone: made.standalone.get(id).copied().unwrap_or(false)
            && (kind == "value" || kind == "enum"),
        connections: crate::kinds::connections_of(&Value::Object(doc.clone())).to_vec(),
        inside: text(doc, "in").filter(|i| !i.is_empty()),
        kind,
        ..Card::default()
    }
}

fn inner_of(id: &str, doc: &Obj, made: &Composition, project_id: &str) -> Card {
    let f = fields_of(doc);
    // Where it sits is its box on its container's card map.
    let container = text(doc, "in").unwrap_or_default();
    Card {
        place: made.places.get(&(container, id.to_string())).cloned(),
        id: id.to_string(),
        project_id: project_id.to_string(),
        name: text(doc, "name").unwrap_or_default(),
        label: text(&f, "label").unwrap_or_default(),
        description: text(&f, "description").unwrap_or_default(),
        sort_order: int(doc, "order"),
        section_id: text(doc, "section"),
        form: Some(form_of(doc)),
        connections: crate::kinds::connections_of(&Value::Object(doc.clone())).to_vec(),
        inside: text(doc, "in").filter(|i| !i.is_empty()),
        kind: text(doc, "kind").unwrap_or_default(),
        ..Card::default()
    }
}

/// A card only as far as the project's names, order and containers need it (its id, name,
/// section, container, place among the cards and kind): what `of_made_over` makes of a card
/// whose entry it keeps from before.
fn outline_of(id: &str, doc: &Obj, class: Class, kinds: &crate::kinds::Kinds) -> Card {
    Card {
        id: id.to_string(),
        name: text(doc, "name").unwrap_or_default(),
        sort_order: int(doc, "order"),
        section_id: text(doc, "section"),
        inside: text(doc, "in").filter(|i| !i.is_empty()),
        kind: match class {
            Class::Inner => text(doc, "kind").unwrap_or_default(),
            _ => card_kind(text(doc, "kind"), kinds),
        },
        ..Card::default()
    }
}

/// As SP lists them: by order, then name (stable, so the rest as composed).
fn by_place(a: &Card, b: &Card) -> Ordering {
    a.sort_order
        .cmp(&b.sort_order)
        .then_with(|| a.name.cmp(&b.name))
}

fn cards(
    made: &Composition,
    project_id: &str,
    kinds: &crate::kinds::Kinds,
) -> (Vec<Card>, Vec<Ghost>, Vec<Card>) {
    // The card ghosts apart: they are named, and point at a card, and that is all. And the
    // cards put only inside a container (a flow's nodes), apart too (`inner`): only what
    // reads containers reads them.
    let mut ghosts: Vec<Ghost> = Vec::new();
    let mut cards: Vec<Card> = Vec::new();
    let mut inner: Vec<Card> = Vec::new();
    for item in &made.cards {
        match class_of(&item.doc) {
            Class::Ghost => ghosts.push(ghost_of(&item.id, &item.doc, made, project_id)),
            Class::Inner => inner.push(inner_of(&item.id, &item.doc, made, project_id)),
            Class::Card => cards.push(card_of(&item.id, &item.doc, made, project_id, kinds)),
        }
    }
    ghosts.sort_by(|a, b| {
        a.sort_order
            .cmp(&b.sort_order)
            .then_with(|| a.name.cmp(&b.name))
    });
    cards.sort_by(by_place);
    inner.sort_by(by_place);
    (cards, ghosts, inner)
}

/// A card of a kind the screen reads in the definitions' form: its fields and rows as kept.
fn form_of(doc: &Obj) -> Value {
    let mut form = Map::new();
    form.insert("fields".into(), Value::Object(fields_of(doc)));
    if let Some(rows) = doc.get("rows") {
        form.insert("rows".into(), rows.clone());
    }
    Value::Object(form)
}

// MARK: - What a container holds, in the keys of before

/// The cards inside each container, in the cards' order: what a goal holds (`held_before`).
fn goal_items(cards: &[Card]) -> HashMap<String, Vec<String>> {
    let mut out: HashMap<String, Vec<String>> = HashMap::new();
    for card in cards {
        if let Some(container) = &card.inside {
            out.entry(container.clone())
                .or_default()
                .push(card.id.clone());
        }
    }
    out
}

/// A flow's nodes as they were before they were cards (`nodes`), for the typed reading:
/// each node card inside it, its fields, its rows (`steps`) and the ways and places its
/// lines name, where it sits on the flow's card map (`topLeft`). And a goal's cards: the ids
/// inside it in their order (`form.fields.items`) and where each sits on its card map
/// (`itemPlaces`).
fn held_before(
    cards: &mut [Card],
    inner: &[Card],
    made: &Composition,
    named: &HashMap<String, String>,
    kinds: &crate::kinds::Kinds,
    full: &dyn Fn(&str) -> bool,
    goal_items: &HashMap<String, Vec<String>>,
) {
    for card in cards.iter_mut().filter(|c| full(&c.id)) {
        match card.kind.as_str() {
            "flow" => {
                let nodes: Vec<&Card> = inner
                    .iter()
                    .filter(|n| n.inside.as_deref() == Some(card.id.as_str()))
                    .collect();
                let names: HashMap<&str, &str> = nodes
                    .iter()
                    .map(|n| (n.id.as_str(), n.name.as_str()))
                    .collect();
                let raw: Vec<Value> = nodes
                    .iter()
                    .map(|n| {
                        let at = made
                            .places
                            .get(&(card.id.clone(), n.id.clone()))
                            .and_then(|p| Some((p.x?.round() as i64, p.y?.round() as i64)));
                        node_before(n, &names, named, at)
                    })
                    .collect();
                card.nodes = wire::list_or_empty(Some(&Value::Array(raw)), wire::flow_node);
            }
            // A container of the project's own kind: where each card it holds sits on its card
            // map, as a goal's.
            kind if kinds.is_own(kind) && kinds.holds_cards(kind) => {
                let held = goal_items.get(&card.id).cloned().unwrap_or_default();
                card.item_places = held
                    .iter()
                    .filter_map(|id| {
                        let place = made.places.get(&(card.id.clone(), id.clone()))?;
                        Some((
                            id.clone(),
                            (place.x?.round() as i64, place.y?.round() as i64),
                        ))
                    })
                    .collect();
            }
            "goal" => {
                let held = goal_items.get(&card.id).cloned().unwrap_or_default();
                card.item_places = held
                    .iter()
                    .filter_map(|id| {
                        let place = made.places.get(&(card.id.clone(), id.clone()))?;
                        Some((
                            id.clone(),
                            (place.x?.round() as i64, place.y?.round() as i64),
                        ))
                    })
                    .collect();
                if let Some(Value::Object(form)) = card.form.as_mut()
                    && let Some(Value::Object(fields)) = form.get_mut("fields")
                {
                    fields.insert(
                        "items".into(),
                        Value::Array(held.into_iter().map(Value::from).collect()),
                    );
                }
            }
            _ => {}
        }
    }
}

/// `path` put at the end of the list `key` holds in `map`.
fn push_to(map: &mut Obj, key: &str, path: String) {
    match map.entry(key).or_insert_with(|| Value::Array(Vec::new())) {
        Value::Array(list) => list.push(path.into()),
        other => *other = Value::Array(vec![path.into()]),
    }
}

/// Where a line from a node of a flow leaves: its head, or a step row by its place
/// (`steps[0]`).
pub(crate) fn step_index(from: &str) -> Option<usize> {
    from.strip_prefix("steps[")?.strip_suffix(']')?.parse().ok()
}

/// One node card as the flow's `nodes` held it before (`FlowNode`): what its fields say,
/// and what its lines name — the nodes its ways go to by name, every other place by its
/// path in full as it was written.
fn node_before(
    node: &Card,
    names: &HashMap<&str, &str>,
    named: &HashMap<String, String>,
    at: Option<(i64, i64)>,
) -> Value {
    let empty = Map::new();
    let form = node.form.as_ref().and_then(Value::as_object);
    let fields = form
        .and_then(|f| f.get("fields"))
        .and_then(Value::as_object)
        .unwrap_or(&empty);
    let mut out = Map::new();
    out.insert("name".into(), node.name.clone().into());
    out.insert("kind".into(), node.kind.clone().into());
    for key in [
        "label",
        "description",
        "trigger",
        "every",
        "program",
        "carries",
        "returns",
        "via",
        "initiator",
        "value",
        "for",
        "timeout",
        "join",
        "partial",
        "outcome",
    ] {
        if let Some(v) = fields.get(key) {
            out.insert(key.into(), v.clone());
        }
    }
    let mut reply: Option<Obj> = fields.get("return").and_then(Value::as_object).cloned();
    let mut nexts: Vec<Value> = Vec::new();
    let mut branches: Vec<Value> = Vec::new();
    let lines: Vec<&Obj> = node
        .connections
        .iter()
        .filter_map(Value::as_object)
        .collect();
    let line_text = |line: &Obj, key: &str| -> Option<String> {
        line.get("fields")
            .and_then(|f| f.get(key))
            .and_then(Value::as_str)
            .map(str::to_string)
    };
    let way = |line: &Obj| -> Option<String> {
        let to = line.get("to").and_then(Value::as_str)?;
        names.get(to).map(|n| n.to_string())
    };
    for line in lines.iter().filter(|l| {
        l.get("from")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .is_empty()
    }) {
        let kind = line.get("line").and_then(Value::as_str).unwrap_or_default();
        match kind {
            "next" => {
                if let Some(to) = way(line) {
                    nexts.push(to.into());
                }
            }
            "branch" => {
                if let Some(to) = way(line) {
                    let mut b = Map::new();
                    b.insert(
                        "when".into(),
                        line_text(line, "when").unwrap_or_default().into(),
                    );
                    b.insert("next".into(), to.into());
                    branches.push(Value::Object(b));
                }
            }
            "else" | "onError" | "onTimeout" => {
                if let Some(to) = way(line) {
                    out.insert(kind.into(), to.into());
                }
            }
            "runs" => {
                if let Some(path) = landing_path(line, named) {
                    out.insert("target".into(), path.into());
                }
            }
            "event" => {
                if let Some(path) = landing_path(line, named) {
                    let mut e = Map::new();
                    e.insert("flow".into(), path.into());
                    e.insert(
                        "outcome".into(),
                        line_text(line, "outcome").unwrap_or_default().into(),
                    );
                    out.insert("event".into(), Value::Object(e));
                }
            }
            "peer" => {
                if let Some(path) = landing_path(line, named) {
                    let held = if node.kind == "end" {
                        reply.get_or_insert_with(Map::new)
                    } else {
                        &mut out
                    };
                    push_to(held, "peers", path);
                }
            }
            "key" => {
                if let Some(path) = landing_path(line, named) {
                    let sign = line_text(line, "use").as_deref() == Some("sign");
                    if node.kind == "end" {
                        reply
                            .get_or_insert_with(Map::new)
                            .insert("signWith".into(), path.into());
                    } else {
                        let key = if sign { "signWith" } else { "verifyWith" };
                        out.insert(key.into(), path.into());
                    }
                }
            }
            _ => {}
        }
    }
    if !branches.is_empty() {
        out.insert("branches".into(), Value::Array(branches));
    }
    if let Some(reply) = reply {
        let mut ordered = Map::new();
        for key in ["carries", "peers", "program", "signWith"] {
            if let Some(v) = reply.get(key) {
                ordered.insert(key.into(), v.clone());
            }
        }
        out.insert("return".into(), Value::Object(ordered));
    }
    if !nexts.is_empty() {
        out.insert("next".into(), Value::Array(nexts));
    }
    // Its rows, each with what its lines name.
    let steps: Vec<Value> =
        form.and_then(|f| f.get("rows"))
            .and_then(|r| r.get("steps"))
            .and_then(Value::as_array)
            .map(|rows| {
                rows.iter()
                    .enumerate()
                    .map(|(i, row)| {
                        let mut step = row_fields(row);
                        let from = format!("steps[{i}]");
                        let op = step
                            .get("do")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string();
                        let mut paths: Vec<Value> = step
                            .get("paths")
                            .and_then(Value::as_array)
                            .cloned()
                            .unwrap_or_default();
                        for line in lines.iter().filter(|l| {
                            l.get("from").and_then(Value::as_str) == Some(from.as_str())
                        }) {
                            let Some(path) = landing_path(line, named) else {
                                continue;
                            };
                            match line.get("line").and_then(Value::as_str).unwrap_or_default() {
                                "reads" | "writes" => paths.push(path.into()),
                                "peer" => push_to(&mut step, "peers", path),
                                "key" if op == "sign" || op == "verify" => {
                                    step.insert("key".into(), path.into());
                                }
                                "key" => {
                                    let key = if line_text(line, "use").as_deref() == Some("sign") {
                                        "signWith"
                                    } else {
                                        "verifyWith"
                                    };
                                    step.insert(key.into(), path.into());
                                }
                                _ => {}
                            }
                        }
                        if !paths.is_empty() {
                            step.insert("paths".into(), Value::Array(paths));
                        }
                        Value::Object(step)
                    })
                    .collect()
            })
            .unwrap_or_default();
    if !steps.is_empty() {
        out.insert("steps".into(), Value::Array(steps));
    }
    if let Some((x, y)) = at {
        out.insert("topLeft".into(), json!({"x": x, "y": y}));
    }
    Value::Object(out)
}

/// A formatdoc's row as SP keeps it: its fields, with the rows inside under `rows`.
fn format_doc(row: &Value) -> Value {
    let mut out = row_fields(row);
    let inside: Vec<Value> = row_children(row, "rows").iter().map(format_doc).collect();
    if !inside.is_empty() {
        out.insert("rows".into(), Value::Array(inside));
    }
    Value::Object(out)
}

fn card_refs(cards: &[Card]) -> Vec<CardRef> {
    cards
        .iter()
        .map(|c| CardRef {
            id: c.id.clone(),
            name: c.name.clone(),
            section_id: c.section_id.clone(),
            inside: c.inside.clone(),
        })
        .collect()
}

fn ghost_refs(ghosts: &[Ghost]) -> Vec<GhostRef> {
    ghosts
        .iter()
        .map(|g| GhostRef {
            id: g.id.clone(),
            name: g.name.clone(),
            section_id: g.section_id.clone(),
            target: g.target.clone(),
        })
        .collect()
}

/// A card ghost as `project_detail` answers it: what it points at (`target`, the id it holds)
/// and the card its chain leads to (`resolvesTo`; none when it leads nowhere).
fn ghost_json(g: &Ghost, cards: &[&Card], directory: &Directory) -> Value {
    let mut out = Map::new();
    out.insert("id".into(), g.id.clone().into());
    out.insert("projectId".into(), g.project_id.clone().into());
    out.insert("name".into(), g.name.clone().into());
    out.insert("kind".into(), GHOST.into());
    if let Some(s) = &g.section_id {
        out.insert("sectionId".into(), s.clone().into());
    }
    if let Some(t) = &g.target {
        out.insert("target".into(), t.clone().into());
    }
    let reached = directory
        .entry_of(&g.id)
        .and_then(|e| directory.follow(e))
        .map(|i| cards[i].id.clone());
    if let Some(r) = reached {
        out.insert("resolvesTo".into(), r.into());
    }
    out.insert("sortOrder".into(), g.sort_order.into());
    if let Some(x) = g.place.as_ref().and_then(|p| p.x) {
        out.insert("posX".into(), x.into());
    }
    if let Some(y) = g.place.as_ref().and_then(|p| p.y) {
        out.insert("posY".into(), y.into());
    }
    out.insert(
        "pinned".into(),
        g.place.as_ref().map(|p| p.pinned).unwrap_or(false).into(),
    );
    Value::Object(out)
}

/// An enum's values, in the order they are kept, each with its place as its order.
fn values(rows: &[Value]) -> Vec<Value> {
    rows.iter()
        .enumerate()
        .map(|(order, row)| {
            let f = row_fields(row);
            let id = row.get("id").and_then(Value::as_str).unwrap_or_default();
            json!({
                "id": id,
                "value": text(&f, "value").unwrap_or_default(),
                "label": text(&f, "label").unwrap_or_default(),
                "description": text(&f, "description").unwrap_or_default(),
                "sortOrder": order,
            })
        })
        .collect()
}

fn card_json(c: &Card) -> Value {
    let mut out = Map::new();
    let mut put = |k: &str, v: Value| {
        out.insert(k.to_string(), v);
    };
    put("id", c.id.clone().into());
    put("projectId", c.project_id.clone().into());
    put("name", c.name.clone().into());
    put("label", c.label.clone().into());
    put("kind", c.kind.clone().into());
    put("description", c.description.clone().into());
    put("tags", c.tags.clone().into());
    put("payloads", Value::Array(c.payloads.clone()));
    put("metadata", Value::Object(c.metadata.clone()));
    put("rules", Value::Array(c.rules.clone()));
    put("absentFields", Value::Array(c.absent_fields.clone()));
    if let Some(d) = &c.discriminator {
        put("discriminator", d.clone().into());
    }
    put("enumOpen", c.enum_open.into());
    if let Some(x) = c.place.as_ref().and_then(|p| p.x) {
        put("posX", x.into());
    }
    if let Some(y) = c.place.as_ref().and_then(|p| p.y) {
        put("posY", y.into());
    }
    put(
        "pinned",
        c.place.as_ref().map(|p| p.pinned).unwrap_or(false).into(),
    );
    put("sortOrder", c.sort_order.into());
    put("fields", Value::Array(c.fields.clone()));
    put("values", Value::Array(c.values.clone()));
    put("uses", Value::Array(c.uses.clone()));
    put("signedBy", c.signed_by.clone().into());
    put("verifiedBy", c.verified_by.clone().into());
    put("next", Value::Array(c.next.clone()));
    put("notes", c.notes.clone().into());
    put("body", c.body.clone().into());
    put("rows", Value::Array(c.rows.clone()));
    if let Some(t) = &c.trigger {
        put("trigger", t.clone());
    }
    put("steps", Value::Array(c.steps.clone()));
    put("nodes", Value::Array(c.nodes.clone()));
    put("holds", c.holds.clone().into());
    if let Some(d) = &c.diagram {
        put("diagram", d.clone().into());
    }
    if let Some(size) = c.place.as_ref().and_then(|p| p.size) {
        put("size", crate::tools::grid::size_value(size));
    }
    if let Some(v) = c.place.as_ref().and_then(|p| p.view.clone()) {
        put("docView", v.into());
    }
    if let Some(v) = c.place.as_ref().and_then(|p| p.version.clone()) {
        put("version", v.into());
    }
    if let Some(s) = &c.section_id {
        put("sectionId", s.clone().into());
    }
    put("customFields", Value::Object(c.custom_fields.clone()));
    put("customRows", Value::Array(c.custom_rows.clone()));
    if let Some(form) = &c.form {
        put("form", form.clone());
    }
    if !c.item_places.is_empty() {
        let mut at = Map::new();
        for (id, (x, y)) in &c.item_places {
            at.insert(id.clone(), json!({"x": x, "y": y}));
        }
        put("itemPlaces", Value::Object(at));
    }
    if c.standalone {
        put("standalone", true.into());
    }
    put("connections", Value::Array(c.connections.clone()));
    if let Some(container) = &c.inside {
        put("in", container.clone().into());
    }
    Value::Object(out)
}

/// A card put only inside a container (a flow's node), as `project_detail` answers it apart
/// from the cards (`inner`): its name, kind, section and container, its place among the
/// container's cards, its fields and rows as kept (`form`), and its lines.
fn inner_json(c: &Card) -> Value {
    let mut out = Map::new();
    out.insert("id".into(), c.id.clone().into());
    out.insert("projectId".into(), c.project_id.clone().into());
    out.insert("name".into(), c.name.clone().into());
    out.insert("kind".into(), c.kind.clone().into());
    if let Some(s) = &c.section_id {
        out.insert("sectionId".into(), s.clone().into());
    }
    if let Some(container) = &c.inside {
        out.insert("in".into(), container.clone().into());
    }
    out.insert("sortOrder".into(), c.sort_order.into());
    if let Some(x) = c.place.as_ref().and_then(|p| p.x) {
        out.insert("posX".into(), x.into());
    }
    if let Some(y) = c.place.as_ref().and_then(|p| p.y) {
        out.insert("posY".into(), y.into());
    }
    out.insert("label".into(), c.label.clone().into());
    out.insert("description".into(), c.description.clone().into());
    if let Some(form) = &c.form {
        out.insert("form".into(), form.clone());
    }
    out.insert("connections".into(), Value::Array(c.connections.clone()));
    Value::Object(out)
}

// MARK: - Fields

/// A field as its document keeps it, and the fields inside it.
struct FieldRow {
    id: String,
    parent: Option<String>,
    /// Its names from the card down, `.` between: where a line leaves it (`from`).
    path: String,
    name: String,
    label: String,
    kind: String,
    ref_schema_id: Option<String>,
    points_at: Option<Value>,
    is_array: bool,
    is_required: bool,
    is_nullable: bool,
    const_value: Option<Value>,
    is_unique: bool,
    is_pk: bool,
    description: String,
    rules: Vec<Value>,
    variants: Vec<String>,
    also: Vec<String>,
    sort_order: i64,
}

/// A card's fields, flat, each with the field it sits in and its place as its order. What
/// a field embeds and what its value names are its card's lines (`legacy_lines`).
fn field_rows(list: &[Value], parent: Option<(&str, &str)>) -> Vec<FieldRow> {
    let mut out = Vec::new();
    for (order, row) in list.iter().enumerate() {
        let f = row_fields(row);
        let id = row
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let name = text(&f, "name").unwrap_or_default();
        let path = match parent {
            Some((_, above)) => crate::pathtext::member(above, &name),
            None => crate::pathtext::quote(&name).into_owned(),
        };
        let flag = |key: &str| f.get(key).and_then(Value::as_bool).unwrap_or(false);
        out.push(FieldRow {
            id: id.clone(),
            parent: parent.map(|(p, _)| p.to_string()),
            path: path.clone(),
            name,
            label: text(&f, "label").unwrap_or_default(),
            kind: text(&f, "type").unwrap_or_else(|| "string".to_string()),
            ref_schema_id: None,
            points_at: None,
            is_array: flag("array"),
            is_required: flag("required"),
            is_nullable: flag("nullable"),
            const_value: f.get("const").cloned(),
            is_unique: flag("unique"),
            is_pk: flag("primaryKey"),
            description: text(&f, "description").unwrap_or_default(),
            rules: wire::rules(f.get("rules")),
            variants: wire::strings_or_empty(f.get("variants")),
            also: wire::strings_or_empty(f.get("also")),
            sort_order: order as i64,
        });
        out.extend(field_rows(&row_children(row, "fields"), Some((&id, &path))));
    }
    out
}

// MARK: - The keys of before, from the lines

/// Where a line lands, in full as it was written (a card ghost by its own path): its card,
/// and the names there; a landing written in full elsewhere as it is. None when its card
/// is gone.
fn landing_path(line: &Obj, named: &HashMap<String, String>) -> Option<String> {
    let to = line.get("to").and_then(Value::as_str)?;
    let at = line.get("at").and_then(Value::as_str).unwrap_or_default();
    if crate::pathtext::has_section(at) {
        return Some(at.to_string());
    }
    let head = named.get(to)?;
    Some(if at.is_empty() {
        head.clone()
    } else {
        format!("{head}.{at}")
    })
}

/// A card's lines read into the keys of before, for the typed reading: what a
/// field embeds (`ref`) and what its value names as `x-pointsAt` says (`pointsAt`); what a
/// store holds, a flow's rules, what a task is joined to (`holds`, `notes`, `links`); a
/// signature's keys as `x-payloads` says (`payloads`). The exports read them so too.
fn legacy_lines(card: &mut Card, defs: &crate::kinds::Defs, named: &HashMap<String, String>) {
    let mut payloads: Vec<(String, Map<String, Value>)> = Vec::new();
    let mut links: Vec<String> = Vec::new();
    for line in card.connections.iter().filter_map(Value::as_object) {
        let Some(name) = line.get("line").and_then(Value::as_str) else {
            continue;
        };
        let Some(def) = defs.line(name) else {
            continue;
        };
        let from = line.get("from").and_then(Value::as_str).unwrap_or_default();
        let fields = line
            .get("fields")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        let to = line.get("to").and_then(Value::as_str).map(str::to_string);
        if def.base == crate::kinds::Base::Embed && !from.is_empty() {
            if let Some(row) = card.field_rows.iter_mut().find(|r| r.path == from) {
                row.kind = "ref".to_string();
                row.ref_schema_id = to;
            }
            continue;
        }
        if def.annotation() == Some("x-pointsAt") && !from.is_empty() {
            let Some(row) = card.field_rows.iter_mut().find(|r| r.path == from) else {
                continue;
            };
            let mut stored = Map::new();
            if let Some(to) = &to {
                stored.insert("schemaId".into(), to.clone().into());
            }
            stored.insert("as".into(), def.word().into());
            if line.contains_key("at")
                && let Some(landed) = landing_path(line, named)
            {
                stored.insert("in".into(), landed.into());
            }
            for key in [
                "when",
                "discriminatedBy",
                "discriminatorField",
                "cardinality",
            ] {
                if let Some(v) = fields.get(key) {
                    stored.insert(key.into(), v.clone());
                }
            }
            let mut list = match row.points_at.take() {
                Some(Value::Array(list)) => list,
                _ => Vec::new(),
            };
            list.push(Value::Object(stored));
            row.points_at = Some(Value::Array(list));
            continue;
        }
        let Some(path) = landing_path(line, named) else {
            continue;
        };
        match name {
            "holds" if card.kind == "store" => card.holds.push(path),
            "notes" if card.kind == "flow" => card.notes.push(path),
            "link" if card.kind == "task" => links.push(path),
            _ if def.annotation() == Some("x-payloads") => {
                let payload = fields
                    .get("payload")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                let at = match payloads.iter().position(|(n, _)| *n == payload) {
                    Some(at) => at,
                    None => {
                        let mut entry = Map::new();
                        entry.insert("name".into(), payload.clone().into());
                        entry.insert(
                            "excludes".into(),
                            fields
                                .get("excludes")
                                .cloned()
                                .unwrap_or_else(|| Value::Array(Vec::new())),
                        );
                        if let Some(v) = fields.get("valueAt") {
                            entry.insert("valueAt".into(), v.clone());
                        }
                        payloads.push((payload, entry));
                        payloads.len() - 1
                    }
                };
                let entry = &mut payloads[at].1;
                let keys = entry
                    .entry(name.to_string())
                    .or_insert_with(|| Value::Array(Vec::new()));
                if let Value::Array(keys) = keys {
                    keys.push(path.into());
                }
            }
            _ => {}
        }
    }
    // A payload's keys as it listed them: those that make a signature, then those that
    // check one.
    card.payloads = payloads
        .into_iter()
        .map(|(_, mut entry)| {
            let mut out = Map::new();
            for key in ["name", "excludes", "valueAt", "signedBy", "verifiedBy"] {
                if let Some(v) = entry.shift_remove(key) {
                    out.insert(key.into(), v);
                }
            }
            out.extend(entry);
            Value::Object(out)
        })
        .collect();
    if !links.is_empty()
        && let Some(Value::Object(form)) = card.form.as_mut()
        && let Some(Value::Object(fields)) = form.get_mut("fields")
    {
        fields.insert(
            "links".into(),
            Value::Array(links.into_iter().map(Value::from).collect()),
        );
    }
}

/// The field tree (`Repository.buildTree`): each level by order, then name; a field whose
/// parent is not there at the top.
fn build_fields(
    rows: &[FieldRow],
    known: &dyn Fn(&str) -> bool,
    named: &HashMap<String, String>,
) -> Vec<Value> {
    let present: HashSet<&str> = rows.iter().map(|r| r.id.as_str()).collect();
    let mut children: HashMap<&str, Vec<&FieldRow>> = HashMap::new();
    for row in rows {
        let up = row
            .parent
            .as_deref()
            .filter(|p| present.contains(p))
            .unwrap_or("");
        children.entry(up).or_default().push(row);
    }
    fn level(
        up: &str,
        children: &HashMap<&str, Vec<&FieldRow>>,
        known: &dyn Fn(&str) -> bool,
        named: &HashMap<String, String>,
    ) -> Vec<Value> {
        let mut list: Vec<&FieldRow> = children.get(up).cloned().unwrap_or_default();
        list.sort_by(|a, b| {
            a.sort_order
                .cmp(&b.sort_order)
                .then_with(|| a.name.cmp(&b.name))
        });
        list.into_iter()
            .map(|row| field_json(row, level(&row.id, children, known, named), known, named))
            .collect()
    }
    level("", &children, known, named)
}

fn field_json(
    row: &FieldRow,
    children: Vec<Value>,
    known: &dyn Fn(&str) -> bool,
    named: &HashMap<String, String>,
) -> Value {
    // A pointer names its target in full; one whose target is gone, nothing.
    let points_at: Vec<Value> = row
        .points_at
        .as_ref()
        .and_then(|v| wire::list(v, wire::stored_points_at).ok())
        .unwrap_or_default()
        .iter()
        .map(|p| {
            wire::points_at(
                p,
                p.schema_id.as_ref().and_then(|id| named.get(id).cloned()),
            )
        })
        .collect();
    let mut out = Map::new();
    out.insert("id".into(), row.id.clone().into());
    if let Some(p) = &row.parent {
        out.insert("parentFieldId".into(), p.clone().into());
    }
    out.insert("name".into(), row.name.clone().into());
    out.insert("label".into(), row.label.clone().into());
    out.insert("type".into(), row.kind.clone().into());
    // A target that is not in this view reads as unresolved.
    if let Some(target) = row.ref_schema_id.as_ref().filter(|t| known(t)) {
        out.insert("refSchemaId".into(), target.clone().into());
    }
    out.insert("pointsAt".into(), Value::Array(points_at));
    out.insert("isArray".into(), row.is_array.into());
    out.insert("isRequired".into(), row.is_required.into());
    out.insert("isNullable".into(), row.is_nullable.into());
    if let Some(c) = &row.const_value {
        out.insert("constValue".into(), c.clone());
    }
    out.insert("isUnique".into(), row.is_unique.into());
    out.insert("isPk".into(), row.is_pk.into());
    out.insert("description".into(), row.description.clone().into());
    out.insert("rules".into(), Value::Array(row.rules.clone()));
    out.insert("variants".into(), row.variants.clone().into());
    out.insert("also".into(), row.also.clone().into());
    out.insert("sortOrder".into(), row.sort_order.into());
    out.insert("children".into(), Value::Array(children));
    Value::Object(out)
}

// MARK: - Relations

/// A card a line reaches, as `relations_of` reads it: its id, and its fields as its entry says
/// them (what a discriminator is read in).
#[derive(Clone, Copy)]
struct Target<'a> {
    id: &'a str,
    fields: &'a [Value],
}

/// Every line of every card (`deriveRelations`, for every kind of line now): to the card it
/// reaches — through a card ghost, the card the ghost leads to, saying the ghost
/// (`viaGhostId`) — or, for a pointer told apart by its discriminator, to each of the
/// cards of its card's section that pin a value on that field. Each says its kind of line
/// (`line`), the row it leaves (`from`, none for the head) and where it lands (`at`).
///
/// A field's lines come in the order of the fields, then the card's other lines. Those
/// the screen drew before the definitions — what a field embeds and what it names as
/// `x-pointsAt` says — keep the form they had (`refKind` `embed`, `lookup` for `id`, or the
/// word) and say so (`legacy`: the exports' mermaid draws them so); every other is of its
/// kind of line by name. The screen's are `screen_detail`'s.
/// One card's lines (`deriveRelations`, for every kind of line now), as the project's are said
/// (the cards' in their order).
fn relations_of<'a>(
    card: &Card,
    all: &dyn Fn(usize) -> Target<'a>,
    directory: &Directory,
    defs: &crate::kinds::Defs,
    named: &HashMap<String, String>,
) -> Vec<Value> {
    let mut out = Vec::new();
    // A ghost's id as the card it leads to.
    let through = |entry: Option<Entry>| -> Option<(Target<'a>, Option<String>)> {
        let entry = entry?;
        let card = all(directory.follow(entry)?);
        let via = match entry {
            Entry::Ghost(g) => directory.ghost_id(g),
            Entry::Card(_) => None,
        };
        Some((card, via))
    };
    let base = card
        .section_id
        .clone()
        .unwrap_or_else(|| directory.home.clone());
    let lines: Vec<&Obj> = card
        .connections
        .iter()
        .filter_map(Value::as_object)
        .collect();
    let from_of = |line: &Obj| {
        line.get("from")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    // The fields in their order, then what leaves elsewhere.
    let mut ordered: Vec<(&Obj, Option<&FieldRow>)> = Vec::new();
    let mut placed = vec![false; lines.len()];
    walk_rows(&card.fields, &card.field_rows, &mut |row| {
        for (i, line) in lines.iter().enumerate() {
            if !placed[i] && from_of(line) == row.path {
                placed[i] = true;
                ordered.push((line, Some(row)));
            }
        }
    });
    for (i, line) in lines.iter().enumerate() {
        if !placed[i] {
            ordered.push((line, None));
        }
    }
    for (line, field) in ordered {
        let Some(name) = line.get("line").and_then(Value::as_str) else {
            continue;
        };
        let def = defs.line(name);
        let from = from_of(line);
        let at = line.get("at").and_then(Value::as_str);
        let fields = line.get("fields").and_then(Value::as_object);
        let field_id = field.map(|f| f.id.clone()).unwrap_or_default();
        let is_array = field.is_some_and(|f| f.is_array);
        let embed = def.is_some_and(|d| d.base == crate::kinds::Base::Embed);
        let pointer = def.is_some_and(|d| d.annotation() == Some("x-pointsAt"));
        let legacy = field.is_some() && (embed || pointer);
        let tag = fields
            .and_then(|f| {
                f.get("discriminatorField")
                    .or_else(|| f.get("discriminatedBy"))
            })
            .and_then(Value::as_str);
        let targets: Vec<(Target<'a>, Option<String>)> =
            match line.get("to").and_then(Value::as_str) {
                Some(to) => through(directory.entry_of(to)).into_iter().collect(),
                None => match tag {
                    Some(tag) => directory
                        .members(&base)
                        .iter()
                        .map(|&i| all(i))
                        .filter(|c| {
                            c.fields
                                .iter()
                                .find(|f| f["name"] == tag)
                                .and_then(|f| f.get("constValue"))
                                .is_some_and(|v| !v.is_null())
                        })
                        .map(|c| (c, None))
                        .collect(),
                    None => Vec::new(),
                },
            };
        let ref_kind = if embed && field.is_some() {
            "embed".to_string()
        } else if pointer && field.is_some() {
            match def.map(|d| d.word()).unwrap_or(name) {
                "id" => "lookup".to_string(),
                word => word.to_string(),
            }
        } else {
            name.to_string()
        };
        let landed = line
            .contains_key("at")
            .then(|| landing_path(line, named))
            .flatten();
        for (target, via) in targets {
            let mut r = Map::new();
            r.insert("fromSchemaId".into(), card.id.clone().into());
            r.insert("toSchemaId".into(), target.id.into());
            r.insert("fieldId".into(), field_id.clone().into());
            r.insert("path".into(), from.clone().into());
            if !embed && let Some(c) = fields.and_then(|f| f.get("cardinality")) {
                r.insert("cardinality".into(), c.clone());
            }
            r.insert("refKind".into(), ref_kind.clone().into());
            if legacy {
                if let Some(landed) = &landed {
                    r.insert("refTarget".into(), landed.clone().into());
                }
            } else if let Some(at) = at {
                r.insert("refTarget".into(), at.into());
            }
            r.insert("isArray".into(), is_array.into());
            if let Some(via) = via {
                r.insert("viaGhostId".into(), via.into());
            }
            r.insert("line".into(), name.into());
            r.insert("from".into(), from.clone().into());
            if let Some(at) = at {
                r.insert("at".into(), at.into());
            }
            if legacy {
                r.insert("legacy".into(), true.into());
            }
            out.push(Value::Object(r));
        }
    }
    out
}

/// A card's fields depth first, each with its row (as the fields were built).
fn walk_rows<'r>(fields: &[Value], rows: &'r [FieldRow], visit: &mut dyn FnMut(&'r FieldRow)) {
    for field in fields {
        let id = field["id"].as_str().unwrap_or_default();
        if let Some(row) = rows.iter().find(|r| r.id == id) {
            visit(row);
        }
        if let Some(children) = field["children"].as_array() {
            walk_rows(children, rows, visit);
        }
    }
}
