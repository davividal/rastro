//! How a node is read: one module per host interface.

mod node_settings;
mod resident_node;

pub use node_settings::{NodeSettings, UnreadSettings};
pub use resident_node::ResidentNode;
