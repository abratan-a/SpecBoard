//! What MCP answers is JSON whose keys a client can rely on: a read names the form its target is
//! written in and gives the keys that form writes, in the shapes it writes them (so what was read
//! goes back as it is); text — Markdown, a diagram, a file — comes under a key of its own with what
//! was said of it beside it; a list is the same list with or without a search and goes on by a
//! string; a refusal is an object naming the topic to read and, where the fix is mechanical, the
//! call that goes through.

use crate::common;

use common::{call, matches, ok, refused, script_store};
use serde_json::{Value, json};
use specboard_core::Core;

fn core() -> Core {
    Core::open(script_store()).unwrap()
}

/// A call through MCP that must be refused: its answer's `error`, whole.
fn refusal(core: &Core, tool: &str, mut arguments: Value) -> Value {
    arguments["project"] = common::PROJECT.into();
    let (failed, text) = call(core, tool, arguments.clone());
    assert!(failed, "{tool} {arguments} が通った: {text}");
    let said: Value = serde_json::from_str(&text).unwrap();
    said["error"].clone()
}

fn keys(answer: &Value) -> Vec<String> {
    answer.as_object().unwrap().keys().cloned().collect()
}

#[test]
fn a_read_names_the_form_its_target_is_written_in() {
    let core = core();
    let read = ok(&core, "project_read", json!({}));
    let mut tree = read["projectTree"].clone();
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
        json!({"target": "Data:Order1", "value": {"kind": "instance", "of": "Schemas/Ordering:Order",
            "data": {"id": "7d1e2f3a-4b5c-4d6e-8f90-a1b2c3d4e5f6", "total": "880", "status": "paid",
                "lines": [{"sku": "C-3", "qty": 1}], "kind": "order"}}}),
    );
    ok(
        &core,
        "card_create",
        json!({"target": "Docs:Log", "value": {"kind": "stackdoc", "versions": [
            {"name": "v1", "key": "1.0", "body": "最初"}]}}),
    );
    for (path, form) in [
        ("Schemas:Customer", "card"),
        ("Charts/Shop:Checkout.save", "card"),
        ("Schemas:Customer.name", "field"),
        ("Schemas:Status.paid", "enumValue"),
        ("Data:Order1.total", "datum"),
        ("Docs:Spec.intro", "row"),
        ("Docs:Log.v1", "row"),
        ("Charts/Shop:Checkout.save.steps[1]", "row"),
    ] {
        let read = ok(&core, "card_read", json!({"target": path}));
        assert_eq!(read["form"], form, "{path}: {read}");
        // Said beside the path, before the rev a write gives back.
        let at = keys(&read);
        let path_at = at.iter().position(|k| k == "path").unwrap();
        assert_eq!(at[path_at + 1], "form", "{read}");
        assert_eq!(at[path_at + 2], "rev", "{read}");
    }
    // Read in part, it still says it.
    let part = ok(
        &core,
        "card_read",
        json!({"target": "Docs:Overview", "value": {"lines": {"from": 1, "to": 1}}}),
    );
    assert_eq!(part["form"], "card", "{part}");
}

#[test]
fn a_field_reads_as_card_update_writes_it_and_goes_back_as_read() {
    let core = core();
    let read = ok(
        &core,
        "card_read",
        json!({"target": "Schemas:Customer.name"}),
    );
    assert_eq!(read["path"], "Schemas:Customer.name");
    assert_eq!(read["name"], "name");
    assert!(read["type"].is_string(), "{read}");
    assert!(read.get("parent").is_none(), "{read}");
    assert!(read.get("fields").is_none(), "{read}");
    // Every key it says beside the tree and the path is one card_update takes for a field; the rev
    // holds the write to what was read.
    let mut back = read.as_object().unwrap().clone();
    for said in ["project", "branch", "path", "form"] {
        back.shift_remove(said);
    }
    back.insert("description".into(), "書き戻した".into());
    ok(
        &core,
        "card_update",
        json!({"target": "Schemas:Customer.name", "value": back}),
    );
    let again = ok(
        &core,
        "card_read",
        json!({"target": "Schemas:Customer.name"}),
    );
    assert_eq!(again["description"], "書き戻した");
    assert_eq!(again["type"], read["type"]);
    // An object field: its own keys, and its fields as card_create writes them, named from it.
    ok(
        &core,
        "card_create",
        json!({"target": "Schemas:Parcel", "value": {"kind": "entity", "fields": [
            {"name": "size", "type": "object", "label": "大きさ"},
            {"name": "size.w", "type": "int"},
            {"name": "size.depth", "type": "object"},
            {"name": "size.depth.cm", "type": "int"}
        ]}}),
    );
    let size = ok(&core, "card_read", json!({"target": "Schemas:Parcel.size"}));
    assert_eq!(size["form"], "field");
    assert_eq!(size["name"], "size");
    assert_eq!(size["type"], "object");
    assert_eq!(size["label"], "大きさ");
    let inner: Vec<&str> = size["fields"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["name"].as_str().unwrap())
        .collect();
    assert_eq!(inner, ["w", "depth", "depth.cm"], "{size}");
}

