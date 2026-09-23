//! Which node gets addressed, and which does not.
//!
//! The register names every Erlang node on the box. Addressing one that is not a broker would
//! make it log an authentication failure, which is a write to a box rastro was asked only to
//! read, so a node is asked only where a process that booted RabbitMQ holds the distribution
//! port epmd named for it. The shims here record having been executed, so the restraint is
//! asserted rather than reasoned about.

use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

use rastro::collectors::canonical_tool::CanonicalTool;
use rastro::collectors::rabbitmq::{BrokerClient, BrokerEvidence, NodeInventory};

mod support;

use support::fs_tree::{scratch_tree, write};
use support::shim;

const REGISTER: &str = "epmd: up and running on port 4369 with data:\nname rabbit at port 25672\n";

const EPMD_ARGV: &str = "/usr/lib/erlang/erts-15.2.7/bin/epmd\0-daemon\0";
const BROKER_ARGV: &str = "/usr/lib/erlang/erts-15.2.7/bin/beam.smp\0-s\0rabbit\0boot\0";
const OTHER_ARGV: &str = "/usr/lib/erlang/erts-15.2.7/bin/beam.smp\0-s\0ejabberd\0boot\0";

/// A descriptor every running broker holds: Ra's write-ahead log, under the store, in a
/// directory named for the node. It is where the node's own name is read from, so a fixture
/// without one describes a broker rastro cannot name and therefore will not address.
const WAL: &str = "/var/lib/rabbitmq/mnesia/rabbit@box/quorum/rabbit@box/00000001.wal";

/// `/proc/net/tcp` with one socket offered on 25672, whose inode the fixture hands to a
/// process below.
/// The same table with no rows, which is a readable answer that nothing is being offered.
const EMPTY_TCP: &str = "  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode\n";

const TCP: &str = "\
  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode
   0: 00000000:6448 00000000:0000 0A 00000000:00000000 00:00000000 00000000     0        0 787924 1 0000000000000000 100 0 0 10 0
";

const STATUS: &str = r#"{"rabbitmq_version":"4.0.5","erlang_version":"Erlang/OTP 27 [erts-15.2.7]",
  "os":"Linux","data_directory":"/var/lib/rabbitmq/mnesia/rabbit@box","config_files":[],
  "log_files":["/var/log/rabbitmq/rabbit@box.log"],"active_plugins":[],
  "listeners":[{"node":"rabbit@box","port":25672,"protocol":"clustering","interface":"[::]"}]}"#;

const FEATURE_FLAGS: &str = r#"[{"name":"khepri_db","state":"disabled"}]"#;

const DEFINITIONS: &str = r#"{"rabbitmq_version":"4.0.5","vhosts":[{"name":"/"}],
  "users":[{"name":"guest","tags":["administrator"],
  "hashing_algorithm":"rabbit_password_hashing_sha256","password_hash":"Z3Vlc3Q="}]}"#;

/// A box the test built: a `/proc` with the processes named, a socket table, and the node's
/// distribution socket held by whichever process the caller says holds it.
struct Box_ {
    root: PathBuf,
    proc: PathBuf,
}

fn box_with(name: &str, processes: &[(&str, &str)], holder: Option<&str>) -> Box_ {
    let root = scratch_tree(name, &["bin", "fixtures"]);
    let proc = root.join("proc");
    fs::create_dir_all(proc.join("net")).expect("a writable scratch directory");
    write(&proc, "net/tcp", TCP);

    for (pid, argv) in processes {
        fs::create_dir_all(proc.join(pid).join("fd")).expect("a writable scratch directory");
        write(&proc, &format!("{pid}/cmdline"), argv);
        write(&proc, &format!("{pid}/comm"), "beam.smp\n");
    }

    if let Some(pid) = holder {
        symlink("socket:[787924]", proc.join(pid).join("fd/14"))
            .expect("a writable scratch symlink");
    }

    // Every beam that booted RabbitMQ holds its store open, which is what names it.
    for (pid, argv) in processes {
        if argv.contains("rabbit") {
            symlink(WAL, proc.join(pid).join("fd/15")).expect("a writable scratch symlink");
        }
    }

    write(&root, "fixtures/status.json", STATUS);
    write(&root, "fixtures/definitions.json", DEFINITIONS);
    write(&root, "fixtures/feature_flags.json", FEATURE_FLAGS);

    Box_ { root, proc }
}

