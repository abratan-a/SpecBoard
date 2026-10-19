//! One commit of a history read through MCP (`branch_read` with `as: history` and `seq`): what it
//! changed, before and after, the way a merge tells its units apart (`specboard://SpecBoard/Docs/Design:CoreApi`
//! `tools.history`). The project is composed twice — with the rows the commit kept as they were
//! before it, and as they were right after it (`write::history_around`) — and each card and section it
//! wrote is said by the units that differ, each version in the shape card_read gives, its paths read
//! in the project of its side. What it made or removed is said whole, as card_create is given it. Where
//! boxes sit is not in a commit, and is not said. What a branch changed against main (`branch_read`
//! with `as: diff` and `path`) is said the same way: before is the branch with each of its own rows as
//! it copied it from main (and none it made), and after is the branch now.

use super::merges::{said_change, said_whole};
use super::paging;
use crate::export::Project;
use crate::store::{Composition, Over, OverRow, Store};
use crate::write::{self, Table};
use serde_json::{Map, Value, json};
use std::collections::HashMap;

/// How long both versions of a unit are, in characters, for it to be said by the lines of its one
/// text that changed rather than whole.
const LONG: usize = 2_000;

/// The most lines two texts are compared line by line in: past it, what lies between what they
/// begin and end with alike is said as one change.
const MOST_COMPARED: usize = 1_000_000;

/// What a call asks of a commit.
pub(crate) struct Asked {
    pub seq: i64,
    /// Cards' or sections' paths, before or after the commit: those changes alone (none: all).
    pub paths: Vec<String>,
    /// Long texts whole.
    pub full: bool,
    /// The path of the last change read: the ones after it (through MCP).
    pub after: Option<String>,
}

/// The commit `asked.seq` of the tree, and what it changed (`paged`: through MCP, cut to the
/// characters an answer is given in).
pub(crate) fn commit_view(
    store: &Store,
    project: &str,
    tree: &str,
    asked: &Asked,
    paged: bool,
) -> Result<Map<String, Value>, String> {
    let Some(commit) = write::history_commit(&store.conn, project, tree, asked.seq)? else {
        return Err(text!("history.commit.noneOrCut", seq = asked.seq));
    };
    let mut out = Map::new();
    if let Value::Array(mut rows) = super::branches::commits_view(std::slice::from_ref(&commit)) {
        out.insert("commit".into(), rows.remove(0));
    }
    let around = write::history_around(&store.conn, project, tree, asked.seq)?;
    let changes = if around.is_empty() {
        Vec::new()
    } else {
        changes_of(store, project, tree, &around, asked.full)?
    };
    let mut changes: Vec<Value> = changes.into_iter().map(|c| c.said).collect();
    if !asked.paths.is_empty() {
        if let Some(missing) = asked
            .paths
            .iter()
            .find(|p| !changes.iter().any(|c| names(c, p)))
        {
            return Err(text!(
                "branches.read.notInCommit",
                seq = asked.seq,
                path = missing.trim()
            ));
        }
        changes.retain(|c| asked.paths.iter().any(|p| names(c, p)));
    }
    paged_into(&mut out, changes, asked.after.clone(), paged);
    Ok(out)
}

/// Whether a change is the one at `path`: by its path, or the path it had before.
fn names(change: &Value, path: &str) -> bool {
    let wanted = path.trim();
    ["path", "from"]
        .iter()
        .any(|k| change[*k].as_str().is_some_and(|p| p == wanted))
}

/// The changes put in `out`: all of them, or through MCP the ones after `after` within the
/// characters an answer is given in, by their paths.
fn paged_into(
    out: &mut Map<String, Value>,
    changes: Vec<Value>,
    after: Option<String>,
    paged: bool,
) {
    if !paged {
        out.insert("changes".into(), Value::Array(changes));
        return;
    }
    let keys: Vec<String> = changes
        .iter()
        .map(|c| c["path"].as_str().unwrap_or_default().to_string())
        .collect();
    let page = paging::Page {
        limit: usize::MAX,
        after,
    };
    let cut = paging::window(
        &keys,
        &page,
        paging::Order::Listed,
        paging::MOST_CHARS,
        |i| paging::chars(&changes[i]),
    );
    if let Some(warning) = cut.warning() {
        out.insert("warnings".into(), json!([warning]));
    }
    out.extend(cut.said());
    out.insert(
        "changes".into(),
        Value::Array(changes[cut.start..cut.end].to_vec()),
    );
}

