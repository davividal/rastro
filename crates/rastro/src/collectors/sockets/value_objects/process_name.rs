//! Which program holds a socket open.

use rastro_collector::{CollectionError, NonEmptyText, Observation};

/// A process's name as the kernel records it.
///
/// **Truncated to fifteen characters, and rastro records the truncation rather than
/// hiding it.** The name comes from the kernel's `comm`, which is `TASK_COMM_LEN` bytes
/// wide, so the development box reports `systemd_exporte`, `process-exporte` and
/// `postgres_export`. Padding them out to a guess at the real name would put a program
/// in the fingerprint that no interface reported, and the truncated form is stable, which
/// is what a diff needs.
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
