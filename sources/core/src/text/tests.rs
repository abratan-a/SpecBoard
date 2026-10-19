use super::*;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// The `{name}`s a template takes.
fn names(template: &str) -> BTreeSet<&str> {
    pieces(template)
        .into_iter()
        .filter_map(|p| match p {
            Piece::Name(name) => Some(name),
            Piece::Text(_) => None,
        })
        .collect()
}

#[test]
fn system_takes_the_first_preferred_language_that_has_a_table() {
    assert_eq!(resolve_system(["ja-JP", "en-US"]), "ja");
    assert_eq!(resolve_system(["fr-FR", "ja"]), "ja");
    assert_eq!(resolve_system(["en-GB", "ja-JP"]), "en");
}

#[test]
fn system_is_english_when_no_preferred_language_has_a_table() {
    assert_eq!(resolve_system(["fr-FR"]), "en");
    assert_eq!(resolve_system(["fr", "de-DE"]), "en");
    assert_eq!(resolve_system([]), "en");
}

#[test]
fn the_machine_language_is_lc_all_else_lc_messages_else_lang() {
    let env = |pairs: &'static [(&'static str, &'static str)]| {
        move |name: &str| {
            pairs
                .iter()
                .find(|(n, _)| *n == name)
                .map(|(_, v)| v.to_string())
        }
    };
    assert_eq!(system_from_env(env(&[("LANG", "ja_JP.UTF-8")])), Some("ja"));
    assert_eq!(
        system_from_env(env(&[
            ("LC_MESSAGES", "en_US.UTF-8"),
            ("LANG", "ja_JP.UTF-8")
        ])),
        Some("en")
    );
    assert_eq!(
        system_from_env(env(&[
            ("LC_ALL", "ja_JP.UTF-8"),
            ("LC_MESSAGES", "en_US.UTF-8")
        ])),
        Some("ja")
    );
    // The first one set decides, even when it names a language without a table.
    assert_eq!(
        system_from_env(env(&[("LC_ALL", "fr_FR.UTF-8"), ("LANG", "ja_JP.UTF-8")])),
        Some("en")
    );
    assert_eq!(system_from_env(env(&[("LANG", "C.UTF-8")])), Some("en"));
    assert_eq!(system_from_env(env(&[("LANG", "")])), None);
    assert_eq!(system_from_env(env(&[])), None);
}

#[test]
fn every_language_has_the_same_keys_with_the_same_names_to_put_in() {
    let first = table(0);
    for (index, code) in LANGUAGES.iter().enumerate() {
        let table = table(index);
        let mut missing: Vec<&String> = first.keys().filter(|k| !table.contains_key(*k)).collect();
        let mut extra: Vec<&String> = table.keys().filter(|k| !first.contains_key(*k)).collect();
        missing.sort();
        extra.sort();
        assert!(
            missing.is_empty() && extra.is_empty(),
            "text/{code}.json lacks {missing:?} and has {extra:?} beyond text/{}.json",
            LANGUAGES[0]
        );
        for (key, text) in table {
            assert_eq!(
                names(text),
                names(&first[key]),
                "text/{code}.json {key} puts in other names than text/{}.json",
                LANGUAGES[0]
            );
        }
    }
}

#[test]
fn every_language_has_a_folder_of_the_same_areas() {
    let areas = |code: &str| -> Vec<&str> { sources(code).iter().map(|(a, _)| *a).collect() };
    let first = areas(LANGUAGES[0]);
    assert!(!first.is_empty(), "text/{}/ has no area", LANGUAGES[0]);
    for code in LANGUAGES {
        assert_eq!(
            areas(code),
            first,
            "text/{code}/ has other areas than text/{}/",
            LANGUAGES[0]
        );
    }
    // A folder of a language not in the list would never be said.
    for (code, _) in SOURCES {
        assert!(
            LANGUAGES.contains(code),
            "text/{code}/ is not a language of LANGUAGES"
        );
    }
}

