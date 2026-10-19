//! The places a piece of Markdown names in the project (`NoteText` in the Swift app's Docs):
//!
//! ```text
//! [[Core:Customer.email]]   a place
//! [[Core:Customer|客]]       a place shown by other words; the path alone is what is followed
//! [[Store/Ordering:]]       a section: its path with the `:` and nothing after it
//! {{Ordering:Status}}       a value the project holds
//! ![[Ordering:Checkout]]    a diagram, drawn in its place; `|sequence` says how
//! ```
//!
//! Code is not read: inside backticks and fenced blocks (``` or ~~~) nothing is a
//! reference. One scanner finds and rewrites, so the two never disagree. A name quoted in the
//! path (`[[Docs:'release notes.md']]`) is one name to the scanner: a space, `]]`, `}}` or `|`
//! inside its quotes ends or splits nothing; a space outside them has it read as words.

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Kind {
    Place,
    Value,
    Embed,
}

#[derive(Clone, Debug)]
pub struct Ref {
    pub kind: Kind,
    /// Between the brackets, trimmed — for an embed, before the `|`.
    pub path: String,
    /// The whole reference, as byte offsets in the text.
    pub start: usize,
    pub end: usize,
    /// An embed's `|sequence`, when said.
    pub option: Option<String>,
    /// A place's `|shown text`: what the link reads as instead of its path, when said. A rename
    /// rewrites the path and keeps it.
    pub shown: Option<String>,
}

impl Ref {
    /// The reference as it was written, naming `path` instead.
    pub fn written(&self, path: &str) -> String {
        match self.kind {
            Kind::Place => match &self.shown {
                Some(s) => format!("[[{path}|{s}]]"),
                None => format!("[[{path}]]"),
            },
            Kind::Value => format!("{{{{{path}}}}}"),
            Kind::Embed => match &self.option {
                Some(o) => format!("![[{path}|{o}]]"),
                None => format!("![[{path}]]"),
            },
        }
    }
}

pub fn refs(text: &str) -> Vec<Ref> {
    let mut out = Vec::new();
    for_each_prose(text, |start, end| out.extend(refs_in(text, start, end)));
    out
}

/// The text with each reference replaced by what `replace` returns for it; none leaves
/// it as written.
pub fn transform(text: &str, mut replace: impl FnMut(&Ref) -> Option<String>) -> String {
    let mut out = String::new();
    let mut at = 0;
    for r in refs(text) {
        let Some(replacement) = replace(&r) else {
            continue;
        };
        out.push_str(&text[at..r.start]);
        out.push_str(&replacement);
        at = r.end;
    }
    out.push_str(&text[at..]);
    out
}

/// Swift's `.whitespaces`: horizontal spaces, not line breaks.
fn trim_spaces(s: &str) -> &str {
    s.trim_matches(|c: char| {
        c.is_whitespace()
            && c != '\n'
            && c != '\r'
            && c != '\u{0B}'
            && c != '\u{0C}'
            && c != '\u{85}'
            && c != '\u{2028}'
            && c != '\u{2029}'
    })
}

/// Calls `body` with each stretch of the text that is not code.
fn for_each_prose(text: &str, mut body: impl FnMut(usize, usize)) {
    let mut fence: Option<(char, usize)> = None;
    let mut line_start = 0;
    while line_start < text.len() {
        let line_end = text[line_start..]
            .find('\n')
            .map(|i| line_start + i)
            .unwrap_or(text.len());
        let line = &text[line_start..line_end];
        let trimmed = line.trim_start_matches(' ');
        let indent = line.chars().count() - trimmed.chars().count();
        if indent <= 3
            && let Some(first) = trimmed.chars().next()
            && (first == '`' || first == '~')
        {
            let run = trimmed.chars().take_while(|c| *c == first).count();
            if run >= 3 {
                match fence {
                    Some((open, count)) => {
                        let rest: String = trimmed.chars().skip(run).collect();
                        if open == first && run >= count && rest.chars().all(char::is_whitespace) {
                            fence = None;
                        }
                    }
                    None => fence = Some((first, run)),
                }
                line_start = if line_end < text.len() {
                    line_end + 1
                } else {
                    line_end
                };
                continue;
            }
        }
        if fence.is_none() {
            outside_inline_code(text, line_start, line_end, &mut body);
        }
        line_start = if line_end < text.len() {
            line_end + 1
        } else {
            line_end
        };
    }
}

