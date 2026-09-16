//! What a container may do beyond, or short of, the engine's default set.

use rastro_collector::Observation;

use crate::collectors::containers::value_objects::Capability;

/// The capabilities added to and dropped from the default set, each sorted.
///
/// **The delta rather than the effective set, because the delta is what was decided.** The
/// effective set is the engine's default plus these, and the default belongs to the engine's
/// version rather than to this container; recording a resolved list would mix a decision
/// somebody made with a default that moves under it, and a docker upgrade would then read as
/// every container having changed.
///
/// Sorted, because the engine keeps them in the order the flags were given and an operator
/// swapping two `--cap-add` flags has not changed the box.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ContainerCapabilities {
    pub added: Vec<Capability>,
    pub dropped: Vec<Capability>,
}

impl From<&ContainerCapabilities> for Observation {
    fn from(capabilities: &ContainerCapabilities) -> Self {
        Observation::object([
            (
                "added",
                Observation::list(capabilities.added.iter().map(Observation::from)),
            ),
            (
                "dropped",
                Observation::list(capabilities.dropped.iter().map(Observation::from)),
            ),
        ])
    }
}
