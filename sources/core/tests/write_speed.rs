//! How long writes take on a project of about 3,000 cards, and how long a read of another project
//! waits while one runs. Times are not held here — a runner's are not steady — and nothing here
//! runs in CI: it is run by hand, built for release:
//!
//! ```sh
//! cargo test --release --test integration write_speed:: -- --ignored --nocapture
//! ```
//!
//! The project is made of sections of 50 entities (11 fields each, a `points` line to the entity
//! before) under `Schemas`, and a freedoc of 20 `[[…]]` for each under `Docs`. `SB_PERF_SECTIONS` sets how many (60 by default: 3,060 cards). It is made once through
//! MCP and kept as a project file (`SB_PERF_CACHE`, else the target's temporary folder), then read in.
//! `SB_PERF_ONLY` takes only the measures it names
//! (`read,detail,light,many,batch,staged,create,delete,rename,lines,board,beside,flows,flow,review,faces`);
//! `SB_PERF_STAGED` is how many steps the staged batch has (50).

use crate::common;

use common::{call, op, place, project_file};
use serde_json::{Map, Value, json};
use specboard_core::{Core, project_rebuilds_on_this_thread};
use std::path::PathBuf;
use std::time::{Duration, Instant};

const PERF: &str = "perf";
const OTHER: &str = "見本";

fn sections() -> usize {
    std::env::var("SB_PERF_SECTIONS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(60)
}

fn entity(i: usize, j: usize) -> String {
    format!("Schemas/S{i:02}:E{i:02}x{j:02}")
}

fn ok(core: &Core, tool: &str, arguments: Value) -> Value {
    let (failed, text) = call(core, tool, arguments.clone());
    assert!(!failed, "{tool}: {}", &text[..text.len().min(2000)]);
    serde_json::from_str(&text).unwrap_or(Value::Null)
}

/// The process's resident memory, in MB (`ps`).
fn rss_mb() -> f64 {
    let out = std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &std::process::id().to_string()])
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout)
        .trim()
        .parse::<f64>()
        .unwrap_or(0.0)
        / 1024.0
}

fn said(name: &str, took: Duration) {
    println!("measure: {name}: {:.3}s", took.as_secs_f64());
}

/// The perf project made through MCP, section by section, as a project file.
fn made(core: &Core, count: usize) -> String {
    ok(core, "project_create", json!({"project": PERF}));
    for i in 0..count {
        ok(
            core,
            "section_create",
            json!({"project": PERF, "target": format!("Schemas/S{i:02}")}),
        );
        ok(
            core,
            "section_create",
            json!({"project": PERF, "target": format!("Docs/D{i:02}")}),
        );
        let mut value = Map::new();
        for j in 0..50 {
            let before = match (i, j) {
                (0, 0) => None,
                (_, 0) => Some(entity(i - 1, 49)),
                _ => Some(entity(i, j - 1)),
            };
            value.insert(entity(i, j), entity_value(before));
        }
        let started = Instant::now();
        ok(
            core,
            "card_create",
            json!({"project": PERF, "value": value}),
        );
        if i + 1 == count {
            said(
                &format!("card_create of 50 at {} cards", i * 51),
                started.elapsed(),
            );
        }
        let body: Vec<String> = (0..20)
            .map(|j| format!("- [[{}]]", entity(i, j * 2)))
            .collect();
        ok(
            core,
            "card_create",
            json!({"project": PERF, "target": format!("Docs/D{i:02}:Doc{i:02}"),
                   "value": {"kind": "freedoc", "body": body.join("\n")}}),
        );
    }
    let id = op(core, json!({"op": "projects"}))["result"]["projects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == PERF)
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    op(core, json!({"op": "project_file", "project": id}))["result"]["text"]
        .as_str()
        .unwrap()
        .to_string()
}

/// A store with the sample project and the perf project in it.
fn perf_store() -> PathBuf {
    let count = sections();
    let cache = std::env::var("SB_PERF_CACHE")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(env!("CARGO_TARGET_TMPDIR")))
        .join(format!("perf-{count}.json"));
    let text = match std::fs::read_to_string(&cache) {
        Ok(text) => text,
        Err(_) => {
            let store = place("perf-make.sqlite");
            let core = Core::create(&store).unwrap();
            let before = rss_mb();
            let started = Instant::now();
            let text = made(&core, count);
            said(&format!("made {} cards", count * 51), started.elapsed());
            println!(
                "measure: memory making it: {before:.0}MB -> {:.0}MB",
                rss_mb()
            );
            std::fs::create_dir_all(cache.parent().unwrap()).unwrap();
            std::fs::write(&cache, &text).unwrap();
            core.close();
            text
        }
    };
    let store = place("perf.sqlite");
    let core = Core::create(&store).unwrap();
    for file in [project_file("sample"), text] {
        let read = op(&core, json!({"op": "project_file_read", "text": file}));
        assert!(read.get("error").is_none(), "{read}");
    }
    core.close();
    store
}

