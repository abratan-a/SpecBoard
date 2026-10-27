//! The elements of a data card's array, named in a path (`specboard://SpecBoard/Docs/Design:CardKinds`「データ（instance）」): by
//! the values of their fields (`ports[number=8200,protocol=tcp]`), by their own value
//! (`groups[=web]`), or by their place (`ports[0]`). A field's path keeps its `[]`, which picks no
//! element. A path's names are split on the dots outside a name's brackets and quotes, so a value
//! may hold a dot (`hosts[ip=192.0.2.1]`), and a key is a field's name, quoted as a path quotes it
//! (`ports['port no'=8200]`).

use crate::pathtext;
use serde_json::Value;
use std::borrow::Cow;

/// A value a pick holds a field to, as written: unquoted, a text's, a number's or a truth's
/// (`8200` is the number 8200 and the text "8200"); quoted (`"a,b"`), a text's only.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Lit {
    pub text: String,
    pub quoted: bool,
}

/// How an element of an array is picked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Pick {
    /// By its place, from 0.
    At(usize),
    /// By its fields' values, every one of them.
    Where(Vec<(String, Lit)>),
    /// By its own value (an array of texts, numbers, truths or an enum's values).
    Is(Lit),
}

/// What a pick finds in an array.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Found {
    One(usize),
    None,
    /// Every element it fits, two or more.
    Many(Vec<usize>),
    /// A place past the end: how many there are.
    Out(usize),
}

/// A path's names (what follows the card's) as written, split on the dots outside brackets and
/// quotes.
pub(crate) fn split(names: &str) -> Vec<String> {
    pathtext::dots(names)
        .into_iter()
        .map(str::to_string)
        .collect()
}

/// Whether a path picks an element of an array anywhere (more than a field's `[]`).
pub(crate) fn picks(path: &str) -> bool {
    split(path)
        .iter()
        .any(|name| matches!(step(name), Ok((_, Some(_)))))
}

/// A name of a path taken apart: the field's name (its quotes taken off), and how an element of it
/// is picked (none for a name with no brackets, or the field's `[]`).
pub(crate) fn step(name: &str) -> Result<(Cow<'_, str>, Option<Pick>), String> {
    step_kept(name).map_err(String::from)
}

/// `step`, why it cannot be read kept by its key: said later, in the language of whoever reads it.
pub(crate) fn step_kept(name: &str) -> Result<(Cow<'_, str>, Option<Pick>), crate::text::Keyed> {
    let name = name.trim_matches([' ', '\t']);
    let (base, rest) = pathtext::step(name);
    if rest.is_empty() {
        return Ok((base, None));
    }
    let Some(inner) = rest.strip_prefix('[').and_then(|r| r.strip_suffix(']')) else {
        return Err(keyed!("picks.step.unclosed", name));
    };
    let inner = inner.trim_matches([' ', '\t']);
    if inner.is_empty() {
        return Ok((base, None));
    }
    if inner.chars().all(|c| c.is_ascii_digit()) {
        return inner
            .parse::<usize>()
            .map(|i| (base, Some(Pick::At(i))))
            .map_err(|_| keyed!("picks.step.indexTooBig", name));
    }
    if let Some(value) = inner.strip_prefix('=') {
        return Ok((base, Some(Pick::Is(lit(name, value)?))));
    }
    let mut keys = Vec::new();
    for (key, value) in pathtext::pick_parts(inner) {
        let (Some(key), Some(value)) = (key, value) else {
            return Err(shape_said(name));
        };
        let key = key.trim_matches([' ', '\t']);
        if key.starts_with('\'') && pathtext::step(key).1.is_empty() {
            // A quoted key: a field's name as a path quotes it.
        } else if key.is_empty() || key.contains(['\'', '"', '[', ']']) {
            return Err(shape_said(name));
        }
        let key = pathtext::unquote(key).into_owned();
        if pathtext::name_problem(&key).is_some() {
            return Err(shape_said(name));
        }
        if keys.iter().any(|(k, _): &(String, Lit)| *k == key) {
            return Err(keyed!("picks.step.keyTwice", name, key));
        }
        let lit = lit(name, value)?;
        keys.push((key, lit));
    }
    Ok((base, Some(Pick::Where(keys))))
}

fn shape_said(name: &str) -> crate::text::Keyed {
    keyed!("picks.step.shape", name)
}

