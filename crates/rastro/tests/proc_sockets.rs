//! The two questions `/proc` answers about a socket, asked of a tree a test built.
//!
//! Shared rather than per collector: `sockets` needs to know who holds every socket on the
//! box, and `rabbitmq` needs to know whether the process holding one particular port is the
//! broker it is about to address. One descriptor walk serves both, and a second copy of it
//! would be a second thing to be right about.

use std::fs;
use std::os::unix::fs::symlink;
use std::path::Path;

use rastro::collectors::proc_sockets::{SocketHolders, listening_inodes};

mod support;

use support::fs_tree::{scratch_tree, write};

/// `/proc/net/tcp` as the kernel writes it, from the box the RabbitMQ footprint was measured
/// on. Row 0 is the broker's distribution listener on port 25672 (`6448`); row 1 is a
/// connection *to* that port, whose local port is something else entirely and whose inode is
/// `0`, and which must not be mistaken for the listener.
const TCP: &str = "\
  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode
   0: 00000000:6448 00000000:0000 0A 00000000:00000000 00:00000000 00000000     0        0 787924 1 0000000000000000 100 0 0 10 0
   1: 0B00580A:9966 0B00580A:6448 06 00000000:00000000 00:00000000 00000000     0        0 0 1 0000000000000000 100 0 0 10 0
   2: 00000000:1628 00000000:0000 0A 00000000:00000000 00:00000000 00000000     0        0 787925 1 0000000000000000 100 0 0 10 0
";

/// `/proc/net/tcp6`, where a dual-stack wildcard listener actually appears: the AMQP port,
/// `1628` being 5672, is bound on `[::]`.
const TCP6: &str = "\
  sl  local_address                         remote_address                        st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode
   0: 00000000000000000000000000000000:1628 00000000000000000000000000000000:0000 0A 00000000:00000000 00:00000000 00000000     0        0 787930 1 0000000000000000 100 0 0 10 0
";

/// An established connection whose *local* port is the listener's, which is what a client
/// talking to the broker looks like. It must not be reported as a listener.
const TCP_WITH_A_CLIENT: &str = "\
  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode
   0: 00000000:6448 00000000:0000 0A 00000000:00000000 00:00000000 00000000     0        0 787924 1 0000000000000000 100 0 0 10 0
   1: 0100007F:6448 0100007F:9966 01 00000000:00000000 00:00000000 00000000     0        0 787999 1 0000000000000000 100 0 0 10 0
";

/// A `/proc/net` holding the tables named.
fn net_with(name: &str, tables: &[(&str, &str)]) -> std::path::PathBuf {
    let root = scratch_tree(name, &["net"]);
    for (table, text) in tables {
        write(&root, &format!("net/{table}"), text);
    }

    root.join("net")
}

#[test]
fn listening_inodes_finds_the_socket_offered_on_a_port() {
    // Arrange
    let net = net_with("proc-sockets-listener", &[("tcp", TCP)]);

    // Act
    let found = listening_inodes(&net, 25672);

    // Assert
    assert_eq!(found, [787924].into_iter().collect());
}

#[test]
fn listening_inodes_ignores_a_connection_to_that_port() {
    // Act
    let found = listening_inodes(&net_with("proc-sockets-remote", &[("tcp", TCP)]), 39270);

    // Assert: the remote column of row 1 holds 6448 and its local column does not, so a
    // reader that looked at both would attribute the listener to a peer's ephemeral port.
    assert!(found.is_empty());
}

#[test]
fn listening_inodes_ignores_an_established_connection_on_the_same_local_port() {
    // Arrange
    let net = net_with("proc-sockets-client", &[("tcp", TCP_WITH_A_CLIENT)]);

    // Act
    let found = listening_inodes(&net, 25672);

    // Assert: a client talking to the broker has the broker's port as its *local* port too,
    // and only the listening row says which socket is the one being offered.
    assert_eq!(found, [787924].into_iter().collect());
}

#[test]
fn listening_inodes_reads_the_ipv6_table_as_well() {
    // Arrange
    let net = net_with("proc-sockets-six", &[("tcp", TCP), ("tcp6", TCP6)]);

    // Act
    let found = listening_inodes(&net, 5672);

    // Assert: a dual-stack wildcard listener appears in `tcp6` alone, so a reader of `tcp`
    // only would report the broker's AMQP port as unheld.
    assert_eq!(found, [787925, 787930].into_iter().collect());
}

#[test]
fn listening_inodes_treats_a_missing_table_as_an_empty_one() {
    // Act & Assert: a kernel with IPv6 disabled has no `tcp6`, which is state rather than
    // failure, and a caller asking about a port gets the honest answer that nothing in the
    // table it could read offers it.
    assert!(listening_inodes(Path::new("/nonexistent/net"), 25672).is_empty());
}

