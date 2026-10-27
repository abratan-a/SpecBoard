//! The daemon answering MCP itself: at its user's loopback, with the access keys of its own
//! database, the HTTP side as the app's listener was — and the port it listens at, its user's to set.
//! It is the only way MCP is answered,
//! so what was tried of the app's listener and of the relay (`HardeningTests`, `MCPProtocolTests`,
//! `relay.rs`) is tried here, of what is HTTP's; the core's part is `core/tests/mcp_protocol.rs`.

use serde_json::{Value, json};
use std::io::{BufRead, BufReader, ErrorKind, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, channel};
use std::time::{Duration, Instant};

/// The machine language the agent runs in: it says its own lines (the CLI, the log) in its machine's,
/// and the tests read them in Japanese. MCP is answered in English all the same.
const LANGUAGE: &str = "ja_JP.UTF-8";

const AGENT: &str = env!("CARGO_BIN_EXE_specboard-agent");

/// The most connections a key came on the daemon holds at once (`agent/src/http.rs`'s `KEYED`: a
/// binary's, not to be named from here).
const KEYED: usize = 32;

/// The longest a line from the daemon is waited for: more than `attach` waits for a daemon it
/// starts (10 s) and MCP's port is tried again for (5 s).
const LINE_WAIT: Duration = Duration::from_secs(30);

/// A home of its own for one test: its daemon stopped and the folder taken away when it goes,
/// whether the test passed or not.
struct Home(PathBuf);

impl std::ops::Deref for Home {
    type Target = Path;
    fn deref(&self) -> &Path {
        &self.0
    }
}

