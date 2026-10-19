//! SVG and HTML cards (`svg`, `html`; `specboard://SpecBoard/Docs/Design:CardKinds`「SVG と HTML のカード」):
//! a body drawn as a picture or a page, held in a document or a free section, sized by hand; an svg's
//! body refused unless it is SVG; a body read from a file of a working directory (`bodyFrom`); each
//! written out as a file of its own.

use crate::common;

use common::{ok, op, project_id, refused, script_store};
use serde_json::{Value, json};
use specboard_core::Core;

const PICTURE: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10"><rect width="10" height="10"/></svg>"#;
const PAGE: &str = "<style>p { color: red }</style><p>画面の案</p>";

fn core() -> Core {
    Core::open(script_store()).unwrap()
}

#[test]
fn svg_and_html_cards_are_made_read_and_changed_with_their_body() {
    let core = core();
    let made = ok(
        &core,
        "card_create",
        json!({"value": {
            "Docs:Logo": {"kind": "svg", "label": "ロゴ", "body": PICTURE},
            "Free:Mock": {"kind": "html", "body": PAGE}
        }}),
    );
    assert_eq!(made["created"].as_array().unwrap().len(), 2, "{made}");
    let read = ok(&core, "card_read", json!({"target": "Docs:Logo"}));
    assert_eq!(read["kind"], "svg");
    assert_eq!(read["label"], "ロゴ");
    assert_eq!(read["body"], PICTURE);
    let read = ok(&core, "card_read", json!({"target": "Free:Mock"}));
    assert_eq!(read["body"], PAGE);
    // Changed whole, and in part as any body.
    ok(
        &core,
        "card_update",
        json!({"target": "Free:Mock", "value": {"body": {"edit": [{"old": "画面の案", "new": "画面"}]}}}),
    );
    let read = ok(&core, "card_read", json!({"target": "Free:Mock"}));
    assert_eq!(read["body"], "<style>p { color: red }</style><p>画面</p>");
    let other = PICTURE.replace("10", "20");
    ok(
        &core,
        "card_update",
        json!({"target": "Docs:Logo", "value": {"body": other}}),
    );
    assert_eq!(
        ok(&core, "card_read", json!({"target": "Docs:Logo"}))["body"],
        other
    );
}

#[test]
fn an_svg_body_that_is_not_svg_is_refused_and_an_html_body_is_not_checked() {
    let core = core();
    for (body, said) in [
        ("<html><body/></html>", "根の要素が <html> です"),
        ("<svg><g></svg>", "SVG として読めません（1 行目"),
        ("ただの文", "要素がありません"),
    ] {
        let why = refused(
            &core,
            "card_create",
            json!({"target": "Docs:Bad", "value": {"kind": "svg", "body": body}}),
        );
        assert!(why.contains(said), "{body}: {why}");
    }
    ok(
        &core,
        "card_create",
        json!({"target": "Docs:Logo", "value": {"kind": "svg", "body": PICTURE}}),
    );
    let why = refused(
        &core,
        "card_update",
        json!({"target": "Docs:Logo", "value": {"body": "<svg>"}}),
    );
    assert!(why.contains("<svg> が閉じていません"), "{why}");
    assert_eq!(
        ok(&core, "card_read", json!({"target": "Docs:Logo"}))["body"],
        PICTURE
    );
    // Whatever an html card holds is a page: only what the page takes has effect.
    ok(
        &core,
        "card_create",
        json!({"target": "Docs:Loose", "value": {"kind": "html", "body": "<p>閉じていない <b>"}}),
    );
}

#[test]
fn a_document_or_a_free_section_holds_them_and_a_schema_section_does_not() {
    let core = core();
    for kind in ["svg", "html"] {
        let why = refused(
            &core,
            "card_create",
            json!({"target": "Schemas:Pic", "value": {"kind": kind, "body": PICTURE}}),
        );
        assert!(why.contains(kind), "{why}");
    }
    let kinds = ok(&core, "kind_read", json!({"target": "document"}));
    let holds = kinds["holds"].as_array().cloned().unwrap_or_default();
    assert!(
        holds.contains(&json!("svg")) && holds.contains(&json!("html")),
        "{kinds}"
    );
}

