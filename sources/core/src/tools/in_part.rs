//! A body changed or read in part: `card_update` takes, for a body (a Markdown or long text field — a document's, a note's, a
//! goal's or a task's — and a formatdoc's text row), what to replace in it and what to put
//! before or after it, over the text as it is; `card_read` gives the lines where a text is, or
//! the lines asked for, instead of the whole of it. Neither reads a pattern: what is written is
//! what is looked for, character for character — `find` whatever the case, as `card_list`'s
//! `query` looks.

use serde_json::{Map, Value, json};

/// The keys a body changed in part is written with.
const EDIT_KEYS: [&str; 3] = ["edit", "append", "prepend"];

/// The most lines `find` gives around each place it found (`context`).
const MOST_CONTEXT: u64 = 20;

/// What a card or a row read in part keeps beside its body: what names it.
const HEAD_KEYS: [&str; 9] = [
    "project", "branch", "path", "form", "name", "kind", "label", "heading", "key",
];

/// The most places `find` gives; how many there were besides is said.
const MOST_FOUND: usize = 50;

/// What text is changed in part: a body, its lines one after another; a table's cell, one
/// line of its own, what is put before or after it run on.
struct Text {
    what: fn() -> String,
    lines: bool,
}

const BODY: Text = Text {
    what: || text!("inPart.word.body"),
    lines: true,
};

const CELL: Text = Text {
    what: || text!("inPart.word.cell"),
    lines: false,
};

/// What a body becomes when `given` says how to change it in part (`{edit, append, prepend}`
/// over `before`); none when `given` is the text itself, written whole as before. `at` names
/// it in a refusal.
pub(crate) fn changed(before: &str, given: &Value, at: &str) -> Result<Option<String>, String> {
    changed_as(before, given, at, &BODY)
}

/// What a table's cell becomes when `given` says how to change it in part, as a body is: what
/// is put before or after it runs on, with no line break between.
pub(crate) fn cell_changed(
    before: &str,
    given: &Value,
    at: &str,
) -> Result<Option<String>, String> {
    changed_as(before, given, at, &CELL)
}

fn changed_as(
    before: &str,
    given: &Value,
    at: &str,
    text_is: &Text,
) -> Result<Option<String>, String> {
    let Some(said) = given.as_object() else {
        return Ok(None);
    };
    if let Some(stray) = said.keys().find(|k| !EDIT_KEYS.contains(&k.as_str())) {
        return Err(text!("inPart.edit.strayKey", at = at, stray = stray));
    }
    if said.is_empty() {
        return Err(text!("inPart.edit.noWay", at = at));
    }
    let mut text = before.to_string();
    if let Some(edits) = said.get("edit") {
        let Some(edits) = edits.as_array() else {
            return Err(text!("inPart.edit.listShape", at = at));
        };
        for (index, edit) in edits.iter().enumerate() {
            let here = format!("{at}.edit[{index}]");
            text = replaced(&text, edit, &here, &(text_is.what)())?;
        }
    }
    if let Some(head) = said.get("prepend") {
        let head = head
            .as_str()
            .ok_or_else(|| text!("inPart.prepend.text", at = at))?;
        text = if text_is.lines {
            joined(head, &text)
        } else {
            format!("{head}{text}")
        };
    }
    if let Some(tail) = said.get("append") {
        let tail = tail
            .as_str()
            .ok_or_else(|| text!("inPart.append.text", at = at))?;
        text = if text_is.lines {
            joined(&text, tail)
        } else {
            format!("{text}{tail}")
        };
    }
    Ok(Some(text))
}

/// `text` with one edit made: `old` where it is, once (or every time with `all`).
fn replaced(text: &str, edit: &Value, at: &str, what: &str) -> Result<String, String> {
    let Some(edit) = edit.as_object() else {
        return Err(text!("inPart.one.shape", at = at));
    };
    if let Some(stray) = edit
        .keys()
        .find(|k| !["old", "new", "all"].contains(&k.as_str()))
    {
        return Err(text!("inPart.one.strayKey", at = at, stray = stray));
    }
    let old = edit
        .get("old")
        .and_then(Value::as_str)
        .filter(|o| !o.is_empty())
        .ok_or_else(|| text!("inPart.one.oldNeeded", at = at))?;
    let new = edit
        .get("new")
        .and_then(Value::as_str)
        .ok_or_else(|| text!("inPart.one.newNeeded", at = at))?;
    let all = match edit.get("all") {
        None => false,
        Some(Value::Bool(all)) => *all,
        Some(_) => return Err(text!("inPart.one.allBool", at = at)),
    };
    let found = text.matches(old).count();
    match found {
        0 => Err(text!("inPart.one.notFound", at = at, what = what)),
        1 => Ok(text.replacen(old, new, 1)),
        _ if all => Ok(text.replace(old, new)),
        n => Err(text!("inPart.one.manyFound", at = at, what = what, n = n)),
    }
}

