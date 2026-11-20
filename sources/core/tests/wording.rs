//! What the core and the app say in Japanese writes a technical term as the term, not as a native
//! word made up for it: a person who knows the term would have to translate it back
//! (`specboard://SpecBoard/Docs/Rules:Terms`). The texts looked through are all that is said: the
//! core's messages (`src/text/ja`), the tools and the guide (`src/mcp/catalog.json`,
//! `src/mcp/tools.json`), the kinds built in (`src/kinds/builtin.json`), and the app's screens
//! (the Japanese of `sources/SpecBoard/Strings/*.xcstrings`).

use serde_json::Value;
use std::path::Path;

/// The native words that stood for a term, each with the term written instead. Only the words that
/// say nothing else in these texts are kept here; a word with an ordinary sense too (木, 形, 並び)
/// is left to the table.
const PARAPHRASES: [(&str, &str); 22] = [
    ("置き場", "データベース・データディレクトリ・場所・ストア"),
    ("本体", "デーモン"),
    ("控え", "バックアップ"),
    ("繋", "接続"),
    ("頼み", "リクエスト"),
    ("知らせ", "通知"),
    ("欄", "フィールド"),
    ("入れ物", "コンテナ"),
    ("名簿", "メンバーリスト"),
    ("持ち主", "オーナー"),
    ("使う側", "利用側"),
    ("呼び手", "呼び出し元"),
    ("見本", "サンプル"),
    ("作り物", "ダミー"),
    ("足を切", "切り詰め"),
    ("読むだけ", "読み取り専用"),
    ("分かれ道", "分岐"),
    ("きっかけ", "トリガー"),
    ("受け継", "継承"),
    ("取ってき", "ダウンロード"),
    ("起こし", "起動"),
    ("起こす", "起動"),
];

/// Words that hold one of the paraphrases and are the ordinary word themselves: the announcements
/// of the notifications' kinds are お知らせ, as Japanese software names them.
const ORDINARY: [&str; 1] = ["お知らせ"];

/// Whether `text` writes `word` other than inside an ordinary word that holds it.
fn writes(text: &str, word: &str) -> bool {
    let inside: usize = ORDINARY
        .iter()
        .filter(|ordinary| ordinary.contains(word))
        .map(|ordinary| text.matches(ordinary).count() * ordinary.matches(word).count())
        .sum();
    text.matches(word).count() > inside
}

/// Every string in `value`, with where it is.
fn strings(value: &Value, at: &str, out: &mut Vec<(String, String)>) {
    match value {
        Value::String(text) => out.push((at.to_string(), text.clone())),
        Value::Array(items) => {
            for (n, item) in items.iter().enumerate() {
                strings(item, &format!("{at}/{n}"), out);
            }
        }
        Value::Object(map) => {
            for (key, item) in map {
                strings(item, &format!("{at}/{key}"), out);
            }
        }
        _ => {}
    }
}

fn read(path: &Path) -> Value {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// The Japanese texts said, each with the file and the place in it.
fn said() -> Vec<(String, String)> {
    let core = Path::new(env!("CARGO_MANIFEST_DIR"));
    let repo = core.parent().unwrap().parent().unwrap();
    let mut out = Vec::new();
    let mut put = |file: &Path, texts: Vec<(String, String)>| {
        let name = file
            .strip_prefix(repo)
            .unwrap_or(file)
            .display()
            .to_string();
        out.extend(
            texts
                .into_iter()
                .map(|(at, text)| (format!("{name}{at}"), text)),
        );
    };
    let mut messages: Vec<_> = std::fs::read_dir(core.join("src/text/ja"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    messages.sort();
    messages.extend(
        [
            "src/mcp/catalog.json",
            "src/mcp/tools.json",
            "src/kinds/builtin.json",
        ]
        .iter()
        .map(|p| core.join(p)),
    );
    for file in messages {
        let mut texts = Vec::new();
        strings(&read(&file), "", &mut texts);
        put(&file, texts);
    }
    let mut screens: Vec<_> = std::fs::read_dir(repo.join("sources/SpecBoard/Strings"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "xcstrings"))
        .collect();
    screens.sort();
    assert!(screens.len() > 20, "{}", screens.len());
    for file in screens {
        let catalog = read(&file);
        let mut texts = Vec::new();
        for (key, entry) in catalog["strings"].as_object().unwrap() {
            strings(
                &entry["localizations"]["ja"],
                &format!("/{key}"),
                &mut texts,
            );
        }
        put(&file, texts);
    }
    out
}

#[test]
fn the_japanese_said_writes_the_terms_not_words_made_up_for_them() {
    let said = said();
    assert!(said.len() > 5000, "{}", said.len());
    let found: Vec<String> = said
        .iter()
        .flat_map(|(at, text)| {
            PARAPHRASES
                .iter()
                .filter(|(word, _)| writes(text, word))
                .map(move |(word, term)| format!("{at}: {word} → {term}"))
        })
        .collect();
    assert!(
        found.is_empty(),
        "{} texts write a word made up for a term:\n{}",
        found.len(),
        found.join("\n")
    );
}
