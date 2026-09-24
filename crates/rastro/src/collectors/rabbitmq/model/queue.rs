//! One queue somebody declared.

use std::collections::BTreeMap;

use rastro_collector::Observation;

use crate::collectors::rabbitmq::model::arguments_observation;
use crate::collectors::rabbitmq::value_objects::DefinitionValue;

/// A durable queue as the definitions export describes it.
///
/// **What is not here is the whole point.** No message count, no consumer count, no memory
/// figure: a queue's depth is workload, it moves while rastro is reading it, and a default
/// view carrying it would teach an operator that the tool is noisy. What a fingerprint
/// answers about a queue is whether it exists and what it was declared to be.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Queue {
    /// The type the broker made it, which is not always the type the declaration asked for.
    ///
    /// Recorded beside `arguments`, which carries the `x-queue-type` the client sent, because
    /// the two are different facts: a vhost's default queue type or a policy can decide the
    /// outcome, so a queue declared with no type at all still has one here.
    pub queue_type: String,

    pub durable: bool,
    pub auto_delete: bool,
    pub arguments: BTreeMap<String, DefinitionValue>,
}

impl From<&Queue> for Observation {
    fn from(queue: &Queue) -> Self {
        Observation::object([
            ("queue_type", Observation::text(queue.queue_type.as_str())),
            ("durable", Observation::boolean(queue.durable)),
            ("auto_delete", Observation::boolean(queue.auto_delete)),
            ("arguments", arguments_observation(&queue.arguments)),
        ])
    }
}