/// `head` and `tail` as lines one after the other: a line break between them unless one is
/// there already, or either is empty.
fn joined(head: &str, tail: &str) -> String {
    if head.is_empty() || tail.is_empty() || head.ends_with('\n') {
        format!("{head}{tail}")
    } else {
        format!("{head}\n{tail}")
    }
}

/// Whether a read asks for a body in part.
pub(crate) fn asked(given: &Map<String, Value>) -> bool {
    given.contains_key("find") || given.contains_key("lines")
}

/// A card or a row as `card_read` gives it, with its body in part: the places `find` is in it
/// (each its line, the lines around it), or the lines `lines` names — in place of the whole. Read
/// through MCP (`most`), `lines` without its `to` gives that many lines at most, and says when it
/// stopped short of the end (`truncated`); with no `find` nor `lines`, a body longer than that is
/// cut to its first lines, said the same way (`cut`).
pub(crate) fn read(
    read: &mut Value,
    given: &Map<String, Value>,
    most: Option<usize>,
) -> Result<(), String> {
    read_at(read, given, most, 1)
}

/// The same for a body that is lines of something longer, its first line numbered `first` (a
/// function's lines in its file): the lines asked for and given are numbered so, and those asked
/// for are cut to the body's.
pub(crate) fn read_at(
    read: &mut Value,
    given: &Map<String, Value>,
    most: Option<usize>,
    first: usize,
) -> Result<(), String> {
    if !asked(given) {
        if let Some(most) = most {
            cut_at(read, most, first);
        }
        return Ok(());
    }
    if given.contains_key("find") && given.contains_key("lines") {
        return Err(text!("inPart.read.findAndLines"));
    }
    let Some(out) = read.as_object_mut() else {
        return Ok(());
    };
    let path = out
        .get("path")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let Some(body) = out.get("body").and_then(Value::as_str).map(str::to_string) else {
        // A working directory's file that was not read says why: that, rather than that it has none.
        if let Some(why) = out.get("problem").and_then(Value::as_str) {
            return Err(why.to_string());
        }
        return Err(text!("inPart.read.noBody", path = path));
    };
    let lines: Vec<&str> = body.split('\n').collect();
    // Only what names it beside its body in part: not where it sits, nor what it points at.
    out.retain(|key, _| HEAD_KEYS.contains(&key.as_str()));
    out.insert("lineCount".into(), lines.len().into());
    if given.contains_key("find") {
        let (wanted, around) = looked_for(given)?;
        let hits = found_in(&body, &wanted, around);
        let more = hits.len().saturating_sub(MOST_FOUND);
        let found: Vec<Value> = hits
            .into_iter()
            .take(MOST_FOUND)
            .map(|mut hit| {
                for key in ["line", "to"] {
                    if let Some(n) = hit.get(key).and_then(Value::as_u64) {
                        hit.insert(key.into(), (n as usize + first - 1).into());
                    }
                }
                Value::Object(hit)
            })
            .collect();
        out.insert("found".into(), Value::Array(found));
        if more > 0 {
            out.insert("moreFound".into(), more.into());
        }
        return Ok(());
    }
    let Some(range) = given.get("lines").and_then(Value::as_object) else {
        return Err(text!("inPart.read.linesShape"));
    };
    if let Some(stray) = range.keys().find(|k| !["from", "to"].contains(&k.as_str())) {
        return Err(text!("inPart.read.linesStray", stray = stray));
    }
    let number = |key: &str| -> Result<Option<usize>, String> {
        match range.get(key) {
            None => Ok(None),
            Some(n) => n
                .as_u64()
                .filter(|n| *n >= 1)
                .map(|n| Some(n as usize))
                .ok_or_else(|| text!("inPart.read.linesNumber", key = key)),
        }
    };
    let said_from = number("from")?.ok_or_else(|| text!("inPart.read.fromNeeded"))?;
    let said_to = number("to")?;
    if said_to.is_some_and(|to| to < said_from) {
        return Err(text!("inPart.read.toBefore"));
    }
    let last = first + lines.len() - 1;
    if said_from > last {
        return Err(if first == 1 {
            text!("inPart.read.fromPast", count = lines.len())
        } else {
            text!("inPart.read.fromPastLines", first, last)
        });
    }
    // In the body's lines, from 1.
    let from = said_from.max(first) - first + 1;
    let to = said_to
        .map(|to| to.saturating_sub(first - 1))
        .or_else(|| most.map(|m| from + m - 1))
        .unwrap_or(lines.len())
        .min(lines.len());
    if to < from {
        return Err(text!("inPart.read.toBefore"));
    }
    if most.is_some() && to < lines.len() && range.get("to").is_none() {
        out.insert("truncated".into(), true.into());
    }
    out.insert(
        "lines".into(),
        json!({"from": from + first - 1, "to": to + first - 1}),
    );
    out.insert("body".into(), lines[from - 1..to].join("\n").into());
    Ok(())
}