#[test]
fn an_entitys_fields_come_flat_to_every_nested_one_and_make_the_same_card() {
    let core = core();
    ok(
        &core,
        "card_create",
        json!({"target": "Schemas:Product", "value": {"kind": "entity", "fields": [
            {"name": "id", "type": "uuid", "primaryKey": true},
            {"name": "dimensions", "type": "object", "label": "寸法"},
            {"name": "dimensions.width", "type": "float"},
            {"name": "dimensions.depth", "type": "object"},
            {"name": "dimensions.depth.cm", "type": "int"},
            {"name": "'e.mail'", "type": "email"},
            {"name": "home address", "type": "string"}
        ]}}),
    );
    let read = ok(&core, "card_read", json!({"target": "Schemas:Product"}));
    let fields = read["fields"].as_array().unwrap().clone();
    let names: Vec<&str> = fields.iter().map(|f| f["name"].as_str().unwrap()).collect();
    assert_eq!(
        names,
        [
            "id",
            "dimensions",
            "dimensions.width",
            "dimensions.depth",
            "dimensions.depth.cm",
            "'e.mail'",
            "home address"
        ]
    );
    for field in &fields {
        assert!(field.get("fieldCount").is_none(), "{field}");
        assert!(field.get("path").is_none(), "{field}");
    }
    assert!(read.get("truncated").is_none(), "{read}");
    // The list read makes the same card again.
    ok(
        &core,
        "card_create",
        json!({"target": "Schemas:Copy", "value": {"kind": "entity", "fields": fields}}),
    );
    let copy = ok(&core, "card_read", json!({"target": "Schemas:Copy"}));
    assert_eq!(copy["fields"], read["fields"]);
}

#[test]
fn fields_past_the_room_of_an_answer_are_cut_and_said_so() {
    let core = core();
    let long = "長い説明。".repeat(100);
    let fields: Vec<Value> = (0..120)
        .map(|i| json!({"name": format!("f{i:03}"), "type": "string", "description": long}))
        .collect();
    ok(
        &core,
        "card_create",
        json!({"target": "Schemas:Wide", "value": {"kind": "entity", "fields": fields}}),
    );
    let read = ok(&core, "card_read", json!({"target": "Schemas:Wide"}));
    assert_eq!(read["truncated"], true, "{}", keys(&read).join(","));
    let given = read["fields"].as_array().unwrap();
    assert!(!given.is_empty() && given.len() < 120, "{}", given.len());
    let size: usize = given.iter().map(|f| f.to_string().chars().count()).sum();
    assert!(size <= 40_000, "{size}");
}

#[test]
fn a_formatdocs_rows_carry_their_names_and_an_item_is_read_by_its_path() {
    let core = core();
    let read = ok(&core, "card_read", json!({"target": "Docs:Spec"}));
    for row in read["rows"].as_array().unwrap() {
        let name = row["name"].as_str().unwrap();
        assert_eq!(row["path"], format!("Docs:Spec.{name}"), "{row}");
    }
    let item = ok(&core, "card_read", json!({"target": "Docs:Spec.tables"}));
    assert_eq!(item["form"], "row");
    assert_eq!(item["name"], "tables");
}

