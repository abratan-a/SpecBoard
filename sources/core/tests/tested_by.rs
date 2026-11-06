//! A spec's item is tied to the tests that check it by a line every project has: `testedBy`.

use crate::common;

use common::{ok, script_store};
use serde_json::json;
use specboard_core::Core;

#[test]
fn a_spec_item_is_tied_to_its_tests_by_a_built_in_line() {
    let core = Core::open(script_store()).unwrap();
    ok(
        &core,
        "card_create",
        json!({"value": {
            "Docs:LoginSpec": {"kind": "formatdoc", "rows": [{"name": "login", "kind": "item", "heading": "ログイン"}]},
            "Docs:LoginTest": {"kind": "note", "body": "ログインのテスト"}
        }}),
    );
    ok(
        &core,
        "card_update",
        json!({"target": "Docs:LoginSpec", "value": {"connections": {"add": [
            {"line": "testedBy", "from": "login", "to": "Docs:LoginTest"}
        ]}}}),
    );
    let read = ok(
        &core,
        "card_read",
        json!({"target": "Docs:LoginSpec", "value": {"include": ["connections"]}}),
    );
    assert!(
        read["connections"]
            .as_array()
            .unwrap()
            .contains(&json!({"line": "testedBy", "from": "login", "to": "Docs:LoginTest"})),
        "{read}"
    );
    let kind = ok(&core, "kind_read", json!({"target": "testedBy"}));
    assert_eq!(kind["builtin"], true, "{kind}");
    assert_eq!(
        kind["look"],
        json!({"color": "green", "dash": "dotted", "begin": "none", "end": "arrow"}),
        "{kind}"
    );
    assert_eq!(kind["visible"], false, "{kind}");
    assert_eq!(kind["label"], "確かめるテスト", "{kind}");
}
