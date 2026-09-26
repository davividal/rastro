//! The redis on this box: what is installed, and what is running.

use std::collections::BTreeSet;

use rastro_collector::Observation;

use crate::collectors::redis::value_objects::ServerKind;

/// What this box holds of redis and valkey.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Installation {
    /// The families with a server binary in a system directory.
    pub installed: BTreeSet<ServerKind>,
}

impl From<&Installation> for Observation {
    fn from(installation: &Installation) -> Self {
        Observation::object([(
            "installed",
            // A set: which families are installed has no order the host keeps.
            Observation::set(
                installation
                    .installed
                    .iter()
                    .map(|kind| Observation::text(kind.as_str())),
            ),
        )])
    }
}
