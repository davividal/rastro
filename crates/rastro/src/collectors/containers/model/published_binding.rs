//! Where a container's port is reachable from.

use rastro_collector::Observation;

use crate::collectors::inet::{InetHost, PortNumber};

/// One address and port on the host that a container's port is published on.
///
/// **The effective binding, not the requested one.** `HostConfig.PortBindings` holds what
/// was asked for and `NetworkSettings.Ports` holds what the engine did about it, and they
/// differ in the way that matters: measured on docker 26.1.5, publishing `-p 9000:9000`
/// records an empty host address in the request and resolves to `0.0.0.0` *and* `::` in the
/// result. The request would have understated the reach of the port.
///
/// The address is the shared [`InetHost`], which is also what `sockets` reports a listener
/// bound to, so the two can be read together: a port this facet says is published on
/// `0.0.0.0` and no listener to match is a different box from one where they agree.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct PublishedBinding {
    pub host_address: InetHost,
    pub host_port: PortNumber,
}

impl From<&PublishedBinding> for Observation {
    fn from(binding: &PublishedBinding) -> Self {
        Observation::object([
            ("host_address", Observation::from(&binding.host_address)),
            (
                "host_port",
                Observation::integer(i64::from(binding.host_port.as_u16())),
            ),
        ])
    }
}
