//! The specification (`spec`; `specboard://SpecBoard/Docs/Design:CardKinds`「仕様（spec）」): one card of a
//! story and scenarios, each scenario a heading and its steps (given, when, then, and, but), with a
//! path of its own to change, move, rename and draw lines from; written out as the spec documents
//! read; a scenario no test is tied to said by the inspection; a formatdoc made one in place.

use crate::common;

use common::{checked, ok, op, project_id, refused, script_store};
use serde_json::{Value, json};
use specboard_core::Core;

fn core() -> Core {
    Core::open(script_store()).unwrap()
}

/// The spec of drawing lines: two scenarios, the first with two steps of each kind it uses.
fn lines(core: &Core) {
    ok(
        core,
        "card_create",
        json!({"target": "Docs:Lines", "value": {"kind": "spec", "label": "線を引く",
        "as": "使う人と AI",
        "iWant": "カードの線を 1 本ずつ足し外ししたい",
        "soThat": "その間にほかの手が書いた線を消さない",
        "about": "設計は [[Schemas/Ordering:Order]] を見る",
        "scenarios": [
            {"name": "addRemove", "heading": "線を 1 本ずつ足し外しする", "steps": [
                {"given": "Order が線を 4 本持つ"},
                {"when": "add を書く"},
                {"then": "1 本足される"},
                {"and": "ほかはそのまま"}]},
            {"name": "heldToKinds", "heading": "線は種類の決まりに合うものだけが通る", "steps": [
                {"when": "note から引く"},
                {"then": "断られる"}]}
        ]}}),
    );
}

#[test]
fn a_spec_is_written_with_its_story_and_scenarios_and_read_back() {
    let core = core();
    lines(&core);
    let read = ok(&core, "card_read", json!({"target": "Docs:Lines"}));
    assert_eq!(read["kind"], "spec", "{read}");
    assert_eq!(read["as"], "使う人と AI", "{read}");
    assert_eq!(
        read["iWant"], "カードの線を 1 本ずつ足し外ししたい",
        "{read}"
    );
    assert_eq!(
        read["soThat"], "その間にほかの手が書いた線を消さない",
        "{read}"
    );
    assert_eq!(
        read["scenarios"],
        json!([
            {"path": "Docs:Lines.addRemove", "heading": "線を 1 本ずつ足し外しする"},
            {"path": "Docs:Lines.heldToKinds", "heading": "線は種類の決まりに合うものだけが通る"},
        ]),
        "{read}"
    );
    let scenario = ok(
        &core,
        "card_read",
        json!({"target": "Docs:Lines.addRemove"}),
    );
    assert_eq!(
        scenario["heading"], "線を 1 本ずつ足し外しする",
        "{scenario}"
    );
    assert_eq!(
        scenario["steps"],
        json!([
            {"given": "Order が線を 4 本持つ"},
            {"when": "add を書く"},
            {"then": "1 本足される"},
            {"and": "ほかはそのまま"}
        ]),
        "{scenario}"
    );
    // A step of two keys, a key no step has, a name taken: refused, nothing written.
    for (steps, name) in [
        (json!([{"given": "a", "when": "b"}]), "two"),
        (json!([{"because": "a"}]), "odd"),
        (json!([{"given": "a"}]), "addRemove"),
    ] {
        refused(
            &core,
            "card_create",
            json!({"target": format!("Docs:Lines.{name}"), "value": {"heading": "x", "steps": steps}}),
        );
    }
    let read = ok(&core, "card_read", json!({"target": "Docs:Lines"}));
    assert_eq!(read["scenarios"].as_array().unwrap().len(), 2, "{read}");
}

