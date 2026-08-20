//! What state a listening socket is in.

use rastro_collector::{CollectionError, NonEmptyText, Observation};

/// A socket's state, in the word `ss` prints for it.
///
/// `LISTEN` for a stream socket accepting connections, and `UNCONN` for a datagram
/// socket, which never listens in the TCP sense but is just as much a port the box has
/// open. Both are recorded, because a UDP port bound by `systemd-resolved` is as real a
/// piece of exposure as a TCP one.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct SocketState(NonEmptyText);

impl SocketState {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(NonEmptyText::new(value, "socket state")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<&SocketState> for Observation {
    fn from(state: &SocketState) -> Self {
        Observation::text(state.as_str())
    }
}
