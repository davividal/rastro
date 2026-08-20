//! What kind of key an entry holds.

use rastro_collector::{CollectionError, NonEmptyText, Observation};

/// A public key's algorithm, as OpenSSH names it.
///
/// `ssh-ed25519`, `ssh-rsa`, `ecdsa-sha2-nistp256`, `sk-ssh-ed25519@openssh.com` for a
/// hardware token, `ssh-dss` on a box nobody has tidied.
///
/// **Worth diffing on its own**, because the algorithm is a policy decision: a box whose keys
/// are all `ssh-ed25519` losing one to `ssh-rsa` is a step backwards that no other field
/// records.
///
/// Text rather than an enum: OpenSSH's list grows, and the `@openssh.com` suffixed names in
/// particular are added as new token types appear.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct KeyType(NonEmptyText);

/// The prefixes every OpenSSH public-key type begins with.
///
/// Used to tell a line's first field apart from an options list, which is the one genuinely
/// ambiguous thing about the `authorized_keys` grammar. No option name has ever begun with any
/// of these.
const TYPE_PREFIXES: [&str; 5] = ["ssh-", "ecdsa-", "sk-ssh-", "sk-ecdsa-", "rsa-sha2-"];

impl KeyType {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(NonEmptyText::new(value, "key type")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    /// Whether a field looks like a key type rather than an options list.
    pub fn looks_like_one(field: &str) -> bool {
        TYPE_PREFIXES.iter().any(|prefix| field.starts_with(prefix))
    }
}

impl From<&KeyType> for Observation {
    fn from(key_type: &KeyType) -> Self {
        Observation::text(key_type.as_str())
    }
}
