//! The cards' lines written out as JSON (the export format `json-connections`): an entry of an export
//! takes every card under its section and writes, a card after another by its path, the lines each
//! has — as `card_read` says them — and a spec's scenarios. For what counts lines by a tool of its
//! own, reading the copy and not the app: so the entry is hidden by no nearer entry, what it writes
//! is the same for the same cards, and a line to a working directory's file is the path as written.

use crate::common;

use common::{mcp, ok, op, place, refused, script_store};
use serde_json::{Value, json};
use specboard_core::Core;

/// The sample, MCP let set its exports, with the cards the tests look at: a spec of two scenarios
/// (one tied to a test note and to a shape's field, one with no line), the test note, a note with
/// a line of its own and one with none — all in Docs, the spec's partner shape in Schemas/Ordering.
fn core() -> Core {
    let core = Core::open(script_store()).unwrap();
    let said = op(
        &core,
        json!({"op": "mcp_writes_set", "project": common::project_id(&core), "group": "export", "state": "allow"}),
    );
    assert!(said.get("error").is_none(), "{said}");
    ok(
        &core,
        "card_create",
        json!({"value": {
            "Docs:Pay": {"kind": "spec", "label": "支払う", "scenarios": [
                {"name": "paid", "heading": "支払うと確定する", "steps": [{"when": "支払う"}, {"then": "確定する"}]},
                {"name": "refused", "heading": "残高が無ければ断る", "steps": [{"when": "支払う"}, {"then": "断る"}]}]},
            "Docs:PayTest": {"kind": "note", "body": "支払いのテスト"},
            "Docs:Plain": {"kind": "note", "body": "線の無いメモ"},
            "Docs:Linked": {"kind": "note", "body": "線のあるメモ"},
        }}),
    );
    ok(
        &core,
        "card_update",
        json!({"target": "Docs:Linked", "value": {"connections": {"add": [
            {"line": "link", "to": "Docs:Pay"}]}}}),
    );
    ok(
        &core,
        "card_update",
        json!({"target": "Docs:Pay", "value": {"connections": {"add": [
            {"line": "testedBy", "from": "paid", "to": "Docs:PayTest"},
            {"line": "link", "from": "paid", "to": "Schemas/Ordering:Order", "at": "total", "description": "合計を確かめる"},
        ]}}}),
    );
    core
}

/// The export `lines`, of the one entry `entry`.
fn lines_export(core: &Core, entry: Value) {
    ok(
        core,
        "export_create",
        json!({"target": "lines", "value": {"entries": [entry]}}),
    );
}

/// What the export `lines` writes to `path` now, as `export_read` says it without writing.
fn text(core: &Core, path: &str) -> String {
    let folder = place("lines-read");
    std::fs::create_dir_all(&folder).unwrap();
    let said = ok(
        core,
        "export_read",
        json!({"targetdir": folder.to_str().unwrap(), "target": "lines", "value": {"files": [path]}}),
    );
    said["files"][0]["text"]
        .as_str()
        .unwrap_or_else(|| panic!("{said}"))
        .to_string()
}

fn cards(text: &str) -> Vec<Value> {
    let said: Value = serde_json::from_str(text).unwrap_or_else(|e| panic!("{text}: {e}"));
    said["cards"].as_array().unwrap().clone()
}

fn paths(cards: &[Value]) -> Vec<&str> {
    cards.iter().map(|c| c["card"].as_str().unwrap()).collect()
}

