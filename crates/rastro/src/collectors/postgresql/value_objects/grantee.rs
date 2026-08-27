//! Who a grant was made to.

use crate::collectors::postgresql::value_objects::RoleName;

/// The holder of a grant: a role, or everybody.
///
/// `PUBLIC` is not a role and cannot be one, so it is a variant rather than a name. The
/// distinction earns its keep here: every login role in the tenancy model reaches its
/// database through `PUBLIC`'s default `CONNECT`, so a grant to `PUBLIC` disappearing
/// affects roles that are not mentioned anywhere near it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Grantee {
    /// Every role, including ones created later.
    ///
    /// Ordered first so it heads the grants of a database, which is where a reader looks to
    /// understand the rest.
    Public,
    Role(RoleName),
}

impl Grantee {
    /// The name the document records. `PUBLIC` is uppercase because that is how `GRANT`
    /// spells it and how no role may be named.
    pub fn as_str(&self) -> &str {
        match self {
            Self::Public => "PUBLIC",
            Self::Role(name) => name.as_str(),
        }
    }
}
