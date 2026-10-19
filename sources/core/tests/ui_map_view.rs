//! A ui section drawn on its map (`specboard://SpecBoard/Docs/Design:CardKinds`「UI のセクションとはめ込み」,
//! `specboard://SpecBoard/Docs/Design:CoreApi`「モックを組み上げる」): each part's face is a page of
//! that part alone, drawn as the mock draws it, which `section_read` gives with `as: mock` and
//! `faces: true`; a part's `headerPlace` says the corner of its box the map shows its header tag at,
//! and the mock never shows it.

use crate::common;

use common::{ok, op, refused, script_store};
use serde_json::{Value, json};
use specboard_core::Core;

const PICTURE: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10"><rect width="10" height="10"/></svg>"#;

/// The sample with a ui section `UI`, and `UI/Home`, `UI/Header` and `UI/Badge` in it.
fn core() -> Core {
    let core = Core::open(script_store()).unwrap();
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
    ok(
        &core,
        "section_create",
        json!({"target": ["UI/Home", "UI/Header", "UI/Badge"]}),
    );
    core
}

/// A card's path, where it sits and how big it is, in grid units.
type Placed<'a> = (&'a str, (i64, i64), (i64, i64));

fn put(core: &Core, section: &str, cards: &[Placed]) {
    let places: serde_json::Map<String, Value> = cards
        .iter()
        .map(|(card, (x, y), (w, h))| {
            (
                card.to_string(),
                json!({"topLeft": {"x": x, "y": y}, "size": {"w": w, "h": h}}),
            )
        })
        .collect();
    ok(
        core,
        "section_update",
        json!({"target": section, "value": {"places": places}}),
    );
}

/// A home screen of a title (with a note, and a line to an entity changed after it), a logo, a
/// frame of the header — which frames a badge — a note card and a part no one placed.
fn home(core: &Core) {
    ok(
        core,
        "card_create",
        json!({"value": {
            "UI/Badge:Dot": {"kind": "svg", "body": PICTURE},
            "UI/Header:Bar": {"kind": "html", "body": "<nav>ナビ</nav>"},
            "UI/Header:Inner": {"kind": "frame", "target": "UI/Badge"},
            "UI/Home:Title": {"kind": "html", "body": "<h1>ホーム</h1>", "note": "見出しの付箋", "notePlace": "topLeft",
                "connections": [{"line": "link", "to": "Schemas/Ordering:Order"}]},
            "UI/Home:Logo": {"kind": "svg", "body": PICTURE},
            "UI/Home:Top": {"kind": "frame", "target": "UI/Header"},
            "UI/Home:Memo": {"kind": "note", "body": "描かない"}
        }}),
    );
    ok(
        core,
        "card_create",
        json!({"target": "UI/Home:Loose", "value": {"kind": "html", "body": "<p>まだ</p>"}}),
    );
    put(core, "UI/Badge", &[("UI/Badge:Dot", (0, 0), (4, 2))]);
    put(
        core,
        "UI/Header",
        &[
            ("UI/Header:Bar", (0, 0), (20, 2)),
            ("UI/Header:Inner", (20, 0), (4, 2)),
        ],
    );
    put(
        core,
        "UI/Home",
        &[
            ("UI/Home:Title", (2, 1), (10, 3)),
            ("UI/Home:Logo", (14, 1), (4, 3)),
            ("UI/Home:Top", (2, 6), (24, 2)),
            ("UI/Home:Memo", (40, 40), (10, 4)),
        ],
    );
    // The entity the title reaches changes after it: the title is to be looked at again.
    ok(
        core,
        "card_update",
        json!({"target": "Schemas/Ordering:Order", "value": {"description": "合計を足した"}}),
    );
}

fn read(core: &Core, section: &str, value: Value) -> Value {
    ok(
        core,
        "section_read",
        json!({"target": section, "value": value}),
    )
}

fn face<'f>(faces: &'f Value, path: &str) -> &'f Value {
    faces
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["path"] == path)
        .unwrap_or_else(|| panic!("{path}: {faces}"))
}

