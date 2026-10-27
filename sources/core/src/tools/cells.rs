//! A table row's cells changed in part: `card_update`'s `cells` as `{set, add, remove}` — a
//! cell written whole or changed as a body is (`edit`, `append`, `prepend`), a row added
//! before or after another, a row taken away. Every row is named over the table as it was
//! read, by what some of its columns hold (`{"<heading>": "<value>"}`) or by its number (0
//! from the top) with what it held (`old`), and a column by its heading: what another wrote
//! meanwhile is then neither written over nor taken for the row meant, and a name that fits
//! nothing, or more than one, is refused with what there is.

use serde_json::{Map, Value, json};

/// The keys `cells` is changed in part with.
const KEYS: [&str; 3] = ["set", "add", "remove"];

/// The most rows a refusal names.
const MOST_SHOWN: usize = 20;

/// The rows shown around each place a table changed. One either side places a change among the
/// rows, and a row holds a whole record, so more would be most of the answer.
const CHANGE_CONTEXT: usize = 1;

/// The most rows shown of one place changed: its first and last half of them, the rest counted.
const MOST_SHOWN_ROWS: usize = 6;

/// The most places changed shown; how many there were besides is said.
const MOST_CHANGES: usize = 20;

/// The most rows either side of what differs that are compared row by row; past that, what
/// differs is one place.
const MOST_COMPARED: usize = 1000;

/// The table's rows once `given` (`{set, add, remove}`) is made over `cells`, under the
/// headings `columns`, with the rows of `cells` it took away (their numbers). `at` names the cells
/// in a refusal.
pub(crate) fn changed(
    columns: &[String],
    cells: &[Vec<String>],
    given: &Map<String, Value>,
    at: &str,
) -> Result<(Vec<Vec<String>>, Vec<usize>), String> {
    if let Some(stray) = given.keys().find(|k| !KEYS.contains(&k.as_str())) {
        return Err(text!("cells.error.strayKey", at, stray));
    }
    if given.is_empty() {
        return Err(text!("cells.error.noChange", at));
    }
    let table = Table { columns, cells };
    let list = |key: &str| -> Result<Vec<&Map<String, Value>>, String> {
        match given.get(key) {
            None => Ok(Vec::new()),
            Some(Value::Array(items)) => items
                .iter()
                .enumerate()
                .map(|(i, item)| {
                    item.as_object()
                        .ok_or_else(|| text!("cells.error.itemNotObject", at, key, i))
                })
                .collect(),
            Some(_) => Err(text!("cells.error.notList", at, key)),
        }
    };
    let stray = |item: &Map<String, Value>, here: &str, keys: &[&str]| -> Result<(), String> {
        match item.keys().find(|k| !keys.contains(&k.as_str())) {
            Some(k) => Err(text!(
                "cells.error.keyNotWritable",
                here,
                k,
                keys = crate::text::list(keys)
            )),
            None => Ok(()),
        }
    };
    let mut out: Vec<Vec<String>> = cells.to_vec();
    let mut written: Vec<(usize, String)> = Vec::new();
    for (i, set) in list("set")?.into_iter().enumerate() {
        let here = format!("{at}.set[{i}]");
        stray(set, &here, &["row", "col", "value", "old"])?;
        let col = table.column(set.get("col"), &format!("{here}.col"))?;
        let row = table.row(
            set.get("row"),
            &here,
            |r| Old::Cell(table.cell(r, col).to_string()),
            set.get("old"),
        )?;
        let value = set
            .get("value")
            .ok_or_else(|| text!("cells.error.needsValue", here))?;
        let line = &mut out[row];
        if line.len() <= col {
            line.resize(col + 1, String::new());
        }
        line[col] = match value {
            Value::String(text) => text.clone(),
            Value::Object(_) => {
                super::in_part::cell_changed(&line[col], value, &format!("{here}.value"))?
                    .unwrap_or_default()
            }
            _ => {
                return Err(text!("cells.error.valueNotText", here));
            }
        };
        written.push((row, here));
    }
    let mut gone: Vec<(usize, String)> = Vec::new();
    for (i, remove) in list("remove")?.into_iter().enumerate() {
        let here = format!("{at}.remove[{i}]");
        stray(remove, &here, &["row", "old"])?;
        let row = table.row(
            remove.get("row"),
            &here,
            |r| Old::Row(table.cells[r].clone()),
            remove.get("old"),
        )?;
        if let Some((_, first)) = gone.iter().find(|(r, _)| *r == row) {
            return Err(text!("cells.error.sameRow", here, first));
        }
        if let Some((_, set)) = written.iter().find(|(r, _)| *r == row) {
            return Err(text!("cells.error.writtenAndRemoved", here, set));
        }
        gone.push((row, here));
    }
    // Rows added, by where: before a row, after it, or at the end.
    let mut before: Vec<Vec<Vec<String>>> = vec![Vec::new(); cells.len()];
    let mut after: Vec<Vec<Vec<String>>> = vec![Vec::new(); cells.len()];
    let mut at_end: Vec<Vec<String>> = Vec::new();
    for (i, add) in list("add")?.into_iter().enumerate() {
        let here = format!("{at}.add[{i}]");
        stray(add, &here, &["cells", "before", "after", "old"])?;
        let line: Vec<String> = add
            .get("cells")
            .and_then(Value::as_array)
            .and_then(|l| l.iter().map(|c| c.as_str().map(str::to_string)).collect())
            .ok_or_else(|| text!("cells.error.addCells", here))?;
        match (add.get("before"), add.get("after")) {
            (Some(_), Some(_)) => {
                return Err(text!("cells.error.beforeAndAfter", here));
            }
            (Some(which), None) | (None, Some(which)) => {
                let row = table.row(
                    Some(which),
                    &here,
                    |r| Old::Row(table.cells[r].clone()),
                    add.get("old"),
                )?;
                if add.contains_key("before") {
                    before[row].push(line);
                } else {
                    after[row].push(line);
                }
            }
            (None, None) => {
                if add.contains_key("old") {
                    return Err(text!("cells.error.oldWithoutBeforeAfter", here));
                }
                at_end.push(line);
            }
        }
    }
    let removed: Vec<usize> = gone.iter().map(|(r, _)| *r).collect();
    let mut whole = Vec::new();
    for (r, line) in out.into_iter().enumerate() {
        whole.append(&mut before[r]);
        if !gone.iter().any(|(g, _)| *g == r) {
            whole.push(line);
        }
        whole.append(&mut after[r]);
    }
    whole.append(&mut at_end);
    Ok((whole, removed))
}

