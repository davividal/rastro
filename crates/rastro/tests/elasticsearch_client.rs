//! The one request rastro sends over the network, against a listener the test holds.
//!
//! A real socket rather than a mock, because what is being pinned is bytes on the wire: the
//! request the node sees, and how the answer is read back.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use rastro::collectors::elasticsearch::{HttpClient, HttpEndpoint, Transport};
use rastro::collectors::inet::{InetHost, PortNumber};

mod support;

use support::tls_listener;

/// Serves one connection with `response`, and hands back the request it was sent.
fn serve_once(response: Vec<u8>) -> (HttpEndpoint, mpsc::Receiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    let port = listener.local_addr().expect("a bound port").port();
    let (sender, receiver) = mpsc::channel();

    thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("one connection");
        let mut request = Vec::new();
        let mut byte = [0_u8; 1];
        while !request.ends_with(b"\r\n\r\n") && stream.read(&mut byte).unwrap_or(0) == 1 {
            request.push(byte[0]);
        }
        let _ = sender.send(String::from_utf8_lossy(&request).into_owned());
        let _ = stream.write_all(&response);
    });

    let endpoint = HttpEndpoint::new(
        InetHost::new("127.0.0.1").expect("a host"),
        PortNumber::parse(&port.to_string()).expect("a port"),
    );
    (endpoint, receiver)
}

#[test]
fn get_returns_the_body_of_a_200() {
    // Arrange
    let (endpoint, _) = serve_once(
        b"HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: 17\r\n\r\n{\"cluster\":\"one\"}"
            .to_vec(),
    );

    // Act
    let body = HttpClient::new().get(&endpoint, "/").expect("an answer");

    // Assert
    assert_eq!(body, "{\"cluster\":\"one\"}");
}

#[test]
fn get_sends_a_bare_get_that_names_nothing_about_rastro() {
    // Arrange: `X-Opaque-Id` would land in the node's tasks and logs, and a user agent says who
    // asked, which the node has no use for and an access log would keep.
    let (endpoint, request) =
        serve_once(b"HTTP/1.1 200 OK\r\ncontent-length: 2\r\n\r\n{}".to_vec());

    // Act
    HttpClient::new()
        .get(&endpoint, "/_cluster/settings?flat_settings=true")
        .expect("an answer");

    // Assert
    let request = request.recv().expect("the request");
    let port = endpoint.port().as_u16();
    assert_eq!(
        request,
        format!(
            "GET /_cluster/settings?flat_settings=true HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n\
             Accept: application/json\r\nConnection: close\r\n\r\n"
        )
    );
}

#[test]
fn get_reads_a_chunked_body() {
    // Arrange: 8.x streams its larger answers.
    let (endpoint, _) = serve_once(
        b"HTTP/1.1 200 OK\r\ntransfer-encoding: chunked\r\n\r\n4\r\n{\"a\"\r\n5\r\n:\"b\"}\r\n0\r\n\r\n"
            .to_vec(),
    );

    // Act
    let body = HttpClient::new().get(&endpoint, "/").expect("an answer");

    // Assert
    assert_eq!(body, "{\"a\":\"b\"}");
}

#[test]
fn get_refuses_any_status_but_200() {
    // Arrange
    let (endpoint, _) =
        serve_once(b"HTTP/1.1 404 Not Found\r\ncontent-length: 2\r\n\r\n{}".to_vec());

    // Act
    let unread = HttpClient::new()
        .get(&endpoint, "/_index_template")
        .expect_err("a 404");

    // Assert
    assert!(unread.reason().contains("404"), "{}", unread.reason());
    assert!(
        unread.reason().contains("/_index_template"),
        "{}",
        unread.reason()
    );
}

#[test]
fn get_refuses_a_body_larger_than_its_bound() {
    // Arrange
    let body = "x".repeat(4096);
    let (endpoint, _) =
        serve_once(format!("HTTP/1.1 200 OK\r\ncontent-length: 4096\r\n\r\n{body}").into_bytes());

    // Act
    let unread = HttpClient::bounded(Duration::from_secs(5), 1024)
        .get(&endpoint, "/")
        .expect_err("an oversized answer");

    // Assert
    assert!(unread.reason().contains("1024"), "{}", unread.reason());
}