fn median(mut times: Vec<Duration>) -> Duration {
    times.sort();
    times[times.len() / 2]
}

/// Whether the measure `name` is taken: `SB_PERF_ONLY` names those to take (`,` between), else all.
fn taken(name: &str) -> bool {
    std::env::var("SB_PERF_ONLY").map_or(true, |only| only.split(',').any(|n| n.trim() == name))
}

/// An entity's value as the perf project's are (11 fields, a `points` line to `before`).
fn entity_value(before: Option<String>) -> Value {
    let mut fields = vec![json!({"name": "id", "type": "string"})];
    fields.extend((1..10).map(|k| json!({"name": format!("f{k}"), "type": "string"})));
    fields.push(json!({"name": "prev_id", "type": "string"}));
    let mut card = json!({"kind": "entity", "fields": fields});
    if let Some(to) = before {
        card["connections"] = json!([{"line": "points", "from": "prev_id", "to": to, "at": "id"}]);
    }
    card
}

/// A flow `Charts/Flows:{name}` and its 6 nodes, as paths to cards for one `card_create`: a begin, 4
/// processes (each a step reading an entity of section `i`, one `rename` leaves as they are) and an
/// end, a `next` line between each.
fn flow_value(value: &mut Map<String, Value>, name: &str, i: usize) {
    let flow = format!("Charts/Flows:{name}");
    value.insert(flow.clone(), json!({"kind": "flow", "label": name}));
    value.insert(
        format!("{flow}.start"),
        json!({"kind": "begin", "trigger": "manual", "connections": [{"line": "next", "to": "p1"}]}),
    );
    for k in 1..=4 {
        let next = if k == 4 {
            "done".to_string()
        } else {
            format!("p{}", k + 1)
        };
        value.insert(
            format!("{flow}.p{k}"),
            json!({"kind": "process", "label": format!("手 {k}"),
            "steps": [{"do": "read"}, {"do": "call", "program": format!("work{k}")}],
            "connections": [
                {"line": "reads", "from": "steps[0]", "to": entity(i, k * 10), "at": "id"},
                {"line": "next", "to": next}
            ]}),
        );
    }
    value.insert(
        format!("{flow}.done"),
        json!({"kind": "end", "outcome": "success"}),
    );
}

