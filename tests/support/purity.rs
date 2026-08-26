#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};

pub const FORBIDDEN_HOST_READS: &[&str] = &[
    "std::fs",
    "std::process",
    "std::net",
    "std::env",
    "SystemTime::now",
];

pub fn sources_in(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut pending = vec![root.to_path_buf()];

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

pub fn code_without_line_comments(source: &str) -> String {
    source
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect::<Vec<&str>>()
        .join("\n")
}

pub fn assert_no_host_reads(root: &Path) {
    for file in sources_in(root) {
        let source = fs::read_to_string(&file).expect("a readable source file");
        let code = code_without_line_comments(&source);

        for forbidden in FORBIDDEN_HOST_READS {
            assert!(
                !code.contains(forbidden),
                "{} reaches the host via {forbidden:?}; that belongs behind the Collector port, in a crate that is allowed to",
                file.display()
            );
        }
    }
}
