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

use support::fake_redis::proc_holding;
use support::fs_tree::{scratch_tree, write};

/// A socket path standing for the server's key; nothing listens on it, because nothing asks.
const SOCKET: &str = "/run/redis/redis-server.sock";

fn claims_of(proc: &Path) -> Vec<FilesystemClaim> {
    RedisCollector::reading(InstalledServers::new([]), proc).filesystem_claims()
}

fn with_working_directory(name: &str, directory: &Path) -> PathBuf {
    let proc = proc_holding(name, Path::new(SOCKET));
    symlink(directory, proc.join("412/cwd")).expect("a writable scratch symlink");

    proc
}

#[test]
fn a_servers_data_directory_is_sealed_for_its_instance() {
    // Arrange: every attribute under it moves when the server saves, which the `save` rules
    // decide and nobody touching the box does.
    let data = scratch_tree("redis-claims-data", &["var/lib/redis"]).join("var/lib/redis");
    let proc = with_working_directory("redis-claims-sealed", &data);

    // Act
    let claims = claims_of(&proc);

    // Assert
    assert_eq!(claims.len(), 1, "{claims:?}");
    assert_eq!(
        claims[0].tree().as_str(),
        data.to_str().expect("a UTF-8 path")
    );
    assert_eq!(claims[0].reading(), ClaimedReading::Sealed);
    assert_eq!(
        claims[0].qualifier().map(|qualifier| qualifier.as_str()),
        Some(SOCKET)
    );
}

#[test]
fn a_server_working_in_the_root_claims_nothing() {
    // Arrange: `dir ./` in a server started from `/`, which a container or a hand start makes
    // easy. Sealing it would seal the whole walk.
    let proc = with_working_directory("redis-claims-root", Path::new("/"));

    // Act & Assert
    assert!(claims_of(&proc).is_empty());
}

#[test]
fn a_server_whose_directory_cannot_be_read_claims_nothing() {
    // Arrange: an unprivileged run, which may not follow another account's `cwd`.
    let proc = proc_holding("redis-claims-unreadable", Path::new(SOCKET));

    // Act & Assert: the walk's default is the safe direction to be wrong in.
    assert!(claims_of(&proc).is_empty());
}

#[test]
fn a_directory_is_still_sealed_when_its_instance_cannot_name_the_claim() {
    // Arrange: a server keyed by its title, whose colon a claim qualifier may not hold.
    let data = scratch_tree("redis-claims-data-unnamed", &["data"]).join("data");
    let proc = with_working_directory("redis-claims-unnamed", &data);
    std::fs::remove_dir_all(proc.join("412/fd")).expect("a removable fixture");
    write(&proc, "412/fd", "");
    write(
        &proc,
        "412/cmdline",
        "/usr/bin/redis-server 127.0.0.1:6379\0",
    );

    // Act
    let claims = claims_of(&proc);

    // Assert: losing which instance asked costs a label; losing the claim costs the seal.
    assert_eq!(claims.len(), 1, "{claims:?}");
    assert!(claims[0].qualifier().is_none());
}
