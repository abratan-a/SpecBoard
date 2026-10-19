//! Tables (`table`; `specboard://SpecBoard/Docs/Design:CardKinds`「表（table）」):
//! the data cards of a shape gathered by a table's rules (where, columns, sort, unnest, within) into
//! a table — read by MCP, put in a document, following its shape's renames, its paths that no longer
//! resolve said by the check.

use crate::common;

use common::{checked, ok, op, refused, script_store};
use serde_json::{Value, json};
use specboard_core::Core;

const PROJECT: &str = "見本";

/// The sample project with a data cards' section `Data` and three orders in it: first (paid, two
/// lines), second (open, none) and third (paid, one).
fn orders() -> Core {
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
    for (name, label, status, total, lines) in [
        (
            "first",
            "最初の注文",
            "paid",
            "12.50",
            json!([{"sku": "A-1", "qty": 2}, {"sku": "B-2", "qty": 1}]),
        ),
        ("second", "", "open", "3.00", json!([])),
        (
            "third",
            "三つ目",
            "paid",
            "40.00",
            json!([{"sku": "C-3", "qty": 4}]),
        ),
    ] {
        ok(
            &core,
            "card_create",
            json!({"target": format!("Data:{name}"), "value": {"kind": "instance", "label": label,
                "of": "Schemas/Ordering:Order",
                "data": {"id": "5f0c1a2b-0000-4000-8000-00000000abcd", "status": status,
                    "total": total, "lines": lines, "kind": "order"}}}),
        );
    }
    core
}

fn table(core: &Core, card: &str) -> (Vec<String>, Vec<Vec<String>>) {
    let read = ok(core, "card_read", json!({"target": card}));
    let t = &read["table"];
    let strings = |v: &Value| -> Vec<String> {
        v.as_array()
            .unwrap()
            .iter()
            .map(|s| s.as_str().unwrap().to_string())
            .collect()
    };
    (
        strings(&t["columns"]),
        t["rows"].as_array().unwrap().iter().map(strings).collect(),
    )
}

