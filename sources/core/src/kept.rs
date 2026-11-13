//! What the core read of the store, kept for the reads that follow while the store stands as it was
//! read (`specboard://SpecBoard/Docs/Design:CoreApi`「読みを覚える」): a tree worked out of its
//! documents, the window's reading of one, the list of the branches. Working a tree out costs the
//! whole project each time, and most reads follow one another with nothing written between.
//!
//! A reading is kept with the moment of the store it was made at, and answers only reads of that
//! same moment. The moment is what one connection of its own (`Watch`) says of the store —
//! SQLite's `data_version`, which differs between two askings whenever any other connection, of
//! this core or of another holder of the file, committed a change in between. A read asks for it
//! before and after its own transaction begins to read: the same both times, nothing was committed
//! in between, so what the transaction sees is the store of that moment and no other; not the
//! same, the read works everything out itself and keeps nothing (`At::moment` is none). So a
//! reading is never taken for a store that has since been written, whoever wrote it, and nothing
//! has to be told to this module when a write is made.
//!
//! What a working directory's folder holds is not the store's, and a tree may be made from it too
//! (a note put in a directory): a reading is kept by which reading of the folders the connection
//! holds as well (`workdir::kept_by`), and not at all while that one is the connection's alone.
//! A file's text is read from the disk each time it is asked for: what was made looking at one
//! (the functions and types a line lands on) is not kept either.
//! What a reading says in words (the kinds' names) is in the language of the call: kept by it too.
//!
//! What is kept takes room — a tree of a few thousand cards, tens of megabytes: no more than
//! `ROOM` is kept, and what no read has taken for `IDLE` is let go of (`Readings::tend`), so that
//! the room is held while reads follow one another and given back once they stop.

use rusqlite::{Connection, OpenFlags};
use std::any::Any;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

/// What is kept: by which the readings made are counted, too (`Core::readings_made`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Kind {
    /// A tree as its documents read (`detail::Tree`).
    Tree,
    /// The window's reading of a tree, with the hash of each entry (`deltas::Read`).
    Detail,
    /// The list of the branches (`tools::branches::branches_view`).
    Branches,
    /// How much of the store a project takes (`tools::reads::project_size`): counted from every row
    /// of its history.
    Sizes,
}

impl Kind {
    const ALL: [Kind; 4] = [Kind::Tree, Kind::Detail, Kind::Branches, Kind::Sizes];

    pub(crate) fn named(name: &str) -> Option<Kind> {
        Kind::ALL.into_iter().find(|k| k.name() == name)
    }

    fn name(self) -> &'static str {
        match self {
            Kind::Tree => "tree",
            Kind::Detail => "detail",
            Kind::Branches => "branches",
            Kind::Sizes => "sizes",
        }
    }

    /// Whether what it says in words may be said otherwise to the window than to MCP
    /// (`text::bind_screen`): then one made for the one is not the other's.
    fn worded_by_asker(self) -> bool {
        self == Kind::Branches
    }
}

/// The most the readings kept may weigh together, in bytes as `weigh` counts them: past it the
/// ones read longest ago are let go, and one heavier than all of it is not kept at all.
const ROOM: usize = 256 * 1024 * 1024;

/// How long a reading is kept with no read taking it.
const IDLE: std::time::Duration = std::time::Duration::from_secs(90);

/// How long a reading that was never made is left in its place: far longer than making one takes.
const NEVER_MADE: std::time::Duration = std::time::Duration::from_secs(600);

/// A moment of the store: which connection told it (one more each time the watch is opened anew,
/// whose count starts over), and what it said.
type Moment = (u64, i64);

/// What the first to ask made of a reading.
enum Made {
    Kept(Arc<dyn Any + Send + Sync>),
    /// Its own alone — it could not be made, the folders were read again while it was, or a file
    /// was looked at as it is on the disk: each of the others makes its own.
    Alone,
}

struct Slot {
    kind: Kind,
    project: String,
    tree: String,
    /// The language it is worded in, and whether for the window (`Kind::worded_by_asker`).
    words: (usize, bool),
    moment: Moment,
    folders: i64,
    /// Made by the first to ask; the others of the same moment wait for it and take it.
    made: Arc<OnceLock<Made>>,
    weight: usize,
    /// When a read last asked for it.
    used: std::time::Instant,
}

