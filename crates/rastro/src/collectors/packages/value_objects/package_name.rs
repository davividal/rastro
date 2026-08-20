//! What a package is called.

use rastro_collector::{CollectionError, NonEmptyText};

/// The name a package manager knows a package by.
///
/// Unique within a manager, which is what lets a package set be keyed by it. dpkg
/// spells a foreign-architecture package `name:arch`, and that whole string is the
/// name.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct PackageName(NonEmptyText);

impl PackageName {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(NonEmptyText::new(value, "package name")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}
