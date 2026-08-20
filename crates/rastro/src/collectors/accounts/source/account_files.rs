//! The three files the local account database is spread across.

use std::fs;
use std::path::{Path, PathBuf};

use rastro_collector::CollectionError;

use super::etc_group::EtcGroup;
use super::etc_passwd::{EtcPasswd, PasswdEntry};
use super::etc_shadow::{ShadowDatabase, ShadowEntry};
use crate::collectors::accounts::model::{AccountRegistry, UserAccount};
use crate::collectors::accounts::value_objects::UserName;

const ETC_PASSWD: &str = "/etc/passwd";
const ETC_GROUP: &str = "/etc/group";
const ETC_SHADOW: &str = "/etc/shadow";

/// The account database as a source rastro can read.
///
/// **Three files behind one source, because they are one answer.** Splitting them
/// into three sources would put the join somewhere above, and the join is the part
/// with the interesting rules: which file may be missing, and what it means when two
/// of them disagree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountFiles {
    passwd: PathBuf,
    group: PathBuf,
    shadow: PathBuf,
}

impl AccountFiles {
    pub fn new() -> Self {
        Self {
            passwd: PathBuf::from(ETC_PASSWD),
            group: PathBuf::from(ETC_GROUP),
            shadow: PathBuf::from(ETC_SHADOW),
        }
    }

    pub fn at(
        passwd: impl Into<PathBuf>,
        group: impl Into<PathBuf>,
        shadow: impl Into<PathBuf>,
    ) -> Self {
        Self {
            passwd: passwd.into(),
            group: group.into(),
            shadow: shadow.into(),
        }
    }

    pub fn passwd(&self) -> &Path {
        &self.passwd
    }

    /// Whether this host keeps a local account database at all.
    ///
    /// The passwd file is what decides it. A host without one is not broken and not
    /// unreadable: an image built from scratch genuinely has no local accounts, and
    /// that is state rather than a failure.
    pub fn exists(&self) -> bool {
        self.passwd.is_file()
    }

    /// Reads all three files and assembles the registry.
    pub fn read(&self) -> Result<AccountRegistry, CollectionError> {
        let users = EtcPasswd::parse(&self.contents_of(&self.passwd)?)?;
        let groups = EtcGroup::parse(&self.contents_of(&self.group)?)?;
        let shadow = self.read_shadow()?;

        let accounts = users
            .into_iter()
            .map(|entry| self.account_of(entry, shadow.as_ref()))
            .collect::<Result<Vec<_>, CollectionError>>()?;

        AccountRegistry::new(accounts, groups)
    }

    /// The shadow database, or nothing if this host keeps none.
    ///
    /// **A missing file and an unreadable one are opposite answers.** No shadow file
    /// means an old or minimal layout where passwords live in the passwd file itself,
    /// which is a host rastro can describe honestly. A shadow file it cannot read
    /// means rastro is not root, and rastro requires root: reporting the accounts
    /// while silently dropping every password state would leave an operator with a
    /// fingerprint that looks complete and answers nothing about who can log in.
    fn read_shadow(&self) -> Result<Option<ShadowDatabase>, CollectionError> {
        if !self.shadow.is_file() {
            return Ok(None);
        }

        ShadowEntry::parse(&self.contents_of(&self.shadow)?).map(Some)
    }

    /// One user, with whatever the shadow database adds to it.
    ///
    /// **A user the shadow database does not mention is refused when a shadow
    /// database exists.** `pwck` reports the same fault. The alternative is to record
    /// the password as absent, and absent already means "logs in with no password at
    /// all", so guessing here would turn an inconsistency into a claim that an
    /// account is wide open.
    ///
    /// The opposite direction is deliberately tolerated: a shadow line naming no
    /// account grants nothing to anybody, because every lookup starts from the passwd
    /// file, so it is a stale leftover rather than an ambiguity about access.
    fn account_of(
        &self,
        entry: PasswdEntry,
        shadow: Option<&ShadowDatabase>,
    ) -> Result<(UserName, UserAccount), CollectionError> {
        let known: Option<&ShadowEntry> = match shadow {
            None => None,
            Some(shadow) => Some(shadow.get(&entry.name).ok_or_else(|| {
                CollectionError::new(format!(
                    "{:?} is in {} but not in {}, so nothing can be said about whether it \
                     has a password",
                    entry.name.as_str(),
                    self.passwd.display(),
                    self.shadow.display()
                ))
            })?),
        };

        let account = UserAccount {
            user_id: entry.user_id,
            primary_group_id: entry.primary_group_id,
            comment: entry.comment,
            home_directory: entry.home_directory,
            login_shell: entry.login_shell,
            password: known.map(|known| known.password.clone()),
            aging: known.map(|known| known.aging.clone()),
        };

        Ok((entry.name, account))
    }

    fn contents_of(&self, path: &Path) -> Result<String, CollectionError> {
        fs::read_to_string(path).map_err(|error| {
            CollectionError::new(format!("could not read {}: {error}", path.display()))
        })
    }
}

impl Default for AccountFiles {
    fn default() -> Self {
        Self::new()
    }
}
