//! What MCP is answered with, cut to a size. A list read through MCP
//! is given up to its default count — `LIST_LIMIT`, or the tool's own (`card_list`'s 20, the history's
//! 50) — and no more than `MOST_CHARS` characters of it; it is cut, never refused. What is cut says so
//! (`truncated`), with how many there are (`matched`), how many it gave (`returned`) and where to go
//! on from (`next`, given back as the value's `after`): the last item given, by its key — a path, mostly
//! — not a count, so what another session writes between the calls shifts nothing. Items of one key in a
//! row go together. An `after` no longer in the list (the item was let go of, renamed or moved) goes on
//! from where it would stand when the list is in the order of its keys, and from the top, said so,
//! when it is not. A body (`card_read`) is cut to `BODY_LINES` lines, read on with `lines`.
//!
//! The window's requests are not cut: the window reads whole (`Args::paged` is MCP's alone).

use crate::ojson;
use serde_json::{Map, Value, json};

/// A list's default count, when the tool has none of its own.
pub(crate) const LIST_LIMIT: usize = 100;

/// The most a list's count may be asked to be, when the tool has none of its own.
pub(crate) const MOST_LIMIT: usize = 1000;

/// The most characters a list is given in (its items as written in the answer).
pub(crate) const MOST_CHARS: usize = 40_000;

/// The most lines a body is given in, when the lines to read are not said.
pub(crate) const BODY_LINES: usize = 400;

/// What a call asks of a list: how many, and from after which.
#[derive(Clone, Debug, Default)]
pub(crate) struct Page {
    pub limit: usize,
    pub after: Option<String>,
}

impl Page {
    /// The value's `limit` (`default` when there is none, `1..=most`) and `after` (a key, as a
    /// string or a number), taken out of it.
    pub fn take(
        value: &mut Map<String, Value>,
        default: usize,
        most: usize,
    ) -> Result<Page, String> {
        let limit = match value.shift_remove("limit") {
            None => default,
            Some(n) => match n.as_f64() {
                Some(f) if f.fract() == 0.0 && f >= 1.0 && f <= most as f64 => f as usize,
                Some(f) => return Err(text!("paging.limit.range", most, f)),
                None => return Err(text!("paging.limit.count", most)),
            },
        };
        let after = after_of(value.shift_remove("after"))?;
        Ok(Page { limit, after })
    }
}

/// An `after` as a call writes it: the key of the last one read, as a string or a number.
pub(crate) fn after_of(raw: Option<Value>) -> Result<Option<String>, String> {
    match raw {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s)),
        Some(Value::Number(n)) => Ok(Some(n.to_string())),
        Some(_) => Err(text!("paging.after.shape")),
    }
}

/// How a list is in order, for an `after` no longer in it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Order {
    /// As it stands (a tree's, by position): an `after` gone is gone on from the top.
    Listed,
    /// By its keys, as text: an `after` gone is gone on from the first that comes after it.
    Ascending,
    /// By its keys as numbers, the largest first (a history's commits, the MRs).
    Descending,
}

impl Order {
    /// Where `after`, not among `keys`, would stand: the first that comes after it.
    fn place(self, keys: &[String], after: &str) -> Option<usize> {
        match self {
            Order::Listed => None,
            Order::Ascending => Some(
                keys.iter()
                    .position(|k| k.as_str() > after)
                    .unwrap_or(keys.len()),
            ),
            Order::Descending => {
                let after: f64 = after.trim().parse().ok()?;
                Some(
                    keys.iter()
                        .position(|k| k.trim().parse::<f64>().is_ok_and(|k| k < after))
                        .unwrap_or(keys.len()),
                )
            }
        }
    }
}

/// What a list is cut to: the items `start..end` of it, and how an `after` not in it was read.
#[derive(Clone, Debug)]
pub(crate) struct Cut {
    pub start: usize,
    pub end: usize,
    pub total: usize,
    /// The key `next` names, when the list goes on after `end`.
    pub next: Option<String>,
    pub lost: Option<Lost>,
}

/// An `after` that is not in the list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Lost {
    /// Gone on from where it would stand.
    Placed(String),
    /// Gone on from the top.
    FromTop(String),
}

impl Cut {
    /// Whether it leaves nothing out and was asked from the top: the answer as it was.
    pub fn whole(&self) -> bool {
        self.start == 0 && self.end == self.total && self.lost.is_none()
    }

