#[path = "../../../tests/support/purity.rs"]
mod purity_support;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

fn scratch_tree(name: &str) -> PathBuf {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let ordinal = COUNTER.fetch_add(1, Ordering::Relaxed);
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("{name}-{ordinal}"));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("a writable scratch directory");
    root
}

#[test]
fn sources_in_finds_only_rust_files() {
    // Arrange
    let root = scratch_tree("purity-support-sources");
    fs::create_dir_all(root.join("nested")).expect("a writable nested directory");
    fs::write(root.join("lib.rs"), "pub fn root() {}\n").expect("a writable source file");
    fs::write(root.join("nested/mod.rs"), "pub fn child() {}\n").expect("a writable source file");
    fs::write(root.join("nested/notes.txt"), "not rust\n").expect("a writable text file");

    // Act
    let mut found = purity_support::sources_in(&root)
        .into_iter()
        .map(|path| path.strip_prefix(&root).expect("under root").to_owned())
        .collect::<Vec<_>>();
    found.sort();

    // Assert
    assert_eq!(
        found,
        vec![PathBuf::from("lib.rs"), PathBuf::from("nested/mod.rs")]
    );
}

#[test]
fn code_without_line_comments_keeps_code_and_drops_comment_lines() {
    // Arrange
    let source = "// top-level comment\nlet keep = 1;\n    // indented comment\nlet also_keep = 2;\nlet slash = \"// text inside code\";\n";

    // Act
    let stripped = purity_support::code_without_line_comments(source);

    // Assert
    assert_eq!(
        stripped,
        "let keep = 1;\nlet also_keep = 2;\nlet slash = \"// text inside code\";"
    );
}
