//! Where the host is configured to fetch packages from, by system.

use std::collections::BTreeMap;

use rastro_collector::{CollectionError, Observation};

use super::repository_set::RepositorySet;
use crate::collectors::repositories::value_objects::RepositorySystem;

/// Every repository system rastro knows how to read, and what each reported.
///
/// The same shape, and the same reasoning, as the packages facet's inventory: a system
/// rastro did not find is present as a key with no set rather than missing from the
/// map. A document saying nothing about apt cannot be told apart from a document
/// written before rastro could read apt, whereas one saying apt is not here states a
/// fact about the host, and installing apt then flips a null to a list.
///
/// rastro not reading a system some other distribution uses is a limit of rastro, never
/// a failure of the host, so it is reported as state and not as an error.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RepositoryInventory(BTreeMap<RepositorySystem, Option<RepositorySet>>);

impl RepositoryInventory {
    /// Files what was found under each system's name, and fills in the rest.
    ///
    /// The caller passes only the systems it found; completeness is this type's job,
    /// because this type is where it is documented.
    ///
    /// A repeated system is refused rather than overwritten, the rule every keyed
    /// collection here follows. Nothing can produce one today, and that is the point:
    /// if it ever does, one system's repositories would vanish from a document
    /// claiming to be complete.
    pub fn new(
        found: impl IntoIterator<Item = (RepositorySystem, RepositorySet)>,
    ) -> Result<Self, CollectionError> {
        let mut inventory: BTreeMap<RepositorySystem, Option<RepositorySet>> =
            RepositorySystem::ALL
                .into_iter()
                .map(|system| (system, None))
                .collect();

        for (system, set) in found {
            if inventory.insert(system, Some(set)).flatten().is_some() {
                return Err(CollectionError::new(format!(
                    "the {} repository system was reported twice",
                    system.as_str()
                )));
            }
        }

        Ok(Self(inventory))
    }

    pub fn sets(&self) -> &BTreeMap<RepositorySystem, Option<RepositorySet>> {
        &self.0
    }
}

impl From<&RepositoryInventory> for Observation {
    fn from(inventory: &RepositoryInventory) -> Self {
        Observation::object(inventory.sets().iter().map(|(system, set)| {
            let reported = match set {
                Some(set) => Observation::from(set),
                // `null`, not the word "absent": a key sometimes text and sometimes a
                // list is awkward for every consumer.
                None => Observation::null(),
            };

            (system.as_str(), reported)
        }))
    }
}
