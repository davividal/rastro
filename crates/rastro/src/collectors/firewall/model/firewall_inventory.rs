//! What each packet-filter interface reported.

use std::collections::BTreeMap;

use rastro_collector::{CollectionError, Observation};

use super::ruleset::Ruleset;
use crate::collectors::firewall::value_objects::FirewallBackend;

/// Every interface rastro knows how to read, and what each reported.
///
/// The same shape as the packages and repositories inventories, and for the same reason: an
/// interface rastro did not find is present as a key with no ruleset rather than missing
/// from the map, so a document that says nothing about `ip6tables` cannot be confused with
/// one written before rastro could read it.
///
/// **A `null` and an empty object are different answers here, and the difference matters
/// more than usual.** `null` means the tool is not on the box. An empty object means the
/// tool ran and the box filters nothing. Reading the first as the second would report an
/// unprotected box as an unknown one, or worse, the other way round.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FirewallInventory(BTreeMap<FirewallBackend, Option<Ruleset>>);

impl FirewallInventory {
    pub fn new(
        found: impl IntoIterator<Item = (FirewallBackend, Ruleset)>,
    ) -> Result<Self, CollectionError> {
        let mut inventory: BTreeMap<FirewallBackend, Option<Ruleset>> = FirewallBackend::ALL
            .into_iter()
            .map(|backend| (backend, None))
            .collect();

        for (backend, ruleset) in found {
            if inventory.insert(backend, Some(ruleset)).flatten().is_some() {
                return Err(CollectionError::new(format!(
                    "the {} ruleset was reported twice",
                    backend.as_str()
                )));
            }
        }

        Ok(Self(inventory))
    }

    pub fn rulesets(&self) -> &BTreeMap<FirewallBackend, Option<Ruleset>> {
        &self.0
    }
}

impl From<&FirewallInventory> for Observation {
    fn from(inventory: &FirewallInventory) -> Self {
        Observation::object(inventory.rulesets().iter().map(|(backend, ruleset)| {
            let reported = match ruleset {
                Some(ruleset) => Observation::from(ruleset),
                None => Observation::null(),
            };

            (backend.as_str(), reported)
        }))
    }
}
