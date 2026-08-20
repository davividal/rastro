//! One interface, and what is on it.

use rastro_collector::Observation;

use super::interface_address::InterfaceAddress;
use crate::collectors::network::value_objects::{
    HardwareAddress, InterfaceFlags, LinkType, OperationalState,
};

/// An interface as rastro means it.
///
/// The name is not a field here, because it is the key this is filed under.
///
/// **The index is recorded and it is the one field here whose churn is arguable.** It is
/// assigned in device-enumeration order at boot, so a reboot that brings devices up in a
/// different order renumbers interfaces without anybody changing anything. It is kept
/// stable rather than marked volatile because the renumbering is itself worth seeing: an
/// interface that swapped index with another is a real difference in what the box will do
/// with rules written against an index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkInterface {
    pub index: i64,
    /// Absent for an interface with no link-layer address at all, such as a tunnel.
    pub hardware_address: Option<HardwareAddress>,
    pub link_type: LinkType,
    pub maximum_transmission_unit: i64,
    pub operational_state: OperationalState,
    pub flags: InterfaceFlags,
    /// Sorted, so the order the kernel happened to list them in never reaches the
    /// document. An interface with no addresses at all is ordinary: a bridge port has
    /// none.
    pub addresses: Vec<InterfaceAddress>,
}

impl From<&NetworkInterface> for Observation {
    fn from(interface: &NetworkInterface) -> Self {
        Observation::object([
            (
                "addresses",
                Observation::list(interface.addresses.iter().map(Observation::from)),
            ),
            ("flags", Observation::from(&interface.flags)),
            (
                "hardware_address",
                interface
                    .hardware_address
                    .as_ref()
                    .map_or_else(Observation::null, Observation::from),
            ),
            ("index", Observation::integer(interface.index)),
            ("link_type", Observation::from(&interface.link_type)),
            (
                "maximum_transmission_unit",
                Observation::integer(interface.maximum_transmission_unit),
            ),
            (
                "operational_state",
                Observation::from(&interface.operational_state),
            ),
        ])
    }
}
