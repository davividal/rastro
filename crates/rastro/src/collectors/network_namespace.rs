//! The network namespace a process listens in, and doing work inside it.
//!
//! Shared, because two collectors read a service in a container: its listener exists only in its
//! own network namespace, and with no published port the host has no route to it at all. So the
//! connection is made from a thread that has joined the process's namespace through
//! `/proc/<pid>/ns/net`: the kernel's `setns`, which moves the calling thread and nothing else, and
//! changes nothing about the namespace joined. A process on the host takes the same path with the
//! join skipped, told apart by the namespace links naming one inode. Found first by elasticsearch.
//!
//! The join is on a thread of its own that ends with the work, so the thread that joined is never
//! one of the collector pool's, and no later work can run in a namespace it did not ask for. A
//! socket keeps the namespace it was created in, so only the connection needs the join.
//!
//! It needs `CAP_SYS_ADMIN`, so an unprivileged run is refused. The call goes through `rustix`,
//! whose `setns` is safe and pure Rust on Linux, so `unsafe_code = "forbid"` still holds for the
//! workspace's own code. See `docs/decisions.md`.

use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::thread;

/// Where a process's network namespace is linked from, relative to its `/proc` entry.
const NETWORK_NAMESPACE: &str = "ns/net";

/// Why work could not be done inside a process's namespace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamespaceRefusal {
    pub reason: String,

    /// Whether the box refused rastro, an unprivileged run, rather than something failing.
    pub refused: bool,
}

impl NamespaceRefusal {
    fn failed(reason: String) -> Self {
        Self {
            reason,
            refused: false,
        }
    }
}

/// The network namespace a process listens in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessNamespace {
    path: PathBuf,
    ours: bool,
}

impl ProcessNamespace {
    /// A process's namespace through a process table the caller names, compared with its `self`.
    pub fn of_in(proc: &Path, process_id: u32) -> Result<Self, NamespaceRefusal> {
        let path = proc.join(process_id.to_string()).join(NETWORK_NAMESPACE);
        let theirs = fs::read_link(&path).map_err(|error| NamespaceRefusal {
            reason: format!("the process's {NETWORK_NAMESPACE} could not be read: {error}"),
            refused: error.kind() == std::io::ErrorKind::PermissionDenied,
        })?;
        let ours = fs::read_link(proc.join("self").join(NETWORK_NAMESPACE)).map_err(|error| {
            NamespaceRefusal::failed(format!(
                "rastro's own {NETWORK_NAMESPACE} could not be read: {error}"
            ))
        })?;

        Ok(Self {
            path,
            ours: theirs == ours,
        })
    }

    /// The namespace rastro runs in, for a process known to be on the host.
    pub fn ours() -> Self {
        Self {
            path: PathBuf::new(),
            ours: true,
        }
    }

    /// Whether the process listens in the namespace rastro runs in, which is a process on the
    /// host.
    pub fn is_ours(&self) -> bool {
        self.ours
    }

    /// Does `work` inside the process's namespace, and does not do it at all where that fails.
    pub fn run<Answer: Send>(
        &self,
        work: impl FnOnce() -> Answer + Send,
    ) -> Result<Answer, NamespaceRefusal> {
        if self.ours {
            return Ok(work());
        }

        let namespace = File::open(&self.path).map_err(|error| NamespaceRefusal {
            reason: format!("the process's network namespace could not be opened: {error}"),
            refused: error.kind() == std::io::ErrorKind::PermissionDenied,
        })?;

        thread::scope(|scope| {
            scope
                .spawn(move || {
                    join(&namespace)?;
                    Ok(work())
                })
                .join()
                .unwrap_or_else(|_| {
                    Err(NamespaceRefusal::failed(
                        "the work inside the process's network namespace panicked".to_owned(),
                    ))
                })
        })
    }
}

#[cfg(target_os = "linux")]
fn join(namespace: &File) -> Result<(), NamespaceRefusal> {
    use std::os::fd::AsFd;

    use rustix::io::Errno;
    use rustix::thread::{LinkNameSpaceType, move_into_link_name_space};

    // Refused, it is the box keeping rastro out: an unprivileged run has no `CAP_SYS_ADMIN`.
    move_into_link_name_space(namespace.as_fd(), Some(LinkNameSpaceType::Network)).map_err(
        |error| NamespaceRefusal {
            reason: format!("joining the process's network namespace failed: {error}"),
            refused: matches!(error, Errno::PERM | Errno::ACCESS),
        },
    )
}

#[cfg(not(target_os = "linux"))]
fn join(_namespace: &File) -> Result<(), NamespaceRefusal> {
    Err(NamespaceRefusal::failed(
        "joining another network namespace needs Linux".to_owned(),
    ))
}
