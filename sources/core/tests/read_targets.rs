//! Reads that reach what an agent would otherwise rebuild by hand: one item of a formatdoc as
//! Markdown, character for character its part of the document's Markdown; `card_list` finding a
//! data card by a value it holds, and giving each card listed its rev, so a card found is written
//! without reading it again; a card ghost read with its own name beside what it points at.

use crate::common;

use common::{matches, ok, script_store};
use serde_json::{Value, json};
use specboard_core::Core;

/// An `as: markdown` answer: what it says of the text, and the Markdown.
fn markdown(core: &Core, target: &str) -> (Value, String) {
    let said = ok(
        core,
        "card_read",
        json!({"target": target, "value": {"as": "markdown"}}),
    );
    let text = said["markdown"]
        .as_str()
        .unwrap_or_else(|| panic!("{said}"))
        .to_string();
    (said, text)
}

#[test]
fn an_item_reads_as_its_part_of_the_documents_markdown() {
    let core = Core::open(script_store()).unwrap();
    // An item inside an item, so the depth of its heading is the document's too.
    ok(
        &core,
        "card_create",
        json!({"target": "Docs:Spec.tables.more", "value": {"kind": "item", "heading": "もっと",
            "rows": [{"name": "note", "kind": "text", "body": "中の本文"}]}}),
    );
    let (_, whole) = markdown(&core, "Docs:Spec");
    let rev = ok(&core, "card_read", json!({"target": "Docs:Spec"}))["rev"].clone();
    let (said, item) = markdown(&core, "Docs:Spec.tables");
    assert_eq!(said["branch"], "main");
    assert_eq!(said["path"], "Docs:Spec.tables");
    assert_eq!(said["rev"], rev);
    assert_eq!(said["lineCount"], item.split('\n').count());
    assert!(item.starts_with("## 表\n<!-- row tables -->"), "{item}");
    assert!(whole.contains(&item), "{whole}\n---\n{item}");
    // It runs to the end of its rows: the next item of the document is not in it.
    let next = whole.find("## カード").unwrap();
    assert!(whole[..next].trim_end().ends_with(&item), "{item}");
    let (_, inner) = markdown(&core, "Docs:Spec.tables.more");
    assert_eq!(inner, "### もっと\n<!-- row tables.more -->\n\n中の本文");
    assert!(whole.contains(&inner));
}

/// The sample project with a data card's section `Data` and an order in it.
fn with_order() -> Core {
    let core = Core::open(script_store()).unwrap();
    let read = ok(&core, "project_read", json!({}));
    let mut tree = read["projectTree"].clone();
    tree["main_sections"]
        .as_array_mut()
        .unwrap()
        .push(json!({"type": "definition", "name": "Data"}));
    ok(
        &core,
        "project_update",
        json!({"value": {"project_tree": tree}}),
    );
    ok(
        &core,
        "card_create",
        json!({"target": "Data:Order2041", "value": {"kind": "instance",
            "of": "Schemas/Ordering:Order",
            "data": {"id": "7d1e2f3a-4b5c-4d6e-8f90-a1b2c3d4e5f6", "total": "880",
                "status": "shipped", "lines": [{"sku": "C-3", "qty": 1}], "kind": "order"}}}),
    );
    core
}

/// What a query found, each place with the card it is in (`common::matches`).
fn hits(core: &Core, query: &str) -> Vec<Value> {
    matches(&ok(core, "card_list", json!({"value": {"query": query}})))
}

#[test]
fn a_data_card_is_found_by_a_value_it_holds() {
    let core = with_order();
    let found = hits(&core, "shipped");
    let data: Vec<&Value> = found
        .iter()
        .filter(|h| h["card"] == "Data:Order2041")
        .collect();
    assert_eq!(data.len(), 1, "{found:?}");
    assert_eq!(data[0]["at"], "Data:Order2041");
    assert_eq!(data[0]["kind"], "instance");
    assert_eq!(data[0]["where"], "data.status");
    // The enum that has the value is still found as before.
    assert!(
        found
            .iter()
            .any(|h| h["card"] == "Schemas:Status" && h["where"] == "values.shipped"),
        "{found:?}"
    );
    let nested = hits(&core, "C-3");
    assert_eq!(nested.len(), 1, "{nested:?}");
    assert_eq!(nested[0]["where"], "data.lines[0].sku");
}

