//! One database, who owns it, and who may do what to it.

use std::collections::BTreeMap;

use rastro_collector::Observation;

use crate::collectors::postgresql::model::DatabaseExtensions;
use crate::collectors::postgresql::value_objects::{
    DatabaseName, DatabasePrivilege, Grantee, RoleName,
};

/// One aclitem: what one grantee was granted, by whom.
///
/// Kept as its own entry rather than merged per grantee, because the same grantee can hold
/// grants made by two different grantors and merging them would lose which is which.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Grant {
    pub grantee: Grantee,
    pub granted_by: RoleName,

    /// The privileges held, and whether each may be passed on.
    ///
    /// A map rather than a list because a privilege is held once or not at all, and the
    /// grant option is a property of holding it.
    pub privileges: BTreeMap<DatabasePrivilege, bool>,
}

/// A database in a cluster.
///
/// **A null ACL is not an empty one**, which is why `grants` is an option rather than a
/// list that happens to be empty. Postgres leaves `datacl` null until somebody grants or
/// revokes something, and null means the built-in defaults apply: the owner holds
/// everything and `PUBLIC` holds `CONNECT` and `TEMPORARY`. Rendering that as an empty list
/// would claim nobody may connect, which is the opposite of what it means.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Database {
    pub name: DatabaseName,
    pub owner: RoleName,
    pub allows_connections: bool,

    /// How many concurrent connections the database allows, or `None` for no limit.
    pub connection_limit: Option<i64>,

    pub grants: Option<Vec<Grant>>,

    /// What is installed in it, or `None` where it refuses connections.
    ///
    /// Absent means nobody could ask rather than nothing is installed: `template0` is kept
    /// unconnectable on purpose.
    pub extensions: Option<DatabaseExtensions>,
}

impl From<&Grant> for Observation {
    fn from(grant: &Grant) -> Self {
        Observation::object([
            ("grantee", Observation::text(grant.grantee.as_str())),
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

impl From<&Database> for Observation {
    fn from(database: &Database) -> Self {
        Observation::object([
            ("owner", Observation::text(database.owner.as_str())),
            (
                "allows_connections",
                Observation::boolean(database.allows_connections),
            ),
            (
                "connection_limit",
                match database.connection_limit {
                    Some(limit) => Observation::integer(limit),
                    None => Observation::null(),
                },
            ),
            (
                "grants",
                match &database.grants {
                    Some(grants) => Observation::list(grants.iter().map(Observation::from)),
                    None => Observation::null(),
                },
            ),
            (
                "extensions",
                match &database.extensions {
                    Some(extensions) => Observation::from(extensions),
                    None => Observation::null(),
                },
            ),
        ])
    }
}
