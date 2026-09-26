//! The socket to one server, and the only place anything is said to it.
//!
//! **The framing is `redis-protocol`'s; everything around it is rastro's**, and the split is
//! the point. A full client library would own the connect handshake, and the redis client sends
//! a `CLIENT SETINFO` of its own on connect unless told not to. This facet has to own every byte
//! it sends: a failed `AUTH` writes an entry into the server's `ACL LOG`, so what is sent, and
//! how often, is a question about changing the host.
//!
//! What is guaranteed here, carried over from the canonical tool seam because a server is
//! another program rastro asks a question of:
//!
//! - **A time bound** on every read and write. A wedged server cannot hang the run.
//! - **A byte bound** on every reply, breached by refusal and never by truncation: a quietly
//!   truncated answer is the bug that disqualified configsnap.
//! - **Strictly valid UTF-8**, see [`Reply`].
//! - **No argument in any message.** An error names the command and never what followed it,
//!   because what follows `AUTH` is a password.

use std::io::{self, ErrorKind, Read, Write};
use std::net::TcpStream;
use std::os::unix::net::UnixStream;
use std::time::Duration;

use rastro_collector::CollectionError;
use redis_protocol::resp2::decode::decode;
use redis_protocol::resp2::encode::encode;
use redis_protocol::resp2::types::{OwnedFrame, Resp2Frame};

use super::reply::Reply;
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

/// How much one read asks the kernel for.
const READ_CHUNK: usize = 16 * 1024;

/// A socket to a server, over either of the two ways a server listens locally.
#[derive(Debug)]
pub enum ServerStream {
    Unix(UnixStream),
    Tcp(TcpStream),
}

impl ServerStream {
    fn set_timeouts(&self, within: Duration) -> io::Result<()> {
        match self {
            ServerStream::Unix(stream) => {
                stream.set_read_timeout(Some(within))?;
                stream.set_write_timeout(Some(within))
            }
            ServerStream::Tcp(stream) => {
                stream.set_read_timeout(Some(within))?;
                stream.set_write_timeout(Some(within))
            }
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

/// A connection to one server, asked one command at a time.
#[derive(Debug)]
pub struct RespConnection {
    stream: ServerStream,
    received: Vec<u8>,
    bound: usize,
    within: Duration,
}

impl RespConnection {
    /// A connection over a socket already open, bounded the way every read here is.
    pub fn over(stream: impl Into<ServerStream>) -> Result<Self, CollectionError> {
        Self {
            stream: stream.into(),
            received: Vec::new(),
            bound: REPLY_BOUND,
            within: ANSWER_WITHIN,
        }
        .timing_out_after(ANSWER_WITHIN)
    }

    /// A connection to the socket discovery chose.
    pub fn dial(target: &DialTarget) -> Result<Self, CollectionError> {
        let stream: io::Result<ServerStream> = match target {
            DialTarget::Unix(path) => UnixStream::connect(path).map(ServerStream::from),
            DialTarget::Tcp(address) => {
                TcpStream::connect_timeout(address, CONNECT_WITHIN).map(ServerStream::from)
            }
        };

        let stream = stream.map_err(|error| {
            CollectionError::new(format!("could not connect to {target}: {error}"))
        })?;

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

    /// Sends one command and reads its whole reply.
    ///
    /// Always the multi-bulk encoding, never the inline one: the inline protocol splits on
    /// spaces, and a password holding one would arrive as two arguments.
    pub fn ask(&mut self, arguments: &[&str]) -> Result<Reply, CollectionError> {
        let command = arguments.first().copied().unwrap_or_default();

        self.send(command, arguments)?;
        self.receive(command)
    }

    fn send(&mut self, command: &str, arguments: &[&str]) -> Result<(), CollectionError> {
        let frame = OwnedFrame::Array(
            arguments
                .iter()
                .map(|argument| OwnedFrame::BulkString(argument.as_bytes().to_vec()))
                .collect(),
        );

        let mut encoded = vec![0; frame.encode_len(false)];
        encode(&mut encoded, &frame, false).map_err(|error| {
            CollectionError::new(format!("{command} could not be encoded: {error}"))
        })?;

        self.stream
            .write_all(&encoded)
            .and_then(|()| self.stream.flush())
            .map_err(|error| self.failure(command, &error))
    }

    fn receive(&mut self, command: &str) -> Result<Reply, CollectionError> {
        let mut chunk = vec![0; READ_CHUNK];

        loop {
            let decoded = decode(&self.received).map_err(|error| {
                CollectionError::new(format!(
                    "the server's reply to {command} is not the redis protocol: {error}"
                ))
            })?;

            if let Some((frame, consumed)) = decoded {
                self.received.drain(..consumed);
                return Reply::try_from(frame);
            }

            // One byte past the bound is enough to know it was breached, and no more is read.
            let room = (self.bound + 1).saturating_sub(self.received.len());
            let wanted = room.min(READ_CHUNK);
            let read = self
                .stream
                .read(&mut chunk[..wanted])
                .map_err(|error| self.failure(command, &error))?;

            if read == 0 {
                return Err(CollectionError::new(format!(
                    "the server closed the connection before its reply to {command} was complete"
                )));
            }

            self.received.extend_from_slice(&chunk[..read]);

            if self.received.len() > self.bound {
                return Err(CollectionError::new(format!(
                    "the server's reply to {command} ran past {bound} bytes, so it was refused \
                     rather than truncated",
                    bound = self.bound
                )));
            }
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
