//! A shape's list face (`specboard://SpecBoard/Docs/Design:CardKinds`「データの一覧の顔」): written on an entity
//! or a value over MCP and read back, held to the shape's fields as it is written, following their
//! renames, an enum value's rename and a field taken away, and drawn for the screen as one row an
//! element of a data card's list, every row folded, the card as tall as those rows.

use crate::common;

use common::{ok, op, project_id, refused, script_store};
use serde_json::{Value, json};
use specboard_core::Core;

/// The face of a test file: the file's shape lists its tests, a test's shape says its text, note,
/// mark and what unfolds, and a step's its text and its details.
fn suite_face() -> Value {
    json!({"list": "tests", "header": ["file", "runner"]})
}

fn case_face() -> Value {
    json!({"text": "checks", "note": "name",
        "mark": {"field": "result", "values": {"passed": "ok", "failed": "bad", "skipped": "muted"}, "none": "hollow"},
        "fold": ["given"]})
}

/// The sample project with a definition section `Data` holding the shapes of a test file and the
/// data card `map_groups` of three tests: one passed with a step, one failed, one not run.
fn with_tests() -> Core {
    let core = Core::open(script_store()).unwrap();
    let mut tree = ok(&core, "project_read", json!({}))["projectTree"].clone();
    tree["main_sections"]
        .as_array_mut()
        .unwrap()
        .push(json!({"type": "definition", "name": "Data"}));
    ok(
        &core,
        "project_update",
        json!({"value": {"project_tree": tree}}),
    );
    ok(
        &core,
        "card_create",
        json!({"value": {
            "Data:Result": {"kind": "enum", "values": [
                {"name": "passed", "label": "通った"}, {"name": "failed"}, {"name": "skipped"}]},
            "Data:Runner": {"kind": "enum", "values": [
                {"name": "core", "label": "core.yml"}, {"name": "app", "label": "test.yml"}]},
            "Data:Step": {"kind": "value", "face": {"text": "text", "fold": ["more"]}, "fields": [
                {"name": "text", "type": "string"},
                {"name": "more", "type": "string", "array": true}]},
            "Data:Case": {"kind": "value", "face": case_face(), "fields": [
                {"name": "name", "type": "string", "primaryKey": true},
                {"name": "checks", "type": "text"},
                {"name": "result", "type": "string"},
                {"name": "given", "type": "string", "label": "Given", "array": true}],
                "connections": [
                    {"line": "embed", "from": "result", "to": "Data:Result"},
                    {"line": "embed", "from": "given", "to": "Data:Step"}]},
            "Data:Suite": {"kind": "entity", "face": suite_face(), "fields": [
                {"name": "file", "type": "string"},
                {"name": "runner", "type": "string"},
                {"name": "tests", "type": "string", "array": true}],
                "connections": [
                    {"line": "embed", "from": "runner", "to": "Data:Runner"},
                    {"line": "embed", "from": "tests", "to": "Data:Case"}]}
        }}),
    );
    ok(
        &core,
        "card_create",
        json!({"target": "Data:map_groups", "value": {"kind": "instance", "of": "Data:Suite",
            "data": {"file": "map_groups.rs", "runner": "core", "tests": [
                {"name": "one", "checks": "一つ目を確かめる", "result": "passed",
                 "given": [{"text": "前提", "more": ["x", "y"]}]},
                {"name": "two", "result": "failed"},
                {"name": "three"}]}}}),
    );
    core
}

fn face_of(core: &Core, card: &str) -> Value {
    ok(core, "card_read", json!({"target": card}))["face"].clone()
}

/// A card as the screen reads it (`project_detail`).
fn on_screen(core: &Core, name: &str) -> Value {
    let detail = op(
        core,
        json!({"op": "project_detail", "project": project_id(core)}),
    )["result"]
        .clone();
    detail["schemas"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == name)
        .cloned()
        .unwrap_or_else(|| panic!("{name}"))
}

#[test]
fn a_face_is_written_on_a_shape_read_back_and_taken_off() {
    let core = with_tests();
    assert_eq!(face_of(&core, "Data:Suite"), suite_face());
    assert_eq!(face_of(&core, "Data:Case"), case_face());
    // The screen reads it on the shape too.
    assert_eq!(on_screen(&core, "Suite")["face"], suite_face());
    ok(
        &core,
        "card_update",
        json!({"target": "Data:Suite", "value": {"face": {"list": "tests"}}}),
    );
    assert_eq!(face_of(&core, "Data:Suite"), json!({"list": "tests"}));
    // Taken off with an empty face, or with null as any key a card_update clears.
    ok(
        &core,
        "card_update",
        json!({"target": "Data:Suite", "value": {"face": {}}}),
    );
    assert_eq!(face_of(&core, "Data:Suite"), Value::Null);
    ok(
        &core,
        "card_update",
        json!({"target": "Data:Case", "value": {"face": null}}),
    );
    assert_eq!(face_of(&core, "Data:Case"), Value::Null);
    assert!(on_screen(&core, "Case").get("face").is_none());
    // Only a record's shape has one.
    let on_enum = refused(
        &core,
        "card_update",
        json!({"target": "Data:Result", "value": {"face": {"list": "x"}}}),
    );
    assert!(on_enum.contains("face"), "{on_enum}");
}

