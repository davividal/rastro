//! One network the engine holds.

use std::collections::BTreeMap;

use rastro_collector::{NonEmptyText, Observation};

use crate::collectors::containers::model::NetworkAddressing;
use crate::collectors::containers::value_objects::{EngineInstant, LabelName, NetworkId};

/// A network as rastro means it: what it permits, and how it addresses.
///
/// **The containers attached to it are deliberately not here.** `docker network inspect`
/// lists them, and every one of those containers already records this network from its own
/// end, with more: its aliases and the address it asked for. Recording the same edge twice
/// would give a reader two places to disagree about one fact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DockerNetwork {
    pub id: NetworkId,
    pub created: EngineInstant,
    pub driver: NonEmptyText,
    /// `local`, or `swarm` for one a cluster manages.
    pub scope: NonEmptyText,
    pub ipv6_enabled: bool,
    /// An internal network has no route off the box, which is the strongest thing a network
    /// can say about what its containers can reach.
    pub internal: bool,
    /// Whether a standalone container may join a swarm-scoped network.
    pub attachable: bool,
    /// Whether this is the swarm's ingress network, which is the one publishing a service's
    /// ports across every node.
    pub ingress: bool,
    pub addressing: NetworkAddressing,
    /// What the driver was given: for a bridge, whether containers on it may reach each
    /// other, which host address an unqualified publish binds to, and the interface name.
    pub options: BTreeMap<NonEmptyText, String>,
    pub labels: BTreeMap<LabelName, String>,
}

impl From<&DockerNetwork> for Observation {
    fn from(network: &DockerNetwork) -> Self {
        Observation::object([
            ("attachable", Observation::boolean(network.attachable)),
            ("created", Observation::from(&network.created)),
            ("driver", Observation::text(network.driver.as_str())),
            ("id", Observation::from(&network.id)),
            ("ingress", Observation::boolean(network.ingress)),
            ("internal", Observation::boolean(network.internal)),
            ("ipam", Observation::from(&network.addressing)),
            ("ipv6_enabled", Observation::boolean(network.ipv6_enabled)),
            (
                "labels",
                Observation::object(
                    network
                        .labels
                        .iter()
                        .map(|(name, value)| (name.as_str(), Observation::text(value))),
                ),
            ),
            (
                "options",
                Observation::object(
                    network
                        .options
                        .iter()
                        .map(|(name, value)| (name.as_str(), Observation::text(value))),
                ),
            ),
            ("scope", Observation::text(network.scope.as_str())),
        ])
    }
}
