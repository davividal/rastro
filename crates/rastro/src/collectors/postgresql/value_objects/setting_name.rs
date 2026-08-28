//! What a setting is called.

use rastro_collector::{CollectionError, NonEmptyText};

/// The name of a server setting, as the server spells it.
///
/// Case is part of it: `DateStyle` and `search_path` are both real names, and the server
/// is the authority on which is which.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct SettingName(NonEmptyText);

/// The settings whose value can carry a credential in cleartext.
///
/// `primary_conninfo` holds `password=` whenever a standby was set up inline, and the rest
/// are shell commands an operator routinely writes an inline secret into. Every one is
/// `GUC_SUPERUSER_ONLY`, so they are visible precisely in the superuser-owner case rastro
/// connects as, and the server redacts none of them. Naming them here, rather than sniffing
/// the value, keeps the judgement a property of the setting: a reader can see which names are
/// withheld and why, and a value that merely looks like a secret is not guessed at.
const CREDENTIAL_BEARING: [&str; 6] = [
    "archive_cleanup_command",
    "archive_command",
    "primary_conninfo",
    "recovery_end_command",
    "restore_command",
    "ssl_passphrase_command",
];

impl SettingName {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(NonEmptyText::new(value, "setting name")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    /// Whether this setting's value can hold a credential, and so must be redacted.
    pub fn holds_credential(&self) -> bool {
        CREDENTIAL_BEARING.contains(&self.as_str())
    }
}
