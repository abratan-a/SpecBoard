//! A project's own kinds of card: defined with the kinds' tools (`kind_create`, `kind_update`,
//! `kind_delete`), tried without writing (`dryRun`), made, read, changed and listed as the built-in kinds
//! are, in the sections that hold them and in containers of the project's own; what a change to a definition
//! leaves behind kept (`strayFields`) and let go of, a rename taking the values along (`renames`), a kind in
//! use kept, and a card that no longer fits said by the check (`cardProblems`).
//!
//! Over the sample project (`見本`): a kind `risk` (a choice `severity`, a person `owner`,
//! rows `steps` with a name and a check) held by a section kind of the project's own `risks`.

use crate::common;

use common::{call, checked, ok, op, project_id, refused, script_store};
use serde_json::{Value, json};
use specboard_core::Core;

const PROJECT: &str = "見本";

fn read(core: &Core, target: &str) -> Value {
    ok(core, "card_read", json!({"target": target}))
}

/// The kind `risk`, with `severity` named `name`.
fn risk(severity: &str) -> Value {
    json!({
        "kind": "risk", "label": "リスク", "color": "#C2410C", "header": [severity],
        "connectable": true,
        "context": { "purpose": "起こりうる困りごと", "use_when": "設計の危うい所を残すとき" },
        "fields": [
            { "name": severity, "label": "深刻さ", "fieldtype": "radio", "options": ["高", "中", "低"], "default": "中" },
            { "name": "owner", "label": "担当", "fieldtype": "person" },
            { "name": "due", "fieldtype": "date" }
        ],
        "rows": [
            { "name": "steps", "label": "対策", "fields": [
                { "name": "name", "fieldtype": "text", "pattern": "identifier", "required": true },
                { "name": "done", "fieldtype": "checkbox" }
            ] }
        ]
    })
}

fn risks_section() -> Value {
    json!({ "section": "risks", "label": "リスク", "holds": ["risk", "note"] })
}

/// A section of `kind` put directly under main's root (and every branch's), last: what is there is
/// the project's (`project_tree`).
fn under_main_root(core: &Core, kind: &str, name: &str) {
    let read = ok(core, "project_read", json!({}));
    let mut tree = read["projectTree"].clone();
    tree["main_sections"]
        .as_array_mut()
        .unwrap()
        .push(json!({"type": kind, "name": name}));
    ok(
        core,
        "project_update",
        json!({"value": {"project_tree": tree}}),
    );
}

/// The project given `risk` and a section kind that holds it, and a section `Risks` of it.
fn defined(core: &Core) {
    common::set_kinds(
        core,
        PROJECT,
        json!({"cards": [risk("severity")], "sections": [risks_section()]}),
    )
    .unwrap();
    under_main_root(core, "risks", "Risks");
}

/// The section of the built-in kind `free`, by its path.
fn free_section(core: &Core) -> String {
    let listed = ok(core, "section_list", json!({}));
    listed["sections"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["type"] == "free")
        .and_then(|s| s["section"].as_str())
        .unwrap()
        .to_string()
}

#[test]
fn a_kind_of_the_projects_own_is_checked_without_writing_and_then_written() {
    let core = Core::open(script_store()).unwrap();
    // Tried as written, every problem said, nothing kept.
    let mut wrong = risk("severity");
    wrong["semantics"] = json!({"editor": "x"});
    wrong["color"] = "orange".into();
    wrong["fields"][0]["default"] = "極".into();
    wrong["extends"] = "entity".into();
    let mut value = wrong.as_object().cloned().unwrap();
    value.remove("kind");
    value.insert("of".into(), "card".into());
    value.insert("dryRun".into(), true.into());
    let checked = ok(
        &core,
        "kind_create",
        json!({"target": "risk", "value": value}),
    );
    assert_eq!(checked["dryRun"], true);
    assert_eq!(checked["ok"], false);
    let problems: Vec<&str> = checked["problems"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p.as_str().unwrap())
        .collect();
    for wanted in [
        "cards の risk: semantics（検査・線・エクスポート・編集画面のコードの名前）は組み込みの種類だけです",
        "cards の risk: color は #RRGGBB です（\"orange\"）",
        "cards の risk: extends は base（組み込みの基の定義）かプロジェクトの種類です（entity は組み込みの種類）",
    ] {
        assert!(problems.contains(&wanted), "{wanted}: {problems:#?}");
    }
    assert!(
        problems
            .iter()
            .any(|p| p.contains("既定の「極」は options にありません")),
        "{problems:#?}"
    );
    // A built-in kind's name is not another's.
    for name in ["note", "ghost"] {
        let said = refused(
            &core,
            "kind_create",
            json!({"target": name, "value": {"of": "card"}}),
        );
        assert!(
            said.contains("もうあります（組み込みのカードの種類）"),
            "{said}"
        );
    }
    let kinds = ok(&core, "kind_read", json!({}));
    assert!(
        kinds["cards"]
            .as_array()
            .unwrap()
            .iter()
            .all(|k| k["builtin"] == true),
        "確かめただけでは書かない: {kinds}"
    );
    // dryRun of project_update is for what is under each root alone.
    assert!(
        refused(
            &core,
            "project_update",
            json!({"value": {"dryRun": true, "name": "x"}})
        )
        .contains("dryRun: true で確かめるのは project_tree と renames だけです")
    );
    defined(&core);
    let kinds = ok(&core, "kind_read", json!({}));
    let own: Vec<&Value> = kinds["cards"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|k| k["builtin"] != true)
        .collect();
    assert_eq!(own.len(), 1);
    assert_eq!(own[0]["kind"], "risk");
    // The history says what changed.
    let commits = op(
        &core,
        json!({"op": "commits", "project": project_id(&core), "limit": 5}),
    );
    let said: Vec<&str> = commits["result"]["commits"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|c| c["lines"][0]["summary"].as_str())
        .collect();
    for wanted in ["カードの種類 risk を作成", "セクションの種類 risks を作成"]
    {
        assert!(said.contains(&wanted), "{wanted}: {said:?}");
    }
}

