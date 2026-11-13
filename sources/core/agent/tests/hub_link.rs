//! An agent's link to the hub (`specboard://SpecBoard/Docs/Design:DesignHub` §2・§4・§6): it joins
//! with a key and an id of its own, keeps one SSH connection out to the hub, and through it a
//! project one agent shares is read and written from the sessions of another, with that other's
//! own access key. The host's `ssh` is stood in for by a script (`SPECBOARD_SSH`) that does what
//! the hub's host's sshd does: finds the line of the hub's `authorized_keys` for the key it was
//! handed, and runs the command that line forces — so the hub is a real one, reached as it is.

use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, channel};
use std::time::{Duration, Instant};

/// The agents say their own lines in their machine's language; MCP is answered in English.
const LANGUAGE: &str = "ja_JP.UTF-8";

const AGENT: &str = env!("CARGO_BIN_EXE_specboard-agent");

const LINE_WAIT: Duration = Duration::from_secs(30);

/// The hub's host's one host key.
const HOST_KEY: &str =
    "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIHostHostHostHostHostHostHostHostHostHostHost";

/// Where an agent is told the hub is, as its host's ssh names it.
const HUB_AT: &str = "sbhub@hub.example";

/// Hosts of their own for one test, in one folder: the hub's (`h/`: its user's home, the hub's
/// place, the host's keys) and one a named agent (its place). Everything running is stopped and
/// the folder taken away when it goes, whether the test passed or not.
struct World {
    root: PathBuf,
    agents: std::cell::RefCell<Vec<(PathBuf, String)>>,
}

impl Drop for World {
    fn drop(&mut self) {
        // Nothing here unwraps: it runs while a test that failed unwinds too.
        for (home, build) in self.agents.borrow().iter() {
            let _ = agent_command(&self.root, home, build, &["stop"])
                .stdin(Stdio::null())
                .output();
        }
        let _ = self
            .hub_command(&["hub", "stop"])
            .stdin(Stdio::null())
            .output();
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn agent_command(root: &Path, home: &Path, build: &str, args: &[&str]) -> Command {
    let mut command = Command::new(AGENT);
    command
        .args(args)
        .env("SPECBOARD_AGENT_HOME", home)
        .env("SPECBOARD_SSH", root.join("ssh"))
        .env("LC_ALL", LANGUAGE)
        .env_remove("SPECBOARD_HUB_HOME")
        .env_remove("SPECBOARD_AGENT_BUILD");
    if !build.is_empty() {
        command.env("SPECBOARD_AGENT_BUILD", build);
    }
    command
}

impl World {
    fn new() -> World {
        static N: AtomicUsize = AtomicUsize::new(0);
        // Short: a socket's path is held to about a hundred bytes.
        let root = std::env::temp_dir().join(format!(
            "sbl-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("h/home/.ssh")).unwrap();
        std::fs::create_dir_all(root.join("h/etc")).unwrap();
        std::fs::write(
            root.join("h/etc/ssh_host_ed25519_key.pub"),
            format!("{HOST_KEY} root@hub\n"),
        )
        .unwrap();
        // What sshd does with a key: the line of `authorized_keys` it is in, and the command
        // that line forces, run as the hub's user.
        let ssh = root.join("ssh");
        std::fs::write(
            &ssh,
            format!(
                r#"#!/bin/sh
echo "$*" >> '{calls}'
echo $$ >> '{pids}'
key=''
while [ $# -gt 0 ]; do
  case "$1" in
    -i) key="$2"; shift 2;;
    -o) shift 2;;
    --) shift; break;;
    -*) shift;;
    *) break;;
  esac
done
if [ ! -f "$key.pub" ]; then echo 'no key' >&2; exit 255; fi
pub=$(cut -d' ' -f1,2 "$key.pub")
line=$(grep -F "$pub" '{authorized}' 2>/dev/null | head -n 1)
if [ -z "$line" ]; then echo 'Permission denied (publickey).' >&2; exit 255; fi
cmd=$(printf '%s\n' "$line" | sed -e 's/^command="//' -e 's/",restrict .*$//')
export HOME='{home}' SPECBOARD_HUB_HOME='{hub}'
unset SPECBOARD_AGENT_HOME
exec $cmd
"#,
                calls = root.join("calls").display(),
                pids = root.join("pids").display(),
                authorized = root.join("h/home/.ssh/authorized_keys").display(),
                home = root.join("h/home").display(),
                hub = root.join("h/hub").display(),
            ),
        )
        .unwrap();
        std::fs::set_permissions(&ssh, std::fs::Permissions::from_mode(0o755)).unwrap();
        World {
            root,
            agents: Default::default(),
        }
    }

