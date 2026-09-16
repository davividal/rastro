//! The running process of a containerd container.

use rastro_collector::{NonEmptyText, Observation};

/// A task, which is what containerd calls the process a container is currently running.
///
/// **The distinction this type exists for**: in containerd a container is a *definition* and
/// a task is it running. A container with no task is defined and not running, which is the
/// same fact docker reports as a status on the container itself. Absent means exactly that,
/// and it is why the task is optional rather than a status word that is sometimes empty.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerdTask {
    /// Volatile, like every pid: it changes whenever the process is replaced, and the
    /// `sockets` facet records a holder's pid the same way for the same reason.
    pub process_id: i64,
    /// containerd's own word: `RUNNING`, `STOPPED`, `PAUSED`, `CREATED`, `UNKNOWN`.
    pub status: NonEmptyText,
}

impl From<&ContainerdTask> for Observation {
    fn from(task: &ContainerdTask) -> Self {
        Observation::object([
            (
                "process_id",
                Observation::integer(task.process_id).volatile(),
            ),
            ("status", Observation::text(task.status.as_str())),
        ])
    }
}
