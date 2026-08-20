//! The leaves of the network facet.

mod address_family;
mod address_lifetime;
mod address_scope;
mod hardware_address;
mod interface_flag;
mod interface_flags;
mod interface_name;
mod ip_address;
mod link_type;
mod operational_state;
mod prefix_length;
mod route_destination;
mod route_preference;
mod route_protocol;

pub use address_family::AddressFamily;
pub use address_lifetime::AddressLifetime;
pub use address_scope::AddressScope;
pub use hardware_address::HardwareAddress;
pub use interface_flag::InterfaceFlag;
pub use interface_flags::InterfaceFlags;
pub use interface_name::InterfaceName;
pub use ip_address::IpAddress;
pub use link_type::LinkType;
pub use operational_state::OperationalState;
pub use prefix_length::PrefixLength;
pub use route_destination::RouteDestination;
pub use route_preference::RoutePreference;
pub use route_protocol::RouteProtocol;
