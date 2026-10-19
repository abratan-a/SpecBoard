//! What is directly under each root, the project's: `project_update`'s `project_tree` — main's and
//! every branch's sections (`main_sections`) and the board's (`board_sections`), each a kind and a
//! name, in order. Written, the sections there are compared by name: kept and put in order, made,
//! renamed with their paths, taken away when empty everywhere or moved with what they hold into
//! `_evacuation_`. Nothing is made, moved, put in order or deleted there by the section tools; a card
//! of a kind the project has no section for is not made; the lists of kinds show the built-in ones the
//! project has a place for.

use crate::common;

use common::{ok, op, refused, script_store};
use serde_json::{Value, json};
use specboard_core::Core;

const PROJECT: &str = "見本";

/// The project's `project_tree`, as `project_read` says it (`projectTree`).
fn tree(core: &Core) -> Value {
    ok(core, "project_read", json!({}))["projectTree"].clone()
}

/// `project_tree` with main's sections as given (`[kind, name]`), the board's as they are.
fn with_main(core: &Core, main: &[(&str, &str)]) -> Value {
    let mut out = tree(core);
    out["main_sections"] = main
        .iter()
        .map(|(kind, name)| json!({"type": kind, "name": name}))
        .collect();
    out
}

/// What a tree has directly under its root, in order, and their kinds.
fn tops(core: &Core, branch: Option<&str>) -> Vec<(String, String)> {
    let mut arguments = json!({});
    if let Some(branch) = branch {
        arguments["branch"] = branch.into();
    }
    ok(core, "section_list", arguments)["sections"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|s| s["main"] != true)
        .filter_map(|s| {
            let path = s["section"].as_str()?;
            (!path.contains('/')).then(|| {
                (
                    path.to_string(),
                    s["type"].as_str().unwrap_or_default().to_string(),
                )
            })
        })
        .collect()
}

fn names(tops: &[(String, String)]) -> Vec<&str> {
    tops.iter().map(|(n, _)| n.as_str()).collect()
}

#[test]
fn what_is_under_the_root_is_kept_put_in_order_made_and_taken_away() {
    let core = Core::open(script_store()).unwrap();
    // A store of before has its roots as they were, written into the project; its working
    // directories after them.
    let read = tree(&core);
    let main: Vec<(&str, &str)> = read["main_sections"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| (t["type"].as_str().unwrap(), t["name"].as_str().unwrap()))
        .collect();
    assert_eq!(
        main,
        [
            ("document", "Docs"),
            ("schema", "Schemas"),
            ("chart", "Charts"),
            ("free", "Free"),
            ("workdir", "ts"),
            ("workdir", "zod"),
            ("workdir", "schema"),
            ("workdir", "charts_2"),
            ("workdir", "docs_2")
        ]
    );
    assert_eq!(
        read["board_sections"],
        json!([{"type": "goals", "name": "Goals"}])
    );
    assert_eq!(
        names(&tops(&core, None)),
        [
            "Docs", "Schemas", "Charts", "Free", "ts", "zod", "schema", "charts_2", "docs_2"
        ]
    );

    // In another order, and a section more of a kind written twice.
    let said = ok(
        &core,
        "project_update",
        json!({"value": {"project_tree": with_main(&core, &[
            ("free", "Free"), ("document", "Docs"), ("schema", "Schemas"), ("chart", "Charts"),
            ("document", "Guide")
        ])}}),
    );
    assert_eq!(said["projectTree"][0]["create"], json!(["Guide"]), "{said}");
    assert_eq!(
        tops(&core, None),
        [
            ("Free".to_string(), "free".to_string()),
            ("Docs".to_string(), "document".to_string()),
            ("Schemas".to_string(), "schema".to_string()),
            ("Charts".to_string(), "chart".to_string()),
            ("Guide".to_string(), "document".to_string()),
        ]
    );

    // Charts (which holds a flow) and Guide (empty) taken out, checked first: nothing written.
    let without = with_main(
        &core,
        &[
            ("free", "Free"),
            ("document", "Docs"),
            ("schema", "Schemas"),
        ],
    );
    let checked = ok(
        &core,
        "project_update",
        json!({"value": {"project_tree": without.clone(), "dryRun": true}}),
    );
    assert_eq!(checked["ok"], true, "{checked}");
    assert_eq!(
        checked["projectTree"][0],
        json!({
            "tree": "main", "keep": ["Free", "Docs", "Schemas"], "create": [],
            "evacuate": [{"section": "Charts", "to": "_evacuation_/Charts"}],
            "delete": ["Guide"], "rename": []
        })
    );
    assert_eq!(tops(&core, None).len(), 5, "check は書かない");

    // Written: Guide gone, Charts moved with what it holds into `_evacuation_`, of the kind free.
    ok(
        &core,
        "project_update",
        json!({"value": {"project_tree": without}}),
    );
    assert_eq!(
        names(&tops(&core, None)),
        ["Free", "Docs", "Schemas", "_evacuation_"]
    );
    let listed = ok(&core, "section_list", json!({}))["sections"].clone();
    let kind_of = |path: &str| {
        listed
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["section"] == path)
            .map(|s| s["type"].clone())
    };
    assert_eq!(kind_of("_evacuation_"), Some(json!("free")));
    assert_eq!(kind_of("_evacuation_/Charts"), Some(json!("free")));
    assert_eq!(kind_of("_evacuation_/Charts/Shop"), Some(json!("free")));
    // What it holds, read and written where it went.
    let flow = ok(
        &core,
        "card_read",
        json!({"target": "_evacuation_/Charts/Shop:Checkout"}),
    );
    assert_eq!(flow["kind"], "flow");
    // Nothing is made, moved or put in order there by the section tools.
    let said = refused(&core, "section_create", json!({"target": "Elsewhere"}));
    assert!(
        said.contains("プロジェクトの project_tree で決まり"),
        "{said}"
    );
    let said = refused(
        &core,
        "section_update",
        json!({"target": "Docs", "value": {"position": 0}}),
    );
    assert!(said.contains("project_tree で決まります"), "{said}");
    let said = refused(&core, "section_delete", json!({"target": "Docs"}));
    assert!(said.contains("外すのは project_tree で"), "{said}");
    let said = refused(&core, "section_create", json!({"target": "Docs/_drafts_"}));
    assert!(said.contains("アプリが使います"), "{said}");
    // `_evacuation_` emptied is deleted as any section.
    ok(
        &core,
        "section_update",
        json!({"target": "_evacuation_/Charts", "value": {"parent": "Free"}}),
    );
    ok(&core, "section_delete", json!({"target": "_evacuation_"}));
    assert_eq!(names(&tops(&core, None)), ["Free", "Docs", "Schemas"]);
}

