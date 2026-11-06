//! The whiteboard (`specboard://SpecBoard/Docs/Design:CardKinds`「ホワイトボード（whiteboard）」): a section
//! where sketches (a shape, its text, its colours), notes and SVGs are joined by arrows that carry their
//! own label and look; sketches made with no place go in columns along their arrows, sized from their
//! text until sized by hand.

use crate::common;

use common::{checked, ok, op, project_id, refused, script_store};
use serde_json::{Value, json};
use specboard_core::Core;

fn core() -> Core {
    let core = Core::open(script_store()).unwrap();
    ok(
        &core,
        "section_create",
        json!({"target": "Free/Explain", "value": {"type": "whiteboard"}}),
    );
    core
}

/// Where a card is drawn: its top left and size.
fn at(core: &Core, path: &str) -> (i64, i64, i64, i64) {
    let read = ok(
        core,
        "card_read",
        json!({"target": path, "value": {"include": ["at"]}}),
    );
    (
        read["topLeft"]["x"].as_i64().unwrap(),
        read["topLeft"]["y"].as_i64().unwrap(),
        read["size"]["w"].as_i64().unwrap(),
        read["size"]["h"].as_i64().unwrap(),
    )
}

#[test]
fn a_whiteboard_is_made_where_documents_and_drafts_are_and_holds_sketches_notes_and_svgs() {
    let core = core();
    ok(
        &core,
        "section_create",
        json!({"target": "Docs/Board", "value": {"type": "whiteboard"}}),
    );
    let read = ok(&core, "section_read", json!({"target": "Free/Explain"}));
    assert_eq!(read["type"], "whiteboard", "{read}");
    ok(
        &core,
        "card_create",
        json!({"value": {
            "Free/Explain:App": {"kind": "sketch", "text": "アプリ"},
            "Free/Explain:Point": {"kind": "note", "body": "ここがポイント"},
            "Docs/Board:Pic": {"kind": "svg", "body": "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"10\" height=\"10\"/>"}
        }}),
    );
    let said = refused(
        &core,
        "card_create",
        json!({"target": "Free/Explain:Order", "value": {"kind": "entity"}}),
    );
    assert!(said.contains("ホワイトボード"), "{said}");
}

#[test]
fn a_sketch_keeps_its_shape_text_and_colours_and_is_a_blue_rounded_box_unless_told() {
    let core = core();
    ok(
        &core,
        "card_create",
        json!({"value": {
            "Free/Explain:Check": {"kind": "sketch", "shape": "diamond", "text": "キーは合う？",
                "fill": "amber", "stroke": "amber"},
            "Free/Explain:Plain": {"kind": "sketch", "text": "ふつう"}
        }}),
    );
    let check = ok(&core, "card_read", json!({"target": "Free/Explain:Check"}));
    assert_eq!(
        (
            &check["shape"],
            &check["text"],
            &check["fill"],
            &check["stroke"]
        ),
        (
            &json!("diamond"),
            &json!("キーは合う？"),
            &json!("amber"),
            &json!("amber")
        ),
        "{check}"
    );
    // Not written: the kind's defaults, which the screen draws it by.
    let plain = ok(&core, "card_read", json!({"target": "Free/Explain:Plain"}));
    assert!(plain.get("shape").is_none(), "{plain}");
    let kind = ok(&core, "kind_read", json!({"target": "sketch"}));
    let default = |name: &str| {
        kind["fields"]
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["name"] == name)
            .unwrap()["default"]
            .clone()
    };
    assert_eq!(
        (default("shape"), default("fill"), default("stroke")),
        (json!("rounded"), json!("blue"), json!("blue")),
        "{kind}"
    );
    for (key, value) in [("shape", "star"), ("fill", "pink")] {
        let said = refused(
            &core,
            "card_update",
            json!({"target": "Free/Explain:Plain", "value": {key: value}}),
        );
        assert!(said.contains(value), "{said}");
    }
}

