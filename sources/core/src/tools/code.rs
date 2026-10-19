//! The functions and types of a working directory's files as the tools read them (`code`): a file's
//! card read with its outline, a function or a type read by its path past the file's
//! (`repo/src:'a.rs'.Order.total`), listed (`card_list`'s `kind: symbol`), and the lines that land
//! at one. Nothing of it is stored: each is read from the file as it is now.

use crate::code::files::{self, Outline};
use crate::code::symbols::{Of, Symbol};
use crate::export::Project;
use crate::model::Card;
use serde_json::{Map, Value, json};

/// A file read now for its functions and types, by its card: none where the reading has no
/// working directories beside it.
pub(crate) fn outline(project: &Project, file: &Card, with_text: bool) -> Option<Outline> {
    let code = project.code.as_ref()?;
    Some(code.outline(&file.id, with_text))
}

/// The lines (from 1, both ends in it) of a span, as an answer says them.
fn lines(from: usize, to: usize) -> Value {
    json!({"from": from, "to": to})
}

/// What a file's card is read with beside what it holds (`card_read`): its language, its
/// functions and types — those at its top, each with the names of what is in it — how many
/// places could not be parsed, and why it has none. Its text only when the read asks for lines of
/// it or for a word in it (`body`, read in part as a body is).
pub(crate) fn file_said(
    project: &Project,
    file: &Card,
    given: &Map<String, Value>,
) -> Map<String, Value> {
    let mut out = Map::new();
    let asked = super::in_part::asked(given);
    let Some(read) = outline(project, file, asked) else {
        return out;
    };
    if let Some(language) = &read.language {
        out.insert("language".into(), language.clone().into());
    }
    let symbols = read.symbols();
    if read.found.is_some_and(|l| l.has_symbols()) && read.loaded.is_some() {
        let top: Vec<Value> = symbols
            .iter()
            .enumerate()
            .filter(|(_, s)| s.parent.is_none())
            .map(|(i, s)| {
                let mut row = Map::new();
                row.insert("name".into(), s.name.clone().into());
                row.insert("of".into(), s.of.word().into());
                row.insert("word".into(), s.word.clone().into());
                row.insert("line".into(), s.line.into());
                row.insert("lines".into(), lines(s.span.from, s.span.to));
                let members: Vec<Value> = symbols
                    .iter()
                    .filter(|m| m.parent == Some(i))
                    .map(|m| m.name.clone().into())
                    .collect();
                if !members.is_empty() {
                    row.insert("members".into(), Value::Array(members));
                }
                Value::Object(row)
            })
            .collect();
        out.insert("symbols".into(), Value::Array(top));
    }
    if read.parse_errors() > 0 {
        out.insert("parseErrors".into(), read.parse_errors().into());
    }
    // A language that colours alone is not a problem of the file's.
    let colours_alone = read.found.is_some_and(|l| !l.has_symbols());
    if let Some(why) = read.problem.as_ref().filter(|_| !colours_alone) {
        out.insert("problem".into(), why.render().into());
    }
    if asked {
        match read.text() {
            Some(text) => {
                out.insert("body".into(), text.into());
            }
            None => match whole_text(project, file) {
                Some(Ok(text)) => {
                    out.insert("body".into(), text.into());
                }
                Some(Err(why)) => {
                    out.insert("problem".into(), why.render().into());
                }
                None => {}
            },
        }
    }
    out
}

/// A file's text read whole, as it is now: one of no language, or one too large to be parsed; or
/// why it is not read (`code::files::Dirs::absolute`).
fn whole_text(project: &Project, file: &Card) -> Option<Result<String, crate::text::Keyed>> {
    let abs = match project.code.as_ref()?.absolute(&file.id) {
        Ok(abs) => abs,
        Err(why) => return Some(Err(why)),
    };
    let bytes = std::fs::read(abs).ok()?;
    String::from_utf8(bytes).ok().map(Ok)
}

/// The text of a symbol: its span's; for one made of blocks alone, the blocks in the file's order
/// with a blank line between.
pub(crate) fn body_of(text: &str, symbol: &Symbol) -> String {
    let cut = |start: usize, end: usize| text.get(start..end).unwrap_or_default();
    if symbol.made {
        symbol
            .parts
            .iter()
            .map(|p| cut(p.start, p.end))
            .collect::<Vec<_>>()
            .join("\n\n")
    } else {
        cut(symbol.span.start, symbol.span.end).to_string()
    }
}

