//! The typed fields the walker's policy and its entries are made of.
//!
//! Each renders as a leaf. Nothing here knows that a filesystem is what gets walked,
//! which is what keeps the source replaceable.

mod content_policy;
mod device_number;
mod digest;
mod digest_algorithm;
mod file_kind;
mod file_mode;
mod nanoseconds_since_epoch;

pub use content_policy::ContentPolicy;
pub use device_number::DeviceNumber;
pub use digest::Digest;
pub use digest_algorithm::DigestAlgorithm;
pub use file_kind::FileKind;
pub use file_mode::FileMode;
pub use nanoseconds_since_epoch::NanosecondsSinceEpoch;
