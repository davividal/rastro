//! Which network namespace a node is dialled in, decided from a `/proc` a test built.
//!
//! Joining another namespace needs `CAP_SYS_ADMIN`, which the container suite does not grant,
//! so the join itself is proved where it can be: `elasticsearch_conformance`, against a node in
//! docker with no published port. What is proved here is the decision around it, which is where
//! a mistake would send a request into the wrong namespace or none.

use std::os::unix::fs::symlink;

use rastro::collectors::elasticsearch::NodeNamespace;

mod support;

use support::fs_tree::scratch_tree;

#[test]
fn run_does_the_work_in_place_for_a_node_in_rastros_own_namespace() {
    // Arrange: two links naming one namespace inode, which is a node on the host.
    let proc = scratch_tree("elasticsearch-namespace-same", &["self/ns", "600/ns"]);
    symlink("net:[4026531840]", proc.join("self/ns/net")).expect("a fixture");
    symlink("net:[4026531840]", proc.join("600/ns/net")).expect("a fixture");
    let namespace = NodeNamespace::of_in(&proc, 600).expect("a readable namespace");

    // Act
    let answer = namespace.run(|| 42).expect("work done in place");

    // Assert
    assert!(namespace.is_ours());
    assert_eq!(answer, 42);
}

#[test]
fn of_in_tells_a_node_in_another_namespace_apart() {
    // Arrange: a node in a container.
    let proc = scratch_tree("elasticsearch-namespace-other", &["self/ns", "600/ns"]);
    symlink("net:[4026531840]", proc.join("self/ns/net")).expect("a fixture");
    symlink("net:[4026532512]", proc.join("600/ns/net")).expect("a fixture");

    // Act
    let namespace = NodeNamespace::of_in(&proc, 600).expect("a readable namespace");

    // Assert
    assert!(!namespace.is_ours());
}

#[test]
fn run_refuses_a_namespace_it_cannot_open_and_does_not_do_the_work() {
    // Arrange: the link reads, and there is nothing behind it to open, as for a node that exited
    // between the two reads.
    let proc = scratch_tree("elasticsearch-namespace-unopenable", &["self/ns", "600/ns"]);
    symlink("net:[4026531840]", proc.join("self/ns/net")).expect("a fixture");
    symlink("net:[4026532512]", proc.join("600/ns/net")).expect("a fixture");
    let namespace = NodeNamespace::of_in(&proc, 600).expect("a readable namespace");
    let mut worked = false;

    // Act
    let unread = namespace
        .run(|| worked = true)
        .expect_err("an unopenable namespace");

    // Assert: work done anyway would be a request sent in rastro's own namespace instead.
    assert!(!worked);
    assert!(unread.reason().contains("namespace"), "{}", unread.reason());
}

#[test]
fn of_in_refuses_a_namespace_link_it_cannot_read() {
    // Arrange: an unprivileged run looking at another user's process cannot read the link.
    let proc = scratch_tree("elasticsearch-namespace-refused", &["self/ns", "600/ns"]);
    symlink("net:[4026531840]", proc.join("self/ns/net")).expect("a fixture");

    // Act
    let unread = NodeNamespace::of_in(&proc, 600).expect_err("an unreadable link");

    // Assert
    assert!(unread.reason().contains("ns/net"), "{}", unread.reason());
}

#[test]
fn of_in_refuses_where_rastros_own_namespace_cannot_be_read() {
    // Arrange: without rastro's own link there is nothing to compare the node's with.
    let proc = scratch_tree(
        "elasticsearch-namespace-self-refused",
        &["self/ns", "600/ns"],
    );
    symlink("net:[4026532512]", proc.join("600/ns/net")).expect("a fixture");

    // Act
    let unread = NodeNamespace::of_in(&proc, 600).expect_err("no own link");

    // Assert
    assert!(
        unread.reason().contains("rastro's own"),
        "{}",
        unread.reason()
    );
}

#[test]
fn run_does_not_do_the_work_where_the_join_fails() {
    // Arrange: a link to something that opens and is not a namespace, which the kernel refuses
    // to join whoever asks, so this holds for root and for an unprivileged run alike.
    let proc = scratch_tree(
        "elasticsearch-namespace-join-refused",
        &["self/ns", "600/ns"],
    );
    let not_a_namespace = proc.join("not-a-namespace");
    std::fs::write(&not_a_namespace, "").expect("a fixture");
    symlink("net:[4026531840]", proc.join("self/ns/net")).expect("a fixture");
    symlink(&not_a_namespace, proc.join("600/ns/net")).expect("a fixture");
    let namespace = NodeNamespace::of_in(&proc, 600).expect("a readable namespace");
    let mut worked = false;

    // Act
    let unread = namespace.run(|| worked = true).expect_err("a refused join");

    // Assert: work done anyway would be a request sent in rastro's own namespace instead.
    assert!(!namespace.is_ours());
    assert!(!worked);
    assert!(unread.reason().contains("namespace"), "{}", unread.reason());
}
