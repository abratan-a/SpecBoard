//! A flow's sequence diagram is as long as the flow: each node drawn once, ways that part an `alt`
//! that ends where they meet, a way back a `loop` around what it goes back over — whatever shape the
//! ways take, so that a read of one flow cannot run on and on. One too large to draw is refused,
//! named, rather than drawn on.

use crate::common;

use common::{ok, op, project_id, refused, script_store, text_in};
use serde_json::{Map, Value, json};
use specboard_core::Core;
use std::sync::mpsc;
use std::time::{Duration, Instant};

const SECTION: &str = "Charts/Shop";

/// `k` diamonds in a row — an eval `e{i}` whose `A` goes to `a{i}` and whose else to `b{i}`, both
/// on to the next — then an eval `fin` going to the end, and with `back` back to `e0` too.
fn diamonds(k: usize, back: bool) -> (String, Map<String, Value>) {
    let name = format!("Dia{k}{}", if back { "B" } else { "" });
    let flow = format!("{SECTION}:{name}");
    let mut v = Map::new();
    v.insert(
        flow.clone(),
        json!({"kind": "flow", "label": name, "diagram": "sequence"}),
    );
    v.insert(
        format!("{flow}.start"),
        json!({"kind": "begin", "trigger": "manual", "connections": [{"line": "next", "to": "e0"}]}),
    );
    for i in 0..k {
        let next = if i + 1 < k {
            format!("e{}", i + 1)
        } else {
            "fin".to_string()
        };
        v.insert(
            format!("{flow}.e{i}"),
            json!({"kind": "eval", "connections": [
                {"line": "condition", "to": format!("a{i}"), "fields": {"when": "A"}},
                {"line": "else", "to": format!("b{i}")}
            ]}),
        );
        v.insert(
            format!("{flow}.a{i}"),
            json!({"kind": "run", "connections": [
                {"line": "runs", "to": format!("{SECTION}:Record")},
                {"line": "next", "to": next}
            ]}),
        );
        v.insert(
            format!("{flow}.b{i}"),
            json!({"kind": "run", "connections": [
                {"line": "runs", "to": format!("{SECTION}:Verify")},
                {"line": "next", "to": next}
            ]}),
        );
    }
    let mut last = vec![json!({"line": "else", "to": "done"})];
    if back {
        last.insert(
            0,
            json!({"line": "condition", "to": "e0", "fields": {"when": "頭が動いた"}}),
        );
    }
    v.insert(
        format!("{flow}.fin"),
        json!({"kind": "eval", "connections": last}),
    );
    v.insert(
        format!("{flow}.done"),
        json!({"kind": "end", "outcome": "success"}),
    );
    (flow, v)
}

/// A store with the flows the diamonds run, and `shapes` of them.
fn with_diamonds(shapes: &[(usize, bool)]) -> (Core, Vec<String>) {
    let core = Core::open(script_store()).unwrap();
    let mut value = Map::new();
    for called in ["Record", "Verify"] {
        value.insert(
            format!("{SECTION}:{called}"),
            json!({"kind": "flow", "label": called}),
        );
    }
    let mut flows = Vec::new();
    for &(k, back) in shapes {
        let (flow, cards) = diamonds(k, back);
        value.extend(cards);
        flows.push(flow);
    }
    ok(&core, "card_create", json!({ "value": value }));
    (core, flows)
}

fn sequence(core: &Core, flow: &str) -> String {
    let chart = ok(
        core,
        "card_read",
        json!({"target": flow, "value": {"as": "diagram", "shape": "sequence"}}),
    );
    text_in(&chart, "mermaid")
}

/// `work` on a thread of its own, failed when it takes longer than `most`: a diagram that grows
/// with the paths through a flow rather than with the flow would not end at all.
fn within<T: Send + 'static>(most: Duration, work: impl FnOnce() -> T + Send + 'static) -> T {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(work());
    });
    rx.recv_timeout(most)
        .unwrap_or_else(|_| panic!("{most:?} のうちに描き終わらない"))
}