#[test]
fn a_section_under_the_root_renamed_takes_its_paths_along_in_every_tree() {
    let core = Core::open(script_store()).unwrap();
    ok(
        &core,
        "card_create",
        json!({"target": "Free:Pointer", "value": {"kind": "note", "body": "[[Docs:Spec]] を見る"}}),
    );
    // A branch keeps a copy of Docs of its own (its description).
    ok(&core, "branch_create", json!({"target": "work"}));
    ok(
        &core,
        "section_update",
        json!({"branch": "work", "target": "Docs", "value": {"description": "branch の説明"}}),
    );

    // Renamed by `renames`, with the name in `project_tree`.
    let renamed = with_main(
        &core,
        &[
            ("document", "Guide"),
            ("schema", "Schemas"),
            ("chart", "Charts"),
            ("free", "Free"),
        ],
    );
    let said = ok(
        &core,
        "project_update",
        json!({"value": {"project_tree": renamed, "renames": [{"top_section": "Docs", "to": "Guide"}]}}),
    );
    assert_eq!(
        said["projectTree"][0]["rename"],
        json!([{"from": "Docs", "to": "Guide"}]),
        "{said}"
    );
    assert_eq!(
        ok(&core, "card_read", json!({"target": "Guide:Spec"}))["kind"],
        "formatdoc"
    );
    let memo = ok(&core, "card_read", json!({"target": "Free:Pointer"}));
    assert_eq!(memo["body"], "[[Guide:Spec]] を見る", "{memo}");
    // The branch's copy too, its description its own still.
    let theirs = tops(&core, Some("work"));
    assert_eq!(theirs[0].0, "Guide", "{theirs:?}");
    let doc = ok(
        &core,
        "section_read",
        json!({"branch": "work", "target": "Guide"}),
    );
    assert_eq!(doc["description"], "branch の説明", "{doc}");

    // By its name as any section is: `project_tree` with it.
    ok(
        &core,
        "section_update",
        json!({"target": "Guide", "value": {"name": "Docs"}}),
    );
    assert_eq!(tree(&core)["main_sections"][0]["name"], "Docs");
    let memo = ok(&core, "card_read", json!({"target": "Free:Pointer"}));
    assert_eq!(memo["body"], "[[Docs:Spec]] を見る", "{memo}");
    // A rename that names no section there, or keeps the old name, is refused.
    let said = refused(
        &core,
        "project_update",
        json!({"value": {"project_tree": tree(&core), "renames": [{"top_section": "Docs", "to": "Specs"}]}}),
    );
    assert!(
        said.contains("project_tree に Specs がありません"),
        "{said}"
    );
}

