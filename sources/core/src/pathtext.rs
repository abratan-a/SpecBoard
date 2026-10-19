//! Paths as they are written, and the names in them (`specboard://SpecBoard/Docs/Design:DesignSections`
//! 「名前に使える字と、パスの中での囲み」): the one place a path is taken apart into its names and
//! written back from them, so that what compares, finds and follows paths never disagrees on where
//! one name ends.
//!
//! ```text
//! Store/Ordering:Order.lines[].sku     sections / card . names, an array's elements
//! repo/docs/design:'CardKinds.md'      a name holding a character a path splits on, quoted
//! Notes:'Owner''s guide'               a quote inside a quoted name, doubled
//! ```
//!
//! A name is quoted only when it holds whitespace or one of `NEEDS_QUOTES`; everything else
//! (Japanese, `-`, `&`, `+`, …) is written bare. Reading, a name that starts with `'` runs to the
//! quote that closes it; a bare one is split where paths were always split (sections at `/` and
//! `:`, the card's names at `.` and `[`), so a path written before names could hold these reads the
//! same. Writing, only the names that need it are quoted: one thing has one way to be written.
//!
//! The case table `tests/fixtures/paths.json` holds what this reads and writes; the app's own
//! reader is held to the same table.

use std::borrow::Cow;

/// The most characters a name has.
pub const NAME_MAX: usize = 255;

/// The characters that have a name quoted in a path, besides whitespace: the path's own
/// separators (`/ : . [ ]`), the quote, the tree of before (`@`), what ends or splits a reference
/// in a text (`| { }`), a line's row address (`#`), a pick's quoted value (`"`) and Markdown's
/// code (`` ` ``).
pub const NEEDS_QUOTES: [char; 13] = [
    '/', ':', '.', '[', ']', '\'', '@', '|', '{', '}', '#', '"', '`',
];

/// Why a name cannot be one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NameProblem {
    Empty,
    TooLong,
    /// A line break, a tab or another control character.
    Control,
    /// Whitespace before or after it (a name is written trimmed).
    Spaces,
}

impl NameProblem {
    /// The key the case table names it by.
    #[cfg(test)]
    pub fn key(self) -> &'static str {
        match self {
            NameProblem::Empty => "empty",
            NameProblem::TooLong => "tooLong",
            NameProblem::Control => "control",
            NameProblem::Spaces => "spaces",
        }
    }
}

/// Why `name` (as it is kept, trimmed) cannot be a name: none when it can.
pub fn name_problem(name: &str) -> Option<NameProblem> {
    if name.is_empty() {
        return Some(NameProblem::Empty);
    }
    if name.chars().count() > NAME_MAX {
        return Some(NameProblem::TooLong);
    }
    if name.chars().any(char::is_control) {
        return Some(NameProblem::Control);
    }
    if name.trim() != name {
        return Some(NameProblem::Spaces);
    }
    None
}

/// A name as it is compared with another in one place: ASCII letters in any case are one (as the
/// store's `NOCASE` indexes compare), every other character as it is (`É` and `é` are two).
pub fn fold(name: &str) -> String {
    name.to_ascii_lowercase()
}

/// Whether two names are the same name (`fold`).
pub fn same(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

/// Whether a name is quoted in a path.
pub fn needs_quotes(name: &str) -> bool {
    name.is_empty()
        || name
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || NEEDS_QUOTES.contains(&c))
}

/// A name as it is written in a path: quoted when it needs it, a quote inside doubled.
pub fn quote(name: &str) -> Cow<'_, str> {
    if needs_quotes(name) {
        Cow::Owned(format!("'{}'", name.replace('\'', "''")))
    } else {
        Cow::Borrowed(name)
    }
}

/// A pick's key (a field's name inside `[…]`) as it is written: quoted as a name is, and when it
/// holds what splits a pick (`=` `,`).
pub fn quote_key(name: &str) -> Cow<'_, str> {
    if name.contains(['=', ',']) && !needs_quotes(name) {
        Cow::Owned(format!("'{}'", name.replace('\'', "''")))
    } else {
        quote(name)
    }
}

/// Sections' names written as a section's path (`Store/Ordering`).
pub fn join_sections<'a>(names: impl IntoIterator<Item = &'a str>) -> String {
    names.into_iter().map(quote).collect::<Vec<_>>().join("/")
}