/// What a call asks of a branch's changes against main.
pub(crate) struct Contents {
    /// Cards' or sections' paths: a changed one's path now, or a removed or moved one's before.
    pub paths: Vec<String>,
    /// Long texts whole.
    pub full: bool,
    /// The path of the last change read: the ones after it (through MCP).
    pub after: Option<String>,
}

/// What the branch changed at `asked.paths`, before and after, as a commit's changes are said; each
/// one main has changed too since the branch copied it is marked `mainChanged`. A path the branch did
/// not change is refused with the paths it did (or, one main changed, said to be main's).
pub(crate) fn branch_contents(
    store: &Store,
    project: &str,
    branch: &str,
    asked: &Contents,
    paged: bool,
) -> Result<Map<String, Value>, String> {
    let reader = store.reader();
    let mut own: Vec<(Table, String, Option<Value>)> = Vec::new();
    for table in [Table::Sections, Table::Cards] {
        let mut stmt = reader
            .conn
            .prepare(&format!(
                "SELECT id, base FROM {} WHERE project_id = ? AND branch_id = ? ORDER BY rowid",
                table.name()
            ))
            .map_err(crate::store::sql)?;
        let rows = stmt
            .query_map([project, branch], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?))
            })
            .map_err(crate::store::sql)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(crate::store::sql)?;
        own.extend(
            rows.into_iter()
                .map(|(id, base)| (table, id, write::parsed(base.as_deref()))),
        );
    }
    // Before: each of the branch's own rows as it copied it from main (none for one it made).
    let around: Vec<write::Around> = own
        .iter()
        .map(|(table, id, base)| write::Around {
            table: *table,
            id: id.clone(),
            before: base
                .as_ref()
                .and_then(Value::as_object)
                .map(|b| as_row(*table, b)),
            after: None,
            later: Vec::new(),
        })
        .collect();
    let changes = if around.is_empty() {
        Vec::new()
    } else {
        changes_of(store, project, branch, &around, asked.full)?
    };
    if let Some(missing) = asked
        .paths
        .iter()
        .find(|p| !changes.iter().any(|c| names(&c.said, p)))
    {
        let diff = super::branches::branch_diff(&reader, project, branch, false)?;
        let wanted = missing.trim();
        let mains = diff
            .main_changed
            .iter()
            .map(|(p, _)| p)
            .chain(&diff.main_made)
            .chain(&diff.main_removed)
            .any(|p| p == wanted);
        if mains {
            return Err(text!("branches.read.pathInMain", path = wanted));
        }
        let changed: Vec<String> = changes
            .iter()
            .filter_map(|c| c.said["path"].as_str().map(str::to_string))
            .take(10)
            .collect();
        return Err(text!(
            "branches.read.pathNotChanged",
            path = wanted,
            changed = crate::text::list(&changed)
        ));
    }
    let main_docs = |table: Table| -> Result<HashMap<String, Value>, String> {
        let mut stmt = reader
            .conn
            .prepare(&format!(
                "SELECT id, doc FROM {} WHERE project_id = ? AND branch_id = '' AND removed = 0",
                table.name()
            ))
            .map_err(crate::store::sql)?;
        let rows = stmt
            .query_map([project], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?))
            })
            .map_err(crate::store::sql)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(crate::store::sql)?;
        Ok(rows
            .into_iter()
            .filter_map(|(id, doc)| write::parsed(doc.as_deref()).map(|d| (id, d)))
            .collect())
    };
    let mains = [
        (Table::Sections, main_docs(Table::Sections)?),
        (Table::Cards, main_docs(Table::Cards)?),
    ];
    let bases: HashMap<(Table, &str), &Value> = own
        .iter()
        .filter_map(|(t, id, base)| base.as_ref().map(|b| ((*t, id.as_str()), b)))
        .collect();
    let mut said: Vec<Value> = Vec::new();
    for change in changes {
        if !asked.paths.iter().any(|p| names(&change.said, p)) {
            continue;
        }
        let mut one = change.said;
        let now = mains
            .iter()
            .find(|(t, _)| *t == change.table)
            .and_then(|(_, docs)| docs.get(&change.id));
        if let (Some(base), Some(now)) = (bases.get(&(change.table, change.id.as_str())), now)
            && *base != now
            && let Value::Object(o) = &mut one
        {
            o.insert("mainChanged".into(), true.into());
        }
        said.push(one);
    }
    let mut out = Map::new();
    paged_into(&mut out, said, asked.after.clone(), paged);
    Ok(out)
}

