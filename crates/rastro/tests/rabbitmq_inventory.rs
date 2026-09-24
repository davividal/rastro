//! Composing the nodes to ask from what is resident and what the register says.
//!
//! The one invariant with teeth: where the port mapper is not resident, nothing runs. A CLI
//! tool invoked on such a box leaves an `epmd -daemon` behind, so the gate is asserted here
//! against a shim that records having been executed, rather than trusted to a code reading.

use std::os::unix::fs::symlink;
use std::path::Path;

use rastro::collectors::canonical_tool::CanonicalTool;
use rastro::collectors::rabbitmq::NodeInventory;
use rastro_collector::Observation;

mod support;

use support::fs_tree::{scratch_tree, write};
use support::observation::{boolean, field, integer, keys_of, object_of};
use support::shim;

/// What epmd printed on the box the footprint was measured on.
const REGISTER: &str = "epmd: up and running on port 4369 with data:\nname rabbit at port 25672\n";

const EPMD_ARGV: &str = "/usr/lib/erlang/erts-15.2.7/bin/epmd\0-daemon\0";
const BROKER_ARGV: &str = "/usr/lib/erlang/erts-15.2.7/bin/beam.smp\0-s\0rabbit\0boot\0";

/// A socket table with no rows, which is a box that answered rather than one that refused.
///
/// Present in every fixture here on purpose: these tests are about which node the register
/// names and what the facet can read of it, and a missing table is a refused read, which fails
/// the facet before any of that is reached.
const EMPTY_TCP: &str = "  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode\n";

/// A `/proc` holding the processes named, under a scratch root of this test's own.
fn proc_with(root: &Path, processes: &[(&str, &str)]) -> std::path::PathBuf {
    let proc = root.join("proc");
    std::fs::create_dir_all(proc.join("net")).expect("a writable scratch directory");
    write(&proc, "net/tcp", EMPTY_TCP);

    for (pid, argv) in processes {
        // `fd` as well, because a process without one is a process rastro cannot read a node
        // name or a store from, and every real one has it.
        std::fs::create_dir_all(proc.join(pid).join("fd")).expect("a writable scratch directory");
        write(&proc, &format!("{pid}/cmdline"), argv);
    }

    proc
}

