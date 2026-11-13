//! The hub's own (`hub serve`): one a user, in its place (`~/.specboard-hub/`, or
//! `SPECBOARD_HUB_HOME`) — who its members are (`members.json`), the socket it is reached at
//! (`hub.sock`, the user's alone), the lock that keeps it one (`hub.lock`), what it says of itself
//! (`hub.log`), and its own name for the build running (`specboard-agent`, a link the members' keys
//! run). A connection speaks one JSON object a line, and is one of two kinds by its first line.
//!
//! A member's (`hub attach --as <id>` writes `{"t":"as","id":…}` first, from the id its key's
//! command names; the agent's lines follow):
//!
//! - `{"t":"hello","protocol":n,"build":…}` → `{"t":"ready","protocol":n,"id":…,"members":[…]}`, or
//!   `{"t":"refused","message":…}` and the connection closed: not a member, or lines of another form
//! - `{"t":"members","members":[{id, name, online, build?, since?, link?, seen?}]}`: who is
//!   there, each time it changes — since when (`since`) and by which of its connections (`link`:
//!   another number for each, so that what was said to the one before is said again to this), or
//!   when one that is away was last there (`seen`); the times in seconds since 1970
//! - `{"t":"to","to":…,"id"?:…,"re"?:…,"body":…}` → the member `to` reads `{"t":"from","from":…,
//!   "id"?,"re"?,"body":…}`: `from` is who this connection is, whatever the line says; `body` is
//!   handed on as it was written, not read. `id` numbers a call, `re` answers one: only a call
//!   this connection was handed and has not answered — an answer to any other is let go of, so
//!   that no member answers for another
//! - `{"t":"undelivered","id":…,"why":…}`: a call (`id`) that was not answered and will not be —
//!   `unknown` (no such member), `offline` (not linked), `cut` (handed to it, and its connection
//!   went before it answered: whether it was done is not known). What waits for no answer (no `id`)
//!   is let go of
//! - `{"t":"ping"}` → `{"t":"pong"}`: asked every 15 seconds (`SPECBOARD_HUB_PING`, in seconds);
//!   a connection nothing came from for three times that is closed
//!
//! One member, one connection: a second of the same id closes the one before, so that a
//! connection half gone is not left to take what is sent.
//!
//! The app's (`hub attach`; anything else first):
//!
//! - `{"t":"who"}` → `{"t":"hub","pid":…,"exe":…,"build":…}` (`attach` asks which build runs)
//! - `{"t":"hello"}` → `{"t":"ready","protocol":n,"build":…,"members":[…]}`, then `members` as they
//!   change
//! - `{"t":"enroll","id":…,"name":…,"key":…}` → `{"t":"enrolled","id":…,"hostKeys":[…],"members":[…]}`:
//!   a member added, with the line of `authorized_keys` that lets its key reach the hub; the host's
//!   public host keys are what its agent is to know this host by
//! - `{"t":"remove","id":…}` → `{"t":"removed","id":…,"members":[…]}`: gone from both, its
//!   connection closed
//! - `{"t":"rename","id":…,"name":…}` → `{"t":"renamed","id":…,"members":[…]}`
//! - each of the three answers with who the members are then: who asked need not wait to be told
//! - `{"t":"stop"}`: it ends
//!
//! What it cannot do is answered `{"t":"refused","message":…}`.

use super::members::{self, Member};
use serde::Deserialize;
use serde_json::value::RawValue;
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// The longest a line may be: one longer closes the connection it came by.
const LINE_MOST: u64 = 64 << 20;

/// The most that may wait to be written to one connection: one that takes none of it is closed
/// rather than kept in memory without end.
const WAITING_MOST: usize = 256 << 20;

/// The longest one write to a connection may make no way before it is taken to be gone.
const WRITE_WAIT: Duration = Duration::from_secs(30);

/// How often a member is asked whether it is there, when `SPECBOARD_HUB_PING` says nothing.
const PING: Duration = Duration::from_secs(15);

