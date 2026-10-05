//! The facet itself: what it says about a box, and what it declines to ask.

use std::path::Path;

use rastro::collectors::elasticsearch::{ApiCredential, ElasticsearchCollector, HttpClient};
use rastro_collector::{Collector, CollectorCategory, Concurrency, Presence};
use rastro_fingerprint::{Completeness, Volatility};

mod support;

use support::es_node::{FakeNode, ROOT, install};
use support::fs_tree::scratch_tree;
use support::observation::{boolean, field, integer, is_null, items_of, keys_of, text};

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
            "/_cluster/state/blocks?local=true",
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
fn collect_reads_a_node_whose_settings_want_tls_over_tls() {
    // Arrange: what 8.x's auto-configuration writes, nested, before a listener whose certificate
    // nothing on this box vouches for. rastro trusts the socket it matched to the node instead.
    let node = FakeNode::serving_tls(&[("/", ROOT)]);
    let proc = node.proc_with(
        "elasticsearch-facet-tls",
        &format!("http.port={}\0", node.port),
        Some("xpack.security.http.ssl:\n  enabled: true\n"),
    );

    // Act
    let facet = collector(&proc, false).collect().expect("a facet");

    // Assert
    let reported = &items_of(&field(&facet, "nodes"))[0];
    assert!(is_null(&field(reported, "error")), "{reported:?}");
    assert_eq!(text(&field(reported, "node_name")), "search-1");
    assert_eq!(text(&field(&field(reported, "http"), "scheme")), "https");
}

#[test]
fn collect_reads_a_node_on_tls_whatever_its_settings_say() {
    // Arrange: cell 06, the blind spot the settings left. A node started with `-d` and TLS switched
    // on by an `-E` its launcher took away has a file that says plain, and was sent plaintext,
    // which the node logs as a WARN. Asked in TLS first, it answers for itself.
    let node = FakeNode::serving_tls(&[("/", ROOT)]);
    let proc = node.proc("elasticsearch-facet-tls-unsaid");

    // Act
    let facet = collector(&proc, false).collect().expect("a facet");

    // Assert
    let reported = &items_of(&field(&facet, "nodes"))[0];
    assert!(is_null(&field(reported, "error")), "{reported:?}");
    assert_eq!(text(&field(&field(reported, "http"), "scheme")), "https");
}

#[test]
fn collect_dials_a_node_on_plain_http_without_tls() {
    // Arrange
    let node = FakeNode::serving(&[("/", ROOT)]);
    let proc = node.proc("elasticsearch-facet-plain");

    // Act
    let facet = collector(&proc, false).collect().expect("a facet");

    // Assert
    let reported = &items_of(&field(&facet, "nodes"))[0];
    assert_eq!(text(&field(&field(reported, "http"), "scheme")), "http");
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

/// What a node of a cluster with security on answers a request without credentials, measured on
/// 8.19.22 (cell 02), trimmed.
const MISSING_CREDENTIALS: &str = r#"{"error":{"root_cause":[{"type":"security_exception","reason":"missing authentication credentials for REST request [/]"}],"type":"security_exception","reason":"missing authentication credentials for REST request [/]"},"status":401}"#;

#[test]
fn collect_reports_a_node_that_wants_credentials_as_not_read_and_asks_nothing_more() {
    // Arrange
    let node = FakeNode::answering(&[("/", 401, MISSING_CREDENTIALS)]);
    let proc = node.proc("elasticsearch-facet-wants-credentials");

    // Act
    let facet = collector(&proc, false).collect().expect("a facet");

    // Assert: what the box shows is kept, the node is not an error, and the read stopped at `/`.
    let reported = &items_of(&field(&facet, "nodes"))[0];
    assert_eq!(node.requests(), ["/"]);
    assert_eq!(
        text(&field(reported, "not_read")),
        "security is on and no credential was given (see --credentials)"
    );
    assert!(is_null(&field(reported, "error")));
    assert_eq!(text(&field(reported, "release")), "8.19.22");
    assert_eq!(reported.completeness(), Completeness::Incomplete);
}

/// `GET /` from a node that has not formed or joined a cluster, measured on 8.19.22 (cell 15).
const NO_MASTER_ROOT: &str = r#"{
  "name" : "cell15",
  "cluster_name" : "docker-cluster",
  "cluster_uuid" : "_na_",
  "version" : {
    "number" : "8.19.22",
    "build_flavor" : "default",
    "build_type" : "docker",
    "build_hash" : "3b2a41103de35e0af4064d647974032fcc1bcde9"
  },
  "tagline" : "You Know, for Search"
}"#;

