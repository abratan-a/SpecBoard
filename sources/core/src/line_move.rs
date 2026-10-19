//! The lines of before moved into a card's connections (the store's
//! mark `core_0005_connections`, a project file before version 12).
//!
//! A card's document kept its lines in its fields: a field's `type: ref` and `ref` (what it
//! embeds), its `pointsAt` (what its value names), a shape's `payloads` (what a signature
//! covers and the keys that make and check it), a store's `holds`, a flow's `notes`, a
//! task's `links`. Now each line is one of the card's `connections`, of a kind of line the
//! project defines (`specboard://SpecBoard/Docs/Design:CardKinds`). Each tree is moved as it sees its cards:
//!
//! - `ref` → the built-in `embed`; the field's type goes back to `string` (the line says it).
//! - `pointsAt` designating by `id` or `lookup` → the built-in `points`; by any other word
//!   (`digest`, `payload`, …) → a line of the project's own named for the word, made once for
//!   the project (base `designate`, written out as `x-pointsAt` as before). `in` is where it
//!   lands (`at`) inside the card it names, its names there. `when`, `cardinality`,
//!   `discriminatedBy` and `discriminatorField` are the line's fields; a pointer told apart
//!   by its discriminator names no card (`to`), as before.
//! - A pointer whose `in` is in another card than the one it names is two lines (a
//!   designating line lands inside the card it reaches): the pointer to the
//!   card it names, with no landing, and a `link` from the same field to where its values
//!   are (`to` that card, `at` the names there), said 「使ってよい値の一覧（移し替え）」. A store
//!   moved before that has its lines split so once more, when it is opened (the mark
//!   `core_0007_landings`, `split_all`), and so has a project file read in (`split_project`).
//! - A payload with keys → one line a key, of the project's own `signedBy` (署名する) and
//!   `verifiedBy` (確かめる), from the card's head to the key; the payload's name, what it
//!   leaves out and where its value is are the line's fields. One with no key cannot be a
//!   line: it is written at the end of the card's description, and said in the record.
//! - `holds` → the built-in `holds`, `notes` → `notes`, `links` → `link`.
//! - A path that leads nowhere cannot be a line either: written at the end of the
//!   description, and said.
//!
//! A connection's id is made from what it is (`ids::made_id`), so main and a branch, and
//! a document and its base, name the same line alike. A branch's prints of main's cards
//! moved, a base, the history's rows and an open request's print are moved or taken again,
//! as `ghost_move` does. Each tree moved has a commit of one record (no rows).

use crate::directory::{CardRef, Directory, Entry, GhostRef, SectionRef};
use crate::store::BOARD;
use rusqlite::{Connection, params};
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, HashMap, HashSet};

type Obj = Map<String, Value>;

fn sql(e: rusqlite::Error) -> String {
    text!("lineMove.error.sql", e)
}

/// A row of `card_docs` or `section_docs` of one tree.
#[derive(Clone)]
struct Row {
    id: String,
    removed: bool,
    doc: Option<Obj>,
    base: Option<String>,
}

fn rows(conn: &Connection, table: &str, project: &str, tree: &str) -> Result<Vec<Row>, String> {
    let mut stmt = conn
        .prepare(&format!(
            "SELECT id, removed, doc, base FROM {table} WHERE project_id = ? AND branch_id = ?"
        ))
        .map_err(sql)?;
    stmt.query_map(params![project, tree], |r| {
        Ok(Row {
            id: r.get(0)?,
            removed: r.get::<_, i64>(1)? != 0,
            doc: crate::store::parse(r.get::<_, Option<String>>(2)?.as_deref()),
            base: r.get(3)?,
        })
    })
    .map_err(sql)?
    .collect::<Result<_, _>>()
    .map_err(sql)
}

/// What a tree sees, to read a path written in it: its sections, cards and card ghosts.
struct View {
    directory: Directory,
    /// The cards' ids, as the directory numbers them.
    card_ids: Vec<String>,
}

