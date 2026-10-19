//! Which projects an access key may touch through MCP (`specboard://SpecBoard/Docs/Design:DesignAccess`; the Swift app's
//! AccessKeyTests and the key check of `MCPServer`): a request carries one of the store's
//! keys or is turned away; a key sees the projects it was given and the public ones, writes only
//! those it was given to write, and a project it may not see is missing in the same words as one
//! that is not there. The keys are made, named, made again, revoked and given projects by the
//! window's requests.

use crate::common;

use common::{call, op};
use serde_json::{Value, json};
use specboard_core::Core;
use std::collections::BTreeMap;

/// What a request with no key of the store's is turned away with (`MCPAccess.keyNeeded`).
const KEY_NEEDED: &str = "MCP の接続にはアクセスキーが要ります（Authorization: Bearer <アクセスキー>）。プロジェクト一覧の場所の設定の「アクセスキー」で作ります";

/// Three projects beside the script's: one the key writes, one public, one it was given
/// nothing for; and the key, made through the window.
struct World {
    core: Core,
    own: String,
    shared: String,
    other: String,
    /// The key's id and value.
    key: (String, String),
}

fn project_id(core: &Core, name: &str) -> String {
    rusqlite::Connection::open(core.place())
        .unwrap()
        .query_row("SELECT id FROM projects WHERE name = ?", [name], |r| {
            r.get(0)
        })
        .unwrap()
}

fn world() -> World {
    let core = Core::open(common::script_store()).unwrap();
    // Made through MCP by the tests' own key, which writes them then.
    for name in ["自分の", "公開の", "よその"] {
        let (refused, said) = call(&core, "project_create", json!({"project": name}));
        assert!(!refused, "{said}");
    }
    let (own, shared, other) = (
        project_id(&core, "自分の"),
        project_id(&core, "公開の"),
        project_id(&core, "よその"),
    );
    let made = op(
        &core,
        json!({"op": "access_key_create", "name": "自分の開発"}),
    );
    let key = (
        made["result"]["key"]["id"].as_str().unwrap().to_string(),
        made["result"]["value"].as_str().unwrap().to_string(),
    );
    assert_eq!(
        op(
            &core,
            json!({"op": "access_grant", "key": key.0, "project": own, "level": "write"})
        ),
        json!({"result": {}})
    );
    assert_eq!(
        op(
            &core,
            json!({"op": "project_public_set", "project": shared, "on": true})
        ),
        json!({"result": {}})
    );
    World {
        core,
        own,
        shared,
        other,
        key,
    }
}

/// One message to `mcp` with these headers: its status and body.
fn mcp(core: &Core, message: Value, headers: Value) -> Value {
    op(
        core,
        json!({"op": "mcp", "message": message, "headers": headers}),
    )["result"]
        .clone()
}

fn bearer(value: &str) -> Value {
    json!({ "authorization": format!("Bearer {value}") })
}

/// The value of the key the tests call with (`common::headers`).
fn tests_key(core: &Core) -> String {
    common::headers(core)["authorization"]
        .as_str()
        .unwrap()
        .trim_start_matches("Bearer ")
        .to_string()
}

/// A tool called as MCP calls it with the key `value`: whether it was refused, and what it
/// said (the error alone, when refused).
fn tool(core: &Core, value: &str, name: &str, arguments: Value) -> (bool, Value) {
    let message = json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": name, "arguments": arguments}});
    let reply = mcp(core, message, bearer(value));
    assert_eq!(reply["status"], 200, "{reply}");
    let result = &reply["body"]["result"];
    let text = result["content"][0]["text"].as_str().unwrap_or_default();
    let said: Value = serde_json::from_str(text).unwrap_or_else(|_| text.into());
    if result["isError"] == true {
        return (true, said["error"]["message"].clone());
    }
    (false, said)
}

fn ok(core: &Core, value: &str, name: &str, arguments: Value) -> Value {
    let (refused, said) = tool(core, value, name, arguments);
    assert!(!refused, "{name}: {said}");
    said
}

