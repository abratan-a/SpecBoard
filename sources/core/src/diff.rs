//! What changed in a card between two of its documents, said a line a change
//! (`CardSnapshot.changes` in the Swift app), read from the kinds' form: its name, kind,
//! label and description; the section it sits in, when asked; a record's fields by their
//! ids — made, gone, renamed, or changed in what they are; an enum's values; anything else
//! as one line. Where it sits on a diagram is how it is shown, not what it says, and is not
//! compared.

use serde_json::{Map, Value};

/// A field as compared: where it is, what it is, and what is said about it.
struct FieldShape {
    path: String,
    shape: String,
    detail: String,
}

fn text<'a>(fields: &'a Map<String, Value>, key: &str) -> &'a str {
    fields.get(key).and_then(Value::as_str).unwrap_or("")
}

fn flag(fields: &Map<String, Value>, key: &str) -> bool {
    fields.get(key).and_then(Value::as_bool).unwrap_or(false)
}

/// A JSON value with its objects' keys sorted, compact.
fn encoded(v: &Value) -> String {
    crate::ojson::compact(&crate::export::sorted(v))
}

/// Every field of a record, outermost first, by its id.
fn fields(rows: Option<&Value>, prefix: &str, out: &mut Vec<(String, FieldShape)>) {
    for row in rows.and_then(Value::as_array).into_iter().flatten() {
        let id = row["id"].as_str().unwrap_or("").to_string();
        let f = row["fields"].as_object().cloned().unwrap_or_default();
        let name = text(&f, "name");
        let path = if prefix.is_empty() {
            crate::pathtext::quote(name).into_owned()
        } else {
            crate::pathtext::member(prefix, name)
        };
        let kind = f.get("type").and_then(Value::as_str).unwrap_or("string");
        let flags: Vec<&str> = [
            ("required", "required"),
            ("nullable", "nullable"),
            ("array", "array"),
            ("unique", "unique"),
            ("primaryKey", "pk"),
        ]
        .iter()
        .filter(|(k, _)| flag(&f, k))
        .map(|(_, w)| *w)
        .collect();
        let fixed = f
            .get("const")
            .filter(|v| !v.is_null())
            .map(|v| format!("={}", encoded(v)))
            .unwrap_or_default();
        let shape = format!("{kind} {}{fixed}", flags.join(" "));
        let detail = [
            text(&f, "description").to_string(),
            encoded(f.get("rules").unwrap_or(&Value::Array(Vec::new()))),
            encoded(f.get("variants").unwrap_or(&Value::Array(Vec::new()))),
            encoded(f.get("also").unwrap_or(&Value::Array(Vec::new()))),
        ]
        .join("\u{2}");
        out.push((
            id,
            FieldShape {
                path: path.clone(),
                shape,
                detail,
            },
        ));
        fields(row["rows"].get("fields"), &path, out);
    }
}

/// An enum's values, in order.
fn values(doc: &Value) -> Vec<String> {
    doc["rows"]["values"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|r| r["fields"]["value"].as_str().map(str::to_string))
        .collect()
}

/// What is left once the compared parts are taken out: the rest of what it says.
fn rest(doc: &Value) -> Value {
    let mut fields = doc["fields"].as_object().cloned().unwrap_or_default();
    for key in ["name", "label", "description"] {
        fields.remove(key);
    }
    // A card ghost's target is said apart.
    if doc["kind"] == crate::detail::GHOST {
        fields.remove("target");
    }
    let mut rows = doc["rows"].as_object().cloned().unwrap_or_default();
    rows.remove("fields");
    rows.remove("values");
    let mut out = Map::new();
    out.insert("fields".into(), Value::Object(fields));
    out.insert("rows".into(), Value::Object(rows));
    Value::Object(out)
}

/// A value as a change says it: an empty one as （空）, not as nothing between the spaces.
fn shown(value: &str) -> String {
    if value.trim().is_empty() {
        text!("diff.part.empty")
    } else {
        value.to_string()
    }
}