#[test]
#[ignore]
fn how_long_writes_take_on_a_large_project() {
    let store = perf_store();
    let core = Core::open(&store).unwrap();
    // The key is made on the first call: not timed.
    ok(&core, "project_list", json!({}));
    println!("measure: memory opened: {:.0}MB", rss_mb());
    let count = sections();

    if taken("read") {
        let reads: Vec<Duration> = (0..3)
            .map(|_| {
                let started = Instant::now();
                ok(
                    &core,
                    "card_read",
                    json!({"project": PERF, "target": entity(0, 1)}),
                );
                started.elapsed()
            })
            .collect();
        said("card_read of one card", median(reads));
    }

    // The window's reading: the whole, and what one card written changes put against it (`since`).
    if taken("detail") {
        let project = op(&core, json!({"op": "projects"}))["result"]["projects"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["name"] == PERF)
            .unwrap()["id"]
            .clone();
        let read = |since: Option<&Value>| -> (Duration, usize, Value) {
            let mut request = json!({"op": "project_detail", "project": project});
            if let Some(since) = since {
                request["since"] = since.clone();
            }
            let started = Instant::now();
            let text = core.call(&request.to_string());
            let took = started.elapsed();
            let answer: Value = serde_json::from_str(&text).unwrap();
            (took, text.len(), answer["result"]["digest"].clone())
        };
        let (took, bytes, digest) = read(None);
        said(&format!("project_detail whole ({bytes} bytes)"), took);
        ok(
            &core,
            "card_update",
            json!({"project": PERF, "target": entity(2, 2), "value": {"description": "差分"}}),
        );
        let (took, bytes, _) = read(Some(&digest));
        said(
            &format!("project_detail after one card ({bytes} bytes)"),
            took,
        );
    }

    if taken("light") {
        let light: Vec<Duration> = (0..3)
            .map(|n| {
                let started = Instant::now();
                ok(
                    &core,
                    "card_update",
                    json!({"project": PERF, "target": entity(1, 1), "value": {"label": format!("軽い {n}")}}),
                );
                started.elapsed()
            })
            .collect();
        said("card_update of one label", median(light));
        println!("measure: memory after light writes: {:.0}MB", rss_mb());
    }

    let many = |n: usize, word: &str| -> Value {
        let mut value = Map::new();
        for k in 0..n {
            value.insert(
                entity(k / 50 % count, k % 50),
                json!({"label": format!("{word} {k}")}),
            );
        }
        json!({"project": PERF, "value": value})
    };
    if taken("many") {
        let started = Instant::now();
        ok(&core, "card_update", many(200, "まとめて"));
        said("card_update of 200 labels in one call", started.elapsed());
        println!("measure: memory after 200: {:.0}MB", rss_mb());
    }

    let step = |k: usize| -> Value {
        json!({"tool": "card_update", "arguments": {
            "target": entity(2 % count, k % 50),
            "value": {"label": format!("手 {k}")}}})
    };
    if taken("batch") {
        let steps: Vec<Value> = (0..50).map(step).collect();
        let started = Instant::now();
        ok(
            &core,
            "batch",
            json!({"project": PERF, "value": {"steps": steps}, "execute": true}),
        );
        said("batch of 50 steps", started.elapsed());
        println!("measure: memory after the batch: {:.0}MB", rss_mb());
    }

    // A batch staged a step at a time: each call adds one, and every step is tried again.
    // `SB_PERF_STAGED` steps (50 by default).
    if taken("staged") {
        let staged: usize = std::env::var("SB_PERF_STAGED")
            .ok()
            .and_then(|n| n.parse().ok())
            .unwrap_or(50);
        let started = Instant::now();
        let opened = ok(
            &core,
            "batch",
            json!({"project": PERF, "value": {"steps": [step(0)], "open": true}}),
        );
        let token = opened["batch"].as_str().unwrap().to_string();
        let mut last = Duration::ZERO;
        for k in 1..staged {
            let added = Instant::now();
            ok(
                &core,
                "batch",
                json!({"project": PERF, "target": token, "value": {"steps": [step(k)]}}),
            );
            last = added.elapsed();
        }
        said(
            &format!("batch staged a step at a time, {staged} calls"),
            started.elapsed(),
        );
        said(&format!("batch staged: adding step {staged}"), last);
        ok(
            &core,
            "batch",
            json!({"project": PERF, "target": token, "value": {"discard": true}}),
        );
    }

    // 50 cards made in one call, and let go in one call.
    if taken("create") {
        ok(
            &core,
            "section_create",
            json!({"project": PERF, "target": "Schemas/Made"}),
        );
        let mut value = Map::new();
        for j in 0..50 {
            let before = (j > 0).then(|| format!("Schemas/Made:M{:02}", j - 1));
            value.insert(format!("Schemas/Made:M{j:02}"), entity_value(before));
        }
        let started = Instant::now();
        ok(
            &core,
            "card_create",
            json!({"project": PERF, "value": value}),
        );
        said("card_create of 50 in one call", started.elapsed());
        if taken("delete") {
            let targets: Vec<String> = (0..50).map(|j| format!("Schemas/Made:M{j:02}")).collect();
            let started = Instant::now();
            ok(
                &core,
                "card_delete",
                json!({"project": PERF, "target": targets}),
            );
            said("card_delete of 50 in one call", started.elapsed());
        }
    }

    // 200 cards renamed in one call (sections 10–13): what names them elsewhere follows them.
    if taken("rename") && count >= 14 {
        let mut value = Map::new();
        for k in 0..200 {
            let (i, j) = (10 + k / 50, k % 50);
            value.insert(entity(i, j), json!({"name": format!("R{i:02}x{j:02}")}));
        }
        let started = Instant::now();
        ok(
            &core,
            "card_update",
            json!({"project": PERF, "value": value}),
        );
        said("card_update renaming 200 in one call", started.elapsed());
    }

    // 200 lines added in one call (sections 20–23): each card a line more, to the card beside it.
    if taken("lines") && count >= 24 {
        let mut value = Map::new();
        for k in 0..200 {
            let (i, j) = (20 + k / 50, k % 50);
            value.insert(
                entity(i, j),
                json!({"connections": {"add": [
                    {"line": "points", "from": "f1", "to": entity(i, (j + 1) % 50), "at": "id"}
                ]}}),
            );
        }
        let started = Instant::now();
        ok(
            &core,
            "card_update",
            json!({"project": PERF, "value": value}),
        );
        said(
            "card_update adding 200 lines in one call",
            started.elapsed(),
        );
    }

    // A goal and its tasks on the board, beside main's 3,000 cards its lines may reach: a task's
    // field and the goal's written, a step at a time.
    if taken("board") {
        let mut value = Map::new();
        value.insert("Goals:Perf".into(), json!({"kind": "goal", "body": "測る"}));
        for t in 0..10 {
            value.insert(
                format!("Goals:Perf.T{t}"),
                json!({"kind": "task", "body": format!("[[Schemas/S00:E00x{t:02}]]")}),
            );
        }
        ok(
            &core,
            "card_create",
            json!({"project": PERF, "branch": "_project_board_", "value": value}),
        );
        let timed = |target: &str, value: Value| -> Duration {
            let runs: Vec<Duration> = (0..3)
                .map(|n| {
                    let mut value = value.clone();
                    if let Some(body) = value.get_mut("body") {
                        *body = format!("{} {n}", body.as_str().unwrap_or_default()).into();
                    }
                    let started = Instant::now();
                    ok(
                        &core,
                        "card_update",
                        json!({"project": PERF, "branch": "_project_board_", "target": target, "value": value}),
                    );
                    started.elapsed()
                })
                .collect();
            median(runs)
        };
        said(
            "board: a task's done",
            timed("Goals:Perf.T1", json!({"done": true})),
        );
        said(
            "board: a task's body",
            timed("Goals:Perf.T2", json!({"body": "書き直した"})),
        );
        said(
            "board: the goal's body",
            timed("Goals:Perf", json!({"body": "書き直した"})),
        );
    }

    // A read of another project while a write goes on: the 200 labels written,
    // and 50 cards let go of in one call.
    if taken("beside") {
        // The write's time, the other project's read's and the written project's read's.
        let beside = |write: &(dyn Fn() + Sync)| -> (Duration, Duration, Duration) {
            std::thread::scope(|s| {
                let writer = s.spawn(|| {
                    let started = Instant::now();
                    write();
                    started.elapsed()
                });
                std::thread::sleep(Duration::from_millis(300));
                let started = Instant::now();
                ok(
                    &core,
                    "card_read",
                    json!({"project": OTHER, "target": "Free:Memo"}),
                );
                let other = started.elapsed();
                let started = Instant::now();
                ok(
                    &core,
                    "card_read",
                    json!({"project": PERF, "target": entity(0, 1)}),
                );
                let same = started.elapsed();
                (writer.join().unwrap(), other, same)
            })
        };
        let (write_took, other, _) = beside(&|| {
            ok(&core, "card_update", many(200, "読みの間に"));
        });
        said("card_update of 200 with a read beside it", write_took);
        said("card_read of another project beside the 200", other);
        ok(
            &core,
            "section_create",
            json!({"project": PERF, "target": "Schemas/Beside"}),
        );
        let mut value = Map::new();
        for j in 0..50 {
            let before = (j > 0).then(|| format!("Schemas/Beside:B{:02}", j - 1));
            value.insert(format!("Schemas/Beside:B{j:02}"), entity_value(before));
        }
        ok(
            &core,
            "card_create",
            json!({"project": PERF, "value": value}),
        );
        let targets: Vec<String> = (0..50).map(|j| format!("Schemas/Beside:B{j:02}")).collect();
        let (write_took, other, same) = beside(&|| {
            ok(
                &core,
                "card_delete",
                json!({"project": PERF, "target": targets}),
            );
        });
        said("card_delete of 50 with reads beside it", write_took);
        said("card_read of another project beside the 50 let go", other);
        said("card_read of the same project beside the 50 let go", same);
    }
    // 50 flows of 6 nodes each made in one call, and a flow alone (the median of 3).
    if taken("flows") || taken("flow") {
        ok(
            &core,
            "section_create",
            json!({"project": PERF, "target": "Charts/Flows"}),
        );
    }
    if taken("flows") {
        let mut value = Map::new();
        for f in 0..50 {
            flow_value(&mut value, &format!("F{f:02}"), f % count.min(10));
        }
        let rebuilds = project_rebuilds_on_this_thread();
        let started = Instant::now();
        ok(
            &core,
            "card_create",
            json!({"project": PERF, "value": value}),
        );
        said(
            "card_create of 50 flows of 6 nodes in one call",
            started.elapsed(),
        );
        println!(
            "measure: worked out whole for the 50 flows: {} times",
            project_rebuilds_on_this_thread() - rebuilds
        );
    }
    if taken("flow") {
        let rebuilds = project_rebuilds_on_this_thread();
        let runs: Vec<Duration> = (0..3)
            .map(|n| {
                let mut value = Map::new();
                flow_value(&mut value, &format!("One{n}"), n % count.min(10));
                let started = Instant::now();
                ok(
                    &core,
                    "card_create",
                    json!({"project": PERF, "value": value}),
                );
                started.elapsed()
            })
            .collect();
        said("card_create of a flow of 6 nodes", median(runs));
        println!(
            "measure: worked out whole for a flow: {} times",
            (project_rebuilds_on_this_thread() - rebuilds) / 3
        );
    }
    // Review marks: a ui section of 100 parts, each with lines to 3 entities, 30 of the entities
    // changed after: the window's whole reading and the check, each reading the tree's history once.
    if taken("review") {
        let mut tree = ok(&core, "project_read", json!({"project": PERF}))["projectTree"].clone();
        tree["main_sections"]
            .as_array_mut()
            .unwrap()
            .push(json!({"type": "ui", "name": "UI"}));
        ok(
            &core,
            "project_update",
            json!({"project": PERF, "value": {"project_tree": tree}}),
        );
        let mut value = Map::new();
        for p in 0..100 {
            let lines: Vec<Value> = (0..3)
                .map(|k| json!({"line": "link", "to": entity((p + k) % count, p % 50)}))
                .collect();
            value.insert(
                format!("UI:Part{p:03}"),
                json!({"kind": "html", "body": format!("<p>{p}</p>"), "connections": lines}),
            );
        }
        ok(
            &core,
            "card_create",
            json!({"project": PERF, "value": value}),
        );
        for p in 0..30 {
            ok(
                &core,
                "card_update",
                json!({"project": PERF, "target": entity(p % count, p % 50), "value": {"description": "変えた"}}),
            );
        }
        let project = op(&core, json!({"op": "projects"}))["result"]["projects"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["name"] == PERF)
            .unwrap()["id"]
            .clone();
        let reads: Vec<Duration> = (0..5)
            .map(|_| {
                let started = Instant::now();
                core.call(&json!({"op": "project_detail", "project": project}).to_string());
                started.elapsed()
            })
            .collect();
        said(
            "project_detail whole with 100 parts to review",
            median(reads),
        );
        let checks: Vec<Duration> = (0..3)
            .map(|_| {
                let started = Instant::now();
                let found = ok(
                    &core,
                    "inspection_list",
                    json!({"project": PERF, "value": {"group": "review"}}),
                );
                assert!(found.to_string().contains("review"), "{found}");
                started.elapsed()
            })
            .collect();
        said("inspection_list of the review group", median(checks));
    }
    // Faces: a ui section of 40 parts (html of a few kB, svg) and 4 frames, each of a section of 10
    // parts and a frame of another such section: the mock alone, and with each part's page.
    if taken("faces") {
        let mut tree = ok(&core, "project_read", json!({"project": PERF}))["projectTree"].clone();
        tree["main_sections"]
            .as_array_mut()
            .unwrap()
            .push(json!({"type": "ui", "name": "Faces"}));
        ok(
            &core,
            "project_update",
            json!({"project": PERF, "value": {"project_tree": tree}}),
        );
        let row = "<div class=\"row\"><span>項目</span><b>値</b></div>".repeat(40);
        let page = format!("<style>.row{{display:flex;gap:8px}}</style><section>{row}</section>");
        let picture = format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 100 100\">{}</svg>",
            "<rect x=\"1\" y=\"1\" width=\"9\" height=\"9\"/>".repeat(50)
        );
        let mut value = Map::new();
        let mut places = Map::new();
        for f in 0..5 {
            let section = format!("Faces/Inner{f}");
            ok(
                &core,
                "section_create",
                json!({"project": PERF, "target": section}),
            );
            for p in 0..10 {
                value.insert(
                    format!("{section}:Part{p}"),
                    json!({"kind": "html", "body": page}),
                );
            }
        }
        for f in 0..4 {
            value.insert(
                format!("Faces/Inner{f}:Next"),
                json!({"kind": "frame", "target": format!("Faces/Inner{}", f + 1)}),
            );
            value.insert(
                format!("Faces:Frame{f}"),
                json!({"kind": "frame", "target": format!("Faces/Inner{f}")}),
            );
            places.insert(
                format!("Faces:Frame{f}"),
                json!({"topLeft": {"x": f * 20, "y": 40}, "size": {"w": 20, "h": 8}}),
            );
        }
        for p in 0..40 {
            let body = if p % 4 == 0 {
                picture.clone()
            } else {
                page.clone()
            };
            let kind = if p % 4 == 0 { "svg" } else { "html" };
            value.insert(
                format!("Faces:Part{p:02}"),
                json!({"kind": kind, "body": body}),
            );
            places.insert(
                format!("Faces:Part{p:02}"),
                json!({"topLeft": {"x": (p % 8) * 10, "y": (p / 8) * 8}, "size": {"w": 10, "h": 8}}),
            );
        }
        ok(
            &core,
            "card_create",
            json!({"project": PERF, "value": value}),
        );
        ok(
            &core,
            "section_update",
            json!({"project": PERF, "target": "Faces", "value": {"places": places}}),
        );
        for (name, value) in [
            ("section_read as: mock", json!({"as": "mock"})),
            (
                "section_read as: mock with faces",
                json!({"as": "mock", "faces": true}),
            ),
            (
                "section_read as: mock without notes",
                json!({"as": "mock", "notes": false}),
            ),
            (
                "section_read as: mock without notes, with faces",
                json!({"as": "mock", "notes": false, "faces": true}),
            ),
        ] {
            let reads: Vec<Duration> = (0..5)
                .map(|_| {
                    let started = Instant::now();
                    let read = ok(
                        &core,
                        "section_read",
                        json!({"project": PERF, "target": "Faces", "value": value}),
                    );
                    assert!(read["parts"].as_u64().unwrap() > 40, "{name}");
                    started.elapsed()
                })
                .collect();
            said(name, median(reads));
        }
        let read = ok(
            &core,
            "section_read",
            json!({"project": PERF, "target": "Faces", "value": {"as": "mock", "faces": true}}),
        );
        let faces = read["faces"].as_array().unwrap();
        let chars: usize = faces
            .iter()
            .map(|f| f["html"].as_str().unwrap().len())
            .sum();
        println!(
            "measure: faces: {} pages, {chars} bytes; the mock {} parts, {} frames",
            faces.len(),
            read["parts"],
            read["frames"]
        );
    }
    println!("measure: memory at the end: {:.0}MB", rss_mb());
    core.close();
}
