//! A diagram read through MCP, in the characters a list is given in: a class diagram that does not fit
//! is drawn smaller — `card_read` a hop less at a time, else a section to a box and the lines between
//! two sections a kind to one, counted — and says so: `truncated=true`, the characters and the cards
//! of the one asked, its rev, and what to ask to draw less (`hint`). One that fits is as it was; a
//! flow's chart, `revisionOnly` and the window's requests are not cut.

use crate::common;

use common::{call, op, store_with};
use serde_json::{Map, Value, json};
use specboard_core::Core;
use std::collections::BTreeMap;
use std::sync::OnceLock;

const SAMPLE: &str = "見本";
const MOST: usize = 40_000;

/// The sections grown, and the cards in each.
const SECTIONS: usize = 20;
const CARDS: usize = 30;
const HUB: usize = 200;

fn core() -> Core {
    Core::open(store_with(&["sample"])).unwrap()
}

/// The sample grown (`grow`): grown once in a run, each test given a copy of its own.
fn grown() -> Core {
    static GROWN: OnceLock<common::Made> = OnceLock::new();
    Core::open(common::copy_of(&GROWN, "grown", || {
        let store = store_with(&["sample"]);
        let core = Core::open(&store).unwrap();
        grow(&core);
        core.close();
        store
    }))
    .unwrap()
}

fn mcp(core: &Core, tool: &str, arguments: Value) -> Value {
    let (failed, text) = call(core, tool, arguments.clone());
    assert!(!failed, "{tool} {arguments}: {text}");
    serde_json::from_str(&text).unwrap()
}

/// A diagram as the window asks for it.
fn screen(core: &Core, tool: &str, arguments: Value) -> Value {
    let said = op(
        core,
        json!({"op": "tool", "name": tool, "arguments": arguments}),
    );
    assert!(said.get("error").is_none(), "{tool}: {said}");
    said["result"]["json"].clone()
}

/// A diagram's mermaid.
fn text(drawn: &Value) -> &str {
    drawn["mermaid"]
        .as_str()
        .unwrap_or_else(|| panic!("{drawn}"))
}

/// A section's card, by the numbers of both.
fn card(section: usize, n: usize) -> String {
    format!("Schemas/S{section:02}:C{n:02}")
}

fn hub(n: usize) -> String {
    format!("Schemas/Hub:H{n:03}")
}

/// The sample grown: a section `Schemas/Hub` of `HUB` cards, and `SECTIONS` sections of `CARDS` cards,
/// each pointing at one in the hub and at the one of its number in the next section.
fn grow(core: &Core) {
    let mut sections = vec!["Schemas/Hub".to_string()];
    sections.extend((1..=SECTIONS).map(|s| format!("Schemas/S{s:02}")));
    for section in &sections {
        mcp(
            core,
            "section_create",
            json!({"project": SAMPLE, "target": section}),
        );
    }
    let mut made = Map::new();
    for n in 1..=HUB {
        made.insert(hub(n), json!({"kind": "entity"}));
    }
    mcp(
        core,
        "card_create",
        json!({"project": SAMPLE, "value": made}),
    );
    let mut made = Map::new();
    for s in (1..=SECTIONS).rev() {
        for n in 1..=CARDS {
            let mut lines = vec![
                json!({"line": "points", "from": "hub_id", "to": hub((s * CARDS + n) % HUB + 1)}),
            ];
            if s < SECTIONS {
                lines.push(json!({"line": "points", "from": "next_id", "to": card(s + 1, n)}));
            }
            made.insert(
                card(s, n),
                json!({"kind": "entity", "fields": [{"name": "hub_id", "type": "string"}, {"name": "next_id", "type": "string"}], "connections": lines}),
            );
        }
    }
    mcp(
        core,
        "card_create",
        json!({"project": SAMPLE, "value": made}),
    );
}