#[test]
fn collect_reads_what_a_node_with_no_master_holds_itself_and_asks_nothing_cluster_wide() {
    // Arrange: measured on cell 15, each cluster-wide read of a node with no master waits out the
    // 30 s master timeout and answers 503. `GET /` says so at once, so they are not asked.
    let node = FakeNode::serving(&[
        ("/", NO_MASTER_ROOT),
        (
            "/_nodes/_local/plugins",
            r#"{"nodes":{"n":{"name":"cell15","plugins":[]}}}"#,
        ),
    ]);
    let proc = node.proc("elasticsearch-facet-no-master");

    // Act
    let facet = collector(&proc, false).collect().expect("a facet");

    // Assert
    let reported = &items_of(&field(&facet, "nodes"))[0];
    let asked = node.requests();
    assert!(
        asked
            .iter()
            .all(|path| path == "/" || path.starts_with("/_nodes/_local")),
        "{asked:?}"
    );
    assert!(is_null(&field(reported, "error")), "{reported:?}");
    assert!(is_null(&field(reported, "cluster_uuid")));
    assert_eq!(text(&field(reported, "node_name")), "cell15");
    assert_eq!(
        text(&field(&field(reported, "cluster_settings"), "not_read")),
        "the node has no master, so there is no cluster state to read"
    );
    assert!(!is_null(&field(reported, "plugins")));
    assert!(!keys_of(&field(reported, "plugins")).contains(&"not_read".to_owned()));
}

#[test]
fn collect_reports_a_node_that_rejects_the_credential_given_as_not_read() {
    // Arrange: the v1 limitation's other cluster, which the box's one credential is not for.
    let node = FakeNode::answering(&[("/", 401, MISSING_CREDENTIALS)]);
    let proc = node.proc("elasticsearch-facet-rejected");
    let client =
        HttpClient::new().authenticating(Some(ApiCredential::api_key("b3RoZXI6Y2x1c3Rlcg==")));

    // Act
    let facet = ElasticsearchCollector::reading(&proc, false, client)
        .collect()
        .expect("a facet");

    // Assert
    let reported = &items_of(&field(&facet, "nodes"))[0];
    assert_eq!(
        text(&field(reported, "not_read")),
        "the credential given was rejected"
    );
    assert!(is_null(&field(reported, "error")));
    // Asked without the credential first, and with it once the node answered 401.
    let sent = node.authorizations();
    assert_eq!(sent.first(), Some(&None), "{sent:?}");
    assert!(
        sent[1..]
            .iter()
            .all(|header| header.as_deref() == Some("ApiKey b3RoZXI6Y2x1c3Rlcg==")),
        "{sent:?}"
    );
}

#[test]
fn collect_asks_nothing_more_of_a_listener_the_node_no_longer_holds() {
    // Arrange: found by review. The node exits after its listener was found, and another process
    // could take the port and be sent the next request, a credential with it. Each request checks
    // the node still holds the listener; here it lets go of it once `/` is answered.
    let name = "elasticsearch-facet-listener-lost";
    let descriptor = std::path::Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join(name)
        .join("600/fd/3");
    let node = FakeNode::serving_then(&[("/", ROOT)], move |path| {
        if path == "/" {
            let _ = std::fs::remove_file(&descriptor);
        }
    });
    let proc = node.proc(name);

    // Act
    let facet = collector(&proc, false).collect().expect("a facet");

    // Assert
    let reported = &items_of(&field(&facet, "nodes"))[0];
    assert_eq!(node.requests(), ["/"]);
    assert!(
        text(&field(&field(reported, "cluster_settings"), "error")).contains("no longer"),
        "{reported:?}"
    );
}

#[test]
fn collect_asks_nothing_more_of_a_process_id_another_process_has_taken() {
    // Arrange: found by review. The node exits and its process id is reused by a program that
    // binds the same port, which a check of the listener alone accepts. A process is its id and
    // its start time together, field 22 of `stat`, so the start fixed at the census must hold.
    let name = "elasticsearch-facet-pid-reused";
    let stat = std::path::Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join(name)
        .join("600/stat");
    let node = FakeNode::serving_then(&[("/", ROOT)], move |path| {
        if path == "/" {
            let middle = vec!["0"; 17].join(" ");
            let _ = std::fs::write(&stat, format!("600 (other) S 1 {middle} 9999 0 0\n"));
        }
    });
    let proc = node.proc(name);

    // Act
    let facet = collector(&proc, false).collect().expect("a facet");

    // Assert
    let reported = &items_of(&field(&facet, "nodes"))[0];
    assert_eq!(node.requests(), ["/"]);
    assert!(
        text(&field(&field(reported, "cluster_settings"), "error")).contains("replaced"),
        "{reported:?}"
    );
}

