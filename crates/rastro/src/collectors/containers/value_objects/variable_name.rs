//! The name of an environment variable a container carries.

use rastro_collector::{CollectionError, NonEmptyText, Observation};

/// A variable's name, which is the half of an environment entry that is safe to print.
///
/// **The name is public and the value never is**, which is the split this type exists to
/// make: a diff that says `PGPASSWORD` changed is exactly what an operator needs, and the
/// new password is exactly what they must not be handed.
///
/// No `=`, because the entry is `NAME=value` and a name holding the separator means the
/// line was split in the wrong place.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct VariableName(NonEmptyText);

impl VariableName {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        let text = NonEmptyText::new(value, "environment variable name")?;

        if text.as_str().contains('=') {
            return Err(CollectionError::new(format!(
                "the engine reported the environment variable name {:?}, which holds the \
                 separator, so the entry was split in the wrong place",
                text.as_str()
            )));
        }

        Ok(Self(text))
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
