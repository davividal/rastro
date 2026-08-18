//! What a loaded module is called.

use rastro_collector::{CollectionError, NonEmptyText};

/// The name the kernel knows a module by.
///
/// Unique across loaded modules, which the kernel enforces at load time. That
/// uniqueness is what lets the table be keyed by name rather than listed.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ModuleName(NonEmptyText);

impl ModuleName {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(NonEmptyText::new(value, "module name")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}
