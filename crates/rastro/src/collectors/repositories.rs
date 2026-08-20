//! Where this box is configured to fetch packages from.
//!
//! One collector rather than one per system, and not only for tidiness: two collectors
//! claiming the facet name `repositories` would fail the run, because an absent facet
//! is still a facet with that name.
//!
//! Three layers, and the dependency arrows only point one way:
//! [`source`] knows [`model`], `model` knows [`value_objects`], and neither of the last
//! two knows a host interface exists.
//!
//! **The companion of the packages facet, and the more useful half of the pair for
//! answering "what did that change do".** A package list says what is installed; this
//! says what the box is allowed to install *from*, and adding a third-party repository
//! is a change that alters every future upgrade while installing nothing today.
//!
//! Both of apt's formats are read, because Debian 12 ships both at once: `sources.list`
//! and the `.list` drop-ins use the one-line format, while `debian.sources` uses
//! deb822. A deb822 paragraph listing several types and suites is expanded into the
//! repositories it actually describes, so the same configuration written either way
//! produces the same facet.

pub mod model;
pub mod source;
pub mod value_objects;

pub use model::{Repository, RepositoryInventory, RepositorySet};
pub use source::{ApkRepositories, AptSources, RepositorySource, apt_deb822, apt_one_line};
pub use value_objects::{
    ArchiveType, Component, Components, Enablement, RepositorySystem, RepositoryTag, RepositoryUri,
    Suite,
};

// One import, because `rastro-collector` re-exports what an author needs. A
// collector written outside this repo looks exactly like this.
use rastro_collector::{
    CollectionError, Collector, CollectorCategory, CollectorId, CollectorIdentity,
    CollectorVersion, FacetName, Observation, Presence,
};

pub struct RepositoriesCollector {
    name: FacetName,
    identity: CollectorIdentity,
    sources: Vec<RepositorySource>,
}

impl RepositoriesCollector {
    /// Detects every repository system on the host once, at construction.
    ///
    /// Detecting here rather than inside `presence` is what stops the two disagreeing:
    /// what was found is the very thing `collect` will read.
    pub fn new() -> Self {
        Self::reading(RepositorySource::detect_all())
    }

    /// The same collector over sources the caller chose.
    pub fn reading(sources: Vec<RepositorySource>) -> Self {
        Self {
            name: FacetName::new("repositories").expect("`repositories` is a legal facet name"),
            identity: CollectorIdentity::new(
                CollectorId::new("repositories").expect("`repositories` is a legal collector id"),
                CollectorVersion::new("1").expect("`1` is a legal collector version"),
            ),
            sources,
        }
    }
}

impl Default for RepositoriesCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl Collector for RepositoriesCollector {
    fn name(&self) -> &FacetName {
        &self.name
    }

    fn identity(&self) -> &CollectorIdentity {
        &self.identity
    }

    fn category(&self) -> CollectorCategory {
        CollectorCategory::State
    }

    /// Always present, because the subject is the systems rastro can read, and it can
    /// always report on those.
    ///
    /// The same reasoning as the packages collector's, which is worth restating because
    /// it is the reasoning a third distribution will test. `Absent` would say the host
    /// fetches packages from nowhere, which two negative probes cannot establish: a
    /// RHEL box has a `/etc/yum.repos.d` full of them. `Undetermined` maps to a facet
    /// `error`, and rastro not shipping a source for dnf is a limit of rastro rather
    /// than a fault of the host, so an error would be a permanent false alarm in every
    /// diff of that box forever.
    ///
    /// What rastro does know goes in the data instead: a key per system it reads, null
    /// for the ones that are not here.
    fn presence(&self) -> Presence {
        Presence::Present
    }

    fn collect(&self) -> Result<Observation, CollectionError> {
        let found = self
            .sources
            .iter()
            .map(|source| Ok((source.system(), source.read()?)))
            .collect::<Result<Vec<_>, CollectionError>>()?;

        Ok(Observation::from(&RepositoryInventory::new(found)?))
    }
}
