//! One socket the host has open.

use std::collections::BTreeSet;

use rastro_collector::Observation;

use super::socket_address::SocketAddress;
use super::socket_process::SocketProcess;
use crate::collectors::sockets::value_objects::{SocketKind, SocketState};

/// A listening socket, in rastro's terms rather than `ss`'s.
///
/// **The queue depths are dropped at the boundary rather than recorded volatile.** `ss`
/// prints `Recv-Q` and `Send-Q`, and for a listening socket they are the current backlog
/// and the configured maximum. The backlog is noise, and the maximum is worth having, but
/// `ss` gives no way to tell rastro which column means which across socket types, so both
/// go. That is the same call `/proc/mounts`' constant `dump` and `fsck` columns get: a
/// column read and deliberately not carried into the model.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ListeningSocket {
    pub kind: SocketKind,
    pub state: SocketState,
    pub address: SocketAddress,
    /// A set, so the order `ss` happened to list two holders in never reaches the
    /// document. More than one is ordinary: `/run/systemd/journal/stdout` is held by both
    /// `systemd-journal` and `systemd` itself.
    pub processes: BTreeSet<SocketProcess>,
}

impl From<&ListeningSocket> for Observation {
    fn from(socket: &ListeningSocket) -> Self {
        Observation::object([
            ("address", Observation::from(&socket.address)),
            ("kind", Observation::from(&socket.kind)),
            (
                "processes",
                Observation::list(socket.processes.iter().map(Observation::from)),
            ),
            ("state", Observation::from(&socket.state)),
        ])
    }
}
