//! A body changed and read in part: `card_update`'s `body` as `{edit, append, prepend}` over
//! what it is — a document's, a note's, a task's and a formatdoc's text row — and `card_read`'s
//! `find` and `lines`, which give a part of it instead of the whole.

use crate::common;

use common::{call, matches, ok, refused, script_store};
use serde_json::json;
use specboard_core::Core;

const PROJECT: &str = "見本";

fn core() -> Core {
    Core::open(script_store()).unwrap()
}

fn body(core: &Core, target: &str, branch: Option<&str>) -> String {
    let mut arguments = json!({"target": target});
    if let Some(branch) = branch {
        arguments["branch"] = branch.into();
    }
    ok(core, "card_read", arguments)["body"]
        .as_str()
        .unwrap()
        .to_string()
}

const TEXT: &str = "# 設計\n\n## 決めたこと\n\n- 一つ目\n- 二つ目\n\n## 残り\n\n- 三つ目";

#[test]
fn a_document_and_a_note_are_changed_in_part() {
    let core = core();
    ok(
        &core,
        "card_create",
        json!({"target": "Docs:Design", "value": {"kind": "freedoc", "body": TEXT}}),
    );
    ok(
        &core,
        "card_update",
        json!({"target": "Docs:Design", "value": {"body": {
            "edit": [{"old": "- 二つ目", "new": "- 二つ目（済み）"}],
            "append": "- 四つ目",
        }}}),
    );
    assert_eq!(
        body(&core, "Docs:Design", None),
        "# 設計\n\n## 決めたこと\n\n- 一つ目\n- 二つ目（済み）\n\n## 残り\n\n- 三つ目\n- 四つ目"
    );
    // What is not there, or is there twice, writes nothing of the call.
    let said = refused(
        &core,
        "card_update",
        json!({"target": "Docs:Design", "value": {"body": {"edit": [
            {"old": "- 一つ目", "new": "- 1"},
            {"old": "- 無い", "new": "- x"},
        ]}}}),
    );
    assert!(said.contains("edit[1]"), "{said}");
    let said = refused(
        &core,
        "card_update",
        json!({"target": "Docs:Design", "value": {"body": {"edit": [{"old": "## ", "new": "### "}]}}}),
    );
    assert!(said.contains("2 か所"), "{said}");
    assert!(body(&core, "Docs:Design", None).contains("- 一つ目\n"));
    ok(
        &core,
        "card_update",
        json!({"target": "Docs:Design", "value": {"body": {"edit": [{"old": "## ", "new": "### ", "all": true}]}}}),
    );
    assert_eq!(body(&core, "Docs:Design", None).matches("### ").count(), 2);
    // A note, by its long text.
    ok(
        &core,
        "card_create",
        json!({"target": "Docs:Memo", "value": {"kind": "note", "body": "覚え"}}),
    );
    ok(
        &core,
        "card_update",
        json!({"target": "Docs:Memo", "value": {"body": {"prepend": "頭", "append": "末尾"}}}),
    );
    assert_eq!(body(&core, "Docs:Memo", None), "頭\n覚え\n末尾");
    // Written whole as before.
    ok(
        &core,
        "card_update",
        json!({"target": "Docs:Memo", "value": {"body": "丸ごと"}}),
    );
    assert_eq!(body(&core, "Docs:Memo", None), "丸ごと");
}

