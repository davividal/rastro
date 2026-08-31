//! What rastro means by a walk, as opposed to how a host is read.

mod file_entry;
mod filesystem_inventory;
mod policy_rule;
mod unreadable_path;
mod unspellable_path;
mod walk_failure;
mod walk_policy;

pub use file_entry::FileEntry;
pub use filesystem_inventory::FilesystemInventory;
pub use policy_rule::PolicyRule;
pub use unreadable_path::UnreadablePath;
pub use unspellable_path::UnspellablePath;
pub use walk_failure::{Refusal, is_absence};
pub use walk_policy::WalkPolicy;
