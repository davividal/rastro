//! One cluster, and what could be learned about it.

use rastro_collector::Observation;

use crate::collectors::postgresql::model::{
    ClusterAvailableExtensions, ClusterDatabases, ClusterFileSettings, ClusterHbaRules,
    ClusterMemberships, ClusterRoleSettings, ClusterRoles, ClusterSettings, ControlData,
    Postmaster, ReadLens,
};
use crate::collectors::postgresql::value_objects::ClusterStatus;

/// A cluster on the box.
///
/// **Settings are absent for a stopped cluster, and that is not a gap in the reading.** A
/// cluster that is down has no effective configuration, because nothing is in memory
/// applying one. The status carries that, so a reader is never left guessing whether an
/// empty settings map means "stopped" or "the read failed": a failure never gets this far,
/// it fails the facet instead.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cluster {
    pub status: ClusterStatus,

    /// The configured port, or `None` where `pg_lsclusters` could not read it. The port the
    /// server is actually serving on, when it is running, is in [`Cluster::observed`].
    pub port: Option<u16>,
    pub owner: String,

    /// What the running server observes of itself, from `postmaster.pid`, kept apart from the
    /// configured facts above so the two can disagree.
    pub observed: Option<Postmaster>,
    pub control: Option<ControlData>,
    pub hba_rules: Option<ClusterHbaRules>,
    pub lens: Option<ReadLens>,
    pub settings: Option<ClusterSettings>,
    pub file_settings: Option<ClusterFileSettings>,
    pub roles: Option<ClusterRoles>,
    pub memberships: Option<ClusterMemberships>,
    pub role_settings: Option<ClusterRoleSettings>,
    pub available_extensions: Option<ClusterAvailableExtensions>,
    pub databases: Option<ClusterDatabases>,
}

impl From<&Cluster> for Observation {
    fn from(cluster: &Cluster) -> Self {
        let mut entries = vec![
            ("status", Observation::text(cluster.status.as_str())),
            (
                "in_recovery",
                Observation::boolean(cluster.status.in_recovery()),
            ),
            (
                "qualifiers",
                Observation::list(
                    cluster
                        .status
                        .qualifiers()
                        .iter()
                        .map(|qualifier| Observation::text(qualifier.as_str())),
                ),
            ),
            (
                "port",
                match cluster.port {
                    Some(port) => Observation::integer(i64::from(port)),
                    None => Observation::null(),
                },
            ),
            ("owner", Observation::text(cluster.owner.as_str())),
        ];

        // The observed half, kept apart from the configured facts so a stale-config port or a
        // standby refusing connections shows as a disagreement rather than a contradiction.
        entries.push((
            "observed",
            match &cluster.observed {
                Some(observed) => Observation::from(observed),
                None => Observation::null(),
            },
        ));

        // The control-file lineage: which cluster this is, and its timeline.
        entries.push((
            "control",
            match &cluster.control {
                Some(control) => Observation::from(control),
                None => Observation::null(),
            },
        ));

        // Who may connect as whom: authentication state pg_settings does not carry.
        entries.push((
            "hba_rules",
            match &cluster.hba_rules {
                Some(hba_rules) => Observation::from(hba_rules),
                None => Observation::null(),
            },
        ));

        // The lens the settings were read through, and the one fact derived from it a reader
        // needs at a glance: whether the map is complete. A non-superuser without
        // `pg_read_all_settings` loses the 21 `GUC_SUPERUSER_ONLY` rows with no word from the
        // server, so `settings_complete` false is the loud qualifier on the map below.
        entries.push((
            "lens",
            match &cluster.lens {
                Some(lens) => Observation::from(lens),
                None => Observation::null(),
            },
        ));

        entries.push((
            "settings_complete",
            match &cluster.lens {
                Some(lens) => Observation::boolean(lens.sees_all_settings()),
                None => Observation::null(),
            },
        ));

        // Declared rather than sorted: this object's shape is rastro's own, and the key
        // order is the contract. Only the maps beneath it are sorted, by their own types.
        entries.push((
            "settings",
            match &cluster.settings {
                Some(settings) => Observation::from(settings),
                None => Observation::null(),
            },
        ));

        entries.push((
            "file_settings",
            match &cluster.file_settings {
                Some(file_settings) => Observation::from(file_settings),
                None => Observation::null(),
            },
        ));

        entries.push((
            "roles",
            match &cluster.roles {
                Some(roles) => Observation::from(roles),
                None => Observation::null(),
            },
        ));

        entries.push((
            "memberships",
            match &cluster.memberships {
                Some(memberships) => Observation::from(memberships),
                None => Observation::null(),
            },
        ));

        entries.push((
            "role_settings",
            match &cluster.role_settings {
                Some(role_settings) => Observation::from(role_settings),
                None => Observation::null(),
            },
        ));

        // What is installable cluster-wide, distinct from what each database has created.
        entries.push((
            "available_extensions",
            match &cluster.available_extensions {
                Some(available_extensions) => Observation::from(available_extensions),
                None => Observation::null(),
            },
        ));

        entries.push((
            "databases",
            match &cluster.databases {
                Some(databases) => Observation::from(databases),
                None => Observation::null(),
            },
        ));

        Observation::object(entries)
    }
}