/// A card or a row with no body of its own read with `find` over the words written inside it
/// (`places`: a document's rows and versions, a flow's nodes): each place found, by the path
/// of what it is in and where in it, with its line and the lines around it, in place of the
/// whole.
pub(crate) fn find_in_places(
    read: &mut Value,
    given: &Map<String, Value>,
    places: &[super::texts::Place],
) -> Result<(), String> {
    if given.contains_key("lines") {
        return Err(text!("inPart.read.findAndLines"));
    }
    let (wanted, around) = looked_for(given)?;
    let Some(out) = read.as_object_mut() else {
        return Ok(());
    };
    out.retain(|key, _| HEAD_KEYS.contains(&key.as_str()));
    let mut found: Vec<Value> = Vec::new();
    let mut more = 0;
    for place in places {
        for hit in found_in(&place.text, &wanted, around) {
            if found.len() == MOST_FOUND {
                more += 1;
                continue;
            }
            let mut said = Map::new();
            said.insert("path".into(), place.path.clone().into());
            said.insert("where".into(), place.place.clone().into());
            said.extend(hit);
            found.push(Value::Object(said));
        }
    }
    out.insert("found".into(), Value::Array(found));
    if more > 0 {
        out.insert("moreFound".into(), more.into());
    }
    Ok(())
}

/// What `find` looks for and how many lines it gives around each place.
fn looked_for(given: &Map<String, Value>) -> Result<(String, usize), String> {
    let wanted = given
        .get("find")
        .and_then(Value::as_str)
        .filter(|w| !w.is_empty())
        .ok_or_else(|| text!("inPart.read.findText"))?;
    let around = match given.get("context") {
        None => 2,
        Some(n) => n
            .as_u64()
            .filter(|n| *n <= MOST_CONTEXT)
            .ok_or_else(|| text!("inPart.read.contextRange", most = MOST_CONTEXT))?,
    } as usize;
    Ok((wanted.to_string(), around))
}

/// The lines of `body` where `wanted` is, as `card_list`'s `query` looks — whatever the case —
/// each its line (from 1), the line it ends on when that is another (`to`), the lines, and
/// `around` lines before and after; a line once, however many times it is in it. Lowering the
/// case keeps every line break, so the lines found in the lowered text are the text's.
fn found_in(body: &str, wanted: &str, around: usize) -> Vec<Map<String, Value>> {
    let lower = body.to_lowercase();
    let wanted = wanted.to_lowercase();
    if wanted.is_empty() {
        return Vec::new();
    }
    let lines: Vec<&str> = body.split('\n').collect();
    let line_at = |byte: usize| lower[..byte].matches('\n').count();
    // A line once, however many times it is in it.
    let mut starts: Vec<usize> = Vec::new();
    for (at, _) in lower.match_indices(wanted.as_str()) {
        if starts
            .last()
            .is_none_or(|&before| line_at(before) != line_at(at))
        {
            starts.push(at);
        }
    }
    // Where its last character begins: a boundary to cut at.
    let last_char = wanted.chars().last().map_or(1, char::len_utf8);
    starts
        .iter()
        .map(|&start| {
            let first = line_at(start);
            let last = line_at(start + wanted.len() - last_char);
            let mut hit = Map::new();
            hit.insert("line".into(), (first + 1).into());
            if last > first {
                hit.insert("to".into(), (last + 1).into());
            }
            hit.insert("text".into(), lines[first..=last].join("\n").into());
            let before = &lines[first.saturating_sub(around)..first];
            if !before.is_empty() {
                hit.insert("before".into(), json!(before));
            }
            let after = &lines[(last + 1).min(lines.len())..(last + 1 + around).min(lines.len())];
            if !after.is_empty() {
                hit.insert("after".into(), json!(after));
            }
            hit
        })
        .collect()
}

