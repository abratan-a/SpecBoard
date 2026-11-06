//! Answers that read a design wrongly — a path missed, a word that misleads, a shape the guide did not
//! say — kept here so they stay right.

use crate::common;

use common::{ok, script_store};
use serde_json::json;
use specboard_core::Core;

#[test]
fn deleting_a_field_reached_through_an_embedded_shape_warns_of_the_table_reading_it() {
    let core = Core::open(script_store()).unwrap();
    ok(
        &core,
        "card_create",
        json!({"value": {
            "Schemas:Money": {"kind": "value", "fields": [{"name": "amount", "type": "int"}, {"name": "currency", "type": "string"}]},
            "Schemas:Unit": {"kind": "entity", "fields": [{"name": "name", "type": "string", "primaryKey": true}, {"name": "price", "type": "string"}],
                "connections": [{"line": "embed", "from": "price", "to": "Schemas:Money"}]},
            "Docs:Prices": {"kind": "table", "of": "Schemas:Unit", "columns": [{"path": "name"}, {"path": "price.amount"}]}
        }}),
    );
    // Read once, so the broken paths are kept from here on.
    ok(&core, "inspection_list", json!({}));
    let tried = ok(
        &core,
        "batch",
        json!({"value": {"steps": [{"tool": "card_delete", "arguments": {"target": "Schemas:Money.amount"}}]}}),
    )
    .to_string();
    assert!(tried.contains("Docs:Prices"), "試す: {tried}");
    ok(
        &core,
        "card_delete",
        json!({"target": "Schemas:Money.amount"}),
    );
    let broken = common::checked(&core, "main")["brokenPaths"].to_string();
    assert!(broken.contains("Docs:Prices"), "{broken}");
}

#[test]
fn a_finished_goals_context_says_only_its_done_tasks_are_done() {
    let core = Core::open(script_store()).unwrap();
    let board = |tool: &str, args: serde_json::Value| {
        let mut args = args;
        args["branch"] = "_project_board_".into();
        ok(&core, tool, args)
    };
    board(
        "card_create",
        json!({"value": {
            "Goals:Launch": {"kind": "goal", "label": "出す"},
            "Goals:Launch.Params": {"kind": "task", "label": "形", "done": true},
            "Goals:Launch.Flow": {"kind": "task", "label": "流れ"}
        }}),
    );
    board(
        "card_action",
        json!({"target": "Goals:Launch", "op": "finish"}),
    );
    let read = board(
        "card_read",
        json!({"target": "Goals/Complete:Launch", "value": {"as": "context"}}),
    );
    assert_eq!(
        read["doneTasks"],
        json!(["Goals/Complete:Launch.Params"]),
        "{read}"
    );
    let work = read["work"].to_string();
    assert!(work.contains("Goals/Complete:Launch.Flow"), "{work}");
}

#[test]
fn a_whiteboard_reads_as_text_with_its_arrows_and_its_sketches_words() {
    let core = Core::open(script_store()).unwrap();
    ok(
        &core,
        "section_create",
        json!({"target": "Free/Explain", "value": {"type": "whiteboard"}}),
    );
    ok(
        &core,
        "card_create",
        json!({"value": {
            "Free/Explain:App": {"kind": "sketch", "text": "**アプリ**\n画面だけ",
                "connections": [{"line": "arrow", "to": "Free/Explain:Agent", "fields": {"label": "SSH の上で attach"}}]},
            "Free/Explain:Agent": {"kind": "sketch", "shape": "cylinder", "text": "エージェント"}
        }}),
    );
    let read = ok(
        &core,
        "section_read",
        json!({"target": "Free/Explain", "value": {"as": "diagram"}}),
    );
    let mermaid = read["mermaid"].as_str().unwrap_or_default().to_string();
    assert!(mermaid.contains("SSH の上で attach"), "{read}");
    assert!(
        mermaid.contains("アプリ") && mermaid.contains("エージェント"),
        "{mermaid}"
    );
    // The arrows alone, by their kind.
    let only = ok(
        &core,
        "section_read",
        json!({"target": "Free/Explain", "value": {"as": "diagram", "kinds": ["arrow"]}}),
    );
    assert!(only.to_string().contains("SSH の上で attach"), "{only}");
}

