//! One group that files and accounts can belong to.

use rastro_collector::Observation;

use crate::collectors::accounts::value_objects::{GroupId, GroupMembers};

/// A group, in rastro's terms rather than any one file's.
///
/// The name is not a field here, because it is the key this is filed under.
///
/// **The group's own password column is dropped at the boundary and not recorded.**
/// It is a vestige: it exists so that `newgrp` can let a non-member join a group,
/// virtually nobody sets it, and on this box every one of the hundred and eight
/// groups holds the same `x`. What little it could say is a credential, and rastro
/// has nowhere safe to put one yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupAccount {
    pub group_id: GroupId,
    pub members: GroupMembers,
}

impl From<&GroupAccount> for Observation {
    fn from(group: &GroupAccount) -> Self {
        Observation::object([
            ("group_id", Observation::from(&group.group_id)),
            ("members", Observation::from(&group.members)),
        ])
    }
}
