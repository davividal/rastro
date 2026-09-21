//! What one user may publish and consume on one topic exchange.

use rastro_collector::Observation;

/// A user's routing-key patterns on one topic exchange.
///
/// Separate from [`Permission`](super::Permission) because it is a separate grant with a
/// third key in it: ordinary permissions are per vhost and per user, while a topic permission
/// is also per exchange, and a user can hold different patterns on `amq.topic` and on an
/// exchange of their own. There is no `configure` half, because a routing key is not
/// something a client declares.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TopicPermission {
    pub write: String,
    pub read: String,
}

impl From<&TopicPermission> for Observation {
    fn from(permission: &TopicPermission) -> Self {
        Observation::object([
            ("write", Observation::text(permission.write.as_str())),
            ("read", Observation::text(permission.read.as_str())),
        ])
    }
}
