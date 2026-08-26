#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};

pub fn scratch_tree(name: &str, directories: &[&str]) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("a writable scratch directory");

    for directory in directories {
        fs::create_dir_all(root.join(directory)).expect("a writable scratch directory");
    }

    root
}

pub fn write(root: &Path, relative: &str, contents: &str) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().expect("a parent")).expect("a writable tree");
    fs::write(path, contents).expect("a writable file");
}
