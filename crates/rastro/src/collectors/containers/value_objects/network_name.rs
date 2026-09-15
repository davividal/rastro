//! The name of a network a container is attached to.

use rastro_collector::{CollectionError, NonEmptyText};

use crate::collectors::containers::value_objects::single_word::single_word;

/// A network's name, which is what both the operator and the other containers know it by.
///
/// The name is also how a container resolves its neighbours, so it is the key: on a compose
/// network, `db` reaches `web` by name over the network named in the file, and that triple is
/// the state worth diffing.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct NetworkName(NonEmptyText);

impl NetworkName {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(single_word(value, "network name")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}
