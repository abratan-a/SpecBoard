//! A card or section deleted and made again with the same name, merged: it goes in as a
//! replacement of the old one, whichever way the merge goes and however it is asked for. The
//! target's rows are written out of each other's way, so names that change hands meet nowhere;
//! a write refused all the same is said by its path, never as the database says it.

use crate::common;

use common::{ok, op, refused, script_store};
use serde_json::{Value, json};
use specboard_core::Core;

const PROJECT: &str = "見本";

/// Enough cards that their new ids fall on both sides of the old ones.
const NAMES: [&str; 6] = ["Alpha", "Bravo", "Charlie", "Delta", "Echo", "Foxtrot"];

fn read(core: &Core, branch: &str, path: &str) -> Value {
    ok(core, "card_read", json!({"branch": branch, "target": path}))
}

fn project_id(core: &Core) -> String {
    rusqlite::Connection::open(core.place())
        .unwrap()
        .query_row("SELECT id FROM projects WHERE name = ?", [PROJECT], |r| {
            r.get(0)
        })
        .unwrap()
}

/// The id main or a branch holds a card under (its own row, or main's beneath a branch).
fn card_id(core: &Core, branch: &str, section: &str, name: &str) -> Option<String> {
    let conn = rusqlite::Connection::open(core.place()).unwrap();
    let tree: String = if branch == "main" {
        String::new()
    } else {
        conn.query_row("SELECT id FROM branches WHERE name = ?", [branch], |r| {
            r.get(0)
        })
        .unwrap()
    };
    let section_id: String = conn
        .query_row(
            "SELECT id FROM section_docs WHERE branch_id = '' AND name = ? AND removed = 0",
            [section],
            |r| r.get(0),
        )
        .unwrap();
    let own: Option<(String, i64)> = conn
        .query_row(
            "SELECT id, removed FROM card_docs WHERE branch_id = ? AND section_id = ? AND name = ? AND removed = 0",
            rusqlite::params![tree, section_id, name],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .ok();
    if let Some((id, _)) = own {
        return Some(id);
    }
    conn.query_row(
        "SELECT id FROM card_docs d WHERE branch_id = '' AND section_id = ? AND name = ? AND removed = 0
           AND NOT EXISTS (SELECT 1 FROM card_docs o WHERE o.branch_id = ? AND o.id = d.id)",
        rusqlite::params![section_id, name, tree],
        |r| r.get(0),
    )
    .ok()
}

/// main with the cards `NAMES` in `Schemas`, and a branch `work`.
fn with_cards() -> Core {
    let core = Core::open(script_store()).unwrap();
    for name in NAMES {
        ok(
            &core,
            "card_create",
            json!({"target": format!("Schemas:{name}"), "value": {"kind": "entity", "description": "元のカード"}}),
        );
    }
    ok(&core, "branch_create", json!({"target": "work"}));
    core
}

/// Every card of `NAMES` deleted on `tree` and made again there, told apart by its description.
fn make_again(core: &Core, tree: &str) {
    for name in NAMES {
        let path = format!("Schemas:{name}");
        ok(
            core,
            "card_delete",
            json!({"branch": tree, "target": path.clone()}),
        );
        ok(
            core,
            "card_create",
            json!({"branch": tree, "target": path, "value": {"kind": "value", "description": "作り直した"}}),
        );
    }
}

fn assert_made_again(core: &Core, tree: &str, old: &[Option<String>]) {
    for (name, was) in NAMES.iter().zip(old) {
        let card = read(core, tree, &format!("Schemas:{name}"));
        assert_eq!(card["description"], "作り直した", "{name}: {card}");
        assert_eq!(card["kind"], "value", "{name}: {card}");
        assert_ne!(
            &card_id(core, tree, "Schemas", name),
            was,
            "{name} is the card made again"
        );
    }
}

fn old_ids(core: &Core) -> Vec<Option<String>> {
    NAMES
        .iter()
        .map(|n| card_id(core, "main", "Schemas", n))
        .collect()
}

#[test]
fn cards_deleted_and_made_again_on_a_branch_go_into_main_in_place_of_the_old() {
    let core = with_cards();
    let old = old_ids(&core);
    make_again(&core, "work");
    let preview = ok(
        &core,
        "branch_read",
        json!({"target": "work", "value": {"against": "main"}}),
    );
    assert!(preview.get("overlaps").is_none(), "{preview}");
    assert!(preview.get("conflicts").is_none(), "{preview}");
    ok(&core, "branch_merge", json!({"target": "work"}));
    assert_made_again(&core, "main", &old);
}

#[test]
fn the_window_merges_cards_made_again_and_counts_no_path_twice() {
    let core = with_cards();
    let old = old_ids(&core);
    make_again(&core, "work");
    let project = project_id(&core);
    let work: String = rusqlite::Connection::open(core.place())
        .unwrap()
        .query_row("SELECT id FROM branches WHERE name = 'work'", [], |r| {
            r.get(0)
        })
        .unwrap();
    let preview = op(
        &core,
        json!({"op": "merge_preview", "project": project, "source": work, "target": ""}),
    );
    assert!(preview.get("error").is_none(), "{preview}");
    let overlaps = &preview["result"]["overlaps"];
    assert!(overlaps.is_null() || overlaps == &json!([]), "{preview}");
    let done = op(
        &core,
        json!({"op": "merge", "project": project, "source": work, "target": "", "places": true}),
    );
    assert!(done.get("error").is_none(), "{done}");
    assert_made_again(&core, "main", &old);
}

#[test]
fn an_approved_request_brings_cards_made_again_into_a_locked_main() {
    let core = with_cards();
    let old = old_ids(&core);
    make_again(&core, "work");
    let project = project_id(&core);
    let locked = op(
        &core,
        json!({"op": "main_lock", "project": project, "on": true}),
    );
    assert!(locked.get("error").is_none(), "{locked}");
    let asked = ok(&core, "branch_merge", json!({"target": "work"}));
    assert_eq!(asked["mr"], 1, "{asked}");
    let done = op(
        &core,
        json!({"op": "merge_request_approve", "project": project, "number": 1}),
    );
    assert!(done.get("error").is_none(), "{done}");
    assert_made_again(&core, "main", &old);
}

#[test]
fn main_taken_into_a_branch_brings_the_cards_main_made_again() {
    let core = with_cards();
    let old = old_ids(&core);
    // The branch holds rows of its own for the old cards, so the merge writes over them.
    for name in NAMES {
        ok(
            &core,
            "card_update",
            json!({"branch": "work", "target": format!("Schemas:{name}"), "value": {"label": "branch の名"}}),
        );
    }
    make_again(&core, "main");
    let preview = ok(
        &core,
        "branch_read",
        json!({"target": "main", "value": {"against": "work"}}),
    );
    assert!(preview.get("overlaps").is_none(), "{preview}");
    let take: serde_json::Map<String, Value> = NAMES
        .iter()
        .map(|n| (format!("Schemas:{n}"), json!({"(whole)": "source"})))
        .collect();
    ok(
        &core,
        "branch_merge",
        json!({"target": "main", "value": {"into": "work", "take": take}}),
    );
    assert_made_again(&core, "work", &old);
}

#[test]
fn a_branch_merged_into_a_branch_that_changed_the_old_cards_replaces_them_when_taken() {
    let core = with_cards();
    let old = old_ids(&core);
    ok(&core, "branch_create", json!({"target": "other"}));
    for name in NAMES {
        ok(
            &core,
            "card_update",
            json!({"branch": "other", "target": format!("Schemas:{name}"), "value": {"label": "other の名"}}),
        );
    }
    make_again(&core, "work");
    let take: serde_json::Map<String, Value> = NAMES
        .iter()
        .map(|n| (format!("Schemas:{n}"), json!({"(whole)": "source"})))
        .collect();
    let preview = ok(
        &core,
        "branch_read",
        json!({"target": "work", "value": {"against": "other"}}),
    );
    assert!(preview.get("overlaps").is_none(), "{preview}");
    ok(
        &core,
        "branch_merge",
        json!({"target": "work", "value": {"into": "other", "take": take}}),
    );
    assert_made_again(&core, "other", &old);
}

#[test]
fn a_card_made_again_where_main_changed_the_old_one_is_the_choice_of_the_whole() {
    let core = with_cards();
    make_again(&core, "work");
    ok(
        &core,
        "card_update",
        json!({"target": "Schemas:Alpha", "value": {"label": "main で直した"}}),
    );
    let preview = ok(
        &core,
        "branch_read",
        json!({"target": "work", "value": {"against": "main"}}),
    );
    let conflicts = preview["conflicts"].as_array().unwrap();
    assert_eq!(conflicts.len(), 1, "{preview}");
    assert_eq!(conflicts[0]["unit"], "(whole)");
    assert!(conflicts[0]["source"].is_null(), "{preview}");
    // Not chosen yet: whether two cards meet at the path depends on the choice.
    assert!(preview.get("overlaps").is_none(), "{preview}");

    // Kept: main's card and the one made again would share the path.
    let keep = json!({"Schemas:Alpha": {"(whole)": "target"}});
    let said = refused(
        &core,
        "branch_merge",
        json!({"target": "work", "value": {"take": keep}}),
    );
    assert!(said.contains("名前を変えるか消してから"), "{said}");
    assert_eq!(
        read(&core, "main", "Schemas:Alpha")["label"],
        "main で直した"
    );

    // The whole by the Japanese name a request kept before took it: taken the same.
    ok(
        &core,
        "branch_merge",
        json!({"target": "work", "value": {"take": {"Schemas:Alpha": {"（カード）": "source"}}}}),
    );
    let alpha = read(&core, "main", "Schemas:Alpha");
    assert_eq!(alpha["description"], "作り直した", "{alpha}");
    assert!(alpha["label"].is_null(), "{alpha}");
}

#[test]
fn names_two_cards_trade_on_a_branch_go_into_main() {
    let core = with_cards();
    let (alpha, bravo) = (
        card_id(&core, "main", "Schemas", "Alpha"),
        card_id(&core, "main", "Schemas", "Bravo"),
    );
    for (from, to) in [("Alpha", "Tmp"), ("Bravo", "Alpha"), ("Tmp", "Bravo")] {
        ok(
            &core,
            "card_update",
            json!({"branch": "work", "target": format!("Schemas:{from}"), "value": {"name": to}}),
        );
    }
    ok(&core, "branch_merge", json!({"target": "work"}));
    assert_eq!(card_id(&core, "main", "Schemas", "Alpha"), bravo);
    assert_eq!(card_id(&core, "main", "Schemas", "Bravo"), alpha);
}

#[test]
fn sections_deleted_and_made_again_on_a_branch_go_into_main_in_place_of_the_old() {
    let core = Core::open(script_store()).unwrap();
    for name in NAMES {
        ok(
            &core,
            "section_create",
            json!({"target": format!("Schemas/{name}")}),
        );
        ok(
            &core,
            "card_create",
            json!({"target": format!("Schemas/{name}:Old"), "value": {"kind": "entity"}}),
        );
    }
    ok(&core, "branch_create", json!({"target": "work"}));
    for name in NAMES {
        ok(
            &core,
            "card_delete",
            json!({"branch": "work", "target": format!("Schemas/{name}:Old")}),
        );
        ok(
            &core,
            "section_delete",
            json!({"branch": "work", "target": format!("Schemas/{name}")}),
        );
        ok(
            &core,
            "section_create",
            json!({"branch": "work", "target": format!("Schemas/{name}")}),
        );
        ok(
            &core,
            "card_create",
            json!({"branch": "work", "target": format!("Schemas/{name}:New"), "value": {"kind": "value"}}),
        );
    }
    let preview = ok(
        &core,
        "branch_read",
        json!({"target": "work", "value": {"against": "main"}}),
    );
    assert!(preview.get("overlaps").is_none(), "{preview}");
    ok(&core, "branch_merge", json!({"target": "work"}));
    for name in NAMES {
        assert_eq!(
            read(&core, "main", &format!("Schemas/{name}:New"))["kind"],
            "value"
        );
        assert_eq!(card_id(&core, "main", name, "Old"), None, "{name}");
    }
}

#[test]
fn two_new_cards_of_one_name_are_refused_whatever_the_case_of_their_names() {
    let core = with_cards();
    ok(
        &core,
        "card_create",
        json!({"branch": "work", "target": "Schemas:Voucher", "value": {"kind": "entity"}}),
    );
    ok(
        &core,
        "card_create",
        json!({"target": "Schemas:voucher", "value": {"kind": "value"}}),
    );
    let preview = ok(
        &core,
        "branch_read",
        json!({"target": "work", "value": {"against": "main"}}),
    );
    // Named by either card's path: they differ only in case.
    let overlaps = preview["overlaps"].as_array().unwrap();
    assert_eq!(overlaps.len(), 1, "{preview}");
    assert!(
        overlaps[0]
            .as_str()
            .unwrap()
            .eq_ignore_ascii_case("Schemas:Voucher"),
        "{preview}"
    );
    let said = refused(&core, "branch_merge", json!({"target": "work"}));
    assert!(said.contains("名前を変えるか消してから"), "{said}");
    assert_eq!(read(&core, "main", "Schemas:voucher")["kind"], "value");
}

#[test]
fn a_write_the_database_refuses_in_a_merge_is_said_by_its_path_and_nothing_is_written() {
    let core = with_cards();
    ok(
        &core,
        "card_create",
        json!({"branch": "work", "target": "Schemas:Voucher", "value": {"kind": "entity"}}),
    );
    ok(
        &core,
        "card_update",
        json!({"branch": "work", "target": "Schemas:Alpha", "value": {"label": "branch の名"}}),
    );
    // A row in main that holds the name where no card of main says so: the merge cannot see it
    // coming, and only the database refuses the write.
    let conn = rusqlite::Connection::open(core.place()).unwrap();
    conn.execute(
        "INSERT INTO card_docs (project_id, branch_id, id, section_id, name, kind, removed, doc)
         SELECT project_id, '', 'hidden', section_id, 'Voucher', 'entity', 0, NULL
         FROM card_docs WHERE branch_id = '' AND name = 'Alpha'",
        [],
    )
    .unwrap();
    drop(conn);
    let said = refused(&core, "branch_merge", json!({"target": "work"}));
    assert!(!said.contains("UNIQUE"), "{said}");
    assert!(!said.contains("constraint"), "{said}");
    assert!(said.contains("Schemas:Voucher"), "{said}");
    assert!(read(&core, "main", "Schemas:Alpha")["label"].is_null());
}
