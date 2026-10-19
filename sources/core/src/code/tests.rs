//! Each language's functions and types found in a small source of it (`tests/fixtures/code/`),
//! held to the answer kept (`tests/fixtures/kept/symbols/`): a grammar's node names change between
//! its versions, and these fail first.

use super::registry::{Parsers, language_of};
use super::symbols::Symbol;

/// The outline as kept: one line for each, indented by how deep it is.
fn said(symbols: &[Symbol], errors: usize) -> String {
    let mut out = String::new();
    for s in symbols {
        let mut depth = 0;
        let mut at = s.parent;
        while let Some(p) = at {
            depth += 1;
            at = symbols[p].parent;
        }
        out.push_str(&"  ".repeat(depth));
        out.push_str(&format!(
            "{} — {} {} — line {} — {}-{}",
            s.path,
            s.of.word(),
            s.word,
            s.line,
            s.span.from,
            s.span.to
        ));
        if !s.parts.is_empty() {
            let parts: Vec<String> = s
                .parts
                .iter()
                .map(|p| format!("{}-{}", p.from, p.to))
                .collect();
            out.push_str(&format!(" — parts {}", parts.join(", ")));
        }
        if s.made {
            out.push_str(" — made");
        }
        if !s.calls.is_empty() {
            out.push_str(&format!(" — calls {}", s.calls.join(", ")));
        }
        out.push('\n');
    }
    if errors > 0 {
        out.push_str(&format!("parse errors: {errors}\n"));
    }
    out
}

fn outline(file: &str) -> String {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/code")
        .join(file);
    let text = std::fs::read(&path).unwrap();
    let language = language_of(file, &Parsers::new()).unwrap().unwrap();
    let read = super::read_text(language, &text);
    assert!(read.problem.is_none(), "{file}: {:?}", read.problem);
    said(&read.found.symbols, read.found.parse_errors)
}

fn check(file: &str) {
    let got = outline(file);
    crate::fixtures::kept(&format!("symbols/{file}.txt"), &got);
}

#[test]
fn rust_s_impls_join_their_types_and_its_doc_comments_and_attributes_are_in_their_ranges() {
    check("sample.rs");
}

#[test]
fn swift_s_extensions_join_their_types_and_its_inits_are_numbered() {
    check("sample.swift");
}

#[test]
fn typescript_s_exports_and_decorators_are_in_their_ranges_and_its_overloads_are_numbered() {
    check("sample.ts");
}

#[test]
fn tsx_reads_as_typescript_does() {
    check("sample.tsx");
}

#[test]
fn javascript_s_functions_held_by_a_name_are_found_by_it() {
    check("sample.js");
}

/// A function's own returned object, a class's fields and a const's object holding functions, a
/// const made by a call given a function, and a test's calls are found; a callback's object, an
/// object passed to a call and a test called inside a function are not.
#[test]
fn typescript_s_functions_held_in_objects_fields_and_test_calls_are_found() {
    check("shapes.ts");
}

#[test]
fn tsx_finds_them_as_typescript_does() {
    assert_eq!(outline("shapes.tsx"), outline("shapes.ts"));
}

#[test]
fn javascript_finds_them_as_typescript_does() {
    check("shapes.js");
}

#[test]
fn python_s_decorators_are_in_their_ranges_and_its_capitals_are_constants() {
    check("sample.py");
}

#[test]
fn go_s_methods_join_their_receivers_types() {
    check("sample.go");
}

#[test]
fn c_s_types_are_those_with_bodies() {
    check("sample.c");
}

#[test]
fn cpp_s_members_defined_outside_their_class_are_named_with_it() {
    check("sample.cpp");
}

#[test]
fn java_s_constructors_are_numbered_and_its_annotations_are_in_their_ranges() {
    check("sample.java");
}

#[test]
fn csharp_s_namespaces_hold_their_types() {
    check("sample.cs");
}

#[test]
fn ruby_s_modules_hold_their_classes() {
    check("sample.rb");
}

#[test]
fn php_s_classes_hold_their_methods_and_constants() {
    check("sample.php");
}

#[test]
fn kotlin_s_kinds_of_class_are_told_apart() {
    check("sample.kt");
}

#[test]
fn sql_s_tables_views_and_functions_are_found() {
    check("sample.sql");
}

#[test]
fn bash_s_functions_are_found() {
    check("sample.sh");
}

/// The words a span covers, by its UTF-16 offsets over `text`.
fn covered(text: &str, spans: &[u32]) -> Vec<(String, &'static str)> {
    let units: Vec<u16> = text.encode_utf16().collect();
    spans
        .chunks(3)
        .map(|s| {
            let (from, len, class) = (s[0] as usize, s[1] as usize, s[2] as usize);
            (
                String::from_utf16(&units[from..from + len]).unwrap(),
                super::highlight::CLASSES[class],
            )
        })
        .collect()
}

