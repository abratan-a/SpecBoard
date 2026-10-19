//! A working directory's file as code: where it is on this machine and in which language it is read
//! (`Dirs`, made with a reading of the working directories), and what reading it came to now
//! (`Outline`).

use super::cache::{self, Loaded};
use super::registry::{Language, Parsers, language_of};
use crate::text::Keyed;
use rusqlite::Connection;
use std::collections::HashMap;
use std::path::PathBuf;

/// One working directory as its files are read as code: the folder it is read from on this machine
/// (none when it is bound to none), and its own mapping of files to languages.
#[derive(Clone, Debug, Default)]
pub struct DirCode {
    pub folder: Option<String>,
    pub parsers: Parsers,
}

/// What a project's working directories say of their files as code, by each one's id.
#[derive(Clone, Debug, Default)]
pub struct Dirs {
    by_dir: HashMap<String, DirCode>,
}

/// A file's language: none, one this machine has, or one its working directory names that this
/// machine does not have.
#[derive(Clone, Copy, Debug)]
pub enum Lang {
    None,
    Known(&'static Language),
    Missing,
}

impl Dirs {
    pub fn of(conn: &Connection, project: &str) -> Result<Dirs, String> {
        let dirs = crate::tools::directories::directories(conn, project)?;
        let folders = crate::tools::directories::folders(conn, project)?;
        Ok(Dirs {
            by_dir: dirs
                .into_iter()
                .map(|d| {
                    let code = DirCode {
                        folder: folders.get(&d.id).cloned(),
                        parsers: d.parsers,
                    };
                    (d.id, code)
                })
                .collect(),
        })
    }

    /// The language of a file of the working directory `dir` at `path`, and its id as said (one
    /// this machine lacks too).
    pub fn language(&self, dir: &str, path: &str) -> (Lang, Option<String>) {
        let empty = Parsers::new();
        let parsers = self.by_dir.get(dir).map_or(&empty, |d| &d.parsers);
        match language_of(path, parsers) {
            Ok(None) => (Lang::None, None),
            Ok(Some(l)) => (Lang::Known(l), Some(l.id.clone())),
            Err(id) => (Lang::Missing, Some(id)),
        }
    }

    /// Where a file really is on this machine, by its id (`workdir::real_file`): refused when its
    /// working directory is bound to no folder here, or when it leads outside the folder.
    pub fn absolute(&self, file_id: &str) -> Result<PathBuf, Keyed> {
        let unbound = || keyed!("workdir.read.unbound");
        let (dir, path) = crate::workdir::of_file_id(file_id).ok_or_else(unbound)?;
        let folder = self
            .by_dir
            .get(dir)
            .and_then(|d| d.folder.as_deref())
            .ok_or_else(unbound)?;
        crate::workdir::real_file(folder, path)
    }

    /// A file read for its functions and types now, by its id: from what is kept when the file is
    /// as it was; with its text when `with_text`.
    pub fn outline(&self, file_id: &str, with_text: bool) -> Outline {
        let Some((dir, path)) = crate::workdir::of_file_id(file_id) else {
            return Outline::none(None);
        };
        let (lang, id) = self.language(dir, path);
        let language = match lang {
            Lang::None => return Outline::none(None),
            Lang::Missing => {
                let id = id.unwrap_or_default();
                return Outline::problem(Some(id.clone()), keyed!("code.noParser", language = id));
            }
            Lang::Known(l) => l,
        };
        let abs = match self.absolute(file_id) {
            Ok(abs) => abs,
            Err(why) => {
                return Outline {
                    found: Some(language),
                    ..Outline::problem(id, why)
                };
            }
        };
        if !language.has_symbols() && !with_text {
            return Outline {
                found: Some(language),
                ..Outline::problem(id, keyed!("code.noSymbols", language = language.name))
            };
        }
        match cache::load(&abs, language, dir, with_text) {
            Ok(loaded) => Outline {
                language: id,
                found: Some(language),
                problem: loaded.read.problem.clone(),
                loaded: Some(loaded),
            },
            Err(why) => Outline {
                found: Some(language),
                ..Outline::problem(id, why)
            },
        }
    }

