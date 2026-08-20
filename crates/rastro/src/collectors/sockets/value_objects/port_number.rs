//! Which port a socket is bound to.

use rastro_collector::{CollectionError, Observation};

/// A TCP or UDP port.
///
/// Held as a `u16` because that is the width of the field on the wire, and the width is
/// the check: `ss` prints the local address and the port separated by the same character
/// that appears six times inside an IPv6 address, so a number outside the range is the
/// signal that the split found the wrong colon.
///
/// `*` is a legal port in `ss` output, meaning any, and it is not this type's business:
/// it only ever appears in the peer column of a listening socket, which this collector
/// does not record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct PortNumber(u16);

impl PortNumber {
    pub fn parse(value: &str) -> Result<Self, CollectionError> {
        value
            .parse::<u16>()
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