#[test]
fn they_are_sized_by_hand_and_as_big_as_a_document_card_until_then() {
    let core = core();
    ok(
        &core,
        "card_create",
        json!({"value": {
            "Docs:Logo": {"kind": "svg", "body": PICTURE},
            "Docs:Spec2": {"kind": "freedoc", "body": "# 見出し"}
        }}),
    );
    let size = |card: &str| {
        ok(
            &core,
            "card_read",
            json!({"target": card, "value": {"include": ["at"]}}),
        )["size"]
            .clone()
    };
    assert_eq!(size("Docs:Logo"), size("Docs:Spec2"));
    ok(
        &core,
        "card_update",
        json!({"target": "Docs:Logo", "value": {"size": {"w": 20, "h": 12}}}),
    );
    assert_eq!(size("Docs:Logo"), json!({"w": 20, "h": 12}));
    // The screen reads how tall the core places one with no size of its own.
    ok(
        &core,
        "card_update",
        json!({"target": "Docs:Logo", "value": {"size": null}}),
    );
    let detail = op(
        &core,
        json!({"op": "project_detail", "project": project_id(&core)}),
    );
    let card = detail["result"]["schemas"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "Logo")
        .cloned()
        .unwrap();
    assert_eq!(json!(card["faceRows"]), size("Docs:Logo")["h"], "{card}");
}

#[test]
fn guide_and_kind_read_tell_of_them() {
    let core = core();
    for kind in ["svg", "html"] {
        let read = ok(&core, "kind_read", json!({"target": kind}));
        assert!(read["face"].get("resizable").is_none(), "{read}");
        assert_eq!(read["face"][kind], "body", "{read}");
        let said = ok(&core, "guide", json!({"about": kind}));
        assert!(said.to_string().contains("body"), "{said}");
    }
}

/// A folder of a few files bound as the working directory `repo`, and a file outside it.
fn working_directory(core: &Core) -> std::path::PathBuf {
    let place = common::place("bodyfrom");
    let root = place.join("repo");
    for (path, text) in [
        ("mocks/logo.svg", PICTURE),
        ("mocks/checkout.html", PAGE),
        ("notes/idea.md", "# 案\n\n- 一つ目"),
        ("mocks/broken.svg", "<svg><g></svg>"),
    ] {
        let file = root.join(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, text).unwrap();
    }
    std::fs::write(
        root.join("mocks/latin1.html"),
        [0x3c, 0x70, 0x3e, 0xe9, 0xff],
    )
    .unwrap();
    std::fs::write(root.join("mocks/long.html"), "a".repeat(200_001)).unwrap();
    std::fs::write(place.join("secret.txt"), "外のファイル").unwrap();
    std::os::unix::fs::symlink(place.join("secret.txt"), root.join("mocks/link.html")).unwrap();
    std::os::unix::fs::symlink(
        root.join("mocks/checkout.html"),
        root.join("mocks/inside.html"),
    )
    .unwrap();
    let mut tree = ok(core, "project_read", json!({}))["projectTree"].clone();
    tree["main_sections"]
        .as_array_mut()
        .unwrap()
        .push(json!({"type": "workdir", "name": "repo"}));
    ok(
        core,
        "project_update",
        json!({"value": {"project_tree": tree}}),
    );
    let bound = op(
        core,
        json!({"op": "tool", "name": "section_update", "arguments": {
            "project": "見本", "target": "repo", "value": {"folder": root.to_string_lossy()}
        }}),
    );
    assert!(bound.get("error").is_none(), "{bound}");
    root
}

#[test]
fn a_body_is_read_from_a_file_of_a_working_directory_and_tied_to_nothing_after() {
    let core = core();
    let root = working_directory(&core);
    let made = ok(
        &core,
        "card_create",
        json!({"target": "Docs:Logo", "value": {"kind": "svg",
            "bodyFrom": {"directory": "repo", "path": "mocks/logo.svg"}}}),
    );
    assert_eq!(
        made["created"][0]["bodyFrom"],
        json!([{"at": "Docs:Logo", "directory": "repo", "path": "mocks/logo.svg", "chars": PICTURE.chars().count()}]),
        "{made}"
    );
    assert_eq!(
        ok(&core, "card_read", json!({"target": "Docs:Logo"}))["body"],
        PICTURE
    );
    // Any kind with a body, and several at once: each answer says its own.
    let made = ok(
        &core,
        "card_create",
        json!({"value": {
            "Free:Mock": {"kind": "html", "bodyFrom": {"directory": "Repo", "path": "./mocks//inside.html"}},
            "Free:Idea": {"kind": "note", "bodyFrom": {"directory": "repo", "path": "notes/idea.md"}},
            "Free:Plain": {"kind": "note", "body": "そのまま"}
        }}),
    );
    let said: Vec<Value> = made["created"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["bodyFrom"][0]["path"].clone())
        .collect();
    assert_eq!(
        said,
        [
            json!("./mocks//inside.html"),
            json!("notes/idea.md"),
            Value::Null
        ],
        "{made}"
    );
    assert_eq!(
        ok(&core, "card_read", json!({"target": "Free:Mock"}))["body"],
        PAGE
    );
    // Changed from a file; the file changed after does not change the card.
    let changed = ok(
        &core,
        "card_update",
        json!({"target": "Free:Mock", "value": {"bodyFrom": {"directory": "repo", "path": "notes/idea.md"}}}),
    );
    assert_eq!(changed["bodyFrom"][0]["chars"], 10, "{changed}");
    std::fs::write(root.join("notes/idea.md"), "書き換えた").unwrap();
    assert_eq!(
        ok(&core, "card_read", json!({"target": "Free:Mock"}))["body"],
        "# 案\n\n- 一つ目"
    );
}

