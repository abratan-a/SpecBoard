//! Arranging a section's map along its lines (`section_update`'s `arrange` with `by: "lines"`,
//! `specboard://SpecBoard/Docs/Design:Overview` §12): each set the lines join laid out in layers from
//! left to right, the box a data reference points at on the left, a layer wrapped where it grows
//! taller than the set is wide, the sets and the boxes no line reaches packed from the top left, a
//! group arranged inside and then placed whole, and the same map arranged the same way every time.

use crate::common;

use common::{ok, op, project_id, refused, script_store};
use serde_json::{Value, json};
use specboard_core::Core;
use std::collections::BTreeMap;

/// A box's top-left and size, in cells.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Cell {
    x: i64,
    y: i64,
    w: i64,
    h: i64,
}

impl Cell {
    fn right(&self) -> i64 {
        self.x + self.w
    }

    fn bottom(&self) -> i64 {
        self.y + self.h
    }

    /// Whether the two come within `margin` cells of each other.
    fn near(&self, other: &Cell, margin: i64) -> bool {
        self.x - margin < other.right()
            && other.x < self.right() + margin
            && self.y - margin < other.bottom()
            && other.y < self.bottom() + margin
    }
}

/// The drawn boxes of a section's map, by path.
fn boxes(core: &Core, section: &str) -> BTreeMap<String, Cell> {
    let read = ok(
        core,
        "section_read",
        json!({"target": section, "value": {"include": ["places"], "limit": 1000}}),
    );
    let mut out = BTreeMap::new();
    for (path, b) in read["places"].as_object().unwrap() {
        if b["drawn"] == false {
            continue;
        }
        let (tl, br) = (&b["topLeft"], &b["bottomRight"]);
        let (x, y) = (tl["x"].as_i64().unwrap(), tl["y"].as_i64().unwrap());
        out.insert(
            path.clone(),
            Cell {
                x,
                y,
                w: br["x"].as_i64().unwrap() - x + 1,
                h: br["y"].as_i64().unwrap() - y + 1,
            },
        );
    }
    out
}

fn of(boxes: &BTreeMap<String, Cell>, path: &str) -> Cell {
    *boxes
        .get(path)
        .unwrap_or_else(|| panic!("{path}: {:?}", boxes.keys().collect::<Vec<_>>()))
}

/// That no two of the boxes come within the margin of each other.
fn apart(boxes: &BTreeMap<String, Cell>) {
    let all: Vec<(&String, &Cell)> = boxes.iter().collect();
    for (i, (a, ca)) in all.iter().enumerate() {
        for (b, cb) in &all[i + 1..] {
            assert!(!ca.near(cb, 1), "{a} {ca:?} と {b} {cb:?}");
        }
    }
}

fn arrange(core: &Core, section: &str, arrange: Value) -> Value {
    ok(
        core,
        "section_update",
        json!({"target": section, "value": {"arrange": arrange}}),
    )
}

/// What `arranged` names: each row's path (a card's, a section ghost's) or group, with its key.
fn named(said: &Value) -> Vec<(String, String)> {
    said["arranged"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| {
            let key = ["card", "sectionGhost", "group"]
                .into_iter()
                .find(|k| row.get(*k).is_some())
                .unwrap();
            (key.to_string(), row[key].as_str().unwrap().to_string())
        })
        .collect()
}

fn entity(fields: &[&str], lines: Value) -> Value {
    let fields: Vec<Value> = fields
        .iter()
        .map(|f| json!({"name": f, "type": "string"}))
        .collect();
    json!({"kind": "entity", "fields": fields, "connections": lines})
}

