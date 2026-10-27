//! The board's cards reaching main's: a `link` from a board's card to one of main's, written by
//! main's path and kept by its id — read back by main's path, following a rename, said broken when
//! main's card is gone; a path in full written in a board's text found in main; main's card read
//! with the board's cards still to do that reach it (`fromBoard`), its deletion saying what it
//! breaks. No other kind of line crosses, and main reaches nothing of the board's.

use crate::common;

use common::{checked, ok, refused, script_store};
use serde_json::{Value, json};
use specboard_core::Core;

const PROJECT: &str = "見本";

fn board(arguments: Value) -> Value {
    let mut arguments = arguments;
    arguments["branch"] = "_project_board_".into();
    arguments
}

/// The board's cards main's card at `path` is read with, by their paths.
fn from_board(core: &Core, path: &str) -> Vec<String> {
    ok(core, "card_read", json!({"target": path}))["fromBoard"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|r| r["path"].as_str().map(str::to_string))
        .collect()
}

#[test]
fn a_board_card_links_to_main_s_card_by_its_path_and_keeps_it_by_its_id() {
    let core = Core::open(script_store()).unwrap();
    ok(
        &core,
        "card_create",
        board(json!({"value": {
            "Goals:Shape": {"kind": "goal", "label": "形を決める", "status": "inProgress"},
            "Goals:Shape.Decide": {"kind": "task", "label": "決める",
                "connections": [{"line": "link", "to": "Docs:Spec"}]},
            "Goals:Shape.Write": {"kind": "task", "label": "書く", "body": "直すのは [[Docs:Spec]] と [[Schemas/Ordering:Order.id]]"}
        }})),
    );
    // Read back by main's path.
    let decide = ok(
        &core,
        "card_read",
        board(json!({"target": "Goals:Shape.Decide"})),
    );
    assert_eq!(decide["connections"][0]["to"], "Docs:Spec", "{decide}");
    assert!(
        decide["refersTo"].to_string().contains("\"Docs:Spec\""),
        "{decide}"
    );
    // What it may draw to main's card: a link alone, landing there by main's path.
    let lines = ok(
        &core,
        "card_read",
        board(
            json!({"target": "Goals:Shape.Decide", "value": {"as": "lineKinds", "to": "Schemas/Ordering:Order"}}),
        ),
    );
    let kinds: Vec<&str> = lines["lines"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|l| l["line"].as_str())
        .collect();
    assert_eq!(kinds, ["link"], "{lines}");
    assert_eq!(lines["to"], "Schemas/Ordering:Order", "{lines}");
    // Its diagram: drawn without main's card, not refused.
    ok(
        &core,
        "card_read",
        board(json!({"target": "Goals:Shape.Decide", "value": {"as": "diagram"}})),
    );
    // A path in full written in a board's text names main's card: nothing broken on the board.
    let after = checked(&core, "_project_board_");
    assert!(after.get("brokenPaths").is_none(), "{after}");
    assert!(after.get("lineProblems").is_none(), "{after}");
    // main's cards read with the board's still to do that reach them, or the row read.
    assert_eq!(
        from_board(&core, "Docs:Spec"),
        ["Goals:Shape.Decide", "Goals:Shape.Write"]
    );
    let spec = ok(&core, "card_read", json!({"target": "Docs:Spec"}));
    assert_eq!(spec["fromBoard"][0]["kind"], "task", "{spec}");
    assert_eq!(spec["fromBoard"][0]["done"], false, "{spec}");
    assert_eq!(
        from_board(&core, "Schemas/Ordering:Order"),
        ["Goals:Shape.Write"]
    );
    assert_eq!(
        from_board(&core, "Schemas/Ordering:Order.id"),
        ["Goals:Shape.Write"]
    );
    // Done, it is no longer listed; nor what is inside a goal finished.
    ok(
        &core,
        "card_update",
        board(json!({"target": "Goals:Shape.Decide", "value": {"done": true}})),
    );
    assert_eq!(from_board(&core, "Docs:Spec"), ["Goals:Shape.Write"]);
    ok(
        &core,
        "card_update",
        board(json!({"target": "Goals:Shape", "value": {"status": "cancelled"}})),
    );
    assert!(from_board(&core, "Docs:Spec").is_empty());
    let spec = ok(&core, "card_read", json!({"target": "Docs:Spec"}));
    assert!(spec.get("fromBoard").is_none(), "{spec}");

    // main's card renamed: the line follows it.
    ok(
        &core,
        "card_update",
        json!({"target": "Docs:Spec", "value": {"name": "Specs"}}),
    );
    let decide = ok(
        &core,
        "card_read",
        board(json!({"target": "Goals:Shape.Decide"})),
    );
    assert_eq!(decide["connections"][0]["to"], "Docs:Specs", "{decide}");

    // main's card deleted: said, and the line said broken on the board.
    let deleted = ok(&core, "card_delete", json!({"target": "Docs:Specs"}));
    let warnings = deleted["warnings"].to_string();
    assert!(warnings.contains("ボードの 2 枚"), "{deleted}");
    let after = checked(&core, "_project_board_");
    let broken = after["brokenPaths"].to_string();
    assert!(broken.contains("相手のカードがありません"), "{after}");
    assert!(
        broken.contains("Docs:Specs") && !broken.contains("あるのは: Goals"),
        "{after}"
    );
}