/// A document as its table keeps it, where a tree's rows are put in place (`Over`): where it sits,
/// its name, and itself.
fn as_row(table: Table, doc: &Map<String, Value>) -> Map<String, Value> {
    let text = |key: &str| doc.get(key).and_then(Value::as_str).unwrap_or_default();
    let mut row = Map::new();
    match table {
        Table::Cards => {
            row.insert("section_id".into(), write::holder(doc).into());
            row.insert("kind".into(), text("kind").into());
        }
        Table::Sections => {
            row.insert(
                "parent_id".into(),
                doc.get("parent_id").cloned().unwrap_or(Value::Null),
            );
        }
    }
    row.insert("name".into(), text("name").into());
    row.insert("removed".into(), 0.into());
    row.insert(
        "doc".into(),
        crate::ojson::compact(&Value::Object(doc.clone())).into(),
    );
    row
}

/// A card or section changed, by its table and id, as it is said.
struct Change {
    table: Table,
    id: String,
    said: Value,
}

/// The cards and sections a commit wrote, as they were before and after it: sections first, then cards,
/// each by its path.
fn changes_of(
    store: &Store,
    project: &str,
    tree: &str,
    around: &[write::Around],
    full: bool,
) -> Result<Vec<Change>, String> {
    let before: Vec<OverRow> = around
        .iter()
        .map(|a| (a.table.name().to_string(), a.id.clone(), a.before.clone()))
        .collect();
    let after: Vec<OverRow> = around
        .iter()
        .filter_map(|a| {
            a.after
                .clone()
                .map(|next| (a.table.name().to_string(), a.id.clone(), next))
        })
        .collect();
    let row = store
        .project(project)?
        .ok_or_else(|| text!("screen.projectNotFound"))?;
    let reader = store.reader();
    let branch = (!tree.is_empty()).then_some(tree);
    let compose =
        |rows: &[OverRow]| reader.compose_over(project, branch, Some(&Over { tree, rows }));
    let (was_made, is_made) = (compose(&before)?, compose(&after)?);
    let was = crate::model::typed(&crate::detail::of_made(&row, &was_made))?;
    let is = crate::model::typed(&crate::detail::of_made(&row, &is_made))?;
    let (was_project, is_project) = (Project::new(&was), Project::new(&is));
    let docs = |made: &Composition| -> HashMap<(Table, String), Map<String, Value>> {
        made.cards
            .iter()
            .map(|c| ((Table::Cards, c.id.clone()), c.doc.clone()))
            .chain(
                made.sections
                    .iter()
                    .map(|s| ((Table::Sections, s.id.clone()), s.doc.clone())),
            )
            .collect()
    };
    let (was_docs, is_docs) = (docs(&was_made), docs(&is_made));
    let path_in =
        |project: &Project, detail: &crate::model::Detail, table: Table, id: &str| match table {
            Table::Sections => detail
                .sections
                .iter()
                .any(|s| s.id == id)
                .then(|| project.directory.full_section(id)),
            Table::Cards => project.named(id),
        };
    let mut out: Vec<(bool, String, Change)> = Vec::new();
    for kept in around {
        let section = kept.table == Table::Sections;
        let key = (kept.table, kept.id.clone());
        let (then, now) = (was_docs.get(&key), is_docs.get(&key));
        let from = path_in(&was_project, &was, kept.table, &kept.id);
        let to = path_in(&is_project, &is, kept.table, &kept.id);
        let mut change = Map::new();
        let path = match (&from, &to, then, now) {
            (None, Some(to), _, Some(now)) => {
                change.insert("change".into(), "add".into());
                change.insert("after".into(), said_whole(section, now, &is_project));
                to.clone()
            }
            (Some(from), None, Some(then), _) => {
                change.insert("change".into(), "remove".into());
                change.insert("before".into(), said_whole(section, then, &was_project));
                from.clone()
            }
            (Some(from), Some(to), Some(then), Some(now)) => {
                let kind = (!section)
                    .then(|| {
                        now.get("kind")
                            .or_else(|| then.get("kind"))
                            .and_then(Value::as_str)
                    })
                    .flatten();
                let units: Vec<Value> = write::unit_changes(section, then, now)
                    .into_iter()
                    .map(|u| {
                        let before =
                            said_change(section, kind, &u.unit, u.before.as_ref(), &was_project);
                        let after =
                            said_change(section, kind, &u.unit, u.after.as_ref(), &is_project);
                        unit_said(&u.label, before, after, full)
                    })
                    .collect();
                if units.is_empty() && from == to {
                    continue;
                }
                change.insert("change".into(), "change".into());
                if from != to {
                    change.insert("from".into(), from.clone().into());
                }
                change.insert("units".into(), Value::Array(units));
                to.clone()
            }
            _ => continue,
        };
        let mut said = Map::new();
        said.insert("path".into(), path.clone().into());
        if section {
            said.insert("section".into(), true.into());
        }
        said.extend(change);
        if !kept.later.is_empty() {
            said.insert(
                "later".into(),
                kept.later.iter().map(|s| Value::from(*s)).collect(),
            );
        }
        out.push((
            section,
            path,
            Change {
                table: kept.table,
                id: kept.id.clone(),
                said: Value::Object(said),
            },
        ));
    }
    out.sort_by(|a, b| (!a.0, &a.1).cmp(&(!b.0, &b.1)));
    Ok(out.into_iter().map(|(_, _, c)| c).collect())
}