/// Free/Shop: Customer, Order (customer points at Customer), Invoice (order points at Order), the
/// flow Checkout whose step writes Order, and three notes no line reaches — all put in one pile.
fn shop() -> Core {
    let core = Core::open(script_store()).unwrap();
    ok(&core, "section_create", json!({"target": "Free/Shop"}));
    ok(
        &core,
        "card_create",
        json!({"value": {
            "Free/Shop:Customer": entity(&["id"], json!([])),
            "Free/Shop:Order": entity(&["id", "customer"], json!([{"line": "points", "from": "customer", "to": "Free/Shop:Customer"}])),
            "Free/Shop:Invoice": entity(&["id", "order"], json!([{"line": "points", "from": "order", "to": "Free/Shop:Order"}])),
            "Free/Shop:Checkout": {"kind": "flow"},
            "Free/Shop:Checkout.save": {"kind": "process", "steps": [{"do": "write"}],
                "connections": [{"line": "writes", "from": "steps[0]", "to": "Free/Shop:Order"}]},
            "Free/Shop:N1": {"kind": "note", "body": "one"},
            "Free/Shop:N2": {"kind": "note", "body": "two"},
            "Free/Shop:N3": {"kind": "note", "body": "three"},
        }}),
    );
    core
}

const SHOP: [&str; 7] = [
    "Free/Shop:Customer",
    "Free/Shop:Order",
    "Free/Shop:Invoice",
    "Free/Shop:Checkout",
    "Free/Shop:N1",
    "Free/Shop:N2",
    "Free/Shop:N3",
];

#[test]
fn joined_cards_are_laid_in_layers_from_left_to_right_by_their_lines() {
    let core = shop();
    arrange(&core, "Free/Shop", json!({"by": "lines"}));
    let b = boxes(&core, "Free/Shop");
    let (customer, checkout) = (of(&b, SHOP[0]), of(&b, SHOP[3]));
    let (order, invoice) = (of(&b, SHOP[1]), of(&b, SHOP[2]));
    assert_eq!(customer.x, checkout.x, "{b:?}");
    let left = customer.right().max(checkout.right());
    assert!(order.x >= left + 4, "{b:?}");
    assert!(invoice.x >= order.right() + 4, "{b:?}");
    let set = [customer, checkout, order, invoice];
    let (right, bottom) = (
        set.iter().map(Cell::right).max().unwrap(),
        set.iter().map(Cell::bottom).max().unwrap(),
    );
    for note in &SHOP[4..] {
        let n = of(&b, note);
        assert!(n.y >= bottom || n.x >= right, "{note} {n:?} {b:?}");
    }
    apart(&b);
}

#[test]
fn a_data_reference_puts_the_card_pointed_at_on_the_left_and_other_lines_on_the_right() {
    let core = Core::open(script_store()).unwrap();
    ok(&core, "section_create", json!({"target": "Free/Shop"}));
    ok(
        &core,
        "kind_create",
        json!({"target": "token", "value": {"of": "line", "label": "トークン", "base": "designate",
            "from": [{"kinds": ["entity"], "at": "row"}], "to": {"kinds": ["entity"], "at": ["card"]}}}),
    );
    ok(
        &core,
        "card_create",
        json!({"value": {
            "Free/Shop:B": {"kind": "value", "standalone": true, "fields": [{"name": "v", "type": "string"}]},
            "Free/Shop:C": entity(&["id"], json!([])),
            "Free/Shop:A": entity(&["b", "c"], json!([
                {"line": "embed", "from": "b", "to": "Free/Shop:B"},
                {"line": "points", "from": "c", "to": "Free/Shop:C"}])),
            "Free/Shop:F": {"kind": "flow"},
            "Free/Shop:F.look": {"kind": "process", "steps": [{"do": "read"}],
                "connections": [{"line": "reads", "from": "steps[0]", "to": "Free/Shop:A"}]},
            "Free/Shop:N": {"kind": "note", "body": "n", "connections": [{"line": "link", "to": "Free/Shop:A"}]},
            "Free/Shop:D": entity(&["a"], json!([{"line": "token", "from": "a", "to": "Free/Shop:A"}])),
        }}),
    );
    arrange(&core, "Free/Shop", json!({"by": "lines"}));
    let b = boxes(&core, "Free/Shop");
    let a = of(&b, "Free/Shop:A");
    for left in ["B", "C", "F", "N"] {
        let c = of(&b, &format!("Free/Shop:{left}"));
        assert!(c.right() <= a.x, "{left} {c:?} A {a:?}");
    }
    assert!(of(&b, "Free/Shop:D").x >= a.right(), "{b:?}");
    apart(&b);
}

