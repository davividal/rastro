//! What a node holds that somebody declared, as opposed to what it is running.

use std::collections::BTreeMap;

use rastro_collector::Observation;

use crate::collectors::rabbitmq::model::{Permission, TopicPermission, User, Vhost};

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

    /// Permissions, by vhost and then by user.
    ///
    /// Nested rather than listed, because a permission is a fact about a pair and neither
    /// half is unique on its own: one user holds different permissions in each vhost, and one
    /// vhost grants different permissions to each user. Nesting lets a reader open either
    /// question without scanning a list.
    pub permissions: BTreeMap<String, BTreeMap<String, Permission>>,

    /// Topic permissions, by vhost, then user, then exchange.
    ///
    /// Three levels, because this grant has a third key: the exchange it applies to.
    pub topic_permissions: BTreeMap<String, BTreeMap<String, BTreeMap<String, TopicPermission>>>,
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
            (
                "permissions",
                Observation::object(definitions.permissions.iter().map(|(vhost, users)| {
                    (
                        vhost.as_str(),
                        Observation::object(users.iter().map(|(user, permission)| {
                            (user.as_str(), Observation::from(permission))
                        })),
                    )
                })),
            ),
            (
                "topic_permissions",
                Observation::object(definitions.topic_permissions.iter().map(|(vhost, users)| {
                    (
                        vhost.as_str(),
                        Observation::object(users.iter().map(|(user, exchanges)| {
                            (
                                user.as_str(),
                                Observation::object(exchanges.iter().map(
                                    |(exchange, permission)| {
                                        (exchange.as_str(), Observation::from(permission))
                                    },
                                )),
                            )
                        })),
                    )
                })),
            ),
        ])
    }
}
