//! Whether a path that would not be read is gone, or merely closed.

use std::io::ErrorKind;

/// Whether this is a path that is no longer there, as opposed to one that will not be read.
///
/// **The distinction decides between omitting an entry and recording a refusal**, and it has
/// to, because the two answers have opposite consequences for the byte-identical guarantee.
/// A path that vanished between the walk reaching its parent and reaching it did not exist at
/// one moment of this host, and it will not exist on the next run either, so recording it
/// would put an entry in one document and not the next for a reason that is not a change to
/// the box. Omitting it is also honest rather than silent: absence is state, and a path that
/// was not there when the walk arrived is reported by the same absence as one that never
/// existed, which is what actually happened.
///
/// A refusal is the opposite: `EACCES` on a directory or `EIO` on a bad sector reproduces at
/// the same path on every run, so it diffs cleanly and belongs in the document as the lasting
/// blind spot it is.
///
/// `StaleNetworkFileHandle` is here with `NotFound` because that is what an NFS server says
/// about a file deleted under it, and it means the same thing.
pub fn is_absence(kind: ErrorKind) -> bool {
    matches!(
        kind,
        ErrorKind::NotFound | ErrorKind::StaleNetworkFileHandle
    )
}
