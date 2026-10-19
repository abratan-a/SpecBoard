//! `app_control`, answered by the app (`specboard_core::App`; the Swift app's `AppControl`):
//! the core checks the call — a key that may do what the op does with the window (allowed, or
//! said yes to each time), the arguments, the value against its op's form — hands it over with
//! who called, and reads
//! the app's answer back as the tool's: structure, text blocks, a picture as an MCP image
//! block, or a refusal.
//! And `subscriptions/listen`, checked here and left to the HTTP side to keep open.

use crate::common;

use common::op;
use serde_json::{Value, json};
use specboard_core::{App, Core};
use std::sync::{Arc, Mutex};

/// An app that answers each op as the test says, and keeps what it was asked.
struct Window {
    asked: Mutex<Vec<(Value, Value)>>,
}

impl App for Window {
    fn app_control(&self, arguments: &str, caller: &str) -> String {
        let arguments: Value = serde_json::from_str(arguments).unwrap();
        let caller: Value = serde_json::from_str(caller).unwrap();
        self.asked.lock().unwrap().push((arguments.clone(), caller));
        match arguments["op"].as_str().unwrap() {
            "state" => {
                json!({"json": {"project": "店", "zoom": 1, "all": arguments["value"]["all"]}})
            }
            "select" => json!({"blocks": ["一つ目", "二つ目"]}),
            "capture" => json!({
                "image": {"data": "iVBORw0KGgo=", "mimeType": "image/png"},
                "note": {"target": "window", "width": 640},
            }),
            "view" => json!({"error": "セクションマップが表示されていません"}),
            other => panic!("窓に頼むはずのない op: {other}"),
        }
        .to_string()
    }
}

struct Seen {
    core: Core,
    window: Arc<Window>,
    /// The key that may choose, change how the window looks and take pictures of it: its
    /// id and value.
    driver: (String, String),
    /// One that may not.
    reader: String,
}

fn seen() -> Seen {
    let core = Core::open(common::script_store()).unwrap();
    let make = |name: &str, drive: &str| {
        let made = op(
            &core,
            json!({"op": "access_key_create", "name": name, "permits": {"app_control": drive, "app_control.capture": drive}}),
        );
        (
            made["result"]["key"]["id"].as_str().unwrap().to_string(),
            made["result"]["value"].as_str().unwrap().to_string(),
        )
    };
    let driver = make("画面の係", "allow");
    let reader = make("読むだけの係", "deny").1;
    // The project the calls name, for the key to see: what it may not see is not looked into.
    let project: String = rusqlite::Connection::open(core.place())
        .unwrap()
        .query_row("SELECT id FROM projects WHERE name = '見本'", [], |r| {
            r.get(0)
        })
        .unwrap();
    let given = op(
        &core,
        json!({"op": "access_grant", "key": driver.0, "project": project, "level": "read"}),
    );
    assert_eq!(given, json!({"result": {}}));
    let window = Arc::new(Window {
        asked: Mutex::new(Vec::new()),
    });
    core.set_app(window.clone());
    Seen {
        core,
        window,
        driver,
        reader,
    }
}

/// One message to `mcp` with the key `value` and more headers: what it answered.
fn mcp(core: &Core, value: &str, message: Value, more: Value) -> Value {
    let mut headers = json!({ "authorization": format!("Bearer {value}") });
    for (k, v) in more.as_object().unwrap() {
        headers[k] = v.clone();
    }
    op(
        core,
        json!({"op": "mcp", "message": message, "headers": headers}),
    )["result"]
        .clone()
}

/// `app_control` called with these arguments: the call's result.
fn call(core: &Core, value: &str, arguments: Value) -> Value {
    let message = json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call",
        "params": {"name": "app_control", "arguments": arguments}});
    let reply = mcp(core, value, message, json!({}));
    assert_eq!(reply["status"], 200, "{reply}");
    reply["body"]["result"].clone()
}