fn text(doc: &Obj, key: &str) -> String {
    doc.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn view(main: (&[Row], &[Row]), own: Option<(&[Row], &[Row])>) -> View {
    let mut sections: BTreeMap<String, Obj> = BTreeMap::new();
    let mut cards: BTreeMap<String, Obj> = BTreeMap::new();
    for (section_rows, card_rows) in std::iter::once(main).chain(own) {
        for row in section_rows {
            if row.removed {
                sections.remove(&row.id);
            } else if let Some(doc) = &row.doc {
                sections.insert(row.id.clone(), doc.clone());
            }
        }
        for row in card_rows {
            if row.removed {
                cards.remove(&row.id);
            } else if let Some(doc) = &row.doc {
                cards.insert(row.id.clone(), doc.clone());
            }
        }
    }
    let section_refs: Vec<SectionRef> = sections
        .iter()
        .map(|(id, doc)| SectionRef {
            id: id.clone(),
            name: text(doc, "name"),
            parent_id: doc
                .get("parent_id")
                .and_then(Value::as_str)
                .map(str::to_string),
        })
        .collect();
    let mut card_refs = Vec::new();
    let mut ghost_refs = Vec::new();
    for (id, doc) in &cards {
        let section = text(doc, "section");
        if text(doc, "kind") == crate::detail::GHOST {
            ghost_refs.push(GhostRef {
                id: id.clone(),
                name: text(doc, "name"),
                section_id: Some(section),
                target: doc
                    .get("fields")
                    .and_then(|f| f.get("target"))
                    .and_then(Value::as_str)
                    .map(str::to_string),
            });
        } else {
            card_refs.push(CardRef {
                id: id.clone(),
                name: text(doc, "name"),
                section_id: Some(section),
                inside: doc.get("in").and_then(Value::as_str).map(str::to_string),
            });
        }
    }
    let card_ids = card_refs.iter().map(|c| c.id.clone()).collect();
    View {
        directory: Directory::new(&section_refs, &card_refs, &ghost_refs),
        card_ids,
    }
}

impl View {
    /// What a path written in a card of `base` names: the card's or ghost's id, as written,
    /// and the names after it (`[]` kept); none when it names no card.
    fn resolve(&self, raw: &str, base: &str) -> Option<(String, String)> {
        let raw = raw.trim();
        let (head, rest) = match raw.find(':') {
            Some(colon) => {
                let after = &raw[colon + 1..];
                match after.find('.') {
                    Some(dot) => (
                        format!("{}{}", &raw[..=colon], &after[..dot]),
                        after[dot + 1..].to_string(),
                    ),
                    None => (raw.to_string(), String::new()),
                }
            }
            None => match raw.find('.') {
                Some(dot) => (raw[..dot].to_string(), raw[dot + 1..].to_string()),
                None => (raw.to_string(), String::new()),
            },
        };
        let head = head.trim_end_matches("[]");
        let id = self.entry_id(self.directory.find_entry(head, Some(base))?)?;
        Some((id, rest))
    }

    fn entry_id(&self, entry: Entry) -> Option<String> {
        match entry {
            Entry::Card(i) => self.card_ids.get(i).cloned(),
            Entry::Ghost(g) => self.directory.ghost_id(g),
        }
    }

    /// The card a path written in full in a card of `base` starts in — a card inside a
    /// container through its container's names — by its id as written (a card ghost's own),
    /// the id of the card it leads to, and the names after it (`[]` kept).
    fn landing(&self, raw: &str, base: &str) -> Option<(String, String, String)> {
        let (entry, rest) = self.directory.split_entry(raw, Some(base))?;
        let written = self.entry_id(entry)?;
        let real = self.card_ids.get(self.directory.follow(entry)?)?.clone();
        Some((written, real, rest))
    }

    /// The card an id leads to: itself, or a card ghost's card.
    fn real(&self, id: &str) -> Option<String> {
        let entry = self.directory.entry_of(id)?;
        self.card_ids.get(self.directory.follow(entry)?).cloned()
    }
}

// MARK: - The project's own lines the move makes

/// The line a pointer's word moves into: the built-in `points`, or the project's own one named
/// for the word (made once for the project).
fn word_line(word: &str, names: &HashMap<String, String>) -> String {
    match word {
        "id" | "lookup" | "" => "points".to_string(),
        other => names
            .get(other)
            .cloned()
            .unwrap_or_else(|| other.to_string()),
    }
}

/// A name a word may take as a line's: itself when it can be one and is not taken.
fn name_for(word: &str, taken: &dyn Fn(&str) -> bool) -> String {
    let cleaned: String = word
        .trim()
        .chars()
        .map(|c| if ":./,\"\n\r\t".contains(c) { '_' } else { c })
        .take(36)
        .collect();
    let cleaned = if cleaned.is_empty() {
        "designate".to_string()
    } else {
        cleaned
    };
    if !taken(&cleaned) {
        return cleaned;
    }
    let mut n = 2;
    while taken(&format!("{cleaned}_{n}")) {
        n += 1;
    }
    format!("{cleaned}_{n}")
}

/// The fields of a line that designates as `points` does.
fn pointer_fields() -> Value {
    json!([
        { "name": "cardinality", "label": "数", "fieldtype": "dropdownlist", "options": ["1:1", "1:N", "N:1", "N:N"] },
        { "name": "when", "label": "この相手になる条件", "fieldtype": "group",
          "fields": [
            { "name": "field", "fieldtype": "text", "required": true, "label": "判別子のフィールド" },
            { "name": "equals", "fieldtype": "text", "required": true, "label": "選ばれる値" }
          ] },
        { "name": "discriminatedBy", "fieldtype": "text", "label": "相手の種類名が入っている欄" },
        { "name": "discriminatorField", "fieldtype": "text", "label": "相手の側で const を持つ欄" }
    ])
}

/// A line of the project's own for a pointer's word.
fn word_definition(word: &str, name: &str) -> Value {
    let mut export = json!({ "as": "annotation", "key": "x-pointsAt" });
    if name != word {
        export["word"] = word.into();
    }
    let color = if word == "digest" { "blue" } else { "violet" };
    json!({
        "line": name,
        "label": word,
        "description": format!("値が相手を {word} で指す（型は変わらない）。移し替えで pointsAt の as: {word} から作った"),
        "base": "designate",
        "look": { "color": color, "dash": "solid", "begin": "none", "end": "arrow" },
        "from": [{ "kinds": ["entity", "value"], "at": "row" }],
        "to": { "kinds": ["*"], "at": ["card", "row"] },
        "max": null,
        "fields": pointer_fields(),
        "export": export
    })
}

/// The lines of the project's own a signature's keys move into: making it (`signedBy`) and
/// checking it (`verifiedBy`).
fn key_definition(name: &str) -> Value {
    let (label, about, color) = if name == "signedBy" {
        ("署名する", "署名を作る鍵（秘密鍵かそのハンドル）", "red")
    } else {
        ("確かめる", "署名を確かめる鍵（公開鍵）", "teal")
    };
    json!({
        "line": name,
        "label": label,
        "description": format!("{about}。payload（覆いの名前）・excludes（覆わないパス）・valueAt（値の置き場所）を欄に持つ。移し替えで payloads から作った"),
        "base": "link",
        "look": { "color": color, "dash": "dashed", "begin": "none", "end": "arrow" },
        "from": [{ "kinds": ["entity", "value", "enum"], "at": "card" }],
        "to": { "kinds": ["*"], "at": ["card", "row"] },
        "max": null,
        "fields": [
            { "name": "payload", "label": "覆いの名前", "fieldtype": "text", "required": true },
            { "name": "excludes", "label": "覆わないパス", "fieldtype": "path", "many": true },
            { "name": "valueAt", "label": "値の置き場所", "fieldtype": "path" }
        ],
        "export": { "as": "annotation", "key": "x-payloads" }
    })
}

// MARK: - One document

/// What moving a document did: how many lines of each kind, what could not be a line, and
/// the card's name once for each line split in two (`split_landings`).
#[derive(Default)]
struct Moved {
    counts: BTreeMap<String, usize>,
    kept: Vec<crate::text::Keyed>,
    split: Vec<String>,
    /// The names of the cards a pass of another kind moved (`move_card_docs`).
    cards: Vec<String>,
}

impl Moved {
    fn add(&mut self, other: Moved) {
        for (line, n) in other.counts {
            *self.counts.entry(line).or_default() += n;
        }
        self.kept.extend(other.kept);
        self.split.extend(other.split);
        self.cards.extend(other.cards);
    }
}

/// What a move reads a document with.
struct Ctx<'a> {
    view: &'a View,
    /// The project's own line for each pointer's word.
    words: &'a HashMap<String, String>,
    /// The kinds of line that designate (base `designate`): the built-in `points`, and the
    /// project's own.
    designating: &'a HashSet<String>,
}

