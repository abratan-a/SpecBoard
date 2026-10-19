use super::*;

/// The values cards keep and the names of a goal's fields of before, which a text in another
/// language quotes as they are.
const KEPT: [&str; 11] = [
    "未着手",
    "進行中",
    "完了",
    "打ち切り",
    "高",
    "中",
    "低",
    "状態",
    "優先度",
    "期限",
    "担当",
];

#[test]
fn every_language_but_japanese_has_words_for_the_tools() {
    let with_words: Vec<&str> = overlay_languages().collect();
    for code in crate::text::LANGUAGES {
        assert_eq!(
            with_words.contains(code),
            *code != "ja",
            "mcp/text/{code}.json: every language but Japanese (the catalog's) has one"
        );
    }
    for code in with_words {
        assert!(
            crate::text::LANGUAGES.contains(&code),
            "mcp/text/{code}.json is not a language of LANGUAGES"
        );
    }
}

#[test]
fn every_japanese_text_of_the_tools_and_the_catalog_has_words_in_every_language() {
    let written = catalog_written();
    for code in overlay_languages() {
        let overs = overs(code).unwrap();
        let missing: Vec<String> = places(&written)
            .into_iter()
            .filter(|(tool, pointer, japanese)| {
                let found = match tool {
                    None => crate::text::over(&overs.catalog, pointer, japanese),
                    Some(name) => overs
                        .tools
                        .get(name)
                        .and_then(|o| crate::text::over(o, pointer, japanese)),
                };
                found.is_none()
            })
            .map(|(tool, pointer, japanese)| {
                format!(
                    "{} {pointer}: {japanese}",
                    tool.as_deref().unwrap_or("catalog")
                )
            })
            .collect();
        assert!(
            missing.is_empty(),
            "mcp/text/{code}.json has no words for these (or words made from what they said before; \
             after tools.json is written out again, give its new texts theirs): {missing:#?}"
        );
    }
}

#[test]
fn no_words_of_the_tools_are_left_for_a_text_no_more_written() {
    let places = places(&catalog_written());
    let written = |tool: Option<&str>, pointer: &str, from: &str| {
        places
            .iter()
            .any(|(t, p, j)| t.as_deref() == tool && p == pointer && j == from)
    };
    for code in overlay_languages() {
        let overs = overs(code).unwrap();
        let mut stale: Vec<String> = overs
            .catalog
            .iter()
            .filter(|(pointer, over)| !written(None, pointer, &over.from))
            .map(|(pointer, _)| format!("catalog {pointer}"))
            .chain(overs.tools.iter().flat_map(|(tool, places)| {
                places
                    .iter()
                    .filter(|(pointer, over)| !written(Some(tool), pointer, &over.from))
                    .map(move |(pointer, _)| format!("{tool} {pointer}"))
            }))
            .collect();
        stale.sort();
        assert!(
            stale.is_empty(),
            "mcp/text/{code}.json has words for texts the catalog and tools.json no longer have: \
             {stale:#?}"
        );
    }
}

#[test]
fn the_words_of_the_tools_in_english_have_no_japanese_but_values_cards_keep() {
    let overs = overs("en").unwrap();
    let japanese = |text: &str| {
        let mut text = text.to_string();
        for value in KEPT {
            text = text.replace(value, "");
        }
        crate::text::has_japanese(&text)
    };
    let mut found: Vec<String> = overs
        .catalog
        .iter()
        .filter(|(_, o)| japanese(&o.text))
        .map(|(p, o)| format!("catalog {p}: {}", o.text))
        .chain(overs.tools.iter().flat_map(|(tool, places)| {
            places
                .iter()
                .filter(|(_, o)| japanese(&o.text))
                .map(move |(p, o)| format!("{tool} {p}: {}", o.text))
        }))
        .collect();
    found.sort();
    assert!(found.is_empty(), "{found:#?}");
}

/// Whether a description says more than its first sentence, as the language of the call cuts one.
fn says_more(text: &str) -> bool {
    brief(text) != text
}

#[test]
fn the_list_cuts_an_english_description_where_the_japanese_one_is_cut() {
    let japanese = {
        let _ja = crate::text::bind(Some("ja"));
        catalog().clone()
    };
    let _en = crate::text::bind(Some("en"));
    let english = catalog();
    let mut found = Vec::new();
    for (tool, pointer, said) in places(&japanese) {
        let Some(name) = tool else { continue };
        // What the list cuts: a tool's description, and its arguments' one level in (value's is
        // said whole).
        let listed = pointer == "/description"
            || (pointer.starts_with("/inputSchema/properties/")
                && pointer.ends_with("/description")
                && pointer != "/inputSchema/properties/value/description"
                && pointer.matches("/properties/").count() <= 2);
        if !listed {
            continue;
        }
        let ja_more = {
            let _ja = crate::text::bind(Some("ja"));
            says_more(&said)
        };
        let english_said = english["tools"]
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["name"] == name.as_str())
            .and_then(|t| t.pointer(&pointer))
            .and_then(Value::as_str)
            .unwrap()
            .to_string();
        let cut = brief(&english_said);
        if says_more(&english_said) != ja_more || (ja_more && !cut.ends_with('.')) {
            found.push(format!("{name} {pointer}: {cut}"));
        }
    }
    assert!(found.is_empty(), "{found:#?}");
}

#[test]
fn the_catalog_is_said_in_the_language_of_the_call() {
    let said = |lang: &str| {
        let _lang = crate::text::bind(Some(lang));
        (
            text("instructions").to_string(),
            tool("card_read").unwrap()["title"].clone(),
            catalog()["gone"]["save_plan"].clone(),
        )
    };
    let (instructions, title, gone) = said("ja");
    assert!(instructions.starts_with("SpecBoard のプロジェクトを読み書きします。"));
    assert!(crate::text::has_japanese(title.as_str().unwrap()));
    assert!(crate::text::has_japanese(gone.as_str().unwrap()));
    let (instructions, title, gone) = said("en");
    assert!(!crate::text::has_japanese(&instructions), "{instructions}");
    assert!(
        !crate::text::has_japanese(title.as_str().unwrap()),
        "{title}"
    );
    assert!(!crate::text::has_japanese(gone.as_str().unwrap()), "{gone}");
}
