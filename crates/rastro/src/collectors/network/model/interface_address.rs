//! One address on one interface.

use rastro_collector::Observation;

use crate::collectors::network::value_objects::{
    AddressFamily, AddressLifetime, AddressScope, IpAddress, PrefixLength,
};

/// An address assigned to an interface.
///
/// **`dynamic` and the lifetime say overlapping but different things, and both are kept.**
/// `dynamic` is the kernel's flag for an address a protocol installed rather than an
/// operator, and it is stable. The lifetime says how long that address has left, and only
/// its boolean half is. An address can be dynamic with a lifetime of forever, which is what
/// a DHCP server offering an infinite lease produces.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct InterfaceAddress {
    pub family: AddressFamily,
    pub local: IpAddress,
    pub prefix_length: PrefixLength,
    /// Absent for the address families `ip` reports without one.
    pub scope: Option<AddressScope>,
    /// Whether a protocol installed this address rather than an operator.
    pub dynamic: bool,
    pub valid_lifetime: AddressLifetime,
    pub preferred_lifetime: AddressLifetime,
}

impl From<&InterfaceAddress> for Observation {
    fn from(address: &InterfaceAddress) -> Self {
        Observation::object([
            ("dynamic", Observation::boolean(address.dynamic)),
            ("family", Observation::from(&address.family)),
            ("local", Observation::from(&address.local)),
            (
                "preferred_lifetime",
                Observation::from(&address.preferred_lifetime),
            ),
            ("prefix_length", Observation::from(&address.prefix_length)),
            (
                "scope",
                address
                    .scope
                    .as_ref()
                    .map_or_else(Observation::null, Observation::from),
            ),
            ("valid_lifetime", Observation::from(&address.valid_lifetime)),
        ])
    }
}
