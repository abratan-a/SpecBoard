//! The MCP surface as the core answers it: the protocol through the core, the guides of the board's
//! kinds and of what the core added, and the complaints about arguments that do not fit
//! (`tests/fixtures/complaints.json`).

use crate::common;

use serde_json::{Map, Value, json};
use specboard_core::{Core, mcp};
use std::path::PathBuf;

/// The branch guide's word on a locked main now: the item the catalog has in its place.
fn lock_now() -> String {
    let guide = mcp::catalog()["branchGuide"].as_str().unwrap();
    let start = guide
        .find("main がロック中（project_read の locked: true）")
        .unwrap();
    let end = start + guide[start..].find("\n- ").unwrap();
    guide[start..end].to_string()
}

#[test]
fn arguments_that_do_not_fit_are_told_so() {
    let catalog = mcp::catalog();
    let cases: Value = serde_json::from_str(
        &std::fs::read_to_string(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/complaints.json"),
        )
        .unwrap(),
    )
    .unwrap();
    for case in cases.as_array().unwrap() {
        let name = case["tool"].as_str().unwrap();
        let tool = catalog["tools"]
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["name"] == name)
            .unwrap();
        let arguments: Map<String, Value> = case["arguments"].as_object().unwrap().clone();
        let got = mcp::complaints(&arguments, &tool["inputSchema"], "");
        let want: Vec<String> = case["complaints"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect();
        assert_eq!(want, got, "{name} への断る文");
    }
}