#[test]
fn a_sketch_is_as_big_as_its_text_until_sized_by_hand() {
    let core = core();
    ok(
        &core,
        "card_create",
        json!({"value": {
            "Free/Explain:Short": {"kind": "sketch", "text": "DB", "topLeft": {"x": 0, "y": 0}},
            "Free/Explain:Long": {"kind": "sketch", "topLeft": {"x": 0, "y": 20},
                "text": "**エージェント**\nspecboard-agent serve でホストに立つ本体\nプロジェクトの DB とコアを持つ"}
        }}),
    );
    let (_, _, short_w, short_h) = at(&core, "Free/Explain:Short");
    let (_, _, long_w, long_h) = at(&core, "Free/Explain:Long");
    assert!(
        long_w > short_w && long_h > short_h,
        "{short_w}×{short_h} {long_w}×{long_h}"
    );
    // The window reads the same size.
    let whole = op(
        &core,
        json!({"op": "project_detail", "project": project_id(&core)}),
    )["result"]
        .clone();
    let long = whole["schemas"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "Long")
        .unwrap()
        .clone();
    assert_eq!(long["size"], json!({"w": long_w, "h": long_h}), "{long}");
    // Sized by hand: kept whatever the text says.
    ok(
        &core,
        "card_update",
        json!({"target": "Free/Explain:Short", "value": {"size": {"w": 8, "h": 4}}}),
    );
    ok(
        &core,
        "card_update",
        json!({"target": "Free/Explain:Short", "value": {"text": "とても長い字を書き足しても大きさは変わらない"}}),
    );
    let (_, _, w, h) = at(&core, "Free/Explain:Short");
    assert_eq!((w, h), (8, 4));
}

