//! Every grant on one database, and the shape a reader diffs them in.

use std::collections::BTreeMap;

use rastro_collector::Observation;

use crate::collectors::postgresql::model::Grant;

/// What one database's ACL holds, keyed by grantee.
///
/// **Keyed rather than listed, and the key is the grantee alone.** Both halves of that were
/// measured against a real pair of fingerprints, over an `ALTER DATABASE … OWNER` that
/// rewrote the grantor of thirteen entries, gave the new owner an entry the ACL did not
/// carry, and revoked `CONNECT` and `TEMPORARY` from the old one. Listed, the insertion
/// shifted every later element and four grants were reported as changing hands. Keyed by
/// grantee *and grantor*, worse: the grantor rewrite renames every key, so each grant reads
/// as one removed and one added and a diff never descends far enough to show the revoke at
/// all. Keyed by grantee, the insertion is one key, the rewrite is one field per holder, and
/// the revoke is two keys leaving a path that names the role they were taken from.
///
/// **So the grantor is a field and not a key, and a grantee holds a list.** The same role can
/// hold `CONNECT` from one grantor and `CREATE` from another, which is why the list is there;
/// it is one element wide in every ordinary ACL, and a position only shifts within one
/// grantee's own grants rather than across the database's.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DatabaseGrants {
    grants: Vec<Grant>,
}

impl DatabaseGrants {
    pub fn new(grants: impl IntoIterator<Item = Grant>) -> Self {
        Self {
            grants: grants.into_iter().collect(),
        }
    }

    pub fn grants(&self) -> &[Grant] {
        &self.grants
    }
}

impl From<&DatabaseGrants> for Observation {
    /// Grouped by grantee, each holder's grants ordered by the role that made them.
    ///
    /// Ordering is the map's rather than the collector's, so `PUBLIC` heads a database's
    /// grants because it is uppercase and Postgres folds an unquoted identifier to lowercase.
    /// A role deliberately created as `"ANALYST"` sorts above it; that is a rendering order,
    /// not a claim about privilege.
    fn from(grants: &DatabaseGrants) -> Self {
        let mut grouped: BTreeMap<&str, BTreeMap<&str, Observation>> = BTreeMap::new();

        for grant in grants.grants() {
            grouped
                .entry(grant.grantee.as_str())
                .or_default()
                .insert(grant.granted_by.as_str(), Observation::from(grant));
        }

        Observation::object(
            grouped.into_iter().map(|(grantee, by_grantor)| {
                (grantee, Observation::list(by_grantor.into_values()))
            }),
        )
    }
}

impl From<&Grant> for Observation {
    /// One aclitem: who made the grant, and what it carries.
    ///
    /// The grantee is the key this sits under, so it is not repeated here.
    fn from(grant: &Grant) -> Self {
        Observation::object([
            ("granted_by", Observation::text(grant.granted_by.as_str())),
            (
                "privileges",
                Observation::object(grant.privileges.iter().map(|(privilege, grantable)| {
                    (
                        privilege.as_str(),
                        Observation::object([("grantable", Observation::boolean(*grantable))]),
                    )
                })),
            ),
        ])
    }
}
