//! One member of a pool.

use rastro_collector::Observation;

use crate::collectors::nginx::value_objects::{Endpoint, ServerParameter};

/// An `upstream` `server` line: where it points, and how it is weighted.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct UpstreamServer {
    pub endpoint: Endpoint,
    pub parameters: Vec<ServerParameter>,
}

impl From<&UpstreamServer> for Observation {
    fn from(server: &UpstreamServer) -> Self {
        Observation::object([
            ("endpoint", Observation::from(&server.endpoint)),
            (
                "parameters",
                Observation::list(server.parameters.iter().map(Observation::from)),
            ),
        ])
    }
}
