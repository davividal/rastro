//! Who a server copies, and who copies it.

use rastro_collector::Observation;

/// A server's place in replication, from `INFO replication`.
///
/// **Beside `replicaof` in the settings, not a repeat of it.** In cluster mode a replica is
/// assigned with `CLUSTER REPLICATE` and recorded in the cluster's own file, so the setting says
/// nothing and only the role does.
///
/// The words are redis's own, `master` and `slave` included, because they are what an operator
/// greps the server's output for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Replication {
    pub role: String,

    /// The master a replica copies, as `host:port`.
    pub master: Option<String>,

    /// Whether a replica's link to its master is up at this moment.
    ///
    /// Volatile: a link drops and recovers with nobody touching the box.
    pub master_link_status: Option<String>,

    /// The replicas connected to a master, as `host:port`, sorted.
    ///
    /// Volatile for the same reason: a replica restarting elsewhere leaves and rejoins this list.
    pub replicas: Vec<String>,
}

impl From<&Replication> for Observation {
    fn from(replication: &Replication) -> Self {
        let text_or_null = |value: &Option<String>| match value {
            Some(value) => Observation::text(value.as_str()),
            None => Observation::null(),
        };

        Observation::object([
            ("role", Observation::text(replication.role.as_str())),
            ("master", text_or_null(&replication.master)),
            (
                "master_link_status",
                text_or_null(&replication.master_link_status).volatile(),
            ),
            (
                "replicas",
                Observation::list(
                    replication
                        .replicas
                        .iter()
                        .map(|replica| Observation::text(replica.as_str())),
                )
                .volatile(),
            ),
        ])
    }
}
