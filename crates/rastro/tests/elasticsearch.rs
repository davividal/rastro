//! The facet itself: what it says about a box, and what it declines to ask.

use std::path::Path;

use rastro::collectors::elasticsearch::{ElasticsearchCollector, HttpClient};
use rastro_collector::{Collector, CollectorCategory, Concurrency, Presence};
use rastro_fingerprint::{Completeness, Volatility};

mod support;

use support::es_node::{FakeNode, ROOT, install};
use support::fs_tree::scratch_tree;
use support::observation::{boolean, field, integer, is_null, items_of, text};

fn collector(proc: &Path, package_installed: bool) -> ElasticsearchCollector {
    ElasticsearchCollector::reading(proc, package_installed, HttpClient::new())
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
    assert!(boolean(&field(&facet, "package_installed")));
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
        "8.19.22"
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
            "/*,-.*/_alias?expand_wildcards=open,closed",
            "/_ilm/policy",
            "/_ingest/pipeline",
            "/_snapshot",
            "/_nodes/_local/plugins",
            "/_nodes/_local?flat_settings=true&filter_path=nodes.*.settings,nodes.*.roles,nodes.*.attributes,nodes.*.jvm.input_arguments,nodes.*.jvm.mem.heap_max_in_bytes",
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
fn collect_asks_a_node_whose_settings_switch_security_on() {
    // Arrange: measured on cell 05, a daemonised 9.4 node switched open with `-E` has nothing in
    // its file, and 9.4's default reads as security on. Whether a node wants credentials is the
    // node's to say, in its answer, so the file no longer decides it.
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
    assert_eq!(node.requests().first().map(String::as_str), Some("/"));
    assert!(is_null(&field(reported, "error")), "{reported:?}");
}

#[test]
fn collect_asks_a_node_that_audits_requests() {
    // Arrange: the audit gate is gone with the credentials it guarded against. A read made with
    // the operator's own credential is the operator's to have audited.
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
    assert!(!node.requests().is_empty());
    assert!(is_null(&field(reported, "error")), "{reported:?}");
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
fn presence_cannot_tell_where_a_process_could_not_be_inspected_and_nothing_else_is_found() {
    // Arrange: nothing installed, no node found, and one process it was refused.
    let proc = scratch_tree("elasticsearch-facet-unseen-only", &["900/cmdline"]);

    // Act
    let presence = collector(&proc, false).presence();

    // Assert: not `absent`, which would be a confident claim about a box it could not see.
    assert!(
        matches!(presence, Presence::Undetermined { .. }),
        "{presence:?}"
    );
}

#[test]
fn collect_marks_the_facet_incomplete_where_a_process_could_not_be_inspected() {
    // Arrange: one node found, beside a process that may be another.
    let node = FakeNode::serving(&[("/", ROOT)]);
    let proc = node.proc("elasticsearch-facet-unseen-beside-node");
    std::fs::create_dir_all(proc.join("900/cmdline")).expect("a writable fixture");

    // Act
    let facet = collector(&proc, false).collect().expect("a facet");

    // Assert: the node is reported, and so is what could not be looked at.
    assert_eq!(items_of(&field(&facet, "nodes")).len(), 1);
    let unseen = field(&facet, "uninspected_processes");
    assert_eq!(unseen.completeness(), Completeness::Incomplete);
}

#[test]
fn collect_does_not_read_a_node_that_has_not_joined_a_cluster() {
    // Arrange: found by the second domain review. `GET /` answers `"_na_"` for the cluster UUID
    // while a node has not formed or joined a cluster, and its surfaces are not the cluster's yet.
    let unjoined = ROOT.replace("uh7ULRBqQ1m4mIbk9MNkIg", "_na_");
    let node = FakeNode::serving(&[("/", &unjoined)]);
    let proc = node.proc("elasticsearch-facet-unjoined");

    // Act
    let facet = collector(&proc, false).collect().expect("a facet");

    // Assert
    let reported = &items_of(&field(&facet, "nodes"))[0];
    assert!(
        text(&field(reported, "error")).contains("cluster"),
        "{reported:?}"
    );
    assert_eq!(node.requests(), ["/"]);
}

/// `GET /` as a node of `release` answers it.
fn root_of(release: &str) -> String {
    ROOT.replace(
        r#""number" : "8.19.22""#,
        &format!(r#""number" : "{release}""#),
    )
}

#[test]
fn collect_names_the_release_the_install_holds() {
    // Arrange
    let node = FakeNode::serving(&[("/", ROOT)]);
    let proc = node.proc("elasticsearch-facet-release");

    // Act
    let facet = collector(&proc, false).collect().expect("a facet");

    // Assert
    let reported = &items_of(&field(&facet, "nodes"))[0];
    assert_eq!(text(&field(reported, "release")), "8.19.22");
    assert!(is_null(&field(reported, "unsupported")));
    assert_eq!(facet.approximate_items(), 0);
}

#[test]
fn collect_reads_an_unsupported_release_as_the_closest_supported_one_and_says_so() {
    // Arrange
    let answer = root_of("8.15.3");
    let node = FakeNode::serving(&[("/", answer.as_str())]);
    let proc = node.proc("elasticsearch-facet-unsupported");
    install(&proc, "8.15.3");

    // Act
    let facet = collector(&proc, false).collect().expect("a facet");

    // Assert: read, and marked, so the run's summary tells the operator.
    let reported = &items_of(&field(&facet, "nodes"))[0];
    assert_eq!(text(&field(reported, "node_name")), "search-1");
    assert_eq!(
        text(&field(reported, "unsupported")),
        "version 8.15.3 is not supported, read as 8.19"
    );
    assert_eq!(facet.approximate_items(), 1);
}

#[test]
fn collect_sends_nothing_to_a_node_below_7_and_says_why() {
    // Arrange
    let node = FakeNode::serving(&[("/", ROOT)]);
    let proc = node.proc("elasticsearch-facet-below-seven");
    install(&proc, "6.8.23");

    // Act
    let facet = collector(&proc, false).collect().expect("a facet");

    // Assert: listed with what the box shows, not asked, and not an error.
    let reported = &items_of(&field(&facet, "nodes"))[0];
    assert!(node.requests().is_empty(), "{:?}", node.requests());
    assert_eq!(text(&field(reported, "release")), "6.8.23");
    assert_eq!(
        text(&field(reported, "not_read")),
        "version 6.8.23 is below 7, which rastro does not read"
    );
    assert!(is_null(&field(reported, "error")));
    assert_eq!(reported.completeness(), Completeness::Incomplete);
}

#[test]
fn collect_sends_nothing_to_a_node_whose_release_cannot_be_read() {
    // Arrange
    let node = FakeNode::serving(&[("/", ROOT)]);
    let proc = node.proc("elasticsearch-facet-no-release");
    std::fs::remove_dir_all(proc.join("600/root/usr/share/elasticsearch/lib")).expect("a lib");

    // Act
    let facet = collector(&proc, false).collect().expect("a facet");

    // Assert
    let reported = &items_of(&field(&facet, "nodes"))[0];
    assert!(node.requests().is_empty(), "{:?}", node.requests());
    assert!(
        text(&field(reported, "error")).contains("release"),
        "{}",
        text(&field(reported, "error"))
    );
}

#[test]
fn collect_refuses_a_node_whose_answer_names_another_release_than_its_install() {
    // Arrange: the jar was replaced under a running node, or the listener is not this node's.
    let answer = root_of("8.19.21");
    let node = FakeNode::serving(&[("/", answer.as_str())]);
    let proc = node.proc("elasticsearch-facet-release-mismatch");

    // Act
    let facet = collector(&proc, false).collect().expect("a facet");

    // Assert
    let reported = &items_of(&field(&facet, "nodes"))[0];
    assert!(
        text(&field(reported, "error")).contains("8.19.21"),
        "{}",
        text(&field(reported, "error"))
    );
    assert!(is_null(&field(reported, "cluster_settings")));
}
