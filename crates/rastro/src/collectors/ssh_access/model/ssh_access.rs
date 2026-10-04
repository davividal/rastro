//! Who can log in to this box, and how.

use std::collections::BTreeMap;

use rastro_collector::{CollectionError, Observation};

use super::authorized_key::AuthorizedKey;
use super::ssh_server::SshServer;

/// The server's settings and every account's keys.
///
/// Accounts are keyed by name, and an account with a readable key file but nothing in it is
/// present with an empty list — which is not the same as an account with no key file at all,
/// and the latter is simply not a key here. An account's keys keep sshd's search order: the
/// files in `AuthorizedKeysFile` order, then the lines in file order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SshAccess {
    server: SshServer,
    accounts: BTreeMap<String, Vec<AuthorizedKey>>,
}

impl SshAccess {
    pub fn new(
        server: SshServer,
        accounts: impl IntoIterator<Item = (String, Vec<AuthorizedKey>)>,
    ) -> Result<Self, CollectionError> {
        let mut filed: BTreeMap<String, Vec<AuthorizedKey>> = BTreeMap::new();

        for (account, keys) in accounts {
            // sshd searches every pattern, so keys from several files accumulate, in the order
            // given: the first line that matches a key decides its options.
            filed.entry(account).or_default().extend(keys);
        }

        Ok(Self {
            server,
            accounts: filed,
        })
    }

    pub fn server(&self) -> &SshServer {
        &self.server
    }

    pub fn accounts(&self) -> &BTreeMap<String, Vec<AuthorizedKey>> {
        &self.accounts
    }
}

impl From<&SshAccess> for Observation {
    fn from(access: &SshAccess) -> Self {
        Observation::object([
            (
                "accounts",
                Observation::object(access.accounts().iter().map(|(account, keys)| {
                    (
                        account.as_str(),
                        Observation::sequence(keys.iter().map(Observation::from)),
                    )
                })),
            ),
            ("server", Observation::from(access.server())),
        ])
    }
}
