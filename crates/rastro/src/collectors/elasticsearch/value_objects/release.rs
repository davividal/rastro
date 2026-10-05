//! Which Elasticsearch release a node runs.

use std::fmt;

use crate::collectors::elasticsearch::value_objects::{ReleaseSupport, SupportedRelease};

/// A release as its server jar names it, `major.minor.patch`.
///
/// Read before any request, since it decides which release's rules a node is read with and
/// whether it is asked at all. Ordered, so a node can be placed against the supported releases.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Release {
    major: u32,
    minor: u32,
    patch: u32,
}

impl Release {
    /// The version in `7.17.29`, and nothing for anything else: a pre-release suffix, a missing
    /// component or a fourth one is not how a released server jar is named.
    pub fn parse(text: &str) -> Option<Self> {
        let mut components = text.split('.').map(|component| match component {
            digits if !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit()) => {
                digits.parse::<u32>().ok()
            }
            _ => None,
        });
        let version = Self {
            major: components.next()??,
            minor: components.next()??,
            patch: components.next()??,
        };

        components.next().is_none().then_some(version)
    }

    pub fn major(&self) -> u32 {
        self.major
    }

    /// How rastro reads a node of this release: the closest supported line by its own major, the
    /// newest one for a major after 9, as the postgresql collector reads a newer major.
    pub fn support(&self) -> ReleaseSupport {
        let closest = match (self.major, self.minor) {
            (..7, _) => return ReleaseSupport::BelowSeven,
            (7, _) => SupportedRelease::V7_17,
            (8, _) => SupportedRelease::V8_19,
            (9, ..=4) => SupportedRelease::V9_4,
            _ => SupportedRelease::V9_5,
        };

        match closest.line() == (self.major, self.minor) {
            true => ReleaseSupport::Supported(closest),
            false => ReleaseSupport::ReadAs(closest),
        }
    }
}

impl fmt::Display for Release {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}