#[test]
fn a_scenario_is_written_moved_and_renamed_by_its_path() {
    let core = core();
    lines(&core);
    ok(
        &core,
        "card_update",
        json!({"target": "Docs:Lines", "value": {"connections": {"add": [
            {"line": "link", "from": "addRemove", "to": "Schemas:Customer"}]}}}),
    );
    ok(
        &core,
        "card_update",
        json!({"target": "Docs:Lines.addRemove", "value": {"heading": "線を足し外しする",
            "steps": [{"when": "add を書く"}, {"then": "足される"}]}}),
    );
    let scenario = ok(
        &core,
        "card_read",
        json!({"target": "Docs:Lines.addRemove"}),
    );
    assert_eq!(scenario["heading"], "線を足し外しする");
    assert_eq!(scenario["steps"].as_array().unwrap().len(), 2);
    let other = ok(
        &core,
        "card_read",
        json!({"target": "Docs:Lines.heldToKinds"}),
    );
    assert_eq!(other["steps"].as_array().unwrap().len(), 2, "{other}");
    // Moved before the other, by its name.
    ok(
        &core,
        "card_update",
        json!({"target": "Docs:Lines.heldToKinds", "value": {"before": "addRemove"}}),
    );
    let order: Vec<Value> = ok(&core, "card_read", json!({"target": "Docs:Lines"}))["scenarios"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["path"].clone())
        .collect();
    assert_eq!(
        order,
        [
            json!("Docs:Lines.heldToKinds"),
            json!("Docs:Lines.addRemove")
        ]
    );
    // Renamed: the line from it follows.
    ok(
        &core,
        "card_update",
        json!({"target": "Docs:Lines.addRemove", "value": {"name": "adding"}}),
    );
    let read = ok(
        &core,
        "card_read",
        json!({"target": "Docs:Lines", "value": {"include": ["connections"]}}),
    );
    assert!(
        read["connections"]
            .as_array()
            .unwrap()
            .iter()
            .any(|l| l["line"] == "link" && l["from"] == "adding"),
        "{read}"
    );
    // Made and let go of by its path.
    ok(
        &core,
        "card_create",
        json!({"target": "Docs:Lines.gone", "value": {"heading": "消える", "steps": [{"then": "無い"}]}}),
    );
    ok(&core, "card_delete", json!({"target": "Docs:Lines.gone"}));
    let read = ok(&core, "card_read", json!({"target": "Docs:Lines"}));
    assert_eq!(read["scenarios"].as_array().unwrap().len(), 2, "{read}");
}

#[test]
fn a_spec_is_markdown_as_the_spec_documents_are() {
    let core = core();
    lines(&core);
    let read = ok(
        &core,
        "card_read",
        json!({"target": "Docs:Lines", "value": {"as": "markdown"}}),
    );
    let markdown = read["markdown"]
        .as_str()
        .unwrap_or_else(|| panic!("{read}"));
    for wanted in [
        "# Feature: 線を引く\n",
        "## ねらい\n",
        "- **As** 使う人と AI\n- **I want** カードの線を 1 本ずつ足し外ししたい\n- **So that** その間にほかの手が書いた線を消さない\n",
        "## Scenario: 線を 1 本ずつ足し外しする\n<!-- row addRemove -->\n\n- **Given** Order が線を 4 本持つ\n- **When** add を書く\n- **Then** 1 本足される\n- **And** ほかはそのまま\n",
        "## Scenario: 線は種類の決まりに合うものだけが通る\n<!-- row heldToKinds -->\n",
    ] {
        assert!(markdown.contains(wanted), "wanted「{wanted}」:\n{markdown}");
    }
    // Drawn as its document, as a formatdoc may be.
    ok(
        &core,
        "card_update",
        json!({"target": "Docs:Lines", "value": {"view": "document"}}),
    );
    // Written out in English: the story under its English heading.
    ok(
        &core,
        "project_update",
        json!({"value": {"exportLanguage": "en"}}),
    );
    let read = ok(
        &core,
        "card_read",
        json!({"target": "Docs:Lines", "value": {"as": "markdown"}}),
    );
    let markdown = read["markdown"].as_str().unwrap();
    assert!(markdown.contains("## Story\n"), "{markdown}");
    assert!(!markdown.contains("## ねらい"), "{markdown}");
}

#[test]
fn a_scenario_without_a_test_is_inspected() {
    let core = core();
    lines(&core);
    ok(
        &core,
        "card_create",
        json!({"value": {
            "Docs:LinesTest": {"kind": "note", "body": "線のテスト"},
            "Docs:Plain": {"kind": "formatdoc", "rows": [{"name": "untested", "kind": "item", "heading": "テストの無い項目"}]}
        }}),
    );
    ok(
        &core,
        "card_update",
        json!({"target": "Docs:Lines", "value": {"connections": {"add": [
            {"line": "testedBy", "from": "addRemove", "to": "Docs:LinesTest"}]}}}),
    );
    let untested = |core: &Core| -> Vec<String> {
        checked(core, "main")["untestedScenarios"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|i| i["at"].as_str().unwrap().to_string())
            .collect()
    };
    assert_eq!(untested(&core), ["Docs:Lines.heldToKinds"]);
    // A line from the card's head covers every scenario.
    ok(
        &core,
        "card_update",
        json!({"target": "Docs:Lines", "value": {"connections": {"add": [
            {"line": "testedBy", "to": "Docs:LinesTest"}]}}}),
    );
    assert!(untested(&core).is_empty());
}