/// A connection made from what it is, so the same line is made alike wherever it is moved.
#[allow(clippy::too_many_arguments)]
fn connection(
    card: &str,
    line: &str,
    from: &str,
    to: Option<&str>,
    at: &str,
    fields: Obj,
    seen: &mut HashMap<String, usize>,
) -> Value {
    let key = format!("{line}\u{1}{from}\u{1}{}\u{1}{at}", to.unwrap_or_default());
    let n = seen.entry(key).or_insert(0);
    *n += 1;
    let id = crate::ids::made_id(&[
        "connection",
        card,
        line,
        from,
        to.unwrap_or_default(),
        at,
        &n.to_string(),
    ]);
    let mut out = Map::new();
    out.insert("id".into(), id.into());
    out.insert("line".into(), line.into());
    if !from.is_empty() {
        out.insert("from".into(), from.into());
    }
    if let Some(to) = to {
        out.insert("to".into(), to.into());
    }
    if !at.is_empty() {
        out.insert("at".into(), at.into());
    }
    if !fields.is_empty() {
        out.insert("fields".into(), Value::Object(fields));
    }
    Value::Object(out)
}

/// Whether a document holds a line of before.
fn has_lines(doc: &Obj) -> bool {
    let fields = doc.get("fields").and_then(Value::as_object);
    let field_has = |key: &str| fields.is_some_and(|f| f.contains_key(key));
    if ["payloads", "holds", "notes", "links"]
        .iter()
        .any(|k| field_has(k))
    {
        return true;
    }
    fn rows_have(list: Option<&Value>) -> bool {
        list.and_then(Value::as_array)
            .into_iter()
            .flatten()
            .any(|row| {
                let f = row.get("fields").and_then(Value::as_object);
                f.is_some_and(|f| {
                    f.contains_key("ref")
                        || f.contains_key("pointsAt")
                        || f.get("type").and_then(Value::as_str) == Some("ref")
                }) || rows_have(row.get("rows").and_then(|r| r.get("fields")))
            })
    }
    rows_have(doc.get("rows").and_then(|r| r.get("fields")))
}

/// The words of a document's pointers, and whether it has a signature's keys: what the project
/// needs of its own lines.
fn needs(doc: &Obj, words: &mut Vec<String>, keys: &mut Vec<String>) {
    fn walk(list: Option<&Value>, words: &mut Vec<String>) {
        for row in list.and_then(Value::as_array).into_iter().flatten() {
            for p in row
                .get("fields")
                .and_then(|f| f.get("pointsAt"))
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                let word = p.get("as").and_then(Value::as_str).unwrap_or_default();
                if !["id", "lookup", ""].contains(&word) && !words.iter().any(|w| w == word) {
                    words.push(word.to_string());
                }
            }
            walk(row.get("rows").and_then(|r| r.get("fields")), words);
        }
    }
    walk(doc.get("rows").and_then(|r| r.get("fields")), words);
    for p in doc
        .get("fields")
        .and_then(|f| f.get("payloads"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        for key in ["signedBy", "verifiedBy"] {
            let has = match p.get(key) {
                Some(Value::Array(a)) => !a.is_empty(),
                Some(Value::String(s)) => !s.is_empty(),
                _ => false,
            };
            if has && !keys.iter().any(|k| k == key) {
                keys.push(key.to_string());
            }
        }
    }
}

/// Strings of a value: one, or a list of them.
fn strings(value: Option<&Value>) -> Vec<String> {
    match value {
        Some(Value::String(s)) => vec![s.clone()],
        Some(Value::Array(a)) => a
            .iter()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect(),
        _ => Vec::new(),
    }
}

