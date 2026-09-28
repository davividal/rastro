//! The one request rastro sends over the network, against a listener the test holds.
//!
//! A real socket rather than a mock, because what is being pinned is bytes on the wire: the
//! request the node sees, and how the answer is read back.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use rastro::collectors::elasticsearch::{HttpClient, HttpEndpoint};
use rastro::collectors::inet::{InetHost, PortNumber};

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
fn get_reports_a_node_that_wants_credentials() {
    // Arrange: what 8.x answers without them, measured on 8.15.3 with security on and TLS off.
    let (endpoint, _) = serve_once(
        b"HTTP/1.1 401 Unauthorized\r\nWWW-Authenticate: Basic realm=\"security\"\r\ncontent-length: 2\r\n\r\n{}"
            .to_vec(),
    );

    // Act
    let unread = HttpClient::new().get(&endpoint, "/").expect_err("a 401");

    // Assert
    assert!(
        unread.reason().contains("credentials"),
        "{}",
        unread.reason()
    );
}

#[test]
fn get_reports_a_node_that_forbids_the_read() {
    // Arrange
    let (endpoint, _) =
        serve_once(b"HTTP/1.1 403 Forbidden\r\ncontent-length: 2\r\n\r\n{}".to_vec());

    // Act
    let unread = HttpClient::new().get(&endpoint, "/").expect_err("a 403");

    // Assert
    assert!(
        unread.reason().contains("credentials"),
        "{}",
        unread.reason()
    );
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
