//! The daemon's MCP: the
//! projects it keeps, answered to the AI sessions of its user at their own loopback, with no app in
//! between — the only way MCP is answered. Every JSON-RPC
//! message is the core's to answer (op `mcp`: the access key, in the core's database here, both
//! eras' envelopes, the tools); what is HTTP's is this: what a web page could have sent turned away,
//! the methods and the content type, `/healthz`, the read-only path that is gone, and the 2026 era's
//! `subscriptions/listen` stream, kept open with a comment every 25 seconds.
//!
//! It listens at the port of its user's setting (`~/.specboard/agent.json`'s `mcpPort`, 8787 when
//! none is set; 0 is any port free, for tests): one taken already (another user's agent, or a relay
//! of an older app) is said, and the daemon serves its apps all the same — and tries
//! it again every few seconds, so a port let go of comes to it.
//!
//! It answers in English (`ANSWERS_IN`; `specboard://SpecBoard/Docs/Design:Localization` §1): an AI
//! reads the answers, English takes it the fewest tokens, and every agent answers every session
//! alike. A setting kept by a version that let the language be chosen (`mcpLanguage`) is not read.

use crate::http::{MAX_CONNECTIONS, Request, read_request, response, stream_head, text_headers};
use serde_json::{Map, Value, json};
use specboard_core::{Core, text};
use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// The port the agents listened at before there was a setting, and what one with none listens at.
pub const DEFAULT_PORT: u16 = 8787;

/// The language MCP is answered in, whatever the machine's or a screen's.
pub const ANSWERS_IN: &str = "en";

/// What the read-only connection of before is told.
const READ_ONLY_PATH: &str = "/mcp/read";
fn read_only_gone() -> String {
    specboard_core::text!("agent.mcp.readOnlyGone")
}
const SUBSCRIPTION_ID: &str = "io.modelcontextprotocol/subscriptionId";

/// Where the user's settings for the daemon are kept.
pub fn settings(home: &Path) -> PathBuf {
    home.join("agent.json")
}

/// The port of the user's setting.
pub fn port_set(home: &Path) -> u16 {
    std::fs::read_to_string(settings(home))
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .and_then(|v| v["mcpPort"].as_u64())
        .and_then(|p| u16::try_from(p).ok())
        .unwrap_or(DEFAULT_PORT)
}

/// The user's settings as kept, or none.
fn settings_kept(home: &Path) -> Value {
    std::fs::read_to_string(settings(home))
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .filter(Value::is_object)
        .unwrap_or_else(|| json!({}))
}

/// One setting kept, the others as they were. Two apps' lines are answered on two threads: one at a
/// time, so neither writes over what the other kept.
fn keep(home: &Path, key: &str, value: Value) -> Result<(), String> {
    static WRITING: Mutex<()> = Mutex::new(());
    let _one = WRITING.lock().unwrap_or_else(|e| e.into_inner());
    let place = settings(home);
    let mut kept = settings_kept(home);
    kept[key] = value;
    std::fs::write(&place, format!("{kept:#}\n")).map_err(|e| {
        specboard_core::text!(
            "agent.mcp.settingsWriteFailed",
            path = place.display(),
            error = e
        )
    })
}

fn keep_port(home: &Path, port: u16) -> Result<(), String> {
    keep(home, "mcpPort", port.into())
}

/// An open `subscriptions/listen` stream.
struct Listening {
    out: TcpStream,
}

/// Where it listens, or why it does not.
#[derive(Clone)]
struct Listen {
    port: u16,
    problem: String,
}

pub struct Mcp {
    core: Arc<Core>,
    home: PathBuf,
    listen: Mutex<Option<Listen>>,
    /// Bumped as a listener is let go: its loop ends.
    generation: AtomicU64,
    open: AtomicUsize,
    streams: Mutex<Vec<Listening>>,
    /// Told where it listens whenever that changes on its own (the port taken came free).
    told: std::sync::OnceLock<Box<dyn Fn(Value) + Send + Sync>>,
}

/// How often a port taken is tried again.
const RETRY: Duration = Duration::from_secs(5);

