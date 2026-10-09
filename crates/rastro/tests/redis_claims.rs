//! The tree the walk is asked to step back from, found without asking the server anything.
//!
//! Claims are gathered before any collector runs, on the critical path of every run, so nothing
//! here connects. It does not need to: redis implements `dir` with `chdir`, and `CONFIG GET dir`
//! is the server's `getcwd`, so the directory it writes into is the process's working directory.

use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

use rastro::collectors::redis::{InstalledServers, RedisCollector};
use rastro_collector::{ClaimedReading, Collector, FilesystemClaim};

mod support;

use support::fake_redis::{proc_holding, server_pid};
use support::fs_tree::{scratch_tree, write};

/// A socket path standing for the server's key; nothing listens on it, because nothing asks.
const SOCKET: &str = "/run/redis/redis-server.sock";

fn claims_of(proc: &Path) -> Vec<FilesystemClaim> {
    RedisCollector::reading(InstalledServers::new([]), proc).filesystem_claims()
}

fn in_own_mount_namespace(proc: &Path) {
    let link = proc.join(server_pid()).join("ns/mnt");
    std::fs::remove_file(&link).expect("a removable fixture");
    symlink("mnt:[4026532999]", &link).expect("a writable scratch symlink");
}

fn with_working_directory(name: &str, directory: &Path) -> PathBuf {
    let proc = proc_holding(name, Path::new(SOCKET));
    symlink(directory, proc.join(server_pid()).join("cwd")).expect("a writable scratch symlink");

    proc
}

/// The paths the claims name, sorted.
fn trees_of(claims: &[FilesystemClaim]) -> Vec<String> {
    let mut trees: Vec<String> = claims
        .iter()
        .map(|claim| claim.tree().as_str().to_owned())
        .collect();
    trees.sort();
    trees
}

/// The server's own files in `directory`: its snapshot and its append-only directory.
fn own_files_in(directory: &Path) -> Vec<String> {
    let directory = directory
        .to_str()
        .expect("a UTF-8 path")
        .trim_end_matches('/');
    vec![
        format!("{directory}/appendonlydir"),
        format!("{directory}/dump.rdb"),
    ]
}

#[test]
fn a_servers_own_files_are_sealed_for_its_instance() {
    // Arrange: every attribute of them moves when the server saves, which the `save` rules
    // decide and nobody touching the box does.
    let data = scratch_tree("redis-claims-data", &["var/lib/redis"]).join("var/lib/redis");
    let proc = with_working_directory("redis-claims-sealed", &data);

    // Act
    let claims = claims_of(&proc);

    // Assert
    assert_eq!(trees_of(&claims), own_files_in(&data));
    for claim in &claims {
        assert_eq!(claim.reading(), ClaimedReading::Sealed);
        assert_eq!(
            claim.qualifier().map(|qualifier| qualifier.as_str()),
            Some(SOCKET)
        );
    }
}

#[test]
fn a_server_working_in_a_home_directory_seals_its_files_and_nothing_else() {
    // Arrange: measured as cell 19, a server started by hand from `/root` with `dir ./`. The home
    // is the operator's, and its keys and scripts are what a fingerprint is for.
    let home = scratch_tree("redis-claims-home", &["root"]).join("root");
    let proc = with_working_directory("redis-claims-home-proc", &home);

    // Act & Assert
    assert_eq!(trees_of(&claims_of(&proc)), own_files_in(&home));
}

#[test]
fn a_server_working_in_the_root_seals_its_files_and_not_the_walk() {
    // Arrange: `dir ./` in a server started from `/`, which a container or a hand start makes
    // easy.
    let proc = with_working_directory("redis-claims-root", Path::new("/"));

    // Act & Assert
    assert_eq!(trees_of(&claims_of(&proc)), own_files_in(Path::new("/")));
}

#[test]
fn a_server_whose_directory_cannot_be_read_claims_nothing() {
    // Arrange: an unprivileged run, which may not follow another account's `cwd`.
    let proc = proc_holding("redis-claims-unreadable", Path::new(SOCKET));

    // Act & Assert: the walk's default is the safe direction to be wrong in.
    assert!(claims_of(&proc).is_empty());
}

#[test]
fn a_servers_files_are_still_sealed_when_its_instance_cannot_name_the_claim() {
    // Arrange: a server keyed by its title, whose colon a claim qualifier may not hold.
    let data = scratch_tree("redis-claims-data-unnamed", &["data"]).join("data");
    let proc = with_working_directory("redis-claims-unnamed", &data);
    std::fs::remove_dir_all(proc.join(server_pid()).join("fd")).expect("a removable fixture");
    write(&proc, &format!("{}/fd", server_pid()), "");
    write(
        &proc,
        &format!("{}/cmdline", server_pid()),
        "/usr/bin/redis-server 127.0.0.1:6379\0",
    );

    // Act
    let claims = claims_of(&proc);

    // Assert: losing which instance asked costs a label; losing the claim costs the seal.
    assert_eq!(claims.len(), 2, "{claims:?}");
    assert!(claims.iter().all(|claim| claim.qualifier().is_none()));
}

#[test]
fn a_packaged_server_in_a_private_mount_namespace_seals_the_hosts_directory() {
    // Arrange: measured, the package's unit gives every server a mount namespace of its own
    // (`PrivateTmp=yes`, `ReadOnlyDirectories=/`), and its `dir` is still the host's directory.
    let data = scratch_tree("redis-claims-data-private", &["var/lib/redis"]).join("var/lib/redis");
    let proc = with_working_directory("redis-claims-private", &data);
    in_own_mount_namespace(&proc);

    // Act
    let claims = claims_of(&proc);

    // Assert
    assert_eq!(trees_of(&claims), own_files_in(&data));
}

#[test]
fn a_server_whose_directory_is_its_own_roots_and_not_the_hosts_claims_nothing() {
    // Arrange: measured, a redis in a container reports its working directory as `/data`, a path
    // in its own root; the host has a directory at that spelling, and it is not the server's.
    let host = scratch_tree("redis-claims-data-container", &["data"]).join("data");
    let proc = with_working_directory("redis-claims-container", &host);
    in_own_mount_namespace(&proc);
    let image = scratch_tree("redis-claims-image", &[]);
    std::fs::create_dir_all(image.join(host.strip_prefix("/").expect("an absolute path")))
        .expect("a scratch directory");
    let root = proc.join(server_pid()).join("root");
    std::fs::remove_file(&root).expect("a removable fixture");
    symlink(&image, &root).expect("a writable scratch symlink");

    // Act & Assert: the walk's default is the safe direction to be wrong in.
    assert!(claims_of(&proc).is_empty());
}
