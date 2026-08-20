//! A restriction placed on a key.

use rastro_collector::{CollectionError, NonEmptyText, Observation};

/// One option from the front of an `authorized_keys` line.
///
/// `restrict`, `no-port-forwarding`, `from="10.0.0.0/8"`, `command="/usr/bin/rrsync ~/backup"`,
/// `expiry-time="20261231"`.
///
/// **These are as important as the key itself and are the field most often overlooked.** A key
/// with `command=` can only run that one command however its owner invokes ssh; the same key
/// with the option removed is a full shell. `from=` is the difference between a key usable
/// from one subnet and one usable from anywhere. So an option disappearing is a privilege
/// escalation that leaves the key body untouched.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct KeyOption(NonEmptyText);

impl KeyOption {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(NonEmptyText::new(value, "key option")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<&KeyOption> for Observation {
    fn from(option: &KeyOption) -> Self {
        Observation::text(option.as_str())
    }
}