impl Mcp {
    /// Listening at the port of the setting (or saying why it cannot), the streams kept alive.
    pub fn start(core: Arc<Core>, home: &Path) -> Arc<Mcp> {
        let mcp = Arc::new(Mcp {
            core,
            home: home.to_path_buf(),
            listen: Mutex::new(None),
            generation: AtomicU64::new(0),
            open: AtomicUsize::new(0),
            streams: Mutex::new(Vec::new()),
            told: std::sync::OnceLock::new(),
        });
        let port = port_set(home);
        if let Err(problem) = mcp.bind(port) {
            eprintln!(
                "{}",
                specboard_core::text!("agent.mcp.listenFailed", problem = &problem)
            );
            *mcp.state() = Some(Listen { port, problem });
        }
        let kept = mcp.clone();
        std::thread::spawn(move || {
            loop {
                std::thread::sleep(Duration::from_secs(25));
                kept.ping();
            }
        });
        // A port taken, tried again until it comes free (or another is chosen).
        let kept = mcp.clone();
        std::thread::spawn(move || {
            loop {
                std::thread::sleep(RETRY);
                let waiting = kept.state().clone().filter(|l| !l.problem.is_empty());
                let Some(Listen { port, .. }) = waiting else {
                    continue;
                };
                if kept.bind(port).is_ok()
                    && let Some(tell) = kept.told.get()
                {
                    tell(kept.said());
                }
            }
        });
        mcp
    }

    /// Who is told where it listens when that changes on its own.
    pub fn tell_with(&self, tell: impl Fn(Value) + Send + Sync + 'static) {
        let _ = self.told.set(Box::new(tell));
    }

    fn state(&self) -> std::sync::MutexGuard<'_, Option<Listen>> {
        self.listen.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Where it listens, as the apps and `status` are told it: `{port, listening, problem}`.
    pub fn said(&self) -> Value {
        match &*self.state() {
            Some(l) => {
                json!({"port": l.port, "listening": l.problem.is_empty(), "problem": l.problem})
            }
            None => json!({"port": port_set(&self.home), "listening": false, "problem": ""}),
        }
    }

    /// Listens at `port` from now on, and keeps it as the setting; the port before let go. One that
    /// cannot be listened at changes nothing.
    pub fn move_to(self: &Arc<Self>, port: u16) -> Result<(), String> {
        if port == 0 {
            return Err(specboard_core::text!("agent.mcp.portRange"));
        }
        let now = self.state().clone();
        if now
            .as_ref()
            .is_some_and(|l| l.port == port && l.problem.is_empty())
        {
            return keep_port(&self.home, port);
        }
        self.bind(port)?;
        keep_port(&self.home, port)
    }

    /// A listener at `port`, answered on a thread of its own; the one before let go.
    fn bind(self: &Arc<Self>, port: u16) -> Result<(), String> {
        let listener = TcpListener::bind(("127.0.0.1", port))
            .map_err(|e| specboard_core::text!("agent.mcp.cannotBind", port, error = e))?;
        listener
            .set_nonblocking(true)
            .map_err(|e| specboard_core::text!("agent.mcp.cannotSetUp", error = e))?;
        let port = listener.local_addr().map(|a| a.port()).unwrap_or(port);
        let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        *self.state() = Some(Listen {
            port,
            problem: String::new(),
        });
        eprintln!("{}", specboard_core::text!("agent.mcp.listening", port));
        let mcp = self.clone();
        std::thread::spawn(move || mcp.accept(listener, generation));
        Ok(())
    }