fn error_of(result: &Value) -> String {
    assert_eq!(result["isError"], true, "{result}");
    let text: Value = serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap();
    text["error"]["message"].as_str().unwrap().to_string()
}

#[test]
fn the_app_is_asked_with_the_call_and_who_made_it() {
    let s = seen();
    let result = call(
        &s.core,
        &s.driver.1,
        json!({"op": "state", "value": {"all": true}}),
    );
    assert_eq!(result["isError"], false, "{result}");
    assert_eq!(
        result["content"],
        json!([{"type": "text", "text": "{\"project\":\"店\",\"zoom\":1,\"all\":true}"}])
    );
    let asked = s.window.asked.lock().unwrap().clone();
    assert_eq!(asked.len(), 1);
    // The arguments as written, and the key it came with.
    assert_eq!(
        serde_json::to_string(&asked[0].0).unwrap(),
        r#"{"op":"state","value":{"all":true}}"#
    );
    assert_eq!(asked[0].1, json!({"id": s.driver.0, "name": "画面の係"}));

    let blocks = call(
        &s.core,
        &s.driver.1,
        json!({"op": "select", "project": "見本", "target": "Schemas"}),
    );
    assert_eq!(
        blocks["content"],
        json!([{"type": "text", "text": "一つ目"}, {"type": "text", "text": "二つ目"}])
    );
}

#[test]
fn a_picture_goes_as_an_image_block_with_what_it_is_beside_it() {
    let s = seen();
    let result = call(&s.core, &s.driver.1, json!({"op": "capture"}));
    assert_eq!(
        result,
        json!({
            "content": [
                {"type": "image", "data": "iVBORw0KGgo=", "mimeType": "image/png"},
                {"type": "text", "text": "{\"target\":\"window\",\"width\":640}"},
            ],
            "isError": false,
        })
    );
}

#[test]
fn what_the_app_refuses_is_a_refusal() {
    let s = seen();
    assert_eq!(
        error_of(&call(
            &s.core,
            &s.driver.1,
            json!({"op": "view", "target": "fit"})
        )),
        "セクションマップが表示されていません"
    );
    // An op the app knows nothing of never gets past the schema.
    let result = call(&s.core, &s.driver.1, json!({"op": "zoom"}));
    assert!(
        error_of(&result)
            .starts_with("op は state / select / view / capture / windows のいずれかです"),
        "{result}"
    );
}

#[test]
fn the_call_is_checked_before_the_app_is_asked() {
    let s = seen();
    // A key denied choosing, changing how the window looks, and taking pictures of it.
    assert_eq!(
        error_of(&call(&s.core, &s.reader, json!({"op": "state"}))),
        "このアクセスキーでは、app_control（画面を選ぶ・見え方を変える） は使えません（拒否）。変えられるのは人だけで、プロジェクト一覧の場所の設定の「アクセスキー」でこのキーの項目を許可か都度承認にします"
    );
    assert_eq!(
        error_of(&call(&s.core, &s.reader, json!({"op": "capture"}))),
        "このアクセスキーでは、app_control.capture（画面を撮る） は使えません（拒否）。変えられるのは人だけで、プロジェクト一覧の場所の設定の「アクセスキー」でこのキーの項目を許可か都度承認にします"
    );
    // The value against its op's form.
    assert_eq!(
        error_of(&call(
            &s.core,
            &s.driver.1,
            json!({"op": "capture", "value": {"maxSize": "大"}})
        )),
        "value の maxSize は integer で指定してください（渡されたのは 文字列）（value の形は guide の about: app_control）"
    );
    let said = error_of(&call(
        &s.core,
        &s.driver.1,
        json!({"op": "select", "value": {"cards": ["Schemas:Status"]}}),
    ));
    assert!(
        said.starts_with("value に使えないキーです: cards。"),
        "{said}"
    );
    // A branch written into a path, as for every tool.
    let said = error_of(&call(
        &s.core,
        &s.driver.1,
        json!({"op": "select", "project": "見本", "target": "refunds@Schemas:Refund"}),
    ));
    assert!(said.contains("パスに branch は書きません"), "{said}");
    assert!(s.window.asked.lock().unwrap().is_empty());
}

