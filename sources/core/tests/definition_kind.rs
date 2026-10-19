//! The built-in kind of section `definition` (`specboard://SpecBoard/Docs/Design:CardKinds`
//! 「データ（instance）と定義のセクション」): a definition promised to the outside is kept as its
//! shapes beside their data, in a section a new project has directly under main's root; a project
//! file of before, whose sections of the kind `instances` it replaced, reads them as definition
//! sections, and a project's own kind of its name is let go of or kept by a name of its own.

use crate::common;

use serde_json::{Value, json};
use specboard_core::Core;

/// A tool called through MCP on `project`: what it answered, or why it was refused.
fn on(core: &Core, project: &str, tool: &str, mut arguments: Value) -> Result<Value, String> {
    arguments["project"] = project.into();
    let (refused, text) = common::call(core, tool, arguments);
    let said: Value = serde_json::from_str(&text).unwrap_or_else(|_| text.clone().into());
    if refused {
        return Err(said["error"].as_str().map_or(text, str::to_string));
    }
    Ok(said)
}

fn ok(core: &Core, project: &str, tool: &str, arguments: Value) -> Value {
    let shown = arguments.to_string();
    on(core, project, tool, arguments).unwrap_or_else(|e| panic!("{tool} {shown}: {e}"))
}

/// The sections of a tree of `project` with their kinds, by their paths.
fn sections(core: &Core, project: &str, branch: &str) -> Vec<(String, String)> {
    ok(core, project, "section_list", json!({"branch": branch}))["sections"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| {
            (
                s["section"].as_str().unwrap().to_string(),
                s["type"].as_str().unwrap_or_default().to_string(),
            )
        })
        .collect()
}

fn pair(path: &str, kind: &str) -> (String, String) {
    (path.to_string(), kind.to_string())
}

/// The project's own kinds of section, as `kind_list` gives them.
fn own_sections(core: &Core, project: &str) -> Vec<Value> {
    ok(core, project, "kind_list", json!({}))["sections"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|s| s["own"] == true)
        .cloned()
        .collect()
}

#[test]
fn a_new_project_keeps_shapes_and_their_data_in_its_definition_section() {
    let core = Core::create(common::place("definition-new.sqlite")).unwrap();
    let (refused, said) = common::call(&core, "project_create", json!({"project": "新しい"}));
    assert!(!refused, "{said}");
    let read = ok(&core, "新しい", "project_read", json!({}));
    assert_eq!(
        read["projectTree"]["main_sections"],
        json!([
            {"type": "document", "name": "Docs"}, {"type": "schema", "name": "Schemas"},
            {"type": "definition", "name": "Definitions"}, {"type": "chart", "name": "Charts"},
            {"type": "free", "name": "Free"}
        ])
    );
    assert_eq!(
        sections(&core, "新しい", "main"),
        [
            pair("Main", ""),
            pair("Docs", "document"),
            pair("Schemas", "schema"),
            pair("Definitions", "definition"),
            pair("Charts", "chart"),
            pair("Free", "free")
        ]
    );
    // A shape, its data and a table of them side by side.
    for (target, value) in [
        ("Definitions:Order", json!({"kind": "entity"})),
        ("Definitions:Line", json!({"kind": "value"})),
        ("Definitions:Status", json!({"kind": "enum"})),
        ("Definitions:Orders", json!({"kind": "store"})),
        (
            "Definitions:first",
            json!({"kind": "instance", "of": "Definitions:Order", "data": {}}),
        ),
        (
            "Definitions:all",
            json!({"kind": "table", "of": "Definitions:Order"}),
        ),
        ("Definitions:Memo", json!({"kind": "note", "body": "例"})),
    ] {
        ok(
            &core,
            "新しい",
            "card_create",
            json!({"target": target, "value": value}),
        );
    }
    let flow = on(
        &core,
        "新しい",
        "card_create",
        json!({"target": "Definitions:Save", "value": {"kind": "flow"}}),
    )
    .unwrap_err();
    assert!(flow.contains("flow は置けません"), "{flow}");
    // A section made inside it is of its kind.
    ok(
        &core,
        "新しい",
        "section_create",
        json!({"target": "Definitions/Api", "value": {}}),
    );
    assert!(sections(&core, "新しい", "main").contains(&pair("Definitions/Api", "definition")));
}

/// A project's file as it is written now, made a file of version 22: what it says of the kind
/// `definition` said of `instances`, and `kinds` written over what it has.
fn file_of_before(core: &Core, project: &str, own: Option<Value>) -> Value {
    let id = project_id(core, project);
    let text = common::op(core, json!({"op": "project_file", "project": id}))["result"]["text"]
        .as_str()
        .unwrap()
        .to_string();
    let mut file: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(file["version"], 28);
    file["version"] = 22.into();
    match own {
        // A project's own kind of the name: its sections are of it.
        Some(own) => {
            file["project"]["kinds"]["sections"] = json!([own]);
        }
        None => said_before(&mut file),
    }
    file
}

/// Every section's kind and the project tree's `type` that say `definition`, said by the name of
/// before.
fn said_before(value: &mut Value) {
    match value {
        Value::Object(o) => {
            for key in ["kind", "type"] {
                if o.get(key).is_some_and(|k| k == "definition") {
                    o.insert(key.into(), "instances".into());
                }
            }
            for inner in o.values_mut() {
                said_before(inner);
            }
        }
        Value::Array(list) => list.iter_mut().for_each(said_before),
        _ => {}
    }
}

