//! Which box this run is describing.

use std::fs;

// One import, because `rastro-collector` re-exports what an author needs. A
// collector written outside this repo looks exactly like this.
use rastro_collector::{
    CollectionError, Collector, CollectorCategory, CollectorId, CollectorIdentity,
    CollectorVersion, FacetName, Observation, Presence,
};

/// The live hostname, as opposed to the configured one in `/etc/hostname`.
///
/// Effective state over declared state: a `hostnamectl set-hostname` that never
/// reached the config file is exactly the drift worth seeing.
const HOSTNAME_PATH: &str = "/proc/sys/kernel/hostname";

pub struct HostCollector {
    name: FacetName,
    identity: CollectorIdentity,
    hostname: Result<String, String>,
}

impl HostCollector {
    pub fn new() -> Self {
        Self::reading(read_hostname())
    }

    /// The same collector over a reading somebody else took.
    ///
    /// The composition root reads the hostname, because the default output filename carries
    /// it and a second read could disagree with the one in the document. Carried as a
    /// `Result` so an unreadable hostname is still this facet's recorded error rather than
    /// the end of the run.
    pub fn reading(hostname: Result<String, String>) -> Self {
        Self {
            hostname,
            name: FacetName::new("host").expect("`host` is a legal facet name"),
            identity: CollectorIdentity::new(
                CollectorId::new("host").expect("`host` is a legal collector id"),
                CollectorVersion::new("1").expect("`1` is a legal collector version"),
            ),
        }
    }
}

impl Default for HostCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl Collector for HostCollector {
    fn name(&self) -> &FacetName {
        &self.name
    }

    fn identity(&self) -> &CollectorIdentity {
        &self.identity
    }

    fn category(&self) -> CollectorCategory {
        CollectorCategory::Metadata
    }

    /// Always present: a run that produced a fingerprint ran on a host.
    ///
    /// An unreadable hostname is a failure to *read* the host, not evidence
    /// that there isn't one, so it surfaces from `collect` instead.
    fn presence(&self) -> Presence {
        Presence::Present
    }

    fn collect(&self) -> Result<Observation, CollectionError> {
        let hostname = self.hostname.as_ref().map_err(CollectionError::new)?;

        Ok(Observation::object([(
            "hostname",
            Observation::text(hostname.trim()),
        )]))
    }
}

/// The live hostname, or why it could not be read.
///
/// Public because the composition root reads it and hands it both to this collector and to
/// the output filename: one read, so the document and the file it lands in cannot disagree.
pub fn read_hostname() -> Result<String, String> {
    fs::read_to_string(HOSTNAME_PATH)
        .map(|hostname| hostname.trim().to_owned())
        .map_err(|error| format!("could not read {HOSTNAME_PATH}: {error}"))
}
