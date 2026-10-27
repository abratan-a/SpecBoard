//! Small fixes to what MCP reads, writes and says, found moving a design in: a diagram that folds
//! only what a note can say, a refusal that names the cause, the words the guide gives.

use crate::common;

use common::{call, ok, refused, script_store};
use serde_json::{Value, json};
use specboard_core::Core;

const PROJECT: &str = "見本";

fn core() -> Core {
    Core::open(script_store()).unwrap()
}

/// Entities `names` in `Schemas`, each with an `id` and an `owner` string.
fn entities(core: &Core, names: &[&str]) {
    let mut cards = serde_json::Map::new();
    for name in names {
        cards.insert(
            format!("Schemas:{name}"),
            json!({"kind": "entity", "fields": [
                {"name": "id", "type": "string"}, {"name": "owner", "type": "string"}]}),
        );
    }
    ok(core, "card_create", json!({"value": cards}));
}

// MARK: - A diagram's notes

#[test]
fn a_card_with_many_lines_of_its_own_keeps_each_line_in_the_diagram() {
    let core = core();
    entities(&core, &["Hub", "A", "B", "C", "D"]);
    ok(
        &core,
        "kind_create",
        json!({"target": "uses", "value": {"of": "line", "label": "uses", "description": "uses",
            "base": "link", "look": {"color": "green", "dash": "solid", "begin": "none", "end": "arrow"},
            "from": [{"kinds": ["entity"], "at": "card"}],
            "to": {"kinds": ["entity"], "at": ["card"]}}}),
    );
    let lines: Vec<Value> = ["A", "B", "C", "D"]
        .iter()
        .map(|n| json!({"line": "uses", "to": format!("Schemas:{n}")}))
        .collect();
    ok(
        &core,
        "card_update",
        json!({"target": "Schemas:Hub", "value": {"connections": {"add": lines}}}),
    );
    let drawn = ok(
        &core,
        "section_read",
        json!({"target": "Schemas", "value": {"as": "diagram"}}),
    );
    let mermaid = drawn["mermaid"].as_str().unwrap();
    for to in ["A", "B", "C", "D"] {
        assert!(
            mermaid.contains(&format!("Schemas__Hub ..> Schemas__{to} : uses")),
            "{mermaid}"
        );
    }
    assert!(!mermaid.contains("note for Schemas__Hub"), "{mermaid}");
}

#[test]
fn a_field_naming_many_cards_is_one_note_that_says_the_line_and_the_cards() {
    let core = core();
    entities(&core, &["Hub", "A", "B", "C", "D"]);
    let lines: Vec<Value> = ["A", "B", "C", "D"]
        .iter()
        .map(|n| json!({"line": "points", "from": "owner", "to": format!("Schemas:{n}"), "at": "id"}))
        .collect();
    ok(
        &core,
        "card_update",
        json!({"target": "Schemas:Hub", "value": {"connections": {"add": lines}}}),
    );
    let drawn = ok(
        &core,
        "section_read",
        json!({"target": "Schemas", "value": {"as": "diagram"}}),
    );
    let mermaid = drawn["mermaid"].as_str().unwrap();
    let note = mermaid
        .lines()
        .find(|l| l.contains("note for Schemas__Hub"))
        .unwrap_or_else(|| panic!("{mermaid}"));
    assert!(
        note.contains("owner")
            && note.contains("points")
            && note.contains("Schemas:A・Schemas:B・Schemas:C・Schemas:D"),
        "{note}"
    );
    let _ = (call, refused, PROJECT);
}

// MARK: - A container refused

