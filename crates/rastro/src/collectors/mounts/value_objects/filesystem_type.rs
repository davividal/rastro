//! Which driver serves a mount.

use rastro_collector::{CollectionError, NonEmptyText};

/// The filesystem driver behind a mount: `ext4`, `proc`, `overlay`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct FilesystemType(NonEmptyText);

impl FilesystemType {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(NonEmptyText::new(value, "filesystem type")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}