#[test]
fn get_gives_up_on_a_node_that_never_answers() {
    // Arrange: a listener that accepts and says nothing.
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    let port = listener.local_addr().expect("a bound port").port();
    let holder = thread::spawn(move || {
        let connection = listener.accept();
        thread::sleep(Duration::from_secs(2));
        drop(connection);
    });
    let endpoint = HttpEndpoint::new(
        InetHost::new("127.0.0.1").expect("a host"),
        PortNumber::parse(&port.to_string()).expect("a port"),
    );
    let started = Instant::now();

    // Act
    let unread = HttpClient::bounded(Duration::from_millis(300), 1024)
        .get(&endpoint, "/")
        .expect_err("a silent node");

    // Assert
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "{:?}",
        started.elapsed()
    );
    assert!(unread.reason().contains("time"), "{}", unread.reason());
    holder.join().expect("the holder");
}

#[test]
fn get_refuses_a_listener_nothing_holds() {
    // Arrange: a port bound and released, so nothing accepts on it.
    let port = TcpListener::bind("127.0.0.1:0")
        .expect("a loopback port")
        .local_addr()
        .expect("a bound port")
        .port();
    let endpoint = HttpEndpoint::new(
        InetHost::new("127.0.0.1").expect("a host"),
        PortNumber::parse(&port.to_string()).expect("a port"),
    );

    // Act
    let unread = HttpClient::new()
        .get(&endpoint, "/")
        .expect_err("a refused connection");

    // Assert
    assert!(unread.reason().contains("connect"), "{}", unread.reason());
}

#[test]
fn get_reports_a_node_that_wants_credentials_as_not_read() {
    // Arrange: what 8.x answers without them, measured on 8.19.22 (cell 02). Security switched on
    // is the node's configuration, not a failure of rastro's, so the node is not an error.
    let (endpoint, _) = serve_once(
        b"HTTP/1.1 401 Unauthorized\r\nWWW-Authenticate: Basic realm=\"security\"\r\ncontent-length: 2\r\n\r\n{}"
            .to_vec(),
    );

    // Act
    let unread = HttpClient::new().get(&endpoint, "/").expect_err("a 401");

    // Assert
    assert!(unread.is_not_read());
    assert_eq!(
        unread.reason(),
        "security is on and no credential was given (see --credentials)"
    );
}

#[test]
fn get_reports_a_node_that_forbids_the_read_as_not_read() {
    // Arrange: a credential without the privilege for one read.
    let (endpoint, _) =
        serve_once(b"HTTP/1.1 403 Forbidden\r\ncontent-length: 2\r\n\r\n{}".to_vec());

    // Act
    let unread = HttpClient::new().get(&endpoint, "/").expect_err("a 403");

    // Assert
    assert!(unread.is_not_read());
    assert!(unread.reason().contains("refused"), "{}", unread.reason());
}

#[test]
fn get_reports_a_node_that_answers_with_a_server_error_as_an_error() {
    // Arrange
    let (endpoint, _) =
        serve_once(b"HTTP/1.1 500 Internal Server Error\r\ncontent-length: 2\r\n\r\n{}".to_vec());

    // Act
    let unread = HttpClient::new().get(&endpoint, "/").expect_err("a 500");

    // Assert
    assert!(!unread.is_not_read());
}

#[test]
fn get_reports_a_listener_that_closes_without_answering_as_wanting_tls() {
    // Arrange: measured on 8.15.3, a TLS-only listener closes a plaintext connection with no
    // answer at all, and logs a WARN, which is why the settings are read first.
    let (endpoint, _) = serve_once(Vec::new());

    // Act
    let unread = HttpClient::new()
        .get(&endpoint, "/")
        .expect_err("no answer");

    // Assert
    assert!(unread.reason().contains("TLS"), "{}", unread.reason());
}

