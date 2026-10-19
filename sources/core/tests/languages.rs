//! What the core says, in the language of each call (`specboard://SpecBoard/Docs/Design:Localization`
//! §3): a history line kept by its key and said in the language of whoever reads it (a line kept
//! before keys were, as it was kept), the window as a mark's author said in each language, the check's
//! items the same in every language, and the first sentence of a description cut where the language
//! ends one.

use crate::common;

use common::{PROJECT, checked, headers, ok, op, project_id, script_store};
use serde_json::{Value, json};
use specboard_core::Core;

/// Whether `text` has a letter of Japanese (kana, kanji, the full-width forms).
fn japanese(text: &str) -> bool {
    text.chars().any(|c| {
        matches!(c, '\u{3040}'..='\u{30ff}' | '\u{4e00}'..='\u{9fff}' | '\u{ff00}'..='\u{ffef}')
            || matches!(c, '「' | '」' | '。' | '、')
    })
}

/// A tool called through MCP on the sample's project, in `lang`: what it answered, or why it was
/// refused.
fn mcp_in(core: &Core, lang: &str, tool: &str, mut arguments: Value) -> Result<Value, String> {
    arguments["project"] = PROJECT.into();
    let message = json!({
        "jsonrpc": "2.0", "id": 1, "method": "tools/call",
        "params": {"name": tool, "arguments": arguments}
    });
    let reply = op(
        core,
        json!({"op": "mcp", "message": message, "headers": headers(core), "lang": lang}),
    );
    let result = &reply["result"]["body"]["result"];
    let text = result["content"][0]["text"]
        .as_str()
        .unwrap_or_else(|| panic!("{reply}"))
        .to_string();
    if result["isError"].as_bool().unwrap_or(false) {
        return Err(text);
    }
    Ok(serde_json::from_str(&text).unwrap_or(Value::String(text)))
}

/// A tool the window calls, in `lang`.
fn window_in(core: &Core, lang: &str, tool: &str, mut arguments: Value) -> Value {
    arguments["project"] = PROJECT.into();
    let said = op(
        core,
        json!({"op": "tool", "name": tool, "arguments": arguments, "lang": lang}),
    );
    assert!(said.get("error").is_none(), "{tool}: {said}");
    said["result"]["json"].clone()
}

/// The lines of main's newest commit, as the window reads them in `lang`.
fn newest_lines(core: &Core, lang: &str) -> Vec<String> {
    let read = op(
        core,
        json!({"op": "commits", "project": project_id(core), "limit": 1, "lang": lang}),
    );
    read["result"]["commits"][0]["lines"]
        .as_array()
        .unwrap_or_else(|| panic!("{read}"))
        .iter()
        .map(|l| l["summary"].as_str().unwrap().to_string())
        .collect()
}

