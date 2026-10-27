//! What MCP is answered with, cut to a size: a list cut to its default count and to the characters an
//! answer is given in, read on from the last one it gave (`after` with the answer's `next`) to the
//! end, whatever another session writes between; a body cut to its first lines, read on with `lines`;
//! a body changed in part answered with where it changed, not the whole. The window reads whole. A
//! card list gives what each card holds itself unless more is named, and takes the sections under
//! its section when asked to.

use crate::common;

use common::{call, op, place, store_with};
use serde_json::{Value, json};
use specboard_core::Core;

const SAMPLE: &str = "見本";
const LARGE: &str = "Harbor City Library Network";

/// The sample and a project of many cards (`large`).
fn core() -> Core {
    Core::open(store_with(&["sample", "large"])).unwrap()
}

/// The sample alone, for what does not need many cards.
fn sample() -> Core {
    Core::open(store_with(&["sample"])).unwrap()
}

fn mcp(core: &Core, tool: &str, arguments: Value) -> Value {
    let (failed, text) = call(core, tool, arguments.clone());
    assert!(!failed, "{tool} {arguments}: {text}");
    serde_json::from_str(&text).unwrap_or(Value::String(text))
}

fn mcp_text(core: &Core, tool: &str, arguments: Value) -> String {
    let (failed, text) = call(core, tool, arguments.clone());
    assert!(!failed, "{tool} {arguments}: {text}");
    text
}

/// A tool as the window asks for it: what it answered.
fn screen(core: &Core, tool: &str, arguments: Value) -> Value {
    let said = op(
        core,
        json!({"op": "tool", "name": tool, "arguments": arguments}),
    );
    assert!(said.get("error").is_none(), "{tool}: {said}");
    said["result"]["json"].clone()
}

/// Every page of a list read through MCP, from the top to the end: each page as it came.
fn pages(core: &Core, tool: &str, arguments: Value) -> Vec<Value> {
    let mut out = Vec::new();
    let mut after: Option<Value> = None;
    loop {
        let mut asked = arguments.clone();
        if let Some(after) = &after {
            asked["value"]["after"] = after.clone();
        }
        let page = mcp(core, tool, asked);
        let next = page.get("next").cloned();
        assert_eq!(
            next.is_some(),
            page.get("truncated") == Some(&Value::Bool(true)),
            "{page}"
        );
        out.push(page);
        match next {
            Some(n) => after = Some(n),
            None => return out,
        }
        assert!(out.len() < 500, "終わらない");
    }
}

fn paths_of(list: &Value) -> Vec<String> {
    list.as_array()
        .unwrap()
        .iter()
        .map(|c| c["path"].as_str().unwrap().to_string())
        .collect()
}

/// The items of `key` of every page, one after the other.
fn joined(pages: &[Value], key: &str) -> Vec<Value> {
    pages
        .iter()
        .flat_map(|p| p[key].as_array().cloned().unwrap_or_default())
        .collect()
}

#[test]
fn a_card_list_is_read_page_by_page_to_its_end() {
    let core = core();
    let whole = screen(
        &core,
        "card_list",
        json!({"project": LARGE, "value": {"limit": 200}}),
    );
    let all = paths_of(&whole["cards"]);
    assert!(all.len() > 100, "{}", all.len());
    let first = mcp(&core, "card_list", json!({"project": LARGE}));
    assert_eq!(first["truncated"], true);
    assert_eq!(first["matched"], all.len());
    assert_eq!(first["returned"], 20);
    assert_eq!(first["next"], first["cards"][19]["path"]);
    for limit in [20, 50, 200] {
        let read = pages(
            &core,
            "card_list",
            json!({"project": LARGE, "value": {"limit": limit}}),
        );
        assert_eq!(
            paths_of(&Value::Array(joined(&read, "cards"))),
            all,
            "limit {limit}"
        );
    }
}

#[test]
fn a_card_list_read_on_while_another_writes_misses_none_it_had() {
    let core = core();
    let all = paths_of(
        &screen(
            &core,
            "card_list",
            json!({"project": LARGE, "value": {"limit": 200}}),
        )["cards"],
    );
    let first = mcp(&core, "card_list", json!({"project": LARGE}));
    let mut read = paths_of(&first["cards"]);
    // Between the calls: one read is let go, one not yet read is let go, one is made.
    let (gone_read, gone_ahead) = (all[3].clone(), all[40].clone());
    for path in [&gone_read, &gone_ahead] {
        mcp(
            &core,
            "card_delete",
            json!({"project": LARGE, "target": path}),
        );
    }
    let section = gone_ahead.split(':').next().unwrap();
    mcp(
        &core,
        "card_create",
        json!({"project": LARGE, "target": format!("{section}:AddedBetween"), "value": {"kind": "enum", "values": [{"name": "a"}]}}),
    );
    let rest = pages(
        &core,
        "card_list",
        json!({"project": LARGE, "value": {"after": first["next"]}}),
    );
    read.extend(paths_of(&Value::Array(joined(&rest, "cards"))));
    for path in &all {
        let times = read.iter().filter(|p| *p == path).count();
        if *path == gone_ahead {
            assert_eq!(times, 0, "{path}");
        } else {
            assert_eq!(times, 1, "{path} を {times} 回読んだ");
        }
    }
    assert!(read.iter().any(|p| p.ends_with(":AddedBetween")));
}