#[test]
fn get_reports_a_listener_that_answers_in_tls_as_wanting_tls() {
    // Arrange: a TLS alert record, which is what some TLS stacks send to a plaintext client.
    let (endpoint, _) = serve_once(vec![0x15, 0x03, 0x03, 0x00, 0x02, 0x02, 0x46]);

    // Act
    let unread = HttpClient::new()
        .get(&endpoint, "/")
        .expect_err("a TLS alert");

    // Assert
    assert!(unread.reason().contains("TLS"), "{}", unread.reason());
}

/// An endpoint on a listener that accepts and hands each connection to `answer`.
fn answering_with(answer: impl FnOnce(std::net::TcpStream) + Send + 'static) -> HttpEndpoint {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    let port = listener.local_addr().expect("a bound port").port();
    thread::spawn(move || {
        if let Ok((stream, _)) = listener.accept() {
            answer(stream);
        }
    });
    HttpEndpoint::new(
        InetHost::new("127.0.0.1").expect("a host"),
        PortNumber::parse(&port.to_string()).expect("a port"),
    )
}

#[test]
fn get_gives_up_on_a_node_that_trickles_its_answer_past_the_deadline() {
    // Arrange: a byte every 50 ms never completes a status line, and each read succeeds, so a
    // bound per read alone would wait forever.
    let endpoint = answering_with(|mut stream| {
        for _ in 0..100 {
            if stream.write_all(b"H").is_err() {
                return;
            }
            thread::sleep(Duration::from_millis(50));
        }
    });
    let started = Instant::now();

    // Act
    let unread = HttpClient::bounded(Duration::from_millis(300), 1024)
        .get(&endpoint, "/")
        .expect_err("a trickling node");

    // Assert
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "{:?}",
        started.elapsed()
    );
    assert!(unread.reason().contains("timed out"), "{}", unread.reason());
}

#[test]
fn get_stops_reading_an_answer_that_outgrows_its_bound() {
    // Arrange: an answer with no length on a connection that never closes, 64 KiB every 10 ms.
    // Only the bound applied while reading stops it early; without it the read runs to the
    // deadline with everything so far held in memory. Paced on purpose: loopback delivered a
    // declared 100 MiB in 80 ms, which made the two outcomes look alike.
    let endpoint = answering_with(|mut stream| {
        let _ = stream.write_all(b"HTTP/1.1 200 OK\r\n\r\n");
        let chunk = vec![b'x'; 64 * 1024];
        while stream.write_all(&chunk).is_ok() {
            thread::sleep(Duration::from_millis(10));
        }
    });
    let started = Instant::now();

    // Act
    let unread = HttpClient::bounded(Duration::from_secs(3), 1024)
        .get(&endpoint, "/")
        .expect_err("an oversized answer");

    // Assert
    assert!(
        unread.reason().contains("larger than 1024"),
        "{}",
        unread.reason()
    );
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "{:?}",
        started.elapsed()
    );
}

#[test]
fn get_resolves_no_names() {
    // Arrange: a host always comes from a kernel table, and a name would mean a lookup, which
    // the network boundary rules out.
    let endpoint = HttpEndpoint::new(
        InetHost::new("localhost").expect("a host"),
        PortNumber::parse("9200").expect("a port"),
    );

    // Act
    let unread = HttpClient::new().get(&endpoint, "/").expect_err("a name");

    // Assert
    assert!(
        unread.reason().contains("resolves no names"),
        "{}",
        unread.reason()
    );
}

#[test]
fn get_reads_an_answer_without_a_length_to_the_end_of_the_connection() {
    // Act
    let (endpoint, _) = serve_once(b"HTTP/1.1 200 OK\r\n\r\n{\"closed\":true}".to_vec());
    let body = HttpClient::new().get(&endpoint, "/").expect("an answer");

    // Assert
    assert_eq!(body, "{\"closed\":true}");
}

