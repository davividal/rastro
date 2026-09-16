//! What an environment variable is called.

use crate::CollectionError;

use super::non_empty_text::NonEmptyText;

/// The name half of an environment variable, wherever on the box one is set.
///
/// Shared because a box has several places that set environment and a diff wants them
/// spelled alike: a crontab's `PATH`, a unit's `Environment=PATH` and a container's `PATH`
/// are one concept reported by three collectors, and a facet that invented its own spelling
/// would make them incomparable inside one document.
///
/// **The name is public and the value never is**, which is the split this type exists to
/// make: a diff saying `PGPASSWORD` changed is exactly what an operator needs, and the new
/// password is exactly what they must not be handed.
///
/// **Kept as the host spells it, with one exception.** POSIX reserves upper case for the
/// shell's own variables and `execve(2)` accepts any byte but `=` and NUL, so a rule
/// stricter than that would refuse a name that is really there — recording what is on the
/// box is the job, and judging it is not. `=` is the exception, and it is not a style rule:
/// it is the separator, so a name holding one means the entry was split in the wrong place
/// and the value on either side of it is untrustworthy.
///
/// A collector whose own source is stricter enforces that where it parses, not here. The
/// systemd environment-file reader is the worked example: systemd sets nothing from a name
/// that is not a C identifier, so `1BAD=y` is dropped there, while a container engine
/// reporting the same name is reporting something the process really has.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct EnvironmentVariableName(NonEmptyText);

impl EnvironmentVariableName {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        let text = NonEmptyText::new(value, "environment variable name")?;

        if text.as_str().contains('=') {
            return Err(CollectionError::new(format!(
                "the environment variable name {:?} holds the separator, so the entry was \
                 split in the wrong place",
                text.as_str()
            )));
        }

        Ok(Self(text))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}
