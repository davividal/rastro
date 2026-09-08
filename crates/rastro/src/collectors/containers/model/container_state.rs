//! Where a container is in its life.

use rastro_collector::{NonEmptyText, Observation};

use crate::collectors::containers::value_objects::{ContainerStatus, EngineInstant};

/// The stable half and the moving half of a container's state, in one type.
///
/// **Which is which is the whole design.** The `processes` facet had to annotate its entire
/// table volatile, because a process table cannot be byte-identical on a machine that is
/// doing anything. A container list is the opposite: a container is *declared*, it outlives
/// the run, and whether it is up is the line an operator reads first. So this facet keeps its
/// entries and annotates the values that move on their own.
///
/// Volatile: both stamps and the restart count. A container restarting under a policy moves
/// all three with nobody having touched the box.
///
/// Stable: the status, the exit code, whether the kernel killed it for memory, and the error
/// the engine recorded. Each of those changes only when something about the container did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerState {
    pub status: ContainerStatus,
    pub exit_code: i64,
    /// What the engine recorded about a container that would not start, absent when it had
    /// nothing to say. Empty text would claim the engine explained itself and said nothing.
    pub error: Option<NonEmptyText>,
    pub oom_killed: bool,
    /// Absent for a container that has never run.
    pub started_at: Option<EngineInstant>,
    /// Absent for a container that is still running.
    ///
    /// The engine fills the field with Go's zero time rather than leaving it out, and
    /// `0001-01-01T00:00:00Z` is not a date anything finished at. Mapping it to absent
    /// happens where that spelling is known, in the source.
    pub finished_at: Option<EngineInstant>,
    pub restart_count: i64,
}

impl From<&ContainerState> for Observation {
    fn from(state: &ContainerState) -> Self {
        Observation::object([
            (
                "error",
                match &state.error {
                    Some(error) => Observation::text(error.as_str()),
                    None => Observation::null(),
                },
            ),
            ("exit_code", Observation::integer(state.exit_code)),
            ("finished_at", stamp(state.finished_at.as_ref()).volatile()),
            ("oom_killed", Observation::boolean(state.oom_killed)),
            (
                "restart_count",
                Observation::integer(state.restart_count).volatile(),
            ),
            ("started_at", stamp(state.started_at.as_ref()).volatile()),
            ("status", Observation::from(&state.status)),
        ])
    }
}

fn stamp(instant: Option<&EngineInstant>) -> Observation {
    match instant {
        Some(instant) => Observation::from(instant),
        None => Observation::null(),
    }
}