/// A shim that answers like epmd and leaves evidence that it ran.
fn epmd_shim(directory: &Path, witness: &Path) -> CanonicalTool {
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
    let inventory = NodeInventory::using(epmd_shim(&scratch.join("bin"), &witness))
        .in_proc(&proc_with(&scratch, &[("1", "/sbin/init\0")]));

    // Act
    let installation = inventory
        .read(None)
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

/// A descriptor a running broker holds: Ra's log, under the store, in a directory named for
/// the node. It is where the node's own name is read from.
const WAL: &str = "/var/lib/rabbitmq/mnesia/rabbit@box/quorum/rabbit@box/00000001.wal";

#[test]
fn read_keys_a_node_by_the_name_the_register_knows_it_by() {
    // Arrange: a registration whose broker holds nothing open, so its own name cannot be
    // read.
    let scratch = scratch_tree("rabbitmq-inventory-node", &["bin"]);
    let inventory = NodeInventory::using(epmd_shim(&scratch.join("bin"), &scratch.join("ran")))
        .in_proc(&proc_with(&scratch, &[("748", EPMD_ARGV)]));

    // Act
    let installation = inventory.read(None).expect("the shim answers like epmd");

    // Assert: epmd's own name for the node, which is the half that is always readable. The
    // name the node runs under is a separate field, and it is absent here because nothing on
    // the box said what it is.
    let keys: Vec<&str> = installation.nodes().keys().map(String::as_str).collect();
    assert_eq!(keys, ["rabbit"]);
    assert!(installation.nodes()["rabbit"].node_name.is_none());
}

#[test]
fn read_names_a_node_from_the_files_its_broker_holds_open() {
    // Arrange: the same box, with a broker holding its store open.
    let scratch = scratch_tree("rabbitmq-inventory-named", &["bin"]);
    let proc = proc_with(&scratch, &[("748", EPMD_ARGV), ("966", BROKER_ARGV)]);
    symlink(WAL, proc.join("966/fd/9")).expect("a writable scratch symlink");
    let inventory =
        NodeInventory::using(epmd_shim(&scratch.join("bin"), &scratch.join("ran"))).in_proc(&proc);

    // Act
    let installation = inventory.read(None).expect("the shim answers like epmd");

    // Assert: read from the directory the broker writes into, never composed from the box's
    // hostname. A node under long names is the case that proves the difference.
    assert_eq!(
        installation.nodes()["rabbit"]
            .node_name
            .as_ref()
            .expect("a name")
            .as_str(),
        "rabbit@box"
    );
}

#[test]
fn read_records_the_distribution_port_and_the_brokers_it_counted() {
    // Arrange
    let scratch = scratch_tree("rabbitmq-inventory-render", &["bin"]);
    let inventory =
        NodeInventory::using(epmd_shim(&scratch.join("bin"), &scratch.join("ran"))).in_proc(
            &proc_with(&scratch, &[("748", EPMD_ARGV), ("966", BROKER_ARGV)]),
        );

    // Act
    let installation = inventory.read(None).expect("the shim answers like epmd");
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
    assert_eq!(key, "rabbit");
    assert_eq!(integer(&field(&node, "distribution_port")), 25672);
}

#[test]
fn read_reports_a_register_that_would_not_answer_as_a_failure() {
    // Arrange
    let scratch = scratch_tree("rabbitmq-inventory-refusal", &["bin"]);
    let refusing = shim::executable(&scratch.join("bin"), "epmd", "#!/bin/sh\nexit 1\n");
    let inventory =
        NodeInventory::using(refusing).in_proc(&proc_with(&scratch, &[("748", EPMD_ARGV)]));

    // Act & Assert: epmd is resident and will not say what it holds, which rastro cannot
    // tell apart from a node it failed to find. Loud, per the absence-is-state rule.
    assert!(inventory.read(None).is_err());
}

#[test]
fn a_cli_tools_own_node_is_not_a_node_on_the_box() {
    // Arrange: the register as it reads while somebody runs `rabbitmqctl`. Measured by
    // sampling it continuously during six calls, and caught in CI from the other side, where
    // the facet reported `["rabbit", "rabbitmqcli-819-rabbit"]` as two nodes.
    let both = "epmd: up and running on port 4369 with data:\n\
                name rabbit at port 25672\n\
                name rabbitmqcli-308-rabbit at port 35672\n";
    let scratch = scratch_tree("rabbitmq-inventory-cli-node", &["bin"]);
    let epmd = shim::executable(
        &scratch.join("bin"),
        "epmd",
        &format!("#!/bin/sh\ncat <<'OUT'\n{both}OUT\n"),
    );
    let inventory = NodeInventory::using(epmd).in_proc(&proc_with(&scratch, &[("748", EPMD_ARGV)]));

    // Act
    let installation = inventory.read(None).expect("the shim answers like epmd");

    // Assert: the entry exists only while a tool is running, so reporting it would make two
    // runs of an unchanged box differ, which is the one thing the document promises not to do.
    let keys: Vec<&str> = installation.nodes().keys().map(String::as_str).collect();
    assert_eq!(keys, ["rabbit"]);
}

#[test]
fn the_register_itself_still_reports_what_epmd_printed() {
    // Act & Assert: the filtering belongs to what rastro calls a node, not to the reading of
    // the tool's output. A source that quietly dropped rows would make the two disagree about
    // what epmd said.
    let both = "epmd: up and running on port 4369 with data:\n\
                name rabbit at port 25672\n\
                name rabbitmqcli-308-rabbit at port 35672\n";
    let registered = rastro::collectors::rabbitmq::EpmdRegister::parse(both).expect("well formed");

    assert_eq!(registered.len(), 2);
}