/// Names written one after another inside a card (`Checkout.review`, `lines.sku`).
pub fn join_names<'a>(names: impl IntoIterator<Item = &'a str>) -> String {
    names.into_iter().map(quote).collect::<Vec<_>>().join(".")
}

/// A card's path: its section's path (as written) and its name.
pub fn card_path(section: &str, card: &str) -> String {
    format!("{section}:{}", quote(card))
}

/// A path (as written) and a name inside what it names.
pub fn member(path: &str, name: &str) -> String {
    format!("{path}.{}", quote(name))
}

// MARK: - Reading

/// Why a path cannot be read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PathError {
    /// Nothing written.
    Empty,
    /// A quote that opens a name and never closes.
    UnclosedQuote,
    /// Something right after a quoted name or a `[…]`, before the next separator.
    Stray,
    /// A name with nothing in it (`Store::User`, `a..b`, `''`).
    EmptyName,
    /// A `[` that never closes, or a `"` in it.
    UnclosedBracket,
    /// `[…]` after a section's name.
    BracketInSection,
}

impl PathError {
    /// The key the case table names it by.
    #[cfg(test)]
    pub fn key(self) -> &'static str {
        match self {
            PathError::Empty => "empty",
            PathError::UnclosedQuote => "unclosedQuote",
            PathError::Stray => "stray",
            PathError::EmptyName => "emptyName",
            PathError::UnclosedBracket => "unclosedBracket",
            PathError::BracketInSection => "bracketInSection",
        }
    }

    /// Why `path` cannot be read, said to the one who wrote it.
    pub fn said(self, path: &str) -> String {
        match self {
            PathError::Empty => text!("pathText.error.empty"),
            PathError::UnclosedQuote => text!("pathText.error.unclosedQuote", path),
            PathError::Stray => text!("pathText.error.stray", path),
            PathError::EmptyName => text!("pathText.error.emptyName", path),
            PathError::UnclosedBracket => text!("pathText.error.unclosedBracket", path),
            PathError::BracketInSection => text!("pathText.error.bracketInSection", path),
        }
    }
}

fn is_space(c: char) -> bool {
    c == ' ' || c == '\t'
}

/// The end of a quoted name that starts at `open` (its `'`): the byte just past the quote that
/// closes it; none when it never closes.
pub fn quoted_end(text: &str, open: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut i = open + 1;
    while i < bytes.len() {
        if bytes[i] == b'\'' {
            if bytes.get(i + 1) == Some(&b'\'') {
                i += 2;
                continue;
            }
            return Some(i + 1);
        }
        i += 1;
    }
    None
}

/// The end of a pick's quoted value that starts at `open` (its `"`, `\` escaping the next
/// character): the byte just past the closing `"`.
fn string_end(text: &str, open: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut i = open + 1;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 2,
            b'"' => return Some(i + 1),
            _ => i += 1,
        }
    }
    None
}

/// The end of a `[…]` that starts at `open` (its `[`): the byte just past the `]` that closes
/// it — keys quoted as names, values quoted with `"`, and `[…]` inside it counted.
pub fn bracket_end(text: &str, open: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut depth = 0usize;
    let mut key_start = true;
    let mut i = open;
    while i < bytes.len() {
        let b = bytes[i];
        if key_start && (b == b' ' || b == b'\t') {
            i += 1;
            continue;
        }
        if key_start && b == b'\'' && depth == 1 {
            i = quoted_end(text, i)?;
            key_start = false;
            continue;
        }
        key_start = false;
        match b {
            b'"' if depth > 0 => {
                i = string_end(text, i)?;
                continue;
            }
            b'[' => {
                depth += 1;
                key_start = depth == 1;
            }
            b']' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i + 1);
                }
            }
            b',' if depth == 1 => key_start = true,
            _ => {}
        }
        i += 1;
    }
    None
}

/// A separator of a path outside every quoted name and `[…]`, and where it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Mark {
    at: usize,
    sep: char,
}