#[test]
fn each_card_listed_or_found_has_its_rev() {
    let core = Core::open(script_store()).unwrap();
    let rev = |path: &str| ok(&core, "card_read", json!({"target": path}))["rev"].clone();
    let listed = ok(
        &core,
        "card_list",
        json!({"value": {"section": "Schemas", "recursive": true}}),
    );
    let cards = listed["cards"].as_array().unwrap();
    assert!(!cards.is_empty());
    for card in cards {
        assert_eq!(card["rev"], rev(card["path"].as_str().unwrap()), "{card}");
    }
    // A place found in a card gives the card's rev.
    for hit in hits(&core, "total") {
        let path = hit["at"].as_str().unwrap();
        let card = ok(&core, "card_read", json!({"target": path}));
        assert_eq!(hit["rev"], card["rev"], "{hit}");
    }
}

#[test]
fn a_ghost_is_read_with_its_own_name() {
    let core = Core::open(script_store()).unwrap();
    ok(
        &core,
        "card_create",
        json!({"target": "Schemas/Ordering:Buyer", "value": {"kind": "ghost", "target": "Schemas:Customer"}}),
    );
    let read = ok(
        &core,
        "card_read",
        json!({"target": "Schemas/Ordering:Buyer"}),
    );
    assert_eq!(read["name"], "Buyer");
    assert_eq!(read["target"], "Schemas:Customer");
}

#[test]
fn an_enums_value_is_found_by_its_description() {
    let core = Core::open(script_store()).unwrap();
    ok(
        &core,
        "card_update",
        json!({"target": "Schemas:Status.paid", "value": {"description": "代金を受け取った"}}),
    );
    let found = hits(&core, "代金を受け");
    assert!(
        found
            .iter()
            .any(|h| h["card"] == "Schemas:Status" && h["where"] == "values.paid"),
        "{found:?}"
    );
}

#[test]
fn a_flow_says_the_writings_that_mention_it() {
    let core = Core::open(script_store()).unwrap();
    ok(
        &core,
        "card_create",
        json!({"target": "Free:Links", "value": {"kind": "note", "body": "払うのは [[Charts/Shop:Checkout]]"}}),
    );
    let read = ok(
        &core,
        "card_read",
        json!({"target": ["Charts/Shop:Checkout"], "value": {"include": ["used_by"]}}),
    );
    let mentioned = read["mentionedBy"].as_array().cloned().unwrap_or_default();
    assert!(
        mentioned.iter().any(|m| m["note"] == "Free:Links"),
        "{read}"
    );
}

#[test]
fn the_rules_for_a_body_say_how_to_name_a_card_in_words_of_ones_own() {
    let core = Core::open(script_store()).unwrap();
    for place in ["note.body", "document.body", "card.entity.description"] {
        let read = ok(
            &core,
            "project_read",
            json!({"value": {"as": "rules", "place": place}}),
        );
        assert!(read.to_string().contains("[[パス|字]]"), "{place}: {read}");
    }
}

#[test]
fn a_field_and_a_value_say_what_uses_them_and_what_lands_on_them_when_asked() {
    let core = Core::open(script_store()).unwrap();
    let read = ok(
        &core,
        "card_read",
        json!({"target": ["Schemas/Ordering:Order.total"], "value": {"include": ["used_by", "ref_by"]}}),
    );
    let used = read["usedBy"].as_array().cloned().unwrap_or_default();
    assert!(
        used.iter().any(|u| u["flow"] == "Charts/Shop:Checkout"),
        "{read}"
    );
    let landed = read["referencedBy"].as_array().cloned().unwrap_or_default();
    assert!(!landed.is_empty(), "{read}");
    // An enum's value is not followed through the flows: asked by name, refused with where to look.
    let (failed, said) = common::call(
        &core,
        "card_read",
        json!({"project": "見本", "target": ["Schemas:Status.paid"], "value": {"include": ["used_by"]}}),
    );
    assert!(
        failed && said.contains("card_list") && said.contains("paid"),
        "{said}"
    );
}

#[test]
fn a_path_written_into_a_fields_a_sections_or_the_projects_description_that_does_not_resolve_is_warned()
 {
    let core = Core::open(script_store()).unwrap();
    let written = [
        ok(
            &core,
            "card_update",
            json!({"target": "Schemas:Customer.name", "value": {"description": "[[Nope:Field]] を見る"}}),
        ),
        ok(
            &core,
            "section_update",
            json!({"target": "Schemas", "value": {"description": "[[Nope:Section]] を見る"}}),
        ),
        ok(
            &core,
            "project_update",
            json!({"value": {"description": "[[Nope:Project]] を見る"}}),
        ),
    ];
    for (said, path) in written
        .iter()
        .zip(["Nope:Field", "Nope:Section", "Nope:Project"])
    {
        let warnings = said["warnings"].to_string();
        assert!(warnings.contains(path), "{path}: {said}");
    }
}