#[test]
fn each_part_has_a_page_of_itself_alone_at_its_box_size() {
    let core = core();
    home(&core);
    let read = read(&core, "UI/Home", json!({"as": "mock", "faces": true}));
    let faces = &read["faces"];
    // The parts directly in the section, in the mock's order — the one no one placed last —
    // and no note card.
    let listed: Vec<(&str, &str)> = faces
        .as_array()
        .unwrap()
        .iter()
        .map(|f| (f["path"].as_str().unwrap(), f["kind"].as_str().unwrap()))
        .collect();
    assert_eq!(
        listed,
        [
            ("UI/Home:Title", "html"),
            ("UI/Home:Logo", "svg"),
            ("UI/Home:Top", "frame"),
            ("UI/Home:Loose", "html"),
        ]
    );
    for f in faces.as_array().unwrap() {
        let html = f["html"].as_str().unwrap();
        assert!(
            html.starts_with("<!doctype html>\n<html>\n<head>\n<meta charset=\"utf-8\">\n"),
            "{html}"
        );
        assert!(
            html.contains("<meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; style-src 'unsafe-inline'; img-src data:; font-src data:; base-uri 'none'; form-action 'none'\">"),
            "{html}"
        );
        assert!(
            html.contains(&format!("<title>{}</title>", f["path"].as_str().unwrap())),
            "{html}"
        );
        // On nothing: what the part does not draw shows the map under it.
        assert!(html.contains("html,body{background:transparent}"), "{html}");
        assert!(!html.to_lowercase().contains("<script"), "{html}");
        // Nothing laid over it: the map lays the notes and the marks itself.
        for over in ["sb-note", "sb-review", "data-review", "見出しの付箋"] {
            assert!(!html.contains(over), "{over}: {html}");
        }
        // From the top left, as big as its box.
        assert!(
            html.contains(&format!(
                "data-path=\"{}\" data-kind=\"{}\"",
                f["path"].as_str().unwrap(),
                f["kind"].as_str().unwrap()
            )),
            "{html}"
        );
        assert!(html.contains("style=\"left:0px;top:0px;width:"), "{html}");
    }
    // The page of the title is the mock's part, at the top left of a page as big as its box.
    let title = face(faces, "UI/Home:Title");
    assert_eq!(
        (title["width"].clone(), title["height"].clone()),
        (json!(352), json!(81))
    );
    assert!(
        title["html"].as_str().unwrap().contains(
            "<body>\n<div class=\"sb-mock\" data-section=\"UI/Home\" style=\"width:352px;height:81px\">\n\
             <div class=\"sb-part\" data-path=\"UI/Home:Title\" data-kind=\"html\" style=\"left:0px;top:0px;width:352px;height:81px\">\
             <template shadowrootmode=\"open\"><h1>ホーム</h1></template></div>\n</div>\n</body>"
        ),
        "{title}"
    );
    // A box a fraction of a pixel across is said in whole pixels, drawn at its own size.
    let logo = face(faces, "UI/Home:Logo");
    assert_eq!(
        (logo["width"].clone(), logo["height"].clone()),
        (json!(141), json!(81))
    );
    assert!(
        logo["html"].as_str().unwrap().contains(
            "style=\"left:0px;top:0px;width:140.8px;height:81px\"><img alt=\"\" src=\"data:image/svg+xml;base64,PHN2Zy"
        ),
        "{logo}"
    );
    // A frame holds its target's mock at 1:1, the frames in it too.
    let top = face(faces, "UI/Home:Top");
    assert_eq!(
        (top["width"].clone(), top["height"].clone()),
        (json!(845), json!(54))
    );
    let html = top["html"].as_str().unwrap();
    for inside in [
        "<div class=\"sb-part sb-frame\" data-path=\"UI/Home:Top\" data-kind=\"frame\" data-frame=\"UI/Header\" style=\"left:0px;top:0px;width:844.8px;height:54px\">",
        "data-path=\"UI/Header:Bar\" data-kind=\"html\"",
        "<nav>ナビ</nav>",
        "data-path=\"UI/Header:Inner\" data-kind=\"frame\" data-frame=\"UI/Badge\"",
        "data-path=\"UI/Badge:Dot\" data-kind=\"svg\"",
    ] {
        assert!(html.contains(inside), "{inside}: {html}");
    }
    // One no one placed has a page as big as its box would be.
    let loose = face(faces, "UI/Home:Loose");
    assert_eq!(loose["width"], 352, "{loose}");
    assert!(
        loose["html"].as_str().unwrap().contains("<p>まだ</p>"),
        "{loose}"
    );
    // The mock beside them still has the note and the mark over the title.
    let mock = read["html"].as_str().unwrap();
    assert!(
        mock.contains("見出しの付箋") && mock.contains("data-review"),
        "{mock}"
    );
}

#[test]
fn a_part_gives_the_same_page_each_time_and_the_mock_is_as_without_faces() {
    let core = core();
    home(&core);
    let once = read(&core, "UI/Home", json!({"as": "mock", "faces": true}));
    let again = read(&core, "UI/Home", json!({"as": "mock", "faces": true}));
    assert_eq!(once["faces"], again["faces"]);
    let without = read(&core, "UI/Home", json!({"as": "mock"}));
    assert!(without.get("faces").is_none(), "{without}");
    let off = read(&core, "UI/Home", json!({"as": "mock", "faces": false}));
    assert!(off.get("faces").is_none(), "{off}");
    for key in ["html", "width", "height", "parts", "frames"] {
        assert_eq!(once[key], without[key], "{key}");
    }
    // Moved, a part's page is the same: only its box's size is drawn.
    put(&core, "UI/Home", &[("UI/Home:Title", (30, 10), (10, 3))]);
    let moved = read(&core, "UI/Home", json!({"as": "mock", "faces": true}));
    assert_eq!(
        face(&moved["faces"], "UI/Home:Title"),
        face(&once["faces"], "UI/Home:Title")
    );
    // Another of the section's parts changed, the others' pages are as they were.
    ok(
        &core,
        "card_update",
        json!({"target": "UI/Home:Logo", "value": {"body": PICTURE.replace("10\"/>", "5\"/>")}}),
    );
    let changed = read(&core, "UI/Home", json!({"as": "mock", "faces": true}));
    assert_ne!(
        face(&changed["faces"], "UI/Home:Logo"),
        face(&once["faces"], "UI/Home:Logo")
    );
    assert_eq!(
        face(&changed["faces"], "UI/Home:Top"),
        face(&once["faces"], "UI/Home:Top")
    );
}

