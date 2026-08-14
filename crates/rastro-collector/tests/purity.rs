//! This crate must build and run on a machine that is not the target platform.
//!
//! Cargo already stops it depending on the tool, since the dependency arrow
//! only points the other way and a crate cycle will not compile. What Cargo
//! cannot stop is this crate reaching the host directly, which is what would
//! quietly make the model untestable off Linux.

use std::fs;
use std::path::{Path, PathBuf};

fn sources() -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut pending = vec![Path::new(env!("CARGO_MANIFEST_DIR")).join("src")];

    while let Some(current) = pending.pop() {
        for entry in fs::read_dir(&current).expect("the crate has a src directory") {
            let path = entry.expect("a readable directory entry").path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                found.push(path);
            }
        }
    }

    found
}

#[test]
fn nothing_here_reads_the_host() {
    // Act & Assert
    for file in sources() {
        let source = fs::read_to_string(&file).expect("a readable source file");
        // Comment lines are skipped so that prose explaining why the host is
        // off limits does not read as a violation of itself.
        let code: String = source
            .lines()
            .filter(|line| !line.trim_start().starts_with("//"))
            .collect::<Vec<&str>>()
            .join("\n");

        for forbidden in [
            "std::fs",
            "std::process",
            "std::net",
            "std::env",
            "SystemTime::now",
        ] {
            assert!(
                !code.contains(forbidden),
                "{} reaches the host via {forbidden:?}; that belongs behind the \
                 Collector port, in a crate that is allowed to",
                file.display()
            );
        }
    }
}