#[test]
fn a_table_gathers_its_shapes_data_by_its_rules() {
    let core = orders();
    ok(
        &core,
        "card_create",
        json!({"target": "Data:paid", "value": {"kind": "table", "of": "Schemas/Ordering:Order",
            "where": [{"path": "status", "op": "eq", "value": "paid"}],
            "columns": [{"path": "@label", "label": "注文"}, {"path": "total"}, {"path": "lines[].sku", "label": "品"}],
            "sort": [{"path": "total", "desc": true}]}}),
    );
    let read = ok(&core, "card_read", json!({"target": "Data:paid"}));
    assert_eq!(read["kind"], "table");
    assert_eq!(read["of"], "Schemas/Ordering:Order");
    assert_eq!(read["where"][0]["op"], "eq");
    let (columns, rows) = table(&core, "Data:paid");
    assert_eq!(columns, ["注文", "total", "品"]);
    assert_eq!(
        rows,
        [
            vec!["三つ目", "40.00", "C-3"],
            vec!["最初の注文", "12.50", "A-1, B-2"]
        ]
    );
    // With no columns, the name and the shape's top fields; with no rules, by name.
    ok(
        &core,
        "card_create",
        json!({"target": "Data:all", "value": {"kind": "table", "of": "Schemas/Ordering:Order"}}),
    );
    let (columns, rows) = table(&core, "Data:all");
    assert_eq!(columns[0], "名前");
    assert!(columns.contains(&"status".to_string()), "{columns:?}");
    assert_eq!(
        rows.iter().map(|r| r[0].as_str()).collect::<Vec<_>>(),
        ["first", "second", "third"]
    );
    // An element a row: the array's other columns said again; none for one with no elements.
    ok(
        &core,
        "card_update",
        json!({"target": "Data:all", "value": {"unnest": "lines",
            "columns": [{"path": "@name"}, {"path": "lines[].sku"}, {"path": "lines[].qty"}],
            "where": [{"path": "lines[].qty", "op": "ge", "value": 2}]}}),
    );
    let (columns, rows) = table(&core, "Data:all");
    assert_eq!(columns, ["名前", "lines[].sku", "lines[].qty"]);
    assert_eq!(rows, [vec!["first", "A-1", "2"], vec!["third", "C-3", "4"]]);
    // contains: in a text, and among an array's elements; exists.
    for (filter, names) in [
        (
            json!({"path": "@label", "op": "contains", "value": "注文"}),
            vec!["first"],
        ),
        (
            json!({"path": "lines[].sku", "op": "contains", "value": "B-"}),
            vec!["first"],
        ),
        (
            json!({"path": "lines", "op": "exists", "value": false}),
            vec![],
        ),
        (
            json!({"path": "lines", "op": "exists", "value": true}),
            vec!["first", "second", "third"],
        ),
        (
            json!({"path": "lines[].sku", "op": "exists", "value": true}),
            vec!["first", "third"],
        ),
        (
            json!({"path": "lines[].sku", "op": "exists", "value": false}),
            vec!["second"],
        ),
        (
            json!({"path": "total", "op": "lt", "value": 13}),
            vec!["first", "second"],
        ),
        (
            json!({"path": "status", "op": "ne", "value": "paid"}),
            vec!["second"],
        ),
    ] {
        ok(
            &core,
            "card_update",
            json!({"target": "Data:all", "value": {"unnest": null, "columns": [{"path": "@name"}], "where": [filter]}}),
        );
        let (_, rows) = table(&core, "Data:all");
        assert_eq!(
            rows.iter().map(|r| r[0].as_str()).collect::<Vec<_>>(),
            names,
            "{filter}"
        );
    }
    // Within a section: none there.
    ok(
        &core,
        "card_update",
        json!({"target": "Data:all", "value": {"where": null, "within": "Free"}}),
    );
    assert!(table(&core, "Data:all").1.is_empty());
    // As Markdown, `|` let out.
    let md = ok(
        &core,
        "card_read",
        json!({"target": "Data:paid", "value": {"as": "markdown"}}),
    );
    assert!(
        md["markdown"]
            .as_str()
            .unwrap()
            .contains("| 注文 | total | 品 |\n|---|---|---|\n| 三つ目 | 40.00 | C-3 |"),
        "{md}"
    );
}

