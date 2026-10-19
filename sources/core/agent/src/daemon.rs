//! The agent's own: the
//! projects of the user it runs as, and the core over them, kept running beside the apps that come and
//! go — as Claude Code's server on a host it reaches by SSH. One runs for each user (`serve`), its
//! place in the user's home (`~/.specboard/`, or `SPECBOARD_AGENT_HOME`): the database in `data/`,
//! the socket an app reaches it at (`agent.sock`, the user's alone), the lock that keeps it one
//! (`agent.lock`) and what it says of itself (`agent.log`). The programs the apps put there are
//! beside them, one folder a build (`agent/<version>/`): an app lets the builds before its own go,
//! and what the agent keeps is not among them. An app reaches it over SSH through a process of its
//! own for each connection (`attach`), one JSON object a line each way:
//!
//! - `{"t":"who"}` → `{"t":"daemon","pid":…,"exe":…,"build":…}`: which one runs (`attach` asks
//!   before it hands the app's lines on, and has one of an older build stopped)
//! - `{"t":"hello","client":…}` → `{"t":"ready","pid":…,"exe":…,"build":…}`: the app, by the name of
//!   its machine
//! - `{"t":"core","id":n,"request":"…"}` → `{"t":"core","id":n,"answer":"…"}`: a request of the core's
//!   (`Core::call`), as the app makes it of the core it holds itself, and the core's answer as the
//!   core wrote it (its keys in their order). Both are the JSON's text (a request may be the object).
//!   The request goes with the app's machine as its `client`: the windows of two Macs are two
//!   hands, and what one holds the other waits for (`Who::client`); and with the connection's own
//!   number as its `connection`: what the core sent it of a project (`project_detail`) is its own,
//!   answered next with what changed, and let go of when it goes
//! - `{"t":"changed","notice":"…","own":…}`: what changed, told to every app it holds
//!   (`Core::listen`), the notice's text; `own` to the app whose request changed it, which reads it
//!   again itself (as the window does what it writes to the core it holds)
//! - `{"t":"mcp_port","port":n,"lang":…}` → `{"t":"mcp","mcp":{…}}`: MCP listened at that port from now
//!   on, and kept as the user's setting (`mcp.rs`); every app told where it listens (`mcp` in `ready`
//!   too). Why one is refused is said in `lang` (the screen's language), else the machine's
//! - `{"t":"windows","windows":[{window, project, branch, section, card, front}]}`: the app's windows
//!   that show this agent's projects (the project by its id; none while it does not take `app_control`
//!   from here)
//! - `{"t":"app","id":n,"arguments":"…","caller":"…","window":w}` → `{"t":"app","id":n,"answer":"…"}`:
//!   `app_control` handed to the app whose window it is (`screens.rs`), answered as the app answers it
//!   for the core it holds itself
//! - `{"t":"picture","id":n,"request":"…"}` → `{"t":"picture","id":n,"answer":"…"}`: a map to draw as a
//!   picture (`App::picture`; `section_read`'s and `card_read`'s `as: picture`), handed to one app it holds —
//!   not to a window: the app draws it on a canvas of its own, whatever its windows show (`screens.rs`) —
//!   answered as the app answers it for the core it holds itself
//! - `{"t":"ask","id":n,"request":"…"}` → `{"t":"ask","id":n,"answer":"…"}`: a person asked whether MCP
//!   may do what it asked to (`App::ask`),
//!   on every app it holds — a person is at one Mac or another — and the first answer taken; then
//!   `{"t":"asked","id":n}` to every app, which takes the question away (answered on another Mac, or no
//!   answer in time)
//! - `{"t":"stop"}`: it ends, and every app it holds with it
//!
//! It answers MCP itself at its user's loopback, with the access keys in its own database (`mcp.rs`).

use crate::mcp::Mcp;
use serde_json::{Value, json};
use specboard_core::{Core, Listener};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// Where the agent of the user it runs as keeps itself.
pub fn home() -> PathBuf {
    if let Some(place) = std::env::var_os("SPECBOARD_AGENT_HOME") {
        return PathBuf::from(place);
    }
    let user = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default();
    user.join(".specboard")
}

