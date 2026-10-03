//! The tree the walk is asked to step back from: a node's data directory.
//!
//! Sealed, for the reason the PostgreSQL and RabbitMQ stores are: measured on 7.17, a node with
//! no request at all moved every index's translog checkpoint and retention-lease file within
//! ninety seconds, so a walk of it cannot be byte-identical across two runs of an unchanged box.
//!
//! **What decides a claim is whether the node's data path is the same directory on the host**,
//! not whether the node shares rastro's mount namespace. Found by review: Elastic's own systemd
//! unit sets `PrivateTmp=true`, on 7.17 and 8.15 alike, so every packaged node has a mount
//! namespace of its own and still keeps its data in the host's `/var/lib/elasticsearch`. A node
//! in a container names a directory in its own image, which is not the host's.

use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

use rastro::collectors::elasticsearch::{ElasticsearchCollector, HttpClient};
use rastro_collector::{ClaimedReading, Collector};

mod support;

use support::fs_tree::{scratch_tree, write};

/// A box with one node on it, its config and data under a scratch tree the test owns.
struct Box_ {
    proc: PathBuf,
    scratch: PathBuf,
}

impl Box_ {
    /// A node whose root is the host's, its mount namespace the same as rastro's or not.
    fn host_node(name: &str, own_mount_namespace: bool, config_file: &str) -> Self {
        let scratch = scratch_tree(name, &["proc/self/ns", "proc/600/ns", "conf", "home"]);
        let proc = scratch.join("proc");
        let argv = format!(
            "/usr/share/elasticsearch/jdk/bin/java\0-Des.path.home={home}\0\
             -Des.path.conf={conf}\0-Des.distribution.type=tar\0\
             -cp\0lib/*\0org.elasticsearch.bootstrap.Elasticsearch\0",
            home = scratch.join("home").display(),
            conf = scratch.join("conf").display(),
        );
        write(&proc, "600/cmdline", &argv);
        write(&proc, "600/environ", "");
        support::process::started(&proc, "600", "1", 5);
        symlink("/", proc.join("600/root")).expect("a writable fixture");
        symlink("mnt:[4026531841]", proc.join("self/ns/mnt")).expect("a writable fixture");
        let theirs = match own_mount_namespace {
            true => "mnt:[4026532777]",
            false => "mnt:[4026531841]",
        };
        symlink(theirs, proc.join("600/ns/mnt")).expect("a writable fixture");
        write(&scratch, "conf/elasticsearch.yml", config_file);

        Self { proc, scratch }
    }

    fn path(&self, relative: &str) -> PathBuf {
        self.scratch.join(relative)
    }

    fn directory(&self, relative: &str) -> PathBuf {
        let path = self.path(relative);
        fs::create_dir_all(&path).expect("a writable fixture");
        path
    }

    fn claimed_trees(&self) -> Vec<String> {
        claimed_trees(&self.proc)
    }
}

fn claimed_trees(proc: &Path) -> Vec<String> {
    ElasticsearchCollector::reading(proc, false, HttpClient::new())
        .filesystem_claims()
        .iter()
        .inspect(|claim| assert_eq!(claim.reading(), ClaimedReading::Sealed))
        .map(|claim| claim.tree().as_str().to_owned())
        .collect()
}

fn data_setting(paths: &[&Path]) -> String {
    let listed: Vec<String> = paths
        .iter()
        .map(|path| path.display().to_string())
        .collect();
    format!("path.data: [{}]\n", listed.join(", "))
}

#[test]
fn filesystem_claims_seal_the_data_directory_the_settings_name() {
    // Arrange: a package install sets it in `elasticsearch.yml`.
    let host = Box_::host_node("elasticsearch-claims-configured", false, "");
    let data = host.directory("var-lib-elasticsearch");
    write(
        &host.scratch,
        "conf/elasticsearch.yml",
        &data_setting(&[&data]),
    );

    // Act & Assert
    assert_eq!(host.claimed_trees(), [data.display().to_string()]);
}

#[test]
fn filesystem_claims_seal_a_packaged_nodes_data_in_its_own_mount_namespace() {
    // Arrange: the packaged unit's `PrivateTmp=true` gives the node a mount namespace of its own,
    // and its data directory is still the host's.
    let host = Box_::host_node("elasticsearch-claims-private-tmp", true, "");
    let data = host.directory("var-lib-elasticsearch");
    write(
        &host.scratch,
        "conf/elasticsearch.yml",
        &data_setting(&[&data]),
    );

    // Act & Assert
    assert_eq!(host.claimed_trees(), [data.display().to_string()]);
}