/// A document moved, or none when it held no line of before.
fn move_doc(doc: &Obj, ctx: &Ctx) -> Option<(Obj, Moved)> {
    if !has_lines(doc) {
        return None;
    }
    let mut doc = doc.clone();
    let card = text(&doc, "id");
    let base = text(&doc, "section");
    let name = text(&doc, "name");
    let mut moved = Moved::default();
    let mut made: Vec<Value> = doc
        .get("connections")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut seen: HashMap<String, usize> = HashMap::new();
    fn count(moved: &mut Moved, line: &str) {
        *moved.counts.entry(line.to_string()).or_default() += 1;
    }

    // A record's fields: what each embeds and names.
    fn walk_fields(
        list: &mut [Value],
        trail: &mut Vec<String>,
        visit: &mut dyn FnMut(&mut Obj, &str),
    ) {
        for row in list.iter_mut() {
            let field_name = row
                .get("fields")
                .and_then(|f| f.get("name"))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            trail.push(field_name);
            let path = trail.join(".");
            if let Some(fields) = row.get_mut("fields").and_then(Value::as_object_mut) {
                visit(fields, &path);
            }
            if let Some(children) = row
                .get_mut("rows")
                .and_then(|r| r.get_mut("fields"))
                .and_then(Value::as_array_mut)
            {
                walk_fields(children, trail, visit);
            }
            trail.pop();
        }
    }
    if let Some(list) = doc
        .get_mut("rows")
        .and_then(|r| r.get_mut("fields"))
        .and_then(Value::as_array_mut)
    {
        walk_fields(list, &mut Vec::new(), &mut |fields, path| {
            let is_ref = fields.get("type").and_then(Value::as_str) == Some("ref");
            let target = fields
                .shift_remove("ref")
                .and_then(|v| v.as_str().map(str::to_string));
            if is_ref || target.is_some() {
                if is_ref {
                    fields.shift_remove("type");
                }
                match target.filter(|t| !t.is_empty()) {
                    Some(to) if is_ref => {
                        made.push(connection(
                            &card,
                            "embed",
                            path,
                            Some(&to),
                            "",
                            Map::new(),
                            &mut seen,
                        ));
                        count(&mut moved, "embed");
                    }
                    _ => moved
                        .kept
                        .push(keyed!("lineMove.line.refWithoutTarget", path)),
                }
            }
            let Some(Value::Array(pointers)) = fields.shift_remove("pointsAt") else {
                return;
            };
            for p in pointers {
                let Some(p) = p.as_object() else { continue };
                let word = p.get("as").and_then(Value::as_str).unwrap_or_default();
                let line = word_line(word, ctx.words);
                let to = p
                    .get("schema")
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string);
                let mut line_fields = Map::new();
                for key in [
                    "cardinality",
                    "when",
                    "discriminatedBy",
                    "discriminatorField",
                ] {
                    if let Some(v) = p.get(key).filter(|v| !v.is_null()) {
                        line_fields.insert(key.into(), v.clone());
                    }
                }
                if to.is_none() && !line_fields.contains_key("discriminatedBy") {
                    moved
                        .kept
                        .push(keyed!("lineMove.line.pointerWithoutTarget", path, word));
                    continue;
                }
                // Where it lands: inside the card it names, its names there. In another
                // card, where the values are is a line of its own. A
                // path that names no card is kept as it was written (`split_landings` takes
                // one in full off).
                let (at, elsewhere) = match p.get("in").and_then(Value::as_str) {
                    None => (String::new(), None),
                    Some(written) => match ctx.view.resolve(written, &base) {
                        Some((landed, rest))
                            if to.as_deref().and_then(|t| ctx.view.real(t))
                                == ctx.view.real(&landed) =>
                        {
                            (rest, None)
                        }
                        Some(there) => (String::new(), Some(there)),
                        None => (written.to_string(), None),
                    },
                };
                let pointer = connection(
                    &card,
                    &line,
                    path,
                    to.as_deref(),
                    &at,
                    line_fields,
                    &mut seen,
                );
                count(&mut moved, &line);
                let link = elsewhere
                    .map(|(there, names)| value_list(&card, &pointer, path, &there, &names));
                made.push(pointer);
                if let Some(link) = link {
                    made.push(link);
                    count(&mut moved, "link");
                    moved.split.push(name.clone());
                }
            }
        });
    }

    // What the card keeps in its own fields.
    let mut notes: Vec<String> = Vec::new();
    let fields = doc
        .entry("fields")
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .cloned()
        .unwrap_or_default();
    let mut fields = fields;
    let mut lines_to = |key: &str, line: &str, fields: &mut Obj, moved: &mut Moved| {
        let Some(list) = fields.shift_remove(key) else {
            return;
        };
        for path in strings(Some(&list)) {
            match ctx.view.resolve(&path, &base) {
                Some((to, at)) => {
                    made.push(connection(
                        &card,
                        line,
                        "",
                        Some(&to),
                        &at,
                        Map::new(),
                        &mut seen,
                    ));
                    *moved.counts.entry(line.to_string()).or_default() += 1;
                }
                None => {
                    notes.push(format!("{key}: {path}"));
                    moved
                        .kept
                        .push(keyed!("lineMove.line.pathWithoutPlace", key, path));
                }
            }
        }
    };
    lines_to("holds", "holds", &mut fields, &mut moved);
    lines_to("notes", "notes", &mut fields, &mut moved);
    lines_to("links", "link", &mut fields, &mut moved);
    if let Some(Value::Array(payloads)) = fields.shift_remove("payloads") {
        for p in payloads {
            let Some(p) = p.as_object() else { continue };
            let payload = p
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let excludes = strings(p.get("excludes"));
            let value_at = p.get("valueAt").and_then(Value::as_str).map(str::to_string);
            let mut line_fields = Map::new();
            line_fields.insert("payload".into(), payload.clone().into());
            if !excludes.is_empty() {
                line_fields.insert(
                    "excludes".into(),
                    Value::Array(excludes.iter().cloned().map(Value::from).collect()),
                );
            }
            if let Some(v) = &value_at {
                line_fields.insert("valueAt".into(), v.clone().into());
            }
            let mut keyed = 0;
            let mut lost: Vec<String> = Vec::new();
            for key in ["signedBy", "verifiedBy"] {
                for path in strings(p.get(key)) {
                    match ctx.view.resolve(&path, &base) {
                        Some((to, at)) => {
                            made.push(connection(
                                &card,
                                key,
                                "",
                                Some(&to),
                                &at,
                                line_fields.clone(),
                                &mut seen,
                            ));
                            count(&mut moved, key);
                            keyed += 1;
                        }
                        None => lost.push(format!("{key} {path}")),
                    }
                }
            }
            if keyed == 0 || !lost.is_empty() {
                let mut said = vec![format!("payloads の {payload}")];
                if !excludes.is_empty() {
                    said.push(format!("覆わないパス {}", excludes.join(", ")));
                }
                if let Some(v) = &value_at {
                    said.push(format!("値の置き場所 {v}"));
                }
                said.extend(lost.iter().cloned());
                notes.push(said.join(" / "));
                moved.kept.push(if keyed == 0 {
                    keyed!("lineMove.line.payloadWithoutKey", payload)
                } else {
                    keyed!(
                        "lineMove.line.payloadKeyWithoutPlace",
                        payload,
                        keys = crate::text::List::of(&lost)
                    )
                });
            }
        }
    }
    // A pointer landing in another card than the one it names: two lines.
    let (lines, split) = split_landings(made, &card, &base, ctx.view, ctx.designating);
    made = lines;
    if split.count > 0 {
        *moved.counts.entry("link".to_string()).or_default() += split.count;
    }
    moved
        .split
        .extend(std::iter::repeat_n(name.clone(), split.count));
    moved.kept.extend(split.kept);
    notes.extend(split.notes);
    note_down(&mut fields, MOVED_HERE, &notes);
    if fields.is_empty() {
        doc.shift_remove("fields");
    } else {
        doc.insert("fields".into(), Value::Object(fields));
    }
    if !made.is_empty() {
        doc.insert("connections".into(), Value::Array(made));
    }
    doc.insert("v".into(), crate::kinds::builtin().version.into());
    for line in moved.kept.iter_mut() {
        *line = keyed!("lineMove.line.inCard", name, line = line.clone());
    }
    Some((doc, moved))
}

/// What the end of a card's description says of the lines the move could not make.
const MOVED_HERE: &str = "（線の書き方の移し替え 2026-10-05 で線にできなかったもの）";

/// What it says of the landings the split could not make lines of.
const SPLIT_HERE: &str = "（線の着く所の移し替え 2026-10-06 で線にできなかったもの）";

/// What a line split from a pointer says: where the values it may take are.
const VALUE_LIST: &str = "使ってよい値の一覧（移し替え）";

/// What could not be a line written at the end of a card's description, under `header`.
fn note_down(fields: &mut Obj, header: &str, notes: &[String]) {
    if notes.is_empty() {
        return;
    }
    let mut description = fields
        .get("description")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    if !description.is_empty() {
        description.push_str("\n\n");
    }
    description.push_str(header);
    for note in notes {
        description.push_str("\n- ");
        description.push_str(note);
    }
    fields.insert("description".into(), description.into());
}

// MARK: - A landing elsewhere, split in two

/// What splitting a card's landings did.
#[derive(Default)]
struct Split {
    /// How many lines were split in two.
    count: usize,
    /// What could not be a line, said (the card's name before it is the caller's).
    kept: Vec<crate::text::Keyed>,
    /// What could not be a line, as the end of the description writes it.
    notes: Vec<String>,
}