#[test]
fn an_arrow_carries_its_own_label_dash_ends_and_colour_and_joins_whiteboard_parts_only() {
    let core = core();
    ok(
        &core,
        "card_create",
        json!({"value": {
            "Free/Explain:App": {"kind": "sketch", "text": "アプリ"},
            "Free/Explain:Agent": {"kind": "sketch", "text": "エージェント",
                "connections": []},
        }}),
    );
    ok(
        &core,
        "card_update",
        json!({"target": "Free/Explain:App", "value": {"connections": {"add": [
            {"line": "arrow", "to": "Free/Explain:Agent", "fields": {"label": "SSH の上で attach", "dashed": true}}
        ]}}}),
    );
    let read = ok(
        &core,
        "card_read",
        json!({"target": "Free/Explain:App", "value": {"include": ["connections"]}}),
    );
    assert_eq!(
        read["connections"][0]["fields"]["label"], "SSH の上で attach",
        "{read}"
    );
    // The window reads each arrow's own look with it.
    let whole = op(
        &core,
        json!({"op": "project_detail", "project": project_id(&core)}),
    )["result"]
        .clone();
    let arrow = whole["relations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["line"] == "arrow")
        .unwrap_or_else(|| panic!("{}", whole["relations"]))
        .clone();
    assert_eq!(arrow["fields"]["label"], "SSH の上で attach", "{arrow}");
    assert_eq!(arrow["fields"]["dashed"], true, "{arrow}");
    // Only between the whiteboard's parts.
    let said = refused(
        &core,
        "card_update",
        json!({"target": "Free/Explain:App", "value": {"connections": {"add": [
            {"line": "arrow", "to": "Schemas:Customer"}
        ]}}}),
    );
    assert!(said.contains("arrow"), "{said}");
    let said = refused(
        &core,
        "card_update",
        json!({"target": "Free/Explain:App", "value": {"connections": {"add": [
            {"line": "arrow", "to": "Free/Explain:Agent", "fields": {"ends": "sideways"}}
        ]}}}),
    );
    assert!(said.contains("sideways"), "{said}");
    // Notes elsewhere are not a whiteboard's parts.
    ok(
        &core,
        "card_create",
        json!({"value": {
            "Docs:One": {"kind": "note", "body": "一"},
            "Docs:Two": {"kind": "note", "body": "二"}
        }}),
    );
    let said = refused(
        &core,
        "card_update",
        json!({"target": "Docs:One", "value": {"connections": {"add": [
            {"line": "arrow", "to": "Docs:Two"}
        ]}}}),
    );
    assert!(said.contains("ホワイトボード"), "{said}");
    let offered = ok(
        &core,
        "card_read",
        json!({"target": "Docs:One", "value": {"as": "lineKinds", "to": "Docs:Two"}}),
    )
    .to_string();
    assert!(!offered.contains("\"arrow\""), "{offered}");
}

#[test]
fn sketches_made_with_no_place_go_in_columns_along_their_arrows() {
    let core = core();
    ok(
        &core,
        "card_create",
        json!({"target": "Free/Explain:Kept", "value": {"kind": "sketch", "text": "前から", "topLeft": {"x": 60, "y": 40}}}),
    );
    let arrow = |to: &str| json!({"line": "arrow", "to": format!("Free/Explain:{to}")});
    ok(
        &core,
        "card_create",
        json!({"value": {
            "Free/Explain:App": {"kind": "sketch", "text": "アプリ", "connections": [arrow("Core"), arrow("Agent")]},
            "Free/Explain:Core": {"kind": "sketch", "text": "手元のコア"},
            "Free/Explain:Agent": {"kind": "sketch", "text": "エージェント", "connections": [arrow("DB")]},
            "Free/Explain:DB": {"kind": "sketch", "shape": "cylinder", "text": "DB"}
        }}),
    );
    let (app, core_, agent, db) = (
        at(&core, "Free/Explain:App"),
        at(&core, "Free/Explain:Core"),
        at(&core, "Free/Explain:Agent"),
        at(&core, "Free/Explain:DB"),
    );
    assert!(
        app.0 < core_.0 && core_.0 == agent.0 && agent.0 < db.0,
        "{app:?} {core_:?} {agent:?} {db:?}"
    );
    assert!(
        core_.1 < agent.1,
        "同じ列は作った順に上から: {core_:?} {agent:?}"
    );
    assert!(
        core_.1 + core_.3 <= agent.1,
        "重ならない: {core_:?} {agent:?}"
    );
    assert_eq!(at(&core, "Free/Explain:Kept").0, 60);
    // Arranged again along the arrows, every one of them: DB moved left of App comes back after Agent.
    ok(
        &core,
        "card_update",
        json!({"target": "Free/Explain:DB", "value": {"topLeft": {"x": 0, "y": 80}}}),
    );
    ok(
        &core,
        "section_update",
        json!({"target": "Free/Explain", "value": {"arrange": {"by": "lines"}}}),
    );
    let (app, agent, db) = (
        at(&core, "Free/Explain:App"),
        at(&core, "Free/Explain:Agent"),
        at(&core, "Free/Explain:DB"),
    );
    assert!(
        app.0 < agent.0 && agent.0 < db.0,
        "{app:?} {agent:?} {db:?}"
    );
}

#[test]
fn many_sketches_in_layers_are_placed_at_once() {
    let core = core();
    // Thirty layers of two, each sketch with an arrow to both of the next layer's: a way through
    // for every choice at every layer.
    let name = |layer: usize, i: usize| format!("Free/Explain:S{layer}x{i}");
    let mut value = serde_json::Map::new();
    for layer in 0..30 {
        for i in 0..2 {
            let connections: Vec<Value> = if layer < 29 {
                (0..2)
                    .map(|j| json!({"line": "arrow", "to": name(layer + 1, j)}))
                    .collect()
            } else {
                Vec::new()
            };
            value.insert(
                name(layer, i),
                json!({"kind": "sketch", "text": format!("{layer}"), "connections": connections}),
            );
        }
    }
    let started = std::time::Instant::now();
    ok(&core, "card_create", json!({"value": value}));
    assert!(
        started.elapsed() < std::time::Duration::from_secs(10),
        "{:?}",
        started.elapsed()
    );
    let (first, last) = (at(&core, &name(0, 0)), at(&core, &name(29, 1)));
    assert!(first.0 < last.0, "{first:?} {last:?}");
}

#[test]
fn a_long_line_of_text_wraps_and_the_sketch_is_as_tall_as_it_wraps() {
    let core = core();
    let long = "設計の考えを絵で説明するための長い一行。".repeat(5);
    ok(
        &core,
        "card_create",
        json!({"value": {
            "Free/Explain:Short": {"kind": "sketch", "text": "短い", "topLeft": {"x": 0, "y": 0}},
            "Free/Explain:Long": {"kind": "sketch", "text": long, "topLeft": {"x": 0, "y": 20}}
        }}),
    );
    let (_, _, _, short_h) = at(&core, "Free/Explain:Short");
    let (_, _, long_w, long_h) = at(&core, "Free/Explain:Long");
    assert_eq!(long_w, 14);
    assert!(long_h > short_h, "{short_h} {long_h}");
}

#[test]
fn a_link_in_a_sketchs_text_follows_a_rename_and_one_leading_nowhere_is_inspected() {
    let core = core();
    ok(
        &core,
        "card_create",
        json!({"value": {
            "Docs:Target": {"kind": "note", "body": "相手"},
            "Free/Explain:Agent": {"kind": "sketch", "text": "設計は [[Docs:Target]]、前は [[Docs:Nowhere]]"}
        }}),
    );
    ok(
        &core,
        "card_update",
        json!({"target": "Docs:Target", "value": {"name": "Moved"}}),
    );
    let read: Value = ok(&core, "card_read", json!({"target": "Free/Explain:Agent"}));
    assert_eq!(
        read["text"], "設計は [[Docs:Moved]]、前は [[Docs:Nowhere]]",
        "{read}"
    );
    let broken = checked(&core, "main")["brokenPaths"].to_string();
    assert!(broken.contains("Docs:Nowhere"), "{broken}");
}

// MARK: - Images

/// The head of a PNG `w` × `h`: what the core reads a picture's form and size from.
fn png(w: u32, h: u32) -> Vec<u8> {
    let mut out = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 13];
    out.extend_from_slice(b"IHDR");
    out.extend_from_slice(&w.to_be_bytes());
    out.extend_from_slice(&h.to_be_bytes());
    out.extend_from_slice(&[8, 6, 0, 0, 0, 0, 0, 0, 0]);
    out
}

