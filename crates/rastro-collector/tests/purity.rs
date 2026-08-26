//! This crate must build and run on a machine that is not the target platform.
//!
//! Cargo already stops it depending on the tool, since the dependency arrow
//! only points the other way and a crate cycle will not compile. What Cargo
//! cannot stop is this crate reaching the host directly, which is what would
//! quietly make the model untestable off Linux.

#[path = "../../../tests/support/purity.rs"]
mod purity_support;

use std::path::Path;

#[test]
fn nothing_here_reads_the_host() {
    // Act & Assert
    purity_support::assert_no_host_reads(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src"));
}