#[test]
fn a_table_is_held_to_its_shape_and_follows_it() {
    let core = orders();
    // Refused: no shape, a path the shape has not, a way to compare that is none, an unnest that
    // is not an array, a key a rule does not take.
    for (value, why) in [
        (json!({"kind": "table"}), "表（table）には of が要ります"),
        (
            json!({"kind": "table", "of": "Schemas/Ordering:Order", "columns": [{"path": "nope"}]}),
            "columns[0].path: 形 Order に「nope」はありません",
        ),
        (
            json!({"kind": "table", "of": "Schemas/Ordering:Order", "where": [{"path": "status", "op": "like", "value": "x"}]}),
            "where[0].op: 比べ方は eq",
        ),
        (
            json!({"kind": "table", "of": "Schemas/Ordering:Order", "unnest": "status"}),
            "unnest は並び（配列）の欄のパスです",
        ),
        (
            json!({"kind": "table", "of": "Schemas/Ordering:Order", "sort": [{"path": "total", "up": true}]}),
            "up は書けません",
        ),
        (
            json!({"kind": "table", "of": "Schemas/Ordering:Order", "columns": [{"path": "lines.sku"}]}),
            "lines[] か要素を選んで指します",
        ),
    ] {
        let said = refused(
            &core,
            "card_create",
            json!({"target": "Data:wrong", "value": value}),
        );
        assert!(said.contains(why), "{value}: {said}");
    }
    ok(
        &core,
        "card_create",
        json!({"target": "Data:paid", "value": {"kind": "table", "of": "Schemas/Ordering:Order",
            "where": [{"path": "status", "op": "eq", "value": "paid"}],
            "columns": [{"path": "@name"}, {"path": "lines[sku=A-1].qty"}],
            "sort": [{"path": "status"}], "unnest": "lines"}}),
    );
    // A field renamed: the paths follow, the picks' fields too.
    ok(
        &core,
        "card_update",
        json!({"target": "Schemas/Ordering:Order.status", "value": {"name": "state"}}),
    );
    ok(
        &core,
        "card_update",
        json!({"target": "Schemas/Ordering:Line.sku", "value": {"name": "code"}}),
    );
    ok(
        &core,
        "card_update",
        json!({"target": "Schemas/Ordering:Order.lines", "value": {"name": "items"}}),
    );
    let read = ok(&core, "card_read", json!({"target": "Data:paid"}));
    assert_eq!(read["where"][0]["path"], "state");
    assert_eq!(read["columns"][1]["path"], "items[code=A-1].qty");
    assert_eq!(read["sort"][0]["path"], "state");
    assert_eq!(read["unnest"], "items");
    // A field let go of: the check says where; the table built without it.
    ok(
        &core,
        "card_delete",
        json!({"target": "Schemas/Ordering:Order.state"}),
    );
    let broken = checked(&core, "main")["brokenPaths"].to_string();
    assert!(
        broken.contains("Data:paid") && broken.contains("where[0].path"),
        "{broken}"
    );
    assert!(
        table(&core, "Data:paid").1.is_empty(),
        "解けない条件には合わない"
    );
    // A table is not made a data card, nor the other way.
    let said = refused(
        &core,
        "card_update",
        json!({"target": "Data:paid", "value": {"kind": "note"}}),
    );
    assert!(
        said.contains("表（table）とほかの種類は互いに変えられません"),
        "{said}"
    );
    // The shape gone: the line `of` says it.
    ok(
        &core,
        "card_delete",
        json!({"target": "Schemas/Ordering:Order"}),
    );
    let broken = checked(&core, "main")["brokenPaths"].to_string();
    assert!(broken.contains("形のカードがありません"), "{broken}");
}

#[test]
fn a_table_is_put_in_a_document() {
    let core = orders();
    ok(
        &core,
        "card_create",
        json!({"target": "Data:paid", "value": {"kind": "table", "of": "Schemas/Ordering:Order",
            "where": [{"path": "status", "op": "eq", "value": "paid"}],
            "columns": [{"path": "@name"}, {"path": "total"}]}}),
    );
    let rows = "| first | 12.50 |\n| third | 40.00 |";
    // A formatdoc's row card.
    ok(
        &core,
        "card_create",
        json!({"target": "Docs:Spec.cards.paid", "value": {"kind": "card", "target": "Data:paid"}}),
    );
    let md = ok(
        &core,
        "card_read",
        json!({"target": "Docs:Spec", "value": {"as": "markdown"}}),
    );
    assert!(md["markdown"].as_str().unwrap().contains(rows), "{md}");
    // The check takes a table for a row's card, as the Markdown draws it.
    let check = checked(&core, "main");
    assert!(!check.to_string().contains("cards.paid"), "{check}");
    // `![[…]]` in the window: the table's Markdown, not a diagram.
    let project: String = rusqlite::Connection::open(core.place())
        .unwrap()
        .query_row("SELECT id FROM projects WHERE name = ?", [PROJECT], |r| {
            r.get(0)
        })
        .unwrap();
    let embedded = op(
        &core,
        json!({"op": "embed_diagram", "project": project, "path": "Data:paid"}),
    );
    let markdown = embedded["result"]["markdown"].as_str().unwrap_or_default();
    assert!(markdown.contains(rows), "{embedded}");
    // Drawn on the diagram: its Markdown beside it.
    let detail = op(&core, json!({"op": "project_detail", "project": project}));
    let card = detail["result"]["schemas"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "paid")
        .unwrap();
    assert!(
        card["tableMarkdown"].as_str().unwrap().contains(rows),
        "{card}"
    );
    // Listed by its kind.
    let listed = ok(&core, "card_list", json!({"value": {"kind": "table"}}));
    assert_eq!(listed["cards"][0]["path"], "Data:paid", "{listed}");
}