/// One unit that changed: its two versions, or — both long, apart in one text alone, and not asked
/// whole — where that text is in it and the lines of it that changed.
fn unit_said(label: &str, before: Value, after: Value, full: bool) -> Value {
    if !full
        && !before.is_null()
        && !after.is_null()
        && paging::chars(&before) > LONG
        && paging::chars(&after) > LONG
    {
        let mut apart = Vec::new();
        if alike_but_texts(&before, &after, "", &mut apart)
            && let [(at, was, is)] = apart.as_slice()
        {
            let key = if at.is_empty() {
                label.to_string()
            } else {
                format!("{label}.{at}")
            };
            return json!({ "unit": label, "key": key, "lines": hunks(was, is) });
        }
    }
    json!({ "unit": label, "before": before, "after": after })
}

/// Whether two values are alike but for their texts: the texts apart gathered, each by where it is
/// in them (`body`, `steps[2].description`).
fn alike_but_texts(
    a: &Value,
    b: &Value,
    at: &str,
    apart: &mut Vec<(String, String, String)>,
) -> bool {
    let under = |key: &str| {
        if at.is_empty() {
            key.to_string()
        } else {
            format!("{at}.{key}")
        }
    };
    match (a, b) {
        (Value::String(x), Value::String(y)) => {
            if x != y {
                apart.push((at.to_string(), x.clone(), y.clone()));
            }
            true
        }
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len()
                && x.iter().all(|(k, v)| {
                    y.get(k)
                        .is_some_and(|w| alike_but_texts(v, w, &under(k), apart))
                })
        }
        (Value::Array(x), Value::Array(y)) => {
            x.len() == y.len()
                && x.iter()
                    .zip(y)
                    .enumerate()
                    .all(|(i, (v, w))| alike_but_texts(v, w, &format!("{at}[{i}]"), apart))
        }
        _ => a == b,
    }
}

