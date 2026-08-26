mod support;

use std::fs;

use rastro_fingerprint::Observation;
use support::fs_tree::{scratch_tree, write};
use support::observation::{field, items_of, keys_of};

#[test]
fn field_reads_the_named_entry_from_an_object() {
    // Arrange
    let observation = Observation::object([
        ("alpha", Observation::text("first")),
        ("beta", Observation::text("second")),
    ]);

    // Act
    let found = field(&observation, "beta");

    // Assert
    assert_eq!(found, Observation::text("second"));
}

#[test]
fn items_of_reads_a_list_observation() {
    // Arrange
    let observation = Observation::list([Observation::text("a"), Observation::text("b")]);

    // Act
    let items = items_of(&observation);

    // Assert
    assert_eq!(items, vec![Observation::text("a"), Observation::text("b")]);
}

#[test]
fn keys_of_preserves_the_objects_sorted_keys() {
    // Arrange
    let observation = Observation::object([
        ("beta", Observation::text("second")),
        ("alpha", Observation::text("first")),
    ]);

    // Act
    let keys = keys_of(&observation);

    // Assert
    assert_eq!(keys, vec!["alpha".to_owned(), "beta".to_owned()]);
}

#[test]
fn scratch_tree_recreates_an_empty_root_with_its_subdirectories() {
    // Arrange
    let root = scratch_tree("support-recreated-root", &["nested"]);
    fs::write(root.join("stale.txt"), "left behind").expect("a writable stale file");

    // Act
    let recreated = scratch_tree("support-recreated-root", &["nested"]);

    // Assert
    assert_eq!(recreated, root);
    assert!(!recreated.join("stale.txt").exists());
    assert!(recreated.join("nested").is_dir());
}

#[test]
fn write_creates_missing_parent_directories() {
    // Arrange
    let root = scratch_tree("support-write", &[]);

    // Act
    write(&root, "deep/tree/file.txt", "hello");

    // Assert
    assert_eq!(
        fs::read_to_string(root.join("deep/tree/file.txt")).expect("a readable file"),
        "hello"
    );
}
