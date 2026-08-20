//! Where a unix socket lives.

use rastro_collector::{CollectionError, NonEmptyText, Observation};

/// A unix socket's address.
///
/// **Not an [`AbsolutePath`](rastro_collector::AbsolutePath), and the exception is the
/// point.** Most are ordinary paths such as `/run/systemd/private`, but an *abstract*
/// socket has a name in a namespace that is not the filesystem at all, and `ss` prints it
/// with a leading `@`: the development box has
/// `@/var/spool/exim4/exim_daemon_notify`. An abstract socket is invisible to a
/// filesystem walk, which makes this the only place it appears in a fingerprint, so
/// refusing it for not starting with `/` would drop exactly the sockets nothing else
/// records.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct SocketPath(NonEmptyText);

/// What `ss` prefixes an abstract socket's name with.
const ABSTRACT_MARKER: char = '@';

impl SocketPath {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(NonEmptyText::new(value, "socket path")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    /// Whether this socket exists outside the filesystem.
    pub fn is_abstract(&self) -> bool {
        self.as_str().starts_with(ABSTRACT_MARKER)
    }
}

impl From<&SocketPath> for Observation {
    fn from(path: &SocketPath) -> Self {
        Observation::text(path.as_str())
    }
}
