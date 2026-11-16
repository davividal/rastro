//! The `nfs` facet: which halves are here, and what each reads as.
//!
//! Every host interface is a scratch tree: the process table, `/sys/module`, the `nfsd`
//! filesystem and nfs-utils' state directory, laid out as Debian 12 lays them out.

mod support;

use std::path::{Path, PathBuf};

use rastro::collectors::nfs::{NfsCollector, NfsHost};
use rastro_collector::{ClaimedReading, Collector, Observation, Presence};
use support::fs_tree::{scratch_tree, write};
use support::observation::{field, is_null, keys_of, text};

const ETAB: &str = "/srv/nfs/a\t*(rw,sync,sec=sys,rw,secure,root_squash,no_all_squash)\n";

/// The four trees a collector reads, under one scratch root.
struct Host {
    root: PathBuf,
}

impl Host {
    fn empty(name: &str) -> Self {
        Self {
            root: scratch_tree(name, &["proc/1", "sys/module", "nfsd", "var/lib/nfs"]),
        }
    }

    fn with_process(self, process_id: u32, comm: &str) -> Self {
        write(
            &self.root,
            &format!("proc/{process_id}/comm"),
            &format!("{comm}\n"),
        );
        self
    }

    fn with_module(self, module: &str, parameters: &[(&str, &str)]) -> Self {
        std::fs::create_dir_all(self.root.join("sys/module").join(module)).expect("a scratch dir");
        for (name, value) in parameters {
            write(
                &self.root,
                &format!("sys/module/{module}/parameters/{name}"),
                &format!("{value}\n"),
            );
        }
        self
    }

    /// A started server: the module, its filesystem, and the table `exportfs` wrote.
    fn serving(self) -> Self {
        write(&self.root, "nfsd/threads", "8\n");
        write(&self.root, "nfsd/versions", "+3 +4 +4.1 +4.2\n");
        write(&self.root, "var/lib/nfs/etab", ETAB);
        self.with_module("nfsd", &[("nfs4_disable_idmapping", "N")])
    }

    fn path(&self, relative: &str) -> PathBuf {
        self.root.join(relative)
    }

    fn collector(&self) -> NfsCollector {
        NfsCollector::reading(NfsHost {
            proc: self.path("proc"),
            sys_module: self.path("sys/module"),
            nfsd: self.path("nfsd"),
            state_directory: self.path("var/lib/nfs"),
        })
    }

    fn facet(&self) -> Observation {
        self.collector().collect().expect("the facet collects")
    }
}

fn error_of(observation: &Observation) -> String {
    text(&field(observation, "error"))
}

#[test]
fn presence_is_absent_without_a_server_or_a_client() {
    // Arrange
    let host = Host::empty("nfs_absent").with_process(1, "systemd");

    // Act & Assert
    assert_eq!(host.collector().presence(), Presence::Absent);
}

#[test]
fn presence_is_undetermined_where_the_module_directory_is_missing() {
    // Arrange: no sysfs mounted, so whether a module is loaded cannot be told.
    let host = Host::empty("nfs_no_sysfs");
    std::fs::remove_dir(host.path("sys/module")).expect("an empty scratch dir");

    // Act & Assert
    assert!(matches!(
        host.collector().presence(),
        Presence::Undetermined { .. }
    ));
}

#[test]
fn collect_reads_a_started_server() {
    // Arrange
    let host = Host::empty("nfs_server").serving();

    // Act
    let facet = host.facet();

    // Assert: the client half is a null, a fact about the box rather than a gap.
    assert_eq!(host.collector().presence(), Presence::Present);
    assert!(is_null(&field(&facet, "client")));
    let server = field(&facet, "server");
    assert_eq!(keys_of(&field(&server, "exports")), vec!["/srv/nfs/a"]);
    assert_eq!(text(&field(&field(&server, "runtime"), "threads")), "8");
    assert_eq!(
        text(&field(
            &field(&server, "parameters"),
            "nfs4_disable_idmapping"
        )),
        "N"
    );
}

