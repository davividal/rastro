//! One account that can own files and log in.

use rastro_collector::{AbsolutePath, Observation};

use super::password_aging::PasswordAging;
use super::password_status::PasswordStatus;
use crate::collectors::accounts::value_objects::{Comment, GroupId, UserId};

/// A user, in rastro's terms rather than any one file's.
///
/// The name is not a field here, because it is the key this is filed under.
///
/// **The primary group is a number and stays one.** Resolving it to a name would
/// need the group database, and joining two files is presentation rather than
/// observation: a collector records what the host said. It also cannot always be
/// done, since a passwd entry may name a gid no group claims, and inventing a name
/// for one would hide exactly that fault.
///
/// **Credentials are absent by construction.** [`PasswordStatus`] carries whether a
/// password exists and what hashed it, never the hash, so no run of rastro can put a
/// credential on stdout however it is invoked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserAccount {
    pub user_id: UserId,
    pub primary_group_id: GroupId,
    pub comment: Comment,
    /// Blank in the file for a handful of system accounts, where the kernel then
    /// treats the account as having `/` as its home. Recorded as absent rather than
    /// as `/`, because the file says nothing and rastro does not guess.
    pub home_directory: Option<AbsolutePath>,
    /// Blank means the system default, conventionally `/bin/sh`. Left absent for the
    /// same reason as the home directory: the substitution is the kernel's, not the
    /// file's.
    pub login_shell: Option<AbsolutePath>,
    /// Absent when the host keeps no shadow database at all, which is the only case
    /// where nothing is known about this account's password.
    pub password: Option<PasswordStatus>,
    /// Absent for the same reason as the password.
    pub aging: Option<PasswordAging>,
}

impl From<&UserAccount> for Observation {
    fn from(account: &UserAccount) -> Self {
        Observation::object([
            (
                "aging",
                account
                    .aging
                    .as_ref()
                    .map_or_else(Observation::null, Observation::from),
            ),
            ("comment", Observation::from(&account.comment)),
            ("home_directory", path(account.home_directory.as_ref())),
            ("login_shell", path(account.login_shell.as_ref())),
            (
                "password",
                account
                    .password
                    .as_ref()
                    .map_or_else(Observation::null, Observation::from),
            ),
            (
                "primary_group_id",
                Observation::from(&account.primary_group_id),
            ),
            ("user_id", Observation::from(&account.user_id)),
        ])
    }
}

fn path(value: Option<&AbsolutePath>) -> Observation {
    value.map_or_else(Observation::null, |path| Observation::text(path.as_str()))
}