#[test]
fn what_goes_inside_a_refused_container_says_the_container_was_not_made() {
    let core = core();
    let said = refused(
        &core,
        "card_create",
        json!({"value": {
            "Charts/Shop:Bad": {"kind": "flow", "label": "x", "wrongkey": 1},
            "Charts/Shop:Bad.start": {"kind": "begin", "trigger": "receive"},
            "Charts/Shop:Bad.done": {"kind": "end", "outcome": "success"},
        }}),
    );
    assert!(said.starts_with("3 件の誤り"), "{said}");
    assert!(said.contains("wrongkey"), "{said}");
    for node in ["start", "done"] {
        assert!(
            said.contains(&format!(
                "入れ物 Charts/Shop:Bad が作れなかったので、中の Charts/Shop:Bad.{node} は作っていません"
            )),
            "{said}"
        );
    }
    assert!(!said.contains("type は必須"), "{said}");
}

// MARK: - The lines a row may draw

/// A row of a process node is drawn from by the kinds of line its own kind of row is written by,
/// as the write holds it to: a write has no peer, a send no reads.
#[test]
fn the_lines_a_process_row_may_draw_are_those_its_kind_of_row_writes() {
    let core = core();
    ok(
        &core,
        "card_create",
        json!({"value": {
            "Charts/Shop:P": {"kind": "flow", "label": "p"},
            "Charts/Shop:P.work": {"kind": "process", "steps": [
                {"do": "read"}, {"do": "write"}, {"do": "send", "program": "mailer"},
                {"do": "compute"}]}
        }}),
    );
    let drawn = |step: usize| -> Vec<String> {
        let read = ok(
            &core,
            "card_read",
            json!({"target": "Charts/Shop:P.work", "value": {"as": "lineKinds", "from": format!("steps[{step}]")}}),
        );
        read["lines"]
            .as_array()
            .unwrap()
            .iter()
            .map(|k| k["line"].as_str().unwrap().to_string())
            .filter(|l| ["reads", "writes", "partner", "key"].contains(&l.as_str()))
            .collect()
    };
    assert_eq!(drawn(0), ["reads"]);
    assert_eq!(drawn(1), ["writes"]);
    assert_eq!(drawn(2), ["partner", "key"]);
    assert!(drawn(3).is_empty());
    // What it says may be drawn goes through.
    assert!(mcp_write(&core, 1, "writes").is_ok());
    assert!(mcp_write(&core, 1, "partner").is_err());
}

fn mcp_write(core: &Core, step: usize, line: &str) -> Result<Value, String> {
    common::mcp(
        core,
        "card_update",
        json!({"target": "Charts/Shop:P.work", "value": {"connections": {"add": [
            {"line": line, "from": format!("steps[{step}]"), "to": "Schemas:Orders"}]}}}),
    )
}

// MARK: - A card made again

#[test]
fn making_a_card_that_is_there_points_to_card_update() {
    let core = core();
    entities(&core, &["Once"]);
    let said = refused(
        &core,
        "card_create",
        json!({"target": "Schemas:Once", "value": {"kind": "entity"}}),
    );
    assert!(
        said.contains("既にあります") && said.contains("card_update"),
        "{said}"
    );
}

// MARK: - A card's size as it is made

#[test]
fn a_card_that_can_be_sized_is_made_with_its_size() {
    let core = core();
    ok(
        &core,
        "card_create",
        json!({"target": "Schemas:Memo", "value": {"kind": "note", "body": "x",
            "topLeft": {"x": 30, "y": 30}, "size": {"w": 21, "h": 13}}}),
    );
    let read = ok(&core, "card_read", json!({"target": "Schemas:Memo"}));
    assert_eq!(read["size"], json!({"w": 21, "h": 13}), "{read}");
    // A size said wrongly is refused, and nothing is made half way.
    let said = refused(
        &core,
        "card_create",
        json!({"target": "Schemas:Fixed", "value": {"kind": "entity", "size": {"h": 13}}}),
    );
    assert!(said.contains("size の w は必須です"), "{said}");
    let there = ok(
        &core,
        "card_read",
        json!({"target": "Schemas:Fixed", "value": {"as": "exists"}}),
    );
    assert_eq!(there["unresolved"], 1, "{there}");
}

// MARK: - A list without the text

