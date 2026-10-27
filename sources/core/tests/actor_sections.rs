//! Actor sections (`specboard://SpecBoard/Docs/Design:Overview` §9「アクターのセクション」): a chart's
//! parts as sections that provide and use, nested in charts and in each other, holding the flows that
//! are what they do. A flow's actor is the nearest one around it; a store is an actor's where a ghost
//! of it sits; what they are (`figure`) and the sections' own marks go through the project file, and an
//! actor's page in an export starts with what it provides and uses.

use crate::common;

use common::{checked, mcp, ok, op, project_id, refused, script_store, text_in};
use serde_json::{Value, json};
use specboard_core::Core;

/// The made-up shop the actor tests share: in the sample, the chart root `Charts` holds the actors
/// 利用者 (a person), ブラウザ, 注文サーバ (holding the actors 受付 and 在庫), 決済サービス (outside)
/// and DB, and the plain chart section シナリオ; `Schemas/Shop` holds the shapes and the store
/// `ShopDB`, a ghost of which sits in DB. The flows: a person orders through the browser, the order
/// desk takes it — checks the prices, stops short when there is not enough, has the stock reserved,
/// pays through the outside service and writes the order — and answers with the receipt number.
pub(crate) fn shop(core: &Core) {
    let sections = [
        (
            "Charts/利用者",
            json!({"type": "actor", "figure": "person"}),
        ),
        ("Charts/ブラウザ", json!({"type": "actor"})),
        ("Charts/注文サーバ", json!({"type": "actor"})),
        ("Charts/注文サーバ/受付", json!({})),
        ("Charts/注文サーバ/在庫", json!({})),
        (
            "Charts/決済サービス",
            json!({"type": "actor", "figure": "outside"}),
        ),
        ("Charts/DB", json!({"type": "actor"})),
        ("Charts/シナリオ", json!({})),
        ("Schemas/Shop", json!({})),
    ];
    for (target, value) in sections {
        ok(
            core,
            "section_create",
            json!({"target": target, "value": value}),
        );
    }
    ok(
        core,
        "card_create",
        json!({"value": {
            "Schemas/Shop:Line": {"kind": "value", "fields": [{"name": "qty", "type": "int"}]},
            "Schemas/Shop:Order": {"kind": "entity", "fields": [
                {"name": "items", "type": "string", "array": true}, {"name": "total", "type": "decimal"}],
                "connections": [{"line": "embed", "from": "items", "to": "Schemas/Shop:Line"}]},
            "Schemas/Shop:Stock": {"kind": "entity", "fields": [{"name": "reserved", "type": "int"}]},
            "Schemas/Shop:Product": {"kind": "entity", "fields": [
                {"name": "price", "type": "decimal"}, {"name": "name", "type": "string"}]},
            "Schemas/Shop:Receipt": {"kind": "entity", "fields": [{"name": "number", "type": "string"}]},
            "Schemas/Shop:ShopDB": {"kind": "store", "connections": [
                {"line": "holds", "to": "Schemas/Shop:Order"},
                {"line": "holds", "to": "Schemas/Shop:Stock"},
                {"line": "holds", "to": "Schemas/Shop:Product"}]},
            "Charts/DB:ShopDB": {"kind": "ghost", "target": "Schemas/Shop:ShopDB"}
        }}),
    );
    ok(
        core,
        "card_create",
        json!({"value": {
            "Charts/利用者:PlaceOrder": {"kind": "flow", "label": "注文する"},
            "Charts/ブラウザ:CheckoutPage": {"kind": "flow", "label": "注文画面"},
            "Charts/注文サーバ/受付:TakeOrder": {"kind": "flow", "label": "注文を受ける"},
            "Charts/注文サーバ/在庫:Reserve": {"kind": "flow", "label": "在庫を押さえる"},
            "Charts/決済サービス:Charge": {"kind": "flow", "label": "支払う"},

            "Charts/利用者:PlaceOrder.start": {"kind": "begin", "trigger": "manual",
                "connections": [{"line": "next", "to": "order"}]},
            "Charts/利用者:PlaceOrder.order": {"kind": "process", "label": "注文を送る",
                "steps": [{"do": "send", "carries": ["注文の内容"]}],
                "connections": [
                    {"line": "partner", "from": "steps[0]", "to": "Charts/ブラウザ:CheckoutPage"},
                    {"line": "next", "to": "done"}]},
            "Charts/利用者:PlaceOrder.done": {"kind": "end", "outcome": "success"},

            "Charts/ブラウザ:CheckoutPage.start": {"kind": "begin", "trigger": "receive", "carries": ["注文の内容"],
                "connections": [
                    {"line": "partner", "to": "Charts/利用者:PlaceOrder"},
                    {"line": "next", "to": "ask"}]},
            "Charts/ブラウザ:CheckoutPage.ask": {"kind": "process", "label": "注文を出す",
                "steps": [{"do": "send", "carries": ["注文の内容"]}, {"do": "receive", "carries": ["受付番号"]}],
                "connections": [
                    {"line": "partner", "from": "steps[0]", "to": "Charts/注文サーバ/受付:TakeOrder"},
                    {"line": "partner", "from": "steps[1]", "to": "Charts/注文サーバ/受付:TakeOrder"},
                    {"line": "next", "to": "done"}]},
            "Charts/ブラウザ:CheckoutPage.done": {"kind": "end", "outcome": "success"},

            "Charts/注文サーバ/受付:TakeOrder.start": {"kind": "begin", "trigger": "receive", "carries": ["注文の内容"],
                "description": "画面から注文が届く",
                "connections": [
                    {"line": "partner", "to": "Charts/ブラウザ:CheckoutPage"},
                    {"line": "next", "to": "check"}]},
            "Charts/注文サーバ/受付:TakeOrder.check": {"kind": "process", "label": "在庫を確かめる",
                "steps": [{"do": "read", "description": "値段と名前を見る"}],
                "connections": [
                    {"line": "reads", "from": "steps[0]", "to": "Schemas/Shop:Product", "at": "price"},
                    {"line": "reads", "from": "steps[0]", "to": "Schemas/Shop:Product", "at": "name"},
                    {"line": "next", "to": "enough"}]},
            "Charts/注文サーバ/受付:TakeOrder.enough": {"kind": "eval", "value": "Schemas/Shop:Stock.reserved",
                "connections": [
                    {"line": "condition", "to": "short", "fields": {"when": "足りない"}},
                    {"line": "else", "to": "reserve"}]},
            "Charts/注文サーバ/受付:TakeOrder.reserve": {"kind": "run", "label": "押さえる",
                "connections": [
                    {"line": "runs", "to": "Charts/注文サーバ/在庫:Reserve"},
                    {"line": "next", "to": "pay"}]},
            "Charts/注文サーバ/受付:TakeOrder.pay": {"kind": "process", "label": "支払う",
                "steps": [{"do": "send", "carries": ["支払い"]}, {"do": "receive", "carries": ["決済の結果"]}],
                "connections": [
                    {"line": "partner", "from": "steps[0]", "to": "Charts/決済サービス:Charge"},
                    {"line": "partner", "from": "steps[1]", "to": "Charts/決済サービス:Charge"},
                    {"line": "next", "to": "save"}]},
            "Charts/注文サーバ/受付:TakeOrder.save": {"kind": "process", "label": "記録する",
                "steps": [{"do": "write", "description": "注文を作る"}, {"do": "write"}, {"do": "write"}],
                "connections": [
                    {"line": "writes", "from": "steps[0]", "to": "Schemas/Shop:Order", "at": "items"},
                    {"line": "writes", "from": "steps[0]", "to": "Schemas/Shop:Order", "at": "total"},
                    {"line": "writes", "from": "steps[1]", "to": "Schemas/Shop:Line", "at": "qty"},
                    {"line": "writes", "from": "steps[2]", "to": "Schemas/Shop:Receipt", "at": "number"},
                    {"line": "next", "to": "done"}]},
            "Charts/注文サーバ/受付:TakeOrder.done": {"kind": "end", "outcome": "success",
                "return": {"carries": ["受付番号"]},
                "connections": [{"line": "partner", "to": "Charts/ブラウザ:CheckoutPage"}]},
            "Charts/注文サーバ/受付:TakeOrder.short": {"kind": "end", "outcome": "failure"},

            "Charts/注文サーバ/在庫:Reserve.start": {"kind": "begin", "trigger": "receive",
                "connections": [{"line": "next", "to": "put"}]},
            "Charts/注文サーバ/在庫:Reserve.put": {"kind": "process", "label": "取り置く",
                "steps": [{"do": "write"}],
                "connections": [
                    {"line": "writes", "from": "steps[0]", "to": "Schemas/Shop:Stock", "at": "reserved"},
                    {"line": "next", "to": "done"}]},
            "Charts/注文サーバ/在庫:Reserve.done": {"kind": "end", "outcome": "success"},

            "Charts/決済サービス:Charge.start": {"kind": "begin", "trigger": "receive", "carries": ["支払い"],
                "connections": [{"line": "next", "to": "done"}]},
            "Charts/決済サービス:Charge.done": {"kind": "end", "outcome": "success",
                "return": {"carries": ["決済の結果"]}}
        }}),
    );
}

