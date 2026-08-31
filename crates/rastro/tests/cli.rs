//! Behaviour of the binary as an operator invokes it.
//!
//! These tests exercise the real executable rather than library functions,
//! because rastro's invariants about streams and exit codes are only
//! observable from outside the process.

use std::path::Path;
use std::process::{Command, Output};

use serde_json::{Value, json};

const BINARY: &str = env!("CARGO_BIN_EXE_rastro");

/// Runs the binary, sending the document to stdout unless the caller named a destination.
///
/// The default is a file now, so a bare `run(&[])` would drop a fingerprint into whatever
/// directory the test happened to run in. `-o -` is also what every assertion on `stdout`
/// here needs, and what `rastro-ssh` passes for the same reason.
fn run(arguments: &[&str]) -> Output {
    let names_output = arguments.contains(&"-o");
    let mut command = Command::new(BINARY);

    if !names_output {
        command.args(["-o", "-"]);
    }

    command
        .args(arguments)
        .output()
        .expect("the binary under test should be executable")
}

fn document(arguments: &[&str]) -> Value {
    let output = run(arguments);
    assert!(output.status.success(), "rastro should have succeeded");
    serde_json::from_slice(&output.stdout).expect("stdout should carry a JSON document")
}

/// The filesystem facet's entries, or a failure naming why the walk did not finish.
///
/// A walk of the real host is only as good as the host: one path that will not decode as
/// UTF-8 anywhere on the box refuses the whole facet, so a bare `expect` here would report
/// "no entries" for something that is really a named refusal.
fn walked_paths(document: &Value) -> &serde_json::Map<String, Value> {
    let facet = facet(document, "facets", "filesystem");
    assert_eq!(
        facet["status"], "ok",
        "the filesystem facet did not survive this host: {}",
        facet["error"]
    );

    facet["data"]
        .as_object()
        .expect("the filesystem facet is keyed by path")
}

/// The path of a config that excludes the filesystem walk.
///
/// **Written once, for speed rather than for coverage.** A walk of the whole host costs
/// seconds under a coverage-instrumented binary on a runner whose disk carries a cargo
/// registry and a target directory, and the tests here invoke the binary dozens of times
/// between them. The tests that never read the `filesystem` facet skip it, which is the
/// difference between a suite measured in minutes and one measured in seconds.
///
/// It also removes a second problem: an instrumented run drops a `.profraw` file into the
/// target directory, which a walk of the whole host then reports — so a test that walks
/// changes what the next walk sees.
fn without_walking() -> &'static str {
    static PATH: std::sync::OnceLock<String> = std::sync::OnceLock::new();

    PATH.get_or_init(|| {
        // Per process, because `cargo nextest` gives each test one: several processes
        // writing one path would let a reader catch a partial write.
        let path = Path::new(env!("CARGO_TARGET_TMPDIR"))
            .join(format!("no-filesystem-walk-{}.toml", std::process::id()));
        std::fs::write(&path, "[collectors]\nexclude = [\"filesystem\"]\n")
            .expect("a writable scratch directory");

        path.to_str().expect("a UTF-8 scratch path").to_owned()
    })
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
    let output = run(&["--version", "--config", without_walking()]);

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
    let document = document(&["--config", without_walking()]);

    // Assert
    assert_eq!(document["schema_version"], 1);
    assert!(document["metadata"].is_array());
    assert!(document["facets"].is_array());
}

#[test]
fn a_bare_run_reports_the_hostname_as_metadata() {
    // Act
    let host = facet(
        &document(&["--config", without_walking()]),
        "metadata",
        "host",
    )
    .clone();

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
    let mounts = facet(
        &document(&["--config", without_walking()]),
        "facets",
        "mounts",
    )
    .clone();

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
    let modules = facet(
        &document(&["--config", without_walking()]),
        "facets",
        "modules",
    )
    .clone();

    // Assert: `absent` is a legitimate answer, from a kernel built without
    // `CONFIG_MODULES`, so the status is not asserted to be `ok`. What must hold is
    // that the facet is there and did not fail.
    assert_ne!(modules["status"], "error", "got {modules:?}");
}

