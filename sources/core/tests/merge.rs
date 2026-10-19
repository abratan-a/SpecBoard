//! A merge between any two trees, main among them, as MCP asks for it (`branch_read` with
//! `against`, `branch_merge`): compared unit by unit from what both last had in common,
//! settled where they part, and written into the target the way it keeps what it holds
//! (over the sample's project).

use crate::common;

use common::{call, ok, refused, script_store};
use serde_json::{Value, json};
use specboard_core::Core;

const PROJECT: &str = "見本";

fn read(core: &Core, branch: &str, path: &str) -> Value {
    ok(core, "card_read", json!({"branch": branch, "target": path}))
}

fn with_branch(name: &str) -> Core {
    let core = Core::open(script_store()).unwrap();
    ok(&core, "branch_create", json!({"target": name}));
    core
}

#[test]
fn units_changed_apart_both_go_in_and_alike_conflict_until_chosen() {
    let core = with_branch("work");
    ok(
        &core,
        "card_update",
        json!({"branch": "work", "target": "Schemas/Ordering:Order", "value": {"description": "branch の説明"}}),
    );
    ok(
        &core,
        "card_create",
        json!({"branch": "work", "target": "Schemas/Ordering:Order.refunded", "value": {"type": "bool"}}),
    );
    ok(
        &core,
        "card_update",
        json!({"branch": "work", "target": "Schemas:Customer", "value": {"label": "branch の客"}}),
    );
    ok(
        &core,
        "card_update",
        json!({"target": "Schemas/Ordering:Order", "value": {"label": "main の注文"}}),
    );
    ok(
        &core,
        "card_update",
        json!({"target": "Schemas:Customer", "value": {"label": "main の客"}}),
    );

    let preview = ok(
        &core,
        "branch_read",
        json!({"target": "work", "value": {"against": "main"}}),
    );
    let conflicts: Vec<(String, String)> = preview["conflicts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| {
            (
                c["path"].as_str().unwrap().into(),
                c["unit"].as_str().unwrap().into(),
            )
        })
        .collect();
    assert_eq!(
        conflicts,
        [("Schemas:Customer".to_string(), "label".to_string())]
    );
    assert_eq!(preview["conflicts"][0]["source"], "branch の客");
    assert_eq!(preview["conflicts"][0]["target"], "main の客");
    let said = refused(&core, "branch_merge", json!({"target": "work"}));
    assert!(said.contains("衝突が 1 あるので merge しません"), "{said}");

    let done = ok(
        &core,
        "branch_merge",
        json!({"target": "work", "value": {"take": {"Schemas:Customer": {"label": "source"}}, "keep": true}}),
    );
    assert_eq!(
        (done["merged"].as_str(), done["into"].as_str()),
        (Some("work"), Some("main"))
    );
    let order = read(&core, "main", "Schemas/Ordering:Order");
    assert_eq!(order["description"], "branch の説明");
    assert_eq!(order["label"], "main の注文");
    let fields: Vec<&str> = order["fields"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["name"].as_str().unwrap())
        .collect();
    assert!(
        fields.contains(&"refunded") && fields.contains(&"total"),
        "{fields:?}"
    );
    assert_eq!(
        read(&core, "main", "Schemas:Customer")["label"],
        "branch の客"
    );
    // Into main: the source is main again.
    let after = ok(&core, "branch_read", json!({"target": "work"}));
    for key in [
        "changed",
        "made",
        "removed",
        "mainChanged",
        "mainMade",
        "mainRemoved",
    ] {
        assert!(after.get(key).is_none(), "{key}: {after}");
    }
    // Read still, as a branch merged and kept.
    assert!(after["merged"].is_string(), "{after}");
    // One commit in each, a merge.
    let history = ok(
        &core,
        "branch_read",
        json!({"target": "main", "value": {"as": "history", "limit": 1}}),
    );
    assert_eq!(history["commits"][0]["kind"], "merge");
    assert!(
        history["commits"][0]["lines"][0]
            .as_str()
            .unwrap()
            .starts_with("work を merge（")
    );
}

#[test]
fn a_card_one_side_removed_and_the_other_changed_is_chosen_whole() {
    let core = with_branch("work");
    ok(
        &core,
        "card_delete",
        json!({"branch": "work", "target": "Schemas:Customer"}),
    );
    ok(
        &core,
        "card_update",
        json!({"target": "Schemas:Customer", "value": {"description": "main で直した"}}),
    );
    let preview = ok(
        &core,
        "branch_read",
        json!({"target": "work", "value": {"against": "main"}}),
    );
    assert_eq!(preview["conflicts"][0]["unit"], "（カード）");
    assert!(preview["conflicts"][0]["source"].is_null());
    ok(
        &core,
        "branch_merge",
        json!({"target": "work", "value": {"take": {"Schemas:Customer": {"（カード）": "target"}}}}),
    );
    assert_eq!(
        read(&core, "main", "Schemas:Customer")["description"],
        "main で直した"
    );
}

#[test]
fn main_taken_in_moves_the_base_on() {
    let core = with_branch("work");
    ok(
        &core,
        "card_update",
        json!({"branch": "work", "target": "Schemas/Ordering:Order", "value": {"description": "branch の説明"}}),
    );
    ok(
        &core,
        "card_update",
        json!({"target": "Schemas/Ordering:Order", "value": {"label": "main の注文"}}),
    );
    ok(
        &core,
        "card_update",
        json!({"target": "Schemas:Customer", "value": {"label": "main の客"}}),
    );
    let listed = ok(&core, "branch_list", json!({}));
    let work = listed["branches"]
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["branch"] == "work")
        .unwrap()
        .clone();
    assert_eq!(work["mainMoved"], 2, "{work}");

    let done = ok(
        &core,
        "branch_merge",
        json!({"target": "main", "value": {"into": "work"}}),
    );
    assert_eq!(
        (done["merged"].as_str(), done["into"].as_str()),
        (Some("main"), Some("work"))
    );
    let order = read(&core, "work", "Schemas/Ordering:Order");
    assert_eq!(order["description"], "branch の説明");
    assert_eq!(order["label"], "main の注文");
    let diff = ok(&core, "branch_read", json!({"target": "work"}));
    assert!(
        diff.get("mainChanged").is_none() && diff.get("mainMade").is_none(),
        "{diff}"
    );
    assert_eq!(diff["changed"][0]["card"], "Schemas/Ordering:Order");
    assert!(
        diff["changed"][0]["changes"]
            .as_array()
            .unwrap()
            .iter()
            .all(|c| !c.as_str().unwrap().contains("main の注文")),
        "{diff}"
    );
    // main is as it was.
    assert_ne!(
        read(&core, "main", "Schemas/Ordering:Order")["description"],
        "branch の説明"
    );
}

