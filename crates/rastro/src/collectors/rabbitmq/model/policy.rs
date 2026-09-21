//! One policy, and what it does to the queues and exchanges it matches.

use std::collections::BTreeMap;

use rastro_collector::Observation;

use crate::collectors::rabbitmq::value_objects::DefinitionValue;

/// A policy as the definitions export describes it.
///
/// Policies are how a broker's behaviour is set without touching a client: a queue's length
/// limit, its dead-letter exchange, its type. A policy appearing, or its pattern widening, can
/// change how every queue on a vhost behaves, which makes it one of the highest-value entries
/// in this facet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Policy {
    /// The regular expression the broker matches resource names against, as text.
    pub pattern: String,

    /// What the policy applies to: `queues`, `exchanges`, `all`.
    pub apply_to: Option<String>,

    /// Which policy wins where two match one resource.
    pub priority: Option<i64>,

    pub definition: BTreeMap<String, DefinitionValue>,
}

impl From<&Policy> for Observation {
    fn from(policy: &Policy) -> Self {
        Observation::object([
            ("pattern", Observation::text(policy.pattern.as_str())),
            (
                "apply_to",
                match &policy.apply_to {
                    Some(target) => Observation::text(target.as_str()),
                    None => Observation::null(),
                },
            ),
            (
                "priority",
                match policy.priority {
                    Some(priority) => Observation::integer(priority),
                    None => Observation::null(),
                },
            ),
            (
                "definition",
                Observation::object(policy.definition.iter().map(|(name, value)| {
                    (
                        name.as_str(),
                        match value {
                            DefinitionValue::Integer(number) => Observation::integer(*number),
                            DefinitionValue::Boolean(flag) => Observation::boolean(*flag),
                            DefinitionValue::Text(text) => Observation::text(text.as_str()),
                        },
                    )
                })),
            ),
        ])
    }
}