/// The shop's Receipt held inside two shapes kept by two stores: Order (in ShopDB) gets a field
/// `receipt` that has it inside, and Folder, kept by the store Archive made after ShopDB, has it
/// inside too. No store holds Receipt itself.
pub(crate) fn receipt_in_two_stores(core: &Core) {
    ok(
        core,
        "card_create",
        json!({"target": "Schemas/Shop:Order.receipt", "value": {"type": "string"}}),
    );
    ok(
        core,
        "card_update",
        json!({"target": "Schemas/Shop:Order", "value": {"connections": {"add": [
            {"line": "embed", "from": "receipt", "to": "Schemas/Shop:Receipt"}]}}}),
    );
    ok(
        core,
        "card_create",
        json!({"value": {
            "Schemas/Shop:Folder": {"kind": "entity", "fields": [{"name": "receipt", "type": "string"}],
                "connections": [{"line": "embed", "from": "receipt", "to": "Schemas/Shop:Receipt"}]},
            "Schemas/Shop:Archive": {"kind": "store", "connections": [
                {"line": "holds", "to": "Schemas/Shop:Folder"}]}
        }}),
    );
}

/// The shop in the sample's store, opened.
pub(crate) fn shop_core() -> Core {
    let core = Core::open(script_store()).unwrap();
    shop(&core);
    core
}

