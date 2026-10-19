//! UI mocks (`specboard://SpecBoard/Docs/Design:CardKinds`「UI のセクションとはめ込み」,
//! `specboard://SpecBoard/Docs/Design:CoreApi`「モックを組み上げる」): a ui section holds html and svg parts,
//! notes and frames; a frame puts another ui section's mock into its box, held by the section's id;
//! the parts of a ui section make one page, which `section_read` gives as `as: mock`, the app draws as
//! `as: picture` and an export writes as a file.

use crate::common;

use common::{ok, refused, script_store};
use serde_json::{Value, json};
use specboard_core::Core;

const PICTURE: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10"><rect width="10" height="10"/></svg>"#;

/// The sample with a ui section `UI` under main's root, and `UI/Home` and `UI/Header` in it.
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
        json!({"target": ["UI/Home", "UI/Header"]}),
    );
    core
}

#[test]
fn a_ui_section_holds_parts_notes_and_frames_and_nothing_else() {
    let core = core();
    ok(
        &core,
        "card_create",
        json!({"value": {
            "UI/Home:Title": {"kind": "html", "body": "<h1>ホーム</h1>"},
            "UI/Home:Logo": {"kind": "svg", "body": PICTURE},
            "UI/Home:Memo": {"kind": "note", "body": "あとで決める"},
            "UI/Home:Top": {"kind": "frame", "target": "UI/Header"}
        }}),
    );
    let read = ok(&core, "section_read", json!({"target": "UI/Home"}));
    assert_eq!(read["type"], "ui", "{read}");
    assert_eq!(read["cards"].as_array().unwrap().len(), 4, "{read}");
    for kind in ["entity", "freedoc", "flow"] {
        let why = refused(
            &core,
            "card_create",
            json!({"target": "UI/Home:Other", "value": {"kind": kind}}),
        );
        assert!(why.contains(kind), "{kind}: {why}");
    }
    // A frame goes only where a section names it: not in one that holds anything.
    let why = refused(
        &core,
        "card_create",
        json!({"target": "Free:Top", "value": {"kind": "frame", "target": "UI/Header"}}),
    );
    assert!(why.contains("frame"), "{why}");
    // A ui section is made inside a ui section, as deep as any.
    ok(&core, "section_create", json!({"target": "UI/Home/Dialog"}));
    let kind = ok(&core, "kind_read", json!({"target": "ui"}));
    // It opens on its map, as every section does: its parts are drawn there as the mock draws them.
    assert_eq!(kind["opens"], "relations", "{kind}");
    assert_eq!(kind["icon"]["name"], "macwindow", "{kind}");
}

#[test]
fn a_frame_points_at_a_ui_section_by_id_and_follows_it() {
    let core = core();
    ok(
        &core,
        "card_create",
        json!({"value": {
            "UI/Home:Top": {"kind": "frame", "target": "UI/Header"},
            "UI/Home:Again": {"kind": "frame", "target": "UI/Header"}
        }}),
    );
    let read = ok(&core, "card_read", json!({"target": "UI/Home:Top"}));
    assert_eq!(read["kind"], "frame", "{read}");
    assert_eq!(read["target"], "UI/Header", "{read}");
    // Renamed and moved, the section is still the one it points at.
    ok(
        &core,
        "section_update",
        json!({"target": "UI/Header", "value": {"name": "Bar"}}),
    );
    assert_eq!(
        ok(&core, "card_read", json!({"target": "UI/Home:Top"}))["target"],
        "UI/Bar"
    );
    ok(&core, "section_create", json!({"target": "UI/Parts"}));
    ok(
        &core,
        "section_update",
        json!({"target": "UI/Bar", "value": {"parent": "UI/Parts"}}),
    );
    assert_eq!(
        ok(&core, "card_read", json!({"target": "UI/Home:Again"}))["target"],
        "UI/Parts/Bar"
    );
    // Pointed elsewhere by card_update.
    ok(
        &core,
        "card_update",
        json!({"target": "UI/Home:Again", "value": {"target": "UI/Parts"}}),
    );
    assert_eq!(
        ok(&core, "card_read", json!({"target": "UI/Home:Again"}))["target"],
        "UI/Parts"
    );
}