#[test]
fn a_text_s_colours_are_spans_of_utf16_offsets_past_characters_of_two_units_and_more() {
    let text = "// 日本語 😀\nfn main() { let s = \"é😀\"; }\n";
    let rust = super::registry::registry().find("rust").unwrap();
    let spans = super::highlight::spans(rust, text).unwrap().unwrap();
    let got = covered(text, &spans);
    assert!(
        got.contains(&("// 日本語 😀".to_string(), "comment")),
        "{got:?}"
    );
    assert!(got.contains(&("fn".to_string(), "keyword")), "{got:?}");
    assert!(got.contains(&("main".to_string(), "function")), "{got:?}");
    assert!(got.contains(&("\"é😀\"".to_string(), "string")), "{got:?}");
}

#[test]
fn every_sample_is_coloured() {
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/code");
    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    files.sort();
    for file in files {
        let language = language_of(&file, &Parsers::new()).unwrap().unwrap();
        let text = std::fs::read_to_string(dir.join(&file)).unwrap();
        let spans = super::highlight::spans(language, &text).unwrap().unwrap();
        let classes: std::collections::BTreeSet<&str> =
            covered(&text, &spans).into_iter().map(|(_, c)| c).collect();
        assert!(
            classes.contains("comment") && classes.contains("keyword"),
            "{file}: {classes:?}"
        );
    }
    // Markdown's inline text and code fences are coloured in the languages they name.
    let md = "# Title\n\nSome `code` and **bold**.\n\n```rust\nfn main() {}\n```\n";
    let markdown = super::registry::registry().find("markdown").unwrap();
    let got = covered(md, &super::highlight::spans(markdown, md).unwrap().unwrap());
    assert!(
        got.iter().any(|(w, c)| w == "fn" && *c == "keyword"),
        "{got:?}"
    );
}

/// The core's own sources bound as a working directory: listing every function and type in them
/// parses each file once, side by side, and main read as the window and MCP read it parses none
/// and takes no longer for them.
#[test]
fn a_repository_s_functions_cost_the_reads_of_main_nothing() {
    use serde_json::json;
    let core = crate::Core::open(crate::fixtures::store_with(&["sample"])).unwrap();
    let project: String = rusqlite::Connection::open(core.place())
        .unwrap()
        .query_row("SELECT id FROM projects WHERE name = '見本'", [], |r| {
            r.get(0)
        })
        .unwrap();
    let call = |request: serde_json::Value| -> serde_json::Value {
        let said: serde_json::Value =
            serde_json::from_str(&core.call(&request.to_string())).unwrap();
        assert!(said.get("error").is_none(), "{request}: {said}");
        said
    };
    let sources = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    call(
        json!({"op": "tool", "name": "section_update", "arguments": {
            "project": "見本", "target": "ts", "value": {"folder": sources.to_string_lossy()}
        }}),
    );
    let detail = json!({"op": "project_detail", "project": project});
    let list = json!({"op": "tool", "name": "card_list", "arguments": {"project": "見本", "value": {"limit": 200}}});
    let least = |request: &serde_json::Value| -> std::time::Duration {
        (0..4)
            .map(|_| {
                let at = std::time::Instant::now();
                call(request.clone());
                at.elapsed()
            })
            .min()
            .unwrap()
    };
    let (detail_before, list_before) = (least(&detail), least(&list));
    let parsed = super::cache::parsed();
    let at = std::time::Instant::now();
    let symbols = call(json!({"op": "tool", "name": "card_list", "arguments": {
        "project": "見本", "value": {"kind": "symbol", "section": "ts", "of": "type", "limit": 200}
    }}));
    let first = at.elapsed();
    let files = super::cache::parsed() - parsed;
    let at = std::time::Instant::now();
    call(json!({"op": "tool", "name": "card_list", "arguments": {
        "project": "見本", "value": {"kind": "symbol", "section": "ts", "of": "function", "limit": 200}
    }}));
    let again = at.elapsed();
    let found = symbols["result"]["json"]["matched"].as_u64().unwrap();
    eprintln!("{files} files parsed in {first:?} ({found} types); listed again in {again:?}");
    assert!(files > 100 && found > 100, "{files} {found}");
    let before_reads = super::cache::parsed();
    let (detail_after, list_after) = (least(&detail), least(&list));
    assert_eq!(
        super::cache::parsed(),
        before_reads,
        "main's reads parse nothing"
    );
    eprintln!(
        "project_detail {detail_before:?} → {detail_after:?}, card_list {list_before:?} → {list_after:?}"
    );
    let room = |before: std::time::Duration| before * 2 + std::time::Duration::from_millis(50);
    assert!(
        detail_after <= room(detail_before),
        "{detail_before:?} → {detail_after:?}"
    );
    assert!(
        list_after <= room(list_before),
        "{list_before:?} → {list_after:?}"
    );
    // Read again, nothing parsed again: the files are as they were.
    assert_eq!(super::cache::parsed(), before_reads);
}

