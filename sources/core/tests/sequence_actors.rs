//! A flow's sequence diagram with actors (`specboard://SpecBoard/Docs/Design:Overview` §11
//! 「シーケンス図の生命線と帯」): its lifelines are the actors as the flow sees them — at the depth
//! `lifelines` asks for — the stores, and a lane for each kind of what is not written (the partner,
//! the store), or for anyone where the begin takes any caller; its process and run
//! nodes are bars with their rows' arrows inside, each arrow saying what it does and its data alone;
//! the start and the end are always drawn, a way that only fails is one mark. An actor's chart frames
//! each flow it provides; the window's request `sequence_chart` gives the same chart as data.

use crate::actor_sections::shop_core;
use crate::common;

use common::{headers, mcp, ok, op, project_id, text_in};
use serde_json::{Value, json};
use specboard_core::{App, Core};
use std::sync::Arc;

const TAKE_ORDER: &str = "Charts/注文サーバ/受付:TakeOrder";

fn sequence(core: &Core, flow: &str, lifelines: Option<Value>) -> String {
    let mut value = json!({"as": "diagram", "shape": "sequence"});
    if let Some(l) = lifelines {
        value["lifelines"] = l;
    }
    text_in(
        &ok(core, "card_read", json!({"target": flow, "value": value})),
        "mermaid",
    )
}

/// The participants a diagram draws, by their labels, in order.
fn participants(chart: &str) -> Vec<String> {
    chart
        .lines()
        .map(str::trim)
        .filter_map(|l| {
            let rest = l
                .strip_prefix("participant ")
                .or_else(|| l.strip_prefix("actor "))?;
            Some(rest.split_once(" as ")?.1.to_string())
        })
        .collect()
}

/// The alias (`P3`) of the participant drawn as `label`.
fn alias(chart: &str, label: &str) -> String {
    let at = participants(chart)
        .iter()
        .position(|p| p == label)
        .unwrap_or_else(|| panic!("{label} の参加者がありません: {chart}"));
    format!("P{}", at + 1)
}

/// The diagram's lines past its participants, without their indent.
fn body(chart: &str) -> Vec<String> {
    chart
        .lines()
        .map(str::trim)
        .filter(|l| {
            !l.is_empty()
                && *l != "sequenceDiagram"
                && !l.starts_with("participant ")
                && !l.starts_with("actor ")
                && !l.starts_with("box ")
                && !l.starts_with("%%")
        })
        .skip_while(|l| *l == "end")
        .map(str::to_string)
        .collect()
}

#[test]
fn lifelines_are_the_actors_seen_from_the_flow() {
    let core = shop_core();
    let chart = sequence(&core, TAKE_ORDER, None);
    let drawn = participants(&chart);
    assert_eq!(
        drawn[..5],
        ["ブラウザ", "受付", "在庫", "決済サービス", "DB"],
        "{chart}"
    );
    for shape in ["Product", "Order", "Line", "Stock", "Receipt"] {
        assert!(
            !drawn.iter().any(|p| p.starts_with(shape)),
            "{shape}\n---\n{chart}"
        );
    }
    let lines: Vec<&str> = chart.lines().map(str::trim).collect();
    let at = lines.iter().position(|l| *l == "box 注文サーバ").unwrap();
    assert_eq!(
        lines[at + 1..at + 4],
        ["participant P2 as 受付", "participant P3 as 在庫", "end"],
        "{chart}"
    );
    let stock = alias(&chart, "在庫");
    let run = lines
        .iter()
        .find(|l| l.starts_with(&format!("P2->>{stock}:")))
        .unwrap_or_else(|| panic!("{chart}"));
    assert!(run.contains(": run ") && run.contains("Reserve"), "{run}");
}

/// 受付:Confirm — receives, sends the payment on to the outside service, succeeds — and a run node of
/// it in TakeOrder right after `check`, on the order desk's own lifeline.
fn confirm_after_check(core: &Core) {
    ok(
        core,
        "card_create",
        json!({"value": {
            "Charts/注文サーバ/受付:Confirm": {"kind": "flow", "label": "確かめる"},
            "Charts/注文サーバ/受付:Confirm.start": {"kind": "begin", "trigger": "receive",
                "connections": [{"line": "next", "to": "pay"}]},
            "Charts/注文サーバ/受付:Confirm.pay": {"kind": "process", "steps": [{"do": "send", "carries": ["支払い"]}],
                "connections": [
                    {"line": "partner", "from": "steps[0]", "to": "Charts/決済サービス:Charge"},
                    {"line": "next", "to": "done"}]},
            "Charts/注文サーバ/受付:Confirm.done": {"kind": "end", "outcome": "success"},
            "Charts/注文サーバ/受付:TakeOrder.confirm": {"kind": "run",
                "connections": [
                    {"line": "runs", "to": "Charts/注文サーバ/受付:Confirm"},
                    {"line": "next", "to": "enough"}]}
        }}),
    );
    ok(
        core,
        "card_update",
        json!({"target": "Charts/注文サーバ/受付:TakeOrder.check", "value": {"connections": {
            "add": [{"line": "next", "to": "confirm"}],
            "remove": [{"line": "next", "to": "enough"}]
        }}}),
    );
}

/// 受付:Notify — receives, writes the order's total, succeeds — run by Confirm between `pay` and `done`.
fn notify_in_confirm(core: &Core) {
    ok(
        core,
        "card_create",
        json!({"value": {
            "Charts/注文サーバ/受付:Notify": {"kind": "flow", "label": "知らせる"},
            "Charts/注文サーバ/受付:Notify.start": {"kind": "begin", "trigger": "receive",
                "connections": [{"line": "next", "to": "tell"}]},
            "Charts/注文サーバ/受付:Notify.tell": {"kind": "process", "steps": [{"do": "write"}],
                "connections": [
                    {"line": "writes", "from": "steps[0]", "to": "Schemas/Shop:Order", "at": "total"},
                    {"line": "next", "to": "done"}]},
            "Charts/注文サーバ/受付:Notify.done": {"kind": "end", "outcome": "success"},
            "Charts/注文サーバ/受付:Confirm.notify": {"kind": "run",
                "connections": [
                    {"line": "runs", "to": "Charts/注文サーバ/受付:Notify"},
                    {"line": "next", "to": "done"}]}
        }}),
    );
    ok(
        core,
        "card_update",
        json!({"target": "Charts/注文サーバ/受付:Confirm.pay", "value": {"connections": {
            "add": [{"line": "next", "to": "notify"}],
            "remove": [{"line": "next", "to": "done"}]
        }}}),
    );
}

/// The lines of a flow's chart, from the line `first` on (without their indent).
fn from_line(chart: &str, first: &str) -> Vec<String> {
    let lines = body(chart);
    let at = lines
        .iter()
        .position(|l| l == first)
        .unwrap_or_else(|| panic!("{first} の行がありません: {chart}"));
    lines[at..].to_vec()
}

