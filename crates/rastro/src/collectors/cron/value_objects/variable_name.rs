//! What a crontab environment variable is called.

use rastro_collector::{CollectionError, NonEmptyText, Observation};

/// The name of a variable a crontab sets for its jobs.
///
/// `SHELL`, `PATH`, `MAILTO`. All three change what every job in the file does without
/// appearing in any job's own line, which is why they are collected rather than skipped:
/// `PATH` decides which binary a bare command name resolves to, and `MAILTO` decides whether
/// anybody hears about a failure.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct VariableName(NonEmptyText);

impl VariableName {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(NonEmptyText::new(value, "variable name")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<&VariableName> for Observation {
    fn from(name: &VariableName) -> Self {
        Observation::text(name.as_str())
    }
}
