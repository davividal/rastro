//! The facet's own model: the order nodes are listed in, and what a value from a node's answer
//! renders and digests as.

use std::collections::BTreeMap;

use rastro::collectors::elasticsearch::{
    ApiValue, HttpEndpoint, Node, NodeIdentity, NodeVersion, Plugins, Unread,
};
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

#[test]
fn ordering_breaks_a_tie_between_unread_nodes_by_their_error() {
    // Arrange: found by review. Two nodes in containers share a config directory and, unread,
    // have no port or name, so every key tied and process-id order decided, which a restart of
    // both reverses. Their errors are stable, and different.
    let mut tls = node(10, None, "/usr/share/elasticsearch/config");
    tls.error = Some(Unread::new(
        "b: the node's settings put its HTTP listener behind TLS",
    ));
    let mut secured = node(20, None, "/usr/share/elasticsearch/config");
    secured.error = Some(Unread::new("a: the node's settings switch security on"));
    let mut nodes = [tls, secured];

    // Act
    nodes.sort_by(Node::ordering);

    // Assert
    let order: Vec<u32> = nodes.iter().map(|node| node.process_id).collect();
    assert_eq!(order, [20, 10]);
}

fn identity(cluster_uuid: &str) -> NodeIdentity {
    NodeIdentity {
        node_name: "search".to_owned(),
        cluster_name: "docker-cluster".to_owned(),
        cluster_uuid: cluster_uuid.to_owned(),
        version: NodeVersion {
            number: "8.15.3".to_owned(),
            build_flavor: None,
            build_type: None,
            build_hash: None,
        },
    }
}

#[test]
fn ordering_breaks_a_tie_between_read_nodes_by_their_cluster() {
    // Arrange: found by review. Two containers can each serve 9200 in their own namespace, share
    // the image's config directory and carry the same node name while belonging to different
    // clusters, so every earlier key ties and process-id order decided.
    let mut second = node(10, Some(9200), "/usr/share/elasticsearch/config");
    second.identity = Some(identity("zzzzzzzzzzzzzzzzzzzzzz"));
    let mut first = node(20, Some(9200), "/usr/share/elasticsearch/config");
    first.identity = Some(identity("aaaaaaaaaaaaaaaaaaaaaa"));
    let mut nodes = [second, first];

    // Act
    nodes.sort_by(Node::ordering);

    // Assert
    let order: Vec<u32> = nodes.iter().map(|node| node.process_id).collect();
    assert_eq!(order, [20, 10]);
}

#[test]
fn ordering_is_the_same_whichever_order_nodes_arrive_in_when_only_node_local_state_differs() {
    // Arrange: found by review, the third time this comparator was. Two nodes alike in every
    // identity key, one with a plugin the other lacks; plugins come from `_nodes/_local`, so they
    // are this node's own. Rather than one more key, the comparator ends on the node's whole
    // stable rendering, so two nodes it cannot tell apart render the same.
    let alike = |process_id: u32, plugins: &[(&str, &str)]| {
        let mut node = node(process_id, Some(9200), "/usr/share/elasticsearch/config");
        node.identity = Some(identity("aaaaaaaaaaaaaaaaaaaaaa"));
        node.plugins = Some(Ok(Plugins(
            plugins
                .iter()
                .map(|(name, version)| ((*name).to_owned(), (*version).to_owned()))
                .collect(),
        )));
        node
    };
    let mut forwards = [alike(10, &[]), alike(20, &[("analysis-icu", "8.15.3")])];
    let mut backwards = [alike(20, &[("analysis-icu", "8.15.3")]), alike(10, &[])];

    // Act
    forwards.sort_by(Node::ordering);
    backwards.sort_by(Node::ordering);

    // Assert
    let order = |nodes: &[Node]| nodes.iter().map(|node| node.process_id).collect::<Vec<_>>();
    assert_eq!(order(&forwards), order(&backwards));
}