#[test]
fn a_kinds_guide_says_the_keys_only_it_takes() {
    let core = Core::open(script_store()).unwrap();
    let spec = ok(&core, "guide", json!({"about": "spec"})).to_string();
    for key in ["iWant", "soThat", "scenarios"] {
        assert!(spec.contains(key), "{key}: {spec}");
    }
    let image = ok(&core, "guide", json!({"about": "image"})).to_string();
    for key in ["imageData", "imageFrom", "alt"] {
        assert!(image.contains(key), "{key}: {image}");
    }
}

#[test]
fn the_paths_guide_says_where_brackets_cannot_be_left_out() {
    let core = Core::open(script_store()).unwrap();
    let paths = ok(&core, "guide", json!({"about": "paths"})).to_string();
    assert!(paths.contains("表の列"), "{paths}");
}

#[test]
fn a_wrong_project_tree_is_said_as_the_tree_and_a_tree_left_out_is_kept() {
    let core = Core::open(script_store()).unwrap();
    let said = common::refused(
        &core,
        "project_update",
        json!({"value": {"project_tree": {"main_sections": [{"type": "nosuch", "name": "X"}]}}}),
    );
    assert!(said.contains("project_tree"), "{said}");
    assert!(!said.contains("種類の定義"), "{said}");
    assert!(
        !said.contains("board_sections"),
        "書いていない木を求めない: {said}"
    );
    // main's alone changed: the board's stays as it is.
    let tree = ok(&core, "project_read", json!({}))["projectTree"].clone();
    let mut main = tree["main_sections"].as_array().unwrap().clone();
    main.push(json!({"type": "free", "name": "Scratch"}));
    ok(
        &core,
        "project_update",
        json!({"value": {"project_tree": {"main_sections": main}}}),
    );
    let after = ok(&core, "project_read", json!({}))["projectTree"].clone();
    assert_eq!(after["board_sections"], tree["board_sections"]);
}

#[test]
fn a_second_tag_of_an_exclusive_group_is_warned_of_as_it_is_written() {
    let core = Core::open(script_store()).unwrap();
    ok(
        &core,
        "tag_update",
        json!({"target": "group:role", "value": {"add": ["manager", "agent"], "exclusive": true}}),
    );
    let listed = ok(&core, "tag_list", json!({}));
    assert!(
        listed["groups"]
            .as_array()
            .unwrap()
            .iter()
            .any(|g| g["name"] == "role" && g["exclusive"] == true),
        "{listed}"
    );
    ok(
        &core,
        "card_create",
        json!({"target": "Schemas:Host", "value": {"kind": "entity", "tags": ["manager"]}}),
    );
    let said = ok(
        &core,
        "card_update",
        json!({"target": "Schemas:Host", "value": {"tags": {"add": ["agent"]}}}),
    );
    let warned = said["warnings"].to_string();
    assert!(
        warned.contains("role") && warned.contains("Schemas:Host"),
        "{said}"
    );
}

#[test]
fn impact_says_every_path_a_change_would_break() {
    let core = Core::open(script_store()).unwrap();
    ok(
        &core,
        "card_create",
        json!({"value": {
            "Schemas:Money": {"kind": "value", "fields": [{"name": "amount", "type": "int"}]},
            "Schemas:Unit": {"kind": "entity", "fields": [{"name": "name", "type": "string", "primaryKey": true}, {"name": "price", "type": "string"}],
                "connections": [{"line": "embed", "from": "price", "to": "Schemas:Money"}]},
            "Schemas:Job": {"kind": "entity", "fields": [{"name": "unit", "type": "string"}],
                "connections": [{"line": "points", "from": "unit", "to": "Schemas:Unit", "at": "name"}]},
            "Docs:Prices": {"kind": "table", "of": "Schemas:Unit", "columns": [{"path": "name"}, {"path": "price.amount"}]}
        }}),
    );
    let name = ok(
        &core,
        "card_read",
        json!({"target": "Schemas:Unit.name", "value": {"as": "impact"}}),
    );
    let said = name["paths"].to_string();
    assert!(
        said.contains("Schemas:Job") && said.contains("Docs:Prices"),
        "{name}"
    );
    let amount = ok(
        &core,
        "card_read",
        json!({"target": "Schemas:Money", "value": {"as": "impact"}}),
    );
    assert!(
        amount["paths"].to_string().contains("Docs:Prices"),
        "{amount}"
    );
}

