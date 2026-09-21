//! What a node holds that somebody declared, as opposed to what it is running.

use std::collections::BTreeMap;

use rastro_collector::Observation;

use crate::collectors::rabbitmq::model::{User, Vhost};

/// The durable half of a node: the tenancy and the accounts.
///
/// **Durable by construction, which is why this is the half worth fingerprinting.** The
/// export carries what would survive a restart, so a queue a client declared as exclusive
/// and the connections holding it are absent without rastro having to filter them out.
///
/// Keyed maps rather than lists throughout, because every one of these has a name that is
/// unique within its scope and a reader looks things up by it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Definitions {
    /// The version of the node that wrote the export, as the export states it.
    pub rabbitmq_version: Option<String>,

    pub vhosts: BTreeMap<String, Vhost>,
    pub users: BTreeMap<String, User>,
}

impl From<&Definitions> for Observation {
    fn from(definitions: &Definitions) -> Self {
        Observation::object([
            (
                "rabbitmq_version",
                match &definitions.rabbitmq_version {
                    Some(version) => Observation::text(version.as_str()),
                    None => Observation::null(),
                },
            ),
            (
                "vhosts",
                Observation::object(
                    definitions
                        .vhosts
                        .iter()
                        .map(|(name, vhost)| (name.as_str(), Observation::from(vhost))),
                ),
            ),
            (
                "users",
                Observation::object(
                    definitions
                        .users
                        .iter()
                        .map(|(name, user)| (name.as_str(), Observation::from(user))),
                ),
            ),
        ])
    }
}
