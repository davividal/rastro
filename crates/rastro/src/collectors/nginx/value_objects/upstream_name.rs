//! What a pool of servers is called.

use rastro_collector::{CollectionError, NonEmptyText, Observation};

/// The name an `upstream` block declares, and the name a `proxy_pass` reaches it by.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct UpstreamName(NonEmptyText);

impl UpstreamName {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(NonEmptyText::new(value, "upstream name")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<&UpstreamName> for Observation {
    fn from(name: &UpstreamName) -> Self {
        Observation::text(name.as_str())
    }
}
