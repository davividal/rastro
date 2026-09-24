//! One value inside a policy's definition.

/// A policy definition's value, in the three shapes a document can carry.
///
/// **Typed rather than all text, because a value that changed type would otherwise read as
/// unchanged.** `"1000"` and `1000` are different states of a policy and a fingerprint has to
/// be able to tell them apart.
///
/// **A non-integer number becomes text carrying its own spelling**, because the output format
/// admits no floating point: rendering `0.5` as a number is impossible and rounding it would
/// report a policy the broker does not have. The spelling is what the broker printed, so a
/// change to it is still visible, which is what the document is for.
///
/// **A nested list or object becomes text too**, as its compact JSON spelling. Policy
/// definitions are flat in every shape RabbitMQ documents, so this is the honest fallback for
/// a shape nobody has seen rather than a modelling of one.
/// Ordered so that a [`Binding`](crate::collectors::rabbitmq::model::Binding) carrying one can
/// live in a sorted collection. The order between variants is arbitrary and exists only to be
/// stable: what the document promises is that two runs of an unchanged box render the same
/// bytes, not that an integer sorts before a word for a reason.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum DefinitionValue {
    Integer(i64),
    Boolean(bool),
    Text(String),
}
