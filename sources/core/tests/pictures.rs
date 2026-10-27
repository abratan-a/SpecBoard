//! `section_read`'s and `card_read`'s `as: picture`: the core checks the call and works out which map
//! of what — a section's section map, a flow's node diagram, a goal's or container's content map, any
//! other card's connections map — and the app draws it (`specboard_core::App::picture`). The answer
//! is the picture and what it is, headed by the core; whatever keeps the app from drawing, the
//! refusal offers the same call as `as: diagram`.

use crate::common;

use common::{headers, op, project_id, script_store};
use serde_json::{Value, json};
use specboard_core::{App, Core};
use std::sync::{Arc, Mutex};

/// An app that answers every picture as the test says, and keeps what it was asked.
struct Painter {
    asked: Mutex<Vec<Value>>,
    answer: Mutex<String>,
}

impl App for Painter {
    fn app_control(&self, _arguments: &str, _caller: &str) -> String {
        panic!("a picture does not go through app_control")
    }

    fn picture(&self, request: &str) -> String {
        self.asked
            .lock()
            .unwrap()
            .push(serde_json::from_str(request).unwrap());
        self.answer.lock().unwrap().clone()
    }
}

/// What a drawn picture is said to be by the app: as wide as the screen draws it.
fn drawn(note: Value) -> String {
    json!({"image": {"data": "iVBORw0KGgo=", "mimeType": "image/png"}, "note": note}).to_string()
}

fn painted() -> (Core, Arc<Painter>) {
    let core = Core::open(script_store()).unwrap();
    let painter = Arc::new(Painter {
        asked: Mutex::new(Vec::new()),
        // Its keys in an order of their own: what Swift writes a dictionary in is not kept.
        answer: Mutex::new(drawn(json!({
            "cards": 3, "scale": 1, "height": 800, "width": 1200, "picture": "sectionMap"
        }))),
    });
    core.set_app(painter.clone());
    (core, painter)
}

/// A read called through MCP on the sample's project: its result (`content`, `isError`).
fn read(core: &Core, tool: &str, mut arguments: Value) -> Value {
    arguments["project"] = "見本".into();
    let message = json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call",
        "params": {"name": tool, "arguments": arguments}});
    let reply = op(
        core,
        json!({"op": "mcp", "message": message, "headers": headers(core)}),
    );
    reply["result"]["body"]["result"].clone()
}

/// What a picture is said to be (the text beside the image).
fn note_of(result: &Value) -> Value {
    assert_eq!(result["isError"], false, "{result}");
    assert_eq!(
        result["content"][0],
        json!({"type": "image", "data": "iVBORw0KGgo=", "mimeType": "image/png"})
    );
    serde_json::from_str(result["content"][1]["text"].as_str().unwrap()).unwrap()
}

/// A refusal as MCP answers it: `{message, guide?, use?}`.
fn refusal_of(result: &Value) -> Value {
    assert_eq!(result["isError"], true, "{result}");
    let text: Value = serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap();
    text["error"].clone()
}

fn message_of(result: &Value) -> String {
    refusal_of(result)["message"].as_str().unwrap().to_string()
}

/// The id of a card or section of the sample's project, by its name.
fn id_of(core: &Core, table: &str, name: &str) -> String {
    rusqlite::Connection::open(core.place())
        .unwrap()
        .query_row(
            &format!("SELECT id FROM {table} WHERE name = ? AND branch_id = '' LIMIT 1"),
            [name],
            |r| r.get(0),
        )
        .unwrap()
}

fn last(painter: &Painter) -> Value {
    painter.asked.lock().unwrap().last().cloned().unwrap()
}

#[test]
fn a_section_is_drawn_as_its_section_map() {
    let (core, painter) = painted();
    let result = read(
        &core,
        "section_read",
        json!({"target": "Schemas/Ordering", "value": {"as": "picture"}}),
    );
    let note = note_of(&result);
    assert_eq!(
        serde_json::to_string(&note).unwrap(),
        r#"{"project":"見本","branch":"main","path":"Schemas/Ordering","picture":"sectionMap","width":1200,"height":800,"scale":1,"cards":3}"#
    );
    assert_eq!(
        last(&painter),
        json!({
            "picture": "sectionMap",
            "project": project_id(&core),
            "tree": "",
            "section": id_of(&core, "section_docs", "Ordering"),
            "margin": 2,
            "maxSize": 1600,
            "lang": "ja",
        })
    );
}

