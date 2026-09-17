//! Every grant on one database, and the shape a reader diffs them in.

use std::collections::BTreeMap;

use rastro_collector::Observation;

use crate::collectors::postgresql::model::Grant;

/// What one database's ACL holds, keyed by grantee and then by the role that granted it.
///
/// **Keyed rather than listed, and that is a readability decision with a correctness edge.**
/// An `ALTER DATABASE … OWNER` gives the new owner an explicit entry the ACL did not carry
/// before. In a list every entry after the insertion shifts along, so a diff of two
/// fingerprints reports the shift as though each of those grants had changed hands, and the
/// revoke that same statement performs is one line among a dozen artefacts. Keyed, the
/// insertion is one key appearing and the revoke is one key leaving, at a path that names the
/// grantee it was taken from.
///
/// **Two levels, because a grantee is not a unique key.** The same role can hold `CONNECT`
/// from one grantor and `CREATE` from another, and a `REVOKE` has to name the grantor to take
/// either away, so merging the two would lose which is which.
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
    /// Grouped by grantee, then by grantor.
    ///
    /// Ordering is the map's rather than the collector's, so `PUBLIC` heads the grants of
    /// every database whose roles are lowercase, which is the convention Postgres itself
    /// follows for an unquoted identifier. A role deliberately named in uppercase sorts
    /// against it; that is a rendering order, not a claim about privilege.
    fn from(grants: &DatabaseGrants) -> Self {
        let mut grouped: BTreeMap<&str, BTreeMap<&str, Observation>> = BTreeMap::new();

        for grant in grants.grants() {
            grouped
                .entry(grant.grantee.as_str())
                .or_default()
                .insert(grant.granted_by.as_str(), privileges_of(grant));
        }

        Observation::object(
            grouped
                .into_iter()
                .map(|(grantee, granted)| (grantee, Observation::object(granted))),
        )
    }
}

/// Each privilege held, and whether it may be passed on.
fn privileges_of(grant: &Grant) -> Observation {
    Observation::object(grant.privileges.iter().map(|(privilege, grantable)| {
        (
            privilege.as_str(),
            Observation::object([("grantable", Observation::boolean(*grantable))]),
        )
    }))
}
