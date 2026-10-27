//! An enum's values one at a time, by their paths (`Schemas:Status.shipped`): read as
//! `{path, rev, name, label}`, added at the end or at a place, renamed or relabelled, and let go of.
//! A value is written alone so that writing one does not rewrite the others' labels; a rename
//! takes along the data that holds the value and the paths that name it, since the old name would
//! no longer be one of the enum's; a value let go of leaves the data as it is and says where it is,
//! for the person to decide.

use crate::common;

use common::{checked, ok, refused, script_store};
use serde_json::{Value, json};
use specboard_core::Core;

/// The sample project with a data card's section `Data`, and two orders: one shipped, one paid.
fn with_orders() -> Core {
    let core = Core::open(script_store()).unwrap();
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
    for (name, id, status) in [
        (
            "Order2041",
            "7d1e2f3a-4b5c-4d6e-8f90-a1b2c3d4e5f6",
            "shipped",
        ),
        ("Order2042", "0c9d8e7f-6a5b-4c3d-9e2f-1a0b9c8d7e6f", "paid"),
    ] {
        ok(
            &core,
            "card_create",
            json!({"target": format!("Data:{name}"), "value": {"kind": "instance",
                "of": "Schemas/Ordering:Order",
                "data": {"id": id, "total": "880", "status": status,
                    "lines": [{"sku": "C-3", "qty": 1}], "kind": "order"}}}),
        );
    }
    core
}

/// The enum's values as its card reads them.
fn values(core: &Core) -> Value {
    ok(
        core,
        "card_read",
        json!({"target": "Schemas:Status", "value": {"include": ["fields"]}}),
    )["values"]
        .clone()
}

fn status_of(core: &Core, card: &str) -> Value {
    ok(
        core,
        "card_read",
        json!({"target": format!("{card}.status")}),
    )["value"]
        .clone()
}

#[test]
fn a_value_is_read_alone_by_its_path() {
    let core = Core::open(script_store()).unwrap();
    let shipped = ok(
        &core,
        "card_read",
        json!({"target": "Schemas:Status.shipped"}),
    );
    assert_eq!(shipped["path"], "Schemas:Status.shipped");
    assert_eq!(shipped["name"], "shipped");
    assert_eq!(shipped["label"], "発送済み");
    // The rev is the enum's: a write made from this read is held to the enum as it was read.
    let card = ok(&core, "card_read", json!({"target": "Schemas:Status"}));
    assert_eq!(shipped["rev"], card["rev"]);
    // A value with no label reads none.
    let paid = ok(&core, "card_read", json!({"target": "Schemas:Status.paid"}));
    assert!(paid.get("label").is_none(), "{paid}");
}

#[test]
fn a_value_is_added_at_the_end_or_at_its_place_leaving_the_others_as_they_are() {
    let core = Core::open(script_store()).unwrap();
    let made = ok(
        &core,
        "card_create",
        json!({"target": "Schemas:Status.cancelled", "value": {"label": "Cancelled"}}),
    );
    assert_eq!(made["path"], "Schemas:Status.cancelled");
    assert_eq!(made["label"], "Cancelled");
    assert!(made["rev"].is_string(), "{made}");
    assert_eq!(
        values(&core),
        json!([{"name": "open", "label": "受付"}, {"name": "paid"},
            {"name": "shipped", "label": "発送済み"}, {"name": "cancelled", "label": "Cancelled"}])
    );
    ok(
        &core,
        "card_create",
        json!({"target": "Schemas:Status.held", "value": {"position": 1}}),
    );
    let names: Vec<Value> = values(&core)
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["name"].clone())
        .collect();
    assert_eq!(
        names,
        ["open", "held", "paid", "shipped", "cancelled"].map(Value::from)
    );
    // A name the enum has already is not made twice, and the name is the path's.
    let twice = refused(
        &core,
        "card_create",
        json!({"target": "Schemas:Status.paid", "value": {}}),
    );
    assert!(twice.contains("paid"), "{twice}");
    let other = refused(
        &core,
        "card_create",
        json!({"target": "Schemas:Status.lost", "value": {"name": "found"}}),
    );
    assert!(other.contains("lost"), "{other}");
    // A key a value does not take is refused by its form, with the keys it takes.
    let stray = refused(
        &core,
        "card_create",
        json!({"target": "Schemas:Status.lost", "value": {"value": "lost"}}),
    );
    assert!(
        stray.contains("label") && stray.contains("position"),
        "{stray}"
    );
}