/// Within one line, the parts outside backtick code spans.
fn outside_inline_code(text: &str, from: usize, to: usize, body: &mut impl FnMut(usize, usize)) {
    let bytes = text.as_bytes();
    let mut start = from;
    let mut i = from;
    while i < to {
        if bytes[i] != b'`' {
            i += 1;
            continue;
        }
        let mut run_end = i;
        while run_end < to && bytes[run_end] == b'`' {
            run_end += 1;
        }
        let run = run_end - i;
        // The closing run of the same length, or none: then it is a backtick.
        let mut j = run_end;
        let mut close = None;
        while j < to {
            if bytes[j] != b'`' {
                j += 1;
                continue;
            }
            let mut k = j;
            while k < to && bytes[k] == b'`' {
                k += 1;
            }
            if k - j == run {
                close = Some((j, k));
                break;
            }
            j = k;
        }
        let Some((_, close_end)) = close else {
            i = run_end;
            continue;
        };
        if start < i {
            body(start, i);
        }
        start = close_end;
        i = close_end;
    }
    if start < to {
        body(start, to);
    }
}

fn refs_in(text: &str, from: usize, to: usize) -> Vec<Ref> {
    let mut out = Vec::new();
    let mut i = from;
    while i < to {
        let rest = &text[i..to];
        if rest.starts_with("![[")
            && let Some(r) = embed(text, i, to)
        {
            i = r.end;
            out.push(r);
            continue;
        }
        if rest.starts_with("[[")
            && let Some(r) = place(text, i, to)
        {
            i = r.end;
            out.push(r);
            continue;
        }
        if rest.starts_with("{{")
            && let Some(r) = value(text, i, to)
        {
            i = r.end;
            out.push(r);
            continue;
        }
        i += rest.chars().next().map(char::len_utf8).unwrap_or(1);
    }
    out
}

/// Where `close` (`]]`, `}}`) first comes in a line from `open`, outside the names quoted in the
/// path and its `[…]` (`subjects[]`); none past the line's end.
fn closing(text: &str, open: usize, to: usize, close: &str) -> Option<usize> {
    let line = &text[open..to];
    let line = &line[..line.find('\n').unwrap_or(line.len())];
    crate::pathtext::find_outside(line, |i| line.as_bytes()[i..].starts_with(close.as_bytes()))
        .map(|i| open + i)
}

/// Whether what is written between the brackets is a path: not empty, and no space or tab
/// outside its quoted names (prose in brackets is not one).
fn is_path(path: &str) -> bool {
    !path.is_empty()
        && crate::pathtext::find_outside(path, |i| matches!(path.as_bytes()[i], b' ' | b'\t'))
            .is_none()
}

/// `[[` … `]]`, where the path may hold its own `[]` — `subjects[]`. A `|` outside the path's
/// quoted names ends the path; what follows it to the first `]]` is the text shown, read as
/// prose (quotes, dots and spaces in it are its own). An empty one is as if none were written.
fn place(text: &str, start: usize, to: usize) -> Option<Ref> {
    let open = start + 2;
    let line = &text[open..to];
    let line = &line[..line.find('\n').unwrap_or(line.len())];
    let bytes = line.as_bytes();
    let stop =
        crate::pathtext::find_outside(line, |i| bytes[i] == b'|' || bytes[i..].starts_with(b"]]"))?;
    let (path_end, end, shown) = if bytes[stop] == b'|' {
        let close = line[stop + 1..].find("]]")? + stop + 1;
        let shown = trim_spaces(&line[stop + 1..close]);
        (
            stop,
            close + 2,
            (!shown.is_empty()).then(|| shown.to_string()),
        )
    } else {
        (stop, stop + 2, None)
    };
    let path = trim_spaces(&line[..path_end]);
    if !is_path(path) {
        return None;
    }
    Some(Ref {
        kind: Kind::Place,
        path: path.to_string(),
        start,
        end: open + end,
        option: None,
        shown,
    })
}

/// `![[` … `]]`: a place with a `!` before it, and what follows a `|` (outside the path's
/// quoted names) in it.
fn embed(text: &str, start: usize, to: usize) -> Option<Ref> {
    let open = start + 3;
    let i = closing(text, open, to, "]]")?;
    let inner = trim_spaces(&text[open..i]);
    let (path, option) = match crate::pathtext::find_outside(inner, |k| inner.as_bytes()[k] == b'|')
    {
        Some(bar) => (
            trim_spaces(&inner[..bar]),
            Some(inner[bar + 1..].to_string()),
        ),
        None => (inner, None),
    };
    if !is_path(path) || option.as_deref().is_some_and(|o| o.contains(' ')) {
        return None;
    }
    Some(Ref {
        kind: Kind::Embed,
        path: path.to_string(),
        start,
        end: i + 2,
        option,
        shown: None,
    })
}

fn value(text: &str, start: usize, to: usize) -> Option<Ref> {
    let open = start + 2;
    let close = closing(text, open, to, "}}")?;
    let path = trim_spaces(&text[open..close]);
    if !is_path(path) {
        return None;
    }
    Some(Ref {
        kind: Kind::Value,
        path: path.to_string(),
        start,
        end: close + 2,
        option: None,
        shown: None,
    })
}

