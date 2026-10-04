//! The host's networking, interfaces and routes together.

use std::collections::BTreeMap;

use rastro_collector::{CollectionError, Observation};

use super::network_interface::NetworkInterface;
use super::route::Route;
use crate::collectors::network::value_objects::InterfaceName;

/// Every interface and every route.
///
/// **One facet rather than two, because a route is meaningless without the interface it
/// leaves by.** `default via 10.0.2.2 dev enp0s8` says nothing on its own about whether
/// the box can reach the internet; that depends on `enp0s8` being up and holding an
/// address. Splitting them would also let a config exclude half of an answer.
///
/// Interfaces are keyed by name and routes are a sequence in the kernel's order: an interface
/// name is unique, and a route's identity is not one rastro can key on. Two routes to the same
/// destination through different gateways are how a box is multi-homed, and among IPv4 routes
/// that `ip route append` kept under one destination and metric the kernel uses the first that
/// is alive, so the order is what says which route is the failover.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NetworkState {
    interfaces: BTreeMap<InterfaceName, NetworkInterface>,
    routes: Vec<Route>,
}

impl NetworkState {
    /// Files each interface under its name and keeps the routes in the order given.
    ///
    /// A repeated interface name is refused: the kernel cannot produce one, so it means
    /// rastro misread the output, and keeping the last of two would drop an interface from
    /// a document claiming to be complete.
    pub fn new(
        interfaces: impl IntoIterator<Item = (InterfaceName, NetworkInterface)>,
        routes: impl IntoIterator<Item = Route>,
    ) -> Result<Self, CollectionError> {
        let mut filed = BTreeMap::new();
        for (name, interface) in interfaces {
            if filed.insert(name.clone(), interface).is_some() {
                return Err(CollectionError::new(format!(
                    "the interface {:?} was reported twice, so the output was misread",
                    name.as_str()
                )));
            }
        }

        Ok(Self {
            interfaces: filed,
            routes: routes.into_iter().collect(),
        })
    }

    pub fn interfaces(&self) -> &BTreeMap<InterfaceName, NetworkInterface> {
        &self.interfaces
    }

    pub fn routes(&self) -> &[Route] {
        &self.routes
    }
}

impl From<&NetworkState> for Observation {
    fn from(state: &NetworkState) -> Self {
        Observation::object([
            (
                "interfaces",
                Observation::object(
                    state
                        .interfaces()
                        .iter()
                        .map(|(name, interface)| (name.as_str(), Observation::from(interface))),
                ),
            ),
            (
                "routes",
                Observation::sequence(state.routes().iter().map(Observation::from)),
            ),
        ])
    }
}