#[test]
fn a_value_put_in_a_text_reads_as_its_value_in_markdown() {
    let core = Core::open(script_store()).unwrap();
    ok(
        &core,
        "card_create",
        json!({"value": {
            "Schemas:Limit": {"kind": "value", "fields": [{"name": "most", "type": "int", "const": 4096}]},
            "Docs:Notes": {"kind": "freedoc", "body": "1 回に送れるのは {{Schemas:Limit.most}} 件まで"}
        }}),
    );
    let read = ok(
        &core,
        "card_read",
        json!({"target": "Docs:Notes", "value": {"as": "markdown"}}),
    );
    let text = read["markdown"].as_str().unwrap_or_default().to_string();
    assert!(text.contains("4096"), "{read}");
    assert!(text.contains("Schemas:Limit.most"), "パスも残す: {text}");
}

#[test]
fn a_list_with_nothing_included_is_each_cards_path_kind_and_label() {
    let core = Core::open(script_store()).unwrap();
    let whole = ok(&core, "card_list", json!({"value": {"limit": 200}})).to_string();
    let lean = ok(
        &core,
        "card_list",
        json!({"value": {"limit": 200, "include": []}}),
    );
    let first = &lean["cards"][0];
    assert!(
        first["path"].is_string() && first["kind"].is_string(),
        "{first}"
    );
    assert!(
        first.get("fields").is_none() && first.get("body").is_none(),
        "{first}"
    );
    assert!(
        lean.to_string().len() * 3 < whole.len(),
        "{} / {}",
        lean.to_string().len(),
        whole.len()
    );
}

#[test]
fn a_merge_conflict_is_said_short_and_taken_by_source_or_into() {
    let core = Core::open(script_store()).unwrap();
    let base: String = (1..=12).map(|i| format!("{i} 行目の文\n")).collect();
    ok(
        &core,
        "card_create",
        json!({"target": "Docs:Plan", "value": {"kind": "freedoc", "body": base}}),
    );
    ok(&core, "branch_create", json!({"target": "work"}));
    ok(
        &core,
        "card_update",
        json!({"target": "Docs:Plan", "value": {"body": base.replace("3 行目の文", "3 行目を main で直した")}}),
    );
    ok(
        &core,
        "card_update",
        json!({"branch": "work", "target": "Docs:Plan", "value": {"body": base.replace("3 行目の文", "3 行目を branch で直した")}}),
    );
    let said = common::refused(
        &core,
        "branch_merge",
        json!({"target": "main", "value": {"into": "work"}}),
    );
    assert!(said.contains("into"), "{said}");
    assert!(
        said.matches("12 行目の文").count() <= 1,
        "元を一度だけ: {said}"
    );
    let wrong = common::refused(
        &core,
        "branch_merge",
        json!({"target": "main", "value": {"into": "work", "take": {"Docs:Plan": {"body": "target"}}}}),
    );
    assert!(wrong.contains("into"), "{wrong}");
    ok(
        &core,
        "branch_merge",
        json!({"target": "main", "value": {"into": "work", "take": {"Docs:Plan": {"body": "into"}}}}),
    );
    let kept = ok(
        &core,
        "card_read",
        json!({"branch": "work", "target": "Docs:Plan"}),
    );
    assert!(
        kept["body"].as_str().unwrap().contains("branch で直した"),
        "into は merge 先: {kept}"
    );
}

#[test]
fn a_dry_run_says_a_long_list_of_changes_in_short() {
    let core = Core::open(script_store()).unwrap();
    let fields: Vec<serde_json::Value> = (0..30)
        .map(|i| json!({"name": format!("f{i}"), "type": "string"}))
        .collect();
    let tried = ok(
        &core,
        "batch",
        json!({"value": {"steps": [{"tool": "card_create", "arguments": {"target": "Schemas:Wide", "value": {"kind": "entity", "fields": fields}}}]}}),
    );
    let changes = tried["steps"][0]["changes"].as_array().unwrap().clone();
    assert!(changes.len() <= 6, "{changes:?}");
    assert!(
        changes.last().unwrap().as_str().unwrap().contains("26"),
        "{changes:?}"
    );
}