/// A table's cells changed in part as MCP is answered with them: not the rows, but how many there
/// are now (`rowCount`) and each place they changed (`changed`, as a body's places are:
/// `row`, `to`, `rows`, `before`, `after`). A place where rows were taken away says how many
/// (`removed`): in place of `rows` when nothing stands there now, beside them when rows were put
/// where others went — so a row taken away and one put in its place is not read as one put in
/// alone. Rows are numbered from 0, as `set`, `add` and `remove` name them; `gone` are the rows
/// of `before` the change took away.
pub(crate) fn quiet(before: &[Vec<String>], after: &[Vec<String>], gone: &[usize]) -> Value {
    let joined = places(before, after);
    let shown: Vec<Value> = joined
        .iter()
        .take(MOST_CHANGES)
        .map(|place_at| {
            let Place {
                from,
                to,
                removed,
                old_from,
                old_to,
            } = *place_at;
            let mut place = Map::new();
            place.insert("row".into(), from.into());
            if to > from + 1 {
                place.insert("to".into(), (to - 1).into());
            }
            let rows = &after[from..to];
            let taken = gone
                .iter()
                .filter(|r| (old_from..old_to).contains(*r))
                .count();
            if rows.is_empty() {
                place.insert("removed".into(), removed.into());
            } else {
                if taken > 0 {
                    place.insert("removed".into(), taken.into());
                }
                if rows.len() > MOST_SHOWN_ROWS {
                    let half = MOST_SHOWN_ROWS / 2;
                    let mut kept: Vec<&Vec<String>> = rows[..half].iter().collect();
                    kept.extend(&rows[rows.len() - half..]);
                    place.insert("rows".into(), json!(kept));
                    place.insert("omitted".into(), (rows.len() - MOST_SHOWN_ROWS).into());
                } else {
                    place.insert("rows".into(), json!(rows));
                }
            }
            let around_before = &after[from.saturating_sub(CHANGE_CONTEXT)..from];
            if !around_before.is_empty() {
                place.insert("before".into(), json!(around_before));
            }
            let around_after = &after[to.min(after.len())..(to + CHANGE_CONTEXT).min(after.len())];
            if !around_after.is_empty() {
                place.insert("after".into(), json!(around_after));
            }
            Value::Object(place)
        })
        .collect();
    let mut out = Map::new();
    out.insert("rowCount".into(), after.len().into());
    out.insert("changed".into(), Value::Array(shown));
    if joined.len() > MOST_CHANGES {
        out.insert("moreChanged".into(), (joined.len() - MOST_CHANGES).into());
    }
    Value::Object(out)
}

