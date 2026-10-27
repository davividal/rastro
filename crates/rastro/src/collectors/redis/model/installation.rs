//! The redis on this box: what is installed, and what is running.

use std::collections::{BTreeMap, BTreeSet};

use rastro_collector::Observation;

use crate::collectors::redis::model::Instance;
use crate::collectors::redis::value_objects::ServerKind;

/// What this box holds of redis and valkey.
///
/// **Installed and running are reported side by side rather than folded into a verdict.** A
/// server running from a directory no package owns, and a package installed with nothing
/// running, are both findings, and a single "running" flag would hide either.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Installation {
    /// The families with a server binary in a system directory.
    pub installed: BTreeSet<ServerKind>,

    /// The running servers, keyed as discovery keyed them.
    pub instances: BTreeMap<String, Instance>,

    /// Whether some process could not be inspected, so a server among them would not be listed.
    ///
    /// Rendered as words with no count, and marked incomplete: how many processes a run was
    /// refused moves between two runs of an unchanged box, which the document must not.
    pub uninspected_processes: bool,
}

impl From<&Installation> for Observation {
    fn from(installation: &Installation) -> Self {
        Observation::object([
            (
                "installed",
                // A set: which families are installed has no order the host keeps.
                Observation::set(
                    installation
                        .installed
                        .iter()
                        .map(|kind| Observation::text(kind.as_str())),
                ),
            ),
            (
                "instances",
                Observation::object(
                    installation
                        .instances
                        .iter()
                        .map(|(key, instance)| (key.as_str(), Observation::from(instance))),
                ),
            ),
            (
                "uninspected_processes",
                match installation.uninspected_processes {
                    true => Observation::text(
                        "some processes could not be inspected, so a server among them would not \
                         be listed",
                    )
                    .incomplete(),
                    false => Observation::null(),
                },
            ),
        ])
    }
}
