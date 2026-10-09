//! Talking to a server: the protocol is the crate's, the bounds are rastro's.
//!
//! Every test here stands a fake server on the far end of a socket pair, so what went over the
//! wire and what came back are both asserted rather than reasoned about.

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use rastro::collectors::redis::{Reply, RespConnection};

/// A server that reads exactly the request it expects, then writes each chunk in turn.
///
/// Returns the client's end and a handle yielding the bytes the server received.
fn server(request_length: usize, chunks: Vec<Vec<u8>>) -> (UnixStream, JoinHandle<Vec<u8>>) {
    let (client, mut far_end) = UnixStream::pair().expect("a socket pair");

    let handle = thread::spawn(move || {
        let mut received = vec![0; request_length];
        far_end
            .read_exact(&mut received)
            .expect("the whole request");

        for chunk in chunks {
            far_end.write_all(&chunk).expect("a writable socket");
            far_end.flush().expect("a flushable socket");
            thread::sleep(Duration::from_millis(20));
        }

        received
    });

    (client, handle)
}

/// The request a `PING` puts on the wire, which most tests send and none are about.
const PING: &[u8] = b"*1\r\n$4\r\nPING\r\n";

fn answer_to_ping(reply: &[u8]) -> Result<Reply, String> {
    let (client, handle) = server(PING.len(), vec![reply.to_vec()]);
    let mut connection = RespConnection::over(client).expect("a connection");

    let answer = connection.ask(&["PING"]).map_err(|error| error.to_string());
    handle.join().expect("the server finished");

    answer
}

#[test]
fn a_command_goes_out_as_an_array_of_bulk_strings() {
    // Arrange
    let request = b"*3\r\n$6\r\nCONFIG\r\n$3\r\nGET\r\n$1\r\n*\r\n";
    let (client, handle) = server(request.len(), vec![b"*0\r\n".to_vec()]);
    let mut connection = RespConnection::over(client).expect("a connection");

    // Act
    let answer = connection.ask(&["CONFIG", "GET", "*"]);

    // Assert
    assert_eq!(handle.join().expect("the server finished"), request);
    assert_eq!(answer.expect("an answer"), Reply::Array(Vec::new()));
}

#[test]
fn an_argument_holding_a_space_stays_one_argument() {
    // Arrange: the inline protocol splits on spaces, so a password holding one would reach the
    // server as two arguments. Only the bulk encoding keeps it whole.
    let request = b"*2\r\n$4\r\nAUTH\r\n$9\r\ntwo words\r\n";
    let (client, handle) = server(request.len(), vec![b"+OK\r\n".to_vec()]);
    let mut connection = RespConnection::over(client).expect("a connection");

    // Act
    let answer = connection.ask(&["AUTH", "two words"]);

    // Assert
    assert_eq!(handle.join().expect("the server finished"), request);
    assert_eq!(answer.expect("an answer"), Reply::Simple("OK".to_owned()));
}

#[test]
fn an_empty_value_is_not_nil() {
    // Arrange: `save ""` is how the estate turns persistence off, and `redis-cli --raw` prints
    // it as a blank line that a missing value also prints as. Measured.
    let reply = b"*4\r\n$4\r\nsave\r\n$0\r\n\r\n$6\r\nnobody\r\n$-1\r\n";

    // Act
    let answer = answer_to_ping(reply);

    // Assert
    assert_eq!(
        answer.expect("an answer"),
        Reply::Array(vec![
            Reply::Bulk("save".to_owned()),
            Reply::Bulk(String::new()),
            Reply::Bulk("nobody".to_owned()),
            Reply::Nil,
        ])
    );
}

#[test]
fn a_value_keeps_an_embedded_newline() {
    // Act
    let answer = answer_to_ping(b"$11\r\nline1\nline2\r\n");

    // Assert
    assert_eq!(
        answer.expect("an answer"),
        Reply::Bulk("line1\nline2".to_owned())
    );
}

#[test]
fn a_reply_split_across_writes_is_reassembled() {
    // Arrange: a reply larger than one read arrives in pieces, and a piece is not an answer.
    let (client, handle) = server(
        PING.len(),
        vec![b"*2\r\n$3\r\nfo".to_vec(), b"o\r\n:7\r\n".to_vec()],
    );
    let mut connection = RespConnection::over(client).expect("a connection");

    // Act
    let answer = connection.ask(&["PING"]);
    handle.join().expect("the server finished");

    // Assert
    assert_eq!(
        answer.expect("an answer"),
        Reply::Array(vec![Reply::Bulk("foo".to_owned()), Reply::Integer(7)])
    );
}

