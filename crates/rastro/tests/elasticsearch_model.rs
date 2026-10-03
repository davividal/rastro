//! The facet's own model: the order nodes are listed in, and what a value from a node's answer
//! renders and digests as.

use std::collections::BTreeMap;

use rastro::collectors::elasticsearch::{ApiValue, HttpEndpoint, Node};
use rastro::collectors::inet::{InetHost, PortNumber};
use rastro_fingerprint::Observation;

mod support;

use support::observation::is_null;

fn node(process_id: u32, port: Option<u16>, config: &str) -> Node {
    Node {
        process_id,
        config_directory: Some(config.to_owned()),
        network_namespace: None,
        http: port.map(|port| {
            HttpEndpoint::new(
                InetHost::new("127.0.0.1").expect("a host"),
                PortNumber::parse(&port.to_string()).expect("a port"),
            )
        }),
        identity: None,
        cluster_settings: None,
        index_templates: None,
        component_templates: None,
        indices: None,
        ilm_policies: None,
        ingest_pipelines: None,
        snapshot_repositories: None,
        plugins: None,
        error: None,
    }
}

#[test]
fn ordering_lists_nodes_by_port_then_configuration_and_never_by_process_id() {
    // Arrange: process ids in the opposite order, as after a restart of both nodes. An unread
    // node has no port, and among those the configuration directory decides.
    let mut nodes = [
        node(10, Some(9201), "/etc/b"),
        node(20, Some(9200), "/etc/a"),
        node(30, None, "/etc/z"),
        node(40, None, "/etc/y"),
    ];

    // Act
    nodes.sort_by(Node::ordering);

    // Assert
    let order: Vec<u32> = nodes.iter().map(|node| node.process_id).collect();
    assert_eq!(order, [40, 30, 20, 10]);
}

#[test]
fn digest_tells_a_list_of_one_joined_value_from_a_list_of_two() {
    // Arrange: the encoding tags lengths so these cannot coincide.
    let joined = ApiValue::List(vec![ApiValue::Text("a,b".to_owned())]);
    let split = ApiValue::List(vec![
        ApiValue::Text("a".to_owned()),
        ApiValue::Text("b".to_owned()),
    ]);

    // Act & Assert
    assert_ne!(joined.digest(), split.digest());
}

#[test]
fn digest_tells_each_kind_from_its_spelling_in_another_kind() {
    // Arrange: a mapping that changed `null` to `"null"`, or `true` to `1`, is a changed mapping.
    let pairs = [
        (ApiValue::Null, ApiValue::Text("null".to_owned())),
        (ApiValue::Boolean(true), ApiValue::Integer(1)),
        (ApiValue::Integer(1), ApiValue::Text("1".to_owned())),
        (
            ApiValue::List(Vec::new()),
            ApiValue::Object(BTreeMap::new()),
        ),
    ];

    // Act & Assert
    for (left, right) in pairs {
        assert_ne!(left.digest(), right.digest(), "{left:?} against {right:?}");
    }
}

#[test]
fn a_null_renders_as_null() {
    // Act
    let rendered = Observation::from(&ApiValue::Null);

    // Assert
    assert!(is_null(&rendered));
}