#[test]
fn a_card_of_the_projects_own_kind_is_as_tall_as_the_screen_draws_it() {
    let core = Core::open(script_store()).unwrap();
    defined(&core);
    // Its fields but its header (severity) and the ones not written (due), then its rows:
    // owner and two steps, three rows — 54 + 3 × 27 points, five cells.
    ok(
        &core,
        "card_create",
        json!({"target": "Risks:Outage", "value": {"kind": "risk", "severity": "高", "owner": "Claude",
            "steps": [{"name": "backup"}, {"name": "failover"}]}}),
    );
    let tall = |core: &Core| {
        let read = ok(
            core,
            "section_read",
            json!({"target": "Risks", "value": {"include": ["places"]}}),
        );
        let place = read["places"]["Risks:Outage"].clone();
        place["bottomRight"]["y"].as_i64().unwrap() - place["topLeft"]["y"].as_i64().unwrap() + 1
    };
    assert_eq!(tall(&core), 5);
    // Twenty-one rows: fifteen shown and the strip below for the rest — 54 + 15 × 27 + 27 points.
    let steps: Vec<Value> = (0..20).map(|i| json!({"name": format!("s{i}")})).collect();
    ok(
        &core,
        "card_update",
        json!({"target": "Risks:Outage", "value": {"steps": steps}}),
    );
    assert_eq!(tall(&core), 18);
}

#[test]
fn a_card_of_the_projects_own_kind_is_made_read_changed_and_listed_as_any() {
    let core = Core::open(script_store()).unwrap();
    defined(&core);
    let made = ok(
        &core,
        "card_create",
        json!({"target": "Risks:Outage", "value": {"kind": "risk", "label": "停止", "severity": "高", "owner": "Claude",
            "steps": [{"name": "backup"}, {"name": "failover", "done": true}],
            "connections": [{"line": "link", "to": "Docs:Spec"}]}}),
    );
    assert_eq!(made["created"][0]["path"], "Risks:Outage", "{made}");
    assert_eq!(made["created"][0]["kind"], "risk");
    // Read by its definition: its fields and rows by name, its lines.
    let outage = read(&core, "Risks:Outage");
    assert_eq!(outage["kind"], "risk");
    assert_eq!(outage["label"], "停止");
    assert_eq!(outage["severity"], "高");
    assert_eq!(outage["owner"], "Claude");
    assert_eq!(
        outage["steps"],
        json!([{"name": "backup"}, {"name": "failover", "done": true}])
    );
    assert_eq!(outage["connections"][0]["to"], "Docs:Spec");
    assert!(outage.get("strayFields").is_none(), "{outage}");
    // What its definition refuses is refused, every one at once.
    let said = refused(
        &core,
        "card_create",
        json!({"target": "Risks:Leak", "value": {"kind": "risk", "severity": "極", "due": "明日", "steps": [{"done": true}]}}),
    );
    assert!(
        said.contains("severity: \"極\" は選べません（高・中・低）"),
        "{said}"
    );
    assert!(said.contains("due: 日付（YYYY-MM-DD）です"), "{said}");
    assert!(said.contains("steps[0].name: 要ります"), "{said}");
    let said = refused(
        &core,
        "card_create",
        json!({"target": "Risks:Leak", "value": {"kind": "risk", "color": "red"}}),
    );
    assert!(said.contains("color: risk が持てないキーです"), "{said}");
    // Only where a section's kind holds it, or anything (free).
    let said = refused(
        &core,
        "card_create",
        json!({"target": "Schemas:Leak", "value": {"kind": "risk"}}),
    );
    assert!(said.contains("なので risk は置けません"), "{said}");
    let free = free_section(&core);
    ok(
        &core,
        "card_create",
        json!({"target": format!("{free}:Leak"), "value": {"kind": "risk", "severity": "低"}}),
    );
    // A kind the project has not is said with the ones it has.
    let said = refused(
        &core,
        "card_create",
        json!({"target": "Risks:Leak2", "value": {"kind": "hazard"}}),
    );
    assert!(said.contains("（プロジェクトの種類: risk）"), "{said}");
    // Changed by name, the history saying which.
    let changed = ok(
        &core,
        "card_update",
        json!({"target": "Risks:Outage", "value": {"severity": "中", "due": "2026-10-31"}}),
    );
    assert_eq!(changed["path"], "Risks:Outage", "{changed}");
    assert_eq!(read(&core, "Risks:Outage")["severity"], "中");
    let commits = op(
        &core,
        json!({"op": "commits", "project": project_id(&core), "limit": 1}),
    );
    assert_eq!(
        commits["result"]["commits"][0]["lines"][0]["summary"],
        "カード Risks:Outage を更新（欄 severity・due）"
    );
    let said = refused(
        &core,
        "card_update",
        json!({"target": "Risks:Outage", "value": {"body": "x"}}),
    );
    assert!(
        said.contains("body は risk の欄にも行にもありません"),
        "{said}"
    );
    // Its fields are no target of their own.
    let said = refused(
        &core,
        "card_update",
        json!({"target": "Risks:Outage.severity", "value": {"label": "x"}}),
    );
    assert!(said.contains("欄と行はカードの value に書きます"), "{said}");
    // Listed by its kind and by what its fields hold.
    let listed = ok(&core, "card_list", json!({"value": {"kind": "risk"}}));
    let names: Vec<&str> = listed["cards"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["path"].as_str().unwrap())
        .collect();
    assert_eq!(names.len(), 2, "{listed}");
    let chosen = ok(
        &core,
        "card_list",
        json!({"value": {"kind": "risk", "fields": {"severity": "低"}}}),
    );
    assert_eq!(chosen["cards"].as_array().unwrap().len(), 1, "{chosen}");
    assert!(
        refused(&core, "card_list", json!({"value": {"kind": "hazard"}})).contains(
            "kind の「hazard」という種類はありません（組み込みの種類か、プロジェクトの種類 risk）"
        )
    );
    // Its guide: the definition as written and whole, and how a card of it is written.
    let guide = ok(&core, "guide", json!({"about": "risk"}));
    assert_eq!(guide["kind"], "risk", "{guide}");
    assert_eq!(guide["definition"]["color"], "#C2410C");
    let fields: Vec<&str> = guide["fields"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        fields,
        [
            "label",
            "description",
            "tags",
            "metadata",
            "severity",
            "owner",
            "due"
        ]
    );
    assert!(guide["more"].as_str().unwrap().contains("about: ownKinds"));
    // Several at once, a built-in topic beside the project's kind.
    let both = ok(&core, "guide", json!({"about": ["risk", "rev"]}));
    assert_eq!(both["risk"]["kind"], "risk", "{both}");
    assert!(both["rev"].is_string(), "{both}");
    let (refused_guide, said) = call(&core, "guide", json!({"about": "risk"}));
    assert!(
        refused_guide && said.contains("risk の説明はありません"),
        "{said}"
    );
    // The window reads it with its form and its kind whole.
    let whole = op(
        &core,
        json!({"op": "project_detail", "project": project_id(&core)}),
    )["result"]
        .clone();
    let card = whole["schemas"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "Outage")
        .unwrap();
    assert_eq!(card["kind"], "risk");
    assert_eq!(card["form"]["fields"]["severity"], "中");
    let defined_whole = whole["kinds"]["cards"]
        .as_array()
        .unwrap()
        .iter()
        .find(|k| k["kind"] == "risk")
        .unwrap();
    assert!(defined_whole.get("builtin").is_none());
    assert_eq!(defined_whole["fields"][0]["name"], "label");
    assert_eq!(defined_whole["rows"][0]["name"], "steps");
    let goal = whole["kinds"]["cards"]
        .as_array()
        .unwrap()
        .iter()
        .find(|k| k["kind"] == "goal")
        .unwrap();
    assert_eq!(goal["builtin"], true);
    assert!(goal.get("semantics").is_none());
    // Not a type in what is written out; on the diagrams.
    let ts = ok(
        &core,
        "export_read",
        json!({"targetdir": "[default]", "target": "ts", "value": {"files": ["types.ts"]}}),
    );
    assert!(
        !ts["files"][0]["text"].as_str().unwrap().contains("Outage"),
        "{ts}"
    );
    let (_, diagram) = call(
        &core,
        "section_read",
        json!({"project": PROJECT, "target": "Risks", "value": {"as": "diagram"}}),
    );
    assert!(diagram.contains("Outage"), "{diagram}");
}