#[test]
fn a_frame_is_refused_at_what_is_not_a_ui_section_at_its_own_and_round() {
    let core = core();
    for (target, said) in [
        ("Docs", "UI のセクションではありません"),
        ("UI/Nowhere", "というセクションはありません"),
        ("UI/Home", "自分のいるセクションです"),
    ] {
        let why = refused(
            &core,
            "card_create",
            json!({"target": "UI/Home:Top", "value": {"kind": "frame", "target": target}}),
        );
        assert!(why.contains(said), "{target}: {why}");
    }
    ok(&core, "section_create", json!({"target": "UI/Side"}));
    ok(
        &core,
        "card_create",
        json!({"value": {
            "UI/Home:Top": {"kind": "frame", "target": "UI/Header"},
            "UI/Header:Side": {"kind": "frame", "target": "UI/Side"}
        }}),
    );
    let why = refused(
        &core,
        "card_create",
        json!({"target": "UI/Side:Back", "value": {"kind": "frame", "target": "UI/Home"}}),
    );
    assert!(
        why.contains("UI/Side → UI/Home → UI/Header → UI/Side"),
        "{why}"
    );
    // Changed or moved round, the same.
    let why = refused(
        &core,
        "card_update",
        json!({"target": "UI/Home:Top", "value": {"target": "UI/Home"}}),
    );
    assert!(why.contains("自分のいるセクション"), "{why}");
    ok(
        &core,
        "card_create",
        json!({"target": "UI:Loose", "value": {"kind": "frame", "target": "UI/Home"}}),
    );
    let why = refused(
        &core,
        "card_update",
        json!({"target": "UI:Loose", "value": {"section": "UI/Side"}}),
    );
    assert!(why.contains("輪になる"), "{why}");
}

#[test]
fn a_frame_whose_section_is_gone_is_told_by_the_check() {
    let core = core();
    ok(
        &core,
        "card_create",
        json!({"target": "UI/Home:Top", "value": {"kind": "frame", "target": "UI/Header"}}),
    );
    let found = common::checked(&core, "main").to_string();
    assert!(!found.contains("UI/Home:Top"), "{found}");
    ok(&core, "section_delete", json!({"target": "UI/Header"}));
    let found = common::checked(&core, "main");
    let broken: Vec<&Value> = found["brokenPaths"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|i| i.to_string().contains("UI/Home:Top"))
        .collect();
    assert_eq!(broken.len(), 1, "{found}");
    assert!(
        broken[0]
            .to_string()
            .contains("指す先のセクションがありません"),
        "{found}"
    );
}

/// A card's path, where it sits and how big it is, in grid units.
type Box<'a> = (&'a str, (i64, i64), (i64, i64));

/// Each card put where `at` says and as big as `size` says (grid units), all at once, in one
/// section.
fn put(core: &Core, section: &str, cards: &[Box]) {
    let places: serde_json::Map<String, Value> = cards
        .iter()
        .map(|(card, (x, y), (w, h))| {
            (
                card.to_string(),
                json!({"topLeft": {"x": x, "y": y}, "size": {"w": w, "h": h}}),
            )
        })
        .collect();
    let said = ok(
        core,
        "section_update",
        json!({"target": section, "value": {"places": places}}),
    );
    assert!(said.get("jumped").is_none(), "{said}");
}

