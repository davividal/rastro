//! What the running postmaster observes about itself, from `postmaster.pid`.

use rastro_collector::Observation;

use crate::collectors::postgresql::value_objects::PostmasterStatus;

/// The observed half of a running cluster, kept apart from what `pg_lsclusters` configures.
///
/// `design.md` states the rule this follows: the endpoint the flags configure is a different
/// fact from the one the server is bound to, and the two are separate so they can disagree.
/// `pg_lsclusters` prints the *configured* port from `postgresql.conf`, which is stale the
/// moment the file is edited without a reload; `postmaster.pid` line 4 is the port the
/// running server is actually serving on. Reading it is what stops a stale-config port from
/// making a live cluster read as `down`.
///
/// **The volatile lines are dropped, not annotated.** Line 1 (the PID) and line 3 (the start
/// time) move on every restart, so they are never read into this type; line 7 (the shared
/// memory key) is left out as noise. What remains, lines 4, 5, 6 and 8, is stable on an
/// unchanged cluster.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Postmaster {
    pub port: u16,

    /// The directory the server's Unix socket lives in, or `None` where it listens on none.
    pub socket_directory: Option<String>,

    /// The addresses the server accepts TCP connections on, as the file records them, or
    /// `None` where it accepts none.
    pub listen_addresses: Option<String>,

    /// What the postmaster says it is doing, or `None` on a pid file too short to carry it.
    pub status: Option<PostmasterStatus>,
}

impl From<&Postmaster> for Observation {
    fn from(postmaster: &Postmaster) -> Self {
        Observation::object([
            ("port", Observation::integer(i64::from(postmaster.port))),
            (
                "socket_directory",
                match &postmaster.socket_directory {
                    Some(directory) => Observation::text(directory.as_str()),
                    None => Observation::null(),
                },
            ),
            (
                "listen_addresses",
                match &postmaster.listen_addresses {
                    Some(addresses) => Observation::text(addresses.as_str()),
                    None => Observation::null(),
                },
            ),
            (
                "status",
                match &postmaster.status {
                    Some(status) => Observation::text(status.as_str()),
                    None => Observation::null(),
                },
            ),
        ])
    }
}
