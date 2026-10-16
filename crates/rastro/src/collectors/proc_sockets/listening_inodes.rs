//! Which sockets are being offered on one port, from the inet tables.
//!
//! **The address is not a criterion.** A service offered on a port is offered by whoever holds
//! that port whichever address it is bound to, and which interface that is gets answered
//! properly by the `sockets` facet. The table's pitfalls are dealt with once, in
//! [`inet_listeners`].

use std::collections::BTreeSet;
use std::path::Path;

use super::listeners::inet_listeners;

/// The inodes of the sockets being offered on `port`, or nothing where no table could be read.
///
/// The two answers stay apart for the reason [`inet_listeners`] gives: a readable table that
/// does not carry the port means nothing is offering it, and an unreadable one means nothing
/// is known.
pub fn listening_inodes(net: &Path, port: u16) -> Option<BTreeSet<u64>> {
    let listeners = inet_listeners(net)?;

    Some(
        listeners
            .into_iter()
            .filter(|listener| listener.address.port() == port)
            .map(|listener| listener.inode)
            .collect(),
    )
}