#[test]
fn a_layer_taller_than_its_limit_wraps_into_another_column() {
    let core = Core::open(script_store()).unwrap();
    ok(&core, "section_create", json!({"target": "Free/Hub"}));
    let mut cards = serde_json::Map::new();
    cards.insert("Free/Hub:Hub".into(), entity(&["id"], json!([])));
    for i in 0..40 {
        cards.insert(
            format!("Free/Hub:E{i:02}"),
            entity(
                &["id", "hub"],
                json!([{"line": "points", "from": "hub", "to": "Free/Hub:Hub"}]),
            ),
        );
    }
    ok(&core, "card_create", json!({"value": cards}));
    arrange(&core, "Free/Hub", json!({"by": "lines"}));
    let b = boxes(&core, "Free/Hub");
    let hub = of(&b, "Free/Hub:Hub");
    let mut columns: BTreeMap<i64, Vec<Cell>> = BTreeMap::new();
    for i in 0..40 {
        let e = of(&b, &format!("Free/Hub:E{i:02}"));
        assert_eq!((e.w, e.h), (10, 4), "{e:?}");
        assert!(e.x >= hub.right(), "{e:?} {hub:?}");
        columns.entry(e.x).or_default().push(e);
    }
    let xs: Vec<i64> = columns.keys().copied().collect();
    assert_eq!(xs.len(), 4, "{xs:?}");
    for pair in xs.windows(2) {
        assert_eq!(pair[1] - (pair[0] + 10), 2, "{xs:?}");
    }
    for column in columns.values() {
        let top = column.iter().map(|c| c.y).min().unwrap();
        let bottom = column.iter().map(Cell::bottom).max().unwrap();
        assert!(bottom - top <= 50, "{column:?}");
    }
    apart(&b);
}

#[test]
fn a_long_chain_is_wrapped_into_bands_no_wider_than_the_pack() {
    let core = Core::open(script_store()).unwrap();
    ok(&core, "section_create", json!({"target": "Free/Chain"}));
    let mut cards = serde_json::Map::new();
    for i in 0..40 {
        let mut note = json!({"kind": "note", "body": format!("c{i}")});
        if i < 39 {
            note["connections"] =
                json!([{"line": "link", "to": format!("Free/Chain:C{:02}", i + 1)}]);
        }
        cards.insert(format!("Free/Chain:C{i:02}"), note);
    }
    ok(&core, "card_create", json!({"value": cards}));
    arrange(&core, "Free/Chain", json!({"by": "lines"}));
    let b = boxes(&core, "Free/Chain");
    let cells: Vec<Cell> = (0..40)
        .map(|i| of(&b, &format!("Free/Chain:C{i:02}")))
        .collect();
    assert!(cells.iter().all(|c| (c.w, c.h) == (10, 4)), "{cells:?}");
    // The width the map is packed across: the square of what its boxes cover with the margin, a
    // quarter wider, and four cards at the least.
    let covered = (40 * (10 + 1) * (4 + 1)) as f64;
    let pack = ((1.25 * covered.sqrt()).ceil() as i64).max(4 * (10 + 1));
    let left = cells.iter().map(|c| c.x).min().unwrap();
    let right = cells.iter().map(Cell::right).max().unwrap();
    assert!(
        right - left <= pack + 10,
        "{} > {pack}: {cells:?}",
        right - left
    );
    let bands: std::collections::BTreeSet<i64> = cells.iter().map(|c| c.y).collect();
    assert!(bands.len() > 1, "{cells:?}");
    // Read along a band, then from the next band's left: the chain's order.
    for pair in cells.windows(2) {
        let (a, c) = (pair[0], pair[1]);
        assert!(
            (c.y == a.y && c.x >= a.right() + 4) || (c.y >= a.bottom() + 4 && c.x == left),
            "{a:?} then {c:?}"
        );
    }
    apart(&b);
}

