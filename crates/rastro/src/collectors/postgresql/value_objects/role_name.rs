//! What a role is called.

use rastro_collector::{CollectionError, NonEmptyText};

/// The name of a login role or a group role.
///
/// One type for both, because PostgreSQL has one concept: `CREATE USER` is `CREATE ROLE`
/// with `LOGIN`, and whether a role can log in is an attribute rather than a kind. Modelling
/// users and groups separately would invent a distinction the server does not make and lose
/// the one it does.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct RoleName(NonEmptyText);

impl RoleName {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(NonEmptyText::new(value, "role name")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}
