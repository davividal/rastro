//! Which machine a package was built for.

use rastro_collector::{CollectionError, NonEmptyText};

/// A package's architecture: `amd64`, `aarch64`, or `all` for one that needs none.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Architecture(NonEmptyText);

impl Architecture {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(NonEmptyText::new(value, "architecture")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}