#[test]
fn the_tables_are_kept_in_key_order_with_keys_of_dotted_words_of_their_area() {
    // In key order, two sessions adding keys meet only where their keys are neighbours.
    for code in LANGUAGES {
        for (area, source) in sources(code) {
            let map: serde_json::Map<String, Value> = serde_json::from_str(source).unwrap();
            let keys: Vec<&String> = map.keys().collect();
            let mut sorted = keys.clone();
            sorted.sort();
            assert_eq!(keys, sorted, "text/{code}/{area}.json is not in key order");
            for key in keys {
                let words: Vec<&str> = key.split('.').collect();
                assert!(
                    words.len() >= 2
                        && words.iter().all(|w| {
                            w.starts_with(|c: char| c.is_ascii_lowercase())
                                && w.chars().all(|c| c.is_ascii_alphanumeric())
                        }),
                    "text/{code}/{area}.json: {key} is not dotted camelCase words (area.thing.what)"
                );
                assert_eq!(
                    words[0], *area,
                    "text/{code}/{area}.json: {key} belongs in text/{code}/{}.json",
                    words[0]
                );
            }
        }
    }
}

#[test]
fn a_text_is_said_in_the_language_bound_with_its_values_put_in() {
    let refused = |lang: &str| {
        let _bound = bind(Some(lang));
        text!(
            "agent.mcp.settingsWriteFailed",
            path = "agent.json",
            error = "system・ja・en"
        )
    };
    assert_eq!(refused("ja"), "agent.json に書けません: system・ja・en");
    assert_eq!(refused("en"), "Cannot write agent.json: system・ja・en");
    assert_eq!(refused("en-US"), "Cannot write agent.json: system・ja・en");
}

#[test]
fn a_call_with_no_language_bound_is_said_in_japanese() {
    assert_eq!(current(), "ja");
    assert_eq!(text!("inspection.group.brokenPaths.label"), "壊れたパス");
}

#[test]
fn binding_puts_back_the_language_before_when_it_ends() {
    {
        let _en = bind(Some("en"));
        assert_eq!(current(), "en");
        {
            let _ja = bind(Some("ja"));
            assert_eq!(current(), "ja");
        }
        assert_eq!(current(), "en");
        {
            // A language without a table, or none, leaves the call's language as it is.
            let _fr = bind(Some("fr"));
            assert_eq!(current(), "en");
            let _none = bind(None);
            assert_eq!(current(), "en");
        }
        assert_eq!(current(), "en");
    }
    assert_eq!(current(), UNBOUND);
}

#[test]
fn braces_that_are_not_a_name_stay_as_written() {
    let _en = bind(Some("en"));
    let said = |template: &str| {
        pieces(template)
            .into_iter()
            .map(|p| match p {
                Piece::Text(t) => t.to_string(),
                Piece::Name(n) => format!("<{n}>"),
            })
            .collect::<String>()
    };
    assert_eq!(said("{{path}} and {name}"), "{{path}} and <name>");
    assert_eq!(said("{{パス}} {\"as\": 1} {}"), "{{パス}} {\"as\": 1} {}");
    assert_eq!(said("{a_1}{b}"), "<a_1><b>");
    assert_eq!(said("{ name}"), "{ name}");
    assert_eq!(
        names("{{path}} {count} of {total}"),
        ["count", "total"].into()
    );
}

/// Every `.rs` file under `dir`.
fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn every_key_the_code_says_is_in_the_tables_and_every_key_there_is_said() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    // The agent says the core's texts too.
    for dir in ["src", "agent/src"] {
        rust_files(&root.join(dir), &mut files);
    }
    let code: Vec<(PathBuf, String)> = files
        .into_iter()
        .filter(|p| !p.ends_with("text/tests.rs"))
        .map(|p| {
            let source = std::fs::read_to_string(&p).unwrap();
            (p, source)
        })
        .collect();
    let keys = table(0);
    let mut unknown = Vec::new();
    for (path, source) in &code {
        for opening in ["text!(\"", "keyed!(\"", "is_said(\""] {
            for said in source.split(opening).skip(1) {
                let key = said.split('"').next().unwrap_or("");
                if !keys.contains_key(key) {
                    unknown.push(format!("{}: {key}", path.display()));
                }
            }
        }
    }
    assert!(unknown.is_empty(), "keys no table has: {unknown:#?}");
    let mut unsaid: Vec<&String> = keys
        .keys()
        .filter(|key| {
            let quoted = format!("\"{key}\"");
            !code.iter().any(|(_, source)| source.contains(&quoted))
        })
        .collect();
    unsaid.sort();
    assert!(
        unsaid.is_empty(),
        "keys no code says (write each key whole, as a literal): {unsaid:#?}"
    );
}

