//! rastro's account of the box's Elasticsearch nodes against the nodes' own answers.
//!
//! Every other test of this facet asserts what rastro does with answers somebody captured once,
//! which pins the code and cannot catch a fixture captured wrong. This one asks real nodes, the
//! way `rabbitmq_conformance.rs` asks a real broker.
//!
//! **It needs the nodes `.github/workflows/live-search.yml` starts, and fails without them**
//! rather than skipping: one per supported release and one per path through the read, each in a
//! container with no published port, so every read goes through the join into the node's
//! network namespace. That join needs `CAP_SYS_ADMIN`, which is why this file is `test = false`
//! in `Cargo.toml` and runs as root.
//!
//! The reference answers are taken with `nsenter --net=/proc/<pid>/ns/net curl`, a route that
//! shares no code with rastro's.

use std::collections::BTreeMap;
use std::process::Command;
use std::thread;
use std::time::Duration;

use rastro::collectors::elasticsearch::{ApiCredential, ElasticsearchCollector};
use rastro_collector::Collector;
use rastro_fingerprint::{Completeness, Observation};

mod support;

use support::observation::{field, is_null, items_of, keys_of, text};

/// The nodes the workflow starts, each named by its container's hostname, which is also its
/// default `node.name`: the daemonised node's `-E` settings are on no argv, so no other name
/// both rastro and `/proc` would agree on.
const OPEN_7: &str = "conformance-7";
const OPEN_8: &str = "conformance-8";

/// 9.5, open, its data on the named volume `conformance-data`.
const OPEN_9: &str = "conformance-9";

/// 9.4, started with `-d` and switched open with `-E`, which left with its launcher.
const DAEMONISED_9_4: &str = "conformance-94";

/// The tail of the volume's host directory, under whichever engine's root holds it.
const VOLUME_DIRECTORY: &str = "conformance-data/_data";

/// 8.19 at its secured default, TLS on HTTP and security on, `elastic` given this password.
const SECURED_8: &str = "conformance-secured";
const SECURED_PASSWORD: &str = "conformance-password";

/// An 8.19 node whose `elasticsearch.yml` is an absolute symlink inside its image, pinning an
/// HTTP port outside the default range. Read past the node's root, the file is missing and no
/// listener is in range, so this node is read only if the file is resolved inside it.
const SYMLINKED_8: &str = "conformance-symlinked";
const SYMLINKED_PORT: i64 = 9350;

/// The open nodes, each seeded with one of everything the facet reports.
const OPEN: [&str; 4] = [OPEN_7, OPEN_8, OPEN_9, DAEMONISED_9_4];

/// The listing of every index with its document count, hidden and system ones included, which
/// is where a write the facet caused would appear.
const INDEX_LIST: &str = "/_cat/indices?expand_wildcards=all&h=index,docs.count&s=index";

/// The node's own answer to `path`, asked from inside its namespace by another route.
fn asked_directly(process_id: u32, path: &str) -> String {
    let run = Command::new("nsenter")
        .arg(format!("--net=/proc/{process_id}/ns/net"))
        .args(["curl", "-s", &format!("http://127.0.0.1:9200{path}")])
        .output()
        .unwrap_or_else(|error| panic!("nsenter and curl are needed to ask a node: {error}"));
    assert!(
        run.status.success(),
        "asking {process_id} for {path} failed: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    String::from_utf8_lossy(&run.stdout).into_owned()
}

/// Every server process on the box, by the hostname its environment gives it.
fn servers_by_name() -> BTreeMap<String, u32> {
    let run = Command::new("pgrep")
        .args(["-f", "org.elasticsearch.bootstrap.Elasticsearch"])
        .output()
        .expect("pgrep, from procps");
    String::from_utf8_lossy(&run.stdout)
        .lines()
        .filter_map(|line| line.trim().parse::<u32>().ok())
        .filter_map(|process_id| {
            let environ = std::fs::read(format!("/proc/{process_id}/environ")).ok()?;
            let name = String::from_utf8_lossy(&environ)
                .split('\0')
                .find_map(|entry| entry.strip_prefix("HOSTNAME=").map(str::to_owned))?;
            Some((name, process_id))
        })
        .collect()
}

fn nodes_by_name(facet: &Observation) -> BTreeMap<String, Observation> {
    items_of(&field(facet, "nodes"))
        .into_iter()
        .filter_map(|node| {
            let name = field(&node, "node_name");
            (!is_null(&name)).then(|| (text(&name), node))
        })
        .collect()
}

fn reported<'facet>(
    nodes: &'facet BTreeMap<String, Observation>,
    name: &str,
) -> &'facet Observation {
    nodes
        .get(name)
        .unwrap_or_else(|| panic!("{name} in the facet: {nodes:?}"))
}

