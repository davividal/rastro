//! The name of a module parameter.

use rastro_collector::{CollectionError, NonEmptyText};

/// A parameter's name, as the kernel names its file.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ParameterName(NonEmptyText);

impl ParameterName {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(NonEmptyText::new(value, "module parameter")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}
