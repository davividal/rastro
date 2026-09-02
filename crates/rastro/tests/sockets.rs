//! Reading what the host is listening on, over a `/proc` tree the test owns.
//!
//! Every fixture row is a real row from the development box. The whole facet is assembled
//! here rather than parsed in pieces, because the join from a socket to the process holding
//! it runs across two interfaces — `/proc/net/*` for the socket and `/proc/<pid>/fd` for
//! the holder — and neither half is worth much without the other.

mod support;

use std::os::unix::fs::symlink;

use rastro::collectors::sockets::{
    InetHost, ListeningSocket, ProcNet, SocketAddress, SocketTable, SocketsCollector,
};
use rastro_collector::{Collector, Presence};
use rastro_fingerprint::{Content, Observation, Scalar, View};
use support::fs_tree::{scratch_tree, write};
use support::observation::{field, items_of, object_of};

const TCP: &str = "\
  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode
   0: 00000000:0016 00000000:0000 0A 00000000:00000000 00:00000000 00000000     0        0 45643 1 00000000ee7a3cea 100 0 0 10 0
   1: 0100007F:1538 00000000:0000 0A 00000000:00000000 00:00000000 00000000   105        0 66633 1 000000006d675ddf 100 0 0 10 0
";

const TCP6: &str = "\
   0: 00000000000000000000000001000000:0019 00000000000000000000000000000000:0000 0A 00000000:00000000 00:00000000 00000000     0        0 22348 1 00000000f53bcb72 100 0 0 10 20
   1: 00000000000000000000000000000000:238F 00000000000000000000000000000000:0000 0A 00000000:00000000 00:00000000 00000000     0        0 96835 1 00000000b8b804c7 100 0 0 10 0
";

const UDP: &str = "\
15006: 3600007F:0035 00000000:0000 07 00000000:00000000 00:00000000 00000000   996        0 43874 2 000000001b2ef305 0
";

const UDP6: &str = "\
15499: 000080FE00000000FF27000ADD9CA0FE:0222 00000000000000000000000000000000:0000 07 00000000:00000000 00:00000000 00000000   998        0 146930 2 00000000706425d8 0
";

/// A stream socket held by two processes, a sequenced-packet socket held by none that the
/// fixture admits to, and an abstract socket.
const UNIX: &str = "\
Num       RefCount Protocol Flags    Type St Inode Path
000000004bfa98b1: 00000002 00000000 00010000 0001 01 11955 /run/systemd/journal/stdout
00000000256667e3: 00000002 00000000 00010000 0005 01 11958 /run/udev/control
0000000033f7d9c3: 00000002 00000000 00000000 0002 01 22346 @/var/spool/exim4/exim_daemon_notify
";

/// One descriptor a process holds open, and the socket it points at.
struct HeldSocket {
    descriptor: &'static str,
    inode: u64,
}

/// One process, and every socket descriptor it holds.
struct Holder {
    process_id: &'static str,
    name: &'static str,
    sockets: &'static [HeldSocket],
}

/// The processes holding those sockets, as `/proc/<pid>/fd` presents them.
///
/// The last entry holds nothing and gets no `fd` directory at all, which is what rastro
/// meets when a process exits between being listed and being read.
const HOLDERS: [Holder; 6] = [
    Holder {
        process_id: "1",
        name: "systemd",
        sockets: &[HeldSocket {
            descriptor: "117",
            inode: 11955,
        }],
    },
    Holder {
        process_id: "3989",
        name: "systemd-journal",
        sockets: &[HeldSocket {
            descriptor: "5",
            inode: 11955,
        }],
    },
    Holder {
        process_id: "24169",
        name: "sshd",
        sockets: &[HeldSocket {
            descriptor: "3",
            inode: 45643,
        }],
    },
    Holder {
        process_id: "44012",
        name: "postgres",
        sockets: &[HeldSocket {
            descriptor: "7",
            inode: 66633,
        }],
    },
    Holder {
        process_id: "2636",
        name: "exim4",
        sockets: &[
            HeldSocket {
                descriptor: "3",
                inode: 22346,
            },
            HeldSocket {
                descriptor: "4",
                inode: 22348,
            },
        ],
    },
    Holder {
        process_id: "9999",
        name: "gone",
        sockets: &[],
    },
];

fn source(name: &str) -> ProcNet {
    let root = scratch_tree(name, &["net", "proc"]);

    for (file, contents) in [
        ("net/tcp", TCP),
        ("net/tcp6", TCP6),
        ("net/udp", UDP),
        ("net/udp6", UDP6),
        ("net/unix", UNIX),
    ] {
        write(&root, file, contents);
    }

    for holder in HOLDERS {
        let process_id = holder.process_id;
        write(
            &root,
            &format!("proc/{process_id}/comm"),
            &format!("{}\n", holder.name),
        );
        for held in holder.sockets {
            let link = root.join(format!("proc/{process_id}/fd/{}", held.descriptor));
            std::fs::create_dir_all(link.parent().expect("a parent")).expect("a writable tree");
            // A dangling link on purpose: `socket:[N]` is what the kernel writes, and it
            // resolves to nothing on any filesystem. `read_link` never follows it.
            symlink(format!("socket:[{}]", held.inode), &link).expect("a writable tree");
        }
    }

    ProcNet::at(root.join("net"), root.join("proc"))
}

