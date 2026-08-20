//! One process.

use rastro_collector::{AbsolutePath, Observation};

use crate::collectors::processes::value_objects::{
    CommandLine, ControlGroup, ProcessId, ProcessName, ProcessState,
};

/// A process as rastro means it.
///
/// **The stable half and the volatile half are deliberately mixed in one type**, and which
/// is which is the whole design of this facet. What a process *is* — its name, its
/// arguments, the account it runs as, the unit it belongs to, the binary behind it — does
/// not change while it lives, and a steady-state box reports the same set of those between
/// two runs. Its pid, its parent's pid, its state and its thread count all move on their
/// own.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Process {
    pub name: ProcessName,
    /// Empty for a kernel thread, which is how one is told apart from userspace.
    pub command_line: CommandLine,
    pub user_id: u32,
    pub group_id: u32,
    /// Absent on a host with no cgroup hierarchy, which a container can be.
    pub control_group: Option<ControlGroup>,
    /// Absent for a kernel thread, which has no binary, and for a process whose executable
    /// was deleted while it ran.
    pub executable: Option<AbsolutePath>,
    pub process_id: ProcessId,
    pub parent_process_id: ProcessId,
    pub state: ProcessState,
    pub thread_count: i64,
}

impl From<&Process> for Observation {
    /// **Every process is annotated volatile, so the whole table is absent from the diffable
    /// view.** This is the facet's central decision and it was arrived at by being wrong
    /// twice; the collector's own documentation carries the reasoning and the measurements.
    ///
    /// The short version: a process table cannot be byte-identical on a machine that is
    /// doing anything, and byte-identity is the contract every other facet rests on. Earlier
    /// versions annotated two specific kinds of churn — kernel workqueue threads, whose names
    /// the kernel rewrites, and control-group paths carrying a login-session counter — and
    /// both were real, but both were instances of a general truth rather than the whole of it.
    fn from(process: &Process) -> Self {
        let observed = Observation::object([
            ("command_line", Observation::from(&process.command_line)),
            (
                "control_group",
                process
                    .control_group
                    .as_ref()
                    .map_or_else(Observation::null, Observation::from),
            ),
            (
                "executable",
                process
                    .executable
                    .as_ref()
                    .map_or_else(Observation::null, |path| Observation::text(path.as_str())),
            ),
            (
                "group_id",
                Observation::integer(i64::from(process.group_id)),
            ),
            ("name", Observation::from(&process.name)),
            (
                "parent_process_id",
                Observation::from(&process.parent_process_id),
            ),
            ("process_id", Observation::from(&process.process_id)),
            ("state", Observation::from(&process.state)),
            (
                "thread_count",
                // A daemon that spawns work per request moves this constantly.
                Observation::integer(process.thread_count).volatile(),
            ),
            ("user_id", Observation::integer(i64::from(process.user_id))),
        ]);

        observed.volatile()
    }
}
