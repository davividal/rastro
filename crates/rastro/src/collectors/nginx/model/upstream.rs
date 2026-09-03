//! A pool of servers requests are shared between.

use std::collections::BTreeMap;

use rastro_collector::Observation;

use crate::collectors::nginx::model::UpstreamServer;
use crate::collectors::nginx::value_objects::UpstreamName;

/// An `upstream` block: its name, its members, and how it balances between them.
///
/// **Members are sorted and the settings are a map, because neither order is state.** nginx
/// weighs a pool by the parameters on each line, not by which line came first, so a member
/// moved from the top to the bottom has changed nothing and must not read as a change.
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
                Observation::list(upstream.servers.iter().map(Observation::from)),
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