#[test]
fn a_number_the_window_cannot_hold_is_refused_before_it_is_asked() {
    let s = seen();
    let refused = |arguments: Value| error_of(&call(&s.core, &s.driver.1, arguments));
    let select = |depth: Value| {
        json!({"op": "select", "project": "見本", "target": "Schemas:Status",
            "value": {"as": "connections", "depth": depth}})
    };
    for depth in [json!(1e300), json!(-1e300), json!(3), json!(-1)] {
        assert_eq!(
            refused(select(depth.clone())),
            format!("depth は 0〜2 の整数で書きます（渡されたのは {depth}）")
        );
    }
    assert_eq!(
        refused(json!({"op": "view", "target": "fit", "value": {"margin": 1e300}})),
        "margin は 0〜400 の整数で書きます（渡されたのは 1e+300）"
    );
    assert_eq!(
        refused(json!({"op": "view", "target": "zoom", "value": {"zoom": 1e300}})),
        "zoom は 0.02〜2.5 の数で書きます（渡されたのは 1e+300）"
    );
    assert_eq!(
        refused(json!({"op": "view", "target": "area",
            "value": {"area": {"x": 0, "y": 1e300, "width": 10, "height": 10}}})),
        "area.y は -10000000〜10000000 の数で書きます（渡されたのは 1e+300）"
    );
    assert_eq!(
        refused(
            json!({"op": "view", "target": "center", "value": {"point": {"x": -1e300, "y": 0}}})
        ),
        "point.x は -10000000〜10000000 の数で書きます（渡されたのは -1e+300）"
    );
    assert_eq!(
        refused(json!({"op": "capture", "value": {"maxSize": 1e300}})),
        "maxSize は 400〜3000 の整数で書きます（渡されたのは 1e+300）"
    );
    assert!(s.window.asked.lock().unwrap().is_empty());
    // Those in range go to the window.
    for depth in [0, 1, 2] {
        let result = call(&s.core, &s.driver.1, select(json!(depth)));
        assert_eq!(result["isError"], false, "{result}");
    }
    let result = call(
        &s.core,
        &s.driver.1,
        json!({"op": "capture", "value": {"maxSize": 400}}),
    );
    assert_eq!(result["isError"], false, "{result}");
}

