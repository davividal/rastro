//! The labels attached to a container.

use std::collections::BTreeMap;

use rastro_collector::Observation;

use crate::collectors::containers::value_objects::LabelName;

/// Every label, keyed by name, recorded as it stands.
///
/// **Public, unlike the environment, and that asymmetry is deliberate.** A label is metadata
/// somebody attached to describe the container, and it is the link back to the definition it
/// came from: compose writes its project, its service and a hash of the config it rendered.
/// For a container nobody named by hand, that is the only durable identity a diff has.
///
/// The value is plain text: an empty label is one that was set to nothing, and compose puts
/// rendered JSON in some of them.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ContainerLabels(BTreeMap<LabelName, String>);

impl ContainerLabels {
    pub fn new(labels: impl IntoIterator<Item = (LabelName, String)>) -> Self {
        Self(labels.into_iter().collect())
    }

    pub fn labels(&self) -> &BTreeMap<LabelName, String> {
        &self.0
    }
}

impl From<&ContainerLabels> for Observation {
    fn from(labels: &ContainerLabels) -> Self {
        Observation::object(
            labels
                .labels()
                .iter()
                .map(|(name, value)| (name.as_str(), Observation::text(value))),
        )
    }
}
