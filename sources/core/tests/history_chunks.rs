//! What a commit keeps of a long row, kept in pieces (`specboard://SpecBoard/Docs/Design:DesignBranches`
//! §11): a long document changed a little adds only what changed to the history, every commit still
//! reads what it kept to the letter, pieces no commit points at any more go, and the sizes count a
//! piece once.

use crate::common;

use common::{PROJECT, ok, op, project_id, script_store};
use rusqlite::Connection;
use serde_json::{Value, json};
use specboard_core::Core;
use std::collections::HashSet;

/// The lines of a document of about 250 KB: each differs, and holds letters of several bytes, so a
/// piece's edge falls inside one.
fn long_lines(word: &str) -> Vec<String> {
    (1..=4000)
        .map(|n| {
            format!(
                "{n} 行目の{word}。札 {} の説明を足して長くする 📎",
                n * 7919 % 1000
            )
        })
        .collect()
}

/// A tool called as the window calls it: its answer.
fn window(core: &Core, tool: &str, mut arguments: Value) -> Value {
    arguments["project"] = PROJECT.into();
    let said = op(
        core,
        json!({"op": "tool", "name": tool, "arguments": arguments}),
    );
    assert!(said.get("error").is_none(), "{tool}: {said}");
    said["result"]["json"].clone()
}

/// A request of the window's on the sample's project: its answer.
fn screen(core: &Core, mut request: Value) -> Value {
    request["project"] = project_id(core).into();
    let said = op(core, request.clone());
    assert!(said.get("error").is_none(), "{request}: {said}");
    said["result"].clone()
}

fn seq_of(core: &Core, tree: &str) -> i64 {
    ok(
        core,
        "branch_read",
        json!({"target": tree, "value": {"as": "history", "limit": 1}}),
    )["commits"][0]["seq"]
        .as_i64()
        .unwrap()
}

/// The note `Docs:<name>` made with `lines` in `tree`, then changed one line at a time `times` times:
/// the body after each write (the first as made), and the commit of each change.
fn written(
    core: &Core,
    tree: &str,
    name: &str,
    lines: &mut [String],
    times: usize,
) -> (Vec<String>, Vec<i64>) {
    let target = format!("Docs:{name}");
    ok(
        core,
        "card_create",
        json!({"branch": tree, "target": target, "value": {"kind": "note", "body": lines.join("\n")}}),
    );
    let mut bodies = vec![lines.join("\n")];
    let mut commits = Vec::new();
    for round in 0..times {
        changed(core, tree, name, lines, round);
        bodies.push(lines.join("\n"));
        commits.push(seq_of(core, tree));
    }
    (bodies, commits)
}

/// One more line of the note changed, the `round`th time.
fn changed(core: &Core, tree: &str, name: &str, lines: &mut [String], round: usize) {
    let at = 100 + round * 190;
    lines[at] = format!("{} 行目を直した（{} 回目）", at + 1, round + 1);
    ok(
        core,
        "card_update",
        json!({"branch": tree, "target": format!("Docs:{name}"), "value": {"body": lines.join("\n")}}),
    );
}

/// What the commit `seq` of `tree` kept of the body it changed, and what the body was after it.
fn around(core: &Core, tree: &str, seq: i64) -> (Value, Value) {
    let read = ok(
        core,
        "branch_read",
        json!({"target": tree, "value": {"as": "history", "seq": seq, "full": true}}),
    );
    let unit = &read["changes"][0]["units"][0];
    assert_eq!(unit["unit"], "body", "{read}");
    (unit["before"].clone(), unit["after"].clone())
}

/// The body of the note `Docs:<name>` as the tree holds it, whole: read from the store, as MCP
/// answers a long body in part.
fn body(core: &Core, tree: &str, name: &str) -> String {
    let conn = Connection::open(core.place()).unwrap();
    let tree = match tree {
        "main" => String::new(),
        name => branch_id(&conn, name),
    };
    conn.query_row(
        "SELECT json_extract(doc, '$.fields.body') FROM card_docs WHERE branch_id = ? AND name = ? AND removed = 0",
        [tree.as_str(), name],
        |r| r.get(0),
    )
    .unwrap()
}

