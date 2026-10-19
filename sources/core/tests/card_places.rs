//! Where `card_update` puts a card: where it says, or — when that overlaps another card
//! of its section or comes within the margin of one — the nearest clear cell, said in a
//! warning. A card made with no place is given one.

use crate::common;

use common::{call, checked, script_store};
use serde_json::{Value, json};
use specboard_core::Core;

fn top_left(core: &Core, path: &str) -> Value {
    let (failed, text) = call(
        core,
        "card_read",
        json!({"project": "見本", "target": path}),
    );
    assert!(!failed, "{text}");
    serde_json::from_str::<Value>(&text).unwrap()["topLeft"].clone()
}

#[test]
fn a_card_put_over_another_goes_to_the_nearest_clear_cell() {
    let core = Core::open(script_store()).unwrap();
    let there = top_left(&core, "Schemas:Status");
    let (failed, text) = call(
        &core,
        "card_update",
        json!({"project": "見本", "target": "Schemas:Customer", "value": {"topLeft": there}}),
    );
    assert!(!failed, "{text}");
    let answer: Value = serde_json::from_str(&text).unwrap();
    let said = answer["warnings"][0].as_str().unwrap_or_default();
    assert!(said.contains("Schemas:Status と重なる"), "{text}");
    let landed = top_left(&core, "Schemas:Customer");
    assert_ne!(landed, there);
    assert!(
        said.contains(&format!("x: {}, y: {}", landed["x"], landed["y"])),
        "{said}"
    );
}

#[test]
fn a_card_put_in_the_clear_stays_where_it_was_put() {
    let core = Core::open(script_store()).unwrap();
    let (failed, text) = call(
        &core,
        "card_update",
        json!({"project": "見本", "target": "Schemas:Customer", "value": {"topLeft": {"x": 500, "y": 500}}}),
    );
    assert!(!failed, "{text}");
    assert!(!text.contains("warnings"), "{text}");
    assert_eq!(
        top_left(&core, "Schemas:Customer"),
        json!({"x": 500, "y": 500})
    );
}

#[test]
fn a_card_made_with_no_place_is_given_one() {
    let core = Core::open(script_store()).unwrap();
    let (failed, text) = call(
        &core,
        "card_create",
        json!({"project": "見本", "target": "Free:Fresh", "value": {"kind": "note", "body": "新しい"}}),
    );
    assert!(!failed, "{text}");
    assert!(top_left(&core, "Free:Fresh").is_object());
}

fn places(core: &Core, section: &str) -> Value {
    let (failed, text) = call(
        core,
        "section_read",
        json!({"project": "見本", "target": section, "value": {"include": ["places"]}}),
    );
    assert!(!failed, "{text}");
    let read: Value = serde_json::from_str(&text).unwrap();
    assert!(read["places"].is_object(), "{text}");
    read["places"].clone()
}

/// The box put by `path`: a card's, a card ghost's or a section ghost's.
fn box_of<'v>(boxes: &'v Value, path: &str) -> &'v Value {
    boxes
        .get(path)
        .unwrap_or_else(|| panic!("{path} が無い: {boxes}"))
}

#[test]
fn a_diagrams_boxes_are_put_in_one_go_and_read_back() {
    let core = Core::open(script_store()).unwrap();
    let (failed, text) = call(
        &core,
        "section_update",
        json!({"project": "見本", "target": "Schemas", "value": {"places": {
            "Schemas:Customer": {"topLeft": {"x": 200, "y": 0}},
            "Schemas/Ordering:Order": {"topLeft": {"x": 200, "y": 40}},
            "Schemas/Ordering": {"topLeft": {"x": 200, "y": 80}, "size": {"w": 12, "h": 6}},
        }}}),
    );
    assert!(!failed, "{text}");
    let answer: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(answer["placed"], 3);
    assert!(answer.get("jumped").is_none(), "{text}");
    let boxes = places(&core, "Schemas");
    assert_eq!(
        box_of(&boxes, "Schemas:Customer")["topLeft"],
        json!({"x": 200, "y": 0})
    );
    // A card ghost — another section's card drawn on this diagram — keeps its own place
    // there.
    assert_eq!(
        box_of(&boxes, "Schemas/Ordering:Order")["topLeft"],
        json!({"x": 200, "y": 40})
    );
    let section_ghost = box_of(&boxes, "Schemas/Ordering");
    assert_eq!(section_ghost["size"], json!({"w": 12, "h": 6}));
    assert_eq!(section_ghost["bottomRight"], json!({"x": 211, "y": 85}));
    // The card itself is where it was in its own section.
    let own = places(&core, "Schemas/Ordering");
    assert_ne!(
        box_of(&own, "Schemas/Ordering:Order")["topLeft"],
        json!({"x": 200, "y": 40})
    );
}

