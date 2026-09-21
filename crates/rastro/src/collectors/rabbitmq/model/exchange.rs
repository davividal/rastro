//! One exchange somebody declared.

use std::collections::BTreeMap;

use rastro_collector::Observation;

use crate::collectors::rabbitmq::model::arguments_observation;
use crate::collectors::rabbitmq::value_objects::DefinitionValue;

/// A durable exchange as the definitions export describes it.
///
/// Only the durable ones are here, and that is the export's doing rather than a filter of
/// rastro's: a transient exchange does not survive a restart, so it is workload rather than
/// state and the document is better for not carrying it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Exchange {
    /// `direct`, `topic`, `fanout`, `headers`, or a type a plugin added.
    ///
    /// Named `exchange_type` rather than `type` because the wire spelling is a keyword in
    /// this language and a reader of the document should not have to know which.
    pub exchange_type: String,

    pub durable: bool,
    pub auto_delete: bool,
    pub arguments: BTreeMap<String, DefinitionValue>,
}

impl From<&Exchange> for Observation {
    fn from(exchange: &Exchange) -> Self {
        Observation::object([
            (
                "exchange_type",
                Observation::text(exchange.exchange_type.as_str()),
            ),
            ("durable", Observation::boolean(exchange.durable)),
            ("auto_delete", Observation::boolean(exchange.auto_delete)),
            ("arguments", arguments_observation(&exchange.arguments)),
        ])
    }
}
