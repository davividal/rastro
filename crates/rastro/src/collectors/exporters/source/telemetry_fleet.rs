//! Finding the agents, and asking each one what it is.

use std::collections::BTreeMap;

use rastro_collector::CollectionError;

use super::flags;
use super::known_agent::{self, KnownAgent};
use crate::collectors::canonical_tool::CanonicalTool;
use crate::collectors::exporters::model::ExporterBuild;
use crate::collectors::exporters::model::{Endpoint, Exporter, ExporterFleet};
use crate::collectors::exporters::value_objects::{AgentId, SettingName, SettingValue};
use crate::collectors::systemd::{ExecutablePath, UnitName, systemctl_show};

const PROGRAM: &str = "systemctl";
const SHOW: &str = "show";
const ID_PROPERTY: &str = "--property=Id";
const EXEC_START_PROPERTY: &str = "--property=ExecStartEx";
const NO_PAGER: &str = "--no-pager";

/// Every loaded unit, so the scan sees the whole box rather than a guessed subset.
const LIST_UNITS: [&str; 4] = ["list-units", "--all", "--output=json", NO_PAGER];

/// A known agent found on the box, before its binary has been asked for a version.
///
/// Separate from [`Exporter`] because everything here comes from systemd alone, which is
/// what makes the whole identification testable without running a single agent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Deployment {
    pub unit: UnitName,
    pub agent: AgentId,
    pub executable: ExecutablePath,
    pub endpoint: Option<Endpoint>,
    pub settings: BTreeMap<SettingName, Option<SettingValue>>,
    known: &'static KnownAgent,
}

/// systemd's view of the box, narrowed to the agents rastro can read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TelemetryFleet {
    tool: CanonicalTool,
}

impl TelemetryFleet {
    /// Finds systemd's control tool, or reports that this host does not run systemd.
    pub fn detect() -> Option<Self> {
        CanonicalTool::located(PROGRAM).map(Self::using)
    }

    /// The same over a tool the caller located.
    pub fn using(tool: CanonicalTool) -> Self {
        Self { tool }
    }

    pub fn tool(&self) -> &CanonicalTool {
        &self.tool
    }

    /// Identifies the agents, then asks each one which build it is.
    pub fn read(&self) -> Result<ExporterFleet, CollectionError> {
        let deployments = Self::identify(&self.show()?)?;
        let exporters = deployments
            .into_iter()
            .map(|deployment| {
                let exporter = self.resolve(&deployment)?;

                Ok((deployment.unit, exporter))
            })
            .collect::<Result<Vec<_>, CollectionError>>()?;

        Ok(ExporterFleet::new(exporters))
    }

    /// Picks the known agents out of what systemd showed.
    ///
    /// Separate from [`Self::read`] so the whole identification is exercised from a fixture,
    /// with no systemd to ask and no agent to run.
    ///
    /// A unit that starts several commands is read from its first: an agent is started once,
    /// and the later commands of a multi-command unit are its setup rather than the service.
    pub fn identify(shown: &str) -> Result<Vec<Deployment>, CollectionError> {
        let mut found = Vec::new();

        for (unit, starts) in systemctl_show::parse(shown)? {
            let Some(start) = starts.first() else {
                continue;
            };
            let Some(known) = known_agent::agent_of(start.executable.as_str()) else {
                continue;
            };

            let settings = flags::parse(start.argv.as_str())?;
            found.push(Deployment {
                unit,
                agent: AgentId::new(known.program)?,
                executable: start.executable.clone(),
                endpoint: known.endpoint.read(&settings)?,
                settings,
                known,
            });
        }

        Ok(found)
    }

    /// The fleet these deployments make up, with no build read for any of them.
    ///
    /// What the document looks like when rastro found the agents but ran none of them,
    /// which is also every case where the binaries live outside the searched directories.
    pub fn fleet_of(deployments: Vec<Deployment>) -> ExporterFleet {
        ExporterFleet::new(
            deployments
                .into_iter()
                .map(|deployment| (deployment.unit.clone(), exporter_of(deployment, None))),
        )
    }

    /// Asks systemd what every loaded unit starts.
    fn show(&self) -> Result<String, CollectionError> {
        let loaded = self.tool.run(&LIST_UNITS)?;
        let names = systemctl_show::unit_names(&loaded)?;
        if names.is_empty() {
            return Ok(String::new());
        }

        // `--` because systemd's own root slice and root mount are named `-.slice` and
        // `-.mount`, which `systemctl` otherwise rejects as invalid options.
        let mut arguments = vec![SHOW, ID_PROPERTY, EXEC_START_PROPERTY, NO_PAGER, "--"];
        arguments.extend(names.iter().map(String::as_str));

        self.tool.run(&arguments)
    }

    /// One deployment, with its version read if rastro is willing to run its binary.
    ///
    /// **The binary is run only where the execution seam would have found it.** A unit may
    /// point at any path at all, and rastro runs as root, so executing whatever a unit
    /// names would hand the decision of which binary runs with full privilege to whoever
    /// wrote the unit. An agent installed outside the root-owned directories is recorded
    /// with everything systemd knows about it and no build, which is honest about both what
    /// was seen and what was declined.
    fn resolve(&self, deployment: &Deployment) -> Result<Exporter, CollectionError> {
        let Some(flag) = deployment.known.version.flag() else {
            return Ok(exporter_of(deployment.clone(), None));
        };
        let Some(binary) = CanonicalTool::located(deployment.known.program) else {
            return Ok(exporter_of(deployment.clone(), None));
        };

        let printed = binary.run_capturing_stderr(&[flag])?;

        Ok(exporter_of(
            deployment.clone(),
            deployment.known.version.parse(&printed)?,
        ))
    }
}

fn exporter_of(deployment: Deployment, build: Option<ExporterBuild>) -> Exporter {
    Exporter {
        agent: deployment.agent,
        executable: deployment.executable,
        build,
        endpoint: deployment.endpoint,
        settings: deployment.settings,
    }
}
