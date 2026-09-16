//! The vocabulary of an internet endpoint, shared across facets.
//!
//! A host and a port are one concept each, and more than one facet spells them: `sockets`
//! reports what is bound on this box, `exporters` reports the address a telemetry agent
//! was configured to serve on.
//!
//! An address and a hardware address moved here for the same reason and by the same rule:
//! `network` reports what the kernel assigned to an interface, and `containers` reports what
//! the engine assigned to a container on one of its networks. Reading the two together to
//! see whether a container's address is on the bridge the host thinks it is only works if
//! both facets spell an address the same way. Giving each its own newtype would break the
//! one-term-per-concept rule at the place a reader would most notice it, since the whole
//! point of reading both facets together is to see whether a configured address and a
//! bound one agree.
//!
//! Shared *here* rather than in `rastro-collector`: the port an outside collector author
//! depends on carries what every collector spells, and a TCP port is common but not
//! universal. The same reasoning, and the same shape, as
//! [`systemd`](super::systemd) and [`canonical_tool`](super::canonical_tool).

mod hardware_address;
mod inet_host;
mod ip_address;
mod port_number;

pub use hardware_address::HardwareAddress;
pub use inet_host::InetHost;
pub use ip_address::IpAddress;
pub use port_number::PortNumber;
