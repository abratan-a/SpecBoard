//! The links a description writes: a card's, a field's, a node's, a step's, a line's, a section's
//! and the project's description — followed through a rename, stored in full and said in
//! `brokenPaths` when they lead nowhere, as a body's are.

use crate::common;

use common::{checked, ok, script_store};
use serde_json::{Value, json};
use specboard_core::Core;

fn core() -> Core {
    Core::open(script_store()).unwrap()
}

/// A card's description, or a field's.
fn described(core: &Core, target: &str) -> String {
    let read = ok(core, "card_read", json!({"target": target}));
    read["description"].as_str().unwrap_or_default().to_string()
}

fn section_described(core: &Core, name: &str) -> String {
    ok(core, "section_list", json!({}))["sections"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["section"] == name)
        .and_then(|s| s["description"].as_str())
        .unwrap_or_default()
        .to_string()
}

fn signed_line(core: &Core) -> Value {
    ok(
        core,
        "card_read",
        json!({"target": "Schemas/Ordering:Order", "value": {"include": ["connections"]}}),
    )["connections"]
        .as_array()
        .unwrap()
        .iter()
        .find(|l| l["line"] == "signedBy")
        .cloned()
        .unwrap()
}

/// Every kind of description, each naming `Schemas:Customer`.
fn write_them(core: &Core) {
    ok(
        core,
        "card_update",
        json!({"target": "Schemas:Rule", "value": {"description": "[[Customer]] の決まり"}}),
    );
    ok(
        core,
        "card_update",
        json!({"target": "Schemas/Ordering:Order.customer_id", "value": {"description": "[[Schemas:Customer]] の id"}}),
    );
    ok(
        core,
        "card_update",
        json!({"target": "Schemas/Ordering:Order", "value": {"connections": {
            "remove": [{"line": "signedBy"}],
            "add": [{"line": "signedBy", "to": "Schemas:Customer", "at": "id", "description": "[[Schemas:Customer]] が署名する",
                     "fields": {"payload": "approval"}}]
        }}}),
    );
    ok(
        core,
        "card_update",
        json!({"target": "Charts/Shop:Checkout.save", "value": {
            "description": "[[Schemas:Customer]] を置く",
            "steps": [{"do": "read"}, {"do": "write", "description": "[[Schemas:Customer]] に置く"}]
        }}),
    );
    ok(
        core,
        "section_update",
        json!({"target": "Schemas", "value": {"description": "[[Customer]] を置く"}}),
    );
    ok(
        core,
        "project_update",
        json!({"value": {"description": "まず [[Schemas:Customer]] を読む"}}),
    );
}

#[test]
fn a_descriptions_links_follow_a_rename_and_are_kept_in_full() {
    let core = core();
    write_them(&core);
    // Written from inside its section, kept in full.
    assert_eq!(
        described(&core, "Schemas:Rule"),
        "[[Schemas:Customer]] の決まり"
    );
    ok(
        &core,
        "card_update",
        json!({"target": "Schemas:Customer", "value": {"name": "Client"}}),
    );
    assert_eq!(
        described(&core, "Schemas:Rule"),
        "[[Schemas:Client]] の決まり"
    );
    assert_eq!(
        described(&core, "Schemas/Ordering:Order.customer_id"),
        "[[Schemas:Client]] の id"
    );
    assert_eq!(
        signed_line(&core)["description"],
        "[[Schemas:Client]] が署名する"
    );
    let node = ok(
        &core,
        "card_read",
        json!({"target": "Charts/Shop:Checkout.save"}),
    );
    assert_eq!(node["description"], "[[Schemas:Client]] を置く", "{node}");
    assert_eq!(
        node["steps"][1]["description"], "[[Schemas:Client]] に置く",
        "{node}"
    );
    assert_eq!(
        section_described(&core, "Schemas"),
        "[[Schemas:Client]] を置く"
    );
    assert_eq!(
        ok(&core, "project_read", json!({}))["description"],
        "まず [[Schemas:Client]] を読む"
    );
    // Nothing of it is broken.
    let after = checked(&core, "main");
    let via: Vec<&str> = after["brokenPaths"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["via"].as_str().unwrap())
        .collect();
    assert!(!via.iter().any(|v| v.contains("description")), "{after}");
}

#[test]
fn a_rename_in_a_branch_leaves_the_projects_description() {
    let core = core();
    write_them(&core);
    ok(
        &core,
        "card_update",
        json!({"branch": "refunds", "target": "Schemas:Customer", "value": {"name": "Client"}}),
    );
    assert_eq!(
        ok(&core, "project_read", json!({}))["description"],
        "まず [[Schemas:Customer]] を読む"
    );
    assert_eq!(
        described(&core, "Schemas:Rule"),
        "[[Schemas:Customer]] の決まり"
    );
}

#[test]
fn a_description_that_leads_nowhere_is_said() {
    let core = core();
    let written = ok(
        &core,
        "card_update",
        json!({"target": "Schemas:Rule", "value": {"description": "[[Schemas:Nope]] と `[[Not:Read]]`"}}),
    );
    assert!(
        written["warnings"]
            .to_string()
            .contains("description のパス「Schemas:Nope」が解決できません"),
        "{written}"
    );
    ok(
        &core,
        "card_update",
        json!({"target": "Schemas/Ordering:Order.total", "value": {"description": "[[Schemas:Gone]] の合計"}}),
    );
    ok(
        &core,
        "section_update",
        json!({"target": "Docs", "value": {"description": "[[Docs:Missing]] を置く"}}),
    );
    ok(
        &core,
        "project_update",
        json!({"value": {"description": "[[Nowhere:Card]] から読む"}}),
    );
    let after = checked(&core, "main");
    let broken: Vec<(String, String, String)> = after["brokenPaths"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|b| b["via"].as_str().unwrap().contains("description"))
        .map(|b| {
            (
                b["at"].as_str().unwrap().to_string(),
                b["via"].as_str().unwrap().to_string(),
                b["path"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    let want = [
        ("Schemas:Rule", "description", "Schemas:Nope"),
        (
            "Schemas/Ordering:Order.total",
            "description",
            "Schemas:Gone",
        ),
        ("Docs", "description", "Docs:Missing"),
        ("見本", "description", "Nowhere:Card"),
    ];
    for (at, via, path) in want {
        assert!(
            broken.contains(&(at.to_string(), via.to_string(), path.to_string())),
            "{at} {via} {path}: {broken:?}"
        );
    }
    // Code is not read.
    assert!(!after.to_string().contains("Not:Read"), "{after}");
}
