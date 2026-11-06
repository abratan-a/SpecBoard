//! What the reads and writes say where a name or a path does not land as written: `card_list` refuses
//! a tag or a field it does not know, a path through an embedded field is refused naming where it
//! leads, a reason a card gives for a field it lacks follows a rename and is checked, an export's
//! broken embed keeps its path whole and speaks to a reader; every read is headed with its project;
//! and a section, the project and a kind each have a rev a write is held to, as a card does.

use crate::common;

use common::{checked, ok, place, refused, script_store};
use serde_json::{Value, json};
use specboard_core::Core;

fn core() -> Core {
    Core::open(script_store()).unwrap()
}

fn is_rev(rev: &Value) -> String {
    let rev = rev.as_str().unwrap_or_else(|| panic!("rev がない: {rev}"));
    assert_eq!(rev.len(), 12, "{rev}");
    assert!(rev.chars().all(|c| c.is_ascii_hexdigit()), "{rev}");
    rev.to_string()
}

#[test]
fn card_list_refuses_a_tag_no_card_has_and_says_the_tags_there_are() {
    let core = core();
    let said = refused(&core, "card_list", json!({"value": {"tags": ["無いタグ"]}}));
    assert!(said.contains("無いタグ"), "{said}");
    assert!(said.contains("中核"), "{said}");
    // A tag there is, held by no card of the kind asked for: nothing found, as before.
    let none = ok(
        &core,
        "card_list",
        json!({"value": {"tags": ["中核"], "kind": "enum"}}),
    );
    assert_eq!(none["cards"], json!([]), "{none}");
}

#[test]
fn card_list_refuses_a_field_no_kind_it_lists_has() {
    let core = core();
    let said = refused(
        &core,
        "card_list",
        json!({"branch": "_project_board_", "value": {"kind": "goal", "fields": {"無い欄": "x"}}}),
    );
    assert!(said.contains("無い欄"), "{said}");
    assert!(said.contains("status"), "{said}");
    // A field the kind has, holding no such value: nothing found.
    let none = ok(
        &core,
        "card_list",
        json!({"branch": "_project_board_", "value": {"kind": "goal", "fields": {"status": "cancelled"}}}),
    );
    assert_eq!(none["cards"], json!([]), "{none}");
}

#[test]
fn a_path_through_an_embedded_field_is_refused_saying_the_card_it_leads_into() {
    let core = core();
    let said = refused(
        &core,
        "card_read",
        json!({"target": "Schemas/Ordering:Order.lines[].qty"}),
    );
    assert!(said.contains("Schemas/Ordering:Line.qty"), "{said}");
    // Where it leads is read there.
    ok(
        &core,
        "card_read",
        json!({"target": "Schemas/Ordering:Line.qty"}),
    );
}

#[test]
fn a_reason_for_a_field_a_card_lacks_follows_a_rename_and_is_checked() {
    let core = core();
    ok(
        &core,
        "card_update",
        json!({"target": "Schemas/Ordering:Order", "value": {"absentFields": [
            {"name": "discount", "reason": "割引は [[Schemas:Customer]] で持つ"},
            {"name": "coupon", "reason": "[[Schemas:Nope]] に置く"},
        ]}}),
    );
    ok(
        &core,
        "card_update",
        json!({"target": "Schemas:Customer", "value": {"name": "Client"}}),
    );
    let order = ok(
        &core,
        "card_read",
        json!({"target": "Schemas/Ordering:Order", "value": {"include": ["absentFields"]}}),
    );
    assert_eq!(
        order["absentFields"][0]["reason"], "割引は [[Schemas:Client]] で持つ",
        "{order}"
    );
    let after = checked(&core, "main");
    let broken: Vec<&Value> = after["brokenPaths"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|b| b["path"] == "Schemas:Nope")
        .collect();
    assert_eq!(broken.len(), 1, "{after}");
    assert!(
        broken[0]["at"].as_str().unwrap().contains("Order"),
        "{after}"
    );
}

#[test]
fn an_exported_broken_embed_keeps_its_path_whole_and_speaks_to_a_reader() {
    let core = core();
    ok(
        &core,
        "card_update",
        json!({"target": "Docs:Overview", "value": {"body": "前置き\n\n![[Schemas:Nope]]\n"}}),
    );
    let folder = place("broken-embed");
    std::fs::create_dir_all(&folder).unwrap();
    // Exports written without asking each time, as a person may let them be.
    let set = common::op(
        &core,
        json!({"op": "mcp_writes_set", "project": common::project_id(&core), "group": "export", "state": "allow"}),
    );
    assert!(set.get("error").is_none(), "{set}");
    ok(
        &core,
        "export_create",
        json!({"target": "notes", "value": {"path": folder.to_str().unwrap(), "entries": [
            {"from": "Docs", "to": "docs/"},
        ]}}),
    );
    ok(
        &core,
        "export_execute",
        json!({"targetdir": folder.to_str().unwrap(), "target": "notes"}),
    );
    let mut said = String::new();
    for entry in walk(&folder) {
        let text = std::fs::read_to_string(&entry).unwrap_or_default();
        if text.contains("Nope") {
            said = text;
        }
    }
    let line = said
        .lines()
        .find(|l| l.contains("⚠︎"))
        .unwrap_or_else(|| panic!("壊れた埋め込みの行が無い: {said}"));
    assert!(line.contains("`![[Schemas:Nope]]`"), "{line}");
    assert!(!line.contains("card_list"), "{line}");
}

