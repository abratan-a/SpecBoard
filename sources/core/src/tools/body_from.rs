//! A body read from a file (`bodyFrom: {directory, path}` in `card_create`'s and `card_update`'s
//! value — at its top, in a formatdoc's text rows and in a stackdoc's versions —,
//! `specboard://SpecBoard/Docs/Design:CoreApi`「本文をファイルから取り込む」): the text of a
//! file in one of the project's working directories, from its folder on this machine, written as
//! the body. The card holds it from then on, tied to nothing: what is in the file changes outside
//! the history, the branches and the merges, and the card does not. The same read serves `batch`'s
//! `stepsFrom`.
//!
//! Only a file inside the working directory's folder is read — a path out of it (`..`, from the
//! top, a link to somewhere else) is refused — so the write tools' own items allow it, and no item
//! of its own is asked.

use rusqlite::Connection;
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// The most a body holds, in characters: the kinds' bodies' `max`.
const MOST: usize = 200_000;

/// The most a file may be to be read, in bytes: `MOST` characters of four bytes each, so a file
/// past it is refused before it is read.
const MOST_BYTES: u64 = MOST as u64 * 4;

/// The most files may be, together, in one call: what a call could carry inline.
pub(crate) const MOST_CALL_BYTES: u64 = crate::MAX_REQUEST_BYTES as u64;

/// How much of files one call has read: held to `MOST_CALL_BYTES`, so many files of a size each
/// may be read never add up to more than a call could carry.
#[derive(Default)]
pub(crate) struct Budget {
    used: u64,
}

impl Budget {
    /// `bytes` more read, unless that is past the call's most.
    fn take(&mut self, bytes: u64) -> Result<(), String> {
        let total = self.used + bytes;
        if total > MOST_CALL_BYTES {
            return Err(text!(
                "writes.bodyFrom.callTooBig",
                most = MOST_CALL_BYTES,
                bytes = total
            ));
        }
        self.used = total;
        Ok(())
    }
}

/// Where a `bodyFrom` may stand in the value of one target: on the target itself (when it has a
/// body), in its `rows` (a formatdoc's) and in its `versions` (a stackdoc's).
#[derive(Clone, Copy, Default)]
pub(crate) struct Holds {
    /// Whether the target takes a body; none when it cannot be told (a batch step is received
    /// before the steps ahead of it have made what it writes to).
    pub body: Option<bool>,
    pub rows: bool,
    pub versions: bool,
}

/// What was read for one `bodyFrom`: the rows and versions it stood in (by name, from the target
/// down; none for the target itself), the working directory and path it names, the characters
/// written.
#[derive(Clone)]
pub(crate) struct Read {
    pub names: Vec<String>,
    pub directory: String,
    pub path: String,
    pub chars: usize,
}

impl Read {
    /// The answer's entry: `at` is the full path of what got the body, from the target's `full`.
    pub fn said(&self, full: &str) -> Value {
        let mut at = full.to_string();
        for name in &self.names {
            at.push('.');
            at.push_str(&crate::pathtext::quote(name));
        }
        let mut said = Map::new();
        said.insert("at".into(), at.into());
        said.insert("directory".into(), self.directory.clone().into());
        said.insert("path".into(), self.path.clone().into());
        said.insert("chars".into(), self.chars.into());
        Value::Object(said)
    }
}

/// Whether a `bodyFrom` stands anywhere in `value`: on it, or in the rows and versions it holds.
pub(crate) fn wanted(value: &Map<String, Value>) -> bool {
    value.contains_key("bodyFrom")
        || ["rows", "versions"].iter().any(|key| {
            value
                .get(*key)
                .and_then(Value::as_array)
                .is_some_and(|items| items.iter().filter_map(Value::as_object).any(wanted))
        })
}

/// Which place of the target a map is.
#[derive(Clone, Copy)]
enum Spot {
    Top(Holds),
    Row,
    Version,
}

/// The files one call reads: the folders of the working directories asked for once, and all of it
/// held to the call's `Budget`.
pub(crate) struct Files<'a> {
    conn: &'a Connection,
    project: &'a str,
    budget: &'a mut Budget,
    folders: HashMap<String, (String, String)>,
}