#[test]
fn a_formatdoc_becomes_a_spec_keeping_its_id_and_lines() {
    let core = core();
    ok(
        &core,
        "card_create",
        json!({"value": {
            "Docs:Old": {"kind": "formatdoc", "rows": [{"name": "addRemove", "kind": "item", "heading": "Scenario: 足し外し"}]},
            "Docs:OldTest": {"kind": "note", "body": "テスト"}
        }}),
    );
    ok(
        &core,
        "card_update",
        json!({"target": "Docs:Old", "value": {"connections": {"add": [
            {"line": "testedBy", "from": "addRemove", "to": "Docs:OldTest"}]}}}),
    );
    // The card's id, as the project's file has it.
    let id = |core: &Core| -> Value {
        let file: Value = serde_json::from_str(
            op(
                core,
                json!({"op": "project_file", "project": project_id(core)}),
            )["result"]["text"]
                .as_str()
                .unwrap(),
        )
        .unwrap();
        file["cards"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["name"] == "Old")
            .unwrap()["id"]
            .clone()
    };
    let before = id(&core);
    // The kind alone, the rows still there: refused.
    refused(
        &core,
        "card_update",
        json!({"target": "Docs:Old", "value": {"kind": "spec"}}),
    );
    ok(
        &core,
        "card_update",
        json!({"target": "Docs:Old", "value": {"kind": "spec", "rows": [],
            "as": "使う人", "scenarios": [{"name": "addRemove", "heading": "足し外し", "steps": [{"then": "通る"}]}]}}),
    );
    let read = ok(
        &core,
        "card_read",
        json!({"target": "Docs:Old", "value": {"include": ["connections"]}}),
    );
    assert_eq!(read["kind"], "spec", "{read}");
    assert_eq!(id(&core), before);
    assert!(
        read["connections"]
            .as_array()
            .unwrap()
            .contains(&json!({"line": "testedBy", "from": "addRemove", "to": "Docs:OldTest"})),
        "{read}"
    );
}