    /// What it says beside the list, when it leaves some out: `truncated`, `matched`, `returned`
    /// and `next`.
    pub fn said(&self) -> Map<String, Value> {
        let mut out = Map::new();
        if let Some(next) = &self.next {
            out.insert("truncated".into(), true.into());
            out.insert("matched".into(), self.total.into());
            out.insert("returned".into(), (self.end - self.start).into());
            out.insert("next".into(), next.clone().into());
        }
        out
    }

    /// What an `after` not in the list is told.
    pub fn warning(&self) -> Option<String> {
        match &self.lost {
            None => None,
            Some(Lost::Placed(after)) => Some(text!("paging.after.gone", after)),
            Some(Lost::FromTop(after)) => Some(text!("paging.after.goneFromTop", after)),
        }
    }
}

/// The part of a list `page` asks for, by its items' keys in order: from after `page.after` (past the
/// items of its key in a row), up to `page.limit` items and `room` characters — `size` says an item's —
/// a run of items of one key whole, and at least one run.
pub(crate) fn window(
    keys: &[String],
    page: &Page,
    order: Order,
    room: usize,
    mut size: impl FnMut(usize) -> usize,
) -> Cut {
    let total = keys.len();
    let (start, lost) = match &page.after {
        None => (0, None),
        Some(after) => match keys.iter().position(|k| k == after) {
            Some(at) => (run_end(keys, at), None),
            None => match order.place(keys, after) {
                Some(at) => (at, Some(Lost::Placed(after.clone()))),
                None => (0, Some(Lost::FromTop(after.clone()))),
            },
        },
    };
    let mut end = start;
    let mut used = 0;
    while end < total {
        let until = run_end(keys, end);
        let cost: usize = (end..until).map(&mut size).sum();
        if end > start && (until - start > page.limit || used + cost > room) {
            break;
        }
        used += cost;
        end = until;
    }
    Cut {
        start,
        end,
        total,
        next: (end < total).then(|| keys[end - 1].clone()),
        lost,
    }
}

/// Past the items of the key at `at`, in a row.
fn run_end(keys: &[String], at: usize) -> usize {
    let mut end = at + 1;
    while end < keys.len() && keys[end] == keys[at] {
        end += 1;
    }
    end
}

/// The characters an item takes in the answer (packed, as MCP sends it).
pub(crate) fn chars(value: &Value) -> usize {
    ojson::compact(value).chars().count() + 1
}

/// What an item of a list is known by: itself, when it is a word; else the first of its keys that
/// names it.
pub(crate) fn key_of(item: &Value, field: Option<&str>) -> String {
    const NAMING: [&str; 12] = [
        "at", "path", "card", "document", "section", "kind", "line", "name", "tag", "branch",
        "number", "project",
    ];
    match item {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Object(o) => field
            .into_iter()
            .chain(NAMING)
            .find_map(|k| match o.get(k) {
                Some(Value::String(s)) => Some(s.clone()),
                Some(Value::Number(n)) => Some(n.to_string()),
                _ => None,
            })
            .unwrap_or_else(|| ojson::compact(item)),
        other => ojson::compact(other),
    }
}

/// Where the lists of an answer are.
#[derive(Clone, Debug)]
pub(crate) enum Lists {
    /// One list, at this key, its items known by this field.
    One(&'static str, Option<&'static str>),
    /// These lists, one after the other, each item known by its list's name and its key
    /// (`tags 中核`).
    Several(&'static [&'static str]),
    /// Every list at the top of the answer but its warnings and problems, one after the other.
    Every,
    /// A list in each item of a list (an export's files): each known by the outer item's key and
    /// its own (`docs api.md`).
    Inside {
        outer: &'static str,
        outer_key: &'static str,
        inner: &'static str,
        inner_key: &'static str,
    },
}

/// How a tool's answer is cut: where its lists are, in what order, and how many by default.
#[derive(Clone, Debug)]
pub(crate) struct Spec {
    pub lists: Lists,
    pub order: Order,
    pub limit: usize,
    pub most: usize,
}

impl Spec {
    fn new(lists: Lists) -> Spec {
        Spec {
            lists,
            order: Order::Listed,
            limit: LIST_LIMIT,
            most: MOST_LIMIT,
        }
    }

    fn ordered(mut self, order: Order) -> Spec {
        self.order = order;
        self
    }
}

