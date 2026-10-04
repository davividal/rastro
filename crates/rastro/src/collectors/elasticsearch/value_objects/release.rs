//! Which Elasticsearch release a node runs.

use std::fmt;

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
}

impl fmt::Display for Release {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}
