//! Whether a cluster is running, and whether it is a standby.

use rastro_collector::CollectionError;

/// What postgresql-common says a cluster is doing.
///
/// **Two independent facts, because `pg_lsclusters` prints them in one column.** Its status
/// is `online` or `down`, with `,recovery` appended when the data directory carries a
/// recovery marker: `recovery.signal` or `standby.signal` from PostgreSQL 12, and
/// `recovery.conf` before it. A streaming standby is therefore `online,recovery`, and a
/// stopped one `down,recovery`; all four combinations occur.
///
/// **Running is the fact the settings read branches on.** A running cluster has an effective
/// configuration; a stopped one does not, and nothing is applying a `postgresql.conf` in a
/// cluster that is down.
///
/// **Recovery is state worth its own field.** A cluster promoted to primary, or demoted to
/// standby, is exactly the change a fingerprint exists to catch, and it is invisible in the
/// settings alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ClusterStatus {
    online: bool,
    in_recovery: bool,
}

/// postgresql-common's suffix for a cluster with a recovery marker in its data directory.
const RECOVERY: &str = "recovery";

impl ClusterStatus {
    /// Reads postgresql-common's own spelling, suffix included.
    ///
    /// An unrecognised word is a failure rather than a guess at either state: both answers
    /// would be a claim about a cluster rastro has not understood, and whether the cluster
    /// is running is what the whole read branches on.
    pub fn parse(value: &str) -> Result<Self, CollectionError> {
        let mut words = value.split(',');
        let online = match words.next() {
            Some("online") => true,
            Some("down") => false,
            _ => {
                return Err(CollectionError::new(format!(
                    "pg_lsclusters reported the status {value:?}, which starts with neither \
                     \"online\" nor \"down\", so whether the cluster is running cannot be told"
                )));
            }
        };
        let mut in_recovery = false;

        for word in words {
            if word != RECOVERY {
                return Err(CollectionError::new(format!(
                    "pg_lsclusters reported the status {value:?}, and {word:?} is not a \
                     qualifier rastro knows, so what it says about the cluster cannot be told"
                )));
            }

            in_recovery = true;
        }

        Ok(Self {
            online,
            in_recovery,
        })
    }

    /// The running half, which is what the facet records as `status`.
    pub fn as_str(&self) -> &'static str {
        if self.online { "online" } else { "down" }
    }

    pub fn is_online(&self) -> bool {
        self.online
    }

    pub fn in_recovery(&self) -> bool {
        self.in_recovery
    }
}