fn refused(core: &Core, value: &str, name: &str, arguments: Value) -> String {
    let (refused, said) = tool(core, value, name, arguments);
    assert!(refused, "{name} が通った: {said}");
    said.as_str().unwrap().to_string()
}

/// Project → access, as `project_list` gives them (under `projects`).
fn listed(said: &Value) -> BTreeMap<String, String> {
    said["projects"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| {
            (
                row["project"].as_str().unwrap().to_string(),
                row["access"].as_str().unwrap_or("?").to_string(),
            )
        })
        .collect()
}

#[test]
fn a_request_without_a_key_of_the_stores_is_turned_away() {
    let w = world();
    let initialize = json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-06-18"}});
    let turned_away = |headers: Value| {
        let reply = mcp(&w.core, initialize.clone(), headers.clone());
        assert_eq!(
            reply,
            json!({"status": 401, "body": KEY_NEEDED}),
            "{headers}"
        );
    };
    turned_away(json!({}));
    turned_away(json!({"authorization": "Bearer "}));
    turned_away(json!({"authorization": "Basic abc"}));
    turned_away(json!({"authorization": w.key.1}));
    turned_away(bearer("spk_00000000000000000000000000000000"));
    // The handshake goes through with the key, in any case of the word before it.
    let reply = mcp(
        &w.core,
        initialize.clone(),
        json!({"authorization": format!("bearer {}", w.key.1)}),
    );
    assert_eq!(reply["status"], 200, "{reply}");
    // Revoked, it is not one of the store's any more: the next request is turned away.
    assert_eq!(
        op(&w.core, json!({"op": "access_key_revoke", "key": w.key.0})),
        json!({"result": {}})
    );
    turned_away(bearer(&w.key.1));
    // So is a request with no body to speak of: the key comes first.
    let reply = mcp(&w.core, Value::Null, json!({}));
    assert_eq!(reply["status"], 401);
}

#[test]
fn a_key_made_again_keeps_what_it_was_given_and_turns_the_old_value_away() {
    let w = world();
    let before = listed(&ok(&w.core, &w.key.1, "project_list", json!({})));
    let row = |core: &Core| {
        op(core, json!({"op": "access_keys"}))["result"]["keys"]
            .as_array()
            .unwrap()
            .iter()
            .find(|k| k["id"] == w.key.0.as_str())
            .cloned()
            .unwrap()
    };
    let had = row(&w.core);
    assert!(had["usedAt"].is_string(), "{had}");

    let again = op(
        &w.core,
        json!({"op": "access_key_regenerate", "key": w.key.0}),
    );
    let value = again["result"]["value"].as_str().unwrap().to_string();
    assert!(value.starts_with("spk_") && value.len() == 36, "{value}");
    assert_ne!(value, w.key.1);
    // The same key: its id, name, what it may do and the projects it was given; not used yet.
    let key = &again["result"]["key"];
    for item in ["id", "name", "permits", "createdAt", "grants"] {
        assert_eq!(key[item], had[item], "{item}: {key}");
    }
    assert!(key["usedAt"].is_null(), "{key}");
    assert_eq!(row(&w.core), *key);

    // The value before is not one of the store's any more; the new one sees what it saw.
    let initialize = json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-06-18"}});
    assert_eq!(
        mcp(&w.core, initialize, bearer(&w.key.1)),
        json!({"status": 401, "body": KEY_NEEDED})
    );
    assert_eq!(
        listed(&ok(&w.core, &value, "project_list", json!({}))),
        before
    );

    assert_eq!(
        op(
            &w.core,
            json!({"op": "access_key_regenerate", "key": "nothing"})
        )["error"],
        "アクセスキーが見つかりません"
    );
}

