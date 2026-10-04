//! Where a node serves HTTP.

use crate::collectors::elasticsearch::value_objects::Transport;
use crate::collectors::inet::{InetHost, PortNumber};

/// The address and port a node serves HTTP on, and the protocol, as rastro will dial them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpEndpoint {
    host: InetHost,
    port: PortNumber,
    transport: Transport,
}

impl HttpEndpoint {
    /// A plain HTTP endpoint.
    pub fn new(host: InetHost, port: PortNumber) -> Self {
        Self {
            host,
            port,
            transport: Transport::Plain,
        }
    }

    /// The same endpoint, spoken to over `transport`.
    pub fn over(self, transport: Transport) -> Self {
        Self { transport, ..self }
    }

    pub fn host(&self) -> &InetHost {
        &self.host
    }

    pub fn port(&self) -> &PortNumber {
        &self.port
    }

    pub fn transport(&self) -> Transport {
        self.transport
    }
}
