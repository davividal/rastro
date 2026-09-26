//! The `ACL LIST` reply: one line per account, `user <name> <rules…>`.
//!
//! ```text
//! user default on nopass sanitize-payload ~* &* +@all
//! ```

use std::collections::BTreeMap;

use rastro_collector::CollectionError;

use super::reply::Reply;
use crate::collectors::redis::model::Accounts;

/// The word every account line starts with.
const USER: &str = "user";

/// The reader of `ACL LIST`.
pub struct AclList;

impl AclList {
    /// The accounts, by name, each with its rules in the server's order.
    ///
    /// **Split on whitespace, which loses nothing.** A redis 7 selector, `(~cache:* +get)`, holds
    /// spaces and comes apart into two tokens, but the tokens stay in order and joining them
    /// gives back what the server printed.
    pub fn parse(reply: Reply) -> Result<Accounts, CollectionError> {
        let Reply::Array(lines) = reply else {
            return Err(CollectionError::new(format!(
                "the server answered ACL LIST with {reply:?} rather than a list of accounts"
            )));
        };

        let mut users = BTreeMap::new();
        for line in lines {
            let Reply::Bulk(line) = line else {
                return Err(CollectionError::new(
                    "the server's ACL LIST holds something other than text, so it was misread",
                ));
            };

            let mut words = line.split_whitespace();
            let (Some(USER), Some(name)) = (words.next(), words.next()) else {
                return Err(CollectionError::new(format!(
                    "the server's ACL LIST has a line that names no account, so it was misread: \
                     it starts {:?}",
                    line.split_whitespace().next().unwrap_or_default()
                )));
            };

            let rules: Vec<String> = words.map(str::to_owned).collect();
            if users.insert(name.to_owned(), rules).is_some() {
                return Err(CollectionError::new(format!(
                    "the server's ACL LIST names {name} twice, so it was misread"
                )));
            }
        }

        Ok(Accounts { users })
    }
}
