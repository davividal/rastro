//! The separations inside this crate, which cargo does not draw for us.
//!
//! Between crates the dependency arrows are enforced by the compiler, because a
//! cycle will not build. `cli` and `collectors` are siblings in one crate, so
//! nothing but this file stops them reaching into each other.
//!
//! It is a substring scan, and worth knowing what that cannot see: a
//! `super::super::cli::` path would slip past, and only line comments are
//! stripped, not block comments. It catches the accidental `use`, which is the
//! breach that actually happens, and nothing subtler.

use std::fs;
use std::path::{Path, PathBuf};

fn sources_of(module: &str) -> Vec<PathBuf> {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut found = Vec::new();
    let mut pending = vec![source.join(module)];

    if source.join(format!("{module}.rs")).exists() {
        found.push(source.join(format!("{module}.rs")));
    }

    while let Some(current) = pending.pop() {
        if !current.is_dir() {
            continue;
        }
        for entry in fs::read_dir(&current).expect("a readable directory") {
            let path = entry.expect("a readable directory entry").path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                found.push(path);
            }
        }
    }

    assert!(!found.is_empty(), "no source found for module {module:?}");
    found
}

/// Comment lines are skipped, so prose explaining a boundary does not read as a
/// breach of it.
fn code_of(file: &Path) -> String {
    fs::read_to_string(file)
        .expect("a readable source file")
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect::<Vec<&str>>()
        .join("\n")
}

fn assert_module_never_mentions(module: &str, forbidden: &[&str], why: &str) {
    for file in sources_of(module) {
        let code = code_of(&file);
        for needle in forbidden {
            assert!(
                !code.contains(needle),
                "{} mentions {needle:?}: {why}",
                file.display()
            );
        }
    }
}

#[test]
fn collectors_know_nothing_about_the_command_line() {
    // Act & Assert
    assert_module_never_mentions(
        "collectors",
        &["crate::cli", "rastro::cli"],
        "how a collector was invoked is none of its business",
    );
}

#[test]
fn the_command_line_knows_nothing_about_collectors() {
    // Act & Assert
    assert_module_never_mentions(
        "cli",
        &["crate::collectors", "rastro::collectors"],
        "the command line reads an invocation; what gets collected is decided \
         in main, which is the only place that knows both",
    );
}

#[test]
fn collectors_reach_the_document_model_only_through_the_port() {
    // Act & Assert: `rastro-collector` promises that one dependency is enough
    // to write a collector. The built-ins are the worked examples, so if one of
    // them reaches around the port the promise is quietly untrue for everyone
    // copying them. The fix is to widen the port's re-exports, not to reach.
    assert_module_never_mentions(
        "collectors",
        &["rastro_fingerprint"],
        "add the missing type to `rastro-collector`'s re-exports instead",
    );
}
