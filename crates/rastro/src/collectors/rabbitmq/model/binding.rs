//! One route from an exchange to a queue or another exchange.

use std::collections::BTreeMap;

use rastro_collector::Observation;

use crate::collectors::rabbitmq::model::arguments_observation;
use crate::collectors::rabbitmq::value_objects::DefinitionValue;

/// A binding as the definitions export describes it.
///
/// **Listed rather than keyed, because a binding has no unique name.** One source and one
/// destination can be bound several times over with different routing keys, and a headers
/// exchange can carry two bindings with the same routing key and different arguments. There
/// is no key that would not collide, which is the same conclusion
/// [a database's grants reached](#a-grantee-holds-a-list-because-a-grantee-is-not-a-unique-key)
/// from the other direction.
///
/// **Ordered by its own fields**, which is what makes the list a contract rather than
/// whatever order the export happened to print: the derived ordering runs source, then
/// destination type, then destination, then routing key, then arguments, and the field order
/// below is therefore part of the output format.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Binding {
    pub source: String,

    /// `queue` or `exchange`, since an exchange can be bound to another exchange.
    pub destination_type: String,

    pub destination: String,
    pub routing_key: String,
    pub arguments: BTreeMap<String, DefinitionValue>,
}

impl From<&Binding> for Observation {
    fn from(binding: &Binding) -> Self {
        Observation::object([
            ("source", Observation::text(binding.source.as_str())),
            (
                "destination_type",
                Observation::text(binding.destination_type.as_str()),
            ),
            (
                "destination",
                Observation::text(binding.destination.as_str()),
            ),
            (
                "routing_key",
                Observation::text(binding.routing_key.as_str()),
            ),
            ("arguments", arguments_observation(&binding.arguments)),
        ])
    }
}
