//! One resolved setting value.
//!
//! Several collectors keep setting values as opaque text rather than re-typing them from one
//! observed spelling. The collector that knows the setting's semantics can interpret it further
//! when needed; this type only guarantees that the reported text exists.

use crate::{CollectionError, Observation};

use super::NonEmptyText;

/// A non-empty value some interface reported for a setting.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct SettingValue(NonEmptyText);

impl SettingValue {
    pub fn new(value: impl Into<String>, kind: &str) -> Result<Self, CollectionError> {
        Ok(Self(NonEmptyText::new(value, kind)?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<&SettingValue> for Observation {
    fn from(value: &SettingValue) -> Self {
        Observation::text(value.as_str())
    }
}
