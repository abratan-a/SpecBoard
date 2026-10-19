//! A write's reading of its tree brought up to what the write wrote since, instead of read whole
//! again: a card's
//! label, description or body into its own entry alone (`detail::bring_card`); any other card
//! made, written or let go of, by working out again the entries of the cards it touches — itself,
//! the cards whose documents name it or what is inside it, its container, and so on — and keeping
//! every other card's entry (`detail::of_made_over`). What cannot be brought up so (a card ghost
//! written, a card made or let go of in a branch, …) has the tree read whole again.

use super::Seen;
use crate::write::Written;
use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};

type Obj = Map<String, Value>;

/// What a write's reading was made of, kept with it to bring it up (`Seen::in_tx`).
pub(crate) struct Kept {
    /// What the tree saw of the project, but its cards' documents (`Seen::docs`).
    pub made: crate::store::Composition,
    pub row: crate::store::ProjectRow,
    /// Every card's and card ghost's path in full, by id (`detail::of_made_named`).
    pub paths: HashMap<String, String>,
    /// Which cards name which (`Refs`), made the first time a card is brought up so.
    pub refs: Option<Refs>,
    /// The cards by id in the order the store composes them (`Seen::composed_order`); none once a
    /// card is made, until it is read whole again.
    pub order: Option<Vec<String>>,
}

/// Which cards' documents name which cards — what a card's entry says of others is read from
/// them —, kept as the documents are written.
#[derive(Default, Debug, PartialEq, Eq)]
pub(crate) struct Refs {
    /// By id: the cards whose documents name it — a line's `to`, `in`, a card ghost's `target`.
    named_by: HashMap<String, HashSet<String>>,
    /// The cards with a line that reaches no card by name: told apart by a discriminator, it
    /// reaches each card of its section that pins a value (`detail::relations_of`).
    scanners: HashSet<String>,
    /// The ids card ghosts point at, and how many point at each.
    ghost_targets: HashMap<String, usize>,
}

/// What a card's document names of other cards, by id.
fn names_in(doc: &Obj) -> Vec<String> {
    let mut out: Vec<String> = doc
        .get("connections")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|line| line.get("to").and_then(Value::as_str))
        .map(str::to_string)
        .collect();
    if let Some(container) = doc.get("in").and_then(Value::as_str) {
        out.push(container.to_string());
    }
    if let Some(target) = ghost_target(doc) {
        out.push(target.to_string());
    }
    out
}

fn ghost_target(doc: &Obj) -> Option<&str> {
    (doc.get("kind").and_then(Value::as_str) == Some(crate::detail::GHOST))
        .then(|| doc.get("fields")?.get("target")?.as_str())
        .flatten()
}

/// Whether a card has a line that reaches no card by name.
fn scans(doc: &Obj) -> bool {
    doc.get("connections")
        .and_then(Value::as_array)
        .is_some_and(|lines| {
            lines
                .iter()
                .any(|line| line.get("to").and_then(Value::as_str).is_none())
        })
}

impl Refs {
    pub(crate) fn of(docs: &HashMap<String, Obj>) -> Refs {
        let mut refs = Refs::default();
        for (id, doc) in docs {
            refs.add(id, doc);
        }
        refs
    }

    fn add(&mut self, id: &str, doc: &Obj) {
        for named in names_in(doc) {
            self.named_by
                .entry(named)
                .or_default()
                .insert(id.to_string());
        }
        if scans(doc) {
            self.scanners.insert(id.to_string());
        }
        if let Some(target) = ghost_target(doc) {
            *self.ghost_targets.entry(target.to_string()).or_default() += 1;
        }
    }

    fn remove(&mut self, id: &str, doc: &Obj) {
        for named in names_in(doc) {
            if let Some(by) = self.named_by.get_mut(&named) {
                by.remove(id);
                if by.is_empty() {
                    self.named_by.remove(&named);
                }
            }
        }
        self.scanners.remove(id);
        if let Some(target) = ghost_target(doc)
            && let Some(n) = self.ghost_targets.get_mut(target)
        {
            *n -= 1;
            if *n == 0 {
                self.ghost_targets.remove(target);
            }
        }
    }

