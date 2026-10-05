//! The releases rastro reads Elasticsearch by.

use std::fmt;

/// A release line rastro supports: the closed set its reads are measured against.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SupportedRelease {
    V7_17,
    V8_19,
    V9_4,
    V9_5,
}

impl SupportedRelease {
    /// `major.minor`, the line a node's release belongs to.
    pub fn line(&self) -> (u32, u32) {
        match self {
            Self::V7_17 => (7, 17),
            Self::V8_19 => (8, 19),
            Self::V9_4 => (9, 4),
            Self::V9_5 => (9, 5),
        }
    }
}

impl fmt::Display for SupportedRelease {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (major, minor) = self.line();
        write!(formatter, "{major}.{minor}")
    }
}