    fn hub_command(&self, args: &[&str]) -> Command {
        let mut command = Command::new(AGENT);
        command
            .args(args)
            .env("HOME", self.root.join("h/home"))
            .env("SPECBOARD_HUB_HOME", self.root.join("h/hub"))
            .env("SPECBOARD_HUB_HOST_KEYS", self.root.join("h/etc"))
            .env("LC_ALL", LANGUAGE)
            .env_remove("SPECBOARD_AGENT_HOME")
            .env_remove("SPECBOARD_AGENT_BUILD");
        command
    }

    /// The app's connection to the hub (`hub attach`), which starts it when none runs.
    fn hub(&self) -> Line {
        let mut line = Line::open(self.hub_command(&["hub", "attach"]));
        line.send(&json!({"t": "hello"}));
        let ready = line.next_of("ready");
        assert_eq!(ready["t"], "ready", "{ready}");
        line
    }

    /// The place of the agent `name`, made the first time: MCP at any port free.
    fn home_of(&self, name: &str, build: &str) -> PathBuf {
        let home = self.root.join(name);
        if !home.exists() {
            std::fs::create_dir_all(&home).unwrap();
            std::fs::write(home.join("agent.json"), r#"{"mcpPort": 0}"#).unwrap();
            self.agents
                .borrow_mut()
                .push((home.clone(), build.to_string()));
        }
        home
    }

    /// An agent's app (`attach`, which starts the agent when none runs).
    fn agent(&self, name: &str) -> Agent {
        self.agent_built(name, "")
    }

    /// An agent of the app's build `build`.
    fn agent_built(&self, name: &str, build: &str) -> Agent {
        let home = self.home_of(name, build);
        let mut line = Line::open(agent_command(&self.root, &home, build, &["attach"]));
        line.send(&json!({"t": "hello", "client": "MacBook", "lang": "ja"}));
        let ready = line.next_of("ready");
        Agent {
            mcp_port: ready["mcp"]["port"].as_u64().unwrap() as u16,
            line,
            home,
            build: build.to_string(),
            next: 0,
            id: String::new(),
        }
    }

    /// The agent made a member of the hub as `name`, and linked.
    fn join(&self, hub: &mut Line, agent: &mut Agent, name: &str) {
        agent.line.send(&json!({"t": "hub_key"}));
        let made = agent.line.next_of("hub_key");
        agent.id = made["id"].as_str().unwrap().to_string();
        hub.send(&json!({"t": "enroll", "id": agent.id, "name": name, "key": made["key"]}));
        let enrolled = hub.next_of("enrolled");
        agent
            .line
            .send(&json!({"t": "hub_join", "to": HUB_AT, "hostKeys": enrolled["hostKeys"]}));
        agent.until_link("up");
    }

    /// A member the test speaks for, with no agent behind it: added to the hub as `name`, and its
    /// connection as its key's command starts it, said hello.
    fn member(&self, hub: &mut Line, id: &str, name: &str) -> Line {
        let key = format!("ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIMember{id}Member{id}Member{id}");
        hub.send(&json!({"t": "enroll", "id": id, "name": name, "key": key}));
        hub.next_of("enrolled");
        self.member_again(id)
    }

    /// One more connection of a member the test speaks for.
    fn member_again(&self, id: &str) -> Line {
        let mut line = Line::open(self.hub_command(&["hub", "attach", "--as", id]));
        line.send(&json!({"t": "hello", "protocol": 1}));
        line.next_of("ready");
        line
    }

    /// What the stand-in for `ssh` was handed, a line a call.
    fn ssh_calls(&self) -> Vec<String> {
        std::fs::read_to_string(self.root.join("calls"))
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect()
    }

    /// The processes the stand-in for `ssh` ran as that are still there.
    fn ssh_alive(&self) -> Vec<i32> {
        std::fs::read_to_string(self.root.join("pids"))
            .unwrap_or_default()
            .lines()
            .filter_map(|l| l.trim().parse::<i32>().ok())
            // SAFETY: signal 0 only asks whether the process is there.
            .filter(|pid| unsafe { libc::kill(*pid, 0) } == 0)
            .collect()
    }
}

/// Waited for until `done` says so, for `seconds` at most.
fn until(what: &str, seconds: u64, mut done: impl FnMut() -> bool) {
    let end = Instant::now() + Duration::from_secs(seconds);
    while !done() {
        assert!(Instant::now() < end, "{seconds} 秒待っても成らない: {what}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// One connection of an app's, its lines read as they come.
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

    fn next(&mut self) -> Value {
        let text = match self.lines.recv_timeout(LINE_WAIT) {
            Ok(text) => text,
            Err(RecvTimeoutError::Timeout) => panic!("{LINE_WAIT:?} 待っても 1 行も来ない"),
            Err(RecvTimeoutError::Disconnected) => panic!("繋ぎが閉じた"),
        };
        serde_json::from_str(&text).unwrap_or_else(|e| panic!("JSON でない行: {text:?}（{e}）"))
    }

    /// The next line of the kind `t`, the ones before it passed over.
    fn next_of(&mut self, t: &str) -> Value {
        loop {
            let line = self.next();
            if line["t"] == t {
                return line;
            }
            assert!(
                line["t"] != "refused" && line["t"] != "failed" && line["t"] != "error",
                "{t} を待つ間に: {line}"
            );
        }
    }
}

/// An agent, by its app's connection.
struct Agent {
    line: Line,
    home: PathBuf,
    build: String,
    /// Where it answers MCP.
    mcp_port: u16,
    next: u64,
    /// Its id in the hub, once it has a key.
    id: String,
}

impl Agent {
    /// A request of the core's, as the window makes it: its result.
    fn core(&mut self, request: Value) -> Value {
        let id = self.next;
        self.next += 1;
        self.line
            .send(&json!({"t": "core", "id": id, "request": request}));
        loop {
            let line = self.line.next();
            if line["t"] == "core" && line["id"] == id {
                let answer: Value = serde_json::from_str(line["answer"].as_str().unwrap()).unwrap();
                assert!(answer.get("error").is_none(), "{request}: {answer}");
                return answer["result"].clone();
            }
        }
    }

    /// A project made here, by the window: its id.
    fn project(&mut self, name: &str) -> String {
        self.core(json!({"op": "tool", "name": "project_create", "arguments": {"project": name}}));
        self.core(json!({"op": "projects"}))["projects"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["name"] == name)
            .unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string()
    }

    /// An access key made here: its id and its value.
    fn key(&mut self, name: &str) -> (String, String) {
        let made = self.core(json!({"op": "access_key_create", "name": name}));
        (
            made["key"]["id"].as_str().unwrap().to_string(),
            made["value"].as_str().unwrap().to_string(),
        )
    }

    /// The link as the app is told it next stands `is`.
    fn until_link(&mut self, is: &str) -> Value {
        loop {
            let told = self.line.next_of("hub");
            if told["link"]["is"] == is {
                return told;
            }
        }
    }

    /// What the agent says of the hub now.
    fn hub(&mut self) -> Value {
        self.line.send(&json!({"t": "hub"}));
        self.line.next_of("hub")
    }

    /// The projects shared with it, as its window lists them.
    fn shared(&mut self) -> Value {
        self.core(json!({"op": "hub_shares"}))["shared"].clone()
    }

    /// A tool called through its MCP with the key `key` (`mcp_at`).
    fn mcp(&self, key: &str, tool: &str, arguments: Value) -> (bool, Value) {
        mcp_at(self.mcp_port, key, tool, arguments)
    }

    fn ok(&self, key: &str, tool: &str, arguments: Value) -> Value {
        let (refused, said) = self.mcp(key, tool, arguments);
        assert!(!refused, "{tool}: {said}");
        said
    }

    fn refused(&self, key: &str, tool: &str, arguments: Value) -> String {
        let (refused, said) = self.mcp(key, tool, arguments);
        assert!(refused, "{tool} が通った: {said}");
        said.as_str().unwrap().to_string()
    }
}

/// A tool called through the MCP an agent answers at `port`, with the key `key`: whether it was
/// refused, and what it said (the refusal's words alone, when refused).
fn mcp_at(port: u16, key: &str, tool: &str, arguments: Value) -> (bool, Value) {
    let body = json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": tool, "arguments": arguments}})
        .to_string();
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(60)))
        .unwrap();
    let head = format!(
        "POST /mcp HTTP/1.1\r\nhost: 127.0.0.1:{}\r\ncontent-type: application/json\r\nauthorization: Bearer {key}\r\nconnection: close\r\ncontent-length: {}\r\n\r\n",
        port,
        body.len()
    );
    stream.write_all(head.as_bytes()).unwrap();
    stream.write_all(body.as_bytes()).unwrap();
    let mut got = Vec::new();
    let mut chunk = [0u8; 8192];
    let said = loop {
        let n = stream.read(&mut chunk).unwrap_or(0);
        got.extend_from_slice(&chunk[..n]);
        let text = String::from_utf8_lossy(&got).into_owned();
        if let Some(end) = text.find("\r\n\r\n") {
            let length = text[..end]
                .to_lowercase()
                .lines()
                .find_map(|l| l.strip_prefix("content-length: ").map(str::to_string))
                .and_then(|l| l.trim().parse::<usize>().ok())
                .unwrap_or(0);
            if got.len() >= end + 4 + length {
                break String::from_utf8_lossy(&got[end + 4..end + 4 + length]).into_owned();
            }
        }
        assert!(n > 0, "答えの途中で閉じた: {text}");
    };
    let reply: Value = serde_json::from_str(&said).unwrap_or_else(|e| panic!("{said:?}: {e}"));
    let result = &reply["result"];
    let text = result["content"][0]["text"].as_str().unwrap_or_default();
    let said: Value = serde_json::from_str(text).unwrap_or_else(|_| text.into());
    if result["isError"] == true {
        return (true, said["error"]["message"].clone());
    }
    assert!(reply.get("error").is_none(), "{reply}");
    (false, said)
}

/// Server and Mac, both members and linked; Server's project Shop shared with Mac at `level`, and
/// Mac's key K given it at `level`: the hub's app, the two agents, Shop's id and K's value.
fn shared_shop(world: &World, level: &str) -> (Line, Agent, Agent, String, String) {
    shared_shop_built(world, level, "", "")
}

fn shared_shop_built(
    world: &World,
    level: &str,
    server_build: &str,
    mac_build: &str,
) -> (Line, Agent, Agent, String, String) {
    let mut hub = world.hub();
    let mut server = world.agent_built("s", server_build);
    let mut mac = world.agent_built("m", mac_build);
    world.join(&mut hub, &mut server, "Server");
    world.join(&mut hub, &mut mac, "Mac");
    let shop = server.project("Shop");
    server.core(json!({"op": "hub_share_set", "project": shop, "member": mac.id, "level": level}));
    // Told over the hub: there once Mac has taken it in.
    until("共有が Mac に届く", 20, || {
        mac.shared().as_array().is_some_and(|s| !s.is_empty())
    });
    let (key, value) = mac.key("K");
    mac.core(json!({"op": "access_grant", "key": key, "owner": server.id, "project": shop, "level": level}));
    (hub, server, mac, shop, value)
}

#[test]
fn a_shared_project_is_read_and_written_from_the_other_agents_mcp() {
    let world = World::new();
    let (_hub, mut server, mac, shop, key) = shared_shop(&world, "write");

    assert_eq!(
        mac.ok(&key, "project_list", json!({}))["projects"],
        json!([{"project": "Shop", "owner": "Server", "access": "write"}])
    );
    // Written at its owner, under the key at its agent.
    let thing = json!({"project": "Shop", "target": "Schemas:Thing", "value": {"kind": "entity", "description": "共有越し"}});
    mac.ok(&key, "card_create", thing);
    let commits = server.core(json!({"op": "commits", "project": shop}));
    assert_eq!(commits["commits"][0]["authorKind"], "mcp", "{commits}");
    assert_eq!(commits["commits"][0]["author"], "K@Mac", "{commits}");
    // Read as its owner's own MCP reads it.
    let (own_key, own_value) = server.key("Own");
    server.core(json!({"op": "access_grant", "key": own_key, "project": shop, "level": "read"}));
    let read = json!({"project": "Shop", "target": "Schemas:Thing"});
    let over_hub = mac.ok(&key, "card_read", read.clone());
    assert_eq!(over_hub["description"], "共有越し", "{over_hub}");
    assert_eq!(over_hub, server.ok(&own_value, "card_read", read));
    // A refusal comes back as the owner said it, with what it says beside its words.
    let (refused, _) = mac.mcp(
        &key,
        "card_read",
        json!({"project": "Shop", "target": "Schemas:Nope"}),
    );
    assert!(refused);
    // What touches the owner's folders is not done over the hub.
    let said = mac.refused(
        &key,
        "export_execute",
        json!({"project": "Shop", "targetdir": "/tmp/nowhere"}),
    );
    assert!(said.contains("over the hub"), "{said}");
    // Many at once, each answered with its own.
    std::thread::scope(|s| {
        let lists: Vec<_> = (0..8)
            .map(|n| {
                let (port, key) = (mac.mcp_port, &key);
                s.spawn(move || {
                    let target = format!("Schemas:Many{n}");
                    let (refused, said) = mcp_at(
                        port,
                        key,
                        "card_create",
                        json!({"project": "Shop", "target": target, "value": {"kind": "entity"}}),
                    );
                    assert!(!refused, "{said}");
                    let (refused, read) = mcp_at(
                        port,
                        key,
                        "card_read",
                        json!({"project": "Shop", "target": target}),
                    );
                    assert!(!refused, "{read}");
                    read
                })
            })
            .collect();
        for (n, list) in lists.into_iter().enumerate() {
            let read = list.join().unwrap();
            assert_eq!(read["path"], format!("Schemas:Many{n}"), "{read}");
        }
    });
}

#[test]
fn joining_makes_a_key_and_an_id_and_leaving_discards_them() {
    let world = World::new();
    let mut hub = world.hub();
    let mut mac = world.agent("m");
    let place = mac.home.join("hub");

    // Not joined: nothing kept, nothing running. What is asked with a number is answered with it.
    mac.line.send(&json!({"t": "hub", "id": 7}));
    assert_eq!(mac.line.next_of("hub")["re"], 7);
    let told = mac.hub();
    assert!(told.get("re").is_none(), "{told}");
    assert_eq!(told["joined"], false, "{told}");
    assert_eq!(told["link"], json!({"is": "off"}), "{told}");
    assert!(!place.exists());

    // A key and an id, made here: the private half never leaves this host.
    mac.line.send(&json!({"t": "hub_key"}));
    let made = mac.line.next_of("hub_key");
    let (id, key) = (made["id"].as_str().unwrap(), made["key"].as_str().unwrap());
    assert!(key.starts_with("ssh-ed25519 "), "{made}");
    assert_eq!(made.as_object().unwrap().len(), 3, "{made}");
    mac.line.send(&json!({"t": "hub_key", "id": 8}));
    assert_eq!(mac.line.next_of("hub_key")["re"], 8);
    let mode = |name: &str| {
        std::fs::metadata(place.join(name))
            .unwrap()
            .permissions()
            .mode()
            & 0o777
    };
    assert_eq!(mode("key"), 0o600);
    assert!(
        std::fs::read_to_string(place.join("key.pub"))
            .unwrap()
            .starts_with(key)
    );
    let member: Value =
        serde_json::from_str(&std::fs::read_to_string(place.join("member.json")).unwrap()).unwrap();
    assert_eq!(member, json!({"id": id}));
    // Asked again, the ones it has.
    mac.line.send(&json!({"t": "hub_key"}));
    assert_eq!(mac.line.next_of("hub_key"), made);
    assert!(world.ssh_calls().is_empty());

    // Joined: where the hub is and the one host it is known by are kept, and the link is made.
    hub.send(&json!({"t": "enroll", "id": id, "name": "Mac", "key": key}));
    let enrolled = hub.next_of("enrolled");
    mac.line
        .send(&json!({"t": "hub_join", "to": HUB_AT, "hostKeys": enrolled["hostKeys"]}));
    let told = mac.until_link("up");
    assert_eq!(told["joined"], true, "{told}");
    assert_eq!(told["id"], id);
    assert_eq!(told["to"], HUB_AT);
    assert!(told["link"]["since"].is_u64(), "{told}");
    assert!(told["members"][0]["since"].is_u64(), "{told}");
    let mut members = told["members"].clone();
    for of_the_link in ["since", "link"] {
        members[0].as_object_mut().unwrap().remove(of_the_link);
    }
    assert_eq!(members, json!([{"id": id, "name": "Mac", "online": true}]));
    let member: Value =
        serde_json::from_str(&std::fs::read_to_string(place.join("member.json")).unwrap()).unwrap();
    assert_eq!(member, json!({"id": id, "to": HUB_AT}));
    let known = std::fs::read_to_string(place.join("known_hosts")).unwrap();
    assert_eq!(known, format!("specboard-hub {HOST_KEY}\n"));
    // The host's ssh, with this key alone, the hub's host known by what the hub handed over and
    // nothing of the user's own.
    let calls = world.ssh_calls();
    assert_eq!(calls.len(), 1, "{calls:?}");
    for part in [
        format!("-i {}", place.join("key").display()),
        format!("UserKnownHostsFile={}", place.join("known_hosts").display()),
        "GlobalKnownHostsFile=/dev/null".to_string(),
        "StrictHostKeyChecking=yes".to_string(),
        "HostKeyAlias=specboard-hub".to_string(),
        "IdentitiesOnly=yes".to_string(),
        "BatchMode=yes".to_string(),
        "ControlMaster=no".to_string(),
        "ControlPath=none".to_string(),
        "ServerAliveInterval=15".to_string(),
        format!("-- {HUB_AT}"),
    ] {
        assert!(calls[0].contains(&part), "{part}: {}", calls[0]);
    }
    assert!(calls[0].ends_with(HUB_AT), "{}", calls[0]);
    assert_eq!(world.ssh_alive().len(), 1);

    // A destination ssh would read as an option is not joined.
    mac.line.send(
        &json!({"t": "hub_join", "id": 9, "to": "-oProxyCommand=id", "hostKeys": enrolled["hostKeys"]}),
    );
    let told = mac.line.next_of("hub");
    assert!(told["refused"].is_string(), "{told}");
    assert_eq!(told["re"], 9, "{told}");
    assert_eq!(told["to"], HUB_AT, "{told}");

    // Left: nothing of it is kept, the link is let go, and what the core kept of the hub goes.
    mac.line.send(&json!({"t": "hub_leave"}));
    let told = mac.until_link("off");
    assert_eq!(told["joined"], false, "{told}");
    assert_eq!(told["members"], json!([]));
    assert!(
        told.get("id").is_none() && told.get("to").is_none(),
        "{told}"
    );
    assert!(!place.exists() || std::fs::read_dir(&place).unwrap().next().is_none());
    until("ssh が止まる", 10, || world.ssh_alive().is_empty());
    loop {
        let told = hub.next_of("members");
        if told["members"][0]["online"] == false {
            break;
        }
    }
    assert_eq!(
        mac.core(json!({"op": "hub_shares"})),
        json!({"members": [], "shares": [], "shared": []})
    );

    // Joined again, it is another member: what was shared with the one before is not its own.
    mac.line.send(&json!({"t": "hub_key"}));
    let again = mac.line.next_of("hub_key");
    assert_ne!(again["id"], id);
    assert_ne!(again["key"], key);
}

#[test]
fn the_link_is_put_back_when_it_drops_and_apps_are_told() {
    let world = World::new();
    let mut hub = world.hub();
    let mut mac = world.agent("m");
    world.join(&mut hub, &mut mac, "Mac");
    assert_eq!(world.ssh_alive().len(), 1);

    // The hub gone: said, with why and when it is tried next.
    drop(hub);
    assert_eq!(
        world
            .hub_command(&["hub", "stop"])
            .output()
            .unwrap()
            .status
            .code(),
        Some(0)
    );
    let told = mac.until_link("down");
    assert!(told["link"]["why"].is_string(), "{told}");
    assert!(
        told["link"]["retryAt"].as_u64() >= told["link"]["at"].as_u64(),
        "{told}"
    );
    assert_eq!(told["joined"], true, "{told}");
    // Not linked, the members are not known: none is said to be there.
    assert!(
        told["members"]
            .as_array()
            .unwrap()
            .iter()
            .all(|m| m["online"] == false),
        "{told}"
    );

    // Back: linked again without anyone asking.
    let _hub = world.hub();
    let told = mac.until_link("up");
    assert_eq!(told["members"][0]["online"], true, "{told}");

    // The agent stopped: its ssh goes with it.
    let (home, build) = (mac.home.clone(), mac.build.clone());
    drop(mac);
    let stopped = agent_command(&world.root, &home, &build, &["stop"])
        .output()
        .unwrap();
    assert_eq!(stopped.status.code(), Some(0));
    until("ssh が止まる", 10, || world.ssh_alive().is_empty());
}

#[test]
fn an_owner_away_is_said_without_waiting() {
    let world = World::new();
    let (_hub, server, mut mac, _shop, key) = shared_shop(&world, "write");
    // Reached while its owner is there.
    mac.ok(&key, "card_list", json!({"project": "Shop"}));

    let (home, build) = (server.home.clone(), server.build.clone());
    drop(server);
    let stopped = agent_command(&world.root, &home, &build, &["stop"])
        .output()
        .unwrap();
    assert_eq!(stopped.status.code(), Some(0));
    until("Server が離れたと Mac が知る", 20, || {
        mac.hub()["members"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m["name"] == "Server" && m["online"] == false)
    });

    let away = "Server is not linked to the hub now";
    let asked = Instant::now();
    assert_eq!(
        mac.refused(&key, "card_list", json!({"project": "Shop"})),
        away
    );
    assert!(
        asked.elapsed() < Duration::from_secs(2),
        "{:?}",
        asked.elapsed()
    );
    // Still listed, with why it cannot be reached; what the key was given stays.
    assert_eq!(
        mac.ok(&key, "project_list", json!({}))["projects"],
        json!([{"project": "Shop", "owner": "Server", "access": "write", "reachable": false, "why": away}])
    );
}

#[test]
fn agents_of_different_builds_do_not_hand_calls_on() {
    let world = World::new();
    let (mut hub, mut server, mac, shop, key) =
        shared_shop_built(&world, "write", "202611100001", "202611100002");

    // The side that uses it does not hand the call on.
    let other = "Server is running another version (connecting the app to both agents brings them to the same one)";
    assert_eq!(
        mac.refused(&key, "card_list", json!({"project": "Shop"})),
        other
    );
    assert_eq!(
        mac.ok(&key, "project_list", json!({}))["projects"],
        json!([{"project": "Shop", "owner": "Server", "access": "write", "reachable": false, "why": other}])
    );

    // Nor does its owner run one handed to it all the same: a member that says another build.
    let pretender = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIPretenderPretenderPretender";
    hub.send(&json!({"t": "enroll", "id": "x", "name": "Other", "key": pretender}));
    hub.next_of("enrolled");
    until("Server が名簿の x を知る", 20, || {
        server.core(json!({"op": "hub_shares"}))["members"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m["id"] == "x")
    });
    server.core(json!({"op": "hub_share_set", "project": shop, "member": "x", "level": "write"}));
    let mut x = Line::open(world.hub_command(&["hub", "attach", "--as", "x"]));
    x.send(&json!({"t": "hello", "protocol": 1, "build": "202611100002"}));
    x.next_of("ready");
    let server_id = server.id.clone();
    let call = |id: u64, build: &str, name: &str| {
        json!({"t": "to", "to": server_id, "id": id, "body": {
            "k": "call",
            "tool": "card_create",
            "arguments": {"project": shop, "target": format!("Schemas:{name}"), "value": {"kind": "entity"}},
            "key": "K",
            "level": "write",
            "build": build,
            "lang": "en",
        }})
    };
    x.send(&call(1, "202611100002", "Other"));
    let said = loop {
        let line = x.next_of("from");
        if line["re"] == 1 {
            break line;
        }
    };
    assert_eq!(said["from"], server_id);
    assert!(
        said["body"]["error"]
            .as_str()
            .is_some_and(|e| e.contains("another version")),
        "{said}"
    );
    // One of its own build is run: only the build held the other back.
    x.send(&call(2, "202611100001", "Same"));
    let said = loop {
        let line = x.next_of("from");
        if line["re"] == 2 {
            break line;
        }
    };
    assert!(said["body"]["output"].is_object(), "{said}");
    let (own_key, own_value) = server.key("Own");
    server.core(json!({"op": "access_grant", "key": own_key, "project": shop, "level": "read"}));
    let cards = server
        .ok(&own_value, "card_list", json!({"project": "Shop"}))
        .to_string();
    assert!(cards.contains("Schemas:Same"), "{cards}");
    assert!(!cards.contains("Schemas:Other"), "{cards}");
}

#[test]
fn the_shared_list_follows_the_shares_and_the_names() {
    let world = World::new();
    let (_hub, mut server, mut mac, shop, _key) = shared_shop(&world, "read");
    let (server_id, mac_id) = (server.id.clone(), mac.id.clone());
    let listed = |name: &str| json!([{"owner": server_id, "ownerName": "Server", "project": shop, "name": name, "level": "read", "reachable": true}]);
    assert_eq!(mac.shared(), listed("Shop"));
    let granted =
        |mac: &mut Agent| mac.core(json!({"op": "access_keys"}))["keys"][0]["hubGrants"].clone();
    assert_eq!(
        granted(&mut mac),
        json!([{"owner": server_id, "project": shop, "level": "read"}])
    );

    // Renamed at its owner: called so here too, and what the key was given stays.
    server.core(json!({"op": "project_rename", "project": shop, "name": "Store"}));
    until("名前が Mac に届く", 20, || {
        mac.shared() == listed("Store")
    });
    assert_eq!(
        granted(&mut mac),
        json!([{"owner": server_id, "project": shop, "level": "read"}])
    );

    // Shared no more: gone here, with what the key was given.
    server.core(json!({"op": "hub_share_set", "project": shop, "member": mac_id, "level": null}));
    until("共有が Mac から消える", 20, || {
        mac.shared() == json!([])
    });
    assert_eq!(granted(&mut mac), json!([]));

    // Shared again while Mac is away: told when it is back.
    let (home, build) = (mac.home.clone(), mac.build.clone());
    drop(mac);
    assert_eq!(
        agent_command(&world.root, &home, &build, &["stop"])
            .output()
            .unwrap()
            .status
            .code(),
        Some(0)
    );
    until("Mac が離れたと Server が知る", 20, || {
        server.core(json!({"op": "hub_shares"}))["members"][0]["online"] == false
    });
    server.core(json!({"op": "hub_share_set", "project": shop, "member": mac_id, "level": "read"}));
    let mut mac = world.agent("m");
    until("共有が戻った Mac に届く", 30, || {
        mac.shared() == listed("Store")
    });
}

/// Mac, a member and linked, and Server, a member the test speaks for, which told Mac it shares
/// Shop (`shop-srv`) with it; Mac's key K given it to read: the hub's app, Mac, Server's
/// connection and K's value.
fn shop_of_a_spoken_owner(world: &World) -> (Line, Agent, Line, String) {
    let mut hub = world.hub();
    let mut mac = world.agent("m");
    world.join(&mut hub, &mut mac, "Mac");
    let mut server = world.member(&mut hub, "srv", "Server");
    let shares = json!({"k": "shares", "projects": [{"id": "shop-srv", "name": "Shop", "slug": "shop", "level": "read"}]});
    server.send(&json!({"t": "to", "to": mac.id, "body": shares}));
    until("共有が Mac に届く", 20, || {
        mac.shared().as_array().is_some_and(|s| !s.is_empty())
    });
    let (key, value) = mac.key("K");
    mac.core(json!({"op": "access_grant", "key": key, "owner": "srv", "project": "shop-srv", "level": "read"}));
    (hub, mac, server, value)
}

/// The next call a member the test speaks for is handed (what else members tell it passed over).
fn next_call(line: &mut Line) -> Value {
    loop {
        let said = line.next_of("from");
        if said["body"]["k"] == "call" {
            return said;
        }
    }
}

#[test]
fn a_call_is_answered_by_its_owner_alone() {
    let world = World::new();
    let (mut hub, mac, mut server, key) = shop_of_a_spoken_owner(&world);
    let mut other = world.member(&mut hub, "oth", "Other");
    let (port, to) = (mac.mcp_port, mac.id.clone());

    std::thread::scope(|scope| {
        let read = scope.spawn(|| mcp_at(port, &key, "card_list", json!({"project": "Shop"})));
        let asked = next_call(&mut server);
        assert_eq!(asked["from"], to, "{asked}");
        assert_eq!(asked["body"]["tool"], "card_list", "{asked}");
        // Another member answers first, by the call's number — and the hub has read it before the
        // owner answers.
        other.send(&json!({"t": "to", "to": to, "re": asked["id"], "body": {"output": {"json": {"from": "another"}}}}));
        other.send(&json!({"t": "to", "to": "nobody", "id": 1, "body": {}}));
        assert_eq!(other.next_of("undelivered")["id"], 1);
        server.send(&json!({"t": "to", "to": to, "re": asked["id"], "body": {"output": {"json": {"from": "owner"}}}}));
        let (refused, said) = read.join().unwrap();
        assert!(!refused, "{said}");
        assert_eq!(said, json!({"from": "owner"}));
    });
}

#[test]
fn a_calls_number_is_not_one_an_earlier_run_of_the_agent_used() {
    let world = World::new();
    let (_hub, mac, mut server, key) = shop_of_a_spoken_owner(&world);
    let to = mac.id.clone();
    // A call asked, and what its owner then read of it.
    let asked = |port: u16, server: &mut Line| -> Value {
        std::thread::scope(|scope| {
            let read = scope.spawn(|| mcp_at(port, &key, "card_list", json!({"project": "Shop"})));
            let asked = next_call(server);
            server.send(
                &json!({"t": "to", "to": to, "re": asked["id"], "body": {"output": {"json": {}}}}),
            );
            let (refused, said) = read.join().unwrap();
            assert!(!refused, "{said}");
            asked
        })
    };
    let first = asked(mac.mcp_port, &mut server);

    // The agent stopped and started again. What its owner answers late to a call of the run
    // before must not be taken for the answer to a call of this one: no number comes twice.
    let (home, build) = (mac.home.clone(), mac.build.clone());
    drop(mac);
    let stopped = agent_command(&world.root, &home, &build, &["stop"])
        .output()
        .unwrap();
    assert_eq!(stopped.status.code(), Some(0));
    let mut mac = world.agent("m");
    until("Mac がハブに繋ぎ直す", 30, || {
        mac.hub()["link"]["is"] == "up"
    });
    let second = asked(mac.mcp_port, &mut server);
    assert!(
        second["id"].as_u64() > first["id"].as_u64(),
        "{} のあとに {}",
        first["id"],
        second["id"]
    );
}

#[test]
fn a_member_in_a_new_connection_is_told_the_shares_again() {
    let world = World::new();
    let mut hub = world.hub();
    let mut server = world.agent("s");
    world.join(&mut hub, &mut server, "Server");
    let shop = server.project("Shop");
    let mut first = world.member(&mut hub, "mac", "Mac");
    until("Server が名簿の mac を知る", 20, || {
        server.core(json!({"op": "hub_shares"}))["members"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m["id"] == "mac" && m["online"] == true)
    });
    server.core(json!({"op": "hub_share_set", "project": shop, "member": "mac", "level": "read"}));
    let told = |line: &mut Line| -> Value {
        loop {
            let said = line.next_of("from");
            if said["body"]["k"] == "shares" && said["body"]["projects"] != json!([]) {
                return said["body"]["projects"].clone();
            }
        }
    };
    let shared = told(&mut first);
    assert_eq!(shared[0]["id"], shop.as_str(), "{shared}");

    // Its connection replaced by another, the moment after — as when the first was half gone and
    // what was said to it never read: what is shared with it is said to the new one.
    let mut second = world.member_again("mac");
    assert_eq!(told(&mut second), shared);
}

#[test]
fn forwards_left_in_the_settings_are_not_read() {
    let world = World::new();
    // A setting an agent of before kept: a forward to another host's agent, on.
    let home = world.home_of("m", "");
    let kept = json!({"mcpPort": 0, "forwards": [{"name": "hostA", "to": "someone@hostA", "port": 18788, "on": true}]});
    std::fs::write(home.join("agent.json"), kept.to_string()).unwrap();
    let mut mac = world.agent("m");

    // Its line is no line of the agent's any more, and its command no command.
    mac.line.send(&json!({"t": "forwards"}));
    let said = mac.line.next();
    assert_eq!(said["t"], "error", "{said}");
    let out = agent_command(&world.root, &home, "", &["forward", "list"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("specboard-agent <"),
        "{out:?}"
    );
    // No ssh was run for it — the agent has been up for as long as the lines above took, and a
    // little more.
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(world.ssh_calls(), Vec::<String>::new());
    // The setting is left as it was: what is not read is not written over either.
    let now: Value =
        serde_json::from_str(&std::fs::read_to_string(home.join("agent.json")).unwrap()).unwrap();
    assert_eq!(now["forwards"], kept["forwards"]);
}