#[test]
fn boxes_that_still_overlap_once_all_are_put_go_to_the_nearest_clear_cell() {
    let core = Core::open(script_store()).unwrap();
    let (failed, text) = call(
        &core,
        "section_update",
        json!({"project": "見本", "target": "Schemas", "value": {"places": {
            "Schemas:Customer": {"topLeft": {"x": 300, "y": 0}},
            "Schemas:Status": {"topLeft": {"x": 300, "y": 0}},
        }}}),
    );
    assert!(!failed, "{text}");
    let answer: Value = serde_json::from_str(&text).unwrap();
    let jumped = answer["jumped"].as_array().unwrap();
    assert_eq!(jumped.len(), 1, "{text}");
    let boxes = places(&core, "Schemas");
    let moved = jumped[0]["card"].as_str().unwrap();
    assert_eq!(box_of(&boxes, moved)["topLeft"], jumped[0]["topLeft"]);
    assert_ne!(jumped[0]["topLeft"], json!({"x": 300, "y": 0}));
}

#[test]
fn a_swap_of_two_boxes_is_not_held_up_by_the_way_there() {
    let core = Core::open(script_store()).unwrap();
    let put = |customer: Value, status: Value| {
        let (failed, text) = call(
            &core,
            "section_update",
            json!({"project": "見本", "target": "Schemas", "value": {"places": {
                "Schemas:Customer": {"topLeft": customer},
                "Schemas:Status": {"topLeft": status},
            }}}),
        );
        assert!(!failed, "{text}");
        serde_json::from_str::<Value>(&text).unwrap()
    };
    let (a, b) = (json!({"x": 500, "y": 0}), json!({"x": 500, "y": 100}));
    put(a.clone(), b.clone());
    // Each to where the other was: put one by one, the first would land on the second.
    let answer = put(b.clone(), a.clone());
    assert!(answer.get("jumped").is_none(), "{answer}");
    let after = places(&core, "Schemas");
    assert_eq!(box_of(&after, "Schemas:Customer")["topLeft"], b);
    assert_eq!(box_of(&after, "Schemas:Status")["topLeft"], a);
}

#[test]
fn a_box_said_wrongly_is_refused_and_nothing_is_put() {
    let core = Core::open(script_store()).unwrap();
    let before = places(&core, "Schemas");
    let (failed, text) = call(
        &core,
        "section_update",
        json!({"project": "見本", "target": "Schemas", "value": {"places": {
            "Schemas:Customer": {"topLeft": {"x": 900, "y": 0}},
            "Schemas:Status": {"at": {"x": 0, "y": 0}},
        }}}),
    );
    assert!(failed);
    assert!(
        text.contains(
            "places の「Schemas:Status」に使えないキーです: at。使えるのは topLeft / size"
        ),
        "{text}"
    );
    assert_eq!(places(&core, "Schemas"), before);
    let (failed, text) = call(
        &core,
        "section_update",
        json!({"project": "見本", "target": "Schemas", "value": {"places": {
            "Schemas:Customer": {"size": {"h": 6}},
        }}}),
    );
    assert!(failed);
    assert!(text.contains("高さだけは書けません"), "{text}");
}

#[test]
fn a_flow_made_with_its_nodes_where_they_sit_keeps_them_there() {
    let core = Core::open(script_store()).unwrap();
    // The flow and its node cards in one call: a node's place is its box on the flow's
    // card map.
    let (failed, text) = call(
        &core,
        "card_create",
        json!({"project": "見本", "value": {
            "Charts:Placed": {"kind": "flow"},
            "Charts:Placed.start": {"kind": "begin", "trigger": "manual", "topLeft": {"x": 4, "y": 6},
                                    "connections": [{"line": "next", "to": "done"}]},
            "Charts:Placed.done": {"kind": "end", "outcome": "success"}
        }}),
    );
    assert!(!failed, "{text}");
    let at = |path: &str| {
        let (failed, text) = call(
            &core,
            "card_read",
            json!({"project": "見本", "target": path, "value": {"include": ["at"]}}),
        );
        assert!(!failed, "{text}");
        serde_json::from_str::<Value>(&text).unwrap()["topLeft"].clone()
    };
    assert_eq!(at("Charts:Placed.start"), json!({"x": 4, "y": 6}));
    // One given no place is put where there is room.
    assert!(at("Charts:Placed.done").is_object());
}