pub fn socket(home: &Path) -> PathBuf {
    home.join("agent.sock")
}

fn lock(home: &Path) -> PathBuf {
    home.join("agent.lock")
}

pub fn log(home: &Path) -> PathBuf {
    home.join("agent.log")
}

fn database(home: &Path) -> PathBuf {
    home.join("data").join("specboard-core.db")
}

/// The app's build this was made for (its `CFBundleVersion`, the year to the minute it was built:
/// `tools/build-agent.sh`); empty in a build of the agent's own (tests).
pub const BUILD: &str = match option_env!("SPECBOARD_BUILD") {
    Some(build) => build,
    None => "",
};

/// What this process is: its number, the file it runs from, the app's build it was made for (or
/// the one `SPECBOARD_AGENT_BUILD` says, for tests of the builds' order).
pub fn me() -> Value {
    json!({
        "pid": std::process::id(),
        "exe": std::env::current_exe().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default(),
        "build": std::env::var("SPECBOARD_AGENT_BUILD").unwrap_or_else(|_| BUILD.to_string()),
    })
}

thread_local! {
    /// The app whose request the core is answering on this thread: what changed meanwhile is its
    /// own (the core tells it on the thread the write ran on).
    static ASKING: std::cell::Cell<Option<u64>> = const { std::cell::Cell::new(None) };
}

/// An app it holds: the way back to it, and its windows that show this agent's projects.
pub(crate) struct Client {
    out: Mutex<UnixStream>,
    pub(crate) seen: Mutex<Seen>,
}

/// What an app said of itself: its machine's name, and its windows (`{"t":"windows"}`).
#[derive(Default)]
pub(crate) struct Seen {
    pub(crate) name: String,
    /// Each `{window, project, branch, section, card, front}`, the project by its id.
    pub(crate) windows: Vec<Value>,
    /// When it said them last, against the other apps (the one said last is the nearer).
    pub(crate) said_at: u64,
}

impl Client {
    /// One line to the app; false when it is gone.
    fn say(&self, message: &Value) -> bool {
        let mut out = self.out.lock().unwrap_or_else(|e| e.into_inner());
        writeln!(out, "{message}")
            .and_then(|()| out.flush())
            .is_ok()
    }
}

/// The apps it holds, told what changed, and asked to answer `app_control` and draw pictures
/// (`screens.rs`).
#[derive(Default)]
pub(crate) struct Clients {
    all: Mutex<Vec<(u64, Arc<Client>)>>,
    next: AtomicU64,
    /// What was handed to an app and not answered yet: the app's, and where the answer goes.
    asked: Mutex<std::collections::HashMap<u64, (u64, std::sync::mpsc::Sender<String>)>>,
    next_ask: AtomicU64,
    said: AtomicU64,
}

impl Clients {
    /// Every app it holds now.
    pub(crate) fn all(&self) -> Vec<(u64, Arc<Client>)> {
        self.all.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// What the app `id` is handed on a line `line` (`app`, `picture`): its answer, or none when it
    /// went or said nothing in `wait`.
    pub(crate) fn ask(
        &self,
        id: u64,
        line: &str,
        mut message: Value,
        wait: std::time::Duration,
    ) -> Option<String> {
        let client = self.all().into_iter().find(|(n, _)| *n == id)?.1;
        let ask = self.next_ask.fetch_add(1, Ordering::SeqCst);
        let (sender, answer) = std::sync::mpsc::channel();
        self.asked
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(ask, (id, sender));
        message["t"] = line.into();
        message["id"] = ask.into();
        let said = client
            .say(&message)
            .then(|| answer.recv_timeout(wait).ok())
            .flatten();
        self.asked
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&ask);
        said
    }