    fn named_by(&self, id: &str) -> impl Iterator<Item = &String> {
        self.named_by.get(id).into_iter().flatten()
    }
}

/// Which cards' documents say which words — a path that lands on something says its name, a
/// word —, kept as the documents are written: what a change that names a card, a field or a
/// section otherwise may change, found by its name (`Seen::saying`) instead of reading every card.
#[derive(Default, Debug, PartialEq, Eq)]
pub(crate) struct Words {
    by_word: HashMap<String, HashSet<String>>,
}

/// Whether a name is one word as `Words` keeps them: ASCII letters, digits and `_` (a card's and a
/// field's always are).
pub(crate) fn is_word(name: &str) -> bool {
    !name.is_empty() && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

/// A document's words: the runs of ASCII letters, digits and `_` of its strings and keys, in lower
/// case.
fn words_in(doc: &Obj) -> HashSet<String> {
    fn each(s: &str, out: &mut HashSet<String>) {
        for w in s.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_')) {
            if !w.is_empty() {
                out.insert(w.to_ascii_lowercase());
            }
        }
    }
    fn value(v: &Value, out: &mut HashSet<String>) {
        match v {
            Value::String(s) => each(s, out),
            Value::Array(items) => items.iter().for_each(|x| value(x, out)),
            Value::Object(map) => {
                for (k, x) in map {
                    each(k, out);
                    value(x, out);
                }
            }
            _ => {}
        }
    }
    let mut out = HashSet::new();
    for (k, v) in doc {
        each(k, &mut out);
        value(v, &mut out);
    }
    out
}

impl Words {
    pub(crate) fn of(docs: &HashMap<String, Obj>) -> Words {
        let mut words = Words::default();
        for (id, doc) in docs {
            words.add(id, doc);
        }
        words
    }

    fn add(&mut self, id: &str, doc: &Obj) {
        for w in words_in(doc) {
            self.by_word.entry(w).or_default().insert(id.to_string());
        }
    }

    fn remove(&mut self, id: &str, doc: &Obj) {
        for w in words_in(doc) {
            if let Some(by) = self.by_word.get_mut(&w) {
                by.remove(id);
                if by.is_empty() {
                    self.by_word.remove(&w);
                }
            }
        }
    }

    /// The cards whose documents say any of `names` (in lower case); none when one is not a word.
    pub(crate) fn saying(&self, names: &[String]) -> Option<HashSet<String>> {
        let mut out = HashSet::new();
        for name in names {
            if !is_word(name) {
                return None;
            }
            out.extend(self.by_word.get(name).into_iter().flatten().cloned());
        }
        Some(out)
    }
}

fn text<'d>(doc: &'d Obj, key: &str) -> Option<&'d str> {
    doc.get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
}

/// What tells a card apart among the project's: its name, section, container, place among the
/// cards and kind. A card whose these are as they were is where it was, named as it was.
fn key_of(doc: &Obj) -> [Option<&Value>; 5] {
    ["name", "section", "in", "order", "kind"].map(|k| doc.get(k))
}

fn fields_rows(doc: &Obj) -> Option<&Value> {
    doc.get("rows").and_then(|r| r.get("fields"))
}

impl Seen {
    /// Brought up to what was written since it was made (`Tx::remembered_or_brought_up`), in
    /// order: what is said in a card's own entry alone, there alone; from the first that is not,
    /// the rest together (`brought_up_whole`). False when it cannot be: it is made again.
    pub(super) fn brought_up(&mut self, written: &[Written], tx: &crate::write::Tx) -> bool {
        let mut at = 0;
        while at < written.len() {
            match &written[at] {
                // Another project's is none of this one's.
                Written::Marked { project, .. }
                | Written::Card { project, .. }
                | Written::Removed { project, .. }
                    if *project != self.summary.id => {}
                Written::Marked { at: when, .. } => {
                    if let Some(project) = self.raw["project"].as_object_mut() {
                        project.insert("updatedAt".into(), when.clone().into());
                    }
                    if let Some(kept) = self.kept.as_mut() {
                        kept.row.updated_at = when.clone();
                    }
                }
                Written::Card {
                    tree,
                    id,
                    before: Some(before),
                    after,
                    ..
                } if *tree == self.named.tree
                    && crate::detail::bring_card(
                        &mut self.raw,
                        &mut self.detail,
                        &mut self.docs,
                        id,
                        before,
                        after,
                    ) =>
                {
                    self.written_as.get_mut().remove(id);
                    self.broken_by_card.get_mut().remove(id);
                    if let Some(words) = self.words.get_mut() {
                        words.remove(id, before);
                        words.add(id, after);
                    }
                    // In a branch, one of main's written goes among the branch's own cards.
                    if !tree.is_empty()
                        && tree != crate::store::BOARD
                        && let Some(kept) = self.kept.as_mut()
                    {
                        kept.order = None;
                    }
                }
                _ => break,
            }
            at += 1;
        }
        self.broken.take();
        at == written.len() || self.brought_up_whole(&written[at..], tx)
    }