/// The one node not asked its name: the secured one, read without a credential.
fn unnamed_node(facet: &Observation) -> Observation {
    items_of(&field(facet, "nodes"))
        .into_iter()
        .find(|node| is_null(&field(node, "node_name")))
        .expect("the secured node, which tells nobody its name without a credential")
}

#[test]
fn every_node_the_workflow_started_is_found() {
    // Arrange
    let servers = servers_by_name();

    // Act
    let facet = ElasticsearchCollector::new().collect().expect("a facet");

    // Assert
    assert_eq!(
        servers.keys().cloned().collect::<Vec<_>>(),
        [
            OPEN_7,
            OPEN_8,
            OPEN_9,
            DAEMONISED_9_4,
            SECURED_8,
            SYMLINKED_8
        ],
        "start the nodes .github/workflows/live-search.yml starts"
    );
    assert_eq!(items_of(&field(&facet, "nodes")).len(), 6);
}

#[test]
fn a_node_whose_file_is_an_absolute_symlink_is_read_through_it() {
    // Act
    let facet = ElasticsearchCollector::new().collect().expect("a facet");

    // Assert
    let nodes = nodes_by_name(&facet);
    let reported = reported(&nodes, SYMLINKED_8);
    assert!(is_null(&field(reported, "error")), "{reported:?}");
    assert_eq!(
        support::observation::integer(&field(&field(reported, "http"), "port")),
        SYMLINKED_PORT
    );
}

#[test]
fn an_open_node_reads_as_it_answers_itself() {
    // Arrange
    let servers = servers_by_name();

    // Act
    let facet = ElasticsearchCollector::new().collect().expect("a facet");

    // Assert
    let nodes = nodes_by_name(&facet);
    for name in OPEN {
        let reported = reported(&nodes, name);
        let own: serde_json::Value =
            serde_json::from_str(&asked_directly(servers[name], "/")).expect("the node's JSON");

        assert!(is_null(&field(reported, "error")), "{name}: {reported:?}");
        assert!(
            is_null(&field(reported, "not_read")),
            "{name}: {reported:?}"
        );
        assert!(
            is_null(&field(reported, "unsupported")),
            "{name}: {reported:?}"
        );
        assert_eq!(reported.completeness(), Completeness::Complete, "{name}");
        assert_eq!(text(&field(reported, "network_namespace")), "separate");
        assert_eq!(
            text(&field(reported, "release")),
            own["version"]["number"].as_str().expect("a version")
        );
        assert_eq!(
            text(&field(reported, "cluster_uuid")),
            own["cluster_uuid"].as_str().expect("a cluster uuid")
        );

        let persistent = field(&field(reported, "cluster_settings"), "persistent");
        assert_eq!(
            text(&field(&persistent, "cluster.routing.allocation.enable")),
            "all",
            "{name}"
        );
        assert!(keys_of(&field(reported, "index_templates")).contains(&"app".to_owned()));
        assert!(
            keys_of(&field(reported, "component_templates")).contains(&"app-mappings".to_owned())
        );
        let indices = field(reported, "indices");
        assert_eq!(keys_of(&indices), ["myapp-tenant1", "unaliased"], "{name}");
        assert_eq!(
            text(&field(&field(&indices, "myapp-tenant1"), "index")),
            "myapp-tenant1_1790000000"
        );
        assert!(keys_of(&field(reported, "ilm_policies")).contains(&"app-policy".to_owned()));
        assert!(keys_of(&field(reported, "ingest_pipelines")).contains(&"app-pipeline".to_owned()));
        assert_eq!(
            text(&field(
                &field(&field(reported, "snapshot_repositories"), "backups"),
                "type"
            )),
            "fs"
        );
    }
}

