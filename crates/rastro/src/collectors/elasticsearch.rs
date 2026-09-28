//! Layer 3: what an Elasticsearch node is actually running with.
//!
//! **The node's own HTTP API is the only place its effective state lives**, so this is the one
//! collector that makes a request over the network, and the boundary it works inside is
//! narrow: a `GET`, to a listener held by a process already found in `/proc`, from inside
//! that process's network namespace, and never a request that could write. See
//! `docs/decisions.md`.
pub mod source;
pub mod value_objects;

pub use source::{NodeListener, NodeSettings, ResidentNode, http_endpoint};
pub use value_objects::{HttpEndpoint, Transport, Unread};