#[test]
fn branch_into_branch_brings_what_the_source_made() {
    let core = with_branch("refunds2");
    ok(&core, "branch_create", json!({"target": "keys"}));
    ok(
        &core,
        "card_update",
        json!({"branch": "refunds2", "target": "Schemas/Ordering:Order", "value": {"description": "refunds2 の説明"}}),
    );
    ok(
        &core,
        "card_create",
        json!({"branch": "refunds2", "target": "Schemas:Voucher", "value": {"kind": "entity"}}),
    );
    ok(
        &core,
        "card_update",
        json!({"target": "Schemas/Ordering:Order", "value": {"label": "main の注文"}}),
    );

    ok(
        &core,
        "branch_merge",
        json!({"target": "refunds2", "value": {"into": "keys"}}),
    );
    let order = read(&core, "keys", "Schemas/Ordering:Order");
    assert_eq!(order["description"], "refunds2 の説明");
    // keys sees main as it was made: main's label since is not its until it takes
    // main in.
    assert_eq!(order["label"], "注文");
    read(&core, "keys", "Schemas:Voucher");
    let diff = ok(&core, "branch_read", json!({"target": "keys"}));
    assert_eq!(diff["made"], json!(["Schemas:Voucher"]));
    assert_eq!(diff["changed"][0]["card"], "Schemas/Ordering:Order");
    // The source is as it was, and main too.
    assert_ne!(
        read(&core, "refunds2", "Schemas/Ordering:Order")["label"],
        "main の注文"
    );
    let (failed, _) = call(
        &core,
        "card_read",
        json!({"project": PROJECT, "target": "Schemas:Voucher"}),
    );
    assert!(failed);
}

#[test]
fn two_branches_from_main_at_two_times_are_compared_from_the_older_base() {
    let core = with_branch("early");
    ok(
        &core,
        "card_update",
        json!({"branch": "early", "target": "Schemas/Ordering:Order", "value": {"label": "early の名"}}),
    );
    ok(
        &core,
        "card_update",
        json!({"target": "Schemas/Ordering:Order", "value": {"description": "main で後から"}}),
    );
    ok(&core, "branch_create", json!({"target": "late"}));
    ok(
        &core,
        "card_update",
        json!({"branch": "late", "target": "Schemas/Ordering:Order", "value": {"tags": ["late"]}}),
    );

    let preview = ok(
        &core,
        "branch_read",
        json!({"target": "early", "value": {"against": "late"}}),
    );
    assert!(preview.get("conflicts").is_none(), "{preview}");
    ok(
        &core,
        "branch_merge",
        json!({"target": "early", "value": {"into": "late"}}),
    );
    let order = read(&core, "late", "Schemas/Ordering:Order");
    assert_eq!(order["description"], "main で後から");
    assert_eq!(order["label"], "early の名");
    assert_eq!(order["tags"], json!(["late"]));
}

#[test]
fn two_things_at_one_path_are_refused_and_nothing_is_written() {
    let core = with_branch("work");
    ok(
        &core,
        "card_create",
        json!({"branch": "work", "target": "Schemas:Voucher", "value": {"kind": "entity"}}),
    );
    ok(
        &core,
        "card_create",
        json!({"target": "Schemas:Voucher", "value": {"kind": "value"}}),
    );
    let preview = ok(
        &core,
        "branch_read",
        json!({"target": "work", "value": {"against": "main"}}),
    );
    assert_eq!(preview["overlaps"], json!(["Schemas:Voucher"]));
    let said = refused(&core, "branch_merge", json!({"target": "work"}));
    assert!(said.contains("名前を変えるか消してから"), "{said}");
    assert_eq!(read(&core, "main", "Schemas:Voucher")["kind"], "value");
}

/// The rows a card keeps by id — a record's fields, an enum's values, a stackdoc's versions, a
/// formatdoc's rows — made with one name on both sides (made by two branches): one after a
/// merge, not two.
fn rows_made_on_both_sides() -> Core {
    let core = Core::open(script_store()).unwrap();
    ok(
        &core,
        "card_create",
        json!({"target": "Docs:Log", "value": {"kind": "stackdoc"}}),
    );
    ok(
        &core,
        "card_create",
        json!({"target": "Schemas:Level", "value": {"kind": "enum", "values": [{"name": "low"}]}}),
    );
    ok(&core, "branch_create", json!({"target": "work"}));
    for tree in ["main", "work"] {
        ok(
            &core,
            "card_create",
            json!({"branch": tree, "target": "Docs:Log.v1", "value": {"key": "1", "body": format!("{tree} の行")}}),
        );
        ok(
            &core,
            "card_create",
            json!({"branch": tree, "target": "Schemas/Ordering:Order.memo", "value": {"type": "string"}}),
        );
        ok(
            &core,
            "card_update",
            json!({"branch": tree, "target": "Schemas:Level", "value": {"values": ["low", "high"]}}),
        );
        ok(
            &core,
            "card_create",
            json!({"branch": tree, "target": "Docs:Spec.intro.memo", "value": {"kind": "text", "body": format!("{tree} のメモ")}}),
        );
    }
    core
}

