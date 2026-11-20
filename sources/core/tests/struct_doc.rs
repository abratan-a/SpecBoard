//! The built-in kind of card that holds a document of items and rows is `structdoc` (a document
//! of structure), and the built-in kinds are labelled as the screen names them
//! (`specboard://SpecBoard/Docs/Design:CardKinds`). A store of before (`core_0031_struct_doc`) and
//! a project file before version 35 are moved to it once; the name of before written through MCP
//! is refused with the name of now.

use crate::common;

use common::{PROJECT, headers, ok, op, project_id, refused, script_store};
use rusqlite::Connection;
use serde_json::{Value, json};
use specboard_core::Core;

const MARK: &str = "core_0031_struct_doc";

/// The kind's name of before and of now, as a document holds them: of one length, so a piece of
/// the history keeps its length when one is written back as the other.
const BEFORE: &str = "\"kind\":\"formatdoc\"";
const NOW: &str = "\"kind\":\"structdoc\"";

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

/// A document of 4,000 lines that differ: long enough that the history keeps it in pieces.
fn long_text(word: &str) -> String {
    (1..=4000)
        .map(|n| format!("{n} 行目の{word}。長くするための文 {}", n * 7919 % 1000))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The first 16 bytes of the SHA-256 of `data`: what a piece of the history is found by.
fn digest(data: &[u8]) -> Vec<u8> {
    use sha2::{Digest, Sha256};
    Sha256::digest(data)[..16].to_vec()
}

/// How many places of the store still name the kind of before: the cards' documents, bases and
/// kind, what branches froze, the history's rows and pieces, the projects' kinds and exports.
fn named_before(conn: &Connection) -> i64 {
    let count = |query: &str| -> i64 { conn.query_row(query, [], |r| r.get(0)).unwrap() };
    count(
        "SELECT count(*) FROM card_docs WHERE kind = 'formatdoc' OR instr(ifnull(doc, ''), 'formatdoc') > 0 OR instr(ifnull(base, ''), 'formatdoc') > 0",
    ) + count("SELECT count(*) FROM branch_frozen WHERE instr(ifnull(doc, ''), 'formatdoc') > 0")
        + count(
            "SELECT count(*) FROM commit_rows WHERE typeof(before) = 'text' AND instr(before, 'formatdoc') > 0",
        )
        + count(
            "SELECT count(*) FROM history_chunks WHERE instr(CAST(data AS TEXT), 'formatdoc') > 0",
        )
        + count(
            "SELECT count(*) FROM projects WHERE instr(ifnull(kinds, ''), 'formatdoc') > 0 OR instr(ifnull(exports, ''), 'formatdoc') > 0",
        )
}

/// The kind's name of before and of now as a history row keeps a document: a text inside its row.
const BEFORE_KEPT: &str = "\\\"kind\\\":\\\"formatdoc\\\"";
const NOW_KEPT: &str = "\\\"kind\\\":\\\"structdoc\\\"";

/// `data` with the kind by its name of before wherever it has it by its name of now, as a row or as
/// a document inside a row: of one length.
fn written_before(data: &[u8]) -> Vec<u8> {
    let mut out = data.to_vec();
    for (now, before) in [(NOW, BEFORE), (NOW_KEPT, BEFORE_KEPT)] {
        let (now, before) = (now.as_bytes(), before.as_bytes());
        let mut at = 0;
        while at + now.len() <= out.len() {
            if &out[at..at + now.len()] == now {
                out[at..at + now.len()].copy_from_slice(before);
                at += now.len();
            } else {
                at += 1;
            }
        }
    }
    out
}

/// The store written back as a store of before had it: the kind by its name of before wherever
/// one is kept — the pieces of the history in place, of one length, with their digests — and the
/// mark gone.
fn back_to_before(place: &std::path::Path) {
    let conn = Connection::open(place).unwrap();
    let main_before = main_docs(&conn);
    let requests_before = open_requests(&conn);
    // Written by the kinds of before: a document moved is written anew in the kinds of now.
    let v_now = format!("\"v\":{}", specboard_core::kinds::builtin().version);
    let v_before = format!("\"v\":{}", specboard_core::kinds::builtin().version - 1);
    conn.execute_batch(&format!(
        "UPDATE card_docs SET kind = 'formatdoc' WHERE kind = 'structdoc';
         UPDATE card_docs SET doc = replace(doc, '{NOW}', '{BEFORE}'), base = replace(base, '{NOW}', '{BEFORE}');
         UPDATE card_docs SET doc = replace(doc, '{v_now}', '{v_before}') WHERE instr(doc, '{BEFORE}') > 0;
         UPDATE card_docs SET base = replace(base, '{v_now}', '{v_before}') WHERE instr(base, '{BEFORE}') > 0;
         UPDATE branch_frozen SET doc = replace(doc, '{NOW}', '{BEFORE}');
         UPDATE branch_frozen SET doc = replace(doc, '{v_now}', '{v_before}') WHERE instr(doc, '{BEFORE}') > 0;
         UPDATE commit_rows SET before = replace(replace(before, '{NOW}', '{BEFORE}'), '{NOW_KEPT}', '{BEFORE_KEPT}') WHERE typeof(before) = 'text';
         UPDATE projects SET kinds = replace(kinds, '\"structdoc\"', '\"formatdoc\"'), exports = replace(exports, '\"structdoc\"', '\"formatdoc\"');
         DELETE FROM _migrations WHERE name = '{MARK}';"
    ))
    .unwrap();
    let pieces: Vec<(i64, Vec<u8>)> = {
        let mut stmt = conn.prepare("SELECT id, data FROM history_chunks").unwrap();
        stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    };
    for (id, data) in pieces {
        // A piece may end inside a character: replaced as bytes.
        let before = written_before(&data);
        if before != data {
            assert_eq!(before.len(), data.len());
            conn.execute(
                "UPDATE history_chunks SET data = ?, hash = ? WHERE id = ?",
                rusqlite::params![before, digest(&before), id],
            )
            .unwrap();
        }
    }
    // What a branch remembers of main's cards and what an open request was tried on: as the store
    // of before had them, of the documents of before.
    let main_after = main_docs(&conn);
    let prints: Vec<(String, String, String)> = {
        let mut stmt = conn
            .prepare("SELECT branch_id, card_id, print FROM branch_prints")
            .unwrap();
        stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    };
    for (branch, card, print) in prints {
        if let (Some(was), Some(now)) = (main_before.get(&card), main_after.get(&card))
            && specboard_core::schema::print(was) == print
        {
            conn.execute(
                "UPDATE branch_prints SET print = ? WHERE branch_id = ? AND card_id = ?",
                rusqlite::params![specboard_core::schema::print(now), branch, card],
            )
            .unwrap();
        }
    }
    let requests_after = open_requests(&conn);
    for (id, (stored, was)) in &requests_before {
        if stored == was {
            conn.execute(
                "UPDATE merge_requests SET print = ? WHERE id = ?",
                rusqlite::params![requests_after[id].1, id],
            )
            .unwrap();
        }
    }
    assert!(named_before(&conn) > 0, "前の名前に書き戻せていない");
}

/// main's card documents, by id.
fn main_docs(conn: &Connection) -> std::collections::HashMap<String, String> {
    let mut stmt = conn
        .prepare("SELECT id, doc FROM card_docs WHERE branch_id = '' AND doc IS NOT NULL")
        .unwrap();
    stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

/// The open requests to merge, by id: the print each keeps, and the print of their branch and main
/// as they are (`requestPrint`: each tree's sections' and cards' rows, digested).
fn open_requests(conn: &Connection) -> std::collections::HashMap<String, (String, String)> {
    use sha2::{Digest, Sha256};
    let tree = |project: &str, tree: &str| -> String {
        let mut hash = Sha256::new();
        for table in ["section_docs", "card_docs"] {
            hash.update(format!("\u{2}{table}\u{1}{tree}").as_bytes());
            let mut stmt = conn
                .prepare(&format!(
                    "SELECT id, removed, ifnull(doc, ''), ifnull(base, '') FROM {table} WHERE project_id = ? AND branch_id = ? ORDER BY id"
                ))
                .unwrap();
            let rows: Vec<String> = stmt
                .query_map([project, tree], |r| {
                    Ok(format!(
                        "{}\u{1}{}\u{1}{}\u{1}{}",
                        r.get::<_, String>(0)?,
                        r.get::<_, i64>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, String>(3)?
                    ))
                })
                .unwrap()
                .collect::<Result<_, _>>()
                .unwrap();
            for row in rows {
                hash.update(format!("{row}\u{0}").as_bytes());
            }
        }
        hash.finalize().iter().map(|b| format!("{b:02x}")).collect()
    };
    let open: Vec<(String, String, String, String)> = {
        let mut stmt = conn
            .prepare(
                "SELECT id, project_id, branch_id, print FROM merge_requests WHERE state = 'open'",
            )
            .unwrap();
        stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    };
    open.into_iter()
        .map(|(id, project, branch, print)| {
            let now = format!("{}.{}", tree(&project, &branch), tree(&project, ""));
            (id, (print, now))
        })
        .collect()
}

/// A tree as the window names it: main by none, a branch by its id.
fn tree_id(core: &Core, branch: &str) -> String {
    if branch == "main" {
        return String::new();
    }
    Connection::open(core.place())
        .unwrap()
        .query_row("SELECT id FROM branches WHERE name = ?", [branch], |r| {
            r.get(0)
        })
        .unwrap()
}

/// The newest lines of a tree's history, as the window reads them.
fn lines_of(core: &Core, branch: &str) -> Vec<String> {
    let read = op(
        core,
        json!({"op": "commits", "project": project_id(core), "branch": tree_id(core, branch)}),
    );
    read["result"]["commits"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|c| c["lines"].as_array().cloned().unwrap_or_default())
        .filter_map(|l| l["summary"].as_str().map(str::to_string))
        .collect()
}

#[test]
fn the_kinds_are_named_and_labelled_as_now() {
    let core = Core::open(script_store()).unwrap();
    let labels = |lang: &str| -> Vec<(String, String)> {
        let read = |name: &str| -> String {
            let kind = mcp_in(&core, lang, "kind_read", json!({"target": name}))
                .unwrap_or_else(|e| panic!("{name}: {e}"));
            kind["label"].as_str().unwrap_or_default().to_string()
        };
        [
            "flow",
            "entity",
            "value",
            "enum",
            "note",
            "structdoc",
            "freedoc",
            "stackdoc",
            "frame",
        ]
        .iter()
        .map(|name| (name.to_string(), read(name)))
        .collect()
    };
    let pairs = |list: &[(&str, &str)]| -> Vec<(String, String)> {
        list.iter()
            .map(|(k, l)| (k.to_string(), l.to_string()))
            .collect()
    };
    assert_eq!(
        labels("ja"),
        pairs(&[
            ("flow", "フロー"),
            ("entity", "エンティティ"),
            ("value", "値オブジェクト"),
            ("enum", "列挙型"),
            ("note", "付箋"),
            ("structdoc", "構造ドキュメント"),
            ("freedoc", "フリードキュメント"),
            ("stackdoc", "スタックドキュメント"),
            ("frame", "フレーム"),
        ])
    );
    assert_eq!(
        labels("en"),
        pairs(&[
            ("flow", "Flow"),
            ("entity", "Entity"),
            ("value", "Value object"),
            ("enum", "Enumeration"),
            ("note", "Sticky note"),
            ("structdoc", "Structured document"),
            ("freedoc", "Free document"),
            ("stackdoc", "Stacked document"),
            ("frame", "Frame"),
        ])
    );
    // The parent of entities and values, which no card is made of: named in the definitions.
    let record = specboard_core::kinds::builtin().get("record").unwrap();
    assert!(record.is_abstract);
    assert_eq!(record.label, "レコードの形");
    let listed = ok(&core, "kind_list", json!({}));
    let cards = listed["cards"].as_array().unwrap();
    let structdoc = cards
        .iter()
        .find(|c| c["kind"] == "structdoc")
        .unwrap_or_else(|| panic!("{listed}"));
    assert!(
        structdoc["in"]
            .as_array()
            .unwrap()
            .contains(&json!("document")),
        "{structdoc}"
    );
    assert!(cards.iter().all(|c| c["kind"] != "formatdoc"), "{listed}");
}

#[test]
fn a_kind_of_before_is_refused_with_its_name_of_now() {
    let core = Core::open(script_store()).unwrap();
    let says_now = |said: &str| {
        assert!(
            said.contains("「formatdoc」は前の名前です。今は「structdoc」で書きます"),
            "{said}"
        );
    };
    says_now(&refused(
        &core,
        "card_create",
        json!({"target": "Docs:Old", "value": {"kind": "formatdoc"}}),
    ));
    says_now(&refused(
        &core,
        "card_list",
        json!({"value": {"kind": "formatdoc"}}),
    ));
    says_now(&refused(&core, "kind_read", json!({"target": "formatdoc"})));
    says_now(&refused(
        &core,
        "kind_create",
        json!({"target": "papers", "value": {"of": "section", "label": "紙", "holds": ["formatdoc", "note"]}}),
    ));
    let allowed = op(
        &core,
        json!({"op": "mcp_writes_set", "project": project_id(&core), "group": "export", "state": "allow"}),
    );
    assert!(allowed.get("error").is_none(), "{allowed}");
    says_now(&refused(
        &core,
        "export_create",
        json!({"target": "old", "value": {"entries": [{"from": "Docs", "to": "docs/", "kinds": ["formatdoc"]}]}}),
    ));
    says_now(&refused(
        &core,
        "kind_create",
        json!({"target": "sheet", "value": {"of": "card", "label": "シート", "extends": "formatdoc"}}),
    ));
    says_now(&refused(
        &core,
        "kind_create",
        json!({"target": "formatdoc", "value": {"of": "card", "label": "古い"}}),
    ));
    says_now(&refused(
        &core,
        "kind_create",
        json!({"target": "binder", "value": {"of": "card", "label": "綴り", "holds": ["formatdoc"], "opens": "card-map"}}),
    ));
    says_now(&refused(
        &core,
        "kind_create",
        json!({"target": "sheet", "value": {"of": "card", "label": "シート", "fields": [{"name": "doc", "fieldtype": "card", "label": "文書", "kinds": ["formatdoc"]}]}}),
    ));
    says_now(&refused(
        &core,
        "kind_update",
        json!({"target": "formatdoc", "value": {"context": {"use_when": "設計書"}}}),
    ));
    says_now(&refused(
        &core,
        "kind_delete",
        json!({"target": "formatdoc"}),
    ));
    says_now(&refused(&core, "guide", json!({"about": "formatdoc"})));
    says_now(&refused(
        &core,
        "guide",
        json!({"about": ["formatdoc", "nonsense"]}),
    ));
    // Nothing of it was written.
    let gone = refused(&core, "card_read", json!({"target": "Docs:Old"}));
    assert!(!gone.is_empty());
    let kinds = ok(&core, "kind_list", json!({}));
    assert!(
        kinds["sections"]
            .as_array()
            .unwrap()
            .iter()
            .all(|s| s["section"] != "papers"),
        "{kinds}"
    );
    let exports = ok(&core, "export_read", json!({"targetdir": "[default]"}));
    assert!(!exports.to_string().contains("\"old\""), "{exports}");
}

/// The sample with a long structured document whose history keeps its body in pieces, a short one
/// changed on main and on a branch, a kind of section of the project's own holding structured
/// documents and an export of them: written back as a store of before had it, then opened.
#[test]
fn a_store_of_before_has_its_formatdocs_named_structdoc_once() {
    let place = common::place("struct-doc.sqlite");
    std::fs::copy(script_store(), &place).unwrap();
    let core = Core::open(place.clone()).unwrap();
    let long = long_text("前の文");
    ok(
        &core,
        "card_create",
        json!({"target": "Docs:Long", "value": {"kind": "structdoc", "rows": [{"name": "a", "kind": "text", "body": long}]}}),
    );
    // Kept by the history in pieces: the long rows as they were.
    let changed = ok(
        &core,
        "card_update",
        json!({"target": "Docs:Long.a", "value": {"body": long_text("今の文")}}),
    );
    let undo_seq = changed["seq"].as_i64().unwrap_or_else(|| {
        let commits = op(
            &core,
            json!({"op": "commits", "project": project_id(&core), "limit": 1}),
        );
        commits["result"]["commits"][0]["seq"].as_i64().unwrap()
    });
    ok(
        &core,
        "card_update",
        json!({"target": "Docs:Spec", "value": {"description": "main で直した"}}),
    );
    ok(
        &core,
        "card_create",
        json!({"target": "Docs:Third", "value": {"kind": "structdoc", "description": "分ける前"}}),
    );
    ok(&core, "branch_create", json!({"target": "work"}));
    ok(
        &core,
        "card_update",
        json!({"branch": "work", "target": "Docs:Spec", "value": {"label": "branch の仕様"}}),
    );
    // Written on main after the branch was made: the branch keeps it frozen as it was.
    ok(
        &core,
        "card_update",
        json!({"target": "Docs:Third", "value": {"description": "分けたあと"}}),
    );
    common::set_kinds(
        &core,
        PROJECT,
        json!({
            "sections": [{"section": "papers", "label": "紙", "holds": ["structdoc", "note"]}],
            "cards": [
                {"kind": "binder", "label": "綴り", "holds": ["structdoc"], "opens": "card-map"},
                {"kind": "sheet", "label": "シート", "fields": [{"name": "doc", "fieldtype": "card", "label": "文書", "kinds": ["structdoc"]}]}
            ]
        }),
    )
    .unwrap();
    let allowed = op(
        &core,
        json!({"op": "mcp_writes_set", "project": project_id(&core), "group": "export", "state": "allow"}),
    );
    assert!(allowed.get("error").is_none(), "{allowed}");
    ok(
        &core,
        "export_create",
        json!({"target": "specs", "value": {"entries": [{"from": "Docs", "to": "specs/", "kinds": ["structdoc"]}]}}),
    );
    // main locked: the branch merged in is a request, open on what it was tried on.
    let locked = op(
        &core,
        json!({"op": "main_lock", "project": project_id(&core), "on": true}),
    );
    assert!(locked.get("error").is_none(), "{locked}");
    ok(
        &core,
        "branch_merge",
        json!({"target": "work", "value": {"title": "仕様を直す"}}),
    );
    assert_eq!(
        ok(&core, "mr_read", json!({"target": "1"}))["state"],
        "open"
    );
    core.close();
    drop(core);
    back_to_before(&place);
    {
        // The long rows' pieces name the kind of before too.
        let conn = Connection::open(&place).unwrap();
        let pieces: i64 = conn
            .query_row(
                "SELECT count(*) FROM history_chunks WHERE instr(CAST(data AS TEXT), 'formatdoc') > 0",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(pieces > 0, "履歴の pieces を前の名前に書き戻せていない");
    }

    let core = Core::open(place.clone()).unwrap();
    for branch in ["main", "work"] {
        let spec = ok(
            &core,
            "card_read",
            json!({"branch": branch, "target": "Docs:Spec"}),
        );
        assert_eq!(spec["kind"], "structdoc", "{branch}: {spec}");
    }
    let said = |branch: &str| {
        lines_of(&core, branch)
            .into_iter()
            .filter(|l| l.contains("structdoc"))
            .collect::<Vec<_>>()
    };
    let main_said = said("main");
    assert_eq!(main_said.len(), 1, "{main_said:?}");
    assert!(main_said[0].contains("Spec"), "{main_said:?}");
    assert_eq!(said("work").len(), 1, "{:?}", lines_of(&core, "work"));
    let kinds = ok(&core, "kind_read", json!({"target": "papers"}));
    assert_eq!(kinds["holds"], json!(["structdoc", "note"]), "{kinds}");
    let binder = ok(&core, "kind_read", json!({"target": "binder"}));
    assert_eq!(binder["holds"], json!(["structdoc"]), "{binder}");
    let sheet = ok(&core, "kind_read", json!({"target": "sheet"}));
    assert!(
        sheet.to_string().contains("\"kinds\":[\"structdoc\"]"),
        "{sheet}"
    );
    // The project's kinds are as they may be written: one is changed.
    ok(
        &core,
        "kind_update",
        json!({"target": "papers", "value": {"label": "紙の束"}}),
    );
    // The branch reads main's document as it was when the branch was made, by the name of now.
    let third = ok(
        &core,
        "card_read",
        json!({"branch": "work", "target": "Docs:Third"}),
    );
    assert_eq!(third["kind"], "structdoc", "{third}");
    assert_eq!(third["description"], "分ける前", "{third}");
    // What the branch remembers of main's Spec is of main's document as it is now, and the request
    // is still on what it was tried on.
    {
        let conn = Connection::open(&place).unwrap();
        let (spec, doc): (String, String) = conn
            .query_row(
                "SELECT id, doc FROM card_docs WHERE branch_id = '' AND name = 'Spec'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        let remembered: String = conn
            .query_row(
                "SELECT print FROM branch_prints WHERE branch_id = ? AND card_id = ?",
                [tree_id(&core, "work"), spec],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(remembered, specboard_core::schema::print(&doc));
    }
    assert_eq!(
        ok(&core, "mr_read", json!({"target": "1"}))["state"],
        "open"
    );
    // The history's long rows read as structured documents: undone, the body of before comes back.
    let undone = ok(
        &core,
        "branch_revert",
        json!({"target": "main", "value": {"undo": undo_seq}}),
    );
    assert_eq!(undone["undone"], undo_seq, "{undone}");
    // main's undo lands on a branch that a merge takes in.
    let landed = undone["made"].as_str().unwrap_or("main").to_string();
    // Read from the store whole, as MCP answers a long body in part.
    let (kind, doc): (String, String) = Connection::open(&place)
        .unwrap()
        .query_row(
            "SELECT kind, doc FROM card_docs WHERE branch_id = ? AND name = 'Long' AND removed = 0",
            [tree_id(&core, &landed)],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(kind, "structdoc");
    let kept = serde_json::to_string(&long).unwrap();
    assert!(
        doc.contains(&kept[1..kept.len() - 1]),
        "本文が前のものに戻っていない"
    );
    let card = ok(
        &core,
        "card_read",
        json!({"branch": landed, "target": "Docs:Long"}),
    );
    assert_eq!(card["kind"], "structdoc", "{card}");
    let commits_now = lines_of(&core, "main").len();
    core.close();
    drop(core);
    {
        let conn = Connection::open(&place).unwrap();
        assert_eq!(named_before(&conn), 0, "前の名前が残っている");
        let marked: i64 = conn
            .query_row(
                "SELECT count(*) FROM _migrations WHERE name = ?",
                [MARK],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(marked, 1);
    }
    let mut kept = place.clone().into_os_string();
    kept.push(".before-upgrade-from-core_0030_history_chunks.db");
    assert!(
        std::path::Path::new(&kept).exists(),
        "移す前のバックアップが無い"
    );
    // Opened again, nothing more.
    let core = Core::open(place.clone()).unwrap();
    assert_eq!(lines_of(&core, "main").len(), commits_now);
}

/// A project's own kind of card named as the built-in one is now, in a store of before: it steps
/// aside as `structdoc_project`, its cards with it; the built-in one of before is `structdoc`.
#[test]
fn a_projects_own_structdoc_steps_aside() {
    let place = common::place("struct-doc-own.sqlite");
    std::fs::copy(script_store(), &place).unwrap();
    let core = Core::open(place.clone()).unwrap();
    common::set_kinds(
        &core,
        PROJECT,
        json!({"cards": [{"kind": "papers", "label": "紙", "fields": [{"name": "body", "fieldtype": "markdown", "label": "本文"}]}]}),
    )
    .unwrap();
    ok(
        &core,
        "card_create",
        json!({"target": "Free:Paper", "value": {"kind": "papers", "body": "紙の本文"}}),
    );
    core.close();
    drop(core);
    back_to_before(&place);
    {
        // The project's own kind named as the built-in one is now, and its card.
        let conn = Connection::open(&place).unwrap();
        conn.execute_batch(
            "UPDATE projects SET kinds = replace(kinds, '\"kind\":\"papers\"', '\"kind\":\"structdoc\"');
             UPDATE card_docs SET kind = 'structdoc', doc = replace(doc, '\"kind\":\"papers\"', '\"kind\":\"structdoc\"') WHERE kind = 'papers';",
        )
        .unwrap();
    }
    let core = Core::open(place.clone()).unwrap();
    let paper = ok(&core, "card_read", json!({"target": "Free:Paper"}));
    assert_eq!(paper["kind"], "structdoc_project", "{paper}");
    assert_eq!(paper["body"], "紙の本文", "{paper}");
    let spec = ok(&core, "card_read", json!({"target": "Docs:Spec"}));
    assert_eq!(spec["kind"], "structdoc", "{spec}");
    let own = ok(&core, "kind_read", json!({"target": "structdoc_project"}));
    assert_eq!(own["label"], "紙", "{own}");
}

/// The sample's file as version 34 wrote it: its structured document by the name of before, and a
/// kind of section of its own and an export naming it so.
#[test]
fn a_file_of_before_reads_its_formatdocs_as_structdoc() {
    let core = Core::open(script_store()).unwrap();
    common::set_kinds(
        &core,
        PROJECT,
        json!({
            "sections": [{"section": "papers", "label": "紙", "holds": ["structdoc", "note"]}],
            "cards": [{"kind": "sheetx", "label": "シート", "fields": [{"name": "body", "fieldtype": "markdown", "label": "本文"}]}]
        }),
    )
    .unwrap();
    ok(
        &core,
        "card_create",
        json!({"target": "Free:Sheet", "value": {"kind": "sheetx", "body": "シートの本文"}}),
    );
    let allowed = op(
        &core,
        json!({"op": "mcp_writes_set", "project": project_id(&core), "group": "export", "state": "allow"}),
    );
    assert!(allowed.get("error").is_none(), "{allowed}");
    ok(
        &core,
        "export_create",
        json!({"target": "specs", "value": {"entries": [{"from": "Docs", "to": "specs/", "kinds": ["structdoc"]}]}}),
    );
    let written = op(
        &core,
        json!({"op": "project_file", "project": project_id(&core)}),
    )["result"]["text"]
        .as_str()
        .unwrap()
        .to_string();
    let mut file: Value = serde_json::from_str(&written).unwrap();
    assert_eq!(file["version"], 35, "今のファイルのバージョン");
    assert!(
        file.to_string().contains(NOW),
        "今のファイルは structdoc を書く"
    );
    file["version"] = json!(34);
    let text = file
        .to_string()
        .replace(NOW, BEFORE)
        .replace("\"structdoc\"", "\"formatdoc\"")
        // A kind of the project's own named as the built-in one is now.
        .replace("\"sheetx\"", "\"structdoc\"");
    let place = common::place("struct-doc-file.sqlite");
    let fresh = Core::create(&place).unwrap();
    let read = op(&fresh, json!({"op": "project_file_read", "text": text}));
    assert!(read.get("error").is_none(), "{read}");
    let spec = ok(&fresh, "card_read", json!({"target": "Docs:Spec"}));
    assert_eq!(spec["kind"], "structdoc", "{spec}");
    let kinds = ok(&fresh, "kind_read", json!({"target": "papers"}));
    assert_eq!(kinds["holds"], json!(["structdoc", "note"]), "{kinds}");
    let sheet = ok(&fresh, "card_read", json!({"target": "Free:Sheet"}));
    assert_eq!(sheet["kind"], "structdoc_project", "{sheet}");
    assert_eq!(sheet["body"], "シートの本文", "{sheet}");
    let conn = Connection::open(&place).unwrap();
    assert_eq!(named_before(&conn), 0, "前の名前が残っている");
}
