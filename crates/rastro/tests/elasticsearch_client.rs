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