#[test]
fn an_error_reply_is_an_answer_rather_than_a_failure() {
    // Act: the caller has to tell `NOAUTH` from `ERR unknown command`, so both come back.
    let answer = answer_to_ping(b"-NOAUTH Authentication required.\r\n");

    // Assert
    assert_eq!(
        answer.expect("an answer"),
        Reply::Error("NOAUTH Authentication required.".to_owned())
    );
}

#[test]
fn a_reply_past_the_bound_fails_and_names_the_bound() {
    // Arrange
    let reply = format!("${}\r\n{}\r\n", 64, "x".repeat(64));
    let (client, handle) = server(PING.len(), vec![reply.into_bytes()]);
    let mut connection = RespConnection::over(client)
        .expect("a connection")
        .bounded_by(16);

    // Act
    let answer = connection.ask(&["PING"]);
    handle.join().expect("the server finished");

    // Assert: refused, never a truncated answer passed off as the whole.
    let error = answer.expect_err("a reply past the bound").to_string();
    assert!(error.contains("16 bytes"), "{error}");
}

#[test]
fn a_connection_closed_mid_reply_is_a_failure() {
    // Act: the server promises ten bytes, sends three and hangs up.
    let answer = answer_to_ping(b"$10\r\nabc");

    // Assert
    let error = answer.expect_err("a half reply");
    assert!(error.contains("closed"), "{error}");
}

#[test]
fn a_value_that_is_not_utf8_is_refused() {
    // Act
    let answer = answer_to_ping(b"$2\r\n\xff\xfe\r\n");

    // Assert: refused rather than repaired, because a replacement character is text that was
    // never on the box.
    let error = answer.expect_err("bytes that are not UTF-8");
    assert!(error.contains("UTF-8"), "{error}");
}

#[test]
fn a_reply_that_is_not_the_protocol_is_refused() {
    // Act
    let answer = answer_to_ping(b"?what\r\n");

    // Assert
    assert!(answer.is_err(), "{answer:?}");
}

#[test]
fn a_server_that_never_answers_is_given_up_on() {
    // Arrange: the far end reads the request and then holds the socket open, saying nothing.
    let (client, mut far_end) = UnixStream::pair().expect("a socket pair");
    let holder = thread::spawn(move || {
        let mut received = vec![0; PING.len()];
        far_end
            .read_exact(&mut received)
            .expect("the whole request");
        thread::sleep(Duration::from_millis(600));
    });
    let mut connection = RespConnection::over(client)
        .expect("a connection")
        .timing_out_after(Duration::from_millis(100))
        .expect("a settable timeout");

    // Act
    let answer = connection.ask(&["PING"]);
    holder.join().expect("the holder finished");

    // Assert
    let error = answer.expect_err("a silent server").to_string();
    assert!(error.contains("did not answer"), "{error}");
}

#[test]
fn a_reply_nested_past_any_real_one_is_refused_rather_than_crashing_the_run() {
    // Arrange: measured, 10 000 nested arrays, 40 KB, overflowed the stack of a decoder with no
    // depth limit and aborted the whole run. Real replies nest three deep at most.
    let reply = format!("{}:1\r\n", "*1\r\n".repeat(10_000)).into_bytes();
    let (client, handle) = server(PING.len(), vec![reply]);

    // Act: on a thread with the stack a collector gets, where an overflow is an abort.
    let answer = thread::Builder::new()
        .stack_size(2 * 1024 * 1024)
        .spawn(move || {
            let mut connection = RespConnection::over(client).expect("a connection");
            connection.ask(&["PING"]).map_err(|error| error.to_string())
        })
        .expect("a thread")
        .join()
        .expect("no overflow");
    // The server may find rastro gone mid-write, which is the refusal working.
    let _ = handle.join();

    // Assert
    assert!(answer.is_err(), "{answer:?}");
}

#[test]
fn a_peer_that_trickles_is_given_up_on_at_the_deadline() {
    // Arrange: one byte at a time, each well inside a per-read timeout; only a deadline over the
    // whole reply ends it.
    let reply = b"$20\r\nabcdefghijklmnopqrst\r\n"
        .iter()
        .map(|byte| vec![*byte])
        .collect();
    let (client, handle) = server(PING.len(), reply);
    let mut connection = RespConnection::over(client)
        .expect("a connection")
        .timing_out_after(Duration::from_millis(150))
        .expect("a settable timeout");

    // Act
    let started = std::time::Instant::now();
    let answer = connection.ask(&["PING"]);
    let waited = started.elapsed();
    drop(connection);
    let _ = handle.join();

    // Assert: refused, and near the deadline rather than after every byte arrived.
    let error = answer.expect_err("a trickling peer").to_string();
    assert!(error.contains("did not answer"), "{error}");
    assert!(waited < Duration::from_millis(400), "{waited:?}");
}

