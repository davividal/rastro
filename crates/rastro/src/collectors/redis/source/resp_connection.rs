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
use std::net::TcpStream;
use std::os::unix::net::UnixStream;
use std::time::{Duration, Instant};

use rastro_collector::CollectionError;
use redis::Parser;

use super::reply::{Reply, shortened};
use super::server_discovery::DialTarget;

/// How long a server gets to accept a request or finish a reply.
///
/// Generous against a local server, which answers `CONFIG GET *` in well under a millisecond,
/// and short against the whole run, which has other facets to read.
const ANSWER_WITHIN: Duration = Duration::from_secs(5);

/// The largest reply read before refusing it.
///
/// `CONFIG GET *` from a redis 8 build carrying every module is a few tens of kilobytes, and
/// `ACL LIST` grows with the accounts. Megabytes are headroom; the bound is against a server, or
/// something impersonating one on its port, that never stops.
const REPLY_BOUND: usize = 4 * 1024 * 1024;

/// How long a TCP connection gets to be accepted.
///
/// Only ever to an address the server was seen listening on, so a slow accept is a wedged server
/// rather than a distant one.
const CONNECT_WITHIN: Duration = Duration::from_secs(1);

/// The least a socket timeout is set to: zero means "never" to the kernel, and a deadline about to
/// pass is the read's own business to notice.
const SHORTEST_WAIT: Duration = Duration::from_millis(1);

/// A socket to a server, over either of the two ways a server listens locally.
#[derive(Debug)]
pub enum ServerStream {
    Unix(UnixStream),
    Tcp(TcpStream),
}

impl ServerStream {
    fn set_timeouts(&self, within: Duration) -> io::Result<()> {
        self.set_read_timeout(within)?;
        self.set_write_timeout(within)
    }

    fn set_read_timeout(&self, within: Duration) -> io::Result<()> {
        match self {
            ServerStream::Unix(stream) => stream.set_read_timeout(Some(within)),
            ServerStream::Tcp(stream) => stream.set_read_timeout(Some(within)),
        }
    }

    fn set_write_timeout(&self, within: Duration) -> io::Result<()> {
        match self {
            ServerStream::Unix(stream) => stream.set_write_timeout(Some(within)),
            ServerStream::Tcp(stream) => stream.set_write_timeout(Some(within)),
        }
    }
}

impl From<UnixStream> for ServerStream {
    fn from(stream: UnixStream) -> Self {
        ServerStream::Unix(stream)
    }
}

impl From<TcpStream> for ServerStream {
    fn from(stream: TcpStream) -> Self {
        ServerStream::Tcp(stream)
    }
}

impl Read for ServerStream {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        match self {
            ServerStream::Unix(stream) => stream.read(buffer),
            ServerStream::Tcp(stream) => stream.read(buffer),
        }
    }
}

impl Write for ServerStream {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        match self {
            ServerStream::Unix(stream) => stream.write(buffer),
            ServerStream::Tcp(stream) => stream.write(buffer),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            ServerStream::Unix(stream) => stream.flush(),
            ServerStream::Tcp(stream) => stream.flush(),
        }
    }
}

/// Whether the kernel names `server` as the process at the other end of a connected unix socket.
#[cfg(target_os = "linux")]
fn held_by(socket: &UnixStream, server: u32) -> bool {
    rustix::net::sockopt::socket_peercred(socket)
        .is_ok_and(|peer| u32::try_from(peer.pid.as_raw_pid()).is_ok_and(|pid| pid == server))
}

/// rastro reads Linux hosts; elsewhere, where only its tests run, there is no `SO_PEERCRED`.
#[cfg(not(target_os = "linux"))]
fn held_by(_socket: &UnixStream, _server: u32) -> bool {
    true
}

/// A connection to one server, asked one command at a time.
///
/// One parser for the life of the connection, so bytes past one reply are kept for the next
/// rather than lost or read as part of it.
pub struct RespConnection {
    stream: ServerStream,
    parser: Parser,
    bound: usize,
    within: Duration,
}

impl RespConnection {
    /// A connection over a socket already open, bounded the way every read here is.
    pub fn over(stream: impl Into<ServerStream>) -> Result<Self, CollectionError> {
        Self {
            stream: stream.into(),
            parser: Parser::new(),
            bound: REPLY_BOUND,
            within: ANSWER_WITHIN,
        }
        .timing_out_after(ANSWER_WITHIN)
    }

    /// A connection to the socket discovery chose.
    ///
    /// **A unix socket is spoken to only once the kernel names the server as its peer.** The socket
    /// table that led here prints a path raw, newline included, measured, so another account can
    /// bind a path that forges a row naming the server's inode. `SO_PEERCRED` on the connected
    /// socket is the kernel's own answer, and anything but the server's pid is sent nothing. A TCP
    /// row has no free text to forge.
    pub fn dial(target: &DialTarget, server: u32) -> Result<Self, CollectionError> {
        let stream: io::Result<ServerStream> = match target {
            DialTarget::Unix(path) => UnixStream::connect(path).map(ServerStream::from),
            DialTarget::Tcp(address) => {
                TcpStream::connect_timeout(address, CONNECT_WITHIN).map(ServerStream::from)
            }
        };

        let stream = stream.map_err(|error| {
            CollectionError::new(format!("could not connect to {target}: {error}"))
        })?;

        if let ServerStream::Unix(socket) = &stream
            && !held_by(socket, server)
        {
            return Err(CollectionError::new(format!(
                "the socket at {target} is not the server's own: the kernel names another process \
                 as its holder, so nothing was sent"
            )));
        }

        Self::over(stream)
    }

    /// The same, giving up on a server sooner or later than the default.
    pub fn timing_out_after(mut self, within: Duration) -> Result<Self, CollectionError> {
        self.stream.set_timeouts(within).map_err(|error| {
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
        let deadline = Instant::now() + self.within;

        self.send(command, arguments, deadline)?;
        self.receive(command, deadline)
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
                .set_write_timeout(remaining.max(SHORTEST_WAIT))?;
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

    fn receive(&mut self, command: &str, deadline: Instant) -> Result<Reply, CollectionError> {
        let mut exchange = Exchange {
            stream: &mut self.stream,
            deadline,
            read: 0,
            bound: self.bound,
            interrupted: None,
        };

        match self.parser.parse_value(&mut exchange) {
            Ok(value) => Reply::try_from(value),
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
}

/// The socket, as one command's reply is read from it.
///
/// **Each read gets what is left of the command's deadline, and at most what is left of the bound
/// plus one byte**, which is how a breach of either is noticed without reading past it.
struct Exchange<'a> {
    stream: &'a mut ServerStream,
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
        self.stream.set_read_timeout(remaining.max(SHORTEST_WAIT))?;

        let room = (self.bound + 1).saturating_sub(self.read).min(buffer.len());
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
            Err(error) => Err(error),
        }
    }
}
