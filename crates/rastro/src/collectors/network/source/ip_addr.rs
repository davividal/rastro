//! One object of `ip -j addr show`.

use serde::Deserialize;

use rastro_collector::CollectionError;

use crate::collectors::network::model::{InterfaceAddress, NetworkInterface};
use crate::collectors::network::value_objects::{
    AddressFamily, AddressLifetime, AddressScope, HardwareAddress, InterfaceFlag, InterfaceFlags,
    InterfaceName, IpAddress, LinkType, OperationalState, PrefixLength,
};

/// `ip`'s spelling of an interface, kept apart from rastro's meaning.
///
/// One call answers both halves: `ip -j addr show` reports every field `ip -j link show`
/// does *and* the addresses, so there is no second run and no join. That was checked
/// against the box rather than assumed, by comparing the key sets of the two outputs.
#[derive(Debug, Clone, Deserialize)]
pub struct InterfaceObject {
    ifindex: i64,
    ifname: String,
    link_type: String,
    mtu: i64,
    operstate: String,
    #[serde(default)]
    flags: Vec<String>,
    /// Absent for an interface with no link-layer address, such as a tunnel.
    #[serde(default)]
    address: Option<String>,
    #[serde(default)]
    addr_info: Vec<AddressObject>,
}

/// `ip`'s spelling of one address on an interface.
#[derive(Debug, Clone, Deserialize)]
pub struct AddressObject {
    family: String,
    local: String,
    prefixlen: u8,
    #[serde(default)]
    scope: Option<String>,
    /// `ip` omits the key entirely rather than writing `false`, which is why this is a
    /// `default` rather than a required boolean.
    #[serde(default)]
    dynamic: bool,
    valid_life_time: u32,
    preferred_life_time: u32,
}

impl InterfaceObject {
    /// Translates `ip`'s object into rastro's model.
    pub fn to_interface(&self) -> Result<(InterfaceName, NetworkInterface), CollectionError> {
        let flags = self
            .flags
            .iter()
            .map(InterfaceFlag::new)
            .collect::<Result<Vec<InterfaceFlag>, CollectionError>>()?;

        let mut addresses = self
            .addr_info
            .iter()
            .map(AddressObject::to_address)
            .collect::<Result<Vec<InterfaceAddress>, CollectionError>>()?;
        addresses.sort();

        let interface = NetworkInterface {
            index: self.ifindex,
            hardware_address: match &self.address {
                Some(address) => Some(HardwareAddress::new(address)?),
                None => None,
            },
            link_type: LinkType::new(&self.link_type)?,
            maximum_transmission_unit: self.mtu,
            operational_state: OperationalState::new(&self.operstate)?,
            flags: InterfaceFlags::new(flags),
            addresses,
        };

        Ok((InterfaceName::new(&self.ifname)?, interface))
    }
}

impl AddressObject {
    fn to_address(&self) -> Result<InterfaceAddress, CollectionError> {
        Ok(InterfaceAddress {
            family: AddressFamily::new(&self.family)?,
            local: IpAddress::new(&self.local)?,
            prefix_length: PrefixLength::new(self.prefixlen)?,
            scope: match &self.scope {
                Some(scope) => Some(AddressScope::new(scope)?),
                None => None,
            },
            dynamic: self.dynamic,
            valid_lifetime: AddressLifetime::new(self.valid_life_time),
            preferred_lifetime: AddressLifetime::new(self.preferred_life_time),
        })
    }
}