fn table(name: &str) -> SocketTable {
    source(name).read().expect("these fixtures are real rows")
}

fn bound_to(table: &SocketTable, host: &str, port: u16) -> ListeningSocket {
    table
        .sockets()
        .iter()
        .find(|socket| match &socket.address {
            SocketAddress::Inet {
                host: found,
                port: bound,
            } => found.as_str() == host && bound.as_u16() == port,
            SocketAddress::Local { .. } => false,
        })
        .unwrap_or_else(|| panic!("expected a socket bound to {host}:{port}"))
        .clone()
}

fn at_path(table: &SocketTable, path: &str) -> ListeningSocket {
    table
        .sockets()
        .iter()
        .find(|socket| match &socket.address {
            SocketAddress::Local { path: found } => found.as_str() == path,
            SocketAddress::Inet { .. } => false,
        })
        .unwrap_or_else(|| panic!("expected a socket at {path}"))
        .clone()
}

fn names_of(socket: &ListeningSocket) -> Vec<&str> {
    socket
        .processes
        .iter()
        .map(|process| process.name.as_str())
        .collect()
}

#[test]
fn a_socket_is_joined_to_the_process_holding_it() {
    // Arrange: `/proc/net/tcp` names no process at all. The inode in its last read column
    // is the only route to one, through every open descriptor on the box.

    // Act
    let ssh = bound_to(&table("sockets_holder"), "0.0.0.0", 22);

    // Assert
    assert_eq!(ssh.kind.as_str(), "tcp");
    assert_eq!(ssh.state.as_str(), "LISTEN");
    assert_eq!(names_of(&ssh), ["sshd"]);
}

#[test]
fn a_socket_held_by_more_than_one_process_names_both() {
    // Arrange: `/run/systemd/journal/stdout` really is held by both on the development box.

    // Act
    let journal = at_path(&table("sockets_two_holders"), "/run/systemd/journal/stdout");

    // Assert: sorted, so the order the descriptors happened to be walked in never reaches
    // the document.
    assert_eq!(names_of(&journal), ["systemd", "systemd-journal"]);
}

#[test]
fn a_socket_no_visible_process_holds_is_reported_without_one() {
    // Arrange: the holder exited between the two reads, or an unprivileged run cannot open
    // that process's descriptors. `ss -p` gives the same partial view under the same
    // conditions, and a socket rastro cannot attribute is still a port the box has open.

    // Act
    let udev = at_path(&table("sockets_no_holder"), "/run/udev/control");

    // Assert
    assert!(udev.processes.is_empty());
}

#[test]
fn both_families_land_in_one_table() {
    // Act
    let table = table("sockets_families");

    // Assert: two TCP, two TCP over IPv6, one UDP, one UDP over IPv6, three unix.
    assert_eq!(table.len(), 9);
}

#[test]
fn a_wildcard_is_told_apart_from_a_loopback_binding() {
    // Act: the difference between a service reachable from the network and one reachable
    // only from the box, which is the whole point of the facet.
    let table = table("sockets_wildcard");

    // Assert
    assert!(
        bound_to(&table, "0.0.0.0", 22)
            .address
            .ne(&bound_to(&table, "127.0.0.1", 5432).address)
    );
    for host in ["0.0.0.0", "::"] {
        assert!(
            InetHost::new(host).expect("legal").is_a_wildcard(),
            "{host} reaches the network"
        );
    }
    assert!(!InetHost::new("127.0.0.1").expect("legal").is_a_wildcard());
}

#[test]
fn an_abstract_unix_socket_is_kept() {
    // Act: an abstract socket has a name in no filesystem, so a filesystem walk cannot see
    // it and this facet is the only place it appears.
    let socket = at_path(
        &table("sockets_abstract"),
        "@/var/spool/exim4/exim_daemon_notify",
    );

    // Assert
    match &socket.address {
        SocketAddress::Local { path } => assert!(path.is_abstract()),
        other => panic!("expected a local address, got {other:?}"),
    }
}

#[test]
fn the_sockets_are_sorted() {
    // Act: the tables are walked in the order the kernel keeps them, which depends on
    // which sockets were opened when.
    let table = table("sockets_sorted");

    // Assert
    let mut sorted = table.sockets().to_vec();
    sorted.sort();
    assert_eq!(table.sockets(), sorted.as_slice());
}