#[test]
fn a_definition_changed_leaves_its_values_and_a_rename_takes_them_along() {
    let core = Core::open(script_store()).unwrap();
    common::set_kinds(
        &core,
        PROJECT,
        json!({"cards": [risk("sev")], "sections": [risks_section()]}),
    )
    .unwrap();
    under_main_root(&core, "risks", "Risks");
    for (name, sev) in [("A", "高"), ("B", "低")] {
        ok(
            &core,
            "card_create",
            json!({"target": format!("Risks:{name}"), "value": {"kind": "risk", "sev": sev, "steps": [{"name": "s1"}],
                "connections": [{"line": "link", "to": "Docs:Spec"}]}}),
        );
    }
    // Renamed in the definition alone: the values stay, as fields it does not have.
    let mut renamed = risk("severity");
    renamed["header"] = json!(["severity"]);
    common::set_kinds(&core, PROJECT, json!({"cards": [renamed.clone()]})).unwrap();
    let a = read(&core, "Risks:A");
    assert_eq!(a["strayFields"], json!({"sev": "高"}), "{a}");
    assert!(a.get("severity").is_none());
    // The window reads them with the card, to draw them faint.
    let whole = op(
        &core,
        json!({"op": "project_detail", "project": project_id(&core)}),
    )["result"]
        .clone();
    let shown = whole["schemas"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "A")
        .unwrap();
    assert_eq!(shown["strayFields"], json!({"sev": "高"}));
    let check = checked(&core, "main");
    let strays: Vec<(&Value, &Value)> = check["strayFields"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| (&s["at"], &s["fields"]))
        .collect();
    assert_eq!(
        strays,
        [
            (&json!("Risks:A"), &json!(["sev"])),
            (&json!("Risks:B"), &json!(["sev"]))
        ],
        "{check}"
    );
    assert!(check.get("cardProblems").is_none(), "{check}");
    // The card is still written as any, what is left kept.
    ok(
        &core,
        "card_update",
        json!({"target": "Risks:A", "value": {"owner": "Claude"}}),
    );
    assert_eq!(read(&core, "Risks:A")["strayFields"], json!({"sev": "高"}));
    // Let go of: only what is there, and only to let go of.
    assert!(
        refused(
            &core,
            "card_update",
            json!({"target": "Risks:A", "value": {"strayFields": {"sev": "中"}}})
        )
        .contains("strayFields は消すだけです")
    );
    assert!(
        refused(
            &core,
            "card_update",
            json!({"target": "Risks:A", "value": {"strayFields": {"x": null}}})
        )
        .contains("A に定義に無い欄「x」はありません（あるのは sev）")
    );
    ok(
        &core,
        "card_update",
        json!({"target": "Risks:A", "value": {"strayFields": {"sev": null}}}),
    );
    assert!(read(&core, "Risks:A").get("strayFields").is_none());
    // A branch's copy, and main changed beside it.
    ok(&core, "branch_create", json!({"target": "fix"}));
    ok(
        &core,
        "card_update",
        json!({"branch": "fix", "target": "Risks:B", "value": {"owner": "Claude"}}),
    );
    // A rename said takes the values along — what was left behind too — in every tree,
    // the base a branch's copy was made from with them.
    let moved = ok(
        &core,
        "kind_update",
        json!({"target": "risk", "value": {"renames": [{"field": "sev", "to": "severity"}]}}),
    );
    assert_eq!(
        moved["renamed"],
        json!([{"kind": "risk", "field": "sev", "to": "severity", "cards": 1}]),
        "{moved}"
    );
    let b = read(&core, "Risks:B");
    assert_eq!(b["severity"], "低", "{b}");
    assert!(b.get("strayFields").is_none());
    let in_branch = ok(
        &core,
        "card_read",
        json!({"branch": "fix", "target": "Risks:B"}),
    );
    assert_eq!(in_branch["severity"], "低", "{in_branch}");
    assert_eq!(in_branch["owner"], "Claude");
    let preview = ok(
        &core,
        "branch_read",
        json!({"target": "fix", "value": {"against": "main"}}),
    );
    assert!(
        preview["conflicts"].as_array().is_none_or(Vec::is_empty),
        "名前を変えたのは衝突しない: {preview}"
    );
    ok(&core, "branch_merge", json!({"target": "fix"}));
    let b = read(&core, "Risks:B");
    assert_eq!(
        (b["severity"].as_str(), b["owner"].as_str()),
        (Some("低"), Some("Claude"))
    );
    // A rename to a name the definition has not, and the old name not there either, is refused.
    let said = refused(
        &core,
        "kind_update",
        json!({"target": "risk", "value": {"renames": [{"field": "nope", "to": "level"}]}}),
    );
    assert!(
        said.contains("risk の定義に欄 level がありません"),
        "{said}"
    );
    // A kind of card and a row's kind renamed, the definitions (the section kind that holds it too)
    // and the lines along with them.
    let checked = ok(
        &core,
        "kind_update",
        json!({"target": "risk", "value": {"dryRun": true, "name": "hazard", "renames": [{"row": "steps", "to": "actions"}]}}),
    );
    assert_eq!(checked["ok"], true, "{checked}");
    assert_eq!(checked["renamed"][0]["cards"], 2);
    let renamed = ok(
        &core,
        "kind_update",
        json!({"target": "risk", "value": {"name": "hazard", "renames": [{"row": "steps", "to": "actions"}]}}),
    );
    assert_eq!(renamed["rows"][0]["name"], "actions", "{renamed}");
    assert_eq!(renamed["alsoChanged"], json!(["risks"]));
    assert_eq!(
        ok(&core, "kind_read", json!({"target": "risks"}))["holds"],
        json!(["hazard", "note"])
    );
    let b = read(&core, "Risks:B");
    assert_eq!(b["kind"], "hazard", "{b}");
    assert_eq!(b["actions"], json!([{"name": "s1"}]));
    assert_eq!(b["connections"][0]["to"], "Docs:Spec");
    let commits = op(
        &core,
        json!({"op": "commits", "project": project_id(&core), "limit": 1}),
    );
    assert!(
        commits["result"]["commits"][0]["lines"][0]["summary"]
            .as_str()
            .unwrap()
            .contains("名前を変更: カードの種類 risk → hazard（カード 2 枚）・hazard の行の種類 steps → actions（カード 2 枚）"),
        "{commits}"
    );
}