/// A home screen of a title, a logo, a note and a frame of the header, the header a bar.
fn home(core: &Core) {
    ok(
        core,
        "card_create",
        json!({"value": {
            "UI/Header:Bar": {"kind": "html", "body": "<!doctype html><html><head><title>t</title><style>nav{color:red}</style></head><body class=\"b\"><nav>ナビ</nav></body></html>"},
            "UI/Home:Title": {"kind": "html", "body": "<h1>ホーム</h1><a href=\"sb-nav:UI/Header\">上</a>"},
            "UI/Home:Logo": {"kind": "svg", "body": PICTURE},
            "UI/Home:Memo": {"kind": "note", "body": "描かない"},
            "UI/Home:Top": {"kind": "frame", "target": "UI/Header"}
        }}),
    );
    put(core, "UI/Header", &[("UI/Header:Bar", (0, 0), (20, 2))]);
    put(
        core,
        "UI/Home",
        &[
            ("UI/Home:Title", (2, 1), (10, 3)),
            ("UI/Home:Logo", (14, 1), (4, 3)),
            ("UI/Home:Top", (2, 6), (20, 2)),
            ("UI/Home:Memo", (40, 40), (10, 4)),
        ],
    );
}

#[test]
fn a_ui_section_is_read_as_one_page_of_its_parts_where_they_sit() {
    let core = core();
    home(&core);
    let mock = ok(
        &core,
        "section_read",
        json!({"target": "UI/Home", "value": {"as": "mock"}}),
    );
    // A grid unit is a tenth of the card's 352 points across and a card row (27) down; the page is
    // the box round the parts, the note left out.
    assert_eq!(mock["path"], "UI/Home", "{mock}");
    assert_eq!(mock["width"], 704, "{mock}");
    assert_eq!(mock["height"], 189, "{mock}");
    assert_eq!(mock["parts"], 3, "{mock}");
    assert_eq!(mock["frames"], 1, "{mock}");
    let html = mock["html"].as_str().unwrap();
    assert!(
        html.starts_with("<!doctype html>\n<html>\n<head>\n<meta charset=\"utf-8\">\n"),
        "{html}"
    );
    assert!(
        html.contains("<meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; style-src 'unsafe-inline'; img-src data:; font-src data:; base-uri 'none'; form-action 'none'\">"),
        "{html}"
    );
    assert!(html.contains("<title>UI/Home</title>"), "{html}");
    assert!(!html.to_lowercase().contains("<script"), "{html}");
    assert!(!html.contains("描かない"), "{html}");
    assert!(
        html.contains(
            "<div class=\"sb-mock\" data-section=\"UI/Home\" style=\"width:704px;height:189px\">"
        ),
        "{html}"
    );
    // An html part's body in a shadow root, its links as written.
    assert!(
        html.contains("<div class=\"sb-part\" data-path=\"UI/Home:Title\" data-kind=\"html\" style=\"left:0px;top:0px;width:352px;height:81px\"><template shadowrootmode=\"open\"><h1>ホーム</h1><a href=\"sb-nav:UI/Header\">上</a></template></div>"),
        "{html}"
    );
    // An svg part an image of its body.
    assert!(
        html.contains("<div class=\"sb-part\" data-path=\"UI/Home:Logo\" data-kind=\"svg\" style=\"left:422.4px;top:0px;width:140.8px;height:81px\"><img alt=\"\" src=\"data:image/svg+xml;base64,PHN2Zy"),
        "{html}"
    );
    // A frame holds the header's mock at 1:1; a whole page gives its head's styles and its body.
    assert!(
        html.contains("<div class=\"sb-part sb-frame\" data-path=\"UI/Home:Top\" data-kind=\"frame\" data-frame=\"UI/Header\" style=\"left:0px;top:135px;width:704px;height:54px\"><div class=\"sb-mock\" data-section=\"UI/Header\" style=\"width:704px;height:54px\">\n<div class=\"sb-part\" data-path=\"UI/Header:Bar\" data-kind=\"html\" style=\"left:0px;top:0px;width:704px;height:54px\"><template shadowrootmode=\"open\"><style>nav{color:red}</style><nav>ナビ</nav></template></div>"),
        "{html}"
    );
}

