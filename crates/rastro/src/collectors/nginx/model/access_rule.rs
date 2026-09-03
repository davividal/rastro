//! One rule about who may reach something.

use rastro_collector::Observation;

use crate::collectors::nginx::value_objects::{AddressPattern, Permission};

/// An `allow` or a `deny`, and who it is about.
///
/// **Order is kept**, and this is the one list in the facet where that is not a stylistic
/// choice: nginx applies these in the order they are written and stops at the first match,
/// so `allow 10.0.0.0/8; deny all;` and `deny all; allow 10.0.0.0/8;` are opposite
/// configurations built from identical rules.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccessRule {
    pub permission: Permission,
    pub subject: AddressPattern,
}

impl From<&AccessRule> for Observation {
    fn from(rule: &AccessRule) -> Self {
        Observation::object([
            ("permission", Observation::from(&rule.permission)),
            ("subject", Observation::from(&rule.subject)),
        ])
    }
}
