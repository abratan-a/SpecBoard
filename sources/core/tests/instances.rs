//! Data cards (`instance`; `specboard://SpecBoard/Docs/Design:CardKinds`「データ（instance）と定義のセクション」):
//! the values of an entity or a value, its shape the line `of` goes to — made in a data card's section
//! with `of` and `data`, held to the shape's fields as they are written, a value at a time by its
//! path, following the shape's renames, kept when its field goes, its shape's examples when the shape
//! is written out, and a table in a document.

use crate::common;

use common::{checked, ok, op, refused, script_store};
use serde_json::{Value, json};
use specboard_core::Core;

const PROJECT: &str = "見本";

const ID: &str = "5f0c1a2b-0000-4000-8000-00000000abcd";

fn said(answer: &Value) -> String {
    answer["warnings"].to_string()
}

/// The sample project with a data card's section `Data` under main's root.
fn with_data() -> Core {
    let core = Core::open(script_store()).unwrap();
    let read = ok(&core, "project_read", json!({}));
    let mut tree = read["projectTree"].clone();
    tree["main_sections"]
        .as_array_mut()
        .unwrap()
        .push(json!({"type": "definition", "name": "Data"}));
    ok(
        &core,
        "project_update",
        json!({"value": {"project_tree": tree}}),
    );
    core
}

/// An order written whole: every required field, a line, the fixed kind.
fn order() -> Value {
    json!({"id": ID, "total": "12.50", "status": "paid",
        "lines": [{"sku": "A-1", "qty": 2}], "kind": "order"})
}

fn first(core: &Core) -> Value {
    ok(
        core,
        "card_create",
        json!({"target": "Data:first", "value": {"kind": "instance", "label": "最初の注文",
            "of": "Schemas/Ordering:Order", "data": order()}}),
    )
}

#[test]
fn a_data_card_is_made_with_its_shape_in_its_section() {
    let core = with_data();
    // Made half-written: what its shape requires and it lacks is said, not refused.
    let made = ok(
        &core,
        "card_create",
        json!({"target": "Data:draft", "value": {"kind": "instance", "of": "Schemas/Ordering:Order",
            "data": {"status": "open"}}}),
    );
    let warnings = said(&made);
    for wanted in ["Data:draft.id が要ります", "Data:draft.total が要ります"] {
        assert!(warnings.contains(wanted), "{made}");
    }
    let problems = checked(&core, "main")["cardProblems"].to_string();
    assert!(
        problems.contains("Data:draft.id") && problems.contains("形 Order では必須"),
        "{problems}"
    );
    // Its shape is needed, an entity or a value; and only a data card has one.
    let no_shape = refused(
        &core,
        "card_create",
        json!({"target": "Data:none", "value": {"kind": "instance"}}),
    );
    assert!(no_shape.contains("of が要ります"), "{no_shape}");
    let not_a_shape = refused(
        &core,
        "card_create",
        json!({"target": "Data:color", "value": {"kind": "instance", "of": "Schemas:Status"}}),
    );
    assert!(
        not_a_shape.contains("Schemas:Status は enum"),
        "{not_a_shape}"
    );
    let not_data = refused(
        &core,
        "card_create",
        json!({"target": "Schemas:Odd", "value": {"kind": "entity", "of": "Schemas:Customer"}}),
    );
    assert!(
        not_data.contains("of はデータ（instance）と表（table）にだけ書けます"),
        "{not_data}"
    );
    // Its section holds the shapes beside their data cards, and notes, but no flow; a free
    // section holds data cards too.
    ok(
        &core,
        "card_create",
        json!({"target": "Data:Shape", "value": {"kind": "entity"}}),
    );
    let flow_there = refused(
        &core,
        "card_create",
        json!({"target": "Data:Steps", "value": {"kind": "flow"}}),
    );
    assert!(flow_there.contains("flow は置けません"), "{flow_there}");
    ok(
        &core,
        "card_create",
        json!({"target": "Data:Memo", "value": {"kind": "note", "body": "例"}}),
    );
    ok(
        &core,
        "card_create",
        json!({"target": "Free:loose", "value": {"kind": "instance", "of": "Schemas:Customer"}}),
    );
    let in_schemas = refused(
        &core,
        "card_create",
        json!({"target": "Schemas:inside", "value": {"kind": "instance", "of": "Schemas:Customer"}}),
    );
    assert!(in_schemas.contains("instance は置けません"), "{in_schemas}");
    // Gathered by its kind.
    let listed = ok(&core, "card_list", json!({"value": {"kind": "instance"}}));
    let mut paths: Vec<&str> = listed["cards"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["path"].as_str().unwrap())
        .collect();
    paths.sort();
    assert_eq!(paths, ["Data:draft", "Free:loose"], "{listed}");
}

#[test]
fn a_value_its_field_does_not_take_is_refused_by_its_path() {
    let core = with_data();
    first(&core);
    // Every misfit at once, each by its path; nothing written.
    let wrong = refused(
        &core,
        "card_update",
        json!({"target": "Data:first", "value": {"data": {"id": 12, "total": 12.5,
            "lines": [{"sku": 1, "extra": true}], "kind": "other", "shipping": "x",
            "customer_id": null, "extra": 1}}}),
    );
    for wanted in [
        "Data:first.id: uuid の文字ではありません",
        "Data:first.total: 10 進の数の文字",
        "Data:first.lines[0].sku: 文字ではありません",
        "Data:first.lines[0].extra: 形にこのフィールドがありません",
        "Data:first.kind: 固定の値 \"order\" ではありません",
        "Data:first.shipping: オブジェクトではありません",
        "Data:first.customer_id: null は書けません",
        "Data:first.extra: 形 Order にこのフィールドがありません",
    ] {
        assert!(wrong.contains(wanted), "wanted「{wanted}」: {wrong}");
    }
    let read = ok(&core, "card_read", json!({"target": "Data:first"}));
    assert_eq!(read["data"], order());
    // An enum that is not open takes its values only; an open one any text.
    ok(
        &core,
        "card_create",
        json!({"target": "Schemas:Color", "value": {"kind": "enum", "values": [{"name": "red"}, {"name": "blue"}]}}),
    );
    ok(
        &core,
        "card_create",
        json!({"target": "Schemas:Paint", "value": {"kind": "value",
            "fields": [{"name": "color", "type": "string"}, {"name": "at", "type": "datetime", "nullable": true}],
            "connections": [{"line": "embed", "from": "color", "to": "Schemas:Color"}]}}),
    );
    let not_one = refused(
        &core,
        "card_create",
        json!({"target": "Data:paint", "value": {"kind": "instance", "of": "Schemas:Paint",
            "data": {"color": "green", "at": "2026-10-10 09:30"}}}),
    );
    assert!(
        not_one.contains("Data:paint.color: Color の値ではありません（red・blue）"),
        "{not_one}"
    );
    assert!(not_one.contains("Data:paint.at: 日時の文字"), "{not_one}");
    ok(
        &core,
        "card_create",
        json!({"target": "Data:paint", "value": {"kind": "instance", "of": "Schemas:Paint",
            "data": {"color": "red", "at": null}}}),
    );
    ok(
        &core,
        "card_update",
        json!({"target": "Data:first.status", "value": {"value": "anything"}}),
    );
    // Its data is an object.
    let flat = refused(
        &core,
        "card_update",
        json!({"target": "Data:first", "value": {"data": [1]}}),
    );
    assert!(flat.contains("data は object で指定してください"), "{flat}");
}