#[test]
fn a_value_renamed_takes_the_data_that_holds_it_and_the_paths_that_name_it() {
    let core = with_orders();
    ok(
        &core,
        "card_create",
        json!({"target": "Free:About", "value": {"kind": "note",
            "body": "発送は [[Schemas:Status.shipped]] で表す"}}),
    );
    let renamed = ok(
        &core,
        "card_update",
        json!({"target": "Schemas:Status.shipped", "value": {"name": "dispatched"}}),
    );
    assert_eq!(renamed["path"], "Schemas:Status.dispatched");
    assert_eq!(renamed["name"], "dispatched");
    assert_eq!(renamed["dataRewritten"], json!(["Data:Order2041.status"]));
    // Its label and its place are as they were.
    assert_eq!(
        values(&core),
        json!([{"name": "open", "label": "受付"}, {"name": "paid"},
            {"name": "dispatched", "label": "発送済み"}])
    );
    assert_eq!(status_of(&core, "Data:Order2041"), "dispatched");
    assert_eq!(status_of(&core, "Data:Order2042"), "paid");
    let note = ok(&core, "card_read", json!({"target": "Free:About"}));
    assert_eq!(note["body"], "発送は [[Schemas:Status.dispatched]] で表す");
    assert!(checked(&core, "main").get("cardProblems").is_none());
}

#[test]
fn a_label_or_a_place_changes_alone() {
    let core = with_orders();
    let relabelled = ok(
        &core,
        "card_update",
        json!({"target": "Schemas:Status.shipped", "value": {"label": "Sent", "position": 0}}),
    );
    assert_eq!(relabelled["path"], "Schemas:Status.shipped");
    assert!(relabelled.get("dataRewritten").is_none(), "{relabelled}");
    assert_eq!(
        values(&core),
        json!([{"name": "shipped", "label": "Sent"}, {"name": "open", "label": "受付"},
            {"name": "paid"}])
    );
    assert_eq!(status_of(&core, "Data:Order2041"), "shipped");
    // An empty label, or null, takes it away.
    ok(
        &core,
        "card_update",
        json!({"target": "Schemas:Status.open", "value": {"label": ""}}),
    );
    let open = ok(&core, "card_read", json!({"target": "Schemas:Status.open"}));
    assert!(open.get("label").is_none(), "{open}");
    ok(
        &core,
        "card_update",
        json!({"target": "Schemas:Status.shipped", "value": {"label": null}}),
    );
    let shipped = ok(
        &core,
        "card_read",
        json!({"target": "Schemas:Status.shipped"}),
    );
    assert!(shipped.get("label").is_none(), "{shipped}");
}

#[test]
fn a_value_let_go_of_leaves_the_data_and_says_where_it_is() {
    let core = with_orders();
    // Closed: a value the enum does not have is what the inspection finds.
    ok(
        &core,
        "card_update",
        json!({"target": "Schemas:Status", "value": {"open": false}}),
    );
    let gone = ok(
        &core,
        "card_delete",
        json!({"target": "Schemas:Status.shipped"}),
    );
    assert_eq!(gone["deleted"], "Schemas:Status.shipped");
    assert_eq!(gone["valueCount"], 2);
    let warnings = gone["warnings"].to_string();
    assert!(warnings.contains("Data:Order2041.status"), "{gone}");
    assert!(!warnings.contains("Data:Order2042"), "{gone}");
    assert_eq!(status_of(&core, "Data:Order2041"), "shipped");
    assert_eq!(
        values(&core),
        json!([{"name": "open", "label": "受付"}, {"name": "paid"}])
    );
    let problems = checked(&core, "main")["cardProblems"].to_string();
    assert!(problems.contains("Data:Order2041.status"), "{problems}");
}

