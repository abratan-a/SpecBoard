//! What to write at each place that has a description (`WritingRules`): the project's own
//! rule, or the app's, a broader place's when it has none of its own.

use serde_json::{Map, Value};

/// A place to write, and the broader place whose rule it takes when it has none.
pub(crate) struct Place {
    pub name: String,
    pub broader: Option<&'static str>,
}

/// Every place, in the order they are listed.
pub(crate) fn places() -> Vec<Place> {
    let mut out = vec![
        Place {
            name: "project.description".into(),
            broader: None,
        },
        Place {
            name: "section.description".into(),
            broader: None,
        },
        Place {
            name: "card.description".into(),
            broader: None,
        },
    ];
    // A card ghost has nothing of its own to describe.
    for kind in super::cards::MADE_KINDS
        .into_iter()
        .filter(|k| *k != crate::detail::GHOST)
    {
        out.push(Place {
            name: format!("card.{kind}.description"),
            broader: Some("card.description"),
        });
    }
    for (name, broader) in [
        ("field.description", None),
        ("field.entity.description", Some("field.description")),
        ("field.value.description", Some("field.description")),
        ("step.description", None),
        ("node.description", None),
        ("note.body", None),
        ("document.body", None),
    ] {
        out.push(Place {
            name: name.into(),
            broader,
        });
    }
    out
}

/// The app's rule at a place, where it has one.
fn default_rule(place: &str) -> Option<String> {
    Some(match place {
        "project.description" => text!("rules.default.projectDescription"),
        "section.description" => text!("rules.default.sectionDescription"),
        "card.description" => text!("rules.default.cardDescription"),
        "field.description" => text!("rules.default.fieldDescription"),
        "step.description" => text!("rules.default.stepDescription"),
        "node.description" => text!("rules.default.nodeDescription"),
        "note.body" => text!("rules.default.noteBody"),
        "document.body" => text!("rules.default.documentBody"),
        _ => return None,
    })
}

/// The rule at a place: its text, whether it is the project's or the app's, and the place it
/// is set at (`WritingRules.rule`).
fn rule(place: &str, project: &Map<String, Value>) -> Option<(String, &'static str, String)> {
    let all = places();
    let mut chain = Vec::new();
    let mut at = Some(place.to_string());
    while let Some(name) = at {
        at = all
            .iter()
            .find(|p| p.name == name)
            .and_then(|p| p.broader)
            .map(str::to_string);
        chain.push(name);
    }
    for name in &chain {
        if let Some(text) = project
            .get(name)
            .and_then(Value::as_str)
            .filter(|t| !t.trim().is_empty())
        {
            return Some((text.to_string(), "project", name.clone()));
        }
    }
    for name in &chain {
        if let Some(text) = default_rule(name) {
            return Some((text, "app", name.clone()));
        }
    }
    None
}

/// One place's row: its rule unless a listed place takes a broader one's.
fn row(name: &str, project: &Map<String, Value>, listed: bool) -> Map<String, Value> {
    let mut out = Map::new();
    out.insert("place".into(), name.into());
    let Some((text, from, at)) = rule(name, project) else {
        return out;
    };
    if !(listed && at != name) {
        out.insert("rule".into(), text.into());
    }
    out.insert("from".into(), from.into());
    if at != name {
        out.insert("setAt".into(), at.into());
    }
    out
}

/// The rules as `project_read` with `as: rules` answers them: every place with a rule of its own, or
/// the one asked.
pub(crate) fn view(
    project_name: &str,
    rules: &Map<String, Value>,
    place: Option<&str>,
) -> Result<Value, String> {
    let mut out = Map::new();
    out.insert("project".into(), project_name.into());
    if let Some(place) = place {
        let all = places();
        if !all.iter().any(|p| p.name == place) {
            return Err(text!(
                "rules.place.unknown",
                place,
                places = all
                    .iter()
                    .map(|p| p.name.as_str())
                    .collect::<Vec<_>>()
                    .join(" / ")
            ));
        }
        out.extend(row(place, rules, false));
        return Ok(Value::Object(out));
    }
    // Only the places with a rule of their own: one that takes a broader
    // place's says so when it is asked for by `place`.
    out.insert(
        "places".into(),
        Value::Array(
            places()
                .iter()
                .map(|p| row(&p.name, rules, true))
                .filter(|r| r.contains_key("rule"))
                .map(Value::Object)
                .collect(),
        ),
    );
    Ok(Value::Object(out))
}
