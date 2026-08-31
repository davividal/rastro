#![allow(dead_code)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

/// The modes a fixture creates, set rather than inherited.
///
/// A file created without saying so takes whatever the caller's umask allows: 0644 under the
/// usual 022 and 0664 under the 002 that `useradd` gives a fresh user on Debian. Tests here
/// assert modes, so leaving that to the ambient umask makes them pass or fail on who is
/// running them — which is a trap rather than a test.
///
/// **Nothing for others**, which is not about the tests: these are scratch files, and any mode
/// works as long as it is fixed. Granting the world a read bit it does not need would be a
/// finding in a security scan, and being right about it in a fixture costs nothing.
const FILE_MODE: u32 = 0o640;
const DIRECTORY_MODE: u32 = 0o750;

pub fn scratch_tree(name: &str, directories: &[&str]) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("a writable scratch directory");

    fs::set_permissions(&root, fs::Permissions::from_mode(DIRECTORY_MODE))
        .expect("a writable scratch directory");

    for directory in directories {
        let path = root.join(directory);
        fs::create_dir_all(&path).expect("a writable scratch directory");

        // Every level, because `create_dir_all` made the intermediate ones too.
        for level in path.ancestors().take_while(|level| *level != root) {
            fs::set_permissions(level, fs::Permissions::from_mode(DIRECTORY_MODE))
                .expect("a writable scratch directory");
        }
    }

    root
}

pub fn write(root: &Path, relative: &str, contents: &str) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().expect("a parent")).expect("a writable tree");
    fs::write(&path, contents).expect("a writable file");
    fs::set_permissions(&path, fs::Permissions::from_mode(FILE_MODE)).expect("a writable file");
}
