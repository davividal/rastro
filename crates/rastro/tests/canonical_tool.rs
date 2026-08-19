//! The guarantees the execution seam claims, exercised against real processes.
//!
//! Several tests run `sh` as their subject, because producing a specific behaviour
//! (hanging, flooding stdout, emitting invalid UTF-8, exiting non-zero) needs a
//! program that can be told to do it. That is `sh` being an ordinary tool under test,
//! not the seam using a shell: what the seam guarantees is that *rastro* never builds
//! a command line out of data, and it does not.

use std::time::Duration;

use rastro::collectors::canonical_tool::{CanonicalTool, RunLimits};

/// Present on Debian and on Alpine's busybox alike.
const SHELL: &str = "sh";

fn shell() -> CanonicalTool {
    CanonicalTool::located(SHELL).expect("a POSIX shell is on every host rastro targets")
}

/// Short bounds so the hardening can be proven in milliseconds rather than in the
/// thirty seconds a real run is allowed.
fn shell_within(time: Duration, output: usize) -> CanonicalTool {
    shell().within(RunLimits::new(time, output))
}

#[test]
fn located_finds_a_tool_in_a_system_directory() {
    // Act
    let tool = shell();

    // Assert: the resolved path is absolute, which is the guarantee that what was detected is
    // what will run, and it comes from a system directory rather than from `PATH`.
    assert!(tool.path().is_absolute(), "got {:?}", tool.path());
    assert!(
        tool.path().starts_with("/bin") || tool.path().starts_with("/usr/bin"),
        "got {:?}",
        tool.path()
    );
    assert_eq!(tool.program(), SHELL);
}

#[test]
fn located_reports_a_tool_that_is_not_installed() {
    // Act
    let tool = CanonicalTool::located("rastro-tool-that-does-not-exist");

    // Assert: an ordinary answer, not a failure. A box without dpkg-query is a box
    // that does not use dpkg, which is state.
    assert!(tool.is_none());
}

#[test]
fn run_returns_what_the_tool_wrote_to_stdout() {
    // Act
    let output = shell()
        .run(&["-c", "printf 'one\\ntwo\\n'"])
        .expect("this shell command succeeds");

    // Assert
    assert_eq!(output, "one\ntwo\n");
}

#[test]
fn run_clears_the_environment_apart_from_the_locale() {
    // Arrange: `env` rather than `sh -c env`, because a shell sets `PWD` itself on
    // startup and that would read as an inherited variable when it is not one.
    let environment =
        CanonicalTool::located("env").expect("coreutils and busybox both provide env");

    // Act
    let output = environment.run(&[]).expect("env succeeds");

    // Assert: an inherited environment is an input nobody audited, and a localised box
    // would render different bytes for the same state.
    let mut variables: Vec<&str> = output.lines().filter(|line| !line.is_empty()).collect();
    variables.sort_unstable();
    assert_eq!(variables, ["LC_ALL=C"], "got {output:?}");
}

#[test]
fn run_gives_a_tool_that_reads_stdin_an_immediate_end_of_input() {
    // Act: `cat` with no redirection would wait for a terminal that is not there. If
    // this test hangs, the guarantee is broken.
    let output = shell()
        .run(&["-c", "cat"])
        .expect("cat succeeds on empty input");

    // Assert
    assert_eq!(output, "");
}

#[test]
fn run_reports_a_tool_that_exits_unsuccessfully() {
    // Act
    let failure = shell()
        .run(&["-c", "echo 'it went wrong' >&2; exit 3"])
        .expect_err("a non-zero exit is a failure");

    // Assert: partial output is never treated as an answer, and the message carries
    // enough for an operator to act on.
    let message = failure.to_string();
    assert!(message.contains("it went wrong"), "got {message:?}");
    assert!(message.contains(SHELL), "got {message:?}");
}