/// How the answer of the tool `name` called with `args` is cut through MCP, when it holds lists the
/// core cuts after the answer is made (`card_list`, a history and a body are cut as they are made).
pub(crate) fn spec_of(name: &str, args: &super::Args) -> Option<Spec> {
    let value = args.value().unwrap_or_default();
    let asked = value.get("as").and_then(Value::as_str);
    let targets = args.targets().ok().flatten().filter(|t| !t.is_empty());
    match name {
        "project_list" => {
            Some(Spec::new(Lists::One("projects", Some("project"))).ordered(Order::Ascending))
        }
        "section_list" => Some(Spec::new(Lists::One("sections", Some("section")))),
        "section_read" if asked.is_none() => {
            Some(Spec::new(Lists::Several(&["cards", "sections", "places"])))
        }
        "tag_list" => Some(Spec::new(Lists::Several(&[
            "tags",
            "groups",
            "declared",
            "ungrouped",
        ]))),
        "mr_list" => Some(Spec::new(Lists::One("mrs", Some("number"))).ordered(Order::Descending)),
        "branch_list" => Some(Spec::new(Lists::One("branches", Some("branch")))),
        "kind_read" if targets.is_none() => {
            Some(Spec::new(Lists::Several(&["cards", "sections", "lines"])))
        }
        "export_read" if !value.contains_key("files") => Some(Spec::new(match targets {
            None => Lists::One("exports", Some("name")),
            Some(_) => Lists::Inside {
                outer: "exports",
                outer_key: "name",
                inner: "files",
                inner_key: "path",
            },
        })),
        // A branch's changes read with their contents are cut as they are made, by their paths.
        "branch_read" if matches!(asked, None | Some("diff")) && !value.contains_key("path") => {
            Some(Spec::new(Lists::Every))
        }
        // Only the items: the groups, an object, are the whole tree's. Each known by its id, which
        // no other item has.
        "inspection_list" => Some(Spec::new(Lists::One("items", Some("id")))),
        _ => None,
    }
}

/// One item of an answer's lists, flattened: which list (by its place among them), which item of it,
/// and, for a list inside a list, which outer item.
#[derive(Clone, Copy, Debug)]
struct At {
    list: usize,
    outer: usize,
    item: usize,
}

/// The answer cut as `spec` and `page` say. An answer that leaves nothing out is as it was. A list
/// that is an object (the boxes by their paths) is cut by its keys, as a list of them is.
pub(crate) fn cut_answer(answer: Value, spec: &Spec, page: &Page) -> Value {
    let Value::Object(mut body) = answer else {
        return answer;
    };
    let Lists::Several(names) = &spec.lists else {
        return cut_lists(Value::Object(body), spec, page);
    };
    let keyed: Vec<&str> = names
        .iter()
        .copied()
        .filter(|n| body.get(*n).is_some_and(Value::is_object))
        .collect();
    if keyed.is_empty() {
        return cut_lists(Value::Object(body), spec, page);
    }
    for name in &keyed {
        if let Some(Value::Object(entries)) = body.get_mut(*name) {
            let wrapped: Vec<Value> = std::mem::take(entries)
                .into_iter()
                .map(|(path, entry)| json!({ "path": path, "entry": entry }))
                .collect();
            body.insert((*name).into(), Value::Array(wrapped));
        }
    }
    let Value::Object(mut cut) = cut_lists(Value::Object(body), spec, page) else {
        unreachable!("an object is cut to an object");
    };
    for name in &keyed {
        if let Some(Value::Array(wrapped)) = cut.get_mut(*name) {
            let entries: Map<String, Value> = std::mem::take(wrapped)
                .into_iter()
                .filter_map(|w| {
                    Some((
                        w.get("path")?.as_str()?.to_string(),
                        w.get("entry")?.clone(),
                    ))
                })
                .collect();
            cut.insert((*name).into(), Value::Object(entries));
        }
    }
    Value::Object(cut)
}