    /// A person asked on every app it holds (`App::ask`): the first answer, or none when no app was
    /// there to ask or none answered in `wait`. The question is taken away from every app after.
    pub(crate) fn ask_all(&self, mut message: Value, wait: std::time::Duration) -> Option<String> {
        let all = self.all();
        let ask = self.next_ask.fetch_add(1, Ordering::SeqCst);
        let (sender, answer) = std::sync::mpsc::channel();
        // Of no one app: an app that goes leaves it to the others.
        self.asked
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(ask, (u64::MAX, sender));
        message["t"] = "ask".into();
        message["id"] = ask.into();
        let mut asked = false;
        for (_, client) in &all {
            asked |= client.say(&message);
        }
        let said = asked.then(|| answer.recv_timeout(wait).ok()).flatten();
        self.asked
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&ask);
        for (_, client) in self.all() {
            client.say(&json!({"t": "asked", "id": ask}));
        }
        said
    }

    /// An app's answer to what it was handed.
    fn answered(&self, ask: u64, answer: String) {
        if let Some((_, sender)) = self
            .asked
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&ask)
        {
            let _ = sender.send(answer);
        }
    }

    fn add(&self, client: Arc<Client>) -> u64 {
        let id = self.next.fetch_add(1, Ordering::SeqCst);
        self.all
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push((id, client));
        id
    }

    fn remove(&self, id: u64) {
        self.all
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .retain(|(n, _)| *n != id);
        // What it was asked has no answer coming.
        self.asked
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .retain(|_, (app, _)| *app != id);
    }
}

impl Clients {
    /// One line to every app it holds.
    fn tell(&self, message: &Value) {
        let all: Vec<Arc<Client>> = self
            .all
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .map(|(_, c)| c.clone())
            .collect();
        for client in all {
            client.say(message);
        }
    }
}

impl Listener for Clients {
    fn changed(&self, notice: &str) {
        let asking = ASKING.with(|a| a.get());
        let all: Vec<(u64, Arc<Client>)> =
            self.all.lock().unwrap_or_else(|e| e.into_inner()).clone();
        for (id, client) in all {
            client.say(&json!({"t": "changed", "notice": notice, "own": asking == Some(id)}));
        }
    }
}

/// Run as the one of the user it runs as: the database opened (made, the first time), the socket
/// listened at, each app that comes answered on a thread of its own, until told to stop. Another
/// running already: nothing done.
pub fn serve() -> i32 {
    let home = home();
    if let Err(error) = std::fs::create_dir_all(home.join("data")) {
        eprintln!(
            "{}",
            specboard_core::text!("agent.run.cannotCreate", path = home.display(), error)
        );
        return 1;
    }
    let _ = std::fs::set_permissions(&home, std::fs::Permissions::from_mode(0o700));
    // One a user: the lock is held for as long as it runs, and let go when it ends.
    let Some(_held) = hold(&lock(&home)) else {
        return 0;
    };
    specboard_core::load_grammars(home.join("grammars"));
    let place = database(&home);
    let opened = if place.exists() {
        Core::open(&place)
    } else {
        Core::create(&place)
    };
    let core = match opened {
        Ok(core) => Arc::new(core),
        Err(error) => {
            eprintln!(
                "{}",
                specboard_core::text!("agent.run.databaseFailed", error)
            );
            return 1;
        }
    };
    let clients = Arc::new(Clients::default());
    core.listen(clients.clone());
    core.set_app(Arc::new(crate::screens::Screens::new(
        clients.clone(),
        Arc::downgrade(&core),
    )));
    let mcp = Mcp::start(core.clone(), &home);
    {
        let clients = clients.clone();
        mcp.tell_with(move |said| clients.tell(&json!({"t": "mcp", "mcp": said})));
    }
    let path = socket(&home);
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
    eprintln!("{}", specboard_core::text!("agent.run.running", me = me()));
    for connection in listener.incoming() {
        let Ok(connection) = connection else { continue };
        let core = core.clone();
        let clients = clients.clone();
        let path = path.clone();
        let mcp = mcp.clone();
        std::thread::spawn(move || answer(&core, &clients, &mcp, connection, &path));
    }
    0
}

