//! What is running on this box.
//!
//! Three layers, and the dependency arrows only point one way:
//! [`source`] knows [`model`], `model` knows [`value_objects`], and neither of the last
//! two knows a host interface exists.
//!
//! # The most volatile facet here, and what is left after the noise is removed
//!
//! A process table is mostly weather. Pids churn, states flip, thread counts move, and
//! short-lived processes appear and vanish between two runs seconds apart. Treated naively
//! this facet would differ on every run and teach an operator to ignore it.
//!
//! Two decisions make it useful instead. The table is a **sorted list rather than a map
//! keyed by pid**, because a volatile key takes its whole entry out of the diffable view and
//! keying by pid would leave the facet empty on every run. And the four moving fields — pid,
//! parent pid, state, thread count — are annotated volatile, so what the diffable view
//! keeps is each process's name, arguments, account, control group and binary.
//!
//! On a box in steady state that surviving list is identical between runs, and a new daemon
//! shows up as one new entry. On a busy box it will still churn, and that churn is real:
//! a process was running. `ProcessTable`'s own documentation says so plainly rather than
//! claiming more than the design delivers.
//!
//! The control group is the field worth knowing about: on a systemd box it names the unit a
//! process belongs to, which is the only link between this facet and `units`.
pub mod model;
pub mod source;
pub mod value_objects;

pub use model::{Process, ProcessTable};
pub use source::{ProcProcesses, proc_cmdline, proc_status};
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
                CollectorVersion::new("1").expect("`1` is a legal collector version"),
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
