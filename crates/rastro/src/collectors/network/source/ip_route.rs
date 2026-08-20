//! One object of `ip -j route show`.

use serde::Deserialize;

use rastro_collector::CollectionError;

use crate::collectors::network::model::Route;
use crate::collectors::network::value_objects::{
    AddressScope, InterfaceName, IpAddress, RouteDestination, RoutePreference, RouteProtocol,
};

/// `ip`'s spelling of a route, kept apart from rastro's meaning.
///
/// Every field but the destination and the protocol is optional, which is the kernel's
/// shape: the two families report overlapping but different sets. An IPv4 route carries a
/// `scope` and a `prefsrc`; an IPv6 route from a router advertisement carries a `pref` and
/// an `expires` and neither of the other two.
#[derive(Debug, Clone, Deserialize)]
pub struct RouteObject {
    dst: String,
    protocol: String,
    #[serde(default)]
    gateway: Option<String>,
    #[serde(default)]
    dev: Option<String>,
    #[serde(default)]
    scope: Option<String>,
    #[serde(default)]
    metric: Option<i64>,
    #[serde(default)]
    prefsrc: Option<String>,
    #[serde(default)]
    pref: Option<String>,
    #[serde(default)]
    expires: Option<i64>,
    /// Read and deliberately unused. `ip` writes an empty array here for every ordinary
    /// route; the values it can hold (`dead`, `linkdown`, `onlink`) describe a route's
    /// health rather than its configuration. Named so a reader comparing this struct
    /// against `ip`'s output does not think the field was missed.
    #[serde(default, rename = "flags")]
    _flags: Vec<String>,
}

impl RouteObject {
    /// Translates `ip`'s object into rastro's model.
    pub fn to_route(&self) -> Result<Route, CollectionError> {
        Ok(Route {
            destination: RouteDestination::new(&self.dst)?,
            gateway: match &self.gateway {
                Some(gateway) => Some(IpAddress::new(gateway)?),
                None => None,
            },
            device: match &self.dev {
                Some(device) => Some(InterfaceName::new(device)?),
                None => None,
            },
            protocol: RouteProtocol::new(&self.protocol)?,
            scope: match &self.scope {
                Some(scope) => Some(AddressScope::new(scope)?),
                None => None,
            },
            metric: self.metric,
            preferred_source: match &self.prefsrc {
                Some(source) => Some(IpAddress::new(source)?),
                None => None,
            },
            preference: match &self.pref {
                Some(preference) => Some(RoutePreference::new(preference)?),
                None => None,
            },
            expires_seconds: self.expires,
        })
    }
}
