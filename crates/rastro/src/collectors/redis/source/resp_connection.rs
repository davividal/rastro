//! The socket to one server, and the only place anything is said to it.
//!
//! **The parser and the encoder are the `redis` crate's; everything around them is rastro's**,
//! and the split is the point. Its client would own the connect handshake and send a
//! `CLIENT SETINFO` of its own; this facet has to own every byte it sends, because a failed `AUTH`
//! writes an entry into the server's `ACL LOG`, so what is sent, and how often, is a question about
//! changing the host. Its parser caps nesting, where the framing crate this replaced recursed
//! without a limit and was measured to abort the run on a 40 KB reply.
//!
//! What is guaranteed here, carried over from the canonical tool seam because a server is
//! another program rastro asks a question of:
//!
//! - **A deadline** over each command, its write and its whole reply: a timeout per read alone
//!   restarts on every byte, so a peer that trickles would hold the run for as long as it liked.
//! - **A byte bound** on every reply, breached by refusal and never by truncation: a quietly
//!   truncated answer is the bug that disqualified configsnap.
//! - **Strictly valid UTF-8**, see [`Reply`].
//! - **No argument in any message.** An error names the command and never what followed it,
//!   because what follows `AUTH` is a password.

use std::io::{self, ErrorKind, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::{Duration, Instant};

use rastro_collector::CollectionError;
use redis::Parser;

use super::reply::{Reply, shortened};

/// How long a server gets to accept a request or finish a reply.
///
/// Generous against a local server, which answers `CONFIG GET *` in well under a millisecond,
/// and short against the whole run, which has other facets to read.
const ANSWER_WITHIN: Duration = Duration::from_secs(5);

/// The largest reply read before refusing it.
///
/// The largest captured reply, `CONFIG GET *` from a redis 8 build carrying every module, is 11
/// KiB, and `ACL LIST` grows with the accounts. A mebibyte is headroom; the bound is against a
/// server that never stops, and it is kept that low because the parser holds a few dozen bytes per
/// three it reads before rastro refuses a reply in a protocol it did not ask for, found by review.
const REPLY_BOUND: usize = 1024 * 1024;

/// How long a TCP connection gets to be accepted.
///
/// Only ever to an address the server was seen listening on, so a slow accept is a wedged server
/// rather than a distant one.
const CONNECT_WITHIN: Duration = Duration::from_secs(1);

/// The least a socket timeout is set to: zero means "never" to the kernel, and a deadline about to
/// pass is the read's own business to notice.
const SHORTEST_WAIT: Duration = Duration::from_millis(1);

/// A connection to one server, asked one command at a time.
///
/// One parser for the life of the connection, so bytes past one reply are kept for the next
/// rather than lost or read as part of it.
///
/// **Broken by the first exchange that fails**, found by review: a reply abandoned part way leaves
/// the rest of it on the socket, where the next command would read it as its own reply.
pub struct RespConnection {
    stream: TcpStream,
    parser: Parser,
    bound: usize,
    within: Duration,
    broken: bool,
}

impl RespConnection {
    /// A connection over a socket already open, bounded the way every read here is.
    pub fn over(stream: TcpStream) -> Result<Self, CollectionError> {
        Self {
            stream,
            parser: Parser::new(),
            bound: REPLY_BOUND,
            within: ANSWER_WITHIN,
            broken: false,
        }
        .timing_out_after(ANSWER_WITHIN)
    }

    /// A connection to the socket discovery chose.
    pub fn dial(address: &SocketAddr) -> Result<Self, CollectionError> {
        let stream = TcpStream::connect_timeout(address, CONNECT_WITHIN).map_err(|error| {
            CollectionError::new(format!("could not connect to {address}: {error}"))
        })?;

        Self::over(stream)
    }

    /// The same, giving up on a server sooner or later than the default.
    pub fn timing_out_after(mut self, within: Duration) -> Result<Self, CollectionError> {
        self.stream
            .set_read_timeout(Some(within))
            .and_then(|()| self.stream.set_write_timeout(Some(within)))
            .map_err(|error| {
                CollectionError::new(format!("the server's socket refused a timeout: {error}"))
            })?;
        self.within = within;

        Ok(self)
    }

    /// The same, refusing a reply past a bound other than the default.
    pub fn bounded_by(mut self, bytes: usize) -> Self {
        self.bound = bytes;
        self
    }

    /// Sends one command and reads its whole reply, within one deadline.
    ///
    /// Always the multi-bulk encoding, never the inline one: the inline protocol splits on
    /// spaces, and a password holding one would arrive as two arguments.
    pub fn ask(&mut self, arguments: &[&str]) -> Result<Reply, CollectionError> {
        let command = arguments.first().copied().unwrap_or_default();
        if self.broken {
            return Err(CollectionError::new(format!(
                "{command} was not sent: an earlier command failed part way, so what is left on \
                 the connection is no reply to it"
            )));
        }
        let deadline = Instant::now() + self.within;

        // An error reply, or one refused once whole, leaves the stream at the next reply.
        let exchanged = self
            .send(command, arguments, deadline)
            .and_then(|()| self.receive(command, deadline));
        self.broken = exchanged.is_err();
        exchanged?
    }

    fn send(
        &mut self,
        command: &str,
        arguments: &[&str],
        deadline: Instant,
    ) -> Result<(), CollectionError> {
        let mut packed = redis::cmd(command);
        for argument in arguments.iter().skip(1) {
            packed.arg(*argument);
        }

        self.write_before(&packed.get_packed_command(), deadline)
            .and_then(|()| self.stream.flush())
            .map_err(|error| self.failure(command, &error))
    }

    /// Writes all of `bytes`, each write given what is left of the deadline, found by review: a
    /// timeout per write restarts with every partial one, so a peer draining a little before each
    /// ran out could hold the run long past the deadline. Each write is capped too, since some
    /// kernels restart the timeout whenever the peer frees buffer space within one call.
    fn write_before(&mut self, bytes: &[u8], deadline: Instant) -> io::Result<()> {
        let mut written = 0;
        while written < bytes.len() {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(io::Error::from(ErrorKind::TimedOut));
            }
            self.stream
                .set_write_timeout(Some(remaining.max(SHORTEST_WAIT)))?;
            let end = bytes.len().min(written + MOST_WRITTEN_AT_ONCE);
            match self.stream.write(&bytes[written..end]) {
                Ok(0) => return Err(io::Error::from(ErrorKind::WriteZero)),
                Ok(sent) => written += sent,
                Err(error) if error.kind() == ErrorKind::Interrupted => {}
                Err(error) => return Err(error),
            }
        }
        Ok(())
    }

    /// The reply, or why the stream broke; inside, why a whole reply was refused.
    fn receive(
        &mut self,
        command: &str,
        deadline: Instant,
    ) -> Result<Result<Reply, CollectionError>, CollectionError> {
        let mut exchange = Exchange {
            stream: &mut self.stream,
            deadline,
            read: 0,
            bound: self.bound,
            interrupted: None,
        };

        match self.parser.parse_value(&mut exchange) {
            Ok(value) => Ok(Reply::try_from(value)),
            Err(error) => Err(match exchange.interrupted {
                Some(Interrupted::PastBound) => CollectionError::new(format!(
                    "the server's reply to {command} ran past {bound} bytes, so it was refused \
                     rather than truncated",
                    bound = self.bound
                )),
                Some(Interrupted::PastDeadline) => CollectionError::new(format!(
                    "the server did not answer {command} within {within:?}",
                    within = self.within
                )),
                Some(Interrupted::Closed) => CollectionError::new(format!(
                    "the server closed the connection before its reply to {command} was complete"
                )),
                Some(Interrupted::Failed(failure)) => CollectionError::new(format!(
                    "the connection failed while the reply to {command} was read: {failure}"
                )),
                None => CollectionError::new(format!(
                    "the server's reply to {command} is not the redis protocol: {}",
                    shortened(&error.to_string())
                )),
            }),
        }
    }

    fn failure(&self, command: &str, error: &io::Error) -> CollectionError {
        match error.kind() {
            ErrorKind::WouldBlock | ErrorKind::TimedOut => CollectionError::new(format!(
                "the server did not answer {command} within {within:?}",
                within = self.within
            )),
            _ => CollectionError::new(format!("{command} could not be exchanged: {error}")),
        }
    }
}

