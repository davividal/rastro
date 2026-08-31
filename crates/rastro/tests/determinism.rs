//! Two readings of an unchanged tree render the same bytes.
//!
//! **The filesystem facet's half of the flagship contract, over a tree this test owns.** The
//! end-to-end harness in `cli.rs` walks the whole host, which on a busy machine is not an
//! unchanged host at all: sibling tests write coverage files into the walked tree and a CI
//! runner writes its own logs while the suite runs. It therefore compares the envelope and the
//! other twenty facets, and the facet that dominates the document is compared here instead,
//! against a tree nothing else touches.
//!
//! This is also the half that was never really being tested. Until the walk learned to tolerate
//! one unreadable path, an unprivileged run — which CI is — failed the whole facet, so both
//! runs errored identically and the byte comparison passed on documents that contained no
//! filesystem data at all.

mod support;

use std::path::Path;

use rastro::collectors::filesystem::{ContentPolicy, FilesystemCollector, PolicyRule, WalkPolicy};
use rastro_collector::{AbsolutePath, Collector, WalkedTree, fingerprint_host};
use rastro_fingerprint::{View, json};
use support::fs_tree::{scratch_tree, write};

const CONTENTS: &str = "hello\n";

fn absolute(path: &Path) -> AbsolutePath {
    AbsolutePath::new(path.to_str().expect("a UTF-8 scratch path"), "walked root")
        .expect("a legal path")
}

/// The shipped table, plus one tree declared to churn, named under the scratch root.
fn policy(root: &Path, churning: &str) -> WalkPolicy {
    let mut rules = vec![PolicyRule::shipped(
        WalkedTree::new("/").expect("a legal tree"),
        ContentPolicy::MetadataOnly,
    )];
    rules.push(PolicyRule::shipped(
        WalkedTree::new(root.join(churning).to_str().expect("a UTF-8 scratch path"))
            .expect("a legal tree"),
        ContentPolicy::Churns,
    ));

    WalkPolicy::new(rules).expect("a legal table")
}

fn rendered(root: &Path, policy: WalkPolicy, view: View) -> String {
    let collectors: Vec<Box<dyn Collector>> = vec![Box::new(FilesystemCollector::walking(
        vec![absolute(root)],
        policy,
    ))];
    let fingerprint = fingerprint_host::run(&collectors).expect("one collector, one facet");

    json::to_canonical_json(&fingerprint, view)
}

#[test]
fn two_walks_of_an_unchanged_tree_render_identical_bytes() {
    // Arrange
    let root = scratch_tree("determinism-unchanged", &["etc/ssh", "opt", "var/log"]);
    write(&root, "etc/ssh/sshd_config", CONTENTS);
    write(&root, "opt/payload", CONTENTS);
    write(&root, "var/log/syslog", CONTENTS);
    std::os::unix::fs::symlink("/dev/null", root.join("etc/link")).expect("a symlink");

    // Act
    let first = rendered(&root, policy(&root, "var/log"), View::Diffable);
    let second = rendered(&root, policy(&root, "var/log"), View::Diffable);

    // Assert: bytes, because bytes are the contract. `diff(1)` is sufficient only if this holds.
    assert_eq!(first, second);
    assert!(first.contains("sshd_config"), "the tree should be in it");
}

#[test]
fn a_churning_trees_moving_stamps_do_not_move_the_document() {
    // Arrange: the strong version of the guarantee, and the one the whole-host harness could
    // never arrange — something volatile genuinely changes between the two readings, and the
    // diffable view has to be unchanged anyway.
    let root = scratch_tree("determinism-churn", &["var/log"]);
    write(&root, "var/log/syslog", CONTENTS);
    let first = rendered(&root, policy(&root, "var/log"), View::Diffable);

    // Act: rewrite the churning file, moving its size, mtime and ctime.
    write(&root, "var/log/syslog", "a later line that is longer\n");
    let second = rendered(&root, policy(&root, "var/log"), View::Diffable);

    // Assert
    assert_eq!(first, second);
}

#[test]
fn a_change_outside_a_churning_tree_does_move_the_document() {
    // Arrange: the counterweight. A guarantee that held because nothing was recorded would be
    // worthless, so the same fixture has to notice a real change.
    let root = scratch_tree("determinism-signal", &["etc", "var/log"]);
    write(&root, "etc/hosts", CONTENTS);
    write(&root, "var/log/syslog", CONTENTS);
    let before = rendered(&root, policy(&root, "var/log"), View::Diffable);

    // Act
    write(&root, "etc/hosts", "127.0.0.1 added\n");
    let after = rendered(&root, policy(&root, "var/log"), View::Diffable);

    // Assert
    assert_ne!(before, after);
}