fn cut_lists(answer: Value, spec: &Spec, page: &Page) -> Value {
    let Value::Object(mut body) = answer else {
        return answer;
    };
    // The lists, by name, in order. What is said of the call (warnings, problems) goes whole
    // with every page.
    let names: Vec<String> = match &spec.lists {
        Lists::One(name, _) => vec![name.to_string()],
        Lists::Several(names) => names.iter().map(|n| n.to_string()).collect(),
        Lists::Every => body
            .iter()
            .filter(|(k, v)| v.is_array() && !["warnings", "problems"].contains(&k.as_str()))
            .map(|(k, _)| k.clone())
            .collect(),
        Lists::Inside { outer, .. } => vec![outer.to_string()],
    };
    let several = matches!(spec.lists, Lists::Several(_) | Lists::Every);
    let mut keys: Vec<String> = Vec::new();
    let mut ats: Vec<At> = Vec::new();
    let mut sizes: Vec<usize> = Vec::new();
    // For a list inside a list: where each outer item's own begin among the flattened.
    let mut outer_starts: Vec<usize> = Vec::new();
    for (list, name) in names.iter().enumerate() {
        let Some(items) = body.get(name).and_then(Value::as_array) else {
            continue;
        };
        match &spec.lists {
            Lists::Inside {
                outer_key,
                inner,
                inner_key,
                ..
            } => {
                for (outer, item) in items.iter().enumerate() {
                    outer_starts.push(keys.len());
                    let head = key_of(item, Some(outer_key));
                    for (index, file) in item
                        .get(*inner)
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                        .enumerate()
                    {
                        keys.push(format!("{head} {}", key_of(file, Some(inner_key))));
                        ats.push(At {
                            list,
                            outer,
                            item: index,
                        });
                        sizes.push(chars(file));
                    }
                }
            }
            lists => {
                let field = match lists {
                    Lists::One(_, field) => *field,
                    _ => None,
                };
                for (index, item) in items.iter().enumerate() {
                    let key = key_of(item, field);
                    keys.push(if several {
                        format!("{name} {key}")
                    } else {
                        key
                    });
                    ats.push(At {
                        list,
                        outer: 0,
                        item: index,
                    });
                    sizes.push(chars(item));
                }
            }
        }
    }
    // What the rest of the answer takes, its lists emptied.
    let rest = {
        let mut bare = body.clone();
        for name in &names {
            if let Some(Value::Array(items)) = bare.get_mut(name) {
                if let Lists::Inside { inner, .. } = &spec.lists {
                    for item in items.iter_mut() {
                        if let Some(Value::Array(files)) = item.get_mut(*inner) {
                            files.clear();
                        }
                    }
                } else {
                    items.clear();
                }
            }
        }
        chars(&Value::Object(bare))
    };
    let room = MOST_CHARS.saturating_sub(rest + 200).max(1);
    let cut = window(&keys, page, spec.order, room, |i| sizes[i]);
    if cut.whole() {
        return Value::Object(body);
    }
    // What this page gives, by where each is.
    let shown: std::collections::HashSet<(usize, usize, usize)> = ats[cut.start..cut.end]
        .iter()
        .map(|a| (a.list, a.outer, a.item))
        .collect();
    let total = keys.len();
    for (list, name) in names.iter().enumerate() {
        let Some(Value::Array(items)) = body.get_mut(name) else {
            continue;
        };
        match &spec.lists {
            Lists::Inside { inner, .. } => {
                let all = std::mem::take(items);
                *items = all
                    .into_iter()
                    .enumerate()
                    .filter_map(|(outer, mut item)| {
                        let files = item.get_mut(*inner).and_then(Value::as_array_mut);
                        let had = files.as_ref().is_some_and(|f| !f.is_empty());
                        // An outer item with nothing inside stands where its first would be: given
                        // on the page that reaches there.
                        let begins = outer_starts[outer];
                        let empty_here =
                            !had && cut.start <= begins && (begins < cut.end || cut.end == total);
                        let mut any = false;
                        if let Some(files) = files {
                            let all = std::mem::take(files);
                            *files = all
                                .into_iter()
                                .enumerate()
                                .filter(|(i, _)| shown.contains(&(list, outer, *i)))
                                .map(|(_, f)| f)
                                .collect();
                            any = !files.is_empty();
                        }
                        (any || empty_here).then_some(item)
                    })
                    .collect();
            }
            _ => {
                let all = std::mem::take(items);
                *items = all
                    .into_iter()
                    .enumerate()
                    .filter(|(i, _)| shown.contains(&(list, 0, *i)))
                    .map(|(_, v)| v)
                    .collect();
            }
        }
    }
    // A list with nothing on this page is left out, unless it is the only one.
    if names.len() > 1 {
        body.retain(|k, v| !(names.contains(k) && v.as_array().is_some_and(Vec::is_empty)));
    }
    let said = cut.said();
    let mut out = Map::new();
    let mut put = false;
    for (k, v) in body {
        if !put && names.contains(&k) {
            out.extend(said.clone());
            put = true;
        }
        out.insert(k, v);
    }
    if !put {
        out.extend(said);
    }
    let out = Value::Object(out);
    match cut.warning() {
        Some(w) => super::warned(out, &[w]),
        None => out,
    }
}