// MARK: - Paths as they are written out

/// A path as it is exported: without a tree from before (`main@Core:User.id` →
/// `Core:User.id`).
pub fn exported_path(stored: &str) -> String {
    match crate::pathtext::tree_prefix(stored) {
        Some((_, rest)) => rest.strip_prefix(':').unwrap_or(rest).to_string(),
        None => stored.to_string(),
    }
}

/// A stored path cut where it is written out: the card as the app names it, the card's
/// name, and the fields below it — each as written (quoted where it is).
pub fn plain_parts(stored: &str) -> (String, String, Option<String>) {
    let bare = exported_path(stored);
    let (section, rest) = crate::pathtext::split_section(&bare);
    let head = section.map(|s| format!("{s}:")).unwrap_or_default();
    let name = crate::pathtext::dots(rest)[0];
    if name.len() >= rest.len() {
        return (head + rest, rest.to_string(), None);
    }
    let field = &rest[name.len() + 1..];
    (
        head + name,
        name.to_string(),
        (!field.is_empty()).then(|| field.to_string()),
    )
}

/// For what is written out: the card's name and the fields below it, `Core:User.id` →
/// `User.id`; a section by its own name, `Store/Ordering:` → `Ordering`.
pub fn plain_path(stored: &str) -> String {
    let bare = exported_path(stored);
    if let Some(section) = crate::pathtext::section_only(&bare) {
        return crate::pathtext::slashes(section)
            .last()
            .map(|s| s.trim().to_string())
            .unwrap_or_default();
    }
    let (_, name, field) = plain_parts(stored);
    match field {
        Some(f) => format!("{name}.{f}"),
        None => name,
    }
}

/// A path into a working directory (its first section one of `mounts`, the project tree's
/// `workdir` names, in any case) as it is written out: the path inside the working directory, as
/// the repository it is written into names it — the folders joined by `/`, the file, then `#` and
/// the names inside the file joined by `.`, all unquoted (`repo/src:'a.ts'.Store.save` →
/// `src/a.ts#Store.save`); a folder ends in `/`, the working directory itself is `./`. Made of
/// the names alone, so the same tree writes the same bytes on every machine, whatever its folder
/// holds; none for any other path.
pub fn work_dir_path(stored: &str, mounts: &[String]) -> Option<String> {
    if mounts.is_empty() {
        return None;
    }
    let bare = exported_path(stored);
    let (section, rest) = crate::pathtext::split_section(&bare);
    let mut folders = crate::pathtext::slashes(section?).into_iter();
    let mount = crate::pathtext::unquote(folders.next()?);
    if !mounts.iter().any(|m| crate::pathtext::same(m, &mount)) {
        return None;
    }
    let mut parts: Vec<String> = folders
        .map(|f| crate::pathtext::unquote(f).into_owned())
        .collect();
    let rest = rest.trim();
    if rest.is_empty() {
        return Some(if parts.is_empty() {
            "./".to_string()
        } else {
            format!("{}/", parts.join("/"))
        });
    }
    let names = crate::pathtext::names(rest);
    parts.push(names[0].clone());
    let mut out = parts.join("/");
    if names.len() > 1 {
        out.push('#');
        out.push_str(&names[1..].join("."));
    }
    Some(out)
}

/// What a reference reads as where it is written out: the text shown when one was said, else
/// the card's name and field (`plain_path`); a path into a working directory as the path inside
/// it (`work_dir_path`), after the text shown when one was said.
pub fn shown(r: &Ref, mounts: &[String]) -> String {
    match (work_dir_path(&r.path, mounts), &r.shown) {
        (Some(path), Some(said)) => format!("{said} ({path})"),
        (Some(path), None) => path,
        (None, Some(said)) => said.clone(),
        (None, None) => plain_path(&r.path),
    }
}