#[test]
fn a_task_and_a_text_row_are_changed_in_part() {
    let core = core();
    ok(
        &core,
        "card_create",
        json!({"branch": "_project_board_", "target": "Goals:Plan", "value": {"kind": "goal", "label": "目標"}}),
    );
    ok(
        &core,
        "card_create",
        json!({"branch": "_project_board_", "target": "Goals:Plan.Step", "value": {"kind": "task", "body": "やること\n\n## どこまで進んだか"}}),
    );
    ok(
        &core,
        "card_update",
        json!({"branch": "_project_board_", "target": "Goals:Plan.Step", "value": {"body": {"append": "- 半分"}}}),
    );
    assert_eq!(
        body(&core, "Goals:Plan.Step", Some("_project_board_")),
        "やること\n\n## どこまで進んだか\n- 半分"
    );
    ok(
        &core,
        "card_create",
        json!({"target": "Docs:Written", "value": {"kind": "formatdoc", "rows": [
            {"name": "story", "heading": "ねらい", "rows": [{"name": "said", "kind": "text", "body": "前の文。続き"}]},
        ]}}),
    );
    ok(
        &core,
        "card_update",
        json!({"target": "Docs:Written.story.said", "value": {"body": {"edit": [{"old": "前の文", "new": "今の文"}]}}}),
    );
    assert_eq!(body(&core, "Docs:Written.story.said", None), "今の文。続き");
    // The same through a batch.
    let staged = ok(
        &core,
        "batch",
        json!({"value": {"open": true, "steps": [{"tool": "card_update", "arguments": {"target": "Docs:Written.story.said", "value": {"body": {"append": "足した行"}}}}]}}),
    );
    let (failed, text) = call(
        &core,
        "batch",
        json!({"target": staged["batch"], "execute": true}),
    );
    assert!(!failed, "{text}");
    assert_eq!(
        body(&core, "Docs:Written.story.said", None),
        "今の文。続き\n足した行"
    );
}

#[test]
fn a_body_is_read_in_part() {
    let core = core();
    ok(
        &core,
        "card_create",
        json!({"target": "Docs:Design", "value": {"kind": "freedoc", "body": TEXT}}),
    );
    let found = ok(
        &core,
        "card_read",
        json!({"target": "Docs:Design", "value": {"find": "二つ目", "context": 1}}),
    );
    assert!(found.get("body").is_none(), "{found}");
    assert_eq!(found["lineCount"], 10);
    assert_eq!(
        found["found"],
        json!([{"line": 6, "text": "- 二つ目", "before": ["- 一つ目"], "after": [""]}])
    );
    let part = ok(
        &core,
        "card_read",
        json!({"target": "Docs:Design", "value": {"lines": {"from": 8, "to": 10}}}),
    );
    assert_eq!(part["body"], "## 残り\n\n- 三つ目");
    assert_eq!(part["lines"], json!({"from": 8, "to": 10}));
    let said = refused(
        &core,
        "card_read",
        json!({"target": "Schemas:Customer", "value": {"find": "x"}}),
    );
    assert!(said.contains("本文がありません"), "{said}");
}

/// A kind of the project's own: its Markdown field, whatever its name, changed in part too.
#[test]
fn a_markdown_field_of_the_projects_own_kind_is_changed_in_part() {
    let core = core();
    let page = json!({ "kind": "page", "fields": [{ "name": "memo", "fieldtype": "markdown" }] });
    common::set_kinds(&core, PROJECT, json!({"cards": [page]})).unwrap();
    let free = ok(&core, "section_list", json!({}))["sections"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["type"] == "free")
        .unwrap()["section"]
        .as_str()
        .unwrap()
        .to_string();
    let path = format!("{free}:Intro");
    ok(
        &core,
        "card_create",
        json!({"target": path, "value": {"kind": "page", "memo": "はじめ"}}),
    );
    ok(
        &core,
        "card_update",
        json!({"target": path, "value": {"memo": {"append": "つづき"}}}),
    );
    assert_eq!(
        ok(&core, "card_read", json!({"target": path}))["memo"],
        "はじめ\nつづき"
    );
}

// MARK: - The words inside a document and a flow, looked for