#[test]
fn a_data_card_is_read_and_written_a_value_at_a_time() {
    let core = with_data();
    first(&core);
    // Its shape and values; the line `of` is not among its lines.
    let read = ok(&core, "card_read", json!({"target": "Data:first"}));
    assert_eq!(read["kind"], "instance");
    assert_eq!(read["of"], "Schemas/Ordering:Order");
    assert_eq!(read["data"], order());
    assert!(read.get("connections").is_none(), "{read}");
    // A value by its path, with its field's type.
    let status = ok(&core, "card_read", json!({"target": "Data:first.status"}));
    assert_eq!(
        (&status["type"], &status["value"]),
        (&json!("Status"), &json!("paid"))
    );
    let lines = ok(&core, "card_read", json!({"target": "Data:first.lines"}));
    assert_eq!(lines["type"], "Line[]");
    assert_eq!(lines["value"], json!([{"sku": "A-1", "qty": 2}]));
    let postal = ok(
        &core,
        "card_read",
        json!({"target": "Data:first.shipping.postal_code"}),
    );
    assert!(postal.get("value").is_none(), "{postal}");
    // Nothing inside an array is named; nor a name the shape does not have.
    let inside = refused(
        &core,
        "card_read",
        json!({"target": "Data:first.lines.sku"}),
    );
    assert!(inside.contains("並び（配列）"), "{inside}");
    let unknown = refused(&core, "card_read", json!({"target": "Data:first.nope"}));
    assert!(
        unknown.contains("Data:first の下にあるフィールドは: id"),
        "{unknown}"
    );

    // One value written; one inside an object made on the way; one taken away.
    let written = ok(
        &core,
        "card_update",
        json!({"target": "Data:first.status", "value": {"value": "shipped"}}),
    );
    assert_eq!(written["value"], "shipped");
    ok(
        &core,
        "card_update",
        json!({"target": "Data:first.shipping.postal_code", "value": {"value": "100-0001"}}),
    );
    let wrong = refused(
        &core,
        "card_update",
        json!({"target": "Data:first.lines", "value": {"value": "A-1"}}),
    );
    assert!(
        wrong.contains("Data:first.lines: 並び（配列）ではありません"),
        "{wrong}"
    );
    ok(&core, "card_delete", json!({"target": "Data:first.total"}));
    let gone = refused(&core, "card_delete", json!({"target": "Data:first.total"}));
    assert!(gone.contains("Data:first.total に値はありません"), "{gone}");
    let read = ok(&core, "card_read", json!({"target": "Data:first"}));
    assert_eq!(read["data"]["status"], "shipped");
    assert_eq!(read["data"]["shipping"], json!({"postal_code": "100-0001"}));
    assert!(read["data"].get("total").is_none(), "{read}");
    // Nothing is made inside a data card.
    let made = refused(
        &core,
        "card_create",
        json!({"target": "Data:first.total", "value": {"value": "1"}}),
    );
    assert!(made.contains("中の値は作るものではありません"), "{made}");

    // Its shape is its key `of`, not one of its lines; replaced by it.
    let line = refused(
        &core,
        "card_update",
        json!({"target": "Data:first", "value": {"connections": {"add": [{"line": "shapeOf", "to": "Schemas:Customer"}]}}}),
    );
    assert!(line.contains("value のキー of"), "{line}");
    let moved = ok(
        &core,
        "card_update",
        json!({"target": "Data:first", "value": {"of": "Schemas:Customer"}}),
    );
    // Its values the new shape has not are kept, said apart.
    assert_eq!(moved["of"], "Schemas:Customer", "{moved}");
    let read = ok(&core, "card_read", json!({"target": "Data:first"}));
    assert_eq!(read["data"], json!({"id": ID}));
    assert_eq!(read["strayFields"]["status"], "shipped");
    // The guide says how a data card is written.
    let guide = ok(&core, "guide", json!({"about": "instance"}));
    let text = guide.to_string();
    assert!(
        text.contains("\"of\"") && text.contains("\"data\""),
        "{guide}"
    );
}