#[test]
fn a_missing_address_family_is_state_rather_than_a_failure() {
    // Arrange: a kernel with IPv6 disabled has no `/proc/net/tcp6`, and it is listening on
    // no IPv6 socket. Failing the facet over that would report a box with no exposure at
    // all.
    let root = scratch_tree("sockets_no_ipv6", &["net", "proc"]);
    write(&root, "net/tcp", TCP);
    write(&root, "net/unix", UNIX);

    // Act
    let table = ProcNet::at(root.join("net"), root.join("proc"))
        .read()
        .expect("a kernel without IPv6 is not a failure");

    // Assert
    assert_eq!(table.len(), 5);
}

#[test]
fn a_process_id_is_volatile_and_its_name_is_not() {
    // Act: a pid changes every time a service restarts. `postgres` no longer holding 5432
    // is a change; `postgres` holding it under a new pid is not.
    let observation = Observation::from(&table("sockets_volatile"));
    let diffable = observation
        .in_view(View::Diffable)
        .expect("the facet survives the diffable view");

    // Assert
    let holders = items_of(&field(&items_of(&diffable)[0], "processes"));
    let keys: Vec<String> = object_of(&holders[0])
        .into_iter()
        .map(|(key, _)| key)
        .collect();
    assert_eq!(keys, ["name"]);
}

#[test]
fn an_address_renders_the_same_keys_for_either_family() {
    // Act
    let observation = Observation::from(&table("sockets_keys"));

    // Assert: a consumer never meets a key that is sometimes absent, and which family it
    // is stays readable from which keys are null. `scope` is gone from this list because
    // no `/proc` column carries it, and a key that is always null would assert rastro
    // looked.
    for socket in items_of(&observation) {
        let address = field(&socket, "address");
        let keys: Vec<String> = object_of(&address)
            .into_iter()
            .map(|(key, _)| key)
            .collect();
        assert_eq!(keys, ["host", "path", "port"]);
    }
}

#[test]
fn a_unix_socket_renders_a_null_port() {
    // Act
    let observation = Observation::from(&table("sockets_null_port"));
    let local = items_of(&observation)
        .into_iter()
        .find(|socket| {
            matches!(
                field(&field(socket, "address"), "path").content(),
                Content::Scalar(Scalar::Text(_))
            )
        })
        .expect("a unix socket");

    // Assert
    assert_eq!(
        field(&field(&local, "address"), "port").content(),
        &Content::Scalar(Scalar::Null)
    );
}

#[test]
fn presence_is_undetermined_without_a_procfs_rather_than_absent() {
    // Act: a box rastro cannot read `/proc/net` on has not stopped listening on anything,
    // so `absent` would be a confident lie about the box's exposure.
    let presence = SocketsCollector::reading(None).presence();

    // Assert
    match presence {
        Presence::Undetermined { reason } => assert!(
            reason.contains("cannot be told"),
            "the reason must say rastro could not see, got: {reason}"
        ),
        other => panic!("expected an undetermined presence, got {other:?}"),
    }
}

#[test]
fn presence_is_present_when_the_tables_are_there() {
    // Act & Assert
    assert_eq!(
        SocketsCollector::reading(Some(source("sockets_presence"))).presence(),
        Presence::Present
    );
}

#[test]
fn collect_fails_rather_than_reporting_an_empty_table_without_a_procfs() {
    // Act & Assert
    assert!(SocketsCollector::reading(None).collect().is_err());
}

#[test]
fn a_table_that_exists_but_cannot_be_read_fails_rather_than_reporting_nothing() {
    // Arrange: a missing table is a kernel without that family, which is state. A table
    // that is there and unreadable is not, and reporting an empty socket list for it would
    // describe a box as listening on nothing.
    let root = scratch_tree("sockets_unreadable", &["net", "proc"]);
    std::fs::create_dir_all(root.join("net/unix")).expect("a writable tree");

    // Act
    let result = ProcNet::at(root.join("net"), root.join("proc")).read();

    // Assert
    let failure = result.expect_err("an unreadable table must not read as an empty one");
    assert!(
        failure.to_string().contains("could not read"),
        "the message must name the failure, got: {failure}"
    );
}

#[test]
fn a_process_tree_rastro_cannot_read_leaves_the_sockets_unattributed() {
    // Arrange: an unprivileged run cannot open other users' descriptors, and a socket rastro
    // cannot attribute is still a port the box has open. Losing the whole facet over it
    // would trade a complete answer for no answer.
    let root = scratch_tree("sockets_no_proc", &["net"]);
    write(&root, "net/tcp", TCP);
    write(&root, "net/unix", UNIX);

    // Act
    let table = ProcNet::at(root.join("net"), root.join("nothing-here"))
        .read()
        .expect("an unreadable process tree is not a failure");

    // Assert
    assert_eq!(table.len(), 5);
    assert!(
        table
            .sockets()
            .iter()
            .all(|socket| socket.processes.is_empty())
    );
}
