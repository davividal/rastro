//! What nginx serves over HTTP.

use rastro_collector::Observation;

use crate::collectors::nginx::model::{Upstream, VirtualHost};

/// The `http` context: the hosts it answers as, and the pools they send to.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HttpService {
    pub hosts: Vec<VirtualHost>,
    pub upstreams: Vec<Upstream>,
}

impl From<&HttpService> for Observation {
    fn from(service: &HttpService) -> Self {
        Observation::object([
            (
                "hosts",
                Observation::list(service.hosts.iter().map(Observation::from)),
            ),
            (
                "upstreams",
                Observation::list(service.upstreams.iter().map(Observation::from)),
            ),
        ])
    }
}
