//! Which number the kernel knows a user by.

use rastro_collector::{CollectionError, Observation};

/// A numeric user id.
///
/// Held as a `u32` because that is what a `uid_t` is, and the width is the whole
/// check: the account database is a text file anyone can edit, and a negative or
/// oversized number in the third column means the line was tokenised into the wrong
/// slots rather than that the host has an exotic account.
///
/// Distinct from [`GroupId`](super::GroupId) even though both wrap the same integer.
/// Every user carries one of each, one column apart, and a type that let them be
/// swapped would make that mistake compile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UserId(u32);

impl UserId {
    pub fn parse(value: &str) -> Result<Self, CollectionError> {
        value
            .parse::<u32>()
            .map(Self)
            .map_err(|_| CollectionError::new(format!("{value:?} is not a user id")))
    }

    pub fn as_u32(&self) -> u32 {
        self.0
    }
}

impl From<&UserId> for Observation {
    fn from(id: &UserId) -> Self {
        Observation::integer(i64::from(id.as_u32()))
    }
}
