//! What a directive is called.

use rastro_collector::{CollectionError, NonEmptyText};

/// The first token of a directive.
///
/// Not checked against a catalogue of the directives nginx knows, and deliberately so: a
/// third-party module adds its own, and a collector that refused what it had not heard of
/// would report a smaller configuration than the box is running.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct DirectiveName(NonEmptyText);

impl DirectiveName {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(NonEmptyText::new(value, "directive name")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}
