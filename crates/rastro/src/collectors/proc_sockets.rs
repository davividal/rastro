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
//! families, its host-order hexadecimal words, its wildcard spellings and its per-protocol
//! state vocabulary, stays in the `sockets` collector: that is a document shape rather than a
//! shared mechanism, and the decisions behind it are recorded about that facet. What this
//! module reads of the table is two columns and a state, which is a narrower question asked
//! of the same file rather than a second answer to the same one. The residue is that the port
//! hexadecimal is decoded in both places, six lines of it.

mod listening_inodes;
mod socket_holders;

pub use listening_inodes::listening_inodes;
pub use socket_holders::{HeldDescriptor, SocketHolders};