/// Free text as it is written out: each reference as the words it is shown by; `mounts` the
/// project's working directories' names (`work_dir_path`).
pub fn plain_note(text: &str, mounts: &[String]) -> String {
    transform(text, |r| Some(shown(r, mounts)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn references_outside_code() {
        let text = "a [[X:Y.z]] `[[no]]` {{X:E}}\n```\n[[in fence]]\n```\n![[C:F|sequence]]";
        let found: Vec<(Kind, String)> = refs(text).into_iter().map(|r| (r.kind, r.path)).collect();
        assert_eq!(
            found,
            vec![
                (Kind::Place, "X:Y.z".into()),
                (Kind::Value, "X:E".into()),
                (Kind::Embed, "C:F".into())
            ]
        );
        assert_eq!(
            plain_note("see [[Core:User.id]] and [[subjects[]]]", &[]),
            "see User.id and subjects[]"
        );
        assert_eq!(
            plain_note("[[Docs:'release notes.md'.'a.b']] [[main@Core:User]]", &[]),
            "'release notes.md'.'a.b' User"
        );
    }

    #[test]
    fn a_quoted_name_is_one_name_to_the_scanner() {
        let text = "[[Docs:'release notes.md']] {{Data:'a}}b'.x}} ![[Docs:'a|b'|sequence]] \
                    [[Docs:'x]]y']] [[not a path]] [[a 'b c']] [[Owner's]]";
        let found: Vec<(Kind, String, Option<String>)> = refs(text)
            .into_iter()
            .map(|r| (r.kind, r.path, r.option))
            .collect();
        assert_eq!(
            found,
            vec![
                (Kind::Place, "Docs:'release notes.md'".into(), None),
                (Kind::Value, "Data:'a}}b'.x".into(), None),
                (Kind::Embed, "Docs:'a|b'".into(), Some("sequence".into())),
                (Kind::Place, "Docs:'x]]y'".into(), None),
                (Kind::Place, "Owner's".into(), None),
            ]
        );
        let renamed = transform(text, |r| {
            (r.path == "Docs:'x]]y'").then(|| r.written("Docs:'x y'"))
        });
        assert!(renamed.contains("[[Docs:'x y']]"), "{renamed}");
    }

    /// The app reads references with its own scanner: both are held to the same table.
    #[test]
    fn every_reference_of_the_case_table_is_found_as_it_says() {
        let table: serde_json::Value =
            serde_json::from_str(include_str!("../tests/fixtures/paths.json")).unwrap();
        let mut wrong = Vec::new();
        for case in table["refs"].as_array().unwrap() {
            let text = case["text"].as_str().unwrap();
            let got: Vec<serde_json::Value> = refs(text)
                .into_iter()
                .map(|r| {
                    let mut o = serde_json::json!({
                        "kind": match r.kind {
                            Kind::Place => "place",
                            Kind::Value => "value",
                            Kind::Embed => "embed",
                        },
                        "path": r.path,
                    });
                    if let Some(s) = r.shown {
                        o["shown"] = s.into();
                    }
                    if let Some(x) = r.option {
                        o["option"] = x.into();
                    }
                    o
                })
                .collect();
            if got != case["refs"].as_array().unwrap().clone() {
                wrong.push(format!("{text:?}: found {got:?}"));
            }
            if let Some(renamed) = case.get("renamed") {
                let (from, to) = (renamed["from"].as_str().unwrap(), renamed["to"].as_str());
                let again = transform(text, |r| (r.path == from).then(|| r.written(to.unwrap())));
                if again != renamed["text"].as_str().unwrap() {
                    wrong.push(format!("{text:?}: renamed {again:?}"));
                }
            }
        }
        for case in table["sectionOnly"].as_array().unwrap() {
            let text = case["text"].as_str().unwrap();
            if crate::pathtext::section_only(text) != case["section"].as_str() {
                wrong.push(format!(
                    "{text:?}: section {:?}",
                    crate::pathtext::section_only(text)
                ));
            }
        }
        assert!(wrong.is_empty(), "{wrong:#?}");
    }

    #[test]
    fn a_link_is_written_out_as_the_text_it_shows() {
        assert_eq!(
            plain_note(
                "[[Core:User|the customer]] in [[Store/Ordering:]] and [[Core:User.id]]",
                &[]
            ),
            "the customer in Ordering and User.id"
        );
        assert_eq!(plain_path("main@Store/'Q&A 2026':"), "'Q&A 2026'");
    }

    #[test]
    fn a_path_into_a_working_directory_is_written_as_the_path_inside_it() {
        let mounts = vec!["repo".to_string()];
        let cases = [
            (
                "repo/src/domain:'project.ts'.canEdit",
                "src/domain/project.ts#canEdit",
            ),
            ("repo/src/domain:'project.ts'", "src/domain/project.ts"),
            ("repo/src/domain:", "src/domain/"),
            ("repo:", "./"),
            ("Repo:'README.md'", "README.md"),
            ("main@repo/src:'a.ts'.Store.save", "src/a.ts#Store.save"),
            (
                "repo/'my dir':'a b.ts'.'can edit'",
                "my dir/a b.ts#can edit",
            ),
        ];
        for (stored, written) in cases {
            assert_eq!(
                work_dir_path(stored, &mounts).as_deref(),
                Some(written),
                "{stored}"
            );
        }
        assert_eq!(work_dir_path("Core:User.id", &mounts), None);
        assert_eq!(work_dir_path("repo", &mounts), None);
        assert_eq!(work_dir_path("repo:'a.ts'", &[]), None);
        assert_eq!(
            plain_note(
                "[[repo/src:'a.ts'.Store.save|保存]] [[repo/src:'a.ts']] [[Core:User.id]]",
                &mounts
            ),
            "保存 (src/a.ts#Store.save) src/a.ts User.id"
        );
    }
}
