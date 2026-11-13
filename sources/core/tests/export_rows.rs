//! An export's rows changed where they stand (`export_update`'s `entries.change`), and each file
//! saying the rows that write it (`export_read`'s `rows`): what the screen's panel for a row picked
//! is built on, and what MCP changes a row by without moving it to the end of the list.

use crate::common;

use common::{ok, op, place, refused, script_store};
use serde_json::{Value, json};
use specboard_core::Core;

/// The sample, MCP let set its exports.
fn core() -> Core {
    let core = Core::open(script_store()).unwrap();
    let said = op(
        &core,
        json!({"op": "mcp_writes_set", "project": common::project_id(&core), "group": "export", "state": "allow"}),
    );
    assert!(said.get("error").is_none(), "{said}");
    core
}

/// The export `rows` as `export_read` says it.
fn read(core: &Core) -> Value {
    let folder = place("rows-read");
    std::fs::create_dir_all(&folder).unwrap();
    ok(
        core,
        "export_read",
        json!({"targetdir": folder.to_str().unwrap(), "target": "rows"}),
    )["exports"][0]
        .clone()
}

fn entries(core: &Core) -> Value {
    read(core)["entries"].clone()
}

fn change(core: &Core, value: Value) -> Value {
    ok(
        core,
        "export_update",
        json!({"target": "rows", "value": {"entries": value}}),
    )
}

#[test]
fn a_row_is_changed_where_it_stands() {
    let core = core();
    ok(
        &core,
        "export_create",
        json!({"target": "rows", "value": {"entries": [
            {"from": "Schemas", "to": "types.ts"},
            {"from": "Docs", "to": "notes/"},
            {"from": "Charts/Shop:Checkout", "to": "charts.md"}]}}),
    );
    // The one row of its from, where it stands: only what is set.
    change(
        &core,
        json!({"change": [{"from": "Docs", "set": {"to": "pages/"}}]}),
    );
    assert_eq!(
        entries(&core),
        json!([
            {"from": "Schemas", "to": "types.ts"},
            {"from": "Docs", "to": "pages/"},
            {"from": "Charts/Shop:Checkout", "to": "charts.md"}])
    );
    // Keys written, then taken off again with null.
    change(
        &core,
        json!({"change": [{"from": "Schemas", "set": {"format": "zod", "kinds": ["entity"]}}]}),
    );
    assert_eq!(
        entries(&core)[0],
        json!({"from": "Schemas", "to": "types.ts", "format": "zod", "kinds": ["entity"]})
    );
    change(
        &core,
        json!({"change": [{"from": "Schemas", "set": {"format": null, "kinds": null}}]}),
    );
    assert_eq!(
        entries(&core)[0],
        json!({"from": "Schemas", "to": "types.ts"})
    );
    // A row of the cards' lines, picked by its from and to among the rows of one from.
    change(
        &core,
        json!({"add": [{"from": "Docs", "to": "lines.json", "format": "json-connections"}]}),
    );
    change(
        &core,
        json!({"change": [{"from": "Docs", "to": "lines.json", "set": {"lines": ["link"]}}]}),
    );
    assert_eq!(
        entries(&core)[3],
        json!({"from": "Docs", "to": "lines.json", "format": "json-connections", "lines": ["link"]})
    );
    // In one call: taken away, then changed, then added.
    change(
        &core,
        json!({
            "remove": [{"from": "Charts/Shop:Checkout"}],
            "change": [{"from": "Schemas", "set": {"to": "all.ts"}}],
            "add": [{"from": "Charts/Shop:Refund", "to": "charts.md"}]}),
    );
    assert_eq!(
        entries(&core),
        json!([
            {"from": "Schemas", "to": "all.ts"},
            {"from": "Docs", "to": "pages/"},
            {"from": "Docs", "to": "lines.json", "format": "json-connections", "lines": ["link"]},
            {"from": "Charts/Shop:Refund", "to": "charts.md"}])
    );
}

