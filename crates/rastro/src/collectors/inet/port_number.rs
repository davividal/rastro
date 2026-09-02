//! Which port a socket is bound to.

use rastro_collector::{CollectionError, Observation};

/// A TCP or UDP port.
///
/// Held as a `u16` because that is the width of the field on the wire, and the width is
/// the check: a value outside the range means the field it came from was misread.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct PortNumber(u16);

impl PortNumber {
    pub fn parse(value: &str) -> Result<Self, CollectionError> {
        value
            .parse::<u16>()
            .map(Self)
            .map_err(|_| CollectionError::new(format!("{value:?} is not a port number")))
    }

    /// The same from the hexadecimal `/proc/net/*` writes.
    ///
    /// The kernel prints the port already in host order, unlike the address beside it, so
    /// this is a plain base-16 read and not a reinterpretation.
    pub fn parse_hexadecimal(value: &str) -> Result<Self, CollectionError> {
        u16::from_str_radix(value, 16)
            .map(Self)
            .map_err(|_| CollectionError::new(format!("{value:?} is not a port number")))
    }

    pub fn as_u16(&self) -> u16 {
        self.0
    }
}

impl From<&PortNumber> for Observation {
    fn from(port: &PortNumber) -> Self {
        Observation::integer(i64::from(port.as_u16()))
    }
}
