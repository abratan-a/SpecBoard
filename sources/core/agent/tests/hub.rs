//! The hub (`specboard-agent hub …`, `specboard://SpecBoard/Docs/Design:DesignHub` §2・§9): the
//! place the agents of one person meet. It keeps who its members are and hands their lines on —
//! the sender written by the hub from the key the connection came by — and nothing of any project.
//! A member's connection is what its key's forced command starts (`hub attach --as <id>`); the app's
//! is `hub attach`, which also adds and removes members.

use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, channel};
use std::time::Duration;

/// The hub says its own lines (why it turns one away) in its machine's language: Japanese here.
const LANGUAGE: &str = "ja_JP.UTF-8";

const AGENT: &str = env!("CARGO_BIN_EXE_specboard-agent");

/// The longest a line is waited for: more than `hub attach` waits for a hub it starts.
const LINE_WAIT: Duration = Duration::from_secs(30);

/// The lines' form the hub and the agents of this build speak.
const PROTOCOL: u64 = 1;

/// A host of its own for one test: the hub's user's home (`home/`, with its `.ssh`), the hub's
/// place (`hub/`) and the host's public host keys (`etc/`). Its hub is stopped and the folder taken
/// away when it goes, whether the test passed or not.
struct Host {
    root: PathBuf,
    /// How often the hub asks a member whether it is there, when not the 15 seconds it has.
    ping: Option<&'static str>,
}