/// The line its name is on, as written there: what declares it.
fn declaration(text: &str, starts: &[usize], line: usize) -> String {
    let from = starts.get(line - 1).copied().unwrap_or(0);
    let to = starts.get(line).copied().unwrap_or(text.len());
    text.get(from..to).unwrap_or_default().trim().to_string()
}

/// A symbol's path in full: its file's, then its own.
pub(crate) fn full_path(project: &Project, file: &Card, symbol_path: &str) -> String {
    format!("{}.{symbol_path}", project.full(file))
}

/// The names of a symbol path as a target walks them, quoted where a path quotes them.
fn written(names: &[String]) -> String {
    names
        .iter()
        .map(|n| crate::pathtext::quote(n).into_owned())
        .collect::<Vec<_>>()
        .join(".")
}

/// Why a symbol is not read: its file not read for its functions and types, or none of that path
/// in it — said with the names nearest it there.
fn missing(project: &Project, file: &Card, names: &[String], read: &Outline) -> String {
    let file_path = project.full(file);
    let wanted = written(names);
    if !read.read_whole() {
        let why = read
            .problem
            .as_ref()
            .map(|k| k.render())
            .unwrap_or_else(|| text!("code.symbol.noLanguage"));
        return text!("code.symbol.unread", file = file_path, why);
    }
    let near = files::nearest(read.symbols(), &wanted, 5);
    text!(
        "code.symbol.missing",
        file = file_path,
        symbol = wanted,
        near = if near.is_empty() {
            text!("code.symbol.noneNear")
        } else {
            crate::text::list(&near)
        }
    )
}

