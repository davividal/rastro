//! One telemetry agent, as this box deploys it.

use std::collections::BTreeMap;

use rastro_collector::Observation;

use super::endpoint::Endpoint;
use super::exporter_build::ExporterBuild;
use crate::collectors::exporters::value_objects::{AgentId, SettingName, SettingValue};
use crate::collectors::systemd::ExecutablePath;

/// A deployed agent: which one it is, where it lives, which build, and how it was told to
/// run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Exporter {
    pub agent: AgentId,
    pub executable: ExecutablePath,
    /// Absent for an agent that will not tell rastro its version, and for one rastro
    /// declined to run.
    ///
    /// Two causes, both real state rather than failures. collectd has no version flag at
    /// all — its `-V` is rejected as invalid — and it is a Debian package, so the packages
    /// facet already carries `5.12.0-14` for it. The other cause is an agent whose binary
    /// sits outside the root-owned directories the execution seam searches, which rastro
    /// records but will not execute.
    pub build: Option<ExporterBuild>,
    /// Absent when the flags do not configure one, as collectd's do not.
    pub endpoint: Option<Endpoint>,
    /// Keyed by flag name, so a diff names the flag that moved rather than reporting one
    /// long argument string as changed. The value is absent for a flag given without one.
    pub settings: BTreeMap<SettingName, Option<SettingValue>>,
}

impl From<&Exporter> for Observation {
    fn from(exporter: &Exporter) -> Self {
        let settings = exporter.settings.iter().map(|(name, value)| {
            let observed = value
                .as_ref()
                .map_or_else(Observation::null, Observation::from);

            (name.as_str().to_owned(), observed)
        });

        Observation::object([
            ("agent", Observation::from(&exporter.agent)),
            (
                "build",
                exporter
                    .build
                    .as_ref()
                    .map_or_else(Observation::null, Observation::from),
            ),
            (
                "endpoint",
                exporter
                    .endpoint
                    .as_ref()
                    .map_or_else(Observation::null, Observation::from),
            ),
            (
                "executable",
                Observation::text(exporter.executable.as_str()),
            ),
            ("settings", Observation::object(settings)),
        ])
    }
}
