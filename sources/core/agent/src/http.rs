//! HTTP/1.1 as the daemon serves MCP on 127.0.0.1 (`mcp.rs`): a request read whole with the limits
//! the app's own listener had before it went (32 connections, 64 KB of header, 8 MB of body, 30
//! seconds for a request once begun and 600 for the next to begin), what is turned away before the
//! body is read, and the answer written.

use serde_json::{Map, Value};
use specboard_core::access::key_needed;
use std::io::Read;
use std::net::TcpStream;
use std::time::Duration;

pub const MAX_CONNECTIONS: usize = 32;
pub const MAX_HEADER_BYTES: usize = 64 << 10;
pub const MAX_BODY_BYTES: usize = specboard_core::MAX_REQUEST_BYTES;
pub const REQUEST_TIME: Duration = Duration::from_secs(30);
pub const IDLE_TIME: Duration = Duration::from_secs(600);

/// One request as it came in.
pub struct Request {
    pub method: String,
    pub path: String,
    pub headers: Map<String, Value>,
    pub body: String,
}

/// The next whole request on the connection; None when the client closed or went quiet; the
/// status and the words to turn it away with when it is not one this serves.
pub fn read_request(
    connection: &mut TcpStream,
    buffer: &mut Vec<u8>,
) -> Result<Option<Request>, (u16, String)> {
    let mut chunk = vec![0u8; 1 << 16];
    loop {
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
            // The refusal is an MCP answer: in the language MCP answers in.
            let refused = {
                let _lang = specboard_core::text::bind(Some(crate::mcp::ANSWERS_IN));
                refusal(request_line[1], &headers)
            };
            if let Some(refused) = refused {
                return Err(refused);
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
        let wait = if buffer.is_empty() {
            IDLE_TIME
        } else {
            REQUEST_TIME
        };
        let _ = connection.set_read_timeout(Some(wait));
        match connection.read(&mut chunk) {
            Ok(0) | Err(_) => return Ok(None),
            Ok(read) => buffer.extend_from_slice(&chunk[..read]),
        }
    }
}

/// What is turned away whatever the body: a request a web page sent (it names its site in `Origin`,
/// which no MCP client sends), and one to anything but `/healthz` with no key, in the words the core
/// says it with. Whether a key is one of the store's, and the rest of the guard, the core and
/// `mcp.rs` ask of the whole request.
pub fn refusal(path: &str, headers: &Map<String, Value>) -> Option<(u16, String)> {
    if headers.contains_key("origin") {
        return Some((403, "requests from web pages are not served".into()));
    }
    let bearer = headers
        .get("authorization")
        .and_then(Value::as_str)
        .and_then(|value| {
            let lower = value.to_ascii_lowercase();
            lower
                .starts_with("bearer ")
                .then(|| value["bearer ".len()..].trim())
        })
        .is_some_and(|key| !key.is_empty());
    if path != "/healthz" && !bearer {
        return Some((401, key_needed()));
    }
    None
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
