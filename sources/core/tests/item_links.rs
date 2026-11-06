//! A formatdoc's item read for what it is tied to (`card_read`'s `as: impact` given the item): the
//! places its rows name, the items it points at and the writings pointing at it, each with its
//! heading and the head of its text, as the property panel lists them.

use crate::common;

use common::{ok, script_store};
use serde_json::json;
use specboard_core::Core;

#[test]
fn an_item_says_what_it_points_at_and_what_points_at_it() {
    let core = Core::open(script_store()).unwrap();
    ok(
        &core,
        "card_create",
        json!({"value": {
            "Docs:Rules": {"kind": "formatdoc", "rows": [{"name": "limits", "kind": "item", "heading": "上限",
                "rows": [{"name": "body", "kind": "text", "body": "1 回に 10 件"}]}]},
            "Docs:Auth": {"kind": "formatdoc", "rows": [{"name": "auth", "kind": "item", "heading": "認証",
                "rows": [{"name": "body", "kind": "text", "body": "上限は [[Docs:Rules.limits]]"}]},
                {"name": "alone", "kind": "item", "heading": "ひとり"}]},
            "Docs:Memo": {"kind": "note", "body": "[[Docs:Auth.auth]] を読む"}
        }}),
    );
    let read = ok(
        &core,
        "card_read",
        json!({"target": "Docs:Auth.auth", "value": {"as": "impact"}}),
    );
    assert_eq!(read["names"], json!(["Docs:Rules.limits"]), "{read}");
    assert_eq!(
        read["items"]["mentions"],
        json!([{"path": "Docs:Rules.limits", "heading": "上限", "head": "1 回に 10 件"}]),
        "{read}"
    );
    assert_eq!(
        read["items"]["mentionedBy"],
        json!([{"path": "Docs:Memo", "heading": "Memo", "head": "[[Docs:Auth.auth]] を読む"}]),
        "{read}"
    );
    // An item tied to nothing: both lists empty.
    let alone = ok(
        &core,
        "card_read",
        json!({"target": "Docs:Auth.alone", "value": {"as": "impact"}}),
    );
    for list in ["mentions", "mentionedBy"] {
        assert!(
            alone["items"][list].as_array().is_none_or(|l| l.is_empty()),
            "{alone}"
        );
    }
}