/// The window's `sequence_chart` of a flow, with the keys of the deeper calls opened.
fn chart_of(core: &Core, card: &str, open: &[String]) -> Value {
    op(
        core,
        json!({"op": "sequence_chart", "project": project_id(core), "branch": "", "card": card,
            "open": open}),
    )["result"]
        .clone()
}

/// The bar the chart draws for the flow `flow` called on its own lifeline.
fn call_of(chart: &Value, flow: &str) -> Value {
    every_item(&chart["items"])
        .into_iter()
        .find(|i| i["step"]["flow"] == flow && i["step"].get("node").is_none())
        .unwrap_or_else(|| panic!("{flow} の呼ぶ所がありません: {chart}"))["step"]
        .clone()
}

#[test]
fn a_call_into_a_flow_of_the_same_lifeline_is_a_bar_on_it() {
    let core = shop_core();
    ok(
        &core,
        "card_create",
        json!({"value": {
            "Charts/注文サーバ/受付:Confirm": {"kind": "flow", "label": "確かめる"},
            "Charts/注文サーバ/受付:Confirm.start": {"kind": "begin", "trigger": "receive",
                "connections": [{"line": "next", "to": "done"}]},
            "Charts/注文サーバ/受付:Confirm.done": {"kind": "end", "outcome": "success"},
            "Charts/注文サーバ/受付:TakeOrder.confirm": {"kind": "run",
                "connections": [
                    {"line": "runs", "to": "Charts/注文サーバ/受付:Confirm"},
                    {"line": "next", "to": "enough"}]}
        }}),
    );
    ok(
        &core,
        "card_update",
        json!({"target": "Charts/注文サーバ/受付:TakeOrder.check", "value": {"connections": {
            "add": [{"line": "next", "to": "confirm"}],
            "remove": [{"line": "next", "to": "enough"}]
        }}}),
    );
    let chart = sequence(&core, TAKE_ORDER, None);
    let me = alias(&chart, "受付");
    let lines = body(&chart);
    let at = lines
        .iter()
        .position(|l| *l == format!("Note over {me}: confirm"))
        .unwrap_or_else(|| panic!("{chart}"));
    // The bar names the flow called and holds nothing of what it does.
    assert_eq!(
        lines[at..at + 6],
        [
            format!("Note over {me}: confirm"),
            format!("activate {me}"),
            format!("Note over {me}: Confirm 確かめる"),
            format!("activate {me}"),
            format!("deactivate {me}"),
            format!("deactivate {me}"),
        ],
        "{chart}"
    );
    assert!(
        !lines
            .iter()
            .any(|l| l.contains("->>") && l.contains("Confirm")),
        "{chart}"
    );

    // Seen as the one server, running the stock's flow is a bar on it too, as empty.
    let chart = sequence(&core, TAKE_ORDER, Some(json!(["Charts/注文サーバ"])));
    let server = alias(&chart, "注文サーバ");
    let lines = body(&chart);
    let at = lines
        .iter()
        .position(|l| *l == format!("Note over {server}: Reserve 在庫を押さえる"))
        .unwrap_or_else(|| panic!("{chart}"));
    assert!(
        !participants(&chart).contains(&"在庫".to_string()),
        "{chart}"
    );
    assert_eq!(lines[at + 1], format!("activate {server}"), "{chart}");
    assert_eq!(lines[at + 2], format!("deactivate {server}"), "{chart}");
    assert!(
        !chart.contains("write Stock.reserved"),
        "Reserve の中身が書かれている: {chart}"
    );
}

#[test]
fn a_flow_called_on_its_own_lifeline_is_drawn_inside_the_call() {
    let core = shop_core();
    confirm_after_check(&core);
    let chart = sequence(&core, TAKE_ORDER, None);
    let me = alias(&chart, "受付");
    // Written out, the bar of Confirm names the flow and holds nothing of what it does.
    let from = from_line(&chart, &format!("Note over {me}: Confirm 確かめる"));
    assert_eq!(
        from[..3],
        [
            format!("Note over {me}: Confirm 確かめる"),
            format!("activate {me}"),
            format!("deactivate {me}"),
        ],
        "{chart}"
    );
    assert!(
        !chart.contains("return TakeOrder 注文を受ける"),
        "呼んだ側への返す帯がある: {chart}"
    );

    // The window asks for it: without a key the call is a folded line that says its key; with the key
    // the bar holds what Confirm does, ending in its own note, not an arrow back to TakeOrder.
    let confirm = card_id(&core, "Confirm");
    let take = card_id(&core, "TakeOrder");
    let closed = chart_of(&core, &take, &[]);
    let call = call_of(&closed, &confirm);
    assert_eq!(call["op"], "run", "{call}");
    assert_eq!(call["folded"], true, "{call}");
    assert_eq!(call["items"], json!([]), "{call}");
    assert!(call["call"].is_string(), "{call}");
    assert!(
        call.get("level").is_none() && call.get("cycle").is_none(),
        "{call}"
    );

    let key = call["call"].as_str().unwrap().to_string();
    let opened = chart_of(&core, &take, &[key]);
    let call = call_of(&opened, &confirm);
    assert!(call.get("folded").is_none(), "{call}");
    let inside = every_item(&call["items"]);
    let send = inside
        .iter()
        .find(|i| i["message"]["op"] == "send")
        .unwrap_or_else(|| panic!("{call}"));
    assert_eq!(
        send["message"]["origin"],
        json!({"card": confirm, "node": "pay", "row": 0}),
        "{send}"
    );
    let end = inside
        .iter()
        .find(|i| i["note"]["kind"] == "end")
        .unwrap_or_else(|| panic!("{call}"));
    assert_eq!(end["note"]["text"], "success", "{end}");
    assert!(
        !inside.iter().any(|i| i["message"]["reply"] == true),
        "呼んだ側へ返す矢印がある: {call}"
    );
}