/// The connection that tells the store's moments.
#[derive(Default)]
struct Watch {
    conn: Option<Connection>,
    opened: u64,
}

/// The readings kept for one store (`Core`), and the connection that tells its moments.
pub(crate) struct Readings {
    place: PathBuf,
    watch: Mutex<Watch>,
    /// The one read last at the back.
    slots: Mutex<Vec<Slot>>,
    made: [AtomicU64; Kind::ALL.len()],
    idle: std::time::Duration,
    /// Whether the thread that lets go of what no read takes any more runs (`tend`): read and
    /// written with `slots` held.
    tended: std::sync::atomic::AtomicBool,
    /// That thread, to be woken when the readings go: it ends then, not at its next look.
    tender: Mutex<Option<std::thread::Thread>>,
}

impl Drop for Readings {
    fn drop(&mut self) {
        if let Some(tender) = self.tender.lock().unwrap_or_else(|e| e.into_inner()).take() {
            tender.unpark();
        }
    }
}

impl Readings {
    pub(crate) fn of(place: &Path) -> Readings {
        Readings::kept_for(place, IDLE)
    }

    /// `of`, a reading kept for `idle` with no read taking it.
    fn kept_for(place: &Path, idle: std::time::Duration) -> Readings {
        Readings {
            place: place.to_path_buf(),
            watch: Mutex::new(Watch::default()),
            slots: Mutex::new(Vec::new()),
            made: Default::default(),
            idle,
            tended: std::sync::atomic::AtomicBool::new(false),
            tender: Mutex::new(None),
        }
    }

    /// Has what no read took for `idle` let go of, from a thread of its own, for as long as
    /// anything is kept: started when something is kept and none runs (`slots` is the caller's,
    /// held). The thread holds the readings only while it looks, and ends with them.
    fn tend(self: &Arc<Self>) {
        if self.tended.swap(true, Ordering::SeqCst) {
            return;
        }
        let readings = Arc::downgrade(self);
        let every = (self.idle / 4).max(std::time::Duration::from_millis(10));
        let started = std::thread::Builder::new()
            .name("specboard-kept".into())
            .spawn(move || {
                loop {
                    std::thread::park_timeout(every);
                    let Some(readings) = readings.upgrade() else {
                        return;
                    };
                    let mut slots = readings.slots();
                    slots.retain(|s| {
                        let waited = s.used.elapsed();
                        match s.made.get() {
                            Some(_) => waited < readings.idle,
                            None => waited < NEVER_MADE,
                        }
                    });
                    if slots.is_empty() {
                        readings.tended.store(false, Ordering::SeqCst);
                        return;
                    }
                }
            });
        match started {
            Ok(thread) => {
                *self.tender.lock().unwrap_or_else(|e| e.into_inner()) =
                    Some(thread.thread().clone());
            }
            Err(_) => self.tended.store(false, Ordering::SeqCst),
        }
    }

    /// The store's moment now: none when it cannot be told (then nothing is kept).
    fn moment(&self) -> Option<Moment> {
        let mut watch = self.watch.lock().unwrap_or_else(|e| e.into_inner());
        if watch.conn.is_none() {
            let conn = Connection::open_with_flags(
                &self.place,
                OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
            )
            .ok()?;
            // Every read asks here as it begins: one that cannot be told at once is not waited for.
            conn.busy_timeout(std::time::Duration::ZERO).ok()?;
            watch.conn = Some(conn);
            watch.opened += 1;
        }
        let told = watch
            .conn
            .as_ref()?
            .query_row("PRAGMA main.data_version", [], |r| r.get::<_, i64>(0));
        match told {
            Ok(version) => Some((watch.opened, version)),
            // Busy for now: the connection stands, and tells the next one who asks.
            Err(rusqlite::Error::SqliteFailure(error, _))
                if error.code == rusqlite::ErrorCode::DatabaseBusy =>
            {
                None
            }
            Err(_) => {
                // Asked on a connection opened anew next time.
                watch.conn = None;
                None
            }
        }
    }

    /// How many readings of `kind` were worked out so far to answer a read.
    pub(crate) fn made(&self, kind: Kind) -> u64 {
        self.made[kind as usize].load(Ordering::Relaxed)
    }

