//! The parts of a project as SP's types read and write them (`Codable` in the Swift app's
//! Model): each reads a stored JSON value the way the type decodes it — what it
//! requires, what it fills in, what it takes under an older name — and gives it back
//! the way the type encodes it — what it leaves out when empty or absent.
//!
//! A value the type would refuse is `Err(())`. SP reads a list of them all or nothing:
//! one it cannot read leaves the whole list at its fallback (`JSONValue.decode`), so
//! the callers here do the same.

use serde_json::{Map, Value};

pub type Obj = Map<String, Value>;
pub type Read<T> = Result<T, ()>;

// MARK: - Reading keys as a decoder does

fn get<'a>(o: &'a Obj, key: &str) -> Option<&'a Value> {
    match o.get(key) {
        None | Some(Value::Null) => None,
        Some(v) => Some(v),
    }
}

/// `decodeIfPresent(String.self)`: absent or null is none; anything but a string refuses.
pub fn opt_string(o: &Obj, key: &str) -> Read<Option<String>> {
    match get(o, key) {
        None => Ok(None),
        Some(Value::String(s)) => Ok(Some(s.clone())),
        Some(_) => Err(()),
    }
}

/// `decode(String.self)`.
pub fn req_string(o: &Obj, key: &str) -> Read<String> {
    opt_string(o, key)?.ok_or(())
}

pub fn string_list(v: &Value) -> Read<Vec<String>> {
    match v {
        Value::Array(items) => items
            .iter()
            .map(|i| i.as_str().map(str::to_string).ok_or(()))
            .collect(),
        _ => Err(()),
    }
}

/// `decodeIfPresent([String].self) ?? []`.
pub fn opt_strings(o: &Obj, key: &str) -> Read<Vec<String>> {
    match get(o, key) {
        None => Ok(Vec::new()),
        Some(v) => string_list(v),
    }
}

pub fn opt_bool(o: &Obj, key: &str) -> Read<Option<bool>> {
    match get(o, key) {
        None => Ok(None),
        Some(Value::Bool(b)) => Ok(Some(*b)),
        Some(_) => Err(()),
    }
}

/// An `Int`: a number with no fraction.
pub fn int_of(v: &Value) -> Read<i64> {
    match v {
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(i)
            } else if let Some(f) = n.as_f64() {
                if f.fract() == 0.0 && f.abs() < 9.0e15 {
                    Ok(f as i64)
                } else {
                    Err(())
                }
            } else {
                Err(())
            }
        }
        _ => Err(()),
    }
}

pub fn req_int(o: &Obj, key: &str) -> Read<i64> {
    get(o, key).ok_or(()).and_then(int_of)
}

fn object(v: &Value) -> Read<&Obj> {
    v.as_object().ok_or(())
}

/// A list of one type: all of it, or refused.
pub fn list<T>(v: &Value, each: impl Fn(&Value) -> Read<T>) -> Read<Vec<T>> {
    match v {
        Value::Array(items) => items.iter().map(each).collect(),
        _ => Err(()),
    }
}

fn opt_list<T>(o: &Obj, key: &str, each: impl Fn(&Value) -> Read<T>) -> Read<Vec<T>> {
    match get(o, key) {
        None => Ok(Vec::new()),
        Some(v) => list(v, each),
    }
}

// MARK: - Writing keys as an encoder does

struct Out(Obj);

impl Out {
    fn new() -> Out {
        Out(Map::new())
    }
    fn put(&mut self, key: &str, value: impl Into<Value>) {
        self.0.insert(key.to_string(), value.into());
    }
    fn some(&mut self, key: &str, value: &Option<String>) {
        if let Some(v) = value {
            self.put(key, v.clone());
        }
    }
    fn strings(&mut self, key: &str, list: &[String]) {
        if !list.is_empty() {
            self.put(key, list.to_vec());
        }
    }
    fn text(&mut self, key: &str, text: &str) {
        if !text.is_empty() {
            self.put(key, text);
        }
    }
    fn values(&mut self, key: &str, list: Vec<Value>) {
        if !list.is_empty() {
            self.put(key, Value::Array(list));
        }
    }
    fn done(self) -> Value {
        Value::Object(self.0)
    }
}

// MARK: - Flows

/// The flows a node, an answer or a row names by its peer lines, in their order; a single
/// `peer` of before is the one.
fn peers(o: &Obj) -> Read<Vec<String>> {
    let mut out = opt_strings(o, "peers")?;
    if let Some(one) = opt_string(o, "peer")?
        && !out.contains(&one)
    {
        out.push(one);
    }
    Ok(out)
}