#[test]
fn a_bare_run_reports_the_installed_packages_as_state() {
    // Act
    let packages = facet(
        &document(&["--config", without_walking()]),
        "facets",
        "packages",
    )
    .clone();

    // Assert: always `ok`, on every host. rastro can always report the state of the managers it
    // reads, and both are always named, so a box with neither is described rather than reported
    // as a failure.
    assert_eq!(packages["status"], "ok", "got {packages:?}");
    assert!(packages["data"].get("apk").is_some(), "got {packages:?}");
    assert!(packages["data"].get("dpkg").is_some(), "got {packages:?}");

    // And where dpkg is genuinely installed, that it was genuinely read. `get` returns
    // `Some(Null)` for a null key and the status is `ok` either way, so without this the whole
    // detect-run-parse chain could go dark in CI with every test still green.
    if std::path::Path::new("/usr/bin/dpkg-query").is_file() {
        assert!(
            packages["data"]["dpkg"]
                .as_object()
                .is_some_and(|packages| !packages.is_empty()),
            "dpkg-query is installed here, so its packages should have been read: {packages:?}"
        );
    }
}

#[test]
fn no_kernel_pointer_reaches_the_document() {
    // Act: the complete view is the one that keeps volatile values, so a load address
    // that was merely annotated rather than dropped would surface here.
    let rendered =
        String::from_utf8(run(&["--include-volatile", "--config", without_walking()]).stdout)
            .expect("stdout should carry a UTF-8 document");

    // Assert: `/proc/modules` publishes each module's kernel text address. It changes every
    // boot, so it is noise, and it leaks a KASLR offset into a document that gets copied off
    // the box and stored.
    //
    // Both forms, because a reader without `CAP_SYSLOG` sees zeros rather than a real
    // pointer, which is exactly the case in a container. Asserting only the real form left
    // this test unable to fail in the environment it runs in.
    for pointer in ["0xffff", "0x00000000"] {
        assert!(
            !rendered.contains(pointer),
            "a kernel address reached stdout as {pointer}"
        );
    }
}

#[test]
fn including_volatile_values_carries_the_run_timestamp() {
    // Act
    let invocation = facet(
        &document(&["--include-volatile", "--config", without_walking()]),
        "metadata",
        "invocation",
    )
    .clone();

    // Assert
    assert!(invocation["data"]["started_at"].is_i64());
}

