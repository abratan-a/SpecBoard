//! The MCP tools as documents (the project's section `specboard://SpecBoard/Docs/Design/MCPTools`): the
//! tools as `tools/list` gives them and the `instructions`, written from the catalog
//! (`src/mcp/catalog.json`) so that what a person reads is what an agent is given. Written to
//! `target/tmp/mcp-tools/` each run, every tool in it: `Overview.md` (the counts, the table and the
//! `instructions`) and one file a group of tools by what their names begin with (`Branch.md` for
//! `branch_*`); put into the project's cards when the catalog changes (`tools/mcp-tools-doc.py`, skill
//! `testing`). What is written is the point; what is held here is that
//! each tool is written once, in its group's file, and in the table.

use serde_json::{Value, json};
use specboard_core::mcp;
use std::path::PathBuf;

fn chars(text: &str) -> usize {
    text.chars().count()
}

fn compact(value: &Value) -> usize {
    chars(&serde_json::to_string(value).unwrap())
}

/// What a tool does, as the doc sorts it: reads, adds, changes or takes away.
fn kind(annotations: &Value) -> &'static str {
    let hint = |key: &str| annotations[key].as_bool().unwrap_or(false);
    if hint("readOnlyHint") {
        "読む"
    } else if hint("destructiveHint") {
        "消す"
    } else if !hint("idempotentHint") {
        "足す"
    } else {
        "変える"
    }
}

struct Tool {
    name: String,
    title: String,
    kind: &'static str,
    brief: String,
    listing: Value,
    /// The tools/list entry, packed.
    line: usize,
    /// What a model reads of it: its name, its first sentence and its arguments' shape.
    read: usize,
    /// All of it: the whole description, the whole arguments, and every value's shape.
    full: usize,
}

/// The group a tool is in, as the doc splits them: what its name begins with (`branch` of
/// `branch_merge`, `guide` of `guide`).
fn group(name: &str) -> &str {
    name.split('_').next().unwrap()
}

/// A group's card in the project's section: its name with the first letter large (`Branch`).
fn card_name(group: &str) -> String {
    let mut chars = group.chars();
    let first = chars.next().unwrap().to_ascii_uppercase();
    std::iter::once(first).chain(chars).collect()
}

/// Where the docs go in the project: the overview and one card a group.
const SECTION: &str = "Docs/Design/MCPTools";

