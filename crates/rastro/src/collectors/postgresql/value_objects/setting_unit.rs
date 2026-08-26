//! What a setting's value is measured in.

use rastro_collector::{CollectionError, NonEmptyText};

/// The unit a numeric setting is expressed in: `8kB`, `ms`, `min`.
///
/// Non-empty, because absence is modelled by the option that holds it rather than by an
/// empty unit. Most settings have none: 303 of the 379 on a default PostgreSQL 17 cluster.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct SettingUnit(NonEmptyText);

impl SettingUnit {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(NonEmptyText::new(value, "setting unit")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}