/// The line a pointer's values are told by when they are in another card than the one it
/// names: a `link` from the same place (`from`) to that card (`there`, as
/// written: a card ghost by its own), landing at the names there, said
/// 「使ってよい値の一覧（移し替え）」. Its id is made from the pointer's, so main and a branch,
/// and a document and its base, make it alike.
fn value_list(card: &str, pointer: &Value, from: &str, there: &str, names: &str) -> Value {
    let id = pointer
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let mut link = Map::new();
    link.insert(
        "id".into(),
        crate::ids::made_id(&["landing", card, id]).into(),
    );
    link.insert("line".into(), "link".into());
    if !from.is_empty() {
        link.insert("from".into(), from.into());
    }
    link.insert("to".into(), there.into());
    if !names.is_empty() {
        link.insert("at".into(), names.into());
    }
    link.insert("description".into(), VALUE_LIST.into());
    Value::Object(link)
}

/// Whether a line designates and lands somewhere written in full.
fn lands_in_full(line: &Value, designating: &HashSet<String>) -> bool {
    line.get("line")
        .and_then(Value::as_str)
        .is_some_and(|n| designating.contains(n))
        && line
            .get("at")
            .and_then(Value::as_str)
            .is_some_and(|at| at.contains(':'))
}

/// A card's lines with each designating line whose landing (`at`) is written in full in
/// another card than the one it reaches split in two (a designating line
/// lands inside the card it reaches): the line itself to its card, with no landing, and
/// after it a `link` from the same place to where the value is (`to` that card as written,
/// a card ghost by its own; `at` the names there), said 「使ってよい値の一覧（移し替え）」. The
/// link's id is made from the line's, so main and a branch, and a document and its base,
/// split alike. A landing in full that names no card cannot be a line: it is taken off and
/// written at the end of the description. One written in full inside the card it reaches is
/// left as it is.
fn split_landings(
    lines: Vec<Value>,
    card: &str,
    base: &str,
    view: &View,
    designating: &HashSet<String>,
) -> (Vec<Value>, Split) {
    let mut split = Split::default();
    let mut out = Vec::with_capacity(lines.len());
    for line in lines {
        if !lands_in_full(&line, designating) {
            out.push(line);
            continue;
        }
        let Some(obj) = line.as_object() else {
            out.push(line);
            continue;
        };
        let at = text(obj, "at");
        let there = view.landing(&at, base);
        let inside = match (obj.get("to").and_then(Value::as_str), &there) {
            (Some(to), Some((_, real, _))) => view.real(to).as_deref() == Some(real.as_str()),
            _ => false,
        };
        if inside {
            out.push(line);
            continue;
        }
        let (kind, from) = (text(obj, "line"), text(obj, "from"));
        let mut pointer = obj.clone();
        pointer.shift_remove("at");
        let pointer = Value::Object(pointer);
        match there {
            Some((written, _, rest)) => {
                let link = value_list(card, &pointer, &from, &written, &rest);
                out.push(pointer);
                out.push(link);
                split.count += 1;
            }
            None => {
                out.push(pointer);
                let place = if from.is_empty() {
                    String::new()
                } else {
                    format!("{from} から出る")
                };
                split.notes.push(format!("{place}線 {kind} の着く所 {at}"));
                split.kept.push(if from.is_empty() {
                    keyed!("lineMove.line.landingOutside", kind, at)
                } else {
                    keyed!("lineMove.line.landingOutsideFrom", from, kind, at)
                });
            }
        }
    }
    (out, split)
}

/// A document of a store or a file moved before (its lines are its connections) with its
/// landings elsewhere split in two (`split_landings`); none when it has none.
fn split_doc(doc: &Obj, ctx: &Ctx) -> Option<(Obj, Moved)> {
    let lines = doc.get("connections").and_then(Value::as_array)?;
    if !lines.iter().any(|l| lands_in_full(l, ctx.designating)) {
        return None;
    }
    let (card, base, name) = (text(doc, "id"), text(doc, "section"), text(doc, "name"));
    let (lines, split) = split_landings(lines.clone(), &card, &base, ctx.view, ctx.designating);
    if split.count == 0 && split.kept.is_empty() {
        return None;
    }
    let mut doc = doc.clone();
    doc.insert("connections".into(), Value::Array(lines));
    if !split.notes.is_empty() {
        let mut fields = doc
            .get("fields")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        note_down(&mut fields, SPLIT_HERE, &split.notes);
        doc.insert("fields".into(), Value::Object(fields));
    }
    let mut moved = Moved::default();
    if split.count > 0 {
        moved.counts.insert("link".into(), split.count);
    }
    moved.split = vec![name.clone(); split.count];
    moved.kept = split
        .kept
        .iter()
        .map(|k| {
            keyed!(
                "lineMove.line.inCard",
                name = name.as_str(),
                line = k.clone()
            )
        })
        .collect();
    Some((doc, moved))
}

// MARK: - A tree

/// A tree's record: one commit `name` of lines, after the last.
fn record(
    conn: &Connection,
    project: &str,
    tree: &str,
    name: &str,
    lines: &[crate::text::Keyed],
) -> Result<(), String> {
    if lines.is_empty() {
        return Ok(());
    }
    let seq: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(seq), 0) + 1 FROM commits WHERE project_id = ? AND branch_id = ?",
            params![project, tree],
            |r| r.get(0),
        )
        .map_err(sql)?;
    let id = crate::write::new_id();
    conn.execute(
        "INSERT INTO commits (id, project_id, branch_id, seq, at, author_kind, author, kind, name) VALUES (?, ?, ?, ?, strftime('%Y-%m-%dT%H:%M:%fZ','now'), 'screen', NULL, 'migrate', ?)",
        params![id, project, tree, seq, name],
    )
    .map_err(sql)?;
    for (i, line) in lines.iter().enumerate() {
        crate::write::insert_line(
            conn,
            &id,
            i as i64,
            "migrate",
            "project",
            Some(project),
            line,
        )
        .map_err(sql)?;
    }
    Ok(())
}

/// What a tree's moves add up to, said as its record: each kind of line in the order the
/// definitions have them (`order`), the pointers split in two, what could not be a line.
fn said(
    moved: &Moved,
    labels: &dyn Fn(&str) -> String,
    order: &[String],
) -> Vec<crate::text::Keyed> {
    let mut out = Vec::new();
    if !moved.counts.is_empty() {
        let total: usize = moved.counts.values().sum();
        let mut each: Vec<(&String, &usize)> = moved.counts.iter().collect();
        each.sort_by_key(|(line, _)| order.iter().position(|o| o == *line).unwrap_or(usize::MAX));
        out.push(keyed!(
            "lineMove.line.moved",
            total,
            kinds =
                crate::text::List::of(each.iter().map(|(line, n)| format!("{} {n}", labels(line))))
        ));
    }
    out.extend(split_said(
        keyed!("lineMove.part.splitPointers"),
        &moved.split,
    ));
    out.extend(moved.kept.iter().cloned());
    out
}

