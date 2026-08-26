//! Running a canonical tool as another user.
//!
//! No test here runs real `sudo`. Whether `sudo` is installed is part of the host and
//! may change between the Debian and Alpine environments this project cares about.
//! What *is* stable is the argument vector this seam builds and the way it reports the
//! delegated run's result, so the executable path is faked where the host would
//! otherwise decide the answer.

mod support;

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::time::Duration;

use rastro::collectors::canonical_tool::{CanonicalTool, RunLimits, TargetUser, ToolAsUser};
use support::fs_tree::scratch_tree;

/// Present on Debian and on Alpine's busybox alike, so it stands in for both the
/// delegator and the tool.
const SHELL: &str = "sh";

fn shell() -> CanonicalTool {
    CanonicalTool::located(SHELL).expect("a POSIX shell is on every host rastro targets")
}

fn postgres() -> TargetUser {
    TargetUser::new("postgres").expect("a legal user name")
}

fn executable(tree: &str, program: &str, body: &str) -> CanonicalTool {
    let root = scratch_tree(&format!("tool-as-user-{tree}"), &[]);
    let path = root.join(program);
    fs::write(&path, format!("#!/bin/sh\n{body}\n")).expect("a writable script");
    let mut permissions = fs::metadata(&path).expect("metadata").permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&path, permissions).expect("an executable script");
    CanonicalTool::located_in(program, &[root.to_str().expect("utf-8 scratch path")])
        .expect("the script should be locatable")
}

fn passthrough_delegator(tree: &str) -> CanonicalTool {
    executable(tree, "sudo", "shift 3\nexec \"$@\"")
}

#[test]
fn new_accepts_an_ordinary_user_name() {
    // Act & Assert
    assert_eq!(postgres().as_str(), "postgres");
}

#[test]
fn new_refuses_a_name_that_sudo_would_read_as_an_option() {
    // Act & Assert: the name reaches sudo as an argument, and a leading dash would make
    // it a flag instead. Names come from host output, so this is reachable.
    assert!(TargetUser::new("-u").is_err());
    assert!(TargetUser::new("--reset-timestamp").is_err());
}

#[test]
fn new_refuses_a_name_that_sudo_would_read_as_a_numeric_uid() {
    // Act & Assert: sudo documents `-u` as taking a user name *or* a UID prefixed with
    // `#`, so `#0` delegates to root. The name is read from the host, so a passwd entry
    // shaped like this would turn the privilege drop into no drop at all.
    assert!(TargetUser::new("#0").is_err());
    assert!(TargetUser::new("#1000").is_err());
}

#[test]
fn new_refuses_a_name_no_account_could_have() {
    // Act & Assert
    assert!(TargetUser::new("").is_err());
    assert!(TargetUser::new("two words").is_err());
}

#[test]
fn located_fails_rather_than_reporting_a_service_absent() {
    // Act
    let refused = ToolAsUser::located("rastro-tool-that-does-not-exist", &postgres());

    // Assert: a missing client says nothing about the service. A cluster is running or
    // not regardless of whether this host has something to talk to it with, so the only
    // honest answer is that rastro could not look, and the reason has to name what was
    // missing or an operator cannot act on it.
    let reason = refused.expect_err("no host has that tool").to_string();
    assert!(
        reason.contains("rastro-tool-that-does-not-exist"),
        "got {reason:?}"
    );
    assert!(reason.contains("postgres"), "got {reason:?}");
}

#[test]
fn using_builds_the_argument_vector_sudo_receives() {
    // Arrange
    let delegated = ToolAsUser::using(shell(), shell(), postgres());

    // Act
    let arguments = delegated.arguments_for(&["--csv", "-c", "SELECT 1"]);

    // Assert: `-n` first, because a fingerprint run has no operator to answer a
    // password prompt, then the target user, then the tool's own absolute path.
    assert_eq!(arguments[0], "-n");
    assert_eq!(arguments[1], "-u");
    assert_eq!(arguments[2], "postgres");
    assert_eq!(arguments[3], shell().path().display().to_string());
}

#[test]
fn arguments_for_passes_the_tools_arguments_through_untouched() {
    // Arrange
    let delegated = ToolAsUser::using(shell(), shell(), postgres());

    // Act
    let arguments = delegated.arguments_for(&["-c", "SELECT 'a b', \"c\"; -- ;rm -rf /"]);

    // Assert: an argument vector, never a command line, so nothing in an argument needs
    // quoting and nothing in it can become a second command.
    assert_eq!(arguments.len(), 6);
    assert_eq!(arguments[4], "-c");
    assert_eq!(arguments[5], "SELECT 'a b', \"c\"; -- ;rm -rf /");
}

#[test]
fn program_and_user_name_what_would_run() {
    // Arrange
    let delegated = ToolAsUser::using(shell(), shell(), postgres());

    // Act & Assert: a failure message has to say which tool, as which user, or an
    // operator reading stderr cannot tell a missing binary from a refused sudo.
    assert_eq!(delegated.program(), SHELL);
    assert_eq!(delegated.user().as_str(), "postgres");
}

#[test]
fn located_succeeds_when_the_host_has_both_halves() {
    if CanonicalTool::located("sudo").is_none() {
        return;
    }

    let delegated = ToolAsUser::located("sh", &postgres()).expect("the host has both tools");
    assert_eq!(delegated.program(), "sh");
    assert_eq!(delegated.user().as_str(), "postgres");
}

#[test]
fn within_changes_the_bounds_of_the_delegated_run() {
    let delegated = ToolAsUser::using(
        passthrough_delegator("within"),
        executable("within-tool", "fake-tool", "sleep 1\nprintf too-late"),
        postgres(),
    )
    .within(RunLimits::new(Duration::from_millis(50), 1024));

    let failure = delegated
        .run(&[])
        .expect_err("a short bound must still apply through the delegator");

    assert!(failure.to_string().contains("fake-tool as postgres"));
}

#[test]
fn run_returns_the_delegated_tools_stdout() {
    let delegated = ToolAsUser::using(
        passthrough_delegator("stdout"),
        executable("stdout-tool", "fake-tool", "printf delegated-output"),
        postgres(),
    );

    let output = delegated
        .run(&["ignored by fake tool"])
        .expect("the fake tool succeeds");
    assert_eq!(output, "delegated-output");
}

#[test]
fn run_names_the_tool_and_user_when_the_delegator_fails() {
    let delegated = ToolAsUser::using(
        executable("failure-delegator", "sudo", "printf denied >&2\nexit 1"),
        executable("failure-tool", "fake-tool", "printf never-runs"),
        postgres(),
    );

    let failure = delegated
        .run(&[])
        .expect_err("a failing delegator must be reported");

    assert!(failure.to_string().contains("fake-tool as postgres"));
    assert!(failure.to_string().contains("denied"));
}
