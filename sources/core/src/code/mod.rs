//! A working directory's files read as code: each file's functions and types (`symbols`) and its
//! text coloured by its language (`highlight`), with the grammars built in (`registry`) and, on the
//! agent, those a machine adds (`loader`). What is read is kept for the process by each file's path
//! and checked against the file each time it is used (`cache`), never stored: a file's functions and
//! types are always the file's now. Design: `specboard://SpecBoard/Docs/Design:Overview` (the code
//! in the working directories).

pub mod cache;
pub mod files;
pub mod highlight;
#[cfg(feature = "dynamic-grammars")]
pub mod loader;
pub mod mend;
pub mod registry;
pub mod symbols;

use crate::text::Keyed;
use registry::Language;
use std::borrow::Cow;
use std::cell::RefCell;
use std::ops::ControlFlow;
use std::time::{Duration, Instant};

/// The largest file read for its functions and types: past it, a file's text is mostly data.
pub const MOST_BYTES: usize = 2 * 1024 * 1024;

/// How long one file is parsed before it is given up: a grammar that loops on some text holds no
/// call up.
pub const MOST_PARSE: Duration = Duration::from_millis(500);

thread_local! {
    /// One parser for each thread: a parser is not shared, and making one each time costs.
    static PARSER: RefCell<tree_sitter::Parser> = RefCell::new(tree_sitter::Parser::new());
}

/// The most times a file is parsed again for its grammar's mis-reads (`mend`): each blanks at least
/// one byte, and a file of thousands would cost as many parses.
pub const MOST_MENDS: usize = 64;

/// A file's text parsed in its language, given up after `MOST_PARSE`.
pub fn parse(language: &Language, text: &[u8]) -> Result<tree_sitter::Tree, Keyed> {
    parse_mended(language, text).map(|(tree, _)| tree)
}

/// A file's text parsed in its language, with the text the tree was parsed from: the text itself, or
/// a copy of the same length with the bytes the grammar mis-reads blanked (`Language::mend`), so a
/// node's bytes are still read from the text itself. `MOST_PARSE` covers every pass.
pub fn parse_mended<'t>(
    language: &Language,
    text: &'t [u8],
) -> Result<(tree_sitter::Tree, Cow<'t, [u8]>), Keyed> {
    PARSER.with(|parser| {
        let mut parser = parser.borrow_mut();
        parser
            .set_language(&language.grammar)
            .map_err(|e| keyed!("code.grammar", language = language.id, e))?;
        let began = Instant::now();
        let mut source: Cow<[u8]> = Cow::Borrowed(text);
        let mut tree = parse_once(&mut parser, &source, None, began)?;
        if let Some(mend) = language.mend.filter(|m| m.may(text)) {
            for _ in 0..MOST_MENDS {
                let Some(blank) = (mend.find)(&tree, &source) else {
                    break;
                };
                let edit = tree_sitter::InputEdit {
                    start_byte: blank.start,
                    old_end_byte: blank.end,
                    new_end_byte: blank.end,
                    start_position: point_at(&source, blank.start),
                    old_end_position: point_at(&source, blank.end),
                    new_end_position: point_at(&source, blank.end),
                };
                source.to_mut()[blank].fill(b' ');
                tree.edit(&edit);
                tree = parse_once(&mut parser, &source, Some(&tree), began)?;
            }
        }
        Ok((tree, source))
    })
}

fn parse_once(
    parser: &mut tree_sitter::Parser,
    text: &[u8],
    old: Option<&tree_sitter::Tree>,
    began: Instant,
) -> Result<tree_sitter::Tree, Keyed> {
    let mut over = |_: &tree_sitter::ParseState| {
        if began.elapsed() > MOST_PARSE {
            ControlFlow::Break(())
        } else {
            ControlFlow::Continue(())
        }
    };
    let options = tree_sitter::ParseOptions::new().progress_callback(&mut over);
    let len = text.len();
    let tree = parser.parse_with_options(
        &mut |at: usize, _| if at < len { &text[at..] } else { &[][..] },
        old,
        Some(options),
    );
    parser.reset();
    tree.ok_or_else(|| keyed!("code.timedOut", ms = MOST_PARSE.as_millis()))
}

/// The row and byte column of `byte`, as a tree's edit is told.
fn point_at(text: &[u8], byte: usize) -> tree_sitter::Point {
    let before = &text[..byte];
    let row = before.iter().filter(|&&b| b == b'\n').count();
    let column = byte
        - before
            .iter()
            .rposition(|&b| b == b'\n')
            .map_or(0, |i| i + 1);
    tree_sitter::Point { row, column }
}

/// What one file's text came to: its functions and types, or why it has none.
#[derive(Clone, Debug)]
pub struct Read {
    pub found: symbols::Found,
    pub line_starts: Vec<usize>,
    pub problem: Option<Keyed>,
    /// The whole text's fingerprint by its words (`symbols::tokens_digest`), when it was parsed.
    pub digest: Option<String>,
}

/// A file's text read for its functions and types, in `language`.
pub fn read_text(language: &Language, text: &[u8]) -> Read {
    let mut out = Read {
        found: symbols::Found::default(),
        line_starts: Vec::new(),
        problem: None,
        digest: None,
    };
    let Some(query) = language.symbols_query() else {
        out.problem = Some(keyed!("code.noSymbols", language = language.name));
        return out;
    };
    let query = match query {
        Ok(q) => q,
        Err(e) => {
            out.problem = Some(keyed!("code.query", language = language.id, e));
            return out;
        }
    };
    if text.len() > MOST_BYTES {
        out.problem = Some(keyed!(
            "code.tooLarge",
            size = text.len(),
            most = MOST_BYTES
        ));
        return out;
    }
    if std::str::from_utf8(text).is_err() || text.contains(&0) {
        out.problem = Some(keyed!("code.notText"));
        return out;
    }
    let tree = match parse(language, text) {
        Ok(t) => t,
        Err(why) => {
            out.problem = Some(why);
            return out;
        }
    };
    out.line_starts = symbols::line_starts(text);
    out.digest = Some(symbols::tokens_digest(&tree, text, [(0, text.len())]));
    out.found = symbols::find(&tree, text, query, &language.rules, &out.line_starts);
    if out.found.cut {
        out.problem = Some(keyed!("code.cut", most = symbols::MOST_SYMBOLS));
    }
    out
}

#[cfg(test)]
mod tests;
