//! A working directory's files read as code: a file's card says its language and its functions and
//! types, each read by its path past the file's (`repo/src:'a.rs'.Order.total`) from the file as it
//! is now, listed (`kind: symbol`), and reached by an implementation's line that lands at it.

use crate::common;

use common::{call, op};
use serde_json::{Value, json};
use specboard_core::Core;
use std::fs;
use std::path::{Path, PathBuf};

const PROJECT: &str = "見本";

fn ok(core: &Core, tool: &str, mut arguments: Value) -> Value {
    arguments["project"] = PROJECT.into();
    let (refused, text) = call(core, tool, arguments.clone());
    assert!(!refused, "{tool} {arguments}: {text}");
    serde_json::from_str(&text).unwrap()
}

fn refused(core: &Core, tool: &str, mut arguments: Value) -> String {
    arguments["project"] = PROJECT.into();
    let (refused, text) = call(core, tool, arguments.clone());
    assert!(refused, "{tool} {arguments} went through: {text}");
    text
}

/// A tool called as the window calls it (`tool`).
fn window(core: &Core, tool: &str, mut arguments: Value) -> Value {
    arguments["project"] = PROJECT.into();
    let said = op(
        core,
        json!({"op": "tool", "name": tool, "arguments": arguments}),
    );
    assert!(said.get("error").is_none(), "{tool}: {said}");
    said["result"]["json"].clone()
}

const A_RS: &str = r#"use std::fmt;

/// An order.
#[derive(Debug)]
pub struct Order {
    lines: Vec<u32>,
}

impl Order {
    /// A new one.
    pub fn new() -> Order {
        Order { lines: Vec::new() }
    }

    pub fn total(&self) -> u32 {
        self.lines.iter().sum::<u32>() + helper(1)
    }
}

impl fmt::Display for Order {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.total())
    }
}

fn helper(n: u32) -> u32 {
    n + 1
}
"#;

/// A repository of a few files: Rust, a header, JSON, a file of no language.
fn repository() -> PathBuf {
    let root = common::place("code");
    for (path, text) in [
        ("src/a.rs", A_RS),
        ("src/b.rs", "pub fn b() { helper(2); }\n"),
        ("src/c.h", "int c(void);\n"),
        ("data.json", "{\"a\": 1}\n"),
        ("Makefile", "all:\n\techo hi\n"),
    ] {
        let file = root.join(path);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(file, text).unwrap();
    }
    root
}

fn tree_with(core: &Core, extra: &[Value]) -> Value {
    let mut tree = ok(core, "project_read", json!({}))["projectTree"].clone();
    for e in extra {
        tree["main_sections"]
            .as_array_mut()
            .unwrap()
            .push(e.clone());
    }
    tree
}

/// The working directory `repo` listed in the project tree, and bound to `folder`.
fn bind(core: &Core, folder: &Path) {
    ok(
        core,
        "project_update",
        json!({"value": {"project_tree": tree_with(core, &[json!({"type": "workdir", "name": "repo"})])}}),
    );
    window(
        core,
        "section_update",
        json!({"target": "repo", "value": {"folder": folder.to_string_lossy()}}),
    );
}

fn core_with_repository() -> (Core, PathBuf) {
    let core = Core::open(common::script_store()).unwrap();
    let repo = repository();
    bind(&core, &repo);
    (core, repo)
}