#[test]
fn a_card_list_whose_last_card_read_is_gone_goes_on_from_the_top() {
    let core = core();
    let first = mcp(&core, "card_list", json!({"project": LARGE}));
    let next = first["next"].as_str().unwrap().to_string();
    mcp(
        &core,
        "card_delete",
        json!({"project": LARGE, "target": next}),
    );
    let again = mcp(
        &core,
        "card_list",
        json!({"project": LARGE, "value": {"after": next}}),
    );
    let said = again["warnings"][0].as_str().unwrap();
    assert!(said.contains("頭から返します"), "{said}");
    assert_eq!(again["cards"][0]["path"], first["cards"][0]["path"]);
}

#[test]
fn a_query_is_read_on_by_the_path_it_stopped_at() {
    let core = core();
    let whole = screen(
        &core,
        "card_list",
        json!({"project": LARGE, "value": {"query": "id"}}),
    );
    let cards = whole["cards"].as_array().unwrap().clone();
    assert!(cards.len() > 100, "{}", cards.len());
    assert!(whole.get("truncated").is_none(), "画面は切らない");
    let first = mcp(
        &core,
        "card_list",
        json!({"project": LARGE, "value": {"query": "id"}}),
    );
    assert_eq!(first["truncated"], true);
    assert_eq!(first["matched"], cards.len());
    assert!(first["cards"].as_array().unwrap().len() <= 100);
    let read = pages(
        &core,
        "card_list",
        json!({"project": LARGE, "value": {"query": "id", "limit": 7}}),
    );
    // Card by card, each with every place found in it, on one page.
    assert_eq!(joined(&read, "cards"), cards);
    for pair in read.windows(2) {
        let last = pair[0]["cards"].as_array().unwrap().last().unwrap()["path"].clone();
        assert_ne!(pair[1]["cards"][0]["path"], last);
    }
}

#[test]
fn a_list_stops_at_the_characters_an_answer_is_given_in() {
    let core = sample();
    let long = "長い本文の行。".repeat(1000);
    let mut made = serde_json::Map::new();
    for i in 0..12 {
        made.insert(
            format!("Docs:Long{i:02}"),
            json!({"kind": "note", "body": long}),
        );
    }
    mcp(
        &core,
        "card_create",
        json!({"project": SAMPLE, "value": made}),
    );
    let first = mcp_text(
        &core,
        "card_list",
        json!({"project": SAMPLE, "value": {"section": "Docs", "kind": "note"}}),
    );
    assert!(first.chars().count() < 41_000, "{}", first.chars().count());
    let first: Value = serde_json::from_str(&first).unwrap();
    assert_eq!(first["truncated"], true);
    assert!(first["returned"].as_u64().unwrap() < 12);
    let read = pages(
        &core,
        "card_list",
        json!({"project": SAMPLE, "value": {"section": "Docs", "kind": "note"}}),
    );
    assert_eq!(joined(&read, "cards").len(), 12);
    // A note's body in the list, as card_read gives it: its first lines.
    let lines: String = (1..=600).map(|i| format!("行{i}\n")).collect();
    mcp(
        &core,
        "card_create",
        json!({"project": SAMPLE, "target": "Docs:Lines", "value": {"kind": "note", "body": lines}}),
    );
    let listed = joined(
        &pages(
            &core,
            "card_list",
            json!({"project": SAMPLE, "value": {"section": "Docs", "kind": "note"}}),
        ),
        "cards",
    );
    let note = listed.iter().find(|c| c["path"] == "Docs:Lines").unwrap();
    assert_eq!(note["truncated"], true);
    assert_eq!(note["lineCount"], 601);
    assert_eq!(note["body"].as_str().unwrap().split('\n').count(), 400);
}

#[test]
fn the_window_reads_lists_and_bodies_whole() {
    let core = core();
    // card_list: its count, not cut by characters, and read on from where it left off when asked.
    let listed = screen(&core, "card_list", json!({"project": LARGE}));
    assert_eq!(listed["cards"].as_array().unwrap().len(), 20);
    let last = listed["cards"][19]["path"].clone();
    assert_eq!(listed["next"], last, "{listed:?}");
    let more = screen(
        &core,
        "card_list",
        json!({"project": LARGE, "value": {"after": last}}),
    );
    assert_ne!(more["cards"][0]["path"], listed["cards"][0]["path"]);
    assert_ne!(more["cards"][0]["path"], last);
    // A section of more cards than a page.
    let section = screen(
        &core,
        "section_read",
        json!({"project": LARGE, "target": "Schemas"}),
    );
    assert!(section["cards"].as_array().unwrap().len() > 100);
    assert!(section.get("truncated").is_none());
    // A body of many lines.
    let body: String = (1..=1000).map(|i| format!("行{i}\n")).collect();
    mcp(
        &core,
        "card_create",
        json!({"project": SAMPLE, "target": "Docs:Big", "value": {"kind": "freedoc", "body": body}}),
    );
    let read = screen(
        &core,
        "card_read",
        json!({"project": SAMPLE, "target": "Docs:Big"}),
    );
    assert_eq!(read["body"], body);
    assert!(read.get("truncated").is_none());
    // A text row's body changed in part: the row with its whole body, as before.
    mcp(
        &core,
        "card_create",
        json!({"project": SAMPLE, "target": "Docs:Written", "value": {"kind": "formatdoc", "rows": [
            {"name": "said", "kind": "text", "body": body},
        ]}}),
    );
    let written = screen(
        &core,
        "card_update",
        json!({"project": SAMPLE, "target": "Docs:Written.said", "value": {"body": {"append": "末尾"}}}),
    );
    assert_eq!(written["body"], format!("{body}末尾"));
    let written = screen(
        &core,
        "card_update",
        json!({"project": SAMPLE, "target": "Docs:Big", "value": {"body": {"append": "末尾"}}}),
    );
    assert!(written.get("body").is_none(), "{written}");
}