/// What reading this repository's Rust and Swift costs, said and not held to a number (a machine's
/// speed is its own): `cargo test --release --lib measure_the_repository -- --ignored --nocapture`.
#[test]
#[ignore]
fn measure_the_repository() {
    let top = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut files = Vec::new();
    fn walk(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') || name == "target" || name == "build" {
                continue;
            }
            if path.is_dir() {
                walk(&path, out);
            } else if name.ends_with(".rs") || name.ends_with(".swift") {
                out.push(path);
            }
        }
    }
    walk(&top.join("sources"), &mut files);
    let none = Parsers::new();
    let wanted: Vec<_> = files
        .iter()
        .map(|p| {
            let l = language_of(&p.to_string_lossy(), &none).unwrap().unwrap();
            (p.clone(), l, "measure".to_string())
        })
        .collect();
    let bytes: u64 = files
        .iter()
        .map(|p| std::fs::metadata(p).unwrap().len())
        .sum();
    // One thread, nothing kept.
    super::cache::forget_dir("measure");
    let at = std::time::Instant::now();
    let mut symbols = 0;
    for (p, l, d) in &wanted {
        symbols += super::cache::load(p, l, d, false)
            .unwrap()
            .read
            .found
            .symbols
            .len();
    }
    let one = at.elapsed();
    super::cache::forget_dir("measure");
    let at = std::time::Instant::now();
    let read = super::cache::load_many(&wanted);
    let many = at.elapsed();
    assert!(read.iter().all(Result::is_ok));
    let at = std::time::Instant::now();
    super::cache::load_many(&wanted);
    let kept = at.elapsed();
    let at = std::time::Instant::now();
    let mut spans = 0;
    for (p, l, _) in &wanted {
        let text = std::fs::read_to_string(p).unwrap();
        spans += super::highlight::spans(l, &text).unwrap().unwrap().len() / 3;
    }
    let coloured = at.elapsed();
    eprintln!(
        "{} files, {:.1} MB, {symbols} symbols: one thread {one:?}, side by side {many:?}, kept {kept:?}; coloured in {coloured:?} ({spans} spans)",
        files.len(),
        bytes as f64 / 1e6
    );
}

/// What is kept of a file is used while the file is as it was — its time changed with its text
/// the same too —, read anew once its text changed, and a file too large is said so unread.
#[test]
fn a_file_is_parsed_again_only_when_its_text_changed() {
    let folder = crate::fixtures::folder("cache");
    let path = folder.join("a.rs");
    std::fs::write(&path, "fn a() {}\n").unwrap();
    let rust = super::registry::registry().find("rust").unwrap();
    let load = || super::cache::load(&path, rust, "cache-test", false).unwrap();
    let first = load();
    assert!(std::sync::Arc::ptr_eq(&first.read, &load().read));
    // Touched, the same text: what was kept.
    let later = std::time::SystemTime::now() + std::time::Duration::from_secs(5);
    std::fs::File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_modified(later)
        .unwrap();
    let touched = load();
    assert!(std::sync::Arc::ptr_eq(&first.read, &touched.read));
    assert_eq!(first.stamp, touched.stamp);
    // Its text changed: read anew.
    std::fs::write(&path, "fn a() {}\nfn b() {}\n").unwrap();
    let changed = load();
    assert!(!std::sync::Arc::ptr_eq(&first.read, &changed.read));
    assert_ne!(first.stamp, changed.stamp);
    assert_eq!(changed.read.found.symbols.len(), 2);
    // Its language's mapping changed: what was kept of the directory let go.
    super::cache::forget_dir("cache-test");
    assert!(!std::sync::Arc::ptr_eq(&changed.read, &load().read));
    // Too large: said so, not parsed.
    let big = folder.join("big.rs");
    std::fs::write(&big, "// x\n".repeat(super::MOST_BYTES / 5 + 1)).unwrap();
    let read = super::cache::load(&big, rust, "cache-test", false).unwrap();
    assert_eq!(
        read.read.problem.as_ref().map(|k| k.key()),
        Some("code.tooLarge")
    );
    assert!(read.read.found.symbols.is_empty());
}

/// A file's functions and types are cut at the most kept, and what one calls at the most named; a
/// tree with places the parser could not read still gives what it could.
#[test]
fn what_is_found_is_cut_at_the_most_and_read_past_what_cannot_be_parsed() {
    let rust = super::registry::registry().find("rust").unwrap();
    let many: String = (0..super::symbols::MOST_SYMBOLS + 10)
        .map(|i| format!("fn f{i}() {{}}\n"))
        .collect();
    let read = super::read_text(rust, many.as_bytes());
    assert_eq!(read.found.symbols.len(), super::symbols::MOST_SYMBOLS);
    assert_eq!(read.problem.as_ref().map(|k| k.key()), Some("code.cut"));
    let calls: String = (0..super::symbols::MOST_CALLS + 10)
        .map(|i| format!("g{i}();"))
        .collect();
    let read = super::read_text(rust, format!("fn f() {{ {calls} }}").as_bytes());
    assert_eq!(
        read.found.symbols[0].calls.len(),
        super::symbols::MOST_CALLS
    );
    let broken = "fn a() {}\nfn b( {\nfn c() {}\n";
    let read = super::read_text(rust, broken.as_bytes());
    assert!(read.found.parse_errors > 0, "{:?}", read.found);
    assert!(read.found.symbols.iter().any(|s| s.name == "a"));
}
