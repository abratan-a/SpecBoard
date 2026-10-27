//! Data cards (`instance`; `specboard://SpecBoard/Docs/Design:CardKinds`「データ（instance）と定義のセクション」): the values
//! of a shape — an entity or a value, the line `of` goes to — held as one JSON as nested as
//! the shape (`data`: a key a field's name, an array where the field is one).
//!
//! What a value is held to is its field's: its type (an `int` a whole number, a `uuid`, `url`,
//! `email`, `date` or `datetime` written so), null only where the field may be, its fixed
//! value, an array where the field is one, an object's fields, what a `ref` embeds (a shape's
//! fields, an enum's values — any text when it is open), a discriminator's branch, and a text its
//! field's `pattern` rules (each element of an array). A pattern the `regex` crate cannot read (a
//! look-around, a back-reference: JavaScript's, for zod) holds nothing, and the check says it is
//! not held to (`unread_patterns`). Not held to: the other rules, uniqueness, keys, whether what
//! an id names is there. A value whose field the shape has lost is kept (a stray), not wrong; a
//! name is followed through a field's rename by its id (`rekeyed`).

use super::Seen;
use super::cards::Scope;
use super::picks::{self, Found, Pick};
use crate::export::Project;
use crate::kinds::{INSTANCE, OF_LINE};
use crate::model::{Card, Field};
use crate::write::Tx;
use serde_json::{Map, Value, json};
use std::collections::HashMap;

/// How deep a shape's refs are followed: a shape that embeds itself stops here.
const DEPTH: usize = 24;

/// What a data card's values come to against its shape.
#[derive(Default, Debug)]
pub(crate) struct Findings {
    /// A value its field does not take: where in the data (`lines[0].qty`) and why.
    pub wrong: Vec<(String, crate::text::Keyed)>,
    /// A required field with no value: where.
    pub missing: Vec<String>,
    /// A value whose field the shape no longer has, outside any array: where, and the value.
    pub strays: Vec<(String, Value)>,
    /// An element of an array its element's keys do not hold apart (a key field missing, a key
    /// or a `unique` field's value the same as an element's before it): the array's place, the
    /// element's or its field's, and why. Refused whenever the array is written.
    pub keys: Vec<(String, String, crate::text::Keyed)>,
    /// The patterns read so far, each once a check: none for one that cannot be read.
    patterns: HashMap<String, Option<regex::Regex>>,
}

/// The regexes of a field's `pattern` rules, as written.
fn patterns(field: &Field) -> impl Iterator<Item = &str> {
    field
        .rules
        .iter()
        .filter(|r| crate::export::rule_kind(r) == "pattern")
        .filter_map(|r| crate::export::rule_string(r, "regex"))
}

/// Why a text does not match its field's patterns, when it does not: each a pattern read. A
/// pattern matches anywhere in the text unless it is anchored, as JSON Schema's and zod's do.
fn unmatched(field: &Field, text: &str, out: &mut Findings) -> Option<crate::text::Keyed> {
    for written in patterns(field) {
        let read = out
            .patterns
            .entry(written.to_string())
            .or_insert_with(|| regex::Regex::new(written).ok());
        if let Some(re) = read
            && !re.is_match(text)
        {
            return Some(keyed!(
                "instances.wrong.pattern",
                value = Value::from(text),
                written
            ));
        }
    }
    None
}

/// A data card's shape: the entity or value its line `of` goes to.
pub(crate) fn shape<'a>(project: &Project<'a>, card: &Card) -> Option<&'a Card> {
    let id = card.instance_of()?;
    project
        .card(id)
        .filter(|c| c.kind == "entity" || c.kind == "value")
}

/// A data card's values against its shape.
pub(crate) fn check(project: &Project, shape: &Card, data: Option<&Value>) -> Findings {
    let mut out = Findings::default();
    let empty = Value::Object(Map::new());
    object(
        project,
        &shape.fields,
        shape.discriminator.as_deref(),
        data.unwrap_or(&empty),
        "",
        false,
        0,
        &mut out,
    );
    out
}

/// A place in the data and a key inside it, as a path writes them (`lines[0].qty`).
fn joined(at: &str, name: &str) -> String {
    if at.is_empty() {
        crate::pathtext::quote(name).into_owned()
    } else {
        crate::pathtext::member(at, name)
    }
}

#[allow(clippy::too_many_arguments)]
fn object(
    project: &Project,
    fields: &[Field],
    discriminator: Option<&str>,
    value: &Value,
    at: &str,
    in_array: bool,
    depth: usize,
    out: &mut Findings,
) {
    let Value::Object(map) = value else {
        out.wrong
            .push((at.to_string(), keyed!("instances.wrong.notObject")));
        return;
    };
    if depth > DEPTH {
        return;
    }
    // A discriminator's branch: the fields it names, beside those of every branch.
    let branch = discriminator
        .and_then(|d| map.get(d))
        .and_then(Value::as_str);
    let active = |f: &Field| {
        f.variants.is_empty() || branch.is_some_and(|b| f.variants.iter().any(|v| v == b))
    };
    for field in fields.iter().filter(|f| active(f)) {
        let place = joined(at, &field.name);
        match map.get(&field.name) {
            Some(given) => one(project, field, given, &place, in_array, depth, out),
            None if field.is_required => out.missing.push(place),
            None => {}
        }
    }
    for (key, given) in map {
        if fields.iter().any(|f| &f.name == key && active(f)) {
            continue;
        }
        let place = joined(at, key);
        if let Some(d) = discriminator
            && fields.iter().any(|f| &f.name == key)
        {
            let why = match branch {
                Some(b) => keyed!("instances.wrong.outOfBranch", d, branch = b),
                None => keyed!("instances.wrong.outOfNoBranch", d),
            };
            out.wrong.push((place, why));
        } else if in_array {
            out.wrong.push((place, keyed!("instances.wrong.noField")));
        } else {
            out.strays.push((place, given.clone()));
        }
    }
}

/// How a value meets the other types a field takes.
enum Also {
    /// None of them: its own type holds.
    No,
    /// A scalar or an object it also takes.
    Fits,
    /// An array of its own type's elements.
    Elements,
}

fn also(field: &Field, given: &Value) -> Also {
    for t in &field.also {
        match t.as_str() {
            "string" if given.is_string() => return Also::Fits,
            "integer" if given.is_i64() || given.is_u64() => return Also::Fits,
            "number" if given.is_number() => return Also::Fits,
            "boolean" if given.is_boolean() => return Also::Fits,
            "object" if given.is_object() && !field.is_object() && !field.is_ref() => {
                return Also::Fits;
            }
            "array" if given.is_array() && !field.is_array => return Also::Elements,
            _ => {}
        }
    }
    Also::No
}

fn one(
    project: &Project,
    field: &Field,
    given: &Value,
    at: &str,
    in_array: bool,
    depth: usize,
    out: &mut Findings,
) {
    if given.is_null() {
        if !field.is_nullable {
            out.wrong
                .push((at.to_string(), keyed!("instances.wrong.nullNotAllowed")));
        }
        return;
    }
    if let Some(fixed) = field.fixed()
        && given != fixed
    {
        out.wrong
            .push((at.to_string(), keyed!("instances.wrong.notFixed", fixed)));
        return;
    }
    // The other types it takes (`also`): a value of one is its own.
    match also(field, given) {
        Also::No => {}
        Also::Fits => return,
        Also::Elements => {
            if let Value::Array(items) = given {
                for (i, item) in items.iter().enumerate() {
                    element(
                        project,
                        field,
                        item,
                        &format!("{at}[{i}]"),
                        true,
                        depth,
                        out,
                    );
                }
            }
            return;
        }
    }
    if field.is_array {
        let Value::Array(items) = given else {
            out.wrong
                .push((at.to_string(), keyed!("instances.wrong.notArray")));
            return;
        };
        for (i, item) in items.iter().enumerate() {
            element(
                project,
                field,
                item,
                &format!("{at}[{i}]"),
                true,
                depth,
                out,
            );
        }
        held_apart(inner_fields(project, field), items, at, out);
        return;
    }
    element(project, field, given, at, in_array, depth, out);
}

fn element(
    project: &Project,
    field: &Field,
    given: &Value,
    at: &str,
    in_array: bool,
    depth: usize,
    out: &mut Findings,
) {
    if given.is_null() {
        if !field.is_nullable {
            out.wrong
                .push((at.to_string(), keyed!("instances.wrong.nullNotAllowed")));
        }
        return;
    }
    if field.is_ref() {
        let Some(target) = field
            .ref_schema_id
            .as_deref()
            .and_then(|id| project.card(id))
        else {
            return;
        };
        if target.kind == "enum" {
            match given.as_str() {
                Some(text) if target.enum_open || target.values.iter().any(|v| v.value == text) => {
                }
                _ => out.wrong.push((
                    at.to_string(),
                    keyed!(
                        "instances.wrong.notEnumValue",
                        name = target.name,
                        values =
                            crate::text::List::of(target.values.iter().map(|v| v.value.as_str()))
                    ),
                )),
            }
        } else {
            object(
                project,
                &target.fields,
                target.discriminator.as_deref(),
                given,
                at,
                in_array,
                depth + 1,
                out,
            );
        }
        return;
    }
    if field.is_object() {
        object(
            project,
            &field.children,
            None,
            given,
            at,
            in_array,
            depth + 1,
            out,
        );
        return;
    }
    if let Some(why) = scalar_misfit(&field.kind, given) {
        out.wrong.push((at.to_string(), why));
    } else if let Some(text) = given.as_str()
        && let Some(why) = unmatched(field, text, out)
    {
        out.wrong.push((at.to_string(), why));
    }
}

