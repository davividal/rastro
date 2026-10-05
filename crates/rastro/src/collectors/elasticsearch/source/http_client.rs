//! The one request rastro sends over the network.
//!
//! **HTTP/1.1 `GET`, over plain TCP or TLS, and nothing else.** No redirects followed, no
//! retries, no keep-alive, and no header that names rastro: `X-Opaque-Id` would be recorded in
//! the node's tasks and logs, and a user agent says who asked to anyone keeping an access log.
//! Hand-written rather than a client crate because this much of HTTP is thirty lines and a crate
//! would bring proxies from the environment and name resolution with it, each of which the
//! boundary in `docs/decisions.md` rules out.
//!
//! **TLS trusts the socket, not a certificate chain.** By the time a node is dialled, rastro has
//! matched the listener's inode to the node's own process and joined its network namespace, so
//! the peer is the node. Its certificate is the auto-configured one or the operator's, signed by
//! a CA this box need not hold, and checking it against one would refuse the node for nothing.
//! The handshake's signatures are still verified, so the peer holds the key it presents.
//!
//! Bounded twice: a deadline over the whole exchange, so a node that trickles bytes cannot hold
//! a run open, and a size, so a node with ten thousand indices cannot fill the box's memory.

use std::collections::BTreeMap;
use std::io::{ErrorKind, Read, Write};
use std::net::{IpAddr, SocketAddr, TcpStream};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{CryptoProvider, verify_tls12_signature, verify_tls13_signature};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{ClientConfig, ClientConnection, DigitallySignedStruct, SignatureScheme, StreamOwned};

use crate::collectors::elasticsearch::value_objects::{
    ApiCredential, HttpEndpoint, Transport, Unread,
};
use crate::collectors::inet::InetHost;

/// How long one exchange may take, connecting included.
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);

/// The largest body read, which a mapping-heavy cluster's templates stay well inside.
const DEFAULT_BODY_LIMIT: usize = 16 * 1024 * 1024;

/// The wider budget of the reads that grow with every index, `_settings` and `_mapping`.
const LARGE_TIMEOUT: Duration = Duration::from_secs(60);
const LARGE_BODY_LIMIT: usize = 64 * 1024 * 1024;

/// Room for the status line and headers on top of the body.
const HEAD_ALLOWANCE: usize = 64 * 1024;

const HEAD_END: &[u8] = b"\r\n\r\n";
const HTTP_VERSION_PREFIX: &[u8] = b"HTTP/";

/// The two answers a secured node gives a request without credentials.
const UNAUTHORISED: u16 = 401;
const FORBIDDEN: u16 = 403;
const LAST_CHUNK: &[u8] = b"0\r\n\r\n";

/// A check that the peer is still the node, run before each request.
type PeerCheck = Arc<dyn Fn() -> Result<(), Unread> + Send + Sync>;

/// A client for one node's HTTP API.
#[derive(Clone)]
pub struct HttpClient {
    timeout: Duration,
    body_limit: usize,

    /// What every request authenticates with, where the operator gave one.
    credential: Option<ApiCredential>,

    /// Whether the listener is still the node's, asked before each request.
    peer_check: Option<PeerCheck>,
}

impl std::fmt::Debug for HttpClient {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("HttpClient")
            .field("timeout", &self.timeout)
            .field("body_limit", &self.body_limit)
            .field("credential", &self.credential)
            .field("peer_check", &self.peer_check.is_some())
            .finish()
    }
}

impl HttpClient {
    pub fn new() -> Self {
        Self::bounded(DEFAULT_TIMEOUT, DEFAULT_BODY_LIMIT)
    }

    pub fn bounded(timeout: Duration, body_limit: usize) -> Self {
        Self {
            timeout,
            body_limit,
            credential: None,
            peer_check: None,
        }
    }

    /// The same client, asking `check` before each request whether the listener is still the
    /// node's, and sending nothing where it is not.
    ///
    /// Found by review: a node that exits after its listener was found leaves the port to whoever
    /// binds it next, and the next request, a credential with it, would go there. The check
    /// narrows that to the instant between it and the connection, which nothing closes.
    pub fn checking_the_peer_with(
        self,
        check: impl Fn() -> Result<(), Unread> + Send + Sync + 'static,
    ) -> Self {
        Self {
            peer_check: Some(Arc::new(check)),
            ..self
        }
    }