/// One place a table changed: the rows of the table now (`from..to`), how many rows of before it
/// stands for besides those it shares (`removed`), and the rows of before it covers
/// (`old_from..old_to`).
#[derive(Clone, Copy)]
struct Place {
    from: usize,
    to: usize,
    removed: usize,
    old_from: usize,
    old_to: usize,
}

/// Where `after` differs from `before`, places near each other shown as one (as a body's), each
/// with the rows of before it covers: what tells a row taken away from one written.
fn places(before: &[Vec<String>], after: &[Vec<String>]) -> Vec<Place> {
    let mut grown: isize = 0;
    let mut joined: Vec<Place> = Vec::new();
    for (from, to, removed) in super::in_part::changed_places(before, after, MOST_COMPARED) {
        // The rows between places are the same on both sides: a place's rows of before start as
        // far from its rows now as the places before it grew the table.
        let old_from = (from as isize - grown) as usize;
        grown += (to - from) as isize - removed as isize;
        let place = Place {
            from,
            to,
            removed,
            old_from,
            old_to: old_from + removed,
        };
        match joined.last_mut() {
            Some(last) if place.from <= last.to + 2 * CHANGE_CONTEXT => {
                last.to = last.to.max(place.to);
                last.removed += place.removed;
                last.old_to = place.old_to;
            }
            _ => joined.push(place),
        }
    }
    joined
}

/// An answer's `cells` (a table's cells changed in part from `before` to `after`, the rows
/// `gone` of before taken away) said short (`quiet`), in place of the rows.
pub(crate) fn quieted(
    answer: &mut Value,
    before: &[Vec<String>],
    after: &[Vec<String>],
    gone: &[usize],
) {
    if let Some(out) = answer.as_object_mut() {
        out.insert("cells".to_string(), quiet(before, after, gone));
    }
}

/// What a row named by its number is to hold still: a cell, or the whole row.
enum Old {
    Cell(String),
    Row(Vec<String>),
}

/// A table as it was read.
struct Table<'t> {
    columns: &'t [String],
    cells: &'t [Vec<String>],
}

