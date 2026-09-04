//! One server block: a host this box answers as.

use rastro_collector::{NonEmptyText, Observation};

use crate::collectors::nginx::model::{
    AccessRule, Authentication, Certificate, Listen, Location, LogDestination,
};
use crate::collectors::nginx::value_objects::{AddressPattern, ServerName};

/// A virtual host, with what it listens on, what it answers to, and who may reach it.
///
/// **What is not here is as deliberate as what is.** No directive is inherited down from the
/// `http` block into these fields: a `root` set once at the top and used by every host below
/// belongs to the block that declares it, and synthesising it into each host would put
/// rastro's reading of nginx's inheritance rules into the document as though it were an
/// observation. Every directive this model does not name is still covered by its file's
/// digest, so a change to one is visible even where its meaning is not modelled.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VirtualHost {
    pub listens: Vec<Listen>,
    pub server_names: Vec<ServerName>,
    pub root: Option<NonEmptyText>,
    pub certificates: Vec<Certificate>,
    pub access: Vec<AccessRule>,
    pub logs: Vec<LogDestination>,
    pub authentication: Option<Authentication>,
    /// `set_real_ip_from`: whose `X-Forwarded-For` this host believes.
    pub trusted_proxies: Vec<AddressPattern>,
    /// `resolver`: the nameservers this host resolves upstream names with.
    pub resolvers: Vec<AddressPattern>,
    pub locations: Vec<Location>,
}

impl From<&VirtualHost> for Observation {
    fn from(host: &VirtualHost) -> Self {
        Observation::object([
            (
                "access",
                Observation::list(host.access.iter().map(Observation::from)),
            ),
            (
                "authentication",
                host.authentication
                    .as_ref()
                    .map_or_else(Observation::null, Observation::from),
            ),
            (
                "certificates",
                Observation::list(host.certificates.iter().map(Observation::from)),
            ),
            (
                "listens",
                Observation::list(host.listens.iter().map(Observation::from)),
            ),
            (
                "locations",
                Observation::list(host.locations.iter().map(Observation::from)),
            ),
            (
                "logs",
                Observation::list(host.logs.iter().map(Observation::from)),
            ),
            (
                "resolvers",
                Observation::list(host.resolvers.iter().map(Observation::from)),
            ),
            (
                "root",
                host.root
                    .as_ref()
                    .map_or_else(Observation::null, |root| Observation::text(root.as_str())),
            ),
            (
                "server_names",
                Observation::list(host.server_names.iter().map(Observation::from)),
            ),
            (
                "trusted_proxies",
                Observation::list(host.trusted_proxies.iter().map(Observation::from)),
            ),
        ])
    }
}
