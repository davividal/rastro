//! How a node is asked, decided before it is.

/// The protocol a node's HTTP listener speaks.
///
/// **Asked of the listener with a TLS handshake and nothing after it**, which no node logs,
/// measured: a plaintext request to a TLS listener is a WARN in the node's log, and the settings
/// that once decided it could not see TLS switched on by an `-E` that left with its launcher.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Transport {
    #[default]
    Plain,
    Tls,
}

impl Transport {
    /// The URL scheme it is, which is how an operator names it.
    pub fn scheme(&self) -> &'static str {
        match self {
            Self::Plain => "http",
            Self::Tls => "https",
        }
    }
}