#[test]
fn scenarios_two_branches_changed_apart_both_go_in_with_their_steps() {
    let core = core();
    lines(&core);
    for name in ["a", "b"] {
        ok(&core, "branch_create", json!({"target": name}));
    }
    ok(
        &core,
        "card_update",
        json!({"branch": "a", "target": "Docs:Lines.addRemove", "value": {"steps": [{"when": "a が書く"}, {"then": "a で通る"}]}}),
    );
    ok(
        &core,
        "card_update",
        json!({"branch": "b", "target": "Docs:Lines.heldToKinds", "value": {"heading": "b の題"}}),
    );
    ok(
        &core,
        "card_create",
        json!({"branch": "b", "target": "Docs:Lines.added", "value": {"heading": "b が足した", "steps": [{"then": "b で足される"}]}}),
    );
    ok(&core, "branch_merge", json!({"target": "a"}));
    let preview = ok(
        &core,
        "branch_read",
        json!({"target": "b", "value": {"against": "main"}}),
    );
    assert!(
        preview["conflicts"].as_array().is_none_or(|c| c.is_empty()),
        "{preview}"
    );
    ok(&core, "branch_merge", json!({"target": "b"}));
    let read = |path: &str| ok(&core, "card_read", json!({"target": path}));
    assert_eq!(
        read("Docs:Lines.addRemove")["steps"],
        json!([{"when": "a が書く"}, {"then": "a で通る"}])
    );
    let held = read("Docs:Lines.heldToKinds");
    assert_eq!(held["heading"], "b の題");
    assert_eq!(held["steps"].as_array().unwrap().len(), 2, "{held}");
    assert_eq!(
        read("Docs:Lines.added")["steps"],
        json!([{"then": "b で足される"}])
    );

    // One scenario both sides changed: a conflict of that scenario alone.
    ok(&core, "branch_create", json!({"target": "c"}));
    for (tree, text) in [("c", "c の題"), ("main", "main の題")] {
        ok(
            &core,
            "card_update",
            json!({"branch": tree, "target": "Docs:Lines.added", "value": {"heading": text}}),
        );
    }
    ok(
        &core,
        "card_update",
        json!({"branch": "c", "target": "Docs:Lines.addRemove", "value": {"heading": "c の別の題"}}),
    );
    let preview = ok(
        &core,
        "branch_read",
        json!({"target": "c", "value": {"against": "main"}}),
    );
    let said: Vec<(&str, &str)> = preview["conflicts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| (c["path"].as_str().unwrap(), c["unit"].as_str().unwrap()))
        .collect();
    assert_eq!(said, [("Docs:Lines", "scenarios.added")], "{preview}");
}

#[test]
fn a_spec_is_as_tall_as_its_story_and_scenarios_drawn_as_rows() {
    let core = core();
    lines(&core);
    // Five rows each: the story's three and two scenarios, and five fields.
    let fields: Vec<Value> = ["a", "b", "c", "d", "e"]
        .iter()
        .map(|n| json!({"name": n, "type": "string"}))
        .collect();
    ok(
        &core,
        "card_create",
        json!({"target": "Schemas:Five", "value": {"fields": fields}}),
    );
    let size = |path: &str| ok(&core, "card_read", json!({"target": path}))["size"].clone();
    assert_eq!(size("Docs:Lines")["h"], size("Schemas:Five")["h"]);
    // The story's three rows whatever of it is written.
    ok(
        &core,
        "card_create",
        json!({"target": "Docs:Bare", "value": {"kind": "spec", "scenarios": [
            {"name": "a", "heading": "a", "steps": []}, {"name": "b", "heading": "b", "steps": []}]}}),
    );
    assert_eq!(size("Docs:Bare")["h"], size("Schemas:Five")["h"]);
}

#[test]
fn a_link_in_a_step_follows_a_rename_and_one_that_leads_nowhere_is_inspected() {
    let core = core();
    lines(&core);
    ok(
        &core,
        "card_create",
        json!({"target": "Docs:Target", "value": {"kind": "note", "body": "相手"}}),
    );
    ok(
        &core,
        "card_update",
        json!({"target": "Docs:Lines.addRemove", "value": {"steps": [
            {"given": "[[Docs:Target]] がある"}, {"then": "[[Docs:Nowhere]] を見る"}]}}),
    );
    ok(
        &core,
        "card_update",
        json!({"target": "Docs:Target", "value": {"name": "Moved"}}),
    );
    let steps = ok(
        &core,
        "card_read",
        json!({"target": "Docs:Lines.addRemove"}),
    )["steps"]
        .clone();
    assert_eq!(steps[0]["given"], "[[Docs:Moved]] がある", "{steps}");
    let broken = checked(&core, "main")["brokenPaths"].to_string();
    assert!(broken.contains("Docs:Nowhere"), "{broken}");
    assert!(!broken.contains("Docs:Moved"), "{broken}");
}

#[test]
fn a_link_written_out_to_a_scenario_lands_on_its_anchor() {
    let core = core();
    lines(&core);
    ok(
        &core,
        "card_create",
        json!({"target": "Docs:Guide", "value": {"kind": "freedoc", "body": "[[Docs:Lines.addRemove]] を読む"}}),
    );
    let written = |file: &str| -> String {
        ok(
            &core,
            "export_read",
            json!({"targetdir": "[default]", "target": "docs", "value": {"files": [file]}}),
        )["files"][0]["text"]
            .as_str()
            .unwrap()
            .to_string()
    };
    let lines = written("notes/Lines.md");
    assert!(lines.contains("<a id=\"row-addRemove\"></a>"), "{lines}");
    let guide = written("notes/Guide.md");
    assert!(guide.contains("(Lines.md#row-addRemove)"), "{guide}");
}

#[test]
fn what_a_scenarios_steps_name_is_drawn_from_its_row() {
    let core = core();
    lines(&core);
    ok(
        &core,
        "card_update",
        json!({"target": "Docs:Lines.heldToKinds", "value": {"steps": [{"when": "[[Schemas:Customer]] から引く"}]}}),
    );
    let read = ok(&core, "card_read", json!({"target": "Docs:Lines"}));
    assert_eq!(
        read["mentions"],
        json!(["Schemas/Ordering:Order", "Schemas:Customer"]),
        "{read}"
    );
    let detail = op(
        &core,
        json!({"op": "project_detail", "project": project_id(&core), "branch": "", "connection": "app"}),
    )["result"]
        .clone();
    let id_of = |name: &str| -> String {
        detail["schemas"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["name"] == name)
            .unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string()
    };
    let (lines_id, customer, order) = (id_of("Lines"), id_of("Customer"), id_of("Order"));
    let from = |to: &str| -> Vec<String> {
        detail["relations"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| {
                r["line"] == "mentions"
                    && r["fromSchemaId"] == lines_id.as_str()
                    && r["toSchemaId"] == to
            })
            .map(|r| r["from"].as_str().unwrap().to_string())
            .collect()
    };
    assert_eq!(from(&customer), ["heldToKinds"]);
    // The story's, from its head.
    assert_eq!(from(&order), [""]);
}

#[test]
fn a_scenarios_steps_are_found_by_query() {
    let core = core();
    ok(
        &core,
        "card_create",
        json!({"target": "Docs:Rev", "value": {"kind": "spec", "scenarios": [{"name": "cascade", "heading": "連なって消える",
            "steps": [{"given": "注文がある"}, {"when": "zephyr を押す"}, {"then": "行も消える"}]}]}}),
    );
    let found = ok(
        &core,
        "card_list",
        json!({"value": {"query": "zephyr", "section": "Docs"}}),
    );
    assert_eq!(found["matched"], 1, "{found}");
    let wheres: Vec<String> = found["cards"][0]["matches"]
        .as_array()
        .unwrap_or_else(|| panic!("{found}"))
        .iter()
        .map(|m| m["where"].as_str().unwrap_or("").to_string())
        .collect();
    assert!(
        wheres.iter().any(|w| w == "scenarios[0].steps[1].when"),
        "{found}"
    );
}
