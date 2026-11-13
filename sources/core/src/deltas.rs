//! The screen's reading of a project (`project_detail`) told as what changed since the reader's last
//! (`specboard://SpecBoard/Docs/Design:CoreApi`「頼み」): every answer carries a hash of each of its
//! entries (`hashes`) and one over them all (`digest`); a request that says the digest the reader
//! holds (`since`) is answered with only the entries that differ from it (`delta`), when the core
//! sent that reader that digest of the same tree. The detail is built whole each time and compared
//! entry by entry, rather than worked out from what was written: one card's change reaches what other
//! cards are drawn with (the lines their text names, a table's Markdown, the lines a discriminator
//! picks, where a ghost leads, the board's lines to main, a branch's view of main), and the built
//! answer misses none of it. What was sent is kept as its hashes alone, by reader, project and tree.

use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

/// The lists of the answer whose entries are told apart by their `id`; each one's order is an entry
/// too (`order/<list>`), so a reader puts its entries in the core's order without being sent them.
const LISTS: [&str; 4] = ["schemas", "inner", "ghosts", "sections"];

/// The entries the answer has one of.
const ONES: [&str; 3] = ["project", "kinds", "filePlaces"];

/// How many of what was sent are kept for one tree of a reader: a reader may still hold one sent
/// before the last (a read it let go of for a newer one, a second window on the same tree), and is
/// answered from it rather than sent the whole again.
const STATES_PER_TREE: usize = 3;

/// How many trees are kept for one reader: those it read last (the open tree, the board's main,
/// a working directory's).
const TREES_PER_READER: usize = 8;

/// How many readers are kept: those that read last. The agent lets one go when its connection goes;
/// this bounds what is kept of readers that never say they went.
const READERS: usize = 32;

/// What was sent of one tree: each entry's hash by its key, and the digest over them.
struct State {
    digest: String,
    hashes: HashMap<String, String>,
}

/// A tree as the screen reads it (`screen_detail::read`), with what its entries are told apart by:
/// made once for all who read the tree while the store stands as it was read (`kept`), and
/// answered from, never changed.
pub(crate) struct Read {
    detail: Value,
    groups: Vec<(String, Vec<usize>)>,
    state: Arc<State>,
}

impl Read {
    fn of(detail: Value) -> Read {
        let groups = groups_of(&detail);
        let hashes = hashes_of(&detail, &groups);
        let state = Arc::new(State {
            digest: digest_of(&hashes),
            hashes,
        });
        Read {
            detail,
            groups,
            state,
        }
    }

    /// What it weighs kept (`kept::weigh`).
    fn weight(&self) -> usize {
        crate::kept::weigh(&self.detail)
            + self
                .state
                .hashes
                .keys()
                .map(|key| 2 * key.len() + 96)
                .sum::<usize>()
    }
}

/// The tree `branch` of the project as the screen reads it: the one kept when the read stands at a
/// moment of the store one was made at (`Reader::at`), else made now.
pub(crate) fn read(
    store: &crate::store::Reader,
    project: &str,
    branch: Option<&str>,
) -> Result<Arc<Read>, String> {
    let make = || -> Result<(Read, usize), String> {
        let read = Read::of(crate::screen_detail::read(store, project, branch)?);
        let weight = read.weight();
        Ok((read, weight))
    };
    let Some(at) = &store.at else {
        return make().map(|(read, _)| Arc::new(read));
    };
    at.reading(
        crate::kept::Kind::Detail,
        project,
        branch.unwrap_or(""),
        || crate::workdir::kept_by(store.conn, project),
        make,
        |kept, made| {
            kept.state.digest == made.state.digest
                && crate::kept::written_alike(&kept.detail, &made.detail)
        },
    )
}

type TreeKey = (String, String);

/// What was sent to one reader: its trees, the one read last at the back, each with what it was sent
/// of it, the last at the back.
#[derive(Default)]
struct Reader {
    trees: VecDeque<(TreeKey, VecDeque<Arc<State>>)>,
}

/// What was sent to each reader (`Core::call`'s `connection`; none for the app that holds the core
/// itself), the one that read last at the back.
#[derive(Default)]
pub(crate) struct Sent {
    readers: Mutex<VecDeque<(String, Reader)>>,
}

impl Sent {
    /// The answer to `project_detail` of `read`, the project as `tree` sees it now, for `reader`:
    /// only what differs from the digest `since` when it was sent to this reader of this tree,
    /// else the whole with its hashes. Either way, what it is now is kept as sent.
    pub(crate) fn answer(
        &self,
        reader: &str,
        project: &str,
        tree: &str,
        read: &Read,
        since: Option<&str>,
    ) -> Value {
        let key = (project.to_string(), tree.to_string());
        let base = since.and_then(|since| self.find(reader, &key, since));
        self.keep(reader, key, read.state.clone());
        match base {
            Some(base) => delta(&read.detail, &base, &read.state, &read.groups),
            None => whole(&read.detail, &read.state),
        }
    }