#[test]
fn sets_and_cards_with_no_lines_are_packed_in_reading_order() {
    let core = Core::open(script_store()).unwrap();
    ok(&core, "section_create", json!({"target": "Free/Many"}));
    let mut cards = serde_json::Map::new();
    for i in 0..60 {
        cards.insert(
            format!("Free/Many:M{i:02}"),
            json!({"kind": "note", "body": format!("m{i}"), "size": {"w": 18, "h": 10}}),
        );
    }
    ok(&core, "card_create", json!({"value": cards}));
    ok(
        &core,
        "card_create",
        json!({"value": {
            "Free/Many:A1": {"kind": "note", "body": "a1", "connections": [{"line": "link", "to": "Free/Many:A2"}]},
            "Free/Many:A2": {"kind": "note", "body": "a2"},
            "Free/Many:B1": {"kind": "note", "body": "b1", "connections": [{"line": "link", "to": "Free/Many:B2"}]},
            "Free/Many:B2": {"kind": "note", "body": "b2"},
        }}),
    );
    arrange(&core, "Free/Many", json!({"by": "lines"}));
    let b = boxes(&core, "Free/Many");
    let left = b.values().map(|c| c.x).min().unwrap();
    let top = b.values().map(|c| c.y).min().unwrap();
    let right = b.values().map(Cell::right).max().unwrap();
    let bottom = b.values().map(Cell::bottom).max().unwrap();
    assert!(
        (bottom - top) as f64 * 27.0 <= 2.0 * (right - left) as f64 * 35.2,
        "{} × {}",
        right - left,
        bottom - top
    );
    let corner = |names: &[&str]| -> (i64, i64) {
        let cells: Vec<Cell> = names
            .iter()
            .map(|n| of(&b, &format!("Free/Many:{n}")))
            .collect();
        (
            cells.iter().map(|c| c.y).min().unwrap(),
            cells.iter().map(|c| c.x).min().unwrap(),
        )
    };
    let (a, set_b) = (corner(&["A1", "A2"]), corner(&["B1", "B2"]));
    assert!(a < set_b, "{a:?} {set_b:?}");
    for i in 0..60 {
        let m = of(&b, &format!("Free/Many:M{i:02}"));
        assert!((m.y, m.x) > set_b, "M{i:02} {m:?}");
    }
    apart(&b);
}

#[test]
fn cards_that_can_grow_go_last_and_keep_room_below() {
    let core = Core::open(script_store()).unwrap();
    ok(&core, "section_create", json!({"target": "Free/Grow"}));
    let held = |name: &str| {
        json!({"kind": "entity", "fields": [{"name": "id", "type": "string"}, {"name": "address", "type": "string"}],
            "connections": [{"line": "embed", "from": "address", "to": "Free/Grow:Address"}], "label": name})
    };
    ok(
        &core,
        "card_create",
        json!({"value": {
            "Free/Grow:Address": {"kind": "value", "fields": [{"name": "city", "type": "string"}]},
            "Free/Grow:Home": held("Home"),
            "Free/Grow:Work": held("Work"),
        }}),
    );
    let mut notes = serde_json::Map::new();
    for i in 0..6 {
        notes.insert(
            format!("Free/Grow:N{i}"),
            json!({"kind": "note", "body": format!("n{i}")}),
        );
    }
    ok(&core, "card_create", json!({"value": notes}));
    arrange(&core, "Free/Grow", json!({"by": "lines"}));
    let b = boxes(&core, "Free/Grow");
    assert!(!b.contains_key("Free/Grow:Address"), "{b:?}");
    for grows in ["Free/Grow:Home", "Free/Grow:Work"] {
        let g = of(&b, grows);
        for i in 0..6 {
            let n = of(&b, &format!("Free/Grow:N{i}"));
            assert!((g.y, g.x) > (n.y, n.x), "{grows} {g:?} N{i} {n:?}");
        }
        for (path, c) in &b {
            if path != grows && c.y > g.y && c.x < g.right() + 1 && g.x < c.right() + 1 {
                assert!(c.y >= g.y + 20, "{path} {c:?} under {grows} {g:?}");
            }
        }
    }
    apart(&b);
}