#[test]
fn a_kind_in_use_stays_and_a_card_that_no_longer_fits_is_checked() {
    let core = Core::open(script_store()).unwrap();
    defined(&core);
    ok(
        &core,
        "card_create",
        json!({"target": "Risks:Outage", "value": {"kind": "risk", "severity": "高"}}),
    );
    // In use: kept, said with where.
    let said = refused(&core, "kind_delete", json!({"target": "risk"}));
    assert!(
        said.contains("カードの種類「risk」は 1 か所で使われているので消せません（Outage）"),
        "{said}"
    );
    // So is the section kind that holds it, and a section kind that still names it.
    let said = refused(&core, "kind_delete", json!({"target": "risks"}));
    assert!(
        said.contains("セクションの種類「risks」は 1 か所で使われているので消せません"),
        "{said}"
    );
    // Changed so the card no longer fits: not refused, said by the check.
    let mut narrower = risk("severity");
    narrower["fields"][0]["options"] = json!(["中", "低"]);
    narrower["fields"][1]["required"] = true.into();
    common::set_kinds(&core, PROJECT, json!({"cards": [narrower]})).unwrap();
    let check = checked(&core, "main");
    let problems: Vec<(&str, &str)> = check["cardProblems"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| (p["at"].as_str().unwrap(), p["problem"].as_str().unwrap()))
        .collect();
    assert_eq!(
        problems,
        [
            ("Risks:Outage", "severity: \"高\" は選べません（中・低）"),
            ("Risks:Outage", "owner: 要ります")
        ],
        "{check}"
    );
    // Writing it again asks it to fit.
    let said = refused(
        &core,
        "card_update",
        json!({"target": "Risks:Outage", "value": {"due": "2026-11-01"}}),
    );
    assert!(said.contains("owner: 要ります"), "{said}");
    ok(
        &core,
        "card_update",
        json!({"target": "Risks:Outage", "value": {"severity": "中", "owner": "Claude"}}),
    );
    assert!(checked(&core, "main").get("cardProblems").is_none());
    // MCP writes the kinds as kind_update's, renames too.
    let project = project_id(&core);
    let set = op(
        &core,
        json!({"op": "mcp_writes_set", "project": project, "item": "kind_update", "state": "deny"}),
    );
    assert!(set.get("error").is_none(), "{set}");
    let said = refused(
        &core,
        "kind_update",
        json!({"target": "risk", "value": {"renames": [{"field": "x", "to": "severity"}]}}),
    );
    assert!(
        said.contains("kind_update（種類を変える） は MCP から使えません"),
        "{said}"
    );
}