/// A flow's node (`FlowNode`).
pub fn flow_node(v: &Value) -> Read<Value> {
    let o = object(v)?;
    let name = req_string(o, "name")?;
    let kind = req_string(o, "kind")?;
    let label = opt_string(o, "label")?;
    let description = match opt_string(o, "description")? {
        Some(d) => Some(d),
        None => opt_string(o, "note")?,
    };
    let trigger = opt_string(o, "trigger")?;
    let event = match get(o, "event") {
        None => None,
        Some(e) => Some(flow_event(e)?),
    };
    let every = opt_string(o, "every")?;
    let target = opt_string(o, "target")?;
    let targets = opt_strings(o, "targets")?;
    let join = opt_string(o, "join")?;
    let partial = opt_string(o, "partial")?;
    let on_error = opt_string(o, "onError")?;
    let value = opt_string(o, "value")?;
    let branches = opt_list(o, "branches", flow_branch)?;
    let otherwise = opt_string(o, "else")?;
    let wait_for = opt_string(o, "for")?;
    let timeout = opt_string(o, "timeout")?;
    let on_timeout = opt_string(o, "onTimeout")?;
    let outcome = opt_string(o, "outcome")?;
    let reply = match get(o, "return") {
        None => None,
        Some(r) => Some(flow_return(r)?),
    };
    let steps = opt_list(o, "steps", step)?;
    let peers = peers(o)?;
    let program = opt_string(o, "program")?;
    let carries = opt_strings(o, "carries")?;
    let returns = opt_strings(o, "returns")?;
    let via = opt_string(o, "via")?;
    let initiator = opt_string(o, "initiator")?;
    let verify_with = opt_string(o, "verifyWith")?;
    let sign_with = opt_string(o, "signWith")?;
    // One next, or several to part the way.
    let nexts = match get(o, "next") {
        None => Vec::new(),
        Some(Value::String(one)) => vec![one.clone()],
        Some(other) => string_list(other)?,
    };
    let top_left = match get(o, "topLeft") {
        None => None,
        Some(p) => Some(grid_point(p)?),
    };
    let at = match get(o, "at") {
        None => None,
        Some(c) => Some(flow_cell(c)?),
    };

    let mut out = Out::new();
    out.put("name", name);
    out.put("kind", kind);
    out.some("label", &label);
    out.some("description", &description);
    out.some("trigger", &trigger);
    if let Some(e) = event {
        out.put("event", e);
    }
    out.some("every", &every);
    out.some("target", &target);
    out.strings("targets", &targets);
    out.some("join", &join);
    out.some("partial", &partial);
    out.some("onError", &on_error);
    out.some("value", &value);
    out.values("branches", branches);
    out.some("else", &otherwise);
    out.some("for", &wait_for);
    out.some("timeout", &timeout);
    out.some("onTimeout", &on_timeout);
    out.some("outcome", &outcome);
    if let Some(r) = reply {
        out.put("return", r);
    }
    out.values("steps", steps);
    out.strings("peers", &peers);
    out.some("program", &program);
    out.strings("carries", &carries);
    out.strings("returns", &returns);
    out.some("via", &via);
    out.some("initiator", &initiator);
    out.some("verifyWith", &verify_with);
    out.some("signWith", &sign_with);
    match nexts.len() {
        0 => {}
        1 => out.put("next", nexts[0].clone()),
        _ => out.put("next", nexts),
    }
    if let Some(p) = top_left {
        out.put("topLeft", p);
    }
    if let Some(c) = at {
        out.put("at", c);
    }
    Ok(out.done())
}

fn flow_event(v: &Value) -> Read<Value> {
    let o = object(v)?;
    let mut out = Out::new();
    out.put("flow", req_string(o, "flow")?);
    out.put("outcome", req_string(o, "outcome")?);
    Ok(out.done())
}

fn flow_branch(v: &Value) -> Read<Value> {
    let o = object(v)?;
    let mut out = Out::new();
    out.put("when", req_string(o, "when")?);
    out.put("next", req_string(o, "next")?);
    Ok(out.done())
}

fn flow_return(v: &Value) -> Read<Value> {
    let o = object(v)?;
    let carries = opt_strings(o, "carries")?;
    let peers = peers(o)?;
    let program = opt_string(o, "program")?;
    let sign_with = opt_string(o, "signWith")?;
    let mut out = Out::new();
    out.strings("carries", &carries);
    out.strings("peers", &peers);
    out.some("program", &program);
    out.some("signWith", &sign_with);
    Ok(out.done())
}

