//! The name of a network a container is attached to.

use rastro_collector::{CollectionError, NonEmptyText, Observation};

/// A network's name, which is what both the operator and the other containers know it by.
///
/// The name is also how a container resolves its neighbours, so it is the key: on a compose
/// network, `db` reaches `web` by name over the network named in the file, and that triple is
/// the state worth diffing.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct NetworkName(NonEmptyText);

impl NetworkName {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        let text = NonEmptyText::new(value, "network name")?;

        if text.as_str().chars().any(char::is_whitespace) {
            return Err(CollectionError::new(format!(
                "the engine reported the network name {:?}, and a name holding whitespace \
                 means the answer was misread",
                text.as_str()
            )));
        }

        Ok(Self(text))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<&NetworkName> for Observation {
    fn from(name: &NetworkName) -> Self {
        Observation::text(name.as_str())
    }
}