/// An answer of the tool cut (`cut_answer`), when it is JSON.
pub(crate) fn cut_output(
    output: crate::mcp::Output,
    spec: &Spec,
    page: &Page,
) -> crate::mcp::Output {
    match output {
        crate::mcp::Output::Json(v) => crate::mcp::Output::Json(cut_answer(v, spec, page)),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn keys(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    fn page(limit: usize, after: Option<&str>) -> Page {
        Page {
            limit,
            after: after.map(str::to_string),
        }
    }

    #[test]
    fn a_window_goes_on_after_its_key_and_keeps_a_run_of_one_key_whole() {
        let list = keys(&["a", "b", "b", "c", "d"]);
        let first = window(&list, &page(2, None), Order::Listed, 1000, |_| 1);
        assert_eq!((first.start, first.end), (0, 1), "b の 2 つは分けない");
        assert_eq!(first.next.as_deref(), Some("a"));
        let second = window(&list, &page(2, Some("a")), Order::Listed, 1000, |_| 1);
        assert_eq!((second.start, second.end), (1, 3));
        assert_eq!(second.next.as_deref(), Some("b"));
        let third = window(&list, &page(2, Some("b")), Order::Listed, 1000, |_| 1);
        assert_eq!((third.start, third.end), (3, 5));
        assert_eq!(third.next, None);
        // A run longer than the count is given whole, when it comes first.
        let one = window(&list, &page(1, Some("a")), Order::Listed, 1000, |_| 1);
        assert_eq!((one.start, one.end), (1, 3));
    }

    #[test]
    fn a_window_stops_at_its_room_but_gives_one_at_least() {
        let list = keys(&["a", "b", "c"]);
        let cut = window(&list, &page(100, None), Order::Listed, 15, |_| 10);
        assert_eq!((cut.start, cut.end), (0, 1));
        let cut = window(&list, &page(100, None), Order::Listed, 5, |_| 10);
        assert_eq!((cut.start, cut.end), (0, 1), "入らなくても 1 つは返す");
    }

    #[test]
    fn an_after_gone_goes_on_where_it_would_stand_or_from_the_top() {
        let list = keys(&["a", "c", "e"]);
        let cut = window(&list, &page(10, Some("b")), Order::Ascending, 1000, |_| 1);
        assert_eq!(cut.start, 1);
        assert_eq!(cut.lost, Some(Lost::Placed("b".into())));
        let cut = window(&list, &page(10, Some("b")), Order::Listed, 1000, |_| 1);
        assert_eq!(cut.start, 0);
        assert_eq!(cut.lost, Some(Lost::FromTop("b".into())));
        let numbers = keys(&["9", "7", "3"]);
        let cut = window(
            &numbers,
            &page(10, Some("8")),
            Order::Descending,
            1000,
            |_| 1,
        );
        assert_eq!(cut.start, 1);
    }

    #[test]
    fn an_answer_of_several_lists_is_cut_across_them() {
        let answer = json!({"project": "p", "tags": [{"tag": "x"}, {"tag": "y"}], "declared": ["z"], "ungrouped": []});
        let spec = Spec::new(Lists::Several(&["tags", "groups", "declared", "ungrouped"]));
        let first = cut_answer(answer.clone(), &spec, &page(1, None));
        assert_eq!(
            first,
            json!({"project": "p", "truncated": true, "matched": 3, "returned": 1, "next": "tags x", "tags": [{"tag": "x"}]})
        );
        let second = cut_answer(answer.clone(), &spec, &page(1, Some("tags x")));
        assert_eq!(
            second,
            json!({"project": "p", "truncated": true, "matched": 3, "returned": 1, "next": "tags y", "tags": [{"tag": "y"}]})
        );
        let last = cut_answer(answer.clone(), &spec, &page(1, Some("tags y")));
        assert_eq!(last, json!({"project": "p", "declared": ["z"]}));
        assert_eq!(
            cut_answer(answer.clone(), &spec, &page(10, None)),
            answer,
            "切らないものはそのまま"
        );
    }

    #[test]
    fn a_list_inside_a_list_is_cut_by_the_outer_key_and_its_own() {
        let answer = json!({"exports": [
            {"name": "a", "files": [{"path": "1"}, {"path": "2"}]},
            {"name": "b", "files": []},
            {"name": "c", "files": [{"path": "3"}]}
        ]});
        let spec = Spec::new(Lists::Inside {
            outer: "exports",
            outer_key: "name",
            inner: "files",
            inner_key: "path",
        });
        let first = cut_answer(answer.clone(), &spec, &page(2, None));
        assert_eq!(first["next"], "a 2");
        assert_eq!(
            first["exports"],
            json!([{"name": "a", "files": [{"path": "1"}, {"path": "2"}]}])
        );
        let second = cut_answer(answer, &spec, &page(2, Some("a 2")));
        assert_eq!(
            second["exports"],
            json!([{"name": "b", "files": []}, {"name": "c", "files": [{"path": "3"}]}])
        );
        assert!(second.get("truncated").is_none());
    }
}