#[test]
fn a_tables_picture_in_a_body_is_read_as_the_table_it_is_written_out_as() {
    // `as: markdown` says a table's `![[…]]` as the export writes it, the other paths as
    // written.
    let core = orders();
    ok(
        &core,
        "card_create",
        json!({"target": "Data:paid", "value": {"kind": "table", "of": "Schemas/Ordering:Order",
            "where": [{"path": "status", "op": "eq", "value": "paid"}],
            "columns": [{"path": "@name"}, {"path": "total"}]}}),
    );
    let rows = "| first | 12.50 |\n| third | 40.00 |";
    ok(
        &core,
        "card_create",
        json!({"target": "Docs:Paid", "value": {"kind": "freedoc",
            "body": "# 払った注文\n\n![[Data:paid]]\n\n形は [[Schemas/Ordering:Order]]"}}),
    );
    let md = ok(
        &core,
        "card_read",
        json!({"target": "Docs:Paid", "value": {"as": "markdown"}}),
    );
    let md = md["markdown"].as_str().unwrap();
    assert!(md.contains(rows), "{md}");
    assert!(!md.contains("![[Data:paid]]"), "{md}");
    assert!(md.contains("[[Schemas/Ordering:Order]]"), "{md}");
    // A stackdoc's version, read alone: the same.
    ok(
        &core,
        "card_create",
        json!({"target": "Docs:Log", "value": {"kind": "stackdoc",
            "versions": [{"name": "v1", "key": "1", "body": "![[Data:paid]]"}]}}),
    );
    let md = ok(
        &core,
        "card_read",
        json!({"target": "Docs:Log.v1", "value": {"as": "markdown"}}),
    );
    assert!(md["markdown"].as_str().unwrap().contains(rows), "{md}");
}

#[test]
fn a_fields_list_value_is_said_as_its_elements_joined() {
    // A field that is also a list says its elements joined, as `lines[].sku` does — not as
    // JSON.
    let core = orders();
    ok(
        &core,
        "card_create",
        json!({"target": "Schemas:Mount", "value": {"kind": "entity", "fields": [
            {"name": "engines", "type": "string", "also": ["array"]}]}}),
    );
    for (name, engines) in [("vault", json!(["secrets", "kv"])), ("plain", json!("kv"))] {
        ok(
            &core,
            "card_create",
            json!({"target": format!("Data:{name}"), "value": {"kind": "instance",
                "of": "Schemas:Mount", "data": {"engines": engines}}}),
        );
    }
    ok(
        &core,
        "card_create",
        json!({"target": "Data:mounts", "value": {"kind": "table", "of": "Schemas:Mount",
            "columns": [{"path": "@name"}, {"path": "engines"}]}}),
    );
    let (_, rows) = table(&core, "Data:mounts");
    assert_eq!(rows, [vec!["plain", "kv"], vec!["vault", "secrets, kv"]]);
    let md = ok(
        &core,
        "card_read",
        json!({"target": "Data:mounts", "value": {"as": "markdown"}}),
    );
    assert!(
        md["markdown"]
            .as_str()
            .unwrap()
            .contains("| vault | secrets, kv |"),
        "{md}"
    );
}

/// The orders with a field naming a card: first's names a document, second's a data card by its
/// name alone, third's nothing that is there.
fn orders_naming_cards() -> Core {
    let core = orders();
    ok(
        &core,
        "card_create",
        json!({"target": "Schemas/Ordering:Order.doc", "value": {"type": "string"}}),
    );
    for (name, doc) in [
        ("first", "Docs:Overview"),
        ("second", "third"),
        ("third", "Docs:Nowhere"),
    ] {
        ok(
            &core,
            "card_update",
            json!({"target": format!("Data:{name}.doc"), "value": {"value": doc}}),
        );
    }
    core
}

