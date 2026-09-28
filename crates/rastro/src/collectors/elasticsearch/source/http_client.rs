//! The one request rastro sends over the network.
//!
//! **HTTP/1.1 `GET`, plain, and nothing else.** No TLS, no redirects followed, no retries, no
//! keep-alive, and no header that names rastro: `X-Opaque-Id` would be recorded in the node's
//! tasks and logs, and a user agent says who asked to anyone keeping an access log. Hand-written
//! rather than a client crate because this much of HTTP is thirty lines and a crate would bring
//! TLS, proxies from the environment and name resolution with it, each of which the boundary in
//! `docs/decisions.md` rules out.
//!
//! Bounded twice: a deadline over the whole exchange, so a node that trickles bytes cannot hold
//! a run open, and a size, so a node with ten thousand indices cannot fill the box's memory.

use std::collections::BTreeMap;
use std::io::{ErrorKind, Read, Write};
use std::net::{IpAddr, SocketAddr, TcpStream};
use std::time::{Duration, Instant};

use crate::collectors::elasticsearch::value_objects::{HttpEndpoint, Unread};

/// How long one exchange may take, connecting included.
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);

/// The largest body read, which a mapping-heavy cluster's templates stay well inside.
const DEFAULT_BODY_LIMIT: usize = 16 * 1024 * 1024;

/// Room for the status line and headers on top of the body.
const HEAD_ALLOWANCE: usize = 64 * 1024;

const HEAD_END: &[u8] = b"\r\n\r\n";
const LAST_CHUNK: &[u8] = b"0\r\n\r\n";

/// A client for one node's HTTP API.
#[derive(Debug, Clone)]
pub struct HttpClient {
    timeout: Duration,
    body_limit: usize,
}

impl HttpClient {
    pub fn new() -> Self {
        Self::bounded(DEFAULT_TIMEOUT, DEFAULT_BODY_LIMIT)
    }

    pub fn bounded(timeout: Duration, body_limit: usize) -> Self {
        Self {
            timeout,
            body_limit,
        }
    }

    /// The body of a `200` answer to `GET path`; any other outcome is the reason it was not had.
    pub fn get(&self, endpoint: &HttpEndpoint, path: &str) -> Result<String, Unread> {
        let address = socket_address_of(endpoint)?;
        let raw = self.exchange(address, path)?;
        let answer = Answer::parse(&raw, path)?;

        if answer.status != 200 {
            return Err(Unread::new(format!(
                "the node answered {} to GET {path}",
                answer.status
            )));
        }

        let body = answer.body()?;
        if body.len() > self.body_limit {
            return Err(Unread::new(format!(
                "the answer to GET {path} is larger than {} bytes",
                self.body_limit
            )));
        }

        String::from_utf8(body)
            .map_err(|_| Unread::new(format!("the answer to GET {path} is not UTF-8")))
    }

    fn exchange(&self, address: SocketAddr, path: &str) -> Result<Vec<u8>, Unread> {
        let deadline = Instant::now() + self.timeout;
        let timed_out = || Unread::new(format!("GET {path} timed out after {:?}", self.timeout));

        let mut stream = TcpStream::connect_timeout(&address, self.timeout).map_err(|error| {
            match error.kind() {
                ErrorKind::TimedOut => timed_out(),
                _ => Unread::new(format!("could not connect to {address}: {error}")),
            }
        })?;

        let request = format!(
            "GET {path} HTTP/1.1\r\nHost: {address}\r\nAccept: application/json\r\n\
             Connection: close\r\n\r\n"
        );
        stream
            .set_write_timeout(Some(self.timeout))
            .and_then(|()| stream.write_all(request.as_bytes()))
            .map_err(|error| Unread::new(format!("GET {path} could not be sent: {error}")))?;

        let mut raw = Vec::new();
        let mut buffer = [0_u8; 8192];
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(timed_out());
            }
            stream
                .set_read_timeout(Some(remaining))
                .map_err(|error| Unread::new(format!("GET {path}: {error}")))?;

