//! Which scheme hashed a password.

use rastro_collector::{CollectionError, NonEmptyText, Observation};

/// The identifier at the front of a password hash, as written.
///
/// A crypt hash begins `$id$`, and this is the `id`: `y` for yescrypt, which is
/// Debian 12's default, `6` for SHA-512, `5` for SHA-256, `2b` for bcrypt, `7` for
/// scrypt, `gy` for gost-yescrypt, `1` for MD5.
///
/// **Text rather than an enum, and that is the departure from how this codebase
/// treats a closed vocabulary.** A kernel module has exactly three states because
/// the kernel's own printer has exactly three cases, so an unrecognised one there
/// proves a misread and is rightly refused. Nothing of the sort holds here: the set
/// is libcrypt's, it has grown twice in recent memory, and a distribution that
/// adopts the next scheme would make rastro refuse to fingerprint a perfectly
/// healthy box. Recording what is written keeps rastro out of the business of
/// maintaining an alphabet, the same call the package collector makes when it has
/// dpkg decode its own status words.
///
/// **Only the identifier is recorded, never the hash.** The scheme a box hashes
/// with is state worth diffing, since a release upgrade migrating SHA-512 to
/// yescrypt shows up here and nowhere else. The hash itself is a credential, and
/// rastro's redaction layer does not exist yet, so carrying one would mean writing
/// live password hashes to stdout in plain text today.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct HashAlgorithm(NonEmptyText);

impl HashAlgorithm {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(NonEmptyText::new(value, "hash algorithm")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<&HashAlgorithm> for Observation {
    fn from(algorithm: &HashAlgorithm) -> Self {
        Observation::text(algorithm.as_str())
    }
}
