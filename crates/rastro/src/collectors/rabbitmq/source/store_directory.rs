//! Where a node keeps its store, read from the descriptors it holds open.
//!
//! **The claim phase cannot afford to ask the broker.** Claims are gathered before any
//! collector runs, sequentially, in the composition root, so whatever a claim needs is paid
//! on the critical path of every run. A `status` read there costs 303 to 326 ms, measured
//! five times on a live node, and it is an Erlang VM boot.
//!
//! It is also unnecessary. A running broker holds its own store open, so the path is already
//! in `/proc` beside the descriptors the attribution walks: ten of them on the measured node,
//! the shallowest being a quorum queue's write-ahead log at
//! `/var/lib/rabbitmq/mnesia/rabbit@<node>/quorum/rabbit@<node>/00000001.wal`.
//!
//! **Resolved, not assumed**, which is the whole reason this is worth a module.
//! `/var/lib/rabbitmq/mnesia/<node>` is Debian's default and not a rule: `RABBITMQ_MNESIA_DIR`
//! moves it, and the postgres entry records what a claim over an assumed default costs, which
//! is sealing a directory that holds nothing while hashing the one that holds the data. The
//! usual escape is shut here too, measured: the broker's beam carries **no environment
//! variables at all**, so those variables cannot be read off the process.

use std::fs;
use std::path::{Component, Path, PathBuf};

use crate::collectors::rabbitmq::value_objects::NodeName;

/// The node's store, as the prefix of a path it holds open.
///
/// **The first component equal to the node name decides it**, and that is not fussiness: the
/// name appears twice in a quorum queue's path, `mnesia/rabbit@box/quorum/rabbit@box/…`, so
/// taking the last occurrence would seal a subtree of the store and leave the rest of it in
/// the walk.
pub fn under(proc: &Path, process_ids: &[u32], node: &NodeName) -> Option<String> {
    process_ids
        .iter()
        .flat_map(|process_id| descriptors_of(&proc.join(process_id.to_string()).join("fd")))
        .find_map(|target| store_root_of(&target, node))
}

/// Every path a process has open, as the kernel resolves them.
///
/// A descriptor that is not a path at all, a socket or a pipe, resolves to a string that is
/// not absolute and is skipped by the caller's own test rather than filtered here.
fn descriptors_of(descriptors: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(descriptors) else {
        return Vec::new();
    };

    entries
        .flatten()
        .filter_map(|entry| fs::read_link(entry.path()).ok())
        .collect()
}

/// The prefix of `target` up to and including the first component named for the node.
fn store_root_of(target: &Path, node: &NodeName) -> Option<String> {
    if !target.is_absolute() {
        return None;
    }

    let mut root = PathBuf::new();
    for component in target.components() {
        root.push(component);

        if matches!(component, Component::Normal(name) if name == node.as_str()) {
            return root.to_str().map(str::to_owned);
        }
    }

    None
}