#[test]
fn project_list_gives_what_the_key_sees_and_how() {
    let w = world();
    let own = listed(&ok(&w.core, &w.key.1, "project_list", json!({})));
    assert_eq!(
        own,
        BTreeMap::from([
            ("公開の".to_string(), "read".to_string()),
            ("自分の".to_string(), "write".to_string()),
        ])
    );
    // The tests' key was given every project.
    let all = listed(&ok(&w.core, &tests_key(&w.core), "project_list", json!({})));
    assert_eq!(all.len(), 4, "{all:?}");
    assert!(all.values().all(|a| a == "write"));
    let read = ok(
        &w.core,
        &w.key.1,
        "project_read",
        json!({"project": "公開の"}),
    );
    assert_eq!(read["access"], "read");
    let read = ok(
        &w.core,
        &w.key.1,
        "project_read",
        json!({"project": "自分の"}),
    );
    assert_eq!(read["access"], "write");
}

#[test]
fn a_project_the_key_may_not_see_is_missing_as_one_that_is_not_there() {
    let w = world();
    let unseen = refused(
        &w.core,
        &w.key.1,
        "project_read",
        json!({"project": "よその"}),
    );
    let absent = refused(
        &w.core,
        &w.key.1,
        "project_read",
        json!({"project": "無い"}),
    );
    assert_eq!(unseen.replace("よその", "無い"), absent);
    assert_eq!(
        absent,
        "プロジェクト「無い」がありません。あるのは: 公開の / 自分の"
    );
    // By id too, and for the tools that write.
    let by_id = refused(
        &w.core,
        &w.key.1,
        "project_read",
        json!({"project": w.other}),
    );
    assert_eq!(
        by_id,
        format!(
            "プロジェクト「{}」がありません。あるのは: 公開の / 自分の",
            w.other
        )
    );
    let write = refused(
        &w.core,
        &w.key.1,
        "project_update",
        json!({"project": "よその", "value": {"description": "書いた"}}),
    );
    assert!(
        write.starts_with("プロジェクト「よその」がありません"),
        "{write}"
    );
    // A key that sees none is told so.
    let made = op(
        &w.core,
        json!({"op": "access_key_create", "name": "何もない"}),
    );
    let nothing = made["result"]["value"].as_str().unwrap();
    op(
        &w.core,
        json!({"op": "project_public_set", "project": w.shared, "on": false}),
    );
    assert_eq!(
        refused(&w.core, nothing, "card_list", json!({"project": "自分の"})),
        "プロジェクト「自分の」がありません。このアクセスキーから見えるプロジェクトはありません"
    );
    assert_eq!(
        ok(&w.core, nothing, "project_list", json!({})),
        json!({"projects": []})
    );
}

#[test]
fn a_write_to_a_project_the_key_only_reads_is_refused_as_read_only() {
    let w = world();
    let description = |text: &str| json!({"description": text});
    let on_public = refused(
        &w.core,
        &w.key.1,
        "project_update",
        json!({"project": "公開の", "value": description("書いた")}),
    );
    assert_eq!(
        on_public,
        "プロジェクト「公開の」は、このアクセスキーでは読むだけです（書けるようにするのは、プロジェクト一覧の場所の設定の「アクセスキー」で）"
    );
    op(
        &w.core,
        json!({"op": "access_grant", "key": w.key.0, "project": w.other, "level": "read"}),
    );
    for (name, arguments) in [
        (
            "project_update",
            json!({"project": "よその", "value": description("書いた")}),
        ),
        (
            "branch_create",
            json!({"project": "よその", "target": "work"}),
        ),
        (
            "section_create",
            json!({"project": "よその", "target": "Schemas/Billing"}),
        ),
        ("batch", json!({"project": "よその"})),
    ] {
        let said = refused(&w.core, &w.key.1, name, arguments);
        assert!(
            said.contains("プロジェクト「よその」は、このアクセスキーでは読むだけです"),
            "{name}: {said}"
        );
    }
    // Reading it is as for any key that may.
    ok(
        &w.core,
        &w.key.1,
        "section_list",
        json!({"project": "よその"}),
    );
    ok(
        &w.core,
        &w.key.1,
        "project_update",
        json!({"project": "自分の", "value": description("書いた")}),
    );
    let descriptions: BTreeMap<String, String> = {
        let conn = rusqlite::Connection::open(w.core.place()).unwrap();
        let mut stmt = conn
            .prepare("SELECT id, description FROM projects")
            .unwrap();
        stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    };
    assert_eq!(descriptions[&w.own], "書いた");
    assert_eq!(descriptions[&w.shared], "");
    assert_eq!(descriptions[&w.other], "");
}

