//! The MCP surface: the tools as
//! data — the catalog, every tool's definition and the texts beside them — and what the
//! protocol answers from it: the list (each description's first sentence, each argument
//! one level deep), `guide`, the complaints about arguments that do not fit, and the
//! JSON-RPC envelopes of both eras of the protocol. The agent's daemon (`agent/src/mcp.rs`)
//! hands every message here and keeps only
//! what is HTTP's: a `subscriptions/listen` that passes is answered with `Reply::listen`,
//! for it to open the stream.
//!
//! The catalog (`catalog.json`) is this crate's data, changed here.

use crate::ojson;
use serde_json::{Map, Value, json};
use std::collections::HashMap;
use std::sync::OnceLock;

const CATALOG: &str = include_str!("catalog.json");
/// The tools (`{"tools": [...]}`): written out from the project's Definition (`export::tools`), not changed by hand.
const TOOLS: &str = include_str!("tools.json");

// The other languages' words over the Japanese ones (`text/<language>.json`), by `build.rs`:
// `OVERLAYS`.
include!(concat!(env!("OUT_DIR"), "/mcp_overlays.rs"));

/// The catalog, read once for each language: the words (`catalog.json`) with the tools (`tools.json`)
/// put in, said in the call's language (`specboard://SpecBoard/Docs/Design:Localization` §5).
pub fn catalog() -> &'static Value {
    static PARSED: [OnceLock<Value>; crate::text::LANGUAGES.len()] =
        [const { OnceLock::new() }; crate::text::LANGUAGES.len()];
    let index = crate::text::current_index();
    PARSED[index].get_or_init(|| said_in(catalog_written(), crate::text::LANGUAGES[index]))
}

/// The catalog as it is written, in Japanese.
fn catalog_written() -> Value {
    let mut whole: Value = serde_json::from_str(CATALOG).expect("catalog.json is JSON");
    let tools: Value = serde_json::from_str(TOOLS).expect("tools.json is JSON");
    if let (Some(into), Some(list)) = (whole.as_object_mut(), Some(&tools).filter(|t| t.is_array()))
    {
        // Where `tools` stood in the catalog, as before it was split.
        let mut ordered = Map::new();
        let mut put = false;
        for (key, value) in std::mem::take(into) {
            ordered.insert(key.clone(), value);
            if key == "serverName" {
                ordered.insert("tools".into(), list.clone());
                put = true;
            }
        }
        if !put {
            ordered.insert("tools".into(), list.clone());
        }
        *into = ordered;
    }
    whole
}

/// A language's words over the catalog's: the catalog's own by their JSON pointers (`catalog`), and
/// each tool's by its name and the JSON pointer inside it (`tools`).
pub(crate) struct Overs {
    pub catalog: HashMap<String, crate::text::Over>,
    pub tools: HashMap<String, HashMap<String, crate::text::Over>>,
}

/// The words of `lang` over the Japanese ones; none for Japanese and for a language with no file.
pub(crate) fn overs(lang: &str) -> Option<&'static Overs> {
    static READ: [OnceLock<Option<Overs>>; crate::text::LANGUAGES.len()] =
        [const { OnceLock::new() }; crate::text::LANGUAGES.len()];
    let index = crate::text::LANGUAGES.iter().position(|l| *l == lang)?;
    READ[index]
        .get_or_init(|| {
            let (_, source) = OVERLAYS.iter().find(|(code, _)| *code == lang)?;
            let name = format!("mcp/text/{lang}.json");
            let parsed: Value =
                serde_json::from_str(source).unwrap_or_else(|e| panic!("{name} is not JSON: {e}"));
            let catalog = crate::text::overs(&parsed["catalog"], &format!("{name} catalog"));
            let tools = parsed["tools"]
                .as_object()
                .into_iter()
                .flatten()
                .map(|(tool, places)| {
                    (
                        tool.clone(),
                        crate::text::overs(places, &format!("{name} tools.{tool}")),
                    )
                })
                .collect();
            Some(Overs { catalog, tools })
        })
        .as_ref()
}

/// The languages with a file of their own, by code.
#[cfg(test)]
pub(crate) fn overlay_languages() -> impl Iterator<Item = &'static str> {
    OVERLAYS.iter().map(|(code, _)| *code)
}

