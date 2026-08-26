//! How an agent spells its own version.

use rastro_collector::CollectionError;

use crate::collectors::canonical_tool::ToolOutput;
use crate::collectors::exporters::model::ExporterBuild;
use crate::collectors::exporters::value_objects::{BuildRevision, ExporterVersion};

/// What separates a version from the bracketed detail after it, in both dialects.
const DETAIL_OPENS: &str = " (";
const DETAIL_CLOSES: char = ')';

/// The `prometheus/common/version` banner: `<agent>, version <V> (branch: …, revision: …)`.
const PROMETHEUS_VERSION: &str = ", version ";
const PROMETHEUS_REVISION: &str = "revision: ";

/// cAdvisor's own banner, which predates the Prometheus convention.
const CADVISOR_BANNER: &str = "cAdvisor version ";

/// The ways the agents rastro knows announce a version.
///
/// An exhaustive enum rather than a lookup of parsers, so adding an agent whose banner is a
/// third shape makes the compiler name every place that has to decide about it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VersionDialect {
    /// Five of the six agents on the development box, and every Prometheus exporter.
    PrometheusCommon,
    Cadvisor,
    /// The agent will not say. collectd is the case: its only version-ish flag is `-V`,
    /// which it rejects as an invalid option.
    None,
}

impl VersionDialect {
    /// The flag that makes the agent print its banner, if asking is worth anything.
    pub fn flag(&self) -> Option<&'static str> {
        match self {
            Self::PrometheusCommon | Self::Cadvisor => Some("--version"),
            Self::None => None,
        }
    }

    /// Reads the banner out of whichever stream carries it.
    ///
    /// **Both streams are searched, and that is measured rather than defensive.** Of the
    /// four Prometheus-style agents on the development box, `node_exporter` and
    /// `process-exporter` print the banner to stdout while `systemd_exporter` and
    /// `postgres_exporter` print the same text to stderr, all four exiting zero. Reading
    /// stdout alone would report half the fleet as having no version, which is the quiet
    /// kind of wrong this project exists to avoid.
    pub fn parse(&self, output: &ToolOutput) -> Result<Option<ExporterBuild>, CollectionError> {
        let anchor = match self {
            Self::PrometheusCommon => PROMETHEUS_VERSION,
            Self::Cadvisor => CADVISOR_BANNER,
            Self::None => return Ok(None),
        };

        let line = banner(output, anchor).ok_or_else(|| {
            CollectionError::new(format!(
                "no line of the agent's output contains {anchor:?}, so what it printed is \
                 not a version banner rastro knows: {:?}",
                first_line(output)
            ))
        })?;

        let stated = match self {
            Self::PrometheusCommon => prometheus_build(line),
            Self::Cadvisor => cadvisor_build(line),
            Self::None => None,
        };

        let (version, revision) = stated.ok_or_else(|| {
            CollectionError::new(format!(
                "the agent's version banner is not shaped as rastro expects: {line:?}"
            ))
        })?;

        Ok(Some(ExporterBuild {
            version: ExporterVersion::new(version)?,
            revision: BuildRevision::new(revision)?,
        }))
    }
}

/// The first line of either stream that carries the dialect's anchor.
fn banner<'a>(output: &'a ToolOutput, anchor: &str) -> Option<&'a str> {
    output
        .stdout
        .lines()
        .chain(output.stderr.lines())
        .find(|line| line.contains(anchor))
}

/// What the agent said, for a failure that has to quote something.
fn first_line(output: &ToolOutput) -> &str {
    output
        .stdout
        .lines()
        .chain(output.stderr.lines())
        .next()
        .unwrap_or_default()
}

/// `node_exporter, version 1.12.1 (branch: HEAD, revision: 6044da78)`.
///
/// The branch is deliberately not read: `process-exporter` is built without one and prints
/// `branch: ` empty, so a parser that required it would refuse a real agent's real output.
fn prometheus_build(line: &str) -> Option<(&str, &str)> {
    let stated = line.split_once(PROMETHEUS_VERSION)?.1;
    let (version, detail) = stated.split_once(DETAIL_OPENS)?;
    let revision = detail.split_once(PROMETHEUS_REVISION)?.1;

    Some((version, revision.split(DETAIL_CLOSES).next()?))
}

/// `cAdvisor version v0.49.2 (6876475a)`.
fn cadvisor_build(line: &str) -> Option<(&str, &str)> {
    let stated = line.split_once(CADVISOR_BANNER)?.1;
    let (version, detail) = stated.split_once(DETAIL_OPENS)?;

    Some((version, detail.strip_suffix(DETAIL_CLOSES)?))
}
