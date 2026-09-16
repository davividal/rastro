//! Where a container's output goes.

use std::collections::BTreeMap;

use rastro_collector::{NonEmptyText, Observation};

/// The log driver and the options it was given.
///
/// **The options are as much state as the driver.** An unbounded `json-file` is how a box
/// fills its disk, and the difference between that and the same driver with `max-size` set is
/// invisible unless both are recorded. A driver change is the other half: a container moved
/// from `json-file` to `journald` still logs, and `docker logs` stops answering for it.
///
/// **Every option value is sensitive, on the same reasoning the environment gets.** docker's
/// splunk driver requires a `splunk-token`, and the gelf and fluentd drivers take an address
/// that can carry a credential in it. The names are open-ended, since a logging plugin
/// defines its own, so a rule that judged by key would have to enumerate every driver's
/// options and would be wrong about the next one. The keys stay public, which is what keeps
/// this readable: a diff still says `max-size` changed.
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
                        .map(|(name, value)| (name.as_str(), Observation::text(value).sensitive())),
                ),
            ),
        ])
    }
}
