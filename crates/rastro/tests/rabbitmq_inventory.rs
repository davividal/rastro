//! Composing the nodes to ask from what is resident and what the register says.
//!
//! The one invariant with teeth: where the port mapper is not resident, nothing runs. A CLI
//! tool invoked on such a box leaves an `epmd -daemon` behind, so the gate is asserted here
//! against a shim that records having been executed, rather than trusted to a code reading.

use rastro::collectors::rabbitmq::{NodeInventory, ResidentRuntime};
use rastro_collector::Observation;

mod support;

use support::fs_tree::{scratch_tree, write};
use support::observation::{boolean, field, integer, keys_of};
use support::{observation::object_of, shim};

/// What epmd printed on the box the footprint was measured on.
const REGISTER: &str = "epmd: up and running on port 4369 with data:\nname rabbit at port 25672\n";

const EPMD_ARGV: &str = "/usr/lib/erlang/erts-15.2.7/bin/epmd\0-daemon\0";
const BROKER_ARGV: &str = "/usr/lib/erlang/erts-15.2.7/bin/beam.smp\0-s\0rabbit\0boot\0";

/// A `/proc` with epmd and one broker in it.
fn resident(name: &str) -> ResidentRuntime {
    let proc = scratch_tree(name, &["748", "966"]);
    write(&proc, "748/cmdline", EPMD_ARGV);
    write(&proc, "966/cmdline", BROKER_ARGV);

    ResidentRuntime::read_in(&proc)
}

/// A shim that answers like epmd and leaves evidence that it ran.
fn epmd_shim(
    directory: &std::path::Path,
    witness: &std::path::Path,
) -> rastro::collectors::canonical_tool::CanonicalTool {
    let script = format!(
        "#!/bin/sh\ntouch {}\ncat <<'OUT'\n{REGISTER}OUT\n",
        witness.display()
    );

    shim::executable(directory, "epmd", &script)
}

#[test]
fn read_asks_nothing_where_the_port_mapper_is_not_resident() {
    // Arrange
    let scratch = scratch_tree("rabbitmq-inventory-silent", &["bin"]);
    let witness = scratch.join("ran");
    let inventory = NodeInventory::using(epmd_shim(&scratch.join("bin"), &witness), "host");
    let nothing = ResidentRuntime::default();

    // Act
    let installation = inventory
        .read(&nothing)
        .expect("a box with no RabbitMQ is not a failure");

    // Assert: the gate, and the reason the whole facet is arranged this way.
    assert!(
        !witness.exists(),
        "the register was asked on a box with no port mapper, which is how a fingerprint run \
         leaves a daemon behind"
    );
    assert!(installation.nodes().is_empty());
    assert!(!installation.port_mapper_running());
}

#[test]
fn read_keys_a_node_by_the_name_a_cli_tool_would_be_given() {
    // Arrange
    let scratch = scratch_tree("rabbitmq-inventory-node", &["bin"]);
    let inventory = NodeInventory::using(
        epmd_shim(&scratch.join("bin"), &scratch.join("ran")),
        "measured-box",
    );

    // Act
    let installation = inventory
        .read(&resident("rabbitmq-inventory-node-proc"))
        .expect("the shim answers like epmd");

    // Assert: the local part comes from the register, the host from the box, because epmd
    // prints only the half before the `@` and the CLI needs both.
    let names: Vec<&str> = installation
        .nodes()
        .keys()
        .map(|name| name.as_str())
        .collect();
    assert_eq!(names, ["rabbit@measured-box"]);
}

#[test]
fn read_records_the_distribution_port_and_the_brokers_it_counted() {
    // Arrange
    let scratch = scratch_tree("rabbitmq-inventory-render", &["bin"]);
    let inventory =
        NodeInventory::using(epmd_shim(&scratch.join("bin"), &scratch.join("ran")), "box");

    // Act
    let installation = inventory
        .read(&resident("rabbitmq-inventory-render-proc"))
        .expect("the shim answers like epmd");
    let rendered = Observation::from(&installation);

    // Assert: the count rather than the pids, which move on every restart of an unchanged
    // box. Zero brokers beside a registered node is a stale register, which is why the two
    // are reported side by side rather than folded into one verdict.
    assert_eq!(
        keys_of(&rendered),
        ["broker_processes", "nodes", "port_mapper_running"]
    );
    assert!(boolean(&field(&rendered, "port_mapper_running")));
    assert_eq!(integer(&field(&rendered, "broker_processes")), 1);

    let nodes = field(&rendered, "nodes");
    let (key, node) = object_of(&nodes).into_iter().next().expect("one node");
    assert_eq!(key, "rabbit@box");
    assert_eq!(integer(&field(&node, "distribution_port")), 25672);
}

#[test]
fn read_reports_a_register_that_would_not_answer_as_a_failure() {
    // Arrange
    let scratch = scratch_tree("rabbitmq-inventory-refusal", &["bin"]);
    let refusing = shim::executable(&scratch.join("bin"), "epmd", "#!/bin/sh\nexit 1\n");
    let inventory = NodeInventory::using(refusing, "box");

    // Act & Assert: epmd is resident and will not say what it holds, which rastro cannot
    // tell apart from a node it failed to find. Loud, per the absence-is-state rule.
    assert!(
        inventory
            .read(&resident("rabbitmq-inventory-refusal-proc"))
            .is_err()
    );
}
