//! Every socket the host is listening on.

use rastro_collector::Observation;

use super::listening_socket::ListeningSocket;

/// The listening sockets, sorted.
///
/// **A list, because a socket has no unique name to key on, and sorted because it has no
/// meaningful order either.** The obvious key would be the address, and it is very nearly
/// unique: two processes cannot normally bind the same one. `SO_REUSEPORT` exists
/// precisely so that they can, which is how a load-balanced daemon runs several accepting
/// processes, so keying would silently drop all but one of them.
///
/// Sorting is what stops the facet churning. `ss` walks the kernel's hash tables, and
/// their order depends on which sockets were opened when.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SocketTable(Vec<ListeningSocket>);

impl SocketTable {
    pub fn new(sockets: impl IntoIterator<Item = ListeningSocket>) -> Self {
        let mut sorted: Vec<ListeningSocket> = sockets.into_iter().collect();
        sorted.sort();

        Self(sorted)
    }

    pub fn sockets(&self) -> &[ListeningSocket] {
        &self.0
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl From<&SocketTable> for Observation {
    fn from(table: &SocketTable) -> Self {
        Observation::list(table.sockets().iter().map(Observation::from))
    }
}
