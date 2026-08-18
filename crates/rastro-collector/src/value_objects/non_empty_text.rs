//! Text the host actually reported.

use crate::CollectionError;

/// A text value that is present rather than blank.
///
/// The rule every collector needs and for the same reason: a host does not report
/// a nameless anything, so a blank field means the line was tokenised into the
/// wrong slots. Catching it here turns a silent misread into a recorded failure.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NonEmptyText(String);

impl NonEmptyText {
    /// Reads the value, naming the kind of field so a failure says what was being
    /// read rather than only that something was empty.
    pub fn new(value: impl Into<String>, kind: &str) -> Result<Self, CollectionError> {
        let value = value.into();

        if value.is_empty() {
            return Err(CollectionError::new(format!("a {kind} cannot be empty")));
        }

        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}
