//! What kind of socket is listening.

use rastro_collector::{CollectionError, NonEmptyText, Observation};

/// The family and type of a socket, in the word `ss` prints for it.
///
/// `tcp` and `udp` for the internet families, and `u_str`, `u_dgr` or `u_seq` for a unix
/// socket by its type. `ss` knows more that this collector does not ask for: `nl`,
/// `raw`, `p_raw`, `vsock`, `xdp`.
///
/// Text rather than an enum, on the same reasoning as systemd's unit-file state: the set
/// belongs to iproute2 and has grown with each address family Linux gained, so refusing
/// an unfamiliar word would make rastro fail on a newer kernel rather than describe it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct SocketKind(NonEmptyText);

impl SocketKind {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(NonEmptyText::new(value, "socket kind")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<&SocketKind> for Observation {
    fn from(kind: &SocketKind) -> Self {
        Observation::text(kind.as_str())
    }
}
