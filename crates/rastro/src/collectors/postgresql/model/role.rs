//! One role a cluster knows, and what it may do.

use rastro_collector::{Observation, Xxh3Digest};

use crate::collectors::postgresql::value_objects::{PasswordMethod, RoleName};

/// A role as the server reports it.
///
/// Six booleans rather than a summary, because each is a distinct privilege an operator
/// grants separately and any one of them appearing is a different event. `superuser` is the
/// one worth naming: the reference box carries two besides the cluster owner, and a third
/// appearing is the change this facet exists to make loud.
///
/// **No password, and no verifier.** `password_method` says whether the role has one and how
/// it is stored, `password_digest` whether it changed. Both are derived in the query, so the
/// verifier never leaves the server; a fingerprint has no business carrying one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Role {
    pub name: RoleName,
    pub superuser: bool,
    pub creates_databases: bool,
    pub creates_roles: bool,
    pub replication: bool,
    pub bypasses_row_level_security: bool,
    pub can_login: bool,

    /// How many concurrent connections the role may hold, or `None` for no limit.
    ///
    /// The server spells no limit `-1`. Recording that as a number would put a negative
    /// count of connections in the document and invite arithmetic on it.
    pub connection_limit: Option<i64>,

    /// When the role's password stops being accepted, as the server printed it.
    ///
    /// Text rather than a parsed instant: reformatting would invent a timezone rendering
    /// rastro does not own, and the value is compared rather than computed with.
    pub valid_until: Option<String>,

    /// How the role's password is stored, or `None` where it has none.
    ///
    /// A role with no password cannot log in with one, whatever `can_login` says, so the two
    /// together are what an operator reads.
    pub password_method: Option<PasswordMethod>,

    /// Whether the role's password changed, or `None` where it has none.
    ///
    /// The companion `password_method` cannot answer that: it names the algorithm, so a
    /// rotation from one SCRAM password to another leaves it untouched, and PostgreSQL
    /// re-salts on every set. The digest is taken over the sha256 the *server* computed, so
    /// the verifier is never read into this process at all.
    pub password_digest: Option<Xxh3Digest>,
}

impl From<&Role> for Observation {
    fn from(role: &Role) -> Self {
        Observation::object([
            ("superuser", Observation::boolean(role.superuser)),
            (
                "creates_databases",
                Observation::boolean(role.creates_databases),
            ),
            ("creates_roles", Observation::boolean(role.creates_roles)),
            ("replication", Observation::boolean(role.replication)),
            (
                "bypasses_row_level_security",
                Observation::boolean(role.bypasses_row_level_security),
            ),
            ("can_login", Observation::boolean(role.can_login)),
            (
                "connection_limit",
                match role.connection_limit {
                    Some(limit) => Observation::integer(limit),
                    None => Observation::null(),
                },
            ),
            (
                "valid_until",
                match &role.valid_until {
                    Some(expiry) => Observation::text(expiry.as_str()),
                    None => Observation::null(),
                },
            ),
            (
                "password_method",
                match role.password_method {
                    Some(method) => Observation::text(method.as_str()),
                    None => Observation::null(),
                },
            ),
            (
                "password_digest",
                match &role.password_digest {
                    Some(digest) => digest.into(),
                    None => Observation::null(),
                },
            ),
        ])
    }
}
