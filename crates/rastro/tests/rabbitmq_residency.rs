//! What of the Erlang runtime is already up, read from a `/proc` a test built.
//!
//! This is the gate the whole facet hangs on: a CLI tool may be invoked only where epmd is
//! resident already, because an invocation that finds nothing still leaves a daemon behind.
//! So the residency read has to be right about the two questions the dispatch asks, and it
//! has to answer them without asking anything.

use rastro::collectors::rabbitmq::ResidentRuntime;

mod support;

use support::fs_tree::{scratch_tree, write};

/// A beam started by RabbitMQ's boot script, trimmed to the tokens that identify it.
///
/// The real argv is fifty tokens of VM tuning. What matters is that `-s rabbit boot` is in
/// it, which is how the broker is told apart from any other Erlang application on the box,
/// and it is *all* that can be: the measured beam carried no node name in its argv and no
/// environment variables at all.
const BROKER_ARGV: &str =
    "/usr/lib/erlang/erts-15.2.7/bin/beam.smp\0-root\0/usr/lib/erlang\0-s\0rabbit\0boot\0";

/// Another Erlang application, which is not a broker however much it looks like one.
const OTHER_ARGV: &str = "/usr/lib/erlang/erts-15.2.7/bin/beam.smp\0-s\0ejabberd\0boot\0";

const EPMD_ARGV: &str = "/usr/lib/erlang/erts-15.2.7/bin/epmd\0-daemon\0";

#[test]
fn read_finds_the_port_mapper_and_the_broker() {
    // Arrange
    let proc = scratch_tree("rabbitmq-residency-both", &["748", "966"]);
    write(&proc, "748/cmdline", EPMD_ARGV);
    write(&proc, "966/cmdline", BROKER_ARGV);

    // Act
    let resident = ResidentRuntime::read_in(&proc);

    // Assert
    assert!(resident.port_mapper_running());
    assert_eq!(resident.broker_process_ids(), [966]);
}

#[test]
fn read_reports_no_port_mapper_on_a_box_where_nothing_erlang_runs() {
    // Arrange
    let proc = scratch_tree("rabbitmq-residency-none", &["1"]);
    write(&proc, "1/cmdline", "/sbin/init\0");

    // Act
    let resident = ResidentRuntime::read_in(&proc);

    // Assert: this is the state in which no CLI tool may be invoked at all.
    assert!(!resident.port_mapper_running());
    assert!(resident.broker_process_ids().is_empty());
}

#[test]
fn read_does_not_mistake_another_erlang_application_for_a_broker() {
    // Arrange
    let proc = scratch_tree("rabbitmq-residency-other", &["500"]);
    write(&proc, "500/cmdline", OTHER_ARGV);

    // Act
    let resident = ResidentRuntime::read_in(&proc);

    // Assert: a beam is a beam. `-s rabbit boot` is what names the application it booted,
    // and asking somebody else's node for a RabbitMQ status is both rude and wrong.
    assert!(resident.broker_process_ids().is_empty());
}

#[test]
fn read_finds_a_port_mapper_that_outlived_its_broker() {
    // Arrange: epmd detaches and stays, so a stopped broker leaves exactly this.
    let proc = scratch_tree("rabbitmq-residency-orphan", &["748"]);
    write(&proc, "748/cmdline", EPMD_ARGV);

    // Act
    let resident = ResidentRuntime::read_in(&proc);

    // Assert: the register may be read, and there will be nothing in it.
    assert!(resident.port_mapper_running());
    assert!(resident.broker_process_ids().is_empty());
}

#[test]
fn read_skips_an_entry_that_is_not_a_process_or_will_not_be_read() {
    // Arrange: `/proc` carries plenty that is not a pid, and a process that exits while the
    // table is being walked leaves a directory whose files answer ESRCH.
    let proc = scratch_tree("rabbitmq-residency-noise", &["self", "966", "cpuinfo"]);
    write(&proc, "cpuinfo/cmdline", "not a process at all\0");
    write(&proc, "966/cmdline", BROKER_ARGV);

    // Act
    let resident = ResidentRuntime::read_in(&proc);

    // Assert: a walk of the process table is never allowed to fail over one entry.
    assert_eq!(resident.broker_process_ids(), [966]);
}

#[test]
fn read_sorts_the_brokers_it_found() {
    // Arrange: directory order is the filesystem's, so two nodes on one box would otherwise
    // arrive in an order that moves between runs.
    let proc = scratch_tree("rabbitmq-residency-two", &["966", "312"]);
    write(&proc, "966/cmdline", BROKER_ARGV);
    write(&proc, "312/cmdline", BROKER_ARGV);

    // Act
    let resident = ResidentRuntime::read_in(&proc);

    // Assert
    assert_eq!(resident.broker_process_ids(), [312, 966]);
}
