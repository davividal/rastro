//! The tool's own parts: the collectors that ship in the binary, and the
//! command line.
//!
//! A library target beside the binary so the collectors can be tested directly
//! rather than only through the process.

pub mod cli;
pub mod collectors;
pub mod config;
pub mod output;
pub mod preflight;
pub mod progress;
