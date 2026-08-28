//! Every client-authentication rule `pg_hba_file_rules` reports for a cluster.

use rastro_collector::{CollectionError, Observation};

use crate::collectors::postgresql::model::HbaRule;

/// The host-based authentication rules of one cluster, in the order the server read them.
///
/// **Empty is allowed rather than refused.** The read is only reachable as a superuser, since
/// the view is revoked from PUBLIC and `pg_read_all_settings` does not lift it, so a
/// non-superuser owner fails the whole cluster read loudly upstream instead of reaching here
/// with a short set.
///
/// Ordered here rather than by the query, because list order is part of the output contract.
/// The sort is by `rule_number` first, which is the precedence order on PostgreSQL 16 and
/// later; a PostgreSQL 15 read has no rule number, so it falls back to the file and line the
/// rule came from. No uniqueness is enforced: two files can legitimately carry a line at the
/// same number, and on 15 there is no rule number to tell them apart, so a defined order is
/// all this type promises.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClusterHbaRules {
    rules: Vec<HbaRule>,
}

impl ClusterHbaRules {
    pub fn new(mut rules: Vec<HbaRule>) -> Result<Self, CollectionError> {
        rules.sort();

        Ok(Self { rules })
    }

    pub fn rules(&self) -> &[HbaRule] {
        &self.rules
    }
}

impl From<&ClusterHbaRules> for Observation {
    fn from(rules: &ClusterHbaRules) -> Self {
        Observation::list(rules.rules().iter().map(|rule| Observation::from(rule)))
    }
}