    /// Many files read for their functions and types, side by side (`cache::load_many`): those of
    /// no language or none here said so without being read.
    pub fn outlines(&self, file_ids: &[String]) -> Vec<Outline> {
        let mut out: Vec<Option<Outline>> = Vec::with_capacity(file_ids.len());
        let mut wanted: Vec<(usize, PathBuf, &'static Language, String)> = Vec::new();
        for (i, id) in file_ids.iter().enumerate() {
            let Some((dir, path)) = crate::workdir::of_file_id(id) else {
                out.push(Some(Outline::none(None)));
                continue;
            };
            match self.language(dir, path) {
                (Lang::Known(l), Some(lid)) if l.has_symbols() => match self.absolute(id) {
                    Ok(abs) => {
                        out.push(None);
                        wanted.push((i, abs, l, dir.to_string()));
                        let _ = lid;
                    }
                    Err(why) => out.push(Some(Outline::problem(Some(l.id.clone()), why))),
                },
                _ => out.push(Some(self.outline(id, false))),
            }
        }
        let files: Vec<(PathBuf, &'static Language, String)> = wanted
            .iter()
            .map(|(_, p, l, d)| (p.clone(), *l, d.clone()))
            .collect();
        for ((i, _, language, _), got) in wanted.iter().zip(cache::load_many(&files)) {
            out[*i] = Some(match got {
                Ok(loaded) => Outline {
                    language: Some(language.id.clone()),
                    found: Some(*language),
                    problem: loaded.read.problem.clone(),
                    loaded: Some(loaded),
                },
                Err(why) => Outline::problem(Some(language.id.clone()), why),
            });
        }
        out.into_iter()
            .map(|o| o.expect("each file said"))
            .collect()
    }
}

impl Dirs {
    /// Whether any working directory is bound to a folder on this machine: where none is, no file
    /// of the project is read here.
    pub fn any_bound(&self) -> bool {
        self.by_dir.values().any(|d| d.folder.is_some())
    }

    /// The fingerprint of what each line to a file lands on, now (`symbols::digest_of`): the
    /// function or type at its path in the file (`Some`, its names from the file's top as a path
    /// writes them), or the whole file (`None`); or why it is not read on this machine. Each file
    /// is read once, side by side (`outlines`), and as kept while it is as it was.
    pub fn digests(&self, wanted: &[(String, Option<String>)]) -> Vec<Result<String, Keyed>> {
        let mut files: Vec<String> = wanted
            .iter()
            .filter(|(_, at)| at.is_some())
            .map(|(f, _)| f.clone())
            .collect();
        files.sort();
        files.dedup();
        let outlines: HashMap<String, Outline> =
            files.iter().cloned().zip(self.outlines(&files)).collect();
        wanted
            .iter()
            .map(|(file, at)| {
                let abs = self.absolute(file)?;
                let Some(at) = at else {
                    return cache::digest(&abs);
                };
                let outline = &outlines[file];
                if !outline.read_whole() {
                    return Err(match &outline.problem {
                        Some(why) => why.clone(),
                        None if outline.loaded.is_none() && outline.found.is_none() => {
                            keyed!("code.symbol.noLanguage")
                        }
                        None => keyed!("code.unreadable", path = abs.display()),
                    });
                }
                let names = crate::pathtext::names(at);
                match find(outline.symbols(), &names) {
                    Some(symbol) => Ok(symbol.digest.clone()),
                    None => {
                        let near = nearest(outline.symbols(), at, 3);
                        Err(keyed!(
                            "code.symbol.missingHere",
                            symbol = at.clone(),
                            near = if near.is_empty() {
                                text!("code.symbol.noneNear")
                            } else {
                                crate::text::list(&near)
                            }
                        ))
                    }
                }
            })
            .collect()
    }
}

/// What reading one file for its functions and types came to.
#[derive(Clone)]
pub struct Outline {
    /// Its language's id, when it has one (one this machine lacks too).
    pub language: Option<String>,
    pub found: Option<&'static Language>,
    /// What was read, when it was.
    pub loaded: Option<Loaded>,
    /// Why it has no functions and types, or not all of them.
    pub problem: Option<Keyed>,
}

impl Outline {
    fn none(language: Option<String>) -> Outline {
        Outline {
            language,
            found: None,
            loaded: None,
            problem: None,
        }
    }

    fn problem(language: Option<String>, why: Keyed) -> Outline {
        Outline {
            language,
            found: None,
            loaded: None,
            problem: Some(why),
        }
    }

    /// The functions and types read, in the file's order; none when it was not read.
    pub fn symbols(&self) -> &[super::symbols::Symbol] {
        self.loaded
            .as_ref()
            .map_or(&[], |l| l.read.found.symbols.as_slice())
    }

    pub fn parse_errors(&self) -> usize {
        self.loaded
            .as_ref()
            .map_or(0, |l| l.read.found.parse_errors)
    }

    /// Whether it was read for its functions and types: a symbol not among them is not there.
    pub fn read_whole(&self) -> bool {
        self.loaded.as_ref().is_some_and(|l| {
            l.read.problem.is_none() && self.found.is_some_and(|f| f.has_symbols())
        })
    }

    /// The file's text, when it was read with it.
    pub fn text(&self) -> Option<&str> {
        self.loaded
            .as_ref()
            .and_then(|l| l.text.as_deref())
            .and_then(|b| std::str::from_utf8(b).ok())
    }
}

/// A symbol among a file's, by its names from the file's top as a path writes them unquoted: the
/// same names first, then the one whose names differ in case alone.
pub fn find<'s>(
    symbols: &'s [super::symbols::Symbol],
    names: &[String],
) -> Option<&'s super::symbols::Symbol> {
    let wanted = names
        .iter()
        .map(|n| crate::pathtext::quote(n).into_owned())
        .collect::<Vec<_>>()
        .join(".");
    symbols.iter().find(|s| s.path == wanted).or_else(|| {
        let mut alike = symbols
            .iter()
            .filter(|s| s.path.eq_ignore_ascii_case(&wanted));
        let one = alike.next()?;
        alike.next().is_none().then_some(one)
    })
}

/// The paths of a file's symbols nearest to `wanted` (a path), nearest first: what to write
/// instead of one not there.
pub fn nearest(symbols: &[super::symbols::Symbol], wanted: &str, most: usize) -> Vec<String> {
    let wanted_lower = wanted.to_lowercase();
    let last = wanted_lower.rsplit('.').next().unwrap_or(&wanted_lower);
    let mut scored: Vec<(usize, &str)> = symbols
        .iter()
        .map(|s| {
            let lower = s.path.to_lowercase();
            let own = lower.rsplit('.').next().unwrap_or(&lower);
            let d = distance(&lower, &wanted_lower).min(distance(own, last) + 1);
            (d, s.path.as_str())
        })
        .collect();
    scored.sort();
    let limit = (wanted.chars().count() / 2).max(3);
    let mut out: Vec<String> = scored
        .iter()
        .filter(|(d, _)| *d <= limit)
        .take(most)
        .map(|(_, p)| p.to_string())
        .collect();
    if out.is_empty() {
        // Those beside where it would be: its parent's members, or the file's top.
        let parent = wanted.rsplit_once('.').map(|(p, _)| p);
        out = symbols
            .iter()
            .filter(|s| match parent {
                Some(p) => s.path.rsplit_once('.').map(|(q, _)| q) == Some(p),
                None => s.parent.is_none(),
            })
            .take(most)
            .map(|s| s.path.clone())
            .collect();
    }
    out
}

/// How many characters one has to change to make `a` into `b`.
fn distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut prev = row[0];
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let here = row[j + 1];
            row[j + 1] = if ca == *cb {
                prev
            } else {
                1 + prev.min(row[j]).min(row[j + 1])
            };
            prev = here;
        }
    }
    row[b.len()]
}
