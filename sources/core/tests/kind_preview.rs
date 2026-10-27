//! The window's preview of a definition being written (`kind_preview`): the draft read with the
//! project's kinds as though it were written, in `project_detail`'s shapes — the kind as `kinds`
//! says it, and a card of it as `schemas` says one (a card of the tree read with the draft, or one of
//! values made up for the kind) — and nothing written.
//!
//! Over the sample project (`見本`): a kind `risk` (a choice `severity`, a person `owner`, rows
//! `steps`) held by a section kind of the project's own `risks`, and a section `Risks` of it.

use crate::common;

use common::{kept, ok, op, project_id, script_store};
use serde_json::{Value, json};
use specboard_core::Core;

const PROJECT: &str = "見本";

fn risk() -> Value {
    json!({
        "kind": "risk", "label": "リスク", "color": "#C2410C", "header": ["severity"],
        "fields": [
            { "name": "severity", "label": "深刻さ", "fieldtype": "radio", "options": ["高", "中", "低"], "default": "中" },
            { "name": "owner", "label": "担当", "fieldtype": "person" },
            { "name": "due", "fieldtype": "date" },
            { "name": "score", "fieldtype": "number", "min": 10 },
            { "name": "areas", "fieldtype": "dropdownlist", "options": ["db", "web", "net"], "many": true },
            { "name": "spec", "fieldtype": "path" }
        ],
        "rows": [
            { "name": "steps", "label": "対策", "fields": [
                { "name": "name", "fieldtype": "text", "pattern": "identifier", "required": true },
                { "name": "done", "fieldtype": "checkbox" }
            ] }
        ]
    })
}

/// The project given `risk` and a section `Risks` that holds it.
fn defined(core: &Core) {
    common::set_kinds(
        core,
        PROJECT,
        json!({"cards": [risk()], "sections": [{ "section": "risks", "label": "リスク", "holds": ["risk", "note"] }]}),
    )
    .unwrap();
    let read = ok(core, "project_read", json!({}));
    let mut tree = read["projectTree"].clone();
    tree["main_sections"]
        .as_array_mut()
        .unwrap()
        .push(json!({"type": "risks", "name": "Risks"}));
    ok(
        core,
        "project_update",
        json!({"value": {"project_tree": tree}}),
    );
}

/// `kind_preview` asked with `asked` (and the sample project): what it answered.
fn preview(core: &Core, asked: Value) -> Value {
    let mut request = asked;
    request["op"] = "kind_preview".into();
    request["project"] = project_id(core).into();
    let answer = op(core, request);
    assert!(answer.get("error").is_none(), "{answer}");
    answer["result"].clone()
}

/// The keys of a card's entry that depend on the store it is read from, left out of what is kept.
fn kept_part(card: &Value) -> Value {
    let mut card = card.clone();
    card.as_object_mut().unwrap().shift_remove("projectId");
    card
}

#[test]
fn a_new_kind_of_card_is_drawn_with_values_made_up_by_type() {
    let core = Core::open(script_store()).unwrap();
    let said = preview(&core, json!({"of": "card", "definition": risk()}));
    // The kind whole, as the screen draws a card by it: the base fields first.
    let kind = &said["kind"];
    assert_eq!(kind["kind"], "risk", "{said}");
    let names: Vec<&str> = kind["fields"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        [
            "label",
            "description",
            "tags",
            "metadata",
            "severity",
            "owner",
            "due",
            "score",
            "areas",
            "spec"
        ]
    );
    // A card of values made up for each type: its default, a fixed value, the first options, one
    // row of each kind of row; nothing where a field points somewhere.
    let card = &said["card"];
    assert_eq!(card["made"], true);
    assert_eq!(card["kind"], "risk");
    assert_eq!(card["name"], "見本");
    let fields = &card["form"]["fields"];
    assert_eq!(fields["severity"], "中");
    assert_eq!(fields["owner"], "Claude");
    assert_eq!(fields["due"], "2026-04-01");
    assert_eq!(fields["score"], 10, "a number made up keeps to its range");
    assert_eq!(fields["areas"], json!(["db", "web"]));
    assert!(fields.get("spec").is_none(), "{card}");
    assert!(fields.get("label").is_none(), "{card}");
    let steps = card["form"]["rows"]["steps"].as_array().unwrap();
    assert_eq!(steps.len(), 1);
    assert_eq!(steps[0]["fields"]["name"], "見本の文");
    assert_eq!(steps[0]["fields"]["done"], false);
    kept(
        "reads/kind_preview_made_ja.json",
        &serde_json::to_string_pretty(&kept_part(card)).unwrap(),
    );
}

#[test]
fn the_values_made_up_are_said_in_the_language_asked_for() {
    let core = Core::open(script_store()).unwrap();
    let mut definition = risk();
    definition["fields"]
        .as_array_mut()
        .unwrap()
        .push(json!({ "name": "notes", "fieldtype": "markdown" }));
    let said = preview(
        &core,
        json!({"of": "card", "definition": definition, "lang": "en"}),
    );
    let card = &said["card"];
    assert_eq!(card["name"], "Sample");
    assert_eq!(
        card["form"]["rows"]["steps"][0]["fields"]["name"],
        "Sample text"
    );
    kept(
        "reads/kind_preview_made_en.json",
        &serde_json::to_string_pretty(&kept_part(card)).unwrap(),
    );
}