#[test]
fn what_a_key_writes_goes_in_the_history_under_its_name() {
    let w = world();
    ok(
        &w.core,
        &w.key.1,
        "project_update",
        json!({"project": "自分の", "value": {"description": "書いた"}}),
    );
    let commits = op(&w.core, json!({"op": "commits", "project": w.own}));
    let newest = &commits["result"]["commits"][0];
    assert_eq!(newest["authorKind"], "mcp", "{commits}");
    assert_eq!(newest["author"], "自分の開発");
    let read = ok(
        &w.core,
        &w.key.1,
        "branch_read",
        json!({"project": "自分の", "target": "main", "value": {"as": "history", "limit": 1}}),
    );
    assert_eq!(read["commits"][0]["author"], "MCP・自分の開発", "{read}");
    // A request to merge is the key's too.
    ok(
        &w.core,
        &w.key.1,
        "branch_create",
        json!({"project": "自分の", "target": "work"}),
    );
    ok(
        &w.core,
        &w.key.1,
        "section_create",
        json!({"project": "自分の", "branch": "work", "target": "Schemas/Billing"}),
    );
    // Into a locked main, a merge request (branch_merge, not a tool of its own).
    let locked = op(
        &w.core,
        json!({"op": "main_lock", "project": w.own, "on": true}),
    );
    assert!(locked.get("error").is_none(), "{locked}");
    let request = ok(
        &w.core,
        &w.key.1,
        "branch_merge",
        json!({"project": "自分の", "target": "work", "value": {"title": "請求"}}),
    );
    assert_eq!(request["mr"], 1, "{request}");
    assert_eq!(request["author"], "MCP（自分の開発）", "{request}");
}

#[test]
fn a_project_made_through_a_key_is_that_keys_alone() {
    let w = world();
    ok(
        &w.core,
        &w.key.1,
        "project_create",
        json!({"project": "新しい"}),
    );
    assert_eq!(
        listed(&ok(&w.core, &w.key.1, "project_list", json!({})))["新しい"],
        "write"
    );
    let made = op(
        &w.core,
        json!({"op": "access_key_create", "name": "よその開発"}),
    );
    let stranger = made["result"]["value"].as_str().unwrap();
    assert_eq!(
        listed(&ok(&w.core, stranger, "project_list", json!({}))),
        BTreeMap::from([("公開の".to_string(), "read".to_string())])
    );
}

#[test]
fn a_batch_is_made_only_by_a_key_that_writes_its_project_and_is_missing_to_one_that_cannot_see_it()
{
    let w = world();
    let said = refused(&w.core, &w.key.1, "batch", json!({"project": "公開の"}));
    assert!(said.contains("読むだけ"), "{said}");
    let opened = ok(
        &w.core,
        &w.key.1,
        "batch",
        json!({"project": "自分の", "value": {"open": true, "steps": [{"tool": "project_update", "arguments": {"value": {"description": "バッチで"}}}]}}),
    );
    let token = opened["batch"].as_str().unwrap();
    let made = op(
        &w.core,
        json!({"op": "access_key_create", "name": "よその開発"}),
    );
    let stranger = made["result"]["value"].as_str().unwrap();
    let unseen = refused(&w.core, stranger, "batch", json!({"target": token}));
    assert_eq!(
        unseen,
        format!(
            "バッチ「{token}」はありません — 流した・捨てたか、7 日触られず片付けられました。batch で開き直してください"
        )
    );
    // Given the project to read, it may neither try the batch (that holds the tree) nor run it.
    let stranger_id = made["result"]["key"]["id"].as_str().unwrap();
    op(
        &w.core,
        json!({"op": "access_grant", "key": stranger_id, "project": w.own, "level": "read"}),
    );
    for arguments in [
        json!({"target": token}),
        json!({"target": token, "execute": true}),
    ] {
        let said = refused(&w.core, stranger, "batch", arguments);
        assert!(
            said.contains("プロジェクト「自分の」は、このアクセスキーでは読むだけです"),
            "{said}"
        );
    }
}