#[test]
fn a_change_that_cannot_stand_is_refused_and_nothing_changes() {
    let core = core();
    ok(
        &core,
        "export_create",
        json!({"target": "rows", "value": {"entries": [
            {"from": "Docs", "to": "notes/"},
            {"from": "Docs", "to": "lines.json", "format": "json-connections"},
            {"from": "Schemas", "to": "types.ts"},
            {"from": "Schemas", "to": "other.json", "format": "json-connections"}]}}),
    );
    let before = entries(&core);
    let no = |value: Value| {
        refused(
            &core,
            "export_update",
            json!({"target": "rows", "value": {"entries": value}}),
        )
    };
    // No such row: said with the rows there are.
    let none = no(json!({"change": [{"from": "Free", "set": {"to": "free/"}}]}));
    assert!(none.contains("Docs → notes/"), "{none}");
    // Two rows of one from: which is not said.
    let two = no(json!({"change": [{"from": "Docs", "set": {"to": "pages/"}}]}));
    assert!(
        two.contains("notes/") && two.contains("lines.json"),
        "{two}"
    );
    // What is set: the keys of a row, and something.
    let stray = no(json!({"change": [{"from": "Docs", "to": "notes/", "set": {"color": "red"}}]}));
    assert!(
        stray.contains("color") && stray.contains("kinds"),
        "{stray}"
    );
    let unset = no(json!({"change": [{"from": "Docs", "to": "notes/"}]}));
    assert!(unset.contains("set"), "{unset}");
    // The row as it would be is looked at as a row added is.
    let folder =
        no(json!({"change": [{"from": "Docs", "to": "lines.json", "set": {"to": "lines/"}}]}));
    assert!(folder.contains("lines/"), "{folder}");
    let lines =
        no(json!({"change": [{"from": "Schemas", "to": "types.ts", "set": {"lines": ["embed"]}}]}));
    assert!(lines.contains("json-connections"), "{lines}");
    let twice = no(
        json!({"change": [{"from": "Schemas", "to": "other.json", "set": {"to": "lines.json"}}]}),
    );
    assert!(
        twice.contains("lines.json") && twice.contains("2 つ"),
        "{twice}"
    );
    // A row the same call takes away is not there to change, and the call writes nothing.
    let gone = no(json!({
        "remove": [{"from": "Docs", "to": "notes/"}],
        "change": [{"from": "Docs", "to": "notes/", "set": {"to": "pages/"}}]}));
    assert!(gone.contains("notes/"), "{gone}");
    assert_eq!(entries(&core), before);
}

#[test]
fn a_file_says_the_rows_that_write_it() {
    let core = core();
    ok(
        &core,
        "export_create",
        json!({"target": "rows", "value": {"entries": [
            {"from": "Charts/Shop:Checkout", "to": "charts.md"},
            {"from": "Charts/Shop:Refund", "to": "charts.md"},
            {"from": "Docs", "to": "notes/"},
            {"from": "Free", "to": "lines.json", "format": "json-connections", "kinds": ["entity"]}]}}),
    );
    let read = read(&core);
    let files = read["files"].as_array().unwrap();
    let rows = |path: &str| {
        files
            .iter()
            .find(|f| f["path"] == path)
            .unwrap_or_else(|| panic!("{path}: {read}"))["rows"]
            .clone()
    };
    // One file of two rows; a folder's files of the one row; a file with no card to say, of its row.
    assert_eq!(rows("charts.md"), json!([1, 2]));
    let notes: Vec<&Value> = files
        .iter()
        .filter(|f| f["path"].as_str().unwrap().starts_with("notes/"))
        .collect();
    assert!(!notes.is_empty(), "{read}");
    assert!(notes.iter().all(|f| f["rows"] == json!([3])), "{read}");
    assert_eq!(rows("lines.json"), json!([4]));
    assert_eq!(
        files.iter().find(|f| f["path"] == "lines.json").unwrap()["cards"],
        json!([])
    );
}