fn b64(bytes: &[u8]) -> String {
    const ABC: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, b)| n | (*b as u32) << (16 - 8 * i));
        for i in 0..4 {
            out.push(if i <= chunk.len() {
                ABC[(n >> (18 - 6 * i) & 63) as usize] as char
            } else {
                '='
            });
        }
    }
    out
}

/// What `card_read` `as: image` answers through MCP: the picture's part, and the text after it.
fn image_part(core: &Core, path: &str) -> (Value, Value) {
    let message = json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": "card_read",
        "arguments": {"project": common::PROJECT, "target": path, "value": {"as": "image"}}}});
    let reply: Value = serde_json::from_str(&core.call(
        &json!({"op": "mcp", "message": message, "headers": common::headers(core)}).to_string(),
    ))
    .unwrap();
    let content = reply["result"]["body"]["result"]["content"].clone();
    (content[0].clone(), content[1].clone())
}

#[test]
fn an_image_is_kept_once_by_its_contents_and_read_back_whole() {
    let core = core();
    let data = b64(&png(800, 400));
    ok(
        &core,
        "card_create",
        json!({"value": {
            "Free/Explain:Screen": {"kind": "image", "imageData": data, "alt": "今の画面",
                "topLeft": {"x": 0, "y": 0}},
            "Free/Explain:Again": {"kind": "image", "imageData": data, "topLeft": {"x": 0, "y": 30}}
        }}),
    );
    let read = ok(&core, "card_read", json!({"target": "Free/Explain:Screen"}));
    assert_eq!(
        (
            &read["format"],
            &read["width"],
            &read["height"],
            &read["alt"]
        ),
        (&json!("png"), &json!(800), &json!(400), &json!("今の画面")),
        "{read}"
    );
    assert_eq!(read["bytes"], json!(png(800, 400).len()), "{read}");
    let hash = read["image"].as_str().unwrap().to_string();
    assert_eq!(hash.len(), 64, "{read}");
    let again = ok(&core, "card_read", json!({"target": "Free/Explain:Again"}));
    assert_eq!(again["image"], json!(hash));
    // Kept once: the project's store holds one picture for the two cards.
    let held: i64 = rusqlite::Connection::open(core.place())
        .unwrap()
        .query_row("SELECT count(*) FROM images", [], |r| r.get(0))
        .unwrap();
    assert_eq!(held, 1);
    // As wide as it is, 20 columns at the most, and as tall as its sides say.
    let (_, _, w, h) = at(&core, "Free/Explain:Screen");
    // 20 columns of 35.2 points are 704 points across, so half that down: 352 points, 14 rows.
    assert_eq!((w, h), (20, 14));
    // A tall picture's box goes no further than 100 rows down.
    ok(
        &core,
        "card_create",
        json!({"target": "Free/Explain:Tall", "value": {"kind": "image", "imageData": b64(&png(10, 16_000)),
            "topLeft": {"x": 40, "y": 0}}}),
    );
    let (_, _, _, tall) = at(&core, "Free/Explain:Tall");
    assert_eq!(tall, 100);
    // The picture itself.
    let (picture, note) = image_part(&core, "Free/Explain:Screen");
    assert_eq!(picture["type"], "image", "{picture}");
    assert_eq!(picture["mimeType"], "image/png");
    assert_eq!(picture["data"], json!(data));
    assert!(note["text"].as_str().unwrap().contains("800"), "{note}");
}