#[test]
fn get_refuses_an_answer_with_no_status_code() {
    // Act
    let (endpoint, _) = serve_once(b"HTTP/1.1\r\n\r\n{}".to_vec());
    let unread = HttpClient::new()
        .get(&endpoint, "/")
        .expect_err("no status");

    // Assert
    assert!(
        unread.reason().contains("no status line"),
        "{}",
        unread.reason()
    );
}

#[test]
fn get_refuses_headers_that_are_not_text() {
    // Act
    let (endpoint, _) = serve_once(b"HTTP/1.1 200 OK\r\nX-Odd: \xff\r\n\r\n{}".to_vec());
    let unread = HttpClient::new()
        .get(&endpoint, "/")
        .expect_err("binary headers");

    // Assert
    assert!(unread.reason().contains("not text"), "{}", unread.reason());
}

fn loopback(port: u16) -> HttpEndpoint {
    HttpEndpoint::new(
        InetHost::new("127.0.0.1").expect("a host"),
        PortNumber::parse(&port.to_string()).expect("a port"),
    )
}

/// A plain HTTP listener as a node's is, answering a ClientHello with a status line.
fn plain_http_listener() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    let port = listener.local_addr().expect("a bound port").port();
    thread::spawn(move || {
        for mut stream in listener.incoming().flatten() {
            let mut hello = [0_u8; 512];
            let _ = stream.read(&mut hello);
            let _ = stream.write_all(b"HTTP/1.1 400 Bad Request\r\ncontent-length: 0\r\n\r\n");
        }
    });
    port
}

#[test]
fn transport_of_a_listener_that_completes_a_handshake_is_tls() {
    // Arrange: measured on 8.19.22 and 9.5.4 with security on, a handshake and nothing after it
    // leaves no line in the node's log.
    let port = tls_listener::serving(|_| Vec::new());

    // Act
    let transport = HttpClient::new().transport_of(&loopback(port));

    // Assert
    assert_eq!(transport, Ok(Transport::Tls));
}

#[test]
fn transport_of_a_listener_that_answers_a_handshake_in_http_is_plain() {
    // Arrange: measured on 7.17.29, 8.19.22 and 9.5.4, a plain node sent a ClientHello answers
    // in HTTP and logs nothing, where plaintext sent to a TLS node is a WARN in its log.
    let port = plain_http_listener();

    // Act
    let transport = HttpClient::new().transport_of(&loopback(port));

    // Assert
    assert_eq!(transport, Ok(Transport::Plain));
}

#[test]
fn transport_of_a_listener_that_never_answers_gives_up_within_the_deadline() {
    // Arrange
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    let port = listener.local_addr().expect("a bound port").port();
    thread::spawn(move || {
        let held = listener.accept();
        thread::sleep(Duration::from_secs(30));
        drop(held);
    });
    let started = Instant::now();

    // Act
    let unread = HttpClient::bounded(Duration::from_millis(300), 1024)
        .transport_of(&loopback(port))
        .expect_err("no answer");

    // Assert
    assert!(unread.reason().contains("timed out"), "{}", unread.reason());
    assert!(started.elapsed() < Duration::from_secs(3));
}

#[test]
fn get_reads_a_node_on_tls_whatever_certificate_it_presents() {
    // Arrange: a self-signed certificate for another name, which a CA check would refuse. The
    // socket is the node's by its inode, so its certificate adds nothing a chain could vouch for.
    let port = tls_listener::serving(|_| {
        b"HTTP/1.1 200 OK\r\ncontent-length: 17\r\n\r\n{\"cluster\":\"one\"}".to_vec()
    });

    // Act
    let body = HttpClient::new()
        .get(&loopback(port).over(Transport::Tls), "/")
        .expect("an answer over TLS");

    // Assert
    assert_eq!(body, "{\"cluster\":\"one\"}");
}

