//! Rows of a table as JSON objects (`RowCodec` in the Swift app's Store): keyed by the
//! table's columns in its order, a value equal to the column's default left out and so
//! is a null, a column holding JSON as the JSON itself.

use crate::store::sql;
use rusqlite::Connection;
use rusqlite::types::{Value as Sql, ValueRef};
use serde_json::{Map, Value};
use std::collections::HashMap;

/// One table, and how its rows sit in an object.
pub struct Table<'a> {
    /// The column tying a row to what holds it, left out.
    pub owner: Option<&'a str>,
    /// Kept out.
    pub skip: &'a [&'a str],
    /// Columns holding JSON text, written as the JSON itself.
    pub json: &'a [&'a str],
}

/// A column as the store declares it.
pub struct Column {
    pub name: String,
    /// The literal it falls back to; none for no default or an expression.
    fallback: Option<Sql>,
    expression: bool,
}

impl Column {
    /// Whether leaving the value out gives the same row back.
    fn is_default(&self, value: &Sql) -> bool {
        if self.expression {
            return false;
        }
        match (value, &self.fallback) {
            (Sql::Null, None) => true,
            (Sql::Text(a), Some(Sql::Text(b))) => a == b,
            (Sql::Integer(a), Some(Sql::Integer(b))) => a == b,
            (Sql::Real(a), Some(Sql::Real(b))) => a == b,
            (Sql::Integer(a), Some(Sql::Real(b))) | (Sql::Real(b), Some(Sql::Integer(a))) => {
                *a as f64 == *b
            }
            _ => false,
        }
    }
}

pub fn columns(conn: &Connection, table: &str) -> Result<Vec<Column>, String> {
    let mut stmt = conn
        .prepare(&format!("PRAGMA table_info({table})"))
        .map_err(sql)?;
    let rows = stmt
        .query_map([], |r| {
            let name: String = r.get(1)?;
            let declared: String = r.get::<_, String>(2)?.to_uppercase();
            let raw: Option<String> = r.get(4)?;
            Ok((name, declared, raw))
        })
        .map_err(sql)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sql)?;
    Ok(rows
        .into_iter()
        .map(|(name, declared, raw)| {
            let mut fallback = None;
            let mut expression = false;
            if let Some(raw) = raw {
                if raw.len() >= 2 && raw.starts_with('\'') && raw.ends_with('\'') {
                    fallback = Some(Sql::Text(raw[1..raw.len() - 1].replace("''", "'")));
                } else if let Ok(i) = raw.parse::<i64>() {
                    fallback = Some(if declared == "REAL" {
                        Sql::Real(i as f64)
                    } else {
                        Sql::Integer(i)
                    });
                } else if let Ok(f) = raw.parse::<f64>() {
                    fallback = Some(Sql::Real(f));
                } else if raw.to_uppercase() != "NULL" {
                    expression = true;
                }
            }
            Column {
                name,
                fallback,
                expression,
            }
        })
        .collect())
}

pub type Row = HashMap<String, Sql>;

/// The rows `clause` picks, every column as stored.
pub fn rows(
    conn: &Connection,
    table: &str,
    clause: &str,
    binds: &[&dyn rusqlite::ToSql],
) -> Result<Vec<Row>, String> {
    let names: Vec<String> = columns(conn, table)?.into_iter().map(|c| c.name).collect();
    let list = names
        .iter()
        .map(|n| format!("\"{n}\""))
        .collect::<Vec<_>>()
        .join(", ");
    let mut stmt = conn
        .prepare(&format!("SELECT {list} FROM {table} WHERE {clause}"))
        .map_err(sql)?;
    let out = stmt
        .query_map(binds, |r| {
            let mut row = Row::new();
            for (i, name) in names.iter().enumerate() {
                let value = match r.get_ref(i)? {
                    ValueRef::Null => Sql::Null,
                    ValueRef::Integer(n) => Sql::Integer(n),
                    ValueRef::Real(f) => Sql::Real(f),
                    ValueRef::Text(t) => Sql::Text(String::from_utf8_lossy(t).into_owned()),
                    ValueRef::Blob(b) => Sql::Blob(b.to_vec()),
                };
                row.insert(name.clone(), value);
            }
            Ok(row)
        })
        .map_err(sql)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sql)?;
    Ok(out)
}

/// A row as its object.
pub fn object(row: &Row, table: &Table, columns: &[Column]) -> Map<String, Value> {
    let mut out = Map::new();
    for column in columns {
        if Some(column.name.as_str()) == table.owner || table.skip.contains(&column.name.as_str()) {
            continue;
        }
        let value = row.get(&column.name).cloned().unwrap_or(Sql::Null);
        if column.is_default(&value) {
            continue;
        }
        let json = if table.json.contains(&column.name.as_str()) {
            as_json(&value)
        } else {
            plain(&value)
        };
        out.insert(column.name.clone(), json);
    }
    out
}

/// A JSON column's text as the JSON it holds; text that is not JSON kept as it is, marked.
fn as_json(value: &Sql) -> Value {
    match value {
        Sql::Text(text) => serde_json::from_str(text).unwrap_or_else(|_| {
            let mut marked = Map::new();
            marked.insert("$text".into(), Value::String(text.clone()));
            Value::Object(marked)
        }),
        other => plain(other),
    }
}

fn plain(value: &Sql) -> Value {
    match value {
        Sql::Null | Sql::Blob(_) => Value::Null,
        Sql::Integer(n) => Value::from(*n as f64),
        Sql::Real(f) => Value::from(*f),
        Sql::Text(t) => Value::String(t.clone()),
    }
}

/// Text of a column, or "".
pub fn text(row: &Row, key: &str) -> String {
    match row.get(key) {
        Some(Sql::Text(t)) => t.clone(),
        _ => String::new(),
    }
}

/// Rows in a fixed order: by these keys, integers as numbers, text as text.
pub fn by_order(mut list: Vec<Row>, keys: &[&str]) -> Vec<Row> {
    list.sort_by(|a, b| {
        for key in keys {
            let x = a.get(*key).unwrap_or(&Sql::Null);
            let y = b.get(*key).unwrap_or(&Sql::Null);
            match (x, y) {
                (Sql::Integer(i), Sql::Integer(j)) if i != j => return i.cmp(j),
                (Sql::Text(s), Sql::Text(t)) if s != t => return s.cmp(t),
                _ => {}
            }
        }
        std::cmp::Ordering::Equal
    });
    list
}
