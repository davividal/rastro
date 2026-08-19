//! What is installed on the host, by manager.

use std::collections::BTreeMap;

use rastro_collector::{CollectionError, Observation};

use super::package_set::PackageSet;
use crate::collectors::packages::value_objects::PackageManager;

/// Every manager rastro knows how to read, and what each reported.
///
/// A manager rastro did not find is present as a key with no set, not missing from the map.
/// That distinction is the whole point: a document saying nothing about dpkg cannot be told
/// apart from a document written before rastro could read dpkg, whereas one saying dpkg is
/// not here states a fact about the host. It also keeps the facet diffable in the direction
/// that matters, since installing a manager flips a null to an object.
///
/// rastro not reading a manager some other distribution uses is a limit of rastro, never a
/// failure of the host, so it is reported as state and not as an error.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PackageInventory(BTreeMap<PackageManager, Option<PackageSet>>);

impl PackageInventory {
    /// Files each manager's report under its name.
    ///
    /// A repeated manager is refused rather than overwritten, which is the rule the other two
    /// keyed collections already follow. Nothing can produce one today, and that is the point:
    /// if it ever does, one manager's packages would vanish from a document claiming to be
    /// complete.
    pub fn new(
        sets: impl IntoIterator<Item = (PackageManager, Option<PackageSet>)>,
    ) -> Result<Self, CollectionError> {
        let mut inventory = BTreeMap::new();

        for (manager, set) in sets {
            if inventory.insert(manager, set).is_some() {
                return Err(CollectionError::new(format!(
                    "the {} package manager was reported twice",
                    manager.as_str()
                )));
            }
        }

        Ok(Self(inventory))
    }

    pub fn sets(&self) -> &BTreeMap<PackageManager, Option<PackageSet>> {
        &self.0
    }
}

impl From<&PackageInventory> for Observation {
    fn from(inventory: &PackageInventory) -> Self {
        Observation::object(inventory.sets().iter().map(|(manager, set)| {
            let reported = match set {
                Some(set) => Observation::from(set),
                // `null`, not the word "absent": a key sometimes text and sometimes an object
                // is awkward for every consumer.
                None => Observation::null(),
            };

            (manager.as_str(), reported)
        }))
    }
}
