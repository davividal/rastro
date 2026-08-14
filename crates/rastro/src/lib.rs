//! The tool's own parts: the collectors that ship in the binary, and the
//! command line.
//!
//! A library target beside the binary so the collectors can be tested directly
//! rather than only through the process.

#![deny(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]

pub mod cli;
pub mod collectors;