#[test]
fn only_png_jpeg_and_gif_of_five_megabytes_at_most_are_taken_and_the_core_writes_what_it_reads() {
    let core = core();
    let said = refused(
        &core,
        "card_create",
        json!({"target": "Free/Explain:Text", "value": {"kind": "image", "imageData": b64(b"hello, world")}}),
    );
    assert!(said.contains("PNG"), "{said}");
    let mut big = png(10, 10);
    big.resize(5 * 1024 * 1024 + 1, 0);
    let said = refused(
        &core,
        "card_create",
        json!({"target": "Free/Explain:Big", "value": {"kind": "image", "imageData": b64(&big)}}),
    );
    assert!(said.contains("5MB"), "{said}");
    // A picture of no size, or of a side past what is drawn, says nothing a box can show.
    for (w, h) in [(0, 10), (10, 0), (20_000, 10), (1, 4_000_000_000)] {
        let said = refused(
            &core,
            "card_create",
            json!({"target": "Free/Explain:Odd", "value": {"kind": "image", "imageData": b64(&png(w, h))}}),
        );
        assert!(said.contains("16384"), "{w}×{h}: {said}");
    }
    let said = refused(
        &core,
        "card_create",
        json!({"target": "Free/Explain:Hand", "value": {"kind": "image", "image": "0".repeat(64)}}),
    );
    assert!(said.contains("imageData"), "{said}");
    // Replaced: the card takes the new picture's form and size.
    ok(
        &core,
        "card_create",
        json!({"target": "Free/Explain:Pic", "value": {"kind": "image", "imageData": b64(&png(100, 50))}}),
    );
    let gif = [b"GIF89a".as_slice(), &[40, 0, 30, 0, 0, 0, 0]].concat();
    ok(
        &core,
        "card_update",
        json!({"target": "Free/Explain:Pic", "value": {"imageData": b64(&gif)}}),
    );
    let read = ok(&core, "card_read", json!({"target": "Free/Explain:Pic"}));
    assert_eq!(
        (&read["format"], &read["width"], &read["height"]),
        (&json!("gif"), &json!(40), &json!(30)),
        "{read}"
    );
}

#[test]
fn a_project_file_carries_its_images() {
    let core = core();
    ok(
        &core,
        "card_create",
        json!({"target": "Free/Explain:Screen", "value": {"kind": "image", "imageData": b64(&png(64, 32))}}),
    );
    let text = op(
        &core,
        json!({"op": "project_file", "project": project_id(&core)}),
    )["result"]["text"]
        .as_str()
        .unwrap()
        .to_string();
    let file: Value = serde_json::from_str(&text).unwrap();
    let images = file["images"]
        .as_array()
        .unwrap_or_else(|| panic!("{}", file["images"]));
    assert_eq!(images.len(), 1);
    // Read back as a copy: the picture comes with it.
    let copy = op(
        &core,
        json!({"op": "project_file_read", "text": text, "landing": "copy", "name": "写し"}),
    );
    assert!(copy["error"].is_null(), "{copy}");
    let conn = rusqlite::Connection::open(core.place()).unwrap();
    let datas: Vec<String> = conn
        .prepare("SELECT data FROM images")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(datas.len(), 2);
    assert_eq!(datas[0], datas[1], "写しの絵は同じ中身");
    // A file whose picture is not what its name says, or no picture at all, brings none of it.
    let mut forged = file.clone();
    forged["images"][0]["data"] = json!(b64(&png(32, 64)));
    forged["images"].as_array_mut().unwrap().push(json!({
        "hash": "0".repeat(64), "format": "svg", "width": 1, "height": 1, "bytes": 5, "data": b64(b"<svg>")
    }));
    let read = op(
        &core,
        json!({"op": "project_file_read", "text": forged.to_string(), "landing": "copy", "name": "偽"}),
    );
    assert!(read["error"].is_null(), "{read}");
    let held: i64 = conn
        .query_row("SELECT count(*) FROM images", [], |r| r.get(0))
        .unwrap();
    assert_eq!(held, 2, "偽の絵は入らない");
}

