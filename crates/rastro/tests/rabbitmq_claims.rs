//! The tree the walk is asked to step back from, resolved without asking the broker.
//!
//! Claims are gathered before any collector runs, sequentially, in the composition root, so
//! anything a claim needs is paid for on the critical path of every run. A `status` read
//! there costs ~310 ms, measured five times over. It is not needed: the broker holds its own
//! store open, so the path is in `/proc` beside the descriptors the attribution already
//! walks. Measured on a live node: ten descriptors under the store, and `cwd` at the mnesia
//! base.

use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

use rastro::collectors::canonical_tool::CanonicalTool;
use rastro::collectors::rabbitmq::{NodeInventory, RabbitmqCollector};
use rastro_collector::{ClaimedReading, Collector};

mod support;

use support::fs_tree::{scratch_tree, write};
use support::shim;

const REGISTER: &str = "epmd: up and running on port 4369 with data:\nname rabbit at port 25672\n";

const EPMD_ARGV: &str = "/usr/lib/erlang/erts-15.2.7/bin/epmd\0-daemon\0";
const BROKER_ARGV: &str = "/usr/lib/erlang/erts-15.2.7/bin/beam.smp\0-s\0rabbit\0boot\0";
const OTHER_ARGV: &str = "/usr/lib/erlang/erts-15.2.7/bin/beam.smp\0-s\0ejabberd\0boot\0";

/// A descriptor the broker really holds, from the measured box. The node name appears twice
/// in it, which is why the *first* component decides where the store root is.
const WAL: &str = "/var/lib/rabbitmq/mnesia/rabbit@box/quorum/rabbit@box/00000001.wal";

/// The store root that path implies.
const STORE: &str = "/var/lib/rabbitmq/mnesia/rabbit@box";

/// A box the test built, with descriptors pointing where the caller says.
struct Box_ {
    root: PathBuf,
    proc: PathBuf,
}

fn box_with(name: &str, processes: &[(&str, &str, &[&str])]) -> Box_ {
    let root = scratch_tree(name, &["bin"]);
    let proc = root.join("proc");
    fs::create_dir_all(proc.join("net")).expect("a writable scratch directory");

    for (pid, argv, descriptors) in processes {
        fs::create_dir_all(proc.join(pid).join("fd")).expect("a writable scratch directory");
        write(&proc, &format!("{pid}/cmdline"), argv);
        write(&proc, &format!("{pid}/comm"), "beam.smp\n");

        for (number, target) in descriptors.iter().enumerate() {
            symlink(target, proc.join(pid).join("fd").join(number.to_string()))
                .expect("a writable scratch symlink");
        }
    }

    Box_ { root, proc }
}

impl Box_ {
    fn epmd(&self, witness: Option<&Path>) -> CanonicalTool {
        let script = match witness {
            Some(path) => format!(
                "#!/bin/sh\ntouch {}\ncat <<'OUT'\n{REGISTER}OUT\n",
                path.display()
            ),
            None => format!("#!/bin/sh\ncat <<'OUT'\n{REGISTER}OUT\n"),
        };

        shim::executable(&self.root.join("bin"), "epmd", &script)
    }

    fn inventory(&self) -> NodeInventory {
        NodeInventory::using(self.epmd(None), Ok("box".to_owned())).in_proc(&self.proc)
    }
}

#[test]
fn the_store_is_resolved_from_a_descriptor_the_broker_holds_open() {
    // Arrange
    let host = box_with(
        "rabbitmq-claims-store",
        &[("748", EPMD_ARGV, &[]), ("966", BROKER_ARGV, &[WAL])],
    );

    // Act
    let resolved = host.inventory().store_directories();

    // Assert: the prefix up to the *first* component equal to the node name, because the
    // name appears again deeper in the path and the second occurrence would seal a subtree
    // of the store rather than the store.
    assert_eq!(resolved.len(), 1);
    assert_eq!(resolved[0].1, STORE);
    assert_eq!(resolved[0].0.as_str(), "rabbit@box");
}

#[test]
fn a_descriptor_held_by_something_that_is_not_a_broker_is_ignored() {
    // Arrange: an ejabberd whose own store happens to sit under a path carrying the name.
    let host = box_with(
        "rabbitmq-claims-foreign",
        &[("748", EPMD_ARGV, &[]), ("500", OTHER_ARGV, &[WAL])],
    );

    // Act & Assert: only a process that booted RabbitMQ says anything about a RabbitMQ
    // store, which is the same restraint the attribution applies.
    assert!(host.inventory().store_directories().is_empty());
}

#[test]
fn a_broker_holding_nothing_under_a_store_claims_nothing() {
    // Arrange: a node whose descriptors are all sockets and devices.
    let host = box_with(
        "rabbitmq-claims-nothing",
        &[
            ("748", EPMD_ARGV, &[]),
            ("966", BROKER_ARGV, &["socket:[787924]", "/dev/null"]),
        ],
    );

    // Act & Assert: no claim rather than a guessed path. The walk's own default is the safe
    // direction to be wrong in, and it is loud rather than silent.
    assert!(host.inventory().store_directories().is_empty());
}

#[test]
fn nothing_is_asked_where_the_port_mapper_is_not_resident() {
    // Arrange: a box with a broker process and no epmd, which is a state the register cannot
    // be read on.
    let host = box_with("rabbitmq-claims-silent", &[("966", BROKER_ARGV, &[WAL])]);
    let witness = host.root.join("asked");
    let inventory =
        NodeInventory::using(host.epmd(Some(&witness)), Ok("box".to_owned())).in_proc(&host.proc);

    // Act
    let resolved = inventory.store_directories();

    // Assert: the gate holds in the claim phase too, which is the phase that runs first.
    assert!(
        !witness.exists(),
        "the register was asked with no port mapper resident"
    );
    assert!(resolved.is_empty());
}

#[test]
fn the_collector_seals_the_store_and_says_which_node_asked() {
    // Arrange
    let host = box_with(
        "rabbitmq-claims-collector",
        &[("748", EPMD_ARGV, &[]), ("966", BROKER_ARGV, &[WAL])],
    );
    let collector = RabbitmqCollector::reading(None, Some(host.inventory()));

    // Act
    let claims = collector.filesystem_claims();

    // Assert: sealed rather than merely unhashed, and qualified, so a directory two nodes
    // point at says which two.
    assert_eq!(claims.len(), 1);
    assert_eq!(claims[0].tree().as_str(), STORE);
    assert_eq!(claims[0].reading(), ClaimedReading::Sealed);
    assert_eq!(
        claims[0].qualifier().map(|qualifier| qualifier.as_str()),
        Some("rabbit@box")
    );
}

#[test]
fn a_box_with_no_rabbitmq_claims_nothing() {
    // Act & Assert: a collector with no inventory at all, which is a box with no epmd
    // installed.
    assert!(
        RabbitmqCollector::reading(None, None)
            .filesystem_claims()
            .is_empty()
    );
}