/// A value as written in a pick.
fn lit(name: &str, raw: &str) -> Result<Lit, crate::text::Keyed> {
    let raw = raw.trim_matches([' ', '\t']);
    let Some(inner) = raw.strip_prefix('"') else {
        if raw.contains('"') {
            return Err(keyed!("picks.lit.aroundValue", name));
        }
        return Ok(Lit {
            text: raw.to_string(),
            quoted: false,
        });
    };
    let mut text = String::new();
    let mut chars = inner.chars();
    loop {
        match chars.next() {
            None => return Err(keyed!("picks.lit.unclosed", name)),
            Some('\\') => match chars.next() {
                Some(c) => text.push(c),
                None => return Err(keyed!("picks.lit.unclosed", name)),
            },
            Some('"') => break,
            Some(c) => text.push(c),
        }
    }
    if !chars.as_str().trim_matches([' ', '\t']).is_empty() {
        return Err(keyed!("picks.lit.aroundValue", name));
    }
    Ok(Lit { text, quoted: true })
}

impl Lit {
    /// A field's value as a pick writes it; none for one that is not a text, a number or a truth.
    /// A text that would read as more than itself unquoted (a number's, a truth's, null) is quoted.
    pub(crate) fn of(value: &Value) -> Option<Lit> {
        let text = match value {
            Value::String(s) => s.clone(),
            Value::Number(n) => return Some(Lit::bare(n.to_string())),
            Value::Bool(b) => return Some(Lit::bare(b.to_string())),
            _ => return None,
        };
        let plain = !text.is_empty()
            && text.trim_matches([' ', '\t']) == text
            && !text.contains([',', '[', ']', '=', '"', '\\'])
            && text.parse::<f64>().is_err()
            && !["true", "false", "null"].contains(&text.as_str());
        Some(Lit {
            text,
            quoted: !plain,
        })
    }

    fn bare(text: String) -> Lit {
        Lit {
            text,
            quoted: false,
        }
    }

    /// Whether a value is this one.
    pub(crate) fn fits(&self, value: &Value) -> bool {
        match value {
            Value::String(s) => s == &self.text,
            _ if self.quoted => false,
            Value::Number(n) => {
                self.text == n.to_string()
                    || matches!((self.text.parse::<f64>(), n.as_f64()), (Ok(a), Some(b)) if a == b)
            }
            Value::Bool(b) => self.text == b.to_string(),
            _ => false,
        }
    }

    fn written(&self) -> String {
        if self.quoted {
            format!(
                "\"{}\"",
                self.text.replace('\\', "\\\\").replace('"', "\\\"")
            )
        } else {
            self.text.clone()
        }
    }
}

impl Pick {
    /// Whether an element is the one picked (a place picks by place, not by the element).
    pub(crate) fn fits(&self, element: &Value) -> bool {
        match self {
            Pick::At(_) => false,
            Pick::Is(lit) => lit.fits(element),
            Pick::Where(keys) => keys
                .iter()
                .all(|(k, lit)| element.get(k).is_some_and(|v| lit.fits(v))),
        }
    }

    /// The element of `items` it picks.
    pub(crate) fn find(&self, items: &[Value]) -> Found {
        if let Pick::At(i) = self {
            return if *i < items.len() {
                Found::One(*i)
            } else {
                Found::Out(items.len())
            };
        }
        let hits: Vec<usize> = (0..items.len()).filter(|&i| self.fits(&items[i])).collect();
        match hits.len() {
            0 => Found::None,
            1 => Found::One(hits[0]),
            _ => Found::Many(hits),
        }
    }

    /// As it is written after a field's name.
    pub(crate) fn written(&self) -> String {
        match self {
            Pick::At(i) => format!("[{i}]"),
            Pick::Is(lit) => format!("[={}]", lit.written()),
            Pick::Where(keys) => format!(
                "[{}]",
                keys.iter()
                    .map(|(k, lit)| format!("{}={}", pathtext::quote_key(k), lit.written()))
                    .collect::<Vec<_>>()
                    .join(",")
            ),
        }
    }