#[test]
fn the_protocol_answers_through_the_core() {
    let core = Core::open(common::script_store()).unwrap();
    // Every request carries an access key, the handshake too (`tests/access.rs` for those
    // that do not).
    let key = common::headers(&core)["authorization"].clone();
    let call = |message: Value, mut headers: Value| -> Value {
        headers["authorization"] = key.clone();
        let reply: Value = serde_json::from_str(
            &core.call(&json!({"op": "mcp", "message": message, "headers": headers}).to_string()),
        )
        .unwrap();
        reply["result"].clone()
    };
    let init = call(
        json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-06-18"}}),
        json!({}),
    );
    assert_eq!(init["status"], 200);
    assert_eq!(init["body"]["result"]["serverInfo"]["name"], "specboard");
    let listed = call(
        json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
        json!({}),
    );
    // The tools of `src/mcp/tools.json`, as many as there are: a tool made or let go changes this.
    assert_eq!(
        listed["body"]["result"]["tools"].as_array().unwrap().len(),
        44
    );
    let note = call(
        json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
        json!({}),
    );
    assert_eq!(note["status"], 202);
    assert!(note["body"].is_null());

    // A guide through tools/call: the text block, not an error.
    let guide = call(
        json!({"jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": {"name": "guide", "arguments": {"about": "branch"}}}),
        json!({}),
    );
    assert_eq!(guide["body"]["result"]["isError"], false);
    assert!(
        guide["body"]["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .starts_with("# branch")
    );
    // Arguments that do not fit are refused before anything runs.
    let refused = call(
        json!({"jsonrpc": "2.0", "id": 4, "method": "tools/call", "params": {"name": "card_read", "arguments": {"path": "x"}}}),
        json!({}),
    );
    assert_eq!(refused["body"]["result"]["isError"], true);
    let gone = call(
        json!({"jsonrpc": "2.0", "id": 5, "method": "tools/call", "params": {"name": "no_such_tool"}}),
        json!({}),
    );
    assert_eq!(
        gone["body"]["error"]["message"],
        "そのツールはありません: no_such_tool"
    );

    // The 2026 era: the headers have to say what the body says.
    let meta = json!({"_meta": {"io.modelcontextprotocol/protocolVersion": "2026-07-28"}});
    let discover = call(
        json!({"jsonrpc": "2.0", "id": 6, "method": "server/discover", "params": meta}),
        json!({"mcp-protocol-version": "2026-07-28", "mcp-method": "server/discover"}),
    );
    assert_eq!(discover["status"], 200);
    assert_eq!(discover["body"]["result"]["resultType"], "complete");
    let mismatch = call(
        json!({"jsonrpc": "2.0", "id": 7, "method": "tools/list", "params": meta}),
        json!({"mcp-protocol-version": "2026-07-28", "mcp-method": "server/discover"}),
    );
    assert_eq!(mismatch["status"], 400);
    assert_eq!(mismatch["body"]["error"]["code"], -32020);
}

/// The board's kinds say their fields by their names, the people's names beside them, and point at
/// how the board is kept; a custom card's name leads there.
#[test]
fn the_guides_tell_of_goal_and_task() {
    let json = |about: &str| match mcp::guide(Some(about)) {
        Ok(mcp::Output::Json(said)) => said,
        _ => panic!("{about} の guide がありません"),
    };
    for (about, kind) in [("goal", "goal"), ("task", "task"), ("custom_goal", "goal")] {
        let answer = json(about);
        assert_eq!(answer["kind"], kind);
        assert!(answer["more"].as_str().unwrap().contains("about: board"));
    }
    let goal = json("goal");
    let fields: Vec<&String> = goal["fields"].as_object().unwrap().keys().collect();
    assert_eq!(fields, ["status", "priority", "due", "workBranch", "MR"]);
    for (name, label) in [("status", "状態"), ("priority", "優先度"), ("due", "期限")] {
        assert!(
            goal["fields"][name].as_str().unwrap().contains(label),
            "{goal}"
        );
    }
    assert!(
        goal["fields"]["status"]
            .as_str()
            .unwrap()
            .contains("inProgress")
    );
    let task = json("task");
    let fields: Vec<&String> = task["fields"].as_object().unwrap().keys().collect();
    assert_eq!(fields, ["done"]);
    // How the board is kept: its own topic, said once.
    let board = mcp::guide(Some("board")).unwrap().text();
    assert_eq!(board, mcp::catalog()["boardGuide"].as_str().unwrap());
    assert!(board.contains("card_list"));
    assert!(board.contains(r#"include: ["places"]"#), "{board}");
    let kinds = mcp::guide(Some("kinds")).unwrap().text();
    assert!(kinds.contains("face で図の上の顔"), "{kinds}");
    // The board's guide and the glossary say the fields by their names.
    for text in ["boardGuide", "instructions", "glossary"] {
        let said = mcp::catalog()[text].as_str().unwrap();
        assert!(!said.contains("担当と"), "{text}: {said}");
        if text != "instructions" {
            assert!(said.contains("status"), "{text}");
        }
    }
    assert!(
        !mcp::catalog()["instructions"]
            .as_str()
            .unwrap()
            .contains("custom_goal")
    );
}

/// What the core added says itself in the topics of its tools and kinds.
#[test]
fn the_guides_tell_of_the_diagram_boxes() {
    let read = mcp::guide(Some("section_read")).unwrap().text();
    assert!(read.contains("include"), "{read}");
    let update = mcp::guide(Some("section_update")).unwrap().text();
    assert!(update.contains("places"), "{update}");
    // A goal's card map, and a card made in a goal.
    let read = mcp::guide(Some("card_read")).unwrap().text();
    assert!(read.contains("コンテンツマップ"), "{read}");
    let goal = mcp::guide(Some("goal")).unwrap().text();
    assert!(goal.contains("コンテンツマップ"), "{goal}");
    let create = mcp::guide(Some("card_create")).unwrap().text();
    assert!(create.contains("Goals:Launch.Design"), "{create}");
}

/// Where a card goes among its section's cards: said in `card_update`'s words and its value.
#[test]
fn the_guide_tells_of_a_cards_position() {
    let Ok(mcp::Output::Json(update)) = mcp::guide(Some("card_update")) else {
        panic!("card_update の guide がありません");
    };
    assert!(
        update["purpose"]
            .as_str()
            .unwrap()
            .contains("position で並べ")
    );
    assert!(
        update["value"]["card"]["position"]
            .as_str()
            .unwrap()
            .starts_with("integer")
    );
    // What a locked main lets through says it too.
    assert!(lock_now().contains("position"), "{}", lock_now());
}

/// `card_create` and `card_update` say the keys every kind takes and where the rest are; a kind by
/// its name says the keys only it takes.
#[test]
fn a_card_tool_says_the_keys_of_every_kind_and_a_kind_its_own() {
    let json = |about: &str| match mcp::guide(Some(about)) {
        Ok(mcp::Output::Json(said)) => said,
        _ => panic!("{about} の guide がありません"),
    };
    let keys = |table: &Value| -> Vec<String> {
        table
            .as_object()
            .map(|t| t.keys().filter(|k| *k != "same").cloned().collect())
            .unwrap_or_default()
    };
    for tool in ["card_create", "card_update"] {
        let said = json(tool);
        let card = keys(&said["value"]["card"]);
        for every in [
            "kind",
            "label",
            "description",
            "tags",
            "connections",
            "topLeft",
            "size",
        ] {
            assert!(card.iter().any(|k| k == every), "{tool}: {every}");
        }
        for only in [
            "fields", "values", "body", "rows", "diagram", "target", "of", "data",
        ] {
            assert!(!card.iter().any(|k| k == only), "{tool}: {only}");
        }
        for form in ["field", "row", "node"] {
            assert!(said["value"][form].is_string(), "{tool}: {form}");
        }
    }
    let entity = json("entity");
    assert_eq!(
        keys(&entity["keys"]["card_create"]),
        ["fields", "absentFields", "discriminator", "face"]
    );
    let formatdoc = json("formatdoc");
    assert_eq!(keys(&formatdoc["keys"]["card_create"]), ["rows"]);
    assert_eq!(keys(&formatdoc["keys"]["card_update"]), ["view"]);
    assert!(formatdoc["keys"]["card_create"]["rows"]["heading"].is_string());
    assert!(
        formatdoc["row"]["card_create"]["same"]
            .as_str()
            .unwrap()
            .contains("keys.card_create.rows"),
        "{formatdoc}"
    );
    // A store takes nothing of its own: what it holds are its lines.
    assert!(json("store").get("keys").is_none());
    let goal = json("goal");
    assert_eq!(keys(&goal["keys"]["card_create"]), ["body"]);
    assert_eq!(keys(&goal["keys"]["card_update"]), ["places"]);
    // A node: its kind's keys, and every node's.
    let begin = json("begin");
    let own = keys(&begin["keys"]["card_create"]);
    for k in ["trigger", "carries"] {
        assert!(own.iter().any(|n| n == k), "{k}: {own:?}");
    }
    for k in ["steps", "outcome", "join", "kind"] {
        assert!(!own.iter().any(|n| n == k), "{k}: {own:?}");
    }
    let every = keys(&begin["nodeKeys"]["card_create"]);
    for k in ["kind", "connections", "topLeft"] {
        assert!(every.iter().any(|n| n == k), "{k}: {every:?}");
    }
    assert_eq!(keys(&json("ghost")["keys"]["card_update"]), ["target"]);
}

#[test]
fn what_each_do_of_a_process_row_means_is_said_in_the_tool_and_the_process_guide() {
    // In the tools' shapes of a process row, card_create's and card_update's alike.
    let tools: Value = serde_json::from_str(include_str!("../src/mcp/tools.json")).unwrap();
    let mut said = Vec::new();
    for tool in tools.as_array().unwrap() {
        for form in tool["forms"].as_array().into_iter().flatten() {
            if let Some(about) =
                form["spec"]["properties"]["steps"]["items"]["properties"]["do"]["description"]
                    .as_str()
            {
                said.push(about.to_string());
            }
        }
    }
    assert_eq!(said.len(), 2, "{said:?}");
    let words = [
        "署名を確かめる",
        "業務の確かめではない",
        "call（プロジェクトに無いプログラムを呼んで待つ）",
        "compute（業務の検証・計算のような中だけの処理",
    ];
    for about in &said {
        for word in words {
            assert!(about.contains(word), "{word}: {about}");
        }
    }
    // And once in the process's topic.
    let process = mcp::guide(Some("process")).unwrap().text();
    for word in words {
        assert_eq!(process.matches(word).count(), 1, "{word}: {process}");
    }
}

/// The glossary's legend says each built-in kind of line as its definition does (its label and its
/// one-line description), and nothing more; the lines' guide names every one.
#[test]
fn the_glossarys_legend_says_each_line_as_its_definition() {
    let builtin: Value = serde_json::from_str(
        &std::fs::read_to_string(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/kinds/builtin.json"),
        )
        .unwrap(),
    )
    .unwrap();
    let glossary = mcp::catalog()["glossary"].as_str().unwrap();
    let start = glossary.rfind("| `embed` |").expect("凡例の表が無い");
    let rows: Vec<&str> = glossary[start..]
        .lines()
        .take_while(|l| l.starts_with('|'))
        .collect();
    let lines = builtin["lines"].as_array().unwrap();
    let said: Vec<String> = lines
        .iter()
        .map(|l| {
            format!(
                "| `{}` | {}: {} |",
                l["line"].as_str().unwrap(),
                l["label"].as_str().unwrap(),
                l["description"]
                    .as_str()
                    .unwrap()
                    .replace('|', "\\|")
                    // A glossary writes how a body names a place in code, so a body that holds it
                    // does not read it as a place.
                    .replace("[[ ]]", "`[[ ]]`")
            )
        })
        .collect();
    assert_eq!(rows, said, "凡例は線の種類の定義のとおり");
    let guide = mcp::catalog()["linesGuide"].as_str().unwrap();
    for line in lines {
        let name = line["line"].as_str().unwrap();
        assert!(guide.contains(name), "linesGuide に {name} が無い");
    }
    assert!(!guide.contains("同じ木の中"), "{guide}");
}