/// The free section's map with the group Auth (A and B, A pointing at B, and the group Session
/// holding C) and D outside it pointing at A; the section's own note out of the way.
fn auth() -> Core {
    let core = Core::open(script_store()).unwrap();
    ok(
        &core,
        "card_create",
        json!({"value": {
            "Free:B": entity(&["id"], json!([])),
            "Free:A": entity(&["id", "b"], json!([{"line": "points", "from": "b", "to": "Free:B"}])),
            "Free:C": entity(&["id"], json!([])),
            "Free:D": entity(&["a"], json!([{"line": "points", "from": "a", "to": "Free:A"}])),
        }}),
    );
    ok(
        &core,
        "section_update",
        json!({"target": "Free", "value": {"places": {
            "Free:Memo": {"topLeft": {"x": 300, "y": 300}},
            "Free:A": {"topLeft": {"x": 2, "y": 3}},
            "Free:B": {"topLeft": {"x": 20, "y": 3}},
            "Free:C": {"topLeft": {"x": 4, "y": 16}},
            "Free:D": {"topLeft": {"x": 60, "y": 0}},
        }}}),
    );
    ok(
        &core,
        "section_update",
        json!({"target": "Free", "value": {"groups": {
            "Auth": {"topLeft": {"x": 0, "y": 0}, "size": {"w": 40, "h": 30}},
            "Session": {"topLeft": {"x": 2, "y": 13}, "size": {"w": 16, "h": 10}},
        }}}),
    );
    core
}

fn groups(core: &Core) -> Value {
    ok(
        core,
        "section_read",
        json!({"target": "Free", "value": {"include": ["groups"]}}),
    )["groups"]
        .clone()
}

fn frame(groups: &Value, name: &str) -> Cell {
    let g = &groups[name];
    Cell {
        x: g["topLeft"]["x"].as_i64().unwrap(),
        y: g["topLeft"]["y"].as_i64().unwrap(),
        w: g["size"]["w"].as_i64().unwrap(),
        h: g["size"]["h"].as_i64().unwrap(),
    }
}

/// That a frame holds `inside` with a cell of room left, right and below, two above.
fn fits(frame: Cell, inside: &[Cell]) {
    let left = inside.iter().map(|c| c.x).min().unwrap();
    let top = inside.iter().map(|c| c.y).min().unwrap();
    let right = inside.iter().map(Cell::right).max().unwrap();
    let bottom = inside.iter().map(Cell::bottom).max().unwrap();
    assert_eq!(
        (frame.x, frame.y, frame.right(), frame.bottom()),
        (left - 1, top - 2, right + 1, bottom + 1),
        "{frame:?} {inside:?}"
    );
}

#[test]
fn a_group_is_arranged_inside_then_placed_whole_with_its_frame() {
    let core = auth();
    let said = arrange(&core, "Free", json!({"by": "lines"}));
    let b = boxes(&core, "Free");
    let read = groups(&core);
    let (auth, session) = (frame(&read, "Auth"), frame(&read, "Session"));
    fits(session, &[of(&b, "Free:C")]);
    fits(auth, &[of(&b, "Free:A"), of(&b, "Free:B"), session]);
    let members = read["Auth"]["members"].as_array().unwrap();
    assert!(
        members.contains(&json!("Free:A")) && members.contains(&json!("Free:B")),
        "{read}"
    );
    assert_eq!(read["Auth"]["groups"], json!(["Session"]), "{read}");
    assert!(
        read["Session"]["members"]
            .as_array()
            .unwrap()
            .contains(&json!("Free:C")),
        "{read}"
    );
    // D a layer after Auth: right of it in its band, or in a band below when the two are wider
    // than the map is packed across.
    let d = of(&b, "Free:D");
    assert!(
        (d.y == auth.y && auth.right() <= d.x) || auth.bottom() <= d.y,
        "{auth:?} {b:?}"
    );
    for name in ["Auth", "Session"] {
        let row = said["arranged"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["group"] == name)
            .unwrap_or_else(|| panic!("{name}: {said}"));
        assert!(row["size"].is_object(), "{said}");
    }
}