#[test]
fn a_columns_show_says_an_enums_values_by_their_labels_and_a_cards_path_as_a_link() {
    let core = orders_naming_cards();
    ok(
        &core,
        "card_create",
        json!({"target": "Data:shown", "value": {"kind": "table", "of": "Schemas/Ordering:Order",
            "columns": [{"path": "@name"}, {"path": "status", "show": "label"},
                {"path": "doc", "show": "link"}, {"path": "status", "label": "値"}]}}),
    );
    let (columns, rows) = table(&core, "Data:shown");
    assert_eq!(columns, ["名前", "status", "doc", "値"]);
    // An enum's value by its label, the value when it has none; a path in full as a link, one
    // read from the data card's section written in full, one naming nothing as it is.
    assert_eq!(
        rows,
        [
            vec!["first", "paid", "[[Docs:Overview]]", "paid"],
            vec!["second", "受付", "[[Data:third]]", "open"],
            vec!["third", "paid", "Docs:Nowhere", "paid"],
        ]
    );
    // What a rule compares and sorts by is the value, not what is shown.
    ok(
        &core,
        "card_create",
        json!({"target": "Data:open", "value": {"kind": "table", "of": "Schemas/Ordering:Order",
            "where": [{"path": "status", "op": "eq", "value": "open"}],
            "columns": [{"path": "@name"}, {"path": "status", "show": "label"}],
            "sort": [{"path": "status"}]}}),
    );
    assert_eq!(table(&core, "Data:open").1, [vec!["second", "受付"]]);
    // A show that is none of them is refused, saying which there are.
    let why = refused(
        &core,
        "card_create",
        json!({"target": "Data:bold", "value": {"kind": "table", "of": "Schemas/Ordering:Order",
            "columns": [{"path": "status", "show": "bold"}]}}),
    );
    assert!(why.contains("Data:bold.columns[0].show"), "{why}");
    assert!(why.contains("label・link"), "{why}");
}

#[test]
fn a_tables_links_are_written_out_as_a_bodys_and_read_by_mcp_as_written() {
    let core = orders_naming_cards();
    ok(
        &core,
        "card_create",
        json!({"value": {
            "Data:docs": {"kind": "table", "of": "Schemas/Ordering:Order",
                "columns": [{"path": "@name"}, {"path": "doc", "show": "link"}]},
            "Docs:Orders": {"kind": "freedoc", "body": "# 注文\n\n![[Data:docs]]"},
            "Docs:Spec.cards.docs": {"kind": "card", "target": "Data:docs"}
        }}),
    );
    // As MCP reads it: the paths as written.
    let md = ok(
        &core,
        "card_read",
        json!({"target": "Docs:Orders", "value": {"as": "markdown"}}),
    );
    let md = md["markdown"].as_str().unwrap();
    assert!(md.contains("| first | [[Docs:Overview]] |"), "{md}");
    assert!(md.contains("| third | Docs:Nowhere |"), "{md}");
    // Written out: a link to the file the card is written to, as a body's path is — in the body's
    // picture and in a formatdoc's row card alike.
    let written = ok(
        &core,
        "export_read",
        json!({"targetdir": "[default]", "target": "docs",
            "value": {"files": ["notes/Orders.md", "notes/Spec.md"]}}),
    );
    let text: String = written["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["text"].as_str().unwrap())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        text.contains("| first | [Overview](Overview.md) |"),
        "{text}"
    );
    assert_eq!(
        text.matches("| first | [Overview](Overview.md) |").count(),
        2,
        "{text}"
    );
    // A data card not written out: its label, not a link.
    assert!(text.contains("| second | 三つ目 |"), "{text}");
    assert!(text.contains("| third | Docs:Nowhere |"), "{text}");
    // On the screen: the paths as written, for the window to draw as links.
    let project: String = rusqlite::Connection::open(core.place())
        .unwrap()
        .query_row("SELECT id FROM projects WHERE name = ?", [PROJECT], |r| {
            r.get(0)
        })
        .unwrap();
    let embedded = op(
        &core,
        json!({"op": "embed_diagram", "project": project, "path": "Data:docs"}),
    );
    assert!(
        embedded["result"]["markdown"]
            .as_str()
            .unwrap_or_default()
            .contains("| first | [[Docs:Overview]] |"),
        "{embedded}"
    );
}
