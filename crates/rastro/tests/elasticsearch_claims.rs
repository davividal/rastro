//! The tree the walk is asked to step back from: a node's data directory.
//!
//! Sealed, for the reason the PostgreSQL and RabbitMQ stores are: measured on 7.17, a node with
//! no request at all moved every index's translog checkpoint and retention-lease file within
//! ninety seconds, so a walk of it cannot be byte-identical across two runs of an unchanged box.

use std::fs;
use std::os::unix::fs::symlink;

use rastro::collectors::elasticsearch::{ElasticsearchCollector, HttpClient};
use rastro_collector::{ClaimedReading, Collector};

mod support;

use support::es_node::{FakeNode, PID};

fn claimed_trees(proc: &std::path::Path) -> Vec<String> {
    ElasticsearchCollector::reading(proc, false, HttpClient::new())
        .filesystem_claims()
        .iter()
        .inspect(|claim| assert_eq!(claim.reading(), ClaimedReading::Sealed))
        .map(|claim| claim.tree().as_str().to_owned())
        .collect()
}

#[test]
fn filesystem_claims_seal_the_data_directory_the_settings_name() {
    // Arrange: a package install sets it in `elasticsearch.yml`.
    let node = FakeNode::serving(&[]);
    let proc = node.proc_with(
        "elasticsearch-claims-configured",
        "",
        Some("path.data: /var/lib/elasticsearch\n"),
    );

    // Act & Assert
    assert_eq!(claimed_trees(&proc), ["/var/lib/elasticsearch"]);
}

#[test]
fn filesystem_claims_seal_every_directory_of_a_multi_path_node() {
    // Arrange: deprecated since 7.13 and still accepted.
    let node = FakeNode::serving(&[]);
    let proc = node.proc_with(
        "elasticsearch-claims-several",
        "",
        Some("path.data: [/srv/es-a, /srv/es-b]\n"),
    );

    // Act & Assert
    assert_eq!(claimed_trees(&proc), ["/srv/es-a", "/srv/es-b"]);
}

#[test]
fn filesystem_claims_seal_the_default_under_the_home_where_nothing_names_one() {
    // Arrange: a tarball install, whose data directory is `data` under its home.
    let node = FakeNode::serving(&[]);
    let proc = node.proc_with("elasticsearch-claims-default", "", None);

    // Act & Assert
    assert_eq!(claimed_trees(&proc), ["/usr/share/elasticsearch/data"]);
}

#[test]
fn filesystem_claims_resolve_a_relative_path_against_the_home() {
    // Arrange
    let node = FakeNode::serving(&[]);
    let proc = node.proc_with("elasticsearch-claims-relative", "path.data=storage\0", None);

    // Act & Assert
    assert_eq!(claimed_trees(&proc), ["/usr/share/elasticsearch/storage"]);
}

#[test]
fn filesystem_claims_leave_a_node_in_its_own_mount_namespace_alone() {
    // Arrange: a node in a container, whose data directory is a path in its own image, not on
    // the host the walk reads.
    let node = FakeNode::serving(&[]);
    let proc = node.proc_with(
        "elasticsearch-claims-container",
        "",
        Some("path.data: /usr/share/elasticsearch/data\n"),
    );
    let link = proc.join(PID).join("ns/mnt");
    fs::remove_file(&link).expect("the fixture's link");
    symlink("mnt:[4026532999]", &link).expect("a writable fixture");

    // Act & Assert
    assert!(claimed_trees(&proc).is_empty());
}

#[test]
fn filesystem_claims_make_no_claim_where_the_settings_cannot_be_read() {
    // Arrange: the walk's own default is the safe direction to be wrong in.
    let node = FakeNode::serving(&[]);
    let proc = node.proc_with(
        "elasticsearch-claims-unread",
        "",
        Some("path.data: ${ES_DATA}\n"),
    );

    // Act & Assert
    assert!(claimed_trees(&proc).is_empty());
}