#[test]
fn a_file_s_card_says_its_language_and_the_functions_and_types_at_its_top() {
    let (core, _repo) = core_with_repository();
    let read = ok(&core, "card_read", json!({"target": "repo/src:'a.rs'"}));
    assert_eq!(read["language"], "rust", "{read}");
    let symbols = read["symbols"].as_array().unwrap();
    let names: Vec<&str> = symbols
        .iter()
        .map(|s| s["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["Order", "helper"], "{read}");
    assert_eq!(symbols[0]["of"], "type");
    assert_eq!(symbols[0]["word"], "struct");
    assert_eq!(symbols[0]["line"], 5);
    assert_eq!(symbols[0]["lines"], json!({"from": 3, "to": 7}));
    assert_eq!(symbols[0]["members"], json!(["new", "total", "fmt"]));
    // Its text only when lines of it are asked for.
    assert!(read.get("body").is_none(), "{read}");
    let part = ok(
        &core,
        "card_read",
        json!({"target": "repo/src:'a.rs'", "value": {"lines": {"from": 15, "to": 17}}}),
    );
    assert_eq!(part["lines"], json!({"from": 15, "to": 17}), "{part}");
    assert!(
        part["body"]
            .as_str()
            .unwrap()
            .starts_with("    pub fn total"),
        "{part}"
    );
    // A file of no language says none; JSON is coloured, not read for functions.
    let make = ok(&core, "card_read", json!({"target": "repo:'Makefile'"}));
    assert!(
        make.get("language").is_none() && make.get("symbols").is_none(),
        "{make}"
    );
    let data = ok(&core, "card_read", json!({"target": "repo:'data.json'"}));
    assert_eq!(data["language"], "json");
    assert!(
        data.get("symbols").is_none() && data.get("problem").is_none(),
        "{data}"
    );
}

#[test]
fn a_function_is_read_by_its_path_past_its_file_s_with_the_file_s_line_numbers() {
    let (core, _repo) = core_with_repository();
    let read = ok(
        &core,
        "card_read",
        json!({"target": "repo/src:'a.rs'.Order.total"}),
    );
    assert_eq!(read["kind"], "symbol", "{read}");
    assert_eq!(read["path"], "repo/src:'a.rs'.Order.total");
    assert_eq!(read["of"], "method");
    assert_eq!(read["word"], "fn");
    assert_eq!(read["language"], "rust");
    assert_eq!(read["file"], "repo/src:'a.rs'");
    assert_eq!(read["line"], 15);
    assert_eq!(read["lines"], json!({"from": 15, "to": 17}));
    assert_eq!(read["declaration"], "pub fn total(&self) -> u32 {");
    assert_eq!(read["calls"], json!(["iter", "sum", "helper"]));
    assert!(read["body"].as_str().unwrap().ends_with("    }"), "{read}");
    // A type: its own text, and the blocks joined to it.
    let order = ok(
        &core,
        "card_read",
        json!({"target": "repo/src:'a.rs'.Order"}),
    );
    assert_eq!(order["lines"], json!({"from": 3, "to": 7}), "{order}");
    assert_eq!(
        order["parts"],
        json!([{"from": 9, "to": 18}, {"from": 20, "to": 24}])
    );
    assert_eq!(order["members"], json!(["new", "total", "fmt"]));
    // Lines by the file's numbers, cut to the function's.
    let part = ok(
        &core,
        "card_read",
        json!({"target": "repo/src:'a.rs'.Order.new", "value": {"lines": {"from": 1, "to": 11}}}),
    );
    assert_eq!(part["lines"], json!({"from": 10, "to": 11}), "{part}");
    let found = ok(
        &core,
        "card_read",
        json!({"target": "repo/src:'a.rs'.Order.total", "value": {"find": "helper"}}),
    );
    assert_eq!(found["found"][0]["line"], 16, "{found}");
    // A name not there: the nearest named.
    let said = refused(
        &core,
        "card_read",
        json!({"target": "repo/src:'a.rs'.Order.totl"}),
    );
    assert!(said.contains("Order.total"), "{said}");
    // Not written.
    let said = refused(
        &core,
        "card_update",
        json!({"target": "repo/src:'a.rs'.Order.total", "value": {"description": "x"}}),
    );
    assert!(said.contains("Order.total"), "{said}");
}

#[test]
fn a_function_is_read_from_its_file_as_it_is_now() {
    let (core, repo) = core_with_repository();
    let read = ok(&core, "card_read", json!({"target": "repo/src:'b.rs'.b"}));
    assert_eq!(read["calls"], json!(["helper"]));
    fs::write(
        repo.join("src/b.rs"),
        "pub fn b() { other(); }\n\npub fn c() {}\n",
    )
    .unwrap();
    let read = ok(&core, "card_read", json!({"target": "repo/src:'b.rs'.b"}));
    assert_eq!(read["calls"], json!(["other"]), "{read}");
    let read = ok(&core, "card_read", json!({"target": "repo/src:'b.rs'.c"}));
    assert_eq!(read["line"], 3, "{read}");
}

const ORDER: &str = "Schemas/Ordering:Order";

fn lines_of(core: &Core, path: &str) -> Value {
    ok(
        core,
        "card_read",
        json!({"target": path, "value": {"include": ["connections"]}}),
    )["connections"]
        .clone()
}

fn broken(core: &Core) -> Vec<Value> {
    common::checked(core, "main")["brokenPaths"]
        .as_array()
        .cloned()
        .unwrap_or_default()
}

#[test]
fn an_implementation_s_line_lands_at_a_function_kept_as_the_file_and_where_in_it() {
    let (core, repo) = core_with_repository();
    let written = ok(
        &core,
        "card_update",
        json!({"target": ORDER, "value": {"connections": {"add": [
            {"line": "implementedBy", "to": "repo/src:'a.rs'.Order.total"},
            {"line": "implementedBy", "from": "id", "to": "repo/src:'a.rs'.Order.discount"}
        ]}}}),
    );
    // Not there yet: written, and said so where the file was read.
    assert!(
        written["warnings"].to_string().contains("Order.discount"),
        "{written}"
    );
    let lines = lines_of(&core, ORDER);
    assert!(
        lines
            .as_array()
            .unwrap()
            .contains(&json!({"line": "implementedBy", "to": "repo/src:'a.rs'.Order.total"})),
        "{lines}"
    );
    // Kept as the file's id and the path in it.
    let kept: String = rusqlite::Connection::open(core.place())
        .unwrap()
        .query_row(
            "SELECT doc FROM card_docs WHERE branch_id = '' AND name = 'Order'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(kept.contains("\"at\":\"Order.total\""), "{kept}");
    // The check: the one not there, with the names near it.
    let said = broken(&core);
    let gone: Vec<&Value> = said
        .iter()
        .filter(|b| b["path"].as_str().unwrap_or_default().contains("discount"))
        .collect();
    assert_eq!(gone.len(), 1, "{said:?}");
    assert!(
        gone[0].to_string().contains("Order.total") || gone[0].to_string().contains("Order"),
        "{said:?}"
    );
    assert!(
        !said.iter().any(|b| b["path"]
            .as_str()
            .unwrap_or_default()
            .ends_with("Order.total")),
        "{said:?}"
    );
    // Made: no longer said.
    let mut text = A_RS.to_string();
    text = text.replace(
        "    pub fn total(&self)",
        "    pub fn discount(&self) -> u32 {\n        0\n    }\n\n    pub fn total(&self)",
    );
    fs::write(repo.join("src/a.rs"), text).unwrap();
    let said = broken(&core);
    assert!(
        !said.iter().any(|b| b.to_string().contains("discount")),
        "{said:?}"
    );
    // The other way round: the function, the type it is in and the file each say who lands there.
    let total = ok(
        &core,
        "card_read",
        json!({"target": "repo/src:'a.rs'.Order.total"}),
    );
    assert_eq!(total["referencedBy"], json!([ORDER]), "{total}");
    let order = ok(
        &core,
        "card_read",
        json!({"target": "repo/src:'a.rs'.Order"}),
    );
    assert_eq!(
        order["referencedBy"].as_array().unwrap().len(),
        2,
        "{order}"
    );
    let file = ok(&core, "card_read", json!({"target": "repo/src:'a.rs'"}));
    assert!(
        file["referencedBy"]
            .as_array()
            .unwrap()
            .contains(&json!({"path": format!("{ORDER}.id"), "at": "Order.discount"})),
        "{file}"
    );
    // What may be drawn to it: the implementation's line alone; none from it.
    let kinds = ok(
        &core,
        "card_read",
        json!({"target": ORDER, "value": {"as": "lineKinds", "to": "repo/src:'a.rs'.Order.total"}}),
    );
    let names: Vec<&str> = kinds["lines"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|l| l["line"].as_str())
        .collect();
    assert_eq!(names, ["implementedBy"], "{kinds}");
    refused(
        &core,
        "card_read",
        json!({"target": "repo/src:'a.rs'.Order.total", "value": {"as": "lineKinds"}}),
    );
    // Where it resolves.
    let exists = ok(
        &core,
        "card_read",
        json!({"target": ["repo/src:'a.rs'.Order.total", "repo/src:'a.rs'.Order.nope"], "value": {"as": "exists"}}),
    );
    assert_eq!(exists["results"][0]["resolved"], true, "{exists}");
    assert_eq!(exists["results"][0]["type"], "method", "{exists}");
    assert_eq!(exists["results"][1]["resolved"], false, "{exists}");
    assert!(
        exists["results"][1]["hint"]
            .as_str()
            .unwrap()
            .contains("Order.new"),
        "{exists}"
    );
}

#[test]
fn the_functions_and_types_of_a_section_s_files_are_listed_by_what_they_are_and_their_names() {
    let (core, _repo) = core_with_repository();
    let listed = ok(
        &core,
        "card_list",
        json!({"value": {"kind": "symbol", "section": "repo/src", "of": "function"}}),
    );
    let paths: Vec<&str> = listed["cards"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["path"].as_str().unwrap())
        .collect();
    assert_eq!(
        paths,
        [
            "repo/src:'a.rs'.helper",
            "repo/src:'b.rs'.b",
            "repo/src:'c.h'.c"
        ],
        "{listed}"
    );
    // Every working directory, by a word in the name.
    let listed = ok(
        &core,
        "card_list",
        json!({"value": {"kind": "symbol", "name": "TOT"}}),
    );
    assert_eq!(
        listed["cards"][0]["path"], "repo/src:'a.rs'.Order.total",
        "{listed}"
    );
    assert_eq!(listed["cards"][0]["of"], "method");
    assert_eq!(listed["matched"], 1);
    // A list of cards lists no functions.
    let cards = ok(
        &core,
        "card_list",
        json!({"value": {"section": "repo/src", "recursive": true, "limit": 200}}),
    );
    assert!(!cards.to_string().contains("Order.total"), "{cards}");
    let said = refused(
        &core,
        "card_list",
        json!({"value": {"kind": "symbol", "of": "class"}}),
    );
    assert!(said.contains("function"), "{said}");
}

#[test]
fn a_working_directory_maps_its_files_to_languages_its_own_way() {
    let (core, _repo) = core_with_repository();
    let read = ok(&core, "section_read", json!({"target": "repo"}));
    let parsers = read["parsers"].as_array().unwrap();
    assert!(
        parsers.contains(&json!({"match": ".h", "language": "c"})),
        "{read}"
    );
    let languages = read["languages"].as_array().unwrap();
    assert!(
        languages.contains(
            &json!({"id": "rust", "source": "bundled", "symbols": true, "highlight": true})
        ),
        "{read}"
    );
    assert!(
        languages.contains(
            &json!({"id": "json", "source": "bundled", "symbols": false, "highlight": true})
        ),
        "{read}"
    );
    assert!(read.get("problems").is_none(), "{read}");
    let header = ok(&core, "card_read", json!({"target": "repo/src:'c.h'"}));
    assert_eq!(header["language"], "c");
    // Set: the header as C++, Makefile as Bash, a language another machine has.
    let set = window(
        &core,
        "section_update",
        json!({"target": "repo", "value": {"parsers": {".h": "cpp", "Makefile": "bash", ".zig": "zig"}}}),
    );
    assert!(
        set["parsers"]
            .as_array()
            .unwrap()
            .contains(&json!({"match": ".h", "language": "cpp", "override": true})),
        "{set}"
    );
    assert!(set["problems"].to_string().contains("zig"), "{set}");
    let header = ok(&core, "card_read", json!({"target": "repo/src:'c.h'"}));
    assert_eq!(header["language"], "cpp", "{header}");
    let make = ok(&core, "card_read", json!({"target": "repo:'Makefile'"}));
    assert_eq!(make["language"], "bash", "{make}");
    // The project's: in its file, and back to the table's with null.
    let project = common::project_id(&core);
    let file = op(&core, json!({"op": "project_file", "project": project}));
    let text = file["result"]["text"].as_str().unwrap();
    assert!(text.contains("\"parsers\""), "{text}");
    window(
        &core,
        "section_update",
        json!({"target": "repo", "value": {"parsers": null}}),
    );
    let header = ok(&core, "card_read", json!({"target": "repo/src:'c.h'"}));
    assert_eq!(header["language"], "c", "{header}");
    // Not of a form it takes; not in a batch.
    let said = refused(
        &core,
        "section_update",
        json!({"target": "repo", "value": {"parsers": {"src/x.h": "cpp"}}}),
    );
    assert!(said.contains("src/x.h"), "{said}");
    let said = refused(
        &core,
        "batch",
        json!({"value": {"steps": [{"tool": "section_update", "arguments": {"target": "repo", "value": {"parsers": {".h": "cpp"}}}}]}}),
    );
    assert!(!said.is_empty());
}

#[test]
fn a_change_reaches_the_functions_that_carry_it_out_and_those_that_call_them() {
    let (core, _repo) = core_with_repository();
    ok(
        &core,
        "card_update",
        json!({"target": ORDER, "value": {"connections": {"add": [
            {"line": "implementedBy", "to": "repo/src:'a.rs'.helper"},
            {"line": "implementedBy", "to": "repo/src:'b.rs'"}
        ]}}}),
    );
    let impact = ok(
        &core,
        "card_read",
        json!({"target": ORDER, "value": {"as": "impact"}}),
    );
    let files = impact["files"].as_array().unwrap();
    assert!(
        files.contains(&json!({"file": "repo/src:'a.rs'", "for": ORDER, "symbol": "helper", "lines": {"from": 26, "to": 28}})),
        "{impact}"
    );
    assert!(
        files.contains(&json!({"file": "repo/src:'b.rs'", "for": ORDER})),
        "{impact}"
    );
    // A function's: what implements it, and who calls it by its name.
    let helper = ok(
        &core,
        "card_read",
        json!({"target": "repo/src:'a.rs'.helper", "value": {"as": "impact"}}),
    );
    assert_eq!(helper["implementedBy"], json!([ORDER]), "{helper}");
    assert_eq!(
        helper["callers"],
        json!([
            {"path": "repo/src:'a.rs'.Order.total", "approx": true},
            {"path": "repo/src:'b.rs'.b", "approx": true}
        ]),
        "{helper}"
    );
}

#[test]
fn a_text_names_a_function_by_its_path() {
    let (core, _repo) = core_with_repository();
    ok(
        &core,
        "card_create",
        json!({"target": "Docs:CodeNote", "value": {"kind": "note",
            "body": "See [[repo/src:'a.rs'.Order.total]] and [[repo/src:'a.rs'.Order.gone]]."}}),
    );
    let said = broken(&core);
    let paths: Vec<&str> = said
        .iter()
        .filter_map(|b| b["path"].as_str())
        .filter(|p| p.contains("a.rs"))
        .collect();
    assert_eq!(paths, ["repo/src:'a.rs'.Order.gone"], "{said:?}");
    // What the window draws: a mention landing at the function in the file.
    let project = common::project_id(&core);
    let detail = op(&core, json!({"op": "project_detail", "project": project}));
    let mention = detail["result"]["relations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| {
            r["line"] == "mentions"
                && r["toSchemaId"]
                    .as_str()
                    .is_some_and(|t| t.starts_with("file:"))
        })
        .cloned();
    assert!(mention.is_some(), "{}", detail["result"]["relations"]);
    assert_eq!(mention.unwrap()["at"], "Order.total");
}

