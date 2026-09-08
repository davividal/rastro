//! The key of a label attached to a container.

use rastro_collector::{CollectionError, NonEmptyText, Observation};

/// A label's key, conventionally reverse-DNS: `com.docker.compose.project`.
///
/// Worth naming because labels are how a container says who put it there. compose writes its
/// project, service and config hash into them, so for a container nobody named by hand the
/// labels are the only durable link between the box and the definition it came from.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct LabelName(NonEmptyText);

impl LabelName {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        let text = NonEmptyText::new(value, "label name")?;

        if text.as_str().chars().any(char::is_whitespace) {
            return Err(CollectionError::new(format!(
                "the engine reported the label name {:?}, and a key holding whitespace means \
                 the answer was misread",
                text.as_str()
            )));
        }

        Ok(Self(text))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<&LabelName> for Observation {
    fn from(name: &LabelName) -> Self {
        Observation::text(name.as_str())
    }
}
