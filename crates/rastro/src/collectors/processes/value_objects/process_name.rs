//! What a process calls itself.

use rastro_collector::{CollectionError, NonEmptyText, Observation};

/// A process's name, as the kernel records it.
///
/// Truncated to fifteen characters, because that is what `TASK_COMM_LEN` allows, so a box
/// running Prometheus exporters reports `node_exporter` in full and `systemd_exporte`
/// cut short. rastro records the truncation rather than guessing at the rest.
///
/// **Read from `/proc/<pid>/status` rather than from `/proc/<pid>/stat`, and that is the
/// one interesting decision in this collector's source.** The `stat` file puts the name in
/// parentheses as its second field, and the name may itself contain parentheses and
/// spaces: this very box runs a process whose name is `(sd-pam)`. Splitting `stat` on
/// whitespace therefore mis-slots every field after the name, and doing it correctly means
/// finding the *last* `)` in the line. The `status` file is `Name:\tvalue`, one field per
/// line, and the trap does not exist.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ProcessName(NonEmptyText);

/// What the kernel prefixes a workqueue thread with.
const WORKQUEUE_PREFIX: &str = "kworker/";

impl ProcessName {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(NonEmptyText::new(value, "process name")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    /// Whether this is a kernel workqueue thread, whose name is not an identity.
    ///
    /// **Measured, and it is the one process kind with no stable name at all.** The kernel
    /// *rewrites* a workqueue thread's name to whatever work item it is currently running,
    /// so two runs of rastro seconds apart on an idle box reported
    /// `kworker/0:3-cgroup_release` and then `kworker/0:3-events`, and
    /// `kworker/u4:2-events_unbound` and then `kworker/u4:2-flush-8:0`. The pool index moves
    /// too as the kernel grows and shrinks its pools.
    ///
    /// Because the name *is* the identity here, there is nothing to keep: a process whose
    /// name means "currently doing X" cannot be diffed. So the whole entry is annotated
    /// volatile rather than the name alone, which would leave a nameless process behind.
    /// Ten of the fifty-eight kernel threads on the development box are these.
    pub fn is_a_kernel_workqueue(&self) -> bool {
        self.as_str().starts_with(WORKQUEUE_PREFIX)
    }
}

impl From<&ProcessName> for Observation {
    fn from(name: &ProcessName) -> Self {
        Observation::text(name.as_str())
    }
}