#[test]
fn get_over_tls_sends_the_same_bare_get() {
    // Arrange
    let (sender, receiver) = mpsc::channel();
    let port = tls_listener::serving(move |request| {
        let _ = sender.send(request.to_owned());
        b"HTTP/1.1 200 OK\r\ncontent-length: 2\r\n\r\n{}".to_vec()
    });

    // Act
    HttpClient::new()
        .get(&loopback(port).over(Transport::Tls), "/_cluster/settings")
        .expect("an answer over TLS");

    // Assert
    let request = receiver.recv().expect("the request");
    assert!(
        request.starts_with("GET /_cluster/settings HTTP/1.1\r\n"),
        "{request}"
    );
    assert!(!request.to_lowercase().contains("user-agent"), "{request}");
}

#[test]
fn get_over_tls_reads_an_answer_whose_peer_closes_without_close_notify() {
    // Arrange: an answer delimited by the close alone, the peer skipping `close_notify`.
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    let port = listener.local_addr().expect("a bound port").port();
    thread::spawn(move || {
        if let Some((_, mut tls)) = listener
            .accept()
            .ok()
            .and_then(|(stream, _)| tls_listener::accept(stream))
        {
            let _ = tls.write_all(b"HTTP/1.1 200 OK\r\n\r\n{\"cluster\":\"one\"}");
            let _ = tls.flush();
        }
    });

    // Act
    let body = HttpClient::new()
        .get(&loopback(port).over(Transport::Tls), "/")
        .expect("an answer over TLS");

    // Assert
    assert_eq!(body, "{\"cluster\":\"one\"}");
}

/// Serves one connection per answer in `responses`, in turn, and hands back each request.
fn serve_each(responses: &[&[u8]]) -> (HttpEndpoint, mpsc::Receiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    let port = listener.local_addr().expect("a bound port").port();
    let (sender, receiver) = mpsc::channel();
    let responses: Vec<Vec<u8>> = responses.iter().map(|response| response.to_vec()).collect();

    thread::spawn(move || {
        for response in responses {
            let Ok((mut stream, _)) = listener.accept() else {
                return;
            };
            let mut request = Vec::new();
            let mut byte = [0_u8; 1];
            while !request.ends_with(b"\r\n\r\n") && stream.read(&mut byte).unwrap_or(0) == 1 {
                request.push(byte[0]);
            }
            let _ = sender.send(String::from_utf8_lossy(&request).into_owned());
            let _ = stream.write_all(&response);
        }
    });

    (loopback(port), receiver)
}

const OK: &[u8] = b"HTTP/1.1 200 OK\r\ncontent-length: 2\r\n\r\n{}";
const UNAUTHORISED: &[u8] = b"HTTP/1.1 401 Unauthorized\r\ncontent-length: 2\r\n\r\n{}";
const API_KEY_HEADER: &str = "\r\nAuthorization: ApiKey a2V5OnNlY3JldA==\r\n";

fn api_key() -> rastro::collectors::elasticsearch::ApiCredential {
    rastro::collectors::elasticsearch::ApiCredential::api_key("a2V5OnNlY3JldA==")
}

#[test]
fn get_over_plain_http_sends_no_credential_to_a_node_that_does_not_ask_for_one() {
    // Arrange: found by the security review. Plain HTTP carries the credential in the clear to
    // whatever holds the listener, so it goes only where the node asks for it.
    let (endpoint, requests) = serve_each(&[OK]);

    // Act
    HttpClient::new()
        .authenticating(Some(api_key()))
        .get(&endpoint, "/")
        .expect("an answer");

    // Assert
    let request = requests.recv().expect("the request");
    assert!(!request.contains("Authorization"), "{request}");
}

#[test]
fn get_over_plain_http_sends_the_credential_once_the_node_asks_for_one() {
    // Arrange: a secured node answers 401, then the same request with the credential, then the
    // next request, which needs no second asking.
    let (endpoint, requests) = serve_each(&[UNAUTHORISED, OK, OK]);
    let client = HttpClient::new().authenticating(Some(api_key()));

    // Act
    client.get(&endpoint, "/").expect("an answer");
    client
        .get(&endpoint, "/_cluster/settings")
        .expect("an answer");

    // Assert
    let sent: Vec<String> = requests.iter().take(3).collect();
    assert!(!sent[0].contains("Authorization"), "{}", sent[0]);
    assert!(sent[1].contains(API_KEY_HEADER), "{}", sent[1]);
    assert!(sent[2].contains(API_KEY_HEADER), "{}", sent[2]);
}

