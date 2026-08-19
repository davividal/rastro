//! What is installed on the host, by manager.

use std::collections::BTreeMap;

use rastro_collector::Observation;

use super::package_set::PackageSet;
use crate::collectors::packages::value_objects::PackageManager;

/// Every manager rastro found, with what each reported.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PackageInventory(BTreeMap<PackageManager, PackageSet>);

impl PackageInventory {
    pub fn new(sets: impl IntoIterator<Item = (PackageManager, PackageSet)>) -> Self {
        Self(sets.into_iter().collect())
    }

    pub fn sets(&self) -> &BTreeMap<PackageManager, PackageSet> {
        &self.0
    }
}

impl From<&PackageInventory> for Observation {
    fn from(inventory: &PackageInventory) -> Self {
        Observation::object(
            inventory
                .sets()
                .iter()
                .map(|(manager, set)| (manager.as_str(), Observation::from(set))),
        )
    }
}