#[test]
fn a_section_map_is_cut_to_some_cards_or_an_area() {
    let (core, painter) = painted();
    // A card's bare name is read in the section drawn.
    let result = read(
        &core,
        "section_read",
        json!({"target": "Schemas/Ordering", "value": {"as": "picture", "cards": ["Order", "Schemas:Customer"], "margin": 0, "maxSize": 3000}}),
    );
    note_of(&result);
    let asked = last(&painter);
    assert_eq!(
        asked["cards"],
        json!([
            id_of(&core, "card_docs", "Order"),
            id_of(&core, "card_docs", "Customer")
        ])
    );
    assert_eq!(
        (asked["margin"].clone(), asked["maxSize"].clone()),
        (json!(0), json!(3000))
    );
    let result = read(
        &core,
        "section_read",
        json!({"target": "Schemas/Ordering", "value": {"as": "picture", "area": {"topLeft": {"x": -4, "y": 0}, "bottomRight": {"x": 20.5, "y": 10}}}}),
    );
    note_of(&result);
    assert_eq!(
        last(&painter)["area"],
        json!({"topLeft": {"x": -4, "y": 0}, "bottomRight": {"x": 20, "y": 10}})
    );
    for (value, said) in [
        (
            json!({"as": "picture", "cards": ["Order"], "area": {"topLeft": {"x": 0, "y": 0}, "bottomRight": {"x": 1, "y": 1}}}),
            "cards か area のどちらか 1 つ",
        ),
        (
            json!({"as": "picture", "area": {"topLeft": {"x": 5, "y": 0}, "bottomRight": {"x": 1, "y": 1}}}),
            "左上が右下より左・上",
        ),
        (json!({"as": "picture", "cards": ["Nowhere"]}), "Nowhere"),
        (
            json!({"as": "picture", "maxSize": 399}),
            "maxSize は 400〜3000",
        ),
        (json!({"as": "picture", "margin": -1}), "margin は 0〜100"),
    ] {
        let result = read(
            &core,
            "section_read",
            json!({"target": "Schemas/Ordering", "value": value}),
        );
        let message = message_of(&result);
        assert!(message.contains(said), "{value}: {message}");
    }
}

#[test]
fn an_area_past_the_grids_reach_is_refused_before_the_app_is_asked() {
    let (core, painter) = painted();
    let area = |left: f64, top: f64, right: f64, bottom: f64| json!({"as": "picture", "area": {"topLeft": {"x": left, "y": top}, "bottomRight": {"x": right, "y": bottom}}});
    for value in [
        area(0.0, 0.0, 1e300, 0.0),
        area(-1e300, 0.0, 0.0, 0.0),
        area(0.0, 0.0, 0.0, 1_000_001.0),
    ] {
        let result = read(
            &core,
            "section_read",
            json!({"target": "Schemas/Ordering", "value": value}),
        );
        let message = message_of(&result);
        assert!(message.contains("±1,000,000"), "{value}: {message}");
    }
    assert!(painter.asked.lock().unwrap().is_empty());
    let result = read(
        &core,
        "section_read",
        json!({"target": "Schemas/Ordering", "value": area(-1e6, -1e6, 1e6, 1e6)}),
    );
    note_of(&result);
    assert_eq!(
        last(&painter)["area"],
        json!({"topLeft": {"x": -1_000_000, "y": -1_000_000}, "bottomRight": {"x": 1_000_000, "y": 1_000_000}})
    );
}

