//! What a process is doing right now.

use rastro_collector::{CollectionError, NonEmptyText, Observation};

/// A process's scheduling state, as `/proc/<pid>/status` words it.
///
/// `S (sleeping)`, `R (running)`, `D (disk sleep)`, `Z (zombie)`, `T (stopped)`, `I (idle)`.
///
/// **Volatile, because almost every process is asleep at any instant and briefly is not.**
/// A daemon flipping between sleeping and running between two runs says nothing about the
/// box having changed. `Z` and `D` genuinely matter — a zombie or an uninterruptible sleep
/// is a fault — but they are moments rather than configuration, and a fingerprint that
/// reported them as state would put a different value in every run.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ProcessState(NonEmptyText);

impl ProcessState {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(NonEmptyText::new(value, "process state")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<&ProcessState> for Observation {
    fn from(state: &ProcessState) -> Self {
        Observation::text(state.as_str()).volatile()
    }
}
