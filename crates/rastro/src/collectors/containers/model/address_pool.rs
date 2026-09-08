//! One block of addresses a network hands out.

use rastro_collector::{NonEmptyText, Observation};

use crate::collectors::inet::IpAddress;

/// A subnet, and the two optional values that narrow how it is used.
///
/// **The subnet is what a container's address has to fall inside**, which makes this the
/// half of a network that decides whether the addresses recorded on every container
/// attached to it make sense. A network recreated with a different subnet moves every
/// container on it, and that is one line here against one line per container.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddressPool {
    /// Held as text rather than as an address: a subnet is an address and a prefix
    /// together, `172.17.0.0/16`, and splitting it would invent two values where the
    /// engine reports one.
    pub subnet: NonEmptyText,
    /// The route off the network, absent where the engine did not report one.
    ///
    /// **Optional because it was measured both ways on one docker.** A network created with
    /// a subnet and nothing else came back with no gateway in this config immediately after
    /// creation, and with `172.30.0.1` in it after the daemon had restarted. So an absent
    /// gateway means the engine did not say, and must not be read as a network without one.
    pub gateway: Option<IpAddress>,
    /// The narrower range IPAM allocates from, where one was asked for.
    pub allocation_range: Option<NonEmptyText>,
}

impl From<&AddressPool> for Observation {
    fn from(pool: &AddressPool) -> Self {
        Observation::object([
            (
                "allocation_range",
                match &pool.allocation_range {
                    Some(range) => Observation::text(range.as_str()),
                    None => Observation::null(),
                },
            ),
            (
                "gateway",
                match &pool.gateway {
                    Some(gateway) => Observation::from(gateway),
                    None => Observation::null(),
                },
            ),
            ("subnet", Observation::text(pool.subnet.as_str())),
        ])
    }
}
