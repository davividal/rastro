//! The vocabulary of an internet endpoint, shared across facets.
//!
//! A host and a port are one concept each, and more than one facet spells them: `sockets`
//! reports what is bound on this box, `exporters` reports the address a telemetry agent
//! was configured to serve on. Giving each its own newtype would break the
//! one-term-per-concept rule at the place a reader would most notice it, since the whole
//! point of reading both facets together is to see whether a configured address and a
//! bound one agree.
//!
//! Shared *here* rather than in `rastro-collector`: the port an outside collector author
//! depends on carries what every collector spells, and a TCP port is common but not
//! universal. The same reasoning, and the same shape, as
//! [`systemd`](super::systemd) and [`canonical_tool`](super::canonical_tool).

mod inet_host;
mod port_number;

pub use inet_host::InetHost;
pub use port_number::PortNumber;