/// The separators of a written path (`/ : . @`) outside quoted names and `[…]`, and the first
/// reason it cannot be read, when there is one (the separators up to there are found).
fn marks(text: &str) -> (Vec<Mark>, Option<PathError>) {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut error = None;
    let mut start = true;
    // After a quoted name only its `[…]` or spaces until the next separator; after a `[…]`, only
    // spaces.
    let (mut quoted, mut picked) = (false, false);
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if start && (b == b' ' || b == b'\t') {
            i += 1;
            continue;
        }
        if start && b == b'\'' {
            match quoted_end(text, i) {
                Some(end) => {
                    i = end;
                    start = false;
                    quoted = true;
                    continue;
                }
                None => {
                    error.get_or_insert(PathError::UnclosedQuote);
                    return (out, error);
                }
            }
        }
        start = false;
        match b {
            b'/' | b':' | b'.' | b'@' => {
                out.push(Mark {
                    at: i,
                    sep: b as char,
                });
                start = true;
                (quoted, picked) = (false, false);
            }
            b' ' | b'\t' => {}
            b'[' if !picked => match bracket_end(text, i) {
                Some(end) => {
                    i = end;
                    picked = true;
                    continue;
                }
                None => {
                    error.get_or_insert(PathError::UnclosedBracket);
                    return (out, error);
                }
            },
            _ if quoted || picked => {
                error.get_or_insert(PathError::Stray);
            }
            _ => {}
        }
        i += 1;
    }
    (out, error)
}

/// Where the first byte `wanted` takes lies in a text that starts with a written path, outside
/// quoted names and `[…]`: a quote opens a name at the start and after a separator (`/ : . @`),
/// spaces before it let be. None when it is not there, or a quote or a `[` before it never closes.
pub fn find_outside(text: &str, wanted: impl Fn(usize) -> bool) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut start = true;
    let mut i = 0;
    while i < bytes.len() {
        if wanted(i) {
            return Some(i);
        }
        let b = bytes[i];
        if start && b == b'\'' {
            i = quoted_end(text, i)?;
            start = false;
            continue;
        }
        match b {
            b' ' | b'\t' => {}
            b'/' | b':' | b'.' | b'@' => {
                start = true;
                i += 1;
                continue;
            }
            b'[' => {
                i = bracket_end(text, i)?;
                start = false;
                continue;
            }
            _ => {}
        }
        if !(b == b' ' || b == b'\t') {
            start = false;
        }
        i += 1;
    }
    None
}

/// The first `:` outside quoted names and `[…]`: where a path's sections end and its card begins.
pub fn colon(text: &str) -> Option<usize> {
    marks(text)
        .0
        .into_iter()
        .find(|m| m.sep == ':')
        .map(|m| m.at)
}

/// Whether a path says its section (`Core:User`, not `User`).
pub fn has_section(text: &str) -> bool {
    colon(text).is_some()
}

/// The sections of a path that names a section alone (`Store/Ordering:`): a `:` written outside
/// quoted names with nothing after it. None for a card's path and for a path without a `:` — a
/// section is written with its `:`, so it is never read as a card of the same name.
pub fn section_only(text: &str) -> Option<&str> {
    let text = text.trim_matches(is_space);
    let c = colon(text)?;
    if !text[c + 1..].trim_matches(is_space).is_empty() {
        return None;
    }
    let section = text[..c].trim_matches(is_space);
    (!section.is_empty()).then_some(section)
}

/// A path cut at its `:`: the sections (none without one) and what follows.
pub fn split_section(text: &str) -> (Option<&str>, &str) {
    match colon(text) {
        Some(c) => (Some(&text[..c]), &text[c + 1..]),
        None => (None, text),
    }
}

/// The first name of a path as written, read: its top section's, or the card's when it says none
/// (`repo/src:'a.rs'` → `repo`).
pub fn first_name(text: &str) -> String {
    let end = marks(text)
        .0
        .into_iter()
        .find(|m| ['/', ':', '.'].contains(&m.sep))
        .map_or(text.len(), |m| m.at);
    unquote(&text[..end]).into_owned()
}

/// A path written with the tree of before (`main@Store:Order`): the tree and the rest. Only an `@`
/// outside quoted names and before every other separator is one.
pub fn tree_prefix(text: &str) -> Option<(&str, &str)> {
    let (marks, _) = marks(text);
    let first = marks.first().filter(|m| m.sep == '@')?;
    Some((&text[..first.at], &text[first.at + 1..]))
}

fn split_at_marks(text: &str, sep: char) -> Vec<&str> {
    let mut out = Vec::new();
    let mut from = 0;
    for m in marks(text).0.into_iter().filter(|m| m.sep == sep) {
        out.push(&text[from..m.at]);
        from = m.at + 1;
    }
    out.push(&text[from..]);
    out
}