#[test]
fn each_card_is_drawn_as_the_map_the_screen_shows_it_in() {
    let (core, painter) = painted();
    // A flow: its node diagram, headed with the card's rev.
    let note = note_of(&read(
        &core,
        "card_read",
        json!({"target": "Charts/Shop:Checkout", "value": {"as": "picture"}}),
    ));
    let rev = note["rev"].as_str().unwrap().to_string();
    assert_eq!(rev.len(), 12, "{note}");
    assert_eq!(note["path"], "Charts/Shop:Checkout");
    assert_eq!(
        last(&painter),
        json!({
            "picture": "flowDiagram",
            "project": project_id(&core),
            "tree": "",
            "card": id_of(&core, "card_docs", "Checkout"),
            "maxSize": 1600,
            "lang": "ja",
        })
    );
    let read_rev = common::ok(
        &core,
        "card_read",
        json!({"target": "Charts/Shop:Checkout"}),
    )["rev"]
        .clone();
    assert_eq!(read_rev, rev.as_str());
    // Any other card: its connections map, a line out by default, as far as depth says.
    note_of(&read(
        &core,
        "card_read",
        json!({"target": "Order", "value": {"as": "picture", "section": "Schemas/Ordering"}}),
    ));
    let asked = last(&painter);
    assert_eq!(asked["picture"], "connectionsMap");
    assert_eq!(asked["card"], id_of(&core, "card_docs", "Order"));
    assert_eq!(asked["depth"], 1);
    note_of(&read(
        &core,
        "card_read",
        json!({"target": "Schemas/Ordering:Order", "value": {"as": "picture", "depth": 0}}),
    ));
    assert_eq!(last(&painter)["depth"], 0);
    let message = message_of(&read(
        &core,
        "card_read",
        json!({"target": "Schemas/Ordering:Order", "value": {"as": "picture", "depth": 3}}),
    ));
    assert!(message.contains("depth は 1・2、0 で全部"), "{message}");
    // A goal on the board: its content map, cut as a section map is.
    let note = note_of(&read(
        &core,
        "card_read",
        json!({"branch": "_project_board_", "target": "Goals:Ship", "value": {"as": "picture", "cards": ["Goals:Ship.Design"]}}),
    ));
    assert_eq!(
        (note["branch"].clone(), note["path"].clone()),
        (json!("_project_board_"), json!("Goals:Ship"))
    );
    let asked = last(&painter);
    assert_eq!(asked["picture"], "contentMap");
    assert_eq!(asked["tree"], "~board");
    assert_eq!(asked["margin"], 2);
    assert_eq!(asked["cards"].as_array().map(Vec::len), Some(1), "{asked}");
}

#[test]
fn the_kinds_of_line_are_read_as_as_diagram_reads_them() {
    let (core, painter) = painted();
    let message = message_of(&read(
        &core,
        "section_read",
        json!({"target": "Schemas/Ordering", "value": {"as": "picture", "kinds": ["nothing"]}}),
    ));
    let diagram = message_of(&read(
        &core,
        "section_read",
        json!({"target": "Schemas/Ordering", "value": {"as": "diagram", "kinds": ["nothing"]}}),
    ));
    assert_eq!(message, diagram);
    // One that is there, by the name MCP says it (the drawing keeps `points` as `lookup`).
    assert!(diagram.contains("points"), "{diagram}");
    note_of(&read(
        &core,
        "section_read",
        json!({"target": "Schemas/Ordering", "value": {"as": "picture", "kinds": ["next", "points"]}}),
    ));
    assert_eq!(last(&painter)["kinds"], json!(["points", "next"]));
}

#[test]
fn what_is_not_one_card_or_one_section_is_refused_with_how_it_is_read() {
    let (core, painter) = painted();
    for (target, said) in [
        (
            json!("Schemas/Ordering:Order.total"),
            "Schemas/Ordering:Order の欄",
        ),
        (json!("Docs:Spec.intro"), "Docs:Spec の行"),
        (
            json!("Charts/Shop:Checkout.save"),
            "flow Charts/Shop:Checkout のノード",
        ),
        (json!("Schemas:Status.paid"), "enum Schemas:Status の値"),
        (
            json!("Charts/Shop:Checkout.save.steps[0]"),
            "その flow（Charts/Shop:Checkout）を描きます",
        ),
        (
            json!(["Schemas:Status", "Schemas:Customer"]),
            "カード 1 枚ずつです（target が 2 個）",
        ),
    ] {
        let message = message_of(&read(
            &core,
            "card_read",
            json!({"target": target, "value": {"as": "picture"}}),
        ));
        assert!(message.contains(said), "{target}: {message}");
    }
    // A card ghost: the call that draws the card it leads to.
    common::ok(
        &core,
        "card_create",
        json!({"target": "Charts:OrderThere", "value": {"kind": "ghost", "target": "Schemas/Ordering:Order"}}),
    );
    let refusal = refusal_of(&read(
        &core,
        "card_read",
        json!({"target": "Charts:OrderThere", "value": {"as": "picture", "maxSize": 800}}),
    ));
    assert!(
        refusal["message"]
            .as_str()
            .unwrap()
            .contains("Schemas/Ordering:Order"),
        "{refusal}"
    );
    assert_eq!(
        refusal["use"],
        json!({"tool": "card_read", "arguments": {"target": "Schemas/Ordering:Order", "value": {"as": "picture", "maxSize": 800}, "project": "見本"}})
    );
    assert!(painter.asked.lock().unwrap().is_empty());
}

