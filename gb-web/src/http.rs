//! Minimal HTTP/1.1 plumbing: bounded request parsing and response writing.
//!
//! Only what the game library service needs: a request line, headers, and a
//! `Content-Length` body. Chunked request bodies are not supported (they need
//! not be, Appendix D.1).

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

/// Largest request header block accepted (Security Considerations).
pub const MAX_HEADER_BYTES: usize = 64 * 1024;
/// Largest request body buffered; longer uploads are cut here and the upload
/// handler answers `413`.
pub const MAX_BODY_BYTES: usize = 9 * 1024 * 1024;
/// How long a stalled connection may block a worker thread.
const READ_TIMEOUT: Duration = Duration::from_secs(60);

/// A parsed HTTP request.
pub struct Request {
    pub method: String,
    /// Path with the query string removed.
    pub path: String,
    /// Raw query string (without the `?`).
    pub query: String,
    headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    /// True when the body was longer than [`MAX_BODY_BYTES`] and was cut.
    pub body_truncated: bool,
}

impl Request {
    /// A header value by lower-case name, if present.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }

    /// The query parameters, percent-decoded, in order.
    pub fn query_params(&self) -> Vec<(String, String)> {
        parse_query(&self.query)
    }
}

/// Why a request could not be read.
#[derive(Debug)]
pub enum ParseError {
    /// The header block exceeded [`MAX_HEADER_BYTES`].
    TooLarge,
    /// The bytes were not a valid HTTP request.
    Malformed,
    /// An I/O error occurred on the socket.
    Io,
}

/// Read one request. `Ok(None)` means the peer closed without sending one.
pub fn read_request(stream: &mut TcpStream) -> Result<Option<Request>, ParseError> {
    stream.set_read_timeout(Some(READ_TIMEOUT)).ok();

    let mut buf = Vec::with_capacity(2048);
    let head_end = loop {
        if let Some(pos) = find(&buf, b"\r\n\r\n") {
            break pos;
        }
        if buf.len() > MAX_HEADER_BYTES {
            return Err(ParseError::TooLarge);
        }
        let mut chunk = [0u8; 4096];
        match stream.read(&mut chunk) {
            Ok(0) => {
                if buf.is_empty() {
                    return Ok(None);
                }
                return Err(ParseError::Malformed);
            }
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
            Err(_) => return Err(ParseError::Io),
        }
    };

    let head = std::str::from_utf8(&buf[..head_end]).map_err(|_| ParseError::Malformed)?;
    let mut lines = head.split("\r\n");
    let request_line = lines.next().ok_or(ParseError::Malformed)?;
    let mut words = request_line.split_whitespace();
    let method = words
        .next()
        .ok_or(ParseError::Malformed)?
        .to_ascii_uppercase();
    let target = words.next().ok_or(ParseError::Malformed)?;
    let (path, query) = match target.split_once('?') {
        Some((p, q)) => (p.to_string(), q.to_string()),
        None => (target.to_string(), String::new()),
    };

    let mut headers = Vec::new();
    for line in lines {
        if line.is_empty() {
            continue;
        }
        let (k, v) = line.split_once(':').ok_or(ParseError::Malformed)?;
        headers.push((k.trim().to_ascii_lowercase(), v.trim().to_string()));
    }

    let content_length = headers
        .iter()
        .find(|(k, _)| k == "content-length")
        .map(|(_, v)| v.parse::<usize>().map_err(|_| ParseError::Malformed))
        .transpose()?
        .unwrap_or(0);

    // Honour `Expect: 100-continue` so clients that wait for it (curl for
    // non-trivial bodies) do not stall before sending the upload.
    if content_length > 0
        && headers
            .iter()
            .any(|(k, v)| k == "expect" && v.eq_ignore_ascii_case("100-continue"))
    {
        let _ = stream.write_all(b"HTTP/1.1 100 Continue\r\n\r\n");
        let _ = stream.flush();
    }

    let want = content_length.min(MAX_BODY_BYTES);
    let mut body = buf[head_end + 4..].to_vec();
    body.truncate(want);
    while body.len() < want {
        let mut chunk = [0u8; 8192];
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                let room = want - body.len();
                body.extend_from_slice(&chunk[..n.min(room)]);
            }
            Err(_) => break,
        }
    }

    Ok(Some(Request {
        method,
        path,
        query,
        headers,
        body,
        body_truncated: content_length > MAX_BODY_BYTES,
    }))
}

