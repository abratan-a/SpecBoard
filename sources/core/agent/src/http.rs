//! HTTP/1.1 as the daemon serves MCP on 127.0.0.1 (`mcp.rs`): a request read whole with the limits
//! the app's own listener had before it went (64 KB of header, 8 MB of body), what is turned away
//! before the body is read — a web page's, and one without a key of the store's, whatever its
//! method — and the answer written.
//!
//! Anyone on the host may connect, so how long one may be held is bounded by the request, not by
//! each read (`Limits`): a client sending a byte now and then is cut when its time is up. One that
//! has not shown a key of the store's has a short while for its head (`Limits::key`); one that has
//! may stay open between requests (`Limits::idle`). And how many may be held is counted apart for
//! those two (`Slots`): connections that have shown no key never take the places of those that
//! have, however many are opened.

use serde_json::{Map, Value};
use specboard_core::access::key_refused;
use std::io::Read;
use std::net::{Shutdown, TcpStream};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// The most connections held at once that have not shown a key of the store's: enough for the
/// clients that are about to show one, connecting all at once, few enough that opening many costs
/// the host little.
pub const STRANGERS: usize = 32;
/// The most connections held at once that have shown a key of the store's: as many as the app's
/// listener took in all.
pub const KEYED: usize = 32;
pub const MAX_HEADER_BYTES: usize = 64 << 10;
pub const MAX_BODY_BYTES: usize = specboard_core::MAX_REQUEST_BYTES;

/// How long a connection may take, whole requests rather than each read.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    /// For the next request to begin, on a connection a key of the store's came on.
    pub idle: Duration,
    /// For the head of a request to come whole: on a connection no key has come on yet, counted
    /// from when it was waited for — so a stranger holds a connection no longer than this.
    pub key: Duration,
    /// For a request to come whole, head and body, from its first byte.
    pub request: Duration,
}

pub const LIMITS: Limits = Limits {
    idle: Duration::from_secs(600),
    key: Duration::from_secs(10),
    request: Duration::from_secs(30),
};

/// One request as it came in.
pub struct Request {
    pub method: String,
    pub path: String,
    pub headers: Map<String, Value>,
    pub body: String,
}

/// One connection's requests, as they come: what was read past the last, whether a key of the
/// store's has come on it, and its place among the connections held (`Slots`), when it has one.
pub struct Incoming {
    pub buffer: Vec<u8>,
    pub known: bool,
    pub slot: Option<Slot>,
}

impl Incoming {
    pub fn new() -> Incoming {
        Incoming {
            buffer: Vec::new(),
            known: false,
            slot: None,
        }
    }

    /// The requests of a connection held in `slot`.
    pub fn held(slot: Slot) -> Incoming {
        Incoming {
            slot: Some(slot),
            ..Incoming::new()
        }
    }
}

impl Default for Incoming {
    fn default() -> Self {
        Incoming::new()
    }
}

