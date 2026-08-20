//! The host interfaces the sysctl facet can be read from.

mod proc_sys;
pub mod proc_sys_entry;

pub use proc_sys::ProcSys;
