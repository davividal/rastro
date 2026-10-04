//! How rastro reads a node of a given release.

use crate::collectors::elasticsearch::value_objects::SupportedRelease;

/// Whether a release is one rastro supports, one it reads by another's rules, or one it does
/// not read at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReleaseSupport {
    Supported(SupportedRelease),

    /// Read with the rules of the closest supported release, and said so.
    ReadAs(SupportedRelease),

    /// Older than 7, whose API differs too far to read by any supported release's rules.
    BelowSeven,
}