#[test]
fn a_lines_entry_writes_each_cards_lines_and_a_specs_scenarios() {
    let core = core();
    lines_export(
        &core,
        json!({"from": "Docs", "to": "lines.json", "format": "json-connections"}),
    );
    let folder = place("lines-out");
    std::fs::create_dir_all(&folder).unwrap();
    let at = folder.to_str().unwrap();
    ok(
        &core,
        "export_execute",
        json!({"targetdir": at, "target": "lines"}),
    );
    let written = std::fs::read_to_string(folder.join("lines.json")).unwrap();
    assert_eq!(written, text(&core, "lines.json"));
    assert!(written.ends_with("}\n"), "{written:?}");

    // By path: only the cards that have a line, and the spec — the note with none is not there.
    let all = cards(&written);
    let names = paths(&all);
    let mut sorted = names.clone();
    sorted.sort_unstable();
    assert_eq!(names, sorted, "{written}");
    assert!(names.contains(&"Docs:Pay"), "{written}");
    assert!(!names.contains(&"Docs:Plain"), "{written}");
    assert!(!names.contains(&"Docs:PayTest"), "{written}");
    // The spec: its scenarios — the one with no line too — and its lines as card_read says them.
    let pay = all.iter().find(|c| c["card"] == "Docs:Pay").unwrap();
    let read = ok(
        &core,
        "card_read",
        json!({"target": "Docs:Pay", "value": {"include": ["connections"]}}),
    );
    assert_eq!(
        *pay,
        json!({
            "card": "Docs:Pay",
            "kind": "spec",
            "scenarios": [
                {"name": "paid", "heading": "支払うと確定する"},
                {"name": "refused", "heading": "残高が無ければ断る"}],
            "connections": [
                {"line": "testedBy", "from": "paid", "to": "Docs:PayTest"},
                {"line": "link", "from": "paid", "to": "Schemas/Ordering:Order", "at": "total", "description": "合計を確かめる"}],
        })
    );
    assert_eq!(pay["connections"], read["connections"]);
    // The keys in the order they are written in, whatever order a reader sorts them in.
    let at_pay = written.find("\"card\": \"Docs:Pay\"").unwrap();
    let order: Vec<usize> = ["\"kind\"", "\"scenarios\"", "\"connections\""]
        .iter()
        .map(|k| written[at_pay..].find(k).unwrap())
        .collect();
    assert!(order[0] < order[1] && order[1] < order[2], "{written}");
    // A card of another kind has its lines, and no scenarios to say.
    let linked = all.iter().find(|c| c["card"] == "Docs:Linked").unwrap();
    assert_eq!(
        *linked,
        json!({"card": "Docs:Linked", "kind": "note", "connections": [{"line": "link", "to": "Docs:Pay"}]})
    );
    assert!(
        all.iter()
            .all(|c| c["kind"] == "spec" || !c["connections"].as_array().unwrap().is_empty()),
        "{written}"
    );

    // Written again with nothing changed: the same bytes.
    ok(
        &core,
        "export_execute",
        json!({"targetdir": at, "target": "lines"}),
    );
    assert_eq!(
        std::fs::read_to_string(folder.join("lines.json")).unwrap(),
        written
    );

    // The card a line reaches gone: the line stays, with no card to name.
    ok(&core, "card_delete", json!({"target": ["Docs:PayTest"]}));
    let after = cards(&text(&core, "lines.json"));
    let pay = after.iter().find(|c| c["card"] == "Docs:Pay").unwrap();
    assert_eq!(
        pay["connections"][0],
        json!({"line": "testedBy", "from": "paid", "to": null})
    );

    // Narrowed: the kinds of line, then the kinds of card.
    ok(
        &core,
        "export_update",
        json!({"target": "lines", "value": {"entries": {"remove": ["Docs"], "add": [
            {"from": "Docs", "to": "lines.json", "format": "json-connections", "lines": ["link"]}]}}}),
    );
    let narrowed = cards(&text(&core, "lines.json"));
    assert!(
        narrowed.iter().all(|c| c["connections"]
            .as_array()
            .unwrap()
            .iter()
            .all(|l| l["line"] == "link")),
        "{narrowed:?}"
    );
    assert_eq!(
        narrowed.iter().find(|c| c["card"] == "Docs:Pay").unwrap()["connections"],
        json!([{"line": "link", "from": "paid", "to": "Schemas/Ordering:Order", "at": "total", "description": "合計を確かめる"}])
    );
    ok(
        &core,
        "export_update",
        json!({"target": "lines", "value": {"entries": {"remove": ["Docs"], "add": [
            {"from": "Docs", "to": "lines.json", "format": "json-connections", "kinds": ["spec"]}]}}}),
    );
    let specs = cards(&text(&core, "lines.json"));
    assert!(!specs.is_empty(), "{specs:?}");
    assert!(specs.iter().all(|c| c["kind"] == "spec"), "{specs:?}");
}

