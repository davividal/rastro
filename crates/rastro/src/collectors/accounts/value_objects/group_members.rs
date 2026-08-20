//! Who a group lists.

use std::collections::BTreeSet;

use rastro_collector::Observation;

use super::user_name::UserName;

/// The users a group names, as the set it is.
///
/// A set, not a list, so ordering is a property of the type rather than of a `sort`
/// call somebody has to remember. The file's own order is the order names were
/// appended in, which changes when a user is removed and re-added without anything
/// about the host's access having changed.
///
/// **These are the group's *secondary* members only, and that is not the same as
/// everyone in the group.** A user whose primary group this is does not appear in
/// the list at all: `postgres` is in group `postgres` by way of the fourth column of
/// its own entry, not by being named here. Joining the two is deliberately left
/// undone, because a collector records what the host said and composing a complete
/// membership from two files is presentation.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GroupMembers(BTreeSet<UserName>);

impl GroupMembers {
    pub fn new(members: impl IntoIterator<Item = UserName>) -> Self {
        Self(members.into_iter().collect())
    }

    /// The members in order, which is the sorted order of their names.
    pub fn iter(&self) -> impl Iterator<Item = &UserName> {
        self.0.iter()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl From<&GroupMembers> for Observation {
    fn from(members: &GroupMembers) -> Self {
        Observation::list(
            members
                .iter()
                .map(|member| Observation::text(member.as_str())),
        )
    }
}