/// The fields an array's elements are told apart by: their shape's primary key, in its order.
pub(crate) fn key_fields(inner: &[Field]) -> Vec<&Field> {
    inner.iter().filter(|f| f.is_pk).collect()
}

/// An element's key as a path picks it (`[number=53,protocol=udp]`): each key field's value as a
/// pick writes it. None when a key field has no text, number or truth.
pub(crate) fn key_pick(keys: &[&Field], element: &Value) -> Option<Pick> {
    keys.iter()
        .map(|k| {
            element
                .get(&k.name)
                .and_then(picks::Lit::of)
                .map(|lit| (k.name.clone(), lit))
        })
        .collect::<Option<Vec<_>>>()
        .map(Pick::Where)
}

/// A key as a person reads it, without its brackets (`number=53,protocol=udp`).
pub(crate) fn key_text(pick: &Pick) -> String {
    let written = pick.written();
    written
        .strip_prefix('[')
        .and_then(|w| w.strip_suffix(']'))
        .unwrap_or(&written)
        .to_string()
}

/// An array's elements held apart by their key and their `unique` fields: a key field each
/// element lacks, and a key or a unique field's value an element before it has already. Values
/// are compared by the text a path writes them in, so the number 53 and the text "53" are the same.
fn held_apart(inner: &[Field], items: &[Value], at: &str, out: &mut Findings) {
    let keys = key_fields(inner);
    let unique: Vec<&Field> = inner.iter().filter(|f| f.is_unique && !f.is_pk).collect();
    if keys.is_empty() && unique.is_empty() {
        return;
    }
    let text = |v: Option<&Value>| v.and_then(picks::Lit::of).map(|l| l.text);
    let mut seen_keys: HashMap<Vec<String>, usize> = HashMap::new();
    let mut seen_unique: HashMap<(String, String), usize> = HashMap::new();
    for (i, item) in items.iter().enumerate() {
        let place = format!("{at}[{i}]");
        if !keys.is_empty() {
            let mut tuple = Vec::new();
            for k in &keys {
                match text(item.get(&k.name)) {
                    Some(t) => tuple.push(t),
                    None => out.keys.push((
                        at.to_string(),
                        place.clone(),
                        keyed!("instances.keys.missing", name = k.name),
                    )),
                }
            }
            if tuple.len() == keys.len() {
                match seen_keys.get(&tuple) {
                    Some(first) => out.keys.push((
                        at.to_string(),
                        place.clone(),
                        keyed!(
                            "instances.keys.twice",
                            key = key_pick(&keys, item)
                                .map(|p| key_text(&p))
                                .unwrap_or_default(),
                            other = format!("{at}[{first}]")
                        ),
                    )),
                    None => {
                        seen_keys.insert(tuple, i);
                    }
                }
            }
        }
        for u in &unique {
            let Some(t) = text(item.get(&u.name)) else {
                continue;
            };
            match seen_unique.get(&(u.name.clone(), t.clone())) {
                Some(first) => out.keys.push((
                    at.to_string(),
                    joined(&place, &u.name),
                    keyed!(
                        "instances.keys.uniqueTwice",
                        name = u.name,
                        value = t,
                        other = format!("{at}[{first}]")
                    ),
                )),
                None => {
                    seen_unique.insert((u.name.clone(), t), i);
                }
            }
        }
    }
}

/// Why a value is not of a scalar type, if it is not.
fn scalar_misfit(kind: &str, given: &Value) -> Option<crate::text::Keyed> {
    let text = given.as_str();
    let ok = match kind {
        "int" => given.as_i64().is_some() || given.as_u64().is_some(),
        "float" => given.is_number(),
        "decimal" => text.is_some_and(is_decimal),
        "bool" => given.is_boolean(),
        "uuid" => text.is_some_and(is_uuid),
        "url" => text.is_some_and(is_url),
        "email" => text.is_some_and(is_email),
        "date" => text.is_some_and(is_date),
        "datetime" => text.is_some_and(is_datetime),
        "json" => true,
        _ => text.is_some(),
    };
    (!ok).then(|| match kind {
        "int" => keyed!("instances.wrong.notInt"),
        "float" => keyed!("instances.wrong.notNumber"),
        "decimal" => keyed!("instances.wrong.notDecimal"),
        "bool" => keyed!("instances.wrong.notBool"),
        "uuid" => keyed!("instances.wrong.notUuid"),
        "url" => keyed!("instances.wrong.notUrl"),
        "email" => keyed!("instances.wrong.notEmail"),
        "date" => keyed!("instances.wrong.notDate"),
        "datetime" => keyed!("instances.wrong.notDatetime"),
        _ => keyed!("instances.wrong.notString"),
    })
}

fn is_decimal(text: &str) -> bool {
    let body = text.strip_prefix('-').unwrap_or(text);
    let (whole, part) = body.split_once('.').unwrap_or((body, "0"));
    !whole.is_empty()
        && !part.is_empty()
        && whole.bytes().all(|b| b.is_ascii_digit())
        && part.bytes().all(|b| b.is_ascii_digit())
}

fn is_uuid(text: &str) -> bool {
    let groups: Vec<&str> = text.split('-').collect();
    groups.len() == 5
        && groups
            .iter()
            .zip([8, 4, 4, 4, 12])
            .all(|(g, n)| g.len() == n && g.bytes().all(|b| b.is_ascii_hexdigit()))
}

fn is_url(text: &str) -> bool {
    match text.split_once("://") {
        Some((scheme, rest)) => {
            !scheme.is_empty()
                && scheme
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"+-.".contains(&b))
                && scheme.as_bytes()[0].is_ascii_alphabetic()
                && !rest.is_empty()
                && !rest.contains(char::is_whitespace)
        }
        None => false,
    }
}

fn is_email(text: &str) -> bool {
    match text.split_once('@') {
        Some((local, domain)) => {
            !local.is_empty()
                && domain.contains('.')
                && !domain.starts_with('.')
                && !domain.ends_with('.')
                && !text.contains(char::is_whitespace)
                && !domain.contains('@')
        }
        None => false,
    }
}

fn is_date(text: &str) -> bool {
    let parts: Vec<&str> = text.split('-').collect();
    if parts.len() != 3
        || parts[0].len() != 4
        || parts[1].len() != 2
        || parts[2].len() != 2
        || !parts.iter().all(|p| p.bytes().all(|b| b.is_ascii_digit()))
    {
        return false;
    }
    let (y, m, d): (i32, u32, u32) = (
        parts[0].parse().unwrap_or(0),
        parts[1].parse().unwrap_or(0),
        parts[2].parse().unwrap_or(0),
    );
    let days = match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 => 29,
        2 => 28,
        _ => return false,
    };
    (1..=days).contains(&d)
}

fn is_datetime(text: &str) -> bool {
    let Some((date, time)) = text.split_once(['T', 't']) else {
        return false;
    };
    if !is_date(date) {
        return false;
    }
    let (clock, zone) = if let Some(c) = time.strip_suffix(['Z', 'z']) {
        (c, "")
    } else if let Some(i) = time.rfind(['+', '-']) {
        (&time[..i], &time[i..])
    } else {
        return false;
    };
    if !zone.is_empty() {
        let z = &zone[1..];
        let ok = z.len() == 5
            && z.as_bytes()[2] == b':'
            && z[..2].bytes().all(|b| b.is_ascii_digit())
            && z[3..].bytes().all(|b| b.is_ascii_digit());
        if !ok {
            return false;
        }
    }
    let (hms, fraction) = clock.split_once('.').unwrap_or((clock, "0"));
    let hms: Vec<&str> = hms.split(':').collect();
    hms.len() == 3
        && hms
            .iter()
            .zip([23u32, 59, 60])
            .all(|(p, top)| p.len() == 2 && p.parse::<u32>().is_ok_and(|n| n <= top))
        && !fraction.is_empty()
        && fraction.bytes().all(|b| b.is_ascii_digit())
}

// MARK: - Written

