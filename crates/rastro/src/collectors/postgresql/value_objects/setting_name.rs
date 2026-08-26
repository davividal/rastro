//! What a setting is called.

use rastro_collector::{CollectionError, NonEmptyText};

/// The name of a server setting, as the server spells it.
///
/// Case is part of it: `DateStyle` and `search_path` are both real names, and the server
/// is the authority on which is which.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct SettingName(NonEmptyText);

impl SettingName {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(NonEmptyText::new(value, "setting name")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}