#[test]
fn rows_of_one_name_made_on_both_sides_are_refused_whichever_way() {
    let core = rows_made_on_both_sides();
    let paths = json!([
        "Docs:Log.v1",
        "Docs:Spec.intro.memo",
        "Schemas/Ordering:Order.memo",
        "Schemas:Level.high"
    ]);
    // Into main, and main taken in: seen first, and refused, nothing written.
    for (source, into) in [("work", "main"), ("main", "work")] {
        let preview = ok(
            &core,
            "branch_read",
            json!({"target": source, "value": {"against": into}}),
        );
        assert_eq!(preview["overlaps"], paths, "{source} → {into}: {preview}");
        let said = refused(
            &core,
            "branch_merge",
            json!({"target": source, "value": {"into": into}}),
        );
        assert!(said.contains("名前を変えるか消してから"), "{said}");
    }
    let log = read(&core, "main", "Docs:Log");
    assert_eq!(log["versions"].as_array().unwrap().len(), 1, "{log}");
    let log = read(&core, "work", "Docs:Log");
    assert_eq!(log["versions"].as_array().unwrap().len(), 1, "{log}");
    // One side's let go of: the merge goes in, and the other side's is the one there.
    ok(
        &core,
        "card_delete",
        json!({"branch": "work", "target": ["Docs:Log.v1", "Docs:Spec.intro.memo", "Schemas/Ordering:Order.memo"]}),
    );
    ok(
        &core,
        "card_update",
        json!({"branch": "work", "target": "Schemas:Level", "value": {"values": ["low"]}}),
    );
    ok(
        &core,
        "branch_merge",
        json!({"target": "main", "value": {"into": "work"}}),
    );
    let log = read(&core, "work", "Docs:Log");
    assert_eq!(log["versions"].as_array().unwrap().len(), 1, "{log}");
    assert_eq!(
        read(&core, "work", "Docs:Log.v1")["body"],
        "main の行",
        "{log}"
    );
    assert_eq!(
        read(&core, "work", "Docs:Spec.intro.memo")["body"],
        "main のメモ"
    );
}

