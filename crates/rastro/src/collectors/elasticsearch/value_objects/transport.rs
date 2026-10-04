//! How a node is asked, decided before it is.

/// The protocol a node's HTTP listener speaks.
///
/// **Decided from the node's settings, never by trying.** A plaintext request to a TLS listener
/// is a WARN in the node's log, measured, and a handshake to a plain one is a request it could
/// not parse. The settings decide it rather than the version: an 8.x node with TLS switched off
/// serves plain HTTP exactly as 7.17 does.
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