/// A formatdoc with a word in an item's heading, a text's body and a table's cell, a stackdoc
/// with it in a version, and a flow with it in a node's step.
fn words_inside(core: &Core) {
    ok(
        core,
        "card_create",
        json!({"target": "Docs:Words", "value": {"kind": "formatdoc", "rows": [
            {"name": "terms", "heading": "基点の言い方", "rows": [
                {"name": "said", "kind": "text", "body": "前の文\n取得の土台はここ\n後の文"},
                {"name": "table", "kind": "table", "columns": ["名前", "意味"],
                 "cells": [["Anchor", "取得の土台"], ["x", "y"]]}]}]}}),
    );
    ok(
        core,
        "card_create",
        json!({"target": "Docs:Log", "value": {"kind": "stackdoc", "versions": [
            {"name": "v2", "key": "2.0", "body": "ほかのこと"},
            {"name": "v1", "key": "1.0", "body": "取得の土台を足した"}]}}),
    );
    ok(
        core,
        "card_create",
        json!({"value": {
            "Charts/Shop:Fetch": {"kind": "flow", "label": "取る"},
            "Charts/Shop:Fetch.start": {"kind": "begin", "trigger": "receive",
                "connections": [{"line": "next", "to": "take"}]},
            "Charts/Shop:Fetch.take": {"kind": "process", "label": "読む",
                "steps": [{"do": "read", "description": "取得の土台から読む"}],
                "connections": [{"line": "next", "to": "done"}]},
            "Charts/Shop:Fetch.done": {"kind": "end", "outcome": "success"}
        }}),
    );
}