#[test]
fn collect_reads_a_client_and_names_the_modules_it_has_not_loaded() {
    // Arrange: `nfsv4` loads on the first v4 mount, so a box with only v3 mounts has `nfs` alone.
    let host = Host::empty("nfs_client").with_module("nfs", &[("enable_ino64", "Y")]);

    // Act
    let facet = host.facet();

    // Assert
    assert!(is_null(&field(&facet, "server")));
    let parameters = field(&field(&facet, "client"), "parameters");
    assert_eq!(
        text(&field(&field(&parameters, "nfs"), "enable_ino64")),
        "Y"
    );
    assert!(is_null(&field(&parameters, "nfsv4")));
}

#[test]
fn collect_reports_a_loaded_server_whose_filesystem_is_not_mounted_as_an_error() {
    // Arrange: rastro does not mount it, since mounting is a change to the host.
    let host = Host::empty("nfs_unmounted").with_module("nfsd", &[]);

    // Act
    let server = field(&host.facet(), "server");

    // Assert
    assert!(error_of(&server).contains("not mounted"), "got {server:?}");
    assert_eq!(server.incomplete_items(), 1);
}

#[test]
fn collect_refuses_a_userspace_server_rather_than_reporting_none() {
    // Arrange: nfs-ganesha serves NFS with no `nfsd` module and no `nfsd` filesystem, so the
    // kernel's interfaces alone would describe a server as no server.
    let host = Host::empty("nfs_ganesha").with_process(412, "ganesha.nfsd");

    // Act
    let server = field(&host.facet(), "server");

    // Assert
    assert_eq!(host.collector().presence(), Presence::Present);
    assert!(error_of(&server).contains("ganesha"), "got {server:?}");
}

#[test]
fn collect_reports_a_server_it_could_not_rule_out_as_an_error() {
    // Arrange: a process whose program could not be read, as `hidepid=1` leaves one to an
    // unprivileged run. Stood in for by a `comm` that is a directory, whoever runs this.
    let host = Host::empty("nfs_unseen").with_module("nfs", &[]);
    std::fs::create_dir_all(host.path("proc/77/comm")).expect("a scratch dir");

    // Act
    let server = field(&host.facet(), "server");

    // Assert: a null would say there is no server, which this run did not see.
    assert!(!is_null(&server));
    assert!(!error_of(&server).is_empty());
}

#[test]
fn collect_reports_a_missing_export_table_as_an_error_on_the_exports_alone() {
    // Arrange
    let host = Host::empty("nfs_no_etab").serving();
    std::fs::remove_file(host.path("var/lib/nfs/etab")).expect("a scratch file");

    // Act
    let server = field(&host.facet(), "server");

    // Assert: the runtime still reads.
    assert!(!error_of(&field(&server, "exports")).is_empty());
    assert_eq!(text(&field(&field(&server, "runtime"), "threads")), "8");
}

fn claims_of(host: &Host) -> Vec<(String, ClaimedReading)> {
    host.collector()
        .filesystem_claims()
        .iter()
        .map(|claim| (claim.tree().as_str().to_owned(), claim.reading()))
        .collect()
}

fn under(root: &Path, relative: &str) -> String {
    root.join(relative).to_string_lossy().into_owned()
}

#[test]
fn filesystem_claims_cover_the_state_that_moves_with_clients() {
    // Arrange: `nfsdcld/main.sqlite` was measured to change on a client's first connect, and
    // statd keeps a file per peer it watches.
    let host = Host::empty("nfs_claims");
    for directory in ["nfsdcld", "nfsdcltrack", "v4recovery", "sm", "sm.bak"] {
        std::fs::create_dir_all(host.path("var/lib/nfs").join(directory)).expect("a scratch dir");
    }
    write(&host.root, "var/lib/nfs/rmtab", "");

    // Act
    let claims = claims_of(&host);

    // Assert
    let state = host.path("var/lib/nfs");
    for sealed in ["nfsdcld", "nfsdcltrack", "sm", "sm.bak", "v4recovery"] {
        assert!(
            claims.contains(&(under(&state, sealed), ClaimedReading::Sealed)),
            "{sealed} in {claims:?}"
        );
    }
    assert!(claims.contains(&(under(&state, "rmtab"), ClaimedReading::Churns)));
}

#[test]
fn filesystem_claims_name_only_what_is_there() {
    // Arrange: a box that has never had nfs-utils.
    let host = Host::empty("nfs_claims_none");
    std::fs::remove_dir(host.path("var/lib/nfs")).expect("an empty scratch dir");

    // Act & Assert: a claim the host does not resolve is left unmade.
    assert!(claims_of(&host).is_empty());
}
