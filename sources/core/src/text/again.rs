//! A sentence kept in Japanese alone, said again in the language of the call: the history's lines
//! kept before they were kept by key (`write::history::line_said`) were made from the Japanese
//! table, so one the table's Japanese still makes is read back into its key and values and said
//! anew. A value that is a sentence of its own (a change in a line, the items of a slashed list)
//! is read back the same way; a name, a path or a number is said as it is. What the tables do not
//! make, or make two ways that read differently, is left as it was kept.

use super::{Piece, current, fill, has_japanese, index_of, pieces, table, template};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// How deep a value is read back as a sentence of its own.
const MOST_DEPTH: usize = 4;

/// The names of the values that hold a name, a path or what was written (a card's, a field's value):
/// said as they are, even when the table has a sentence that reads the same.
const AS_WRITTEN: [&str; 24] = [
    "name",
    "names",
    "full",
    "path",
    "display",
    "target",
    "card",
    "cards",
    "section",
    "project",
    "tree",
    "branch",
    "file",
    "folder",
    "symbol",
    "at",
    "into_path",
    "holder",
    "value",
    "values",
    "from",
    "to",
    "now",
    "field",
];

/// The most sentences kept as said: past it, they are all let go.
const MOST_KEPT: usize = 10_000;

/// The Japanese templates that can be read back: each by its key, cut into its pieces, and how much
/// text of its own it has (what tells it from the others).
struct Candidate {
    key: &'static str,
    pieces: Vec<Piece<'static>>,
    weight: usize,
    /// Its own text has Japanese in it: a sentence, not a joining of values.
    worded: bool,
    /// The letter of its own text the fewest templates have: a text without it is not made by it.
    anchor: char,
}

fn candidates() -> &'static [Candidate] {
    static ALL: OnceLock<Vec<Candidate>> = OnceLock::new();
    ALL.get_or_init(|| {
        let ja = table(index_of("ja"));
        let own_of = |text: &str| -> String {
            pieces(text)
                .iter()
                .filter_map(|p| match p {
                    Piece::Text(t) => Some(*t),
                    Piece::Name(_) => None,
                })
                .collect()
        };
        let mut letters: HashMap<char, usize> = HashMap::new();
        for text in ja.values() {
            let mut seen: Vec<char> = own_of(text).chars().collect();
            seen.sort_unstable();
            seen.dedup();
            for c in seen {
                *letters.entry(c).or_default() += 1;
            }
        }
        let mut out: Vec<Candidate> = ja
            .iter()
            .filter_map(|(key, text)| {
                let own = own_of(text);
                // A template of values alone (`{items}`) would take any text for its own.
                if own.trim().is_empty() || own.chars().count() < 2 {
                    return None;
                }
                let anchor = own.chars().min_by_key(|c| (letters[c], *c))?;
                Some(Candidate {
                    key: key.as_str(),
                    weight: own.chars().count(),
                    worded: has_japanese(&own),
                    anchor,
                    pieces: pieces(text),
                })
            })
            .collect();
        out.sort_by(|a, b| b.weight.cmp(&a.weight).then(a.key.cmp(b.key)));
        out
    })
}

/// `japanese`, a sentence the Japanese table makes, said in the language of the call; none when it
/// is said in Japanese already, or the tables do not make it one way only.
pub fn said_again(japanese: &str) -> Option<String> {
    if current() == "ja" || !has_japanese(japanese) {
        return None;
    }
    sentence(japanese, 0, true)
}

/// `kept`, a text one of `keys` made in some language with its values put in as they are (names,
/// paths, numbers), said again in the language of the call; none when none of them makes it. What
/// kept the text is known to hold only these keys' texts, so it is read back whatever language it
/// was kept in.
pub fn said_as(keys: &[&str], kept: &str) -> Option<String> {
    for key in keys {
        for (index, _) in super::LANGUAGES.iter().enumerate() {
            let Some(template) = table(index).get(*key) else {
                continue;
            };
            let pieces = pieces(template);
            if let Some(values) = read_back(&pieces, kept) {
                let template = super::template(key)?;
                return Some(fill(template, |name| {
                    values
                        .iter()
                        .find(|(n, _)| *n == name)
                        .map(|(_, v)| (*v).to_string())
                }));
            }
        }
    }
    None
}

/// What `sentence` said of a text, by the language, whether it was the whole, and the text: a line
/// is read again with every read of the history, and the same changes are in many lines.
type Said = HashMap<(&'static str, bool, String), Option<String>>;

fn said() -> &'static Mutex<Said> {
    static SAID: OnceLock<Mutex<Said>> = OnceLock::new();
    SAID.get_or_init(|| Mutex::new(HashMap::new()))
}

/// `text` read back as one template of the table and said in the language of the call: the
/// template with the most text of its own that makes it, when every such template says it alike.
/// `top`: only a sentence (a template with Japanese of its own) is taken for the whole.
fn sentence(text: &str, depth: usize, top: bool) -> Option<String> {
    let key = (current(), top, text.to_string());
    if let Some(was) = said().lock().unwrap_or_else(|e| e.into_inner()).get(&key) {
        return was.clone();
    }
    let now = sentence_now(text, depth, top);
    let mut held = said().lock().unwrap_or_else(|e| e.into_inner());
    if held.len() >= MOST_KEPT {
        held.clear();
    }
    held.insert(key, now.clone());
    now
}