/// A card or a row as `card_read` gives it through MCP: a body of more than `most` lines cut to its first `most`, with how many
/// lines it has (`lineCount`), the lines given (`lines`, read on with the value's `lines`) and
/// `truncated`, put where the body is.
pub(crate) fn cut(read: &mut Value, most: usize) {
    cut_at(read, most, 1)
}

/// The same for a body whose first line is numbered `first`.
fn cut_at(read: &mut Value, most: usize, first: usize) {
    let Some(out) = read.as_object_mut() else {
        return;
    };
    let Some(body) = out.get("body").and_then(Value::as_str) else {
        return;
    };
    let lines: Vec<&str> = body.split('\n').collect();
    if lines.len() <= most {
        return;
    }
    let count = lines.len();
    let kept = lines[..most].join("\n");
    let mut said = Map::new();
    for (key, value) in std::mem::take(out) {
        if key == "body" {
            said.insert("truncated".into(), true.into());
            said.insert("lineCount".into(), count.into());
            said.insert(
                "lines".into(),
                json!({"from": first, "to": first + most - 1}),
            );
            said.insert("body".into(), kept.clone().into());
        } else {
            said.insert(key, value);
        }
    }
    *out = said;
}

// MARK: - A body changed in part, answered short

/// The lines shown around each place a body changed.
const CHANGE_CONTEXT: usize = 2;

/// The most lines shown of one place changed: its first and last half of them, the rest counted.
const MOST_SHOWN: usize = 6;

/// The most places changed shown; how many there were besides is said.
const MOST_CHANGES: usize = 20;

/// The most lines either side of what differs that are compared line by line; past that, what
/// differs is one place.
const MOST_COMPARED: usize = 400;

/// Whether `given` changes a body in part (`{edit, append, prepend}`), not writes it whole.
pub(crate) fn in_part(given: &Value) -> bool {
    given
        .as_object()
        .is_some_and(|o| !o.is_empty() && o.keys().all(|k| EDIT_KEYS.contains(&k.as_str())))
}

/// A body changed in part as MCP is answered with it: not the whole, but how long it is now (`lineCount`,
/// `chars`) and each place it changed (`changed`, as `find` gives the places it found: `line`, `to`,
/// `text`, `before`, `after`; a place only taken away says how many lines, `removed`).
pub(crate) fn quiet(before: &str, after: &str) -> Value {
    let old: Vec<&str> = before.split('\n').collect();
    let new: Vec<&str> = after.split('\n').collect();
    let mut places = changed_places(&old, &new);
    // Places close together shown as one.
    let mut joined: Vec<(usize, usize, usize)> = Vec::new();
    for place in places.drain(..) {
        match joined.last_mut() {
            Some(last) if place.0 <= last.1 + 2 * CHANGE_CONTEXT => {
                last.1 = last.1.max(place.1);
                last.2 += place.2;
            }
            _ => joined.push(place),
        }
    }
    let shown: Vec<Value> = joined
        .iter()
        .take(MOST_CHANGES)
        .map(|&(from, to, removed)| {
            let mut place = Map::new();
            place.insert("line".into(), (from + 1).into());
            if to > from + 1 {
                place.insert("to".into(), to.into());
            }
            let lines = &new[from..to];
            if lines.is_empty() {
                place.insert("removed".into(), removed.into());
            } else if lines.len() > MOST_SHOWN {
                let half = MOST_SHOWN / 2;
                let mut text: Vec<&str> = lines[..half].to_vec();
                text.push("…");
                text.extend(&lines[lines.len() - half..]);
                place.insert("text".into(), text.join("\n").into());
                place.insert("omitted".into(), (lines.len() - MOST_SHOWN).into());
            } else {
                place.insert("text".into(), lines.join("\n").into());
            }
            let around_before = &new[from.saturating_sub(CHANGE_CONTEXT)..from];
            if !around_before.is_empty() {
                place.insert("before".into(), json!(around_before));
            }
            let around_after = &new[to.min(new.len())..(to + CHANGE_CONTEXT).min(new.len())];
            if !around_after.is_empty() {
                place.insert("after".into(), json!(around_after));
            }
            Value::Object(place)
        })
        .collect();
    let mut out = Map::new();
    out.insert("lineCount".into(), new.len().into());
    out.insert("chars".into(), after.chars().count().into());
    out.insert("changed".into(), Value::Array(shown));
    if joined.len() > MOST_CHANGES {
        out.insert("moreChanged".into(), (joined.len() - MOST_CHANGES).into());
    }
    Value::Object(out)
}

