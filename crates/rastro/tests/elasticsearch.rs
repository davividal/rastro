//! The facet itself: what it says about a box, and what it declines to ask.

use std::path::Path;

use rastro::collectors::elasticsearch::{ElasticsearchCollector, HttpClient};
use rastro_collector::{Collector, CollectorCategory, Concurrency, Presence};
use rastro_fingerprint::{Completeness, Volatility};

mod support;

use support::es_node::{FakeNode, ROOT};
use support::fs_tree::scratch_tree;
use support::observation::{boolean, field, integer, is_null, items_of, text};

fn collector(proc: &Path, installed: bool) -> ElasticsearchCollector {
    ElasticsearchCollector::reading(proc, installed, HttpClient::new())
}

#[test]
fn the_facet_is_state_named_for_the_service_and_runs_alongside_others() {
    // Arrange: a request leaves an established connection and no listener, and the sockets
    // facet records listeners only, so nothing another collector reads is disturbed.
    let proc = scratch_tree("elasticsearch-facet-identity", &[]);

    // Act
    let collector = collector(&proc, false);

    // Assert
    assert_eq!(collector.name().as_str(), "elasticsearch");
    assert_eq!(collector.category(), CollectorCategory::State);
    assert_eq!(collector.concurrency(), Concurrency::Shared);
}

#[test]
fn presence_is_absent_where_nothing_is_installed_or_running() {
    // Arrange
    let proc = scratch_tree("elasticsearch-facet-absent", &["1"]);

    // Act & Assert
    assert_eq!(collector(&proc, false).presence(), Presence::Absent);
}

#[test]
fn presence_is_present_where_it_is_installed_and_stopped() {
    // Arrange
    let proc = scratch_tree("elasticsearch-facet-stopped", &["1"]);

    // Act & Assert: installed and stopped is a fact about the box, not a failure to look.
    assert_eq!(collector(&proc, true).presence(), Presence::Present);
}

#[test]
fn presence_is_present_where_a_node_runs_that_the_host_did_not_install() {
    // Arrange: a node in a container, which no package on the host accounts for.
    let node = FakeNode::serving(&[("/", ROOT)]);
    let proc = node.proc("elasticsearch-facet-container");

    // Act & Assert
    assert_eq!(collector(&proc, false).presence(), Presence::Present);
}

#[test]
fn collect_reports_an_installation_with_nothing_running() {
    // Arrange
    let proc = scratch_tree("elasticsearch-facet-idle", &["1"]);

    // Act
    let facet = collector(&proc, true).collect().expect("a facet");

    // Assert
    assert!(boolean(&field(&facet, "installed")));
    assert!(items_of(&field(&facet, "nodes")).is_empty());
}

#[test]
fn collect_reports_what_a_running_node_says_about_itself() {
    // Arrange
    let node = FakeNode::serving(&[("/", ROOT)]);
    let proc = node.proc("elasticsearch-facet-node");

    // Act
    let facet = collector(&proc, false).collect().expect("a facet");

    // Assert
    let nodes = items_of(&field(&facet, "nodes"));
    assert_eq!(nodes.len(), 1);
    let reported = &nodes[0];
    assert_eq!(text(&field(reported, "node_name")), "search-1");
    assert_eq!(text(&field(reported, "cluster_name")), "docker-cluster");
    assert_eq!(
        text(&field(reported, "cluster_uuid")),
        "uh7ULRBqQ1m4mIbk9MNkIg"
    );
    assert_eq!(
        text(&field(&field(reported, "version"), "number")),
        "8.15.3"
    );
    assert_eq!(
        text(&field(&field(reported, "version"), "build_type")),
        "docker"
    );
    assert_eq!(
        text(&field(reported, "config_directory")),
        "/etc/elasticsearch"
    );
    assert_eq!(text(&field(reported, "network_namespace")), "host");
    assert_eq!(
        integer(&field(&field(reported, "http"), "port")),
        i64::from(node.port)
    );
    assert!(is_null(&field(reported, "error")));
    assert_eq!(reported.completeness(), Completeness::Complete);
}

#[test]
fn collect_marks_the_process_id_volatile() {
    // Arrange
    let node = FakeNode::serving(&[("/", ROOT)]);
    let proc = node.proc("elasticsearch-facet-pid");

    // Act
    let facet = collector(&proc, false).collect().expect("a facet");

    // Assert: it moves on every restart, which is not a change to the node.
    let reported = &items_of(&field(&facet, "nodes"))[0];
    assert_eq!(integer(&field(reported, "process_id")), 600);
    assert_eq!(
        field(reported, "process_id").volatility(),
        Volatility::Volatile
    );
}

