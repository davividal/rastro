//! Where a node serves HTTP.

use crate::collectors::inet::{InetHost, PortNumber};

/// The address and port a node serves HTTP on, as rastro will dial them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpEndpoint {
    host: InetHost,
    port: PortNumber,
}

impl HttpEndpoint {
    pub fn new(host: InetHost, port: PortNumber) -> Self {
        Self { host, port }
    }

    pub fn host(&self) -> &InetHost {
        &self.host
    }

    pub fn port(&self) -> &PortNumber {
        &self.port
    }
}
