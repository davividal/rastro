//! One replication slot, the stable subset of it.

use rastro_collector::Observation;

/// A replication slot as the server reports it, minus everything that moves.
///
/// **Only the stable columns.** A slot's identity is worth a fingerprint: a logical slot
/// appearing means a subscription was pointed at this cluster, and a plugin or a two-phase
/// flag is how that subscription is shaped. What is left out is deliberate: `restart_lsn`,
/// `confirmed_flush_lsn`, `wal_status`, `safe_wal_size` and `active` all move as the slot
/// does its job, so recording them would fill a diff with noise.
///
/// A physical slot has no plugin and no database; a logical slot names both. The subscription
/// side (`pg_subscription`) is not read here, because its `subconninfo` carries a password.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ReplicationSlot {
    pub name: String,

    /// The output plugin a logical slot decodes with, or `None` for a physical slot.
    pub plugin: Option<String>,

    /// `physical` or `logical`.
    pub slot_type: String,

    /// The database a logical slot is bound to, or `None` for a physical slot.
    pub database: Option<String>,

    /// Whether the slot is dropped at the end of the session that made it.
    pub temporary: bool,

    /// Whether the slot decodes two-phase commits.
    pub two_phase: bool,
}

impl From<&ReplicationSlot> for Observation {
    fn from(slot: &ReplicationSlot) -> Self {
        Observation::object([
            (
                "plugin",
                match &slot.plugin {
                    Some(plugin) => Observation::text(plugin.as_str()),
                    None => Observation::null(),
                },
            ),
            ("slot_type", Observation::text(slot.slot_type.as_str())),
            (
                "database",
                match &slot.database {
                    Some(database) => Observation::text(database.as_str()),
                    None => Observation::null(),
                },
            ),
            ("temporary", Observation::boolean(slot.temporary)),
            ("two_phase", Observation::boolean(slot.two_phase)),
        ])
    }
}
