//! Behaviour of the binary as an operator invokes it.
//!
//! These tests exercise the real executable rather than library functions,
//! because rastro's invariants about streams and exit codes are only
//! observable from outside the process.

use std::process::{Command, Output};

use serde_json::Value;

const BINARY: &str = env!("CARGO_BIN_EXE_rastro");

fn run(arguments: &[&str]) -> Output {
    Command::new(BINARY)
        .args(arguments)
        .output()
        .expect("the binary under test should be executable")
}

fn document(arguments: &[&str]) -> Value {
    let output = run(arguments);
    assert!(output.status.success(), "rastro should have succeeded");
    serde_json::from_slice(&output.stdout).expect("stdout should carry a JSON document")
}

fn facet<'a>(document: &'a Value, section: &str, name: &str) -> &'a Value {
    document[section]
        .as_array()
        .expect("a section is an array of facets")
        .iter()
        .find(|facet| facet["name"] == name)
        .unwrap_or_else(|| panic!("expected a {name:?} facet in {section}"))
}

#[test]
fn run_with_version_flag_prints_the_crate_version() {
    // Act
    let output = run(&["--version"]);

    // Assert
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("version output should be UTF-8");
    assert!(
        stdout.contains(env!("CARGO_PKG_VERSION")),
        "reported version should match the crate version, got: {stdout}"
    );
}

#[test]
fn a_bare_run_writes_a_fingerprint_to_stdout() {
    // Act
    let document = document(&[]);

    // Assert
    assert_eq!(document["schema_version"], 1);
    assert!(document["metadata"].is_array());
    assert!(document["facets"].is_array());
}

#[test]
fn a_bare_run_reports_the_hostname_as_metadata() {
    // Act
    let host = facet(&document(&[]), "metadata", "host").clone();

    // Assert
    assert_eq!(host["status"], "ok");
    assert!(
        host["data"]["hostname"]
            .as_str()
            .is_some_and(|hostname| !hostname.is_empty()),
        "expected a non-empty hostname, got {:?}",
        host["data"]
    );
}

#[test]
fn a_bare_run_reports_the_mount_table_as_state() {
    // Act
    let mounts = facet(&document(&[]), "facets", "mounts").clone();

    // Assert
    assert_eq!(mounts["status"], "ok");
    assert!(
        !mounts["data"]
            .as_array()
            .expect("mounts are a list")
            .is_empty(),
        "a running host has at least one mount"
    );
}

#[test]
fn including_volatile_values_carries_the_run_timestamp() {
    // Act
    let invocation = facet(&document(&["--include-volatile"]), "metadata", "invocation").clone();

    // Assert
    assert!(invocation["data"]["started_at"].is_i64());
}

#[test]
fn a_bare_run_omits_the_run_timestamp() {
    // Act: the diffable view is the default, so this needs no flag.
    let invocation = facet(&document(&[]), "metadata", "invocation").clone();

    // Assert
    assert_eq!(
        invocation["data"]["rastro_version"],
        env!("CARGO_PKG_VERSION")
    );
    assert!(
        invocation["data"].get("started_at").is_none(),
        "the run timestamp is volatile and must not reach the diffable view"
    );
}

#[test]
fn two_bare_runs_produce_byte_identical_output() {
    // Act
    let first = run(&[]).stdout;
    let second = run(&[]).stdout;

    // Assert: the determinism contract, end to end through the real binary,
    // with no flag needed to get it.
    assert_eq!(first, second);
}

#[test]
fn stderr_stays_empty_on_a_successful_run() {
    // Act
    let output = run(&[]);

    // Assert
    assert!(
        output.stderr.is_empty(),
        "diagnostics belong on stderr, but a clean run has none: {:?}",
        String::from_utf8_lossy(&output.stderr)
    );
}
