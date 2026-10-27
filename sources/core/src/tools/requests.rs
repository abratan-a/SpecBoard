//! Merge requests: requests to merge a branch into a locked main (`branch_merge` into it,
//! `mr_list`, `mr_read`, `mr_delete`; `specboard://SpecBoard/Docs/Design:DesignBranches` §8): made from MCP with the merge tried and its
//! conflicts chosen, approved or rejected by a person on screen. Approved, a request goes into main as
//! it is now with the choices it was tried with — only while the branch has not moved since, and what
//! it brings is what it brought when tried.

use super::merges;
use crate::write::{MergeChange, MergePreview, MergeTake, Tx, db};
use rusqlite::{Connection, params};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

/// One request, as it is kept: its result is what the merge did when it was tried.
pub(crate) struct Request {
    pub number: i64,
    pub branch_id: String,
    pub branch_name: String,
    pub title: String,
    pub description: String,
    /// The access key's name it came through; none for a call without one.
    pub author: Option<String>,
    pub at: String,
    /// `open`, `stale` (open, but the branch moved since it was tried, or main moved and what it
    /// brings is not what it was: worked out on each read, never kept), `merged`, `rejected` or
    /// `withdrawn`.
    pub state: String,
    pub reason: Option<String>,
    pub result: Value,
    /// Whether it asks for the boxes the branch moved to go into main too (`places`).
    pub places: bool,
    /// Whether its branch stays as it is once approved, rather than set aside (`keep`).
    pub keep: bool,
}

impl Request {
    fn state_label(&self) -> String {
        match self.state.as_str() {
            "open" => text!("requests.state.open"),
            "stale" => text!("requests.state.stale"),
            "merged" => text!("requests.state.merged"),
            "rejected" => text!("requests.state.rejected"),
            _ => text!("requests.state.withdrawn"),
        }
    }

    fn author_keyed(&self) -> crate::text::Keyed {
        match &self.author {
            Some(name) => keyed!("requests.author.named", name),
            None => keyed!("requests.author.unnamed"),
        }
    }

    fn author_label(&self) -> String {
        self.author_keyed().render()
    }
}

/// The print of what a request was tried on: the branch's own rows and main's documents, as they
/// are now, the two apart (`<branch>.<main>`): a branch that moved leaves the request stale, main
/// that moved has it tried again (`list`). A print kept as one of both (from before the two were apart) reads as stale.
pub(crate) fn request_print(
    conn: &Connection,
    project: &str,
    branch: &str,
) -> Result<String, String> {
    Ok(format!(
        "{}.{}",
        print_of(conn, project, branch)?,
        print_of(conn, project, "")?
    ))
}