/// The next whole request on the connection; None when the client closed, went quiet or ran out of
/// its time (`limits`); the status and the words to turn it away with when it is not one this
/// serves. `knows` says whether the headers carry a key of the store's: asked before the body is
/// read, for any path but `/healthz`.
pub fn read_request(
    connection: &mut TcpStream,
    incoming: &mut Incoming,
    knows: &dyn Fn(&Map<String, Value>) -> bool,
    limits: &Limits,
) -> Result<Option<Request>, (u16, String)> {
    let waited = Instant::now();
    let mut begun = (!incoming.buffer.is_empty()).then_some(waited);
    let mut head_read = false;
    let mut chunk = vec![0u8; 1 << 16];
    loop {
        let buffer = &mut incoming.buffer;
        if let Some(end) = find(buffer, b"\r\n\r\n") {
            if end > MAX_HEADER_BYTES {
                return Err((431, "request header too large".into()));
            }
            let head = String::from_utf8_lossy(&buffer[..end]).into_owned();
            let mut lines = head.split("\r\n");
            let request_line: Vec<&str> = lines.next().unwrap_or("").split(' ').collect();
            if request_line.len() < 2 || request_line[0].is_empty() {
                return Err((400, "bad request".into()));
            }
            let mut headers = Map::new();
            for line in lines {
                let Some((name, value)) = line.split_once(':') else {
                    continue;
                };
                headers.insert(
                    name.trim().to_lowercase(),
                    Value::String(value.trim().to_string()),
                );
            }
            if !head_read {
                head_read = true;
                // Asked only when there is a key to look up: an origin is turned away first.
                let known = !headers.contains_key("origin") && bearer(&headers) && knows(&headers);
                // The refusal is an MCP answer: in the language MCP answers in.
                let refused = {
                    let _lang = specboard_core::text::bind(Some(crate::mcp::ANSWERS_IN));
                    refusal(request_line[1], &headers, known)
                };
                if let Some(refused) = refused {
                    return Err(refused);
                }
                // The first key on it: it takes a place of those that showed one, or is told
                // they are all in use (none of them waiting, so none to let go of).
                if known
                    && !incoming.known
                    && incoming.slot.as_ref().is_some_and(|slot| !slot.keyed())
                {
                    let _lang = specboard_core::text::bind(Some(crate::mcp::ANSWERS_IN));
                    return Err((503, specboard_core::text!("agent.mcp.allInUse")));
                }
                incoming.known |= known;
            }
            let length = match headers.get("content-length").and_then(Value::as_str) {
                None => 0,
                Some(declared) => match declared.parse::<usize>() {
                    Ok(length) if length > MAX_BODY_BYTES => {
                        return Err((413, "request body too large".into()));
                    }
                    Ok(length) => length,
                    Err(_) => return Err((400, "bad content-length".into())),
                },
            };
            let start = end + 4;
            if buffer.len() >= start + length {
                let body = String::from_utf8_lossy(&buffer[start..start + length]).into_owned();
                buffer.drain(..start + length);
                return Ok(Some(Request {
                    method: request_line[0].to_string(),
                    path: request_line[1].to_string(),
                    headers,
                    body,
                }));
            }
        } else if buffer.len() > MAX_HEADER_BYTES {
            return Err((431, "request header too large".into()));
        }
        let due = match (head_read, begun) {
            (true, Some(begun)) => begun + limits.request,
            (false, _) if !incoming.known => waited + limits.key,
            (false, Some(begun)) => begun + limits.key.min(limits.request),
            (_, None) => waited + limits.idle,
        };
        let Some(left) = due
            .checked_duration_since(Instant::now())
            .filter(|left| !left.is_zero())
        else {
            return Ok(None);
        };
        // Waiting for its next request, a connection a key came on may be let go of for
        // another that showed one.
        let waiting = begun.is_none() && incoming.known;
        if let Some(slot) = incoming.slot.as_ref().filter(|_| waiting) {
            slot.set(State::Waiting);
        }
        let _ = connection.set_read_timeout(Some(left));
        let read = connection.read(&mut chunk);
        if let Some(slot) = incoming.slot.as_ref().filter(|_| waiting) {
            slot.set(State::Busy);
        }
        match read {
            Ok(0) | Err(_) => return Ok(None),
            Ok(read) => {
                if begun.is_none()
                    && !incoming.known
                    && let Some(slot) = &incoming.slot
                {
                    slot.set(State::Heard);
                }
                incoming.buffer.extend_from_slice(&chunk[..read]);
                begun.get_or_insert_with(Instant::now);
            }
        }
    }
}

/// Whether the headers carry `Authorization: Bearer` with something after it.
fn bearer(headers: &Map<String, Value>) -> bool {
    headers
        .get("authorization")
        .and_then(Value::as_str)
        .and_then(|value| {
            let lower = value.to_ascii_lowercase();
            lower
                .starts_with("bearer ")
                .then(|| value["bearer ".len()..].trim())
        })
        .is_some_and(|key| !key.is_empty())
}

/// What is turned away whatever the body: a request a web page sent (it names its site in `Origin`,
/// which no MCP client sends), and one to anything but `/healthz` without a key of the store's
/// (`known`), whatever its method, in the words the core says it with. The rest of the guard the
/// core and `mcp.rs` ask of the whole request.
pub fn refusal(path: &str, headers: &Map<String, Value>, known: bool) -> Option<(u16, String)> {
    if headers.contains_key("origin") {
        return Some((403, "requests from web pages are not served".into()));
    }
    if path != "/healthz" && !known {
        return Some((401, key_refused(headers)));
    }
    None
}

