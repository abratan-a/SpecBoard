//! What a user's review found where the core did not do as designed: a rule's arguments that name a
//! field follow its rename and are checked, a section's export entry says why it takes no card, a
//! wait or a run that goes on more than one way is drawn with its time running out or its failure,
//! and the guide says what `{{ }}` inserts of an enum.

use crate::common;

use common::{checked, ok, op, script_store};
use serde_json::{Value, json};
use specboard_core::Core;

fn core() -> Core {
    Core::open(script_store()).unwrap()
}

#[test]
fn a_rules_field_names_follow_the_fields_rename_and_a_name_of_no_field_is_said() {
    let core = core();
    ok(
        &core,
        "card_update",
        json!({"target": "Schemas/Ordering:Order", "value": {"rules": [
            {"kind": "requiredIf", "when": "status == paid", "then": "status"},
            {"kind": "uniqueBy", "keys": ["status", "total"]},
            {"kind": "sortedBy", "by": ["status"]},
        ]}}),
    );
    let renamed = ok(
        &core,
        "card_update",
        json!({"target": "Schemas/Ordering:Order.status", "value": {"name": "state"}}),
    );
    assert!(renamed.to_string().contains("state"), "{renamed}");
    let rules = ok(
        &core,
        "card_read",
        json!({"target": "Schemas/Ordering:Order"}),
    )["rules"]
        .clone();
    assert_eq!(rules[0]["then"], "state", "{rules}");
    assert_eq!(rules[0]["when"], "status == paid", "{rules}");
    assert_eq!(rules[1]["keys"], json!(["state", "total"]), "{rules}");
    assert_eq!(rules[2]["by"], json!(["state"]), "{rules}");
    // A name of no field of the card is a broken path of the rule.
    ok(
        &core,
        "card_update",
        json!({"target": "Schemas/Ordering:Order", "value": {"rules": [
            {"kind": "requiredIf", "when": "x", "then": "gone"},
        ]}}),
    );
    let after = checked(&core, "main");
    let broken: Vec<&Value> = after["brokenPaths"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|b| b["via"] == "rules.requiredIf.then")
        .collect();
    assert_eq!(broken.len(), 1, "{after}");
    assert!(
        broken[0]["path"].as_str().unwrap().ends_with("gone"),
        "{after}"
    );
}

/// Exports written without asking each time.
fn exports_allowed(core: &Core) {
    let said = op(
        core,
        json!({"op": "mcp_writes_set", "project": common::project_id(core), "group": "export", "state": "allow"}),
    );
    assert!(said.get("error").is_none(), "{said}");
}

#[test]
fn a_sections_entry_to_a_folder_says_its_schema_cards_go_to_one_file() {
    let core = core();
    exports_allowed(&core);
    ok(
        &core,
        "export_create",
        json!({"target": "probe", "value": {}}),
    );
    let said = ok(
        &core,
        "export_update",
        json!({"target": "probe", "value": {"entries": {"add": [{"from": "Schemas/Ordering", "to": "ordering/"}]}}}),
    );
    let warnings = said["warnings"].to_string();
    assert!(warnings.contains("1 つのファイル"), "{said}");
    assert!(!warnings.contains("flow を取らず"), "{said}");
    // The export says it too, on the entry.
    let read = ok(
        &core,
        "export_read",
        json!({"targetdir": "[default]", "target": "probe"}),
    );
    let entry = &read["exports"][0]["entries"][0];
    assert!(
        entry["unwritten"].to_string().contains("1 つのファイル"),
        "{read}"
    );
}

/// The flow `Charts/Shop:Fan`: a wait that goes on two ways that meet, its time running out to a
/// cancelling end; and a run that goes on two ways, failing to the same end.
fn fan(core: &Core) {
    let flow = "Charts/Shop:Fan";
    ok(
        core,
        "card_create",
        json!({"target": flow, "value": {"kind": "flow"}}),
    );
    let node = |name: &str, value: Value| {
        ok(
            core,
            "card_create",
            json!({"target": format!("{flow}.{name}"), "value": value}),
        );
    };
    node("start", json!({"kind": "begin", "trigger": "manual"}));
    node(
        "ok",
        json!({"kind": "wait", "for": "approval", "timeout": "1日"}),
    );
    node("pay", json!({"kind": "process"}));
    node("tell", json!({"kind": "process"}));
    node("meet", json!({"kind": "join", "join": "all"}));
    node("done", json!({"kind": "end", "outcome": "success"}));
    node("gone", json!({"kind": "end", "outcome": "cancelled"}));
    let link = |from: &str, line: &str, to: &str| {
        ok(
            core,
            "card_update",
            json!({"target": format!("{flow}.{from}"), "value": {"connections": {"add": [{"line": line, "to": to}]}}}),
        );
    };
    link("start", "next", "ok");
    link("ok", "next", "pay");
    link("ok", "next", "tell");
    link("ok", "onTimeout", "gone");
    link("pay", "next", "meet");
    link("tell", "next", "meet");
    link("meet", "next", "done");
}

#[test]
fn a_wait_that_goes_on_two_ways_is_drawn_with_its_time_running_out() {
    let core = core();
    fan(&core);
    let chart = common::text_in(
        &ok(
            &core,
            "card_read",
            json!({"target": "Charts/Shop:Fan", "value": {"as": "diagram", "shape": "sequence"}}),
        ),
        "mermaid",
    );
    let note = chart
        .find("wait approval（期限 1日）")
        .unwrap_or_else(|| panic!("wait の注記が無い:\n{chart}"));
    let late = chart
        .find("期限切れ")
        .unwrap_or_else(|| panic!("期限切れが無い:\n{chart}"));
    let par = chart
        .find("\n    par\n")
        .unwrap_or_else(|| panic!("par が無い:\n{chart}"));
    assert!(note < par && par < late, "{chart}");
    assert!(chart.contains("cancelled"), "{chart}");
}