#[test]
fn a_value_the_enum_does_not_have_is_refused_with_the_ones_it_has() {
    let core = Core::open(script_store()).unwrap();
    for (tool, value) in [
        ("card_read", json!({})),
        ("card_update", json!({"label": "x"})),
        ("card_delete", json!({})),
    ] {
        let said = refused(
            &core,
            tool,
            json!({"target": "Schemas:Status.lost", "value": value}),
        );
        for name in ["open", "paid", "shipped"] {
            assert!(said.contains(name), "{tool}: {said}");
        }
    }
    let taken = refused(
        &core,
        "card_update",
        json!({"target": "Schemas:Status.paid", "value": {"name": "open"}}),
    );
    assert!(taken.contains("open"), "{taken}");
}

#[test]
fn a_value_is_written_with_the_rev_it_was_read_at() {
    let core = Core::open(script_store()).unwrap();
    let read = ok(&core, "card_read", json!({"target": "Schemas:Status.paid"}));
    let rev = read["rev"].clone();
    ok(
        &core,
        "card_update",
        json!({"target": "Schemas:Status.paid", "value": {"label": "Paid", "rev": rev}}),
    );
    // Another value written since: the enum is not as it was read.
    let stale = refused(
        &core,
        "card_update",
        json!({"target": "Schemas:Status.open", "value": {"label": "Open", "rev": rev}}),
    );
    assert!(stale.contains("Schemas:Status.open"), "{stale}");
}

// MARK: - What a value stands for

#[test]
fn a_value_says_what_it_stands_for_and_a_whole_list_keeps_it() {
    let core = Core::open(script_store()).unwrap();
    ok(
        &core,
        "card_update",
        json!({"target": "Schemas:Status", "value": {"values": [
            {"name": "open", "label": "受付", "description": "注文を受けて、まだ払われていない"},
            {"name": "paid"},
            {"name": "shipped", "label": "発送済み", "description": "  倉庫から出た  "}
        ]}}),
    );
    assert_eq!(
        values(&core),
        json!([
            {"name": "open", "label": "受付", "description": "注文を受けて、まだ払われていない"},
            {"name": "paid"},
            {"name": "shipped", "label": "発送済み", "description": "倉庫から出た"}
        ])
    );
    let shipped = ok(
        &core,
        "card_read",
        json!({"target": "Schemas:Status.shipped"}),
    );
    assert_eq!(shipped["description"], "倉庫から出た", "{shipped}");
    // A value written alone, by its path, writes the others back with what they say.
    ok(
        &core,
        "card_update",
        json!({"target": "Schemas:Status.paid", "value": {"label": "支払い済み", "position": 0}}),
    );
    ok(
        &core,
        "card_update",
        json!({"target": "Schemas:Status.shipped", "value": {"name": "sent"}}),
    );
    assert_eq!(
        values(&core),
        json!([
            {"name": "paid", "label": "支払い済み"},
            {"name": "open", "label": "受付", "description": "注文を受けて、まだ払われていない"},
            {"name": "sent", "label": "発送済み", "description": "倉庫から出た"}
        ])
    );
}

