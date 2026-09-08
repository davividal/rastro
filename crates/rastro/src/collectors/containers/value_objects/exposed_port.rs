//! A port a container listens on.

use rastro_collector::CollectionError;

use crate::collectors::containers::value_objects::TransportProtocol;
use crate::collectors::inet::PortNumber;

/// A port and its transport, which together are what the engine keys its port table by.
///
/// **Two values, one spelling.** `80/tcp` is the engine's own name for a port and the one an
/// operator reads out of `docker ps`, so it is what the document keys on; holding the number
/// and the protocol separately is what stops that string being the only thing rastro has, and
/// gives the map an order by port rather than by the text of one.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ExposedPort {
    number: PortNumber,
    protocol: TransportProtocol,
}

impl ExposedPort {
    pub fn new(number: PortNumber, protocol: TransportProtocol) -> Self {
        Self { number, protocol }
    }

    /// Reads the engine's `<port>/<protocol>` spelling.
    ///
    /// A key with no protocol is refused rather than defaulted to tcp: the engine always
    /// writes both, so its absence means this is not the table rastro thinks it is, and
    /// guessing would record a udp port as a tcp one.
    pub fn parse(key: &str) -> Result<Self, CollectionError> {
        let Some((number, protocol)) = key.split_once('/') else {
            return Err(CollectionError::new(format!(
                "the engine reported the port {key:?}, which names no transport, so the port \
                 table was misread"
            )));
        };

        Ok(Self::new(
            PortNumber::parse(number)?,
            TransportProtocol::new(protocol)?,
        ))
    }

    /// The engine's spelling, which is the document's key.
    pub fn as_key(&self) -> String {
        format!("{}/{}", self.number.as_u16(), self.protocol.as_str())
    }
}
