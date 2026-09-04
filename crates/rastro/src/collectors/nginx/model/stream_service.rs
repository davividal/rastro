//! What nginx proxies as plain TCP and UDP.

use rastro_collector::Observation;

use crate::collectors::nginx::model::{StreamServer, Upstream};

/// The `stream` context: its servers, and the pools they send to.
///
/// **A separate node from `http`, because the two are separate services.** A box can proxy
/// a database port and serve a website, and the same listen port number means different
/// things in each. Folding them together would also make a server that moved from one
/// context to the other — a real change, with entirely different handling — read as no
/// change at all.
///
/// The pools are the same model as the `http` ones: an `upstream` block is spelled and
/// weighted identically in both contexts, so it is one concept rather than two.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StreamService {
    pub servers: Vec<StreamServer>,
    pub upstreams: Vec<Upstream>,
}

impl From<&StreamService> for Observation {
    fn from(service: &StreamService) -> Self {
        Observation::object([
            (
                "servers",
                Observation::list(service.servers.iter().map(Observation::from)),
            ),
            (
                "upstreams",
                Observation::list(service.upstreams.iter().map(Observation::from)),
            ),
        ])
    }
}