#[test]
fn filesystem_claims_seal_every_directory_of_a_multi_path_node() {
    // Arrange: deprecated since 7.13 and still accepted.
    let host = Box_::host_node("elasticsearch-claims-several", false, "");
    let first = host.directory("es-a");
    let second = host.directory("es-b");
    write(
        &host.scratch,
        "conf/elasticsearch.yml",
        &data_setting(&[&first, &second]),
    );

    // Act & Assert
    assert_eq!(
        host.claimed_trees(),
        [first.display().to_string(), second.display().to_string()]
    );
}

#[test]
fn filesystem_claims_seal_the_default_under_the_home_where_nothing_names_one() {
    // Arrange: a tarball install, whose data directory is `data` under its home.
    let host = Box_::host_node("elasticsearch-claims-default", false, "");
    let data = host.directory("home/data");

    // Act & Assert
    assert_eq!(host.claimed_trees(), [data.display().to_string()]);
}

#[test]
fn filesystem_claims_resolve_a_relative_path_against_the_home() {
    // Arrange
    let host = Box_::host_node(
        "elasticsearch-claims-relative",
        false,
        "path.data: storage\n",
    );
    let data = host.directory("home/storage");

    // Act & Assert
    assert_eq!(host.claimed_trees(), [data.display().to_string()]);
}

#[test]
fn filesystem_claims_make_no_claim_for_a_directory_the_host_does_not_have() {
    // Arrange: a path nothing on the host answers to is not one to seal on a guess.
    let host = Box_::host_node("elasticsearch-claims-missing", false, "");
    let missing = host.path("not-created");
    write(
        &host.scratch,
        "conf/elasticsearch.yml",
        &data_setting(&[&missing]),
    );

    // Act & Assert
    assert!(host.claimed_trees().is_empty());
}

#[test]
fn filesystem_claims_leave_a_container_nodes_data_alone() {
    // Arrange: a root of the node's own, holding the same paths as the host's and different
    // directories at them, which is what a container's image is.
    let host = Box_::host_node("elasticsearch-claims-container", true, "");
    let data = host.directory("var-lib-elasticsearch");
    let setting = data_setting(&[&data]);
    let image = host.directory("image");
    fs::remove_file(host.proc.join("600/root")).expect("the fixture's link");
    symlink(&image, host.proc.join("600/root")).expect("a writable fixture");
    let inside = |path: &Path| image.join(path.strip_prefix("/").expect("an absolute path"));
    fs::create_dir_all(inside(&data)).expect("a writable fixture");
    let config = inside(&host.path("conf"));
    fs::create_dir_all(&config).expect("a writable fixture");
    fs::write(config.join("elasticsearch.yml"), setting).expect("a writable fixture");

    // Act & Assert
    assert!(host.claimed_trees().is_empty());
}

#[test]
fn filesystem_claims_make_no_claim_where_the_settings_cannot_be_read() {
    // Arrange: the walk's own default is the safe direction to be wrong in.
    let host = Box_::host_node(
        "elasticsearch-claims-unread",
        false,
        "path.data: ${ES_DATA}\n",
    );

    // Act & Assert
    assert!(host.claimed_trees().is_empty());
}

#[test]
fn filesystem_claims_seal_the_directory_a_symlinked_data_path_leads_to() {
    // Arrange: found by review. `/var/lib/elasticsearch` linking to `/mnt/es` passed the identity
    // check, and the claim named the link, which the walk matches as text: it would have walked
    // into `/mnt/es`, the live store itself.
    let host = Box_::host_node("elasticsearch-claims-symlinked-data", false, "");
    let real = host.directory("mnt-es");
    let link = host.path("var-lib-elasticsearch");
    symlink(&real, &link).expect("a writable fixture");
    write(
        &host.scratch,
        "conf/elasticsearch.yml",
        &data_setting(&[&link]),
    );

    // Act & Assert
    let canonical = fs::canonicalize(&real).expect("the real directory");
    assert_eq!(host.claimed_trees(), [canonical.display().to_string()]);
}

#[test]
fn filesystem_claims_seal_a_data_path_spelled_with_dot_dot_as_the_directory_it_is() {
    // Arrange
    let host = Box_::host_node("elasticsearch-claims-dot-dot", false, "");
    let real = host.directory("data");
    host.directory("home");
    let spelled = host.path("home/../data");
    write(
        &host.scratch,
        "conf/elasticsearch.yml",
        &data_setting(&[&spelled]),
    );

    // Act & Assert
    let canonical = fs::canonicalize(&real).expect("the real directory");
    assert_eq!(host.claimed_trees(), [canonical.display().to_string()]);
}
