//! How a network hands out addresses.

use std::collections::BTreeMap;

use rastro_collector::{NonEmptyText, Observation};

use crate::collectors::containers::model::AddressPool;

/// The IPAM driver and the pools it was given.
///
/// A list rather than a single pool, because a dual-stack network has one per family and
/// docker allows several of either. Kept in the engine's order: these are the operator's
/// declarations in the order they were declared, and unlike a set of aliases the order is
/// the engine's own record of which pool it prefers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkAddressing {
    pub driver: NonEmptyText,
    pub configured: Vec<AddressPool>,
    pub options: BTreeMap<NonEmptyText, String>,
}

impl From<&NetworkAddressing> for Observation {
    fn from(addressing: &NetworkAddressing) -> Self {
        Observation::object([
            (
                "configured",
                Observation::list(addressing.configured.iter().map(Observation::from)),
            ),
            ("driver", Observation::text(addressing.driver.as_str())),
            (
                "options",
                Observation::object(
                    addressing
                        .options
                        .iter()
                        .map(|(name, value)| (name.as_str(), Observation::text(value))),
                ),
            ),
        ])
    }
}