#[test]
fn a_secured_node_without_a_credential_is_asked_over_tls_and_not_read() {
    // Act
    let facet = ElasticsearchCollector::new().collect().expect("a facet");

    // Assert: dialled in the protocol it serves, which the workflow checks in the node's own log.
    let reported = unnamed_node(&facet);
    assert_eq!(
        text(&field(&reported, "not_read")),
        "security is on and no credential was given (see --credentials)",
        "{reported:?}"
    );
    assert!(is_null(&field(&reported, "error")), "{reported:?}");
    assert_eq!(text(&field(&field(&reported, "http"), "scheme")), "https");
    assert_eq!(reported.completeness(), Completeness::Incomplete);
}

#[test]
fn a_secured_node_is_read_with_the_operators_credential() {
    // Arrange
    let credential = ApiCredential::Basic {
        username: "elastic".to_owned(),
        password: SECURED_PASSWORD.to_owned(),
    };

    // Act
    let facet = ElasticsearchCollector::authenticating(Some(credential))
        .collect()
        .expect("a facet");

    // Assert: the open nodes ignore the credential, so every node is read.
    let nodes = nodes_by_name(&facet);
    let reported = reported(&nodes, SECURED_8);
    assert!(is_null(&field(reported, "error")), "{reported:?}");
    assert!(is_null(&field(reported, "not_read")), "{reported:?}");
    assert_eq!(text(&field(reported, "release")), "8.19.22");
    assert!(!keys_of(&field(reported, "cluster_settings")).contains(&"not_read".to_owned()));
    assert_eq!(nodes.len(), 6);
}

#[test]
fn a_daemonised_node_is_read_though_its_launcher_is_gone() {
    // Act
    let facet = ElasticsearchCollector::new().collect().expect("a facet");

    // Assert: its `-E` settings left with the launcher, and the node is still read, from itself.
    let nodes = nodes_by_name(&facet);
    let reported = reported(&nodes, DAEMONISED_9_4);
    assert!(is_null(&field(reported, "error")), "{reported:?}");
    assert_eq!(text(&field(reported, "release")), "9.4.7");
}

#[test]
fn a_read_changes_no_index_on_any_open_node() {
    // Arrange: 12 s is well past the deprecation logger's 5 s flush, which a quicker look misses;
    // the write lands one to five seconds after the response.
    let servers = servers_by_name();
    let before: Vec<String> = OPEN
        .iter()
        .map(|name| asked_directly(servers[*name], INDEX_LIST))
        .collect();

    // Act
    ElasticsearchCollector::new().collect().expect("a facet");
    thread::sleep(Duration::from_secs(12));

    // Assert
    let after: Vec<String> = OPEN
        .iter()
        .map(|name| asked_directly(servers[*name], INDEX_LIST))
        .collect();
    assert_eq!(before, after);
}

#[test]
fn two_reads_of_unchanged_nodes_are_the_same() {
    // Act
    let first = ElasticsearchCollector::new().collect().expect("a facet");
    let second = ElasticsearchCollector::new().collect().expect("a facet");

    // Assert
    assert_eq!(first, second);
}

#[test]
fn a_nodes_data_on_a_volume_is_sealed_at_its_host_directory() {
    // Act: the path the node uses exists only in its own mount namespace, so this is the mount
    // tables' answer, not a path rastro could have guessed.
    let claimed: Vec<String> = ElasticsearchCollector::new()
        .filesystem_claims()
        .iter()
        .map(|claim| claim.tree().as_str().to_owned())
        .collect();

    // Assert
    assert!(
        claimed.iter().any(|tree| tree.ends_with(VOLUME_DIRECTORY)),
        "{claimed:?}"
    );
}