#[test]
fn a_data_card_follows_its_shapes_renames_and_keeps_what_it_loses() {
    let core = with_data();
    first(&core);
    ok(
        &core,
        "card_create",
        json!({"target": "Docs:About", "value": {"kind": "freedoc", "body": "合計は {{Data:first.total}}、[[Data:first.lines]]"}}),
    );
    // A field renamed: the key of its value, inside an array of an embedded shape too, and the
    // paths that name it.
    ok(
        &core,
        "card_update",
        json!({"target": "Schemas/Ordering:Order.total", "value": {"name": "amount"}}),
    );
    ok(
        &core,
        "card_update",
        json!({"target": "Schemas/Ordering:Line.sku", "value": {"name": "code"}}),
    );
    let read = ok(&core, "card_read", json!({"target": "Data:first"}));
    assert_eq!(read["data"]["amount"], "12.50");
    assert!(read["data"].get("total").is_none(), "{read}");
    assert_eq!(read["data"]["lines"], json!([{"code": "A-1", "qty": 2}]));
    let about = ok(&core, "card_read", json!({"target": "Docs:About"}));
    assert_eq!(
        about["body"],
        "合計は {{Data:first.amount}}、[[Data:first.lines]]"
    );
    // Its shape renamed: `of` follows.
    ok(
        &core,
        "card_update",
        json!({"target": "Schemas/Ordering:Order", "value": {"name": "Purchase"}}),
    );
    let read = ok(&core, "card_read", json!({"target": "Data:first"}));
    assert_eq!(read["of"], "Schemas/Ordering:Purchase");

    // A field let go of: its value is kept apart, until it is let go of too.
    ok(
        &core,
        "card_delete",
        json!({"target": "Schemas/Ordering:Purchase.amount"}),
    );
    let read = ok(&core, "card_read", json!({"target": "Data:first"}));
    assert_eq!(read["strayFields"], json!({"amount": "12.50"}));
    assert!(read["data"].get("amount").is_none(), "{read}");
    let strays = checked(&core, "main")["strayFields"].clone();
    assert_eq!(strays.as_array().unwrap().len(), 1, "{strays}");
    assert_eq!(strays[0]["at"], "Data:first", "{strays}");
    assert_eq!(strays[0]["fields"], json!(["amount"]), "{strays}");
    // Written whole, its strays stay.
    let mut data = order();
    data.as_object_mut().unwrap().remove("total");
    data["lines"] = json!([{"code": "B-2"}]);
    ok(
        &core,
        "card_update",
        json!({"target": "Data:first", "value": {"data": data}}),
    );
    let read = ok(&core, "card_read", json!({"target": "Data:first"}));
    assert_eq!(read["strayFields"], json!({"amount": "12.50"}));
    let unknown = refused(
        &core,
        "card_update",
        json!({"target": "Data:first", "value": {"strayFields": {"nope": null}}}),
    );
    assert!(unknown.contains("あるのは amount"), "{unknown}");
    ok(
        &core,
        "card_update",
        json!({"target": "Data:first", "value": {"strayFields": null}}),
    );
    let read = ok(&core, "card_read", json!({"target": "Data:first"}));
    assert!(read.get("strayFields").is_none(), "{read}");

    // Its shape let go of: said where it was, and that it has none.
    ok(
        &core,
        "card_create",
        json!({"target": "Data:alice", "value": {"kind": "instance", "of": "Schemas:Customer",
            "data": {"id": ID, "name": "Alice"}}}),
    );
    ok(&core, "card_delete", json!({"target": "Schemas:Customer"}));
    let after = checked(&core, "main");
    let broken = after["brokenPaths"].to_string();
    assert!(
        broken.contains("Data:alice") && broken.contains("card_update の of で付け替えます"),
        "{broken}"
    );
    let problems = after["cardProblems"].to_string();
    assert!(
        problems.contains("Data:alice") && problems.contains("形（of）がありません"),
        "{problems}"
    );
}

#[test]
fn its_shape_is_written_out_with_it_as_examples_and_a_document_shows_its_values() {
    let core = with_data();
    first(&core);
    ok(
        &core,
        "card_create",
        json!({"target": "Free:another", "value": {"kind": "instance", "of": "Schemas/Ordering:Order",
            "data": {"id": ID, "total": "1.00", "kind": "order"}}}),
    );
    let written = ok(
        &core,
        "export_read",
        json!({"targetdir": "[default]", "target": "schema", "value": {"files": ["types.schema.json"]}}),
    );
    let text = written["files"][0]["text"].as_str().unwrap();
    let schema: Value = serde_json::from_str(text).unwrap();
    let order_def = &schema["$defs"]["Order"];
    let keys: Vec<&String> = order_def.as_object().unwrap().keys().collect();
    assert_eq!(keys.last().map(|k| k.as_str()), Some("examples"));
    assert_eq!(
        order_def["examples"],
        json!([{"id": ID, "total": "1.00", "kind": "order"}, order()])
    );
    // Not a type itself.
    assert!(schema["$defs"].get("first").is_none(), "{schema}");
    // One the check finds wanting — a required field with no value, a value its field does not
    // take — is no example of it.
    ok(
        &core,
        "card_create",
        json!({"target": "Data:draft", "value": {"kind": "instance",
            "of": "Schemas/Ordering:Order", "data": {"status": "open"}}}),
    );
    ok(
        &core,
        "card_create",
        json!({"target": "Schemas:Tag", "value": {"kind": "entity", "fields": [
            {"name": "name", "type": "string"}, {"name": "size", "type": "string"}]}}),
    );
    for (name, data) in [
        ("plain", json!({"name": "a"})),
        ("odd", json!({"name": "b", "size": "x"})),
    ] {
        ok(
            &core,
            "card_create",
            json!({"target": format!("Data:{name}"), "value": {"kind": "instance",
                "of": "Schemas:Tag", "data": data}}),
        );
    }
    ok(
        &core,
        "card_update",
        json!({"target": "Schemas:Tag.size", "value": {"type": "int"}}),
    );
    let problems = checked(&core, "main")["cardProblems"].to_string();
    assert!(
        problems.contains("Data:draft") && problems.contains("Data:odd"),
        "{problems}"
    );
    let again = ok(
        &core,
        "export_read",
        json!({"targetdir": "[default]", "target": "schema", "value": {"files": ["types.schema.json"]}}),
    );
    let text = again["files"][0]["text"].as_str().unwrap();
    let schema: Value = serde_json::from_str(text).unwrap();
    assert_eq!(
        schema["$defs"]["Order"]["examples"],
        json!([{"id": ID, "total": "1.00", "kind": "order"}, order()])
    );
    assert_eq!(schema["$defs"]["Tag"]["examples"], json!([{"name": "a"}]));
    for name in ["Data:draft", "Data:plain", "Data:odd", "Schemas:Tag"] {
        ok(&core, "card_delete", json!({"target": name}));
    }
    let ts = ok(
        &core,
        "export_read",
        json!({"targetdir": "[default]", "target": "ts", "value": {"files": ["types.ts"]}}),
    );
    assert!(
        !ts["files"][0]["text"].as_str().unwrap().contains("first"),
        "{ts}"
    );

    // A document's card row: its fields, their types and its values.
    ok(
        &core,
        "card_create",
        json!({"target": "Docs:Spec.cards.first", "value": {"kind": "card", "target": "Data:first"}}),
    );
    let markdown = ok(
        &core,
        "card_read",
        json!({"target": "Docs:Spec", "value": {"as": "markdown"}}),
    );
    let text = markdown["markdown"].as_str().unwrap();
    for wanted in [
        "| 名前 | 型 | 値 |",
        &format!("| `id` | uuid | `\"{ID}\"` |"),
        "| `status` | Status | `\"paid\"` |",
        // An array, element by element.
        "| `lines` | Line[] |  |",
        "| `lines[0]` | Line |  |",
        "| `lines[0].sku` | string | `\"A-1\"` |",
        "| `lines[0].qty` | int | `2` |",
        "| `shipping` | object |  |",
        "| `shipping.postal_code` | string |  |",
    ] {
        assert!(text.contains(wanted), "wanted「{wanted}」: {text}");
    }
}

