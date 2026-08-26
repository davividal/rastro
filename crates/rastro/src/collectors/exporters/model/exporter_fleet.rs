//! Every telemetry agent on the box.

use std::collections::BTreeMap;

use rastro_collector::Observation;

use super::exporter::Exporter;
use crate::collectors::systemd::UnitName;

/// The agents, keyed by the unit that starts each one.
///
/// **Keyed by unit rather than by agent, because one agent can be deployed twice.** A box
/// with two PostgreSQL clusters runs two `postgres_exporter` instances on two ports, and
/// keying by the agent would let the second silently overwrite the first. systemd enforces
/// one unit per name, so the unit is the key that cannot collide.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ExporterFleet(BTreeMap<UnitName, Exporter>);

impl ExporterFleet {
    pub fn new(exporters: impl IntoIterator<Item = (UnitName, Exporter)>) -> Self {
        Self(exporters.into_iter().collect())
    }

    pub fn exporters(&self) -> &BTreeMap<UnitName, Exporter> {
        &self.0
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl From<&ExporterFleet> for Observation {
    fn from(fleet: &ExporterFleet) -> Self {
        Observation::object(
            fleet
                .exporters()
                .iter()
                .map(|(unit, exporter)| (unit.as_str(), Observation::from(exporter))),
        )
    }
}