/// The places a query found, as (path, where).
fn places(found: &serde_json::Value) -> Vec<(String, String)> {
    matches(found)
        .iter()
        .map(|h| {
            (
                h["at"].as_str().unwrap().to_string(),
                h["where"].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

fn place(path: &str, at: &str) -> (String, String) {
    (path.to_string(), at.to_string())
}

/// `card_list`'s `query` finds a word in a formatdoc's rows (heading, body, a table's cells), a
/// stackdoc's version and a flow's node, each by the path of the row, version or node and
/// where in it, whatever the case; narrowed by `kind` and `in` too.
#[test]
fn a_query_finds_the_words_in_a_documents_rows_and_versions_and_a_flows_nodes() {
    let core = core();
    words_inside(&core);
    let found = places(&ok(
        &core,
        "card_list",
        json!({"value": {"query": "取得の土台"}}),
    ));
    for want in [
        place("Docs:Words.terms.said", "body"),
        place("Docs:Words.terms.table", "cells[0][1]"),
        place("Docs:Log.v1", "body"),
        place("Charts/Shop:Fetch.take", "steps[0].description"),
    ] {
        assert!(found.contains(&want), "{want:?} not in {found:?}");
    }
    let found = ok(&core, "card_list", json!({"value": {"query": "anchor"}}));
    let hit = matches(&found)
        .into_iter()
        .find(|h| h["at"] == "Docs:Words.terms.table")
        .unwrap();
    assert_eq!(hit["where"], "cells[0][0]");
    assert_eq!(hit["card"], "Docs:Words");
    assert_eq!(hit["kind"], "formatdoc");
    assert_eq!(hit["excerpt"], "Anchor");
    assert!(
        places(&ok(
            &core,
            "card_list",
            json!({"value": {"query": "基点の"}})
        ))
        .contains(&place("Docs:Words.terms", "heading"))
    );
    // With kind and in: only the cards of that kind, or inside that container.
    let docs = places(&ok(
        &core,
        "card_list",
        json!({"value": {"query": "取得の土台", "kind": "formatdoc"}}),
    ));
    assert!(!docs.is_empty());
    assert!(
        docs.iter().all(|(p, _)| p.starts_with("Docs:Words.")),
        "{docs:?}"
    );
    let nodes = places(&ok(
        &core,
        "card_list",
        json!({"value": {"query": "取得の土台", "in": "Charts/Shop:Fetch"}}),
    ));
    assert_eq!(
        nodes,
        vec![place("Charts/Shop:Fetch.take", "steps[0].description")]
    );
    let said = refused(
        &core,
        "card_list",
        json!({"value": {"query": "取得の土台", "tags": ["x"]}}),
    );
    assert_eq!(
        said,
        "query は tags と一緒には使いません（section・recursive・kind・in・limit・after・body は使えます）"
    );
}

/// `card_read`'s `find` on a formatdoc, an item of it, a stackdoc and a flow looks through the
/// whole of it: each place found by its path and where, with its line and the lines around it,
/// whatever the case. A body is looked for whatever the case too.
#[test]
fn find_looks_through_a_whole_document_and_a_flow() {
    let core = core();
    words_inside(&core);
    let found = ok(
        &core,
        "card_read",
        json!({"target": "Docs:Words", "value": {"find": "取得の土台", "context": 1}}),
    );
    assert!(found.get("rows").is_none(), "{found}");
    assert_eq!(found["path"], "Docs:Words");
    assert_eq!(
        found["found"],
        json!([
            {"path": "Docs:Words.terms.said", "where": "body", "line": 2, "text": "取得の土台はここ",
             "before": ["前の文"], "after": ["後の文"]},
            {"path": "Docs:Words.terms.table", "where": "cells[0][1]", "line": 1, "text": "取得の土台"}
        ])
    );
    let item = ok(
        &core,
        "card_read",
        json!({"target": "Docs:Words.terms", "value": {"find": "ANCHOR"}}),
    );
    assert_eq!(
        item["found"],
        json!([{"path": "Docs:Words.terms.table", "where": "cells[0][0]", "line": 1, "text": "Anchor"}])
    );
    let log = ok(
        &core,
        "card_read",
        json!({"target": "Docs:Log", "value": {"find": "取得の土台"}}),
    );
    assert_eq!(
        log["found"],
        json!([{"path": "Docs:Log.v1", "where": "body", "line": 1, "text": "取得の土台を足した"}])
    );
    let flow = ok(
        &core,
        "card_read",
        json!({"target": "Charts/Shop:Fetch", "value": {"find": "取得の土台"}}),
    );
    assert_eq!(
        flow["found"],
        json!([{"path": "Charts/Shop:Fetch.take", "where": "steps[0].description", "line": 1,
                "text": "取得の土台から読む"}])
    );
    // A body: whatever the case.
    ok(
        &core,
        "card_create",
        json!({"target": "Docs:Cased", "value": {"kind": "freedoc", "body": "One\nTwo Anchors"}}),
    );
    let cased = ok(
        &core,
        "card_read",
        json!({"target": "Docs:Cased", "value": {"find": "anchor"}}),
    );
    assert_eq!(
        cased["found"],
        json!([{"line": 2, "text": "Two Anchors", "before": ["One"]}])
    );
    // Lines are a body's alone.
    let said = refused(
        &core,
        "card_read",
        json!({"target": "Docs:Words", "value": {"lines": {"from": 1}}}),
    );
    assert!(said.contains("Docs:Words には本文がありません"), "{said}");
}

// MARK: - A formatdoc's rows put by name, and a table's cells changed one by one

/// The names of the rows directly in what `target` names, in their order.
fn row_names(core: &Core, target: &str) -> Vec<String> {
    ok(core, "card_read", json!({"target": target}))["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| {
            r["path"]
                .as_str()
                .unwrap()
                .rsplit('.')
                .next()
                .unwrap()
                .to_string()
        })
        .collect()
}

/// `card_create` puts a row before or after a sibling it names, `card_update` moves one there;
/// with neither, as before (at the end, where it is). A sibling another added meanwhile does not
/// move it elsewhere; a name not among the siblings, itself, or both are refused, nothing
/// written.
#[test]
fn a_row_is_put_before_or_after_a_sibling_by_its_name() {
    let core = core();
    ok(
        &core,
        "card_create",
        json!({"target": "Docs:Steps", "value": {"kind": "formatdoc", "rows": [
            {"name": "steps", "heading": "手順", "rows": [
                {"name": "a", "kind": "text", "body": "一"},
                {"name": "b", "kind": "text", "body": "二"}]}]}}),
    );
    let list = "Docs:Steps.steps";
    ok(
        &core,
        "card_create",
        json!({"target": "Docs:Steps.steps.head", "value": {"kind": "text", "body": "この手順の flow は …", "before": "a"}}),
    );
    assert_eq!(row_names(&core, list), ["head", "a", "b"]);
    // Another adds after a meanwhile: a row put after a still goes right after it.
    ok(
        &core,
        "card_create",
        json!({"target": "Docs:Steps.steps.other", "value": {"kind": "text", "body": "他", "after": "a"}}),
    );
    ok(
        &core,
        "card_create",
        json!({"target": "Docs:Steps.steps.mid", "value": {"kind": "text", "body": "間", "after": "a"}}),
    );
    assert_eq!(row_names(&core, list), ["head", "a", "mid", "other", "b"]);
    ok(
        &core,
        "card_create",
        json!({"target": "Docs:Steps.steps.last", "value": {"kind": "text", "body": "終"}}),
    );
    assert_eq!(
        row_names(&core, list),
        ["head", "a", "mid", "other", "b", "last"]
    );
    // Moved, alone or with what else is changed.
    ok(
        &core,
        "card_update",
        json!({"target": "Docs:Steps.steps.b", "value": {"before": "head"}}),
    );
    assert_eq!(
        row_names(&core, list),
        ["b", "head", "a", "mid", "other", "last"]
    );
    ok(
        &core,
        "card_update",
        json!({"target": "Docs:Steps.steps.other", "value": {"after": "last", "body": "他の文"}}),
    );
    assert_eq!(
        row_names(&core, list),
        ["b", "head", "a", "mid", "last", "other"]
    );
    assert_eq!(body(&core, "Docs:Steps.steps.other", None), "他の文");
    // Refused, nothing written.
    let said = refused(
        &core,
        "card_create",
        json!({"target": "Docs:Steps.steps.x", "value": {"kind": "text", "before": "zz"}}),
    );
    assert_eq!(
        said,
        "before の「zz」は同じ親の行にありません（同じ親の行: b・head・a・mid・last・other）"
    );
    let said = refused(
        &core,
        "card_update",
        json!({"target": "Docs:Steps.steps.a", "value": {"before": "b", "after": "b"}}),
    );
    assert_eq!(
        said,
        "before と after は一緒には書けません（どちらか 1 つ）"
    );
    let said = refused(
        &core,
        "card_update",
        json!({"target": "Docs:Steps.steps.a", "value": {"after": "a"}}),
    );
    assert_eq!(said, "after に自分（a）は書けません");
    assert_eq!(
        row_names(&core, list),
        ["b", "head", "a", "mid", "last", "other"]
    );
}

/// A table's cells read back.
fn cells(core: &Core, target: &str) -> serde_json::Value {
    ok(core, "card_read", json!({"target": target}))["cells"].clone()
}

/// A table row's `cells` as `{set, add, remove}`: a cell written whole or in part, a row added
/// before or after one, a row taken away — each named over the table as read, by its columns'
/// values or by its number with what it held, a column by its heading. What fits nothing or
/// more than one, a number without `old` or with another, are refused with what there is and
/// nothing written. The whole list is written as before.
#[test]
fn a_tables_cells_are_changed_one_by_one_over_the_table_as_read() {
    let core = core();
    ok(
        &core,
        "card_create",
        json!({"target": "Docs:Terms", "value": {"kind": "formatdoc", "rows": [
            {"name": "words", "kind": "table", "columns": ["名前", "意味", "状態"],
             "cells": [["anchor", "基点", "未"], ["ledger", "台帳", "未"], ["proof", "証跡", "未"]]},
            {"name": "said", "kind": "text", "body": "文"}]}}),
    );
    let table = "Docs:Terms.words";
    let edit = |cells: serde_json::Value| json!({"target": table, "value": {"cells": cells}});
    // A cell by its row's value, and one by its number with what it holds, in part.
    ok(
        &core,
        "card_update",
        edit(json!({"set": [
            {"row": {"名前": "ledger"}, "col": "状態", "value": "済み"},
            {"row": 2, "old": "証跡", "col": "意味", "value": {"append": "（証拠）"}},
            {"row": {"名前": "anchor"}, "col": "意味", "value": {"edit": [{"old": "基", "new": "起"}]}}
        ]})),
    );
    assert_eq!(
        cells(&core, table),
        json!([
            ["anchor", "起点", "未"],
            ["ledger", "台帳", "済み"],
            ["proof", "証跡（証拠）", "未"]
        ])
    );
    // Rows added after one, before one by its number, at the end; one taken away.
    ok(
        &core,
        "card_update",
        edit(json!({
            "add": [
                {"cells": ["base", "土台", "未"], "after": {"名前": "anchor"}},
                {"cells": ["head", "頭", "未"], "before": 0, "old": ["anchor", "起点", "未"]},
                {"cells": ["tail", "尾", "未"]}
            ],
            "remove": [{"row": {"名前": "proof"}}]
        })),
    );
    assert_eq!(
        cells(&core, table),
        json!([
            ["head", "頭", "未"],
            ["anchor", "起点", "未"],
            ["base", "土台", "未"],
            ["ledger", "台帳", "済み"],
            ["tail", "尾", "未"]
        ])
    );
    let before = cells(&core, table);
    // Refused, nothing written: a number without old, or with another; a column or a row that
    // is not there; a row two fit.
    let said = refused(
        &core,
        "card_update",
        edit(json!({"set": [{"row": 1, "col": "状態", "value": "済み"}]})),
    );
    assert!(
        said.starts_with(
            "words.cells.set[0] は番号で行を指すので、old に今の中身を添えます（そのセルの文字）"
        ),
        "{said}"
    );
    let said = refused(
        &core,
        "card_update",
        edit(json!({"set": [{"row": 1, "old": "済み", "col": "状態", "value": "未"}]})),
    );
    assert_eq!(
        said,
        "words.cells.set[0].old が今のセルと違います（今: \"未\"）。読み直してから書きます"
    );
    let said = refused(
        &core,
        "card_update",
        edit(json!({"remove": [{"row": 3, "old": ["anchor", "起点", "未"]}]})),
    );
    assert!(
        said.starts_with(
            "words.cells.remove[0].old が今の行と違います（今: [\"ledger\",\"台帳\",\"済み\"]）"
        ),
        "{said}"
    );
    let said = refused(
        &core,
        "card_update",
        edit(json!({"set": [{"row": {"名前": "anchor"}, "col": "読み", "value": "x"}]})),
    );
    assert_eq!(
        said,
        "words.cells.set[0].col の「読み」は列の見出しにありません（列: 名前・意味・状態）"
    );
    let said = refused(
        &core,
        "card_update",
        edit(json!({"remove": [{"row": {"名前": "nothing"}}]})),
    );
    assert!(
        said.starts_with(
            "words.cells.remove[0] に合う行がありません。今の行: 0: [\"head\",\"頭\",\"未\"]・"
        ),
        "{said}"
    );
    let said = refused(
        &core,
        "card_update",
        edit(json!({"set": [{"row": {"状態": "未"}, "col": "意味", "value": "x"}]})),
    );
    assert!(
        said.starts_with("words.cells.set[0] に合う行が 4 行あります（列を足して 1 行に絞るか、番号と old で指します）: 0: "),
        "{said}"
    );
    let said = refused(&core, "card_update", edit(json!({"move": []})));
    assert!(
        said.contains("move") && said.contains("set / add / remove"),
        "{said}"
    );
    let said = refused(
        &core,
        "card_update",
        json!({"target": "Docs:Terms.said", "value": {"cells": {"remove": [{"row": 0, "old": []}]}}}),
    );
    assert_eq!(
        said,
        "said は表（table）の行ではないので、cells の一部は直せません（text）"
    );
    assert_eq!(cells(&core, table), before);
    // The whole list, as before.
    ok(&core, "card_update", edit(json!([["only", "一つ", "未"]])));
    assert_eq!(cells(&core, table), json!([["only", "一つ", "未"]]));
}
