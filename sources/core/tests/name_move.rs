//! The built-in names of now (`specboard://SpecBoard/Docs/Design:CardKinds`): the kinds of line
//! named as the screen says them (`rules`, `shapeOf`, `condition`, `awaits`, `partner`), an enum's
//! value keyed by `name`, a goal's status and priority by values in English. A store of before
//! (`core_0026_builtin_names`) and a project file before version 33 are moved to them once; the
//! names and values of before written through MCP are refused with those of now.

use crate::common;

use common::{ok, op, refused, script_store};
use rusqlite::Connection;
use serde_json::{Value, json};
use specboard_core::Core;

/// The kinds of line renamed, each by its name of before and of now.
const LINES: [(&str, &str); 5] = [
    ("notes", "rules"),
    ("of", "shapeOf"),
    ("branch", "condition"),
    ("event", "awaits"),
    ("peer", "partner"),
];

/// A goal's values renamed, each in the JSON a document holds it in.
const VALUES: [(&str, &str); 2] = [
    ("\"status\":\"inProgress\"", "\"status\":\"進行中\""),
    ("\"priority\":\"high\"", "\"priority\":\"高\""),
];

const MARK: &str = "core_0026_builtin_names";

fn lines_of(read: &Value) -> Vec<String> {
    read["connections"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|l| l["line"].as_str().map(str::to_string))
        .collect()
}

#[test]
fn the_builtin_lines_are_named_as_they_are_shown() {
    let core = Core::open(script_store()).unwrap();
    let read = ok(&core, "kind_read", json!({}));
    let lines = read["lines"].as_array().unwrap();
    let label = |name: &str| {
        lines
            .iter()
            .find(|l| l["line"] == name)
            .unwrap_or_else(|| panic!("{name} が無い"))["label"]
            .clone()
    };
    for (name, said) in [
        ("rules", "決まり"),
        ("shapeOf", "形"),
        ("condition", "条件"),
        ("awaits", "待つ終わり"),
        ("partner", "相手"),
        ("key", "鍵"),
    ] {
        assert_eq!(label(name), said, "{name}");
    }
    for (before, _) in LINES {
        assert!(
            lines.iter().all(|l| l["line"] != before),
            "前の名前 {before} の線の種類がある"
        );
    }
}

#[test]
fn lines_and_keys_of_before_are_refused_with_the_names_of_now() {
    let core = Core::open(script_store()).unwrap();
    let said = refused(
        &core,
        "card_update",
        json!({"target": "Charts/Shop:Refund", "value": {"connections": {"add": [
            {"line": "notes", "to": "Free:Memo"}
        ]}}}),
    );
    assert!(said.contains("rules"), "{said}");
    let said = refused(
        &core,
        "card_update",
        json!({"target": "Charts/Shop:Refund.start", "value": {"peer": "Charts/Shop:Checkout"}}),
    );
    assert!(said.contains("partner"), "{said}");
}

