//! What the tests share: the fixture projects read into stores of their own
//! (`tests/fixtures/projects/`), the answers kept (`tests/fixtures/kept/`), and MCP called as an
//! access key calls it.

#![allow(dead_code)]

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

/// The folder this run of a test binary keeps its places in (`sb-tests-<process>` in the temporary
/// folder), taken away with all in it — the stores, their `-wal`・`-shm` and the copies kept beside
/// them, the folders written to — when the run ends, whether the tests passed or not: the harness
/// ends the process through `exit`, after the last test, and `exit` calls what `atexit` was given.
/// One of the same name is left only by a run of a process numbered the same that was killed, and
/// is taken away first.
fn root() -> &'static Path {
    static ROOT: OnceLock<PathBuf> = OnceLock::new();
    unsafe extern "C" {
        fn atexit(callback: extern "C" fn()) -> std::ffi::c_int;
    }
    extern "C" fn sweep() {
        if let Some(root) = ROOT.get() {
            let _ = std::fs::remove_dir_all(root);
        }
    }
    ROOT.get_or_init(|| {
        let root = std::env::temp_dir().join(format!("sb-tests-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        // SAFETY: `sweep` is a plain function that touches only what is set before it is given.
        unsafe { atexit(sweep) };
        root
    })
}

/// A place of its own, not there yet, in the run's folder (`root`): tests run side by side. It is
/// taken away with what sits beside it (a store's `-wal`・`-shm`, its copies) when the test that
/// asked for it ends, so that a run of every test in one process does not keep all their stores
/// until it ends.
pub fn place(name: &str) -> PathBuf {
    static COUNT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = COUNT.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let place = root().join(format!("sb-{name}-{n}"));
    let _ = MINE.try_with(|mine| mine.0.borrow_mut().push(place.clone()));
    place
}

thread_local! {
    /// The places the test running on this thread asked for: the harness runs each test on a thread
    /// of its own, and the thread ends with the test, passed or not. A run on one thread
    /// (`--test-threads=1`) leaves them to the sweep at its end.
    static MINE: Mine = const { Mine(std::cell::RefCell::new(Vec::new())) };
}

struct Mine(std::cell::RefCell<Vec<PathBuf>>);

impl Drop for Mine {
    fn drop(&mut self) {
        for place in self.0.take() {
            let Some(name) = place.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            let Ok(entries) = std::fs::read_dir(root()) else {
                continue;
            };
            for entry in entries.flatten() {
                let found = entry.file_name();
                // What follows the place's name is not a digit: `…-1` is not `…-10`'s.
                let beside = found
                    .to_str()
                    .and_then(|f| f.strip_prefix(name))
                    .is_some_and(|rest| !rest.starts_with(|c: char| c.is_ascii_digit()));
                if beside {
                    let path = entry.path();
                    let _ = std::fs::remove_dir_all(&path).or_else(|_| std::fs::remove_file(&path));
                }
            }
        }
    }
}

/// A store made once a run, by `make`, that each test is given a copy of its own of: for a store
/// long to make that tests only start from. The key MCP is called with goes with it.
pub struct Made {
    bytes: Vec<u8>,
    key: Option<String>,
}

/// A copy, of its own, of the store `once` holds (made by `make` the first time).
pub fn copy_of(once: &OnceLock<Made>, name: &str, make: impl FnOnce() -> PathBuf) -> PathBuf {
    let made = once.get_or_init(|| {
        let store = make();
        // Whole in its one file: what the write-ahead log holds is put in it.
        let conn = rusqlite::Connection::open(&store).unwrap();
        conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))
            .unwrap();
        drop(conn);
        Made {
            bytes: std::fs::read(&store).unwrap(),
            key: keys().lock().unwrap().get(&store).cloned(),
        }
    });
    let to = place(&format!("{name}.sqlite"));
    std::fs::write(&to, &made.bytes).unwrap();
    if let Some(key) = &made.key {
        keys().lock().unwrap().insert(to.clone(), key.clone());
    }
    to
}

/// A project file of the fixtures (`tests/fixtures/projects/`), by its name: the sample (`sample`) and
/// the projects the exports are kept for (`large`, `datamodel`, `small`).
pub fn project_file(name: &str) -> String {
    std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/projects")
            .join(format!("{name}.json")),
    )
    .unwrap()
}

