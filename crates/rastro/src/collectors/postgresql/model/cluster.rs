//! One cluster, and what could be learned about it.

use rastro_collector::Observation;

use crate::collectors::postgresql::model::{
    ClusterDatabases, ClusterMemberships, ClusterRoles, ClusterSettings,
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
    pub port: u16,
    pub owner: String,
    pub settings: Option<ClusterSettings>,
    pub roles: Option<ClusterRoles>,
    pub memberships: Option<ClusterMemberships>,
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
            ("port", Observation::integer(i64::from(cluster.port))),
            ("owner", Observation::text(cluster.owner.as_str())),
        ];

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
            "databases",
            match &cluster.databases {
                Some(databases) => Observation::from(databases),
                None => Observation::null(),
            },
        ));

        Observation::object(entries)
    }
}