    /// The same client with the wider budget of the index reads.
    ///
    /// Found by review: a cluster with thousands of indices, or Fleet-sized mappings, answers
    /// `_settings` and `_mapping` past the common bound, and that is the one read that grows.
    pub fn for_large_answers(&self) -> Self {
        Self {
            timeout: self.timeout.max(LARGE_TIMEOUT),
            body_limit: self.body_limit.max(LARGE_BODY_LIMIT),
            ..self.clone()
        }
    }

    /// The same client, sending `credential` with every request.
    pub fn authenticating(self, credential: Option<ApiCredential>) -> Self {
        Self { credential, ..self }
    }

    /// The body of a `200` answer to `GET path`; any other outcome is the reason it was not had.
    pub fn get(&self, endpoint: &HttpEndpoint, path: &str) -> Result<String, Unread> {
        if let Some(check) = &self.peer_check {
            check()?;
        }
        let mut address = socket_address_of(endpoint.host(), endpoint)?;
        let raw = match self.exchange(address, endpoint.transport(), path) {
            Err(Dial::Unreachable(_)) if endpoint.fallback().is_some() => {
                address = socket_address_of(endpoint.fallback().expect("checked"), endpoint)?;
                self.exchange(address, endpoint.transport(), path)
                    .map_err(Dial::into_unread)?
            }
            other => other.map_err(Dial::into_unread)?,
        };

        // Measured on 8.15.3: a TLS-only listener closes a plaintext connection unanswered. The
        // settings said plain, so this is the blind spot `docs/decisions.md` accepts.
        if raw.is_empty() {
            return Err(Unread::new(format!(
                "{address} closed the connection without an HTTP answer, as a listener that \
                 wants TLS does, although the node's settings say it serves plain HTTP"
            )));
        }
        if !raw.starts_with(HTTP_VERSION_PREFIX) {
            return Err(Unread::new(format!(
                "{address} answered in something other than HTTP, most likely TLS, although the \
                 node's settings say it serves plain HTTP"
            )));
        }

        let answer = Answer::parse(&raw, path)?;

        if answer.status == UNAUTHORISED {
            return Err(Unread::not_read(match self.credential {
                Some(_) => "the credential given was rejected",
                None => "security is on and no credential was given (see --credentials)",
            }));
        }
        if answer.status == FORBIDDEN {
            return Err(Unread::not_read(format!(
                "the node refused GET {path} to the credential given"
            )));
        }
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

    fn exchange(
        &self,
        address: SocketAddr,
        transport: Transport,
        path: &str,
    ) -> Result<Vec<u8>, Dial> {
        let timed_out = || Unread::new(format!("GET {path} timed out after {:?}", self.timeout));

        let socket = TcpStream::connect_timeout(&address, self.timeout).map_err(|error| {
            let unread = Unread::new(format!("could not connect to {address}: {error}"));
            match error.kind() {
                ErrorKind::TimedOut => Dial::Failed(timed_out()),
                ErrorKind::ConnectionRefused
                | ErrorKind::AddrNotAvailable
                | ErrorKind::NetworkUnreachable
                | ErrorKind::HostUnreachable => Dial::Unreachable(unread),
                _ => Dial::Failed(unread),
            }
        })?;

        match transport {
            Transport::Plain => self.converse(socket, address, path).map_err(Dial::Failed),
            Transport::Tls => {
                let peer = ServerName::IpAddress(address.ip().into());
                let connection =
                    ClientConnection::new(tls_configuration(), peer).map_err(|error| {
                        Dial::Failed(Unread::new(format!(
                            "TLS to {address} could not start: {error}"
                        )))
                    })?;
                self.converse(StreamOwned::new(connection, socket), address, path)
                    .map_err(Dial::Failed)
            }
        }
    }

    /// Sends the request on `stream` and reads the answer back, within the deadline and the bound.
    fn converse(
        &self,
        mut stream: impl Socket,
        address: SocketAddr,
        path: &str,
    ) -> Result<Vec<u8>, Unread> {
        let deadline = Instant::now() + self.timeout;
        let timed_out = || Unread::new(format!("GET {path} timed out after {:?}", self.timeout));

        let authorization = self
            .credential
            .as_ref()
            .map(|credential| format!("Authorization: {}\r\n", credential.authorization()))
            .unwrap_or_default();
        let request = format!(
            "GET {path} HTTP/1.1\r\nHost: {address}\r\nAccept: application/json\r\n\
             {authorization}Connection: close\r\n\r\n"
        );
        // The read deadline too, before the first write: over TLS that write drives the handshake,
        // which reads, and a listener that never answers it would otherwise hold the run.
        stream
            .tcp()
            .set_write_timeout(Some(self.timeout))
            .and_then(|()| stream.tcp().set_read_timeout(Some(self.timeout)))
            .and_then(|()| stream.write_all(request.as_bytes()))
            .and_then(|()| stream.flush())
            .map_err(|error| match error.kind() {
                ErrorKind::WouldBlock | ErrorKind::TimedOut => timed_out(),
                _ if demands_a_client_certificate(&error) => client_certificate_demanded(),
                _ => Unread::new(format!("GET {path} could not be sent: {error}")),
            })?;

        let mut raw = Vec::new();
        let mut buffer = [0_u8; 8192];
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(timed_out());
            }
            stream
                .tcp()
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
                // A TLS peer that closes without `close_notify`: what arrived is checked whole
                // by the answer's own length, so a cut answer is still caught.
                Err(error) if error.kind() == ErrorKind::UnexpectedEof => return Ok(raw),
                Err(error) if demands_a_client_certificate(&error) => {
                    return Err(client_certificate_demanded());
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

/// Whether the TLS peer refused the handshake for want of a client certificate, as a node with
/// `client_authentication: required` does, measured on 8.19.22.
fn demands_a_client_certificate(error: &std::io::Error) -> bool {
    use rustls::AlertDescription;

    matches!(
        error
            .get_ref()
            .and_then(|inner| inner.downcast_ref::<rustls::Error>()),
        Some(rustls::Error::AlertReceived(
            AlertDescription::CertificateRequired | AlertDescription::BadCertificate
        ))
    )
}

/// Mutual TLS is the node's configuration, and rastro has no certificate to present: the box
/// keeping it out, not a failure to read, found by the third domain review.
fn client_certificate_demanded() -> Unread {
    Unread::not_read("the node demands a client certificate, which rastro cannot present")
}

/// A connection the exchange can bound by time: plain TCP, or TLS over it.
trait Socket: Read + Write {
    fn tcp(&self) -> &TcpStream;
}

impl Socket for TcpStream {
    fn tcp(&self) -> &TcpStream {
        self
    }
}

impl Socket for StreamOwned<ClientConnection, TcpStream> {
    fn tcp(&self) -> &TcpStream {
        &self.sock
    }
}

/// One TLS configuration for the run, which trusts the node's socket rather than its chain.
fn tls_configuration() -> Arc<ClientConfig> {
    static CONFIGURATION: OnceLock<Arc<ClientConfig>> = OnceLock::new();

    Arc::clone(CONFIGURATION.get_or_init(|| {
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let configuration = ClientConfig::builder_with_provider(Arc::clone(&provider))
            .with_safe_default_protocol_versions()
            .expect("ring supports TLS 1.2 and 1.3")
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(TheNodesOwnSocket { provider }))
            .with_no_client_auth();
        Arc::new(configuration)
    }))
}

/// Accepts the certificate of the peer rastro already knows is the node, and verifies that the
/// peer holds its key. See the module's account of why the chain is not checked.
#[derive(Debug)]
struct TheNodesOwnSocket {
    provider: Arc<CryptoProvider>,
}

impl ServerCertVerifier for TheNodesOwnSocket {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        certificate: &CertificateDer<'_>,
        signature: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls12_signature(
            message,
            certificate,
            signature,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        certificate: &CertificateDer<'_>,
        signature: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls13_signature(
            message,
            certificate,
            signature,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider
            .signature_verification_algorithms
            .supported_schemes()
    }
}

impl Default for HttpClient {
    fn default() -> Self {
        Self::new()
    }
}

/// How a dial failed: before anything reached the listener, which another address may still
/// reach, or after.
enum Dial {
    Unreachable(Unread),
    Failed(Unread),
}

impl Dial {
    fn into_unread(self) -> Unread {
        match self {
            Self::Unreachable(unread) | Self::Failed(unread) => unread,
        }
    }
}

/// `host` at the endpoint's port as a socket address. A host always came from a kernel table, so
/// it is an address and never a name, and nothing is resolved.
fn socket_address_of(host: &InetHost, endpoint: &HttpEndpoint) -> Result<SocketAddr, Unread> {
    let address: IpAddr = host.as_str().parse().map_err(|_| {
        Unread::new(format!(
            "{} is not an address, and rastro resolves no names",
            host.as_str()
        ))
    })?;

    Ok(SocketAddr::new(address, endpoint.port().as_u16()))
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
