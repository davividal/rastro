//! Where the mount table comes from, and how that host spells it.
//!
//! The anti-corruption boundary. Everything peculiar to one interface stays here:
//! its location, its column order, its escaping. The model on the other side of it
//! is what rastro means, so adding `/proc/self/mountinfo` later is a second source
//! rather than a change to the model.

mod proc_mounts;
mod proc_mounts_line;

pub use proc_mounts::ProcMounts;
pub use proc_mounts_line::ProcMountsLine;
