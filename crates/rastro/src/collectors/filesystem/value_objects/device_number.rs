//! Which device a device node addresses.

use rastro_collector::Observation;

/// The major and minor numbers of a block or character device node.
///
/// **A device node's content is its numbers.** There is nothing to hash and nothing to
/// read: the major says which driver, the minor says which of that driver's devices, and
/// `/dev/sda` becoming `8:16` instead of `8:0` is a different disk under the same path.
/// Recording only the kind would let two inventories pointing at different devices compare
/// equal.
///
/// The pair is carried as one value rather than two independent fields because neither
/// half means anything alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct DeviceNumber {
    major: i64,
    minor: i64,
}

impl DeviceNumber {
    /// Splits the kernel's packed `st_rdev`.
    ///
    /// Linux scatters the two numbers through a 64-bit word rather than keeping each in one
    /// run of bits: the major is bits 8-19 and 32-63, the minor is bits 0-7 and 20-31. This
    /// is the split `major(3)` and `minor(3)` perform, and doing it here is what stops a
    /// reader having to know the encoding to read the document.
    pub fn of(raw_device: u64) -> Self {
        let major = ((raw_device >> 8) & 0x0000_0fff) | ((raw_device >> 32) & 0xffff_f000);
        let minor = (raw_device & 0x0000_00ff) | ((raw_device >> 12) & 0xffff_ff00);

        // Both masks leave at most 32 bits set, so neither can reach an i64's sign bit.
        Self {
            major: major as i64,
            minor: minor as i64,
        }
    }

    pub fn major(&self) -> i64 {
        self.major
    }

    pub fn minor(&self) -> i64 {
        self.minor
    }
}

impl From<&DeviceNumber> for Observation {
    fn from(device: &DeviceNumber) -> Self {
        Observation::object([
            ("major", Observation::integer(device.major())),
            ("minor", Observation::integer(device.minor())),
        ])
    }
}