/// What a call is told for the key's items (as `<id>（<name>）`) that are denied.
fn key_denied(items: &[&str]) -> String {
    format!(
        "このアクセスキーでは、{} は使えません（拒否）。変えられるのは人だけで、プロジェクト一覧の場所の設定の「アクセスキー」でこのキーの項目を許可か都度承認にします",
        items.join("・")
    )
}

/// What a call is told for the key's items asked each time, when it did not say `request: true`.
fn key_unrequested(item: &str, tool: &str) -> String {
    format!(
        "{item} は都度承認です。{tool} に request: true を付けて呼び直すと、画面に許可を求めて答えを待ちます（2 分まで）"
    )
}

const CONTROL: &str = "app_control（画面を選ぶ・見え方を変える）";
const CAPTURE: &str = "app_control.capture（画面を撮る）";

/// What the core says once a call passes and nothing answers for the window.
const NO_APP: &str =
    "app_control に答えるアプリがありません（画面を読む・動かすのは、開いているアプリです）";

fn permit(w: &World, permits: Value) -> Value {
    let permitted = op(
        &w.core,
        json!({"op": "access_key_permit", "key": w.key.0, "permits": permits}),
    );
    assert!(permitted.get("error").is_none(), "{permitted}");
    permitted["result"]["key"]["permits"].clone()
}

#[test]
fn app_control_needs_what_its_op_does_with_the_window() {
    let w = world();
    // Asked each time, by default: refused when not asked for.
    for asked in ["state", "select", "view", "windows"] {
        let said = refused(&w.core, &w.key.1, "app_control", json!({"op": asked}));
        assert_eq!(said, key_unrequested(CONTROL, "app_control"), "{asked}");
    }
    let said = refused(&w.core, &w.key.1, "app_control", json!({"op": "capture"}));
    assert_eq!(said, key_unrequested(CAPTURE, "app_control"));
    // Allowed to choose and change how it looks: not to take pictures of it.
    assert_eq!(
        permit(&w, json!({"app_control": "allow"})),
        json!({"app_control": "allow", "app_control.capture": "ask", "project_create": "allow"})
    );
    // Let through: the core does not move the window itself, and no app was handed over to
    // answer it (`tests/app.rs` hands one over).
    let said = refused(&w.core, &w.key.1, "app_control", json!({"op": "state"}));
    assert_eq!(said, NO_APP);
    let said = refused(&w.core, &w.key.1, "app_control", json!({"op": "capture"}));
    assert_eq!(said, key_unrequested(CAPTURE, "app_control"));
    // And the other way about; denied, refused whether asked for or not.
    permit(
        &w,
        json!({"app_control": "deny", "app_control.capture": "allow"}),
    );
    let said = refused(
        &w.core,
        &w.key.1,
        "app_control",
        json!({"op": "view", "request": true}),
    );
    assert_eq!(said, key_denied(&[CONTROL]));
    let said = refused(&w.core, &w.key.1, "app_control", json!({"op": "capture"}));
    assert_eq!(said, NO_APP);
}

