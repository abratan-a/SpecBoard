//! The cards' lines written as JSON (the export format `json-connections`;
//! `specboard://SpecBoard/Docs/Design:DesignSections` §7): `{"cards": [{card, kind, scenarios?,
//! connections}]}`, a card after another by its path in full. A card's `connections` are its lines
//! as `card_read` says them (`tools::connections_json`), in the order they were written, narrowed to
//! the kinds of line the entry names; `scenarios` are a spec's, each its name and heading, said
//! whether or not a line leaves it.
//!
//! It is read by tools that count lines without reaching the app, so it says the app's own paths —
//! what the other formats leave out —, nothing of when or by what it was written, and nothing that
//! a folder bound on one machine and not on another would change: the same cards are the same file.

use super::Project;
use super::settings::Entry;
use crate::model::Card;
use crate::ojson;
use serde_json::{Map, Value, json};

/// The file of `cards`' lines, narrowed as `entry` says (not at all with none).
pub fn build(project: &Project, cards: &[&Card], entry: Option<&Entry>) -> String {
    let mut said: Vec<(String, Value)> = cards
        .iter()
        .map(|card| {
            let path = project.full(card);
            let mut one = Map::new();
            one.insert("card".into(), path.clone().into());
            one.insert("kind".into(), card.kind.clone().into());
            if card.kind == "spec" {
                one.insert(
                    "scenarios".into(),
                    card.scenarios()
                        .iter()
                        .map(|s| json!({"name": s.name, "heading": s.heading}))
                        .collect(),
                );
            }
            let lines: Vec<Value> = crate::tools::connections_json(project, card)
                .as_array()
                .into_iter()
                .flatten()
                .filter(|line| {
                    entry.is_none_or(|e| e.says_line(line["line"].as_str().unwrap_or_default()))
                })
                .cloned()
                .collect();
            one.insert("connections".into(), lines.into());
            (path, Value::Object(one))
        })
        .collect();
    said.sort_by(|a, b| a.0.cmp(&b.0));
    let cards: Vec<Value> = said.into_iter().map(|(_, card)| card).collect();
    ojson::pretty(&json!({ "cards": cards })) + "\n"
}