/// A section's path split into its names as written (`Store/'设计'` → `Store`, `'设计'`).
pub fn slashes(text: &str) -> Vec<&str> {
    split_at_marks(text, '/')
}

/// The names after a card's section split as written: the card's, then the names inside it, each
/// with its `[…]` (`Order.lines[].sku`, `Hosts.ports[ip=192.0.2.1]`).
pub fn dots(text: &str) -> Vec<&str> {
    split_at_marks(text, '.')
}

/// A section's path cut before its last name: the parent's path and the name as written.
pub fn last_slash(text: &str) -> Option<(&str, &str)> {
    let m = marks(text).0.into_iter().rfind(|m| m.sep == '/')?;
    Some((&text[..m.at], &text[m.at + 1..]))
}

/// A path cut before its last name inside a card: the path before it and the name as written.
pub fn last_dot(text: &str) -> Option<(&str, &str)> {
    let m = marks(text).0.into_iter().rfind(|m| m.sep == '.')?;
    Some((&text[..m.at], &text[m.at + 1..]))
}

/// One name of a path as written, read: its quotes taken off (`'a''b'` → `a'b`), a bare one as it
/// is; spaces around it dropped. Its `[…]`, if any, is the rest (`step`).
pub fn unquote(written: &str) -> Cow<'_, str> {
    step(written).0
}

/// One name of a path as written, read into the name and what follows it: the `[…]` that picks
/// in it (`ports[0]` → `ports`, `[0]`), or nothing.
pub fn step(written: &str) -> (Cow<'_, str>, &str) {
    let text = written.trim_matches(is_space);
    if text.starts_with('\'')
        && let Some(end) = quoted_end(text, 0)
    {
        let inner = &text[1..end - 1];
        let name = if inner.contains("''") {
            Cow::Owned(inner.replace("''", "'"))
        } else {
            Cow::Borrowed(inner)
        };
        return (name, text[end..].trim_matches(is_space));
    }
    match text.find('[') {
        Some(open) => (
            Cow::Borrowed(text[..open].trim_matches(is_space)),
            &text[open..],
        ),
        None => (Cow::Borrowed(text), ""),
    }
}

/// One name of a path as written, read for a place that picks no element: the name, `[]` dropped;
/// one that picks (`ports[0]`) as it is written, which names nothing there.
pub fn plain(written: &str) -> Cow<'_, str> {
    let text = written.trim_matches(is_space);
    let (name, rest) = step(text);
    let rest = rest.trim_matches(is_space);
    if rest.is_empty()
        || rest
            .strip_prefix('[')
            .and_then(|r| r.strip_suffix(']'))
            .is_some_and(|inner| inner.trim_matches(is_space).is_empty())
    {
        name
    } else {
        Cow::Borrowed(text)
    }
}

/// The names written one after another inside a card, read (`lines[].sku` → `lines`, `sku`): each
/// unquoted, its `[…]` dropped.
pub fn names(written: &str) -> Vec<String> {
    dots(written)
        .into_iter()
        .map(|s| unquote(s).into_owned())
        .collect()
}

/// Names written one after another inside a card, written again as they are kept (quoted only
/// where they need it), their `[…]` dropped.
pub fn canonical_names(written: &str) -> String {
    join_names(names(written).iter().map(String::as_str))
}

/// Whether a path is one name alone (no section, nothing inside it): a name beside another.
pub fn is_one_name(written: &str) -> bool {
    let text = written.trim();
    !text.is_empty() && marks(text).0.is_empty()
}

/// Names written one after another with each `[]` dropped (`lines[].sku` → `lines.sku`), as
/// written otherwise.
pub fn without_alls(written: &str) -> String {
    dots(written)
        .into_iter()
        .map(without_all)
        .collect::<Vec<_>>()
        .join(".")
}

/// A name of a path as written with a `[]` at its end dropped (`lines[]` → `lines`), quotes and all.
pub fn without_all(written: &str) -> &str {
    let text = written.trim_matches(is_space);
    let (_, rest) = step(text);
    if rest.trim_matches(is_space) == "[]" {
        let open = text.len() - rest.len();
        text[..open].trim_matches(is_space)
    } else {
        text
    }
}

