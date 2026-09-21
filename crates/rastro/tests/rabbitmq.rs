//! The facet itself: what it says about a box, and what it declines to say.

use rastro::collectors::rabbitmq::{BrokerClient, NodeInventory, RabbitmqCollector};
use rastro_collector::{Collector, CollectorCategory, Presence};

mod support;

use support::fs_tree::{scratch_tree, write};
use support::observation::{boolean, field, integer};
use support::shim;

const REGISTER: &str = "epmd: up and running on port 4369 with data:\nname rabbit at port 25672\n";
const EPMD_ARGV: &str = "/usr/lib/erlang/erts-15.2.7/bin/epmd\0-daemon\0";
const BROKER_ARGV: &str = "/usr/lib/erlang/erts-15.2.7/bin/beam.smp\0-s\0rabbit\0boot\0";

/// A collector over a box the test built: a `/proc`, a fake epmd and a fake client.
fn collector_over(
    name: &str,
    processes: &[(&str, &str)],
    hostname: Result<String, String>,
) -> RabbitmqCollector {
    let scratch = scratch_tree(name, &["bin"]);
    let proc = scratch.join("proc");
    std::fs::create_dir_all(&proc).expect("a writable scratch directory");

    for (pid, argv) in processes {
        std::fs::create_dir_all(proc.join(pid)).expect("a writable scratch directory");
        write(&proc, &format!("{pid}/cmdline"), argv);
    }

    let epmd = shim::executable(
        &scratch.join("bin"),
        "epmd",
        &format!("#!/bin/sh\ncat <<'OUT'\n{REGISTER}OUT\n"),
    );
    let client = BrokerClient::using(shim::executable(
        &scratch.join("bin"),
        "rabbitmqctl",
        "#!/bin/sh\nexit 0\n",
    ));

    RabbitmqCollector::reading(
        Some(client),
        Some(NodeInventory::using(epmd, hostname).in_proc(&proc)),
    )
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
    let collector = collector_over("rabbitmq-facet-stopped", &[], Ok("box".to_owned()));

    // Act & Assert: installed with nothing running is a different fact from not installed,
    // and the document keeps them apart.
    assert_eq!(collector.presence(), Presence::Present);
}

#[test]
fn collect_reports_an_installation_with_nothing_up_without_asking_anything() {
    // Arrange
    let collector = collector_over("rabbitmq-facet-quiet", &[], Ok("box".to_owned()));

    // Act
    let observed = collector
        .collect()
        .expect("a stopped broker is not a failure");

    // Assert
    assert!(!boolean(&field(&observed, "port_mapper_running")));
    assert_eq!(integer(&field(&observed, "broker_processes")), 0);
}

#[test]
fn collect_reports_the_node_a_running_broker_registered() {
    // Arrange
    let collector = collector_over(
        "rabbitmq-facet-running",
        &[("748", EPMD_ARGV), ("966", BROKER_ARGV)],
        Ok("measured-box".to_owned()),
    );

    // Act
    let observed = collector.collect().expect("the shims answer");

    // Assert
    assert!(boolean(&field(&observed, "port_mapper_running")));
    assert_eq!(integer(&field(&observed, "broker_processes")), 1);

    let node = field(&field(&observed, "nodes"), "rabbit@measured-box");
    assert_eq!(integer(&field(&node, "distribution_port")), 25672);
}

#[test]
fn collect_fails_where_the_box_could_not_say_what_it_is_called() {
    // Arrange: the run resolves the hostname once, and a node cannot be addressed without
    // it, since `rabbitmqctl -n` takes `local@host`.
    let collector = collector_over(
        "rabbitmq-facet-nameless",
        &[("748", EPMD_ARGV)],
        Err("no hostname could be read".to_owned()),
    );

    // Act & Assert: loud, rather than a node keyed on a guess.
    assert!(collector.collect().is_err());
}