#[test]
fn a_body_from_a_file_is_refused_with_why_and_nothing_is_written() {
    let core = core();
    working_directory(&core);
    let from = |directory: &str, path: &str| json!({"directory": directory, "path": path});
    for (value, said) in [
        (
            json!({"kind": "html", "body": "x", "bodyFrom": from("repo", "mocks/checkout.html")}),
            "body と一緒には書けません",
        ),
        (
            json!({"kind": "html", "bodyFrom": {"directory": "repo"}}),
            "bodyFrom は {\"directory\"",
        ),
        (
            json!({"kind": "html", "bodyFrom": from("nowhere", "a.html")}),
            "作業ディレクトリ「nowhere」はありません",
        ),
        (
            json!({"kind": "html", "bodyFrom": from("docs_2", "a.html")}),
            "この置き場でのフォルダが決まっていません",
        ),
        (
            json!({"kind": "html", "bodyFrom": from("repo", "../secret.txt")}),
            "外へ出ます",
        ),
        (
            json!({"kind": "html", "bodyFrom": from("repo", "/etc/hosts")}),
            "外へ出ます",
        ),
        (
            json!({"kind": "html", "bodyFrom": from("repo", "~/x.html")}),
            "外へ出ます",
        ),
        (
            json!({"kind": "html", "bodyFrom": from("repo", "mocks/link.html")}),
            "外へ出ます",
        ),
        (
            json!({"kind": "html", "bodyFrom": from("repo", "mocks/none.html")}),
            "ファイルがありません",
        ),
        (
            json!({"kind": "html", "bodyFrom": from("repo", "mocks")}),
            "ファイルではありません",
        ),
        (
            json!({"kind": "html", "bodyFrom": from("repo", "mocks/latin1.html")}),
            "UTF-8 の文字ではありません",
        ),
        (
            json!({"kind": "html", "bodyFrom": from("repo", "mocks/long.html")}),
            "上限 200000 字を超えます",
        ),
        (
            json!({"kind": "svg", "bodyFrom": from("repo", "mocks/broken.svg")}),
            "SVG として読めません",
        ),
    ] {
        let why = refused(
            &core,
            "card_create",
            json!({"target": "Docs:Page", "value": value}),
        );
        assert!(why.contains(said), "{value}: {why}");
        let gone = refused(&core, "card_read", json!({"target": "Docs:Page"}));
        assert!(!gone.is_empty());
    }
    // An update refused writes nothing, the other targets of the call neither.
    ok(
        &core,
        "card_create",
        json!({"value": {"Docs:A": {"kind": "note", "body": "前"}, "Docs:B": {"kind": "note", "body": "前"}}}),
    );
    let why = refused(
        &core,
        "card_update",
        json!({"value": {
            "Docs:A": {"body": "後"},
            "Docs:B": {"bodyFrom": from("repo", "../secret.txt")}
        }}),
    );
    assert!(why.contains("外へ出ます"), "{why}");
    for card in ["Docs:A", "Docs:B"] {
        assert_eq!(
            ok(&core, "card_read", json!({"target": card}))["body"],
            "前"
        );
    }
}

