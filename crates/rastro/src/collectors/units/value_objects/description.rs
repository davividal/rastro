//! What a unit says it is for.

use rastro_collector::{CollectionError, NonEmptyText, Observation};

/// The one-line description the unit file gives itself.
///
/// State rather than decoration: it comes from the unit file's `Description=`, so it
/// changes when the unit file changes, and it is what makes a diff of this facet
/// readable to somebody who does not recognise a unit by name.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Description(NonEmptyText);

impl Description {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(NonEmptyText::new(value, "description")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<&Description> for Observation {
    fn from(value: &Description) -> Self {
        Observation::text(value.as_str())
    }
}