#[test]
fn a_lines_entry_stands_beside_the_other_entries() {
    let core = core();
    // A section inside Docs with a spec of its own, and an entry of that section's: the nearest
    // entry of its cards.
    ok(&core, "section_create", json!({"target": "Docs/Deep"}));
    ok(
        &core,
        "card_create",
        json!({"value": {"Docs/Deep:Ship": {"kind": "spec", "label": "送る", "scenarios": [
            {"name": "sent", "heading": "送ると届く", "steps": [{"when": "送る"}, {"then": "届く"}]}]}}}),
    );
    ok(
        &core,
        "card_update",
        json!({"target": "Docs/Deep:Ship", "value": {"connections": {"add": [
            {"line": "testedBy", "from": "sent", "to": "Docs:PayTest"}]}}}),
    );
    ok(
        &core,
        "export_create",
        json!({"target": "lines", "value": {"entries": [
            {"from": "Docs", "to": "out/"},
            {"from": "Docs/Deep", "to": "deep/"}]}}),
    );
    let files = |core: &Core| -> Vec<(String, Vec<String>)> {
        let folder = place("beside");
        std::fs::create_dir_all(&folder).unwrap();
        let read = ok(
            core,
            "export_read",
            json!({"targetdir": folder.to_str().unwrap(), "target": "lines"}),
        );
        read["exports"][0]["files"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| {
                (
                    f["path"].as_str().unwrap().to_string(),
                    f["cards"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|c| c.as_str().unwrap().to_string())
                        .collect(),
                )
            })
            .collect()
    };
    let before = files(&core);
    assert!(
        before.iter().any(|(path, _)| path == "deep/Ship.md"),
        "{before:?}"
    );

    ok(
        &core,
        "export_update",
        json!({"target": "lines", "value": {"entries": {"add": [
            {"from": "Docs", "to": "lines.json", "format": "json-connections"}]}}}),
    );
    // Hidden by no nearer entry: the cards of the section inside are there too.
    let after = files(&core);
    let lines = &after
        .iter()
        .find(|(path, _)| path == "lines.json")
        .unwrap()
        .1;
    assert!(lines.contains(&"Docs:Pay".to_string()), "{lines:?}");
    assert!(lines.contains(&"Docs/Deep:Ship".to_string()), "{lines:?}");
    // Hiding none: what the other entries write is what they wrote.
    let others: Vec<&(String, Vec<String>)> = after
        .iter()
        .filter(|(path, _)| path != "lines.json")
        .collect();
    assert_eq!(others, before.iter().collect::<Vec<_>>());
    // A card says both the files this export writes it to.
    let read = ok(&core, "card_read", json!({"target": "Docs/Deep:Ship"}));
    let written: Vec<&Value> = read["writtenTo"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|w| w["export"] == "lines")
        .collect();
    assert_eq!(
        written,
        [
            &json!({"export": "lines", "file": "deep/Ship.md"}),
            &json!({"export": "lines", "file": "lines.json", "format": "json-connections"})
        ],
    );

    // Left out of the entries above it, the lines' too.
    ok(
        &core,
        "export_update",
        json!({"target": "lines", "value": {"exclude": {"add": ["Docs/Deep"]}}}),
    );
    let left = files(&core);
    let lines = &left
        .iter()
        .find(|(path, _)| path == "lines.json")
        .unwrap()
        .1;
    assert!(lines.contains(&"Docs:Pay".to_string()), "{lines:?}");
    assert!(!lines.contains(&"Docs/Deep:Ship".to_string()), "{lines:?}");
}

