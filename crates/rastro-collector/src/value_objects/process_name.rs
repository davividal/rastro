//! What a process calls itself.
//!
//! The same concept reaches more than one collector. `/proc/<pid>/status` reports it for the
//! processes facet, and socket ownership reports the kernel's truncated `comm` for the sockets
//! facet. Both need the same guarantees: non-empty text, kept exactly as the host reported it.

use crate::{CollectionError, Observation};

use super::NonEmptyText;

/// A process's name as the kernel records it.
///
/// Truncated to fifteen characters, because that is what `TASK_COMM_LEN` allows, and rastro
/// records the truncation rather than guessing at the rest.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ProcessName(NonEmptyText);

impl ProcessName {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(NonEmptyText::new(value, "process name")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<&ProcessName> for Observation {
    fn from(name: &ProcessName) -> Self {
        Observation::text(name.as_str())
    }
}
