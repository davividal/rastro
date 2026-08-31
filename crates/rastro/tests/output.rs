//! Where the document goes, and what the file it lands in is called.
//!
//! The end-to-end tests here live in their own binary rather than in `cli.rs`, and that is
//! load-bearing: they write files, `CARGO_TARGET_TMPDIR` is inside the tree a walk of the real
//! host covers, and `cli.rs` holds the determinism harness. Two tests in one binary run
//! concurrently, so a fingerprint written by one would land in the document of the other.

use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use rastro::output::{Counting, Destination, default_file_name, publish, utc_stamp};
use serde_json::Value;
mod support;

use support::narrowing::{sealing, trees_sealing_everything_except, without_walking};

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
    // Sealed by siblings rather than by ancestors, because this test needs its own scratch tree
    // genuinely walked: `SEALING_THE_SHIPPED_TREES` seals the root, so the walk would never
    // descend to find it, and it seals `/tmp`, where `CARGO_TARGET_TMPDIR` lives under the
    // container recipe. Walking the whole box instead cost three minutes for an assertion about
    // one directory.
    let mut sealed = trees_sealing_everything_except(&directory);
    sealed.push(noisy.to_str().expect("a UTF-8 scratch path").to_owned());
    std::fs::write(&config, sealing(&sealed)).expect("a write");

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

/// A staged document, and the name it is about to be published under.
///
/// `publish` is called directly by the tests below rather than through the binary, because
/// `to_file` refuses an existing destination before it stages anything: every refusal inside
/// `publish` belongs to the window between that check and the publication, and a test cannot win
/// that race. It can state it in two lines.
fn staged(name: &str) -> PathBuf {
    let directory = scratch(name);
    let staging = directory.join(".rastro-staging");
    std::fs::write(&staging, "a finished document").expect("a write");

    staging
}

#[test]
fn a_destination_that_appeared_while_the_document_was_rendering_is_refused() {
    // Arrange: the race the `link`-not-`rename` decision exists for. A big document takes a while
    // to write, and `to_file`'s check that the destination was free happened before that; this is
    // the moment afterwards, with the destination now taken.
    let staging = staged("publish-refuses-a-latecomer");
    let target = staging.with_file_name("before.json");
    std::fs::write(&target, "the state this run would be compared against").expect("a write");

    // Act
    let refused = publish(&staging, &target, false).expect_err("a destination that is taken");

    // Assert: the `before` survived, checked first and deliberately. This is the harm the
    // decision exists to prevent, so it is what a regression should report — swapping `link` for
    // `rename` makes this line fail with "the original was replaced" instead of leaving a reader
    // to work backwards from a confusing message about an unlinkable staging file.
    assert_eq!(
        std::fs::read_to_string(&target).expect("the original is still there"),
        "the state this run would be compared against",
        "the destination was replaced instead of refused"
    );

    // Assert: and the kernel is what decided it. No check taken earlier can close a window it
    // sits before, which is why the refusal lives at the moment of publication.
    assert!(
        refused.to_string().contains("already exists"),
        "got {refused}"
    );
}

#[test]
fn a_publication_that_fails_for_any_other_reason_names_the_destination() {
    // Arrange: a destination whose parent is not there, so `link` fails with something other
    // than EEXIST and the refusal must not claim the file already exists.
    let staging = staged("publish-names-the-destination");
    let target = staging.with_file_name("gone").join("before.json");

    // Act
    let refused = publish(&staging, &target, false).expect_err("a parent that is not there");

    // Assert: named for the destination the operator typed, not for rastro's staging file.
    let message = refused.to_string();
    assert!(message.contains("before.json"), "got {message}");
    assert!(!message.contains("already exists"), "got {message}");
    assert!(!message.contains(".rastro-staging"), "got {message}");
}

#[test]
fn a_forced_publication_that_cannot_replace_the_destination_says_so() {
    // Arrange: `--force` publishes by rename, and a rename cannot replace a directory with a
    // file. This is the arm that reports it rather than leaving the operator with a success and
    // no document.
    let staging = staged("publish-force-refused");
    let target = staging.with_file_name("before.json");
    std::fs::create_dir(&target).expect("a writable scratch directory");

    // Act
    let refused = publish(&staging, &target, true).expect_err("a directory in the way");

    // Assert
    assert!(
        refused
            .to_string()
            .contains("could not be replaced with the finished document"),
        "got {refused}"
    );
}

