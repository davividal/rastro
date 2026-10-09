//! The password the `default` account accepts, worked out from its files before anything is sent.
//!
//! **Measured on redis 8.0.2, every rule.** A `user default` line outranks `requirepass`
//! whatever their order, because the server resets the account and applies the line after the
//! rest of the file; with an `aclfile`, `requirepass` is ignored entirely. `CONFIG REWRITE`, the
//! ordinary way to persist a runtime change, keeps `requirepass` and appends
//! `user default on #<sha256>` beside it, so a file that has been rewritten once holds two
//! statements about one password, and only the account line counts.
//!
//! So the account is replayed the way the server builds it, and a password is sent only where
//! the account as the files leave it would accept it. The verifier being an unsalted `sha256`
//! is what makes that checkable here rather than by asking the server, which would be an
//! `ACL LOG` entry for every wrong answer.

use std::collections::BTreeSet;

use sha2::{Digest, Sha256};

/// The default account, as the rules of its line leave it.
#[derive(Default)]
struct Account {
    enabled: bool,
    no_password: bool,
    hashes: BTreeSet<String>,
    /// Every plaintext a `>` rule gave, kept only as candidates: one a later rule removed is
    /// no longer among the hashes, and a candidate is sent only where its hash is.
    passwords: Vec<String>,
}

/// The password to send as the `default` account, or why none may be sent.
///
/// `requirepass` is the value from the command line or the file, whichever the server applied
/// last; `default_user` is the account's rules, from the configuration file or the ACL file.
/// Without rules, `requirepass` is the password. With them, `requirepass` is only a candidate,
/// sent where it matches a hash the account holds, which is the state `CONFIG REWRITE` leaves.
pub fn password_for_default_account(
    requirepass: Option<&str>,
    default_user: Option<&[String]>,
) -> Result<String, String> {
    let Some(rules) = default_user else {
        return requirepass.map(str::to_owned).ok_or_else(|| {
            "the server's configuration sets no password, so it was set at runtime, where \
                 rastro has no way to read it"
                .to_owned()
        });
    };

    let account = account_from(rules);
    if !account.enabled {
        return Err(
            "the default account is switched off in the server's configuration, so no password \
             opens it"
                .to_owned(),
        );
    }
    if account.no_password {
        return Err(
            "the server's configuration leaves the default account without a password, and the \
             server asks for one, so the two disagree"
                .to_owned(),
        );
    }
    if account.hashes.is_empty() {
        return Err(
            "the server's configuration leaves the default account with no password at all"
                .to_owned(),
        );
    }

    requirepass
        .map(str::to_owned)
        .into_iter()
        .chain(account.passwords)
        .find(|candidate| account.hashes.contains(&hash_of(candidate)))
        .ok_or_else(|| {
            "the server's configuration gives the default account's password only as a hash, and \
             no password rastro has matches it"
                .to_owned()
        })
}

/// Replays an account's rules from a reset account, as the server does.
///
/// Only the rules that touch passwords and the switch are read; a key or command rule changes
/// nothing about which password opens the account.
fn account_from(rules: &[String]) -> Account {
    let mut account = Account::default();

    for rule in rules {
        // Rule words are case-insensitive in redis, measured; a password rule is not lowercased.
        match rule.to_ascii_lowercase().as_str() {
            "on" => account.enabled = true,
            "off" => account.enabled = false,
            "nopass" => {
                account.no_password = true;
                account.hashes.clear();
                account.passwords.clear();
            }
            "resetpass" => account.clear_passwords(),
            "reset" => {
                account.clear_passwords();
                account.enabled = false;
            }
            _ => account.apply_password_rule(rule),
        }
    }

    account
}

impl Account {
    fn clear_passwords(&mut self) {
        self.no_password = false;
        self.hashes.clear();
        self.passwords.clear();
    }

    fn apply_password_rule(&mut self, rule: &str) {
        if let Some(password) = rule.strip_prefix('>') {
            self.no_password = false;
            self.hashes.insert(hash_of(password));
            self.passwords.push(password.to_owned());
        } else if let Some(password) = rule.strip_prefix('<') {
            self.hashes.remove(&hash_of(password));
        } else if let Some(hash) = rule.strip_prefix('#') {
            self.no_password = false;
            self.hashes.insert(hash.to_owned());
        } else if let Some(hash) = rule.strip_prefix('!') {
            self.hashes.remove(hash);
        }
    }
}

/// The verifier redis keeps for a password: `sha256`, lowercase hex, no salt.
fn hash_of(password: &str) -> String {
    Sha256::digest(password.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
