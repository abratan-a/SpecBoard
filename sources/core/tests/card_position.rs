//! Where a card goes among its section's cards: `card_update`'s `position`, counting from zero
//! as the cards are listed (by `order`, then name) and leaving out the cards of the sections
//! inside, as `section_update`'s is for a section among its siblings. The section's cards are
//! given the `order`s they had, handed out again in the new arrangement; no other card's
//! changes. With `section`, it is a place among the cards of the section moved to. It is a
//! line of the history; a locked main takes it alone; what the project lets MCP write counts
//! it as `card_update`'s (`board`'s, in the board).

use crate::common;

use common::{mcp, op, project_id, script_store};
use serde_json::{Value, json};
use specboard_core::Core;
use std::collections::BTreeMap;
use std::path::Path;

const PROJECT: &str = "見本";

/// `card_update` of one card's position, and with it what else `value` says.
fn put(core: &Core, branch: &str, target: &str, value: Value) -> Result<Value, String> {
    mcp(
        core,
        "card_update",
        json!({"branch": branch, "target": target, "value": value}),
    )
}

/// The cards of a section as `card_list` lists them, by name.
fn listed(core: &Core, branch: &str, section: &str) -> Vec<String> {
    let read = mcp(
        core,
        "card_list",
        json!({"branch": branch, "value": {"section": section, "limit": 200}}),
    )
    .unwrap();
    read["cards"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| {
            let path = c["path"].as_str().unwrap();
            path.rsplit_once(':').unwrap().1.to_string()
        })
        .collect()
}

/// The cards of a section as the window reads the project (`project_detail`), by name.
fn detailed(core: &Core, section: &str) -> Vec<String> {
    let read = op(
        core,
        json!({"op": "project_detail", "project": project_id(core)}),
    );
    let section_id = read["result"]["sections"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == section)
        .unwrap()["id"]
        .clone();
    read["result"]["schemas"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["sectionId"] == section_id)
        .map(|c| c["name"].as_str().unwrap().to_string())
        .collect()
}