#[test]
fn what_cannot_be_under_a_root_is_refused() {
    let core = Core::open(script_store()).unwrap();
    let wrong = |main: &[(&str, &str)]| -> String {
        let tree = with_main(&core, main);
        refused(
            &core,
            "project_update",
            json!({"value": {"project_tree": tree}}),
        )
    };
    let said = wrong(&[
        ("schema", "Docs"),
        ("schema", "Schemas"),
        ("chart", "Charts"),
        ("free", "Free"),
    ]);
    assert!(
        said.contains("種類（type）だけを変えることはできません"),
        "{said}"
    );
    let said = wrong(&[("document", "Docs"), ("goals", "Projects")]);
    assert!(
        said.contains("main・branch の直下に置けない種類です"),
        "{said}"
    );
    let said = wrong(&[("document", "Docs"), ("free", "_drafts_")]);
    assert!(said.contains("前後を _ で挟んだ名前"), "{said}");
    let said = wrong(&[("document", "Docs"), ("free", "Goals")]);
    assert!(said.contains("project_tree にもうあります"), "{said}");
    let said = wrong(&[]);
    assert!(said.contains("main_sections が空です"), "{said}");
    // A card the app alone draws is never made: not among the kinds MCP takes, nor the window's.
    let said = refused(
        &core,
        "card_create",
        json!({"target": "Free:Ghostly", "value": {"kind": "section"}}),
    );
    assert!(said.contains("渡されたのは section"), "{said}");
    let said = op(
        &core,
        json!({"op": "tool", "name": "card_create", "arguments": {"project": PROJECT, "target": "Free:Ghostly", "value": {"kind": "section"}}}),
    );
    assert!(
        said["error"]
            .as_str()
            .is_some_and(|e| e.contains("渡されたのは section")),
        "{said}"
    );
    // Not made.
    let said = refused(&core, "card_read", json!({"target": "Free:Ghostly"}));
    assert!(said.contains("Ghostly"), "{said}");
}

#[test]
fn under_main_s_lock_its_roots_are_the_window_s_and_the_board_s_are_anyone_s() {
    let core = Core::open(script_store()).unwrap();
    let project: String = rusqlite::Connection::open(core.place())
        .unwrap()
        .query_row("SELECT id FROM projects WHERE name = ?", [PROJECT], |r| {
            r.get(0)
        })
        .unwrap();
    let locked = op(
        &core,
        json!({"op": "main_lock", "project": project, "on": true}),
    );
    assert!(locked.get("error").is_none(), "{locked}");
    let more = with_main(
        &core,
        &[
            ("document", "Docs"),
            ("schema", "Schemas"),
            ("chart", "Charts"),
            ("free", "Free"),
            ("free", "Drafts"),
        ],
    );
    let said = refused(
        &core,
        "project_update",
        json!({"value": {"project_tree": more.clone()}}),
    );
    // MCP's is an item of its own, asked each time by default: said so when it does not ask.
    assert!(
        said.contains("project_update.main_tree") && said.contains("request: true"),
        "{said}"
    );
    // The board's are not main's.
    let mut board = tree(&core);
    board["board_sections"]
        .as_array_mut()
        .unwrap()
        .push(json!({"type": "free", "name": "Notes"}));
    ok(
        &core,
        "project_update",
        json!({"value": {"project_tree": board}}),
    );
    assert_eq!(
        names(&tops(&core, Some("_project_board_"))),
        ["Goals", "Notes"]
    );
    // The window's goes through.
    let mut more = more;
    more["board_sections"] = tree(&core)["board_sections"].clone();
    let said = op(
        &core,
        json!({"op": "tool", "name": "project_update", "arguments": {"project": PROJECT, "value": {"project_tree": more}}}),
    );
    assert!(said.get("error").is_none(), "{said}");
    assert_eq!(
        names(&tops(&core, None)),
        ["Docs", "Schemas", "Charts", "Free", "Drafts"]
    );
}

#[test]
fn a_card_of_a_kind_the_project_has_no_section_for_is_not_made_nor_listed() {
    let core = Core::open(script_store()).unwrap();
    // No chart and no free: nowhere for a flow.
    ok(
        &core,
        "project_update",
        json!({"value": {"project_tree": with_main(&core, &[("document", "Docs"), ("schema", "Schemas")])}}),
    );
    let said = refused(
        &core,
        "card_create",
        json!({"target": "Docs:Run", "value": {"kind": "flow"}}),
    );
    assert!(
        said.contains("このプロジェクトには flow を置けるセクションがありません"),
        "{said}"
    );
    // The built-in kinds listed: those with a place, and the one the app draws.
    let kinds = ok(&core, "kind_read", json!({}))["cards"].clone();
    let listed: Vec<&str> = kinds
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|k| k["kind"].as_str())
        .collect();
    assert!(
        listed.contains(&"entity") && listed.contains(&"note"),
        "{listed:?}"
    );
    assert!(listed.contains(&"section"), "{listed:?}");
    assert!(
        !listed.contains(&"flow") && !listed.contains(&"begin"),
        "{listed:?}"
    );
    // A kind of the project's own with nowhere to go is listed, marked.
    common::set_kinds(
        &core,
        PROJECT,
        json!({"cards": [{"kind": "risk", "fields": [{"name": "sev", "fieldtype": "text"}]}]}),
    )
    .unwrap();
    let own = ok(&core, "kind_read", json!({}))["cards"]
        .as_array()
        .unwrap()
        .iter()
        .find(|k| k["kind"] == "risk")
        .cloned()
        .unwrap();
    assert_eq!(own["noPlace"], true, "{own}");
}
