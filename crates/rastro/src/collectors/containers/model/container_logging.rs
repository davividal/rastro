//! Where a container's output goes.

use std::collections::BTreeMap;

use rastro_collector::{NonEmptyText, Observation};

/// The log driver and the options it was given.
///
/// **The options are as much state as the driver.** An unbounded `json-file` is how a box
/// fills its disk, and the difference between that and the same driver with `max-size` set is
/// invisible unless both are recorded. A driver change is the other half: a container moved
/// from `json-file` to `journald` still logs, and `docker logs` stops answering for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerLogging {
    pub driver: NonEmptyText,
    /// Empty for a container on the engine's defaults, which is a real and common state.
    pub options: BTreeMap<NonEmptyText, String>,
}

impl From<&ContainerLogging> for Observation {
    fn from(logging: &ContainerLogging) -> Self {
        Observation::object([
            ("driver", Observation::text(logging.driver.as_str())),
            (
                "options",
                Observation::object(
                    logging
                        .options
                        .iter()
                        .map(|(name, value)| (name.as_str(), Observation::text(value))),
                ),
            ),
        ])
    }
}