#[test]
fn a_key_makes_projects_only_while_it_may() {
    let w = world();
    // A key is made able to, as every key was.
    ok(
        &w.core,
        &w.key.1,
        "project_create",
        json!({"project": "作れる"}),
    );
    assert_eq!(
        permit(&w, json!({"project_create": "deny"}))["project_create"],
        "deny"
    );
    let said = refused(
        &w.core,
        &w.key.1,
        "project_create",
        json!({"project": "作れない"}),
    );
    assert_eq!(said, key_denied(&["project_create（プロジェクトを作る）"]));
    assert!(!listed(&ok(&w.core, &w.key.1, "project_list", json!({}))).contains_key("作れない"));
    permit(&w, json!({"project_create": "ask"}));
    let said = refused(
        &w.core,
        &w.key.1,
        "project_create",
        json!({"project": "作れない"}),
    );
    assert_eq!(
        said,
        key_unrequested("project_create（プロジェクトを作る）", "project_create")
    );
    // What it was given it still writes.
    ok(
        &w.core,
        &w.key.1,
        "project_update",
        json!({"project": "自分の", "value": {"description": "書ける"}}),
    );
    // Made with what it may do said, each apart.
    let made = op(
        &w.core,
        json!({"op": "access_key_create", "name": "撮るだけ", "permits": {"app_control.capture": "allow", "project_create": "deny"}}),
    );
    assert_eq!(
        made["result"]["key"]["permits"],
        json!({"app_control": "ask", "app_control.capture": "allow", "project_create": "deny"})
    );
    // What the window asks is checked.
    assert_eq!(
        op(&w.core, json!({"op": "access_key_permit", "key": w.key.0}))["error"],
        "access_key_permit には permits（{項目: allow・ask・deny}。項目は app_control・app_control.capture・project_create）が要ります"
    );
    assert_eq!(
        op(
            &w.core,
            json!({"op": "access_key_permit", "key": w.key.0, "permits": {"project_create": "yes"}})
        )["error"],
        "access_key_permit の permits の project_create は allow・ask・deny のどれかです（\"yes\" ではなく）"
    );
    assert_eq!(
        op(
            &w.core,
            json!({"op": "access_key_permit", "key": "nope", "permits": {"project_create": "allow"}})
        )["error"],
        "アクセスキーが見つかりません"
    );
}