/// The conflicts of a preview as `(path, unit)`.
fn conflicts(preview: &Value) -> Vec<(String, String)> {
    preview["conflicts"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|c| {
            (
                c["path"].as_str().unwrap().to_string(),
                c["unit"].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

fn conflict(path: &str, unit: &str) -> (String, String) {
    (path.to_string(), unit.to_string())
}

#[test]
fn rows_of_a_formatdoc_two_branches_changed_apart_both_go_in() {
    let core = Core::open(script_store()).unwrap();
    for name in ["a", "b"] {
        ok(&core, "branch_create", json!({"target": name}));
    }
    ok(
        &core,
        "card_update",
        json!({"branch": "a", "target": "Docs:Spec.intro.body", "value": {"body": "a の本文"}}),
    );
    ok(
        &core,
        "card_update",
        json!({"branch": "b", "target": "Docs:Spec.tables", "value": {"heading": "b の見出し"}}),
    );
    ok(&core, "branch_merge", json!({"target": "a"}));
    let preview = ok(
        &core,
        "branch_read",
        json!({"target": "b", "value": {"against": "main"}}),
    );
    assert!(conflicts(&preview).is_empty(), "{preview}");
    ok(&core, "branch_merge", json!({"target": "b"}));
    assert_eq!(
        read(&core, "main", "Docs:Spec.intro.body")["body"],
        "a の本文"
    );
    assert_eq!(
        read(&core, "main", "Docs:Spec.tables")["heading"],
        "b の見出し"
    );
}

#[test]
fn one_row_both_sides_changed_is_a_conflict_of_that_row_chosen_by_its_path() {
    let core = with_branch("work");
    for (tree, row, value) in [
        (
            "work",
            "Docs:Spec.intro.body",
            json!({"body": "work の本文"}),
        ),
        (
            "work",
            "Docs:Spec.cards",
            json!({"heading": "work の見出し"}),
        ),
        (
            "main",
            "Docs:Spec.intro.body",
            json!({"body": "main の本文"}),
        ),
        (
            "main",
            "Docs:Spec.tables",
            json!({"heading": "main の見出し"}),
        ),
    ] {
        ok(
            &core,
            "card_update",
            json!({"branch": tree, "target": row, "value": value}),
        );
    }
    let preview = ok(
        &core,
        "branch_read",
        json!({"target": "work", "value": {"against": "main"}}),
    );
    assert_eq!(
        conflicts(&preview),
        [conflict("Docs:Spec", "rows.intro.body")],
        "{preview}"
    );
    let said = &preview["conflicts"][0];
    assert_eq!(said["source"]["body"], "work の本文", "{said}");
    assert_eq!(said["target"]["body"], "main の本文", "{said}");
    let refusal = refused(&core, "branch_merge", json!({"target": "work"}));
    assert!(
        refusal.contains("衝突が 1 あるので merge しません"),
        "{refusal}"
    );
    ok(
        &core,
        "branch_merge",
        json!({"target": "work", "value": {"take": {"Docs:Spec": {"rows.intro.body": "source"}}}}),
    );
    assert_eq!(
        read(&core, "main", "Docs:Spec.intro.body")["body"],
        "work の本文"
    );
    assert_eq!(
        read(&core, "main", "Docs:Spec.cards")["heading"],
        "work の見出し"
    );
    assert_eq!(
        read(&core, "main", "Docs:Spec.tables")["heading"],
        "main の見出し"
    );
}

#[test]
fn rows_added_apart_on_both_sides_both_go_in() {
    let core = with_branch("work");
    ok(
        &core,
        "card_create",
        json!({"branch": "work", "target": "Docs:Spec.intro.alpha", "value": {"kind": "text", "body": "work"}}),
    );
    ok(
        &core,
        "card_create",
        json!({"target": "Docs:Spec.intro.beta", "value": {"kind": "text", "body": "main"}}),
    );
    ok(&core, "branch_merge", json!({"target": "work"}));
    let intro = read(&core, "main", "Docs:Spec.intro");
    let names: Vec<&str> = intro["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["path"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        [
            "Docs:Spec.intro.body",
            "Docs:Spec.intro.alpha",
            "Docs:Spec.intro.beta"
        ],
        "{intro}"
    );
}

/// main removed the item `intro` of `Docs:Spec`; the branch added a row inside it.
fn an_item_removed_and_a_row_added_inside() -> Core {
    let core = with_branch("work");
    ok(
        &core,
        "card_create",
        json!({"branch": "work", "target": "Docs:Spec.intro.extra", "value": {"kind": "text", "body": "足した"}}),
    );
    ok(&core, "card_delete", json!({"target": "Docs:Spec.intro"}));
    core
}

#[test]
fn a_row_added_inside_an_item_the_other_side_removed_is_a_conflict_of_the_item() {
    let core = an_item_removed_and_a_row_added_inside();
    let preview = ok(
        &core,
        "branch_read",
        json!({"target": "work", "value": {"against": "main"}}),
    );
    assert_eq!(
        conflicts(&preview),
        [conflict("Docs:Spec", "rows.intro")],
        "{preview}"
    );
    let said = &preview["conflicts"][0];
    assert!(said["target"].is_null(), "{said}");
    assert_eq!(
        said["source"]["rows"].as_array().unwrap().len(),
        2,
        "{said}"
    );
    // The removal taken: the item goes, and the row added inside it with it.
    ok(
        &core,
        "branch_merge",
        json!({"target": "work", "value": {"take": {"Docs:Spec": {"rows.intro": "target"}}, "keep": true}}),
    );
    let spec = read(&core, "main", "Docs:Spec");
    let rows: Vec<&str> = spec["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["path"].as_str().unwrap())
        .collect();
    assert_eq!(rows, ["Docs:Spec.tables", "Docs:Spec.cards"], "{spec}");
    // The item kept: it stays with all it holds, the row added inside it among them.
    let core = an_item_removed_and_a_row_added_inside();
    ok(
        &core,
        "branch_merge",
        json!({"target": "work", "value": {"take": {"Docs:Spec": {"rows.intro": "source"}}}}),
    );
    assert_eq!(
        read(&core, "main", "Docs:Spec.intro.extra")["body"],
        "足した"
    );
    let intro = read(&core, "main", "Docs:Spec.intro");
    assert_eq!(intro["rows"].as_array().unwrap().len(), 2, "{intro}");
}

#[test]
fn an_item_the_branch_removed_where_main_changed_a_row_inside_is_kept_whole_when_kept() {
    let core = with_branch("work");
    ok(
        &core,
        "card_delete",
        json!({"branch": "work", "target": "Docs:Spec.tables"}),
    );
    ok(
        &core,
        "card_update",
        json!({"target": "Docs:Spec.tables.words", "value": {"columns": ["main の列"]}}),
    );
    let preview = ok(
        &core,
        "branch_read",
        json!({"target": "work", "value": {"against": "main"}}),
    );
    assert_eq!(
        conflicts(&preview),
        [conflict("Docs:Spec", "rows.tables")],
        "{preview}"
    );
    ok(
        &core,
        "branch_merge",
        json!({"target": "work", "value": {"take": {"Docs:Spec": {"rows.tables": "target"}}}}),
    );
    let tables = read(&core, "main", "Docs:Spec.tables");
    assert_eq!(tables["rows"].as_array().unwrap().len(), 2, "{tables}");
    assert_eq!(
        read(&core, "main", "Docs:Spec.tables.words")["columns"],
        json!(["main の列"])
    );
}

#[test]
fn versions_both_sides_changed_are_conflicts_named_one_by_one() {
    let core = Core::open(script_store()).unwrap();
    ok(
        &core,
        "card_create",
        json!({"target": "Docs:Log", "value": {"kind": "stackdoc", "versions": [
            {"name": "v2", "key": "2", "body": "二"},
            {"name": "v1", "key": "1", "body": "一"}]}}),
    );
    ok(&core, "branch_create", json!({"target": "work"}));
    for tree in ["work", "main"] {
        for version in ["v1", "v2"] {
            ok(
                &core,
                "card_update",
                json!({"branch": tree, "target": format!("Docs:Log.{version}"), "value": {"body": format!("{tree} の {version}")}}),
            );
        }
    }
    let preview = ok(
        &core,
        "branch_read",
        json!({"target": "work", "value": {"against": "main"}}),
    );
    let mut said = conflicts(&preview);
    said.sort();
    assert_eq!(
        said,
        [
            conflict("Docs:Log", "versions.v1"),
            conflict("Docs:Log", "versions.v2")
        ],
        "{preview}"
    );
    ok(
        &core,
        "branch_merge",
        json!({"target": "work", "value": {"take": {"Docs:Log": {"versions.v1": "source", "versions.v2": "target"}}}}),
    );
    assert_eq!(read(&core, "main", "Docs:Log.v1")["body"], "work の v1");
    assert_eq!(read(&core, "main", "Docs:Log.v2")["body"], "main の v2");
}

#[test]
fn a_card_the_target_places_nowhere_goes_where_the_source_put_it() {
    let core = with_branch("work");
    ok(
        &core,
        "card_update",
        json!({"target": "Schemas/Ordering:Order", "value": {"topLeft": {"x": 300, "y": 4}}}),
    );
    ok(
        &core,
        "card_create",
        json!({"branch": "work", "target": "Schemas:Voucher", "value": {"kind": "entity", "topLeft": {"x": 400, "y": 20}}}),
    );
    ok(
        &core,
        "card_update",
        json!({"branch": "work", "target": "Schemas/Ordering:Order", "value": {"description": "branch で直す", "topLeft": {"x": 500, "y": 50}}}),
    );
    ok(&core, "branch_merge", json!({"target": "work"}));
    assert_eq!(
        read(&core, "main", "Schemas:Voucher")["topLeft"],
        json!({"x": 400, "y": 20})
    );
    let order = read(&core, "main", "Schemas/Ordering:Order");
    assert_eq!(order["topLeft"], json!({"x": 300, "y": 4}));
    assert_eq!(order["description"], "branch で直す");
}

#[test]
fn paths_the_merge_breaks_are_told_not_refused() {
    let core = with_branch("work");
    ok(
        &core,
        "card_update",
        json!({"branch": "work", "target": "Schemas/Ordering:Order.total", "value": {"name": "amount"}}),
    );
    // Made in main after: it names the old name, which the merge takes away.
    ok(
        &core,
        "card_create",
        json!({"target": "Free:Audit", "value": {"kind": "note", "body": "[[Schemas/Ordering:Order.total]] を見る"}}),
    );
    let done = ok(&core, "branch_merge", json!({"target": "work"}));
    let warnings = done["warnings"].as_array().cloned().unwrap_or_default();
    assert!(
        warnings
            .iter()
            .any(|w| w.as_str().unwrap().contains("Schemas/Ordering:Order.total")),
        "{done}"
    );
    read(&core, "main", "Schemas/Ordering:Order.amount");
}

/// Into a locked main, an AI's merge is not merged but asked for: a merge request a person
/// approves. main stays as it was.
#[test]
fn an_ai_merging_into_a_locked_main_asks_for_it() {
    let store = script_store();
    let project: String = rusqlite::Connection::open(&store)
        .unwrap()
        .query_row("SELECT id FROM projects WHERE name = ?", [PROJECT], |r| {
            r.get(0)
        })
        .unwrap();
    let core = Core::open(&store).unwrap();
    ok(&core, "branch_create", json!({"target": "work"}));
    ok(
        &core,
        "card_update",
        json!({"branch": "work", "target": "Schemas:Customer", "value": {"label": "客"}}),
    );
    let locked: Value = serde_json::from_str(
        &core.call(&json!({"op": "main_lock", "project": project, "on": true}).to_string()),
    )
    .unwrap();
    assert!(locked.get("error").is_none(), "{locked}");
    let asked = ok(&core, "branch_merge", json!({"target": "work"}));
    assert_eq!(asked["mr"], 1, "{asked}");
    assert_eq!(asked["state"], "open", "{asked}");
    assert_eq!(asked["title"], "work", "題を省けば branch の名前: {asked}");
    let said = asked["said"].as_str().unwrap();
    assert!(
        said.starts_with("main はロック中なので、merge せずに MR 1 を出しました"),
        "{said}"
    );
    assert!(said.contains("見て承認するよう伝えてください"), "{said}");
    assert_eq!(
        asked["result"]["change"],
        json!(["Schemas:Customer"]),
        "{asked}"
    );
    assert_eq!(
        read(&core, "main", "Schemas:Customer")["label"],
        Value::Null,
        "main に入った"
    );
    // Asked again: the one before withdrawn, a new one out.
    let again = ok(
        &core,
        "branch_merge",
        json!({"target": "work", "value": {"title": "客の表示名", "description": "なぜ"}}),
    );
    assert_eq!(again["mr"], 2, "{again}");
    assert_eq!(again["title"], "客の表示名");
    let listed = ok(&core, "mr_list", json!({}));
    let states: Vec<(i64, &str)> = listed["mrs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| (m["mr"].as_i64().unwrap(), m["state"].as_str().unwrap()))
        .collect();
    assert_eq!(states, [(2, "open"), (1, "withdrawn")]);
    // The tool of before says what to call instead.
    let message = json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call",
        "params": {"name": "pr_create", "arguments": {"project": PROJECT, "target": "work"}}});
    let reply: Value = serde_json::from_str(&core.call(
        &json!({"op": "mcp", "message": message, "headers": common::headers(&core)}).to_string(),
    ))
    .unwrap();
    let gone = reply["result"]["body"]["error"]["message"]
        .as_str()
        .unwrap_or_default();
    assert!(
        gone.contains("branch_merge で main へ入れると、ロック中なら MR"),
        "{reply}"
    );
    // Into a branch, the lock is not in the way.
    ok(&core, "branch_create", json!({"target": "other"}));
    ok(
        &core,
        "branch_merge",
        json!({"target": "work", "value": {"into": "other"}}),
    );
    assert_eq!(read(&core, "other", "Schemas:Customer")["label"], "客");
}

/// What the core tells, heard.
struct Heard(std::sync::Mutex<Vec<Value>>);

impl specboard_core::Listener for Heard {
    fn changed(&self, notice: &str) {
        self.0
            .lock()
            .unwrap()
            .push(serde_json::from_str(notice).unwrap());
    }
}

/// The project's switch: an AI's merge into a locked main goes in without being approved, says
/// so, and the window is told who merged what; MCP reads the switch and cannot change it; off
/// again, a merge request as before.
#[test]
fn an_ai_merges_into_a_locked_main_unasked_when_the_project_lets_it() {
    let store = script_store();
    let project: String = rusqlite::Connection::open(&store)
        .unwrap()
        .query_row("SELECT id FROM projects WHERE name = ?", [PROJECT], |r| {
            r.get(0)
        })
        .unwrap();
    let core = Core::open(&store).unwrap();
    let window = |request: Value| -> Value {
        serde_json::from_str(&core.call(&request.to_string())).unwrap()
    };
    ok(&core, "branch_create", json!({"target": "work"}));
    ok(
        &core,
        "card_update",
        json!({"branch": "work", "target": "Schemas:Customer", "value": {"label": "客"}}),
    );
    window(json!({"op": "main_lock", "project": project, "on": true}));
    let on = window(json!({"op": "main_ai_merge", "project": project, "on": true}));
    assert_eq!(on, json!({"result": {}}));
    let projects = window(json!({"op": "projects"}));
    let listed = projects["result"]["projects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == project.as_str())
        .unwrap()
        .clone();
    assert_eq!(listed["aiMerge"], true);
    let read_back = ok(&core, "project_read", json!({}));
    assert_eq!(
        (read_back["locked"].clone(), read_back["aiMerge"].clone()),
        (json!(true), json!(true))
    );

    let heard = std::sync::Arc::new(Heard(Default::default()));
    core.listen(heard.clone());
    // Kept: written in again below.
    let merged = ok(
        &core,
        "branch_merge",
        json!({"target": "work", "value": {"keep": true}}),
    );
    assert_eq!(
        merged["said"],
        "自動 merge が有効なので、承認なしで main に入れました"
    );
    assert_eq!(merged["changed"], json!(["Schemas:Customer"]));
    assert_eq!(read(&core, "main", "Schemas:Customer")["label"], "客");
    assert_eq!(ok(&core, "mr_list", json!({}))["mrs"], json!([]));
    let told: Vec<Value> = heard
        .0
        .lock()
        .unwrap()
        .iter()
        .filter(|n| n["op"] == "merged")
        .cloned()
        .collect();
    assert_eq!(
        told,
        [
            json!({"op": "merged", "project": project, "branch": "work", "by": common::KEY,
                "added": 0, "changed": 1, "removed": 0})
        ]
    );

    // While the window puts main right in one go, an AI's merge waits.
    ok(
        &core,
        "card_update",
        json!({"branch": "work", "target": "Schemas:Customer", "value": {"label": "客 2"}}),
    );
    window(json!({"op": "group_begin", "project": project}));
    let held = refused(&core, "branch_merge", json!({"target": "work"}));
    assert!(held.contains("アプリバッチ処理中です（main、"), "{held}");
    window(json!({"op": "group_abandon", "project": project}));

    // Off again: asked for, as before.
    window(json!({"op": "main_ai_merge", "project": project, "on": false}));
    let asked = ok(&core, "branch_merge", json!({"target": "work"}));
    assert_eq!(asked["mr"], 1, "{asked}");
    assert!(
        ok(&core, "project_read", json!({}))
            .get("aiMerge")
            .is_none()
    );
    let commits = window(json!({"op": "commits", "project": project, "limit": 6}));
    let lines: Vec<&str> = commits["result"]["commits"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["lines"][0]["summary"].as_str().unwrap())
        .collect();
    assert!(
        lines.contains(&"ロック中も、AI の merge を承認なしで main に入れるようにした")
            && lines.contains(&"ロック中の main への AI の merge を、承認を待つ MR に戻した"),
        "{lines:?}"
    );
}

// MARK: - The boxes a branch moved, taken along (`places`)

/// In main, before the branch: a card ghost of Customer in Ordering, a goal with a task inside,
/// and Order and Ordering itself drawn on Schemas' diagram.
fn arranged_project() -> Core {
    let core = Core::open(script_store()).unwrap();
    ok(
        &core,
        "card_create",
        json!({"target": "Schemas/Ordering:Client", "value": {"kind": "ghost", "target": "Schemas:Customer"}}),
    );
    ok(
        &core,
        "card_create",
        json!({"value": {"Free:Aim": {"kind": "goal"}, "Free:Aim.Step": {"kind": "task"}}}),
    );
    ok(
        &core,
        "section_update",
        json!({"target": "Schemas", "value": {"places": {
            "Schemas/Ordering:Order": {"topLeft": {"x": 300, "y": 4}},
            "Schemas/Ordering": {"topLeft": {"x": 300, "y": 40}},
        }}}),
    );
    ok(&core, "branch_create", json!({"target": "work"}));
    core
}

/// Every kind of box the branch moves, moved in `work`: a card, another section's card and a
/// section ghost on a section's diagram, a card ghost, a writing card's size, a flow's node,
/// a card on a goal's card map — seven.
fn move_everything(core: &Core) {
    ok(
        core,
        "card_update",
        json!({"branch": "work", "target": "Schemas:Customer", "value": {"topLeft": {"x": 500, "y": 60}}}),
    );
    ok(
        core,
        "section_update",
        json!({"branch": "work", "target": "Schemas", "value": {"places": {
            "Schemas/Ordering:Order": {"topLeft": {"x": 600, "y": 4}},
            "Schemas/Ordering": {"topLeft": {"x": 600, "y": 40}, "size": {"w": 12, "h": 6}},
        }}}),
    );
    ok(
        core,
        "section_update",
        json!({"branch": "work", "target": "Schemas/Ordering", "value": {"places": {
            "Schemas/Ordering:Client": {"topLeft": {"x": 400, "y": 30}},
        }}}),
    );
    ok(
        core,
        "card_update",
        json!({"branch": "work", "target": "Free:Memo", "value": {"size": {"w": 12, "h": 6}}}),
    );
    ok(
        core,
        "card_update",
        json!({"branch": "work", "target": "Charts/Shop:Checkout.save", "value": {"topLeft": {"x": 30, "y": 30}}}),
    );
    ok(
        core,
        "card_update",
        json!({"branch": "work", "target": "Free:Aim", "value": {"places": {"Free:Aim.Step": {"topLeft": {"x": 40, "y": 20}}}}}),
    );
}

/// Where a tree places what `move_everything` moves.
fn arranged(core: &Core, branch: &str) -> Value {
    let boxes = |section: &str| {
        ok(
            core,
            "section_read",
            json!({"branch": branch, "target": section, "value": {"include": ["places"]}}),
        )["places"]
            .clone()
    };
    let save = ok(
        core,
        "card_read",
        json!({"branch": branch, "target": "Charts/Shop:Checkout.save"}),
    )["topLeft"]
        .clone();
    let map = ok(
        core,
        "card_read",
        json!({"branch": branch, "target": "Free:Aim", "value": {"include": ["places"]}}),
    )["places"]
        .clone();
    json!({
        "Schemas": boxes("Schemas"),
        "Ordering": boxes("Schemas/Ordering"),
        "Free": boxes("Free"),
        "save": save,
        "Aim": map,
    })
}

/// The box of `path` among a diagram's boxes.
fn box_at<'v>(boxes: &'v Value, path: &str) -> &'v Value {
    boxes
        .get(path)
        .unwrap_or_else(|| panic!("{path} が無い: {boxes}"))
}

/// The first line of a tree's newest commit.
fn last_line(core: &Core, branch: &str) -> String {
    ok(
        core,
        "branch_read",
        json!({"target": branch, "value": {"as": "history", "limit": 1}}),
    )["commits"][0]["lines"][0]
        .as_str()
        .unwrap()
        .to_string()
}

#[test]
fn the_boxes_a_branch_moved_go_into_main_only_when_asked() {
    let core = arranged_project();
    let before = arranged(&core, "main");
    move_everything(&core);
    let moved = arranged(&core, "work");
    assert_ne!(moved, before);

    // The preview says how many would change; nothing of the cards goes in.
    let preview = ok(
        &core,
        "branch_read",
        json!({"target": "work", "value": {"against": "main"}}),
    );
    assert_eq!(preview["places"], 7, "{preview}");
    assert_eq!(preview["nothing"], true, "{preview}");

    // Not asked: main keeps where it places them.
    // Kept: merged again below.
    let done = ok(
        &core,
        "branch_merge",
        json!({"target": "work", "value": {"keep": true}}),
    );
    assert!(done.get("places").is_none(), "{done}");
    // Told what was left out, and how to take it.
    assert_eq!(done["placesLeft"], 7, "{done}");
    assert!(
        done["hint"].as_str().unwrap().contains("places: true"),
        "{done}"
    );
    assert_eq!(arranged(&core, "main"), before);
    assert_eq!(arranged(&core, "work"), moved);

    // Asked: every one goes in over main's, and the branch lets go of its own, main now
    // holding them the same.
    let done = ok(
        &core,
        "branch_merge",
        json!({"target": "work", "value": {"places": true, "keep": true}}),
    );
    assert_eq!(done["places"], 7, "{done}");
    let main = arranged(&core, "main");
    assert_eq!(main, moved);
    let schemas = &main["Schemas"];
    assert_eq!(
        box_at(schemas, "Schemas:Customer")["topLeft"],
        json!({"x": 500, "y": 60})
    );
    assert_eq!(
        box_at(schemas, "Schemas/Ordering:Order")["topLeft"],
        json!({"x": 600, "y": 4})
    );
    assert_eq!(
        box_at(schemas, "Schemas/Ordering")["size"],
        json!({"w": 12, "h": 6})
    );
    assert_eq!(
        box_at(&main["Ordering"], "Schemas/Ordering:Client")["topLeft"],
        json!({"x": 400, "y": 30})
    );
    assert_eq!(
        box_at(&main["Free"], "Free:Memo")["size"],
        json!({"w": 12, "h": 6})
    );
    assert_eq!(main["save"], json!({"x": 30, "y": 30}));
    assert_eq!(
        box_at(&main["Aim"], "Free:Aim.Step")["topLeft"],
        json!({"x": 40, "y": 20})
    );
    assert_eq!(
        last_line(&core, "main"),
        "work を merge（足す 0・直す 0・消す 0・位置 7）"
    );
    let after = ok(
        &core,
        "branch_read",
        json!({"target": "work", "value": {"against": "main"}}),
    );
    assert!(after.get("places").is_none(), "{after}");
    // main moved on: the branch, holding none of its own, follows.
    ok(
        &core,
        "card_update",
        json!({"target": "Schemas:Customer", "value": {"topLeft": {"x": 700, "y": 90}}}),
    );
    assert_eq!(
        read(&core, "work", "Schemas:Customer")["topLeft"],
        json!({"x": 700, "y": 90})
    );
}

#[test]
fn a_merge_that_left_no_boxes_out_says_nothing_of_them() {
    let core = arranged_project();
    ok(
        &core,
        "card_update",
        json!({"branch": "work", "target": "Schemas:Customer", "value": {"description": "直した"}}),
    );
    let done = ok(
        &core,
        "branch_merge",
        json!({"target": "work", "value": {"keep": true}}),
    );
    assert_eq!(done["changed"], json!(["Schemas:Customer"]), "{done}");
    for key in ["places", "placesLeft", "hint"] {
        assert!(done.get(key).is_none(), "{key}: {done}");
    }
}

#[test]
fn the_boxes_go_over_what_the_target_put_and_with_the_cards_it_brings() {
    let core = arranged_project();
    // A card the branch made, with its place: it goes in where the branch put it either
    // way, so taking the boxes along changes nothing more — it is not counted.
    ok(
        &core,
        "card_create",
        json!({"branch": "work", "target": "Schemas:Voucher", "value": {"kind": "entity", "topLeft": {"x": 520, "y": 120}}}),
    );
    let preview = ok(
        &core,
        "branch_read",
        json!({"target": "work", "value": {"against": "main"}}),
    );
    assert_eq!(preview["add"], json!(["Schemas:Voucher"]), "{preview}");
    assert!(preview.get("places").is_none(), "{preview}");
    // main moved Customer since; the branch moves it too: that one is counted.
    ok(
        &core,
        "card_update",
        json!({"target": "Schemas:Customer", "value": {"topLeft": {"x": 200, "y": 90}}}),
    );
    ok(
        &core,
        "card_update",
        json!({"branch": "work", "target": "Schemas:Customer", "value": {"topLeft": {"x": 500, "y": 60}}}),
    );
    let preview = ok(
        &core,
        "branch_read",
        json!({"target": "work", "value": {"against": "main"}}),
    );
    assert_eq!(preview["places"], 1, "{preview}");
    let done = ok(
        &core,
        "branch_merge",
        json!({"target": "work", "value": {"places": true}}),
    );
    assert_eq!(done["added"], json!(["Schemas:Voucher"]));
    assert_eq!(done["places"], 1, "{done}");
    assert_eq!(
        last_line(&core, "main"),
        "work を merge（足す 1・直す 0・消す 0・位置 1）"
    );
    assert_eq!(
        read(&core, "main", "Schemas:Customer")["topLeft"],
        json!({"x": 500, "y": 60})
    );
    assert_eq!(
        read(&core, "main", "Schemas:Voucher")["topLeft"],
        json!({"x": 520, "y": 120})
    );
}

/// The window's request on the project (`op` and its fields): its result.
fn screen(core: &Core, mut request: Value) -> Value {
    let project: String = rusqlite::Connection::open(core.place())
        .unwrap()
        .query_row("SELECT id FROM projects WHERE name = ?", [PROJECT], |r| {
            r.get(0)
        })
        .unwrap();
    request["project"] = project.into();
    let answer: Value = serde_json::from_str(&core.call(&request.to_string())).unwrap();
    assert!(answer.get("error").is_none(), "{request}: {answer}");
    answer["result"].clone()
}

/// A branch's id by its name.
fn branch_id(core: &Core, name: &str) -> String {
    screen(core, json!({"op": "branches"}))["branches"]
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["name"] == name)
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string()
}

#[test]
fn between_branches_the_boxes_go_too_and_mains_box_taken_lets_the_targets_go() {
    let core = arranged_project();
    let main_at = read(&core, "main", "Schemas:Customer")["topLeft"].clone();
    ok(&core, "branch_create", json!({"target": "other"}));
    ok(
        &core,
        "card_update",
        json!({"branch": "other", "target": "Schemas:Customer", "value": {"topLeft": {"x": 600, "y": 80}}}),
    );
    // work moves Order, and Customer away and back where main has it, as the window puts
    // a box (MCP would move it off what it overlaps there).
    ok(
        &core,
        "card_update",
        json!({"branch": "work", "target": "Schemas/Ordering:Order", "value": {"topLeft": {"x": 640, "y": 120}}}),
    );
    ok(
        &core,
        "card_update",
        json!({"branch": "work", "target": "Schemas:Customer", "value": {"topLeft": {"x": 500, "y": 60}}}),
    );
    let customer = screen(&core, json!({"op": "project_detail"}))["schemas"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "Customer")
        .cloned()
        .unwrap();
    screen(
        &core,
        json!({"op": "places", "branch": branch_id(&core, "work"), "boxes": [{
            "item": customer["id"], "x": customer["posX"].as_f64().unwrap() as i64,
            "y": customer["posY"].as_f64().unwrap() as i64, "pinned": customer["pinned"]}]}),
    );
    assert_eq!(read(&core, "work", "Schemas:Customer")["topLeft"], main_at);
    let preview = ok(
        &core,
        "branch_read",
        json!({"target": "work", "value": {"against": "other"}}),
    );
    assert_eq!(preview["places"], 2, "{preview}");
    let done = ok(
        &core,
        "branch_merge",
        json!({"target": "work", "value": {"into": "other", "places": true}}),
    );
    assert_eq!(done["places"], 2, "{done}");
    assert_eq!(
        read(&core, "other", "Schemas/Ordering:Order")["topLeft"],
        json!({"x": 640, "y": 120})
    );
    // other holds main's again: main moves it, and other follows.
    assert_eq!(read(&core, "other", "Schemas:Customer")["topLeft"], main_at);
    ok(
        &core,
        "card_update",
        json!({"target": "Schemas:Customer", "value": {"topLeft": {"x": 700, "y": 90}}}),
    );
    assert_eq!(
        read(&core, "other", "Schemas:Customer")["topLeft"],
        json!({"x": 700, "y": 90})
    );
    // The source keeps its own between branches.
    assert_eq!(
        read(&core, "work", "Schemas/Ordering:Order")["topLeft"],
        json!({"x": 640, "y": 120})
    );
}

#[test]
fn main_taken_in_does_not_take_its_boxes_and_places_is_true_or_false() {
    let core = arranged_project();
    let said = refused(
        &core,
        "branch_merge",
        json!({"target": "main", "value": {"into": "work", "places": true}}),
    );
    assert!(
        said.contains(
            "places（branch で動かした位置も入れる）は、branch を merge するときだけです"
        ),
        "{said}"
    );
    let said = refused(
        &core,
        "branch_merge",
        json!({"target": "work", "value": {"places": "yes"}}),
    );
    assert!(
        said.contains("value の places は boolean で指定してください"),
        "{said}"
    );
    // Taken into the branch, main's boxes are not: the branch's stay where it put them.
    move_everything(&core);
    ok(
        &core,
        "card_update",
        json!({"target": "Free:Memo", "value": {"body": "main で直した"}}),
    );
    let kept = arranged(&core, "work");
    ok(
        &core,
        "branch_merge",
        json!({"target": "main", "value": {"into": "work", "places": false}}),
    );
    assert_eq!(arranged(&core, "work"), kept);
    let memo = ok(
        &core,
        "card_read",
        json!({"branch": "work", "target": "Free:Memo"}),
    );
    assert_eq!(memo["body"], "main で直した", "{memo}");
}

#[test]
fn the_window_merges_the_boxes_into_a_locked_main() {
    let core = with_branch("work");
    ok(
        &core,
        "card_update",
        json!({"branch": "work", "target": "Schemas:Customer", "value": {"topLeft": {"x": 500, "y": 60}}}),
    );
    screen(&core, json!({"op": "main_lock", "on": true}));
    let work = branch_id(&core, "work");
    let preview = screen(
        &core,
        json!({"op": "merge_preview", "source": work, "target": ""}),
    );
    assert_eq!(preview["places"], 1, "{preview}");
    let done = screen(
        &core,
        json!({"op": "merge", "source": work, "target": "", "places": true}),
    );
    assert_eq!(done["places"], 1, "{done}");
    assert_eq!(
        read(&core, "main", "Schemas:Customer")["topLeft"],
        json!({"x": 500, "y": 60})
    );
}

#[test]
fn the_window_says_a_conflicting_row_by_its_fields_and_merges_it_as_taken() {
    let core = with_branch("work");
    for (tree, body) in [("work", "work の本文"), ("main", "main の本文")] {
        ok(
            &core,
            "card_update",
            json!({"branch": tree, "target": "Docs:Spec.intro.body", "value": {"body": body}}),
        );
    }
    let work = branch_id(&core, "work");
    let preview = screen(
        &core,
        json!({"op": "merge_preview", "source": work, "target": ""}),
    );
    let conflict = &preview["conflicts"][0];
    assert_eq!(conflict["label"], "rows.intro.body", "{preview}");
    assert!(conflict["unit"].as_str().unwrap().starts_with("r:rows:"));
    assert_eq!(
        conflict["source"],
        json!({"name": "body", "kind": "text", "body": "work の本文"}),
        "{conflict}"
    );
    screen(
        &core,
        json!({"op": "merge", "source": work, "target": "",
            "take": {"Docs:Spec": {"rows.intro.body": "source"}}}),
    );
    assert_eq!(
        read(&core, "main", "Docs:Spec.intro.body")["body"],
        "work の本文"
    );
}