/// Every text of the catalog a person or an AI reads in a language, with where: the catalog's own
/// (`None`) or a tool's (its name), the JSON pointer, and the Japanese written there. A form's
/// `label` is a name the guides and the code know it by, never said otherwise.
pub(crate) fn places(catalog: &Value) -> Vec<(Option<String>, String, String)> {
    fn walk(value: &Value, pointer: String, out: &mut Vec<(String, String)>) {
        match value {
            Value::String(said) if crate::text::has_japanese(said) => {
                out.push((pointer, said.clone()))
            }
            Value::Array(items) => {
                for (i, item) in items.iter().enumerate() {
                    walk(item, format!("{pointer}/{i}"), out);
                }
            }
            Value::Object(object) => {
                for (key, item) in object {
                    let key = key.replace('~', "~0").replace('/', "~1");
                    walk(item, format!("{pointer}/{key}"), out);
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    for (key, value) in catalog.as_object().into_iter().flatten() {
        if key == "tools" {
            continue;
        }
        let mut found = Vec::new();
        walk(value, format!("/{key}"), &mut found);
        out.extend(found.into_iter().map(|(p, j)| (None, p, j)));
    }
    for tool in catalog["tools"].as_array().into_iter().flatten() {
        let name = tool["name"].as_str().unwrap_or_default().to_string();
        let mut found = Vec::new();
        walk(tool, String::new(), &mut found);
        out.extend(
            found
                .into_iter()
                .filter(|(p, _)| !is_form_label(p))
                .map(|(p, j)| (Some(name.clone()), p, j)),
        );
    }
    out
}

/// Whether a JSON pointer inside a tool is a form's `label` (`/forms/<n>/label`).
fn is_form_label(pointer: &str) -> bool {
    let parts: Vec<&str> = pointer.split('/').collect();
    parts.len() == 4 && parts[1] == "forms" && parts[3] == "label"
}

/// The catalog in `lang`'s words: each text with words over it made from what is written there now.
fn said_in(mut catalog: Value, lang: &str) -> Value {
    let Some(overs) = overs(lang) else {
        return catalog;
    };
    for (tool, pointer, japanese) in places(&catalog) {
        let said = match &tool {
            None => crate::text::over(&overs.catalog, &pointer, &japanese),
            Some(name) => overs
                .tools
                .get(name)
                .and_then(|o| crate::text::over(o, &pointer, &japanese)),
        };
        let Some(said) = said else {
            continue;
        };
        let at = match &tool {
            None => catalog.pointer_mut(&pointer),
            Some(name) => catalog["tools"]
                .as_array_mut()
                .and_then(|tools| tools.iter_mut().find(|t| t["name"] == name.as_str()))
                .and_then(|t| t.pointer_mut(&pointer)),
        };
        if let Some(at) = at {
            *at = Value::String(said.to_string());
        }
    }
    catalog
}

fn tools() -> &'static [Value] {
    catalog()["tools"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

fn text(key: &str) -> &'static str {
    catalog()[key].as_str().unwrap_or_default()
}

pub(crate) fn tool(name: &str) -> Option<&'static Value> {
    tools().iter().find(|t| t["name"] == name)
}

/// The tools `guide` tells about: every one but itself.
fn told() -> impl Iterator<Item = &'static Value> {
    tools().iter().filter(|t| t["name"] != "guide")
}

// MARK: - The list

/// The first sentence, as the language of the call ends one, outside brackets: up to a `。` in
/// Japanese; in English up to a `.` before a space or the end (or a `。`, for a text said in Japanese
/// still).
pub fn brief(text: &str) -> String {
    let japanese = crate::text::current() == "ja";
    let mut depth = 0i32;
    let mut chars = text.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        match c {
            '（' | '(' => depth += 1,
            '）' | ')' => depth = (depth - 1).max(0),
            '。' if depth == 0 => return text[..i + c.len_utf8()].to_string(),
            '.' if depth == 0
                && !japanese
                && chars.peek().is_none_or(|(_, next)| next.is_whitespace()) =>
            {
                return text[..i + 1].to_string();
            }
            _ => {}
        }
    }
    text.to_string()
}

/// An argument schema as the list carries it: each argument with its first sentence;
/// one level into it named and typed only.
pub fn listing(schema: &Value) -> Value {
    trimmed(schema, 0)
}

fn trimmed(spec: &Value, level: usize) -> Value {
    let Value::Object(object) = spec else {
        return spec.clone();
    };
    let mut out = Map::new();
    for (key, value) in object {
        match key.as_str() {
            "description" => {
                if level <= 1
                    && let Value::String(t) = value
                {
                    out.insert(key.clone(), Value::String(brief(t)));
                }
            }
            "properties" => {
                if level < 2
                    && let Value::Object(properties) = value
                {
                    out.insert(
                        key.clone(),
                        Value::Object(
                            properties
                                .iter()
                                .map(|(k, v)| (k.clone(), trimmed(v, level + 1)))
                                .collect(),
                        ),
                    );
                }
            }
            "required" => {
                if level < 2 {
                    out.insert(key.clone(), value.clone());
                }
            }
            "items" => {
                // An array's elements sit at the array's own level.
                if level < 2 {
                    out.insert(key.clone(), trimmed(value, level));
                } else if let Value::Object(item) = value
                    && let Some(kind) = item.get("type")
                {
                    out.insert(key.clone(), json!({ "type": kind }));
                }
            }
            "x-aliases" => {}
            _ => {
                out.insert(key.clone(), value.clone());
            }
        }
    }
    Value::Object(out)
}

/// The arguments every tool means the same by, said once in `instructions`: listed by their
/// names and types only.
const SAID_ONCE: [&str; 3] = ["project", "branch", "request"];

/// The tools as `tools/list` gives them.
pub fn listed() -> Vec<Value> {
    tools()
        .iter()
        .map(|t| {
            let mut out = Map::new();
            out.insert("name".into(), t["name"].clone());
            out.insert("title".into(), t["title"].clone());
            out.insert(
                "description".into(),
                Value::String(brief(t["description"].as_str().unwrap_or_default())),
            );
            let mut schema = listing(&t["inputSchema"]);
            if let Some(properties) = schema.get_mut("properties").and_then(Value::as_object_mut) {
                for key in SAID_ONCE {
                    if let Some(Value::Object(argument)) = properties.get_mut(key) {
                        argument.shift_remove("description");
                    }
                }
                // What value is written with is each tool's own, a line for each kind of thing
                // target points at: said whole, for an agent to write from the list alone.
                if let Some(Value::Object(argument)) = properties.get_mut("value")
                    && let Some(said) = t["inputSchema"]["properties"]["value"].get("description")
                {
                    argument.insert("description".into(), said.clone());
                }
            }
            out.insert("inputSchema".into(), schema);
            if let Some(a) = t.get("annotations") {
                out.insert("annotations".into(), a.clone());
            }
            Value::Object(out)
        })
        .collect()
}

// MARK: - guide

/// Runs a tool other than `guide`: its name and arguments, to what it answers.
pub type Runner<'a> = dyn Fn(&str, &Map<String, Value>) -> Result<Output, String> + 'a;

/// What a tool answers with: structure, pretty-printed into one text, or text blocks; or a
/// picture (`app_control`'s capture), sent as an MCP image block with a line of structure
/// beside it saying what it is.
pub enum Output {
    Json(Value),
    Blocks(Vec<String>),
    Image {
        /// The picture, in base64.
        data: String,
        mime_type: String,
        note: Value,
    },
}

impl Output {
    /// What the caller reads, as one string.
    pub fn text(&self) -> String {
        match self {
            Output::Json(v) => ojson::pretty(v),
            Output::Blocks(b) => b.join("\n"),
            Output::Image {
                data,
                mime_type,
                note,
            } => {
                let kind = mime_type
                    .rsplit('/')
                    .next()
                    .unwrap_or(mime_type)
                    .to_uppercase();
                let padding = data.bytes().rev().take_while(|&b| b == b'=').count();
                let bytes = (data.len() / 4 * 3).saturating_sub(padding);
                format!(
                    "{}\n{}",
                    ojson::pretty(note),
                    text!("mcp.image.size", kind, bytes)
                )
            }
        }
    }
}

pub use guide::{argument_forms, argument_shapes, guide, guide_of};

// MARK: - Arguments

fn fits(value: &Value, expected: &str) -> bool {
    match (value, expected) {
        (Value::String(_), "string")
        | (Value::Bool(_), "boolean")
        | (Value::Array(_), "array")
        | (Value::Object(_), "object") => true,
        (Value::Number(n), "integer") => n.as_f64().is_some_and(|f| f == f.round()),
        (Value::Number(_), "number") => true,
        _ => false,
    }
}

fn describe(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(_) => text!("mcp.type.boolean"),
        Value::Number(_) => text!("mcp.type.number"),
        Value::String(_) => text!("mcp.type.string"),
        Value::Array(_) => text!("mcp.type.array"),
        Value::Object(_) => text!("mcp.type.object"),
    }
}

