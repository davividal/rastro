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

/// Whether a file belongs to one of a collector's domain layers.
///
/// Two ways to belong, and the second is easy to miss: a file inside the layer's
/// directory, *or* the aggregator file declaring it, which sits beside that directory
/// rather than in it. Testing only the parent directory would leave `model.rs` and
/// `value_objects.rs` unscanned, and those are the one file per layer where a stray
/// `pub use super::source::X` would be least conspicuous.
fn belongs_to_layer(path: &Path, layer: &str) -> bool {
    let inside_the_directory = path
        .parent()
        .and_then(Path::file_name)
        .is_some_and(|directory| directory == layer);
    let is_the_aggregator = path.file_stem().is_some_and(|stem| stem == layer);

    inside_the_directory || is_the_aggregator
}

/// Every domain file of every collector, so a new collector is covered without touching
/// this file.
fn domain_sources_of_every_collector() -> Vec<PathBuf> {
    sources_of("collectors")
        .into_iter()
        .filter(|path| belongs_to_layer(path, "model") || belongs_to_layer(path, "value_objects"))
        .collect()
}

#[test]
fn a_collectors_domain_knows_nothing_about_the_host_interface_it_came_from() {
    // Arrange
    let domain = domain_sources_of_every_collector();
    assert!(!domain.is_empty(), "no collector has a domain to check");

    // Act & Assert: the anti-corruption boundary. A source holds one host
    // interface's spelling, so the moment the model reaches back into it, adding a
    // second interface reporting the same concepts stops being a local change.
    //
    // `canonical_tool` is on the list because it is the *other* route to the host, and
    // the one a future collector is most likely to take: sysctl, systemd units and
    // nftables all shell out. A model calling it directly would bypass the source layer
    // entirely while every needle about `source` stayed satisfied.
    for file in domain {
        let code = code_of(&file);
        // The last four are the set `rastro-collector`'s own purity test uses. Without them
        // the guard rests on a naming convention nobody wrote down: a future source called
        // `EtcPasswd` or `NetlinkSockets` matches none of the names above, and neither does
        // the shortest route around the boundary, a `model/` file reading `/proc` itself.
        for needle in [
            "source",
            "Proc",
            "Query",
            "Database",
            "canonical_tool",
            "CanonicalTool",
            "std::fs",
            "std::process",
            "std::net",
            "std::env",
        ] {
            assert!(
                !code.contains(needle),
                "{} mentions {needle:?}: the model is what rastro means, not how one \
                 host spells it. Map it in the source instead",
                file.display()
            );
        }
    }
}

#[test]
fn a_collectors_leaf_values_know_nothing_about_the_shape_they_compose_into() {
    // Act & Assert: `value_objects` holds the facet's leaves and `model` its structure,
    // so the arrow runs model to value_objects. Reversing it would make a leaf
    // unusable in any other shape.
    for file in domain_sources_of_every_collector() {
        if !belongs_to_layer(&file, "value_objects") {
            continue;
        }
        assert!(
            !code_of(&file).contains("model"),
            "{} mentions `model`: a leaf value is composed *by* a shape, never aware \
             of one",
            file.display()
        );
    }
}

#[test]
fn the_config_knows_nothing_about_what_it_configures() {
    // Act & Assert: a third sibling, added when the config layer landed. It
    // reads settings; deciding what they mean is the registry's job.
    assert_module_never_mentions(
        "config",
        &["crate::collectors", "crate::cli", "rastro::collectors"],
        "what a setting does is decided where the collectors are, not here",
    );
}

#[test]
fn the_config_is_a_plain_settings_type() {
    // Act & Assert: shaping the effective config into an observation belongs to
    // the collector that reports it, so this type has no document model at all.
    assert_module_never_mentions(
        "config",
        &["rastro_fingerprint", "rastro_collector"],
        "assembling a facet's tree is the collector's job",
    );
}
