//! The typed fields the walker's policy and its entries are made of.
//!
//! Each renders as a leaf. Nothing here knows that a filesystem is what gets walked,
//! which is what keeps the source replaceable.

mod canonical_bytes;
mod content_policy;
mod detail;
mod device_number;
mod digest;
mod digest_algorithm;
mod file_kind;
mod nanoseconds_since_epoch;

pub use canonical_bytes::CanonicalBytes;
pub use content_policy::ContentPolicy;
pub use detail::Detail;
pub use device_number::DeviceNumber;
pub use digest::Digest;
pub use digest_algorithm::DigestAlgorithm;
pub use file_kind::FileKind;
pub use nanoseconds_since_epoch::NanosecondsSinceEpoch;