impl<'a> Files<'a> {
    pub fn new(conn: &'a Connection, project: &'a str, budget: &'a mut Budget) -> Files<'a> {
        Files {
            conn,
            project,
            budget,
            folders: HashMap::new(),
        }
    }

    /// `value` with every `bodyFrom` in it read into `body` — on the target (`at`, as the target is
    /// written) where `holds` says it has a body, in each of its rows and versions — and what was
    /// read; none when it has none. Every one is tried and all that is wrong is said together, each
    /// with its place; `value` is not to be used when any is. A place with no body to give is refused
    /// before its file is read.
    pub fn take(
        &mut self,
        at: &str,
        holds: Holds,
        value: &mut Map<String, Value>,
    ) -> Result<Vec<Read>, Vec<String>> {
        let (mut reads, mut errors) = (Vec::new(), Vec::new());
        self.visit(
            Spot::Top(holds),
            "value",
            at,
            value,
            &mut Vec::new(),
            &mut reads,
            &mut errors,
        );
        if errors.is_empty() {
            Ok(reads)
        } else {
            Err(errors)
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn visit(
        &mut self,
        spot: Spot,
        place: &str,
        at: &str,
        map: &mut Map<String, Value>,
        names: &mut Vec<String>,
        reads: &mut Vec<Read>,
        errors: &mut Vec<String>,
    ) {
        let nested = !matches!(spot, Spot::Top(_));
        if let Some(given) = map.get("bodyFrom") {
            let has_body = match spot {
                Spot::Top(holds) => holds.body != Some(false),
                Spot::Row => map.get("kind").and_then(Value::as_str) == Some("text"),
                Spot::Version => true,
            };
            // Said in the place's words, which a prefix would only repeat.
            let said = |why: String| {
                if nested {
                    format!("{place}.bodyFrom: {why}")
                } else {
                    why
                }
            };
            if !has_body {
                errors.push(text!(
                    "writes.bodyFrom.noBody",
                    at = if nested { place } else { at }
                ));
            } else if map.contains_key("body") {
                errors.push(said(text!("writes.bodyFrom.withBody")));
            } else {
                match self.fetch(given) {
                    Ok((directory, path, text)) => {
                        let chars = text.chars().count();
                        map.shift_remove("bodyFrom");
                        map.insert("body".into(), text.into());
                        reads.push(Read {
                            names: names.clone(),
                            directory,
                            path,
                            chars,
                        });
                    }
                    Err(why) => errors.push(said(why)),
                }
            }
        }
        let (rows, versions) = match spot {
            Spot::Top(holds) => (holds.rows, holds.versions),
            Spot::Row => (true, false),
            Spot::Version => (false, false),
        };
        for (key, inside, wanted) in [
            ("rows", Spot::Row, rows),
            ("versions", Spot::Version, versions),
        ] {
            if !wanted {
                continue;
            }
            let Some(Value::Array(items)) = map.get_mut(key) else {
                continue;
            };
            for (index, item) in items.iter_mut().enumerate() {
                let Value::Object(item) = item else {
                    continue;
                };
                names.push(
                    item.get("name")
                        .and_then(Value::as_str)
                        .map_or_else(|| index.to_string(), str::to_string),
                );
                self.visit(
                    inside,
                    &format!("{place}.{key}[{index}]"),
                    at,
                    item,
                    names,
                    reads,
                    errors,
                );
                names.pop();
            }
        }
    }

    /// The working directory's name, the path and the text a `bodyFrom` names.
    fn fetch(&mut self, given: &Value) -> Result<(String, String, String), String> {
        let (directory, path) = shape(given, "bodyFrom")?;
        let (name, folder) = self.folder(&directory)?;
        let text = read(
            &folder,
            &path,
            "bodyFrom",
            MOST_BYTES,
            &|bytes| {
                text!(
                    "writes.bodyFrom.tooLong",
                    key = "bodyFrom",
                    path,
                    chars = bytes,
                    most = MOST
                )
            },
            self.budget,
        )?;
        let chars = text.chars().count();
        if chars > MOST {
            return Err(text!(
                "writes.bodyFrom.tooLong",
                key = "bodyFrom",
                path,
                chars,
                most = MOST
            ));
        }
        Ok((name, path, text))
    }

    /// The working directory `directory` names and its folder here, asked for once.
    fn folder(&mut self, directory: &str) -> Result<(String, String), String> {
        if let Some(found) = self.folders.get(directory) {
            return Ok(found.clone());
        }
        let found = folder_of(self.conn, self.project, directory)?;
        self.folders.insert(directory.to_string(), found.clone());
        Ok(found)
    }
}

/// The directory and the path of a `{directory, path}`, as `key` (`bodyFrom`, `stepsFrom`) is given.
pub(crate) fn shape(given: &Value, key: &str) -> Result<(String, String), String> {
    match given {
        Value::Object(o)
            if o.len() == 2
                && o.get("directory").is_some_and(Value::is_string)
                && o.get("path").is_some_and(Value::is_string) =>
        {
            Ok((
                o["directory"].as_str().unwrap_or_default().to_string(),
                o["path"].as_str().unwrap_or_default().to_string(),
            ))
        }
        _ => Err(text!("writes.bodyFrom.shape", key)),
    }
}

/// The name of the working directory `directory` names, and its folder on this machine.
pub(crate) fn folder_of(
    conn: &Connection,
    project: &str,
    directory: &str,
) -> Result<(String, String), String> {
    let dirs = super::directories::directories(conn, project)?;
    let dir = super::directories::named(&dirs, directory)?;
    let folders = super::directories::folders(conn, project)?;
    let Some(folder) = folders.get(&dir.id) else {
        return Err(text!("writes.bodyFrom.unbound", name = dir.name));
    };
    Ok((dir.name.clone(), folder.clone()))
}

/// The text of the file at `path` in `folder`, when it is a file inside it — the folder itself
/// and the path through it with every link followed — and UTF-8, and not past `limit` bytes
/// (`too_big` says it) nor past what the call may read in all (`budget`).
pub(crate) fn read(
    folder: &str,
    path: &str,
    key: &str,
    limit: u64,
    too_big: &dyn Fn(u64) -> String,
    budget: &mut Budget,
) -> Result<String, String> {
    let parts: Vec<&str> = path
        .split('/')
        .filter(|p| !p.is_empty() && *p != ".")
        .collect();
    if path.starts_with('/')
        || path.starts_with('~')
        || path.contains('\\')
        || path.chars().any(char::is_control)
        || parts.contains(&"..")
    {
        return Err(text!("writes.bodyFrom.outside", key, path));
    }
    if parts.is_empty() {
        return Err(text!("writes.bodyFrom.notFile", key, path));
    }
    let top = PathBuf::from(super::files::absolute(folder, ""));
    let top = top
        .canonicalize()
        .map_err(|_| text!("writes.bodyFrom.folderGone", folder))?;
    let file: PathBuf = parts.iter().fold(top.clone(), |at, p| at.join(p));
    let real = match file.canonicalize() {
        Ok(real) => real,
        Err(_) if is_link(&file) => return Err(text!("writes.bodyFrom.outside", key, path)),
        Err(_) => return Err(text!("writes.bodyFrom.missing", key, path)),
    };
    if !real.starts_with(&top) {
        return Err(text!("writes.bodyFrom.outside", key, path));
    }
    let meta = std::fs::metadata(&real).map_err(|_| text!("writes.bodyFrom.missing", key, path))?;
    if !meta.is_file() {
        return Err(text!("writes.bodyFrom.notFile", key, path));
    }
    if meta.len() > limit {
        return Err(too_big(meta.len()));
    }
    budget.take(meta.len())?;
    let bytes = std::fs::read(&real).map_err(|_| text!("writes.bodyFrom.missing", key, path))?;
    String::from_utf8(bytes).map_err(|_| text!("writes.bodyFrom.notUtf8", key, path))
}

/// Whether `path` is a link (one that leads nowhere among them).
fn is_link(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink())
}