/// A section's row as `section_list` has it, by its path.
fn listed(core: &Core, path: &str) -> Value {
    ok(core, "section_list", json!({}))["sections"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["section"] == path)
        .cloned()
        .unwrap_or_else(|| panic!("{path} がありません"))
}

#[test]
fn an_actor_section_sits_in_a_chart_or_an_actor_and_holds_flows() {
    let core = Core::open(script_store()).unwrap();
    let made = ok(
        &core,
        "section_create",
        json!({"target": "Charts/注文サーバ", "value": {"type": "actor"}}),
    );
    assert_eq!(made["type"], "actor", "{made}");
    // Inside an actor, an actor unless a chart is asked for.
    let inner = ok(
        &core,
        "section_create",
        json!({"target": "Charts/注文サーバ/受付"}),
    );
    assert_eq!(inner["type"], "actor", "{inner}");
    let tests = ok(
        &core,
        "section_create",
        json!({"target": "Charts/注文サーバ/テスト", "value": {"type": "chart"}}),
    );
    assert_eq!(tests["type"], "chart", "{tests}");
    ok(
        &core,
        "card_create",
        json!({"target": "Charts/注文サーバ/受付:TakeOrder", "value": {"kind": "flow"}}),
    );
    assert_eq!(listed(&core, "Charts/注文サーバ")["type"], "actor");
    assert_eq!(listed(&core, "Charts/注文サーバ/受付")["type"], "actor");
    assert_eq!(listed(&core, "Charts/注文サーバ/テスト")["type"], "chart");
    // Inside a chart, a chart as before.
    let plain = ok(&core, "section_create", json!({"target": "Charts/Plain"}));
    assert_eq!(plain["type"], "chart", "{plain}");

    // Nowhere a chart or an actor does not list it.
    assert_eq!(
        refused(
            &core,
            "section_create",
            json!({"target": "Schemas/Ordering/Actors", "value": {"type": "actor"}})
        ),
        "スキーマセクションの中に作れるのは schema です（actor は作れません）"
    );
    let top = refused(
        &core,
        "section_create",
        json!({"target": "Actors", "value": {"type": "actor"}}),
    );
    assert!(
        top.starts_with("main・branch の直下にはセクションを作れません"),
        "{top}"
    );
    // It holds flows and notes: anything else is refused with where it goes.
    let entity = refused(
        &core,
        "card_create",
        json!({"target": "Charts/注文サーバ/受付:Order", "value": {"kind": "entity"}}),
    );
    assert!(entity.contains("Charts/注文サーバ/受付"), "{entity}");
    assert!(entity.contains("Schemas"), "{entity}");
}