/// The lines split in two said (`what` they were, how many, and the cards that have them).
fn split_said(what: crate::text::Keyed, cards: &[String]) -> Option<crate::text::Keyed> {
    if cards.is_empty() {
        return None;
    }
    let mut names: Vec<&str> = Vec::new();
    for card in cards {
        if !names.contains(&card.as_str()) {
            names.push(card);
        }
    }
    Some(keyed!(
        "lineMove.line.split",
        what,
        count = cards.len(),
        cards = crate::text::List::of(names)
    ))
}

/// How a pass moves one document, read in the view of the tree that has it: the document
/// moved and what it did, or none when it has nothing to move.
type Pass<'p> = dyn Fn(&Obj, &View) -> Option<(Obj, Moved)> + 'p;

/// One tree's card rows moved: each document, and each base (read as main sees it).
fn move_rows(
    conn: &Connection,
    (project, tree): (&str, &str),
    list: &[Row],
    (own, main): (&View, &View),
    one: &Pass,
    changed: &mut HashMap<String, (String, String)>,
) -> Result<Moved, String> {
    let mut all = Moved::default();
    for row in list {
        let doc = row.doc.as_ref().and_then(|d| one(d, own));
        let base = row
            .base
            .as_deref()
            .and_then(|b| crate::store::parse(Some(b)))
            .and_then(|b| one(&b, main));
        if let Some((moved_doc, moved)) = doc {
            let before = Value::Object(row.doc.clone().unwrap_or_default()).to_string();
            let after = Value::Object(moved_doc).to_string();
            conn.execute(
                "UPDATE card_docs SET doc = ? WHERE project_id = ? AND branch_id = ? AND id = ?",
                params![after, project, tree, row.id],
            )
            .map_err(sql)?;
            all.add(moved);
            changed.insert(row.id.clone(), (before, after));
        }
        if let Some((moved_base, _)) = base {
            conn.execute(
                "UPDATE card_docs SET base = ? WHERE project_id = ? AND branch_id = ? AND id = ?",
                params![Value::Object(moved_base).to_string(), project, tree, row.id],
            )
            .map_err(sql)?;
        }
    }
    Ok(all)
}

/// Every project of the store moved.
pub fn move_all(conn: &Connection) -> Result<(), String> {
    for project in projects(conn)? {
        move_project(conn, &project)?;
    }
    Ok(())
}

fn projects(conn: &Connection) -> Result<Vec<String>, String> {
    let mut stmt = conn.prepare("SELECT id FROM projects").map_err(sql)?;
    stmt.query_map([], |r| r.get(0))
        .map_err(sql)?
        .collect::<Result<_, _>>()
        .map_err(sql)
}

/// The project's own definitions as it keeps them (`projects.kinds`).
fn own_kinds(conn: &Connection, project: &str) -> Result<Obj, String> {
    let text: Option<String> = conn
        .query_row("SELECT kinds FROM projects WHERE id = ?", [project], |r| {
            r.get(0)
        })
        .map_err(sql)?;
    Ok(text
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default())
}

/// The kinds of line that designate: the built-in ones, and those of the project's own lines
/// (`own_lines`, as written) whose base is `designate`.
fn designating(own_lines: &[Value]) -> HashSet<String> {
    crate::kinds::builtin_defs()
        .lines
        .iter()
        .filter(|l| l.base == crate::kinds::Base::Designate)
        .map(|l| l.name.clone())
        .chain(
            own_lines
                .iter()
                .filter(|l| l["base"] == "designate")
                .filter_map(|l| l["line"].as_str().map(str::to_string)),
        )
        .collect()
}

/// A project's branches, by id.
fn branches_of(conn: &Connection, project: &str) -> Result<Vec<String>, String> {
    let mut stmt = conn
        .prepare("SELECT id FROM branches WHERE project_id = ?")
        .map_err(sql)?;
    stmt.query_map([project], |r| r.get(0))
        .map_err(sql)?
        .collect::<Result<_, _>>()
        .map_err(sql)
}

/// The project's open requests to merge whose print is still the branch's as it is (taken
/// again once the branch is moved): by id and branch.
fn current_requests(conn: &Connection, project: &str) -> Result<Vec<(String, String)>, String> {
    let open: Vec<(String, String, String)> = {
        let mut stmt = conn
            .prepare(&format!(
                "SELECT id, branch_id, print FROM {} WHERE project_id = ? AND state = 'open'",
                crate::store::requests_table(conn)?
            ))
            .map_err(sql)?;
        stmt.query_map([project], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .map_err(sql)?
            .collect::<Result<_, _>>()
            .map_err(sql)?
    };
    let mut current = Vec::new();
    for (id, branch, print) in &open {
        if crate::tools::requests::request_print(conn, project, branch)? == *print {
            current.push((id.clone(), branch.clone()));
        }
    }
    Ok(current)
}

/// What a pass does over a project: how it moves a document, and what each tree's record (a
/// commit named `name`) says of what it did there (main's tree is the empty one).
struct Trees<'p> {
    one: &'p Pass<'p>,
    name: &'p str,
    say: &'p dyn Fn(&str, &Moved) -> Vec<crate::text::Keyed>,
}

