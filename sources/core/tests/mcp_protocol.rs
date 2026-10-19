//! The two eras of the protocol as the core answers them (op `mcp`): what the 2026-07-28 era's
//! results carry and what it refuses, and the 2025 family left as it was. The HTTP side's part
//! is the agent's daemon's to try (`agent/tests/daemon_mcp.rs`).

use crate::common;

use serde_json::{Value, json};
use specboard_core::Core;

const MODERN: &str = "2026-07-28";
const SERVER_INFO: &str = "io.modelcontextprotocol/serverInfo";

/// One message to the core with the store's key, as the HTTP side hands it on.
fn mcp(core: &Core, message: Value, mut headers: Value) -> Value {
    headers["authorization"] = common::headers(core)["authorization"].clone();
    let reply: Value = serde_json::from_str(
        &core.call(&json!({"op": "mcp", "message": message, "headers": headers}).to_string()),
    )
    .unwrap();
    reply["result"].clone()
}

/// A 2026-07-28 request as a client sends it: the version in the body's `_meta` and the headers
/// that repeat the body (`mcp-name` too for a call). `version` stands for both.
fn modern(core: &Core, method: &str, params: Value, version: &str, change: Value) -> Value {
    let mut params = params;
    params["_meta"] = json!({
        "io.modelcontextprotocol/protocolVersion": version,
        "io.modelcontextprotocol/clientInfo": {"name": "test", "version": "0"},
        "io.modelcontextprotocol/clientCapabilities": {},
    });
    let mut headers = json!({"mcp-protocol-version": version, "mcp-method": method});
    if method == "tools/call" {
        headers["mcp-name"] = params["name"].clone();
    }
    for (name, value) in change.as_object().unwrap() {
        if value.is_null() {
            headers.as_object_mut().unwrap().remove(name);
        } else {
            headers[name] = value.clone();
        }
    }
    mcp(
        core,
        json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params}),
        headers,
    )
}

#[test]
fn discover_names_the_modern_era_only_and_says_the_list_changes() {
    let core = Core::open(common::script_store()).unwrap();
    let found = modern(&core, "server/discover", json!({}), MODERN, json!({}));
    assert_eq!(found["status"], 200);
    let result = &found["body"]["result"];
    assert_eq!(result["resultType"], "complete");
    // Only modern revisions. The 2025 family is reached through `initialize`; offering it here
    // would offer a version this path does not speak.
    assert_eq!(result["supportedVersions"], json!([MODERN]));
    // What makes Claude Code register for list changes when it connects.
    assert_eq!(result["capabilities"]["tools"]["listChanged"], true);
    assert!(result["_meta"][SERVER_INFO].is_object(), "{result}");
    assert!(
        result["ttlMs"].is_number() && result["cacheScope"].is_string(),
        "{result}"
    );
}

#[test]
fn the_modern_results_carry_their_fields() {
    let core = Core::open(common::script_store()).unwrap();
    let listed = modern(&core, "tools/list", json!({}), MODERN, json!({}));
    let list = &listed["body"]["result"];
    assert_eq!(list["resultType"], "complete");
    assert_eq!(list["cacheScope"], "private");
    assert_eq!(list["ttlMs"], 5 * 60 * 1000);
    assert!(
        list["tools"]
            .as_array()
            .unwrap()
            .iter()
            .any(|t| t["name"] == "guide")
    );

    let called = modern(
        &core,
        "tools/call",
        json!({"name": "guide", "arguments": {"about": "glossary"}}),
        MODERN,
        json!({}),
    );
    assert_eq!(called["status"], 200);
    let result = &called["body"]["result"];
    assert_eq!(result["resultType"], "complete");
    assert_eq!(result["isError"], false, "{result}");
    assert!(result["_meta"][SERVER_INFO].is_object(), "{result}");
}

#[test]
fn headers_that_disagree_with_the_body_are_refused() {
    let core = Core::open(common::script_store()).unwrap();
    let call = json!({"name": "guide", "arguments": {}});

    let missing = modern(
        &core,
        "tools/list",
        json!({}),
        MODERN,
        json!({"mcp-method": null}),
    );
    assert_eq!(missing["status"], 400);
    assert_eq!(missing["body"]["error"]["code"], -32020);

    let wrong = modern(
        &core,
        "tools/call",
        call.clone(),
        MODERN,
        json!({"mcp-name": "other"}),
    );
    assert_eq!(wrong["status"], 400);
    assert_eq!(wrong["body"]["error"]["code"], -32020);

    // A name sent in the base64 form is compared after decoding ("guide").
    let encoded = modern(
        &core,
        "tools/call",
        call,
        MODERN,
        json!({"mcp-name": "=?base64?Z3VpZGU=?="}),
    );
    assert_eq!(encoded["status"], 200, "{encoded}");
}

#[test]
fn an_unknown_version_is_refused_with_the_ones_there_are() {
    let core = Core::open(common::script_store()).unwrap();
    let refused = modern(&core, "tools/list", json!({}), "2099-01-01", json!({}));
    assert_eq!(refused["status"], 400);
    let error = &refused["body"]["error"];
    assert_eq!(error["code"], -32022);
    assert!(error.to_string().contains(MODERN), "{error}");
}

#[test]
fn a_method_the_modern_era_has_not_is_not_found() {
    let core = Core::open(common::script_store()).unwrap();
    // `ping` is not a method of 2026-07-28.
    let ping = modern(&core, "ping", json!({}), MODERN, json!({}));
    assert_eq!(ping["status"], 404);
    assert_eq!(ping["body"]["error"]["code"], -32601);
}

#[test]
fn the_2025_family_is_as_it_was() {
    let core = Core::open(common::script_store()).unwrap();
    let init = mcp(
        &core,
        json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-11-25", "capabilities": {}, "clientInfo": {"name": "t", "version": "0"}}}),
        json!({}),
    );
    assert_eq!(init["status"], 200);
    // Nothing is pushed on a 2025 connection, so it declares nothing.
    assert_eq!(
        init["body"]["result"]["capabilities"]["tools"]["listChanged"],
        false
    );
    // And a 2025 `tools/list` has none of the 2026 fields.
    let listed = mcp(
        &core,
        json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
        json!({}),
    );
    let list = &listed["body"]["result"];
    assert!(list["tools"].is_array());
    assert!(
        list.get("resultType").is_none() && list.get("ttlMs").is_none(),
        "{list}"
    );
}