impl Drop for Home {
    fn drop(&mut self) {
        // Nothing here unwraps: it runs while a test that failed unwinds too.
        let _ = Command::new(AGENT)
            .arg("stop")
            .env("SPECBOARD_AGENT_HOME", &self.0)
            .env("LC_ALL", LANGUAGE)
            .env_remove("SPECBOARD_AGENT_BUILD")
            .stdin(Stdio::null())
            .output();
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A home of its own, its MCP at `port` (0: any free).
fn home(port: u16) -> Home {
    static N: AtomicUsize = AtomicUsize::new(0);
    let place = std::env::temp_dir().join(format!(
        "sbm-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&place);
    std::fs::create_dir_all(&place).unwrap();
    std::fs::write(
        place.join("agent.json"),
        json!({"mcpPort": port}).to_string(),
    )
    .unwrap();
    Home(place)
}

/// What the core says, as MCP answers it: in English.
fn english(say: impl Fn() -> String) -> String {
    let _lang = specboard_core::text::bind(Some("en"));
    say()
}

/// Waited for until `done` says so, for 5 seconds at most.
fn until(what: &str, mut done: impl FnMut() -> bool) {
    let end = Instant::now() + Duration::from_secs(5);
    while !done() {
        assert!(Instant::now() < end, "5 秒待っても成らない: {what}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn run(home: &Path, args: &[&str]) -> (i32, String, String) {
    let out = Command::new(AGENT)
        .args(args)
        .env("SPECBOARD_AGENT_HOME", home)
        .env("LC_ALL", LANGUAGE)
        .env_remove("SPECBOARD_AGENT_BUILD")
        .output()
        .unwrap();
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// An app on `attach`: its lines read as they come (`lines`), so that one is waited for no longer
/// than `LINE_WAIT`.
struct App {
    child: Child,
    input: ChildStdin,
    lines: Receiver<String>,
    next: u64,
}

impl App {
    fn attach(home: &Path) -> (App, Value) {
        Self::attach_in(home, None)
    }

    /// An app whose screen is in `lang` (none: one that says none, as an app of before).
    fn attach_in(home: &Path, lang: Option<&str>) -> (App, Value) {
        let mut child = Command::new(AGENT)
            .arg("attach")
            .env("SPECBOARD_AGENT_HOME", home)
            .env("LC_ALL", LANGUAGE)
            .env_remove("SPECBOARD_AGENT_BUILD")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let input = child.stdin.take().unwrap();
        let output = child.stdout.take().unwrap();
        let (sender, lines) = channel();
        std::thread::spawn(move || {
            for line in BufReader::new(output).lines() {
                let Ok(line) = line else { break };
                if sender.send(line).is_err() {
                    break;
                }
            }
        });
        let mut app = App {
            child,
            input,
            lines,
            next: 1,
        };
        let mut hello = json!({"t": "hello", "client": "MacBook"});
        if let Some(lang) = lang {
            hello["lang"] = lang.into();
        }
        app.send(&hello);
        let ready = app.line();
        (app, ready)
    }

    fn send(&mut self, message: &Value) {
        writeln!(self.input, "{message}").unwrap();
        self.input.flush().unwrap();
    }

    /// The next line: none in `LINE_WAIT`, the daemon gone (what `attach` says to closed) or a
    /// line not JSON fails the test.
    fn line(&mut self) -> Value {
        let text = match self.lines.recv_timeout(LINE_WAIT) {
            Ok(text) => text,
            Err(RecvTimeoutError::Timeout) => panic!("{LINE_WAIT:?} 待っても 1 行も来ない"),
            Err(RecvTimeoutError::Disconnected) => {
                panic!("attach の出力が閉じた（daemon が落ちた）")
            }
        };
        serde_json::from_str(&text).unwrap_or_else(|e| panic!("JSON でない行: {text:?}（{e}）"))
    }

    /// The next line of the kind `t`, what came before it let go.
    fn next_of(&mut self, t: &str) -> Value {
        loop {
            let line = self.line();
            if line["t"] == t {
                return line;
            }
        }
    }

    /// The next line that is not what changed.
    fn said(&mut self) -> Value {
        loop {
            let line = self.line();
            if line["t"] != "changed" {
                return line;
            }
        }
    }

    fn core(&mut self, request: Value) -> Value {
        let id = self.next;
        self.next += 1;
        self.send(&json!({"t": "core", "id": id, "request": request}));
        loop {
            let line = self.line();
            if line["t"] == "core" && line["id"] == id {
                return serde_json::from_str(line["answer"].as_str().unwrap()).unwrap();
            }
            assert!(line["t"] == "changed", "{line}");
        }
    }
}

impl Drop for App {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// One request, as `curl` would send it; the status, the head and the body.
fn http(
    port: u16,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: &str,
) -> (u16, String, String) {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut head = format!(
        "{method} {path} HTTP/1.1\r\nhost: 127.0.0.1:{port}\r\ncontent-length: {}\r\n",
        body.len()
    );
    for (name, value) in headers {
        head.push_str(&format!("{name}: {value}\r\n"));
    }
    head.push_str("\r\n");
    stream.write_all(head.as_bytes()).unwrap();
    stream.write_all(body.as_bytes()).unwrap();
    read_answer(&mut stream)
}

/// What came back on a connection: the status, the head and the body (by its length, or what a
/// stream sent at first).
fn read_answer(stream: &mut TcpStream) -> (u16, String, String) {
    let mut got = Vec::new();
    let mut chunk = [0u8; 8192];
    // Up to the end of the head, then the body by its length (or what a stream sent at first).
    loop {
        let n = stream.read(&mut chunk).unwrap_or(0);
        if n == 0 {
            break;
        }
        got.extend_from_slice(&chunk[..n]);
        let text = String::from_utf8_lossy(&got).into_owned();
        if let Some(end) = text.find("\r\n\r\n") {
            let head = text[..end].to_lowercase();
            let length = head
                .lines()
                .find_map(|l| l.strip_prefix("content-length: "))
                .and_then(|l| l.trim().parse::<usize>().ok());
            match length {
                Some(length) if got.len() >= end + 4 + length => break,
                None if head.contains("text/event-stream") && text.matches("\n\n").count() >= 2 => {
                    break;
                }
                _ => {}
            }
        }
    }
    let text = String::from_utf8_lossy(&got).into_owned();
    let (head, body) = text.split_once("\r\n\r\n").unwrap_or((&text, ""));
    let status = head
        .split(' ')
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    (status, head.to_lowercase(), body.to_string())
}

fn post(port: u16, key: Option<&str>, body: &str, more: &[(&str, &str)]) -> (u16, String, String) {
    let bearer = key.map(|k| format!("Bearer {k}"));
    let mut headers = vec![("content-type", "application/json")];
    if let Some(bearer) = &bearer {
        headers.push(("authorization", bearer.as_str()));
    }
    headers.extend_from_slice(more);
    http(port, "POST", "/mcp", &headers, body)
}

#[test]
fn the_daemon_answers_mcp_with_its_own_keys() {
    let home = home(0);
    let (mut app, ready) = App::attach(&home);
    assert_eq!(ready["mcp"]["listening"], true, "{ready}");
    let port = ready["mcp"]["port"].as_u64().unwrap() as u16;
    assert_ne!(port, 0);
    let (code, said, _) = run(&home, &["status"]);
    assert_eq!(code, 0);
    assert!(
        said.contains(&format!("127.0.0.1:{port} で待ち受けています")),
        "{said}"
    );

    // A project, and a key the daemon's own database holds, given it to write — through the app's way.
    let made = app
        .core(json!({"op": "tool", "name": "project_create", "arguments": {"project": "本体の"}}));
    assert!(made.get("error").is_none(), "{made}");
    let project = app.core(json!({"op": "projects"}))["result"]["projects"][0]["id"].clone();
    let key = app.core(json!({"op": "access_key_create", "name": "Shop の AI"}));
    let value = key["result"]["value"].as_str().unwrap().to_string();
    let id = key["result"]["key"]["id"].clone();
    let granted =
        app.core(json!({"op": "access_grant", "key": id, "project": project, "level": "write"}));
    assert!(granted.get("error").is_none(), "{granted}");

    // Answered by the core here: the envelope, the tools.
    let (status, _, body) = post(
        port,
        Some(&value),
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"0"}}}"#,
        &[],
    );
    assert_eq!(status, 200, "{body}");
    assert!(body.contains(r#""name":"specboard""#), "{body}");
    let (status, _, body) = post(
        port,
        Some(&value),
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"project_list","arguments":{}}}"#,
        &[],
    );
    assert_eq!(status, 200);
    assert!(body.contains("本体の"), "{body}");

    // No key, another store's key: turned away in the core's words. The way in is the HTTP side's.
    let ping = r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#;
    let (status, _, said) = post(port, None, ping, &[]);
    assert_eq!(status, 401);
    assert_eq!(said, english(specboard_core::access::key_needed));
    let (status, _, said) = post(port, Some("spk_wrong"), ping, &[]);
    assert_eq!(status, 401);
    assert_eq!(
        said,
        english(|| specboard_core::text!("access.keyUnusable"))
    );
    assert_eq!(post(port, Some(&value), ping, &[]).0, 200);
    assert_eq!(
        post(
            port,
            Some(&value),
            ping,
            &[("origin", "https://example.com")]
        )
        .0,
        403
    );
    let rebinding = http(
        port,
        "POST",
        "/mcp",
        &[
            ("content-type", "application/json"),
            ("authorization", &format!("Bearer {value}")),
            ("host", "evil.example"),
        ],
        ping,
    );
    assert_eq!(rebinding.0, 403, "{rebinding:?}");
    assert_eq!(
        http(
            port,
            "POST",
            "/mcp",
            &[("authorization", &format!("Bearer {value}"))],
            ping
        )
        .0,
        415
    );
    assert_eq!(
        http(
            port,
            "GET",
            "/mcp",
            &[("authorization", &format!("Bearer {value}"))],
            ""
        )
        .0,
        405
    );
    let (status, _, body) = http(port, "GET", "/healthz", &[], "");
    assert_eq!((status, body.as_str()), (200, r#"{"ok":true}"#));

    // The 2026 era's listen: acknowledged, the tool list said to have changed, kept open.
    let meta = r#""_meta":{"io.modelcontextprotocol/protocolVersion":"2026-07-28","io.modelcontextprotocol/clientInfo":{"name":"test","version":"0"},"io.modelcontextprotocol/clientCapabilities":{}}"#;
    let (status, head, events) = post(
        port,
        Some(&value),
        &format!(
            r#"{{"jsonrpc":"2.0","id":"listen:1","method":"subscriptions/listen","params":{{"notifications":{{"toolsListChanged":true}},{meta}}}}}"#
        ),
        &[
            ("mcp-protocol-version", "2026-07-28"),
            ("mcp-method", "subscriptions/listen"),
        ],
    );
    assert_eq!(status, 200, "{head}\n{events}");
    assert!(head.contains("content-type: text/event-stream"), "{head}");
    assert!(
        events.contains("notifications/subscriptions/acknowledged"),
        "{events}"
    );
    assert!(
        events.contains("notifications/tools/list_changed"),
        "{events}"
    );

    // Another port, set from the shell: listened at from now on, and kept.
    let free = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let next = free.local_addr().unwrap().port();
    drop(free);
    let (code, said, _) = run(&home, &["port", &next.to_string()]);
    assert_eq!(code, 0);
    assert!(said.contains(&format!("127.0.0.1:{next}")), "{said}");
    let told = app.said();
    assert_eq!(told["t"], "mcp", "{told}");
    assert_eq!(told["mcp"]["port"], next);
    assert_eq!(http(next, "GET", "/healthz", &[], "").0, 200);
    until("前のポートは閉じる", || {
        TcpStream::connect(("127.0.0.1", port)).is_err()
    });
    let kept: Value =
        serde_json::from_str(&std::fs::read_to_string(home.join("agent.json")).unwrap()).unwrap();
    assert_eq!(kept["mcpPort"], next);

    drop(app);
    assert_eq!(run(&home, &["stop"]).0, 0);
}

#[test]
fn a_port_taken_is_said_and_the_apps_are_served_all_the_same() {
    let taken = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = taken.local_addr().unwrap().port();
    let home = home(port);
    let (mut app, ready) = App::attach(&home);
    assert_eq!(ready["mcp"]["listening"], false, "{ready}");
    assert_eq!(ready["mcp"]["port"], port);
    assert!(
        ready["mcp"]["problem"]
            .as_str()
            .unwrap()
            .contains(&port.to_string()),
        "{ready}"
    );
    assert!(app.core(json!({"op": "projects"})).get("error").is_none());
    let (_, said, _) = run(&home, &["status"]);
    assert!(said.contains("待ち受けていません"), "{said}");
    // An app whose screen is in English is told why in English, the machine being in Japanese.
    let (_english, told) = App::attach_in(&home, Some("en"));
    let problem = told["mcp"]["problem"].as_str().unwrap();
    assert!(problem.contains(&port.to_string()), "{told}");
    assert!(problem.is_ascii(), "{told}");
    assert!(
        !ready["mcp"]["problem"].as_str().unwrap().is_ascii(),
        "{ready}"
    );
    // A port taken asked for: refused, nothing changes.
    app.send(&json!({"t": "mcp_port", "port": port}));
    let refused = app.said();
    assert!(refused["refused"].is_string(), "{refused}");
    // The port let go of: taken up on its own, and said.
    drop(taken);
    let told = app.said();
    assert_eq!(told["t"], "mcp", "{told}");
    assert_eq!(told["mcp"]["listening"], true, "{told}");
    assert_eq!(told["mcp"]["port"], port);
    assert_eq!(http(port, "GET", "/healthz", &[], "").0, 200);
    drop(app);
    assert_eq!(run(&home, &["stop"]).0, 0);
}

/// MCP is answered in English whatever the machine's language (the agent runs in Japanese here):
/// the tools, the instructions and what an answer says. A setting kept by a version that let the
/// language be chosen is read without a word and changes nothing; the apps are told only the port.
#[test]
fn mcp_answers_in_english_whatever_the_machines_language_and_an_old_setting() {
    let home = home(0);
    std::fs::write(
        home.join("agent.json"),
        json!({"mcpPort": 0, "mcpLanguage": "ja", "appSystemLanguage": "ja"}).to_string(),
    )
    .unwrap();
    let (mut app, ready) = App::attach(&home);
    assert_eq!(ready["mcp"]["listening"], true, "{ready}");
    assert!(ready["mcp"].get("language").is_none(), "{ready}");
    assert!(ready["mcp"].get("answersIn").is_none(), "{ready}");
    let port = ready["mcp"]["port"].as_u64().unwrap() as u16;

    // The sample project, and a key given it to read.
    let sample = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/fixtures/projects/sample.json"),
    )
    .unwrap();
    let read = app.core(json!({"op": "project_file_read", "text": sample}));
    assert!(read.get("error").is_none(), "{read}");
    let project = app.core(json!({"op": "projects"}))["result"]["projects"][0]["id"].clone();
    let key = app.core(json!({"op": "access_key_create", "name": "言語を試す AI"}));
    let value = key["result"]["value"].as_str().unwrap().to_string();
    let granted = app.core(json!({
        "op": "access_grant", "key": key["result"]["key"]["id"], "project": project, "level": "read"
    }));
    assert!(granted.get("error").is_none(), "{granted}");
    let (status, _, body) = post(
        port,
        Some(&value),
        r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"inspection_list","arguments":{"project":"見本","value":{"show":"all"}}}}"#,
        &[],
    );
    assert_eq!(status, 200, "{body}");
    let reply: Value = serde_json::from_str(&body).unwrap_or_else(|_| panic!("{body}"));
    let text = reply["result"]["content"][0]["text"]
        .as_str()
        .unwrap_or_else(|| panic!("{body}"));
    let listed: Value = serde_json::from_str(text).unwrap();
    assert_eq!(
        listed["groups"]["freeCards"]["label"],
        "Cards in free sections"
    );
    let (status, _, body) = post(
        port,
        Some(&value),
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}"#,
        &[],
    );
    assert_eq!(status, 200, "{body}");
    let reply: Value = serde_json::from_str(&body).unwrap_or_else(|_| panic!("{body}"));
    assert_eq!(reply["result"]["tools"][0]["title"], "Project list");

    // The line a version that let the language be chosen sent is not one there is.
    app.send(&json!({"t": "mcp_language", "value": "ja"}));
    assert_eq!(app.said()["t"], "error");
    let kept: Value =
        serde_json::from_str(&std::fs::read_to_string(home.join("agent.json")).unwrap()).unwrap();
    assert_eq!(kept["mcpPort"], 0, "{kept}");
    drop(app);
    assert_eq!(run(&home, &["stop"]).0, 0);
}

/// `app_control` through the daemon: the windows are the apps' it holds, each named by its machine;
/// a key sees those of the projects it may read, and a call goes to the app whose window it is.
#[test]
fn app_control_goes_to_the_app_whose_window_it_is() {
    let home = home(0);
    let (mut macbook, ready) = App::attach(&home);
    let port = ready["mcp"]["port"].as_u64().unwrap() as u16;
    for name in ["見える", "見えない"] {
        let made = macbook
            .core(json!({"op": "tool", "name": "project_create", "arguments": {"project": name}}));
        assert!(made.get("error").is_none(), "{made}");
    }
    let projects = macbook.core(json!({"op": "projects"}))["result"]["projects"].clone();
    let id = |name: &str| {
        projects
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["name"] == name)
            .unwrap()["id"]
            .clone()
    };
    let (seen, unseen) = (id("見える"), id("見えない"));
    let key = macbook.core(
        json!({"op": "access_key_create", "name": "AI", "permits": {"app_control": "allow"}}),
    );
    let value = key["result"]["value"].as_str().unwrap().to_string();
    macbook.core(json!({"op": "access_grant", "key": key["result"]["key"]["id"], "project": seen, "level": "read"}));

    // Two Macs: each says its windows. The other Mac's comes as an app of its own.
    // (A request of the core's after it on the same connection: the daemon takes an app's lines
    // one by one, so its windows are taken when it is answered.)
    macbook.send(&json!({"t": "windows", "windows": [
        {"window": 1, "project": seen, "branch": "main", "section": "Schemas", "card": null, "front": false}]}));
    macbook.core(json!({"op": "projects"}));
    let imac = {
        let (mut app, _) = App::attach(&home);
        app.send(&json!({"t": "hello", "client": "iMac"}));
        let _ = app.line();
        app.send(&json!({"t": "windows", "windows": [
            {"window": 1, "project": unseen, "branch": "main", "section": null, "card": null, "front": true}]}));
        app.core(json!({"op": "projects"}));
        app
    };

    let call = move |arguments: Value| -> Value {
        let body = json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call",
                          "params": {"name": "app_control", "arguments": arguments}});
        let (status, _, said) = post(port, Some(&value), &body.to_string(), &[]);
        assert_eq!(status, 200, "{said}");
        let reply: Value = serde_json::from_str(&said).unwrap();
        let text = reply["result"]["content"][0]["text"]
            .as_str()
            .unwrap_or("")
            .to_string();
        json!({"text": text, "isError": reply["result"]["isError"]})
    };

