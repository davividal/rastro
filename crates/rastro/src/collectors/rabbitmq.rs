//! Layer 3: what a RabbitMQ node is actually running with.
//!
//! **Nothing is asked speculatively, and that is the facet's first rule rather than a
//! precaution.** A RabbitMQ CLI tool is not a client that opens a socket: it boots an Erlang
//! VM and joins the broker's distribution cluster, and a call that fails because no node is
//! there still leaves an `epmd -daemon` running on a box that had none. Measured, twice, as
//! root and as the broker's own user. So the dispatch starts from what is already resident:
//! epmd in the process list, then the register it keeps, then a CLI tool addressed at a node
//! the register named. See `docs/decisions.md`.
pub mod model;
pub mod source;
pub mod value_objects;

pub use model::{Installation, Node};
pub use source::{EpmdRegister, NodeInventory, RegisteredNode, ResidentRuntime};
pub use value_objects::NodeName;