#[test]
fn a_key_is_said_once_kept_as_its_digest_and_noted_when_used() {
    let w = world();
    let value = &w.key.1;
    assert!(value.starts_with("spk_") && value.len() == 36, "{value}");
    assert!(
        value[4..]
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
    );
    let mut kept = std::fs::read(w.core.place()).unwrap();
    kept.extend(std::fs::read(format!("{}-wal", w.core.place().display())).unwrap_or_default());
    assert!(
        !kept.windows(value.len()).any(|w| w == value.as_bytes()),
        "値がデータベースに残っている"
    );
    let keys = |core: &Core| op(core, json!({"op": "access_keys"}))["result"]["keys"].clone();
    let mine = |all: &Value| {
        all.as_array()
            .unwrap()
            .iter()
            .find(|k| k["name"] == "自分の開発")
            .unwrap()
            .clone()
    };
    let before = mine(&keys(&w.core));
    assert_eq!(before["id"], w.key.0.as_str());
    assert_eq!(
        before["permits"],
        json!({"app_control": "ask", "app_control.capture": "ask", "project_create": "allow"})
    );
    assert!(before["usedAt"].is_null(), "{before}");
    assert_eq!(before["grants"], json!({ w.own.clone(): "write" }));
    assert!(before.get("value").is_none());
    ok(&w.core, value, "project_list", json!({}));
    let after = mine(&keys(&w.core));
    let used = after["usedAt"].as_str().unwrap().to_string();
    // Noted once a minute at the most.
    ok(&w.core, value, "project_list", json!({}));
    assert_eq!(mine(&keys(&w.core))["usedAt"], used.as_str());
    // In the order they were made: the tests' key came first.
    let names: Vec<String> = keys(&w.core)
        .as_array()
        .unwrap()
        .iter()
        .map(|k| k["name"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(names, [common::KEY, "自分の開発"]);
}

#[test]
fn the_window_names_revokes_and_gives_projects_to_keys() {
    let w = world();
    let call = |request: Value| op(&w.core, request);
    assert_eq!(
        call(json!({"op": "access_key_rename", "key": w.key.0, "name": "  名前を変えた "})),
        json!({"result": {}})
    );
    assert_eq!(
        call(json!({"op": "access_key_rename", "key": w.key.0, "name": " "}))["error"],
        "アクセスキーの名前 を入力してください"
    );
    assert_eq!(
        call(json!({"op": "access_key_rename", "key": w.key.0, "name": "あ".repeat(81)}))["error"],
        "アクセスキーの名前 は 80 文字以内です"
    );
    assert_eq!(
        call(json!({"op": "access_key_create", "name": ""}))["error"],
        "アクセスキーの名前 を入力してください"
    );
    assert_eq!(
        call(json!({"op": "access_key_rename", "key": "nope", "name": "x"}))["error"],
        "アクセスキーが見つかりません"
    );
    // The new name is who writes from now on.
    ok(
        &w.core,
        &w.key.1,
        "project_update",
        json!({"project": "自分の", "value": {"description": "書いた"}}),
    );
    let commits = call(json!({"op": "commits", "project": w.own}));
    assert_eq!(commits["result"]["commits"][0]["author"], "名前を変えた");

    // Given, changed and taken away.
    for level in ["read", "write"] {
        call(json!({"op": "access_grant", "key": w.key.0, "project": w.other, "level": level}));
        let keys = call(json!({"op": "access_keys"}));
        let mine = keys["result"]["keys"]
            .as_array()
            .unwrap()
            .iter()
            .find(|k| k["id"] == w.key.0.as_str())
            .unwrap()
            .clone();
        assert_eq!(mine["grants"][&w.other], level);
    }
    call(json!({"op": "access_grant", "key": w.key.0, "project": w.other}));
    let unseen = refused(
        &w.core,
        &w.key.1,
        "project_read",
        json!({"project": "よその"}),
    );
    assert!(
        unseen.starts_with("プロジェクト「よその」がありません"),
        "{unseen}"
    );
    assert_eq!(
        call(json!({"op": "access_grant", "key": w.key.0, "project": w.other, "level": "admin"}))["error"],
        "level は read か write です（admin は読めません）"
    );
    assert_eq!(
        call(json!({"op": "access_grant", "key": w.key.0, "project": "nope", "level": "read"}))["error"],
        "プロジェクトが見つかりません"
    );

    // Public, and back.
    assert_eq!(
        call(json!({"op": "project_public"})),
        json!({"result": {"projects": [w.shared]}})
    );
    call(json!({"op": "project_public_set", "project": w.shared, "on": false}));
    assert_eq!(
        call(json!({"op": "project_public"})),
        json!({"result": {"projects": []}})
    );
    assert_eq!(
        listed(&ok(&w.core, &w.key.1, "project_list", json!({}))),
        BTreeMap::from([("自分の".to_string(), "write".to_string())])
    );

    // Revoked: gone, with what it was given.
    call(json!({"op": "access_key_revoke", "key": w.key.0}));
    let left: i64 = rusqlite::Connection::open(w.core.place())
        .unwrap()
        .query_row(
            "SELECT COUNT(*) FROM access_grants WHERE key_id = ?",
            [&w.key.0],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(left, 0);
    assert_eq!(
        call(json!({"op": "access_key_revoke", "key": w.key.0}))["error"],
        "アクセスキーが見つかりません"
    );
}

#[test]
fn a_project_file_read_in_its_place_keeps_its_keys_and_being_public() {
    let w = world();
    let text =
        op(&w.core, json!({"op": "project_file", "project": w.shared}))["result"]["text"].clone();
    op(
        &w.core,
        json!({"op": "access_grant", "key": w.key.0, "project": w.shared, "level": "write"}),
    );
    let read = op(&w.core, json!({"op": "project_file_read", "text": text}));
    assert_eq!(read["result"]["project"], w.shared.as_str(), "{read}");
    assert_eq!(
        op(&w.core, json!({"op": "project_public"}))["result"]["projects"],
        json!([w.shared])
    );
    assert_eq!(
        listed(&ok(&w.core, &w.key.1, "project_list", json!({})))["公開の"],
        "write"
    );
    // A copy is a project of its own: no key was given it, and it is not public.
    let copy = op(
        &w.core,
        json!({"op": "project_file_read", "text": text, "landing": "copy", "name": "写し"}),
    );
    assert!(copy.get("error").is_none(), "{copy}");
    assert!(!listed(&ok(&w.core, &w.key.1, "project_list", json!({}))).contains_key("写し"));
}
