//! How values are written through MCP: tags added or taken away without the rest, an enum's values by
//! `name`, the boxes read in the shape `places` writes, a spelling that was replaced told where it was
//! replaced, a union of types held to its keys, and a decimal written as a number put right in the call.

use crate::common;

use common::{call, ok, refused, script_store};
use serde_json::{Value, json};
use specboard_core::Core;

fn core() -> Core {
    Core::open(script_store()).unwrap()
}

/// A call through MCP that must be refused: its answer's `error`, whole.
fn refusal(core: &Core, tool: &str, mut arguments: Value) -> Value {
    arguments["project"] = common::PROJECT.into();
    let (failed, text) = call(core, tool, arguments.clone());
    assert!(failed, "{tool} {arguments} が通った: {text}");
    let said: Value = serde_json::from_str(&text).unwrap();
    said["error"].clone()
}

/// The call an error says goes through: `use`, sent as it is.
fn sent(core: &Core, error: &Value) -> Value {
    let used = &error["use"];
    assert!(used["tool"].is_string(), "use が無い: {error}");
    let mut arguments = used["arguments"].clone();
    arguments["project"] = common::PROJECT.into();
    ok(core, used["tool"].as_str().unwrap(), arguments)
}

fn tags_of(core: &Core, path: &str) -> Vec<String> {
    ok(
        core,
        "card_read",
        json!({"target": path, "value": {"include": ["tags"]}}),
    )["tags"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(|t| t.as_str().unwrap().to_string())
        .collect()
}

// MARK: - tags

#[test]
fn tags_are_replaced_by_an_array_and_added_or_taken_away_by_add_and_remove() {
    let core = core();
    ok(
        &core,
        "card_update",
        json!({"target": "Schemas:Customer", "value": {"tags": ["a", "b", "c"]}}),
    );
    assert_eq!(tags_of(&core, "Schemas:Customer"), ["a", "b", "c"]);
    // Only what is named changes; a tag taken away and not there is no error, one added twice is once.
    ok(
        &core,
        "card_update",
        json!({"target": "Schemas:Customer", "value": {"tags": {"add": ["d", "a"], "remove": ["b", "zz"]}}}),
    );
    assert_eq!(tags_of(&core, "Schemas:Customer"), ["a", "c", "d"]);
    // remove goes first, so a tag in both stays.
    ok(
        &core,
        "card_update",
        json!({"target": "Schemas:Customer", "value": {"tags": {"add": ["c"], "remove": ["c"]}}}),
    );
    assert_eq!(tags_of(&core, "Schemas:Customer"), ["a", "d", "c"]);
    ok(
        &core,
        "card_update",
        json!({"target": "Schemas:Customer", "value": {"tags": []}}),
    );
    assert!(tags_of(&core, "Schemas:Customer").is_empty());
}

#[test]
fn tags_with_a_key_that_is_not_add_or_remove_are_refused_and_nothing_changes() {
    let core = core();
    ok(
        &core,
        "card_update",
        json!({"target": "Schemas:Customer", "value": {"tags": ["a"]}}),
    );
    for value in [
        json!({"added": ["b"]}),
        json!({}),
        json!({"add": "b"}),
        json!({"add": [1]}),
    ] {
        let said = refused(
            &core,
            "card_update",
            json!({"target": "Schemas:Customer", "value": {"tags": value}}),
        );
        assert!(said.contains("tags"), "{value}: {said}");
    }
    assert_eq!(tags_of(&core, "Schemas:Customer"), ["a"]);
}

// MARK: - An enum's values

#[test]
fn an_enums_values_are_read_and_written_as_name_and_label() {
    let core = core();
    let read = ok(
        &core,
        "card_read",
        json!({"target": "Schemas:Status", "value": {"include": ["fields"]}}),
    );
    assert_eq!(
        read["values"],
        json!([{"name": "open", "label": "受付"}, {"name": "paid"},
            {"name": "shipped", "label": "発送済み"}])
    );
    // What was read goes back as it is; a bare text is a name with no label.
    ok(
        &core,
        "card_update",
        json!({"target": "Schemas:Status", "value": {"values": read["values"].clone()}}),
    );
    ok(
        &core,
        "card_update",
        json!({"target": "Schemas:Status", "value": {"values": [{"name": "open", "label": "受付"}, "paid", {"name": "lost", "label": "失われた"}]}}),
    );
    let after = ok(
        &core,
        "card_read",
        json!({"target": "Schemas:Status", "value": {"include": ["fields"]}}),
    );
    assert_eq!(
        after["values"],
        json!([{"name": "open", "label": "受付"}, {"name": "paid"}, {"name": "lost", "label": "失われた"}])
    );
    ok(
        &core,
        "card_create",
        json!({"target": "Schemas:Level", "value": {"kind": "enum", "values": [{"name": "low", "label": "低"}, "high"]}}),
    );
    let made = ok(
        &core,
        "card_read",
        json!({"target": "Schemas:Level", "value": {"include": ["fields"]}}),
    );
    assert_eq!(
        made["values"],
        json!([{"name": "low", "label": "低"}, {"name": "high"}])
    );
}

#[test]
fn values_named_by_value_are_refused_with_the_call_written_by_name() {
    let core = core();
    let said = refusal(
        &core,
        "card_update",
        json!({"target": "Schemas:Status", "value": {"values": [{"value": "open", "label": "受付"}, {"value": "paid"}, "lost"]}}),
    );
    assert!(said["message"].as_str().unwrap().contains("name"), "{said}");
    assert_eq!(
        said["use"]["arguments"]["value"]["values"],
        json!([{"name": "open", "label": "受付"}, {"name": "paid"}, "lost"]),
        "{said}"
    );
    sent(&core, &said);
    let read = ok(&core, "card_read", json!({"target": "Schemas:Status"}));
    assert!(
        read.get("values").is_none() || read["values"].is_array(),
        "{read}"
    );
    // In a call that makes cards, each target's value is named in `use`.
    let said = refusal(
        &core,
        "card_create",
        json!({"value": {
            "Schemas:Level": {"kind": "enum", "values": [{"value": "low"}]},
            "Schemas:Size": {"kind": "enum", "values": [{"value": "big", "label": "大"}]},
        }}),
    );
    assert_eq!(
        said["use"]["arguments"]["value"]["Schemas:Size"]["values"],
        json!([{"name": "big", "label": "大"}]),
        "{said}"
    );
}

// MARK: - The boxes of a diagram

#[test]
fn a_sections_boxes_are_read_by_the_path_a_write_puts_them_by() {
    let core = core();
    ok(
        &core,
        "section_update",
        json!({"target": "Schemas", "value": {"places": {
            "Schemas:Customer": {"topLeft": {"x": 200, "y": 0}},
            "Schemas/Ordering:Order": {"topLeft": {"x": 200, "y": 40}},
            "Schemas/Ordering": {"topLeft": {"x": 200, "y": 80}, "size": {"w": 12, "h": 6}},
        }}}),
    );
    let read = ok(
        &core,
        "section_read",
        json!({"target": "Schemas", "value": {"include": ["places"]}}),
    );
    let places = read["places"].as_object().unwrap();
    // A card, a card ghost of another section's card and a section ghost, each under its own path.
    assert_eq!(
        read["places"]["Schemas:Customer"]["topLeft"],
        json!({"x": 200, "y": 0})
    );
    assert_eq!(
        read["places"]["Schemas/Ordering:Order"]["topLeft"],
        json!({"x": 200, "y": 40})
    );
    assert_eq!(
        read["places"]["Schemas/Ordering"],
        json!({"topLeft": {"x": 200, "y": 80}, "bottomRight": {"x": 211, "y": 85}, "size": {"w": 12, "h": 6}})
    );
    assert!(places.values().all(|b| b["topLeft"].is_object()));
    // What was read is written back as it is: the same boxes, nothing moved.
    let again = ok(
        &core,
        "section_update",
        json!({"target": "Schemas", "value": {"places": read["places"].clone()}}),
    );
    assert_eq!(again["placed"], places.len(), "{again}");
    assert!(again.get("jumped").is_none(), "{again}");
    let after = ok(
        &core,
        "section_read",
        json!({"target": "Schemas", "value": {"include": ["places"]}}),
    );
    assert_eq!(after["places"], read["places"]);
}

#[test]
fn the_boxes_are_cut_by_their_paths_like_a_list() {
    let core = core();
    let whole = ok(
        &core,
        "section_read",
        json!({"target": "Schemas", "value": {"include": ["places"]}}),
    );
    let all: Vec<String> = whole["places"]
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect();
    assert!(all.len() >= 3, "{whole}");
    // Cards and boxes together are cut at the limit, and the rest goes on by `next`.
    let mut seen: Vec<String> = Vec::new();
    let mut after: Option<String> = None;
    for _ in 0..40 {
        let mut value = json!({"include": ["places"], "limit": 2});
        if let Some(after) = &after {
            value["after"] = after.clone().into();
        }
        let page = ok(
            &core,
            "section_read",
            json!({"target": "Schemas", "value": value}),
        );
        if let Some(places) = page.get("places") {
            assert!(places.is_object(), "{page}");
            seen.extend(places.as_object().unwrap().keys().cloned());
        }
        match page.get("next").and_then(Value::as_str) {
            Some(next) => after = Some(next.to_string()),
            None => break,
        }
    }
    let mut seen_sorted = seen.clone();
    seen_sorted.sort();
    seen_sorted.dedup();
    let mut all_sorted = all.clone();
    all_sorted.sort();
    assert_eq!(seen_sorted, all_sorted, "{seen:?}");
}

#[test]
fn include_at_gives_the_size_beside_the_corners() {
    let core = core();
    let read = ok(
        &core,
        "card_read",
        json!({"target": "Schemas:Customer", "value": {"include": ["at"]}}),
    );
    let (top_left, bottom_right, size) = (&read["topLeft"], &read["bottomRight"], &read["size"]);
    assert_eq!(
        size["w"].as_i64().unwrap(),
        bottom_right["x"].as_i64().unwrap() - top_left["x"].as_i64().unwrap() + 1,
        "{read}"
    );
    assert_eq!(
        size["h"].as_i64().unwrap(),
        bottom_right["y"].as_i64().unwrap() - top_left["y"].as_i64().unwrap() + 1,
        "{read}"
    );
}

// MARK: - A spelling that was replaced

#[test]
fn a_replaced_key_is_told_where_it_was_replaced_and_not_where_it_is_still_written() {
    let core = core();
    // card_create has no `section` in its value: the section is in the target.
    let said = refused(
        &core,
        "card_create",
        json!({"target": "Schemas:Fresh", "value": {"kind": "entity", "section": "Schemas"}}),
    );
    assert!(
        said.contains("section") && said.contains("target"),
        "{said}"
    );
    // card_update still has it: it moves the card.
    ok(
        &core,
        "card_update",
        json!({"target": "Schemas:Customer", "value": {"section": "Schemas/Ordering"}}),
    );
    // A key that was replaced in card_update's card form says what it is now.
    let said = refused(
        &core,
        "card_update",
        json!({"target": "Schemas/Ordering:Customer", "value": {"moveTo": "Schemas"}}),
    );
    assert!(
        said.contains("moveTo") && said.contains("section"),
        "{said}"
    );
    let said = refused(
        &core,
        "card_update",
        json!({"target": "Schemas/Ordering:Customer", "value": {"at": {"x": 1, "y": 2}}}),
    );
    assert!(said.contains("topLeft"), "{said}");
    // Another tool's value does not get a card's replaced keys: card_read takes `section`.
    ok(
        &core,
        "card_read",
        json!({"target": "Customer", "value": {"section": "Schemas/Ordering"}}),
    );
}

// MARK: - A union of types

#[test]
fn a_key_inside_a_body_patch_that_is_not_one_is_a_complaint_listing_the_keys() {
    let core = core();
    let said = refused(
        &core,
        "card_update",
        json!({"target": "Docs:Overview", "value": {"body": {"appnd": "x"}}}),
    );
    assert!(
        said.contains("body")
            && said.contains("edit")
            && said.contains("append")
            && said.contains("prepend"),
        "{said}"
    );
    let said = refused(
        &core,
        "card_update",
        json!({"target": "Docs:Overview", "value": {"body": {"edit": [{"old": "a", "new": "b", "every": true}]}}}),
    );
    assert!(
        said.contains("every") && said.contains("old") && said.contains("all"),
        "{said}"
    );
    // A value that is none of the types says which it can be.
    let said = refused(
        &core,
        "card_update",
        json!({"target": "Docs:Overview", "value": {"body": 5}}),
    );
    assert!(said.contains("string") && said.contains("object"), "{said}");
}

// MARK: - A decimal written as a number

/// The sample project with a data section and an order whose total is a decimal.
fn with_order() -> Core {
    let core = core();
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
        json!({"target": "Data:Order1", "value": {"kind": "instance", "of": "Schemas/Ordering:Order",
            "data": {"id": "7d1e2f3a-4b5c-4d6e-8f90-a1b2c3d4e5f6", "total": "880", "status": "paid",
                "lines": [{"sku": "C-3", "qty": 1}], "kind": "order"}}}),
    );
    core
}

