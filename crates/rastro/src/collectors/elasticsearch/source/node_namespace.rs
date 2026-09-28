//! The network namespace a node listens in, and doing work inside it.
//!
//! **A node in a container is read, because it runs on the server.** Its listener exists only in
//! its own network namespace, and with no published port the host's loopback has no route to it
//! at all. So a request to it is made from a thread that has joined the node's namespace through
//! `/proc/<pid>/ns/net`: the kernel's `setns`, which moves the calling thread and nothing else,
//! and changes nothing about the namespace joined. A node on the host takes the same path with
//! the join skipped, told apart by the namespace links naming one inode.
//!
//! The join is on a thread of its own that ends with the work, so the thread that joined is never
//! one of the collector pool's, and no later work can run in a namespace it did not ask for.
//!
//! It needs `CAP_SYS_ADMIN`, so an unprivileged run gets a refusal for a node in a container. The
//! call goes through `rustix`, whose `setns` is safe and pure Rust on Linux, so
//! `unsafe_code = "forbid"` still holds for the workspace's own code. See `docs/decisions.md`.

use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::thread;

use crate::collectors::elasticsearch::value_objects::Unread;

/// Where a process's network namespace is linked from, relative to its `/proc` entry.
const NETWORK_NAMESPACE: &str = "ns/net";

/// The network namespace a node listens in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeNamespace {
    path: PathBuf,
    ours: bool,
}

impl NodeNamespace {
    /// Reads a node's namespace through the box's `/proc`.
    pub fn of(process_id: u32) -> Result<Self, Unread> {
        Self::of_in(Path::new("/proc"), process_id)
    }

    /// The same through a process table the caller names, compared with its `self`.
    pub fn of_in(proc: &Path, process_id: u32) -> Result<Self, Unread> {
        let path = proc.join(process_id.to_string()).join(NETWORK_NAMESPACE);
        let theirs = fs::read_link(&path).map_err(|error| {
            Unread::new(format!(
                "{process_id}/{NETWORK_NAMESPACE} could not be read: {error}"
            ))
        })?;
        let ours = fs::read_link(proc.join("self").join(NETWORK_NAMESPACE)).map_err(|error| {
            Unread::new(format!(
                "rastro's own {NETWORK_NAMESPACE} could not be read: {error}"
            ))
        })?;

        Ok(Self {
            path,
            ours: theirs == ours,
        })
    }

    /// Whether the node listens in the namespace rastro runs in, which is a node on the host.
    pub fn is_ours(&self) -> bool {
        self.ours
    }

    /// Does `work` inside the node's namespace, and does not do it at all where that fails.
    pub fn run<Answer: Send>(
        &self,
        work: impl FnOnce() -> Answer + Send,
    ) -> Result<Answer, Unread> {
        if self.ours {
            return Ok(work());
        }

        let namespace = File::open(&self.path).map_err(|error| {
            Unread::new(format!(
                "the node's network namespace could not be opened: {error}"
            ))
        })?;

        thread::scope(|scope| {
            scope
                .spawn(move || {
                    join(&namespace)?;
                    Ok(work())
                })
                .join()
                .unwrap_or_else(|_| {
                    Err(Unread::new(
                        "the work inside the node's network namespace panicked",
                    ))
                })
        })
    }
}

#[cfg(target_os = "linux")]
fn join(namespace: &File) -> Result<(), Unread> {
    use std::os::fd::AsFd;

    use rustix::thread::{LinkNameSpaceType, move_into_link_name_space};

    move_into_link_name_space(namespace.as_fd(), Some(LinkNameSpaceType::Network)).map_err(
        |error| {
            Unread::new(format!(
                "joining the node's network namespace failed: {error}"
            ))
        },
    )
}

#[cfg(not(target_os = "linux"))]
fn join(_namespace: &File) -> Result<(), Unread> {
    Err(Unread::new("joining another network namespace needs Linux"))
}
