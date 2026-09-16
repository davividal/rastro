//! The key of a label attached to a container.

use rastro_collector::{CollectionError, NonEmptyText};

use crate::collectors::containers::value_objects::single_word::single_word;

/// A label's key, conventionally reverse-DNS: `com.docker.compose.project`.
///
/// Worth naming because labels are how a container says who put it there. compose writes its
/// project, service and config hash into them, so for a container nobody named by hand the
/// labels are the only durable link between the box and the definition it came from.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct LabelName(NonEmptyText);

impl LabelName {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(single_word(value, "label name")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}
