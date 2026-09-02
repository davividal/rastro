//! One row of a `/proc` socket table, before its holder is known.

use crate::collectors::sockets::model::SocketAddress;
use crate::collectors::sockets::value_objects::{SocketKind, SocketState};

/// A socket as `/proc` describes it, carrying the inode that leads to its process.
///
/// **A source-layer type, and the inode is why.** `/proc/net/*` names no process, so the
/// holder has to be joined on afterwards from `/proc/<pid>/fd`. The inode is the join key
/// and nothing else: it is a kernel object identifier that changes every time a service
/// restarts, so it never reaches the document and a [`ListeningSocket`] has no field for
/// it.
///
/// [`ListeningSocket`]: crate::collectors::sockets::model::ListeningSocket
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SocketRow {
    pub kind: SocketKind,
    pub state: SocketState,
    pub address: SocketAddress,
    pub inode: u64,
}