/// A line of the project's own that names the kinds of row it leaves and lands at
/// (`rows`): for a kind of card with more than one, only those — written, and what
/// `as: lines` answers, alike. A field of the card itself is a row of no kind.
#[test]
fn a_line_of_the_projects_own_keeps_to_the_kinds_of_row_it_names() {
    let core = Core::open(script_store()).unwrap();
    let mut two = risk("severity");
    two["rows"]
        .as_array_mut()
        .unwrap()
        .push(json!({ "name": "causes", "label": "原因", "fields": [{ "name": "name", "fieldtype": "text" }] }));
    let answer = json!({ "line": "対策する", "base": "link",
        "from": [{ "kinds": ["note"], "at": "card" }],
        "to": { "kinds": ["risk"], "at": ["row"], "rows": ["steps"] } });
    let cause = json!({ "line": "原因の文書", "base": "link",
        "from": [{ "kinds": ["risk"], "at": "row", "rows": ["causes"] }],
        "to": { "kinds": ["*"], "at": ["card"] } });
    // The project's lines kept beside them (the script's moved signatures).
    let kept: Vec<Value> = ok(&core, "kind_read", json!({}))["lines"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|l| l["builtin"] != true)
        .cloned()
        .collect();
    let lines =
        |more: &[Value]| -> Value { Value::Array(kept.iter().chain(more).cloned().collect()) };
    // Checked with the kinds of card: a kind of row the kinds do not have is not one.
    let mut wrong = answer.clone();
    wrong["to"]["rows"] = json!(["fixes"]);
    let said = common::set_kinds(
        &core,
        PROJECT,
        json!({"cards": [two.clone()], "sections": [risks_section()], "lines": lines(&[wrong])}),
    )
    .unwrap_err();
    assert!(
        said.contains(
            "lines の 対策する の to: rows の「fixes」という行の種類は risk にありません（あるのは: steps・causes）"
        ),
        "{said}"
    );
    common::set_kinds(
        &core,
        PROJECT,
        json!({"cards": [two], "sections": [risks_section()], "lines": lines(&[answer, cause])}),
    )
    .unwrap();
    under_main_root(&core, "risks", "Risks");
    ok(
        &core,
        "card_create",
        json!({"target": "Risks:Outage", "value": {"kind": "risk", "severity": "高",
            "steps": [{"name": "backup"}], "causes": [{"name": "power"}]}}),
    );
    ok(
        &core,
        "card_create",
        json!({"target": "Risks:Memo", "value": {"kind": "note", "body": "停電の対策"}}),
    );
    // To a row of the kind it names.
    ok(
        &core,
        "card_update",
        json!({"target": "Risks:Memo", "value": {"connections": {"add": [{"line": "対策する", "to": "Risks:Outage", "at": "steps.backup"}]}}}),
    );
    let said = refused(
        &core,
        "card_update",
        json!({"target": "Risks:Memo", "value": {"connections": {"add": [{"line": "対策する", "to": "Risks:Outage", "at": "causes.power"}]}}}),
    );
    assert!(
        said.contains(
            "対策する（対策する）: 着く所は riskの行（steps） です（causes.power は行（causes））"
        ),
        "{said}"
    );
    // From a row of the kind it names; not another's, nor a field of the card.
    ok(
        &core,
        "card_update",
        json!({"target": "Risks:Outage", "value": {"connections": {"add": [{"line": "原因の文書", "from": "causes.power", "to": "Risks:Memo"}]}}}),
    );
    for (from, there) in [("steps.backup", "行（steps）"), ("severity", "欄")] {
        let said = refused(
            &core,
            "card_update",
            json!({"target": "Risks:Outage", "value": {"connections": {"add": [{"line": "原因の文書", "from": from, "to": "Risks:Memo"}]}}}),
        );
        assert!(
            said.contains(&format!(
                "{from} の 原因の文書（原因の文書）: この線は riskの行（causes） から出せます（risk の{there}からは出せません）"
            )),
            "{said}"
        );
    }
    // What `as: lines` answers keeps to it.
    let drawn = |target: &str, to: &str| -> Vec<String> {
        let read = ok(
            &core,
            "card_read",
            json!({"target": target, "value": {"as": "lineKinds", "to": to}}),
        );
        read["lines"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| l["line"].as_str().unwrap().to_string())
            .collect()
    };
    assert_eq!(
        drawn("Risks:Memo", "Risks:Outage.steps.backup"),
        ["link", "対策する"]
    );
    assert_eq!(drawn("Risks:Memo", "Risks:Outage.causes.power"), ["link"]);
    assert_eq!(
        drawn("Risks:Outage.causes.power", "Risks:Memo"),
        ["link", "原因の文書"]
    );
    assert_eq!(drawn("Risks:Outage.steps.backup", "Risks:Memo"), ["link"]);
    let read = ok(
        &core,
        "card_read",
        json!({"target": "Risks:Memo", "value": {"as": "lineKinds"}}),
    );
    let reaches = read["lines"]
        .as_array()
        .unwrap()
        .iter()
        .find(|l| l["line"] == "対策する")
        .unwrap()["reaches"]
        .clone();
    assert_eq!(reaches, "riskの行（steps）");
}

