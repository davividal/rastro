//! Which cgroup interface the engine drives, and which version of it.

use rastro_collector::{NonEmptyText, Observation};

/// The driver and hierarchy version a container's limits are applied through.
///
/// **Kept because it decides whether a limit applies at all.** A `--memory` or `--pids-limit`
/// on a container is a request to the engine, and what becomes of it depends on this pair: a
/// cgroupfs driver on a systemd box fights systemd for the hierarchy, and a v1 hierarchy has
/// no pids controller to hand a limit to. So a box that moved from v1 to v2, or from cgroupfs
/// to systemd, is different state even when every container on it is identical.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CgroupControl {
    pub driver: NonEmptyText,
    pub version: NonEmptyText,
}

impl From<&CgroupControl> for Observation {
    fn from(cgroup: &CgroupControl) -> Self {
        Observation::object([
            ("driver", Observation::text(cgroup.driver.as_str())),
            ("version", Observation::text(cgroup.version.as_str())),
        ])
    }
}