    // What the key sees: the window of the project it may read, by its machine.
    let listed = call(json!({"op": "windows"}));
    assert!(
        listed["text"].as_str().unwrap().contains("MacBook:1"),
        "{listed}"
    );
    assert!(
        !listed["text"].as_str().unwrap().contains("iMac"),
        "{listed}"
    );

    // A call on the project: handed to the MacBook, which answers it.
    let asking = std::thread::spawn({
        let call = call.clone();
        move || call(json!({"op": "state", "project": "見える"}))
    });
    let handed = macbook.next_of("app");
    assert_eq!(handed["window"], 1, "{handed}");
    let arguments: Value = serde_json::from_str(handed["arguments"].as_str().unwrap()).unwrap();
    assert_eq!(arguments["op"], "state");
    macbook.send(&json!({"t": "app", "id": handed["id"], "answer": r#"{"json":{"project":"見える","from":"MacBook"}}"#}));
    let answered = asking.join().unwrap();
    assert!(
        answered["text"]
            .as_str()
            .unwrap()
            .contains(r#""from":"MacBook""#),
        "{answered}"
    );

    // A project the key may not read: as if not there. A window not there: said, with those there are.
    let refused = call(json!({"op": "state", "project": "見えない"}));
    assert_eq!(refused["isError"], true, "{refused}");
    assert!(
        refused["text"]
            .as_str()
            .unwrap()
            .contains(r#"There is no project \"見えない\""#),
        "{refused}"
    );
    let refused = call(json!({"op": "view", "value": {"window": "iMac:1"}}));
    assert!(
        refused["text"]
            .as_str()
            .unwrap()
            .contains("There are: MacBook:1"),
        "{refused}"
    );

    // The window closed: no window to choose in.
    macbook.send(&json!({"t": "windows", "windows": []}));
    macbook.core(json!({"op": "projects"}));
    let refused = call(json!({"op": "select", "project": "見える"}));
    assert!(
        refused["text"]
            .as_str()
            .unwrap()
            .contains(r#"No window has the project \"見える\" open"#),
        "{refused}"
    );

    drop(imac);
    drop(macbook);
    assert_eq!(run(&home, &["stop"]).0, 0);
}

/// A picture through the daemon: handed to one app it holds — the one with the window in front, else
/// the one attached last — whatever its windows show, and with no app there, refused with the same call
/// as `as: diagram`.
#[test]
fn a_picture_goes_to_the_nearest_app_and_with_none_is_refused() {
    let home = home(0);
    let (mut macbook, ready) = App::attach(&home);
    let port = ready["mcp"]["port"].as_u64().unwrap() as u16;
    let made = macbook
        .core(json!({"op": "tool", "name": "project_create", "arguments": {"project": "絵"}}));
    assert!(made.get("error").is_none(), "{made}");
    let project = macbook.core(json!({"op": "projects"}))["result"]["projects"][0]["id"].clone();
    // A key that reads the project and may not touch the window.
    let key = macbook.core(json!({"op": "access_key_create", "name": "AI", "permits": {"app_control": "deny", "app_control.capture": "deny"}}));
    let value = key["result"]["value"].as_str().unwrap().to_string();
    macbook.core(json!({"op": "access_grant", "key": key["result"]["key"]["id"], "project": project, "level": "read"}));
    // The iMac's window is in front, on no project of this agent's.
    let mut imac = {
        let (mut app, _) = App::attach(&home);
        app.send(&json!({"t": "hello", "client": "iMac"}));
        let _ = app.line();
        app.send(&json!({"t": "windows", "windows": [
            {"window": 1, "project": "elsewhere", "branch": "main", "section": null, "card": null, "front": true}]}));
        app.core(json!({"op": "projects"}));
        app
    };
    let call = move || -> Value {
        let body = json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call",
            "params": {"name": "section_read", "arguments": {"project": "絵", "target": "Main", "value": {"as": "picture", "maxSize": 800}}}});
        let (status, _, said) = post(port, Some(&value), &body.to_string(), &[]);
        assert_eq!(status, 200, "{said}");
        let reply: Value = serde_json::from_str(&said).unwrap();
        reply["result"].clone()
    };

    let asking = std::thread::spawn({
        let call = call.clone();
        move || call()
    });
    let handed = imac.next_of("picture");
    let request: Value = serde_json::from_str(handed["request"].as_str().unwrap()).unwrap();
    assert_eq!(request["picture"], "sectionMap", "{request}");
    assert_eq!(request["project"], project);
    assert_eq!(request["maxSize"], 800);
    imac.send(&json!({"t": "picture", "id": handed["id"], "answer": json!({
        "image": {"data": "iVBORw0KGgo=", "mimeType": "image/png"},
        "note": {"picture": "sectionMap", "width": 800, "height": 600, "scale": 1, "cards": 0}}).to_string()}));
    let answered = asking.join().unwrap();
    assert_eq!(answered["isError"], false, "{answered}");
    assert_eq!(answered["content"][0]["type"], "image", "{answered}");
    let note: Value =
        serde_json::from_str(answered["content"][1]["text"].as_str().unwrap()).unwrap();
    assert_eq!(
        (
            note["project"].clone(),
            note["branch"].clone(),
            note["path"].clone()
        ),
        (json!("絵"), json!("main"), json!("Main"))
    );

    // The iMac gone: the MacBook draws it, though no window of its shows a project.
    drop(imac);
    let asking = std::thread::spawn({
        let call = call.clone();
        move || call()
    });
    let handed = macbook.next_of("picture");
    macbook.send(
        &json!({"t": "picture", "id": handed["id"], "answer": r#"{"error":"描けませんでした"}"#}),
    );
    let refused = asking.join().unwrap();
    assert_eq!(refused["isError"], true, "{refused}");
    let error: Value =
        serde_json::from_str(refused["content"][0]["text"].as_str().unwrap()).unwrap();
    assert!(
        error["error"]["message"]
            .as_str()
            .unwrap()
            .starts_with("No picture: 描けませんでした."),
        "{error}"
    );
    assert_eq!(
        error["error"]["use"],
        json!({"tool": "section_read", "arguments": {"project": "絵", "target": "Main", "value": {"as": "diagram"}}})
    );

    // No app there: said so, with the same call as a diagram.
    drop(macbook);
    let mut refused = Value::Null;
    until("アプリが外れたのを daemon が見る", || {
        refused = call();
        refused["content"][0]["text"]
            .as_str()
            .is_some_and(|t| t.contains("no SpecBoard app is connected to draw it"))
    });
    let error: Value =
        serde_json::from_str(refused["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(
        error["error"]["use"]["arguments"]["value"],
        json!({"as": "diagram"})
    );
    assert_eq!(run(&home, &["stop"]).0, 0);
}

/// A person asked through the daemon: on
/// every app it holds, the first answer taken, and the question taken away from each after; with no app
/// there, said so.
#[test]
fn a_person_is_asked_on_every_app_and_the_first_answer_is_taken() {
    let home = home(0);
    let (mut macbook, ready) = App::attach(&home);
    let port = ready["mcp"]["port"].as_u64().unwrap() as u16;
    let (mut imac, _) = App::attach(&home);
    imac.send(&json!({"t": "hello", "client": "iMac"}));
    let _ = imac.line();
    // A key whose window is asked about each time, as every key's is made.
    let key = macbook.core(json!({"op": "access_key_create", "name": "AI"}));
    let value = key["result"]["value"].as_str().unwrap().to_string();
    let call = move |arguments: Value| -> Value {
        let body = json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call",
                          "params": {"name": "app_control", "arguments": arguments}});
        let (status, _, said) = post(port, Some(&value), &body.to_string(), &[]);
        assert_eq!(status, 200, "{said}");
        let reply: Value = serde_json::from_str(&said).unwrap();
        json!({"text": reply["result"]["content"][0]["text"], "isError": reply["result"]["isError"]})
    };

    let asking = std::thread::spawn({
        let call = call.clone();
        move || call(json!({"op": "windows", "request": true}))
    });
    let on_macbook = macbook.said();
    let on_imac = imac.said();
    assert_eq!(on_macbook["t"], "ask", "{on_macbook}");
    assert_eq!(on_imac["id"], on_macbook["id"]);
    let request: Value = serde_json::from_str(on_macbook["request"].as_str().unwrap()).unwrap();
    assert_eq!(request["tool"], "app_control");
    assert_eq!(request["key"], "AI");
    assert_eq!(request["items"][0]["id"], "app_control");
    assert_eq!(request["items"][0]["whose"], "key");
    // The iMac answers; the question taken away from both.
    imac.send(
        &json!({"t": "ask", "id": on_imac["id"], "answer": r#"{"answer":"once","client":"iMac"}"#}),
    );
    let answered = asking.join().unwrap();
    assert_eq!(answered["isError"], false, "{answered}");
    assert_eq!(
        macbook.said(),
        json!({"t": "asked", "id": on_macbook["id"]})
    );
    assert_eq!(imac.said(), json!({"t": "asked", "id": on_imac["id"]}));

    // No app there: said so, at once.
    drop(imac);
    drop(macbook);
    // Asked of an app that went before the daemon saw it go, it is not answered (a write to it
    // fails at once): asked again until the daemon has.
    let mut refused = Value::Null;
    until("アプリが外れたのを daemon が見る", || {
        refused = call(json!({"op": "windows", "request": true}));
        refused["text"]
            .as_str()
            .is_some_and(|t| t.contains("no app is connected to this agent"))
    });
    assert_eq!(refused["isError"], true, "{refused}");
    assert_eq!(run(&home, &["stop"]).0, 0);
}

/// Bytes as they are sent, and all that came back before the daemon closed (or 5 seconds went by).
fn raw(port: u16, sent: &[u8]) -> String {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    // Turned away early, the rest of what is sent may meet a closed socket.
    let _ = stream.write_all(sent);
    let mut got = Vec::new();
    let _ = stream.read_to_end(&mut got);
    String::from_utf8_lossy(&got).into_owned()
}

/// The events a listen stream sends at first, up to `want` of them, and any more that came within
/// a moment of the last.
fn listen(port: u16, key: &str, notifications: Value, want: usize) -> (String, Vec<Value>) {
    let body = json!({"jsonrpc": "2.0", "id": "listen:1", "method": "subscriptions/listen",
        "params": {"notifications": notifications, "_meta": {
            "io.modelcontextprotocol/protocolVersion": "2026-07-28",
            "io.modelcontextprotocol/clientInfo": {"name": "test", "version": "0"},
            "io.modelcontextprotocol/clientCapabilities": {}}}})
    .to_string();
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    write!(
        stream,
        "POST /mcp HTTP/1.1\r\nhost: 127.0.0.1:{port}\r\ncontent-type: application/json\r\n\
         authorization: Bearer {key}\r\nmcp-protocol-version: 2026-07-28\r\n\
         mcp-method: subscriptions/listen\r\ncontent-length: {}\r\n\r\n{body}",
        body.len()
    )
    .unwrap();
    let mut got = String::new();
    let mut chunk = [0u8; 8192];
    let events = |text: &str| text.matches("\ndata: ").count();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    while events(&got) < want {
        match stream.read(&mut chunk) {
            Ok(n) if n > 0 => got.push_str(&String::from_utf8_lossy(&chunk[..n])),
            _ => break,
        }
    }
    stream
        .set_read_timeout(Some(Duration::from_millis(300)))
        .unwrap();
    while let Ok(n) = stream.read(&mut chunk) {
        if n == 0 {
            break;
        }
        got.push_str(&String::from_utf8_lossy(&chunk[..n]));
    }
    let (head, rest) = got.split_once("\r\n\r\n").unwrap_or((&got, ""));
    let events = rest
        .lines()
        .filter_map(|line| line.strip_prefix("data: "))
        .map(|data| serde_json::from_str(data).unwrap())
        .collect();
    (head.to_lowercase(), events)
}

#[test]
fn what_is_not_a_request_it_serves_is_turned_away_before_the_body() {
    let home = home(0);
    let (mut app, ready) = App::attach(&home);
    let port = ready["mcp"]["port"].as_u64().unwrap() as u16;
    let key = app.core(json!({"op": "access_key_create", "name": "AI"}))["result"]["value"]
        .as_str()
        .unwrap()
        .to_string();
    let revoked = app.core(json!({"op": "access_key_create", "name": "前の AI"}));
    let revoked_value = revoked["result"]["value"].as_str().unwrap().to_string();
    let gone = app.core(json!({"op": "access_key_revoke", "key": revoked["result"]["key"]["id"]}));
    assert!(gone.get("error").is_none(), "{gone}");
    let bearer = format!("Authorization: Bearer {key}\r\n");
    let cases: Vec<(String, &str)> = vec![
        (
            format!("GET / HTTP/1.1\r\nX: {}\r\n\r\n", "a".repeat(70 << 10)),
            "431",
        ),
        (
            format!("POST /mcp HTTP/1.1\r\n{bearer}Content-Length: 9999999999\r\n\r\n"),
            "413",
        ),
        (
            format!("POST /mcp HTTP/1.1\r\n{bearer}Content-Length: nine\r\n\r\n"),
            "400",
        ),
        (
            format!("POST /mcp HTTP/1.1\r\n{bearer}Content-Length: -1\r\n\r\n{{}}"),
            "400",
        ),
        // No key: turned away before the 8 MB it says are coming, which never come.
        (
            "POST /mcp HTTP/1.1\r\nContent-Length: 8388608\r\n\r\n".into(),
            "401",
        ),
        // A key the store does not have, or no longer has: turned away as soon as its head is
        // read, before the 8 MB it says are coming are waited for, read or parsed.
        (
            "POST /mcp HTTP/1.1\r\nAuthorization: Bearer spk_0123456789abcdef0123456789abcdef\r\nContent-Length: 8388608\r\n\r\n".into(),
            "401",
        ),
        (
            format!("POST /mcp HTTP/1.1\r\nAuthorization: Bearer {revoked_value}\r\nContent-Length: 8388608\r\n\r\n"),
            "401",
        ),
        (
            "POST /mcp HTTP/1.1\r\nAuthorization: Bearer   \r\nContent-Length: 2\r\n\r\n{}".into(),
            "401",
        ),
        (
            format!("POST /mcp HTTP/1.1\r\nAuthorization: {key}\r\nContent-Length: 2\r\n\r\n{{}}"),
            "401",
        ),
        // A web page, key or not.
        (
            format!(
                "POST /mcp HTTP/1.1\r\n{bearer}Origin: https://example.com\r\nContent-Length: 2\r\n\r\n{{}}"
            ),
            "403",
        ),
    ];
    for (sent, status) in cases {
        let all = raw(port, sent.as_bytes());
        assert!(
            all.starts_with(&format!("HTTP/1.1 {status} ")),
            "{status}: {}",
            &all[..all.len().min(200)]
        );
        if status == "401" {
            // A key sent and not the store's is told so; none sent, that one is needed.
            let keyed = sent.contains("Bearer spk_");
            let said = if keyed {
                english(|| specboard_core::text!("access.keyUnusable"))
            } else {
                english(specboard_core::access::key_needed)
            };
            assert!(all.ends_with(&said), "{all}");
        }
    }

    // Whatever the method and whichever path under /mcp: a wrong key is told it cannot be used,
    // and a right one is answered as before.
    let wrong = "Bearer spk_0123456789abcdef0123456789abcdef";
    let auth = format!("Bearer {key}");
    for (method, path, right) in [
        ("DELETE", "/mcp", 200),
        ("GET", "/mcp", 405),
        ("PUT", "/mcp", 405),
        ("POST", "/mcp/read", 200),
    ] {
        for (authorization, status) in [(wrong, 401), (auth.as_str(), right)] {
            let (got, _, said) = http(
                port,
                method,
                path,
                &[
                    ("content-type", "application/json"),
                    ("authorization", authorization),
                ],
                r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#,
            );
            assert_eq!(got, status, "{method} {path}: {said}");
            if status == 401 {
                assert_eq!(
                    said,
                    english(|| specboard_core::text!("access.keyUnusable"))
                );
            }
        }
    }

    let ping = r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#;
    let with = |headers: &[(&str, &str)]| {
        let mut all = vec![("content-type", "application/json")];
        all.extend_from_slice(headers);
        http(port, "POST", "/mcp", &all, ping).0
    };
    // The scheme's name is any case.
    assert_eq!(with(&[("authorization", &format!("bearer {key}"))]), 200);
    // The names of the loopback, at any port: an SSH tunnel's far end is one.
    for host in [
        format!("localhost:{port}"),
        "LOCALHOST".into(),
        "[::1]:9000".into(),
        "127.0.0.1:41234".into(),
    ] {
        assert_eq!(
            with(&[("authorization", &auth), ("host", &host)]),
            200,
            "{host}"
        );
    }
    // Any other name is a page that had its own name turned to the loopback (DNS rebinding).
    for host in [
        "attacker.example:8787",
        "attacker.example",
        "127.0.0.1.attacker.example:8787",
    ] {
        assert_eq!(
            with(&[("authorization", &auth), ("host", host)]),
            403,
            "{host}"
        );
    }
    // A POST a page can send without asking first.
    assert_eq!(
        http(
            port,
            "POST",
            "/mcp",
            &[("content-type", "text/plain"), ("authorization", &auth)],
            ping
        )
        .0,
        415
    );

    // A request that has not all come yet is waited for.
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    write!(
        stream,
        "POST /mcp HTTP/1.1\r\nhost: 127.0.0.1\r\ncontent-type: application/json\r\n\
         authorization: {auth}\r\ncontent-length: {}\r\n\r\n{}",
        ping.len(),
        &ping[..10]
    )
    .unwrap();
    // Neither answered nor closed meanwhile.
    stream
        .set_read_timeout(Some(Duration::from_millis(300)))
        .unwrap();
    let early = stream.read(&mut [0u8; 1]);
    assert!(
        matches!(&early, Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut)),
        "途中で答えたか閉じた: {early:?}"
    );
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    stream.write_all(&ping.as_bytes()[10..]).unwrap();
    let (status, _, body) = read_answer(&mut stream);
    assert_eq!(status, 200, "{body}");
    assert!(body.contains(r#""result":{}"#), "{body}");
    drop(stream);

    // Connections that show no key never take the places of those that do: however many are held
    // open without a word, a request with a key is answered at once.
    let strangers: Vec<TcpStream> = (0..127)
        .map(|_| TcpStream::connect(("127.0.0.1", port)).unwrap())
        .collect();
    let begun = Instant::now();
    let (status, _, body) = http(
        port,
        "POST",
        "/mcp",
        &[
            ("content-type", "application/json"),
            ("authorization", &auth),
        ],
        ping,
    );
    assert_eq!(status, 200, "{body}");
    assert!(
        begun.elapsed() < Duration::from_secs(2),
        "{:?}",
        begun.elapsed()
    );
    drop(strangers);

    // As many connections a key came on as are held, each waiting for its next request: one more
    // takes the place of the one that waited longest.
    let slot = || -> Option<TcpStream> {
        let mut held = TcpStream::connect(("127.0.0.1", port)).ok()?;
        held.set_read_timeout(Some(Duration::from_secs(5))).ok()?;
        held.write_all(
            format!("GET /healthz HTTP/1.1\r\nhost: 127.0.0.1\r\nauthorization: {auth}\r\n\r\n")
                .as_bytes(),
        )
        .ok()?;
        (read_answer(&mut held).0 == 200).then_some(held)
    };
    let mut held = Vec::new();
    while held.len() < KEYED {
        held.push(slot().expect("鍵の来た接続が枠を取れない"));
    }
    assert!(
        slot().is_some(),
        "{} 本目の鍵の来た接続が断られた",
        KEYED + 1
    );
    assert!(
        matches!(held[0].read(&mut [0u8; 1]), Ok(0)),
        "一番長く待っていた接続が閉じられていない"
    );
    drop(held);

    // Every one in a request (its body still to come): none is let go of, and one more is told
    // they are all in use.
    let in_request = || -> Option<TcpStream> {
        let mut held = TcpStream::connect(("127.0.0.1", port)).ok()?;
        held.write_all(
            format!(
                "POST /mcp HTTP/1.1\r\nhost: 127.0.0.1\r\ncontent-type: application/json\r\n\
                 authorization: {auth}\r\ncontent-length: 100\r\n\r\n"
            )
            .as_bytes(),
        )
        .ok()?;
        Some(held)
    };
    // One let go of while it was still a stranger (connections coming all at once) is opened again.
    let mut busy: Vec<TcpStream> = Vec::new();
    until("どの接続も要求の途中になる", || {
        busy.retain_mut(|b| {
            b.set_read_timeout(Some(Duration::from_millis(5))).unwrap();
            b.read(&mut [0u8; 1]).is_err()
        });
        while busy.len() < KEYED {
            busy.push(in_request().unwrap());
        }
        let (status, _, _) = http(
            port,
            "POST",
            "/mcp",
            &[
                ("content-type", "application/json"),
                ("authorization", &auth),
            ],
            ping,
        );
        status == 503
    });
    drop(busy);
    until("枠が戻る", || {
        http(
            port,
            "POST",
            "/mcp",
            &[
                ("content-type", "application/json"),
                ("authorization", &auth),
            ],
            ping,
        )
        .0 == 200
    });

    drop(app);
    assert_eq!(run(&home, &["stop"]).0, 0);
}

#[test]
fn the_read_only_path_points_at_the_one_and_a_listen_says_what_it_honours() {
    let home = home(0);
    let (mut app, ready) = App::attach(&home);
    let port = ready["mcp"]["port"].as_u64().unwrap() as u16;
    let key = app.core(json!({"op": "access_key_create", "name": "AI"}))["result"]["value"]
        .as_str()
        .unwrap()
        .to_string();
    let auth = format!("Bearer {key}");

    // A client still set for the read-only connection is told where to go, in either era.
    let headers = [
        ("content-type", "application/json"),
        ("authorization", auth.as_str()),
    ];
    let (_, _, legacy) = http(
        port,
        "POST",
        "/mcp/read",
        &headers,
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"t","version":"0"}}}"#,
    );
    let legacy: Value = serde_json::from_str(&legacy).unwrap();
    assert!(legacy.get("result").is_none(), "{legacy}");
    let refusal = legacy["error"]["message"].as_str().unwrap().to_string();
    assert!(refusal.contains("Connect to /mcp again"), "{refusal}");
    let (_, _, modern) = http(
        port,
        "POST",
        "/mcp/read",
        &[
            ("content-type", "application/json"),
            ("authorization", auth.as_str()),
            ("mcp-protocol-version", "2026-07-28"),
            ("mcp-method", "tools/list"),
        ],
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{"_meta":{"io.modelcontextprotocol/protocolVersion":"2026-07-28"}}}"#,
    );
    let modern: Value = serde_json::from_str(&modern).unwrap();
    assert_eq!(modern["error"]["message"], refusal.as_str());

    // A listen: acknowledged with what is honoured of what was asked, then told the list changed;
    // every event tagged with the request that opened it, none of them an answer.
    let (head, events) = listen(
        port,
        &key,
        json!({"toolsListChanged": true, "promptsListChanged": true}),
        2,
    );
    assert!(head.contains("content-type: text/event-stream"), "{head}");
    let methods: Vec<&str> = events
        .iter()
        .map(|e| e["method"].as_str().unwrap())
        .collect();
    assert_eq!(
        methods,
        [
            "notifications/subscriptions/acknowledged",
            "notifications/tools/list_changed"
        ]
    );
    for event in &events {
        assert_eq!(
            event["params"]["_meta"]["io.modelcontextprotocol/subscriptionId"],
            "listen:1"
        );
        assert!(event.get("id").is_none(), "{event}");
    }
    // Prompts are not something this server has: left out, the protocol's way of saying so.
    assert_eq!(
        events[0]["params"]["notifications"],
        json!({"toolsListChanged": true})
    );
    // Nothing asked for, nothing told but the acknowledgement.
    let (_, events) = listen(port, &key, json!({}), 1);
    let methods: Vec<&str> = events
        .iter()
        .map(|e| e["method"].as_str().unwrap())
        .collect();
    assert_eq!(methods, ["notifications/subscriptions/acknowledged"]);

    drop(app);
    assert_eq!(run(&home, &["stop"]).0, 0);
}
