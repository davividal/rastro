//! Both halves of NFS on one box.

use rastro_collector::Observation;

use super::nfs_client::NfsClient;
use super::nfs_server::ServerHalf;

/// The facet: a box can serve, mount, both, or neither, and each half says which.
///
/// An absent half is a key with a `null`, never a missing key, so that starting a server reads
/// as a `null` becoming an object.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NfsReport {
    pub client: Option<NfsClient>,
    pub server: ServerHalf,
}

impl From<&NfsReport> for Observation {
    fn from(report: &NfsReport) -> Self {
        Observation::object([
            (
                "client",
                report
                    .client
                    .as_ref()
                    .map_or_else(Observation::null, Observation::from),
            ),
            ("server", Observation::from(&report.server)),
        ])
    }
}