#[test]
fn a_chart_section_becomes_an_actor_and_back_keeping_what_is_inside() {
    let core = shop_core();
    ok(
        &core,
        "card_create",
        json!({"value": {
            "Charts/シナリオ:一通り": {"kind": "flow", "label": "注文の一通り"},
            "Charts/シナリオ:一通り.start": {"kind": "begin", "trigger": "manual",
                "connections": [{"line": "next", "to": "go"}]},
            "Charts/シナリオ:一通り.go": {"kind": "run",
                "connections": [{"line": "runs", "to": "Charts/注文サーバ/受付:TakeOrder"},
                    {"line": "next", "to": "done"}]},
            "Charts/シナリオ:一通り.done": {"kind": "end", "outcome": "success"}
        }}),
    );
    let before = ok(
        &core,
        "card_read",
        json!({"target": "Charts/シナリオ:一通り.go"}),
    );
    let made = ok(
        &core,
        "section_update",
        json!({"target": "Charts/シナリオ", "value": {"type": "actor"}}),
    );
    assert_eq!(made["type"], "actor", "{made}");
    assert_eq!(made["rewrittenPaths"], 0, "{made}");
    assert_eq!(
        ok(
            &core,
            "card_read",
            json!({"target": "Charts/シナリオ:一通り.go"})
        )["connections"],
        before["connections"]
    );
    let history = ok(
        &core,
        "branch_read",
        json!({"target": "main", "value": {"as": "history", "limit": 1}}),
    );
    assert_eq!(
        history["commits"][0]["lines"][0],
        "セクション Charts/シナリオ を更新（種類を アクター に変更）"
    );

    // Back to a chart: what it is goes, what is inside stays actors.
    ok(
        &core,
        "section_update",
        json!({"target": "Charts/注文サーバ", "value": {"figure": "person"}}),
    );
    assert_eq!(listed(&core, "Charts/注文サーバ")["figure"], "person");
    let chart = ok(
        &core,
        "section_update",
        json!({"target": "Charts/注文サーバ", "value": {"type": "chart"}}),
    );
    assert_eq!(chart["type"], "chart", "{chart}");
    assert!(chart.get("figure").is_none(), "{chart}");
    assert_eq!(listed(&core, "Charts/注文サーバ/受付")["type"], "actor");
    assert_eq!(listed(&core, "Charts/注文サーバ/在庫")["type"], "actor");
    assert_eq!(
        ok(
            &core,
            "card_read",
            json!({"target": "Charts/注文サーバ/受付:TakeOrder"})
        )["path"],
        "Charts/注文サーバ/受付:TakeOrder"
    );

    // The root's own section keeps its kind.
    assert_eq!(
        refused(
            &core,
            "section_update",
            json!({"target": "Charts", "value": {"type": "actor"}})
        ),
        "Charts は main・branch の直下のチャートセクションなので、種類を変えられません"
    );
}

#[test]
fn a_moved_section_keeps_the_inner_kinds_its_new_parent_allows() {
    let core = shop_core();
    let moved = ok(
        &core,
        "section_update",
        json!({"target": "Charts/注文サーバ", "value": {"parent": "Charts/シナリオ"}}),
    );
    assert_eq!(moved["section"], "Charts/シナリオ/注文サーバ", "{moved}");
    assert_eq!(listed(&core, "Charts/シナリオ/注文サーバ")["type"], "actor");
    assert_eq!(
        listed(&core, "Charts/シナリオ/注文サーバ/受付")["type"],
        "actor"
    );
    assert_eq!(
        listed(&core, "Charts/シナリオ/注文サーバ/在庫")["type"],
        "actor"
    );
    assert_eq!(listed(&core, "Charts/シナリオ")["type"], "chart");

    // A kind the chart does not list becomes its first, level by level.
    ok(&core, "section_create", json!({"target": "Free/下書き"}));
    ok(
        &core,
        "section_create",
        json!({"target": "Free/下書き/メモ"}),
    );
    ok(
        &core,
        "section_update",
        json!({"target": "Free/下書き", "value": {"parent": "Charts"}}),
    );
    assert_eq!(listed(&core, "Charts/下書き")["type"], "chart");
    assert_eq!(listed(&core, "Charts/下書き/メモ")["type"], "chart");
}