/// What a diagram says of itself beside its mermaid, each as text (a list joined with commas).
fn head(drawn: &Value) -> BTreeMap<String, String> {
    drawn
        .as_object()
        .unwrap()
        .iter()
        .filter(|(k, _)| *k != "mermaid")
        .map(|(k, v)| {
            let said = match v {
                Value::String(s) => s.clone(),
                Value::Array(a) => a
                    .iter()
                    .map(|i| i.as_str().unwrap_or_default())
                    .collect::<Vec<_>>()
                    .join(","),
                other => other.to_string(),
            };
            (k.clone(), said)
        })
        .collect()
}

/// The cards a class diagram draws: its classes.
fn classes(text: &str) -> usize {
    text.lines()
        .filter(|l| l.trim_start().starts_with("class "))
        .count()
}

fn chars(text: &str) -> usize {
    text.chars().count()
}

/// The arguments of `tool` reading `target` with `as: diagram` and `more` in its value.
fn asked(target: Value, more: Value) -> Value {
    let mut value = json!({"as": "diagram"});
    for (k, v) in more.as_object().unwrap() {
        value[k] = v.clone();
    }
    json!({"project": SAMPLE, "target": target, "value": value})
}

/// The rev `revisionOnly` answers through MCP.
fn rev_only(core: &Core, tool: &str, arguments: &Value) -> String {
    let mut arguments = arguments.clone();
    arguments["value"]["revisionOnly"] = true.into();
    let read = mcp(core, tool, arguments);
    assert!(read.get("truncated").is_none(), "{read}");
    read["rev"].as_str().unwrap().to_string()
}

/// A diagram read through MCP in place of `whole` (the window's, the one asked): in the characters,
/// and saying it is, with the characters, the cards and the rev of `whole` — the rev `revisionOnly`
/// answers too. Its head.
fn given_in_place_of(
    core: &Core,
    tool: &str,
    arguments: &Value,
    whole: &Value,
    given: &Value,
) -> BTreeMap<String, String> {
    assert!(chars(text(whole)) > MOST, "{tool}: {}", chars(text(whole)));
    assert!(!head(whole).contains_key("truncated"));
    assert!(chars(text(given)) <= MOST, "{tool}: {}", chars(text(given)));
    let said = head(given);
    assert_eq!(said["truncated"], "true", "{given}");
    assert_eq!(said["chars"], chars(text(whole)).to_string());
    assert_eq!(said["cards"], classes(text(whole)).to_string());
    assert_eq!(said["rev"], head(whole)["rev"]);
    assert_eq!(said["rev"], rev_only(core, tool, arguments));
    assert!(said["hint"].contains("tags・kinds"), "{}", said["hint"]);
    assert!(!said["hint"].contains("fields: false"), "{}", said["hint"]);
    said
}

/// The box a section is drawn as, by its path: its class name (`s3`).
fn box_of(text: &str, section: &str) -> String {
    let line = text
        .lines()
        .find(|l| l.contains(&format!("[\"{section}（")))
        .unwrap_or_else(|| panic!("{section} の箱がない\n{text}"));
    line.trim()
        .trim_start_matches("class ")
        .split('[')
        .next()
        .unwrap()
        .to_string()
}

