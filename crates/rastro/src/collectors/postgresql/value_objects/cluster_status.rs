//! Whether a cluster is running, whether it is a standby, and whatever else it is flagged as.

use rastro_collector::CollectionError;

/// What postgresql-common says a cluster is doing.
///
/// **A running word, a recovery flag, and an open set of qualifiers.** `pg_lsclusters` prints
/// one comma-separated column: `online` or `down` first, then any of `recovery` (a
/// data-directory recovery marker), `binaries_missing` (the package for this version is
/// gone), and a supervisor's name (`patroni`, `pacemaker`) where one manages the cluster.
/// That set grows with postgresql-common, so an unrecognised qualifier is *recorded*, not
/// rejected: `down,binaries_missing` is what a purged old-version package leaves after a
/// major upgrade, and failing on it would take the whole facet down, every other cluster on
/// the box included. Only a first word that is neither `online` nor `down` is refused,
/// because whether the cluster is running is what the read branches on.
///
/// **Recovery is lifted to its own field.** A cluster promoted to primary, or demoted to
/// standby, is exactly the change a fingerprint exists to catch, and it is invisible in the
/// settings alone. The remaining qualifiers stay a sorted list, so a supervisor appearing or
/// a package going missing shows in a diff without any of them being able to fail the read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClusterStatus {
    online: bool,
    in_recovery: bool,
    qualifiers: Vec<String>,
}

/// postgresql-common's suffix for a cluster with a recovery marker in its data directory.
const RECOVERY: &str = "recovery";

impl ClusterStatus {
    /// Reads postgresql-common's own spelling, every suffix included.
    ///
    /// A first word that is neither `online` nor `down` is a failure rather than a guess:
    /// both answers would be a claim about a cluster rastro has not understood, and whether
    /// the cluster is running is what the whole read branches on. Every later word is kept,
    /// because a qualifier rastro does not yet name is still a fact about the cluster and
    /// never a reason to lose the facet.
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
        let mut qualifiers = Vec::new();

        for word in words {
            if word.is_empty() {
                continue;
            }

            if word == RECOVERY {
                in_recovery = true;
            } else {
                qualifiers.push(word.to_owned());
            }
        }

        qualifiers.sort();

        Ok(Self {
            online,
            in_recovery,
            qualifiers,
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

    /// The qualifiers beyond running and recovery, sorted: `binaries_missing`, a supervisor.
    pub fn qualifiers(&self) -> &[String] {
        &self.qualifiers
    }
}
