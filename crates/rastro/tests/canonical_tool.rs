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
fn located_prefers_a_named_directory_over_the_path() {
    // Arrange: rastro runs as root, so leaving the choice to a `PATH` search invites
    // shadowing. Debian and Alpine both keep a shell at `/bin/sh`.
    let tool = CanonicalTool::located_in(SHELL, &["/bin"]).expect("/bin/sh exists");

    // Assert
    assert_eq!(tool.path(), std::path::Path::new("/bin/sh"));
}

#[test]
fn located_falls_back_to_searching_the_path() {
    // Arrange: a tool installed somewhere unusual must still be found. Reporting it absent
    // because rastro did not guess its directory would be a lie about the host.
    let tool = CanonicalTool::located_in(SHELL, &["/nowhere/at/all"]).expect("sh is on PATH");

    // Assert
    assert!(tool.path().is_absolute(), "got {:?}", tool.path());
}

#[test]
fn located_reports_a_tool_that_is_in_neither() {
    // Act & Assert
    assert!(
        CanonicalTool::located_in("rastro-tool-that-does-not-exist", &["/nowhere/at/all"])
            .is_none()
    );
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