/// One line as any connection may send it: what the hub reads of it. `body` is kept as it was
/// written, to be handed on untouched, and so are the numbers of a call.
#[derive(Deserialize)]
struct Line<'a> {
    t: String,
    to: Option<String>,
    #[serde(borrow)]
    id: Option<&'a RawValue>,
    #[serde(borrow)]
    re: Option<&'a RawValue>,
    #[serde(borrow)]
    body: Option<&'a RawValue>,
    protocol: Option<u64>,
    build: Option<String>,
    name: Option<String>,
    key: Option<String>,
}

impl Line<'_> {
    /// `id` when it is a text (a member's id, in the app's lines).
    fn id_text(&self) -> Option<String> {
        serde_json::from_str::<String>(self.id?.get()).ok()
    }
}

/// The way back to a connection: lines are written by a thread of its own, in the order they were
/// said, so that one that reads slowly keeps no other waiting.
struct Out {
    lines: Mutex<Option<Sender<String>>>,
    waiting: Arc<AtomicUsize>,
    stream: UnixStream,
}

impl Out {
    fn new(stream: UnixStream) -> Arc<Out> {
        let (lines, said) = channel::<String>();
        let waiting = Arc::new(AtomicUsize::new(0));
        let out = Arc::new(Out {
            lines: Mutex::new(Some(lines)),
            waiting: waiting.clone(),
            stream,
        });
        if let Ok(mut writer) = out.stream.try_clone() {
            let _ = writer.set_write_timeout(Some(WRITE_WAIT));
            std::thread::spawn(move || {
                for line in said {
                    let written = writer
                        .write_all(line.as_bytes())
                        .and_then(|()| writer.write_all(b"\n"))
                        .and_then(|()| writer.flush());
                    waiting.fetch_sub(line.len(), Ordering::SeqCst);
                    if written.is_err() {
                        break;
                    }
                }
                // Let go of (`close`) with what was said written, or gone, or not reading: closed
                // both ways, and its own thread ends with it.
                let _ = writer.shutdown(std::net::Shutdown::Both);
            });
        }
        out
    }

    /// One line to it; false when it is gone, or takes nothing of what waits for it.
    fn say(&self, line: String) -> bool {
        let lines = self.lines.lock().unwrap_or_else(|e| e.into_inner());
        let Some(sender) = lines.as_ref() else {
            return false;
        };
        if self.waiting.fetch_add(line.len(), Ordering::SeqCst) + line.len() > WAITING_MOST {
            drop(lines);
            self.close();
            return false;
        }
        sender.send(line).is_ok()
    }

    fn tell(&self, message: &Value) -> bool {
        self.say(message.to_string())
    }

    /// Let go of: nothing more is read from it, so its own thread ends, and it is closed once
    /// what was said to it is written — why it is turned away is the last thing it reads.
    fn close(&self) {
        *self.lines.lock().unwrap_or_else(|e| e.into_inner()) = None;
        let _ = self.stream.shutdown(std::net::Shutdown::Read);
    }
}

/// A member's connection.
struct Linked {
    /// Told apart from the one before and the one after of the same member.
    n: u64,
    out: Arc<Out>,
    build: Option<String>,
    /// When it was let in, in seconds since 1970.
    since: u64,
    /// When anything last came from it.
    heard: Mutex<Instant>,
    /// The calls handed to it that it has not answered: who sent each, and its number as written.
    /// Told to be lost when this connection goes.
    owes: Mutex<HashSet<(String, String)>>,
    /// Let go of already (`Hub::unlinked`).
    gone: AtomicBool,
}

struct Hub {
    home: PathBuf,
    members: Mutex<Vec<Member>>,
    linked: Mutex<HashMap<String, Arc<Linked>>>,
    /// The apps' connections that said hello: told who is there.
    apps: Mutex<Vec<(u64, Arc<Out>)>>,
    next: AtomicU64,
    ping: Duration,
}

fn refused(message: String) -> Value {
    json!({"t": "refused", "message": message})
}

/// One line of `reader`, without its end; false when the connection ended, or the line is longer
/// than a line may be.
fn read_line(reader: &mut BufReader<UnixStream>, line: &mut Vec<u8>) -> bool {
    line.clear();
    match reader.by_ref().take(LINE_MOST + 1).read_until(b'\n', line) {
        Ok(n) if n > 0 && line.last() == Some(&b'\n') => {
            line.pop();
            true
        }
        _ => false,
    }
}

