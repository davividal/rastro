//! What is running on this box.
//!
//! Three layers, and the dependency arrows only point one way:
//! [`source`] knows [`model`], `model` knows [`value_objects`], and neither of the last
//! two knows a host interface exists.
//!
//! # This facet does not reach the diffable view, and that was learned the hard way
//!
//! **Every process is annotated volatile, so a default run reports an empty list and
//! `--include-volatile` reports the table.** That is the whole design, and it replaced two
//! narrower attempts that were each correct about a real thing and wrong about the general
//! case.
//!
//! The first attempt annotated only the four obviously moving fields — pid, parent pid,
//! state, thread count — and kept name, arguments, account, control group and binary, on the
//! reasoning that a box in steady state reports the same set of daemons twice. Running it
//! proved two specific holes, both real and both now recorded here rather than in code:
//!
//! - **The kernel rewrites a workqueue thread's name to the work it is running.**
//!   `kworker/0:3-cgroup_release` became `kworker/0:3-events` between two runs seconds
//!   apart. A kernel thread's name is its only identity, so there was nothing left to diff.
//! - **rastro observing the box changed what it observed.** Its own process, and the `sudo`,
//!   `sh` and `sshd` above it, sit in a control group carrying the login-session counter, so
//!   `session-851.scope` became `session-852.scope` over two ssh connections.
//!
//! Annotating those two was enough to make the box byte-identical, including under four
//! hundred concurrently spawned processes. It was **not** enough in CI, where the harness
//! runs while sibling tests spawn `sleep` and re-run this very binary, and two runs differed
//! by a couple of entries.
//!
//! That is the general truth the two narrow rules were instances of: **a process table
//! cannot be byte-identical on a machine that is doing anything**, and byte-identity is the
//! contract every other facet rests on. A production box has cron jobs and timers firing for
//! the same reason a CI runner has test processes. So the table is volatile whole, which is
//! precisely what `Volatility` is for — "changes on its own between two runs of an unchanged
//! host" — rather than something the contract had to be weakened to accommodate.
//!
//! # What answers the durable question instead
//!
//! The `units` facet. What an operator wants from a diff is which services are enabled and
//! loaded, and that is stable by construction because it is configuration rather than a
//! snapshot. This facet answers "what is running *right now*", which is a thing to look at
//! while standing in front of the box, not a thing to diff.
//!
//! The control group is still the field worth knowing about in the complete view: on a
//! systemd box it names the unit a process belongs to, which is the only link between this
//! facet and `units`.
pub mod model;
pub mod source;
pub mod value_objects;

pub use model::{Process, ProcessTable};
pub use source::{ProcProcesses, proc_cmdline, proc_processes, proc_status};
pub use value_objects::{CommandLine, ControlGroup, ProcessId, ProcessName, ProcessState};

// One import, because `rastro-collector` re-exports what an author needs. A
// collector written outside this repo looks exactly like this.
use rastro_collector::{
    CollectionError, Collector, CollectorCategory, CollectorId, CollectorIdentity,
    CollectorVersion, FacetName, Observation, Presence,
};

pub struct ProcessesCollector {
    name: FacetName,
    identity: CollectorIdentity,
    processes: ProcProcesses,
}

impl ProcessesCollector {
    pub fn new() -> Self {
        Self::reading(ProcProcesses::new())
    }

    /// The same collector over a source the caller chose.
    pub fn reading(processes: ProcProcesses) -> Self {
        Self {
            name: FacetName::new("processes").expect("`processes` is a legal facet name"),
            identity: CollectorIdentity::new(
                CollectorId::new("processes").expect("`processes` is a legal collector id"),
                // Second version: the facet's data is unchanged but its *visibility* is not.
                // A consumer that saw processes in a default run and now sees none needs to
                // be able to tell that the collector moved rather than the host emptying.
                CollectorVersion::new("2").expect("`2` is a legal collector version"),
            ),
            processes,
        }
    }
}

impl Default for ProcessesCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl Collector for ProcessesCollector {
    fn name(&self) -> &FacetName {
        &self.name
    }

    fn identity(&self) -> &CollectorIdentity {
        &self.identity
    }

    fn category(&self) -> CollectorCategory {
        CollectorCategory::State
    }

    /// Two answers, and unlike the other `/proc` collectors there is no `absent`.
    ///
    /// A running kernel always has processes — rastro is one of them — so there is no
    /// equivalent of a kernel built without module support. Either procfs is mounted and the
    /// table can be read, or it is not and rastro cannot see the box's processes at all.
    fn presence(&self) -> Presence {
        if self.processes.filesystem_is_mounted() {
            return Presence::Present;
        }

        Presence::Undetermined {
            reason: format!(
                "{} is not mounted, so what is running on this host cannot be told",
                self.processes.filesystem().display()
            ),
        }
    }

    fn collect(&self) -> Result<Observation, CollectionError> {
        Ok(Observation::from(&self.processes.read()?))
    }
}
