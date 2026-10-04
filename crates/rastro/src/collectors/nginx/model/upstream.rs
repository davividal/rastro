//! A pool of servers requests are shared between.

use std::collections::BTreeMap;

use rastro_collector::Observation;

use crate::collectors::nginx::model::UpstreamServer;
use crate::collectors::nginx::value_objects::UpstreamName;

/// An `upstream` block: its name, its members, and how it balances between them.
///
/// **Members keep the order written, and the settings are a map.** Round-robin walks the
/// members in order, and `hash` and `ip_hash` map a key to a member by its position, so a
/// member moved from the top to the bottom moves traffic. The settings are directives nginx
/// reads by name, so their order is not state.
///
/// `settings` carries every directive in the block that is not a `server`, verbatim: the
/// balancing method (`least_conn`, `ip_hash`, `hash`), `keepalive`, `zone`, and whatever a
/// module adds next. Naming them one at a time would leave the facet silent about the one
/// nginx gains after this was written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Upstream {
    pub name: UpstreamName,
    pub servers: Vec<UpstreamServer>,
    pub settings: BTreeMap<String, String>,
}

impl From<&Upstream> for Observation {
    fn from(upstream: &Upstream) -> Self {
        Observation::object([
            ("name", Observation::from(&upstream.name)),
            (
                "servers",
                Observation::sequence(upstream.servers.iter().map(Observation::from)),
            ),
            (
                "settings",
                Observation::object(
                    upstream
                        .settings
                        .iter()
                        .map(|(name, value)| (name.as_str(), Observation::text(value.clone()))),
                ),
            ),
        ])
    }
}