    fn accept(self: Arc<Self>, listener: TcpListener, generation: u64) {
        while self.generation.load(Ordering::SeqCst) == generation {
            match listener.accept() {
                Ok((connection, _)) => {
                    // What a non-blocking listener takes is non-blocking on some systems.
                    let _ = connection.set_nonblocking(false);
                    if self.open.load(Ordering::SeqCst) >= MAX_CONNECTIONS {
                        continue;
                    }
                    self.open.fetch_add(1, Ordering::SeqCst);
                    let mcp = self.clone();
                    std::thread::spawn(move || {
                        mcp.serve(connection);
                        mcp.open.fetch_sub(1, Ordering::SeqCst);
                    });
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(50));
                }
                Err(_) => std::thread::sleep(Duration::from_millis(50)),
            }
        }
    }

    /// Requests on one connection, one after another, until the client goes or a stream opens.
    fn serve(&self, mut connection: TcpStream) {
        let mut buffer: Vec<u8> = Vec::new();
        loop {
            let request = match read_request(&mut connection, &mut buffer) {
                Ok(Some(request)) => request,
                Ok(None) => return,
                Err((status, message)) => {
                    let _ = connection.write_all(&response(
                        status,
                        &text_headers(),
                        message.as_bytes(),
                    ));
                    let _ = connection.shutdown(Shutdown::Both);
                    return;
                }
            };
            match self.answer(&request) {
                Answer::Plain {
                    status,
                    headers,
                    body,
                } => {
                    if connection
                        .write_all(&response(status, &headers, body.as_bytes()))
                        .is_err()
                    {
                        return;
                    }
                }
                Answer::Listen(events) => {
                    self.stream(connection, &events);
                    return;
                }
            }
        }
    }

    /// One request, answered as the app's listener answered one.
    fn answer(&self, request: &Request) -> Answer {
        let _lang = text::bind(Some(ANSWERS_IN));
        if let Some(refused) = from_browser(&request.headers) {
            return refused;
        }
        if !request.path.starts_with("/mcp") {
            if request.path == "/healthz" {
                return Answer::json(200, "{\"ok\":true}".to_string());
            }
            return Answer::text(404, "not found");
        }
        match request.method.as_str() {
            "POST" => {
                // A page can send a form or a text/plain `fetch` anywhere without asking first;
                // `application/json` makes the browser ask, and nothing here answers that.
                let kind = request
                    .headers
                    .get("content-type")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_ascii_lowercase();
                if !kind.starts_with("application/json") {
                    return Answer::text(415, "content-type must be application/json");
                }
            }
            "DELETE" => return Answer::empty(200),
            "GET" => {
                return Answer::text(405, "this server does not open a notification stream");
            }
            _ => return Answer::text(405, "method not allowed"),
        }
        // What is not JSON goes on as null: the core turns it away, after the key.
        let message: Value = serde_json::from_str(&request.body).unwrap_or(Value::Null);
        if request.path.starts_with(READ_ONLY_PATH) {
            let Some(envelope) = message.as_object() else {
                return Answer::json(
                    200,
                    error_message(
                        &Value::Null,
                        -32700,
                        &specboard_core::text!("mcp.rpc.notJson"),
                    ),
                );
            };
            let Some(id) = envelope.get("id") else {
                return Answer::empty(202);
            };
            return Answer::json(200, error_message(id, -32601, &read_only_gone()));
        }
        let mut headers = request.headers.clone();
        headers.sort_keys();
        let said = self.core.call(
            &json!({"op": "mcp", "message": message, "headers": headers, "lang": ANSWERS_IN})
                .to_string(),
        );
        let answer: Value = serde_json::from_str(&said).unwrap_or(Value::Null);
        if let Some(error) = answer["error"].as_str() {
            return Answer::text(500, error);
        }
        let reply = &answer["result"];
        let Some(status) = reply["status"].as_u64().and_then(|s| u16::try_from(s).ok()) else {
            return Answer::text(500, &specboard_core::text!("agent.mcp.coreUnreadable"));
        };
        // A listen the key and the headers passed: the stream is this side's to keep.
        if reply["listen"] == true
            && let Some(id) = message.get("id")
        {
            return Answer::Listen(listen_events(id, &message["params"]));
        }
        match &reply["body"] {
            // Not a JSON-RPC message: why the request is turned away (401 without a key).
            Value::String(text) => Answer::text(status, text),
            // A notification: nothing to answer with.
            Value::Null => Answer::empty(status),
            body => Answer::json(status, body.to_string()),
        }
    }

    /// A listen stream: acknowledged (and told the tool list changed, when asked), and kept until
    /// the client leaves.
    fn stream(&self, mut connection: TcpStream, events: &[String]) {
        let mut head = Map::new();
        head.insert("x-accel-buffering".into(), "no".into());
        if connection.write_all(&stream_head(200, &head)).is_err() {
            return;
        }
        for event in events {
            if connection
                .write_all(format!("data: {event}\n\n").as_bytes())
                .is_err()
            {
                return;
            }
        }
        if let Ok(out) = connection.try_clone() {
            self.streams
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push(Listening { out });
        }
        // Anything the client sends on this socket is not a request — a stream carries one way —
        // so it is read and dropped; its end is the client leaving (the ping finds it gone).
        let _ = connection.set_read_timeout(None);
        let mut sink = [0u8; 4096];
        while matches!(connection.read(&mut sink), Ok(n) if n > 0) {}
        let _ = connection.shutdown(Shutdown::Both);
    }

    /// A comment on every open stream: what idle timeouts would close is kept, and a client gone
    /// without closing is found out.
    fn ping(&self) {
        self.streams
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .retain_mut(|s| s.out.write_all(b": ping\n\n").is_ok());
    }
}

