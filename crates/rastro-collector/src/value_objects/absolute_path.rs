//! A path as the host spelled it.

use crate::CollectionError;

/// A path rooted at `/`.
///
/// Held as text rather than as a `PathBuf` on purpose. A `PathBuf` invites
/// `exists()` and `canonicalize()`, which read the host, and this crate must stay
/// buildable and testable on a machine that is not the target platform. It is also
/// what the document stores: a fingerprint records the path the host reported, not
/// the one this machine could resolve.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AbsolutePath(String);

impl AbsolutePath {
    /// Reads the value, naming the kind of field so a failure says what was being
    /// read.
    ///
    /// Leading `/` is the whole invariant, and it implies non-emptiness. Where the
    /// fields of a host interface are positional, a relative value is the signal
    /// that the line was tokenised into the wrong slots.
    pub fn new(value: impl Into<String>, kind: &str) -> Result<Self, CollectionError> {
        let value = value.into();

        if !value.starts_with('/') {
            return Err(CollectionError::new(format!(
                "a {kind} is an absolute path, but the host reported {value:?}"
            )));
        }

        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}