#[test]
fn get_reports_a_403_without_a_credential_as_refused_to_no_credential() {
    // Arrange: found by review. A node with anonymous access whose anonymous role lacks a
    // privilege answers 403 to a request that carried nothing, and the refusal said a credential
    // had been given.
    let (endpoint, _) = serve_each(&[b"HTTP/1.1 403 Forbidden\r\ncontent-length: 2\r\n\r\n{}"]);

    // Act
    let unread = HttpClient::new().get(&endpoint, "/").expect_err("a 403");

    // Assert
    assert!(unread.is_not_read());
    assert!(
        !unread.reason().contains("credential given"),
        "{}",
        unread.reason()
    );
}

#[test]
fn get_reports_a_rejected_credential_as_not_read() {
    // Arrange
    let (endpoint, _) = serve_each(&[UNAUTHORISED, UNAUTHORISED]);

    // Act
    let unread = HttpClient::new()
        .authenticating(Some(api_key()))
        .get(&endpoint, "/")
        .expect_err("a 401");

    // Assert
    assert!(unread.is_not_read());
    assert_eq!(unread.reason(), "the credential given was rejected");
}

#[test]
fn get_over_tls_gives_up_on_a_listener_that_never_completes_the_handshake() {
    // Arrange: found by review. The handshake reads before the request is written, so a read
    // deadline set only for the answer left a stalled listener holding the run forever.
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    let port = listener.local_addr().expect("a bound port").port();
    thread::spawn(move || {
        let held = listener.accept();
        thread::sleep(Duration::from_secs(30));
        drop(held);
    });
    let (sender, receiver) = mpsc::channel();

    // Act
    thread::spawn(move || {
        let outcome = HttpClient::bounded(Duration::from_millis(500), 1024)
            .get(&loopback(port).over(Transport::Tls), "/");
        let _ = sender.send(outcome);
    });

    // Assert
    let outcome = receiver
        .recv_timeout(Duration::from_secs(5))
        .expect("the client to give up within its deadline");
    let unread = outcome.expect_err("no handshake");
    assert!(unread.reason().contains("timed out"), "{}", unread.reason());
}

#[test]
fn get_over_tls_gives_up_on_a_listener_that_trickles_its_handshake_past_the_deadline() {
    // Arrange: found by the security review. Each read of the handshake had a bound of its own,
    // and a listener that sends a byte of a large record faster than that bound never ran one
    // out. The record header says sixteen kilobytes are coming; they come a byte at a time.
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    let port = listener.local_addr().expect("a bound port").port();
    thread::spawn(move || {
        let Ok((mut stream, _)) = listener.accept() else {
            return;
        };
        let mut hello = [0_u8; 512];
        let _ = stream.read(&mut hello);
        if stream
            .write_all(&[0x16, 0x03, 0x03, 0x40, 0x00, 0x02])
            .is_err()
        {
            return;
        }
        for _ in 0..200 {
            thread::sleep(Duration::from_millis(100));
            if stream.write_all(&[0]).is_err() {
                return;
            }
        }
    });
    let (sender, receiver) = mpsc::channel();

    // Act
    thread::spawn(move || {
        let outcome = HttpClient::bounded(Duration::from_millis(500), 1024)
            .get(&loopback(port).over(Transport::Tls), "/");
        let _ = sender.send(outcome);
    });

    // Assert
    let outcome = receiver
        .recv_timeout(Duration::from_secs(5))
        .expect("the client to give up within its deadline");
    let unread = outcome.expect_err("no handshake");
    assert!(unread.reason().contains("timed out"), "{}", unread.reason());
}