#[test]
fn frames_are_drawn_four_deep_and_a_section_used_twice_is_drawn_twice() {
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
    let mock = ok(
        &core,
        "section_read",
        json!({"target": "UI/D1", "value": {"as": "mock"}}),
    );
    let html = mock["html"].as_str().unwrap();
    for name in &names[..5] {
        assert!(html.contains(&format!("<p>{name}</p>")), "{name}: {html}");
    }
    assert!(!html.contains("<p>D6</p>"), "{html}");
    assert!(html.contains("<div class=\"sb-cut\">UI/D6</div>"), "{html}");
    assert_eq!(mock["parts"], 5, "{mock}");
    assert_eq!(mock["frames"], 5, "{mock}");
    // One section in two frames is drawn in each.
    ok(
        &core,
        "card_create",
        json!({"target": "UI/Home:Again", "value": {"kind": "frame", "target": "UI/D5"}}),
    );
    ok(
        &core,
        "card_create",
        json!({"target": "UI/Home:Once", "value": {"kind": "frame", "target": "UI/D6"}}),
    );
    let mock = ok(
        &core,
        "section_read",
        json!({"target": "UI/Home", "value": {"as": "mock"}}),
    );
    assert_eq!(
        mock["html"].as_str().unwrap().matches("<p>D6</p>").count(),
        2,
        "{mock}"
    );
}

#[test]
fn a_section_that_is_not_a_ui_one_has_no_mock() {
    let core = core();
    let why = refused(
        &core,
        "section_read",
        json!({"target": "Docs", "value": {"as": "mock"}}),
    );
    assert!(
        why.contains("UI のセクションではありません")
            && why.contains("as: picture")
            && why.contains("as: diagram"),
        "{why}"
    );
    // An empty ui section is an empty page.
    let mock = ok(
        &core,
        "section_read",
        json!({"target": "UI/Header", "value": {"as": "mock"}}),
    );
    assert_eq!(
        (mock["width"].clone(), mock["parts"].clone()),
        (json!(0), json!(0))
    );
    // Its map is read as any section's.
    home(&core);
    let diagram = ok(
        &core,
        "section_read",
        json!({"target": "UI/Home", "value": {"as": "diagram"}}),
    );
    let mermaid = diagram["mermaid"].as_str().unwrap();
    assert!(mermaid.contains("<<frame>>"), "{mermaid}");
    let read = ok(&core, "section_read", json!({"target": "UI/Home"}));
    assert_eq!(read["cards"].as_array().unwrap().len(), 4, "{read}");
}

/// An app that draws every picture it is asked for, and keeps what it was asked.
struct Painter {
    asked: std::sync::Mutex<Vec<Value>>,
}

impl specboard_core::App for Painter {
    fn app_control(&self, _arguments: &str, _caller: &str) -> String {
        panic!("a picture does not go through app_control")
    }

    fn picture(&self, request: &str) -> String {
        let request: Value = serde_json::from_str(request).unwrap();
        let picture = request["picture"].clone();
        self.asked.lock().unwrap().push(request);
        json!({"image": {"data": "iVBORw0KGgo=", "mimeType": "image/png"},
            "note": {"picture": picture, "width": 704, "height": 189, "scale": 1}})
        .to_string()
    }
}

/// A read through MCP: its result (`content`, `isError`).
fn result(core: &Core, tool: &str, mut arguments: Value) -> Value {
    arguments["project"] = "見本".into();
    let message = json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call",
        "params": {"name": tool, "arguments": arguments}});
    let reply = common::op(
        core,
        json!({"op": "mcp", "message": message, "headers": common::headers(core)}),
    );
    reply["result"]["body"]["result"].clone()
}

