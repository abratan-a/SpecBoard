//! The built-in definitions' words in the other languages (`specboard://SpecBoard/Docs/Design:Localization`
//! §4): `builtin.json` is written in Japanese, and each other language has a file `text/<language>.json`
//! of its words by their places in the definitions (`cards.entity.label`,
//! `cards.entity.fields.name.description`, `lines.peer.label`, `cards.goal.fields.status.optionLabels.完了`).
//! The definitions are read in the language of the call, so everything said of a kind (its label, what
//! it is for, its fields' labels) is said in that language. A choice's values are data a card keeps,
//! never changed: a language says how each is shown (`optionLabels`).

use crate::text::{self, LANGUAGES, Over};
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::sync::OnceLock;

// The other languages' files, by `build.rs`: `OVERLAYS`.
include!(concat!(env!("OUT_DIR"), "/kinds_overlays.rs"));

/// Where a text is in the definitions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum At {
    /// A text, by its JSON pointer.
    Text(String),
    /// How a choice's value is shown: the field's JSON pointer, and the value.
    Option { field: String, value: String },
}

/// A text of the definitions a language says in its own words: its place's name, the Japanese
/// written there, and where it is.
#[derive(Debug, Clone)]
pub(crate) struct Place {
    pub name: String,
    pub japanese: String,
    pub at: At,
}

/// Every text of `root` (`builtin.json`'s form) a person or an AI reads, with its place: a kind's,
/// section's, line's and action's label, description and context; each field's (a group's too) and row's
/// label and description; and each value of a choice written in Japanese.
pub(crate) fn places(root: &Value) -> Vec<Place> {
    let mut out = Vec::new();
    for (list, key, head) in [
        ("kinds", "kind", "cards"),
        ("sections", "section", "sections"),
        ("lines", "line", "lines"),
        ("actions", "action", "actions"),
    ] {
        for (i, def) in root[list].as_array().into_iter().flatten().enumerate() {
            let Some(name) = def[key].as_str() else {
                continue;
            };
            let place = format!("{head}.{name}");
            let pointer = format!("/{list}/{i}");
            texts(def, &place, &pointer, &mut out);
            if let Some(context) = def["context"].as_object() {
                for (key, value) in context {
                    if let Some(said) = value.as_str() {
                        out.push(Place {
                            name: format!("{place}.context.{key}"),
                            japanese: said.to_string(),
                            at: At::Text(format!("{pointer}/context/{key}")),
                        });
                    }
                }
            }
            fields(def, &place, &pointer, &mut out);
            rows(def, &place, &pointer, &mut out);
        }
    }
    out
}

/// A definition's own `label` and `description`.
fn texts(def: &Value, place: &str, pointer: &str, out: &mut Vec<Place>) {
    for key in ["label", "description"] {
        if let Some(said) = def[key].as_str() {
            out.push(Place {
                name: format!("{place}.{key}"),
                japanese: said.to_string(),
                at: At::Text(format!("{pointer}/{key}")),
            });
        }
    }
}

fn fields(def: &Value, place: &str, pointer: &str, out: &mut Vec<Place>) {
    for (i, field) in def["fields"].as_array().into_iter().flatten().enumerate() {
        let Some(name) = field["name"].as_str() else {
            continue;
        };
        let place = format!("{place}.fields.{name}");
        let pointer = format!("{pointer}/fields/{i}");
        texts(field, &place, &pointer, out);
        let options: Vec<&str> = field["options"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect();
        // Values a card keeps in other words than Japanese are shown by the labels written beside
        // them, which a language says in its own words as it says any other text.
        let labels = field["optionLabels"].as_object();
        for (value, label) in labels.into_iter().flatten() {
            if let Some(said) = label.as_str() {
                out.push(Place {
                    name: format!("{place}.optionLabels.{value}"),
                    japanese: said.to_string(),
                    at: At::Text(format!(
                        "{pointer}/optionLabels/{}",
                        value.replace('~', "~0").replace('/', "~1")
                    )),
                });
            }
        }
        if labels.is_none() && options.iter().any(|o| text::has_japanese(o)) {
            for value in options {
                out.push(Place {
                    name: format!("{place}.optionLabels.{value}"),
                    japanese: value.to_string(),
                    at: At::Option {
                        field: pointer.clone(),
                        value: value.to_string(),
                    },
                });
            }
        }
        fields(field, &place, &pointer, out);
    }
}

fn rows(def: &Value, place: &str, pointer: &str, out: &mut Vec<Place>) {
    for (i, row) in def["rows"].as_array().into_iter().flatten().enumerate() {
        let Some(name) = row["name"].as_str() else {
            continue;
        };
        let place = format!("{place}.rows.{name}");
        let pointer = format!("{pointer}/rows/{i}");
        texts(row, &place, &pointer, out);
        fields(row, &place, &pointer, out);
        rows(row, &place, &pointer, out);
    }
}

/// The words of `lang` over the Japanese ones, by place; none for Japanese, and none for a language
/// with no file.
pub(crate) fn overs(lang: &str) -> Option<&'static HashMap<String, Over>> {
    static READ: [OnceLock<Option<HashMap<String, Over>>>; LANGUAGES.len()] =
        [const { OnceLock::new() }; LANGUAGES.len()];
    let index = LANGUAGES.iter().position(|l| *l == lang)?;
    READ[index]
        .get_or_init(|| {
            let (_, source) = OVERLAYS.iter().find(|(code, _)| *code == lang)?;
            let name = format!("kinds/text/{lang}.json");
            let parsed: Value =
                serde_json::from_str(source).unwrap_or_else(|e| panic!("{name} is not JSON: {e}"));
            Some(text::overs(&parsed, &name))
        })
        .as_ref()
}

/// The languages with a file of their own, by code.
#[cfg(test)]
pub(crate) fn languages() -> impl Iterator<Item = &'static str> {
    OVERLAYS.iter().map(|(code, _)| *code)
}

/// `root` in `lang`'s words: each text with words over it, and each choice whose values are shown in
/// other words given them as `optionLabels` (`{value: label}`).
pub(crate) fn said_in(mut root: Value, lang: &str) -> Value {
    let Some(overs) = overs(lang) else {
        return root;
    };
    for place in places(&root) {
        let Some(said) = text::over(overs, &place.name, &place.japanese) else {
            continue;
        };
        match &place.at {
            At::Text(pointer) => {
                if let Some(at) = root.pointer_mut(pointer) {
                    *at = Value::String(said.to_string());
                }
            }
            At::Option { field, value } => {
                if let Some(Value::Object(field)) = root.pointer_mut(field) {
                    let labels = field
                        .entry("optionLabels")
                        .or_insert_with(|| Value::Object(Map::new()));
                    if let Value::Object(labels) = labels {
                        labels.insert(value.clone(), Value::String(said.to_string()));
                    }
                }
            }
        }
    }
    root
}