#[test]
fn collect_reads_a_node_upgraded_under_itself_as_what_it_runs_and_says_a_restart_is_pending() {
    // Arrange: found by the third domain review, measured on cell 31. A package upgraded without
    // a restart is a state a fingerprint should show, not a node rastro failed to read.
    let answer = root_of("8.15.3");
    let node = FakeNode::serving(&[("/", answer.as_str())]);
    let proc = node.proc("elasticsearch-facet-pending-restart");
    std::os::unix::fs::symlink(
        "/usr/share/elasticsearch/lib/elasticsearch-8.15.3.jar (deleted)",
        proc.join("600/fd/9"),
    )
    .expect("a writable fixture");

    // Act
    let facet = collector(&proc, false).collect().expect("a facet");

    // Assert
    let reported = &items_of(&field(&facet, "nodes"))[0];
    assert!(is_null(&field(reported, "error")), "{reported:?}");
    assert_eq!(text(&field(reported, "release")), "8.15.3");
    assert_eq!(text(&field(reported, "installed_release")), "8.19.22");
    assert_eq!(text(&field(reported, "node_name")), "search-1");
}

/// The local cluster blocks of a node whose master is gone, measured on cell 28.
const NO_MASTER_BLOCKS: &str = r#"{"cluster_name":"cell28","cluster_uuid":"HGZmhCrtQU-O_CgWnC4NlQ","blocks":{"global":{"2":{"description":"no master","retryable":true,"levels":["write","metadata_write"]}}}}"#;

#[test]
fn collect_asks_nothing_cluster_wide_of_a_node_that_lost_its_master() {
    // Arrange: found by the third domain review, measured on cell 28. A survivor of a cluster
    // whose master is gone still answers `GET /` with its cluster's UUID, and each cluster-wide
    // read waited out the 30 s master timeout. Its local blocks say so at once.
    let node = FakeNode::serving(&[
        ("/", ROOT),
        ("/_cluster/state/blocks?local=true", NO_MASTER_BLOCKS),
    ]);
    let proc = node.proc("elasticsearch-facet-lost-master");

    // Act
    let facet = collector(&proc, false).collect().expect("a facet");

    // Assert
    let reported = &items_of(&field(&facet, "nodes"))[0];
    let asked = node.requests();
    assert!(
        asked.iter().all(|path| path == "/"
            || path == "/_cluster/state/blocks?local=true"
            || path.starts_with("/_nodes/_local")),
        "{asked:?}"
    );
    assert!(is_null(&field(reported, "error")), "{reported:?}");
    assert_eq!(
        text(&field(&field(reported, "cluster_settings"), "not_read")),
        "the node has no master, so there is no cluster state to read"
    );
}

#[test]
fn collect_asks_a_9_node_for_its_blocks_without_the_parameter_9_deprecates() {
    // Arrange: measured on 9.4.7 and 9.5.4, `?local` on this API is deprecated, a warning the
    // node indexes, and has no effect: the answer is the node's own state either way.
    let answer = root_of("9.5.4");
    let node = FakeNode::serving(&[("/", answer.as_str())]);
    let proc = node.proc("elasticsearch-facet-blocks-9");
    install(&proc, "9.5.4");

    // Act
    collector(&proc, false).collect().expect("a facet");

    // Assert
    let asked = node.requests();
    assert!(
        asked.contains(&"/_cluster/state/blocks".to_owned()),
        "{asked:?}"
    );
    assert!(
        !asked.iter().any(|path| path.contains("local=true")),
        "{asked:?}"
    );
}

#[test]
fn collect_reports_a_node_whose_process_files_are_refused_as_not_read_naming_why() {
    // Arrange: found by the third domain review, measured as `nobody`. Another account's
    // `environ`, `fd` and `root` are refused to an unprivileged run, and the node was an error
    // blaming a missing jar. A refusal stands in here as an `environ` that is a directory, which
    // is refused to root as well, so the test holds for both runs of the suite.
    let node = FakeNode::serving(&[("/", ROOT)]);
    let proc = node.proc("elasticsearch-facet-process-files-refused");
    std::fs::remove_file(proc.join("600/environ")).expect("the fixture's environ");
    std::fs::create_dir(proc.join("600/environ")).expect("a writable fixture");

    // Act
    let facet = collector(&proc, false).collect().expect("a facet");

    // Assert
    let reported = &items_of(&field(&facet, "nodes"))[0];
    assert!(node.requests().is_empty(), "{:?}", node.requests());
    assert!(is_null(&field(reported, "error")), "{reported:?}");
    let reason = text(&field(reported, "not_read"));
    assert!(reason.contains("environ"), "{reason}");
    assert!(reason.contains("root"), "{reason}");
    // Found by review: the process id moves on every restart, which is why `process_id` is
    // volatile, and a reason naming it differed between two runs either side of one.
    assert!(!reason.contains("600"), "{reason}");
}
