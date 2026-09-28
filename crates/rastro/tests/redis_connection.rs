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