#[test]
fn a_data_cards_value_draws_a_line_to_another_card() {
    let core = with_data();
    first(&core);
    ok(
        &core,
        "card_create",
        json!({"target": "Data:alice", "value": {"kind": "instance", "of": "Schemas:Customer",
            "data": {"id": ID, "name": "Alice"}}}),
    );
    ok(
        &core,
        "card_update",
        json!({"target": "Data:first.customer_id", "value": {"value": ID}}),
    );
    ok(
        &core,
        "card_update",
        json!({"target": "Data:first", "value": {"connections": {"add": [{"line": "link", "from": "customer_id", "to": "Data:alice"}]}}}),
    );
    let read = ok(&core, "card_read", json!({"target": "Data:first"}));
    assert_eq!(
        read["connections"],
        json!([{"line": "link", "from": "customer_id", "to": "Data:alice"}])
    );
    assert_eq!(read["of"], "Schemas/Ordering:Order");
    // Pointing by id is a shape's line, not a data card's.
    let points = refused(
        &core,
        "card_update",
        json!({"target": "Data:first", "value": {"connections": {"add": [{"line": "points", "from": "customer_id", "to": "Data:alice"}]}}}),
    );
    assert!(
        points.contains("この線は entity・valueの欄 から出せます（instance の"),
        "{points}"
    );
}

/// An order with three lines, two of the same sku.
fn three_lines(core: &Core) {
    first(core);
    ok(
        core,
        "card_update",
        json!({"target": "Data:first.lines", "value": {"value": [
            {"sku": "A-1", "qty": 2}, {"sku": "B-2", "qty": 1}, {"sku": "B-2", "qty": 5}]}}),
    );
}

#[test]
fn an_arrays_element_is_picked_by_its_values_or_its_place() {
    let core = with_data();
    three_lines(&core);
    // By its fields' values, and by its place: the element and its own type.
    let one = ok(
        &core,
        "card_read",
        json!({"target": "Data:first.lines[sku=A-1]"}),
    );
    assert_eq!(
        (&one["type"], &one["value"]),
        (&json!("Line"), &json!({"sku": "A-1", "qty": 2}))
    );
    let same = ok(&core, "card_read", json!({"target": "Data:first.lines[0]"}));
    assert_eq!(same["value"], one["value"]);
    let qty = ok(
        &core,
        "card_read",
        json!({"target": "Data:first.lines[sku=B-2,qty=5].qty"}),
    );
    assert_eq!((&qty["type"], &qty["value"]), (&json!("int"), &json!(5)));
    // Not one element, or a pick its elements are not picked by: refused, and why.
    for (target, why) in [
        (
            "Data:first.lines[sku=B-2]",
            "Data:first.lines[sku=B-2]: 2 つに当たります（lines[1]・lines[2]）",
        ),
        (
            "Data:first.lines[sku=Z-9]",
            "Data:first.lines[sku=Z-9]: 当たる要素がありません",
        ),
        (
            "Data:first.lines[3]",
            "Data:first.lines[3]: lines は 3 個です",
        ),
    ] {
        assert_eq!(refused(&core, "card_read", json!({"target": target})), why);
    }
    for (target, why) in [
        (
            "Data:first.lines[skuu=A-1]",
            "lines の要素に欄「skuu」はありません（欄: sku・qty）",
        ),
        (
            "Data:first.lines[=A-1]",
            "lines の要素は欄（sku・qty）を持つので",
        ),
        ("Data:first.status[0]", "status は並び（配列）ではない"),
        ("Data:first.lines[sku]", "欄の値の条件 [欄=値,…]"),
    ] {
        let said = refused(&core, "card_read", json!({"target": target}));
        assert!(said.contains(why), "{target}: {said}");
    }

    // A value inside one written; one of a pick's fields written: answered at the path it is at now.
    let written = ok(
        &core,
        "card_update",
        json!({"target": "Data:first.lines[sku=A-1].qty", "value": {"value": 3}}),
    );
    assert_eq!(written["value"], 3);
    let renamed = ok(
        &core,
        "card_update",
        json!({"target": "Data:first.lines[sku=A-1].sku", "value": {"value": "A-9"}}),
    );
    assert_eq!(renamed["path"], "Data:first.lines[sku=A-9].sku");
    let wrong = refused(
        &core,
        "card_update",
        json!({"target": "Data:first.lines[0].qty", "value": {"value": "x"}}),
    );
    assert!(
        wrong.contains("Data:first.lines[0].qty: 整数ではありません"),
        "{wrong}"
    );
    // An element taken out of its array; one not there is not.
    ok(
        &core,
        "card_delete",
        json!({"target": "Data:first.lines[1]"}),
    );
    let read = ok(&core, "card_read", json!({"target": "Data:first.lines"}));
    assert_eq!(
        read["value"],
        json!([{"sku": "A-9", "qty": 3}, {"sku": "B-2", "qty": 5}])
    );
    let gone = refused(
        &core,
        "card_delete",
        json!({"target": "Data:first.lines[sku=Z-9]"}),
    );
    assert!(gone.contains("当たる要素がありません"), "{gone}");
}

