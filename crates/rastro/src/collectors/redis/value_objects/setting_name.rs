//! The name of one server setting.

use rastro_collector::{CollectionError, NonEmptyText};

/// The settings whose value is a credential, arriving in plain text.
///
/// **Named rather than sniffed.** A value is never judged by how it looks, and a name that merely
/// sounds like a secret is not one: `masteruser` names the account a replica authenticates as,
/// which is whose secret `masterauth` is, and is carried as it stands. The TLS pair are the
/// passphrases of the key files, present on Debian's build as well as Alpine's.
const CREDENTIALS: [&str; 4] = [
    "requirepass",
    "masterauth",
    "tls-key-file-pass",
    "tls-client-key-file-pass",
];

/// A setting's name, as `CONFIG GET` reports it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct SettingName(NonEmptyText);

impl SettingName {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(NonEmptyText::new(value, "setting name")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    /// Whether this setting's value is a credential, which the document must not print as it
    /// stands.
    pub fn holds_credential(&self) -> bool {
        CREDENTIALS.contains(&self.as_str())
    }
}
