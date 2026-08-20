//! An IPv6 route's advertised preference.

use rastro_collector::{CollectionError, NonEmptyText, Observation};

/// The preference an IPv6 route was advertised with.
///
/// `low`, `medium` or `high`, from the router advertisement. IPv4 routes have no equivalent,
/// so the field is absent for them rather than defaulted to a value the kernel never
/// reported.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct RoutePreference(NonEmptyText);

impl RoutePreference {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(NonEmptyText::new(value, "route preference")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<&RoutePreference> for Observation {
    fn from(value: &RoutePreference) -> Self {
        Observation::text(value.as_str())
    }
}