#[test]
fn a_project_too_big_is_given_a_section_to_a_box() {
    let core = grown();
    let arguments = json!({"project": SAMPLE, "target": "main", "value": {"as": "diagram"}});
    let whole = screen(&core, "branch_read", arguments.clone());
    let given = mcp(&core, "branch_read", arguments.clone());
    let said = given_in_place_of(&core, "branch_read", &arguments, &whole, &given);
    assert!(
        said["hint"].contains("section_read の as: diagram"),
        "{}",
        said["hint"]
    );
    assert_eq!(said["depth"], "1");
    assert_eq!(said["branch"], "main");
    assert_eq!(said["roots"], "*");
    let given = text(&given);
    // A section a box, with the cards drawn in it.
    let hub = box_of(given, "Schemas/Hub");
    assert!(given.contains(&format!("class {hub}[\"Schemas/Hub（{HUB} 枚）\"]")));
    for s in 1..=SECTIONS {
        let at = box_of(given, &format!("Schemas/S{s:02}"));
        assert!(given.contains(&format!("class {at}[\"Schemas/S{s:02}（{CARDS} 枚）\"]")));
        // The lines between two sections, a kind to one, counted.
        assert!(
            given.contains(&format!("    {at} o-- {hub} : points ×{CARDS}\n")),
            "{given}"
        );
        if s < SECTIONS {
            let next = box_of(given, &format!("Schemas/S{:02}", s + 1));
            assert!(given.contains(&format!("    {at} o-- {next} : points ×{CARDS}\n")));
        }
    }
    // No card drawn, nor a line inside a section.
    assert!(!given.contains("Schemas_S01__C01"), "{given}");
    let schemas = box_of(given, "Schemas");
    assert!(!given.contains(&format!("{schemas} ..> {schemas} ")));
    assert!(given.starts_with("classDiagram\n"));
    // With the fields drawn, said to leave them out.
    let fields = asked(json!("main"), json!({"fields": true}));
    let said = head(&mcp(&core, "branch_read", fields));
    assert!(said["hint"].contains("fields: false"), "{}", said["hint"]);
}

#[test]
fn a_section_too_big_is_given_a_section_to_a_box() {
    let core = grown();
    let arguments = asked(json!("Schemas/Hub"), json!({}));
    let whole = screen(&core, "section_read", arguments.clone());
    let given = mcp(&core, "section_read", arguments.clone());
    let said = given_in_place_of(&core, "section_read", &arguments, &whole, &given);
    assert_eq!(said["section"], "Schemas/Hub");
    assert!(
        said["hint"].contains("card_read の as: diagram"),
        "{}",
        said["hint"]
    );
    let given = text(&given);
    let hub = box_of(given, "Schemas/Hub");
    let first = box_of(given, "Schemas/S01");
    let second = box_of(given, "Schemas/S02");
    assert!(given.contains(&format!("    {first} o-- {hub} : points ×{CARDS}\n")));
    // Only the lines with an end in the section, as the section's diagram draws them.
    assert!(
        !given.contains(&format!("    {first} o-- {second} ")),
        "{given}"
    );
}

#[test]
fn the_cards_around_some_too_big_are_drawn_a_hop_less_at_a_time() {
    let core = grown();
    let roots: Vec<String> = (1..=CARDS).map(|n| card(1, n)).collect();
    let arguments = asked(json!(roots), json!({"depth": 10}));
    let whole = screen(&core, "card_read", arguments.clone());
    let given = mcp(&core, "card_read", arguments.clone());
    let said = given_in_place_of(&core, "card_read", &arguments, &whole, &given);
    let depth: usize = said["depth"].parse().unwrap();
    assert!((1..10).contains(&depth), "{depth}");
    assert_eq!(
        said["hint"],
        format!(
            "depth 10 では 4 万字を超えるので、depth {depth} で描きました（深く描くには、value の tags・kinds で絞るか、target のカードを減らします）"
        )
    );
    // The diagram of that depth, as the window draws it, said of as the one asked.
    let at = |depth: usize| {
        screen(
            &core,
            "card_read",
            asked(json!(roots), json!({"depth": depth})),
        )
    };
    assert_eq!(text(&given), text(&at(depth)));
    // The first that fits: a hop more does not.
    assert!(chars(text(&at(depth + 1))) > MOST);
    // The roots of a section's diagram, the same.
    let names: Vec<String> = (1..=CARDS).map(|n| format!("C{n:02}")).collect();
    let arguments = asked(json!("Schemas/S01"), json!({"roots": names, "depth": 10}));
    let whole = screen(&core, "section_read", arguments.clone());
    let given = mcp(&core, "section_read", arguments.clone());
    let said = given_in_place_of(&core, "section_read", &arguments, &whole, &given);
    let depth: usize = said["depth"].parse().unwrap();
    assert!((1..10).contains(&depth), "{depth}");
    assert!(
        said["hint"].ends_with("value の roots のカードを減らします）"),
        "{}",
        said["hint"]
    );
}