#[test]
fn collect_asks_the_node_nothing_but_the_reads_it_needs() {
    // Arrange
    let node = FakeNode::serving(&[("/", ROOT)]);
    let proc = node.proc("elasticsearch-facet-requests");

    // Act
    collector(&proc, false).collect().expect("a facet");

    // Assert
    assert_eq!(
        node.requests(),
        [
            "/",
            "/_cluster/settings?flat_settings=true",
            "/_index_template",
            "/_component_template",
            "/*/_alias?expand_wildcards=open,closed",
            "/_ilm/policy",
            "/_ingest/pipeline",
            "/_snapshot",
            "/_nodes/_local/plugins",
        ]
    );
}

#[test]
fn collect_does_not_dial_a_node_whose_settings_want_tls() {
    // Arrange: what 8.x's auto-configuration writes, nested.
    let node = FakeNode::serving(&[("/", ROOT)]);
    let proc = node.proc_with(
        "elasticsearch-facet-tls",
        &format!("http.port={}\0", node.port),
        Some("xpack.security.http.ssl:\n  enabled: true\n"),
    );

    // Act
    let facet = collector(&proc, false).collect().expect("a facet");

    // Assert: measured, a plaintext request to a TLS listener is a WARN in the node's log.
    let reported = &items_of(&field(&facet, "nodes"))[0];
    assert!(text(&field(reported, "error")).contains("TLS"));
    assert_eq!(reported.completeness(), Completeness::Incomplete);
    assert!(node.requests().is_empty(), "{:?}", node.requests());
}

#[test]
fn collect_reports_a_node_whose_settings_cannot_be_read_on_the_node() {
    // Arrange
    let node = FakeNode::serving(&[("/", ROOT)]);
    let proc = node.proc_with(
        "elasticsearch-facet-unresolved",
        "",
        Some("http.port: ${ES_HTTP_PORT}\n"),
    );

    // Act
    let facet = collector(&proc, false).collect().expect("a facet");

    // Assert: the refusal goes on the node, and the facet is still `ok`.
    let reported = &items_of(&field(&facet, "nodes"))[0];
    assert!(text(&field(reported, "error")).contains("ES_HTTP_PORT"));
    assert!(is_null(&field(reported, "http")));
    assert_eq!(reported.completeness(), Completeness::Incomplete);
    assert!(node.requests().is_empty());
}

#[test]
fn collect_reports_a_node_that_refuses_the_read_on_the_node() {
    // Arrange: a node answering 404 to everything stands for any refusal it gives.
    let node = FakeNode::serving(&[]);
    let proc = node.proc("elasticsearch-facet-refused");

    // Act
    let facet = collector(&proc, false).collect().expect("a facet");

    // Assert: where it listens was established, so it is kept beside the reason.
    let reported = &items_of(&field(&facet, "nodes"))[0];
    assert!(text(&field(reported, "error")).contains("404"));
    assert_eq!(
        integer(&field(&field(reported, "http"), "port")),
        i64::from(node.port)
    );
    assert!(is_null(&field(reported, "version")));
}

#[test]
fn collect_reports_an_answer_of_the_wrong_shape_naming_the_field() {
    // Arrange: an operator reading the error no longer has the answer, so a byte offset would
    // name nothing they can act on.
    let node = FakeNode::serving(&[(
        "/",
        r#"{"name":5,"cluster_name":"c","cluster_uuid":"u","version":{"number":"8.15.3"}}"#,
    )]);
    let proc = node.proc("elasticsearch-facet-wrong-shape");

    // Act
    let facet = collector(&proc, false).collect().expect("a facet");

    // Assert
    let reported = &items_of(&field(&facet, "nodes"))[0];
    let error = text(&field(reported, "error"));
    assert!(error.contains("GET /") && error.contains("name"), "{error}");
}

#[test]
fn collect_refuses_an_answer_with_more_after_it() {
    // Arrange: a second document after the first would otherwise be dropped in silence.
    let doubled = format!("{ROOT}{{}}");
    let node = FakeNode::serving(&[("/", &doubled)]);
    let proc = node.proc("elasticsearch-facet-trailing");

    // Act
    let facet = collector(&proc, false).collect().expect("a facet");

    // Assert
    let reported = &items_of(&field(&facet, "nodes"))[0];
    assert!(text(&field(reported, "error")).contains("more after it"));
}

