//! What the core says to a person or an agent — the tools and the guide (`src/mcp/catalog.json`,
//! `src/mcp/tools.json`), the kinds built in (`src/kinds/builtin.json`), and the strings in its
//! code but what is built only for its tests — carries no note
//! of how it was made: no owner's date (`オーナー 2026-10-05`), no issue's number (`（#81）`),
//! no name of the debugging machine (`<machine> の issue #3 の続き, オーナー 2026-10-06`). A colour
//! (`#2F6FEB`) is not a number.
//!
//! Nor do the comments of the code, the core's and the app's, their tests' and the tools', nor the
//! titles of the app's tests: a comment says why the code is as it is, and how it came to be — who
//! decided it when, the board's goal or task, the issue — goes in the board's task and the commit
//! (`CLAUDE.md`). A link to a design (`specboard://SpecBoard/Docs/Design:…`) is not a note.

use std::path::{Path, PathBuf};

/// Where a note of the making is in `text`, if it is: the words found.
fn note(text: &str) -> Option<String> {
    // The machine's name is joined at compile time, so that this file does not hold the word that the
    // check of the published tree forbids.
    for word in ["オーナー 20", "オーナー　20", concat!("debug", "-mac")] {
        if text.contains(word) {
            return Some(word.to_string());
        }
    }
    // `#` and two or three digits not running on into a colour's hex.
    let chars: Vec<char> = text.chars().collect();
    for (i, c) in chars.iter().enumerate() {
        if *c != '#' {
            continue;
        }
        let digits = chars[i + 1..]
            .iter()
            .take_while(|c| c.is_ascii_digit())
            .count();
        let next = chars.get(i + 1 + digits);
        if (2..=3).contains(&digits) && !next.is_some_and(|c| c.is_ascii_hexdigit()) {
            return Some(chars[i..i + 1 + digits].iter().collect());
        }
    }
    None
}

/// The string literals of a Rust file, by their lines, its comments left out and what is built only
/// for its tests: the item under each `#[cfg(test)]` — a module of a file of its own (`mod tests;`,
/// that file left out by `test_only`), or one written there (`mod tests { … }`, `fn …() { … }`) to
/// the brace that closes it at its own indent (the code is formatted). What comes after it is read.
fn literals(source: &str) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    // The indent of the `#[cfg(test)]` whose item is left out, and whether the item has begun.
    let mut skipping: Option<(usize, bool)> = None;
    for (n, line) in source.lines().enumerate() {
        let trimmed = line.trim();
        let indent = line.len() - line.trim_start().len();
        if let Some((at, begun)) = skipping {
            if !begun {
                if trimmed.starts_with("#[") || trimmed.starts_with("//") || trimmed.is_empty() {
                    continue;
                }
                // `mod tests;`, or an item closed on its one line.
                skipping =
                    (!trimmed.ends_with(';') && !trimmed.ends_with('}')).then_some((at, true));
            } else if indent == at && trimmed.starts_with('}') {
                skipping = None;
            }
            continue;
        }
        if trimmed == "#[cfg(test)]" {
            skipping = Some((indent, false));
            continue;
        }
        let code = line.split("//").next().unwrap_or_default();
        let mut inside = false;
        let mut current = String::new();
        let mut escaped = false;
        for c in code.chars() {
            if inside {
                if escaped {
                    escaped = false;
                    current.push(c);
                } else if c == '\\' {
                    escaped = true;
                } else if c == '"' {
                    out.push((n + 1, std::mem::take(&mut current)));
                    inside = false;
                } else {
                    current.push(c);
                }
            } else if c == '"' {
                inside = true;
            }
        }
    }
    out
}

/// The files of the modules `source` (the file at `path`) builds only for its tests
/// (`#[cfg(test)] mod fixtures;`).
fn test_only(path: &Path, source: &str) -> Vec<PathBuf> {
    let dir = match path.file_stem().and_then(|s| s.to_str()) {
        Some("lib" | "mod" | "main") => path.parent().unwrap().to_path_buf(),
        _ => path.with_extension(""),
    };
    let lines: Vec<&str> = source.lines().map(str::trim).collect();
    lines
        .windows(2)
        .filter(|pair| pair[0] == "#[cfg(test)]")
        .filter_map(|pair| {
            let name = pair[1]
                .trim_start_matches("pub(crate) ")
                .trim_start_matches("pub(super) ")
                .strip_prefix("mod ")?
                .strip_suffix(';')?;
            Some(dir.join(format!("{name}.rs")))
        })
        .collect()
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            if path.file_name().is_some_and(|n| n == "tests") {
                continue;
            }
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs")
            && path.file_name().is_none_or(|n| n != "tests.rs")
        {
            out.push(path);
        }
    }
}