#[test]
fn figure_is_kept_on_actors_and_refused_elsewhere() {
    let core = shop_core();
    ok(
        &core,
        "section_update",
        json!({"target": "Charts/利用者", "value": {"figure": "person"}}),
    );
    ok(
        &core,
        "section_update",
        json!({"target": "Charts/DB", "value": {"figure": "system"}}),
    );
    let person = ok(&core, "section_read", json!({"target": "Charts/利用者"}));
    assert_eq!(person["figure"], "person", "{person}");
    let db = ok(&core, "section_read", json!({"target": "Charts/DB"}));
    assert!(db.get("figure").is_none(), "{db}");
    assert_eq!(
        refused(
            &core,
            "section_update",
            json!({"target": "Charts/シナリオ", "value": {"figure": "person"}})
        ),
        "figure はアクターのセクションだけに書けます（Charts/シナリオ はチャートセクションです）"
    );
    assert_eq!(
        refused(
            &core,
            "section_update",
            json!({"target": "Charts/利用者", "value": {"figure": "robot"}})
        ),
        "figure は system・person・outside のどれかです（\"robot\"）"
    );
}

#[test]
fn a_flow_reads_its_nearest_actor() {
    let core = shop_core();
    ok(
        &core,
        "section_create",
        json!({"target": "Charts/注文サーバ/受付/手順", "value": {"type": "chart"}}),
    );
    ok(
        &core,
        "card_create",
        json!({"value": {
            "Charts/シナリオ:一通り": {"kind": "flow"},
            "Charts/注文サーバ/受付/手順:Retry": {"kind": "flow"}
        }}),
    );
    let actor = |path: &str| {
        ok(&core, "card_read", json!({"target": path}))
            .get("actor")
            .cloned()
    };
    assert_eq!(
        actor("Charts/注文サーバ/受付:TakeOrder"),
        Some(json!("Charts/注文サーバ/受付"))
    );
    assert_eq!(
        actor("Charts/注文サーバ/受付/手順:Retry"),
        Some(json!("Charts/注文サーバ/受付"))
    );
    assert_eq!(actor("Charts/シナリオ:一通り"), None);
    // Moved, it is the new actor's: nothing kept goes stale.
    ok(
        &core,
        "card_update",
        json!({"target": "Charts/注文サーバ/受付:TakeOrder", "value": {"section": "Charts/注文サーバ/在庫"}}),
    );
    assert_eq!(
        actor("Charts/注文サーバ/在庫:TakeOrder"),
        Some(json!("Charts/注文サーバ/在庫"))
    );
}

/// The participant a sequence diagram draws as `label`: its alias (`P3`).
fn alias(chart: &str, label: &str) -> String {
    chart
        .lines()
        .map(str::trim)
        .find_map(|l| {
            let rest = l
                .strip_prefix("participant ")
                .or_else(|| l.strip_prefix("actor "))?;
            let (alias, said) = rest.split_once(" as ")?;
            (said == label).then(|| alias.to_string())
        })
        .unwrap_or_else(|| panic!("{label} の参加者がありません: {chart}"))
}

fn sequence(core: &Core, flow: &str) -> String {
    text_in(
        &ok(
            core,
            "card_read",
            json!({"target": flow, "value": {"as": "diagram", "shape": "sequence"}}),
        ),
        "mermaid",
    )
}

#[test]
fn a_store_ghost_puts_the_store_under_its_actor_and_two_holders_are_said() {
    let core = shop_core();
    let chart = sequence(&core, "Charts/注文サーバ/受付:TakeOrder");
    let (me, db) = (alias(&chart, "受付"), alias(&chart, "DB"));
    assert!(
        chart.contains(&format!("{me}->>{db}: read Product.price, name")),
        "{chart}"
    );
    assert!(
        chart.contains(&format!("{me}->>{db}: write Order.items, total")),
        "{chart}"
    );
    assert!(!chart.contains("ShopDB"), "{chart}");
    assert!(checked(&core, "main").get("storeActors").is_none());

    // A ghost in a second actor: the one nearer the flow, and the check says so.
    ok(
        &core,
        "card_create",
        json!({"target": "Charts/注文サーバ/在庫:ShopDB", "value": {"kind": "ghost", "target": "Schemas/Shop:ShopDB"}}),
    );
    let chart = sequence(&core, "Charts/注文サーバ/受付:TakeOrder");
    let (me, stock) = (alias(&chart, "受付"), alias(&chart, "在庫"));
    assert!(
        chart.contains(&format!("{me}->>{stock}: write Order.items, total")),
        "{chart}"
    );
    let found = checked(&core, "main");
    let items = found["storeActors"].as_array().unwrap();
    assert_eq!(items.len(), 1, "{found}");
    assert_eq!(items[0]["at"], "Schemas/Shop:ShopDB");
    let problem = items[0]["problem"].as_str().unwrap();
    assert!(
        problem.contains("Charts/注文サーバ/在庫") && problem.contains("Charts/DB"),
        "{problem}"
    );
}