#[test]
fn collect_reports_a_node_in_a_namespace_it_cannot_join_on_the_node() {
    // Arrange: the node's namespace link leads to something the kernel will not join.
    let node = FakeNode::serving(&[("/", ROOT)]);
    let proc = node.proc("elasticsearch-facet-unjoinable");
    let not_a_namespace = proc.join("not-a-namespace");
    std::fs::write(&not_a_namespace, "").expect("a fixture");
    let link = proc.join(support::es_node::PID).join("ns/net");
    std::fs::remove_file(&link).expect("the fixture's link");
    std::os::unix::fs::symlink(&not_a_namespace, &link).expect("a fixture");

    // Act
    let facet = collector(&proc, false).collect().expect("a facet");

    // Assert: where it listens was found from its own table; asking it was not possible.
    let reported = &items_of(&field(&facet, "nodes"))[0];
    assert_eq!(text(&field(reported, "network_namespace")), "separate");
    assert!(text(&field(reported, "error")).contains("namespace"));
    assert!(node.requests().is_empty(), "{:?}", node.requests());
}

#[test]
fn collect_does_not_dial_a_node_whose_settings_switch_security_on() {
    // Arrange: found by review. A 7.x node with security on and HTTP TLS off serves plaintext,
    // so it passed the TLS gate and was sent a request it could only refuse with a 401, which an
    // audit log records. The 401 was already this node's error, so asking bought nothing.
    let node = FakeNode::serving(&[("/", ROOT)]);
    let proc = node.proc_with(
        "elasticsearch-facet-security-on",
        &format!("http.port={}\0", node.port),
        Some("xpack.security.enabled: true\n"),
    );

    // Act
    let facet = collector(&proc, false).collect().expect("a facet");

    // Assert
    let reported = &items_of(&field(&facet, "nodes"))[0];
    assert!(text(&field(reported, "error")).contains("credentials"));
    assert!(node.requests().is_empty(), "{:?}", node.requests());
}

#[test]
fn collect_does_not_dial_a_node_that_audits_requests() {
    // Arrange: with audit logging on, any request the node receives is recorded.
    let node = FakeNode::serving(&[("/", ROOT)]);
    let proc = node.proc_with(
        "elasticsearch-facet-audit-on",
        &format!("http.port={}\0", node.port),
        Some("xpack.security.audit.enabled: true\n"),
    );

    // Act
    let facet = collector(&proc, false).collect().expect("a facet");

    // Assert
    let reported = &items_of(&field(&facet, "nodes"))[0];
    assert!(text(&field(reported, "error")).contains("audit"));
    assert!(node.requests().is_empty(), "{:?}", node.requests());
}

#[test]
fn collect_reads_a_node_whose_settings_switch_security_off() {
    // Arrange: the other direction, so the gate cannot be satisfied by refusing everything.
    let node = FakeNode::serving(&[("/", ROOT)]);
    let proc = node.proc_with(
        "elasticsearch-facet-security-off",
        &format!("http.port={}\0", node.port),
        Some("xpack.security.enabled: false\nxpack.security.audit.enabled: false\n"),
    );

    // Act
    let facet = collector(&proc, false).collect().expect("a facet");

    // Assert
    let reported = &items_of(&field(&facet, "nodes"))[0];
    assert!(is_null(&field(reported, "error")), "{reported:?}");
}

#[test]
fn collect_does_not_dial_a_node_that_audits_through_an_encoded_setting() {
    // Arrange: found by review. The encoded spelling was ignored, so audit logging switched on
    // this way read as absent and the node was sent a request it records.
    let node = FakeNode::serving(&[("/", ROOT)]);
    let proc = node.proc_with(
        "elasticsearch-facet-audit-encoded",
        &format!(
            "http.port={}\0ES_SETTING_XPACK_SECURITY_AUDIT_ENABLED=true\0",
            node.port
        ),
        None,
    );

    // Act
    let facet = collector(&proc, false).collect().expect("a facet");

    // Assert
    let reported = &items_of(&field(&facet, "nodes"))[0];
    assert!(text(&field(reported, "error")).contains("audit"));
    assert!(node.requests().is_empty(), "{:?}", node.requests());
}