#[test]
fn a_kept_text_keeps_its_key_and_values_and_is_said_when_read() {
    let inner = keyed!(
        "agent.mcp.settingsWriteFailed",
        path = "fr",
        error = List::of(["a", "b"])
    );
    let name = String::from("x");
    let outer = keyed!("agent.mcp.settingsWriteFailed", path = &inner, error = name);
    assert_eq!(
        outer.json(),
        serde_json::json!({
            "key": "agent.mcp.settingsWriteFailed",
            "values": {
                "path": {"key": "agent.mcp.settingsWriteFailed", "values": {"path": "fr", "error": ["a", "b"]}},
                "error": "x"
            }
        })
    );
    assert_eq!(Keyed::from_json(&outer.json()), Some(outer.clone()));
    assert_eq!(inner.render_in("en"), "Cannot write fr: a, b");
    assert_eq!(inner.render_in("ja"), "fr に書けません: a・b");
}

#[test]
fn a_text_is_known_by_its_key_whatever_was_put_in_and_around_it() {
    for lang in LANGUAGES {
        let _bound = bind(Some(lang));
        let said = text!(
            "update.broken.noLongerResolves",
            at = "Docs:A",
            via = "body",
            path = "X:Y"
        );
        assert!(is_said("update.broken.noLongerResolves", &said), "{said}");
        assert!(
            is_said("update.broken.noLongerResolves", &format!("Docs:B: {said}")),
            "{said}"
        );
        assert!(!is_said("update.broken.targetGone", &said), "{said}");
        assert!(!is_said("update.broken.noLongerResolves", "Docs:A"));
    }
}

#[test]
fn a_list_is_joined_as_the_language_joins_one() {
    let items = ["a", "b", "c"];
    {
        let _ja = bind(Some("ja"));
        assert_eq!(list(&items), "a・b・c");
        assert_eq!(list_comma(&items), "a、b、c");
        assert_eq!(List::of(items).to_string(), "a・b・c");
    }
    let _en = bind(Some("en"));
    assert_eq!(list(&items), "a, b, c");
    assert_eq!(list_comma(&items), "a, b, c");
    let kept = keyed!("inspection.problem.overlap", over = List::of(items));
    assert_eq!(kept.render_in("ja"), "a・b・c に重なっています");
    assert_eq!(kept.render_in("en"), "It overlaps a, b, c");
    let labels = List::of_keyed([
        keyed!("common.label.description"),
        keyed!("common.label.displayName"),
    ]);
    assert_eq!(labels.to_string(), "Description, Display name");
}

#[test]
fn texts_of_every_kind_are_said_in_english() {
    let _en = bind(Some("en"));
    // A refusal.
    assert!(text!("write.lockedMain").starts_with("main is locked (only a merge can change it)."));
    // A warning.
    assert_eq!(
        text!(
            "update.broken.noLongerResolves",
            at = "Docs:A",
            via = "body",
            path = "X:Y"
        ),
        "\"X:Y\" in body of Docs:A no longer resolves"
    );
    // A history line, kept by key and said when read.
    let line = keyed!("history.revert.line", count = 2, seq = 5);
    assert_eq!(
        line.render(),
        "Revert (2 commits, back to right after commit 5)"
    );
    assert_eq!(
        line.render_in("ja"),
        "戻す（2 件。コミット 5 のすぐあとへ）"
    );
    // An item of the check.
    assert_eq!(
        text!("inspection.problem.freeCard", section = "Free"),
        "It is left in the free section Free"
    );
    // The agent's refusal.
    assert_eq!(
        text!(
            "agent.mcp.settingsWriteFailed",
            path = "agent.json",
            error = list(&["system", "ja", "en"])
        ),
        "Cannot write agent.json: system, ja, en"
    );
    // The window, as an author.
    assert_eq!(text!("history.author.screen"), "Screen");
    assert_eq!(text!("history.author.mcpNamed", name = "dev"), "MCP: dev");
}

/// Names the screen no longer says (`specboard://SpecBoard/Docs/Design:UI` §12): a Japanese text is shown on the
/// screen, and one written later with an old name would call a part of it what the screen does not.
const OLD_SCREEN_NAMES: &[&str] = &[
    "リレーションパネル",
    "つながりの図",
    "カードマップ",
    "書き出",
    "今すぐ更新",
];

#[test]
fn the_japanese_texts_name_the_screen_as_it_is_named() {
    let japanese = table(index_of("ja"));
    let mut found: Vec<String> = japanese
        .iter()
        .filter(|(_, text)| OLD_SCREEN_NAMES.iter().any(|old| text.contains(old)))
        .map(|(key, text)| format!("{key}: {text}"))
        .collect();
    found.sort();
    assert!(
        found.is_empty(),
        "Japanese texts naming the screen by an old name: {found:#?}"
    );
}

