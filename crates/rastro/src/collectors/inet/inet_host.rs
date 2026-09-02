//! Which address a socket is bound to.

use rastro_collector::{CollectionError, NonEmptyText, Observation};

/// The address half of an internet socket's local end.
///
/// **Kept as text and not parsed into an address type**, because the exporters facet shares
/// this leaf and reaches it from configuration, where a name rather than an address is
/// ordinary. What the sockets facet puts here is always the printed form of an address the
/// kernel published.
///
/// **A wildcard is the most important value here**, because it is the difference between a
/// service reachable from the network and one reachable only from the box. There are two
/// spellings and not three: `0.0.0.0` for IPv4 and `::` for IPv6.
///
/// `ss` prints a third, `*`, for a dual-stack socket that serves both families through one
/// IPv6 binding, and it is deliberately absent. `ss` derives it from a socket option the
/// kernel returns over diag netlink and no `/proc` column carries, so rastro cannot observe
/// it. Nothing is lost from the document by that: a dual-stack socket appears once as an
/// IPv6 wildcard, and a family-separated pair appears as two rows, so the arrangement is
/// still readable from the facet. Only the spelling of one row changed.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct InetHost(NonEmptyText);

impl InetHost {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(NonEmptyText::new(value, "socket address")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    /// Whether this address accepts connections from off the box.
    ///
    /// Not recorded as a field, because it is a conclusion rather than an observation and
    /// the renderer's job. It exists so a test can state the distinction the doc above
    /// claims matters.
    pub fn is_a_wildcard(&self) -> bool {
        matches!(self.as_str(), "0.0.0.0" | "::")
    }
}

impl From<&InetHost> for Observation {
    fn from(host: &InetHost) -> Self {
        Observation::text(host.as_str())
    }
}
