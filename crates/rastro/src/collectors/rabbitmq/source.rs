//! How a node is read: one module per host interface.

mod epmd_register;

pub use epmd_register::{EpmdRegister, RegisteredNode};
