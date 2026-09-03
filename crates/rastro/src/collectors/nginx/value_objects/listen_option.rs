//! One switch on a `listen` directive.

use rastro_collector::{CollectionError, NonEmptyText, Observation};

/// Everything a `listen` says after the address: `ssl`, `http2`, `default_server`,
/// `proxy_protocol`, `backlog=511`.
///
/// Carried verbatim rather than modelled one flag at a time. `ssl` is the one that changes
/// what the port *is*, and the rest are tuning, but nginx keeps adding to the list and a
/// facet that named them individually would go quiet about the next one.
///
/// Sorted rather than kept in the written order: nginx reads them as a set, so an operator
/// swapping `ssl` and `http2` has changed nothing about the service.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ListenOption(NonEmptyText);

impl ListenOption {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(NonEmptyText::new(value, "listen option")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<&ListenOption> for Observation {
    fn from(option: &ListenOption) -> Self {
        Observation::text(option.as_str())
    }
}