/// What a data card says once it is written, or why it is refused: a value written that its
/// field does not take, or whose field its shape does not have, is refused, said with its path;
/// a required field with no value is said, the card being written a field at a time (as a flow
/// is a node at a time). `written`: the values written, as nested as the data (none for every
/// one, as when the card is made) — a value written before, wrong since its shape changed, is
/// not refused when another is written.
pub(crate) fn said(
    project: &Project,
    card: &Card,
    written: Option<&Value>,
    pointer: Pointer,
) -> Result<Vec<String>, String> {
    let full = project.full(card);
    let Some(shape) = shape(project, card) else {
        return Err(text!("instances.said.ofNeeded", full));
    };
    let found = check(project, shape, card.instance_data());
    let counts = |at: &str| written.is_none_or(|w| present(w, at));
    let mut refused: Vec<String> = found
        .wrong
        .iter()
        .filter(|(at, _)| counts(at))
        .map(|(at, why)| format!("{}: {why}", place(&full, at)))
        .collect();
    refused.extend(
        found
            .strays
            .iter()
            .filter(|(at, _)| counts(at))
            .map(|(at, _)| text!("instances.said.noFieldInShape", full, at, name = shape.name)),
    );
    // An array's keys, whenever anything of the array is written: a duplicate made by changing
    // one element sits at the element after it.
    refused.extend(
        found
            .keys
            .iter()
            .filter(|(array, _, _)| counts(array))
            .map(|(_, at, why)| format!("{}: {why}", place(&full, at))),
    );
    if !refused.is_empty() {
        // A number written for a decimal is put right by writing it as the text; offered only when
        // that is all that is wrong, so the corrected call goes through.
        let fixes: Vec<Value> = found
            .wrong
            .iter()
            .filter(|(at, _)| counts(at))
            .filter_map(|(at, why)| decimal_fix(card.instance_data(), at, why, pointer))
            .collect();
        let beside = if fixes.len() == refused.len() {
            json!({ "fix": fixes })
        } else {
            Value::Null
        };
        return Err(crate::mcp::refusal(refused.join("\n"), beside));
    }
    Ok(found
        .missing
        .iter()
        .map(|at| text!("instances.said.required", full, at, name = shape.name))
        .collect())
}

/// Where a data value sits in the call that wrote it, for a fix to name (`crate::mcp::refusal`): in the
/// `data` of a card written whole, or inside the `value` of one value written, `skip` places down the
/// data before the value begins.
#[derive(Clone, Copy)]
pub(crate) enum Pointer {
    Data,
    Value { skip: usize },
}

/// The places a value is named by, the names and the numbers picking in an array.
fn segments(at: &str) -> Vec<String> {
    let mut out = Vec::new();
    for part in crate::pathtext::dots(at) {
        let (name, indexes) = crate::pathtext::step(part);
        out.push(name.to_string());
        out.extend(
            indexes
                .split(['[', ']'])
                .filter(|s| !s.is_empty())
                .map(str::to_string),
        );
    }
    out
}

/// The fix for a decimal written as a number: the same number written as the text, at its place in the
/// call. None for any other wrong value, and for a number that has no plain decimal text.
fn decimal_fix(
    data: Option<&Value>,
    at: &str,
    why: &crate::text::Keyed,
    pointer: Pointer,
) -> Option<Value> {
    if *why != keyed!("instances.wrong.notDecimal") {
        return None;
    }
    let places = segments(at);
    let mut here = data?;
    for place in &places {
        here = match here {
            Value::Object(m) => m.get(place)?,
            Value::Array(a) => a.get(place.parse::<usize>().ok()?)?,
            _ => return None,
        };
    }
    let Value::Number(number) = here else {
        return None;
    };
    let text = number.to_string();
    if !is_decimal(&text) {
        return None;
    }
    let (head, rest) = match pointer {
        Pointer::Data => ("/data", &places[..]),
        Pointer::Value { skip } => ("/value", places.get(skip..)?),
    };
    let mut to = head.to_string();
    for place in rest {
        to.push('/');
        to.push_str(&crate::mcp::pointer_key(place));
    }
    Some(json!({ "at": to, "to": text }))
}

fn place(full: &str, at: &str) -> String {
    if at.is_empty() {
        format!("{full}.data")
    } else {
        format!("{full}.{at}")
    }
}

/// Whether a place in the data (`lines[0].qty`) is among the values written.
fn present(written: &Value, at: &str) -> bool {
    if at.is_empty() {
        return !written.is_null();
    }
    let mut here = written;
    for part in crate::pathtext::dots(at) {
        let (name, indexes) = crate::pathtext::step(part);
        let Some(next) = here.as_object().and_then(|m| m.get(name.as_ref())) else {
            return false;
        };
        here = next;
        for index in indexes.split(['[', ']']).filter(|s| !s.is_empty()) {
            let Some(next) = index
                .parse::<usize>()
                .ok()
                .and_then(|i| here.as_array()?.get(i))
            else {
                return false;
            };
            here = next;
        }
    }
    true
}

/// The values written over a data card's (`data` is written whole), with the values it holds
/// whose fields its shape has lost put back: they go only when they are let go of
/// (`strayFields`). Those with no place to go back to (what held them written as no object)
/// are said.
pub(crate) fn with_strays(given: Value, strays: &[(String, Value)]) -> (Value, Vec<String>) {
    let mut out = given;
    let mut lost = Vec::new();
    for (at, value) in strays {
        let steps = picks::split(at);
        if value_at(Some(&out), &steps).is_some() {
            continue;
        }
        let holder = &steps[..steps.len() - 1];
        if !holder.is_empty() && !value_at(Some(&out), holder).is_some_and(Value::is_object) {
            lost.push(at.clone());
            continue;
        }
        out = put_at(Some(&out), &crate::pathtext::names(at), Some(value.clone()));
    }
    (out, lost)
}

/// What is wrong with a data card for the check of the tree (`inspection_list`'s `cardProblems`):
/// its shape gone or not one, values its fields do not take, required fields with no value.
pub(crate) fn problems(project: &Project, card: &Card) -> Vec<(String, crate::text::Keyed)> {
    let Some(shape) = shape(project, card) else {
        return vec![(String::new(), keyed!("instances.problem.noShape"))];
    };
    let found = check(project, shape, card.instance_data());
    found
        .wrong
        .into_iter()
        .chain(found.keys.into_iter().map(|(_, at, why)| (at, why)))
        .chain(
            found
                .missing
                .into_iter()
                .map(|at| (at, keyed!("instances.problem.required", name = shape.name))),
        )
        .collect()
}

/// The `pattern` rules data is held to that the check cannot read, so holds nothing to: of the
/// fields of the shapes data cards have and of what those embed, each once — where
/// (`Schemas:Host.name`), the regex, and why it cannot be read.
pub(crate) fn unread_patterns(project: &Project) -> Vec<(String, String, String)> {
    let mut shapes: Vec<&Card> = Vec::new();
    for card in project.detail.schemas.iter() {
        if card.kind == INSTANCE
            && let Some(s) = shape(project, card)
            && !shapes.iter().any(|c| c.id == s.id)
        {
            shapes.push(s);
        }
    }
    let mut out = Vec::new();
    let mut i = 0;
    while i < shapes.len() {
        let card = shapes[i];
        i += 1;
        let mut fields: Vec<(String, &Field)> = card
            .fields
            .iter()
            .map(|f| (crate::pathtext::quote(&f.name).into_owned(), f))
            .collect();
        while let Some((at, field)) = fields.pop() {
            for written in patterns(field) {
                if let Err(why) = regex::Regex::new(written) {
                    let why = why.to_string();
                    let last = why
                        .lines()
                        .rfind(|l| l.trim_start().starts_with("error:"))
                        .unwrap_or(&why);
                    let last = last.trim();
                    out.push((
                        format!("{}.{at}", project.full(card)),
                        written.to_string(),
                        last.strip_prefix("error:")
                            .unwrap_or(last)
                            .trim()
                            .to_string(),
                    ));
                }
            }
            if field.is_object() {
                fields.extend(
                    field
                        .children
                        .iter()
                        .map(|c| (crate::pathtext::member(&at, &c.name), c)),
                );
            } else if field.is_ref()
                && let Some(target) = field
                    .ref_schema_id
                    .as_deref()
                    .and_then(|id| project.card(id))
                && (target.kind == "entity" || target.kind == "value")
                && !shapes.iter().any(|c| c.id == target.id)
            {
                shapes.push(target);
            }
        }
    }
    out.sort();
    out
}

/// A data card's values whose fields its shape no longer has: where, and the value.
pub(crate) fn strays(project: &Project, card: &Card) -> Vec<(String, Value)> {
    match shape(project, card) {
        Some(shape) => check(project, shape, card.instance_data()).strays,
        None => Vec::new(),
    }
}

// MARK: - Its shape, a line

/// The lines a card is made with: those written (`connections`), and a data card's shape
/// (`of`) as a line to it — `of` is not written among the lines, nor on another kind. None
/// when there are none.
pub(crate) fn lines_made(kind: &str, value: &Map<String, Value>) -> Result<Option<Value>, String> {
    let written = value.get("connections");
    no_of_line(written)?;
    let of = value.get("of").filter(|v| !v.is_null());
    let table = kind == crate::kinds::TABLE;
    if kind != INSTANCE && !table {
        if of.is_some() {
            return Err(text!("instances.of.onlyOnData", kind));
        }
        return Ok(written.cloned());
    }
    if let Some(data) = value.get("data").filter(|_| !table) {
        data_shape(data)?;
    }
    let Some(Value::String(of)) = of else {
        return Err(if table {
            text!("instances.of.tableNeeded")
        } else {
            text!("instances.of.instanceNeeded")
        });
    };
    let mut lines = match written {
        Some(Value::Array(items)) => items.clone(),
        Some(_) => return Err(text!("instances.of.connectionsNotArray")),
        None => Vec::new(),
    };
    lines.push(serde_json::json!({"line": OF_LINE, "to": of}));
    Ok(Some(Value::Array(lines)))
}

