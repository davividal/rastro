//! Who may connect to a server, and what each account may do.

use std::collections::BTreeMap;
use std::fmt;

use rastro_collector::Observation;

/// The prefix of a password verifier in an account's rules.
const VERIFIER: char = '#';

/// Every account a server knows, from `ACL LIST`, by name.
///
/// **Each account's rules in the server's order, never sorted.** A rule can undo an earlier one,
/// `+@all` then `-flushall`, so the sequence is the meaning. The server prints its accounts in
/// a canonical form, which is what makes that order stable between two runs of an unchanged box.
///
/// **A password verifier is carried and marked sensitive.** It is `sha256` of the password with
/// no salt at all, measured on three builds, so its stand-in is as guessable as the password
/// behind it, the same bargain `requirepass` already makes in the settings, and for the default
/// account it is the same secret. Withholding one while carrying the other would protect
/// nothing; carrying both lets a rotation show.
#[derive(Clone, PartialEq, Eq, Default)]
pub struct Accounts {
    pub users: BTreeMap<String, Vec<String>>,
}

/// Every rule but a verifier, so a `{:?}` in some later message cannot carry one.
impl fmt::Debug for Accounts {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_map()
            .entries(self.users.iter().map(|(name, rules)| {
                let shown: Vec<&str> = rules
                    .iter()
                    .map(|rule| match rule.starts_with(VERIFIER) {
                        true => "<withheld>",
                        false => rule.as_str(),
                    })
                    .collect();
                (name, shown)
            }))
            .finish()
    }
}

impl From<&Accounts> for Observation {
    fn from(accounts: &Accounts) -> Self {
        Observation::object(accounts.users.iter().map(|(name, rules)| {
            (
                name.as_str(),
                // A sequence: a later rule can undo an earlier one, so the order is the meaning.
                Observation::sequence(rules.iter().map(|rule| {
                    let observed = Observation::text(rule.as_str());

                    match rule.starts_with(VERIFIER) {
                        true => observed.sensitive(),
                        false => observed,
                    }
                })),
            )
        }))
    }
}