/// Every card's `order` in a tree as its document keeps it (none for 0), by name.
fn orders(place: &Path, branch: &str) -> BTreeMap<String, Option<i64>> {
    let conn = rusqlite::Connection::open(place).unwrap();
    let mut stmt = conn
        .prepare("SELECT name, json_extract(doc, '$.order') FROM card_docs WHERE branch_id = ? AND removed = 0")
        .unwrap();
    stmt.query_map([branch], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

/// The tree's commits, newest first: each commit's lines.
fn commits(core: &Core, branch: &str) -> Vec<Vec<String>> {
    let read = op(
        core,
        json!({"op": "commits", "project": project_id(core), "branch": branch}),
    );
    read["result"]["commits"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| {
            c["lines"]
                .as_array()
                .unwrap()
                .iter()
                .map(|l| l["summary"].as_str().unwrap().to_string())
                .collect()
        })
        .collect()
}

#[test]
fn a_card_goes_where_its_position_puts_it_among_its_sections_cards() {
    let core = Core::open(script_store()).unwrap();
    // Schemas holds four cards of its own; those of Schemas/Ordering are not among them.
    assert_eq!(
        listed(&core, "main", "Schemas"),
        ["Status", "Customer", "Orders", "Rule"]
    );
    let before = orders(core.place(), "");
    let commits_before = commits(&core, "").len();

    // To the front.
    put(&core, "main", "Schemas:Rule", json!({"position": 0})).unwrap();
    assert_eq!(
        listed(&core, "main", "Schemas"),
        ["Rule", "Status", "Customer", "Orders"]
    );
    assert_eq!(
        detailed(&core, "Schemas"),
        ["Rule", "Status", "Customer", "Orders"]
    );
    // The orders the section's cards had, handed out again; no other card's changes.
    let after = orders(core.place(), "");
    assert_eq!(
        ["Rule", "Status", "Customer", "Orders"].map(|n| after[n]),
        ["Status", "Customer", "Orders", "Rule"].map(|n| before[n])
    );
    let others = |all: &BTreeMap<String, Option<i64>>| -> BTreeMap<String, Option<i64>> {
        all.iter()
            .filter(|(n, _)| !["Rule", "Status", "Customer", "Orders"].contains(&n.as_str()))
            .map(|(n, o)| (n.clone(), *o))
            .collect()
    };
    assert_eq!(others(&after), others(&before));
    assert_eq!(listed(&core, "main", "Schemas/Ordering"), ["Order", "Line"]);

    // A line of the history, a commit a call.
    let lines = commits(&core, "");
    assert_eq!(lines.len(), commits_before + 1);
    assert_eq!(lines[0], ["カード Schemas:Rule を更新（並び順）"]);

    // Toward the back, by another card.
    put(&core, "main", "Schemas:Status", json!({"position": 2})).unwrap();
    assert_eq!(
        listed(&core, "main", "Schemas"),
        ["Rule", "Customer", "Status", "Orders"]
    );
    put(&core, "main", "Schemas:Rule", json!({"position": 3})).unwrap();
    assert_eq!(
        listed(&core, "main", "Schemas"),
        ["Customer", "Status", "Orders", "Rule"]
    );
    assert_eq!(others(&orders(core.place(), "")), others(&before));

    // Where it already is: nothing written, no commit.
    let count = commits(&core, "").len();
    put(&core, "main", "Schemas:Rule", json!({"position": 3})).unwrap();
    assert_eq!(commits(&core, "").len(), count);

    // Several targets, one after the other.
    mcp(
        &core,
        "card_update",
        json!({"value": {"Schemas:Orders": {"position": 0}, "Schemas:Rule": {"position": 1}}}),
    )
    .unwrap();
    assert_eq!(
        listed(&core, "main", "Schemas"),
        ["Orders", "Rule", "Customer", "Status"]
    );
}

#[test]
fn a_position_out_of_its_sections_cards_is_refused() {
    let core = Core::open(script_store()).unwrap();
    let before = orders(core.place(), "");
    for (at, said) in [
        (-1, "position は 0〜3 です（-1）"),
        (4, "position は 0〜3 です（4）"),
    ] {
        assert_eq!(
            put(&core, "main", "Schemas:Rule", json!({"position": at})).unwrap_err(),
            said
        );
    }
    // A section's only card has one place.
    assert_eq!(
        put(&core, "main", "Free:Memo", json!({"position": 1})).unwrap_err(),
        "position は 0〜0 です（1）"
    );
    // Not a whole number: as the form says.
    let refused = put(&core, "main", "Schemas:Rule", json!({"position": "first"})).unwrap_err();
    assert!(
        refused.contains("position は integer で指定してください"),
        "{refused}"
    );
    let refused = put(&core, "main", "Schemas:Rule", json!({"position": 1.5})).unwrap_err();
    assert!(
        refused.contains("position は integer で指定してください"),
        "{refused}"
    );
    // Refused with another target: nothing of the call is written.
    assert!(
        mcp(
            &core,
            "card_update",
            json!({"value": {"Schemas:Status": {"position": 3}, "Schemas:Rule": {"position": 9}}}),
        )
        .unwrap_err()
        .contains("Schemas:Rule: position は 0〜3 です（9）")
    );
    assert_eq!(orders(core.place(), ""), before);
    assert_eq!(
        listed(&core, "main", "Schemas"),
        ["Status", "Customer", "Orders", "Rule"]
    );
}

#[test]
fn moved_with_its_section_it_goes_there_among_the_cards_moved_to() {
    let core = Core::open(script_store()).unwrap();
    // Order (0), Line (1), and Rule (5) coming in: Rule put between them.
    put(
        &core,
        "main",
        "Schemas:Rule",
        json!({"section": "Schemas/Ordering", "position": 1}),
    )
    .unwrap();
    assert_eq!(
        listed(&core, "main", "Schemas/Ordering"),
        ["Order", "Rule", "Line"]
    );
    assert_eq!(
        listed(&core, "main", "Schemas"),
        ["Status", "Customer", "Orders"]
    );
    let lines = commits(&core, "");
    assert_eq!(
        lines[0],
        [
            "1 件をセクション Schemas/Ordering へ移動（Schemas:Rule）",
            "カード Schemas/Ordering:Rule を更新（並び順）"
        ]
    );
    // The range is that of the section moved to, with the card in it.
    assert_eq!(
        put(
            &core,
            "main",
            "Schemas:Status",
            json!({"section": "Schemas/Ordering", "position": 4}),
        )
        .unwrap_err(),
        "position は 0〜3 です（4）"
    );
    assert_eq!(listed(&core, "main", "Schemas/Ordering").len(), 3);
    // A move refused: only that is said, not a place in the section it stayed in.
    let refused = put(
        &core,
        "main",
        "Schemas:Status",
        json!({"section": "Charts/Shop", "position": 9}),
    )
    .unwrap_err();
    assert!(refused.contains("チャートセクションなので"), "{refused}");
    assert!(!refused.contains("position"), "{refused}");
}

#[test]
fn a_locked_main_takes_a_position_alone() {
    let core = Core::open(script_store()).unwrap();
    let project = project_id(&core);
    let locked = op(
        &core,
        json!({"op": "main_lock", "project": project, "on": true}),
    );
    assert!(locked.get("error").is_none(), "{locked}");

    put(&core, "main", "Schemas:Rule", json!({"position": 0})).unwrap();
    assert_eq!(
        listed(&core, "main", "Schemas"),
        ["Rule", "Status", "Customer", "Orders"]
    );
    assert_eq!(
        commits(&core, "")[0],
        ["カード Schemas:Rule を更新（並び順）"]
    );
    // The window's arranging goes the same way.
    let said = op(
        &core,
        json!({"op": "tool", "name": "card_update", "arguments": {"project": PROJECT, "target": "Schemas:Rule", "value": {"position": 1}}}),
    );
    assert!(said.get("error").is_none(), "{said}");
    assert_eq!(
        listed(&core, "main", "Schemas"),
        ["Status", "Rule", "Customer", "Orders"]
    );

    // With what the lock holds, the call is refused whole.
    let before = orders(core.place(), "");
    for value in [
        json!({"position": 0, "description": "書かない"}),
        json!({"position": 0, "section": "Schemas/Ordering"}),
    ] {
        let refused = put(&core, "main", "Schemas:Rule", value.clone()).unwrap_err();
        assert!(
            refused.starts_with("main はロック中です") && !refused.contains('\n'),
            "{value}: {refused}"
        );
    }
    assert_eq!(orders(core.place(), ""), before);
    // With what it lets through, both go.
    put(
        &core,
        "main",
        "Schemas:Rule",
        json!({"position": 0, "tags": ["中核"], "topLeft": {"x": 0, "y": 80}}),
    )
    .unwrap();
    assert_eq!(listed(&core, "main", "Schemas")[0], "Rule");

    let unlocked = op(
        &core,
        json!({"op": "main_lock", "project": project, "on": false}),
    );
    assert!(unlocked.get("error").is_none(), "{unlocked}");
}

#[test]
fn a_branch_writes_only_the_cards_whose_order_changes() {
    let core = Core::open(script_store()).unwrap();
    let (branch_id, name): (String, String) = rusqlite::Connection::open(core.place())
        .unwrap()
        .query_row(
            "SELECT id, name FROM branches WHERE project_id = ?",
            [project_id(&core)],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    // The branch's Schemas: main's four, and its own Refund (11).
    assert_eq!(
        listed(&core, &name, "Schemas"),
        ["Status", "Customer", "Orders", "Rule", "Refund"]
    );
    let own_before = orders(core.place(), &branch_id);
    put(&core, &name, "Schemas:Refund", json!({"position": 3})).unwrap();
    assert_eq!(
        listed(&core, &name, "Schemas"),
        ["Status", "Customer", "Orders", "Refund", "Rule"]
    );
    // Refund takes Rule's order and Rule Refund's: only Rule becomes the branch's copy.
    let own_after = orders(core.place(), &branch_id);
    let made: Vec<&String> = own_after
        .keys()
        .filter(|n| !own_before.contains_key(*n))
        .collect();
    assert_eq!(made, ["Rule"]);
    assert_eq!(own_after["Rule"], Some(11));
    assert_eq!(own_after["Refund"], Some(5));
    // main is as it was.
    assert_eq!(
        listed(&core, "main", "Schemas"),
        ["Status", "Customer", "Orders", "Rule"]
    );
}

#[test]
fn orders_shared_are_told_apart() {
    // Status and Customer made to share Status's order, before the store is opened.
    let place = script_store();
    {
        let conn = rusqlite::Connection::open(&place).unwrap();
        let status: i64 = conn
            .query_row(
                "SELECT json_extract(doc, '$.order') FROM card_docs WHERE branch_id = '' AND name = 'Status'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        conn.execute(
            "UPDATE card_docs SET doc = json_set(doc, '$.order', ?) WHERE branch_id = '' AND name = 'Customer'",
            [status],
        )
        .unwrap();
    }
    let core = Core::open(place).unwrap();
    let before = orders(core.place(), "");
    assert_eq!(before["Status"], before["Customer"]);
    let shared = before["Status"].unwrap();
    // Listed by name where they share one.
    assert_eq!(
        listed(&core, "main", "Schemas"),
        ["Customer", "Status", "Orders", "Rule"]
    );
    put(&core, "main", "Schemas:Status", json!({"position": 0})).unwrap();
    assert_eq!(
        listed(&core, "main", "Schemas"),
        ["Status", "Customer", "Orders", "Rule"]
    );
    // The one shared kept by the first, the next raised past it; those apart stay.
    let after = orders(core.place(), "");
    assert_eq!(after["Status"], Some(shared));
    assert_eq!(after["Customer"], Some(shared + 1));
    assert_eq!(after["Orders"], before["Orders"]);
    assert_eq!(after["Rule"], before["Rule"]);
}

#[test]
fn where_a_card_goes_among_its_sections_is_card_updates_to_write() {
    let core = Core::open(script_store()).unwrap();
    let project = project_id(&core);
    let set = |item: &str, state: &str| {
        let said = op(
            &core,
            json!({"op": "mcp_writes_set", "project": project, "item": item, "state": state}),
        );
        assert!(said.get("error").is_none(), "{said}");
    };
    let refusal = |item: &str| {
        format!(
            "このプロジェクトでは、{item} は MCP から使えません（拒否）。変えられるのは人だけで、アプリのプロジェクト設定の「MCP」のタブで許可か都度承認にします"
        )
    };
    // A tool, an item: where a card goes is card_update's, as the rest of it.
    set("card_update", "deny");
    assert_eq!(
        put(&core, "main", "Schemas:Rule", json!({"position": 0})).unwrap_err(),
        refusal("card_update（カードを変える）")
    );
    assert_eq!(
        put(
            &core,
            "main",
            "Schemas:Rule",
            json!({"section": "Schemas/Ordering", "position": 0})
        )
        .unwrap_err(),
        refusal("card_update（カードを変える）")
    );
    assert_eq!(
        listed(&core, "main", "Schemas"),
        ["Status", "Customer", "Orders", "Rule"]
    );
    // The window writes whatever it writes.
    let said = op(
        &core,
        json!({"op": "tool", "name": "card_update", "arguments": {"project": PROJECT, "target": "Schemas:Rule", "value": {"position": 0}}}),
    );
    assert!(said.get("error").is_none(), "{said}");
    assert_eq!(listed(&core, "main", "Schemas")[0], "Rule");
    set("card_update", "allow");
    put(&core, "main", "Schemas:Rule", json!({"position": 3})).unwrap();
    assert_eq!(listed(&core, "main", "Schemas")[3], "Rule");

    // In the board, the board's alone: a goal's cards inside it, listed after it, put among
    // the cards beside them there.
    assert_eq!(
        listed(&core, "_project_board_", "Goals"),
        ["Ship", "Ship.Design", "Ship.t2"]
    );
    set("board", "deny");
    assert_eq!(
        put(
            &core,
            "_project_board_",
            "Goals:Ship.t2",
            json!({"position": 0})
        )
        .unwrap_err(),
        refusal("board（ボードに書く）")
    );
    set("board", "allow");
    set("card_update", "deny");
    put(
        &core,
        "_project_board_",
        "Goals:Ship.t2",
        json!({"position": 0}),
    )
    .unwrap();
    assert_eq!(
        listed(&core, "_project_board_", "Goals"),
        ["Ship", "Ship.t2", "Ship.Design"]
    );
}