#[test]
fn a_lines_entry_is_refused_where_it_cannot_be_written() {
    let core = core();
    lines_export(
        &core,
        json!({"from": "Docs", "to": "lines.json", "format": "json-connections"}),
    );
    let folder = place("lines-refused");
    std::fs::create_dir_all(&folder).unwrap();
    let kept = |core: &Core| {
        ok(
            core,
            "export_read",
            json!({"targetdir": folder.to_str().unwrap(), "target": "lines"}),
        )["exports"][0]["entries"]
            .clone()
    };
    let before = kept(&core);
    let add = |entry: Value| {
        refused(
            &core,
            "export_update",
            json!({"target": "lines", "value": {"entries": {"add": [entry]}}}),
        )
    };
    // One file, of JSON.
    let folder = add(json!({"from": "Docs", "to": "lines/", "format": "json-connections"}));
    assert!(folder.contains("lines/"), "{folder}");
    let markdown = add(json!({"from": "Docs", "to": "lines.md", "format": "json-connections"}));
    assert!(markdown.contains(".json"), "{markdown}");
    // A kind of line the project has: said with the ones there are.
    let unknown = add(
        json!({"from": "Schemas", "to": "schemas.json", "format": "json-connections", "lines": ["testedBy", "cheks"]}),
    );
    assert!(
        unknown.contains("cheks") && unknown.contains("testedBy") && unknown.contains("embed"),
        "{unknown}"
    );
    // Only this format narrows by kinds of line.
    let other = add(json!({"from": "Schemas", "to": "types.ts", "lines": ["embed"]}));
    assert!(other.contains("json-connections"), "{other}");
    // One entry a file: two that narrow differently would be one file no reader could trust.
    let twice = add(
        json!({"from": "Schemas", "to": "lines.json", "format": "json-connections", "lines": ["embed"]}),
    );
    assert!(twice.contains("lines.json"), "{twice}");
    // A card or a section: a stackdoc's version has no lines of its own.
    ok(
        &core,
        "card_create",
        json!({"value": {"Docs:Log": {"kind": "stackdoc", "versions": [
            {"key": "v1", "name": "v1", "body": "始めの版"}]}}}),
    );
    let version =
        add(json!({"from": "Docs:Log.v1", "to": "log.json", "format": "json-connections"}));
    assert!(version.contains("Docs:Log.v1"), "{version}");
    assert_eq!(kept(&core), before);
    // What the entry is said back as: its kinds of line with it.
    ok(
        &core,
        "export_update",
        json!({"target": "lines", "value": {"entries": {"add": [
            {"from": "Schemas", "to": "schemas.json", "format": "json-connections", "lines": ["embed", "points"]}]}}}),
    );
    assert_eq!(
        kept(&core)[1],
        json!({"from": "Schemas", "to": "schemas.json", "format": "json-connections", "lines": ["embed", "points"]})
    );
}