impl Table<'_> {
    fn cell(&self, row: usize, col: usize) -> &str {
        self.cells[row].get(col).map_or("", String::as_str)
    }

    /// The column a heading names.
    fn column(&self, given: Option<&Value>, here: &str) -> Result<usize, String> {
        let name = given
            .and_then(Value::as_str)
            .ok_or_else(|| text!("cells.error.headingNeeded", here, columns = self.headings()))?;
        let at: Vec<usize> = self
            .columns
            .iter()
            .enumerate()
            .filter(|(_, c)| c.as_str() == name)
            .map(|(i, _)| i)
            .collect();
        match at.as_slice() {
            [one] => Ok(*one),
            [] => Err(text!(
                "cells.error.noSuchHeading",
                here,
                name,
                columns = self.headings()
            )),
            many => Err(text!(
                "cells.error.headingRepeated",
                here,
                name,
                count = many.len()
            )),
        }
    }

    /// The row `which` names: its number (0 from the top) with what it holds (`old`, as
    /// `held` says of it), or the values of some of its columns. `old` written with the values
    /// is held to as well.
    fn row(
        &self,
        which: Option<&Value>,
        here: &str,
        held: impl Fn(usize) -> Old,
        old: Option<&Value>,
    ) -> Result<usize, String> {
        let row = match which {
            Some(Value::Number(n)) => {
                let n = n
                    .as_u64()
                    .map(|n| n as usize)
                    .ok_or_else(|| text!("cells.error.rowNumber", here))?;
                if n >= self.cells.len() {
                    return Err(text!(
                        "cells.error.noRowNumber",
                        here,
                        n,
                        count = self.cells.len()
                    ));
                }
                if old.is_none() {
                    return Err(text!(
                        "cells.error.oldNeeded",
                        here,
                        held = match held(n) {
                            Old::Cell(_) => text!("cells.held.cell"),
                            Old::Row(_) => text!("cells.held.row"),
                        }
                    ));
                }
                n
            }
            Some(Value::Object(wanted)) if !wanted.is_empty() => {
                let mut by: Vec<(usize, &str)> = Vec::new();
                for (heading, value) in wanted {
                    let col = self.column(Some(&Value::String(heading.clone())), here)?;
                    let value = value
                        .as_str()
                        .ok_or_else(|| text!("cells.error.valueNotTextAt", here, heading))?;
                    by.push((col, value));
                }
                let fits: Vec<usize> = (0..self.cells.len())
                    .filter(|r| by.iter().all(|(c, v)| self.cell(*r, *c) == *v))
                    .collect();
                match fits.as_slice() {
                    [one] => *one,
                    [] => {
                        return Err(text!(
                            "cells.error.noRowFits",
                            here,
                            rows = self.shown(&(0..self.cells.len()).collect::<Vec<_>>())
                        ));
                    }
                    many => {
                        return Err(text!(
                            "cells.error.manyRowsFit",
                            here,
                            count = many.len(),
                            rows = self.shown(many)
                        ));
                    }
                }
            }
            _ => {
                return Err(text!("cells.error.rowNeeded", here));
            }
        };
        if let Some(old) = old {
            match held(row) {
                Old::Cell(now) => {
                    if old.as_str() != Some(now.as_str()) {
                        return Err(text!(
                            "cells.error.oldCellDiffers",
                            here,
                            now = Value::String(now)
                        ));
                    }
                }
                Old::Row(now) => {
                    let said: Option<Vec<&str>> = old
                        .as_array()
                        .and_then(|l| l.iter().map(Value::as_str).collect());
                    if said.as_deref()
                        != Some(&now.iter().map(String::as_str).collect::<Vec<_>>()[..])
                    {
                        return Err(text!(
                            "cells.error.oldRowDiffers",
                            here,
                            now = crate::ojson::compact(&serde_json::json!(now))
                        ));
                    }
                }
            }
        }
        Ok(row)
    }

    fn headings(&self) -> String {
        if self.columns.is_empty() {
            text!("cells.headings.none")
        } else {
            crate::text::list(self.columns)
        }
    }

    /// Rows by their numbers and cells, as many as a refusal names.
    fn shown(&self, rows: &[usize]) -> String {
        if rows.is_empty() {
            return text!("cells.rows.none");
        }
        let mut said: Vec<String> = rows
            .iter()
            .take(MOST_SHOWN)
            .map(|r| {
                format!(
                    "{r}: {}",
                    crate::ojson::compact(&serde_json::json!(self.cells[*r]))
                )
            })
            .collect();
        if rows.len() > MOST_SHOWN {
            said.push(text!("cells.rows.more", count = rows.len() - MOST_SHOWN));
        }
        crate::text::list(&said)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(rows: usize) -> Vec<Vec<String>> {
        (0..rows)
            .map(|i| vec![format!("r{i}"), "x".into()])
            .collect()
    }

    #[test]
    fn places_far_apart_are_two_and_places_beside_each_other_one() {
        let before = table(30);
        let mut after = before.clone();
        after[3][1] = "y".into();
        after[20][1] = "y".into();
        let said = quiet(&before, &after, &[]);
        let rows: Vec<&Value> = said["changed"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| &p["row"])
            .collect();
        assert_eq!(rows, [3, 20]);
        // Two rows apart, their rows beside them would overlap: one place.
        after[5][1] = "y".into();
        let said = quiet(&before, &after, &[]);
        assert_eq!(said["changed"][0]["row"], 3);
        assert_eq!(said["changed"][0]["to"], 5);
        assert_eq!(said["changed"][0]["rows"].as_array().unwrap().len(), 3);
        assert_eq!(said["changed"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn a_table_changed_nowhere_says_so() {
        let rows = table(4);
        let said = quiet(&rows, &rows, &[]);
        assert_eq!(said["rowCount"], 4);
        assert_eq!(said["changed"], json!([]));
    }

    #[test]
    fn a_table_changed_in_more_places_than_are_shown_counts_the_rest() {
        let before = table(100);
        let mut after = before.clone();
        for i in (0..100).step_by(4).take(25) {
            after[i][1] = "y".into();
        }
        let said = quiet(&before, &after, &[]);
        assert_eq!(said["changed"].as_array().unwrap().len(), MOST_CHANGES);
        assert_eq!(said["moreChanged"], 5);
    }

    /// A table of ten rows named H000…H009, as the issue's: the rows are told apart by `id`.
    fn hosts() -> (Vec<String>, Vec<Vec<String>>) {
        let columns = vec!["id".to_string(), "name".to_string()];
        let rows = (0..10)
            .map(|i| vec![format!("H{i:03}"), "n".into()])
            .collect();
        (columns, rows)
    }

    fn made(given: Value) -> Value {
        let (columns, rows) = hosts();
        let (after, gone) =
            changed(&columns, &rows, given.as_object().unwrap(), "hosts.cells").unwrap();
        quiet(&rows, &after, &gone)
    }

    #[test]
    fn a_row_taken_away_where_one_is_put_says_both() {
        // The last row taken away and one put at the end: the same place.
        let said = made(json!({
            "remove": [{"row": {"id": "H009"}}],
            "add": [{"cells": ["HNEW", "x"]}]
        }));
        assert_eq!(said["rowCount"], 10);
        let places = said["changed"].as_array().unwrap();
        assert_eq!(places.len(), 1, "{said}");
        assert_eq!(places[0]["row"], 9);
        assert_eq!(places[0]["removed"], 1, "{said}");
        assert_eq!(places[0]["rows"], json!([["HNEW", "x"]]));
        // A row in the middle taken away and one put after the row before it.
        let said = made(json!({
            "remove": [{"row": {"id": "H004"}}],
            "add": [{"cells": ["HN2", "x"], "after": {"id": "H003"}}]
        }));
        let places = said["changed"].as_array().unwrap();
        assert_eq!(places.len(), 1, "{said}");
        assert_eq!(places[0]["row"], 4);
        assert_eq!(places[0]["removed"], 1, "{said}");
        assert_eq!(places[0]["rows"], json!([["HN2", "x"]]));
        assert_eq!(places[0]["before"], json!([["H003", "n"]]));
        assert_eq!(places[0]["after"], json!([["H005", "n"]]));
    }

    #[test]
    fn a_cell_written_says_its_row_without_a_removal_and_rows_only_taken_away_say_how_many() {
        let said = made(json!({"set": [{"row": {"id": "H004"}, "col": "name", "value": "m"}]}));
        let place = &said["changed"][0];
        assert_eq!(place["rows"], json!([["H004", "m"]]));
        assert!(place.get("removed").is_none(), "{said}");
        let said = made(json!({"remove": [{"row": {"id": "H001"}}, {"row": {"id": "H002"}}]}));
        let place = &said["changed"][0];
        assert_eq!(place["row"], 1);
        assert_eq!(place["removed"], 2);
        assert!(place.get("rows").is_none(), "{said}");
        // A row taken away after places that grew the table is still found where it was.
        let said = made(json!({
            "add": [{"cells": ["A", "x"], "before": {"id": "H000"}}, {"cells": ["B", "x"], "before": {"id": "H000"}}],
            "remove": [{"row": {"id": "H007"}}],
            "set": [{"row": {"id": "H008"}, "col": "name", "value": "m"}]
        }));
        let places = said["changed"].as_array().unwrap();
        assert_eq!(places.len(), 2, "{said}");
        assert!(places[0].get("removed").is_none(), "{said}");
        assert_eq!(places[1]["removed"], 1, "{said}");
        assert_eq!(places[1]["rows"], json!([["H008", "m"]]));
    }
}