    fn count(&self, kind: Kind) {
        // What is made again only to hold what is kept to it answers no read.
        if !HOLDING.with(std::cell::Cell::get) {
            self.made[kind as usize].fetch_add(1, Ordering::Relaxed);
        }
    }

    fn slots(&self) -> std::sync::MutexGuard<'_, Vec<Slot>> {
        self.slots.lock().unwrap_or_else(|e| e.into_inner())
    }
}

thread_local! {
    /// Whether this thread is making a reading again to hold a kept one to it (debug builds):
    /// what that makes on its way is not counted, nor held to another making in its turn.
    static HOLDING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Where a read stands: the readings of its core, and the moment its transaction reads the store
/// at, when that could be told (`Core::reading`).
#[derive(Clone)]
pub(crate) struct At {
    readings: Arc<Readings>,
    moment: Option<Moment>,
    /// Whether what it makes is kept for the reads that follow (`passing`: not).
    keeps: bool,
}

impl At {
    /// A read's standing, its transaction begun by `begin` — which must have read the store by the
    /// time it returns, so that what the transaction sees from then on is fixed: the moment is
    /// told only when it was the same before and after.
    pub(crate) fn begun<E>(
        readings: &Arc<Readings>,
        begin: impl FnOnce() -> Result<(), E>,
    ) -> Result<At, E> {
        let before = readings.moment();
        begin()?;
        let after = readings.moment();
        Ok(At {
            readings: readings.clone(),
            moment: before.filter(|b| after == Some(*b)),
            keeps: true,
        })
    }

    /// The same standing, for what is read in passing: a reading kept is taken, one that is not is
    /// made and let go of — what is read once to make something else that is kept (each branch's
    /// tree, for the list of the branches) would only take the room of what is read again.
    pub(crate) fn passing(&self) -> At {
        At {
            keeps: false,
            ..self.clone()
        }
    }

    /// The reading `kind` of `tree` of `project`: the one kept of this moment, else made by `make`
    /// (with what it weighs) and kept. `folders` says which reading of the project's working
    /// directories the read's connection holds now (`workdir::kept_by`).
    ///
    /// `same` holds two makings to be one reading: built for the tests (debug), a reading taken
    /// from the kept ones is made again as well and held to it, so every test that reads holds
    /// what is kept to what the store says.
    pub(crate) fn reading<T: Any + Send + Sync>(
        &self,
        kind: Kind,
        project: &str,
        tree: &str,
        folders: impl Fn() -> Option<i64>,
        make: impl FnOnce() -> Result<(T, usize), String>,
        same: impl Fn(&T, &T) -> bool,
    ) -> Result<Arc<T>, String> {
        let readings = &self.readings;
        let (Some(moment), Some(held)) = (self.moment, folders()) else {
            readings.count(kind);
            return make().map(|(made, _)| Arc::new(made));
        };
        let words = (
            crate::text::current_index(),
            kind.worded_by_asker() && crate::text::on_screen(),
        );
        let cell = {
            let mut slots = readings.slots();
            // What was made of a moment gone is no read's again.
            slots.retain(|s| s.moment >= moment);
            let found = slots.iter().position(|s| {
                s.kind == kind
                    && s.project == project
                    && s.tree == tree
                    && s.words == words
                    && s.moment == moment
                    && s.folders == held
            });
            let mut slot = match found {
                Some(at) => slots.remove(at),
                None if !self.keeps => {
                    drop(slots);
                    readings.count(kind);
                    return make().map(|(made, _)| Arc::new(made));
                }
                None => Slot {
                    kind,
                    project: project.to_string(),
                    tree: tree.to_string(),
                    words,
                    moment,
                    folders: held,
                    made: Arc::new(OnceLock::new()),
                    weight: 0,
                    used: std::time::Instant::now(),
                },
            };
            slot.used = std::time::Instant::now();
            let cell = slot.made.clone();
            slots.push(slot);
            cell
        };
        let mut make = Some(make);
        // What this one made itself, and what it weighs when it is kept.
        let mut mine: Option<Result<Arc<T>, String>> = None;
        let mut weight = None;
        let made = cell.get_or_init(|| {
            readings.count(kind);
            let make = make.take().expect("made once");
            let looked = crate::workdir::files_looked_at_on_this_thread();
            match make() {
                // Made from the store and the folders as they were read when it began, and from
                // no file as it is on the disk now.
                Ok((made, weighs))
                    if folders() == Some(held)
                        && crate::workdir::files_looked_at_on_this_thread() == looked =>
                {
                    let made = Arc::new(made);
                    mine = Some(Ok(made.clone()));
                    weight = Some(weighs);
                    Made::Kept(made)
                }
                Ok((made, _)) => {
                    mine = Some(Ok(Arc::new(made)));
                    Made::Alone
                }
                Err(problem) => {
                    mine = Some(Err(problem));
                    Made::Alone
                }
            }
        });
        {
            let mut slots = readings.slots();
            let found = slots.iter().position(|s| Arc::ptr_eq(&s.made, &cell));
            match (made, found) {
                (Made::Alone, Some(at)) => {
                    slots.remove(at);
                }
                (Made::Kept(_), Some(at)) => {
                    if let Some(weight) = weight {
                        // Read last now that it is made: what it was made from was read before it.
                        let mut slot = slots.remove(at);
                        slot.weight = weight;
                        slot.used = std::time::Instant::now();
                        slots.push(slot);
                        readings.tend();
                        // The ones read longest ago first; one still being made weighs
                        // nothing yet, and letting go of it would free nothing.
                        let mut held: usize = slots.iter().map(|s| s.weight).sum();
                        let mut next = 0;
                        while held > ROOM && next < slots.len() {
                            if slots[next].weight == 0 {
                                next += 1;
                            } else {
                                held -= slots.remove(next).weight;
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        if let Some(mine) = mine {
            return mine;
        }
        let make = make.take().expect("not made here");
        let Made::Kept(kept) = made else {
            readings.count(kind);
            return make().map(|(made, _)| Arc::new(made));
        };
        let kept = kept
            .clone()
            .downcast::<T>()
            .map_err(|_| "a reading of another kind was kept".to_string())?;
        // Taken, not made here: held to what making it now gives.
        if cfg!(debug_assertions) && !HOLDING.with(|h| h.replace(true)) {
            // Put back however the making ends.
            struct Held;
            impl Drop for Held {
                fn drop(&mut self) {
                    HOLDING.with(|h| h.set(false));
                }
            }
            let (again, _) = {
                let _held = Held;
                make()?
            };
            assert!(
                same(&kept, &again),
                "the {} kept of {project} ({tree:?}) is not what the store reads as now",
                kind.name()
            );
        }
        Ok(kept)
    }
}

/// Whether two JSON values are written the same, their keys in the same order: what two makings of
/// one reading are held to (`At::reading`'s `same`) — the answers are sent as written.
pub(crate) fn written_alike(a: &serde_json::Value, b: &serde_json::Value) -> bool {
    match (serde_json::to_vec(a), serde_json::to_vec(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

/// What a JSON value takes in memory, about: what bounds what is kept (`ROOM`). Counted as the
/// values and their maps hold it, and half as much again for what the allocator rounds up and the
/// maps and lists keep spare — a tree of 3,000 small cards weighed so takes what was measured of it.
pub(crate) fn weigh(value: &serde_json::Value) -> usize {
    counted(value) * 3 / 2
}

/// `weigh`, of an object's fields.
pub(crate) fn weigh_fields(fields: &serde_json::Map<String, serde_json::Value>) -> usize {
    (std::mem::size_of::<serde_json::Value>() + counted_fields(fields)) * 3 / 2
}

/// A text's own bytes, apart from what holds it: its length and what an allocation costs.
fn text_bytes(text: &str) -> usize {
    text.len() + 16
}

fn counted(value: &serde_json::Value) -> usize {
    use serde_json::Value;
    const NODE: usize = std::mem::size_of::<Value>();
    match value {
        Value::String(text) => NODE + text_bytes(text),
        Value::Array(items) => NODE + items.iter().map(counted).sum::<usize>(),
        Value::Object(fields) => NODE + counted_fields(fields),
        _ => NODE,
    }
}

/// Each field: its place in the map (its hash, its key's handle, its place among the indices),
/// its key's text and its value.
fn counted_fields(fields: &serde_json::Map<String, serde_json::Value>) -> usize {
    fields
        .iter()
        .map(|(key, value)| 48 + text_bytes(key) + counted(value))
        .sum::<usize>()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A store of its own, kept in a write-ahead log, and a connection that writes it.
    fn store() -> (PathBuf, Connection) {
        let place = crate::fixtures::sample();
        let writer = Connection::open(&place).unwrap();
        writer
            .query_row("PRAGMA journal_mode = WAL", [], |r| r.get::<_, String>(0))
            .unwrap();
        (place, writer)
    }

    fn write(writer: &Connection) {
        writer
            .execute("UPDATE projects SET name = name || '!'", [])
            .unwrap();
    }

    fn name(conn: &Connection) -> String {
        conn.query_row("SELECT name FROM projects LIMIT 1", [], |r| r.get(0))
            .unwrap()
    }

    fn at(readings: &Arc<Readings>) -> At {
        At::begun(readings, || Ok::<(), String>(())).unwrap()
    }

    fn read(at: &At, tree: &str, folders: Option<i64>, n: u64) -> u64 {
        *at.reading(
            Kind::Tree,
            "p",
            tree,
            || folders,
            || Ok((n, 8)),
            // Told apart by the tests themselves: what is kept is not what would be made now.
            |_, _| true,
        )
        .unwrap()
    }

    /// What `Core::reading` leans on: once a transaction has asked for the data version, what it
    /// reads is fixed, and a change committed after is not seen until it ends.
    #[test]
    fn a_transaction_that_asked_the_data_version_reads_the_store_of_then() {
        let (place, writer) = store();
        let reader = Connection::open(&place).unwrap();
        let was = name(&reader);
        reader.execute_batch("BEGIN").unwrap();
        reader
            .query_row("PRAGMA data_version", [], |r| r.get::<_, i64>(0))
            .unwrap();
        write(&writer);
        assert_eq!(name(&reader), was);
        reader.execute_batch("COMMIT").unwrap();
        assert_eq!(name(&reader), format!("{was}!"));
    }

    #[test]
    fn the_moment_differs_once_another_connection_has_written() {
        let (place, writer) = store();
        let readings = Readings::of(&place);
        let first = readings.moment().unwrap();
        assert_eq!(readings.moment(), Some(first));
        write(&writer);
        let second = readings.moment().unwrap();
        assert!(second > first);
        assert_eq!(readings.moment(), Some(second));
    }

    #[test]
    fn a_reading_is_made_once_for_a_moment_and_again_for_the_next() {
        let (place, writer) = store();
        let readings = Arc::new(Readings::of(&place));
        assert_eq!(read(&at(&readings), "", Some(0), 1), 1);
        // Kept: what the second would have made is not taken.
        assert_eq!(read(&at(&readings), "", Some(0), 2), 1);
        assert_eq!(readings.made(Kind::Tree), 1);
        // Another tree, and the same tree by another reading of the folders, are their own.
        assert_eq!(read(&at(&readings), "b", Some(0), 3), 3);
        assert_eq!(read(&at(&readings), "", Some(7), 4), 4);
        assert_eq!(read(&at(&readings), "", Some(0), 5), 1);
        write(&writer);
        assert_eq!(read(&at(&readings), "", Some(0), 6), 6);
        assert_eq!(readings.made(Kind::Tree), 4);
        // What was kept of the moment before is let go.
        assert_eq!(readings.slots().len(), 1);
    }

    #[test]
    fn what_could_not_be_made_or_was_made_of_folders_read_meanwhile_is_not_kept() {
        let (place, _writer) = store();
        let readings = Arc::new(Readings::of(&place));
        let at = at(&readings);
        let failed = at.reading(
            Kind::Branches,
            "p",
            "",
            || Some(0),
            || Err::<(u64, usize), _>("no".to_string()),
            |_, _| true,
        );
        assert_eq!(failed.err().as_deref(), Some("no"));
        assert!(readings.slots().is_empty());
        // The folders read again while it was made: the one who made it has it, no one else.
        let turn = std::cell::Cell::new(1);
        let made = at.reading(
            Kind::Tree,
            "p",
            "",
            || Some(turn.get()),
            || {
                turn.set(2);
                Ok((1u64, 8))
            },
            |_, _| true,
        );
        assert_eq!(*made.unwrap(), 1);
        assert!(readings.slots().is_empty());
        // Folders that are the connection's alone: made each time.
        assert_eq!(read(&at, "", None, 2), 2);
        assert_eq!(read(&at, "", None, 3), 3);
        assert!(readings.slots().is_empty());
    }

    #[test]
    fn what_is_read_in_passing_takes_what_is_kept_and_keeps_nothing() {
        let (place, _writer) = store();
        let readings = Arc::new(Readings::of(&place));
        let at = at(&readings);
        assert_eq!(read(&at, "kept", Some(0), 1), 1);
        let passing = at.passing();
        assert_eq!(read(&passing, "kept", Some(0), 2), 1);
        assert_eq!(read(&passing, "other", Some(0), 3), 3);
        assert_eq!(read(&passing, "other", Some(0), 4), 4);
        assert_eq!(readings.made(Kind::Tree), 3);
        assert_eq!(readings.slots().len(), 1);
    }

    #[test]
    fn what_a_reading_was_made_from_is_let_go_of_before_it() {
        let (place, _writer) = store();
        let readings = Arc::new(Readings::of(&place));
        let at = at(&readings);
        // A list made from two trees, each a third of the room: the list is read last.
        at.reading(
            Kind::Branches,
            "p",
            "",
            || Some(0),
            || {
                for tree in ["a", "b"] {
                    at.reading(
                        Kind::Tree,
                        "p",
                        tree,
                        || Some(0),
                        || Ok((0u64, ROOM / 3)),
                        |_, _| true,
                    )?;
                }
                Ok((0u64, ROOM / 3))
            },
            |_, _| true,
        )
        .unwrap();
        at.reading(
            Kind::Tree,
            "p",
            "c",
            || Some(0),
            || Ok((0u64, ROOM / 3)),
            |_, _| true,
        )
        .unwrap();
        let slots = readings.slots();
        let kept: Vec<(Kind, &str)> = slots.iter().map(|s| (s.kind, s.tree.as_str())).collect();
        assert_eq!(
            kept,
            [(Kind::Tree, "b"), (Kind::Branches, ""), (Kind::Tree, "c")]
        );
    }

    #[test]
    fn what_no_read_takes_for_a_while_is_let_go_of() {
        let (place, _writer) = store();
        let readings = Arc::new(Readings::kept_for(
            &place,
            std::time::Duration::from_millis(100),
        ));
        let at = at(&readings);
        assert_eq!(read(&at, "", Some(0), 1), 1);
        assert_eq!(readings.slots().len(), 1);
        // Left alone: let go of, however long the wait is made by a busy machine.
        let begun = std::time::Instant::now();
        while !readings.slots().is_empty() {
            assert!(
                begun.elapsed() < std::time::Duration::from_secs(30),
                "what no read took was kept"
            );
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        // Asked for again, it is made again and kept as before.
        assert_eq!(read(&at, "", Some(0), 2), 2);
        assert_eq!(read(&at, "", Some(0), 3), 2);
        assert_eq!(readings.made(Kind::Tree), 2);
    }

    #[test]
    fn a_read_begun_while_the_store_is_written_keeps_nothing() {
        let (place, writer) = store();
        let readings = Arc::new(Readings::of(&place));
        readings.moment();
        let at = At::begun(&readings, || {
            write(&writer);
            Ok::<(), String>(())
        })
        .unwrap();
        assert!(at.moment.is_none());
        assert_eq!(read(&at, "", Some(0), 1), 1);
        assert_eq!(read(&at, "", Some(0), 2), 2);
        assert!(readings.slots().is_empty());
    }

    #[test]
    fn what_is_kept_is_bounded_by_what_it_weighs() {
        let (place, _writer) = store();
        let readings = Arc::new(Readings::of(&place));
        let at = at(&readings);
        for n in 0..4u64 {
            at.reading(
                Kind::Tree,
                "p",
                &n.to_string(),
                || Some(0),
                || Ok((n, ROOM / 2 - 1)),
                |_, _| true,
            )
            .unwrap();
        }
        let slots = readings.slots();
        let trees: Vec<&str> = slots.iter().map(|s| s.tree.as_str()).collect();
        assert_eq!(trees, ["2", "3"]);
    }
}
