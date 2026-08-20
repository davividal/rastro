//! What a localisation setting is called.

use rastro_collector::{CollectionError, NonEmptyText, Observation};

/// The name of a localisation variable.
///
/// `LANG`, `LC_TIME`, `LC_MESSAGES`, `KEYMAP`, `FONT`, `XKBLAYOUT`. Upper case by
/// convention, and rastro keeps whatever case the file used rather than normalising: a
/// variable the shell would not export because somebody wrote it in lower case is a real
/// misconfiguration, and folding the case would hide it.
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

impl From<&SettingName> for Observation {
    fn from(name: &SettingName) -> Self {
        Observation::text(name.as_str())
    }
}
