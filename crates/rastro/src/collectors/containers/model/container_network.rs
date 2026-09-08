//! One network a container is attached to.

use rastro_collector::{NonEmptyText, Observation};

use crate::collectors::containers::value_objects::NetworkId;
use crate::collectors::inet::{HardwareAddress, IpAddress};

/// The container's end of one network, with what was asked for beside what was assigned.
///
/// **The requested addresses are kept apart from the assigned ones on purpose.** A compose
/// file naming a fixed address is a declaration; what the engine's IPAM did about it is an
/// observation. They agree almost always, and the almost is the whole reason a fingerprint
/// is taken. The same shape as the postgresql facet's configured port beside the port its
/// running postmaster reports.
///
/// **What is deliberately not here.** The endpoint id, which is a per-connection handle with
/// no meaning to an operator; the gateway and the prefix length, which are properties of the
/// network rather than of this container's end of it; and docker's `DNSNames`, which is the
/// container's name, its aliases and its own short id, all of them already in the document
/// under names that say what they are.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerNetwork {
    /// The names other containers on this network can reach it by, sorted.
    ///
    /// Plain text rather than a value object: an alias is a DNS label the operator chose,
    /// and the engine has already refused anything it would not accept.
    pub aliases: Vec<NonEmptyText>,
    /// Absent for a container that is attached and has no address on this network yet.
    pub address: Option<IpAddress>,
    pub ipv6_address: Option<IpAddress>,
    pub hardware_address: Option<HardwareAddress>,
    pub network_id: Option<NetworkId>,
    /// The static address the container asked for, absent where it asked for none.
    pub requested_address: Option<IpAddress>,
    pub requested_ipv6_address: Option<IpAddress>,
}

impl From<&ContainerNetwork> for Observation {
    fn from(network: &ContainerNetwork) -> Self {
        Observation::object([
            ("address", optional_address(network.address.as_ref())),
            (
                "aliases",
                Observation::list(
                    network
                        .aliases
                        .iter()
                        .map(|alias| Observation::text(alias.as_str())),
                ),
            ),
            (
                "hardware_address",
                match &network.hardware_address {
                    Some(address) => Observation::text(address.as_str()),
                    None => Observation::null(),
                },
            ),
            (
                "ipv6_address",
                optional_address(network.ipv6_address.as_ref()),
            ),
            (
                "network_id",
                match &network.network_id {
                    Some(id) => Observation::from(id),
                    None => Observation::null(),
                },
            ),
            (
                "requested_address",
                optional_address(network.requested_address.as_ref()),
            ),
            (
                "requested_ipv6_address",
                optional_address(network.requested_ipv6_address.as_ref()),
            ),
        ])
    }
}

fn optional_address(address: Option<&IpAddress>) -> Observation {
    match address {
        Some(address) => Observation::from(address),
        None => Observation::null(),
    }
}
