//! `[[…]]` that names a section (`[[Schemas/Ordering:]]`, its path with the `:` and nothing after
//! it) and `[[path|shown text]]`: checked, followed through a section's rename and move, stored in
//! full, answered by `as: exists` and written out. A section is written with its `:` as an embed's
//! is, so a card of the same name is never taken for it; the shown text is the writer's words,
//! which a rename leaves as they are.

use crate::common;

use common::{checked, ok, script_store};
use serde_json::{Value, json};
use specboard_core::Core;

fn core() -> Core {
    Core::open(script_store()).unwrap()
}

fn body(core: &Core, target: &str) -> String {
    ok(core, "card_read", json!({"target": target}))["body"]
        .as_str()
        .unwrap_or_default()
        .to_string()
}

fn guide(core: &Core, text: &str) {
    ok(
        core,
        "card_create",
        json!({"target": "Docs:Guide", "value": {"kind": "freedoc", "body": text}}),
    );
}

/// What the check says of `Docs:Guide`'s body: each broken path and why.
fn broken(core: &Core) -> Vec<(String, String)> {
    checked(core, "main")["brokenPaths"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|b| b["at"].as_str().is_some_and(|a| a.ends_with("Guide")))
        .map(|b| {
            (
                b["path"].as_str().unwrap().to_string(),
                b["hint"].as_str().unwrap_or_default().to_string(),
            )
        })
        .collect()
}

#[test]
fn a_section_is_named_with_its_colon_and_checked_as_one() {
    let core = core();
    guide(
        &core,
        "[[Schemas/Ordering:]] [[Schemas:|型]] [[Main:]] [[Schemas]] [[Nowhere:]] \
         [[Schemas:Customer|客]] [[Schemas:Nope|無い客]]",
    );
    let said = broken(&core);
    let paths: Vec<&str> = said.iter().map(|(p, _)| p.as_str()).collect();
    assert_eq!(paths, ["Schemas", "Nowhere:", "Schemas:Nope"], "{said:?}");
    // Not a card: the section of the name, written with its colon.
    assert!(said[0].1.contains("[[Schemas:]]"), "{said:?}");
    assert!(said[1].1.contains("Nowhere"), "{said:?}");
    // A broken link with shown text says its path, not its words.
    assert!(!said[2].1.contains("無い客"), "{said:?}");
}

#[test]
fn exists_answers_a_section_as_a_section_of_its_kind() {
    let core = core();
    let exists = ok(
        &core,
        "card_read",
        json!({"target": ["Schemas/Ordering:", "Nowhere:", "Schemas"], "value": {"as": "exists"}}),
    );
    let results = exists["results"].as_array().unwrap();
    assert_eq!(results[0]["resolved"], true, "{exists}");
    assert_eq!(results[0]["kind"], "section", "{exists}");
    assert_eq!(results[0]["type"], "schema", "{exists}");
    assert_eq!(results[1]["resolved"], false, "{exists}");
    assert_eq!(results[2]["resolved"], false, "{exists}");
    assert!(
        results[2]["hint"]
            .as_str()
            .unwrap()
            .contains("[[Schemas:]]"),
        "{exists}"
    );
}

#[test]
fn a_section_named_alone_follows_its_rename_and_its_move() {
    let core = core();
    // Written in any case: stored as the section's path is.
    guide(
        &core,
        "[[schemas/ordering:]] と [[Schemas/Ordering:|注文の型]] ![[Schemas/Ordering:]] [[Schemas/Ordering:Order|注文]]",
    );
    assert_eq!(
        body(&core, "Docs:Guide"),
        "[[Schemas/Ordering:]] と [[Schemas/Ordering:|注文の型]] ![[Schemas/Ordering:]] [[Schemas/Ordering:Order|注文]]"
    );
    ok(
        &core,
        "section_update",
        json!({"target": "Schemas/Ordering", "value": {"name": "Orders"}}),
    );
    assert_eq!(
        body(&core, "Docs:Guide"),
        "[[Schemas/Orders:]] と [[Schemas/Orders:|注文の型]] ![[Schemas/Orders:]] [[Schemas/Orders:Order|注文]]"
    );
    // A parent renamed moves the path of what is under it.
    ok(
        &core,
        "section_update",
        json!({"target": "Schemas", "value": {"name": "Types"}}),
    );
    assert_eq!(
        body(&core, "Docs:Guide"),
        "[[Types/Orders:]] と [[Types/Orders:|注文の型]] ![[Types/Orders:]] [[Types/Orders:Order|注文]]"
    );
    assert!(broken(&core).is_empty(), "{:?}", broken(&core));
    // Mentioned as written; a section is no box to draw a line to.
    let read = ok(&core, "card_read", json!({"target": "Docs:Guide"}));
    assert_eq!(
        read["mentions"],
        json!(["Types/Orders:", "Types/Orders:Order"]),
        "{read}"
    );
}

