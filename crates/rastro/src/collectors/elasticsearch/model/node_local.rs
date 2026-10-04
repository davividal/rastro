//! A node's own effective settings: what is configured on this node rather than the cluster.

use std::collections::BTreeMap;

use rastro_collector::Observation;

use crate::collectors::elasticsearch::value_objects::ApiValue;

/// The part of a temporary directory's path the node makes new on every start.
const PER_START_DIRECTORY: &str = "/tmp/elasticsearch-";

/// What this node runs with, from `_nodes/_local`: what an operator changes in its file, its
/// environment or its JVM options, which the cluster-wide surfaces do not hold. Found as a gap by
/// the second domain review.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeLocal {
    /// Sorted, since roles are a set and the node lists them in an order of its own.
    pub roles: Vec<String>,

    /// `node.attr.*`, an operator's allocation awareness among them, and the ones the node sets
    /// itself from the hardware, which move when the box is resized, a real change.
    pub attributes: BTreeMap<String, String>,

    /// The effective settings, flat. **Sensitive whole**: a plugin's setting that is not declared
    /// filtered would show here, so no one key is trusted by name, as with snapshot repositories.
    pub settings: ApiValue,

    /// The JVM's arguments, in order, since a later one overrides an earlier one.
    pub jvm_arguments: Vec<String>,

    pub heap_max_bytes: Option<i64>,
}

impl From<&NodeLocal> for Observation {
    fn from(local: &NodeLocal) -> Self {
        Observation::object([
            (
                "roles",
                Observation::set(local.roles.iter().map(Observation::text)),
            ),
            (
                "attributes",
                Observation::object(
                    local
                        .attributes
                        .iter()
                        .map(|(name, value)| (name.as_str(), Observation::text(value))),
                ),
            ),
            ("settings", Observation::from(&local.settings).sensitive()),
            (
                "jvm_arguments",
                // A sequence: the JVM takes the last of two settings of one option.
                Observation::sequence(local.jvm_arguments.iter().map(|argument| {
                    // Measured by the second domain review: new on every start.
                    let rendered = Observation::text(argument);
                    match argument.contains(PER_START_DIRECTORY) {
                        true => rendered.volatile(),
                        false => rendered,
                    }
                })),
            ),
            (
                "heap_max_bytes",
                match local.heap_max_bytes {
                    Some(bytes) => Observation::integer(bytes),
                    None => Observation::null(),
                },
            ),
        ])
    }
}
