//! Where the document goes, and what the file it lands in is called.
//!
//! The end-to-end tests here live in their own binary rather than in `cli.rs`, and that is
//! load-bearing: they write files, `CARGO_TARGET_TMPDIR` is inside the tree a walk of the real
//! host covers, and `cli.rs` holds the determinism harness. Two tests in one binary run
//! concurrently, so a fingerprint written by one would land in the document of the other.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use rastro::output::{Destination, default_file_name, utc_stamp};
use serde_json::Value;
mod support;

use support::narrowing::without_walking;

const BINARY: &str = env!("CARGO_BIN_EXE_rastro");

/// The facet a section holds under this name.
fn facet<'a>(document: &'a Value, section: &str, name: &str) -> &'a Value {
    document[section]
        .as_array()
        .expect("a section is an array of facets")
        .iter()
        .find(|facet| facet["name"] == name)
        .unwrap_or_else(|| panic!("expected a {name:?} facet in {section}"))
}

#[test]
fn utc_stamp_writes_a_sortable_instant_without_a_colon() {
    // Act & Assert: no colon, because a name with one needs shell quoting, breaks on VFAT and
    // exFAT, and reads as a host separator to `scp` and `rsync`. Sortable because an archive
    // of these is meant to be listed in order.
    for (seconds, expected) in [
        (0, "19700101T000000Z"),
        (1_234_567_890, "20090213T233130Z"),
        (1_786_632_455, "20260813T144735Z"),
    ] {
        assert_eq!(utc_stamp(seconds), expected, "{seconds}");
    }
}

#[test]
fn utc_stamp_gets_the_calendar_right_where_it_is_easy_to_get_wrong() {
    // Act & Assert: 2000 is a leap year because it divides by 400, 2100 is not because it
    // divides by 100 and not 400, and 2038 is past where a 32-bit count would have stopped.
    assert_eq!(utc_stamp(951_782_400), "20000229T000000Z");
    assert_eq!(utc_stamp(4_107_542_400), "21000301T000000Z");
    assert_eq!(utc_stamp(2_147_483_648), "20380119T031408Z");
}

#[test]
fn a_default_name_carries_the_host_and_the_instant() {
    // Act
    let name = default_file_name(Some("mr-d0-pgsql-01"), 1_786_632_455);

    // Assert: the hostname is in it because the realistic archive is many boxes' fingerprints
    // in one directory, and it is the one fact that identifies a document without opening it.
    assert_eq!(name, "rastro-mr-d0-pgsql-01-20260813T144735Z.json");
}

#[test]
fn a_default_name_leaves_out_a_hostname_it_could_not_read() {
    // Act & Assert: a segment for a host nobody could name would say nothing, so it goes
    // rather than becoming `unknown`, which would collide across every such box.
    assert_eq!(
        default_file_name(None, 1_786_632_455),
        "rastro-20260813T144735Z.json"
    );
}

#[test]
fn a_default_name_cannot_be_steered_out_of_the_working_directory() {
    // Arrange: the hostname comes from `/proc/sys/kernel/hostname`, which is settable, and
    // rastro runs as root. A name of `../../etc` would otherwise put the file wherever the
    // host asked, which is a traversal in a root process rather than a cosmetic problem.
    let name = default_file_name(Some("../../etc/cron.d/evil"), 1_786_632_455);

    // Assert: one path component and nothing else. A `.` inside a filename is harmless, so
    // the property worth asserting is that no separator survived and the name cannot climb —
    // not that the characters of `..` are gone.
    assert_eq!(
        Path::new(&name).components().count(),
        1,
        "{name} is not a single path component"
    );
    assert!(!name.contains('/'), "got {name}");
    assert!(name.starts_with("rastro-"), "got {name}");
}

#[test]
fn a_default_name_drops_a_hostname_that_survives_filtering_as_nothing() {
    // Act & Assert: filter-and-fall-back rather than filter-and-hope. `///` keeps no legal
    // character, so the segment is omitted exactly as an unreadable hostname is.
    assert_eq!(
        default_file_name(Some("///"), 1_786_632_455),
        "rastro-20260813T144735Z.json"
    );
}