/// A data card's lines with its shape's line in them: to `of` when it is written (the one there
/// let go of), else the one it has, kept when the lines are written whole without it.
pub(crate) fn shape_line(doc: &Map<String, Value>, whole: &mut Value, of: Option<&str>) {
    let Value::Array(lines) = whole else {
        return;
    };
    let is_of = |l: &Value| l.get("line").and_then(Value::as_str) == Some(OF_LINE);
    match of {
        Some(of) => {
            lines.retain(|l| !is_of(l));
            lines.push(serde_json::json!({"line": OF_LINE, "to": of}));
        }
        None if !lines.iter().any(is_of) => {
            if let Some(there) = doc
                .get("connections")
                .and_then(Value::as_array)
                .and_then(|l| l.iter().find(|l| is_of(l)))
            {
                lines.push(there.clone());
            }
        }
        None => {}
    }
}

/// A data card's values whose fields its shape has lost, let go of (`strayFields`): every one
/// (`null`) or those named (`{"<where>": null}`). Returns where they were.
pub(crate) fn drop_strays(
    project: &Project,
    card: &Card,
    doc: &mut Map<String, Value>,
    raw: &Value,
) -> Result<Vec<String>, String> {
    let there = strays(project, card);
    let which: Vec<String> = match raw {
        Value::Null => there.iter().map(|(at, _)| at.clone()).collect(),
        Value::Object(named) => {
            if let Some((key, _)) = named.iter().find(|(_, v)| !v.is_null()) {
                return Err(text!("instances.strays.onlyDelete", key));
            }
            named.keys().cloned().collect()
        }
        _ => {
            return Err(text!("instances.strays.shape"));
        }
    };
    if let Some(missing) = which.iter().find(|w| !there.iter().any(|(at, _)| at == *w)) {
        return Err(if there.is_empty() {
            text!("instances.strays.noValueNone", name = card.name, missing)
        } else {
            text!(
                "instances.strays.noValueAmong",
                name = card.name,
                missing,
                places = crate::text::List::of(there.iter().map(|(at, _)| at.as_str()))
            )
        });
    }
    let mut data = doc
        .get("fields")
        .and_then(|f| f.get("data"))
        .cloned()
        .unwrap_or(Value::Null);
    for at in &which {
        data = put_at(Some(&data), &crate::pathtext::names(at), None);
    }
    if let Some(Value::Object(fields)) = doc.get_mut("fields") {
        fields.insert("data".into(), data);
    }
    Ok(which)
}

/// Refused: the line `of` written among the lines (a data card's shape is its key `of`).
pub(crate) fn no_of_line(written: Option<&Value>) -> Result<(), String> {
    let mut lines: Vec<&Value> = Vec::new();
    match written {
        Some(Value::Array(items)) => lines.extend(items),
        Some(Value::Object(edits)) => {
            for key in ["add", "remove"] {
                if let Some(Value::Array(items)) = edits.get(key) {
                    lines.extend(items);
                }
            }
        }
        _ => {}
    }
    if lines
        .iter()
        .any(|l| l.get("line").and_then(Value::as_str) == Some(OF_LINE))
    {
        return Err(text!("instances.of.notALine"));
    }
    Ok(())
}

/// Refused: values written as anything but an object (`data` is as nested as its shape).
pub(crate) fn data_shape(data: &Value) -> Result<(), String> {
    if data.is_object() || data.is_null() {
        return Ok(());
    }
    Err(text!("instances.of.dataShape"))
}

// MARK: - A value by its path

/// Why the names of a data card's path lead nowhere in its shape: at which name, and why.
#[derive(Debug)]
pub(crate) enum ValueMiss {
    /// A name the shape does not have there: the names there are.
    Name(usize, Vec<String>),
    /// Into an array with no element picked (`ports.number`; `ports[0].number` picks one).
    InArray(usize),
    /// A pick written wrong, or one its array's elements cannot be picked by: why.
    Pick(usize, String),
}

/// The fields what a field holds has: an object's own, what a `ref` embeds (none for an enum's
/// values, or a scalar).
pub(crate) fn inner_fields<'a>(project: &Project<'a>, field: &'a Field) -> &'a [Field] {
    if field.is_ref() {
        match field
            .ref_schema_id
            .as_deref()
            .and_then(|id| project.card(id))
        {
            Some(target) if target.kind != "enum" => &target.fields,
            _ => &[],
        }
    } else {
        &field.children
    }
}

/// Whether a field holds one text, number or truth (an enum's value too): what a pick may name.
pub(crate) fn scalar(project: &Project, field: &Field) -> bool {
    !field.is_array && !field.is_object() && inner_fields(project, field).is_empty()
}

/// A pick held to what its array's elements are: by fields' values, elements with fields — and fields of theirs that
/// hold a text, a number or a truth; by their own value, elements without; by place, any.
pub(crate) fn held(
    project: &Project,
    pick: &Pick,
    base: &str,
    inner: &[Field],
) -> Result<(), String> {
    let names = || crate::text::List::of(inner.iter().map(|f| f.name.as_str()));
    match pick {
        Pick::At(_) => Ok(()),
        Pick::Is(_) if inner.is_empty() => Ok(()),
        Pick::Is(_) => Err(text!("instances.held.hasFields", base, names = names())),
        Pick::Where(_) if inner.is_empty() => Err(text!("instances.held.bareValues", base)),
        Pick::Where(keys) => {
            for (key, _) in keys {
                match inner.iter().find(|f| &f.name == key) {
                    None => {
                        return Err(text!("instances.held.noField", base, key, names = names()));
                    }
                    Some(f) if !scalar(project, f) => {
                        return Err(text!("instances.held.notScalar", base, key));
                    }
                    Some(_) => {}
                }
            }
            Ok(())
        }
    }
}

/// The fields a name of a data card's path is among, after the names before it: the shape's,
/// an object field's, what a `ref` embeds — past an array, its element's, the element picked
/// (`ports[number=8200,protocol=tcp].number`, `ports[0]`). Refused at an
/// array passed with no pick, at a pick written wrong and at a name the shape does not have.
/// The fields passed are returned, outermost first (a path follows them through a rename).
pub(crate) fn field_at<'a>(
    project: &Project<'a>,
    shape: &'a Card,
    names: &[String],
) -> Result<Vec<&'a Field>, ValueMiss> {
    let mut list: &'a [Field] = &shape.fields;
    let mut found: Vec<&'a Field> = Vec::new();
    let mut open = false;
    for (i, name) in names.iter().enumerate() {
        if open {
            return Err(ValueMiss::InArray(i));
        }
        let (base, pick) = picks::step(name).map_err(|why| ValueMiss::Pick(i, why))?;
        let Some(field) = list.iter().find(|f| f.name == base) else {
            return Err(ValueMiss::Name(
                i,
                list.iter().map(|f| f.name.clone()).collect(),
            ));
        };
        let inner = inner_fields(project, field);
        if let Some(pick) = &pick {
            if !field.is_array {
                return Err(ValueMiss::Pick(i, text!("instances.pick.notArray", base)));
            }
            held(project, pick, &base, inner).map_err(|why| ValueMiss::Pick(i, why))?;
        }
        found.push(field);
        open = field.is_array && pick.is_none();
        list = inner;
    }
    if found.is_empty() {
        return Err(ValueMiss::Name(
            0,
            list.iter().map(|f| f.name.clone()).collect(),
        ));
    }
    Ok(found)
}

/// How each name of a data card's path picks an element, a pick by fields' values holding its
/// fields by id (`fields`: the fields the names pass, as `field_at` gives them).
pub(crate) fn picks_by_id(
    project: &Project,
    fields: &[&Field],
    names: &[String],
) -> Vec<Option<Pick>> {
    names
        .iter()
        .zip(fields)
        .map(|(name, field)| {
            let (_, pick) = picks::step(name).ok()?;
            Some(match pick? {
                Pick::Where(keys) => {
                    let inner = inner_fields(project, field);
                    Pick::Where(
                        keys.into_iter()
                            .map(|(k, lit)| {
                                let id = inner
                                    .iter()
                                    .find(|f| f.name == k)
                                    .map_or(k, |f| f.id.clone());
                                (id, lit)
                            })
                            .collect(),
                    )
                }
                other => other,
            })
        })
        .collect()
}

/// A pick held by ids written with its fields' names (`fields`: every field's name by id).
pub(crate) fn pick_named(pick: &Pick, fields: &HashMap<String, String>) -> String {
    match pick {
        Pick::Where(keys) => Pick::Where(
            keys.iter()
                .map(|(id, lit)| {
                    (
                        fields.get(id).cloned().unwrap_or_else(|| id.clone()),
                        lit.clone(),
                    )
                })
                .collect(),
        )
        .written(),
        other => other.written(),
    }
}

/// A place in a data card as the screen draws its rows: each element picked named by its place (`ports[0].number`), and a
/// pick that finds no one element the array's own row. Any other place, as it is.
pub(crate) fn drawn_at(card: &Card, path: &str) -> String {
    if card.kind != INSTANCE || !picks::picks(path) {
        return path.to_string();
    }
    let names = picks::split(path);
    let placed = |steps: &[(String, Option<usize>)]| {
        steps
            .iter()
            .map(|(k, at)| match at {
                Some(i) => format!("{}[{i}]", crate::pathtext::quote(k)),
                None => crate::pathtext::quote(k).into_owned(),
            })
            .collect::<Vec<_>>()
    };
    match walked(card.instance_data(), &names) {
        Ok(steps) => placed(&steps).join("."),
        // Up to the array whose pick finds none, which the names before it lead to.
        Err((i, _)) => {
            let mut out = walked(card.instance_data(), &names[..i])
                .map(|s| placed(&s))
                .unwrap_or_default();
            out.push(picks::step(&names[i]).map_or_else(
                |_| names[i].clone(),
                |(b, _)| crate::pathtext::quote(&b).into_owned(),
            ));
            out.join(".")
        }
    }
}

