//! Which address a socket is bound to.

use rastro_collector::{CollectionError, NonEmptyText, Observation};

/// The address half of an internet socket's local end, as `ss` writes it.
///
/// **Kept as text and not parsed into an address type.** The values `ss` prints are not
/// all addresses: `*` means every address of every family, and it is the single most
/// important value here because it is the difference between a service reachable from
/// the network and one reachable only from the box. `0.0.0.0` and `[::]` mean the same
/// thing for one family each. Parsing would have to invent a representation for `*`, and
/// normalising would erase the distinction between the three spellings, which is a real
/// difference in what a daemon asked the kernel for.
///
/// The brackets around an IPv6 address are removed, because they are `ss`'s punctuation
/// for separating the address from the port rather than part of the address. The
/// interface scope after a `%` is removed too and kept as its own field: it is a
/// different fact from the address.
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
        matches!(self.as_str(), "*" | "0.0.0.0" | "::")
    }
}

impl From<&InetHost> for Observation {
    fn from(host: &InetHost) -> Self {
        Observation::text(host.as_str())
    }
}