impl Box_ {
    fn epmd(&self) -> CanonicalTool {
        shim::executable(
            &self.root.join("bin"),
            "epmd",
            &format!("#!/bin/sh\ncat <<'OUT'\n{REGISTER}OUT\n"),
        )
    }

    /// A client that answers both reads from the fixtures and records that it ran.
    fn client(&self) -> BrokerClient {
        let script = format!(
            "#!/bin/sh\ntouch {witness}\nfor argument in \"$@\"; do\n\
             \x20 case $argument in\n\
             \x20   status) cat {root}/fixtures/status.json; exit 0 ;;\n\
             \x20   export_definitions) cat {root}/fixtures/definitions.json; exit 0 ;;\n\
             \x20   list_feature_flags) cat {root}/fixtures/feature_flags.json; exit 0 ;;\n\
             \x20 esac\ndone\nexit 64\n",
            witness = self.witness().display(),
            root = self.root.display(),
        );

        BrokerClient::using(shim::executable(
            &self.root.join("bin"),
            "rabbitmqctl",
            &script,
        ))
    }

    /// A client that refuses, for the case where a broker is there and cannot be read.
    fn refusing_client(&self) -> BrokerClient {
        BrokerClient::using(shim::executable(
            &self.root.join("bin"),
            "rabbitmqctl",
            "#!/bin/sh\necho 'Error: unable to perform an operation' >&2\nexit 69\n",
        ))
    }

    fn witness(&self) -> PathBuf {
        self.root.join("asked")
    }

    fn inventory(&self) -> NodeInventory {
        NodeInventory::using(self.epmd()).in_proc(&self.proc)
    }

    fn asked(&self) -> bool {
        self.witness().exists()
    }
}

#[test]
fn a_node_whose_port_a_broker_holds_is_asked() {
    // Arrange
    let host = box_with(
        "rabbitmq-asking-broker",
        &[("748", EPMD_ARGV), ("966", BROKER_ARGV)],
        Some("966"),
    );

    // Act
    let installation = host
        .inventory()
        .read(Some(&host.client()))
        .expect("the shims answer");

    // Assert
    assert!(host.asked());

    let node = installation.nodes().values().next().expect("one node");
    assert_eq!(node.evidence, BrokerEvidence::RabbitmqProcess);
    assert_eq!(
        node.status.as_ref().expect("a status").rabbitmq_version,
        "4.0.5"
    );
    assert!(
        node.definitions
            .as_ref()
            .expect("definitions")
            .users
            .contains_key("guest")
    );
}

#[test]
fn a_node_held_by_another_erlang_application_is_not_asked() {
    // Arrange: the register names it, a beam holds its port, and that beam booted ejabberd.
    let host = box_with(
        "rabbitmq-asking-foreign",
        &[("748", EPMD_ARGV), ("500", OTHER_ARGV)],
        Some("500"),
    );

    // Act
    let installation = host
        .inventory()
        .read(Some(&host.client()))
        .expect("a foreign node is not a failure");

    // Assert: the restraint that matters. Addressing it would have made somebody else's node
    // log an authentication failure.
    assert!(
        !host.asked(),
        "a node that no RabbitMQ process holds must not be addressed"
    );

    let node = installation.nodes().values().next().expect("one node");
    assert_eq!(node.evidence, BrokerEvidence::OtherApplication);
    assert_eq!(node.evidence.runs_rabbitmq(), Some(false));
    assert!(node.status.is_none());
    assert!(node.definitions.is_none());
}

#[test]
fn a_node_whose_holder_cannot_be_read_is_undetermined_rather_than_denied() {
    // Arrange: the port is offered and no descriptor of any process names its inode. That is
    // what an unprivileged run meets, and what a container with reduced capabilities meets
    // even as root. Found that way: a live broker reported `runs_rabbitmq: false`.
    let host = box_with("rabbitmq-asking-unheld", &[("748", EPMD_ARGV)], None);

    // Act
    let installation = host
        .inventory()
        .read(Some(&host.client()))
        .expect("an unattributable node is not a failure");

    // Assert: still not addressed, which is the safe behaviour, and now the document says
    // rastro could not tell rather than asserting the node is not a broker.
    assert!(!host.asked());

    let node = installation.nodes().values().next().expect("one node");
    assert_eq!(node.evidence, BrokerEvidence::HolderUnreadable);
    assert_eq!(node.evidence.runs_rabbitmq(), None);
}

