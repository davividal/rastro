//! Running a canonical tool as another user.
//!
//! No test here runs sudo. Whether sudo is installed, and whether it would grant the
//! request, are properties of the host rather than of this seam, and a test that needed
//! either would pass or fail for reasons that have nothing to do with the code. What is
//! testable everywhere is what the seam *builds*, so that is what these check.

use rastro::collectors::canonical_tool::{CanonicalTool, TargetUser, ToolAsUser};

/// Present on Debian and on Alpine's busybox alike, so it stands in for both the
/// delegator and the tool.
const SHELL: &str = "sh";

fn shell() -> CanonicalTool {
    CanonicalTool::located(SHELL).expect("a POSIX shell is on every host rastro targets")
}

fn postgres() -> TargetUser {
    TargetUser::new("postgres").expect("a legal user name")
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
