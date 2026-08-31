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

const BINARY: &str = env!("CARGO_BIN_EXE_rastro");

/// A config that excludes the filesystem walk, for the tests that never read that facet.
///
/// These tests are about where a document goes and what the file looks like, not about what is
/// in it. A walk of the whole runner under a coverage-instrumented binary costs seconds, and
/// there are a dozen invocations here.
fn without_walking() -> &'static str {
    static PATH: std::sync::OnceLock<String> = std::sync::OnceLock::new();

    PATH.get_or_init(|| {
        // Per process, because `cargo nextest` gives each test one: several processes
        // writing one path would let a reader catch a partial write.
        let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!(
            "output-no-filesystem-walk-{}.toml",
            std::process::id()
        ));
        std::fs::write(&path, "[collectors]\nexclude = [\"filesystem\"]\n")
            .expect("a writable scratch directory");

        path.to_str().expect("a UTF-8 scratch path").to_owned()
    })
}

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
    // Arrange: written 0644 first, which is the trap. A mode applies when a file is created,
    // so a truncate-in-place would have left this world-readable.
    use std::os::unix::fs::PermissionsExt;
    let directory = scratch("force-replaces");
    let target = directory.join("before.json");
    std::fs::write(&target, "stale").expect("a write");
    std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o644)).expect("a chmod");

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
fn a_run_leaves_its_own_output_file_out_of_the_document() {
    // Arrange: run one writes the file; run two's walk finds it sitting there. Without the
    // omission the most natural use of `-o` breaks the byte-identical guarantee by a megabyte,
    // at the one facet that dominates the document.
    let directory = scratch("output-left-out-of-the-walk");
    let target = directory.join("fingerprint.json");
    assert!(
        run_in(&directory, &["-o", "fingerprint.json"])
            .status
            .success(),
        "the first run should have succeeded"
    );
    assert!(target.exists(), "the first run should have written it");

    // Act
    assert!(
        run_in(&directory, &["-o", "fingerprint.json", "--force"])
            .status
            .success(),
        "the second run should have succeeded"
    );
    let document: Value =
        serde_json::from_slice(&std::fs::read(&target).expect("a readable document"))
            .expect("a JSON document");

    // Assert: asserted on the one path rather than on the whole document, because sibling
    // tests in this binary write files of their own while this runs. Whole-document identity
    // is the determinism harness's job, and it has no such neighbours.
    let walked = facet(&document, "facets", "filesystem")["data"]
        .as_object()
        .expect("the filesystem facet is keyed by path");
    let named = target.to_str().expect("a UTF-8 scratch path");
    assert!(
        !walked.contains_key(named),
        "the run reported the document it was writing"
    );
    assert!(
        walked
            .keys()
            .any(|key| key.starts_with(directory.to_str().expect("a UTF-8 scratch path"))),
        "the scratch directory itself should still be walked"
    );
}

#[test]
fn the_invocation_facet_admits_the_output_file_it_left_out() {
    // Arrange
    let directory = scratch("output-is-declared");

    // Act
    run_in(
        &directory,
        &["-o", "fingerprint.json", "--include-volatile"],
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
fn an_output_path_through_a_symlinked_directory_is_still_left_out_of_the_walk() {
    // Arrange: `std::path::absolute` is lexical, so it keeps the symlink in the path. The walk
    // never follows a symlink, so it meets the file under the real directory instead — and the
    // two spellings would not match, putting the previous document back in the next run.
    let directory = scratch("symlinked-output-parent");
    let real = directory.join("real");
    let linked = directory.join("linked");
    std::fs::create_dir(&real).expect("a writable scratch directory");
    std::os::unix::fs::symlink(&real, &linked).expect("a symlink");

    assert!(
        run_in(&directory, &["-o", "linked/fingerprint.json"])
            .status
            .success(),
        "the first run should have succeeded"
    );

    // Act
    assert!(
        run_in(&directory, &["-o", "linked/fingerprint.json", "--force"])
            .status
            .success(),
        "the second run should have succeeded"
    );
    let document: Value = serde_json::from_slice(
        &std::fs::read(real.join("fingerprint.json")).expect("a readable document"),
    )
    .expect("a JSON document");

    // Assert: keyed by the path the walk met it under, which is the real one.
    let walked = facet(&document, "facets", "filesystem")["data"]
        .as_object()
        .expect("the filesystem facet is keyed by path");
    let met_as = real
        .canonicalize()
        .expect("a real directory")
        .join("fingerprint.json");
    assert!(
        !walked.contains_key(met_as.to_str().expect("a UTF-8 path")),
        "the run reported the document it was writing"
    );
}