    /// Lets go of what was sent to `reader`.
    pub(crate) fn forget(&self, reader: &str) {
        self.readers
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .retain(|(r, _)| r != reader);
    }

    fn find(&self, reader: &str, key: &TreeKey, digest: &str) -> Option<Arc<State>> {
        let readers = self.readers.lock().unwrap_or_else(|e| e.into_inner());
        let (_, kept) = readers.iter().find(|(r, _)| r == reader)?;
        let (_, states) = kept.trees.iter().find(|(k, _)| k == key)?;
        states.iter().find(|s| s.digest == digest).cloned()
    }

    fn keep(&self, reader: &str, key: TreeKey, state: Arc<State>) {
        let mut readers = self.readers.lock().unwrap_or_else(|e| e.into_inner());
        let mut kept = match readers.iter().position(|(r, _)| r == reader) {
            Some(at) => readers.remove(at).map(|(_, kept)| kept).unwrap_or_default(),
            None => Reader::default(),
        };
        let mut states = match kept.trees.iter().position(|(k, _)| *k == key) {
            Some(at) => kept
                .trees
                .remove(at)
                .map(|(_, states)| states)
                .unwrap_or_default(),
            None => VecDeque::new(),
        };
        states.retain(|s| s.digest != state.digest);
        states.push_back(state);
        while states.len() > STATES_PER_TREE {
            states.pop_front();
        }
        kept.trees.push_back((key, states));
        while kept.trees.len() > TREES_PER_READER {
            kept.trees.pop_front();
        }
        readers.push_back((reader.to_string(), kept));
        while readers.len() > READERS {
            readers.pop_front();
        }
    }
}

/// An entry's id in its list (a line's, the card it leaves).
fn id_of<'a>(entry: &'a Value, key: &str) -> &'a str {
    entry[key].as_str().unwrap_or_default()
}

/// The lines of `relations` by the card each leaves, in the order the cards first come; each card's
/// lines as the places they are at in `relations`, in their order.
fn groups_of(detail: &Value) -> Vec<(String, Vec<usize>)> {
    let mut at: HashMap<&str, usize> = HashMap::new();
    let mut out: Vec<(String, Vec<usize>)> = Vec::new();
    for (i, line) in detail["relations"]
        .as_array()
        .into_iter()
        .flatten()
        .enumerate()
    {
        let from = id_of(line, "fromSchemaId");
        match at.get(from) {
            Some(&n) => out[n].1.push(i),
            None => {
                at.insert(from, out.len());
                out.push((from.to_string(), vec![i]));
            }
        }
    }
    out
}

/// The hash of each entry of `detail` by its key: `<list>/<id>` for each entry of the lists,
/// `order/<list>` for the ids of each list in their order, `relations/<id>` for the lines that leave
/// a card together, `project` and `kinds`.
fn hashes_of(detail: &Value, groups: &[(String, Vec<usize>)]) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for list in LISTS {
        let mut order = Sha256::new();
        for entry in detail[list].as_array().into_iter().flatten() {
            let id = id_of(entry, "id");
            out.insert(format!("{list}/{id}"), hash_of([entry].into_iter()));
            order.update(id.as_bytes());
            order.update(b"\n");
        }
        out.insert(format!("order/{list}"), short(order));
    }
    let relations = detail["relations"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default();
    for (from, at) in groups {
        out.insert(
            format!("relations/{from}"),
            hash_of(at.iter().map(|&i| &relations[i])),
        );
    }
    for one in ONES {
        out.insert(one.to_string(), hash_of([&detail[one]].into_iter()));
    }
    out
}

/// The hash of entries, of the JSON text the answer carries them as: what is built the same is
/// written the same (the keys in the order they are put in).
fn hash_of<'a>(entries: impl Iterator<Item = &'a Value>) -> String {
    let mut hasher = Sha256::new();
    for entry in entries {
        // Written to a hasher, it cannot fail.
        let _ = serde_json::to_writer(&mut hasher, entry);
        hasher.update(b"\n");
    }
    short(hasher)
}

/// The digest of a reading: over each key and its hash, the keys in the order of their UTF-8 bytes,
/// which a reader works out the same of the hashes it holds.
fn digest_of(hashes: &HashMap<String, String>) -> String {
    let mut keys: Vec<&String> = hashes.keys().collect();
    keys.sort();
    let mut hasher = Sha256::new();
    for key in keys {
        hasher.update(key.as_bytes());
        hasher.update(b"\t");
        hasher.update(hashes[key].as_bytes());
        hasher.update(b"\n");
    }
    short(hasher)
}