#[test]
fn text_comes_under_a_key_of_its_own_with_what_is_said_of_it_beside() {
    let core = core();
    let markdown = ok(
        &core,
        "card_read",
        json!({"target": "Docs:Spec", "value": {"as": "markdown"}}),
    );
    assert_eq!(
        keys(&markdown),
        ["project", "branch", "path", "rev", "lineCount", "markdown"]
    );
    assert!(
        markdown["markdown"]
            .as_str()
            .unwrap()
            .starts_with("# 仕様\n")
    );
    let diagram = ok(
        &core,
        "card_read",
        json!({"target": "Charts/Shop:Checkout", "value": {"as": "diagram"}}),
    );
    assert_eq!(
        keys(&diagram),
        ["project", "branch", "flow", "shape", "rev", "mermaid"]
    );
    assert!(
        diagram["mermaid"]
            .as_str()
            .unwrap()
            .starts_with("sequenceDiagram\n"),
        "{diagram}"
    );
    for (tool, target) in [("section_read", "Schemas"), ("branch_read", "main")] {
        let drawn = ok(
            &core,
            tool,
            json!({"target": target, "value": {"as": "diagram"}}),
        );
        assert!(drawn["rev"].is_string(), "{drawn}");
        assert_eq!(drawn["depth"], 1, "{drawn}");
        assert!(drawn["kinds"].is_array(), "{drawn}");
        assert!(
            drawn["mermaid"]
                .as_str()
                .unwrap()
                .starts_with("classDiagram\n"),
            "{drawn}"
        );
        // Its rev alone: the same answer without the mermaid.
        let only = ok(
            &core,
            tool,
            json!({"target": target, "value": {"as": "diagram", "revisionOnly": true}}),
        );
        assert_eq!(only["rev"], drawn["rev"]);
        assert!(only.get("mermaid").is_none(), "{only}");
    }
    let files = ok(
        &core,
        "export_read",
        json!({"targetdir": "[default]", "target": "ts", "value": {"files": ["types.ts"]}}),
    );
    assert_eq!(files["export"], "ts");
    assert_eq!(files["files"][0]["path"], "types.ts");
    assert!(
        files["files"][0]["text"]
            .as_str()
            .unwrap()
            .contains("export interface"),
        "{files}"
    );
}

#[test]
fn a_card_list_is_the_same_list_with_or_without_a_search() {
    let core = core();
    let listed = ok(&core, "card_list", json!({"value": {"section": "Schemas"}}));
    let found = ok(&core, "card_list", json!({"value": {"query": "total"}}));
    assert_eq!(keys(&listed), keys(&found));
    assert_eq!(keys(&found), ["project", "branch", "matched", "cards"]);
    let cards = found["cards"].as_array().unwrap();
    assert_eq!(found["matched"], cards.len());
    // A card once, with each place found in it, in the order they were found.
    let order = cards
        .iter()
        .find(|c| c["path"] == "Schemas/Ordering:Order")
        .unwrap();
    assert_eq!(order["kind"], "entity");
    assert!(order["rev"].is_string(), "{order}");
    let at: Vec<&Value> = order["matches"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| &m["at"])
        .collect();
    assert!(
        at.contains(&&json!("Schemas/Ordering:Order.total")),
        "{order}"
    );
    for m in matches(&found) {
        assert!(m["where"].is_string(), "{m}");
    }
}

#[test]
fn every_list_goes_on_by_a_string() {
    let core = core();
    let page = ok(&core, "card_list", json!({"value": {"limit": 1}}));
    assert!(page["next"].is_string(), "{page}");
    let found = ok(
        &core,
        "card_list",
        json!({"value": {"query": "id", "limit": 1}}),
    );
    assert!(found["next"].is_string(), "{found}");
    let more = ok(
        &core,
        "card_list",
        json!({"value": {"query": "id", "limit": 1, "after": found["next"]}}),
    );
    assert_ne!(more["cards"][0]["path"], found["cards"][0]["path"]);
    for _ in 0..2 {
        ok(
            &core,
            "card_update",
            json!({"target": "Schemas:Customer", "value": {"description": "もう一度"}}),
        );
    }
    let history = ok(
        &core,
        "branch_read",
        json!({"target": "main", "value": {"as": "history", "limit": 1}}),
    );
    let next = history["next"].as_str().unwrap();
    let on = ok(
        &core,
        "branch_read",
        json!({"target": "main", "value": {"as": "history", "limit": 1, "after": next}}),
    );
    assert!(
        on["commits"][0]["seq"].as_i64().unwrap() < history["commits"][0]["seq"].as_i64().unwrap()
    );
}

#[test]
fn as_lines_is_refused_with_its_new_name_and_the_call_that_goes_through() {
    let core = core();
    let asked = json!({"target": "Schemas/Ordering:Order.customer_id",
        "value": {"as": "lines", "to": "Schemas:Customer"}});
    let error = refusal(&core, "card_read", asked.clone());
    assert!(
        error["message"].as_str().unwrap().contains("lineKinds"),
        "{error}"
    );
    let corrected = &error["use"];
    assert_eq!(corrected["tool"], "card_read");
    let mut sent = asked.clone();
    sent["project"] = common::PROJECT.into();
    sent["value"]["as"] = "lineKinds".into();
    assert_eq!(corrected["arguments"], sent);
    // Sent as it is, it goes through.
    let (failed, text) = call(&core, "card_read", corrected["arguments"].clone());
    assert!(!failed, "{text}");
}

