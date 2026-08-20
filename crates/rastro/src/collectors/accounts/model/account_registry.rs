//! Everyone and every group the host knows about locally.

use std::collections::BTreeMap;

use rastro_collector::{CollectionError, Observation};

use super::group_account::GroupAccount;
use super::user_account::UserAccount;
use crate::collectors::accounts::value_objects::{GroupName, UserName};

/// The local account database, users and groups together.
///
/// **One facet rather than two, because they are one state surface.** A user's
/// primary group is a gid in the user's own record and a group's secondary members
/// are names in the group's, so neither file answers "who can do what" on its own,
/// and nobody auditing a box wants one without the other. Splitting them would also
/// let a config exclude half of an answer.
///
/// **Local only, and that is a real boundary rather than an omission.** These are
/// the files; a host with LDAP or SSSD has accounts that appear in `getent passwd`
/// and in neither file. Reporting the files as though they were the whole answer
/// would be a lie, so the facet's name says what it read.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AccountRegistry {
    users: BTreeMap<UserName, UserAccount>,
    groups: BTreeMap<GroupName, GroupAccount>,
}

impl AccountRegistry {
    /// Files each account under its name.
    ///
    /// **A repeated name is refused, and here that is a statement about the host
    /// rather than about rastro.** The package collector refuses a repeated key
    /// because no package manager can emit one, so a repeat proves a misread. These
    /// are text files an operator edits, and a second `root` line is something
    /// `useradd` will not create but `vi` will. It is also invisible in normal use,
    /// because every lookup returns the first match and the second entry simply
    /// never applies. Keying would silently drop it; refusing puts it on stderr,
    /// which for a duplicated privileged account is the only acceptable outcome.
    pub fn new(
        users: impl IntoIterator<Item = (UserName, UserAccount)>,
        groups: impl IntoIterator<Item = (GroupName, GroupAccount)>,
    ) -> Result<Self, CollectionError> {
        let mut filed_users = BTreeMap::new();
        for (name, account) in users {
            if filed_users.insert(name.clone(), account).is_some() {
                return Err(CollectionError::new(format!(
                    "the user {:?} is defined twice, so every lookup silently ignores one \
                     of the two definitions",
                    name.as_str()
                )));
            }
        }

        let mut filed_groups = BTreeMap::new();
        for (name, group) in groups {
            if filed_groups.insert(name.clone(), group).is_some() {
                return Err(CollectionError::new(format!(
                    "the group {:?} is defined twice, so every lookup silently ignores one \
                     of the two definitions",
                    name.as_str()
                )));
            }
        }

        Ok(Self {
            users: filed_users,
            groups: filed_groups,
        })
    }

    pub fn users(&self) -> &BTreeMap<UserName, UserAccount> {
        &self.users
    }

    pub fn groups(&self) -> &BTreeMap<GroupName, GroupAccount> {
        &self.groups
    }
}

impl From<&AccountRegistry> for Observation {
    fn from(registry: &AccountRegistry) -> Self {
        Observation::object([
            (
                "groups",
                Observation::object(
                    registry
                        .groups()
                        .iter()
                        .map(|(name, group)| (name.as_str(), Observation::from(group))),
                ),
            ),
            (
                "users",
                Observation::object(
                    registry
                        .users()
                        .iter()
                        .map(|(name, account)| (name.as_str(), Observation::from(account))),
                ),
            ),
        ])
    }
}