#[test]
fn keys_a_map_does_not_take_are_refused() {
    let (core, painter) = painted();
    for (tool, target, value, said) in [
        (
            "card_read",
            "Charts/Shop:Checkout",
            json!({"as": "picture", "depth": 2}),
            "depth を取るのはコネクションマップ",
        ),
        (
            "card_read",
            "Schemas/Ordering:Order",
            json!({"as": "picture", "cards": ["Schemas:Customer"]}),
            "cards を取るのはセクションマップとコンテンツマップだけ",
        ),
        (
            "card_read",
            "Charts/Shop:Checkout",
            json!({"as": "picture", "area": {"topLeft": {"x": 0, "y": 0}, "bottomRight": {"x": 1, "y": 1}}, "margin": 1}),
            "area・margin を取るのは",
        ),
        (
            "card_read",
            "Charts/Shop:Checkout",
            json!({"as": "picture", "kinds": ["next"]}),
            "flow のノードの図は kinds を取りません",
        ),
        (
            "card_read",
            "Schemas/Ordering:Order",
            json!({"as": "picture", "roots": ["Order"]}),
            "roots",
        ),
        (
            "card_read",
            "Charts/Shop:Checkout",
            json!({"as": "picture", "shape": "sequence"}),
            "shape",
        ),
        (
            "section_read",
            "Schemas/Ordering",
            json!({"as": "picture", "depth": 2}),
            "depth",
        ),
        (
            "section_read",
            "Schemas/Ordering",
            json!({"as": "picture", "tags": ["x"]}),
            "tags",
        ),
    ] {
        let message = message_of(&read(
            &core,
            tool,
            json!({"target": target, "value": value}),
        ));
        assert!(message.contains(said), "{tool} {value}: {message}");
    }
    assert!(painter.asked.lock().unwrap().is_empty());
}

#[test]
fn with_no_app_the_same_call_as_a_diagram_is_offered() {
    let core = Core::open(script_store()).unwrap();
    let refusal = refusal_of(&read(
        &core,
        "section_read",
        json!({"target": "Schemas/Ordering", "value": {"as": "picture", "kinds": ["points"], "cards": ["Order"], "margin": 1, "maxSize": 900}}),
    ));
    assert_eq!(
        refusal["message"],
        "絵を描けません: 描く SpecBoard のアプリが繋いでいません。絵は SpecBoard のアプリが描きます。as: diagram なら同じ図を文字（mermaid）で読めます"
    );
    assert_eq!(
        refusal["use"],
        json!({"tool": "section_read", "arguments": {"target": "Schemas/Ordering", "value": {"as": "diagram", "kinds": ["points"]}, "project": "見本"}})
    );
    let refusal = refusal_of(&read(
        &core,
        "card_read",
        json!({"target": "Charts/Shop:Checkout", "value": {"as": "picture", "maxSize": 900}, "branch": "main"}),
    ));
    assert_eq!(
        refusal["use"],
        json!({"tool": "card_read", "arguments": {"target": "Charts/Shop:Checkout", "value": {"as": "diagram"}, "branch": "main", "project": "見本"}})
    );
    // The call it offers is answered.
    let (failed, _) = common::call(&core, "card_read", refusal["use"]["arguments"].clone());
    assert!(!failed);
}