impl Hub {
    fn locked_members(&self) -> std::sync::MutexGuard<'_, Vec<Member>> {
        self.members.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn locked_links(&self) -> std::sync::MutexGuard<'_, HashMap<String, Arc<Linked>>> {
        self.linked.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Who the members are and which are linked, as every connection is told it.
    fn said(&self) -> Value {
        let members = self.locked_members().clone();
        let linked = self.locked_links();
        Value::Array(
            members
                .iter()
                .map(|m| {
                    let mut one = json!({"id": m.id, "name": m.name, "online": false});
                    if let Some(link) = linked.get(&m.id) {
                        one["online"] = true.into();
                        one["since"] = link.since.into();
                        one["link"] = link.n.into();
                        if let Some(build) = &link.build {
                            one["build"] = build.clone().into();
                        }
                    } else if let Some(seen) = m.seen {
                        one["seen"] = seen.into();
                    }
                    one
                })
                .collect(),
        )
    }

    /// Every connection told who is there now, but the member's numbered `but` (told it as it was
    /// let in).
    fn tell_all(&self, but: Option<u64>) {
        let line = json!({"t": "members", "members": self.said()}).to_string();
        let links: Vec<Arc<Linked>> = self.locked_links().values().cloned().collect();
        for link in links {
            if Some(link.n) != but {
                link.out.say(line.clone());
            }
        }
        let apps: Vec<Arc<Out>> = self
            .apps
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .map(|(_, out)| out.clone())
            .collect();
        for app in apps {
            app.say(line.clone());
        }
    }

