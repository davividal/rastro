//! Every replication slot a cluster holds, keyed by name.

use rastro_collector::{CollectionError, Observation};

use crate::collectors::postgresql::model::ReplicationSlot;

/// The replication slots of one cluster, keyed by name.
///
/// **Empty is the common case**, so it is allowed rather than refused: most clusters hold no
/// slot at all. The same name twice is refused, because `slot_name` is unique in the catalog,
/// so a repeat means two reads were spliced.
///
/// Rendered as an object keyed by name, whose order the format owns; the type holds only the
/// uniqueness the individual slots cannot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClusterReplicationSlots {
    slots: Vec<ReplicationSlot>,
}

impl ClusterReplicationSlots {
    pub fn new(mut slots: Vec<ReplicationSlot>) -> Result<Self, CollectionError> {
        slots.sort();

        if let Some(pair) = slots.windows(2).find(|pair| pair[0].name == pair[1].name) {
            return Err(CollectionError::new(format!(
                "the server reported the replication slot {:?} twice, so its state cannot be told",
                pair[0].name
            )));
        }

        Ok(Self { slots })
    }

    pub fn slots(&self) -> &[ReplicationSlot] {
        &self.slots
    }
}

impl From<&ClusterReplicationSlots> for Observation {
    fn from(slots: &ClusterReplicationSlots) -> Self {
        Observation::object(
            slots
                .slots()
                .iter()
                .map(|slot| (slot.name.as_str(), Observation::from(slot))),
        )
    }
}
