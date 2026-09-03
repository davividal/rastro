//! A name a virtual host answers to.

use rastro_collector::{CollectionError, NonEmptyText, Observation};

/// One entry of a `server_name` directive.
///
/// Kept as nginx spells it, wildcards and regular expressions included: `*.example.org`,
/// `~^www\d+\.example\.org$` and the catch-all `_` are all legal here, and each is a
/// different rule about which requests this host answers.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ServerName(NonEmptyText);

impl ServerName {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(NonEmptyText::new(value, "server name")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<&ServerName> for Observation {
    fn from(name: &ServerName) -> Self {
        Observation::text(name.as_str())
    }
}