/// Whether the last name of a path picks an element (its value is the element, not the array).
pub(crate) fn picks_last(names: &[String]) -> bool {
    names
        .last()
        .is_some_and(|n| matches!(picks::step(n), Ok((_, Some(_)))))
}

/// A path's names in the data: each a key, and the place of the element picked in its array.
pub(crate) type Walked = Vec<(String, Option<usize>)>;

/// The names of a path in the data, each array's element picked to its place — or, where a pick
/// finds no one element, which name and why. An array not written is an empty one.
pub(crate) fn walked(data: Option<&Value>, names: &[String]) -> Result<Walked, (usize, String)> {
    let mut here = data;
    let mut out = Vec::new();
    for (i, name) in names.iter().enumerate() {
        let (base, pick) = picks::step(name).map_err(|why| (i, why))?;
        here = here.and_then(Value::as_object).and_then(|m| m.get(&*base));
        let mut at = None;
        if let Some(pick) = pick {
            let items: &[Value] = here.and_then(Value::as_array).map_or(&[], Vec::as_slice);
            match pick.find(items) {
                Found::One(k) => {
                    at = Some(k);
                    here = items.get(k);
                }
                found => {
                    let mut array: Vec<String> = names[..i].to_vec();
                    array.push(crate::pathtext::quote(&base).into_owned());
                    let why = picks::not_one(&found, &array.join(".")).unwrap_or_default();
                    return Err((i, why));
                }
            }
        }
        out.push((base.to_string(), at));
    }
    Ok(out)
}

/// The value at a path in the data: none when it is not written, or when a pick on the way
/// finds no one element.
pub(crate) fn value_at<'v>(data: Option<&'v Value>, names: &[String]) -> Option<&'v Value> {
    let mut here = data?;
    for name in names {
        let (base, pick) = picks::step(name).ok()?;
        here = here.as_object()?.get(&*base)?;
        if let Some(pick) = pick {
            let items = here.as_array()?;
            match pick.find(items) {
                Found::One(k) => here = &items[k],
                _ => return None,
            }
        }
    }
    Some(here)
}

/// The data with a value put at a path walked to its places (objects made on the way), or taken
/// away (`None`): a key out of its object, an element out of its array.
fn put_walked(
    data: Option<&Value>,
    steps: &[(String, Option<usize>)],
    value: Option<Value>,
) -> Value {
    fn put(here: &mut Value, steps: &[(String, Option<usize>)], value: Option<Value>) {
        if !here.is_object() {
            *here = Value::Object(Map::new());
        }
        let Value::Object(map) = here else {
            return;
        };
        let ((key, at), rest) = (&steps[0], &steps[1..]);
        match at {
            None if rest.is_empty() => match value {
                Some(v) => {
                    map.insert(key.clone(), v);
                }
                None => {
                    map.shift_remove(key);
                }
            },
            None => put(
                map.entry(key.clone())
                    .or_insert_with(|| Value::Object(Map::new())),
                rest,
                value,
            ),
            Some(k) => {
                let Some(Value::Array(items)) = map.get_mut(key) else {
                    return;
                };
                if *k >= items.len() {
                    return;
                }
                if rest.is_empty() {
                    match value {
                        Some(v) => items[*k] = v,
                        None => {
                            items.remove(*k);
                        }
                    }
                } else {
                    put(&mut items[*k], rest, value);
                }
            }
        }
    }
    let mut out = match data {
        Some(v @ Value::Object(_)) => v.clone(),
        _ => Value::Object(Map::new()),
    };
    if !steps.is_empty() {
        put(&mut out, steps, value);
    }
    out
}

/// A value alone at a path walked to its places, as `present` reads what was written: an array
/// on the way holds it at its place, nothing (null) before it.
fn sparse(steps: &[(String, Option<usize>)], value: Value) -> Value {
    let mut here = value;
    for (key, at) in steps.iter().rev() {
        if let Some(k) = at {
            let mut items = vec![Value::Null; *k];
            items.push(here);
            here = Value::Array(items);
        }
        let mut map = Map::new();
        map.insert(key.clone(), here);
        here = Value::Object(map);
    }
    here
}

/// The data with a value put at a path (objects made on the way), or taken away (`None`).
pub(crate) fn put_at(data: Option<&Value>, names: &[String], value: Option<Value>) -> Value {
    fn put(here: &mut Map<String, Value>, names: &[String], value: Option<Value>) {
        let (first, rest) = (&names[0], &names[1..]);
        if rest.is_empty() {
            match value {
                Some(v) => {
                    here.insert(first.clone(), v);
                }
                None => {
                    here.shift_remove(first);
                }
            }
            return;
        }
        let inner = here
            .entry(first.clone())
            .or_insert_with(|| Value::Object(Map::new()));
        if !inner.is_object() {
            *inner = Value::Object(Map::new());
        }
        if let Value::Object(map) = inner {
            put(map, rest, value);
        }
    }
    let mut out = match data {
        Some(Value::Object(m)) => m.clone(),
        _ => Map::new(),
    };
    if !names.is_empty() {
        put(&mut out, names, value);
    }
    Value::Object(out)
}

/// One value of a data card written (`value`) or taken away (none), by its names (`card_update`
/// and `card_delete` on a value's path): what is written held to its field, what else is there
/// left as it is. Inside an array, the element picked: written, or taken out of its array — the lines that pick it follow
/// (`follow_picks`). Answers the value as `card_read` reads it, at its path as it is now.
pub(crate) fn write_value(
    tx: &mut Tx,
    scope: &Scope,
    id: &str,
    names: &[String],
    value: Option<Value>,
) -> Result<Value, String> {
    let (full, mut docs) = {
        let seen = Seen::in_tx(tx, scope)?;
        let project = seen.project();
        let card = project
            .card(id)
            .ok_or_else(|| text!("instances.write.noCard"))?;
        (project.full(card), scope.docs(tx)?)
    };
    let mut doc = docs
        .remove(id)
        .ok_or_else(|| text!("instances.write.noCard"))?;
    let at = format!("{full}.{}", names.join("."));
    let data = doc.get("fields").and_then(|f| f.get("data")).cloned();
    let steps = walked(data.as_ref(), names)
        .map_err(|(i, why)| format!("{full}.{}: {why}", names[..=i].join(".")))?;
    if value.is_none() && value_at(data.as_ref(), names).is_none() {
        return Err(text!("instances.write.noValue", at));
    }
    // An element taken out: the lines that leave it, or anything inside it, go with it; those of
    // other cards that land on it are said to point nowhere now.
    let element_removed = value.is_none() && picks_last(names);
    let broken_before = if element_removed {
        Some(Seen::in_tx(tx, scope)?.broken_keys())
    } else {
        None
    };
    let mut lines_removed: Vec<Value> = Vec::new();
    if element_removed && let Some(Value::Array(lines)) = doc.get_mut("connections") {
        let seen = Seen::in_tx(tx, scope)?;
        let project = seen.project();
        lines.retain(|line| {
            let leaves = line
                .get("from")
                .and_then(Value::as_str)
                .filter(|f| !f.is_empty())
                .and_then(|f| walked(data.as_ref(), &picks::split(f)).ok())
                .is_some_and(|from| from.len() >= steps.len() && from[..steps.len()] == steps[..]);
            if leaves {
                lines_removed.push(line_read(&project, line));
            }
            !leaves
        });
        if lines.is_empty() {
            doc.shift_remove("connections");
        }
    }
    // What holds it is an object, or not there yet (made on the way).
    for end in 1..names.len() {
        if let Some(there) = value_at(data.as_ref(), &names[..end])
            && !there.is_object()
        {
            return Err(text!(
                "instances.write.notObject",
                full,
                inner = names[..end].join(".")
            ));
        }
    }
    let next = put_walked(data.as_ref(), &steps, value.clone());
    // The names as they are now: a pick of the element written, by the values it has now.
    let now = picked_again(names, &steps, &next);
    let mut followed = Vec::new();
    let removed = value.is_none();
    if let Some(lines) = doc.get_mut("connections").and_then(Value::as_array_mut) {
        for line in lines.iter_mut().filter_map(Value::as_object_mut) {
            if let Some(said) = follow_picks(line, "from", data.as_ref(), &next, &steps, removed) {
                followed.push(text!("instances.write.followedFrom", full, said));
            }
        }
    }
    match doc
        .entry("fields")
        .or_insert_with(|| Value::Object(Map::new()))
    {
        Value::Object(fields) => {
            fields.insert("data".into(), next.clone());
        }
        _ => return Err(text!("instances.write.fieldsNotObject")),
    }
    super::update::put_checked(tx, scope, &doc)?;
    tx.line(
        &scope.project,
        &scope.tree,
        "schema.update",
        "schema",
        Some(id),
        if removed {
            keyed!("instances.line.valueRemoved", full, path = names.join("."))
        } else {
            keyed!("instances.line.valueWritten", full, path = names.join("."))
        },
    )?;
    // The lines of other cards that land on one of its elements (`at`).
    let mut others: Vec<(String, Map<String, Value>, Vec<String>)> = Vec::new();
    for (other, mut odoc) in docs {
        let mut said = Vec::new();
        if let Some(lines) = odoc.get_mut("connections").and_then(Value::as_array_mut) {
            for line in lines.iter_mut().filter_map(Value::as_object_mut) {
                let lands = line.get("to").and_then(Value::as_str) == Some(id)
                    && line
                        .get("at")
                        .and_then(Value::as_str)
                        .is_some_and(|a| !crate::pathtext::has_section(a));
                if lands
                    && let Some(now) =
                        follow_picks(line, "at", data.as_ref(), &next, &steps, removed)
                {
                    said.push(now);
                }
            }
        }
        if !said.is_empty() {
            others.push((other, odoc, said));
        }
    }
    others.sort_by(|a, b| a.0.cmp(&b.0));
    for (other, odoc, said) in others {
        super::update::put_checked(tx, scope, &odoc)?;
        let name = {
            let seen = Seen::in_tx(tx, scope)?;
            let project = seen.project();
            project
                .card(&other)
                .map(|c| project.full(c))
                .unwrap_or_default()
        };
        tx.line(
            &scope.project,
            &scope.tree,
            "schema.update",
            "schema",
            Some(&other),
            keyed!("instances.line.linesFollowed", name, full),
        )?;
        for now in said {
            followed.push(text!("instances.write.followedAt", name, now));
        }
    }
    let seen = Seen::in_tx(tx, scope)?;
    let project = seen.project();
    let card = project
        .card(id)
        .ok_or_else(|| text!("instances.write.noCard"))?;
    let written = match value {
        Some(v) => sparse(&steps, v),
        None => Value::Null,
    };
    let skip = steps
        .iter()
        .map(|(_, at)| 1 + usize::from(at.is_some()))
        .sum();
    let mut said = said(&project, card, Some(&written), Pointer::Value { skip })?;
    said.extend(followed);
    let answer = if removed { names } else { now.as_slice() };
    let mut view = seen.value_view(card, answer);
    if let Some(before) = broken_before {
        drop(project);
        drop(seen);
        let newly = super::update::newly_broken(tx, scope, &before, None)?;
        said.extend(newly.iter().map(super::update::broken_warning));
        if let Value::Object(out) = &mut view {
            if !lines_removed.is_empty() {
                out.insert("linesRemoved".into(), Value::Array(lines_removed));
            }
            out.insert("danglingRefs".into(), newly.len().into());
        }
    }
    Ok(super::warned(view, &said))
}