#[test]
fn a_peer_that_drains_a_command_slowly_is_given_up_on_at_the_deadline() {
    // Arrange: a command larger than the socket's buffer, to a peer that takes a little of it
    // before each per-write timeout runs out, so every write makes progress and none times out.
    let (client, mut far_end) = UnixStream::pair().expect("a socket pair");
    thread::spawn(move || {
        let mut buffer = [0_u8; 4096];
        while far_end.read(&mut buffer).is_ok_and(|read| read > 0) {
            thread::sleep(Duration::from_millis(10));
        }
    });
    let mut connection = RespConnection::over(client)
        .expect("a connection")
        .timing_out_after(Duration::from_millis(300))
        .expect("a timeout");
    let password = "x".repeat(2 * 1024 * 1024);

    // Act
    let started = std::time::Instant::now();
    let result = connection.ask(&["AUTH", &password]);

    // Assert: held to the command's deadline, not to one per write.
    let error = result.expect_err("a peer past the deadline").to_string();
    assert!(error.contains("within"), "{error}");
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "{:?}",
        started.elapsed()
    );
}

/// Linux alone: there a blocking connect waits for room in a full queue, where macOS refuses it.
#[cfg(target_os = "linux")]
#[test]
fn a_socket_whose_queue_is_full_is_given_up_on_at_the_deadline() {
    // Arrange: a listener nobody accepts on, its queue filled by connections that stay pending,
    // which another account with access to the socket can hold open; a blocking connect then waits.
    let path = std::env::temp_dir().join(format!("rastro-full-{}.sock", std::process::id()));
    let _ = std::fs::remove_file(&path);
    // A queue of one, so a handful of pending connections fills it on any kernel's defaults.
    let listener = rustix::net::socket(
        rustix::net::AddressFamily::UNIX,
        rustix::net::SocketType::STREAM,
        None,
    )
    .expect("a socket");
    rustix::net::bind(
        &listener,
        &rustix::net::SocketAddrUnix::new(&path).expect("a socket path"),
    )
    .expect("a bindable socket path");
    rustix::net::listen(&listener, 1).expect("a listening socket");
    for _ in 0..8 {
        let path = path.clone();
        thread::spawn(move || {
            let held = UnixStream::connect(path);
            thread::sleep(Duration::from_secs(30));
            drop(held);
        });
    }
    thread::sleep(Duration::from_millis(300));

    // Act
    let started = std::time::Instant::now();
    let result = RespConnection::dial(
        &rastro::collectors::redis::DialTarget::Unix(path.clone()),
        std::process::id(),
    );

    // Assert: refused near the connect deadline rather than waiting on the queue.
    assert!(result.is_err());
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "{:?}",
        started.elapsed()
    );
    drop(listener);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn two_replies_arriving_together_are_answered_in_order() {
    // Arrange: a reply's surplus is the next reply, never lost and never misread.
    let (client, handle) = server(PING.len(), vec![b"+first\r\n+second\r\n".to_vec()]);
    let mut connection = RespConnection::over(client).expect("a connection");

    // Act
    let first = connection.ask(&["PING"]);
    let second = connection.ask(&["PING"]);
    handle.join().expect("the server finished");

    // Assert
    assert_eq!(first.expect("an answer"), Reply::Simple("first".to_owned()));
    assert_eq!(
        second.expect("an answer"),
        Reply::Simple("second".to_owned())
    );
}

#[test]
fn every_reply_shape_has_a_name_for_a_refusal() {
    // Act & Assert: a refusal names the shape, never the content.
    let kinds: Vec<&str> = [
        Reply::Simple(String::new()),
        Reply::Error(String::new()),
        Reply::Integer(0),
        Reply::Bulk(String::new()),
        Reply::Nil,
        Reply::Array(Vec::new()),
    ]
    .iter()
    .map(Reply::kind)
    .collect();
    assert_eq!(
        kinds,
        [
            "a status",
            "an error",
            "an integer",
            "text",
            "nothing",
            "a list"
        ]
    );
}

#[test]
fn an_error_with_no_detail_is_its_code() {
    // Act & Assert
    assert_eq!(
        answer_to_ping(b"-NOAUTH\r\n").expect("an answer"),
        Reply::Error("NOAUTH".to_owned())
    );
}

#[test]
fn a_reply_in_a_newer_protocol_is_refused() {
    // Act: a RESP3 boolean, which a server sends only after a `HELLO 3` rastro never sends.
    let answer = answer_to_ping(b"#t\r\n");

    // Assert
    let error = answer.expect_err("a newer protocol");
    assert!(error.contains("newer than the RESP2"), "{error}");
}