/// A pick (what is inside `[…]`) cut into its parts at its `,`: each a key as written (none for
/// `=value`) and the value as written. A part with no `=` is a key alone (none) — said wrong by the
/// one that reads the pick.
pub fn pick_parts(inner: &str) -> Vec<(Option<&str>, Option<&str>)> {
    let mut out = Vec::new();
    let bytes = inner.as_bytes();
    let mut from = 0;
    let mut key_start = true;
    let mut eq: Option<usize> = None;
    let mut i = 0;
    let mut push = |from: usize, to: usize, eq: Option<usize>| {
        let part = &inner[from..to];
        match eq {
            Some(e) => out.push((Some(&inner[from..e]), Some(&inner[e + 1..to]))),
            None => out.push((Some(part), None)),
        }
    };
    while i < bytes.len() {
        let b = bytes[i];
        if key_start && (b == b' ' || b == b'\t') {
            i += 1;
            continue;
        }
        if key_start && b == b'\'' {
            key_start = false;
            i = quoted_end(inner, i).unwrap_or(bytes.len());
            continue;
        }
        key_start = false;
        match b {
            b'"' => {
                i = string_end(inner, i).unwrap_or(bytes.len());
                continue;
            }
            b'=' if eq.is_none() => eq = Some(i),
            b',' => {
                push(from, i, eq);
                from = i + 1;
                eq = None;
                key_start = true;
            }
            _ => {}
        }
        i += 1;
    }
    push(from, bytes.len(), eq);
    out
}

/// A path read into its names.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Parts {
    /// The tree written before an `@` (`main@Store:Order`), of before.
    pub tree: Option<String>,
    /// The sections' names, from the top (before the `:`, or the whole of a section's path).
    pub sections: Vec<String>,
    /// Whether a `:` was written: the names after it are a card's, and none after it names the
    /// section itself (`![[Data/Tenant:]]`).
    pub colon: bool,
    /// The names after the `:` (or of a path with neither `:` nor `/`): the card's, then those
    /// inside it.
    pub names: Vec<String>,
    /// What each name picks, by its `[…]` as written inside the brackets (`""` for `[]`); none
    /// without one. As many as `names`.
    pub picks: Vec<Option<String>>,
}

/// One name after the `:` read: the name and its pick.
fn read_step(written: &str) -> Result<(String, Option<String>), PathError> {
    let (name, rest) = step(written);
    if name.is_empty() {
        return Err(PathError::EmptyName);
    }
    let pick = if rest.is_empty() {
        None
    } else {
        let inner = rest
            .strip_prefix('[')
            .and_then(|r| r.strip_suffix(']'))
            .ok_or(PathError::Stray)?;
        Some(canonical_pick(inner.trim_matches(is_space)))
    };
    Ok((name.into_owned(), pick))
}

/// A pick written again with its keys quoted only where they need it.
fn canonical_pick(inner: &str) -> String {
    if inner.is_empty() || inner.chars().all(|c| c.is_ascii_digit()) {
        return inner.to_string();
    }
    pick_parts(inner)
        .into_iter()
        .map(|(key, value)| {
            let key = key.unwrap_or("").trim_matches(is_space);
            let key = if key.is_empty() {
                String::new()
            } else {
                quote_key(&unquote(key)).into_owned()
            };
            match value {
                Some(v) => format!("{key}={}", v.trim_matches(is_space)),
                None => key,
            }
        })
        .collect::<Vec<_>>()
        .join(",")
}

/// A section's name read.
fn read_section(written: &str) -> Result<String, PathError> {
    let (name, rest) = step(written);
    if !rest.is_empty() {
        return Err(if rest.starts_with('[') {
            PathError::BracketInSection
        } else {
            PathError::Stray
        });
    }
    if name.is_empty() {
        return Err(PathError::EmptyName);
    }
    Ok(name.into_owned())
}

/// Why a quote or a pick in a path keeps it from being read (an unclosed quote, something
/// right after a quoted name or a `[…]`, a `[` never closed), said: what a target and a check say
/// before looking the path up. Other ways a path is written wrong are left to the lookup.
pub fn unreadable(text: &str) -> Option<String> {
    match parse(text) {
        Err(e @ (PathError::UnclosedQuote | PathError::Stray | PathError::UnclosedBracket)) => {
            Some(e.said(text.trim()))
        }
        _ => None,
    }
}

