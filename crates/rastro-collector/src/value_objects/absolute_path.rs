//! A path as the host spelled it.

use crate::CollectionError;
use crate::value_objects::NonEmptyText;

/// A path rooted at `/`.
///
/// Text the host reported, with one more constraint on it, so it is built from
/// [`NonEmptyText`] rather than from a bare `String`. The non-empty check is redundant
/// against the leading `/`, and that is not the reason: composing is what makes the
/// relationship between the two structural instead of something a reader has to work out.
///
/// Text rather than a `PathBuf` on purpose. A `PathBuf` invites `exists()` and
/// `canonicalize()`, which read the host, and this crate must stay buildable and testable
/// on a machine that is not the target platform. It is also what the document stores: a
/// fingerprint records the path the host reported, not the one this machine could resolve.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AbsolutePath(NonEmptyText);

impl AbsolutePath {
    /// Reads the value, naming the kind of field so a failure says what was being read.
    ///
    /// Where the fields of a host interface are positional, a relative value is the signal
    /// that the line was tokenised into the wrong slots.
    pub fn new(value: impl Into<String>, kind: &str) -> Result<Self, CollectionError> {
        let text = NonEmptyText::new(value, kind)?;

        if !text.as_str().starts_with('/') {
            return Err(CollectionError::new(format!(
                "a {kind} is an absolute path, but the host reported {:?}",
                text.as_str()
            )));
        }

        Ok(Self(text))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}
