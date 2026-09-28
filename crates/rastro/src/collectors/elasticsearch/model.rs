//! What the facet reports: an installation, and the nodes running on the box.

mod installation;
mod node;
mod node_identity;

pub use installation::Installation;
pub use node::Node;
pub use node_identity::{NodeIdentity, NodeVersion};
