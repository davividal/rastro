//! Cluster settings: what an operator set through the API, which no file on the box records.

use rastro::collectors::elasticsearch::{ElasticsearchCollector, HttpClient};
use rastro_collector::Collector;
use rastro_fingerprint::Completeness;

mod support;

use support::es_node::{FakeNode, ROOT};
use support::observation::{field, items_of, keys_of, text};

const CLUSTER_SETTINGS: &str = "/_cluster/settings?flat_settings=true";

/// Measured on 8.15.3 after one persistent and one transient `PUT _cluster/settings`.
const SETTINGS: &str = r#"{"persistent":{"cluster.routing.allocation.enable":"all"},"transient":{"indices.recovery.max_bytes_per_sec":"40mb"}}"#;

fn node_reported(node: &FakeNode, name: &str) -> rastro_fingerprint::Observation {
    let proc = node.proc(name);
    let facet = ElasticsearchCollector::reading(&proc, false, HttpClient::new())
        .collect()
        .expect("a facet");
    items_of(&field(&facet, "nodes")).remove(0)
}

#[test]
fn collect_keeps_persistent_and_transient_settings_apart() {
    // Arrange: they differ in whether a full cluster restart keeps them.
    let node = FakeNode::serving(&[("/", ROOT), (CLUSTER_SETTINGS, SETTINGS)]);

    // Act
    let reported = node_reported(&node, "elasticsearch-cluster-settings");

    // Assert
    let settings = field(&reported, "cluster_settings");
    assert_eq!(
        text(&field(
            &field(&settings, "persistent"),
            "cluster.routing.allocation.enable"
        )),
        "all"
    );
    assert_eq!(
        text(&field(
            &field(&settings, "transient"),
            "indices.recovery.max_bytes_per_sec"
        )),
        "40mb"
    );
}

#[test]
fn collect_reports_a_cluster_with_no_settings_as_two_empty_sets() {
    // Arrange: the field host's case.
    let node = FakeNode::serving(&[
        ("/", ROOT),
        (CLUSTER_SETTINGS, r#"{"persistent":{},"transient":{}}"#),
    ]);

    // Act
    let reported = node_reported(&node, "elasticsearch-cluster-settings-empty");

    // Assert
    let settings = field(&reported, "cluster_settings");
    assert!(keys_of(&field(&settings, "persistent")).is_empty());
    assert!(keys_of(&field(&settings, "transient")).is_empty());
}

#[test]
fn collect_keeps_a_non_integer_number_as_its_own_spelling() {
    // Arrange: the format carries no floating point, and rounding would report a setting the
    // cluster does not have.
    let node = FakeNode::serving(&[
        ("/", ROOT),
        (
            CLUSTER_SETTINGS,
            r#"{"persistent":{"cluster.routing.allocation.disk.watermark.low":0.85},"transient":{}}"#,
        ),
    ]);

    // Act
    let reported = node_reported(&node, "elasticsearch-cluster-settings-real");

    // Assert
    let persistent = field(&field(&reported, "cluster_settings"), "persistent");
    assert_eq!(
        text(&field(
            &persistent,
            "cluster.routing.allocation.disk.watermark.low"
        )),
        "0.85"
    );
}

#[test]
fn collect_reports_a_refused_surface_on_the_surface_and_keeps_the_rest() {
    // Arrange: a node answering `/` and refusing the settings read.
    let node = FakeNode::serving(&[("/", ROOT)]);

    // Act
    let reported = node_reported(&node, "elasticsearch-cluster-settings-refused");

    // Assert: who the node is was read, and stays.
    let settings = field(&reported, "cluster_settings");
    assert!(text(&field(&settings, "error")).contains("404"));
    assert_eq!(settings.completeness(), Completeness::Incomplete);
    assert_eq!(text(&field(&reported, "node_name")), "search-1");
}

#[test]
fn collect_keeps_a_null_setting_as_null() {
    // Arrange: `null` is how the API shows a setting that was reset rather than removed.
    let node = FakeNode::serving(&[
        ("/", ROOT),
        (
            CLUSTER_SETTINGS,
            r#"{"persistent":{"cluster.routing.allocation.enable":null},"transient":{}}"#,
        ),
    ]);

    // Act
    let reported = node_reported(&node, "elasticsearch-cluster-settings-null");

    // Assert
    let persistent = field(&field(&reported, "cluster_settings"), "persistent");
    assert!(support::observation::is_null(&field(
        &persistent,
        "cluster.routing.allocation.enable"
    )));
}
