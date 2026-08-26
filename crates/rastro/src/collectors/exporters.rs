//! What watches this box, and how it was told to watch.
//!
//! Three layers, and the dependency arrows only point one way:
//! [`source`] knows [`model`], `model` knows [`value_objects`], and neither of the last
//! two knows a host interface exists.
//!
//! # The facet the packages one cannot be
//!
//! On the box this was developed against, six telemetry agents are running and **`dpkg` has
//! heard of exactly one of them**. collectd is a Debian package at `5.12.0-14`; cAdvisor,
//! node_exporter, process-exporter, systemd_exporter and postgres_exporter are binaries
//! dropped into `/usr/local/bin` by Ansible, invisible to every package manager on the host.
//! Their versions exist nowhere on the box except inside the binaries themselves, which is
//! why this facet runs them and asks.
//!
//! That is also the answer to "why not just read the units facet". The units facet now
//! records what every unit starts, which covers the flags; it cannot tell you that the
//! `node_exporter` behind an unchanged unit file was swapped for a different build.
//!
//! # Layer 3, dispatched rather than guessed
//!
//! An agent is found by the **binary a unit starts**, matched against a named catalogue.
//! Not by unit name, because `process_exporter.service` runs `process-exporter` and an
//! operator may name a unit anything; and not by a heuristic such as "any unit with a
//! `--web.listen-address`", which would sweep in unrelated daemons and still miss cAdvisor
//! and collectd, neither of which uses that flag.
//!
//! # Configured, not bound
//!
//! Every endpoint here is what the flags asked for. Whether anything is listening on it is
//! the sockets facet's answer, read from the kernel, and the two are deliberately separate
//! so they can disagree: an agent configured for 9100 with nothing bound to it is a dead
//! exporter, and only two independent observations can show that.
pub mod model;
pub mod source;
pub mod value_objects;

pub use model::{Endpoint, Exporter, ExporterBuild, ExporterFleet};
pub use source::{
    CATALOGUE, Deployment, EndpointDialect, KnownAgent, TelemetryFleet, VersionDialect,
};
pub use value_objects::{AgentId, BuildRevision, ExporterVersion, SettingName, SettingValue};

// One import, because `rastro-collector` re-exports what an author needs. A
// collector written outside this repo looks exactly like this.
use rastro_collector::{
    CollectionError, Collector, CollectorCategory, CollectorId, CollectorIdentity,
    CollectorVersion, FacetName, Observation, Presence,
};

pub struct ExportersCollector {
    name: FacetName,
    identity: CollectorIdentity,
    fleet: Option<TelemetryFleet>,
}

impl ExportersCollector {
    pub fn new() -> Self {
        Self::reading(TelemetryFleet::detect())
    }

    /// The same collector over a source the caller chose.
    pub fn reading(fleet: Option<TelemetryFleet>) -> Self {
        Self {
            name: FacetName::new("exporters").expect("`exporters` is a legal facet name"),
            identity: CollectorIdentity::new(
                CollectorId::new("exporters").expect("`exporters` is a legal collector id"),
                CollectorVersion::new("1").expect("`1` is a legal collector version"),
            ),
            fleet,
        }
    }
}

impl Default for ExportersCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl Collector for ExportersCollector {
    fn name(&self) -> &FacetName {
        &self.name
    }

    fn identity(&self) -> &CollectorIdentity {
        &self.identity
    }

    fn category(&self) -> CollectorCategory {
        CollectorCategory::State
    }

    /// `absent` without systemd, because every agent here is dispatched from a unit.
    ///
    /// The same answer the units collector gives, and for the same reason: a box with no
    /// systemd is one this collector has not looked at, not one it can call empty. An agent
    /// started by something else is real and would be missed, which is a documented gap
    /// rather than a claim.
    ///
    /// A box that runs systemd and no telemetry is `present` with an empty fleet. That is a
    /// different fact from "rastro could not look", and the document keeps them apart.
    fn presence(&self) -> Presence {
        match self.fleet {
            Some(_) => Presence::Present,
            None => Presence::Absent,
        }
    }

    fn collect(&self) -> Result<Observation, CollectionError> {
        let fleet = self.fleet.as_ref().ok_or_else(|| {
            CollectionError::new("no systemd was found, so there are no units to dispatch from")
        })?;

        Ok(Observation::from(&fleet.read()?))
    }
}