#[test]
fn a_decimal_written_as_a_number_is_refused_with_the_call_written_as_text() {
    let core = with_order();
    let said = refusal(
        &core,
        "card_update",
        json!({"target": "Data:Order1", "value": {"data": {"id": "7d1e2f3a-4b5c-4d6e-8f90-a1b2c3d4e5f6", "total": 880.5,
            "status": "paid", "lines": [{"sku": "C-3", "qty": 1}], "kind": "order"}}}),
    );
    assert_eq!(
        said["use"]["arguments"]["value"]["data"]["total"], "880.5",
        "{said}"
    );
    sent(&core, &said);
    assert_eq!(
        ok(&core, "card_read", json!({"target": "Data:Order1.total"}))["value"],
        "880.5"
    );
    // One value alone, by its path.
    let said = refusal(
        &core,
        "card_update",
        json!({"target": "Data:Order1.total", "value": {"value": 12}}),
    );
    assert_eq!(said["use"]["arguments"]["value"]["value"], "12", "{said}");
    sent(&core, &said);
    // A card made with its data, in a call keyed by the cards' paths.
    let said = refusal(
        &core,
        "card_create",
        json!({"value": {"Data:Order2": {"kind": "instance", "of": "Schemas/Ordering:Order",
            "data": {"id": "0c9d8e7f-6a5b-4c3d-9e2f-1a0b9c8d7e6f", "total": 450.25, "status": "paid",
                "lines": [{"sku": "A-1", "qty": 3}], "kind": "order"}}}}),
    );
    assert_eq!(
        said["use"]["arguments"]["value"]["Data:Order2"]["data"]["total"], "450.25",
        "{said}"
    );
    sent(&core, &said);
}

