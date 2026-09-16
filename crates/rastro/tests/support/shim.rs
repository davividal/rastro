#![allow(dead_code)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use rastro::collectors::canonical_tool::CanonicalTool;

/// Writes an executable stand-in for a host tool and returns the seam that finds it.
///
/// **A script on `PATH` rather than a mock**, because the seam under test is the one that
/// runs a program: a fake that answered in Rust would skip the execution, the argument list
/// and the exit status, which is most of what these tests are about.
///
/// The mode is set rather than inherited for the reason `fs_tree` sets its own: a script the
/// caller's umask left unexecutable fails as "tool not found", which reads as a bug in the
/// detection rather than in the fixture.
pub fn executable(directory: &Path, program: &str, script: &str) -> CanonicalTool {
    let path = directory.join(program);
    fs::write(&path, script).expect("a writable script");

    let mut permissions = fs::metadata(&path).expect("metadata").permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(&path, permissions).expect("an executable script");

    CanonicalTool::located_in(
        program,
        &[directory.to_str().expect("a UTF-8 scratch path")],
    )
    .expect("the fake tool is locatable")
}
