//! Digesting a file's content, and what a read that fails part way costs.
//!
//! Against a reader rather than a real file, which is the one thing a scratch tree cannot
//! arrange: `read(2)` returning `EINTR` needs a caught signal landing inside the call, and
//! no test can time that. The reader here is the seam that makes the branch reachable.

use std::io::{self, Read};

use rastro::collectors::filesystem::sha256_of_stream;

/// `sha256sum` of the six bytes below, so the expectation comes from outside this crate.
const HELLO_DIGEST: &str = "5891b5b522d5df086d0ff0b110fbd9d21bb4fc7163af34d08286a2e846f6be03";
const HELLO: &[u8] = b"hello\n";

/// A reader that fails with `kind` on the nth read, and otherwise serves `content`.
///
/// Arrange: the kernel is entitled to return a short read, `EINTR` among them, at any point
/// in a file rastro is hashing. What separates the two cases is whether the digest still
/// comes out right afterwards.
struct FailsOnce {
    content: Vec<u8>,
    offset: usize,
    failure: Option<io::ErrorKind>,
}

impl FailsOnce {
    fn with(kind: io::ErrorKind) -> Self {
        Self {
            content: HELLO.to_vec(),
            offset: 0,
            failure: Some(kind),
        }
    }
}

impl Read for FailsOnce {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if let Some(kind) = self.failure.take() {
            return Err(io::Error::new(kind, "arranged by the test"));
        }

        let remaining = &self.content[self.offset..];
        let taken = remaining.len().min(buffer.len());
        buffer[..taken].copy_from_slice(&remaining[..taken]);
        self.offset += taken;

        Ok(taken)
    }
}

#[test]
fn a_stream_digests_to_the_value_sha256sum_reports() {
    // Act
    let digest = sha256_of_stream(HELLO).expect("a reader that never fails");

    // Assert
    assert_eq!(digest.as_str(), HELLO_DIGEST);
}

#[test]
fn an_interrupted_read_is_retried_rather_than_reported() {
    // Arrange: `io::copy` retried these before the loop replaced it. Losing that would cost
    // the byte-identical invariant, not just one entry: a caught signal arriving mid-read
    // would record the file unreadable in one run and digest it in the next.
    let interrupted = FailsOnce::with(io::ErrorKind::Interrupted);

    // Act
    let digest = sha256_of_stream(interrupted).expect("an interrupted read is not a failure");

    // Assert
    assert_eq!(digest.as_str(), HELLO_DIGEST);
}

#[test]
fn a_read_that_genuinely_fails_is_reported() {
    // Arrange: the other half of the branch. Retrying everything would hang a walk on a
    // reader that always fails, and hide a real unreadable file behind an infinite loop.
    let denied = FailsOnce::with(io::ErrorKind::PermissionDenied);

    // Act
    let failure = sha256_of_stream(denied).expect_err("a denied read is a failure");

    // Assert
    assert_eq!(failure.kind(), io::ErrorKind::PermissionDenied);
}
