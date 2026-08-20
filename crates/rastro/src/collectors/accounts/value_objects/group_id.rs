//! Which number the kernel knows a group by.

use rastro_collector::{CollectionError, Observation};

/// A numeric group id.
///
/// The counterpart to [`UserId`](super::UserId), separate for the reason given
/// there: a user's own id and their primary group's id sit one column apart, and
/// nothing but the type stops them being read into each other's slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GroupId(u32);

impl GroupId {
    pub fn parse(value: &str) -> Result<Self, CollectionError> {
        value
            .parse::<u32>()
            .map(Self)
            .map_err(|_| CollectionError::new(format!("{value:?} is not a group id")))
    }

    pub fn as_u32(&self) -> u32 {
        self.0
    }
}

impl From<&GroupId> for Observation {
    fn from(id: &GroupId) -> Self {
        Observation::integer(i64::from(id.as_u32()))
    }
}
