//! The node's own effective settings: what an operator changes in its file or its JVM options,
//! which the cluster-wide surfaces do not hold. Found as a gap by the second domain review.

use rastro::collectors::elasticsearch::{ElasticsearchCollector, HttpClient};
use rastro_collector::Collector;
use rastro_fingerprint::{Completeness, Observation, Sensitivity, Volatility};

mod support;

use support::es_node::{FakeNode, ROOT};
use support::observation::{field, integer, items_of, text};

const NODE_LOCAL: &str = "/_nodes/_local?flat_settings=true&filter_path=nodes.*.settings,nodes.*.roles,nodes.*.attributes,nodes.*.jvm.input_arguments,nodes.*.jvm.mem.heap_max_in_bytes";

/// The shape 9.2.0 answers in, trimmed: measured by the second domain review.
const NODE_LOCAL_ANSWER: &str = r#"{"nodes":{"cyvbyyu2TvCNmDQYsxjtkQ":{
  "settings":{"cluster.name":"docker-cluster","network.host":"0.0.0.0","node.attr.zone":"eu-1","path.repo":"/tmp/snapshots","xpack.security.enabled":"false"},
  "roles":["ingest","master","data"],
  "attributes":{"zone":"eu-1","ml.machine_memory":"7706554368"},
  "jvm":{"input_arguments":["-Xms512m","-Djava.io.tmpdir=/tmp/elasticsearch-4567307896532071893","-XX:+UseG1GC"],"mem":{"heap_max_in_bytes":536870912}}
}}}"#;

fn node_local(routes: &[(&str, &str)], name: &str) -> Observation {
    let node = FakeNode::serving(routes);
    let proc = node.proc(name);
    let facet = ElasticsearchCollector::reading(&proc, false, HttpClient::new())
        .collect()
        .expect("a facet");
    field(&items_of(&field(&facet, "nodes"))[0], "node_local")
}

#[test]
fn collect_reports_the_nodes_roles_sorted_and_its_attributes() {
    // Act
    let local = node_local(
        &[("/", ROOT), (NODE_LOCAL, NODE_LOCAL_ANSWER)],
        "elasticsearch-node-local-roles",
    );

    // Assert: roles are a set; the node lists them in an order of its own.
    let roles: Vec<String> = items_of(&field(&local, "roles")).iter().map(text).collect();
    assert_eq!(roles, ["data", "ingest", "master"]);
    assert_eq!(text(&field(&field(&local, "attributes"), "zone")), "eu-1");
    assert_eq!(integer(&field(&local, "heap_max_bytes")), 536_870_912);
}

#[test]
fn collect_withholds_the_nodes_settings_whole() {
    // Act
    let local = node_local(
        &[("/", ROOT), (NODE_LOCAL, NODE_LOCAL_ANSWER)],
        "elasticsearch-node-local-settings",
    );

    // Assert: a plugin's setting that is not declared filtered would show here, so no one key is
    // trusted by name, the rule the snapshot repositories follow.
    assert_eq!(
        field(&local, "settings").sensitivity(),
        Sensitivity::Sensitive
    );
}

#[test]
fn collect_marks_the_per_start_temporary_directory_volatile() {
    // Act: measured by the second domain review, `-Djava.io.tmpdir=/tmp/elasticsearch-<random>` is
    // new on every start.
    let local = node_local(
        &[("/", ROOT), (NODE_LOCAL, NODE_LOCAL_ANSWER)],
        "elasticsearch-node-local-jvm",
    );

    // Assert
    let arguments = items_of(&field(&local, "jvm_arguments"));
    assert_eq!(text(&arguments[0]), "-Xms512m");
    assert_eq!(arguments[0].volatility(), Volatility::Stable);
    assert_eq!(arguments[1].volatility(), Volatility::Volatile);
}

#[test]
fn collect_reports_a_refused_node_local_read_on_that_surface_alone() {
    // Act
    let local = node_local(&[("/", ROOT)], "elasticsearch-node-local-refused");

    // Assert
    assert_eq!(local.completeness(), Completeness::Incomplete);
}