#[test]
fn deeper_calls_are_drawn_folded_until_the_request_opens_them() {
    let core = shop_core();
    confirm_after_check(&core);
    notify_in_confirm(&core);
    let take = card_id(&core, "TakeOrder");
    let (confirm, notify) = (card_id(&core, "Confirm"), card_id(&core, "Notify"));

    // Asked with no key, the call into Confirm is a folded line and Notify, inside it, is not drawn.
    let closed = chart_of(&core, &take, &[]);
    let outer = call_of(&closed, &confirm);
    assert_eq!(outer["folded"], true, "{outer}");
    assert_eq!(outer["items"], json!([]), "{outer}");
    let outer_key = outer["call"].as_str().expect("鍵がありません").to_string();
    assert!(
        every_item(&closed["items"])
            .iter()
            .all(|i| i["step"]["flow"] != notify.as_str()),
        "Notify が図にある: {closed}"
    );

    // Confirm's key opens it, and the call into Notify inside is a folded line again.
    let first = chart_of(&core, &take, std::slice::from_ref(&outer_key));
    assert!(
        call_of(&first, &confirm).get("folded").is_none(),
        "Confirm が開かない"
    );
    let deeper = call_of(&first, &notify);
    assert_eq!(deeper["folded"], true, "{deeper}");
    assert_eq!(deeper["items"], json!([]), "{deeper}");
    let key = deeper["call"].as_str().expect("鍵がありません").to_string();

    let opened = chart_of(&core, &take, &[outer_key.clone(), key.clone()]);
    let deeper = call_of(&opened, &notify);
    assert!(deeper.get("folded").is_none(), "{deeper}");
    let write = every_item(&deeper["items"])
        .into_iter()
        .find(|i| i["message"]["op"] == "write")
        .unwrap_or_else(|| panic!("{deeper}"));
    assert!(
        write["message"]["text"]
            .as_str()
            .unwrap()
            .contains("Order.total"),
        "{write}"
    );
    assert_eq!(
        write["message"]["origin"],
        json!({"card": notify, "node": "tell", "row": 0}),
        "{write}"
    );
    assert!(
        call_of(&opened, &confirm).get("folded").is_none(),
        "Confirm が畳まれた"
    );

    // A key for the inner call alone opens nothing: its Confirm is not opened, so Notify is not built.
    let alone = chart_of(&core, &take, &[key]);
    assert_eq!(call_of(&alone, &confirm)["folded"], true, "{alone}");
    assert!(
        every_item(&alone["items"])
            .iter()
            .all(|i| i["step"]["flow"] != notify.as_str()),
        "Notify が図にある: {alone}"
    );

    // Written out, no call opens: the bars say no more than their names.
    let chart = sequence(&core, TAKE_ORDER, None);
    let me = alias(&chart, "受付");
    let from = from_line(&chart, &format!("Note over {me}: Confirm 確かめる"));
    assert_eq!(
        from[..3],
        [
            format!("Note over {me}: Confirm 確かめる"),
            format!("activate {me}"),
            format!("deactivate {me}"),
        ],
        "{chart}"
    );
    assert!(!chart.contains("Notify 知らせる"), "{chart}");
}

#[test]
fn a_flow_being_drawn_above_is_not_opened_again() {
    let core = shop_core();
    confirm_after_check(&core);
    ok(
        &core,
        "card_create",
        json!({"value": {
            "Charts/注文サーバ/受付:Confirm.again": {"kind": "run",
                "connections": [
                    {"line": "runs", "to": "Charts/注文サーバ/受付:TakeOrder"},
                    {"line": "next", "to": "done"}]},
            "Charts/注文サーバ/受付:Loop": {"kind": "flow", "label": "繰り返す"},
            "Charts/注文サーバ/受付:Loop.start": {"kind": "begin", "trigger": "manual",
                "connections": [{"line": "next", "to": "self"}]},
            "Charts/注文サーバ/受付:Loop.self": {"kind": "run",
                "connections": [
                    {"line": "runs", "to": "Charts/注文サーバ/受付:Loop"},
                    {"line": "next", "to": "done"}]},
            "Charts/注文サーバ/受付:Loop.done": {"kind": "end", "outcome": "success"}
        }}),
    );
    ok(
        &core,
        "card_update",
        json!({"target": "Charts/注文サーバ/受付:Confirm.pay", "value": {"connections": {
            "add": [{"line": "next", "to": "again"}],
            "remove": [{"line": "next", "to": "done"}]
        }}}),
    );
    let (take, looping) = (card_id(&core, "TakeOrder"), card_id(&core, "Loop"));
    let closed_loop = |chart: &Value, flow: &str| {
        let call = call_of(chart, flow);
        assert_eq!(call["cycle"], true, "{call}");
        assert_eq!(call["items"], json!([]), "{call}");
        assert!(
            call.get("call").is_none() && call.get("folded").is_none(),
            "{call}"
        );
    };
    // The cycle step is inside Confirm, which is drawn only once its key is in `open`.
    let key = call_of(&chart_of(&core, &take, &[]), &card_id(&core, "Confirm"))["call"]
        .as_str()
        .expect("鍵がありません")
        .to_string();
    let drawn = chart_of(&core, &take, &[key]);
    closed_loop(&drawn, &take);
    assert!(drawn.get("tooLarge").is_none(), "{drawn}");
    let drawn = chart_of(&core, &looping, &[]);
    closed_loop(&drawn, &looping);
    assert!(drawn.get("tooLarge").is_none(), "{drawn}");
    for flow in [
        "Charts/注文サーバ/受付:TakeOrder",
        "Charts/注文サーバ/受付:Loop",
    ] {
        assert!(!sequence(&core, flow, None).contains("大きすぎ"));
    }
}

