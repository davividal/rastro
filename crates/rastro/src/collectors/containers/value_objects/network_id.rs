//! The identity an engine gives a network.

use rastro_collector::{CollectionError, NonEmptyText, Observation};

/// A network's own id, as the engine minted it.
///
/// **Recorded even though the name is the key, because the name is not identity.** A network
/// destroyed and recreated under the same name is a different network, with a different
/// subnet and different neighbours, and the id is the only witness to that having happened.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct NetworkId(NonEmptyText);

impl NetworkId {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(NonEmptyText::new(value, "network id")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<&NetworkId> for Observation {
    fn from(id: &NetworkId) -> Self {
        Observation::text(id.as_str())
    }
}
