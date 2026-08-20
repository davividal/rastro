//! How much of an address is the network.

use rastro_collector::{CollectionError, Observation};

/// A prefix length in bits.
///
/// Held as a `u8` and checked against the widest family, because the width is the check:
/// 32 is the whole of an IPv4 address and 128 the whole of an IPv6 one, so anything above
/// 128 means the address and its prefix were read out of the wrong fields.
///
/// Deliberately *not* checked against the address's own family. That would need this type
/// to know which family it belongs to, and `/64` is legal for IPv6 and nonsense for IPv4
/// while both are legal numbers. Rejecting the combination is the kernel's job; rastro
/// records what the kernel said.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct PrefixLength(u8);

/// The widest prefix any family has, which is an IPv6 address in full.
const WIDEST: u8 = 128;

impl PrefixLength {
    pub fn new(value: u8) -> Result<Self, CollectionError> {
        if value > WIDEST {
            return Err(CollectionError::new(format!(
                "{value} is wider than the widest address family's {WIDEST} bits"
            )));
        }

        Ok(Self(value))
    }

    pub fn as_u8(&self) -> u8 {
        self.0
    }
}

impl From<&PrefixLength> for Observation {
    fn from(prefix: &PrefixLength) -> Self {
        Observation::integer(i64::from(prefix.as_u8()))
    }
}
