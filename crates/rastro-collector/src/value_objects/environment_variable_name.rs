//! What an environment variable is called.

use rastro_fingerprint::Observation;

use crate::CollectionError;

use super::non_empty_text::NonEmptyText;

/// The name half of an environment variable, wherever the box sets one.
///
/// Shared because a box has several places that set environment and a diff wants them
/// spelled alike: a crontab's `PATH` and a unit's `Environment=PATH` are the same concept
/// reported by two collectors, and a facet that invented its own spelling would make the
/// two incomparable in one document.
///
/// **The name is kept as the host spells it, and no character rule is imposed.** POSIX
/// reserves upper case for the shell's own variables and `execve(2)` accepts any byte but
/// `=` and NUL, so a rule stricter than "not empty" would refuse a name that is really
/// there. Recording what is on the box is the job; judging it is not.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct EnvironmentVariableName(NonEmptyText);

impl EnvironmentVariableName {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(NonEmptyText::new(value, "environment variable name")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<&EnvironmentVariableName> for Observation {
    fn from(name: &EnvironmentVariableName) -> Self {
        Observation::text(name.as_str())
    }
}
