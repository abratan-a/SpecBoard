//! The agent's own and the way to it: `attach` starts the daemon when none runs and hands an
//! app's lines to it — the core's requests answered, what changed told to every app it holds; the
//! daemon stays when an app goes, is one for a home, is started again from a build of another file,
//! and is stopped and asked after from a shell.

use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, channel};
use std::time::Duration;

/// The machine language the agent runs in: it says its own lines (the CLI, the log) in its
/// machine's, and the tests read them in Japanese.
const LANGUAGE: &str = "ja_JP.UTF-8";

const AGENT: &str = env!("CARGO_BIN_EXE_specboard-agent");

/// The longest a line from the daemon is waited for: more than `attach` waits for a daemon it
/// starts (10 s).
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

/// A home of its own for one test (short: a socket's path is held to about a hundred bytes).
fn home() -> Home {
    static N: AtomicUsize = AtomicUsize::new(0);
    let place = std::env::temp_dir().join(format!(
        "sba-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&place);
    std::fs::create_dir_all(&place).unwrap();
    // MCP at any port free: not the one the agents of the host listen at.
    std::fs::write(place.join("agent.json"), r#"{"mcpPort": 0}"#).unwrap();
    Home(place)
}

/// A shell's command on the agent of `home`, its exit code and what it printed.
fn run(exe: &Path, home: &Path, role: &str) -> (i32, String) {
    let out = Command::new(exe)
        .arg(role)
        .env("SPECBOARD_AGENT_HOME", home)
        .env("LC_ALL", LANGUAGE)
        .env_remove("SPECBOARD_AGENT_BUILD")
        .output()
        .unwrap();
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
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
    fn attach(exe: &Path, home: &Path, name: &str) -> (App, Value) {
        Self::attach_built(exe, home, name, "")
    }

    /// An app of the build `build` (the year to the minute; empty: a build of the agent's own).
    fn attach_built(exe: &Path, home: &Path, name: &str, build: &str) -> (App, Value) {
        let mut child = Command::new(exe)
            .arg("attach")
            .env("SPECBOARD_AGENT_HOME", home)
            .env("LC_ALL", LANGUAGE)
            .env("SPECBOARD_AGENT_BUILD", build)
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
        app.send(&json!({"t": "hello", "client": name}));
        let ready = app.line();
        (app, ready)
    }

    /// A request of the core's, its answer and what was told meanwhile.
    fn core_told(&mut self, request: Value) -> (Value, Vec<Value>) {
        let id = self.next;
        self.next += 1;
        self.send(&json!({"t": "core", "id": id, "request": request}));
        let mut told = Vec::new();
        loop {
            let line = self.line();
            if line["t"] == "core" && line["id"] == id {
                return (text_of(&line["answer"]), told);
            }
            assert!(line["t"] == "changed", "{line}");
            told.push(line);
        }
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

    /// A request of the core's, its answer (what changed told meanwhile set aside).
    fn core(&mut self, request: Value) -> Value {
        let id = self.next;
        self.next += 1;
        self.send(&json!({"t": "core", "id": id, "request": request}));
        loop {
            let line = self.line();
            if line["t"] == "core" && line["id"] == id {
                return text_of(&line["answer"]);
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

/// The JSON a line carries as its text.
fn text_of(said: &Value) -> Value {
    serde_json::from_str(said.as_str().expect("JSON の文字")).unwrap()
}

fn projects(app: &mut App) -> Vec<String> {
    app.core(json!({"op": "projects"}))["result"]["projects"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["name"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn an_app_reaches_the_agents_own_which_stays_when_it_goes() {
    let home = home();
    let agent = Path::new(AGENT);
    assert_eq!(run(agent, &home, "status").0, 3, "まだ動いていない");

    // The first app: the daemon started, the core's requests answered.
    let (mut first, ready) = App::attach(agent, &home, "MacBook");
    assert_eq!(ready["t"], "ready", "{ready}");
    let pid = ready["pid"].clone();
    assert!(projects(&mut first).is_empty());
    // A second app: what the first changes is told to it.
    let (mut second, again) = App::attach(agent, &home, "iMac");
    assert_eq!(again["pid"], pid, "同じ本体に繋ぐ");
    // What the first changes is its own: it reads again itself.
    let (made, own) = first
        .core_told(json!({"op": "tool", "name": "project_create", "arguments": {"project": "机"}}));
    assert!(made.get("error").is_none(), "{made}");
    assert!(
        !own.is_empty() && own.iter().all(|l| l["own"] == true),
        "{own:?}"
    );
    let told = second.line();
    assert_eq!(told["t"], "changed", "{told}");
    assert_eq!(told["own"], false, "{told}");
    assert_eq!(text_of(&told["notice"])["op"], "changed", "{told}");
    assert_eq!(projects(&mut second), ["机"]);
    // Each request with the Mac it came from: what one window holds is that Mac's.
    let project = second.core(json!({"op": "projects"}))["result"]["projects"][0]["id"].clone();
    let begun = first.core(json!({"op": "group_begin", "project": project}));
    assert!(begun.get("error").is_none(), "{begun}");
    let holds = second.core(json!({"op": "holds", "project": project}));
    assert_eq!(holds["result"]["holds"][0]["client"], "MacBook", "{holds}");
    first.core(json!({"op": "group_end", "project": project}));

    // The apps gone (their SSH sessions with them): the daemon stays, with what it holds.
    drop(first);
    drop(second);
    let (code, said) = run(agent, &home, "status");
    assert_eq!(code, 0, "{said}");
    assert!(said.contains(&pid.to_string()), "{said}");
    let (mut back, ready) = App::attach(agent, &home, "MacBook");
    assert_eq!(ready["pid"], pid);
    assert_eq!(projects(&mut back), ["机"]);
    drop(back);

    // One a home: a second daemon does nothing.
    let (code, _) = run(agent, &home, "serve");
    assert_eq!(code, 0);
    assert!(run(agent, &home, "status").1.contains(&pid.to_string()));

    // Another build (another file, as the app puts each build in a folder of its own): the daemon
    // stopped and started again from it, what it holds kept.
    let other = home.join("v2").join("specboard-agent");
    std::fs::create_dir_all(other.parent().unwrap()).unwrap();
    std::fs::copy(agent, &other).unwrap();
    let (mut newer, ready) = App::attach(&other, &home, "MacBook");
    assert_ne!(ready["pid"], pid, "{ready}");
    assert_eq!(ready["exe"], other.to_string_lossy().as_ref());
    assert_eq!(projects(&mut newer), ["机"]);
    drop(newer);

    // An app of a newer build: the daemon started again from it. One of an older build is told so,
    // and the newer one runs on, what it holds kept.
    let (mut newest, ready) = App::attach_built(agent, &home, "MacBook", "202610090900");
    assert_eq!(ready["build"], "202610090900", "{ready}");
    let newest_pid = ready["pid"].clone();
    assert_eq!(projects(&mut newest), ["机"]);
    let (_older, said) = App::attach_built(&other, &home, "iMac", "202610081200");
    assert_eq!(said["t"], "failed", "{said}");
    assert!(
        said["message"].as_str().unwrap().contains("202610090900"),
        "{said}"
    );
    assert_eq!(projects(&mut newest), ["机"]);
    assert!(
        run(agent, &home, "status")
            .1
            .contains(&newest_pid.to_string())
    );
    drop(newest);

    // Stopped from a shell, and started again.
    assert_eq!(run(&other, &home, "stop").0, 0);
    assert_eq!(run(&other, &home, "status").0, 3);
    let (code, said) = run(agent, &home, "start");
    assert_eq!(code, 0, "{said}");
    assert!(said.contains("動いています"), "{said}");
    assert_eq!(run(agent, &home, "stop").0, 0);
}

#[test]
fn a_database_a_newer_core_moved_on_is_not_opened_and_the_app_is_told_why() {
    let home = home();
    let agent = Path::new(AGENT);
    let data = home.join("data");
    std::fs::create_dir_all(&data).unwrap();
    let place = data.join("specboard-core.db");
    specboard_core::Core::create(&place).unwrap().close();
    let newer = "core_9999_from_a_newer_core";
    rusqlite::Connection::open(&place)
        .unwrap()
        .execute("INSERT INTO _migrations (name) VALUES (?1)", [newer])
        .unwrap();

    // Not opened: the app is told why where it is told the agent cannot be reached, and no daemon
    // runs on it.
    let (_app, said) = App::attach(agent, &home, "MacBook");
    assert_eq!(said["t"], "failed", "{said}");
    let message = said["message"].as_str().unwrap();
    assert!(
        message.contains("新しい版の SpecBoard で更新されていて、この版では開けません"),
        "{message}"
    );
    assert!(message.contains(&place.display().to_string()), "{message}");
    assert_eq!(run(agent, &home, "status").0, 3);
    let (code, _) = run(agent, &home, "start");
    assert_eq!(code, 1);
    // Nothing was written to it.
    let marks: Vec<String> = rusqlite::Connection::open(&place)
        .unwrap()
        .prepare("SELECT name FROM _migrations")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert!(marks.contains(&newer.to_string()), "{marks:?}");
}

#[test]
fn an_app_reads_why_the_agent_cannot_start_in_its_screens_language_on_a_host_with_none_set() {
    let home = home();
    let agent = Path::new(AGENT);
    let place = home.join("data").join("specboard-core.db");
    std::fs::create_dir_all(place.parent().unwrap()).unwrap();
    specboard_core::Core::create(&place).unwrap().close();
    rusqlite::Connection::open(&place)
        .unwrap()
        .execute(
            "INSERT INTO _migrations (name) VALUES (?1)",
            ["core_9999_from_a_newer_core"],
        )
        .unwrap();
    // A host whose session says no language, an app whose screen is in English.
    let mut child = Command::new(agent)
        .args(["attach", "--lang", "en"])
        .env("SPECBOARD_AGENT_HOME", &*home)
        .env_remove("LC_ALL")
        .env_remove("LC_MESSAGES")
        .env_remove("LANG")
        .env_remove("SPECBOARD_AGENT_BUILD")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    writeln!(
        input,
        "{}",
        json!({"t": "hello", "client": "MacBook", "lang": "en"})
    )
    .unwrap();
    drop(input);
    let mut line = String::new();
    BufReader::new(child.stdout.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    let _ = child.wait();
    let said: Value = serde_json::from_str(&line).unwrap_or_else(|e| panic!("{line:?}: {e}"));
    assert_eq!(said["t"], "failed", "{said}");
    let message = said["message"].as_str().unwrap();
    assert!(message.contains("newer SpecBoard"), "{message}");
    assert!(
        !message
            .chars()
            .any(|c| matches!(c, '\u{3040}'..='\u{30ff}' | '\u{4e00}'..='\u{9fff}')),
        "{message}"
    );
}
