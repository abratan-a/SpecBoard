//! A link written out to an item of a formatdoc lands on it: the item's heading carries an anchor
//! made from the row's path, which does not change when its heading is reworded.

use crate::common;

use common::{ok, script_store};
use serde_json::json;
use specboard_core::Core;

/// A file the export `docs` writes, as `export_read` says it.
fn written(core: &Core, file: &str) -> String {
    let read = ok(
        core,
        "export_read",
        json!({"targetdir": "[default]", "target": "docs", "value": {"files": [file]}}),
    );
    read["files"][0]["text"].as_str().unwrap().to_string()
}

#[test]
fn a_link_to_a_documents_item_lands_on_its_anchor() {
    let core = Core::open(script_store()).unwrap();
    ok(
        &core,
        "card_create",
        json!({"value": {
            "Docs:Decisions": {"kind": "formatdoc", "rows": [
                {"name": "adr_single_binary", "kind": "item", "heading": "1 つのバイナリで配る", "rows": [
                    {"name": "why", "kind": "item", "heading": "理由", "rows": [
                        {"name": "body", "kind": "text", "body": "本文"}
                    ]}
                ]},
                {"name": "決め方", "kind": "item", "heading": "決め方"}
            ]},
            "Docs:Guide": {"kind": "freedoc", "body":
                "[[Docs:Decisions.adr_single_binary.why]] と [[Docs:Decisions.adr_single_binary.why.body]] と [[Docs:Decisions.決め方]]"}
        }}),
    );
    let decisions = written(&core, "notes/Decisions.md");
    assert!(
        decisions.contains(
            "<a id=\"row-adr_single_binary.why\"></a>\n\n### 理由\n<!-- row adr_single_binary.why -->"
        ),
        "{decisions}"
    );
    assert!(
        decisions.contains("<a id=\"row-_6c7a__3081__65b9_\"></a>\n\n## 決め方"),
        "{decisions}"
    );
    let guide = written(&core, "notes/Guide.md");
    assert_eq!(
        guide
            .matches("(Decisions.md#row-adr_single_binary.why)")
            .count(),
        2,
        "{guide}"
    );
    assert!(
        guide.contains("(Decisions.md#row-_6c7a__3081__65b9_)"),
        "{guide}"
    );
    let markdown = ok(
        &core,
        "card_read",
        json!({"target": "Docs:Decisions", "value": {"as": "markdown"}}),
    );
    assert!(
        !markdown["markdown"].as_str().unwrap().contains("<a id="),
        "{markdown}"
    );
}