#[test]
fn arranging_inside_a_group_leaves_the_rest_and_pushes_down_what_the_frame_now_covers() {
    let core = Core::open(script_store()).unwrap();
    ok(
        &core,
        "card_create",
        json!({"value": {
            "Free:B": entity(&["id", "name"], json!([])),
            "Free:C": entity(&["id", "name"], json!([])),
            "Free:A": entity(&["b", "c"], json!([
                {"line": "points", "from": "b", "to": "Free:B"},
                {"line": "points", "from": "c", "to": "Free:C"}])),
            "Free:E": {"kind": "note", "body": "e"},
            "Free:F": {"kind": "note", "body": "f"},
        }}),
    );
    // A, B and C in the frame however they overlap, as a hand may leave them; E under it, F beside.
    let put: Vec<Value> = [
        ("A", 1, 2),
        ("B", 8, 2),
        ("C", 3, 5),
        ("E", 0, 11),
        ("F", 40, 0),
        ("Memo", 300, 300),
    ]
    .into_iter()
    .map(|(name, x, y)| json!({"item": card_id(&core, name), "x": x, "y": y}))
    .collect();
    let said = op(
        &core,
        json!({"op": "places", "project": project_id(&core), "boxes": put}),
    );
    assert!(said.get("error").is_none(), "{said}");
    ok(
        &core,
        "section_update",
        json!({"target": "Free", "value": {"groups": {
            "Auth": {"topLeft": {"x": 0, "y": 0}, "size": {"w": 14, "h": 10}}}}}),
    );
    let said = arrange(&core, "Free", json!({"by": "lines", "group": "Auth"}));
    let b = boxes(&core, "Free");
    let auth = frame(&groups(&core), "Auth");
    assert_eq!((auth.x, auth.y), (0, 0));
    let a = of(&b, "Free:A");
    for left in ["Free:B", "Free:C"] {
        assert!(of(&b, left).right() <= a.x, "{left} {b:?}");
    }
    let e = of(&b, "Free:E");
    assert_eq!(e.x, 0);
    assert!(e.y > auth.bottom(), "{e:?} {auth:?}");
    assert_eq!(
        (of(&b, "Free:F").x, of(&b, "Free:F").y),
        (40, 0),
        "{auth:?}"
    );
    let rows = named(&said);
    for want in [
        ("card", "Free:A"),
        ("card", "Free:B"),
        ("card", "Free:C"),
        ("group", "Auth"),
        ("card", "Free:E"),
    ] {
        assert!(
            rows.contains(&(want.0.to_string(), want.1.to_string())),
            "{want:?} {said}"
        );
    }
    assert!(!rows.iter().any(|(_, p)| p == "Free:F"), "{said}");
}

/// The id a card of the free section is kept by, in main.
fn card_id(core: &Core, name: &str) -> String {
    rusqlite::Connection::open(core.place())
        .unwrap()
        .query_row(
            "SELECT c.id FROM card_docs c JOIN section_docs s ON s.id = c.section_id AND s.branch_id = ''
              WHERE c.branch_id = '' AND s.name = 'Free' AND c.name = ?",
            [name],
            |r| r.get(0),
        )
        .unwrap()
}

