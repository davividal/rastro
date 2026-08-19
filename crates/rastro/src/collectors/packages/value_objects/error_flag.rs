//! Whether a package needs attention.

use rastro_collector::{CollectionError, NonEmptyText};

/// dpkg's error flag: `ok`, or `reinstreq` for a package that must be reinstalled.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ErrorFlag(NonEmptyText);

impl ErrorFlag {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(NonEmptyText::new(value, "error flag")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}
