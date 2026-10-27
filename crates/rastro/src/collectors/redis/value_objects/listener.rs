//! A socket a server accepts connections on.

use std::fmt;

use rastro_collector::Observation;

use crate::collectors::inet::{InetHost, PortNumber};

/// Where a server listens, as the kernel reports it held.
///
/// **What is bound, not what rastro connected to.** A wildcard stays a wildcard here even though
/// rastro reaches it on loopback, because `bind 0.0.0.0` against `bind 127.0.0.1` is the
/// difference between a server the network can reach and one it cannot, and on the field host
/// it was the finding.
///
/// Renders as one string in the form a redis operator writes it, `127.0.0.1:6379`,
/// `[::1]:6379`, rather than as an object: a list of these is read at a glance.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Listener {
    pub host: InetHost,
    pub port: PortNumber,
}

impl fmt::Display for Listener {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        // The one family whose address holds colons, so the only one that needs brackets.
        match self.host.as_str().contains(':') {
            true => write!(formatter, "[{}]:{}", self.host.as_str(), self.port.as_u16()),
            false => write!(formatter, "{}:{}", self.host.as_str(), self.port.as_u16()),
        }
    }
}

impl From<&Listener> for Observation {
    fn from(listener: &Listener) -> Self {
        Observation::text(listener.to_string())
    }
}
