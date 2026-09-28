//! Whether a node shares the host's network namespace.

/// Whether a node listens in the namespace rastro runs in or in one of its own.
///
/// State rather than detail: a node moved into a container, or out of one, is reached by a
/// different route, and whether anything off the box can reach it changes with it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkNamespace {
    /// The namespace rastro runs in, which is a node on the host.
    Host,

    /// A namespace of the node's own, which is a node in a container or a sandbox.
    Separate,
}

impl NetworkNamespace {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Host => "host",
            Self::Separate => "separate",
        }
    }
}