#[test]
fn a_line_from_an_element_goes_with_it() {
    let core = with_data();
    first(&core);
    ok(
        &core,
        "card_update",
        json!({"target": "Data:first.lines", "value": {"value": [
            {"sku": "A-1", "qty": 2}, {"sku": "B-2", "qty": 1}, {"sku": "C-3", "qty": 4}]}}),
    );
    ok(
        &core,
        "card_create",
        json!({"target": "Data:alice", "value": {"kind": "instance", "of": "Schemas:Customer",
            "data": {"id": ID, "name": "Alice"}}}),
    );
    // From an element by its values and by its place; onto one from another card.
    ok(
        &core,
        "card_update",
        json!({"target": "Data:first", "value": {"connections": {"add": [
            {"line": "link", "from": "lines[sku=B-2]", "to": "Data:alice"},
            {"line": "link", "from": "lines[2].qty", "to": "Data:alice"}]}}}),
    );
    ok(
        &core,
        "card_update",
        json!({"target": "Data:alice", "value": {"connections": {"add": [
            {"line": "link", "from": "name", "to": "Data:first", "at": "lines[sku=A-1]"}]}}}),
    );
    // What the data cards' lines break (the sample breaks some of its own).
    let broken = |core: &Core| -> Vec<Value> {
        checked(core, "main")["brokenPaths"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|b| b["at"].as_str().is_some_and(|a| a.starts_with("Data:")))
            .cloned()
            .collect()
    };
    assert_eq!(broken(&core), Vec::<Value>::new());
    let from = |core: &Core, card: &str| -> Vec<String> {
        ok(core, "card_read", json!({"target": card}))["connections"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| {
                format!(
                    "{}>{}",
                    l["from"].as_str().unwrap_or(""),
                    l["at"].as_str().unwrap_or("")
                )
            })
            .collect()
    };
    // A field of a pick written over: the lines picking it by that field follow.
    let said = ok(
        &core,
        "card_update",
        json!({"target": "Data:first.lines[sku=B-2].sku", "value": {"value": "B-3"}}),
    );
    assert!(
        said["warnings"]
            .to_string()
            .contains("Data:first の線の from を lines[sku=B-3] に書き直しました"),
        "{said}"
    );
    assert_eq!(
        from(&core, "Data:first"),
        ["lines[sku=B-3]>", "lines[2].qty>"]
    );
    ok(
        &core,
        "card_update",
        json!({"target": "Data:first.lines[0].sku", "value": {"value": "A-2"}}),
    );
    assert_eq!(from(&core, "Data:alice"), ["name>lines[sku=A-2]"]);
    // Written whole in another order: by values, the same element; by place, the place.
    ok(
        &core,
        "card_update",
        json!({"target": "Data:first.lines", "value": {"value": [
            {"sku": "C-3", "qty": 4}, {"sku": "A-2", "qty": 2}, {"sku": "B-3", "qty": 1}]}}),
    );
    assert_eq!(broken(&core), Vec::<Value>::new());
    assert_eq!(
        from(&core, "Data:first"),
        ["lines[sku=B-3]>", "lines[2].qty>"]
    );
    // The window draws each from the row of the element it picks now, by its place.
    let project = ok(&core, "project_read", json!({}))["id"]
        .as_str()
        .map(str::to_string)
        .unwrap_or_else(|| {
            let conn = rusqlite::Connection::open(core.place()).unwrap();
            conn.query_row("SELECT id FROM projects WHERE name = ?", [PROJECT], |r| {
                r.get(0)
            })
            .unwrap()
        });
    let detail = common::op(&core, json!({"op": "project_detail", "project": project}));
    let drawn: Vec<String> = detail["result"]["relations"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["line"] == "link")
        .map(|r| {
            format!(
                "{}>{}",
                r["from"].as_str().unwrap_or(""),
                r["at"].as_str().unwrap_or("")
            )
        })
        .collect();
    assert!(
        drawn.contains(&"lines[2]>".to_string())
            && drawn.contains(&"lines[2].qty>".to_string())
            && drawn.contains(&"name>lines[1]".to_string()),
        "{drawn:?}"
    );
    // An element taken out: the places after it move one back, and a pick of it is said broken.
    ok(
        &core,
        "card_delete",
        json!({"target": "Data:first.lines[1]"}),
    );
    assert_eq!(
        from(&core, "Data:first"),
        ["lines[sku=B-3]>", "lines[1].qty>"]
    );
    let said = json!(broken(&core)).to_string();
    assert!(
        said.contains("Data:first.lines[sku=A-2]") && said.contains("当たる要素がありません"),
        "{said}"
    );
    // A field a pick names renamed: the picks follow.
    ok(
        &core,
        "card_update",
        json!({"target": "Schemas/Ordering:Line.sku", "value": {"name": "code"}}),
    );
    assert_eq!(
        from(&core, "Data:first"),
        ["lines[code=B-3]>", "lines[1].qty>"]
    );
    assert_eq!(from(&core, "Data:alice"), ["name>lines[code=A-2]"]);
    // A pick of a field its elements have not: refused. Of none there yet: written, and said.
    let refused_line = refused(
        &core,
        "card_update",
        json!({"target": "Data:first", "value": {"connections": {"add": [
            {"line": "link", "from": "lines[sku=B-3]", "to": "Data:alice"}]}}}),
    );
    assert!(
        refused_line.contains("出元の「lines[sku=B-3]」は Data:first にありません"),
        "{refused_line}"
    );
    let pending = ok(
        &core,
        "card_update",
        json!({"target": "Data:first", "value": {"connections": {"add": [
            {"line": "link", "from": "lines[code=Z-9]", "to": "Data:alice"}]}}}),
    );
    assert!(
        pending["warnings"]
            .to_string()
            .contains("当たる要素がありません"),
        "{pending}"
    );
}

#[test]
fn a_field_takes_the_other_types_it_names_and_writes_them_out_so() {
    let core = with_data();
    // `also`: "one or a list" and the like, as the field's own attribute.
    ok(
        &core,
        "card_create",
        json!({"target": "Schemas:Call", "value": {"kind": "entity", "fields": [
            {"name": "command", "type": "string", "also": ["array"]},
            {"name": "limit", "type": "int", "also": ["string"]},
            {"name": "env", "type": "string", "also": ["object"]}]}}),
    );
    let made = |data: Value| -> Value {
        ok(
            &core,
            "card_create",
            json!({"target": "Data:call", "value": {"kind": "instance", "of": "Schemas:Call", "data": data}}),
        )
    };
    let unmade = |core: &Core| {
        ok(core, "card_delete", json!({"target": "Data:call"}));
    };
    // Its own type, a list of its own type's elements, and the others it names.
    let good = made(json!({"command": ["ls", "-l"], "limit": "10", "env": {"LANG": "C"}}));
    assert!(good.get("warnings").is_none(), "{good}");
    unmade(&core);
    let good = made(json!({"command": "ls", "limit": 10}));
    assert!(good.get("warnings").is_none(), "{good}");
    unmade(&core);
    // What none of them is, and a list's element that is not the type.
    let bad = refused(
        &core,
        "card_create",
        json!({"target": "Data:call", "value": {"kind": "instance", "of": "Schemas:Call",
            "data": {"command": ["ls", 3], "limit": true, "env": 4}}}),
    );
    for wanted in [
        "Data:call.command[1]: 文字ではありません",
        "Data:call.limit: 整数ではありません",
        "Data:call.env: 文字ではありません",
    ] {
        assert!(bad.contains(wanted), "wanted「{wanted}」: {bad}");
    }

    // Written out as a JSON Schema: a type list, and its items.
    let written = ok(
        &core,
        "export_read",
        json!({"targetdir": "[default]", "target": "schema", "value": {"files": ["types.schema.json"]}}),
    );
    let text = written["files"][0]["text"].as_str().unwrap();
    let schema: Value = serde_json::from_str(text).unwrap();
    let properties = &schema["$defs"]["Call"]["properties"];
    assert_eq!(properties["command"]["type"], json!(["string", "array"]));
    assert_eq!(properties["command"]["items"], json!({"type": "string"}));
    assert_eq!(properties["limit"]["type"], json!(["string", "integer"]));
    assert_eq!(properties["env"]["type"], json!(["string", "object"]));

    // TypeScript and Zod: a union, its own type first.
    let ts = ok(
        &core,
        "export_read",
        json!({"targetdir": "[default]", "target": "ts", "value": {"files": ["types.ts"]}}),
    );
    let ts = ts["files"][0]["text"].as_str().unwrap();
    for wanted in [
        "command?: string | string[];",
        "limit?: number | string;",
        "env?: string | Record<string, unknown>;",
    ] {
        assert!(ts.contains(wanted), "wanted「{wanted}」: {ts}");
    }
    let zod = ok(
        &core,
        "export_read",
        json!({"targetdir": "[default]", "target": "zod", "value": {"files": ["types.zod.ts"]}}),
    );
    let zod = zod["files"][0]["text"].as_str().unwrap();
    for wanted in [
        "command: z.union([z.string(), z.array(z.string())]).optional(),",
        "limit: z.union([z.int(), z.string()]).optional(),",
        "env: z.union([z.string(), z.record(z.string(), z.unknown())]).optional(),",
    ] {
        assert!(zod.contains(wanted), "wanted「{wanted}」: {zod}");
    }
}

