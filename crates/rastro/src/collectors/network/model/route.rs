//! One route in the kernel's table.

use rastro_collector::Observation;

use crate::collectors::network::value_objects::{
    AddressScope, InterfaceName, IpAddress, RouteDestination, RoutePreference, RouteProtocol,
};

/// A route as rastro means it.
///
/// **Almost every field is optional, and that is the kernel's shape rather than
/// defensiveness.** A route to a directly attached network has no gateway; a route from a
/// router advertisement has a preference and an expiry and no scope; a route installed by
/// hand may have neither metric nor preferred source. Filling any of them in with a
/// default would put a value in the fingerprint the kernel never reported.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Route {
    pub destination: RouteDestination,
    /// Absent for a route to a directly attached network, which needs no next hop.
    pub gateway: Option<IpAddress>,
    /// Absent for a route the kernel reports without one, such as a blackhole route.
    pub device: Option<InterfaceName>,
    pub protocol: RouteProtocol,
    pub scope: Option<AddressScope>,
    pub metric: Option<i64>,
    /// The source address the kernel will use for traffic on this route.
    pub preferred_source: Option<IpAddress>,
    /// IPv6 only, from the router advertisement that installed the route.
    pub preference: Option<RoutePreference>,
    /// Seconds until a learned route is forgotten. Volatile: it counts down.
    pub expires_seconds: Option<i64>,
}

impl From<&Route> for Observation {
    fn from(route: &Route) -> Self {
        Observation::object([
            ("destination", Observation::from(&route.destination)),
            (
                "device",
                route
                    .device
                    .as_ref()
                    .map_or_else(Observation::null, Observation::from),
            ),
            (
                "expires_seconds",
                // Volatile whether present or not, so a route that starts expiring does
                // not show `null` becoming a number in the diffable view.
                route.expires_seconds.map_or_else(
                    || Observation::null().volatile(),
                    |seconds| Observation::integer(seconds).volatile(),
                ),
            ),
            (
                "gateway",
                route
                    .gateway
                    .as_ref()
                    .map_or_else(Observation::null, Observation::from),
            ),
            (
                "metric",
                route
                    .metric
                    .map_or_else(Observation::null, Observation::integer),
            ),
            (
                "preference",
                route
                    .preference
                    .as_ref()
                    .map_or_else(Observation::null, Observation::from),
            ),
            (
                "preferred_source",
                route
                    .preferred_source
                    .as_ref()
                    .map_or_else(Observation::null, Observation::from),
            ),
            ("protocol", Observation::from(&route.protocol)),
            (
                "scope",
                route
                    .scope
                    .as_ref()
                    .map_or_else(Observation::null, Observation::from),
            ),
        ])
    }
}
