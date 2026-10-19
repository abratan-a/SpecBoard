//! A part's note and its review mark (`specboard://SpecBoard/Docs/Design:CardKinds`「UI のセクションとはめ込み」,
//! `specboard://SpecBoard/Docs/Design:CoreApi`「モックを組み上げる」): an html, svg or frame part's
//! `note` is laid over the mock at the corner its `notePlace` names, inside a frame where the part is
//! framed, and never in an export; a part is marked to be looked at again when a card its lines
//! reach changes after the part last did, until the part is written or marked `reviewed`.

use crate::common;

use common::{ok, op, refused, script_store};
use serde_json::{Value, json};
use specboard_core::Core;

/// The sample with a ui section `UI`, and `UI/Home` and `UI/Header` in it.
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

/// A card's path, where it sits and how big it is, in grid units.
type Placed<'a> = (&'a str, (i64, i64), (i64, i64));

/// Each card put where `at` says and as big as `size` says (grid units), in one section.
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

fn mock(core: &Core, section: &str, value: Value) -> String {
    let mut value = value;
    value["as"] = "mock".into();
    ok(
        core,
        "section_read",
        json!({"target": section, "value": value}),
    )["html"]
        .as_str()
        .unwrap()
        .to_string()
}

#[test]
fn a_note_sits_at_the_corner_its_place_names_and_a_hidden_one_is_not_drawn() {
    let core = core();
    ok(
        &core,
        "card_create",
        json!({"value": {
            "UI/Home:A": {"kind": "html", "body": "<p>a</p>", "note": "左上", "notePlace": "topLeft"},
            "UI/Home:B": {"kind": "html", "body": "<p>b</p>", "note": "<b>右上 & 既定</b>"},
            "UI/Home:C": {"kind": "svg", "body": "<svg xmlns=\"http://www.w3.org/2000/svg\"/>", "note": "左下", "notePlace": "bottomLeft"},
            "UI/Home:D": {"kind": "html", "body": "<p>d</p>", "note": "右下", "notePlace": "bottomRight"},
            "UI/Home:E": {"kind": "html", "body": "<p>e</p>", "note": "出さない", "notePlace": "hidden"},
            "UI/Home:F": {"kind": "html", "body": "<p>f</p>"}
        }}),
    );
    put(
        &core,
        "UI/Home",
        &[
            ("UI/Home:A", (0, 0), (10, 2)),
            ("UI/Home:B", (10, 0), (10, 2)),
            ("UI/Home:C", (0, 2), (10, 2)),
            ("UI/Home:D", (10, 2), (10, 2)),
            ("UI/Home:E", (20, 0), (5, 2)),
            ("UI/Home:F", (20, 2), (5, 2)),
        ],
    );
    // The page is 25 units across (880 px) and 4 down (108 px): a note on the right is held from
    // the page's right, one at the bottom from its bottom, so it stays in the part's corner.
    let html = mock(&core, "UI/Home", json!({}));
    for note in [
        "<div class=\"sb-note\" data-path=\"UI/Home:A\" data-place=\"topLeft\" style=\"left:0px;top:0px\">左上</div>",
        "<div class=\"sb-note\" data-path=\"UI/Home:B\" data-place=\"topRight\" style=\"right:176px;top:0px\">&lt;b&gt;右上 &amp; 既定&lt;/b&gt;</div>",
        "<div class=\"sb-note\" data-path=\"UI/Home:C\" data-place=\"bottomLeft\" style=\"left:0px;bottom:0px\">左下</div>",
        "<div class=\"sb-note\" data-path=\"UI/Home:D\" data-place=\"bottomRight\" style=\"right:176px;bottom:0px\">右下</div>",
    ] {
        assert!(html.contains(note), "{note}\n{html}");
    }
    assert!(!html.contains("出さない"), "{html}");
    assert_eq!(html.matches("class=\"sb-note\"").count(), 4, "{html}");
    // Laid over the parts: after every part of the section.
    let last_part = html.rfind("data-path=\"UI/Home:F\" data-kind").unwrap();
    assert!(
        html.find("class=\"sb-note\"").unwrap() > last_part,
        "{html}"
    );
    assert!(html.contains(".sb-note{"), "{html}");
    // `notes: false` leaves them out, and the style that draws them.
    let bare = mock(&core, "UI/Home", json!({"notes": false}));
    assert!(!bare.contains("sb-note"), "{bare}");
    assert!(!bare.contains("左上"), "{bare}");
}

