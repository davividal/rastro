//! How a node is read: one module per host interface.

mod epmd_register;
mod resident_runtime;

pub use epmd_register::{EpmdRegister, RegisteredNode};
pub use resident_runtime::ResidentRuntime;