/// A store of its own with the fixtures `names` read into it, as the window reads a project file.
pub fn store_with(names: &[&str]) -> PathBuf {
    let store = place(&format!("{}.sqlite", names.join("-")));
    let core = specboard_core::Core::create(&store).unwrap();
    for name in names {
        let reply = op(
            &core,
            serde_json::json!({"op": "project_file_read", "text": project_file(name)}),
        );
        assert!(reply.get("error").is_none(), "{name}: {reply}");
    }
    core.close();
    store
}

/// The sample project (`tests/fixtures/projects/sample.json`), in a store of its own.
pub fn script_store() -> PathBuf {
    store_with(&["sample"])
}

/// The name of the access key the tests call MCP with.
pub const KEY: &str = "テストのキー";

/// A window's request, its answer (`{"result": …}` or `{"error": …}`).
pub fn op(core: &specboard_core::Core, request: serde_json::Value) -> serde_json::Value {
    serde_json::from_str(&core.call(&request.to_string())).unwrap()
}

/// A key named `name`, made through the window's requests and given every project the store
/// has now to write: its id and its value.
pub fn key_for_every_project(core: &specboard_core::Core, name: &str) -> (String, String) {
    use serde_json::json;
    let made = op(core, json!({"op": "access_key_create", "name": name}));
    let id = made["result"]["key"]["id"].as_str().unwrap().to_string();
    let value = made["result"]["value"].as_str().unwrap().to_string();
    let projects: Vec<String> = {
        let conn = rusqlite::Connection::open(core.place()).unwrap();
        let mut stmt = conn.prepare("SELECT id FROM projects").unwrap();
        stmt.query_map([], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    };
    for project in projects {
        let given = op(
            core,
            json!({"op": "access_grant", "key": id, "project": project, "level": "write"}),
        );
        assert!(given.get("error").is_none(), "{given}");
    }
    (id, value)
}

/// The headers the tests call MCP with: MCP takes no request without an access key of the
/// store's own (`specboard://SpecBoard/Docs/Design:DesignAccess`), and the tests go in by the same door — a key
/// (`KEY`) that writes every project the store had when it was first called, made once a store.
/// A project made later through MCP is the key's to write, as any key's.
pub fn headers(core: &specboard_core::Core) -> serde_json::Value {
    let mut keys = keys().lock().unwrap();
    let value = keys
        .entry(core.place().to_path_buf())
        .or_insert_with(|| key_for_every_project(core, KEY).1)
        .clone();
    serde_json::json!({ "authorization": format!("Bearer {value}") })
}

/// The key (`KEY`) each store has been called with, by its place.
fn keys() -> &'static Mutex<HashMap<PathBuf, String>> {
    static KEYS: OnceLock<Mutex<HashMap<PathBuf, String>>> = OnceLock::new();
    KEYS.get_or_init(Default::default)
}

/// The sample's project (`tests/fixtures/projects/sample.json`), by its name.
pub const PROJECT: &str = "見本";

/// The sample's project's id, in the store `core` has open.
pub fn project_id(core: &specboard_core::Core) -> String {
    rusqlite::Connection::open(core.place())
        .unwrap()
        .query_row("SELECT id FROM projects WHERE name = ?", [PROJECT], |r| {
            r.get(0)
        })
        .unwrap()
}

/// A tool called through MCP on the sample's project (`PROJECT`): what it answered, or why it was
/// refused (the `message` of its answer's `error`, else its text).
pub fn mcp(
    core: &specboard_core::Core,
    tool: &str,
    mut arguments: serde_json::Value,
) -> Result<serde_json::Value, String> {
    use serde_json::Value;
    arguments["project"] = PROJECT.into();
    let (refused, text) = call(core, tool, arguments);
    let said: Value = serde_json::from_str(&text).unwrap_or_else(|_| text.clone().into());
    if refused {
        return Err(said["error"]["message"]
            .as_str()
            .map_or(text, str::to_string));
    }
    Ok(said)
}

/// A tool called through MCP on the sample's project that must go through: what it answered.
pub fn ok(
    core: &specboard_core::Core,
    tool: &str,
    arguments: serde_json::Value,
) -> serde_json::Value {
    let shown = arguments.to_string();
    mcp(core, tool, arguments).unwrap_or_else(|e| panic!("{tool} {shown}: {e}"))
}

