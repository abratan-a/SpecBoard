//! A table row's cells changed in part: `card_update`'s `cells` as `{set, add, remove}` — a
//! cell written whole or changed as a body is (`edit`, `append`, `prepend`), a row added
//! before or after another, a row taken away. Every row is named over the table as it was
//! read, by what some of its columns hold (`{"<heading>": "<value>"}`) or by its number (0
//! from the top) with what it held (`old`), and a column by its heading: what another wrote
//! meanwhile is then neither written over nor taken for the row meant, and a name that fits
//! nothing, or more than one, is refused with what there is.

use serde_json::{Map, Value};

/// The keys `cells` is changed in part with.
const KEYS: [&str; 3] = ["set", "add", "remove"];

/// The most rows a refusal names.
const MOST_SHOWN: usize = 20;

/// The table's rows once `given` (`{set, add, remove}`) is made over `cells`, under the
/// headings `columns`. `at` names the cells in a refusal.
pub(crate) fn changed(
    columns: &[String],
    cells: &[Vec<String>],
    given: &Map<String, Value>,
    at: &str,
) -> Result<Vec<Vec<String>>, String> {
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
    let mut whole = Vec::new();
    for (r, line) in out.into_iter().enumerate() {
        whole.append(&mut before[r]);
        if !gone.iter().any(|(g, _)| *g == r) {
            whole.push(line);
        }
        whole.append(&mut after[r]);
    }
    whole.append(&mut at_end);
    Ok(whole)
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