fn mcp(core: &Core, tool: &str, mut arguments: Value) -> Value {
    arguments["project"] = "見本".into();
    let (failed, text) = call(core, tool, arguments.clone());
    assert!(!failed, "{tool} {arguments}: {text}");
    serde_json::from_str(&text).unwrap()
}

/// A value object an entity of its section holds is drawn folded into it, not as a card: its
/// box is still read — marked so — and blocks nothing; another card goes where it is put.
#[test]
fn a_value_folded_into_its_holder_blocks_nothing() {
    let core = Core::open(script_store()).unwrap();
    mcp(&core, "section_create", json!({"target": "Schemas/Shapes"}));
    mcp(
        &core,
        "card_create",
        json!({"target": "Schemas/Shapes:Inner", "value": {"kind": "value", "topLeft": {"x": 0, "y": 0},
            "fields": [{"name": "a", "type": "string"}]}}),
    );
    mcp(
        &core,
        "card_create",
        json!({"target": "Schemas/Shapes:Holder", "value": {"kind": "entity", "topLeft": {"x": 30, "y": 0},
            "fields": [{"name": "inner", "type": "string"}],
            "connections": [{"line": "embed", "from": "inner", "to": "Inner"}]}}),
    );
    let boxes = places(&core, "Schemas/Shapes");
    assert_eq!(
        box_of(&boxes, "Schemas/Shapes:Inner")["drawn"],
        json!(false)
    );
    assert!(
        box_of(&boxes, "Schemas/Shapes:Holder")
            .get("drawn")
            .is_none()
    );
    let made = mcp(
        &core,
        "card_create",
        json!({"target": "Schemas/Shapes:Other", "value": {"kind": "entity", "topLeft": {"x": 0, "y": 0}}}),
    );
    assert!(made.get("warnings").is_none(), "{made}");
    assert_eq!(
        top_left(&core, "Schemas/Shapes:Other"),
        json!({"x": 0, "y": 0})
    );
}

