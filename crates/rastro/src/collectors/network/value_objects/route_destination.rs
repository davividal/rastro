//! Where a route leads.

use rastro_collector::{CollectionError, NonEmptyText, Observation};

/// A route's destination.
///
/// A prefix such as `10.0.2.0/24`, a bare host address, or the literal word `default`, which
/// is what `ip` prints for `0.0.0.0/0` and `::/0`. The word is kept rather than expanded: it
/// is what `ip route` takes back, and it is what an operator scans for.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct RouteDestination(NonEmptyText);

impl RouteDestination {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(NonEmptyText::new(value, "route destination")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<&RouteDestination> for Observation {
    fn from(value: &RouteDestination) -> Self {
        Observation::text(value.as_str())
    }
}