fn guide_hint(path: &str) -> String {
    let key = path
        .rsplit('.')
        .next()
        .and_then(|last| last.split('[').next())
        .unwrap_or_default();
    if key.is_empty() {
        String::new()
    } else {
        text!("mcp.complaint.guideHint", key)
    }
}

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// Everything wrong with these arguments, in the order they were written, inside
/// objects and arrays too.
pub fn complaints(arguments: &Map<String, Value>, schema: &Value, path: &str) -> Vec<String> {
    complaints_with(
        arguments,
        schema,
        path,
        catalog()["replacedArguments"].as_object(),
    )
}

/// A tool's value, held to the form it is written in: the keys replaced in that form are said with
/// what to write now (`replacedForms`), not those replaced in another tool or form.
pub fn complaints_of_form(
    arguments: &Map<String, Value>,
    schema: &Value,
    path: &str,
    tool: &str,
    form: &str,
) -> Vec<String> {
    complaints_with(
        arguments,
        schema,
        path,
        catalog()["replacedForms"][tool][form].as_object(),
    )
}

/// `complaints` with the table of keys that were replaced, for the keys written at this level (what is
/// inside them has none).
fn complaints_with(
    arguments: &Map<String, Value>,
    schema: &Value,
    path: &str,
    replaced: Option<&Map<String, Value>>,
) -> Vec<String> {
    let Some(Value::Object(properties)) = schema.get("properties") else {
        return Vec::new();
    };
    let prefix = if path.is_empty() {
        String::new()
    } else {
        text!("mcp.complaint.prefix", path)
    };
    let mut found = Vec::new();

    let mut unknown: Vec<&String> = arguments
        .keys()
        .filter(|k| !properties.contains_key(*k))
        .collect();
    for key in &unknown {
        if let Some(moved) = replaced.and_then(|r| r.get(*key)).and_then(Value::as_str) {
            found.push(text!("mcp.complaint.replaced", prefix, key, moved));
        }
    }
    unknown.retain(|k| replaced.and_then(|r| r.get(*k)).is_none());
    // A form with no keys at all (branch_create's value): said so, not an empty list.
    if !unknown.is_empty() && properties.is_empty() {
        found.push(text!(
            "mcp.complaint.noKeys",
            path,
            given = unknown
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ));
    } else if !unknown.is_empty() {
        let given = unknown
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        let takes = properties.keys().cloned().collect::<Vec<_>>().join(" / ");
        found.push(if path.is_empty() {
            text!("mcp.complaint.unknownTop", given, takes)
        } else {
            text!(
                "mcp.complaint.unknownNested",
                path,
                given,
                takes,
                hint = guide_hint(path)
            )
        });
    }
    if let Some(Value::Array(required)) = schema.get("required") {
        for name in required.iter().filter_map(Value::as_str) {
            if !arguments.contains_key(name) {
                // A key that takes one of a few words says them, so the next call can be right.
                let words = properties
                    .get(name)
                    .and_then(|spec| spec.get("enum"))
                    .map(strings)
                    .filter(|w| !w.is_empty());
                found.push(match words {
                    Some(words) => text!(
                        "mcp.complaint.requiredOneOf",
                        prefix,
                        name,
                        names = words.join(" / "),
                        hint = guide_hint(path)
                    ),
                    None => text!(
                        "mcp.complaint.required",
                        prefix,
                        name,
                        hint = guide_hint(path)
                    ),
                });
            }
        }
    }
    for (key, value) in arguments {
        let Some(Value::Object(spec)) = properties.get(key) else {
            continue;
        };
        if value.is_null() {
            match spec.get("type") {
                None => continue,
                Some(Value::Array(types)) if types.iter().any(|t| t == "null") => continue,
                _ => {
                    found.push(text!("mcp.complaint.noNull", prefix, key));
                    continue;
                }
            }
        }
        // One type, or several a value may be (`["string", "object"]`).
        let types: Vec<&str> = match spec.get("type") {
            Some(Value::String(one)) => vec![one.as_str()],
            Some(Value::Array(many)) => many.iter().filter_map(Value::as_str).collect(),
            _ => continue,
        };
        if !types.iter().any(|t| fits(value, t)) {
            found.push(text!(
                "mcp.complaint.wrongType",
                prefix,
                key,
                expected = types
                    .iter()
                    .filter(|t| **t != "null")
                    .copied()
                    .collect::<Vec<_>>()
                    .join(" / "),
                given = describe(value)
            ));
            continue;
        }
        if let (Some(allowed), Value::String(given)) = (spec.get("enum"), value) {
            let names = strings(allowed);
            let aliases = spec.get("x-aliases").map(strings).unwrap_or_default();
            if !names.contains(given) && !aliases.contains(given) {
                found.push(text!(
                    "mcp.complaint.oneOf",
                    prefix,
                    key,
                    names = names.join(" / "),
                    given
                ));
            }
        }
        if let (Some(Value::Object(items)), Value::Array(given)) = (spec.get("items"), value) {
            if let Some(allowed) = items.get("enum") {
                let names = strings(allowed);
                let unknown: Vec<&str> = given
                    .iter()
                    .filter_map(Value::as_str)
                    .filter(|g| !names.iter().any(|n| n == g))
                    .collect();
                if !unknown.is_empty() {
                    found.push(text!(
                        "mcp.complaint.badValues",
                        prefix,
                        key,
                        unknown = unknown.join(", "),
                        names = names.join(" / ")
                    ));
                }
            }
            if items.get("type").and_then(Value::as_str) == Some("string") {
                for (index, element) in given.iter().enumerate() {
                    if !element.is_string() {
                        found.push(text!(
                            "mcp.complaint.itemNotString",
                            prefix,
                            key,
                            index,
                            given = describe(element)
                        ));
                    }
                }
            }
        }
        let here = if path.is_empty() {
            key.clone()
        } else {
            format!("{path}.{key}")
        };
        if let Value::Object(nested) = value
            && spec.contains_key("properties")
        {
            found.extend(complaints_with(
                nested,
                &Value::Object(spec.clone()),
                &here,
                None,
            ));
        }
        if let (Value::Array(items), Some(item_spec @ Value::Object(item))) =
            (value, spec.get("items"))
            && item.contains_key("properties")
        {
            for (index, element) in items.iter().enumerate() {
                if let Value::Object(element) = element {
                    found.extend(complaints_with(
                        element,
                        item_spec,
                        &format!("{here}[{index}]"),
                        None,
                    ));
                }
            }
        }
    }
    found
}