#[test]
fn an_actors_chart_opens_calls_on_one_lifeline_inside_its_frames() {
    let core = shop_core();
    confirm_after_check(&core);
    let chart = actor_chart(&core, "Charts/注文サーバ", None);
    // Run only from inside the order desk, Confirm has no frame of its own, and its bar in the
    // TakeOrder frame is written out as the name alone.
    assert_eq!(frame_heads(&chart), ["TakeOrder 注文を受ける"], "{chart}");
    let me = alias(&chart, "受付");
    let from = from_line(&chart, &format!("Note over {me}: Confirm 確かめる"));
    assert_eq!(
        from[..3],
        [
            format!("Note over {me}: Confirm 確かめる"),
            format!("activate {me}"),
            format!("deactivate {me}"),
        ],
        "{chart}"
    );

    let server = section_id(&core, "Charts/注文サーバ");
    let chart_of_section = |open: &[String]| {
        op(
            &core,
            json!({"op": "sequence_chart", "project": project_id(&core), "branch": "", "section": server,
                "open": open}),
        )["result"]
            .clone()
    };
    let confirm = card_id(&core, "Confirm");
    let call_in_frame = |data: &Value| {
        let frame = data["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|i| i["block"]["kind"] == "flow")
            .unwrap_or_else(|| panic!("{data}"));
        call_of(
            &json!({"items": frame["block"]["sections"][0]["items"]}),
            &confirm,
        )
    };
    let call = call_in_frame(&chart_of_section(&[]));
    assert_eq!(call["folded"], true, "{call}");
    assert_eq!(call["items"], json!([]), "{call}");
    let key = call["call"].as_str().expect("鍵がありません").to_string();

    let opened = chart_of_section(&[key]);
    let call = call_in_frame(&opened);
    assert!(call.get("folded").is_none(), "{call}");
    let inside = every_item(&call["items"]);
    let charge = inside
        .iter()
        .find(|i| i["message"]["op"] == "send")
        .unwrap_or_else(|| panic!("{call}"));
    let label_of = |at: &Value| {
        opened["participants"][at.as_u64().unwrap() as usize]["name"]
            .as_str()
            .unwrap()
            .to_string()
    };
    assert_eq!(label_of(&charge["message"]["from"]), "受付", "{charge}");
    assert_eq!(
        label_of(&charge["message"]["to"]),
        "決済サービス",
        "{charge}"
    );
    assert_eq!(charge["message"]["text"], "支払い", "{charge}");
    assert!(
        inside
            .iter()
            .any(|i| i["note"]["kind"] == "end" && i["note"]["text"] == "success"),
        "終わりの注がありません: {call}"
    );
}

#[test]
fn lifelines_given_choose_the_depth() {
    let core = shop_core();
    let chart = sequence(&core, TAKE_ORDER, Some(json!(["Charts/注文サーバ"])));
    assert_eq!(
        participants(&chart)[..4],
        ["ブラウザ", "注文サーバ", "決済サービス", "DB"],
        "{chart}"
    );
    // What holds no partner deeper than the default changes nothing.
    assert_eq!(
        sequence(
            &core,
            TAKE_ORDER,
            Some(json!(["Charts/決済サービス", "Charts/注文サーバ/受付"]))
        ),
        sequence(&core, TAKE_ORDER, None)
    );
    let refused = mcp(
        &core,
        "card_read",
        json!({"target": TAKE_ORDER, "value": {"as": "diagram", "shape": "sequence", "lifelines": ["Charts/シナリオ"]}}),
    )
    .unwrap_err();
    assert_eq!(
        refused,
        "Charts/シナリオ はアクターのセクションではありません"
    );
}

#[test]
fn a_begin_naming_no_one_is_drawn_from_the_partner_gap() {
    let core = shop_core();
    let chart = sequence(&core, "Charts/注文サーバ/在庫:Reserve", None);
    assert_eq!(participants(&chart)[0], "相手が書かれていない", "{chart}");
    assert_eq!(
        body(&chart)[0],
        format!("P1->>{}: receive", alias(&chart, "在庫")),
        "{chart}"
    );
    // The one flow that runs it is not taken for its partner.
    assert!(
        !participants(&chart).contains(&"受付".to_string()),
        "{chart}"
    );
}

/// 受付:OrderStatus, which anyone may call: it receives the receipt number and answers with the
/// order's state.
fn order_status(core: &Core) {
    ok(
        core,
        "card_create",
        json!({"value": {
            "Charts/注文サーバ/受付:OrderStatus": {"kind": "flow", "label": "注文の状態を返す"},
            "Charts/注文サーバ/受付:OrderStatus.start": {"kind": "begin", "trigger": "receive",
                "anyCaller": true, "carries": ["受付番号"],
                "connections": [{"line": "next", "to": "done"}]},
            "Charts/注文サーバ/受付:OrderStatus.done": {"kind": "end", "outcome": "success",
                "return": {"carries": ["注文の状態"]}}
        }}),
    );
}

#[test]
fn a_begin_for_any_caller_is_drawn_from_anyone() {
    let core = shop_core();
    order_status(&core);
    let chart = sequence(&core, "Charts/注文サーバ/受付:OrderStatus", None);
    assert_eq!(participants(&chart)[0], "誰からでも", "{chart}");
    let me = alias(&chart, "受付");
    let lines = body(&chart);
    assert_eq!(lines[0], format!("P1->>{me}: receive 受付番号"), "{chart}");
    assert!(
        lines.contains(&format!("{me}-->>P1: return 注文の状態 · success")),
        "{chart}"
    );
    assert!(
        !participants(&chart).contains(&"相手が書かれていない".to_string()),
        "{chart}"
    );
    let gaps = ok(
        &core,
        "inspection_list",
        json!({"value": {"group": "missingPartners"}}),
    );
    assert!(
        !gaps["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|i| i["at"] == "Charts/注文サーバ/受付:OrderStatus.start"),
        "{gaps}"
    );
}

#[test]
fn rows_and_ends_naming_no_one_answer_the_begins_partner() {
    let core = shop_core();
    ok(
        &core,
        "card_update",
        json!({"target": "Charts/注文サーバ/受付:TakeOrder.done", "value": {"connections": {
            "remove": [{"line": "partner", "to": "Charts/ブラウザ:CheckoutPage"}]
        }}}),
    );
    ok(
        &core,
        "card_update",
        json!({"target": "Charts/注文サーバ/在庫:Reserve.done", "value": {"return": {"carries": ["押さえた数"]}}}),
    );
    let chart = sequence(&core, TAKE_ORDER, None);
    assert!(
        body(&chart).contains(&format!(
            "{}-->>{}: return 受付番号 · success",
            alias(&chart, "受付"),
            alias(&chart, "ブラウザ")
        )),
        "{chart}"
    );
    assert!(
        !participants(&chart).contains(&"相手が書かれていない".to_string()),
        "{chart}"
    );
    // A begin with no partner: what its end answers goes to the same one lane.
    let chart = sequence(&core, "Charts/注文サーバ/在庫:Reserve", None);
    assert_eq!(
        participants(&chart)
            .iter()
            .filter(|p| *p == "相手が書かれていない")
            .count(),
        1,
        "{chart}"
    );
    let gap = alias(&chart, "相手が書かれていない");
    let stock = alias(&chart, "在庫");
    let lines = body(&chart);
    assert_eq!(lines[0], format!("{gap}->>{stock}: receive"), "{chart}");
    assert!(
        lines.contains(&format!("{stock}-->>{gap}: return 押さえた数 · success")),
        "{chart}"
    );
}

#[test]
fn a_row_naming_no_one_in_a_flow_that_does_not_receive_is_a_gap() {
    let core = shop_core();
    ok(
        &core,
        "card_update",
        json!({"target": "Charts/利用者:PlaceOrder.order", "value": {"steps": [
            {"do": "send", "carries": ["注文の内容"]},
            {"do": "receive", "carries": ["受付番号"]}
        ]}}),
    );
    let chart = sequence(&core, "Charts/利用者:PlaceOrder", None);
    assert!(
        body(&chart).contains(&format!(
            "{}->>{}: receive 受付番号",
            alias(&chart, "相手が書かれていない"),
            alias(&chart, "利用者")
        )),
        "{chart}"
    );
    let gaps = ok(
        &core,
        "inspection_list",
        json!({"value": {"group": "missingPartners"}}),
    );
    let row = gaps["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["at"] == "Charts/利用者:PlaceOrder.order.steps[1]")
        .cloned()
        .unwrap_or_else(|| panic!("{gaps}"));
    assert_eq!(
        row["problem"], "receive の相手が書かれていず、開始も受けません",
        "{row}"
    );
}

#[test]
fn shapes_go_to_the_store_that_keeps_them_or_to_the_store_gap() {
    let core = shop_core();
    let chart = sequence(&core, TAKE_ORDER, None);
    let me = alias(&chart, "受付");
    let lines = body(&chart);
    assert!(
        lines.contains(&format!("{me}->>{}: write Line.qty", alias(&chart, "DB"))),
        "{chart}"
    );
    assert!(
        lines.contains(&format!(
            "{me}->>{}: write Receipt.number",
            alias(&chart, "置き場が書かれていない")
        )),
        "{chart}"
    );
    assert_eq!(
        participants(&chart).last().map(String::as_str),
        Some("置き場が書かれていない"),
        "{chart}"
    );
    // Without the ghost the store is a lifeline of its own.
    ok(&core, "card_delete", json!({"target": "Charts/DB:ShopDB"}));
    let chart = sequence(&core, TAKE_ORDER, None);
    assert!(!participants(&chart).contains(&"DB".to_string()), "{chart}");
    assert!(
        body(&chart).contains(&format!(
            "{}->>{}: write Line.qty",
            alias(&chart, "受付"),
            alias(&chart, "ShopDB（store）")
        )),
        "{chart}"
    );
}

#[test]
fn each_row_is_one_arrow_per_lifeline_with_its_op_and_data_only() {
    let core = shop_core();
    let chart = sequence(&core, TAKE_ORDER, None);
    assert_eq!(
        chart.matches(": write Order.items, total").count(),
        1,
        "{chart}"
    );
    assert!(!chart.contains("Schemas/Shop:"), "{chart}");
    assert!(!chart.contains("注文を作る"), "{chart}");
}

#[test]
fn steps_are_bars_named_by_their_node_and_failure_only_ways_are_marks() {
    let core = shop_core();
    let chart = sequence(&core, TAKE_ORDER, None);
    let lines = body(&chart);
    let at = lines
        .iter()
        .position(|l| l == "Note over P2: check 在庫を確かめる")
        .unwrap_or_else(|| panic!("{chart}"));
    assert_eq!(lines[at + 1], "activate P2", "{chart}");
    assert!(lines[at + 2].starts_with("P2->>P5: read"), "{chart}");
    assert_eq!(lines[at + 3], "deactivate P2", "{chart}");
    assert_eq!(
        lines[at + 4],
        "Note right of P2: ✕ enough: 足りない → failure（→ ブラウザ）",
        "{chart}"
    );
    assert!(
        !lines
            .iter()
            .any(|l| l.starts_with("break ") || l.starts_with("alt ")),
        "{chart}"
    );

    // With something it answers, the way is drawn as before.
    ok(
        &core,
        "card_update",
        json!({"target": "Charts/注文サーバ/受付:TakeOrder.short", "value": {"return": {"carries": ["不足"]},
            "connections": {"add": [{"line": "partner", "to": "Charts/ブラウザ:CheckoutPage"}]}}}),
    );
    let chart = sequence(&core, TAKE_ORDER, None);
    assert!(
        body(&chart)
            .iter()
            .any(|l| l == "break 足りない" || l == "alt 足りない"),
        "{chart}"
    );
    assert!(!chart.contains('✕'), "{chart}");
}

#[test]
fn the_start_and_the_end_are_always_drawn() {
    let core = shop_core();
    let chart = sequence(&core, TAKE_ORDER, None);
    let lines = body(&chart);
    let (browser, me) = (alias(&chart, "ブラウザ"), alias(&chart, "受付"));
    assert_eq!(
        lines.first().unwrap(),
        &format!("{browser}->>{me}: receive 注文の内容"),
        "{chart}"
    );
    assert_eq!(
        lines.last().unwrap(),
        &format!("{me}-->>{browser}: return 受付番号 · success"),
        "{chart}"
    );
    let chart = sequence(&core, "Charts/利用者:PlaceOrder", None);
    assert_eq!(
        body(&chart).last().unwrap(),
        &format!("Note over {}: ■ success", alias(&chart, "利用者")),
        "{chart}"
    );
}

#[test]
fn mermaid_writes_persons_as_actor_and_sibling_actors_in_a_box() {
    let core = shop_core();
    let page = sequence(&core, "Charts/ブラウザ:CheckoutPage", None);
    assert!(page.contains("actor P1 as 利用者"), "{page}");
    let chart = sequence(&core, TAKE_ORDER, None);
    let lines: Vec<&str> = chart.lines().map(str::trim).collect();
    let at = lines.iter().position(|l| *l == "box 注文サーバ").unwrap();
    assert!(lines[at + 1].starts_with("participant "), "{chart}");
    assert!(lines[at + 2].starts_with("participant "), "{chart}");
    assert_eq!(lines[at + 3], "end", "{chart}");
    assert!(!page.contains("@{") && !chart.contains("@{"));
}

/// An app that draws every picture as the same small image.
struct Painter;

impl App for Painter {
    fn app_control(&self, _arguments: &str, _caller: &str) -> String {
        panic!("a picture does not go through app_control")
    }

    fn picture(&self, request: &str) -> String {
        let asked: Value = serde_json::from_str(request).unwrap();
        assert_eq!(asked["picture"], "sequence", "{asked}");
        json!({"image": {"data": "iVBORw0KGgo=", "mimeType": "image/png"},
            "note": {"picture": "sequence", "width": 800, "height": 600, "scale": 1}})
        .to_string()
    }
}

/// The cancelling flows and the restocking one, beside the shop's.
fn more_flows(core: &Core) {
    ok(
        core,
        "card_create",
        json!({"value": {
            "Charts/ブラウザ:CancelPage": {"kind": "flow", "label": "取消画面"},
            "Charts/注文サーバ/受付:CancelOrder": {"kind": "flow", "label": "注文を取り消す"},
            "Charts/注文サーバ/在庫:Release": {"kind": "flow", "label": "在庫を戻す"},
            "Charts/注文サーバ/在庫:Restock": {"kind": "flow", "label": "入荷する"},
            "Charts/ブラウザ:CancelPage.start": {"kind": "begin", "trigger": "manual",
                "connections": [{"line": "next", "to": "ask"}]},
            "Charts/ブラウザ:CancelPage.ask": {"kind": "process", "steps": [{"do": "send"}],
                "connections": [
                    {"line": "partner", "from": "steps[0]", "to": "Charts/注文サーバ/受付:CancelOrder"},
                    {"line": "next", "to": "done"}]},
            "Charts/ブラウザ:CancelPage.done": {"kind": "end", "outcome": "success"},
            "Charts/注文サーバ/受付:CancelOrder.start": {"kind": "begin", "trigger": "receive",
                "connections": [
                    {"line": "partner", "to": "Charts/ブラウザ:CancelPage"},
                    {"line": "next", "to": "release"}]},
            "Charts/注文サーバ/受付:CancelOrder.release": {"kind": "run",
                "connections": [
                    {"line": "runs", "to": "Charts/注文サーバ/在庫:Release"},
                    {"line": "next", "to": "done"}]},
            "Charts/注文サーバ/受付:CancelOrder.done": {"kind": "end", "outcome": "success"},
            "Charts/注文サーバ/在庫:Release.start": {"kind": "begin", "trigger": "receive",
                "connections": [{"line": "next", "to": "put"}]},
            "Charts/注文サーバ/在庫:Release.put": {"kind": "process", "steps": [{"do": "write"}],
                "connections": [
                    {"line": "writes", "from": "steps[0]", "to": "Schemas/Shop:Stock", "at": "reserved"},
                    {"line": "next", "to": "done"}]},
            "Charts/注文サーバ/在庫:Release.done": {"kind": "end", "outcome": "success"},
            "Charts/注文サーバ/在庫:Restock.start": {"kind": "begin", "trigger": "schedule", "every": "毎朝",
                "connections": [{"line": "next", "to": "put"}]},
            "Charts/注文サーバ/在庫:Restock.put": {"kind": "process", "steps": [{"do": "write"}],
                "connections": [
                    {"line": "writes", "from": "steps[0]", "to": "Schemas/Shop:Stock", "at": "reserved"},
                    {"line": "next", "to": "done"}]},
            "Charts/注文サーバ/在庫:Restock.done": {"kind": "end", "outcome": "success"}
        }}),
    );
}

#[test]
fn an_actors_chart_frames_each_flow_it_provides() {
    let core = shop_core();
    more_flows(&core);
    let chart = text_in(
        &ok(
            &core,
            "section_read",
            json!({"target": "Charts/注文サーバ", "value": {"as": "diagram", "shape": "sequence"}}),
        ),
        "mermaid",
    );
    assert_eq!(
        participants(&chart)[..5],
        ["ブラウザ", "受付", "在庫", "決済サービス", "DB"],
        "{chart}"
    );
    let lines: Vec<&str> = chart.lines().map(str::trim).collect();
    let at = lines.iter().position(|l| *l == "box 注文サーバ").unwrap();
    assert_eq!(
        lines[at + 1..at + 4],
        ["participant P2 as 受付", "participant P3 as 在庫", "end"],
        "{chart}"
    );
    // Each frame is a rect headed with its flow.
    let heads: Vec<String> = lines
        .windows(2)
        .filter(|w| w[0].starts_with("rect "))
        .map(|w| w[1].split_once(": ").unwrap().1.to_string())
        .collect();
    assert_eq!(
        heads,
        [
            "TakeOrder 注文を受ける",
            "CancelOrder 注文を取り消す",
            "Restock 入荷する"
        ],
        "{chart}"
    );
    // Run only from inside the actor, Reserve and Release are bars on 在庫's lifeline in the frames
    // that run them.
    let stock = alias(&chart, "在庫");
    let db = alias(&chart, "DB");
    let frame_of = |head: &str| {
        let at = lines
            .iter()
            .position(|l| l.ends_with(&format!(": {head}")))
            .unwrap_or_else(|| panic!("{head}: {chart}"));
        let end = lines[at..]
            .iter()
            .position(|l| l.starts_with("rect "))
            .map_or(lines.len(), |n| at + n);
        lines[at..end].to_vec()
    };
    for (frame, inner) in [
        ("TakeOrder 注文を受ける", "Reserve 在庫を押さえる"),
        ("CancelOrder 注文を取り消す", "Release 在庫を戻す"),
    ] {
        let drawn = frame_of(frame);
        let bar = drawn
            .iter()
            .position(|l| *l == format!("Note over {stock}: {inner}"))
            .unwrap_or_else(|| panic!("{inner} の帯がありません: {drawn:?}"));
        assert_eq!(drawn[bar + 1], format!("activate {stock}"), "{drawn:?}");
        assert!(
            drawn[bar..]
                .iter()
                .any(|l| l.starts_with(&format!("{stock}->>{db}: write Stock.reserved"))),
            "{drawn:?}"
        );
    }

    // Its picture, drawn by the app.
    core.set_app(Arc::new(Painter));
    let message = json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {
        "name": "section_read", "arguments": {"project": common::PROJECT, "target": "Charts/注文サーバ",
        "value": {"as": "picture", "shape": "sequence"}}}});
    let reply = op(
        &core,
        json!({"op": "mcp", "message": message, "headers": headers(&core)}),
    );
    let result = &reply["result"]["body"]["result"];
    assert_eq!(result["isError"], false, "{result}");
    assert_eq!(result["content"][0]["mimeType"], "image/png", "{result}");

    // A plain chart section has no actor's chart.
    let refused = mcp(
        &core,
        "section_read",
        json!({"target": "Charts/シナリオ", "value": {"as": "diagram", "shape": "sequence"}}),
    )
    .unwrap_err();
    assert_eq!(
        refused,
        "shape: sequence はアクターのセクションだけです（Charts/シナリオ はチャートセクションです）"
    );
}

/// The project as its file writes it, where ids are said.
fn file_of(core: &Core) -> Value {
    serde_json::from_str(
        op(
            core,
            json!({"op": "project_file", "project": project_id(core)}),
        )["result"]["text"]
            .as_str()
            .unwrap(),
    )
    .unwrap()
}

/// A card's id in main, by its name (one of its name).
fn card_id(core: &Core, name: &str) -> String {
    file_of(core)["cards"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == name)
        .unwrap_or_else(|| panic!("{name} がありません"))["id"]
        .as_str()
        .unwrap()
        .to_string()
}

/// A section's id, by its path.
fn section_id(core: &Core, path: &str) -> String {
    let file = file_of(core);
    let sections = file["sections"].as_array().unwrap();
    let mut at: Option<String> = sections
        .iter()
        .find(|s| s.get("parent_id").is_none())
        .map(|s| s["id"].as_str().unwrap().to_string());
    for name in path.split('/') {
        at = sections
            .iter()
            .find(|s| s["name"] == name && s["parent_id"].as_str() == at.as_deref())
            .map(|s| s["id"].as_str().unwrap().to_string());
    }
    at.unwrap_or_else(|| panic!("{path} がありません"))
}

/// The items of a chart and of every block and bar inside them, in order.
fn every_item(items: &Value) -> Vec<Value> {
    let mut out = Vec::new();
    for item in items.as_array().into_iter().flatten() {
        out.push(item.clone());
        if let Some(inner) = item["step"].get("items") {
            out.extend(every_item(inner));
        }
        for section in item["block"]["sections"].as_array().into_iter().flatten() {
            out.extend(every_item(&section["items"]));
        }
    }
    out
}

#[test]
fn the_sequence_chart_op_gives_lifelines_items_origins_and_descriptions() {
    let core = shop_core();
    more_flows(&core);
    let project = project_id(&core);
    let take_id = card_id(&core, "TakeOrder");
    let chart = op(
        &core,
        json!({"op": "sequence_chart", "project": project, "branch": "", "card": take_id}),
    )["result"]
        .clone();
    let server = section_id(&core, "Charts/注文サーバ");
    let named = |label: &str| {
        chart["participants"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["label"] == label)
            .cloned()
            .unwrap_or_else(|| panic!("{label}: {chart}"))
    };
    let desk = named("受付");
    assert_eq!(desk["role"], "actor");
    assert_eq!(desk["figure"], "system");
    assert_eq!(desk["group"], server.as_str());
    assert_eq!(desk["fold"], true);
    assert_eq!(named("決済サービス")["figure"], "outside");
    assert_eq!(named("DB")["role"], "actor");

    let items = every_item(&chart["items"]);
    let check = items
        .iter()
        .find(|i| i["step"]["node"] == "check")
        .unwrap_or_else(|| panic!("{chart}"));
    assert_eq!(check["step"]["label"], "在庫を確かめる");
    let read = &check["step"]["items"][0]["message"];
    assert_eq!(
        read["origin"],
        json!({"card": take_id, "node": "check", "row": 0})
    );
    assert_eq!(read["description"], "値段と名前を見る");

    let actors = op(
        &core,
        json!({"op": "sequence_chart", "project": project, "branch": "", "section": server}),
    )["result"]
        .clone();
    let frames = actors["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|i| i["block"]["kind"] == "flow")
        .count();
    assert_eq!(frames, 3, "{actors}");
    // Run only from inside the actor, Reserve is a bar on 在庫's lifeline that says which flow it is.
    let stock_at = actors["participants"]
        .as_array()
        .unwrap()
        .iter()
        .position(|p| p["label"] == "在庫")
        .unwrap_or_else(|| panic!("{actors}"));
    let reserve = every_item(&actors["items"])
        .into_iter()
        .find(|i| {
            i["step"].is_object()
                && i["step"].get("node").is_none()
                && i["step"]["over"] == stock_at
        })
        .unwrap_or_else(|| panic!("在庫の生命線に Reserve の帯がありません: {actors}"));
    assert_eq!(
        reserve["step"]["flow"],
        card_id(&core, "Reserve"),
        "{reserve}"
    );
    assert_eq!(reserve["step"]["op"], "run", "{reserve}");
    assert_eq!(
        reserve["step"]["place"], "Charts/注文サーバ/在庫",
        "{reserve}"
    );

    let deep = op(
        &core,
        json!({"op": "sequence_chart", "project": project, "branch": "", "card": take_id, "lifelines": [server]}),
    )["result"]
        .clone();
    let whole = deep["participants"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["label"] == "注文サーバ")
        .cloned()
        .unwrap_or_else(|| panic!("{deep}"));
    assert_eq!(whole["open"], true);
}

#[test]
fn the_chart_data_names_the_gap_roles() {
    let core = shop_core();
    order_status(&core);
    let project = project_id(&core);
    let chart = |name: &str| {
        op(
            &core,
            json!({"op": "sequence_chart", "project": project, "branch": "", "card": card_id(&core, name)}),
        )["result"]["participants"]
            .as_array()
            .unwrap()
            .clone()
    };
    let reserve = chart("Reserve");
    assert_eq!(reserve[0]["role"], "noPartner", "{reserve:?}");
    assert_eq!(reserve[0]["label"], "相手が書かれていない");
    assert_eq!(reserve[0]["place"], "Reserve.start");
    assert_eq!(
        reserve[0]["origin"],
        json!({"card": card_id(&core, "Reserve"), "node": "start"})
    );
    let take = chart("TakeOrder");
    let last = take.last().unwrap();
    assert_eq!(last["role"], "noStore", "{take:?}");
    assert_eq!(last["label"], "置き場が書かれていない");
    assert_eq!(last["place"], "Receipt");
    assert_eq!(
        last["origin"],
        json!({"card": card_id(&core, "TakeOrder"), "node": "save", "row": 2})
    );
    let status = chart("OrderStatus");
    assert_eq!(status[0]["role"], "anyone", "{status:?}");
    assert_eq!(status[0]["label"], "誰からでも");
    assert!(status[0].get("origin").is_none(), "{status:?}");
}

#[test]
fn the_store_gap_names_the_stores_a_shape_reaches() {
    let core = shop_core();
    crate::actor_sections::receipt_in_two_stores(&core);
    let chart = sequence(&core, TAKE_ORDER, None);
    assert!(
        body(&chart).contains(&format!(
            "{}->>{}: write Receipt.number",
            alias(&chart, "受付"),
            alias(&chart, "置き場が書かれていない")
        )),
        "{chart}"
    );
    let take = op(
        &core,
        json!({"op": "sequence_chart", "project": project_id(&core), "branch": "",
            "card": card_id(&core, "TakeOrder")}),
    )["result"]["participants"]
        .as_array()
        .unwrap()
        .clone();
    let last = take.last().unwrap();
    assert_eq!(last["role"], "noStore", "{take:?}");
    assert_eq!(last["label"], "置き場が書かれていない");
    assert_eq!(last["place"], "Receipt（ShopDB・Archive のどれか）");
}

/// An actor's chart of `section`, as mermaid.
fn actor_chart(core: &Core, section: &str, lifelines: Option<Value>) -> String {
    let mut value = json!({"as": "diagram", "shape": "sequence"});
    if let Some(l) = lifelines {
        value["lifelines"] = l;
    }
    text_in(
        &ok(
            core,
            "section_read",
            json!({"target": section, "value": value}),
        ),
        "mermaid",
    )
}

/// The heads of the frames of an actor's chart, in order.
fn frame_heads(chart: &str) -> Vec<String> {
    let lines: Vec<&str> = chart.lines().map(str::trim).collect();
    lines
        .windows(2)
        .filter(|w| w[0].starts_with("rect "))
        .map(|w| w[1].split_once(": ").unwrap().1.to_string())
        .collect()
}

#[test]
fn an_actor_with_flows_of_its_own_draws_its_own_lifeline_first_in_its_box() {
    let core = shop_core();
    ok(
        &core,
        "card_create",
        json!({"value": {
            "Charts/注文サーバ:Open": {"kind": "flow", "label": "店を開ける"},
            "Charts/注文サーバ:Open.start": {"kind": "begin", "trigger": "manual",
                "connections": [{"line": "next", "to": "load"}]},
            "Charts/注文サーバ:Open.load": {"kind": "process", "steps": [{"do": "read"}],
                "connections": [
                    {"line": "reads", "from": "steps[0]", "to": "Schemas/Shop:Product", "at": "price"},
                    {"line": "next", "to": "done"}]},
            "Charts/注文サーバ:Open.done": {"kind": "end", "outcome": "success"}
        }}),
    );
    let chart = actor_chart(&core, "Charts/注文サーバ", None);
    assert_eq!(
        participants(&chart)[..6],
        [
            "ブラウザ",
            "注文サーバ",
            "受付",
            "在庫",
            "決済サービス",
            "DB"
        ],
        "{chart}"
    );
    let lines: Vec<&str> = chart.lines().map(str::trim).collect();
    let at = lines.iter().position(|l| *l == "box 注文サーバ").unwrap();
    assert_eq!(
        lines[at + 1..at + 5],
        [
            "participant P2 as 注文サーバ",
            "participant P3 as 受付",
            "participant P4 as 在庫",
            "end"
        ],
        "{chart}"
    );

    let project = project_id(&core);
    let server = section_id(&core, "Charts/注文サーバ");
    let data = op(
        &core,
        json!({"op": "sequence_chart", "project": project, "branch": "", "section": server}),
    )["result"]
        .clone();
    let named = |data: &Value, label: &str| {
        data["participants"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["label"] == label)
            .cloned()
            .unwrap_or_else(|| panic!("{label}: {data}"))
    };
    let own = named(&data, "注文サーバ");
    assert_eq!(own["group"], server.as_str(), "{own}");
    assert!(own.get("open").is_none(), "{own}");

    // Folded into itself: one lifeline, the actors inside gone, nothing bracketed.
    let folded = actor_chart(
        &core,
        "Charts/注文サーバ",
        Some(json!(["Charts/注文サーバ"])),
    );
    let drawn = participants(&folded);
    assert_eq!(
        drawn.iter().filter(|p| *p == "注文サーバ").count(),
        1,
        "{folded}"
    );
    assert!(
        !drawn.iter().any(|p| p == "受付" || p == "在庫"),
        "{folded}"
    );
    assert!(!folded.contains("box "), "{folded}");
    let data = op(
        &core,
        json!({"op": "sequence_chart", "project": project, "branch": "", "section": server, "lifelines": [server]}),
    )["result"]
        .clone();
    assert_eq!(named(&data, "注文サーバ")["open"], true, "{data}");
}

#[test]
fn a_flow_that_receives_with_no_caller_named_is_provided() {
    let core = shop_core();
    ok(
        &core,
        "card_create",
        json!({"value": {
            "Charts/注文サーバ/在庫:Count": {"kind": "flow", "label": "数える"},
            "Charts/注文サーバ/在庫:Count.start": {"kind": "begin", "trigger": "receive",
                "connections": [{"line": "next", "to": "put"}]},
            "Charts/注文サーバ/在庫:Count.put": {"kind": "process", "steps": [{"do": "write"}],
                "connections": [
                    {"line": "writes", "from": "steps[0]", "to": "Schemas/Shop:Stock", "at": "reserved"},
                    {"line": "next", "to": "done"}]},
            "Charts/注文サーバ/在庫:Count.done": {"kind": "end", "outcome": "success"}
        }}),
    );
    let chart = actor_chart(&core, "Charts/注文サーバ", None);
    assert_eq!(
        frame_heads(&chart),
        ["TakeOrder 注文を受ける", "Count 数える"],
        "{chart}"
    );
    let lines: Vec<&str> = chart.lines().map(str::trim).collect();
    let at = lines
        .iter()
        .position(|l| l.ends_with(": Count 数える"))
        .unwrap();
    assert_eq!(
        lines[at + 1],
        format!(
            "{}->>{}: receive",
            alias(&chart, "相手が書かれていない"),
            alias(&chart, "在庫")
        ),
        "{chart}"
    );
}

#[test]
fn open_is_only_where_opening_draws_an_actor_not_drawn_now() {
    let core = shop_core();
    ok(
        &core,
        "section_create",
        json!({"target": "Charts/決済サービス/不正検知"}),
    );
    ok(
        &core,
        "card_create",
        json!({"value": {
            "Charts/決済サービス/不正検知:Score": {"kind": "flow", "label": "不正を見る"},
            "Charts/決済サービス/不正検知:Score.start": {"kind": "begin", "trigger": "receive",
                "connections": [{"line": "next", "to": "done"}]},
            "Charts/決済サービス/不正検知:Score.done": {"kind": "end", "outcome": "success"}
        }}),
    );
    let project = project_id(&core);
    let take = card_id(&core, "TakeOrder");
    let server = section_id(&core, "Charts/注文サーバ");
    let payment = section_id(&core, "Charts/決済サービス");
    let fraud = section_id(&core, "Charts/決済サービス/不正検知");
    let chart = |asked: Value| op(&core, asked)["result"].clone();
    let flow = |lifelines: Option<&str>| {
        let mut asked =
            json!({"op": "sequence_chart", "project": project, "branch": "", "card": take});
        if let Some(l) = lifelines {
            asked["lifelines"] = json!([l]);
        }
        chart(asked)
    };
    let actor = || {
        chart(json!({"op": "sequence_chart", "project": project, "branch": "", "section": server}))
    };
    let named = |data: &Value, label: &str| {
        data["participants"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["label"] == label)
            .cloned()
            .unwrap_or_else(|| panic!("{label}: {data}"))
    };

    // Nothing the chart deals with is inside the payment service: opening it would add nothing.
    for data in [flow(None), actor()] {
        let pay = named(&data, "決済サービス");
        assert!(pay.get("open").is_none(), "{pay}");
    }
    assert_eq!(
        named(&flow(Some(&server)), "注文サーバ")["open"],
        true,
        "受付と在庫が中にある"
    );

    ok(
        &core,
        "card_update",
        json!({"target": "Charts/注文サーバ/受付:TakeOrder.pay", "value": {"connections": {
            "add": [{"line": "partner", "from": "steps[1]", "to": "Charts/決済サービス/不正検知:Score"}],
            "remove": [{"line": "partner", "from": "steps[1]", "to": "Charts/決済サービス:Charge"}]
        }}}),
    );
    for data in [flow(None), actor()] {
        assert_eq!(named(&data, "決済サービス")["open"], true, "{data}");
    }
    let opened = flow(Some(&fraud));
    let inner = named(&opened, "不正検知");
    assert_eq!(inner["role"], "actor", "{inner}");
    let pay = named(&opened, "決済サービス");
    assert_eq!(pay["group"], payment.as_str(), "{pay}");
    assert!(pay.get("open").is_none(), "{pay}");
}
