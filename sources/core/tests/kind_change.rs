//! A card's kind changed in place (`card_update`'s `kind`): kept where it is when its section and
//! what it holds fit the new kind, refused with the place or what it holds when they do not.

use crate::common;

use common::{ok, refused, script_store};
use serde_json::json;
use specboard_core::Core;

#[test]
fn a_kind_is_changed_only_where_its_place_and_what_it_holds_fit() {
    let core = Core::open(script_store()).unwrap();
    ok(
        &core,
        "card_create",
        json!({"target": "Schemas:Money", "value": {"kind": "entity", "fields": [{"name": "amount", "type": "int"}]}}),
    );
    ok(
        &core,
        "card_update",
        json!({"target": "Schemas:Money", "value": {"kind": "value"}}),
    );
    let read = ok(&core, "card_read", json!({"target": "Schemas:Money"}));
    assert_eq!(read["kind"], "value", "{read}");
    assert_eq!(read["path"], "Schemas:Money", "{read}");
    // A kind its section does not hold: refused, saying the section.
    let said = refused(
        &core,
        "card_update",
        json!({"target": "Schemas:Money", "value": {"kind": "flow"}}),
    );
    assert!(said.contains("Schemas"), "{said}");
    // A goal and a card of another kind, a card and a card ghost: made again, not changed.
    let said = refused(
        &core,
        "card_update",
        json!({"branch": "_project_board_", "target": "Goals:Ship", "value": {"kind": "note"}}),
    );
    assert!(
        said.contains("goal・task とほかの種類は互いに変えられません"),
        "{said}"
    );
    let said = refused(
        &core,
        "card_update",
        json!({"target": "Schemas:Money", "value": {"kind": "ghost"}}),
    );
    assert!(said.contains("カードゴーストにはできません"), "{said}");
    // A container holding cards the new kind cannot hold: move or delete them first.
    let said = refused(
        &core,
        "card_update",
        json!({"branch": "_project_board_", "target": "Goals:Ship", "value": {"kind": "task"}}),
    );
    assert!(
        said.contains("を持っているので task にできません"),
        "{said}"
    );
}
