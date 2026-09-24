//! Whether this run can read what only root may read, said before it starts.

use rastro::privilege;

/// `/proc/self/status` for a process of uid 1000, trimmed to the lines around the one read.
const UNPRIVILEGED: &str = "\
Name:\trastro
Umask:\t0022
State:\tR (running)
Uid:\t1000\t1000\t1000\t1000
Gid:\t1000\t1000\t1000\t1000
";

#[test]
fn the_effective_user_id_is_the_second_column() {
    // Arrange: real, effective, saved and filesystem, in that order. `sudo` from a setuid
    // wrapper leaves the real id as the caller's and the effective one as root's, and access
    // checks use the effective one.
    let status = "Name:\trastro\nUid:\t1000\t0\t0\t0\n";

    // Act & Assert
    assert_eq!(privilege::effective_user_id_in(status), Some(0));
}

#[test]
fn a_status_without_a_uid_line_gives_no_answer() {
    // Act & Assert
    assert_eq!(privilege::effective_user_id_in("Name:\trastro\n"), None);
}

#[test]
fn root_hears_nothing() {
    // Act & Assert
    assert_eq!(privilege::concern(Some(0)), None);
}

#[test]
fn an_unprivileged_run_is_told_before_it_starts() {
    // Arrange
    let effective = privilege::effective_user_id_in(UNPRIVILEGED);

    // Act
    let concern = privilege::concern(effective).expect("uid 1000 is not root");

    // Assert: which id, and what it costs. The facets are not named, because which ones fail
    // depends on the box; the summary after the run names them.
    assert!(
        concern.starts_with("running as uid 1000, not root"),
        "got {concern}"
    );
    assert!(concern.contains("recorded as an error"), "got {concern}");
}

#[test]
fn a_run_that_cannot_tell_its_own_uid_says_nothing() {
    // Act & Assert: rather than guess, which would warn root about nothing.
    assert_eq!(privilege::concern(None), None);
}
