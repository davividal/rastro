//! Every mount the host reports.

use rastro_collector::Observation;

use super::mount::Mount;

/// The mount table, in the order the host reported it.
///
/// A list rather than a map keyed by mount point, because a mount point can
/// legitimately appear twice: stacked and bind mounts are real, and keying would
/// drop one of them silently. This is the opposite call from a collector whose
/// subject has a unique name, where keying is what removes ordering churn.
///
/// The reported order is kept for the same reason. It is stable between two runs of
/// an unchanged host, so it satisfies the ordering rule, and it carries the
/// stacking that a sort would discard.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MountTable(Vec<Mount>);

impl MountTable {
    pub fn new(mounts: impl IntoIterator<Item = Mount>) -> Self {
        Self(mounts.into_iter().collect())
    }

    pub fn mounts(&self) -> &[Mount] {
        &self.0
    }
}

impl From<&MountTable> for Observation {
    fn from(table: &MountTable) -> Self {
        Observation::list(table.mounts().iter().map(Observation::from))
    }
}