#[test]
fn the_guide_says_an_enum_inserts_its_values_not_one() {
    let _lang = specboard_core::text::bind(Some("en"));
    let said = ok(&core(), "guide", json!({"about": "paths"}));
    let guide = said
        .as_str()
        .map_or_else(|| said.to_string(), str::to_string);
    assert!(guide.contains("{{"), "{guide}");
    assert!(!guide.contains("an enum value"), "{guide}");
}

/// A card whose rules name fields where they sit: a value field's rule names its siblings, a
/// rule on a field with fields inside names them, and a name with dots goes down through
/// objects and into what an array's elements embed.
fn named_rules(core: &Core) {
    ok(
        core,
        "card_create",
        json!({"value": {
            "Schemas/Ordering:Key": {"kind": "value", "fields": [{"name": "key_id", "type": "string"}]},
            "Schemas/Ordering:Subject": {"kind": "entity", "fields": [
                {"name": "tenant_id", "type": "string"},
                {"name": "generation", "type": "int"},
                {"name": "pin", "type": "object"},
                {"name": "pin.digest", "type": "string"},
                {"name": "pin.gen", "type": "int"},
                {"name": "creds", "type": "string", "array": true},
                {"name": "slots", "type": "object", "array": true},
                {"name": "slots.key", "type": "object"},
                {"name": "slots.key.key_id", "type": "string"}],
                "connections": [{"line": "embed", "from": "creds", "to": "Schemas/Ordering:Key"}]},
        }}),
    );
    let rules = |target: &str, rules: Value| {
        ok(
            core,
            "card_update",
            json!({"target": target, "value": {"rules": rules}}),
        );
    };
    rules(
        "Schemas/Ordering:Subject",
        json!([{"kind": "requiredIf", "when": "x", "then": ["pin.digest", "pin.gen"]}]),
    );
    rules(
        "Schemas/Ordering:Subject.generation",
        json!([{"kind": "monotonicPerKey", "keys": ["tenant_id"]}]),
    );
    rules(
        "Schemas/Ordering:Subject.pin",
        json!([{"kind": "requiredIf", "when": "x", "then": ["digest"]}]),
    );
    rules(
        "Schemas/Ordering:Subject.creds",
        json!([{"kind": "sortedBy", "by": ["key_id"]}, {"kind": "uniqueBy", "by": ["key_id"]}]),
    );
    rules(
        "Schemas/Ordering:Subject.slots",
        json!([{"kind": "sortedBy", "by": ["key.key_id"]}, {"kind": "uniqueBy", "keys": ["key.key_id"]}]),
    );
}

/// The broken paths the inspection says of a rule's names, as `(via, path)`.
fn broken_rule_names(core: &Core) -> Vec<(String, String)> {
    let after = checked(core, "main");
    after["brokenPaths"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|b| b["via"].as_str().is_some_and(|v| v.starts_with("rules.")))
        .map(|b| {
            (
                b["via"].as_str().unwrap().to_string(),
                b["path"].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

#[test]
fn a_rules_names_are_looked_for_where_the_rule_names_fields() {
    let core = core();
    named_rules(&core);
    assert_eq!(broken_rule_names(&core), vec![]);
}

#[test]
fn a_name_of_no_field_is_said_in_every_argument_that_names_fields() {
    let core = core();
    named_rules(&core);
    ok(
        &core,
        "card_update",
        json!({"target": "Schemas/Ordering:Subject.slots", "value": {"rules": [
            {"kind": "uniqueBy", "by": ["nothing"]},
            {"kind": "uniqueBy", "by": ["key.nothing"]},
            {"kind": "sortedBy", "keys": ["nothing"]},
        ]}}),
    );
    let broken = broken_rule_names(&core);
    let vias: Vec<&str> = broken.iter().map(|(via, _)| via.as_str()).collect();
    assert_eq!(
        vias,
        vec![
            "rules.uniqueBy.by",
            "rules.uniqueBy.by",
            "rules.sortedBy.keys"
        ],
        "{broken:?}"
    );
}

#[test]
fn a_nested_name_in_a_rule_follows_a_rename_on_its_way() {
    let core = core();
    named_rules(&core);
    ok(
        &core,
        "card_update",
        json!({"target": "Schemas/Ordering:Subject.slots.key.key_id", "value": {"name": "kid"}}),
    );
    ok(
        &core,
        "card_update",
        json!({"target": "Schemas/Ordering:Subject.pin", "value": {"name": "seal"}}),
    );
    let subject = ok(
        &core,
        "card_read",
        json!({"target": "Schemas/Ordering:Subject"}),
    );
    assert_eq!(
        subject["rules"][0]["then"],
        json!(["seal.digest", "seal.gen"]),
        "{subject}"
    );
    let slots = ok(
        &core,
        "card_read",
        json!({"target": "Schemas/Ordering:Subject.slots"}),
    );
    assert_eq!(slots["rules"][0]["by"], json!(["key.kid"]), "{slots}");
    assert_eq!(slots["rules"][1]["keys"], json!(["key.kid"]), "{slots}");
    assert_eq!(broken_rule_names(&core), vec![]);
}
