//! A project moved whole from one store to another: everything it has, its ids and history as they
//! were; what is the store's own stays.

use crate::common;

use common::op;
use serde_json::{Value, json};
use specboard_core::Core;

fn ok(core: &Core, request: Value) -> Value {
    let answer = op(core, request.clone());
    assert!(answer.get("error").is_none(), "{request}: {answer}");
    answer["result"].clone()
}

fn tool(core: &Core, name: &str, arguments: Value) -> Value {
    ok(
        core,
        json!({"op": "tool", "name": name, "arguments": arguments}),
    )
}

fn refused(core: &Core, request: Value) -> String {
    op(core, request.clone())["error"]
        .as_str()
        .unwrap_or_else(|| panic!("{request} が通った"))
        .to_string()
}

#[test]
fn a_project_moves_whole_and_what_is_the_stores_stays() {
    let here = Core::create(common::place("bundle-here.sqlite")).unwrap();
    let there = Core::create(common::place("bundle-there.sqlite")).unwrap();
    tool(&here, "project_create", json!({"project": "動く"}));
    let mut tree =
        tool(&here, "project_read", json!({"project": "動く"}))["json"]["projectTree"].clone();
    tree["main_sections"]
        .as_array_mut()
        .unwrap()
        .push(json!({"type": "workdir", "name": "repo"}));
    tool(
        &here,
        "project_update",
        json!({"project": "動く", "value": {"project_tree": tree}}),
    );
    let project = ok(&here, json!({"op": "projects"}))["projects"][0]["id"].clone();
    tool(
        &here,
        "card_create",
        json!({"project": "動く", "target": "Schemas:Order", "value": {"label": "注文"}}),
    );
    tool(
        &here,
        "branch_create",
        json!({"project": "動く", "target": "work"}),
    );
    tool(
        &here,
        "card_update",
        json!({"project": "動く", "branch": "work", "target": "Schemas:Order", "value": {"label": "受注"}}),
    );
    tool(
        &here,
        "card_create",
        json!({"project": "動く", "branch": "_project_board_", "target": "Goals:Ship", "value": {"kind": "goal"}}),
    );
    // The store's own: a folder bound on this machine, a key given the project, public.
    tool(
        &here,
        "section_update",
        json!({"project": "動く", "target": "repo", "value": {"folder": "/tmp"}}),
    );
    let key = ok(&here, json!({"op": "access_key_create", "name": "AI"}))["key"]["id"].clone();
    ok(
        &here,
        json!({"op": "access_grant", "key": key, "project": project, "level": "write"}),
    );
    ok(
        &here,
        json!({"op": "project_public_set", "project": project, "on": true}),
    );
    let commits = |core: &Core, branch: &str| {
        ok(
            core,
            json!({"op": "commits", "project": project, "branch": branch}),
        )["commits"]
            .as_array()
            .unwrap()
            .len()
    };
    let work = ok(&here, json!({"op": "branches", "project": project}))["branches"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let history = (commits(&here, ""), commits(&here, &work));

    let bundle = ok(&here, json!({"op": "project_bundle", "project": project}));
    let read = ok(
        &there,
        json!({"op": "project_bundle_read", "bundle": bundle}),
    );
    assert_eq!(read["project"], project);

    // The same project, ids and history as they were: main, the branch, the board.
    let listed = ok(&there, json!({"op": "projects"}))["projects"].clone();
    assert_eq!(listed[0]["id"], project);
    assert_eq!(listed[0]["name"], "動く");
    assert_eq!(
        ok(&there, json!({"op": "branches", "project": project}))["branches"][0]["id"],
        work.as_str()
    );
    assert_eq!((commits(&there, ""), commits(&there, &work)), history);
    let order = |core: &Core, branch: &str| {
        tool(
            core,
            "card_read",
            json!({"project": "動く", "branch": branch, "target": "Schemas:Order"}),
        )["json"]["label"]
            .clone()
    };
    assert_eq!(order(&there, "main"), "注文");
    assert_eq!(order(&there, "work"), "受注");
    assert_eq!(
        tool(
            &there,
            "card_read",
            json!({"project": "動く", "branch": "_project_board_", "target": "Goals:Ship"})
        )["json"]["kind"],
        "goal"
    );
    // The working directory goes, its folder on the machine does not; nor the key, nor public.
    let dirs = ok(&there, json!({"op": "workdir_detail", "project": project}))["dirs"].clone();
    assert_eq!(dirs[0]["name"], "repo", "{dirs}");
    assert!(dirs[0]["folder"].is_null(), "{dirs}");
    assert_eq!(ok(&there, json!({"op": "access_keys"}))["keys"], json!([]));
    assert_eq!(
        ok(&there, json!({"op": "project_public"}))["projects"],
        json!([])
    );

    // Twice: the project is there already.
    assert!(
        refused(
            &there,
            json!({"op": "project_bundle_read", "bundle": bundle})
        )
        .contains("もうあります")
    );
    // A store of another form: refused, nothing written.
    let mut other = bundle.clone();
    other["form"] = json!(["core_0001_kinds"]);
    let fresh = Core::create(common::place("bundle-fresh.sqlite")).unwrap();
    assert!(
        refused(
            &fresh,
            json!({"op": "project_bundle_read", "bundle": other})
        )
        .contains("置き場の形が違います")
    );
    // Another project by its slug there: refused.
    tool(&fresh, "project_create", json!({"project": "動く"}));
    assert!(
        refused(
            &fresh,
            json!({"op": "project_bundle_read", "bundle": bundle})
        )
        .contains("slug「動く」")
    );
}

#[test]
fn an_archived_project_moves_archived() {
    let here = Core::create(common::place("bundle-archived.sqlite")).unwrap();
    let there = Core::create(common::place("bundle-archived-there.sqlite")).unwrap();
    tool(&here, "project_create", json!({"project": "しまった"}));
    tool(
        &here,
        "card_create",
        json!({"project": "しまった", "target": "Schemas:Order"}),
    );
    let project = ok(&here, json!({"op": "projects"}))["projects"][0]["id"].clone();
    ok(&here, json!({"op": "project_archive", "project": project}));
    let bundle = ok(&here, json!({"op": "project_bundle", "project": project}));
    ok(
        &there,
        json!({"op": "project_bundle_read", "bundle": bundle}),
    );
    let listed = ok(&there, json!({"op": "projects"}))["projects"][0].clone();
    assert!(listed["archivedAt"].is_string(), "{listed}");
    let said = op(
        &there,
        json!({"op": "tool", "name": "card_update", "arguments": {
        "project": "しまった", "target": "Schemas:Order", "value": {"label": "書けない"}}}),
    );
    assert!(
        said["error"]
            .as_str()
            .is_some_and(|e| e.contains("アーカイブです（読むだけ）")),
        "{said}"
    );
}
