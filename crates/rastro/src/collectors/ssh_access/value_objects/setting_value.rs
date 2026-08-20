//! One resolved sshd setting.

use rastro_collector::{CollectionError, NonEmptyText, Observation};

/// A value `sshd -T` reported for a setting.
///
/// `yes`, `no`, `without-password`, `prohibit-password`, `none`.
///
/// **Kept as the word sshd resolved it to rather than as a boolean**, and `PermitRootLogin` is
/// why: its four legal values are `yes`, `no`, `prohibit-password` and `forced-commands-only`,
/// and only the first two are booleans. Flattening them would erase the distinction between a
/// box where root can log in with a key and one where root cannot log in at all.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct SettingValue(NonEmptyText);

impl SettingValue {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(NonEmptyText::new(value, "sshd setting")?))
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
