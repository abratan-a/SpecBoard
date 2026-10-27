//! A class diagram carries a line on through the cards it does not draw only when they are value
//! objects and enums folded into what holds them. Any other card left undrawn ends the way: a web
//! of flows and documents that run, link and mention each other would otherwise draw a line for
//! every route through it, and those routes multiply with each card in the web. Even through folded
//! cards alone the ways are counted, and a diagram with too many is refused, named, rather than
//! drawn on.

use crate::common;

use common::{ok, op, project_id, refused, script_store, text_in};
use serde_json::{Map, Value, json};
use specboard_core::Core;
use std::sync::mpsc;
use std::time::Duration;

const SECTION: &str = "Charts/Shop";

/// `work` on a thread of its own, failed when it takes longer than `most`: a diagram that grows
/// with the routes through the cards it leaves out would not end at all.
fn within<T: Send + 'static>(most: Duration, work: impl FnOnce() -> T + Send + 'static) -> T {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(work());
    });
    rx.recv_timeout(most)
        .unwrap_or_else(|_| panic!("{most:?} のうちに描き終わらない"))
}

/// A flow `name` whose run nodes run `runs`, one node each, in a row.
fn flow(v: &mut Map<String, Value>, name: &str, runs: &[String]) {
    let flow = format!("{SECTION}:{name}");
    v.insert(flow.clone(), json!({"kind": "flow", "label": name}));
    let node = |n: usize| {
        if n < runs.len() {
            format!("r{n}")
        } else {
            "done".to_string()
        }
    };
    v.insert(
        format!("{flow}.start"),
        json!({"kind": "begin", "trigger": "manual", "connections": [{"line": "next", "to": node(0)}]}),
    );
    for (n, to) in runs.iter().enumerate() {
        v.insert(
            format!("{flow}.r{n}"),
            json!({"kind": "run", "connections": [
                {"line": "runs", "to": format!("{SECTION}:{to}")},
                {"line": "next", "to": node(n + 1)}
            ]}),
        );
    }
    v.insert(
        format!("{flow}.done"),
        json!({"kind": "end", "outcome": "success"}),
    );
}

/// `MeshA` runs `MeshB` and `MeshHub`; the hub runs `k` flows that each run every other of them
/// and `MeshB`. Read with `MeshA` and `MeshB`, the `k` are two hops out, so none is drawn.
fn mesh(k: usize) -> Core {
    let core = Core::open(script_store()).unwrap();
    let mut v = Map::new();
    let cluster: Vec<String> = (0..k).map(|i| format!("MeshC{i}")).collect();
    flow(&mut v, "MeshB", &[]);
    for (i, c) in cluster.iter().enumerate() {
        let mut runs: Vec<String> = cluster
            .iter()
            .enumerate()
            .filter(|(j, _)| *j != i)
            .map(|(_, o)| o.clone())
            .collect();
        runs.push("MeshB".into());
        flow(&mut v, c, &runs);
    }
    flow(&mut v, "MeshHub", &cluster);
    flow(
        &mut v,
        "MeshA",
        &["MeshB".to_string(), "MeshHub".to_string()],
    );
    ok(&core, "card_create", json!({ "value": v }));
    core
}

fn read_a_and_b(core: &Core) -> String {
    let read = ok(
        core,
        "card_read",
        json!({"target": [format!("{SECTION}:MeshA"), format!("{SECTION}:MeshB")], "value": {"as": "diagram"}}),
    );
    text_in(&read, "mermaid")
}

#[test]
fn a_flow_left_undrawn_carries_no_line_on() {
    let chart = within(Duration::from_secs(20), || read_a_and_b(&mesh(3)));
    let lines: Vec<&str> = chart.lines().filter(|l| l.contains("..>")).collect();
    // A runs B and the hub; the hub runs B only through flows not drawn, so no line says it does.
    assert_eq!(lines.len(), 2, "{chart}");
    assert!(
        lines
            .iter()
            .any(|l| l.contains("MeshA ..> Charts_Shop__MeshB")),
        "{chart}"
    );
    assert!(
        lines
            .iter()
            .any(|l| l.contains("MeshA ..> Charts_Shop__MeshHub")),
        "{chart}"
    );
    assert!(!chart.contains("MeshHub ..> Charts_Shop__MeshB"), "{chart}");
}

#[test]
fn a_larger_web_beyond_what_is_drawn_draws_the_same() {
    let (small, large) = within(Duration::from_secs(20), || {
        (read_a_and_b(&mesh(3)), read_a_and_b(&mesh(8)))
    });
    assert_eq!(small, large);
}

/// `Schemas:Deep` holding `wide` fields of the value `Schemas:Deep0`, each `Schemas:Deep{i}` holding
/// `wide` of the next up to `Schemas:Deep6`: the ways down through them are `wide` to the seventh.
fn deep_values(core: &Core, wide: usize) {
    let mut v = Map::new();
    let fields = |to: &str| -> (Vec<Value>, Vec<Value>) {
        (0..wide)
            .map(|f| {
                (
                    json!({"name": format!("f{f}"), "type": "string"}),
                    json!({"line": "embed", "from": format!("f{f}"), "to": to}),
                )
            })
            .unzip()
    };
    let (held, lines) = fields("Schemas:Deep0");
    v.insert(
        "Schemas:Deep".into(),
        json!({"kind": "entity", "fields": held, "connections": lines}),
    );
    for i in 0..7 {
        let card = format!("Schemas:Deep{i}");
        if i < 6 {
            let (held, lines) = fields(&format!("Schemas:Deep{}", i + 1));
            v.insert(
                card,
                json!({"kind": "value", "fields": held, "connections": lines}),
            );
        } else {
            v.insert(card, json!({"kind": "value"}));
        }
    }
    ok(core, "card_create", json!({ "value": v }));
}

#[test]
fn too_many_ways_through_folded_values_are_refused_named_or_said_in_a_note() {
    let (said, embedded) = within(Duration::from_secs(60), || {
        let core = Core::open(script_store()).unwrap();
        deep_values(&core, 8);
        let said = refused(
            &core,
            "card_read",
            json!({"target": "Schemas:Deep", "value": {"as": "diagram"}}),
        );
        let embedded = op(
            &core,
            json!({"op": "embed_diagram", "project": project_id(&core), "path": "Schemas:Deep"}),
        );
        (said, embedded)
    });
    assert!(
        said.contains("Schemas:Deep の図は大きすぎて描けません"),
        "{said}"
    );
    assert!(said.contains("kinds"), "{said}");
    // Written out or put in a document, the diagram is a note saying why in place of the lines.
    let mermaid = embedded["result"]["mermaid"]
        .as_str()
        .unwrap_or_else(|| panic!("{embedded}"));
    assert!(mermaid.contains("図が大きすぎて描けません"), "{mermaid}");
    assert!(!mermaid.contains("*--"), "{mermaid}");
}

#[test]
fn ways_through_folded_values_short_of_the_limit_are_drawn() {
    let chart = within(Duration::from_secs(60), || {
        let core = Core::open(script_store()).unwrap();
        deep_values(&core, 2);
        let read = ok(
            &core,
            "card_read",
            json!({"target": "Schemas:Deep", "value": {"as": "diagram"}}),
        );
        text_in(&read, "mermaid")
    });
    assert!(chart.contains("Schemas:Deep"), "{chart}");
    assert!(!chart.contains("大きすぎ"), "{chart}");
}