/// A card that has come to lie over another — grown since it was put — is said by the check
/// (`overlaps`: which, over what, and where it would go) and moved clear by `tidy`, nothing
/// else touched.
#[test]
fn cards_that_overlap_are_said_and_tidied() {
    let core = Core::open(script_store()).unwrap();
    mcp(&core, "section_create", json!({"target": "Schemas/Shapes"}));
    mcp(
        &core,
        "card_create",
        json!({"target": "Schemas/Shapes:Grows", "value": {"kind": "entity", "topLeft": {"x": 0, "y": 0}}}),
    );
    mcp(
        &core,
        "card_create",
        json!({"target": "Schemas/Shapes:Below", "value": {"kind": "entity", "topLeft": {"x": 0, "y": 12}}}),
    );
    mcp(
        &core,
        "card_create",
        json!({"target": "Schemas/Shapes:Aside", "value": {"kind": "entity", "topLeft": {"x": 40, "y": 0}}}),
    );
    let overlapping = |core: &Core| -> Vec<Value> {
        checked(core, "main")["overlaps"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter(|o| o["section"] == "Schemas/Shapes")
            .collect()
    };
    assert!(overlapping(&core).is_empty());
    // Grown past the card below it.
    let fields: Vec<String> = (0..20)
        .map(|n| format!("Schemas/Shapes:Grows.f{n}"))
        .collect();
    mcp(
        &core,
        "card_create",
        json!({"target": fields, "value": {"type": "string"}}),
    );
    let said = overlapping(&core);
    assert_eq!(said.len(), 1, "{said:?}");
    assert_eq!(said[0]["at"], "Schemas/Shapes:Below");
    assert_eq!(said[0]["over"], json!(["Schemas/Shapes:Grows"]));
    let to = said[0]["moveTo"].clone();
    let tidied = mcp(
        &core,
        "section_update",
        json!({"target": "Schemas/Shapes", "value": {"tidy": true}}),
    );
    assert_eq!(
        tidied["tidied"],
        json!([{"card": "Schemas/Shapes:Below", "topLeft": to}])
    );
    assert_eq!(top_left(&core, "Schemas/Shapes:Below"), to);
    assert_eq!(
        top_left(&core, "Schemas/Shapes:Aside"),
        json!({"x": 40, "y": 0})
    );
    assert!(overlapping(&core).is_empty());
}

/// Untangling moves a card off where an earlier one would land, which a card clear where it sits
/// can be: the check says only cards lying over another where they all sit now.
#[test]
fn a_card_clear_where_it_sits_is_not_said_to_overlap() {
    let core = Core::open(script_store()).unwrap();
    mcp(&core, "section_create", json!({"target": "Schemas/Shapes"}));
    for (name, y) in [("Grows", 0), ("Below", 12)] {
        mcp(
            &core,
            "card_create",
            json!({"target": format!("Schemas/Shapes:{name}"), "value": {"kind": "entity", "topLeft": {"x": 0, "y": y}}}),
        );
    }
    let fields: Vec<String> = (0..20)
        .map(|n| format!("Schemas/Shapes:Grows.f{n}"))
        .collect();
    mcp(
        &core,
        "card_create",
        json!({"target": fields, "value": {"type": "string"}}),
    );
    let overlapping = |core: &Core| -> Vec<Value> {
        checked(core, "main")["overlaps"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter(|o| o["section"] == "Schemas/Shapes")
            .collect()
    };
    let to = overlapping(&core)[0]["moveTo"].clone();
    // A card put where the one below would go, clear of both where they sit.
    mcp(
        &core,
        "card_create",
        json!({"target": "Schemas/Shapes:There", "value": {"kind": "entity", "topLeft": to}}),
    );
    assert_eq!(top_left(&core, "Schemas/Shapes:There"), to);
    let said = overlapping(&core);
    let cards: Vec<&Value> = said.iter().map(|o| &o["at"]).collect();
    assert_eq!(cards, [&json!("Schemas/Shapes:Below")], "{said:?}");
    assert_eq!(said[0]["over"], json!(["Schemas/Shapes:Grows"]));
    // Said for knowing, as cards can lie over one another and be as meant.
    assert_eq!(said[0]["kind"], "info", "{said:?}");
    let list = mcp(&core, "inspection_list", json!({"value": {"show": "info"}}));
    assert_eq!(list["groups"]["overlaps"]["kind"], "info", "{list}");
}

// MARK: - A region, and boxes moved and arranged together

/// A section Free/Lab of three notes a line long (10 by 3 cells): A at (0, 0), B at (11, 0) and
/// C at (0, 10).
fn lab() -> Core {
    let core = Core::open(script_store()).unwrap();
    let ok = |tool: &str, arguments: Value| {
        let (failed, text) = call(&core, tool, arguments.clone());
        assert!(!failed, "{tool} {arguments}: {text}");
    };
    ok(
        "section_create",
        json!({"project": "見本", "target": "Free/Lab"}),
    );
    for (name, x, y) in [("A", 0, 0), ("B", 11, 0), ("C", 0, 10)] {
        ok(
            "card_create",
            json!({"project": "見本", "target": format!("Free/Lab:{name}"),
                   "value": {"kind": "note", "body": name, "topLeft": {"x": x, "y": y}}}),
        );
    }
    core
}

fn update(core: &Core, value: Value) -> Result<Value, String> {
    let (failed, text) = call(
        core,
        "section_update",
        json!({"project": "見本", "target": "Free/Lab", "value": value}),
    );
    if failed {
        return Err(text);
    }
    Ok(serde_json::from_str(&text).unwrap())
}

fn at(core: &Core, name: &str) -> (i64, i64) {
    let place = top_left(core, &format!("Free/Lab:{name}"));
    (place["x"].as_i64().unwrap(), place["y"].as_i64().unwrap())
}

#[test]
fn the_cards_a_region_takes_are_listed_in_either_mode() {
    let core = lab();
    let names = |value: Value| -> Result<Vec<String>, String> {
        let (failed, text) = call(
            &core,
            "card_list",
            json!({"project": "見本", "value": value}),
        );
        if failed {
            return Err(text);
        }
        let read: Value = serde_json::from_str(&text).unwrap();
        Ok(read["cards"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["path"].as_str().unwrap().to_string())
            .collect())
    };
    // Any of B in it (it reaches from x 11 to 20), or the whole of it: A alone.
    let touching = names(json!({"section": "Free/Lab", "region": {"from": {"x": 0, "y": 0}, "to": {"x": 12, "y": 5}}}))
        .unwrap();
    assert_eq!(touching, ["Free/Lab:A", "Free/Lab:B"]);
    let inside = names(json!({"section": "Free/Lab", "region": {"from": {"x": 12, "y": 5}, "to": {"x": 0, "y": 0}, "mode": "contain"}}))
        .unwrap();
    assert_eq!(inside, ["Free/Lab:A"]);
    // The corners both in it: C's top row is 10.
    let corner = names(json!({"section": "Free/Lab", "region": {"from": {"x": 9, "y": 10}, "to": {"x": 9, "y": 10}}}))
        .unwrap();
    assert_eq!(corner, ["Free/Lab:C"]);
    let alone =
        names(json!({"region": {"from": {"x": 0, "y": 0}, "to": {"x": 1, "y": 1}}})).unwrap_err();
    assert!(
        alone.contains("region は section（どのセクションの図か）と一緒に使います"),
        "{alone}"
    );
    let wrong = names(json!({"section": "Free/Lab", "region": {"from": {"x": 0, "y": 0}, "to": {"x": 1, "y": 1}, "mode": "all"}}))
        .unwrap_err();
    assert!(wrong.contains("intersect"), "{wrong}");
}

#[test]
fn boxes_are_moved_together_by_or_to_and_kept_clear_as_a_group() {
    let core = lab();
    let moved = update(
        &core,
        json!({"move": {"cards": ["Free/Lab:A", "Free/Lab:B"], "by": {"x": 0, "y": 20}}}),
    )
    .unwrap();
    assert_eq!(
        moved["moved"],
        json!([{"card": "Free/Lab:A", "topLeft": {"x": 0, "y": 20}}, {"card": "Free/Lab:B", "topLeft": {"x": 11, "y": 20}}])
    );
    assert!(moved.get("shifted").is_none(), "{moved}");
    assert_eq!(at(&core, "C"), (0, 10));
    // Picked by a region, their top left put at a cell, kept as they were among themselves.
    update(
        &core,
        json!({"move": {"region": {"from": {"x": 0, "y": 20}, "to": {"x": 30, "y": 25}}, "to": {"x": 40, "y": 0}}}),
    )
    .unwrap();
    assert_eq!((at(&core, "A"), at(&core, "B")), ((40, 0), (51, 0)));
    // Asked to land on A: C goes as near as it is clear of A and B, a cell around them.
    let shifted = update(
        &core,
        json!({"move": {"cards": ["Free/Lab:C"], "to": {"x": 40, "y": 0}}}),
    )
    .unwrap();
    assert!(shifted["shifted"].is_object(), "{shifted}");
    let (x, y) = at(&core, "C");
    // Clear of a box at (ox, oy) by the margin (a cell): 10 wide, 3 tall.
    let clear = |(ox, oy): (i64, i64)| {
        let (gap_x, gap_y) = (ox.max(x) - ox.min(x), oy.max(y) - oy.min(y));
        gap_x > 10 || gap_y > 3
    };
    assert!(clear((40, 0)) && clear((51, 0)), "C at ({x}, {y})");
    let refused = update(&core, json!({"move": {"cards": ["Free/Lab:C"]}})).unwrap_err();
    assert!(refused.contains("by（ずらすマス）か to"), "{refused}");
    let refused = update(
        &core,
        json!({"move": {"cards": ["Free/Lab:C"], "region": {"from": {"x": 0, "y": 0}, "to": {"x": 1, "y": 1}}, "by": {"x": 1, "y": 0}}}),
    )
    .unwrap_err();
    assert!(
        refused.contains("cards（パスの並び）か region のどちらか一方"),
        "{refused}"
    );
}

#[test]
fn boxes_are_arranged_in_columns_a_margin_apart() {
    let core = lab();
    let arranged = update(
        &core,
        json!({"arrange": {"cards": ["Free/Lab:C", "Free/Lab:A", "Free/Lab:B"], "columns": 2, "at": {"x": 100, "y": 100}}}),
    )
    .unwrap();
    // In the order given, two columns 11 apart (a card 10 wide and the margin), each under the
    // one above it in its column (a note a line long is 3 tall, then the margin).
    assert_eq!(
        (at(&core, "C"), at(&core, "A"), at(&core, "B")),
        ((100, 100), (111, 100), (100, 104))
    );
    assert_eq!(arranged["arranged"].as_array().unwrap().len(), 3);
    // By a region, from the top down: from where the top left of them was, four columns.
    update(
        &core,
        json!({"arrange": {"region": {"from": {"x": 100, "y": 100}, "to": {"x": 130, "y": 110}, "mode": "contain"}}}),
    )
    .unwrap();
    assert_eq!(
        (at(&core, "C"), at(&core, "A"), at(&core, "B")),
        ((100, 100), (111, 100), (122, 100))
    );
    let refused = update(
        &core,
        json!({"arrange": {"cards": ["Free/Lab:A"], "columns": 0}}),
    )
    .unwrap_err();
    assert!(refused.contains("columns は 1〜12 の整数"), "{refused}");
    let refused = update(
        &core,
        json!({"arrange": {"cards": ["Free/Lab:A"], "gap": 2}}),
    )
    .unwrap_err();
    assert!(refused.contains("gap"), "{refused}");
}
