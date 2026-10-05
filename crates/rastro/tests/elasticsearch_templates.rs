//! Composable index templates and component templates: what the next index will be made from.
//!
//! Legacy templates are not read. Reading one back writes nothing, measured, but the legacy API
//! is the corner where a deprecated parameter is one release away. See `docs/decisions.md`.

use rastro::collectors::elasticsearch::{ElasticsearchCollector, HttpClient};
use rastro_collector::Collector;
use rastro_fingerprint::{Observation, Sensitivity};

mod support;

use support::es_node::{FakeNode, ROOT};
use support::observation::{field, integer, items_of, keys_of, text};

const INDEX_TEMPLATES: &str = "/_index_template";
const COMPONENT_TEMPLATES: &str = "/_component_template";

/// Measured on 8.15.3, trimmed to the template the test set up and one the node ships.
const INDEX_TEMPLATE_ANSWER: &str = r#"{"index_templates":[
  {"name":"logs","index_template":{"index_patterns":["logs-*-*"],"composed_of":["logs@mappings","logs@settings"],"priority":100,"version":14,"_meta":{"managed":true},"data_stream":{"hidden":false,"allow_custom_routing":false}}},
  {"name":"app","index_template":{"index_patterns":["myapp-*"],"template":{"settings":{"index":{"number_of_shards":"1","number_of_replicas":"0"}}},"composed_of":["app-mappings"],"priority":100}}
]}"#;

const COMPONENT_TEMPLATE_ANSWER: &str = r#"{"component_templates":[
  {"name":"app-mappings","component_template":{"template":{"mappings":{"properties":{"score":{"type":"float"},"title":{"type":"text"}}}}}}
]}"#;

fn node_reported(node: &FakeNode, name: &str) -> Observation {
    let proc = node.proc(name);
    let facet = ElasticsearchCollector::reading(&proc, false, HttpClient::new())
        .collect()
        .expect("a facet");
    items_of(&field(&facet, "nodes")).remove(0)
}

#[test]
fn collect_keys_index_templates_by_name_whatever_order_the_node_lists_them_in() {
    // Arrange
    let node = FakeNode::serving(&[
        ("/", ROOT),
        (INDEX_TEMPLATES, INDEX_TEMPLATE_ANSWER),
        (COMPONENT_TEMPLATES, COMPONENT_TEMPLATE_ANSWER),
    ]);

    // Act
    let reported = node_reported(&node, "elasticsearch-templates-keys");

    // Assert
    let templates = field(&reported, "index_templates");
    assert_eq!(keys_of(&templates), ["app", "logs"]);
    let app = field(&templates, "app");
    assert_eq!(integer(&field(&app, "priority")), 100);
    assert_eq!(
        items_of(&field(&app, "composed_of"))
            .iter()
            .map(text)
            .collect::<Vec<_>>(),
        ["app-mappings"]
    );
}

#[test]
fn collect_keeps_the_order_of_a_templates_patterns() {
    // Arrange: `composed_of` is applied in order, so a reordering is a change to what an
    // index is made from.
    let node = FakeNode::serving(&[
        ("/", ROOT),
        (INDEX_TEMPLATES, INDEX_TEMPLATE_ANSWER),
        (COMPONENT_TEMPLATES, COMPONENT_TEMPLATE_ANSWER),
    ]);

    // Act
    let reported = node_reported(&node, "elasticsearch-templates-order");

    // Assert
    let logs = field(&field(&reported, "index_templates"), "logs");
    assert_eq!(
        items_of(&field(&logs, "composed_of"))
            .iter()
            .map(text)
            .collect::<Vec<_>>(),
        ["logs@mappings", "logs@settings"]
    );
}

#[test]
fn collect_reports_component_templates_by_name() {
    // Arrange
    let node = FakeNode::serving(&[
        ("/", ROOT),
        (INDEX_TEMPLATES, INDEX_TEMPLATE_ANSWER),
        (COMPONENT_TEMPLATES, COMPONENT_TEMPLATE_ANSWER),
    ]);

    // Act
    let reported = node_reported(&node, "elasticsearch-templates-components");

    // Assert
    let mappings = field(
        &field(&field(&reported, "component_templates"), "app-mappings"),
        "template",
    );
    let title = field(&field(&field(&mappings, "mappings"), "properties"), "title");
    assert_eq!(text(&field(&title, "type")), "text");
}

#[test]
fn collect_reports_a_node_with_no_templates_as_none() {
    // Arrange
    let node = FakeNode::serving(&[
        ("/", ROOT),
        (INDEX_TEMPLATES, r#"{"index_templates":[]}"#),
        (COMPONENT_TEMPLATES, r#"{"component_templates":[]}"#),
    ]);

    // Act
    let reported = node_reported(&node, "elasticsearch-templates-none");

    // Assert
    assert!(keys_of(&field(&reported, "index_templates")).is_empty());
    assert!(keys_of(&field(&reported, "component_templates")).is_empty());
}

#[test]
fn collect_withholds_each_value_a_template_sets_and_keeps_its_structure() {
    // Arrange: found by review. A template's `settings` are index settings, which a plugin's
    // unfiltered credential can be among; its patterns, priority and composition are structure an
    // operator diffs.
    let node = FakeNode::serving(&[
        ("/", ROOT),
        (INDEX_TEMPLATES, INDEX_TEMPLATE_ANSWER),
        (COMPONENT_TEMPLATES, COMPONENT_TEMPLATE_WITH_SETTINGS),
    ]);

    // Act
    let reported = node_reported(&node, "elasticsearch-templates-settings-withheld");

    // Assert
    let app = field(&field(&reported, "index_templates"), "app");
    let shards = field(
        &field(&field(&field(&app, "template"), "settings"), "index"),
        "number_of_shards",
    );
    assert_eq!(shards.sensitivity(), Sensitivity::Sensitive);
    assert_eq!(field(&app, "priority").sensitivity(), Sensitivity::Public);
    assert_eq!(
        items_of(&field(&app, "index_patterns"))[0].sensitivity(),
        Sensitivity::Public
    );
    let component = field(&field(&reported, "component_templates"), "app-settings");
    let refresh = field(
        &field(&field(&component, "template"), "settings"),
        "index.refresh_interval",
    );
    assert_eq!(refresh.sensitivity(), Sensitivity::Sensitive);
}

const COMPONENT_TEMPLATE_WITH_SETTINGS: &str = r#"{"component_templates":[
  {"name":"app-settings","component_template":{"template":{"settings":{"index.refresh_interval":"30s"}}}}
]}"#;
