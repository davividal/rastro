//! This crate must build and run on a machine that is not the target platform.
//!
//! Cargo already stops it depending on the tool, since the dependency arrow
//! only points the other way and a crate cycle will not compile. What Cargo
//! cannot stop is this crate reaching the host directly, which is what would
//! quietly make the model untestable off Linux.

use std::collections::{BTreeMap, BTreeSet};
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

/// Which sibling modules each module of this crate reaches into.
fn module_graph() -> BTreeMap<String, BTreeSet<String>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let modules: Vec<String> = fs::read_to_string(root.join("lib.rs"))
        .expect("a readable lib.rs")
        .lines()
        .filter_map(|line| {
            Some(
                line.trim()
                    .strip_prefix("pub mod ")?
                    .strip_suffix(';')?
                    .to_owned(),
            )
        })
        .collect();

    let mut graph = BTreeMap::new();
    for module in &modules {
        let owned: Vec<PathBuf> = sources()
            .into_iter()
            .filter(|file| {
                file.file_stem().is_some_and(|stem| stem == module.as_str())
                    || file.parent().is_some_and(|parent| parent.ends_with(module))
            })
            .collect();

        let mut reaches = BTreeSet::new();
        for file in owned {
            let source = fs::read_to_string(&file).expect("a readable source file");
            for line in source.lines().filter(|l| !l.trim_start().starts_with("//")) {
                for other in &modules {
                    if other != module && line.contains(&format!("crate::{other}::")) {
                        reaches.insert(other.clone());
                    }
                }
            }
        }
        graph.insert(module.clone(), reaches);
    }
    graph
}

#[test]
fn the_modules_of_this_crate_form_no_cycle() {
    // Arrange
    let graph = module_graph();

    // Act & Assert: cargo cannot catch this one, because it is inside a single
    // crate. Two modules that reach into each other are one module pretending
    // to be two, which is what splitting the collector port out of the identity
    // types fixed.
    //
    // This also carries the older rule that observations know nothing about
    // documents, but only while the edge runs `facet -> observation`. If that
    // direction ever reverses the rule inverts silently, so the intended
    // direction is recorded here rather than left implicit.
    fn walk(
        graph: &BTreeMap<String, BTreeSet<String>>,
        node: &str,
        path: &mut Vec<String>,
    ) -> Option<Vec<String>> {
        if let Some(at) = path.iter().position(|seen| seen == node) {
            let mut cycle = path[at..].to_vec();
            cycle.push(node.to_owned());
            return Some(cycle);
        }
        path.push(node.to_owned());
        for next in graph.get(node).into_iter().flatten() {
            if let Some(cycle) = walk(graph, next, path) {
                return Some(cycle);
            }
        }
        path.pop();
        None
    }

    for module in graph.keys() {
        if let Some(cycle) = walk(&graph, module, &mut Vec::new()) {
            panic!("modules form a cycle: {}", cycle.join(" -> "));
        }
    }
}