enum Answer {
    Plain {
        status: u16,
        headers: Map<String, Value>,
        body: String,
    },
    /// A listen stream, its first events.
    Listen(Vec<String>),
}

impl Answer {
    fn text(status: u16, text: &str) -> Answer {
        Answer::Plain {
            status,
            headers: text_headers(),
            body: text.to_string(),
        }
    }

    fn json(status: u16, body: String) -> Answer {
        let mut headers = Map::new();
        headers.insert("content-type".into(), "application/json".into());
        Answer::Plain {
            status,
            headers,
            body,
        }
    }

    fn empty(status: u16) -> Answer {
        Answer::Plain {
            status,
            headers: Map::new(),
            body: String::new(),
        }
    }
}

/// Turns away what a web page could have sent: one naming its site (`Origin` — read already, before
/// the body), and one addressing this by a name that is not loopback (DNS rebinding).
fn from_browser(headers: &Map<String, Value>) -> Option<Answer> {
    if headers.contains_key("origin") {
        return Some(Answer::text(403, "requests from web pages are not served"));
    }
    let host = headers.get("host").and_then(Value::as_str)?;
    let name = if host.starts_with('[') {
        let Some(close) = host.find(']') else {
            return Some(Answer::text(403, "bad host"));
        };
        &host[..=close]
    } else {
        host.split(':').next().unwrap_or("")
    };
    match name.to_ascii_lowercase().as_str() {
        "127.0.0.1" | "localhost" | "[::1]" => None,
        _ => Some(Answer::text(403, "only loopback hosts are served")),
    }
}

fn error_message(id: &Value, code: i64, message: &str) -> String {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}}).to_string()
}

/// What a listen stream opens with: acknowledged with what this honours of what was asked (the tool
/// list changing; anything else is left out of the acknowledgement, the protocol's way of saying it
/// is not supported), and the list said to have changed.
///
/// The list never changes while this process runs. It changes across a new build, and a client that
/// read it from the build before keeps that build's definitions for as long as it stays connected.
/// Seen directly (with the app's listener of before): after a build that added
/// `payloads[].verifiedBy`, Claude Code went on describing `payloads` without it — through a fresh
/// `initialize` after a 404, and through a `list_changed` sent on a 2025 connection, both. A new
/// build drops this stream and the client opens it again, within a second. Claude Code reads the
/// list again on that reopen by itself — on a connection that *began* on this era, because it
/// registers for list changes once, when it first connects, from what `server/discover` declared.
/// The notice sent on every open does the same for a client that waits to be told. On a first
/// connection it costs one extra read.
fn listen_events(id: &Value, params: &Value) -> Vec<String> {
    let tools = params["notifications"]["toolsListChanged"] == true;
    let mut agreed = Map::new();
    if tools {
        agreed.insert("toolsListChanged".into(), true.into());
    }
    let tag = json!({ SUBSCRIPTION_ID: id });
    let mut events = vec![
        json!({"jsonrpc": "2.0", "method": "notifications/subscriptions/acknowledged",
               "params": {"_meta": tag, "notifications": agreed}})
        .to_string(),
    ];
    if tools {
        events.push(
            json!({"jsonrpc": "2.0", "method": "notifications/tools/list_changed",
                   "params": {"_meta": tag}})
            .to_string(),
        );
    }
    events
}