#[test]
fn a_container_of_the_projects_own_holds_cards_of_the_kinds_it_says() {
    let core = Core::open(script_store()).unwrap();
    let epic = json!({ "kind": "epic", "label": "エピック", "holds": ["story", "note"], "opens": "card-map",
        "fields": [{ "name": "state", "fieldtype": "dropdownlist", "options": ["todo", "done"], "done": ["done"] }] });
    let story = json!({ "kind": "story", "nest": true, "as_row": ["done", "label"], "on_click": "toggle_done",
        "fields": [{ "name": "done", "fieldtype": "checkbox", "done": [true] }, { "name": "points", "fieldtype": "number", "min": 0 }] });
    // A container says how it opens.
    let mut unopened = epic.clone();
    unopened.as_object_mut().unwrap().remove("opens");
    common::set_kinds(&core, PROJECT, json!({"cards": [story.clone()]})).unwrap();
    let mut value = unopened.as_object().cloned().unwrap();
    value.remove("kind");
    value.insert("of".into(), "card".into());
    value.insert("dryRun".into(), true.into());
    let dry = ok(
        &core,
        "kind_create",
        json!({"target": "epic", "value": value}),
    );
    assert!(
        dry["problems"]
            .to_string()
            .contains("holds のある種類（入れ物）には opens"),
        "{dry}"
    );
    common::set_kinds(&core, PROJECT, json!({"cards": [epic, story]})).unwrap();
    let free = free_section(&core);
    let made = ok(
        &core,
        "card_create",
        json!({"value": {
            format!("{free}:Login"): {"kind": "epic", "state": "todo"},
            format!("{free}:Login.Form"): {"kind": "story", "points": 3},
            format!("{free}:Login.Mail"): {"kind": "story", "done": true}
        }}),
    );
    assert_eq!(made["created"].as_array().unwrap().len(), 3, "{made}");
    let login = ok(
        &core,
        "card_read",
        json!({"target": format!("{free}:Login")}),
    );
    assert_eq!(
        login["cards"],
        json!([format!("{free}:Login.Form"), format!("{free}:Login.Mail")]),
        "{login}"
    );
    let form = read(&core, &format!("{free}:Login.Form"));
    assert_eq!(form["kind"], "story");
    assert_eq!(form["points"], 3);
    // Its card map, as a goal's: each card inside given a box, and put where it is said.
    let boxes = ok(
        &core,
        "card_read",
        json!({"target": format!("{free}:Login"), "value": {"include": ["places"]}}),
    );
    assert_eq!(boxes["places"].as_object().unwrap().len(), 2, "{boxes}");
    ok(
        &core,
        "card_update",
        json!({"target": format!("{free}:Login"), "value": {"places": {format!("{free}:Login.Mail"): {"topLeft": {"x": 40, "y": 0}}}}}),
    );
    let boxes = ok(
        &core,
        "card_read",
        json!({"target": format!("{free}:Login"), "value": {"include": ["places"]}}),
    );
    assert_eq!(
        boxes["places"][format!("{free}:Login.Mail")]["topLeft"]["x"],
        40,
        "{boxes}"
    );
    // Only what it holds.
    let said = refused(
        &core,
        "card_create",
        json!({"target": format!("{free}:Login.Spec"), "value": {"kind": "freedoc"}}),
    );
    assert!(
        said.contains("の中に置けるのは story・note です（freedoc ではなく）"),
        "{said}"
    );
    // Listed with what it holds.
    let inside = ok(
        &core,
        "card_list",
        json!({"value": {"in": format!("{free}:Login")}}),
    );
    assert_eq!(inside["cards"].as_array().unwrap().len(), 2, "{inside}");
    // What it holds changed under it: the cards inside it said by the check.
    let epic = json!({ "kind": "epic", "holds": ["note"], "opens": "card-map" });
    let story = json!({ "kind": "story", "fields": [{ "name": "done", "fieldtype": "checkbox" }, { "name": "points", "fieldtype": "number" }] });
    common::set_kinds(&core, PROJECT, json!({"cards": [epic, story]})).unwrap();
    let check = checked(&core, "main");
    let problems = check["cardProblems"].as_array().unwrap();
    assert!(
        problems
            .iter()
            .any(|p| p["at"] == format!("{free}:Login.Form")
                && p["problem"]
                    .as_str()
                    .unwrap()
                    .contains("の中に置けるのは note です（story ではなく）")),
        "{check}"
    );
    // The epic's old field is left behind, said apart.
    assert!(
        check["strayFields"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s["at"] == format!("{free}:Login") && s["fields"] == json!(["state"])),
        "{check}"
    );
}

#[test]
fn a_project_file_carries_the_projects_own_kinds_and_their_cards() {
    let core = Core::open(script_store()).unwrap();
    defined(&core);
    ok(
        &core,
        "card_create",
        json!({"target": "Risks:Outage", "value": {"kind": "risk", "severity": "高", "steps": [{"name": "backup"}]}}),
    );
    let project = project_id(&core);
    let text = op(&core, json!({"op": "project_file", "project": project}))["result"]["text"]
        .as_str()
        .unwrap()
        .to_string();
    let file: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(
        file["project"]["kinds"]["cards"][0]["kind"], "risk",
        "{file}"
    );
    // Read in as a copy: the kind and its card come along, and fit.
    let read_in = op(
        &core,
        json!({"op": "project_file_read", "text": text, "landing": "copy", "name": "写し"}),
    );
    assert!(read_in.get("error").is_none(), "{read_in}");
    // As the window reads the copy (the test's key reaches only the projects it was given).
    let copy = read_in["result"]["project"].as_str().unwrap().to_string();
    let whole = op(&core, json!({"op": "project_detail", "project": copy}))["result"].clone();
    let outage = whole["schemas"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "Outage")
        .unwrap();
    assert_eq!(outage["kind"], "risk", "{outage}");
    assert_eq!(outage["form"]["fields"]["severity"], "高");
    assert_eq!(
        outage["form"]["rows"]["steps"][0]["fields"]["name"],
        "backup"
    );
    assert_eq!(whole["project"]["kinds"]["cards"][0]["kind"], "risk");
}

/// A kind of line the project defines says whether it is drawn from the start (`visible`):
/// false hides it on every diagram until it is shown, and MCP's diagrams leave it out
/// unless it is asked for by name. Anything but true or false is refused.
#[test]
fn a_line_hidden_from_the_start_is_left_out_of_a_diagram_unless_asked_for() {
    let core = Core::open(script_store()).unwrap();
    let kept: Vec<Value> = ok(&core, "kind_read", json!({}))["lines"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|l| l["builtin"] != true)
        .cloned()
        .collect();
    let signed = kept
        .iter()
        .position(|l| l["line"] == "signedBy")
        .expect("答えのプロジェクトに signedBy の線が無い");
    let drawn = |core: &Core, kinds: Option<Value>| -> Vec<String> {
        let mut value = json!({"as": "diagram", "revisionOnly": true});
        if let Some(kinds) = kinds {
            value["kinds"] = kinds;
        }
        let read = ok(
            core,
            "branch_read",
            json!({ "target": "main", "value": value }),
        );
        read["spec"]["kinds"]
            .as_array()
            .unwrap()
            .iter()
            .map(|k| k.as_str().unwrap().to_string())
            .collect()
    };
    // Every kind but the built-in ones hidden from the start (mentions), while none of the
    // project's says otherwise.
    let before = drawn(&core, None);
    assert!(
        !before.is_empty()
            && !before.contains(&"mentions".to_string())
            && before.contains(&"signedBy".to_string()),
        "{before:?}"
    );

    let mut wrong = kept.clone();
    wrong[signed]["visible"] = json!("no");
    let said = common::set_kinds(&core, PROJECT, json!({"lines": wrong})).unwrap_err();
    assert!(said.contains("visible は true か false です"), "{said}");

    let mut hidden = kept.clone();
    hidden[signed]["visible"] = json!(false);
    common::set_kinds(&core, PROJECT, json!({"lines": hidden})).unwrap();
    let read = ok(&core, "kind_read", json!({}));
    let line = read["lines"]
        .as_array()
        .unwrap()
        .iter()
        .find(|l| l["line"] == "signedBy")
        .unwrap()
        .clone();
    assert_eq!(line["visible"], json!(false));
    // Not asked: every kind there is but the one hidden.
    let shown = drawn(&core, None);
    assert!(
        !shown.is_empty() && !shown.contains(&"signedBy".to_string()),
        "{shown:?}"
    );
    // Asked by name: drawn.
    assert_eq!(
        drawn(&core, Some(json!(["signedBy"]))),
        vec!["signedBy".to_string()]
    );
}

