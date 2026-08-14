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
}

impl HostCollector {
    pub fn new() -> Self {
        Self {
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
        let hostname = fs::read_to_string(HOSTNAME_PATH).map_err(|error| {
            CollectionError::new(format!("could not read {HOSTNAME_PATH}: {error}"))
        })?;

        Ok(Observation::object([(
            "hostname",
            Observation::text(hostname.trim()),
        )]))
    }
}
