//! How a role's password is stored, never what it is.

use rastro_collector::CollectionError;

/// The scheme a role's password is verified with.
///
/// **This is the whole of what rastro records about a password**, and the distinction it
/// buys is worth having: an `md5` password on a modern cluster means the role predates the
/// move to SCRAM, or that `password_encryption` was wrong when it was set. That is drift a
/// fingerprint should show, and it needs no part of the secret to show it.
///
/// Derived in the query rather than parsed from the stored value here, so the hash itself
/// never leaves the server.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PasswordMethod {
    ScramSha256,
    Md5,

    /// There is a password, in a form this version of rastro does not name.
    ///
    /// Recorded rather than treated as absent, because "has a password" and "has no
    /// password" are different states and only one of them lets a role log in with one.
    Unrecognised,
}

impl PasswordMethod {
    /// Reads the name the collector's own `CASE` produces.
    pub fn of(value: &str) -> Result<Self, CollectionError> {
        match value {
            "scram-sha-256" => Ok(Self::ScramSha256),
            "md5" => Ok(Self::Md5),
            "unrecognised" => Ok(Self::Unrecognised),
            other => Err(CollectionError::new(format!(
                "{other:?} is not a password method this collector's query produces"
            ))),
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ScramSha256 => "scram-sha-256",
            Self::Md5 => "md5",
            Self::Unrecognised => "unrecognised",
        }
    }
}
