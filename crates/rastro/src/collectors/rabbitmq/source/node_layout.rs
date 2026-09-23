//! What a broker's own open files say about it: the name it runs under, and where its store
//! is.
//!
//! **Read, never composed.** A node's name is `local@host`, and rastro used to build it from
//! the register's local part and the box's hostname. That is a guess dressed as a reading: a
//! node under long names calls itself `rabbit@broker.example.test` and rastro would have said
//! `rabbit@broker`, keyed the facet on a name nothing answers to, and addressed the CLI with
//! it. The broker writes its own name into the paths it keeps open, so it can be read
//! instead.
//!
//! **The store cannot be found by looking for that name either**, which is the other half of
//! the same lesson. `RABBITMQ_MNESIA_DIR=/srv/rabbit-data` moves the store somewhere that
//! carries no node name at all, while `/srv/rabbit-data/quorum/rabbit@host/` still does, so a
//! rule that took the first node-named component sealed a subtree and left the rest of a live
//! message store in the walk.
//!
//! **One shape answers both, and it was measured on four layouts**: RabbitMQ 3.12.1 and
//! 4.0.5, each with the default store and with a relocated one.
//!
//! ```text
//! <store>/coordination/<node>/names.dets
//! <store>/quorum/<node>/00000001.wal
//! <store>/msg_stores/vhosts/<id>/msg_store_persistent/0.rdq
//! ```
//!
//! `coordination` and `quorum` are Ra's system directories, RabbitMQ puts them directly under
//! the data directory, and each holds one subdirectory named for the node. So the component
//! after the bucket is the node's own name, and everything before the bucket is the store
//! root, whatever either happens to be called.
//!
//! **No bucket, no answer.** A broker holding neither open is not one rastro can name or seal
//! from the outside, and it says so rather than falling back to a default path.

use std::fs;
use std::path::{Component, Path, PathBuf};

/// Ra's system directories, which sit directly under the data directory and hold one
/// subdirectory each, named for the node.
const BUCKETS: [&str; 2] = ["coordination", "quorum"];

/// The separator between a node's local name and its host.
const SEPARATOR: char = '@';

/// What a broker's open files say about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeLayout {
    /// The node's own name, exactly as the broker writes it: short or long, never composed.
    pub name: String,

    /// The directory the node keeps its store in.
    pub store: String,
}

/// What the broker processes say about the node the register calls `registered`.
///
/// `registered` is epmd's half of the name, the part before the `@`. It is what ties a
/// registration to the files a process holds, on a box where two brokers each have their own.
pub fn read(proc: &Path, process_ids: &[u32], registered: &str) -> Option<NodeLayout> {
    process_ids
        .iter()
        .flat_map(|process_id| descriptors_of(&proc.join(process_id.to_string()).join("fd")))
        .find_map(|target| layout_of(&target, registered))
}

/// Every path a process has open, as the kernel resolves them.
fn descriptors_of(descriptors: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(descriptors) else {
        return Vec::new();
    };

    entries
        .flatten()
        .filter_map(|entry| fs::read_link(entry.path()).ok())
        .collect()
}

/// The node and store one descriptor implies, where it is one of Ra's.
fn layout_of(target: &Path, registered: &str) -> Option<NodeLayout> {
    if !target.is_absolute() {
        return None;
    }

    let named = format!("{registered}{SEPARATOR}");
    let components: Vec<&str> = target
        .components()
        .filter_map(|component| match component {
            Component::Normal(name) => name.to_str(),
            _ => None,
        })
        .collect();

    for (index, component) in components.iter().enumerate() {
        let node = components.get(index + 1)?;

        if BUCKETS.contains(component) && node.starts_with(&named) {
            return Some(NodeLayout {
                name: (*node).to_owned(),
                store: store_before(target, index)?,
            });
        }
    }

    None
}

/// Everything before the bucket, which is the store root.
///
/// Rebuilt from the original path rather than from the filtered components, so a root that is
/// not `/` on some future host, or a path with a prefix component, cannot be flattened into
/// something that only looks right.
fn store_before(target: &Path, bucket: usize) -> Option<String> {
    let mut store = PathBuf::new();
    let mut seen = 0;

    for component in target.components() {
        if let Component::Normal(_) = component {
            if seen == bucket {
                return store.to_str().map(str::to_owned);
            }
            seen += 1;
        }

        store.push(component);
    }

    None
}