#[test]
fn inside_a_batch_the_fix_is_in_the_batch_call() {
    let core = with_order();
    let steps = json!([
        {"tool": "card_update", "arguments": {"target": "Schemas:Customer", "value": {"label": "客"}}},
        {"tool": "card_update", "arguments": {"target": "Data:Order1.total", "value": {"value": 75.5}}},
    ]);
    let said = refusal(
        &core,
        "batch",
        json!({"execute": true, "value": {"steps": steps}}),
    );
    assert_eq!(said["use"]["tool"], "batch", "{said}");
    assert_eq!(
        said["use"]["arguments"]["value"]["steps"][1]["arguments"]["value"]["value"], "75.5",
        "{said}"
    );
    assert_eq!(
        said["use"]["arguments"]["value"]["steps"][0], steps[0],
        "{said}"
    );
    sent(&core, &said);
    assert_eq!(
        ok(&core, "card_read", json!({"target": "Data:Order1.total"}))["value"],
        "75.5"
    );
}

#[test]
fn a_number_a_float_field_takes_is_not_refused() {
    let core = with_order();
    // `qty` is an integer, `total` alone is the decimal: a number for the integer goes through.
    ok(
        &core,
        "card_update",
        json!({"target": "Data:Order1.lines[0].qty", "value": {"value": 2}}),
    );
}