#[test]
fn the_cards_around_some_too_big_at_one_hop_are_given_a_section_to_a_box() {
    let core = grown();
    let roots: Vec<String> = (1..=HUB).map(hub).collect();
    let arguments = asked(json!(roots), json!({"direction": "in"}));
    let whole = screen(&core, "card_read", arguments.clone());
    let given = mcp(&core, "card_read", arguments.clone());
    let said = given_in_place_of(&core, "card_read", &arguments, &whole, &given);
    assert_eq!(said["depth"], "1");
    assert!(
        said["hint"].starts_with("depth 1 でも 4 万字を超えるので"),
        "{}",
        said["hint"]
    );
    assert!(said["hint"].contains("section_read の as: diagram"));
    let given = text(&given);
    let hub = box_of(given, "Schemas/Hub");
    assert!(given.contains(&format!("class {hub}[\"Schemas/Hub（{HUB} 枚）\"]")));
    let last = box_of(given, &format!("Schemas/S{SECTIONS:02}"));
    assert!(given.contains(&format!("    {last} o-- {hub} : points ×{CARDS}\n")));
}

#[test]
fn a_diagram_that_fits_is_as_it_was() {
    let core = core();
    let whole = |core: &Core, tool: &str, arguments: Value| {
        let given = mcp(core, tool, arguments.clone());
        assert_eq!(given, screen(core, tool, arguments), "{tool}");
        assert!(!head(&given).contains_key("truncated"));
        text(&given).to_string()
    };
    whole(&core, "branch_read", asked(json!("main"), json!({})));
    whole(&core, "section_read", asked(json!("Schemas"), json!({})));
    let core = grown();
    let given = whole(
        &core,
        "card_read",
        asked(json!([card(1, 1)]), json!({"depth": 2})),
    );
    assert!(given.contains("Schemas_S03__C01"), "{given}");
    whole(
        &core,
        "section_read",
        asked(json!("Schemas/S05"), json!({})),
    );
    whole(
        &core,
        "branch_read",
        asked(json!("main"), json!({"tags": ["無いタグ"]})),
    );
}

#[test]
fn a_flow_chart_and_revision_only_are_not_cut() {
    let core = core();
    // A flow of 80 nodes in a row, each calling at length.
    const NODES: usize = 80;
    let mut made = Map::new();
    made.insert("Charts/Shop:Long".into(), json!({"kind": "flow"}));
    made.insert(
        "Charts/Shop:Long.start".into(),
        json!({"kind": "begin", "trigger": "manual", "connections": [{"line": "next", "to": "w01"}]}),
    );
    for n in 1..=NODES {
        let next = if n < NODES {
            format!("w{:02}", n + 1)
        } else {
            "done".into()
        };
        let steps: Vec<Value> = (1..=10)
            .map(|i| json!({"do": "call", "program": format!("program-{n}-{i}-{}", "long-name-".repeat(8))}))
            .collect();
        made.insert(
            format!("Charts/Shop:Long.w{n:02}"),
            json!({"kind": "process", "steps": steps, "connections": [{"line": "next", "to": next}]}),
        );
    }
    made.insert(
        "Charts/Shop:Long.done".into(),
        json!({"kind": "end", "outcome": "success"}),
    );
    mcp(
        &core,
        "card_create",
        json!({"project": SAMPLE, "value": made}),
    );
    let arguments = asked(json!("Charts/Shop:Long"), json!({}));
    let given = mcp(&core, "card_read", arguments.clone());
    assert!(chars(text(&given)) > MOST, "{}", chars(text(&given)));
    assert_eq!(given, screen(&core, "card_read", arguments.clone()));
    assert!(given.get("truncated").is_none());
    // revisionOnly: the rev and what it was of, whatever the size (the sample grown).
    let core = grown();
    let arguments = asked(json!("main"), json!({"revisionOnly": true}));
    let read = mcp(&core, "branch_read", arguments);
    assert_eq!(
        read.as_object().unwrap().keys().collect::<Vec<_>>(),
        ["branch", "rev", "spec"]
    );
}
