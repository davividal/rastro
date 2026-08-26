//! The agents rastro can read, and how each one has to be asked.

use super::endpoint_dialect::EndpointDialect;
use super::version_dialect::VersionDialect;

/// One telemetry agent rastro knows how to read.
///
/// # Why a fixed table is the honest shape here
///
/// This is Layer 3, where knowing the service *is* the collector, and the alternative is
/// worse rather than more general: a heuristic that treated any unit with a
/// `--web.listen-address` as an exporter would sweep in unrelated daemons and would still
/// miss cAdvisor and collectd, which do not use that flag. A named table is auditable, and
/// an agent it does not know is simply not in the facet rather than half-read.
///
/// Keyed on the **binary**, not the unit. `process_exporter.service` starts a program
/// called `process-exporter`, and an operator may name a unit anything at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KnownAgent {
    pub program: &'static str,
    pub version: VersionDialect,
    pub endpoint: EndpointDialect,
}

/// Every agent this facet reads, in the order they are declared.
///
/// The six deployed on the development box. Each entry is measured: the dialects say what
/// that agent was actually observed to do, not what its documentation claims.
pub const CATALOGUE: [KnownAgent; 6] = [
    KnownAgent {
        program: "cadvisor",
        version: VersionDialect::Cadvisor,
        endpoint: EndpointDialect::SeparateIpAndPort,
    },
    KnownAgent {
        program: "node_exporter",
        version: VersionDialect::PrometheusCommon,
        endpoint: EndpointDialect::WebListenAddress,
    },
    KnownAgent {
        program: "process-exporter",
        version: VersionDialect::PrometheusCommon,
        endpoint: EndpointDialect::WebListenAddress,
    },
    KnownAgent {
        program: "systemd_exporter",
        version: VersionDialect::PrometheusCommon,
        endpoint: EndpointDialect::WebListenAddress,
    },
    KnownAgent {
        program: "postgres_exporter",
        version: VersionDialect::PrometheusCommon,
        endpoint: EndpointDialect::WebListenAddress,
    },
    KnownAgent {
        program: "collectd",
        version: VersionDialect::None,
        endpoint: EndpointDialect::NotInArguments,
    },
];

/// The agent a unit starts, if rastro knows it.
///
/// Matched on the final path component, so a binary installed somewhere unusual is still
/// recognised. Where it lives is recorded separately, and it is what decides whether rastro
/// is willing to run it.
pub fn agent_of(executable: &str) -> Option<&'static KnownAgent> {
    let program = executable.rsplit('/').next()?;

    CATALOGUE.iter().find(|agent| agent.program == program)
}