#[test]
fn a_staging_file_that_outlives_its_publication_is_reported() {
    // Arrange: the document is published, and then the staging link cannot be dropped. The run
    // succeeded at what it was for, so this is the one refusal named for the staging file — it is
    // the file the operator has to go and remove.
    //
    // Arranged by publishing *out of* a directory that is read-only: the link into the target
    // needs no write permission on the source's directory, but unlinking the source does.
    let staging = staged("publish-leaves-a-staging-file");
    let closed = staging.parent().expect("a scratch parent").to_owned();
    let target = closed.join("elsewhere").join("before.json");
    std::fs::create_dir(target.parent().expect("a parent")).expect("a writable scratch directory");

    let reopened = std::fs::Permissions::from_mode(0o700);
    std::fs::set_permissions(&closed, std::fs::Permissions::from_mode(0o500))
        .expect("a scratch directory this user owns");

    // Act
    let published = publish(&staging, &target, false);

    std::fs::set_permissions(&closed, reopened).expect("a scratch directory this user owns");

    // Assert
    if published.is_ok() {
        // Root carries `CAP_DAC_OVERRIDE` and unlinks regardless of the mode bits, so there is
        // nothing to report. The unprivileged CI runner is where this arm is exercised.
        eprintln!("skipped: this user unlinks from a directory without write permission");
        return;
    }

    let message = published
        .expect_err("an unlinkable staging file")
        .to_string();
    assert!(
        message.contains("was published but could not be unlinked"),
        "got {message}"
    );
    assert!(message.contains(".rastro-staging"), "got {message}");
    assert!(target.exists(), "the document itself was published");
}

#[test]
fn the_counting_writer_reports_what_went_through_it_and_flushes_the_writer() {
    // Arrange: the byte figure the `--debug` report prints comes from here rather than from
    // asking the filesystem, because stdout has nothing to ask.
    let mut sink: Vec<u8> = Vec::new();
    let mut counted = Counting::over(&mut sink);

    // Act
    counted.write_all(b"a document").expect("a write to memory");
    counted.flush().expect("a flush to memory");

    // Assert: and the flush is delegated rather than swallowed. Production never reaches it —
    // the `BufWriter` sits inside this, not around it — so a writer that quietly dropped a
    // flush would be a trap laid for the next caller rather than a bug seen today.
    assert_eq!(counted.bytes(), 10);
    assert_eq!(sink, b"a document");
}

#[test]
fn debug_names_the_file_the_document_went_to_and_how_big_it_was() {
    // Arrange: every other `--debug` test runs with `-o -`, so the arm that reports a *file* was
    // the one nobody exercised — and a file is the default. "Where did it go" is half of why
    // `--debug` exists.
    let directory = scratch("debug-names-the-file");

    // Act
    let output = run_in(
        &directory,
        &[
            "-o",
            "fingerprint.json",
            "--debug",
            "--config",
            without_walking(),
        ],
    );

    // Assert
    assert!(
        output.status.success(),
        "rastro should have succeeded: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let reported = String::from_utf8_lossy(&output.stderr);
    assert!(reported.contains("fingerprint.json"), "got {reported}");
    assert!(reported.contains("bytes"), "got {reported}");
}

#[test]
fn an_output_file_behind_a_symlinked_parent_is_still_left_out_of_the_walk() {
    // Arrange: `-o link/before.json`, where `link` is a symlink to a sibling directory. This is
    // the shape a reviewer flagged: `std::path::absolute` is purely lexical, so it would keep
    // `.../link/before.json` — while the walk never follows the symlink and meets the file under
    // `.../real/before.json` instead. The two paths would not match, the omission would miss, and
    // run one's document would land inside run two's.
    let directory = scratch("symlinked-output-parent");
    let real = directory.join("real");
    std::fs::create_dir(&real).expect("a writable scratch directory");
    std::os::unix::fs::symlink(&real, directory.join("link")).expect("a symlink");

    let config = directory.join("rastro.toml");
    let sealed = trees_sealing_everything_except(&directory);
    std::fs::write(&config, sealing(&sealed)).expect("a write");

    // Act: twice, and the second run is the whole point. On the first there is no output file
    // for the walk to find — it is created after the walk, from the document the walk produced —
    // so a single run cannot tell a working omission from a broken one.
    let arguments = [
        "-o",
        "link/before.json",
        "--config",
        config.to_str().expect("a UTF-8 scratch path"),
    ];
    let first = run_in(&directory, &arguments);
    assert!(
        first.status.success(),
        "the first run should have succeeded: {}",
        String::from_utf8_lossy(&first.stderr)
    );

    let mut again = arguments.to_vec();
    again.push("--force");
    let output = run_in(&directory, &again);
    assert!(
        output.status.success(),
        "the second run should have succeeded: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let document: Value = serde_json::from_slice(
        &std::fs::read(real.join("before.json")).expect("a document behind the symlink"),
    )
    .expect("a JSON document");

    // Assert: the walk left it out under the path the walk actually meets it by — the physical
    // one. Asserting on the lexical path would pass even with the bug, since the walk never
    // produces that key either way.
    let walked = facet(&document, "facets", "filesystem")["data"]
        .as_object()
        .expect("the filesystem facet is keyed by path");
    let physical = std::fs::canonicalize(&real)
        .expect("a real directory")
        .join("before.json");

    assert!(
        !walked.contains_key(physical.to_str().expect("a UTF-8 path")),
        "the document carries its own output file at {}",
        physical.display()
    );
}