#[test]
fn a_framed_parts_note_is_drawn_inside_the_frame() {
    let core = core();
    ok(
        &core,
        "card_create",
        json!({"value": {
            "UI/Header:Bar": {"kind": "html", "body": "<nav>ナビ</nav>", "note": "ナビの説明"},
            "UI/Home:Top": {"kind": "frame", "target": "UI/Header", "note": "共通のヘッダー", "notePlace": "bottomLeft"}
        }}),
    );
    put(&core, "UI/Header", &[("UI/Header:Bar", (0, 0), (20, 2))]);
    put(&core, "UI/Home", &[("UI/Home:Top", (0, 0), (20, 3))]);
    let html = mock(&core, "UI/Home", json!({}));
    let framed = html.find("data-section=\"UI/Header\"").unwrap();
    let inner = html
        .find("<div class=\"sb-note\" data-path=\"UI/Header:Bar\" data-place=\"topRight\" style=\"right:0px;top:0px\">ナビの説明</div>")
        .unwrap_or_else(|| panic!("{html}"));
    let outer = html
        .find("<div class=\"sb-note\" data-path=\"UI/Home:Top\" data-place=\"bottomLeft\" style=\"left:0px;bottom:0px\">共通のヘッダー</div>")
        .unwrap_or_else(|| panic!("{html}"));
    // The framed part's note in the frame's own section, the frame's own note over the frame.
    assert!(framed < inner && inner < outer, "{html}");
    // The header's own mock has its note too.
    let header = mock(&core, "UI/Header", json!({}));
    assert!(header.contains("ナビの説明"), "{header}");
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
        self.asked.lock().unwrap().push(request);
        json!({"image": {"data": "iVBORw0KGgo=", "mimeType": "image/png"},
            "note": {"picture": "mock", "width": 352, "height": 54, "scale": 1}})
        .to_string()
    }
}

#[test]
fn a_mocks_picture_has_the_notes_unless_left_out() {
    let core = core();
    ok(
        &core,
        "card_create",
        json!({"target": "UI/Home:A", "value": {"kind": "html", "body": "<p>a</p>", "note": "付箋"}}),
    );
    let painter = std::sync::Arc::new(Painter {
        asked: std::sync::Mutex::new(Vec::new()),
    });
    core.set_app(painter.clone());
    let draw = |value: Value| {
        let message = json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call",
            "params": {"name": "section_read", "arguments": {"project": "見本", "target": "UI/Home", "value": value}}});
        let reply = op(
            &core,
            json!({"op": "mcp", "message": message, "headers": common::headers(&core)}),
        );
        assert_eq!(
            reply["result"]["body"]["result"]["isError"], false,
            "{reply}"
        );
        painter.asked.lock().unwrap().last().cloned().unwrap()
    };
    let asked = draw(json!({"as": "picture"}));
    assert!(asked["html"].as_str().unwrap().contains("付箋"), "{asked}");
    let asked = draw(json!({"as": "picture", "notes": false}));
    assert!(!asked["html"].as_str().unwrap().contains("付箋"), "{asked}");
}

#[test]
fn an_export_has_no_notes_and_no_review_marks() {
    let core = core();
    let project = common::project_id(&core);
    let set = op(
        &core,
        json!({"op": "mcp_writes_set", "project": project, "group": "export", "state": "allow"}),
    );
    assert!(set.get("error").is_none(), "{set}");
    ok(
        &core,
        "card_create",
        json!({"target": "UI/Home:A", "value": {"kind": "html", "body": "<p>a</p>", "note": "書き出さない付箋",
            "connections": [{"line": "link", "to": "Schemas/Ordering:Order"}]}}),
    );
    ok(
        &core,
        "card_update",
        json!({"target": "Schemas/Ordering:Order", "value": {"description": "変えた"}}),
    );
    assert!(mock(&core, "UI/Home", json!({})).contains("data-review"));
    ok(
        &core,
        "export_create",
        json!({"target": "mocks", "value": {"entries": [{"from": "UI/Home", "to": "home.html"}]}}),
    );
    let folder = common::place("notes-export");
    std::fs::create_dir_all(&folder).unwrap();
    ok(
        &core,
        "export_execute",
        json!({"target": "mocks", "targetdir": folder.to_string_lossy()}),
    );
    let page = std::fs::read_to_string(folder.join("home.html")).unwrap();
    assert!(page.contains("<p>a</p>"), "{page}");
    for absent in ["書き出さない付箋", "sb-note", "sb-review", "data-review"] {
        assert!(!page.contains(absent), "{absent}: {page}");
    }
}

// MARK: - Review marks

/// The part `UI/Home:Order`, a line from it to the entity `Schemas/Ordering:Order`, written after
/// the entity.
fn linked_part(core: &Core) {
    ok(
        core,
        "card_create",
        json!({"target": "UI/Home:Order", "value": {"kind": "html", "body": "<p>注文</p>",
            "connections": [{"line": "link", "to": "Schemas/Ordering:Order"}]}}),
    );
}

fn change_entity(core: &Core, branch: &str, said: &str) {
    ok(
        core,
        "card_update",
        json!({"branch": branch, "target": "Schemas/Ordering:Order", "value": {"description": said}}),
    );
}