#[test]
fn a_bare_run_omits_the_run_timestamp() {
    // Act: the diffable view is the default, so this needs no flag.
    let invocation = facet(
        &document(&["--config", without_walking()]),
        "metadata",
        "invocation",
    )
    .clone();

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
/// Compared on stdout rather than on two files, and that is not incidental: a fingerprint
/// written anywhere on real disk is an entry of the next run's document, so two runs writing
/// to files would differ for a reason that has nothing to do with the renderer. Do not
/// "fix" this back to comparing files.
///
/// **Without the filesystem facet, and that is not a weakening of the contract but the only
/// way to state it honestly here.** This walks the machine the suite is running on, which is
/// not an unchanged host: sibling tests write coverage files into the tree, and a CI runner
/// writes its own logs while this runs. Comparing whole-host walks would assert that nothing
/// on the box moved during the test, which is not rastro's promise and not true.
///
/// The filesystem facet's own byte-identity is `determinism.rs`, over a tree that test owns —
/// including the case this one could never arrange, where something volatile really does change
/// between the two readings and the diffable view has to be identical anyway.
fn two_bare_runs_produce_byte_identical_output() {
    // Act
    let first = run(&["--config", without_walking()]).stdout;
    let second = run(&["--config", without_walking()]).stdout;

    // Assert: the determinism contract for the envelope and every other facet, end to end
    // through the real binary.
    //
    // The comparison is on bytes, because bytes are the contract. The *message* is
    // not: this test used to `assert_eq!` the two `Vec<u8>` directly, and when it
    // finally caught something it reported two four-hundred-kilobyte byte arrays,
    // which told a CI log reader nothing at all. Naming the facet is what turns a
    // failure here into a starting point instead of a puzzle.
    if first != second {
        panic!("{}", divergence(&first, &second));
    }
}

/// Which facets two runs disagreed about, for the failure above.
///
/// Reached only when the byte comparison has already failed, so it is free to be
/// slower and more thorough than the assertion it explains.
fn divergence(first: &[u8], second: &[u8]) -> String {
    let (Ok(first), Ok(second)) = (
        serde_json::from_slice::<Value>(first),
        serde_json::from_slice::<Value>(second),
    ) else {
        return "two runs differed, and at least one did not parse as JSON".to_owned();
    };

    let mut report = String::from("two runs of an unchanged host differed.\n");
    for section in ["metadata", "facets"] {
        for name in facet_names(&first, section) {
            let (before, after) = (
                facet(&first, section, &name),
                facet(&second, section, &name),
            );
            if before == after {
                continue;
            }

            report.push_str(&format!("\n  {section}/{name} differs:\n"));
            report.push_str(&detail(&before["data"], &after["data"]));
        }
    }

    report
}

/// What changed inside one facet's data, in terms of the shape it has.
///
/// **A structural comparison, not a text excerpt.** The first version of this printed the
/// first four hundred characters of each side, which for a facet whose difference is one
/// key out of eleven hundred showed two identical prefixes and told nobody anything. What a
/// reader needs is the key, or the entry, that moved.
fn detail(before: &Value, after: &Value) -> String {
    match (before, after) {
        (Value::Object(before), Value::Object(after)) => {
            let mut lines = String::new();
            for key in union(before.keys().chain(after.keys()).cloned()) {
                let (was, now) = (before.get(&key), after.get(&key));
                if was == now {
                    continue;
                }
                lines.push_str(&format!(
                    "    {key}: {} -> {}\n",
                    rendered(was),
                    rendered(now)
                ));
            }
            lines
        }
        (Value::Array(before), Value::Array(after)) => {
            // Counted, not set-compared: two runs catching three and then five identical
            // entries differ in bytes while sharing every distinct one.
            let mut lines = format!("    {} entries -> {}\n", before.len(), after.len());
            let counts = |items: &[Value]| {
                let mut counted: std::collections::BTreeMap<String, usize> = Default::default();
                for item in items {
                    *counted.entry(item.to_string()).or_default() += 1;
                }
                counted
            };
            let (before, after) = (counts(before), counts(after));
            for entry in union(before.keys().chain(after.keys()).cloned()) {
                let (was, now) = (
                    before.get(&entry).copied().unwrap_or_default(),
                    after.get(&entry).copied().unwrap_or_default(),
                );
                if was == now {
                    continue;
                }
                lines.push_str(&format!("    {was} -> {now}  {}\n", clipped(&entry)));
            }
            lines
        }
        _ => format!(
            "    first:  {}\n    second: {}\n",
            clipped(&before.to_string()),
            clipped(&after.to_string())
        ),
    }
}

/// Every key either side has, sorted, so the report reads the same way twice.
fn union(keys: impl Iterator<Item = String>) -> std::collections::BTreeSet<String> {
    keys.collect()
}

fn rendered(value: Option<&Value>) -> String {
    value.map_or_else(
        || "<absent>".to_owned(),
        |value| clipped(&value.to_string()),
    )
}

fn facet_names(document: &Value, section: &str) -> Vec<String> {
    document[section]
        .as_array()
        .expect("a section is an array of facets")
        .iter()
        .filter_map(|facet| facet["name"].as_str().map(str::to_owned))
        .collect()
}

/// How much of one value the message shows.
const CLIPPED: usize = 200;

/// One value, short enough to read in a CI log.
///
/// Cut on a character boundary rather than a byte offset, because a facet may hold text
/// from the host and slicing mid-sequence would panic.
fn clipped(value: &str) -> String {
    if value.len() <= CLIPPED {
        return value.to_owned();
    }

    let end = value
        .char_indices()
        .map(|(index, _)| index)
        .take_while(|index| *index <= CLIPPED)
        .last()
        .unwrap_or(0);

    format!("{}... ({} bytes)", &value[..end], value.len())
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
    // Act: no config, which is the subject. It pays for a whole walk to say so.
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
fn a_bare_run_reports_the_binary_it_is_running_from() {
    // Act
    let document = document(&["--include-volatile", "--config", without_walking()]);

    // Assert: rastro installed on a box is part of that box, and a binary somebody swapped
    // is exactly the change this tool exists to catch. The default hides nothing.
    let config = facet(&document, "metadata", "invocation")["data"]["config"].clone();
    assert_eq!(config["staged_binary"], json!(false));
}

#[test]
fn a_staged_run_says_so_in_the_effective_config() {
    // Act: the flag `rastro-ssh` passes, because the party that made the temporary copy is
    // the only one that knows the file will be gone a second later.
    let document = document(&[
        "--staged",
        "--include-volatile",
        "--config",
        without_walking(),
    ]);

    // Assert: an omission the operator can see was requested, rather than a rule that
    // quietly drops a path.
    let config = facet(&document, "metadata", "invocation")["data"]["config"].clone();
    assert_eq!(config["staged_binary"], json!(true));
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

#[test]
fn a_bare_run_records_each_walked_path_as_one_digest() {
    // Act
    let document = document(&[]);

    // Assert: the default view answers "did anything about this path change", and a digest is
    // the whole of that answer. Listing eleven attributes per path cost 444 bytes an entry
    // against 81 for this, on a document that is 80% filesystem facet.
    let entries = walked_paths(&document);
    let (path, digest) = entries
        .iter()
        .find(|(_, value)| value.is_string())
        .expect("at least one path is described rather than refused");
    let digest = digest.as_str().expect("a described path is its digest");

    assert_eq!(digest.len(), 16, "{path} rendered as {digest:?}");
    assert!(
        digest
            .chars()
            .all(|character| character.is_ascii_hexdigit()),
        "{path} rendered as {digest:?}"
    );
}

#[test]
fn detail_records_every_attribute_the_walk_read() {
    // Act
    let document = document(&["--detail"]);

    // Assert: the digest says a path moved and not which attribute did, so this is how to ask.
    // It has to be asked at the time: a summary taken yesterday cannot be expanded today.
    let entries = walked_paths(&document);
    let (path, attributes) = entries
        .iter()
        .find(|(_, value)| value.get("kind").is_some())
        .expect("at least one path is described in full");

    assert!(attributes["mode"].is_string(), "{path}: {attributes}");
    assert!(attributes["owner"].is_i64(), "{path}: {attributes}");
    assert!(attributes["inode"].is_i64(), "{path}: {attributes}");
}

#[test]
fn the_effective_config_records_which_detail_the_run_was_taken_at() {
    // Act
    let summary = document(&["--config", without_walking()]);
    let full = document(&["--detail", "--config", without_walking()]);

    // Assert: two documents taken at different detail cannot be diffed against each other, so
    // the difference has to show in the envelope rather than being inferred from the shape.
    let recorded = |document: &Value| {
        facet(document, "metadata", "invocation")["data"]["config"]["detail"]
            .as_str()
            .expect("the effective config names the detail")
            .to_owned()
    };

    assert_eq!(recorded(&summary), "summary");
    assert_eq!(recorded(&full), "full");
}

#[test]
fn debug_reports_a_line_per_collector_on_stderr() {
    // Act
    let output = run(&["--debug"]);

    // Assert: on stderr, because stdout carries only the fingerprint, and named per collector
    // because "it took 40 seconds" is not actionable while "the filesystem walk took 39 of
    // them" is.
    assert!(output.status.success(), "rastro should have succeeded");
    let reported = String::from_utf8_lossy(&output.stderr);
    assert!(reported.contains("filesystem"), "got {reported}");
    assert!(reported.contains("invocation"), "got {reported}");
    assert!(reported.contains("total"), "got {reported}");
}

#[test]
fn debug_reports_what_the_walk_cost_and_where_the_document_went() {
    // Act
    let output = run(&["--debug"]);

    // Assert: the two questions an operator actually has after a slow run, and the reason
    // this exists at all: `time ./rastro > file` answers neither.
    let reported = String::from_utf8_lossy(&output.stderr);
    assert!(reported.contains("entries"), "got {reported}");
    assert!(reported.contains("wrote"), "got {reported}");
}

#[test]
fn debug_writes_no_timing_into_the_document() {
    // Act
    let document = document(&[
        "--debug",
        "--include-volatile",
        "--config",
        without_walking(),
    ]);

    // Assert: a fingerprint records what a box *is*, not what it is doing, so a duration has
    // no place in it — and it would be volatile anyway, so the default view would drop it and
    // two runs would stop being comparable at the complete view.
    //
    // Asserted on the `invocation` facet, which is where a run describes itself and therefore
    // the only place a timing could land. Grepping the whole document was the first attempt
    // and it was wrong: under `--include-volatile` the document carries process command lines
    // and unit descriptions from the live host, so it failed on whatever happened to be
    // running rather than on anything rastro wrote.
    let run = &facet(&document, "metadata", "invocation")["data"];
    let described = run
        .as_object()
        .expect("the invocation facet describes the run")
        .keys()
        .cloned()
        .collect::<Vec<String>>();
    for key in &described {
        for forbidden in ["elapsed", "duration", "taken", "seconds", "millis"] {
            assert!(
                !key.contains(forbidden),
                "the invocation facet carries {key:?}"
            );
        }
    }
    assert!(
        described.contains(&"started_at".to_owned()),
        "the run should still stamp itself: {described:?}"
    );
}

#[test]
fn a_clean_run_without_debug_still_says_nothing_on_stderr() {
    // Act
    let output = run(&[]);

    // Assert: the contract `--debug` must not quietly break. Diagnostics belong on stderr, and
    // a clean run has none.
    assert!(
        output.stderr.is_empty(),
        "got {:?}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn progress_forced_on_reaches_stderr_even_when_it_is_not_a_terminal() {
    // Act: forced, because a test's stderr is a pipe and the counter is off there by default.
    // This is the only way to exercise the renderer without a pty.
    let output = run(&["--progress", "--config", without_walking()]);

    // Assert: a counter, not a bar. The walk discovers its own work as it goes, so a
    // percentage would need a denominator nobody has, and a number that moves smoothly and
    // means nothing is worse than no number.
    assert!(output.status.success(), "rastro should have succeeded");
    let shown = String::from_utf8_lossy(&output.stderr);
    assert!(shown.contains("entries"), "got {shown}");
    assert!(
        !shown.contains('%'),
        "a percentage would be invented: {shown}"
    );
}

#[test]
fn no_progress_keeps_stderr_empty_even_on_a_terminal() {
    // Act
    let output = run(&["--no-progress"]);

    // Assert
    assert!(
        output.stderr.is_empty(),
        "got {:?}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn progress_and_no_progress_together_are_refused() {
    // Act
    let output = run(&["--progress", "--no-progress", "--config", without_walking()]);

    // Assert: asking for both is a mistake in a script, and guessing which was meant would
    // hide it. clap refuses it before the run starts.
    assert!(!output.status.success(), "rastro should have refused");
    assert!(output.stdout.is_empty());
}