#[test]
fn a_section_and_the_other_lists_are_read_on_to_their_ends() {
    let core = core();
    // A section's cards past a page.
    let whole = screen(
        &core,
        "section_read",
        json!({"project": LARGE, "target": "Schemas"}),
    );
    let read = pages(
        &core,
        "section_read",
        json!({"project": LARGE, "target": "Schemas"}),
    );
    assert_eq!(read[0]["truncated"], true);
    assert_eq!(read[0]["cards"].as_array().unwrap().len(), 100);
    assert_eq!(read[0]["matched"], whole["cards"].as_array().unwrap().len());
    assert_eq!(
        read[0]["next"],
        format!("cards {}", read[0]["cards"][99].as_str().unwrap())
    );
    assert_eq!(joined(&read, "cards"), *whole["cards"].as_array().unwrap());
    // Lists of several lists, a page of one at a time.
    for (tool, arguments, lists) in [
        (
            "kind_read",
            json!({"project": SAMPLE}),
            vec!["cards", "sections", "lines"],
        ),
        (
            "tag_list",
            json!({"project": LARGE}),
            vec!["tags", "groups", "declared", "ungrouped"],
        ),
        (
            "branch_read",
            json!({"project": SAMPLE, "target": "refunds"}),
            vec![
                "changed",
                "made",
                "removed",
                "mainChanged",
                "mainMade",
                "mainRemoved",
            ],
        ),
    ] {
        let whole = screen(&core, tool, arguments.clone());
        let mut limited = arguments.clone();
        limited["value"] = json!({"limit": 1});
        let read = pages(&core, tool, limited);
        assert!(read.len() > 2, "{tool}: {}", read.len());
        for list in lists {
            assert_eq!(
                Value::Array(joined(&read, list)),
                whole.get(list).cloned().unwrap_or(json!([])),
                "{tool} {list}"
            );
        }
    }
    // The check of a tree is read item by item, a page apiece; the counts and groups of the whole
    // tree stay whole in every page.
    let all = mcp(
        &core,
        "inspection_list",
        json!({"project": SAMPLE, "value": {"show": "all"}}),
    );
    let items = all["items"].as_array().unwrap();
    assert!(items.len() > 2, "{all}");
    let first = mcp(
        &core,
        "inspection_list",
        json!({"project": SAMPLE, "value": {"show": "all", "limit": 1}}),
    );
    assert_eq!(first["items"].as_array().unwrap().len(), 1, "{first}");
    assert_eq!(first["items"][0]["group"], "brokenPaths", "{first}");
    assert_eq!(first["items"][0]["id"], items[0]["id"], "{first}");
    assert_eq!(first["truncated"], true, "{first}");
    assert_eq!(first["next"], first["items"][0]["id"], "{first}");
    assert_eq!(first["groups"], all["groups"], "{first}");
    assert_eq!(first["counts"], all["counts"], "{first}");
    let rest = mcp(
        &core,
        "inspection_list",
        json!({"project": SAMPLE, "value": {"show": "all", "limit": 1, "after": first["next"]}}),
    );
    assert_eq!(rest["items"].as_array().unwrap().len(), 1, "{rest}");
    assert_ne!(rest["items"][0]["id"], first["items"][0]["id"], "{rest}");
    assert_eq!(rest["items"][0]["id"], items[1]["id"], "{rest}");
    assert_eq!(rest["groups"], all["groups"], "{rest}");
    assert_eq!(rest["counts"], all["counts"], "{rest}");
    // An export's files, through the exports (read against a folder nothing was written to).
    let folder = place("unwritten");
    std::fs::create_dir_all(&folder).unwrap();
    let files = pages(
        &core,
        "export_read",
        json!({"project": SAMPLE, "targetdir": folder.to_str().unwrap(), "target": ["ts", "docs"], "value": {"limit": 1}}),
    );
    let said: Vec<String> = files
        .iter()
        .flat_map(|p| {
            p["exports"]
                .as_array()
                .unwrap()
                .iter()
                .flat_map(|e| {
                    e["files"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|f| {
                            format!(
                                "{} {}",
                                e["name"].as_str().unwrap(),
                                f["path"].as_str().unwrap()
                            )
                        })
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>()
        })
        .collect();
    assert_eq!(said.len(), 5, "{said:?}");
    assert_eq!(files[0]["next"], said[0]);
    // The projects, by name; the branches.
    let projects = pages(&core, "project_list", json!({"value": {"limit": 1}}));
    let names: Vec<Value> = joined(&projects, "projects")
        .iter()
        .map(|p| p["project"].clone())
        .collect();
    assert_eq!(names, vec![json!(LARGE), json!(SAMPLE)]);
    mcp(
        &core,
        "branch_create",
        json!({"project": SAMPLE, "target": "another"}),
    );
    let branches = pages(
        &core,
        "branch_list",
        json!({"project": SAMPLE, "value": {"limit": 1}}),
    );
    assert_eq!(joined(&branches, "branches").len(), 2);
    let sections = pages(
        &core,
        "section_list",
        json!({"project": SAMPLE, "value": {"limit": 3}}),
    );
    assert_eq!(
        Value::Array(joined(&sections, "sections")),
        screen(&core, "section_list", json!({"project": SAMPLE}))["sections"]
    );
}

#[test]
fn a_project_gone_from_between_the_pages_is_gone_on_from_where_it_stood() {
    let core = Core::open(store_with(&["sample", "datamodel", "small"])).unwrap();
    let names = |page: &Value| -> Vec<String> {
        page["projects"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p["project"].as_str().unwrap().to_string())
            .collect()
    };
    let all = names(&mcp(&core, "project_list", json!({})));
    assert_eq!(all.len(), 3);
    let first = mcp(&core, "project_list", json!({"value": {"limit": 1}}));
    assert_eq!(names(&first), vec![all[0].clone()]);
    // The one it stopped at, by the name it has no more: from what comes after that name.
    let gone = all[0].clone();
    mcp(
        &core,
        "project_update",
        json!({"project": gone, "value": {"name": "zzz 改名した"}}),
    );
    let now = names(&mcp(&core, "project_list", json!({})));
    let after_it = now
        .iter()
        .find(|n| n.as_str() > gone.as_str())
        .unwrap()
        .clone();
    let next = mcp(
        &core,
        "project_list",
        json!({"value": {"limit": 1, "after": first["next"]}}),
    );
    assert_eq!(names(&next), vec![after_it], "{next}");
    assert!(
        next["warnings"][0]
            .as_str()
            .unwrap()
            .contains("その次に来るもの"),
        "{next}"
    );
}

#[test]
fn a_body_is_cut_to_its_first_lines_and_read_on_with_lines() {
    let core = sample();
    let body: String = (1..=1000)
        .map(|i| format!("行{i}"))
        .collect::<Vec<_>>()
        .join("\n");
    mcp(
        &core,
        "card_create",
        json!({"project": SAMPLE, "target": "Docs:Big", "value": {"kind": "freedoc", "body": body}}),
    );
    let lines: Vec<&str> = body.split('\n').collect();
    let first = mcp(
        &core,
        "card_read",
        json!({"project": SAMPLE, "target": "Docs:Big"}),
    );
    assert_eq!(first["truncated"], true);
    assert_eq!(first["lineCount"], 1000);
    assert_eq!(first["lines"], json!({"from": 1, "to": 400}));
    assert_eq!(first["body"], lines[..400].join("\n"));
    let second = mcp(
        &core,
        "card_read",
        json!({"project": SAMPLE, "target": "Docs:Big", "value": {"lines": {"from": 401}}}),
    );
    assert_eq!(second["truncated"], true);
    assert_eq!(second["lines"], json!({"from": 401, "to": 800}));
    let last = mcp(
        &core,
        "card_read",
        json!({"project": SAMPLE, "target": "Docs:Big", "value": {"lines": {"from": 801}}}),
    );
    assert!(last.get("truncated").is_none());
    assert_eq!(last["body"], lines[800..].join("\n"));
    // The lines asked for, to their end: as many as asked.
    let told = mcp(
        &core,
        "card_read",
        json!({"project": SAMPLE, "target": "Docs:Big", "value": {"lines": {"from": 1, "to": 1000}}}),
    );
    assert_eq!(told["body"], body);
    assert!(told.get("truncated").is_none());
    // As Markdown: the answer says which lines.
    let whole = screen(
        &core,
        "card_read",
        json!({"project": SAMPLE, "target": "Docs:Big", "value": {"as": "markdown"}}),
    );
    assert_eq!(whole["path"], "Docs:Big");
    assert!(whole.get("lines").is_none(), "{whole}");
    let text = whole["markdown"].as_str().unwrap().to_string();
    let count = text.split('\n').count();
    assert_eq!(whole["lineCount"], count);
    let mut read: Vec<String> = Vec::new();
    let mut from = 1;
    loop {
        let mut value = json!({"as": "markdown"});
        if from > 1 {
            value["lines"] = json!({"from": from});
        }
        let part = mcp(
            &core,
            "card_read",
            json!({"project": SAMPLE, "target": "Docs:Big", "value": value}),
        );
        let to = (from + 399).min(count);
        assert_eq!(part["lines"], json!({"from": from, "to": to}), "{part}");
        assert_eq!(part["lineCount"], count, "{part}");
        read.push(part["markdown"].as_str().unwrap().to_string());
        if to == count {
            assert!(part.get("truncated").is_none(), "{part}");
            break;
        }
        assert_eq!(part["truncated"], true, "{part}");
        from = to + 1;
    }
    assert_eq!(read.join("\n"), text);
}

#[test]
fn a_flow_read_as_markdown_is_cut_as_a_body_is_and_read_on_with_lines() {
    let core = sample();
    // A flow of many nodes, each a step to the next.
    let mut value = serde_json::Map::new();
    value.insert("Charts/Shop:Long".into(), json!({"kind": "flow"}));
    value.insert(
        "Charts/Shop:Long.start".into(),
        json!({"kind": "begin", "trigger": "manual", "connections": [{"line": "next", "to": "n0"}]}),
    );
    for i in 0..80 {
        let next = if i == 79 {
            "end".to_string()
        } else {
            format!("n{}", i + 1)
        };
        value.insert(
            format!("Charts/Shop:Long.n{i}"),
            json!({"kind": "process", "steps": [{"do": "write", "description": format!("{i} 番目")}],
                "connections": [{"line": "writes", "from": "steps[0]", "to": "Schemas:Orders"}, {"line": "next", "to": next}]}),
        );
    }
    value.insert(
        "Charts/Shop:Long.end".into(),
        json!({"kind": "end", "outcome": "success"}),
    );
    mcp(
        &core,
        "card_create",
        json!({"project": SAMPLE, "value": value}),
    );
    let whole = screen(
        &core,
        "card_read",
        json!({"project": SAMPLE, "target": "Charts/Shop:Long", "value": {"as": "markdown"}}),
    );
    let text = whole["markdown"].as_str().unwrap().to_string();
    let count = text.split('\n').count();
    assert!(count > 400, "{count}");
    let first = mcp(
        &core,
        "card_read",
        json!({"project": SAMPLE, "target": "Charts/Shop:Long", "value": {"as": "markdown"}}),
    );
    assert_eq!(first["lines"], json!({"from": 1, "to": 400}), "{first}");
    assert_eq!(first["lineCount"], count, "{first}");
    assert_eq!(first["truncated"], true, "{first}");
    let rest = mcp(
        &core,
        "card_read",
        json!({"project": SAMPLE, "target": "Charts/Shop:Long", "value": {"as": "markdown", "lines": {"from": 401}}}),
    );
    assert_eq!(
        rest["lines"],
        json!({"from": 401, "to": count.min(800)}),
        "{rest}"
    );
    assert_eq!(count <= 800, rest.get("truncated").is_none(), "{rest}");
    let (part, more) = (
        first["markdown"].as_str().unwrap(),
        rest["markdown"].as_str().unwrap(),
    );
    assert!(text.starts_with(&format!("{part}\n{more}")), "{text}");
}

#[test]
fn a_version_is_cut_too() {
    let core = sample();
    let body: String = (1..=500)
        .map(|i| format!("- {i}"))
        .collect::<Vec<_>>()
        .join("\n");
    mcp(
        &core,
        "card_create",
        json!({"project": SAMPLE, "target": "Docs:Changes", "value": {"kind": "stackdoc", "versions": [{"name": "v1", "key": "1", "body": body}]}}),
    );
    let read = mcp(
        &core,
        "card_read",
        json!({"project": SAMPLE, "target": "Docs:Changes.v1"}),
    );
    assert_eq!(read["truncated"], true);
    assert_eq!(read["lineCount"], 500);
    assert_eq!(read["body"].as_str().unwrap().split('\n').count(), 400);
}

#[test]
fn a_body_changed_in_part_is_answered_with_where_it_changed() {
    let core = sample();
    let body: String = (1..=300)
        .map(|i| format!("行{i}"))
        .collect::<Vec<_>>()
        .join("\n");
    mcp(
        &core,
        "card_create",
        json!({"project": SAMPLE, "target": "Docs:Big", "value": {"kind": "freedoc", "body": body}}),
    );
    let written = mcp(
        &core,
        "card_update",
        json!({"project": SAMPLE, "target": "Docs:Big", "value": {"body": {"append": "足した"}}}),
    );
    assert_eq!(written["path"], "Docs:Big");
    assert_eq!(
        written["body"],
        json!({"lineCount": 301, "chars": body.chars().count() + 4, "changed": [
            {"line": 301, "text": "足した", "before": ["行299", "行300"]}
        ]})
    );
    let edited = mcp(
        &core,
        "card_update",
        json!({"project": SAMPLE, "target": "Docs:Big", "value": {"body": {"edit": [{"old": "行150\n", "new": "百五十\n"}]}}}),
    );
    assert_eq!(edited["body"]["changed"][0]["line"], 150);
    assert_eq!(edited["body"]["changed"][0]["text"], "百五十");
    assert!(written.to_string().chars().count() < 1000, "{written}");
    // Written whole: as before (a card's answer leaves its body out).
    let whole = mcp(
        &core,
        "card_update",
        json!({"project": SAMPLE, "target": "Docs:Big", "value": {"body": "丸ごと"}}),
    );
    assert!(whole.get("body").is_none(), "{whole}");
    // A formatdoc's text row and a stackdoc's version.
    mcp(
        &core,
        "card_create",
        json!({"project": SAMPLE, "target": "Docs:Written", "value": {"kind": "formatdoc", "rows": [
            {"name": "story", "heading": "ねらい", "rows": [{"name": "said", "kind": "text", "body": body}]},
        ]}}),
    );
    let row = mcp(
        &core,
        "card_update",
        json!({"project": SAMPLE, "target": "Docs:Written.story.said", "value": {"body": {"prepend": "頭"}}}),
    );
    assert_eq!(
        row["body"]["changed"][0],
        json!({"line": 1, "text": "頭", "after": ["行1", "行2"]})
    );
    assert_eq!(row["body"]["lineCount"], 301);
    let whole_row = mcp(
        &core,
        "card_update",
        json!({"project": SAMPLE, "target": "Docs:Written.story", "value": {"heading": "ねらい2"}}),
    );
    assert_eq!(whole_row["heading"], "ねらい2", "{whole_row}");
    let whole_text = mcp(
        &core,
        "card_update",
        json!({"project": SAMPLE, "target": "Docs:Written.story.said", "value": {"body": format!("頭\n{body}")}}),
    );
    assert_eq!(
        whole_text["body"],
        format!("頭\n{body}"),
        "丸ごと書いたものは今のまま"
    );
    mcp(
        &core,
        "card_create",
        json!({"project": SAMPLE, "target": "Docs:Changes", "value": {"kind": "stackdoc", "versions": [{"name": "v1", "key": "1", "body": body}]}}),
    );
    let version = mcp(
        &core,
        "card_update",
        json!({"project": SAMPLE, "target": "Docs:Changes.v1", "value": {"body": {"edit": [{"old": "行2\n", "new": ""}]}}}),
    );
    assert_eq!(version["body"]["changed"][0]["removed"], 1, "{version}");
    // Read again, it is what was written.
    let read = mcp(
        &core,
        "card_read",
        json!({"project": SAMPLE, "target": "Docs:Written.story.said", "value": {"lines": {"from": 1, "to": 2}}}),
    );
    assert_eq!(read["body"], "頭\n行1");
}

/// A table of `rows` rows of three columns, each cell long enough that the whole table is far more
/// than a change to one of its cells.
fn big_table(core: &Core, rows: usize) -> usize {
    let cells: Vec<Value> = (0..rows)
        .map(|i| {
            json!([
                format!("op{i}"),
                format!("引数{i}"),
                format!("答え{i}の長い説明。{}", "あ".repeat(120))
            ])
        })
        .collect();
    mcp(
        core,
        "card_create",
        json!({"project": SAMPLE, "target": "Docs:Api", "value": {"kind": "formatdoc", "rows": [
            {"name": "ops", "kind": "table", "columns": ["op", "引数", "答え"], "cells": cells},
        ]}}),
    );
    mcp_text(
        core,
        "card_read",
        json!({"project": SAMPLE, "target": "Docs:Api.ops"}),
    )
    .chars()
    .count()
}

/// A table's cells changed in part are answered with the rows that changed and the rows beside
/// them, numbered as `set`, `add` and `remove` number them; written whole they are answered as
/// before. What the next write is held to (`rev`) stays.
#[test]
fn a_tables_cells_changed_in_part_are_answered_with_the_rows_that_changed() {
    let core = sample();
    let whole_size = big_table(&core, 200);
    let target = "Docs:Api.ops";
    let ask =
        |cells: Value| json!({"project": SAMPLE, "target": target, "value": {"cells": cells}});
    // One cell.
    let one = mcp(
        &core,
        "card_update",
        ask(json!({"set": [{"row": {"op": "op100"}, "col": "引数", "value": "直した"}]})),
    );
    assert_eq!(one["path"], target);
    assert_eq!(one["columns"], json!(["op", "引数", "答え"]));
    let changed = &one["cells"];
    assert_eq!(changed["rowCount"], 200);
    let places = changed["changed"].as_array().unwrap();
    assert_eq!(places.len(), 1, "{one}");
    assert_eq!(places[0]["row"], 100);
    assert!(places[0].get("to").is_none(), "{one}");
    assert_eq!(places[0]["rows"][0][0], "op100");
    assert_eq!(places[0]["rows"][0][1], "直した");
    assert_eq!(places[0]["before"][0][0], "op99");
    assert_eq!(places[0]["after"][0][0], "op101");
    assert_eq!(places[0]["before"].as_array().unwrap().len(), 1);
    let size = one.to_string().chars().count();
    assert!(size * 20 < whole_size, "{size} / {whole_size}");
    // Held to the card as it is now: the rev it answers is the one the next write is given.
    let rev = one["rev"].as_str().unwrap().to_string();
    let read = mcp(
        &core,
        "card_read",
        json!({"project": SAMPLE, "target": target}),
    );
    assert_eq!(read["rev"], rev.as_str());
    assert_eq!(read["cells"][100][1], "直した");
    // Rows added (after one, at the end) and one taken away.
    let added = mcp(
        &core,
        "card_update",
        ask(json!({
            "add": [
                {"cells": ["new", "足した", "行"], "after": {"op": "op10"}},
                {"cells": ["tail", "尾", "行"]}
            ],
            "remove": [{"row": {"op": "op50"}}]
        })),
    );
    assert_eq!(added["cells"]["rowCount"], 201);
    let places = added["cells"]["changed"].as_array().unwrap();
    let rows: Vec<(u64, Value)> = places
        .iter()
        .map(|p| (p["row"].as_u64().unwrap(), p.clone()))
        .collect();
    assert_eq!(
        rows.iter().map(|r| r.0).collect::<Vec<_>>(),
        [11, 51, 200],
        "{added}"
    );
    assert_eq!(places[0]["rows"][0][0], "new");
    assert_eq!(places[0]["before"][0][0], "op10");
    assert_eq!(places[0]["after"][0][0], "op11");
    assert_eq!(places[1]["removed"], 1);
    assert!(places[1].get("rows").is_none(), "{added}");
    assert_eq!(places[1]["before"][0][0], "op49");
    assert_eq!(places[1]["after"][0][0], "op51");
    assert_eq!(places[2]["rows"][0][0], "tail");
    assert!(places[2].get("after").is_none(), "{added}");
    assert!(
        added.to_string().chars().count() * 10 < whole_size,
        "{added}"
    );
    // Many rows put in at once: the first and the last of them, the rest counted.
    let many: Vec<Value> = (0..12).map(|i| json!([format!("m{i}"), "", ""])).collect();
    let flood = mcp(
        &core,
        "card_update",
        ask(
            json!({"add": many.iter().map(|c| json!({"cells": c, "before": {"op": "op0"}})).collect::<Vec<_>>()}),
        ),
    );
    let place = &flood["cells"]["changed"][0];
    assert_eq!(place["row"], 0);
    assert_eq!(place["to"], 11);
    assert_eq!(place["omitted"], 6);
    assert_eq!(place["rows"].as_array().unwrap().len(), 6);
    assert_eq!(place["after"][0][0], "op0");
    // Written whole, the rows come back as before.
    let whole = mcp(&core, "card_update", ask(json!([["only", "一つ", "行"]])));
    assert_eq!(whole["cells"], json!([["only", "一つ", "行"]]));
    assert!(whole.get("rowCount").is_none(), "{whole}");
}

/// The window reads the rows whole, as it reads a body whole: it keeps them to draw.
#[test]
fn the_window_is_answered_a_changed_table_with_all_its_rows() {
    let core = sample();
    big_table(&core, 30);
    let said = screen(
        &core,
        "card_update",
        json!({"project": SAMPLE, "target": "Docs:Api.ops", "value": {"cells": {"set": [
            {"row": {"op": "op3"}, "col": "引数", "value": "直した"}]}}}),
    );
    assert_eq!(said["cells"].as_array().unwrap().len(), 30, "{said}");
    assert_eq!(said["cells"][3][1], "直した");
}

/// What a card list gives of a card when `include` names nothing: what the card holds itself.
fn bare_of_the_rest(card: &Value) {
    for key in [
        "topLeft",
        "bottomRight",
        "usedBy",
        "mentionedBy",
        "referencedBy",
        "writtenTo",
    ] {
        assert!(card.get(key).is_none(), "{key}: {card}");
    }
}

#[test]
fn a_card_list_gives_what_each_card_holds_itself_and_the_rest_when_named() {
    let core = sample();
    for value in [json!({"limit": 200}), json!({"limit": 200, "include": []})] {
        let listed = mcp(
            &core,
            "card_list",
            json!({"project": SAMPLE, "value": value}),
        );
        let cards = listed["cards"].as_array().unwrap();
        assert!(!cards.is_empty());
        cards.iter().for_each(bare_of_the_rest);
        let customer = cards
            .iter()
            .find(|c| c["path"] == "Schemas:Customer")
            .unwrap();
        assert!(customer.get("fields").is_some(), "{customer}");
        let order = cards
            .iter()
            .find(|c| c["path"] == "Schemas/Ordering:Order")
            .unwrap();
        assert!(order.get("refersTo").is_some(), "{order}");
    }
    // The window lists the same.
    let window = screen(
        &core,
        "card_list",
        json!({"project": SAMPLE, "value": {"limit": 200}}),
    );
    window["cards"]
        .as_array()
        .unwrap()
        .iter()
        .for_each(bare_of_the_rest);
    // Where they sit and what points back at them, when they are named.
    let named = mcp(
        &core,
        "card_list",
        json!({"project": SAMPLE, "value": {"section": "Schemas", "include": ["at", "used_by", "ref_by"]}}),
    );
    let cards = named["cards"].as_array().unwrap();
    let customer = cards
        .iter()
        .find(|c| c["path"] == "Schemas:Customer")
        .unwrap();
    assert!(customer.get("topLeft").is_some(), "{customer}");
    assert!(customer.get("bottomRight").is_some(), "{customer}");
    assert!(customer.get("referencedBy").is_some(), "{customer}");
    assert!(customer.get("fields").is_none(), "{customer}");
    // A card read is given every part still.
    let read = mcp(
        &core,
        "card_read",
        json!({"project": SAMPLE, "target": "Schemas:Customer"}),
    );
    assert!(read.get("topLeft").is_some(), "{read}");
    assert!(read.get("referencedBy").is_some(), "{read}");
}

#[test]
fn a_card_list_gathers_the_sections_under_its_section_when_asked() {
    let core = sample();
    let list = |value: Value| {
        paths_of(
            &mcp(
                &core,
                "card_list",
                json!({"project": SAMPLE, "value": value}),
            )["cards"],
        )
    };
    let own = list(json!({"section": "Schemas"}));
    assert!(!own.is_empty());
    assert!(own.iter().all(|p| p.starts_with("Schemas:")), "{own:?}");
    let under = list(json!({"section": "Schemas", "recursive": true}));
    assert!(
        under.contains(&"Schemas/Ordering:Order".to_string()),
        "{under:?}"
    );
    assert!(
        under.contains(&"Schemas/Ordering:Line".to_string()),
        "{under:?}"
    );
    // The section's own first, then the section under it, each as it was.
    assert_eq!(under[..own.len()], own[..]);
    assert!(
        under[own.len()..]
            .iter()
            .all(|p| p.starts_with("Schemas/Ordering:")),
        "{under:?}"
    );
    // As it was without it, and so with it false.
    assert_eq!(list(json!({"section": "Schemas", "recursive": false})), own);
    // A section of no cards of its own: those of the sections under it.
    assert!(list(json!({"section": "Charts"})).is_empty());
    let charts = list(json!({"section": "Charts", "recursive": true}));
    assert!(
        charts.contains(&"Charts/Shop:Checkout".to_string()),
        "{charts:?}"
    );
    // A query too.
    let found = |value: Value| -> Vec<String> {
        common::matches(&mcp(
            &core,
            "card_list",
            json!({"project": SAMPLE, "value": value}),
        ))
        .iter()
        .map(|h| h["at"].as_str().unwrap_or_default().to_string())
        .collect()
    };
    assert!(
        !found(json!({"query": "Line", "section": "Schemas"}))
            .iter()
            .any(|p| p.starts_with("Schemas/Ordering:")),
    );
    assert!(
        found(json!({"query": "Line", "section": "Schemas", "recursive": true}))
            .iter()
            .any(|p| p.starts_with("Schemas/Ordering:Line")),
    );
    // Under what section, or with what reads one section alone, is said.
    for (value, said) in [
        (json!({"recursive": true}), "section"),
        (
            json!({"section": "Schemas", "recursive": true, "roots": ["Customer"]}),
            "roots",
        ),
        (
            json!({"section": "Schemas", "recursive": true, "region": {"from": {"x": 0, "y": 0}, "to": {"x": 9, "y": 9}}}),
            "region",
        ),
    ] {
        let (failed, text) = call(
            &core,
            "card_list",
            json!({"project": SAMPLE, "value": value}),
        );
        assert!(failed, "{text}");
        assert!(text.contains("recursive") && text.contains(said), "{text}");
    }
}

#[test]
fn answers_leave_out_what_says_nothing() {
    let core = sample();
    let stores = mcp(
        &core,
        "card_list",
        json!({"project": SAMPLE, "value": {"kind": "store"}}),
    );
    let store = stores["cards"][0]["path"].as_str().unwrap().to_string();
    let section = store.split(':').next().unwrap().to_string();
    let bare = format!("{section}:Bare");
    mcp(
        &core,
        "card_create",
        json!({"project": SAMPLE, "target": bare, "value": {"kind": "entity"}}),
    );
    mcp(
        &core,
        "card_create",
        json!({"project": SAMPLE, "branch": "_project_board_", "target": "Goals:Empty", "value": {"kind": "goal"}}),
    );
    let list = mcp(
        &core,
        "card_list",
        json!({"project": SAMPLE, "value": {"section": section}}),
    );
    assert_eq!(list["project"], SAMPLE, "{list}");
    for card in list["cards"].as_array().unwrap() {
        assert!(card.get("project").is_none(), "1 枚ずつに project: {card}");
        if card["path"] == store.as_str() || card["path"] == bare.as_str() {
            assert!(card.get("fields").is_none(), "空の fields: {card}");
        }
    }
    let read = mcp(
        &core,
        "card_read",
        json!({"project": SAMPLE, "target": store}),
    );
    assert!(read.get("fields").is_none(), "{read}");
    let goal = mcp(
        &core,
        "card_read",
        json!({"project": SAMPLE, "branch": "_project_board_", "target": "Goals:Empty"}),
    );
    assert!(
        goal.get("cards").is_none() && goal.get("progress").is_none(),
        "{goal}"
    );
}
