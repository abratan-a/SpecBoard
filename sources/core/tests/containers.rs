//! Containers: a flow holds its nodes and a goal its goals and tasks as cards inside them
//! (`in`), named on from the container's path (`Charts/Shop:Checkout.save`). A flow's nodes are
//! joined by the flow's lines (`next`, `branch`, `else`, `onError`, `onTimeout`), and what
//! their rows read, write, sign with and hand on to are its lines too (`reads`, `writes`,
//! `key`, `peer`, `runs`, `event`). Written with the card tools alone, from the window too (the
//! keys of before refused from both); read by the window as the cards inside (`inner`, `in`)
//! and their lines (`relations`); a store and a project file of before are moved once
//! (`container_move`).

use crate::common;

use common::{call, checked, line_edits, ok, op, project_id, refused, script_store, text_in};
use rusqlite::Connection;
use serde_json::{Value, json};
use specboard_core::Core;

const PROJECT: &str = "見本";

/// A card's lines written as `after`, as `card_update` writes them: what goes from the lines it
/// has to those (`{add, remove}`).
fn whole(core: &Core, card: &str, after: Value) -> Value {
    let before = ok(core, "card_read", json!({"target": card}))["connections"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    json!({"target": card, "value": {"connections": line_edits(&before, after.as_array().unwrap())}})
}

/// The project as the window reads it.
fn detail(core: &Core) -> Value {
    op(
        core,
        json!({"op": "project_detail", "project": project_id(core)}),
    )["result"]
        .clone()
}

/// The cards inside a flow as the window reads them (`inner`), in their order.
fn inner_of(core: &Core, flow: &str) -> Vec<Value> {
    let whole = detail(core);
    let id = whole["schemas"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == flow)
        .unwrap()["id"]
        .clone();
    whole["inner"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|n| n["in"] == id)
        .cloned()
        .collect()
}

/// A card's lines as MCP reads them: (line, from, to).
fn lines(card: &Value) -> Vec<(String, String, String)> {
    card["connections"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|l| {
            (
                l["line"].as_str().unwrap().to_string(),
                l["from"].as_str().unwrap_or_default().to_string(),
                l["to"].as_str().unwrap_or_default().to_string(),
            )
        })
        .collect()
}

/// A flow and its nodes made in one call, the flow before what goes inside it.
fn make_pay(core: &Core) -> Value {
    ok(
        core,
        "card_create",
        json!({"value": {
            "Charts/Shop:Pay": {"kind": "flow", "label": "支払う"},
            "Charts/Shop:Pay.start": {"kind": "begin", "trigger": "receive", "carries": ["Schemas/Ordering:Order.total"],
                "connections": [{"line": "partner", "to": "Charts/Shop:Checkout"}, {"line": "next", "to": "check"}]},
            "Charts/Shop:Pay.check": {"kind": "process", "label": "確かめる",
                "steps": [{"do": "read"}, {"do": "write", "description": "記録に置く"}, {"do": "sign"}],
                "connections": [
                    {"line": "reads", "from": "steps[0]", "to": "Schemas/Ordering:Order", "at": "total"},
                    {"line": "writes", "from": "steps[1]", "to": "Schemas:Orders"},
                    {"line": "key", "from": "steps[2]", "to": "Schemas:Customer", "at": "id", "fields": {"use": "sign"}},
                    {"line": "next", "to": "decide"}
                ]},
            "Charts/Shop:Pay.decide": {"kind": "eval", "value": "Schemas/Ordering:Order.status",
                "connections": [
                    {"line": "condition", "to": "done", "fields": {"when": "[[Schemas:Status]] が paid"}},
                    {"line": "else", "to": "failed"}
                ]},
            "Charts/Shop:Pay.done": {"kind": "end", "outcome": "success", "return": {"carries": ["Schemas/Ordering:Order.id"]}},
            "Charts/Shop:Pay.failed": {"kind": "end", "outcome": "failure"}
        }}),
    )
}

#[test]
fn a_flow_and_its_nodes_are_made_in_one_call_and_read_as_cards() {
    let core = Core::open(script_store()).unwrap();
    let made = make_pay(&core);
    assert!(made.get("warnings").is_none(), "{made}");
    // The flow: the cards inside it, one level, by their paths.
    let pay = ok(&core, "card_read", json!({"target": "Charts/Shop:Pay"}));
    assert_eq!(
        pay["cards"],
        json!([
            "Charts/Shop:Pay.start",
            "Charts/Shop:Pay.check",
            "Charts/Shop:Pay.decide",
            "Charts/Shop:Pay.done",
            "Charts/Shop:Pay.failed"
        ])
    );
    // A node: its kind, its fields and rows, its lines by path.
    let check = ok(
        &core,
        "card_read",
        json!({"target": "Charts/Shop:Pay.check"}),
    );
    assert_eq!(check["kind"], "process");
    assert_eq!(check["label"], "確かめる");
    assert_eq!(check["steps"][1]["description"], "記録に置く");
    assert_eq!(
        lines(&check),
        [
            (
                "reads".into(),
                "steps[0]".into(),
                "Schemas/Ordering:Order".into()
            ),
            ("writes".into(), "steps[1]".into(), "Schemas:Orders".into()),
            ("key".into(), "steps[2]".into(), "Schemas:Customer".into()),
            ("next".into(), "".into(), "Charts/Shop:Pay.decide".into())
        ]
    );
    // card_list reads them by what holds them, or by their kind.
    let held = ok(
        &core,
        "card_list",
        json!({"value": {"in": "Charts/Shop:Pay", "include": ["connections"]}}),
    );
    assert_eq!(held["matched"], 5);
    let ends = ok(
        &core,
        "card_list",
        json!({"value": {"kind": "end", "section": "Charts/Shop"}}),
    );
    assert!(ends["matched"].as_u64().unwrap() >= 2, "{ends}");
    // Not among a section's cards, nor counted with them.
    let shop = ok(&core, "section_read", json!({"target": "Charts/Shop"}));
    assert!(
        !shop["cards"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c.as_str().unwrap().contains("Pay.")),
        "{shop}"
    );

    // The window reads them as the cards inside the flow (`inner`): their fields and rows as
    // kept, their lines. The flow keeps no `nodes`.
    let nodes = inner_of(&core, "Pay");
    let names: Vec<&str> = nodes.iter().map(|n| n["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["start", "check", "decide", "done", "failed"]);
    let whole = detail(&core);
    let pay_card = whole["schemas"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "Pay")
        .unwrap()
        .clone();
    assert!(pay_card.get("nodes").is_none(), "{pay_card}");
    assert_eq!(nodes[0]["kind"], "begin");
    assert_eq!(nodes[0]["form"]["fields"]["trigger"], "receive");
    assert_eq!(
        nodes[1]["form"]["rows"]["steps"][1]["fields"]["description"],
        "記録に置く"
    );
    assert_eq!(nodes[2]["connections"][0]["line"], "condition");
    assert_eq!(nodes[2]["connections"][0]["to"], nodes[3]["id"]);
    // Each placed on the flow's card map.
    assert!(
        nodes
            .iter()
            .all(|n| n["posX"].is_number() && n["posY"].is_number()),
        "{nodes:?}"
    );
    // Every line of theirs a relation, from the node card: the ways between the nodes, and
    // what a row reads, writes and signs with.
    let from_check: Vec<(String, String, Value)> = whole["relations"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["fromSchemaId"] == nodes[1]["id"])
        .map(|r| {
            (
                r["line"].as_str().unwrap().to_string(),
                r["from"].as_str().unwrap().to_string(),
                r["at"].clone(),
            )
        })
        .collect();
    assert_eq!(
        from_check,
        [
            ("reads".to_string(), "steps[0]".to_string(), json!("total")),
            ("writes".to_string(), "steps[1]".to_string(), Value::Null),
            ("key".to_string(), "steps[2]".to_string(), json!("id")),
            ("next".to_string(), String::new(), Value::Null),
        ]
    );

    // The diagrams are drawn from them as from the nodes before.
    let chart = ok(
        &core,
        "card_read",
        json!({"target": "Charts/Shop:Pay", "value": {"as": "diagram"}}),
    );
    let chart = text_in(&chart, "mermaid");
    assert!(chart.contains("flowchart"), "{chart}");
    assert!(chart.contains("確かめる"), "{chart}");
    // A key's use: the flow that signs with it.
    let impact = ok(
        &core,
        "card_read",
        json!({"target": "Schemas:Customer.id", "value": {"as": "impact"}}),
    );
    assert!(impact.to_string().contains("Charts/Shop:Pay"), "{impact}");
}

/// A card read as Markdown, as MCP gives it: what is said of it, and its text.
fn markdown(core: &Core, card: &str) -> (Value, String) {
    let said = ok(
        core,
        "card_read",
        json!({"target": card, "value": {"as": "markdown"}}),
    );
    let text = text_in(&said, "markdown");
    (said, text)
}

#[test]
fn a_flow_is_read_as_one_markdown_with_every_node_its_rows_and_its_ways() {
    let core = Core::open(script_store()).unwrap();
    ok(
        &core,
        "card_create",
        json!({"value": {
            "Charts/Shop:Tell": {"kind": "flow", "label": "知らせる", "description": "返金が済んだら [[Schemas:Customer]] に知らせる"},
            "Charts/Shop:Tell.start": {"kind": "begin", "trigger": "event", "description": "返金の終わりを受ける",
                "connections": [{"line": "awaits", "to": "Charts/Shop:Refund", "fields": {"outcome": "success"}}, {"line": "next", "to": "send"}]},
            "Charts/Shop:Tell.send": {"kind": "process", "label": "送る",
                "steps": [{"do": "send", "program": "mailer", "via": "http", "carries": ["Schemas/Ordering:Order.id"], "description": "メールで"}],
                "connections": [{"line": "next", "to": "wait"}]},
            "Charts/Shop:Tell.wait": {"kind": "wait", "for": "timer", "timeout": "1時間",
                "connections": [{"line": "next", "to": "again"}, {"line": "onTimeout", "to": "gave_up"}]},
            "Charts/Shop:Tell.again": {"kind": "run",
                "connections": [{"line": "runs", "to": "Charts/Shop:Checkout"}, {"line": "next", "to": "done"}, {"line": "onError", "to": "gave_up"}]},
            "Charts/Shop:Tell.done": {"kind": "end", "outcome": "success"},
            "Charts/Shop:Tell.gave_up": {"kind": "end", "outcome": "cancelled"}
        }}),
    );
    let (said, text) = markdown(&core, "Charts/Shop:Tell");
    assert_eq!(said["branch"], "main", "{said}");
    assert_eq!(said["path"], "Charts/Shop:Tell", "{said}");
    assert!(said["rev"].is_string(), "{said}");
    // Its label, its description with the paths as written, its flowchart as the diagram is.
    assert!(
        text.starts_with(
            "# 知らせる\n\n返金が済んだら [[Schemas:Customer]] に知らせる\n\n```mermaid\n"
        ),
        "{text}"
    );
    let drawn = ok(
        &core,
        "card_read",
        json!({"target": "Charts/Shop:Tell", "value": {"as": "diagram", "shape": "flowchart"}}),
    );
    let drawn = text_in(&drawn, "mermaid");
    let chart = drawn.split_once("flowchart LR\n").unwrap().1;
    assert!(text.contains(chart.trim_end()), "{text}\n{drawn}");
    // Every node in the flow's order, each its name and kind, what it is and where it goes on.
    let headings: Vec<&str> = text.lines().filter(|l| l.starts_with("## ")).collect();
    assert_eq!(
        headings,
        [
            "## start（begin）",
            "## send（process）",
            "## wait（wait）",
            "## again（run）",
            "## done（end）",
            "## gave_up（end）"
        ]
    );
    for part in [
        "## start（begin）\n\n返金の終わりを受ける\n\n- きっかけ: `event`\n- event: [[Charts/Shop:Refund]] が `success`\n- 次: [[Charts/Shop:Tell.send]]\n",
        "## send（process）\n\n**送る**\n\n| 行 | do | 相手・パス | 運ぶもの | 説明 |\n|---|---|---|---|---|\n| `steps[0]` | send | `mailer`（via `http`） | [[Schemas/Ordering:Order.id]] | メールで |\n\n- 次: [[Charts/Shop:Tell.wait]]\n",
        "## wait（wait）\n\n- 待つもの: `timer`\n- 期限: 1時間\n- 次: [[Charts/Shop:Tell.again]]\n- 期限切れ: [[Charts/Shop:Tell.gave_up]]\n",
        "## again（run）\n\n- 呼ぶ flow: [[Charts/Shop:Checkout]]\n- 次: [[Charts/Shop:Tell.done]]\n- 失敗したら: [[Charts/Shop:Tell.gave_up]]\n",
        "## gave_up（end）\n\n- 結果: `cancelled`",
    ] {
        assert!(text.contains(part), "{part}\n---\n{text}");
    }
    // A start that receives, rows that call and send, the ways an eval parts, what an end gives
    // back.
    let (_, refund) = markdown(&core, "Charts/Shop:Refund");
    for part in [
        "- きっかけ: `receive`\n- 相手: [[Charts/Shop:Checkout]]\n- 運ぶもの: [[Schemas/Ordering:Order.total]]\n",
        "| `steps[0]` | call | `payment-gateway` | [[Schemas/Ordering:Order.total]] → [[Schemas/Ordering:Order.status]] |  |",
        "| `steps[1]` | send | [[Charts/Shop:Checkout]] | [[Schemas/Ordering:Order.id]] |  |",
        "- 次（並列）: [[Charts/Shop:Refund.a]]、[[Charts/Shop:Refund.b]]",
        "- 判断に使う値: [[Schemas/Ordering:Order.status]]\n- 条件 [[Schemas:Status]] が paid: [[Charts/Shop:Refund.done]]\n- それ以外: [[Charts/Shop:Refund.ask]]",
        "- 結果: `success`\n- 返す相手: 開始の相手\n- 返すもの: [[Schemas/Ordering:Order.id]]",
    ] {
        assert!(refund.contains(part), "{part}\n---\n{refund}");
    }
    // Written out, its page says the same of each node, the paths as a document writes them out.
    let page = ok(
        &core,
        "export_read",
        json!({"targetdir": "[default]", "target": "charts", "value": {"files": ["charts.md"]}}),
    );
    let page = page["files"][0]["text"].as_str().unwrap();
    for part in [
        "### check（eval）\n\n- 判断に使う値: `Order.status`\n- 条件 Status が paid: [`Refund.done`](charts.md#refund)\n",
        "| `steps[1]` | send | [購入](charts.md#checkout) | `Order.id` |  |",
        "### again（run）\n\n- 呼ぶ flow: [購入](charts.md#checkout)\n",
    ] {
        assert!(page.contains(part), "{part}\n---\n{page}");
    }
    // A node is no document: what can be read so is said.
    let (failed, said) = call(
        &core,
        "card_read",
        json!({"project": PROJECT, "target": "Charts/Shop:Tell.send", "value": {"as": "markdown"}}),
    );
    assert!(failed && said.contains("flow"), "{said}");
}

#[test]
fn a_nodes_lines_are_held_to_the_flows_kinds_of_line() {
    let core = Core::open(script_store()).unwrap();
    make_pay(&core);
    // A way goes to a node beside it, in the same flow.
    let said = refused(
        &core,
        "card_update",
        whole(
            &core,
            "Charts/Shop:Pay.decide",
            json!([
                {"line": "condition", "to": "Charts/Shop:Checkout.done", "fields": {"when": "x"}},
                {"line": "else", "to": "failed"}
            ]),
        ),
    );
    assert!(
        said.contains("相手は同じ入れ物（Charts/Shop:Pay）の中のカードです"),
        "{said}"
    );
    // An end goes nowhere; a row is one there is; a key says its use.
    let said = refused(
        &core,
        "card_update",
        whole(
            &core,
            "Charts/Shop:Pay.done",
            json!([{"line": "next", "to": "failed"}]),
        ),
    );
    assert!(said.contains("next は書けません"), "{said}");
    let said = refused(
        &core,
        "card_update",
        whole(
            &core,
            "Charts/Shop:Pay.check",
            json!([
                {"line": "reads", "from": "steps[0]", "to": "Schemas/Ordering:Order", "at": "total"},
                {"line": "writes", "from": "steps[1]", "to": "Schemas:Orders"},
                {"line": "key", "from": "steps[2]", "to": "Schemas:Customer", "fields": {"use": "sign"}},
                {"line": "reads", "from": "steps[7]", "to": "Schemas:Orders"},
                {"line": "next", "to": "decide"}
            ]),
        ),
    );
    assert!(
        said.contains("出元の「steps[7]」は Charts/Shop:Pay.check にありません"),
        "{said}"
    );
    let said = refused(
        &core,
        "card_update",
        whole(
            &core,
            "Charts/Shop:Pay.check",
            json!([
                {"line": "reads", "from": "steps[0]", "to": "Schemas/Ordering:Order", "at": "total"},
                {"line": "writes", "from": "steps[1]", "to": "Schemas:Orders"},
                {"line": "key", "from": "steps[2]", "to": "Schemas:Customer"},
                {"line": "next", "to": "decide"}
            ]),
        ),
    );
    assert!(said.contains("fields.use: 要ります"), "{said}");
    // A read or write row without what it reads or writes is not finished yet, not wrong:
    // kept, and said in the warnings and `flowProblems` (the window writes a node before
    // its lines).
    let kept = ok(
        &core,
        "card_update",
        whole(
            &core,
            "Charts/Shop:Pay.check",
            json!([
                {"line": "key", "from": "steps[2]", "to": "Schemas:Customer", "fields": {"use": "sign"}},
                {"line": "next", "to": "decide"}
            ]),
        ),
    );
    let said = kept["warnings"].to_string();
    assert!(
        said.contains(
            "check（処理）に steps[0]（read）の reads の線がありません（from: steps[0]。読む所）"
        ),
        "{said}"
    );
    assert!(
        said.contains(
            "check（処理）に steps[1]（write）の writes の線がありません（from: steps[1]。書く所）"
        ),
        "{said}"
    );
    let read = checked(&core, "main");
    assert!(
        read["flowProblems"]
            .to_string()
            .contains("steps[0]（read）の reads の線がありません"),
        "{read}"
    );
    // Made so, as the window makes a node: its other lines with it, these later.
    let made = ok(
        &core,
        "card_create",
        json!({"target": "Charts/Shop:Pay.more", "value": {"kind": "process", "steps": [{"do": "write"}]}}),
    );
    assert!(
        made["warnings"]
            .to_string()
            .contains("more（処理）に steps[0]（write）の writes の線がありません"),
        "{made}"
    );
    // A node written in the keys of before is told the lines they are now.
    let said = refused(
        &core,
        "card_update",
        json!({"target": "Charts/Shop:Pay.decide", "value": {"branches": [{"when": "x", "next": "done"}], "else": "failed"}}),
    );
    assert!(said.contains("branches は使いません"), "{said}");
    assert!(
        said.contains(r#"{"line": "else", "to": "<行き先のノード>"}"#),
        "{said}"
    );
    // A node stays in its flow.
    let said = refused(
        &core,
        "card_update",
        json!({"target": "Charts/Shop:Pay.done", "value": {"in": "Charts/Shop:Checkout"}}),
    );
    assert!(said.contains("ほかの flow へは移せません"), "{said}");
    // A flow has one begin.
    let said = refused(
        &core,
        "card_create",
        json!({"target": "Charts/Shop:Pay.again", "value": {"kind": "begin", "trigger": "manual"}}),
    );
    assert!(said.contains("開始（begin）が 2 つあります"), "{said}");
    // A node goes only inside a flow; a flow holds only nodes.
    let said = refused(
        &core,
        "card_create",
        json!({"target": "Charts/Shop:Loose", "value": {"kind": "process"}}),
    );
    assert!(
        said.contains("process は flow の中にだけ置けます"),
        "{said}"
    );
    let said = refused(
        &core,
        "card_create",
        json!({"target": "Charts/Shop:Pay.memo", "value": {"kind": "note", "body": "x"}}),
    );
    assert!(
        said.contains("Charts/Shop:Pay（flow）の中に置けるのは begin・process・run・eval・wait・join・end です"),
        "{said}"
    );
    // What is not finished yet is said, not refused: a flow is written a node at a time.
    let made = ok(
        &core,
        "card_create",
        json!({"target": "Charts/Shop:Pay.later", "value": {"kind": "run"}}),
    );
    let warnings = made["warnings"].to_string();
    assert!(
        warnings.contains("later（実行）に runs の線がありません"),
        "{made}"
    );
}

#[test]
fn a_node_renamed_or_let_go_takes_its_ways_and_paths_along() {
    let core = Core::open(script_store()).unwrap();
    ok(
        &core,
        "card_update",
        json!({"target": "Free:Memo", "value": {"body": "見る [[Charts/Shop:Checkout.save]]"}}),
    );
    // Renamed: the ways to it follow by id, the paths that named it are written again.
    let renamed = ok(
        &core,
        "card_update",
        json!({"target": "Charts/Shop:Checkout.save", "value": {"name": "persist"}}),
    );
    assert_eq!(renamed["path"], "Charts/Shop:Checkout.persist", "{renamed}");
    let start = ok(
        &core,
        "card_read",
        json!({"target": "Charts/Shop:Checkout.start"}),
    );
    assert!(
        lines(&start).contains(&(
            "next".to_string(),
            String::new(),
            "Charts/Shop:Checkout.persist".to_string()
        )),
        "{start}"
    );
    let memo = ok(&core, "card_read", json!({"target": "Free:Memo"}));
    assert_eq!(memo["body"], "見る [[Charts/Shop:Checkout.persist]]");
    // A node's name is its own in its flow: another flow may have one of the same name.
    let said = refused(
        &core,
        "card_update",
        json!({"target": "Charts/Shop:Checkout.persist", "value": {"name": "check"}}),
    );
    assert!(said.contains("カード「check」は既にあります"), "{said}");
    // Let go of: the ways to it go with it, the node they came from left unfinished.
    let gone = ok(
        &core,
        "card_delete",
        json!({"target": "Charts/Shop:Checkout.persist"}),
    );
    assert_eq!(gone["deleted"], "Charts/Shop:Checkout.persist");
    let start = ok(
        &core,
        "card_read",
        json!({"target": "Charts/Shop:Checkout.start"}),
    );
    assert!(!lines(&start).iter().any(|l| l.0 == "next"), "{start}");
    let flow = checked(&core, "main");
    assert!(
        flow["flowProblems"]
            .to_string()
            .contains("start（開始）に next の線がありません"),
        "{flow}"
    );
    // The flow let go of, its nodes with it.
    let nodes: Vec<String> = inner_of(&core, "Checkout")
        .iter()
        .map(|n| n["id"].as_str().unwrap().to_string())
        .collect();
    assert!(nodes.len() >= 4, "{nodes:?}");
    ok(
        &core,
        "card_delete",
        json!({"target": "Charts/Shop:Checkout"}),
    );
    let conn = Connection::open(core.place()).unwrap();
    for id in &nodes {
        let left: i64 = conn
            .query_row(
                "SELECT count(*) FROM card_docs WHERE id = ? AND removed = 0",
                [id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(left, 0, "{id}");
    }
}

#[test]
fn the_keys_of_before_are_refused_with_how_to_write_them_now() {
    let core = Core::open(script_store()).unwrap();
    let said = refused(
        &core,
        "card_update",
        json!({"target": "Charts/Shop:Checkout", "value": {"nodes": []}}),
    );
    assert!(said.starts_with("nodes は使いません"), "{said}");
    let said = refused(
        &core,
        "card_create",
        json!({"target": "Charts/Shop:New", "value": {"kind": "flow", "nodes": []}}),
    );
    assert!(said.contains("nodes は使いません"), "{said}");
    let mut arguments = json!({"target": "Goals:Ship", "value": {"items": []}});
    arguments["branch"] = "_project_board_".into();
    let said = refused(&core, "card_update", arguments);
    assert!(said.starts_with("items は使いません"), "{said}");
    let mut arguments =
        json!({"target": "Goals:Next", "value": {"kind": "task", "in": "Goals:Ship"}});
    arguments["branch"] = "_project_board_".into();
    let said = refused(&core, "card_create", arguments);
    assert!(said.starts_with("card_create の in は使いません"), "{said}");
}

#[test]
fn the_window_writes_what_a_goal_holds_with_the_tools() {
    let core = Core::open(script_store()).unwrap();
    let project = project_id(&core);
    let tool = |name: &str, arguments: Value| -> Value {
        let mut arguments = arguments;
        arguments["project"] = PROJECT.into();
        arguments["branch"] = "_project_board_".into();
        op(
            &core,
            json!({"op": "tool", "name": name, "arguments": arguments}),
        )
    };
    // The keys of before are refused from the window too.
    let said = tool(
        "card_create",
        json!({"target": "Goals:Decide", "value": {"kind": "task", "in": "Goals:Ship", "label": "決める"}}),
    );
    assert!(
        said["error"]
            .as_str()
            .unwrap()
            .contains("card_create の in は使いません"),
        "{said}"
    );
    let said = tool(
        "card_update",
        json!({"target": "Goals:Ship", "value": {"items": []}}),
    );
    assert!(
        said["error"]
            .as_str()
            .unwrap()
            .contains("items は使いません"),
        "{said}"
    );
    // A task made in the goal by its path (`addTask`).
    let made = tool(
        "card_create",
        json!({"target": "Goals:Ship.Decide", "value": {"kind": "task", "label": "決める"}}),
    );
    assert!(made.get("error").is_none(), "{made}");
    let board = || {
        op(
            &core,
            json!({"op": "project_detail", "project": project, "branch": "~board"}),
        )["result"]["schemas"]
            .as_array()
            .unwrap()
            .clone()
    };
    let id = |name: &str| {
        board().iter().find(|c| c["name"] == name).unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string()
    };
    // What the goal holds: the cards whose `in` is the goal's, in their order.
    let held = || {
        let cards = board();
        let ship = cards.iter().find(|c| c["name"] == "Ship").unwrap()["id"].clone();
        assert!(
            cards.iter().find(|c| c["name"] == "Ship").unwrap()["form"]["fields"]
                .get("items")
                .is_none()
        );
        cards
            .iter()
            .filter(|c| c["in"] == ship)
            .map(|c| c["id"].as_str().unwrap().to_string())
            .collect::<Vec<_>>()
    };
    assert_eq!(held(), [id("Design"), id("t2"), id("Decide")]);
    // Put in another order (`setCustomRows`: `position`), and one taken out (`in: null`).
    let wrote = tool(
        "card_update",
        json!({"target": "Goals:Ship.t2", "value": {"position": 0}}),
    );
    assert!(wrote.get("error").is_none(), "{wrote}");
    let out = tool(
        "card_update",
        json!({"target": "Goals:Ship.Decide", "value": {"in": null}}),
    );
    assert!(out.get("error").is_none(), "{out}");
    assert_eq!(held(), [id("t2"), id("Design")]);
    let decide = board().into_iter().find(|c| c["name"] == "Decide").unwrap();
    assert!(decide.get("in").is_none(), "{decide}");
    // The path of before (the section and the name alone) names nothing now.
    let before = tool(
        "card_update",
        json!({"target": "Goals:t2", "value": {"done": true}}),
    );
    assert!(before.get("error").is_some(), "{before}");
    let done = tool(
        "card_update",
        json!({"target": "Goals:Ship.t2", "value": {"done": true}}),
    );
    assert!(done.get("error").is_none(), "{done}");
    let t2 = board().into_iter().find(|c| c["name"] == "t2").unwrap();
    assert_eq!(t2["form"]["fields"]["done"], true);
    let gone = tool("card_delete", json!({"target": "Goals:Ship.t2"}));
    assert!(gone.get("error").is_none(), "{gone}");
    assert_eq!(held(), [id("Design")]);
}

#[test]
fn a_projects_own_line_named_as_a_flows_is_renamed_on_the_move() {
    let place = common::place("own-next.sqlite");
    let core = Core::create(&place).unwrap();
    let (refused, said) = call(&core, "project_create", json!({"project": "線の名前"}));
    assert!(!refused, "{said}");
    core.close();
    drop(core);
    {
        let conn = Connection::open(&place).unwrap();
        let project: String = conn
            .query_row(
                "SELECT id FROM projects WHERE name = '線の名前'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let schemas: String = conn
            .query_row(
                "SELECT id FROM section_docs WHERE project_id = ? AND name = 'Schemas'",
                [&project],
                |r| r.get(0),
            )
            .unwrap();
        let own = json!({"lines": [{ "line": "next", "label": "次の版", "base": "link",
            "from": [{"kinds": ["entity"], "at": "card"}], "to": {"kinds": ["entity"], "at": ["card"]} }]});
        conn.execute(
            "UPDATE projects SET kinds = ? WHERE id = ?",
            rusqlite::params![own.to_string(), project],
        )
        .unwrap();
        let card = json!({"id": "c-a", "kind": "entity", "name": "A", "section": schemas, "v": 2,
            "connections": [{"id": "l1", "line": "next", "to": "c-a"}]});
        conn.execute(
            "INSERT INTO card_docs (project_id, branch_id, id, section_id, name, kind, doc) VALUES (?, '', 'c-a', ?, 'A', 'entity', ?)",
            rusqlite::params![project, schemas, card.to_string()],
        )
        .unwrap();
        conn.execute_batch("DELETE FROM _migrations WHERE name = 'core_0006_containers'")
            .unwrap();
    }
    Core::open(&place).unwrap().close();
    let conn = Connection::open(&place).unwrap();
    let kinds: String = conn
        .query_row(
            "SELECT kinds FROM projects WHERE name = '線の名前'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let kinds: Value = serde_json::from_str(&kinds).unwrap();
    assert_eq!(kinds["lines"][0]["line"], "next_2");
    let doc: String = conn
        .query_row("SELECT doc FROM card_docs WHERE id = 'c-a'", [], |r| {
            r.get(0)
        })
        .unwrap();
    let doc: Value = serde_json::from_str(&doc).unwrap();
    assert_eq!(doc["connections"][0]["line"], "next_2");
}

#[test]
fn a_branchs_copy_of_a_flow_and_the_history_are_moved_too() {
    // A store of before: main's flow, a branch's copy of it with a node changed, and a
    // commit in main that changed the flow's nodes.
    let place = common::place("branch-flow.sqlite");
    let core = Core::create(&place).unwrap();
    let (refused, said) = call(&core, "project_create", json!({"project": "枝の流れ"}));
    assert!(!refused, "{said}");
    let (refused, said) = call(
        &core,
        "branch_create",
        json!({"project": "枝の流れ", "target": "work"}),
    );
    assert!(!refused, "{said}");
    core.close();
    drop(core);
    let (project, branch, last) = {
        let conn = Connection::open(&place).unwrap();
        let project: String = conn
            .query_row(
                "SELECT id FROM projects WHERE name = '枝の流れ'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let branch: String = conn
            .query_row("SELECT id FROM branches WHERE name = 'work'", [], |r| {
                r.get(0)
            })
            .unwrap();
        let charts: String = conn
            .query_row(
                "SELECT id FROM section_docs WHERE project_id = ? AND name = 'Charts'",
                [&project],
                |r| r.get(0),
            )
            .unwrap();
        let flow = |end_label: &str, outcome: &str| {
            json!({ "id": "c-flow", "kind": "flow", "name": "Go", "section": charts, "v": 2,
                "rows": { "nodes": [
                    { "id": "n-start", "fields": { "name": "start", "kind": "begin", "trigger": "manual", "next": ["done"] } },
                    { "id": "n-done", "fields": { "name": "done", "kind": "end", "outcome": outcome, "label": end_label } }
                ] } })
        };
        let main = flow("終わり", "success");
        conn.execute(
            "INSERT INTO card_docs (project_id, branch_id, id, section_id, name, kind, doc) VALUES (?, '', 'c-flow', ?, 'Go', 'flow', ?)",
            rusqlite::params![project, charts, main.to_string()],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO card_docs (project_id, branch_id, id, section_id, name, kind, doc, base, based_at) VALUES (?, ?, 'c-flow', ?, 'Go', 'flow', ?, ?, 'then')",
            rusqlite::params![project, branch, charts, flow("枝で直した", "failure").to_string(), main.to_string()],
        )
        .unwrap();
        // main's history: a commit that changed the end's label, kept as it was before.
        let before = json!({"section_id": charts, "name": "Go", "kind": "flow", "removed": 0,
            "doc": flow("前", "success").to_string(), "base": null, "based_at": null});
        let last: i64 = conn
            .query_row(
                "SELECT MAX(seq) FROM commits WHERE project_id = ? AND branch_id = ''",
                [&project],
                |r| r.get(0),
            )
            .unwrap();
        conn.execute(
            "INSERT INTO commits (id, project_id, branch_id, seq, at, author_kind, kind) VALUES ('k1', ?, '', ?, 'now', 'screen', 'edit')",
            rusqlite::params![project, last + 1],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO commit_rows (commit_id, tbl, row_id, before) VALUES ('k1', 'card_docs', 'c-flow', ?)",
            [before.to_string()],
        )
        .unwrap();
        conn.execute_batch("DELETE FROM _migrations WHERE name = 'core_0006_containers'")
            .unwrap();
        (project, branch, last)
    };
    let core = Core::open(&place).unwrap();
    let conn = Connection::open(&place).unwrap();
    let row = |tree: &str, id: &str| -> (Option<Value>, Option<Value>) {
        conn.query_row(
            "SELECT doc, base FROM card_docs WHERE branch_id = ? AND id = ?",
            [tree, id],
            |r| {
                let parse = |t: Option<String>| t.and_then(|t| serde_json::from_str(&t).ok());
                Ok((parse(r.get(0)?), parse(r.get(1)?)))
            },
        )
        .unwrap()
    };
    // The branch's copy of the flow gives copies of its nodes, each with main's as its base.
    let (doc, base) = row(&branch, "n-done");
    assert_eq!(doc.unwrap()["fields"]["label"], "枝で直した");
    assert_eq!(base.unwrap()["fields"]["label"], "終わり");
    let (doc, _) = row(&branch, "c-flow");
    assert!(doc.unwrap().get("rows").is_none());
    // The branch says it changed the node, not main.
    let (failed, diff) = call(
        &core,
        "branch_read",
        json!({"project": "枝の流れ", "target": "work"}),
    );
    assert!(!failed, "{diff}");
    let diff: Value = serde_json::from_str(&diff).unwrap();
    assert!(
        diff["changed"].to_string().contains("Charts:Go.done"),
        "{diff}"
    );
    assert!(diff.get("mainChanged").is_none(), "{diff}");
    // The history: the flow's row without its nodes, and a row of each node card as it was.
    let kept: Vec<(String, Option<String>)> = {
        let mut stmt = conn
            .prepare(
                "SELECT row_id, before FROM commit_rows WHERE commit_id = 'k1' ORDER BY row_id",
            )
            .unwrap();
        stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    };
    let ids: Vec<&str> = kept.iter().map(|(id, _)| id.as_str()).collect();
    assert_eq!(ids, ["c-flow", "n-done", "n-start"]);
    let done: Value = serde_json::from_str(kept[1].1.as_deref().unwrap()).unwrap();
    let done_doc: Value = serde_json::from_str(done["doc"].as_str().unwrap()).unwrap();
    assert_eq!(done_doc["fields"]["label"], "前");
    assert_eq!(done_doc["in"], "c-flow");
    // Going back past it puts the node back as it was: main's going back is a branch, merged
    // in.
    let undone = op(
        &core,
        json!({"op": "revert", "project": project, "to": last}),
    );
    assert!(undone.get("error").is_none(), "{undone}");
    let back = undone["result"]["branch"].as_str().unwrap().to_string();
    let merged = op(
        &core,
        json!({"op": "merge", "project": project, "source": back, "target": ""}),
    );
    assert!(merged.get("error").is_none(), "{merged}");
    let (doc, _) = row("", "n-done");
    assert_eq!(doc.unwrap()["fields"]["label"], "前");
}

#[test]
fn the_guide_tells_of_the_nodes_their_kinds_and_their_lines() {
    // A flow is a card holding its nodes, joined by lines: its guide lists their kinds, one line
    // each, and how they are made.
    let Ok(specboard_core::mcp::Output::Json(flow)) = specboard_core::mcp::guide(Some("flow"))
    else {
        panic!("flow の guide がありません");
    };
    let kinds: Vec<&str> = flow["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|k| k.as_str().unwrap().split('（').next().unwrap())
        .collect();
    assert_eq!(
        kinds,
        ["begin", "process", "run", "eval", "wait", "join", "end"]
    );
    // The example makes a flow and its nodes in one call.
    let made = flow["example"]["value"].as_object().unwrap();
    assert!(made.values().any(|card| card["kind"] == "flow"), "{flow}");
    assert!(made.values().filter(|card| card["kind"] != "flow").count() >= 2);
    // A line one at a time: the key's guide, the flow's kinds of line among them.
    let Ok(specboard_core::mcp::Output::Json(lines)) =
        specboard_core::mcp::guide(Some("connections"))
    else {
        panic!("connections の guide がありません");
    };
    let said = lines["value"].as_str().unwrap();
    for line in [
        "next",
        "condition",
        "else",
        "onError",
        "onTimeout",
        "runs",
        "reads",
        "key",
    ] {
        assert!(said.contains(line), "{line}: {said}");
    }
    // No topic of their own any more: a key of that name is said as a key.
    for gone in ["nodes", "lines", "kinds"] {
        if let Ok(specboard_core::mcp::Output::Json(said)) = specboard_core::mcp::guide(Some(gone))
        {
            assert!(said.get("about").is_none(), "{gone}: {said}");
        }
    }
    let Ok(specboard_core::mcp::Output::Json(one)) = specboard_core::mcp::guide(Some("eval"))
    else {
        panic!("eval の guide がありません");
    };
    assert_eq!(one["kind"], "eval");
}

#[test]
fn two_cards_of_one_name_in_a_goals_items_go_in_one_and_are_linked_one() {
    let place = common::place("goal-names.sqlite");
    let core = Core::create(&place).unwrap();
    let (refused, said) = call(&core, "project_create", json!({"project": "同じ名前"}));
    assert!(!refused, "{said}");
    let (refused, said) = call(
        &core,
        "section_create",
        json!({"project": "同じ名前", "target": "Free/Sub"}),
    );
    assert!(!refused, "{said}");
    core.close();
    drop(core);
    {
        let conn = Connection::open(&place).unwrap();
        let project: String = conn
            .query_row(
                "SELECT id FROM projects WHERE name = '同じ名前'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let section = |name: &str| -> String {
            conn.query_row(
                "SELECT id FROM section_docs WHERE project_id = ? AND name = ?",
                [&project, name],
                |r| r.get(0),
            )
            .unwrap()
        };
        let (free, sub) = (section("Free"), section("Sub"));
        let put = |id: &str, section: &str, name: &str, kind: &str, doc: Value| {
            conn.execute(
                "INSERT INTO card_docs (project_id, branch_id, id, section_id, name, kind, doc) VALUES (?, '', ?, ?, ?, ?, ?)",
                rusqlite::params![project, id, section, name, kind, doc.to_string()],
            )
            .unwrap();
        };
        put(
            "g",
            &free,
            "Aim",
            "goal",
            json!({"id": "g", "kind": "goal", "name": "Aim", "section": free, "v": 2,
                "fields": {"items": ["t1", "t2"]}}),
        );
        put(
            "t1",
            &free,
            "T",
            "task",
            json!({"id": "t1", "kind": "task", "name": "T", "section": free, "v": 2}),
        );
        put(
            "t2",
            &sub,
            "T",
            "task",
            json!({"id": "t2", "kind": "task", "name": "T", "section": sub, "v": 2}),
        );
        conn.execute_batch("DELETE FROM _migrations WHERE name = 'core_0006_containers'")
            .unwrap();
    }
    let core = Core::open(&place).unwrap();
    let read = |value: Value| -> Value {
        let (refused, aim) = call(
            &core,
            "card_read",
            json!({"project": "同じ名前", "target": "Free:Aim", "value": value}),
        );
        assert!(!refused, "{aim}");
        serde_json::from_str(&aim).unwrap()
    };
    assert_eq!(read(json!({}))["cards"], json!(["Free:Aim.T"]));
    assert_eq!(
        read(json!({"include": ["connections"]}))["connections"],
        json!([{"line": "link", "to": "Free/Sub:T"}])
    );
}

/// A flow drawn as its sequence diagram (`card_read` `as: diagram`, `shape: sequence`).
fn sequence(core: &Core, flow: &str) -> String {
    let chart = ok(
        core,
        "card_read",
        json!({"target": flow, "value": {"as": "diagram", "shape": "sequence"}}),
    );
    text_in(&chart, "mermaid")
}

/// The lines of a diagram from the first `from` on, without their indent, `n` of them.
fn lines_from(chart: &str, from: &str, n: usize) -> Vec<String> {
    let all: Vec<String> = chart.lines().map(|l| l.trim().to_string()).collect();
    let at = all
        .iter()
        .position(|l| l.starts_with(from))
        .unwrap_or_else(|| panic!("{from} がありません: {chart}"));
    all[at..(at + n).min(all.len())].to_vec()
}

#[test]
fn a_decisions_else_with_a_way_of_its_own_is_drawn_in_the_alts_else() {
    // One way or the other until they meet, not the else after the alt as if both were
    // taken.
    let core = Core::open(script_store()).unwrap();
    ok(
        &core,
        "card_create",
        json!({"value": {
            "Charts/Shop:Pick": {"kind": "flow", "label": "選ぶ"},
            "Charts/Shop:Pick.start": {"kind": "begin", "trigger": "manual", "connections": [{"line": "next", "to": "who"}]},
            "Charts/Shop:Pick.who": {"kind": "eval", "value": "Schemas/Ordering:Order.status",
                "connections": [
                    {"line": "condition", "to": "warden", "fields": {"when": "warden"}},
                    {"line": "else", "to": "notary"}
                ]},
            "Charts/Shop:Pick.warden": {"kind": "process", "label": "warden が取り込む",
                "connections": [{"line": "next", "to": "after"}]},
            "Charts/Shop:Pick.notary": {"kind": "process", "label": "notary が取り込む",
                "connections": [{"line": "next", "to": "after"}]},
            "Charts/Shop:Pick.after": {"kind": "process", "label": "受け入れる",
                "connections": [{"line": "next", "to": "done"}]},
            "Charts/Shop:Pick.done": {"kind": "end", "outcome": "success"}
        }}),
    );
    let chart = sequence(&core, "Charts/Shop:Pick");
    assert_eq!(
        lines_from(&chart, "alt warden", 12),
        [
            "alt warden",
            "Note over P1: warden warden が取り込む",
            "activate P1",
            "deactivate P1",
            "else それ以外",
            "Note over P1: notary notary が取り込む",
            "activate P1",
            "deactivate P1",
            "end",
            "Note over P1: after 受け入れる",
            "activate P1",
            "deactivate P1",
        ],
        "{chart}"
    );
    assert_eq!(chart.matches("受け入れる").count(), 1, "{chart}");

    // Ways that come to the else's node: the else is where it goes on, as before.
    ok(
        &core,
        "card_update",
        json!({"target": "Charts/Shop:Pick.warden", "value": {"connections": {
            "add": [{"line": "next", "to": "notary"}],
            "remove": [{"line": "next", "to": "after"}]
        }}}),
    );
    let chart = sequence(&core, "Charts/Shop:Pick");
    assert_eq!(
        lines_from(&chart, "alt warden", 11),
        [
            "alt warden",
            "Note over P1: warden warden が取り込む",
            "activate P1",
            "deactivate P1",
            "end",
            "Note over P1: notary notary が取り込む",
            "activate P1",
            "deactivate P1",
            "Note over P1: after 受け入れる",
            "activate P1",
            "deactivate P1",
        ],
        "{chart}"
    );
}

#[test]
fn ways_that_stop_at_ends_of_their_own_leave_the_others_to_come_together() {
    // The ways that come together at an end still do, one that stops on its own says its answer
    // — or, answering nothing, how it ends.
    let core = Core::open(script_store()).unwrap();
    ok(
        &core,
        "card_create",
        json!({"value": {
            "Charts/Shop:Lock": {"kind": "flow", "label": "鍵を見る"},
            "Charts/Shop:Lock.start": {"kind": "begin", "trigger": "manual", "connections": [{"line": "next", "to": "check"}]},
            "Charts/Shop:Lock.check": {"kind": "eval", "value": "Schemas/Ordering:Order.status",
                "connections": [
                    {"line": "condition", "to": "refused", "fields": {"when": "ロック中"}},
                    {"line": "condition", "to": "fix", "fields": {"when": "壊れた"}},
                    {"line": "condition", "to": "done", "fields": {"when": "何もしない場合"}},
                    {"line": "condition", "to": "write", "fields": {"when": "それ以外"}},
                    {"line": "else", "to": "done"}
                ]},
            "Charts/Shop:Lock.refused": {"kind": "end", "outcome": "success", "description": "断る",
                "return": {"carries": ["Schemas/Ordering:Order.id"]}},
            "Charts/Shop:Lock.fix": {"kind": "process", "label": "直す", "connections": [{"line": "next", "to": "broken"}]},
            "Charts/Shop:Lock.broken": {"kind": "end", "outcome": "failure"},
            "Charts/Shop:Lock.write": {"kind": "process", "label": "書く", "connections": [{"line": "next", "to": "done"}]},
            "Charts/Shop:Lock.done": {"kind": "end", "outcome": "success",
                "return": {"carries": ["Schemas/Ordering:Order.total"]}}
        }}),
    );
    let chart = sequence(&core, "Charts/Shop:Lock");
    assert_eq!(
        lines_from(&chart, "alt ロック中", 15),
        [
            "alt ロック中",
            "P2-->>P1: return Order.id · success",
            "else 壊れた",
            "Note over P2: fix 直す",
            "activate P2",
            "deactivate P2",
            "Note over P2: failure",
            "else 何もしない場合",
            "Note over P2: → done へ",
            "else それ以外",
            "Note over P2: write 書く",
            "activate P2",
            "deactivate P2",
            "end",
            "P2-->>P1: return Order.total · success",
        ],
        "{chart}"
    );
}

#[test]
fn a_way_with_nothing_of_its_own_says_where_it_goes_on() {
    // The place the ways meet is drawn after the alt, so a way that goes straight there has
    // nothing of its own: it says where it goes, not that nothing is done.
    let core = Core::open(script_store()).unwrap();
    ok(
        &core,
        "card_create",
        json!({"value": {
            "Charts/Shop:Fetch": {"kind": "flow", "label": "取ってくる"},
            "Charts/Shop:Fetch.start": {"kind": "begin", "trigger": "manual", "connections": [{"line": "next", "to": "found"}]},
            "Charts/Shop:Fetch.found": {"kind": "eval", "value": "Schemas/Ordering:Order.status",
                "connections": [
                    {"line": "condition", "to": "download", "fields": {"when": "入"}},
                    {"line": "condition", "to": "tell", "fields": {"when": "切"}},
                    {"line": "else", "to": "none"}
                ]},
            "Charts/Shop:Fetch.tell": {"kind": "process", "label": "知らせる", "connections": [{"line": "next", "to": "download"}]},
            "Charts/Shop:Fetch.download": {"kind": "process", "label": "取ってきて確かめる", "connections": [{"line": "next", "to": "done"}]},
            "Charts/Shop:Fetch.none": {"kind": "end", "outcome": "success"},
            "Charts/Shop:Fetch.done": {"kind": "end", "outcome": "success"}
        }}),
    );
    let chart = sequence(&core, "Charts/Shop:Fetch");
    assert!(!chart.contains("何もしない"), "{chart}");
    let from = lines_from(&chart, "alt 入", 12);
    assert_eq!(from[1], "Note over P1: → 取ってきて確かめる へ", "{chart}");
    assert!(
        from.iter().any(|l| l == "Note over P1: tell 知らせる"),
        "{chart}"
    );
    assert_eq!(
        chart
            .matches("Note over P1: download 取ってきて確かめる")
            .count(),
        1,
        "{chart}"
    );
}

#[test]
fn a_waits_time_running_out_is_drawn_as_a_way_of_its_own() {
    // A decision's else that goes elsewhere and a wait's onTimeout, each drawn — not left
    // out.
    let core = Core::open(script_store()).unwrap();
    ok(
        &core,
        "card_create",
        json!({"value": {
            "Charts/Shop:Dispatch": {"kind": "flow", "label": "配る"},
            "Charts/Shop:Dispatch.start": {"kind": "begin", "trigger": "manual", "connections": [{"line": "next", "to": "found"}]},
            "Charts/Shop:Dispatch.found": {"kind": "eval", "value": "Schemas/Ordering:Order.status",
                "connections": [
                    {"line": "condition", "to": "send", "fields": {"when": "いる"}},
                    {"line": "else", "to": "later"}
                ]},
            "Charts/Shop:Dispatch.send": {"kind": "process", "label": "配る",
                "connections": [{"line": "next", "to": "ack"}]},
            "Charts/Shop:Dispatch.ack": {"kind": "wait", "for": "approval", "timeout": "30秒",
                "connections": [{"line": "next", "to": "sent"}, {"line": "onTimeout", "to": "expire"}]},
            "Charts/Shop:Dispatch.expire": {"kind": "process", "label": "リースを切る",
                "connections": [{"line": "next", "to": "expired"}]},
            "Charts/Shop:Dispatch.later": {"kind": "process", "label": "あとで配る",
                "connections": [{"line": "next", "to": "queued"}]},
            "Charts/Shop:Dispatch.sent": {"kind": "end", "outcome": "success", "label": "配った"},
            "Charts/Shop:Dispatch.expired": {"kind": "end", "outcome": "cancelled"},
            "Charts/Shop:Dispatch.queued": {"kind": "end", "outcome": "success"}
        }}),
    );
    let chart = sequence(&core, "Charts/Shop:Dispatch");
    assert_eq!(
        lines_from(&chart, "alt いる", 17),
        [
            "alt いる",
            "Note over P1: send 配る",
            "activate P1",
            "deactivate P1",
            "Note over P1: wait approval（30秒 まで）",
            "alt 間に合ったとき",
            "Note over P1: 配った",
            "else 期限切れ",
            "Note over P1: expire リースを切る",
            "activate P1",
            "deactivate P1",
            "end",
            "else それ以外",
            "Note over P1: later あとで配る",
            "activate P1",
            "deactivate P1",
            "end",
        ],
        "{chart}"
    );

    // A time running out that stops at once in failure, with nothing on the way: one mark.
    ok(
        &core,
        "card_update",
        json!({"target": "Charts/Shop:Dispatch.ack", "value": {"connections": {
            "add": [{"line": "onTimeout", "to": "expired"}],
            "remove": [{"line": "onTimeout", "to": "expire"}]
        }}}),
    );
    let chart = sequence(&core, "Charts/Shop:Dispatch");
    assert_eq!(
        lines_from(&chart, "Note over P1: wait approval", 2),
        [
            "Note over P1: wait approval（30秒 まで）",
            "Note right of P1: ✕ 期限切れ → cancelled",
        ],
        "{chart}"
    );
    assert!(!chart.contains("break "), "{chart}");

    // Ways that come together again, inside ways that do too: each alt ends where its own
    // ways meet, what is after them drawn once.
    ok(
        &core,
        "card_create",
        json!({"value": {
            "Charts/Shop:Lease": {"kind": "flow", "label": "貸す"},
            "Charts/Shop:Lease.start": {"kind": "begin", "trigger": "manual", "connections": [{"line": "next", "to": "found"}]},
            "Charts/Shop:Lease.found": {"kind": "eval", "value": "Schemas/Ordering:Order.status",
                "connections": [
                    {"line": "condition", "to": "send", "fields": {"when": "いる"}},
                    {"line": "else", "to": "later"}
                ]},
            "Charts/Shop:Lease.send": {"kind": "process", "label": "配る", "connections": [{"line": "next", "to": "ack"}]},
            "Charts/Shop:Lease.ack": {"kind": "wait", "for": "approval", "timeout": "30秒",
                "connections": [{"line": "next", "to": "after"}, {"line": "onTimeout", "to": "expire"}]},
            "Charts/Shop:Lease.expire": {"kind": "process", "label": "リースを切る", "connections": [{"line": "next", "to": "after"}]},
            "Charts/Shop:Lease.later": {"kind": "process", "label": "あとで配る", "connections": [{"line": "next", "to": "after"}]},
            "Charts/Shop:Lease.after": {"kind": "process", "label": "記録する", "connections": [{"line": "next", "to": "done"}]},
            "Charts/Shop:Lease.done": {"kind": "end", "outcome": "success"}
        }}),
    );
    let chart = sequence(&core, "Charts/Shop:Lease");
    assert_eq!(
        lines_from(&chart, "alt いる", 18),
        [
            "alt いる",
            "Note over P1: send 配る",
            "activate P1",
            "deactivate P1",
            "Note over P1: wait approval（30秒 まで）",
            "alt 期限切れ",
            "Note over P1: expire リースを切る",
            "activate P1",
            "deactivate P1",
            "end",
            "else それ以外",
            "Note over P1: later あとで配る",
            "activate P1",
            "deactivate P1",
            "end",
            "Note over P1: after 記録する",
            "activate P1",
            "deactivate P1",
        ],
        "{chart}"
    );
    assert_eq!(chart.matches("記録する").count(), 1, "{chart}");
}

/// Flows made with their nodes in one call are held together once all are written, not a node at a
/// time: a flow wrong among them is refused as a node at a time refuses it — at the node that made
/// it wrong, and nothing of the call written.
#[test]
fn a_flow_wrong_among_those_made_in_one_call_is_refused_at_its_node() {
    let core = Core::open(script_store()).unwrap();
    let flow = |name: &str| -> Vec<(String, Value)> {
        let at = format!("Charts/Shop:{name}");
        vec![
            (at.clone(), json!({"kind": "flow"})),
            (
                format!("{at}.start"),
                json!({"kind": "begin", "trigger": "manual", "connections": [{"line": "next", "to": "work"}]}),
            ),
            (
                format!("{at}.work"),
                json!({"kind": "process", "steps": [{"do": "read"}], "connections": [
                    {"line": "reads", "from": "steps[0]", "to": "Schemas/Ordering:Order", "at": "total"},
                    {"line": "next", "to": "done"}
                ]}),
            ),
            (
                format!("{at}.done"),
                json!({"kind": "end", "outcome": "success"}),
            ),
        ]
    };
    let mut value: serde_json::Map<String, Value> = flow("Good").into_iter().collect();
    value.extend(flow("Twice"));
    value.insert(
        "Charts/Shop:Twice.again".into(),
        json!({"kind": "begin", "trigger": "manual"}),
    );
    let said = refused(&core, "card_create", json!({"value": value}));
    assert!(
        said.contains("Charts/Shop:Twice.again: 開始（begin）が 2 つあります（start・again）"),
        "{said}"
    );
    let (failed, _) = call(
        &core,
        "card_read",
        json!({"project": "見本", "target": "Charts/Shop:Good"}),
    );
    assert!(failed, "断られた呼び出しの flow が書かれている");
    // Without it, both are made, their ways joined.
    value.remove("Charts/Shop:Twice.again");
    let made = ok(&core, "card_create", json!({"value": value}));
    assert_eq!(made["created"].as_array().map(Vec::len), Some(8), "{made}");
    let work = ok(
        &core,
        "card_read",
        json!({"target": "Charts/Shop:Twice.work"}),
    );
    assert_eq!(
        work["referencedBy"],
        json!(["Charts/Shop:Twice.start"]),
        "{work}"
    );
}

/// Node names are told apart in any case, so a way to a node is looked up the same: a node
/// named with a capital letter is one a way can lead to.
#[test]
fn a_way_leads_to_a_node_named_with_a_capital_letter() {
    let core = Core::open(script_store()).unwrap();
    let made = ok(
        &core,
        "card_create",
        json!({"value": {
            "Charts/Shop:Draw": {"kind": "flow"},
            "Charts/Shop:Draw.start": {"kind": "begin", "trigger": "manual",
                "connections": [{"line": "next", "to": "askApp"}]},
            "Charts/Shop:Draw.askApp": {"kind": "process", "steps": [{"do": "read"}], "connections": [
                {"line": "reads", "from": "steps[0]", "to": "Schemas/Ordering:Order", "at": "total"},
                {"line": "next", "to": "answered"}
            ]},
            "Charts/Shop:Draw.answered": {"kind": "eval", "value": "Schemas/Ordering:Order.status",
                "connections": [
                    {"line": "condition", "to": "noApp", "fields": {"when": "アプリが無い"}},
                    {"line": "else", "to": "done"}
                ]},
            "Charts/Shop:Draw.noApp": {"kind": "end", "outcome": "failure"},
            "Charts/Shop:Draw.done": {"kind": "end", "outcome": "success"}
        }}),
    );
    assert_eq!(made["created"].as_array().map(Vec::len), Some(6), "{made}");
    let no_app = ok(
        &core,
        "card_read",
        json!({"target": "Charts/Shop:Draw.noApp"}),
    );
    assert_eq!(
        no_app["referencedBy"],
        json!(["Charts/Shop:Draw.answered"]),
        "{no_app}"
    );
}

#[test]
fn a_compute_row_says_what_it_works_out_and_takes_nothing_else() {
    let core = Core::open(script_store()).unwrap();
    ok(
        &core,
        "card_create",
        json!({"value": {
            "Charts/Shop:Tally": {"kind": "flow"},
            "Charts/Shop:Tally.start": {"kind": "begin", "trigger": "manual", "connections": [{"line": "next", "to": "count"}]},
            "Charts/Shop:Tally.count": {"kind": "process", "steps": [
                {"do": "compute", "description": "在庫と注文の数を突き合わせる"}, {"do": "compute"}],
                "connections": [{"line": "next", "to": "done"}]},
            "Charts/Shop:Tally.done": {"kind": "end", "outcome": "success"}
        }}),
    );
    // Over the flow itself in the sequence diagram: what the row says it does, or its word.
    let chart = sequence(&core, "Charts/Shop:Tally");
    assert!(
        chart.contains("Note over P1: 在庫と注文の数を突き合わせる"),
        "{chart}"
    );
    assert!(chart.contains("Note over P1: compute"), "{chart}");
    // In the flowchart, its word alone: it goes to no one.
    let flowchart = ok(
        &core,
        "card_read",
        json!({"target": "Charts/Shop:Tally", "value": {"as": "diagram", "shape": "flowchart"}}),
    );
    let flowchart = text_in(&flowchart, "mermaid");
    assert!(flowchart.contains("compute"), "{flowchart}");
    assert!(!flowchart.contains("compute 開始の相手"), "{flowchart}");
    // What it does is its description: what it would pass or read with is not its.
    for (row, key) in [
        (
            json!({"do": "compute", "paths": ["Schemas/Ordering:Order"]}),
            "paths",
        ),
        (json!({"do": "compute", "program": "calc"}), "program"),
    ] {
        let said = refused(
            &core,
            "card_update",
            json!({"target": "Charts/Shop:Tally.count", "value": {"steps": [row]}}),
        );
        assert!(said.contains(key), "{key}: {said}");
    }
    let said = refused(
        &core,
        "card_update",
        json!({"target": "Charts/Shop:Tally.count", "value": {"connections": {"add": [
            {"line": "partner", "from": "steps[0]", "to": "Charts/Shop:Checkout"}]}}}),
    );
    assert!(said.contains("partner"), "{said}");
}

/// A door three flows send to and a program outside the project, its end answering two of them
/// and a program, and a row that sends to two: each written with its lines, as many as it has.
fn make_login(core: &Core) {
    ok(
        core,
        "card_create",
        json!({"value": {
            "Charts/Shop:Client": {"kind": "flow"},
            "Charts/Shop:Agent": {"kind": "flow"},
            "Charts/Shop:Mcp": {"kind": "flow"},
            "Charts/Shop:Login": {"kind": "flow"},
            "Charts/Shop:Login.start": {"kind": "begin", "trigger": "receive", "program": "外の監視",
                "carries": ["Schemas/Ordering:Order.id"],
                "connections": [
                    {"line": "partner", "to": "Charts/Shop:Client"},
                    {"line": "partner", "to": "Charts/Shop:Agent"},
                    {"line": "partner", "to": "Charts/Shop:Mcp"},
                    {"line": "next", "to": "tell"}]},
            "Charts/Shop:Login.tell": {"kind": "process",
                "steps": [{"do": "send", "carries": ["Schemas/Ordering:Order.total"]}],
                "connections": [
                    {"line": "partner", "from": "steps[0]", "to": "Charts/Shop:Client"},
                    {"line": "partner", "from": "steps[0]", "to": "Charts/Shop:Agent"},
                    {"line": "next", "to": "done"}]},
            "Charts/Shop:Login.done": {"kind": "end", "outcome": "success",
                "return": {"carries": ["Schemas/Ordering:Order.status"], "program": "記録"},
                "connections": [
                    {"line": "partner", "to": "Charts/Shop:Client"},
                    {"line": "partner", "to": "Charts/Shop:Agent"}]}
        }}),
    );
}

#[test]
fn any_caller_is_only_for_a_receiving_begin_with_no_partner() {
    let core = Core::open(script_store()).unwrap();
    ok(
        &core,
        "card_create",
        json!({"value": {
            "Charts/Shop:Status": {"kind": "flow"},
            "Charts/Shop:Status.start": {"kind": "begin", "trigger": "receive",
                "connections": [{"line": "next", "to": "done"}]},
            "Charts/Shop:Status.done": {"kind": "end", "outcome": "success"},
            "Charts/Shop:Tick": {"kind": "flow"},
            "Charts/Shop:Tick.start": {"kind": "begin", "trigger": "manual",
                "connections": [{"line": "next", "to": "done"}]},
            "Charts/Shop:Tick.done": {"kind": "end", "outcome": "success"}
        }}),
    );
    ok(
        &core,
        "card_update",
        json!({"target": "Charts/Shop:Status.start", "value": {"anyCaller": true}}),
    );
    let start = ok(
        &core,
        "card_read",
        json!({"target": "Charts/Shop:Status.start"}),
    );
    assert_eq!(start["anyCaller"], true, "{start}");
    // Anyone, or the partners named: one or the other.
    let alone = "start（開始）の anyCaller と相手（partner の線・program）は一緒に書けません。どちらかにします";
    let with_peer = refused(
        &core,
        "card_update",
        json!({"target": "Charts/Shop:Status.start", "value": {"connections": {
            "add": [{"line": "partner", "to": "Charts/Shop:Checkout"}]}}}),
    );
    assert!(with_peer.contains(alone), "{with_peer}");
    let with_program = refused(
        &core,
        "card_update",
        json!({"target": "Charts/Shop:Status.start", "value": {"program": "外の監視"}}),
    );
    assert!(with_program.contains(alone), "{with_program}");
    let manual = refused(
        &core,
        "card_update",
        json!({"target": "Charts/Shop:Tick.start", "value": {"anyCaller": true}}),
    );
    assert!(
        manual.contains("anyCaller: trigger が receive のときだけ書けます"),
        "{manual}"
    );
}

#[test]
fn a_begin_takes_from_many_flows_and_a_program_together_and_is_read_back_whole() {
    let core = Core::open(script_store()).unwrap();
    make_login(&core);
    let start = ok(
        &core,
        "card_read",
        json!({"target": "Charts/Shop:Login.start"}),
    );
    assert_eq!(start["program"], "外の監視");
    assert_eq!(
        lines(&start),
        [
            ("partner".into(), "".into(), "Charts/Shop:Client".into()),
            ("partner".into(), "".into(), "Charts/Shop:Agent".into()),
            ("partner".into(), "".into(), "Charts/Shop:Mcp".into()),
            ("next".into(), "".into(), "Charts/Shop:Login.tell".into())
        ]
    );
    let tell = ok(
        &core,
        "card_read",
        json!({"target": "Charts/Shop:Login.tell"}),
    );
    assert_eq!(
        lines(&tell)[..2],
        [
            (
                "partner".into(),
                "steps[0]".into(),
                "Charts/Shop:Client".into()
            ),
            (
                "partner".into(),
                "steps[0]".into(),
                "Charts/Shop:Agent".into()
            )
        ]
    );
    let done = ok(
        &core,
        "card_read",
        json!({"target": "Charts/Shop:Login.done"}),
    );
    assert_eq!(done["return"]["program"], "記録", "{done}");
    assert_eq!(
        lines(&done),
        [
            ("partner".into(), "".into(), "Charts/Shop:Client".into()),
            ("partner".into(), "".into(), "Charts/Shop:Agent".into())
        ]
    );
    // The window reads every line of each node card, in their order.
    let start = inner_of(&core, "Login")
        .into_iter()
        .find(|n| n["name"] == "start")
        .unwrap();
    let peers = start["connections"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|l| l["line"] == "partner")
        .count();
    assert_eq!(peers, 3, "{start}");
}

#[test]
fn two_peer_lines_from_one_place_to_one_flow_are_refused() {
    let core = Core::open(script_store()).unwrap();
    make_login(&core);
    // Said apart, they would still name the one flow twice.
    let said = refused(
        &core,
        "card_create",
        json!({"value": {
            "Charts/Shop:Twice": {"kind": "flow"},
            "Charts/Shop:Twice.start": {"kind": "begin", "trigger": "receive",
                "connections": [
                    {"line": "partner", "to": "Charts/Shop:Client"},
                    {"line": "partner", "to": "Charts/Shop:Client", "description": "もう 1 本"},
                    {"line": "next", "to": "tell"}]},
            "Charts/Shop:Twice.tell": {"kind": "process", "steps": [{"do": "send"}],
                "connections": [
                    {"line": "partner", "from": "steps[0]", "to": "Charts/Shop:Agent"},
                    {"line": "partner", "from": "steps[0]", "to": "Charts/Shop:Agent", "description": "もう 1 本"},
                    {"line": "next", "to": "done"}]},
            "Charts/Shop:Twice.done": {"kind": "end", "outcome": "success"}
        }}),
    );
    for part in [
        "Charts/Shop:Twice.start から Charts/Shop:Client への相手（peer）の線が 2 本あります",
        "Charts/Shop:Twice.tell の steps[0] から Charts/Shop:Agent への相手（peer）の線が 2 本あります",
    ] {
        assert!(said.contains(part), "{part}\n---\n{said}");
    }
    // The same flow from another place is another exchange.
    ok(
        &core,
        "card_update",
        json!({"target": "Charts/Shop:Login.tell", "value": {
            "steps": [{"do": "send", "carries": ["Schemas/Ordering:Order.total"]}, {"do": "receive"}],
            "connections": {"add": [{"line": "partner", "from": "steps[1]", "to": "Charts/Shop:Client"}]}}}),
    );
}

#[test]
fn a_call_row_names_a_program_alone_and_a_send_row_needs_someone_to_send_to() {
    let core = Core::open(script_store()).unwrap();
    make_login(&core);
    let row =
        |steps: Value, lines: Value| whole_rows(&core, "Charts/Shop:Login.tell", steps, lines);
    // A call waits for a program; a flow is called and waited for by a run.
    let said = refused(
        &core,
        "card_update",
        row(
            json!([{"do": "call", "program": "git"}]),
            json!([{"line": "partner", "from": "steps[0]", "to": "Charts/Shop:Client"}]),
        ),
    );
    assert!(said.contains("実行ノード"), "{said}");
    let said = refused(
        &core,
        "card_update",
        row(json!([{"do": "call"}]), json!([])),
    );
    assert!(said.contains("steps[0].program: 要ります"), "{said}");
    // A send goes to someone: flows, a program, or both.
    let said = refused(
        &core,
        "card_update",
        row(json!([{"do": "send"}]), json!([])),
    );
    assert!(said.contains("（send）には相手が要ります"), "{said}");
    ok(
        &core,
        "card_update",
        row(
            json!([{"do": "send", "program": "通知"}]),
            json!([
                {"line": "partner", "from": "steps[0]", "to": "Charts/Shop:Client"},
                {"line": "partner", "from": "steps[0]", "to": "Charts/Shop:Mcp"}
            ]),
        ),
    );
    // A receive naming no one takes from whoever called.
    ok(
        &core,
        "card_update",
        row(json!([{"do": "receive"}]), json!([])),
    );
}

/// A process node's rows written as `steps` with its row lines as `lines`, its way on kept.
fn whole_rows(core: &Core, node: &str, steps: Value, lines: Value) -> Value {
    let mut after = lines.as_array().unwrap().clone();
    let before = ok(core, "card_read", json!({"target": node}))["connections"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    after.extend(before.into_iter().filter(|l| l["line"] == "next"));
    let mut value = whole(core, node, Value::Array(after));
    value["value"]["steps"] = steps;
    value
}

#[test]
fn a_sequence_draws_a_way_for_each_sender_and_an_arrow_for_each_flow_sent_to() {
    let core = Core::open(script_store()).unwrap();
    make_login(&core);
    let chart = sequence(&core, "Charts/Shop:Login");
    for said in [
        "participant P1 as Charts/Shop（Login）",
        "participant P2 as Charts/Shop（Client）",
        "participant P3 as Charts/Shop（Agent）",
        "participant P4 as Charts/Shop（Mcp）",
        "participant P5 as 外の監視",
        "participant P6 as 記録",
    ] {
        assert!(chart.contains(said), "{said}\n---\n{chart}");
    }
    // One of them sends at a time: a way of its own for each.
    assert_eq!(
        lines_from(&chart, "alt Client から", 9),
        [
            "alt Client から",
            "P2->>P1: receive Order.id",
            "else Agent から",
            "P3->>P1: receive Order.id",
            "else Mcp から",
            "P4->>P1: receive Order.id",
            "else 外の監視 から",
            "P5->>P1: receive Order.id",
            "end",
        ],
        "{chart}"
    );
    // A send goes to each, one after another.
    assert_eq!(
        lines_from(&chart, "P1->>P2: send", 2),
        ["P1->>P2: send Order.total", "P1->>P3: send Order.total"],
        "{chart}"
    );
    assert_eq!(
        lines_from(&chart, "alt Client へ", 7),
        [
            "alt Client へ",
            "P1-->>P2: return Order.status · success",
            "else Agent へ",
            "P1-->>P3: return Order.status · success",
            "else 記録 へ",
            "P1-->>P6: return Order.status · success",
            "end",
        ],
        "{chart}"
    );
    // One is drawn as it always was: its arrow alone.
    ok(
        &core,
        "card_create",
        json!({"value": {
            "Charts/Shop:Solo": {"kind": "flow"},
            "Charts/Shop:Solo.start": {"kind": "begin", "trigger": "receive", "carries": ["Schemas/Ordering:Order.id"],
                "connections": [{"line": "partner", "to": "Charts/Shop:Client"}, {"line": "next", "to": "done"}]},
            "Charts/Shop:Solo.done": {"kind": "end", "outcome": "success", "return": {}}
        }}),
    );
    let chart = sequence(&core, "Charts/Shop:Solo");
    assert!(!chart.contains("alt "), "{chart}");
    assert!(chart.contains("P2->>P1: receive Order.id"), "{chart}");
    // Its end names no one: it answers the one its begin receives from.
    assert!(chart.contains("P1-->>P2: return success"), "{chart}");
    assert!(!chart.contains("participant P3"), "{chart}");
}

#[test]
fn a_flows_page_and_its_flowchart_name_every_partner_joined_with_a_dot() {
    let core = Core::open(script_store()).unwrap();
    make_login(&core);
    let (_, text) = markdown(&core, "Charts/Shop:Login");
    for part in [
        "- 相手: [[Charts/Shop:Client]]・[[Charts/Shop:Agent]]・[[Charts/Shop:Mcp]]・`外の監視`\n",
        "| `steps[0]` | send | [[Charts/Shop:Client]]・[[Charts/Shop:Agent]] | [[Schemas/Ordering:Order.total]] |  |",
        "- 返す相手: [[Charts/Shop:Client]]・[[Charts/Shop:Agent]]・`記録`\n",
    ] {
        assert!(text.contains(part), "{part}\n---\n{text}");
    }
    let (failed, flowchart) = call(
        &core,
        "card_read",
        json!({"project": PROJECT, "target": "Charts/Shop:Login", "value": {"as": "diagram", "shape": "flowchart"}}),
    );
    assert!(!failed, "{flowchart}");
    for part in [
        "開始: Charts/Shop:Client・Charts/Shop:Agent・Charts/Shop:Mcp・外の監視 から受ける（Schemas/Ordering:Order.id）",
        "send Charts/Shop:Client・Charts/Shop:Agent: Schemas/Ordering:Order.total",
        "終了: success（Charts/Shop:Client・Charts/Shop:Agent・記録 に返す: Schemas/Ordering:Order.status）",
    ] {
        assert!(flowchart.contains(part), "{part}\n---\n{flowchart}");
    }
}
