//! One value in a node's answer, in the shapes the document can carry.

use std::collections::BTreeMap;

use rastro_collector::Observation;

/// A value from a node's answer, as a tree.
///
/// **Typed rather than all text**, so `"1"` and `1` stay different states, as the RabbitMQ
/// facet's definition values do. **A non-integer number becomes text carrying its own
/// spelling**, because the format admits no floating point and rounding `0.85` would report a
/// setting the cluster does not have. Object keys are sorted by the map, so two answers that
/// differ only in the order the node printed them render the same bytes; list order is kept,
/// because in a pipeline's processors or a template's patterns the order is the meaning.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApiValue {
    Null,
    Boolean(bool),
    Integer(i64),
    Text(String),
    List(Vec<ApiValue>),
    Object(BTreeMap<String, ApiValue>),
}

impl From<&ApiValue> for Observation {
    fn from(value: &ApiValue) -> Self {
        match value {
            ApiValue::Null => Observation::null(),
            ApiValue::Boolean(flag) => Observation::boolean(*flag),
            ApiValue::Integer(number) => Observation::integer(*number),
            ApiValue::Text(text) => Observation::text(text),
            ApiValue::List(items) => Observation::list(items.iter().map(Observation::from)),
            ApiValue::Object(entries) => Observation::object(
                entries
                    .iter()
                    .map(|(key, value)| (key.as_str(), Observation::from(value))),
            ),
        }
    }
}