    /// A connection of a member's that went, or was put out: let go of if it is still the member's
    /// one, and each call it had not answered said to be lost to who sent it. Once: one put out
    /// comes here again as its own thread ends, when what its member sent is another connection's
    /// already.
    fn unlinked(&self, id: &str, link: &Arc<Linked>) {
        if link.gone.swap(true, Ordering::SeqCst) {
            return;
        }
        let was_the_one = {
            let mut linked = self.locked_links();
            let same = linked.get(id).is_some_and(|now| now.n == link.n);
            if same {
                linked.remove(id);
            }
            same
        };
        link.out.close();
        let owed: Vec<(String, String)> = link
            .owes
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .drain()
            .collect();
        for (from, call) in owed {
            let sender = self.locked_links().get(&from).cloned();
            if let Some(sender) = sender {
                sender
                    .out
                    .say(format!(r#"{{"t":"undelivered","id":{call},"why":"cut"}}"#));
            }
        }
        // What it sent and was not answered is nobody's to answer any more.
        let others: Vec<Arc<Linked>> = self.locked_links().values().cloned().collect();
        for other in others {
            other
                .owes
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .retain(|(from, _)| from != id);
        }
        if was_the_one {
            // When it was last there, kept with the members: said for as long as it stays away.
            {
                let mut members = self.locked_members();
                if let Some(member) = members.iter_mut().find(|m| m.id == id) {
                    member.seen = Some(members::seconds());
                    let _ = members::save(&self.home, &members);
                }
            }
            self.tell_all(None);
        }
    }

    /// One connection, answered until it goes.
    fn answer(self: &Arc<Self>, connection: UnixStream) {
        let Ok(stream) = connection.try_clone() else {
            return;
        };
        let out = Out::new(stream);
        let mut reader = BufReader::new(connection);
        let mut line = Vec::new();
        if !read_line(&mut reader, &mut line) {
            return;
        }
        // A member's begins with who it is, written by `hub attach --as` from its key's command.
        let member = serde_json::from_slice::<Line>(&line)
            .ok()
            .filter(|first| first.t == "as")
            .map(|first| first.id_text().unwrap_or_default());
        match member {
            Some(id) => self.member(&id, &out, &mut reader),
            None => self.app(&out, &mut reader, line),
        }
        out.close();
    }

    /// A member's connection: let in when it is a member and speaks this form of lines, then its
    /// lines handed on until it goes.
    fn member(self: &Arc<Self>, id: &str, out: &Arc<Out>, reader: &mut BufReader<UnixStream>) {
        let is_member = |hub: &Hub| hub.locked_members().iter().any(|m| m.id == id);
        if !is_member(self) {
            out.tell(&refused(specboard_core::text!("agent.hub.notMember", id)));
            return;
        }
        let mut line = Vec::new();
        if !read_line(reader, &mut line) {
            return;
        }
        let hello: Option<Line> = serde_json::from_slice(&line).ok();
        let Some(hello) = hello.filter(|l| l.t == "hello") else {
            out.tell(&refused(specboard_core::text!("agent.hub.helloFirst")));
            return;
        };
        let theirs = hello.protocol.unwrap_or(0);
        if theirs != super::PROTOCOL {
            let (hub, agent) = (super::PROTOCOL, theirs);
            out.tell(&refused(if theirs > super::PROTOCOL {
                specboard_core::text!("agent.hub.hubOlder", hub, agent)
            } else {
                specboard_core::text!("agent.hub.agentOlder", hub, agent)
            }));
            return;
        }
        let link = Arc::new(Linked {
            n: self.next.fetch_add(1, Ordering::SeqCst),
            out: out.clone(),
            build: hello.build.filter(|b| !b.is_empty()),
            since: members::seconds(),
            heard: Mutex::new(Instant::now()),
            owes: Mutex::new(HashSet::new()),
            gone: AtomicBool::new(false),
        });
        // Still a member, held to the members while it is let in: one removed meanwhile is not.
        let before = {
            let members = self.locked_members();
            if !members.iter().any(|m| m.id == id) {
                drop(members);
                out.tell(&refused(specboard_core::text!("agent.hub.notMember", id)));
                return;
            }
            self.locked_links().insert(id.to_string(), link.clone())
        };
        if let Some(before) = before {
            self.unlinked(id, &before);
        }
        out.tell(&json!({
            "t": "ready",
            "protocol": super::PROTOCOL,
            "id": id,
            "members": self.said(),
        }));
        self.tell_all(Some(link.n));
        while read_line(reader, &mut line) {
            *link.heard.lock().unwrap_or_else(|e| e.into_inner()) = Instant::now();
            let Ok(said) = serde_json::from_slice::<Line>(&line) else {
                out.tell(&json!({"t": "error", "message": specboard_core::text!("agent.hub.unreadable")}));
                continue;
            };
            match said.t.as_str() {
                "pong" => {}
                "ping" => {
                    out.say(r#"{"t":"pong"}"#.to_string());
                }
                "to" => self.hand_on(id, &link, &said),
                other => {
                    out.tell(&json!({
                        "t": "error",
                        "message": specboard_core::text!("agent.hub.notAMembers", line = other),
                    }));
                }
            }
        }
        self.unlinked(id, &link);
    }

    /// A member's line handed to the member it names, written as from the member the connection
    /// is — the body as it came. One that cannot be handed on is said so when it is a call; an
    /// answer, and what waits for none, is let go of. So is an answer to a call this connection
    /// was not handed, or has answered: a call is answered by the one it went to, once.
    fn hand_on(&self, from: &str, link: &Arc<Linked>, said: &Line) {
        let to = said.to.as_deref().unwrap_or("");
        let call = said.id.map(|id| id.get().to_string());
        if let Some(re) = said.re {
            let owed = link
                .owes
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .remove(&(to.to_string(), re.get().to_string()));
            if !owed {
                return;
            }
        }
        let undelivered = |why: &str| {
            if let Some(call) = &call {
                link.out.say(format!(
                    r#"{{"t":"undelivered","id":{call},"why":"{why}"}}"#
                ));
            }
        };
        let target = self.locked_links().get(to).cloned();
        let Some(target) = target else {
            let member = self.locked_members().iter().any(|m| m.id == to);
            undelivered(if member { "offline" } else { "unknown" });
            return;
        };
        let mut line = format!(r#"{{"t":"from","from":{}"#, Value::String(from.to_string()));
        if let Some(call) = &call {
            line.push_str(&format!(r#","id":{call}"#));
            target
                .owes
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .insert((from.to_string(), call.clone()));
        }
        if let Some(re) = said.re {
            line.push_str(&format!(r#","re":{}"#, re.get()));
        }
        line.push_str(&format!(
            r#","body":{}}}"#,
            said.body.map_or("null", RawValue::get)
        ));
        if !target.out.say(line) {
            // Gone as this was said: its own thread says what it owed is lost.
            if let Some(call) = &call
                && target
                    .owes
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .remove(&(from.to_string(), call.clone()))
            {
                undelivered("offline");
            }
        }
    }

    /// The app's connection: `first` is the line it began with.
    fn app(self: &Arc<Self>, out: &Arc<Out>, reader: &mut BufReader<UnixStream>, first: Vec<u8>) {
        let mut line = first;
        let mut told = None;
        loop {
            match serde_json::from_slice::<Line>(&line) {
                Ok(said) => self.app_line(&said, out, &mut told),
                Err(_) => {
                    out.tell(&json!({"t": "error", "message": specboard_core::text!("agent.hub.unreadable")}));
                }
            }
            if !read_line(reader, &mut line) {
                break;
            }
        }
        if let Some(n) = told {
            self.apps
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .retain(|(app, _)| *app != n);
        }
    }

    fn app_line(self: &Arc<Self>, said: &Line, out: &Arc<Out>, told: &mut Option<u64>) {
        match said.t.as_str() {
            "who" => {
                let mut me = crate::daemon::me();
                me["t"] = "hub".into();
                out.tell(&me);
            }
            "hello" => {
                if told.is_none() {
                    let n = self.next.fetch_add(1, Ordering::SeqCst);
                    self.apps
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .push((n, out.clone()));
                    *told = Some(n);
                }
                out.tell(&json!({
                    "t": "ready",
                    "protocol": super::PROTOCOL,
                    "build": crate::daemon::me()["build"],
                    "members": self.said(),
                }));
            }
            "enroll" => {
                let id = said.id_text().unwrap_or_default();
                match self.enroll(&id, said.name.as_deref(), said.key.as_deref()) {
                    Ok(host_keys) => {
                        out.tell(&json!({
                            "t": "enrolled",
                            "id": id,
                            "hostKeys": host_keys,
                            "members": self.said(),
                        }));
                        self.tell_all(None);
                    }
                    Err(why) => {
                        out.tell(&refused(why));
                    }
                }
            }
            "remove" => {
                let id = said.id_text().unwrap_or_default();
                match self.remove(&id) {
                    Ok(()) => {
                        out.tell(&json!({"t": "removed", "id": id, "members": self.said()}));
                        self.tell_all(None);
                    }
                    Err(why) => {
                        out.tell(&refused(why));
                    }
                }
            }
            "rename" => {
                let id = said.id_text().unwrap_or_default();
                match self.rename(&id, said.name.as_deref()) {
                    Ok(()) => {
                        out.tell(&json!({"t": "renamed", "id": id, "members": self.said()}));
                        self.tell_all(None);
                    }
                    Err(why) => {
                        out.tell(&refused(why));
                    }
                }
            }
            "stop" => {
                eprintln!("{}", specboard_core::text!("agent.run.stopping"));
                let _ = std::fs::remove_file(super::socket(&self.home));
                std::process::exit(0);
            }
            other => {
                out.tell(&json!({
                    "t": "error",
                    "message": specboard_core::text!("agent.run.unknownLine", line = other),
                }));
            }
        }
    }

    /// The members written — the hub's own list first, then the keys' lines made from it — or put
    /// back as they were when either cannot be written.
    fn keep(&self, members: &mut Vec<Member>, now: Vec<Member>) -> Result<(), String> {
        members::sync_keys(&self.home, &now)?;
        if let Err(why) = members::save(&self.home, &now) {
            let _ = members::sync_keys(&self.home, members);
            return Err(why);
        }
        *members = now;
        Ok(())
    }

    /// A member added: its id, its name and its key each its own among the members. The host's
    /// public host keys, for its agent.
    fn enroll(
        &self,
        id: &str,
        name: Option<&str>,
        key: Option<&str>,
    ) -> Result<Vec<String>, String> {
        if !members::id_fits(id) {
            return Err(specboard_core::text!("agent.hub.badId"));
        }
        let name = name
            .and_then(members::name_of)
            .ok_or_else(|| specboard_core::text!("agent.hub.badName"))?;
        let key = key
            .and_then(members::key_of)
            .ok_or_else(|| specboard_core::text!("agent.hub.badKey"))?;
        let host_keys = members::host_keys()?;
        let mut members = self.locked_members();
        if members.iter().any(|m| m.id == id) {
            return Err(specboard_core::text!("agent.hub.idTaken", id));
        }
        if let Some(other) = members.iter().find(|m| m.key == key) {
            return Err(specboard_core::text!(
                "agent.hub.keyTaken",
                name = other.name
            ));
        }
        name_free(&members, &name, None)?;
        let mut now = members.clone();
        now.push(Member {
            id: id.to_string(),
            name,
            key,
            added: members::now(),
            seen: None,
        });
        self.keep(&mut members, now)?;
        Ok(host_keys)
    }

    /// A member gone: from the members and the keys, and its connection closed.
    fn remove(&self, id: &str) -> Result<(), String> {
        {
            let mut members = self.locked_members();
            if !members.iter().any(|m| m.id == id) {
                return Err(specboard_core::text!("agent.hub.notMember", id));
            }
            let now: Vec<Member> = members.iter().filter(|m| m.id != id).cloned().collect();
            self.keep(&mut members, now)?;
        }
        let link = self.locked_links().get(id).cloned();
        if let Some(link) = link {
            self.unlinked(id, &link);
        }
        Ok(())
    }

    fn rename(&self, id: &str, name: Option<&str>) -> Result<(), String> {
        let name = name
            .and_then(members::name_of)
            .ok_or_else(|| specboard_core::text!("agent.hub.badName"))?;
        let mut members = self.locked_members();
        if !members.iter().any(|m| m.id == id) {
            return Err(specboard_core::text!("agent.hub.notMember", id));
        }
        name_free(&members, &name, Some(id))?;
        let mut now = members.clone();
        for member in &mut now {
            if member.id == id {
                member.name = name.clone();
            }
        }
        self.keep(&mut members, now)
    }

    /// Each member asked whether it is there, every `ping`; one nothing came from for three times
    /// that is closed.
    fn keep_asking(self: Arc<Self>) {
        loop {
            std::thread::sleep(self.ping);
            let links: Vec<(String, Arc<Linked>)> = self
                .locked_links()
                .iter()
                .map(|(id, link)| (id.clone(), link.clone()))
                .collect();
            for (id, link) in links {
                let heard = *link.heard.lock().unwrap_or_else(|e| e.into_inner());
                if heard.elapsed() > self.ping * 3 {
                    self.unlinked(&id, &link);
                } else {
                    link.out.say(r#"{"t":"ping"}"#.to_string());
                }
            }
        }
    }
}

/// No other member is called `name`, in any case: a member is named by it where a project is
/// (`<project>@<member>`), read in any case.
fn name_free(members: &[Member], name: &str, but: Option<&str>) -> Result<(), String> {
    let taken = members
        .iter()
        .any(|m| Some(m.id.as_str()) != but && m.name.to_lowercase() == name.to_lowercase());
    if taken {
        return Err(specboard_core::text!("agent.hub.nameTaken", name));
    }
    Ok(())
}

/// The lock that keeps one hub a user, held: none when another holds it.
fn hold(path: &Path) -> Option<std::fs::File> {
    use std::os::unix::io::AsRawFd;
    let file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(path)
        .ok()?;
    // SAFETY: flock on a file descriptor this function owns; nothing else is touched.
    let held = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0;
    held.then_some(file)
}

/// How often a member is asked whether it is there: `SPECBOARD_HUB_PING` seconds, else `PING`.
fn ping() -> Duration {
    std::env::var("SPECBOARD_HUB_PING")
        .ok()
        .and_then(|s| s.parse::<f64>().ok())
        .filter(|s| s.is_finite() && *s > 0.0)
        .map_or(PING, Duration::from_secs_f64)
}

/// Run as the hub of the user it runs as, until told to stop. Another running already: nothing
/// done.
pub fn serve() -> i32 {
    let home = super::home();
    if let Err(error) = std::fs::create_dir_all(&home) {
        eprintln!(
            "{}",
            specboard_core::text!("agent.run.cannotCreate", path = home.display(), error)
        );
        return 1;
    }
    let _ = std::fs::set_permissions(&home, std::fs::Permissions::from_mode(0o700));
    let Some(_held) = hold(&home.join("hub.lock")) else {
        return 0;
    };
    // The name the members' keys run, pointed at the build running now.
    let link = members::link(&home);
    let _ = std::fs::remove_file(&link);
    if let Ok(exe) = std::env::current_exe() {
        let _ = std::os::unix::fs::symlink(exe, &link);
    }
    // Before anything of the hub's is touched: a list that cannot be read is put right by hand.
    let kept = match members::load(&home) {
        Ok(kept) => kept,
        Err(why) => {
            eprintln!("{why}");
            return 1;
        }
    };
    // The keys' lines as the members are: one written by hand into the list, or a line lost, is
    // put right as the hub starts.
    if let Err(why) = members::sync_keys(&home, &kept) {
        eprintln!("{why}");
    }
    let hub = Arc::new(Hub {
        home: home.clone(),
        members: Mutex::new(kept),
        linked: Mutex::new(HashMap::new()),
        apps: Mutex::new(Vec::new()),
        next: AtomicU64::new(0),
        ping: ping(),
    });
    let path = super::socket(&home);
    let _ = std::fs::remove_file(&path);
    let listener = match UnixListener::bind(&path) {
        Ok(listener) => listener,
        Err(error) => {
            eprintln!(
                "{}",
                specboard_core::text!("agent.run.cannotWait", path = path.display(), error)
            );
            return 1;
        }
    };
    let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    eprintln!(
        "{}",
        specboard_core::text!("agent.run.running", me = crate::daemon::me())
    );
    {
        let hub = hub.clone();
        std::thread::spawn(move || hub.keep_asking());
    }
    for connection in listener.incoming() {
        let Ok(connection) = connection else { continue };
        let hub = hub.clone();
        std::thread::spawn(move || hub.answer(connection));
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn linked(n: u64) -> Arc<Linked> {
        let (ours, _theirs) = UnixStream::pair().unwrap();
        Arc::new(Linked {
            n,
            out: Out::new(ours),
            build: None,
            since: 0,
            heard: Mutex::new(Instant::now()),
            owes: Mutex::new(HashSet::new()),
            gone: AtomicBool::new(false),
        })
    }

    #[test]
    fn a_connection_let_go_of_twice_forgets_what_its_member_sent_once() {
        let hub = Hub {
            home: std::env::temp_dir().join("specboard-hub-none"),
            members: Mutex::new(Vec::new()),
            linked: Mutex::new(HashMap::new()),
            apps: Mutex::new(Vec::new()),
            next: AtomicU64::new(0),
            ping: PING,
        };
        let (first, second, owner) = (linked(1), linked(2), linked(3));
        let owed = |owner: &Linked| -> Vec<(String, String)> {
            owner.owes.lock().unwrap().iter().cloned().collect()
        };
        // A's second connection is in the place of its first, which had a call out to B.
        hub.locked_links().insert("b".into(), owner.clone());
        hub.locked_links().insert("a".into(), second);
        owner.owes.lock().unwrap().insert(("a".into(), "1".into()));
        hub.unlinked("a", &first);
        assert!(owed(&owner).is_empty());
        // What A then sends by its second is B's to answer, whenever the first's own thread comes
        // to its end and lets go of it once more.
        owner.owes.lock().unwrap().insert(("a".into(), "2".into()));
        hub.unlinked("a", &first);
        assert_eq!(owed(&owner), [("a".to_string(), "2".to_string())]);
    }
}