#[test]
fn a_refusal_is_an_object_naming_the_topic_to_read() {
    let core = core();
    let error = refusal(
        &core,
        "card_list",
        json!({"value": {"section": "Schemas"}, "nope": true}),
    );
    assert!(
        error["message"].as_str().unwrap().contains("nope"),
        "{error}"
    );
    assert_eq!(error["guide"], "card_list");
    assert!(error.get("use").is_none(), "{error}");
    // One the core says alone: its words, under message.
    let error = refusal(&core, "card_read", json!({"target": "Nowhere:Gone"}));
    assert!(error["message"].is_string(), "{error}");
    assert_eq!(keys(&error), ["message"]);
    // The window reads the words alone.
    let said = refused(&core, "card_read", json!({"target": "Nowhere:Gone"}));
    assert!(!said.contains('\u{1e}'), "{said}");
}

#[test]
fn guide_names_the_topics_close_to_a_name_and_the_key_a_value_belongs_to() {
    let core = core();
    let error = refusal(&core, "guide", json!({"about": "impact"}));
    let said = error["message"].as_str().unwrap();
    assert!(said.contains("card_read の value.as"), "{said}");
    let error = refusal(&core, "guide", json!({"about": "card_reed"}));
    let said = error["message"].as_str().unwrap();
    assert!(said.contains("card_read"), "{said}");
    let error = refusal(&core, "guide", json!({"about": ["entity", "conections"]}));
    let said = error["message"].as_str().unwrap();
    assert!(said.contains("connections"), "{said}");
}

#[test]
fn the_working_directorys_words_say_what_the_tools_do() {
    let list = specboard_core::mcp::listed();
    let tool = |name: &str| list.iter().find(|t| t["name"] == name).cloned();
    // A working directory is a section now: its tools of before are gone, and its section is set
    // and read again by the section's tools.
    for gone in [
        "directory_create",
        "directory_read",
        "directory_update",
        "directory_delete",
    ] {
        assert!(tool(gone).is_none(), "{gone}");
    }
    assert!(tool("section_action").is_some());
    let read_type = specboard_core::mcp::catalog()["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "section_update")
        .unwrap()["forms"][0]["spec"]["properties"]["readType"]["description"]
        .clone();
    for word in ["none", "basic", "git"] {
        assert!(read_type.as_str().unwrap().contains(word), "{read_type}");
    }
}

#[test]
fn places_on_a_target_without_a_goal_map_names_the_box_it_has_and_the_call_that_goes_through() {
    let core = core();
    // A card has its own box (`at`); a field has none, so `places` is simply left out.
    for (target, to) in [
        ("Schemas/Ordering:Order", json!(["fields", "at"])),
        ("Schemas/Ordering:Order.customer_id", json!(["fields"])),
    ] {
        let asked = json!({"target": target, "value": {"include": ["fields", "places"]}});
        let error = refusal(&core, "card_read", asked.clone());
        let message = error["message"].as_str().unwrap();
        assert!(message.contains("section_read"), "{error}");
        if to.as_array().unwrap().len() == 2 {
            assert!(message.contains("include: [\"at\"]"), "{error}");
        }
        let corrected = &error["use"];
        assert_eq!(corrected["tool"], "card_read", "{error}");
        assert_eq!(corrected["arguments"]["value"]["include"], to, "{error}");
        let (failed, text) = call(&core, "card_read", corrected["arguments"].clone());
        assert!(!failed, "{text}");
    }
}

#[test]
fn as_impact_on_an_enum_value_says_what_it_is_and_where_its_uses_are_found() {
    let core = core();
    let error = refusal(
        &core,
        "card_read",
        json!({"target": "Schemas:Status.shipped", "value": {"as": "impact"}}),
    );
    let message = error["message"].as_str().unwrap();
    assert!(!message.contains("見つかりません"), "{error}");
    assert!(
        message.contains("as: impact が取るのは") && message.contains("列挙の値"),
        "{error}"
    );
    assert!(message.contains("card_list"), "{error}");
    let corrected = &error["use"];
    assert_eq!(corrected["tool"], "card_list", "{error}");
    assert_eq!(corrected["arguments"]["value"]["query"], "shipped");
    let (failed, text) = call(&core, "card_list", corrected["arguments"].clone());
    assert!(!failed, "{text}");
    // Other modes say the same of the form, without the hint.
    let error = refusal(
        &core,
        "card_read",
        json!({"target": "Schemas:Status.shipped", "value": {"as": "markdown"}}),
    );
    let message = error["message"].as_str().unwrap();
    assert!(
        message.contains("as: markdown が取るのは") && message.contains("列挙の値"),
        "{error}"
    );
    assert!(error.get("use").is_none(), "{error}");
}