#[test]
fn a_long_hostname_is_cut_rather_than_making_an_unusable_name() {
    // Act
    let name = default_file_name(Some(&"a".repeat(300)), 1_786_632_455);

    // Assert: most filesystems stop at 255 bytes for one component, so a name built from an
    // unbounded hostname would fail to open rather than merely look silly.
    assert!(name.len() < 120, "got {} bytes", name.len());
}

#[test]
fn a_dash_means_stdout_rather_than_a_file_called_dash() {
    // Act & Assert: the escape hatch every pipeline needs, and `rastro-ssh` depends on it.
    assert_eq!(
        Destination::resolve(Some(Path::new("-")), None, 0),
        Destination::Stdout
    );
}

#[test]
fn a_named_path_is_taken_as_given() {
    // Act & Assert: no timestamp, no hostname, no surprises. An operator who names a path is
    // usually feeding a script that already chose it.
    assert_eq!(
        Destination::resolve(Some(Path::new("/tmp/before.json")), Some("box"), 0),
        Destination::File(Path::new("/tmp/before.json").to_path_buf())
    );
}

#[test]
fn no_path_means_the_default_name_in_the_working_directory() {
    // Act
    let resolved = Destination::resolve(None, Some("box"), 1_786_632_455);

    // Assert
    assert_eq!(
        resolved,
        Destination::File(Path::new("rastro-box-20260813T144735Z.json").to_path_buf())
    );
}

/// A scratch directory this test owns, so a fingerprint written into it disturbs nobody.
fn scratch(name: &str) -> PathBuf {
    let directory = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("a writable scratch directory");

    directory
}

/// The same, in a directory of its own, for the tests that are about the file itself.
fn run_in(directory: &Path, arguments: &[&str]) -> Output {
    Command::new(BINARY)
        .current_dir(directory)
        .args(arguments)
        .output()
        .expect("the binary under test should be executable")
}

#[test]
fn a_bare_run_writes_the_document_to_a_file_and_nothing_to_stdout() {
    // Arrange
    let directory = scratch("bare-run-writes-a-file");

    // Act
    let output = run_in(&directory, &["--config", without_walking()]);

    // Assert: a fingerprint of a real host is megabytes, so the default is a file. stdout
    // carrying nothing at all still satisfies "stdout carries only the fingerprint".
    assert!(output.status.success(), "rastro should have succeeded");
    assert!(output.stdout.is_empty(), "got {:?}", output.stdout.len());

    let written: Vec<_> = std::fs::read_dir(&directory)
        .expect("a readable scratch directory")
        .map(|entry| entry.expect("a readable entry").file_name())
        .collect();
    assert_eq!(written.len(), 1, "got {written:?}");

    let name = written[0].to_string_lossy().into_owned();
    assert!(name.starts_with("rastro-"), "got {name}");
    assert!(name.ends_with(".json"), "got {name}");
    serde_json::from_slice::<Value>(
        &std::fs::read(directory.join(&name)).expect("a readable document"),
    )
    .expect("the file carries a JSON document");
}

#[test]
fn the_output_file_is_created_private_to_its_owner() {
    // Arrange
    use std::os::unix::fs::PermissionsExt;
    let directory = scratch("output-is-private");

    // Act
    run_in(&directory, &["--config", without_walking()]);

    // Assert: a fingerprint names every path on the box, so it is nobody else's business. Set
    // at creation rather than after, because a chmod afterwards leaves a readable window.
    let path = std::fs::read_dir(&directory)
        .expect("a readable scratch directory")
        .map(|entry| entry.expect("a readable entry").path())
        .next()
        .expect("one document");
    let mode = std::fs::metadata(&path)
        .expect("a readable document")
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o600, "got {:o}", mode & 0o777);
}

#[test]
fn an_existing_output_file_is_refused_rather_than_overwritten() {
    // Arrange: the workflow is a `before` and an `after`, so replacing the `before` destroys
    // the only record of the state being compared against.
    let directory = scratch("refuse-to-overwrite");
    let target = directory.join("before.json");
    std::fs::write(&target, "the state this run would be compared against").expect("a write");

    // Act
    let output = run_in(
        &directory,
        &["-o", "before.json", "--config", without_walking()],
    );

    // Assert
    assert!(!output.status.success(), "rastro should have refused");
    let complaint = String::from_utf8_lossy(&output.stderr);
    assert!(complaint.contains("before.json"), "got {complaint}");
    assert!(complaint.contains("--force"), "got {complaint}");
    assert_eq!(
        std::fs::read_to_string(&target).expect("the original is intact"),
        "the state this run would be compared against"
    );
}

