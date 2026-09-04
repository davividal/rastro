//! The host interfaces the processes facet can be read from.

pub mod proc_cmdline;
pub mod proc_processes;
pub mod proc_status;

pub use proc_processes::ProcProcesses;