#[test]
fn the_windows_merge_says_which_goals_still_name_the_branch_merged() {
    let core = Core::open(script_store()).unwrap();
    let project = common::project_id(&core);
    ok(
        &core,
        "card_create",
        json!({"branch": "_project_board_", "target": "Goals:Scope", "value": {"kind": "goal", "label": "範囲"}}),
    );
    ok(
        &core,
        "card_action",
        json!({"branch": "_project_board_", "target": "Goals:Scope", "op": "start", "value": {"branch": "scope"}}),
    );
    ok(
        &core,
        "card_create",
        json!({"branch": "scope", "target": "Docs:Scope", "value": {"kind": "note", "body": "範囲"}}),
    );
    let branches = common::op(&core, json!({"op": "branches", "project": project}));
    let id = branches["result"]["branches"]
        .as_array()
        .and_then(|b| b.iter().find(|b| b["name"] == "scope"))
        .and_then(|b| b["id"].as_str())
        .unwrap_or_else(|| panic!("{branches}"))
        .to_string();
    let merged = common::op(
        &core,
        json!({"op": "merge", "project": project, "source": id, "target": ""}),
    );
    assert!(merged.get("error").is_none(), "{merged}");
    assert_eq!(
        merged["result"]["stillNamed"],
        json!(["Goals:Scope"]),
        "{merged}"
    );
}

#[test]
fn a_field_two_embeddings_up_is_still_followed_past_a_card_that_also_names_it() {
    let core = Core::open(script_store()).unwrap();
    ok(
        &core,
        "card_create",
        json!({"value": {
            "Schemas:Money": {"kind": "value", "fields": [{"name": "amount", "type": "int"}]},
            "Schemas:Unit": {"kind": "value", "fields": [{"name": "price", "type": "string"}],
                "connections": [{"line": "embed", "from": "price", "to": "Schemas:Money"}]},
            "Schemas:Order": {"kind": "entity", "fields": [{"name": "id", "type": "string", "primaryKey": true}, {"name": "unit", "type": "string"}],
                "description": "金額は [[Schemas:Money]] で持つ",
                "connections": [{"line": "embed", "from": "unit", "to": "Schemas:Unit"}]},
            "Docs:Orders": {"kind": "table", "of": "Schemas:Order", "columns": [{"path": "id"}, {"path": "unit.price.amount"}]}
        }}),
    );
    ok(&core, "inspection_list", json!({}));
    ok(
        &core,
        "card_delete",
        json!({"target": "Schemas:Money.amount"}),
    );
    let broken = common::checked(&core, "main")["brokenPaths"].to_string();
    assert!(broken.contains("Docs:Orders"), "{broken}");
}

#[test]
fn impact_names_what_a_node_inside_a_flow_breaks() {
    let core = Core::open(script_store()).unwrap();
    ok(
        &core,
        "card_create",
        json!({"value": {
            "Charts:Pay": {"kind": "flow"},
            "Charts:Pay.start": {"kind": "begin", "trigger": "manual", "connections": [{"line": "next", "to": "check"}]},
            "Charts:Pay.check": {"kind": "process", "steps": [{"do": "read"}], "connections": [{"line": "next", "to": "done"}]},
            "Charts:Pay.done": {"kind": "end", "outcome": "success"},
            "Docs:PayNotes": {"kind": "note", "body": "確かめるのは [[Charts:Pay.check]]"}
        }}),
    );
    let read = ok(
        &core,
        "card_read",
        json!({"target": "Charts:Pay.check", "value": {"as": "impact"}}),
    );
    assert!(
        read["paths"].to_string().contains("Docs:PayNotes"),
        "{read}"
    );
}

#[test]
fn a_card_made_with_two_tags_of_an_exclusive_group_is_warned_of() {
    let core = Core::open(script_store()).unwrap();
    ok(
        &core,
        "tag_update",
        json!({"target": "group:role", "value": {"add": ["admin", "guest"], "exclusive": true}}),
    );
    let made = ok(
        &core,
        "card_create",
        json!({"target": "Schemas:Staff", "value": {"kind": "entity", "tags": ["admin", "guest"]}}),
    );
    assert!(made["warnings"].to_string().contains("role"), "{made}");
}
