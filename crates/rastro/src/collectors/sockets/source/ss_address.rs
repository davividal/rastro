//! The local-address column of `ss`'s internet rows.
//!
//! `0.0.0.0:22`, `*:9100`, `127.0.0.53%lo:53`, `[::]:5355`,
//! `[fe80::a00:27ff:fea0:9cdd]%enp0s9:546` — every one of those is on the development
//! box, and they are the reason this is a module rather than a `split(':')`.

use rastro_collector::CollectionError;

use crate::collectors::sockets::model::SocketAddress;
use crate::collectors::sockets::value_objects::{InetHost, InterfaceScope, PortNumber};

/// What separates an address from its interface scope.
const SCOPE_MARKER: char = '%';

/// Splits an internet socket's local end into its three parts.
///
/// **Split on the *last* colon, and that is the whole difficulty.** An IPv6 address
/// contains up to seven of them, so splitting on the first, or on all of them, mis-slots
/// every IPv6 row. The port is the only field after the final colon, and `ss` brackets an
/// IPv6 address precisely so that this split is unambiguous.
///
/// The brackets then come off, because they are `ss`'s punctuation and not part of the
/// address, and a `%scope` suffix is separated out because it is a different fact from the
/// address it qualifies.
pub fn parse(column: &str) -> Result<SocketAddress, CollectionError> {
    let (host, port) = column.rsplit_once(':').ok_or_else(|| {
        CollectionError::new(format!(
            "{column:?} is not an address and port, so the row was misread"
        ))
    })?;

    let (host, scope) = match host.split_once(SCOPE_MARKER) {
        Some((host, scope)) => (host, Some(InterfaceScope::new(scope)?)),
        None => (host, None),
    };

    Ok(SocketAddress::Inet {
        host: InetHost::new(unbracketed(host))?,
        port: PortNumber::parse(port)?,
        scope,
    })
}

/// An IPv6 address without the brackets `ss` wraps it in.
///
/// Both are stripped or neither, so a stray bracket in one position is left alone to fail
/// the address check rather than being quietly half-repaired.
fn unbracketed(host: &str) -> &str {
    host.strip_prefix('[')
        .and_then(|rest| rest.strip_suffix(']'))
        .unwrap_or(host)
}