#[test]
fn a_line_written_as_a_schema_puts_its_form_into_the_data_json() {
    let core = with_data();
    // `export: {as: schema}`: only from a data card's field or element, to a form's head.
    let checked = ok(
        &core,
        "kind_create",
        json!({"target": "wrong", "value": {"of": "line", "base": "link",
            "from": [{"kinds": ["*"], "at": "card"}], "to": {"kinds": ["*"], "at": ["card"]},
            "export": {"as": "schema"}, "dryRun": true}}),
    );
    let problems = checked["problems"].to_string();
    for wanted in [
        "export の as が schema の線は、データ（instance）の欄・要素",
        "export の as が schema の線は、形（to の kinds が entity・value",
    ] {
        assert!(problems.contains(wanted), "wanted「{wanted}」: {checked}");
    }
    for (name, label, max, export) in [
        (
            "schemaOf",
            "形の JSON Schema",
            Some(1),
            json!({"as": "schema"}),
        ),
        (
            "partOf",
            "要素の形",
            None,
            json!({"as": "schema", "key": "spec"}),
        ),
    ] {
        ok(
            &core,
            "kind_create",
            json!({"target": name, "value": {"of": "line", "label": label, "base": "link",
                "from": [{"kinds": ["instance"], "at": "row"}],
                "to": {"kinds": ["entity", "value"], "at": ["card"]},
                "max": max, "visible": false, "export": export}}),
        );
    }

    // A command, its arguments' form, and the form each of its parts has.
    ok(
        &core,
        "card_create",
        json!({"value": {
            "Schemas:CmdArgs": {"kind": "entity", "fields": [
                {"name": "path", "type": "string", "required": true},
                {"name": "depth", "type": "int", "also": ["string"]}]},
            "Schemas:CmdPart": {"kind": "value", "fields": [
                {"name": "label", "type": "string", "required": true}]},
            "Schemas:CmdBody": {"kind": "value", "description": "本体", "fields": [
                {"name": "mode", "type": "string"}]},
            "Schemas:Cmd": {"kind": "entity", "fields": [
                {"name": "name", "type": "string", "required": true},
                {"name": "args", "type": "object"},
                {"name": "parts", "type": "string", "array": true}],
             "connections": [{"line": "embed", "from": "parts", "to": "Schemas:CmdPart"}]}}}),
    );
    ok(
        &core,
        "card_create",
        json!({"target": "Data:ls", "value": {"kind": "instance", "of": "Schemas:Cmd",
            "data": {"name": "ls", "parts": [{"label": "head"}, {"label": "body"}]},
            "connections": [
                {"line": "schemaOf", "from": "args", "to": "Schemas:CmdArgs"},
                {"line": "partOf", "from": "parts[label=head]", "to": "Schemas:CmdArgs"},
                {"line": "partOf", "from": "parts[label=body]", "to": "Schemas:CmdBody"}]}}),
    );
    // Made as the window makes it (an export's creation is asked for through MCP).
    let made = common::op(
        &core,
        json!({"op": "tool", "name": "export_create", "arguments": {"project": PROJECT,
            "target": "cmds", "value": {"entries": [
                {"from": "Data", "to": "cmds.json", "format": "json-data"}]}}}),
    );
    assert!(made.get("error").is_none(), "{made}");
    let read = |core: &Core| -> Value {
        let written = ok(
            core,
            "export_read",
            json!({"targetdir": "[default]", "target": "cmds", "value": {"files": ["cmds.json"]}}),
        );
        serde_json::from_str(written["files"][0]["text"].as_str().unwrap()).unwrap()
    };
    let args = json!({"type": "object",
        "properties": {"path": {"type": "string"}, "depth": {"type": ["string", "integer"]}},
        "required": ["path"]});
    let got = read(&core);
    assert_eq!(
        got,
        json!([{"name": "ls",
            "parts": [{"label": "head", "spec": args},
                {"label": "body", "spec": {"type": "object", "description": "本体",
                    "properties": {"mode": {"type": "string"}}}}],
            "args": args}]),
        "{got}"
    );
}

