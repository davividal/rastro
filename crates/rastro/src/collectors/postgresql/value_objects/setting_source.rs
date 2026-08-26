//! Where a setting's value came from.

use rastro_collector::{CollectionError, NonEmptyText};

/// What decided a setting's value: `default`, `configuration file`, `override`, `client`.
///
/// The most valuable column of the lot, and the reason this collector reads the server's
/// own view rather than `postgresql.conf`: it distinguishes a value somebody chose from a
/// value that is merely what this build ships with. On a default Debian cluster 21 of 379
/// settings come from the configuration file and 350 are defaults, so it is also what
/// makes the other 350 readable as background rather than as decisions.
///
/// **Validated text, not an enum.** The vocabulary belongs to the server and grows with
/// it, so a closed set here would turn a newer PostgreSQL into an error rather than an
/// observation. rastro is not the authority on what sources exist.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct SettingSource(NonEmptyText);

impl SettingSource {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(NonEmptyText::new(value, "setting source")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}
