//! Every configuration-file line `pg_file_settings` reports for a cluster.

use rastro_collector::{CollectionError, Observation};

use crate::collectors::postgresql::model::FileSetting;

/// The configuration-file lines of one cluster, in the order the server read them.
///
/// **Empty is allowed rather than refused.** The read is only reachable as a superuser, since
/// the view is revoked from PUBLIC and `pg_read_all_settings` does not lift it, so a
/// non-superuser owner fails the whole cluster read loudly upstream instead of reaching here
/// with a short set. A genuinely empty file set is then a real state, not a failed read.
///
/// Ordered by `seqno`, the order the server read the lines in, which is the order precedence
/// follows and the contract this type keeps rather than one the query promises. The same
/// `seqno` twice is refused: it is a sequence number, so a repeat means two reads were
/// spliced and the precedence can no longer be told.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClusterFileSettings {
    settings: Vec<FileSetting>,
}

impl ClusterFileSettings {
    pub fn new(mut settings: Vec<FileSetting>) -> Result<Self, CollectionError> {
        settings.sort();

        if let Some(pair) = settings
            .windows(2)
            .find(|pair| pair[0].seqno == pair[1].seqno)
        {
            return Err(CollectionError::new(format!(
                "the server reported seqno {} twice in pg_file_settings, so which line takes \
                 precedence cannot be told",
                pair[0].seqno
            )));
        }

        Ok(Self { settings })
    }

    pub fn settings(&self) -> &[FileSetting] {
        &self.settings
    }
}

impl From<&ClusterFileSettings> for Observation {
    fn from(settings: &ClusterFileSettings) -> Self {
        Observation::list(settings.settings().iter().map(Observation::from))
    }
}