/// One project's documents moved by a pass in every tree — main, the board, each branch, each
/// seen as it sees its cards — with a branch's prints of main's cards, the history's rows
/// and the requests to merge still as their branch is (`current`) taken again.
fn move_trees(
    conn: &Connection,
    project: &str,
    current: &[(String, String)],
    pass: &Trees,
) -> Result<(), String> {
    let branches = branches_of(conn, project)?;
    let mut trees: Vec<String> = vec![String::new(), BOARD.to_string()];
    trees.extend(branches.iter().cloned());

    // main and the board stand on their own rows; a branch on main's beneath its own.
    let main_sections = rows(conn, "section_docs", project, "")?;
    let main_cards = rows(conn, "card_docs", project, "")?;
    let main_view = view((&main_sections, &main_cards), None);
    let mut main_changed: HashMap<String, (String, String)> = HashMap::new();
    for tree in &trees {
        let own_view;
        let (seen, list) = if tree.is_empty() {
            (&main_view, main_cards.clone())
        } else {
            let sections = rows(conn, "section_docs", project, tree)?;
            let cards = rows(conn, "card_docs", project, tree)?;
            own_view = if tree == BOARD {
                view((&sections, &cards), None)
            } else {
                view((&main_sections, &main_cards), Some((&sections, &cards)))
            };
            (&own_view, cards)
        };
        let mut changed = HashMap::new();
        let moved = move_rows(
            conn,
            (project, tree),
            &list,
            (seen, &main_view),
            pass.one,
            &mut changed,
        )?;
        if tree.is_empty() {
            main_changed = changed;
        }
        record(conn, project, tree, pass.name, &(pass.say)(tree, &moved))?;
    }

    // A branch's prints of main's cards moved: what it remembered is what main had.
    for branch in &branches {
        let prints: Vec<(String, String)> = {
            let mut stmt = conn
                .prepare("SELECT card_id, print FROM branch_prints WHERE branch_id = ?")
                .map_err(sql)?;
            stmt.query_map([branch], |r| Ok((r.get(0)?, r.get(1)?)))
                .map_err(sql)?
                .collect::<Result<_, _>>()
                .map_err(sql)?
        };
        for (card, print) in prints {
            if let Some((before, after)) = main_changed.get(&card)
                && crate::schema::print(before) == print
            {
                conn.execute(
                    "UPDATE branch_prints SET print = ? WHERE branch_id = ? AND card_id = ?",
                    params![crate::schema::print(after), branch, card],
                )
                .map_err(sql)?;
            }
        }
    }

    // The history's rows of card documents moved too, so what a commit kept compares with
    // what is written now.
    for ((commit, row), mut columns) in history(conn, project)? {
        let mut moved_any = false;
        for key in ["doc", "base"] {
            if let Some(doc) = columns
                .get(key)
                .and_then(Value::as_str)
                .and_then(|t| crate::store::parse(Some(t)))
                && let Some((next, _)) = (pass.one)(&doc, &main_view)
            {
                columns.insert(key.into(), Value::Object(next).to_string().into());
                moved_any = true;
            }
        }
        if moved_any {
            conn.execute(
                "UPDATE commit_rows SET before = ? WHERE commit_id = ? AND tbl = 'card_docs' AND row_id = ?",
                params![Value::Object(columns).to_string(), commit, row],
            )
            .map_err(sql)?;
        }
    }
    for (id, branch) in current {
        let print = crate::tools::requests::request_print(conn, project, branch)?;
        conn.execute(
            &format!(
                "UPDATE {} SET print = ? WHERE id = ?",
                crate::store::requests_table(conn)?
            ),
            params![print, id],
        )
        .map_err(sql)?;
    }
    Ok(())
}

/// One project's lines moved, in every tree.
pub fn move_project(conn: &Connection, project: &str) -> Result<(), String> {
    let current = current_requests(conn, project)?;
    let mut trees: Vec<String> = vec![String::new(), BOARD.to_string()];
    trees.extend(branches_of(conn, project)?);

    // What the project needs of its own lines: a pointer's words and a signature's keys,
    // wherever they are written (a document, a base, the history).
    let mut words: Vec<String> = Vec::new();
    let mut keys: Vec<String> = Vec::new();
    for tree in &trees {
        for row in rows(conn, "card_docs", project, tree)? {
            if let Some(doc) = &row.doc {
                needs(doc, &mut words, &mut keys);
            }
            if let Some(base) = crate::store::parse(row.base.as_deref()) {
                needs(&base, &mut words, &mut keys);
            }
        }
    }
    for (_, columns) in history(conn, project)? {
        for key in ["doc", "base"] {
            if let Some(doc) = columns
                .get(key)
                .and_then(Value::as_str)
                .and_then(|t| crate::store::parse(Some(t)))
            {
                needs(&doc, &mut words, &mut keys);
            }
        }
    }
    words.sort();
    let mut own = own_kinds(conn, project)?;
    let mut own_lines: Vec<Value> = own
        .get("lines")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let builtin = crate::kinds::builtin_defs();
    let mut word_names: HashMap<String, String> = HashMap::new();
    let mut made_lines: Vec<String> = Vec::new();
    {
        let named = |n: &str, own_lines: &[Value]| {
            builtin.line(n).is_some() || own_lines.iter().any(|l| l["line"] == n)
        };
        for word in &words {
            // A line the project has already for the word (a move done before).
            if let Some(existing) = own_lines.iter().find(|l| {
                l["export"]["key"] == "x-pointsAt"
                    && l["export"]["word"].as_str().or(l["line"].as_str()) == Some(word.as_str())
            }) {
                word_names.insert(
                    word.clone(),
                    existing["line"].as_str().unwrap_or(word).to_string(),
                );
                continue;
            }
            let name = name_for(word, &|n| named(n, &own_lines));
            own_lines.push(word_definition(word, &name));
            made_lines.push(name.clone());
            word_names.insert(word.clone(), name);
        }
        for key in ["signedBy", "verifiedBy"] {
            if keys.iter().any(|k| k == key) && !named(key, &own_lines) {
                own_lines.push(key_definition(key));
                made_lines.push(key.to_string());
            }
        }
    }
    if !made_lines.is_empty() {
        own.insert("lines".into(), Value::Array(own_lines.clone()));
        conn.execute(
            "UPDATE projects SET kinds = ? WHERE id = ?",
            params![Value::Object(own.clone()).to_string(), project],
        )
        .map_err(sql)?;
    }
    let labels = |line: &str| -> String {
        builtin
            .line(line)
            .map(|l| l.label.clone())
            .or_else(|| {
                own_lines
                    .iter()
                    .find(|l| l["line"] == line)
                    .and_then(|l| l["label"].as_str().map(str::to_string))
            })
            .unwrap_or_else(|| line.to_string())
    };

    // The kinds of line in their definitions' order: the built-in ones, then the project's.
    let order: Vec<String> = builtin
        .lines
        .iter()
        .map(|l| l.name.clone())
        .chain(
            own_lines
                .iter()
                .filter_map(|l| l["line"].as_str().map(str::to_string)),
        )
        .collect();
    let designating = designating(&own_lines);
    let one = |doc: &Obj, view: &View| {
        move_doc(
            doc,
            &Ctx {
                view,
                words: &word_names,
                designating: &designating,
            },
        )
    };
    let say = |tree: &str, moved: &Moved| {
        let mut lines = Vec::new();
        if tree.is_empty() && !made_lines.is_empty() {
            lines.push(keyed!(
                "lineMove.line.kindsMade",
                kinds = crate::text::List::of(made_lines.iter().map(|n| labels(n)))
            ));
        }
        lines.extend(said(moved, &labels, &order));
        lines
    };
    move_trees(
        conn,
        project,
        &current,
        &Trees {
            one: &one,
            name: "線の書き方を移した",
            say: &say,
        },
    )
}

