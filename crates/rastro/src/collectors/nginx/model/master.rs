//! The nginx that is actually running.

use rastro_collector::{NonEmptyText, Observation};

use crate::collectors::nginx::value_objects::SecondsSinceEpoch;

/// The running master process, and the workers under it.
///
/// **Why the workers are here.** The question a fingerprint is taken to answer about a web
/// server is whether the configuration on disk is the one being served, and the master
/// cannot answer it: a reload leaves the master exactly as it was, so its own start time
/// says only when the service last *restarted*. The workers are replaced on every reload, so
/// the oldest of them dates the last time nginx read its configuration. Held against
/// `configuration.newest_modified`, that is the whole signal: files newer than the oldest
/// worker have not been served yet.
///
/// **`executable` is the link `/proc/<pid>/exe` points at, verbatim**, which the kernel
/// marks ` (deleted)` when the file has been replaced under the running process. That is how
/// a package upgrade without a restart looks from the outside, and it is worth the one field
/// it costs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Master {
    pub process_id: i64,
    pub executable: NonEmptyText,
    pub started_at: SecondsSinceEpoch,
    /// The `-c` the master was started with, when it was given one.
    pub configuration_path: Option<NonEmptyText>,
    /// The `-p` the master was started with, when it was given one.
    pub prefix: Option<NonEmptyText>,
    pub worker_count: i64,
    /// When the oldest worker started, which dates the last reload.
    pub workers_started_at: Option<SecondsSinceEpoch>,
}

impl From<&Master> for Observation {
    fn from(master: &Master) -> Self {
        Observation::object([
            (
                "configuration_path",
                master
                    .configuration_path
                    .as_ref()
                    .map_or_else(Observation::null, |path| Observation::text(path.as_str())),
            ),
            ("executable", Observation::text(master.executable.as_str())),
            (
                "prefix",
                master
                    .prefix
                    .as_ref()
                    .map_or_else(Observation::null, |prefix| {
                        Observation::text(prefix.as_str())
                    }),
            ),
            ("process_id", Observation::integer(master.process_id)),
            ("started_at", Observation::from(&master.started_at)),
            ("worker_count", Observation::integer(master.worker_count)),
            (
                "workers_started_at",
                master
                    .workers_started_at
                    .as_ref()
                    .map_or_else(Observation::null, Observation::from),
            ),
        ])
    }
}