use super::has_japanese as japanese;

/// English texts that name what is written in Japanese: the names of a goal's fields of before.
const JAPANESE_NAMES: &[&str] = &["keyMove.line.goalKeys"];

#[test]
fn the_english_texts_are_written_in_english() {
    let english = table(index_of("en"));
    let mut found: Vec<String> = english
        .iter()
        .filter(|(key, text)| japanese(text) && !JAPANESE_NAMES.contains(&key.as_str()))
        .map(|(key, text)| format!("{key}: {text}"))
        .collect();
    found.sort();
    assert!(
        found.is_empty(),
        "English texts with Japanese in them: {found:#?}"
    );
}

/// Rust source with what is built only for its tests blanked: the item under each `#[cfg(test)]`
/// (a module or a function written there, to the brace that closes it at its own indent — the code
/// is formatted), its lines kept so the lines after it keep their numbers.
fn without_tests(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    // The indent of the `#[cfg(test)]` whose item is blanked, and whether the item has begun.
    let mut skipping: Option<(usize, bool)> = None;
    for line in source.lines() {
        let trimmed = line.trim();
        let indent = line.len() - line.trim_start().len();
        let blank = match skipping {
            Some((at, begun)) => {
                if !begun {
                    if !(trimmed.starts_with("#[")
                        || trimmed.starts_with("//")
                        || trimmed.is_empty())
                    {
                        // `mod tests;`, or an item closed on its one line.
                        skipping = (!trimmed.ends_with(';') && !trimmed.ends_with('}'))
                            .then_some((at, true));
                    }
                } else if indent == at && trimmed.starts_with('}') {
                    skipping = None;
                }
                true
            }
            None if trimmed == "#[cfg(test)]" => {
                skipping = Some((indent, false));
                true
            }
            None => false,
        };
        if !blank {
            out.push_str(line);
        }
        out.push('\n');
    }
    out
}

/// The string literals of Rust source by their lines (from 1): its comments, its characters and its
/// lifetimes left out.
fn string_literals(source: &str) -> Vec<(usize, String)> {
    let chars: Vec<char> = source.chars().collect();
    let mut out = Vec::new();
    let (mut i, mut line) = (0, 1);
    let at = |i: usize| chars.get(i).copied().unwrap_or('\0');
    while i < chars.len() {
        let c = chars[i];
        let word_before = i > 0 && (chars[i - 1].is_alphanumeric() || chars[i - 1] == '_');
        if c == '\n' {
            line += 1;
            i += 1;
        } else if c == '/' && at(i + 1) == '/' {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
        } else if c == '/' && at(i + 1) == '*' {
            let mut depth = 0;
            while i < chars.len() {
                if chars[i] == '/' && at(i + 1) == '*' {
                    depth += 1;
                    i += 2;
                } else if chars[i] == '*' && at(i + 1) == '/' {
                    depth -= 1;
                    i += 2;
                    if depth == 0 {
                        break;
                    }
                } else {
                    if chars[i] == '\n' {
                        line += 1;
                    }
                    i += 1;
                }
            }
        } else if !word_before && (c == 'r' || (c == 'b' && at(i + 1) == 'r')) && {
            let mut j = i + if c == 'b' { 2 } else { 1 };
            while at(j) == '#' {
                j += 1;
            }
            at(j) == '"'
        } {
            let mut j = i + if c == 'b' { 2 } else { 1 };
            let mut hashes = 0;
            while at(j) == '#' {
                hashes += 1;
                j += 1;
            }
            j += 1;
            let start = (j, line);
            let mut text = String::new();
            loop {
                if j >= chars.len() {
                    break;
                }
                if chars[j] == '"' && (1..=hashes).all(|k| at(j + k) == '#') {
                    j += 1 + hashes;
                    break;
                }
                if chars[j] == '\n' {
                    line += 1;
                }
                text.push(chars[j]);
                j += 1;
            }
            out.push((start.1, text));
            i = j;
        } else if c == '"' || (c == 'b' && at(i + 1) == '"' && !word_before) {
            let mut j = i + if c == 'b' { 2 } else { 1 };
            let start = line;
            let mut text = String::new();
            while j < chars.len() && chars[j] != '"' {
                if chars[j] == '\\' {
                    text.push(chars[j]);
                    j += 1;
                }
                if chars[j] == '\n' {
                    line += 1;
                }
                text.push(chars[j]);
                j += 1;
            }
            out.push((start, text));
            i = j + 1;
        } else if c == '\'' {
            // A character ('a', '\n', '\u{3000}'), else a lifetime.
            if at(i + 1) == '\\' {
                let mut j = i + 2;
                while j < chars.len() && chars[j] != '\'' {
                    j += 1;
                }
                i = j + 1;
            } else if at(i + 2) == '\'' {
                i += 3;
            } else {
                i += 1;
            }
        } else {
            i += 1;
        }
    }
    out
}