/// The lines of `after` that are not as in `before`, a run of them at a time: where the run is in
/// `after` (its first line, from 1; where they would be, for lines only taken away), the lines it
/// had before and has now — none for no line.
fn hunks(before: &str, after: &str) -> Vec<Value> {
    let a: Vec<&str> = before.split('\n').collect();
    let b: Vec<&str> = after.split('\n').collect();
    let head = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
    let tail = a[head..]
        .iter()
        .rev()
        .zip(b[head..].iter().rev())
        .take_while(|(x, y)| x == y)
        .count();
    let (x, y) = (&a[head..a.len() - tail], &b[head..b.len() - tail]);
    // Each line of the middles, alike in both or not: by their longest run in common.
    let mut same: Vec<(usize, usize)> = Vec::new();
    if !x.is_empty() && !y.is_empty() && x.len().saturating_mul(y.len()) <= MOST_COMPARED {
        let mut longest = vec![vec![0u32; y.len() + 1]; x.len() + 1];
        for i in (0..x.len()).rev() {
            for j in (0..y.len()).rev() {
                longest[i][j] = if x[i] == y[j] {
                    longest[i + 1][j + 1] + 1
                } else {
                    longest[i + 1][j].max(longest[i][j + 1])
                };
            }
        }
        let (mut i, mut j) = (0, 0);
        while i < x.len() && j < y.len() {
            if x[i] == y[j] {
                same.push((i, j));
                i += 1;
                j += 1;
            } else if longest[i + 1][j] >= longest[i][j + 1] {
                i += 1;
            } else {
                j += 1;
            }
        }
    }
    same.push((x.len(), y.len()));
    let joined = |lines: &[&str]| -> Value {
        if lines.is_empty() {
            Value::Null
        } else {
            lines.join("\n").into()
        }
    };
    let mut out = Vec::new();
    let (mut i, mut j) = (0, 0);
    for (si, sj) in same {
        if si > i || sj > j {
            out.push(json!({
                "line": head + j + 1,
                "before": joined(&x[i..si]),
                "after": joined(&y[j..sj]),
            }));
        }
        i = si + 1;
        j = sj + 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hunks_say_only_the_lines_that_changed_where_they_are_now() {
        assert_eq!(
            hunks("a\nb\nc\nd", "a\nB\nc\nd\ne"),
            vec![
                json!({"line": 2, "before": "b", "after": "B"}),
                json!({"line": 5, "before": null, "after": "e"})
            ]
        );
        assert_eq!(
            hunks("a\nb\nc", "a\nc"),
            vec![json!({"line": 2, "before": "b", "after": null})]
        );
    }

    #[test]
    fn a_long_unit_apart_in_one_text_is_said_by_its_lines() {
        let body = |middle: &str| format!("{}\n{middle}\n{}", "x".repeat(1500), "y".repeat(1500));
        let said = unit_said(
            "rows.auth",
            json!({"heading": "認証", "body": body("前")}),
            json!({"heading": "認証", "body": body("後")}),
            false,
        );
        assert_eq!(
            said,
            json!({"unit": "rows.auth", "key": "rows.auth.body", "lines": [{"line": 2, "before": "前", "after": "後"}]})
        );
        // Asked whole, or apart in two places, both versions whole.
        let whole = unit_said(
            "rows.auth",
            json!({"heading": "認証", "body": body("前")}),
            json!({"heading": "認証", "body": body("後")}),
            true,
        );
        assert!(whole.get("before").is_some());
        let twice = unit_said(
            "rows.auth",
            json!({"heading": "前", "body": body("前")}),
            json!({"heading": "後", "body": body("後")}),
            false,
        );
        assert!(twice.get("before").is_some());
    }
}
