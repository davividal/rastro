//! One server in the `stream` context: TCP and UDP, proxied without reading it as HTTP.

use rastro_collector::Observation;

use crate::collectors::nginx::model::{
    AccessRule, Certificate, Listen, LogDestination, PassTarget,
};

/// A `stream` server block.
///
/// **Not a virtual host, and shaped differently on purpose.** A stream server has no
/// `server_name` and no locations: nginx has no request to name a host with, so one listen
/// address means one backend. Modelling it with the http shape would put fields in the
/// document that the configuration cannot fill.
///
/// What it does share with a virtual host is where it listens, what it serves TLS with, who
/// may reach it and where it logs — so those are the same types, and a reader compares them
/// the same way.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamServer {
    pub listens: Vec<Listen>,
    pub pass: Option<PassTarget>,
    pub certificates: Vec<Certificate>,
    pub access: Vec<AccessRule>,
    pub logs: Vec<LogDestination>,
}

impl From<&StreamServer> for Observation {
    fn from(server: &StreamServer) -> Self {
        Observation::object([
            (
                "access",
                Observation::list(server.access.iter().map(Observation::from)),
            ),
            (
                "certificates",
                Observation::list(server.certificates.iter().map(Observation::from)),
            ),
            (
                "listens",
                Observation::list(server.listens.iter().map(Observation::from)),
            ),
            (
                "logs",
                Observation::list(server.logs.iter().map(Observation::from)),
            ),
            (
                "pass",
                server
                    .pass
                    .as_ref()
                    .map_or_else(Observation::null, Observation::from),
            ),
        ])
    }
}