/// A line as `card_read` reads it: its kind, the row it leaves, the card it reaches and where.
fn line_read(project: &Project, line: &Value) -> Value {
    let mut out = Map::new();
    for key in ["line", "from"] {
        if let Some(said) = line.get(key) {
            out.insert(key.into(), said.clone());
        }
    }
    if let Some(to) = line.get("to").and_then(Value::as_str) {
        out.insert(
            "to".into(),
            project.named(to).map(Value::from).unwrap_or(Value::Null),
        );
    }
    if let Some(at) = line.get("at") {
        out.insert("at".into(), at.clone());
    }
    Value::Object(out)
}

/// A value a pick names, as its field holds it: a text quoted, or of a field that takes a text,
/// is that text; unquoted, a number of a field of numbers and a truth of a field of truths.
fn lit_value(field: &Field, lit: &picks::Lit) -> Value {
    if !lit.quoted {
        match field.kind.as_str() {
            "int" => {
                if let Ok(n) = lit.text.parse::<i64>() {
                    return n.into();
                }
            }
            "float" => {
                if let Some(n) = lit
                    .text
                    .parse::<f64>()
                    .ok()
                    .and_then(serde_json::Number::from_f64)
                {
                    return Value::Number(n);
                }
            }
            "bool" => {
                if let Ok(b) = lit.text.parse::<bool>() {
                    return b.into();
                }
            }
            _ => {}
        }
    }
    Value::from(lit.text.clone())
}

/// A line's place in a data card (`key`: its `from`, or its `at` in the card it reaches) that picks
/// an element of the array at `parent` + `base` by its place, at or after `at`: one place on, past
/// the element put in at `at`. Returns the place as it is now, when it changed.
fn follow_insert(
    line: &mut Map<String, Value>,
    key: &str,
    before: Option<&Value>,
    parent: &[(String, Option<usize>)],
    base: &str,
    at: usize,
) -> Option<String> {
    let raw = line.get(key).and_then(Value::as_str)?.to_string();
    if !picks::picks(&raw) {
        return None;
    }
    let names = picks::split(&raw);
    let j = parent.len();
    if names.len() <= j || walked(before, &names[..j]).ok()? != parent {
        return None;
    }
    let (b, pick) = picks::step(&names[j]).ok()?;
    let Some(Pick::At(i)) = pick else {
        return None;
    };
    if b != base || i < at {
        return None;
    }
    let mut out = names.clone();
    out[j] = format!("{}[{}]", crate::pathtext::quote(&b), i + 1);
    let now = out.join(".");
    line.insert(key.to_string(), now.clone().into());
    Some(now)
}

