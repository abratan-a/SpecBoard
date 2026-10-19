//! What the files read as code came to, kept for the process by each file's path: each use looks at
//! the file's size and time first, and reads it again only when either changed — parsing it again
//! only when its text did (`sha256`). So what is answered is always the file's now, without waiting
//! for its working directory to be read again, and a file not changed costs a `stat`. Trees are not
//! kept, only what was found; the oldest used goes first past `MOST_KEPT`.

use super::Read;
use super::registry::Language;
use crate::text::Keyed;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::SystemTime;

/// About the most kept, in bytes.
pub const MOST_KEPT: usize = 64 * 1024 * 1024;

struct Entry {
    mtime: Option<SystemTime>,
    size: u64,
    sha: [u8; 32],
    language: String,
    /// The working directory it was read for: what is let go when its languages change.
    dir: String,
    read: Arc<Read>,
    cost: usize,
    used: u64,
}

#[derive(Default)]
struct Kept {
    entries: HashMap<PathBuf, Entry>,
    bytes: usize,
    tick: u64,
    /// How many files were parsed: what the tests hold a reuse to.
    parsed: u64,
}

fn kept() -> &'static Mutex<Kept> {
    static KEPT: OnceLock<Mutex<Kept>> = OnceLock::new();
    KEPT.get_or_init(|| Mutex::new(Kept::default()))
}

/// What one file read came to: its functions and types, a stamp of its text and language (the same
/// while neither changes), and its text when it was read for this use.
#[derive(Clone)]
pub struct Loaded {
    pub read: Arc<Read>,
    pub stamp: String,
    pub text: Option<Arc<Vec<u8>>>,
}

fn stamp_of(sha: &[u8; 32], language: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(sha);
    hasher.update(language.as_bytes());
    let digest = hasher.finalize();
    digest[..8].iter().map(|b| format!("{b:02x}")).collect()
}

fn cost_of(read: &Read) -> usize {
    let symbols: usize = read
        .found
        .symbols
        .iter()
        .map(|s| {
            160 + s.name.len()
                + s.path.len()
                + s.word.len()
                + s.parts.len() * 32
                + s.calls.iter().map(|c| c.len() + 24).sum::<usize>()
                + s.digest.len()
        })
        .sum();
    256 + symbols + read.line_starts.len() * 8
}

/// The file at `path` (absolute) read in `language` for the working directory `dir`, from what is
/// kept when it is the same file; with its text when `with_text`. `Err` when it cannot be read.
pub fn load(
    path: &Path,
    language: &'static Language,
    dir: &str,
    with_text: bool,
) -> Result<Loaded, Keyed> {
    let unreadable = || keyed!("code.unreadable", path = path.display());
    let meta = std::fs::metadata(path).map_err(|_| unreadable())?;
    if !meta.is_file() {
        return Err(unreadable());
    }
    let (mtime, size) = (meta.modified().ok(), meta.len());
    if !with_text {
        let mut held = kept().lock().unwrap();
        held.tick += 1;
        let tick = held.tick;
        if let Some(e) = held.entries.get_mut(path)
            && e.language == language.id
            && e.size == size
            && e.mtime.is_some()
            && e.mtime == mtime
        {
            e.used = tick;
            return Ok(Loaded {
                read: e.read.clone(),
                stamp: stamp_of(&e.sha, &e.language),
                text: None,
            });
        }
    }
    // Too large to be read: said so without reading it.
    if size as usize > super::MOST_BYTES && !with_text {
        let read = Arc::new(Read {
            found: Default::default(),
            line_starts: Vec::new(),
            problem: Some(keyed!(
                "code.tooLarge",
                size = size,
                most = super::MOST_BYTES
            )),
        });
        let mut sha = [0u8; 32];
        sha[..8].copy_from_slice(&size.to_le_bytes());
        return Ok(Loaded {
            stamp: stamp_of(&sha, &language.id),
            read,
            text: None,
        });
    }
    let bytes = std::fs::read(path).map_err(|_| unreadable())?;
    let sha: [u8; 32] = Sha256::digest(&bytes).into();
    {
        let mut held = kept().lock().unwrap();
        held.tick += 1;
        let tick = held.tick;
        if let Some(e) = held.entries.get_mut(path)
            && e.language == language.id
            && e.sha == sha
        {
            e.mtime = mtime;
            e.size = size;
            e.used = tick;
            return Ok(Loaded {
                read: e.read.clone(),
                stamp: stamp_of(&sha, &e.language),
                text: with_text.then(|| Arc::new(bytes)),
            });
        }
    }
    let read = Arc::new(super::read_text(language, &bytes));
    let cost = cost_of(&read);
    let mut held = kept().lock().unwrap();
    held.parsed += 1;
    held.tick += 1;
    let tick = held.tick;
    if let Some(before) = held.entries.remove(path) {
        held.bytes -= before.cost;
    }
    held.bytes += cost;
    held.entries.insert(
        path.to_path_buf(),
        Entry {
            mtime,
            size,
            sha,
            language: language.id.clone(),
            dir: dir.to_string(),
            read: read.clone(),
            cost,
            used: tick,
        },
    );
    if held.bytes > MOST_KEPT {
        let mut by_use: Vec<(u64, PathBuf)> = held
            .entries
            .iter()
            .map(|(p, e)| (e.used, p.clone()))
            .collect();
        by_use.sort();
        for (_, p) in by_use {
            if held.bytes <= MOST_KEPT * 3 / 4 {
                break;
            }
            if let Some(e) = held.entries.remove(&p) {
                held.bytes -= e.cost;
            }
        }
    }
    Ok(Loaded {
        read,
        stamp: stamp_of(&sha, &language.id),
        text: with_text.then(|| Arc::new(bytes)),
    })
}