impl Drop for Host {
    fn drop(&mut self) {
        // Nothing here unwraps: it runs while a test that failed unwinds too.
        let _ = self.command(&["hub", "stop"]).stdin(Stdio::null()).output();
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// The host's one host key, as `enroll` hands it to an agent.
const HOST_KEY: &str =
    "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIHostHostHostHostHostHostHostHostHostHostHost";

/// A member's public key, as its agent would hand it over (with a comment, which is not kept).
fn key_of(id: &str) -> String {
    format!("ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIKey{id}Key{id}Key{id}Key{id}Key{id}Key{id}Key")
}

/// A line of `authorized_keys` that is not the hub's.
const OTHERS_LINE: &str = "ssh-rsa AAAAB3NzaC1yc2EAAAADAQABAAABAQother someone@elsewhere";

impl Host {
    fn new() -> Host {
        Host::with_ping(None)
    }

    fn with_ping(ping: Option<&'static str>) -> Host {
        static N: AtomicUsize = AtomicUsize::new(0);
        // Short: a socket's path is held to about a hundred bytes.
        let root = std::env::temp_dir().join(format!(
            "sbh-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("home/.ssh")).unwrap();
        std::fs::create_dir_all(root.join("etc")).unwrap();
        std::fs::write(
            root.join("home/.ssh/authorized_keys"),
            format!("{OTHERS_LINE}\n"),
        )
        .unwrap();
        std::fs::write(
            root.join("etc/ssh_host_ed25519_key.pub"),
            format!("{HOST_KEY} root@hub\n"),
        )
        .unwrap();
        Host { root, ping }
    }

    /// The hub's place.
    fn hub(&self) -> PathBuf {
        self.root.join("hub")
    }

    fn authorized_keys(&self) -> Vec<String> {
        std::fs::read_to_string(self.root.join("home/.ssh/authorized_keys"))
            .unwrap()
            .lines()
            .map(str::to_string)
            .collect()
    }

    /// The hub's members, as it keeps them.
    fn kept(&self) -> Value {
        serde_json::from_str(&std::fs::read_to_string(self.hub().join("members.json")).unwrap())
            .unwrap()
    }

    fn command(&self, args: &[&str]) -> Command {
        let mut command = Command::new(AGENT);
        command
            .args(args)
            .env("HOME", self.root.join("home"))
            .env("SPECBOARD_HUB_HOME", self.hub())
            .env("SPECBOARD_HUB_HOST_KEYS", self.root.join("etc"))
            .env("LC_ALL", LANGUAGE)
            .env_remove("SPECBOARD_AGENT_HOME")
            .env_remove("SPECBOARD_AGENT_BUILD")
            .env_remove("SPECBOARD_HUB_PING");
        if let Some(ping) = self.ping {
            command.env("SPECBOARD_HUB_PING", ping);
        }
        command
    }

    /// A shell's command on the hub: its exit code and what it printed.
    fn run(&self, args: &[&str]) -> (i32, String) {
        let out = self.command(args).stdin(Stdio::null()).output().unwrap();
        (
            out.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&out.stdout).into_owned(),
        )
    }

    /// The app's connection (`hub attach`), which starts the hub when none runs: said hello, with
    /// what the hub answered.
    fn app(&self) -> (Line, Value) {
        let mut line = Line::open(self.command(&["hub", "attach"]));
        line.send(&json!({"t": "hello"}));
        let ready = line.next();
        (line, ready)
    }

    /// A member's connection, as its key's forced command starts it (`hub attach --as <id>`): said
    /// hello with `build`, with what the hub answered.
    fn member(&self, id: &str, build: &str) -> (Line, Value) {
        let mut line = self.member_silent(id);
        line.send(&json!({"t": "hello", "protocol": PROTOCOL, "build": build}));
        let ready = line.next();
        (line, ready)
    }

    /// A member's connection that has said nothing yet.
    fn member_silent(&self, id: &str) -> Line {
        Line::open(self.command(&["hub", "attach", "--as", id]))
    }

    /// The hub running, with the members `(id, name)` enrolled: the app's connection that did it.
    fn with_members(&self, members: &[(&str, &str)]) -> Line {
        let (mut app, ready) = self.app();
        assert_eq!(ready["t"], "ready", "{ready}");
        for (id, name) in members {
            app.send(&json!({"t": "enroll", "id": id, "name": name, "key": key_of(id)}));
            let said = app.next_of("enrolled");
            assert_eq!(said["id"], *id, "{said}");
        }
        app
    }
}

/// One connection to the hub, its lines read as they come so that one is waited for no longer than
/// `LINE_WAIT`.
struct Line {
    child: Child,
    input: Option<ChildStdin>,
    lines: Receiver<String>,
}

impl Drop for Line {
    fn drop(&mut self) {
        self.input = None;
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Line {
    fn open(mut command: Command) -> Line {
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let input = child.stdin.take();
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
        Line {
            child,
            input,
            lines,
        }
    }

    fn send(&mut self, message: &Value) {
        let input = self.input.as_mut().unwrap();
        writeln!(input, "{message}").unwrap();
        input.flush().unwrap();
    }

    /// The next line; none when the connection closed.
    fn try_next(&mut self) -> Option<Value> {
        match self.lines.recv_timeout(LINE_WAIT) {
            Ok(text) => Some(
                serde_json::from_str(&text)
                    .unwrap_or_else(|e| panic!("JSON でない行: {text:?}（{e}）")),
            ),
            Err(RecvTimeoutError::Timeout) => panic!("{LINE_WAIT:?} 待っても 1 行も来ない"),
            Err(RecvTimeoutError::Disconnected) => None,
        }
    }

    fn next(&mut self) -> Value {
        self.try_next().expect("繋ぎが閉じた")
    }

    /// What the hub says next to what this connection sent: the next line that is not one it
    /// sends of its own accord (who is there, whether this one is).
    fn said(&mut self) -> Value {
        loop {
            let line = self.next();
            if line["t"] != "members" && line["t"] != "ping" {
                return line;
            }
        }
    }

    /// The next line of the kind `t`, the ones before it (who is there, a ping) passed over.
    fn next_of(&mut self, t: &str) -> Value {
        loop {
            let line = self.next();
            if line["t"] == t {
                return line;
            }
            assert_ne!(line["t"], "refused", "{line}");
        }
    }

    /// Waited for until the hub closes it: the lines that came before it did.
    fn until_closed(&mut self) -> Vec<Value> {
        let mut before = Vec::new();
        while let Some(line) = self.try_next() {
            before.push(line);
        }
        before
    }

    /// The connection let go of from this side, as an agent that went: the lines that had come.
    fn until_closed_soon(&mut self) -> Vec<Value> {
        self.input = None;
        let _ = self.child.kill();
        self.until_closed()
    }

    /// The exit code of the process the connection ran in, once it ended.
    fn ended(&mut self) -> i32 {
        self.input = None;
        self.child.wait().unwrap().code().unwrap_or(-1)
    }
}

/// The members as a line says them, each without when it came or was last there, or which
/// connection of its it is (looked at where it is the point).
fn members(line: &Value) -> Value {
    let mut members = line["members"].clone();
    for member in members.as_array_mut().into_iter().flatten() {
        let member = member.as_object_mut().unwrap();
        member.remove("since");
        member.remove("seen");
        member.remove("link");
    }
    members
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

#[test]
fn the_hub_hands_a_line_on_with_the_sender_its_key_names() {
    let host = Host::new();
    let _app = host.with_members(&[("a", "Mac"), ("b", "Server")]);
    let (mut a, ready) = host.member("a", "1");
    assert_eq!(ready["t"], "ready", "{ready}");
    assert_eq!(ready["protocol"], PROTOCOL);
    // Who it is there: what its key says, not what it says.
    assert_eq!(ready["id"], "a");
    let (mut b, _) = host.member("b", "1");

    // A call, saying it is from someone else: the hub writes who it came from, and hands the body
    // on as it is.
    let body = json!({"k": "call", "tool": "card_read", "arguments": {"project": "p", "list": [1, {"x": "日本語"}]}});
    a.send(&json!({"t": "to", "to": "b", "id": 1, "body": body, "from": "x"}));
    assert_eq!(
        b.next_of("from"),
        json!({"t": "from", "from": "a", "id": 1, "body": body})
    );
    // Its answer, by the call's number.
    let answer = json!({"output": {"json": {"card": "Schemas:Order"}}});
    b.send(&json!({"t": "to", "to": "a", "re": 1, "body": answer}));
    assert_eq!(
        a.next_of("from"),
        json!({"t": "from", "from": "b", "re": 1, "body": answer})
    );

    // A member's connection is only a member's: it adds no one.
    a.send(&json!({"t": "enroll", "id": "c", "name": "Other", "key": key_of("c")}));
    let said = a.said();
    assert_eq!(said["t"], "error", "{said}");
    assert_eq!(host.kept()["members"].as_array().unwrap().len(), 2);
    assert_eq!(host.authorized_keys().len(), 3);
}

#[test]
fn a_line_to_a_member_away_or_unknown_comes_back_undelivered() {
    let host = Host::new();
    let _app = host.with_members(&[("a", "Mac"), ("b", "Server")]);
    let (mut a, _) = host.member("a", "1");

    a.send(&json!({"t": "to", "to": "b", "id": 7, "body": {"k": "call"}}));
    assert_eq!(
        a.said(),
        json!({"t": "undelivered", "id": 7, "why": "offline"})
    );
    a.send(&json!({"t": "to", "to": "z", "id": 8, "body": {"k": "call"}}));
    assert_eq!(
        a.said(),
        json!({"t": "undelivered", "id": 8, "why": "unknown"})
    );
    // An answer, or what waits for none, to one that is away is let go of: nobody waits for it.
    a.send(&json!({"t": "to", "to": "b", "re": 3, "body": {"output": {"json": 1}}}));
    a.send(&json!({"t": "to", "to": "b", "body": {"k": "shares", "projects": []}}));
    a.send(&json!({"t": "to", "to": "z", "id": 9, "body": {"k": "call"}}));
    assert_eq!(
        a.said(),
        json!({"t": "undelivered", "id": 9, "why": "unknown"})
    );
}

#[test]
fn a_call_whose_owner_goes_before_answering_comes_back_cut() {
    let host = Host::new();
    let _app = host.with_members(&[("a", "Mac"), ("b", "Server")]);
    let (mut a, _) = host.member("a", "1");
    let (mut b, _) = host.member("b", "1");

    // Two calls handed to B; it answers one, and goes.
    for id in [5, 6] {
        a.send(&json!({"t": "to", "to": "b", "id": id, "body": {"k": "call"}}));
        assert_eq!(b.next_of("from")["id"], id);
    }
    b.send(&json!({"t": "to", "to": "a", "re": 6, "body": {"output": {"json": 1}}}));
    assert_eq!(a.said()["re"], 6);
    drop(b);
    // The one it had not answered is said to be lost — whether it was done is not known —, the
    // one it answered is not.
    assert_eq!(a.said(), json!({"t": "undelivered", "id": 5, "why": "cut"}));

    // So is one handed to a connection another of the same member then took the place of.
    let (mut first, _) = host.member("b", "1");
    a.send(&json!({"t": "to", "to": "b", "id": 7, "body": {"k": "call"}}));
    assert_eq!(first.next_of("from")["id"], 7);
    let (mut second, _) = host.member("b", "1");
    assert_eq!(a.said(), json!({"t": "undelivered", "id": 7, "why": "cut"}));
    // And nothing more is said of them when the member goes again.
    drop(second.until_closed_soon());
    a.send(&json!({"t": "to", "to": "z", "id": 8, "body": {"k": "call"}}));
    assert_eq!(
        a.said(),
        json!({"t": "undelivered", "id": 8, "why": "unknown"})
    );
}

#[test]
fn an_answer_to_a_call_it_was_not_handed_is_not_handed_on() {
    let host = Host::new();
    let _app = host.with_members(&[("a", "Mac"), ("b", "Server"), ("c", "Laptop")]);
    let (mut a, _) = host.member("a", "1");
    let (mut b, _) = host.member("b", "1");
    let (mut c, _) = host.member("c", "1");
    let nowhere = |line: &mut Line, id: u64| {
        line.send(&json!({"t": "to", "to": "z", "id": id, "body": {"k": "call"}}));
        assert_eq!(
            line.said(),
            json!({"t": "undelivered", "id": id, "why": "unknown"})
        );
    };

    a.send(&json!({"t": "to", "to": "b", "id": 4, "body": {"k": "call"}}));
    assert_eq!(b.next_of("from")["id"], 4);
    // Another member answers it, by its number: a call is answered by the one it was handed to.
    // The hub has read that before the owner answers.
    c.send(&json!({"t": "to", "to": "a", "re": 4, "body": {"output": {"json": "another's"}}}));
    nowhere(&mut c, 1);
    let answer = json!({"output": {"json": "the owner's"}});
    b.send(&json!({"t": "to", "to": "a", "re": 4, "body": answer}));
    assert_eq!(
        a.said(),
        json!({"t": "from", "from": "b", "re": 4, "body": answer})
    );
    // Answered, it is owed no more: said again, it is not handed on again.
    b.send(&json!({"t": "to", "to": "a", "re": 4, "body": {"output": {"json": "again"}}}));
    nowhere(&mut b, 1);
    nowhere(&mut a, 5);
}

#[test]
fn a_connection_in_anothers_place_is_told_apart_from_it() {
    let host = Host::new();
    let _app = host.with_members(&[("a", "Mac"), ("b", "Server")]);
    let (mut a, _) = host.member("a", "1");
    let link_of_b = |line: &Value| {
        let b = &line["members"][1];
        assert_eq!(b["online"], true, "{line}");
        b["link"].as_u64().unwrap_or_else(|| panic!("{line}"))
    };
    let (_first, ready) = host.member("b", "1");
    let first = link_of_b(&ready);
    assert_eq!(link_of_b(&a.next_of("members")), first);
    // Its second connection, the moment after: what was said to the first may never have been
    // read, so who tells a member things knows it for another.
    let (_second, ready) = host.member("b", "1");
    let second = link_of_b(&ready);
    assert_ne!(second, first);
    assert_eq!(link_of_b(&a.next_of("members")), second);
}

#[test]
fn a_list_of_members_that_cannot_be_read_stops_the_hub_and_leaves_the_keys() {
    let host = Host::new();
    drop(host.with_members(&[("a", "Mac")]));
    assert_eq!(host.run(&["hub", "stop"]).0, 0);
    let keys = host.authorized_keys();
    assert_eq!(keys.len(), 2, "{keys:?}");
    let list = host.hub().join("members.json");
    let kept = std::fs::read_to_string(&list).unwrap();

    // Cut short, or holding one that could be no member's (an id is written into the command its
    // key runs): the hub does not start as one with no members, which would take every member's
    // key away.
    let not_an_id = json!({"members": [{"id": "a b", "name": "Mac", "key": key_of("a")}]});
    for broken in [&kept[..kept.len() / 2], &not_an_id.to_string()] {
        std::fs::write(&list, broken).unwrap();
        let (_app, said) = host.app();
        assert_eq!(said["t"], "failed", "{broken}: {said}");
        let why = said["message"].as_str().unwrap();
        assert!(why.contains("members.json"), "{why}");
        assert_eq!(host.authorized_keys(), keys, "{broken}");
        assert_eq!(std::fs::read_to_string(&list).unwrap(), broken);
    }

    // Put right, it starts with its members.
    std::fs::write(&list, &kept).unwrap();
    let (_app, ready) = host.app();
    assert_eq!(
        members(&ready),
        json!([{"id": "a", "name": "Mac", "online": false}])
    );
    assert_eq!(host.authorized_keys(), keys);
}

#[test]
fn members_are_told_who_is_there_as_it_changes() {
    let host = Host::new();
    let mut app = host.with_members(&[("a", "Mac"), ("b", "Server")]);
    let (mut a, ready) = host.member("a", "7");
    assert_eq!(
        members(&ready),
        json!([
            {"id": "a", "name": "Mac", "online": true, "build": "7"},
            {"id": "b", "name": "Server", "online": false},
        ])
    );
    let both = json!([
        {"id": "a", "name": "Mac", "online": true, "build": "7"},
        {"id": "b", "name": "Server", "online": true, "build": "8"},
    ]);
    let (b, ready) = host.member("b", "8");
    assert_eq!(members(&ready), both);
    assert_eq!(members(&a.next_of("members")), both);
    // The app's connection is told too.
    let told = loop {
        let line = app.next_of("members");
        if line["members"][1]["online"] == true {
            break line;
        }
    };
    assert_eq!(members(&told), both);

    // Since when each is there.
    let recent = |at: &Value| at.as_u64().is_some_and(|at| now().abs_diff(at) < 60);
    assert!(recent(&told["members"][0]["since"]), "{told}");
    assert!(recent(&told["members"][1]["since"]), "{told}");
    assert!(told["members"][1].get("seen").is_none(), "{told}");

    drop(b);
    let alone = json!([
        {"id": "a", "name": "Mac", "online": true, "build": "7"},
        {"id": "b", "name": "Server", "online": false},
    ]);
    let told = a.next_of("members");
    assert_eq!(members(&told), alone);
    assert_eq!(members(&app.next_of("members")), alone);
    // One that is away: when it was last there, kept by the hub — still known once the hub has
    // been started again.
    assert!(recent(&told["members"][1]["seen"]), "{told}");
    assert!(told["members"][1].get("since").is_none(), "{told}");
    assert!(
        recent(&host.kept()["members"][1]["seen"]),
        "{}",
        host.kept()
    );
    assert!(host.kept()["members"][0].get("seen").is_none());
}

#[test]
fn enrolling_writes_the_member_and_its_key_and_removing_takes_both_away() {
    let host = Host::new();
    let (mut app, ready) = host.app();
    assert_eq!(ready["protocol"], PROTOCOL, "{ready}");
    assert_eq!(members(&ready), json!([]));

    app.send(&json!({"t": "enroll", "id": "a", "name": "Mac", "key": format!("{} mac@example", key_of("a"))}));
    // The host's public host keys, for the agent to trust this host alone; and who the members
    // are now, with the answer.
    let enrolled = app.next_of("enrolled");
    assert_eq!(enrolled["id"], "a");
    assert_eq!(enrolled["hostKeys"], json!([HOST_KEY]));
    assert_eq!(
        members(&enrolled),
        json!([{"id": "a", "name": "Mac", "online": false}])
    );
    let kept = host.kept();
    let one = &kept["members"][0];
    assert_eq!(kept["members"].as_array().unwrap().len(), 1);
    assert_eq!(one["id"], "a");
    assert_eq!(one["name"], "Mac");
    assert_eq!(one["key"], key_of("a"));
    assert!(one["added"].is_string(), "{kept}");
    // The key may only reach the hub, as the member the hub's line names: the one program that
    // stays where it is whatever build runs, and nothing else of SSH.
    let link = host.hub().join("specboard-agent");
    let line_of = |id: &str| {
        format!(
            "command=\"{} hub attach --as {id}\",restrict {} specboard-hub:{id}",
            link.display(),
            key_of(id)
        )
    };
    assert_eq!(
        host.authorized_keys(),
        [OTHERS_LINE.to_string(), line_of("a")]
    );
    let mode = std::fs::metadata(host.root.join("home/.ssh/authorized_keys"))
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o600);
    assert_eq!(
        std::fs::read_link(&link).unwrap(),
        Path::new(AGENT),
        "今動いているビルドを指す"
    );

    // No two of one id, one name, or one key; and nothing else is a member's id or key.
    for (id, name, key) in [
        ("a", "Other", key_of("x")),
        ("x", "Mac", key_of("x")),
        ("x", "mac", key_of("x")),
        ("x", "Other", key_of("a")),
        ("x y", "Other", key_of("x")),
        ("x\";id", "Other", key_of("x")),
        (
            "x",
            "Other",
            "ssh-ed25519 AAAA\ncommand=\"sh\" ssh-ed25519 AAAA".to_string(),
        ),
        ("x", "Other", "not a key".to_string()),
        ("x", "", key_of("x")),
    ] {
        app.send(&json!({"t": "enroll", "id": id, "name": name, "key": key}));
        let said = app.said();
        assert_eq!(said["t"], "refused", "{id} {name} {key}: {said}");
        assert!(!said["message"].as_str().unwrap().is_empty(), "{said}");
    }
    assert_eq!(host.kept(), kept);
    assert_eq!(host.authorized_keys().len(), 2);

    app.send(&json!({"t": "enroll", "id": "b", "name": "Server", "key": key_of("b")}));
    app.next_of("enrolled");
    // A name of its own, changed.
    app.send(&json!({"t": "rename", "id": "a", "name": "Server"}));
    assert_eq!(app.said()["t"], "refused");
    app.send(&json!({"t": "rename", "id": "a", "name": "MacBook"}));
    let renamed = app.next_of("renamed");
    assert_eq!(renamed["id"], "a");
    assert_eq!(members(&renamed)[0]["name"], "MacBook");
    assert_eq!(host.kept()["members"][0]["name"], "MacBook");
    let renamed = loop {
        let line = app.next_of("members");
        if line["members"][0]["name"] == "MacBook" {
            break line;
        }
    };
    assert_eq!(
        members(&renamed),
        json!([
            {"id": "a", "name": "MacBook", "online": false},
            {"id": "b", "name": "Server", "online": false},
        ])
    );

    // Removed: its line and its place among the members go, and its connection is closed.
    let (mut a, _) = host.member("a", "1");
    app.send(&json!({"t": "remove", "id": "a"}));
    let removed = app.next_of("removed");
    assert_eq!(removed["id"], "a");
    assert_eq!(
        members(&removed),
        json!([{"id": "b", "name": "Server", "online": false}])
    );
    a.until_closed();
    assert_eq!(
        host.authorized_keys(),
        [OTHERS_LINE.to_string(), line_of("b")]
    );
    let kept = host.kept();
    assert_eq!(kept["members"].as_array().unwrap().len(), 1);
    assert_eq!(kept["members"][0]["id"], "b");
    // And it is not let in again.
    let mut again = host.member_silent("a");
    assert_eq!(again.next()["t"], "refused");
    assert!(again.try_next().is_none());
}

#[test]
fn a_member_that_says_nothing_is_let_go() {
    let host = Host::with_ping(Some("0.2"));
    let mut app = host.with_members(&[("a", "Mac")]);
    let (mut a, _) = host.member("a", "1");
    loop {
        let line = app.next_of("members");
        if line["members"][0]["online"] == true {
            break;
        }
    }
    // Asked whether it is there, and saying nothing: closed.
    let before = a.until_closed();
    assert!(before.iter().any(|l| l["t"] == "ping"), "{before:?}");
    assert_eq!(
        members(&app.next_of("members")),
        json!([{"id": "a", "name": "Mac", "online": false}])
    );

    // One that answers stays.
    let (mut a, _) = host.member("a", "1");
    for _ in 0..6 {
        assert_eq!(a.next_of("ping"), json!({"t": "ping"}));
        a.send(&json!({"t": "pong"}));
    }
}

#[test]
fn a_member_not_enrolled_is_turned_away() {
    let host = Host::new();
    let app = host.with_members(&[]);
    let mut z = host.member_silent("z");
    let said = z.next();
    assert_eq!(said["t"], "refused", "{said}");
    assert!(said["message"].as_str().unwrap().contains("名簿"), "{said}");
    assert!(z.try_next().is_none());
    assert_ne!(z.ended(), 0);

    // With no hub running, a member's connection starts none: only the app's does.
    drop(app);
    assert_eq!(host.run(&["hub", "stop"]).0, 0);
    assert_eq!(host.run(&["hub", "status"]).0, 3);
    let mut a = host.member_silent("a");
    let said = a.next();
    assert_eq!(said["t"], "refused", "{said}");
    assert!(a.try_next().is_none());
    assert_ne!(a.ended(), 0);
    assert_eq!(host.run(&["hub", "status"]).0, 3, "本体が起きた");
}

#[test]
fn a_members_second_connection_takes_the_firsts_place() {
    let host = Host::new();
    let _app = host.with_members(&[("a", "Mac"), ("b", "Server")]);
    let (mut first, _) = host.member("a", "1");
    let (mut b, _) = host.member("b", "1");
    let (mut second, ready) = host.member("a", "1");
    assert_eq!(ready["t"], "ready", "{ready}");
    // The one before is closed: a connection half gone is not left to take what is sent.
    first.until_closed();
    b.send(&json!({"t": "to", "to": "a", "id": 1, "body": {"k": "call"}}));
    assert_eq!(
        second.next_of("from"),
        json!({"t": "from", "from": "b", "id": 1, "body": {"k": "call"}})
    );
}

#[test]
fn another_protocol_is_turned_away_saying_which_to_renew() {
    let host = Host::new();
    let mut app = host.with_members(&[("a", "Mac")]);
    for (protocol, renew) in [(PROTOCOL + 1, "ハブを新しく"), (0, "エージェントを新しく")]
    {
        let mut a = host.member_silent("a");
        a.send(&json!({"t": "hello", "protocol": protocol, "build": "1"}));
        let said = a.next();
        assert_eq!(said["t"], "refused", "{said}");
        let message = said["message"].as_str().unwrap();
        assert!(message.contains(renew), "{protocol}: {message}");
        assert!(a.try_next().is_none());
    }
    // Neither was ever there.
    app.send(&json!({"t": "hello"}));
    assert_eq!(
        members(&app.next_of("ready")),
        json!([{"id": "a", "name": "Mac", "online": false}])
    );
}

#[test]
fn the_hub_keeps_no_database() {
    let host = Host::new();
    let _app = host.with_members(&[("a", "Mac"), ("b", "Server")]);
    let (mut a, _) = host.member("a", "1");
    let (mut b, _) = host.member("b", "1");
    a.send(&json!({"t": "to", "to": "b", "id": 1, "body": {"k": "call", "tool": "card_list"}}));
    b.next_of("from");

    let mut held: Vec<String> = std::fs::read_dir(host.hub())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    held.sort();
    assert_eq!(
        held,
        [
            "hub.lock",
            "hub.log",
            "hub.sock",
            "members.json",
            "specboard-agent"
        ]
    );
    let socket = std::fs::metadata(host.hub().join("hub.sock")).unwrap();
    assert_eq!(socket.permissions().mode() & 0o777, 0o600);
    // No agent's place either: nothing of a project is kept on this host.
    assert!(!host.root.join("home/.specboard").exists());
}
