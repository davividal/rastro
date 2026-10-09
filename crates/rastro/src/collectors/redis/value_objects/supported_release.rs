//! The releases rastro supports, and where a server's version falls against them.

use crate::collectors::redis::value_objects::ServerKind;

/// One supported line: a family, and the major and minor of its release.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SupportedRelease {
    pub kind: ServerKind,
    pub major: u32,
    pub minor: u32,
}

/// Every supported line.
///
/// **From endoflife.date**, rastro's source of truth for what to support, fetched 2026-10-06, with
/// one override by the maintainer: redis 8.0 to 8.10 only, where endoflife.date also lists 7.4, 7.2
/// and 6.2 as maintained. valkey: every line it lists as maintained. Each line was captured on a
/// real server in `docs/redis-matrix.md`.
pub const SUPPORTED_RELEASES: [SupportedRelease; 11] = [
    release(ServerKind::Redis, 8, 0),
    release(ServerKind::Redis, 8, 2),
    release(ServerKind::Redis, 8, 4),
    release(ServerKind::Redis, 8, 6),
    release(ServerKind::Redis, 8, 8),
    release(ServerKind::Redis, 8, 10),
    release(ServerKind::Valkey, 7, 2),
    release(ServerKind::Valkey, 8, 0),
    release(ServerKind::Valkey, 8, 1),
    release(ServerKind::Valkey, 9, 0),
    release(ServerKind::Valkey, 9, 1),
];

const fn release(kind: ServerKind, major: u32, minor: u32) -> SupportedRelease {
    SupportedRelease { kind, major, minor }
}

/// Whether a family's version is one of the supported lines.
///
/// A version that does not parse is not supported: whatever it is, it was not measured.
pub fn is_supported(kind: ServerKind, version: &str) -> bool {
    let mut parts = version.split('.').map(str::parse::<u32>);
    let (Some(Ok(major)), Some(Ok(minor))) = (parts.next(), parts.next()) else {
        return false;
    };

    SUPPORTED_RELEASES.contains(&release(kind, major, minor))
}