/// What was kept of the files read for the working directory `dir` let go: its languages changed.
pub fn forget_dir(dir: &str) {
    let mut held = kept().lock().unwrap();
    let gone: Vec<PathBuf> = held
        .entries
        .iter()
        .filter(|(_, e)| e.dir == dir)
        .map(|(p, _)| p.clone())
        .collect();
    for p in gone {
        if let Some(e) = held.entries.remove(&p) {
            held.bytes -= e.cost;
        }
    }
}

/// The most whole files' fingerprints kept (`digest`): past it, they are all let go.
const MOST_DIGESTS: usize = 50_000;

/// A whole file's fingerprint as last made, by its path: with the size and time it was made at.
type Digests = HashMap<PathBuf, (Option<SystemTime>, u64, String)>;

fn digests() -> &'static Mutex<Digests> {
    static DIGESTS: OnceLock<Mutex<Digests>> = OnceLock::new();
    DIGESTS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// The fingerprint of the whole text of the file at `path` (absolute; `symbols::digest_of`), of
/// any language or none: made again only when its size or time changed. `Err` when it cannot be
/// read, or is larger than a file read as code is.
pub fn digest(path: &Path) -> Result<String, Keyed> {
    let unreadable = || keyed!("code.unreadable", path = path.display());
    let meta = std::fs::metadata(path).map_err(|_| unreadable())?;
    if !meta.is_file() {
        return Err(unreadable());
    }
    let (mtime, size) = (meta.modified().ok(), meta.len());
    if size as usize > super::MOST_BYTES {
        return Err(keyed!(
            "code.tooLarge",
            size = size,
            most = super::MOST_BYTES
        ));
    }
    if let Some((at, kept_size, said)) = digests().lock().unwrap().get(path)
        && mtime.is_some()
        && *at == mtime
        && *kept_size == size
    {
        return Ok(said.clone());
    }
    let bytes = std::fs::read(path).map_err(|_| unreadable())?;
    let said = super::symbols::digest_of(&bytes, [(0, bytes.len())]);
    let mut held = digests().lock().unwrap();
    if held.len() >= MOST_DIGESTS {
        held.clear();
    }
    held.insert(path.to_path_buf(), (mtime, size, said.clone()));
    Ok(said)
}

/// How many files were parsed in this process.
#[cfg(test)]
pub fn parsed() -> u64 {
    kept().lock().unwrap().parsed
}

/// Many files read side by side, each as `load` reads it (without its text): one parser for each
/// thread, as many threads as the machine has cores, up to 8.
pub fn load_many(files: &[(PathBuf, &'static Language, String)]) -> Vec<Result<Loaded, Keyed>> {
    if files.len() <= 1 {
        return files.iter().map(|(p, l, d)| load(p, l, d, false)).collect();
    }
    let threads = std::thread::available_parallelism()
        .map_or(4, |n| n.get())
        .clamp(1, 8)
        .min(files.len());
    let next = std::sync::atomic::AtomicUsize::new(0);
    let out: Vec<Mutex<Option<Result<Loaded, Keyed>>>> =
        files.iter().map(|_| Mutex::new(None)).collect();
    std::thread::scope(|scope| {
        for _ in 0..threads {
            scope.spawn(|| {
                loop {
                    let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    let Some((p, l, d)) = files.get(i) else {
                        break;
                    };
                    *out[i].lock().unwrap() = Some(load(p, l, d, false));
                }
            });
        }
    });
    out.into_iter()
        .map(|m| m.into_inner().unwrap().expect("each file is read"))
        .collect()
}