/// A tool called through MCP on the sample's project that must be refused: why.
pub fn refused(core: &specboard_core::Core, tool: &str, arguments: serde_json::Value) -> String {
    let shown = arguments.to_string();
    match mcp(core, tool, arguments) {
        Ok(v) => panic!("{tool} {shown} が通った: {v}"),
        Err(e) => e,
    }
}

/// The check of a tree (`main`, a branch's name, `_project_board_`, `_directory_`) on the sample's
/// project as MCP reads it, every item whole and by its group: `inspection_list` (`show: all`) read on
/// through its pages, then `inspection_read` of every id. `{project, branch, <group>: [item, …]}`;
/// a group with nothing found is not there.
pub fn checked(core: &specboard_core::Core, tree: &str) -> serde_json::Value {
    use serde_json::{Map, Value, json};
    let mut ids: Vec<Value> = Vec::new();
    let mut after: Option<Value> = None;
    let mut out = Map::new();
    loop {
        let mut value = json!({"show": "all", "limit": 1000});
        if let Some(next) = &after {
            value["after"] = next.clone();
        }
        let page = ok(
            core,
            "inspection_list",
            json!({"branch": tree, "value": value}),
        );
        out.insert("project".into(), page["project"].clone());
        out.insert("branch".into(), page["branch"].clone());
        ids.extend(
            page["items"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|i| i["id"].clone()),
        );
        match page.get("next") {
            Some(next) => after = Some(next.clone()),
            None => break,
        }
    }
    if ids.is_empty() {
        return Value::Object(out);
    }
    let read = ok(
        core,
        "inspection_read",
        json!({"branch": tree, "target": ids}),
    );
    for item in read["items"].as_array().into_iter().flatten() {
        let group = item["group"].as_str().unwrap().to_string();
        out.entry(group)
            .or_insert_with(|| Value::Array(Vec::new()))
            .as_array_mut()
            .unwrap()
            .push(item.clone());
    }
    Value::Object(out)
}

/// A tool called as MCP calls it: whether it was refused, and its text (blocks joined).
pub fn call(
    core: &specboard_core::Core,
    tool: &str,
    arguments: serde_json::Value,
) -> (bool, String) {
    use serde_json::{Value, json};
    let message = json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": tool, "arguments": arguments}});
    let reply: Value = serde_json::from_str(
        &core.call(&json!({"op": "mcp", "message": message, "headers": headers(core)}).to_string()),
    )
    .unwrap();
    let result = &reply["result"]["body"]["result"];
    let texts: Vec<String> = result["content"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|c| c["text"].as_str().map(str::to_string))
        .collect();
    (
        result["isError"].as_bool().unwrap_or(false),
        texts.join("\n"),
    )
}

/// A card's lines as `card_update` writes them (`{add, remove}`): what goes from `before` to
/// `after`, the lines `before` has and `after` does not taken away (by the keys they are
/// told apart by) and the ones `after` has anew added.
pub fn line_edits(before: &[serde_json::Value], after: &[serde_json::Value]) -> serde_json::Value {
    use serde_json::{Map, Value, json};
    let remove: Vec<Value> = before
        .iter()
        .filter(|line| !after.contains(line))
        .map(|line| {
            let mut picked = Map::new();
            for key in ["line", "from", "to", "at", "description", "fields"] {
                if let Some(v) = line.get(key) {
                    picked.insert(key.into(), v.clone());
                }
            }
            Value::Object(picked)
        })
        .collect();
    let add: Vec<Value> = after
        .iter()
        .filter(|line| !before.contains(line))
        .cloned()
        .collect();
    json!({ "add": add, "remove": remove })
}

/// The answer a test keeps to compare with (`tests/fixtures/kept/<name>`): the core's own, taken
/// as it said it when it was kept. With `SPECBOARD_KEEP=1` what the core says now is kept
/// instead — after a change meant to change it, and looked over in the diff.
pub fn kept(name: &str, got: &str) {
    let place = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/kept")
        .join(name);
    if std::env::var("SPECBOARD_KEEP").as_deref() == Ok("1") {
        std::fs::create_dir_all(place.parent().unwrap()).unwrap();
        std::fs::write(&place, got).unwrap();
        return;
    }
    let want = std::fs::read_to_string(&place)
        .unwrap_or_else(|_| panic!("{name} の見本がありません（SPECBOARD_KEEP=1 で取る）"));
    if want != got {
        let (w, g): (Vec<&str>, Vec<&str>) = (want.lines().collect(), got.lines().collect());
        let at = (0..w.len().max(g.len()))
            .find(|&i| w.get(i) != g.get(i))
            .unwrap_or(0);
        panic!(
            "{name} が見本と違う（{} 行目）\n見本: {:?}\n今:   {:?}\n（変えたつもりなら SPECBOARD_KEEP=1 で取り直す）",
            at + 1,
            w.get(at),
            g.get(at)
        );
    }
}

