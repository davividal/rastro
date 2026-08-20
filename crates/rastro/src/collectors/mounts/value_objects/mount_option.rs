//! One thing a mount was mounted with.

use rastro_collector::{CollectionError, NonEmptyText};

/// A flag or a `key=value` pair.
///
/// The value is kept whole rather than split on `=`:
/// `context="system_u:object_r:container_file_t:s0:c132,c369"` is one option whose
/// value carries both a comma and a colon, and taking it apart would invent
/// structure the host does not report.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct MountOption(NonEmptyText);

impl MountOption {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(NonEmptyText::new(value, "mount option")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}