/// The id a section under Free is kept by, in main.
fn section_id(core: &Core, name: &str) -> String {
    rusqlite::Connection::open(core.place())
        .unwrap()
        .query_row(
            "SELECT s.id FROM section_docs s JOIN section_docs p ON p.id = s.parent_id AND p.branch_id = ''
              WHERE s.branch_id = '' AND p.name = 'Free' AND s.name = ?",
            [name],
            |r| r.get(0),
        )
        .unwrap()
}

#[test]
fn ghosts_are_arranged_as_boxes_and_lines_land_on_them() {
    let core = Core::open(script_store()).unwrap();
    ok(
        &core,
        "section_create",
        json!({"target": ["Free/Lab", "Free/Shop", "Free/Store"]}),
    );
    ok(
        &core,
        "card_create",
        json!({"value": {
            "Free/Shop:Item": entity(&["id"], json!([])),
            "Free/Store:Stock": entity(&["id"], json!([])),
        }}),
    );
    ok(
        &core,
        "card_create",
        json!({"value": {
            "Free/Lab:A": entity(&["item", "stock"], json!([
                {"line": "points", "from": "item", "to": "Free/Shop:Item"},
                {"line": "points", "from": "stock", "to": "Free/Store:Stock"}])),
            "Free/Lab:G": {"kind": "ghost", "target": "Free/Store:Stock"},
        }}),
    );
    // The section ghost as the window puts it, right of A; the card ghost there too.
    let said = op(
        &core,
        json!({"op": "places", "project": project_id(&core), "boxes": [
            {"view": section_id(&core, "Lab"), "kind": "section", "item": section_id(&core, "Shop"), "x": 40, "y": 0},
        ]}),
    );
    assert!(said.get("error").is_none(), "{said}");
    ok(
        &core,
        "section_update",
        json!({"target": "Free/Lab", "value": {"places": {
            "Free/Lab:A": {"topLeft": {"x": 0, "y": 0}},
            "Free/Lab:G": {"topLeft": {"x": 40, "y": 20}},
        }}}),
    );
    let said = arrange(&core, "Free/Lab", json!({"by": "lines"}));
    let b = boxes(&core, "Free/Lab");
    let a = of(&b, "Free/Lab:A");
    for left in ["Free/Shop", "Free/Lab:G"] {
        assert!(of(&b, left).right() <= a.x, "{left} {b:?}");
    }
    let rows = named(&said);
    assert!(
        rows.contains(&("sectionGhost".to_string(), "Free/Shop".to_string())),
        "{said}"
    );
    assert!(
        rows.contains(&("card".to_string(), "Free/Lab:G".to_string())),
        "{said}"
    );
    apart(&b);
}

#[test]
fn the_same_map_is_arranged_the_same_way_every_time() {
    let core = shop();
    let first = arrange(&core, "Free/Shop", json!({"by": "lines"}));
    assert!(!first["arranged"].as_array().unwrap().is_empty());
    let after = boxes(&core, "Free/Shop");
    let second = arrange(&core, "Free/Shop", json!({"by": "lines"}));
    assert_eq!(second["arranged"], json!([]));
    assert_eq!(boxes(&core, "Free/Shop"), after);
    // Put elsewhere, every box in another place, then arranged from the same top-left.
    let mut places = serde_json::Map::new();
    for (n, path) in SHOP.iter().rev().enumerate() {
        places.insert(
            path.to_string(),
            json!({"topLeft": {"x": 200 + 30 * n as i64, "y": 100 + 7 * (n as i64 % 3)}}),
        );
    }
    ok(
        &core,
        "section_update",
        json!({"target": "Free/Shop", "value": {"places": places}}),
    );
    assert_ne!(boxes(&core, "Free/Shop"), after);
    let left = after.values().map(|c| c.x).min().unwrap();
    let top = after.values().map(|c| c.y).min().unwrap();
    arrange(
        &core,
        "Free/Shop",
        json!({"by": "lines", "at": {"x": left, "y": top}}),
    );
    assert_eq!(boxes(&core, "Free/Shop"), after);
}