#[test]
fn get_over_tls_refuses_a_peer_that_does_not_hold_the_key_its_certificate_names() {
    // Arrange: found by review. The chain is not checked, by design, and nothing showed that the
    // handshake's signature still is: a verifier that accepted every signature passed the suite.
    let port = tls_listener::presenting_a_certificate_it_holds_no_key_for();

    // Act
    let unread = HttpClient::new()
        .get(&loopback(port).over(Transport::Tls), "/")
        .expect_err("a forged handshake");

    // Assert
    assert!(
        unread.reason().contains("could not be sent"),
        "{}",
        unread.reason()
    );
}

#[test]
fn get_over_tls_says_what_closed_without_blaming_the_settings() {
    // Arrange: found by review. A TLS listener that closes after its handshake was reported as
    // one the settings said served plain HTTP, which on this path they did not.
    let port = tls_listener::serving(|_| Vec::new());

    // Act
    let unread = HttpClient::new()
        .get(&loopback(port).over(Transport::Tls), "/")
        .expect_err("no answer");

    // Assert
    assert!(
        unread.reason().contains("without an HTTP answer"),
        "{}",
        unread.reason()
    );
    assert!(
        !unread.reason().contains("plain HTTP"),
        "{}",
        unread.reason()
    );
}

#[test]
fn get_dials_the_fallback_where_the_first_address_cannot_be_reached() {
    // Arrange: nothing listens on `::1` at this port, the IPv4 loopback does.
    let (served, _) = serve_once(b"HTTP/1.1 200 OK\r\ncontent-length: 2\r\n\r\n{}".to_vec());
    let endpoint = HttpEndpoint::new(InetHost::new("::1").expect("a host"), *served.port())
        .falling_back_to(InetHost::new("127.0.0.1").expect("a host"));

    // Act
    let body = HttpClient::new().get(&endpoint, "/").expect("an answer");

    // Assert
    assert_eq!(body, "{}");
}

/// An answer of `size` bytes of JSON, `{"a":"…"}`.
fn json_of(size: usize) -> Vec<u8> {
    let padding = "a".repeat(size - 8);
    let body = format!("{{\"a\":\"{padding}\"}}");
    let mut answer =
        format!("HTTP/1.1 200 OK\r\ncontent-length: {}\r\n\r\n", body.len()).into_bytes();
    answer.extend_from_slice(body.as_bytes());
    answer
}

#[test]
fn get_refuses_an_answer_above_its_bound() {
    // Arrange: 17 MB, above the 16 MB every read but the index ones is bounded by.
    let (endpoint, _) = serve_once(json_of(17 * 1024 * 1024));

    // Act
    let unread = HttpClient::new()
        .get(&endpoint, "/")
        .expect_err("too large");

    // Assert
    assert!(
        unread.reason().contains("larger than"),
        "{}",
        unread.reason()
    );
}

#[test]
fn get_for_large_answers_reads_what_a_cluster_with_many_indices_returns() {
    // Arrange: found by review. `_settings` and `_mapping` grow with every index, and a cluster
    // with thousands of them, or Fleet-sized mappings, answered past the common bound.
    let (endpoint, _) = serve_once(json_of(17 * 1024 * 1024));

    // Act
    let body = HttpClient::new()
        .for_large_answers()
        .get(&endpoint, "/")
        .expect("an answer within the wider bound");

    // Assert
    assert_eq!(body.len(), 17 * 1024 * 1024);
}

#[test]
fn get_reports_a_node_that_demands_a_client_certificate_as_not_read() {
    // Arrange: found by the third domain review, measured on 8.19.22 (cell 30) with
    // `client_authentication: required`. Mutual TLS is the node's configuration: rastro has no
    // client certificate to present, which is the box keeping it out and not a failure to read.
    let port = tls_listener::requiring_a_client_certificate();

    // Act
    let unread = HttpClient::new()
        .get(&loopback(port).over(Transport::Tls), "/")
        .expect_err("no client certificate");

    // Assert
    assert!(unread.is_not_read(), "{}", unread.reason());
    assert!(
        unread.reason().contains("client certificate"),
        "{}",
        unread.reason()
    );
}