// MARK: - JSON-RPC

/// An answer to one message: the HTTP status, and the body (none for a notification).
pub struct Reply {
    pub status: u16,
    pub body: Option<Value>,
    /// A `subscriptions/listen` that passed: a stream for the HTTP side to open, there
    /// being nothing to answer with here (`listen_events` in `agent/src/mcp.rs`).
    pub listen: bool,
}

impl Reply {
    fn empty(status: u16) -> Reply {
        Reply {
            status,
            body: None,
            listen: false,
        }
    }
}

const MODERN_VERSIONS: [&str; 1] = ["2026-07-28"];
/// The versions of the 2025 family `initialize` agrees to, newest first: one asked for is
/// answered with itself, any other with the newest (the handshake's rule).
const HANDSHAKE_VERSIONS: [&str; 4] = ["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];
const LIST_TTL: i64 = 5 * 60 * 1000;

fn server_info() -> Value {
    json!({ "name": text("serverName"), "version": crate::VERSION })
}

fn ok(id: Value, result: Value) -> Reply {
    Reply {
        status: 200,
        body: Some(json!({ "jsonrpc": "2.0", "id": id, "result": result })),
        listen: false,
    }
}

fn error(status: u16, id: Value, code: i64, message: &str, data: Option<Value>) -> Reply {
    let mut e = Map::new();
    e.insert("code".into(), code.into());
    e.insert("message".into(), message.into());
    if let Some(d) = data {
        e.insert("data".into(), d);
    }
    Reply {
        status,
        body: Some(json!({ "jsonrpc": "2.0", "id": id, "error": Value::Object(e) })),
        listen: false,
    }
}

fn content(output: Result<Output, String>, call: Option<(&str, &Map<String, Value>)>) -> Value {
    let text = |t: String| json!({"type": "text", "text": t});
    let (blocks, is_error) = match output {
        // Packed, not indented (an indented answer is a third to a half bigger).
        Ok(Output::Json(v)) => {
            let packed = ojson::compact(&v);
            // A refusal put in an answer is put as its words (`refusal_words`).
            debug_assert!(
                !packed.contains(BESIDE) && !packed.contains("\\u001e"),
                "{packed}"
            );
            (vec![text(packed)], false)
        }
        Ok(Output::Blocks(b)) => (b.into_iter().map(text).collect(), false),
        // The picture first, then what it is.
        Ok(Output::Image {
            data,
            mime_type,
            note,
        }) => (
            vec![
                json!({"type": "image", "data": data, "mimeType": mime_type}),
                text(ojson::compact(&note)),
            ],
            false,
        ),
        Err(message) => (
            vec![text(ojson::compact(
                &json!({ "error": refused(&message, call) }),
            ))],
            true,
        ),
    };
    json!({ "content": blocks, "isError": is_error })
}

// MARK: - Refusals

/// What sets a refusal's words apart from what it says beside them (`refusal`): a character no text
/// of the core writes.
const BESIDE: char = '\u{1e}';

/// A refusal that says more than its words, as `beside` says it: the topic of `guide` that tells how
/// to write it (`guide`), the call that goes through as it is (`use`), or the places in the call to
/// change for it (`fix`: `[{at: <JSON pointer into the call's arguments>, to: <value>}]`, made into
/// `use` where the call is known). Carried in the refusal's text behind `BESIDE` up to where the
/// answer is made: words put before or after it on the way stay words.
pub(crate) fn refusal(message: impl Into<String>, beside: Value) -> String {
    let mut out = message.into();
    if beside.as_object().is_some_and(|b| !b.is_empty()) {
        out.push(BESIDE);
        out.push_str(&ojson::compact(&beside));
    }
    out
}

/// A refusal's words, and what it says beside them (`refusal`; the first said wins).
pub(crate) fn refusal_parts(said: &str) -> (String, Map<String, Value>) {
    let mut words = String::new();
    let mut beside = Map::new();
    let mut rest = said;
    while let Some(at) = rest.find(BESIDE) {
        words.push_str(&rest[..at]);
        let after = &rest[at + BESIDE.len_utf8()..];
        let mut stream = serde_json::Deserializer::from_str(after).into_iter::<Value>();
        match stream.next() {
            Some(Ok(Value::Object(more))) => {
                for (k, v) in more {
                    // The places to change add up; the rest, the first said wins.
                    match (beside.get_mut(&k), v) {
                        (Some(Value::Array(have)), Value::Array(more)) if k == "fix" => {
                            have.extend(more)
                        }
                        (Some(_), _) => {}
                        (None, v) => {
                            beside.insert(k, v);
                        }
                    }
                }
                rest = &after[stream.byte_offset()..];
            }
            _ => rest = after,
        }
    }
    words.push_str(rest);
    (words, beside)
}

/// One place of a JSON pointer, as the pointer writes it.
pub(crate) fn pointer_key(key: &str) -> String {
    key.replace('~', "~0").replace('/', "~1")
}

/// A refusal with the places it says to change (`fix`) moved to where the call that holds the one
/// refused puts them: each begins with `base`. None for `base` drops them, for a call whose arguments
/// the refused one is not in.
pub(crate) fn rebase_fix(said: &str, base: Option<&str>) -> String {
    if !said.contains(BESIDE) {
        return said.to_string();
    }
    let (words, mut beside) = refusal_parts(said);
    match (beside.get_mut("fix"), base) {
        (Some(Value::Array(fixes)), Some(base)) => {
            for fix in fixes {
                if let Some(at) = fix["at"].as_str() {
                    fix["at"] = format!("{base}{at}").into();
                }
            }
        }
        (Some(_), None) => {
            beside.shift_remove("fix");
        }
        _ => {}
    }
    refusal(words, Value::Object(beside))
}

/// A refusal's words alone, for a reader that takes text (the window).
pub(crate) fn refusal_words(said: &str) -> String {
    refusal_parts(said).0
}

/// A refusal as MCP answers it: `{message, guide?, use?}`, `use` the call `call` made with what
/// `fix` says changed.
fn refused(said: &str, call: Option<(&str, &Map<String, Value>)>) -> Value {
    let (message, mut beside) = refusal_parts(said);
    let mut out = Map::new();
    out.insert("message".into(), message.into());
    if let Some(guide) = beside.remove("guide") {
        out.insert("guide".into(), guide);
    }
    let fixed = match (beside.remove("fix"), call) {
        (Some(Value::Array(fixes)), Some((tool, arguments))) => {
            let mut arguments = Value::Object(arguments.clone());
            let mut all = true;
            for fix in &fixes {
                let at = fix["at"].as_str().unwrap_or_default();
                match arguments.pointer_mut(at) {
                    Some(place) => *place = fix["to"].clone(),
                    None => all = false,
                }
            }
            all.then(|| json!({ "tool": tool, "arguments": arguments }))
        }
        _ => None,
    };
    if let Some(call) = beside.remove("use").or(fixed) {
        out.insert("use".into(), call);
    }
    Value::Object(out)
}

/// A tool call: checked against the tool's own schema before anything runs.
fn call_tool(params: Option<&Value>, run: &Runner<'_>) -> Result<Value, (i64, String)> {
    let Some(Value::Object(params)) = params else {
        return Err((-32602, text!("mcp.call.nameMissing")));
    };
    let Some(Value::String(name)) = params.get("name") else {
        return Err((-32602, text!("mcp.call.nameMissing")));
    };
    let Some(t) = tool(name) else {
        let gone = catalog()["gone"][name.as_str()]
            .as_str()
            .map(str::to_string);
        return Err((
            -32602,
            gone.unwrap_or_else(|| text!("mcp.call.noTool", name)),
        ));
    };
    // Arguments that are not an object are turned away, not read as none.
    let arguments = match params.get("arguments") {
        Some(Value::Object(given)) => given.clone(),
        None | Some(Value::Null) => Map::new(),
        Some(other) => {
            return Err((
                -32602,
                text!("mcp.call.argumentsNotObject", given = describe(other)),
            ));
        }
    };
    let found = complaints(&arguments, &t["inputSchema"], "");
    if !found.is_empty() {
        let mut complaint = found.join("\n");
        if !complaint.contains("guide") {
            complaint += &text!("mcp.call.shapeHint", name);
        }
        let complaint = refusal(complaint, json!({ "guide": name }));
        return Ok(content(Err(complaint), Some((name, &arguments))));
    }
    let outcome = if name == "guide" {
        match arguments.get("about") {
            // Several topics in one call: a name the built-in topics do not know is asked of the
            // project, as one alone is.
            Some(Value::Array(abouts)) => {
                let abouts: Vec<String> = abouts
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect();
                let elsewhere = |about: &str| -> Option<Value> {
                    if !arguments.contains_key("project") {
                        return None;
                    }
                    let mut one = arguments.clone();
                    one.insert("about".into(), about.into());
                    match run(name, &one) {
                        Ok(Output::Json(answer)) => Some(answer),
                        _ => None,
                    }
                };
                guide_of(&abouts, &elsewhere)
            }
            // A kind of card the project defines: asked of the project, once the
            // built-in guides do not know it.
            about => match guide(about.and_then(Value::as_str)) {
                Err(_) if arguments.contains_key("project") => run(name, &arguments),
                outcome => outcome,
            },
        }
    } else {
        run(name, &arguments)
    };
    Ok(content(outcome, Some((name, &arguments))))
}

/// One JSON-RPC message, answered as the server answers it. `headers` are the
/// request's, by lower-case name; `run` runs a tool other than `guide`.
pub fn answer(message: &Value, headers: &Map<String, Value>, run: &Runner<'_>) -> Reply {
    // A batch is JSON, but not taken: the 2025-06-18 protocol has none.
    if message.is_array() {
        return error(200, Value::Null, -32600, &text!("mcp.rpc.batch"), None);
    }
    let Value::Object(envelope) = message else {
        return error(200, Value::Null, -32700, &text!("mcp.rpc.notJson"), None);
    };
    // A message is JSON-RPC 2.0's only when it says so.
    if envelope.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
        let id = envelope.get("id").cloned().unwrap_or(Value::Null);
        return error(200, id, -32600, &text!("mcp.rpc.versionMissing"), None);
    }
    let version = envelope
        .get("params")
        .and_then(|p| p.get("_meta"))
        .and_then(|m| m.get("io.modelcontextprotocol/protocolVersion"))
        .and_then(Value::as_str);
    match version {
        Some(version) => modern(envelope, version, headers, run),
        None => dispatch(envelope, run),
    }
}