#[test]
fn a_select_is_looked_up_in_the_tree_it_names_and_the_app_told_which() {
    let s = seen();
    let select = |arguments: Value| {
        let mut arguments = arguments;
        arguments["op"] = "select".into();
        arguments["project"] = "見本".into();
        call(&s.core, &s.driver.1, arguments)
    };
    let told = || {
        let asked = s.window.asked.lock().unwrap();
        asked.last().unwrap().0["branch"].clone()
    };
    // No branch is main, as for every tool, whatever the window has open: its sections, cards,
    // fields, rows and nodes.
    for target in [
        "Schemas:Status",
        "Schemas/Ordering",
        "Schemas:Customer.id",
        "Docs:Spec.intro",
        "Charts/Shop:Checkout.save",
    ] {
        let result = select(json!({"target": target}));
        assert_eq!(result["isError"], false, "{target}: {result}");
        assert_eq!(told(), "main");
    }
    // The board's cards and sections, by the board's name.
    for target in ["Goals:Ship", "Goals:Ship.Design", "Goals/Complete", "Goals"] {
        let result = select(json!({"branch": "_project_board_", "target": target}));
        assert_eq!(result["isError"], false, "{target}: {result}");
        assert_eq!(told(), "_project_board_");
    }
    assert_eq!(select(json!({"branch": "REFUNDS"}))["isError"], false);
    assert_eq!(told(), "refunds");
    // A card ghost by its own path, the window picking the ghost: one that leads nowhere too.
    common::ok(
        &s.core,
        "card_create",
        json!({"target": "Docs:Buyer", "value": {"kind": "ghost", "target": "Schemas:Customer"}}),
    );
    common::ok(&s.core, "card_create", json!({"target": "Free:Gone"}));
    common::ok(
        &s.core,
        "card_create",
        json!({"target": "Docs:Lost", "value": {"kind": "ghost", "target": "Free:Gone"}}),
    );
    common::ok(&s.core, "card_delete", json!({"target": "Free:Gone"}));
    for target in ["Docs:Buyer", "Docs:Buyer.name", "Docs:Lost"] {
        let result = select(json!({"target": target}));
        assert_eq!(result["isError"], false, "{target}: {result}");
    }

    // What the tree does not have is refused here, so the window is not moved for it.
    let asked = s.window.asked.lock().unwrap().len();
    let said = error_of(&select(json!({"target": "Goals:Ship"})));
    assert!(
        said.starts_with("Goals:Ship は main にありません（窓は動かしていません）: "),
        "{said}"
    );
    let said = error_of(&select(
        json!({"branch": "_project_board_", "target": "Schemas:Status"}),
    ));
    assert!(
        said.starts_with(
            "Schemas:Status は _project_board_ にありません（窓は動かしていません）: "
        ),
        "{said}"
    );
    let said = error_of(&select(json!({"target": "Nowhere"})));
    assert!(said.starts_with("Nowhere は main にありません"), "{said}");
    let said = error_of(&select(json!({"target": "Schemas:Status.nothing"})));
    assert!(
        said.starts_with("Schemas:Status.nothing は main にありません"),
        "{said}"
    );
    let said = error_of(&select(
        json!({"branch": "nope", "target": "Schemas:Status"}),
    ));
    assert!(said.contains("nope"), "{said}");
    assert_eq!(s.window.asked.lock().unwrap().len(), asked);
}

#[test]
fn a_core_no_app_was_handed_to_says_so() {
    let s = seen();
    s.core.close();
    let core = Core::open(s.core.place()).unwrap();
    let result = call(&core, &s.driver.1, json!({"op": "state"}));
    assert_eq!(
        error_of(&result),
        "app_control に答えるアプリがありません（画面を読む・動かすのは、開いているアプリです）"
    );
}

#[test]
fn a_listen_that_passes_is_left_to_the_http_side_to_open() {
    let s = seen();
    let meta = json!({"io.modelcontextprotocol/protocolVersion": "2026-07-28"});
    let message = json!({"jsonrpc": "2.0", "id": "listen:1", "method": "subscriptions/listen",
        "params": {"notifications": {"toolsListChanged": true}, "_meta": meta}});
    let headers =
        json!({"mcp-protocol-version": "2026-07-28", "mcp-method": "subscriptions/listen"});
    assert_eq!(
        mcp(&s.core, &s.reader, message.clone(), headers.clone()),
        json!({"status": 200, "body": null, "listen": true})
    );
    // Not without a key of the store's, nor with headers that disagree with the body.
    let refused = mcp(&s.core, "spk_nobody", message.clone(), headers);
    assert_eq!(refused["status"], 401);
    assert!(refused.get("listen").is_none());
    let mismatched = mcp(
        &s.core,
        &s.reader,
        message,
        json!({"mcp-protocol-version": "2026-07-28", "mcp-method": "tools/list"}),
    );
    assert_eq!(mismatched["status"], 400);
    assert!(mismatched.get("listen").is_none());
    // The 2025 family has no listen.
    let old = mcp(
        &s.core,
        &s.reader,
        json!({"jsonrpc": "2.0", "id": 2, "method": "subscriptions/listen"}),
        json!({}),
    );
    assert_eq!(
        old["body"]["error"]["message"],
        "未対応のメソッドです: subscriptions/listen"
    );
}