#[test]
fn a_list_can_leave_the_bodies_out() {
    let core = core();
    ok(
        &core,
        "card_create",
        json!({"target": "Docs:Long", "value": {"kind": "note", "body": "一行目\n二行目"}}),
    );
    let with = ok(
        &core,
        "card_list",
        json!({"value": {"section": "Docs", "kind": "note"}}),
    );
    assert!(
        with["cards"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["body"].is_string()),
        "{with}"
    );
    let without = ok(
        &core,
        "card_list",
        json!({"value": {"section": "Docs", "kind": "note", "body": false}}),
    );
    let cards = without["cards"].as_array().unwrap();
    assert!(!cards.is_empty(), "{without}");
    assert!(
        cards
            .iter()
            .all(|c| c.get("body").is_none() && c["rev"].is_string()),
        "{without}"
    );
    assert_eq!(without["matched"], with["matched"]);
}

// MARK: - A group of information named

#[test]
fn naming_a_group_of_information_lists_its_items_without_show() {
    let core = core();
    let all = ok(&core, "inspection_list", json!({}));
    let info_groups: Vec<(&String, u64)> = all["groups"]
        .as_object()
        .unwrap()
        .iter()
        .filter(|(_, g)| g["kind"] == "info")
        .map(|(name, g)| (name, g["count"].as_u64().unwrap()))
        .filter(|(_, count)| *count > 0)
        .collect();
    assert!(!info_groups.is_empty(), "{all}");
    for (group, count) in info_groups {
        let named = ok(&core, "inspection_list", json!({"value": {"group": group}}));
        assert_eq!(
            named["items"].as_array().unwrap().len() as u64,
            count,
            "{group}: {named}"
        );
        // Asked for as fixes, they are not there.
        let as_fix = ok(
            &core,
            "inspection_list",
            json!({"value": {"group": group, "show": "fix"}}),
        );
        assert!(as_fix["items"].as_array().unwrap().is_empty(), "{as_fix}");
    }
}

// MARK: - An export to a folder

#[test]
fn an_entry_with_a_format_to_a_folder_is_refused_for_writing_one_file() {
    let core = core();
    let set = common::op(
        &core,
        json!({"op": "mcp_writes_set", "project": common::project_id(&core), "group": "export", "state": "allow"}),
    );
    assert!(set.get("error").is_none(), "{set}");
    let said = refused(
        &core,
        "export_create",
        json!({"target": "schemas", "value": {"entries": [
            {"from": "Schemas", "to": "data/", "format": "json-schema"}]}}),
    );
    assert!(
        said.contains("json-schema") && said.contains("data/") && said.contains("ファイル"),
        "{said}"
    );
    ok(
        &core,
        "export_create",
        json!({"target": "schemas", "value": {"entries": [
            {"from": "Schemas", "to": "data/types.schema.json", "format": "json-schema"}]}}),
    );
}

// MARK: - The sequence diagram

/// Two entities of one name in two sections, each kept in a store of one name of its own section,
/// read by one flow whose rows' descriptions name cards.
fn flow_reading_two_items(core: &Core) -> String {
    ok(
        core,
        "card_create",
        json!({"value": {
            "Schemas:Item": {"kind": "entity", "fields": [{"name": "id", "type": "string"}]},
            "Schemas/Ordering:Item": {"kind": "entity", "fields": [{"name": "id", "type": "string"}]},
            "Schemas:Items": {"kind": "store", "connections": [{"line": "holds", "to": "Schemas:Item"}]},
            "Schemas/Ordering:Items": {"kind": "store",
                "connections": [{"line": "holds", "to": "Schemas/Ordering:Item"}]},
            "Charts/Shop:Scan": {"kind": "flow", "label": "scan"},
            "Charts/Shop:Scan.start": {"kind": "begin", "trigger": "manual",
                "connections": [{"line": "next", "to": "look"}]},
            "Charts/Shop:Scan.look": {"kind": "process",
                "steps": [{"do": "read", "description": "[[Schemas:Customer.name]] を引く"}, {"do": "read"},
                    {"do": "compute", "description": "[[Schemas:Customer]] を数える"}],
                "connections": [
                    {"line": "reads", "from": "steps[0]", "to": "Schemas:Item", "at": "id"},
                    {"line": "reads", "from": "steps[1]", "to": "Schemas/Ordering:Item", "at": "id"},
                    {"line": "next", "to": "done"}]},
            "Charts/Shop:Scan.done": {"kind": "end", "outcome": "success"}
        }}),
    );
    let chart = ok(
        core,
        "card_read",
        json!({"target": "Charts/Shop:Scan", "value": {"as": "diagram", "shape": "sequence"}}),
    );
    chart["mermaid"].as_str().unwrap().to_string()
}