#[test]
fn what_by_lines_cannot_take_is_refused() {
    let core = shop();
    let mut tree = ok(&core, "project_read", json!({}))["projectTree"].clone();
    tree["main_sections"]
        .as_array_mut()
        .unwrap()
        .push(json!({"type": "ui", "name": "UI"}));
    ok(
        &core,
        "project_update",
        json!({"value": {"project_tree": tree}}),
    );
    ok(&core, "section_create", json!({"target": "UI/Home"}));
    ok(
        &core,
        "card_create",
        json!({"value": {
            "UI/Home:Title": {"kind": "html", "body": "<h1>a</h1>", "topLeft": {"x": 0, "y": 0}},
            "ts:One": {"kind": "note", "body": "one", "topLeft": {"x": 0, "y": 0}},
        }}),
    );
    ok(
        &core,
        "section_update",
        json!({"target": "Free", "value": {"groups": {"Auth": {"topLeft": {"x": 500, "y": 500}, "size": {"w": 12, "h": 6}}}}}),
    );
    let before: Vec<_> = ["Free/Shop", "Free", "UI/Home", "ts"]
        .into_iter()
        .map(|s| boxes(&core, s))
        .collect();
    let cases = [
        (
            "Free/Shop",
            json!({"by": "lines", "cards": ["Free/Shop:Order"]}),
            "cards",
        ),
        (
            "Free/Shop",
            json!({"by": "lines", "region": {"from": {"x": 0, "y": 0}, "to": {"x": 9, "y": 9}}}),
            "region",
        ),
        ("Free/Shop", json!({"by": "lines", "columns": 2}), "columns"),
        (
            "Free",
            json!({"by": "lines", "group": "Auth", "at": {"x": 0, "y": 0}}),
            "at",
        ),
        ("Free", json!({"by": "lines", "group": "Nope"}), "Nope"),
        ("UI/Home", json!({"by": "lines"}), "UI/Home"),
        ("ts", json!({"by": "lines"}), "ts"),
    ];
    for (section, value, says) in cases {
        let why = refused(
            &core,
            "section_update",
            json!({"target": section, "value": {"arrange": value}}),
        );
        assert!(why.contains(says), "{section} {value}: {why}");
    }
    let after: Vec<_> = ["Free/Shop", "Free", "UI/Home", "ts"]
        .into_iter()
        .map(|s| boxes(&core, s))
        .collect();
    assert_eq!(after, before);
}

#[test]
fn two_thousand_cards_and_four_thousand_lines_are_arranged_within_the_budget() {
    // A fixed seed: the same map every run.
    let mut seed: u64 = 0x5eed_2000_4000;
    let mut next = |below: u64| {
        seed = seed
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (seed >> 33) % below
    };
    let sizes: Vec<(i64, i64)> = (0..2000).map(|_| (10, 3 + next(6) as i64)).collect();
    let pointers: Vec<(usize, usize)> = (0..4000)
        .map(|_| (next(2000) as usize, next(2000) as usize))
        .collect();
    let started = std::time::Instant::now();
    let at = specboard_core::arranged_by_pointers(&sizes, &pointers, 1);
    let took = started.elapsed();
    println!(
        "arranging 2,000 cards and 4,000 lines took {} ms",
        took.as_millis()
    );
    assert!(took.as_secs_f64() < 2.0, "{took:?}");
    // No two overlap: swept left to right, each against those whose span reaches it.
    let mut order: Vec<usize> = (0..sizes.len()).collect();
    order.sort_by_key(|&i| at[i].0);
    for (n, &i) in order.iter().enumerate() {
        let (x, y) = at[i];
        let (w, h) = sizes[i];
        for &j in &order[n + 1..] {
            let (ox, oy) = at[j];
            if ox >= x + w {
                break;
            }
            let (ow, oh) = sizes[j];
            assert!(
                !(oy < y + h && y < oy + oh && ox < x + w && x < ox + ow),
                "{i} {:?} {j} {:?}",
                (x, y, w, h),
                (ox, oy, ow, oh)
            );
        }
    }
}