#[test]
fn a_file_two_rows_write_differently_is_told_in_the_formats_words() {
    let core = core();
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
        json!({"target": "Data:first", "value": {"kind": "instance", "of": "Schemas:Customer"}}),
    );
    ok(
        &core,
        "export_create",
        json!({"target": "rows", "value": {"entries": [
            {"from": "Data", "to": "all.json", "format": "json-data"},
            {"from": "Docs", "to": "all.json", "format": "json-connections"}]}}),
    );
    let folder = place("rows-words");
    std::fs::create_dir_all(&folder).unwrap();
    for lang in specboard_core::text::LANGUAGES {
        let _lang = specboard_core::text::bind(Some(lang));
        let problems = ok(
            &core,
            "export_read",
            json!({"targetdir": folder.to_str().unwrap()}),
        )["problems"]
            .to_string();
        assert!(
            problems.contains("JSON Data") && problems.contains("JSON Connections"),
            "{lang}: {problems}"
        );
    }
}

#[test]
fn a_row_is_one_by_its_from_and_to() {
    let core = core();
    // A row is picked — to take away, to change, on the screen — by its from and to: one of each.
    let both = refused(
        &core,
        "export_create",
        json!({"target": "rows", "value": {"entries": [
            {"from": "Schemas", "to": "a.ts", "format": "zod"},
            {"from": "Schemas", "to": "a.ts"}]}}),
    );
    assert!(both.contains("Schemas → a.ts"), "{both}");
    ok(
        &core,
        "export_create",
        json!({"target": "rows", "value": {"entries": [
            {"from": "Schemas", "to": "a.ts", "format": "zod"},
            {"from": "Schemas", "to": "b.ts"}]}}),
    );
    let before = entries(&core);
    let no = |value: Value| {
        refused(
            &core,
            "export_update",
            json!({"target": "rows", "value": {"entries": value}}),
        )
    };
    let added = no(json!({"add": [{"from": "Schemas", "to": "a.ts"}]}));
    assert!(added.contains("Schemas → a.ts"), "{added}");
    let changed = no(json!({"change": [{"from": "Schemas", "to": "b.ts", "set": {"to": "a.ts"}}]}));
    assert!(changed.contains("Schemas → a.ts"), "{changed}");
    assert_eq!(entries(&core), before);
    // The same row added again is the row there is: nothing to say.
    change(
        &core,
        json!({"add": [{"from": "Schemas", "to": "a.ts", "format": "zod"}]}),
    );
    assert_eq!(entries(&core), before);
}

#[test]
fn a_row_whose_from_is_gone_is_picked_as_it_is_said() {
    let core = core();
    ok(
        &core,
        "card_create",
        json!({"target": "Docs:Gone", "value": {"kind": "note", "body": "消すメモ"}}),
    );
    ok(
        &core,
        "export_create",
        json!({"target": "rows", "value": {"entries": [
            {"from": "Docs:Gone", "to": "gone.md"},
            {"from": "Docs", "to": "notes/"}]}}),
    );
    ok(&core, "card_delete", json!({"target": ["Docs:Gone"]}));
    // What export_read says of the row is what picks it: there is no path left to say.
    let said = entries(&core)[0]["from"].as_str().unwrap().to_string();
    assert_ne!(said, "Docs:Gone");
    change(
        &core,
        json!({"change": [{"from": said, "to": "gone.md", "set": {"to": "old.md"}}]}),
    );
    assert_eq!(
        entries(&core),
        json!([{"from": said, "to": "old.md"}, {"from": "Docs", "to": "notes/"}])
    );
    change(&core, json!({"remove": [{"from": said, "to": "old.md"}]}));
    assert_eq!(entries(&core), json!([{"from": "Docs", "to": "notes/"}]));
}