/// The most one write is handed, so the deadline is checked between writes of a long command.
const MOST_WRITTEN_AT_ONCE: usize = 64 * 1024;

/// Why a read stopped short of a reply, so the refusal names the cause rather than the parser's
/// account of a stream that ended.
enum Interrupted {
    PastBound,
    PastDeadline,
    Closed,
    Failed(ErrorKind),
}

/// The socket, as one command's reply is read from it.
///
/// **Each read gets what is left of the command's deadline, and at most what is left of the bound
/// plus one byte**, which is how a breach of either is noticed without reading past it.
struct Exchange<'a> {
    stream: &'a mut TcpStream,
    deadline: Instant,
    read: usize,
    bound: usize,
    interrupted: Option<Interrupted>,
}

impl Read for Exchange<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let remaining = self.deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            self.interrupted = Some(Interrupted::PastDeadline);
            return Err(io::Error::from(ErrorKind::TimedOut));
        }
        self.stream
            .set_read_timeout(Some(remaining.max(SHORTEST_WAIT)))?;

        let room = self
            .bound
            .saturating_add(1)
            .saturating_sub(self.read)
            .min(buffer.len());
        match self.stream.read(&mut buffer[..room]) {
            Ok(0) => {
                self.interrupted = Some(Interrupted::Closed);
                Ok(0)
            }
            Ok(read) => {
                self.read += read;
                if self.read > self.bound {
                    self.interrupted = Some(Interrupted::PastBound);
                    return Err(io::Error::other("past the bound"));
                }
                Ok(read)
            }
            Err(error) if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                self.interrupted = Some(Interrupted::PastDeadline);
                Err(error)
            }
            Err(error) if error.kind() == ErrorKind::Interrupted => Err(error),
            Err(error) => {
                self.interrupted = Some(Interrupted::Failed(error.kind()));
                Err(error)
            }
        }
    }
}