#[test]
fn what_the_core_says_carries_no_note_of_its_making() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut found = Vec::new();
    let mut tables: Vec<String> = ["mcp/catalog.json", "mcp/tools.json", "kinds/builtin.json"]
        .map(String::from)
        .to_vec();
    // Every language's tables of what the core says (`text/<language>/<area>.json`).
    for language in std::fs::read_dir(root.join("text")).unwrap() {
        let language = language.unwrap().path();
        if !language.is_dir() {
            continue;
        }
        let code = language.file_name().unwrap().to_string_lossy().into_owned();
        for entry in std::fs::read_dir(&language).unwrap() {
            let name = entry.unwrap().file_name().to_string_lossy().into_owned();
            if name.ends_with(".json") {
                tables.push(format!("text/{code}/{name}"));
            }
        }
    }
    for name in &tables {
        let text = std::fs::read_to_string(root.join(name)).unwrap();
        for (n, line) in text.lines().enumerate() {
            if let Some(word) = note(line) {
                found.push(format!("src/{name}:{}: {word}", n + 1));
            }
        }
    }
    let mut files = Vec::new();
    rust_files(&root, &mut files);
    let left_out: Vec<PathBuf> = files
        .iter()
        .flat_map(|path| test_only(path, &std::fs::read_to_string(path).unwrap()))
        .collect();
    assert!(
        left_out.iter().any(|p| p.ends_with("src/fixtures.rs")),
        "{left_out:?}"
    );
    for path in files.iter().filter(|p| !left_out.contains(p)) {
        let source = std::fs::read_to_string(path).unwrap();
        for (n, literal) in literals(&source) {
            if let Some(word) = note(&literal) {
                let shown = path.strip_prefix(&root).unwrap().display().to_string();
                found.push(format!("src/{shown}:{n}: {word}"));
            }
        }
    }
    assert!(
        found.is_empty(),
        "開発のメモが文に紛れ込んでいます:\n{}",
        found.join("\n")
    );
}

/// Where a comment says how the code came to be, if it does: the words found. A note of the
/// making (`note`), or a goal or task of the board by its name.
fn history(text: &str) -> Option<String> {
    // A goal or task named (`the board's goal `Goals:X``), not the kinds spoken of (the board's goal
    // and task) nor a path written as an example.
    for word in [
        "the board's goal `",
        "the board's goals `",
        "the board's task `",
        "ボードの目標 `",
        "ボードの goal `",
        "ボードの task `",
        concat!("Schema", "Planner #"),
        "issue #",
    ] {
        if text.contains(word) {
            return Some(word.to_string());
        }
    }
    note(text)
}

/// The comments of a Rust or Swift file, by their lines: a line that is one (`//`, `///`, `//!`),
/// or what follows `//` after code when the code before it closes every string it opens.
fn comments(source: &str) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    for (n, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("//") {
            out.push((n + 1, trimmed.to_string()));
            continue;
        }
        if let Some((at, _)) = line
            .match_indices(" // ")
            .find(|(at, _)| line[..*at].matches('"').count() % 2 == 0)
        {
            out.push((n + 1, line[at..].to_string()));
        }
    }
    out
}

/// The titles of a Swift file's tests and suites (`@Test("…")`, `@Suite("…")`), by their lines.
fn titles(source: &str) -> Vec<(usize, String)> {
    source
        .lines()
        .enumerate()
        .filter(|(_, line)| {
            let t = line.trim_start();
            t.starts_with("@Test(") || t.starts_with("@Suite(")
        })
        .map(|(n, line)| (n + 1, line.to_string()))
        .collect()
}

/// The files under `dir` with one of `extensions`, those under a folder named in `skipping` left out.
fn files(dir: &Path, extensions: &[&str], skipping: &[&str], out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries {
        let path = entry.unwrap().path();
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        if path.is_dir() {
            if !skipping.contains(&name) && name != "target" {
                files(&path, extensions, skipping, out);
            }
        } else if path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| extensions.contains(&e))
        {
            out.push(path);
        }
    }
}