            match stream.read(&mut buffer) {
                Ok(0) => return Ok(raw),
                Ok(read) => raw.extend_from_slice(&buffer[..read]),
                Err(error)
                    if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) =>
                {
                    return Err(timed_out());
                }
                Err(error) => {
                    return Err(Unread::new(format!(
                        "GET {path} failed while reading: {error}"
                    )));
                }
            }

            if raw.len() > self.body_limit + HEAD_ALLOWANCE {
                return Err(Unread::new(format!(
                    "the answer to GET {path} is larger than {} bytes",
                    self.body_limit
                )));
            }
            if is_complete(&raw) {
                return Ok(raw);
            }
        }
    }
}

impl Default for HttpClient {
    fn default() -> Self {
        Self::new()
    }
}

/// The endpoint as a socket address. Its host always came from a kernel table, so it is an
/// address and never a name, and nothing is resolved.
fn socket_address_of(endpoint: &HttpEndpoint) -> Result<SocketAddr, Unread> {
    let host: IpAddr = endpoint.host().as_str().parse().map_err(|_| {
        Unread::new(format!(
            "{} is not an address, and rastro resolves no names",
            endpoint.host().as_str()
        ))
    })?;

    Ok(SocketAddr::new(host, endpoint.port().as_u16()))
}

/// Whether the answer is all here, for a node that keeps the connection open after it.
fn is_complete(raw: &[u8]) -> bool {
    let Some(head_end) = find(raw, HEAD_END) else {
        return false;
    };
    let Ok(answer) = Answer::parse(raw, "") else {
        return false;
    };
    let body = &raw[head_end + HEAD_END.len()..];

    match (answer.is_chunked(), answer.content_length()) {
        (true, _) => body.ends_with(LAST_CHUNK),
        (false, Some(length)) => body.len() >= length,
        (false, None) => false,
    }
}

/// A response, split into what this client reads of it.
struct Answer<'raw> {
    status: u16,
    headers: BTreeMap<String, String>,
    body: &'raw [u8],
    path: &'raw str,
}

impl<'raw> Answer<'raw> {
    fn parse(raw: &'raw [u8], path: &'raw str) -> Result<Self, Unread> {
        let head_end = find(raw, HEAD_END).ok_or_else(|| {
            Unread::new(format!(
                "the answer to GET {path} ended before its headers did"
            ))
        })?;
        let head = std::str::from_utf8(&raw[..head_end]).map_err(|_| {
            Unread::new(format!(
                "the answer to GET {path} has headers that are not text"
            ))
        })?;

        let mut lines = head.split("\r\n");
        let status = lines
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .and_then(|code| code.parse().ok())
            .ok_or_else(|| Unread::new(format!("the answer to GET {path} has no status line")))?;

        let headers = lines
            .filter_map(|line| line.split_once(':'))
            .map(|(name, value)| (name.trim().to_ascii_lowercase(), value.trim().to_owned()))
            .collect();

        Ok(Self {
            status,
            headers,
            body: &raw[head_end + HEAD_END.len()..],
            path,
        })
    }

    fn is_chunked(&self) -> bool {
        self.headers
            .get("transfer-encoding")
            .is_some_and(|encoding| encoding.eq_ignore_ascii_case("chunked"))
    }

    fn content_length(&self) -> Option<usize> {
        self.headers.get("content-length")?.parse().ok()
    }

    fn body(&self) -> Result<Vec<u8>, Unread> {
        let truncated = || Unread::new(format!("the answer to GET {} was cut short", self.path));

        if self.is_chunked() {
            return dechunk(self.body).ok_or_else(truncated);
        }

        match self.content_length() {
            Some(length) => self
                .body
                .get(..length)
                .map(<[u8]>::to_vec)
                .ok_or_else(truncated),
            None => Ok(self.body.to_vec()),
        }
    }
}

/// Joins a chunked body, or nothing where it is malformed or incomplete.
fn dechunk(mut rest: &[u8]) -> Option<Vec<u8>> {
    let mut body = Vec::new();
    loop {
        let line_end = find(rest, b"\r\n")?;
        let size_text = std::str::from_utf8(&rest[..line_end]).ok()?;
        let size = usize::from_str_radix(size_text.split(';').next()?.trim(), 16).ok()?;
        rest = &rest[line_end + 2..];

        if size == 0 {
            return Some(body);
        }

        body.extend_from_slice(rest.get(..size)?);
        rest = rest.get(size + 2..)?;
    }
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}