/// The number of the connection an app came by: told apart from those before it, which a request's
/// answer was sent to.
static CONNECTIONS: AtomicU64 = AtomicU64::new(0);

/// One app's lines, answered until it goes (or says stop).
fn answer(core: &Core, clients: &Clients, mcp: &Arc<Mcp>, connection: UnixStream, path: &Path) {
    let Ok(out) = connection.try_clone() else {
        return;
    };
    let reader = CONNECTIONS.fetch_add(1, Ordering::SeqCst).to_string();
    let client = Arc::new(Client {
        out: Mutex::new(out),
        seen: Mutex::new(Seen::default()),
    });
    let mut id = None;
    let mut name = String::new();
    for line in BufReader::new(connection).lines() {
        let Ok(line) = line else { break };
        let message: Value = serde_json::from_str(&line).unwrap_or(Value::Null);
        match message["t"].as_str() {
            Some("who") => {
                let mut said = me();
                said["t"] = "daemon".into();
                said["mcp"] = mcp.said();
                client.say(&said);
            }
            Some("hello") => {
                let mut said = me();
                said["t"] = "ready".into();
                if id.is_none() {
                    id = Some(clients.add(client.clone()));
                }
                name = message["client"].as_str().unwrap_or("").to_string();
                said["mcp"] = mcp.said();
                client.seen.lock().unwrap_or_else(|e| e.into_inner()).name = name.clone();
                eprintln!("{}", specboard_core::text!("agent.run.connected", name));
                client.say(&said);
            }
            Some("core") => {
                let mut request = match &message["request"] {
                    Value::String(text) => serde_json::from_str(text).unwrap_or(Value::Null),
                    other => other.clone(),
                };
                if let Some(fields) = request.as_object_mut() {
                    fields.insert("client".to_string(), Value::String(name.clone()));
                    fields.insert("connection".to_string(), Value::String(reader.clone()));
                }
                let request = request.to_string();
                ASKING.with(|a| a.set(id));
                let answer = core.call(&request);
                ASKING.with(|a| a.set(None));
                client.say(&json!({"t": "core", "id": message["id"], "answer": answer}));
            }
            // The app's windows that show this agent's projects (none while it does not take
            // `app_control` from here).
            Some("windows") => {
                let mut seen = client.seen.lock().unwrap_or_else(|e| e.into_inner());
                seen.windows = message["windows"].as_array().cloned().unwrap_or_default();
                seen.said_at = clients.said.fetch_add(1, Ordering::SeqCst) + 1;
            }
            Some("app") | Some("ask") | Some("picture") => {
                let ask = message["id"].as_u64().unwrap_or(0);
                clients.answered(ask, message["answer"].as_str().unwrap_or("").to_string());
            }
            Some("mcp_port") => {
                // Refused in the screen's language, when the app says it.
                let _lang = specboard_core::text::bind(message["lang"].as_str());
                let port = message["port"]
                    .as_u64()
                    .and_then(|p| u16::try_from(p).ok())
                    .unwrap_or(0);
                match mcp.move_to(port) {
                    Ok(()) => {
                        let told = json!({"t": "mcp", "mcp": mcp.said()});
                        clients.tell(&told);
                        if id.is_none() {
                            client.say(&told);
                        }
                    }
                    Err(problem) => {
                        client.say(&json!({"t": "mcp", "mcp": mcp.said(), "refused": problem}));
                    }
                }
            }
            Some("stop") => {
                eprintln!("{}", specboard_core::text!("agent.run.stopping"));
                core.close();
                let _ = std::fs::remove_file(path);
                std::process::exit(0);
            }
            _ => {
                client.say(&json!({
                    "t": "error",
                    "message": specboard_core::text!("agent.run.unknownLine", line)
                }));
            }
        }
    }
    if let Some(id) = id {
        clients.remove(id);
    }
    core.forget_reader(&reader);
}

/// The lock that keeps one running, held: none when another holds it.
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
