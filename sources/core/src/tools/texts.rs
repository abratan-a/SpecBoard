//! The words a card writes inside itself, place by place: what `card_list`'s `query` and
//! `card_read`'s `find` look through besides a card's own name, word and body. Each place
//! is named by the path of what it is in (the card, a formatdoc's row, a stackdoc's version)
//! and where in it (`heading`, `body`, `cells[2][1]`, `steps[0].description`), the two a
//! `card_update` that changes it is written with.

use crate::export::Project;
use crate::kinds::{Field, FieldType, Kinds};
use crate::model::Card;
use serde_json::Value;

/// One place a card writes words in.
pub(crate) struct Place {
    /// The full path of the card, row or version it is in.
    pub path: String,
    /// What is there: the row's kind, `version`, the card's kind.
    pub kind: String,
    /// Where in it.
    pub place: String,
    pub text: String,
}

/// The places a card writes words in besides its head (its name, label, description, body and
/// lines): a formatdoc's rows (an item's heading, a text's body, a table's headings and cells),
/// a stackdoc's versions (key, heading, body), and the text a card read by its kind's
/// definition writes in its fields and rows (a node's steps, a field of the project's own
/// kind). In the order they read, the places of one path together.
pub(crate) fn inside(project: &Project, card: &Card) -> Vec<Place> {
    let own = project.full(card);
    let mut out = Vec::new();
    if card.kind == "formatdoc" {
        super::facts::walk_format(&card.rows, &mut |row, names, _| {
            let path = format!(
                "{own}.{}",
                crate::pathtext::join_names(names.iter().map(String::as_str))
            );
            let mut put = |place: String, text: &str| {
                if !text.is_empty() {
                    out.push(Place {
                        path: path.clone(),
                        kind: row.kind.clone(),
                        place,
                        text: text.to_string(),
                    });
                }
            };
            if row.kind == "item" {
                put("heading".to_string(), &row.heading);
            }
            for (place, text) in super::facts::texts(row) {
                put(place, &text);
            }
        });
        return out;
    }
    if card.kind == "stackdoc" {
        for version in card.versions() {
            let path = crate::pathtext::member(&own, &version.name);
            for (place, text) in [
                ("key", &version.key),
                ("heading", &version.heading),
                ("body", &version.body),
            ] {
                if !text.is_empty() {
                    out.push(Place {
                        path: path.clone(),
                        kind: "version".to_string(),
                        place: place.to_string(),
                        text: text.clone(),
                    });
                }
            }
        }
        return out;
    }
    for (place, text) in form_words(project.defs.cards(), card) {
        out.push(Place {
            path: own.clone(),
            kind: card.kind.clone(),
            place,
            text,
        });
    }
    out
}

/// Everything a card says in words, for `find` over the whole of it: its own label,
/// description and body, the words of its lines, what it writes inside itself (`inside`), and
/// the same of each card it holds (a flow's nodes) after it.
pub(crate) fn whole(project: &Project, card: &Card) -> Vec<Place> {
    let mut out = Vec::new();
    let own = project.full(card);
    for (place, text) in [
        ("label", &card.label),
        ("description", &card.description),
        ("body", &card.body),
    ] {
        if !text.is_empty() {
            out.push(Place {
                path: own.clone(),
                kind: card.kind.clone(),
                place: place.to_string(),
                text: text.clone(),
            });
        }
    }
    for (index, line) in card.connections.iter().enumerate() {
        let mut said: Vec<(String, String)> = Vec::new();
        if let Some(description) = line.description.as_deref().filter(|d| !d.is_empty()) {
            said.push((
                format!("connections[{index}].description"),
                description.to_string(),
            ));
        }
        for (key, value) in &line.fields {
            if let Some(text) = value.as_str().filter(|t| !t.is_empty()) {
                said.push((
                    format!("connections[{index}].fields.{key}"),
                    text.to_string(),
                ));
            }
        }
        for (place, text) in said {
            out.push(Place {
                path: own.clone(),
                kind: card.kind.clone(),
                place,
                text,
            });
        }
    }
    out.extend(inside(project, card));
    for held in project.inner_of(&card.id) {
        out.extend(whole(project, held));
    }
    out
}

/// The text a card read by its kind's definition writes in its fields and rows, each with
/// where (`steps[0].description`, `return.program`): every text, long text and Markdown
/// field, in groups too. Its own label, description and body are the card's, read apart;
/// paths are the declared paths', not words.
fn form_words(kinds: &Kinds, card: &Card) -> Vec<(String, String)> {
    let Some(kind) = kinds.get(&card.kind) else {
        return Vec::new();
    };
    let Some(form) = card.form.as_ref() else {
        return Vec::new();
    };
    fn texts(value: &Value, at: &str, out: &mut Vec<(String, String)>) {
        match value {
            Value::String(text) if !text.is_empty() => out.push((at.to_string(), text.clone())),
            Value::Array(list) => {
                for (i, item) in list.iter().enumerate() {
                    texts(item, &format!("{at}[{i}]"), out);
                }
            }
            _ => {}
        }
    }
    fn fields(defs: &[Field], values: &Value, at: &str, out: &mut Vec<(String, String)>) {
        for def in defs {
            let Some(v) = values.get(&def.name) else {
                continue;
            };
            if at.is_empty()
                && ["name", "label", "description", "body"].contains(&def.name.as_str())
            {
                continue;
            }
            let here = if at.is_empty() {
                def.name.clone()
            } else {
                format!("{at}.{}", def.name)
            };
            match def.fieldtype {
                FieldType::Text | FieldType::LongText | FieldType::Markdown => {
                    texts(v, &here, out);
                }
                FieldType::Group => match v {
                    Value::Array(list) => {
                        for (i, item) in list.iter().enumerate() {
                            fields(&def.fields, item, &format!("{here}[{i}]"), out);
                        }
                    }
                    _ => fields(&def.fields, v, &here, out),
                },
                _ => {}
            }
        }
    }
    let mut out = Vec::new();
    fields(&kind.fields, &form["fields"], "", &mut out);
    for row in &kind.rows {
        for (i, item) in form["rows"][&row.name]
            .as_array()
            .into_iter()
            .flatten()
            .enumerate()
        {
            fields(
                &row.fields,
                &item["fields"],
                &format!("{}[{i}]", row.name),
                &mut out,
            );
        }
    }
    out
}