#[test]
fn only_a_link_crosses_and_only_from_the_board() {
    let core = Core::open(script_store()).unwrap();
    // A line of another kind does not cross: one that touches a type (a project's own, `designate`).
    ok(
        &core,
        "kind_create",
        json!({"target": "cites", "value": {"of": "line", "label": "引く", "base": "designate",
            "from": [{"kinds": ["note"], "at": "card"}], "to": {"kinds": ["*"], "at": ["card"]}}}),
    );
    let said = refused(
        &core,
        "card_create",
        board(
            json!({"target": "Goals:Odd", "value": {"kind": "note", "body": "メモ",
            "connections": [{"line": "cites", "to": "Docs:Spec"}]}}),
        ),
    );
    assert!(
        said.contains("ボードから main のカード（Docs:Spec）へ引けるのは、型に関係しない線（base が link のもの。つながる など）だけです"),
        "{said}"
    );
    // A line picked out of a text is not written at all, across or not.
    let said = refused(
        &core,
        "card_create",
        board(
            json!({"target": "Goals:Odd", "value": {"kind": "note", "body": "メモ",
            "connections": [{"line": "mentions", "to": "Docs:Spec"}]}}),
        ),
    );
    assert!(
        said.contains(
            "mentions（言及）は本文の [[ ]] から拾う線なので、connections には書きません"
        ),
        "{said}"
    );
    // main reaches nothing of the board's.
    ok(
        &core,
        "card_create",
        board(json!({"target": "Goals:Plan", "value": {"kind": "goal"}})),
    );
    let said = refused(
        &core,
        "card_update",
        json!({"target": "Docs:Spec", "value": {"connections": {"add": [{"line": "link", "to": "Goals:Plan"}]}}}),
    );
    // Said as that: a card of the board's, not one that is not there — when made, a
    // branch's too.
    assert!(
        said.contains("「Goals:Plan」はボードのカードです。main からボードへは線を引けません（ボードから main へは引けます）"),
        "{said}"
    );
    assert!(!said.contains("というカードはありません"), "{said}");
    let said = refused(
        &core,
        "card_create",
        json!({"target": "Free:Ask", "value": {"kind": "note", "body": "メモ",
            "connections": [{"line": "link", "to": "Goals:Plan"}, {"line": "link", "to": "Goals:Nothing"}]}}),
    );
    assert!(said.contains("main からボードへは線を引けません"), "{said}");
    assert!(
        said.contains("「Goals:Nothing」というカードはありません"),
        "{said}"
    );
    ok(&core, "branch_create", json!({"target": "plan"}));
    let said = refused(
        &core,
        "card_update",
        json!({"branch": "plan", "target": "Docs:Spec",
            "value": {"connections": {"add": [{"line": "link", "to": "Goals:Plan"}]}}}),
    );
    assert!(
        said.contains("branch「plan」からボードへは線を引けません"),
        "{said}"
    );
    // A path that names nothing in either tree is refused as before.
    let said = refused(
        &core,
        "card_create",
        board(
            json!({"target": "Goals:Plan.Lost", "value": {"kind": "task",
            "connections": [{"line": "link", "to": "Docs:Nowhere"}]}}),
        ),
    );
    assert!(
        said.contains("「Docs:Nowhere」というカードはありません"),
        "{said}"
    );
}

