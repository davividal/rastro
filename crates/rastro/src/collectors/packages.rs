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
    CollectorVersion, FacetName, FilesystemClaim, Observation, Presence, WalkedTree,
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

        Ok(Observation::from(&PackageInventory::new(found)?))
    }

    /// The database of each manager that is here, as a tree that churns.
    ///
    /// dpkg's `/var/lib/dpkg` and apk's `/lib/apk/db` are where this facet's own answer
    /// comes from, so hashing them adds a second, worse account of the same fact: "something
    /// in the package database changed" where the facet already says which package, at which
    /// version. They churn as well - `status-old`, the lock files and the trigger state all
    /// move during any install - and on the reference cycle five of them were in the diff on
    /// nothing but an inode or an mtime.
    ///
    /// Claimed per manager found rather than as a fixed pair, so an Alpine box claims apk's
    /// database and not a Debian path it does not have.
    ///
    /// **Not sealed.** The files are few and their presence, mode and ownership are worth
    /// seeing: a world-readable `/var/lib/dpkg` is a finding, and a missing one says the
    /// manager was removed.
    fn filesystem_claims(&self) -> Vec<FilesystemClaim> {
        self.sources
            .iter()
            .filter_map(|source| WalkedTree::new(database_of(source.manager())).ok())
            .map(FilesystemClaim::churns)
            .collect()
    }
}

/// Where a manager keeps the database this facet reads.
fn database_of(manager: PackageManager) -> &'static str {
    match manager {
        PackageManager::Apk => "/lib/apk/db",
        PackageManager::Dpkg => "/var/lib/dpkg",
    }
}