#[test]
fn the_comments_carry_no_note_of_how_the_code_came_to_be() {
    let core = Path::new(env!("CARGO_MANIFEST_DIR"));
    let repo = core.parent().unwrap().parent().unwrap();
    let mut code = Vec::new();
    for dir in ["src", "tests", "agent", "ffi", "bindgen"] {
        files(&core.join(dir), &["rs"], &["fixtures"], &mut code);
    }
    for dir in ["sources/SpecBoard", "sources/SpecBoardTests"] {
        files(
            &repo.join(dir),
            &["swift"],
            &["Resources", "Fixtures"],
            &mut code,
        );
    }
    assert!(code.len() > 300, "{}", code.len());
    let mut found = Vec::new();
    let mut shown = |path: &Path, n: usize, word: String| {
        let at = path
            .strip_prefix(repo)
            .unwrap_or(path)
            .display()
            .to_string();
        found.push(format!("{at}:{n}: {word}"));
    };
    for path in &code {
        // This file names the words it looks for.
        if path.ends_with("tests/dev_notes.rs") {
            continue;
        }
        let source = std::fs::read_to_string(path).unwrap();
        let mut said = comments(&source);
        if path.extension().is_some_and(|e| e == "swift") {
            said.extend(titles(&source));
        }
        for (n, text) in said {
            if let Some(word) = history(&text) {
                shown(path, n, word);
            }
        }
    }
    // The tools: their comments and what they say of themselves (`#`, docstrings), every line.
    let mut tools = Vec::new();
    files(&repo.join("tools"), &["py", "sh"], &[], &mut tools);
    for path in &tools {
        let source = std::fs::read_to_string(path).unwrap();
        for (n, line) in source.lines().enumerate() {
            if let Some(word) = history(line) {
                shown(path, n + 1, word);
            }
        }
    }
    assert!(
        found.is_empty(),
        "注釈・テストの題・道具の説明に経緯（日付・決めた人・goal・task・issue）が入っています。なぜだけを書き、経緯はボードの task とコミットに:\n{}",
        found.join("\n")
    );
}

#[test]
fn a_comment_s_history_is_told_from_its_why() {
    assert_eq!(
        history("// (オーナー 2026-10-13, the board's task `Goals:X`)"),
        Some("the board's task `".into())
    );
    assert_eq!(
        history("/// as the board's goal `Goals/Complete:Y` decided"),
        Some("the board's goal `".into())
    );
    assert_eq!(
        history("/// the board's goal and task keep their strays"),
        None
    );
    assert_eq!(
        history("/// by its path (`Goals/Complete:Ship` → `Goals`)"),
        None
    );
    assert_eq!(
        history(concat!("// debug", "-mac #23 A1")),
        Some(concat!("debug", "-mac").into())
    );
    assert_eq!(
        history("// see specboard://SpecBoard/Docs/Design:CoreApi"),
        None
    );
    assert_eq!(history("// the board's tree is read like main"), None);
    assert_eq!(
        comments("let a = \"x // y\"; // 二\n  /// 三\nlet b = 1;"),
        vec![(1, " // 二".to_string()), (2, "/// 三".to_string())]
    );
    assert_eq!(
        titles("    @Test(\"題\")\n    func a() {}\n@Suite(\"束\")"),
        vec![
            (1, "    @Test(\"題\")".to_string()),
            (3, "@Suite(\"束\")".to_string())
        ]
    );
}

#[test]
fn a_colour_is_not_a_number_and_a_number_is_a_note() {
    assert_eq!(note("color: #2F6FEB"), None);
    assert_eq!(note("#111111"), None);
    assert_eq!(note("（#81）"), Some("#81".into()));
    assert_eq!(note("（オーナー 2026-10-05）"), Some("オーナー 20".into()));
    assert_eq!(
        literals("let a = \"一\"; // \"二\"\n#[cfg(test)]\nlet b = \"三\";"),
        vec![(1, "一".to_string())]
    );
    // What is built only for the tests is left out, and what comes after it read.
    let source = "#[cfg(test)]\nmod fixtures;\nlet a = \"一\";\n#[cfg(test)]\n/// 試す\nmod tests {\n    let b = \"二\";\n    fn f() {\n    }\n}\nlet c = \"三\";";
    assert_eq!(
        literals(source),
        vec![(3, "一".to_string()), (11, "三".to_string())]
    );
    assert_eq!(
        test_only(Path::new("/src/lib.rs"), source),
        vec![PathBuf::from("/src/fixtures.rs")]
    );
    assert_eq!(
        test_only(Path::new("/src/kinds.rs"), source),
        vec![PathBuf::from("/src/kinds/fixtures.rs")]
    );
}
