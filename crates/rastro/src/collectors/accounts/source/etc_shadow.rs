//! The `/etc/shadow` interface.
//!
//! The file that holds the credentials, which is why the only things this module
//! lets out of itself are a state and an algorithm name. The hash column is read,
//! classified and dropped where the line is parsed; no type here has a field it
//! could be stored in.
//!
//! **This is also the one parser here whose failures do not quote the offending
//! line.** Every other one does, because that is what makes a failure actionable,
//! and a malformed shadow line would carry a hash into stderr and from there into
//! whatever collected it. The column count goes into the message instead, which is
//! enough to tell a truncated file from a well-formed one. Nothing in the type
//! system enforces that, so `a_shadow_failure_never_quotes_the_line_it_came_from`
//! in `tests/accounts.rs` does.

use std::collections::BTreeMap;

use rastro_collector::CollectionError;

use super::etc_passwd::carries_an_entry;
use crate::collectors::accounts::model::{PasswordAging, PasswordStatus, optional_days};
use crate::collectors::accounts::value_objects::UserName;

/// How many columns a shadow line has.
const COLUMNS: usize = 9;

/// What the shadow database knows about one account.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShadowEntry {
    pub password: PasswordStatus,
    pub aging: PasswordAging,
}

/// The whole shadow database, keyed by the account each line is about.
pub type ShadowDatabase = BTreeMap<UserName, ShadowEntry>;

impl ShadowEntry {
    /// Translates the file's text into entries, keyed by account name.
    ///
    /// Keyed here rather than listed, because the only thing the caller ever does
    /// with this is look an account up while walking the passwd file.
    pub fn parse(text: &str) -> Result<ShadowDatabase, CollectionError> {
        let mut entries = BTreeMap::new();

        for line in text.lines().filter(|line| carries_an_entry(line)) {
            let (name, entry) = Self::parse_line(line)?;
            if entries.insert(name.clone(), entry).is_some() {
                return Err(CollectionError::new(format!(
                    "the shadow database defines {:?} twice, so which password applies \
                     depends on which line a tool reads first",
                    name.as_str()
                )));
            }
        }

        Ok(entries)
    }

    fn parse_line(line: &str) -> Result<(UserName, Self), CollectionError> {
        let columns: Vec<&str> = line.split(':').collect();
        let [
            name,
            password,
            last_changed,
            minimum,
            maximum,
            warning,
            inactive,
            expires,
            _reserved,
        ] = columns.as_slice()
        else {
            return Err(CollectionError::new(format!(
                "expected {COLUMNS} colon-separated columns in an /etc/shadow line, got {}",
                columns.len()
            )));
        };

        // The one place a hash is in scope, and it leaves as a classification.
        let entry = Self {
            password: PasswordStatus::parse(password),
            aging: PasswordAging {
                last_changed_days_since_epoch: optional_days(last_changed)?,
                minimum_age_days: optional_days(minimum)?,
                maximum_age_days: optional_days(maximum)?,
                warning_days: optional_days(warning)?,
                inactive_days: optional_days(inactive)?,
                expires_days_since_epoch: optional_days(expires)?,
            },
        };

        Ok((UserName::new(*name)?, entry))
    }
}
