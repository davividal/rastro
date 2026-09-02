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
mod xxh3_digest;

pub use absolute_path::AbsolutePath;
pub use byte_size::ByteSize;
pub use non_empty_text::NonEmptyText;
pub use process_name::ProcessName;
pub use setting_value::SettingValue;
pub use walked_tree::WalkedTree;
pub use xxh3_digest::Xxh3Digest;
