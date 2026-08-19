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

    /// Always present, because the subject is the managers rastro can read, and it can always
    /// report on those.
    ///
    /// Deliberately not `Absent` and not `Undetermined`. `Absent` would say the host has no
    /// packages, which rastro cannot establish from two negative probes: a RHEL box has
    /// fifteen hundred rpms. `Undetermined` maps to a facet `error`, and rastro not shipping a
    /// collector for rpm is a limit of rastro, not a fault of the host, so an error would be a
    /// permanent false alarm sitting in every diff of that box forever.
    ///
    /// What rastro does know goes in the data instead: a key per manager it reads, null for
    /// the ones that are not here. That is a fact about the host, it is diffable, and it
    /// leaves the operator to draw their own conclusion from it.
    fn presence(&self) -> Presence {
        Presence::Present
    }

    fn collect(&self) -> Result<Observation, CollectionError> {
        let found = self
            .sources
            .iter()
            .map(|source| Ok((source.manager(), source.read()?)))
            .collect::<Result<Vec<_>, CollectionError>>()?;

        Ok(Observation::from(&PackageInventory::of(found)?))
    }
}