#[test]
fn a_branchs_lines_are_written_and_kept_as_any_file_is() {
    let core = core();
    lines_export(
        &core,
        json!({"from": "Docs", "to": "lines.json", "format": "json-connections", "kinds": ["spec"]}),
    );
    ok(&core, "branch_create", json!({"target": "goal"}));
    ok(
        &core,
        "card_update",
        json!({"branch": "goal", "target": "Docs:Pay", "value": {"connections": {"add": [
            {"line": "testedBy", "from": "refused", "to": "Docs:PayTest"}]}}}),
    );
    let worktree = place("lines-branch");
    std::fs::create_dir_all(&worktree).unwrap();
    let at = worktree.to_str().unwrap();
    ok(
        &core,
        "export_execute",
        json!({"targetdir": at, "branch": "goal", "target": "lines"}),
    );
    let written = std::fs::read_to_string(worktree.join("lines.json")).unwrap();
    let branch_line = json!({"line": "testedBy", "from": "refused", "to": "Docs:PayTest"});
    let lines_in = |text: &str| cards(text)[0]["connections"].as_array().unwrap().clone();
    assert!(lines_in(&written).contains(&branch_line), "{written}");
    assert!(!lines_in(&text(&core, "lines.json")).contains(&branch_line));

    // Kept beside what was written, as any file: what it holds, and its cards.
    let record: Value = serde_json::from_str(
        &std::fs::read_to_string(
            worktree
                .join(".specboard")
                .join(common::PROJECT)
                .join("lines.json"),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(record["branch"], "goal");
    let file = &record["files"][0];
    assert_eq!(file["path"], "lines.json");
    assert_eq!(file["content"], "json-connections");
    assert_eq!(file["cards"][0]["path"], "Docs:Pay");

    // How it stands against what was written: current, and changed once a line of its card is.
    let stands = |core: &Core| {
        let read = ok(
            core,
            "export_read",
            json!({"targetdir": at, "branch": "goal", "target": "lines"}),
        );
        let file = read["exports"][0]["files"][0].clone();
        assert_eq!(file["content"], "json-connections", "{read}");
        file["status"].as_str().unwrap().to_string()
    };
    assert_eq!(stands(&core), "current");
    ok(
        &core,
        "card_update",
        json!({"branch": "goal", "target": "Docs:Pay", "value": {"connections": {"remove": [
            {"line": "testedBy", "from": "refused"}]}}}),
    );
    assert_eq!(stands(&core), "changed");
}

#[test]
fn a_line_to_a_working_directorys_file_is_written_as_its_path() {
    let core = core();
    // The working directory `repo`, bound to a folder while the lines are drawn.
    let mut tree = ok(&core, "project_read", json!({}))["projectTree"].clone();
    tree["main_sections"]
        .as_array_mut()
        .unwrap()
        .push(json!({"type": "workdir", "name": "repo"}));
    ok(
        &core,
        "project_update",
        json!({"value": {"project_tree": tree}}),
    );
    let root = place("lines-files");
    std::fs::create_dir_all(root.join("tests")).unwrap();
    std::fs::write(root.join("tests/pay.rs"), "fn pays() {}\n").unwrap();
    let bind = |folder: Value| {
        let said = op(
            &core,
            json!({"op": "tool", "name": "section_update", "arguments": {
                "project": common::PROJECT, "target": "repo", "value": {"folder": folder}}}),
        );
        assert!(said.get("error").is_none(), "{said}");
    };
    bind(root.to_string_lossy().into_owned().into());
    mcp(
        &core,
        "card_update",
        json!({"target": "Docs:Pay", "value": {"connections": {"add": [
            {"line": "testedBy", "from": "refused", "to": "repo/tests:'pay.rs'"},
            {"line": "testedBy", "from": "refused", "to": "repo/tests:'pay.rs'.pays"}]}}}),
    )
    .unwrap();
    let said = ok(
        &core,
        "card_read",
        json!({"target": "Docs:Pay", "value": {"include": ["connections"]}}),
    )["connections"]
        .clone();
    let to_files: Vec<&Value> = said
        .as_array()
        .unwrap()
        .iter()
        .filter(|l| l["from"] == "refused")
        .collect();
    assert_eq!(
        to_files,
        [
            &json!({"line": "testedBy", "from": "refused", "to": "repo/tests:'pay.rs'"}),
            &json!({"line": "testedBy", "from": "refused", "to": "repo/tests:'pay.rs'.pays"})
        ],
        "{said}"
    );

    // No folder bound on the machine that writes: the same paths, as written — read and written out.
    bind(Value::Null);
    assert_eq!(
        ok(
            &core,
            "card_read",
            json!({"target": "Docs:Pay", "value": {"include": ["connections"]}}),
        )["connections"],
        said
    );
    lines_export(
        &core,
        json!({"from": "Docs:Pay", "to": "pay.json", "format": "json-connections"}),
    );
    let pay = &cards(&text(&core, "pay.json"))[0];
    assert_eq!(pay["connections"], said, "{pay}");
}

#[test]
fn a_lines_entry_takes_the_cards_inside_a_card_and_a_ui_sections() {
    let core = core();
    // What a flow holds are cards with lines of their own: a card each, by its path.
    let save = ok(
        &core,
        "card_read",
        json!({"target": "Charts/Shop:Checkout.save", "value": {"include": ["connections"]}}),
    )["connections"]
        .clone();
    let touching: Vec<Value> = save
        .as_array()
        .unwrap()
        .iter()
        .filter(|l| l["line"] == "reads" || l["line"] == "writes")
        .cloned()
        .collect();
    assert_eq!(touching.len(), 2, "{save}");
    ok(
        &core,
        "export_create",
        json!({"target": "lines", "value": {"entries": [
            {"from": "Charts", "to": "lines.json", "format": "json-connections", "lines": ["reads", "writes"]},
            {"from": "Charts/Shop:Checkout", "to": "flow.json", "format": "json-connections"}]}}),
    );
    let under = cards(&text(&core, "lines.json"));
    let node = under
        .iter()
        .find(|c| c["card"] == "Charts/Shop:Checkout.save")
        .unwrap_or_else(|| panic!("{under:?}"));
    assert_eq!(node["kind"], "process");
    assert_eq!(node["connections"], Value::Array(touching));
    assert!(
        paths(&under).contains(&"Charts/Shop:Refund.ask"),
        "{under:?}"
    );

    // From a card: the card and what is inside it, nothing beside it.
    let flow = cards(&text(&core, "flow.json"));
    assert!(
        paths(&flow).contains(&"Charts/Shop:Checkout.save"),
        "{flow:?}"
    );
    assert!(
        paths(&flow)
            .iter()
            .all(|p| p.starts_with("Charts/Shop:Checkout")),
        "{flow:?}"
    );
    let inside = &flow
        .iter()
        .find(|c| c["card"] == "Charts/Shop:Checkout.save")
        .unwrap()["connections"];
    assert_eq!(inside, &save);

    // A card left out takes what is inside it along, from the entries above it — not from its own.
    ok(
        &core,
        "export_update",
        json!({"target": "lines", "value": {"exclude": {"add": ["Charts/Shop:Checkout"]}}}),
    );
    let left = cards(&text(&core, "lines.json"));
    assert!(
        paths(&left)
            .iter()
            .all(|p| !p.starts_with("Charts/Shop:Checkout")),
        "{left:?}"
    );
    assert!(paths(&left).contains(&"Charts/Shop:Refund.ask"), "{left:?}");
    assert_eq!(cards(&text(&core, "flow.json")), flow);

    // A ui section's cards are parts of its page to the other entries; their lines are lines.
    let mut tree = ok(&core, "project_read", json!({}))["projectTree"].clone();
    tree["main_sections"]
        .as_array_mut()
        .unwrap()
        .push(json!({"type": "ui", "name": "UI"}));
    ok(
        &core,
        "project_update",
        json!({"value": {"project_tree": tree}}),
    );
    ok(&core, "section_create", json!({"target": "UI/Home"}));
    ok(
        &core,
        "card_create",
        json!({"target": "UI/Home:Title", "value": {"kind": "html", "body": "<h1>支払い</h1>",
            "connections": [{"line": "link", "to": "Docs:Pay"}]}}),
    );
    ok(
        &core,
        "export_update",
        json!({"target": "lines", "value": {"entries": {"add": [
            {"from": "UI", "to": "ui.json", "format": "json-connections"}]}}}),
    );
    assert_eq!(
        cards(&text(&core, "ui.json")),
        [
            json!({"card": "UI/Home:Title", "kind": "html", "connections": [
            {"line": "link", "to": "Docs:Pay"}]})
        ]
    );

    // Written and kept as any card's: the file stands changed once a line of what is inside is.
    let folder = place("lines-inside");
    std::fs::create_dir_all(&folder).unwrap();
    let at = folder.to_str().unwrap();
    ok(
        &core,
        "export_execute",
        json!({"targetdir": at, "target": "lines"}),
    );
    let stands = |core: &Core| {
        let read = ok(
            core,
            "export_read",
            json!({"targetdir": at, "target": "lines"}),
        );
        read["exports"][0]["files"]
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["path"] == "flow.json")
            .unwrap_or_else(|| panic!("{read}"))["status"]
            .as_str()
            .unwrap()
            .to_string()
    };
    assert_eq!(stands(&core), "current");
    ok(
        &core,
        "card_update",
        json!({"target": "Charts/Shop:Checkout.save", "value": {"connections": {"add": [
            {"line": "link", "to": "Docs:Pay"}]}}}),
    );
    assert_eq!(stands(&core), "changed");
}

#[test]
fn a_lines_entry_writes_its_file_with_no_card_to_say() {
    let core = core();
    // No shape is in Docs: an entry narrowed to them has no card to say.
    lines_export(
        &core,
        json!({"from": "Docs", "to": "lines.json", "format": "json-connections", "kinds": ["note"], "lines": ["testedBy"]}),
    );
    assert_eq!(cards(&text(&core, "lines.json")), Vec::<Value>::new());
    let folder = place("lines-none");
    std::fs::create_dir_all(&folder).unwrap();
    let at = folder.to_str().unwrap();
    let execute = |core: &Core| {
        ok(
            core,
            "export_execute",
            json!({"targetdir": at, "target": "lines"}),
        )
    };
    let written = || std::fs::read_to_string(folder.join("lines.json")).unwrap();
    execute(&core);
    assert_eq!(
        serde_json::from_str::<Value>(&written()).unwrap(),
        json!({"cards": []})
    );

    // A line there is: said. Gone again: the file stays, saying none — not taken away, which a
    // reader could not tell from an export never written.
    ok(
        &core,
        "card_update",
        json!({"target": "Docs:Plain", "value": {"connections": {"add": [
            {"line": "testedBy", "to": "Docs:PayTest"}]}}}),
    );
    execute(&core);
    assert_eq!(paths(&cards(&written())), ["Docs:Plain"]);
    ok(
        &core,
        "card_update",
        json!({"target": "Docs:Plain", "value": {"connections": {"remove": [
            {"line": "testedBy", "to": "Docs:PayTest"}]}}}),
    );
    let read = ok(
        &core,
        "export_read",
        json!({"targetdir": at, "target": "lines"}),
    );
    assert!(read["exports"][0].get("removing").is_none(), "{read}");
    assert_eq!(read["exports"][0]["files"][0]["path"], "lines.json");
    assert_eq!(read["exports"][0]["files"][0]["status"], "changed");
    let said = execute(&core);
    assert!(
        said["exports"][0]["removed"]
            .as_array()
            .is_none_or(Vec::is_empty),
        "{said}"
    );
    assert_eq!(
        serde_json::from_str::<Value>(&written()).unwrap(),
        json!({"cards": []})
    );
}

#[test]
fn a_kind_of_line_gone_from_the_project_is_told_not_written_as_none() {
    let core = core();
    ok(
        &core,
        "kind_create",
        json!({"target": "cites", "value": {"of": "line", "label": "引く", "base": "link",
            "from": [{"kinds": ["note"], "at": "card"}], "to": {"kinds": ["*"], "at": ["card"]}}}),
    );
    lines_export(
        &core,
        json!({"from": "Docs", "to": "lines.json", "format": "json-connections", "lines": ["cites"]}),
    );
    ok(&core, "kind_delete", json!({"target": "cites"}));
    let folder = place("lines-kind-gone");
    std::fs::create_dir_all(&folder).unwrap();
    let at = folder.to_str().unwrap();
    let read = ok(&core, "export_read", json!({"targetdir": at}));
    let problems = read["problems"].to_string();
    assert!(
        problems.contains("cites") && problems.contains("lines.json"),
        "{read}"
    );
    let said = refused(
        &core,
        "export_execute",
        json!({"targetdir": at, "target": "lines"}),
    );
    assert!(said.contains("cites"), "{said}");
    assert!(!folder.join("lines.json").exists());
}