/// What changed from `before` to `after`, a line a change. `path_of` names a card by
/// its id, in full; `section_of`, when given, a section by its id, and a card moved to
/// another section says so (a reader that says a card by its path says the move already).
pub fn changes(
    before: &Value,
    after: &Value,
    path_of: &dyn Fn(&str) -> Option<String>,
    section_of: Option<&dyn Fn(&str) -> String>,
) -> Vec<String> {
    let mut out = Vec::new();
    let name = |d: &Value| d["name"].as_str().unwrap_or("").to_string();
    if name(before) != name(after) {
        out.push(text!(
            "diff.change.name",
            from = shown(&name(before)),
            to = shown(&name(after))
        ));
    }
    let kind = |d: &Value| d["kind"].as_str().unwrap_or("").to_string();
    if kind(before) != kind(after) {
        out.push(text!(
            "diff.change.kind",
            from = shown(&kind(before)),
            to = shown(&kind(after))
        ));
    }
    // Its label too: what a merge takes in, a branch says it changed.
    let label = |d: &Value| d["fields"]["label"].as_str().unwrap_or("").to_string();
    if label(before) != label(after) {
        out.push(text!(
            "diff.change.label",
            from = shown(&label(before)),
            to = shown(&label(after))
        ));
    }
    let description = |d: &Value| {
        d["fields"]["description"]
            .as_str()
            .unwrap_or("")
            .to_string()
    };
    if description(before) != description(after) {
        out.push(text!("diff.change.description"));
    }
    // A card ghost: what it points at.
    let target = |d: &Value| {
        (d["kind"] == crate::detail::GHOST)
            .then(|| d["fields"]["target"].as_str().map(str::to_string))
            .flatten()
    };
    if let (Some(a), Some(b)) = (target(before), target(after))
        && a != b
    {
        let said = |id: &str| path_of(id).unwrap_or_else(|| id.to_string());
        out.push(text!("diff.change.target", from = said(&a), to = said(&b)));
    }
    // The container it is inside.
    let inside = |d: &Value| d["in"].as_str().map(str::to_string);
    // The section it sits in: one inside a container sits where the container does, and goes
    // with it.
    let section = |d: &Value| d["section"].as_str().unwrap_or("").to_string();
    if let Some(section_of) = section_of
        && inside(after).is_none()
        && section(before) != section(after)
    {
        out.push(text!(
            "diff.change.section",
            from = section_of(&section(before)),
            to = section_of(&section(after))
        ));
    }
    if inside(before) != inside(after) {
        let said = |id: &Option<String>| match id {
            Some(id) => path_of(id).unwrap_or_else(|| id.clone()),
            None => text!("diff.part.section"),
        };
        out.push(text!(
            "diff.change.container",
            from = said(&inside(before)),
            to = said(&inside(after))
        ));
    }
    let (mut was, mut now) = (Vec::new(), Vec::new());
    fields(before["rows"].get("fields"), "", &mut was);
    fields(after["rows"].get("fields"), "", &mut now);
    let had: std::collections::HashMap<&str, &FieldShape> =
        was.iter().map(|(id, f)| (id.as_str(), f)).collect();
    let has: std::collections::HashMap<&str, &FieldShape> =
        now.iter().map(|(id, f)| (id.as_str(), f)).collect();
    for (id, f) in &now {
        if !had.contains_key(id.as_str()) {
            out.push(text!("diff.change.fieldAdded", path = f.path.as_str()));
        }
    }
    for (id, f) in &was {
        if !has.contains_key(id.as_str()) {
            out.push(text!("diff.change.fieldRemoved", path = f.path.as_str()));
        }
    }
    for (id, f) in &was {
        let Some(other) = has.get(id.as_str()) else {
            continue;
        };
        if other.path != f.path {
            out.push(text!(
                "diff.change.fieldRenamed",
                from = f.path.as_str(),
                to = other.path.as_str()
            ));
        }
        if other.shape != f.shape {
            out.push(text!(
                "diff.change.fieldChanged",
                path = other.path.as_str(),
                from = f.shape.trim(),
                to = other.shape.trim()
            ));
        } else if other.detail != f.detail {
            out.push(text!(
                "diff.change.fieldDescribed",
                path = other.path.as_str()
            ));
        }
    }
    let (then, values_now) = (values(before), values(after));
    let added: Vec<&str> = values_now
        .iter()
        .filter(|v| !then.contains(v))
        .map(String::as_str)
        .collect();
    let removed: Vec<&str> = then
        .iter()
        .filter(|v| !values_now.contains(v))
        .map(String::as_str)
        .collect();
    if !added.is_empty() {
        out.push(text!("diff.change.valuesAdded", values = added.join(", ")));
    }
    if !removed.is_empty() {
        out.push(text!(
            "diff.change.valuesRemoved",
            values = removed.join(", ")
        ));
    }
    // Its lines, by id.
    let lines = |d: &Value| -> Vec<(String, String, String)> {
        d["connections"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|c| {
                let to = c["to"]
                    .as_str()
                    .map(|t| path_of(t).unwrap_or_else(|| t.to_string()))
                    .unwrap_or_default();
                let at = c["at"]
                    .as_str()
                    .map(|at| text!("diff.part.lineAt", at))
                    .unwrap_or_default();
                let from = c["from"]
                    .as_str()
                    .map(|f| format!("{f} "))
                    .unwrap_or_default();
                let said = format!("{} {from}→ {to}{at}", c["line"].as_str().unwrap_or(""));
                let detail = format!("{}\u{2}{}", encoded(&c["fields"]), c["description"]);
                (c["id"].as_str().unwrap_or("").to_string(), said, detail)
            })
            .collect()
    };
    let (had_lines, has_lines) = (lines(before), lines(after));
    for (id, said, _) in &has_lines {
        if !had_lines.iter().any(|(i, _, _)| i == id) {
            out.push(text!("diff.change.lineAdded", said));
        }
    }
    for (id, said, _) in &had_lines {
        if !has_lines.iter().any(|(i, _, _)| i == id) {
            out.push(text!("diff.change.lineRemoved", said));
        }
    }
    for (id, said, detail) in &had_lines {
        if let Some((_, now, now_detail)) = has_lines.iter().find(|(i, _, _)| i == id)
            && (now != said || now_detail != detail)
        {
            out.push(text!("diff.change.lineChanged", now = now.as_str()));
        }
    }
    // The rest of its rows — a document's, a step's, a kind of the project's own — a row at a
    // time by where it is, not only said that rows changed.
    let (rows_then, rows_now) = (other_rows(before), other_rows(after));
    for (id, (path, _, _)) in &rows_now {
        if !rows_then.contains_key(id) {
            out.push(text!("diff.change.rowAdded", path = path.as_str()));
        }
    }
    for (id, (path, _, _)) in &rows_then {
        if !rows_now.contains_key(id) {
            out.push(text!("diff.change.rowRemoved", path = path.as_str()));
        }
    }
    let mut reordered = false;
    for (id, (path, said, at)) in &rows_then {
        let Some((now_path, now_said, now_at)) = rows_now.get(id) else {
            continue;
        };
        if now_path != path {
            out.push(text!(
                "diff.change.rowRenamed",
                from = path.as_str(),
                to = now_path.as_str()
            ));
        }
        if now_said != said {
            out.push(text!("diff.change.rowChanged", path = now_path.as_str()));
        }
        // Moved to another list.
        reordered |= now_at.0 != at.0;
    }
    // Among those beside it, the rows both have in another order: a row added or let go of
    // moves the others' places, not their order (a stackdoc's version goes on top).
    let order =
        |rows: &std::collections::BTreeMap<String, (String, String, (String, usize))>,
         other: &std::collections::BTreeMap<String, (String, String, (String, usize))>| {
            let mut kept: Vec<(&(String, usize), &String)> = rows
                .iter()
                .filter(|(id, (_, _, at))| other.get(*id).is_some_and(|o| o.2.0 == at.0))
                .map(|(id, (_, _, at))| (at, id))
                .collect();
            kept.sort();
            kept.into_iter()
                .map(|((list, _), id)| (list.clone(), id.clone()))
                .collect::<Vec<_>>()
        };
    reordered |= order(&rows_then, &rows_now) != order(&rows_now, &rows_then);
    if reordered {
        out.push(text!("diff.change.rowsReordered"));
    }
    // Its other fields, by name; where it is written out.
    let (kept, keeps) = (rest(before), rest(after));
    let empty = Map::new();
    let then = kept["fields"].as_object().unwrap_or(&empty);
    let now = keeps["fields"].as_object().unwrap_or(&empty);
    let mut keys: Vec<&str> = then.keys().chain(now.keys()).map(String::as_str).collect();
    keys.sort();
    keys.dedup();
    keys.retain(|k| then.get(*k) != now.get(*k));
    if !keys.is_empty() {
        out.push(text!(
            "diff.change.fieldsChanged",
            keys = crate::text::list(&keys)
        ));
    }
    out
}