/// A file's id as the window holds it, read from its card.
fn file_id(core: &Core, path: &str) -> String {
    let project = common::project_id(core);
    let detail = op(core, json!({"op": "workdir_detail", "project": project}));
    let file_path = ok(core, "card_read", json!({"target": path}))["filePath"]
        .as_str()
        .unwrap()
        .to_string();
    detail["result"]["schemas"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["form"]["fields"]["filePath"] == file_path.as_str())
        .unwrap_or_else(|| panic!("{detail}"))["id"]
        .as_str()
        .unwrap()
        .to_string()
}

#[test]
fn the_window_reads_a_file_s_outline_its_colours_and_where_a_function_is() {
    let (core, repo) = core_with_repository();
    let project = common::project_id(&core);
    // Each file's language before it is read.
    let detail = op(&core, json!({"op": "workdir_detail", "project": project}));
    let a = detail["result"]["schemas"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["form"]["fields"]["filePath"] == "src/a.rs")
        .cloned()
        .unwrap();
    assert_eq!(a["form"]["fields"]["language"], "rust", "{a}");
    let a_id = file_id(&core, "repo/src:'a.rs'");
    let json_id = file_id(&core, "repo:'data.json'");
    let outline = op(
        &core,
        json!({"op": "file_outline", "project": project, "ids": [a_id, json_id]}),
    )["result"]
        .clone();
    let a = &outline["files"][0];
    assert_eq!(a["language"], "rust", "{outline}");
    let symbols = a["symbols"].as_array().unwrap();
    assert_eq!(
        symbols[1],
        json!({"path": "Order.new", "name": "new", "of": "method", "word": "fn", "line": 11, "from": 10, "to": 13, "parent": "Order"}),
        "{outline}"
    );
    let data = &outline["files"][1];
    assert_eq!(data["language"], "json");
    assert!(
        data.get("symbols").is_none() && data.get("problem").is_none(),
        "{data}"
    );
    // The same text: answered unchanged.
    let stamp = a["stamp"].as_str().unwrap().to_string();
    let again = op(
        &core,
        json!({"op": "file_outline", "project": project, "ids": [a_id], "since": {a_id.clone(): stamp.clone()}}),
    )["result"]
        .clone();
    assert_eq!(again["files"][0]["unchanged"], true, "{again}");
    assert!(again["files"][0].get("symbols").is_none());
    fs::write(repo.join("src/a.rs"), format!("// moved\n{A_RS}")).unwrap();
    let moved = op(
        &core,
        json!({"op": "file_outline", "project": project, "ids": [a_id], "since": {a_id.clone(): stamp}}),
    )["result"]
        .clone();
    assert!(moved["files"][0].get("unchanged").is_none(), "{moved}");
    assert_eq!(moved["files"][0]["symbols"][1]["line"], 12, "{moved}");
    // Coloured, in UTF-16 offsets; and where a function is.
    let text = op(
        &core,
        json!({"op": "file_text", "project": project, "id": a_id, "highlight": true, "symbol": "Order.total"}),
    )["result"]
        .clone();
    assert_eq!(text["language"], "rust", "{text}");
    assert_eq!(text["classes"][0], "keyword");
    let spans = text["spans"].as_array().unwrap();
    assert!(spans.len() > 30 && spans.len().is_multiple_of(3), "{text}");
    let units: Vec<u16> = text["text"].as_str().unwrap().encode_utf16().collect();
    let first = |class: &str| -> String {
        let index = text["classes"]
            .as_array()
            .unwrap()
            .iter()
            .position(|c| c == class)
            .unwrap() as u64;
        let at = spans
            .chunks(3)
            .find(|s| s[2].as_u64() == Some(index))
            .unwrap();
        let (from, len) = (
            at[0].as_u64().unwrap() as usize,
            at[1].as_u64().unwrap() as usize,
        );
        String::from_utf16(&units[from..from + len]).unwrap()
    };
    assert_eq!(first("comment"), "// moved");
    assert_eq!(first("keyword"), "use");
    assert_eq!(
        text["focus"],
        json!({"from": 16, "to": 18, "line": 16, "path": "Order.total"})
    );
    let gone = op(
        &core,
        json!({"op": "file_text", "project": project, "id": a_id, "symbol": "Order.totl"}),
    )["result"]
        .clone();
    assert!(
        gone["focusProblem"]
            .as_str()
            .unwrap()
            .contains("Order.total"),
        "{gone}"
    );
}