/// The 2025 family: a handshake with `initialize`.
fn dispatch(envelope: &Map<String, Value>, run: &Runner<'_>) -> Reply {
    let id = envelope.get("id").cloned().unwrap_or(Value::Null);
    let Some(Value::String(method)) = envelope.get("method") else {
        return error(200, id, -32600, &text!("mcp.rpc.methodMissing"), None);
    };
    match method.as_str() {
        "initialize" => {
            let asked = envelope
                .get("params")
                .and_then(|p| p.get("protocolVersion"))
                .and_then(Value::as_str);
            let version = asked
                .filter(|v| HANDSHAKE_VERSIONS.contains(v))
                .unwrap_or(HANDSHAKE_VERSIONS[0]);
            ok(
                id,
                json!({
                    "protocolVersion": version,
                    "capabilities": {"tools": {"listChanged": false}},
                    "serverInfo": server_info(),
                    "instructions": text("instructions"),
                }),
            )
        }
        "notifications/initialized" | "notifications/cancelled" => Reply::empty(202),
        "ping" => ok(id, json!({})),
        "tools/list" => ok(id, json!({ "tools": listed() })),
        "tools/call" => match call_tool(envelope.get("params"), run) {
            Ok(result) => ok(id, result),
            Err((code, message)) => error(200, id, code, &message, None),
        },
        other => error(
            200,
            id,
            -32601,
            &text!("mcp.rpc.unknownMethod", other),
            None,
        ),
    }
}

