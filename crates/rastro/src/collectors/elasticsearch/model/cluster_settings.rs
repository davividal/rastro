//! What an operator set on the cluster through the API.

use std::collections::BTreeMap;

use rastro_collector::Observation;

use crate::collectors::elasticsearch::value_objects::ApiValue;

/// Cluster settings set through the API, which is the only place they are recorded.
///
/// **Persistent and transient are kept apart**, because they differ in whether a full cluster
/// restart keeps them, and a setting moving from one to the other is a change that survives or
/// does not survive the next outage. Defaults are not read: what the cluster was *told* is the
/// state an operator changes, and the defaults change with the version the facet already names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClusterSettings {
    pub persistent: BTreeMap<String, ApiValue>,
    pub transient: BTreeMap<String, ApiValue>,
}

impl From<&ClusterSettings> for Observation {
    fn from(settings: &ClusterSettings) -> Self {
        let side = |values: &BTreeMap<String, ApiValue>| {
            Observation::object(
                values
                    .iter()
                    .map(|(name, value)| (name.as_str(), Observation::from(value))),
            )
        };

        Observation::object([
            ("persistent", side(&settings.persistent)),
            ("transient", side(&settings.transient)),
        ])
    }
}
