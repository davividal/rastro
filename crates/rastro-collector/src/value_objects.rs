//! The vocabulary collectors share.
//!
//! A value here earns its place by having consumers in more than one collector.
//! Living in the port rather than in the tool is deliberate: a collector written
//! outside this repo depends on this crate, and if it cannot reach these types it
//! will invent its own, leaving two facets in one document spelling the same
//! concept differently.
//!
//! Nothing here reads the host, which `tests/purity.rs` enforces.

mod absolute_path;
mod byte_size;
mod non_empty_text;
mod process_name;
mod setting_value;
mod walked_tree;

pub use absolute_path::AbsolutePath;
pub use byte_size::ByteSize;
pub use non_empty_text::NonEmptyText;
pub use process_name::ProcessName;
/// Re-exported rather than defined here: redaction spells the same digest, and that rule is
/// about observations, so the type moved down into the document crate. A collector's import
/// is unchanged.
pub use rastro_fingerprint::Xxh3Digest;
pub use setting_value::SettingValue;
pub use walked_tree::WalkedTree;
