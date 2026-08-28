//! The cluster's identity from its control file: lineage, not configuration.

use rastro_collector::Observation;

/// What `pg_control` says about which cluster this is.
///
/// Neither value is a GUC, so nothing in `pg_settings` can produce them, and both answer a
/// question a settings diff cannot. `system_identifier` is minted at initdb and never changes
/// after: two clusters sharing it share a lineage (one was restored from the other's
/// basebackup), and it changing means the cluster was re-initdb'd. `timeline_id` increments
/// on promotion, so it is how a standby becoming a primary shows in a fingerprint.
///
/// The other columns of `pg_control_checkpoint()` and `pg_control_system()` are deliberately
/// left out: they are LSNs, xids and a checkpoint time that move whenever a checkpoint runs,
/// which is not a property a fingerprint should rest on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ControlData {
    /// The cluster's system identifier, as text.
    ///
    /// Text rather than a number: it is a 64-bit unsigned value that does not fit an `i64`,
    /// and PostgreSQL 18 changes how it is rendered, so it is compared rather than computed
    /// with.
    pub system_identifier: String,

    /// The current timeline, which increments on promotion.
    pub timeline_id: i64,
}

impl From<&ControlData> for Observation {
    fn from(control: &ControlData) -> Self {
        Observation::object([
            (
                "system_identifier",
                Observation::text(control.system_identifier.as_str()),
            ),
            ("timeline_id", Observation::integer(control.timeline_id)),
        ])
    }
}