#[test]
fn a_formatdocs_diagram_target_follows_its_sections_rename() {
    let core = core();
    ok(
        &core,
        "card_create",
        json!({"target": "Docs:Model", "value": {"kind": "formatdoc", "rows": [
            {"kind": "diagram", "name": "types", "target": "Schemas/Ordering:"}
        ]}}),
    );
    ok(
        &core,
        "section_update",
        json!({"target": "Schemas/Ordering", "value": {"name": "Orders"}}),
    );
    let read = ok(&core, "card_read", json!({"target": "Docs:Model.types"}));
    assert_eq!(read["target"], "Schemas/Orders:", "{read}");
}

#[test]
fn shown_text_is_kept_through_a_cards_rename() {
    let core = core();
    guide(
        &core,
        "[[Schemas:Customer|お客さま]] と [[Schemas:Customer.name | 名前]]",
    );
    // Left as written while it names what it named.
    assert_eq!(
        body(&core, "Docs:Guide"),
        "[[Schemas:Customer|お客さま]] と [[Schemas:Customer.name | 名前]]"
    );
    ok(
        &core,
        "card_update",
        json!({"target": "Schemas:Customer", "value": {"name": "Client"}}),
    );
    assert_eq!(
        body(&core, "Docs:Guide"),
        "[[Schemas:Client|お客さま]] と [[Schemas:Client.name|名前]]"
    );
    // What is mentioned is the path, as written.
    let read = ok(&core, "card_read", json!({"target": "Docs:Guide"}));
    assert_eq!(
        read["mentions"],
        json!(["Schemas:Client", "Schemas:Client.name"]),
        "{read}"
    );
}

#[test]
fn a_board_card_names_mains_section() {
    let core = core();
    ok(
        &core,
        "card_create",
        json!({"branch": "_project_board_", "target": "Goals:Typed", "value": {"kind": "goal",
            "body": "[[Schemas/Ordering:]] を整える"}}),
    );
    let board = checked(&core, "_project_board_");
    assert!(board.get("brokenPaths").is_none(), "{board}");
    ok(
        &core,
        "section_update",
        json!({"target": "Schemas/Ordering", "value": {"name": "Orders"}}),
    );
    assert_eq!(
        ok(
            &core,
            "card_read",
            json!({"branch": "_project_board_", "target": "Goals:Typed"}),
        )["body"],
        "[[Schemas/Orders:]] を整える"
    );
}

/// The sample with a ui section `UI/Home`, a page of its own where an export writes `UI`.
fn with_ui(core: &Core) {
    let mut tree = ok(core, "project_read", json!({}))["projectTree"].clone();
    tree["main_sections"]
        .as_array_mut()
        .unwrap()
        .push(json!({"type": "ui", "name": "UI"}));
    ok(
        core,
        "project_update",
        json!({"value": {"project_tree": tree}}),
    );
    ok(core, "section_create", json!({"target": "UI/Home"}));
    ok(
        core,
        "card_create",
        json!({"target": "UI/Home:Title", "value": {"kind": "html", "body": "<h1>ホーム</h1>"}}),
    );
}

#[test]
fn a_document_writes_a_section_out_by_its_name_or_the_words_shown() {
    let core = core();
    with_ui(&core);
    let project = common::project_id(&core);
    let set = common::op(
        &core,
        json!({"op": "mcp_writes_set", "project": project, "group": "export", "state": "allow"}),
    );
    assert!(set.get("error").is_none(), "{set}");
    guide(
        &core,
        "[[UI/Home:]] [[UI/Home:|ホーム画面]] [[Schemas/Ordering:]] [[Schemas:Customer|お客さま]] \
         [[Schemas:Nope|無い]] [[Nowhere:]]",
    );
    ok(
        &core,
        "export_create",
        json!({"target": "site", "value": {"entries": [
            {"from": "UI", "to": "pages/"},
            {"from": "Docs:Guide", "to": "docs/Guide.md"}
        ]}}),
    );
    let folder = common::place("site");
    let read = ok(
        &core,
        "export_read",
        json!({"target": "site", "targetdir": folder.to_string_lossy(), "value": {"files": ["docs/Guide.md"]}}),
    );
    let text = read["files"][0]["text"].as_str().unwrap();
    assert!(
        text.contains(
            "[Home](../pages/UI/Home.html) [ホーム画面](../pages/UI/Home.html) Ordering お客さま ~~無い~~ ~~`Nowhere`~~"
        ),
        "{text}"
    );
}

#[test]
fn code_comments_say_a_link_by_its_shown_text_or_its_sections_name() {
    let core = core();
    ok(
        &core,
        "card_update",
        json!({"target": "Schemas:Customer", "value": {"description": "[[Schemas/Ordering:]] の [[Schemas/Ordering:Order|注文]] をする人"}}),
    );
    let read = ok(
        &core,
        "export_read",
        json!({"targetdir": "[default]", "target": "ts", "value": {"files": ["types.ts"]}}),
    );
    let ts = read["files"][0]["text"].as_str().unwrap();
    assert!(ts.contains("Ordering の 注文 をする人"), "{ts}");
    let read: Value = ok(
        &core,
        "export_read",
        json!({"targetdir": "[default]", "target": "schema", "value": {"files": ["types.schema.json"]}}),
    );
    let schema = read["files"][0]["text"].as_str().unwrap();
    assert!(schema.contains("Ordering の 注文 をする人"), "{schema}");
}
