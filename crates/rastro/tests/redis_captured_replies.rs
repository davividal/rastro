//! Every captured server's replies, read through the whole facet.
//!
//! A fake server replays what each real one sent to rastro's fixed list of commands, byte for byte,
//! so what is tested is the reading of every release and both families as they really answer. One
//! case per release and per shape of `docs/redis-matrix.md`: a cell whose server answers exactly as
//! another release's does keeps no `CONFIG GET` of its own.

use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};

use rastro::collectors::redis::{InstalledServers, RedisCollector};
use rastro_collector::{Collector, Observation};

mod support;

use support::captured_redis_cell::servers_of;
use support::fake_redis::{FakeRedis, key_of};
use support::observation::{field, is_null, keys_of, text};

/// How many servers this binary has replayed, which numbers each one's socket.
static REPLAYED: AtomicUsize = AtomicUsize::new(0);

/// The command each captured reply file answers, from its name.
fn command_of(reply: &Path) -> String {
    let name = reply
        .file_stem()
        .and_then(std::ffi::OsStr::to_str)
        .expect("a name");
    let (_, command) = name.split_once('-').expect("a numbered reply");

    command.replace('-', " ").replace("star", "*")
}

/// The instance the facet reports for the cell's server at `index`, replayed.
fn replayed(cell: &str, index: usize) -> Observation {
    let server = &servers_of(cell)[index];
    let script: Vec<(String, String)> = fs::read_dir(server.join("replies/authenticated"))
        .expect("captured replies")
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| !path.to_string_lossy().ends_with("-AUTH.resp"))
        .map(|path| {
            let reply = String::from_utf8(fs::read(&path).expect("a reply")).expect("UTF-8");
            (command_of(&path), reply)
        })
        .collect();
    let borrowed: Vec<(&str, &str)> = script
        .iter()
        .map(|(command, reply)| (command.as_str(), reply.as_str()))
        .collect();

    // Named after the test too: two tests replaying one server would remove each other's tree.
    let test = std::thread::current()
        .name()
        .unwrap_or("unnamed")
        .replace("::", "-");
    // The socket's own name short: a unix socket path is capped at about a hundred bytes.
    let socket = REPLAYED.fetch_add(1, Ordering::Relaxed);
    let fake = FakeRedis::answering(&format!("rp-{}-{socket}", std::process::id()), &borrowed);
    let proc = fake.proc(&format!("redis-replayed-{cell}-{index}-{test}"));
    let observation = RedisCollector::reading(InstalledServers::new([]), &proc)
        .collect()
        .expect("a readable box");
    let instances = field(&observation, "instances");
    assert_eq!(keys_of(&instances), [key_of(&fake)]);

    field(&instances, &key_of(&fake))
}

/// A cell whose server is read in full: a version, every setting it answered, and no error.
macro_rules! read_in_full {
    ($name:ident, $cell:literal, $index:literal, $version:literal) => {
        #[test]
        fn $name() {
            // Act
            let instance = replayed($cell, $index);

            // Assert
            assert!(is_null(&field(&instance, "error")), "{instance:?}");
            assert_eq!(text(&field(&instance, "version")), $version);
            assert!(!keys_of(&field(&instance, "settings")).is_empty());
            assert!(!is_null(&field(&instance, "replication")));
            assert!(!is_null(&field(&instance, "modules")));
        }
    };
}

read_in_full!(cell_01_redis_8_10, "01", 0, "8.10.2");
read_in_full!(cell_02_redis_8_0, "02", 0, "8.0.6");
read_in_full!(cell_03_redis_8_2, "03", 0, "8.2.10");
read_in_full!(cell_05_redis_8_6_rewritten, "05", 0, "8.6.7");
read_in_full!(cell_08_redis_8_8_with_an_acl_file, "08", 0, "8.8.3");
read_in_full!(cell_12_the_queue_instance, "12", 0, "8.10.2");
read_in_full!(cell_12_the_cache_replica, "12", 1, "8.10.2");
read_in_full!(cell_13_redis_8_10_in_cluster_mode, "13", 0, "8.10.2");
read_in_full!(cell_17_redis_8_10_with_a_runtime_module, "17", 0, "8.10.2");
read_in_full!(cell_26_valkey_9_1, "26", 0, "9.1.2");
read_in_full!(cell_27_valkey_9_0, "27", 0, "9.0.6");
read_in_full!(cell_28_valkey_8_1, "28", 0, "8.1.10");
read_in_full!(cell_29_valkey_8_0, "29", 0, "8.0.11");
read_in_full!(cell_30_valkey_7_2, "30", 0, "7.2.14");
read_in_full!(cell_32_redis_7_0, "32", 0, "7.0.15");
read_in_full!(cell_33_redis_7_4, "33", 0, "7.4.11");
read_in_full!(cell_34_redis_6_2, "34", 0, "6.2.24");

#[test]
fn cell_12_the_replica_names_its_master() {
    // Act
    let replication = field(&replayed("12", 1), "replication");

    // Assert
    assert_eq!(text(&field(&replication, "role")), "slave");
    assert!(!is_null(&field(&replication, "master")));
}

#[test]
fn cell_14_config_renamed_away_costs_the_settings_alone() {
    // Act
    let instance = replayed("14", 0);

    // Assert
    assert!(is_null(&field(&instance, "settings")));
    assert!(text(&field(&instance, "not_read")).contains("CONFIG"));
    assert!(is_null(&field(&instance, "error")));
    assert!(!is_null(&field(&instance, "modules")));
}

#[test]
fn cell_35_redis_5_has_no_accounts_and_is_not_asked_for_them() {
    // Act
    let instance = replayed("35", 0);

    // Assert: absent without an error, a server older than accounts.
    assert_eq!(text(&field(&instance, "version")), "5.0.14");
    assert!(is_null(&field(&instance, "acl")));
    assert!(is_null(&field(&instance, "error")), "{instance:?}");
}