#[test]
fn stores_of_one_name_in_two_sections_are_two_lifelines() {
    let core = core();
    let chart = flow_reading_two_items(&core);
    let participants: Vec<&str> = chart
        .lines()
        .filter(|l| l.trim_start().starts_with("participant"))
        .collect();
    assert!(
        participants
            .iter()
            .any(|p| p.ends_with("as Schemas:Items（store）"))
            && participants
                .iter()
                .any(|p| p.ends_with("as Schemas/Ordering:Items（store）")),
        "{chart}"
    );
    // What is read is no lifeline: where it is kept is.
    assert!(
        !participants
            .iter()
            .any(|p| p.ends_with("as Schemas:Item") || p.ends_with("as Schemas/Ordering:Item")),
        "{chart}"
    );
}

#[test]
fn a_row_s_description_names_its_cards_plainly_in_the_sequence() {
    let core = core();
    let chart = flow_reading_two_items(&core);
    assert!(!chart.contains("[["), "{chart}");
    // A compute row's description is its note; any other row's is not on its arrow.
    assert!(chart.contains("Customer を数える"), "{chart}");
    assert!(!chart.contains("を引く"), "{chart}");
}

// MARK: - The names a kind cannot have

#[test]
fn a_kind_named_as_a_reserved_name_is_refused_with_the_names_and_the_guide_lists_them() {
    let core = core();
    let said = refused(
        &core,
        "kind_create",
        json!({"target": "module", "value": {"of": "card", "label": "モジュール"}}),
    );
    for name in ["ghost", "action", "module", "custom_goal"] {
        assert!(said.contains(name), "{said}");
    }
    let (_, guide) = call(&core, "guide", json!({"about": "kinds"}));
    for name in ["ghost", "action", "module", "custom_goal"] {
        assert!(guide.contains(name), "{guide}");
    }
}

// MARK: - What the guide says

#[test]
fn the_guide_tells_how_a_batch_a_dispatch_and_a_wait_are_written() {
    let core = core();
    let guide = |about: &str| call(&core, "guide", json!({"about": about})).1;
    // A step may point at what an earlier step made, not a later one.
    assert!(guide("batch").contains("前の手"), "{}", guide("batch"));
    let flow = guide("flow");
    // Dispatch inside the process is a run; "until" of a wait is written in its timeout.
    assert!(flow.contains("run ノードと runs の線"), "{flow}");
    assert!(flow.contains("timeout"), "{flow}");
    let paths = guide("paths");
    assert!(paths.contains("コードの書式"), "{paths}");
}

#[test]
fn a_value_slot_inside_a_code_span_is_not_picked_up() {
    let core = core();
    let said = ok(
        &core,
        "card_create",
        json!({"target": "Docs:Slots", "value": {"kind": "note",
            "body": "`{{timezone}}` は字のまま\n```\n{{region}}\n```\n"}}),
    );
    assert!(said.get("warnings").is_none(), "{said}");
    let bare = ok(
        &core,
        "card_create",
        json!({"target": "Docs:Bare", "value": {"kind": "note", "body": "{{timezone}}"}}),
    );
    assert!(bare["warnings"].is_array(), "{bare}");
}
