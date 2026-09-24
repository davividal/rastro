//! What rastro means by a RabbitMQ installation, as opposed to how epmd prints it.

mod alarm;
mod binding;
mod definitions;
mod exchange;
mod installation;
mod listener;
mod node;
mod node_status;
mod parameter;
mod permission;
mod policy;
mod queue;
mod topic_permission;
mod user;
mod vhost;

pub use alarm::Alarm;
pub use binding::Binding;
pub use definitions::Definitions;
pub use exchange::Exchange;
pub use installation::Installation;
pub use listener::Listener;
pub use node::Node;
pub use node_status::NodeStatus;
pub use parameter::Parameter;
pub use permission::Permission;
pub use policy::Policy;
pub use queue::Queue;
pub use topic_permission::TopicPermission;
pub use user::{User, UserLimit};
pub use vhost::Vhost;

use std::collections::BTreeMap;

use rastro_collector::Observation;

use crate::collectors::rabbitmq::value_objects::DefinitionValue;

/// The arguments of an exchange, a queue or a binding, rendered one way.
///
/// Shared here rather than repeated in the three types, because it is one concept: the
/// `x-` arguments a client sent with a declaration, and they are read the same whichever
/// declaration carried them. Types are kept, for the reason
/// [`DefinitionValue`] gives.
pub fn arguments_observation(arguments: &BTreeMap<String, DefinitionValue>) -> Observation {
    Observation::object(arguments.iter().map(|(name, value)| {
        (
            name.as_str(),
            match value {
                DefinitionValue::Integer(number) => Observation::integer(*number),
                DefinitionValue::Boolean(flag) => Observation::boolean(*flag),
                DefinitionValue::Text(text) => Observation::text(text.as_str()),
            },
        )
    }))
}