#[test]
fn a_face_that_does_not_fit_its_fields_is_refused_with_every_problem_and_nothing_written() {
    let core = with_tests();
    let said = refused(
        &core,
        "card_update",
        json!({"target": "Data:Case", "value": {"face": {
            "text": "nope", "note": "given", "list": "checks",
            "mark": {"field": "checks", "values": {"x": "green"}, "none": "red"},
            "fold": ["name", "given", "given"]}}}),
    );
    for wanted in [
        "Data:Case.face.text: この形に欄「nope」はありません",
        "Data:Case.face.note: given は文字（string・text）か列挙の 1 つの値の欄ではない",
        "Data:Case.face.list: checks は entity・value の形の要素の並びではない",
        "Data:Case.face.mark.field: checks は列挙か bool の 1 つの値の欄ではない",
        "Data:Case.face.mark.values.x: \"green\" は印の役ではありません",
        "Data:Case.face.mark.none: \"red\" は印の役ではありません",
        "Data:Case.face.fold[1]: given は行の字・後ろの字・印に出している",
        "Data:Case.face.fold[2]: given を 2 度書いています",
    ] {
        assert!(said.contains(wanted), "wanted「{wanted}」: {said}");
    }
    // A key a face does not have: refused by the form, as the core refuses it.
    let said = refused(
        &core,
        "card_update",
        json!({"target": "Data:Case", "value": {"face": {"text": "checks", "extra": 1}}}),
    );
    assert!(said.contains("extra"), "{said}");
    // An enum's value a mark maps from is one of its values.
    let said = refused(
        &core,
        "card_update",
        json!({"target": "Data:Case", "value": {"face": {
            "mark": {"field": "result", "values": {"broken": "bad"}}}}}),
    );
    assert!(
        said.contains("Data:Case.face.mark.values.broken: broken は欄の値ではありません"),
        "{said}"
    );
    // A card made with a wrong face is not made.
    let said = refused(
        &core,
        "card_create",
        json!({"target": "Data:Odd", "value": {"kind": "value", "face": {"text": "x"},
            "fields": [{"name": "y", "type": "string"}]}}),
    );
    assert!(said.contains("Data:Odd.face.text"), "{said}");
    assert_eq!(face_of(&core, "Data:Case"), case_face());
}

#[test]
fn a_face_follows_its_fields_renamed_or_taken_away_and_its_enums_values_renamed() {
    let core = with_tests();
    ok(
        &core,
        "card_update",
        json!({"target": "Data:Case.result", "value": {"name": "outcome"}}),
    );
    ok(
        &core,
        "card_update",
        json!({"target": "Data:Case.checks", "value": {"name": "says"}}),
    );
    ok(
        &core,
        "card_update",
        json!({"target": "Data:Result.passed", "value": {"name": "pass"}}),
    );
    assert_eq!(
        face_of(&core, "Data:Case"),
        json!({"text": "says", "note": "name",
            "mark": {"field": "outcome", "values": {"pass": "ok", "failed": "bad", "skipped": "muted"}, "none": "hollow"},
            "fold": ["given"]})
    );
    ok(&core, "card_delete", json!({"target": "Data:Case.given"}));
    ok(&core, "card_delete", json!({"target": "Data:Suite.runner"}));
    assert_eq!(face_of(&core, "Data:Case")["fold"], json!([]));
    assert_eq!(face_of(&core, "Data:Suite")["header"], json!(["file"]));
}

#[test]
fn a_data_card_of_a_list_face_is_drawn_a_row_an_element_folded() {
    let core = with_tests();
    let card = on_screen(&core, "map_groups");
    let list = &card["listFace"];
    assert_eq!(list["list"], "tests", "{list}");
    assert_eq!(
        list["header"],
        json!([
            {"field": "file", "label": "file", "text": "map_groups.rs"},
            {"field": "runner", "label": "runner", "text": "core.yml"}
        ])
    );
    assert_eq!(
        list["counts"],
        json!([{"role": "ok", "count": 1}, {"role": "bad", "count": 1}, {"role": "hollow", "count": 1}])
    );
    assert_eq!(
        list["rows"][0],
        json!({"at": "tests[0]", "path": "tests[name=one]", "text": "一つ目を確かめる", "note": "one",
            "mark": "ok", "fold": [
                {"depth": 0, "label": "Given", "text": "前提"},
                {"depth": 1, "label": "more", "text": "x"},
                {"depth": 1, "text": "y"}]})
    );
    // No text: the key, said as falling back.
    assert_eq!(
        list["rows"][1],
        json!({"at": "tests[1]", "path": "tests[name=two]", "text": "two", "fallback": true,
            "note": "two", "mark": "bad"})
    );
    assert_eq!(list["rows"][2]["mark"], "hollow");
    // As tall as three rows, the height an enum of three values has; without a face, its tree.
    let height = |path: &str| ok(&core, "card_read", json!({"target": path}))["size"]["h"].clone();
    ok(
        &core,
        "card_create",
        json!({"target": "Data:Three", "value": {"kind": "enum", "values": [
            {"name": "a"}, {"name": "b"}, {"name": "c"}]}}),
    );
    assert_eq!(height("Data:map_groups"), height("Data:Three"));
    ok(
        &core,
        "card_update",
        json!({"target": "Data:Suite", "value": {"face": {}}}),
    );
    assert!(on_screen(&core, "map_groups").get("listFace").is_none());
    assert!(
        height("Data:map_groups").as_i64() > height("Data:Three").as_i64(),
        "{}",
        height("Data:map_groups")
    );
}