/// Where something sits on the grid, held within its reach.
pub const GRID_REACH: i64 = 1_000_000;

pub fn grid_point(v: &Value) -> Read<Value> {
    let o = object(v)?;
    let within = |n: i64| n.clamp(-GRID_REACH, GRID_REACH);
    let mut out = Out::new();
    out.put("x", within(req_int(o, "x")?));
    out.put("y", within(req_int(o, "y")?));
    Ok(out.done())
}

fn flow_cell(v: &Value) -> Read<Value> {
    let o = object(v)?;
    let mut out = Out::new();
    out.put("r", req_int(o, "r")?);
    out.put("c", req_int(o, "c")?);
    Ok(out.done())
}

/// A row of a process node, or of an action from before (`Step`).
pub fn step(v: &Value) -> Read<Value> {
    let o = object(v)?;
    let op = opt_string(o, "do")?.unwrap_or_default();
    let peers = peers(o)?;
    let program = opt_string(o, "program")?;
    let carries = opt_strings(o, "carries")?;
    let returns = opt_strings(o, "returns")?;
    let paths = opt_strings(o, "paths")?;
    let key = opt_string(o, "key")?;
    let sign_with = opt_string(o, "signWith")?;
    let verify_with = opt_string(o, "verifyWith")?;
    let via = opt_string(o, "via")?;
    let initiator = opt_string(o, "initiator")?;
    let action = opt_string(o, "action")?;
    let when = opt_string(o, "when")?;
    let over = opt_string(o, "over")?;
    let description = match opt_string(o, "description")? {
        Some(d) => Some(d),
        None => opt_string(o, "comment")?,
    };
    let steps = opt_list(o, "steps", step)?;
    let cases = opt_list(o, "cases", step)?;

    let mut out = Out::new();
    out.text("do", &op);
    out.strings("peers", &peers);
    out.some("program", &program);
    out.strings("carries", &carries);
    out.strings("returns", &returns);
    out.strings("paths", &paths);
    out.some("key", &key);
    out.some("signWith", &sign_with);
    out.some("verifyWith", &verify_with);
    out.some("via", &via);
    out.some("initiator", &initiator);
    out.some("action", &action);
    out.some("when", &when);
    out.some("over", &over);
    out.some("description", &description);
    out.values("steps", steps);
    out.values("cases", cases);
    Ok(out.done())
}

// MARK: - Documents

const ROW_KINDS: [&str; 6] = ["item", "text", "table", "diagram", "diagrams", "card"];

/// A formatdoc's row (`FormatRow`).
pub fn format_row(v: &Value) -> Read<Value> {
    let o = object(v)?;
    let name = req_string(o, "name")?;
    let kind = match opt_string(o, "kind")? {
        None => "item".to_string(),
        Some(k) if ROW_KINDS.contains(&k.as_str()) => k,
        Some(_) => return Err(()),
    };
    let heading = opt_string(o, "heading")?.unwrap_or_default();
    let body = opt_string(o, "body")?.unwrap_or_default();
    let rows = opt_list(o, "rows", format_row)?;
    let columns = opt_strings(o, "columns")?;
    let cells = opt_list(o, "cells", string_list)?;
    let target = opt_string(o, "target")?.unwrap_or_default();

    let mut out = Out::new();
    out.put("name", name);
    out.put("kind", kind);
    out.text("heading", &heading);
    out.text("body", &body);
    out.values("rows", rows);
    out.strings("columns", &columns);
    if !cells.is_empty() {
        out.put("cells", cells);
    }
    out.text("target", &target);
    Ok(out.done())
}

// MARK: - Cards' parts

/// A field deliberately not there (`AbsentField`).
pub fn absent_field(v: &Value) -> Read<Value> {
    let o = object(v)?;
    let mut out = Out::new();
    out.put("name", req_string(o, "name")?);
    out.put("reason", req_string(o, "reason")?);
    Ok(out.done())
}

/// A named bundle of tags (`TagGroup`).
pub fn tag_group(v: &Value) -> Read<Value> {
    let o = object(v)?;
    let name = req_string(o, "name")?;
    let tags = string_list(get(o, "tags").ok_or(())?)?;
    let exclusive = opt_bool(o, "exclusive")?.unwrap_or(false);
    let mut out = Out::new();
    out.put("name", name);
    out.put("tags", tags);
    out.put("exclusive", exclusive);
    Ok(out.done())
}

const CARDINALITIES: [&str; 4] = ["1:1", "1:N", "N:1", "N:N"];