/// The project's own kinds set to `lists` — `{cards, sections, lines}`, each the definitions as
/// kept (named by `kind`・`section`・`line`) in place of the list there — one by one with the kinds'
/// tools: what a list has anew made (`kind_create`) and what it has written whole (`kind_update`, a
/// key no longer there taken away), cards first; then what it leaves out let go (`kind_delete`),
/// lines first. The first refusal, as it came.
pub fn set_kinds(
    core: &specboard_core::Core,
    project: &str,
    lists: serde_json::Value,
) -> Result<(), String> {
    use serde_json::{Map, Value, json};
    let run = |tool: &str, mut arguments: Value| -> Result<Value, String> {
        arguments["project"] = project.into();
        let (refused, text) = call(core, tool, arguments);
        let said: Value = serde_json::from_str(&text).unwrap_or(Value::String(text.clone()));
        if refused {
            Err(said["error"].as_str().map_or(text, str::to_string))
        } else {
            Ok(said)
        }
    };
    const LISTS: [(&str, &str, &str); 3] = [
        ("cards", "kind", "card"),
        ("sections", "section", "section"),
        ("lines", "line", "line"),
    ];
    let now = run("kind_read", json!({}))?;
    let own = |list: &str| -> Vec<Map<String, Value>> {
        now[list]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|d| d["builtin"] != true)
            .filter_map(|d| d.as_object().cloned())
            .collect()
    };
    for (list, key, of) in LISTS {
        let Some(given) = lists.get(list) else {
            continue;
        };
        let before = own(list);
        for definition in given.as_array().into_iter().flatten() {
            let mut value = definition.as_object().cloned().unwrap_or_default();
            let name = value
                .remove(key)
                .and_then(|n| n.as_str().map(str::to_string))
                .unwrap_or_default();
            match before.iter().find(|b| b[key] == name.as_str()) {
                None => {
                    value.insert("of".into(), of.into());
                    run("kind_create", json!({"target": name, "value": value}))?;
                }
                Some(was) => {
                    for k in was.keys() {
                        if k != key
                            && !["builtin", "noPlace", "of", "contextChanged"].contains(&k.as_str())
                            && !value.contains_key(k)
                        {
                            value.insert(k.clone(), Value::Null);
                        }
                    }
                    if !value.is_empty() {
                        run("kind_update", json!({"target": name, "value": value}))?;
                    }
                }
            }
        }
    }
    for (list, key, _) in LISTS.iter().rev() {
        let Some(given) = lists.get(*list) else {
            continue;
        };
        let kept: Vec<&str> = given
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|d| d[*key].as_str())
            .collect();
        let gone: Vec<String> = own(list)
            .iter()
            .filter_map(|d| d[*key].as_str())
            .filter(|n| !kept.contains(n))
            .map(str::to_string)
            .collect();
        if !gone.is_empty() {
            run("kind_delete", json!({"target": gone}))?;
        }
    }
    Ok(())
}

/// The places a `card_list` query found, in order: each match with the card it is in (`card`, `kind`,
/// `rev`).
pub fn matches(found: &serde_json::Value) -> Vec<serde_json::Value> {
    let mut out = Vec::new();
    for card in found["cards"].as_array().into_iter().flatten() {
        for one in card["matches"].as_array().into_iter().flatten() {
            let mut one = one.clone();
            for key in ["path", "kind", "rev"] {
                if let Some(v) = card.get(key) {
                    one[if key == "path" { "card" } else { key }] = v.clone();
                }
            }
            out.push(one);
        }
    }
    out
}

/// What a read answers as text — `markdown` or `mermaid` — out of its answer.
pub fn text_in(said: &serde_json::Value, key: &str) -> String {
    said[key]
        .as_str()
        .unwrap_or_else(|| panic!("{key} がありません: {said}"))
        .to_string()
}
