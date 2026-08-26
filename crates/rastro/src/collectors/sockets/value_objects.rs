//! The leaves of the sockets facet.
//!
//! A host and a port are not among them: they are shared with the exporters facet and
//! live in [`inet`](crate::collectors::inet), re-exported here so this facet's leaves
//! still read as one set.

mod interface_scope;
mod process_name;
mod socket_kind;
mod socket_path;
mod socket_state;

pub use interface_scope::InterfaceScope;
pub use process_name::ProcessName;
pub use socket_kind::SocketKind;
pub use socket_path::SocketPath;
pub use socket_state::SocketState;

pub use crate::collectors::inet::{InetHost, PortNumber};
