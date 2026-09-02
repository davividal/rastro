//! Where a socket is bound.

use rastro_collector::Observation;

use crate::collectors::sockets::value_objects::{InetHost, PortNumber, SocketPath};

/// A listening socket's local end, in whichever shape its family has one.
///
/// **An enum rather than a struct of optional fields**, because the two shapes have
/// nothing in common: an internet socket is bound to an address and a port, a unix socket
/// to a name. A struct carrying all three would make "a unix socket with a port" and "a
/// TCP socket with a path" expressible, and an exhaustive match is what makes the
/// compiler name every site when a third family arrives.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum SocketAddress {
    Inet { host: InetHost, port: PortNumber },
    Local { path: SocketPath },
}

impl From<&SocketAddress> for Observation {
    fn from(address: &SocketAddress) -> Self {
        match address {
            SocketAddress::Inet { host, port } => Observation::object([
                ("host", Observation::from(host)),
                ("path", Observation::null()),
                ("port", Observation::from(port)),
            ]),
            // The same three keys, so a consumer never meets a key that is sometimes
            // absent. Which family it is stays readable from which keys are null.
            SocketAddress::Local { path } => Observation::object([
                ("host", Observation::null()),
                ("path", Observation::from(path)),
                ("port", Observation::null()),
            ]),
        }
    }
}
