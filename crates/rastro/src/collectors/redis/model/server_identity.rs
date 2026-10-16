//! What a server says it is.

use crate::collectors::redis::value_objects::{ServerKind, is_supported};

/// A server's own account of itself, from `INFO server`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerIdentity {
    /// The family, as the server names it.
    ///
    /// Outranks the process's `comm` once a server has answered: Debian's valkey compatibility
    /// package installs a `redis-server` symlink, and the kernel records the name it was started
    /// under.
    pub kind: ServerKind,

    /// The release, read from the field that is actually that family's.
    pub version: String,

    /// `standalone`, `cluster` or `sentinel`, verbatim.
    ///
    /// Recorded rather than acted on: this facet reads one box, and a cluster's topology is a
    /// read of many.
    pub mode: Option<String>,

    /// The binary the server was started from, where the server reports one.
    pub executable: Option<String>,

    /// The file the server read at start, where it read one.
    ///
    /// **The file's name, never its contents as configuration.** What the server is running with
    /// comes from the server; this says which file an operator would have to edit.
    pub config_file: Option<String>,
}

/// The first release with accounts, `ACL`.
///
/// One rule for both families, because valkey forked from redis 7.2 and has had them from its
/// first release.
const ACCOUNTS_SINCE: u32 = 6;

impl ServerIdentity {
    /// Whether this server can have accounts at all, so that asking for them is not a question
    /// it cannot understand.
    ///
    /// A version that does not parse is asked anyway: the refusal, if one comes, then says what
    /// really happened rather than rastro guessing it.
    pub fn has_accounts(&self) -> bool {
        self.version
            .split('.')
            .next()
            .and_then(|major| major.parse::<u32>().ok())
            .is_none_or(|major| major >= ACCOUNTS_SINCE)
    }

    /// Why this server is read on a best-effort basis, where its release is not a supported one.
    ///
    /// Read with the same rules all the same, so what the collector can handle appears and what it
    /// cannot is a refused item with its reason: nothing about an older server is an error.
    pub fn unsupported(&self) -> Option<String> {
        (!is_supported(self.kind, &self.version)).then(|| {
            format!(
                "{} {} is not a release rastro supports, so it is read on a best-effort basis",
                self.kind.as_str(),
                self.version
            )
        })
    }
}