#[test]
fn actor_sections_and_marks_go_through_the_project_file() {
    let core = shop_core();
    ok(
        &core,
        "section_update",
        json!({"target": "Charts/注文サーバ/受付", "value": {"icon": {"name": "key", "color": "#FF9F0A"}}}),
    );
    let text = op(
        &core,
        json!({"op": "project_file", "project": project_id(&core)}),
    )["result"]["text"]
        .as_str()
        .unwrap()
        .to_string();
    let file: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(file["version"], 33);

    let again = Core::create(common::place("again.sqlite")).unwrap();
    let read = op(&again, json!({"op": "project_file_read", "text": text}));
    assert!(read.get("error").is_none(), "{read}");
    let section = |path: &str| {
        mcp(&again, "section_read", json!({"target": path})).unwrap_or_else(|e| panic!("{e}"))
    };
    assert_eq!(section("Charts/注文サーバ")["type"], "actor");
    let desk = section("Charts/注文サーバ/受付");
    assert_eq!(desk["type"], "actor");
    assert_eq!(desk["icon"], json!({"name": "key", "color": "#FF9F0A"}));
    assert_eq!(section("Charts/利用者")["figure"], "person");

    // A file written by an app newer than this one is refused as one.
    let mut newer = file.clone();
    newer["version"] = 34.into();
    let later = Core::create(common::place("later.sqlite")).unwrap();
    let refused = op(
        &later,
        json!({"op": "project_file_read", "text": newer.to_string()}),
    );
    assert_eq!(
        refused["error"],
        "新しい版のアプリで書かれたファイルです。アプリを新しくしてから読み込んでください",
        "{refused}"
    );
}

#[test]
fn an_actors_sections_page_says_what_it_provides_and_uses() {
    let core = shop_core();
    let allowed = op(
        &core,
        json!({"op": "mcp_writes_set", "project": project_id(&core), "group": "export", "state": "allow"}),
    );
    assert!(allowed.get("error").is_none(), "{allowed}");
    ok(
        &core,
        "export_create",
        json!({"target": "店の図", "value": {"entries": [
            {"from": "Charts/注文サーバ", "to": "docs/charts/注文サーバ.md", "kinds": ["flow"]}
        ]}}),
    );
    let files = op(
        &core,
        json!({"op": "export_files", "project": project_id(&core)}),
    )["result"]
        .clone();
    // The files by the export's name and their paths in it.
    fn find(value: &Value, path: &str) -> Option<String> {
        match value {
            Value::Object(o) => o.iter().find_map(|(k, v)| {
                if k.ends_with(path) {
                    v.as_str().map(str::to_string)
                } else {
                    find(v, path)
                }
            }),
            Value::Array(a) => a.iter().find_map(|v| find(v, path)),
            _ => None,
        }
    }
    let page = find(&files, "docs/charts/注文サーバ.md").unwrap_or_else(|| panic!("{files}"));
    let head = page.split("## ").next().unwrap();
    assert!(head.contains("姿: システム"), "{page}");
    // Reserve is run only from inside 注文サーバ: what TakeOrder does, not a flow it provides.
    let provides = head
        .lines()
        .find(|l| l.starts_with("提供する flow"))
        .unwrap_or_else(|| panic!("{page}"));
    assert!(provides.contains("[`TakeOrder`](#takeorder)"), "{provides}");
    assert!(!provides.contains("Reserve"), "{provides}");
    let uses = head
        .lines()
        .find(|l| l.starts_with("使う相手"))
        .unwrap_or_else(|| panic!("{page}"));
    assert!(
        uses.contains("決済サービス") && uses.contains("DB"),
        "{uses}"
    );
    let used = head
        .lines()
        .find(|l| l.starts_with("使われる相手"))
        .unwrap_or_else(|| panic!("{page}"));
    assert!(used.contains("ブラウザ"), "{used}");
    // Its chart, then the flows as before.
    let chart = head
        .split("```mermaid")
        .nth(1)
        .unwrap_or_else(|| panic!("{page}"));
    assert!(chart.contains("sequenceDiagram"), "{chart}");
    assert!(chart.contains("box 注文サーバ"), "{chart}");
    assert!(chart.contains("TakeOrder 注文を受ける"), "{chart}");
    assert!(page.contains("## TakeOrder"), "{page}");
    assert!(page.contains("## Reserve"), "{page}");
}