/// The rows a card holds but its fields and values (said apart), by id: where each is
/// (the names from the top, `intro.goal`), what it says itself, and where it sits among
/// those beside it.
fn other_rows(
    doc: &Value,
) -> std::collections::BTreeMap<String, (String, String, (String, usize))> {
    fn walk(
        rows: &Map<String, Value>,
        parent: &str,
        top: bool,
        out: &mut std::collections::BTreeMap<String, (String, String, (String, usize))>,
    ) {
        for (kind, list) in rows {
            if top && (kind == "fields" || kind == "values") {
                continue;
            }
            for (i, row) in list.as_array().into_iter().flatten().enumerate() {
                let id = row["id"].as_str().unwrap_or_default().to_string();
                let name = row["fields"]["name"]
                    .as_str()
                    .map(|n| crate::pathtext::quote(n).into_owned())
                    .unwrap_or_else(|| format!("{kind}[{i}]"));
                let path = if parent.is_empty() {
                    name
                } else {
                    format!("{parent}.{name}")
                };
                out.insert(
                    id.clone(),
                    (
                        path.clone(),
                        encoded(&row["fields"]),
                        (format!("{parent}\u{1}{kind}"), i),
                    ),
                );
                if let Some(inner) = row["rows"].as_object() {
                    walk(inner, &path, false, out);
                }
            }
        }
    }
    let mut out = std::collections::BTreeMap::new();
    if let Some(rows) = doc["rows"].as_object() {
        walk(rows, "", true, &mut out);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn field(id: &str, name: &str, kind: &str) -> Value {
        json!({ "id": id, "fields": { "name": name, "type": kind } })
    }

    #[test]
    fn an_empty_value_is_said_as_empty() {
        let before = json!({ "name": "Port", "kind": "entity", "fields": {} });
        let after = json!({ "name": "Port", "kind": "entity", "fields": { "label": "ポート" } });
        assert_eq!(
            changes(&before, &after, &|_| None, None),
            ["表示名 （空） → ポート"]
        );
        assert_eq!(
            changes(&after, &before, &|_| None, None),
            ["表示名 ポート → （空）"]
        );
    }

    #[test]
    fn a_card_moved_to_another_section_says_where_from_and_to() {
        let at = |section: &str| json!({ "name": "Invoice", "kind": "entity", "section": section });
        let named = |id: &str| format!("Main/{id}");
        assert_eq!(
            changes(&at("Data"), &at("Billing"), &|_| None, Some(&named)),
            ["セクション Main/Data → Main/Billing"]
        );
        // A reader that says the card by its path says the move already.
        assert!(changes(&at("Data"), &at("Billing"), &|_| None, None).is_empty());
        // One inside a container goes where its container goes.
        let inside = |section: &str| json!({ "name": "begin", "kind": "begin", "in": "flow", "section": section });
        assert!(changes(&inside("Data"), &inside("Billing"), &|_| None, Some(&named)).is_empty());
    }

    #[test]
    fn a_card_says_what_changed_in_it() {
        let before = json!({ "name": "Order", "kind": "entity", "fields": { "label": "注文" },
            "rows": { "fields": [field("a", "id", "uuid"), field("b", "total", "decimal")] } });
        let after = json!({ "name": "Order", "kind": "entity", "fields": { "label": "受注", "description": "x" },
            "rows": { "fields": [field("a", "id", "uuid"),
                { "id": "b", "fields": { "name": "sum", "type": "int", "required": true } },
                field("c", "refund_total", "decimal")] } });
        assert_eq!(
            changes(&before, &after, &|_| None, None),
            [
                "表示名 注文 → 受注",
                "説明を変える",
                "足す refund_total",
                "改名 total → sum",
                "変える sum（decimal → int required）"
            ]
        );
        let status = |values: &[&str]| {
            json!({ "name": "Status", "kind": "enum", "rows": { "values":
                values.iter().map(|v| json!({ "id": v, "fields": { "value": v } })).collect::<Vec<_>>() } })
        };
        assert_eq!(
            changes(
                &status(&["open", "shipped"]),
                &status(&["open", "refunded"]),
                &|_| None,
                None
            ),
            ["値を足す refunded", "値を消す shipped"]
        );
    }
}