fn sentence_now(text: &str, depth: usize, top: bool) -> Option<String> {
    // The templates that make it with the most text of their own: (its kind's rank, what it says).
    let mut found: Vec<(u8, String)> = Vec::new();
    let mut weight = 0;
    let letters: std::collections::HashSet<char> = text.chars().collect();
    for candidate in candidates() {
        if (top && !candidate.worded) || !letters.contains(&candidate.anchor) {
            continue;
        }
        if !found.is_empty() && candidate.weight < weight {
            break;
        }
        let Some(values) = read_back(&candidate.pieces, text) else {
            continue;
        };
        let Some(template) = template(candidate.key) else {
            continue;
        };
        let said = fill(template, |name| {
            values
                .iter()
                .find(|(n, _)| *n == name)
                .map(|(n, v)| value(n, v, depth))
        });
        weight = candidate.weight;
        found.push((rank(candidate.key), said));
    }
    // Of those of the first kind, what most of them say (the same joining of values is written
    // under several keys); none when two ways are said as often.
    let first = found.iter().map(|(r, _)| *r).min()?;
    let mut counted: Vec<(String, usize)> = Vec::new();
    for (_, said) in found.into_iter().filter(|(r, _)| *r == first) {
        match counted.iter_mut().find(|(s, _)| *s == said) {
            Some((_, n)) => *n += 1,
            None => counted.push((said, 1)),
        }
    }
    counted.sort_by(|a, b| b.1.cmp(&a.1));
    match counted.as_slice() {
        [(one, _)] => Some(one.clone()),
        [(one, n), (_, m), ..] if n > m => Some(one.clone()),
        _ => None,
    }
}

/// How likely a key's text is to be in a history line: a line's own, a change or a label said in
/// one, then any other.
fn rank(key: &str) -> u8 {
    if key.contains(".line") {
        0
    } else if key.contains(".change") || key.starts_with("common.label.") {
        1
    } else {
        2
    }
}

/// A value put into a sentence, said in the language of the call: a sentence of its own, or a
/// list of them joined as the language joins one; else as it is.
fn value(name: &str, text: &str, depth: usize) -> String {
    if depth >= MOST_DEPTH || !has_japanese(text) || AS_WRITTEN.contains(&name) {
        return text.to_string();
    }
    if let Some(said) = sentence(text, depth + 1, false) {
        return said;
    }
    let separator = super::render("common.list.separator", &[]);
    let Some(japanese_separator) = table(index_of("ja")).get("common.list.separator") else {
        return text.to_string();
    };
    let items: Vec<&str> = text.split(japanese_separator.as_str()).collect();
    if items.len() > 1 {
        let said: Vec<Option<String>> = items
            .iter()
            .map(|item| {
                if has_japanese(item) {
                    sentence(item, depth + 1, false)
                } else {
                    Some(item.to_string())
                }
            })
            .collect();
        // Every item a sentence or a plain value: a list. A name with the separator in it stays whole.
        if said.iter().all(Option::is_some) {
            return said
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(&separator);
        }
    }
    text.to_string()
}

/// The values `pieces` (a template) were filled with to make `text`, by name: none when it does not
/// make it. Each value is some text; one name written twice holds the same.
fn read_back<'p, 't>(pieces: &[Piece<'p>], text: &'t str) -> Option<Vec<(&'p str, &'t str)>> {
    // Its own text, in order, is all in it: the cheap test first.
    let mut rest = text;
    for piece in pieces {
        if let Piece::Text(own) = piece {
            let at = rest.find(own)?;
            rest = &rest[at + own.len()..];
        }
    }
    let mut values = Vec::new();
    fits(pieces, text, &mut values).then_some(values)
}

fn fits<'p, 't>(pieces: &[Piece<'p>], text: &'t str, values: &mut Vec<(&'p str, &'t str)>) -> bool {
    match pieces.first() {
        None => text.is_empty(),
        Some(Piece::Text(own)) => text
            .strip_prefix(own)
            .is_some_and(|rest| fits(&pieces[1..], rest, values)),
        Some(Piece::Name(name)) => {
            let ends: Vec<usize> = match pieces.get(1) {
                // The last piece: the value is the rest.
                None => vec![text.len()],
                // The value runs to one of the places the text after it starts.
                Some(Piece::Text(next)) => text.match_indices(next).map(|(at, _)| at).collect(),
                // Two names side by side: not read back (where one ends cannot be told).
                Some(Piece::Name(_)) => return false,
            };
            for end in ends {
                if end == 0 {
                    continue;
                }
                let said = &text[..end];
                if let Some((_, held)) = values.iter().find(|(n, _)| n == name)
                    && *held != said
                {
                    continue;
                }
                let before = values.len();
                values.push((name, said));
                if fits(&pieces[1..], &text[end..], values) {
                    return true;
                }
                values.truncate(before);
            }
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{List, bind};
    use super::*;

    #[test]
    fn a_line_kept_in_japanese_alone_is_said_in_the_language_of_the_call() {
        let changes = crate::tools::slashed(vec![
            crate::keyed!("cards.changed.body"),
            crate::keyed!("common.label.description"),
        ]);
        let line = crate::keyed!(
            "create.line.updated",
            full = "Docs:Rules",
            changed = changes
        );
        let japanese = line.render_in("ja");
        let _en = bind(Some("en"));
        assert_eq!(
            said_again(&japanese).as_deref(),
            Some(line.render().as_str())
        );
        // A list of names stays as it is.
        let deleted = crate::keyed!("kindTools.line.deleted", names = List::of(["注文", "明細"]));
        let said = said_again(&deleted.render_in("ja")).unwrap();
        assert!(said.contains("注文・明細"), "{said}");
        assert!(!said.contains("削除"), "{said}");
    }

    #[test]
    fn what_the_tables_do_not_make_or_a_call_in_japanese_is_left_as_it_was() {
        let _en = bind(Some("en"));
        assert_eq!(said_again("手で書いた説明"), None);
        assert_eq!(said_again("Updated card A"), None);
        let _ja = bind(Some("ja"));
        assert_eq!(said_again("カード A を作成"), None);
    }
}