#[test]
fn socket_holders_finds_the_process_holding_an_inode() {
    // Arrange: a descriptor is a symlink reading `socket:[<inode>]`, which is the only thread
    // between a socket table and a process.
    let proc = scratch_tree("proc-sockets-holder", &["966/fd"]);
    write(&proc, "966/comm", "beam.smp\n");
    symlink("socket:[787924]", proc.join("966/fd/14")).expect("a writable scratch symlink");
    symlink("/var/lib/rabbitmq", proc.join("966/fd/15")).expect("a writable scratch symlink");

    // Act
    let holders = SocketHolders::at(&proc);
    let holding = holders.of(787924);

    // Assert: grouped by program name, which is the `sockets` facet's decision about its own
    // document and the shape the walk therefore produces.
    assert_eq!(holding.len(), 1);
    let (name, processes) = holding.iter().next().expect("one holder");
    assert_eq!(name.as_str(), "beam.smp");
    assert_eq!(processes.len(), 1);

    let held = processes.iter().next().expect("one process");
    assert_eq!(held.process_id, 966);
    assert_eq!(held.file_descriptor, 14);

    // And the question the `rabbitmq` facet asks of the same walk: which processes, never
    // mind what they are called.
    assert_eq!(holders.process_ids_of(787924), [966].into_iter().collect());
}

#[test]
fn socket_holders_groups_every_process_of_one_program_under_its_name() {
    // Arrange: a daemon that forked, so parent and child hold the same listening descriptor.
    let proc = scratch_tree("proc-sockets-forked", &["966/fd", "967/fd"]);
    write(&proc, "966/comm", "nginx\n");
    write(&proc, "967/comm", "nginx\n");
    symlink("socket:[787924]", proc.join("966/fd/6")).expect("a writable scratch symlink");
    symlink("socket:[787924]", proc.join("967/fd/6")).expect("a writable scratch symlink");

    // Act
    let holders = SocketHolders::at(&proc);

    // Assert: one holder, two processes under it, and both pids reachable for a caller that
    // wants the processes rather than the rendering.
    assert_eq!(holders.of(787924).len(), 1);
    assert_eq!(
        holders.process_ids_of(787924),
        [966, 967].into_iter().collect()
    );
}

#[test]
fn socket_holders_reports_nothing_for_a_socket_nobody_holds() {
    // Arrange
    let proc = scratch_tree("proc-sockets-unheld", &["966/fd"]);
    write(&proc, "966/comm", "beam.smp\n");

    // Act & Assert: a socket whose holder exited between the two reads, or one held by a
    // process an unprivileged run cannot see. A real answer rather than a failure, which is
    // the same partial view `ss -p` gives under the same conditions.
    assert!(SocketHolders::at(&proc).of(787924).is_empty());
}

#[test]
fn socket_holders_skips_a_process_whose_descriptors_will_not_be_read() {
    // Arrange: an unreadable descriptor directory is what an unprivileged run meets on
    // somebody else's process, and what a container meets on every process it did not start.
    let proc = scratch_tree("proc-sockets-denied", &["966/fd", "312/fd"]);
    write(&proc, "966/comm", "beam.smp\n");
    write(&proc, "312/comm", "postgres\n");
    symlink("socket:[787924]", proc.join("966/fd/14")).expect("a writable scratch symlink");
    fs::set_permissions(proc.join("312/fd"), unreadable())
        .expect("a scratch directory whose mode can be set");

    // Act
    let holders = SocketHolders::at(&proc);

    // The mode goes back before the assertion, because a directory nobody can read is a
    // directory this test's own next run cannot delete, and the failure it leaves behind
    // looks like a bug in the walk rather than in the fixture.
    fs::set_permissions(proc.join("312/fd"), readable()).expect("a restorable scratch mode");

    // Assert: the readable process is still reported, which is the difference between a
    // partial answer and no answer at all.
    assert_eq!(holders.process_ids_of(787924), [966].into_iter().collect());
}

/// A mode with no read or search bit for anybody, which is how this test makes a directory
/// refuse. Root ignores it, so the assertion above is about the walk not failing rather than
/// about the denial itself, and the container suite runs both ways.
fn unreadable() -> fs::Permissions {
    use std::os::unix::fs::PermissionsExt;

    fs::Permissions::from_mode(0o000)
}

/// The mode the fixture goes back to, so the tree can be removed again.
fn readable() -> fs::Permissions {
    use std::os::unix::fs::PermissionsExt;

    fs::Permissions::from_mode(0o700)
}
