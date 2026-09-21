//! rastro's account of a box's RabbitMQ against the broker's own answer.
//!
//! Every other test of this facet asserts what rastro does with output somebody captured
//! once, which pins the code and cannot catch a fixture captured wrong. This one asks the
//! broker. Same arrangement as `containers_conformance.rs` and `nginx_conformance.rs`, and
//! for the same reason: a test whose expected answer was written by the author of the parser
//! only re-encodes their belief.
//!
//! **It needs a live node, and it says so rather than skipping.** An absent broker is a
//! failure here, naming what to do about it. `.github/workflows/live-broker.yml` provides
//! one in CI.
//!
//! **It cannot run in an ordinary container**, which is the awkward part and is measured
//! rather than assumed: attributing a node to a broker process means reading that process's
//! descriptors, and podman's default capability set refuses that even to root, so the
//! confirmed case is unreachable there. A GitHub runner with `sudo` has the full set. That is
//! also why this file is `test = false` in `Cargo.toml`: it would otherwise fail the ordinary
//! suite on every machine that has no broker.

use std::collections::BTreeSet;
use std::process::Command;

mod support;

use rastro::collectors::rabbitmq::RabbitmqCollector;
use rastro::collectors::read_hostname;
use rastro_collector::Collector;
use support::observation::{boolean, field, items_of, keys_of, text};

const PROGRAM: &str = "rabbitmqctl";

/// The diagnostics tool, which is where the listener report lives.
const DIAGNOSTICS: &str = "rabbitmq-diagnostics";

/// What the broker says, or a failure naming how to give this test a node.
fn rabbitmqctl(arguments: &[&str]) -> String {
    run_tool(PROGRAM, arguments)
}

fn run_tool(program: &str, arguments: &[&str]) -> String {
    let run = Command::new(program)
        .args(arguments)
        .output()
        .unwrap_or_else(|error| {
            panic!(
                "this test asks the broker for its own answer, and {program} could not be \
                 run: {error}. Install rabbitmq-server and start it, or run this target \
                 where one is already running: see .github/workflows/live-broker.yml"
            )
        });

    assert!(
        run.status.success(),
        "{program} {arguments:?} failed: {}. This test needs a running node it is allowed \
         to read, which means root or the broker's own account",
        String::from_utf8_lossy(&run.stderr).trim()
    );

    String::from_utf8_lossy(&run.stdout).into_owned()
}

/// What the diagnostics tool says, under the same rules.
fn rabbitmq_diagnostics(arguments: &[&str]) -> String {
    run_tool(DIAGNOSTICS, arguments)
}

/// One column of every row in a `--formatter json` answer.
///
/// **The column is a parameter because the tool is not consistent about it**, measured rather
/// than assumed: `list_vhosts` names its column `name` and `list_users` names its column
/// `user`. A helper that hardcoded either one compared a real list against an empty set and
/// reported it as rastro disagreeing with the broker.
///
/// **JSON rather than the text form**, for a second reason found the same way:
/// `rabbitmqctl list_vhosts` prints an informational preamble *before* its header, so a
/// reader that skipped one line kept `name` as though it were a vhost.
fn column_in(output: &str, column: &str) -> BTreeSet<String> {
    let rows: Vec<serde_json::Value> = serde_json::from_str(output)
        .unwrap_or_else(|error| panic!("a --formatter json answer is a JSON array: {error}"));

    let found: BTreeSet<String> = rows
        .iter()
        .filter_map(|row| row.get(column)?.as_str().map(str::to_owned))
        .collect();

    assert!(
        !found.is_empty(),
        "no row carried a {column:?} column, so this test is wrong about the tool rather \
         than rastro being wrong about the broker: {output}"
    );

    found
}

/// The facet, read from this box.
fn facet() -> rastro_collector::Observation {
    RabbitmqCollector::new(read_hostname())
        .collect()
        .expect("a box with a running broker is readable")
}

