//! What the core writes out, kept: the exports of the fixture projects (`tests/fixtures/projects/`) —
//! every file and the problems — as the core wrote them when they were kept
//! (`tests/fixtures/kept/exports/`), and each project file written again as it was read.

use crate::common;

use serde_json::{Value, json};
use specboard_core::Core;

/// The fixtures the exports are kept for.
const PROJECTS: [&str; 4] = ["sample", "large", "datamodel", "small"];

fn result(core: &Core, request: Value) -> Value {
    let reply = common::op(core, request);
    assert!(reply.get("error").is_none(), "{reply}");
    reply["result"].clone()
}

fn project_id(name: &str) -> String {
    let file: Value = serde_json::from_str(&common::project_file(name)).unwrap();
    file["project"]["id"].as_str().unwrap().to_string()
}

#[test]
fn every_export_is_as_kept() {
    for name in PROJECTS {
        let core = Core::open(common::store_with(&[name])).unwrap();
        let got = result(
            &core,
            json!({"op": "export_files", "project": project_id(name)}),
        );
        common::kept(
            &format!("exports/{name}.json"),
            &serde_json::to_string_pretty(&got).unwrap(),
        );
    }
}

#[test]
fn a_project_file_read_is_written_again_as_it_was() {
    for name in PROJECTS {
        let core = Core::open(common::store_with(&[name])).unwrap();
        let text = result(
            &core,
            json!({"op": "project_file", "project": project_id(name)}),
        )["text"]
            .as_str()
            .unwrap()
            .to_string();
        let want: Value = serde_json::from_str(&common::project_file(name)).unwrap();
        let got: Value = serde_json::from_str(&text).unwrap();
        assert!(want == got, "{name}: 読んで書き直すと違う");
    }
}

/// A file of the name before a plan was called a project (`specboard/plan`, versions 11 to 17):
/// read as one of now, and written again as the project's.
#[test]
fn a_file_of_the_name_before_is_read_as_a_project() {
    let mut before: Value = serde_json::from_str(&common::project_file("sample")).unwrap();
    let top = before.as_object_mut().unwrap();
    top.insert("format".into(), "specboard/plan".into());
    top.insert("version".into(), 17.into());
    let mut project = top.shift_remove("project").unwrap();
    let kinds = project["kinds"].as_object_mut().unwrap();
    let tree = kinds.shift_remove("project_tree").unwrap();
    kinds.insert("plan_tree".into(), tree);
    // A writing rule of the project's own at the place of before.
    project["rules"] = json!({"plan.description": "地図を書く", "card.description": "一文で言う"});
    top.insert("plan".into(), project);
    let place = common::place("name-before.sqlite");
    let core = Core::create(&place).unwrap();
    result(
        &core,
        json!({"op": "project_file_read", "text": before.to_string()}),
    );
    let text = result(
        &core,
        json!({"op": "project_file", "project": project_id("sample")}),
    )["text"]
        .as_str()
        .unwrap()
        .to_string();
    let mut want: Value = serde_json::from_str(&common::project_file("sample")).unwrap();
    want["project"]["rules"] =
        json!({"project.description": "地図を書く", "card.description": "一文で言う"});
    let got: Value = serde_json::from_str(&text).unwrap();
    assert!(
        want == got,
        "前の名前のファイルを読んで書き直すと違う: {text}"
    );
}

/// A file of before version 19: its cards' and sections' export settings read as the project's exports,
/// which write what they wrote.
#[test]
fn a_file_of_before_has_its_settings_read_as_exports_that_write_the_same() {
    let text = std::fs::read_to_string(
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/projects/before/large.json"),
    )
    .unwrap();
    let place = common::place("before-large.sqlite");
    let core = Core::create(&place).unwrap();
    result(&core, json!({"op": "project_file_read", "text": text}));
    let got = result(
        &core,
        json!({"op": "export_files", "project": project_id("large")}),
    );
    common::kept(
        "exports/large.json",
        &serde_json::to_string_pretty(&got).unwrap(),
    );
    let written = result(
        &core,
        json!({"op": "project_file", "project": project_id("large")}),
    )["text"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(!written.contains("exportTo") && !written.contains("export_to"));
    assert!(written.contains(r#""exports": ["#));
}

/// An enum whose values say what they stand for: the code's comments say each, the JSON Schema
/// beside its list, the Markdown table in a column of its own. An enum none of whose values say
/// one is written as it was (the fixtures' answers above).
#[test]
fn an_enums_described_values_are_written_out_as_kept() {
    let core = Core::open(common::store_with(&["sample"])).unwrap();
    let mcp = |tool: &str, arguments: Value| {
        common::mcp(&core, tool, arguments).unwrap_or_else(|e| panic!("{tool}: {e}"))
    };
    mcp(
        "card_update",
        json!({"target": "Schemas:Status", "value": {"values": [
            {"name": "open", "label": "受付", "description": "注文を受けて、まだ払われていない"},
            {"name": "paid", "description": "代金を受け取った（*/ も書ける）"},
            {"name": "shipped", "label": "発送済み"}
        ]}}),
    );
    mcp(
        "card_create",
        json!({"target": "Schemas:Payment", "value": {"kind": "enum", "values": [
            {"name": "card", "label": "カード", "description": "クレジットカードで払う"},
            {"name": "cash"}
        ]}}),
    );
    mcp(
        "card_create",
        json!({"target": "Docs:Statuses", "value": {"kind": "formatdoc", "rows": [
            {"kind": "card", "name": "status", "target": "Schemas:Status"},
            {"kind": "card", "name": "payment", "target": "Schemas:Payment"}
        ]}}),
    );
    let got = result(
        &core,
        json!({"op": "export_files", "project": project_id("sample")}),
    );
    common::kept(
        "exports/sample-described.json",
        &serde_json::to_string_pretty(&got).unwrap(),
    );
}

/// An enum whose values have labels and no descriptions: the code's comments still say each label.
#[test]
fn an_enums_labelled_values_are_written_out_too() {
    let core = Core::open(common::store_with(&["sample"])).unwrap();
    common::mcp(
        &core,
        "card_update",
        json!({"target": "Schemas:Status", "value": {"values": [
            {"name": "open", "label": "受付"}, {"name": "paid"}, {"name": "shipped", "label": "発送済み"}
        ]}}),
    )
    .unwrap();
    let got = result(
        &core,
        json!({"op": "export_files", "project": project_id("sample")}),
    );
    let files = got["files"].as_object().unwrap_or_else(|| panic!("{got}"));
    let with_status: Vec<(&String, &str)> = files
        .iter()
        .filter_map(|(n, t)| Some((n, t.as_str()?)))
        .filter(|(n, t)| n.ends_with(".ts") && t.contains(" Status = "))
        .collect();
    assert!(with_status.len() >= 2, "{got}");
    for (name, text) in with_status {
        assert!(
            text.contains("- `open`（受付）\n") && text.contains("- `shipped`（発送済み）\n"),
            "{name}: {text}"
        );
        assert!(!text.contains("- `paid`"), "{name}: {text}");
    }
}