fn modern_result(id: Value, fields: Value) -> Reply {
    let mut result = Map::new();
    result.insert("resultType".into(), "complete".into());
    if let Value::Object(f) = fields {
        result.extend(f);
    }
    result.insert(
        "_meta".into(),
        json!({ "io.modelcontextprotocol/serverInfo": server_info() }),
    );
    ok(id, Value::Object(result))
}

/// A header value, with `=?base64?…?=` decoded.
fn header(headers: &Map<String, Value>, name: &str) -> Option<String> {
    let given = headers.get(name)?.as_str()?;
    if let Some(inner) = given
        .strip_prefix("=?base64?")
        .and_then(|r| r.strip_suffix("?="))
        && let Some(decoded) = base64_decode(inner)
    {
        return Some(decoded);
    }
    Some(given.to_string())
}

fn base64_decode(text: &str) -> Option<String> {
    const TABLE: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut bits = 0u32;
    let mut count = 0;
    let mut out = Vec::new();
    for c in text.chars() {
        if c == '=' {
            break;
        }
        let v = TABLE.find(c)? as u32;
        bits = (bits << 6) | v;
        count += 6;
        if count >= 8 {
            count -= 8;
            out.push((bits >> count) as u8);
            bits &= (1 << count) - 1;
        }
    }
    String::from_utf8(out).ok()
}

