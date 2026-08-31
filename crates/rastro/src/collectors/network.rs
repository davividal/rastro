//! How this box is attached to the network.
//!
//! Three layers, and the dependency arrows only point one way:
//! [`source`] knows [`model`], `model` knows [`value_objects`], and neither of the last
//! two knows a host interface exists.
//!
//! **The other half of the sockets facet.** That one says which ports are open; this says
//! which addresses they are open on and what the box can reach. A service bound to
//! `0.0.0.0` is exposed to exactly the networks the interfaces here are attached to.
//!
//! Both address families are asked for explicitly, because `ip route show` with no family
//! reports IPv4 only and says nothing about the omission.
pub mod model;
pub mod source;
pub mod value_objects;

pub use model::{InterfaceAddress, NetworkInterface, NetworkState, Route};
pub use source::{AddressObject, InterfaceObject, Ip, RouteObject};
pub use value_objects::{
    AddressFamily, AddressLifetime, AddressScope, HardwareAddress, InterfaceFlag, InterfaceFlags,
    InterfaceName, IpAddress, LinkType, OperationalState, PrefixLength, RouteDestination,
    RoutePreference, RouteProtocol,
};

// One import, because `rastro-collector` re-exports what an author needs. A
// collector written outside this repo looks exactly like this.
use rastro_collector::{
    CollectionError, Collector, CollectorCategory, CollectorId, CollectorIdentity,
    CollectorVersion, FacetName, Observation, Presence,
};

pub struct NetworkCollector {
    name: FacetName,
    identity: CollectorIdentity,
    ip: Option<Ip>,
}

impl NetworkCollector {
    pub fn new() -> Self {
        Self::reading(Ip::detect())
    }

    /// The same collector over a source the caller chose.
    pub fn reading(ip: Option<Ip>) -> Self {
        Self {
            name: FacetName::new("network").expect("`network` is a legal facet name"),
            identity: CollectorIdentity::new(
                CollectorId::new("network").expect("`network` is a legal collector id"),
                CollectorVersion::new("2").expect("`2` is a legal collector version"),
            ),
            ip,
        }
    }
}

impl Default for NetworkCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl Collector for NetworkCollector {
    fn name(&self) -> &FacetName {
        &self.name
    }

    fn identity(&self) -> &CollectorIdentity {
        &self.identity
    }

    fn category(&self) -> CollectorCategory {
        CollectorCategory::State
    }

    /// `undetermined` without `ip`, on the same reasoning as the sockets collector: a box
    /// with no `ip` has not stopped having interfaces, so `absent` would be a confident
    /// lie about how the box is attached to the network.
    fn presence(&self) -> Presence {
        match self.ip {
            Some(_) => Presence::Present,
            None => Presence::Undetermined {
                reason: "`ip` was not found, so this host's networking cannot be told".to_owned(),
            },
        }
    }

    fn collect(&self) -> Result<Observation, CollectionError> {
        let ip = self
            .ip
            .as_ref()
            .ok_or_else(|| CollectionError::new("`ip` was not found"))?;

        Ok(Observation::from(&ip.read()?))
    }
}