#[test]
fn force_replaces_the_file_and_the_result_is_still_private() {
    // Arrange: written 0640 first, which is the trap. A mode applies when a file is created, so
    // a truncate-in-place would have left this group-readable rather than tightening it. 0640
    // rather than 0644 for the same reason the fixtures use it: no test needs to hand the world
    // a read bit, and granting one is a finding in a scan.
    use std::os::unix::fs::PermissionsExt;
    let directory = scratch("force-replaces");
    let target = directory.join("before.json");
    std::fs::write(&target, "stale").expect("a write");
    std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o640)).expect("a chmod");

    // Act
    let output = run_in(
        &directory,
        &[
            "-o",
            "before.json",
            "--force",
            "--config",
            without_walking(),
        ],
    );

    // Assert
    assert!(output.status.success(), "rastro should have succeeded");
    let mode = std::fs::metadata(&target)
        .expect("a readable document")
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o600, "got {:o}", mode & 0o777);
    serde_json::from_slice::<Value>(&std::fs::read(&target).expect("a readable document"))
        .expect("the file carries a JSON document");
}

#[test]
fn a_write_that_cannot_happen_names_the_path_and_leaves_nothing() {
    // Arrange: a parent that cannot be a directory, so the write fails with `ENOTDIR` for
    // *any* user. The first version made the parent mode 0500, which root ignores — so it
    // skipped on every local run and only ever really executed on CI, where it then caught
    // that the failure named the staging file rather than the path the operator typed.
    let directory = scratch("write-that-fails");
    let blocking = directory.join("notadir");
    std::fs::write(&blocking, "a file where a directory would have to be").expect("a write");

    // Act
    let output = run_in(
        &directory,
        &["-o", "notadir/before.json", "--config", without_walking()],
    );

    // Assert: it fails, and it names the destination rather than the temporary file, because
    // that is the path the operator chose and the only one they can act on.
    assert!(!output.status.success(), "rastro should have failed");
    let complaint = String::from_utf8_lossy(&output.stderr);
    assert!(
        complaint.contains("notadir/before.json"),
        "the failure should name the destination, got {complaint}"
    );
    assert!(
        !complaint.contains(".partial"),
        "the failure should not send the operator after a temporary file, got {complaint}"
    );

    // Assert: and nothing was disturbed on the way.
    assert_eq!(
        std::fs::read_to_string(&blocking).expect("the blocking file is intact"),
        "a file where a directory would have to be"
    );
    let left: Vec<String> = std::fs::read_dir(&directory)
        .expect("a listable directory")
        .map(|entry| {
            entry
                .expect("a readable entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    assert_eq!(left, vec!["notadir".to_owned()], "got {left:?}");
}

#[test]
fn the_default_file_name_agrees_with_the_started_at_in_the_document() {
    // Arrange
    let directory = scratch("name-agrees-with-document");

    // Act
    run_in(
        &directory,
        &["--include-volatile", "--config", without_walking()],
    );

    // Assert: one clock reading serves both, so the name and the document cannot disagree.
    // Two reads could straddle a second and produce a file whose name contradicts itself.
    let path = std::fs::read_dir(&directory)
        .expect("a readable scratch directory")
        .map(|entry| entry.expect("a readable entry").path())
        .next()
        .expect("one document");
    let document: Value =
        serde_json::from_slice(&std::fs::read(&path).expect("a readable document"))
            .expect("a JSON document");
    let started_at = facet(&document, "metadata", "invocation")["data"]["started_at"]
        .as_i64()
        .expect("the invocation facet stamps the run");

    let name = path.file_name().expect("a named file").to_string_lossy();
    assert!(
        name.contains(&rastro::output::utc_stamp(started_at)),
        "{name} does not carry {started_at}"
    );
}

#[test]
fn the_invocation_facet_admits_the_output_file_it_left_out() {
    // Arrange
    let directory = scratch("output-is-declared");

    // Act
    run_in(
        &directory,
        &[
            "-o",
            "fingerprint.json",
            "--include-volatile",
            "--config",
            without_walking(),
        ],
    );
    let document: Value = serde_json::from_slice(
        &std::fs::read(directory.join("fingerprint.json")).expect("a readable document"),
    )
    .expect("a JSON document");

    // Assert: an omission the document does not admit to is the one thing this format does
    // not do. Volatile for the same reason the observer is: the path carries a timestamp.
    let output = &facet(&document, "metadata", "invocation")["data"]["output"];
    assert!(
        output
            .as_str()
            .is_some_and(|path| path.ends_with("fingerprint.json")),
        "got {output}"
    );
}

#[test]
#[cfg(target_os = "linux")]
fn a_device_destination_is_written_through_and_not_replaced() {
    // Arrange: rastro runs as root, so publishing by rename would replace the null device
    // with a regular file and leave the box worse than it found it. Only meaningful as a
    // user that could actually do the damage.
    use std::os::unix::fs::FileTypeExt;
    let directory = scratch("device-destination");

    // Act
    let output = run_in(
        &directory,
        &["-o", "/dev/null", "--config", without_walking()],
    );

    // Assert: it worked, and /dev/null is still a character device.
    assert!(
        output.status.success(),
        "writing to /dev/null should work: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        std::fs::metadata("/dev/null")
            .expect("/dev/null exists")
            .file_type()
            .is_char_device(),
        "the null device was replaced with a regular file"
    );
    assert_eq!(
        std::fs::read_dir(&directory)
            .expect("a listable directory")
            .count(),
        0,
        "a staging file was left behind"
    );
}

#[test]
fn a_refused_publication_leaves_the_original_and_no_staging_file() {
    // Arrange
    let directory = scratch("refused-leaves-nothing");
    let target = directory.join("before.json");
    std::fs::write(&target, "the state this run would be compared against").expect("a write");

    // Act
    let output = run_in(
        &directory,
        &["-o", "before.json", "--config", without_walking()],
    );

    // Assert: the promise is that the `before` survives, and that nothing half-written is
    // left lying next to it. Published by `link` rather than `rename` so the refusal is
    // decided by the kernel at publication time rather than by a check taken earlier — the
    // race itself is not arranged here, since another process would have to win it.
    assert!(!output.status.success());
    assert_eq!(
        std::fs::read_to_string(&target).expect("the original is intact"),
        "the state this run would be compared against"
    );
    let left: Vec<String> = std::fs::read_dir(&directory)
        .expect("a listable directory")
        .map(|entry| {
            entry
                .expect("a readable entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    assert_eq!(left, vec!["before.json".to_owned()], "got {left:?}");
}

#[test]
fn a_config_can_seal_a_tree_so_the_walk_stops_there() {
    // Arrange: the whole point of the feature. Until this existed the only lever over which
    // trees the walk read was a collector's claim resolved from the host, so a runaway walk
    // needed a new binary — and CI had no way to say that its own build directory is noise.
    let directory = scratch("config-seals-a-tree");
    let noisy = directory.join("noise");
    std::fs::create_dir(&noisy).expect("a writable scratch directory");
    std::fs::write(noisy.join("churning.log"), "a line\n").expect("a write");
    std::fs::write(directory.join("kept.conf"), "a setting\n").expect("a write");

    let config = directory.join("rastro.toml");
    // No sealing of the shipped trees here, unlike the tests that only need a facet to exist:
    // this one asserts that a sibling of the sealed tree *is* still walked, and the scratch
    // directory it lives in sits under `/tmp` on some hosts. Sealing that would seal away the
    // very thing being checked, so this test pays for a real walk and is worth it.
    std::fs::write(
        &config,
        format!(
            "[filesystem]\nsealed = [{:?}]\n",
            noisy.to_str().expect("a UTF-8 scratch path")
        ),
    )
    .expect("a write");

    // Act
    let output = run_in(
        &directory,
        &[
            "-o",
            "fingerprint.json",
            "--config",
            config.to_str().expect("a UTF-8 scratch path"),
        ],
    );
    assert!(
        output.status.success(),
        "rastro should have succeeded: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let document: Value = serde_json::from_slice(
        &std::fs::read(directory.join("fingerprint.json")).expect("a readable document"),
    )
    .expect("a JSON document");

    // Assert: the sealed directory is recorded, nothing under it is, and a sibling the config
    // said nothing about is untouched. A narrowing narrows one tree.
    let walked = facet(&document, "facets", "filesystem")["data"]
        .as_object()
        .expect("the filesystem facet is keyed by path");
    let named = |path: &std::path::Path| path.to_str().expect("a UTF-8 path").to_owned();

    assert!(
        walked.contains_key(&named(&noisy)),
        "the tree itself is state"
    );
    assert!(
        !walked.contains_key(&named(&noisy.join("churning.log"))),
        "the walk descended into a sealed tree"
    );
    assert!(
        walked.contains_key(&named(&directory.join("kept.conf"))),
        "a narrowing narrowed something it was not asked to"
    );

    // Assert: and the envelope says the operator decided it, not rastro.
    let table = &facet(&document, "metadata", "invocation")["data"]["walk_policy"];
    assert_eq!(table[named(&noisy)]["reading"], "sealed");
    assert_eq!(table[named(&noisy)]["claimed_by"], "config");
}

#[test]
fn a_destination_with_no_filename_is_refused_rather_than_panicking() {
    // Arrange: `-o /` is a plausible slip, and it has no file name to stage beside. It must
    // fail like any other unusable path rather than panic on an `expect`.
    let directory = scratch("destination-is-the-root");

    // Act
    let output = run_in(&directory, &["-o", "/", "--config", without_walking()]);

    // Assert
    assert!(!output.status.success(), "rastro should have refused");
    assert!(
        !String::from_utf8_lossy(&output.stderr).contains("panicked"),
        "got {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn a_destination_that_is_a_directory_is_refused_and_left_alone() {
    // Arrange: `-o somedir` is the other plausible slip. A directory is not a regular file, so
    // it takes the write-through path — which must fail on the open rather than replace the
    // directory, given rastro runs as root.
    let directory = scratch("destination-is-a-directory");
    let target = directory.join("already-a-directory");
    std::fs::create_dir(&target).expect("a writable scratch directory");
    std::fs::write(target.join("inside"), "a file nobody asked rastro to touch").expect("a write");

    // Act
    let output = run_in(
        &directory,
        &["-o", "already-a-directory", "--config", without_walking()],
    );

    // Assert
    assert!(!output.status.success(), "rastro should have refused");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("already-a-directory"),
        "the failure should name the destination"
    );
    assert!(target.is_dir(), "the directory was replaced");
    assert_eq!(
        std::fs::read_to_string(target.join("inside")).expect("the contents are intact"),
        "a file nobody asked rastro to touch"
    );
}

#[test]
fn the_resolved_output_path_reaches_both_the_walk_and_the_envelope() {
    // Arrange: what is left for the binary to prove. That the walk *omits* the path is asserted
    // over a scratch root in `filesystem_scope.rs`, where the walk can be scoped and the claim
    // is exact; through the binary the same assertion cost over two minutes on a
    // coverage-instrumented runner, because it had to walk a whole host for the file to be in.
    //
    // What only an end-to-end run can show is that one resolved path reaches both places, so
    // the omission and the declaration cannot be two different spellings of one file. Through a
    // symlinked directory, because that is where the two spellings diverge.
    let directory = scratch("resolved-output-reaches-both");
    let real = directory.join("real");
    std::fs::create_dir(&real).expect("a writable scratch directory");
    std::os::unix::fs::symlink(&real, directory.join("linked")).expect("a symlink");

    // Act
    let output = run_in(
        &directory,
        &[
            "-o",
            "linked/fingerprint.json",
            "--include-volatile",
            "--config",
            without_walking(),
        ],
    );
    assert!(
        output.status.success(),
        "rastro should have succeeded: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let document: Value = serde_json::from_slice(
        &std::fs::read(real.join("fingerprint.json")).expect("a readable document"),
    )
    .expect("a JSON document");

    // Assert: declared as the walk would have met it, with the symlink resolved away.
    let declared = facet(&document, "metadata", "invocation")["data"]["output"]
        .as_str()
        .expect("the invocation facet declares the output")
        .to_owned();
    let met_as = real
        .canonicalize()
        .expect("a real directory")
        .join("fingerprint.json");
    assert_eq!(
        declared,
        met_as.to_str().expect("a UTF-8 path"),
        "the declared path is not the one the walk would meet"
    );
}