/// The first 8 bytes of a hash, in lowercase hex: enough to tell one reading of a tree from the next.
fn short(hasher: Sha256) -> String {
    hasher.finalize()[..8]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn sorted(hashes: impl Iterator<Item = (String, String)>) -> Map<String, Value> {
    let mut pairs: Vec<(String, String)> = hashes.collect();
    pairs.sort();
    pairs
        .into_iter()
        .map(|(k, h)| (k, Value::String(h)))
        .collect()
}

/// The whole detail, with its hashes and digest.
fn whole(detail: &Value, now: &State) -> Value {
    let mut detail = detail.clone();
    if let Some(out) = detail.as_object_mut() {
        let hashes = sorted(now.hashes.iter().map(|(k, h)| (k.clone(), h.clone())));
        out.insert("hashes".into(), Value::Object(hashes));
        out.insert("digest".into(), now.digest.clone().into());
    }
    detail
}

/// What differs in `detail` (`now`) from what was sent (`base`): the entries whose hash is not the
/// one sent (new ones among them), the ids of the entries no longer there, the lists whose order is
/// not the one sent, and the hashes that are not.
fn delta(detail: &Value, base: &State, now: &State, groups: &[(String, Vec<usize>)]) -> Value {
    let differs = |key: &str| base.hashes.get(key) != now.hashes.get(key);
    let mut changed = Map::new();
    let mut order = Map::new();
    for list in LISTS {
        let entries = detail[list]
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or_default();
        if differs(&format!("order/{list}")) {
            let ids = entries
                .iter()
                .map(|e| Value::from(id_of(e, "id")))
                .collect();
            order.insert(list.into(), Value::Array(ids));
        }
        let entries = entries
            .iter()
            .filter(|e| differs(&format!("{list}/{}", id_of(e, "id"))))
            .cloned()
            .collect();
        changed.insert(list.into(), Value::Array(entries));
    }
    let lines = detail["relations"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default();
    let mut by_card = Map::new();
    for (from, at) in groups {
        if differs(&format!("relations/{from}")) {
            let group = at.iter().filter_map(|&i| lines.get(i).cloned()).collect();
            by_card.insert(from.clone(), Value::Array(group));
        }
    }
    changed.insert("relations".into(), Value::Object(by_card));
    for one in ONES {
        if differs(one) {
            changed.insert(one.into(), detail[one].clone());
        }
    }
    let mut gone: HashMap<&str, Vec<&str>> = HashMap::new();
    for key in base.hashes.keys().filter(|k| !now.hashes.contains_key(*k)) {
        if let Some((list, id)) = key.split_once('/')
            && list != "order"
        {
            gone.entry(list).or_default().push(id);
        }
    }
    let mut removed = Map::new();
    for list in LISTS.into_iter().chain(["relations"]) {
        let mut ids = gone.remove(list).unwrap_or_default();
        ids.sort_unstable();
        removed.insert(list.into(), json!(ids));
    }
    let hashes = sorted(
        now.hashes
            .iter()
            .filter(|(k, h)| base.hashes.get(*k) != Some(*h))
            .map(|(k, h)| (k.clone(), h.clone())),
    );
    json!({
        "delta": true,
        "from": base.digest,
        "digest": now.digest,
        "changed": changed,
        "removed": removed,
        "order": order,
        "hashes": hashes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn detail(cards: &[(&str, &str)]) -> Value {
        json!({
            "project": {"id": "p"},
            "schemas": cards.iter().map(|(id, name)| json!({"id": id, "name": name})).collect::<Vec<_>>(),
            "inner": [],
            "ghosts": [],
            "sections": [{"id": "s"}],
            "relations": [],
            "kinds": {},
        })
    }

    #[test]
    fn what_was_kept_is_bounded_by_reader_tree_and_state() {
        let sent = Sent::default();
        for n in 0..(READERS + 3) {
            sent.answer(
                &n.to_string(),
                "p",
                "",
                &Read::of(detail(&[("a", "A")])),
                None,
            );
        }
        assert_eq!(sent.readers.lock().unwrap().len(), READERS);
        for n in 0..(TREES_PER_READER + 2) {
            sent.answer(
                "r",
                "p",
                &n.to_string(),
                &Read::of(detail(&[("a", "A")])),
                None,
            );
        }
        let readers = sent.readers.lock().unwrap();
        let (_, r) = readers.iter().find(|(r, _)| r == "r").unwrap();
        assert_eq!(r.trees.len(), TREES_PER_READER);
        drop(readers);
        for n in 0..(STATES_PER_TREE + 2) {
            sent.answer(
                "s",
                "p",
                "",
                &Read::of(detail(&[("a", &n.to_string())])),
                None,
            );
        }
        let readers = sent.readers.lock().unwrap();
        let (_, s) = readers.iter().find(|(r, _)| r == "s").unwrap();
        assert_eq!(s.trees[0].1.len(), STATES_PER_TREE);
    }

    #[test]
    fn a_reader_that_holds_one_sent_before_the_last_is_still_sent_what_changed() {
        let sent = Sent::default();
        let first = sent.answer("r", "p", "", &Read::of(detail(&[("a", "A")])), None);
        let held = first["digest"].as_str().unwrap().to_string();
        sent.answer("r", "p", "", &Read::of(detail(&[("a", "B")])), None);
        let answer = sent.answer("r", "p", "", &Read::of(detail(&[("a", "C")])), Some(&held));
        assert_eq!(answer["delta"], true);
        assert_eq!(answer["changed"]["schemas"][0]["name"], "C");
    }
}