/// What a connection held is doing, as `Slots` lets go of them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    /// No key of the store's has come on it yet, nor anything else.
    Stranger,
    /// No key of the store's has come on it yet, but something has: a head on its way.
    Heard,
    /// A key came on it, and it waits for its next request.
    Waiting,
    /// A key came on it, and a request (or a stream) is on it.
    Busy,
}

impl State {
    fn keyed(self) -> bool {
        matches!(self, State::Waiting | State::Busy)
    }
}

/// The connections held, counted apart by whether a key of the store's came on them: those that
/// have not shown one (`STRANGERS`) never take the places of those that have (`KEYED`). A new
/// connection is always taken: when strangers are as many as they may be, the one held longest
/// that has sent nothing is let go of (else the one held longest) — opening more only turns them
/// over among themselves, and a client sending its head is let go of last. A connection that
/// shows a key when those that did are as many as they may be takes the place of the one that has
/// waited longest for its next request; when every one is in a request, it is turned away.
pub struct Slots {
    held: Mutex<Held>,
    strangers: usize,
    keyed: usize,
}

#[derive(Default)]
struct Held {
    next: u64,
    /// Oldest first.
    all: Vec<Holder>,
}

struct Holder {
    id: u64,
    /// The same socket as the one served: shut, its reads end and it is let go of.
    stream: TcpStream,
    state: State,
}

impl Held {
    /// The oldest of `state` shut and let go of; whether there was one.
    fn let_go_of(&mut self, state: State) -> bool {
        let Some(at) = self.all.iter().position(|h| h.state == state) else {
            return false;
        };
        let gone = self.all.remove(at);
        let _ = gone.stream.shutdown(Shutdown::Both);
        true
    }

    fn count(&self, keyed: bool) -> usize {
        self.all.iter().filter(|h| h.state.keyed() == keyed).count()
    }
}

