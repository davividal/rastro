//! A size the host reported.

use crate::CollectionError;

/// A size in bytes.
///
/// Held as `i64` because that is the only integer the document has, and the
/// conversion is checked at construction rather than at render time: a size that
/// cannot be recorded faithfully must fail where the value came from, not silently
/// wrap into a negative number three layers later.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ByteSize(i64);

impl ByteSize {
    pub fn new(bytes: u64, kind: &str) -> Result<Self, CollectionError> {
        let bytes = i64::try_from(bytes).map_err(|_| {
            CollectionError::new(format!(
                "a {kind} of {bytes} bytes is too large to record as an integer"
            ))
        })?;

        Ok(Self(bytes))
    }

    pub fn bytes(&self) -> i64 {
        self.0
    }
}
