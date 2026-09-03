//! The web server this box would serve with.

use rastro_collector::Observation;

use crate::collectors::nginx::model::{Binary, Configuration, Upstream, VirtualHost};

/// Everything the facet reports: the binary, the configuration it would read, and what that
/// configuration says it serves.
///
/// The binary and the configuration are kept apart on purpose. A package upgrade changes the
/// binary and leaves the configuration alone; an edit does the opposite; and a reader looking
/// at a diff needs to see which of the two moved.
///
/// The hosts and the pools are a projection of the same configuration, not a second reading
/// of it: they name what the model understands, while `configuration.files` digests
/// everything each file says, modelled or not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebServer {
    pub binary: Binary,
    pub configuration: Configuration,
    pub hosts: Vec<VirtualHost>,
    pub upstreams: Vec<Upstream>,
}

impl From<&WebServer> for Observation {
    fn from(server: &WebServer) -> Self {
        Observation::object([
            ("binary", Observation::from(&server.binary)),
            ("configuration", Observation::from(&server.configuration)),
            (
                "hosts",
                Observation::list(server.hosts.iter().map(Observation::from)),
            ),
            (
                "upstreams",
                Observation::list(server.upstreams.iter().map(Observation::from)),
            ),
        ])
    }
}
