//! What an extension is called.

use rastro_collector::{CollectionError, NonEmptyText};

/// The name of an extension installed in a database.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ExtensionName(NonEmptyText);

impl ExtensionName {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(NonEmptyText::new(value, "extension name")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}
