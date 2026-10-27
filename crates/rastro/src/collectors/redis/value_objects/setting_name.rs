//! The name of one server setting.

use rastro_collector::{CollectionError, NonEmptyText};

/// The settings whose value is a credential, arriving in plain text.
///
/// **Named rather than sniffed.** A value is never judged by how it looks, and a name that merely
/// sounds like a secret is not one: `masteruser` names the account a replica authenticates as,
/// which is whose secret `masterauth` is, and is carried as it stands. The TLS pair are the
/// passphrases of the key files, present on Debian's build as well as Alpine's. `primaryauth` is
/// valkey's name for `masterauth` from 8.1, answered beside it with the same value, measured on
/// 8.1 and 9.1; a release that renames a credential again has to be added here by name.
const CREDENTIALS: [&str; 5] = [
    "requirepass",
    "masterauth",
    "primaryauth",
    "tls-key-file-pass",
    "tls-client-key-file-pass",
];

/// What separates a module's name from its setting's, in a setting the module API registered.
const MODULE_SEPARATOR: char = '.';

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
    ///
    /// **Or may be one: every setting a module registered**, named `<module>.<name>` by the module
    /// API, found by review: a module can take a credential as a setting, and which of its settings
    /// do is the module's own business. redis 8's bundled modules name theirs like core settings,
    /// `search-timeout`, and none of them is a credential, measured.
    pub fn holds_credential(&self) -> bool {
        CREDENTIALS.contains(&self.as_str()) || self.as_str().contains(MODULE_SEPARATOR)
    }
}
