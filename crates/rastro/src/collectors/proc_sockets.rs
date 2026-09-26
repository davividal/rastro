//! What `/proc` says about a socket: who holds it, and which of them is offered on a port.
//!
//! Shared, because two collectors meet the same host interface from different directions.
//! `sockets` needs the holder of every socket on the box, to report what is listening and
//! which process is behind it. `rabbitmq` needs the opposite question about one socket: the
//! Erlang port mapper names a node and the port it accepts distribution connections on, and
//! whether the process holding that port is really a broker decides whether rastro may
//! address it at all.
//!
//! Beside [`canonical_tool`](super::canonical_tool) and [`file_glob`](super::file_glob) for
//! the reason those give: one place to be right about a fiddly host interface rather than one
//! per caller that can drift.
//!
//! **What is deliberately not here.** The full reading of an inet table, with its address
//! families, its wildcard spellings and its per-protocol state vocabulary, stays in the
//! `sockets` collector: that is a document shape rather than a shared mechanism, and the
//! decisions behind it are recorded about that facet. What is here is the spelling underneath
//! it, the host-order hexadecimal words and the unix table's trailing path, since `redis` needs
//! a server's own addresses too and a second decoder is how two facets come to disagree about
//! one socket. The residue is that the port hexadecimal is decoded in both places, six lines
//! of it.

mod kernel_address;
mod listeners;
mod listening_inodes;
mod socket_holders;
mod unix_columns;

pub use kernel_address::{ipv4_of, ipv6_of};
pub use listeners::{InetListener, UnixListener, inet_listeners, unix_listeners};
pub use listening_inodes::listening_inodes;
pub use socket_holders::{HeldDescriptor, SocketHolders, sockets_held_by};
pub use unix_columns::{UnixColumns, unix_columns};