#[test]
fn a_data_card_and_a_table_are_as_tall_as_the_screen_draws_them() {
    // A data card a row a field, nested as its shape — an array's elements each a row with their
    // fields under it — and a table the whole card, as a document is; a card placed after them
    // does not overlap them.
    let core = with_data();
    ok(
        &core,
        "card_create",
        json!({"target": "Schemas:Host", "value": {"kind": "entity", "fields": [
            {"name": "name", "type": "string", "required": true},
            {"name": "addr", "type": "object"},
            {"name": "addr.ip", "type": "string"},
            {"name": "addr.port", "type": "int"},
            {"name": "ports", "type": "object", "array": true},
            {"name": "ports.number", "type": "int"},
            {"name": "tags", "type": "string", "array": true}]}}),
    );
    let height = |path: &str| -> i64 {
        let read = ok(&core, "card_read", json!({"target": path}));
        read["bottomRight"]["y"].as_i64().unwrap() - read["topLeft"]["y"].as_i64().unwrap() + 1
    };
    // name, addr, addr.ip, addr.port, ports, ports[0], ports[0].number, ports[1],
    // ports[1].number, tags, tags[0], tags[1]: twelve rows.
    ok(
        &core,
        "card_create",
        json!({"target": "Data:web", "value": {"kind": "instance", "of": "Schemas:Host",
            "data": {"name": "web", "addr": {"ip": "192.0.2.1", "port": 80},
                "ports": [{"number": 80}, {"number": 443}], "tags": ["a", "b"]}}}),
    );
    // As the screen draws twelve rows: the frame and a row each, 27 points a unit.
    assert_eq!(
        height("Data:web"),
        ((2 + 40 + 12 + 12 * 27) as f64 / 27.0).ceil() as i64
    );
    // What a field embeds unfolds as an object's does: an array of a value's elements.
    ok(
        &core,
        "card_create",
        json!({"value": {
            "Schemas:Port": {"kind": "value", "fields": [
                {"name": "number", "type": "int"}, {"name": "protocol", "type": "string"}]},
            "Schemas:Box": {"kind": "entity", "fields": [
                {"name": "name", "type": "string"}, {"name": "ports", "type": "string", "array": true}],
                "connections": [{"line": "embed", "from": "ports", "to": "Schemas:Port"}]}
        }}),
    );
    ok(
        &core,
        "card_create",
        json!({"target": "Data:vault", "value": {"kind": "instance", "of": "Schemas:Box",
            "data": {"name": "vault", "ports": [{"number": 8200, "protocol": "tcp"}, {"number": 8201, "protocol": "tcp"}]}}}),
    );
    // name, ports, ports[0], number, protocol, ports[1], number, protocol.
    assert_eq!(
        height("Data:vault"),
        ((2 + 40 + 12 + 8 * 27) as f64 / 27.0).ceil() as i64
    );
    // Not written: a row each of the shape's fields, an object's under it.
    ok(
        &core,
        "card_create",
        json!({"target": "Data:incomplete", "value": {"kind": "instance", "of": "Schemas:Host",
            "data": {}}}),
    );
    assert_eq!(
        height("Data:incomplete"),
        ((2 + 40 + 12 + 6 * 27) as f64 / 27.0).ceil() as i64
    );
    ok(
        &core,
        "card_create",
        json!({"target": "Data:hosts", "value": {"kind": "table", "of": "Schemas:Host"}}),
    );
    // The project's whole card, as a document is drawn.
    let doc_height = {
        ok(
            &core,
            "card_create",
            json!({"target": "Data:memo", "value": {"kind": "note", "body": "x"}}),
        );
        ok(
            &core,
            "card_create",
            json!({"target": "Docs:Whole", "value": {"kind": "freedoc", "body": "x"}}),
        );
        height("Docs:Whole")
    };
    assert_eq!(height("Data:hosts"), doc_height);
    // None of the cards placed one after another overlaps another, and the check finds none.
    let boxes: Vec<(i64, i64, i64, i64)> = [
        "Data:web",
        "Data:vault",
        "Data:incomplete",
        "Data:hosts",
        "Data:memo",
    ]
    .iter()
    .map(|p| {
        let read = ok(&core, "card_read", json!({"target": p}));
        (
            read["topLeft"]["x"].as_i64().unwrap(),
            read["topLeft"]["y"].as_i64().unwrap(),
            read["bottomRight"]["x"].as_i64().unwrap(),
            read["bottomRight"]["y"].as_i64().unwrap(),
        )
    })
    .collect();
    for (i, a) in boxes.iter().enumerate() {
        for b in &boxes[i + 1..] {
            let apart = a.2 < b.0 || b.2 < a.0 || a.3 < b.1 || b.3 < a.1;
            assert!(apart, "{a:?} と {b:?} が重なる");
        }
    }
    let after = checked(&core, "main");
    assert!(after.get("overlaps").is_none(), "{after}");
}

/// Sets a field's rules to one `pattern`.
fn pattern(core: &Core, field: &str, regex: &str) {
    ok(
        core,
        "card_update",
        json!({"target": field, "value": {"rules": [{"kind": "pattern", "regex": regex}]}}),
    );
}