#[test]
fn a_ui_sections_picture_is_its_mock_and_its_map_when_asked() {
    let core = core();
    home(&core);
    let painter = std::sync::Arc::new(Painter {
        asked: std::sync::Mutex::new(Vec::new()),
    });
    core.set_app(painter.clone());
    let drawn = result(
        &core,
        "section_read",
        json!({"target": "UI/Home", "value": {"as": "picture"}}),
    );
    assert_eq!(drawn["isError"], false, "{drawn}");
    let note: Value = serde_json::from_str(drawn["content"][1]["text"].as_str().unwrap()).unwrap();
    assert_eq!(note["picture"], "mock", "{note}");
    assert_eq!(note["path"], "UI/Home", "{note}");
    let asked = painter.asked.lock().unwrap().last().cloned().unwrap();
    let mock = ok(
        &core,
        "section_read",
        json!({"target": "UI/Home", "value": {"as": "mock"}}),
    );
    assert_eq!(asked["picture"], "mock", "{asked}");
    assert_eq!(asked["html"], mock["html"], "{asked}");
    assert_eq!(
        (asked["width"].clone(), asked["height"].clone()),
        (json!(704), json!(189))
    );
    assert!(
        asked["section"].is_string() && asked["maxSize"] == 1600,
        "{asked}"
    );
    // The map, as any section's.
    result(
        &core,
        "section_read",
        json!({"target": "UI/Home", "value": {"as": "picture", "view": "map"}}),
    );
    let asked = painter.asked.lock().unwrap().last().cloned().unwrap();
    assert_eq!(asked["picture"], "sectionMap", "{asked}");
    assert!(asked.get("html").is_none(), "{asked}");
    // A mock takes none of a map's range, and another section has no mock.
    let why = refused(
        &core,
        "section_read",
        json!({"target": "UI/Home", "value": {"as": "picture", "area": {"topLeft": {"x": 0, "y": 0}, "bottomRight": {"x": 3, "y": 3}}}}),
    );
    assert!(why.contains("area"), "{why}");
    let why = refused(
        &core,
        "section_read",
        json!({"target": "Docs", "value": {"as": "picture", "view": "mock"}}),
    );
    assert!(why.contains("UI のセクションではありません"), "{why}");
}

