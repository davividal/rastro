//! Whether an access rule lets a client in or turns it away.

use rastro_collector::Observation;

/// `allow` or `deny`, the two halves of nginx's access module.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Permission {
    Allow,
    Deny,
}

impl Permission {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Allow => "allow",
            Self::Deny => "deny",
        }
    }

    pub fn of(directive: &str) -> Option<Self> {
        match directive {
            "allow" => Some(Self::Allow),
            "deny" => Some(Self::Deny),
            _ => None,
        }
    }
}

impl From<&Permission> for Observation {
    fn from(permission: &Permission) -> Self {
        Observation::text(permission.as_str())
    }
}