#[test]
fn rastro_names_the_node_the_broker_names() {
    // Arrange
    let observed = facet();
    let nodes = keys_of(&field(&observed, "nodes"));

    // Act: the broker's own name for itself, which `status` carries in every listener.
    let reported = rabbitmqctl(&["eval", "node()."]).trim().to_owned();

    // Assert: rastro composes the key from the register and the box's hostname, and the
    // broker knows its own name. A disagreement here is the long-names case, which this
    // facet does not compose yet and would rather fail loudly about than paper over.
    assert!(
        nodes.contains(&reported),
        "rastro keyed {nodes:?} and the broker calls itself {reported:?}"
    );
}

#[test]
fn rastro_attributes_the_node_to_the_broker_holding_its_port() {
    // Act
    let observed = facet();
    let node = field(&field(&observed, "nodes"), &the_node(&observed));

    // Assert: the claim the whole facet turns on. A box with a running broker must reach the
    // confirmed case, not one of the two undetermined ones, or rastro would never ask a real
    // node anything.
    assert!(
        boolean(&field(&node, "runs_rabbitmq")),
        "a live broker was not attributed to its own process: {}",
        text(&field(&node, "broker_evidence"))
    );
}

#[test]
fn rastro_reports_the_vhosts_and_users_the_broker_reports() {
    // Arrange
    let observed = facet();
    let definitions = field(
        &field(&field(&observed, "nodes"), &the_node(&observed)),
        "definitions",
    );

    // Act
    let vhosts = column_in(
        &rabbitmqctl(&["list_vhosts", "--formatter", "json", "name"]),
        "name",
    );
    let users = column_in(&rabbitmqctl(&["list_users", "--formatter", "json"]), "user");

    // Assert
    assert_eq!(
        keys_of(&field(&definitions, "vhosts"))
            .into_iter()
            .collect::<BTreeSet<String>>(),
        vhosts
    );
    assert_eq!(
        keys_of(&field(&definitions, "users"))
            .into_iter()
            .collect::<BTreeSet<String>>(),
        users
    );
    assert!(!vhosts.is_empty(), "a broker always has at least one vhost");
}

#[test]
fn rastro_reports_the_data_directory_the_broker_reports() {
    // Arrange
    let observed = facet();
    let status = field(
        &field(&field(&observed, "nodes"), &the_node(&observed)),
        "status",
    );

    // Act
    let directory = rabbitmqctl(&["eval", "rabbit_mnesia:dir()."])
        .trim()
        .trim_matches('"')
        .to_owned();

    // Assert: the tree the walk seals is resolved from the node rather than assumed, so a
    // broker whose store is somewhere unusual is still sealed in the right place.
    assert_eq!(text(&field(&status, "data_directory")), directory);
}

#[test]
fn rastro_reports_every_listener_the_broker_is_offering() {
    // Arrange
    let observed = facet();
    let status = field(
        &field(&field(&observed, "nodes"), &the_node(&observed)),
        "status",
    );

    // Act: the broker's own listener report, which is a different command and a different
    // code path in the broker from the `status` this facet reads. Not an independent
    // observation of the kernel, which is the `sockets` facet's job; what it checks is that
    // rastro's reading of one report matches the broker's other account of the same thing.
    let answer = rabbitmq_diagnostics(&["listeners", "--formatter", "json"]);
    let document: serde_json::Value =
        serde_json::from_str(&answer).expect("a --formatter json answer");
    let reported: BTreeSet<i64> = document
        .get("listeners")
        .and_then(serde_json::Value::as_array)
        .expect("a listeners array")
        .iter()
        .filter_map(|listener| listener.get("port")?.as_i64())
        .collect();

    // Assert
    assert!(
        !reported.is_empty(),
        "a running broker offers at least the clustering port"
    );

    for listener in items_of(&field(&status, "listeners")) {
        let port = support::observation::integer(&field(&listener, "port"));
        assert!(
            reported.contains(&port),
            "rastro recorded a listener on {port} that the broker does not report: {reported:?}"
        );
    }
}

/// The one node key in the facet, for a box running one broker.
fn the_node(observed: &rastro_collector::Observation) -> String {
    let nodes = keys_of(&field(observed, "nodes"));

    assert_eq!(
        nodes.len(),
        1,
        "this test expects one node on the box, and found {nodes:?}"
    );

    nodes.into_iter().next().expect("one node")
}
