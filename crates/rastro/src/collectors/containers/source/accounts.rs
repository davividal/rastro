//! Who a uid belongs to, and where their home is.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

/// Where the accounts of a box are listed.
const PASSWD: &str = "etc/passwd";

/// The columns this read needs out of a passwd line.
const NAME: usize = 0;
const USER_ID: usize = 2;
const HOME: usize = 5;

/// How many columns a passwd line has.
const COLUMNS: usize = 7;

/// One account, as much of it as a rootless engine needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Account {
    pub name: String,
    pub home: String,
}

/// The accounts of this box, keyed by uid.
///
/// **Read here rather than taken from the `accounts` facet**, because a collector may not
/// read another collector: what one facet knows is not a channel the others may use. The
/// read is three columns of a file every Unix has, and it exists for one purpose — a
/// rootless engine belongs to a user, and a document that called it `1000` would make the
/// reader go and look the number up.
///
/// An account with no passwd entry is not an error: a box can run a service as a uid nobody
/// named, and the caller falls back to the number.
pub fn accounts(root: impl AsRef<Path>) -> BTreeMap<u32, Account> {
    let Ok(text) = fs::read_to_string(root.as_ref().join(PASSWD)) else {
        return BTreeMap::new();
    };

    let mut accounts = BTreeMap::new();

    for line in text.lines() {
        let columns: Vec<&str> = line.split(':').collect();
        if columns.len() != COLUMNS {
            continue;
        }

        let Ok(user_id) = columns[USER_ID].parse::<u32>() else {
            continue;
        };

        accounts.insert(
            user_id,
            Account {
                name: columns[NAME].to_owned(),
                home: columns[HOME].to_owned(),
            },
        );
    }

    accounts
}