/// What the store keeps of main's newest commit's lines: `(summary, text_key)`.
fn kept_lines(core: &Core) -> Vec<(String, Option<String>)> {
    let conn = rusqlite::Connection::open(core.place()).unwrap();
    let mut stmt = conn
        .prepare(
            "SELECT l.summary, l.text_key FROM commit_lines l JOIN commits c ON c.id = l.commit_id
              WHERE c.project_id = ? AND c.branch_id = '' AND c.seq = (SELECT MAX(seq) FROM commits
              WHERE project_id = c.project_id AND branch_id = '') ORDER BY l.idx",
        )
        .unwrap();
    stmt.query_map([project_id(core)], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

#[test]
fn a_history_line_is_said_in_the_language_of_whoever_reads_it() {
    let core = Core::open(script_store()).unwrap();
    // Written in English through MCP: read back in Japanese by the window, and the other way.
    mcp_in(
        &core,
        "en",
        "card_create",
        json!({"target": "Free:Later", "value": {"kind": "note"}}),
    )
    .unwrap();
    let kept = kept_lines(&core);
    assert!(kept.iter().all(|(_, key)| key.is_some()), "{kept:?}");
    let japanese_lines = newest_lines(&core, "ja");
    let english_lines = newest_lines(&core, "en");
    // The sentence kept beside the key is the Japanese one, as an app of before reads it.
    assert_eq!(
        japanese_lines,
        kept.iter().map(|(s, _)| s.clone()).collect::<Vec<_>>()
    );
    assert!(
        japanese_lines.iter().any(|l| japanese(l)),
        "{japanese_lines:?}"
    );
    assert!(
        english_lines.iter().all(|l| !japanese(l)),
        "{english_lines:?}"
    );
    assert_ne!(japanese_lines, english_lines);

    window_in(
        &core,
        "ja",
        "card_create",
        json!({"target": "Free:Sooner", "value": {"kind": "note"}}),
    );
    let english = newest_lines(&core, "en");
    assert!(english.iter().all(|l| !japanese(l)), "{english:?}");
    assert_eq!(
        newest_lines(&core, "ja"),
        kept_lines(&core)
            .into_iter()
            .map(|(s, _)| s)
            .collect::<Vec<_>>()
    );
    // MCP's history reads the lines in its own language too.
    let history = mcp_in(
        &core,
        "en",
        "branch_read",
        json!({"target": "main", "value": {"as": "history", "limit": 1}}),
    )
    .unwrap();
    let lines = history["commits"][0]["lines"].as_array().unwrap();
    assert_eq!(
        lines
            .iter()
            .map(|l| l.as_str().unwrap().to_string())
            .collect::<Vec<_>>(),
        english
    );
    assert!(history["commits"][0]["author"] == "Screen", "{history}");
}

#[test]
fn a_history_line_kept_before_its_key_is_said_as_it_was_kept() {
    let core = Core::open(script_store()).unwrap();
    ok(
        &core,
        "card_create",
        json!({"target": "Free:Later", "value": {"kind": "note"}}),
    );
    let conn = rusqlite::Connection::open(core.place()).unwrap();
    let newest = "(SELECT id FROM commits WHERE project_id = ?1 AND branch_id = '' ORDER BY seq DESC LIMIT 1)";
    // A line of before: no key, a sentence alone.
    conn.execute(
        &format!(
            "UPDATE commit_lines SET text_key = NULL, text_values = NULL, summary = '前からの行' WHERE commit_id = {newest}"
        ),
        [project_id(&core)],
    )
    .unwrap();
    assert_eq!(newest_lines(&core, "en"), ["前からの行"]);
    assert_eq!(newest_lines(&core, "ja"), ["前からの行"]);
    // One whose key the tables no longer have: as it was kept, too.
    conn.execute(
        &format!(
            "UPDATE commit_lines SET text_key = 'history.gone.long', summary = '消えた呼び名の行' WHERE commit_id = {newest}"
        ),
        [project_id(&core)],
    )
    .unwrap();
    assert_eq!(newest_lines(&core, "en"), ["消えた呼び名の行"]);
}

#[test]
fn a_mark_by_the_window_is_said_as_the_window_in_each_language() {
    let core = Core::open(script_store()).unwrap();
    let all = window_in(
        &core,
        "en",
        "inspection_list",
        json!({"value": {"show": "all"}}),
    );
    let ids: Vec<Value> = all["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["id"].clone())
        .collect();
    let marked = window_in(
        &core,
        "en",
        "inspection_update",
        json!({"target": ids[0], "value": {"accept": "as it is"}}),
    );
    assert_eq!(marked["updated"][0]["accepted"]["by"], "Screen");
    let read = |lang: &str, id: &Value| -> Value {
        window_in(&core, lang, "inspection_read", json!({"target": [id]}))["items"][0]["accepted"]
            ["by"]
            .clone()
    };
    assert_eq!(read("ja", &ids[0]), "画面");
    assert_eq!(read("en", &ids[0]), "Screen");
    // Kept as the window, not as the words of one language.
    let conn = rusqlite::Connection::open(core.place()).unwrap();
    let by: String = conn
        .query_row(
            "SELECT by FROM inspection_marks WHERE id = ?",
            [ids[0].as_str().unwrap()],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(by, "@screen");
    // A mark made before, kept as the window's Japanese name, is the window's too.
    window_in(
        &core,
        "ja",
        "inspection_update",
        json!({"target": ids[1], "value": {"accept": "前から"}}),
    );
    conn.execute(
        "UPDATE inspection_marks SET by = '画面' WHERE id = ?",
        [ids[1].as_str().unwrap()],
    )
    .unwrap();
    assert_eq!(read("en", &ids[1]), "Screen");
    assert_eq!(read("ja", &ids[1]), "画面");
    // A key's name is its own in every language.
    let by_key = mcp_in(
        &core,
        "en",
        "inspection_update",
        json!({"target": ids[2], "value": {"accept": "by MCP"}}),
    )
    .unwrap();
    assert_eq!(by_key["updated"][0]["accepted"]["by"], common::KEY);
}

/// The sample with something of more of the check's groups: data its shape requires more of, a
/// pattern the check cannot read, and a flow with a node nothing leads to.
fn much_to_check() -> Core {
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
    ok(
        &core,
        "card_update",
        json!({"target": "Schemas/Ordering:Line.sku",
            "value": {"rules": [{"kind": "pattern", "regex": r"^([A-Z])\1$"}]}}),
    );
    ok(
        &core,
        "card_create",
        json!({"target": "Data:draft", "value": {"kind": "instance", "of": "Schemas/Ordering:Order",
            "data": {"status": "open", "lines": [{"sku": "AA", "qty": 1}]}}}),
    );
    ok(
        &core,
        "card_create",
        json!({"value": {
            "Charts/Shop:Lost": {"kind": "flow"},
            "Charts/Shop:Lost.start": {"kind": "begin", "trigger": "receive",
                "connections": [{"line": "next", "to": "work"}]},
            "Charts/Shop:Lost.work": {"kind": "process"},
            "Charts/Shop:Lost.astray": {"kind": "process"}
        }}),
    );
    core
}

#[test]
fn an_item_of_the_check_is_the_same_in_every_language() {
    let core = much_to_check();
    let groups = checked(&core, "main");
    for group in [
        "brokenPaths",
        "cardProblems",
        "flowProblems",
        "peerMismatches",
        "freeCards",
        "uncheckedPatterns",
    ] {
        assert!(groups.get(group).is_some(), "{group}: {groups}");
    }
    let listed = |lang: &str| -> Vec<(String, String)> {
        mcp_in(
            &core,
            lang,
            "inspection_list",
            json!({"value": {"show": "all", "limit": 1000}}),
        )
        .unwrap()["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| {
                (
                    i["id"].as_str().unwrap().to_string(),
                    i["problem"].as_str().unwrap().to_string(),
                )
            })
            .collect()
    };
    let japanese_items = listed("ja");
    let english_items = listed("en");
    let ids = |items: &[(String, String)]| -> Vec<String> {
        items.iter().map(|(id, _)| id.clone()).collect()
    };
    assert_eq!(ids(&japanese_items), ids(&english_items));
    for ((id, ja), (_, en)) in japanese_items.iter().zip(&english_items) {
        assert!(japanese(ja), "{id}: {ja}");
        assert!(!japanese(en), "{id}: {en}");
    }
    // Marked in one language, as it is in the other: its print is the same.
    let every: Vec<String> = ids(&japanese_items);
    mcp_in(
        &core,
        "ja",
        "inspection_update",
        json!({"target": every, "value": {"accept": "設計どおり"}}),
    )
    .unwrap();
    let accepted = mcp_in(
        &core,
        "en",
        "inspection_list",
        json!({"value": {"show": "all", "limit": 1000}}),
    )
    .unwrap();
    for item in accepted["items"].as_array().unwrap() {
        assert!(item.get("accepted").is_some(), "{item}");
        assert!(item.get("acceptedBefore").is_none(), "{item}");
    }
    assert_eq!(accepted["counts"]["accepted"], every.len());
}

#[test]
fn a_description_s_first_sentence_ends_where_the_language_ends_one() {
    use specboard_core::{mcp::brief, text::bind};
    {
        let _ja = bind(Some("ja"));
        assert_eq!(brief("一つ目です。二つ目です。"), "一つ目です。");
        assert_eq!(brief("括弧（中。）の外。次。"), "括弧（中。）の外。");
        assert_eq!(brief("a. b です。次"), "a. b です。");
        assert_eq!(brief("句点の無い文"), "句点の無い文");
    }
    let _en = bind(Some("en"));
    assert_eq!(brief("Reads a card. Then more."), "Reads a card.");
    assert_eq!(brief("One (a. b). Two."), "One (a. b).");
    assert_eq!(brief("Card.name is a path. Next"), "Card.name is a path.");
    assert_eq!(brief("Ends at the end."), "Ends at the end.");
    assert_eq!(brief("No end at all"), "No end at all");
    // A text not said in English yet is cut where Japanese ends a sentence.
    assert_eq!(brief("日本語のままの文。次。"), "日本語のままの文。");
}

#[test]
fn a_refusal_and_a_warning_are_said_in_the_language_of_the_call() {
    let core = Core::open(script_store()).unwrap();
    let refused = mcp_in(
        &core,
        "en",
        "card_create",
        json!({"target": "Nowhere:Card", "value": {"kind": "note"}}),
    )
    .unwrap_err();
    assert!(!japanese(&refused), "{refused}");
    let refused = mcp_in(
        &core,
        "ja",
        "card_create",
        json!({"target": "Nowhere:Card", "value": {"kind": "note"}}),
    )
    .unwrap_err();
    assert!(japanese(&refused), "{refused}");
    // A path written elsewhere that stops resolving when its card goes.
    ok(
        &core,
        "card_create",
        json!({"target": "Free:Pointer", "value": {"kind": "note", "body": "[[Free:Memo]]"}}),
    );
    let gone = mcp_in(&core, "en", "card_delete", json!({"target": "Free:Memo"})).unwrap();
    let warnings = gone["warnings"].to_string();
    assert!(warnings.contains("Free:Memo"), "{gone}");
    assert!(!japanese(&warnings), "{gone}");
}

/// Whether `text` has Japanese besides the values a goal keeps (its status and priority), which every
/// language quotes as they are.
fn japanese_but_kept(text: &str) -> bool {
    let mut text = text.to_string();
    for value in ["未着手", "進行中", "完了", "打ち切り", "高", "中", "低"] {
        text = text.replace(value, "");
    }
    japanese(&text)
}

/// An MCP message answered in `lang`, as the agent sends it with its answer language.
fn rpc_in(core: &Core, lang: &str, method: &str, params: Value) -> Value {
    let message = json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params});
    let reply = op(
        core,
        json!({"op": "mcp", "message": message, "headers": headers(core), "lang": lang}),
    );
    reply["result"]["body"]["result"].clone()
}

#[test]
fn the_built_in_kinds_are_said_in_the_language_of_the_call() {
    let core = Core::open(script_store()).unwrap();
    let entity = |listed: &Value| -> Value {
        listed["cards"]
            .as_array()
            .unwrap_or_else(|| panic!("{listed}"))
            .iter()
            .find(|k| k["kind"] == "entity")
            .cloned()
            .unwrap_or_else(|| panic!("{listed}"))
    };
    let japanese_list = mcp_in(&core, "ja", "kind_list", json!({})).unwrap();
    let english_list = mcp_in(&core, "en", "kind_list", json!({})).unwrap();
    assert!(
        entity(&japanese_list).to_string().contains("記録"),
        "{japanese_list}"
    );
    let english_entity = entity(&english_list);
    assert!(
        english_entity.to_string().contains("Record"),
        "{english_entity}"
    );
    assert!(
        !japanese_but_kept(&english_entity.to_string()),
        "{english_entity}"
    );
    let read = mcp_in(&core, "en", "kind_read", json!({"target": "goal"})).unwrap();
    let said = read.to_string();
    assert!(said.contains("\"Status\""), "{said}");
    // The values a goal keeps stay as they are; how each is shown is said beside them.
    assert!(said.contains("未着手"), "{said}");
    assert!(said.contains("Not started"), "{said}");
    let guide = rpc_in(
        &core,
        "en",
        "tools/call",
        json!({"name": "guide", "arguments": {"about": "entity"}}),
    );
    let guide = guide["content"][0]["text"].as_str().unwrap().to_string();
    assert!(guide.contains("Record"), "{guide}");
    // The window reads the kinds in its own language.
    let detail = |lang: &str| {
        let read = op(
            &core,
            json!({"op": "project_detail", "project": project_id(&core), "lang": lang}),
        );
        read["result"]["kinds"]["cards"]
            .as_array()
            .unwrap_or_else(|| panic!("{read}"))
            .iter()
            .find(|k| k["kind"] == "entity")
            .unwrap()["label"]
            .clone()
    };
    assert_eq!(detail("ja"), "記録");
    assert_eq!(detail("en"), "Record");
}

#[test]
fn the_tools_and_the_instructions_are_said_in_the_mcp_answer_language() {
    let core = Core::open(script_store()).unwrap();
    for (lang, in_japanese) in [("ja", true), ("en", false)] {
        let started = rpc_in(
            &core,
            lang,
            "initialize",
            json!({"protocolVersion": "2025-06-18"}),
        );
        let instructions = started["instructions"].as_str().unwrap();
        assert_eq!(
            japanese_but_kept(instructions),
            in_japanese,
            "{instructions}"
        );
        let listed = rpc_in(&core, lang, "tools/list", json!({}));
        let tools = listed["tools"].as_array().unwrap();
        assert!(tools.len() > 40, "{listed}");
        for tool in tools {
            let said = format!("{} {}", tool["title"], tool["description"]);
            assert_eq!(japanese_but_kept(&said), in_japanese, "{tool}");
        }
        let card_create = tools.iter().find(|t| t["name"] == "card_create").unwrap();
        assert_eq!(
            japanese_but_kept(&card_create["inputSchema"].to_string()),
            in_japanese,
            "{card_create}"
        );
        let guide = rpc_in(
            &core,
            lang,
            "tools/call",
            json!({"name": "guide", "arguments": {"about": "card_create"}}),
        );
        let guide = guide["content"][0]["text"].as_str().unwrap();
        assert_eq!(japanese_but_kept(guide), in_japanese, "{guide}");
        // A tool of before is said by what to use now.
        let gone = op(
            &core,
            json!({"op": "mcp", "headers": headers(&core), "lang": lang, "message":
                {"jsonrpc": "2.0", "id": 2, "method": "tools/call",
                 "params": {"name": "get_schema", "arguments": {}}}}),
        );
        let gone = gone["result"]["body"]["error"]["message"]
            .as_str()
            .unwrap_or_else(|| panic!("{gone}"));
        assert_eq!(japanese(gone), in_japanese, "{gone}");
    }
}

/// The project's export language, as `project_read` says it.
fn export_language(core: &Core, lang: &str, project: &str) -> Value {
    let message = json!({
        "jsonrpc": "2.0", "id": 1, "method": "tools/call",
        "params": {"name": "project_read", "arguments": {"project": project}}
    });
    let reply = op(
        core,
        json!({"op": "mcp", "message": message, "headers": headers(core), "lang": lang}),
    );
    let text = reply["result"]["body"]["result"]["content"][0]["text"]
        .as_str()
        .unwrap_or_else(|| panic!("{reply}"))
        .to_string();
    serde_json::from_str::<Value>(&text).unwrap()["exportLanguage"].clone()
}

#[test]
fn the_export_language_is_set_and_read_and_a_new_project_takes_its_makers() {
    let core = Core::open(script_store()).unwrap();
    // A project of before writes out in Japanese.
    assert_eq!(export_language(&core, "en", PROJECT), "ja");
    let set = mcp_in(
        &core,
        "ja",
        "project_update",
        json!({"value": {"exportLanguage": "en"}}),
    )
    .unwrap();
    assert_eq!(set["exportLanguage"], "en", "{set}");
    assert_eq!(export_language(&core, "ja", PROJECT), "en");
    // One language for everyone: no `system`.
    let refused = mcp_in(
        &core,
        "en",
        "project_update",
        json!({"value": {"exportLanguage": "system"}}),
    )
    .unwrap_err();
    assert!(refused.contains("system"), "{refused}");
    // The window reads it with the projects, and sets it as MCP does.
    let listed = op(&core, json!({"op": "projects", "lang": "ja"}));
    let sample = listed["result"]["projects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == PROJECT)
        .unwrap()
        .clone();
    assert_eq!(sample["exportLanguage"], "en", "{sample}");
    // Carried in the project's file: a copy read from it writes out as it did.
    let file = op(
        &core,
        json!({"op": "project_file", "project": project_id(&core)}),
    )["result"]["text"]
        .clone();
    assert!(
        file.as_str()
            .unwrap()
            .contains("\"export_language\": \"en\""),
        "{file}"
    );
    let copy = op(
        &core,
        json!({"op": "project_file_read", "text": file, "landing": "copy", "name": "写し"}),
    );
    assert!(copy.get("error").is_none(), "{copy}");
    let listed = op(&core, json!({"op": "projects"}));
    let copied = listed["result"]["projects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == "写し")
        .unwrap()
        .clone();
    assert_eq!(copied["exportLanguage"], "en", "{copied}");
    let back = op(
        &core,
        json!({"op": "tool", "name": "project_update", "lang": "en",
            "arguments": {"project": PROJECT, "value": {"exportLanguage": "ja"}}}),
    );
    assert!(back.get("error").is_none(), "{back}");
    assert_eq!(export_language(&core, "en", PROJECT), "ja");
    // A new project writes out in the language of whoever made it: MCP's answers', the window's.
    for (lang, name) in [("en", "Made in English"), ("ja", "日本語で作った")] {
        let message = json!({
            "jsonrpc": "2.0", "id": 1, "method": "tools/call",
            "params": {"name": "project_create", "arguments": {"project": name}}
        });
        let made = op(
            &core,
            json!({"op": "mcp", "message": message, "headers": headers(&core), "lang": lang}),
        );
        assert_eq!(made["result"]["body"]["result"]["isError"], false, "{made}");
        assert_eq!(export_language(&core, "ja", name), lang);
    }
    let made = op(
        &core,
        json!({"op": "tool", "name": "project_create", "lang": "en",
            "arguments": {"project": "From the window"}}),
    );
    assert!(made.get("error").is_none(), "{made}");
    let listed = op(&core, json!({"op": "projects"}));
    let window = listed["result"]["projects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == "From the window")
        .unwrap()
        .clone();
    assert_eq!(window["exportLanguage"], "en", "{window}");
}

#[test]
fn an_english_project_writes_its_own_words_in_english_and_its_cards_as_written() {
    let core = Core::open(script_store()).unwrap();
    let page = |lang: &str| -> String {
        let read = mcp_in(
            &core,
            lang,
            "card_read",
            json!({"target": "Charts/Shop:Checkout", "value": {"as": "markdown"}}),
        )
        .unwrap();
        read["markdown"]
            .as_str()
            .unwrap_or_else(|| panic!("{read}"))
            .to_string()
    };
    let diagram = |lang: &str| -> String {
        let read = mcp_in(
            &core,
            lang,
            "card_read",
            json!({"target": "Charts/Shop:Checkout", "value": {"as": "diagram"}}),
        )
        .unwrap();
        read["mermaid"]
            .as_str()
            .unwrap_or_else(|| panic!("{read}"))
            .to_string()
    };
    let japanese_page = page("en");
    let japanese_diagram = diagram("en");
    assert!(japanese_page.contains("きっかけ"), "{japanese_page}");
    assert!(
        japanese_diagram.contains("Orders（store）"),
        "{japanese_diagram}"
    );
    ok(
        &core,
        "project_update",
        json!({"value": {"exportLanguage": "en"}}),
    );
    // Whoever reads it, in whatever language: the project's words.
    let english_page = page("ja");
    assert_eq!(english_page, page("en"));
    assert!(english_page.contains("Trigger"), "{english_page}");
    assert!(!english_page.contains("きっかけ"), "{english_page}");
    let english_diagram = diagram("ja");
    assert_eq!(english_diagram, diagram("en"));
    assert!(
        english_diagram.contains("Orders (store)"),
        "{english_diagram}"
    );
    assert!(!english_diagram.contains("（store）"), "{english_diagram}");
    // What the project wrote stays as written.
    for written in ["Charts/Shop:Checkout", "start"] {
        assert!(english_page.contains(written), "{written}: {english_page}");
    }
    // The files written out: the code's comments and the documents' headings in English.
    let files = op(
        &core,
        json!({"op": "export_files", "project": project_id(&core), "lang": "ja"}),
    );
    let all = files["result"].to_string();
    assert!(
        all.contains("Primary key") || all.contains("Required"),
        "{files}"
    );
    assert!(!all.contains("主キー"), "{files}");
    assert!(!all.contains("（図:"), "{files}");
    // Back in Japanese, the same pages as before.
    ok(
        &core,
        "project_update",
        json!({"value": {"exportLanguage": "ja"}}),
    );
    assert_eq!(page("en"), japanese_page);
    assert_eq!(diagram("en"), japanese_diagram);
}