/// A project file read in as a copy named `name`, written by the key the tests call with.
fn read_copy(core: &Core, file: &Value, name: &str) {
    let read = common::op(
        core,
        json!({"op": "project_file_read", "text": file.to_string(), "landing": "copy", "name": name}),
    );
    assert!(read.get("error").is_none(), "{read}");
    let key = common::op(core, json!({"op": "access_keys"}))["result"]["keys"]
        .as_array()
        .unwrap()
        .iter()
        .find(|k| k["name"] == common::KEY)
        .map(|k| k["id"].clone())
        .unwrap();
    let given = common::op(
        core,
        json!({"op": "access_grant", "key": key, "project": read["result"]["project"], "level": "write"}),
    );
    assert!(given.get("error").is_none(), "{given}");
}

fn project_id(core: &Core, name: &str) -> String {
    rusqlite::Connection::open(core.place())
        .unwrap()
        .query_row("SELECT id FROM projects WHERE name = ?", [name], |r| {
            r.get(0)
        })
        .unwrap()
}

#[test]
fn a_project_file_of_before_reads_its_instances_sections_as_definition_sections() {
    let core = Core::create(common::place("definition-file.sqlite")).unwrap();
    let (refused, said) = common::call(&core, "project_create", json!({"project": "書いた"}));
    assert!(!refused, "{said}");
    ok(
        &core,
        "書いた",
        "card_create",
        json!({"target": "Schemas:Order", "value": {"kind": "entity"}}),
    );
    ok(
        &core,
        "書いた",
        "card_create",
        json!({"target": "Definitions:first", "value": {"kind": "instance", "of": "Schemas:Order", "data": {}}}),
    );
    ok(
        &core,
        "書いた",
        "section_create",
        json!({"target": "Definitions/Inner", "value": {}}),
    );
    ok(&core, "書いた", "branch_create", json!({"target": "work"}));
    ok(
        &core,
        "書いた",
        "section_update",
        json!({"branch": "work", "target": "Definitions", "value": {"description": "branch の説明"}}),
    );
    let file = file_of_before(&core, "書いた", None);
    let text = file.to_string();
    assert!(!text.contains("\"definition\""), "{text}");
    assert!(text.contains("\"instances\""), "{text}");

    read_copy(&core, &file, "前の版");
    let definitions = [
        pair("Definitions", "definition"),
        pair("Definitions/Inner", "definition"),
    ];
    for branch in ["main", "work"] {
        let found = sections(&core, "前の版", branch);
        for wanted in &definitions {
            assert!(found.contains(wanted), "{branch}: {found:?}");
        }
        assert!(!found.iter().any(|(_, k)| k == "instances"), "{found:?}");
    }
    let read = ok(&core, "前の版", "project_read", json!({}));
    assert_eq!(
        read["projectTree"]["main_sections"][2],
        json!({"type": "definition", "name": "Definitions"})
    );
    assert_eq!(
        ok(
            &core,
            "前の版",
            "card_read",
            json!({"target": "Definitions:first"})
        )["of"],
        "Schemas:Order"
    );
    // What it holds now besides: the shapes.
    ok(
        &core,
        "前の版",
        "card_create",
        json!({"target": "Definitions:Line", "value": {"kind": "value"}}),
    );
    // Written again, it is of the version of now and says the kind of now.
    let again = common::op(
        &core,
        json!({"op": "project_file", "project": project_id(&core, "前の版")}),
    )["result"]["text"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(again.contains("\"version\": 28"), "{again}");
    assert!(!again.contains("\"instances\""));
}

#[test]
fn a_project_files_own_definition_kind_is_let_go_of_or_kept_by_a_name_of_its_own() {
    let core = Core::create(common::place("definition-own.sqlite")).unwrap();
    let (refused, said) = common::call(&core, "project_create", json!({"project": "自前"}));
    assert!(!refused, "{said}");
    // Holding nothing the built-in one does not: let go of, its sections the built-in one's.
    let same = file_of_before(
        &core,
        "自前",
        Some(json!({"section": "definition", "label": "定義",
            "holds": ["entity", "value", "enum", "store", "instance", "note"], "sections": ["definition"]})),
    );
    read_copy(&core, &same, "同じ");
    assert!(own_sections(&core, "同じ").is_empty());
    assert!(sections(&core, "同じ", "main").contains(&pair("Definitions", "definition")));
    // Holding more: kept as `definition_project`, its sections and its place in the tree with it.
    let wide = file_of_before(
        &core,
        "自前",
        Some(
            json!({"section": "definition", "holds": ["entity", "flow"], "sections": ["definition"]}),
        ),
    );
    read_copy(&core, &wide, "広い");
    let own = own_sections(&core, "広い");
    assert_eq!(own.len(), 1, "{own:?}");
    assert_eq!(own[0]["section"], "definition_project");
    assert!(sections(&core, "広い", "main").contains(&pair("Definitions", "definition_project")));
    let read = ok(&core, "広い", "project_read", json!({}));
    assert_eq!(
        read["projectTree"]["main_sections"][2],
        json!({"type": "definition_project", "name": "Definitions"})
    );
    ok(
        &core,
        "広い",
        "card_create",
        json!({"target": "Definitions:Steps", "value": {"kind": "flow"}}),
    );
    // The kinds write again: no two kinds of one name.
    for project in ["同じ", "広い"] {
        let tree = ok(&core, project, "project_read", json!({}))["projectTree"].clone();
        ok(
            &core,
            project,
            "project_update",
            json!({"value": {"project_tree": tree}}),
        );
    }
}
