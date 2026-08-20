//! Finding every account's key files.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use rastro_collector::CollectionError;

use super::authorized_keys;
use super::sshd::Sshd;
use crate::collectors::ssh_access::model::{AuthorizedKey, SshAccess, SshServer};

/// Where the accounts and their home directories are listed.
const ETC_PASSWD: &str = "/etc/passwd";

/// How many columns a passwd line has.
const PASSWD_COLUMNS: usize = 7;

/// The token sshd expands to an account's home directory.
const HOME_TOKEN: &str = "%h";

/// The token sshd expands to the account's name.
const USER_TOKEN: &str = "%u";

/// The accounts and their authorized keys, as a source rastro can read.
///
/// # Why this reads `/etc/passwd` itself
///
/// It needs one thing from it — each account's home directory — and the accounts facet's own
/// reader builds a whole `UserAccount`, shadow join and all. Calling into it would couple two
/// collectors so that a change to one facet's model could break the other, for the sake of two
/// columns. So this parses the two columns it needs and nothing else. The duplication is real
/// and is recorded as such rather than hidden.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SshFiles {
    sshd: Sshd,
    passwd: PathBuf,
}

impl SshFiles {
    /// Finds sshd, or reports that this host does not run one.
    pub fn detect() -> Option<Self> {
        Sshd::detect().map(|sshd| Self {
            sshd,
            passwd: PathBuf::from(ETC_PASSWD),
        })
    }

    /// The same over an sshd and a passwd file the caller chose.
    pub fn using(sshd: Sshd, passwd: impl Into<PathBuf>) -> Self {
        Self {
            sshd,
            passwd: passwd.into(),
        }
    }

    /// Asks sshd for its effective settings, then reads every account's key files.
    pub fn read(&self) -> Result<SshAccess, CollectionError> {
        let server = self.sshd.read()?;
        let accounts = self.read_accounts(&server)?;

        SshAccess::new(server, accounts)
    }

    /// The keys of every account that has any.
    ///
    /// Separate from [`Self::read`] so the resolution of sshd's patterns is exercised from a
    /// fixture.
    pub fn read_accounts(
        &self,
        server: &SshServer,
    ) -> Result<Vec<(String, Vec<AuthorizedKey>)>, CollectionError> {
        let homes = self.read_homes()?;
        let mut accounts = Vec::new();

        for (account, home) in homes {
            let mut keys = Vec::new();
            let mut found = false;

            for pattern in &server.authorized_keys_files {
                let path = resolve(pattern, &account, &home);
                if let Some(contents) = read_optional(&path)? {
                    found = true;
                    keys.extend(authorized_keys::parse(&contents).map_err(|error| {
                        CollectionError::new(format!("in {}: {error}", path.display()))
                    })?);
                }
            }

            // Only accounts with a key file at all are reported. An account with none is not
            // an account with no keys in a way worth a key list: every system account on the
            // box would otherwise appear with an empty one, drowning the handful that matter.
            if found {
                accounts.push((account, keys));
            }
        }

        Ok(accounts)
    }

    /// Each account's name and home directory, from the two columns this needs.
    fn read_homes(&self) -> Result<BTreeMap<String, PathBuf>, CollectionError> {
        let contents = fs::read_to_string(&self.passwd).map_err(|error| {
            CollectionError::new(format!("could not read {}: {error}", self.passwd.display()))
        })?;

        let mut homes = BTreeMap::new();
        for line in contents.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            let columns: Vec<&str> = line.split(':').collect();
            if columns.len() != PASSWD_COLUMNS {
                return Err(CollectionError::new(format!(
                    "expected {PASSWD_COLUMNS} colon-separated columns in {}, got {}",
                    self.passwd.display(),
                    columns.len()
                )));
            }

            let (account, home) = (columns[0], columns[5]);
            if account.is_empty() || home.is_empty() {
                continue;
            }
            homes.insert(account.to_owned(), PathBuf::from(home));
        }

        Ok(homes)
    }
}

/// Expands one of sshd's `AuthorizedKeysFile` patterns.
///
/// **A relative pattern is relative to the account's home**, which is what makes the default
/// `.ssh/authorized_keys` mean what everybody assumes it means. An absolute one is used as it
/// stands, which is how a box that centralises keys under `/etc/ssh/authorized_keys/%u` works.
/// `%h` and `%u` are expanded, and `%%` is a literal `%`.
pub fn resolve(pattern: &str, account: &str, home: &Path) -> PathBuf {
    let expanded = pattern
        .replace(HOME_TOKEN, &home.to_string_lossy())
        .replace(USER_TOKEN, account)
        .replace("%%", "%");

    let expanded = PathBuf::from(expanded);
    if expanded.is_absolute() {
        return expanded;
    }

    home.join(expanded)
}

fn read_optional(path: &Path) -> Result<Option<String>, CollectionError> {
    match fs::read_to_string(path) {
        Ok(contents) => Ok(Some(contents)),
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
            ) =>
        {
            Ok(None)
        }
        Err(error) => Err(CollectionError::new(format!(
            "could not read {}: {error}",
            path.display()
        ))),
    }
}