fn review(core: &Core, branch: &str) -> Value {
    ok(
        core,
        "card_read",
        json!({"branch": branch, "target": "UI/Home:Order"}),
    )
    .get("review")
    .cloned()
    .unwrap_or(Value::Null)
}

/// The part as the window reads the tree (`project_detail`; `""` main, else a branch's id).
fn detail_part(core: &Core, tree: &str) -> Value {
    let said = op(
        core,
        json!({"op": "project_detail", "project": common::project_id(core), "branch": tree}),
    );
    said["result"]["schemas"]
        .as_array()
        .unwrap_or_else(|| panic!("{said}"))
        .iter()
        .find(|c| c["name"] == "Order" && c["kind"] == "html")
        .cloned()
        .unwrap()
}

/// The part's `reviewed` as a read says it: whether it can be used, and why not.
fn reviewed_now(core: &Core, branch: &str) -> (bool, Option<String>) {
    let read = ok(
        core,
        "card_read",
        json!({"branch": branch, "target": "UI/Home:Order", "value": {"include": ["actions"]}}),
    );
    let action = read["actions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["op"] == "reviewed")
        .cloned()
        .unwrap_or_else(|| panic!("{read}"));
    assert_eq!(action["label"], "見直した", "{action}");
    (
        action["available"].as_bool().unwrap(),
        action["why"].as_str().map(str::to_string),
    )
}

#[test]
fn a_part_is_marked_when_a_card_it_reaches_changes_after_it_and_reviewed_clears_it() {
    let core = core();
    linked_part(&core);
    // Written after the entity: nothing to look at again.
    assert!(review(&core, "main").is_null());
    assert!(detail_part(&core, "").get("review").is_none());
    let (usable, why) = reviewed_now(&core, "main");
    assert!(!usable && why.unwrap().contains("見直しの印がありません"));
    let why = refused(
        &core,
        "card_action",
        json!({"target": "UI/Home:Order", "op": "reviewed"}),
    );
    assert!(why.contains("見直しの印がありません"), "{why}");

    change_entity(&core, "main", "合計を足した");
    let marks = review(&core, "main");
    assert_eq!(marks.as_array().unwrap().len(), 1, "{marks}");
    assert_eq!(marks[0]["path"], "Schemas/Ordering:Order", "{marks}");
    assert!(marks[0]["at"].as_str().unwrap().ends_with('Z'), "{marks}");
    assert_eq!(detail_part(&core, "")["review"], marks);
    assert_eq!(reviewed_now(&core, "main"), (true, None));
    // The check says it, as information.
    let found = common::checked(&core, "main");
    let items = found["review"]
        .as_array()
        .unwrap_or_else(|| panic!("{found}"));
    assert_eq!(items.len(), 1, "{found}");
    assert_eq!(items[0]["at"], "UI/Home:Order", "{found}");
    assert_eq!(items[0]["kind"], "info", "{found}");
    assert_eq!(items[0]["changed"], marks, "{found}");
    assert!(
        items[0]["problem"]
            .as_str()
            .unwrap()
            .contains("Schemas/Ordering:Order"),
        "{found}"
    );
    // The mock marks the part and draws its dot; `notes: false` leaves both out.
    let html = mock(&core, "UI/Home", json!({}));
    assert!(
        html.contains("data-path=\"UI/Home:Order\" data-kind=\"html\" data-review=\"[{&quot;path&quot;:&quot;Schemas/Ordering:Order&quot;,&quot;at&quot;:"),
        "{html}"
    );
    assert!(
        html.contains(
            "<div class=\"sb-review\" data-path=\"UI/Home:Order\" title=\"Schemas/Ordering:Order ("
        ),
        "{html}"
    );
    let bare = mock(&core, "UI/Home", json!({"notes": false}));
    assert!(!bare.contains("review"), "{bare}");
    // Moving the part is no change of it.
    put(&core, "UI/Home", &[("UI/Home:Order", (4, 4), (10, 3))]);
    assert_eq!(review(&core, "main"), marks);

    let done = ok(
        &core,
        "card_action",
        json!({"target": "UI/Home:Order", "op": "reviewed"}),
    );
    assert_eq!(done["op"], "reviewed", "{done}");
    let changes = done["changes"].to_string();
    assert!(
        changes.contains("Schemas/Ordering:Order の変更を見直した")
            && changes.contains("reviewedAt"),
        "{done}"
    );
    assert!(review(&core, "main").is_null());
    assert!(detail_part(&core, "").get("review").is_none());
    assert!(
        detail_part(&core, "")["form"]["fields"]["reviewedAt"]
            .as_str()
            .is_some_and(|t| t.ends_with('Z'))
    );
    assert!(common::checked(&core, "main").get("review").is_none());
    let (usable, _) = reviewed_now(&core, "main");
    assert!(!usable);
    let lines = op(
        &core,
        json!({"op": "commits", "project": common::project_id(&core), "branch": "", "limit": 1}),
    );
    let line = lines["result"]["commits"][0]["lines"][0]["summary"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(
        line.starts_with("カード UI/Home:Order を見直した（"),
        "{line}"
    );

    // Changed again, marked again; writing the part itself clears it as well.
    change_entity(&core, "main", "また変えた");
    assert!(review(&core, "main").is_array());
    ok(
        &core,
        "card_update",
        json!({"target": "UI/Home:Order", "value": {"note": "合計の欄を足す"}}),
    );
    assert!(review(&core, "main").is_null());
}

#[test]
fn a_branch_sees_main_up_to_where_it_was_made_and_its_own_changes() {
    let core = core();
    linked_part(&core);
    ok(&core, "branch_create", json!({"target": "notes-b"}));
    let branch: String = rusqlite::Connection::open(core.place())
        .unwrap()
        .query_row("SELECT id FROM branches WHERE name = 'notes-b'", [], |r| {
            r.get(0)
        })
        .unwrap();
    // main changing the entity after the branch was made is not the branch's to see.
    change_entity(&core, "main", "main で変えた");
    assert!(review(&core, "main").is_array());
    assert!(review(&core, "notes-b").is_null());
    assert!(detail_part(&core, &branch).get("review").is_none());
    // The branch's own change is.
    change_entity(&core, "notes-b", "branch で変えた");
    let marks = review(&core, "notes-b");
    assert_eq!(marks[0]["path"], "Schemas/Ordering:Order", "{marks}");
    assert_eq!(detail_part(&core, &branch)["review"], marks);
    // Reviewed on the branch, cleared there and not on main.
    ok(
        &core,
        "card_action",
        json!({"branch": "notes-b", "target": "UI/Home:Order", "op": "reviewed"}),
    );
    assert!(review(&core, "notes-b").is_null());
    assert!(review(&core, "main").is_array());
}

#[test]
fn kind_read_says_the_parts_note_fields_and_their_action() {
    let core = core();
    for kind in ["html", "svg", "frame"] {
        let read = ok(&core, "kind_read", json!({"target": kind}));
        let fields: Vec<&str> = read["fields"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|f| f["name"].as_str())
            .collect();
        for name in ["note", "notePlace", "reviewedAt"] {
            assert!(fields.contains(&name), "{kind} {name}: {read}");
        }
        let place = read["fields"]
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["name"] == "notePlace")
            .unwrap();
        assert_eq!(place["default"], "topRight", "{place}");
        assert_eq!(place["optionLabels"]["bottomLeft"], "左下", "{place}");
        assert_eq!(read["actions"][0]["op"], "reviewed", "{read}");
        assert_eq!(read["actions"][0]["label"], "見直した", "{read}");
    }
}

#[test]
fn card_list_narrows_to_the_parts_by_one_kind_or_a_list_of_them_with_their_actions() {
    let core = core();
    ok(
        &core,
        "card_create",
        json!({"value": {
            "UI/Home:Title": {"kind": "html", "body": "<h1>ホーム</h1>"},
            "UI/Home:Logo": {"kind": "svg", "body": "<svg xmlns=\"http://www.w3.org/2000/svg\"/>"},
            "UI/Home:Top": {"kind": "frame", "target": "UI/Header"},
            "UI/Home:Memo": {"kind": "note", "body": "あとで決める"}
        }}),
    );
    let paths = |value: Value| -> Vec<String> {
        let listed = ok(&core, "card_list", json!({"value": value}));
        let mut paths: Vec<String> = listed["cards"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["path"].as_str().unwrap().to_string())
            .collect();
        paths.sort();
        paths
    };
    assert_eq!(paths(json!({"kind": "html"})), ["UI/Home:Title"]);
    assert_eq!(paths(json!({"kind": "svg"})), ["UI/Home:Logo"]);
    assert_eq!(paths(json!({"kind": "frame"})), ["UI/Home:Top"]);
    assert_eq!(
        paths(json!({"kind": ["html", "svg", "frame"]})),
        ["UI/Home:Logo", "UI/Home:Title", "UI/Home:Top"]
    );
    let listed = ok(
        &core,
        "card_list",
        json!({"value": {"kind": ["html", "svg", "frame"], "include": ["actions"]}}),
    );
    let cards = listed["cards"].as_array().unwrap();
    assert_eq!(cards.len(), 3, "{listed}");
    for card in cards {
        assert_eq!(card["actions"][0]["op"], "reviewed", "{card}");
    }
    let why = refused(
        &core,
        "card_list",
        json!({"value": {"kind": ["html", "hazard"]}}),
    );
    assert!(why.contains("hazard"), "{why}");
}