impl Slots {
    pub fn new(strangers: usize, keyed: usize) -> Arc<Slots> {
        Arc::new(Slots {
            held: Mutex::new(Held::default()),
            strangers,
            keyed,
        })
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Held> {
        self.held.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// A connection just taken, held as a stranger; none when it cannot be held (its socket
    /// cannot be shared to be shut).
    pub fn admit(self: &Arc<Self>, connection: &TcpStream) -> Option<Slot> {
        let stream = connection.try_clone().ok()?;
        let mut held = self.lock();
        while held.count(false) >= self.strangers
            && (held.let_go_of(State::Stranger) || held.let_go_of(State::Heard))
        {}
        held.next += 1;
        let id = held.next;
        held.all.push(Holder {
            id,
            stream,
            state: State::Stranger,
        });
        Some(Slot {
            slots: self.clone(),
            id,
        })
    }

    /// How many are held: strangers, and those a key came on.
    #[cfg(test)]
    pub fn counts(&self) -> (usize, usize) {
        let held = self.lock();
        (held.count(false), held.count(true))
    }
}

/// A connection's place among those held, given back when it goes.
pub struct Slot {
    slots: Arc<Slots>,
    id: u64,
}

impl Slot {
    /// A key of the store's came on it: whether it has a place among those that showed one.
    pub fn keyed(&self) -> bool {
        let mut held = self.slots.lock();
        let Some(at) = held.all.iter().position(|h| h.id == self.id) else {
            // Let go of meanwhile: its reads end by themselves.
            return true;
        };
        if held.all[at].state.keyed() {
            return true;
        }
        while held.count(true) >= self.slots.keyed {
            if !held.let_go_of(State::Waiting) {
                return false;
            }
        }
        if let Some(h) = held.all.iter_mut().find(|h| h.id == self.id) {
            h.state = State::Busy;
        }
        true
    }

    /// What it does now: once a key came on it, waiting or in a request; before, heard from.
    pub fn set(&self, state: State) {
        let mut held = self.slots.lock();
        if let Some(h) = held
            .all
            .iter_mut()
            .find(|h| h.id == self.id && h.state.keyed() == state.keyed())
        {
            h.state = state;
        }
    }
}

impl Drop for Slot {
    fn drop(&mut self) {
        self.slots.lock().all.retain(|h| h.id != self.id);
    }
}

pub fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

pub fn text_headers() -> Map<String, Value> {
    let mut headers = Map::new();
    headers.insert("content-type".into(), "text/plain; charset=utf-8".into());
    headers
}

pub fn response(status: u16, headers: &Map<String, Value>, body: &[u8]) -> Vec<u8> {
    let mut headers = headers.clone();
    headers.insert("content-length".into(), body.len().to_string().into());
    headers.insert("connection".into(), "keep-alive".into());
    let mut out = status_and_headers(status, &headers);
    out.extend_from_slice(body);
    out
}

/// The status line and the headers, sorted by name as the app's listener wrote them.
pub fn status_and_headers(status: u16, headers: &Map<String, Value>) -> Vec<u8> {
    let mut text = format!("HTTP/1.1 {status} {}\r\n", reason(status));
    let mut names: Vec<&String> = headers.keys().collect();
    names.sort();
    for name in names {
        let value = match &headers[name.as_str()] {
            Value::String(value) => value.clone(),
            other => other.to_string(),
        };
        text.push_str(&format!("{name}: {value}\r\n"));
    }
    text.push_str("\r\n");
    text.into_bytes()
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        202 => "Accepted",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        406 => "Not Acceptable",
        413 => "Content Too Large",
        415 => "Unsupported Media Type",
        431 => "Request Header Fields Too Large",
        500 => "Internal Server Error",
        503 => "Service Unavailable",
        _ => "Status",
    }
}

/// An event stream's head: no length, and the stream's own headers over what was given.
pub fn stream_head(status: u16, headers: &Map<String, Value>) -> Vec<u8> {
    let mut head = headers.clone();
    head.insert("content-type".into(), "text/event-stream".into());
    head.insert("cache-control".into(), "no-cache".into());
    head.insert("connection".into(), "keep-alive".into());
    status_and_headers(status, &head)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::net::TcpListener;

    /// Short enough for a test, in the same order as the served ones.
    const SHORT: Limits = Limits {
        idle: Duration::from_secs(5),
        key: Duration::from_millis(300),
        request: Duration::from_millis(1500),
    };

    /// A connection to read from, and the client's side of it, which sends `sent` (each piece
    /// after `every`) and then holds the connection open until `for_` has gone by.
    fn connected(sent: Vec<Vec<u8>>, every: Duration, for_: Duration) -> TcpStream {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            let begun = Instant::now();
            let Ok(mut client) = TcpStream::connect(("127.0.0.1", port)) else {
                return;
            };
            for piece in sent {
                if client.write_all(&piece).is_err() {
                    return;
                }
                std::thread::sleep(every);
            }
            while begun.elapsed() < for_ {
                std::thread::sleep(Duration::from_millis(20));
            }
        });
        listener.accept().unwrap().0
    }

    /// One byte at a time, as long as the test lasts.
    fn drip(text: &str) -> Vec<Vec<u8>> {
        text.bytes().map(|b| vec![b]).collect()
    }

    #[test]
    fn a_client_without_a_key_sending_a_byte_now_and_then_is_cut_when_its_head_is_due() {
        let mut connection = connected(
            drip(&"POST /mcp HTTP/1.1\r\nX: ".repeat(10)),
            Duration::from_millis(50),
            Duration::from_secs(10),
        );
        let begun = Instant::now();
        let read = read_request(&mut connection, &mut Incoming::new(), &|_| true, &SHORT);
        assert!(matches!(read, Ok(None)));
        let took = begun.elapsed();
        assert!(took >= SHORT.key && took < SHORT.key * 3, "{took:?}");
    }

    #[test]
    fn a_body_sent_a_byte_now_and_then_is_cut_when_the_request_is_due() {
        let head =
            "POST /mcp HTTP/1.1\r\nAuthorization: Bearer spk_x\r\nContent-Length: 500\r\n\r\n";
        let mut sent = vec![head.as_bytes().to_vec()];
        sent.extend(drip(&"x".repeat(500)));
        let mut connection = connected(sent, Duration::from_millis(50), Duration::from_secs(10));
        let mut incoming = Incoming::new();
        let begun = Instant::now();
        let read = read_request(&mut connection, &mut incoming, &|_| true, &SHORT);
        assert!(matches!(read, Ok(None)));
        let took = begun.elapsed();
        assert!(
            took >= SHORT.request && took < SHORT.request * 2,
            "{took:?}"
        );
        assert!(incoming.known);
    }

    #[test]
    fn a_connection_a_key_came_on_may_wait_for_its_next_request_and_one_without_may_not() {
        let ping = "POST /mcp HTTP/1.1\r\nAuthorization: Bearer spk_x\r\nContent-Length: 0\r\n\r\n";
        let wait = SHORT.key * 2;
        // Its first request, then nothing until the key's time for a head has gone by twice: served.
        let mut connection = connected(
            vec![ping.as_bytes().to_vec(), ping.as_bytes().to_vec()],
            wait,
            Duration::from_secs(10),
        );
        let mut incoming = Incoming::new();
        let first = read_request(&mut connection, &mut incoming, &|_| true, &SHORT);
        assert!(matches!(first, Ok(Some(_))));
        let next = read_request(&mut connection, &mut incoming, &|_| true, &SHORT);
        assert!(
            matches!(next, Ok(Some(_))),
            "鍵の来た接続が次を待たなかった"
        );

        // No key yet: let go once the key's time for a head is up, not the idle one's.
        let mut quiet = connected(vec![], wait, Duration::from_secs(10));
        let begun = Instant::now();
        let read = read_request(&mut quiet, &mut Incoming::new(), &|_| true, &SHORT);
        assert!(matches!(read, Ok(None)));
        assert!(begun.elapsed() < SHORT.key * 3, "{:?}", begun.elapsed());
    }

    #[test]
    fn a_key_the_store_does_not_have_is_turned_away_before_the_body_whatever_the_method() {
        for method in ["POST", "DELETE", "GET", "PUT"] {
            let head = format!(
                "{method} /mcp HTTP/1.1\r\nAuthorization: Bearer spk_x\r\nContent-Length: 8388608\r\n\r\n"
            );
            let mut connection = connected(
                vec![head.into_bytes()],
                Duration::ZERO,
                Duration::from_secs(10),
            );
            let begun = Instant::now();
            let read = read_request(&mut connection, &mut Incoming::new(), &|_| false, &SHORT);
            assert!(matches!(&read, Err((401, _))), "{method}");
            assert!(begun.elapsed() < SHORT.key, "{method}");
        }
    }

    /// A connection's two ends: the one served, and the client's.
    fn pair(listener: &TcpListener) -> (TcpStream, TcpStream) {
        let port = listener.local_addr().unwrap().port();
        let client = TcpStream::connect(("127.0.0.1", port)).unwrap();
        let served = listener.accept().unwrap().0;
        client
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        (served, client)
    }

    /// Whether the client's end was closed by the side served.
    fn closed(client: &mut TcpStream) -> bool {
        matches!(client.read(&mut [0u8; 1]), Ok(0) | Err(_))
    }

    #[test]
    fn strangers_past_their_number_let_go_of_the_oldest_and_never_take_a_keyed_place() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let slots = Slots::new(2, 2);
        let mut ends = Vec::new();
        let mut held = Vec::new();
        for _ in 0..5 {
            let (served, client) = pair(&listener);
            held.push(slots.admit(&served).unwrap());
            ends.push((served, client));
        }
        assert_eq!(slots.counts(), (2, 0));
        for (i, (_, client)) in ends.iter_mut().enumerate().take(3) {
            assert!(closed(client), "{i} 本目が閉じられていない");
        }
        // A key on one of those held: a keyed place of its own, whatever the strangers.
        assert!(held[4].keyed());
        assert_eq!(slots.counts(), (1, 1));
        let (served, _client) = pair(&listener);
        let _more = slots.admit(&served).unwrap();
        assert_eq!(slots.counts(), (2, 1));
    }

    #[test]
    fn a_stranger_sending_its_head_is_let_go_of_after_those_that_sent_nothing() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let slots = Slots::new(2, 2);
        let (served, mut sending) = pair(&listener);
        let first = slots.admit(&served).unwrap();
        first.set(State::Heard);
        let mut ends = Vec::new();
        let mut held = vec![first];
        for _ in 0..3 {
            let (served, client) = pair(&listener);
            held.push(slots.admit(&served).unwrap());
            ends.push((served, client));
        }
        assert!(closed(&mut ends[0].1));
        assert!(closed(&mut ends[1].1));
        sending
            .set_read_timeout(Some(Duration::from_millis(200)))
            .unwrap();
        assert!(
            matches!(sending.read(&mut [0u8; 1]), Err(e) if e.kind() == std::io::ErrorKind::WouldBlock || e.kind() == std::io::ErrorKind::TimedOut),
            "頭を送っている接続が先に閉じられた"
        );
        assert_eq!(slots.counts(), (2, 0));
    }

    #[test]
    fn a_new_key_takes_the_place_of_the_one_waiting_longest_and_none_of_one_in_a_request() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let slots = Slots::new(4, 2);
        let mut ends = Vec::new();
        let mut held = Vec::new();
        for _ in 0..2 {
            let (served, client) = pair(&listener);
            let slot = slots.admit(&served).unwrap();
            assert!(slot.keyed());
            slot.set(State::Waiting);
            held.push(slot);
            ends.push(client);
        }
        let (served, _third) = pair(&listener);
        let third = slots.admit(&served).unwrap();
        assert!(third.keyed(), "待っている接続を空けずに断った");
        assert!(
            closed(&mut ends[0]),
            "一番長く待っていた接続が閉じられていない"
        );
        assert_eq!(slots.counts(), (0, 2));
        // Every one in a request: none is let go of, and the new one is turned away.
        held[1].set(State::Busy);
        let (served, _fourth) = pair(&listener);
        let fourth = slots.admit(&served).unwrap();
        assert!(!fourth.keyed());
        assert_eq!(slots.counts(), (1, 2));
        drop(third);
        assert!(fourth.keyed(), "空いた枠を取れない");
    }

    #[test]
    fn a_key_that_finds_every_place_in_a_request_is_told_so_and_one_waiting_gives_way() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let slots = Slots::new(4, 1);
        let ping = "POST /mcp HTTP/1.1\r\nAuthorization: Bearer spk_x\r\nContent-Length: 0\r\n\r\n";
        let (busy, _busy_client) = pair(&listener);
        let in_request = slots.admit(&busy).unwrap();
        assert!(in_request.keyed());
        let (mut served, mut client) = pair(&listener);
        client.write_all(ping.as_bytes()).unwrap();
        let mut incoming = Incoming::held(slots.admit(&served).unwrap());
        let read = read_request(&mut served, &mut incoming, &|_| true, &SHORT);
        assert!(matches!(&read, Err((503, _))), "{:?}", read.err());
        // The one in a request now waits for its next: the key is let on in its place.
        in_request.set(State::Waiting);
        let (mut served, mut client) = pair(&listener);
        client.write_all(ping.as_bytes()).unwrap();
        let mut incoming = Incoming::held(slots.admit(&served).unwrap());
        let read = read_request(&mut served, &mut incoming, &|_| true, &SHORT);
        assert!(matches!(read, Ok(Some(_))));
        assert_eq!(slots.counts().1, 1);
    }

    #[test]
    fn a_key_the_store_does_not_have_is_told_it_cannot_be_used() {
        let _lang = specboard_core::text::bind(Some("en"));
        let mut with_key = Map::new();
        with_key.insert("authorization".into(), "Bearer spk_gone".into());
        let (status, said) = refusal("/mcp", &with_key, false).unwrap();
        assert_eq!(status, 401);
        assert_eq!(said, specboard_core::text!("access.keyUnusable"));
        let (_, said) = refusal("/mcp", &Map::new(), false).unwrap();
        assert_eq!(said, specboard_core::access::key_needed());
    }
}