/// A kind of section the project defines may say the mark the nav panel puts before its sections:
/// `icon: {name, color}`, an SF Symbols name and a colour; anything else is refused, and so is the
/// way before (`icon` a string beside a `color`).
#[test]
fn a_kind_of_section_says_its_mark_and_its_colour() {
    let core = Core::open(script_store()).unwrap();
    let mut section = risks_section();
    section["icon"] = json!({"name": "exclamationmark.triangle", "color": "#C2410C"});
    common::set_kinds(
        &core,
        PROJECT,
        json!({"cards": [risk("severity")], "sections": [section.clone()]}),
    )
    .unwrap();
    let read = ok(&core, "kind_read", json!({}));
    let mine = read["sections"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["section"] == section["section"])
        .unwrap()
        .clone();
    assert_eq!(
        mine["icon"],
        json!({"name": "exclamationmark.triangle", "color": "#C2410C"})
    );
    // Either of the two may be left out.
    ok(
        &core,
        "kind_update",
        json!({"target": "risks", "value": {"icon": {"color": "#112233"}}}),
    );
    let only = ok(&core, "kind_read", json!({"target": "risks"}));
    assert_eq!(only["icon"], json!({"color": "#112233"}));
    // What is wrong, said.
    let wrong = ok(
        &core,
        "kind_update",
        json!({"target": "risks", "value": {"dryRun": true,
            "icon": {"name": "Exclamation Mark", "color": "orange", "size": 3}}}),
    );
    let said = wrong["problems"].to_string();
    assert_eq!(wrong["ok"], json!(false));
    assert!(
        said.contains("icon の name は SF Symbols の名前です"),
        "{said}"
    );
    assert!(said.contains("icon の color は #RRGGBB です"), "{said}");
    assert!(said.contains("icon に「size」は書けません"), "{said}");
    // The way before: refused when written, with how to write it.
    let flat = ok(
        &core,
        "kind_update",
        json!({"target": "risks", "value": {"dryRun": true, "icon": "folder", "color": "#C2410C"}}),
    );
    let said = flat["problems"].to_string();
    assert_eq!(flat["ok"], json!(false));
    assert!(said.contains("icon は {name, color} の形です"), "{said}");
    assert!(said.contains("color は icon の中に書きます"), "{said}");
}

/// The built-in kinds of section have their marks in `builtin.json`; a project writes one over —
/// `kind_update` of the built-in kind with `icon`, kept as `icons` by the kind's name — and the part
/// it leaves out stays the built-in one's. `null` gives it back; the screen's detail has it merged.
#[test]
fn a_built_in_kind_of_section_has_its_mark_and_a_project_writes_it_over() {
    let core = Core::open(script_store()).unwrap();
    let own = ok(&core, "kind_read", json!({"target": "schema"}));
    assert_eq!(
        own["icon"],
        json!({"name": "tablecells", "color": "#FF9500"})
    );
    assert!(own.get("iconChanged").is_none(), "{own}");
    // The colour alone.
    let written = ok(
        &core,
        "kind_update",
        json!({"target": "schema", "value": {"icon": {"color": "#112233"}}}),
    );
    assert_eq!(
        written["icon"],
        json!({"name": "tablecells", "color": "#112233"})
    );
    assert_eq!(written["iconChanged"], true);
    // Both; the context beside it, in one write.
    let both = ok(
        &core,
        "kind_update",
        json!({"target": "schema", "value": {"icon": {"name": "cube", "color": "#445566"},
            "context": {"use_when": "このプロジェクトの形"}}}),
    );
    assert_eq!(both["icon"], json!({"name": "cube", "color": "#445566"}));
    assert_eq!(both["contextChanged"], true);
    // The screen's detail carries it merged, beside the ones left as built.
    let detail = op(
        &core,
        json!({"op": "project_detail", "project": project_id(&core)}),
    );
    let sections = detail["result"]["kinds"]["sections"].as_array().unwrap();
    let marked = |name: &str| -> Value {
        sections
            .iter()
            .find(|s| s["section"] == name)
            .unwrap_or_else(|| panic!("{name}"))
            .clone()
    };
    assert_eq!(
        marked("schema")["icon"],
        json!({"name": "cube", "color": "#445566"})
    );
    assert_eq!(marked("schema")["iconChanged"], true);
    assert_eq!(
        marked("chart")["icon"],
        json!({"name": "arrow.triangle.branch", "color": "#FF3B30"})
    );
    // Only a built-in kind of section has a mark written over; and what is not a mark is refused.
    assert!(
        refused(
            &core,
            "kind_update",
            json!({"target": "entity", "value": {"icon": {"name": "cube"}}})
        )
        .contains("（icon は変えられません）")
    );
    let bad = ok(
        &core,
        "kind_update",
        json!({"target": "schema", "value": {"dryRun": true, "icon": {"color": "blue"}}}),
    );
    assert_eq!(bad["ok"], json!(false));
    assert!(
        bad["problems"]
            .to_string()
            .contains("icon の color は #RRGGBB です"),
        "{bad}"
    );
    // Back to its own.
    let back = ok(
        &core,
        "kind_update",
        json!({"target": "schema", "value": {"icon": null}}),
    );
    assert_eq!(
        back["icon"],
        json!({"name": "tablecells", "color": "#FF9500"})
    );
    assert!(back.get("iconChanged").is_none(), "{back}");
    assert_eq!(
        back["contextChanged"], true,
        "the context stays written over"
    );
}

