//! What a server says it is.

use crate::collectors::redis::value_objects::ServerKind;

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