#[test]
fn a_path_to_main_written_on_the_board_follows_main_s_rename_merged_in() {
    let core = Core::open(script_store()).unwrap();
    ok(
        &core,
        "card_create",
        board(
            json!({"target": "Goals:Shape", "value": {"kind": "goal", "label": "形を決める",
            "body": "直すのは [[Docs:Spec.intro]] と [[Goals:Shape]]"}}),
        ),
    );
    ok(&core, "branch_create", json!({"target": "rename"}));
    ok(
        &core,
        "card_update",
        json!({"target": "Docs:Spec", "branch": "rename", "value": {"name": "Specs"}}),
    );
    // Not merged yet: the board names main as it is.
    let goal = ok(&core, "card_read", board(json!({"target": "Goals:Shape"})));
    assert!(
        goal["body"]
            .as_str()
            .unwrap()
            .contains("[[Docs:Spec.intro]]"),
        "{goal}"
    );
    ok(
        &core,
        "branch_merge",
        json!({"target": "rename", "value": {"into": "main"}}),
    );
    let goal = ok(&core, "card_read", board(json!({"target": "Goals:Shape"})));
    let body = goal["body"].as_str().unwrap();
    assert!(body.contains("[[Docs:Specs.intro]]"), "{goal}");
    // The board's own path is the board's, left as it was.
    assert!(body.contains("[[Goals:Shape]]"), "{goal}");
    assert_eq!(from_board(&core, "Docs:Specs.intro"), ["Goals:Shape"]);
}

#[test]
fn the_window_reads_the_board_s_lines_to_main_and_main_s_cards_with_what_to_do() {
    let core = Core::open(script_store()).unwrap();
    ok(
        &core,
        "card_create",
        board(json!({"value": {
            "Goals:Shape": {"kind": "goal", "label": "形を決める", "status": "inProgress",
                "body": "[[Docs:Spec]] の形を決める"},
            "Goals:Shape.Decide": {"kind": "task",
                "connections": [{"line": "link", "to": "Docs:Spec", "at": "intro"}]},
            "Goals:Note": {"kind": "note", "body": "[[Schemas/Ordering:Order]] も見る"}
        }})),
    );
    let project: String = rusqlite::Connection::open(core.place())
        .unwrap()
        .query_row("SELECT id FROM projects WHERE name = ?", [PROJECT], |r| {
            r.get(0)
        })
        .unwrap();
    let detail: Value = serde_json::from_str(&core.call(
        &json!({"op": "project_detail", "project": project, "branch": "~board"}).to_string(),
    ))
    .unwrap();
    let detail = &detail["result"];
    let id_of = |name: &str| -> String {
        detail["schemas"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["name"] == name)
            .map(|c| c["id"].as_str().unwrap().to_string())
            .unwrap()
    };
    let main: Value = serde_json::from_str(
        &core.call(&json!({"op": "project_detail", "project": project, "branch": ""}).to_string()),
    )
    .unwrap();
    let main_id = |name: &str| -> String {
        main["result"]["schemas"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["name"] == name)
            .map(|c| c["id"].as_str().unwrap().to_string())
            .unwrap()
    };
    let relations = detail["relations"].as_array().unwrap();
    let to_spec = relations
        .iter()
        .find(|r| r["fromSchemaId"] == id_of("Decide").as_str())
        .unwrap_or_else(|| panic!("{relations:?}"));
    assert_eq!(to_spec["toSchemaId"], main_id("Spec").as_str());
    assert_eq!(to_spec["at"], "intro");
    assert_eq!(to_spec["outside"], true);
    let mention = relations
        .iter()
        .find(|r| r["fromSchemaId"] == id_of("Note").as_str())
        .unwrap_or_else(|| panic!("{relations:?}"));
    assert_eq!(mention["toSchemaId"], main_id("Order").as_str());
    assert_eq!(mention["line"], "mentions");
    assert_eq!(mention["outside"], true);
    // main's detail draws nothing of the board's.
    assert!(
        !main["result"]["relations"]
            .to_string()
            .contains(&id_of("Decide"))
    );

    // `include` names it, or leaves it out.
    let spec = ok(
        &core,
        "card_read",
        json!({"target": "Docs:Spec", "value": {"include": ["from_board"]}}),
    );
    assert_eq!(spec["fromBoard"][0]["path"], "Goals:Shape", "{spec}");
    assert_eq!(spec["fromBoard"][0]["status"], "inProgress", "{spec}");
    assert_eq!(spec["fromBoard"][1]["path"], "Goals:Shape.Decide", "{spec}");
    assert_eq!(spec["fromBoard"][1]["at"], "intro", "{spec}");
    let spec = ok(
        &core,
        "card_read",
        json!({"target": "Docs:Spec", "value": {"include": ["fields"]}}),
    );
    assert!(spec.get("fromBoard").is_none(), "{spec}");
    // A row read: those that land in it.
    assert_eq!(from_board(&core, "Docs:Spec.intro"), ["Goals:Shape.Decide"]);
    assert!(from_board(&core, "Docs:Spec.tables").is_empty());
    // A note has nothing to be done by: listed while it is there.
    assert_eq!(from_board(&core, "Schemas/Ordering:Order"), ["Goals:Note"]);
}