#[test]
fn a_values_description_is_added_changed_and_taken_away_by_its_path() {
    let core = Core::open(script_store()).unwrap();
    let made = ok(
        &core,
        "card_create",
        json!({"target": "Schemas:Status.cancelled", "value": {"label": "取消", "description": "払う前に取りやめた"}}),
    );
    assert_eq!(made["description"], "払う前に取りやめた", "{made}");
    ok(
        &core,
        "card_update",
        json!({"target": "Schemas:Status.cancelled", "value": {"description": "客が取りやめた"}}),
    );
    let read = ok(
        &core,
        "card_read",
        json!({"target": "Schemas:Status.cancelled"}),
    );
    assert_eq!(read["description"], "客が取りやめた", "{read}");
    for gone in [Value::Null, json!("")] {
        ok(
            &core,
            "card_update",
            json!({"target": "Schemas:Status.cancelled", "value": {"description": "またある"}}),
        );
        let read = ok(
            &core,
            "card_update",
            json!({"target": "Schemas:Status.cancelled", "value": {"description": gone}}),
        );
        assert!(read.get("description").is_none(), "{read}");
        assert_eq!(read["label"], "取消", "{read}");
    }
}

#[test]
fn a_description_is_one_line_of_500_characters_at_most() {
    let core = Core::open(script_store()).unwrap();
    let long = "あ".repeat(501);
    for (target, value) in [
        (
            "Schemas:Status.paid",
            json!({"description": "一行目\n二行目"}),
        ),
        ("Schemas:Status.paid", json!({"description": long})),
        ("Schemas:Status.paid", json!({"description": 3})),
        (
            "Schemas:Status",
            json!({"values": [{"name": "paid", "description": "一行目\r\n二行目"}]}),
        ),
        (
            "Schemas:Status",
            json!({"values": [{"name": "paid", "description": long}]}),
        ),
    ] {
        let said = refused(
            &core,
            "card_update",
            json!({"target": target, "value": value}),
        );
        assert!(
            said.contains("改行") || said.contains("500") || said.contains("description"),
            "{said}"
        );
    }
    let said = refused(
        &core,
        "card_create",
        json!({"target": "Schemas:Kind", "value": {"kind": "enum", "values": [
            {"name": "a", "description": "一行目\n二行目"}
        ]}}),
    );
    assert!(said.contains("改行"), "{said}");
    ok(
        &core,
        "card_update",
        json!({"target": "Schemas:Status.paid", "value": {"description": "あ".repeat(500)}}),
    );
}

#[test]
fn a_description_edited_in_a_branch_merges_by_its_value() {
    let core = Core::open(script_store()).unwrap();
    ok(&core, "branch_create", json!({"target": "words"}));
    ok(
        &core,
        "card_update",
        json!({"branch": "words", "target": "Schemas:Status.paid", "value": {"description": "代金を受け取った"}}),
    );
    // main meanwhile relabels another value of the same enum.
    ok(
        &core,
        "card_update",
        json!({"target": "Schemas:Status.open", "value": {"label": "受け付けた"}}),
    );
    let merged = ok(&core, "branch_merge", json!({"target": "words"}));
    assert!(merged.get("conflicts").is_none(), "{merged}");
    assert_eq!(
        values(&core),
        json!([
            {"name": "open", "label": "受け付けた"},
            {"name": "paid", "description": "代金を受け取った"},
            {"name": "shipped", "label": "発送済み"}
        ])
    );
    assert!(checked(&core, "main").get("cardProblems").is_none());
}

/// What a value stands for goes with the project file, whose version says so: an app that reads
/// none refuses the file as newer, not as cards that do not fit.
#[test]
fn a_values_description_goes_with_the_project_file() {
    let core = Core::open(script_store()).unwrap();
    ok(
        &core,
        "card_update",
        json!({"target": "Schemas:Status.paid", "value": {"description": "代金を受け取った"}}),
    );
    let project = common::project_id(&core);
    let text =
        common::op(&core, json!({"op": "project_file", "project": project}))["result"]["text"]
            .as_str()
            .unwrap()
            .to_string();
    let file: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(file["version"], 33);
    assert!(text.contains("代金を受け取った"));
    let again = Core::create(common::place("described-again.sqlite")).unwrap();
    let read = common::op(&again, json!({"op": "project_file_read", "text": text}));
    assert!(read.get("error").is_none(), "{read}");
    assert_eq!(values(&again)[1]["description"], "代金を受け取った");
}