/// An element made in a data card's array at a path that names it (`card_create`; `value`: the
/// element's `value` and its `position`): by its key where the element's shape has one
/// (`ports[number=443,protocol=tcp]`, the key fields taken from the path), by the place it goes in
/// where it has none (`tags[1]`; the array's length puts it last), by its own value in an array of
/// values (`groups[=api]`). The array is not written whole, so what another write put in it since
/// it was read stays. Lines that pick an element after it by its place move one on with it.
pub(crate) fn create_element(
    tx: &mut Tx,
    scope: &Scope,
    path: &str,
    value: &Map<String, Value>,
) -> Result<Value, String> {
    let (id, full, names, element, insert_at, parent, base, said_path) = {
        let seen = Seen::in_tx(tx, scope)?;
        let project = seen.project();
        let (card, names) = match super::walk::locate(&project, path, None)? {
            super::walk::Target::CustomRow(card, names) if card.kind == INSTANCE => (card, names),
            _ => return Err(text!("instances.create.notElement", path)),
        };
        let full = project.full(card);
        let shape = shape(&project, card).ok_or_else(|| text!("instances.said.ofNeeded", full))?;
        let last = names.last().cloned().unwrap_or_default();
        let (base, pick) = picks::step(&last).map_err(|why| format!("{path}: {why}"))?;
        let base = base.into_owned();
        let Some(pick) = pick else {
            return Err(text!("instances.create.notElement", path));
        };
        let fields = field_at(&project, shape, &names)
            .map_err(|_| text!("instances.create.notElement", path))?;
        let field = fields[fields.len() - 1];
        let inner = inner_fields(&project, field);
        let keys = key_fields(inner);
        let data = card.instance_data();
        let parent = walked(data, &names[..names.len() - 1])
            .map_err(|(i, why)| format!("{full}.{}: {why}", names[..=i].join(".")))?;
        let array_at = format!(
            "{full}.{}",
            names[..names.len() - 1]
                .iter()
                .cloned()
                .chain([crate::pathtext::quote(&base).into_owned()])
                .collect::<Vec<_>>()
                .join(".")
        );
        let holder = value_at(data, &names[..names.len() - 1]);
        if holder.is_some_and(|h| !h.is_object()) {
            return Err(text!(
                "instances.write.notObject",
                full,
                inner = names[..names.len() - 1].join(".")
            ));
        }
        let items: Vec<Value> = holder
            .and_then(|h| h.get(&base))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        if holder
            .and_then(|h| h.get(&base))
            .is_some_and(|a| !a.is_array())
        {
            return Err(text!("instances.create.notArray", at = array_at));
        }
        let given = value.get("value");
        let position = match value.get("position") {
            None => None,
            Some(p) => Some(
                p.as_u64()
                    .map(|p| p as usize)
                    .filter(|p| *p <= items.len())
                    .ok_or_else(|| text!("instances.create.position", count = items.len()))?,
            ),
        };
        let keys_named = || crate::text::List::of(keys.iter().map(|k| k.name.as_str()));
        let (element, insert_at) = match &pick {
            Pick::Where(written) => {
                if keys.is_empty() {
                    return Err(text!("instances.create.unkeyed", at = array_at));
                }
                let mut wanted: Vec<&str> = keys.iter().map(|k| k.name.as_str()).collect();
                let mut said: Vec<&str> = written.iter().map(|(k, _)| k.as_str()).collect();
                wanted.sort_unstable();
                said.sort_unstable();
                if wanted != said {
                    return Err(text!(
                        "instances.create.keyNames",
                        at = array_at,
                        keys = keys_named()
                    ));
                }
                let mut element = match given {
                    None | Some(Value::Null) => Map::new(),
                    Some(Value::Object(o)) => o.clone(),
                    Some(_) => return Err(text!("instances.create.notObject", path)),
                };
                for (k, lit) in written {
                    let Some(f) = inner.iter().find(|f| &f.name == k) else {
                        continue;
                    };
                    match element.get(k) {
                        Some(there) if !lit.fits(there) => {
                            return Err(text!("instances.create.keyDiffers", key = k, path));
                        }
                        Some(_) => {}
                        None => {
                            element.insert(k.clone(), lit_value(f, lit));
                        }
                    }
                }
                (Value::Object(element), position.unwrap_or(items.len()))
            }
            Pick::At(i) => {
                if !keys.is_empty() {
                    return Err(text!(
                        "instances.create.keyed",
                        at = array_at,
                        keys = keys_named()
                    ));
                }
                if *i > items.len() {
                    return Err(text!("instances.create.position", count = items.len()));
                }
                if position.is_some_and(|p| p != *i) {
                    return Err(text!("instances.create.twoPlaces", path));
                }
                let element = match given {
                    Some(v) if !v.is_null() => v.clone(),
                    _ if !inner.is_empty() => Value::Object(Map::new()),
                    _ => return Err(text!("instances.create.valueNeeded", path)),
                };
                (element, *i)
            }
            Pick::Is(lit) => {
                let element = lit_value(field, lit);
                if let Some(v) = given.filter(|v| !v.is_null())
                    && !lit.fits(v)
                {
                    return Err(text!("instances.create.valueDiffers", path));
                }
                (element, position.unwrap_or(items.len()))
            }
        };
        if !matches!(pick, Pick::At(_)) && !matches!(pick.find(&items), Found::None) {
            return Err(text!(
                "instances.create.exists",
                path = format!("{array_at}{}", pick.written())
            ));
        }
        let said_path = match &pick {
            Pick::At(_) => format!("{array_at}[{insert_at}]"),
            other => format!("{array_at}{}", other.written()),
        };
        (
            card.id.clone(),
            full,
            names,
            element,
            insert_at,
            parent,
            base,
            said_path,
        )
    };
    let mut docs = scope.docs(tx)?;
    let mut doc = docs
        .remove(&id)
        .ok_or_else(|| text!("instances.write.noCard"))?;
    let data = doc.get("fields").and_then(|f| f.get("data")).cloned();
    let holder = &names[..names.len() - 1];
    let mut items: Vec<Value> = value_at(data.as_ref(), holder)
        .and_then(|h| h.get(&base))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    items.insert(insert_at, element.clone());
    let mut array_steps = parent.clone();
    array_steps.push((base.clone(), None));
    let next = put_walked(data.as_ref(), &array_steps, Some(Value::Array(items)));
    let mut followed = Vec::new();
    if let Some(lines) = doc.get_mut("connections").and_then(Value::as_array_mut) {
        for line in lines.iter_mut().filter_map(Value::as_object_mut) {
            if let Some(said) =
                follow_insert(line, "from", data.as_ref(), &parent, &base, insert_at)
            {
                followed.push(text!("instances.write.followedFrom", full, said));
            }
        }
    }
    match doc
        .entry("fields")
        .or_insert_with(|| Value::Object(Map::new()))
    {
        Value::Object(fields) => {
            fields.insert("data".into(), next);
        }
        _ => return Err(text!("instances.write.fieldsNotObject")),
    }
    super::update::put_checked(tx, scope, &doc)?;
    tx.line(
        &scope.project,
        &scope.tree,
        "schema.update",
        "schema",
        Some(&id),
        keyed!(
            "instances.line.elementAdded",
            full,
            path = said_path.clone()
        ),
    )?;
    // The lines of other cards that land on an element after it by its place.
    let mut others: Vec<(String, Map<String, Value>, Vec<String>)> = Vec::new();
    for (other, mut odoc) in docs {
        let mut said = Vec::new();
        if let Some(lines) = odoc.get_mut("connections").and_then(Value::as_array_mut) {
            for line in lines.iter_mut().filter_map(Value::as_object_mut) {
                let lands = line.get("to").and_then(Value::as_str) == Some(id.as_str())
                    && line
                        .get("at")
                        .and_then(Value::as_str)
                        .is_some_and(|a| !crate::pathtext::has_section(a));
                if lands
                    && let Some(now) =
                        follow_insert(line, "at", data.as_ref(), &parent, &base, insert_at)
                {
                    said.push(now);
                }
            }
        }
        if !said.is_empty() {
            others.push((other, odoc, said));
        }
    }
    others.sort_by(|a, b| a.0.cmp(&b.0));
    for (other, odoc, said) in others {
        super::update::put_checked(tx, scope, &odoc)?;
        let name = {
            let seen = Seen::in_tx(tx, scope)?;
            let project = seen.project();
            project
                .card(&other)
                .map(|c| project.full(c))
                .unwrap_or_default()
        };
        tx.line(
            &scope.project,
            &scope.tree,
            "schema.update",
            "schema",
            Some(&other),
            keyed!("instances.line.linesFollowed", name, full),
        )?;
        for now in said {
            followed.push(text!("instances.write.followedAt", name, now));
        }
    }
    let seen = Seen::in_tx(tx, scope)?;
    let project = seen.project();
    let card = project
        .card(&id)
        .ok_or_else(|| text!("instances.write.noCard"))?;
    let mut at_element = parent.clone();
    at_element.push((base, Some(insert_at)));
    let written = sparse(&at_element, element);
    let skip = at_element
        .iter()
        .map(|(_, at)| 1 + usize::from(at.is_some()))
        .sum();
    let mut said = said(&project, card, Some(&written), Pointer::Value { skip })?;
    said.extend(followed);
    let mut out = Map::new();
    out.insert("project".into(), scope.project_name.clone().into());
    out.insert("path".into(), said_path.into());
    Ok(super::warned(Value::Object(out), &said))
}

/// The names of a path written, each pick of an element on the way picking it by the values it
/// has now (a field of a pick written over: `ports[number=8201,protocol=tcp].number`).
fn picked_again(names: &[String], steps: &[(String, Option<usize>)], data: &Value) -> Vec<String> {
    let mut here = Some(data);
    let mut out = Vec::new();
    for (name, (key, at)) in names.iter().zip(steps) {
        here = here.and_then(|h| h.get(key));
        let Some(k) = at else {
            out.push(name.clone());
            continue;
        };
        here = here.and_then(|h| h.get(*k));
        match (picks::step(name), here) {
            (Ok((base, Some(pick))), Some(element))
                if !matches!(pick, Pick::At(_)) && !pick.fits(element) =>
            {
                out.push(format!(
                    "{}{}",
                    crate::pathtext::quote(&base),
                    pick.again(element).written()
                ))
            }
            _ => out.push(name.clone()),
        }
    }
    out
}

/// A line's place in a data card (`key`: its `from`, or its `at` in the card it reaches) that
/// picks an element, followed through a value written at `steps`: an element taken out moves the places after it in its array one
/// back (a place picking the element itself is left, and the check says it); a pick by values
/// of an element written over picks it by the values it has now. Data written whole is not
/// followed. Returns the place as it is now, when it changed.
fn follow_picks(
    line: &mut Map<String, Value>,
    key: &str,
    before: Option<&Value>,
    after: &Value,
    steps: &[(String, Option<usize>)],
    removed: bool,
) -> Option<String> {
    let raw = line.get(key).and_then(Value::as_str)?.to_string();
    if !picks::picks(&raw) {
        return None;
    }
    let names = picks::split(&raw);
    let mut here = before;
    let mut out: Vec<String> = Vec::new();
    let mut walked: Vec<(String, Option<usize>)> = Vec::new();
    let mut changed = false;
    for (j, name) in names.iter().enumerate() {
        let Ok((base, pick)) = picks::step(name) else {
            return None;
        };
        here = here.and_then(|h| h.get(&*base));
        let Some(pick) = pick else {
            walked.push((base.to_string(), None));
            out.push(name.clone());
            continue;
        };
        let items: &[Value] = here.and_then(Value::as_array).map_or(&[], Vec::as_slice);
        let Found::One(k) = pick.find(items) else {
            out.extend(names[j..].iter().cloned());
            break;
        };
        here = items.get(k);
        // The same array as the one written in?
        let same = steps.len() > j
            && steps[..j] == walked[..]
            && steps[j].0 == base
            && steps[j].1.is_some();
        walked.push((base.to_string(), Some(k)));
        if same && removed && steps.len() == j + 1 {
            let gone = steps[j].1.unwrap_or_default();
            match pick {
                Pick::At(a) if a > gone => {
                    out.push(format!("{}[{}]", crate::pathtext::quote(&base), a - 1));
                    changed = true;
                }
                _ => out.push(name.clone()),
            }
            out.extend(names[j + 1..].iter().cloned());
            break;
        }
        if same && !removed && steps[..=j] == walked[..] && !matches!(pick, Pick::At(_)) {
            // The element as it is now: at the same place, written over.
            let element = walked.iter().try_fold(after, |h, (k, at)| {
                let next = h.get(k)?;
                match at {
                    Some(i) => next.get(*i),
                    None => Some(next),
                }
            });
            if let Some(element) = element
                && !pick.fits(element)
            {
                out.push(format!(
                    "{}{}",
                    crate::pathtext::quote(&base),
                    pick.again(element).written()
                ));
                changed = true;
                continue;
            }
        }
        out.push(name.clone());
    }
    if !changed {
        return None;
    }
    let now = out.join(".");
    line.insert(key.to_string(), now.clone().into());
    Some(now)
}