/// The 2026-07-28 era: stateless, every request carrying its version.
fn modern(
    envelope: &Map<String, Value>,
    version: &str,
    headers: &Map<String, Value>,
    run: &Runner,
) -> Reply {
    let Some(id) = envelope.get("id").cloned() else {
        return Reply::empty(202);
    };
    if !MODERN_VERSIONS.contains(&version) {
        return error(
            400,
            id,
            -32022,
            "Unsupported protocol version",
            Some(json!({ "supported": MODERN_VERSIONS, "requested": version })),
        );
    }
    let Some(Value::String(method)) = envelope.get("method") else {
        return error(400, id, -32600, &text!("mcp.rpc.methodMissing"), None);
    };
    let check = |name: &str, label: &str, expected: &str| -> Option<String> {
        match header(headers, name) {
            None => Some(text!("mcp.header.missing", label)),
            Some(given) if given == expected => None,
            Some(_) => Some(text!(
                "mcp.header.differs",
                label,
                given = headers[name].as_str().unwrap_or_default(),
                expected
            )),
        }
    };
    let mismatch = check("mcp-protocol-version", "MCP-Protocol-Version", version)
        .or_else(|| check("mcp-method", "Mcp-Method", method))
        .or_else(|| {
            if method == "tools/call"
                && let Some(name) = envelope
                    .get("params")
                    .and_then(|p| p.get("name"))
                    .and_then(Value::as_str)
            {
                return check("mcp-name", "Mcp-Name", name);
            }
            None
        });
    if let Some(m) = mismatch {
        return error(400, id, -32020, &m, None);
    }
    match method.as_str() {
        "server/discover" => modern_result(
            id,
            json!({
                "supportedVersions": MODERN_VERSIONS,
                "capabilities": {"tools": {"listChanged": true}},
                "instructions": text("instructions"),
                "ttlMs": LIST_TTL,
                "cacheScope": "private",
            }),
        ),
        "tools/list" => modern_result(
            id,
            json!({ "tools": listed(), "ttlMs": LIST_TTL, "cacheScope": "private" }),
        ),
        "tools/call" => match call_tool(envelope.get("params"), run) {
            Ok(result) => modern_result(id, result),
            Err((code, message)) => error(200, id, code, &message, None),
        },
        // A stream that stays open, told when the tools change: HTTP's to keep, so said
        // to the HTTP side to open rather than answered here.
        "subscriptions/listen" => Reply {
            listen: true,
            ..Reply::empty(200)
        },
        other => error(
            404,
            id,
            -32601,
            &text!("mcp.rpc.unknownMethod", other),
            None,
        ),
    }
}

mod guide;
#[cfg(test)]
mod tests;