#[test]
fn the_reading_of_literals_leaves_out_comments_characters_and_tests() {
    let source = "let a = \"一\"; // \"二\"\n/* \"三\" /* \"四\" */ */ let b = r#\"五\"#;\nlet c = '六'; fn f<'a>(x: &'a str) {}\nlet d = \"七\\\"\n八\";\n#[cfg(test)]\nmod tests {\n    let e = \"九\";\n}\nlet f = b\"x\";";
    let found: Vec<(usize, String)> = string_literals(&without_tests(source));
    assert_eq!(
        found,
        vec![
            (1, "一".to_string()),
            (2, "五".to_string()),
            (4, "七\\\"\n八".to_string()),
            (10, "x".to_string()),
        ]
    );
}

/// The files whose Japanese is not what the core says to a person or an AI in a language: what a
/// move of a store of before writes into a project (its kinds' definitions, its cards' descriptions,
/// its commits' names — in the words the project was written in), and the code built only for the
/// tests.
const SAID_ELSEWHERE: &[&str] = &["src/line_move.rs", "src/fixtures.rs"];

/// Japanese that is not a text said, by its file and a piece of the literal it is in.
const NOT_SAID: &[(&str, &str)] = &[
    // The window's name in marks made before, met again when they are read.
    ("src/tools/inspection.rs", "画面"),
    // The names of a goal's fields of before, met again when a card of before is read.
    ("src/kinds/mod.rs", "状態"),
    ("src/kinds/mod.rs", "優先度"),
    ("src/kinds/mod.rs", "期限"),
    ("src/kinds/mod.rs", "担当"),
    // The parts of a conflict a merge request keeps it took, met again when it is merged.
    ("src/write/merge.rs", "（カード）"),
    ("src/write/merge.rs", "（セクション）"),
    // The forms of the tools' values by their names in the catalog (`mcp/catalog.json`).
    ("src/tools/branches.rs", "branch（as: history）"),
    ("src/tools/reads.rs", "card・field"),
    ("src/tools/reads.rs", "project（as: rules）"),
    ("src/tools/diagram.rs", "branch（as: diagram）"),
    ("src/tools/diagram.rs", "section（as: diagram）"),
    ("src/tools/picture.rs", "section（as: picture）"),
    ("src/tools/mocks.rs", "section（as: mock）"),
    // A wide character a checkbox is measured as.
    ("src/tools/markdown_height.rs", "口{r}"),
    // What a move of a store of before writes into a project: its commits' names, its cards'
    // descriptions.
    ("src/container_move.rs", "入れ物の中身を移した"),
    ("src/ghost_move.rs", "カードゴーストを移した"),
    ("src/key_move.rs", "goal の欄"),
    ("src/nodes.rs", "ノードの移し替え"),
    // A full-width number sign stands in for `#`, which mermaid reads as its own, in every language.
    ("src/export/ident.rs", "＃"),
];

#[test]
fn no_japanese_is_written_in_the_code_but_in_the_tables() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    for dir in ["src", "agent/src"] {
        rust_files(&root.join(dir), &mut files);
    }
    let mut found = Vec::new();
    for path in files {
        let shown = path
            .strip_prefix(root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        if shown.ends_with("/tests.rs") || SAID_ELSEWHERE.contains(&shown.as_str()) {
            continue;
        }
        let source = std::fs::read_to_string(&path).unwrap();
        for (line, literal) in string_literals(&without_tests(&source)) {
            let not_said = NOT_SAID
                .iter()
                .any(|(file, piece)| *file == shown && literal.contains(piece));
            if japanese(&literal) && !not_said {
                found.push(format!("{shown}:{line}: {literal}"));
            }
        }
    }
    assert!(
        found.is_empty(),
        "Japanese written in the code: put it in the text tables (src/text/<language>/<area>.json) \
         and say it with text! or keyed!:\n{}",
        found.join("\n")
    );
}