fn walk(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(walk(&path));
        } else {
            out.push(path);
        }
    }
    out
}

#[test]
fn tag_list_and_exists_are_headed_with_the_project() {
    let core = core();
    let tags = ok(&core, "tag_list", json!({}));
    assert_eq!(
        tags.as_object().unwrap().keys().next().map(String::as_str),
        Some("project"),
        "{tags}"
    );
    let exists = ok(
        &core,
        "card_read",
        json!({"target": "Schemas:Customer", "value": {"as": "exists"}}),
    );
    assert_eq!(
        exists
            .as_object()
            .unwrap()
            .keys()
            .next()
            .map(String::as_str),
        Some("project"),
        "{exists}"
    );
}

#[test]
fn a_section_s_rev_holds_a_write_to_the_section_as_it_was_read() {
    let core = core();
    let read = is_rev(&ok(&core, "section_read", json!({"target": "Docs"}))["rev"]);
    // Its cards are not in it.
    ok(
        &core,
        "card_update",
        json!({"target": "Docs:Overview", "value": {"label": "概要の表示名"}}),
    );
    assert_eq!(
        is_rev(&ok(&core, "section_read", json!({"target": "Docs"}))["rev"]),
        read
    );
    let wrote = ok(
        &core,
        "section_update",
        json!({"target": "Docs", "value": {"description": "一つめ", "rev": read}}),
    );
    let now = is_rev(&wrote["rev"]);
    assert_ne!(now, read);
    let said = refused(
        &core,
        "section_update",
        json!({"target": "Docs", "value": {"description": "二つめ", "rev": read}}),
    );
    assert!(said.contains("Docs"), "{said}");
    assert!(said.contains(&now), "{said}");
    let said = refused(
        &core,
        "section_delete",
        json!({"target": "Docs", "value": {"rev": read}}),
    );
    assert!(said.contains(&now), "{said}");
    assert_eq!(
        ok(&core, "section_read", json!({"target": "Docs"}))["description"],
        "一つめ"
    );
}

#[test]
fn the_project_s_rev_holds_a_write_to_its_settings_as_they_were_read() {
    let core = core();
    let read = is_rev(&ok(&core, "project_read", json!({}))["rev"]);
    // A card written leaves it as it was.
    ok(
        &core,
        "card_update",
        json!({"target": "Docs:Overview", "value": {"label": "概要の表示名"}}),
    );
    assert_eq!(is_rev(&ok(&core, "project_read", json!({}))["rev"]), read);
    let wrote = ok(
        &core,
        "project_update",
        json!({"value": {"description": "一つめ", "rev": read}}),
    );
    let now = is_rev(&wrote["rev"]);
    assert_ne!(now, read);
    let said = refused(
        &core,
        "project_update",
        json!({"value": {"description": "二つめ", "rev": read}}),
    );
    assert!(said.contains(&now), "{said}");
    assert_eq!(
        ok(&core, "project_read", json!({}))["description"],
        "一つめ"
    );
}

#[test]
fn a_kind_s_rev_holds_a_write_to_its_definition_as_it_was_read() {
    let core = core();
    let read = is_rev(&ok(&core, "kind_read", json!({"target": "signedBy"}))["rev"]);
    let wrote = ok(
        &core,
        "kind_update",
        json!({"target": "signedBy", "value": {"label": "署名の鍵", "rev": read}}),
    );
    let now = is_rev(&wrote["rev"]);
    assert_ne!(now, read);
    let said = refused(
        &core,
        "kind_update",
        json!({"target": "signedBy", "value": {"label": "鍵", "rev": read}}),
    );
    assert!(said.contains("signedBy"), "{said}");
    assert!(said.contains(&now), "{said}");
    let said = refused(
        &core,
        "kind_delete",
        json!({"target": "signedBy", "value": {"rev": read}}),
    );
    assert!(said.contains(&now), "{said}");
    assert_eq!(
        ok(&core, "kind_read", json!({"target": "signedBy"}))["label"],
        "署名の鍵"
    );
}

#[test]
fn the_english_words_say_contents_map_never_content_map() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut found = Vec::new();
    for file in walk(&root) {
        let in_english = file
            .to_str()
            .is_some_and(|p| p.contains("/text/en") || p.ends_with("/mcp/text/en.json"));
        if !in_english || file.extension().is_none_or(|e| e != "json") {
            continue;
        }
        let text = std::fs::read_to_string(&file).unwrap();
        for line in text
            .lines()
            .filter(|l| l.to_lowercase().contains("content map"))
        {
            found.push(format!("{}: {}", file.display(), line.trim()));
        }
    }
    assert!(found.is_empty(), "{found:#?}");
}