/// How a field's type is said beside its value (the screen's badge, a document's table): its
/// type, or the name of what a `ref` embeds; `[]` after it for an array.
pub(crate) fn type_label(project: &Project, field: &Field) -> String {
    let base = if field.is_ref() {
        field
            .ref_schema_id
            .as_deref()
            .and_then(|id| project.card(id))
            .map_or_else(|| "ref".to_string(), |c| c.name.clone())
    } else {
        field.kind.clone()
    };
    if field.is_array {
        format!("{base}[]")
    } else {
        base
    }
}

/// A data card's values with those whose fields its shape has lost taken out (they are said
/// apart, `strayFields`).
pub(crate) fn without_strays(data: Option<&Value>, strays: &[(String, Value)]) -> Value {
    let mut out = data.cloned().unwrap_or_else(|| Value::Object(Map::new()));
    for (at, _) in strays {
        out = put_at(Some(&out), &crate::pathtext::names(at), None);
    }
    out
}

// MARK: - Followed through a rename

/// Each value of a data card's data whose field takes the enum `enum_id`, with where it is in the
/// data (`lines[0].status`): `each` may write it over. Read by the shape's fields as `rekeyed`
/// reads them; a key no field names is passed by.
pub(crate) fn enum_values_mut(
    project: &Project,
    shape: &Card,
    data: &mut Value,
    enum_id: &str,
    each: &mut dyn FnMut(&str, &mut Value),
) {
    fn object(
        project: &Project,
        fields: &[Field],
        value: &mut Value,
        at: &str,
        enum_id: &str,
        depth: usize,
        each: &mut dyn FnMut(&str, &mut Value),
    ) {
        let Value::Object(map) = value else {
            return;
        };
        if depth > DEPTH {
            return;
        }
        for (key, given) in map.iter_mut() {
            let Some(field) = fields.iter().find(|f| &f.name == key) else {
                continue;
            };
            let place = joined(at, key);
            match given {
                Value::Array(items) if field.is_array => {
                    for (i, item) in items.iter_mut().enumerate() {
                        inside(
                            project,
                            field,
                            item,
                            &format!("{place}[{i}]"),
                            enum_id,
                            depth,
                            each,
                        );
                    }
                }
                v => inside(project, field, v, &place, enum_id, depth, each),
            }
        }
    }
    fn inside(
        project: &Project,
        field: &Field,
        value: &mut Value,
        at: &str,
        enum_id: &str,
        depth: usize,
        each: &mut dyn FnMut(&str, &mut Value),
    ) {
        if field.is_object() {
            return object(
                project,
                &field.children,
                value,
                at,
                enum_id,
                depth + 1,
                each,
            );
        }
        let Some(target) = field
            .ref_schema_id
            .as_deref()
            .filter(|_| field.is_ref())
            .and_then(|id| project.card(id))
        else {
            return;
        };
        if target.kind != "enum" {
            object(project, &target.fields, value, at, enum_id, depth + 1, each);
        } else if target.id == enum_id && value.is_string() {
            each(at, value);
        }
    }
    object(project, &shape.fields, data, "", enum_id, 0, each);
}

/// Where the data cards of the tree hold the value `name` of the enum `enum_id`: each card's id and
/// the full paths of its values that hold it (`Definitions:Order2041.status`).
pub(crate) fn holding_enum_value(
    project: &Project,
    enum_id: &str,
    name: &str,
) -> Vec<(String, Vec<String>)> {
    let mut out = Vec::new();
    for card in project.detail.schemas.iter().filter(|c| c.kind == INSTANCE) {
        let (Some(shape), Some(data)) = (shape(project, card), card.instance_data()) else {
            continue;
        };
        let full = project.full(card);
        let mut data = data.clone();
        let mut found = Vec::new();
        enum_values_mut(project, shape, &mut data, enum_id, &mut |at, value| {
            if value.as_str() == Some(name) {
                found.push(place(&full, at));
            }
        });
        if !found.is_empty() {
            out.push((card.id.clone(), found));
        }
    }
    out
}

/// A data card's values with each key named as its field is now: read against the shape as it
/// was (`before`), the fields known by their ids, a field renamed giving its key the new name.
/// A key whose field is gone, or none read, stays as it is (a stray). With how many keys took a
/// new name; none when none did.
pub(crate) fn rekeyed(
    before: &Project,
    shape_before: &Card,
    data: &Value,
    now: &HashMap<String, String>,
) -> Option<(Value, usize)> {
    fn object(
        before: &Project,
        fields: &[Field],
        value: &Value,
        now: &HashMap<String, String>,
        depth: usize,
        changed: &mut usize,
    ) -> Value {
        let Value::Object(map) = value else {
            return value.clone();
        };
        if depth > DEPTH {
            return value.clone();
        }
        let mut out = Map::new();
        for (key, given) in map {
            match fields.iter().find(|f| &f.name == key) {
                Some(field) => {
                    let name = now.get(&field.id).cloned().unwrap_or_else(|| key.clone());
                    if &name != key {
                        *changed += 1;
                    }
                    let inner = |v: &Value, changed: &mut usize| {
                        inside(before, field, v, now, depth, changed)
                    };
                    let value = match given {
                        Value::Array(items) if field.is_array => {
                            Value::Array(items.iter().map(|i| inner(i, changed)).collect())
                        }
                        v => inner(v, changed),
                    };
                    out.insert(name, value);
                }
                None => {
                    out.insert(key.clone(), given.clone());
                }
            }
        }
        Value::Object(out)
    }
    fn inside(
        before: &Project,
        field: &Field,
        value: &Value,
        now: &HashMap<String, String>,
        depth: usize,
        changed: &mut usize,
    ) -> Value {
        if field.is_object() {
            return object(before, &field.children, value, now, depth + 1, changed);
        }
        if field.is_ref()
            && let Some(target) = field
                .ref_schema_id
                .as_deref()
                .and_then(|id| before.card(id))
            && target.kind != "enum"
        {
            return object(before, &target.fields, value, now, depth + 1, changed);
        }
        value.clone()
    }
    let mut changed = 0;
    let out = object(before, &shape_before.fields, data, now, 0, &mut changed);
    (changed > 0).then_some((out, changed))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scalars_are_held_to_their_type() {
        assert!(scalar_misfit("int", &serde_json::json!(3)).is_none());
        assert!(scalar_misfit("int", &serde_json::json!(3.5)).is_some());
        assert!(scalar_misfit("decimal", &serde_json::json!("12.50")).is_none());
        assert!(scalar_misfit("decimal", &serde_json::json!(12.5)).is_some());
        assert!(
            scalar_misfit(
                "uuid",
                &serde_json::json!("5f0c1a2b-0000-4000-8000-00000000abcd")
            )
            .is_none()
        );
        assert!(scalar_misfit("uuid", &serde_json::json!("5f0c")).is_some());
        assert!(scalar_misfit("url", &serde_json::json!("https://example.com/a")).is_none());
        assert!(scalar_misfit("url", &serde_json::json!("example.com")).is_some());
        assert!(scalar_misfit("email", &serde_json::json!("a@example.com")).is_none());
        assert!(scalar_misfit("email", &serde_json::json!("a@b")).is_some());
        assert!(scalar_misfit("date", &serde_json::json!("2024-02-29")).is_none());
        assert!(scalar_misfit("date", &serde_json::json!("2023-02-29")).is_some());
        assert!(scalar_misfit("datetime", &serde_json::json!("2026-10-10T09:30:00Z")).is_none());
        assert!(
            scalar_misfit(
                "datetime",
                &serde_json::json!("2026-10-10T09:30:00.123+09:00")
            )
            .is_none()
        );
        assert!(scalar_misfit("datetime", &serde_json::json!("2026-10-10 09:30")).is_some());
        assert!(scalar_misfit("string", &serde_json::json!(1)).is_some());
        assert!(scalar_misfit("json", &serde_json::json!([1, {"a": null}])).is_none());
    }

    #[test]
    fn a_value_is_put_and_taken_at_a_path() {
        let data = serde_json::json!({"a": {"b": 1}, "c": 2});
        let names = |s: &str| s.split('.').map(String::from).collect::<Vec<_>>();
        assert_eq!(
            value_at(Some(&data), &names("a.b")),
            Some(&serde_json::json!(1))
        );
        let put = put_at(Some(&data), &names("a.d"), Some(serde_json::json!("x")));
        assert_eq!(put, serde_json::json!({"a": {"b": 1, "d": "x"}, "c": 2}));
        let taken = put_at(Some(&put), &names("c"), None);
        assert_eq!(taken, serde_json::json!({"a": {"b": 1, "d": "x"}}));
        let written = serde_json::json!({"lines": [{"qty": 1}], "a": {"b": null}});
        assert!(present(&written, "lines[0].qty"));
        assert!(!present(&written, "lines[1].qty"));
        assert!(present(&written, "a.b"));
        assert!(!present(&written, "c"));
        let (kept, lost) = with_strays(
            serde_json::json!({"a": {"b": 1}, "c": null}),
            &[
                ("old".to_string(), serde_json::json!(1)),
                ("a.old".to_string(), serde_json::json!(2)),
                ("c.old".to_string(), serde_json::json!(3)),
            ],
        );
        assert_eq!(
            kept,
            serde_json::json!({"a": {"b": 1, "old": 2}, "c": null, "old": 1})
        );
        assert_eq!(lost, vec!["c.old".to_string()]);
    }
}