#[test]
fn a_data_cards_text_not_matching_its_fields_pattern_is_refused_as_a_wrong_type_is() {
    let core = with_data();
    pattern(&core, "Schemas/Ordering:Order.customer_id", "^[a-z0-9.-]+$");
    pattern(&core, "Schemas/Ordering:Line.sku", r"^[A-Z]-\d$");
    ok(
        &core,
        "card_create",
        json!({"target": "Schemas/Ordering:Order.codes",
            "value": {"type": "string", "array": true,
                "rules": [{"kind": "pattern", "regex": "^[A-Z]+$"}]}}),
    );
    first(&core);
    // A value at a time, and the data whole: refused, saying where and which pattern.
    let why = refused(
        &core,
        "card_update",
        json!({"target": "Data:first.customer_id", "value": {"value": "Web_01"}}),
    );
    assert!(
        why.contains(r#"Data:first.customer_id: "Web_01" は pattern ^[a-z0-9.-]+$ に合いません"#),
        "{why}"
    );
    ok(
        &core,
        "card_update",
        json!({"target": "Data:first.customer_id", "value": {"value": "web-01"}}),
    );
    // An array's elements each, an embedded shape's fields too.
    let mut data = order();
    data["lines"] = json!([{"sku": "A-1", "qty": 2}, {"sku": "b2", "qty": 1}]);
    data["codes"] = json!(["AB", "c"]);
    let why = refused(
        &core,
        "card_update",
        json!({"target": "Data:first", "value": {"data": data}}),
    );
    assert!(
        why.contains(r#"Data:first.lines[1].sku: "b2" は pattern ^[A-Z]-\d$ に合いません"#),
        "{why}"
    );
    assert!(
        why.contains(r#"Data:first.codes[1]: "c" は pattern ^[A-Z]+$ に合いません"#),
        "{why}"
    );
    assert!(
        !why.contains("lines[0]") && !why.contains("codes[0]"),
        "{why}"
    );
    // Tried in a batch: refused the same, nothing written.
    let why = refused(
        &core,
        "batch",
        json!({"value": {"steps": [{"tool": "card_update",
            "arguments": {"target": "Data:first.customer_id", "value": {"value": "Web_01"}}}]}}),
    );
    assert!(why.contains("pattern ^[a-z0-9.-]+$ に合いません"), "{why}");
    let read = ok(&core, "card_read", json!({"target": "Data:first"}));
    assert_eq!(read["data"]["customer_id"], "web-01", "{read}");
}

#[test]
fn a_data_cards_value_its_fields_pattern_no_longer_matches_is_said_by_the_check() {
    let core = with_data();
    first(&core);
    ok(
        &core,
        "card_update",
        json!({"target": "Data:first.customer_id", "value": {"value": "web-01"}}),
    );
    assert!(checked(&core, "main").get("cardProblems").is_none());
    // The pattern changed under it: the check says it, and the shape's examples leave it out.
    pattern(&core, "Schemas/Ordering:Order.customer_id", "^[0-9]+$");
    let problems = checked(&core, "main")["cardProblems"].to_string();
    assert!(problems.contains("Data:first.customer_id"), "{problems}");
    assert!(
        problems.contains(r#"\"web-01\" は pattern ^[0-9]+$ に合いません"#),
        "{problems}"
    );
    let written = ok(
        &core,
        "export_read",
        json!({"targetdir": "[default]", "target": "schema", "value": {"files": ["types.schema.json"]}}),
    );
    let text = written["files"][0]["text"].as_str().unwrap();
    let schema: Value = serde_json::from_str(text).unwrap();
    assert!(
        schema["$defs"]["Order"].get("examples").is_none(),
        "{}",
        schema["$defs"]["Order"]
    );
}

#[test]
fn a_pattern_the_check_cannot_read_holds_nothing_and_is_said_by_the_check() {
    let core = with_data();
    // Read by zod, not by the check: no data, nothing said.
    pattern(
        &core,
        "Schemas/Ordering:Order.customer_id",
        "^(?!tmp)[a-z]+$",
    );
    pattern(&core, "Schemas/Ordering:Line.sku", r"^([A-Z])\1$");
    assert!(checked(&core, "main").get("uncheckedPatterns").is_none());
    // With data of the shape: what it is not held to, each once, the embedded shape's too.
    first(&core);
    ok(
        &core,
        "card_update",
        json!({"target": "Data:first.customer_id", "value": {"value": "Web_01"}}),
    );
    let unchecked = checked(&core, "main")["uncheckedPatterns"].clone();
    let rows = unchecked.as_array().unwrap();
    assert_eq!(rows.len(), 2, "{unchecked}");
    assert_eq!(rows[0]["at"], "Schemas/Ordering:Line.sku", "{unchecked}");
    assert_eq!(rows[0]["regex"], r"^([A-Z])\1$", "{unchecked}");
    assert_eq!(
        rows[1]["at"], "Schemas/Ordering:Order.customer_id",
        "{unchecked}"
    );
    assert!(
        rows[1]["problem"]
            .as_str()
            .unwrap()
            .contains("データの値を確かめていません"),
        "{unchecked}"
    );
    assert!(checked(&core, "main").get("cardProblems").is_none());
}

/// A vault's data: three ports — two on 8200, by protocol — and tags.
fn vault(core: &Core) {
    ok(
        core,
        "card_create",
        json!({"value": {
            "Schemas:Port": {"kind": "value", "fields": [
                {"name": "number", "type": "int"}, {"name": "protocol", "type": "string"},
                {"name": "note", "type": "string"}, {"name": "open", "type": "bool"}]},
            "Schemas:Vault": {"kind": "entity", "fields": [
                {"name": "ports", "type": "string", "array": true},
                {"name": "tags", "type": "string", "array": true}],
                "connections": [{"line": "embed", "from": "ports", "to": "Schemas:Port"}]}
        }}),
    );
    ok(
        core,
        "card_create",
        json!({"target": "Data:vault", "value": {"kind": "instance", "of": "Schemas:Vault", "data": {
        "tags": ["kv", "secret"],
        "ports": [
            {"number": 8200, "protocol": "tcp", "note": "API", "open": true},
            {"number": 8201, "protocol": "tcp", "note": "クラスタ"},
            {"number": 8200, "protocol": "udp"}
        ]}}}),
    );
}

#[test]
fn a_value_put_in_from_data_is_written_out_and_one_not_found_is_said_why() {
    let core = with_data();
    vault(&core);
    let body = [
        "注: {{Data:vault.ports[number=8200,protocol=tcp].note}}",
        "番号: {{Data:vault.ports[1].number}}",
        "開く: {{Data:vault.ports[0].open}}",
        "タグ: {{Data:vault.tags}}",
        "要素: {{Data:vault.ports[number=8201]}}",
        "二つ: {{Data:vault.ports[number=8200].note}}",
        "無い: {{Data:vault.ports[2].note}}",
    ]
    .join("\n\n");
    ok(
        &core,
        "card_create",
        json!({"target": "Docs:Vault", "value": {"kind": "freedoc", "body": body}}),
    );
    // Written out: a text as it is, a number and a truth as their text, an array as a table's
    // cell joins it, an element as JSON; what finds no one value struck through.
    let read = ok(
        &core,
        "export_read",
        json!({"targetdir": "[default]", "target": "docs", "value": {"files": ["notes/Vault.md"]}}),
    );
    let written = read["files"][0]["text"].as_str().unwrap();
    for line in [
        "注: `API`",
        "番号: `8201`",
        "開く: `true`",
        "タグ: `kv, secret`",
        r#"要素: `{"note":"クラスタ","number":8201,"protocol":"tcp"}`"#,
        "二つ: ~~`{{",
        "無い: ~~`{{",
    ] {
        assert!(written.contains(line), "{line}: {written}");
    }
    // The check says why of each that finds none, and nothing of the rest.
    let broken: Vec<Value> = checked(&core, "main")["brokenPaths"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|b| b["at"] == "Docs:Vault")
        .collect();
    let hints: Vec<(&str, &str)> = broken
        .iter()
        .map(|b| (b["path"].as_str().unwrap(), b["hint"].as_str().unwrap()))
        .collect();
    assert_eq!(
        hints,
        [
            (
                "Data:vault.ports[number=8200].note",
                "Data:vault.ports[number=8200]: 2 つに当たります（ports[0]・ports[2]）"
            ),
            (
                "Data:vault.ports[2].note",
                "Data:vault.ports[2].note には値が書かれていません"
            ),
        ],
        "{broken:?}"
    );
    // The window's documents put in the same, a document's paths in one request.
    let project: String = rusqlite::Connection::open(core.place())
        .unwrap()
        .query_row("SELECT id FROM projects WHERE name = ?", [PROJECT], |r| {
            r.get(0)
        })
        .unwrap();
    let put = op(
        &core,
        json!({"op": "inserted_values", "project": project, "paths": [
            "Data:vault.ports[number=8200,protocol=tcp].note", "Data:vault.tags",
            "Data:vault.ports[number=8200].note", "Schemas:Status"
        ]}),
    );
    assert_eq!(
        put["result"]["values"],
        json!({
            "Data:vault.ports[number=8200,protocol=tcp].note": "API",
            "Data:vault.tags": "kv, secret",
            "Schemas:Status": "open / paid / shipped"
        }),
        "{put}"
    );
    assert_eq!(
        put["result"]["problems"]["Data:vault.ports[number=8200].note"],
        "Data:vault.ports[number=8200]: 2 つに当たります（ports[0]・ports[2]）"
    );
}
