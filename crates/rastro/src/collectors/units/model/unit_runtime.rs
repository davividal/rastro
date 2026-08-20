//! What systemd is doing with a unit right now.

use rastro_collector::Observation;

use crate::collectors::units::value_objects::{ActiveState, Description, LoadState, SubState};

/// The loaded side of a unit: whether systemd found it, and what it is doing.
///
/// **Three states rather than one, because systemd genuinely reports three**, and the
/// combination is what carries meaning. `loaded` with `active/running` is a daemon
/// that is up; `loaded` with `active/exited` is a oneshot that finished; `not-found`
/// with `inactive/dead` is a unit something in the dependency graph asks for and the
/// box does not have.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnitRuntime {
    pub load: LoadState,
    pub active: ActiveState,
    pub sub: SubState,
    /// Absent only when systemd reported no description at all.
    ///
    /// Not the `not-found` case, which a first guess would expect: systemd substitutes
    /// the unit's own name as the description for those, so they carry one like any
    /// other unit.
    pub description: Option<Description>,
}

impl From<&UnitRuntime> for Observation {
    fn from(runtime: &UnitRuntime) -> Self {
        Observation::object([
            ("active", Observation::from(&runtime.active)),
            (
                "description",
                runtime
                    .description
                    .as_ref()
                    .map_or_else(Observation::null, Observation::from),
            ),
            ("load", Observation::from(&runtime.load)),
            ("sub", Observation::from(&runtime.sub)),
        ])
    }
}