/// An answer's `key` (a body changed in part from `before` to `after`) said short (`quiet`): in place of
/// the text, or beside the rest when the answer leaves the text out (a card's).
pub(crate) fn quieted(answer: &mut Value, key: &str, before: &str, after: &str) {
    if let Some(out) = answer.as_object_mut() {
        out.insert(key.to_string(), quiet(before, after));
    }
}

/// Where `new` differs from `old`, by its lines: each place as the lines of `new` it is (`from..to`,
/// none for lines only taken away) and how many lines of `old` it stands for.
fn changed_places(old: &[&str], new: &[&str]) -> Vec<(usize, usize, usize)> {
    let head = old.iter().zip(new).take_while(|(a, b)| a == b).count();
    let tail = old[head..]
        .iter()
        .rev()
        .zip(new[head..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    let (old_mid, new_mid) = (&old[head..old.len() - tail], &new[head..new.len() - tail]);
    if old_mid.is_empty() && new_mid.is_empty() {
        return Vec::new();
    }
    if old_mid.len() > MOST_COMPARED || new_mid.len() > MOST_COMPARED {
        return vec![(head, head + new_mid.len(), old_mid.len())];
    }
    // The longest run in common, line by line, and what is not in it, place by place.
    let (n, m) = (old_mid.len(), new_mid.len());
    let mut common = vec![vec![0u32; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            common[i][j] = if old_mid[i] == new_mid[j] {
                common[i + 1][j + 1] + 1
            } else {
                common[i + 1][j].max(common[i][j + 1])
            };
        }
    }
    let mut places = Vec::new();
    let (mut i, mut j) = (0, 0);
    let mut open: Option<(usize, usize)> = None;
    while i < n || j < m {
        if i < n && j < m && old_mid[i] == new_mid[j] {
            if let Some((from, removed)) = open.take() {
                places.push((head + from, head + j, removed));
            }
            i += 1;
            j += 1;
        } else if j < m && (i == n || common[i][j + 1] >= common[i + 1][j]) {
            open.get_or_insert((j, 0));
            j += 1;
        } else {
            open.get_or_insert((j, 0)).1 += 1;
            i += 1;
        }
    }
    if let Some((from, removed)) = open {
        places.push((head + from, head + m, removed));
    }
    places
}

#[cfg(test)]
mod tests {
    use super::*;

    fn change(before: &str, given: Value) -> Result<String, String> {
        changed(before, &given, "body").map(Option::unwrap)
    }

    #[test]
    fn a_body_changed_in_part_is_said_by_where_it_changed() {
        let before: String = (1..=50).map(|i| format!("行{i}\n")).collect();
        let after = before.replace("行10\n", "十行目\n") + "足した\n";
        let said = quiet(&before, &after);
        assert_eq!(said["lineCount"], 52);
        assert_eq!(said["chars"], after.chars().count());
        let places = said["changed"].as_array().unwrap();
        assert_eq!(places.len(), 2, "{said}");
        assert_eq!(places[0]["line"], 10);
        assert_eq!(places[0]["text"], "十行目");
        assert_eq!(places[0]["before"], json!(["行8", "行9"]));
        assert_eq!(places[0]["after"], json!(["行11", "行12"]));
        assert_eq!(places[1]["line"], 51);
        assert_eq!(places[1]["text"], "足した");
        // Lines only taken away.
        let shorter = before.replace("行20\n行21\n", "");
        let said = quiet(&before, &shorter);
        assert_eq!(said["changed"][0]["removed"], 2);
        assert_eq!(said["changed"][0]["line"], 20);
        // Many lines put in: the first and the last of them.
        let long: String = (1..=30).map(|i| format!("新{i}\n")).collect();
        let said = quiet(&before, &(before.clone() + &long));
        assert_eq!(said["changed"][0]["omitted"], 24);
        assert_eq!(
            said["changed"][0]["text"],
            "新1\n新2\n新3\n…\n新28\n新29\n新30"
        );
    }

    #[test]
    fn a_body_written_whole_is_left_to_be_written_so() {
        assert_eq!(changed("前", &json!("後"), "body").unwrap(), None);
    }

    #[test]
    fn an_edit_replaces_what_is_there_once_and_in_order() {
        let before = "# 見出し\n\n- 一\n- 二\n";
        let after = change(
            before,
            json!({"edit": [{"old": "- 一", "new": "- いち"}, {"old": "- いち\n- 二", "new": "- いち\n- に"}]}),
        )
        .unwrap();
        assert_eq!(after, "# 見出し\n\n- いち\n- に\n");
    }

    #[test]
    fn an_edit_that_is_not_there_or_is_there_twice_is_refused() {
        let before = "a b a";
        assert!(
            change(before, json!({"edit": [{"old": "c", "new": "d"}]}))
                .unwrap_err()
                .contains("ありません")
        );
        assert!(
            change(before, json!({"edit": [{"old": "a", "new": "d"}]}))
                .unwrap_err()
                .contains("2 か所")
        );
        assert_eq!(
            change(
                before,
                json!({"edit": [{"old": "a", "new": "d", "all": true}]})
            )
            .unwrap(),
            "d b d"
        );
        assert!(
            change(before, json!({"edit": [{"old": "", "new": "d"}]}))
                .unwrap_err()
                .contains("old に置き換える文字が要ります")
        );
        assert!(
            change(before, json!({"edit": [{"old": "b"}]}))
                .unwrap_err()
                .contains("new に置き換えた後の文字が要ります")
        );
    }

    #[test]
    fn what_is_put_before_and_after_is_a_line_of_its_own() {
        assert_eq!(
            change("一行目", json!({"append": "二行目", "prepend": "零行目"})).unwrap(),
            "零行目\n一行目\n二行目"
        );
        assert_eq!(
            change("一行目\n", json!({"append": "二行目"})).unwrap(),
            "一行目\n二行目"
        );
        assert_eq!(change("", json!({"append": "最初"})).unwrap(), "最初");
        assert!(
            change("a", json!({"replace": "b"}))
                .unwrap_err()
                .contains("replace")
        );
        assert!(change("a", json!({})).unwrap_err().contains("変え方"));
    }

    fn reading(body: &str, given: Value) -> Result<Value, String> {
        let mut card = json!({"path": "Docs:A", "topLeft": {"x": 0, "y": 0}, "body": body});
        read(&mut card, given.as_object().unwrap(), None)?;
        Ok(card)
    }

    #[test]
    fn find_gives_each_place_with_the_lines_around_it() {
        let body = "一\n二\n三 探す\n四\n五\n六 探す\n七";
        let got = reading(body, json!({"find": "探す", "context": 1})).unwrap();
        assert_eq!(got["lineCount"], 7);
        assert!(got.get("body").is_none());
        assert!(got.get("topLeft").is_none(), "{got}");
        assert_eq!(
            got["found"],
            json!([
                {"line": 3, "text": "三 探す", "before": ["二"], "after": ["四"]},
                {"line": 6, "text": "六 探す", "before": ["五"], "after": ["七"]},
            ])
        );
        // Across lines: where it begins and ends.
        let got = reading(body, json!({"find": "四\n五", "context": 0})).unwrap();
        assert_eq!(
            got["found"],
            json!([{"line": 4, "to": 5, "text": "四\n五"}])
        );
        assert_eq!(
            reading(body, json!({"find": "無い"})).unwrap()["found"],
            json!([])
        );
        // Twice on a line, the line once.
        let got = reading("a a\nb", json!({"find": "a", "context": 0})).unwrap();
        assert_eq!(got["found"], json!([{"line": 1, "text": "a a"}]));
    }

    #[test]
    fn lines_gives_those_lines_only() {
        let body = "一\n二\n三\n四";
        let got = reading(body, json!({"lines": {"from": 2, "to": 3}})).unwrap();
        assert_eq!(got["body"], "二\n三");
        assert_eq!(got["lines"], json!({"from": 2, "to": 3}));
        assert_eq!(got["lineCount"], 4);
        assert_eq!(
            reading(body, json!({"lines": {"from": 3}})).unwrap()["body"],
            "三\n四"
        );
        assert!(reading(body, json!({"lines": {"from": 5}})).is_err());
        assert!(reading(body, json!({"lines": {"from": 3, "to": 2}})).is_err());
        assert!(reading(body, json!({"lines": {"from": 1}, "find": "一"})).is_err());
        let mut entity = json!({"path": "Core:User"});
        assert!(
            read(&mut entity, json!({"find": "x"}).as_object().unwrap(), None)
                .unwrap_err()
                .contains("本文がありません")
        );
    }
}
