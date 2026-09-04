//! Where a virtual host takes connections.

use rastro_collector::Observation;

use crate::collectors::nginx::value_objects::{Endpoint, ListenOption};

/// One `listen` directive: an address, and what it was switched on with.
///
/// **The configured address, which is not the bound one.** The `sockets` facet reports what
/// the kernel has; this reports what the configuration asks for. They are recorded
/// separately so that a vhost configured on a port nothing is listening on — a reload that
/// never happened, a bind that failed — shows up as the disagreement it is.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Listen {
    pub endpoint: Endpoint,
    pub options: Vec<ListenOption>,
}

impl From<&Listen> for Observation {
    fn from(listen: &Listen) -> Self {
        Observation::object([
            ("endpoint", Observation::from(&listen.endpoint)),
            (
                "options",
                Observation::list(listen.options.iter().map(Observation::from)),
            ),
        ])
    }
}