/// The cards whose lines land at a file — at a symbol in it when `at` is said, and at what is in that
/// one too —: each by its path (its row's when the line leaves one), with where it lands when it
/// lands inside the file and not at `at` itself.
pub(crate) fn landing_here(project: &Project, file_id: &str, at: Option<&str>) -> Vec<Value> {
    let detail = project.detail;
    detail
        .schemas
        .iter()
        .chain(detail.inner.iter())
        .flat_map(|c| {
            c.connections
                .iter()
                .filter(|l| l.line == "implementedBy" && l.to.as_deref() == Some(file_id))
                .filter(|l| match at {
                    None => true,
                    Some(at) => l.at.as_deref().is_some_and(|a| {
                        a == at || a.strip_prefix(at).is_some_and(|r| r.starts_with('.'))
                    }),
                })
                .map(|l| {
                    let full = project.full(c);
                    let from = if l.from.is_empty() {
                        full
                    } else {
                        format!("{full}.{}", l.from)
                    };
                    // Where it lands, when that is not what is read itself.
                    match l.at.as_deref().filter(|a| !a.is_empty() && Some(*a) != at) {
                        Some(landing) => json!({"path": from, "at": landing}),
                        None => Value::from(from),
                    }
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

/// A function or a type read (`card_read` of `repo/src:'a.rs'.Order.total`): what it is, where it is
/// in its file (its lines, the line its name is on, as written there), its text, what is in it,
/// the names it calls, and the cards that land at it. Its lines and a word in it are read as a
/// body's are, by the file's line numbers. Refused when it is not in the file now, with the names
/// nearest it there.
pub(crate) fn symbol_read(
    project: &Project,
    project_name: &str,
    file: &Card,
    names: &[String],
    given: &Map<String, Value>,
    most: Option<usize>,
) -> Result<Value, String> {
    let read = outline(project, file, true).ok_or_else(|| text!("code.symbol.noDirs"))?;
    let Some(symbol) = files::find(read.symbols(), names) else {
        return Err(missing(project, file, names, &read));
    };
    let text = read.text().unwrap_or_default();
    let starts = read
        .loaded
        .as_ref()
        .map(|l| l.read.line_starts.clone())
        .unwrap_or_default();
    let all = read.symbols();
    let index = all
        .iter()
        .position(|s| std::ptr::eq(s, symbol))
        .unwrap_or(0);
    let mut out = Map::new();
    out.insert("project".into(), project_name.into());
    out.insert("path".into(), full_path(project, file, &symbol.path).into());
    out.insert("kind".into(), "symbol".into());
    out.insert("of".into(), symbol.of.word().into());
    out.insert("word".into(), symbol.word.clone().into());
    if let Some(language) = &read.language {
        out.insert("language".into(), language.clone().into());
    }
    out.insert("file".into(), project.full(file).into());
    out.insert("line".into(), symbol.line.into());
    out.insert("lines".into(), lines(symbol.span.from, symbol.span.to));
    out.insert(
        "declaration".into(),
        declaration(text, &starts, symbol.line).into(),
    );
    if !symbol.parts.is_empty() {
        out.insert(
            "parts".into(),
            Value::Array(symbol.parts.iter().map(|p| lines(p.from, p.to)).collect()),
        );
    }
    if symbol.of.holds() {
        let members: Vec<Value> = all
            .iter()
            .filter(|m| m.parent == Some(index))
            .map(|m| m.name.clone().into())
            .collect();
        if !members.is_empty() {
            out.insert("members".into(), Value::Array(members));
        }
    }
    if !symbol.calls.is_empty() {
        out.insert("calls".into(), json!(symbol.calls));
    }
    let landing = landing_here(project, &file.id, Some(&symbol.path));
    if !landing.is_empty() {
        out.insert("referencedBy".into(), Value::Array(landing));
    }
    // Its lines and a word in it, by the file's numbers: over the file's lines it spans (one made of
    // blocks alone, from its first to its last).
    let asked = super::in_part::asked(given);
    let body = if asked {
        let from = starts.get(symbol.span.from - 1).copied().unwrap_or(0);
        let to = starts.get(symbol.span.to).copied().unwrap_or(text.len());
        text.get(from..to)
            .unwrap_or_default()
            .trim_end_matches('\n')
            .to_string()
    } else {
        body_of(text, symbol)
    };
    out.insert("body".into(), body.into());
    let mut said = Value::Object(out);
    super::in_part::read_at(&mut said, given, most, symbol.span.from)?;
    Ok(said)
}

/// A symbol as `card_list` lists it.
fn listed(project: &Project, file: &Card, symbol: &Symbol, language: Option<&str>) -> Value {
    let mut row = Map::new();
    row.insert("path".into(), full_path(project, file, &symbol.path).into());
    row.insert("kind".into(), "symbol".into());
    row.insert("of".into(), symbol.of.word().into());
    row.insert("word".into(), symbol.word.clone().into());
    if let Some(language) = language {
        row.insert("language".into(), language.into());
    }
    row.insert("line".into(), symbol.line.into());
    row.insert("lines".into(), lines(symbol.span.from, symbol.span.to));
    Value::Object(row)
}

/// What `card_list` lists with `kind: symbol`: the functions and types of the files given (each
/// file read now, side by side), those of `of` alone and those whose name has `name` in it (in any
/// case) — with why a file could not be read, and how many files were left out past the most read.
pub(crate) fn listed_symbols(
    project: &Project,
    files: &[&Card],
    of: Option<Of>,
    name: Option<&str>,
    landing: Option<&super::coverage::Landing>,
) -> (Vec<Value>, Vec<String>) {
    let Some(code) = project.code.as_ref() else {
        return (Vec::new(), Vec::new());
    };
    let mut warnings = Vec::new();
    let mut shown = files;
    if files.len() > crate::workdir::LIMIT {
        warnings.push(text!("code.list.overLimit", limit = crate::workdir::LIMIT));
        shown = &files[..crate::workdir::LIMIT];
    }
    let ids: Vec<String> = shown.iter().map(|f| f.id.clone()).collect();
    let wanted = name.map(str::to_lowercase);
    let mut out = Vec::new();
    for (file, read) in shown.iter().zip(code.outlines(&ids)) {
        if read.found.is_some_and(|l| l.has_symbols())
            && let Some(why) = &read.problem
        {
            warnings.push(format!("{}: {}", project.full(file), why.render()));
        }
        for symbol in read.symbols() {
            if of.is_some_and(|o| o != symbol.of) {
                continue;
            }
            if let Some(w) = &wanted
                && !symbol.name.to_lowercase().contains(w.as_str())
            {
                continue;
            }
            let mut row = listed(project, file, symbol, read.language.as_deref());
            // By the lines that land at it (`card_list`'s `implementation`).
            if let Some(landing) = landing {
                let Some(extra) = landing.symbol(&file.id, &symbol.path) else {
                    continue;
                };
                if let Value::Object(object) = &mut row {
                    object.extend(extra);
                }
            }
            out.push(row);
        }
    }
    (out, warnings)
}

/// The most callers an impact names.
const MOST_CALLERS: usize = 200;

/// What a change of a function or a type reaches (`card_read`'s `as: impact`): the cards that say
/// they are carried out in it (`implementedBy`), and the functions and types of its working
/// directory that call it — by its name alone, so a guess (`approx`).
pub(crate) fn symbol_impact(
    project: &Project,
    project_name: &str,
    file: &Card,
    names: &[String],
) -> Result<Value, String> {
    let read = outline(project, file, false).ok_or_else(|| text!("code.symbol.noDirs"))?;
    let Some(symbol) = files::find(read.symbols(), names) else {
        return Err(missing(project, file, names, &read));
    };
    let mut out = Map::new();
    out.insert("project".into(), project_name.into());
    out.insert("path".into(), full_path(project, file, &symbol.path).into());
    out.insert(
        "implementedBy".into(),
        Value::Array(landing_here(project, &file.id, Some(&symbol.path))),
    );
    // Called by its own name: the identifier, past a scope (`Order::total`) and without the number
    // a name of the same beside it is given.
    let own = symbol
        .name
        .rsplit_once(" (")
        .map_or(symbol.name.as_str(), |(n, _)| n);
    let own = own.rsplit("::").next().unwrap_or(own).to_string();
    let (callers, cut, warnings) = callers_of(project, file, symbol, &own);
    out.insert(
        "callers".into(),
        Value::Array(
            callers
                .into_iter()
                .map(|p| json!({"path": p, "approx": true}))
                .collect(),
        ),
    );
    if cut {
        out.insert("callersTruncated".into(), true.into());
    }
    Ok(super::warned(Value::Object(out), &warnings))
}

/// The functions and types of `file`'s working directory that call `name` (but `symbol` itself), by
/// their paths in full: the directory's files read now, side by side.
fn callers_of(
    project: &Project,
    file: &Card,
    symbol: &Symbol,
    name: &str,
) -> (Vec<String>, bool, Vec<String>) {
    let Some(code) = project.code.as_ref() else {
        return (Vec::new(), false, Vec::new());
    };
    let Some((dir, _)) = crate::workdir::of_file_id(&file.id) else {
        return (Vec::new(), false, Vec::new());
    };
    let prefix = format!("file:{dir}/");
    let tree = project.files.as_deref().unwrap_or(project);
    let mut cards: Vec<&Card> = tree
        .detail
        .schemas
        .iter()
        .map(|c| &**c)
        .filter(|c| c.kind == "file" && c.id.starts_with(&prefix))
        .collect();
    let mut warnings = Vec::new();
    if cards.len() > crate::workdir::LIMIT {
        warnings.push(text!("code.list.overLimit", limit = crate::workdir::LIMIT));
        cards.truncate(crate::workdir::LIMIT);
    }
    let ids: Vec<String> = cards.iter().map(|c| c.id.clone()).collect();
    let mut out = Vec::new();
    let mut cut = false;
    for (card, read) in cards.iter().zip(code.outlines(&ids)) {
        for s in read.symbols() {
            if card.id == file.id && s.path == symbol.path {
                continue;
            }
            if s.calls.iter().any(|c| c == name) {
                if out.len() == MOST_CALLERS {
                    cut = true;
                    break;
                }
                out.push(full_path(project, card, &s.path));
            }
        }
    }
    (out, cut, warnings)
}

/// Where a line into a file lands, as an impact's row says it: the function or type (`symbol`) and
/// its lines now, when it is there.
pub(crate) fn landing_lines(project: &Project, file_id: &str, at: &str) -> Option<Value> {
    let read = project.code.as_ref()?.outline(file_id, false);
    let names = crate::pathtext::names(at);
    let symbol = files::find(read.symbols(), &names)?;
    Some(lines(symbol.span.from, symbol.span.to))
}

// MARK: - The window's requests

/// A working directory's file shown by the window (`file_text`), with its text coloured
/// (`highlight`: `language`, `classes`, `spans` as `[utf16From, utf16Len, class, …]` over the text
/// given) and where a function or a type in it is (`symbol`: `focus` with its lines and the line its
/// name is on, or why not, with the names nearest it).
pub(crate) fn file_text(
    conn: &rusqlite::Connection,
    project: &str,
    id: &str,
    request: &Value,
) -> Result<Value, String> {
    let mut out = crate::workdir::text(conn, project, id)?;
    let highlight = request.get("highlight").and_then(Value::as_bool) == Some(true);
    let symbol = request.get("symbol").and_then(Value::as_str);
    if !highlight && symbol.is_none() {
        return Ok(out);
    }
    let dirs = crate::code::files::Dirs::of(conn, project)?;
    let Some((dir, path)) = crate::workdir::of_file_id(id) else {
        return Ok(out);
    };
    let (lang, language_id) = dirs.language(dir, path);
    if let Some(language) = &language_id {
        out["language"] = language.clone().into();
    }
    if highlight
        && let crate::code::files::Lang::Known(language) = lang
        && let Some(text) = out.get("text").and_then(Value::as_str)
    {
        match crate::code::highlight::spans(language, text) {
            Some(Ok(spans)) => {
                out["classes"] = json!(crate::code::highlight::CLASSES);
                out["spans"] = json!(spans);
            }
            Some(Err(why)) => out["highlightProblem"] = why.render().into(),
            None => {}
        }
    }
    if let Some(symbol) = symbol {
        let read = dirs.outline(id, false);
        let names = crate::pathtext::names(symbol);
        match files::find(read.symbols(), &names) {
            Some(s) => {
                out["focus"] =
                    json!({"from": s.span.from, "to": s.span.to, "line": s.line, "path": s.path});
            }
            None => {
                let near = files::nearest(read.symbols(), symbol, 5);
                out["focusProblem"] = match &read.problem {
                    Some(why) if !read.read_whole() => why.render(),
                    _ => text!(
                        "code.symbol.missingHere",
                        symbol,
                        near = if near.is_empty() {
                            text!("code.symbol.noneNear")
                        } else {
                            crate::text::list(&near)
                        }
                    ),
                }
                .into();
                out["near"] = json!(near);
            }
        }
    }
    Ok(out)
}

/// The functions and types of the files the window shows (`file_outline`), each file read now,
/// side by side: its language, a stamp of its text and language (`stamp`), how many places could
/// not be parsed, why it has none, and each function or type in the file's order with the one it is
/// in (`parent`, by path). A file whose stamp is the one the window holds (`since`) is answered
/// `unchanged`, without them.
pub(crate) fn file_outline(
    conn: &rusqlite::Connection,
    project: &str,
    request: &Value,
) -> Result<Value, String> {
    let ids: Vec<String> = request
        .get("ids")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            text!(
                "ops.error.needs",
                op = "file_outline",
                key = "ids",
                what = text!("screen.what.fileIds")
            )
        })?
        .iter()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect();
    let since = request.get("since").and_then(Value::as_object);
    let dirs = crate::code::files::Dirs::of(conn, project)?;
    let files: Vec<Value> = ids
        .iter()
        .zip(dirs.outlines(&ids))
        .map(|(id, read)| {
            let mut row = Map::new();
            row.insert("id".into(), id.clone().into());
            if let Some(language) = &read.language {
                row.insert("language".into(), language.clone().into());
            }
            let stamp = read.loaded.as_ref().map(|l| l.stamp.clone());
            if let Some(stamp) = &stamp {
                row.insert("stamp".into(), stamp.clone().into());
            }
            let colours_alone = read.found.is_some_and(|l| !l.has_symbols());
            if let Some(why) = read.problem.as_ref().filter(|_| !colours_alone) {
                row.insert("problem".into(), why.render().into());
            }
            if stamp.is_some()
                && since.and_then(|s| s.get(id)).and_then(Value::as_str) == stamp.as_deref()
            {
                row.insert("unchanged".into(), true.into());
                return Value::Object(row);
            }
            if read.parse_errors() > 0 {
                row.insert("parseErrors".into(), read.parse_errors().into());
            }
            let symbols = read.symbols();
            if read.loaded.is_some() && !colours_alone {
                row.insert(
                    "symbols".into(),
                    Value::Array(
                        symbols
                            .iter()
                            .map(|s| {
                                let mut one = Map::new();
                                one.insert("path".into(), s.path.clone().into());
                                one.insert("name".into(), s.name.clone().into());
                                one.insert("of".into(), s.of.word().into());
                                one.insert("word".into(), s.word.clone().into());
                                one.insert("line".into(), s.line.into());
                                one.insert("from".into(), s.span.from.into());
                                one.insert("to".into(), s.span.to.into());
                                if let Some(p) = s.parent {
                                    one.insert("parent".into(), symbols[p].path.clone().into());
                                }
                                Value::Object(one)
                            })
                            .collect(),
                    ),
                );
            }
            Value::Object(row)
        })
        .collect();
    Ok(json!({ "files": files }))
}