/// A mark written the way before is read and let go: the project still opens and reads, the section's
/// mark the default. (Nothing in a store is rewritten; it is refused only when it is written again.)
#[test]
fn a_mark_written_the_way_before_reads_as_none() {
    use specboard_core::kinds::Defs;
    let own = json!({
        "cards": [risk("severity")],
        "sections": [{"section": "risks", "label": "リスク", "holds": ["risk", "note"],
                      "icon": "exclamationmark.triangle", "color": "#C2410C"}]
    });
    let defs = Defs::of_project(Some(&own.to_string()));
    let section = defs.section("risks").unwrap();
    assert_eq!((section.icon.clone(), section.color.clone()), (None, None));
    // And a built-in one's `icons`, as the store keeps them, set into the definitions.
    let kept = json!({"icons": {"schema": {"name": "cube"}, "nothing": {"name": "x"}}});
    let defs = Defs::of_project(Some(&kept.to_string()));
    let schema = defs.section("schema").unwrap();
    assert_eq!(
        (schema.icon.clone(), schema.color.clone()),
        (Some("cube".to_string()), Some("#FF9500".to_string()))
    );
}

/// A container of the project's own draws a row for each card inside it, as the card's kind's `as_row`
/// says: the core reckons its box as tall — a row a card, and a row of buttons for each choice it
/// names but the one that says done. `nest`, written before, is read and let go.
#[test]
fn a_container_of_the_projects_own_is_as_tall_as_its_rows_for_the_cards_inside() {
    let core = Core::open(script_store()).unwrap();
    let epic = json!({ "kind": "epic", "holds": ["story"], "opens": "card-map",
        "fields": [{ "name": "state", "fieldtype": "dropdownlist", "options": ["todo", "done"] }] });
    let story = json!({ "kind": "story", "nest": true, "as_row": ["done", "label", "priority", "points"],
        "fields": [{ "name": "done", "fieldtype": "checkbox", "done": [true] },
                   { "name": "priority", "fieldtype": "radio", "options": ["高", "低"] },
                   { "name": "points", "fieldtype": "number", "min": 0 }] });
    common::set_kinds(&core, PROJECT, json!({"cards": [story, epic]})).unwrap();
    let free = free_section(&core);
    ok(
        &core,
        "card_create",
        json!({"value": {
            format!("{free}:Login"): {"kind": "epic", "state": "todo"},
            format!("{free}:Login.Form"): {"kind": "story", "points": 3},
            format!("{free}:Login.Mail"): {"kind": "story", "done": true}
        }}),
    );
    let read = ok(
        &core,
        "section_read",
        json!({"target": free, "value": {"include": ["places"]}}),
    );
    let place = read["places"][format!("{free}:Login")].clone();
    let tall =
        place["bottomRight"]["y"].as_i64().unwrap() - place["topLeft"]["y"].as_i64().unwrap() + 1;
    // Its state written (one row), then each story a row and a row of buttons for its priority —
    // Mail, done, in the one row 「済み」: four rows — 54 + 4 × 27 points, six cells.
    assert_eq!(tall, 6);
}

/// How a card of the project's own opens and what pressing it twice does: `opens: markdown` with
/// a body to write, a container on its card map alone; `on_double_click: edit_in_place` kept,
/// `on_click` read and let go.
#[test]
fn a_kind_of_the_projects_own_opens_as_a_note_with_a_body_to_write() {
    let core = Core::open(script_store()).unwrap();
    let page = json!({ "kind": "page", "opens": "markdown", "on_double_click": "edit_in_place",
        "on_click": "toggle_done",
        "fields": [{ "name": "body", "fieldtype": "markdown" }, { "name": "state", "fieldtype": "radio", "options": ["下書き", "済み"] }] });
    common::set_kinds(&core, PROJECT, json!({"cards": [page]})).unwrap();
    let free = free_section(&core);
    ok(
        &core,
        "card_create",
        json!({"target": format!("{free}:Intro"), "value": {"kind": "page", "body": "# はじめに"}}),
    );
    ok(
        &core,
        "card_update",
        json!({"target": format!("{free}:Intro"), "value": {"body": "# はじめに\n\n書いた"}}),
    );
    assert_eq!(
        read(&core, &format!("{free}:Intro"))["body"],
        "# はじめに\n\n書いた"
    );
    let kinds = ok(&core, "kind_read", json!({}));
    let read_back = kinds["cards"]
        .as_array()
        .unwrap()
        .iter()
        .find(|k| k["kind"] == "page")
        .unwrap()
        .clone();
    assert_eq!(read_back["opens"], "markdown");
    assert_eq!(read_back["on_double_click"], "edit_in_place");
    // The built-in task no longer says what a press does: its check is its face's and its row's.
    let task = kinds["cards"]
        .as_array()
        .unwrap()
        .iter()
        .find(|k| k["kind"] == "task")
        .unwrap()
        .clone();
    assert!(task.get("on_click").is_none(), "{task}");

    let no_body = json!({ "kind": "page", "opens": "markdown", "fields": [{ "name": "text", "fieldtype": "markdown" }] });
    let said = common::set_kinds(&core, PROJECT, json!({"cards": [no_body]})).unwrap_err();
    assert!(
        said.contains("opens の markdown には、longtext か markdown の欄 body が要ります"),
        "{said}"
    );
    let lonely_map = json!({ "kind": "lonely", "opens": "card-map" });
    let said = common::set_kinds(&core, PROJECT, json!({"cards": [lonely_map]})).unwrap_err();
    assert!(
        said.contains("opens の card-map は、holds のある種類（入れ物）だけです"),
        "{said}"
    );
}
