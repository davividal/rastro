//! The facet itself: what it says about a box, and what it declines to say.

use rastro::collectors::rabbitmq::{BrokerClient, NodeInventory, RabbitmqCollector};
use rastro_collector::{Collector, CollectorCategory, Concurrency, Presence};

mod support;

use support::fs_tree::{scratch_tree, write};
use support::observation::{boolean, field, integer};
use support::shim;

const REGISTER: &str = "epmd: up and running on port 4369 with data:\nname rabbit at port 25672\n";
const EPMD_ARGV: &str = "/usr/lib/erlang/erts-15.2.7/bin/epmd\0-daemon\0";
const BROKER_ARGV: &str = "/usr/lib/erlang/erts-15.2.7/bin/beam.smp\0-s\0rabbit\0boot\0";

/// A socket table with no rows, which is a box that answered rather than one that refused.
///
/// Present in every fixture here on purpose: these tests are about which node the register
/// names and what the facet can read of it, and a missing table is a refused read, which fails
/// the facet before any of that is reached.
const EMPTY_TCP: &str = "  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode\n";

/// The inventory over a box the test built: a `/proc`, and a fake epmd that answers.
fn inventory_over(name: &str, processes: &[(&str, &str)]) -> NodeInventory {
    let scratch = scratch_tree(name, &["bin"]);
    let proc = scratch.join("proc");
    std::fs::create_dir_all(proc.join("net")).expect("a writable scratch directory");
    write(&proc, "net/tcp", EMPTY_TCP);

    for (pid, argv) in processes {
        std::fs::create_dir_all(proc.join(pid)).expect("a writable scratch directory");
        write(&proc, &format!("{pid}/cmdline"), argv);
    }

    let epmd = shim::executable(
        &scratch.join("bin"),
        "epmd",
        &format!("#!/bin/sh\ncat <<'OUT'\n{REGISTER}OUT\n"),
    );

    NodeInventory::using(epmd).in_proc(&proc)
}

/// The same box, with a client to ask it with.
fn collector_over(name: &str, processes: &[(&str, &str)]) -> RabbitmqCollector {
    let client = BrokerClient::using(shim::executable(
        &scratch_tree(name, &["bin"]).join("bin"),
        "rabbitmqctl",
        "#!/bin/sh\nexit 0\n",
    ));

    RabbitmqCollector::reading(Some(client), Some(inventory_over(name, processes)))
}

#[test]
fn the_facet_is_state_and_is_named_for_the_service() {
    // Act
    let collector = RabbitmqCollector::reading(None, None);

    // Assert: state rather than metadata, so an operator may exclude it.
    assert_eq!(collector.name().as_str(), "rabbitmq");
    assert_eq!(collector.category(), CollectorCategory::State);
}

#[test]
fn presence_is_absent_where_no_client_is_installed() {
    // Act & Assert: no `rabbitmqctl` on the box means RabbitMQ was never installed here,
    // which is state and not a failed look.
    assert_eq!(
        RabbitmqCollector::reading(None, None).presence(),
        Presence::Absent
    );
}

#[test]
fn presence_is_present_where_the_client_is_installed_and_nothing_runs() {
    // Arrange: installed, no epmd, no broker.
    let collector = collector_over("rabbitmq-facet-stopped", &[]);

    // Act & Assert: installed with nothing running is a different fact from not installed,
    // and the document keeps them apart.
    assert_eq!(collector.presence(), Presence::Present);
}

#[test]
fn collect_reports_an_installation_with_nothing_up_without_asking_anything() {
    // Arrange
    let collector = collector_over("rabbitmq-facet-quiet", &[]);

    // Act
    let observed = collector
        .collect()
        .expect("a stopped broker is not a failure");

    // Assert
    assert!(!boolean(&field(&observed, "port_mapper_running")));
    assert_eq!(integer(&field(&observed, "broker_processes")), 0);
}

#[test]
fn collect_keys_a_node_by_what_the_register_calls_it() {
    // Arrange
    let collector = collector_over(
        "rabbitmq-facet-running",
        &[("748", EPMD_ARGV), ("966", BROKER_ARGV)],
    );

    // Act
    let observed = collector.collect().expect("the shims answer");

    // Assert: epmd's own name for the node, which is the one thing about it that is always
    // readable. The name the node runs under is inside the entry, and here it is null: this
    // fixture's broker holds no store open, so there is nothing to read it from.
    assert!(boolean(&field(&observed, "port_mapper_running")));
    assert_eq!(integer(&field(&observed, "broker_processes")), 1);

    let node = field(&field(&observed, "nodes"), "rabbit");
    assert_eq!(integer(&field(&node, "distribution_port")), 25672);
    assert!(support::observation::is_null(&field(&node, "node_name")));
}

#[test]
fn collect_reports_a_node_it_cannot_name_rather_than_failing() {
    // Arrange: a broker whose descriptors say nothing about a store, which is what an
    // unprivileged run sees of somebody else's process.
    let collector = collector_over(
        "rabbitmq-facet-nameless",
        &[("748", EPMD_ARGV), ("966", BROKER_ARGV)],
    );

    // Act
    let observed = collector
        .collect()
        .expect("an unnameable node is not a failure");

    // Assert: the register still says a node is there, and the facet says it could not read
    // what that node calls itself. An earlier version failed the whole facet when the box
    // could not supply a hostname to compose one from; nothing is composed now, so there is
    // nothing to fail over.
    let node = field(&field(&observed, "nodes"), "rabbit");
    assert!(support::observation::is_null(&field(&node, "node_name")));
    assert!(support::observation::is_null(&field(&node, "status")));
}

#[test]
fn the_collector_runs_alone() {
    // Act & Assert: every read of a node boots an Erlang VM that joins the broker's
    // distribution cluster, which binds a port for as long as the call lasts. On the shared
    // pool that ephemeral listener, and its `beam.smp`, race the `sockets` and `processes`
    // collectors reading the same box: two runs of an unchanged host would then differ by
    // whichever of them happened to be scheduled first. Exclusive is the same answer the
    // filesystem walk gives for the same reason.
    assert_eq!(
        RabbitmqCollector::reading(None, None).concurrency(),
        Concurrency::Exclusive
    );
}

#[test]
fn presence_is_present_where_a_broker_runs_without_a_client_to_ask_with() {
    // Arrange: epmd and a broker are up, and `rabbitmqctl` is not installed, which is what a
    // box running RabbitMQ in a container and nothing on the host looks like.
    let host = inventory_over(
        "rabbitmq-facet-clientless",
        &[("748", EPMD_ARGV), ("966", BROKER_ARGV)],
    );
    let clientless = RabbitmqCollector::reading(None, Some(host));

    // Act & Assert: `absent` would be a confident lie about a box with a broker on it. What
    // rastro cannot do here is ask the node anything, and the facet says that per node rather
    // than by denying the whole installation.
    assert_eq!(clientless.presence(), Presence::Present);
    assert!(clientless.collect().is_ok());
}
