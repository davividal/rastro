//! Where a running node keeps its data and logs, from the files it holds open.
//!
//! **Effective state, whatever started the node.** Measured on every cell of the matrix: a node
//! holds `node.lock` open in each data directory, and its server log, or the JVM's `gc.log` where
//! it logs to stdout, in its logs directory. Its settings can name other directories: an `-E`
//! that left with the launcher of a node started with `-d`, or a file changed since start. The
//! files it has open are the ones it uses, so they decide what is sealed where they can be read.

use std::fs;
use std::path::{Path, PathBuf};

use crate::collectors::elasticsearch::value_objects::Release;

/// The file a node locks in each data directory.
const DATA_LOCK: &str = "node.lock";

/// The directory 7.x and earlier keep the lock in, `<path.data>/nodes/<ordinal>/node.lock`.
const NODES_DIRECTORY: &str = "nodes";

/// The server log every packaged and archive node writes, `<cluster>_server.json`.
const SERVER_LOG_SUFFIX: &str = "_server.json";

/// The JVM's own log, which `jvm.options` puts in the logs directory on every distribution.
const GC_LOG: &str = "gc.log";

/// A node's data and logs directories, as paths in its own mount namespace.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HeldStore {
    pub data: Vec<PathBuf>,
    pub logs: Vec<PathBuf>,
}

impl HeldStore {
    /// What the node holds open, or nothing where its descriptors cannot be listed or show no
    /// data lock, which leaves the decision to its settings.
    ///
    /// `release` decides the lock's layout: below 8 it is under `nodes/<ordinal>`.
    pub fn of_in(proc: &Path, process_id: u32, release: Option<Release>) -> Option<Self> {
        let nested = release.is_some_and(|release| release.major() < 8);
        let descriptors = fs::read_dir(proc.join(process_id.to_string()).join("fd")).ok()?;
        let mut held = Self::default();

        for target in descriptors
            .flatten()
            .filter_map(|descriptor| fs::read_link(descriptor.path()).ok())
        {
            if let Some(directory) = data_directory_of(&target, nested) {
                held.data.push(directory);
            } else if is_a_log(&target)
                && let Some(directory) = target.parent()
            {
                held.logs.push(directory.to_path_buf());
            }
        }

        held.data.sort();
        held.data.dedup();
        held.logs.sort();
        held.logs.dedup();
        (!held.data.is_empty()).then_some(held)
    }
}

/// The data directory a held `node.lock` is in: its directory on 8.x and 9.x, and the one above
/// `nodes/<ordinal>` below 8, where the ordinal is a number, as all 7.x writes.
///
/// Found by review: an 8.x data path that is itself `…/nodes/0` read as 7.x's layout and sealed a
/// tree two levels above the node's own.
fn data_directory_of(target: &Path, nested: bool) -> Option<PathBuf> {
    if target.file_name()? != DATA_LOCK {
        return None;
    }
    let directory = target.parent()?;
    let is_an_ordinal = directory
        .file_name()
        .and_then(std::ffi::OsStr::to_str)
        .is_some_and(|name| !name.is_empty() && name.bytes().all(|byte| byte.is_ascii_digit()));
    let above_ordinal = directory.parent()?;
    let under_nodes = above_ordinal
        .file_name()
        .is_some_and(|name| name == NODES_DIRECTORY);

    match nested && is_an_ordinal && under_nodes {
        true => above_ordinal.parent().map(Path::to_path_buf),
        false => Some(directory.to_path_buf()),
    }
}

fn is_a_log(target: &Path) -> bool {
    target
        .file_name()
        .and_then(std::ffi::OsStr::to_str)
        .is_some_and(|name| name == GC_LOG || name.ends_with(SERVER_LOG_SUFFIX))
}