/// The docs, each a card's name and its body: `Overview` first, then the groups by name.
fn render() -> Vec<(String, String)> {
    let catalog = mcp::catalog();
    let raw: Vec<&Value> = catalog["tools"].as_array().unwrap().iter().collect();
    let mut tools: Vec<Tool> = mcp::listed()
        .into_iter()
        .map(|t| {
            let name = t["name"].as_str().unwrap().to_string();
            let whole = raw.iter().find(|r| r["name"] == name.as_str()).unwrap();
            let brief = t["description"].as_str().unwrap().to_string();
            let forms: usize = whole["forms"]
                .as_array()
                .map(|f| {
                    f.iter()
                        .map(|f| compact(&f["spec"]) + chars(f["label"].as_str().unwrap_or("")))
                        .sum()
                })
                .unwrap_or(0);
            Tool {
                title: t["title"].as_str().unwrap_or("").to_string(),
                kind: kind(&t["annotations"]),
                line: compact(&t),
                read: chars(&name) + chars(&brief) + compact(&t["inputSchema"]),
                full: chars(&name)
                    + chars(whole["description"].as_str().unwrap())
                    + compact(&whole["inputSchema"])
                    + forms,
                listing: t["inputSchema"].clone(),
                brief,
                name,
            }
        })
        .collect();
    // By the tools' names, the table and the sections both.
    tools.sort_by(|a, b| a.name.cmp(&b.name));
    let mut groups: Vec<&str> = tools.iter().map(|t| group(&t.name)).collect();
    groups.dedup();
    let instructions = catalog["instructions"].as_str().unwrap();
    let mut gone: Vec<&String> = catalog["gone"].as_object().unwrap().keys().collect();
    gone.sort();
    let keys: Vec<String> = mcp::argument_shapes().keys().cloned().collect();

    let mut out = String::new();
    out += "# MCP のツール定義\n\n";
    out += "アプリが `tools/list` で返す一覧と、接続時の `instructions` をそのまま並べたもの。定義の正本はコアの\n";
    out += "`sources/core/src/mcp/catalog.json` で、この文書はそこから cargo test（`tests/mcp_tools_doc.rs`）が\n";
    out += &format!(
        "`sources/core/target/tmp/mcp-tools/` に書き出し、プロジェクトの {SECTION} に置く（定義を変えたら置き直す。`tools/mcp-tools-doc.py`）。\n"
    );
    out += "ツールごとの引数は、ツールの名前の頭（`branch_merge` なら branch）ごとの文書にある:\n";
    out += &groups
        .iter()
        .map(|g| format!("[[{SECTION}:{}]]", card_name(g)))
        .collect::<Vec<_>>()
        .join("・");
    out += "\n\n";
    out += "一覧に載るのは、説明の最初の一文と、引数の一段目（その中のキーは名前と型だけ）と、value の説明（target の指すものごとのキー）\n";
    out += "まで。どのツールでも同じ意味の project・branch・request は名前と型だけで、意味は `instructions` に 1 度だけ書く。\n";
    out += "ツールが何をするか・value のキーの表・答え・例・断られやすいことは `guide`（about にツール名）が返す。\n\n";
    out += &format!("- 一覧に出るツール: {} 個\n", tools.len());
    out += &format!(
        "- `tools/list` の JSON（詰めた形）: {} 文字\n",
        compact(&json!({ "tools": mcp::listed() }))
    );
    out += &format!(
        "- そのうちモデルが読む分（名前・説明・引数の形）: {} 文字（全文なら {} 文字）\n",
        tools.iter().map(|t| t.read).sum::<usize>(),
        tools.iter().map(|t| t.full).sum::<usize>()
    );
    out += &format!("- `instructions`: {} 文字\n", chars(instructions));
    out += &format!(
        "- guide の about に渡せるキー: {}\n",
        keys.iter()
            .map(|k| format!("`{k}`"))
            .collect::<Vec<_>>()
            .join("・")
    );
    out += &format!(
        "- 前の名前（呼ぶと、代わりに使うツールを返して断る）: {}\n\n",
        gone.iter()
            .map(|k| format!("`{k}`"))
            .collect::<Vec<_>>()
            .join("・")
    );
    out += "文字数は JSON に詰めたときの文字数。「一覧」は tools/list の 1 件（区分を含む）、「読む分」は\n";
    out += "名前・説明・引数の形、「全文」は説明の全文と引数の形と value の形（guide はこれを表にして返す）。\n\n";
    out += "| ツール | 区分 | 一覧 | 読む分 | 全文 |\n|---|---|---:|---:|---:|\n";
    for t in &tools {
        out += &format!(
            "| `{}` | {} | {} | {} | {} |\n",
            t.name, t.kind, t.line, t.read, t.full
        );
    }
    out += "\n## instructions\n\n```text\n";
    out += instructions.trim_end();
    out += "\n```\n";

    let mut docs = vec![("Overview".to_string(), out)];
    for g in groups {
        let mine: Vec<&Tool> = tools.iter().filter(|t| group(&t.name) == g).collect();
        let mut out = String::new();
        let alone = mine.len() == 1 && mine[0].name == g;
        out += &format!("# MCP のツール {g}{}\n\n", if alone { "" } else { "_*" });
        out += &format!(
            "名前が {g} {}ツール（{} 個。読む分 {} 文字、全文 {} 文字）。全部のツールの表と `instructions` は [[{SECTION}:Overview]]。\n",
            if alone { "の" } else { "で始まる" },
            mine.len(),
            mine.iter().map(|t| t.read).sum::<usize>(),
            mine.iter().map(|t| t.full).sum::<usize>()
        );
        for t in mine {
            out += &format!("\n## {} — {}\n\n", t.name, t.title);
            out += &format!(
                "区分: {}・読む分 {} 文字（全文 {} 文字）\n\n",
                t.kind, t.read, t.full
            );
            out += &t.brief;
            out += "\n\n```json\n";
            out += &serde_json::to_string_pretty(&t.listing).unwrap();
            out += "\n```\n";
        }
        docs.push((card_name(g), out));
    }
    docs
}

#[test]
fn the_tools_doc_is_what_the_catalog_says() {
    let docs = render();
    let listed = mcp::listed();
    assert!(!listed.is_empty());
    for tool in &listed {
        let name = tool["name"].as_str().unwrap();
        let mine = card_name(group(name));
        let (_, body) = docs.iter().find(|(n, _)| *n == mine).unwrap();
        assert!(
            body.contains(&format!("## {name} — ")),
            "{name} が {mine} の書き出しに無い"
        );
        assert!(
            docs[0].1.contains(&format!("`{name}`")),
            "{name} が Overview の表に無い"
        );
    }
    // Each once: as many tools written in the groups' files as there are.
    let written: usize = docs[1..]
        .iter()
        .map(|(_, body)| {
            body.lines()
                .filter(|l| l.starts_with("## ") && l.contains(" — "))
                .count()
        })
        .sum();
    assert_eq!(written, listed.len());
    let place = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("mcp-tools");
    let _ = std::fs::remove_dir_all(&place);
    std::fs::create_dir_all(&place).unwrap();
    for (name, body) in &docs {
        std::fs::write(place.join(format!("{name}.md")), body).unwrap();
    }
}