    /// The same pick of `element` with each of its keys' values as `element` has them (a value
    /// it no longer has, or no text, number or truth, left as it was).
    pub(crate) fn again(&self, element: &Value) -> Pick {
        match self {
            Pick::At(_) => self.clone(),
            Pick::Is(lit) => Pick::Is(Lit::of(element).unwrap_or_else(|| lit.clone())),
            Pick::Where(keys) => Pick::Where(
                keys.iter()
                    .map(|(k, lit)| {
                        let now = element.get(k).and_then(Lit::of);
                        (k.clone(), now.unwrap_or_else(|| lit.clone()))
                    })
                    .collect(),
            ),
        }
    }
}

/// Why a pick finds no one element: none fits, two or more do (their places), or its place is
/// past the end. `at`: the array's path as written before the pick.
pub(crate) fn not_one(found: &Found, at: &str) -> Option<String> {
    match found {
        Found::One(_) => None,
        Found::None => Some(text!("picks.notOne.none")),
        Found::Many(hits) => Some(text!(
            "picks.notOne.many",
            count = hits.len(),
            places = crate::text::List::of(hits.iter().map(|i| format!("{at}[{i}]")))
        )),
        Found::Out(len) => Some(text!("picks.notOne.out", at, len)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_path_is_split_outside_its_picks() {
        assert_eq!(split("a.b[]"), ["a", "b[]"]);
        assert_eq!(
            split("hosts[ip=192.0.2.1].name"),
            ["hosts[ip=192.0.2.1]", "name"]
        );
        assert_eq!(split(r#"x[n="a.]b"].y"#), [r#"x[n="a.]b"]"#, "y"]);
        assert_eq!(split("'a.b'.c['d.e'=1]"), ["'a.b'", "c['d.e'=1]"]);
        assert!(picks("ports[0]"));
        assert!(!picks("lines[].sku"));
    }

    #[test]
    fn a_pick_is_read_and_written_as_it_was() {
        assert_eq!(step("ports").unwrap(), ("ports".into(), None));
        assert_eq!(step("ports[]").unwrap(), ("ports".into(), None));
        assert_eq!(
            step("ports[2]").unwrap(),
            ("ports".into(), Some(Pick::At(2)))
        );
        assert_eq!(
            step("'port list'[2]").unwrap(),
            ("port list".into(), Some(Pick::At(2)))
        );
        let (_, pick) = step("ports['port no'=8200,e-mail=a]").unwrap();
        assert_eq!(pick.unwrap().written(), "['port no'=8200,e-mail=a]");
        let (_, pick) = step("ports[number=8200, protocol=tcp]").unwrap();
        assert_eq!(pick.unwrap().written(), "[number=8200,protocol=tcp]");
        let (_, pick) = step(r#"tags[="a,b"]"#).unwrap();
        assert_eq!(pick.unwrap().written(), r#"[="a,b"]"#);
        for wrong in ["p[number]", "p[=x", "p['a=2]", r#"p[n="x]"#, "p[n=1,n=2]"] {
            assert!(step(wrong).is_err(), "{wrong}");
        }
    }

    #[test]
    fn a_pick_finds_one_element_or_says_why_not() {
        let ports = [
            json!({"number": 8200, "protocol": "tcp"}),
            json!({"number": 53, "protocol": "tcp"}),
            json!({"number": 53, "protocol": "udp"}),
        ];
        let pick = |s: &str| step(s).unwrap().1.unwrap();
        assert_eq!(
            pick("p[number=53,protocol=udp]").find(&ports),
            Found::One(2)
        );
        assert_eq!(pick("p[number=53]").find(&ports), Found::Many(vec![1, 2]));
        assert_eq!(pick("p[number=9]").find(&ports), Found::None);
        assert_eq!(pick("p[5]").find(&ports), Found::Out(3));
        // Unquoted, a number's and a text's; quoted, a text's only.
        assert!(pick("p[n=8200]").fits(&json!({"n": "8200"})));
        assert!(!pick(r#"p[n="8200"]"#).fits(&json!({"n": 8200})));
        assert!(pick("g[=web]").fits(&json!("web")));
        assert_eq!(
            not_one(&Found::Many(vec![1, 2]), "ports").unwrap(),
            "2 つに当たります（ports[1]・ports[2]）"
        );
        // Written again with the values an element has now.
        let again =
            pick("p[number=8200,protocol=tcp]").again(&json!({"number": 8201, "protocol": "tcp"}));
        assert_eq!(again.written(), "[number=8201,protocol=tcp]");
        assert_eq!(Lit::of(&json!("8200")).unwrap().written(), "\"8200\"");
    }
}