#[test]
fn run_kills_a_tool_that_does_not_finish_in_time() {
    // Arrange
    let tool = shell_within(Duration::from_millis(200), 1024);

    // Act
    let failure = tool
        .run(&["-c", "sleep 30"])
        .expect_err("a hung tool is a failure");

    // Assert: a wedged tool must not be able to hang a fingerprint run on a
    // production box.
    assert!(
        failure.to_string().contains("did not finish"),
        "got {failure}"
    );
}

#[test]
fn run_refuses_a_tool_that_writes_more_than_its_bound() {
    // Arrange
    let tool = shell_within(Duration::from_secs(10), 1024);

    // Act
    let failure = tool
        .run(&["-c", "head -c 100000 /dev/zero"])
        .expect_err("output past the bound is a failure");

    // Assert: `subprocess` enforces its size limit by stopping the read and buffering
    // the rest, so taking it at face value would have produced a quietly truncated
    // answer. That is the configsnap bug this project exists in reaction to, so it has
    // to be loud.
    assert!(failure.to_string().contains("truncated"), "got {failure}");
}

#[test]
fn run_accepts_output_exactly_at_its_bound() {
    // Arrange: the bound is read one byte past precisely so that output *at* the bound
    // is not mistaken for output beyond it.
    let tool = shell_within(Duration::from_secs(10), 8);

    // Act
    let output = tool
        .run(&["-c", "printf '12345678'"])
        .expect("output at the bound is not over it");

    // Assert
    assert_eq!(output, "12345678");
}

#[test]
fn run_refuses_output_that_is_not_valid_utf8() {
    // Act
    let failure = shell()
        .run(&["-c", "printf '\\377\\376'"])
        .expect_err("invalid UTF-8 is a failure");

    // Assert: replacing the bytes with U+FFFD would put text into a fingerprint that
    // was never on the box.
    assert!(failure.to_string().contains("UTF-8"), "got {failure}");
}

#[test]
fn run_kills_what_the_tool_started_too() {
    // Arrange: a tool that backgrounds a helper and then hangs. Killing only the direct
    // child leaves the helper running on the box, unbounded, after rastro has already
    // reported a failure and moved on.
    let marker = std::env::temp_dir().join("rastro-descendant-marker");
    let _ = std::fs::remove_file(&marker);
    let script = format!("(sleep 2; echo alive > {}) & sleep 30", marker.display());
    let tool = shell_within(Duration::from_millis(300), 1024);

    // Act
    tool.run(&["-c", &script])
        .expect_err("a hung tool is a failure");

    // Assert: wait past the helper's own delay, then check it never got to write.
    std::thread::sleep(Duration::from_secs(4));
    let survived = marker.exists();
    let _ = std::fs::remove_file(&marker);
    assert!(!survived, "a descendant outlived the kill and kept running");
}

#[test]
fn located_finds_a_tool_in_a_named_directory() {
    // Arrange: Debian and Alpine both keep a shell at `/bin/sh`.
    let tool = CanonicalTool::located_in(SHELL, &["/bin"]).expect("/bin/sh exists");

    // Assert
    assert_eq!(tool.path(), std::path::Path::new("/bin/sh"));
}

#[test]
fn located_does_not_search_the_path() {
    // Arrange: a shell is certainly on `PATH`, and that is deliberately not enough. rastro runs
    // as root, so the first directory on root's inherited `PATH` that is not root-owned would
    // otherwise decide which binary runs with full privilege.
    let tool = CanonicalTool::located_in(SHELL, &["/nowhere/at/all"]);

    // Assert: reported as not found, which is a narrower claim than a lie and a far better
    // failure than executing the wrong thing.
    assert!(tool.is_none());
}

#[test]
fn run_returns_when_a_tool_closes_its_streams_and_keeps_going() {
    // Arrange: the bounded read is satisfied the moment both pipes reach EOF, so a tool that
    // answers, closes its streams and carries on would leave an unbounded wait blocking
    // forever. If this test hangs, the seam's central claim is false.
    let tool = shell_within(Duration::from_millis(300), 4096);

    // Act
    let failure = tool
        .run(&["-c", "printf hello; exec 1>&- 2>&-; sleep 60"])
        .expect_err("a tool that outlasts its bound is a failure");

    // Assert
    assert!(
        failure.to_string().contains("did not finish"),
        "got {failure}"
    );
}