/// A pointer as the store keeps it, by the target's id (`StoredPointsAt`).
pub struct StoredPointsAt {
    pub schema_id: Option<String>,
    pub designation: String,
    pub exists_in: Option<String>,
    pub when: Option<Value>,
    pub discriminated_by: Option<String>,
    pub discriminator_field: Option<String>,
    pub cardinality: Option<String>,
}

pub fn stored_points_at(v: &Value) -> Read<StoredPointsAt> {
    let o = object(v)?;
    let when = match get(o, "when") {
        None => None,
        Some(w) => {
            let w = object(w)?;
            let mut out = Out::new();
            out.put("field", req_string(w, "field")?);
            out.put("equals", req_string(w, "equals")?);
            Some(out.done())
        }
    };
    let cardinality = match opt_string(o, "cardinality")? {
        None => None,
        Some(c) if CARDINALITIES.contains(&c.as_str()) => Some(c),
        Some(_) => return Err(()),
    };
    Ok(StoredPointsAt {
        schema_id: opt_string(o, "schemaId")?,
        designation: req_string(o, "as")?,
        exists_in: opt_string(o, "in")?,
        when,
        discriminated_by: opt_string(o, "discriminatedBy")?,
        discriminator_field: opt_string(o, "discriminatorField")?,
        cardinality,
    })
}

/// A pointer as the project reads it, its target named (`PointsAt`).
pub fn points_at(stored: &StoredPointsAt, schema: Option<String>) -> Value {
    let mut out = Out::new();
    out.some("schema", &schema);
    out.put("as", stored.designation.clone());
    out.some("in", &stored.exists_in);
    if let Some(w) = &stored.when {
        out.put("when", w.clone());
    }
    out.some("discriminatedBy", &stored.discriminated_by);
    out.some("discriminatorField", &stored.discriminator_field);
    out.some("cardinality", &stored.cardinality);
    out.done()
}

/// Rules as the store keeps them (`Rule.list(fromJSON:)`): each object with a string
/// `kind`, its other keys in the order written. Anything else is left out.
pub fn rules(v: Option<&Value>) -> Vec<Value> {
    let Some(Value::Array(items)) = v else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| {
            let o = item.as_object()?;
            let kind = o.get("kind")?.as_str()?;
            let mut out = Out::new();
            out.put("kind", kind);
            for (key, value) in o {
                if key != "kind" {
                    out.put(key, value.clone());
                }
            }
            Some(out.done())
        })
        .collect()
}

/// `[String: String]`: every value a string, or none of it.
pub fn string_map(v: Option<&Value>) -> Obj {
    let Some(Value::Object(o)) = v else {
        return Map::new();
    };
    if o.values().all(Value::is_string) {
        o.clone()
    } else {
        Map::new()
    }
}

/// `[String]`, or none of it.
pub fn strings_or_empty(v: Option<&Value>) -> Vec<String> {
    v.and_then(|v| string_list(v).ok()).unwrap_or_default()
}

/// A list of one type read as SP reads a JSON column: all of it, or empty.
pub fn list_or_empty(v: Option<&Value>, each: impl Fn(&Value) -> Read<Value>) -> Vec<Value> {
    v.and_then(|v| list(v, each).ok()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_stored_node_is_read_by_the_names_of_now() {
        let stored = json!({"name": "a", "kind": "process", "note": "前の名前", "next": ["b"],
                            "steps": [{"do": "read", "paths": ["X:Y"], "comment": "c"}], "targets": [],
                            "topLeft": {"x": 2_000_000, "y": 3}, "extra": 1});
        assert_eq!(
            flow_node(&stored).unwrap(),
            json!({"name": "a", "kind": "process", "description": "前の名前",
                   "steps": [{"do": "read", "paths": ["X:Y"], "description": "c"}],
                   "next": "b", "topLeft": {"x": 1_000_000, "y": 3}})
        );
        assert!(
            flow_node(&json!({"kind": "end"})).is_err(),
            "名前が無ければ読めない"
        );
    }

    #[test]
    fn a_list_with_one_it_cannot_read_is_empty() {
        let nodes = json!([{"name": "a", "kind": "end"}, {"name": 3, "kind": "end"}]);
        assert!(list_or_empty(Some(&nodes), flow_node).is_empty());
    }

    #[test]
    fn rules_keep_their_order_and_drop_what_has_no_kind() {
        let stored = json!([{"when": "w", "kind": "requiredIf", "then": "t"}, {"note": "x"}]);
        let read = rules(Some(&stored));
        assert_eq!(read.len(), 1);
        let keys: Vec<&String> = read[0].as_object().unwrap().keys().collect();
        assert_eq!(keys, ["kind", "when", "then"]);
    }
}
