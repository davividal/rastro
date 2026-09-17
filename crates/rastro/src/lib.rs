//! The tool's own parts: the collectors that ship in the binary, and the
//! command line.
//!
//! A library target beside the binary so the collectors can be tested directly
//! rather than only through the process.

/// The version this binary reports, everywhere it reports one.
///
/// **A build may override the crate version, and the rolling build does.** rastro is at
/// `0.0.0` and every development build carries that same number, so a fingerprint taken by
/// one cannot be told from a fingerprint taken by another, which defeats the `invocation`
/// facet's whole job of saying what produced the document. A build sets
/// `RASTRO_BUILD_VERSION` to a semver string naming the commit
/// (`0.0.0-rolling+abc1234`) and this reports that instead.
///
/// Read here rather than at each use, because the command line and the `invocation` facet
/// both report a version and two `env!` sites are two things to keep in step.
pub const VERSION: &str = match option_env!("RASTRO_BUILD_VERSION") {
    Some(version) => version,
    None => env!("CARGO_PKG_VERSION"),
};

pub mod cli;
pub mod collectors;
pub mod config;
pub mod output;
pub mod preflight;
pub mod progress;
