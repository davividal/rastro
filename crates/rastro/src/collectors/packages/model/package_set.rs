//! Every package one manager reported.

use std::collections::BTreeMap;

use rastro_collector::{CollectionError, Observation};

use super::package::Package;
use crate::collectors::packages::value_objects::PackageName;

/// One manager's packages, keyed by name.
///
/// Keyed rather than listed, as with kernel modules and for the same reason: a manager
/// enforces unique names, so keying loses nothing and installing one package shows as a
/// single added key.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PackageSet(BTreeMap<PackageName, Package>);

impl PackageSet {
    /// Files each package under its name.
    ///
    /// A repeated name is refused rather than overwritten: no manager can produce one, so
    /// it means rastro misread the output, and keeping the last of two would drop a
    /// package from a document claiming to be complete.
    pub fn new(
        packages: impl IntoIterator<Item = (PackageName, Package)>,
    ) -> Result<Self, CollectionError> {
        let mut set = BTreeMap::new();

        for (name, package) in packages {
            if set.insert(name.clone(), package).is_some() {
                return Err(CollectionError::new(format!(
                    "the package {:?} was reported twice, so the output was misread",
                    name.as_str()
                )));
            }
        }

        Ok(Self(set))
    }

    pub fn packages(&self) -> &BTreeMap<PackageName, Package> {
        &self.0
    }
}

impl From<&PackageSet> for Observation {
    fn from(set: &PackageSet) -> Self {
        Observation::object(
            set.packages()
                .iter()
                .map(|(name, package)| (name.as_str(), Observation::from(package))),
        )
    }
}
