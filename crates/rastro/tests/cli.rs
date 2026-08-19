//! Behaviour of the binary as an operator invokes it.
//!
//! These tests exercise the real executable rather than library functions,
//! because rastro's invariants about streams and exit codes are only
//! observable from outside the process.

use std::process::{Command, Output};

use serde_json::{Value, json};

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
fn a_bare_run_reports_the_loaded_modules_as_state() {
    // Act
    let modules = facet(&document(&[]), "facets", "modules").clone();

    // Assert: `absent` is a legitimate answer, from a kernel built without
    // `CONFIG_MODULES`, so the status is not asserted to be `ok`. What must hold is
    // that the facet is there and did not fail.
    assert_ne!(modules["status"], "error", "got {modules:?}");
}

#[test]
fn a_bare_run_reports_the_installed_packages_as_state() {
    // Act
    let packages = facet(&document(&[]), "facets", "packages").clone();

    // Assert: `absent` is a legitimate answer on a host with neither dpkg nor apk, which
    // is how this holds off Linux. What must never happen is a failure.
    assert_ne!(packages["status"], "error", "got {packages:?}");
}

#[test]
fn no_kernel_pointer_reaches_the_document() {
    // Act: the complete view is the one that keeps volatile values, so a load address
    // that was merely annotated rather than dropped would surface here.
    let rendered = String::from_utf8(run(&["--include-volatile"]).stdout)
        .expect("stdout should carry a UTF-8 document");

    // Assert: `/proc/modules` publishes each module's kernel text address. It changes
    // every boot, so it is noise, and it leaks a KASLR offset into a document that gets
    // copied off the box and stored.
    assert!(
        !rendered.contains("0xffff"),
        "a kernel pointer reached stdout"
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

/// A config file in the temp dir, named per test so parallel runs cannot clash.
fn config_file(name: &str, contents: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("rastro-{name}.toml"));
    std::fs::write(&path, contents).expect("the temp directory should be writable");
    path
}

#[test]
fn a_run_with_no_config_collects_everything() {
    // Act
    let document = document(&[]);

    // Assert: the premise is a box nobody documented, so the default cannot ask
    // the operator which collectors they want.
    let invocation = facet(&document, "metadata", "invocation").clone();
    assert_eq!(
        invocation["data"]["config"]["excluded_collectors"],
        json!([])
    );
    assert_eq!(invocation["data"]["config"]["view"], json!("diffable"));
    assert_eq!(invocation["data"]["config"]["source"], Value::Null);
    assert!(!document["facets"].as_array().expect("facets").is_empty());
}

#[test]
fn an_excluded_collector_is_omitted_rather_than_recorded_absent() {
    // Arrange
    let path = config_file("exclude-mounts", "[collectors]\nexclude = [\"mounts\"]\n");

    // Act
    let output = run(&["--config", path.to_str().expect("a UTF-8 temp path")]);
    let document: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should carry a document");

    // Assert: absence is something observed about the host; exclusion is
    // something the operator chose, and the two must not be conflated.
    assert!(output.status.success());
    let names: Vec<&str> = document["facets"]
        .as_array()
        .expect("facets")
        .iter()
        .filter_map(|facet| facet["name"].as_str())
        .collect();
    assert!(!names.contains(&"mounts"), "got {names:?}");
}

#[test]
fn an_exclusion_is_announced_on_stderr() {
    // Arrange
    let path = config_file("warn-mounts", "[collectors]\nexclude = [\"mounts\"]\n");

    // Act
    let output = run(&["--config", path.to_str().expect("a UTF-8 temp path")]);

    // Assert: the only trace in the document is the effective config, so the
    // operator has to be told directly.
    let stderr = String::from_utf8(output.stderr).expect("stderr should be UTF-8");
    assert!(stderr.contains("mounts"), "got {stderr:?}");
}

#[test]
fn the_effective_config_reaches_the_document() {
    // Arrange
    let path = config_file("effective", "[collectors]\nexclude = [\"mounts\"]\n");

    // Act
    let output = run(&["--config", path.to_str().expect("a UTF-8 temp path")]);
    let document: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should carry a document");

    // Assert: two runs under different scope must not be diffable without the
    // difference showing.
    let config = facet(&document, "metadata", "invocation")["data"]["config"].clone();
    assert_eq!(config["excluded_collectors"], json!(["mounts"]));
    assert_eq!(config["view"], json!("diffable"));
    assert_eq!(
        config["source"],
        json!(path.to_str().expect("a UTF-8 temp path"))
    );
}

#[test]
fn a_config_path_that_cannot_be_read_fails_the_run() {
    // Act: falling back to the defaults would silently widen the run.
    let output = run(&["--config", "/nonexistent/rastro.toml"]);

    // Assert
    assert!(!output.status.success());
    assert!(output.stdout.is_empty(), "stdout carries only fingerprints");
    assert!(!output.stderr.is_empty());
}

#[test]
fn excluding_a_collector_that_does_not_exist_fails_the_run() {
    // Arrange
    let path = config_file("typo", "[collectors]\nexclude = [\"mount\"]\n");

    // Act
    let output = run(&["--config", path.to_str().expect("a UTF-8 temp path")]);

    // Assert: a typo would otherwise leave `mounts` running while the operator
    // believed it was switched off.
    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("stderr should be UTF-8");
    assert!(stderr.contains("mount"), "got {stderr:?}");
}

#[test]
fn excluding_a_metadata_collector_fails_the_run() {
    // Arrange
    let path = config_file(
        "no-invocation",
        "[collectors]\nexclude = [\"invocation\"]\n",
    );

    // Act
    let output = run(&["--config", path.to_str().expect("a UTF-8 temp path")]);

    // Assert
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
}
