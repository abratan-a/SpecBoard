//! A file's text coloured by its language: spans of a few classes, in UTF-16 offsets over the text
//! returned, ready for the window's attributed string. Made each time it is asked for (about 10 MB a
//! second): a file shown is read anew anyway.

use super::registry::{Language, registry};
use crate::text::Keyed;
use tree_sitter_highlight::{HighlightEvent, Highlighter};

/// The classes a span is of, by its index: what the window gives each a colour by.
pub const CLASSES: [&str; 16] = [
    "keyword",
    "string",
    "comment",
    "number",
    "type",
    "function",
    "constant",
    "property",
    "variable.builtin",
    "operator",
    "punctuation",
    "attribute",
    "tag",
    "escape",
    "label",
    "module",
];

/// The capture names the grammars' highlight queries use that are coloured, each with its class
/// (by index into `CLASSES`). A capture is matched to the longest of these it starts with
/// (`function.method` to `function`); one matched to none is not coloured.
const NAMES: [(&str, usize); 41] = [
    ("keyword", 0),
    ("conditional", 0),
    ("repeat", 0),
    ("include", 0),
    ("exception", 0),
    ("storageclass", 0),
    ("text.title", 0),
    ("markup.heading", 0),
    ("string", 1),
    ("character", 1),
    ("text.literal", 1),
    ("markup.raw", 1),
    ("text.uri", 1),
    ("markup.link", 1),
    ("comment", 2),
    ("number", 3),
    ("float", 3),
    ("type", 4),
    ("constructor", 4),
    ("function", 5),
    ("method", 5),
    ("constant", 6),
    ("boolean", 6),
    ("property", 7),
    ("field", 7),
    ("variable.builtin", 8),
    ("variable.parameter.builtin", 8),
    ("operator", 9),
    ("punctuation", 10),
    ("attribute", 11),
    ("decorator", 11),
    ("annotation", 11),
    ("tag", 12),
    ("escape", 13),
    ("string.escape", 13),
    ("label", 14),
    ("text.reference", 14),
    ("module", 15),
    ("namespace", 15),
    ("constant.builtin", 6),
    ("function.macro", 5),
];

/// The names a highlight configuration is given (`HighlightConfiguration::configure`).
pub const RECOGNIZED: [&str; 41] = {
    let mut out = [""; 41];
    let mut i = 0;
    while i < NAMES.len() {
        out[i] = NAMES[i].0;
        i += 1;
    }
    out
};

/// `text` coloured in `language`: `[utf16From, utf16Len, class, …]` for each run of one class, in
/// order. None when the language colours nothing.
pub fn spans(language: &Language, text: &str) -> Option<Result<Vec<u32>, Keyed>> {
    let config = match language.highlight_config()? {
        Ok(c) => c,
        Err(e) => return Some(Err(keyed!("code.query", language = language.id, e))),
    };
    // Coloured from the copy its grammar reads right (`super::parse_mended`): the same bytes but the
    // blanked ones, so every offset below is the text's own.
    let mended;
    let source = if language.mend.is_some_and(|m| m.may(text.as_bytes())) {
        match super::parse_mended(language, text.as_bytes()) {
            Ok((_, copy)) => {
                mended = copy;
                &*mended
            }
            Err(_) => text.as_bytes(),
        }
    } else {
        text.as_bytes()
    };
    let mut highlighter = Highlighter::new();
    let events = match highlighter.highlight(config, source, None, None, |said| {
        let found = registry().named(said)?;
        found.highlight_config()?.ok()
    }) {
        Ok(events) => events,
        Err(e) => return Some(Err(keyed!("code.highlight", language = language.id, e))),
    };
    let mut out: Vec<u32> = Vec::new();
    let mut stack: Vec<usize> = Vec::new();
    // Where the text has been counted to: its byte and its UTF-16 offset.
    let (mut byte, mut utf16) = (0usize, 0u32);
    let advance = |to: usize, byte: &mut usize, utf16: &mut u32| {
        if to > *byte {
            *utf16 += text[*byte..to].encode_utf16().count() as u32;
            *byte = to;
        }
    };
    for event in events {
        match event {
            Ok(HighlightEvent::HighlightStart(h)) => stack.push(NAMES[h.0].1),
            Ok(HighlightEvent::HighlightEnd) => {
                stack.pop();
            }
            Ok(HighlightEvent::Source { start, end }) => {
                let Some(&class) = stack.last() else {
                    continue;
                };
                // A range's ends are where characters begin (the highlighter's are).
                if !text.is_char_boundary(start) || !text.is_char_boundary(end) {
                    continue;
                }
                advance(start, &mut byte, &mut utf16);
                let from = utf16;
                advance(end, &mut byte, &mut utf16);
                let len = utf16 - from;
                if len == 0 {
                    continue;
                }
                let n = out.len();
                if n >= 3 && out[n - 1] == class as u32 && out[n - 3] + out[n - 2] == from {
                    out[n - 2] += len;
                } else {
                    out.extend([from, len, class as u32]);
                }
            }
            Err(e) => return Some(Err(keyed!("code.highlight", language = language.id, e))),
        }
    }
    Some(Ok(out))
}
