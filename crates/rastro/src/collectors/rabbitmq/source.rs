//! How a node is read: one module per host interface.

mod epmd_register;
mod node_inventory;
mod resident_runtime;

pub use epmd_register::{EpmdRegister, RegisteredNode};
pub use node_inventory::NodeInventory;
pub use resident_runtime::ResidentRuntime;