#[test]
fn each_is_written_out_as_a_file_of_its_own_and_an_html_page_keeps_to_the_apps_policy() {
    let core = core();
    let project = project_id(&core);
    let set = op(
        &core,
        json!({"op": "mcp_writes_set", "project": project, "group": "export", "state": "allow"}),
    );
    assert!(set.get("error").is_none(), "{set}");
    ok(
        &core,
        "card_create",
        json!({"value": {
            "Docs:Logo": {"kind": "svg", "body": PICTURE},
            "Docs:Mock": {"kind": "html", "label": "画面の案", "body": PAGE},
            "Free:Whole": {"kind": "html", "body": "<!doctype html><html><head><title>t</title></head><body>x</body></html>"}
        }}),
    );
    ok(
        &core,
        "export_create",
        json!({"target": "pages", "value": {"entries": [
            {"from": "Docs", "to": "pages/", "kinds": ["svg", "html"]},
            {"from": "Free:Whole", "to": "whole.html"}
        ]}}),
    );
    // A file that is not a folder takes the kind's extension; a format is not theirs.
    for (entry, said) in [
        (
            json!({"from": "Docs:Logo", "to": "logo.png"}),
            ".svg のファイル",
        ),
        (
            json!({"from": "Docs:Mock", "to": "mock.md"}),
            ".html のファイル",
        ),
        (
            json!({"from": "Docs:Mock", "to": "mock.html", "format": "typescript"}),
            "format は書けません",
        ),
    ] {
        let why = refused(
            &core,
            "export_create",
            json!({"target": "bad", "value": {"entries": [entry]}}),
        );
        assert!(why.contains(said), "{why}");
    }
    let folder = common::place("pages");
    std::fs::create_dir_all(&folder).unwrap();
    let wrote = ok(
        &core,
        "export_execute",
        json!({"target": "pages", "targetdir": folder.to_string_lossy()}),
    );
    let mut written: Vec<String> = wrote["exports"][0]["written"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p.as_str().unwrap().to_string())
        .collect();
    written.sort();
    assert_eq!(
        written,
        ["pages/Logo.svg", "pages/Mock.html", "whole.html"],
        "{wrote}"
    );
    assert_eq!(
        std::fs::read_to_string(folder.join("pages/Logo.svg")).unwrap(),
        PICTURE
    );
    let policy = "<meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; style-src 'unsafe-inline'; img-src data:; font-src data:; base-uri 'none'; form-action 'none'\">";
    let page = std::fs::read_to_string(folder.join("pages/Mock.html")).unwrap();
    assert!(page.starts_with("<!doctype html>\n<!-- "), "{page}");
    assert!(page.contains(policy), "{page}");
    assert!(page.contains("<title>画面の案</title>"), "{page}");
    assert!(page.contains(&format!("<body>\n{PAGE}\n</body>")), "{page}");
    let whole = std::fs::read_to_string(folder.join("whole.html")).unwrap();
    assert!(
        whole.contains(&format!(
            "<head>\n<meta charset=\"utf-8\">\n{policy}<title>t</title>"
        )),
        "{whole}"
    );
    // As written, export_read says each is now.
    let read = ok(
        &core,
        "export_read",
        json!({"target": "pages", "targetdir": folder.to_string_lossy()}),
    );
    for file in read["exports"][0]["files"].as_array().unwrap() {
        assert_eq!(file["status"], "current", "{file}");
    }
}

#[test]
fn reads_a_diagram_and_the_check_take_them_as_cards() {
    let core = core();
    let long: String = (1..=500).map(|n| format!("<p>{n}</p>\n")).collect();
    ok(
        &core,
        "card_create",
        json!({"value": {
            "Docs:Logo": {"kind": "svg", "body": PICTURE},
            "Docs:Long": {"kind": "html", "body": long}
        }}),
    );
    // A long body is cut as any body is, and read on by its lines.
    let read = ok(&core, "card_read", json!({"target": "Docs:Long"}));
    assert_eq!(read["truncated"], true, "{read}");
    let part = ok(
        &core,
        "card_read",
        json!({"target": "Docs:Long", "value": {"lines": {"from": 499, "to": 500}}}),
    );
    assert_eq!(part["body"], "<p>499</p>\n<p>500</p>", "{part}");
    // Not a document: not read as Markdown.
    let why = refused(
        &core,
        "card_read",
        json!({"target": "Docs:Logo", "value": {"as": "markdown"}}),
    );
    assert!(why.contains("svg"), "{why}");
    // The section's diagram draws them as cards.
    let diagram = ok(
        &core,
        "section_read",
        json!({"target": "Docs", "value": {"as": "diagram"}}),
    );
    let mermaid = diagram["mermaid"].as_str().unwrap();
    assert!(
        mermaid.contains("<<svg>> Docs__Logo") && mermaid.contains("<<html>> Docs__Long"),
        "{mermaid}"
    );
    // Nothing found of them by the check.
    let found = common::checked(&core, "main").to_string();
    assert!(
        !found.contains("Docs:Logo") && !found.contains("Docs:Long"),
        "{found}"
    );
}