#[test]
fn diamonds_with_a_way_back_grow_with_the_flow_not_with_its_paths() {
    let sizes = within(Duration::from_secs(20), || {
        let shapes = [(4, true), (11, true), (22, true), (20, false)];
        let (core, flows) = with_diamonds(&shapes);
        flows
            .iter()
            .map(|f| {
                let at = Instant::now();
                let chart = sequence(&core, f);
                (f.clone(), chart.chars().count(), at.elapsed())
            })
            .collect::<Vec<_>>()
    });
    let size = |name: &str| sizes.iter().find(|s| s.0.ends_with(name)).unwrap().1;
    for (flow, chars, took) in &sizes {
        assert!(*took < Duration::from_secs(2), "{flow}: {took:?}");
        assert!(*chars < 10_000, "{flow}: {chars} 字");
    }
    assert!(size(":Dia22B") < 3 * size(":Dia11B"), "{sizes:?}");
    // The way back adds one loop around the rest, not a copy of what it goes back over.
    assert!(size(":Dia22B") < 2 * size(":Dia20"), "{sizes:?}");
}

#[test]
fn a_diamond_is_an_alt_the_way_back_one_loop_and_what_follows_a_meeting_is_drawn_once() {
    let (core, flows) = with_diamonds(&[(3, true)]);
    let chart = sequence(&core, &flows[0]);
    let lines: Vec<&str> = chart.lines().map(str::trim).collect();
    let count = |line: &str| lines.iter().filter(|l| **l == line).count();
    assert_eq!(count("alt A"), 3, "{chart}");
    assert_eq!(count("loop A"), 0, "{chart}");
    assert_eq!(count("loop 頭が動いた"), 1, "{chart}");
    assert_eq!(count("else それ以外"), 3, "{chart}");
    // Each run's call once: the ways meet before the next eval, which is drawn after the alt.
    assert_eq!(chart.matches("->>P2").count(), 3, "{chart}");
    assert_eq!(chart.matches("->>P3").count(), 3, "{chart}");
    // The loop holds the three alts and ends before the end: the flow, started by hand, then ends
    // with its mark.
    let opened = lines.iter().position(|l| *l == "loop 頭が動いた").unwrap();
    let first_alt = lines.iter().position(|l| *l == "alt A").unwrap();
    assert!(opened < first_alt, "{chart}");
    assert_eq!(lines[lines.len() - 2], "end", "{chart}");
    assert_eq!(lines[lines.len() - 1], "Note over P1: ■ success", "{chart}");
    let mut depth = 0usize;
    for l in &lines[opened..] {
        if l.starts_with("loop") || l.starts_with("alt") {
            depth += 1;
        } else if *l == "end" {
            depth -= 1;
        }
    }
    assert_eq!(depth, 0, "{chart}");
}