#[test]
fn what_keeps_the_app_from_drawing_is_said_with_the_diagram_offered() {
    let (core, painter) = painted();
    for (answer, said) in [
        (
            json!({"error": "そのセクションは消えました"}).to_string(),
            "絵を描けません: そのセクションは消えました。",
        ),
        (
            "not json".to_string(),
            "アプリの答えが読めません（not json）",
        ),
        (
            json!({"image": {"data": "x"}}).to_string(),
            "アプリの答えが読めません",
        ),
    ] {
        *painter.answer.lock().unwrap() = answer;
        let refusal = refusal_of(&read(
            &core,
            "card_read",
            json!({"target": "Schemas/Ordering:Order", "value": {"as": "picture", "depth": 2}}),
        ));
        assert!(
            refusal["message"].as_str().unwrap().contains(said),
            "{refusal}"
        );
        assert_eq!(
            refusal["use"]["arguments"]["value"],
            json!({"as": "diagram", "depth": 2})
        );
    }
}

#[test]
fn a_picture_drawn_small_says_how_to_read_it_larger() {
    let (core, painter) = painted();
    let small = |picture: &str| {
        drawn(json!({"picture": picture, "width": 1600, "height": 900, "scale": 0.3, "cards": 120}))
    };
    *painter.answer.lock().unwrap() = small("sectionMap");
    let note = note_of(&read(
        &core,
        "section_read",
        json!({"target": "Schemas", "value": {"as": "picture"}}),
    ));
    assert_eq!(
        note["hint"],
        "画面の 30% の大きさで描いたので、字が読みにくいかもしれません。cards・area で絞るか、maxSize を上げます（3000 まで）"
    );
    *painter.answer.lock().unwrap() = small("flowDiagram");
    let note = note_of(&read(
        &core,
        "card_read",
        json!({"target": "Charts/Shop:Checkout", "value": {"as": "picture"}}),
    ));
    assert_eq!(
        note["hint"],
        "画面の 30% の大きさで描いたので、字が読みにくいかもしれません。maxSize を上げます（3000 まで）"
    );
    *painter.answer.lock().unwrap() = drawn(
        json!({"picture": "flowDiagram", "width": 800, "height": 400, "scale": 0.5, "cards": 9}),
    );
    let note = note_of(&read(
        &core,
        "card_read",
        json!({"target": "Charts/Shop:Checkout", "value": {"as": "picture"}}),
    ));
    assert!(note.get("hint").is_none(), "{note}");
}

#[test]
fn reading_the_project_is_all_a_key_needs() {
    let (core, painter) = painted();
    let made = op(
        &core,
        json!({"op": "access_key_create", "name": "読むだけの係", "permits": {"app_control": "deny", "app_control.capture": "deny"}}),
    );
    let id = made["result"]["key"]["id"].as_str().unwrap();
    let value = made["result"]["value"].as_str().unwrap();
    let given = op(
        &core,
        json!({"op": "access_grant", "key": id, "project": project_id(&core), "level": "read"}),
    );
    assert_eq!(given, json!({"result": {}}));
    let message = json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call",
        "params": {"name": "section_read", "arguments": {"project": "見本", "target": "Schemas/Ordering", "value": {"as": "picture"}}}});
    let reply = op(
        &core,
        json!({"op": "mcp", "message": message, "headers": {"authorization": format!("Bearer {value}")}}),
    );
    note_of(&reply["result"]["body"]["result"]);
    assert_eq!(painter.asked.lock().unwrap().len(), 1);
}

#[test]
fn the_area_of_the_note_is_top_left_first_whatever_order_the_app_wrote() {
    let (core, painter) = painted();
    *painter.answer.lock().unwrap() = drawn(json!({
        "picture": "sectionMap",
        "area": {"bottomRight": {"y": 10, "x": 20}, "topLeft": {"y": 0, "x": -4}},
    }));
    let result = read(
        &core,
        "section_read",
        json!({"target": "Schemas/Ordering", "value": {"as": "picture"}}),
    );
    assert_eq!(result["isError"], false, "{result}");
    let text = result["content"][1]["text"].as_str().unwrap();
    let top = text.find("topLeft").unwrap();
    let bottom = text.find("bottomRight").unwrap();
    assert!(top < bottom, "{text}");
    for corner in ["topLeft", "bottomRight"] {
        let at = text.find(corner).unwrap();
        let rest = &text[at..];
        assert!(
            rest.find("\"x\"").unwrap() < rest.find("\"y\"").unwrap(),
            "{text}"
        );
    }
}