#[test]
fn a_face_drawing_markdown_gives_the_rows_it_is_drawn_in() {
    let core = Core::open(script_store()).unwrap();
    let mut definition = risk();
    definition["fields"].as_array_mut().unwrap().extend([
        json!({ "name": "done", "fieldtype": "checkbox" }),
        json!({ "name": "body", "fieldtype": "markdown" }),
    ]);
    definition["face"] = json!({ "top": ["done"], "markdown": "body" });
    let said = preview(&core, json!({"of": "card", "definition": definition}));
    let card = &said["card"];
    assert!(
        card["form"]["fields"]["body"]
            .as_str()
            .unwrap()
            .starts_with("## ")
    );
    let rows = card["faceRows"].as_i64().unwrap();
    assert!(rows > 1, "{card}");
    // Without a face the rows are the screen's to count.
    let plain = preview(&core, json!({"of": "card", "definition": risk()}));
    assert!(plain["card"].get("faceRows").is_none(), "{plain}");
}

#[test]
fn a_card_of_the_tree_is_read_with_the_draft_and_what_it_no_longer_has_is_stray() {
    let core = Core::open(script_store()).unwrap();
    defined(&core);
    ok(
        &core,
        "card_create",
        json!({"target": "Risks:Outage", "value": {"kind": "risk", "severity": "高", "owner": "Claude",
            "steps": [{"name": "backup"}]}}),
    );
    // The draft lets go of owner: the card's value of it is stray, drawn faint.
    let mut draft = risk();
    draft["fields"].as_array_mut().unwrap().remove(1);
    let said = preview(
        &core,
        json!({"of": "card", "replacing": "risk", "definition": draft, "sample": "Risks:Outage"}),
    );
    let card = &said["card"];
    assert_eq!(card["name"], "Outage", "{said}");
    assert!(card.get("made").is_none(), "{card}");
    assert_eq!(card["form"]["fields"]["severity"], "高");
    assert_eq!(card["strayFields"]["owner"], "Claude", "{card}");
    // Renamed in the draft, its cards are read as the new kind.
    let mut renamed = risk();
    renamed["kind"] = "hazard".into();
    let said = preview(
        &core,
        json!({"of": "card", "replacing": "risk", "definition": renamed, "sample": "Risks:Outage"}),
    );
    assert_eq!(said["kind"]["kind"], "hazard", "{said}");
    assert_eq!(said["card"]["kind"], "hazard", "{said}");
    assert_eq!(said["card"]["name"], "Outage");
    // A sample that is not a card of the kind gives way to values made up.
    let said = preview(
        &core,
        json!({"of": "card", "replacing": "risk", "definition": risk(), "sample": "Risks:Nowhere"}),
    );
    assert_eq!(said["card"]["made"], true, "{said}");
}

#[test]
fn a_kind_of_section_and_of_line_are_said_as_project_detail_says_them() {
    let core = Core::open(script_store()).unwrap();
    let section = json!({ "section": "keys", "label": "鍵", "holds": ["note"], "icon": { "name": "key", "color": "#FF9500" } });
    let said = preview(&core, json!({"of": "section", "definition": section}));
    assert_eq!(said["kind"], section, "{said}");
    assert!(said.get("card").is_none(), "{said}");
    let line = json!({ "line": "checks", "label": "確かめる", "base": "link",
        "from": [{ "kinds": ["*"], "at": "card" }], "to": { "kinds": ["*"], "at": ["card"] } });
    let said = preview(&core, json!({"of": "line", "definition": line}));
    assert_eq!(said["kind"], line, "{said}");
    // A built-in one, as it is built (its draft is read only).
    let said = preview(
        &core,
        json!({"of": "card", "replacing": "task", "definition": {}}),
    );
    assert_eq!(said["kind"]["kind"], "task", "{said}");
    assert_eq!(said["kind"]["builtin"], true);
    assert_eq!(said["card"]["made"], true);
    assert!(said["card"]["faceRows"].as_i64().is_some(), "{said}");
}

#[test]
fn a_draft_that_is_no_definition_reads_as_none() {
    let core = Core::open(script_store()).unwrap();
    for definition in [
        json!("{"),
        json!({"label": "名前が無い"}),
        json!({"kind": ""}),
        // A name another kind has.
        json!({"kind": "entity"}),
        json!({"kind": "note"}),
    ] {
        let said = preview(&core, json!({"of": "card", "definition": definition}));
        assert_eq!(said, json!({"kind": null}), "{definition}");
    }
    // Asked of a list there is none of, refused.
    let mut request = json!({"op": "kind_preview", "of": "table", "definition": {}});
    request["project"] = project_id(&core).into();
    assert!(op(&core, request)["error"].is_string());
}

#[test]
fn nothing_is_written_by_a_preview() {
    let core = Core::open(script_store()).unwrap();
    defined(&core);
    ok(
        &core,
        "card_create",
        json!({"target": "Risks:Outage", "value": {"kind": "risk", "severity": "高"}}),
    );
    let project = project_id(&core);
    let state = |core: &Core| {
        let detail = op(core, json!({"op": "project_detail", "project": project}));
        let commits = op(core, json!({"op": "commits", "project": project}));
        (
            detail["result"]["digest"].clone(),
            commits["result"]["commits"].clone(),
        )
    };
    let before = state(&core);
    let mut renamed = risk();
    renamed["kind"] = "hazard".into();
    preview(
        &core,
        json!({"of": "card", "replacing": "risk", "definition": renamed, "sample": "Risks:Outage"}),
    );
    preview(
        &core,
        json!({"of": "card", "definition": {"kind": "fresh"}}),
    );
    assert_eq!(state(&core), before);
    assert_eq!(
        ok(&core, "card_read", json!({"target": "Risks:Outage"}))["kind"],
        "risk"
    );
}
