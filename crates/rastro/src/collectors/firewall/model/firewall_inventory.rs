//! What each packet-filter interface reported.

use std::collections::BTreeMap;

use rastro_collector::{CollectionError, Observation};

use super::backend_report::BackendReport;
use crate::collectors::firewall::value_objects::FirewallBackend;

/// Every interface rastro knows how to read, and what each reported.
///
/// The same shape as the packages and repositories inventories, and for the same reason: an
/// interface rastro did not read is present as a key with a report saying so rather than
/// missing from the map, so a document that says nothing about `ip6tables_nft` cannot be
/// confused with one written before rastro could read it.
///
/// **Every key carries a status, and that is what makes the two kinds of "no rules"
/// separable.** See [`BackendReport`]: a tool that ran and found nothing is not the same
/// observation as a kernel subsystem that cannot hold rules, and neither is the same as an
/// interface rastro failed to read. The first two both mean the box filters nothing on that
/// interface; the third means it might not.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FirewallInventory(BTreeMap<FirewallBackend, BackendReport>);

impl FirewallInventory {
    pub fn new(
        found: impl IntoIterator<Item = (FirewallBackend, BackendReport)>,
    ) -> Result<Self, CollectionError> {
        let mut inventory: BTreeMap<FirewallBackend, BackendReport> = FirewallBackend::ALL
            .into_iter()
            .map(|backend| (backend, Self::unprobed(backend)))
            .collect();
        let mut reported = Vec::new();

        for (backend, report) in found {
            if reported.contains(&backend) {
                return Err(CollectionError::new(format!(
                    "the {} ruleset was reported twice",
                    backend.as_str()
                )));
            }
            reported.push(backend);
            inventory.insert(backend, report);
        }

        Ok(Self(inventory))
    }

    pub fn reports(&self) -> &BTreeMap<FirewallBackend, BackendReport> {
        &self.0
    }

    /// The placeholder for an interface nothing reported on.
    ///
    /// `detect_all` yields one source per backend, so this is reachable only when a caller
    /// hands the collector a narrower list. It is deliberately an error rather than an
    /// empty ruleset: not looking is not the same as looking and finding nothing.
    fn unprobed(backend: FirewallBackend) -> BackendReport {
        BackendReport::Unreadable(format!("rastro did not probe {}", backend.as_str()))
    }
}

impl From<&FirewallInventory> for Observation {
    fn from(inventory: &FirewallInventory) -> Self {
        Observation::object(
            inventory
                .reports()
                .iter()
                .map(|(backend, report)| (backend.as_str(), Observation::from(report))),
        )
    }
}
