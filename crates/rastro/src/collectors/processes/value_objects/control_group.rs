//! Which control group a process belongs to.

use rastro_collector::{CollectionError, NonEmptyText, Observation};

/// A process's control group path.
///
/// `/init.scope`, `/system.slice/ssh.service`, `/user.slice/user-1000.slice/session-4.scope`.
///
/// **The field that ties this facet to the `units` one**, and the reason it is worth
/// collecting at all: on a systemd box the cgroup path names the unit a process belongs to,
/// so a process appearing under `/system.slice/something.service` is how you learn which
/// unit spawned it. Nothing else in the process table says that.
///
/// Text rather than a path type: the value is a cgroup hierarchy path, not a filesystem one,
/// and under cgroup v2 the line `/proc/<pid>/cgroup` carries is `0::` followed by it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ControlGroup(NonEmptyText);

impl ControlGroup {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(NonEmptyText::new(value, "control group")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<&ControlGroup> for Observation {
    fn from(group: &ControlGroup) -> Self {
        Observation::text(group.as_str())
    }
}
