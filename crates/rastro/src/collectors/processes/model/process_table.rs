//! Every process running.

use rastro_collector::Observation;

use super::process::Process;

/// The process table, sorted.
///
/// # Why a sorted list rather than a map keyed by pid
///
/// **Keying by pid would make the whole facet vanish from the diffable view**, and that is
/// not a hypothetical: a pid is volatile, so every key would be, and a volatile key takes
/// its entry with it. The facet would be an empty object on every run.
///
/// A sorted list moves the volatility inside each entry instead, where the view can drop
/// the four moving fields and keep the six that do not move. What survives is a sorted list
/// of "this binary, with these arguments, as this account, under this unit" — which for a
/// box in steady state is the same list on two runs, and which shows a new daemon as one
/// new entry.
///
/// Sorted rather than left in the kernel's order, because that order is by pid: a daemon
/// that restarts jumps to the end of the table and every entry after it shifts.
///
/// # What still churns, honestly stated
///
/// Anything short-lived. A cron job caught mid-run, a login shell, and rastro's own process
/// are all in here, and a run that catches a different transient set differs. The pid being
/// dropped means rastro's own entry is *identical* between two runs rather than differing by
/// its pid, which is why this works at all on a quiet box, but a busy one will show churn
/// that is real rather than noise: a process was running, and that is state.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ProcessTable(Vec<Process>);

impl ProcessTable {
    pub fn new(processes: impl IntoIterator<Item = Process>) -> Self {
        let mut sorted: Vec<Process> = processes.into_iter().collect();
        sorted.sort();

        Self(sorted)
    }

    pub fn processes(&self) -> &[Process] {
        &self.0
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl From<&ProcessTable> for Observation {
    fn from(table: &ProcessTable) -> Self {
        Observation::list(table.processes().iter().map(Observation::from))
    }
}