#[test]
fn with_no_app_a_mocks_picture_offers_the_mock_as_text() {
    let core = core();
    home(&core);
    let drawn = result(
        &core,
        "section_read",
        json!({"target": "UI/Home", "value": {"as": "picture", "maxSize": 800}}),
    );
    assert_eq!(drawn["isError"], true, "{drawn}");
    let said: Value = serde_json::from_str(drawn["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(
        said["error"]["use"],
        json!({"tool": "section_read", "arguments": {"target": "UI/Home", "value": {"as": "mock"}, "project": "見本"}}),
        "{said}"
    );
    let drawn = result(
        &core,
        "section_read",
        json!({"target": "UI/Home", "value": {"as": "picture", "view": "map"}}),
    );
    let said: Value = serde_json::from_str(drawn["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(
        said["error"]["use"]["arguments"]["value"],
        json!({"as": "diagram"}),
        "{said}"
    );
}

#[test]
fn an_export_writes_each_ui_section_as_one_page_linked_to_the_others() {
    let core = core();
    home(&core);
    let project = common::project_id(&core);
    let set = common::op(
        &core,
        json!({"op": "mcp_writes_set", "project": project, "group": "export", "state": "allow"}),
    );
    assert!(set.get("error").is_none(), "{set}");
    ok(
        &core,
        "card_create",
        json!({"value": {
            "Docs:Guide": {"kind": "freedoc", "body": "# 手引き"},
            "UI/Header:Help": {"kind": "html", "body": "<a href=\"sb-nav:Docs:Guide\">手引き</a><a href=\"sb-nav:UI/Home\">家</a><a href=\"https://example.com\">外</a>"}
        }}),
    );
    ok(
        &core,
        "export_create",
        json!({"target": "mocks", "value": {"entries": [
            {"from": "UI", "to": "pages/"},
            {"from": "Docs:Guide", "to": "docs/Guide.md"},
            {"from": "UI/Home", "to": "home.html"}
        ]}}),
    );
    let folder = common::place("mocks");
    std::fs::create_dir_all(&folder).unwrap();
    let wrote = ok(
        &core,
        "export_execute",
        json!({"target": "mocks", "targetdir": folder.to_string_lossy()}),
    );
    let mut written: Vec<String> = wrote["exports"][0]["written"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p.as_str().unwrap().to_string())
        .collect();
    written.sort();
    // One page a ui section, the parts in none of their own; an entry from the section itself to a
    // file is its page there.
    assert_eq!(
        written,
        [
            "docs/Guide.md",
            "home.html",
            "pages/UI.html",
            "pages/UI/Header.html",
        ],
        "{wrote}"
    );
    let header = std::fs::read_to_string(folder.join("pages/UI/Header.html")).unwrap();
    assert!(header.starts_with("<!doctype html>\n<!-- "), "{header}");
    assert!(
        header.contains("<a href=\"../../docs/Guide.md\">手引き</a><a href=\"../../home.html\">家</a><a href=\"https://example.com\">外</a>"),
        "{header}"
    );
    let page = std::fs::read_to_string(folder.join("home.html")).unwrap();
    assert!(
        page.contains("<a href=\"pages/UI/Header.html\">上</a>"),
        "{page}"
    );
    // The page is the mock, the header framed in it.
    assert!(
        page.contains("data-path=\"UI/Home:Top\" data-kind=\"frame\" data-frame=\"UI/Header\""),
        "{page}"
    );
    let read = ok(
        &core,
        "export_read",
        json!({"target": "mocks", "targetdir": folder.to_string_lossy()}),
    );
    for file in read["exports"][0]["files"].as_array().unwrap() {
        assert_eq!(file["status"], "current", "{file}");
        if file["path"] == "home.html" {
            assert_eq!(file["content"], "mock", "{file}");
            assert_eq!(file["cards"].as_array().unwrap().len(), 3, "{file}");
        }
    }
}

#[test]
fn kind_read_and_guide_tell_of_the_ui_section_and_the_frame() {
    let core = core();
    let frame = ok(&core, "kind_read", json!({"target": "frame"}));
    assert_eq!(frame["face"], json!({}), "{frame}");
    let target = frame["fields"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["name"] == "target")
        .cloned()
        .unwrap();
    assert_eq!(target["fieldtype"], "section", "{frame}");
    let said = ok(&core, "guide", json!({"about": "frame"})).to_string();
    assert!(said.contains("target") && said.contains("UI"), "{said}");
    let list = ok(&core, "kind_list", json!({}));
    let ui = list["sections"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["section"] == "ui")
        .cloned();
    assert!(ui.is_some(), "{list}");
    // With a ui section in the project, a frame has a place, and is listed.
    assert!(
        list["cards"]
            .as_array()
            .unwrap()
            .iter()
            .any(|k| k["kind"] == "frame"),
        "{list}"
    );
}

#[test]
fn the_screen_reads_a_frame_with_its_target_by_id_and_the_ui_kind_opening_on_its_map() {
    let core = core();
    ok(
        &core,
        "card_create",
        json!({"target": "UI/Home:Top", "value": {"kind": "frame", "target": "UI/Header"}}),
    );
    let detail = common::op(
        &core,
        json!({"op": "project_detail", "project": common::project_id(&core)}),
    );
    let result = &detail["result"];
    let header = result["sections"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == "Header")
        .cloned()
        .unwrap();
    let frame = result["schemas"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "Top")
        .cloned()
        .unwrap();
    assert_eq!(frame["kind"], "frame", "{frame}");
    assert_eq!(frame["form"]["fields"]["target"], header["id"], "{frame}");
    assert_eq!(frame["faceRows"], 19, "{frame}");
    let ui = result["kinds"]["sections"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["section"] == "ui")
        .cloned()
        .unwrap();
    assert_eq!(ui["opens"], "relations", "{ui}");
    assert_eq!(header["kind"], "ui", "{header}");
}