#[test]
fn a_way_into_the_middle_of_another_way_is_said_once_not_copied() {
    // `x` is reached from both ways of `e` and from inside `y`'s way: drawn once, the other ways
    // that reach it saying where they go on.
    let core = Core::open(script_store()).unwrap();
    let flow = format!("{SECTION}:Tangle");
    ok(
        &core,
        "card_create",
        json!({"value": {
            &flow: {"kind": "flow", "label": "Tangle", "diagram": "sequence"},
            format!("{flow}.start"): {"kind": "begin", "trigger": "manual", "connections": [{"line": "next", "to": "e"}]},
            format!("{flow}.e"): {"kind": "eval", "connections": [
                {"line": "condition", "to": "p", "fields": {"when": "P"}},
                {"line": "condition", "to": "q", "fields": {"when": "Q"}},
                {"line": "else", "to": "r"}
            ]},
            format!("{flow}.p"): {"kind": "process", "label": "ぴー", "connections": [{"line": "next", "to": "f"}]},
            format!("{flow}.f"): {"kind": "eval", "connections": [
                {"line": "condition", "to": "x", "fields": {"when": "X"}},
                {"line": "else", "to": "y"}
            ]},
            format!("{flow}.q"): {"kind": "process", "label": "きゅー", "connections": [{"line": "next", "to": "y"}]},
            format!("{flow}.x"): {"kind": "process", "label": "えっくす", "connections": [{"line": "next", "to": "z"}]},
            format!("{flow}.y"): {"kind": "process", "label": "わい", "connections": [{"line": "next", "to": "x"}]},
            format!("{flow}.r"): {"kind": "process", "label": "あーる", "connections": [{"line": "next", "to": "z"}]},
            format!("{flow}.z"): {"kind": "process", "label": "ぜっと", "connections": [{"line": "next", "to": "done"}]},
            format!("{flow}.done"): {"kind": "end", "outcome": "success"}
        }}),
    );
    let chart = sequence(&core, &flow);
    // Each node's bar once; a way that goes on to one names it in a note, which is no copy.
    for once in [
        "p ぴー",
        "q きゅー",
        "x えっくす",
        "y わい",
        "r あーる",
        "z ぜっと",
    ] {
        let bar = format!("Note over P1: {once}");
        assert_eq!(chart.matches(bar.as_str()).count(), 1, "{once}: {chart}");
    }
}

#[test]
fn a_diagram_too_large_to_draw_is_refused_naming_the_flow() {
    let core = Core::open(script_store()).unwrap();
    let flow = format!("{SECTION}:Huge");
    let long = "長い説明".repeat(60_000);
    ok(
        &core,
        "card_create",
        json!({"value": {
            &flow: {"kind": "flow", "label": "Huge", "diagram": "sequence"},
            format!("{flow}.start"): {"kind": "begin", "trigger": "manual", "connections": [{"line": "next", "to": "say"}]},
            format!("{flow}.say"): {"kind": "process", "steps": [{"do": "compute", "description": long}],
                "connections": [{"line": "next", "to": "done"}]},
            format!("{flow}.done"): {"kind": "end", "outcome": "success"}
        }}),
    );
    let said = refused(
        &core,
        "card_read",
        json!({"target": &flow, "value": {"as": "diagram"}}),
    );
    assert!(said.contains(&flow), "{said}");
    assert!(said.contains("200000"), "{said}");
    let said = refused(
        &core,
        "card_read",
        json!({"target": &flow, "value": {"as": "diagram", "revisionOnly": true}}),
    );
    assert!(said.contains(&flow), "{said}");
    // Drawn as a flowchart it is still read.
    ok(
        &core,
        "card_read",
        json!({"target": &flow, "value": {"as": "diagram", "shape": "flowchart"}}),
    );
}

#[test]
fn a_document_holding_a_diagram_too_large_says_so_in_its_place() {
    // A document is still read and written out: the diagram in it says why it is not drawn.
    let core = Core::open(script_store()).unwrap();
    let flow = format!("{SECTION}:Huge");
    let long = "長い説明".repeat(60_000);
    ok(
        &core,
        "card_create",
        json!({"value": {
            &flow: {"kind": "flow", "label": "Huge", "diagram": "sequence"},
            format!("{flow}.start"): {"kind": "begin", "trigger": "manual", "connections": [{"line": "next", "to": "say"}]},
            format!("{flow}.say"): {"kind": "process", "steps": [{"do": "compute", "description": long}],
                "connections": [{"line": "next", "to": "done"}]},
            format!("{flow}.done"): {"kind": "end", "outcome": "success"}
        }}),
    );
    let drawn = op(
        &core,
        json!({"op": "embed_diagram", "project": project_id(&core), "path": &flow, "option": "sequence"}),
    )["result"]["mermaid"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    assert!(drawn.contains("200000"), "{drawn}");
    assert!(drawn.chars().count() < 1_000, "{}", drawn.chars().count());
}