/// An HTTP response, written with `Connection: close`.
pub struct Response {
    pub status: u16,
    pub content_type: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Response {
    /// A response with an explicit content type.
    pub fn new(status: u16, content_type: &str, body: Vec<u8>) -> Response {
        Response {
            status,
            content_type: content_type.to_string(),
            headers: Vec::new(),
            body,
        }
    }

    /// An HTML response.
    pub fn html(status: u16, body: &str) -> Response {
        Response::new(status, "text/html; charset=utf-8", body.as_bytes().to_vec())
    }

    /// A JSON response.
    pub fn json(status: u16, body: String) -> Response {
        Response::new(status, "application/json; charset=utf-8", body.into_bytes())
    }

    /// The standard `{"error", "code"}` body.
    pub fn error(status: u16, message: &str, code: &str) -> Response {
        Response::json(status, crate::json::error_body(message, code))
    }

    /// A response without content.
    pub fn empty(status: u16) -> Response {
        Response::new(status, "text/plain; charset=utf-8", Vec::new())
    }

    /// Add a response header.
    pub fn with_header(mut self, name: &str, value: impl Into<String>) -> Response {
        self.headers.push((name.to_string(), value.into()));
        self
    }

    /// Write the status line, headers and body, then flush.
    pub fn write_to(self, stream: &mut TcpStream) -> std::io::Result<()> {
        let mut head = format!(
            "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n",
            self.status,
            reason_phrase(self.status),
            self.content_type,
            self.body.len()
        );
        for (k, v) in &self.headers {
            head.push_str(k);
            head.push_str(": ");
            head.push_str(v);
            head.push_str("\r\n");
        }
        head.push_str("\r\n");
        stream.write_all(head.as_bytes())?;
        stream.write_all(&self.body)?;
        stream.flush()
    }
}

fn reason_phrase(status: u16) -> &'static str {
    match status {
        200 => "OK",
        201 => "Created",
        204 => "No Content",
        400 => "Bad Request",
        404 => "Not Found",
        405 => "Method Not Allowed",
        409 => "Conflict",
        413 => "Content Too Large",
        415 => "Unsupported Media Type",
        422 => "Unprocessable Entity",
        500 => "Internal Server Error",
        _ => "OK",
    }
}

/// Position of `needle` within `haystack`.
pub fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || needle.len() > haystack.len() {
        return None;
    }
    haystack.windows(needle.len()).position(|w| w == needle)
}

fn parse_query(query: &str) -> Vec<(String, String)> {
    query
        .split('&')
        .filter(|p| !p.is_empty())
        .map(|pair| match pair.split_once('=') {
            Some((k, v)) => (url_decode(k), url_decode(v)),
            None => (url_decode(pair), String::new()),
        })
        .collect()
}

fn url_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b'%' if i + 2 < bytes.len() => match (hex_val(bytes[i + 1]), hex_val(bytes[i + 2])) {
                (Some(hi), Some(lo)) => {
                    out.push((hi << 4) | lo);
                    i += 3;
                }
                _ => {
                    out.push(bytes[i]);
                    i += 1;
                }
            },
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_subslice() {
        assert_eq!(find(b"abc--x--y", b"--"), Some(3));
        assert_eq!(find(b"abc", b"z"), None);
    }

    #[test]
    fn decodes_query() {
        let params = parse_query("q=hello+world&cgb=none&x=%41%2f");
        assert_eq!(params[0], ("q".to_string(), "hello world".to_string()));
        assert_eq!(params[1], ("cgb".to_string(), "none".to_string()));
        assert_eq!(params[2], ("x".to_string(), "A/".to_string()));
    }
}