/// The print of a tree's documents: its sections' and cards' rows (`requestPrint`).
fn print_of(conn: &Connection, project: &str, tree: &str) -> Result<String, String> {
    let mut hash = Sha256::new();
    for table in ["section_docs", "card_docs"] {
        hash.update(format!("\u{2}{table}\u{1}{tree}").as_bytes());
        let mut stmt = conn
            .prepare(&format!(
                "SELECT id, removed, ifnull(doc, ''), ifnull(base, '') FROM {table} WHERE project_id = ? AND branch_id = ? ORDER BY id"
            ))
            .map_err(db)?;
        let rows = stmt
            .query_map(params![project, tree], |r| {
                Ok(format!(
                    "{}\u{1}{}\u{1}{}\u{1}{}",
                    r.get::<_, String>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?
                ))
            })
            .map_err(db)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db)?;
        for row in rows {
            hash.update(format!("{row}\u{0}").as_bytes());
        }
    }
    Ok(hash.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

/// One request as it is kept, with the print and the choices (`take`) it was tried with.
struct Kept {
    request: Request,
    print: String,
    take: String,
}

/// The project's requests as they are kept, newest first: none worked out stale.
fn kept_rows(conn: &Connection, project: &str, number: Option<i64>) -> Result<Vec<Kept>, String> {
    let mut sql = "SELECT number, branch_id, branch_name, title, description, author, at, state, reason, result, print, places, keep, take FROM merge_requests WHERE project_id = ?1".to_string();
    if number.is_some() {
        sql.push_str(" AND number = ?2");
    }
    sql.push_str(" ORDER BY number DESC");
    let mut stmt = conn.prepare(&sql).map_err(db)?;
    let read = |r: &rusqlite::Row| -> rusqlite::Result<Kept> {
        let result: String = r.get(9)?;
        Ok(Kept {
            request: Request {
                number: r.get(0)?,
                branch_id: r.get(1)?,
                branch_name: r.get(2)?,
                title: r.get(3)?,
                description: r.get(4)?,
                author: r.get(5)?,
                at: r.get(6)?,
                state: r.get(7)?,
                reason: r.get(8)?,
                result: serde_json::from_str(&result).unwrap_or(Value::Null),
                places: r.get::<_, i64>(11)? != 0,
                keep: r.get::<_, i64>(12)? != 0,
            },
            print: r.get(10)?,
            take: r.get(13)?,
        })
    };
    let rows = match number {
        Some(n) => stmt.query_map(params![project, n], read),
        None => stmt.query_map(params![project], read),
    }
    .map_err(db)?
    .collect::<Result<_, _>>()
    .map_err(db)?;
    Ok(rows)
}

/// One request as it is kept (`kept_rows`): what deciding it needs, its staleness aside.
fn kept(conn: &Connection, project: &str, number: i64) -> Result<Request, String> {
    kept_rows(conn, project, Some(number))?
        .into_iter()
        .next()
        .map(|k| k.request)
        .ok_or_else(|| text!("requests.notFound", number))
}

/// The project's requests, newest first; an open one reads as stale when its branch moved
/// since it was tried (or is gone), or main moved and what it brings now is not what it brought
/// (`brings_the_same`). Worked out in `tx`, and undone with it: a transaction the caller lets go
/// when it only reads (`listPullRequests`).
pub(crate) fn list(
    tx: &mut Tx,
    project: &str,
    number: Option<i64>,
) -> Result<Vec<Request>, String> {
    let rows = kept_rows(tx.conn(), project, number)?;
    let mut branches: std::collections::HashMap<String, Option<String>> = Default::default();
    let mut main: Option<String> = None;
    let mut out = Vec::new();
    for Kept {
        mut request,
        print,
        take,
    } in rows
    {
        if request.state == "open" {
            let open = match print.split_once('.') {
                // A request kept with one print of both (from before the two were apart): stale.
                None => false,
                Some((branch_then, main_then)) => {
                    if !branches.contains_key(&request.branch_id) {
                        let there = tx
                            .conn()
                            .query_row(
                                "SELECT 1 FROM branches WHERE id = ?",
                                [&request.branch_id],
                                |_| Ok(()),
                            )
                            .is_ok();
                        let now = if there {
                            Some(print_of(tx.conn(), project, &request.branch_id)?)
                        } else {
                            None
                        };
                        branches.insert(request.branch_id.clone(), now);
                    }
                    if branches[&request.branch_id].as_deref() != Some(branch_then) {
                        false
                    } else {
                        if main.is_none() {
                            main = Some(print_of(tx.conn(), project, "")?);
                        }
                        main.as_deref() == Some(main_then)
                            || brings_the_same(tx, project, &request, &take)?
                    }
                }
            };
            if !open {
                request.state = "stale".to_string();
            }
        }
        out.push(request);
    }
    Ok(out)
}

/// Whether a request still brings what it brought when it was tried, main having moved since:
/// tried again on main as it is now with its choices — no conflict left and no path holding two
/// things, the same paths going in, changing and going, and the same paths left broken — and
/// undone.
fn brings_the_same(
    tx: &mut Tx,
    project: &str,
    request: &Request,
    take: &str,
) -> Result<bool, String> {
    let take = merges::take(serde_json::from_str::<Value>(take).ok().as_ref())?;
    let chosen = tx.merge_preview(project, &request.branch_id, "", &take)?;
    if !chosen.conflicts.is_empty() || !chosen.overlaps.is_empty() {
        return Ok(false);
    }
    let now: std::collections::BTreeSet<(String, &str, bool)> = chosen
        .items
        .iter()
        .map(|i| (i.path.clone(), change_word(&i.change), i.section))
        .collect();
    let then: std::collections::BTreeSet<(String, &str, bool)> = request.result["items"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .map(|i| {
                    (
                        i["path"].as_str().unwrap_or("").to_string(),
                        match i["change"].as_str() {
                            Some("add") => "add",
                            Some("remove") => "remove",
                            _ => "change",
                        },
                        i["section"].as_bool().unwrap_or(false),
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    if now != then {
        return Ok(false);
    }
    // What the merge would leave broken in main, as `create` found it.
    let name: String = tx
        .conn()
        .query_row("SELECT name FROM projects WHERE id = ?", [project], |r| {
            r.get(0)
        })
        .map_err(db)?;
    // Said in every language: `create` kept them in the language of whoever made the request.
    let said_in_each = |seen: &super::Seen| -> Vec<Vec<String>> {
        crate::text::LANGUAGES
            .iter()
            .map(|lang| {
                let _lang = crate::text::bind(Some(lang));
                merges::broken(seen)
            })
            .collect()
    };
    let before: Vec<std::collections::HashSet<String>> =
        said_in_each(&*merges::seen_in(tx, project, &name, "", "main")?)
            .into_iter()
            .map(|said| said.into_iter().collect())
            .collect();
    let tried = tx.alone(|tx| -> Result<(), Vec<Vec<String>>> {
        let failed = |e: String| vec![vec![e]; crate::text::LANGUAGES.len()];
        tx.merge(project, &request.branch_id, "", &take, false, true)
            .map_err(failed)?;
        let after = merges::seen_in(tx, project, &name, "", "main").map_err(failed)?;
        Err(said_in_each(&after)
            .into_iter()
            .zip(&before)
            .map(|(said, before)| said.into_iter().filter(|w| !before.contains(w)).collect())
            .collect())
    })?;
    let broken_now: Vec<std::collections::BTreeSet<String>> = match tried {
        Err(said) => said.into_iter().map(|w| w.into_iter().collect()).collect(),
        Ok(()) => vec![Default::default(); crate::text::LANGUAGES.len()],
    };
    let broken_then: std::collections::BTreeSet<String> = request.result["warnings"]
        .as_array()
        .map(|w| {
            w.iter()
                .filter_map(Value::as_str)
                .map(String::from)
                .collect()
        })
        .unwrap_or_default();
    Ok(broken_now.contains(&broken_then))
}

fn change_word(change: &MergeChange) -> &'static str {
    match change {
        MergeChange::Add => "add",
        MergeChange::Change => "change",
        MergeChange::Remove => "remove",
    }
}

pub(crate) fn one(tx: &mut Tx, project: &str, number: i64) -> Result<Request, String> {
    list(tx, project, Some(number))?
        .into_iter()
        .next()
        .ok_or_else(|| text!("requests.notFound", number))
}

/// A request in a line of a list (`row`).
pub(crate) fn row(request: &Request) -> Map<String, Value> {
    let mut out = Map::new();
    out.insert("mr".into(), request.number.into());
    out.insert("title".into(), request.title.clone().into());
    out.insert("from".into(), request.branch_name.clone().into());
    out.insert("author".into(), request.author_label().into());
    out.insert("at".into(), request.at.clone().into());
    out.insert("state".into(), request.state.clone().into());
    if request.places {
        out.insert("places".into(), true.into());
    }
    if request.keep {
        out.insert("keep".into(), true.into());
    }
    if let Some(reason) = reason_said(request) {
        out.insert("reason".into(), reason.into());
    }
    out
}

/// How many boxes the source moved were left out of a merge made without `places`, and how to take
/// them (`hint`); nothing said when none were.
pub(crate) fn say_places_left(out: &mut Map<String, Value>, left: usize, hint: String) {
    if left > 0 {
        out.insert("placesLeft".into(), left.into());
        out.insert("hint".into(), hint.into());
    }
}

/// What the merge did when the request was tried, as MCP reads it (`resultView`); `taken`: whether
/// the request asks for the boxes the branch moved too.
pub(crate) fn result_view(result: &Value, taken: bool) -> Map<String, Value> {
    let mut out = Map::new();
    let items = result["items"].as_array().cloned().unwrap_or_default();
    for key in ["add", "change", "remove"] {
        let paths: Vec<Value> = items
            .iter()
            .filter(|i| i["change"] == key)
            .map(|i| i["path"].clone())
            .collect();
        if !paths.is_empty() {
            out.insert(key.into(), Value::Array(paths));
        }
    }
    if let Some(conflicts) = result["conflicts"].as_array().filter(|c| !c.is_empty()) {
        out.insert(
            "conflicts".into(),
            Value::Array(
                conflicts
                    .iter()
                    .map(|c| {
                        let mut row = Map::new();
                        row.insert("path".into(), c["path"].clone());
                        row.insert("unit".into(), unit_said(&c["unit"]));
                        row.insert("took".into(), c["took"].clone());
                        for key in ["base", "source", "target"] {
                            row.insert(
                                key.into(),
                                c[key]
                                    .as_str()
                                    .and_then(|t| serde_json::from_str(t).ok())
                                    .unwrap_or(Value::Null),
                            );
                        }
                        Value::Object(row)
                    })
                    .collect(),
            ),
        );
    }
    if let Some(warnings) = result["warnings"].as_array().filter(|w| !w.is_empty()) {
        out.insert("warnings".into(), warnings_said(warnings));
    }
    if let Some(places) = result["places"].as_u64().filter(|n| *n > 0) {
        if taken {
            out.insert("places".into(), places.into());
        } else {
            say_places_left(&mut out, places as usize, text!("requests.placesLeft"));
        }
    }
    out
}

/// What a merge would leave broken, kept as said to whoever made the request, said again in the
/// language of the call.
fn warnings_said(warnings: &[Value]) -> Value {
    Value::Array(
        warnings
            .iter()
            .map(|w| match w.as_str() {
                Some(kept) => crate::text::said_as(&["merges.broken.item"], kept)
                    .unwrap_or_else(|| kept.to_string())
                    .into(),
                None => w.clone(),
            })
            .collect(),
    )
}

/// A conflict's unit as a request kept it: the whole card or section as `MergeWhole`, whatever
/// language it was kept named in.
fn unit_said(unit: &Value) -> Value {
    match unit.as_str() {
        Some(u) if crate::write::merge_is_whole(u) => crate::write::MergeWhole.into(),
        _ => unit.clone(),
    }
}

/// A request's result as it is shown: what it kept said in the language of the call.
fn result_said(result: &Value) -> Value {
    let mut out = result.clone();
    if let Some(warnings) = result["warnings"].as_array() {
        out["warnings"] = warnings_said(warnings);
    }
    if let Some(conflicts) = out["conflicts"].as_array_mut() {
        for c in conflicts {
            if c.get("unit").is_some() {
                c["unit"] = unit_said(&c["unit"]);
            }
        }
    }
    out
}

/// The reason a request was closed, as it is shown: the one the core gives when a request takes
/// another's place in the language of the call, one a person wrote as written.
fn reason_said(request: &Request) -> Option<String> {
    let reason = request.reason.as_ref()?;
    Some(if request.state == "withdrawn" {
        crate::text::said_as(&["requests.resubmitted"], reason).unwrap_or_else(|| reason.clone())
    } else {
        reason.clone()
    })
}

/// The conflicts left and the overlaps of a preview, as a refusal shows them.
fn shown(preview: &MergePreview, seen: &super::Seen) -> String {
    let view = merges::preview_view(preview, seen);
    let mut out = Map::new();
    for key in ["conflicts", "overlaps"] {
        if let Some(v) = view.get(key) {
            out.insert(key.into(), v.clone());
        }
    }
    crate::ojson::compact(&Value::Object(out))
}

/// A request made (`createPullRequest`): the merge of `branch` into main tried with
/// `take` — run and undone, so what it would leave unresolved is known too — and kept
/// with the print of what it was tried on, and whether it asks for the boxes the branch
/// moved too (`places`). A request still open for the branch is withdrawn: this one is
/// made again. Refused, with the preview, while a conflict is left unchosen or a path
/// holds two things.
#[allow(clippy::too_many_arguments)]
pub(crate) fn create(
    tx: &mut Tx,
    project: &str,
    project_name: &str,
    branch: &(String, String),
    title: &str,
    description: &str,
    take: &MergeTake,
    places: bool,
    keep: bool,
    author: Option<&str>,
) -> Result<i64, String> {
    let archived: Option<String> = tx
        .conn()
        .query_row(
            "SELECT archived_at FROM projects WHERE id = ?",
            [project],
            |r| r.get(0),
        )
        .map_err(db)?;
    if archived.is_some() {
        return Err(text!("requests.archived"));
    }
    let title = title.trim();
    if title.chars().count() > 120 {
        return Err(text!("requests.titleTooLong"));
    }
    if title.is_empty() {
        return Err(text!("requests.titleEmpty"));
    }
    let description = description.trim();
    if description.chars().count() > 4000 {
        return Err(text!("requests.descriptionTooLong"));
    }
    let (id, name) = branch;
    let all = tx.merge_preview(project, id, "", &MergeTake::new())?;
    let chosen = tx.merge_preview(project, id, "", take)?;
    let main = merges::seen_in(tx, project, project_name, "", "main")?;
    if chosen.is_empty() && !(places && chosen.places > 0) {
        return Err(text!("requests.nothingToMerge", name));
    }
    if !chosen.overlaps.is_empty() {
        return Err(text!("requests.overlaps", shown = shown(&chosen, &main)));
    }
    if !chosen.conflicts.is_empty() {
        return Err(text!(
            "requests.conflicts",
            count = chosen.conflicts.len(),
            shown = shown(&chosen, &main)
        ));
    }
    // Tried and undone: what it would leave unresolved in main.
    let before: std::collections::HashSet<String> = merges::broken(&main).into_iter().collect();
    let all_view = merges::preview_view(&all, &main);
    drop(main);
    let tried = tx.alone(|tx| -> Result<(), Vec<String>> {
        tx.merge(project, id, "", take, places, true)
            .map_err(|e| vec![e])?;
        let after = merges::seen_in(tx, project, project_name, "", "main").map_err(|e| vec![e])?;
        Err(merges::broken(&after)
            .into_iter()
            .filter(|w| !before.contains(w))
            .collect())
    })?;
    let warnings = match tried {
        Err(w) => w,
        Ok(()) => Vec::new(),
    };
    // Kept as the Swift app keeps it: the keys in their order, the versions as text.
    let mut result = Map::new();
    let conflicts: Vec<Value> = all
        .conflicts
        .iter()
        .zip(
            all_view
                .get("conflicts")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default(),
        )
        .map(|(c, said)| {
            let took = take
                .get(&c.path)
                .and_then(|units| units.get(&c.label).or_else(|| units.get(&c.unit)))
                .map_or("", |side| match side {
                    crate::write::MergeSide::Source => "source",
                    crate::write::MergeSide::Target => "target",
                });
            let mut row = Map::new();
            for key in ["base", "path", "source", "target", "took", "unit"] {
                let value = match key {
                    "path" => c.path.clone().into(),
                    "unit" => c.label.clone().into(),
                    "took" => took.into(),
                    _ if said[key].is_null() => continue,
                    _ => crate::ojson::compact(&said[key]).into(),
                };
                row.insert(key.into(), value);
            }
            Value::Object(row)
        })
        .collect();
    result.insert("conflicts".into(), Value::Array(conflicts));
    result.insert(
        "items".into(),
        Value::Array(
            chosen
                .items
                .iter()
                .map(|i| {
                    let mut row = Map::new();
                    row.insert(
                        "change".into(),
                        match i.change {
                            MergeChange::Add => "add",
                            MergeChange::Change => "change",
                            MergeChange::Remove => "remove",
                        }
                        .into(),
                    );
                    row.insert("path".into(), i.path.clone().into());
                    row.insert("section".into(), i.section.into());
                    Value::Object(row)
                })
                .collect(),
        ),
    );
    result.insert(
        "warnings".into(),
        Value::Array(warnings.into_iter().map(Value::from).collect()),
    );
    // How many boxes the branch moved would change main, asked for or not: the person
    // who approves it may take them along.
    if chosen.places > 0 {
        result.insert("places".into(), chosen.places.into());
    }
    let print = request_print(tx.conn(), project, id)?;
    let number: i64 = tx
        .conn()
        .query_row(
            "SELECT COALESCE(MAX(number), 0) + 1 FROM merge_requests WHERE project_id = ?",
            [project],
            |r| r.get(0),
        )
        .map_err(db)?;
    let now = tx.now()?;
    let mut sorted: Vec<(
        &String,
        &std::collections::HashMap<String, crate::write::MergeSide>,
    )> = take.iter().collect();
    sorted.sort_by(|a, b| a.0.cmp(b.0));
    let take_text = crate::ojson::compact(&Value::Object(
        sorted
            .into_iter()
            .map(|(path, units)| {
                let mut units: Vec<(&String, &crate::write::MergeSide)> = units.iter().collect();
                units.sort_by(|a, b| a.0.cmp(b.0));
                (
                    path.clone(),
                    Value::Object(
                        units
                            .into_iter()
                            .map(|(unit, side)| {
                                (
                                    unit.clone(),
                                    match side {
                                        crate::write::MergeSide::Source => "source",
                                        crate::write::MergeSide::Target => "target",
                                    }
                                    .into(),
                                )
                            })
                            .collect(),
                    ),
                )
            })
            .collect(),
    ));
    tx.conn()
        .execute(
            "UPDATE merge_requests SET state = 'withdrawn', reason = ?, decided_at = ? WHERE project_id = ? AND branch_id = ? AND state = 'open'",
            params![text!("requests.resubmitted"), now, project, id],
        )
        .map_err(db)?;
    tx.conn()
        .execute(
            "INSERT INTO merge_requests (id, project_id, number, branch_id, branch_name, title, description, author, at, take, result, print, places, keep) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            params![
                crate::write::new_id(),
                project,
                number,
                id,
                name,
                title,
                description,
                author,
                now,
                take_text,
                Value::Object(result).to_string(),
                print,
                places as i64,
                keep as i64
            ],
        )
        .map_err(db)?;
    // The windows read main's requests again (the one it took the place of, withdrawn).
    tx.tell_of(project, "");
    Ok(number)
}

fn decided(request: &Request, number: i64) -> Result<(), String> {
    if request.state == "open" || request.state == "stale" {
        return Ok(());
    }
    Err(text!(
        "requests.decided",
        number,
        state = request.state_label()
    ))
}

/// Withdrawn by whoever made it (`withdrawPullRequest`).
pub(crate) fn withdraw(
    tx: &mut Tx,
    project: &str,
    number: i64,
    by: Option<&str>,
) -> Result<(), String> {
    let request = kept(tx.conn(), project, number)?;
    decided(&request, number)?;
    if request.author.as_deref() != by {
        return Err(text!(
            "requests.withdrawOther",
            number,
            author = request.author_label()
        ));
    }
    let now = tx.now()?;
    tx.conn()
        .execute(
            "UPDATE merge_requests SET state = 'withdrawn', decided_at = ? WHERE project_id = ? AND number = ?",
            params![now, project, number],
        )
        .map_err(db)?;
    tx.tell_of(project, "");
    Ok(())
}

/// Rejected by a person, saying why (`rejectPullRequest`).
pub(crate) fn reject(tx: &mut Tx, project: &str, number: i64, reason: &str) -> Result<(), String> {
    let reason = reason.trim();
    if reason.chars().count() > 120 {
        return Err(text!("requests.reasonTooLong"));
    }
    if reason.is_empty() {
        return Err(text!("requests.reasonEmpty"));
    }
    let request = kept(tx.conn(), project, number)?;
    decided(&request, number)?;
    let now = tx.now()?;
    tx.conn()
        .execute(
            "UPDATE merge_requests SET state = 'rejected', reason = ?, decided_at = ? WHERE project_id = ? AND number = ?",
            params![reason, now, project, number],
        )
        .map_err(db)?;
    tx.tell_of(project, "");
    Ok(())
}

/// Approved by a person: merged into main as it is now with the request's choices — only while
/// it is not stale (`list`; `approvePullRequest`). The merge's commit names the
/// request. The boxes the branch moved go in as the request asks, unless the person says
/// otherwise (`places`): they are how it looks, and moving one leaves the request as it was
/// tried.
pub(crate) fn approve(
    tx: &mut Tx,
    project: &str,
    number: i64,
    places: Option<bool>,
) -> Result<(MergePreview, bool), String> {
    let request = one(tx, project, number)?;
    match request.state.as_str() {
        "open" => {}
        "stale" => {
            return Err(text!("requests.stale", number));
        }
        _ => decided(&request, number)?,
    }
    let take_text: String = tx
        .conn()
        .query_row(
            "SELECT take FROM merge_requests WHERE project_id = ? AND number = ?",
            params![project, number],
            |r| r.get(0),
        )
        .map_err(db)?;
    let take = merges::take(serde_json::from_str::<Value>(&take_text).ok().as_ref())?;
    let seq: i64 = tx
        .conn()
        .query_row(
            "SELECT COALESCE(MAX(seq), 0) + 1 FROM commits WHERE project_id = ? AND branch_id = ''",
            [project],
            |r| r.get(0),
        )
        .map_err(db)?;
    let places = places.unwrap_or(request.places);
    let done = tx.merge(project, &request.branch_id, "", &take, places, request.keep)?;
    tx.line(
        project,
        "",
        "mr.merge",
        "project",
        None,
        keyed!(
            "requests.line.approved",
            number,
            title = request.title,
            author = request.author_keyed()
        ),
    )?;
    let now = tx.now()?;
    tx.conn()
        .execute(
            "UPDATE merge_requests SET state = 'merged', decided_at = ?, merge_seq = ? WHERE project_id = ? AND number = ?",
            params![now, seq, project, number],
        )
        .map_err(db)?;
    Ok((done, places))
}

/// What `branch_merge` into a locked main answers: that it was not merged but asked for, how to
/// go on (the person approves it on screen), and the request as `mr_read` reads it.
pub(crate) fn opened(
    tx: &mut Tx,
    summary: &super::ProjectSummary,
    number: i64,
) -> Result<Value, String> {
    let request = one(tx, &summary.id, number)?;
    let mut out = Map::new();
    out.insert("project".into(), summary.name.clone().into());
    out.insert("said".into(), text!("requests.opened", number).into());
    out.extend(row(&request));
    if !request.description.is_empty() {
        out.insert("description".into(), request.description.clone().into());
    }
    out.insert(
        "result".into(),
        Value::Object(result_view(&request.result, request.places)),
    );
    Ok(Value::Object(out))
}

/// A request's number as `target` says it: `3` or `#3`.
pub(crate) fn number(raw: &str) -> Result<i64, String> {
    raw.trim_matches(['#', ' '])
        .parse::<i64>()
        .ok()
        .filter(|n| *n > 0)
        .ok_or_else(|| text!("requests.numberOf"))
}

impl crate::Core {
    /// `mr_list`, `mr_read`, `mr_delete`. A merge request is made by `branch_merge` into a
    /// locked main (`merges`), not by a tool of its own.
    pub(crate) fn mr_tool(
        &self,
        who: &crate::write::Who,
        name: &str,
        args: &super::Args,
    ) -> Result<crate::mcp::Output, String> {
        use crate::mcp::Output;
        let summary = super::project_summary(&*self.reading()?, args.string("project")?)?;
        let headed = |mut body: Map<String, Value>, rest: Map<String, Value>| {
            body.extend(rest);
            Ok(Output::Json(Value::Object(body)))
        };
        let mut out = Map::new();
        out.insert("project".into(), summary.name.clone().into());
        match name {
            "mr_list" => {
                let given = args.value()?;
                super::check(&given, "mr_list", "project")?;
                // Worked out in a transaction that is let go (`list`).
                let mut all = {
                    let mut store = self.lock();
                    let mut tx = store.write(who.clone())?;
                    list(&mut tx, &summary.id, None)?
                };
                if let Some(state) = given.get("state").and_then(Value::as_str) {
                    all.retain(|r| r.state == state);
                }
                out.insert(
                    "mrs".into(),
                    Value::Array(all.iter().map(|r| Value::Object(row(r))).collect()),
                );
                Ok(Output::Json(Value::Object(out)))
            }
            "mr_read" => {
                let at = number(&args.target()?)?;
                let request = {
                    let mut store = self.lock();
                    let mut tx = store.write(who.clone())?;
                    one(&mut tx, &summary.id, at)?
                };
                let mut rest = row(&request);
                if !request.description.is_empty() {
                    rest.insert("description".into(), request.description.clone().into());
                }
                rest.insert(
                    "result".into(),
                    Value::Object(result_view(&request.result, request.places)),
                );
                headed(out, rest)
            }
            _ => {
                let at = number(&args.target()?)?;
                self.plain_writing(who.clone(), |tx| {
                    withdraw(tx, &summary.id, at, who.name.as_deref())
                })?;
                out.insert("mr".into(), at.into());
                out.insert("state".into(), "withdrawn".into());
                Ok(Output::Json(Value::Object(out)))
            }
        }
    }
}

// MARK: - The screen's requests

/// A preview as the window reads it: every item and conflict with its kind and the three
/// versions as they are kept.
/// A conflicting unit's version as the window shows it. A row kept apart by id (`r:`) is
/// said by what it says — its own fields first, then the rows it holds, each the same way —
/// without the ids the merge keeps it by, which tell a person nothing and crowd out the rest
/// of the little the window shows of it.
fn on_screen(unit: &str, value: Option<&Value>) -> Value {
    fn row(value: &Value) -> Value {
        let Some(o) = value.as_object() else {
            return value.clone();
        };
        let mut out = Map::new();
        if let Some(Value::Object(fields)) = o.get("fields") {
            out.extend(fields.clone());
        }
        for (key, v) in o {
            match (key.as_str(), v) {
                ("id" | "fields" | "~parent", _) => {}
                ("rows", Value::Object(tables)) => {
                    for (name, list) in tables {
                        let list = list.as_array().map_or_else(
                            || list.clone(),
                            |l| Value::Array(l.iter().map(row).collect()),
                        );
                        out.insert(name.clone(), list);
                    }
                }
                _ => {
                    out.insert(key.clone(), v.clone());
                }
            }
        }
        Value::Object(out)
    }
    match value {
        None => Value::Null,
        Some(v) if unit.starts_with("r:") => row(v),
        Some(v) => v.clone(),
    }
}

fn preview_json(preview: &MergePreview) -> Value {
    serde_json::json!({
        "source": preview.source,
        "target": preview.target,
        "items": preview.items.iter().map(|i| serde_json::json!({
            "path": i.path,
            "section": i.section,
            "change": match i.change {
                MergeChange::Add => "add",
                MergeChange::Change => "change",
                MergeChange::Remove => "remove",
            },
        })).collect::<Vec<_>>(),
        "conflicts": preview.conflicts.iter().map(|c| serde_json::json!({
            "path": c.path,
            "section": c.section,
            "unit": c.unit,
            "label": c.label,
            "base": on_screen(&c.unit, c.base.as_ref()),
            "source": on_screen(&c.unit, c.source.as_ref()),
            "target": on_screen(&c.unit, c.target.as_ref()),
        })).collect::<Vec<_>>(),
        "overlaps": preview.overlaps,
        "counts": preview.counts(),
        "places": preview.places,
    })
}

fn request_json(request: &Request) -> Value {
    serde_json::json!({
        "number": request.number,
        "branch": request.branch_id,
        "branchName": request.branch_name,
        "title": request.title,
        "description": request.description,
        "author": request.author,
        "authorLabel": request.author_label(),
        "at": request.at,
        "state": request.state,
        "reason": reason_said(request),
        "result": result_said(&request.result),
        "places": request.places,
        "keep": request.keep,
    })
}

impl crate::Core {
    /// The window's merges and requests (`specboard://SpecBoard/Docs/Design:CoreApi`); none when `op` is not one
    /// of them.
    pub(crate) fn merge_op(&self, op: &str, request: &Value) -> Option<Result<Value, String>> {
        let known = [
            "merge_preview",
            "merge",
            "merge_requests",
            "merge_request_approve",
            "merge_request_reject",
            "merge_result_open",
            "merge_request_open",
            "merge_result_close",
        ];
        known.contains(&op).then(|| self.merge_request(op, request))
    }

    fn merge_request(&self, op: &str, request: &Value) -> Result<Value, String> {
        let text = |key: &str| request.get(key).and_then(Value::as_str);
        let project = text("project")
            .ok_or_else(|| text!("requests.projectNeeded", op))?
            .to_string();
        if self.reading()?.project(&project)?.is_none() {
            return Err(text!("requests.projectNotFound"));
        }
        let number = || {
            request
                .get("number")
                .and_then(Value::as_i64)
                .ok_or_else(|| text!("requests.numberNeeded", op))
        };
        let who = crate::write::Who::screen_for(request);
        match op {
            "merge_preview" | "merge" => {
                let source = text("source").unwrap_or("").to_string();
                let target = text("target").unwrap_or("").to_string();
                let take = merges::take(request.get("take"))?;
                let places = request
                    .get("places")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                if op == "merge_preview" {
                    let mut store = self.lock();
                    // Worked out in a transaction that is let go.
                    let tx = store.write(who)?;
                    return Ok(preview_json(
                        &tx.merge_preview(&project, &source, &target, &take)?,
                    ));
                }
                // The branch merged into main set aside, unless the person keeps it.
                let keep = request
                    .get("keep")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                self.plain_writing(who, |tx| {
                    tx.merge(&project, &source, &target, &take, places, keep)
                        .map(|p| preview_json(&p))
                })
            }
            // What a merge into main would leave, as a branch to open and look at
            // (`Tx::create_result_branch`): the merge window's, or a merge request's
            // as it was asked for. Main only: the merge window's into another branch is not shown so.
            "merge_result_open" | "merge_request_open" => {
                let (source, take, places) = if op == "merge_request_open" {
                    let at = number()?;
                    let asked = {
                        let mut store = self.lock();
                        let mut tx = store.write(who.clone())?;
                        one(&mut tx, &project, at)?
                    };
                    let take_text: String = self
                        .lock()
                        .conn
                        .query_row(
                            "SELECT take FROM merge_requests WHERE project_id = ? AND number = ?",
                            params![project, at],
                            |r| r.get(0),
                        )
                        .map_err(db)?;
                    let take =
                        merges::take(serde_json::from_str::<Value>(&take_text).ok().as_ref())?;
                    // As the person has the toggle, else as it asks.
                    let places = request
                        .get("places")
                        .and_then(Value::as_bool)
                        .unwrap_or(asked.places);
                    (asked.branch_id.clone(), take, places)
                } else {
                    if text("target").is_some_and(|t| !t.is_empty()) {
                        return Err(text!("requests.resultOnlyMain"));
                    }
                    let places = request
                        .get("places")
                        .and_then(Value::as_bool)
                        .unwrap_or(false);
                    (
                        text("source").unwrap_or("").to_string(),
                        merges::take(request.get("take"))?,
                        places,
                    )
                };
                if source.is_empty() {
                    return Err(text!("requests.sourceMissing"));
                }
                self.plain_writing(who, |tx| {
                    let branch = tx.create_result_branch(&project)?;
                    let done = tx.merge(&project, &source, &branch, &take, places, true)?;
                    // The merge's preview, as `merge` answers, with the branch to open.
                    let mut out = preview_json(&done);
                    out["branch"] = branch.into();
                    Ok(out)
                })
            }
            "merge_result_close" => self.plain_writing(who, |tx| {
                tx.drop_result_branches(&project)
                    .map(|_| serde_json::json!({}))
            }),
            "merge_requests" => {
                let all = {
                    let mut store = self.lock();
                    let mut tx = store.write(who.clone())?;
                    list(&mut tx, &project, None)?
                };
                Ok(
                    serde_json::json!({ "requests": all.iter().map(request_json).collect::<Vec<_>>() }),
                )
            }
            "merge_request_approve" => {
                let at = number()?;
                let places = request.get("places").and_then(Value::as_bool);
                self.plain_writing(who, |tx| {
                    approve(tx, &project, at, places).map(|(done, taken)| {
                        let mut out = preview_json(&done);
                        if !taken && let Some(out) = out.as_object_mut() {
                            say_places_left(out, done.places, text!("requests.placesLeft"));
                        }
                        out
                    })
                })
            }
            _ => {
                let at = number()?;
                let reason = text("reason").unwrap_or("").to_string();
                self.plain_writing(who, |tx| {
                    reject(tx, &project, at, &reason).map(|_| serde_json::json!({}))
                })
            }
        }
    }
}