#[test]
fn run_kills_a_descendant_of_a_tool_that_already_exited() {
    // Arrange: the case the group kill exists for. The tool backgrounds a helper and exits at
    // once, so the direct child is gone by kill time while the helper holds the pipes open.
    let marker = std::env::temp_dir().join("rastro-orphan-marker");
    let _ = std::fs::remove_file(&marker);
    let script = format!("(sleep 2; echo alive > {}) & exit 0", marker.display());
    let tool = shell_within(Duration::from_millis(400), 4096);

    // Act
    let _ = tool.run(&["-c", &script]);

    // Assert
    std::thread::sleep(Duration::from_secs(4));
    let survived = marker.exists();
    let _ = std::fs::remove_file(&marker);
    assert!(!survived, "a descendant of an exited tool kept running");
}

#[test]
fn a_timeout_reports_the_bound_it_actually_had() {
    // Arrange: whole seconds truncate a sub-second bound to "0 seconds", and this string
    // reaches the document's error field, so it is a wrong number in a stored artefact.
    let tool = shell_within(Duration::from_millis(250), 4096);

    // Act
    let failure = tool
        .run(&["-c", "sleep 30"])
        .expect_err("a hung tool is a failure");

    // Assert
    let message = failure.to_string();
    assert!(message.contains("250ms"), "got {message:?}");
    assert!(!message.contains("0 seconds"), "got {message:?}");
}

#[test]
fn a_failing_tools_stderr_keeps_the_end_not_the_beginning() {
    // Arrange: more stderr than the quoted bound, marked at both ends. A tool writes its
    // warnings first and its fatal line last, so the end is the half worth keeping.
    let flood = "{ printf FIRST; head -c 600 /dev/zero | tr '\\0' a; printf LAST; } >&2; exit 3";

    // Act
    let failure = shell()
        .run(&["-c", flood])
        .expect_err("a non-zero exit is a failure");

    // Assert
    let message = failure.to_string();
    assert!(message.ends_with("LAST"), "got {message}");
    assert!(
        !message.contains("FIRST"),
        "the beginning should be dropped"
    );
}

#[test]
fn a_multibyte_character_on_the_cut_is_not_split() {
    // Arrange: 513 bytes, with a two-byte character at the very front, so the cut at byte one
    // falls inside it. Slicing on the byte offset would substitute U+FFFD, which is the
    // operation this module refuses for stdout, and this text reaches the document too.
    let flood =
        "{ printf '\\303\\251'; head -c 507 /dev/zero | tr '\\0' a; printf LAST; } >&2; exit 3";

    // Act
    let failure = shell()
        .run(&["-c", flood])
        .expect_err("a non-zero exit is a failure");

    // Assert
    let message = failure.to_string();
    assert!(
        !message.contains('\u{fffd}'),
        "a replacement character reached the message: {message}"
    );
    assert!(message.ends_with("LAST"), "got {message}");
}

#[test]
fn a_short_stderr_is_quoted_whole() {
    // Arrange: the bound compared start indices against the limit, which dropped the last
    // character of every message and all of a one-character one.
    let failure = shell()
        .run(&["-c", "printf 'permission denied' >&2; exit 1"])
        .expect_err("a non-zero exit is a failure");

    // Assert
    let message = failure.to_string();
    assert!(
        message.ends_with("permission denied"),
        "the whole message should survive: {message}"
    );
}

#[test]
fn a_one_character_stderr_is_not_swallowed() {
    // Act
    let failure = shell()
        .run(&["-c", "printf x >&2; exit 1"])
        .expect_err("a non-zero exit is a failure");

    // Assert
    assert!(failure.to_string().ends_with('x'), "got {failure}");
}