/// The head of a PNG `w` × `h` of `depth` bits a sample, interlaced or not.
fn png_as(w: u32, h: u32, depth: u8, interlaced: bool) -> Vec<u8> {
    let mut out = png(w, h);
    out[24] = depth;
    out[28] = u8::from(interlaced);
    out
}

/// The head of a GIF `w` × `h`.
fn gif(w: u16, h: u16) -> Vec<u8> {
    let mut out = b"GIF89a".to_vec();
    out.extend_from_slice(&w.to_le_bytes());
    out.extend_from_slice(&h.to_le_bytes());
    out.extend_from_slice(&[0, 0, 0]);
    out
}

#[test]
fn a_picture_that_decodes_whole_is_kept_to_smaller_sides() {
    let core = core();
    let made = |name: &str, bytes: Vec<u8>| {
        common::mcp(
            &core,
            "card_create",
            json!({"project": common::PROJECT, "target": format!("Free/Explain:{name}"),
                "value": {"kind": "image", "imageData": b64(&bytes)}}),
        )
    };
    // Read whole to be shown at all, whatever size it is shown at: an interlaced or a 16-bit PNG and a GIF.
    for (name, bytes) in [
        ("Interlaced", png_as(8000, 8000, 8, true)),
        ("Deep", png_as(8000, 8000, 16, false)),
        ("Moving", gif(8000, 8000)),
    ] {
        let said = made(name, bytes).expect_err(name);
        assert!(said.contains("4096"), "{name}: {said}");
    }
    for (name, bytes) in [
        ("Plain", png_as(16000, 16000, 8, false)),
        ("SmallInterlaced", png_as(4096, 4096, 8, true)),
        ("SmallMoving", gif(4096, 2000)),
    ] {
        let said = made(name, bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(said.get("created").is_some(), "{name}: {said}");
    }
}

#[test]
fn a_card_turned_into_an_image_cannot_say_a_picture_by_hand() {
    let core = core();
    ok(
        &core,
        "card_create",
        json!({"target": "Free/Explain:Box", "value": {"kind": "sketch"}}),
    );
    let said = common::refused(
        &core,
        "card_update",
        json!({"target": "Free/Explain:Box", "value": {"kind": "image", "image": "f".repeat(64),
            "format": "png", "width": 16000, "height": 16000, "bytes": 1}}),
    );
    // Only the contents tell them: what the core writes from a picture, refused as written by hand.
    assert!(said.contains("imageData"), "{said}");
}

#[test]
fn a_project_files_picture_is_kept_in_plain_base64() {
    let core = core();
    ok(
        &core,
        "card_create",
        json!({"target": "Free/Explain:Screen", "value": {"kind": "image", "imageData": b64(&png(64, 32))}}),
    );
    let text = op(
        &core,
        json!({"op": "project_file", "project": project_id(&core)}),
    )["result"]["text"]
        .as_str()
        .unwrap()
        .to_string();
    let mut file: Value = serde_json::from_str(&text).unwrap();
    let plain = file["images"][0]["data"].as_str().unwrap().to_string();
    // Wrapped as some writers wrap base64: read as the same picture, kept as the plain one.
    let wrapped: String = plain
        .as_bytes()
        .chunks(8)
        .map(|c| std::str::from_utf8(c).unwrap())
        .collect::<Vec<_>>()
        .join("\n");
    file["images"][0]["data"] = json!(wrapped);
    let read = op(
        &core,
        json!({"op": "project_file_read", "text": file.to_string(), "landing": "copy", "name": "折り返し"}),
    );
    assert!(read["error"].is_null(), "{read}");
    let conn = rusqlite::Connection::open(core.place()).unwrap();
    let datas: Vec<String> = conn
        .prepare("SELECT data FROM images")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert!(datas.iter().all(|d| d == &plain), "{datas:?}");
}