/// A path read into its names, or why it cannot be.
pub fn parse(text: &str) -> Result<Parts, PathError> {
    let text = text.trim();
    if text.is_empty() {
        return Err(PathError::Empty);
    }
    if let (_, Some(error)) = marks(text) {
        return Err(error);
    }
    let mut out = Parts::default();
    let mut body = text;
    if let Some((tree, rest)) = tree_prefix(text) {
        let tree = read_section(tree)?;
        out.tree = Some(tree);
        body = rest.trim_matches(is_space);
        if body.is_empty() {
            return Ok(out);
        }
        // `main@:Order`: the tree's root, then a card.
        if let Some(rest) = body.strip_prefix(':') {
            out.colon = true;
            return read_names(rest, out);
        }
    }
    let (sections, rest) = split_section(body);
    match sections {
        Some(sections) => {
            if !sections.trim_matches(is_space).is_empty() {
                out.sections = slashes(sections)
                    .into_iter()
                    .map(read_section)
                    .collect::<Result<_, _>>()?;
            }
            out.colon = true;
            if rest.trim_matches(is_space).is_empty() {
                return Ok(out);
            }
            read_names(rest, out)
        }
        None if marks(rest).0.iter().any(|m| m.sep == '/') => {
            out.sections = slashes(rest)
                .into_iter()
                .map(read_section)
                .collect::<Result<_, _>>()?;
            Ok(out)
        }
        None => read_names(rest, out),
    }
}

fn read_names(rest: &str, mut out: Parts) -> Result<Parts, PathError> {
    for written in dots(rest) {
        let (name, pick) = read_step(written)?;
        out.names.push(name);
        out.picks.push(pick);
    }
    Ok(out)
}

/// A path written from its names, each quoted only when it needs it.
pub fn write(parts: &Parts) -> String {
    let mut out = String::new();
    if let Some(tree) = &parts.tree {
        out.push_str(&quote(tree));
        out.push('@');
    }
    out.push_str(&join_sections(parts.sections.iter().map(String::as_str)));
    if parts.colon {
        out.push(':');
    }
    let names: Vec<String> = parts
        .names
        .iter()
        .enumerate()
        .map(|(i, name)| match parts.picks.get(i).cloned().flatten() {
            Some(pick) => format!("{}[{pick}]", quote(name)),
            None => quote(name).into_owned(),
        })
        .collect();
    out.push_str(&names.join("."));
    out
}