    /// Brought up to `written` together: the documents written put in place, then the entries of
    /// the cards they touch worked out again (`detail::of_made_over`).
    fn brought_up_whole(&mut self, written: &[Written], tx: &crate::write::Tx) -> bool {
        super::count_brought_whole();
        // The names as they are (`detail::bring_over`), when none of them changes.
        let named_alike = written.iter().all(|w| match w {
            Written::Card {
                before: Some(b),
                after,
                ..
            } => key_of(b) == key_of(after),
            Written::Card { before: None, .. } | Written::Removed { .. } => false,
            Written::Marked { .. } => true,
        });
        let directory = named_alike.then(|| self.project().directory.clone());
        let tree = self.named.tree.clone();
        let own = tree.is_empty() || tree == crate::store::BOARD;
        let project = self.summary.id.clone();
        let Some(kept) = self.kept.as_mut() else {
            return false;
        };
        let refs = kept.refs.get_or_insert_with(|| Refs::of(&self.docs));
        // The documents written, in place; each written card's document before, or none.
        let mut olds: Vec<(String, Option<Obj>)> = Vec::new();
        let mut ids: HashSet<String> = HashSet::new();
        let mut moved = false;
        let mut fields = false;
        for w in written {
            match w {
                Written::Marked { project: p, at } if *p == project => {
                    kept.row.updated_at = at.clone();
                }
                Written::Marked { .. } => {}
                Written::Card { project: p, .. } | Written::Removed { project: p, .. }
                    if *p != project => {}
                Written::Card {
                    tree: t,
                    id,
                    before,
                    after,
                    ..
                } => {
                    if *t != tree || self.docs.get(id) != before.as_ref() {
                        return false;
                    }
                    let ghost = |d: &Obj| {
                        d.get("kind").and_then(Value::as_str) == Some(crate::detail::GHOST)
                    };
                    if ghost(after) || before.as_ref().is_some_and(ghost) {
                        return false;
                    }
                    moved |= before.as_ref().is_none_or(|b| key_of(b) != key_of(after));
                    fields |= before.as_ref().is_none_or(|b| {
                        fields_rows(b) != fields_rows(after) || b.get("kind") != after.get("kind")
                    });
                    if let Some(before) = before {
                        refs.remove(id, before);
                    }
                    refs.add(id, after);
                    // A card made goes where the store puts it; in a branch, one of main's written
                    // goes among the branch's own: read again when asked for.
                    if before.is_none() || !own {
                        kept.order = None;
                    }
                    olds.push((id.clone(), before.clone()));
                    self.docs.insert(id.clone(), after.clone());
                    ids.insert(id.clone());
                }
                Written::Removed {
                    tree: t,
                    id,
                    before,
                    ..
                } => {
                    if *t != tree
                        || self.docs.get(id) != Some(before)
                        || before.get("kind").and_then(Value::as_str) == Some(crate::detail::GHOST)
                    {
                        return false;
                    }
                    moved = true;
                    fields = true;
                    refs.remove(id, before);
                    olds.push((id.clone(), Some(before.clone())));
                    self.docs.remove(id);
                    ids.insert(id.clone());
                    if let Some(order) = kept.order.as_mut() {
                        order.retain(|o| o != id);
                    }
                    // Its boxes and how it looked went with it (`Tx::remove_card`).
                    kept.made
                        .places
                        .retain(|(view, item), _| view != id && item != id);
                    kept.made.node_places.remove(id);
                    kept.made.standalone.remove(id);
                }
            }
        }
        // A card made or let go of in a branch is told apart from main's beneath it: read whole.
        if moved && !own {
            return false;
        }
        // What the tree sees: a card whose section or container is not there is not (the store's
        // `compose`), and a card let go of leaves none inside it; a card ghost pointing at one let go
        // of leads elsewhere.
        let sections: HashSet<&str> = kept.made.sections.iter().map(|s| s.id.as_str()).collect();
        for id in &ids {
            match self.docs.get(id) {
                Some(doc) => {
                    let holder = crate::write::holder(doc);
                    if !sections.contains(holder) && !self.docs.contains_key(holder) {
                        return false;
                    }
                }
                None => {
                    if refs.ghost_targets.contains_key(id)
                        || refs
                            .named_by(id)
                            .any(|by| self.docs.get(by).and_then(|d| text(d, "in")) == Some(id))
                    {
                        return false;
                    }
                }
            }
        }
        // The sections whose cards changed: what a card there that reaches its section's cards
        // by a discriminator reaches may have.
        let touched: HashSet<String> = olds
            .iter()
            .filter_map(|(_, b)| b.as_ref())
            .chain(ids.iter().filter_map(|id| self.docs.get(id)))
            .filter_map(|d| text(d, "section").map(str::to_string))
            .collect();
        // The words the written cards' documents say, as they say them now.
        if let Some(words) = self.words.get_mut() {
            for (id, before) in &olds {
                if let Some(before) = before {
                    words.remove(id, before);
                }
                if let Some(now) = self.docs.get(id) {
                    words.add(id, now);
                }
            }
        }
        // A card made, let go of, named or put elsewhere, or its fields changed: what a document
        // that says its name was written as may read otherwise now.
        let names: Vec<String> = if moved || fields {
            olds.iter()
                .filter_map(|(_, b)| b.as_ref())
                .chain(ids.iter().filter_map(|id| self.docs.get(id)))
                .filter_map(|d| text(d, "name"))
                .map(crate::pathtext::fold)
                .collect()
        } else {
            Vec::new()
        };
        {
            let written_as = self.written_as.get_mut();
            for id in &ids {
                written_as.remove(id);
            }
            if !names.is_empty() {
                match self.words.get_mut().as_ref().and_then(|w| w.saying(&names)) {
                    Some(saying) => written_as.retain(|id, _| !saying.contains(id)),
                    None => written_as.clear(),
                }
            }
        }
        let docs = &self.docs;
        let refs = &*refs;
        // The cards whose entries are worked out again: the written ones; when names changed
        // (`repathed`, the cards whose paths are not as they were), the cards whose documents name
        // one named otherwise now or not there now; …
        let dirty_of = |repathed: &HashSet<String>| -> HashSet<String> {
            let mut changed: HashSet<&String> = HashSet::new();
            if moved {
                changed.extend(ids.iter());
                changed.extend(repathed.iter());
            }
            let mut dirty: HashSet<String> = ids
                .iter()
                .filter(|id| docs.contains_key(*id))
                .cloned()
                .collect();
            for id in changed {
                dirty.extend(
                    refs.named_by(id)
                        .filter(|by| docs.contains_key(*by))
                        .cloned(),
                );
            }
            dirty.extend(
                refs.scanners
                    .iter()
                    .filter(|s| {
                        docs.get(*s)
                            .and_then(|d| text(d, "section"))
                            .is_some_and(|s| touched.contains(s))
                    })
                    .cloned(),
            );
            // Their containers (where they were too), a flow's nodes, and the cards a dirty one
            // reaches by a discriminator.
            dirty.extend(
                olds.iter()
                    .filter_map(|(_, b)| b.as_ref().and_then(|b| text(b, "in")))
                    .filter(|c| docs.contains_key(*c))
                    .map(str::to_string),
            );
            loop {
                let before = dirty.len();
                let mut more: Vec<String> = Vec::new();
                for id in &dirty {
                    let doc = &docs[id];
                    if let Some(container) = text(doc, "in").filter(|c| docs.contains_key(*c)) {
                        more.push(container.to_string());
                    }
                    if text(doc, "kind") == Some("flow") {
                        more.extend(
                            refs.named_by(id)
                                .filter(|n| docs.get(*n).and_then(|d| text(d, "in")) == Some(id))
                                .cloned(),
                        );
                    }
                    if refs.scanners.contains(id) {
                        let section = text(doc, "section");
                        more.extend(
                            docs.iter()
                                .filter(|(_, d)| text(d, "section") == section)
                                .map(|(other, _)| other.clone()),
                        );
                    }
                }
                dirty.extend(more);
                if dirty.len() == before {
                    break;
                }
            }
            dirty
        };
        // The cards whose paths changed (their own and what they hold).
        let mut repathed: Vec<String> = Vec::new();
        if moved {
            // Named, placed or there otherwise: the names and the order made again of every card in
            // outline, the touched cards' entries worked out again.
            let raw = std::mem::take(&mut self.raw);
            let paths = std::mem::take(&mut kept.paths);
            // What a card holds: the cards that name it whose documents say they are in it.
            let held = |id: &str| -> Vec<String> {
                refs.named_by(id)
                    .filter(|by| docs.get(*by).and_then(|d| text(d, "in")) == Some(id))
                    .cloned()
                    .collect()
            };
            let Some(crate::detail::MadeOver {
                raw,
                paths: now,
                dirty,
                repathed: moved_paths,
            }) = crate::detail::of_made_over(
                &kept.row,
                &kept.made,
                docs,
                raw,
                &ids,
                paths,
                &held,
                &|repathed| Some(dirty_of(repathed)),
            )
            else {
                return false;
            };
            let Ok(detail) = crate::model::typed_over(&raw, &mut self.detail, &dirty, &ids) else {
                return false;
            };
            repathed.extend(moved_paths);
            self.raw = raw;
            self.detail = detail;
            kept.paths = now;
            self.looked = std::cell::OnceCell::new();
        } else {
            // Each where it was, named as it was: the touched cards' entries put in place.
            let Some(directory) = &directory else {
                return false;
            };
            let dirty = dirty_of(&HashSet::new());
            if crate::detail::bring_over(
                &kept.row,
                &kept.made,
                docs,
                &mut self.raw,
                &kept.paths,
                directory,
                &dirty,
            )
            .is_none()
                || crate::model::typed_in_place(&self.raw, &mut self.detail, &dirty).is_err()
            {
                return false;
            }
        }
        let Ok(gone) = super::gone_of(&tx.reader(), &project, &self.detail) else {
            return false;
        };
        self.gone = gone;
        // What each card says does not resolve, let go of where it may say otherwise now: the cards
        // written or named otherwise, those whose documents name one by id, and those whose
        // documents say the name of one (as it was or is); and when a card was made, let go of or
        // named otherwise, every card that says something does not resolve — what is said of it
        // tells the cards there are (how many a section has, the names near it).
        let by_card = self.broken_by_card.get_mut();
        if !by_card.is_empty() {
            let refs = kept
                .refs
                .as_ref()
                .expect("the refs are made before the lift");
            let mut gone_with: HashSet<String> =
                ids.iter().chain(repathed.iter()).cloned().collect();
            for id in ids.iter().chain(repathed.iter()) {
                gone_with.extend(refs.named_by(id).cloned());
            }
            let mut names: Vec<String> = names;
            names.extend(
                olds.iter()
                    .filter_map(|(_, b)| b.as_ref())
                    .chain(
                        ids.iter()
                            .chain(repathed.iter())
                            .filter_map(|id| self.docs.get(id)),
                    )
                    .filter_map(|d| text(d, "name"))
                    .map(crate::pathtext::fold),
            );
            let words = self
                .words
                .get_mut()
                .get_or_insert_with(|| Words::of(&self.docs));
            let saying = words.saying(&names);
            let docs = &self.docs;
            by_card.retain(|id, part| {
                if gone_with.contains(id) || (moved && !part.is_empty()) {
                    return false;
                }
                docs.contains_key(id) && saying.as_ref().is_some_and(|s| !s.contains(id))
            });
        }
        // Every field by its id is as it was unless a card's fields changed, or a card was made,
        // let go of or made of another kind.
        if fields {
            self.fields = std::cell::OnceCell::new();
        }
        true
    }
}
