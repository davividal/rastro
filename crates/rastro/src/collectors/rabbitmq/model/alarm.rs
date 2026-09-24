//! A limit the node has hit.

use rastro_collector::Observation;

/// One resource alarm, as the node reports it.
///
/// **What it means on a broker**, which is why it is worth recording at all: while an alarm is
/// up the node blocks publishing connections. A box that looks healthy and refuses writes is
/// exactly the state an operator reaches for a fingerprint to explain.
///
/// **Volatile, and the annotation is not a formality.** Measured by raising one: an alarm
/// appears and clears with load, without anybody changing the box, so it is the definition of
/// a value that must not reach the diffable view. It is recorded rather than dropped because
/// `--include-volatile` is for the operator standing in front of the box.
///
/// The node's own name is in the report and is dropped here: this alarm belongs to the node
/// whose entry it sits under, and repeating it would be the document arguing with its own
/// keys.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Alarm {
    /// What kind of limit it is, `resource_limit` being the one RabbitMQ raises.
    pub alarm_type: String,

    /// Which resource ran out: `memory` or `disk`.
    pub resource: String,
}

impl From<&Alarm> for Observation {
    fn from(alarm: &Alarm) -> Self {
        Observation::object([
            ("type", Observation::text(alarm.alarm_type.as_str())),
            ("resource", Observation::text(alarm.resource.as_str())),
        ])
    }
}