/// One project's card documents moved by `one` — a document moved, or none when it has nothing
/// to move — in every tree (main, the board, each branch), its bases and the history's rows
/// with them, and a branch's prints of main's cards and the open requests' prints taken again,
/// as the lines are (`move_trees`). Each tree that moved any says so in a commit `name` of one
/// record, `say` of the names of the cards it moved there (a tree with none gets none).
pub(crate) fn move_card_docs(
    conn: &Connection,
    project: &str,
    name: &str,
    one: &dyn Fn(&Obj) -> Option<Obj>,
    say: &dyn Fn(&[String]) -> Vec<crate::text::Keyed>,
) -> Result<(), String> {
    let current = current_requests(conn, project)?;
    let pass = |doc: &Obj, _: &View| {
        one(doc).map(|moved| {
            let card = Moved {
                cards: vec![text(doc, "name")],
                ..Moved::default()
            };
            (moved, card)
        })
    };
    let said = |_: &str, moved: &Moved| {
        if moved.cards.is_empty() {
            Vec::new()
        } else {
            say(&moved.cards)
        }
    };
    move_trees(
        conn,
        project,
        &current,
        &Trees {
            one: &pass,
            name,
            say: &said,
        },
    )
}

/// Every project of a store whose lines were moved before (the mark
/// `core_0007_landings`) with its designating lines that land in another card than the one
/// they reach split in two (`split_landings`), once.
pub fn split_all(conn: &Connection) -> Result<(), String> {
    for project in projects(conn)? {
        split_project(conn, &project)?;
    }
    Ok(())
}

/// One project's designating lines that land in another card than the one they reach split in
/// two, in every tree; each tree that had any says so in a commit of one record. A project
/// with none is left as it is.
pub fn split_project(conn: &Connection, project: &str) -> Result<(), String> {
    let own_lines: Vec<Value> = own_kinds(conn, project)?
        .get("lines")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let designating = designating(&own_lines);
    let has = |text: Option<&str>| {
        crate::store::parse(text).is_some_and(|doc| {
            doc.get("connections")
                .and_then(Value::as_array)
                .is_some_and(|l| l.iter().any(|l| lands_in_full(l, &designating)))
        })
    };
    let any = conn
        .prepare("SELECT doc, base FROM card_docs WHERE project_id = ?")
        .map_err(sql)?
        .query_map([project], |r| {
            Ok((
                r.get::<_, Option<String>>(0)?,
                r.get::<_, Option<String>>(1)?,
            ))
        })
        .map_err(sql)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sql)?
        .iter()
        .any(|(doc, base)| has(doc.as_deref()) || has(base.as_deref()))
        || history(conn, project)?.iter().any(|(_, columns)| {
            ["doc", "base"]
                .iter()
                .any(|k| has(columns.get(*k).and_then(Value::as_str)))
        });
    if !any {
        return Ok(());
    }
    let current = current_requests(conn, project)?;
    let words = HashMap::new();
    let one = |doc: &Obj, view: &View| {
        split_doc(
            doc,
            &Ctx {
                view,
                words: &words,
                designating: &designating,
            },
        )
    };
    let say = |_: &str, moved: &Moved| {
        split_said(keyed!("lineMove.part.splitLandings"), &moved.split)
            .into_iter()
            .chain(moved.kept.iter().cloned())
            .collect()
    };
    move_trees(
        conn,
        project,
        &current,
        &Trees {
            one: &one,
            name: "線の着く所を移した",
            say: &say,
        },
    )
}

/// The history's rows of card documents of a project: by commit and row, their columns.
type Kept = ((String, String), Obj);

fn history(conn: &Connection, project: &str) -> Result<Vec<Kept>, String> {
    let kept: Vec<(String, String, String)> = {
        let mut stmt = conn
            .prepare(
                "SELECT r.commit_id, r.row_id, r.before FROM commit_rows r JOIN commits c ON c.id = r.commit_id WHERE c.project_id = ? AND r.tbl = 'card_docs' AND r.before IS NOT NULL",
            )
            .map_err(sql)?;
        stmt.query_map([project], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .map_err(sql)?
            .collect::<Result<_, _>>()
            .map_err(sql)?
    };
    Ok(kept
        .into_iter()
        .filter_map(|(commit, row, before)| {
            Some(((commit, row), crate::store::parse(Some(&before))?))
        })
        .collect())
}

/// Every card of the store (or of one project) checked against the definitions it is written
/// in now (the built-in kinds, and the project's own lines): what a move leaves that does not
/// fit, each as `project/tree/card: problem`.
pub fn problems(conn: &Connection, only: Option<&str>) -> Result<Vec<String>, String> {
    type Kept = (String, String, String, Option<String>, Option<String>);
    let rows: Vec<Kept> = {
        let mut stmt = conn
            .prepare("SELECT project_id, branch_id, name, doc, base FROM card_docs WHERE ?1 IS NULL OR project_id = ?1 ORDER BY project_id, branch_id, name")
            .map_err(sql)?;
        stmt.query_map([only], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
        })
        .map_err(sql)?
        .collect::<Result<_, _>>()
        .map_err(sql)?
    };
    let mut defs_of: HashMap<String, crate::kinds::Defs> = HashMap::new();
    let mut out = Vec::new();
    for (project, branch, name, doc, base) in rows {
        if !defs_of.contains_key(&project) {
            let own: Option<String> = conn
                .query_row("SELECT kinds FROM projects WHERE id = ?", [&project], |r| {
                    r.get(0)
                })
                .map_err(sql)?;
            defs_of.insert(
                project.clone(),
                crate::kinds::Defs::of_project(own.as_deref()),
            );
        }
        let defs = &defs_of[&project];
        for (text, what) in [(doc, String::new()), (base, text!("lineMove.problem.base"))] {
            let Some(doc) = text.and_then(|t| serde_json::from_str::<Value>(&t).ok()) else {
                continue;
            };
            // A kind of the project's own, and the board's goal and task, keep what their
            // definitions no longer have.
            let own = doc["kind"]
                .as_str()
                .is_some_and(|k| defs.cards().keeps_strays(k));
            for problem in crate::kinds::check_card(defs.cards(), &doc)
                .into_iter()
                .filter(|p| !(own && p.stray))
                .chain(crate::kinds::check_connections(defs, &doc))
            {
                out.push(format!("{project}/{branch}/{name}{what}: {problem}"));
            }
        }
    }
    Ok(out)
}
