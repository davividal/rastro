//! How a node may be asked, decided before it is.

/// How a node may be asked.
///
/// **Decided from the node's settings, never by trying.** v1 speaks plain HTTP only, and a
/// plaintext request to a TLS listener is a request the node did not want. The settings decide
/// it rather than the version: an 8.x node with security switched off serves plain HTTP
/// exactly as 7.17 does, measured.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transport {
    /// Plain HTTP, the only transport v1 asks over.
    Plain,

    /// The listener wants TLS, which v1 does not speak, so the node is not asked at all.
    TlsRequired,
}
