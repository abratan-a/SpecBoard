//! What the window is told in its own words: a refusal or a hint that says what to do next names the place on
//! the screen to do it, not the MCP tool (`specboard://SpecBoard/Docs/Design:Localization` §3).

use crate::common;

use common::{PROJECT, op, refused, script_store};
use serde_json::json;
use specboard_core::Core;
use std::collections::HashMap;

/// The keys of the texts a person sees on the screen that say what to do next with an MCP tool.
const SEEN: [&str; 19] = [
    "actions.why.notMerged",
    "actions.why.requestOpenCancel",
    "check.ghost.none",
    "check.ghost.round",
    "check.line.noFile",
    "check.line.noPartner",
    "check.line.noShape",
    "check.line.noSymbol",
    "directory.problem.cardCount",
    "directory.problem.closeCards",
    "directory.problem.ghostEmpty",
    "create.card.exists",
    "files.execute.noDefault",
    "files.note.noDefault",
    "sections.error.hasGhosts",
    "sections.error.renameInBranch",
    "write.lockedMain",
    "inspection.ids.notFound",
    "inspection.unmark.none",
];

/// Whether `text` names an MCP tool.
fn names_a_tool(text: &str) -> bool {
    let tools = [
        "card",
        "section",
        "project",
        "kind",
        "branch",
        "export",
        "tag",
        "inspection",
        "mr",
        "app",
        "batch",
    ];
    let verbs = [
        "list", "read", "create", "update", "delete", "action", "merge", "execute", "undo",
        "revert", "accept", "control",
    ];
    tools
        .iter()
        .any(|t| verbs.iter().any(|v| text.contains(&format!("{t}_{v}"))))
}

/// Every text of a language's tables, by key.
fn table(language: &str) -> HashMap<String, String> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/text")
        .join(language);
    let mut out = HashMap::new();
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        let read: HashMap<String, String> =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        out.extend(read);
    }
    out
}

#[test]
fn a_screen_request_is_told_what_to_do_without_tool_names() {
    let core: Core = Core::open(script_store()).unwrap();
    let again = json!({"project": PROJECT, "target": "Schemas/Ordering:Order", "value": {"kind": "entity"}});
    let screen = op(
        &core,
        json!({"op": "tool", "name": "card_create", "arguments": again}),
    );
    let said = screen["error"].as_str().unwrap_or_default().to_string();
    assert!(said.contains("Order"), "{screen}");
    assert!(!names_a_tool(&said), "{said}");
    let mcp = refused(&core, "card_create", again);
    assert!(mcp.contains("card_update"), "{mcp}");
}

#[test]
fn every_text_the_screen_shows_has_its_screen_words() {
    for language in ["ja", "en"] {
        let texts = table(language);
        for key in SEEN {
            assert!(texts.contains_key(key), "{language}: {key}");
            let screen = format!("{key}.screen");
            let Some(said) = texts.get(&screen) else {
                panic!("{language}: {screen} is missing");
            };
            assert!(!names_a_tool(said), "{language}: {screen}: {said}");
        }
    }
}
