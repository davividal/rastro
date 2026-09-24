//! Which process holds which socket, from `/proc/<pid>/fd`.
//!
//! The socket tables name no process, so the holder has to be found the way `ss -p` finds
//! it: every open file descriptor on the box is a symlink, and one holding a socket points
//! at `socket:[<inode>]`. Joining that against the inode in a socket table gives the
//! holder, and it costs nothing but reads.
//!
//! **Grouped by program name, and the grouping is not this module's idea.** It is the
//! `sockets` facet's decision about its own document, recorded in `docs/decisions.md`: a
//! daemon that forks leaves parent and child holding one listening descriptor, and how many
//! of them exist at the moment `/proc` is scanned is a fact about that moment rather than
//! about the host. The shape is kept here because the walk is what produces it, and because
//! a caller that only wants to know *which* processes hold a socket should not have to
//! reassemble it.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use rastro_collector::ProcessName;

/// What a file descriptor pointing at a socket reads as.
const SOCKET_PREFIX: &str = "socket:[";

/// Where a process publishes its name, which is the same 15-character-truncated string
/// `ss` prints.
const COMM: &str = "comm";

/// One process's hold on a socket: which process, and on which descriptor.
///
/// Nameless, because the name is the key it sits under. Both of these move when a service
/// restarts, which is why the facet that renders them annotates them volatile and this
/// module does not care either way.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct HeldDescriptor {
    pub process_id: i64,
    pub file_descriptor: i64,
}

/// Every socket inode on the box, and the programs holding it open, each with its own
/// processes.
///
/// **Built in one pass rather than searched per socket.** There are a few hundred sockets
/// and a few thousand descriptors, so asking the question once per socket would walk
/// `/proc` a few hundred times. The pass costs about 100 ms on a 94-process box, measured,
/// against 7 ms for the two `ss` invocations it replaces, and it is the difference between
/// reading the host and changing it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SocketHolders {
    by_inode: BTreeMap<u64, BTreeMap<ProcessName, BTreeSet<HeldDescriptor>>>,
    complete: bool,
}

impl SocketHolders {
    /// The holders on a `/proc` the caller names.
    ///
    /// **Every failure here is expected and skipped, and the refusals are remembered.** A
    /// process may exit between being listed and being read, and an unprivileged run cannot
    /// open another user's descriptors at all. Neither fails the read: it is the same partial
    /// view `ss -p` gives, and failing a facet over it would make an unprivileged run report
    /// nothing rather than less. A process that exited took its sockets with it; one whose
    /// descriptors were refused may hold any socket nothing else was found holding, which
    /// [`Self::is_complete`] lets a caller say.
    pub fn at(proc: impl AsRef<Path>) -> Self {
        let mut holders: BTreeMap<u64, BTreeMap<ProcessName, BTreeSet<HeldDescriptor>>> =
            BTreeMap::new();
        let mut complete = true;

        let Ok(entries) = fs::read_dir(proc.as_ref()) else {
            return Self {
                by_inode: holders,
                complete: false,
            };
        };

        for entry in entries.flatten() {
            let path = entry.path();
            let Some(process_id) = process_id_of(&path) else {
                continue;
            };
            let name = match name_of(&path) {
                Ok(Some(name)) => name,
                Ok(None) => continue,
                // Gone between the listing and the read, which takes its sockets with it.
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                // `hidepid` shows the pid and refuses its files: its sockets are unattributable.
                Err(_) => {
                    complete = false;
                    continue;
                }
            };

            let held = match sockets_of(&path.join("fd")) {
                Ok(held) => held,
                // Gone between the listing and the read, which takes its sockets with it.
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(_) => {
                    complete = false;
                    continue;
                }
            };

            for (inode, file_descriptor) in held {
                holders
                    .entry(inode)
                    .or_default()
                    .entry(name.clone())
                    .or_default()
                    .insert(HeldDescriptor {
                        process_id,
                        file_descriptor,
                    });
            }
        }

        Self {
            by_inode: holders,
            complete,
        }
    }

    /// Whether every process's descriptors could be listed, so that a socket no holder was
    /// found for is one nothing visible holds rather than one this run could not attribute.
    pub fn is_complete(&self) -> bool {
        self.complete
    }

    /// The programs holding one socket, each with the processes of it that do.
    ///
    /// An empty answer is a real one rather than a failure: a socket whose holder exited
    /// between the two reads, or one held by a process an unprivileged run cannot see.
    pub fn of(&self, inode: u64) -> BTreeMap<ProcessName, BTreeSet<HeldDescriptor>> {
        self.by_inode.get(&inode).cloned().unwrap_or_default()
    }

    /// The processes holding one socket, whatever they are called.
    ///
    /// For a caller asking *which* process holds a socket rather than what to print about
    /// it: the `rabbitmq` facet joins this against the processes that booted a broker, to
    /// decide whether the node behind a distribution port may be addressed at all.
    pub fn process_ids_of(&self, inode: u64) -> BTreeSet<i64> {
        self.by_inode
            .get(&inode)
            .into_iter()
            .flat_map(BTreeMap::values)
            .flatten()
            .map(|held| held.process_id)
            .collect()
    }
}

/// The pid of a `/proc` entry, or `None` for the many entries that are not processes.
fn process_id_of(path: &Path) -> Option<i64> {
    path.file_name()?.to_str()?.parse::<i64>().ok()
}

/// A process's name, as the kernel truncates it.
/// The process's name, nothing where it is not one, or the read's own failure.
fn name_of(path: &Path) -> std::io::Result<Option<ProcessName>> {
    let comm = fs::read_to_string(path.join(COMM))?;

    Ok(ProcessName::new(comm.trim()).ok())
}

/// Every socket one process holds, as inode and descriptor number.
fn sockets_of(descriptors: &Path) -> std::io::Result<Vec<(u64, i64)>> {
    Ok(fs::read_dir(descriptors)?
        .flatten()
        .filter_map(|entry| {
            let file_descriptor = entry.file_name().to_str()?.parse::<i64>().ok()?;
            let target = fs::read_link(entry.path()).ok()?;
            let inode = inode_of(target.to_str()?)?;

            Some((inode, file_descriptor))
        })
        .collect())
}

/// The inode inside a `socket:[12345]` link target.
fn inode_of(target: &str) -> Option<u64> {
    target
        .strip_prefix(SOCKET_PREFIX)?
        .strip_suffix(']')?
        .parse::<u64>()
        .ok()
}