#[test]
fn faces_are_asked_only_with_as_mock() {
    let core = core();
    home(&core);
    for value in [
        json!({"faces": true}),
        json!({"as": "diagram", "faces": true}),
        json!({"as": "picture", "faces": true}),
    ] {
        let why = refused(
            &core,
            "section_read",
            json!({"target": "UI/Home", "value": value.clone()}),
        );
        assert!(why.contains("faces"), "{value}: {why}");
    }
    // A section that is not a ui one has no mock, faces or not.
    let why = refused(
        &core,
        "section_read",
        json!({"target": "Docs", "value": {"as": "mock", "faces": true}}),
    );
    assert!(why.contains("UI のセクションではありません"), "{why}");
    // An empty ui section has none.
    ok(&core, "section_create", json!({"target": "UI/Home/Empty"}));
    let empty = read(&core, "UI/Home/Empty", json!({"as": "mock", "faces": true}));
    assert_eq!(empty["faces"], json!([]), "{empty}");
}

#[test]
fn faces_of_frames_inside_frames_stop_as_deep_as_the_mock() {
    let core = core();
    let names = ["D1", "D2", "D3", "D4", "D5", "D6"];
    let mut cards = serde_json::Map::new();
    for (i, name) in names.iter().enumerate() {
        ok(
            &core,
            "section_create",
            json!({"target": format!("UI/{name}")}),
        );
        cards.insert(
            format!("UI/{name}:Part"),
            json!({"kind": "html", "body": format!("<p>{name}</p>")}),
        );
        if let Some(next) = names.get(i + 1) {
            cards.insert(
                format!("UI/{name}:Next"),
                json!({"kind": "frame", "target": format!("UI/{next}")}),
            );
        }
    }
    ok(&core, "card_create", json!({"value": cards}));
    let read = read(&core, "UI/D1", json!({"as": "mock", "faces": true}));
    let next = face(&read["faces"], "UI/D1:Next")["html"]
        .as_str()
        .unwrap()
        .to_string();
    // As in the mock: D2 to D5 drawn, D6 named.
    for name in &names[1..5] {
        assert!(next.contains(&format!("<p>{name}</p>")), "{name}: {next}");
    }
    assert!(!next.contains("<p>D6</p>"), "{next}");
    assert!(next.contains("<div class=\"sb-cut\">UI/D6</div>"), "{next}");
    let mock = read["html"].as_str().unwrap();
    assert!(mock.contains("<div class=\"sb-cut\">UI/D6</div>"), "{mock}");
}

#[test]
fn a_parts_header_place_is_written_and_read_and_never_in_the_mock() {
    let core = core();
    home(&core);
    // Without the notes and marks: writing the part clears its mark.
    let bare = json!({"as": "mock", "notes": false, "faces": true});
    let before = read(&core, "UI/Home", bare.clone());
    ok(
        &core,
        "card_update",
        json!({"value": {
            "UI/Home:Title": {"headerPlace": "bottomRight"},
            "UI/Home:Top": {"headerPlace": "hidden"}
        }}),
    );
    let card = ok(&core, "card_read", json!({"target": "UI/Home:Title"}));
    assert_eq!(card["headerPlace"], "bottomRight", "{card}");
    // The window reads it in the card's fields; one not written has none (top left).
    let detail = op(
        &core,
        json!({"op": "project_detail", "project": common::project_id(&core)}),
    );
    let part = |name: &str| {
        detail["result"]["schemas"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["name"] == name && c["kind"] != "note")
            .cloned()
            .unwrap()
    };
    assert_eq!(
        part("Title")["form"]["fields"]["headerPlace"],
        "bottomRight"
    );
    assert_eq!(part("Top")["form"]["fields"]["headerPlace"], "hidden");
    assert!(part("Logo")["form"]["fields"].get("headerPlace").is_none());
    // The tag is the map's: the mock and the faces are as they were.
    let after = read(&core, "UI/Home", bare);
    assert_eq!(after["html"], before["html"]);
    assert_eq!(after["faces"], before["faces"]);
    // A corner or hidden, and only on a part.
    let why = refused(
        &core,
        "card_update",
        json!({"target": "UI/Home:Logo", "value": {"headerPlace": "middle"}}),
    );
    assert!(why.contains("headerPlace"), "{why}");
    let why = refused(
        &core,
        "card_update",
        json!({"target": "UI/Home:Memo", "value": {"headerPlace": "topLeft"}}),
    );
    assert!(why.contains("headerPlace"), "{why}");
}
