//! The plugins a node runs, which change what every other surface can hold.

use rastro::collectors::elasticsearch::{ElasticsearchCollector, HttpClient};
use rastro_collector::Collector;
use rastro_fingerprint::{Completeness, Observation};

mod support;

use support::es_node::{FakeNode, ROOT};
use support::observation::{field, items_of, keys_of, text};

const PLUGINS: &str = "/_nodes/_local/plugins";

/// The shape 8.15.3 answers in, with two plugins installed and its 81 modules cut.
const PLUGINS_ANSWER: &str = r#"{"_nodes":{"total":1,"successful":1,"failed":0},"cluster_name":"docker-cluster","nodes":{"cyvbyyu2TvCNmDQYsxjtkQ":{"name":"search-1","version":"8.15.3","plugins":[
  {"name":"repository-s3","version":"8.15.3","elasticsearch_version":"8.15.3","java_version":"17","description":"The S3 repository plugin","classname":"org.elasticsearch.repositories.s3.S3RepositoryPlugin","extended_plugins":[],"has_native_controller":false,"licensed":false,"is_official":true},
  {"name":"analysis-icu","version":"8.15.3","elasticsearch_version":"8.15.3","java_version":"17","description":"ICU analysis","classname":"org.elasticsearch.plugin.analysis.icu.AnalysisICUPlugin","extended_plugins":[],"has_native_controller":false,"licensed":false,"is_official":true}
],"modules":[{"name":"aggregations","version":"8.15.3"}]}}}"#;

fn node_reported(routes: &[(&str, &str)], name: &str) -> Observation {
    let node = FakeNode::serving(routes);
    let proc = node.proc(name);
    let facet = ElasticsearchCollector::reading(&proc, false, HttpClient::new())
        .collect()
        .expect("a facet");
    items_of(&field(&facet, "nodes")).remove(0)
}

#[test]
fn collect_reports_each_plugin_by_name_with_its_version() {
    // Act
    let reported = node_reported(
        &[("/", ROOT), (PLUGINS, PLUGINS_ANSWER)],
        "elasticsearch-plugins",
    );

    // Assert: modules ship with the build the facet already names, so they are not repeated.
    let plugins = field(&reported, "plugins");
    assert_eq!(keys_of(&plugins), ["analysis-icu", "repository-s3"]);
    assert_eq!(text(&field(&plugins, "repository-s3")), "8.15.3");
}

#[test]
fn collect_refuses_an_answer_that_is_not_about_one_node() {
    // Arrange: `_local` names the node asked, and anything else is not that node's answer.
    let answer = r#"{"nodes":{}}"#;

    // Act
    let reported = node_reported(
        &[("/", ROOT), (PLUGINS, answer)],
        "elasticsearch-plugins-none",
    );

    // Assert
    let plugins = field(&reported, "plugins");
    assert_eq!(plugins.completeness(), Completeness::Incomplete);
    assert!(text(&field(&plugins, "error")).contains("one node"));
}
