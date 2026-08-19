//! Which packages are installed, according to whichever manager the host uses.
//!
//! One collector rather than one per manager, and not only for tidiness: two collectors
//! claiming the facet name `packages` would fail the run, because an absent facet is still
//! a facet with that name.

pub mod model;
pub mod source;
pub mod value_objects;

pub use model::{InstallationStatus, Package, PackageInventory, PackageSet};
pub use source::{ApkDatabase, DpkgQuery, PackageSource};
pub use value_objects::{
    Architecture, ErrorFlag, InstallationState, PackageManager, PackageName, PackageVersion,
    SelectionState,
};

// One import, because `rastro-collector` re-exports what an author needs. A
// collector written outside this repo looks exactly like this.
use rastro_collector::{
    CollectionError, Collector, CollectorCategory, CollectorId, CollectorIdentity,
    CollectorVersion, FacetName, Observation, Presence,
};

pub struct PackagesCollector {
    name: FacetName,
    identity: CollectorIdentity,
    sources: Vec<PackageSource>,
}

impl PackagesCollector {
    /// Detects every manager on the host once, at construction.
    ///
    /// Detecting here rather than inside `presence` is what stops the two disagreeing: what
    /// was found is the very thing `collect` will read.
    pub fn new() -> Self {
        Self::reading(PackageSource::detect_all())
    }

    /// The same collector over sources the caller chose.
    pub fn reading(sources: Vec<PackageSource>) -> Self {
        Self {
            name: FacetName::new("packages").expect("`packages` is a legal facet name"),
            identity: CollectorIdentity::new(
                CollectorId::new("packages").expect("`packages` is a legal collector id"),
                CollectorVersion::new("1").expect("`1` is a legal collector version"),
            ),
            sources,
        }
    }
}

impl Default for PackagesCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl Collector for PackagesCollector {
    fn name(&self) -> &FacetName {
        &self.name
    }

    fn identity(&self) -> &CollectorIdentity {
        &self.identity
    }

    fn category(&self) -> CollectorCategory {
        CollectorCategory::State
    }

    /// Absent when the host has no package manager rastro can read.
    ///
    /// A genuine answer rather than a failure, and the first one rastro produces: a box
    /// with neither dpkg nor apk is a box whose packages are not managed by either, which
    /// is state. Which manager was found is then reported in the data, so the difference
    /// between "no manager" and "a manager with nothing installed" survives a diff.
    fn presence(&self) -> Presence {
        if self.sources.is_empty() {
            return Presence::Absent;
        }

        Presence::Present
    }

    fn collect(&self) -> Result<Observation, CollectionError> {
        let sets = self
            .sources
            .iter()
            .map(|source| Ok((source.manager(), source.read()?)))
            .collect::<Result<Vec<_>, CollectionError>>()?;

        Ok(Observation::from(&PackageInventory::new(sets)))
    }
}