/// How each history row of a tree's card documents is kept, oldest first: SQLite's name of the
/// value's type, `null` where the row was not there before.
fn kept_as(conn: &Connection, tree: &str) -> Vec<String> {
    let mut stmt = conn
        .prepare(
            "SELECT typeof(r.before) FROM commit_rows r JOIN commits c ON c.id = r.commit_id
              WHERE c.branch_id = ? AND r.tbl = 'card_docs' ORDER BY c.seq, r.rowid",
        )
        .unwrap();
    stmt.query_map([tree], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

fn branch_id(conn: &Connection, name: &str) -> String {
    conn.query_row("SELECT id FROM branches WHERE name = ?", [name], |r| {
        r.get(0)
    })
    .unwrap()
}

fn count(conn: &Connection, query: &str) -> i64 {
    conn.query_row(query, [], |r| r.get(0)).unwrap()
}

/// The numbers of the pieces the history rows of the store point at.
fn pointed_at(conn: &Connection) -> HashSet<i64> {
    let mut stmt = conn
        .prepare("SELECT before FROM commit_rows WHERE typeof(before) = 'blob'")
        .unwrap();
    let lists: Vec<Vec<u8>> = stmt
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    lists
        .iter()
        .flat_map(|list| common::pieces(list))
        .map(|(id, _)| id)
        .collect()
}

/// The numbers of the pieces the store holds.
fn held(conn: &Connection) -> HashSet<i64> {
    let mut stmt = conn.prepare("SELECT id FROM history_chunks").unwrap();
    stmt.query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

/// What the project's history takes, as the window is told: the trees' numbers added up.
fn history_size(core: &Core) -> i64 {
    screen(core, json!({"op": "project_size"}))["branches"]
        .as_array()
        .unwrap()
        .iter()
        .map(|tree| tree["history"].as_i64().unwrap())
        .sum()
}

#[test]
fn a_long_document_changed_a_little_adds_only_what_changed_to_the_history() {
    let core = Core::open(script_store()).unwrap();
    let mut lines = long_lines("文");
    assert!(lines.join("\n").len() > 200_000);
    let (bodies, commits) = written(&core, "main", "Long", &mut lines, 20);

    let conn = Connection::open(core.place()).unwrap();
    let kept = kept_as(&conn, "");
    assert_eq!(
        kept[kept.len() - 20..],
        vec!["blob"; 20],
        "長い行は並びで持つ"
    );
    let size = history_size(&core);
    assert!(size < 400_000, "履歴 {size} バイト");
    for (i, seq) in commits.iter().enumerate() {
        let (before, after) = around(&core, "main", *seq);
        assert_eq!(before, bodies[i].as_str(), "コミット {seq} の変える前");
        assert_eq!(after, bodies[i + 1].as_str(), "コミット {seq} のあと");
    }
}

#[test]
fn a_short_row_stays_as_its_text() {
    let core = Core::open(script_store()).unwrap();
    ok(
        &core,
        "card_update",
        json!({"target": "Schemas:Customer", "value": {"description": "短い直し"}}),
    );
    let conn = Connection::open(core.place()).unwrap();
    assert_eq!(kept_as(&conn, "").last().map(String::as_str), Some("text"));
    assert_eq!(count(&conn, "SELECT count(*) FROM history_chunks"), 0);
}

#[test]
fn going_back_puts_back_a_long_document_as_it_was() {
    let core = Core::open(script_store()).unwrap();
    for tree in ["back", "undone"] {
        window(&core, "branch_create", json!({"target": tree}));
    }
    let (bodies, commits) = written(&core, "back", "Long", &mut long_lines("文"), 3);
    let conn = Connection::open(core.place()).unwrap();
    assert_eq!(
        kept_as(&conn, &branch_id(&conn, "back")),
        ["null", "blob", "blob", "blob"],
        "長い行は並びで持つ"
    );
    ok(
        &core,
        "branch_revert",
        json!({"target": "back", "value": {"to": commits[0]}}),
    );
    assert_eq!(body(&core, "back", "Long"), bodies[1]);

    let (bodies, commits) = written(&core, "undone", "Long", &mut long_lines("文"), 3);
    ok(
        &core,
        "branch_revert",
        json!({"target": "undone", "value": {"undo": commits[2]}}),
    );
    assert_eq!(body(&core, "undone", "Long"), bodies[2]);
}

#[test]
fn pieces_no_row_points_at_go_with_a_cut_and_with_a_deleted_branch() {
    let core = Core::open(script_store()).unwrap();
    let (bodies, commits) = written(&core, "main", "Long", &mut long_lines("文"), 5);
    window(&core, "branch_create", json!({"target": "work"}));
    written(&core, "work", "Other", &mut long_lines("別の話"), 2);

    let conn = Connection::open(core.place()).unwrap();
    let before = held(&conn);
    assert_eq!(pointed_at(&conn), before);

    let last = *commits.last().unwrap();
    screen(&core, json!({"op": "cut", "before": last}));
    let after_cut = held(&conn);
    assert!(after_cut.len() < before.len(), "切っても塊が減らない");
    assert_eq!(pointed_at(&conn), after_cut);
    let (kept, _) = around(&core, "main", last);
    assert_eq!(kept, bodies[4].as_str());

    window(
        &core,
        "branch_delete",
        json!({"target": "work", "value": {"discard": true}}),
    );
    let after_delete = held(&conn);
    assert!(after_delete.len() < after_cut.len(), "消しても塊が減らない");
    assert_eq!(pointed_at(&conn), after_delete);
    let (kept, _) = around(&core, "main", last);
    assert_eq!(kept, bodies[4].as_str());
}

#[test]
fn a_piece_is_counted_once_and_a_cut_frees_what_it_says() {
    let core = Core::open(script_store()).unwrap();
    let mut lines = long_lines("文");
    let (_, commits) = written(&core, "main", "Long", &mut lines, 5);
    window(&core, "branch_create", json!({"target": "work"}));
    // The first write of main's card in a branch makes its copy; the second keeps the copy.
    for round in 5..7 {
        changed(&core, "work", "Long", &mut lines, round);
    }

    let conn = Connection::open(core.place()).unwrap();
    // The board's history is not in what the window is told.
    let own = count(
        &conn,
        "SELECT COALESCE(SUM(octet_length(r.before)), 0) FROM commit_rows r JOIN commits c ON c.id = r.commit_id
          WHERE c.branch_id <> '~board'",
    ) + count(
        &conn,
        "SELECT COALESCE(SUM(octet_length(l.summary)), 0) FROM commit_lines l JOIN commits c ON c.id = l.commit_id
          WHERE c.branch_id <> '~board'",
    );
    let pieces = count(
        &conn,
        "SELECT COALESCE(SUM(octet_length(data)), 0) FROM history_chunks",
    );
    assert!(pieces > 200_000, "塊 {pieces} バイト");
    let size = screen(&core, json!({"op": "project_size"}));
    let whole = history_size(&core);
    assert_eq!(whole, own + pieces);
    let work = size["branches"]
        .as_array()
        .unwrap()
        .iter()
        .find(|tree| tree["name"] == "work")
        .unwrap()["history"]
        .as_i64()
        .unwrap();
    assert!(work < 100_000, "branch の履歴 {work} バイト");

    let last = *commits.last().unwrap();
    let cost = screen(&core, json!({"op": "cut_cost", "before": last}));
    let bytes = cost["bytes"].as_i64().unwrap();
    assert!(bytes > 0);
    screen(&core, json!({"op": "cut", "before": last}));
    assert_eq!(history_size(&core), whole - bytes);
}

#[test]
fn a_project_moved_to_another_store_keeps_its_long_history() {
    let here = Core::open(script_store()).unwrap();
    let there = Core::create(common::place("chunks-there.sqlite")).unwrap();
    let (bodies, commits) = written(&here, "main", "Long", &mut long_lines("文"), 3);

    let bundle = screen(&here, json!({"op": "project_bundle"}));
    for row in bundle["tables"]["commit_rows"].as_array().unwrap() {
        let before = &row["before"];
        assert!(
            before.is_null() || before.as_str().is_some_and(|text| text.starts_with('{')),
            "束の変える前は文: {}",
            before.to_string().chars().take(80).collect::<String>()
        );
    }
    let read = op(
        &there,
        json!({"op": "project_bundle_read", "bundle": bundle}),
    );
    assert!(read.get("error").is_none(), "{read}");

    for (i, seq) in commits.iter().enumerate() {
        let (before, after) = around(&there, "main", *seq);
        assert_eq!(before, bodies[i].as_str(), "コミット {seq} の変える前");
        assert_eq!(after, bodies[i + 1].as_str(), "コミット {seq} のあと");
    }
    let conn = Connection::open(there.place()).unwrap();
    let kept = kept_as(&conn, "");
    assert_eq!(
        kept[kept.len() - 3..],
        ["blob"; 3],
        "移した先でも並びで持つ"
    );
    assert_eq!(pointed_at(&conn), held(&conn));
    assert!(!held(&conn).is_empty());
}

#[test]
fn a_cut_frees_what_it_says_though_a_branch_set_aside_keeps_the_same_pieces() {
    let core = Core::open(script_store()).unwrap();
    let mut lines = long_lines("文");
    written(&core, "main", "Long", &mut lines, 2);
    // A branch of main as it is now: what it keeps of the card is of this body.
    window(&core, "branch_create", json!({"target": "work"}));
    let mut theirs = lines.clone();
    for round in 2..5 {
        changed(&core, "main", "Long", &mut lines, round);
    }
    for round in 10..12 {
        changed(&core, "work", "Long", &mut theirs, round);
    }
    // Merged, the branch is set aside with its history: the window is told of main's alone.
    window(
        &core,
        "branch_merge",
        json!({"target": "work", "value": {"take": {"Docs:Long": {"body": "source"}}}}),
    );
    let merge = seq_of(&core, "main");
    let conn = Connection::open(core.place()).unwrap();
    assert_eq!(
        count(
            &conn,
            "SELECT count(*) FROM branches WHERE archived_at IS NOT NULL"
        ),
        1,
        "退けた branch"
    );

    let whole = history_size(&core);
    let cost = screen(&core, json!({"op": "cut_cost", "before": merge}));
    let bytes = cost["bytes"].as_i64().unwrap();
    assert!(bytes > 0);
    screen(&core, json!({"op": "cut", "before": merge}));
    assert_eq!(
        count(
            &conn,
            "SELECT count(*) FROM branches WHERE archived_at IS NOT NULL"
        ),
        1,
        "merge のコミットは残したので、退けた branch も残る"
    );
    assert_eq!(history_size(&core), whole - bytes);
}

#[test]
fn a_card_that_is_gone_is_named_by_the_long_row_its_history_kept() {
    let core = Core::open(script_store()).unwrap();
    written(&core, "main", "Long", &mut long_lines("文"), 0);
    ok(
        &core,
        "card_create",
        json!({"target": "Docs:Pointer", "value": {"kind": "note", "body": "指す",
               "connections": [{"line": "link", "to": "Docs:Long"}]}}),
    );
    ok(&core, "card_delete", json!({"target": "Docs:Long"}));
    let conn = Connection::open(core.place()).unwrap();
    assert_eq!(kept_as(&conn, "").last().map(String::as_str), Some("blob"));

    let broken = common::checked(&core, "main")["brokenPaths"].clone();
    let line = broken
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["at"] == "Docs:Pointer")
        .unwrap_or_else(|| panic!("{broken}"));
    assert_eq!(line["path"], "Docs:Long", "{broken}");
}

#[test]
fn a_frozen_copy_of_a_long_card_comes_back_with_a_revert() {
    let core = Core::open(script_store()).unwrap();
    let mut lines = long_lines("文");
    let (bodies, _) = written(&core, "main", "Long", &mut lines, 0);
    window(&core, "branch_create", json!({"target": "work"}));
    ok(
        &core,
        "card_update",
        json!({"branch": "work", "target": "Schemas:Customer", "value": {"description": "branch の直し"}}),
    );
    let first = seq_of(&core, "work");
    // Main changes the card: the branch goes on seeing it as it was, by a copy frozen for it.
    changed(&core, "main", "Long", &mut lines, 0);
    let conn = Connection::open(core.place()).unwrap();
    let work = branch_id(&conn, "work");
    let frozen = |conn: &Connection| -> Option<String> {
        conn.query_row(
            "SELECT json_extract(f.doc, '$.fields.body') FROM branch_frozen f JOIN card_docs c ON c.id = f.id
              WHERE f.branch_id = ? AND c.branch_id = '' AND c.name = 'Long'",
            [work.as_str()],
            |r| r.get(0),
        )
        .ok()
    };
    assert_eq!(frozen(&conn).as_deref(), Some(bodies[0].as_str()));

    // Taking main in lets the copy go, kept by the commit that did.
    window(
        &core,
        "branch_merge",
        json!({"target": "main", "value": {"into": "work"}}),
    );
    assert_eq!(frozen(&conn), None);
    let kept: String = conn
        .query_row(
            "SELECT typeof(r.before) FROM commit_rows r JOIN commits c ON c.id = r.commit_id
              WHERE c.branch_id = ? AND r.tbl LIKE 'frozen:%' AND r.before IS NOT NULL",
            [work.as_str()],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(kept, "blob");

    // By the window, as it took main in: MCP goes back over its own commits alone.
    window(
        &core,
        "branch_revert",
        json!({"target": "work", "value": {"to": first}}),
    );
    assert_eq!(frozen(&conn).as_deref(), Some(bodies[0].as_str()));
}

#[test]
fn pieces_go_when_old_commits_are_pruned_and_when_a_branch_set_aside_is_let_go_of() {
    let core = Core::open(script_store()).unwrap();
    let (_, commits) = written(&core, "main", "Long", &mut long_lines("文"), 3);
    let mut conn = Connection::open(core.place()).unwrap();
    let project = project_id(&core);
    let all = held(&conn);

    // What main has now made old, under a thousand newer commits: past both of the stops.
    let newest = *commits.last().unwrap();
    let tx = conn.transaction().unwrap();
    tx.execute(
        "UPDATE commits SET at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now', '-100 days')
          WHERE project_id = ? AND branch_id = ''",
        [&project],
    )
    .unwrap();
    for seq in 1..=1_000_i64 {
        tx.execute(
            "INSERT INTO commits (id, project_id, branch_id, seq, at, author_kind)
             VALUES (?1, ?2, '', ?3, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'), 'mcp')",
            rusqlite::params![format!("newer-{seq}"), project, newest + seq],
        )
        .unwrap();
    }
    tx.commit().unwrap();
    let pruned = screen(&core, json!({"op": "prune"}))["pruned"]
        .as_i64()
        .unwrap();
    assert!(pruned >= 3, "{pruned}");
    assert!(held(&conn).is_empty(), "片付けたあとに塊が残っている");
    assert!(!all.is_empty());

    // A branch merged is set aside with its history, and let go of when the merge is cut from
    // main's: the pieces only its rows pointed at go then.
    window(&core, "branch_create", json!({"target": "work"}));
    written(&core, "work", "Other", &mut long_lines("別の話"), 2);
    window(&core, "branch_merge", json!({"target": "work"}));
    let merged = held(&conn);
    assert!(!merged.is_empty());
    assert_eq!(pointed_at(&conn), merged);
    ok(
        &core,
        "card_update",
        json!({"target": "Schemas:Customer", "value": {"description": "merge のあとの直し"}}),
    );
    screen(&core, json!({"op": "cut", "before": seq_of(&core, "main")}));
    assert_eq!(
        count(
            &conn,
            "SELECT count(*) FROM branches WHERE archived_at IS NOT NULL"
        ),
        0,
        "退けた branch が残っている"
    );
    assert_eq!(pointed_at(&conn), held(&conn));
    assert!(held(&conn).len() < merged.len());
}
