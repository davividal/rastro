//! Which number the kernel knows a process by.

use rastro_collector::{CollectionError, Observation};

/// A process id.
///
/// **Every value of this type is volatile**, and the type exists partly to make that
/// obvious wherever one appears. A pid is handed out from a counter that wraps, so a daemon
/// restarted between two runs has a different one while being the same daemon, and two
/// unrelated processes can hold the same number on either side of a reboot.
///
/// Held as `u32` because that is the width of a `pid_t` the kernel will print, and the width
/// is the check: a negative or oversized value means a `status` field was read out of the
/// wrong line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ProcessId(u32);

impl ProcessId {
    pub fn parse(value: &str) -> Result<Self, CollectionError> {
        value
            .trim()
            .parse::<u32>()
            .map(Self)
            .map_err(|_| CollectionError::new(format!("{value:?} is not a process id")))
    }

    pub fn as_u32(&self) -> u32 {
        self.0
    }
}

impl From<&ProcessId> for Observation {
    fn from(id: &ProcessId) -> Self {
        // Volatile at the leaf, so no caller has to remember to mark it.
        Observation::integer(i64::from(id.as_u32())).volatile()
    }
}
