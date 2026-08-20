//! Who installed a route.

use rastro_collector::{CollectionError, NonEmptyText, Observation};

/// The protocol that installed a route, in the word `ip` prints.
///
/// `kernel` for one implied by an address, `dhcp` for one from a lease, `ra` for one from an
/// IPv6 router advertisement, `static` or `boot` for one configured by hand.
///
/// **This is the field that says whether a route is configuration or weather.** A `static`
/// route appearing is a change somebody made; an `ra` route appearing is the network telling
/// the box something.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct RouteProtocol(NonEmptyText);

impl RouteProtocol {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(NonEmptyText::new(value, "route protocol")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<&RouteProtocol> for Observation {
    fn from(value: &RouteProtocol) -> Self {
        Observation::text(value.as_str())
    }
}
