//! Who decided which configuration file rastro read.

use rastro_collector::Observation;

/// Where the path to the root configuration came from.
///
/// **Self-description, for the same reason the envelope carries the effective config.** A
/// box whose nginx was started with `-c /etc/nginx/other.conf` has two configurations on it,
/// and a facet that quietly read the compiled-in one would describe a service nobody is
/// running. Saying which was read is what lets a reader tell those apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigurationSource {
    /// The `-c` the running master was started with.
    RunningMaster,
    /// The `--conf-path` the binary was built with, which is what nginx uses by default.
    CompiledIn,
}

impl ConfigurationSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::RunningMaster => "running_master",
            Self::CompiledIn => "compiled_in",
        }
    }
}

impl From<&ConfigurationSource> for Observation {
    fn from(source: &ConfigurationSource) -> Self {
        Observation::text(source.as_str())
    }
}
