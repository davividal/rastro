//! Which transport a container's port speaks.

use rastro_collector::{CollectionError, NonEmptyText, Observation};

/// The engine's own word: `tcp`, `udp`, `sctp`.
///
/// Recorded as reported, because the set belongs to the engine and nothing here branches on
/// it. It is kept apart from the port number so a port can be spelled the way the engine
/// spells it, `80/tcp`, without that string being the only thing rastro holds.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct TransportProtocol(NonEmptyText);

impl TransportProtocol {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(NonEmptyText::new(value, "transport protocol")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<&TransportProtocol> for Observation {
    fn from(protocol: &TransportProtocol) -> Self {
        Observation::text(protocol.as_str())
    }
}
