//! Which log a destination is for.

use rastro_collector::Observation;

/// `access_log` or `error_log`, the two nginx writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogKind {
    Access,
    Error,
}

impl LogKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Access => "access_log",
            Self::Error => "error_log",
        }
    }

    pub fn of(directive: &str) -> Option<Self> {
        match directive {
            "access_log" => Some(Self::Access),
            "error_log" => Some(Self::Error),
            _ => None,
        }
    }
}

impl From<&LogKind> for Observation {
    fn from(kind: &LogKind) -> Self {
        Observation::text(kind.as_str())
    }
}