#[test]
fn a_node_whose_port_nothing_offers_is_a_stale_registration() {
    // Arrange: epmd still names the node, and no socket in the table offers its port.
    let host = box_with("rabbitmq-asking-stale", &[("748", EPMD_ARGV)], None);
    fs::write(host.proc.join("net/tcp"), EMPTY_TCP).expect("a writable scratch table");

    // Act
    let installation = host
        .inventory()
        .read(Some(&host.client()))
        .expect("a stale registration is not a failure");

    // Assert: a confident negative, unlike the case above, because the table was read and
    // says nothing is listening there.
    assert!(!host.asked());

    let node = installation.nodes().values().next().expect("one node");
    assert_eq!(node.evidence, BrokerEvidence::NotOffered);
    assert_eq!(node.evidence.runs_rabbitmq(), Some(false));
}

#[test]
fn a_box_whose_socket_tables_cannot_be_read_says_so() {
    // Arrange
    let host = box_with("rabbitmq-asking-tableless", &[("748", EPMD_ARGV)], None);
    fs::remove_file(host.proc.join("net/tcp")).expect("a removable scratch table");

    // Act
    let installation = host
        .inventory()
        .read(Some(&host.client()))
        .expect("an unreadable table is not a failure of the facet");

    // Assert
    assert!(!host.asked());
    assert_eq!(
        installation
            .nodes()
            .values()
            .next()
            .expect("one node")
            .evidence,
        BrokerEvidence::TablesUnreadable
    );
}

#[test]
fn a_broker_that_will_not_answer_fails_the_facet() {
    // Arrange
    let host = box_with(
        "rabbitmq-asking-refused",
        &[("748", EPMD_ARGV), ("966", BROKER_ARGV)],
        Some("966"),
    );

    // Act & Assert: rastro can see a broker and cannot read it, which is the could-not-tell
    // case. Loud, because reporting the node as bare would read as a broker with nothing in
    // it.
    assert!(
        host.inventory()
            .read(Some(&host.refusing_client()))
            .is_err()
    );
}

#[test]
fn nothing_is_asked_where_no_client_is_installed() {
    // Arrange
    let host = box_with(
        "rabbitmq-asking-clientless",
        &[("748", EPMD_ARGV), ("966", BROKER_ARGV)],
        Some("966"),
    );

    // Act
    let installation = host
        .inventory()
        .read(None)
        .expect("no client is not a failure");

    // Assert: the register is still readable and still worth reporting, and a box with a
    // broker and no CLI tool is a real state rather than a failed look.
    let node = installation.nodes().values().next().expect("one node");
    assert_eq!(node.evidence, BrokerEvidence::RabbitmqProcess);
    assert!(node.status.is_none());
}

#[test]
fn the_name_rastro_read_is_the_one_the_node_answers_to() {
    // Arrange
    let host = box_with(
        "rabbitmq-asking-name",
        &[("748", EPMD_ARGV), ("966", BROKER_ARGV)],
        Some("966"),
    );

    // Act
    let installation = host
        .inventory()
        .read(Some(&host.client()))
        .expect("the shims answer");

    // Assert: the key is what the register calls the node, the name inside it is what rastro
    // read from the node's own files, and the status carries what the node says about itself.
    // All three agree here because nothing is composed: the middle one comes from the very
    // directory the broker writes into.
    let (registered, node) = installation.nodes().iter().next().expect("one node");
    assert_eq!(registered, "rabbit");
    assert_eq!(
        node.node_name.as_ref().expect("a name").as_str(),
        "rabbit@box"
    );
    assert_eq!(
        node.status
            .as_ref()
            .expect("a status")
            .reported_name
            .as_deref(),
        Some("rabbit@box")
    );
}

/// A guard on the fixtures: the shim dispatches on the subcommand, so a script that stopped
/// matching would silently answer nothing and every assertion above would read as absence.
#[test]
fn the_client_shim_answers_both_reads() {
    // Arrange
    let host = box_with("rabbitmq-asking-shim", &[("748", EPMD_ARGV)], None);
    let client = host.client();

    // Act & Assert
    assert!(client.tool().run(&["status"]).is_ok());
    assert!(client.tool().run(&["export_definitions"]).is_ok());
    assert!(Path::new(&host.witness()).exists());
}