/// A path written again as it is kept: each name quoted only when it needs it. One that cannot be
/// read is kept as it was written.
pub fn canonical(text: &str) -> Cow<'_, str> {
    match parse(text) {
        Ok(parts) => {
            let again = write(&parts);
            if again == text {
                Cow::Borrowed(text)
            } else {
                Cow::Owned(again)
            }
        }
        Err(_) => Cow::Borrowed(text),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn table() -> Value {
        serde_json::from_str(include_str!("../tests/fixtures/paths.json")).unwrap()
    }

    fn strings(v: &Value) -> Vec<String> {
        v.as_array()
            .map(|a| a.iter().map(|s| s.as_str().unwrap().to_string()).collect())
            .unwrap_or_default()
    }

    #[test]
    fn every_path_of_the_case_table_reads_and_writes_as_it_says() {
        let table = table();
        let mut wrong = Vec::new();
        for case in table["paths"].as_array().unwrap() {
            let text = case["text"].as_str().unwrap();
            let got = parse(text);
            if let Some(refused) = case["refused"].as_str() {
                if got.as_ref().err().map(|e| e.key()) != Some(refused) {
                    wrong.push(format!("{text}: refused {refused}, read {got:?}"));
                }
                continue;
            }
            let Ok(parts) = got else {
                wrong.push(format!("{text}: {got:?}"));
                continue;
            };
            let want = Parts {
                tree: case["tree"].as_str().map(str::to_string),
                sections: strings(&case["sections"]),
                colon: case["colon"].as_bool().unwrap_or(false),
                names: strings(&case["names"]),
                picks: match case.get("picks") {
                    Some(p) => p
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|v| v.as_str().map(str::to_string))
                        .collect(),
                    None => vec![None; case["names"].as_array().map_or(0, Vec::len)],
                },
            };
            if parts != want {
                wrong.push(format!("{text}: read {parts:?}, the table says {want:?}"));
            }
            let written = case["written"].as_str().unwrap_or(text);
            if write(&parts) != written {
                wrong.push(format!("{text}: written {}, not {written}", write(&parts)));
            }
            if canonical(text) != written {
                wrong.push(format!("{text}: kept as {}", canonical(text)));
            }
            // What is written reads back the same.
            if parse(written).as_ref() != Ok(&parts) {
                wrong.push(format!("{written}: does not read back"));
            }
        }
        assert!(wrong.is_empty(), "{wrong:#?}");
    }

    #[test]
    fn every_name_of_the_case_table_is_quoted_as_it_says() {
        let table = table();
        let mut wrong = Vec::new();
        for case in table["names"].as_array().unwrap() {
            let name = case["name"].as_str().unwrap();
            let written = case["written"].as_str().unwrap();
            if quote(name) != written {
                wrong.push(format!("{name}: {}, not {written}", quote(name)));
            }
            if unquote(written) != name {
                wrong.push(format!("{written}: reads {}", unquote(written)));
            }
            let problem = name_problem(name).map(NameProblem::key);
            if problem != case["problem"].as_str() {
                wrong.push(format!("{name:?}: problem {problem:?}"));
            }
        }
        assert!(wrong.is_empty(), "{wrong:#?}");
    }

    #[test]
    fn a_quoted_name_is_one_name_to_every_splitter() {
        assert_eq!(colon("repo/docs/design:'CardKinds.md'"), Some(16));
        assert_eq!(colon("'a:b'/c:d"), Some(7));
        assert_eq!(colon("Docs"), None);
        assert_eq!(
            split_section("'Q&A 2026':'x:y'.z"),
            (Some("'Q&A 2026'"), "'x:y'.z")
        );
        assert_eq!(
            slashes("repo/'.claude'/rules"),
            ["repo", "'.claude'", "rules"]
        );
        assert_eq!(slashes("Docs/'設計/詳細'"), ["Docs", "'設計/詳細'"]);
        assert_eq!(dots("'release notes.md'.a"), ["'release notes.md'", "a"]);
        assert_eq!(
            dots("hosts[ip=192.0.2.1].name"),
            ["hosts[ip=192.0.2.1]", "name"]
        );
        assert_eq!(dots(r#"x[n="a.]b"].y"#), [r#"x[n="a.]b"]"#, "y"]);
        assert_eq!(dots("x['a.b'=1].y"), ["x['a.b'=1]", "y"]);
        assert_eq!(last_dot("Core:'a.b'.'c.d'"), Some(("Core:'a.b'", "'c.d'")));
        assert_eq!(last_slash("a/'b/c'"), Some(("a", "'b/c'")));
        assert_eq!(
            tree_prefix("main@Store:Order"),
            Some(("main", "Store:Order"))
        );
        assert_eq!(tree_prefix("Store:'a@b'"), None);
        assert_eq!(tree_prefix("'x@y'/z"), None);
        // A quote inside a bare name is the name's own.
        assert_eq!(dots("Owner's.x"), ["Owner's", "x"]);
        assert_eq!(unquote("Owner's"), "Owner's");
    }

    #[test]
    fn a_step_is_its_name_and_its_pick() {
        assert_eq!(step("ports[0]"), (Cow::Borrowed("ports"), "[0]"));
        assert_eq!(step("'user list'[]"), (Cow::Borrowed("user list"), "[]"));
        assert_eq!(step(" 'a''b' "), (Cow::Borrowed("a'b"), ""));
        assert_eq!(without_all("lines[]"), "lines");
        assert_eq!(without_all("'a b'[]"), "'a b'");
        assert_eq!(without_all("ports[0]"), "ports[0]");
        assert_eq!(
            pick_parts("number=8200, 'a=b'=x,=v"),
            [
                (Some("number"), Some("8200")),
                (Some(" 'a=b'"), Some("x")),
                (Some(""), Some("v")),
            ]
        );
        assert_eq!(pick_parts(r#"n="a,b""#), [(Some("n"), Some(r#""a,b""#))]);
    }

    #[test]
    fn names_are_one_in_any_ascii_case_only() {
        assert!(same("Order", "ORDER"));
        assert!(!same("É", "é"));
        assert!(!same("注文", "注文 "));
        assert_eq!(fold("CardKinds.MD"), "cardkinds.md");
        assert_eq!(fold("Éa"), "Éa");
    }
}