/// The sample with a goal on the board and a branch that changed a flow, then every document
/// written back as a store of before had it: the lines by their names of before, the enum's values
/// keyed by `value`, the goal's values in Japanese, and the mark gone.
#[test]
fn a_store_of_before_has_the_builtin_names_of_now() {
    let place = common::place("builtin-names.sqlite");
    std::fs::copy(script_store(), &place).unwrap();
    let core = Core::open(place.clone()).unwrap();
    ok(
        &core,
        "card_create",
        json!({"branch": "_project_board_", "target": "Goals:Plan",
            "value": {"kind": "goal", "status": "inProgress", "priority": "high"}}),
    );
    ok(&core, "branch_create", json!({"target": "work"}));
    ok(
        &core,
        "card_update",
        json!({"branch": "work", "target": "Charts/Shop:Refund", "value": {"description": "branch で直した"}}),
    );
    let before = ok(
        &core,
        "card_read",
        json!({"target": "Charts/Shop:Refund.start"}),
    );
    assert!(
        lines_of(&before).contains(&"partner".to_string()),
        "{before}"
    );
    core.close();
    drop(core);
    {
        let conn = Connection::open(&place).unwrap();
        for (old, now) in LINES {
            for table in ["card_docs", "branch_frozen", "branch_prints"] {
                for column in ["doc", "base"] {
                    let _ = conn.execute(
                        &format!(
                            "UPDATE {table} SET {column} = replace({column}, '\"line\":\"{now}\"', '\"line\":\"{old}\"')"
                        ),
                        [],
                    );
                }
            }
        }
        for (now, old) in VALUES {
            conn.execute(
                &format!("UPDATE card_docs SET doc = replace(doc, '{now}', '{old}')"),
                [],
            )
            .unwrap();
        }
        conn.execute(
            "UPDATE card_docs SET doc = replace(doc, '\"fields\":{\"name\":\"open\"', '\"fields\":{\"value\":\"open\"')",
            [],
        )
        .unwrap();
        conn.execute("DELETE FROM _migrations WHERE name = ?", [MARK])
            .unwrap();
        let old: i64 = conn
            .query_row(
                "SELECT count(*) FROM card_docs WHERE doc LIKE '%\"line\":\"peer\"%'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(old > 0, "前の名前に書き戻せていない");
    }
    let core = Core::open(place.clone()).unwrap();
    for branch in ["main", "work"] {
        let read = ok(
            &core,
            "card_read",
            json!({"branch": branch, "target": "Charts/Shop:Refund.start"}),
        );
        assert!(
            lines_of(&read).contains(&"partner".to_string()),
            "{branch}: {read}"
        );
        let check = ok(
            &core,
            "card_read",
            json!({"branch": branch, "target": "Charts/Shop:Refund.check"}),
        );
        assert!(
            lines_of(&check).contains(&"condition".to_string()),
            "{branch}: {check}"
        );
    }
    let status = ok(&core, "card_read", json!({"target": "Schemas:Status"}));
    assert_eq!(status["values"][0]["name"], "open", "{status}");
    let goal = ok(
        &core,
        "card_read",
        json!({"branch": "_project_board_", "target": "Goals:Plan"}),
    );
    assert_eq!(goal["status"], "inProgress", "{goal}");
    assert_eq!(goal["priority"], "high", "{goal}");
    core.close();
    drop(core);
    let conn = Connection::open(&place).unwrap();
    let left: i64 = conn
        .query_row(
            "SELECT count(*) FROM card_docs WHERE doc LIKE '%\"line\":\"peer\"%' OR doc LIKE '%進行中%'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(left, 0);
    let marked: i64 = conn
        .query_row(
            "SELECT count(*) FROM _migrations WHERE name = ?",
            [MARK],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(marked, 1);
}

/// The sample's file as version 32 wrote it: its lines by their names of before, its enum's values
/// keyed by `value`, a goal on the board finished in Japanese.
#[test]
fn a_file_of_before_has_the_builtin_names_of_now() {
    let core = Core::open(script_store()).unwrap();
    ok(
        &core,
        "card_create",
        json!({"branch": "_project_board_", "target": "Goals:Done",
            "value": {"kind": "goal", "status": "done"}}),
    );
    let project: String = Connection::open(core.place())
        .unwrap()
        .query_row("SELECT id FROM projects WHERE name = ?", ["見本"], |r| {
            r.get(0)
        })
        .unwrap();
    let written = op(&core, json!({"op": "project_file", "project": project}))["result"]["text"]
        .as_str()
        .unwrap()
        .to_string();
    let mut file: Value = serde_json::from_str(&written).unwrap();
    file["version"] = json!(32);
    let mut text = file.to_string();
    for (old, now) in LINES {
        text = text.replace(
            &format!("\"line\":\"{now}\""),
            &format!("\"line\":\"{old}\""),
        );
    }
    text = text.replace("\"status\":\"done\"", "\"status\":\"完了\"");
    text = text.replace("{\"name\":\"open\"", "{\"value\":\"open\"");
    assert!(
        text.contains("\"line\":\"peer\""),
        "前の名前に書き戻せていない"
    );
    let place = common::place("builtin-names-file.sqlite");
    let fresh = Core::create(&place).unwrap();
    let read = op(&fresh, json!({"op": "project_file_read", "text": text}));
    assert!(read.get("error").is_none(), "{read}");
    let start = ok(
        &fresh,
        "card_read",
        json!({"target": "Charts/Shop:Refund.start"}),
    );
    assert!(lines_of(&start).contains(&"partner".to_string()), "{start}");
    let status = ok(&fresh, "card_read", json!({"target": "Schemas:Status"}));
    assert_eq!(status["values"][0]["name"], "open", "{status}");
    let goal = ok(
        &fresh,
        "card_read",
        json!({"branch": "_project_board_", "target": "Goals:Done"}),
    );
    assert_eq!(goal["status"], "done", "{goal}");
}

/// A project's own kind of line named as a built-in one is now (`rules`), in a store of before:
/// it steps aside as `rules_2`, its lines with it; the built-in one of before (`notes`) is `rules`.
#[test]
fn a_projects_own_line_of_a_builtin_name_steps_aside() {
    let place = common::place("builtin-names-own.sqlite");
    std::fs::copy(script_store(), &place).unwrap();
    let core = Core::open(place.clone()).unwrap();
    ok(
        &core,
        "kind_create",
        json!({"target": "cites", "value": {"of": "line", "label": "引く", "base": "link",
            "from": [{"kinds": ["note"], "at": "card"}], "to": {"kinds": ["*"], "at": ["card"]}}}),
    );
    ok(
        &core,
        "card_update",
        json!({"target": "Free:Memo", "value": {"connections": {"add": [
            {"line": "cites", "to": "Schemas/Ordering:Order"}
        ]}}}),
    );
    ok(
        &core,
        "card_update",
        json!({"target": "Charts/Shop:Refund", "value": {"connections": {"add": [
            {"line": "rules", "to": "Free:Memo"}
        ]}}}),
    );
    core.close();
    drop(core);
    {
        // Before: the built-in `notes`, and the project's own named `rules`.
        let conn = Connection::open(&place).unwrap();
        conn.execute_batch(&format!(
            "UPDATE card_docs SET doc = replace(doc, '\"line\":\"rules\"', '\"line\":\"notes\"');
             UPDATE card_docs SET doc = replace(doc, '\"line\":\"cites\"', '\"line\":\"rules\"');
             UPDATE projects SET kinds = replace(kinds, '\"line\":\"cites\"', '\"line\":\"rules\"');
             DELETE FROM _migrations WHERE name = '{MARK}';"
        ))
        .unwrap();
    }
    let core = Core::open(place).unwrap();
    let memo = ok(&core, "card_read", json!({"target": "Free:Memo"}));
    assert_eq!(lines_of(&memo), ["rules_2"], "{memo}");
    let refund = ok(&core, "card_read", json!({"target": "Charts/Shop:Refund"}));
    assert!(lines_of(&refund).contains(&"rules".to_string()), "{refund}");
    let kinds = ok(&core, "kind_read", json!({"target": "rules_2"}));
    assert_eq!(kinds["label"], "引く", "{kinds}");
}

#[test]
fn goal_values_of_before_are_refused_with_the_values_of_now() {
    let core = Core::open(script_store()).unwrap();
    ok(
        &core,
        "card_create",
        json!({"branch": "_project_board_", "target": "Goals:Plan", "value": {"kind": "goal"}}),
    );
    let said = refused(
        &core,
        "card_update",
        json!({"branch": "_project_board_", "target": "Goals:Plan", "value": {"status": "進行中"}}),
    );
    assert!(said.contains("inProgress"), "{said}");
    let said = refused(
        &core,
        "card_list",
        json!({"branch": "_project_board_", "value": {"fields": {"status": "完了"}}}),
    );
    assert!(said.contains("done"), "{said}");
}
